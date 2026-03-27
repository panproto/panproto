# panproto tools

## fetch-grammars.py

Fetches tree-sitter grammar C sources from git repos based on `grammars.toml`.

### Prerequisites

- Python 3.11+ (for `tomllib`)
- Git
- `tree-sitter` CLI (`cargo install tree-sitter-cli`)

The tree-sitter CLI is needed to generate `parser.c` for grammars that don't
ship pre-generated sources (e.g., Swift, SQL, LaTeX, Perl).

### Usage

```sh
# Fetch all 248 grammars
python3 tools/fetch-grammars.py

# Fetch specific grammars
python3 tools/fetch-grammars.py python rust typescript

# Show what would be fetched
python3 tools/fetch-grammars.py --dry-run

# Clean and re-fetch everything
python3 tools/fetch-grammars.py --clean
```

### What it does

1. Reads `grammars.toml` for repo URLs, revisions, and metadata
2. Shallow-clones each repo
3. Runs `tree-sitter generate` if `parser.c` is missing
4. Copies `src/parser.c`, `src/scanner.c`/`src/scanner.cc`, `src/node-types.json`,
   all `src/*.h` headers, and `src/tree_sitter/*.h` into `grammars/{lang}/src/`
5. Copies shared directories (e.g., `common/`) and rewrites relative includes
6. Resolves cross-grammar header dependencies (e.g., Angular depending on HTML's `tag.h`)
7. Verifies permissive licensing (MIT/Apache/BSD/ISC)
8. Records the resolved git SHA in `grammars/{lang}/REVISION`
