# Continuous integration

Add CI after schemas validate locally. The smallest useful gate classifies compatibility; local hooks and a hosted workflow apply the same checks earlier and on every pull request.

| Page | Purpose |
|---|---|
| [Breaking-change gate](./breaking-change-gate.md) | Fail CI when a schema change is classified as breaking unless the change is explicitly acknowledged. |
| [GitHub Actions](./github-actions.md) | Run schema validation and compatibility classification on pull requests. |
| [Pre-commit hooks](./pre-commit-hooks.md) | Run schema validation before each local commit. |

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [What panproto verifies](../../explanation/what-is-verified.md).
