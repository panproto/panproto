# panproto-cli

[![crates.io](https://img.shields.io/crates/v/panproto-cli.svg)](https://crates.io/crates/panproto-cli)

Command-line interface for panproto. The binary is called `schema`.

Provides subcommands for schema validation, migration checking, breaking change detection, record lifting, protolens-based data conversion, and schematic version control.

## Installation

```sh
# macOS (Homebrew)
brew install panproto/tap/schema

# Linux / macOS (shell installer)
curl --proto '=https' -LsSf https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.sh | sh

# Windows (PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.ps1 | iex"

# From source (any platform with Rust)
cargo install panproto-cli
```

## Usage

```sh
# Validate a schema against a protocol
schema validate --protocol atproto schema.json

# Diff two schemas (use --theory for sort/op/equation-level diff)
schema diff old.json new.json
schema diff --theory old.json new.json

# One-step data conversion between schemas (auto-generates a protolens)
schema data convert --src-schema old.json --tgt-schema new.json record.json

# Auto-generate a lens with human-readable summary
schema lens generate old.json new.json
schema lens generate old.json new.json -o lens.json

# Apply a saved lens or protolens chain to data
schema lens apply lens.json record.json

# Verify lens laws (GetPut, PutGet)
schema lens verify --lens lens.json --instance test.json

# Compose lenses or protolens chains
schema lens compose lens1.json lens2.json -o composed.json

# Inspect a protolens chain
schema lens inspect chain.json

# Check applicability of a protolens chain
schema lens --check --chain chain.json --schema schema.json

# Lift a protolens chain to another protocol
schema lens --lift --chain chain.json --morphism morphism.json

# Derive a lens from VCS commit history
schema lens --diff HEAD~1 HEAD
schema lens --diff abc123 def456

# Check a migration (use --typecheck for GAT-level validation)
schema check --src old.json --tgt new.json --mapping mig.json
schema check --src old.json --tgt new.json --mapping mig.json --typecheck

# Type-check a migration at the GAT level
schema typecheck --src old.json --tgt new.json --migration mig.json

# Verify a schema satisfies its protocol theory's equations
schema verify --protocol atproto schema.json

# Generate minimal test data from a protocol theory
schema scaffold --protocol atproto schema.json --depth 3

# Simplify a schema by merging equivalent elements
schema normalize --protocol atproto schema.json --identify "A=B,C=D"

# Initialize a schema repository and commit
schema init
schema add schema.json
schema commit -m "initial schema"
schema commit -m "initial schema" --skip-verify  # bypass GAT checks

# Branch, evolve, merge
schema branch feature
schema checkout feature
schema add schema-v2.json
schema commit -m "add field"
schema checkout main
schema merge feature
schema merge feature --verbose  # show pullback overlap details

# Fuse a multi-step chain into a single protolens
schema lens old.json new.json --protocol atproto --fuse

# Parse a Haskell-style expression and print its AST
schema expr parse "x + 1"

# Evaluate an expression
schema expr eval "2 + 3 * 4"

# Pretty-print in canonical form
schema expr fmt "\x->x+ 1"

# Check syntax without evaluating
schema expr check "let x = 1 in x + 2"

# Lift a record through a migration
schema lift --migration mig.json --src-schema src.json --tgt-schema tgt.json record.json

# Migrate data to match current schema version
schema data migrate records/ --protocol atproto
schema data migrate records/ --dry-run
schema data migrate records/ --range HEAD~3..HEAD
schema data migrate records/ --backward

# Sync data with incremental edit tracking
schema data sync records/
schema data sync records/ --edits

# Show data staleness
schema data status records/

# Stage data alongside schema
schema add schema.json --data records/

# Checkout and migrate data
schema checkout feature --migrate records/

# Merge and migrate data
schema merge feature --migrate records/
```

## Subcommands

| Command | Description |
|---------|-------------|
| `validate` | Validate a schema file against a protocol (also type-checks the protocol theory) |
| `check` | Check existence conditions for a migration (`--typecheck` to also GAT-validate the morphism) |
| `diff` | Diff two schemas and report structural changes (`--theory` to show sort/op/equation-level diff) |
| `data convert` | One-step data conversion between schemas via auto-generated protolens |
| `data migrate` | Migrate data to match current schema version (`--dry-run`, `--range`, `--backward`) |
| `data sync` | Sync data files to target schema version (`--edits` to record edit log) |
| `data status` | Show data staleness |
| `lens generate` | Auto-generate a lens between two schemas (`--fuse` to merge steps) |
| `lens compose` | Compose two protolens chains |
| `lens apply` | Apply a saved lens or protolens chain to data |
| `lens verify` | Verify lens laws (GetPut, PutGet) |
| `lens inspect` | Print human-readable summary of a protolens chain |
| `lift` | Apply a migration to a record |
| `scaffold` | Generate minimal test data from a protocol theory via free model construction |
| `normalize` | Simplify a schema by merging equivalent elements via quotient |
| `typecheck` | Type-check a migration between two schemas at the GAT level |
| `verify` | Verify a schema satisfies its protocol theory's equations by model checking |
| `init` | Initialize a `.panproto/` repository |
| `add` | Stage a schema for the next commit (`--data` to stage data files alongside) |
| `commit` | Commit staged changes (`--skip-verify` to bypass GAT equation verification) |
| `status` | Show working state |
| `log` | Walk commit history |
| `show` | Inspect an object |
| `branch` | Create, list, or delete branches |
| `tag` | Create, list, or delete tags |
| `checkout` | Switch branch or detach HEAD (`--migrate` to migrate data) |
| `merge` | Three-way schema merge (`--verbose` to show pullback overlap details, `--migrate` to migrate data) |
| `rebase` | Replay commits onto another branch |
| `cherry-pick` | Apply a single commit's migration |
| `reset` | Move HEAD / unstage / restore |
| `stash` | Save or restore working state |
| `reflog` | Show ref mutation history |
| `bisect` | Binary search for breaking commit |
| `blame` | Show which commit introduced an element |
| `gc` | Garbage collect unreachable objects |
| `expr parse` | Parse a Haskell-style expression and print the AST |
| `expr eval` | Parse and evaluate an expression, print result as JSON |
| `expr fmt` | Parse and pretty-print in canonical form |
| `expr check` | Validate expression syntax and report errors |
| `expr gat-eval` | Evaluate a JSON-encoded GAT term from a file |
| `expr gat-check` | Type-check a JSON-encoded GAT term against a theory |

## License

[MIT](../../LICENSE)
