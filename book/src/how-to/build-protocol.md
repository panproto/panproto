# Build a custom protocol

A custom protocol is a new GAT plus a parser/emitter pair, registered with panproto so it can participate in the same diff/migrate/version-control workflow as the built-ins.

## Prerequisites

Familiarity with [Schemas as theories](../explanation/schemas-as-theories.md) and [Composing protocols by colimit](../explanation/protocol-colimits.md). The Rust toolchain.

## The task

### Declare the theory

The fastest path is the theory DSL ([`panproto-theory-dsl`](https://github.com/panproto/panproto/tree/main/crates/panproto-theory-dsl)):

```nickel
{
  name = "MyProto",
  components = ["ThGraph", "ThConstraint", "ThNamed"],
  extensions = {
    sorts = ["Permission"],
    ops = [
      { name = "perm_of", in = ["Edge"], out = "Permission" },
    ],
    eqns = [],
  },
}
```

`components` lists the building-block theories to compose by colimit. `extensions` adds sorts, operations, and equations on top of the colimit.

For finer control, declare the theory directly in Rust with the `class!` and `inductive!` macros from [`panproto-gat-macros`](https://github.com/panproto/panproto/tree/main/crates/panproto-gat-macros).

From Python, the same DSL document loads via `Theory.from_nickel(source)`, `Theory.from_yaml(source)`, `Theory.from_json(source)`, or `Theory.from_path(path)` (dispatches by extension). The loaders accept the `theory`, `class`, and `inductive` body variants; multi-body documents (morphism, composition, protocol, bundle) belong in `panproto-theory-dsl::load_and_compile` directly. For incremental authoring, `panproto.TheoryBuilder` mirrors `class!` in a chainable form. Round-trip the flat Theory shape via `to_json` / `to_yaml` paired with `from_dict_json` / `from_dict_yaml`.

### Implement parser and emitter

Each protocol provides a `Parser: Bytes -> Schema` and an `Emitter: Schema -> Bytes`. Implement both in a new submodule of `panproto-protocols`. See existing modules (`serialization::avro`, `data_schema::json_schema`) for canonical structure.

### Register

```rust
// crates/panproto-protocols/src/lib.rs
pub mod my_proto;

pub fn register_all(registry: &mut Registry) {
    serialization::register(registry);
    data_schema::register(registry);
    my_proto::register(registry);   // <- new
}
```

`my_proto::register` calls into `register_constrained_multigraph_wtype` (or another helper that reflects your theory shape). Colimit failures panic with a named intermediate step; this is a build-time bug to fix in the theory composition.

## Verification

```sh
cargo test -p panproto-protocols my_proto
```

The standard property-test suite for protocols verifies parse/emit round-trip, schema validation against the theory, and migration existence between two scaffolded schemas.

## Common mistakes

- Declaring extensions before the colimit components are correct. Extensions interact with the colimit structure; if a building-block step is wrong, the extension may not even reach registration.
- Skipping the parser/emitter. Without both, the protocol cannot participate in `schema convert` or `schema validate` workflows.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).
- [Composing protocols by colimit](../explanation/protocol-colimits.md).
