# Protocol catalog

A [protocol](../glossary.md#protocol) names a schema language together with its schema and instance theories, structural rules, parser, and emitter. The semantic protocols in `panproto-protocols` compose reusable generalized algebraic theories (GATs) [@cartmell1986generalised]. Source-code languages use the separate tree-sitter registry described below.

## Semantic protocols

The protocol registry lists 57 semantic protocols. Names in this table are the canonical hyphenated strings the registry and the generic dispatch functions accept; underscore spellings are normalized before lookup. Of these, 54 read schema documents through [`parse_schema_document`](https://docs.rs/panproto-protocols/latest/panproto_protocols/fn.parse_schema_document.html) or [`parse_schema_source`](https://docs.rs/panproto-protocols/latest/panproto_protocols/fn.parse_schema_source.html). The three generic text formats, `csv`, `toml` and `yaml`, carry data and no schema of their own, so they read no schema documents: a schema for one is built in code, and both dispatch functions refuse them with an error that says so. Their instance codecs are registered under the `tree-sitter` feature of `panproto-io`.

| Category module | Protocol names |
|---|---|
| `annotation` | `amr`, `bead`, `brat`, `concrete`, `conllu`, `decomp`, `elan`, `folia`, `fovea`, `iso-space`, `laf-graf`, `naf`, `nif`, `paula`, `tei`, `timeml`, `ucca`, `uima`, `web-annotation` |
| `api` | `asyncapi`, `graphql`, `jsonapi`, `openapi`, `raml` |
| `config` | `ansible`, `cloudformation`, `k8s-crd` |
| `data_schema` | `bson`, `cddl`, `json-schema` |
| `data_science` | `arrow`, `dataframe`, `parquet` |
| `database` | `cassandra`, `dynamodb`, `mongodb`, `neo4j`, `redis`, `sql` |
| `domain` | `edi-x12`, `fhir`, `geojson`, `rss-atom`, `swift-mt`, `vcard-ical` |
| `serialization` | `asn1`, `avro`, `bond`, `flatbuffers`, `msgpack-schema`, `protobuf` |
| `text` | `csv`, `toml`, `yaml` |
| `web_document` | `atproto`, `docx`, `odf` |

The `raw_file` module is the text-or-binary fallback used during project assembly. It is a protocol implementation, but is not listed by either generic schema-dispatch function.

### Parser dispatch

| Entry point | Accepted input | Registered protocols |
|---|---|---|
| `parse_schema_document` | `serde_json::Value` | 43 protocols |
| `parse_schema_source` | text IDL, DDL, or annotation source | 11 protocols |
| `parse_schema_bundle` | JSON document bundle | `atproto` only |
| `parse_schema_bundle_project` | path and JSON pairs with per-file provenance | `atproto` only |

The exported `document_parser_protocols`, `source_parser_protocols`, `bundle_parser_protocols`, and `bundle_project_protocols` listings are the lookup sources for these sets.

The `schema` CLI and the C and WebAssembly theory-registry helpers resolve every descriptor through the shared registry by canonical name or alias. The generic document and source parser listings expose the 54 protocols that read schema documents.

## Registration behavior

Each semantic protocol module exposes `protocol()` and `register_theories()`. Most registrars call shared theory-group constructors. Those constructors panic if a named pushout fails, rewrite analysis cannot complete, two rewrite paths fail to rejoin (a non-joining critical pair), or the lexicographic-path-order termination check fails. Registration thus treats these outcomes as internal theory-definition defects, not recoverable input errors.

The ATProto registrar instead uses atomic registration for the schema and instance theories that it composes from five component theories. The registrar builds both compositions before mutating the destination registry. A failed composition returns `ProtocolError::TheoryRegistration` naming the stage and leaves the registry unchanged; callers propagate that error rather than proceeding with a missing composite. [What panproto verifies](../explanation/what-is-verified.md#schemas-theories-and-migrations) gives the boundary between these construction-time gates and checks on user schemas.

The source tree is the catalog authority:

| Contract | Source |
|---|---|
| Shared descriptors, canonical names, lookup, and theory registrars | [`crates/panproto-protocols/src/registry.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/registry.rs) |
| Protocol modules and parser dispatch | [`crates/panproto-protocols/src/lib.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/lib.rs) |
| Reusable theories and pushout helpers | [`crates/panproto-protocols/src/theories.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-protocols/src/theories.rs) |
| CLI theory lookup | [`crates/panproto-cli/src/cmd/helpers.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-cli/src/cmd/helpers.rs) |

## Source-code grammars

`panproto-grammars` defines 261 individual `lang-*` features under `group-all`. A selected feature contributes a tree-sitter `Language` and its vendored AST metadata to [`ParserRegistry`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html). The default `group-core` is a subset. Callers do not receive all grammars unless their selected feature set includes them.

[`emit_verification_status`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.emit_verification_status) reports test coverage for the registered parser, not a proof about all inputs:

| Status | Meaning |
|---|---|
| `Verified` | The protocol is in `VERIFIED_EMIT_PROTOCOLS`: 248 grammars pass the full vendored corpus oracle and seven more are covered by dedicated backend regressions. |
| `Generic` | A parser is registered and uses the generic emitter, but the protocol is outside that verified set. |
| `Unsupported` | No parser is registered under the supplied name. |

The verified set currently contains 255 names. [Source-code emission](../explanation/emit-pretty.md#the-verification-tier-api) identifies the seven backend cases and the six grammars outside the set.

## Defining a protocol

[Build a custom protocol](../how-to/build-protocol.md) covers theory declaration, registration, parsing, and emission. Adding a Rust module to `panproto-protocols` does not make the protocol available through the public lookup surfaces. Availability also requires a descriptor in the shared registry, which the CLI, C, and WebAssembly lookup surfaces resolve.

## See also

- [Schemas as theories](../explanation/schemas-as-theories.md)
- [Composing protocols by colimit](../explanation/protocol-colimits.md)
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md)
