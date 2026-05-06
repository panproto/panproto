# Schema version control basics

You will turn the `my-first-schema/` project into a panproto repository, commit two versions of the schema, branch off a feature, and merge it back. About twenty minutes.

By the end you will have: a `.panproto/` directory tracking schema history, two commits on `main`, a feature branch with a third commit, and a merge that reconciles them through the pushout construction.

## Prerequisites

Completed [Your first migration](./your-first-migration.md). The `schema` CLI installed ([Install the CLI](../how-to/install/cli.md)).

## Step 1: initialise

```sh
cd my-first-schema/
schema init
ls .panproto/
```

A `.panproto/` directory appears, structurally similar to `.git/`. It has `objects/`, `refs/`, `HEAD`, and a small config.

## Step 2: commit v1

Save your v1 schema (the one from the first tutorial) as `schemas/user.json`. Then:

```sh
schema add schemas/user.json
schema status
schema commit -m "v1 user schema"
schema log --oneline
```

You see one commit: the v1 schema, with a blake3 hash.

## Step 3: commit v2

Replace `schemas/user.json` with the v2 schema (rename `age` to `years`, add `email`).

```sh
schema diff               # show what changed structurally
schema add schemas/user.json
schema commit -m "v2: rename age to years, add email"
schema log --oneline
```

Two commits. The diff shows structural changes (rename, addition), not text changes.

## Step 4: branch

```sh
schema branch feature/handle
schema checkout feature/handle
```

You are now on a new branch sharing history with `main` up to the v2 commit.

Edit `schemas/user.json` to add a `handle` field:

```json
"handle": { "type": "string" }
```

(or, equivalently, modify the schema with the SDK and re-serialise.)

```sh
schema add schemas/user.json
schema commit -m "add handle"
schema log --oneline --graph
```

You see the linear history with the new commit on `feature/handle`.

## Step 5: merge

```sh
schema checkout main
schema merge feature/handle
```

The merge runs the pushout construction over the v2 schema, the feature-branch schema, and their common ancestor (the v2 commit). Since the changes do not contradict (the feature added a new field; main did nothing in parallel), the pushout is straightforward and produces a clean merge commit.

```sh
schema log --oneline --graph
```

You see the merge commit with two parents.

## Step 6: provoke a conflict

To see what a conflict looks like: branch off `main` again, add `handle` with a different type (say, `integer` instead of `string`), and try to merge.

```sh
schema branch feature/handle-int
schema checkout feature/handle-int
# edit schemas/user.json: handle as integer
schema add schemas/user.json
schema commit -m "add handle as integer"

schema checkout main
schema merge feature/handle-int
```

The merge fails with `UniversalFactorizationFailure`: the pushout would require `handle` to be both `string` (from your earlier merge) and `integer`, which contradicts. panproto-vcs raises a conflict object instead of producing a wrong merge.

```sh
schema status     # shows the conflict
schema show <conflict-hash>
```

To resolve, pick one type, edit the conflict descriptor, `schema add`, `schema commit`.

## What you built

A schema history that you can navigate, branch, and merge with the same affordances as a git history, but where the merge operation is a precise structural construction rather than a three-way text merge.

## Next

- [Cross-protocol translation](./cross-protocol-translation.md) for translating the schema from JSON Schema to Protobuf.
- The plain-terms explanation of merge is at [Schema version control semantics](../explanation/vcs-semantics.md).
- The formal pushout construction is in [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md).
