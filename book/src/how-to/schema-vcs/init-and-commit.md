# Init and commit

Initialize a panproto repository, stage a schema, and record the first content-addressed commit.

## Prerequisites

The `schema` CLI installed. A directory containing schema files (or a fresh directory you are about to populate).

## The task

```sh
cd my-schemas/
schema init                 # create .panproto/
schema add user.json
schema commit -m "initial user schema"
schema log                  # show history
```

`init` creates a `.panproto/` directory analogous to `.git/`. `add` stages a schema; `commit` creates a content-addressed snapshot. The behavior matches git for the parts that overlap.

To inspect the current state:

```sh
schema status
schema diff               # diff working tree vs HEAD
schema show <commit-hash>
```

## Verification

```sh
schema log --oneline
```

prints the commit history. Each commit shows its blake3 hash, schema hash, message, and author.

## Common mistakes

- Forgetting to `schema add` before `schema commit`. Like git, the staging area is explicit; commits only include staged changes.
- Editing inside `.panproto/objects/` directly. The store is content-addressed; manual edits break the hash invariants.

## See also

- [Branch and merge](./branch-and-merge.md).
- [Reference: CLI](../../reference/cli.md).
- [Schema version control semantics](../../explanation/vcs-semantics.md).
