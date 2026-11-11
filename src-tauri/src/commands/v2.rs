//! Commands added in v2: diff checker, passwords, secure share, overview and drift.
//! Same rules as the rest of `commands/`: thin, audited where values leave the core, and the
//! frontend never picks where a file is written.

use super::{gate, save_path, St};
use crate::core::clipboard;
use crate::core::crypto::{KdfParams, VaultKeys};
use crate::core::{credentials, import, overview, parser, share, valuediff};
use crate::db::models::{ProjectDto, SnapshotDto, TargetDto};
use crate::db::repo;
use crate::error::{LokiError, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use tauri::{AppHandle, WebviewWindow};
use zeroize::Zeroizing;

// ------------------------------------------------------------------ shared helpers

fn now(conn: &Connection) -> Result<String> {
    Ok(conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now')", [], |r| r.get(0))?)
}

fn snapshot_label(conn: &Connection, snapshot_id: i64) -> Result<String> {
    let snap = repo::get_snapshot(conn, snapshot_id)?;
    let t = repo::get_target(conn, snap.target_id)?;
    let p = repo::get_project(conn, t.project_id)?;
    Ok(format!("{} / {} / {} v{}", p.name, t.branch, t.stage, snap.version))
}

/// Projects with their targets, for pickers (diff checker, share).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub project: ProjectDto,
    pub targets: Vec<TargetDto>,
}

#[tauri::command(async)]
pub fn target_catalog(state: St) -> Result<Vec<CatalogEntry>> {
    state.with_unlocked(|conn, _, _| {
        repo::list_projects(conn)?
            .into_iter()
            .map(|p| Ok(CatalogEntry { targets: repo::list_targets(conn, p.id)?, project: p }))
            .collect()
    })
}

/// Copies non-vault text (a generated password or a share pass key) to the clipboard the same
/// secure way as values: excluded from clipboard history, cleared after the timeout.
#[tauri::command(async)]
pub fn clipboard_copy_text(state: St, text: String) -> Result<u32> {
    let text = Zeroizing::new(text);
    let seconds = state.settings()?.clipboard_clear_seconds;
    clipboard::copy_secret(&text, Duration::from_secs(u64::from(seconds)))?;
    Ok(seconds)
}

// ------------------------------------------------------------------ diff checker (values)

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum DiffSource {
    Snapshot { snapshot_id: i64 },
    Target { target_id: i64 },
    File { path: String },
    Text { text: String },
}

impl Drop for DiffSource {
    fn drop(&mut self) {
        if let DiffSource::Text { text } = self {
            zeroize::Zeroize::zeroize(text);
        }
    }
}

struct Resolved {
    entries: Vec<parser::Entry>,
    label: String,
    warnings: usize,
}

fn resolve_source(conn: &Connection, keys: &VaultKeys, src: &DiffSource) -> Result<Resolved> {
    match src {
        DiffSource::Snapshot { snapshot_id } => Ok(Resolved {
            entries: repo::load_entries(conn, keys, *snapshot_id)?,
            label: snapshot_label(conn, *snapshot_id)?,
            warnings: 0,
        }),
        DiffSource::Target { target_id } => {
            let t = repo::get_target(conn, *target_id)?;
            match t.current_snapshot_id {
                Some(sid) => Ok(Resolved { entries: repo::load_entries(conn, keys, sid)?, label: snapshot_label(conn, sid)?, warnings: 0 }),
                None => {
                    let p = repo::get_project(conn, t.project_id)?;
                    Ok(Resolved { entries: vec![], label: format!("{} / {} / {} (nothing imported yet)", p.name, t.branch, t.stage), warnings: 0 })
                }
            }
        }
        DiffSource::File { path } => {
            let f = import::read_env_file(std::path::Path::new(path))?;
            let parsed = parser::parse(&f.text);
            Ok(Resolved { warnings: parsed.warnings.len(), entries: parsed.entries, label: path.clone() })
        }
        DiffSource::Text { text } => {
            let parsed = parser::parse(text);
            Ok(Resolved { warnings: parsed.warnings.len(), entries: parsed.entries, label: "pasted text".into() })
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueDiffResult {
    pub left_label: String,
    pub right_label: String,
    pub left_warnings: usize,
    pub right_warnings: usize,
    pub rows: Vec<valuediff::ValueRow>,
    pub counts: valuediff::Counts,
    /// How long the UI should keep revealed values on screen.
    pub reveal_seconds: u32,
}

/// Compares two sources. With no reveal flags the result has statuses only (no values).
/// `reveal_all` or `reveal_keys` include values for those keys, and that is audited.
#[tauri::command(async)]
pub fn value_diff(state: St, left: DiffSource, right: DiffSource, reveal_all: bool, reveal_keys: Vec<String>) -> Result<ValueDiffResult> {
    state.with_unlocked(|conn, keys, settings| {
        let l = resolve_source(conn, keys, &left)?;
        let r = resolve_source(conn, keys, &right)?;
        let wanted: HashSet<String> = reveal_keys.into_iter().collect();
        let reveal = if reveal_all {
            valuediff::Reveal::All
        } else if wanted.is_empty() {
            valuediff::Reveal::None
        } else {
            valuediff::Reveal::Keys(&wanted)
        };
        let revealing = !matches!(reveal, valuediff::Reveal::None);
        let (rows, counts) = valuediff::compare(keys, &l.entries, &r.entries, reveal)?;
        if revealing {
            let what = if reveal_all {
                "all values".to_string()
            } else {
                let mut names: Vec<&str> = wanted.iter().map(|s| s.as_str()).collect();
                names.sort_unstable();
                format!("values of {}", names.join(", "))
            };
            repo::audit(conn, "diff_reveal", "diff", Some(&format!("{} vs {}: {what}", l.label, r.label)))?;
        }
        Ok(ValueDiffResult {
            left_label: l.label,
            right_label: r.label,
            left_warnings: l.warnings,
            right_warnings: r.warnings,
            rows,
            counts,
            reveal_seconds: settings.diff_reveal_seconds,
        })
    })
}

// ------------------------------------------------------------------ passwords

#[tauri::command(async)]
pub fn credential_list(state: St) -> Result<Vec<credentials::CredentialDto>> {
    state.with_unlocked(|conn, keys, _| credentials::list(conn, keys))
}

#[tauri::command(async)]
pub fn credential_create(state: St, input: credentials::CredentialInput) -> Result<credentials::CredentialDto> {
    state.with_unlocked(|conn, keys, _| credentials::create(conn, keys, &input))
}

#[tauri::command(async)]
pub fn credential_update(state: St, id: i64, input: credentials::CredentialInput) -> Result<credentials::CredentialDto> {
    state.with_unlocked(|conn, keys, _| credentials::update(conn, keys, id, &input))
}

#[tauri::command(async)]
pub fn credential_delete(state: St, id: i64) -> Result<()> {
    state.with_unlocked(|conn, keys, _| credentials::delete(conn, keys, id))
}

/// `field`: "password" | "notes". Audited.
#[tauri::command(async)]
pub fn credential_reveal(state: St, id: i64, field: String) -> Result<String> {
    let field = credentials::Field::parse(&field)?;
    state.with_unlocked(|conn, keys, _| Ok(credentials::reveal(conn, keys, id, field, "credential_reveal")?.to_string()))
}

/// Copies from Rust (never through the UI) and clears after the timeout. Audited.
#[tauri::command(async)]
pub fn credential_copy(state: St, id: i64, field: String) -> Result<u32> {
    let field = credentials::Field::parse(&field)?;
    state.with_unlocked(|conn, keys, settings| {
        let v = credentials::reveal(conn, keys, id, field, "credential_copy")?;
        clipboard::copy_secret(&v, Duration::from_secs(u64::from(settings.clipboard_clear_seconds)))?;
        Ok(settings.clipboard_clear_seconds)
    })
}

#[tauri::command(async)]
pub fn password_generate(state: St, length: u32, symbols: bool) -> Result<String> {
    gate(&state)?;
    Ok(credentials::generate_password(length as usize, symbols)?.to_string())
}

// ------------------------------------------------------------------ secure share

/// A fresh LOKI-generated pass key, shown to the sender before the file is created.
/// It is not stored anywhere; the frontend passes it back to `share_create`.
#[tauri::command(async)]
pub fn share_generate_key(state: St) -> Result<String> {
    gate(&state)?;
    Ok(share::generate_pass_key()?.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSelection {
    pub snapshot_id: i64,
    /// `None` = every key in that version.
    pub keys: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCreated {
    pub path: String,
    pub env_count: usize,
    pub key_count: usize,
    pub expires_at: Option<String>,
}

fn file_safe(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    out.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// Encrypts one or more `.env` versions (optionally only some keys of each) into one
/// `.lokishare` file under a pass key from `share_generate_key`.
#[tauri::command(async)]
pub fn share_create(
    app: AppHandle,
    window: WebviewWindow,
    state: St,
    selections: Vec<ShareSelection>,
    pass_key: String,
    expires_days: Option<u32>,
    note: Option<String>,
) -> Result<Option<ShareCreated>> {
    let pass_key = Zeroizing::new(pass_key);
    if !share::is_pass_key(&pass_key) {
        return Err(LokiError::Invalid("Use the pass key LOKI generated for this share.".into()));
    }
    if selections.is_empty() {
        return Err(LokiError::Invalid("Choose at least one .env to share.".into()));
    }
    if selections.len() > share::MAX_ITEMS {
        return Err(LokiError::Invalid(format!("Share at most {} .env files at once.", share::MAX_ITEMS)));
    }
    if let Some(d) = expires_days {
        if !(1..=365).contains(&d) {
            return Err(LokiError::Invalid("Expiry must be between 1 and 365 days.".into()));
        }
    }
    let (payload, default_name) = state.with_unlocked(|conn, vault_keys, _| {
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut projects: Vec<String> = Vec::new();
        for sel in &selections {
            if !seen.insert(sel.snapshot_id) {
                continue;
            }
            let snap = repo::get_snapshot(conn, sel.snapshot_id)?;
            let t = repo::get_target(conn, snap.target_id)?;
            let p = repo::get_project(conn, t.project_id)?;
            let only: Option<HashSet<String>> = sel.keys.clone().map(|k| k.into_iter().collect());
            let vars = share::from_entries(&repo::load_entries(conn, vault_keys, sel.snapshot_id)?, only.as_ref());
            if vars.is_empty() {
                return Err(LokiError::Invalid(format!("Choose at least one key in {} / {} / {}.", p.name, t.branch, t.stage)));
            }
            if !projects.contains(&p.name) {
                projects.push(p.name.clone());
            }
            items.push(share::ShareItem {
                label: format!("{} / {} / {} v{}", p.name, t.branch, t.stage, snap.version),
                project: Some(p.name),
                branch: Some(t.branch),
                stage: Some(t.stage),
                entries: vars,
            });
        }
        let expires_at: Option<String> = match expires_days {
            Some(d) => Some(conn.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now', ?1)", [format!("+{d} days")], |r| r.get(0))?),
            None => None,
        };
        let name = match (projects.as_slice(), items.as_slice()) {
            ([one], [only]) => format!("{}-{}.lokishare", file_safe(one), file_safe(only.stage.as_deref().unwrap_or("env"))),
            ([one], _) => format!("{}.lokishare", file_safe(one)),
            _ => format!("loki-share-{}-env-files.lokishare", items.len()),
        };
        let note = note.clone().map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        Ok((share::SharePayload::new(now(conn)?, expires_at, note, items), name))
    })?;
    let Some(path) = save_path(&app, &window, &default_name, Some(("LOKI share", &["lokishare"]))) else { return Ok(None) };
    let file = share::seal(&payload, &pass_key, KdfParams::DEFAULT)?;
    std::fs::write(&path, file)?;
    let (env_count, key_count) = (payload.items.len(), payload.key_count());
    state.with_unlocked(|conn, _, _| {
        let labels: Vec<&str> = payload.items.iter().map(|i| i.label.as_str()).collect();
        let expiry = payload.expires_at.as_deref().map(|e| format!(", expires {e}")).unwrap_or_default();
        repo::audit(conn, "share_create", "share", Some(&format!("{env_count} .env ({}), {key_count} keys{expiry}", labels.join("; "))))
    })?;
    Ok(Some(ShareCreated { path: path.to_string_lossy().to_string(), env_count, key_count, expires_at: payload.expires_at.clone() }))
}

/// Opens a `.lokishare` with its pass key. Returns names and dates only; values stay in the
/// core until the user imports or saves them.
#[tauri::command(async)]
pub fn share_open(state: St, path: String, pass_key: String) -> Result<share::SharePreview> {
    let pass_key = Zeroizing::new(pass_key);
    gate(&state)?;
    let meta = std::fs::metadata(&path).map_err(|e| LokiError::Io(format!("Cannot read `{path}`: {e}")))?;
    if meta.len() > 10 * 1024 * 1024 {
        return Err(LokiError::Invalid("That file is too large to be a LOKI share.".into()));
    }
    let bytes = std::fs::read(&path).map_err(|e| LokiError::Io(format!("Cannot read `{path}`: {e}")))?;
    let pending = share::open(&bytes, &pass_key)?;
    let current = state.with_unlocked(|conn, _, _| {
        repo::audit(conn, "share_open", "share", Some(&format!("{} .env, {} keys", pending.payload.items.len(), pending.payload.key_count())))?;
        now(conn)
    })?;
    let preview = share::preview(&pending, &current);
    state.set_pending_share(pending)?;
    Ok(preview)
}

fn not_expired(conn: &Connection, s: &share::PendingShare) -> Result<()> {
    if share::is_expired(&s.payload, &now(conn)?) {
        let when = s.payload.expires_at.clone().unwrap_or_default();
        return Err(LokiError::Invalid(format!("This share expired on {when}. Ask the sender for a new one.")));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareMapping {
    /// Which `.env` in the share.
    pub index: usize,
    /// Where it goes. `None` = the project and target named in the share (created if missing).
    pub target_id: Option<i64>,
}

/// The project and target a shared `.env` came from, created in this vault if they do not exist.
fn ensure_target(conn: &mut Connection, item: &share::ShareItem) -> Result<i64> {
    let (Some(project), Some(branch), Some(stage)) = (&item.project, &item.branch, &item.stage) else {
        return Err(LokiError::Invalid("This .env came from an older share without project details. Pick a target for it.".into()));
    };
    let pid = match repo::list_projects(conn)?.into_iter().find(|p| p.name == *project) {
        Some(p) => p.id,
        None => repo::create_project(conn, project, None, Some("Created from a LOKI share".into()))?.id,
    };
    match repo::list_targets(conn, pid)?.into_iter().find(|t| t.branch == *branch && t.stage == *stage) {
        Some(t) => Ok(t.id),
        None => Ok(repo::create_target(conn, pid, branch, stage, None, None)?.id),
    }
}

/// Imports the chosen `.env` files from an open share, each as a new version of its target.
/// Everything is checked before anything is written, so a bad choice imports nothing.
#[tauri::command(async)]
pub fn share_import(state: St, token: String, mappings: Vec<ShareMapping>, note: Option<String>) -> Result<Vec<SnapshotDto>> {
    if mappings.is_empty() {
        return Err(LokiError::Invalid("Choose where at least one .env goes.".into()));
    }
    let snaps = state.with_pending_share(&token, |conn, keys, s| {
        not_expired(conn, s)?;
        let mut seen = HashSet::new();
        for m in &mappings {
            let item = s.payload.items.get(m.index).ok_or_else(|| LokiError::Invalid("That .env is not in this share.".into()))?;
            if !seen.insert(m.index) {
                return Err(LokiError::Invalid("Each .env can be imported once.".into()));
            }
            match m.target_id {
                Some(id) => {
                    repo::get_target(conn, id)?;
                }
                None if item.project.is_none() || item.branch.is_none() || item.stage.is_none() => {
                    return Err(LokiError::Invalid(format!("Pick a target for {}: it came from an older share without project details.", item.label)));
                }
                None => {}
            }
        }
        let mut out = Vec::new();
        for m in &mappings {
            let item = &s.payload.items[m.index];
            let target_id = match m.target_id {
                Some(id) => id,
                None => ensure_target(conn, item)?,
            };
            let note = note.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Imported from a share: {}", item.label));
            out.push(repo::create_snapshot(conn, keys, target_id, &share::entries(item), Some(note), None, None, "share_import")?);
        }
        Ok(out)
    })?;
    state.discard_pending_share();
    Ok(snaps)
}

/// Saves one `.env` from an open share through a save dialog. The share stays open, so the
/// other `.env` files can still be saved or imported.
#[tauri::command(async)]
pub fn share_save_env(app: AppHandle, window: WebviewWindow, state: St, token: String, index: usize) -> Result<Option<String>> {
    let (text, label, name) = state.with_pending_share(&token, |conn, _, s| {
        not_expired(conn, s)?;
        let item = s.payload.items.get(index).ok_or_else(|| LokiError::Invalid("That .env is not in this share.".into()))?;
        let name = item.stage.as_deref().map(|st| format!(".env.{st}")).unwrap_or_else(|| ".env".to_string());
        Ok((Zeroizing::new(parser::serialize(&share::entries(item))), item.label.clone(), name))
    })?;
    let Some(path) = save_path(&app, &window, &name, None) else { return Ok(None) };
    std::fs::write(&path, text.as_bytes())?;
    state.with_unlocked(|conn, _, _| repo::audit(conn, "share_save", "share", Some(&format!("{label}: saved as a .env file"))))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command(async)]
pub fn share_discard(state: St) {
    state.discard_pending_share();
}

// ------------------------------------------------------------------ overview

#[tauri::command(async)]
pub fn overview(state: St) -> Result<overview::Overview> {
    state.with_unlocked(|conn, keys, settings| overview::build(conn, keys, settings))
}

/// Re-reads every target's source file and compares hashes. Explicit user action only.
#[tauri::command(async)]
pub fn drift_check(state: St) -> Result<Vec<overview::DriftRow>> {
    state.with_unlocked(|conn, _, _| overview::drift(conn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_source_json_shapes() {
        let s: DiffSource = serde_json::from_str(r#"{"kind":"snapshot","snapshotId":3}"#).unwrap();
        assert!(matches!(s, DiffSource::Snapshot { snapshot_id: 3 }));
        let s: DiffSource = serde_json::from_str(r#"{"kind":"target","targetId":4}"#).unwrap();
        assert!(matches!(s, DiffSource::Target { target_id: 4 }));
        let s: DiffSource = serde_json::from_str(r#"{"kind":"file","path":"C:\\a b\\.env"}"#).unwrap();
        assert!(matches!(&s, DiffSource::File { path } if path == "C:\\a b\\.env"));
        let s: DiffSource = serde_json::from_str(r#"{"kind":"text","text":"A=1"}"#).unwrap();
        assert!(matches!(&s, DiffSource::Text { .. }));
    }

    #[test]
    fn ensure_target_creates_the_matching_project_and_target_once() {
        let mut conn = crate::db::migrations::open_memory();
        let item = |stage: &str| share::ShareItem {
            label: format!("Payments API / main / {stage} v1"),
            project: Some("Payments API".into()),
            branch: Some("main".into()),
            stage: Some(stage.into()),
            entries: vec![],
        };
        let prod = ensure_target(&mut conn, &item("production")).unwrap();
        assert_eq!(ensure_target(&mut conn, &item("production")).unwrap(), prod, "reused, not duplicated");
        let stg = ensure_target(&mut conn, &item("staging")).unwrap();
        assert_ne!(prod, stg);
        let projects = repo::list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 1, "one project for both .env files");
        assert_eq!(repo::list_targets(&conn, projects[0].id).unwrap().len(), 2);
        // An existing project with the same name is used, not copied.
        let old = share::ShareItem { label: "old".into(), project: None, branch: None, stage: None, entries: vec![] };
        assert!(ensure_target(&mut conn, &old).is_err(), "old shares without names need a chosen target");
    }

    #[test]
    fn share_file_names_are_safe() {
        assert_eq!(file_safe("QSeverse API!"), "qseverse-api");
        assert_eq!(file_safe("../../evil"), "evil");
    }
}
