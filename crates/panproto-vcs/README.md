# panproto-vcs

Schematic version control for panproto.

Schemas are content-addressed objects stored in a commit DAG, with branches, three-way structural merge, and data lifting through history. The CLI binary is `schema`.

## API

| Item | Description |
|------|-------------|
| `Repository` | High-level porcelain: init, add, commit, merge, rebase, cherry-pick, reset, gc |
| `FsStore` | Filesystem-backed object store (`.panproto/` directory) |
| `MemStore` | In-memory store for tests and WASM |
| `Store` | Trait abstracting over storage backends |
| `ObjectId` | Blake3 content-address (32 bytes) |
| `Object` | Enum: `Schema`, `Migration`, `Commit` |
| `CommitObject` | A point in the schema evolution DAG |
| `HeadState` | Branch or detached HEAD |
| `ReflogEntry` | Audit trail entry for ref mutations |
| `Index` | Staging area for the next commit |
| `Instance` | Unified enum wrapping WInstance/FInstance/GInstance |
| `MergeResult` | Three-way merge output with conflicts |
| `BisectState` / `BisectStep` | Binary search for breaking commits |
| `BlameEntry` | Which commit introduced a schema element |
| `GcReport` | Garbage collection results |

## Modules

| Module | Description |
|--------|-------------|
| `hash` | Canonical serialization + blake3 content addressing |
| `dag` | Merge base, path finding, log walk, compose path |
| `merge` | Three-way schema merge + conflict detection |
| `refs` | Branches, tags, resolve_ref |
| `auto_mig` | Derive Migration from SchemaDiff |
| `rebase` | Replay commits onto a new base |
| `cherry_pick` | Apply a single commit's migration |
| `reset` | Soft/mixed/hard HEAD reset |
| `stash` | Save/restore working state |
| `bisect` | Binary search for breaking commit |
| `blame` | Schema element attribution |
| `gc` | Mark-sweep garbage collection |
| `repo` | Repository orchestration (porcelain) |

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
