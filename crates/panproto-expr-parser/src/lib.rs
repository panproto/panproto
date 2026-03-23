//! Haskell-style surface syntax parser for panproto expressions.
//!
//! Parses a human-readable functional language into panproto's native
//! representation types: `Expr`, `InstanceQuery`, `FieldTransform`,
//! `DirectedEquation`, and `WInstance` of `ThExpr`.
//!
//! The surface syntax supports list comprehensions, do-notation, let/where
//! bindings, case/of with guards, lambda expressions, curried application,
//! function composition, operator sections, record syntax with punning,
//! pattern matching, and `->` for graph edge traversal.

/// Token types for the surface syntax.
pub mod token;

/// Lexer with layout insertion pass.
pub mod lexer;

// Re-exports for convenience.
pub use lexer::{LexError, tokenize};
pub use token::{Span, Spanned, Token};
