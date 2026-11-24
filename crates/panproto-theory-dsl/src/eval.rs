//! Evaluation of Nickel, JSON, and YAML sources into [`TheoryDocument`].
//!
//! Three evaluation paths:
//! - **Nickel** (`.ncl`): evaluated via `nickel-lang`, then deserialized via `to_serde`
//! - **JSON** (`.json`): deserialized directly via `serde_json`
//! - **YAML** (`.yaml`, `.yml`): deserialized directly via `yaml_serde`
//!
//! The Nickel path provides contracts, merge composition, functions,
//! and imports. JSON/YAML are pass-through for simple cases.
//!
//! The evaluation machinery lives in
//! [`panproto_dsl_eval`](https://docs.rs/panproto-dsl-eval), which is
//! shared with `panproto-lens-dsl`; this module supplies the theory
//! document type and contract library, then maps the shared error into
//! [`TheoryDslError`], preserving the Nickel source span where one is
//! recovered.

use std::path::PathBuf;

use panproto_dsl_eval::BundledContract;

use crate::document::TheoryDocument;
use crate::error::TheoryDslError;

/// The bundled Nickel contract library source.
///
/// Embedded at compile time via `include_str!` so that
/// `import "panproto/theory.ncl"` resolves without external files.
const THEORY_CONTRACT_SOURCE: &str = include_str!("../contracts/theory.ncl");

/// Evaluate a Nickel source string to a [`TheoryDocument`].
///
/// Sets up an import path so that `import "panproto/theory.ncl"` resolves
/// to the bundled contract library. Additional import paths can be
/// supplied for user-defined Nickel modules.
///
/// # Errors
///
/// Returns [`TheoryDslError::NickelEvalSpanned`] when evaluation fails at a
/// locatable point in `source`, [`TheoryDslError::NickelEval`] when it fails
/// without a recoverable span, and [`TheoryDslError::Shape`] when the
/// exported value is not a document.
pub fn eval_nickel(
    source: &str,
    import_paths: &[PathBuf],
) -> Result<TheoryDocument, TheoryDslError> {
    crate::shape::read_document(&nickel_value(source, import_paths)?)
}

/// Evaluate a Nickel source string to the document value it exports.
///
/// # Errors
///
/// As [`eval_nickel`], before the value is read as a document.
pub fn nickel_value(
    source: &str,
    import_paths: &[PathBuf],
) -> Result<serde_json::Value, TheoryDslError> {
    panproto_dsl_eval::eval_nickel(
        source,
        import_paths,
        &BundledContract {
            file_name: "theory.ncl",
            source: THEORY_CONTRACT_SOURCE,
        },
    )
    .map_err(TheoryDslError::from)
}

/// Evaluate a JSON string to a [`TheoryDocument`].
///
/// # Errors
///
/// Returns [`TheoryDslError::Json`] if parsing fails and
/// [`TheoryDslError::Shape`] if the value is not a document.
pub fn eval_json(source: &str) -> Result<TheoryDocument, TheoryDslError> {
    let value: serde_json::Value = panproto_dsl_eval::eval_json(source)?;
    crate::shape::read_document(&value)
}

/// Evaluate a YAML string to a [`TheoryDocument`].
///
/// # Errors
///
/// Returns [`TheoryDslError::Yaml`] if parsing fails and
/// [`TheoryDslError::Shape`] if the value is not a document.
pub fn eval_yaml(source: &str) -> Result<TheoryDocument, TheoryDslError> {
    crate::shape::read_document(&yaml_value(source)?)
}

/// Parse a YAML string to the document value it holds.
///
/// # Errors
///
/// Returns [`TheoryDslError::Yaml`] if parsing fails.
pub fn yaml_value(source: &str) -> Result<serde_json::Value, TheoryDslError> {
    Ok(panproto_dsl_eval::eval_yaml(source)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A malformed Nickel source whose evaluation fails carries a source
    /// span pointing at the offending token, recovered from the Nickel
    /// diagnostic rather than lost through evaluation.
    #[test]
    fn nickel_eval_error_carries_span() {
        // `undefined_xyz` parses but is unbound: Nickel reports an
        // evaluation error with a primary label spanning the identifier.
        let source = "undefined_xyz";
        let Err(err) = eval_nickel(source, &[]) else {
            panic!("unbound identifier must fail");
        };

        let TheoryDslError::NickelEvalSpanned { span, src, message } = err else {
            panic!("expected a span-carrying NickelEvalSpanned, got: {err:?}");
        };

        assert_eq!(src, source, "the reported source must be the input");
        assert_eq!(span.offset(), 0, "span should start at the identifier");
        assert_eq!(
            span.len(),
            source.len(),
            "span should cover the whole unbound identifier",
        );
        // The recovered span must address real bytes of the source.
        assert_eq!(
            &src[span.offset()..span.offset() + span.len()],
            "undefined_xyz",
        );
        assert!(
            message.contains("unbound identifier"),
            "message should carry Nickel's diagnostic text: {message}",
        );
    }

    /// A span is recovered even when the offending token sits partway
    /// through the source, not only at offset zero.
    #[test]
    fn nickel_eval_span_points_at_inner_token() {
        // A type error localizes to the string literal `"a"`.
        let source = "1 + \"a\"";
        let Err(err) = eval_nickel(source, &[]) else {
            panic!("type error must fail");
        };

        let TheoryDslError::NickelEvalSpanned { span, src, .. } = err else {
            panic!("expected NickelEvalSpanned, got: {err:?}");
        };
        assert!(span.offset() > 0, "inner token should not start at zero");
        assert_eq!(
            &src[span.offset()..span.offset() + span.len()],
            "\"a\"",
            "span should cover the offending string literal",
        );
    }

    /// When evaluation succeeds but the value is not a document, the error
    /// is the grammar's refusal at the value's path: there is no Nickel
    /// diagnostic, hence no source span.
    #[test]
    fn a_nickel_value_that_is_not_a_document_is_refused_by_the_grammar() {
        let Err(err) = eval_nickel("{ foo = 1 }", &[]) else {
            panic!("bad document must fail");
        };
        assert!(
            matches!(
                err,
                TheoryDslError::Shape {
                    problem: crate::shape::ShapeProblem::NoBody,
                    ..
                }
            ),
            "{err:?}",
        );
    }
}
