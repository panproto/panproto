//! Declarative theory DSL for panproto.
//!
//! Provides a human-readable specification format for GAT theories,
//! theory morphisms, compositions, and protocols. Supports Nickel
//! (`.ncl`), JSON, and YAML surface syntax. Nickel is the primary
//! authoring format, providing typed contracts for validation, record
//! merge for fragment composition, functions for parameterized
//! templates, and imports for modularity.
//!
//! ## Evaluation pipeline
//!
//! 1. Surface syntax (Nickel/JSON/YAML) is evaluated to a normalized record
//! 2. The record is deserialized into a [`TheoryDocument`]
//! 3. The document is compiled to `Theory` + `TheoryMorphism` + `Protocol`
//!
//! ## Example
//!
//! ```no_run
//! use panproto_theory_dsl::{load, compile, builtin_resolver};
//!
//! let doc = load(std::path::Path::new("my_theory.json")).unwrap();
//! let resolver = builtin_resolver();
//! let compiled = compile(&doc, &resolver).unwrap();
//! // compiled.theories contains the compiled Theory objects
//! ```

pub mod compile;
pub mod compile_class;
pub mod compile_compose;
pub mod compile_inductive;
pub mod compile_instance;
pub mod compile_morphism;
pub mod compile_protocol;
pub mod compile_theory;
pub mod document;
pub mod error;
pub mod eval;

use std::path::Path;

pub use compile::{builtin_resolver, compile_bundle, compile_with_source};
pub use compile_theory::{compile_theory, compile_theory_with_law_check};
pub use document::{BundleSpec, CompiledTheorySet, TheoryBody, TheoryDocument, TheorySpec};
pub use error::TheoryDslError;

/// Load a theory document from a file.
///
/// Dispatches to the appropriate evaluator based on file extension:
/// - `.ncl` → Nickel evaluation
/// - `.json` → JSON deserialization
/// - `.yaml`, `.yml` → YAML deserialization
///
/// # Errors
///
/// Returns [`TheoryDslError::UnsupportedExtension`] for unknown extensions,
/// [`TheoryDslError::Io`] for read errors, or evaluation-specific errors.
pub fn load(path: &Path) -> Result<TheoryDocument, TheoryDslError> {
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
        _ => Err(TheoryDslError::UnsupportedExtension {
            ext: ext.to_owned(),
        }),
    }
}

/// Result of loading a directory of theory documents.
pub struct LoadDirResult {
    /// Successfully loaded documents.
    pub documents: Vec<TheoryDocument>,
    /// Files that failed to load, with their paths and errors.
    pub errors: Vec<(std::path::PathBuf, TheoryDslError)>,
}

/// Load all theory documents from a directory.
///
/// Scans for `.ncl`, `.json`, `.yaml`, and `.yml` files.
/// Files that fail to parse are reported in `errors`; successfully
/// parsed documents are returned in `documents`.
///
/// # Errors
///
/// Returns [`TheoryDslError::Io`] if the directory itself cannot be read.
/// Per-file errors are returned in [`LoadDirResult::errors`].
pub fn load_dir(dir: &Path) -> Result<LoadDirResult, TheoryDslError> {
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

/// Compile a [`TheoryDocument`] to a [`CompiledTheorySet`].
///
/// Convenience re-export of [`compile::compile`].
///
/// # Errors
///
/// See [`compile::compile`] for error conditions.
pub fn compile(
    doc: &TheoryDocument,
    resolver: &dyn Fn(&str) -> Option<panproto_gat::Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    compile::compile(doc, resolver)
}

/// Load a theory file and compile it in one step.
///
/// Uses the [`builtin_resolver`] for external theory lookup.
///
/// # Errors
///
/// Combines errors from [`load`] and [`compile()`].
pub fn load_and_compile(
    path: &Path,
    resolver: &dyn Fn(&str) -> Option<panproto_gat::Theory>,
) -> Result<CompiledTheorySet, TheoryDslError> {
    let doc = load(path)?;
    compile::compile(&doc, resolver)
}
