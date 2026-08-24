use serde::Serialize;

/// One assignment in a `.env` file.
///
/// Nothing in this module logs, caches or derives anything from `value`. The
/// file is the only place a secret lives; this is a view of it for one render,
/// and it is read fresh every time rather than kept anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvEntry {
    pub key: String,
    pub value: String,
    /// 1-based, so the editor can jump to it.
    pub line: usize,
}

/// Reads the assignments out of a `.env` file.
///
/// Deliberately forgiving: this is for reading someone's real file, not for
/// validating it, so anything unrecognisable is skipped rather than failing the
/// whole view.
pub fn parse(text: &str) -> Vec<EnvEntry> {
    text.lines()
        .enumerate()
        .filter_map(|(index, raw)| {
            parse_line(raw).map(|(key, value)| EnvEntry {
                key,
                value,
                line: index + 1,
            })
        })
        .collect()
}

fn parse_line(raw: &str) -> Option<(String, String)> {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    // `export KEY=value` is common in files meant to be sourced as well.
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();

    let (key, rest) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(is_key_char) {
        return None;
    }

    Some((key.to_string(), parse_value(rest.trim())))
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

fn parse_value(raw: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = raw.strip_prefix(quote)
            && let Some(inner) = inner.strip_suffix(quote)
        {
            // Only double quotes carry escapes, the same as a shell.
            return if quote == '"' {
                inner.replace("\\n", "\n").replace("\\\"", "\"")
            } else {
                inner.to_string()
            };
        }
    }

    // An unquoted value ends at a trailing comment, but only one introduced by
    // whitespace — a `#` inside a token is part of the value.
    match raw.split_once(" #") {
        Some((value, _)) => value.trim_end().to_string(),
        None => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Vec<(String, String)> {
        parse(text)
            .into_iter()
            .map(|entry| (entry.key, entry.value))
            .collect()
    }

    #[test]
    fn reads_plain_assignments() {
        assert_eq!(
            parsed("DATABASE_URL=postgres://localhost/app"),
            vec![("DATABASE_URL".into(), "postgres://localhost/app".into())]
        );
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let text = "# a comment\n\nKEY=value\n\n# another\n";
        assert_eq!(parsed(text), vec![("KEY".into(), "value".into())]);
    }

    #[test]
    fn accepts_the_export_prefix() {
        assert_eq!(
            parsed("export API_KEY=abc"),
            vec![("API_KEY".into(), "abc".into())]
        );
    }

    #[test]
    fn strips_matching_quotes() {
        assert_eq!(
            parsed(r#"A="quoted value""#),
            vec![("A".into(), "quoted value".into())]
        );
        assert_eq!(parsed("B='single'"), vec![("B".into(), "single".into())]);
    }

    #[test]
    fn a_hash_inside_a_value_is_part_of_it() {
        // Losing half a secret to a comment rule would be worse than useless.
        assert_eq!(
            parsed("JWT_SECRET=abc#def"),
            vec![("JWT_SECRET".into(), "abc#def".into())]
        );
    }

    #[test]
    fn a_trailing_comment_after_whitespace_is_dropped() {
        assert_eq!(
            parsed("PORT=3000 # the dev port"),
            vec![("PORT".into(), "3000".into())]
        );
    }

    #[test]
    fn a_hash_inside_quotes_survives() {
        assert_eq!(
            parsed(r##"KEY="value # not a comment""##),
            vec![("KEY".into(), "value # not a comment".into())]
        );
    }

    #[test]
    fn an_empty_value_is_still_an_entry() {
        assert_eq!(parsed("EMPTY="), vec![("EMPTY".into(), String::new())]);
    }

    #[test]
    fn line_numbers_point_at_the_original_file() {
        let entries = parse("# header\n\nFIRST=1\nSECOND=2\n");
        assert_eq!(entries[0].line, 3);
        assert_eq!(entries[1].line, 4);
    }

    #[test]
    fn lines_that_are_not_assignments_are_skipped() {
        assert!(parsed("just some words\n").is_empty());
        assert!(parsed("BAD KEY=value\n").is_empty());
    }

    #[test]
    fn escapes_inside_double_quotes_are_unescaped() {
        assert_eq!(
            parsed(r#"MULTI="line one\nline two""#),
            vec![("MULTI".into(), "line one\nline two".into())]
        );
    }
}
