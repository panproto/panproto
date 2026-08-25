# panproto-vcs

[![crates.io](https://img.shields.io/crates/v/panproto-vcs.svg)](https://crates.io/crates/panproto-vcs)
[![docs.rs](https://docs.rs/panproto-vcs/badge.svg)](https://docs.rs/panproto-vcs)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Content-addressed storage and version control for panproto schemas.

## Storage model

`ObjectId` is a 32-byte BLAKE3 digest of an object's type-specific canonical
serialization. `Store` defines object, ref, HEAD, enumeration, and reflog operations.
`FsStore` stores repositories below `.panproto`; `MemStore` implements the same trait
in memory.

`Object` currently represents migrations, commits, tags, datasets, migration
complements, protocols, expressions, edit logs, theories, theory morphisms, CST
complements, per-file schemas, schema-tree nodes, and flat migration-endpoint schemas.
There is no standalone `Object::Schema` variant.

Commits point to `SchemaTreeObject` roots. Directory entries refer to per-file leaves
or child trees. Unchanged subtrees therefore retain the same content address.
`resolve_commit_schema` assembles a flat in-memory schema. `store_schema_as_tree` is
available under `panproto_vcs::tree`; it is not re-exported at the crate root.

This content-addressed tree structure follows the hash-tree construction introduced
by [Merkle](https://doi.org/10.1007/3-540-48184-2_32). The object types and canonical
serialization are panproto-specific.

## Repository operations

`Repository` provides filesystem-backed `init`, `open`, staging, commit, merge, amend,
log, cherry-pick, rebase, reset, garbage collection, and data operations. Staging
derives a migration from HEAD and can run bounded GAT validation. Protocol equations
are checked only when the caller has registered the corresponding theory with
`set_protocol_theory`.

Three-way merge compares schema structure, reports typed conflicts, and returns
migrations from both sides. A clean merge is checked with `verify_pushout` before the
porcelain records it. Same-name, same-definition additions are deliberately identified
by the implementation, so this merge is an amalgamated quotient rather than a free
pushout.

Pushout-based patch merge is developed by [Mimram and Di
Giusto](https://doi.org/10.1016/j.entcs.2013.09.018). panproto's `verify_pushout`
checks the implemented cocone conditions; it does not prove that this concrete merge
realizes the full categorical model in that paper.

## Data migration

`migrate_forward` must be called explicitly with a stored dataset, source and target
schemas, and a protocol. It stores the migrated dataset and a `ComplementObject`, then
returns both IDs. `migrate_backward` consumes that complement. A schema commit by
itself does not capture dropped field values, and the APIs do not promise losslessness
outside successful lens execution with the matching complement.

## Example

```rust,ignore
use panproto_vcs::Repository;
use std::path::Path;

let mut repo = Repository::init(Path::new("workspace"))?;
repo.add(&schema)?;
let commit_id = repo.commit("initial schema", "alice")?;
```

## Main API groups

| Group | Items |
|-------|-------|
| Porcelain | `Repository`, `AddOptions`, `CommitOptions` |
| Storage | `Store`, `FsStore`, `MemStore`, `Object`, `ObjectId` |
| History | `CommitObject`, `HeadState`, `dag`, `refs`, `rebase`, `cherry_pick`, `reset` |
| Merge | `merge::three_way_merge`, `MergeResult`, typed conflicts, `verify_pushout` |
| Project trees | `FileSchemaObject`, `SchemaTreeObject`, `tree` helpers |
| Data | `DataSetObject`, `ComplementObject`, `migrate_forward`, `migrate_backward` |
| Maintenance | `gc`, `bisect`, `blame`, `stash`, `status` |

## License

[MIT](../../LICENSE)
