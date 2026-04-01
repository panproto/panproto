# panproto-io

[![crates.io](https://img.shields.io/crates/v/panproto-io.svg)](https://crates.io/crates/panproto-io)
[![docs.rs](https://docs.rs/panproto-io/badge.svg)](https://docs.rs/panproto-io)

Instance-level [presentation functors](https://ncatlab.org/nlab/show/functor) for panproto.

This crate provides parse/emit operations connecting raw format bytes to panproto's instance models (`WInstance`, `FInstance`, `GInstance`), completing the functorial data migration pipeline. Together with `panproto-protocols` (schema presentations), `panproto-mig` (migration compilation), and `panproto-inst` (restriction/lifting), this enables end-to-end format-to-format migration with mathematical compositionality guarantees. Since v0.24.0, the `tree-sitter` feature enables format-preserving round-trips through a unified CST-based codec, preserving whitespace, comments, and formatting across JSON, XML, YAML, TOML, CSV, and TSV.

## API

| Item | Description |
|------|-------------|
| `InstanceParser` | Trait for parsing raw bytes into instances |
| `InstanceEmitter` | Trait for emitting instances to raw bytes |
| `NativeRepr` | Which instance model a protocol uses (`WType`, `Functor`, `Either`) |
| `ProtocolRegistry` | Runtime dispatch by protocol name |
| `default_registry()` | Pre-built registry with all 76 protocol codecs |
| `JsonCodec` | Generic JSON codec via `simd-json` |
| `XmlCodec` | Generic zero-copy XML codec via `quick-xml` |
| `TabularCodec` | Generic delimited-text codec via `memchr` |
| `HtmlCodec` | HTML codec via `tl` |
| `MarkdownCodec` | Markdown codec via `pulldown-cmark` |
| `ConlluCodec` | CoNLL-U codec with sentence/token table extraction |
| `FormatPreservingCodec` | Trait for format-preserving parse/emit (behind `tree-sitter` feature) |
| `UnifiedCodec` | Tree-sitter-based codec for JSON, XML, YAML, TOML, CSV, TSV with format preservation |
| `CstComplement` | CST complement capturing full lossless tree for format-preserving round-trips |
| `FormatKind` | Enum dispatching across six data format grammars |
| `ParseInstanceError` / `EmitInstanceError` | Error types |

## Example

```rust,ignore
use panproto_io::default_registry;

let registry = default_registry();
let instance = registry.parse_wtype("html", &schema, &html_bytes)?;
let emitted = registry.emit_wtype("html", &schema, &instance)?;
```

## Protocol coverage (76 codecs)

| Category | Count | Pathway |
|----------|-------|---------|
| Annotation | 19 | JSON, XML, tabular |
| Web/Document | 10 | JSON, XML, `tl` (HTML), `pulldown-cmark` (Markdown) |
| Serialization | 8 | JSON (canonical encoding) |
| Type System | 8 | JSON |
| Data Schema | 7 | JSON, CSV, INI |
| Database | 6 | JSON, TSV |
| Domain | 6 | JSON, XML, delimited |
| API | 5 | JSON |
| Config | 4 | JSON |
| Data Science | 3 | JSON |

## License

[MIT](../../LICENSE)
