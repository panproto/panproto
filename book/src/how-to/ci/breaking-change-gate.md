# Breaking-change gate

A breaking-change gate is a CI step that fails the build when a PR introduces a schema change panproto classifies as breaking. It runs against the diff between the PR's schema and the schema at `main`.

## Prerequisites

A panproto repository under git. A CI system that can run shell commands.

## The task

`schema compat` classifies the diff between two schema versions and sets its exit code by tier: 0 for a non-breaking change (fully compatible or backward compatible), 1 for a breaking one, 2 for a usage or load error. That exit code is the gate. Pair it with `schema check --typecheck`, which catches GAT-level type errors the structural classification does not:

```sh
# In your CI script.
git fetch origin main
git show origin/main:schemas/user.json > /tmp/user-base.json

# Classify the change; exit 1 means breaking, exit 2 a usage or load error.
schema compat /tmp/user-base.json schemas/user.json --protocol atproto

# GAT-level type check.
schema check \
  --src /tmp/user-base.json \
  --tgt schemas/user.json \
  --mapping migrations/user.json \
  --typecheck
```

Either step's non-zero exit fails the build. To allow an explicit override, gate on a commit-message marker or a PR label:

```sh
if git log -1 --format=%B | grep -q '\[breaking-change-acknowledged\]'; then
  schema compat /tmp/user-base.json schemas/user.json --protocol atproto || true
  schema check ... --typecheck || true
else
  schema compat /tmp/user-base.json schemas/user.json --protocol atproto
  schema check ... --typecheck
fi
```

For richer reporting, add `--format json` to `schema compat`, or use the SDK: in Python, `panproto.diff_and_classify(old, new, protocol)` returns a `CompatReport`; in Rust, call `panproto_check::diff(...)` followed by `panproto_check::classify(&diff, &protocol)`; in TypeScript, `panproto.diffFull(old, new).classify(protocol)` returns the same report. Each `CompatReport` carries a `classification` tier (`fully-compatible`, `backward-compatible`, or `breaking`) alongside a `breaking` list, a `non_breaking` list, and a `compatible` boolean, along with the offending elements.

## Verification

Open a PR that adds a backward-compatible field; the gate passes. Open a PR that drops a required field; the gate fails. Add `[breaking-change-acknowledged]` to the commit message; the gate passes with a warning.

## Common mistakes

- Skipping `--typecheck`. Without it, the gate misses GAT-level type errors that are also breaking changes.
- Comparing against the wrong base. The gate must compare against the merge base of the PR and `main`, not against `main`'s tip; otherwise force-pushes to `main` make every PR look breaking.

## See also

- [GitHub Actions](./github-actions.md) for a drop-in workflow.
- [Pre-commit hooks](./pre-commit-hooks.md) for catching breakage before push.
- [Build a migration](../build-migration.md).
