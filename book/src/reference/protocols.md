# Protocol catalog

A [protocol](../glossary.md#protocol) names a schema language together with its schema and instance theories, structural rules, parser, and emitter. The semantic protocols in `panproto-protocols` compose reusable generalized algebraic theories (GATs) [@cartmell1986generalised]. Source-code languages use the separate tree-sitter registry described below.

## Semantic protocols

The generic dispatch functions accept 54 semantic protocols. Names in this table are the canonical hyphenated strings accepted by [`parse_schema_document`](https://docs.rs/panproto-protocols/latest/panproto_protocols/fn.parse_schema_document.html) or [`parse_schema_source`](https://docs.rs/panproto-protocols/latest/panproto_protocols/fn.parse_schema_source.html). Underscore spellings are normalized before dispatch.

| Category module | Protocol names |
|---|---|
| `annotation` | `amr`, `bead`, `brat`, `concrete`, `conllu`, `decomp`, `elan`, `folia`, `fovea`, `iso-space`, `laf-graf`, `naf`, `nif`, `paula`, `tei`, `timeml`, `ucca`, `uima-cas`, `web-annotation` |
| `api` | `asyncapi`, `graphql`, `jsonapi`, `openapi`, `raml` |
| `config` | `ansible`, `cloudformation`, `k8s-crd` |
| `data_schema` | `bson`, `cddl`, `json-schema` |
| `data_science` | `arrow`, `dataframe`, `parquet` |
| `database` | `cassandra`, `dynamodb`, `mongodb`, `neo4j`, `redis`, `sql` |
| `domain` | `edi-x12`, `fhir`, `geojson`, `rss-atom`, `swift-mt`, `vcard-ical` |
| `serialization` | `asn1`, `avro`, `bond`, `flatbuffers`, `msgpack-schema`, `protobuf` |
| `web_document` | `atproto`, `docx`, `odf` |

The `raw_file` module is the text-or-binary fallback used during project assembly. It is a protocol implementation, but is not listed by either generic schema-dispatch function.

### Parser dispatch

| Entry point | Accepted input | Registered protocols |
|---|---|---|
| `parse_schema_document` | `serde_json::Value` | 43 protocols |
| `parse_schema_source` | text IDL, DDL, or annotation source | 11 protocols |
| `parse_schema_bundle` | JSON document bundle | `atproto` only |
| `parse_schema_bundle_project` | path and JSON pairs with per-file provenance | `atproto` only |

The exported `document_parser_protocols`, `source_parser_protocols`, `bundle_parser_protocols`, and `bundle_project_protocols` arrays are the lookup sources for these sets.

Protocol availability also depends on the surface. The current `schema` CLI resolves protocol theories only for `atproto`. The C and WebAssembly theory-registry helpers recognize `atproto`, `json-schema`, `graphql`, `sql`, and `protobuf`, while their protocol lookup tables expose the 54 names above.

## Registration behavior

Each semantic protocol module exposes `protocol()` and `register_theories()`. Most registrars call shared theory-group constructors. Those constructors panic if a named pushout fails, rewrite analysis cannot complete, two rewrite paths fail to rejoin (a non-joining critical pair), or the lexicographic-path-order termination check fails. Registration thus treats these outcomes as internal theory-definition defects, not recoverable input errors.

The ATProto registrar is different. It inserts its five component theories first, then inserts each composed schema or instance theory only when the corresponding `pushout_by_name` call succeeds. Its `register_theories()` function has no `Result`, so a failed composition is omitted rather than returned to the caller. [What panproto verifies](../explanation/what-is-verified.md#schemas-theories-and-migrations) gives the boundary between these construction-time gates and checks on user schemas.

The source tree is the catalog authority:

| Contract | Source |
|---|---|
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

[Build a custom protocol](../how-to/build-protocol.md) covers theory declaration, registration, parsing, and emission. Adding a Rust module to `panproto-protocols` does not automatically extend the CLI, C, or WebAssembly lookup matches. Each exposed surface needs its own dispatch arm.

## See also

- [Schemas as theories](../explanation/schemas-as-theories.md)
- [Composing protocols by colimit](../explanation/protocol-colimits.md)
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md)
