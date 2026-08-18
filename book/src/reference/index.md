# Reference

Use these pages to look up a name, flag, signature, field, [protocol](../glossary.md#protocol "A protocol identifies a schema language and the theories and structural rules that define it."), or grammar. Procedures live in the [how-to guides](../how-to/index.md), and the mathematical model lives in [explanation](../explanation/index.md).

## Operational contracts

| Page | Contract |
|---|---|
| [CLI](./cli.md) | Every `schema` subcommand and its generated `--help` text. |
| [Configuration](./configuration.md) | Fields and defaults in `panproto.toml`. |
| [Protocol catalog](./protocols.md) | Registered protocols, their module categories, and emit support. |

## SDK contracts

| Surface | Contract |
|---|---|
| [Rust](./sdk-rust.md) | Crate selection, feature flags, re-exports, and morphism-search types. |
| [TypeScript](./sdk-typescript.md) | Package initialization, facade objects, and the handle boundary. |
| [Python](./sdk-python.md) | Native-module exports, type stubs, and companion grammar packs. |
| [Haskell](./sdk-haskell.md) | Capability classes, backends, effects, and Cabal flags. |
| [Swift](./sdk-swift.md) | Products, engine isolation, handles, errors, and feature gates. |

## Intermediate lookup

The [expression-language reference](./expression-language.md) lists the surface grammar, types, builtins, and errors used by queries and field transforms. The [lens-combinator reference](./lens-combinators.md) lists optic kinds, constructor families, complement composition, and protolens instantiation.

## Advanced lookup

The [crate map](./crate-map.md) lists the 36 `panproto-*` workspace crates and their dependency roles. Use it when the facade does not expose the level at which an extension or integration must be implemented.
