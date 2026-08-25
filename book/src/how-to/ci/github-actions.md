# GitHub Actions

Add this workflow to validate schemas and classify compatibility on every pull request.

## Prerequisites

A panproto repository on GitHub. This example assumes `schemas/*.json` contains panproto's serialized schema format rather than external schema-language documents.

## The task

```yaml
# .github/workflows/panproto.yml
name: panproto

on:
  pull_request:
    paths:
      - 'schemas/**'
      - 'migrations/**'
      - 'panproto.toml'

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0           # need history for the diff base

      - name: Install schema
        run: cargo install panproto-cli --version 0.72.0 --locked

      - name: Validate
        run: |
          for f in schemas/*.json; do
            schema validate --protocol atproto "$f"
          done

      - name: Breaking-change gate
        run: |
          base=$(git merge-base origin/${{ github.base_ref }} HEAD)
          git show "$base:schemas/user.json" > /tmp/base.json
          schema compat /tmp/base.json schemas/user.json --protocol atproto
          schema check --src /tmp/base.json --tgt schemas/user.json \
            --mapping migrations/user.json --typecheck
```

The `--protocol` flag is required for every per-file `schema validate`. There is no `--project` flag. For an external document such as a Lexicon, use the appropriate parse path before this validation step.

The job has separate validation and breaking-change steps. Validation fails on a malformed schema; `schema compat` gives the gate its compatibility exit code, and `schema check --typecheck` rejects an invalid migration mapping.

## Verification

After a pull request is pushed, validation exits zero when every schema passes. The compatibility gate prints its classification and exits nonzero for a breaking change.

## Common mistakes

- Omitting `fetch-depth: 0`. Shallow clones make the merge-base lookup fail; the gate then runs against the wrong base.
- Leaving the CLI version unpinned. Update the pinned version deliberately and review any compatibility-classifier changes with the update.

## See also

- [Breaking-change gate](./breaking-change-gate.md) for the underlying mechanic.
- [Pre-commit hooks](./pre-commit-hooks.md) for the local-side equivalent.
