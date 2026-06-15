//! Python bindings for schema-document parsing and the schema-to-theory
//! bridge.
//!
//! Two capabilities that already exist in Rust but were unreachable from
//! Python:
//!
//! 1. Parsing a schema-defining document (an `ATProto` lexicon today) into a
//!    [`Schema`], wrapping
//!    [`panproto_core::protocols::web_document::atproto::parse_lexicon`].
//! 2. Extracting the GAT theory a schema instantiates, wrapping
//!    [`panproto_core::vcs::gat_validate::schema_to_theory`].
//!
//! The induced theory has one sort per vertex and one unary operation per
//! edge. Vertices whose kind names a primitive value kind (``"string"``,
//! ``"integer"``, ``"boolean"``, ...) are tagged with the matching
//! [`SortKind::Val`] so the value-level distinction survives into the
//! theory; everything else stays a structural sort. Refined value types
//! (datetime, decimal, uuid), per-field defaults and metadata, and
//! reference-versus-containment edge distinctions are not part of the GAT
//! theory vocabulary; those live on the [`Schema`] (constraints and
//! [`Edge::kind`](panproto_core::schema::Edge)) and are read from there.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyString;

use panproto_core::gat::{SortKind, Theory, ValueKind};
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::Schema;

use crate::error::SchemaValidationError;
use crate::gat::PyTheory;
use crate::schema::PySchema;

/// Coerce a Python value into a [`serde_json::Value`].
///
/// Accepts either a JSON string or any dict / list / scalar that matches
/// the JSON data model (parsed via `pythonize`). A string that is not
/// valid JSON raises ``ValueError``.
fn json_from_py(doc: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if let Ok(text) = doc.downcast::<PyString>() {
        let raw = text.to_str()?;
        return serde_json::from_str(raw).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("document is not valid JSON: {e}"))
        });
    }
    crate::convert::from_python(doc)
}

/// Parse an `ATProto` lexicon value into a [`PySchema`].
fn parse_atproto_value(value: &serde_json::Value) -> PyResult<PySchema> {
    let schema = atproto::parse_lexicon(value).map_err(|e| {
        SchemaValidationError::new_err(format!("ATProto lexicon parse failed: {e}"))
    })?;
    Ok(PySchema {
        inner: Arc::new(schema),
    })
}

/// Map a vertex kind name to a primitive [`ValueKind`], if it names one.
///
/// The canonical names come from [`ValueKind::as_str`]; a few common
/// synonyms are accepted on top so encoders that emit ``"float"`` or
/// ``"int"`` line up with ``"number"`` and ``"integer"``. Kinds that do
/// not name a value kind (``"object"``, ``"record"``, ``"ref"``, ...)
/// return ``None`` and stay structural.
fn value_kind_for(kind: &str) -> Option<ValueKind> {
    if let Some(vk) = ValueKind::all()
        .iter()
        .copied()
        .find(|vk| vk.as_str() == kind)
    {
        return Some(vk);
    }
    match kind {
        "float" | "double" => Some(ValueKind::Float),
        "int" | "int64" => Some(ValueKind::Int),
        "bool" => Some(ValueKind::Bool),
        "str" => Some(ValueKind::Str),
        _ => None,
    }
}

/// Build the GAT theory induced by `schema`, preserving value kinds.
///
/// Reuses [`panproto_core::vcs::gat_validate::schema_to_theory`] for the
/// structural skeleton (deterministic sort and operation names) and then
/// refines each sort that corresponds to a value-kind vertex.
fn induced_theory(name: &str, schema: &Schema) -> Theory {
    let mut theory = panproto_core::vcs::gat_validate::schema_to_theory(name, schema);
    for sort in &mut theory.sorts {
        if let Some(vk) = schema
            .vertex(sort.name.as_ref())
            .and_then(|vertex| value_kind_for(vertex.kind.as_ref()))
        {
            sort.kind = SortKind::Val(vk);
        }
    }
    theory
}

/// Shared backing for [`theory_of`] and the `Schema.theory` method.
pub fn schema_theory(schema: &PySchema, name: Option<&str>) -> PyTheory {
    let theory_name = name.unwrap_or_else(|| schema.inner.protocol.as_str());
    PyTheory {
        inner: Arc::new(induced_theory(theory_name, &schema.inner)),
    }
}

/// Parse an `ATProto` lexicon document into a :class:`Schema`.
///
/// Wraps the Rust ``web_document::atproto::parse_lexicon``: it walks the
/// lexicon ``defs`` and builds a schema under the builtin ``atproto``
/// protocol, with vertices for each type definition and edges for
/// properties, array items, union variants, and references.
///
/// Parameters
/// ----------
/// doc : Mapping or str
///     The lexicon document, either as a parsed dict (the ``lexicon`` /
///     ``id`` / ``defs`` JSON shape) or as a raw JSON string.
///
/// Returns
/// -------
/// Schema
///     The parsed schema. It validates against
///     ``get_builtin_protocol("atproto")``.
///
/// Raises
/// ------
/// `ValueError`
///     If ``doc`` is a string that is not valid JSON.
/// `SchemaValidationError`
///     If the document is not a well-formed lexicon.
#[pyfunction]
pub fn parse_atproto_lexicon(doc: &Bound<'_, PyAny>) -> PyResult<PySchema> {
    let value = json_from_py(doc)?;
    parse_atproto_value(&value)
}

/// Parse a schema-defining document under the named protocol.
///
/// A protocol-dispatching entry point over the document parsers. Today
/// the only registered parser is ``"atproto"`` (which delegates to
/// :func:`parse_atproto_lexicon`); other document-schema protocols can be
/// added here as their Rust parsers are exposed.
///
/// Parameters
/// ----------
/// protocol : str
///     Protocol name selecting the document parser (e.g. ``"atproto"``).
/// doc : Mapping or str
///     The schema document, as a parsed dict or a raw JSON string.
///
/// Returns
/// -------
/// Schema
///     The parsed schema.
///
/// Raises
/// ------
/// `ValueError`
///     If ``protocol`` has no registered document parser, or if ``doc``
///     is a string that is not valid JSON.
/// `SchemaValidationError`
///     If the document is not well-formed for the protocol.
#[pyfunction]
pub fn parse_schema_document(protocol: &str, doc: &Bound<'_, PyAny>) -> PyResult<PySchema> {
    let value = json_from_py(doc)?;
    match protocol {
        "atproto" => parse_atproto_value(&value),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "no document parser registered for protocol {other:?}; supported: [\"atproto\"]"
        ))),
    }
}

/// Extract the GAT theory a schema instantiates.
///
/// The induced theory has one sort per vertex and one unary operation per
/// edge (``src -> tgt``). Vertices whose kind names a primitive value kind
/// carry that kind on the sort, so a ``"string"`` field and an
/// ``"integer"`` field stay distinct in the theory. Refined value types
/// (datetime, decimal, uuid), per-field defaults and descriptions, and the
/// reference-versus-containment edge distinction are not expressible in
/// the GAT theory vocabulary; recover them from the schema's constraints
/// and ``Edge.kind`` instead.
///
/// Parameters
/// ----------
/// schema : Schema
///     The schema to extract a theory from.
/// name : str, optional
///     Name for the resulting theory. Defaults to the schema's protocol
///     name.
///
/// Returns
/// -------
/// Theory
///     The induced theory.
#[pyfunction]
#[pyo3(signature = (schema, name=None))]
pub fn theory_of(schema: &PySchema, name: Option<&str>) -> PyTheory {
    schema_theory(schema, name)
}

/// Register lexicon / schema-document functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction!(parse_atproto_lexicon, parent)?)?;
    parent.add_function(wrap_pyfunction!(parse_schema_document, parent)?)?;
    parent.add_function(wrap_pyfunction!(theory_of, parent)?)?;
    Ok(())
}
