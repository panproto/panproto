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

### git-remote bridge

The `panproto-git-remote` crate registers a custom git remote helper that exposes a panproto-vcs repository to git clients:

```sh
git clone panproto://path/to/repo my-clone
```

git sees panproto commits as git commits; the bridge translates the DAGs on the fly.

### Merge bridging

When git merges a branch that carries `.panproto/` changes, run:

```sh
schema git rebase
```

to translate the git merge into the corresponding panproto-vcs merge with proper pushout semantics. Without this, the `.panproto/` files merge as raw bytes and you lose the structural merge guarantees.

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
