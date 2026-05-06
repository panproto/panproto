# Parse full ASTs

panproto can parse source code in 248 languages via tree-sitter and treat the full AST as a schema instance. The resulting instance can be queried, diffed, migrated, and version-controlled like any other schema.

## Prerequisites

The `schema` CLI, or the Rust SDK with the `full-parse` feature enabled. For the Python SDK, the relevant grammar pack (the wheel ships eleven core languages; install `panproto-grammars-functional`, `-web`, `-systems`, etc. for more).

## The task

### Single file

```sh
schema parse file --in src/main.rs --out main.ast.json
```

The output is the AST as a JSON instance against an auto-derived GAT theory for the language.

### Whole project

```sh
schema parse project --root . --out project.ast.json
```

Walks every recognised file in the project, parses each with the appropriate grammar, and produces a single instance covering the whole project.

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
