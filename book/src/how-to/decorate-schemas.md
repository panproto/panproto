# Decorate an abstract schema

Decoration renders an [`AbstractSchema`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.AbstractSchema.html) with a grammar and parses the result again. The returned `DecoratedSchema` carries the layout constraints needed for replay emission.

## Prerequisites

The Rust SDK with the `full-parse` feature. The requested grammar must be compiled into `panproto-parse`, and the abstract schema's `protocol` field must equal the grammar name.

## Obtain an abstract schema

`SchemaBuilder::build_abstract()` is the constructor for a hand-built layout-free schema. It rejects constraints in the layout fiber, such as `start-byte`, `end-byte`, `interstitial-N`, and `chose-alt-*`.

The following example begins with parsed JSON only to obtain a compact, valid grammar-shaped schema. A hand-built `AbstractSchema` for the same protocol can be substituted directly.

```rust,no_run
use panproto_core::parse::{LayoutPolicy, ParserRegistry};
use panproto_core::schema::DecoratedSchema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ParserRegistry::new();
    let parsed = registry.parse_with_protocol(
        "json",
        br#"{"k": 1}"#,
        "input.json",
    )?;
    let abstract_schema = DecoratedSchema::wrap_unchecked(parsed).forget_layout();

    let decorated = registry.decorate(
        "json",
        &abstract_schema,
        &LayoutPolicy::default(),
    )?;
    let bytes = registry.emit_pretty_with_protocol(
        "json",
        decorated.as_schema(),
    )?;

    println!("{}", String::from_utf8(bytes)?);
    Ok(())
}
```

`decorate` uses the supplied policy to produce canonical bytes, then parses those bytes to recover byte spans, interstitial text, and grammar-choice constraints. The parse step assigns fresh vertex identifiers.

## Render without decoration

If only canonical bytes are needed, call `pretty_with_protocol` and skip the reparsing step:

```rust,no_run
use panproto_core::parse::{LayoutPolicy, ParserRegistry};
use panproto_core::schema::AbstractSchema;

fn render(
    registry: &ParserRegistry,
    schema: &AbstractSchema,
) -> Result<Vec<u8>, panproto_core::parse::ParseError> {
    let policy = LayoutPolicy {
        indent_width: 4,
        newline: "\r\n".into(),
        ..LayoutPolicy::default()
    };
    registry.pretty_with_protocol(schema.protocol(), schema, &policy)
}
```

`LayoutPolicy` is an alias of `FormatPolicy`. Its fields are `indent_width`, `separator`, `newline`, `line_break_after`, `indent_open`, and `indent_close`.

## Verify the result

Decoration preserves the abstract structure at the granularity tested by the library: after `forget_layout`, the vertex-kind and edge-shape multisets should match the input. It does not preserve vertex identifiers.

```rust,no_run
use panproto_core::schema::{edge_multiset, kind_multiset};
# use panproto_core::schema::{AbstractSchema, DecoratedSchema};
# fn check(input: &AbstractSchema, output: &DecoratedSchema) {
let round_trip = output.forget_layout();
assert_eq!(
    kind_multiset(input.as_schema()),
    kind_multiset(round_trip.as_schema()),
);
assert_eq!(
    edge_multiset(input.as_schema()),
    edge_multiset(round_trip.as_schema()),
);
# }
```

Run the focused integration test with the grammar features used by the test:

```sh
cargo test -p panproto-parse --test decorate_section_law \
  --features lang-json,lang-lilypond
```

`ParserRegistry::emit_verification_status(protocol)` reports `Verified`, `Generic`, or `Unsupported`. `Verified` means the repository has an explicit fixed-point or round-trip test for that protocol. `Generic` means the grammar-walker path is available but lacks a protocol-specific verification claim. `Unsupported` means the parser is missing or lacks the grammar data required for pretty emission.

## Limitations

- A schema built for `atproto` cannot be decorated with the `json`, `rust`, or `lilypond` parser. Protocol mismatch returns `ParseError::SchemaConstruction`.
- `build_decorated()` and `DecoratedSchema::wrap_unchecked()` do not validate that a complete layout fiber is present. Reserve them for data produced by parsing or a trusted decoration path.
- The section law concerns abstract structure, not byte equality with an earlier source file. A new `LayoutPolicy` may produce different canonical bytes.

## See also

- [Parse full ASTs](./parse-full-ast.md) for parsing source into a decorated schema.
- [Source-code emission](../explanation/emit-pretty.md) for the emitter.
- [Layout enrichment](../explanation/layout-enrichment.md) for the model.
