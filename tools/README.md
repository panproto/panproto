# Grammar maintenance tools

These scripts read the 261 entries in the workspace-level `grammars.toml` and update files under `grammars/`. Run them from the repository root.

## Requirements

- Python 3.11 or newer
- Git
- The `tree-sitter` CLI when a repository does not contain generated parser data

The scripts clone the current default branch of each upstream repository. `grammars.toml` does not pin revisions. `fetch-grammars.py` records the resolved commit in `grammars/<name>/REVISION` after a successful fetch.

## `fetch-grammars.py`

```bash
python3 tools/fetch-grammars.py
python3 tools/fetch-grammars.py python rust
python3 tools/fetch-grammars.py --dry-run
python3 tools/fetch-grammars.py --clean
```

With no language arguments, the script processes all manifest entries. It copies generated parser sources, `node-types.json`, C and C++ scanners, headers, other source-side JSON files, and available query files. It runs `tree-sitter generate` when `parser.c` is absent but a grammar source is present. It also rewrites selected `common/` includes and copies unresolved local headers from sibling grammar directories after the fetch.

`--clean` deletes the entire `grammars/` directory before fetching, including the two grammars authored in this repository. Use it only when a complete refetch is intended.

The license check is a text heuristic. When the script finds a recognized license file, it rejects the grammar unless the text contains one of its permitted markers. A missing license file is not rejected. Any fetch failure makes the command exit with status 1.

## `fetch-grammar-json.py`

```bash
python3 tools/fetch-grammar-json.py
python3 tools/fetch-grammar-json.py rust go
python3 tools/fetch-grammar-json.py --skip-existing
```

This script updates only `grammars/<name>/src/grammar.json`. It copies an upstream `src/grammar.json` or runs `tree-sitter generate` from `grammar.js`. A repository with neither source is reported as `no-source` but does not make the command fail. Clone or generation failures exit with status 1. Unknown requested names exit with status 2.

## `fetch-query-files.py`

```bash
python3 tools/fetch-query-files.py
python3 tools/fetch-query-files.py python rust
python3 tools/fetch-query-files.py --skip-existing
python3 tools/fetch-query-files.py --dry-run
```

This script copies `.scm` files from an upstream `queries/` directory into `grammars/<name>/queries/`. For a multi-grammar repository it checks the configured subdirectory first, then the repository root. Nested query files are flattened by filename, and an existing destination file wins on a collision.

Unknown names exit with status 1. Clone failures and missing query directories are listed in the final report, but the current script still exits with status 0 for those per-grammar failures.

## `fetch-corpus.py`

```bash
python3 tools/fetch-corpus.py
python3 tools/fetch-corpus.py python rust
python3 tools/fetch-corpus.py --all
```

With no arguments or with `--all`, this script processes every manifest entry. It copies `.txt` and `.scm` corpus files from an upstream `test/corpus/` or `corpus/` directory into `grammars/<name>/test/corpus/`. If the destination already contains a `.txt` file, the grammar is left unchanged.

Unknown names, clone failures, missing corpora, and empty corpora are reported and skipped. The current script does not return a nonzero status for those conditions.
