//! Reading `.env` sources and building previews. A preview lists key names, counts and parser
//! warnings. It never includes a value.

use crate::core::parser::{self, Parsed};
use crate::error::{LokiError, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::Path;
use zeroize::Zeroizing;

pub const MAX_BYTES: u64 = 5 * 1024 * 1024;

pub struct SourceFile {
    pub text: Zeroizing<String>,
    pub sha256: [u8; 32],
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn read_env_file(path: &Path) -> Result<SourceFile> {
    let meta = std::fs::metadata(path).map_err(|e| LokiError::Io(format!("Cannot read `{}`: {e}", path.display())))?;
    if !meta.is_file() {
        return Err(LokiError::Invalid(format!("`{}` is not a file.", path.display())));
    }
    if meta.len() > MAX_BYTES {
        return Err(LokiError::Invalid("That file is larger than 5 MB, so it is probably not a .env file.".into()));
    }
    let bytes = Zeroizing::new(std::fs::read(path).map_err(|e| LokiError::Io(format!("Cannot read `{}`: {e}", path.display())))?);
    let sha = sha256(&bytes);
    let text = String::from_utf8(bytes.to_vec()).map_err(|_| LokiError::Invalid("That file is not valid UTF-8 text.".into()))?;
    Ok(SourceFile { text: Zeroizing::new(text), sha256: sha })
}

/// Where an import comes from: a file path or pasted text, exactly one of the two.
pub struct Source {
    pub parsed: Parsed,
    pub path: Option<String>,
    pub sha256: Option<[u8; 32]>,
}

pub fn resolve(path: Option<String>, text: Option<String>) -> Result<Source> {
    match (path, text) {
        (Some(p), None) => {
            let f = read_env_file(Path::new(&p))?;
            Ok(Source { parsed: parser::parse(&f.text), path: Some(p), sha256: Some(f.sha256) })
        }
        (None, Some(t)) => {
            let t = Zeroizing::new(t);
            if t.len() as u64 > MAX_BYTES {
                return Err(LokiError::Invalid("The pasted text is larger than 5 MB.".into()));
            }
            Ok(Source { parsed: parser::parse(&t), path: None, sha256: None })
        }
        _ => Err(LokiError::Invalid("Choose a file or paste text, not both.".into())),
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviewKey {
    pub key: String,
    pub is_empty: bool,
    pub has_comment: bool,
    pub line: usize,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviewWarning {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub keys: Vec<PreviewKey>,
    pub warnings: Vec<PreviewWarning>,
    pub key_count: usize,
    pub empty_count: usize,
    pub duplicate_count: usize,
    pub source_path: Option<String>,
}

pub fn preview(src: &Source) -> Preview {
    let keys: Vec<PreviewKey> = src
        .parsed
        .entries
        .iter()
        .map(|e| PreviewKey { key: e.key.clone(), is_empty: e.value.is_empty(), has_comment: e.comment.is_some(), line: e.line })
        .collect();
    Preview {
        key_count: keys.len(),
        empty_count: keys.iter().filter(|k| k.is_empty).count(),
        duplicate_count: src.parsed.duplicate_count,
        warnings: src.parsed.warnings.iter().map(|w| PreviewWarning { line: w.line, message: w.message.clone() }).collect(),
        source_path: src.path.clone(),
        keys,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("loki-test-{tag}-{}-{}", std::process::id(), crate::core::crypto::to_hex(&crate::core::crypto::random_bytes::<4>().unwrap())));
        p.push("My Projects");
        std::fs::create_dir_all(&p).unwrap();
        p.push(".env.production");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn preview_has_no_values() {
        let src = resolve(None, Some("A=fake_secret_value_xyz\nB=\n# c\nC=fake_secret_value_xyz\nA=dup_fake_value\nbroken\n".into())).unwrap();
        let p = preview(&src);
        assert_eq!((p.key_count, p.empty_count, p.duplicate_count), (3, 1, 1));
        assert_eq!(p.warnings.len(), 2);
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("fake_secret_value_xyz") && !json.contains("dup_fake_value"));
    }

    #[test]
    fn reads_file_with_spaces_in_path_and_hashes_bytes() {
        let path = temp_file("import", b"A=fake_value_1\r\n");
        let s = resolve(Some(path.to_string_lossy().to_string()), None).unwrap();
        assert_eq!(s.parsed.entries[0].key, "A");
        assert_eq!(s.sha256, Some(sha256(b"A=fake_value_1\r\n")));
        // Same bytes, same hash; different bytes, different hash.
        assert_ne!(sha256(b"A=fake_value_1\r\n"), sha256(b"A=fake_value_2\r\n"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn rejects_bad_sources() {
        assert!(resolve(None, None).is_err());
        assert!(resolve(Some("x".into()), Some("y".into())).is_err());
        assert!(resolve(Some("C:\\no\\such\\file.env".into()), None).is_err());
        let path = temp_file("utf8", &[0xff, 0xfe, 0x00, 0x41]);
        assert!(resolve(Some(path.to_string_lossy().to_string()), None).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
