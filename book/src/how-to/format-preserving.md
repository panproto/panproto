# Round-trip with format preservation

When you parse a JSON, YAML, TOML, XML, or CSV file and emit it back without changes, panproto can guarantee `emit(parse(bytes)) == bytes` byte-for-byte. This requires the format-preserving codec, which uses tree-sitter grammars and a CST complement to capture whitespace, comments, and ordering.

This page covers the structured-data path (JSON / YAML / TOML / XML / CSV). For source-code grammars (Rust, Python, Stan, BUGS, and 257 more), the emit path is `emit_pretty`, which derives spacing and dispatch from `grammar.json` structurally and exercises a per-protocol verification tier. See [Parse full ASTs](./parse-full-ast.md) and [Source-code emission](../explanation/emit-pretty.md). The two systems are independent and serve different formats; choose one based on what you are parsing.

## Prerequisites

Format preservation is gated behind the `tree-sitter` feature flag on `panproto-core` (or directly on `panproto-io`). The shipped `schema` binary does not enable this feature, so its round-trips are not byte-for-byte: a format-preserving parse or emit requested from the default binary returns canonical output with no layout complement and prints a one-line notice to stderr. Byte preservation requires a build that turns the feature on, for instance a tool built against `panproto-core` with `features = ["tree-sitter"]`, or a direct dependency on `panproto-io` with the same feature. The snippets below assume such a build.

## The task

The format-preserving round-trip is exposed by the codec API, not by the shipped CLI. `parse_wtype_preserving` returns the instance together with a complement carrying the CST data the schema does not see, and `emit_wtype_preserving` reconstructs the byte-for-byte original from the pair.

In Rust:

```rust,no_run
use panproto_core::io::unified_codec::UnifiedCodec;
# use panproto_core::schema::{Schema, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let proto = panproto_core::protocols::atproto::protocol();
# let schema: Schema = SchemaBuilder::new(&proto).vertex("root", "object", None)?.entry("root").build()?;
# let bytes: &[u8] = b"";
let codec = UnifiedCodec::yaml("atproto")?;
let (instance, complement) = codec.parse_wtype_preserving(&schema, bytes)?;
let out = codec.emit_wtype_preserving(&schema, &instance, &complement)?;
assert_eq!(out, bytes);
# Ok(()) }
```

The complement carries the CST data that the schema does not see. `emit_wtype_preserving` reconstructs the byte-for-byte original from `(instance, complement)`. Constructors exist for each supported format: `UnifiedCodec::json`, `xml`, `yaml`, `toml`, `csv`, and `tsv`.

Without the `tree-sitter` feature these constructors are compiled out. For code that must run in either build, the registry exposes `parse_wtype_preserving_or_canonical` and `emit_wtype_preserving_or_canonical`, which delegate to the preserving codecs when the feature is present and otherwise return canonical output with a stderr notice, so a caller learns the build cannot honor the request rather than silently receiving reformatted data.

## Verification

The byte equality is the verification. Property tests in CI check `emit(parse(b)) == b` against a corpus of real-world JSON, YAML, TOML, XML, and CSV files.

## Common mistakes

- Modifying the instance without modifying the complement. If you edit a value in the instance, the complement still records the *old* whitespace around the *old* value; the round-trip will preserve the old layout around the new value.
- Mixing `format-preserving` codec output with non-preserving codec input. The two pipelines are separate; choose one consistently.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Convert data between formats](./convert-data.md).
- [Parse full ASTs](./parse-full-ast.md) for tree-sitter parsing of source code.
