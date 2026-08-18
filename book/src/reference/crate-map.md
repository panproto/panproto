# Crate map

The `panproto-*` crates in the workspace, with one-line descriptions and dependency direction. The full dependency graph appears in [explanation/architecture](../explanation/architecture.md). Source for any crate lives at `crates/<name>/` in the [repository](https://github.com/panproto/panproto/tree/main/crates).

## Core

| Crate | Description |
|---|---|
| `panproto-gat` | Generalized algebraic theories: sorts, operations, equations, and the colimit machinery that composes them. |
| `panproto-gat-macros` | `class!` and `inductive!` macros for declaring GATs in Rust source. |
| `panproto-schema` | Schema representation: vertices, edges, the schema graph, validation. Hosts the `AbstractSchema` / `DecoratedSchema` typed-newtype distinction, the layout-fiber forgetful U (`Schema::forget_layout`), `induce` (the one supported way to cut a well-formed sub-schema, accounting for all twenty-one `Schema` fields), and `canonical_digest`. |
| `panproto-inst` | Instances: data records over a schema, including `Value::List` and the typed value lattice. |
| `panproto-mig` | Migration engine: morphisms between schema theories, restrict and lift, plus the `solve` / `span` / `hom_search` subsystem that *finds* those morphisms by minimizing a cost function network. |
| `panproto-lens` | Bidirectional lenses: get/put/complement, the three round-trip laws, fibration over schemas, optic kinds, protolenses, the cross-crate `enrichment_registry` for layout and other schema-level fibers. |
| `panproto-dsl-eval` | Shared Nickel, JSON, and YAML evaluation for the lens and theory DSL crates. |
| `panproto-lens-dsl` | Declarative lens DSL with Nickel, JSON, and YAML surfaces. |
| `panproto-theory-dsl` | Declarative theory DSL for defining custom protocols. |
| `panproto-check` | Breaking-change detection: classifies a schema diff as fully compatible, backward compatible, or breaking via the `Classification` enum, alongside the `breaking`/`non_breaking` lists and `compatible` boolean on `CompatReport`. |
| `panproto-protocols` | Built-in protocol definitions composed via theory colimits. |
| `panproto-expr` | Pure, total expression language: terms, types, environment evaluation. |
| `panproto-expr-parser` | Haskell-style surface syntax parser for `panproto-expr`. |

One line does not locate the morphism search, which is the largest subsystem inside `panproto-mig`. [`panproto_mig::solve`](https://docs.rs/panproto-mig/latest/panproto_mig/solve/) poses the search for a schema morphism as a **cost function network** (CFN): one variable per source vertex, one value per kind-compatible target vertex, and a distinguished bottom value meaning that the vertex is left out of the result. Minimizing the total cost over that network is the search. Because bottom is an ordinary value rather than a failure, the maximum common sub-schema falls out as the network's optimum and no separate subgraph search is needed. [`panproto_mig::span`](https://docs.rs/panproto-mig/latest/panproto_mig/span/) assembles the winning assignment into a span, re-inducing and re-validating its apex, and [`panproto_mig::hom_search`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/) is the surface a caller reaches for. Four algorithms sit behind that surface: exact bucket elimination [@dechter1999bucket] on the primary path, hybrid best-first search [@allouchedegivrykatsirelosschiexzytnicki2015anytime] over branch and bound maintaining existential directional arc consistency [@degivryheraszytnickilarrosa2005existential] when the network is too wide to eliminate, that same search with a counting-based all-different propagator [@mccreeshprosser2015backjumping] added for an injective request, and McSplit [@mccreeshprossertrimble2017partitioning] for an isomorphism request.

## I/O and parsing

| Crate | Description |
|---|---|
| `panproto-io` | Parse/emit codecs that bridge native formats (JSON, Avro, Protobuf, ...) to `panproto-inst`. |
| `panproto-parse` | Tree-sitter full-AST parsing across 261 languages, with interstitial preservation. Hosts the put-direction (`decorate`) of the parse / emit lens and the `LayoutEnricher` adapter installed in `panproto-lens`'s enrichment registry. |
| `panproto-grammars` | Pre-compiled tree-sitter grammars used by `panproto-parse`. |
| `panproto-grammars-all` | Umbrella grammar companion pack for the Python wheel. |
| `panproto-grammars-functional` | Functional-language grammar pack. |
| `panproto-grammars-web` | Web-language grammar pack. |
| `panproto-grammars-systems` | Systems-language grammar pack. |
| `panproto-grammars-jvm` | JVM-language grammar pack. |
| `panproto-grammars-scripting` | Scripting-language grammar pack. |
| `panproto-grammars-data` | Data-language grammar pack. |
| `panproto-grammars-devops` | DevOps-language grammar pack. |
| `panproto-grammars-mobile` | Mobile-language grammar pack. |
| `panproto-grammars-music` | Music-language grammar pack (SuperCollider, LilyPond, ABC, Csound, ChucK, Glicol, Tidal, Strudel). |
| `panproto-project` | Multi-file project assembly via schema coproduct, manifest loading. |

## Version control

| Crate | Description |
|---|---|
| `panproto-vcs` | Schematic version control: DAG, refs, commit/branch/merge, pushout-based merge with a merge-time cocone check, data versioning. |
| `panproto-git` | Bidirectional bridge between `panproto-vcs` and git. |
| `panproto-git-remote` | Custom git remote helper for cloning panproto repositories over git. |
| `panproto-xrpc` | XRPC client for cospan-node VCS operations. |

## Bindings and surfaces

| Crate | Description |
|---|---|
| `panproto-core` | Public facade over thirteen always-on library crates: the nine core schema and migration crates plus `panproto-expr`, `panproto-expr-parser`, `panproto-lens-dsl`, and `panproto-theory-dsl`. |
| `panproto-cli` | The `schema` binary. |
| `panproto-wasm` | WebAssembly bindings; consumed by the TypeScript SDK. |
| `panproto-py` | Native Python bindings via PyO3. |
| `panproto-c` | C ABI for non-Rust language bindings; the Haskell and Swift bindings consume it. 105 entry points by default, 122 with the `full-parse`, `project`, and `git` features. |

The interactive REPL for theories, terms, and morphisms is part of `panproto-cli`, reachable as `schema theory repl`; it is not a separate crate.

## Repository tasks

| Crate | Description |
|---|---|
| `xtask` | Repository tasks (not published). Includes `gen-cli-docs`. |

## See also

- [Architecture](../explanation/architecture.md) for the dependency direction and layering.
- [Searching for a morphism](../explanation/morphism-search.md) for the cost function network the `panproto-mig` search subsystem is posed over, and [Alignment evidence](../explanation/alignment-evidence.md) for what feeds its anchor term.
- The repository [`Cargo.toml`](https://github.com/panproto/panproto/blob/main/Cargo.toml) for authoritative workspace membership.
