# Parse full ASTs

panproto can parse source code in 248 languages via tree-sitter and treat the full AST as a schema instance. The resulting instance can be queried, diffed, migrated, and version-controlled like any other schema.

## Prerequisites

The `schema` CLI, or the Rust SDK with the `full-parse` feature enabled. For the Python SDK, the relevant grammar pack (the wheel ships eleven core languages; install `panproto-grammars-functional`, `-web`, `-systems`, etc. for more).

## The task

### Single file

```sh
schema parse file src/main.rs > main.ast.json
```

`schema parse file <PATH>` writes the AST as a JSON instance against an auto-derived GAT theory for the language to stdout. Redirect to a file as needed.

### Whole project

```sh
schema parse project . > project.ast.json
```

Walks every recognised file in the project (default `.`), parses each with the appropriate grammar, and writes a single instance covering the whole project to stdout.

### Round-trip a single file

```sh
schema parse emit src/main.rs
```

Parses then emits, useful for confirming a clean round-trip through the format-preserving codec.

### From Rust

```rust
use panproto_core::parse::Parser;

let parser = Parser::for_language("rust")?;
let instance = parser.parse_file("src/main.rs")?;
```

### From Python

```python
from panproto import Parser
parser = Parser.for_language("python")
instance = parser.parse_file("src/app.py")
```

## Verification

Tree-sitter parsing is total: every byte sequence parses into *some* AST. `instance.diagnostic_count()` reports the number of error nodes; a clean parse has zero. The interstitial preservation property guarantees `emit(parse(bytes)) == bytes`.

## Common mistakes

- Treating the AST as the source of truth for non-syntactic information. Type information, name resolution, control flow are not modelled by the auto-derived theories.
- Assuming language coverage. The 248-language list is in [`crates/panproto-grammars/`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars). Languages not in the list have no parser.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Round-trip with format preservation](./format-preserving.md).
