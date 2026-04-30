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

/// Tracks whether the previously emitted token can act as a value (a
/// thing that could appear before a binary operator). Used to
/// disambiguate unary minus from subtraction: `-2` after a value is
/// the operator `-` followed by the number `2`; otherwise it is the
/// number `-2`.
const fn token_yields_value(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Number | TokenKind::Identifier | TokenKind::Keyword | TokenKind::String
    )
}

/// Closing-bracket punctuation also yields a value (in `(1+2)-3`, the
/// `)` is followed by binary `-`). The catch-all `Punct` kind covers
/// many tokens; we read the actual byte to decide.
const fn last_punct_is_closer(out: &[(TokenKind, &str)]) -> bool {
    if let Some((TokenKind::Punct, s)) = out.last() {
        matches!(s.as_bytes().last(), Some(b')' | b']' | b'}'))
    } else {
        false
    }
}

fn prev_yields_value(out: &[(TokenKind, &str)]) -> bool {
    for (kind, _) in out.iter().rev() {
        if matches!(kind, TokenKind::Whitespace) {
            continue;
        }
        return token_yields_value(*kind) || last_punct_is_closer(out);
    }
    false
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

        // Numeric literals: optional sign, digits, optional fractional
        // part (only when the dot is followed by a digit, so `1..2`
        // does not accidentally consume the first dot), optional
        // exponent. The leading `-` only counts as a sign when no
        // value-yielding token sits to its left; otherwise it is
        // subtraction and falls through to the operator branch.
        let is_sign = c == b'-'
            && i + 1 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
            && !prev_yields_value(&out);
        if c.is_ascii_digit() || is_sign {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit() {
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

    /// Assert that `s` appears as a contiguous span coloured by `colour`,
    /// i.e. wrapped between the colour ANSI prefix and the reset. This
    /// is the single property most highlighter assertions actually want
    /// (and `out.contains(colour)` over-permits — a different token
    /// could happen to use the same colour).
    fn assert_coloured_as(out: &str, s: &str, colour: &str) {
        let needle = format!("{colour}{s}{RESET}");
        assert!(
            out.contains(&needle),
            "expected `{s}` wrapped in colour `{}…{RESET}`; got:\n{out}",
            colour.escape_debug()
        );
    }

    fn count_runs(out: &str, colour: &str) -> usize {
        out.matches(colour).count()
    }

    fn tokens(line: &str, keywords: &[&str]) -> Vec<(TokenKind, String)> {
        tokenize(line, keywords)
            .into_iter()
            .map(|(k, s)| (k, s.to_owned()))
            .collect()
    }

    // --- Passthrough / boring inputs ---

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
    fn whitespace_only_is_passthrough() {
        let out = highlight_line("   \t  ", &[]);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn passthrough_does_not_allocate() {
        // The Cow::Borrowed branch is a load-bearing optimisation:
        // rustyline calls `highlight` on every keystroke, and most
        // keystrokes leave the line "boring" (only identifiers,
        // whitespace). Regress this and the editor allocates per char.
        let out = highlight_line("an_identifier with whitespace", &[]);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    // --- Commands ---

    #[test]
    fn leading_command_is_coloured() {
        let out = highlight_line(":load file", &[]);
        assert_coloured_as(&out, ":load", COMMAND);
        // The argument identifier survives, even after ANSI-wrapping
        // the command token.
        assert!(out.contains("file"));
    }

    #[test]
    fn command_must_be_followed_by_ident() {
        // A bare `:` (no name) is not a command — it should fall
        // through to the punctuation branch so `:` in JSON object
        // syntax is not mis-coloured as a (zero-length) command.
        let out = highlight_line(": foo", &[]);
        assert!(!out.contains(COMMAND), "saw command colour: {out}");
    }

    #[test]
    fn second_colon_is_not_a_command() {
        // Only the *first* token at line start can be a command.
        // A `:foo` that appears mid-line is JSON punctuation followed
        // by an identifier.
        let out = highlight_line("a :foo", &[]);
        assert_eq!(count_runs(&out, COMMAND), 0);
    }

    #[test]
    fn command_with_leading_whitespace_is_still_a_command() {
        // The underlying engine trims its input before dispatch, so
        // ` :load` and `:load` mean the same thing. The highlighter
        // should match: leading whitespace doesn't break command
        // detection. (`at_start` only flips to false after a
        // non-whitespace token.)
        let out = highlight_line(" :load", &[]);
        assert!(out.contains(COMMAND));
    }

    // --- Keywords ---

    #[test]
    fn keywords_get_keyword_colour() {
        let out = highlight_line("lambda x", &["lambda", "match", "if"]);
        assert_coloured_as(&out, "lambda", KEYWORD);
    }

    #[test]
    fn keyword_match_is_word_bounded() {
        // `lambdax` should not match the `lambda` keyword; word-boundary
        // is the identifier tokenizer, which consumes the trailing `x`
        // into the same token.
        let out = highlight_line("lambdax", &["lambda"]);
        assert!(!out.contains(KEYWORD), "`lambdax` was mis-coloured: {out}");
    }

    // --- Strings ---

    #[test]
    fn unterminated_string_does_not_panic() {
        let out = highlight_line("\"open string", &[]);
        assert!(out.contains(STRING));
    }

    #[test]
    fn string_with_escaped_quote() {
        let out = highlight_line(r#""esc \" still in""#, &[]);
        assert_eq!(count_runs(&out, STRING), 1, "should be one string run");
    }

    #[test]
    fn empty_string_token() {
        let toks = tokens("\"\"", &[]);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].0, TokenKind::String);
        assert_eq!(toks[0].1, "\"\"");
    }

    // --- Numbers ---

    #[test]
    fn numbers_get_number_colour() {
        let out = highlight_line("[1, 2.5, -3, 1e10]", &[]);
        assert!(out.contains(NUMBER));
        // Four numeric tokens, four colour runs.
        assert_eq!(count_runs(&out, NUMBER), 4);
    }

    #[test]
    fn scientific_with_negative_exponent() {
        let toks = tokens("1.5e-10", &[]);
        // The whole literal must be a single number token; `e-10` is
        // the exponent, not a sign-applied subtraction.
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0], (TokenKind::Number, "1.5e-10".to_owned()));
    }

    #[test]
    fn unary_minus_after_value_is_subtraction() {
        // BUG REGRESSION GUARD: `1-2` previously tokenised as `1` +
        // `-2` (two numbers). Correct behaviour is `1` + `-` + `2`
        // because the `-` follows a value-yielding token.
        let toks = tokens("1-2", &[]);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], (TokenKind::Number, "1".to_owned()));
        assert_eq!(toks[1], (TokenKind::Operator, "-".to_owned()));
        assert_eq!(toks[2], (TokenKind::Number, "2".to_owned()));
    }

    #[test]
    fn unary_minus_after_close_paren_is_subtraction() {
        let toks = tokens("(1)-2", &[]);
        // `(` `1` `)` `-` `2`
        assert_eq!(toks.len(), 5);
        assert_eq!(toks[3], (TokenKind::Operator, "-".to_owned()));
        assert_eq!(toks[4], (TokenKind::Number, "2".to_owned()));
    }

    #[test]
    fn unary_minus_at_line_start_is_a_sign() {
        let toks = tokens("-3", &[]);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0], (TokenKind::Number, "-3".to_owned()));
    }

    #[test]
    fn unary_minus_after_operator_is_a_sign() {
        // `2*-3` should be `2` * `-3`, not `2 * - 3`.
        let toks = tokens("2*-3", &[]);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], (TokenKind::Number, "2".to_owned()));
        assert_eq!(toks[1], (TokenKind::Operator, "*".to_owned()));
        assert_eq!(toks[2], (TokenKind::Number, "-3".to_owned()));
    }

    #[test]
    fn double_dot_after_number_does_not_consume_dot() {
        // BUG REGRESSION GUARD: `1..2` previously consumed the first
        // dot into the number, producing `1.` + `.` + `2`. Correct
        // behaviour is `1` + `..` + `2`.
        let toks = tokens("1..2", &[]);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], (TokenKind::Number, "1".to_owned()));
        assert_eq!(toks[1], (TokenKind::Operator, "..".to_owned()));
        assert_eq!(toks[2], (TokenKind::Number, "2".to_owned()));
    }

    #[test]
    fn trailing_dot_does_not_join_following_identifier() {
        // `1.foo` → `1` + `.` + `foo`. We could be more permissive,
        // but matching JSON's strict number grammar produces fewer
        // surprises for the user (the term parser will reject `1.`
        // anyway).
        let toks = tokens("1.foo", &[]);
        assert_eq!(toks.len(), 3);
        assert_eq!(toks[0], (TokenKind::Number, "1".to_owned()));
        assert_eq!(toks[1], (TokenKind::Operator, ".".to_owned()));
        assert_eq!(toks[2], (TokenKind::Identifier, "foo".to_owned()));
    }

    // --- Operators / punctuation ---

    #[test]
    fn arrow_operator_is_one_token() {
        let out = highlight_line("a -> b", &[]);
        assert_eq!(count_runs(&out, OPERATOR), 1);
    }

    #[test]
    fn ampersand_pair_is_one_operator() {
        let toks = tokens("a && b", &[]);
        assert_eq!(toks.len(), 5); // ident, ws, op, ws, ident
        assert_eq!(toks[2], (TokenKind::Operator, "&&".to_owned()));
    }

    #[test]
    fn punctuation_is_punct_kind() {
        let toks = tokens("{}", &[]);
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].0, TokenKind::Punct);
        assert_eq!(toks[1].0, TokenKind::Punct);
    }

    // --- Comments ---

    #[test]
    fn comment_swallows_to_end_of_line() {
        let out = highlight_line("foo -- this is a comment", &[]);
        assert_coloured_as(&out, "-- this is a comment", COMMENT);
    }

    #[test]
    fn slashslash_comment_works_too() {
        let out = highlight_line("foo // c", &[]);
        assert_coloured_as(&out, "// c", COMMENT);
    }

    #[test]
    fn comment_inside_string_is_not_a_comment() {
        let out = highlight_line(r#""path//inside""#, &[]);
        // Only the string colour should appear; no comment colour.
        assert!(!out.contains(COMMENT));
        assert_eq!(count_runs(&out, STRING), 1);
    }

    // --- Non-ASCII / robustness ---

    #[test]
    fn non_ascii_passes_through() {
        let out = highlight_line("café", &[]);
        assert!(out.contains("café"));
    }

    #[test]
    fn non_ascii_in_string_passes_through() {
        let out = highlight_line("\"café\"", &[]);
        assert!(out.contains("café"));
        assert_eq!(count_runs(&out, STRING), 1);
    }

    #[test]
    fn very_long_line_does_not_overflow() {
        // Round-trip a kilobyte of repeated tokens. Smoke test that
        // we don't blow up on growth or have an off-by-one.
        let line = "[".to_owned() + &"1, ".repeat(200) + "1]";
        let _ = highlight_line(&line, &[]);
    }

    #[test]
    fn error_helper_wraps_in_bold_red() {
        let out = error("boom");
        assert!(out.contains(ERROR));
        assert!(out.contains("boom"));
        assert!(out.ends_with(RESET));
    }

    #[test]
    fn colour_prompt_wraps_in_blue() {
        let out = colour_prompt("expr> ");
        assert!(out.contains(PROMPT));
        assert!(out.contains("expr> "));
        assert!(out.ends_with(RESET));
    }
}
