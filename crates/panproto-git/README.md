# panproto-git

[![crates.io](https://img.shields.io/crates/v/panproto-git.svg)](https://crates.io/crates/panproto-git)
[![docs.rs](https://docs.rs/panproto-git/badge.svg)](https://docs.rs/panproto-git)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Translation between a `git2::Repository` and a `panproto-vcs` object store.

## Import

`import_git_repo` resolves a revspec, walks its ancestors with parents before
children, parses each Git tree through `panproto-project`, and writes panproto commit
objects. `ImportResult` reports the imported commit count, the panproto ID for the
selected Git head, and the Git-to-panproto OID map.

The incremental entry points accept a caller-supplied map of Git OIDs already known
to the panproto store. `import_git_repo_persistent` also uses a disk-backed blob
cache. The cache lets unchanged Git blobs reuse `FileSchemaObject` leaves across
imports.

Import copies the commit message, author display name, timestamp, and mapped parent
links. `CommitObject` has no author-email field, so email addresses are not retained.

## Export

`export_to_git` exports one panproto commit. It always writes `schema.json` and
`commit.json`. It also reconstructs source files when the schema contains the
literal and interstitial byte-position constraints needed to do so. The function
synthesizes the Git email as `<author>@panproto`, and it includes only parents found
in the supplied `parent_map`. Thus export is not an unconditional byte-for-byte or
metadata-preserving inverse of import.

## Example

```rust,ignore
use std::collections::HashMap;
use panproto_git::{export_to_git, import_git_repo};
use panproto_vcs::MemStore;

let git_repo = git2::Repository::open(".")?;
let mut store = MemStore::new();
let imported = import_git_repo(&git_repo, &mut store, "HEAD")?;

let out = git2::Repository::init("exported")?;
let parents = HashMap::new();
let exported = export_to_git(&store, &out, imported.head_id, &parents, None)?;
```

## Public API

| Item | Purpose |
|------|---------|
| `import_git_repo` | Import ancestors of a revspec |
| `import_git_repo_incremental` | Import with a caller-supplied known-OID map |
| `import_git_repo_persistent` | Incremental import with a persistent blob cache |
| `load_blob_cache`, `save_blob_cache` | Read and write the blob cache |
| `export_to_git` | Export one commit to a Git repository |
| `ImportResult`, `ExportResult` | Import and export metadata |
| `GitBridgeError` | Bridge errors |

## License

[MIT](../../LICENSE)
