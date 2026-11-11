//! Password store inside the vault.
//!
//! Each credential has two encrypted blobs, both bound to the row by a random `uid` in the AAD:
//! - `meta`: title, username, URL, tags. Decrypted to build the list.
//! - `secret`: password and notes. Decrypted only on an explicit Reveal or Copy (audited).
//!
//! `secret_fp` (HMAC of the password) lets the list flag reused passwords without decrypting.

use crate::core::crypto::{random_bytes, random_index, to_hex, VaultKeys};
use crate::db::repo;
use crate::error::{LokiError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ','now')";

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Meta {
    title: String,
    username: String,
    url: String,
    tags: Vec<String>,
    has_notes: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Secret {
    #[serde(default)]
    password: String,
    #[serde(default)]
    notes: String,
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.password.zeroize();
        self.notes.zeroize();
    }
}

/// What the form sends. `password` / `notes` set to `None` on update means "keep as is".
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInput {
    pub title: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub project_id: Option<i64>,
    pub password: Option<String>,
    pub notes: Option<String>,
}

impl Drop for CredentialInput {
    fn drop(&mut self) {
        if let Some(p) = self.password.as_mut() {
            p.zeroize();
        }
        if let Some(n) = self.notes.as_mut() {
            n.zeroize();
        }
    }
}

/// List entry. Never contains the password or the notes.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDto {
    pub id: i64,
    pub title: String,
    pub username: String,
    pub url: String,
    pub tags: Vec<String>,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub has_notes: bool,
    pub is_weak: bool,
    pub is_reused: bool,
    pub created_at: String,
    pub updated_at: String,
    pub password_changed_at: String,
    pub age_days: i64,
}

fn meta_aad(uid: &[u8]) -> Vec<u8> {
    format!("cred:{}:meta", to_hex(uid)).into_bytes()
}

fn secret_aad(uid: &[u8]) -> Vec<u8> {
    format!("cred:{}:secret", to_hex(uid)).into_bytes()
}

/// Weak = short, low variety, or a well-known password.
pub fn is_weak(pw: &str) -> bool {
    let len = pw.chars().count();
    let classes = [
        pw.chars().any(|c| c.is_ascii_lowercase()),
        pw.chars().any(|c| c.is_ascii_uppercase()),
        pw.chars().any(|c| c.is_ascii_digit()),
        pw.chars().any(|c| !c.is_ascii_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    let lower = pw.to_lowercase();
    let common = ["password", "123456", "qwerty", "letmein", "welcome", "admin", "iloveyou", "changeme"];
    len < 12 || (len < 16 && classes < 3) || common.iter().any(|c| lower.contains(c))
}

fn clean_meta(input: &CredentialInput, has_notes: bool) -> Result<Meta> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(LokiError::Invalid("Give the entry a title, for example `AWS root` or `Staging DB`.".into()));
    }
    let mut tags: Vec<String> = input.tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    tags.sort();
    tags.dedup();
    Ok(Meta { title, username: input.username.trim().to_string(), url: input.url.trim().to_string(), tags, has_notes })
}

fn seal_json<T: Serialize>(keys: &VaultKeys, aad: &[u8], value: &T) -> Result<crate::core::crypto::Sealed> {
    let json = Zeroizing::new(serde_json::to_vec(value).map_err(|_| LokiError::Crypto)?);
    keys.seal(aad, &json)
}

fn open_json<T: for<'de> Deserialize<'de>>(keys: &VaultKeys, aad: &[u8], nonce: &[u8], ct: &[u8]) -> Result<T> {
    let pt = keys.open(aad, nonce, ct)?;
    serde_json::from_slice(&pt).map_err(|_| LokiError::Crypto)
}

/// Fingerprint used for reuse detection. Empty passwords get a random value (never "reused").
fn password_fp(keys: &VaultKeys, pw: &str) -> Result<Vec<u8>> {
    if pw.is_empty() {
        return Ok(random_bytes::<32>()?.to_vec());
    }
    Ok(keys.fingerprint(format!("credential-password:{pw}").as_bytes())?.to_vec())
}

fn project_exists(conn: &Connection, id: Option<i64>) -> Result<()> {
    if let Some(id) = id {
        repo::get_project(conn, id)?;
    }
    Ok(())
}

pub fn list(conn: &Connection, keys: &VaultKeys) -> Result<Vec<CredentialDto>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.uid, c.project_id, p.name, c.meta_nonce, c.meta_ct, c.is_weak, \
         (SELECT COUNT(*) FROM credentials c2 WHERE c2.secret_fp = c.secret_fp) > 1, \
         c.created_at, c.updated_at, c.password_changed_at, \
         CAST(julianday('now') - julianday(c.password_changed_at) AS INTEGER) \
         FROM credentials c LEFT JOIN projects p ON p.id = c.project_id",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let uid: Vec<u8> = r.get(1)?;
        let nonce: Vec<u8> = r.get(4)?;
        let ct: Vec<u8> = r.get(5)?;
        let meta: Meta = open_json(keys, &meta_aad(&uid), &nonce, &ct)?;
        out.push(CredentialDto {
            id: r.get(0)?,
            title: meta.title,
            username: meta.username,
            url: meta.url,
            tags: meta.tags,
            project_id: r.get(2)?,
            project_name: r.get(3)?,
            has_notes: meta.has_notes,
            is_weak: r.get::<_, i64>(6)? != 0,
            is_reused: r.get::<_, i64>(7)? != 0,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
            password_changed_at: r.get(10)?,
            age_days: r.get::<_, Option<i64>>(11)?.unwrap_or(0),
        });
    }
    out.sort_by_key(|c| c.title.to_lowercase());
    Ok(out)
}

fn get(conn: &Connection, keys: &VaultKeys, id: i64) -> Result<CredentialDto> {
    list(conn, keys)?.into_iter().find(|c| c.id == id).ok_or(LokiError::NotFound("Password entry"))
}

/// Inserts a credential. Call inside a transaction. Used by create and by backup import.
#[allow(clippy::too_many_arguments)]
fn insert(
    conn: &Connection,
    keys: &VaultKeys,
    project_id: Option<i64>,
    meta: &Meta,
    secret: &Secret,
    created_at: Option<&str>,
    updated_at: Option<&str>,
    changed_at: Option<&str>,
) -> Result<i64> {
    let uid = random_bytes::<16>()?;
    let m = seal_json(keys, &meta_aad(&uid), meta)?;
    let s = seal_json(keys, &secret_aad(&uid), secret)?;
    let fp = password_fp(keys, &secret.password)?;
    conn.execute(
        &format!(
            "INSERT INTO credentials (uid, project_id, meta_nonce, meta_ct, secret_nonce, secret_ct, secret_fp, is_weak, created_at, updated_at, password_changed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, COALESCE(?9, {NOW}), COALESCE(?10, {NOW}), COALESCE(?11, {NOW}))"
        ),
        params![
            uid.as_slice(),
            project_id,
            m.nonce.as_slice(),
            m.ct,
            s.nonce.as_slice(),
            s.ct,
            fp,
            is_weak(&secret.password) as i64,
            created_at,
            updated_at,
            changed_at
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn create(conn: &mut Connection, keys: &VaultKeys, input: &CredentialInput) -> Result<CredentialDto> {
    project_exists(conn, input.project_id)?;
    let notes = input.notes.clone().unwrap_or_default();
    let meta = clean_meta(input, !notes.trim().is_empty())?;
    let secret = Secret { password: input.password.clone().unwrap_or_default(), notes };
    let tx = conn.transaction()?;
    let id = insert(&tx, keys, input.project_id, &meta, &secret, None, None, None)?;
    repo::audit(&tx, "credential_create", &format!("credential:{id}"), Some(&meta.title))?;
    tx.commit()?;
    get(conn, keys, id)
}

struct Row {
    uid: Vec<u8>,
    secret_nonce: Vec<u8>,
    secret_ct: Vec<u8>,
}

fn load_row(conn: &Connection, id: i64) -> Result<Row> {
    conn.query_row("SELECT uid, secret_nonce, secret_ct FROM credentials WHERE id = ?1", [id], |r| {
        Ok(Row { uid: r.get(0)?, secret_nonce: r.get(1)?, secret_ct: r.get(2)? })
    })
    .optional()?
    .ok_or(LokiError::NotFound("Password entry"))
}

pub fn update(conn: &mut Connection, keys: &VaultKeys, id: i64, input: &CredentialInput) -> Result<CredentialDto> {
    project_exists(conn, input.project_id)?;
    let row = load_row(conn, id)?;
    let mut secret: Secret = open_json(keys, &secret_aad(&row.uid), &row.secret_nonce, &row.secret_ct)?;
    let password_changed = matches!(&input.password, Some(p) if *p != secret.password);
    if let Some(p) = &input.password {
        secret.password = p.clone();
    }
    if let Some(n) = &input.notes {
        secret.notes = n.clone();
    }
    let meta = clean_meta(input, !secret.notes.trim().is_empty())?;
    let m = seal_json(keys, &meta_aad(&row.uid), &meta)?;
    let s = seal_json(keys, &secret_aad(&row.uid), &secret)?;
    let fp = password_fp(keys, &secret.password)?;
    let tx = conn.transaction()?;
    tx.execute(
        &format!(
            "UPDATE credentials SET project_id = ?1, meta_nonce = ?2, meta_ct = ?3, secret_nonce = ?4, secret_ct = ?5, secret_fp = ?6, is_weak = ?7, \
             updated_at = {NOW}, password_changed_at = CASE WHEN ?8 THEN {NOW} ELSE password_changed_at END WHERE id = ?9"
        ),
        params![input.project_id, m.nonce.as_slice(), m.ct, s.nonce.as_slice(), s.ct, fp, is_weak(&secret.password) as i64, password_changed, id],
    )?;
    repo::audit(&tx, if password_changed { "credential_password_change" } else { "credential_update" }, &format!("credential:{id}"), Some(&meta.title))?;
    tx.commit()?;
    get(conn, keys, id)
}

pub fn delete(conn: &mut Connection, keys: &VaultKeys, id: i64) -> Result<()> {
    let title = get(conn, keys, id)?.title;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM credentials WHERE id = ?1", [id])?;
    repo::audit(&tx, "credential_delete", &format!("credential:{id}"), Some(&title))?;
    tx.commit()?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Password,
    Notes,
}

impl Field {
    pub fn parse(s: &str) -> Result<Field> {
        match s {
            "password" => Ok(Field::Password),
            "notes" => Ok(Field::Notes),
            _ => Err(LokiError::Invalid("Field must be password or notes.".into())),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Field::Password => "password",
            Field::Notes => "notes",
        }
    }
}

/// Decrypts one field and writes the audit entry. If the audit fails, nothing is returned.
pub fn reveal(conn: &Connection, keys: &VaultKeys, id: i64, field: Field, action: &str) -> Result<Zeroizing<String>> {
    let row = load_row(conn, id)?;
    let secret: Secret = open_json(keys, &secret_aad(&row.uid), &row.secret_nonce, &row.secret_ct)?;
    let title = get(conn, keys, id)?.title;
    repo::audit(conn, action, &format!("credential:{id}"), Some(&format!("{title}: {}", field.name())))?;
    Ok(Zeroizing::new(match field {
        Field::Password => secret.password.clone(),
        Field::Notes => secret.notes.clone(),
    }))
}

/// Re-encrypts every credential with `new` keys. Call inside the change-password transaction.
pub fn reencrypt_all(conn: &Connection, old: &VaultKeys, new: &VaultKeys) -> Result<usize> {
    type R = (i64, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
    let rows: Vec<R> = {
        let mut stmt = conn.prepare("SELECT id, uid, meta_nonce, meta_ct, secret_nonce, secret_ct FROM credentials")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?;
        mapped.collect::<std::result::Result<_, _>>()?
    };
    let count = rows.len();
    for (id, uid, mn, mc, sn, sc) in rows {
        let meta: Meta = open_json(old, &meta_aad(&uid), &mn, &mc)?;
        let secret: Secret = open_json(old, &secret_aad(&uid), &sn, &sc)?;
        let m = seal_json(new, &meta_aad(&uid), &meta)?;
        let s = seal_json(new, &secret_aad(&uid), &secret)?;
        let fp = password_fp(new, &secret.password)?;
        conn.execute(
            "UPDATE credentials SET meta_nonce = ?1, meta_ct = ?2, secret_nonce = ?3, secret_ct = ?4, secret_fp = ?5 WHERE id = ?6",
            params![m.nonce.as_slice(), m.ct, s.nonce.as_slice(), s.ct, fp, id],
        )?;
    }
    Ok(count)
}

// ---------------------------------------------------------------- backup support

#[derive(Serialize, Deserialize)]
pub struct BackupCredential {
    pub project: Option<String>,
    pub title: String,
    pub username: String,
    pub url: String,
    pub tags: Vec<String>,
    pub password: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
    pub password_changed_at: String,
}

impl Drop for BackupCredential {
    fn drop(&mut self) {
        self.password.zeroize();
        self.notes.zeroize();
    }
}

pub fn export_all(conn: &Connection, keys: &VaultKeys) -> Result<Vec<BackupCredential>> {
    let mut out = Vec::new();
    for c in list(conn, keys)? {
        let row = load_row(conn, c.id)?;
        let secret: Secret = open_json(keys, &secret_aad(&row.uid), &row.secret_nonce, &row.secret_ct)?;
        out.push(BackupCredential {
            project: c.project_name.clone(),
            title: c.title.clone(),
            username: c.username.clone(),
            url: c.url.clone(),
            tags: c.tags.clone(),
            password: secret.password.clone(),
            notes: secret.notes.clone(),
            created_at: c.created_at.clone(),
            updated_at: c.updated_at.clone(),
            password_changed_at: c.password_changed_at.clone(),
        });
    }
    Ok(out)
}

/// Inserts a backed-up credential. `project_id` is already mapped to this vault.
pub fn import_one(conn: &Connection, keys: &VaultKeys, c: &BackupCredential, project_id: Option<i64>) -> Result<()> {
    let meta = Meta { title: c.title.clone(), username: c.username.clone(), url: c.url.clone(), tags: c.tags.clone(), has_notes: !c.notes.trim().is_empty() };
    if meta.title.trim().is_empty() {
        return Err(LokiError::Invalid("A password entry in the backup has no title.".into()));
    }
    let secret = Secret { password: c.password.clone(), notes: c.notes.clone() };
    insert(conn, keys, project_id, &meta, &secret, Some(&c.created_at), Some(&c.updated_at), Some(&c.password_changed_at))?;
    Ok(())
}

// ---------------------------------------------------------------- generator

const LOWER: &str = "abcdefghijkmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &str = "23456789";
const SYMBOLS: &str = "!@#$%^&*-_=+?";

/// Random password from the OS CSPRNG. Ambiguous characters (0/O, 1/l/I) are left out.
/// Always includes at least one character from each enabled class.
pub fn generate_password(length: usize, symbols: bool) -> Result<Zeroizing<String>> {
    let length = length.clamp(12, 128);
    let mut classes: Vec<&str> = vec![LOWER, UPPER, DIGITS];
    if symbols {
        classes.push(SYMBOLS);
    }
    let all: Vec<char> = classes.concat().chars().collect();
    loop {
        let mut out = Zeroizing::new(String::with_capacity(length));
        for _ in 0..length {
            out.push(all[random_index(all.len() as u32)? as usize]);
        }
        if classes.iter().all(|cls| out.chars().any(|c| cls.contains(c))) {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::{derive_keys, TEST_PARAMS};
    use crate::db::migrations::open_memory;

    fn keys(pw: &str) -> VaultKeys {
        derive_keys(pw.as_bytes(), &[9u8; 16], TEST_PARAMS).unwrap()
    }

    fn input(title: &str, pw: Option<&str>) -> CredentialInput {
        CredentialInput {
            title: title.into(),
            username: "fake_user".into(),
            url: "db.internal".into(),
            tags: vec!["db".into(), " db ".into(), "".into()],
            project_id: None,
            password: pw.map(Into::into),
            notes: Some("fake_note_secret".into()),
        }
    }

    #[test]
    fn create_list_reveal_and_nothing_secret_in_the_list() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        let c = create(&mut conn, &k, &input("Staging DB", Some("fake_secret_pw_123!X"))).unwrap();
        assert_eq!(c.title, "Staging DB");
        assert_eq!(c.tags, ["db"]);
        assert!(c.has_notes && !c.is_reused);
        let json = serde_json::to_string(&list(&conn, &k).unwrap()).unwrap();
        assert!(!json.contains("fake_secret_pw_123") && !json.contains("fake_note_secret"));
        assert_eq!(reveal(&conn, &k, c.id, Field::Password, "credential_reveal").unwrap().as_str(), "fake_secret_pw_123!X");
        assert_eq!(reveal(&conn, &k, c.id, Field::Notes, "credential_reveal").unwrap().as_str(), "fake_note_secret");
        // Audited by name, never by value.
        let audit = serde_json::to_string(&repo::audit_list(&conn, 100).unwrap()).unwrap();
        assert!(audit.contains("credential_reveal") && !audit.contains("fake_secret_pw_123"));
    }

    #[test]
    fn nothing_is_plaintext_at_rest() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        create(&mut conn, &k, &input("Unique_Title_Marker", Some("fake_secret_pw_123!X"))).unwrap();
        let blobs: Vec<Vec<u8>> = {
            let mut s = conn.prepare("SELECT meta_ct FROM credentials UNION ALL SELECT secret_ct FROM credentials").unwrap();
            s.query_map([], |r| r.get(0)).unwrap().map(|x| x.unwrap()).collect()
        };
        for b in blobs {
            for needle in [&b"fake_secret_pw_123"[..], b"Unique_Title_Marker", b"fake_note_secret", b"fake_user"] {
                assert!(!b.windows(needle.len()).any(|w| w == needle));
            }
        }
    }

    #[test]
    fn wrong_key_and_swapped_rows_fail() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        let a = create(&mut conn, &k, &input("A", Some("fake_pw_aaaaaaaaaaa1!"))).unwrap();
        let b = create(&mut conn, &k, &input("B", Some("fake_pw_bbbbbbbbbbb2!"))).unwrap();
        assert!(list(&conn, &keys("fake_password_2")).is_err());
        conn.execute("UPDATE credentials SET secret_nonce = (SELECT secret_nonce FROM credentials WHERE id = ?1), secret_ct = (SELECT secret_ct FROM credentials WHERE id = ?1) WHERE id = ?2", [a.id, b.id]).unwrap();
        assert!(reveal(&conn, &k, b.id, Field::Password, "x").is_err());
    }

    #[test]
    fn update_keeps_password_unless_given_and_tracks_reuse_and_weakness() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        let a = create(&mut conn, &k, &input("A", Some("short"))).unwrap();
        assert!(a.is_weak);
        let b = create(&mut conn, &k, &input("B", Some("Fake-Strong-Phrase-2026!"))).unwrap();
        assert!(!b.is_weak);
        // Same password on A: both flagged as reused.
        let mut i = input("A renamed", Some("Fake-Strong-Phrase-2026!"));
        i.notes = None;
        let a2 = update(&mut conn, &k, a.id, &i).unwrap();
        assert_eq!(a2.title, "A renamed");
        assert!(a2.is_reused && list(&conn, &k).unwrap().iter().all(|c| c.is_reused));
        assert!(a2.has_notes, "notes kept when not sent");
        // Title-only change keeps the password.
        let mut keep = input("A again", None);
        keep.notes = Some(String::new());
        let a3 = update(&mut conn, &k, a.id, &keep).unwrap();
        assert!(!a3.has_notes);
        assert_eq!(reveal(&conn, &k, a.id, Field::Password, "x").unwrap().as_str(), "Fake-Strong-Phrase-2026!");
        delete(&mut conn, &k, b.id).unwrap();
        assert!(!list(&conn, &k).unwrap()[0].is_reused);
    }

    #[test]
    fn empty_passwords_are_never_reused() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        create(&mut conn, &k, &input("A", None)).unwrap();
        create(&mut conn, &k, &input("B", None)).unwrap();
        assert!(list(&conn, &k).unwrap().iter().all(|c| !c.is_reused));
    }

    #[test]
    fn validation() {
        let k = keys("fake_password_1");
        let mut conn = open_memory();
        assert!(create(&mut conn, &k, &input("  ", Some("x"))).is_err());
        let mut bad = input("A", Some("x"));
        bad.project_id = Some(999);
        assert!(create(&mut conn, &k, &bad).is_err());
        assert!(Field::parse("nope").is_err());
    }

    #[test]
    fn reencrypt_moves_everything_to_the_new_key() {
        let old = keys("fake_password_1");
        let new = keys("fake_password_2");
        let mut conn = open_memory();
        let a = create(&mut conn, &old, &input("A", Some("Fake-Strong-Phrase-2026!"))).unwrap();
        create(&mut conn, &old, &input("B", Some("Fake-Strong-Phrase-2026!"))).unwrap();
        assert_eq!(reencrypt_all(&conn, &old, &new).unwrap(), 2);
        assert!(list(&conn, &old).is_err());
        let l = list(&conn, &new).unwrap();
        assert!(l.iter().all(|c| c.is_reused), "fingerprints recomputed consistently");
        assert_eq!(reveal(&conn, &new, a.id, Field::Password, "x").unwrap().as_str(), "Fake-Strong-Phrase-2026!");
    }

    #[test]
    fn generator() {
        for len in [12, 20, 64] {
            let p = generate_password(len, true).unwrap();
            assert_eq!(p.chars().count(), len);
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
            assert!(p.chars().any(|c| DIGITS.contains(c)));
            assert!(!p.contains('0') && !p.contains('O') && !p.contains('l') && !p.contains('1'));
            assert!(!is_weak(&p));
        }
        let p = generate_password(3, false).unwrap();
        assert_eq!(p.chars().count(), 12, "clamped to a safe minimum");
        assert!(!p.chars().any(|c| SYMBOLS.contains(c)));
        assert_ne!(*generate_password(20, true).unwrap(), *generate_password(20, true).unwrap());
    }

    #[test]
    fn weakness_rules() {
        assert!(is_weak("short1!"));
        assert!(is_weak("alllowercase"));
        assert!(!is_weak("alllowercaselongenough"), "16+ characters is fine even with one class");
        assert!(is_weak("MyPassword123456!"));
        assert!(!is_weak("Tr7#kLp2-Qz9mW"));
    }
}
