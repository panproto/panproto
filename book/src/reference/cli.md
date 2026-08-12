# CLI reference

Every `schema` subcommand, with its full `--help` text. This page is regenerated
from the live binary by `xtask/src/bin/gen-cli-docs.rs`; edit the CLI, not the
page.

To regenerate locally:

```sh
cargo run -p xtask --bin gen-cli-docs
```

CI runs the same command and fails if the result differs from what is checked in.

For the model that the commands operate on, see
[Schemas as theories](../explanation/schemas-as-theories.md),
[Migrations as morphisms](../explanation/migrations-as-morphisms.md), and
[Schema version control semantics](../explanation/vcs-semantics.md).

## `schema`

```text
Schematic version control: schema migration toolkit based on generalized algebraic theories

Usage: schema [OPTIONS] <COMMAND>

Commands:
  validate      Validate a schema against a protocol
  check         Check existence conditions for a migration between two schemas
  compat        Classify backward-compatibility between two schema versions
  scaffold      Generate minimal test data from a protocol theory using free model construction
  normalize     Simplify a schema by merging equivalent elements
  typecheck     Type-check a migration between two schemas at the GAT level
  verify        Verify that a schema satisfies its protocol theory's equations
  init          Initialize a new panproto repository
  add           Stage a schema for the next commit
  commit        Create a new commit from staged changes
  status        Show repository status
  log           Show commit history
  diff          Diff two schemas or show staged changes
  show          Inspect a commit, schema, or migration object
  branch        Create, list, or delete branches
  tag           Create, list, or delete tags
  checkout      Switch to a branch or commit
  merge         Merge a branch into the current branch
  rebase        Replay current branch onto another
  cherry-pick   Apply a single commit's migration to the current branch
  reset         Move HEAD / unstage / restore
  stash         Save or restore working state
  reflog        Show ref mutation history
  bisect        Binary search for the commit that introduced a breaking change
  blame         Show which commit introduced a schema element
  lift          Apply a migration to a record, transforming it from source to target schema
  integrate     Integrate two schemas by computing their pushout
  auto-migrate  Automatically discover a migration between two schemas
  gc            Garbage collect unreachable objects
  expr          Evaluate, type-check, or interactively explore GAT expressions
  enrich        Add, list, or remove schema enrichments (defaults, coercions, mergers, policies)
  remote        Add, list, or remove remote repositories
  push          Push schemas to a remote repository
  pull          Pull schemas from a remote repository
  fetch         Fetch schemas from a remote repository
  clone         Clone a remote repository
  data          Data operations: migrate, convert, sync, and status
  theory        Theory DSL operations: define theories, morphisms, and protocols from data files
  lens          Bidirectional lens operations
  parse         Parse source files into full-AST schemas via tree-sitter
  git           Import/export between git repositories and panproto-vcs
  help          Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
  -V, --version  Print version
```

### `schema validate`

```text
Validate a schema against a protocol

Usage: schema validate [OPTIONS] --protocol <PROTOCOL> <SCHEMA>

Arguments:
  <SCHEMA>  Path to the schema JSON file

Options:
      --protocol <PROTOCOL>  The protocol name (e.g., "atproto")
  -v, --verbose              Enable verbose output
  -h, --help                 Print help
```

### `schema check`

```text
Check existence conditions for a migration between two schemas

Usage: schema check [OPTIONS] --src <SRC> --tgt <TGT> --mapping <MAPPING>

Options:
      --src <SRC>          Path to the source schema JSON file
  -v, --verbose            Enable verbose output
      --tgt <TGT>          Path to the target schema JSON file
      --mapping <MAPPING>  Path to the migration mapping JSON file
      --typecheck          Also type-check the migration morphism at the GAT level
  -h, --help               Print help
```

### `schema compat`

```text
Classify backward-compatibility between two schema versions.

Runs a structural diff then classifies it against the named protocol, printing the changes grouped by tier. Exit codes: `0` when no breaking changes are found, `1` when at least one breaking change is found, and `2` on a usage or load error (unreadable file, unknown protocol, or bad `--format`).

Usage: schema compat [OPTIONS] --protocol <PROTOCOL> <OLD> <NEW>

Arguments:
  <OLD>
          Path to the old schema JSON file

  <NEW>
          Path to the new schema JSON file

Options:
      --protocol <PROTOCOL>
          The protocol name (e.g., "atproto")

  -v, --verbose
          Enable verbose output

      --format <FORMAT>
          Output format: `text` (default) or `json`
          
          [default: text]

  -h, --help
          Print help (see a summary with '-h')
```

### `schema scaffold`

```text
Generate minimal test data from a protocol theory using free model construction

Usage: schema scaffold [OPTIONS] --protocol <PROTOCOL> <SCHEMA>

Arguments:
  <SCHEMA>  Path to the schema JSON file

Options:
      --protocol <PROTOCOL>    The protocol name (e.g., "atproto")
  -v, --verbose                Enable verbose output
      --depth <DEPTH>          Maximum term generation depth (default: 3) [default: 3]
      --max-terms <MAX_TERMS>  Maximum terms per sort (default: 1000) [default: 1000]
      --json                   Output as JSON
  -h, --help                   Print help
```

### `schema normalize`

```text
Simplify a schema by merging equivalent elements

Usage: schema normalize [OPTIONS] --protocol <PROTOCOL> <SCHEMA>

Arguments:
  <SCHEMA>  Path to the schema JSON file

Options:
      --protocol <PROTOCOL>         The protocol name (e.g., "atproto")
  -v, --verbose                     Enable verbose output
      --identify <IDENTIFICATIONS>  Pairs of elements to identify, as "A=B"
      --json                        Output as JSON
  -h, --help                        Print help
```

### `schema typecheck`

```text
Type-check a migration between two schemas at the GAT level

Usage: schema typecheck [OPTIONS] --src <SRC> --tgt <TGT> --migration <MIGRATION>

Options:
      --src <SRC>              Path to the source schema JSON file
  -v, --verbose                Enable verbose output
      --tgt <TGT>              Path to the target schema JSON file
      --migration <MIGRATION>  Path to the migration mapping JSON file
  -h, --help                   Print help
```

### `schema verify`

```text
Verify that a schema satisfies its protocol theory's equations

Usage: schema verify [OPTIONS] --protocol <PROTOCOL> <SCHEMA>

Arguments:
  <SCHEMA>  Path to the schema JSON file

Options:
      --protocol <PROTOCOL>
          The protocol name (e.g., "atproto")
  -v, --verbose
          Enable verbose output
      --max-assignments <MAX_ASSIGNMENTS>
          Maximum assignments to check per equation (default: 10000) [default: 10000]
  -h, --help
          Print help
```

### `schema init`

```text
Initialize a new panproto repository

Usage: schema init [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to initialize (defaults to current dir) [default: .]

Options:
  -b, --initial-branch <INITIAL_BRANCH>  Use the given name for the initial branch
  -v, --verbose                          Enable verbose output
  -h, --help                             Print help
```

### `schema add`

```text
Stage a schema for the next commit

Usage: schema add [OPTIONS] <SCHEMA>

Arguments:
  <SCHEMA>  Path to the schema JSON file

Options:
  -n, --dry-run      Show what would be staged without actually staging
  -v, --verbose      Enable verbose output
  -f, --force        Force staging even if validation fails
      --data <DATA>  Stage data files alongside the schema
      --skip-verify  Skip GAT migration validation while staging (leaves the stage pending; the migration is still recorded)
  -h, --help         Print help
```

### `schema commit`

```text
Create a new commit from staged changes

Usage: schema commit [OPTIONS] --message <MESSAGE>

Options:
  -m, --message <MESSAGE>  Commit message
  -v, --verbose            Enable verbose output
      --author <AUTHOR>    Author name [default: anonymous]
      --amend              Amend the previous commit instead of creating a new one
      --allow-empty        Allow creating a commit with no changes
      --skip-verify        Skip GAT equation verification
  -h, --help               Print help
```

### `schema status`

```text
Show repository status

Usage: schema status [OPTIONS]

Options:
  -s, --short        Show output in short format
  -v, --verbose      Enable verbose output
      --porcelain    Show output in machine-readable format
  -b, --branch       Show branch information
      --data <DATA>  Show data staleness for files in this directory
  -h, --help         Print help
```

### `schema log`

```text
Show commit history

Usage: schema log [OPTIONS]

Options:
  -n, --limit <LIMIT>    Maximum number of commits to show
  -v, --verbose          Enable verbose output
      --oneline          Show each commit on a single line
      --graph            Show an ASCII graph of the branch structure
      --all              Show all branches, not just the current one
      --format <FORMAT>  Pretty-print commits using a format string
      --author <AUTHOR>  Filter commits by author
      --grep <GREP>      Filter commits whose message matches a pattern
      --data             Show data and complement IDs in commit history
  -h, --help             Print help
```

### `schema diff`

```text
Diff two schemas or show staged changes

Usage: schema diff [OPTIONS] [OLD] [NEW]

Arguments:
  [OLD]  Path to the old schema (or first ref)
  [NEW]  Path to the new schema (or second ref)

Options:
      --stat            Show a diffstat summary
  -v, --verbose         Enable verbose output
      --name-only       Show only names of changed elements
      --name-status     Show names and status (A/D/M) of changed elements
      --staged          Diff the staged schema against HEAD
      --detect-renames  Detect likely renames between schemas
      --theory          Show theory-level diff (sorts, operations, equations)
      --lens            Also generate a protolens chain between the schemas
      --save <SAVE>     Save the protolens chain to a file (requires --lens)
      --optic-kind      Show the optic classification of the diff
  -h, --help            Print help
```

### `schema show`

```text
Inspect a commit, schema, or migration object

Usage: schema show [OPTIONS] <TARGET>

Arguments:
  <TARGET>  Ref name or object ID

Options:
      --format <FORMAT>  Pretty-print using a format string
  -v, --verbose          Enable verbose output
      --stat             Show a diffstat summary for commits
  -h, --help             Print help
```

### `schema branch`

```text
Create, list, or delete branches

Usage: schema branch [OPTIONS] [NAME]

Arguments:
  [NAME]  Branch name to create. Lists branches if omitted

Options:
  -d, --delete         Delete the branch
  -D                   Force-delete the branch even if not fully merged
  -f, --force          Force overwrite if branch already exists
  -m, --move <RENAME>  Rename a branch (value is the new name)
  -v, --verbose        Show commit info for each branch
  -a, --all            List both local and remote-tracking branches
  -h, --help           Print help
```

### `schema tag`

```text
Create, list, or delete tags

Usage: schema tag [OPTIONS] [NAME]

Arguments:
  [NAME]  Tag name to create. Lists tags if omitted

Options:
  -d, --delete             Delete the tag
  -v, --verbose            Enable verbose output
  -a, --annotate           Create an annotated tag
  -m, --message <MESSAGE>  Tag message (implies --annotate)
  -l, --list               List tags matching a pattern
  -f, --force              Force-replace an existing tag
  -h, --help               Print help
```

### `schema checkout`

```text
Switch to a branch or commit

Usage: schema checkout [OPTIONS] <TARGET>

Arguments:
  <TARGET>  Branch name or commit ID

Options:
  -b                       Create a new branch with the given name at HEAD and switch to it
  -v, --verbose            Enable verbose output
      --detach             Detach HEAD at the target commit
      --migrate <MIGRATE>  Migrate data in this directory to match the target branch's schema
  -h, --help               Print help
```

### `schema merge`

```text
Merge a branch into the current branch

Usage: schema merge [OPTIONS] [BRANCH]

Arguments:
  [BRANCH]  Branch to merge

Options:
      --author <AUTHOR>    Author name [default: anonymous]
      --no-commit          Perform the merge but do not commit
      --ff-only            Refuse to merge unless fast-forward is possible
      --no-ff              Create a merge commit even for fast-forward merges
      --squash             Squash the branch into a single change set
      --abort              Abort an in-progress merge
  -m, --message <MESSAGE>  Custom merge commit message
  -v, --verbose            Show pullback-based overlap detection details
      --migrate <MIGRATE>  Migrate data in this directory through the merge
  -h, --help               Print help
```

### `schema rebase`

```text
Replay current branch onto another

Usage: schema rebase [OPTIONS] [ONTO]

Arguments:
  [ONTO]  Branch or commit to rebase onto

Options:
      --author <AUTHOR>  Author name [default: anonymous]
  -v, --verbose          Enable verbose output
      --abort            Abort the current rebase operation
      --cont             Continue a paused rebase after resolving conflicts
  -h, --help             Print help
```

### `schema cherry-pick`

```text
Apply a single commit's migration to the current branch

Usage: schema cherry-pick [OPTIONS] [COMMIT]

Arguments:
  [COMMIT]  Commit ID to cherry-pick

Options:
      --author <AUTHOR>  Author name [default: anonymous]
  -v, --verbose          Enable verbose output
  -n, --no-commit        Apply the change without committing
  -x                     Append "(cherry picked from commit ...)" to the message
      --abort            Abort the current cherry-pick operation
  -h, --help             Print help
```

### `schema reset`

```text
Move HEAD / unstage / restore

Usage: schema reset [OPTIONS] <TARGET>

Arguments:
  <TARGET>  Target ref or commit ID

Options:
      --soft             Soft reset: move HEAD only, keep staged and working changes
  -v, --verbose          Enable verbose output
      --hard             Hard reset: move HEAD, discard all changes
      --author <AUTHOR>  Author name [default: anonymous]
  -h, --help             Print help
```

### `schema stash`

```text
Save or restore working state

Usage: schema stash [OPTIONS] <COMMAND>

Commands:
  push   Save the current staged schema
  pop    Restore the most recent stash
  list   List all stash entries
  drop   Drop the most recent stash
  apply  Apply a stash entry without removing it
  show   Show the contents of a stash entry
  clear  Remove all stash entries
  help   Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema stash push`

```text
Save the current staged schema

Usage: schema stash push [OPTIONS]

Options:
  -m, --message <MESSAGE>  Optional stash message
  -v, --verbose            Enable verbose output
      --author <AUTHOR>    Author name [default: anonymous]
  -h, --help               Print help
```

#### `schema stash pop`

```text
Restore the most recent stash

Usage: schema stash pop [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema stash list`

```text
List all stash entries

Usage: schema stash list [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema stash drop`

```text
Drop the most recent stash

Usage: schema stash drop [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema stash apply`

```text
Apply a stash entry without removing it

Usage: schema stash apply [OPTIONS] [INDEX]

Arguments:
  [INDEX]  Stash index to apply [default: 0]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema stash show`

```text
Show the contents of a stash entry

Usage: schema stash show [OPTIONS] [INDEX]

Arguments:
  [INDEX]  Stash index to inspect [default: 0]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema stash clear`

```text
Remove all stash entries

Usage: schema stash clear [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema reflog`

```text
Show ref mutation history

Usage: schema reflog [OPTIONS] [REF_NAME]

Arguments:
  [REF_NAME]  Ref name (defaults to HEAD) [default: HEAD]

Options:
  -n, --limit <LIMIT>  Maximum entries to show
  -v, --verbose        Enable verbose output
      --all            Show reflogs for all refs
  -h, --help           Print help
```

### `schema bisect`

```text
Binary search for the commit that introduced a breaking change

Usage: schema bisect [OPTIONS] <GOOD> <BAD>

Arguments:
  <GOOD>  Known good commit
  <BAD>   Known bad commit

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema blame`

```text
Show which commit introduced a schema element

Usage: schema blame [OPTIONS] --element-type <ELEMENT_TYPE> <ELEMENT_ID>

Arguments:
  <ELEMENT_ID>  Element identifier (vertex ID, edge `"src->tgt"`, or `"vertex_id:sort"`)

Options:
      --element-type <ELEMENT_TYPE>  Element type: vertex, edge, or constraint
  -v, --verbose                      Enable verbose output
      --reverse                      Walk history from the first commit forward
  -h, --help                         Print help
```

### `schema lift`

```text
Apply a migration to a record, transforming it from source to target schema

Usage: schema lift [OPTIONS] --migration <MIGRATION> --src-schema <SRC_SCHEMA> --tgt-schema <TGT_SCHEMA> <RECORD>

Arguments:
  <RECORD>  Path to the record JSON file

Options:
      --migration <MIGRATION>          Path to the migration mapping JSON file
  -v, --verbose                        Enable verbose output
      --src-schema <SRC_SCHEMA>        Path to the source schema JSON file
      --tgt-schema <TGT_SCHEMA>        Path to the target schema JSON file
      --direction <DIRECTION>          Migration direction: restrict (default, `Delta_F`), sigma (`Sigma_F`), or pi (`Pi_F`) [default: restrict]
      --instance-type <INSTANCE_TYPE>  Instance type: wtype (default) or functor [default: wtype]
  -h, --help                           Print help
```

### `schema integrate`

```text
Integrate two schemas by computing their pushout

Usage: schema integrate [OPTIONS] <LEFT> <RIGHT>

Arguments:
  <LEFT>   Path to the left schema JSON file
  <RIGHT>  Path to the right schema JSON file

Options:
      --auto-overlap  Automatically discover the overlap between schemas
  -v, --verbose       Enable verbose output
      --json          Output the integrated schema as JSON
  -h, --help          Print help
```

### `schema auto-migrate`

```text
Automatically discover a migration between two schemas

Usage: schema auto-migrate [OPTIONS] <OLD> <NEW>

Arguments:
  <OLD>  Path to the old/source schema JSON file
  <NEW>  Path to the new/target schema JSON file

Options:
      --monic    Require injective (one-to-one) vertex mapping
  -v, --verbose  Enable verbose output
      --total    Require every source vertex to be mapped; fail on a partial answer
      --span     Report the span even when it covers nothing at all
      --json     Output the span's right leg, a migration out of the apex, as JSON
  -h, --help     Print help
```

### `schema gc`

```text
Garbage collect unreachable objects

Usage: schema gc [OPTIONS]

Options:
      --dry-run  Show what would be deleted without actually deleting
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema expr`

```text
Evaluate, type-check, or interactively explore GAT expressions

Usage: schema expr [OPTIONS] <COMMAND>

Commands:
  gat-eval   Evaluate a JSON-encoded GAT term from a file
  gat-check  Type-check a JSON-encoded GAT term from a file
  repl       Interactive expression REPL
  parse      Parse a Haskell-style expression and print its AST
  eval       Parse and evaluate a Haskell-style expression, printing the result
  fmt        Parse an expression and pretty-print it back in canonical form
  check      Parse an expression and report any syntax errors
  help       Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema expr gat-eval`

```text
Evaluate a JSON-encoded GAT term from a file

Usage: schema expr gat-eval [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the JSON file containing a GAT term

Options:
      --env <ENV>  Path to a JSON file with variable bindings
  -v, --verbose    Enable verbose output
  -h, --help       Print help
```

#### `schema expr gat-check`

```text
Type-check a JSON-encoded GAT term from a file

Usage: schema expr gat-check [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the JSON file containing term, theory, and context

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema expr repl`

```text
Interactive expression REPL

Usage: schema expr repl [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema expr parse`

```text
Parse a Haskell-style expression and print its AST

Usage: schema expr parse [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>  Expression source text

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema expr eval`

```text
Parse and evaluate a Haskell-style expression, printing the result

Usage: schema expr eval [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>  Expression source text

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema expr fmt`

```text
Parse an expression and pretty-print it back in canonical form

Usage: schema expr fmt [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>  Expression source text

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema expr check`

```text
Parse an expression and report any syntax errors

Usage: schema expr check [OPTIONS] <SOURCE>

Arguments:
  <SOURCE>  Expression source text

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema enrich`

```text
Add, list, or remove schema enrichments (defaults, coercions, mergers, policies)

Usage: schema enrich [OPTIONS] <COMMAND>

Commands:
  add-default   Add a default value expression to a vertex
  add-coercion  Add a coercion expression between two vertex kinds
  add-merger    Add a merger expression to a vertex
  add-policy    Add a conflict policy to a vertex
  list          List all enrichments on the HEAD schema
  remove        Remove an enrichment by name
  help          Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema enrich add-default`

```text
Add a default value expression to a vertex

Usage: schema enrich add-default [OPTIONS] --expr <EXPR> <VERTEX>

Arguments:
  <VERTEX>  Vertex name

Options:
      --expr <EXPR>  Default value as JSON
  -v, --verbose      Enable verbose output
  -h, --help         Print help
```

#### `schema enrich add-coercion`

```text
Add a coercion expression between two vertex kinds

Usage: schema enrich add-coercion [OPTIONS] --expr <EXPR> <FROM> <TO>

Arguments:
  <FROM>  Source vertex kind
  <TO>    Target vertex kind

Options:
      --expr <EXPR>  Coercion expression as JSON
  -v, --verbose      Enable verbose output
  -h, --help         Print help
```

#### `schema enrich add-merger`

```text
Add a merger expression to a vertex

Usage: schema enrich add-merger [OPTIONS] --expr <EXPR> <VERTEX>

Arguments:
  <VERTEX>  Vertex name

Options:
      --expr <EXPR>  Merger specification as JSON
  -v, --verbose      Enable verbose output
  -h, --help         Print help
```

#### `schema enrich add-policy`

```text
Add a conflict policy to a vertex

Usage: schema enrich add-policy [OPTIONS] --strategy <STRATEGY> <VERTEX>

Arguments:
  <VERTEX>  Vertex name

Options:
      --strategy <STRATEGY>  Conflict resolution strategy name
  -v, --verbose              Enable verbose output
  -h, --help                 Print help
```

#### `schema enrich list`

```text
List all enrichments on the HEAD schema

Usage: schema enrich list [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema enrich remove`

```text
Remove an enrichment by name

Usage: schema enrich remove [OPTIONS] <NAME>

Arguments:
  <NAME>  Enrichment name or vertex name to remove enrichments from

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema remote`

```text
Add, list, or remove remote repositories

Usage: schema remote [OPTIONS] <COMMAND>

Commands:
  add     Register a new remote
  remove  Remove a remote
  list    List configured remotes
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema remote add`

```text
Register a new remote

Usage: schema remote add [OPTIONS] <NAME> <URL>

Arguments:
  <NAME>  Remote name
  <URL>   Remote URL

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema remote remove`

```text
Remove a remote

Usage: schema remote remove [OPTIONS] <NAME>

Arguments:
  <NAME>  Remote name to remove

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema remote list`

```text
List configured remotes

Usage: schema remote list [OPTIONS]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema push`

```text
Push schemas to a remote repository

Usage: schema push [OPTIONS] [REMOTE] [BRANCH]

Arguments:
  [REMOTE]  Remote name
  [BRANCH]  Branch to push

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema pull`

```text
Pull schemas from a remote repository

Usage: schema pull [OPTIONS] [REMOTE] [BRANCH]

Arguments:
  [REMOTE]  Remote name
  [BRANCH]  Branch to pull

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema fetch`

```text
Fetch schemas from a remote repository

Usage: schema fetch [OPTIONS] [REMOTE]

Arguments:
  [REMOTE]  Remote name

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema clone`

```text
Clone a remote repository

Usage: schema clone [OPTIONS] <URL> [PATH]

Arguments:
  <URL>   Repository URL
  [PATH]  Local path

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema data`

```text
Data operations: migrate, convert, sync, and status

Usage: schema data [OPTIONS] <COMMAND>

Commands:
  migrate  Migrate data to match the current schema version
  convert  Convert data between schemas
  sync     Sync data to match a target schema version via VCS
  status   Report data staleness relative to the current schema version
  help     Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema data migrate`

```text
Migrate data to match the current schema version

Usage: schema data migrate [OPTIONS] <DATA>

Arguments:
  <DATA>  Data directory containing JSON files

Options:
      --protocol <PROTOCOL>  Protocol name (inferred from HEAD commit if omitted)
  -v, --verbose              Enable verbose output
      --range <RANGE>        Migrate between specific commits (default: parent..HEAD)
      --dry-run              Preview without modifying files
  -o, --output <OUTPUT>      Output directory (default: overwrite in place)
      --backward             Migrate backward (requires stored complement)
      --coverage             Apply migration and print coverage statistics
  -h, --help                 Print help
```

#### `schema data convert`

```text
Convert data between schemas

Usage: schema data convert [OPTIONS] --protocol <PROTOCOL> <DATA>

Arguments:
  <DATA>  Data file or directory of JSON files

Options:
      --from <FROM>            Source schema
  -v, --verbose                Enable verbose output
      --to <TO>                Target schema
      --protocol <PROTOCOL>    Protocol name
      --chain <CHAIN>          Pre-built protolens chain JSON (alternative to --from/--to)
  -o, --output <OUTPUT>        Output file or directory
      --direction <DIRECTION>  Direction: "forward" or "backward" [default: forward]
      --defaults <DEFAULTS>    Default values as key=value pairs
  -h, --help                   Print help
```

#### `schema data sync`

```text
Sync data to match a target schema version via VCS

Usage: schema data sync [OPTIONS] <DATA_DIR>

Arguments:
  <DATA_DIR>  Data directory

Options:
      --edits            Store an edit log object in the VCS
  -v, --verbose          Enable verbose output
      --target <TARGET>  Target ref (default: HEAD)
  -h, --help             Print help
```

#### `schema data status`

```text
Report data staleness relative to the current schema version

Usage: schema data status [OPTIONS] <DATA_DIR>

Arguments:
  <DATA_DIR>  Data directory

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema theory`

```text
Theory DSL operations: define theories, morphisms, and protocols from data files

Usage: schema theory [OPTIONS] <COMMAND>

Commands:
  validate             Validate a theory document (load + typecheck)
  compile              Compile a theory document and print results
  compile-dir          Compile all theory documents in a directory
  check-morphism       Validate a morphism document
  recompose            Replay a composition and print the resulting theory
  check-coercion-laws  Run sample-based coercion law checks over every directed equation in a theory document. Exits non-zero when any declared coercion class is falsified by a sample
  repl                 Interactive theory REPL with syntax highlighting and history
  help                 Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema theory validate`

```text
Validate a theory document (load + typecheck)

Usage: schema theory validate [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the theory document file (.ncl, .json, .yaml)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema theory compile`

```text
Compile a theory document and print results

Usage: schema theory compile [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the theory document file

Options:
      --json     Output as JSON
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema theory compile-dir`

```text
Compile all theory documents in a directory

Usage: schema theory compile-dir [OPTIONS] <DIR>

Arguments:
  <DIR>  Path to the directory

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema theory check-morphism`

```text
Validate a morphism document

Usage: schema theory check-morphism [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the morphism document file

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema theory recompose`

```text
Replay a composition and print the resulting theory

Usage: schema theory recompose [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the composition document file

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema theory check-coercion-laws`

```text
Run sample-based coercion law checks over every directed equation in a theory document. Exits non-zero when any declared coercion class is falsified by a sample

Usage: schema theory check-coercion-laws [OPTIONS] <FILE>

Arguments:
  <FILE>  Path to the theory document file

Options:
      --json                 Output the full report as JSON
  -v, --verbose              Enable verbose output
      --var-name <VAR_NAME>  Name under which each sample is bound in the evaluation environment. Defaults to `"x"`; override when a theory's equations bind a different free variable so the checker does not surface "unbound variable" errors on every sample [default: x]
  -h, --help                 Print help
```

#### `schema theory repl`

```text
Interactive theory REPL with syntax highlighting and history

Usage: schema theory repl [OPTIONS]

Options:
      --load <PATH>  Theory documents to load on startup. Same shape accepted by `:load` inside the REPL
  -v, --verbose      Enable verbose output
  -h, --help         Print help
```

### `schema lens`

```text
Bidirectional lens operations

Usage: schema lens [OPTIONS] <COMMAND>

Commands:
  generate  Generate a lens between two schemas
  compile   Compile a lens DSL document (.ncl/.json/.yaml) to a protolens chain
  apply     Apply a saved lens chain to data
  compose   Compose two protolens chains or schemas
  verify    Verify lens laws on test data
  inspect   Inspect a saved protolens chain
  check     Check applicability of a chain against schemas in a directory
  lift      Lift a chain along a theory morphism
  help      Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema lens generate`

```text
Generate a lens between two schemas

Usage: schema lens generate [OPTIONS] --protocol <PROTOCOL> <OLD> <NEW>

Arguments:
  <OLD>
          Path to the old/source schema

  <NEW>
          Path to the new/target schema

Options:
      --protocol <PROTOCOL>
          Protocol name

  -v, --verbose
          Enable verbose output

      --json
          Output as JSON

      --chain
          Output a reusable protolens chain (JSON to stdout)

      --try-overlap
          Try overlap-based alignment when direct morphism fails

      --save <SAVE>
          Save the generated protolens chain to a file

      --defaults <DEFAULTS>
          Default values as key=value pairs

      --fuse
          Fuse multi-step chain into single protolens

      --requirements
          Show complement requirements (defaults/data needed)

      --hints <HINTS>
          Path to a JSON hints file for guided auto-lens generation

      --stringency <TIER>
          Stringency tier governing which alignment strategies run.
          
          Accepted case-insensitively for parity with the Python and WASM bindings, both of which trim and lowercase their input.
          
          `strict`: only kind-exact name equality. `balanced`: alias dictionary + tight token similarity (default). `lenient`: span-search and structural priors. `exploratory`: lossy retraction witnesses.

          Possible values:
          - strict:      Kind-exact, edge-name-pruned CSP search; total morphism only
          - balanced:    Adds alias dictionary and tight token-similarity priors (default)
          - lenient:     Adds span-search and structural priors
          - exploratory: Adds lossy retraction witnesses

      --top-n <N>
          Emit up to N ranked candidate lenses instead of the single best one. Output format switches to a JSON array when combined with `--json` or `--chain`
          
          [default: 1]

      --explain
          Print per-step explanations (and confidences) for each emitted candidate

  -h, --help
          Print help (see a summary with '-h')
```

#### `schema lens compile`

```text
Compile a lens DSL document (.ncl/.json/.yaml) to a protolens chain

Usage: schema lens compile [OPTIONS] <DOC>

Arguments:
  <DOC>  Path to the lens DSL document (`.ncl`, `.json`, `.yaml`, or `.yml`)

Options:
      --body-vertex <BODY_VERTEX>  Parent vertex under which field-level steps attach [default: record:body]
  -v, --verbose                    Enable verbose output
      --out <OUT>                  Write the chain JSON to this file instead of stdout
  -h, --help                       Print help
```

#### `schema lens apply`

```text
Apply a saved lens chain to data

Usage: schema lens apply [OPTIONS] --protocol <PROTOCOL> <CHAIN> <DATA>

Arguments:
  <CHAIN>  Path to the protolens chain JSON
  <DATA>   Path to the data file

Options:
      --protocol <PROTOCOL>      Protocol name
  -v, --verbose                  Enable verbose output
      --direction <DIRECTION>    Direction: "forward" or "backward" [default: forward]
      --complement <COMPLEMENT>  Complement data for backward apply
      --schema <SCHEMA>          Schema for chain instantiation
  -h, --help                     Print help
```

#### `schema lens compose`

```text
Compose two protolens chains or schemas

Usage: schema lens compose [OPTIONS] --protocol <PROTOCOL> <CHAIN1> <CHAIN2>

Arguments:
  <CHAIN1>  First chain or schema file
  <CHAIN2>  Second chain or schema file

Options:
      --protocol <PROTOCOL>  Protocol name
  -v, --verbose              Enable verbose output
      --json                 Output as JSON
      --chain                Output in chain format
  -h, --help                 Print help
```

#### `schema lens verify`

```text
Verify lens laws on test data

Usage: schema lens verify [OPTIONS] --protocol <PROTOCOL> <DATA> [SCHEMA]

Arguments:
  <DATA>    Path to test data file
  [SCHEMA]  Schema file (second schema is optional)

Options:
      --protocol <PROTOCOL>  Protocol name
  -v, --verbose              Enable verbose output
  -h, --help                 Print help
```

#### `schema lens inspect`

```text
Inspect a saved protolens chain

Usage: schema lens inspect [OPTIONS] --protocol <PROTOCOL> <CHAIN>

Arguments:
  <CHAIN>  Path to the protolens chain JSON

Options:
      --protocol <PROTOCOL>  Protocol name
  -v, --verbose              Enable verbose output
  -h, --help                 Print help
```

#### `schema lens check`

```text
Check applicability of a chain against schemas in a directory

Usage: schema lens check [OPTIONS] --protocol <PROTOCOL> <CHAIN> <SCHEMAS_DIR>

Arguments:
  <CHAIN>        Path to the protolens chain JSON
  <SCHEMAS_DIR>  Directory containing schema JSON files

Options:
      --protocol <PROTOCOL>  Protocol name
  -v, --verbose              Enable verbose output
      --dry-run              Report only, do not instantiate
  -h, --help                 Print help
```

#### `schema lens lift`

```text
Lift a chain along a theory morphism

Usage: schema lens lift [OPTIONS] <CHAIN> <MORPHISM>

Arguments:
  <CHAIN>     Path to the protolens chain JSON
  <MORPHISM>  Path to the theory morphism JSON

Options:
      --json     Output as JSON
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema parse`

```text
Parse source files into full-AST schemas via tree-sitter

Usage: schema parse [OPTIONS] <COMMAND>

Commands:
  file     Parse a single source file into a full-AST schema
  project  Parse all files in a directory into a unified project schema
  emit     Parse a file and emit it back to source (round-trip test)
  help     Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema parse file`

```text
Parse a single source file into a full-AST schema

Usage: schema parse file [OPTIONS] <PATH>

Arguments:
  <PATH>  Path to the source file

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema parse project`

```text
Parse all files in a directory into a unified project schema

Usage: schema parse project [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to the project directory [default: .]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema parse emit`

```text
Parse a file and emit it back to source (round-trip test)

Usage: schema parse emit [OPTIONS] <PATH>

Arguments:
  <PATH>  Path to the source file

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

### `schema git`

```text
Import/export between git repositories and panproto-vcs

Usage: schema git [OPTIONS] <COMMAND>

Commands:
  import  Import a git repository's history into panproto-vcs
  export  Export panproto-vcs history to a git repository
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema git import`

```text
Import a git repository's history into panproto-vcs

Usage: schema git import [OPTIONS] <REPO> [REVSPEC]

Arguments:
  <REPO>     Path to the git repository
  [REVSPEC]  Git revspec (e.g. "HEAD", "main", "HEAD~10..HEAD") [default: HEAD]

Options:
  -v, --verbose  Enable verbose output
  -h, --help     Print help
```

#### `schema git export`

```text
Export panproto-vcs history to a git repository

Usage: schema git export [OPTIONS] <DEST>

Arguments:
  <DEST>  Destination path for the git repository

Options:
      --repo <REPO>  Path to the panproto repository (default: current directory) [default: .]
  -v, --verbose      Enable verbose output
  -h, --help         Print help
```

