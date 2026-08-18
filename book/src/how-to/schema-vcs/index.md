# Schema version control

These guides put schemas, migrations, and associated data under panproto version control. Begin with repository creation and commits; add branching, data migration, or a git bridge only when the project needs that workflow.

| Page | Purpose |
|---|---|
| [Initialize and commit](./init-and-commit.md) | Start a repository, then stage and commit schema changes. |
| [Branch and merge](./branch-and-merge.md) | Diverge a feature branch, merge it back via pushout. |
| [Version data alongside schemas](./data-versioning.md) | Carry data instances in commits and lift them through migrations automatically. |
| [Bridge to git](./git-bridge.md) | Run panproto-vcs alongside git, or as a custom git remote. |

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md) for the model.
- [Pushouts and merge](../../explanation/semantics/pushouts-and-merge.md) for the merge construction.
- [Reference: CLI](../../reference/cli.md) for the full subcommand list.
