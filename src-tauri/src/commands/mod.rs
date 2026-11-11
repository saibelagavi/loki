//! Thin `#[tauri::command]` functions. Logic lives in `core/` and `db/`.
//!
//! Rules enforced here:
//! - DTOs never carry a secret value. `reveal_value` is the only command that returns one.
//! - Reveal, copy, export and delete write an audit entry (names only, never values).
//! - Save and open dialogs for exports and backups are opened from Rust, so the frontend never
//!   chooses where a file is written.
//! - Heavy work runs on Tauri's thread pool (`async`), never on the UI thread.

use crate::core::backup::{self, ImportSummary};
use crate::core::crypto::KdfParams;
use crate::core::diff::{self, DiffEntry, MatrixRow};
use crate::core::export;
use crate::core::import::{self, Preview};
use crate::core::parser;
use crate::core::rules;
use crate::core::pushcheck;
use crate::core::scan::{self, ScanHit};
use crate::core::settings::Settings;
use crate::core::vault::VaultState;
use crate::core::clipboard;
use crate::db::models::*;
use crate::db::repo;
use crate::error::{LokiError, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

pub mod v2;

type St<'a> = State<'a, VaultState>;

fn gate(state: &VaultState) -> Result<()> {
    state.with_unlocked(|_, _, _| Ok(()))
}

// ------------------------------------------------------------------ app

#[derive(Serialize)]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { name: "LOKI", version: env!("CARGO_PKG_VERSION") }
}

// ------------------------------------------------------------------ vault

fn apply_capture_setting(window: &WebviewWindow, state: &VaultState) {
    if let Ok(s) = state.settings() {
        let _ = window.set_content_protected(s.exclude_from_capture);
    }
}

#[tauri::command(async)]
pub fn vault_status(state: St) -> Result<VaultStatus> {
    state.status()
}

#[tauri::command(async)]
pub fn vault_create(state: St, window: WebviewWindow, password: String) -> Result<()> {
    let password = Zeroizing::new(password);
    state.create(&password)?;
    apply_capture_setting(&window, &state);
    Ok(())
}

#[tauri::command(async)]
/// Returns the path of the safety copy if unlocking upgraded the vault's schema.
pub fn vault_unlock(state: St, window: WebviewWindow, password: String) -> Result<Option<String>> {
    let password = Zeroizing::new(password);
    let backup = state.unlock(&password)?;
    apply_capture_setting(&window, &state);
    Ok(backup.map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command(async)]
pub fn vault_lock(state: St) -> Result<()> {
    clipboard::clear_if_ours();
    state.lock();
    Ok(())
}

#[tauri::command(async)]
pub fn vault_change_password(state: St, old_password: String, new_password: String) -> Result<()> {
    let (old, new) = (Zeroizing::new(old_password), Zeroizing::new(new_password));
    state.change_password(&old, &new)
}

/// Called (throttled) by the UI on user input so the auto-lock timer counts real activity.
#[tauri::command]
pub fn vault_touch(state: St) {
    state.touch();
}

// ------------------------------------------------------------------ projects & targets

#[tauri::command(async)]
pub fn project_list(state: St) -> Result<Vec<ProjectDto>> {
    state.with_unlocked(|conn, _, _| repo::list_projects(conn))
}

#[tauri::command(async)]
pub fn project_create(state: St, name: String, root_path: Option<String>, description: Option<String>) -> Result<ProjectDto> {
    state.with_unlocked(|conn, _, _| repo::create_project(conn, &name, root_path, description))
}

#[tauri::command(async)]
pub fn project_update(state: St, id: i64, name: String, root_path: Option<String>, description: Option<String>) -> Result<ProjectDto> {
    state.with_unlocked(|conn, _, _| repo::update_project(conn, id, &name, root_path, description))
}

#[tauri::command(async)]
pub fn project_delete(state: St, id: i64) -> Result<()> {
    state.with_unlocked(|conn, _, _| repo::delete_project(conn, id))
}

#[tauri::command(async)]
pub fn target_list(state: St, project_id: i64) -> Result<Vec<TargetDto>> {
    state.with_unlocked(|conn, _, _| repo::list_targets(conn, project_id))
}

#[tauri::command(async)]
pub fn target_create(state: St, project_id: i64, branch: String, stage: String, label: Option<String>, color: Option<String>) -> Result<TargetDto> {
    state.with_unlocked(|conn, _, _| repo::create_target(conn, project_id, &branch, &stage, label, color))
}

#[tauri::command(async)]
pub fn target_update(state: St, id: i64, branch: String, stage: String, label: Option<String>, color: Option<String>) -> Result<TargetDto> {
    state.with_unlocked(|conn, _, _| repo::update_target(conn, id, &branch, &stage, label, color))
}

#[tauri::command(async)]
pub fn target_archive(state: St, id: i64, archived: bool) -> Result<TargetDto> {
    state.with_unlocked(|conn, _, _| repo::set_archived(conn, id, archived))
}

#[tauri::command(async)]
pub fn target_delete(state: St, id: i64) -> Result<()> {
    state.with_unlocked(|conn, _, _| repo::delete_target(conn, id))
}

// ------------------------------------------------------------------ scan & import

#[tauri::command(async)]
pub fn scan_folder(state: St, path: String) -> Result<Vec<ScanHit>> {
    gate(&state)?;
    scan::scan_folder(&PathBuf::from(path))
}

#[tauri::command(async)]
pub fn import_preview(state: St, path: Option<String>, text: Option<String>) -> Result<Preview> {
    gate(&state)?;
    let src = import::resolve(path, text)?;
    Ok(import::preview(&src))
}

#[tauri::command(async)]
pub fn import_commit(state: St, target_id: i64, path: Option<String>, text: Option<String>, note: Option<String>) -> Result<SnapshotDto> {
    state.with_unlocked(|conn, keys, _| {
        let src = import::resolve(path, text)?;
        if src.parsed.entries.is_empty() {
            return Err(LokiError::Invalid("Nothing to import: no KEY=value lines were found.".into()));
        }
        repo::create_snapshot(conn, keys, target_id, &src.parsed.entries, note, src.path.as_deref(), src.sha256.as_ref().map(|h| h.as_slice()), "import")
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReimportResult {
    pub unchanged: bool,
    pub source_path: String,
    pub snapshot: Option<SnapshotDto>,
}

#[tauri::command(async)]
pub fn snapshot_reimport(state: St, target_id: i64) -> Result<ReimportResult> {
    state.with_unlocked(|conn, keys, _| {
        let last = repo::last_source(conn, target_id)?
            .ok_or_else(|| LokiError::Invalid("This target has no source file to re-read yet. Import a file first.".into()))?;
        let file = import::read_env_file(std::path::Path::new(&last.path))?;
        if last.current_sha.as_deref() == Some(file.sha256.as_slice()) {
            return Ok(ReimportResult { unchanged: true, source_path: last.path, snapshot: None });
        }
        let parsed = parser::parse(&file.text);
        if parsed.entries.is_empty() {
            return Err(LokiError::Invalid("The file no longer contains any KEY=value lines, so nothing was imported.".into()));
        }
        let snap = repo::create_snapshot(conn, keys, target_id, &parsed.entries, Some("Re-imported from file".into()), Some(&last.path), Some(file.sha256.as_slice()), "reimport")?;
        Ok(ReimportResult { unchanged: false, source_path: last.path, snapshot: Some(snap) })
    })
}

// ------------------------------------------------------------------ snapshots

#[tauri::command(async)]
pub fn snapshot_list(state: St, target_id: i64) -> Result<Vec<SnapshotDto>> {
    state.with_unlocked(|conn, _, _| repo::list_snapshots(conn, target_id))
}

#[tauri::command(async)]
pub fn snapshot_keys(state: St, snapshot_id: i64) -> Result<Vec<KeyInfoDto>> {
    state.with_unlocked(|conn, _, _| repo::list_keys(conn, snapshot_id))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub entries: Vec<DiffEntry>,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

#[tauri::command(async)]
pub fn snapshot_diff(state: St, a_id: i64, b_id: i64) -> Result<DiffResult> {
    state.with_unlocked(|conn, _, _| {
        repo::get_snapshot(conn, a_id)?;
        repo::get_snapshot(conn, b_id)?;
        let entries = diff::snapshot_diff(&repo::load_meta(conn, a_id)?, &repo::load_meta(conn, b_id)?);
        let count = |c: diff::Change| entries.iter().filter(|e| e.change == c).count();
        Ok(DiffResult { added: count(diff::Change::Added), removed: count(diff::Change::Removed), changed: count(diff::Change::Changed), entries })
    })
}

#[tauri::command(async)]
pub fn snapshot_restore(state: St, snapshot_id: i64, note: Option<String>) -> Result<SnapshotDto> {
    state.with_unlocked(|conn, keys, _| repo::restore_snapshot(conn, keys, snapshot_id, note))
}

// ------------------------------------------------------------------ compare

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDto {
    pub target_id: i64,
    pub branch: String,
    pub stage: String,
    pub label: Option<String>,
    pub color: Option<String>,
    pub snapshot_id: Option<i64>,
    pub version: Option<i64>,
    pub key_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResult {
    pub columns: Vec<ColumnDto>,
    pub baseline_index: usize,
    pub rows: Vec<MatrixRow>,
}

#[tauri::command(async)]
pub fn compare_matrix(state: St, target_ids: Vec<i64>, baseline_id: i64) -> Result<CompareResult> {
    if target_ids.is_empty() || target_ids.len() > 12 {
        return Err(LokiError::Invalid("Compare between 1 and 12 targets.".into()));
    }
    let baseline_index = target_ids
        .iter()
        .position(|id| *id == baseline_id)
        .ok_or_else(|| LokiError::Invalid("Choose one of the compared targets as the baseline.".into()))?;
    state.with_unlocked(|conn, _, _| {
        let mut columns = Vec::new();
        let mut metas = Vec::new();
        for id in &target_ids {
            let t = repo::get_target(conn, *id)?;
            metas.push(match t.current_snapshot_id {
                Some(s) => repo::load_meta(conn, s)?,
                None => vec![],
            });
            columns.push(ColumnDto {
                target_id: t.id,
                branch: t.branch,
                stage: t.stage,
                label: t.label,
                color: t.color,
                snapshot_id: t.current_snapshot_id,
                version: t.current_version,
                key_count: t.key_count,
            });
        }
        Ok(CompareResult { columns, baseline_index, rows: diff::compare_matrix(&metas, baseline_index) })
    })
}

// ------------------------------------------------------------------ push-day check

#[tauri::command(async)]
pub fn push_check(state: St, from_target: i64, to_target: i64) -> Result<pushcheck::PushResult> {
    state.with_unlocked(|conn, keys, settings| pushcheck::run(conn, keys, settings, from_target, to_target))
}

fn save_path(app: &AppHandle, window: &WebviewWindow, file_name: &str, filter: Option<(&str, &[&str])>) -> Option<PathBuf> {
    // End-to-end tests only: debug builds can skip the native dialog and save into a scratch
    // folder. Release builds are compiled without this, so users always choose in the dialog.
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("LOKI_TEST_SAVE_DIR") {
        return Some(PathBuf::from(dir).join(file_name));
    }
    let mut b = app.dialog().file().set_parent(window).set_file_name(file_name);
    if let Some((name, exts)) = filter {
        b = b.add_filter(name, exts);
    }
    b.blocking_save_file().and_then(|p| p.into_path().ok())
}

/// Exports the findings as a Markdown checklist (keys only, never values). `None` = cancelled.
#[tauri::command(async)]
pub fn push_check_export(app: AppHandle, window: WebviewWindow, state: St, from_target: i64, to_target: i64) -> Result<Option<String>> {
    let result = state.with_unlocked(|conn, keys, settings| pushcheck::run(conn, keys, settings, from_target, to_target))?;
    let Some(path) = save_path(&app, &window, "push-check.md", Some(("Markdown", &["md"]))) else { return Ok(None) };
    let md = rules::to_markdown(&result.from_label, &result.to_label, &result.findings);
    std::fs::write(&path, md)?;
    state.with_unlocked(|conn, _, _| repo::audit(conn, "export", "push_check", Some(&format!("{} to {}: findings checklist (keys only)", result.from_label, result.to_label))))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

// ------------------------------------------------------------------ reveal, copy, export

#[tauri::command(async)]
pub fn reveal_value(state: St, variable_id: i64) -> Result<String> {
    state.with_unlocked(|conn, keys, _| {
        let r = repo::reveal(conn, keys, variable_id)?;
        // If the audit entry cannot be written, the value is dropped and the reveal fails.
        repo::audit(conn, "reveal", &format!("variable:{variable_id}"), Some(&r.label))?;
        Ok(r.value.to_string())
    })
}

/// Copies to the clipboard from Rust and clears it after the configured time. Returns that time.
#[tauri::command(async)]
pub fn copy_value(state: St, variable_id: i64) -> Result<u32> {
    state.with_unlocked(|conn, keys, settings| {
        let r = repo::reveal(conn, keys, variable_id)?;
        repo::audit(conn, "copy", &format!("variable:{variable_id}"), Some(&r.label))?;
        clipboard::copy_secret(&r.value, Duration::from_secs(u64::from(settings.clipboard_clear_seconds)))?;
        Ok(settings.clipboard_clear_seconds)
    })
}

/// `mode`: "full" | "example" | "missing_only". For "missing_only", `compare_snapshot_id` is the
/// destination snapshot: only keys that are absent or empty there are written.
/// Returns the path the user chose, or `None` if they cancelled the save dialog.
#[tauri::command(async)]
pub fn export_env(app: AppHandle, window: WebviewWindow, state: St, snapshot_id: i64, mode: String, compare_snapshot_id: Option<i64>) -> Result<Option<String>> {
    let parsed_mode = export::Mode::parse(&mode).ok_or_else(|| LokiError::Invalid("Export mode must be full, example or missing_only.".into()))?;
    if parsed_mode == export::Mode::MissingOnly && compare_snapshot_id.is_none() {
        return Err(LokiError::Invalid("Choose the target to compare against for a missing-only export.".into()));
    }
    let (default_name, text, label) = state.with_unlocked(|conn, keys, _| {
        let snap = repo::get_snapshot(conn, snapshot_id)?;
        let target = repo::get_target(conn, snap.target_id)?;
        let entries = repo::load_entries(conn, keys, snapshot_id)?;
        let present: HashSet<String> = match compare_snapshot_id {
            Some(c) => repo::load_meta(conn, c)?.into_iter().filter(|m| !m.is_empty).map(|m| m.key).collect(),
            None => HashSet::new(),
        };
        let text = export::render(parsed_mode, &entries, &present);
        Ok((parsed_mode.default_file_name(&target.stage), text, format!("{} / {} v{}", target.branch, target.stage, snap.version)))
    })?;
    let Some(path) = save_path(&app, &window, &default_name, None) else { return Ok(None) };
    std::fs::write(&path, text.as_bytes())?;
    state.with_unlocked(|conn, _, _| repo::audit(conn, "export", &format!("snapshot:{snapshot_id}"), Some(&format!("{label}: {mode}"))))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

// ------------------------------------------------------------------ search, notes, audit

#[tauri::command(async)]
pub fn search_keys(state: St, query: String) -> Result<Vec<SearchGroup>> {
    state.with_unlocked(|conn, _, _| repo::search_keys(conn, &query))
}

#[tauri::command(async)]
pub fn project_keys(state: St, project_id: i64) -> Result<Vec<String>> {
    state.with_unlocked(|conn, _, _| repo::project_key_names(conn, project_id))
}

#[tauri::command(async)]
pub fn key_note_list(state: St, project_id: i64) -> Result<Vec<KeyNoteDto>> {
    state.with_unlocked(|conn, _, _| repo::list_key_notes(conn, project_id))
}

#[tauri::command(async)]
pub fn key_note_set(
    state: St,
    project_id: i64,
    key: String,
    description: Option<String>,
    required_stages: Vec<String>,
    is_secret: bool,
    last_rotated: Option<String>,
) -> Result<()> {
    state.with_unlocked(|conn, _, _| repo::set_key_note(conn, project_id, &key, description, required_stages, is_secret, last_rotated))
}

#[tauri::command(async)]
pub fn audit_list(state: St, limit: Option<i64>) -> Result<Vec<AuditDto>> {
    state.with_unlocked(|conn, _, _| repo::audit_list(conn, limit.unwrap_or(200)))
}

// ------------------------------------------------------------------ settings & rules

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInfo {
    pub id: &'static str,
    pub severity: rules::Severity,
    pub title: &'static str,
    pub description: &'static str,
}

#[tauri::command]
pub fn rules_catalog() -> Vec<RuleInfo> {
    rules::RULES.iter().map(|r| RuleInfo { id: r.id, severity: r.severity, title: r.title, description: r.description }).collect()
}

#[tauri::command(async)]
pub fn settings_get(state: St) -> Result<Settings> {
    state.settings()
}

#[tauri::command(async)]
pub fn settings_set(state: St, window: WebviewWindow, settings: Settings) -> Result<Settings> {
    let saved = state.update_settings(settings)?;
    let _ = window.set_content_protected(saved.exclude_from_capture);
    Ok(saved)
}

// ------------------------------------------------------------------ backup

/// Writes an encrypted `.loki` backup protected by its own password. `None` = cancelled.
#[tauri::command(async)]
pub fn backup_export(app: AppHandle, window: WebviewWindow, state: St, backup_password: String) -> Result<Option<String>> {
    let password = Zeroizing::new(backup_password);
    if password.chars().count() < crate::core::vault::MIN_PASSWORD_LEN {
        return Err(LokiError::Invalid("Use at least 8 characters for the backup password.".into()));
    }
    let Some(path) = save_path(&app, &window, "loki-backup.loki", Some(("LOKI backup", &["loki"]))) else { return Ok(None) };
    let file = state.with_unlocked(|conn, keys, _| {
        let json = backup::export_json(conn, keys)?;
        backup::encrypt(&json, &password, KdfParams::DEFAULT)
    })?;
    std::fs::write(&path, file)?;
    state.with_unlocked(|conn, _, _| repo::audit(conn, "backup_export", "vault", None))?;
    Ok(Some(path.to_string_lossy().to_string()))
}

fn open_path(app: &AppHandle, window: &WebviewWindow, filter: (&str, &[&str])) -> Option<PathBuf> {
    // End-to-end tests only (debug builds): pick this file instead of showing the dialog.
    #[cfg(debug_assertions)]
    if let Some(p) = std::env::var_os("LOKI_TEST_OPEN_FILE") {
        return Some(PathBuf::from(p));
    }
    app.dialog().file().set_parent(window).add_filter(filter.0, filter.1).blocking_pick_file().and_then(|p| p.into_path().ok())
}

/// Adds the projects in a `.loki` backup to this vault (never overwrites). `None` = cancelled.
#[tauri::command(async)]
pub fn backup_import(app: AppHandle, window: WebviewWindow, state: St, backup_password: String) -> Result<Option<ImportSummary>> {
    let password = Zeroizing::new(backup_password);
    gate(&state)?;
    let Some(path) = open_path(&app, &window, ("LOKI backup", &["loki"])) else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|e| LokiError::Io(format!("Cannot read `{}`: {e}", path.display())))?;
    let json = backup::decrypt(&bytes, &password)?;
    let summary = state.with_unlocked(|conn, keys, _| backup::import_json(conn, keys, &json))?;
    Ok(Some(summary))
}
