//! Grammar cassettes: per-grammar defaults for external scanner tokens.
//!
//! A `GrammarCassette` provides text for external scanner tokens that
//! tree-sitter's `grammar.json` cannot resolve on its own:
//!
//! * Anonymous `ALIAS { content: SYMBOL ext, named: false, value: V }`
//!   yields the alias value verbatim and needs no cassette.
//! * `CHOICE { SYMBOL ext, STRING s }` yields the STRING and needs no
//!   cassette.
//! * Tokens stored at parse time via the `CstComplement` (literal-value
//!   constraints) emit the captured text directly.
//! * Everything else — context-dependent string delimiters, layout
//!   tokens that have no grammar-visible text, scanner-state markers
//!   used by the lexer but never emitted — flows through the cassette.
//!
//! Two layers compose:
//!
//! 1. `common_external_default` — universal name-pattern recognition
//!    that applies to every grammar. Recognises layout markers
//!    (`_concat`, `_no_space`, `_brace_start`, ...), immediate-position
//!    markers (`_immediate_*`), error sentinels (`_error_*`,
//!    `error_sentinel`), generic string-delimiter names, and the
//!    automatic-semicolon family. These patterns are stable across
//!    grammars because tree-sitter community convention.
//! 2. `GrammarCassette::external_token_default` — per-grammar
//!    overrides. A grammar that needs a different default (or extra
//!    tokens not covered by the common layer) implements this.
//!
//! The composed lookup is `resolve_external_token`: per-grammar first,
//! then common fallback. The emit walker calls this when it sees an
//! external SYMBOL with no other resolution path.

use std::sync::Arc;

/// Per-grammar defaults for opaque external scanner tokens.
///
/// Implementors override only the tokens that the common layer does
/// not cover or that need a grammar-specific override. Returning
/// `None` from the override delegates to the common layer.
pub trait GrammarCassette: Send + Sync {
    /// Returns the default text for an external scanner token. Return
    /// `None` to delegate to [`common_external_default`].
    fn external_token_default(&self, token_name: &str) -> Option<&str>;

    /// Override a `REPEAT` separator token with a layout action.
    /// Returns `true` if the separator should be emitted as a line
    /// break instead of the literal token text. Used by indent-based
    /// grammars where `;` in `_simple_statements` should produce a
    /// newline rather than a semicolon.
    fn separator_is_line_break(&self, separator_text: &str) -> bool {
        let _ = separator_text;
        false
    }
}

/// Compose the language-specific override with the common fallback.
/// The walker should call this rather than the trait method directly.
#[must_use]
pub fn resolve_external_token<'a>(
    cassette: &'a dyn GrammarCassette,
    token_name: &'a str,
) -> Option<&'a str> {
    if let Some(v) = cassette.external_token_default(token_name) {
        return Some(v);
    }
    common_external_default(token_name)
}

/// Common external-token defaults that apply to every grammar.
///
/// These patterns are derived from a structural audit of all 261
/// vendored grammars: external token names that follow a consistent
/// naming convention have consistent textual content, so a single
/// table covers them uniformly without per-grammar duplication. The
/// table is closed under the audit; new patterns can be added without
/// breaking existing cassettes because per-grammar overrides take
/// precedence.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn common_external_default(token_name: &str) -> Option<&'static str> {
    // Newline-producing externals.
    if matches!(
        token_name,
        "_newline"
            | "_line_break"
            | "_newline_before_do"
            | "_newline_before_binary_operator"
            | "_newline_before_comment"
            | "_newline_inline"
            | "_newline_not_aligned"
    ) {
        return Some("\n");
    }
    // Empty-text externals: scanner-state markers, error sentinels,
    // string-content placeholders, heredoc / raw-string delimiters
    // whose actual text is only available at parse time as a captured
    // literal-value. All of these emit no bytes when no literal-value
    // is available (the walker stores the actual text separately).
    if matches!(
        token_name,
        // Scanner-state markers.
        "_concat"
            | "_brace_concat"
            | "_concat_list"
            | "_no_space"
            | "_begin_brace"
            | "_brace_start"
            | "_bare_dollar"
            | "_no_line_break"
            | "_empty_value"
            | "_eof"
            | "_eof_or_newline"
            | "_after_eof"
            | "_end_of_file"
            | "_ignored"
            | "_non_whitespace_check"
            | "_in_fallback"
            // Error sentinels.
            | "_error"
            | "_error_sentinel"
            | "_error_recovery"
            | "__error_recovery"
            | "error_sentinel"
            | "_failure"
            // Automatic semicolons (layout pass inserts line breaks).
            | "_automatic_semicolon"
            | "_function_signature_automatic_semicolon"
            | "_optional_semi"
            // String / template content placeholders.
            | "_string_content"
            | "string_content"
            | "_template_chars"
            | "raw_string_content"
            | "_quoted_content"
            | "_raw_str_content"
            | "_multi_str_content"
            | "_multi_raw_str_content"
            // Block-comment placeholders.
            | "_block_comment_content"
            | "_documentation_block_comment"
            | "_block_comment"
            | "block_comment"
            | "multiline_comment"
            | "comment"
            | "html_comment"
            // Heredoc tokens (variable text recovered from literal-value).
            | "heredoc_start"
            | "heredoc_end"
            | "heredoc_content"
            | "heredoc_nl"
            | "heredoc_line"
            | "heredoc_marker"
            | "simple_heredoc_body"
            | "_heredoc_body_beginning"
            | "_heredoc_body_start"
            // Raw-string delimiters (variable text).
            | "raw_string_delimiter"
            | "raw_string_start"
            | "raw_string_end"
            // Regex / escape placeholders.
            | "escape_interpolation"
            | "escape_sequence"
            | "regex_pattern"
            | "regex_modifier"
            // HTML-family raw-text bodies.
            | "raw_text"
            | "jsx_text"
    ) {
        return Some("");
    }
    match token_name {
        // ── Generic string delimiters ─────────────────────────────────
        "string_start" | "string_end" | "_string_start" | "_string_end" => Some("\""),

        // ── Common keyword aliases (Crystal, Rust-style) ──────────────
        "not_in" => Some("not in"),
        "not_is" => Some("not is"),

        // ── Ternary operators commonly aliased ────────────────────────
        "_ternary_qmark" => Some("?"),

        // ── Descendant operators in CSS-like grammars ────────────────
        "_descendant_operator" => Some(" "),

        // ── Regex delimiter ───────────────────────────────────────────
        "_regex_start" => Some("/"),

        _ => {
            // Prefix-based rules for families of tokens.
            //
            // `_immediate_*`: scanner-state marker, no text.
            if token_name.starts_with("_immediate_") {
                return Some("");
            }
            // `_quoted_content_*` (Elixir): no default text; the parser
            // captures the actual content as a literal-value.
            if token_name.starts_with("_quoted_content_") {
                return Some("");
            }
            // `_external_expansion_sym_*` (bash/zsh): emit nothing;
            // these are scanner-only markers.
            if token_name.starts_with("_external_expansion_sym_") {
                return Some("");
            }
            // `_virtual_*` (Elm): layout markers with no text.
            if token_name.starts_with("_virtual_") {
                return Some("");
            }
            // `_layout_*` (Idris/Nim/PureScript): layout markers.
            if token_name.starts_with("_layout_") {
                return Some("");
            }
            // Multi-line string content variants.
            if token_name.starts_with("_multi_") {
                return Some("");
            }
            // `_tq_*` (Erlang triple-quoted): no default.
            if token_name.starts_with("_tq_") {
                return Some("");
            }
            None
        }
    }
}

/// The empty cassette: every lookup delegates to the common layer.
struct DefaultCassette;

impl GrammarCassette for DefaultCassette {
    fn external_token_default(&self, _token_name: &str) -> Option<&str> {
        None
    }
}

struct PythonCassette;

impl GrammarCassette for PythonCassette {
    fn external_token_default(&self, _token_name: &str) -> Option<&str> {
        // Python's string delimiters can be `"`, `'`, `"""`, `'''`, or
        // f/r/b-prefixed variants. The common layer defaults to `"`,
        // which is the safe choice; layout-specific overrides live in
        // `separator_is_line_break`.
        None
    }

    fn separator_is_line_break(&self, separator_text: &str) -> bool {
        // Python's `_simple_statements` rule joins statements with `;`,
        // but normal Python uses a newline at the statement level. The
        // emit layout pass replaces `;` with a line break.
        separator_text == ";"
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
            // Ruby's `_line_break` external is the statement terminator;
            // common layer emits `\n` (correct), included here for
            // explicit documentation.
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

/// Cassette for HTML-family grammars (HTML, Vue, Svelte, Astro, Blade,
/// Angular). All share the same external scanner skeleton.
struct HtmlFamilyCassette;

impl GrammarCassette for HtmlFamilyCassette {
    fn external_token_default(&self, token_name: &str) -> Option<&str> {
        match token_name {
            // Tag-name externals: the actual tag text is captured as a
            // literal-value at parse time; if missing, emit nothing
            // rather than guessing.
            "_start_tag_name"
            | "_end_tag_name"
            | "_script_start_tag_name"
            | "_style_start_tag_name"
            | "erroneous_end_tag_name"
            | "_implicit_end_tag" => Some(""),
            // Interpolation delimiters in template languages.
            "_interpolation_start" | "_html_interpolation_start" => Some("{{"),
            "_interpolation_end" | "_html_interpolation_end" => Some("}}"),
            _ => None,
        }
    }
}

/// Cassette for the Bash / Zsh / Fish family, which share most
/// externals (heredocs, variable expansions, brace-start markers).
struct ShellFamilyCassette;

impl GrammarCassette for ShellFamilyCassette {
    fn external_token_default(&self, token_name: &str) -> Option<&str> {
        match token_name {
            "file_descriptor" | "variable_name" | "test_operator" | "regex" | "_regex_no_slash"
            | "_regex_no_space" | "_expansion_word" | "extglob_pattern" => Some(""),
            "_immediate_double_hash" => Some("##"),
            _ => None,
        }
    }
}

/// Cassette for the C-family raw-string grammars (C++, CUDA, HLSL,
/// Arduino, C#). These share `raw_string_delimiter` and
/// `raw_string_content` externals whose actual text is parse-time
/// dependent; emit empty when there is no captured literal.
struct CFamilyCassette;

impl GrammarCassette for CFamilyCassette {
    fn external_token_default(&self, _token_name: &str) -> Option<&str> {
        None // Common layer handles raw_string_* uniformly.
    }
}

/// Cassette for JavaScript / TypeScript / TSX / QML, which share the
/// `_ternary_qmark`, `_automatic_semicolon`, `regex_pattern`, and
/// `jsx_text` externals.
struct JsFamilyCassette;

impl GrammarCassette for JsFamilyCassette {
    fn external_token_default(&self, _token_name: &str) -> Option<&str> {
        None // Common layer covers all the JS family externals.
    }
}

/// Cassette for indent-based grammars: Agda, F#, F# signatures,
/// Bitbake, Earthfile, Firrtl, Cooklang, Djot. Layout externals
/// (`_indent`, `_dedent`, `_newline`) are auto-detected by name in
/// [`crate::emit_pretty::Grammar`]; this cassette is the place to
/// override specific named externals if any need different text.
struct IndentBasedCassette;

impl GrammarCassette for IndentBasedCassette {
    fn external_token_default(&self, _token_name: &str) -> Option<&str> {
        None // Common layer handles _newline / _indent / _dedent.
    }
}

/// Look up the cassette for a grammar by protocol name.
///
/// Grammars not enumerated here get the default empty cassette, which
/// delegates every lookup to [`common_external_default`]. That is sufficient for
/// the majority of the 261 vendored grammars — the per-language
/// implementations above only exist for grammars whose externals
/// genuinely need grammar-specific overrides.
#[must_use]
pub fn cassette_for(protocol: &str) -> Arc<dyn GrammarCassette> {
    match protocol {
        "python" | "starlark" | "bitbake" => Arc::new(PythonCassette),
        "julia" => Arc::new(JuliaCassette),
        "ruby" | "crystal" => Arc::new(RubyCassette),
        "ocaml" | "ocaml_interface" => Arc::new(OcamlCassette),
        "html" | "vue" | "svelte" | "astro" | "blade" | "angular" | "templ" | "heex" => {
            Arc::new(HtmlFamilyCassette)
        }
        "bash" | "zsh" | "fish" => Arc::new(ShellFamilyCassette),
        "cpp" | "cuda" | "hlsl" | "arduino" | "csharp" | "c" => Arc::new(CFamilyCassette),
        "javascript" | "typescript" | "tsx" | "qml" | "rescript" => Arc::new(JsFamilyCassette),
        "agda" | "fsharp" | "fsharp_signature" | "earthfile" | "firrtl" | "cooklang" | "djot"
        | "idris" | "nim" | "purescript" | "haskell" | "elm" => Arc::new(IndentBasedCassette),
        _ => Arc::new(DefaultCassette),
    }
}
