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
Level 4  Protolenses: dependent functions from schemas to lenses (Π(S). Lens(F(S), G(S)))
```

## Workspace

| Crate | Description |
|-------|-------------|
| [`panproto-gat`](crates/panproto-gat) | GAT engine: sorts, operations, equations, directed equations, theory morphisms, colimits, endofunctors, refinement types, and equality witnesses |
| [`panproto-expr`](crates/panproto-expr) | Pure functional expression language: lambda calculus with closures, pattern matching, ~50 builtins, step/depth limits |
| [`panproto-expr-parser`](crates/panproto-expr-parser) | Haskell-style surface syntax parser (logos + chumsky) with Pratt precedence and pretty printer |
| [`panproto-schema`](crates/panproto-schema) | Schema representation with protocol-aware builder and adjacency indices |
| [`panproto-inst`](crates/panproto-inst) | [W-type](https://ncatlab.org/nlab/show/W-type), set-valued functor, and graph instances with restrict/extend/Kan extension pipelines and value-level field transforms |
| [`panproto-mig`](crates/panproto-mig) | Migration engine: existence checks, compilation, lift, compose, invert, coverage analysis |
| [`panproto-lens`](crates/panproto-lens) | [Protolenses](https://ncatlab.org/nlab/show/natural+transformation): schema-parameterized lens families, optic classification, symbolic simplification, auto-generation |
| [`panproto-check`](crates/panproto-check) | Breaking change detection via structural diffing and protocol-aware classification |
| [`panproto-protocols`](crates/panproto-protocols) | 77 built-in protocol definitions composed from 34 building-block theories |
| [`panproto-io`](crates/panproto-io) | Instance-level parse/emit codecs (JSON, XML, tabular, web documents) |
| [`panproto-vcs`](crates/panproto-vcs) | Schematic version control: content-addressed store, commit DAG, pushout-based merge |
| [`panproto-parse`](crates/panproto-parse) | Tree-sitter full-AST parsing for 10 languages with auto-derived GAT theories and interstitial text emission |
| [`panproto-project`](crates/panproto-project) | Multi-file project assembly via schema coproduct with cross-file import resolution |
| [`panproto-git`](crates/panproto-git) | Bidirectional git to panproto-vcs translation bridge (import/export with DAG preservation) |
| [`panproto-llvm`](crates/panproto-llvm) | LLVM IR protocol definition, language AST lowering morphisms, and inkwell-based IR parsing |
| [`panproto-jit`](crates/panproto-jit) | LLVM JIT compilation of panproto expressions via inkwell for accelerated data migration |
| [`panproto-core`](crates/panproto-core) | Re-export facade (feature-gated: `full-parse`, `project`, `git`, `llvm`, `jit`) |
| [`panproto-wasm`](crates/panproto-wasm) | WASM bindings with handle-based slab allocator, MessagePack boundary, and protolens entry points |
| [`panproto-py`](crates/panproto-py) | Native Python bindings via PyO3 with `pythonize` (serde to Python dicts) |
| [`panproto-cli`](crates/panproto-cli) | CLI (`schema`): validate, check, diff, lift, convert, lens, expr, enrich, parse, git bridge, and version control |

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

proto = panproto.get_builtin_protocol("atproto")

builder = proto.schema()
builder.vertex("post", "record", "app.bsky.feed.post")
builder.vertex("post:body", "object")
builder.vertex("post:body.text", "string")
builder.edge("post", "post:body", "record-schema")
builder.edge("post:body", "post:body.text", "prop", "text")
builder.constraint("post:body.text", "maxLength", "3000")
schema = builder.build()

# Diff two schema versions
diff = panproto.diff_schemas(old_schema, new_schema)
report = diff.classify(proto)
print(report.compatible)       # True/False
print(report.report_text())    # human-readable summary

# Auto-generate a lens between two schema versions
lens, quality = panproto.auto_generate_lens(old_schema, new_schema, proto)
view, complement = lens.get(instance)
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
schema lens --apply --lens lens.json record.json

# Verify lens laws and naturality
schema lens --verify --lens lens.json --instance test.json

# Compose lenses or protolens chains
schema lens --compose lens1.json lens2.json -o composed.json

# Check applicability with failure reasons
schema lens --check --chain chain.json --schema schema.json

# Lift a protolens chain to another protocol
schema lens --lift --chain chain.json --morphism morphism.json

# Full-AST parsing
schema parse file src/main.ts                # Parse a source file into structural schema
schema parse project ./src                   # Parse a directory into a unified project schema
schema parse emit src/main.ts                # Round-trip: parse then emit back to source

# Git bridge
schema git import /path/to/repo HEAD         # Import git history into panproto-vcs
schema git export --repo . /path/to/dest     # Export panproto-vcs to a git repository

# Derive a lens from VCS commit history
schema lens --diff HEAD~1 HEAD

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

# Migrate data to match current schema
schema migrate records/
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

# Python SDK (native PyO3 bindings)
maturin develop --manifest-path crates/panproto-py/Cargo.toml
```

## Architecture

panproto implements a four-level architecture rooted in category theory. The GAT engine (Level 0) is the only component implemented directly in Rust. Everything above it (protocols, schemas, instances) is data interpreted by the engine.

[**W-type**](https://ncatlab.org/nlab/show/W-type) **instances** (tree-structured data like JSON/ATProto records) use a 5-step restrict pipeline: anchor surviving nodes, compute reachability from root, contract ancestors, resolve edges, and reconstruct fans.

**[Set-valued functor](https://ncatlab.org/nlab/show/functor) instances** (relational data like SQL tables) use [precomposition](https://ncatlab.org/nlab/show/precomposition) (&#916;<sub>F</sub>) for restrict and [left Kan extension](https://ncatlab.org/nlab/show/Kan+extension) (&#931;<sub>F</sub>) for extend. Right Kan extension (&#928;<sub>F</sub>) computes products over fibers.

**Protolenses** are the primary abstraction for bidirectional schema transformations. A [lens](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29) is a concrete pair (`get`, `put`) between two fixed schemas. A protolens is *not* a lens — it is a [dependent function](https://ncatlab.org/nlab/show/dependent+product+type) from schemas to lenses: `Π(S : Schema | P(S)). Lens(F(S), G(S))`. A single protolens works on any schema satisfying its precondition; a lens is bound to the exact schemas it was built for. Elementary protolens constructors provide the atomic building blocks, while `auto_generate` derives an entire lens automatically from two schemas. `SymmetricLens` pairs two protolens chains for full bidirectional synchronization. Protolens chains can be serialized for cross-project reuse, applied to fleets of schemas in batch, and lifted across protocols via theory morphisms.

**Schematic version control** (`panproto-vcs`) provides git-style operations (commit, branch, merge, rebase, cherry-pick, bisect, blame) operating on schema graphs rather than text. Merges are computed as categorical pushouts. There is no heuristic tie-breaking; the merge is commutative.

**Data versioning** stores instance data, complements, and protocol definitions as content-addressed objects alongside schemas in the commit DAG. `schema migrate` automatically generates lenses from the schema history and applies them to data files. Complements are persisted so backward migration never loses data. `schema checkout --migrate` and `schema merge --migrate` handle data migration as part of normal VCS operations.

**Automatic migration discovery** finds schema morphisms via backtracking CSP with MRV heuristic, discovers overlaps between schemas, and computes schema-level pushouts for merging disparate formats.

**Full-AST parsing** (`panproto-parse`) treats programs as schemas. Tree-sitter grammars are theory presentations: `node-types.json` is structurally isomorphic to a GAT. The theory extraction pipeline auto-derives sorts from node types and operations from field names. A single generic walker handles all 10 languages; interstitial text capture (keywords, punctuation, whitespace between named children) enables exact round-trip emission.

**Multi-file assembly** (`panproto-project`) constructs the project-level schema as a categorical coproduct of per-file schemas, with path-prefixed vertex IDs and cross-file import edges from `ThImport`.

**Git bridge** (`panproto-git`) translates between git repositories and panproto-vcs stores. Import walks the commit DAG topologically, parsing each tree through `panproto-project`. Export reconstructs source files from schema fragments and builds nested git trees. DAG structure is preserved functorially.

**LLVM integration** spans two crates. `panproto-llvm` defines the LLVM IR protocol (31 vertex kinds, 56 instruction opcodes) and theory morphisms lowering language ASTs to LLVM IR (compilation as structure-preserving maps). `panproto-jit` compiles panproto expressions to native code via inkwell for accelerated data migration.

## Safety guarantees

panproto provides three layers of algebraic safety that go beyond structural schema validation:

- **Type-checked migrations.** Auto-derived migrations are validated as well-formed [theory morphisms](https://ncatlab.org/nlab/show/morphism+of+theories) at the GAT level. Every `schema commit` runs GAT type-checking by default (disable with `--skip-verify`) to catch ill-typed vertex maps, arity mismatches, and unsound edge rewirings before they enter the commit DAG.

- **Verified equations.** Schemas are checked against the axioms of their protocol theory. If a protocol declares equations (e.g., associativity of composition in a category theory), `schema verify` enumerates variable assignments over the schema's carrier sets and confirms that both sides of every equation evaluate to the same value.

- **Pullback-enhanced merges.** The three-way merge algorithm uses categorical [pullbacks](https://ncatlab.org/nlab/show/pullback) to detect structural overlap between branches. When two branches modify sorts or operations that share a common image under their protocol morphisms, the merge identifies these shared elements precisely rather than relying on name matching alone, producing fewer false conflicts.

## Documentation

- [Tutorial](https://panproto.dev/tutorial/): 22-chapter guide from first schema to automatic migration
- [Dev Guide](https://panproto.dev/dev-guide/): internals, algorithms, and architecture
- [API Reference (docs.rs)](https://docs.rs/panproto-core)
- Interactive Playground (coming soon): runs entirely in your browser via WebAssembly

## License

[MIT](LICENSE)
