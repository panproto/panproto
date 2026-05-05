//! Companion grammar package: jvm languages.
//!
//! pyo3 cdylib that bakes the `panproto-grammars` `group-jvm`
//! feature (Java, Kotlin, Scala, Groovy, Clojure)
//! into static memory and exposes the metadata `panproto`'s
//! [`AstParserRegistry`] consumes through the
//! `panproto.grammars` entry point. See the architecture notes in
//! the sibling `panproto-grammars-functional` crate.

#![allow(unsafe_code, clippy::doc_markdown)]

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

/// Return the metadata dicts for every grammar baked into this
/// companion. Each dict is consumed by the core
/// `panproto._native.AstParserRegistry` constructor.
#[pyfunction]
fn grammars_metadata(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let entries = PyList::empty(py);
    for grammar in panproto_grammars::grammars() {
        let entry = PyDict::new(py);
        entry.set_item("name", grammar.name)?;
        entry.set_item("extensions", grammar.extensions.to_vec())?;

        let language_ptr: usize =
            unsafe { std::mem::transmute::<tree_sitter::Language, usize>(grammar.language) };
        entry.set_item("language_ptr", language_ptr)?;

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

/// pyo3 entry point. Submodule name `_impl` matches the companion
/// `pyproject.toml`'s `module-name`. The Rust function name is
/// globally unique to keep the emitted `PyInit_<>` symbol free of
/// collisions across companions in the same process.
#[pymodule(name = "_impl")]
fn panproto_grammars_jvm_impl(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(grammars_metadata, m)?)?;
    Ok(())
}
