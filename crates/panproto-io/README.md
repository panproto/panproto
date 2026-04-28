# panproto-io

[![crates.io](https://img.shields.io/crates/v/panproto-io.svg)](https://crates.io/crates/panproto-io)
[![docs.rs](https://docs.rs/panproto-io/badge.svg)](https://docs.rs/panproto-io)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Reads and writes data in each protocol's native format: 50 protocols, multiple codecs, with optional format-preserving round-trips via tree-sitter.

## What it does

Panproto's migration pipeline works on abstract instances: structured data detached from any particular file format. This crate is the bridge between raw bytes and those abstract instances. It parses JSON into an OpenAPI instance, CoNLL-U text into a dependency annotation instance, or an Avro binary into an Avro instance. After migration, it emits the result back to bytes in the target format.

The `ProtocolRegistry` holds one parser and one emitter for each registered protocol. Calling `parse_wtype("openapi", &schema, &bytes)` dispatches to the right codec automatically. The `default_registry()` function returns a registry with all 50 protocols registered. Most codecs use SIMD-accelerated JSON parsing (via `simd-json`) or zero-copy XML parsing (via `quick-xml`) to keep parsing off the critical path.

With the `tree-sitter` feature enabled, the `UnifiedCodec` provides format-preserving round-trips for JSON, XML, YAML, TOML, CSV, and TSV: `emit(parse(bytes)) == bytes` exactly, including whitespace, comments, and original key ordering. This works by storing a CST complement alongside the abstract instance and using it during emission.

`UnifiedCodec::new` and the per-format constructors (`json`, `xml`, `yaml`, `toml`, `csv`, `tsv`) return `Result<Self, UnifiedCodecError>`: construction can fail with `MissingGrammar` (the requested grammar was not compiled into `panproto-grammars`) or `ParserInit` (tree-sitter rejected the grammar's language version). Use `ProtocolRegistry::try_register` to surface those errors when wiring a registry.

## Quick example

```rust,ignore
use panproto_io::default_registry;

let registry = default_registry();

// Parse an OpenAPI document into an abstract instance.
let instance = registry.parse_wtype("openapi", &schema, &openapi_bytes)?;

// Emit it back to bytes.
let output = registry.emit_wtype("openapi", &schema, &instance)?;
```

## API overview

| Export | What it does |
|--------|-------------|
| `default_registry()` | Build a `ProtocolRegistry` with all 50 protocols registered |
| `ProtocolRegistry` | Dispatches parse and emit by protocol name |
| `InstanceParser` | Trait for parsing raw bytes into a `WInstance` or `FInstance` |
| `InstanceEmitter` | Trait for emitting an instance back to raw bytes |
| `NativeRepr` | Which instance model a protocol uses (`WType`, `Functor`, `Either`) |
| `ParseInstanceError` | Error type for parse failures |
| `EmitInstanceError` | Error type for emit failures |
| `UnifiedCodec` | Format-preserving codec for JSON, XML, YAML, TOML, CSV, TSV (requires `tree-sitter` feature); constructors return `Result<Self, UnifiedCodecError>` |
| `UnifiedCodecError` | Error type for codec construction: `MissingGrammar`, `ParserInit` |
| `ProtocolRegistry::try_register` | Register a codec whose construction is fallible, surfacing `UnifiedCodecError` |
| `cst_extract` | CST-to-instance extraction lens for format-preserving round-trips |

## Protocol coverage

| Category | Protocols | Formats |
|----------|-----------|---------|
| Annotation | brat, decomp, ucca, fovea, bead, web_annotation, naf, uima, folia, tei, timeml, elan, iso_space, paula, laf_graf, conllu, amr, concrete, nif | JSON, XML, tabular |
| Web / Document | atproto, docx, odf | JSON, XML |
| Serialization | avro, flatbuffers, asn1, bond, msgpack_schema | JSON (canonical) |
| Database | mongodb, dynamodb, cassandra, neo4j, redis | JSON |
| Config | cloudformation, ansible, k8s_crd | JSON |
| Data science | dataframe, parquet, arrow | JSON |
| Domain | geojson, fhir, rss_atom, vcard_ical, swift_mt, edi_x12 | JSON, XML, delimited |
| API | openapi, asyncapi, jsonapi, raml | JSON |
| Data schema | cddl, bson | JSON |

## License

[MIT](../../LICENSE)
