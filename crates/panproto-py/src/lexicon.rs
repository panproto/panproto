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
use panproto_core::io::schema_documents::{self, BundleSource, ReadOptions, SchemaReadError};
use panproto_core::protocols::config::ansible::FileClass;
use panproto_core::protocols::registry::{self, Parser};
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
    if let Ok(text) = doc.cast::<PyString>() {
        let raw = text.to_str()?;
        return serde_json::from_str(raw).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("document is not valid JSON: {e}"))
        });
    }
    crate::convert::from_python(doc)
}

/// The reader options a call names: the class of an Ansible file.
fn read_options(ansible_class: Option<&str>) -> PyResult<ReadOptions> {
    let file_class = ansible_class
        .map(|name| {
            FileClass::from_name(name).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown Ansible file class {name:?}; expected one of {:?}",
                    FileClass::ALL.map(FileClass::name)
                ))
            })
        })
        .transpose()?;
    Ok(ReadOptions {
        file_class,
        ..ReadOptions::default()
    })
}

/// The message of a schema-document refusal, naming the feature of this
/// extension that provides a reader the build does not include.
fn read_error_message(error: &SchemaReadError) -> String {
    match error {
        SchemaReadError::GrammarNotBuilt { .. } => format!(
            "{error}; this panproto build lacks the panproto-py feature \
             format-preserving, which the published wheel enables"
        ),
        _ => error.to_string(),
    }
}

/// Whether `protocol` is read through a tree-sitter grammar.
fn read_through_grammar(protocol: &str) -> bool {
    registry::descriptor(protocol).is_some_and(|d| matches!(d.parser, Parser::Grammar { .. }))
}

/// The JSON text of a document given as a string or a Python value.
///
/// A string is taken as written, so a reader sees its key order and any
/// repeated key; any other value is written by Python's `json` module,
/// which keeps a mapping's insertion order.
fn document_text(doc: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(text) = doc.cast::<PyString>() {
        return Ok(text.to_str()?.to_owned());
    }
    doc.py()
        .import("json")?
        .call_method1("dumps", (doc,))?
        .extract()
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

/// Parse a schema-defining JSON *document* under the named protocol.
///
/// A protocol-dispatching entry point over every JSON-document schema
/// reader, forwarding the protocol string to the reader layer so no
/// protocol name is hard-coded here. The protocols read through a
/// tree-sitter grammar whose documents are JSON (``openapi``,
/// ``asyncapi``, ``json-schema``, ``k8s-crd``, ``cloudformation``,
/// ``ansible``) are read here too. Protocols whose source is text rather
/// than a JSON document are parsed with :func:`parse_schema_source`.
///
/// Parameters
/// ----------
/// protocol : str
///     Protocol name selecting the document reader. Both the canonical
///     hyphenated name and its underscore compatibility spelling resolve.
/// doc : Mapping or str
///     The schema document, as a parsed dict or a raw JSON string. A
///     grammar protocol reads a string as written and a dict as Python's
///     ``json.dumps`` writes it.
/// ansible_class : str, optional
///     The class of an Ansible file, ``"playbook"``, ``"tasks"`` or
///     ``"vars"``, which the ``ansible`` reader needs and the file does
///     not state.
///
/// Returns
/// -------
/// Schema
///     The parsed schema.
///
/// Raises
/// ------
/// `ValueError`
///     If ``doc`` is a string that is not valid JSON, if no document
///     reader is registered for ``protocol``, if the reader refuses the
///     document, or if ``ansible_class`` is missing for ``ansible`` or
///     names no class.
#[pyfunction]
#[pyo3(signature = (protocol, doc, ansible_class = None))]
pub fn parse_schema_document(
    protocol: &str,
    doc: &Bound<'_, PyAny>,
    ansible_class: Option<&str>,
) -> PyResult<PySchema> {
    let options = read_options(ansible_class)?;
    let schema = if read_through_grammar(protocol) {
        schema_documents::parse_schema_document_text(protocol, &document_text(doc)?, &options)
    } else {
        schema_documents::parse_schema_document(protocol, &json_from_py(doc)?, &options)
    }
    .map_err(|e| pyo3::exceptions::PyValueError::new_err(read_error_message(&e)))?;
    Ok(PySchema {
        inner: Arc::new(schema),
    })
}

/// Parse a schema-defining *text/source* (an IDL or DDL string) under
/// the named protocol.
///
/// The text counterpart to :func:`parse_schema_document`, for protocols
/// whose source is a language rather than a JSON document, and for the
/// protocols read through a tree-sitter grammar, which read their text as
/// written: an ``openapi``, ``asyncapi``, ``k8s-crd``, ``cloudformation``
/// or ``ansible`` document in YAML or JSON, a ``json-schema`` document,
/// or a ``postgresql`` script. Dispatch lives in the reader layer, so no
/// protocol name is hard-coded here.
///
/// Parameters
/// ----------
/// protocol : str
///     Protocol name selecting the source reader.
/// source : str
///     The schema source text.
/// ansible_class : str, optional
///     The class of an Ansible file, ``"playbook"``, ``"tasks"`` or
///     ``"vars"``, which the ``ansible`` reader needs and the file does
///     not state.
///
/// Returns
/// -------
/// Schema
///     The parsed schema.
///
/// Raises
/// ------
/// `ValueError`
///     If no source reader is registered for ``protocol``, if the reader
///     refuses the source, or if ``ansible_class`` is missing for
///     ``ansible`` or names no class.
#[pyfunction]
#[pyo3(signature = (protocol, source, ansible_class = None))]
pub fn parse_schema_source(
    protocol: &str,
    source: &str,
    ansible_class: Option<&str>,
) -> PyResult<PySchema> {
    let schema =
        schema_documents::parse_schema_source(protocol, source, &read_options(ansible_class)?)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(read_error_message(&e)))?;
    Ok(PySchema {
        inner: Arc::new(schema),
    })
}

/// Parse a bundle of schema documents into one schema, resolving
/// cross-document references across the whole bundle.
///
/// :func:`parse_schema_document` sees one document at a time, so a
/// reference into another document resolves to an opaque placeholder
/// carrying no fields. Passing the referenced documents alongside the
/// referring one resolves each such reference to the definition's real,
/// typed vertex, so a lens can bind to the cross-document structure. A
/// reference whose target is in no document of the bundle stays a
/// placeholder, which is what marks it as genuinely external.
///
/// The ``openapi``, ``asyncapi`` and ``json-schema`` bundles are read by
/// their tree-sitter grammar readers, the first document the entry. A
/// reference between their documents resolves against a document's
/// retrieval URI, or against the ``$id`` it declares.
///
/// Parameters
/// ----------
/// protocol : str
///     Protocol name selecting the bundle reader.
/// docs : Sequence of Mapping or str
///     The schema documents, each a parsed dict or a raw JSON string; a
///     grammar protocol also reads a YAML string.
/// retrieval_uris : Sequence of str or None, optional
///     The absolute URI each document was retrieved from, in the order
///     of ``docs``, for a grammar protocol. Only a grammar protocol's
///     reader takes them.
///
/// Returns
/// -------
/// Schema
///     One schema covering every document in the bundle.
///
/// Raises
/// ------
/// `ValueError`
///     If ``protocol`` has no registered bundle parser, or if a document
///     is a string that is not valid JSON.
/// `SchemaValidationError`
///     If the documents are not a well-formed bundle for the protocol.
#[pyfunction]
#[pyo3(signature = (protocol, docs, retrieval_uris = None))]
pub fn parse_schema_bundle(
    protocol: &str,
    docs: &Bound<'_, PyAny>,
    retrieval_uris: Option<Vec<Option<String>>>,
) -> PyResult<PySchema> {
    let schema = if read_through_grammar(protocol) {
        let texts = docs
            .try_iter()?
            .map(|doc| document_text(&doc?))
            .collect::<PyResult<Vec<String>>>()?;
        let uris = retrieval_uris.unwrap_or_else(|| vec![None; texts.len()]);
        if uris.len() != texts.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "{} retrieval URIs for {} documents",
                uris.len(),
                texts.len()
            )));
        }
        let sources: Vec<BundleSource<'_>> = texts
            .iter()
            .zip(&uris)
            .map(|(source, uri)| BundleSource {
                retrieval_uri: uri.as_deref(),
                source,
            })
            .collect();
        schema_documents::parse_schema_bundle(protocol, &sources)
            .map_err(|e| read_error_message(&e))
    } else {
        if retrieval_uris.is_some() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "protocol {protocol:?} reads its bundles without retrieval URIs"
            )));
        }
        let mut values = Vec::new();
        for doc in docs.try_iter()? {
            values.push(json_from_py(&doc?)?);
        }
        panproto_core::protocols::parse_schema_bundle(protocol, &values).map_err(|e| e.to_string())
    }
    .map_err(|e| SchemaValidationError::new_err(format!("bundle parse failed: {e}")))?;

    Ok(PySchema {
        inner: Arc::new(schema),
    })
}

/// The protocols :func:`parse_schema_bundle` accepts.
///
/// A bundle parser is what resolves a reference from one document into
/// a sibling, so this is a smaller set than the built-in protocol list:
/// a protocol absent from it parses one document at a time, and a
/// cross-document reference in such a protocol stays an opaque
/// placeholder. Ask rather than hard-coding the list, which has grown
/// and will grow again.
///
/// Returns
/// -------
/// list of str
///     Canonical protocol names, in the order the registry lists them.
#[pyfunction]
#[must_use]
pub fn list_bundle_parser_protocols() -> Vec<String> {
    panproto_core::protocols::bundle_parser_protocols()
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// Per-file lexicon schemas plus cross-file ref edges: the per-file
/// provenance form of a lexicon set, retained for the version-control
/// layer (where :func:`parse_schema_bundle` fuses the same documents
/// into one flat schema with no per-file identity).
#[pyclass(name = "LexiconProject", module = "panproto._native")]
pub struct PyLexiconProject {
    pub(crate) protocol: String,
    pub(crate) files: Vec<(String, Arc<panproto_core::schema::Schema>)>,
    pub(crate) cross: Vec<(String, Vec<panproto_core::schema::Edge>)>,
}

#[pymethods]
impl PyLexiconProject {
    /// The per-file schemas as ``(path, Schema)`` pairs, in input order.
    fn files(&self) -> Vec<(String, PySchema)> {
        self.files
            .iter()
            .map(|(p, s)| (p.clone(), PySchema { inner: s.clone() }))
            .collect()
    }

    /// The document paths, in input order.
    fn file_paths(&self) -> Vec<String> {
        self.files.iter().map(|(p, _)| p.clone()).collect()
    }

    /// Cross-file ref edges as ``{path: [{"src", "tgt", "kind", "name"}]}``.
    /// Both endpoints are already prefixed with their owning file's path.
    fn cross_file_edges(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let map: serde_json::Map<String, serde_json::Value> = self
            .cross
            .iter()
            .map(|(path, edges)| {
                let list: Vec<serde_json::Value> = edges
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "src": e.src.as_ref(),
                            "tgt": e.tgt.as_ref(),
                            "kind": e.kind.as_ref(),
                            "name": e.name.as_deref(),
                        })
                    })
                    .collect();
                (path.clone(), serde_json::Value::Array(list))
            })
            .collect();
        crate::convert::to_python(py, &serde_json::Value::Object(map))
    }

    fn __repr__(&self) -> String {
        format!("LexiconProject({} files)", self.files.len())
    }
}

/// Parse a set of lexicon documents into per-file schemas with per-file
/// provenance, resolving cross-document refs.
///
/// Where :func:`parse_schema_bundle` fuses a lexicon set into one flat
/// schema, this keeps each document a separate schema plus the ref edges
/// that cross document boundaries (endpoints already prefixed with their
/// owning file's path), so a lexicon set can be stored and diffed as the
/// per-file tree the version-control layer is built around.
///
/// Parameters
/// ----------
/// protocol : str
///     The protocol the documents are written in (currently ``atproto``).
/// docs : list of (str, dict | str)
///     ``(path, document)`` pairs. ``path`` is the project-relative file
///     path; ``document`` is a parsed dict or a raw JSON string.
///
/// Returns
/// -------
/// LexiconProject
///     Per-file schemas (:meth:`LexiconProject.files`) and cross-file
///     edges (:meth:`LexiconProject.cross_file_edges`).
///
/// Raises
/// ------
/// `ValueError`
///     If ``protocol`` has no per-file bundle parser, or a document is a
///     string that is not valid JSON.
#[pyfunction]
pub fn parse_schema_bundle_project(
    protocol: &str,
    docs: &Bound<'_, PyAny>,
) -> PyResult<PyLexiconProject> {
    let mut pairs: Vec<(std::path::PathBuf, serde_json::Value)> = Vec::new();
    for item in docs.try_iter()? {
        let item = item?;
        let path: String = item.get_item(0)?.extract()?;
        let value = json_from_py(&item.get_item(1)?)?;
        pairs.push((std::path::PathBuf::from(path), value));
    }

    let project = panproto_core::protocols::parse_schema_bundle_project(protocol, &pairs)
        .map_err(|e| SchemaValidationError::new_err(format!("bundle project parse failed: {e}")))?;

    let files = project
        .files
        .into_iter()
        .map(|(p, s)| (p.display().to_string(), Arc::new(s)))
        .collect();
    let cross = project
        .cross_file_edges
        .into_iter()
        .map(|(p, edges)| (p.display().to_string(), edges))
        .collect();

    Ok(PyLexiconProject {
        protocol: protocol.replace('_', "-"),
        files,
        cross,
    })
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
    parent.add_function(wrap_pyfunction!(parse_schema_source, parent)?)?;
    parent.add_function(wrap_pyfunction!(parse_schema_bundle, parent)?)?;
    parent.add_function(wrap_pyfunction!(parse_schema_bundle_project, parent)?)?;
    parent.add_function(wrap_pyfunction!(list_bundle_parser_protocols, parent)?)?;
    parent.add_function(wrap_pyfunction!(theory_of, parent)?)?;
    parent.add_class::<PyLexiconProject>()?;
    Ok(())
}
