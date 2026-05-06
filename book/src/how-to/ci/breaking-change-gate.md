# Breaking-change gate

A breaking-change gate is a CI step that fails the build when a PR introduces a schema change panproto classifies as breaking. It runs against the diff between the PR's schema and the schema at `main`.

## Prerequisites

A panproto repository under git. A CI system that can run shell commands.

## The task

`schema check` enforces existence conditions; it does not classify. Use `schema diff` (which reports structural changes) plus `schema lens generate` (which fails when no chain exists) and combine into a gate:

```sh
# In your CI script.
git fetch origin main
git show origin/main:schemas/user.json > /tmp/user-base.json

# Existence + GAT-level type check.
schema check \
  --src /tmp/user-base.json \
  --tgt schemas/user.json \
  --mapping migrations/user.json \
  --typecheck

# Generate a chain; non-zero exit means the change is breaking
# (no auto-generated lens covers it).
schema lens generate --protocol json-schema /tmp/user-base.json schemas/user.json --save /tmp/chain.json
```

Either step's non-zero exit fails the build. To allow an explicit override, gate on a commit-message marker or a PR label:

```sh
if git log -1 --format=%B | grep -q '\[breaking-change-acknowledged\]'; then
  schema check ... --typecheck || true
  schema lens generate ... || true
else
  schema check ... --typecheck
  schema lens generate ...
fi
```

For richer classification, use the SDK: `panproto-check`'s `diff_and_classify` returns a `CompatReport` with `fully_compatible | backward_compatible | breaking` and the offending elements.

## Verification

Open a PR that adds a backward-compatible field; the gate passes. Open a PR that drops a required field; the gate fails. Add `[breaking-change-acknowledged]` to the commit message; the gate passes with a warning.

## Common mistakes

- Skipping `--typecheck`. Without it, the gate misses GAT-level type errors that are also breaking changes.
- Comparing against the wrong base. The gate must compare against the merge base of the PR and `main`, not against `main`'s tip; otherwise force-pushes to `main` make every PR look breaking.

## See also

- [GitHub Actions](./github-actions.md) for a drop-in workflow.
- [Pre-commit hooks](./pre-commit-hooks.md) for catching breakage before push.
- [Build a migration](../build-migration.md).
