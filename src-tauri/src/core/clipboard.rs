//! Copy a secret and clear it again after a timeout.
//!
//! The value goes to the Windows clipboard from Rust and never reaches the frontend. It is marked
//! "exclude from clipboard history" and "exclude from cloud clipboard", so Win+V history and
//! clipboard sync do not keep it. The timer only clears the clipboard if it still holds OUR value.

use crate::error::{LokiError, Result};
use arboard::{Clipboard, SetExtWindows};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

static GENERATION: AtomicU64 = AtomicU64::new(0);
static CURRENT: Mutex<Option<[u8; 32]>> = Mutex::new(None);

fn hash(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

fn current() -> std::sync::MutexGuard<'static, Option<[u8; 32]>> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

fn clip_err<E>(_: E) -> LokiError {
    LokiError::Io("Could not use the clipboard.".into())
}

pub fn copy_secret(value: &str, clear_after: Duration) -> Result<()> {
    let mut cb = Clipboard::new().map_err(clip_err)?;
    cb.set().exclude_from_history().exclude_from_cloud().text(value.to_owned()).map_err(clip_err)?;
    *current() = Some(hash(value));
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::spawn(move || {
        std::thread::sleep(clear_after);
        if GENERATION.load(Ordering::SeqCst) == generation {
            clear_if_ours();
        }
    });
    Ok(())
}

/// Clears the clipboard only if it still contains the value LOKI copied.
pub fn clear_if_ours() {
    let ours = current().take();
    let Some(expected) = ours else { return };
    if let Ok(mut cb) = Clipboard::new() {
        match cb.get_text() {
            Ok(text) if hash(&text) == expected => {
                let _ = cb.clear();
            }
            _ => {}
        }
    }
}
