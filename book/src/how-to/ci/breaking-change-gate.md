# Breaking-change gate

A breaking-change gate fails CI when a pull request introduces a schema change that panproto classifies as breaking. The gate compares the proposed schema with the merge base on `main`.

## Prerequisites

A panproto repository under git. A CI system that can run shell commands.

## The task

`schema compat` classifies the diff between two schema versions and sets its exit code by tier: 0 for a non-breaking change, 1 for a breaking change, and 2 for a usage or load error. Pair it with `schema check --typecheck` when the repository also stores an explicit migration mapping; that second command checks the mapping's existence conditions and theory-level types.

```sh
# In your CI script.
git fetch origin main
base_commit=$(git merge-base HEAD origin/main)
git show "$base_commit:schemas/user.json" > /tmp/user-base.json

# Classify the change; exit 1 means breaking, exit 2 a usage or load error.
schema compat /tmp/user-base.json schemas/user.json --protocol atproto

# Check the explicit mapping and its theory-level types.
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
  schema check \
    --src /tmp/user-base.json \
    --tgt schemas/user.json \
    --mapping migrations/user.json \
    --typecheck || true
else
  schema compat /tmp/user-base.json schemas/user.json --protocol atproto
  schema check \
    --src /tmp/user-base.json \
    --tgt schemas/user.json \
    --mapping migrations/user.json \
    --typecheck
fi
```

For machine-readable output, add `--format json` to `schema compat`. The CLI JSON and the Rust and Python reports include a three-way `classification` field, breaking and non-breaking lists, and a `compatible` boolean. The TypeScript `CompatReport` exposes `isCompatible`, `isBreaking`, `breakingChanges`, and `nonBreakingChanges`; call `toJson()` when the three-way classification string is needed.

## Verification

Open a PR that adds a backward-compatible field, and the gate passes. A PR that drops a required field fails. Adding `[breaking-change-acknowledged]` to the commit message lets the gate pass with a warning.

## Common mistakes

- Running `schema check` without a maintained mapping file. Compatibility classification needs only the two schemas; mapping checks are an additional gate for repositories that version migrations.
- Comparing against the wrong base. The gate must compare against the merge base of the PR and `main`, not against `main`'s tip; otherwise force-pushes to `main` make every PR look breaking.

## See also

- [GitHub Actions](./github-actions.md) for a hosted workflow.
- [Pre-commit hooks](./pre-commit-hooks.md) for catching breakage before push.
- [Build a migration](../build-migration.md).
