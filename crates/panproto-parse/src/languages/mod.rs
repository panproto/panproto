//! Per-language parser and emitter implementations.
//!
//! Each module provides a tree-sitter-based parser and schema emitter for a
//! specific programming language. All parsers use the shared [`LanguageParser`]
//! infrastructure from [`common`], which delegates to the generic
//! [`AstWalker`](crate::walker::AstWalker) with an auto-derived theory.
//!
//! Per-language customization is limited to:
//! - The tree-sitter grammar (Language + `NODE_TYPES`)
//! - [`WalkerConfig`](crate::walker::WalkerConfig) overrides for scope/block detection
//! - File extension mapping

/// Shared language parser implementation.
pub mod common;

/// C full-AST parser.
pub mod c_lang;
/// C++ full-AST parser.
pub mod cpp;
/// C# full-AST parser.
pub mod csharp;
/// Go full-AST parser.
pub mod go_lang;
/// Java full-AST parser.
pub mod java;
/// Kotlin full-AST parser.
pub mod kotlin;
/// Python full-AST parser.
pub mod python;
/// Rust full-AST parser.
pub mod rust_lang;
/// Swift full-AST parser.
pub mod swift;
/// TypeScript and TSX full-AST parsers.
pub mod typescript;
