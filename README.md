# panproto

[![CI](https://github.com/panproto/panproto/actions/workflows/ci.yml/badge.svg)](https://github.com/panproto/panproto/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/panproto-core.svg)](https://crates.io/crates/panproto-core)
[![npm](https://img.shields.io/npm/v/@panproto/core)](https://www.npmjs.com/package/@panproto/core)
[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![docs.rs](https://docs.rs/panproto-core/badge.svg)](https://docs.rs/panproto-core)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A universal schema migration engine built on [Generalized Algebraic Theories](https://ncatlab.org/nlab/show/generalized+algebraic+theory) (GATs), with automatic lens generation via [protolenses](https://ncatlab.org/nlab/show/natural+transformation).

panproto treats every schema language,[ATProto Lexicons](https://atproto.com/specs/lexicon), SQL DDL, [Protocol Buffers](https://protobuf.dev/), [GraphQL](https://graphql.org/), [JSON Schema](https://json-schema.org/), and [71 others](https://panproto.dev/tutorial/appendices/D-protocol-catalog.html),as a model of a common mathematical structure. Migrations become [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories), and correctness guarantees (existence conditions, [lens laws](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29), breaking-change detection) are derived from the algebra rather than hardcoded per format. Protolenses automatically derive schema-parameterized families of lenses, eliminating manual combinator wiring.

## Key idea

A **protocol** is a pair of GATs: one describing the shape of schemas, one describing the shape of instances. Adding support for a new schema language means defining two new theories. No engine code changes required.

```
Level 0  GAT engine (sorts, operations, equations, morphisms, colimits, endofunctors)
Level 1  Theory specifications as data (ThATProtoSchema, ThWType, ThFunctor, …)
Level 2  Concrete schemas as models of a schema theory
Level 3  Concrete instances as models of schemas
Level 4  Protolenses: schema-parameterized lens families via natural transformations
```

## Workspace

| Crate | Description |
|-------|-------------|
| [`panproto-gat`](crates/panproto-gat) | GAT engine: sorts, operations, equations, theory morphisms, colimits, schema endofunctors, and factorization |
| [`panproto-schema`](crates/panproto-schema) | Schema representation with protocol-aware builder and adjacency indices |
| [`panproto-inst`](crates/panproto-inst) | [W-type](https://ncatlab.org/nlab/show/W-type), set-valued functor, and graph instances with restrict/extend/Kan extension pipelines |
| [`panproto-mig`](crates/panproto-mig) | Migration engine: existence checks, compilation, lift, compose, invert, automatic morphism discovery |
| [`panproto-lens`](crates/panproto-lens) | [Protolenses](https://ncatlab.org/nlab/show/natural+transformation): schema-parameterized lens families, auto-generation, symmetric lenses, and law verification |
| [`panproto-check`](crates/panproto-check) | Breaking change detection via structural diffing and protocol-aware classification |
| [`panproto-protocols`](crates/panproto-protocols) | 76 built-in protocol definitions composed from 27 building-block theories |
| [`panproto-io`](crates/panproto-io) | Instance-level parse/emit codecs (JSON, XML, tabular, web documents) |
| [`panproto-vcs`](crates/panproto-vcs) | Schematic version control: content-addressed store, commit DAG, pushout-based merge |
| [`panproto-core`](crates/panproto-core) | Re-export facade |
| [`panproto-wasm`](crates/panproto-wasm) | WASM bindings with handle-based slab allocator, MessagePack boundary, and protolens entry points |
| [`panproto-cli`](crates/panproto-cli) | CLI (`schema`): validate, check, diff, lift, convert, lens, and git-style version control |

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

// Auto-generate a lens between two schema versions
let lens = panproto_lens::auto_generate(&src_schema, &tgt_schema)?;
let (view, complement) = panproto_lens::get(&lens, &instance)?;
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

// One-liner data conversion between schema versions
const converted = p.convert(record, oldSchema, newSchema);

// Or build a reusable protolens chain
const chain = p.protolensChain(oldSchema, newSchema);
const result = chain.apply(record);
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

# One-liner data conversion between schema versions
converted = pp.convert(record, old_schema, new_schema)

# Or build a reusable protolens chain
chain = pp.protolens_chain(old_schema, new_schema)
result = chain.apply(record)
```

### CLI

```sh
# Validate a schema against a protocol
schema validate --protocol atproto schema.json

# Detect breaking changes between two schema versions
schema check --protocol atproto old.json new.json

# Diff two schemas
schema diff old.json new.json

# One-step data conversion between schemas
schema convert --src-schema old.json --tgt-schema new.json record.json

# Auto-generate a lens with human-readable summary
schema lens --src old.json --tgt new.json

# Apply a saved lens or protolens chain
schema lens-apply --lens lens.json record.json

# Verify lens laws and naturality
schema lens-verify --lens lens.json --instance test.json

# Compose lenses or protolens chains
schema lens-compose lens1.json lens2.json -o composed.json

# Derive a lens from VCS commit history
schema lens-diff HEAD~1 HEAD

# Apply a migration to a record
schema lift --protocol atproto --migration mig.json \
  --src-schema old.json --tgt-schema new.json record.json

# Generate minimal test data from a protocol theory
schema scaffold --protocol atproto schema.json

# Simplify a schema by merging equivalent elements
schema normalize --protocol atproto schema.json --identify "A=B,C=D"

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

**[Protolenses](https://ncatlab.org/nlab/show/natural+transformation)** are the primary abstraction for bidirectional schema transformations. A protolens is a [natural transformation](https://ncatlab.org/nlab/show/natural+transformation) between theory endofunctors whose components are lenses. Unlike a single `Lens` (between two specific schemas), a `Protolens` is a schema-parameterized family of lenses: for every schema S satisfying a precondition P(S), it produces a `Lens(F(S), G(S))`. Elementary protolens constructors provide the atomic building blocks, while `auto_generate` derives an entire lens automatically from two schemas. `SymmetricLens` pairs two protolens chains for full bidirectional synchronization.

**Schematic version control** (`panproto-vcs`) provides git-style operations (commit, branch, merge, rebase, cherry-pick, bisect, blame) operating on schema graphs rather than text. Merges are computed as categorical pushouts. There is no heuristic tie-breaking; the merge is commutative.

**Automatic migration discovery** finds schema morphisms via backtracking CSP with MRV heuristic, discovers overlaps between schemas, and computes schema-level pushouts for merging disparate formats.

## Safety guarantees

panproto provides four layers of algebraic safety that go beyond structural schema validation:

- **Type-checked migrations.** Auto-derived migrations are validated as well-formed [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories) at the GAT level. Every `schema commit` runs GAT type-checking by default (disable with `--skip-verify`) to catch ill-typed vertex maps, arity mismatches, and unsound edge rewirings before they enter the commit DAG.

- **Verified equations.** Schemas are checked against the axioms of their protocol theory. If a protocol declares equations (e.g., associativity of composition in a category theory), `schema verify` enumerates variable assignments over the schema's carrier sets and confirms that both sides of every equation evaluate to the same value.

- **Naturality checking.** Protolenses are verified to be natural transformations: for every schema morphism in scope, the naturality square commutes. `schema lens-verify` checks both the lens laws (GetPut, PutGet) and naturality on test instances.

- **Pullback-enhanced merges.** The three-way merge algorithm uses categorical [pullbacks](https://ncatlab.org/nlab/show/pullback) to detect structural overlap between branches. When two branches modify sorts or operations that share a common image under their protocol morphisms, the merge identifies these shared elements precisely rather than relying on name matching alone, producing fewer false conflicts.

## Documentation

- [Tutorial](https://panproto.dev/tutorial/): 22-chapter guide from first schema to automatic migration
- [Dev Guide](https://panproto.dev/dev-guide/): internals, algorithms, and architecture
- [API Reference (docs.rs)](https://docs.rs/panproto-core)
- Interactive Playground (coming soon): runs entirely in your browser via WebAssembly

## License

[MIT](LICENSE)
