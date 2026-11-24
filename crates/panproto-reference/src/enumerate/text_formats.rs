//! Complete finite scopes for the text-formats page.

use crate::pages::text_formats::toml::Statement;
use crate::pages::text_formats::yaml::{Directive, Node as YamlNode};

/// Every key path of length one or two over the keys `a` and `b`.
#[must_use]
pub fn toml_key_paths() -> Vec<Vec<String>> {
    super::sequences(&["a".to_owned(), "b".to_owned()], 2)
        .into_iter()
        .filter(|path| !path.is_empty())
        .collect()
}

/// Every pair, header and array-of-tables header over [`toml_key_paths`].
#[must_use]
pub fn toml_statements() -> Vec<Statement> {
    toml_key_paths()
        .into_iter()
        .flat_map(|path| {
            [
                Statement::Pair(path.clone()),
                Statement::Header(path.clone()),
                Statement::ArrayHeader(path),
            ]
        })
        .collect()
}

/// Every sequence of at most three [`toml_statements`].
#[must_use]
pub fn toml_documents() -> Vec<Vec<Statement>> {
    super::sequences(&toml_statements(), 3)
}

/// The count of [`toml_documents`]: three statement forms over six key
/// paths give eighteen statements, and `sum_{l <= 3} 18^l` sequences.
#[must_use]
pub fn toml_documents_count() -> u64 {
    super::sequences_count(3 * (2 + 2 * 2), 3)
}

/// The directive alphabet: `%YAML` at 1.1, 1.2, 1.3 and 2.0, `%TAG`, and
/// one reserved directive.
#[must_use]
pub const fn yaml_directives() -> [Directive; 6] {
    [
        Directive::Version { major: 1, minor: 1 },
        Directive::Version { major: 1, minor: 2 },
        Directive::Version { major: 1, minor: 3 },
        Directive::Version { major: 2, minor: 0 },
        Directive::Tag,
        Directive::Reserved,
    ]
}

/// Every sequence of at most three [`yaml_directives`].
#[must_use]
pub fn yaml_directive_sequences() -> Vec<Vec<Directive>> {
    super::sequences(&yaml_directives(), 3)
}

/// The count of [`yaml_directive_sequences`]: `sum_{l <= 3} 6^l`.
#[must_use]
pub fn yaml_directive_sequences_count() -> u64 {
    super::sequences_count(6, 3)
}

/// The TOML value tokens of the whole-document scope: an integer, a basic
/// and a literal string, a boolean, a hexadecimal integer, and `01`, which
/// the TOML integer grammar refuses.
#[must_use]
pub fn toml_value_tokens() -> Vec<Vec<u8>> {
    ["1", "\"x\"", "'y'", "true", "0x1F", "01"]
        .iter()
        .map(|token| token.as_bytes().to_vec())
        .collect()
}

/// Every statement of the whole-document scope with its token: a header or
/// an array-of-tables header over [`toml_key_paths`] with no token, or a
/// pair over them with each of [`toml_value_tokens`].
#[must_use]
pub fn toml_token_statements() -> Vec<(Statement, Vec<u8>)> {
    let mut out = Vec::new();
    for path in toml_key_paths() {
        out.push((Statement::Header(path.clone()), Vec::new()));
        out.push((Statement::ArrayHeader(path.clone()), Vec::new()));
        for token in toml_value_tokens() {
            out.push((Statement::Pair(path.clone()), token));
        }
    }
    out
}

/// Every sequence of at most two [`toml_token_statements`].
#[must_use]
pub fn toml_token_documents() -> Vec<Vec<(Statement, Vec<u8>)>> {
    super::sequences(&toml_token_statements(), 2)
}

/// The count of [`toml_token_documents`]: six paths give twelve headers
/// and thirty-six pairs, and `sum_{l <= 2} 48^l` sequences.
#[must_use]
pub fn toml_token_documents_count() -> u64 {
    super::sequences_count(6 * 2 + 6 * 6, 2)
}

/// The YAML scalar tokens of the block-document scope: the empty scalar, a
/// negative integer, `yes` (a string under the core schema) and a
/// single-quoted string.
#[must_use]
pub fn yaml_tokens() -> Vec<String> {
    ["", "-1", "yes", "'q'"]
        .iter()
        .map(|token| (*token).to_owned())
        .collect()
}

fn yaml_nodes(depth: usize) -> Vec<YamlNode> {
    let scalars = yaml_tokens().into_iter().map(YamlNode::Scalar);
    if depth == 0 {
        return scalars.collect();
    }
    let below = yaml_nodes(depth - 1);
    let mut out: Vec<YamlNode> = scalars.collect();
    out.push(YamlNode::Mapping(Vec::new()));
    for key in ["a", "b"] {
        for value in &below {
            out.push(YamlNode::Mapping(vec![(key.to_owned(), value.clone())]));
        }
    }
    for (first, second) in [("a", "b"), ("b", "a")] {
        for value in &below {
            for other in &below {
                out.push(YamlNode::Mapping(vec![
                    (first.to_owned(), value.clone()),
                    (second.to_owned(), other.clone()),
                ]));
            }
        }
    }
    for items in super::sequences(&below, 2) {
        out.push(YamlNode::Sequence(items));
    }
    out
}

/// Every block document of depth at most two over [`yaml_tokens`].
///
/// A mapping holds none, one or two of the keys `a` and `b`, and a sequence
/// at most two items. The top-level empty scalar is left out: its
/// rendering is an empty stream.
#[must_use]
pub fn yaml_block_documents() -> Vec<YamlNode> {
    yaml_nodes(2)
        .into_iter()
        .filter(|node| *node != YamlNode::Scalar(String::new()))
        .collect()
}

/// The count of [`yaml_block_documents`]: with `t = 4` tokens and `f` the
/// count one level down, a level holds `t + (1 + 2f + 2f^2) + (1 + f + f^2)`
/// documents, and the top-level empty scalar is left out.
#[must_use]
pub fn yaml_block_documents_count() -> u64 {
    let level = |below: u64| 4 + (1 + 2 * below + 2 * below * below) + (1 + below + below * below);
    level(level(4)) - 1
}
