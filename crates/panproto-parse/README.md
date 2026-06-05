# panproto-parse

[![crates.io](https://img.shields.io/crates/v/panproto-parse.svg)](https://crates.io/crates/panproto-parse)
[![docs.rs](https://docs.rs/panproto-parse/badge.svg)](https://docs.rs/panproto-parse)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Parses source code in 259 programming languages into panproto schema graphs using tree-sitter grammars.

## What it does

Tree-sitter parses source code into an abstract syntax tree (AST): a tree of named node types (`function_definition`, `class_declaration`, `import_statement`) connected by named fields (`name`, `body`, `parameters`). Panproto converts this AST structure into a schema graph where each node type becomes a vertex kind and each field name becomes an edge kind. The schema graph represents the full structure of the source file as panproto data.

The theory for each language (the formal description of what the schema graph for that language looks like) is extracted automatically from the grammar's `node-types.json` file. Because the theory is always derived from the grammar itself, it stays in sync automatically as grammars are updated. One `AstWalker` implementation handles all 261 languages; there is no per-language parsing code.

Alongside each schema vertex, the walker records interstitial text: the keywords, punctuation, and whitespace that appear between named AST children. The emitter collects these fragments by byte position and concatenates them to reproduce the original source exactly. `emit(parse(source)) == source` for any file the grammar can parse.

For schemas that were built by hand (without an originating CST), the `AstParser::emit_pretty` method renders source bytes by walking the grammar's production rules from `grammar.json`. Per-language implementations currently ship for JSON, TOML, Rust, Python, and Go; YAML is pending. Languages without a custom implementation return `ParseError::EmitFailed` from the default trait method.

The `parse_emit_lens` module exposes the parse/emit pipeline as an asymmetric lens with checkable laws. `ParseEmitLens` packages a single language's parse and emit into a `Lens<bytes, schema>`; `check_emit_parse` and `check_parse_emit` verify the EmitParse retraction (`parse(emit(s)) ≅ s` modulo byte positions) and ParseEmit stability (`emit(parse(b)) == b` for parseable bytes) on concrete inputs. Structural equivalence is witnessed by `kind_multiset` (vertex-kind multiset) and `edge_multiset` (over `(src_kind, edge_kind, tgt_kind)` triples); `strip_complement` removes byte-position constraints while preserving the discriminators that drove non-deterministic choices. `first_divergence` reports the first key where two schemas differ for diagnostic output.

## Quick example

```rust,ignore
use panproto_parse::registry;

// All 261 languages are registered automatically with the default feature set.
let reg = registry::global();

// Parse a Rust source file into a schema graph.
let schema = reg.parse_file("src/main.rs")?;

// Emit the schema back to source code.
let source = reg.emit_file("src/main.rs", &schema)?;
assert_eq!(source, std::fs::read("src/main.rs")?);

// Extract the theory for the Rust language.
let parser = reg.get("rust").unwrap();
let theory_meta = parser.theory_meta();
```

## API overview

| Export | What it does |
| ------ | ------------ |
| `ParserRegistry` | Holds all language parsers; dispatches by protocol name or file extension |
| `registry::global()` | Returns the global registry populated from `panproto-grammars` |
| `AstParser` | Trait for a single-language parser and emitter (implement to add a language); `emit_pretty` renders by-construction schemas from `grammar.json` production rules |
| `ParseEmitLens` | Parse/emit packaged as an asymmetric lens for a single language |
| `check_emit_parse`, `check_parse_emit` | Verify the lens's two laws on concrete inputs |
| `kind_multiset`, `edge_multiset` | Structural-equivalence witnesses used by `check_emit_parse` |
| `strip_complement` | Remove byte-position constraints while preserving choice discriminators |
| `AstWalker` | Generic tree-sitter walker that works for all languages |
| `WalkerConfig` | Per-language customization: scope hints, formatting constraints |
| `extract_theory_from_node_types` | Derive a panproto theory from a grammar's `node-types.json` |
| `ExtractedTheoryMeta` | The derived theory plus sort counts and field statistics |
| `IdGenerator` | Scope-aware vertex ID generation for full-AST schemas |
| `ParseError` | Error type for parse and emit failures |

## Theory extraction mapping

| `node-types.json` concept | panproto GAT concept |
| ------------------------- | -------------------- |
| Named node type | Sort (vertex kind) |
| Required field | Mandatory operation (edge kind) |
| Optional field | Partial operation |
| Multiple field | Ordered operation |
| Supertype | Abstract sort with subtype inclusions |

## License

[MIT](../../LICENSE)
