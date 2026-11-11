//! User settings, stored as one JSON document in the `settings` table. Never holds values.

use crate::core::rules::CustomRule;
use crate::error::{LokiError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub auto_lock_minutes: u32,
    pub lock_on_minimize: bool,
    pub clipboard_clear_seconds: u32,
    pub reveal_seconds: u32,
    /// Windows: exclude the window from screen capture (WDA_EXCLUDEFROMCAPTURE).
    pub exclude_from_capture: bool,
    pub disabled_rules: Vec<String>,
    pub custom_rules: Vec<CustomRule>,
    /// Diff checker: how long revealed values stay on screen.
    pub diff_reveal_seconds: u32,
    /// Overview: remind me to rotate secrets and passwords older than this.
    pub rotation_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_lock_minutes: 5,
            lock_on_minimize: false,
            clipboard_clear_seconds: 30,
            reveal_seconds: 10,
            exclude_from_capture: false,
            disabled_rules: vec![],
            custom_rules: vec![],
            diff_reveal_seconds: 60,
            rotation_days: 90,
        }
    }
}

impl Settings {
    /// Clamps numbers into sane ranges and rejects malformed custom rules.
    pub fn validated(mut self) -> Result<Settings> {
        self.auto_lock_minutes = self.auto_lock_minutes.clamp(1, 240);
        self.clipboard_clear_seconds = self.clipboard_clear_seconds.clamp(5, 300);
        self.reveal_seconds = self.reveal_seconds.clamp(3, 60);
        self.diff_reveal_seconds = self.diff_reveal_seconds.clamp(10, 600);
        self.rotation_days = self.rotation_days.clamp(7, 730);
        let known: HashSet<&str> = crate::core::rules::RULES.iter().map(|r| r.id).collect();
        self.disabled_rules.retain(|id| known.contains(id.as_str()) || id.starts_with("custom_"));
        if self.custom_rules.len() > 50 {
            return Err(LokiError::Invalid("At most 50 custom rules.".into()));
        }
        let mut ids = HashSet::new();
        for r in &self.custom_rules {
            if r.id.trim().is_empty() || !ids.insert(r.id.clone()) {
                return Err(LokiError::Invalid("Each custom rule needs a unique id.".into()));
            }
            if r.key_pattern.trim().is_empty() || r.text.trim().is_empty() {
                return Err(LokiError::Invalid("A custom rule needs a key pattern and some text to look for.".into()));
            }
        }
        Ok(self)
    }
}

pub fn load(conn: &rusqlite::Connection) -> Settings {
    crate::db::repo::get_setting(conn, "settings")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .and_then(|s| s.validated().ok())
        .unwrap_or_default()
}

pub fn save(conn: &rusqlite::Connection, s: &Settings) -> Result<()> {
    let json = serde_json::to_string(s).map_err(|_| LokiError::Invalid("Could not save settings.".into()))?;
    crate::db::repo::set_setting(conn, "settings", &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rules::{CustomOp, Severity};

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!((s.auto_lock_minutes, s.clipboard_clear_seconds, s.reveal_seconds), (5, 30, 10));
        assert_eq!((s.diff_reveal_seconds, s.rotation_days), (60, 90));
    }

    #[test]
    fn values_are_clamped() {
        let s = Settings { auto_lock_minutes: 0, clipboard_clear_seconds: 99999, reveal_seconds: 0, ..Default::default() }.validated().unwrap();
        assert_eq!((s.auto_lock_minutes, s.clipboard_clear_seconds, s.reveal_seconds), (1, 300, 3));
    }

    #[test]
    fn bad_custom_rules_are_rejected() {
        let rule = |id: &str, pat: &str, text: &str| CustomRule {
            id: id.into(),
            key_pattern: pat.into(),
            op: CustomOp::Contains,
            text: text.into(),
            severity: Severity::Warning,
            production_only: true,
        };
        assert!(Settings { custom_rules: vec![rule("custom_1", "*", "abc")], ..Default::default() }.validated().is_ok());
        assert!(Settings { custom_rules: vec![rule("custom_1", "", "abc")], ..Default::default() }.validated().is_err());
        assert!(Settings { custom_rules: vec![rule("custom_1", "*", "")], ..Default::default() }.validated().is_err());
        assert!(Settings { custom_rules: vec![rule("custom_1", "*", "a"), rule("custom_1", "*", "b")], ..Default::default() }.validated().is_err());
    }

    #[test]
    fn unknown_disabled_rules_are_dropped_and_missing_json_fields_default() {
        let s: Settings = serde_json::from_str(r#"{"disabledRules":["nope","stale_in_to","custom_9"],"autoLockMinutes":7}"#).unwrap();
        let s = s.validated().unwrap();
        assert_eq!(s.disabled_rules, ["stale_in_to", "custom_9"]);
        assert_eq!((s.auto_lock_minutes, s.reveal_seconds), (7, 10));
        // Settings saved by the previous release (no new fields) still load, with defaults.
        assert_eq!((s.diff_reveal_seconds, s.rotation_days), (60, 90));
    }
}
