//! Lexer producing a sequence of spanned tokens from source text.
//!
//! Uses logos for fast regex-based tokenization, then applies a layout
//! insertion pass to convert indentation into explicit `Indent`/`Dedent`/
//! `Newline` tokens (the GHC approach).

use logos::Logos;

use crate::token::{Span, Spanned, Token};

/// Tokenize source text into a sequence of spanned tokens.
///
/// This performs two passes:
/// 1. Raw tokenization via logos (skips whitespace within lines).
/// 2. Layout insertion (converts indentation to virtual tokens).
///
/// # Errors
///
/// Returns an error if the input contains an unrecognized token.
pub fn tokenize(input: &str) -> Result<Vec<Spanned>, LexError> {
    let raw = raw_tokenize(input)?;
    Ok(insert_layout(input, &raw))
}

/// A lexer error with source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// Byte offset of the unrecognized token.
    pub offset: usize,
    /// The problematic character(s).
    pub text: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unrecognized token at byte {}: {:?}",
            self.offset, self.text
        )
    }
}

impl std::error::Error for LexError {}

/// Raw tokenization via logos (no layout insertion).
fn raw_tokenize(input: &str) -> Result<Vec<Spanned>, LexError> {
    let mut tokens = Vec::new();
    let mut lexer = Token::lexer(input);

    while let Some(result) = lexer.next() {
        let span = lexer.span();
        if let Ok(token) = result {
            tokens.push(Spanned {
                token,
                span: Span {
                    start: span.start,
                    end: span.end,
                },
            });
        } else {
            // Check if this is a newline (which logos skips).
            let slice = &input[span.clone()];
            if slice.contains('\n') || slice.contains('\r') {
                // Newlines are handled by the layout pass, not as tokens.
                continue;
            }
            return Err(LexError {
                offset: span.start,
                text: slice.to_string(),
            });
        }
    }

    tokens.push(Spanned {
        token: Token::Eof,
        span: Span {
            start: input.len(),
            end: input.len(),
        },
    });

    Ok(tokens)
}

/// Layout insertion pass (GHC-style).
///
/// Scans the raw token stream and the original source text. When a layout
/// keyword (`let`, `where`, `do`, `of`) is followed by a newline and
/// increased indentation, inserts `Indent`. When indentation decreases,
/// inserts `Dedent`. At the same indentation, inserts `Newline` to
/// separate declarations.
///
/// If the layout keyword is followed by `{`, layout is suppressed
/// (explicit delimiters).
fn insert_layout(input: &str, raw: &[Spanned]) -> Vec<Spanned> {
    if raw.is_empty() {
        return vec![];
    }

    let mut result = Vec::with_capacity(raw.len());
    let mut indent_stack: Vec<usize> = vec![0]; // column stack
    let mut cursor = Cursor::new();
    let mut prev_line = 0;
    let mut prev_end = 0;

    for spanned in raw {
        cursor.advance_to(input, spanned.span.start);
        let cur_line = cursor.line;
        let cur_col = cursor.column();

        // If we moved to a new line, check indentation.
        if cur_line > prev_line {
            let current_indent = *indent_stack.last().unwrap_or(&0);

            match cur_col.cmp(&current_indent) {
                std::cmp::Ordering::Greater => {
                    // Check if previous token was a layout keyword.
                    let prev_is_layout = result.last().is_some_and(|s: &Spanned| {
                        matches!(s.token, Token::Let | Token::Where | Token::Do | Token::Of)
                    });
                    if prev_is_layout {
                        indent_stack.push(cur_col);
                        result.push(Spanned {
                            token: Token::Indent,
                            span: Span {
                                start: spanned.span.start,
                                end: spanned.span.start,
                            },
                        });
                    }
                }
                std::cmp::Ordering::Less => {
                    // Dedent: pop indent stack until we match or go below.
                    while indent_stack.len() > 1 && *indent_stack.last().unwrap_or(&0) > cur_col {
                        indent_stack.pop();
                        result.push(Spanned {
                            token: Token::Dedent,
                            span: Span {
                                start: spanned.span.start,
                                end: spanned.span.start,
                            },
                        });
                    }
                }
                std::cmp::Ordering::Equal => {
                    // Same indentation: insert Newline separator.
                    // Only if we're inside a layout block (indent_stack.len() > 1).
                    if indent_stack.len() > 1 {
                        result.push(Spanned {
                            token: Token::Newline,
                            span: Span {
                                start: spanned.span.start,
                                end: spanned.span.start,
                            },
                        });
                    }
                }
            }
        }

        result.push(spanned.clone());
        prev_line = cur_line;
        prev_end = spanned.span.end;
    }

    // Close any remaining open layout blocks.
    while indent_stack.len() > 1 {
        indent_stack.pop();
        result.push(Spanned {
            token: Token::Dedent,
            span: Span {
                start: prev_end,
                end: prev_end,
            },
        });
    }

    result
}

/// A line-and-column position walked forward through the source.
///
/// The layout pass reads every token's position in ascending offset order, so
/// the cursor advances over each byte of the input exactly once across the
/// whole pass. Recomputing a position from the start of the input instead
/// would make the pass quadratic in the source's length.
struct Cursor {
    /// Byte offset the cursor has advanced to.
    offset: usize,
    /// 0-indexed line number at `offset`.
    line: usize,
    /// Byte offset of the start of the line holding `offset`.
    line_start: usize,
}

impl Cursor {
    const fn new() -> Self {
        Self {
            offset: 0,
            line: 0,
            line_start: 0,
        }
    }

    /// Advance to `target`, counting the lines crossed on the way.
    ///
    /// `target` is never behind the cursor: the token stream is in ascending
    /// offset order.
    fn advance_to(&mut self, input: &str, target: usize) {
        let target = target.max(self.offset).min(input.len());
        for (index, byte) in input.as_bytes()[self.offset..target].iter().enumerate() {
            if *byte == b'\n' {
                self.line += 1;
                self.line_start = self.offset + index + 1;
            }
        }
        self.offset = target;
    }

    /// The 0-indexed column: bytes from the start of the current line.
    const fn column(&self) -> usize {
        self.offset - self.line_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_expression() {
        let tokens = tokenize("1 + 2").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Int(1));
        assert_eq!(tokens[1].token, Token::Plus);
        assert_eq!(tokens[2].token, Token::Int(2));
        assert_eq!(tokens[3].token, Token::Eof);
    }

    #[test]
    fn keywords_recognized() {
        let tokens = tokenize("let x = 1 in x").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Let);
        assert_eq!(tokens[1].token, Token::Ident("x".into()));
        assert_eq!(tokens[2].token, Token::Eq);
        assert_eq!(tokens[3].token, Token::Int(1));
        assert_eq!(tokens[4].token, Token::In);
    }

    #[test]
    fn string_literal() {
        let tokens = tokenize(r#""hello world""#).unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Str("hello world".into()));
    }

    #[test]
    fn operators() {
        let tokens = tokenize("a -> b && c || d").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Ident("a".into()));
        assert_eq!(tokens[1].token, Token::Arrow);
        assert_eq!(tokens[2].token, Token::Ident("b".into()));
        assert_eq!(tokens[3].token, Token::AndAnd);
        assert_eq!(tokens[5].token, Token::OrOr);
    }

    #[test]
    fn layout_let_block() {
        let input = "let\n  x = 1\n  y = 2\nin x";
        let tokens = tokenize(input).unwrap_or_default();
        let kinds: Vec<&Token> = tokens.iter().map(|s| &s.token).collect();
        // Should have: Let, Indent, Ident(x), Eq, Int(1), Newline,
        //              Ident(y), Eq, Int(2), Dedent, In, Ident(x), Eof
        assert!(kinds.contains(&&Token::Indent));
        assert!(kinds.contains(&&Token::Newline));
        assert!(kinds.contains(&&Token::Dedent));
    }

    #[test]
    fn comprehension_tokens() {
        let tokens = tokenize("[ a | a <- xs, a > 0 ]").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::LBracket);
        assert_eq!(tokens[1].token, Token::Ident("a".into()));
        assert_eq!(tokens[2].token, Token::Pipe);
        assert_eq!(tokens[3].token, Token::Ident("a".into()));
        assert_eq!(tokens[4].token, Token::LeftArrow);
    }

    #[test]
    fn comment_skipped() {
        let tokens = tokenize("x -- this is a comment\ny").unwrap_or_default();
        let idents: Vec<&str> = tokens
            .iter()
            .filter_map(|s| {
                if let Token::Ident(ref name) = s.token {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(idents, vec!["x", "y"]);
    }

    #[test]
    fn float_literal() {
        let tokens = tokenize("3.125").unwrap_or_default();
        assert!(matches!(tokens[0].token, Token::Float(f) if (f - 3.125).abs() < f64::EPSILON));
    }

    #[test]
    fn hex_literal() {
        let tokens = tokenize("0xFF").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Int(255));
    }

    #[test]
    fn upper_ident() {
        let tokens = tokenize("True Nothing MyType").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::True);
        assert_eq!(tokens[1].token, Token::Nothing);
        assert_eq!(tokens[2].token, Token::UpperIdent("MyType".into()));
    }

    #[test]
    fn lambda_tokens() {
        let tokens = tokenize("\\x -> x + 1").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Backslash);
        assert_eq!(tokens[1].token, Token::Ident("x".into()));
        assert_eq!(tokens[2].token, Token::Arrow);
    }

    #[test]
    fn edge_traversal() {
        let tokens = tokenize("doc -> layers -> annotations").unwrap_or_default();
        assert_eq!(tokens[0].token, Token::Ident("doc".into()));
        assert_eq!(tokens[1].token, Token::Arrow);
        assert_eq!(tokens[2].token, Token::Ident("layers".into()));
        assert_eq!(tokens[3].token, Token::Arrow);
        assert_eq!(tokens[4].token, Token::Ident("annotations".into()));
    }
}
