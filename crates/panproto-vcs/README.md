# panproto-vcs

[![crates.io](https://img.shields.io/crates/v/panproto-vcs.svg)](https://crates.io/crates/panproto-vcs)
[![docs.rs](https://docs.rs/panproto-vcs/badge.svg)](https://docs.rs/panproto-vcs)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Git-style version control for schemas, with structural merging and automatic data migration.

## What it does

Every schema, migration, data snapshot, and commit is stored as a content-addressed object identified by its blake3 hash. Commits form a directed acyclic graph (DAG), the same structure git uses. You get branches, merging, rebasing, cherry-picking, bisecting, and blame, all operating on schemas rather than source files.

The key difference from git is how merges work. Git merges text by looking for the longest common substring. panproto-vcs merges schemas by comparing their graph structure: two branches that both added a field named `email` to a `User` vertex are recognized as adding the same thing, even if they were written independently. This structural comparison (implemented via categorical pushout) produces fewer false conflicts than text diffing.

The VCS also versions data alongside schemas. When you commit a schema change that removes a field, the complement (the dropped field values) is stored so that a future backward migration can recover them. `migrate_forward` applies a migration to a data snapshot and stores the complement; `migrate_backward` restores the original data from the complement. This gives you lossless schema history, not just structural diffs.

Multi-file projects are stored as Merkle trees. Each source file is hashed individually as a `FileSchema` leaf; directories are `SchemaTree::Directory` nodes whose entries point at child trees by content hash. Two commits that touch one file in a thousand-file project share the other 999 leaves automatically. A `FlatSchema` object preserves the merged whole-project schema for queries that span files. The `tree::resolve_commit_schema` and `tree::store_schema_as_tree` helpers move between the tree-shaped on-disk form and the flat in-memory schema used by migration.

## Quick example

```rust,ignore
use panproto_vcs::{Repository, refs};

let mut repo = Repository::init(std::path::Path::new(".")).unwrap();

// Stage a schema and commit it.
repo.add(&v1_schema).unwrap();
let commit_id = repo.commit("initial schema", "alice").unwrap();

// Create a branch, evolve the schema, and merge back.
refs::create_branch(repo.store_mut(), "feature", commit_id).unwrap();
repo.add(&v2_schema).unwrap();
let feature_id = repo.commit("add email field", "alice").unwrap();
repo.merge("main", feature_id).unwrap();
```

## API overview

| Export | What it does |
|--------|-------------|
| `Repository` | High-level API: `init`, `open`, `add`, `commit`, `merge`, `rebase`, `cherry_pick`, `reset`, `gc` |
| `FsStore` | Filesystem-backed object store (`.panproto/` directory) |
| `MemStore` | In-memory object store for tests and WASM contexts |
| `Store` | Trait abstracting over storage backends |
| `ObjectId` | Blake3 content address (32 bytes) |
| `Object` | Enum covering all object types: `Schema`, `FileSchema`, `SchemaTree` (with `SingleLeaf` and `Directory` variants), `FlatSchema`, `Migration`, `Commit`, `Theory`, `Expr`, and others |
| `tree::resolve_commit_schema` | Resolve a commit to its (possibly tree-shaped) schema, walking `SchemaTree` Merkle nodes |
| `tree::store_schema_as_tree` | Store a multi-file schema as a `SchemaTree` of per-file `FileSchema` leaves, sharing unchanged subtrees by content hash |
| `CommitObject` | A point in the schema evolution DAG |
| `CommitObjectBuilder` | Builder for `CommitObject` with sensible defaults |
| `DataSetObject` | A content-addressed data snapshot bound to a schema version |
| `ComplementObject` | Stored complement for lossless backward migration |
| `HeadState` | Branch name or detached HEAD (a bare `ObjectId`) |
| `Index` | Staging area for the next commit |
| `CommitOptions` | Commit configuration (includes `skip_verify` to bypass GAT validation) |
| `VcsError` | Error type for all VCS operations |
| `dag::log_walk` | Walk commits from HEAD toward roots |
| `dag::merge_base` | Find the common ancestor of two commits |
| `refs::create_branch` | Create a branch pointing at a commit |
| `merge` | Three-way structural merge with typed conflict detection |
| `rebase` | Replay commits onto a new base |
| `cherry_pick` | Apply a single commit's migration to the current HEAD |
| `bisect` | Binary search for the commit that introduced a breaking change |
| `blame` | Determine which commit introduced a schema element |
| `gc` | Mark-sweep garbage collection for unreachable objects |
| `data_mig::migrate_forward` | Apply a migration to a data snapshot, storing the complement |
| `data_mig::migrate_backward` | Restore data from a stored complement |
| `data_mig::detect_staleness` | Check which data snapshots need migration |

## License

[MIT](../../LICENSE)
