//! `.env` parsing and serializing.
//!
//! Handles: `KEY=value`, `export KEY=value`, single and double quotes, `\n` escapes in double
//! quotes, multi-line quoted values, `# comments` (full-line and after an unquoted value), empty
//! values, spaces around `=`, CRLF, UTF-8 BOM, duplicate keys (last wins, with a warning) and
//! invalid lines (warning, never a crash).
//!
//! Warnings may name keys but NEVER contain value text.

use std::collections::HashMap;
use zeroize::Zeroize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    /// Full-line comments directly above the key, plus any inline comment, joined by `\n`.
    pub comment: Option<String>,
    /// 1-based line where the key starts.
    pub line: usize,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    pub warnings: Vec<Warning>,
    pub duplicate_count: usize,
}

/// A name we are willing to print in a warning: conventional `UPPER_SNAKE` with an underscore.
/// A bare line might be a secret value pasted without its key, so anything that does not look
/// like an env var name is never echoed.
fn is_safe_to_echo(token: &str) -> bool {
    token.len() <= 64
        && token.contains('_')
        && token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && token.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !token.ends_with('_')
        && !token.contains("__")
}

pub fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

pub fn parse(input: &str) -> Parsed {
    let text = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut normalized = text.replace("\r\n", "\n");
    let mut out = Parsed::default();
    {
        let lines: Vec<&str> = normalized.split('\n').collect();
        parse_lines(&lines, &mut out);
    }
    normalized.zeroize();
    out
}

fn parse_lines(lines: &[&str], out: &mut Parsed) {
    let mut index_of: HashMap<String, usize> = HashMap::new();
    let mut pending: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let line_no = i + 1;
        let trimmed = raw.trim();

        if trimmed.is_empty() {
            pending.clear(); // a blank line detaches a comment block from the next key
            i += 1;
            continue;
        }
        if let Some(c) = trimmed.strip_prefix('#') {
            pending.push(c.strip_prefix(' ').unwrap_or(c).to_string());
            i += 1;
            continue;
        }

        let mut body = raw.trim_start();
        if let Some(rest) = body.strip_prefix("export") {
            if rest.starts_with(char::is_whitespace) {
                body = rest.trim_start();
            }
        }

        let Some(eq) = body.find('=') else {
            let token = body.trim();
            let msg = if is_safe_to_echo(token) {
                format!("Line {line_no}: missing `=` after `{token}`")
            } else {
                format!("Line {line_no}: not a KEY=value line (missing `=`), skipped")
            };
            out.warnings.push(Warning { line: line_no, message: msg });
            pending.clear();
            i += 1;
            continue;
        };

        let key = body[..eq].trim_end();
        if !is_valid_key(key) {
            out.warnings.push(Warning {
                line: line_no,
                message: format!("Line {line_no}: invalid key name, skipped (use letters, digits, `_`, `.`, `-`)"),
            });
            pending.clear();
            i += 1;
            continue;
        }

        let val_part = body[eq + 1..].trim_start();
        let mut comments = std::mem::take(&mut pending);
        let value: String;
        let mut next_line = i + 1;

        if let Some(q) = val_part.chars().next().filter(|c| *c == '"' || *c == '\'') {
            match scan_quoted(lines, i, &val_part[1..], q) {
                Some((v, end_line, after)) => {
                    value = v;
                    next_line = end_line + 1;
                    let after = after.trim();
                    if let Some(c) = after.strip_prefix('#') {
                        comments.push(c.trim().to_string());
                    } else if !after.is_empty() {
                        out.warnings.push(Warning {
                            line: end_line + 1,
                            message: format!(
                                "Line {}: unexpected text after the closing quote of `{key}`, ignored",
                                end_line + 1
                            ),
                        });
                    }
                }
                None => {
                    out.warnings.push(Warning {
                        line: line_no,
                        message: format!("Line {line_no}: the quote opened for `{key}` is never closed"),
                    });
                    value = split_inline_comment(&val_part[1..], &mut comments);
                }
            }
        } else {
            value = split_inline_comment(val_part, &mut comments);
        }

        let comment = if comments.is_empty() { None } else { Some(comments.join("\n")) };
        if let Some(&idx) = index_of.get(key) {
            out.duplicate_count += 1;
            out.warnings.push(Warning {
                line: line_no,
                message: format!("Line {line_no}: duplicate key `{key}`, the last value wins"),
            });
            let e = &mut out.entries[idx];
            e.value = value;
            if comment.is_some() {
                e.comment = comment;
            }
        } else {
            index_of.insert(key.to_string(), out.entries.len());
            out.entries.push(Entry { key: key.to_string(), value, comment, line: line_no });
        }
        i = next_line;
    }
}

/// Unquoted value: a comment starts at `#` preceded by whitespace, or at a `#` that begins the
/// value and is followed by whitespace (`KEY=# note`). A `#` glued to the value is kept, so
/// `COLOR=#fff` and `PASS=ab#cd` survive.
fn split_inline_comment(raw: &str, comments: &mut Vec<String>) -> String {
    let bytes = raw.as_bytes();
    for (idx, b) in bytes.iter().enumerate() {
        let after_space = idx > 0 && bytes[idx - 1].is_ascii_whitespace();
        let leading_then_space = idx == 0 && bytes.get(1).is_some_and(|n| n.is_ascii_whitespace());
        if *b == b'#' && (after_space || leading_then_space) {
            comments.push(raw[idx + 1..].trim().to_string());
            return raw[..idx].trim_end().to_string();
        }
    }
    raw.trim_end().to_string()
}

/// Reads a quoted value that may span lines. Returns (value, last line index, text after quote).
fn scan_quoted(lines: &[&str], start: usize, first: &str, q: char) -> Option<(String, usize, String)> {
    let mut out = String::new();
    let mut li = start;
    let mut seg = first;
    loop {
        let mut chars = seg.char_indices();
        while let Some((idx, c)) = chars.next() {
            if q == '"' && c == '\\' {
                match chars.next() {
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 'r')) => out.push('\r'),
                    Some((_, 't')) => out.push('\t'),
                    Some((_, '"')) => out.push('"'),
                    Some((_, '\\')) => out.push('\\'),
                    Some((_, other)) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else if c == q {
                return Some((out, li, seg[idx + c.len_utf8()..].to_string()));
            } else {
                out.push(c);
            }
        }
        li += 1;
        if li >= lines.len() {
            out.zeroize();
            return None;
        }
        out.push('\n');
        seg = lines[li];
    }
}

fn needs_quotes(v: &str) -> bool {
    v.starts_with(char::is_whitespace)
        || v.ends_with(char::is_whitespace)
        || v.chars().any(|c| matches!(c, '#' | '"' | '\'' | '\\' | '\n' | '\r' | '\t'))
}

pub fn format_value(v: &str) -> String {
    if !needs_quotes(v) {
        return v.to_string();
    }
    let mut s = String::with_capacity(v.len() + 2);
    s.push('"');
    for c in v.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '"' => s.push_str("\\\""),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            other => s.push(other),
        }
    }
    s.push('"');
    s
}

fn push_comment(out: &mut String, comment: &Option<String>, first: bool) {
    if let Some(c) = comment {
        if !first {
            out.push('\n');
        }
        for line in c.split('\n') {
            if line.is_empty() {
                out.push_str("#\n");
            } else {
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
        }
    }
}

/// Full `.env`: comments, keys and real values.
pub fn serialize(entries: &[Entry]) -> String {
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        push_comment(&mut out, &e.comment, i == 0);
        out.push_str(&e.key);
        out.push('=');
        out.push_str(&format_value(&e.value));
        out.push('\n');
    }
    out
}

/// `.env.example`: comments and keys, values blank.
pub fn serialize_example(entries: &[Entry]) -> String {
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        push_comment(&mut out, &e.comment, i == 0);
        out.push_str(&e.key);
        out.push_str("=\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(input: &str) -> Entry {
        let p = parse(input);
        assert_eq!(p.entries.len(), 1, "warnings: {:?}", p.warnings);
        p.entries.into_iter().next().unwrap()
    }

    #[test]
    fn plain_pair() {
        let e = one("API_KEY=fake_value_1");
        assert_eq!((e.key.as_str(), e.value.as_str()), ("API_KEY", "fake_value_1"));
    }

    #[test]
    fn export_prefix() {
        assert_eq!(one("export API_KEY=fake_value_1").key, "API_KEY");
        // A key that merely starts with "export" is not a prefix.
        assert_eq!(one("EXPORT_DIR=fake_value_1").key, "EXPORT_DIR");
        assert_eq!(one("exports=fake_value_1").key, "exports");
    }

    #[test]
    fn single_and_double_quotes() {
        assert_eq!(one("A='fake value 1'").value, "fake value 1");
        assert_eq!(one("A=\"fake value 1\"").value, "fake value 1");
        // Single quotes are literal: no escapes, no inline comment handling.
        assert_eq!(one(r"A='fake\nvalue # 1'").value, r"fake\nvalue # 1");
    }

    #[test]
    fn newline_escapes_only_in_double_quotes() {
        assert_eq!(one(r#"A="line1\nline2""#).value, "line1\nline2");
        assert_eq!(one(r#"A="say \"hi\" \\ done""#).value, r#"say "hi" \ done"#);
        assert_eq!(one(r"A=C:\new\path").value, r"C:\new\path"); // unquoted: literal
        assert_eq!(one(r#"A="keep \q unknown""#).value, r"keep \q unknown");
    }

    #[test]
    fn multi_line_double_quoted() {
        let p = parse("PEM=\"-----BEGIN\nfake_line_2\n-----END\"\nNEXT=fake_value_1\n");
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].value, "-----BEGIN\nfake_line_2\n-----END");
        assert_eq!(p.entries[1].key, "NEXT");
        assert_eq!(p.entries[1].line, 4);
    }

    #[test]
    fn multi_line_single_quoted() {
        let p = parse("A='one\ntwo'\n");
        assert_eq!(p.entries[0].value, "one\ntwo");
    }

    #[test]
    fn full_line_and_inline_comments() {
        let p = parse("# the database\n# second line\nDB=fake_value_1 # inline note\nB=fake#glued\n");
        assert_eq!(p.entries[0].value, "fake_value_1");
        assert_eq!(p.entries[0].comment.as_deref(), Some("the database\nsecond line\ninline note"));
        // `#` with no whitespace before it is part of the value.
        assert_eq!(p.entries[1].value, "fake#glued");
        assert_eq!(p.entries[1].comment, None);
    }

    #[test]
    fn inline_comment_after_closing_quote() {
        let e = one("A=\"fake value\" # note");
        assert_eq!(e.value, "fake value");
        assert_eq!(e.comment.as_deref(), Some("note"));
    }

    #[test]
    fn blank_line_detaches_comment() {
        let p = parse("# header for the file\n\nA=fake_value_1\n");
        assert_eq!(p.entries[0].comment, None);
    }

    #[test]
    fn empty_values() {
        assert_eq!(one("A=").value, "");
        assert_eq!(one("A=   ").value, "");
        assert_eq!(one("A=\"\"").value, "");
        let e = one("A=# only a comment");
        assert_eq!((e.value.as_str(), e.comment.as_deref()), ("", Some("only a comment")));
        // A glued `#` is part of the value (CSS colors, passwords).
        assert_eq!(one("COLOR=#fff").value, "#fff");
    }

    #[test]
    fn spaces_around_equals() {
        let e = one("  KEY  =   fake value 1   ");
        assert_eq!((e.key.as_str(), e.value.as_str()), ("KEY", "fake value 1"));
    }

    #[test]
    fn crlf_and_bom() {
        let p = parse("\u{feff}A=fake_value_1\r\nB=\"x\r\ny\"\r\n");
        assert_eq!(p.entries[0].key, "A");
        assert_eq!(p.entries[0].value, "fake_value_1");
        assert_eq!(p.entries[1].value, "x\ny");
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn duplicate_keys_last_wins_with_warning() {
        let p = parse("A=fake_value_1\nB=fake_value_2\nA=fake_value_3\n");
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].value, "fake_value_3");
        assert_eq!(p.duplicate_count, 1);
        assert_eq!(p.warnings.len(), 1);
        assert_eq!(p.warnings[0].line, 3);
    }

    #[test]
    fn invalid_lines_warn_without_crashing() {
        let p = parse("API_KEY\nfake value with spaces\n1BAD=fake_value_1\nGOOD=fake_value_2\n=fake_value_3\n");
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].key, "GOOD");
        assert_eq!(p.warnings.len(), 4);
        assert_eq!(p.warnings[0].message, "Line 1: missing `=` after `API_KEY`");
    }

    #[test]
    fn bare_lines_are_only_named_when_they_look_like_env_names() {
        // These could be a secret pasted without its key, so they are never echoed.
        for bare in ["fake_secret_value_xyz", "ABCDEF123456", "sk_live_Abc123", "MIIEvgIBADANBgkqhkiG9w0BAQEF"] {
            let p = parse(bare);
            assert_eq!(p.warnings.len(), 1);
            assert!(!p.warnings[0].message.contains(bare), "{}", p.warnings[0].message);
        }
        assert!(parse("DATABASE_URL").warnings[0].message.contains("`DATABASE_URL`"));
    }

    #[test]
    fn unterminated_quote_warns_and_continues() {
        let p = parse("A=\"fake_value_1\nB=fake_value_2");
        // The quote swallows B's line because it never closes, so A falls back to one line.
        assert!(p.warnings.iter().any(|w| w.message.contains("never closed")));
        assert!(p.entries.iter().any(|e| e.key == "B"));
    }

    #[test]
    fn warnings_never_contain_values() {
        let p = parse("fake_secret_value_xyz\nBAD KEY=fake_secret_value_xyz\nA=\"fake_secret_value_xyz\" trailing_fake_secret\nA=fake_secret_value_xyz\n");
        for w in &p.warnings {
            assert!(!w.message.contains("fake_secret_value_xyz"), "{}", w.message);
            assert!(!w.message.contains("trailing_fake_secret"), "{}", w.message);
        }
        assert!(!p.warnings.is_empty());
    }

    #[test]
    fn serialize_quotes_when_needed() {
        assert_eq!(format_value("plain"), "plain");
        assert_eq!(format_value("a b"), "a b");
        assert_eq!(format_value(""), "");
        assert_eq!(format_value("a#b"), "\"a#b\"");
        assert_eq!(format_value("a\nb"), "\"a\\nb\"");
        assert_eq!(format_value(" lead"), "\" lead\"");
        assert_eq!(format_value(r"C:\x"), "\"C:\\\\x\"");
    }

    #[test]
    fn example_export_blanks_values_keeps_comments() {
        let p = parse("# db url\nDB=fake_value_1\nOTHER=fake_value_2\n");
        let s = serialize_example(&p.entries);
        assert_eq!(s, "# db url\nDB=\nOTHER=\n");
        assert!(!s.contains("fake_value"));
    }

    // ---- round-trip property test (deterministic generator, no extra dependency) ----

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
            &items[(self.next() as usize) % items.len()]
        }
    }

    fn random_value(r: &mut Lcg) -> String {
        const CHARS: &[char] = &[
            'a', 'B', '7', ' ', '#', '"', '\'', '\\', '\n', '\t', '\r', '=', '$', 'é', '!', '/', ':', '.', '-',
        ];
        let len = r.next() % 12;
        (0..len).map(|_| *r.pick(CHARS)).collect()
    }

    #[test]
    fn serialize_then_parse_round_trips() {
        let mut r = Lcg(42);
        for round in 0..300 {
            let n = 1 + (r.next() % 6) as usize;
            let mut entries = Vec::new();
            for k in 0..n {
                let comment = if r.next() % 3 == 0 {
                    let lines: Vec<String> = (0..(1 + r.next() % 2))
                        .map(|_| {
                            let t: String = (0..(r.next() % 8)).map(|_| *r.pick(&['x', 'Y', '1', ' ', '#', '='])).collect();
                            t.trim_end().to_string()
                        })
                        .collect();
                    Some(lines.join("\n"))
                } else {
                    None
                };
                entries.push(Entry { key: format!("KEY_{round}_{k}"), value: random_value(&mut r), comment, line: 0 });
            }
            let text = serialize(&entries);
            let parsed = parse(&text);
            assert!(parsed.warnings.is_empty(), "round {round}: {:?}\n---\n{text}", parsed.warnings);
            assert_eq!(parsed.entries.len(), entries.len(), "round {round}\n---\n{text}");
            for (a, b) in entries.iter().zip(parsed.entries.iter()) {
                assert_eq!(a.key, b.key);
                assert_eq!(a.value, b.value, "round {round}\n---\n{text}");
                assert_eq!(a.comment, b.comment, "round {round}\n---\n{text}");
            }
            // Idempotent: serializing the parse result gives the same text.
            assert_eq!(serialize(&parsed.entries), text);
        }
    }

    #[test]
    fn messy_input_is_stable_after_one_pass() {
        let messy = "\u{feff}# top\r\nexport A = 'x y' # c1\r\nB=\"l1\nl2\"\r\nC=v # c2\r\nbroken line\r\nA=dup\r\n";
        let first = parse(messy);
        let text = serialize(&first.entries);
        let second = parse(&text);
        assert_eq!(first.entries.len(), second.entries.len());
        for (a, b) in first.entries.iter().zip(second.entries.iter()) {
            assert_eq!((&a.key, &a.value, &a.comment), (&b.key, &b.value, &b.comment));
        }
    }
}
