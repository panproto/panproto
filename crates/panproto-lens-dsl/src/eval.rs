//! Evaluation of Nickel, JSON, and YAML sources into [`LensDocument`].
//!
//! Three evaluation paths:
//! - **Nickel** (`.ncl`): evaluated via `nickel-lang` to a value
//! - **JSON** (`.json`): parsed via `serde_json`
//! - **YAML** (`.yaml`, `.yml`): parsed via `yaml_serde`
//!
//! The Nickel path provides contracts, merge composition, functions,
//! and imports. Whatever the format, the resulting value is checked
//! against the document grammar ([`crate::shape`]) before it is read.
//!
//! The evaluation machinery lives in
//! [`panproto_dsl_eval`](https://docs.rs/panproto-dsl-eval), which is
//! shared with `panproto-theory-dsl`; this module supplies the lens
//! document type and contract library, then maps the shared error into
//! [`LensDslError`].

use std::path::PathBuf;

use panproto_dsl_eval::BundledContract;

use crate::document::LensDocument;
use crate::error::LensDslError;

/// The bundled Nickel contract library source.
///
/// This is embedded at compile time via `include_str!` so that
/// `import "panproto/lens.ncl"` resolves without external files.
const LENS_CONTRACT_SOURCE: &str = include_str!("../contracts/lens.ncl");

/// Evaluate a Nickel source string to a [`LensDocument`].
///
/// Sets up an import path so that `import "panproto/lens.ncl"` resolves
/// to the bundled contract library. Additional import paths can be
/// supplied for user-defined Nickel modules.
///
/// # Errors
///
/// Returns [`LensDslError::NickelEvalSpanned`] when evaluation fails at a
/// place in `source` that Nickel's diagnostic names,
/// [`LensDslError::NickelEval`] when it fails without one, and
/// [`LensDslError::Shape`] when the exported value is not a document.
pub fn eval_nickel(source: &str, import_paths: &[PathBuf]) -> Result<LensDocument, LensDslError> {
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
) -> Result<serde_json::Value, LensDslError> {
    panproto_dsl_eval::eval_nickel(
        source,
        import_paths,
        &BundledContract {
            file_name: "lens.ncl",
            source: LENS_CONTRACT_SOURCE,
        },
    )
    .map_err(LensDslError::from)
}

/// Evaluate a JSON string to a [`LensDocument`].
///
/// # Errors
///
/// Returns [`LensDslError::Json`] if parsing fails and
/// [`LensDslError::Shape`] if the value is not a document.
pub fn eval_json(source: &str) -> Result<LensDocument, LensDslError> {
    let value: serde_json::Value = panproto_dsl_eval::eval_json(source)?;
    crate::shape::read_document(&value)
}

/// Evaluate a YAML string to a [`LensDocument`].
///
/// # Errors
///
/// Returns [`LensDslError::Yaml`] if parsing fails and
/// [`LensDslError::Shape`] if the value is not a document.
pub fn eval_yaml(source: &str) -> Result<LensDocument, LensDslError> {
    crate::shape::read_document(&yaml_value(source)?)
}

/// Parse a YAML string to the document value it holds.
///
/// # Errors
///
/// Returns [`LensDslError::Yaml`] if parsing fails.
pub fn yaml_value(source: &str) -> Result<serde_json::Value, LensDslError> {
    Ok(panproto_dsl_eval::eval_yaml(source)?)
}
