# Round-trip with format preservation

When you parse a JSON, YAML, TOML, XML, or CSV file and emit it back without changes, panproto can guarantee `emit(parse(bytes)) == bytes` byte-for-byte. This requires the format-preserving codec, which uses tree-sitter grammars and a CST complement to capture whitespace, comments, and ordering.

## Prerequisites

A panproto build with the `format-preserving` feature flag (default on for the CLI; opt-in for `panproto-core` in Rust).

## The task

The format-preserving round-trip is exposed via `schema parse emit`, which parses a file and emits it back in one step:

```sh
schema parse emit config.yaml > config.roundtripped.yaml
diff config.yaml config.roundtripped.yaml
```

The diff is empty when the codec preserves the input. (For programmatic use, parse and emit are exposed separately by the SDK; see below.)

In Rust:

```rust
use panproto_core::format_preserving::UnifiedCodec;

let codec = UnifiedCodec::for_format("yaml")?;
let (instance, complement) = codec.parse(bytes)?;
let out = codec.emit(&instance, &complement)?;
assert_eq!(out, bytes);
```

The complement carries the CST data that the schema does not see. `emit` reconstructs the byte-for-byte original from `(instance, complement)`.

## Verification

The byte equality is the verification. Property tests in CI check `emit(parse(b)) == b` against a corpus of real-world JSON, YAML, TOML, XML, and CSV files.

## Common mistakes

- Modifying the instance without modifying the complement. If you edit a value in the instance, the complement still records the *old* whitespace around the *old* value; the round-trip will preserve the old layout around the new value.
- Mixing `format-preserving` codec output with non-preserving codec input. The two pipelines are separate; choose one consistently.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Convert data between formats](./convert-data.md).
- [Parse full ASTs](./parse-full-ast.md) for tree-sitter parsing of source code.
