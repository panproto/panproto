# Protocol catalogue

A *protocol* in panproto is a schema language: Avro, CDDL, OpenAPI, ATProto Lexicons, Parquet, FHIR, a Kubernetes CRD. Each one is defined by a pair of GATs (a schema theory and an instance theory) composed by colimit from reusable building-block theories. Every protocol provides both a parser (native format → `Schema`) and an emitter (`Schema` → native format), so panproto can round-trip data through any pair.

For the model behind these registrations, see [Schemas as theories](../explanation/schemas-as-theories.md) and [Composing protocols by colimit](../explanation/protocol-colimits.md).

## Categories

The built-in protocols are organised by category in `panproto-protocols`. Each category is a Rust submodule.

| Category | Module | Protocols |
|---|---|---|
| Serialization and IDLs | `serialization` | Avro, FlatBuffers, ASN.1, Bond, MessagePack Schema |
| Data schema | `data_schema` | CDDL, BSON |
| API specifications | `api` | OpenAPI, AsyncAPI, RAML, JSON:API |
| Database | `database` | MongoDB, Cassandra, DynamoDB, Neo4j, Redis |
| Web and document | `web_document` | ATProto Lexicons, DOCX, ODF |
| Data science | `data_science` | Parquet, Arrow, DataFrame schemas |
| Domain | `domain` | GeoJSON, FHIR, RSS/Atom, vCard/iCal, EDI X12, SWIFT MT |
| Configuration | `config` | Kubernetes CRDs, CloudFormation, Ansible |
| Linguistic annotation | `annotation` | AMR, bead, BRAT, Concrete, CoNLL-U, Decomp/UDS, ELAN, FoLiA, FOVEA, ISO-Space, LAF/GrAF, NAF, NIF, PAULA/Salt, TEI XML, TimeML, UCCA, UIMA/CAS, W3C Web Annotation |
| Raw file | `raw_file` | Non-code files (README, LICENSE, images) |

The authoritative list is in the [`panproto-protocols`](https://github.com/panproto/panproto/tree/main/crates/panproto-protocols/src) source tree. Each submodule's `register_*` function documents the building-block theories it composes.

## Registration shape

A protocol registration is a sequence of theory colimits applied in a determined order. For example, the constrained-multigraph-with-W-types theory used by MessagePack Schema (and several other protocols) is built as `colimit(colimit(ThGraph, ThConstraint; Vertex), ThMulti; Vertex, Edge)`, with `ThWType` as the instance theory. If any colimit step fails, registration panics with a message naming the failing intermediate step. This is intentional: a registration failure is a build-time bug in the theory composition, not user input that can fail at runtime.

## Source-of-truth

| Format | Source |
|---|---|
| Built-in protocol list | [`crates/panproto-protocols/src/lib.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/lib.rs) |
| Building-block theories | [`crates/panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs) |
| Tree-sitter grammar list (259 languages) | [`crates/panproto-grammars/`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars) |

## Defining a new protocol

To add a custom protocol, see [Build a custom protocol](../how-to/build-protocol.md). The minimal recipe is: declare schema and instance GATs (Rust or via the [theory DSL](../how-to/build-protocol.md)), register a parser and emitter, and add a registration call to the relevant submodule.

## See also

- [Schemas as theories](../explanation/schemas-as-theories.md)
- [Composing protocols by colimit](../explanation/protocol-colimits.md)
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md)
