//! Token types for the Haskell-style surface syntax.

use logos::Logos;
use std::fmt;

/// Source span (byte offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Start byte offset (inclusive).
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// A token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    /// The token kind.
    pub token: Token,
    /// Source span.
    pub span: Span,
}

/// Token kinds produced by the lexer.
///
/// Keywords are recognized during lexing; identifiers that happen to match
/// a keyword are emitted as the keyword token, not as `Ident`.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t]+")]
#[logos(skip(r"--[^\n]*", allow_greedy = true))]
pub enum Token {
    // ── Keywords ──────────────────────────────────────────────────
    /// `do` keyword (layout block).
    #[token("do")]
    Do,
    /// `let` keyword (layout block).
    #[token("let")]
    Let,
    /// `in` keyword.
    #[token("in")]
    In,
    /// `where` keyword (layout block).
    #[token("where")]
    Where,
    /// `if` keyword.
    #[token("if")]
    If,
    /// `then` keyword.
    #[token("then")]
    Then,
    /// `else` keyword.
    #[token("else")]
    Else,
    /// `case` keyword.
    #[token("case")]
    Case,
    /// `of` keyword (layout block).
    #[token("of")]
    Of,
    /// `guard` keyword.
    #[token("guard")]
    Guard,
    /// `not` keyword (logical negation).
    #[token("not")]
    Not,
    /// `mod` keyword (modulo).
    #[token("mod")]
    ModKw,
    /// `div` keyword (integer division).
    #[token("div")]
    DivKw,
    /// `otherwise` keyword (catch-all guard).
    #[token("otherwise")]
    Otherwise,

    // ── Literals ──────────────────────────────────────────────────
    /// Boolean literal `True`.
    #[token("True")]
    True,
    /// Boolean literal `False`.
    #[token("False")]
    False,
    /// `Nothing` literal (absent value).
    #[token("Nothing")]
    Nothing,

    /// Integer literal (decimal or `0x` hex).
    #[regex(r"0x[0-9a-fA-F]+", |lex| i64::from_str_radix(&lex.slice()[2..], 16).ok())]
    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok(), priority = 2)]
    Int(i64),

    /// Floating-point literal.
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().parse::<f64>().ok())]
    Float(f64),

    /// String literal (double-quoted, backslash escapes).
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        Some(s[1..s.len()-1].to_string())
    })]
    Str(String),

    // ── Identifiers ──────────────────────────────────────────────
    /// Lower-case identifier (variables, fields).
    #[regex(r"[a-z_][a-zA-Z0-9_']*", |lex| lex.slice().to_string(), priority = 1)]
    Ident(String),

    /// Upper-case identifier (constructors, types).
    #[regex(r"[A-Z][a-zA-Z0-9_']*", |lex| lex.slice().to_string())]
    UpperIdent(String),

    // ── Operators ────────────────────────────────────────────────
    /// `->` arrow (function type, edge traversal).
    #[token("->")]
    Arrow,
    /// `<-` left arrow (generators, monadic bind).
    #[token("<-")]
    LeftArrow,
    /// `=>` fat arrow (constraints, pattern clauses).
    #[token("=>")]
    FatArrow,
    /// `::` type annotation.
    #[token("::")]
    DoubleColon,
    /// `..` range operator.
    #[token("..")]
    DotDot,
    /// `==` equality.
    #[token("==")]
    EqEq,
    /// `/=` inequality.
    #[token("/=")]
    Neq,
    /// `<=` less-than-or-equal.
    #[token("<=")]
    Lte,
    /// `>=` greater-than-or-equal.
    #[token(">=")]
    Gte,
    /// `&&` logical and.
    #[token("&&")]
    AndAnd,
    /// `||` logical or.
    #[token("||")]
    OrOr,
    /// `++` list concatenation.
    #[token("++")]
    PlusPlus,
    /// `+` addition.
    #[token("+")]
    Plus,
    /// `-` subtraction / negation.
    #[token("-")]
    Minus,
    /// `*` multiplication.
    #[token("*")]
    Star,
    /// `/` division.
    #[token("/")]
    Slash,
    /// `%` modulo.
    #[token("%")]
    Percent,
    /// `<` less-than.
    #[token("<")]
    Lt,
    /// `>` greater-than.
    #[token(">")]
    Gt,
    /// `=` binding / definition.
    #[token("=")]
    Eq,
    /// `.` field access / composition.
    #[token(".")]
    Dot,
    /// `,` separator.
    #[token(",")]
    Comma,
    /// `:` cons / type annotation.
    #[token(":")]
    Colon,
    /// `|` guard / comprehension separator.
    #[token("|")]
    Pipe,
    /// `\` lambda introducer.
    #[token("\\")]
    Backslash,
    /// `@` as-pattern.
    #[token("@")]
    At,
    /// `&` reference.
    #[token("&")]
    Ampersand,
    /// `` ` `` infix function application.
    #[token("`")]
    Backtick,
    /// `!` strict application.
    #[token("!")]
    Bang,

    // ── Delimiters ───────────────────────────────────────────────
    /// `(` open parenthesis.
    #[token("(")]
    LParen,
    /// `)` close parenthesis.
    #[token(")")]
    RParen,
    /// `[` open bracket.
    #[token("[")]
    LBracket,
    /// `]` close bracket.
    #[token("]")]
    RBracket,
    /// `{` open brace.
    #[token("{")]
    LBrace,
    /// `}` close brace.
    #[token("}")]
    RBrace,

    // ── Layout (virtual tokens inserted by the layout pass) ──────
    /// Indentation increased (opens a block).
    Indent,
    /// Indentation decreased (closes a block).
    Dedent,
    /// Newline at the same indentation level (separates declarations).
    Newline,

    // ── Special ──────────────────────────────────────────────────
    /// End of input.
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ident(s) | Self::UpperIdent(s) | Self::Str(s) => write!(f, "{s}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::Arrow => write!(f, "->"),
            Self::LeftArrow => write!(f, "<-"),
            Self::FatArrow => write!(f, "=>"),
            Self::EqEq => write!(f, "=="),
            Self::Neq => write!(f, "/="),
            Self::AndAnd => write!(f, "&&"),
            Self::OrOr => write!(f, "||"),
            Self::PlusPlus => write!(f, "++"),
            Self::DoubleColon => write!(f, "::"),
            Self::DotDot => write!(f, ".."),
            _ => write!(f, "{self:?}"),
        }
    }
}
