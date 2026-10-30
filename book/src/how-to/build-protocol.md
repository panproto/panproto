# Build a custom protocol

A custom protocol starts with a `Protocol` value and its schema and instance theories. Parsing, emission, and format-specific codecs are separate integration points. A protocol need only implement the surfaces it exposes; one registry entry records its shared capabilities.

## Prerequisites

Familiarity with [Schemas as theories](../explanation/schemas-as-theories.md) and [Composing protocols by colimit](../explanation/protocol-colimits.md). The Rust toolchain.

## The task

### Declare the theory

The theory DSL provides one authoring path:

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

### Implement external-format boundaries

`panproto-protocols` has no common `Emitter` trait. Protocol modules expose format-specific free functions. A JSON document parser usually has the shape `fn(&serde_json::Value) -> Result<Schema, ProtocolError>`; a text-language parser usually accepts `&str`. Record that function as `Parser::Document` or `Parser::Source` in the protocol's shared descriptor. Use `Parser::Programmatic` when schemas for the protocol are constructed in code rather than read from an external schema language. The parser dispatch and parser listings derive from this descriptor.

Emitters are also format-specific functions in their protocol modules. Add one only when the protocol has a defined external representation and the application needs to produce it. Internal schema validation and migration operate on `Schema` values and do not require an external-format emitter.

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
        name: "my-proto".into(),
        schema_theory: "ThMyProtoSchema".into(),
        instance_theory: "ThMyProtoInstance".into(),
        ..Default::default()
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

Pick the `theories::register_*` helper that matches the intended schema and instance shapes (`register_constrained_multigraph_wtype`, `register_typed_graph_wtype`, `register_hypergraph_functor`, and related helpers). A helper is appropriate only when its constructed theories match the names and operations declared by the protocol.

Registration has two shared steps:

1. Export the module from its category module in `panproto-protocols`.
2. Add one `ProtocolDescriptor` to `crates/panproto-protocols/src/registry.rs`. Give it the canonical name, any aliases, its `Parser` variant, the `protocol` constructor, a fallible theory-registration function, and its bundle capabilities.

The CLI, C, WebAssembly, and TypeScript protocol listings and lookups derive from this registry. The semantic-protocol portion of the Python listing and lookup does as well; Python then appends the enabled source-language grammars. The CLI, C, and WebAssembly theory registries use the same descriptors. A failed theory registration must leave no partial protocol-specific entry; this matters for registrations such as ATProto that build theories by pushout. Format-specific record codecs and emitters remain separate integration points and need their own dispatch entries when a binding exposes them. A Rust caller may also use the module's `protocol()` and `register_theories()` functions directly.

## Verification

```sh
cargo test -p panproto-protocols my_proto
```

Add tests under the new protocol module with names containing `my_proto`; the command above runs that subset with Cargo alone. Cover every implemented surface. That means theory registration and schema validation for a theory-backed protocol, parsing for an external parser, and a parse/emit round trip only when an emitter exists. Add a migration test when the protocol claims migration support.

## Common mistakes

- Declaring extensions before the colimit components are correct. Extensions interact with the colimit structure; if a building-block step is wrong, the extension may not even reach registration.
- Adding a module without a shared descriptor. The Rust module remains directly callable, but string-based SDK, binding, and CLI lookups will not find it.
- Assuming that a shared descriptor supplies a format-specific record codec or emitter. It supplies protocol and theory lookup plus schema-parser dispatch; other boundary operations still require their own integration.
- Requiring an emitter for internal operations. `schema validate` reads panproto schema JSON directly; it does not call a protocol document parser or emitter.

## See also

- [Reference: protocol catalog](../reference/protocols.md).
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).
- [Composing protocols by colimit](../explanation/protocol-colimits.md).
