//! Declarative lens DSL for panproto.
//!
//! Provides a human-readable specification format for lenses, protolenses,
//! and related optical constructs. Supports Nickel (`.ncl`), JSON, and YAML
//! surface syntax. Nickel is the primary authoring format, providing typed
//! contracts for validation, record merge for fragment composition, functions
//! for parameterized templates, and imports for modularity.
//!
//! ## Evaluation pipeline
//!
//! 1. Surface syntax (Nickel/JSON/YAML) is evaluated to a normalized record
//! 2. The record is deserialized into a [`LensDocument`]
//! 3. The document is compiled to a `ProtolensChain` + `FieldTransform`s
//!
//! ## Example
//!
//! ```no_run
//! use panproto_lens_dsl::{load, compile};
//!
//! let doc = load(std::path::Path::new("my_lens.ncl")).unwrap();
//! let compiled = compile(&doc, "record:body", &|_| None).unwrap();
//! // compiled.chain is a ProtolensChain ready for instantiation
//! // compiled.field_transforms are value-level transforms
//! ```

pub mod compile;
pub mod compose;
pub mod document;
pub mod error;
pub mod eval;
pub mod rules;
pub mod steps;

use std::path::Path;

pub use compile::CompiledLens;
pub use document::LensDocument;
pub use error::LensDslError;

/// Load a lens document from a file.
///
/// Dispatches to the appropriate evaluator based on file extension:
/// - `.ncl` → Nickel evaluation
/// - `.json` → JSON deserialization
/// - `.yaml`, `.yml` → YAML deserialization
///
/// # Errors
///
/// Returns [`LensDslError::UnsupportedExtension`] for unknown extensions,
/// [`LensDslError::Io`] for read errors, or evaluation-specific errors.
pub fn load(path: &Path) -> Result<LensDocument, LensDslError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let source = std::fs::read_to_string(path)?;

    match ext {
        "ncl" => {
            let parent = path.parent().map(Path::to_path_buf);
            let import_paths = parent.into_iter().collect::<Vec<_>>();
            eval::eval_nickel(&source, &import_paths)
        }
        "json" => eval::eval_json(&source),
        "yaml" | "yml" => eval::eval_yaml(&source),
        _ => Err(LensDslError::UnsupportedExtension {
            ext: ext.to_owned(),
        }),
    }
}

/// Result of loading a directory of lens documents.
pub struct LoadDirResult {
    /// Successfully loaded documents.
    pub documents: Vec<LensDocument>,
    /// Files that failed to load, with their paths and errors.
    pub errors: Vec<(std::path::PathBuf, LensDslError)>,
}

/// Load all lens documents from a directory.
///
/// Scans for `.ncl`, `.json`, `.yaml`, and `.yml` files.
/// Files that fail to parse are reported in `errors`; successfully
/// parsed documents are returned in `documents`.
///
/// # Errors
///
/// Returns [`LensDslError::Io`] if the directory itself cannot be read.
/// Per-file errors are returned in [`LoadDirResult::errors`].
pub fn load_dir(dir: &Path) -> Result<LoadDirResult, LensDslError> {
    let mut documents = Vec::new();
    let mut errors = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if matches!(ext, "ncl" | "json" | "yaml" | "yml") {
            match load(&path) {
                Ok(doc) => documents.push(doc),
                Err(e) => errors.push((path, e)),
            }
        }
    }

    Ok(LoadDirResult { documents, errors })
}

/// Compile a [`LensDocument`] to a [`CompiledLens`].
///
/// Convenience re-export of [`compile::compile`].
///
/// # Errors
///
/// See [`compile::compile`] for error conditions.
pub fn compile(
    doc: &LensDocument,
    body_vertex: &str,
    resolver: &dyn Fn(&str) -> Option<CompiledLens>,
) -> Result<CompiledLens, LensDslError> {
    compile::compile(doc, body_vertex, resolver)
}

/// Load a lens file and compile it in one step.
///
/// # Errors
///
/// Combines errors from [`load`] and [`compile()`].
pub fn load_and_compile(path: &Path, body_vertex: &str) -> Result<CompiledLens, LensDslError> {
    let doc = load(path)?;
    compile::compile(&doc, body_vertex, &|_| None)
}
