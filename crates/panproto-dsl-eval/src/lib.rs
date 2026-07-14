//! Shared evaluation of Nickel, JSON, and YAML sources for panproto's
//! declarative DSLs.
//!
//! [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl) and
//! [`panproto-theory-dsl`](https://docs.rs/panproto-theory-dsl) both load
//! declarative documents from three source formats, and the evaluation
//! machinery is identical across them:
//!
//! - **Nickel** (`.ncl`): evaluated via [`nickel-lang`](https://docs.rs/nickel-lang),
//!   with a bundled contract library staged on the import path, then
//!   deserialized via `to_serde`.
//! - **JSON** (`.json`): deserialized directly via
//!   [`serde_json`](https://docs.rs/serde_json).
//! - **YAML** (`.yaml`, `.yml`): deserialized directly via
//!   [`yaml_serde`](https://docs.rs/yaml_serde).
//!
//! This crate owns `nickel-lang`, `yaml_serde`, and the temp-directory
//! contract staging so that each DSL crate depends on it once rather than
//! compiling the Nickel evaluator itself. The functions are generic over
//! the target document type; each DSL crate supplies its own document and
//! contract library and maps [`DslEvalError`] into its own error type.

use std::ffi::OsString;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use thiserror::Error;

/// A bundled Nickel contract library, staged on the import path so that
/// `import "panproto/<file_name>"` resolves during evaluation.
pub struct BundledContract<'a> {
    /// File name under the staged `panproto/` import directory (e.g.
    /// `lens.ncl`). Doubles as the Nickel source name.
    pub file_name: &'a str,
    /// The contract library source, embedded by the caller via
    /// `include_str!`.
    pub source: &'a str,
}

/// Errors from evaluating a DSL source into a document type.
#[derive(Debug, Error)]
pub enum DslEvalError {
    /// Nickel evaluation, contract checking, or deserialization failed.
    #[error("nickel evaluation failed: {message}")]
    NickelEval {
        /// Human-readable message from the Nickel evaluator.
        message: String,
    },

    /// Nickel evaluation failed at a locatable point in the source. Carries
    /// the source text and the byte span `(offset, length)` of the offending
    /// token, recovered from the Nickel diagnostic.
    #[error("nickel evaluation failed: {message}")]
    NickelEvalSpanned {
        /// Human-readable message from the Nickel evaluator.
        message: String,
        /// The evaluated source text the span indexes into.
        src: String,
        /// The byte span `(offset, length)` of the offending token.
        span: (usize, usize),
    },

    /// JSON deserialization failed.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// YAML deserialization failed.
    #[error("YAML parse error: {message}")]
    Yaml {
        /// Human-readable message from the YAML deserializer.
        message: String,
    },

    /// Staging the bundled contract to a temp directory failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Evaluate a Nickel source string into `T`.
///
/// Stages `contract` under a temporary `panproto/` directory, adds that
/// directory plus any `import_paths` to Nickel's import resolution,
/// evaluates `source` deeply for export, and deserializes the result
/// into `T`.
///
/// # Errors
///
/// Returns [`DslEvalError::NickelEval`] if evaluation, contract checking,
/// or deserialization fails, or [`DslEvalError::Io`] if the bundled
/// contract cannot be staged.
pub fn eval_nickel<T: DeserializeOwned>(
    source: &str,
    import_paths: &[PathBuf],
    contract: &BundledContract<'_>,
) -> Result<T, DslEvalError> {
    // Write the bundled contract to a temp directory so Nickel can import it.
    let contract_dir = write_bundled_contract(contract)?;
    let contract_path = contract_dir.path().as_os_str().to_os_string();

    let mut paths: Vec<OsString> = vec![contract_path];
    paths.extend(import_paths.iter().map(|p| p.as_os_str().to_os_string()));

    let mut ctx = nickel_lang::Context::new()
        .with_added_import_paths(paths)
        .with_source_name(contract.file_name.to_owned());

    let expr = ctx
        .eval_deep_for_export(source)
        .map_err(|e| nickel_eval_error(&e, source))?;

    expr.to_serde::<T>().map_err(|e| DslEvalError::NickelEval {
        message: format!("deserialization failed: {e}"),
    })
}

/// Convert a Nickel evaluation error into a [`DslEvalError`], recovering the
/// source span from the diagnostic where one is available.
///
/// The message comes from Nickel's text renderer; the span comes from the same
/// diagnostic's JSON form. When a diagnostic label resolves to a byte range
/// inside `source` the error is [`DslEvalError::NickelEvalSpanned`], otherwise
/// it is the un-spanned [`DslEvalError::NickelEval`].
fn nickel_eval_error(err: &nickel_lang::Error, source: &str) -> DslEvalError {
    let mut buf = Vec::new();
    let message = if err.format(&mut buf, nickel_lang::ErrorFormat::Text).is_ok() {
        String::from_utf8_lossy(&buf).into_owned()
    } else {
        format!("{err:?}")
    };
    match nickel_error_span(err, source) {
        Some(span) => DslEvalError::NickelEvalSpanned {
            message,
            src: source.to_owned(),
            span,
        },
        None => DslEvalError::NickelEval { message },
    }
}

/// Recover a source byte span `(offset, length)` from a Nickel error's
/// diagnostics.
///
/// Nickel hides its internal spans but exposes them through the JSON
/// diagnostic form: an array of diagnostics, each with `labels` carrying a
/// `range` of byte offsets. This locates a label whose range is a valid slice
/// of `source`, preferring the primary (error-site) label. A label is accepted
/// only when its range lies within `source`, so an out-of-source range (for
/// example one pointing into an imported module) yields `None` rather than a
/// misplaced span.
fn nickel_error_span(err: &nickel_lang::Error, source: &str) -> Option<(usize, usize)> {
    let mut buf = Vec::new();
    err.format(&mut buf, nickel_lang::ErrorFormat::Json).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&buf).ok()?;
    let diagnostics = json.get("diagnostics")?.as_array()?;

    let labels = diagnostics
        .iter()
        .filter_map(|d| d.get("labels")?.as_array())
        .flatten();

    let mut fallback: Option<(usize, usize)> = None;
    for label in labels {
        let Some(range) = label.get("range") else {
            continue;
        };
        let (Some(start), Some(end)) = (
            range.get("start").and_then(serde_json::Value::as_u64),
            range.get("end").and_then(serde_json::Value::as_u64),
        ) else {
            continue;
        };
        let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
            continue;
        };
        // Only accept ranges that are a valid slice of the evaluated source;
        // otherwise the label points into another file.
        if start > end || end > source.len() {
            continue;
        }
        if label.get("style").and_then(serde_json::Value::as_str) == Some("Primary") {
            return Some((start, end - start));
        }
        fallback.get_or_insert((start, end - start));
    }

    fallback
}

/// Evaluate a JSON source string into `T`.
///
/// # Errors
///
/// Returns [`DslEvalError::Json`] if parsing fails.
pub fn eval_json<T: DeserializeOwned>(source: &str) -> Result<T, DslEvalError> {
    Ok(serde_json::from_str(source)?)
}

/// Evaluate a YAML source string into `T`.
///
/// # Errors
///
/// Returns [`DslEvalError::Yaml`] if parsing fails.
pub fn eval_yaml<T: DeserializeOwned>(source: &str) -> Result<T, DslEvalError> {
    yaml_serde::from_str(source).map_err(|e| DslEvalError::Yaml {
        message: e.to_string(),
    })
}

/// Stage the bundled contract under `<tmpdir>/panproto/<file_name>` so
/// that Nickel's import resolution finds it when a document writes
/// `import "panproto/<file_name>"`.
fn write_bundled_contract(
    contract: &BundledContract<'_>,
) -> Result<tempfile::TempDir, std::io::Error> {
    let dir = tempfile::tempdir()?;
    let contract_dir = dir.path().join("panproto");
    std::fs::create_dir_all(&contract_dir)?;
    std::fs::write(contract_dir.join(contract.file_name), contract.source)?;
    Ok(dir)
}
