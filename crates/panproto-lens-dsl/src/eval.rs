//! Evaluation of Nickel, JSON, and YAML sources into [`LensDocument`].
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
/// Returns [`LensDslError::NickelEval`] if evaluation or contract
/// checking fails, or a deserialization error if the evaluated record
/// does not match [`LensDocument`].
pub fn eval_nickel(source: &str, import_paths: &[PathBuf]) -> Result<LensDocument, LensDslError> {
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
/// Returns [`LensDslError::Json`] if parsing fails.
pub fn eval_json(source: &str) -> Result<LensDocument, LensDslError> {
    panproto_dsl_eval::eval_json(source).map_err(LensDslError::from)
}

/// Evaluate a YAML string to a [`LensDocument`].
///
/// # Errors
///
/// Returns [`LensDslError::Yaml`] if parsing fails.
pub fn eval_yaml(source: &str) -> Result<LensDocument, LensDslError> {
    panproto_dsl_eval::eval_yaml(source).map_err(LensDslError::from)
}
