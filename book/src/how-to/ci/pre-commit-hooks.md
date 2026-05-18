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
  schema validate --protocol atproto "$f"
done

# Optional breaking-change warning: try to auto-generate a chain
# between the staged version and the upstream copy. Failure suggests
# the change is breaking.
for f in $changed; do
  base_blob=$(git show :@{u}:"$f" 2>/dev/null) || continue
  echo "$base_blob" > /tmp/base.json
  if ! schema lens generate --protocol atproto /tmp/base.json "$f" --save /tmp/chain.json 2>/dev/null; then
    echo "warning: $f introduces a breaking change (commit anyway? Ctrl-C to abort)"
  fi
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
        entry: schema validate --protocol atproto
        language: system
        files: '^schemas/.*\.json$'
```

The hook receives the staged file paths as positional arguments; the `--protocol` flag is required.

`pre-commit install` activates it.

## Verification

Stage a malformed schema and try to commit; the hook rejects. Fix the schema and commit again; the hook passes.

## Common mistakes

- Hook stalls on missing `schema` binary. Wrap the invocation in a `command -v schema || exit 0` to fall back gracefully on machines without the CLI.
- Validating every file on every commit. The script above scopes to staged `schemas/*.json` only; broader scopes are noisy.

## See also

- [Breaking-change gate](./breaking-change-gate.md) for the CI-side equivalent.
- [Reference: CLI](../../reference/cli.md).
