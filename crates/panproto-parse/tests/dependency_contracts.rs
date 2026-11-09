//! Observers for parser contracts used by the IO dependency campaign.

use panproto_parse::languages::cassettes::{cassette_for, resolve_external_token};
use panproto_parse::languages::walker_configs::walker_config_for;
use panproto_parse::{IdGenerator, ScopeKind};

#[test]
fn recorded_scope_names_and_counter_resets_preserve_identity() {
    let mut ids = IdGenerator::new("input.rs");
    assert_eq!(ids.current_prefix(), "input.rs");
    assert_eq!(ids.record_name("item"), "item");
    let leaf = ids.record_name("item");
    assert_eq!(leaf, "item#1");
    ids.push_recorded_scope(leaf);
    assert_eq!(ids.current_prefix(), "input.rs::item#1");
    assert_eq!(ids.depth(), 2);
    assert_eq!(ids.named_id("local"), "input.rs::item#1::local");
    assert_eq!(ids.anonymous_id(), "input.rs::item#1::$0");
    assert_eq!(ids.anonymous_id(), "input.rs::item#1::$1");
    ids.reset_counter();
    assert_eq!(ids.anonymous_id(), "input.rs::item#1::$0");
    assert_eq!(ids.named_id("local"), "input.rs::item#1::local#1");
    ids.pop_scope();
    assert_eq!(ids.current_prefix(), "input.rs");
    assert_eq!(ids.record_name("item"), "item#2");
    assert_eq!(ids.anonymous_id(), "input.rs::$0");
}

#[test]
fn scope_capture_suffixes_round_trip_known_and_custom_kinds() {
    for (kind, suffix) in [
        (ScopeKind::Function, "function"),
        (ScopeKind::Method, "method"),
        (ScopeKind::Class, "class"),
        (ScopeKind::Module, "module"),
        (ScopeKind::Interface, "interface"),
        (ScopeKind::Type, "type"),
        (ScopeKind::Macro, "macro"),
        (ScopeKind::Other("constructor".to_owned()), "constructor"),
        (ScopeKind::Other(String::new()), ""),
    ] {
        assert_eq!(kind.as_suffix(), suffix);
        assert_eq!(ScopeKind::from_suffix(suffix), kind);
    }
}

#[test]
fn language_blocks_keep_standard_capture_and_depth_settings() {
    let cases: &[(&str, &[&str])] = &[
        (
            "python",
            &["argument_list", "expression_list", "pattern_list"],
        ),
        ("typescript", &["switch_body", "template_string"]),
        ("tsx", &["switch_body", "jsx_expression"]),
        (
            "rust",
            &[
                "match_block",
                "use_list",
                "field_declaration_list",
                "enum_variant_list",
            ],
        ),
        (
            "java",
            &[
                "switch_block",
                "annotation_argument_list",
                "element_value_array_initializer",
            ],
        ),
        (
            "go",
            &[
                "communication_case",
                "type_case",
                "expression_case",
                "default_case",
            ],
        ),
        ("swift", &["switch_entry"]),
        (
            "csharp",
            &["switch_section", "accessor_list", "attribute_list"],
        ),
        (
            "c",
            &["case_statement", "initializer_list", "preproc_params"],
        ),
        (
            "cpp",
            &[
                "case_statement",
                "initializer_list",
                "template_argument_list",
                "base_class_clause",
            ],
        ),
        ("unknown-language", &[]),
    ];
    for &(language, blocks) in cases {
        let config = walker_config_for(language);
        assert_eq!(config.extra_block_kinds, blocks, "{language}");
        assert!(config.capture_comments, "{language}");
        assert!(config.capture_formatting, "{language}");
        assert_eq!(config.max_depth, 128, "{language}");
    }
}

#[test]
fn common_scanner_tokens_resolve_without_inventing_unknown_text() {
    let cassette = cassette_for("unknown-language");
    for (token, text) in [
        ("_newline", "\n"),
        ("_concat", ""),
        ("string_start", "\""),
        ("string_end", "\""),
        ("not_in", "not in"),
        ("not_is", "not is"),
        ("_ternary_qmark", "?"),
        ("_descendant_operator", " "),
        ("_regex_start", "/"),
        ("_immediate_test", ""),
        ("_quoted_content_test", ""),
        ("_quoted_content", ""),
        ("_external_expansion_sym_test", ""),
        ("_virtual_test", ""),
        ("_layout_test", ""),
        ("_multi_test", ""),
        ("_tq_test", ""),
    ] {
        assert_eq!(cassette.external_token_default(token), None, "{token}");
        assert_eq!(
            resolve_external_token(cassette.as_ref(), token),
            Some(text),
            "{token}"
        );
    }
    for token in [
        "unknown",
        "immediate_test",
        "_immediate",
        "_virtual",
        "_layout",
        "_multi",
        "_tq",
    ] {
        assert_eq!(
            resolve_external_token(cassette.as_ref(), token),
            None,
            "{token}"
        );
    }
    assert!(!cassette.separator_is_line_break(";"));
    assert!(!cassette.kind_is_tight_content("text"));
    assert!(!cassette.operator_is_tight("variable_assignment", "="));
    assert!(!cassette.external_is_newline("_virtual_end_decl"));
    assert!(!cassette.external_leads_no_space("interpolation_start_quote"));
}

#[test]
fn language_scanner_overrides_take_precedence_over_common_fallback() {
    let cases: &[(&[&str], &str, &str)] = &[
        (&["julia"], "_immediate_string_start", "\""),
        (&["julia"], "_end_str", "\""),
        (&["julia"], "_immediate_command_start", "`"),
        (&["julia"], "_end_cmd", "`"),
        (&["julia"], "_immediate_paren", ""),
        (&["ruby", "crystal"], "_line_break", "\n"),
        (&["ruby", "crystal"], "_no_line_break", ""),
        (&["ocaml", "ocaml_interface"], "_quoted_string_start", "{|"),
        (&["ocaml", "ocaml_interface"], "_quoted_string_end", "|}"),
        (
            &[
                "html", "vue", "svelte", "astro", "blade", "angular", "templ", "heex",
            ],
            "_interpolation_start",
            "{{",
        ),
        (&["html"], "_interpolation_end", "}}"),
        (&["html"], "_start_tag_name", ""),
        (&["bash", "zsh", "fish"], "_immediate_double_hash", "##"),
        (&["bash"], "file_descriptor", ""),
        (
            &[
                "agda",
                "fsharp",
                "fsharp_signature",
                "earthfile",
                "firrtl",
                "cooklang",
                "djot",
                "idris",
                "nim",
                "purescript",
                "haskell",
            ],
            "_dot",
            ".",
        ),
        (&["kotlin"], "_primary_constructor_keyword", "constructor"),
    ];
    for &(languages, token, expected) in cases {
        for &language in languages {
            let cassette = cassette_for(language);
            assert_eq!(
                cassette.external_token_default(token),
                Some(expected),
                "{language}/{token}"
            );
            assert_eq!(
                resolve_external_token(cassette.as_ref(), token),
                Some(expected),
                "{language}/{token}"
            );
            assert_eq!(
                cassette.external_token_default("unknown-token"),
                None,
                "{language}"
            );
        }
    }
}

#[test]
fn lexical_spacing_overrides_apply_only_to_their_language_and_role() {
    for language in ["python", "starlark", "bitbake"] {
        let c = cassette_for(language);
        assert!(c.separator_is_line_break(";"), "{language}");
        assert!(!c.separator_is_line_break(","), "{language}");
        assert_eq!(c.external_token_default("_newline"), None);
    }
    for language in ["bash", "zsh", "fish"] {
        let c = cassette_for(language);
        for token in ["=", "+="] {
            assert!(c.operator_is_tight("variable_assignment", token));
            assert!(!c.operator_is_tight("test_command", token));
        }
        assert!(!c.operator_is_tight("variable_assignment", "+"));
    }
    let r = cassette_for("r");
    for (rule, tokens) in [
        ("namespace_operator", &["::", ":::"][..]),
        ("extract_operator", &["$", "@"][..]),
        ("binary_operator", &[":"][..]),
        ("unary_operator", &["!", "-", "+", "~", "?"][..]),
    ] {
        for token in tokens {
            assert!(r.operator_is_tight(rule, token), "{rule}/{token}");
            assert!(!r.operator_is_tight("unrelated_rule", token));
        }
        assert!(!r.operator_is_tight(rule, "="));
    }
    assert!(!r.operator_is_tight("binary_operator", "+"));
    assert_eq!(r.external_token_default("_newline"), None);
}

#[test]
fn markup_strings_and_virtual_terminators_preserve_lexical_boundaries() {
    let html = cassette_for("html");
    for kind in ["text", "raw_text"] {
        assert!(html.kind_is_tight_content(kind));
    }
    assert!(!html.kind_is_tight_content("element"));
    let csharp = cassette_for("csharp");
    for token in [
        "interpolation_start_quote",
        "interpolation_open_brace",
        "interpolation_close_brace",
        "interpolation_end_quote",
    ] {
        assert!(csharp.external_leads_no_space(token));
        assert!(!cassette_for("cpp").external_leads_no_space(token));
    }
    for kind in [
        "string_content",
        "interpolation_brace",
        "interpolation_quote",
        "raw_string_content",
    ] {
        assert!(csharp.kind_is_tight_content(kind));
    }
    assert!(!csharp.external_leads_no_space("identifier"));
    assert!(!csharp.kind_is_tight_content("identifier"));
    assert_eq!(csharp.external_token_default("_newline"), None);
    let elm = cassette_for("elm");
    assert!(elm.external_is_newline("_virtual_end_decl"));
    assert!(elm.external_is_newline("_virtual_end_section"));
    assert!(!elm.external_is_newline("_virtual_open_section"));
    assert_eq!(elm.external_token_default("_newline"), None);
}

#[test]
fn default_c_family_and_javascript_cassettes_share_fallback_contracts() {
    for language in [
        "unknown-language",
        "cpp",
        "cuda",
        "hlsl",
        "arduino",
        "c",
        "javascript",
        "typescript",
        "tsx",
        "qml",
        "rescript",
    ] {
        let c = cassette_for(language);
        assert_eq!(c.external_token_default("string_start"), None, "{language}");
        assert_eq!(
            resolve_external_token(c.as_ref(), "string_start"),
            Some("\""),
            "{language}"
        );
        assert_eq!(
            resolve_external_token(c.as_ref(), "raw_string_content"),
            Some(""),
            "{language}"
        );
        assert_eq!(
            resolve_external_token(c.as_ref(), "unknown-token"),
            None,
            "{language}"
        );
        assert!(!c.separator_is_line_break(";"), "{language}");
        assert!(
            !c.operator_is_tight("variable_assignment", "="),
            "{language}"
        );
        assert!(!c.kind_is_tight_content("text"), "{language}");
    }
}

#[test]
#[allow(clippy::unwrap_used)]
fn extracted_fields_distinguish_required_optional_and_ordered_roles() {
    let json = br#"[
        {"@generated": true},
        {"type":"record","named":true,"fields":{
            "required":{"required":true,"multiple":false,"types":[{"type":"identifier","named":true}]},
            "optional":{"required":false,"multiple":false,"types":[{"type":"identifier","named":true}]},
            "items":{"required":true,"multiple":true,"types":[{"type":"identifier","named":true}]}
        }},
        {"type":"identifier","named":true},
        {"type":",","named":false}
    ]"#;
    let entries = panproto_parse::theory_extract::parse_node_types(json).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].node_type, "record");
    let meta = panproto_parse::extract_theory_from_node_types("ThRoles", json).unwrap();
    assert_eq!(meta.vertex_kinds, ["record", "identifier"]);
    assert_eq!(meta.optional_fields.len(), 1);
    assert!(meta.optional_fields.contains("optional"));
    assert_eq!(meta.ordered_fields.len(), 1);
    assert!(meta.ordered_fields.contains("items"));
    assert_eq!(meta.edge_kinds.len(), 3);
}

#[cfg(feature = "group-core")]
#[test]
#[allow(clippy::unwrap_used)]
fn explicit_scope_queries_filter_references_and_update_parser_names() {
    use panproto_parse::languages::common::LanguageParser;
    use panproto_parse::{AstParser, ScopeDetector, WalkerConfig};

    let grammar = panproto_grammars::grammars()
        .into_iter()
        .find(|g| g.name == "rust")
        .unwrap();
    let source = b"fn first() { second(); }\nfn second() {}\n";
    let definitions = "(function_item name: (identifier) @name) @definition.function";
    let references = "(call_expression function: (identifier) @name) @reference.call";
    let mut none = ScopeDetector::new(&grammar.language, None, None).unwrap();
    assert!(!none.has_query());
    assert!(none.scopes(source).is_empty());
    let mut detector =
        ScopeDetector::new(&grammar.language, Some(definitions), Some(references)).unwrap();
    assert!(detector.has_query());
    let mut scopes = detector.scopes(source);
    scopes.sort_by_key(|s| s.node_range.start);
    assert_eq!(scopes.len(), 2);
    assert_eq!(scopes[0].name, "first");
    assert_eq!(scopes[0].kind, ScopeKind::Function);
    assert_eq!(&source[scopes[0].name_range.clone()], b"first");
    assert_eq!(
        &source[scopes[0].node_range.clone()],
        b"fn first() { second(); }"
    );
    assert_eq!(scopes[1].name, "second");
    let mut parser = LanguageParser::from_language(
        "rust",
        vec!["rs"],
        grammar.language,
        grammar.node_types,
        None,
        WalkerConfig::standard(),
    )
    .unwrap();
    assert_eq!(parser.protocol_name(), "rust");
    assert_eq!(parser.supported_extensions(), ["rs"]);
    assert!(!parser.theory_meta().vertex_kinds.is_empty());
    assert!(
        !parser
            .parse(source, "fixture.rs")
            .unwrap()
            .vertices
            .contains_key("fixture.rs::first")
    );
    parser
        .set_tags_override(Some(definitions.to_owned()))
        .unwrap();
    let named = parser.parse(source, "fixture.rs").unwrap();
    assert!(named.vertices.contains_key("fixture.rs::first"));
    assert!(named.vertices.contains_key("fixture.rs::second"));
    assert_eq!(parser.emit(&named).unwrap(), source);
    assert!(parser.emit_pretty(&named).is_err());
    assert!(
        parser
            .set_tags_override(Some("(not_a_rust_node) @name".to_owned()))
            .is_err()
    );
    assert!(
        parser
            .parse(source, "fixture.rs")
            .unwrap()
            .vertices
            .contains_key("fixture.rs::first")
    );
    parser.set_tags_override(None).unwrap();
    assert!(
        !parser
            .parse(source, "fixture.rs")
            .unwrap()
            .vertices
            .contains_key("fixture.rs::first")
    );
}
