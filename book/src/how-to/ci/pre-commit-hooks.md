# Pre-commit hooks

A pre-commit hook runs schema validation before each `git commit`, so you catch malformed schemas before they enter the history. Optionally, it can also warn about breaking changes.

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

for f in $changed; do
  schema validate --protocol json-schema "$f"
done

# Optional breaking-change warning
for f in $changed; do
  base="$(git show :"$f"@{u} 2>/dev/null)" || continue
  schema check --src <(echo "$base") --tgt "$f" --classify || \
    echo "warning: $f introduces a breaking change (commit anyway? Ctrl-C to abort)"
done
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
        entry: schema validate --project .
        language: system
        files: '^schemas/.*\.json$'
        pass_filenames: false
```

`pre-commit install` activates it.

## Verification

Stage a malformed schema and try to commit; the hook rejects. Fix the schema and commit again; the hook passes.

## Common mistakes

- Hook stalls on missing `schema` binary. Wrap the invocation in a `command -v schema || exit 0` to fall back gracefully on machines without the CLI.
- Validating every file on every commit. The script above scopes to staged `schemas/*.json` only; broader scopes are noisy.

## See also

- [Breaking-change gate](./breaking-change-gate.md) for the CI-side equivalent.
- [Reference: CLI](../../reference/cli.md).
