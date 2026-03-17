# panproto

[![CI](https://github.com/panproto/panproto/actions/workflows/ci.yml/badge.svg)](https://github.com/panproto/panproto/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/panproto-core.svg)](https://crates.io/crates/panproto-core)
[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![docs.rs](https://docs.rs/panproto-core/badge.svg)](https://docs.rs/panproto-core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A universal schema migration engine built on [Generalized Algebraic Theories](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (GATs).

panproto treats every schema language,[ATProto Lexicons](https://atproto.com/specs/lexicon), SQL DDL, [Protocol Buffers](https://protobuf.dev/), [GraphQL](https://graphql.org/), [JSON Schema](https://json-schema.org/), and [71 others](https://panproto.dev/tutorial/appendices/D-protocol-catalog.html),as a model of a common mathematical structure. Migrations become [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories), and correctness guarantees (existence conditions, [lens laws](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29), breaking-change detection) are derived from the algebra rather than hardcoded per format.

## Key idea

A **protocol** is a pair of GATs: one describing the shape of schemas, one describing the shape of instances. Adding support for a new schema language means defining two new theories. No engine code changes required.

```
Level 0  GAT engine (sorts, operations, equations, morphisms, colimits)
Level 1  Theory specifications as data (ThATProtoSchema, ThWType, ThFunctor, …)
Level 2  Concrete schemas as models of a schema theory
Level 3  Concrete instances as models of schemas
```

## Workspace

| Crate | Description |
|-------|-------------|
| [`panproto-gat`](crates/panproto-gat) | GAT engine: sorts, operations, equations, theory morphisms, colimits |
| [`panproto-schema`](crates/panproto-schema) | Schema representation with protocol-aware builder and adjacency indices |
| [`panproto-inst`](crates/panproto-inst) | [W-type](https://ncatlab.org/nlab/show/W-type), set-valued functor, and graph instances with restrict/extend/Kan extension pipelines |
| [`panproto-mig`](crates/panproto-mig) | Migration engine: existence checks, compilation, lift, compose, invert, automatic morphism discovery |
| [`panproto-lens`](crates/panproto-lens) | Bidirectional lenses with [Cambria](https://www.inkandswitch.com/cambria/)-style combinators and law verification |
| [`panproto-check`](crates/panproto-check) | Breaking change detection via structural diffing and protocol-aware classification |
| [`panproto-protocols`](crates/panproto-protocols) | 76 built-in protocol definitions composed from 27 building-block theories |
| [`panproto-io`](crates/panproto-io) | Instance-level parse/emit codecs (JSON, XML, tabular, web documents) |
| [`panproto-vcs`](crates/panproto-vcs) | Schematic version control: content-addressed store, commit DAG, pushout-based merge |
| [`panproto-core`](crates/panproto-core) | Re-export facade |
| [`panproto-wasm`](crates/panproto-wasm) | WASM bindings with handle-based slab allocator and MessagePack boundary |
| [`panproto-cli`](crates/panproto-cli) | CLI (`schema`): validate, check, diff, lift, and git-style version control |

### SDKs

| Package | Install |
|---------|---------|
| [`@panproto/core`](sdk/typescript) | `npm install @panproto/core` |
| [`panproto`](sdk/python) | `pip install panproto` |

## Quick start

### Rust

```rust
use panproto_core::*;

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
  .vertex('post', 'record', { nsid: 'app.bsky.feed.post' })
  .vertex('post:body', 'object')
  .vertex('post:body.text', 'string')
  .edge('post', 'post:body', 'record-schema')
  .edge('post:body', 'post:body.text', 'prop', { name: 'text' })
  .constraint('post:body.text', 'maxLength', '3000')
  .build();
```

### Python

```python
import panproto

pp = panproto.Panproto.load()
proto = pp.protocol("atproto")

schema = (proto.schema()
    .vertex("post", "record", nsid="app.bsky.feed.post")
    .vertex("post:body", "object")
    .vertex("post:body.text", "string")
    .edge("post", "post:body", "record-schema")
    .edge("post:body", "post:body.text", "prop", name="text")
    .constraint("post:body.text", "maxLength", "3000")
    .build())
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
cd sdk/typescript && pnpm install && pnpm build

# Python SDK
cd sdk/python && pip install -e .
```

## Architecture

panproto implements a four-level architecture rooted in category theory. The GAT engine (Level 0) is the only component implemented directly in Rust. Everything above it (protocols, schemas, instances) is data interpreted by the engine.

[**W-type**](https://ncatlab.org/nlab/show/W-type) **instances** (tree-structured data like JSON/ATProto records) use a 5-step restrict pipeline: anchor surviving nodes, compute reachability from root, contract ancestors, resolve edges, and reconstruct fans.

**[Set-valued functor](https://ncatlab.org/nlab/show/functor) instances** (relational data like SQL tables) use [precomposition](https://ncatlab.org/nlab/show/precomposition) (&#916;<sub>F</sub>) for restrict and [left Kan extension](https://ncatlab.org/nlab/show/Kan+extension) (&#931;<sub>F</sub>) for extend. Right Kan extension (&#928;<sub>F</sub>) computes products over fibers.

**[Bidirectional lenses](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29)** provide `get` (restrict + complement capture) and `put` (restore from complement) directions, with seven [Cambria](https://www.inkandswitch.com/cambria/)-style combinators. The `GetPut` and `PutGet` laws are verified at test time.

**Schematic version control** (`panproto-vcs`) provides git-style operations (commit, branch, merge, rebase, cherry-pick, bisect, blame) operating on schema graphs rather than text. Merges are computed as categorical pushouts. There is no heuristic tie-breaking; the merge is commutative.

**Automatic migration discovery** finds schema morphisms via backtracking CSP with MRV heuristic, discovers overlaps between schemas, and computes schema-level pushouts for merging disparate formats.

## Documentation

- [Tutorial](https://panproto.dev/tutorial/): 17-chapter guide from first schema to automatic migration
- [Dev Guide](https://panproto.dev/dev-guide/): internals, algorithms, and architecture
- [API Reference (docs.rs)](https://docs.rs/panproto-core)
- Interactive Playground (coming soon): runs entirely in your browser via WebAssembly

## License

[MIT](LICENSE)
