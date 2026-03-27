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

/// Look up a built-in protocol by name.
///
/// Returns ``None`` if the name is not recognized.
fn lookup(name: &str) -> Option<Protocol> {
    Some(match name {
        // annotation (19)
        "brat" => protocols::annotation::brat::protocol(),
        "conllu" => protocols::annotation::conllu::protocol(),
        "naf" => protocols::annotation::naf::protocol(),
        "uima" => protocols::annotation::uima::protocol(),
        "folia" => protocols::annotation::folia::protocol(),
        "tei" => protocols::annotation::tei::protocol(),
        "timeml" => protocols::annotation::timeml::protocol(),
        "elan" => protocols::annotation::elan::protocol(),
        "iso_space" => protocols::annotation::iso_space::protocol(),
        "paula" => protocols::annotation::paula::protocol(),
        "laf_graf" => protocols::annotation::laf_graf::protocol(),
        "decomp" => protocols::annotation::decomp::protocol(),
        "ucca" => protocols::annotation::ucca::protocol(),
        "fovea" => protocols::annotation::fovea::protocol(),
        "bead" => protocols::annotation::bead::protocol(),
        "web_annotation" => protocols::annotation::web_annotation::protocol(),
        "amr" => protocols::annotation::amr::protocol(),
        "concrete" => protocols::annotation::concrete::protocol(),
        "nif" => protocols::annotation::nif::protocol(),
        // api (4)
        "openapi" => protocols::api::openapi::protocol(),
        "asyncapi" => protocols::api::asyncapi::protocol(),
        "jsonapi" => protocols::api::jsonapi::protocol(),
        "raml" => protocols::api::raml::protocol(),
        // config (3)
        "cloudformation" => protocols::config::cloudformation::protocol(),
        "ansible" => protocols::config::ansible::protocol(),
        "k8s_crd" => protocols::config::k8s_crd::protocol(),
        // data_schema (2)
        "cddl" => protocols::data_schema::cddl::protocol(),
        "bson" => protocols::data_schema::bson::protocol(),
        // data_science (3)
        "dataframe" => protocols::data_science::dataframe::protocol(),
        "parquet" => protocols::data_science::parquet::protocol(),
        "arrow" => protocols::data_science::arrow::protocol(),
        // database (5)
        "mongodb" => protocols::database::mongodb::protocol(),
        "dynamodb" => protocols::database::dynamodb::protocol(),
        "cassandra" => protocols::database::cassandra::protocol(),
        "neo4j" => protocols::database::neo4j::protocol(),
        "redis" => protocols::database::redis::protocol(),
        // domain (6)
        "geojson" => protocols::domain::geojson::protocol(),
        "fhir" => protocols::domain::fhir::protocol(),
        "rss_atom" => protocols::domain::rss_atom::protocol(),
        "vcard_ical" => protocols::domain::vcard_ical::protocol(),
        "swift_mt" => protocols::domain::swift_mt::protocol(),
        "edi_x12" => protocols::domain::edi_x12::protocol(),
        // serialization (5)
        "avro" => protocols::serialization::avro::protocol(),
        "flatbuffers" => protocols::serialization::flatbuffers::protocol(),
        "asn1" => protocols::serialization::asn1::protocol(),
        "bond" => protocols::serialization::bond::protocol(),
        "msgpack_schema" => protocols::serialization::msgpack_schema::protocol(),
        // web_document (3)
        "atproto" => protocols::web_document::atproto::protocol(),
        "docx" => protocols::web_document::docx::protocol(),
        "odf" => protocols::web_document::odf::protocol(),
        _ => return None,
    })
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
    // config
    "cloudformation",
    "ansible",
    "k8s_crd",
    // data_schema
    "cddl",
    "bson",
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
    BUILTIN_NAMES.iter().map(|s| (*s).to_owned()).collect()
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
