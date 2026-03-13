# Changelog

All notable changes to panproto will be documented in this file.

## [Unreleased]

### Features

- **panproto-io** (NEW): Instance-level presentation functors for all 77 protocols, completing the functorial data migration pipeline
  - SIMD JSON pathway via `simd-json` (2-4x over `serde_json`)
  - Zero-copy XML pathway via `quick-xml` pull parser
  - SIMD tabular pathway via `memchr` for delimited formats (CoNLL-U, CSV, EDI, SWIFT MT)
  - SIMD HTML codec via `tl`
  - Markdown codec via `pulldown-cmark`
  - Dedicated CoNLL-U codec with sentence/token table extraction
  - `ProtocolRegistry` for runtime dispatch by protocol name
  - `default_registry()` entry point with all 77 codecs pre-registered
  - Arena allocation helpers (`bumpalo`) for zero-copy hot paths
- **panproto-protocols**: Expand protocol coverage to 77 formats (54 base + 19 annotation + 4 new: SWIFT MT, Docker Compose, 2 additional) with bidirectional schema-level parse/emit
  - **Serialization** (7): Avro, Thrift, Cap'n Proto, FlatBuffers, ASN.1, Bond, MsgPack
  - **Data Schema** (7): XML/XSD, CSV/Table Schema, YAML Schema, TOML Schema, CDDL, INI, BSON
  - **API** (4): OpenAPI, AsyncAPI, RAML, JSON:API
  - **Database** (5): MongoDB, Cassandra, DynamoDB, Neo4j, Redis
  - **Type System** (8): TypeScript, Python, Rust, Java, Go, Swift, Kotlin, C#
  - **Web/Document** (8): HTML, CSS, DOCX, ODF, Markdown, JSX, Vue, Svelte
  - **Data Science** (3): Parquet, Arrow, DataFrame
  - **Domain** (5): GeoJSON, FHIR, RSS/Atom, vCard/iCal, EDI X12
  - **Config** (3): HCL, K8s CRD, Docker Compose
  - **Annotation** (19): AMR, bead, brat, Concrete, CoNLL-U, Decomp/UDS, ELAN, FoLiA, FOVEA, ISO-Space, LAF/GrAF, NAF, NIF, PAULA, TEI, TimeML, UCCA, UIMA/CAS, W3C Web Annotation
- **panproto-protocols**: Shared emit helpers (`find_roots`, `children_by_edge`, `vertex_constraints`, `IndentWriter`) and 5 theory group registration functions
- **panproto-core**: Re-exports `panproto-io` as `panproto::io`
- **panproto-python**: Python 3.13+ SDK with strict typing, Pydantic v2 models, and 170 tests

### Documentation

- Tutorial book (Quarto) covering schemas, GATs, protocols, migration, and lenses
- Developer guide (Quarto) covering contribution workflow, architecture, and crate internals
  - Chapter 5: Updated crate hierarchy (11 crates, 6 levels) with `panproto-io` at Level 3.5, updated dependency graph, migration lifecycle sequence diagram, and "What Lives Where" table
  - Chapter 8: Updated instance lifecycle to show `panproto-io` as the format-specific entry point alongside generic `parse_json`
  - Chapter 12: Rewritten parser/emitter convention as two-level presentation architecture (schema presentations in `panproto-protocols`, instance presentations in `panproto-io`); updated "Adding a New Protocol" guide with Step 4b for instance codecs
  - Appendix B: Added `panproto-io` source code map with all 26 source files
- Per-crate README files with linked technical concepts
- Project README and MIT license

### Fixes

- Fix Mermaid diagram newlines in dev-guide (literal `\n` → `<br>`)
- Add version specs to workspace crate dependencies for crates.io publishing
- Add MPL-2.0 to `deny.toml` license allow list

### Testing

- 76 round-trip integration tests for `panproto-io`, one per registered protocol
- Fixture data from public sources: UD English EWT (CC BY-SA), Wikipedia HTML (CC BY-SA), Rust README (MIT), Natural Earth GeoJSON (public domain), HL7 FHIR R4 (CC0), NASA RSS (rssboard.org), AWS CloudFormation (MIT), K8s Gateway API CRD (Apache-2.0), JSON Schema Test Suite (MIT)

### Stats

- 694 tests across the workspace (up from 212 in v0.1.0; 98 in panproto-io)

## [0.1.0] - 2026-03-12

### Features

- **panproto-gat**: Generalized Algebraic Theory engine with sorts, operations, equations, theories, theory morphisms, colimits (pushouts), and model migration
- **panproto-schema**: Schema representation with precomputed adjacency indices, protocol-aware builder with validation, and ref-chain normalization
- **panproto-inst**: W-type and set-valued functor instance representations with 5-step `wtype_restrict` pipeline, `functor_restrict` (precomposition), and `functor_extend` (left Kan extension)
- **panproto-mig**: Migration engine with theory-derived existence checking, compilation, `lift_wtype`/`lift_functor`, composition, and inversion
- **panproto-lens**: Bidirectional lens combinators (RenameField, AddField, RemoveField, WrapInObject, HoistField, CoerceType) with complement tracking and GetPut/PutGet law verification
- **panproto-check**: Breaking change detection via structural schema diffing and protocol-aware classification with human-readable and JSON reports
- **panproto-protocols**: Built-in protocol definitions for ATProto, SQL, Protobuf, GraphQL, and JSON Schema with parsers for each format
- **panproto-core**: Re-export facade for all sub-crates
- **panproto-wasm**: 10 wasm-bindgen entry points with handle-based slab allocator and MessagePack serialization boundary
- **panproto-cli**: Command-line interface with `validate`, `check`, `diff`, and `lift` subcommands
- **@panproto/core**: TypeScript SDK with async WASM initialization, fluent schema builder, migration API, and lens combinators
- 212 tests across the workspace including 59 integration tests covering self-description, ATProto round-trips, SQL migrations, cross-protocol colimits, lens laws, and performance benchmarks
