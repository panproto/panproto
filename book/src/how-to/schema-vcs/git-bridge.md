# Bridge to git

The git bridge stores a panproto repository beside a source repository and can translate history between the two systems.

## Prerequisites

A git repository (the host project) and either a `.panproto/` directory inside it or a sibling panproto repository.

## The task

### Sidecar mode

The simplest setup: keep `.panproto/` next to `.git/`. Both are tracked by their own tools; commits to either are independent.

```sh
git add .panproto/
git commit -m "snapshot panproto state"
```

The object names are content hashes, so unchanged objects deduplicate. Git can still report conflicts in refs or other mutable files under `.panproto/`; do not edit stored object contents by hand.

### Bidirectional translation

`schema git export` writes the current panproto history into a destination git repository:

```sh
schema git export path/to/output-git-repo --repo .
```

`schema git import path/to/git-repo HEAD` currently imports into an in-memory store, prints the imported count and temporary panproto ID, then exits without updating `.panproto/`. Use it only as a diagnostic until the command accepts a persistent destination. Export opens `--repo`, creates or opens the destination, and writes the current `HEAD`.

### git-remote helper

The `panproto-git-remote` crate ships `git-remote-panproto`, a remote helper for a panproto node reached over XRPC. After installing that binary, a node URL has the form `panproto://did:plc:abc123/repository-name`; a local filesystem path is not a valid substitute. See [`crates/panproto-git-remote`](https://github.com/panproto/panproto/tree/main/crates/panproto-git-remote).

### Merge bridging

A git merge that touches `.panproto/` merges files, not schemas. Perform schema-level work with `schema merge`, then export the resulting history. The CLI has no automatic translation from a git merge to a structural schema merge.

## Verification

```sh
schema status
git status
```

Inspect both outputs independently. Panproto status concerns its current branch and staged schema; git status concerns files in the host repository. Either side can be dirty while the other is clean.

## Common mistakes

- Three-way text-merging `.panproto/objects/` files. Re-run the schema operation in panproto, then export the result.
- Mixing the two modes. Choose sidecar or remote-bridge per project; mixing creates ambiguity about which DAG is the source of truth.

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Init and commit](./init-and-commit.md).
- [Branch and merge](./branch-and-merge.md).
