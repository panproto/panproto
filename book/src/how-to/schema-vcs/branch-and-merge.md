# Branch and merge

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

`merge` runs the pushout construction over the schema graphs at HEAD, the merging branch's HEAD, and their common ancestor. The result is the smallest schema containing both branches' changes.

If the pushout's universal property fails (because the two branches' changes contradict on a shared substructure), `merge` raises `UniversalFactorizationFailure` and produces a conflict object instead of an unsafe result.

To resolve a conflict:

```sh
schema status              # lists conflict objects
schema show <conflict>     # inspect the contradiction
# edit the conflict descriptor
schema add <conflict>
schema commit -m "resolve conflict"
```

## Verification

```sh
schema log --graph
```

shows the DAG with the merge commit visible. The merge commit has two parents (HEAD and the merged branch's HEAD).

## Common mistakes

- Editing schemas while on the wrong branch. `schema status` always shows the current branch; check it before editing.
- Treating conflict objects as text. They are structured descriptions of an irreconcilable categorical equation; resolution is by selecting one side or extending the schema, not by editing JSON in place.

## See also

- [Pushouts and merge](../../explanation/semantics/pushouts-and-merge.md) for the merge construction.
- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Init and commit](./init-and-commit.md).
