# panproto-git

[![crates.io](https://img.shields.io/crates/v/panproto-git.svg)](https://crates.io/crates/panproto-git)
[![docs.rs](https://docs.rs/panproto-git/badge.svg)](https://docs.rs/panproto-git)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Translates between git repositories and panproto's version control system.

## What it does

Git stores snapshots of file bytes; panproto-vcs stores snapshots of structured schemas. This crate is the bridge between the two. Import walks a git commit history in topological order (parents before children), reads each commit's file tree, parses every file through `panproto-project`, and writes the resulting schema into a panproto-vcs commit that carries the same author name, email, timestamp, message, and parent links as the original git commit. The entire DAG shape is preserved.

Export goes the other direction: it reads a panproto-vcs commit, reconstructs source files from the stored schemas using the appropriate language emitters, and writes git tree and commit objects. This lets you round-trip a codebase through panproto and back to git without losing history.

Incremental import avoids re-processing the full history on subsequent runs. It takes the panproto-vcs store as input and only walks git commits that are not already present, extending the panproto history from where it left off.

## Quick example

```rust,ignore
use panproto_git::{import_git_repo, import_git_repo_incremental, export_to_git};
use panproto_vcs::FsStore;

// Full import of a git repository.
let store = FsStore::open("my-panproto-store")?;
let result = import_git_repo("path/to/git-repo", &store)?;
println!("imported {} commits", result.commit_count);

// Incremental import: only process commits added since last import.
let result = import_git_repo_incremental("path/to/git-repo", &store)?;
println!("imported {} new commits", result.commit_count);

// Export panproto commits back to a git repository.
let export = export_to_git(&store, "path/to/output-git-repo")?;
println!("exported {} commits", export.commit_count);
```

## API overview

| Export | What it does |
|--------|-------------|
| `import_git_repo()` | Full import: walk all git commits and create panproto-vcs commits |
| `import_git_repo_incremental()` | Incremental import: only process commits not already in the store |
| `export_to_git()` | Export panproto-vcs commits to git tree and commit objects |
| `ImportResult` | Result of an import: commit count, skipped count, any warnings |
| `ExportResult` | Result of an export: commit count, output path |
| `GitBridgeError` | Error variants: git2 errors, parse failures, VCS store errors |

## License

[MIT](../../LICENSE)
