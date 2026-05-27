//! Grammar cassettes: per-grammar defaults for external scanner tokens.
//!
//! A [`GrammarCassette`] provides default text for external scanner
//! tokens that have no anonymous `ALIAS`, no `CHOICE` equivalence, and no
//! `CstComplement` data. This is the static equivalent of the
//! `CstComplement`: it provides token text when no dynamic (parse-time)
//! text is available.
//!
//! The cassette is the ONLY grammar-specific code in the emission
//! pipeline. The emission core (`emit_pretty.rs`) is 100% generic.
//! Cassettes exist because `grammar.json` is an incomplete
//! specification: external scanner tokens can produce text that varies
//! at runtime, and `grammar.json` records no information about what
//! that text is.

use std::sync::Arc;

/// Per-grammar defaults for opaque external scanner tokens.
///
/// When an external token has no anonymous `ALIAS`, no `CHOICE`
/// equivalence, and no `CstComplement` data, the cassette provides a
/// last-resort default text. Returns `None` to signal that the token
/// cannot be emitted without parse context.
pub trait GrammarCassette: Send + Sync {
    /// Returns the default text for an external scanner token, or
    /// `None` if the token cannot be emitted without parse context.
    fn external_token_default(&self, token_name: &str) -> Option<&str>;
}

struct NullCassette;

impl GrammarCassette for NullCassette {
    fn external_token_default(&self, _token_name: &str) -> Option<&str> {
        None
    }
}

struct PythonCassette;

impl GrammarCassette for PythonCassette {
    fn external_token_default(&self, token_name: &str) -> Option<&str> {
        match token_name {
            "string_start" | "string_end" => Some("\""),
            _ => None,
        }
    }
}

struct JuliaCassette;

impl GrammarCassette for JuliaCassette {
    fn external_token_default(&self, token_name: &str) -> Option<&str> {
        match token_name {
            "_end_str" | "_immediate_string_start" => Some("\""),
            "_end_cmd" | "_immediate_command_start" => Some("`"),
            "_immediate_paren" | "_immediate_bracket" | "_immediate_brace" => Some(""),
            _ => None,
        }
    }
}

struct RubyCassette;

impl GrammarCassette for RubyCassette {
    fn external_token_default(&self, token_name: &str) -> Option<&str> {
        match token_name {
            "_line_break" => Some("\n"),
            "_no_line_break" => Some(""),
            _ => None,
        }
    }
}

struct OcamlCassette;

impl GrammarCassette for OcamlCassette {
    fn external_token_default(&self, token_name: &str) -> Option<&str> {
        match token_name {
            "_quoted_string_start" => Some("{|"),
            "_quoted_string_end" => Some("|}"),
            _ => None,
        }
    }
}

/// Look up the cassette for a grammar by protocol name.
#[must_use]
pub fn cassette_for(protocol: &str) -> Arc<dyn GrammarCassette> {
    match protocol {
        "python" => Arc::new(PythonCassette),
        "julia" => Arc::new(JuliaCassette),
        "ruby" => Arc::new(RubyCassette),
        "ocaml" | "ocaml_interface" => Arc::new(OcamlCassette),
        _ => Arc::new(NullCassette),
    }
}
