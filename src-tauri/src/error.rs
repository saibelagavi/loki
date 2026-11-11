//! Errors that NEVER contain secret values. Messages may name keys, never values.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum LokiError {
    #[error("The vault is locked. Unlock it first.")]
    Locked,
    #[error("Wrong password, or the vault file is damaged.")]
    WrongPassword,
    #[error("That pass key does not open this file. Check it was typed exactly as given.")]
    WrongPassKey,
    #[error("A vault already exists on this computer.")]
    VaultExists,
    #[error("No vault yet. Create one first.")]
    NoVault,
    #[error("{0}")]
    Invalid(String),
    #[error("{0} was not found.")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(String),
    // SQLite messages name tables and columns, never row values.
    #[error("Storage error: {0}")]
    Db(String),
    #[error("File error: {0}")]
    Io(String),
    #[error("Encryption error. The data could not be processed.")]
    Crypto,
}

pub type Result<T> = std::result::Result<T, LokiError>;

impl From<rusqlite::Error> for LokiError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::SqliteFailure(f, _)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                LokiError::Conflict("That name already exists here.".into())
            }
            other => LokiError::Db(other.to_string()),
        }
    }
}

impl From<std::io::Error> for LokiError {
    fn from(e: std::io::Error) -> Self {
        LokiError::Io(e.to_string())
    }
}

// Tauri commands return errors to the frontend as plain strings.
impl Serialize for LokiError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
