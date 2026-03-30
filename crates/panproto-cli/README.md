# panproto-cli

[![crates.io](https://img.shields.io/crates/v/panproto-cli.svg)](https://crates.io/crates/panproto-cli)

Command-line interface for panproto. The binary is called `schema`.

Provides subcommands for schema validation, migration checking, breaking change detection, record lifting, protolens-based data conversion, and schematic version control.

## Installation

```sh
# macOS (Homebrew)
brew install panproto/tap/panproto-cli

# Linux / macOS (shell installer)
curl --proto '=https' -LsSf https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.sh | sh

# Windows (PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.ps1 | iex"

# From source (any platform with Rust)
cargo install panproto-cli
```

## Usage

```sh
# Version control
schema init                              # initialize repo, auto-detect packages
schema add schema.json                   # stage a JSON schema
schema add crates/panproto-gat/          # stage a directory (parsed via tree-sitter)
schema commit -m "initial schema"
schema commit -m "fix" --skip-verify     # bypass GAT equation checks
schema status                            # per-file changes grouped by package
schema log
schema diff --staged
schema diff --theory old.json new.json
schema show HEAD

# Branching and merging
schema branch feature
schema checkout feature
schema merge main
schema merge main --migrate records/     # merge and migrate data
schema rebase main
schema cherry-pick abc1234
schema stash push
schema stash pop

# Schema tools
schema validate --protocol atproto schema.json
schema check --src old.json --tgt new.json --mapping mig.json
schema typecheck --src old.json --tgt new.json --migration mig.json
schema verify --protocol atproto schema.json
schema scaffold --protocol atproto schema.json
schema normalize --protocol atproto schema.json

# Lens operations
schema lens generate old.json new.json
schema lens apply lens.json record.json
schema lens verify lens.json --instance test.json
schema lens compose lens1.json lens2.json
schema lens inspect chain.json
schema lens check chain.json schemas/
schema lens lift chain.json morphism.json

# Data operations
schema data convert --src-schema old.json --tgt-schema new.json record.json
schema data migrate records/
schema data sync records/
schema data status records/
schema add schema.json --data records/   # stage data alongside schema

# Record lifting
schema lift --migration mig.json --src-schema src.json --tgt-schema tgt.json record.json

# Full-AST parsing (248 languages)
schema parse file src/main.ts
schema parse project ./src
schema parse emit src/main.ts

# Git bridge
schema git import /path/to/repo HEAD
schema git export --repo . /path/to/dest

# Expressions
schema expr eval "2 + 3 * 4"
schema expr parse "\\x -> x + 1"
schema expr fmt "\\x->x+ 1"
schema expr check "let x = 1 in x + 2"
schema expr repl
schema expr gat-eval term.json
schema expr gat-check term.json

# Enrichments
schema enrich add-default --vertex post.title --expr '"untitled"'
schema enrich add-coercion --from string --to integer --expr "str_to_int(x)"
schema enrich list

# History
schema reflog
schema bisect
schema blame --vertex post.title
schema reset --soft HEAD~1
schema gc
```

## Subcommands

| Command | Description |
|---------|-------------|
| `init` | Initialize a `.panproto/` repository (auto-generates `panproto.toml`) |
| `add` | Stage a schema, file, or directory (`--data` to stage data files alongside) |
| `commit` | Commit staged changes (`--skip-verify` to bypass GAT checks, `--amend` to rewrite) |
| `status` | Show per-file changes grouped by package |
| `log` | Walk commit history (`--oneline`, `--grep`, `--format`) |
| `diff` | Diff two schemas or show staged changes (`--theory`, `--stat`, `--name-only`) |
| `show` | Inspect a commit, schema, migration, theory, or theory morphism object |
| `validate` | Validate a schema against a protocol |
| `check` | Check existence conditions for a migration |
| `typecheck` | Type-check a migration at the GAT level |
| `verify` | Verify a schema satisfies its protocol theory's equations |
| `scaffold` | Generate minimal test data via free model construction |
| `normalize` | Simplify a schema by merging equivalent elements |
| `branch` | Create, list, delete, or rename branches |
| `tag` | Create, list, or delete tags (lightweight or annotated) |
| `checkout` | Switch branch or detach HEAD (`--migrate` to migrate data) |
| `merge` | Three-way schema merge via pushout (`--migrate`, `--no-ff`, `--squash`) |
| `rebase` | Replay commits onto another branch |
| `cherry-pick` | Apply a single commit's migration |
| `reset` | Move HEAD / unstage / restore (`--soft`, `--mixed`, `--hard`) |
| `stash` | Save/restore working state (`push`, `pop`, `list`, `drop`, `apply`, `show`, `clear`) |
| `reflog` | Show ref mutation history |
| `bisect` | Binary search for the commit that introduced a breaking change |
| `blame` | Show which commit introduced a schema element |
| `lift` | Apply a migration to a record |
| `integrate` | Compute the pushout of two schemas |
| `auto-migrate` | Automatically discover a migration between two schemas |
| `gc` | Garbage collect unreachable objects |
| `lens generate` | Auto-generate a lens between two schemas |
| `lens apply` | Apply a saved lens or protolens chain to data |
| `lens compose` | Compose two protolens chains |
| `lens verify` | Verify lens laws (GetPut, PutGet) on test data |
| `lens inspect` | Print human-readable summary of a protolens chain |
| `lens check` | Check applicability of a chain against schemas |
| `lens lift` | Lift a chain along a theory morphism |
| `data convert` | One-step data conversion between schemas |
| `data migrate` | Migrate data to match current schema version |
| `data sync` | Sync data to target schema version via VCS |
| `data status` | Report data staleness |
| `parse file` | Parse a single source file into a structural schema |
| `parse project` | Parse a directory into a unified project schema |
| `parse emit` | Round-trip: parse then emit back to source |
| `git import` | Import git history into panproto-vcs |
| `git export` | Export panproto-vcs history to a git repository |
| `expr eval` | Parse and evaluate an expression |
| `expr parse` | Parse an expression and print its AST |
| `expr fmt` | Pretty-print an expression in canonical form |
| `expr check` | Validate expression syntax |
| `expr repl` | Interactive expression REPL |
| `expr gat-eval` | Evaluate a JSON-encoded GAT term |
| `expr gat-check` | Type-check a JSON-encoded GAT term |
| `enrich add-default` | Add a default value expression to a vertex |
| `enrich add-coercion` | Add a coercion expression between vertex kinds |
| `enrich add-merger` | Add a merger expression to a vertex |
| `enrich add-policy` | Add a conflict policy to a vertex |
| `enrich list` | List all enrichments on the HEAD schema |
| `enrich remove` | Remove an enrichment by name |
| `remote add/remove/list` | Manage remote repositories |
| `push` | Push schemas to a remote repository |
| `pull` | Pull schemas from a remote repository |
| `fetch` | Fetch schemas from a remote repository |
| `clone` | Clone a remote repository |

## License

[MIT](../../LICENSE)
