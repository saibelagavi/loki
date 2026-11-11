mod commands;
mod core;
mod db;
mod error;
#[cfg(test)]
mod leak_tests;

use crate::core::vault::VaultState;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewWindowBuilder, WindowEvent};

/// `%LOCALAPPDATA%\LOKI`. `LOKI_DATA_DIR` overrides it (handy for trying the app on a scratch vault).
fn data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LOKI_DATA_DIR") {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("LOKI"))
}

pub fn run() {
    #[cfg(debug_assertions)]
    let started = std::time::Instant::now();
    let Some(dir) = data_dir() else {
        eprintln!("LOKI cannot start: the LOCALAPPDATA folder is not set.");
        return;
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(VaultState::new(dir.clone()))
        .setup(move |app| {
            // Build the main window here so its WebView2 profile lives inside the LOKI data
            // folder. Nothing is written outside it (the window is created hidden; the frontend
            // shows it after the first paint).
            std::fs::create_dir_all(&dir)?;
            let config = app.config().app.windows.first().cloned().ok_or("no window configured")?;
            WebviewWindowBuilder::from_config(app.handle(), &config)?
                .data_directory(dir.join("webview"))
                .build()?;
            #[cfg(debug_assertions)]
            eprintln!("[loki dev] window created {} ms after start", started.elapsed().as_millis());

            // Auto-lock: once every couple of seconds, lock if idle for longer than the setting.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(2));
                if handle.state::<VaultState>().lock_if_idle() {
                    crate::core::clipboard::clear_if_ours();
                    let _ = handle.emit("vault-locked", "idle");
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Optional: lock when the window is minimized.
            if let WindowEvent::Resized(_) = event {
                if window.is_minimized().unwrap_or(false) {
                    let state = window.state::<VaultState>();
                    if state.lock_on_minimize_enabled() && state.lock() {
                        crate::core::clipboard::clear_if_ours();
                        let _ = window.emit("vault-locked", "minimize");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::vault_status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_lock,
            commands::vault_change_password,
            commands::vault_touch,
            commands::project_list,
            commands::project_create,
            commands::project_update,
            commands::project_delete,
            commands::target_list,
            commands::target_create,
            commands::target_update,
            commands::target_archive,
            commands::target_delete,
            commands::scan_folder,
            commands::import_preview,
            commands::import_commit,
            commands::snapshot_reimport,
            commands::snapshot_list,
            commands::snapshot_keys,
            commands::snapshot_diff,
            commands::snapshot_restore,
            commands::compare_matrix,
            commands::push_check,
            commands::push_check_export,
            commands::reveal_value,
            commands::copy_value,
            commands::export_env,
            commands::search_keys,
            commands::project_keys,
            commands::key_note_list,
            commands::key_note_set,
            commands::audit_list,
            commands::rules_catalog,
            commands::settings_get,
            commands::settings_set,
            commands::backup_export,
            commands::backup_import,
            commands::v2::target_catalog,
            commands::v2::clipboard_copy_text,
            commands::v2::value_diff,
            commands::v2::credential_list,
            commands::v2::credential_create,
            commands::v2::credential_update,
            commands::v2::credential_delete,
            commands::v2::credential_reveal,
            commands::v2::credential_copy,
            commands::v2::password_generate,
            commands::v2::share_generate_key,
            commands::v2::share_create,
            commands::v2::share_open,
            commands::v2::share_import,
            commands::v2::share_save_env,
            commands::v2::share_discard,
            commands::v2::overview,
            commands::v2::drift_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LOKI");
}
