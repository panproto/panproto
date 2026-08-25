# panproto-cli

[![crates.io](https://img.shields.io/crates/v/panproto-cli.svg)](https://crates.io/crates/panproto-cli)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

The `schema` command-line interface for panproto.

## Scope

The binary exposes schema validation, compatibility, migration, version-control,
expression, theory, lens, source-parser, Git-bridge, data, and XRPC operations. Run
`schema <command> --help` for the argument contract of a particular command. The help
is generated from the same `clap` definitions used for dispatch.

Parser coverage depends on the grammar features compiled into `panproto-parse`. The
CLI manifest does not request `group-all`, so a standalone build uses
`panproto-parse`'s default 11-language `group-core` set. A workspace build can contain
more parsers through Cargo feature unification. A registered grammar does not mean
that canonical emission is verified for arbitrary schemas. Check the parser's
verification status before relying on canonical emission.

The expression REPL evaluates the expression language. It does not automatically
attach a live instance resolver, so graph-query built-ins require an execution path
that supplies that context.

## Examples

```sh
# Inspect a project. This prints a summary, not JSON.
schema parse project ./src

# Stage and commit an existing serialized schema.
schema init
schema add schema-v1.json
schema commit -m "initial schema"

# Check a supplied migration mapping.
schema check \
  --src schema-v1.json \
  --tgt schema-v2.json \
  --mapping migration.json

# Generate a lens. The source and target are positional arguments.
schema lens generate \
  --protocol atproto \
  schema-v1.json schema-v2.json

# Import ancestors of a Git revspec.
schema git import ./my-git-repo HEAD
```

## Top-level commands

| Area | Commands |
|------|----------|
| Schema analysis | `validate`, `check`, `compat`, `scaffold`, `normalize`, `typecheck`, `verify` |
| Repository | `init`, `add`, `commit`, `status`, `log`, `diff`, `show`, `branch`, `tag`, `checkout` |
| History editing | `merge`, `rebase`, `cherry-pick`, `reset`, `stash`, `reflog`, `bisect`, `blame`, `gc` |
| Migration | `lift`, `integrate`, `auto-migrate` |
| Language tools | `expr`, `enrich`, `theory`, `lens`, `parse`, `git`, `data` |
| Remote operations | `remote`, `push`, `pull`, `fetch`, `clone` |

`schema lift` offers `restrict`, `sigma`, and `pi` directions. In the current
implementation, plain `restrict` calls the source-to-target W-type or functor pruning
path. It is not the contravariant `Delta_F` operation. `sigma` uses the separate
source-to-target extension path. Functor `pi` forms products over vertex fibers,
whereas W-type `pi` accepts only vertex-injective mappings and relabels the tree.

## Installation

```sh
cargo install panproto-cli
schema --help
```

## License

[MIT](../../LICENSE)
