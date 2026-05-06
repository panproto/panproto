# Continuous integration

CI for panproto projects covers two questions: did this change validate, and did this change break compatibility?

| Page | Purpose |
|---|---|
| [Breaking-change gate](./breaking-change-gate.md) | Block PRs that introduce a breaking schema change unless explicitly acknowledged. |
| [GitHub Actions](./github-actions.md) | Drop-in workflows for schema validation and breaking-change detection. |
| [Pre-commit hooks](./pre-commit-hooks.md) | Run schema validation before each local commit. |

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [What panproto verifies](../../explanation/what-is-verified.md).
