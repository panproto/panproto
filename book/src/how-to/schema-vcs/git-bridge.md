# Bridge to git

panproto-vcs is independent of git, but most projects host their source in git. The bridge lets a panproto-vcs repository live alongside (or inside) a git one, and lets git remotes serve panproto histories.

## Prerequisites

A git repository (the host project) and either a `.panproto/` directory inside it or a sibling panproto repository.

## The task

### Sidecar mode

The simplest setup: keep `.panproto/` next to `.git/`. Both are tracked by their own tools; commits to either are independent.

```sh
git add .panproto/
git commit -m "snapshot panproto state"
```

panproto-vcs's content-addressed objects are deterministic, so storing them in git works (no merge conflicts inside `.panproto/objects/`).

### Bidirectional translation

`schema git import` reads a git repository's history and produces a corresponding panproto-vcs DAG; `schema git export` does the reverse:

```sh
schema git import path/to/git-repo HEAD
schema git export path/to/output-git-repo --repo .
```

The import range can be any git revspec (`HEAD`, `main`, `HEAD~10..HEAD`). The export takes the current panproto repository and writes a git mirror.

### git-remote helper

The `panproto-git-remote` crate registers a custom git remote helper that exposes a panproto-vcs repository to git clients (so `git clone panproto://path/to/repo` works). The helper is separate from `schema git`; install the binary and configure git accordingly. See [`crates/panproto-git-remote`](https://github.com/panproto/panproto/tree/main/crates/panproto-git-remote).

### Merge bridging

A git merge that touches `.panproto/objects/` will not preserve panproto's structural merge guarantees, since git merges the bytes. The supported pattern is to do schema-level work in panproto and then `schema git export` the result. There is no automatic git-merge-to-pushout translation in the CLI; the merge must originate in `schema merge` to get the universal-property-checked pushout.

## Verification

```sh
schema status
git status
```

Both should be clean. A clean panproto status with a dirty git status is normal (you may have untracked files git knows about that panproto does not). The reverse is not.

## Common mistakes

- Three-way text-merging `.panproto/objects/` files. The store is content-addressed; the bytes are correct or wrong, never partially. Use `schema git rebase` for any conflict.
- Mixing the two modes. Choose sidecar or remote-bridge per project; mixing creates ambiguity about which DAG is the source of truth.

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Init and commit](./init-and-commit.md).
- [Branch and merge](./branch-and-merge.md).
