//! Encrypted `.loki` backup: the whole inventory under its OWN password.
//!
//! File layout: magic `LOKIBAK1` | m_cost u32 | t_cost u32 | p_cost u32 | salt (16) | nonce (24) | ciphertext.
//! The header (everything before the nonce) is the AEAD associated data, so it cannot be altered
//! without detection. Key derivation is Argon2id and encryption is XChaCha20-Poly1305.

use crate::core::crypto::{derive_keys, random_bytes, KdfParams, SALT_LEN, NONCE_LEN};
use crate::core::crypto::VaultKeys;
use crate::core::parser::is_valid_key;
use crate::db::repo;
use crate::error::{LokiError, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zeroize::{Zeroize, Zeroizing};

const MAGIC: &[u8; 8] = b"LOKIBAK1";
const HEADER_LEN: usize = 8 + 12 + SALT_LEN;
const MAX_M_COST: u32 = 1024 * 1024; // 1 GiB, protects against a crafted file asking for absurd memory
const MAX_T_COST: u32 = 20;

pub fn encrypt(data: &[u8], password: &str, kdf: KdfParams) -> Result<Vec<u8>> {
    encrypt_with(MAGIC, data, password, kdf)
}

pub fn decrypt(file: &[u8], password: &str) -> Result<Zeroizing<Vec<u8>>> {
    decrypt_with(MAGIC, file, password, LokiError::WrongPassword, "That is not a LOKI backup file.")
}

/// Shared by `.loki` backups and `.lokishare` files; only the 8-byte magic differs.
pub fn encrypt_with(magic: &[u8; 8], data: &[u8], password: &str, kdf: KdfParams) -> Result<Vec<u8>> {
    let salt = random_bytes::<SALT_LEN>()?;
    let keys = derive_keys(password.as_bytes(), &salt, kdf)?;
    let mut out = Vec::with_capacity(HEADER_LEN + NONCE_LEN + data.len() + 16);
    out.extend_from_slice(magic);
    out.extend_from_slice(&kdf.m_cost.to_le_bytes());
    out.extend_from_slice(&kdf.t_cost.to_le_bytes());
    out.extend_from_slice(&kdf.p_cost.to_le_bytes());
    out.extend_from_slice(&salt);
    let sealed = keys.seal(&out[..HEADER_LEN], data)?;
    out.extend_from_slice(&sealed.nonce);
    out.extend_from_slice(&sealed.ct);
    Ok(out)
}

pub fn decrypt_with(magic: &[u8; 8], file: &[u8], password: &str, wrong: LokiError, not_ours: &str) -> Result<Zeroizing<Vec<u8>>> {
    if file.len() < HEADER_LEN + NONCE_LEN + 16 || &file[..8] != magic {
        return Err(LokiError::Invalid(not_ours.into()));
    }
    let u32_at = |i: usize| u32::from_le_bytes([file[i], file[i + 1], file[i + 2], file[i + 3]]);
    let kdf = KdfParams { m_cost: u32_at(8), t_cost: u32_at(12), p_cost: u32_at(16) };
    if kdf.m_cost > MAX_M_COST || kdf.t_cost > MAX_T_COST || kdf.p_cost == 0 || kdf.p_cost > 16 {
        return Err(LokiError::Invalid("This file is damaged.".into()));
    }
    let salt = &file[20..20 + SALT_LEN];
    let keys = derive_keys(password.as_bytes(), salt, kdf)?;
    match keys.open(&file[..HEADER_LEN], &file[HEADER_LEN..HEADER_LEN + NONCE_LEN], &file[HEADER_LEN + NONCE_LEN..]) {
        Ok(pt) => Ok(pt),
        Err(_) => {
            std::thread::sleep(std::time::Duration::from_secs(1));
            Err(wrong)
        }
    }
}

// ---------------------------------------------------------------- data model

#[derive(Serialize, Deserialize)]
struct BackupData {
    format: u32,
    projects: Vec<BProject>,
    /// Added with the password store. Older backups have none.
    #[serde(default)]
    credentials: Vec<crate::core::credentials::BackupCredential>,
}

#[derive(Serialize, Deserialize)]
struct BProject {
    name: String,
    root_path: Option<String>,
    description: Option<String>,
    targets: Vec<BTarget>,
    key_notes: Vec<BNote>,
}

#[derive(Serialize, Deserialize)]
struct BTarget {
    branch: String,
    stage: String,
    label: Option<String>,
    color: Option<String>,
    is_archived: bool,
    snapshots: Vec<BSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct BSnapshot {
    version: i64,
    note: Option<String>,
    source_path: Option<String>,
    created_at: String,
    variables: Vec<BVar>,
}

#[derive(Serialize, Deserialize)]
struct BVar {
    key: String,
    value: String,
    comment: Option<String>,
}

impl Drop for BVar {
    fn drop(&mut self) {
        self.value.zeroize();
        if let Some(c) = self.comment.as_mut() {
            c.zeroize();
        }
    }
}

#[derive(Serialize, Deserialize)]
struct BNote {
    key: String,
    description: Option<String>,
    required_stages: Vec<String>,
    is_secret: bool,
    last_rotated: Option<String>,
}

pub fn export_json(conn: &Connection, keys: &VaultKeys) -> Result<Zeroizing<Vec<u8>>> {
    let mut projects = Vec::new();
    for p in repo::list_projects(conn)? {
        let mut targets = Vec::new();
        for t in repo::list_targets(conn, p.id)? {
            let mut snapshots = Vec::new();
            let mut snaps = repo::list_snapshots(conn, t.id)?;
            snaps.reverse(); // oldest first
            for s in snaps {
                let variables = repo::load_entries(conn, keys, s.id)?
                    .iter()
                    .map(|e| BVar { key: e.key.clone(), value: e.value.clone(), comment: e.comment.clone() })
                    .collect();
                snapshots.push(BSnapshot { version: s.version, note: s.note, source_path: s.source_path, created_at: s.created_at, variables });
            }
            targets.push(BTarget { branch: t.branch, stage: t.stage, label: t.label, color: t.color, is_archived: t.is_archived, snapshots });
        }
        let key_notes = repo::list_key_notes(conn, p.id)?
            .into_iter()
            .map(|n| BNote { key: n.key, description: n.description, required_stages: n.required_stages, is_secret: n.is_secret, last_rotated: n.last_rotated })
            .collect();
        projects.push(BProject { name: p.name, root_path: p.root_path, description: p.description, targets, key_notes });
    }
    let credentials = crate::core::credentials::export_all(conn, keys)?;
    let json = serde_json::to_vec(&BackupData { format: 1, projects, credentials }).map_err(|_| LokiError::Crypto)?;
    Ok(Zeroizing::new(json))
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub projects: usize,
    pub targets: usize,
    pub snapshots: usize,
    /// Projects whose name already existed and were added under a new name.
    pub renamed: usize,
    #[serde(default)]
    pub credentials: usize,
}

fn unique_name(tx: &Connection, name: &str) -> Result<(String, bool)> {
    let exists = |n: &str| -> Result<bool> { Ok(tx.query_row("SELECT COUNT(*) FROM projects WHERE name = ?1", [n], |r| r.get::<_, i64>(0))? > 0) };
    if !exists(name)? {
        return Ok((name.to_string(), false));
    }
    let mut i = 1;
    loop {
        let candidate = if i == 1 { format!("{name} (imported)") } else { format!("{name} (imported {i})") };
        if !exists(&candidate)? {
            return Ok((candidate, true));
        }
        i += 1;
    }
}

/// Adds the backup's projects next to what is already in the vault (never overwrites).
pub fn import_json(conn: &mut Connection, keys: &VaultKeys, json: &[u8]) -> Result<ImportSummary> {
    let data: BackupData = serde_json::from_slice(json).map_err(|_| LokiError::Invalid("This backup file is damaged.".into()))?;
    if data.format != 1 {
        return Err(LokiError::Invalid("This backup was made by a newer version of LOKI.".into()));
    }
    let tx = conn.transaction()?;
    let mut summary = ImportSummary { projects: 0, targets: 0, snapshots: 0, renamed: 0, credentials: 0 };
    let mut new_ids: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for p in &data.projects {
        let (name, renamed) = unique_name(&tx, p.name.trim())?;
        if name.trim().is_empty() {
            return Err(LokiError::Invalid("A project in the backup has no name.".into()));
        }
        tx.execute(
            "INSERT INTO projects (name, root_path, description, created_at, updated_at) VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
            rusqlite::params![name, p.root_path, p.description],
        )?;
        let pid = tx.last_insert_rowid();
        new_ids.insert(p.name.trim().to_string(), pid);
        summary.projects += 1;
        summary.renamed += renamed as usize;
        for t in &p.targets {
            repo::validate_stage(&t.stage)?;
            tx.execute(
                "INSERT INTO targets (project_id, branch, stage, label, color, is_archived) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![pid, t.branch, t.stage, t.label, t.color, t.is_archived as i64],
            )?;
            let tid = tx.last_insert_rowid();
            summary.targets += 1;
            for s in &t.snapshots {
                let mut seen = HashSet::new();
                let mut entries = Vec::with_capacity(s.variables.len());
                for v in &s.variables {
                    if !is_valid_key(&v.key) || !seen.insert(v.key.clone()) {
                        return Err(LokiError::Invalid("This backup contains an invalid or duplicate key name.".into()));
                    }
                    entries.push(crate::core::parser::Entry { key: v.key.clone(), value: v.value.clone(), comment: v.comment.clone(), line: 0 });
                }
                repo::insert_snapshot(&tx, keys, tid, s.version, s.note.as_deref(), s.source_path.as_deref(), None, Some(&s.created_at), &entries)?;
                summary.snapshots += 1;
            }
        }
        for n in &p.key_notes {
            let stages = serde_json::to_string(&n.required_stages).map_err(|_| LokiError::Crypto)?;
            tx.execute(
                "INSERT INTO key_notes (project_id, key, description, required_stages, is_secret, last_rotated) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![pid, n.key, n.description, stages, n.is_secret as i64, n.last_rotated],
            )?;
        }
    }
    for c in &data.credentials {
        let pid = c.project.as_ref().and_then(|n| new_ids.get(n.trim()).copied());
        crate::core::credentials::import_one(&tx, keys, c, pid)?;
        summary.credentials += 1;
    }
    repo::audit(&tx, "backup_import", "vault", Some(&format!("{} projects, {} targets, {} snapshots, {} passwords", summary.projects, summary.targets, summary.snapshots, summary.credentials)))?;
    tx.commit()?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::{derive_keys, TEST_PARAMS};
    use crate::core::parser::parse;
    use crate::db::migrations::open_memory;

    fn keys(pw: &str) -> VaultKeys {
        derive_keys(pw.as_bytes(), &[5u8; 16], TEST_PARAMS).unwrap()
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let file = encrypt(b"{\"hello\":1}", "fake_backup_pw_1", TEST_PARAMS).unwrap();
        assert!(file.starts_with(b"LOKIBAK1"));
        assert!(!file.windows(5).any(|w| w == b"hello"));
        assert_eq!(decrypt(&file, "fake_backup_pw_1").unwrap().as_slice(), b"{\"hello\":1}");
    }

    #[test]
    fn wrong_password_bad_magic_and_tampering_fail() {
        let file = encrypt(b"data", "fake_backup_pw_1", TEST_PARAMS).unwrap();
        assert!(matches!(decrypt(&file, "fake_backup_pw_2"), Err(LokiError::WrongPassword)));
        assert!(matches!(decrypt(b"NOTALOKIFILE-----------------------------------------------", "x"), Err(LokiError::Invalid(_))));
        assert!(matches!(decrypt(b"short", "x"), Err(LokiError::Invalid(_))));
        let mut t = file.clone();
        let last = t.len() - 1;
        t[last] ^= 1;
        assert!(decrypt(&t, "fake_backup_pw_1").is_err());
        // Header tampering (KDF cost) is caught: either rejected as damaged or AEAD fails.
        let mut h = file.clone();
        h[12] ^= 1;
        assert!(decrypt(&h, "fake_backup_pw_1").is_err());
        // A crafted header asking for absurd memory is refused before deriving.
        let mut huge = file.clone();
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(decrypt(&huge, "fake_backup_pw_1"), Err(LokiError::Invalid(_))));
    }

    #[test]
    fn full_round_trip_into_a_different_vault_with_rename_on_conflict() {
        let src_keys = keys("fake_password_1");
        let mut src = open_memory();
        let p = repo::create_project(&mut src, "API", Some("C:\\Users\\Name\\My Projects\\app".into()), Some("desc".into())).unwrap();
        let prod = repo::create_target(&mut src, p.id, "main", "production", Some("Prod".into()), None).unwrap();
        repo::create_snapshot(&mut src, &src_keys, prod.id, &parse("# c1\nA=fake_value_1\n").entries, Some("first".into()), None, None, "import").unwrap();
        repo::create_snapshot(&mut src, &src_keys, prod.id, &parse("A=fake_value_2\nB=\"multi\nline\"\n").entries, None, Some("C:\\x\\.env"), Some(&[1]), "import").unwrap();
        repo::set_key_note(&mut src, p.id, "A", Some("what A is".into()), vec!["production".into()], true, None).unwrap();
        crate::core::credentials::create(
            &mut src,
            &src_keys,
            &crate::core::credentials::CredentialInput {
                title: "Prod DB".into(),
                username: "fake_user".into(),
                url: String::new(),
                tags: vec![],
                project_id: Some(p.id),
                password: Some("fake_cred_password_1".into()),
                notes: None,
            },
        )
        .unwrap();

        let json = export_json(&src, &src_keys).unwrap();
        let file = encrypt(&json, "fake_backup_pw_1", TEST_PARAMS).unwrap();
        assert!(!file.windows(12).any(|w| w == b"fake_value_1"));
        assert!(!file.windows(20).any(|w| w == b"fake_cred_password_1"));
        let plain = decrypt(&file, "fake_backup_pw_1").unwrap();

        // Different vault (different keys), and it already has a project called "API".
        let dst_keys = keys("fake_password_2");
        let mut dst = open_memory();
        repo::create_project(&mut dst, "API", None, None).unwrap();
        let summary = import_json(&mut dst, &dst_keys, &plain).unwrap();
        assert_eq!(summary, ImportSummary { projects: 1, targets: 1, snapshots: 2, renamed: 1, credentials: 1 });
        let creds = crate::core::credentials::list(&dst, &dst_keys).unwrap();
        assert_eq!(creds[0].title, "Prod DB");
        assert_eq!(creds[0].project_name.as_deref(), Some("API (imported)"), "credential follows its renamed project");
        let pw = crate::core::credentials::reveal(&dst, &dst_keys, creds[0].id, crate::core::credentials::Field::Password, "x").unwrap();
        assert_eq!(pw.as_str(), "fake_cred_password_1");

        let projects = repo::list_projects(&dst).unwrap();
        assert_eq!(projects.len(), 2);
        let imported = projects.iter().find(|x| x.name == "API (imported)").unwrap();
        assert_eq!(imported.root_path.as_deref(), Some("C:\\Users\\Name\\My Projects\\app"));
        let t = &repo::list_targets(&dst, imported.id).unwrap()[0];
        assert_eq!((t.snapshot_count, t.label.as_deref()), (2, Some("Prod")));
        let snaps = repo::list_snapshots(&dst, t.id).unwrap();
        assert_eq!(snaps[1].version, 1);
        assert_eq!(snaps[1].note.as_deref(), Some("first"));
        let v1 = repo::load_entries(&dst, &dst_keys, snaps[1].id).unwrap();
        assert_eq!((v1[0].value.as_str(), v1[0].comment.as_deref()), ("fake_value_1", Some("c1")));
        let v2 = repo::load_entries(&dst, &dst_keys, snaps[0].id).unwrap();
        assert_eq!(v2[1].value, "multi\nline");
        assert_eq!(repo::list_key_notes(&dst, imported.id).unwrap().len(), 1);
        // Importing twice adds again under a new name instead of overwriting.
        let again = import_json(&mut dst, &dst_keys, &plain).unwrap();
        assert_eq!(again.renamed, 1);
        assert!(repo::list_projects(&dst).unwrap().iter().any(|x| x.name == "API (imported 2)"));
    }

    #[test]
    fn bad_json_and_bad_content_are_rejected_atomically() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        assert!(import_json(&mut conn, &k, b"not json").is_err());
        assert!(import_json(&mut conn, &k, br#"{"format":2,"projects":[]}"#).is_err());
        let bad = br#"{"format":1,"projects":[{"name":"P","root_path":null,"description":null,"key_notes":[],"targets":[{"branch":"main","stage":"production","label":null,"color":null,"is_archived":false,"snapshots":[{"version":1,"note":null,"source_path":null,"created_at":"2026-01-01T00:00:00Z","variables":[{"key":"bad key","value":"x","comment":null}]}]}]}]}"#;
        assert!(import_json(&mut conn, &k, bad).is_err());
        assert!(repo::list_projects(&conn).unwrap().is_empty()); // rolled back
    }
}

#[cfg(test)]
mod compat_tests {
    use super::*;
    use crate::core::crypto::{derive_keys, TEST_PARAMS};
    use crate::db::migrations::open_memory;

    #[test]
    fn backups_made_before_the_password_store_still_import() {
        let k = derive_keys(b"fake_password_1", &[5u8; 16], TEST_PARAMS).unwrap();
        let mut conn = open_memory();
        let old = br#"{"format":1,"projects":[{"name":"Old","root_path":null,"description":null,"key_notes":[],"targets":[]}]}"#;
        let s = import_json(&mut conn, &k, old).unwrap();
        assert_eq!((s.projects, s.credentials), (1, 0));
    }

    #[test]
    fn magic_keeps_backup_and_share_files_apart() {
        let file = encrypt_with(b"LOKISHR1", b"data", "fake_backup_pw_1", TEST_PARAMS).unwrap();
        assert!(matches!(decrypt(&file, "fake_backup_pw_1"), Err(LokiError::Invalid(_))));
    }
}
