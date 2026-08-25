# Build a custom protocol

A custom protocol starts with a `Protocol` value and its schema and instance theories. Parsing, emission, migration, and language bindings are separate integration points. Implement only the surfaces the protocol will expose, and register each one explicitly.

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

`panproto-protocols` has no common `Parser` or `Emitter` trait. Protocol modules expose format-specific free functions. A JSON document parser usually has the shape `fn(&serde_json::Value) -> Result<Schema, ProtocolError>`; a text-language parser usually accepts `&str`. Add the parser to `parse_schema_document` or `parse_schema_source` in `crates/panproto-protocols/src/lib.rs`, and add its canonical name to the corresponding `document_parser_protocols` or `source_parser_protocols` list.

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

Pick the `theories::register_*` helper that matches the intended schema and instance shapes (`register_constrained_multigraph_wtype`, `register_typed_graph_wtype`, `register_hypergraph_functor`, and related helpers). A helper is appropriate only when its constructed theories match the names and operations declared by the protocol.

Registration is surface-specific:

1. Export the module from its category module in `panproto-protocols`.
2. Add document or source parsing to the dispatch functions in `panproto-protocols::lib` when the protocol has an external parser.
3. Add the protocol name and constructor to `builtin_protocol_names` and `lookup_builtin_protocol` in `crates/panproto-wasm/src/api/helpers.rs` for the TypeScript SDK. Add theory registration there as well if SDK operations need the theories; that registry currently has arms only for `atproto`, `json-schema`, `graphql`, `sql`, and `protobuf`.
4. Add the constructor to `crates/panproto-py/src/protocols.rs` for the Python SDK.
5. Add the constructor and theory registration to `resolve_protocol` and `build_theory_registry` in `crates/panproto-cli/src/cmd/helpers.rs` for CLI commands. Both tables currently contain only `atproto`.

Other bindings that expose a fixed built-in table need their own entry. A Rust caller can use the module's `protocol()` and `register_theories()` functions directly without a global lookup.

## Verification

```sh
cargo test -p panproto-protocols my_proto
```

Add tests under the new protocol module with names containing `my_proto`; the command above runs that subset with Cargo alone. Cover every implemented surface. That means theory registration and schema validation for a theory-backed protocol, parsing for an external parser, and a parse/emit round trip only when an emitter exists. Add a migration test when the protocol claims migration support.

## Common mistakes

- Declaring extensions before the colimit components are correct. Extensions interact with the colimit structure; if a building-block step is wrong, the extension may not even reach registration.
- Adding a module without updating its dispatch tables. The Rust module remains directly callable, but string-based SDK and CLI lookups will not find it.
- Assuming the CLI uses the same catalog as the language SDKs. Its resolver and theory registry are separate and currently recognize only `atproto`.
- Requiring an emitter for internal operations. `schema validate` reads panproto schema JSON directly; it does not call a protocol document parser or emitter.

## See also

- [Reference: protocol catalog](../reference/protocols.md).
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).
- [Composing protocols by colimit](../explanation/protocol-colimits.md).
