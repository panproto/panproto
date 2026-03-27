#![allow(unsafe_code)]
//! Pre-compiled tree-sitter grammars for panproto.
//!
//! This crate bundles up to 248 tree-sitter grammars, compiled from vendored C sources.
//! Each grammar is gated behind a `lang-{name}` feature flag. Group features like
//! `group-core`, `group-web`, `group-all` etc. enable sets of languages at once.
//!
//! The default feature is `group-core` (GitHub's top 10 languages + Rust).

#[allow(
    clippy::vec_init_then_push,
    clippy::match_same_arms,
    clippy::must_use_candidate,
    clippy::redundant_pub_crate,
    unreachable_pub
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/grammar_table.rs"));
}
use generated::{enabled_grammars, ext_to_lang};

/// A compiled tree-sitter grammar with its metadata.
pub struct Grammar {
    /// Protocol name (e.g. `"python"`, `"typescript"`).
    pub name: &'static str,
    /// File extensions this grammar handles (e.g. `["py", "pyi"]`).
    pub extensions: &'static [&'static str],
    /// The tree-sitter `Language` object for parsing.
    pub language: tree_sitter::Language,
    /// Raw `node-types.json` bytes for theory extraction.
    pub node_types: &'static [u8],
}

/// Return all grammars enabled by feature flags.
///
/// Each grammar provides a `Language` for parsing and `node_types` JSON for
/// theory extraction. The returned list is sorted by grammar name.
#[must_use]
pub fn grammars() -> Vec<Grammar> {
    enabled_grammars()
        .into_iter()
        .map(|e| {
            let language = unsafe {
                // SAFETY: `tree_sitter_{name}()` is an extern "C" function that returns
                // a valid `TSLanguage*` pointer from the compiled C grammar. We cast the
                // raw pointer back to the correct function pointer type. This is the
                // standard tree-sitter FFI pattern used by every grammar crate.
                let f: unsafe extern "C" fn() -> *const () = std::mem::transmute(e.language_fn_ptr);
                tree_sitter_language::LanguageFn::from_raw(f).into()
            };
            Grammar {
                name: e.name,
                extensions: e.extensions,
                language,
                node_types: e.node_types,
            }
        })
        .collect()
}

/// Check if a specific grammar is available (enabled by feature flags).
#[must_use]
pub fn has_grammar(name: &str) -> bool {
    enabled_grammars().iter().any(|e| e.name == name)
}

/// Map a file extension to its grammar name.
///
/// Returns `None` if the extension is not recognized among enabled grammars.
#[must_use]
pub fn extension_to_language(ext: &str) -> Option<&'static str> {
    ext_to_lang(ext)
}

/// Return the number of enabled grammars.
#[must_use]
pub fn grammar_count() -> usize {
    enabled_grammars().len()
}
