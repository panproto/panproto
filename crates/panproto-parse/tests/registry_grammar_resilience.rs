//! Grammars whose `tags.scm` uses captures outside the tree-sitter-tags
//! vocabulary, or whose `node-types.json` carries a non-node metadata
//! marker, must still register for parse/emit: a secondary feature's
//! failure must not drop the whole grammar.
//!
//! - C# ships `@module` and AL ships a `@_test_attr` `#match?` helper;
//!   both are rejected by `TagsConfiguration`. Scope detection degrades
//!   to a no-op; parsing still works.
//! - Erlang's `node-types.json` ends with a `{"@generated": true}`
//!   marker that has no `type` field; it is skipped during extraction.

// Each test gates on a specific grammar's `lang-*` feature, none of which
// the workspace-default `group-core` turns on as a cfg. Gate the whole
// file on their union so the shared `assert_registered` helper is never
// orphaned dead code, and the tests run when those features are enabled.
#![cfg(all(
    feature = "grammars",
    any(
        feature = "lang-csharp",
        feature = "lang-al",
        feature = "lang-erlang"
    )
))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::{EmitVerificationStatus, ParserRegistry};

/// A registered protocol never reports `Unsupported` (that status is
/// reserved for protocols absent from the registry).
fn assert_registered(reg: &ParserRegistry, protocol: &str) {
    assert_ne!(
        reg.emit_verification_status(protocol),
        EmitVerificationStatus::Unsupported,
        "{protocol} must be registered (was dropped by a secondary-feature failure)"
    );
}

#[test]
#[cfg(feature = "lang-csharp")]
fn csharp_registers_despite_unsupported_tags_capture() {
    assert_registered(&ParserRegistry::new(), "csharp");
}

#[test]
#[cfg(feature = "lang-al")]
fn al_registers_despite_match_helper_capture() {
    assert_registered(&ParserRegistry::new(), "al");
}

#[test]
#[cfg(feature = "lang-erlang")]
fn erlang_registers_despite_generated_node_type_marker() {
    assert_registered(&ParserRegistry::new(), "erlang");
}
