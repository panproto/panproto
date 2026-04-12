# panproto-protocols

[![crates.io](https://img.shields.io/crates/v/panproto-protocols.svg)](https://crates.io/crates/panproto-protocols)
[![docs.rs](https://docs.rs/panproto-protocols/badge.svg)](https://docs.rs/panproto-protocols)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

51 built-in schema language definitions, each describing the rules for one data or API format.

## What it does

A protocol in panproto is a definition of what a valid schema looks like for a particular format, plus a parser that reads that format's native files into panproto's internal schema representation. For example, the ATProto protocol knows that Lexicon files are JSON objects with `defs` blocks, what kinds of fields are allowed, and how those fields relate to each other. The OpenAPI protocol knows about paths, operations, and components.

Each protocol is assembled from smaller building-block theories by combining them with colimit (a way of merging theories that shares their common parts rather than duplicating them). For instance, a protocol that uses ordered collections includes `ThOrder`; one that needs validation rules includes `ThConstraint`. Building blocks are reused across protocols so the definitions stay compact and consistent.

Programming language parsing (TypeScript, Python, Rust, and so on) is handled separately by `panproto-parse` using tree-sitter grammars. The protocols here are semantic formats: annotation schemas, API specifications, database schemas, domain-specific formats, and serialization IDLs.

## Quick example

```rust,ignore
use panproto_protocols::atproto;

// Get the ATProto protocol definition.
let protocol = atproto::protocol();

// Parse a Lexicon file into a panproto Schema.
let schema = atproto::parse_lexicon(&lexicon_json_bytes)?;
```

## API overview

| Module | What it covers |
|--------|---------------|
| `web_document::atproto` | ATProto Lexicons |
| `api::openapi` | OpenAPI 3.x |
| `api::asyncapi` | AsyncAPI |
| `api::jsonapi` | JSON:API |
| `api::raml` | RAML |
| `database::mongodb` | MongoDB document schemas |
| `database::dynamodb` | DynamoDB table definitions |
| `database::cassandra` | Cassandra CQL schemas |
| `database::neo4j` | Neo4j property graph schemas |
| `database::redis` | Redis data structure schemas |
| `serialization::avro` | Apache Avro |
| `serialization::flatbuffers` | FlatBuffers |
| `serialization::asn1` | ASN.1 |
| `serialization::bond` | Microsoft Bond |
| `serialization::msgpack_schema` | MessagePack schema |
| `config::k8s_crd` | Kubernetes CRDs |
| `config::cloudformation` | AWS CloudFormation |
| `config::ansible` | Ansible playbooks |
| `domain::geojson` | GeoJSON |
| `domain::fhir` | HL7 FHIR |
| `domain::rss_atom` | RSS / Atom feeds |
| `domain::vcard_ical` | vCard / iCal |
| `domain::swift_mt` | SWIFT MT messages |
| `domain::edi_x12` | EDI X12 |
| `theories` | 5 building-block theories: `ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta` |
| `ProtocolError` | Error type for protocol operations |

## Building-block theories

Each protocol's schema and instance theories are composed from five reusable pieces:

| Theory | What it provides |
|--------|----------------|
| `ThGraph` | Basic directed graph: `Vertex`, `Edge`, `src`, `tgt` |
| `ThConstraint` | Vertex-attached validation rules |
| `ThMulti` | Multi-edges between vertex pairs |
| `ThWType` | W-type instance shape for hierarchical data |
| `ThMeta` | Instance metadata (labels, provenance) |

## License

[MIT](../../LICENSE)
