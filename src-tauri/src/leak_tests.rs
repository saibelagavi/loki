//! Leak tests (CLAUDE.md rule 3): a fake secret must never appear in a DTO, an error message,
//! the database file or any log output.
//!
//! LOKI has no logging crate on purpose, so "logs" cannot contain a value. The source scan below
//! keeps it that way: any print or log macro outside an allow-list fails the build.

use crate::core::crypto::TEST_PARAMS;
use crate::core::diff;
use crate::core::import;
use crate::core::parser;
use crate::core::rules::{self, CheckInput, CheckVar};
use crate::core::scan;
use crate::core::vault::VaultState;
use crate::db::repo;
use std::collections::{HashMap, HashSet};

const SECRET: &str = "fake_secret_value_xyz";
const COMMENT_SECRET: &str = "fake_comment_secret_abc";
const PASSWORD: &str = "fake_master_password_leak";

fn temp_state() -> VaultState {
    let mut p = std::env::temp_dir();
    p.push(format!("loki-leak-{}-{}", std::process::id(), crate::core::crypto::to_hex(&crate::core::crypto::random_bytes::<4>().unwrap())));
    VaultState::new(p)
}

fn assert_clean(label: &str, json: &str) {
    for needle in [SECRET, COMMENT_SECRET, PASSWORD] {
        assert!(!json.contains(needle), "{label} leaked `{needle}`: {json}");
    }
}

#[test]
fn no_dto_error_or_file_contains_the_secret() {
    let state = temp_state();
    state.create_with(PASSWORD, TEST_PARAMS).unwrap();

    let env_text = format!("# {COMMENT_SECRET}\nLEAK_KEY={SECRET}\nEMPTY=\nURL=http://localhost/{SECRET}\nbroken-line-{SECRET}\n");
    let mut dtos: Vec<(&str, String)> = Vec::new();
    let json = |v: &dyn erased::Ser| v.to_json();

    let (rows, diff_entries, findings, errors) = state
        .with_unlocked(|conn, keys, settings| {
            let p = repo::create_project(conn, "Leak project", None, None)?;
            let prod = repo::create_target(conn, p.id, "main", "production", None, None)?;
            let stg = repo::create_target(conn, p.id, "staging", "staging", None, None)?;

            // Import preview (no values) and the commit.
            let src = import::resolve(None, Some(env_text.clone()))?;
            dtos.push(("preview", json(&import::preview(&src))));
            let s1 = repo::create_snapshot(conn, keys, prod.id, &src.parsed.entries, None, None, None, "import")?;
            let s2 = repo::create_snapshot(conn, keys, stg.id, &parser::parse(&format!("LEAK_KEY={SECRET}\n")).entries, None, None, None, "import")?;
            let s3 = repo::create_snapshot(conn, keys, prod.id, &parser::parse("LEAK_KEY=other_fake\n").entries, None, None, None, "import")?;
            repo::set_key_note(conn, p.id, "LEAK_KEY", Some("what it is".into()), vec!["production".into()], true, None)?;

            dtos.push(("projects", json(&repo::list_projects(conn)?)));
            dtos.push(("targets", json(&repo::list_targets(conn, p.id)?)));
            dtos.push(("snapshots", json(&repo::list_snapshots(conn, prod.id)?)));
            dtos.push(("snapshot", json(&s1)));
            dtos.push(("keys", json(&repo::list_keys(conn, s1.id)?)));
            dtos.push(("notes", json(&repo::list_key_notes(conn, p.id)?)));
            dtos.push(("search", json(&repo::search_keys(conn, "LEAK")?)));
            dtos.push(("audit", json(&repo::audit_list(conn, 500)?)));
            dtos.push(("settings", json(settings)));

            // Reveal exists, and writes an audit entry that names the key but not the value.
            let var_id = repo::list_keys(conn, s2.id)?[0].variable_id;
            let revealed = repo::reveal(conn, keys, var_id)?;
            assert_eq!(revealed.value.as_str(), SECRET); // the one place a value legitimately comes out
            repo::audit(conn, "reveal", &format!("variable:{var_id}"), Some(&revealed.label))?;

            let m = diff::compare_matrix(&[repo::load_meta(conn, s3.id)?, repo::load_meta(conn, s2.id)?, repo::load_meta(conn, s1.id)?], 0);
            let d = diff::snapshot_diff(&repo::load_meta(conn, s1.id)?, &repo::load_meta(conn, s3.id)?);

            // Push check on real decrypted values: findings must still be value-free.
            let fps = |sid: i64| -> crate::error::Result<Vec<CheckVar>> {
                let metas: HashMap<String, Vec<u8>> = repo::load_meta(conn, sid)?.into_iter().map(|m| (m.key, m.fp)).collect();
                Ok(repo::load_entries(conn, keys, sid)?
                    .iter()
                    .map(|e| CheckVar { key: e.key.clone(), value: e.value.clone(), fp: metas.get(&e.key).cloned().unwrap_or_default(), is_empty: e.value.is_empty() })
                    .collect())
            };
            let (from, to) = (fps(s2.id)?, fps(s1.id)?);
            let notes = repo::notes_map(conn, p.id)?;
            let non_prod = repo::non_production_fingerprints(conn, p.id)?;
            let disabled = HashSet::new();
            let f = rules::run(&CheckInput { from: &from, to: &to, to_stage: "production", notes: &notes, non_prod_fps: &non_prod, disabled: &disabled, custom: &[] });
            assert!(!f.is_empty());

            // Error messages from failing operations.
            let errs: Vec<String> = vec![
                repo::reveal(conn, keys, 99999).err().map(|e| e.to_string()).unwrap_or_default(),
                repo::create_project(conn, "Leak project", None, None).err().map(|e| e.to_string()).unwrap_or_default(),
                import::resolve(Some(format!("C:\\no\\such\\{SECRET}.env")), None).err().map(|e| e.to_string()).unwrap_or_default(),
            ];
            Ok((m, d, f, errs))
        })
        .unwrap();

    dtos.push(("status", json(&state.status().unwrap())));
    dtos.push(("matrix", json(&rows)));
    dtos.push(("diff", json(&diff_entries)));
    dtos.push(("findings", json(&findings)));
    dtos.push(("markdown", rules::to_markdown("staging", "production", &findings)));
    dtos.push(("scan hit", json(&scan::ScanHit { path: "C:\\x\\.env".into(), file_name: ".env".into(), guessed_stage: "local".into(), guessed_branch: None })));
    for (label, text) in &dtos {
        assert_clean(label, text);
    }
    // An error for a path the caller typed may echo that path (it is not a value), but never file content.
    for e in &errors[..2] {
        assert_clean("error", e);
    }

    // The database file never holds a value, a comment or the password in plaintext.
    state.lock();
    let bytes = std::fs::read(state.db_path()).unwrap();
    for needle in [SECRET, COMMENT_SECRET, PASSWORD] {
        assert!(!bytes.windows(needle.len()).any(|w| w == needle.as_bytes()), "database file contains `{needle}`");
    }
    // Key NAMES are stored in plaintext by design (search and diff are fast).
    assert!(bytes.windows(8).any(|w| w == b"LEAK_KEY"));
    let _ = std::fs::remove_dir_all(state.data_dir());
}

/// True if `needle` occurs in `code` as a whole word (not inside `dialog::` or `eprintln!`).
fn has_word(code: &str, needle: &str) -> bool {
    code.match_indices(needle).any(|(i, _)| {
        code[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

#[test]
fn source_has_no_print_or_log_macros() {
    // `eprintln!` is allowed exactly once, for the "LOCALAPPDATA is not set" startup failure.
    let forbidden = ["println!", "print!(", "dbg!", "eprint!(", "log::", "tracing::", "env_logger", "tauri_plugin_log"];
    let mut offenders = Vec::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("leak_tests.rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    if forbidden.iter().any(|f| has_word(code, f)) {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                    if has_word(code, "eprintln!") && !path.ends_with("lib.rs") {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(offenders.is_empty(), "print/log macros found: {offenders:?}");
}

#[test]
fn no_unwrap_or_expect_outside_tests_and_setup() {
    // CLAUDE.md: no `unwrap()` outside tests. Scan non-test code (everything before `#[cfg(test)]`).
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root];
    let mut offenders = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("leak_tests.rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                let prod = text.split("#[cfg(test)]").next().unwrap_or("");
                for (n, line) in prod.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains(".unwrap()") || (code.contains(".expect(") && !path.ends_with("lib.rs")) {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(offenders.is_empty(), "unwrap/expect in non-test code: {offenders:?}");
}

mod erased {
    pub trait Ser {
        fn to_json(&self) -> String;
    }
    impl<T: serde::Serialize + ?Sized> Ser for T {
        fn to_json(&self) -> String {
            serde_json::to_string(self).unwrap_or_default()
        }
    }
}
