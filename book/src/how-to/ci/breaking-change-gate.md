# Breaking-change gate

A breaking-change gate is a CI step that fails the build when a PR introduces a schema change panproto classifies as breaking. It runs against the diff between the PR's schema and the schema at `main`.

## Prerequisites

A panproto repository under git. A CI system that can run shell commands.

## The task

```sh
# In your CI script
git fetch origin main
schema check \
  --src <(git show origin/main:schemas/user.json) \
  --tgt schemas/user.json \
  --mapping <auto> \
  --typecheck \
  --classify
```

`--classify` causes `schema check` to print the migration's classification (`fully-compatible`, `backward-compatible`, or `breaking`) and exit non-zero on `breaking`.

To allow an explicit override, gate on a commit-message marker or a PR label:

```sh
if git log -1 --format=%B | grep -q '\[breaking-change-acknowledged\]'; then
  schema check ... --classify || true   # warn but do not fail
else
  schema check ... --classify           # fail on breaking
fi
```

## Verification

Open a PR that adds a backward-compatible field; the gate passes. Open a PR that drops a required field; the gate fails. Add `[breaking-change-acknowledged]` to the commit message; the gate passes with a warning.

## Common mistakes

- Skipping `--typecheck`. Without it, the gate misses GAT-level type errors that are also breaking changes.
- Comparing against the wrong base. The gate must compare against the merge base of the PR and `main`, not against `main`'s tip; otherwise force-pushes to `main` make every PR look breaking.

## See also

- [GitHub Actions](./github-actions.md) for a drop-in workflow.
- [Pre-commit hooks](./pre-commit-hooks.md) for catching breakage before push.
- [Build a migration](../build-migration.md).
