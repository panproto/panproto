# panproto-cli

[![crates.io](https://img.shields.io/crates/v/panproto-cli.svg)](https://crates.io/crates/panproto-cli)

Command-line interface for panproto. The binary is called `schema`.

Provides subcommands for schema validation, migration checking, breaking change detection, record lifting, protolens-based data conversion, and schematic version control.

## Installation

```sh
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
schema convert --src-schema old.json --tgt-schema new.json record.json

# Auto-generate a lens with human-readable summary
schema lens --src old.json --tgt new.json
schema lens --src old.json --tgt new.json -o lens.json

# Apply a saved lens or protolens chain to data
schema lens-apply --lens lens.json record.json

# Verify lens laws (GetPut, PutGet) and naturality
schema lens-verify --lens lens.json --instance test.json

# Compose lenses or protolens chains
schema lens-compose lens1.json lens2.json -o composed.json

# Derive a lens from VCS commit history
schema lens-diff HEAD~1 HEAD
schema lens-diff abc123 def456

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

# Lift a record through a migration
schema lift --migration mig.json --src-schema src.json --tgt-schema tgt.json record.json
```

## Subcommands

| Command | Description |
|---------|-------------|
| `validate` | Validate a schema file against a protocol (also type-checks the protocol theory) |
| `check` | Check existence conditions for a migration (`--typecheck` to also GAT-validate the morphism) |
| `diff` | Diff two schemas and report structural changes (`--theory` to show sort/op/equation-level diff) |
| `convert` | One-step data conversion between schemas via auto-generated protolens |
| `lens` | Auto-generate a lens between two schemas with human-readable summary |
| `lens-apply` | Apply a saved lens or protolens chain to data |
| `lens-verify` | Verify lens laws (GetPut, PutGet) and naturality on a test instance |
| `lens-compose` | Compose two lenses or protolens chains into one |
| `lens-diff` | Derive a lens from VCS commit history between two refs |
| `lift` | Apply a migration to a record |
| `scaffold` | Generate minimal test data from a protocol theory via free model construction |
| `normalize` | Simplify a schema by merging equivalent elements via quotient |
| `typecheck` | Type-check a migration between two schemas at the GAT level |
| `verify` | Verify a schema satisfies its protocol theory's equations by model checking |
| `init` | Initialize a `.panproto/` repository |
| `add` | Stage a schema for the next commit |
| `commit` | Commit staged changes (`--skip-verify` to bypass GAT equation verification) |
| `status` | Show working state |
| `log` | Walk commit history |
| `show` | Inspect an object |
| `branch` | Create, list, or delete branches |
| `tag` | Create, list, or delete tags |
| `checkout` | Switch branch or detach HEAD |
| `merge` | Three-way schema merge (`--verbose` to show pullback overlap detection details) |
| `rebase` | Replay commits onto another branch |
| `cherry-pick` | Apply a single commit's migration |
| `reset` | Move HEAD / unstage / restore |
| `stash` | Save or restore working state |
| `reflog` | Show ref mutation history |
| `bisect` | Binary search for breaking commit |
| `blame` | Show which commit introduced an element |
| `gc` | Garbage collect unreachable objects |

## License

[MIT](../../LICENSE)
