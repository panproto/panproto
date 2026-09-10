//! Companion grammar package: functional languages.
//!
//! This crate is a Python extension module (`panproto_grammars_functional._impl`)
//! built as its own pyo3 cdylib. It depends on `panproto-grammars` with
//! the `group-functional` feature flag, which bakes the Haskell, OCaml,
//! Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure, Scheme, and
//! Racket tree-sitter grammars into this cdylib's static memory.
//!
//! On import the module exposes a single function, `grammars_metadata`,
//! that returns a list of dicts the core `panproto._native.AstParserRegistry`
//! constructor accepts. The pointer-as-integer encoding lets two
//! independent pyo3 cdylibs (`panproto._native` and this one) share
//! tree-sitter `Language` objects without depending on each other's
//! Rust types.
//!
//! ## Architecture
//!
//! Each `panproto-grammars-*` companion crate follows the same shape:
//!
//! 1. Depend on `panproto-grammars` with one `group-*` feature.
//! 2. Expose `grammars_metadata()` returning the list of dicts.
//! 3. Declare an entry point in `pyproject.toml` under
//!    `panproto.grammars` so the core wheel's `AstParserRegistry` can
//!    discover the companion at construction time.
//!
//! Adding a new group is a copy-paste of this crate with a different
//! feature flag, plus an entry-point declaration.

#![allow(unsafe_code)]

use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyList};

/// Return the metadata dicts for every grammar baked into this companion.
///
/// Each dict has the keys consumed by the core `AstParserRegistry`
/// constructor: `name`, `extensions`, `language_ptr`, `node_types_ptr`,
/// `node_types_len`, plus optional `tags_query_*` and `grammar_json_*`
/// pairs. The pointer values are raw FFI pointers into this crate's
/// static `.rodata` and remain valid for the process lifetime.
#[pyfunction]
fn grammars_metadata(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let entries = PyList::empty(py);
    for grammar in panproto_grammars::grammars() {
        let entry = PyDict::new(py);
        entry.set_item("name", grammar.name)?;
        entry.set_item("extensions", grammar.extensions.to_vec())?;

        // tree_sitter::Language is `#[repr(transparent)]` over a raw
        // pointer; transmute the value to its integer representation
        // for transport across the cdylib boundary. The pointer points
        // into the linked grammar's rodata, which is static for the
        // process.
        let language_ptr: usize =
            unsafe { std::mem::transmute::<tree_sitter::Language, usize>(grammar.language) };
        // Both forms. The capsule is what panproto prefers: it carries a
        // name panproto checks, and it keeps this module alive while it
        // lives, so the pointer cannot outlive the library that owns
        // it. The bare address stays because a panproto older than this
        // one reads only that, and a companion has to work with both.
        entry.set_item("language_ptr", language_ptr)?;
        // SAFETY: `language_ptr` is the `TSLanguage *` that this
        // crate's own compiled-in `tree_sitter_<name>()` returned, and
        // it points into this cdylib's static memory, so it is valid
        // for as long as this module is loaded. The capsule keeps a
        // reference to that module, which is what makes it a stronger
        // claim than the bare address beside it. No destructor: the
        // pointee is static and must not be freed.
        let Some(nonnull) = std::ptr::NonNull::new(language_ptr as *mut std::ffi::c_void) else {
            // A null here would mean this crate's own grammar table is
            // broken; skip the grammar rather than advertise a capsule
            // wrapping nothing.
            continue;
        };
        let capsule =
            unsafe { PyCapsule::new_with_pointer(py, nonnull, c"panproto.tree_sitter_language")? };
        entry.set_item("language_capsule", capsule)?;

        let node_types = grammar.node_types;
        entry.set_item("node_types_ptr", node_types.as_ptr() as usize)?;
        entry.set_item("node_types_len", node_types.len())?;

        if let Some(tq) = grammar.tags_query {
            let bytes = tq.as_bytes();
            entry.set_item("tags_query_ptr", bytes.as_ptr() as usize)?;
            entry.set_item("tags_query_len", bytes.len())?;
        } else {
            entry.set_item("tags_query_ptr", py.None())?;
            entry.set_item("tags_query_len", 0_usize)?;
        }

        if let Some(gj) = grammar.grammar_json {
            entry.set_item("grammar_json_ptr", gj.as_ptr() as usize)?;
            entry.set_item("grammar_json_len", gj.len())?;
        } else {
            entry.set_item("grammar_json_ptr", py.None())?;
            entry.set_item("grammar_json_len", 0_usize)?;
        }

        entries.append(entry)?;
    }
    Ok(entries)
}

/// pyo3 entry point. The Python submodule name is `_impl` (matching
/// `module-name = "panproto_grammars_functional._impl"` in the
/// companion's pyproject.toml); `pymodule(name = ...)` keeps the
/// emitted `PyInit_<>` symbol matching the leaf module name that
/// `CPython`'s loader will dlsym on import. The pyo3 macro requires a
/// distinct Rust function name so the symbol is globally unique
/// across companions; the explicit `name` argument decouples that
/// from the Python-visible name.
#[pymodule(name = "_impl")]
fn panproto_grammars_functional_impl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(grammars_metadata, m)?)?;
    Ok(())
}
