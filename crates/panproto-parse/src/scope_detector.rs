//! Grammar-driven named-scope detection via tree-sitter `tags.scm` queries.
//!
//! Every tree-sitter grammar ships a `queries/tags.scm` file, consumed by
//! GitHub code navigation, Helix, and the `tree-sitter tags` CLI. The capture
//! vocabulary is standardized:
//!
//! - `@definition.function` / `@definition.method` / `@definition.class` /
//!   `@definition.module` / `@definition.interface` / `@definition.type` /
//!   `@definition.macro` (and more): the scope node
//! - `@name`: the identifier within the scope node
//!
//! This module wraps `tree-sitter-tags` to produce a uniform [`NamedScope`]
//! view of source code across all 248 supported languages. The walker uses
//! the resulting scope map to drive named-scope detection without any
//! hardcoded node-kind lists.

use std::ops::Range;

use tree_sitter::Language;
use tree_sitter_tags::{TagsConfiguration, TagsContext};

use crate::error::ParseError;

/// A named scope discovered by the tags query.
///
/// Represents whatever the grammar's `tags.scm` labels with an
/// `@definition.*` capture paired with `@name`: functions, classes,
/// methods, modules, types, interfaces, macros, or custom definitions.
#[derive(Debug, Clone)]
pub struct NamedScope {
    /// Byte range of the scope node (e.g. the whole `fn foo() { ... }`).
    pub node_range: Range<usize>,
    /// Byte range of the name identifier inside the scope.
    pub name_range: Range<usize>,
    /// The identifier text (e.g. `"foo"`), resolved from `name_range`.
    pub name: String,
    /// Grammar-declared kind: the `@definition.X` capture suffix
    /// (`"function"`, `"method"`, `"class"`, `"module"`, `"interface"`,
    /// `"type"`, `"macro"`, or any custom suffix the grammar defines).
    pub kind: ScopeKind,
}

/// Grammar-declared scope kind, parsed from the `@definition.*` capture.
///
/// Named variants cover the standard tree-sitter tags vocabulary; [`Other`]
/// holds any additional suffix a grammar defines.
///
/// [`Other`]: ScopeKind::Other
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    /// `@definition.function`
    Function,
    /// `@definition.method`
    Method,
    /// `@definition.class`
    Class,
    /// `@definition.module`
    Module,
    /// `@definition.interface`
    Interface,
    /// `@definition.type`
    Type,
    /// `@definition.macro`
    Macro,
    /// Any other `@definition.X` suffix the grammar defines.
    Other(String),
}

impl ScopeKind {
    /// Construct from the `@definition.X` capture suffix.
    #[must_use]
    pub fn from_suffix(s: &str) -> Self {
        match s {
            "function" => Self::Function,
            "method" => Self::Method,
            "class" => Self::Class,
            "module" => Self::Module,
            "interface" => Self::Interface,
            "type" => Self::Type,
            "macro" => Self::Macro,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The canonical capture suffix for this kind.
    #[must_use]
    pub fn as_suffix(&self) -> &str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Macro => "macro",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// A reusable per-language detector that runs a tags query over source bytes
/// and yields [`NamedScope`]s.
///
/// Construct once per grammar (the query is compiled inside
/// [`TagsConfiguration::new`]); reuse across many files. The internal
/// [`TagsContext`] holds a tree-sitter `Parser` and `QueryCursor` that are
/// reset between calls.
pub struct ScopeDetector {
    config: Option<TagsConfiguration>,
    ctx: TagsContext,
}

impl std::fmt::Debug for ScopeDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `TagsContext` doesn't implement Debug; we only expose presence of
        // the config. `finish_non_exhaustive` documents the omission to the
        // reader (and to clippy's missing-fields lint).
        f.debug_struct("ScopeDetector")
            .field("has_config", &self.config.is_some())
            .finish_non_exhaustive()
    }
}

impl ScopeDetector {
    /// Build a detector from a grammar's tags query.
    ///
    /// Pass `None` for `base_query` if the grammar does not ship a
    /// `queries/tags.scm`; the detector is then a no-op (always returns an
    /// empty scope list). Pass `Some(project_override)` to concatenate an
    /// override query in front of the base (tree-sitter unions all patterns,
    /// so override captures augment the grammar defaults).
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::ScopeQueryCompile`] if the combined query fails
    /// to compile against the given language (malformed S-expression, unknown
    /// capture name outside the tags-query vocabulary, etc.).
    pub fn new(
        language: &Language,
        base_query: Option<&str>,
        project_override: Option<&str>,
    ) -> Result<Self, ParseError> {
        let combined = match (base_query, project_override) {
            (None, None) => {
                return Ok(Self {
                    config: None,
                    ctx: TagsContext::new(),
                });
            }
            (Some(base), None) => base.to_owned(),
            (None, Some(ov)) => ov.to_owned(),
            (Some(base), Some(ov)) => format!("{ov}\n{base}"),
        };

        let config = TagsConfiguration::new(language.clone(), &combined, "").map_err(|e| {
            ParseError::ScopeQueryCompile {
                reason: e.to_string(),
            }
        })?;

        Ok(Self {
            config: Some(config),
            ctx: TagsContext::new(),
        })
    }

    /// True when this detector has a compiled tags query and will produce
    /// scopes. A detector constructed from `(None, None)` is a no-op.
    #[must_use]
    pub const fn has_query(&self) -> bool {
        self.config.is_some()
    }

    /// Run the tags query over `source` and return every `@definition.*`
    /// match as a [`NamedScope`].
    ///
    /// Non-definition captures (`@reference.*`, etc.) are filtered out: they
    /// describe call sites, not scopes. `@ignore` matches (used by grammars
    /// like Elixir to suppress false positives) are handled internally by
    /// `tree-sitter-tags`.
    ///
    /// Ordering mirrors tree-sitter's match order (roughly source order,
    /// with potential reordering when patterns overlap). Callers that need a
    /// deterministic byte-ordered index should sort the result.
    #[must_use]
    pub fn scopes(&mut self, source: &[u8]) -> Vec<NamedScope> {
        let Some(config) = self.config.as_ref() else {
            return Vec::new();
        };

        let Ok((iter, _had_parse_error)) = self.ctx.generate_tags(config, source, None) else {
            return Vec::new();
        };

        let mut scopes = Vec::new();
        for tag_result in iter {
            let Ok(tag) = tag_result else { continue };
            if !tag.is_definition {
                continue;
            }

            let syntax = config.syntax_type_name(tag.syntax_type_id);
            let kind = ScopeKind::from_suffix(syntax);

            let Some(name_bytes) = source.get(tag.name_range.clone()) else {
                continue;
            };
            let Ok(name) = std::str::from_utf8(name_bytes) else {
                continue;
            };

            scopes.push(NamedScope {
                node_range: tag.range,
                name_range: tag.name_range,
                name: name.to_owned(),
                kind,
            });
        }
        scopes
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn no_query_is_empty() {
        // Build a detector without any query: scopes() should always return [].
        // We use the rust grammar only to supply a Language for the (unused) ctx;
        // the detector with no config should not invoke it.
        #[cfg(feature = "grammars")]
        {
            let lang = panproto_grammars::grammars()
                .into_iter()
                .find(|g| g.name == "rust")
                .map(|g| g.language);
            if let Some(lang) = lang {
                let mut det = ScopeDetector::new(&lang, None, None).unwrap();
                assert!(!det.has_query());
                assert!(det.scopes(b"fn f() {}").is_empty());
            }
        }
    }

    #[test]
    #[cfg(feature = "grammars")]
    fn rust_function_item_is_detected() {
        let grammar = panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == "rust");
        let Some(g) = grammar else {
            return; // rust grammar not enabled in this feature set
        };
        let tags = g.tags_query;
        if tags.is_none() {
            return; // grammar was fetched without queries/tags.scm
        }
        let mut det = ScopeDetector::new(&g.language, tags, None).unwrap();
        assert!(det.has_query());

        let source = b"fn verify_push(token: &str) -> bool { true }\n\
                       struct Foo { x: u32 }\n";
        let scopes = det.scopes(source);
        let names: Vec<&str> = scopes.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"verify_push"), "got {names:?}");
        assert!(names.contains(&"Foo"), "got {names:?}");

        let fn_scope = scopes.iter().find(|s| s.name == "verify_push").unwrap();
        assert_eq!(fn_scope.kind, ScopeKind::Function);
    }

    #[test]
    #[cfg(feature = "grammars")]
    fn rust_impl_method_is_detected_as_method() {
        let Some(g) = panproto_grammars::grammars()
            .into_iter()
            .find(|g| g.name == "rust")
        else {
            return;
        };
        let Some(tags) = g.tags_query else {
            return;
        };
        let mut det = ScopeDetector::new(&g.language, Some(tags), None).unwrap();

        let source = b"impl Foo { fn bar(&self) {} }";
        let scopes = det.scopes(source);
        let bar = scopes.iter().find(|s| s.name == "bar");
        assert!(bar.is_some(), "expected bar method, got {scopes:?}");
        // Most rust tags.scm versions label impl methods as @definition.method;
        // we accept either Method or Function to tolerate upstream variation.
        let k = &bar.unwrap().kind;
        assert!(matches!(k, ScopeKind::Method | ScopeKind::Function));
    }
}
