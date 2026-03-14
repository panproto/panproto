# panproto

A universal schema migration engine built on [Generalized Algebraic Theories](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (GATs).

panproto treats every schema language — [ATProto Lexicons](https://atproto.com/specs/lexicon), SQL DDL, [Protocol Buffers](https://protobuf.dev/), [GraphQL](https://graphql.org/), [JSON Schema](https://json-schema.org/) — as a model of a common mathematical structure. Migrations become [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories), and correctness guarantees (existence conditions, [lens laws](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29), breaking-change detection) are derived from the algebra rather than hardcoded per format.

## Key idea

A **protocol** is a pair of GATs: one describing the shape of schemas, one describing the shape of instances. Adding support for a new schema language means defining two new theories — no engine code changes required.

```
Level 0  GAT engine (sorts, operations, equations, morphisms, colimits)
Level 1  Theory specifications as data (ThATProtoSchema, ThWType, ThFunctor, …)
Level 2  Concrete schemas as models of a schema theory
Level 3  Concrete instances as models of schemas
```

## Workspace

| Crate | Description |
|-------|-------------|
| `panproto-gat` | GAT engine: sorts, operations, equations, theory morphisms, colimits |
| `panproto-schema` | Schema representation with protocol-aware builder and adjacency indices |
| `panproto-inst` | [W-type](https://ncatlab.org/nlab/show/W-type) and set-valued functor instances with restrict/extend pipelines |
| `panproto-mig` | Migration engine: theory-derived existence checks, compilation, lift, compose, invert |
| `panproto-lens` | Bidirectional lenses with [Cambria](https://www.inkandswitch.com/cambria/)-style combinators and law verification |
| `panproto-check` | Breaking change detection via structural diffing and protocol-aware classification |
| `panproto-protocols` | Built-in protocol definitions (76 protocols including ATProto, SQL, Protobuf, GraphQL, JSON Schema) |
| `panproto-io` | Instance-level parse/emit codecs across all protocols (JSON, XML, tabular, web documents) |
| `panproto-vcs` | Schematic version control: content-addressed object store, commit DAG, pushout-based merge |
| `panproto-core` | Re-export facade |
| `panproto-wasm` | [WASM](https://webassembly.org/) bindings with handle-based slab allocator and [MessagePack](https://msgpack.org/) boundary |
| `panproto-cli` | CLI (`schema`): validate, check, diff, lift, and git-style version control |

### TypeScript SDK

The `@panproto/core` package in `sdk/typescript/` provides a high-level TypeScript API over the WASM module, with async initialization, fluent schema builders, and `Symbol.dispose` resource management.

## Quick start

### Rust

```rust
use panproto_core::*;

// Build a schema using the ATProto protocol
let proto = panproto_protocols::atproto::protocol();
let schema = schema::SchemaBuilder::new(&proto)
    .vertex("post", "record", Some("app.bsky.feed.post"))?
    .vertex("post:body", "object", None)?
    .vertex("post:body.text", "string", None)?
    .edge("post", "post:body", "record-schema", None)?
    .edge("post:body", "post:body.text", "prop", Some("text"))?
    .constraint("post:body.text", "maxLength", "3000")
    .build()?;

assert_eq!(schema.vertex_count(), 3);
assert_eq!(schema.edge_count(), 2);
```

### TypeScript

```typescript
import { Panproto } from '@panproto/core';

const p = await Panproto.init();
const proto = p.protocol('atproto');

const schema = proto.schema()
  .vertex('post', 'record', 'app.bsky.feed.post')
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', 'text')
  .constraint('post:body.text', 'maxLength', '3000')
  .build();
```

### CLI

```sh
# Validate a schema against a protocol
schema validate --protocol atproto schema.json

# Detect breaking changes between two schema versions
schema check --protocol atproto old.json new.json

# Diff two schemas
schema diff old.json new.json

# Apply a migration to a record
schema lift --protocol atproto --migration mig.json \
  --src-schema old.json --tgt-schema new.json record.json

# Version control
schema init
schema add schema.json
schema commit -m "initial schema"
schema branch feature
schema checkout feature
schema merge main
schema log
```

## Building

```sh
# Rust
cargo build --workspace
cargo nextest run --workspace

# WASM
wasm-pack build crates/panproto-wasm --target web

# TypeScript SDK
cd sdk/typescript && pnpm install && pnpm build && pnpm test
```

## Architecture

panproto implements a three-level architecture rooted in category theory:

[**W-type**](https://ncatlab.org/nlab/show/W-type) **instances** (tree-structured data like JSON/ATProto records) use a 5-step restrict pipeline: anchor surviving nodes, compute reachability from root, contract ancestors, resolve edges, and reconstruct fans.

**[Set-valued functor](https://ncatlab.org/nlab/show/functor) instances** (relational data like SQL tables) use [precomposition](https://ncatlab.org/nlab/show/precomposition) (&#916;<sub>F</sub>) for restrict and [left Kan extension](https://ncatlab.org/nlab/show/Kan+extension) (&#931;<sub>F</sub>) for extend.

**[Bidirectional lenses](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29)** provide `get` (restrict + complement capture) and `put` (restore from complement) directions, with six [Cambria](https://www.inkandswitch.com/cambria/)-style combinators: `RenameField`, `AddField`, `RemoveField`, `WrapInObject`, `HoistField`, `CoerceType`. The `GetPut` and `PutGet` laws are verified at test time.

**Schematic version control** (`panproto-vcs`) provides git-style operations — commit, branch, merge, rebase, cherry-pick, bisect, blame — operating on schema graphs rather than text. Merges are computed as categorical pushouts with typed conflict detection across all schema fields. There is no heuristic tie-breaking; the merge is commutative.

**Theory-derived existence conditions** determine migration validity by inspecting the schema and instance [theory](https://ncatlab.org/nlab/show/generalized+algebraic+theory) sorts at runtime, rather than hardcoding checks per protocol.

## License

[MIT](LICENSE)
