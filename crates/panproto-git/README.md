# panproto-git

Bidirectional git to panproto-vcs translation bridge.

## Overview

Enables `git push cospan main` by translating between git repositories and panproto-vcs stores. On import, git trees are parsed through `panproto-project` to produce structural schemas. On export, schemas are emitted back to source text via `panproto-parse` emitters.

## Import (git to panproto)

1. Walk git commit DAG topologically (parents before children)
2. For each commit: read all files from the git tree
3. Parse each file through its language parser (via `panproto-project`)
4. Assemble project-level schema (coproduct)
5. Store schema and create panproto-vcs commit (preserving author, timestamp, message)

## Export (panproto to git)

1. Load project schema from panproto-vcs commit
2. Reconstruct per-file source from interstitial text and leaf literal fragments
3. Build nested git tree objects preserving directory hierarchy
4. Create git commit with mapped parent pointers

## Functoriality

Import preserves DAG structure: parent pointers in panproto-vcs match the git DAG. Composition of imports matches import of composition: `import(a ; b) = import(a) ; import(b)`.

## Usage

```rust
use panproto_git::import_git_repo;
use panproto_vcs::MemStore;

let git_repo = git2::Repository::open("./my-project")?;
let mut store = MemStore::new();
let result = import_git_repo(&git_repo, &mut store, "HEAD")?;
println!("Imported {} commits", result.commit_count);
```

## License

MIT
