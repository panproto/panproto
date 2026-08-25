# panproto-parse

[![crates.io](https://img.shields.io/crates/v/panproto-parse.svg)](https://crates.io/crates/panproto-parse)
[![docs.rs](https://docs.rs/panproto-parse/badge.svg)](https://docs.rs/panproto-parse)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Full-AST source parsing and schema emission over bundled tree-sitter grammars.

## Grammar selection

`panproto-grammars` vendors 261 grammar feature entries. `panproto-parse` does not
enable all of them by default. Its default `group-core` feature enables Python,
JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, and Rust. Enable `group-all`,
another group, or individual `lang-*` features to select a different set. A
`ParserRegistry` contains only parsers compiled into that build.

[Tree-sitter](https://www.thestrangeloop.com/2018/tree-sitter---a-new-parsing-system-for-programming-tools.html)
produces concrete syntax trees. Panproto's generic walker turns named nodes and fields
into a schema and records byte positions and interstitial text as constraints.
`AstParser::emit` replays that recorded layout. Exact replay thus depends on
keeping the parse-produced schema and its layout constraints intact.

`emit_pretty` instead derives a canonical rendering from vendored `grammar.json` data
and the generic cassette layer. `emit_verification_status` distinguishes protocols
with dedicated corpus or backend tests (`Verified`), registered protocols using only
the generic path (`Generic`), and protocols that are unavailable or lack the needed
grammar data (`Unsupported`). The current verified allowlist has 255 names. This is a
test-coverage classification, not a proof for arbitrary schemas.

## Example

```rust,ignore
use panproto_parse::ParserRegistry;
use std::path::Path;

let registry = ParserRegistry::new();
let bytes = std::fs::read("src/main.rs")?;
let schema = registry.parse_file(Path::new("src/main.rs"), &bytes)?;
let replayed = registry.emit_with_protocol("rust", &schema)?;
```

## Public API

| Item | Purpose |
|------|---------|
| `ParserRegistry` | Register, detect, parse, emit, and query enabled languages |
| `AstParser` | Interface implemented by a language parser |
| `AstWalker`, `WalkerConfig` | Generic CST-to-schema traversal |
| `extract_theory_from_node_types` | Derive finite theory metadata from `node-types.json` |
| `ParseEmitLens` | Package parse and canonical emit for one protocol |
| `check_emit_parse`, `check_parse_emit` | Run structural law checks on concrete inputs |
| `LayoutPolicy`, `decorate_with_parser` | Configure and synthesize layout constraints |
| `EmitVerificationStatus` | `Verified`, `Generic`, or `Unsupported` |

The parse/emit law checkers compare documented structural witnesses after removing
layout-only constraints. They do not return a generic `Lens<bytes, Schema>` value.

## License

[MIT](../../LICENSE)
