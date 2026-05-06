# GitHub Actions

A drop-in workflow for schema validation and breaking-change detection on every pull request.

## Prerequisites

A panproto repository on GitHub. The `panproto-cli` Homebrew tap, shell installer, or `cargo install` available on Linux runners.

## The task

```yaml
# .github/workflows/panproto.yml
name: panproto

on:
  pull_request:
    paths:
      - 'schemas/**'
      - 'panproto.toml'

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0           # need history for the diff base

      - name: Install schema
        run: |
          curl --proto '=https' -LsSf \
            https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.sh | sh
          echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"

      - name: Validate
        run: schema validate --project .

      - name: Breaking-change gate
        run: |
          base=$(git merge-base origin/${{ github.base_ref }} HEAD)
          schema check \
            --src <(git show $base:schemas/user.json) \
            --tgt schemas/user.json \
            --typecheck --classify
```

Two jobs: validation and the breaking-change gate. Validation fails on a malformed schema; the gate fails on a breaking diff.

## Verification

Push a PR and watch the workflow run. The validation step exits zero on a clean schema; the gate prints the classification and exits non-zero on breaking.

## Common mistakes

- Omitting `fetch-depth: 0`. Shallow clones make the merge-base lookup fail; the gate then runs against the wrong base.
- Pinning `panproto-cli` to a stale version. Use the latest installer; the protocol catalogue and existence-check rules evolve.

## See also

- [Breaking-change gate](./breaking-change-gate.md) for the underlying mechanic.
- [Pre-commit hooks](./pre-commit-hooks.md) for the local-side equivalent.
