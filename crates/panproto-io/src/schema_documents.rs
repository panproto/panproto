//! Schema documents read by protocol name, whichever reader a protocol has.
//!
//! The protocol registry of `panproto-protocols` records how each protocol's
//! schema documents are read. A [`Parser::Document`] or [`Parser::Source`]
//! protocol is read in `panproto-protocols` itself. A [`Parser::Grammar`]
//! protocol is read through a tree-sitter grammar by the reader of this
//! crate whose path the descriptor names, which that crate cannot link.
//!
//! [`parse_schema_document`], [`parse_schema_document_text`],
//! [`parse_schema_source`] and [`parse_schema_bundle`] take a protocol name and answer for every
//! protocol: they hand a document or source protocol to the dispatch of
//! `panproto-protocols`, and a grammar protocol to its reader here, keyed by
//! the reader path the descriptor records, so the registry stays the one
//! list of protocols. A reader's refusal comes back unchanged inside
//! [`SchemaReadError`].
//!
//! The grammar readers exist under this crate's `tree-sitter` feature. A
//! build without it refuses a grammar protocol with
//! [`SchemaReadError::GrammarNotBuilt`], which names the reader, so a front
//! end can say which of its features provides it.

use panproto_protocols::config::ansible::FileClass;
use panproto_protocols::database::postgresql::{OtherStatements, SqlDdlError};
use panproto_protocols::registry::{self, Parser};
use panproto_protocols::{ProtocolError, bundle_parser_protocols};
use panproto_schema::Schema;

#[cfg(feature = "tree-sitter")]
use crate::api::{asyncapi, openapi};
#[cfg(feature = "tree-sitter")]
use crate::config::{ansible, cloudformation, k8s_crd};
#[cfg(feature = "tree-sitter")]
use crate::data_schema::json_schema;

/// What a reader needs that a document does not state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadOptions {
    /// The class of an Ansible file. The `ansible` reader refuses a
    /// document without one with [`SchemaReadError::FileClassRequired`];
    /// every other reader ignores it.
    pub file_class: Option<FileClass>,
    /// What the `postgresql` reader does with a statement that declares no
    /// table: refuse the script, or read past it. Every other reader
    /// ignores it.
    pub other_statements: OtherStatements,
}

/// One document of a bundle: its retrieval URI, if it has one, and its
/// text.
#[derive(Clone, Copy, Debug)]
pub struct BundleSource<'a> {
    /// The absolute URI the document was retrieved from, against which the
    /// references in it resolve.
    pub retrieval_uri: Option<&'a str>,
    /// The document's text: JSON, or YAML where the protocol reads YAML.
    pub source: &'a str,
}

/// A refusal of a schema document, from the dispatch or from the reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaReadError {
    /// The protocol is unknown, takes another kind of input, reads no
    /// schema documents, or its `panproto-protocols` reader refused the
    /// document.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// The `openapi` reader refused the document.
    #[cfg(feature = "tree-sitter")]
    #[error(transparent)]
    OpenApi(#[from] openapi::OpenApiError),
    /// The `asyncapi` reader refused the document.
    #[cfg(feature = "tree-sitter")]
    #[error(transparent)]
    AsyncApi(#[from] asyncapi::AsyncApiError),
    /// The `json-schema` reader refused the document.
    #[cfg(feature = "tree-sitter")]
    #[error(transparent)]
    JsonSchema(#[from] json_schema::JsonSchemaError),
    /// The `k8s-crd` reader refused the document.
    #[cfg(feature = "tree-sitter")]
    #[error(transparent)]
    Crd(#[from] k8s_crd::CrdError),
    /// The `cloudformation` reader refused the template.
    #[cfg(feature = "tree-sitter")]
    #[error(transparent)]
    CloudFormation(#[from] cloudformation::CfnError),
    /// The `ansible` reader refused the file.
    #[cfg(feature = "tree-sitter")]
    #[error(transparent)]
    Ansible(#[from] ansible::AnsibleError),
    /// The `postgresql` reader refused the script.
    #[error(transparent)]
    PostgreSql(#[from] SqlDdlError),
    /// The protocol is read through a tree-sitter grammar, and this build
    /// of `panproto-io` does not have the `tree-sitter` feature that
    /// provides its reader.
    #[error(
        "protocol {protocol:?} is read through its tree-sitter grammar by {reader}, \
         which this build does not include (panproto-io feature tree-sitter)"
    )]
    GrammarNotBuilt {
        /// The protocol's canonical name.
        protocol: &'static str,
        /// The path of the reader, as the registry records it.
        reader: &'static str,
    },
    /// The protocol is `ansible` and no [`FileClass`] was given, which an
    /// Ansible file does not state.
    #[error(
        "protocol \"ansible\" needs the file's class (playbook, tasks or vars), \
         which an Ansible file does not state"
    )]
    FileClassRequired,
}

/// A reader of a bundle of documents.
type ReadBundle = fn(&[BundleSource<'_>]) -> Result<Schema, SchemaReadError>;

/// The reader of one grammar protocol, keyed by the path the registry
/// records for it.
struct GrammarReader {
    /// The reader's path, as `crate::module::function`.
    reader: &'static str,
    /// Whether the protocol's documents are JSON (or YAML, which the JSON
    /// documents of [`parse_schema_document`] are a case of). A protocol
    /// with its own text syntax is reached by [`parse_schema_source`] only.
    json: bool,
    /// Reads one document.
    read: fn(&str, &ReadOptions) -> Result<Schema, SchemaReadError>,
    /// Reads a bundle, for a protocol whose reader has a bundle form.
    bundle: Option<ReadBundle>,
}

/// Every grammar reader this build has.
#[cfg(feature = "tree-sitter")]
const GRAMMAR_READERS: &[GrammarReader] = &[
    GrammarReader {
        reader: "panproto_io::api::openapi::read_openapi",
        json: true,
        read: |source, _| Ok(openapi::read_openapi(source.as_bytes())?.schema),
        bundle: Some(read_openapi_bundle),
    },
    GrammarReader {
        reader: "panproto_io::api::asyncapi::read_asyncapi",
        json: true,
        read: |source, _| Ok(asyncapi::read_asyncapi(source.as_bytes())?.schema),
        bundle: Some(read_asyncapi_bundle),
    },
    GrammarReader {
        reader: "panproto_io::data_schema::json_schema::read_schema",
        json: true,
        read: |source, _| Ok(json_schema::read_schema(source.as_bytes())?.schema),
        bundle: Some(read_json_schema_bundle),
    },
    GrammarReader {
        reader: "panproto_io::config::k8s_crd::read_crd",
        json: true,
        read: |source, _| Ok(k8s_crd::read_crd(source.as_bytes())?.schema),
        bundle: None,
    },
    GrammarReader {
        reader: "panproto_io::config::cloudformation::read_template",
        json: true,
        read: |source, _| Ok(cloudformation::read_template(source.as_bytes())?.schema),
        bundle: None,
    },
    GrammarReader {
        reader: "panproto_io::config::ansible::read_ansible",
        json: true,
        read: |source, options| {
            let class = options
                .file_class
                .ok_or(SchemaReadError::FileClassRequired)?;
            Ok(ansible::read_ansible(source.as_bytes(), class)?.schema)
        },
        bundle: None,
    },
    GrammarReader {
        reader: "panproto_io::postgresql::read_ddl",
        json: false,
        read: |source, options| {
            Ok(crate::postgresql::read_ddl(source, options.other_statements)?.schema)
        },
        bundle: None,
    },
];

/// Every grammar reader this build has: none, without `tree-sitter`.
#[cfg(not(feature = "tree-sitter"))]
const GRAMMAR_READERS: &[GrammarReader] = &[];

/// Whether this build reads the grammar protocols, which is whether it has
/// this crate's `tree-sitter` feature.
#[must_use]
pub const fn grammar_readers_built() -> bool {
    !GRAMMAR_READERS.is_empty()
}

/// The reader of a grammar protocol, or the refusal of a build without it.
fn grammar_reader(
    protocol: &'static str,
    reader: &'static str,
) -> Result<&'static GrammarReader, SchemaReadError> {
    GRAMMAR_READERS
        .iter()
        .find(|arm| arm.reader == reader)
        .ok_or(SchemaReadError::GrammarNotBuilt { protocol, reader })
}

/// Read one schema document, given as parsed JSON, under `protocol`.
///
/// A grammar protocol whose documents are JSON or YAML reads the
/// document's JSON text; every other protocol goes to
/// [`panproto_protocols::parse_schema_document`].
///
/// # Errors
///
/// Returns the reader's refusal, [`SchemaReadError::GrammarNotBuilt`] for a
/// grammar protocol this build cannot read, or a [`ProtocolError`] for an
/// unknown protocol or one read from source text.
pub fn parse_schema_document(
    protocol: &str,
    doc: &serde_json::Value,
    options: &ReadOptions,
) -> Result<Schema, SchemaReadError> {
    let Some(descriptor) = registry::descriptor(protocol) else {
        return Ok(panproto_protocols::parse_schema_document(protocol, doc)?);
    };
    let Parser::Grammar { reader } = descriptor.parser else {
        return Ok(panproto_protocols::parse_schema_document(protocol, doc)?);
    };
    let arm = json_grammar_reader(protocol, descriptor.name, reader)?;
    (arm.read)(&doc.to_string(), options)
}

/// Read one schema document, given as its JSON text, under `protocol`.
///
/// A grammar protocol whose documents are JSON or YAML reads the text as
/// written, so its reader sees the document's key order and any repeated
/// key; every other protocol parses the text as JSON and goes to
/// [`panproto_protocols::parse_schema_document`].
///
/// # Errors
///
/// Returns the reader's refusal, [`SchemaReadError::GrammarNotBuilt`] for a
/// grammar protocol this build cannot read, or a [`ProtocolError`] for text
/// that is not JSON, an unknown protocol, or one read from source text.
pub fn parse_schema_document_text(
    protocol: &str,
    text: &str,
    options: &ReadOptions,
) -> Result<Schema, SchemaReadError> {
    if let Some(descriptor) = registry::descriptor(protocol)
        && let Parser::Grammar { reader } = descriptor.parser
    {
        let arm = json_grammar_reader(protocol, descriptor.name, reader)?;
        return (arm.read)(text, options);
    }
    let doc: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| ProtocolError::Parse(format!("document is not valid JSON: {e}")))?;
    Ok(panproto_protocols::parse_schema_document(protocol, &doc)?)
}

/// The reader of a grammar protocol whose documents are JSON or YAML, or
/// the refusal of a protocol read from text in its own syntax.
fn json_grammar_reader(
    protocol: &str,
    name: &'static str,
    reader: &'static str,
) -> Result<&'static GrammarReader, SchemaReadError> {
    let arm = grammar_reader(name, reader)?;
    if arm.json {
        Ok(arm)
    } else {
        Err(ProtocolError::Parse(format!(
            "protocol {protocol:?} is read from source text, not a JSON document; \
             use parse_schema_source"
        ))
        .into())
    }
}

/// Read one schema document, given as text, under `protocol`.
///
/// A grammar protocol reads the text with its reader, as JSON or YAML or in
/// its own syntax; every other protocol goes to
/// [`panproto_protocols::parse_schema_source`].
///
/// # Errors
///
/// Returns the reader's refusal, [`SchemaReadError::GrammarNotBuilt`] for a
/// grammar protocol this build cannot read, or a [`ProtocolError`] for an
/// unknown protocol or one read from a parsed JSON document.
pub fn parse_schema_source(
    protocol: &str,
    source: &str,
    options: &ReadOptions,
) -> Result<Schema, SchemaReadError> {
    let Some(descriptor) = registry::descriptor(protocol) else {
        return Ok(panproto_protocols::parse_schema_source(protocol, source)?);
    };
    let Parser::Grammar { reader } = descriptor.parser else {
        return Ok(panproto_protocols::parse_schema_source(protocol, source)?);
    };
    (grammar_reader(descriptor.name, reader)?.read)(source, options)
}

/// Read a bundle of schema documents under `protocol` into one schema,
/// resolving references between them.
///
/// A grammar protocol whose reader has a bundle form (`openapi`,
/// `asyncapi`, `json-schema`) reads the documents with it, the first
/// document its entry; every other protocol parses each document as JSON
/// and goes to [`panproto_protocols::parse_schema_bundle`], which takes no
/// retrieval URIs.
///
/// # Errors
///
/// Returns the reader's refusal, [`SchemaReadError::GrammarNotBuilt`] for a
/// grammar protocol this build cannot read, or a [`ProtocolError`] for a
/// protocol with no bundle reader or a document that is not JSON.
pub fn parse_schema_bundle(
    protocol: &str,
    docs: &[BundleSource<'_>],
) -> Result<Schema, SchemaReadError> {
    if let Some(descriptor) = registry::descriptor(protocol)
        && let Parser::Grammar { reader } = descriptor.parser
    {
        let arm = grammar_reader(descriptor.name, reader)?;
        let Some(bundle) = arm.bundle else {
            return Err(ProtocolError::Parse(format!(
                "no bundle parser registered for protocol {protocol:?}; supported: {:?}",
                bundle_parser_protocols()
            ))
            .into());
        };
        return bundle(docs);
    }
    let values = docs
        .iter()
        .map(|doc| {
            serde_json::from_str(doc.source)
                .map_err(|e| ProtocolError::Parse(format!("a bundle document is not JSON: {e}")))
        })
        .collect::<Result<Vec<serde_json::Value>, _>>()?;
    Ok(panproto_protocols::parse_schema_bundle(protocol, &values)?)
}

/// Whether a JSON or YAML document parses and has `key` at its root,
/// which decides the kind of a bundle document, since a bundle names its
/// documents but not their kinds.
#[cfg(feature = "tree-sitter")]
fn has_root_key(source: &str, key: &str) -> bool {
    yaml_serde::from_str::<serde_json::Value>(source).is_ok_and(|root| root.get(key).is_some())
}

#[cfg(feature = "tree-sitter")]
fn read_openapi_bundle(docs: &[BundleSource<'_>]) -> Result<Schema, SchemaReadError> {
    let documents: Vec<openapi::ApiDocument<'_>> = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| openapi::ApiDocument {
            retrieval_uri: doc.retrieval_uri,
            source: doc.source.as_bytes(),
            kind: if i == 0 || has_root_key(doc.source, "openapi") {
                openapi::DocumentKind::OpenApi
            } else {
                openapi::DocumentKind::Schema
            },
        })
        .collect();
    Ok(openapi::read_openapi_bundle(&documents)?.schema)
}

#[cfg(feature = "tree-sitter")]
fn read_asyncapi_bundle(docs: &[BundleSource<'_>]) -> Result<Schema, SchemaReadError> {
    let documents: Vec<asyncapi::AsyncApiDocument<'_>> = docs
        .iter()
        .enumerate()
        .map(|(i, doc)| asyncapi::AsyncApiDocument {
            retrieval_uri: doc.retrieval_uri,
            source: doc.source.as_bytes(),
            kind: if i == 0 || has_root_key(doc.source, "asyncapi") {
                asyncapi::DocumentKind::AsyncApi
            } else {
                asyncapi::DocumentKind::Fragment
            },
        })
        .collect();
    Ok(asyncapi::read_asyncapi_bundle(&documents)?.schema)
}

#[cfg(feature = "tree-sitter")]
fn read_json_schema_bundle(docs: &[BundleSource<'_>]) -> Result<Schema, SchemaReadError> {
    let documents: Vec<json_schema::BundleDocument<'_>> = docs
        .iter()
        .map(|doc| json_schema::BundleDocument {
            retrieval_uri: doc.retrieval_uri,
            source: doc.source.as_bytes(),
        })
        .collect();
    Ok(json_schema::read_bundle(&documents)?.schema)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Whether two schemas have the same canonical encoding, which is
    /// what their identity is over.
    #[cfg(feature = "tree-sitter")]
    fn same(left: &Schema, right: &Schema) -> bool {
        panproto_schema::canonical_bytes(left) == panproto_schema::canonical_bytes(right)
    }

    /// The reader path of every grammar protocol of the registry, with
    /// whether the registry says it reads bundles.
    fn registered() -> Vec<(&'static str, &'static str, bool)> {
        registry::descriptors()
            .iter()
            .filter_map(|d| match d.parser {
                Parser::Grammar { reader } => Some((d.name, reader, d.bundle)),
                _ => None,
            })
            .collect()
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn every_grammar_protocol_has_exactly_one_reader_and_every_reader_a_protocol() {
        for (name, reader, bundle) in registered() {
            let arms: Vec<_> = GRAMMAR_READERS
                .iter()
                .filter(|arm| arm.reader == reader)
                .collect();
            assert_eq!(arms.len(), 1, "{name}: {reader} has {} arms", arms.len());
            assert_eq!(
                arms[0].bundle.is_some(),
                bundle,
                "{name}: the registry and the reader disagree on bundles"
            );
        }
        for arm in GRAMMAR_READERS {
            let protocols = registered()
                .into_iter()
                .filter(|(_, reader, _)| *reader == arm.reader)
                .count();
            assert_eq!(protocols, 1, "{} answers {protocols} protocols", arm.reader);
        }
        assert!(grammar_readers_built());
    }

    #[cfg(not(feature = "tree-sitter"))]
    #[test]
    fn a_build_without_the_grammars_refuses_every_grammar_protocol_by_its_reader() {
        assert!(!grammar_readers_built());
        for (name, reader, _) in registered() {
            let err = parse_schema_source(name, "{}", &ReadOptions::default())
                .expect_err("no grammar reader is built");
            assert!(
                matches!(
                    err,
                    SchemaReadError::GrammarNotBuilt { protocol, reader: r }
                        if protocol == name && r == reader
                ),
                "{name}: {err}"
            );
        }
    }

    #[test]
    fn a_document_protocol_goes_to_the_protocols_dispatch() {
        let doc = serde_json::json!({
            "lexicon": 1,
            "id": "com.example.post",
            "defs": { "main": { "type": "record", "key": "tid",
                "record": { "type": "object", "properties": {} } } }
        });
        let schema = parse_schema_document("atproto", &doc, &ReadOptions::default())
            .expect("an atproto lexicon reads");
        let direct = panproto_protocols::parse_schema_document("atproto", &doc).unwrap();
        assert_eq!(schema.vertex_count(), direct.vertex_count());
    }

    #[test]
    fn document_text_of_a_document_protocol_is_parsed_as_json() {
        let options = ReadOptions::default();
        let text = r#"{"type": "record", "name": "User", "fields": []}"#;
        let schema = parse_schema_document_text("avro", text, &options).unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let direct = panproto_protocols::parse_schema_document("avro", &value).unwrap();
        assert_eq!(schema.vertex_count(), direct.vertex_count());
        let err = parse_schema_document_text("avro", "{", &options).unwrap_err();
        assert!(matches!(err, SchemaReadError::Protocol(_)), "{err}");
    }

    #[test]
    fn an_unknown_protocol_is_refused_by_the_protocols_dispatch() {
        let err = parse_schema_source("no-such-protocol", "", &ReadOptions::default())
            .expect_err("no protocol has the name");
        assert!(matches!(err, SchemaReadError::Protocol(_)), "{err}");
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn a_grammar_protocol_is_read_by_its_reader() {
        let options = ReadOptions::default();
        let api = r#"{"openapi": "3.1.0", "info": {"title": "Pets", "version": "1"}, "paths": {}}"#;
        let schema = parse_schema_source("openapi", api, &options).unwrap();
        assert!(same(
            &schema,
            &openapi::read_openapi(api.as_bytes()).unwrap().schema
        ));
        let value: serde_json::Value = serde_json::from_str(api).unwrap();
        assert!(same(
            &parse_schema_document("openapi", &value, &options).unwrap(),
            &schema
        ));

        let ddl = "CREATE TABLE pets (id integer PRIMARY KEY);";
        let schema = parse_schema_source("postgresql", ddl, &options).unwrap();
        assert!(same(
            &schema,
            &crate::postgresql::read_ddl(ddl, OtherStatements::Refuse)
                .unwrap()
                .schema
        ));
        let err = parse_schema_document("postgresql", &value, &options)
            .expect_err("DDL is not a JSON document");
        assert!(matches!(err, SchemaReadError::Protocol(_)), "{err}");
        let err = parse_schema_document_text("postgresql", api, &options)
            .expect_err("DDL is not a JSON document");
        assert!(matches!(err, SchemaReadError::Protocol(_)), "{err}");

        // A repeated key is the reader's to refuse, which it can only do
        // when it reads the text as written.
        let repeated = r#"{"openapi": "3.1.0", "openapi": "3.1.0"}"#;
        let err = parse_schema_document_text("openapi", repeated, &options).unwrap_err();
        let direct = openapi::read_openapi(repeated.as_bytes()).unwrap_err();
        assert!(
            matches!(err, SchemaReadError::OpenApi(ref e) if *e == direct),
            "{err}"
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn a_reader_refusal_comes_back_unchanged() {
        let options = ReadOptions::default();
        let swagger = r#"{"swagger": "2.0"}"#;
        let err = parse_schema_source("openapi", swagger, &options).unwrap_err();
        let direct = openapi::read_openapi(swagger.as_bytes()).unwrap_err();
        assert!(
            matches!(err, SchemaReadError::OpenApi(ref e) if *e == direct),
            "{err}"
        );
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn ansible_needs_a_file_class() {
        let vars = "greeting: hello\n";
        let err = parse_schema_source("ansible", vars, &ReadOptions::default()).unwrap_err();
        assert!(matches!(err, SchemaReadError::FileClassRequired), "{err}");
        let options = ReadOptions {
            file_class: Some(FileClass::Vars),
            ..ReadOptions::default()
        };
        assert!(same(
            &parse_schema_source("ansible", vars, &options).unwrap(),
            &ansible::read_ansible(vars.as_bytes(), FileClass::Vars)
                .unwrap()
                .schema
        ));
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn a_json_schema_bundle_resolves_between_its_documents() {
        let entry = r#"{"$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/a", "$ref": "b"}"#;
        let target = r#"{"$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/b", "type": "string"}"#;
        let docs = [
            BundleSource {
                retrieval_uri: None,
                source: entry,
            },
            BundleSource {
                retrieval_uri: None,
                source: target,
            },
        ];
        let schema = parse_schema_bundle("json-schema", &docs).unwrap();
        let direct = json_schema::read_bundle(&[
            json_schema::BundleDocument {
                retrieval_uri: None,
                source: entry.as_bytes(),
            },
            json_schema::BundleDocument {
                retrieval_uri: None,
                source: target.as_bytes(),
            },
        ])
        .unwrap()
        .schema;
        assert!(same(&schema, &direct));
        let err = parse_schema_bundle("k8s-crd", &docs).unwrap_err();
        assert!(matches!(err, SchemaReadError::Protocol(_)), "{err}");
    }
}
