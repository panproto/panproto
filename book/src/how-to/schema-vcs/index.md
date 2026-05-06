# Schema version control

panproto-vcs is git for schemas: an immutable DAG of schema, migration, lens, and data objects with branches, tags, merges, and a content-addressed store. The CLI verbs match git: `init`, `add`, `commit`, `branch`, `merge`, `log`, `diff`.

The four pages here cover the practical workflows.

| Page | Purpose |
|---|---|
| [Init and commit](./init-and-commit.md) | Start a repository, stage and commit changes. |
| [Branch and merge](./branch-and-merge.md) | Diverge a feature branch, merge it back via pushout. |
| [Version data alongside schemas](./data-versioning.md) | Carry data instances in commits and lift them through migrations automatically. |
| [Bridge to git](./git-bridge.md) | Run panproto-vcs alongside git, or as a custom git remote. |

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md) for the model.
- [Pushouts and merge](../../explanation/semantics/pushouts-and-merge.md) for the merge construction.
- [Reference: CLI](../../reference/cli.md) for the full subcommand list.
