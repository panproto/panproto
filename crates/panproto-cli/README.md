# panproto-cli

Command-line interface for panproto.

Provides subcommands for schema validation, migration checking, breaking change detection, and record lifting. Supports all built-in protocols: ATProto, SQL, Protobuf, GraphQL, and JSON Schema.

## Installation

```sh
cargo install panproto-cli
```

## Usage

```sh
# Validate a schema against a protocol
panproto validate --protocol atproto schema.json

# Check migration existence conditions
panproto check --src old.json --tgt new.json --mapping migration.json

# Diff two schemas
panproto diff old.json new.json

# Lift a record through a migration
panproto lift --migration mig.json --src-schema src.json --tgt-schema tgt.json record.json
```

## Subcommands

| Command | Description |
|---------|-------------|
| `validate` | Validate a schema file against a protocol |
| `check` | Check existence conditions for a migration between two schemas |
| `diff` | Diff two schemas and report structural changes |
| `lift` | Apply a migration to a record, transforming it to the target schema |

## Global Flags

- `-v, --verbose` -- Enable verbose output

## License

[MIT](../../LICENSE)
