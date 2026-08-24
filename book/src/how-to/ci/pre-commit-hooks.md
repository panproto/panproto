# Pre-commit hooks

A pre-commit hook rejects malformed staged schemas before they enter the history. This example validates the bytes in the index, so an unstaged working-tree edit cannot change the result.

## Prerequisites

The `schema` CLI installed. A git repository.

## The task

### Plain git hook

```sh
# .git/hooks/pre-commit
#!/usr/bin/env bash
set -euo pipefail

changed=$(git diff --cached --name-only --diff-filter=ACM | grep -E '^schemas/.*\.json$' || true)
[ -z "$changed" ] && exit 0
staged_file=$(mktemp)
base_file=$(mktemp)
trap 'rm -f "$staged_file" "$base_file"' EXIT

while IFS= read -r f; do
  git show ":$f" > "$staged_file"
  schema validate --protocol atproto "$staged_file"

  # Optional warning against the tracked upstream copy.
  if git show "@{u}:$f" > "$base_file" 2>/dev/null && \
     ! schema compat "$base_file" "$staged_file" --protocol atproto; then
    echo "warning: compatibility check failed for $f" >&2
  fi
done <<< "$changed"
```

`chmod +x .git/hooks/pre-commit` installs it.

### With pre-commit framework

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: schema-validate
        name: panproto schema validate
        entry: schema validate --protocol atproto
        language: system
        files: '^schemas/.*\.json$'
```

The hook receives the staged file paths as positional arguments; the `--protocol` flag is required.

`pre-commit install` activates it.

## Verification

Stage a malformed schema and try to commit. The hook rejects it. After the schema is fixed, the next commit passes.

## Common mistakes

- Silently skipping a missing `schema` binary. Prefer a failing hook with an installation message; CI remains the authoritative gate when contributors may bypass hooks.
- Validating every file on every commit. The script above scopes to staged `schemas/*.json` only; broader scopes are noisy.

## See also

- [Breaking-change gate](./breaking-change-gate.md) for the CI-side equivalent.
- [Reference: CLI](../../reference/cli.md).
