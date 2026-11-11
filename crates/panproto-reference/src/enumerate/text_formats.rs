//! Complete finite scopes for the text-formats page.

use crate::pages::text_formats::toml::Statement;
use crate::pages::text_formats::yaml::Directive;

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
