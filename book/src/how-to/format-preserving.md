# Round-trip with format preservation

When you parse a JSON, YAML, TOML, XML, or CSV file and emit it back without changes, panproto can guarantee `emit(parse(bytes)) == bytes` byte-for-byte. This requires the format-preserving codec, which uses tree-sitter grammars and a CST complement to capture whitespace, comments, and ordering.

## Prerequisites

A panproto build with the `tree-sitter` feature flag enabled on `panproto-core` (or directly on `panproto-io`). The CLI ships with tree-sitter enabled by default.

## The task

The format-preserving round-trip is exposed via `schema parse emit`, which parses a file and emits it back in one step:

```sh
schema parse emit config.yaml > config.roundtripped.yaml
diff config.yaml config.roundtripped.yaml
```

The diff is empty when the codec preserves the input. (For programmatic use, parse and emit are exposed separately by the SDK; see below.)

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

## Verification

The byte equality is the verification. Property tests in CI check `emit(parse(b)) == b` against a corpus of real-world JSON, YAML, TOML, XML, and CSV files.

## Common mistakes

- Modifying the instance without modifying the complement. If you edit a value in the instance, the complement still records the *old* whitespace around the *old* value; the round-trip will preserve the old layout around the new value.
- Mixing `format-preserving` codec output with non-preserving codec input. The two pipelines are separate; choose one consistently.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Convert data between formats](./convert-data.md).
- [Parse full ASTs](./parse-full-ast.md) for tree-sitter parsing of source code.
