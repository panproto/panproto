# Build a custom protocol

A custom protocol is needed when panproto must parse, emit, compare, and migrate a schema language outside the built-in catalog. The implementation consists of a GAT, a parser/emitter pair, and registrations for the required surfaces.

## Prerequisites

Familiarity with [Schemas as theories](../explanation/schemas-as-theories.md) and [Composing protocols by colimit](../explanation/protocol-colimits.md). The Rust toolchain.

## The task

### Declare the theory

The fastest path is the theory DSL ([`panproto-theory-dsl`](https://github.com/panproto/panproto/tree/main/crates/panproto-theory-dsl)):

```nickel
let T = import "panproto/theory.ncl" in
{
  id = "dev.example.my-proto-schema",
  description = "A directed graph whose edges carry permissions",
  theory = "ThMyProtoSchema",
  sorts = [
    T.simple "Vertex",
    T.simple "Edge",
    T.val_sort "Permission" "string",
  ],
  ops = [
    T.unary "src" "Edge" "Vertex",
    T.unary "tgt" "Edge" "Vertex",
    T.unary "perm_of" "Edge" "Permission",
  ],
  equations = [],
} | T.Theory
```

This document declares the schema-side theory directly. Give every document a stable `id` and `description`; the `theory` field supplies the name used by registrations and later compositions. Use a `compose` document when the schema theory should instead be a colimit of existing theories.

For finer control, declare the theory directly in Rust with the `class!` and `inductive!` macros from [`panproto-gat-macros`](https://github.com/panproto/panproto/tree/main/crates/panproto-gat-macros).

From Python, the same DSL document loads via `Theory.from_nickel(source)`, `Theory.from_yaml(source)`, `Theory.from_json(source)`, or `Theory.from_path(path)` (dispatches by extension). The loaders accept the `theory`, `class`, and `inductive` body variants; multi-body documents (morphism, composition, protocol, bundle) belong in `panproto-theory-dsl::load_and_compile` directly. For incremental authoring, `panproto.TheoryBuilder` mirrors `class!` in a chainable form. Round-trip the flat Theory shape via `to_json` / `to_yaml` paired with `from_dict_json` / `from_dict_yaml`.

### Implement parser and emitter

Each protocol provides a `Parser: Bytes -> Schema` and an `Emitter: Schema -> Bytes`. Implement both in a new submodule of `panproto-protocols`. See existing modules (`serialization::avro`, `data_schema::cddl`, `web_document::atproto`) for canonical structure.

### Register

Each protocol module exposes `protocol() -> Protocol` and `register_theories(&mut HashMap<String, Theory, _>)`. The internal module skeleton below is repository code, not a standalone Rust program:

```text
// crates/panproto-protocols/src/my_proto.rs
use std::collections::HashMap;
use panproto_gat::Theory;
use panproto_schema::Protocol;

use crate::theories;

pub fn protocol() -> Protocol {
    Protocol {
        name: "my_proto".into(),
        schema_theory: "ThMyProtoSchema".into(),
        instance_theory: "ThMyProtoInstance".into(),
        ..Protocol::default()
    }
}

pub fn register_theories<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThMyProtoSchema",
        "ThMyProtoInstance",
    );
}
```

Pick the `theories::register_*` helper that matches your theory shape (`register_constrained_multigraph_wtype`, `register_typed_graph_wtype`, `register_hypergraph_functor`, ...). Colimit failures during registration panic with a named intermediate step; that is a build-time bug to fix in the theory composition. To expose `my_proto` to the language SDKs and CLI, add it to the `lookup` table in `crates/panproto-py/src/protocols.rs` and the `lookup_builtin_protocol` table in `crates/panproto-wasm/src/api/helpers.rs`.

## Verification

```sh
cargo nextest run -p panproto-protocols -E 'test(/my_proto/)'
```

Add tests under the new protocol module with names containing `my_proto`; the command above runs that subset. At minimum, cover theory registration, schema validation, and a parse/emit round trip. Add a migration test when the protocol claims migration support.

## Common mistakes

- Declaring extensions before the colimit components are correct. Extensions interact with the colimit structure; if a building-block step is wrong, the extension may not even reach registration.
- Skipping the parser/emitter. Without both, the protocol cannot participate in `schema convert` or `schema validate` workflows.

## See also

- [Reference: protocol catalog](../reference/protocols.md).
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).
- [Composing protocols by colimit](../explanation/protocol-colimits.md).
