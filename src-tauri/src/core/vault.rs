//! Vault lifecycle: create, unlock, lock, change password, auto-lock.
//!
//! Keys live only in memory inside a `Mutex`, and are zeroized when dropped. The master password
//! is never stored: the file holds a random salt, the KDF parameters and a verifier.
//! On startup only `vault_meta` is read. The full database is migrated and used after unlock.

use crate::core::crypto::{derive_keys, random_bytes, KdfParams, VaultKeys, SALT_LEN};
use crate::core::settings::{self, Settings};
use crate::db::{migrations, models::VaultStatus, repo};
use crate::error::{LokiError, Result};
use rusqlite::{params, Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const MIN_PASSWORD_LEN: usize = 8;

struct Inner {
    keys: Option<VaultKeys>,
    conn: Option<Connection>,
    settings: Settings,
    last_activity: Instant,
    /// A decrypted `.lokishare` waiting for the user to import or save it. Dropped on lock.
    pending_share: Option<crate::core::share::PendingShare>,
}

pub struct VaultState {
    data_dir: PathBuf,
    inner: Mutex<Inner>,
}

fn check_new_password(p: &str) -> Result<()> {
    if p.chars().count() < MIN_PASSWORD_LEN {
        return Err(LokiError::Invalid(format!("Use at least {MIN_PASSWORD_LEN} characters for the master password.")));
    }
    Ok(())
}

impl VaultState {
    pub fn new(data_dir: PathBuf) -> Self {
        VaultState {
            data_dir,
            inner: Mutex::new(Inner { keys: None, conn: None, settings: Settings::default(), last_activity: Instant::now(), pending_share: None }),
        }
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("loki.db")
    }

    fn guard(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock must not brick the vault for the rest of the session.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_unlocked(&self) -> bool {
        self.guard().keys.is_some()
    }

    /// Cheap check used at startup: reads `vault_meta` only.
    pub fn status(&self) -> Result<VaultStatus> {
        if self.is_unlocked() {
            return Ok(VaultStatus { exists: true, unlocked: true });
        }
        let path = self.db_path();
        if !path.exists() {
            return Ok(VaultStatus { exists: false, unlocked: false });
        }
        let exists = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|c| repo::read_meta(&c).ok().flatten())
            .is_some();
        Ok(VaultStatus { exists, unlocked: false })
    }

    pub fn create(&self, password: &str) -> Result<()> {
        self.create_with(password, KdfParams::DEFAULT)
    }

    pub fn create_with(&self, password: &str, kdf: KdfParams) -> Result<()> {
        check_new_password(password)?;
        if self.status()?.exists {
            return Err(LokiError::VaultExists);
        }
        std::fs::create_dir_all(&self.data_dir)?;
        let salt = random_bytes::<SALT_LEN>()?;
        let keys = derive_keys(password.as_bytes(), &salt, kdf)?;
        let verifier = keys.make_verifier()?;

        let path = self.db_path();
        // A leftover file without a vault_meta row is an abandoned attempt: start clean.
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let mut conn = migrations::open(&path)?;
        migrations::migrate(&mut conn)?;
        let tx = conn.transaction()?;
        repo::insert_meta(&tx, &salt, kdf, &verifier.nonce, &verifier.ct)?;
        repo::audit(&tx, "vault_create", "vault", None)?;
        tx.commit()?;

        let mut g = self.guard();
        g.settings = settings::load(&conn);
        g.conn = Some(conn);
        g.keys = Some(keys);
        g.last_activity = Instant::now();
        Ok(())
    }

    /// Unlocks the vault. If the schema had to be upgraded, returns where the pre-upgrade copy
    /// of the database was saved.
    pub fn unlock(&self, password: &str) -> Result<Option<PathBuf>> {
        if self.is_unlocked() {
            return Ok(None);
        }
        let path = self.db_path();
        if !path.exists() {
            return Err(LokiError::NoVault);
        }
        let mut conn = migrations::open(&path)?;
        let meta = repo::read_meta(&conn).map_err(|_| LokiError::NoVault)?.ok_or(LokiError::NoVault)?;
        let keys = derive_keys(password.as_bytes(), &meta.salt, meta.params)?;
        if !keys.check_verifier(&meta.verifier_nonce, &meta.verifier_ct) {
            // Generic error and a 1 s delay, so guessing is slow and tells the caller nothing.
            std::thread::sleep(Duration::from_secs(1));
            return Err(LokiError::WrongPassword);
        }
        // Upgrading an existing vault writes a full copy to `backups/` first.
        let backup = migrations::migrate_with_backup(&mut conn, Some(&self.data_dir.join("backups")))?;
        let loaded = settings::load(&conn);
        repo::audit(&conn, "unlock", "vault", None)?;
        if let Some(b) = &backup {
            let name = b.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            repo::audit(&conn, "schema_upgrade", "vault", Some(&format!("backup saved as backups/{name}")))?;
        }
        let mut g = self.guard();
        g.settings = loaded;
        g.conn = Some(conn);
        g.keys = Some(keys);
        g.last_activity = Instant::now();
        Ok(backup)
    }

    /// Drops the keys (zeroized) and the connection. Returns true if it was unlocked.
    pub fn lock(&self) -> bool {
        let mut g = self.guard();
        let was = g.keys.is_some();
        if let Some(conn) = g.conn.as_ref() {
            let _ = repo::audit(conn, "lock", "vault", None);
        }
        g.keys = None;
        g.conn = None;
        g.settings = Settings::default();
        g.pending_share = None;
        was
    }

    /// Runs `f` with the open database, the keys and the settings. Counts as activity.
    pub fn with_unlocked<R>(&self, f: impl FnOnce(&mut Connection, &VaultKeys, &Settings) -> Result<R>) -> Result<R> {
        let mut g = self.guard();
        let inner = &mut *g;
        let (Some(conn), Some(keys)) = (inner.conn.as_mut(), inner.keys.as_ref()) else {
            return Err(LokiError::Locked);
        };
        inner.last_activity = Instant::now();
        f(conn, keys, &inner.settings)
    }

    pub fn touch(&self) {
        self.guard().last_activity = Instant::now();
    }

    pub fn settings(&self) -> Result<Settings> {
        let g = self.guard();
        if g.keys.is_none() {
            return Err(LokiError::Locked);
        }
        Ok(g.settings.clone())
    }

    pub fn update_settings(&self, new: Settings) -> Result<Settings> {
        let new = new.validated()?;
        let mut g = self.guard();
        let inner = &mut *g;
        let (Some(conn), Some(_)) = (inner.conn.as_ref(), inner.keys.as_ref()) else {
            return Err(LokiError::Locked);
        };
        settings::save(conn, &new)?;
        inner.settings = new.clone();
        inner.last_activity = Instant::now();
        Ok(new)
    }

    /// Locks when idle longer than the auto-lock setting. Returns true if it just locked.
    pub fn lock_if_idle(&self) -> bool {
        let idle = {
            let g = self.guard();
            g.keys.is_some() && g.last_activity.elapsed() >= Duration::from_secs(u64::from(g.settings.auto_lock_minutes) * 60)
        };
        idle && self.lock()
    }

    pub fn set_pending_share(&self, share: crate::core::share::PendingShare) -> Result<()> {
        let mut g = self.guard();
        if g.keys.is_none() {
            return Err(LokiError::Locked);
        }
        g.pending_share = Some(share);
        Ok(())
    }

    /// Runs `f` with the open database, keys and the pending share (if its token matches).
    pub fn with_pending_share<R>(
        &self,
        token: &str,
        f: impl FnOnce(&mut Connection, &VaultKeys, &crate::core::share::PendingShare) -> Result<R>,
    ) -> Result<R> {
        let mut g = self.guard();
        let inner = &mut *g;
        let (Some(conn), Some(keys)) = (inner.conn.as_mut(), inner.keys.as_ref()) else {
            return Err(LokiError::Locked);
        };
        let share = inner
            .pending_share
            .as_ref()
            .filter(|s| s.token == token)
            .ok_or_else(|| LokiError::Invalid("That share is no longer open. Open the file again.".into()))?;
        inner.last_activity = Instant::now();
        f(conn, keys, share)
    }

    pub fn discard_pending_share(&self) {
        self.guard().pending_share = None;
    }

    pub fn lock_on_minimize_enabled(&self) -> bool {
        let g = self.guard();
        g.keys.is_some() && g.settings.lock_on_minimize
    }

    pub fn change_password(&self, old: &str, new: &str) -> Result<()> {
        self.change_password_with(old, new, KdfParams::DEFAULT)
    }

    /// Re-encrypts every row with keys derived from the new password, inside ONE transaction.
    pub fn change_password_with(&self, old: &str, new: &str, kdf: KdfParams) -> Result<()> {
        check_new_password(new)?;
        let mut g = self.guard();
        let inner = &mut *g;
        let (Some(conn), Some(keys)) = (inner.conn.as_mut(), inner.keys.as_ref()) else {
            return Err(LokiError::Locked);
        };
        let meta = repo::read_meta(conn)?.ok_or(LokiError::NoVault)?;
        let old_keys = derive_keys(old.as_bytes(), &meta.salt, meta.params)?;
        if !old_keys.check_verifier(&meta.verifier_nonce, &meta.verifier_ct) {
            std::thread::sleep(Duration::from_secs(1));
            return Err(LokiError::WrongPassword);
        }
        let salt = random_bytes::<SALT_LEN>()?;
        let new_keys = derive_keys(new.as_bytes(), &salt, kdf)?;
        let verifier = new_keys.make_verifier()?;

        let tx = conn.transaction()?;
        let rows = repo::reencrypt_all(&tx, keys, &new_keys)? + crate::core::credentials::reencrypt_all(&tx, keys, &new_keys)?;
        tx.execute(
            "UPDATE vault_meta SET kdf_salt = ?1, kdf_m_cost = ?2, kdf_t_cost = ?3, kdf_p_cost = ?4, verifier_nonce = ?5, verifier_ct = ?6 WHERE id = 1",
            params![salt.as_slice(), kdf.m_cost, kdf.t_cost, kdf.p_cost, verifier.nonce.as_slice(), verifier.ct],
        )?;
        repo::audit(&tx, "change_password", "vault", Some(&format!("{rows} values re-encrypted")))?;
        tx.commit()?;
        inner.keys = Some(new_keys);
        inner.last_activity = Instant::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::TEST_PARAMS;
    use crate::core::parser::parse;

    fn temp_state(tag: &str) -> VaultState {
        let mut p = std::env::temp_dir();
        p.push(format!("loki-test-{tag}-{}-{}", std::process::id(), crate::core::crypto::to_hex(&random_bytes::<4>().unwrap())));
        VaultState::new(p)
    }

    fn cleanup(s: &VaultState) {
        s.lock();
        let _ = std::fs::remove_dir_all(s.data_dir());
    }

    #[test]
    fn create_lock_unlock_cycle() {
        let s = temp_state("cycle");
        assert!(!(s.status().unwrap().exists));
        assert!(matches!(s.with_unlocked(|_, _, _| Ok(())), Err(LokiError::Locked)));
        s.create_with("fake_password_1", TEST_PARAMS).unwrap();
        assert!(s.status().unwrap().exists);
        assert!(s.is_unlocked());
        assert!(matches!(s.create_with("fake_password_1", TEST_PARAMS), Err(LokiError::VaultExists)));

        assert!(s.lock());
        assert!(!s.is_unlocked());
        assert!(matches!(s.with_unlocked(|_, _, _| Ok(())), Err(LokiError::Locked)));
        assert_eq!(s.status().unwrap(), VaultStatus { exists: true, unlocked: false });

        s.unlock("fake_password_1").unwrap();
        assert!(s.is_unlocked());
        cleanup(&s);
    }

    #[test]
    fn wrong_password_is_generic_and_delayed() {
        let s = temp_state("wrong");
        s.create_with("fake_password_1", TEST_PARAMS).unwrap();
        s.lock();
        let t = Instant::now();
        let err = s.unlock("fake_password_2").unwrap_err();
        assert!(t.elapsed() >= Duration::from_millis(900), "no delay");
        assert!(matches!(err, LokiError::WrongPassword));
        assert!(!err.to_string().to_lowercase().contains("fake_password"));
        assert!(!s.is_unlocked());
        cleanup(&s);
    }

    #[test]
    fn short_password_is_rejected() {
        let s = temp_state("short");
        assert!(s.create_with("short", TEST_PARAMS).is_err());
        assert!(!(s.status().unwrap().exists));
        cleanup(&s);
    }

    #[test]
    fn password_is_never_stored_in_the_file() {
        let s = temp_state("nopw");
        s.create_with("fake_password_unique_marker", TEST_PARAMS).unwrap();
        s.lock();
        let bytes = std::fs::read(s.db_path()).unwrap();
        assert!(!bytes.windows(27).any(|w| w == b"fake_password_unique_marker"));
        cleanup(&s);
    }

    #[test]
    fn change_password_re_encrypts_everything() {
        let s = temp_state("chpw");
        s.create_with("fake_password_1", TEST_PARAMS).unwrap();
        let (tid, sid) = s
            .with_unlocked(|conn, keys, _| {
                let p = repo::create_project(conn, "P", None, None)?;
                let t = repo::create_target(conn, p.id, "main", "production", None, None)?;
                let snap = repo::create_snapshot(conn, keys, t.id, &parse("# note\nA=fake_value_1\nB=fake_value_2\n").entries, None, None, None, "import")?;
                Ok((t.id, snap.id))
            })
            .unwrap();
        let fp_before = s.with_unlocked(|conn, _, _| Ok(repo::load_meta(conn, sid)?[0].fp.clone())).unwrap();

        assert!(matches!(s.change_password_with("fake_password_WRONG", "fake_password_2", TEST_PARAMS), Err(LokiError::WrongPassword)));
        assert!(s.change_password_with("fake_password_1", "short", TEST_PARAMS).is_err());
        s.change_password_with("fake_password_1", "fake_password_2", TEST_PARAMS).unwrap();

        // Still readable in this session, with new fingerprints.
        let (vals, fp_after) = s
            .with_unlocked(|conn, keys, _| Ok((repo::load_entries(conn, keys, sid)?, repo::load_meta(conn, sid)?[0].fp.clone())))
            .unwrap();
        assert_eq!(vals[0].value, "fake_value_1");
        assert_eq!(vals[0].comment.as_deref(), Some("note"));
        assert_ne!(fp_before, fp_after);
        let _ = tid;

        // Old password is dead, new one works after a lock.
        s.lock();
        assert!(matches!(s.unlock("fake_password_1"), Err(LokiError::WrongPassword)));
        s.unlock("fake_password_2").unwrap();
        let again = s.with_unlocked(|conn, keys, _| repo::load_entries(conn, keys, sid)).unwrap();
        assert_eq!(again[1].value, "fake_value_2");
        cleanup(&s);
    }

    #[test]
    fn settings_persist_across_lock() {
        let s = temp_state("settings");
        s.create_with("fake_password_1", TEST_PARAMS).unwrap();
        let new = Settings { auto_lock_minutes: 9, reveal_seconds: 20, ..Default::default() };
        s.update_settings(new).unwrap();
        s.lock();
        assert!(s.settings().is_err());
        s.unlock("fake_password_1").unwrap();
        let got = s.settings().unwrap();
        assert_eq!((got.auto_lock_minutes, got.reveal_seconds), (9, 20));
        cleanup(&s);
    }

    #[test]
    fn idle_lock() {
        let s = temp_state("idle");
        s.create_with("fake_password_1", TEST_PARAMS).unwrap();
        assert!(!s.lock_if_idle()); // just used
        // Pretend the last activity was long ago.
        s.guard().last_activity = Instant::now().checked_sub(Duration::from_secs(60 * 6)).unwrap();
        assert!(s.lock_if_idle());
        assert!(!s.is_unlocked());
        assert!(!s.lock_if_idle()); // nothing left to lock
        cleanup(&s);
    }

    #[test]
    fn touch_resets_idle_timer() {
        let s = temp_state("touch");
        s.create_with("fake_password_1", TEST_PARAMS).unwrap();
        s.guard().last_activity = Instant::now().checked_sub(Duration::from_secs(60 * 6)).unwrap();
        s.touch();
        assert!(!s.lock_if_idle());
        cleanup(&s);
    }

    #[test]
    fn garbage_db_file_is_treated_as_no_vault() {
        let s = temp_state("garbage");
        std::fs::create_dir_all(s.data_dir()).unwrap();
        std::fs::write(s.db_path(), b"this is not a sqlite database at all, just text").unwrap();
        assert!(!(s.status().unwrap().exists));
        assert!(s.unlock("fake_password_1").is_err());
        cleanup(&s);
    }
}
