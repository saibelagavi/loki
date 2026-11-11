//! Diff checker: compare actual values between two sources (snapshot, target, file, pasted text).
//!
//! Equality is decided in the core with HMAC fingerprints, so the default result carries no
//! values at all. Values are included only for keys the user explicitly asks to see (or "all"),
//! and the command layer writes an audit entry when it does.

use crate::core::crypto::VaultKeys;
use crate::core::parser::Entry;
use crate::error::Result;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowStatus {
    Same,
    Changed,
    /// Only in the left source.
    Removed,
    /// Only in the right source.
    Added,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ValueRow {
    pub key: String,
    pub status: RowStatus,
    pub left_empty: Option<bool>,
    pub right_empty: Option<bool>,
    /// For changed values: a value-free explanation, e.g. "only whitespace differs".
    pub hint: Option<&'static str>,
    pub left_value: Option<String>,
    pub right_value: Option<String>,
}

#[derive(Debug, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub same: usize,
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
}

/// Which values to include. Default: none.
pub enum Reveal<'a> {
    None,
    All,
    Keys(&'a HashSet<String>),
}

impl Reveal<'_> {
    fn wants(&self, key: &str) -> bool {
        match self {
            Reveal::None => false,
            Reveal::All => true,
            Reveal::Keys(k) => k.contains(key),
        }
    }
}

fn hint(a: &str, b: &str) -> Option<&'static str> {
    if a.trim() == b.trim() {
        return Some("only leading or trailing whitespace differs");
    }
    let strip = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    if strip(a) == strip(b) {
        return Some("only whitespace differs");
    }
    if a.to_lowercase() == b.to_lowercase() {
        return Some("only letter case differs");
    }
    let unslash = |s: &str| s.trim_end_matches('/').to_string();
    if unslash(a) == unslash(b) {
        return Some("only a trailing slash differs");
    }
    if a.is_empty() || b.is_empty() {
        return Some("one side is empty");
    }
    None
}

pub fn compare(keys: &VaultKeys, left: &[Entry], right: &[Entry], reveal: Reveal) -> Result<(Vec<ValueRow>, Counts)> {
    let fp = |e: &Entry| keys.fingerprint(e.value.as_bytes());
    let right_map: HashMap<&str, &Entry> = right.iter().map(|e| (e.key.as_str(), e)).collect();
    let left_keys: HashSet<&str> = left.iter().map(|e| e.key.as_str()).collect();
    let mut rows = Vec::new();
    let mut counts = Counts::default();

    for l in left {
        let show = reveal.wants(&l.key);
        match right_map.get(l.key.as_str()) {
            Some(r) => {
                let same = fp(l)? == fp(r)?;
                if same {
                    counts.same += 1;
                } else {
                    counts.changed += 1;
                }
                rows.push(ValueRow {
                    key: l.key.clone(),
                    status: if same { RowStatus::Same } else { RowStatus::Changed },
                    left_empty: Some(l.value.is_empty()),
                    right_empty: Some(r.value.is_empty()),
                    hint: if same { None } else { hint(&l.value, &r.value) },
                    left_value: show.then(|| l.value.clone()),
                    right_value: show.then(|| r.value.clone()),
                });
            }
            None => {
                counts.removed += 1;
                rows.push(ValueRow {
                    key: l.key.clone(),
                    status: RowStatus::Removed,
                    left_empty: Some(l.value.is_empty()),
                    right_empty: None,
                    hint: None,
                    left_value: show.then(|| l.value.clone()),
                    right_value: None,
                });
            }
        }
    }
    for r in right {
        if !left_keys.contains(r.key.as_str()) {
            counts.added += 1;
            rows.push(ValueRow {
                key: r.key.clone(),
                status: RowStatus::Added,
                left_empty: None,
                right_empty: Some(r.value.is_empty()),
                hint: None,
                left_value: None,
                right_value: reveal.wants(&r.key).then(|| r.value.clone()),
            });
        }
    }
    Ok((rows, counts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::{derive_keys, TEST_PARAMS};
    use crate::core::parser::parse;

    fn keys() -> VaultKeys {
        derive_keys(b"fake_password_1", &[4u8; 16], TEST_PARAMS).unwrap()
    }

    const LEFT: &str = "SAME=fake_value_1\nCHANGED=fake_value_2\nSPACE=fake_value_3\nCASE=Fake_Value_4\nSLASH=https://x.test/\nGONE=fake_value_5\nEMPTY=\n";
    const RIGHT: &str = "SAME=fake_value_1\nCHANGED=fake_value_9\nSPACE=\" fake_value_3 \"\nCASE=fake_value_4\nSLASH=https://x.test\nNEW=fake_value_6\nEMPTY=fake_value_7\n";

    #[test]
    fn statuses_counts_and_hints() {
        let (rows, counts) = compare(&keys(), &parse(LEFT).entries, &parse(RIGHT).entries, Reveal::None).unwrap();
        assert_eq!(counts, Counts { same: 1, changed: 5, added: 1, removed: 1 });
        let get = |k: &str| rows.iter().find(|r| r.key == k).unwrap();
        assert_eq!(get("SAME").status, RowStatus::Same);
        assert_eq!(get("CHANGED").status, RowStatus::Changed);
        assert_eq!(get("CHANGED").hint, None);
        assert_eq!(get("SPACE").hint, Some("only leading or trailing whitespace differs"));
        assert_eq!(get("CASE").hint, Some("only letter case differs"));
        assert_eq!(get("SLASH").hint, Some("only a trailing slash differs"));
        assert_eq!(get("EMPTY").hint, Some("one side is empty"));
        assert_eq!(get("GONE").status, RowStatus::Removed);
        assert_eq!(get("NEW").status, RowStatus::Added);
        // Left order first, then keys only on the right.
        assert_eq!(rows.last().unwrap().key, "NEW");
    }

    #[test]
    fn no_values_unless_asked() {
        let (rows, _) = compare(&keys(), &parse(LEFT).entries, &parse(RIGHT).entries, Reveal::None).unwrap();
        let json = serde_json::to_string(&rows).unwrap();
        assert!(!json.contains("fake_value"), "{json}");
        assert!(rows.iter().all(|r| r.left_value.is_none() && r.right_value.is_none()));
    }

    #[test]
    fn reveal_selected_keys_only() {
        let want: HashSet<String> = ["CHANGED".to_string(), "NEW".to_string()].into();
        let (rows, _) = compare(&keys(), &parse(LEFT).entries, &parse(RIGHT).entries, Reveal::Keys(&want)).unwrap();
        let get = |k: &str| rows.iter().find(|r| r.key == k).unwrap();
        assert_eq!(get("CHANGED").left_value.as_deref(), Some("fake_value_2"));
        assert_eq!(get("CHANGED").right_value.as_deref(), Some("fake_value_9"));
        assert_eq!(get("NEW").right_value.as_deref(), Some("fake_value_6"));
        assert!(get("SAME").left_value.is_none());
        let shown: usize = rows.iter().filter(|r| r.left_value.is_some() || r.right_value.is_some()).count();
        assert_eq!(shown, 2);
    }

    #[test]
    fn reveal_all() {
        let (rows, _) = compare(&keys(), &parse(LEFT).entries, &parse(RIGHT).entries, Reveal::All).unwrap();
        assert!(rows.iter().filter(|r| r.status != RowStatus::Added).all(|r| r.left_value.is_some()));
        assert!(rows.iter().filter(|r| r.status != RowStatus::Removed).all(|r| r.right_value.is_some()));
    }
}
