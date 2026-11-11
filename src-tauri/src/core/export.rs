//! What gets written when the user exports a snapshot. Pure functions, so they can be tested
//! without a save dialog. The command layer picks the path through a save dialog and writes this text.

use crate::core::parser::{self, Entry};
use std::collections::HashSet;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Full `.env`: comments, keys and real values.
    Full,
    /// `.env.example`: comments and keys, values blank.
    Example,
    /// Only the keys that are absent or empty in the destination, as a block to paste into a
    /// deploy platform. No comments.
    MissingOnly,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "full" => Some(Mode::Full),
            "example" => Some(Mode::Example),
            "missing_only" => Some(Mode::MissingOnly),
            _ => None,
        }
    }

    pub fn default_file_name(self, stage: &str) -> String {
        match self {
            Mode::Full => format!(".env.{stage}"),
            Mode::Example => ".env.example".to_string(),
            Mode::MissingOnly => "missing-keys.env".to_string(),
        }
    }
}

/// `present` = keys that exist with a non-empty value in the destination (used by `MissingOnly`).
pub fn render(mode: Mode, entries: &[Entry], present: &HashSet<String>) -> Zeroizing<String> {
    Zeroizing::new(match mode {
        Mode::Full => parser::serialize(entries),
        Mode::Example => parser::serialize_example(entries),
        Mode::MissingOnly => {
            let missing: Vec<Entry> = entries
                .iter()
                .filter(|e| !present.contains(&e.key))
                .map(|e| Entry { key: e.key.clone(), value: e.value.clone(), comment: None, line: 0 })
                .collect();
            parser::serialize(&missing)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser::parse;

    fn entries() -> Vec<Entry> {
        parse("# the db\nDATABASE_URL=fake_value_1\nREDIS_URL=fake_value_2\nEMPTY_ONE=\nPASSWORD=\"has # hash\"\n").entries
    }

    #[test]
    fn mode_parsing() {
        assert_eq!(Mode::parse("full"), Some(Mode::Full));
        assert_eq!(Mode::parse("example"), Some(Mode::Example));
        assert_eq!(Mode::parse("missing_only"), Some(Mode::MissingOnly));
        assert_eq!(Mode::parse("everything"), None);
    }

    #[test]
    fn file_names() {
        assert_eq!(Mode::Full.default_file_name("production"), ".env.production");
        assert_eq!(Mode::Example.default_file_name("production"), ".env.example");
        assert_eq!(Mode::MissingOnly.default_file_name("production"), "missing-keys.env");
    }

    #[test]
    fn full_export_has_values_comments_and_round_trips() {
        let text = render(Mode::Full, &entries(), &HashSet::new());
        assert!(text.contains("# the db\nDATABASE_URL=fake_value_1\n"));
        assert!(text.contains("PASSWORD=\"has # hash\""));
        let again = parse(&text);
        assert_eq!(again.entries.len(), 4);
        assert_eq!(again.entries[3].value, "has # hash");
        assert!(again.warnings.is_empty());
    }

    #[test]
    fn example_export_blanks_every_value_and_keeps_comments() {
        let text = render(Mode::Example, &entries(), &HashSet::new());
        assert_eq!(&*text, "# the db\nDATABASE_URL=\nREDIS_URL=\nEMPTY_ONE=\nPASSWORD=\n");
        assert!(!text.contains("fake_value"));
        assert!(!text.contains("hash"));
    }

    #[test]
    fn missing_only_writes_just_what_the_destination_lacks() {
        // The destination has DATABASE_URL, but REDIS_URL is absent and EMPTY_ONE is empty there
        // (empty keys are not in `present`).
        let present: HashSet<String> = ["DATABASE_URL".to_string(), "PASSWORD".to_string()].into();
        let text = render(Mode::MissingOnly, &entries(), &present);
        assert_eq!(&*text, "REDIS_URL=fake_value_2\nEMPTY_ONE=\n");
        assert!(!text.contains('#'), "no comments in the paste block");
    }

    #[test]
    fn missing_only_is_empty_when_nothing_is_missing() {
        let present: HashSet<String> = entries().iter().map(|e| e.key.clone()).collect();
        assert_eq!(&*render(Mode::MissingOnly, &entries(), &present), "");
    }
}
