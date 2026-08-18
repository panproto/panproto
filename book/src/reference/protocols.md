# Protocol catalog

A [*protocol*](../glossary.md#protocol "A protocol identifies a schema language and the theories and structural rules that define it.") in panproto is a schema language: [Avro](https://avro.apache.org/docs/current/specification/), [CDDL](https://www.rfc-editor.org/rfc/rfc8610.html), [OpenAPI](https://spec.openapis.org/oas/latest.html), [ATProto Lexicons](https://atproto.com/specs/lexicon), [Parquet](https://parquet.apache.org/docs/file-format/), [FHIR](https://hl7.org/fhir/), or a [Kubernetes CRD](https://kubernetes.io/docs/concepts/extend-kubernetes/api-extension/custom-resources/). Each one is defined by a [schema theory](../glossary.md#schema-theory "A schema theory specifies the structures and laws available to schemas for one protocol.") and an [instance theory](../glossary.md#instance-theory "An instance theory specifies how data may inhabit schemas for one protocol.") composed by [colimit](../glossary.md#colimit "A colimit combines theories over their explicitly shared parts.") from reusable [generalized algebraic theories (GATs)](../glossary.md#generalized-algebraic-theory-gat "A GAT is a named collection of dependent sorts, operations, and equations."). A protocol registration supplies a [parser](../glossary.md#parser "A parser reads a protocol's native schema representation into panproto's internal model.") from its native format to [`Schema`](../glossary.md#schema "A schema is a model of a protocol's schema theory.") and an [emitter](../glossary.md#emitter "An emitter writes panproto's internal schema model in a protocol's native representation.") in the reverse direction.

For the model behind these registrations, see [Schemas as theories](../explanation/schemas-as-theories.md) and [Composing protocols by colimit](../explanation/protocol-colimits.md).

## Categories

The built-in protocols are organized by category in `panproto-protocols`. Each category is a Rust submodule.

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
| Tree-sitter grammar list (261 languages) | [`crates/panproto-grammars/`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars) |

## Defining a new protocol

To add a custom protocol, see [Build a custom protocol](../how-to/build-protocol.md). The minimal recipe is: declare schema and instance GATs (Rust or via the [theory DSL](../how-to/build-protocol.md)), register a parser and emitter, and add a registration call to the relevant submodule.

## Source-code grammars and emit verification

In addition to the schema-language protocols cataloged above, panproto ships 261 tree-sitter grammars under [`crates/panproto-grammars/`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars). Each grammar registers a tree-sitter `Language` plus its `node-types.json` AST signature; the resulting parser walks source code into a full-AST schema, and `emit_pretty` renders the schema back to bytes via the structural pipeline described in [Source-code emission](../explanation/emit-pretty.md).

The emitter's correctness varies by grammar; [`ParserRegistry::emit_verification_status`](https://docs.rs/panproto-parse/latest/panproto_parse/struct.ParserRegistry.html#method.emit_verification_status) reports which tier each protocol falls into:

| Tier | Meaning | Currently |
|---|---|---|
| `Verified` | Every entry of the grammar author's own `test/corpus/` round-trips under the strict `emit_corpus_audit` oracle (byte fixed point plus vertex-kind and edge-shape multiset preservation), or the protocol is pinned by a quivers backend test | the 255 names in `VERIFIED_EMIT_PROTOCOLS` |
| `Generic` | Registered grammar; emit uses the generic dispatch + universal cassette path; no test asserts correctness | the remaining vendored grammars not yet in the verified set |
| `Unsupported` | No `grammar.json` vendored, or protocol not registered | grammars whose upstream did not ship `grammar.json` |

Downstream tooling (notably [quivers](https://github.com/aaronstevenwhite/quivers)'s transpile backends) should call this API upfront and refuse emit on protocols that return `Generic` or `Unsupported`. The full list of verified protocols is maintained as `VERIFIED_EMIT_PROTOCOLS` in [`crates/panproto-parse/src/registry.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-parse/src/registry.rs).

## See also

- [Source-code emission](../explanation/emit-pretty.md)
- [Schemas as theories](../explanation/schemas-as-theories.md)
- [Composing protocols by colimit](../explanation/protocol-colimits.md)
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md)
