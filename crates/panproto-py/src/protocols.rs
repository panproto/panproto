//! Python bindings for the built-in protocol registry.
//!
//! Provides access to the remaining semantic protocol definitions across
//! 9 categories (annotation, api, config, `data_schema`, `data_science`,
//! database, domain, serialization, `web_document`).
//!
//! Programming language and data format protocols are handled by tree-sitter
//! grammars via `panproto-grammars`.

use panproto_core::protocols;
use panproto_core::schema::Protocol;
use pyo3::prelude::*;

use crate::schema::PyProtocol;

fn grammar_protocol(name: &str) -> Option<Protocol> {
    if !panproto_grammars::has_grammar(name) {
        return None;
    }
    Some(Protocol {
        name: name.to_owned(),
        schema_theory: name.to_owned(),
        instance_theory: name.to_owned(),
        ..Default::default()
    })
}

/// Look up a built-in protocol by name.
///
/// Resolves through `panproto_core::protocols::registry`, so every spelling
/// the registry accepts reaches the protocol: the canonical name
/// (`rss-atom`), its underscore form (`rss_atom`), and each alias.
/// Returns ``None`` if the name is not recognized.
fn lookup(name: &str) -> Option<Protocol> {
    protocols::registry::descriptor(name).map(|descriptor| (descriptor.protocol)())
}

/// All built-in protocol names.
const BUILTIN_NAMES: &[&str] = &[
    // annotation
    "brat",
    "conllu",
    "naf",
    "uima",
    "folia",
    "tei",
    "timeml",
    "elan",
    "iso_space",
    "paula",
    "laf_graf",
    "decomp",
    "ucca",
    "fovea",
    "bead",
    "web_annotation",
    "amr",
    "concrete",
    "nif",
    // api
    "openapi",
    "asyncapi",
    "jsonapi",
    "raml",
    "graphql",
    // config
    "cloudformation",
    "ansible",
    "k8s_crd",
    // data_schema
    "cddl",
    "bson",
    "json-schema",
    // data_science
    "dataframe",
    "parquet",
    "arrow",
    // database
    "mongodb",
    "dynamodb",
    "cassandra",
    "neo4j",
    "redis",
    "sql",
    // domain
    "geojson",
    "fhir",
    "rss_atom",
    "vcard_ical",
    "swift_mt",
    "edi_x12",
    // serialization
    "avro",
    "flatbuffers",
    "asn1",
    "bond",
    "msgpack_schema",
    "protobuf",
    // web_document
    "atproto",
    "docx",
    "odf",
];

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// List all built-in protocol names.
#[pyfunction]
pub fn list_builtin_protocols() -> Vec<String> {
    let mut names: Vec<String> = BUILTIN_NAMES.iter().map(|s| (*s).to_owned()).collect();
    for g in panproto_grammars::grammars() {
        if !names.iter().any(|n| n == g.name) {
            names.push(g.name.to_owned());
        }
    }
    names
}

/// Get a built-in protocol by name.
///
/// Parameters
/// ----------
/// name : str
///     Protocol name (e.g., ``"atproto"``, ``"brat"``).
///
/// Returns
/// -------
/// Protocol
///     The protocol specification.
///
/// Raises
/// ------
/// `KeyError`
///     If the protocol name is not recognized.
#[pyfunction]
pub fn get_builtin_protocol(name: &str) -> PyResult<PyProtocol> {
    lookup(name)
        .or_else(|| grammar_protocol(name))
        .map(|p| PyProtocol { inner: p })
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("unknown protocol: {name}")))
}

/// Define a custom protocol from a dict specification.
///
/// Parameters
/// ----------
/// spec : dict
///     Protocol specification with keys: ``name``, ``schema_theory``,
///     ``instance_theory``, ``edge_rules``, ``obj_kinds``,
///     ``constraint_sorts``.
///
/// Returns
/// -------
/// Protocol
///     The custom protocol.
#[pyfunction]
pub fn define_protocol(spec: &Bound<'_, pyo3::types::PyAny>) -> PyResult<PyProtocol> {
    let protocol: Protocol = crate::convert::from_python(spec)?;
    Ok(PyProtocol { inner: protocol })
}

/// Register protocol functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_function(wrap_pyfunction!(list_builtin_protocols, parent)?)?;
    parent.add_function(wrap_pyfunction!(get_builtin_protocol, parent)?)?;
    parent.add_function(wrap_pyfunction!(define_protocol, parent)?)?;
    Ok(())
}
