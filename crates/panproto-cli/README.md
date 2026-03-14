# panproto-cli

Command-line interface for panproto. The binary is called `schema`.

Provides subcommands for schema validation, migration checking, breaking change detection, record lifting, and schematic version control (init, commit, branch, merge, rebase, cherry-pick, bisect, blame, and more).

## Installation

```sh
cargo install panproto-cli
```

## Usage

```sh
# Validate a schema against a protocol
schema validate --protocol atproto schema.json

# Diff two schemas
schema diff old.json new.json

# Initialize a schema repository and commit
schema init
schema add schema.json
schema commit -m "initial schema"

# Branch, evolve, merge
schema branch feature
schema checkout feature
schema add schema-v2.json
schema commit -m "add field"
schema checkout main
schema merge feature

# Lift a record through a migration
schema lift --migration mig.json --src-schema src.json --tgt-schema tgt.json record.json
```

## Subcommands

| Command | Description |
|---------|-------------|
| `validate` | Validate a schema file against a protocol |
| `check` | Check existence conditions for a migration |
| `diff` | Diff two schemas and report structural changes |
| `lift` | Apply a migration to a record |
| `init` | Initialize a `.panproto/` repository |
| `add` | Stage a schema for the next commit |
| `commit` | Commit staged changes |
| `status` | Show working state |
| `log` | Walk commit history |
| `show` | Inspect an object |
| `branch` | Create, list, or delete branches |
| `tag` | Create, list, or delete tags |
| `checkout` | Switch branch or detach HEAD |
| `merge` | Three-way schema merge |
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
