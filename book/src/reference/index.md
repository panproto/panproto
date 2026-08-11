# Reference

Reference pages are authoritative lookups: signatures, flags, tables, and grammars. They contain no exposition. If you want to understand the model behind these signatures, follow the per-page link to the corresponding [explanation](../explanation/index.md) chapter.

| Page | What it lists |
|---|---|
| [CLI](./cli.md) | Every `schema` subcommand, with usage, options, and examples. Generated from the clap derive tree; never stale. |
| [Rust SDK](./sdk-rust.md) | Crate selection, feature flags, public re-exports, and the canonical entry points. Links into docs.rs for full type signatures. |
| [TypeScript SDK](./sdk-typescript.md) | Package layout, initialization, the high-level facade, and the handle-based API. Links into TypeDoc. |
| [Python SDK](./sdk-python.md) | PyO3 native module surface, top-level re-exports, and the companion grammar packs. Links into the mkdocs reference site. |
| [Haskell SDK](./sdk-haskell.md) | The capability typeclasses, the two backends, the effect layer, and the standard-class integration. |
| [Swift SDK](./sdk-swift.md) | Product tiers, the engine actor and its thread confinement, the handle taxonomy, the error hierarchy, and the CBOR codec. |
| [Protocol catalogue](./protocols.md) | Every protocol panproto recognises, with its theory composition and supported operations. |
| [Expression language](./expression-language.md) | Surface grammar, builtins by category, and the type signatures of each builtin. |
| [Lens combinators](./lens-combinators.md) | The combinator algebra exposed by `panproto-lens`, organised by optic kind. |
| [Configuration](./configuration.md) | The `panproto.toml` manifest schema. |
| [Crate map](./crate-map.md) | The 38 `panproto-*` crates in the workspace, with one-line descriptions and dependency direction. |
