//! Token-based syntax highlighter for the panproto REPLs.
//!
//! Both the expression REPL (`schema expr repl`) and the theory REPL
//! (`schema theory repl`) accept input that mixes a small set of
//! `:`-prefixed meta-commands with a JSON-flavoured term syntax. This
//! module classifies each character into a [`TokenKind`] so the
//! [`Highlighter`](crate::repl::ReplHelper) can wrap it in ANSI escape
//! codes when the output stream is a TTY.
//!
//! The tokenizer is deliberately small and recognition-only: it never
//! errors, so an unparseable input still gets coloured (the term parser
//! is the source of truth for syntactic correctness, not the
//! highlighter).

use std::borrow::Cow;

const RESET: &str = "\x1b[0m";
const COMMAND: &str = "\x1b[1;36m"; // bold cyan
const KEYWORD: &str = "\x1b[1;35m"; // bold magenta
const STRING: &str = "\x1b[32m"; //   green
const NUMBER: &str = "\x1b[33m"; //   yellow
const OPERATOR: &str = "\x1b[31m"; // red
const COMMENT: &str = "\x1b[90m"; //  bright black (gray)
const PUNCT: &str = "\x1b[37m"; //    light gray
const ERROR: &str = "\x1b[1;31m"; //  bold red
const PROMPT: &str = "\x1b[1;34m"; // bold blue

/// Colour the prompt string so the leading marker stands out from
/// terminal noise. Used by `Highlighter::highlight_prompt` on
/// [`super::ReplHelper`].
pub(super) fn colour_prompt(prompt: &str) -> String {
    format!("{PROMPT}{prompt}{RESET}")
}

/// Token classification used by the highlighter. The variants line up
/// with the colour palette above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Command,
    Keyword,
    String,
    Number,
    Operator,
    Comment,
    Punct,
    Identifier,
    Whitespace,
}

const fn ansi(kind: TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Command => Some(COMMAND),
        TokenKind::Keyword => Some(KEYWORD),
        TokenKind::String => Some(STRING),
        TokenKind::Number => Some(NUMBER),
        TokenKind::Operator => Some(OPERATOR),
        TokenKind::Comment => Some(COMMENT),
        TokenKind::Punct => Some(PUNCT),
        TokenKind::Identifier | TokenKind::Whitespace => None,
    }
}

/// Apply syntax highlighting to `line`, treating `keywords` as
/// language-specific tokens that should be coloured as keywords. The
/// first token, if it begins with `:`, is treated as a REPL meta-command
/// and coloured distinctly.
///
/// Returns the input unchanged when no token would receive non-default
/// colour; this lets callers cheaply skip ANSI emission when the line is
/// boring.
pub(super) fn highlight_line<'a>(line: &'a str, keywords: &[&str]) -> Cow<'a, str> {
    if line.is_empty() {
        return Cow::Borrowed(line);
    }
    let tokens = tokenize(line, keywords);
    if tokens.iter().all(|(k, _)| ansi(*k).is_none()) {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len() + 16);
    for (kind, slice) in tokens {
        if let Some(code) = ansi(kind) {
            out.push_str(code);
            out.push_str(slice);
            out.push_str(RESET);
        } else {
            out.push_str(slice);
        }
    }
    Cow::Owned(out)
}

/// Wrap an error message in the bold-red palette used elsewhere in the
/// REPL. Convenience for callers that print error lines outside of the
/// `Highlighter` trait flow.
#[must_use]
pub fn error(message: &str) -> String {
    format!("{ERROR}{message}{RESET}")
}

fn tokenize<'a>(line: &'a str, keywords: &[&str]) -> Vec<(TokenKind, &'a str)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut at_start = true;
    while i < bytes.len() {
        let start = i;
        let c = bytes[i];

        // Leading `:`-command on the first token of the line.
        if at_start && c == b':' && i + 1 < bytes.len() && is_ident_start(bytes[i + 1]) {
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            out.push((TokenKind::Command, &line[start..i]));
            at_start = false;
            continue;
        }

        // Line comment to end-of-line: `--` (Haskell/SQL/Lua style)
        // or `//` (C/Rust style). Comments span the rest of the line.
        if (c == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-')
            || (c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/')
        {
            out.push((TokenKind::Comment, &line[i..]));
            return out;
        }

        // Double-quoted string with `\\` escapes. Unterminated strings
        // are still highlighted (we colour up to end-of-line and let
        // the term parser report the real error).
        if c == b'"' {
            i += 1;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if b == b'\\' && i < bytes.len() {
                    i += 1;
                    continue;
                }
                if b == b'"' {
                    break;
                }
            }
            out.push((TokenKind::String, &line[start..i]));
            at_start = false;
            continue;
        }

        // Numeric literals: optional minus, digits, optional fractional
        // part, optional exponent. Keeps tokens conservative so a bare
        // `-` on its own falls through to the operator branch.
        if c.is_ascii_digit() || (c == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit())
        {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push((TokenKind::Number, &line[start..i]));
            at_start = false;
            continue;
        }

        // Identifier-or-keyword: first char alphabetic or `_`.
        if is_ident_start(c) {
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            let word = &line[start..i];
            let kind = if keywords.contains(&word) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            };
            out.push((kind, word));
            at_start = false;
            continue;
        }

        // Multi-char operators: `->`, `=>`, `==`, `!=`, `>=`, `<=`,
        // `&&`, `||`, `::`, `..`. Single `=`, `<`, `>`, `+`, `-`, `*`,
        // `/`, `%`, `!`, `&`, `|` are also operators.
        if matches!(
            c,
            b'=' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b'&' | b'|' | b'.'
        ) {
            i += 1;
            // Greedily consume a paired second char if it forms a
            // common two-char operator.
            if i < bytes.len() {
                let next = bytes[i];
                let pair = (c, next);
                if matches!(
                    pair,
                    (b'-' | b'=', b'>')
                        | (b'=' | b'!' | b'>' | b'<', b'=')
                        | (b'&', b'&')
                        | (b'|', b'|')
                        | (b'.', b'.')
                ) {
                    i += 1;
                }
            }
            out.push((TokenKind::Operator, &line[start..i]));
            at_start = false;
            continue;
        }

        // Structural punctuation.
        if matches!(
            c,
            b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',' | b':' | b';'
        ) {
            i += 1;
            out.push((TokenKind::Punct, &line[start..i]));
            at_start = false;
            continue;
        }

        // Whitespace runs.
        if c.is_ascii_whitespace() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push((TokenKind::Whitespace, &line[start..i]));
            continue;
        }

        // Anything else (a single non-ASCII byte or unrecognised
        // ASCII): pass through as an identifier so we don't accidentally
        // strip user input in the highlight path.
        let ch_end = next_char_boundary(line, i);
        out.push((TokenKind::Identifier, &line[i..ch_end]));
        i = ch_end;
        at_start = false;
    }
    out
}

const fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

const fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'\''
}

fn next_char_boundary(s: &str, mut i: usize) -> usize {
    i += 1;
    while !s.is_char_boundary(i) && i < s.len() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_line_is_passthrough() {
        let out = highlight_line("", &[]);
        assert!(matches!(out, Cow::Borrowed("")));
    }

    #[test]
    fn plain_identifier_is_passthrough() {
        let out = highlight_line("foo", &[]);
        assert!(matches!(out, Cow::Borrowed("foo")));
    }

    #[test]
    fn leading_command_is_coloured() {
        let out = highlight_line(":load file", &[]);
        assert!(out.contains(COMMAND));
        assert!(out.contains(":load"));
        // The argument identifier survives, even after ANSI-wrapping
        // the command token.
        assert!(out.contains("file"));
    }

    #[test]
    fn keywords_get_keyword_colour() {
        let out = highlight_line("lambda x", &["lambda", "match", "if"]);
        assert!(out.contains(KEYWORD));
    }

    #[test]
    fn unterminated_string_does_not_panic() {
        let out = highlight_line("\"open string", &[]);
        assert!(out.contains(STRING));
    }

    #[test]
    fn numbers_get_number_colour() {
        let out = highlight_line("[1, 2.5, -3, 1e10]", &[]);
        assert!(out.contains(NUMBER));
        // Ensure all four numeric tokens were classified, not just the first.
        let count = out.matches(NUMBER).count();
        assert_eq!(count, 4, "expected one ANSI run per numeric literal");
    }

    #[test]
    fn arrow_operator_is_one_token() {
        // The output should colour `->` as a single operator run, not
        // as `-` + `>`.
        let out = highlight_line("a -> b", &[]);
        // Two operator opens means we split incorrectly.
        assert_eq!(out.matches(OPERATOR).count(), 1);
    }

    #[test]
    fn comment_swallows_to_end_of_line() {
        let out = highlight_line("foo -- this is a comment", &[]);
        assert!(out.contains(COMMENT));
        assert!(out.contains("this is a comment"));
    }

    #[test]
    fn non_ascii_passes_through() {
        let out = highlight_line("café", &[]);
        assert!(out.contains("café"));
    }
}
