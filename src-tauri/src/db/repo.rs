//! All SQL lives here. Every write path runs inside a transaction. Values are sealed and opened
//! with `VaultKeys`; no function here logs or returns a plaintext value except `reveal` and
//! `load_entries`, which are called only by explicit user actions (reveal, copy, export, restore,
//! backup, push check).

use super::models::*;
use crate::core::crypto::{comment_aad, to_hex, value_aad, KdfParams, VaultKeys, NONCE_LEN};
use crate::core::diff::VarMeta;
use crate::core::parser::Entry;
use crate::core::rules::KeyNote;
use crate::error::{LokiError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use zeroize::Zeroizing;

const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%SZ','now')";
pub const STAGES: [&str; 6] = ["local", "dev", "staging", "preview", "production", "other"];

pub fn validate_stage(stage: &str) -> Result<()> {
    if STAGES.contains(&stage) {
        Ok(())
    } else {
        Err(LokiError::Invalid("Stage must be local, dev, staging, preview, production or other.".into()))
    }
}

fn clean(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------- vault meta

pub struct VaultMeta {
    pub salt: Vec<u8>,
    pub params: KdfParams,
    pub verifier_nonce: Vec<u8>,
    pub verifier_ct: Vec<u8>,
}

pub fn read_meta(conn: &Connection) -> Result<Option<VaultMeta>> {
    let row = conn
        .query_row(
            "SELECT kdf_salt, kdf_m_cost, kdf_t_cost, kdf_p_cost, verifier_nonce, verifier_ct FROM vault_meta WHERE id = 1",
            [],
            |r| {
                Ok(VaultMeta {
                    salt: r.get(0)?,
                    params: KdfParams { m_cost: r.get(1)?, t_cost: r.get(2)?, p_cost: r.get(3)? },
                    verifier_nonce: r.get(4)?,
                    verifier_ct: r.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

pub fn insert_meta(conn: &Connection, salt: &[u8], p: KdfParams, nonce: &[u8], ct: &[u8]) -> Result<()> {
    conn.execute(
        &format!("INSERT INTO vault_meta (id, kdf_salt, kdf_m_cost, kdf_t_cost, kdf_p_cost, verifier_nonce, verifier_ct, created_at) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, {NOW})"),
        params![salt, p.m_cost, p.t_cost, p.p_cost, nonce, ct],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- audit & settings

/// `detail` may name projects, targets and keys. It must never contain a value.
pub fn audit(conn: &Connection, action: &str, entity: &str, detail: Option<&str>) -> Result<()> {
    conn.execute(
        &format!("INSERT INTO audit_log (ts, action, entity, detail) VALUES ({NOW}, ?1, ?2, ?3)"),
        params![action, entity, detail],
    )?;
    Ok(())
}

pub fn audit_list(conn: &Connection, limit: i64) -> Result<Vec<AuditDto>> {
    let mut stmt = conn.prepare("SELECT id, ts, action, entity, detail FROM audit_log ORDER BY id DESC LIMIT ?1")?;
    let rows = stmt.query_map([limit.clamp(1, 1000)], |r| {
        Ok(AuditDto { id: r.get(0)?, ts: r.get(1)?, action: r.get(2)?, entity: r.get(3)?, detail: r.get(4)? })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- projects

const PROJECT_SELECT: &str = "SELECT p.id, p.name, p.root_path, p.description, (SELECT COUNT(*) FROM targets t WHERE t.project_id = p.id), p.created_at, p.updated_at FROM projects p";

fn map_project(r: &rusqlite::Row) -> rusqlite::Result<ProjectDto> {
    Ok(ProjectDto {
        id: r.get(0)?,
        name: r.get(1)?,
        root_path: r.get(2)?,
        description: r.get(3)?,
        target_count: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
    })
}

pub fn list_projects(conn: &Connection) -> Result<Vec<ProjectDto>> {
    let mut stmt = conn.prepare(&format!("{PROJECT_SELECT} ORDER BY p.name COLLATE NOCASE"))?;
    let rows = stmt.query_map([], map_project)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn get_project(conn: &Connection, id: i64) -> Result<ProjectDto> {
    conn.query_row(&format!("{PROJECT_SELECT} WHERE p.id = ?1"), [id], map_project)
        .optional()?
        .ok_or(LokiError::NotFound("Project"))
}

pub fn create_project(conn: &mut Connection, name: &str, root_path: Option<String>, description: Option<String>) -> Result<ProjectDto> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LokiError::Invalid("Give the project a name.".into()));
    }
    let tx = conn.transaction()?;
    tx.execute(
        &format!("INSERT INTO projects (name, root_path, description, created_at, updated_at) VALUES (?1, ?2, ?3, {NOW}, {NOW})"),
        params![name, clean(root_path), clean(description)],
    )
    .map_err(|e| name_conflict(e, "A project with that name already exists."))?;
    let id = tx.last_insert_rowid();
    audit(&tx, "project_create", &format!("project:{id}"), Some(name))?;
    tx.commit()?;
    get_project(conn, id)
}

fn name_conflict(e: rusqlite::Error, msg: &str) -> LokiError {
    match LokiError::from(e) {
        LokiError::Conflict(_) => LokiError::Conflict(msg.into()),
        other => other,
    }
}

pub fn update_project(conn: &mut Connection, id: i64, name: &str, root_path: Option<String>, description: Option<String>) -> Result<ProjectDto> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LokiError::Invalid("Give the project a name.".into()));
    }
    let tx = conn.transaction()?;
    let n = tx
        .execute(
            &format!("UPDATE projects SET name = ?1, root_path = ?2, description = ?3, updated_at = {NOW} WHERE id = ?4"),
            params![name, clean(root_path), clean(description), id],
        )
        .map_err(|e| name_conflict(e, "A project with that name already exists."))?;
    if n == 0 {
        return Err(LokiError::NotFound("Project"));
    }
    audit(&tx, "project_update", &format!("project:{id}"), Some(name))?;
    tx.commit()?;
    get_project(conn, id)
}

pub fn delete_project(conn: &mut Connection, id: i64) -> Result<()> {
    let name = get_project(conn, id)?.name;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    audit(&tx, "project_delete", &format!("project:{id}"), Some(&name))?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------- targets

const TARGET_SELECT: &str = "SELECT t.id, t.project_id, t.branch, t.stage, t.label, t.color, t.is_archived, cs.id, cs.version, \
    (SELECT COUNT(*) FROM variables v WHERE v.snapshot_id = cs.id), \
    (SELECT COUNT(*) FROM snapshots s2 WHERE s2.target_id = t.id), cs.created_at \
    FROM targets t LEFT JOIN snapshots cs ON cs.id = (SELECT s.id FROM snapshots s WHERE s.target_id = t.id ORDER BY s.version DESC LIMIT 1)";

fn map_target(r: &rusqlite::Row) -> rusqlite::Result<TargetDto> {
    Ok(TargetDto {
        id: r.get(0)?,
        project_id: r.get(1)?,
        branch: r.get(2)?,
        stage: r.get(3)?,
        label: r.get(4)?,
        color: r.get(5)?,
        is_archived: r.get::<_, i64>(6)? != 0,
        current_snapshot_id: r.get(7)?,
        current_version: r.get(8)?,
        key_count: r.get(9)?,
        snapshot_count: r.get(10)?,
        last_imported: r.get(11)?,
    })
}

pub fn list_targets(conn: &Connection, project_id: i64) -> Result<Vec<TargetDto>> {
    let sql = format!(
        "{TARGET_SELECT} WHERE t.project_id = ?1 ORDER BY t.is_archived, \
         CASE t.stage WHEN 'production' THEN 0 WHEN 'staging' THEN 1 WHEN 'preview' THEN 2 WHEN 'dev' THEN 3 WHEN 'local' THEN 4 ELSE 5 END, t.branch COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([project_id], map_target)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn get_target(conn: &Connection, id: i64) -> Result<TargetDto> {
    conn.query_row(&format!("{TARGET_SELECT} WHERE t.id = ?1"), [id], map_target)
        .optional()?
        .ok_or(LokiError::NotFound("Target"))
}

pub fn create_target(conn: &mut Connection, project_id: i64, branch: &str, stage: &str, label: Option<String>, color: Option<String>) -> Result<TargetDto> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(LokiError::Invalid("Give the target a branch name, for example `main`.".into()));
    }
    validate_stage(stage)?;
    get_project(conn, project_id)?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO targets (project_id, branch, stage, label, color) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![project_id, branch, stage, clean(label), clean(color)],
    )
    .map_err(|e| name_conflict(e, "This project already has a target with that branch and stage."))?;
    let id = tx.last_insert_rowid();
    audit(&tx, "target_create", &format!("target:{id}"), Some(&format!("{branch} / {stage}")))?;
    tx.commit()?;
    get_target(conn, id)
}

pub fn update_target(conn: &mut Connection, id: i64, branch: &str, stage: &str, label: Option<String>, color: Option<String>) -> Result<TargetDto> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err(LokiError::Invalid("Give the target a branch name.".into()));
    }
    validate_stage(stage)?;
    let tx = conn.transaction()?;
    let n = tx
        .execute(
            "UPDATE targets SET branch = ?1, stage = ?2, label = ?3, color = ?4 WHERE id = ?5",
            params![branch, stage, clean(label), clean(color), id],
        )
        .map_err(|e| name_conflict(e, "This project already has a target with that branch and stage."))?;
    if n == 0 {
        return Err(LokiError::NotFound("Target"));
    }
    audit(&tx, "target_update", &format!("target:{id}"), Some(&format!("{branch} / {stage}")))?;
    tx.commit()?;
    get_target(conn, id)
}

pub fn set_archived(conn: &mut Connection, id: i64, archived: bool) -> Result<TargetDto> {
    let tx = conn.transaction()?;
    let n = tx.execute("UPDATE targets SET is_archived = ?1 WHERE id = ?2", params![archived as i64, id])?;
    if n == 0 {
        return Err(LokiError::NotFound("Target"));
    }
    audit(&tx, if archived { "target_archive" } else { "target_unarchive" }, &format!("target:{id}"), None)?;
    tx.commit()?;
    get_target(conn, id)
}

pub fn delete_target(conn: &mut Connection, id: i64) -> Result<()> {
    let t = get_target(conn, id)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM targets WHERE id = ?1", [id])?;
    audit(&tx, "target_delete", &format!("target:{id}"), Some(&format!("{} / {} ({} snapshots)", t.branch, t.stage, t.snapshot_count)))?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------- snapshots

#[cfg(test)]
pub fn current_snapshot_id(conn: &Connection, target_id: i64) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT id FROM snapshots WHERE target_id = ?1 ORDER BY version DESC LIMIT 1", [target_id], |r| r.get(0))
        .optional()?)
}

/// Inserts a snapshot and its variables. Call inside a transaction.
#[allow(clippy::too_many_arguments)]
pub fn insert_snapshot(
    conn: &Connection,
    keys: &VaultKeys,
    target_id: i64,
    version: i64,
    note: Option<&str>,
    source_path: Option<&str>,
    source_sha: Option<&[u8]>,
    created_at: Option<&str>,
    entries: &[Entry],
) -> Result<i64> {
    conn.execute(
        &format!("INSERT INTO snapshots (target_id, version, note, source_path, source_sha256, created_at) VALUES (?1, ?2, ?3, ?4, ?5, COALESCE(?6, {NOW}))"),
        params![target_id, version, note, source_path, source_sha, created_at],
    )?;
    let sid = conn.last_insert_rowid();
    let mut stmt = conn.prepare(
        "INSERT INTO variables (snapshot_id, key, position, value_nonce, value_ct, fingerprint, is_empty, comment) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for (pos, e) in entries.iter().enumerate() {
        let sealed = keys.seal(&value_aad(sid, &e.key), e.value.as_bytes())?;
        let fp = keys.fingerprint(e.value.as_bytes())?;
        let comment = match &e.comment {
            Some(c) => {
                let s = keys.seal(&comment_aad(sid, &e.key), c.as_bytes())?;
                let mut blob = s.nonce.to_vec();
                blob.extend_from_slice(&s.ct);
                Some(blob)
            }
            None => None,
        };
        stmt.execute(params![sid, e.key, pos as i64, sealed.nonce.as_slice(), sealed.ct, fp.as_slice(), e.value.is_empty() as i64, comment])?;
    }
    Ok(sid)
}

pub fn get_snapshot(conn: &Connection, id: i64) -> Result<SnapshotDto> {
    conn.query_row(
        "SELECT s.id, s.target_id, s.version, s.note, s.source_path, s.created_at, (SELECT COUNT(*) FROM variables v WHERE v.snapshot_id = s.id) FROM snapshots s WHERE s.id = ?1",
        [id],
        map_snapshot,
    )
    .optional()?
    .ok_or(LokiError::NotFound("Snapshot"))
}

fn map_snapshot(r: &rusqlite::Row) -> rusqlite::Result<SnapshotDto> {
    Ok(SnapshotDto {
        id: r.get(0)?,
        target_id: r.get(1)?,
        version: r.get(2)?,
        note: r.get(3)?,
        source_path: r.get(4)?,
        created_at: r.get(5)?,
        key_count: r.get(6)?,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn create_snapshot(
    conn: &mut Connection,
    keys: &VaultKeys,
    target_id: i64,
    entries: &[Entry],
    note: Option<String>,
    source_path: Option<&str>,
    source_sha: Option<&[u8]>,
    audit_action: &str,
) -> Result<SnapshotDto> {
    let note = clean(note);
    let target = get_target(conn, target_id)?;
    let tx = conn.transaction()?;
    let version: i64 = tx.query_row("SELECT COALESCE(MAX(version), 0) + 1 FROM snapshots WHERE target_id = ?1", [target_id], |r| r.get(0))?;
    let sid = insert_snapshot(&tx, keys, target_id, version, note.as_deref(), source_path, source_sha, None, entries)?;
    audit(&tx, audit_action, &format!("target:{target_id}"), Some(&format!("{} / {}: version {version}, {} keys", target.branch, target.stage, entries.len())))?;
    tx.commit()?;
    get_snapshot(conn, sid)
}

pub fn list_snapshots(conn: &Connection, target_id: i64) -> Result<Vec<SnapshotDto>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.target_id, s.version, s.note, s.source_path, s.created_at, (SELECT COUNT(*) FROM variables v WHERE v.snapshot_id = s.id) FROM snapshots s WHERE s.target_id = ?1 ORDER BY s.version DESC",
    )?;
    let rows = stmt.query_map([target_id], map_snapshot)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// The newest snapshot with a recorded source file, and the hash of the current snapshot's file.
pub struct LastSource {
    pub path: String,
    pub current_sha: Option<Vec<u8>>,
}

pub fn last_source(conn: &Connection, target_id: i64) -> Result<Option<LastSource>> {
    let path: Option<String> = conn
        .query_row("SELECT source_path FROM snapshots WHERE target_id = ?1 AND source_path IS NOT NULL ORDER BY version DESC LIMIT 1", [target_id], |r| r.get(0))
        .optional()?;
    let Some(path) = path else { return Ok(None) };
    let current_sha: Option<Vec<u8>> = conn
        .query_row("SELECT source_sha256 FROM snapshots WHERE target_id = ?1 ORDER BY version DESC LIMIT 1", [target_id], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(Some(LastSource { path, current_sha }))
}

pub fn load_meta(conn: &Connection, snapshot_id: i64) -> Result<Vec<VarMeta>> {
    let mut stmt = conn.prepare("SELECT id, key, position, fingerprint, is_empty FROM variables WHERE snapshot_id = ?1 ORDER BY position")?;
    let rows = stmt.query_map([snapshot_id], |r| {
        Ok(VarMeta { id: r.get(0)?, key: r.get(1)?, position: r.get(2)?, fp: r.get(3)?, is_empty: r.get::<_, i64>(4)? != 0 })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn list_keys(conn: &Connection, snapshot_id: i64) -> Result<Vec<KeyInfoDto>> {
    let mut stmt = conn.prepare("SELECT id, key, is_empty, comment IS NOT NULL, fingerprint FROM variables WHERE snapshot_id = ?1 ORDER BY position")?;
    let rows = stmt.query_map([snapshot_id], |r| {
        let fp: Vec<u8> = r.get(4)?;
        Ok(KeyInfoDto {
            variable_id: r.get(0)?,
            key: r.get(1)?,
            is_empty: r.get::<_, i64>(2)? != 0,
            has_comment: r.get::<_, i64>(3)? != 0,
            fingerprint: to_hex(&fp[..fp.len().min(4)]),
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn open_comment(keys: &VaultKeys, sid: i64, key: &str, blob: &[u8]) -> Result<String> {
    if blob.len() <= NONCE_LEN {
        return Err(LokiError::Crypto);
    }
    let pt = keys.open(&comment_aad(sid, key), &blob[..NONCE_LEN], &blob[NONCE_LEN..])?;
    String::from_utf8(pt.to_vec()).map_err(|_| LokiError::Crypto)
}

/// Decrypts a whole snapshot. Only for explicit user actions (export, restore, backup, push check).
pub fn load_entries(conn: &Connection, keys: &VaultKeys, snapshot_id: i64) -> Result<Vec<Entry>> {
    let mut stmt = conn.prepare("SELECT key, value_nonce, value_ct, comment FROM variables WHERE snapshot_id = ?1 ORDER BY position")?;
    let mut rows = stmt.query([snapshot_id])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let key: String = r.get(0)?;
        let nonce: Vec<u8> = r.get(1)?;
        let ct: Vec<u8> = r.get(2)?;
        let comment_blob: Option<Vec<u8>> = r.get(3)?;
        let pt = keys.open(&value_aad(snapshot_id, &key), &nonce, &ct)?;
        let value = String::from_utf8(pt.to_vec()).map_err(|_| LokiError::Crypto)?;
        let comment = match comment_blob {
            Some(b) => Some(open_comment(keys, snapshot_id, &key, &b)?),
            None => None,
        };
        out.push(Entry { key, value, comment, line: 0 });
    }
    Ok(out)
}

pub struct Revealed {
    pub value: Zeroizing<String>,
    pub label: String,
}

/// Decrypts one value. `label` is "project / branch / stage / KEY" for the audit log (no value).
pub fn reveal(conn: &Connection, keys: &VaultKeys, variable_id: i64) -> Result<Revealed> {
    let (sid, key, nonce, ct, project, branch, stage, version): (i64, String, Vec<u8>, Vec<u8>, String, String, String, i64) = conn
        .query_row(
            "SELECT v.snapshot_id, v.key, v.value_nonce, v.value_ct, p.name, t.branch, t.stage, s.version FROM variables v \
             JOIN snapshots s ON s.id = v.snapshot_id JOIN targets t ON t.id = s.target_id JOIN projects p ON p.id = t.project_id WHERE v.id = ?1",
            [variable_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .optional()?
        .ok_or(LokiError::NotFound("Variable"))?;
    let pt = keys.open(&value_aad(sid, &key), &nonce, &ct)?;
    let value = String::from_utf8(pt.to_vec()).map_err(|_| LokiError::Crypto)?;
    Ok(Revealed { value: Zeroizing::new(value), label: format!("{project} / {branch} / {stage} v{version} / {key}") })
}

/// Restore = a NEW snapshot copied from an old one. History is never rewritten.
pub fn restore_snapshot(conn: &mut Connection, keys: &VaultKeys, snapshot_id: i64, note: Option<String>) -> Result<SnapshotDto> {
    let old = get_snapshot(conn, snapshot_id)?;
    let entries = load_entries(conn, keys, snapshot_id)?;
    let note = clean(note).or_else(|| Some(format!("Restored from version {}", old.version)));
    create_snapshot(conn, keys, old.target_id, &entries, note, None, None, "snapshot_restore")
}

/// Re-encrypts every value and comment with `new` and recomputes fingerprints.
/// Call inside the same transaction that updates `vault_meta`.
pub fn reencrypt_all(conn: &Connection, old: &VaultKeys, new: &VaultKeys) -> Result<usize> {
    type Row = (i64, i64, String, Vec<u8>, Vec<u8>, Option<Vec<u8>>);
    let rows: Vec<Row> = {
        let mut stmt = conn.prepare("SELECT id, snapshot_id, key, value_nonce, value_ct, comment FROM variables")?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?;
        mapped.collect::<std::result::Result<_, _>>()?
    };
    let mut update = conn.prepare("UPDATE variables SET value_nonce = ?1, value_ct = ?2, fingerprint = ?3, comment = ?4 WHERE id = ?5")?;
    let count = rows.len();
    for (id, sid, key, nonce, ct, comment) in rows {
        let value = old.open(&value_aad(sid, &key), &nonce, &ct)?;
        let sealed = new.seal(&value_aad(sid, &key), &value)?;
        let fp = new.fingerprint(&value)?;
        let new_comment = match comment {
            Some(blob) => {
                if blob.len() <= NONCE_LEN {
                    return Err(LokiError::Crypto);
                }
                let pt = old.open(&comment_aad(sid, &key), &blob[..NONCE_LEN], &blob[NONCE_LEN..])?;
                let s = new.seal(&comment_aad(sid, &key), &pt)?;
                let mut b = s.nonce.to_vec();
                b.extend_from_slice(&s.ct);
                Some(b)
            }
            None => None,
        };
        update.execute(params![sealed.nonce.as_slice(), sealed.ct, fp.as_slice(), new_comment, id])?;
    }
    Ok(count)
}

// ---------------------------------------------------------------- key notes

pub fn list_key_notes(conn: &Connection, project_id: i64) -> Result<Vec<KeyNoteDto>> {
    let mut stmt = conn.prepare("SELECT key, description, required_stages, is_secret, last_rotated FROM key_notes WHERE project_id = ?1 ORDER BY key")?;
    let rows = stmt.query_map([project_id], |r| {
        let stages: Option<String> = r.get(2)?;
        Ok(KeyNoteDto {
            key: r.get(0)?,
            description: r.get(1)?,
            required_stages: stages.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
            is_secret: r.get::<_, i64>(3)? != 0,
            last_rotated: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn set_key_note(
    conn: &mut Connection,
    project_id: i64,
    key: &str,
    description: Option<String>,
    required_stages: Vec<String>,
    is_secret: bool,
    last_rotated: Option<String>,
) -> Result<()> {
    if !crate::core::parser::is_valid_key(key) {
        return Err(LokiError::Invalid("That is not a valid key name.".into()));
    }
    for s in &required_stages {
        validate_stage(s)?;
    }
    get_project(conn, project_id)?;
    let stages = serde_json::to_string(&required_stages).map_err(|_| LokiError::Invalid("Bad stages.".into()))?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO key_notes (project_id, key, description, required_stages, is_secret, last_rotated) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(project_id, key) DO UPDATE SET description = excluded.description, required_stages = excluded.required_stages, is_secret = excluded.is_secret, last_rotated = excluded.last_rotated",
        params![project_id, key, clean(description), stages, is_secret as i64, clean(last_rotated)],
    )?;
    audit(&tx, "key_note_set", &format!("project:{project_id}"), Some(key))?;
    tx.commit()?;
    Ok(())
}

pub fn notes_map(conn: &Connection, project_id: i64) -> Result<HashMap<String, KeyNote>> {
    Ok(list_key_notes(conn, project_id)?
        .into_iter()
        .map(|n| (n.key, KeyNote { required_stages: n.required_stages, is_secret: n.is_secret }))
        .collect())
}

/// Union of key names across the project's current snapshots of non-archived targets.
pub fn project_key_names(conn: &Connection, project_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT v.key FROM variables v JOIN snapshots s ON s.id = v.snapshot_id JOIN targets t ON t.id = s.target_id \
         WHERE t.project_id = ?1 AND t.is_archived = 0 AND s.id = (SELECT id FROM snapshots WHERE target_id = t.id ORDER BY version DESC LIMIT 1) ORDER BY v.key",
    )?;
    let rows = stmt.query_map([project_id], |r| r.get(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Fingerprints per key across the project's non-production targets (current snapshots).
pub fn non_production_fingerprints(conn: &Connection, project_id: i64) -> Result<HashMap<String, Vec<Vec<u8>>>> {
    let mut stmt = conn.prepare(
        "SELECT v.key, v.fingerprint FROM variables v JOIN snapshots s ON s.id = v.snapshot_id JOIN targets t ON t.id = s.target_id \
         WHERE t.project_id = ?1 AND t.stage != 'production' AND t.is_archived = 0 AND v.is_empty = 0 \
         AND s.id = (SELECT id FROM snapshots WHERE target_id = t.id ORDER BY version DESC LIMIT 1)",
    )?;
    let mut map: HashMap<String, Vec<Vec<u8>>> = HashMap::new();
    let mut rows = stmt.query([project_id])?;
    while let Some(r) = rows.next()? {
        map.entry(r.get(0)?).or_default().push(r.get(1)?);
    }
    Ok(map)
}

// ---------------------------------------------------------------- search

fn like_escape(q: &str) -> String {
    let mut s = String::from("%");
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

pub fn search_keys(conn: &Connection, query: &str) -> Result<Vec<SearchGroup>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT v.key, p.id, p.name, t.id, t.branch, t.stage, v.is_empty, v.fingerprint, v.id \
         FROM variables v JOIN snapshots s ON s.id = v.snapshot_id JOIN targets t ON t.id = s.target_id JOIN projects p ON p.id = t.project_id \
         WHERE t.is_archived = 0 AND v.key LIKE ?1 ESCAPE '\\' AND s.id = (SELECT id FROM snapshots WHERE target_id = t.id ORDER BY version DESC LIMIT 1) \
         ORDER BY v.key COLLATE NOCASE, p.name COLLATE NOCASE, t.branch LIMIT 500",
    )?;
    let mut rows = stmt.query([like_escape(q)])?;
    let mut groups: Vec<SearchGroup> = Vec::new();
    let mut fps: Vec<std::collections::HashSet<Vec<u8>>> = Vec::new();
    while let Some(r) = rows.next()? {
        let key: String = r.get(0)?;
        let fp: Vec<u8> = r.get(7)?;
        let is_empty = r.get::<_, i64>(6)? != 0;
        let hit = SearchHit {
            project_id: r.get(1)?,
            project_name: r.get(2)?,
            target_id: r.get(3)?,
            branch: r.get(4)?,
            stage: r.get(5)?,
            is_empty,
            fingerprint: to_hex(&fp[..fp.len().min(4)]),
            variable_id: r.get(8)?,
        };
        if groups.last().map(|g| g.key != key).unwrap_or(true) {
            groups.push(SearchGroup { key: key.clone(), hits: vec![], distinct_values: 0, consistent: true });
            fps.push(Default::default());
        }
        if !is_empty {
            fps.last_mut().map(|s| s.insert(fp));
        }
        if let Some(g) = groups.last_mut() {
            g.hits.push(hit);
        }
    }
    for (g, set) in groups.iter_mut().zip(fps) {
        g.distinct_values = set.len();
        g.consistent = set.len() <= 1 && !g.hits.iter().any(|h| h.is_empty);
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::{derive_keys, TEST_PARAMS};
    use crate::core::parser::parse;
    use crate::db::migrations::open_memory;

    pub fn test_keys() -> VaultKeys {
        derive_keys(b"fake_password_1", &[3u8; 16], TEST_PARAMS).unwrap()
    }

    fn setup() -> (Connection, VaultKeys, i64, i64) {
        let mut conn = open_memory();
        let keys = test_keys();
        let p = create_project(&mut conn, "QSeverse API", None, None).unwrap();
        let t = create_target(&mut conn, p.id, "main", "production", None, None).unwrap();
        (conn, keys, p.id, t.id)
    }

    #[test]
    fn projects_targets_crud_and_uniqueness() {
        let (mut conn, _k, pid, tid) = setup();
        assert_eq!(list_projects(&conn).unwrap().len(), 1);
        assert!(create_project(&mut conn, "QSeverse API", None, None).is_err());
        assert!(create_project(&mut conn, "  ", None, None).is_err());
        assert!(create_target(&mut conn, pid, "main", "production", None, None).is_err());
        assert!(create_target(&mut conn, pid, "main", "nonsense", None, None).is_err());
        create_target(&mut conn, pid, "staging", "staging", Some("Stage".into()), None).unwrap();
        let ts = list_targets(&conn, pid).unwrap();
        assert_eq!(ts[0].stage, "production"); // production sorts first
        assert_eq!(ts.len(), 2);
        set_archived(&mut conn, tid, true).unwrap();
        assert!(get_target(&conn, tid).unwrap().is_archived);
        delete_target(&mut conn, tid).unwrap();
        delete_project(&mut conn, pid).unwrap();
        assert!(list_projects(&conn).unwrap().is_empty());
        assert!(audit_list(&conn, 100).unwrap().len() >= 5);
    }

    #[test]
    fn snapshots_are_versioned_and_values_encrypted_at_rest() {
        let (mut conn, keys, _p, tid) = setup();
        let parsed = parse("# the key\nAPI_KEY=fake_value_1\nEMPTY=\n");
        let s1 = create_snapshot(&mut conn, &keys, tid, &parsed.entries, None, Some("C:\\Users\\Name\\My Projects\\app\\.env.production"), Some(&[1, 2, 3]), "import").unwrap();
        let s2 = create_snapshot(&mut conn, &keys, tid, &parsed.entries[..1], Some("second".into()), None, None, "import").unwrap();
        assert_eq!((s1.version, s2.version), (1, 2));
        assert_eq!(s1.key_count, 2);
        assert_eq!(current_snapshot_id(&conn, tid).unwrap(), Some(s2.id));

        // Nothing in the file reads back as plaintext.
        let raw: Vec<u8> = conn.query_row("SELECT value_ct FROM variables WHERE key = 'API_KEY' AND snapshot_id = ?1", [s1.id], |r| r.get(0)).unwrap();
        assert!(!raw.windows(12).any(|w| w == b"fake_value_1"));
        let comment: Vec<u8> = conn.query_row("SELECT comment FROM variables WHERE key = 'API_KEY' AND snapshot_id = ?1", [s1.id], |r| r.get(0)).unwrap();
        assert!(!comment.windows(7).any(|w| w == b"the key"));

        let back = load_entries(&conn, &keys, s1.id).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].value, "fake_value_1");
        assert_eq!(back[0].comment.as_deref(), Some("the key"));
        assert_eq!(back[1].value, "");
        let infos = list_keys(&conn, s1.id).unwrap();
        assert!(infos[1].is_empty && infos[0].has_comment);
        assert_eq!(infos[0].fingerprint.len(), 8);
    }

    #[test]
    fn reveal_and_wrong_key() {
        let (mut conn, keys, _p, tid) = setup();
        let s = create_snapshot(&mut conn, &keys, tid, &parse("A=fake_value_1\n").entries, None, None, None, "import").unwrap();
        let id = list_keys(&conn, s.id).unwrap()[0].variable_id;
        let r = reveal(&conn, &keys, id).unwrap();
        assert_eq!(r.value.as_str(), "fake_value_1");
        assert!(r.label.contains("QSeverse API") && r.label.contains("/ A"));
        assert!(!r.label.contains("fake_value_1"));
        let other = derive_keys(b"fake_password_2", &[3u8; 16], TEST_PARAMS).unwrap();
        assert!(reveal(&conn, &other, id).is_err());
        assert!(reveal(&conn, &keys, 9999).is_err());
    }

    #[test]
    fn ciphertext_cannot_be_swapped_between_rows() {
        let (mut conn, keys, _p, tid) = setup();
        let s = create_snapshot(&mut conn, &keys, tid, &parse("A=fake_value_1\nB=fake_value_2\n").entries, None, None, None, "import").unwrap();
        // Copy A's ciphertext over B's row: must fail to open because the AAD names the key.
        conn.execute("UPDATE variables SET value_ct = (SELECT value_ct FROM variables WHERE key = 'A' AND snapshot_id = ?1), value_nonce = (SELECT value_nonce FROM variables WHERE key = 'A' AND snapshot_id = ?1) WHERE key = 'B' AND snapshot_id = ?1", [s.id]).unwrap();
        assert!(load_entries(&conn, &keys, s.id).is_err());
    }

    #[test]
    fn restore_creates_a_new_snapshot_and_keeps_history() {
        let (mut conn, keys, _p, tid) = setup();
        let s1 = create_snapshot(&mut conn, &keys, tid, &parse("A=fake_value_1\nB=fake_value_2\n").entries, None, Some("x"), Some(&[9]), "import").unwrap();
        create_snapshot(&mut conn, &keys, tid, &parse("A=fake_value_9\n").entries, None, None, None, "import").unwrap();
        let s3 = restore_snapshot(&mut conn, &keys, s1.id, None).unwrap();
        assert_eq!(s3.version, 3);
        assert_eq!(list_snapshots(&conn, tid).unwrap().len(), 3);
        let e = load_entries(&conn, &keys, s3.id).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].value, "fake_value_1");
        assert!(s3.note.unwrap().contains("version 1"));
        assert_eq!(s3.source_path, None);
    }

    #[test]
    fn last_source_tracks_file_and_current_hash() {
        let (mut conn, keys, _p, tid) = setup();
        assert!(last_source(&conn, tid).unwrap().is_none());
        create_snapshot(&mut conn, &keys, tid, &parse("A=fake_value_1\n").entries, None, Some("C:\\a\\.env"), Some(&[7, 7]), "import").unwrap();
        let ls = last_source(&conn, tid).unwrap().unwrap();
        assert_eq!((ls.path.as_str(), ls.current_sha), ("C:\\a\\.env", Some(vec![7, 7])));
        // A later snapshot without a file keeps the path but has no current hash.
        create_snapshot(&mut conn, &keys, tid, &parse("A=fake_value_1\n").entries, None, None, None, "import").unwrap();
        let ls = last_source(&conn, tid).unwrap().unwrap();
        assert_eq!((ls.path.as_str(), ls.current_sha), ("C:\\a\\.env", None));
    }

    #[test]
    fn key_notes_and_project_keys() {
        let (mut conn, keys, pid, tid) = setup();
        create_snapshot(&mut conn, &keys, tid, &parse("B=fake_value_1\nA=fake_value_2\n").entries, None, None, None, "import").unwrap();
        assert_eq!(project_key_names(&conn, pid).unwrap(), ["A", "B"]);
        set_key_note(&mut conn, pid, "A", Some("what A is".into()), vec!["production".into()], false, None).unwrap();
        let n = list_key_notes(&conn, pid).unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!((n[0].required_stages.clone(), n[0].is_secret), (vec!["production".to_string()], false));
        set_key_note(&mut conn, pid, "A", None, vec![], true, None).unwrap(); // upsert
        assert!(list_key_notes(&conn, pid).unwrap()[0].is_secret);
        assert!(set_key_note(&mut conn, pid, "bad key", None, vec![], true, None).is_err());
        assert!(set_key_note(&mut conn, pid, "A", None, vec!["nope".into()], true, None).is_err());
    }

    #[test]
    fn search_groups_by_key_and_reports_consistency() {
        let (mut conn, keys, pid, prod) = setup();
        let stg = create_target(&mut conn, pid, "staging", "staging", None, None).unwrap().id;
        let p2 = create_project(&mut conn, "Web", None, None).unwrap();
        let web = create_target(&mut conn, p2.id, "main", "production", None, None).unwrap().id;
        for (t, text) in [(prod, "REDIS_URL=fake_value_1\n"), (stg, "REDIS_URL=fake_value_1\n"), (web, "REDIS_URL=fake_value_2\nOTHER=fake_value_3\n")] {
            create_snapshot(&mut conn, &keys, t, &parse(text).entries, None, None, None, "import").unwrap();
        }
        let g = search_keys(&conn, "redis").unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].hits.len(), 3);
        assert_eq!(g[0].distinct_values, 2);
        assert!(!g[0].consistent);
        let g = search_keys(&conn, "OTHER").unwrap();
        assert!(g[0].consistent);
        assert!(search_keys(&conn, "100%_").unwrap().is_empty()); // wildcard chars are escaped
        assert!(search_keys(&conn, "").unwrap().is_empty());
    }

    #[test]
    fn non_production_fingerprints_exclude_production_and_archived() {
        let (mut conn, keys, pid, prod) = setup();
        let stg = create_target(&mut conn, pid, "staging", "staging", None, None).unwrap().id;
        let dev = create_target(&mut conn, pid, "dev", "dev", None, None).unwrap().id;
        for t in [prod, stg, dev] {
            create_snapshot(&mut conn, &keys, t, &parse("K=fake_value_1\n").entries, None, None, None, "import").unwrap();
        }
        assert_eq!(non_production_fingerprints(&conn, pid).unwrap()["K"].len(), 2);
        set_archived(&mut conn, dev, true).unwrap();
        assert_eq!(non_production_fingerprints(&conn, pid).unwrap()["K"].len(), 1);
    }

    #[test]
    fn audit_never_contains_values() {
        let (mut conn, keys, _p, tid) = setup();
        let s = create_snapshot(&mut conn, &keys, tid, &parse("A=fake_secret_value_xyz\n").entries, None, None, None, "import").unwrap();
        let id = list_keys(&conn, s.id).unwrap()[0].variable_id;
        let r = reveal(&conn, &keys, id).unwrap();
        audit(&conn, "reveal", &format!("variable:{id}"), Some(&r.label)).unwrap();
        for a in audit_list(&conn, 100).unwrap() {
            let text = format!("{} {} {:?}", a.action, a.entity, a.detail);
            assert!(!text.contains("fake_secret_value_xyz"), "{text}");
        }
    }
}
