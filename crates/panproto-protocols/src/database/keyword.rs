//! Case-insensitive scanning for the ASCII keywords of the DDL dialects.
//!
//! Every offset produced here indexes the string it was computed from.
//! That is the whole point of the module: `str::to_uppercase` is not
//! length-preserving (`ɐ` occupies two bytes, its uppercase `Ɐ` three, and
//! `ı` shrinks from two bytes to one), so an offset taken from an
//! uppercased copy and used to slice the original can land inside a
//! multi-byte character and panic, or silently cut the wrong substring.
//!
//! Because every DDL keyword is pure ASCII, a byte-wise
//! `eq_ignore_ascii_case` comparison against the original text is exact:
//! a non-ASCII byte is always `>= 0x80` and can never equal an ASCII one,
//! so a match consists entirely of ASCII bytes and its endpoints are
//! guaranteed character boundaries.

/// Byte offset of the first case-insensitive occurrence of the ASCII
/// `keyword` in `haystack`, or `None` when it does not occur.
pub(super) fn find_keyword(haystack: &str, keyword: &str) -> Option<usize> {
    debug_assert!(keyword.is_ascii(), "keyword {keyword:?} must be ASCII");
    let hay = haystack.as_bytes();
    let kw = keyword.as_bytes();
    if kw.is_empty() || kw.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - kw.len()).find(|&i| hay[i..i + kw.len()].eq_ignore_ascii_case(kw))
}

/// Byte offset just past the first case-insensitive occurrence of the
/// ASCII `keyword` in `haystack`.
pub(super) fn find_keyword_end(haystack: &str, keyword: &str) -> Option<usize> {
    find_keyword(haystack, keyword).map(|i| i + keyword.len())
}

/// Whether `haystack` begins with the ASCII `keyword`, ignoring case.
pub(super) fn starts_with_keyword(haystack: &str, keyword: &str) -> bool {
    debug_assert!(keyword.is_ascii(), "keyword {keyword:?} must be ASCII");
    haystack.len() >= keyword.len()
        && haystack.as_bytes()[..keyword.len()].eq_ignore_ascii_case(keyword.as_bytes())
}

/// Whether `haystack` contains the ASCII `keyword`, ignoring case.
pub(super) fn contains_keyword(haystack: &str, keyword: &str) -> bool {
    find_keyword(haystack, keyword).is_some()
}

/// `haystack` with a leading case-insensitive `keyword` removed, or `None`
/// when `haystack` does not start with it.
pub(super) fn strip_keyword_prefix<'a>(haystack: &'a str, keyword: &str) -> Option<&'a str> {
    starts_with_keyword(haystack, keyword).then(|| &haystack[keyword.len()..])
}

/// The identifier that follows `keyword` in `stmt`, with an optional
/// `IF [NOT] EXISTS` guard skipped.
///
/// The identifier runs to the first whitespace, `(`, `,`, or `;`, and is
/// returned with surrounding double-quote or backtick quoting removed.
pub(super) fn name_after_keyword<'a>(
    stmt: &'a str,
    keyword: &str,
    guards: &[&str],
) -> Option<&'a str> {
    let start = find_keyword_end(stmt, keyword)?;
    let mut rest = stmt[start..].trim_start();
    for guard in guards {
        if let Some(stripped) = strip_keyword_prefix(rest, guard) {
            rest = stripped.trim_start();
            break;
        }
    }
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '(' || c == ',' || c == ';')
        .unwrap_or(rest.len());
    let name = rest[..end].trim().trim_matches('"').trim_matches('`');
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn find_keyword_is_case_insensitive() {
        assert_eq!(find_keyword("create table t", "TABLE"), Some(7));
        assert_eq!(find_keyword("CREATE TABLE t", "table"), Some(7));
        assert_eq!(find_keyword("no keyword here", "TABLE"), None);
    }

    #[test]
    fn offsets_index_the_original_string() {
        // `ɐ` uppercases to `Ɐ`, which is one byte longer, so an offset
        // taken from an uppercased copy would land inside the character.
        let s = "CONSTRAINT \u{250}\u{250} FOREIGN KEY(a) REFERENCES \u{250}x(a)";
        let idx = find_keyword(s, "REFERENCES").expect("keyword present");
        assert!(s.is_char_boundary(idx));
        assert_eq!(&s[idx..idx + "REFERENCES".len()], "REFERENCES");
        assert_ne!(idx, s.to_uppercase().find("REFERENCES").unwrap());
    }

    #[test]
    fn name_after_keyword_skips_guards() {
        assert_eq!(
            name_after_keyword(
                "CREATE TABLE IF NOT EXISTS `t` (a INT)",
                "TABLE",
                &["IF NOT EXISTS"]
            ),
            Some("t")
        );
        assert_eq!(
            name_after_keyword("DROP TABLE if exists t;", "TABLE", &["IF EXISTS"]),
            Some("t")
        );
    }

    #[test]
    fn name_after_keyword_ignores_a_guard_phrase_in_a_literal() {
        // The guard must be adjacent to the keyword, not merely present
        // somewhere later in the statement.
        assert_eq!(
            name_after_keyword(
                "CREATE TABLE users (note TEXT DEFAULT 'IF NOT EXISTS')",
                "TABLE",
                &["IF NOT EXISTS"],
            ),
            Some("users")
        );
    }

    #[test]
    fn non_ascii_identifiers_survive() {
        assert_eq!(
            name_after_keyword("CREATE TABLE \u{250}x (a INT)", "TABLE", &["IF NOT EXISTS"]),
            Some("\u{250}x")
        );
    }
}
