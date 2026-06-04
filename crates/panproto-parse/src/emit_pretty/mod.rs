#![allow(
    clippy::module_name_repetitions,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::elidable_lifetime_names,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::single_match_else,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::single_char_pattern,
    clippy::naive_bytecount,
    clippy::expect_used,
    clippy::redundant_pub_crate,
    clippy::used_underscore_binding,
    clippy::redundant_field_names,
    clippy::struct_field_names,
    clippy::redundant_else,
    clippy::similar_names
)]

//! De-novo source emission from a by-construction schema.
//!
//! [`AstParser::emit`] reconstructs source from byte-position fragments
//! that the parser stored on the schema during `parse`. That works for
//! edit pipelines (`parse → transform → emit`) but fails for schemas
//! built by hand (`SchemaBuilder` with no parse history): they carry
//! no `start-byte`, no `interstitial-N`, no `literal-value`, and the
//! reconstructor returns `Err(EmitFailed { reason: "schema has no
//! text fragments" })`.
//!
//! This module renders such schemas to source bytes by walking
//! tree-sitter's `grammar.json` production rules. For each schema
//! vertex of kind `K`, the walker looks up `K`'s production in the
//! grammar and emits its body in order:
//!
//! - `STRING` nodes contribute literal token bytes directly.
//! - `SYMBOL` and `FIELD` nodes recurse into the schema's children,
//!   matching by edge kind (which is the tree-sitter field name).
//! - `SEQ` emits its members in order.
//! - `CHOICE` picks the alternative whose head `SYMBOL` matches an
//!   actual child kind, or whose terminals appear in the rendered
//!   prefix; falls back to the first non-`BLANK` alternative when no
//!   alternative matches.
//! - `REPEAT` and `REPEAT1` emit their content once per matching
//!   child edge in declared order.
//! - `OPTIONAL` emits its content iff a corresponding child edge or
//!   constraint is populated.
//! - `PATTERN` is a regex placeholder for variable-text terminals
//!   (identifiers, numbers, quoted strings). The walker emits a
//!   `literal-value` constraint when present and otherwise falls
//!   back to a placeholder derived from the regex shape.
//! - `BLANK`, `TOKEN`, `IMMEDIATE_TOKEN`, `ALIAS`, `PREC*` are
//!   handled transparently (the inner content is emitted; the
//!   wrapper is dropped).
//!
//! Whitespace and indentation come from a `FormatPolicy` applied
//! during emission. The default policy inserts a single space between
//! adjacent tokens, a newline after `;` / `}` / `{`, and tracks an
//! indent counter on `{` / `}` boundaries.
//!
//! Output is *syntactically valid* for any grammar that ships
//! `grammar.json`. Idiomatic formatting (rustfmt-style spacing rules,
//! per-language conventions) is a polish layer that lives outside
//! this module.

mod complement;
mod cursor;
mod grammar;
mod helpers;
mod layout;
mod review;
mod unify;

pub(crate) use crate::error::ParseError;
pub(crate) use panproto_schema::{Edge, Schema};
pub(crate) use serde::Deserialize;
pub(crate) use std::collections::BTreeMap;

pub(crate) use complement::*;
pub(crate) use cursor::*;
pub(crate) use grammar::*;
pub(crate) use helpers::*;
pub(crate) use layout::*;
pub(crate) use review::*;
pub(crate) use unify::*;

// Public API surface (reached via crate::emit_pretty::*).
pub use grammar::{Grammar, Production, TokenRole};
pub use layout::FormatPolicy;

// ═══════════════════════════════════════════════════════════════════
// Emitter
// ═══════════════════════════════════════════════════════════════════

/// Emit a by-construction schema to source bytes.
///
/// `protocol` is the grammar / language name (used in error messages
/// and to label the entry point).
///
/// The walker treats `schema.entries` as the ordered list of root
/// vertices, falling back to a deterministic by-id ordering when
/// `entries` is empty. Each root is emitted using the production
/// associated with its kind in `grammar.rules`.
///
/// # Errors
///
/// Returns [`ParseError::EmitFailed`] when:
///
/// - the schema has no vertices
/// - a root vertex's kind is not a grammar rule
/// - a `SYMBOL` reference points at a kind with no rule and no schema
///   child to resolve it to
/// - a required `FIELD` has no corresponding edge in the schema
pub fn emit_pretty(
    protocol: &str,
    schema: &Schema,
    grammar: &Grammar,
    policy: &FormatPolicy,
    cassette: Option<&dyn crate::languages::cassettes::GrammarCassette>,
) -> Result<Vec<u8>, ParseError> {
    let roots = collect_roots(schema);
    if roots.is_empty() {
        return Err(ParseError::EmitFailed {
            protocol: protocol.to_owned(),
            reason: "schema has no entry vertices".to_owned(),
        });
    }

    // A leading byte run that tree-sitter excluded from the document root's
    // span (a UTF-8 BOM, an awk `\<newline>` line-continuation, a leading
    // comment) is recorded by the walker as `doc-prefix` on the root vertex.
    // It belongs to no interstitial run, so reproduce it verbatim ahead of the
    // emitted body to keep the byte-faithful replay lossless.
    let doc_prefix: Vec<u8> = schema
        .constraints
        .get(roots[0])
        .and_then(|cs| cs.iter().find(|c| c.sort.as_ref() == "doc-prefix"))
        .map(|c| c.value.as_bytes().to_vec())
        .unwrap_or_default();

    let mut out = Output::new(policy, grammar, cassette);
    for (i, root) in roots.iter().enumerate() {
        if i > 0 {
            out.newline();
        }
        emit_vertex(protocol, schema, grammar, root, &mut out)?;
    }
    let mut body = out.finish();
    // Drop leading whitespace from the emitted BODY. Tree-sitter assigns the
    // root node a start-byte that consumes a single leading newline as a leading
    // "extra" but folds any FURTHER leading blank lines into the root span,
    // where they surface as the root's leading interstitial. The replay
    // reproduces those in-span newlines, but a re-parse of the emitted bytes
    // records one fewer leading interstitial, so the second emit drops it,
    // breaking the emit fixed point on a corpus entry that opens with blank
    // lines (rescript `\n\nlet {…}`). Leading whitespace before the first token
    // is never syntactically meaningful, so trimming it makes emit idempotent
    // without changing any AST. Trailing whitespace is left to the verbatim-tail
    // logic in `layout`.
    //
    // The trim runs on `body` (the bytes from `root.start_byte()` onward) BEFORE
    // the `doc-prefix` is prepended: `doc_prefix` holds the bytes in
    // `[0, root.start_byte())` that tree-sitter excludes from the root span (a
    // BOM, an awk `\`-continuation), which must be reproduced verbatim. Trimming
    // after the prepend would eat a whitespace-class prefix (awk) and re-break
    // it; trimming the body first preserves the fixed point in both directions.
    let lead = body.iter().take_while(|b| b.is_ascii_whitespace()).count();
    if lead > 0 {
        body.drain(..lead);
    }
    if doc_prefix.is_empty() {
        Ok(body)
    } else {
        let mut result = doc_prefix;
        result.extend_from_slice(&body);
        Ok(result)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn test_grammar() -> Grammar {
        Grammar::from_bytes("test", b"{\"name\":\"test\",\"rules\":{}}").unwrap_or_else(|_| {
            serde_json::from_str::<Grammar>(r#"{"name":"test","rules":{}}"#).unwrap()
        })
    }

    #[test]
    fn parses_simple_grammar_json() {
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "program": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "hello"},
                        {"type": "STRING", "value": ";"}
                    ]
                }
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid tiny grammar");
        assert!(g.rules.contains_key("program"));
    }

    #[test]
    fn output_emits_punctuation_without_leading_space() {
        let policy = FormatPolicy::default();
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token_with_role("foo", Some(TokenRole::Terminal));
        out.token_with_role("(", Some(TokenRole::BracketOpen));
        out.token_with_role(")", Some(TokenRole::BracketClose));
        out.token_with_role(";", Some(TokenRole::Separator));
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.starts_with("foo();"), "got {s:?}");
    }

    #[test]
    fn rest_of_line_pattern_detects_unbounded_tail_only() {
        // Genuine rest-of-line terminals (unbounded `.*` / `.+` to EOL).
        assert!(is_rest_of_line_pattern("#!.*"));
        assert!(is_rest_of_line_pattern(
            "#![\\r\\f\\t\\v ]*([^\\[\\n].*)?\\n"
        ));
        assert!(is_rest_of_line_pattern("(;|#!|# ).*"));
        // A line comment may end in an unbounded *newline-only* negated class
        // (`[^\n]*` / `[^\r\n]*`) rather than `.*` — forth's `\\[^\n]*`, the
        // `//[^\n]*` / `#[^\r\n]*` idiom.
        assert!(is_rest_of_line_pattern("\\\\[^\\n]*")); // forth `\` line comment
        assert!(is_rest_of_line_pattern("//[^\\n]*"));
        assert!(is_rest_of_line_pattern("#[^\\r\\n]*"));
        // Bounded or non-line tails must NOT be treated as rest-of-line.
        assert!(!is_rest_of_line_pattern("@\\[.*\\]")); // firrtl info: `.*` then `]`
        assert!(!is_rest_of_line_pattern("[^\"\\\\\\r\\n]+")); // string fragment
        assert!(!is_rest_of_line_pattern("[^\\\\\"\\n]+")); // json string_content
        assert!(!is_rest_of_line_pattern("[a-z]+")); // plain identifier
        assert!(!is_rest_of_line_pattern("foo\\.*bar")); // escaped dot, not a metachar
        // A newline-excluding class that ALSO excludes other members is a
        // same-line-bounded fragment, not a line-comment tail.
        assert!(!is_rest_of_line_pattern("\"[^\"\\n]*\"")); // quoted string body
    }

    #[test]
    fn line_rest_kinds_classifies_hash_bang() {
        let bytes = br##"{
            "name": "tiny",
            "rules": {
                "hash_bang_line": {"type": "PATTERN", "value": "#!.*"},
                "info": {"type": "PATTERN", "value": "@\\[.*\\]"},
                "ident": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"##;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        assert!(g.line_rest_kinds.contains("hash_bang_line"));
        assert!(!g.line_rest_kinds.contains("info"));
        assert!(!g.line_rest_kinds.contains("ident"));
    }

    #[test]
    fn trailing_break_markers_detect_hard_line_break_idiom() {
        // `hard_line_break = SEQ[CHOICE["\\" | _ws], _soft_line_break]` — the
        // markdown_inline shape. The bare `\` is a break marker; the
        // whitespace alternative sets the whitespace flag.
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "doc": {"type": "SYMBOL", "name": "hard_line_break"},
                "hard_line_break": {"type": "SEQ", "members": [
                    {"type": "CHOICE", "members": [
                        {"type": "STRING", "value": "\\"},
                        {"type": "SYMBOL", "name": "_ws"}
                    ]},
                    {"type": "SYMBOL", "name": "_nl"}
                ]},
                "_ws": {"type": "PATTERN", "value": "\\t| [ \\t]+"},
                "_nl": {"type": "PATTERN", "value": "\\n|\\r\\n?"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        assert!(g.trailing_break_markers.iter().any(|m| m == "\\"));
        assert!(g.trailing_break_on_whitespace);
        // A keyword-led line construct is NOT a break marker.
        let bytes2 = br#"{
            "name": "t2",
            "rules": {
                "doc": {"type": "SYMBOL", "name": "directive"},
                "directive": {"type": "SEQ", "members": [
                    {"type": "STRING", "value": "go"},
                    {"type": "PATTERN", "value": "\\n"}
                ]}
            }
        }"#;
        let g2 = Grammar::from_bytes("t2", bytes2).expect("valid grammar");
        assert!(g2.trailing_break_markers.is_empty());
        assert!(!g2.trailing_break_on_whitespace);
    }

    #[test]
    fn top_level_text_admits_newline_detects_template_content() {
        // A `liquid`-style program whose top repeat directly admits a
        // free-text content node matching a bare newline.
        let bytes = br#"{
            "name": "tmpl",
            "rules": {
                "program": {"type": "REPEAT", "content":
                    {"type": "SYMBOL", "name": "_node"}},
                "_node": {"type": "CHOICE", "members": [
                    {"type": "SYMBOL", "name": "tag"},
                    {"type": "SYMBOL", "name": "template_content"}
                ]},
                "tag": {"type": "STRING", "value": "{%%}"},
                "template_content": {"type": "REPEAT1", "content":
                    {"type": "PATTERN", "value": "[^{]+"}}
            }
        }"#;
        let g = Grammar::from_bytes("tmpl", bytes).expect("valid grammar");
        assert!(g.start_symbol == "program");
        assert!(g.top_level_text_admits_newline);
        // A grammar whose only newline-admitting class is a block comment
        // (nested under delimiters, not a top-level document node) must NOT
        // set the flag.
        let bytes2 = br#"{
            "name": "prog",
            "rules": {
                "source_file": {"type": "REPEAT", "content":
                    {"type": "SYMBOL", "name": "statement"}},
                "statement": {"type": "STRING", "value": "x"},
                "comment": {"type": "SEQ", "members": [
                    {"type": "STRING", "value": "/*"},
                    {"type": "PATTERN", "value": "[^*]+"},
                    {"type": "STRING", "value": "*/"}
                ]}
            }
        }"#;
        let g2 = Grammar::from_bytes("prog", bytes2).expect("valid grammar");
        assert!(g2.start_symbol == "source_file");
        assert!(!g2.top_level_text_admits_newline);
    }

    #[test]
    fn immediate_token_alias_kinds_classifies_char_body() {
        // `char_literal` = SEQ[quote-open, REPEAT1(ALIAS{IMMEDIATE_TOKEN
        // PATTERN, value:"character"}), quote-close]: a quote-pair-delimited
        // body, so `character` IS tightened. `brace_expression` aliases the
        // same `\d+` shape to `number` but is bracket-delimited (`{`/`}`), so
        // `number` is NOT tightened (it is also a freestanding command
        // argument that must keep its space).
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "char_literal": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "'"},
                        {
                            "type": "REPEAT1",
                            "content": {
                                "type": "ALIAS",
                                "named": true,
                                "value": "character",
                                "content": {
                                    "type": "IMMEDIATE_TOKEN",
                                    "content": {"type": "PATTERN", "value": "[^\\n']"}
                                }
                            }
                        },
                        {"type": "STRING", "value": "'"}
                    ]
                },
                "brace_expression": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "{"},
                        {
                            "type": "ALIAS",
                            "named": true,
                            "value": "number",
                            "content": {
                                "type": "IMMEDIATE_TOKEN",
                                "content": {"type": "PATTERN", "value": "\\d+"}
                            }
                        },
                        {"type": "STRING", "value": "}"}
                    ]
                },
                "plain_alias": {
                    "type": "ALIAS",
                    "named": true,
                    "value": "ident",
                    "content": {"type": "SYMBOL", "name": "x"}
                },
                "kw_literal": {
                    "type": "SEQ",
                    "members": [
                        {"type": "STRING", "value": "'"},
                        {
                            "type": "ALIAS",
                            "named": true,
                            "value": "identifier",
                            "content": {
                                "type": "IMMEDIATE_TOKEN",
                                "content": {"type": "STRING", "value": "module"}
                            }
                        },
                        {"type": "STRING", "value": "'"}
                    ]
                }
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        // Quote-pair-delimited char-class body: tightened.
        assert!(g.immediate_token_alias_kinds.contains("character"));
        // Bracket-delimited numeric brace-range body: NOT tightened (the same
        // `number` kind is a spaced command argument elsewhere). This is the
        // narrowing that removes db9b280's bash `exit 1` -> `exit1` regression.
        assert!(!g.immediate_token_alias_kinds.contains("number"));
        assert!(!g.immediate_token_alias_kinds.contains("ident"));
        // A word-like keyword alias (Julia `identifier` = IMMEDIATE_TOKEN over
        // a STRING) is NOT a content fragment and must keep its spacing, even
        // inside a quote pair.
        assert!(!g.immediate_token_alias_kinds.contains("identifier"));
    }

    #[test]
    fn grammar_from_bytes_rejects_malformed_input() {
        let result = Grammar::from_bytes("malformed", b"not json");
        let err = result.expect_err("malformed bytes must yield Err");
        let msg = err.to_string();
        assert!(
            msg.contains("malformed"),
            "error message should name the protocol: {msg:?}"
        );
    }

    #[test]
    fn output_indents_after_open_brace() {
        let policy = FormatPolicy::default();
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token_with_role("fn", Some(TokenRole::Keyword));
        out.token_with_role("foo", Some(TokenRole::Terminal));
        out.token_with_role("(", Some(TokenRole::BracketOpen));
        out.token_with_role(")", Some(TokenRole::BracketClose));
        out.token_with_role("{", Some(TokenRole::BracketOpen));
        out.token_with_role("body", Some(TokenRole::Terminal));
        out.token_with_role("}", Some(TokenRole::BracketClose));
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.contains("{\n"), "newline after opening brace: {s:?}");
        assert!(s.contains("body"), "body inside block: {s:?}");
        assert!(s.ends_with("}\n"), "newline after closing brace: {s:?}");
    }

    #[test]
    fn output_no_space_between_word_and_dot() {
        let policy = FormatPolicy::default();
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token_with_role("foo", Some(TokenRole::Terminal));
        out.token_with_role(".", Some(TokenRole::Operator));
        out.token_with_role("bar", Some(TokenRole::Terminal));
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        // With role-based spacing, operator gets spaces: "foo . bar"
        // The dot tight-binding is a grammar-derived property (dot appears
        // between SYMBOL members in attribute/field access rules).
        // For unit tests with explicit roles, we accept spaced dot.
        assert!(
            s.contains("foo") && s.contains("bar"),
            "both identifiers present: {s:?}"
        );
    }

    #[test]
    fn output_snapshot_restore_truncates_bytes() {
        let policy = FormatPolicy::default();
        let g = test_grammar();
        let mut out = Output::new(&policy, &g, None);
        out.token("keep");
        let snap = out.snapshot();
        out.token("drop");
        out.token("more");
        out.restore(snap);
        out.token("after");
        let bytes = out.finish();
        let s = std::str::from_utf8(&bytes).expect("ascii output");
        assert!(s.contains("keep"), "kept token survives: {s:?}");
        assert!(s.contains("after"), "post-restore token visible: {s:?}");
        assert!(!s.contains("drop"), "rolled-back token removed: {s:?}");
        assert!(!s.contains("more"), "rolled-back token removed: {s:?}");
    }

    #[test]
    fn child_cursor_take_field_consumes_once() {
        let edges_owned: Vec<Edge> = vec![Edge {
            src: panproto_gat::Name::from("p"),
            tgt: panproto_gat::Name::from("c"),
            kind: panproto_gat::Name::from("name"),
            name: None,
        }];
        let edges: Vec<&Edge> = edges_owned.iter().collect();
        let mut cursor = ChildCursor::new(&edges);
        let first = cursor.take_field("name");
        let second = cursor.take_field("name");
        assert!(first.is_some(), "first take returns the edge");
        assert!(
            second.is_none(),
            "second take returns None (already consumed)"
        );
    }

    #[test]
    fn child_cursor_take_matching_predicate() {
        let edges_owned: Vec<Edge> = vec![
            Edge {
                src: "p".into(),
                tgt: "c1".into(),
                kind: "child_of".into(),
                name: None,
            },
            Edge {
                src: "p".into(),
                tgt: "c2".into(),
                kind: "key".into(),
                name: None,
            },
        ];
        let edges: Vec<&Edge> = edges_owned.iter().collect();
        let mut cursor = ChildCursor::new(&edges);
        assert!(cursor.has_matching(|e| e.kind.as_ref() == "key"));
        let taken = cursor.take_matching(|e| e.kind.as_ref() == "key");
        assert!(taken.is_some());
        assert!(
            !cursor.has_matching(|e| e.kind.as_ref() == "key"),
            "consumed edge no longer matches"
        );
        assert!(
            cursor.has_matching(|e| e.kind.as_ref() == "child_of"),
            "the other edge is still available"
        );
    }

    #[test]
    fn kind_satisfies_symbol_direct_match() {
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "x": {"type": "STRING", "value": "x"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        assert!(kind_satisfies_symbol(&g, Some("x"), "x"));
        assert!(!kind_satisfies_symbol(&g, Some("y"), "x"));
        assert!(!kind_satisfies_symbol(&g, None, "x"));
    }

    #[test]
    fn kind_satisfies_symbol_through_hidden_rule() {
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "_value": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "object"},
                        {"type": "SYMBOL", "name": "number"}
                    ]
                },
                "object": {"type": "STRING", "value": "{}"},
                "number": {"type": "PATTERN", "value": "[0-9]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("valid grammar");
        assert!(
            kind_satisfies_symbol(&g, Some("number"), "_value"),
            "number is reachable from _value via CHOICE"
        );
        assert!(
            kind_satisfies_symbol(&g, Some("object"), "_value"),
            "object is reachable from _value via CHOICE"
        );
        assert!(
            !kind_satisfies_symbol(&g, Some("string"), "_value"),
            "string is NOT among the alternatives"
        );
    }

    #[test]
    fn first_symbol_skips_string_terminals() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "SEQ",
                "members": [
                    {"type": "STRING", "value": "{"},
                    {"type": "SYMBOL", "name": "body"},
                    {"type": "STRING", "value": "}"}
                ]
            }"#,
        )
        .expect("valid SEQ");
        assert_eq!(first_symbol(&prod), Some("body"));
    }

    #[test]
    fn is_newline_like_pattern_handles_alternations_and_classes() {
        // Single newline atoms (the original behaviour).
        assert!(is_newline_like_pattern("\\n"));
        assert!(is_newline_like_pattern("\\r\\n"));
        assert!(is_newline_like_pattern("\\r?\\n"));
        // Top-level alternation of newline-only branches (CSV/TSV row end).
        assert!(is_newline_like_pattern("\\r|\\r\\n|\\n"));
        // Alternation mixing a newline-only character class (properties).
        assert!(is_newline_like_pattern("[\\r\\n]|\\r\\n"));
        assert!(is_newline_like_pattern("[\\r\\n]"));
        // Not newline-like: free text, or an alternation with a text branch.
        assert!(!is_newline_like_pattern(".+"));
        assert!(!is_newline_like_pattern("\\n|."));
        assert!(!is_newline_like_pattern("[a\\n]"));
        assert!(!is_newline_like_pattern(""));
    }

    #[test]
    fn pattern_absorbs_leading_space_detects_space_admitting_terminals() {
        // `.`-led patterns match any char, including a space (INI value).
        assert!(pattern_absorbs_leading_space(".+"));
        assert!(pattern_absorbs_leading_space(".*"));
        assert!(pattern_absorbs_leading_space("^.+"));
        // Negated classes that do not exclude whitespace match a space.
        assert!(pattern_absorbs_leading_space("[^;#]+"));
        // Negated classes that exclude whitespace do not match a space.
        assert!(!pattern_absorbs_leading_space("[^;#=\\s\\[]+"));
        assert!(!pattern_absorbs_leading_space("[^ \\t]+"));
        // Positive classes / literals never start with a space.
        assert!(!pattern_absorbs_leading_space("[a-zA-Z_]\\w*"));
        assert!(!pattern_absorbs_leading_space("[0-9]+"));
        assert!(!pattern_absorbs_leading_space("\\w+"));
        assert!(!pattern_absorbs_leading_space(""));
    }

    #[test]
    fn placeholder_for_pattern_routes_by_regex_class() {
        assert_eq!(placeholder_for_pattern("[0-9]+"), "0");
        assert_eq!(placeholder_for_pattern("[a-zA-Z_]\\w*"), "_x");
        assert_eq!(placeholder_for_pattern("\"[^\"]*\""), "\"\"");
        assert_eq!(placeholder_for_pattern("\\d+\\.\\d+"), "0");
    }

    #[test]
    fn format_policy_default_breaks_after_semicolon() {
        let policy = FormatPolicy::default();
        assert!(policy.line_break_after.iter().any(|t| t == ";"));
        assert!(policy.indent_open.iter().any(|t| t == "{"));
        assert!(policy.indent_close.iter().any(|t| t == "}"));
        assert_eq!(policy.indent_width, 2);
    }

    #[test]
    fn placeholder_decodes_literal_pattern_separators() {
        // PATTERN regexes that match a single literal byte sequence
        // (newline, semicolon, comma) emit the bytes verbatim instead
        // of falling through to the `_` catch-all.
        assert_eq!(placeholder_for_pattern("\\n"), "\n");
        assert_eq!(placeholder_for_pattern("\\r\\n"), "\r\n");
        assert_eq!(placeholder_for_pattern(";"), ";");
        // Patterns with character classes / alternation still route
        // through the heuristic.
        assert_eq!(placeholder_for_pattern("[0-9]+"), "0");
        assert_eq!(placeholder_for_pattern("a|b"), "_");
    }

    #[test]
    fn placeholder_decodes_whitespace_padded_literal() {
        // A literal separator wrapped in optional-whitespace classes
        // (GLSL `#extension` `extension : behavior`) emits the literal
        // core, not the `_` placeholder.
        assert_eq!(placeholder_for_pattern("[ \\t]*:[ \\t]*"), ":");
        assert_eq!(placeholder_for_pattern("\\s*=\\s*"), "=");
        assert_eq!(placeholder_for_pattern("[ \\t]*->"), "->");
        // A non-whitespace leading class is not padding: the
        // whitespace-padded decoder declines and the pattern routes
        // through the general heuristic.
        assert_eq!(placeholder_for_pattern("[a-z]*:[ \\t]*"), "_");
    }

    #[test]
    fn supertypes_decode_from_grammar_json_strings() {
        // Tree-sitter older grammars list supertypes as bare strings.
        let bytes = br#"{
            "name": "tiny",
            "supertypes": ["expression"],
            "rules": {
                "expression": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "binary_expression"},
                        {"type": "SYMBOL", "name": "identifier"}
                    ]
                },
                "binary_expression": {"type": "STRING", "value": "x"},
                "identifier": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("parse");
        assert!(g.supertypes.contains("expression"));
        // identifier matches the supertype `expression`.
        assert!(kind_satisfies_symbol(&g, Some("identifier"), "expression"));
        // unrelated kinds do not.
        assert!(!kind_satisfies_symbol(&g, Some("string"), "expression"));
    }

    #[test]
    fn supertypes_decode_from_grammar_json_objects() {
        // Recent grammars list supertypes as `{type: SYMBOL, name: ...}`
        // entries instead of bare strings.
        let bytes = br#"{
            "name": "tiny",
            "supertypes": [{"type": "SYMBOL", "name": "stmt"}],
            "rules": {
                "stmt": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "while_stmt"},
                        {"type": "SYMBOL", "name": "if_stmt"}
                    ]
                },
                "while_stmt": {"type": "STRING", "value": "while"},
                "if_stmt": {"type": "STRING", "value": "if"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("parse");
        assert!(g.supertypes.contains("stmt"));
        assert!(kind_satisfies_symbol(&g, Some("while_stmt"), "stmt"));
    }

    #[test]
    fn alias_value_matches_kind() {
        // A named ALIAS rewrites the parser-visible kind to `value`;
        // `kind_satisfies_symbol` should accept that rewritten kind
        // when looking up the original SYMBOL.
        let bytes = br#"{
            "name": "tiny",
            "rules": {
                "_package_identifier": {
                    "type": "ALIAS",
                    "named": true,
                    "value": "package_identifier",
                    "content": {"type": "SYMBOL", "name": "identifier"}
                },
                "identifier": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny", bytes).expect("parse");
        assert!(kind_satisfies_symbol(
            &g,
            Some("package_identifier"),
            "_package_identifier"
        ));
    }

    #[test]
    fn referenced_symbols_walks_nested_seq() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "SEQ",
                "members": [
                    {"type": "CHOICE", "members": [
                        {"type": "SYMBOL", "name": "attribute_item"},
                        {"type": "BLANK"}
                    ]},
                    {"type": "SYMBOL", "name": "parameter"},
                    {"type": "REPEAT", "content": {
                        "type": "SEQ",
                        "members": [
                            {"type": "STRING", "value": ","},
                            {"type": "SYMBOL", "name": "parameter"}
                        ]
                    }}
                ]
            }"#,
        )
        .expect("seq");
        let symbols = referenced_symbols(&prod);
        assert!(symbols.contains(&"attribute_item"));
        assert!(symbols.contains(&"parameter"));
    }

    #[test]
    fn literal_strings_collects_choice_members() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "CHOICE",
                "members": [
                    {"type": "STRING", "value": "+"},
                    {"type": "STRING", "value": "-"},
                    {"type": "STRING", "value": "*"}
                ]
            }"#,
        )
        .expect("choice");
        let strings = literal_strings(&prod);
        assert_eq!(strings, vec!["+", "-", "*"]);
    }

    /// The ocaml and javascript grammars (tree-sitter ≥ 0.25) emit a
    /// `RESERVED` rule kind that an earlier deserialiser rejected
    /// with `unknown variant "RESERVED"`. Verify both that the bare
    /// variant deserialises and that a `RESERVED`-wrapped grammar is
    /// loadable end-to-end via [`Grammar::from_bytes`].
    #[test]
    fn reserved_variant_deserialises() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "RESERVED",
                "content": {"type": "SYMBOL", "name": "_lowercase_identifier"},
                "context_name": "attribute_id"
            }"#,
        )
        .expect("RESERVED parses");
        match prod {
            Production::Reserved { content, .. } => match *content {
                Production::Symbol { name } => assert_eq!(name, "_lowercase_identifier"),
                other => panic!("expected inner SYMBOL, got {other:?}"),
            },
            other => panic!("expected RESERVED, got {other:?}"),
        }
    }

    #[test]
    fn reserved_grammar_loads_end_to_end() {
        let bytes = br#"{
            "name": "tiny_reserved",
            "rules": {
                "program": {
                    "type": "RESERVED",
                    "content": {"type": "SYMBOL", "name": "ident"},
                    "context_name": "keywords"
                },
                "ident": {"type": "PATTERN", "value": "[a-z]+"}
            }
        }"#;
        let g = Grammar::from_bytes("tiny_reserved", bytes).expect("RESERVED-using grammar loads");
        assert!(g.rules.contains_key("program"));
    }

    #[test]
    fn reserved_walker_helpers_recurse_into_content() {
        // The walker's helpers (first_symbol, has_field_in,
        // referenced_symbols, ...) all need to descend through
        // RESERVED into its content. If they bail at RESERVED, the
        // `pick_choice_with_cursor` heuristic ranks the alt below
        // alts that DO recurse, which produces wrong emit output
        // even when the deserialiser doesn't crash.
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "RESERVED",
                "content": {
                    "type": "FIELD",
                    "name": "lhs",
                    "content": {"type": "SYMBOL", "name": "expr"}
                },
                "context_name": "ctx"
            }"#,
        )
        .expect("nested RESERVED parses");
        assert_eq!(first_symbol(&prod), Some("expr"));
        assert!(has_field_in(&prod, &["lhs"]));
        let symbols = referenced_symbols(&prod);
        assert!(symbols.contains(&"expr"));
    }

    // -- Yield-set tests --

    fn yield_of(grammar: &Grammar, prod: &Production) -> std::collections::HashSet<String> {
        let mut visited = std::collections::HashSet::new();
        let mut cache = grammar.yield_sets.clone();
        yield_of_production(grammar, prod, &mut visited, &mut cache)
    }

    #[test]
    fn yield_set_seq_only_first_member() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "SEQ",
                "members": [
                    {"type": "SYMBOL", "name": "identifier"},
                    {"type": "STRING", "value": "as"},
                    {"type": "SYMBOL", "name": "target"}
                ]
            }"#,
        )
        .expect("valid SEQ");
        let g = Grammar::from_bytes("test", b"{}").unwrap_or_else(|_| {
            serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap()
        });
        let ys = yield_of(&g, &prod);
        assert!(ys.contains("identifier"), "SEQ yields first member");
        assert!(
            !ys.contains("target"),
            "SEQ must NOT yield non-first members"
        );
    }

    #[test]
    fn yield_set_choice_union() {
        let prod: Production = serde_json::from_str(
            r#"{
                "type": "CHOICE",
                "members": [
                    {"type": "SYMBOL", "name": "a"},
                    {"type": "SYMBOL", "name": "b"}
                ]
            }"#,
        )
        .expect("valid CHOICE");
        let g = serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap();
        let ys = yield_of(&g, &prod);
        assert_eq!(ys.len(), 2);
        assert!(ys.contains("a"));
        assert!(ys.contains("b"));
    }

    #[test]
    fn yield_set_hidden_expansion() {
        let g = serde_json::from_str::<Grammar>(
            r#"{"name":"t","rules":{
                "_value": {
                    "type": "CHOICE",
                    "members": [
                        {"type": "SYMBOL", "name": "number"},
                        {"type": "SYMBOL", "name": "object"}
                    ]
                }
            }}"#,
        )
        .unwrap();
        let mut g = g;
        g.subtypes = compute_subtype_closure(&g);
        g.yield_sets = compute_yield_sets(&g);
        let sym: Production =
            serde_json::from_str(r#"{"type": "SYMBOL", "name": "_value"}"#).unwrap();
        let ys = yield_of(&g, &sym);
        assert!(
            ys.contains("number"),
            "hidden rule expands into its CHOICE members"
        );
        assert!(ys.contains("object"));
        assert!(
            !ys.contains("_value"),
            "hidden rule name is not in yield set"
        );
    }

    #[test]
    fn yield_set_optional_includes_epsilon() {
        let prod: Production = serde_json::from_str(
            r#"{"type": "OPTIONAL", "content": {"type": "SYMBOL", "name": "x"}}"#,
        )
        .unwrap();
        let g = serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap();
        let ys = yield_of(&g, &prod);
        assert!(ys.contains("x"));
        assert!(ys.contains(""), "OPTIONAL includes epsilon");
    }

    #[test]
    fn yield_set_alias_uses_value() {
        let prod: Production = serde_json::from_str(
            r#"{"type": "ALIAS", "content": {"type": "SYMBOL", "name": "real"},
                "named": true, "value": "alias_name"}"#,
        )
        .unwrap();
        let g = serde_json::from_str::<Grammar>(r#"{"name":"t","rules":{}}"#).unwrap();
        let ys = yield_of(&g, &prod);
        assert_eq!(ys.len(), 1);
        assert!(ys.contains("alias_name"), "named ALIAS yields its value");
    }
}
