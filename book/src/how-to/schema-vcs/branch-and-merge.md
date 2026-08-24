# Branch and merge

Create a branch to isolate a schema change, then merge it back through panproto's schema-aware merge operation.

## Prerequisites

A panproto repository with at least one commit ([Init and commit](./init-and-commit.md)).

## The task

```sh
schema branch feature/add-handle
schema checkout feature/add-handle
# edit the schema, add a `handle` field
schema add user.json
schema commit -m "add handle"

schema checkout main
schema merge feature/add-handle
```

`merge` uses the current commit, the named branch, and their common ancestor. A fast-forward moves the current branch. A divergent merge combines compatible structural changes and records a two-parent commit unless `--no-commit` or `--squash` changes that behavior.

If both branches make incompatible changes to the same structure, the command prints the detected conflicts and exits nonzero. The current CLI does not persist an editable conflict descriptor.

Resolve the source schemas on one branch, commit that resolution, and rerun the merge:

```sh
schema checkout feature/add-handle
# edit user.json to incorporate the intended result
schema add user.json
schema commit -m "resolve merge inputs"
schema checkout main
schema merge feature/add-handle
```

## Verification

```sh
schema log
```

shows a `Merge:` line for a two-parent merge commit. The `--graph` option is accepted but currently ignored by the renderer.

## Common mistakes

- Expecting checkout to rewrite source files. It moves the repository ref only. Edit or regenerate working files explicitly.
- Looking for a saved conflict object after a failed merge. Capture the printed conflict details; the CLI does not yet provide an interactive continuation flow.

## See also

- [Pushouts and merge](../../explanation/semantics/pushouts-and-merge.md) for the merge construction.
- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Init and commit](./init-and-commit.md).
