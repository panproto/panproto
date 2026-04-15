//! Per-language [`WalkerConfig`][crate::walker::WalkerConfig] overrides.
//!
//! Most languages work well with [`WalkerConfig::standard()`][crate::walker::WalkerConfig::standard]. Overrides here
//! declare additional block node kinds so that sibling statements inside
//! non-standard containers (match arms, switch entries, case statements) get
//! positional IDs that don't cascade on insertion.
//!
//! Named-scope detection is NOT configured here: it is driven by the
//! grammar's own `queries/tags.scm` via [`ScopeDetector`]. The tags query
//! already encodes every language's definition syntax (e.g. Rust's
//! `function_item`, Python's `function_definition`, Haskell's `function`)
//! without hardcoded node-kind lists.
//!
//! [`ScopeDetector`]: crate::scope_detector::ScopeDetector

use crate::walker::WalkerConfig;

/// Return the [`WalkerConfig`] for a given language protocol name.
///
/// Languages without a custom override get [`WalkerConfig::standard()`][crate::walker::WalkerConfig::standard].
#[must_use]
pub fn walker_config_for(lang: &str) -> WalkerConfig {
    match lang {
        "python" => python_config(),
        "typescript" => typescript_config(),
        "tsx" => tsx_config(),
        "rust" => rust_config(),
        "java" => java_config(),
        "go" => go_config(),
        "swift" => swift_config(),
        "csharp" => csharp_config(),
        "c" => c_config(),
        "cpp" => cpp_config(),
        _ => WalkerConfig::standard(),
    }
}

fn python_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "argument_list".to_owned(),
            "expression_list".to_owned(),
            "pattern_list".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn typescript_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec!["switch_body".to_owned(), "template_string".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn tsx_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec!["switch_body".to_owned(), "jsx_expression".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn rust_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "match_block".to_owned(),
            "use_list".to_owned(),
            "field_declaration_list".to_owned(),
            "enum_variant_list".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn java_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "switch_block".to_owned(),
            "annotation_argument_list".to_owned(),
            "element_value_array_initializer".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn go_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "communication_case".to_owned(),
            "type_case".to_owned(),
            "expression_case".to_owned(),
            "default_case".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn swift_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec!["switch_entry".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn csharp_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "switch_section".to_owned(),
            "accessor_list".to_owned(),
            "attribute_list".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn c_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "case_statement".to_owned(),
            "initializer_list".to_owned(),
            "preproc_params".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn cpp_config() -> WalkerConfig {
    WalkerConfig {
        extra_block_kinds: vec![
            "case_statement".to_owned(),
            "initializer_list".to_owned(),
            "template_argument_list".to_owned(),
            "base_class_clause".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}
