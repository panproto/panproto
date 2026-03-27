//! Per-language `WalkerConfig` overrides.
//!
//! Most languages work well with the default `WalkerConfig`. These overrides
//! add language-specific scope and block node kinds for improved ID generation
//! in languages with unusual AST structures.

use crate::walker::WalkerConfig;

/// Return the `WalkerConfig` for a given language protocol name.
///
/// Languages without a custom override get `WalkerConfig::default()`, which
/// uses the standard scope-introducing kinds (function, class, method, etc.)
/// and standard block kinds (`block`, `statement_block`, `compound_statement`, etc.)
/// defined in the walker module.
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
        _ => WalkerConfig::default(),
    }
}

fn python_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "decorated_definition".to_owned(),
            "lambda".to_owned(),
            "list_comprehension".to_owned(),
            "dictionary_comprehension".to_owned(),
            "set_comprehension".to_owned(),
            "generator_expression".to_owned(),
        ],
        extra_block_kinds: vec![
            "argument_list".to_owned(),
            "expression_list".to_owned(),
            "pattern_list".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn typescript_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "arrow_function".to_owned(),
            "generator_function_declaration".to_owned(),
            "type_alias_declaration".to_owned(),
        ],
        extra_block_kinds: vec![
            "switch_body".to_owned(),
            "template_string".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn tsx_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "arrow_function".to_owned(),
            "jsx_element".to_owned(),
            "jsx_self_closing_element".to_owned(),
        ],
        extra_block_kinds: vec![
            "switch_body".to_owned(),
            "jsx_expression".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn rust_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "impl_item".to_owned(),
            "trait_item".to_owned(),
            "mod_item".to_owned(),
            "closure_expression".to_owned(),
            "macro_definition".to_owned(),
        ],
        extra_block_kinds: vec![
            "match_block".to_owned(),
            "use_list".to_owned(),
            "field_declaration_list".to_owned(),
            "enum_variant_list".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn java_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "record_declaration".to_owned(),
            "annotation_type_declaration".to_owned(),
            "lambda_expression".to_owned(),
            "constructor_declaration".to_owned(),
        ],
        extra_block_kinds: vec![
            "switch_block".to_owned(),
            "annotation_argument_list".to_owned(),
            "element_value_array_initializer".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn go_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "func_literal".to_owned(),
            "method_declaration".to_owned(),
            "type_declaration".to_owned(),
        ],
        extra_block_kinds: vec![
            "communication_case".to_owned(),
            "type_case".to_owned(),
            "expression_case".to_owned(),
            "default_case".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn swift_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "protocol_declaration".to_owned(),
            "extension_declaration".to_owned(),
            "closure_expression".to_owned(),
        ],
        extra_block_kinds: vec!["switch_entry".to_owned()],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn csharp_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "record_declaration".to_owned(),
            "namespace_declaration".to_owned(),
            "lambda_expression".to_owned(),
            "local_function_statement".to_owned(),
            "property_declaration".to_owned(),
        ],
        extra_block_kinds: vec![
            "switch_section".to_owned(),
            "accessor_list".to_owned(),
            "attribute_list".to_owned(),
        ],
        name_fields: vec!["name".to_owned(), "identifier".to_owned()],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn c_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "preproc_function_def".to_owned(),
            "preproc_ifdef".to_owned(),
        ],
        extra_block_kinds: vec![
            "case_statement".to_owned(),
            "initializer_list".to_owned(),
            "preproc_params".to_owned(),
        ],
        name_fields: vec![
            "name".to_owned(),
            "identifier".to_owned(),
            "declarator".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}

fn cpp_config() -> WalkerConfig {
    WalkerConfig {
        extra_scope_kinds: vec![
            "template_declaration".to_owned(),
            "namespace_definition".to_owned(),
            "lambda_expression".to_owned(),
            "concept_definition".to_owned(),
        ],
        extra_block_kinds: vec![
            "case_statement".to_owned(),
            "initializer_list".to_owned(),
            "template_argument_list".to_owned(),
            "base_class_clause".to_owned(),
        ],
        name_fields: vec![
            "name".to_owned(),
            "identifier".to_owned(),
            "declarator".to_owned(),
        ],
        capture_comments: true,
        capture_formatting: true,
    }
}
