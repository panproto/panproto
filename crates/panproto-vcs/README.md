# panproto-vcs

[![crates.io](https://img.shields.io/crates/v/panproto-vcs.svg)](https://crates.io/crates/panproto-vcs)
[![docs.rs](https://docs.rs/panproto-vcs/badge.svg)](https://docs.rs/panproto-vcs)

Schematic version control for panproto.

Schemas, data snapshots, complements, and protocol definitions are content-addressed objects (blake3) stored in a commit DAG, with branches, three-way structural merge via categorical pushout, rename detection, data lifting through history, and automatic data migration. The CLI binary is `schema`.

## API

| Item | Description |
|------|-------------|
| `Repository` | High-level porcelain: init, add, commit, merge, rebase, cherry-pick, reset, gc |
| `FsStore` | Filesystem-backed object store (`.panproto/` directory) |
| `MemStore` | In-memory store for tests and WASM |
| `Store` | Trait abstracting over storage backends |
| `ObjectId` | Blake3 content-address (32 bytes) |
| `Object` | Enum: `Schema`, `Migration`, `Commit`, `Theory`, `TheoryMorphism`, `Expr`, and others |
| `CommitObject` | A point in the schema evolution DAG (with `theory_ids` tracking stored theories) |
| `CommitObjectBuilder` | Builder for `CommitObject` with sensible defaults |
| `HeadState` | Branch or detached HEAD |
| `ReflogEntry` | Audit trail entry for ref mutations |
| `Index` | Staging area for the next commit |
| `MergeResult` | Three-way merge output with typed conflict detection |
| `BisectState` / `BisectStep` | Binary search for breaking commits |
| `BlameEntry` | Which commit introduced a schema element |
| `GcReport` | Garbage collection results |
| `GatDiagnostics` | Type errors, equation violations, and migration warnings from GAT validation |
| `CommitOptions` | Commit configuration including `skip_verify` to bypass GAT equation checks |
| `DataSetObject` | Content-addressed data snapshot binding instances to a schema version |
| `ComplementObject` | Persistent complement for backward data migration |
| `data_mig::migrate_forward` | Migrate data forward through a migration, storing complement |
| `data_mig::migrate_backward` | Restore data from a stored complement |
| `data_mig::detect_staleness` | Check which data sets need migration |
| `Repository::add_data` | Stage data files alongside schema changes |
| `Repository::add_protocol` | Stage a protocol definition for versioning |
| `Repository::checkout_with_data` | Switch branch and migrate data |
| `Repository::merge_with_data` | Merge and migrate data |
| `EditLogObject` | Content-addressed edit sequence for incremental migration |
| `edit_mig::incremental_migrate` | Translate a sequence of edits through an edit lens |
| `edit_mig::encode_edit_log` / `decode_edit_log` | MessagePack serialization for edit sequences |
| `store_expr` / `load_expr` | Content-addressed expression storage via `MessagePack` + blake3 |

## Modules

| Module | Description |
|--------|-------------|
| `hash` | Canonical serialization + blake3 content addressing |
| `dag` | Merge base, path finding, log walk, compose path, `compose_path_with_coherence` |
| `merge` | Three-way schema merge + conflict detection |
| `refs` | Branches, tags, resolve_ref |
| `auto_mig` | Derive Migration from SchemaDiff |
| `rename_detect` | Structural similarity scoring for vertex/edge renames |
| `rebase` | Replay commits onto a new base |
| `cherry_pick` | Apply a single commit's migration |
| `reset` | Soft/mixed/hard HEAD reset |
| `stash` | Save/restore working state |
| `bisect` | Binary search for breaking commit |
| `blame` | Schema element attribution |
| `gc` | Mark-sweep garbage collection |
| `gat_validate` | GAT-level validation: type-checked migrations, theory equation verification, schema model checking, `schema_to_theory` extraction |
| `repo` | Repository orchestration (porcelain) |

## Safety

The VCS pipeline integrates GAT-level validation at two points:

- **On commit:** auto-derived migrations are validated as well-formed theory morphisms via `gat_validate::validate_migration`. The protocol theory's equations are type-checked via `gat_validate::validate_theory_equations`, and the schema model is verified against those equations via `gat_validate::validate_schema_equations`. Pass `skip_verify` in `CommitOptions` to bypass these checks.
- **On merge:** pullback-enhanced overlap detection uses `panproto_gat::pullback` to identify structural overlap between the two branch heads and the merge base. This produces fewer false conflicts than name-based matching alone, because two sorts or operations that share a common image under their protocol morphisms are recognized as the same element regardless of local naming.

The `compose_path_with_coherence` function composes a chain of migrations along a commit path and verifies that intermediate morphisms are coherent, returning both the composed migration and any coherence warnings.

## Example

```rust,ignore
use panproto_vcs::Repository;

let mut repo = Repository::init(".").unwrap();

// Stage and commit a schema
repo.add(&schema).unwrap();
let id = repo.commit("initial schema", "alice").unwrap();

// Branch, evolve, merge
panproto_vcs::refs::create_branch(repo.store_mut(), "feature", id).unwrap();
```

## License

[MIT](../../LICENSE)
