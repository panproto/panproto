# panproto-cli

[![crates.io](https://img.shields.io/crates/v/panproto-cli.svg)](https://crates.io/crates/panproto-cli)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

The `schema` command-line tool for schematic version control.

## What it does

`schema` is a git-style CLI for managing schemas and their migrations. You initialize a repository, add and commit schema changes, branch and merge schema histories, and diff any two points in time to see what changed and whether the change breaks existing clients.

Beyond version control, `schema` includes tools for the full schema lifecycle: validate a schema file against a protocol, check whether a migration preserves all required structure, scaffold minimal test data from a protocol definition, and generate or verify lens specifications. Data operations let you convert records between formats and migrate instances forward or backward through schema history. The parse subcommands expose full-AST parsing of source files (248 languages) or entire project directories into panproto schemas. The git bridge imports an existing git repository's history into panproto and can export back. An expression REPL lets you interactively evaluate panproto expressions against live data.

## Quick example

```sh
# Initialize a schema repository.
schema init

# Parse a TypeScript project into a panproto schema and commit it.
schema parse project ./src --output schema.json
schema add schema.json
schema commit -m "initial schema from src/"

# After changing the project, diff what changed.
schema diff HEAD schema-v2.json

# Check if the change breaks any existing clients.
schema check --src schema-v1.json --tgt schema-v2.json --mapping migration.json

# Generate a lens between two schema versions.
schema lens generate --src schema-v1.json --tgt schema-v2.json

# Import a git repo's full history into panproto.
schema git import ./my-git-repo
```

## Command groups

| Command | What it does |
|---------|-------------|
| `schema init` | Initialize a panproto repository |
| `schema add` | Stage a schema for the next commit |
| `schema commit` | Create a commit from staged changes |
| `schema status` | Show staged changes and branch state |
| `schema log` | Walk commit history |
| `schema diff` | Diff two schemas or two commits |
| `schema branch` | Create, list, or delete branches |
| `schema merge` | Merge two branches |
| `schema checkout` | Switch branches or restore a past schema |
| `schema validate` | Validate a schema against a protocol |
| `schema check` | Check existence conditions for a migration |
| `schema scaffold` | Generate minimal test instances from a protocol |
| `schema normalize` | Simplify a schema by merging equivalent elements |
| `schema lens generate` | Auto-generate a protolens between two schemas |
| `schema lens apply` | Apply a lens to an instance |
| `schema lens verify` | Check get-put and put-get lens laws |
| `schema lens compose` | Compose two lens files |
| `schema data convert` | Convert an instance between formats |
| `schema data migrate` | Migrate an instance forward or backward |
| `schema parse file` | Parse a single source file to a schema |
| `schema parse project` | Parse a directory tree to a unified schema |
| `schema git import` | Import a git repository into panproto-vcs |
| `schema git export` | Export panproto-vcs commits back to git |
| `schema expr eval` | Evaluate a panproto expression |
| `schema expr repl` | Start an interactive expression REPL |
| `schema theory` | Compile and check GAT theory definitions |

## Installation

```sh
# Homebrew
brew install panproto/tap/schema

# Shell installer
curl -fsSL https://get.panproto.dev | sh

# Cargo
cargo install panproto-cli
```

## License

[MIT](../../LICENSE)
