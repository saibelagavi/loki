//! Find `.env*` files in a chosen folder and guess stage and branch. No git binary, no network:
//! the branch comes from reading `.git/HEAD` directly.

use crate::error::{LokiError, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &["node_modules", ".git", "target", "dist", "venv", ".venv", "__pycache__", "build", ".next"];
const MAX_DEPTH: usize = 8;
const MAX_FILES: usize = 200;
const MAX_DIRS: usize = 5000;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScanHit {
    pub path: String,
    pub file_name: String,
    pub guessed_stage: String,
    pub guessed_branch: Option<String>,
}

pub fn is_env_file_name(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

pub fn guess_stage(file_name: &str) -> &'static str {
    let lower = file_name.to_ascii_lowercase();
    let suffix = lower.strip_prefix(".env").unwrap_or(&lower).trim_start_matches('.');
    if suffix.is_empty() {
        return "local";
    }
    for part in suffix.split(['.', '-', '_']) {
        match part {
            "production" | "prod" => return "production",
            "staging" | "stage" => return "staging",
            "development" | "dev" => return "dev",
            "local" => return "local",
            "preview" => return "preview",
            _ => {}
        }
    }
    "other"
}

/// Branch name from `.git/HEAD`, walking up from `start`. Handles `.git` as a file (worktrees).
pub fn guess_branch(start: &Path) -> Option<String> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let git = d.join(".git");
        if git.is_dir() {
            return read_head(&git.join("HEAD"));
        }
        if git.is_file() {
            let text = fs::read_to_string(&git).ok()?;
            let target = text.trim().strip_prefix("gitdir:")?.trim();
            let gitdir = if Path::new(target).is_absolute() { PathBuf::from(target) } else { d.join(target) };
            return read_head(&gitdir.join("HEAD"));
        }
        dir = d.parent();
    }
    None
}

fn read_head(head: &Path) -> Option<String> {
    let text = fs::read_to_string(head).ok()?;
    let text = text.trim();
    text.strip_prefix("ref:")
        .map(str::trim)
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .map(str::to_string)
}

pub fn scan_folder(root: &Path) -> Result<Vec<ScanHit>> {
    if !root.is_dir() {
        return Err(LokiError::Invalid(format!("`{}` is not a folder.", root.display())));
    }
    let mut hits = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut dirs_seen = 0;
    while let Some((dir, depth)) = stack.pop() {
        dirs_seen += 1;
        if dirs_seen > MAX_DIRS || hits.len() >= MAX_FILES {
            break;
        }
        let Ok(read) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let Ok(ft) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().to_string();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((entry.path(), depth + 1));
                }
            } else if ft.is_file() && is_env_file_name(&name) {
                let path = entry.path();
                hits.push(ScanHit {
                    guessed_stage: guess_stage(&name).to_string(),
                    guessed_branch: path.parent().and_then(guess_branch),
                    file_name: name,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }
    hits.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("loki-test-{tag}-{}-{}", std::process::id(), crate::core::crypto::to_hex(&crate::core::crypto::random_bytes::<4>().unwrap())));
        // A path with a space, as real Windows project folders often have.
        p.push("My Projects");
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn stage_guesses() {
        assert_eq!(guess_stage(".env"), "local");
        assert_eq!(guess_stage(".env.production"), "production");
        assert_eq!(guess_stage(".env.prod"), "production");
        assert_eq!(guess_stage(".env.staging"), "staging");
        assert_eq!(guess_stage(".env.development"), "dev");
        assert_eq!(guess_stage(".env.dev"), "dev");
        assert_eq!(guess_stage(".env.local"), "local");
        assert_eq!(guess_stage(".env.preview"), "preview");
        assert_eq!(guess_stage(".env.production.local"), "production");
        assert_eq!(guess_stage(".env.example"), "other");
        assert_eq!(guess_stage(".env.test"), "other");
    }

    #[test]
    fn env_file_names() {
        assert!(is_env_file_name(".env"));
        assert!(is_env_file_name(".env.production"));
        assert!(!is_env_file_name(".envrc"));
        assert!(!is_env_file_name("env"));
        assert!(!is_env_file_name("my.env"));
    }

    #[test]
    fn scan_finds_files_skips_junk_and_reads_branch() {
        let root = temp_dir("scan");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("HEAD"), "ref: refs/heads/feature/login\n").unwrap();
        fs::write(root.join(".env"), "A=fake_value_1\n").unwrap();
        fs::write(root.join(".env.production"), "A=fake_value_2\n").unwrap();
        fs::create_dir_all(root.join("node_modules").join("pkg")).unwrap();
        fs::write(root.join("node_modules").join("pkg").join(".env"), "A=fake_value_3\n").unwrap();
        fs::create_dir_all(root.join("apps").join("web")).unwrap();
        fs::write(root.join("apps").join("web").join(".env.staging"), "A=fake_value_4\n").unwrap();
        fs::write(root.join("README.md"), "x").unwrap();

        let hits = scan_folder(&root).unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.file_name.as_str()).collect();
        assert_eq!(hits.len(), 3, "{names:?}");
        assert!(!hits.iter().any(|h| h.path.contains("node_modules")));
        let prod = hits.iter().find(|h| h.file_name == ".env.production").unwrap();
        assert_eq!(prod.guessed_stage, "production");
        assert_eq!(prod.guessed_branch.as_deref(), Some("feature/login"));
        let staging = hits.iter().find(|h| h.file_name == ".env.staging").unwrap();
        assert_eq!(staging.guessed_stage, "staging");
        assert_eq!(staging.guessed_branch.as_deref(), Some("feature/login")); // found by walking up
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn detached_head_gives_no_branch_and_no_git_gives_none() {
        let root = temp_dir("head");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("HEAD"), "0123456789abcdef0123456789abcdef01234567\n").unwrap();
        assert_eq!(guess_branch(&root), None);
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn worktree_gitdir_file_is_followed() {
        let root = temp_dir("wt");
        let real = root.join("real-gitdir");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/hotfix\n").unwrap();
        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        fs::write(work.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        assert_eq!(guess_branch(&work).as_deref(), Some("hotfix"));
        let _ = fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn scanning_a_non_folder_errors() {
        assert!(scan_folder(Path::new("C:\\definitely\\not\\here")).is_err());
    }
}
