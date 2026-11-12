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

/// Explains a startup failure in a native message box: release builds have no console, and
/// without a WebView there is no window to show it in. The text never contains a value.
#[cfg(windows)]
fn startup_error(message: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    const MB_ICONERROR: u32 = 0x10;
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, caption) = (wide(message), wide("LOKI"));
    // SAFETY: both buffers are NUL-terminated UTF-16 and outlive the call; no owner window.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), MB_ICONERROR) };
}

#[cfg(not(windows))]
fn startup_error(_message: &str) {}

pub fn run() {
    #[cfg(debug_assertions)]
    let started = std::time::Instant::now();
    let Some(dir) = data_dir() else {
        eprintln!("LOKI cannot start: the LOCALAPPDATA folder is not set.");
        startup_error("LOKI cannot start: the LOCALAPPDATA folder is not set.");
        return;
    };
    // The Microsoft Store (MSIX) build cannot bundle the WebView2 installer the .exe/.msi carry.
    // Windows 11 and updated Windows 10 have it; anywhere else, say what to install.
    if tauri::webview_version().is_err() {
        startup_error(
            "LOKI needs the Microsoft Edge WebView2 Runtime, which is not installed on this PC.\n\n\
             Install \"WebView2 Runtime\" from Microsoft (free), then start LOKI again. \
             Your vault is untouched.",
        );
        return;
    }

    let result = tauri::Builder::default()
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
        .run(tauri::generate_context!());
    if let Err(e) = result {
        // Tauri startup errors name files and OS errors, never vault contents.
        startup_error(&format!("LOKI could not start.\n\n{e}"));
    }
}
