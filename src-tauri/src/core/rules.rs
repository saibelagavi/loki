//! Push-day checks. Rules are data: each has an id, severity and description, and the user can
//! switch any of them off in Settings or add simple custom rules (key pattern + contains/starts with).
//!
//! Findings name keys only. Messages never contain a value.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

pub struct RuleDef {
    pub id: &'static str,
    pub severity: Severity,
    pub title: &'static str,
    pub description: &'static str,
}

pub const RULES: &[RuleDef] = &[
    RuleDef {
        id: "missing_in_to",
        severity: Severity::Error,
        title: "Key missing in the target",
        description: "A key in the source target does not exist in the destination target.",
    },
    RuleDef {
        id: "required_missing",
        severity: Severity::Error,
        title: "Required key missing or empty",
        description: "A key marked required for the destination stage is missing or empty.",
    },
    RuleDef {
        id: "local_address",
        severity: Severity::Warning,
        title: "Local address in production",
        description: "A production value contains localhost, 127.0.0.1, 0.0.0.0 or example.com.",
    },
    RuleDef {
        id: "placeholder_word",
        severity: Severity::Warning,
        title: "Placeholder word in production",
        description: "A production value contains changeme, xxx, todo or test.",
    },
    RuleDef {
        id: "test_key_prefix",
        severity: Severity::Warning,
        title: "Test API key in production",
        description: "A production value starts with sk_test_, pk_test_ or rk_test_.",
    },
    RuleDef {
        id: "shared_secret",
        severity: Severity::Warning,
        title: "Secret shared across stages",
        description: "A secret has the same value in production and in a non-production target.",
    },
    RuleDef {
        id: "debug_flag",
        severity: Severity::Warning,
        title: "Debug setting in production",
        description: "DEBUG=true or NODE_ENV=development in production.",
    },
    RuleDef {
        id: "stale_in_to",
        severity: Severity::Info,
        title: "Key only in the target",
        description: "A key exists in the destination but not in the source (possibly stale).",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomOp {
    Contains,
    StartsWith,
}

/// `key_pattern` supports `*` wildcards, case-insensitive (`*_URL`, `STRIPE_*`, `*`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CustomRule {
    pub id: String,
    pub key_pattern: String,
    pub op: CustomOp,
    pub text: String,
    pub severity: Severity,
    /// Only applies when the destination stage is production.
    #[serde(default = "yes")]
    pub production_only: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub severity: Severity,
    pub rule_id: String,
    pub key: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct KeyNote {
    pub required_stages: Vec<String>,
    pub is_secret: bool,
}

pub struct CheckVar {
    pub key: String,
    pub value: String,
    pub fp: Vec<u8>,
    pub is_empty: bool,
}

impl Drop for CheckVar {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.value);
    }
}

pub struct CheckInput<'a> {
    pub from: &'a [CheckVar],
    pub to: &'a [CheckVar],
    pub to_stage: &'a str,
    pub notes: &'a HashMap<String, KeyNote>,
    /// Fingerprints per key across the project's non-production targets (current snapshots).
    pub non_prod_fps: &'a HashMap<String, Vec<Vec<u8>>>,
    pub disabled: &'a HashSet<String>,
    pub custom: &'a [CustomRule],
}

fn severity_of(id: &str) -> Severity {
    RULES.iter().find(|r| r.id == id).map(|r| r.severity).unwrap_or(Severity::Info)
}

fn finding(id: &str, key: &str, message: String) -> Finding {
    Finding { severity: severity_of(id), rule_id: id.to_string(), key: Some(key.to_string()), message }
}

const LOCAL_MARKERS: &[&str] = &["localhost", "127.0.0.1", "0.0.0.0", "example.com"];
const PLACEHOLDER_WORDS: &[&str] = &["changeme", "xxx", "todo", "test"];
const TEST_PREFIXES: &[&str] = &["sk_test_", "pk_test_", "rk_test_"];
const SECRET_NAME_HINTS: &[&str] = &["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD", "PWD", "CREDENTIAL", "PRIVATE"];

fn looks_secret(key: &str) -> bool {
    let up = key.to_ascii_uppercase();
    SECRET_NAME_HINTS.iter().any(|h| up.contains(h))
}

/// `*` wildcard match, case-insensitive.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn run(input: &CheckInput) -> Vec<Finding> {
    let on = |id: &str| !input.disabled.contains(id);
    let to_map: HashMap<&str, &CheckVar> = input.to.iter().map(|v| (v.key.as_str(), v)).collect();
    let from_keys: HashSet<&str> = input.from.iter().map(|v| v.key.as_str()).collect();
    let is_prod = input.to_stage == "production";
    let mut out: Vec<Finding> = Vec::new();
    let mut missing_flagged: HashSet<&str> = HashSet::new();

    // Errors
    if on("missing_in_to") {
        for v in input.from {
            if !to_map.contains_key(v.key.as_str()) {
                missing_flagged.insert(v.key.as_str());
                out.push(finding("missing_in_to", &v.key, format!("`{}` is set in the source but missing in the target", v.key)));
            }
        }
    }
    if on("required_missing") {
        let mut keys: Vec<&String> = input.notes.iter().filter(|(_, n)| n.required_stages.iter().any(|s| s == input.to_stage)).map(|(k, _)| k).collect();
        keys.sort();
        for key in keys {
            match to_map.get(key.as_str()) {
                Some(v) if v.is_empty => out.push(finding("required_missing", key, format!("`{key}` is required for {} but is empty", input.to_stage))),
                None if !missing_flagged.contains(key.as_str()) => {
                    out.push(finding("required_missing", key, format!("`{key}` is required for {} but is missing", input.to_stage)))
                }
                _ => {}
            }
        }
    }

    // Warnings (production destination only)
    if is_prod {
        for v in input.to {
            if v.is_empty {
                continue;
            }
            let lower = v.value.to_lowercase();
            if on("local_address") && LOCAL_MARKERS.iter().any(|m| lower.contains(m)) {
                out.push(finding("local_address", &v.key, format!("`{}` points at a local or example address", v.key)));
            }
            if on("placeholder_word") && PLACEHOLDER_WORDS.iter().any(|m| lower.contains(m)) {
                out.push(finding("placeholder_word", &v.key, format!("`{}` contains a placeholder word (changeme, xxx, todo, test)", v.key)));
            }
            if on("test_key_prefix") && TEST_PREFIXES.iter().any(|p| lower.starts_with(p)) {
                out.push(finding("test_key_prefix", &v.key, format!("`{}` looks like a test API key", v.key)));
            }
            if on("shared_secret") {
                let secret = input.notes.get(&v.key).map(|n| n.is_secret).unwrap_or_else(|| looks_secret(&v.key));
                if secret && input.non_prod_fps.get(&v.key).is_some_and(|fps| fps.contains(&v.fp)) {
                    out.push(finding("shared_secret", &v.key, format!("`{}` has the same value in production and in a non-production target", v.key)));
                }
            }
            if on("debug_flag") {
                let key = v.key.to_ascii_uppercase();
                let val = lower.trim();
                if (key == "DEBUG" && matches!(val, "true" | "1" | "yes" | "on")) || (key == "NODE_ENV" && val == "development") {
                    out.push(finding("debug_flag", &v.key, format!("`{}` has a development value in production", v.key)));
                }
            }
        }
    }

    // Custom rules
    for rule in input.custom {
        if input.disabled.contains(&rule.id) || (rule.production_only && !is_prod) || rule.text.is_empty() {
            continue;
        }
        let needle = rule.text.to_lowercase();
        for v in input.to {
            if v.is_empty || !glob_match(&rule.key_pattern, &v.key) {
                continue;
            }
            let lower = v.value.to_lowercase();
            let hit = match rule.op {
                CustomOp::Contains => lower.contains(&needle),
                CustomOp::StartsWith => lower.starts_with(&needle),
            };
            if hit {
                let how = match rule.op {
                    CustomOp::Contains => "contains",
                    CustomOp::StartsWith => "starts with",
                };
                out.push(Finding {
                    severity: rule.severity,
                    rule_id: rule.id.clone(),
                    key: Some(v.key.clone()),
                    message: format!("`{}` matches your custom rule (value {how} the text set in Settings)", v.key),
                });
            }
        }
    }

    // Info
    if on("stale_in_to") {
        for v in input.to {
            if !from_keys.contains(v.key.as_str()) {
                out.push(finding("stale_in_to", &v.key, format!("`{}` exists only in the target (possibly stale)", v.key)));
            }
        }
    }

    out.sort_by_key(|f| (f.severity as u8, f.key.clone()));
    out
}

pub fn to_markdown(from_label: &str, to_label: &str, findings: &[Finding]) -> String {
    let mut s = format!("# Push check: {from_label} to {to_label}\n\n");
    if findings.is_empty() {
        s.push_str("No findings.\n");
        return s;
    }
    for (sev, title) in [(Severity::Error, "Errors"), (Severity::Warning, "Warnings"), (Severity::Info, "Info")] {
        let group: Vec<&Finding> = findings.iter().filter(|f| f.severity == sev).collect();
        if group.is_empty() {
            continue;
        }
        s.push_str(&format!("## {title} ({})\n\n", group.len()));
        for f in group {
            s.push_str(&format!("- [ ] {}\n", f.message));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(key: &str, value: &str) -> CheckVar {
        CheckVar { key: key.into(), value: value.into(), fp: value.as_bytes().to_vec(), is_empty: value.is_empty() }
    }

    struct Ctx {
        notes: HashMap<String, KeyNote>,
        non_prod: HashMap<String, Vec<Vec<u8>>>,
        disabled: HashSet<String>,
        custom: Vec<CustomRule>,
    }
    impl Ctx {
        fn new() -> Self {
            Ctx { notes: HashMap::new(), non_prod: HashMap::new(), disabled: HashSet::new(), custom: vec![] }
        }
        fn run(&self, from: &[CheckVar], to: &[CheckVar], stage: &str) -> Vec<Finding> {
            run(&CheckInput { from, to, to_stage: stage, notes: &self.notes, non_prod_fps: &self.non_prod, disabled: &self.disabled, custom: &self.custom })
        }
    }

    fn ids(f: &[Finding]) -> Vec<&str> {
        f.iter().map(|x| x.rule_id.as_str()).collect()
    }

    #[test]
    fn missing_in_to_positive_and_negative() {
        let c = Ctx::new();
        let f = c.run(&[cv("A", "fake_value_1"), cv("B", "fake_value_2")], &[cv("A", "fake_value_3")], "production");
        assert_eq!(ids(&f), ["missing_in_to"]);
        assert_eq!(f[0].key.as_deref(), Some("B"));
        assert!(f[0].severity == Severity::Error);
        assert!(c.run(&[cv("A", "fake_value_1")], &[cv("A", "fake_value_3")], "production").is_empty());
    }

    #[test]
    fn required_missing_or_empty() {
        let mut c = Ctx::new();
        c.notes.insert("DB_URL".into(), KeyNote { required_stages: vec!["production".into()], is_secret: true });
        let f = c.run(&[], &[], "production");
        assert_eq!(ids(&f), ["required_missing"]);
        let f = c.run(&[cv("DB_URL", "f")], &[cv("DB_URL", "")], "production");
        assert_eq!(ids(&f), ["required_missing"]);
        assert!(c.run(&[cv("DB_URL", "f")], &[cv("DB_URL", "fake_value_1")], "production").is_empty());
        // Not required for staging.
        assert!(c.run(&[], &[], "staging").is_empty());
        // Missing and in source: reported once, by missing_in_to.
        let f = c.run(&[cv("DB_URL", "fake_value_1")], &[], "production");
        assert_eq!(ids(&f), ["missing_in_to"]);
    }

    #[test]
    fn local_address_rule() {
        let c = Ctx::new();
        for bad in ["http://localhost:3000", "postgres://127.0.0.1/db", "http://0.0.0.0", "https://api.example.com/x", "http://LOCALHOST"] {
            let f = c.run(&[cv("API_URL", "fake")], &[cv("API_URL", bad)], "production");
            assert!(ids(&f).contains(&"local_address"), "{bad}");
        }
        assert!(c.run(&[cv("API_URL", "fake")], &[cv("API_URL", "https://api.real-service.io")], "production").is_empty());
        // Only checked for production.
        assert!(c.run(&[cv("API_URL", "fake")], &[cv("API_URL", "http://localhost")], "staging").is_empty());
    }

    #[test]
    fn placeholder_word_rule() {
        let c = Ctx::new();
        for bad in ["changeme", "xxx-xxx", "TODO", "my_test_value"] {
            let f = c.run(&[cv("K", "f")], &[cv("K", bad)], "production");
            assert!(ids(&f).contains(&"placeholder_word"), "{bad}");
        }
        assert!(c.run(&[cv("K", "f")], &[cv("K", "fake_value_1")], "production").is_empty());
    }

    #[test]
    fn test_key_prefix_rule() {
        let c = Ctx::new();
        for bad in ["sk_test_fake", "pk_test_fake", "rk_test_fake"] {
            assert!(ids(&c.run(&[cv("K", "f")], &[cv("K", bad)], "production")).contains(&"test_key_prefix"));
        }
        assert!(!ids(&c.run(&[cv("K", "f")], &[cv("K", "sk_live_fake")], "production")).contains(&"test_key_prefix"));
    }

    #[test]
    fn shared_secret_rule() {
        let mut c = Ctx::new();
        c.non_prod.insert("STRIPE_SECRET".into(), vec![b"fake_value_1".to_vec()]);
        let f = c.run(&[cv("STRIPE_SECRET", "f")], &[cv("STRIPE_SECRET", "fake_value_1")], "production");
        assert_eq!(ids(&f), ["shared_secret"]);
        // Different value in production: fine.
        assert!(c.run(&[cv("STRIPE_SECRET", "f")], &[cv("STRIPE_SECRET", "fake_value_2")], "production").is_empty());
        // Not a secret by note: fine.
        c.notes.insert("STRIPE_SECRET".into(), KeyNote { required_stages: vec![], is_secret: false });
        assert!(c.run(&[cv("STRIPE_SECRET", "f")], &[cv("STRIPE_SECRET", "fake_value_1")], "production").is_empty());
        // Name does not look secret and there is no note: fine (e.g. a shared region name).
        let mut c2 = Ctx::new();
        c2.non_prod.insert("REGION".into(), vec![b"eu".to_vec()]);
        assert!(c2.run(&[cv("REGION", "eu")], &[cv("REGION", "eu")], "production").is_empty());
    }

    #[test]
    fn debug_flag_rule() {
        let c = Ctx::new();
        assert_eq!(ids(&c.run(&[cv("DEBUG", "x")], &[cv("DEBUG", "true")], "production")), ["debug_flag"]);
        assert_eq!(ids(&c.run(&[cv("NODE_ENV", "x")], &[cv("NODE_ENV", "development")], "production")), ["debug_flag"]);
        assert!(c.run(&[cv("DEBUG", "x")], &[cv("DEBUG", "false")], "production").is_empty());
        assert!(c.run(&[cv("NODE_ENV", "x")], &[cv("NODE_ENV", "production")], "production").is_empty());
    }

    #[test]
    fn stale_in_to_rule() {
        let c = Ctx::new();
        let f = c.run(&[cv("A", "f")], &[cv("A", "f2"), cv("OLD", "f3")], "production");
        assert_eq!(ids(&f), ["stale_in_to"]);
        assert!(f[0].severity == Severity::Info);
        assert!(c.run(&[cv("A", "f")], &[cv("A", "f2")], "production").is_empty());
    }

    #[test]
    fn rules_can_be_toggled_off() {
        let mut c = Ctx::new();
        c.disabled.insert("missing_in_to".into());
        assert!(c.run(&[cv("A", "f")], &[], "production").is_empty());
    }

    #[test]
    fn custom_rules() {
        let mut c = Ctx::new();
        c.custom.push(CustomRule {
            id: "custom_1".into(),
            key_pattern: "*_URL".into(),
            op: CustomOp::StartsWith,
            text: "http://".into(),
            severity: Severity::Warning,
            production_only: true,
        });
        let f = c.run(&[cv("API_URL", "f")], &[cv("API_URL", "http://real.io"), cv("OTHER", "http://real.io")], "production");
        let custom: Vec<_> = f.iter().filter(|x| x.rule_id == "custom_1").collect();
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].key.as_deref(), Some("API_URL"));
        // Negative: https does not match.
        assert!(!ids(&c.run(&[cv("API_URL", "f")], &[cv("API_URL", "https://real.io")], "production")).contains(&"custom_1"));
        // Production-only.
        assert!(!ids(&c.run(&[cv("API_URL", "f")], &[cv("API_URL", "http://real.io")], "staging")).contains(&"custom_1"));
    }

    #[test]
    fn glob() {
        assert!(glob_match("*", "ANYTHING"));
        assert!(glob_match("*_url", "API_URL"));
        assert!(glob_match("STRIPE_*", "stripe_key"));
        assert!(glob_match("A*C", "ABBBC"));
        assert!(!glob_match("A*C", "ABBBD"));
        assert!(glob_match("EXACT", "exact"));
        assert!(!glob_match("EXACT", "EXACTLY"));
    }

    #[test]
    fn findings_and_markdown_never_contain_values() {
        let c = Ctx::new();
        let f = c.run(
            &[cv("API_URL", "f"), cv("GONE", "f")],
            &[cv("API_URL", "http://localhost/fake_secret_value_xyz"), cv("DEBUG", "true")],
            "production",
        );
        let md = to_markdown("staging", "production", &f);
        let json = serde_json::to_string(&f).unwrap();
        for text in [&md, &json] {
            assert!(!text.contains("fake_secret_value_xyz"));
            assert!(!text.contains("localhost"));
        }
        assert!(md.contains("- [ ]"));
        assert!(md.contains("## Errors"));
    }

    #[test]
    fn sorted_by_severity() {
        let c = Ctx::new();
        let f = c.run(&[cv("A", "f")], &[cv("OLD", "http://localhost")], "production");
        let sev: Vec<Severity> = f.iter().map(|x| x.severity).collect();
        let mut sorted = sev.clone();
        sorted.sort_by_key(|s| *s as u8);
        assert_eq!(sev, sorted);
    }
}
