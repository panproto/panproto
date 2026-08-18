# Decorate an abstract schema

`decorate` prepares a hand-built schema for source-code emission. It attaches byte spans, inter-token text, and grammar-choice constraints to an abstract schema, producing a `DecoratedSchema` for the emitter; [Layout enrichment](../explanation/layout-enrichment.md) develops the model behind this operation.

## Prerequisites

The Rust SDK with the `full-parse` feature, or the CLI. Python bindings are forthcoming.

## The task

### Build the abstract schema

```rust,no_run
use panproto_core::schema::{Protocol, SchemaBuilder};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let p: Protocol = panproto_core::protocols::atproto::protocol();
let abstract_schema = SchemaBuilder::new(&p)
    .vertex("$0", "record", None)?
    .vertex("$1", "object", None)?
    .edge("$0", "$1", "record-schema", None)?
    .vertex("$2", "string", None)?
    .edge("$1", "$2", "prop", Some("title"))?
    .constraint("$2", "literal-value", "hello")
    .build_abstract()?;
# Ok(()) }
```

`build_abstract` checks that no layout-fiber constraint was added during construction (no `start-byte`, no `interstitial-N`, no `chose-alt-*`) and returns an `AbstractSchema`. If a layout sort slipped in, you get `SchemaError::LayoutConstraintsOnAbstractBuild`; use `build_decorated` if a decorated schema was the intent.

### Decorate

```rust,no_run
use panproto_core::parse::{LayoutPolicy, ParserRegistry};
# use panproto_core::schema::{Protocol, SchemaBuilder};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let p: Protocol = panproto_core::protocols::atproto::protocol();
# let abstract_schema = SchemaBuilder::new(&p).vertex("$0", "record", None)?.entry("$0").build_abstract()?;
let reg = ParserRegistry::new();
let policy = LayoutPolicy::default();
let decorated = reg.decorate("lilypond", &abstract_schema, &policy)?;
# let _ = decorated;
# Ok(()) }
```

`decorate` runs `emit_pretty_with_policy` to render the abstract schema to canonical bytes under the policy, then re-parses those bytes. The re-parse attaches the complete layout fiber: every `start-byte`, every `end-byte`, every `interstitial-N`, plus the `chose-alt-fingerprint` and `chose-alt-child-kinds` discriminators that pin down which CHOICE alternative the parser took.

### Render straight to bytes

If all you want is the rendered source, skip the re-parse:

```rust,no_run
# use panproto_core::parse::{LayoutPolicy, ParserRegistry};
# use panproto_core::schema::{Protocol, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let p: Protocol = panproto_core::protocols::atproto::protocol();
# let abstract_schema = SchemaBuilder::new(&p).vertex("$0", "record", None)?.entry("$0").build_abstract()?;
# let reg = ParserRegistry::new();
# let policy = LayoutPolicy::default();
let bytes = reg.pretty_with_protocol("lilypond", &abstract_schema, &policy)?;
# let _ = bytes;
# Ok(()) }
```

`pretty_with_protocol` honors every field of the policy in the output: `separator`, `newline`, `indent_width`, `line_break_after`, and the indent open/close token sets. Two different policies render the same abstract schema to different bytes.

### Customize the policy

```rust,no_run
use panproto_core::parse::LayoutPolicy;
# use panproto_core::parse::ParserRegistry;
# use panproto_core::schema::{Protocol, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let p: Protocol = panproto_core::protocols::atproto::protocol();
# let abstract_schema = SchemaBuilder::new(&p).vertex("$0", "record", None)?.entry("$0").build_abstract()?;
# let reg = ParserRegistry::new();
let policy = LayoutPolicy {
    indent_width: 4,
    separator: "  ".into(),
    newline: "\r\n".into(),
    ..LayoutPolicy::default()
};
let bytes = reg.pretty_with_protocol("lilypond", &abstract_schema, &policy)?;
# let _ = bytes;
# Ok(()) }
```

`LayoutPolicy` is an alias for the de-novo emitter's `FormatPolicy`; the put direction of the parse / emit lens and the emitter use the same configuration type.

### Strip layout back down

```rust,no_run
# use panproto_core::parse::{LayoutPolicy, ParserRegistry};
# use panproto_core::schema::{Protocol, SchemaBuilder};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let p: Protocol = panproto_core::protocols::atproto::protocol();
# let abstract_schema = SchemaBuilder::new(&p).vertex("$0", "record", None)?.entry("$0").build_abstract()?;
# let reg = ParserRegistry::new();
# let policy = LayoutPolicy::default();
# let decorated = reg.decorate("lilypond", &abstract_schema, &policy)?;
let stripped = decorated.forget_layout();   // -> AbstractSchema
# let _ = stripped;
# Ok(()) }
```

`forget_layout` drops every constraint whose sort is in the layout fiber (per `panproto_gat::is_layout_sort`) and returns an `AbstractSchema`. Decorating an abstract schema and then forgetting its layout returns a schema isomorphic to the original up to vertex identifier renaming and kind/edge-multiset equivalence, which is the granularity panproto's round-trip law machinery uses.

## Verification

The section-law smoke test in `crates/panproto-parse/tests/decorate_section_law.rs` parses a sample, forgets its layout, decorates it, and forgets the new layout for every grammar with a parse fixture. It then compares the vertex-kind and edge-shape multisets. Run it with:

```sh
cargo nextest run -p panproto-parse --test decorate_section_law \
    --features lang-json,lang-lilypond
```

The matching test for policy fidelity (`pretty_with_protocol_honours_policy`) renders the same abstract schema under two distinct policies and asserts the output bytes differ in exactly the way the policy prescribes (CRLF vs LF newline, two-space vs single-space separator, four-space vs zero indent).

## Common mistakes

- Wrapping a parsed schema as an `AbstractSchema` and expecting `decorate` to keep its vertex IDs. The parse walker invents fresh IDs; the section law holds up to multiset equivalence, not pointwise. If you need the parse-side IDs preserved, work with the `DecoratedSchema` directly.
- Passing an `AbstractSchema` built against one protocol into `decorate` for a different protocol. The protocol-match guard rejects the call with `ParseError::SchemaConstruction`; build the schema against the right protocol or look up the parser by the schema's own `protocol()`.
- Reaching for `DecoratedSchema::wrap_unchecked` on a hand-built schema and expecting `emit_pretty_with_protocol` to round-trip through byte-position arithmetic. The wrapping is a type-level assertion the constructor cannot verify; an empty layout fiber means the emitter falls back to a grammar walk, which is well-defined but uses the default `FormatPolicy`, not whatever policy you'd have passed to `decorate`.
- Calling `decorate` on a protocol that returns `EmitVerificationStatus::Generic`. `decorate` runs `emit_pretty` internally, so its output inherits whatever fidelity the emitter has on that grammar. Check `ParserRegistry::emit_verification_status(protocol)` first; if the result is `Generic`, the round-trip kind multiset still satisfies the section law, but byte-for-byte stability across re-emits is not guaranteed.

## See also

- [Source-code emission](../explanation/emit-pretty.md) for what `emit_pretty` does internally during the decorate call.
- [Parse full ASTs](./parse-full-ast.md) for the get direction of the same lens.
- [Round-trip with format preservation](./format-preserving.md) for the parallel codec story at the byte-position level.
- [Reference: protocol catalog](../reference/protocols.md) for the registered grammars.
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md) for the lens machinery the layout fiber rides on.
