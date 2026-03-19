# panproto-lens

[![crates.io](https://img.shields.io/crates/v/panproto-lens.svg)](https://crates.io/crates/panproto-lens)
[![docs.rs](https://docs.rs/panproto-lens/badge.svg)](https://docs.rs/panproto-lens)

Protolens-based bidirectional schema transformations for panproto.

A [lens](https://ncatlab.org/nlab/show/lens+%28in+computer+science%29) is a concrete pair (`get`, `put`) between two *fixed* schemas, with complement tracking and round-trip laws (GetPut, PutGet; see [Diskin et al., 2011](https://doi.org/10.1016/j.tcs.2010.12.039)). A **protolens** is *not* a lens. It is a dependent function from schemas to lenses: for every schema S satisfying a precondition P(S), calling `instantiate(S)` produces a `Lens(F(S), G(S))` where F and G are theory endofunctors. A single protolens works on any compatible schema; a lens is bound to the exact schemas it was built for. `auto_generate` derives an entire protolens chain (and its instantiated lens) automatically from two schemas by factorizing the underlying theory morphism.

## API

### Protolenses

| Item | Description |
|------|-------------|
| `Protolens` | A dependent function from schemas to lenses: `Π(S : Schema \| P(S)). Lens(F(S), G(S))` |
| `ProtolensChain` | Composable sequence of protolenses forming a reusable, schema-independent lens family |
| `elementary::*` | Elementary protolens constructors (add/drop/rename sort, add/drop/rename op, add/drop equation, directed equation, pullback) |
| `auto_generate` | Automatically generate a lens between two schemas by factorizing the underlying morphism |
| `AutoLensConfig` | Configuration for auto-generation (strategy, max steps, etc.) |
| `AutoLensResult` | Result of auto-generation: lens, protolens chain, and human-readable summary |
| `ComplementConstructor` | Schema-parameterized complement factory: Empty, DroppedSortData, CoercedData, MergedSortData, DefaultedSort, Composite |
| `ComplementSpec` | Dependent complement type evaluation for a protolens step |
| `DefaultRequirement` | Specifies default values required when a protolens adds structure |
| `CapturedField` | A field captured into the complement during a `get` step |
| `complement_spec_at` | Compute the complement specification for a single protolens step at a given schema |
| `chain_complement_spec` | Compute the composite complement specification for an entire protolens chain |
| `diff_to_protolens` | Derive a protolens chain from a structural schema diff |
| `diff_to_lens` | Derive a concrete lens from a structural schema diff |
| `DiffSpec` | Configuration for diff-based protolens derivation |
| `SchemaConstraint` | Direct schema-level precondition checking (vs lossy implicit theory extraction) |
| `check_applicability` | Check applicability returning failure reasons (not just boolean) |
| `ProtolensChain::fuse` | Compose all steps into a single protolens (avoids intermediate schemas) |
| `ProtolensChain::to_json` / `from_json` | Serialize and deserialize chains for cross-project reuse |
| `Protolens::to_json` / `from_json` | Serialize and deserialize individual protolenses |
| `FleetResult` | Result of applying a chain to multiple schemas (applied + skipped with reasons) |
| `apply_to_fleet` | Apply a chain to a fleet of schemas |
| `lift_protolens` / `lift_chain` | Lift protolenses along theory morphisms for cross-protocol reuse |
| `ComplementConstructor::AddedElement` | Complement variant for elements requiring defaults |
| `OpticKind` | Optic classification: Iso, Lens, Prism, Affine, Traversal |
| `classify_transform` | Classify a `TheoryTransform` into its optic kind |
| `SymbolicStep` | Symbolic representation of protolens steps for algebraic simplification |
| `simplify_steps` | Normalize a step sequence via inverse cancellation, rename fusion, and add-drop cancellation |

### Lenses

| Item | Description |
|------|-------------|
| `Lens` | Asymmetric lens backed by a compiled migration, source schema, and target schema |
| `get` | Forward direction: project an instance to a view, producing a complement |
| `put` | Backward direction: restore source from a modified view and complement |
| `Complement` | Data discarded by `get`, needed by `put` to reconstruct the source |
| `compose` | Compose two lenses sequentially |

### Symmetric lenses

| Item | Description |
|------|-------------|
| `SymmetricLens` | Symmetric (bidirectional) lens pairing two protolens chains with shared complement |
| `SymmetricLens::from_protolens_chains` | Build a symmetric lens from two protolens chains |
| `SymmetricLens::auto_symmetric` | Automatically generate a symmetric lens between two schemas |

### Verification

| Item | Description |
|------|-------------|
| `check_laws` / `check_get_put` / `check_put_get` | Verify lens laws on a test instance |
| `LensError` / `LawViolation` | Error types |

## Example

```rust,ignore
use panproto_lens::{auto_generate, get, put, check_laws, ProtolensChain};

// Auto-generate a lens between two schema versions
let result = auto_generate(&src_schema, &tgt_schema)?;
let (view, complement) = get(&result.lens, &instance)?;

// Modify the view...
let restored = put(&result.lens, &modified_view, &complement)?;

// Verify round-trip laws
check_laws(&result.lens, &instance)?;

// Build a reusable protolens chain (schema-independent)
let chain = result.chain;
let lens_at_other_schema = chain.instantiate(&other_src, &other_tgt)?;
```

## License

[MIT](../../LICENSE)
