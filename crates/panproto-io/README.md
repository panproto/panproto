# panproto-io

Instance-level [presentation functors](https://ncatlab.org/nlab/show/functor) for panproto.

This crate provides SIMD-accelerated parse/emit operations connecting raw format bytes to panproto's instance models ([`WInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/struct.WInstance.html) and [`FInstance`](https://docs.rs/panproto-inst/latest/panproto_inst/struct.FInstance.html)), completing the [functorial data migration](https://ncatlab.org/nlab/show/data+migration) pipeline. Together with `panproto-protocols` (schema presentations), `panproto-mig` (migration compilation), and `panproto-inst` (restriction/lifting), this enables end-to-end format-to-format migration with mathematical compositionality guarantees.

## API

| Item | Description |
|------|-------------|
| `InstanceParser` | Trait for parsing raw bytes into `WInstance`/`FInstance` |
| `InstanceEmitter` | Trait for emitting `WInstance`/`FInstance` to raw bytes |
| `NativeRepr` | Which instance model a protocol uses (`WType`, `Functor`, `Either`) |
| `ProtocolRegistry` | Runtime dispatch by protocol name |
| `default_registry()` | Pre-built registry with all 76 protocol codecs |
| `JsonCodec` | Generic SIMD JSON codec via [`simd-json`](https://crates.io/crates/simd-json) |
| `XmlCodec` | Generic zero-copy XML codec via [`quick-xml`](https://crates.io/crates/quick-xml) |
| `TabularCodec` | Generic delimited-text codec via [`memchr`](https://crates.io/crates/memchr) |
| `HtmlCodec` | SIMD HTML codec via [`tl`](https://crates.io/crates/tl) |
| `MarkdownCodec` | Markdown codec via [`pulldown-cmark`](https://crates.io/crates/pulldown-cmark) |
| `ConlluCodec` | CoNLL-U codec with sentence/token table extraction |
| `ParseInstanceError` / `EmitInstanceError` | Error types |

## Example

```rust,ignore
use panproto_io::default_registry;

let registry = default_registry();

// Parse raw HTML into a WInstance
let instance = registry.parse_wtype("html", &schema, &html_bytes)?;

// Emit back to raw bytes
let emitted = registry.emit_wtype("html", &schema, &instance)?;

// Parse CoNLL-U into an FInstance (tabular)
let conllu = registry.parse_functor("conllu", &schema, &conllu_bytes)?;
```

## Protocol Coverage (76 codecs)

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
