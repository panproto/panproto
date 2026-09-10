//! The one place a protocol's capabilities are written down.
//!
//! Protocol capabilities used to be described independently in several
//! places: the parser dispatches in this crate, protocol and theory
//! resolution in the CLI, hand-written lists and theory matches in the C
//! API, parallel hand-written lists in the WASM API, and tables in the
//! book. Those registries had already diverged. The C list advertised
//! nine protocols under underscore spellings that are not their
//! canonical names, omitted the `uima-cas` alias, and the dispatch
//! accepted `uima` while no published list mentioned it.
//!
//! A descriptor is the single record of what a protocol can do, and the
//! listings and the dispatch both derive from it, so a protocol added
//! here is reachable and advertised everywhere at once and the two
//! cannot disagree.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{Protocol, Schema};

use crate::error::ProtocolError;
use crate::{
    annotation, api, config, data_schema, data_science, database, domain, serialization,
    web_document,
};

/// How a protocol's schema documents arrive, and the function that
/// reads one.
///
/// The two forms take different input, which is why a single parser
/// function pointer will not do: a JSON-family protocol is handed a
/// parsed [`serde_json::Value`], while a protocol with its own surface
/// syntax is handed the source text.
#[derive(Clone, Copy)]
pub enum Parser {
    /// Reads a parsed JSON document. Reached by
    /// [`parse_schema_document`](crate::parse_schema_document).
    Document(fn(&serde_json::Value) -> Result<Schema, ProtocolError>),
    /// Reads text in the protocol's own syntax. Reached by
    /// [`parse_schema_source`](crate::parse_schema_source).
    Source(fn(&str) -> Result<Schema, ProtocolError>),
}

/// What one protocol supports.
pub struct ProtocolDescriptor {
    /// The canonical name. This is the spelling every listing reports
    /// and the one documentation should use.
    pub name: &'static str,
    /// Other names that resolve to this protocol. An underscore form of
    /// the canonical name is always accepted and is not listed here,
    /// since it is derived rather than declared.
    pub aliases: &'static [&'static str],
    /// How this protocol's documents are read.
    pub parser: Parser,
    /// The schema-level [`Protocol`] this protocol defines: its theory
    /// names, edge rules, vertex kinds and constraint sorts.
    pub protocol: fn() -> Protocol,
    /// Registers this protocol's theories into a registry.
    ///
    /// Every protocol has theories, so this is not optional. Most build
    /// theirs directly and cannot fail; `ATProto` composes two of its own
    /// by pushout and reports a failure rather than registering a
    /// partial set, which is why the signature returns a `Result` at
    /// all.
    pub register_theories: fn(&mut HashMap<String, Theory>) -> Result<(), ProtocolError>,
    /// Whether [`parse_schema_bundle`](crate::parse_schema_bundle)
    /// accepts it, which is to say whether a reference from one
    /// document into a sibling resolves rather than becoming an opaque
    /// placeholder.
    pub bundle: bool,
    /// Whether
    /// [`parse_schema_bundle_project`](crate::parse_schema_bundle_project)
    /// accepts it, retaining per-file provenance for the version
    /// control layer.
    pub bundle_project: bool,
}

impl ProtocolDescriptor {
    /// Whether `name` resolves to this protocol, treating `_` and `-`
    /// as the same separator.
    ///
    /// Both spellings reach the dispatch, so both must reach the
    /// lookup, or a name a caller can parse with is a name it cannot
    /// look up.
    #[must_use]
    pub fn matches(&self, name: &str) -> bool {
        let normalized = name.replace('_', "-");
        self.name == normalized || self.aliases.iter().any(|a| *a == normalized)
    }
}

/// Every protocol this crate knows how to read, in canonical-name
/// order.
#[must_use]
pub const fn descriptors() -> &'static [ProtocolDescriptor] {
    DESCRIPTORS
}

/// The descriptor `name` resolves to, by canonical name or alias.
#[must_use]
pub fn descriptor(name: &str) -> Option<&'static ProtocolDescriptor> {
    DESCRIPTORS.iter().find(|d| d.matches(name))
}

/// Canonical names of every protocol, in order.
///
/// The listing every binding reports, so a name a caller reads back is
/// a name the dispatch accepts.
#[must_use]
pub fn protocol_names() -> Vec<&'static str> {
    DESCRIPTORS.iter().map(|d| d.name).collect()
}

/// Canonical names of the protocols whose descriptors satisfy
/// `predicate`.
#[must_use]
pub fn names_where(predicate: fn(&ProtocolDescriptor) -> bool) -> Vec<&'static str> {
    DESCRIPTORS
        .iter()
        .filter(|d| predicate(d))
        .map(|d| d.name)
        .collect()
}

static DESCRIPTORS: &[ProtocolDescriptor] = &[
    ProtocolDescriptor {
        name: "amr",
        aliases: &[],
        parser: Parser::Document(annotation::amr::parse_amr_schema),
        protocol: annotation::amr::protocol,
        register_theories: |r| {
            annotation::amr::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "ansible",
        aliases: &[],
        parser: Parser::Document(config::ansible::parse_ansible_schema),
        protocol: config::ansible::protocol,
        register_theories: |r| {
            config::ansible::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "arrow",
        aliases: &[],
        parser: Parser::Document(data_science::arrow::parse_arrow_schema),
        protocol: data_science::arrow::protocol,
        register_theories: |r| {
            data_science::arrow::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "asn1",
        aliases: &[],
        parser: Parser::Source(serialization::asn1::parse_asn1),
        protocol: serialization::asn1::protocol,
        register_theories: |r| {
            serialization::asn1::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "asyncapi",
        aliases: &[],
        parser: Parser::Document(api::asyncapi::parse_asyncapi),
        protocol: api::asyncapi::protocol,
        register_theories: |r| {
            api::asyncapi::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "atproto",
        aliases: &[],
        parser: Parser::Document(web_document::atproto::parse_lexicon),
        protocol: web_document::atproto::protocol,
        register_theories: |r| web_document::atproto::register_theories(r),
        bundle: true,
        bundle_project: true,
    },
    ProtocolDescriptor {
        name: "avro",
        aliases: &[],
        parser: Parser::Document(serialization::avro::parse_avsc),
        protocol: serialization::avro::protocol,
        register_theories: |r| {
            serialization::avro::register_theories(r);
            Ok(())
        },
        bundle: true,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "bead",
        aliases: &[],
        parser: Parser::Document(annotation::bead::parse_bead),
        protocol: annotation::bead::protocol,
        register_theories: |r| {
            annotation::bead::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "bond",
        aliases: &[],
        parser: Parser::Source(serialization::bond::parse_bond),
        protocol: serialization::bond::protocol,
        register_theories: |r| {
            serialization::bond::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "brat",
        aliases: &[],
        parser: Parser::Document(annotation::brat::parse_brat),
        protocol: annotation::brat::protocol,
        register_theories: |r| {
            annotation::brat::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "bson",
        aliases: &[],
        parser: Parser::Document(data_schema::bson::parse_bson_schema),
        protocol: data_schema::bson::protocol,
        register_theories: |r| {
            data_schema::bson::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "cassandra",
        aliases: &[],
        parser: Parser::Source(database::cassandra::parse_cql),
        protocol: database::cassandra::protocol,
        register_theories: |r| {
            database::cassandra::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "cddl",
        aliases: &[],
        parser: Parser::Source(data_schema::cddl::parse_cddl),
        protocol: data_schema::cddl::protocol,
        register_theories: |r| {
            data_schema::cddl::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "cloudformation",
        aliases: &[],
        parser: Parser::Document(config::cloudformation::parse_cfn_schema),
        protocol: config::cloudformation::protocol,
        register_theories: |r| {
            config::cloudformation::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "concrete",
        aliases: &[],
        parser: Parser::Document(annotation::concrete::parse_concrete_schema),
        protocol: annotation::concrete::protocol,
        register_theories: |r| {
            annotation::concrete::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "conllu",
        aliases: &[],
        parser: Parser::Source(annotation::conllu::parse_conllu),
        protocol: annotation::conllu::protocol,
        register_theories: |r| {
            annotation::conllu::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "dataframe",
        aliases: &[],
        parser: Parser::Document(data_science::dataframe::parse_dataframe_schema),
        protocol: data_science::dataframe::protocol,
        register_theories: |r| {
            data_science::dataframe::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "decomp",
        aliases: &[],
        parser: Parser::Document(annotation::decomp::parse_decomp),
        protocol: annotation::decomp::protocol,
        register_theories: |r| {
            annotation::decomp::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "docx",
        aliases: &[],
        parser: Parser::Document(web_document::docx::parse_docx_schema),
        protocol: web_document::docx::protocol,
        register_theories: |r| {
            web_document::docx::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "dynamodb",
        aliases: &[],
        parser: Parser::Document(database::dynamodb::parse_dynamodb),
        protocol: database::dynamodb::protocol,
        register_theories: |r| {
            database::dynamodb::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "edi-x12",
        aliases: &[],
        parser: Parser::Document(domain::edi_x12::parse_edi_schema),
        protocol: domain::edi_x12::protocol,
        register_theories: |r| {
            domain::edi_x12::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "elan",
        aliases: &[],
        parser: Parser::Document(annotation::elan::parse_elan),
        protocol: annotation::elan::protocol,
        register_theories: |r| {
            annotation::elan::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "fhir",
        aliases: &[],
        parser: Parser::Document(domain::fhir::parse_fhir_schema),
        protocol: domain::fhir::protocol,
        register_theories: |r| {
            domain::fhir::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "flatbuffers",
        aliases: &[],
        parser: Parser::Source(serialization::flatbuffers::parse_fbs),
        protocol: serialization::flatbuffers::protocol,
        register_theories: |r| {
            serialization::flatbuffers::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "folia",
        aliases: &[],
        parser: Parser::Document(annotation::folia::parse_folia),
        protocol: annotation::folia::protocol,
        register_theories: |r| {
            annotation::folia::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "fovea",
        aliases: &[],
        parser: Parser::Document(annotation::fovea::parse_fovea),
        protocol: annotation::fovea::protocol,
        register_theories: |r| {
            annotation::fovea::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "geojson",
        aliases: &[],
        parser: Parser::Document(domain::geojson::parse_geojson_schema),
        protocol: domain::geojson::protocol,
        register_theories: |r| {
            domain::geojson::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "graphql",
        aliases: &[],
        parser: Parser::Source(api::graphql::parse_sdl),
        protocol: api::graphql::protocol,
        register_theories: |r| {
            api::graphql::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "iso-space",
        aliases: &[],
        parser: Parser::Document(annotation::iso_space::parse_iso_space),
        protocol: annotation::iso_space::protocol,
        register_theories: |r| {
            annotation::iso_space::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "json-schema",
        aliases: &[],
        parser: Parser::Document(data_schema::json_schema::parse_json_schema),
        protocol: data_schema::json_schema::protocol,
        register_theories: |r| {
            data_schema::json_schema::register_theories(r);
            Ok(())
        },
        bundle: true,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "jsonapi",
        aliases: &[],
        parser: Parser::Document(api::jsonapi::parse_jsonapi),
        protocol: api::jsonapi::protocol,
        register_theories: |r| {
            api::jsonapi::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "k8s-crd",
        aliases: &[],
        parser: Parser::Document(config::k8s_crd::parse_k8s_crd_schema),
        protocol: config::k8s_crd::protocol,
        register_theories: |r| {
            config::k8s_crd::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "laf-graf",
        aliases: &[],
        parser: Parser::Document(annotation::laf_graf::parse_laf_graf),
        protocol: annotation::laf_graf::protocol,
        register_theories: |r| {
            annotation::laf_graf::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "mongodb",
        aliases: &[],
        parser: Parser::Document(database::mongodb::parse_mongodb_schema),
        protocol: database::mongodb::protocol,
        register_theories: |r| {
            database::mongodb::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "msgpack-schema",
        aliases: &[],
        parser: Parser::Document(serialization::msgpack_schema::parse_msgpack_schema),
        protocol: serialization::msgpack_schema::protocol,
        register_theories: |r| {
            serialization::msgpack_schema::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "naf",
        aliases: &[],
        parser: Parser::Document(annotation::naf::parse_naf),
        protocol: annotation::naf::protocol,
        register_theories: |r| {
            annotation::naf::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "neo4j",
        aliases: &[],
        parser: Parser::Source(database::neo4j::parse_cypher_schema),
        protocol: database::neo4j::protocol,
        register_theories: |r| {
            database::neo4j::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "nif",
        aliases: &[],
        parser: Parser::Document(annotation::nif::parse_nif_schema),
        protocol: annotation::nif::protocol,
        register_theories: |r| {
            annotation::nif::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "odf",
        aliases: &[],
        parser: Parser::Document(web_document::odf::parse_odf_schema),
        protocol: web_document::odf::protocol,
        register_theories: |r| {
            web_document::odf::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "openapi",
        aliases: &[],
        parser: Parser::Document(api::openapi::parse_openapi),
        protocol: api::openapi::protocol,
        register_theories: |r| {
            api::openapi::register_theories(r);
            Ok(())
        },
        bundle: true,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "parquet",
        aliases: &[],
        parser: Parser::Document(data_science::parquet::parse_parquet_schema),
        protocol: data_science::parquet::protocol,
        register_theories: |r| {
            data_science::parquet::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "paula",
        aliases: &[],
        parser: Parser::Document(annotation::paula::parse_paula_schema),
        protocol: annotation::paula::protocol,
        register_theories: |r| {
            annotation::paula::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "protobuf",
        aliases: &[],
        parser: Parser::Source(serialization::protobuf::parse_proto),
        protocol: serialization::protobuf::protocol,
        register_theories: |r| {
            serialization::protobuf::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "raml",
        aliases: &[],
        parser: Parser::Document(api::raml::parse_raml_schema),
        protocol: api::raml::protocol,
        register_theories: |r| {
            api::raml::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "redis",
        aliases: &[],
        parser: Parser::Source(database::redis::parse_redis_schema),
        protocol: database::redis::protocol,
        register_theories: |r| {
            database::redis::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "rss-atom",
        aliases: &[],
        parser: Parser::Document(domain::rss_atom::parse_rss_atom_schema),
        protocol: domain::rss_atom::protocol,
        register_theories: |r| {
            domain::rss_atom::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "sql",
        aliases: &[],
        parser: Parser::Source(database::sql::parse_ddl),
        protocol: database::sql::protocol,
        register_theories: |r| {
            database::sql::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "swift-mt",
        aliases: &[],
        parser: Parser::Document(domain::swift_mt::parse_swift_mt_schema),
        protocol: domain::swift_mt::protocol,
        register_theories: |r| {
            domain::swift_mt::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "tei",
        aliases: &[],
        parser: Parser::Document(annotation::tei::parse_tei),
        protocol: annotation::tei::protocol,
        register_theories: |r| {
            annotation::tei::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "timeml",
        aliases: &[],
        parser: Parser::Document(annotation::timeml::parse_timeml),
        protocol: annotation::timeml::protocol,
        register_theories: |r| {
            annotation::timeml::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "ucca",
        aliases: &[],
        parser: Parser::Document(annotation::ucca::parse_ucca),
        protocol: annotation::ucca::protocol,
        register_theories: |r| {
            annotation::ucca::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "uima",
        aliases: &["uima-cas"],
        parser: Parser::Document(annotation::uima::parse_uima_schema),
        protocol: annotation::uima::protocol,
        register_theories: |r| {
            annotation::uima::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "vcard-ical",
        aliases: &[],
        parser: Parser::Document(domain::vcard_ical::parse_vcard_ical_schema),
        protocol: domain::vcard_ical::protocol,
        register_theories: |r| {
            domain::vcard_ical::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
    ProtocolDescriptor {
        name: "web-annotation",
        aliases: &[],
        parser: Parser::Document(annotation::web_annotation::parse_web_annotation_schema),
        protocol: annotation::web_annotation::protocol,
        register_theories: |r| {
            annotation::web_annotation::register_theories(r);
            Ok(())
        },
        bundle: false,
        bundle_project: false,
    },
];
