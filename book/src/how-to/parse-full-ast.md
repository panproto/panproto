# Parse full ASTs

panproto can parse source code in 259 languages via tree-sitter and treat the full AST as a schema instance. The resulting instance can be queried, diffed, migrated, and version-controlled like any other schema.

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
use panproto_core::parse::ParserRegistry;

let registry = ParserRegistry::new();
let schema = registry.parse_with_protocol(
    "rust",
    std::fs::read("src/main.rs")?.as_slice(),
    "src/main.rs",
)?;
```

`panproto_core::parse` is the re-export of `panproto-parse`. `ParserRegistry::new()` populates with every grammar enabled at build time; for a specific file path, `registry.parse_file(path, content)` auto-detects the language by extension.

### From Python

```python
import panproto

reg = panproto.AstParserRegistry()
schema = reg.parse_with_protocol("python", source_bytes, "src/app.py")
```

Companion grammar packs install additional languages: `pip install panproto-grammars-functional`, `-web`, `-systems`, etc.

### Read anonymous-token field values

A tree-sitter rule of the form `field('<name>', choice('+', '-', '*', '/'))` attaches a field name to an *unnamed* token alternative. The walker captures the matched token's text as a `field:<name>` constraint on the parent vertex; `Schema::field_text` is the supported accessor:

```python
schema = reg.parse_with_protocol("qvr", b"let y = log(x)", "demo.qvr")
let_call = next(v.id for v in schema.vertices if v.kind == "let_call")
schema.field_text(let_call, "func")   # -> "log"
```

The Rust equivalent is `Schema::field_text(vertex_id, name) -> Option<&str>`. Named-node field children continue to surface as edges; this accessor is specifically for the anonymous-token field case.

### Override a registered grammar at runtime

Grammar authors iterating on a grammar's `parser.c` / `grammar.json` / `node-types.json` outside the panproto release cadence can swap in a freshly-compiled grammar mid-process. Compile the grammar via `tree-sitter build`, load the resulting shared library with `ctypes`, and pass the integer address of the `tree_sitter_<name>` symbol to `override_grammar`:

```python
import ctypes
import panproto

lib = ctypes.CDLL("./build/qvr.dylib")
language_ptr = ctypes.cast(lib.tree_sitter_qvr, ctypes.c_void_p).value

reg = panproto.AstParserRegistry()
reg.override_grammar(
    name="qvr",
    extensions=["qvr"],
    language_ptr=language_ptr,
    node_types=open("./grammars/qvr/src/node-types.json", "rb").read(),
    grammar_json=open("./grammars/qvr/src/grammar.json", "rb").read(),
)
schema = reg.parse_with_protocol("qvr", source_bytes, "demo.qvr")  # uses the new grammar
```

If a parser is already registered under `name`, it is dropped first (along with any extension mappings). Cannot run while a `ParseEmitLens` produced by `reg.lens(...)` is alive: drop outstanding lens handles, or construct a fresh registry, first. The byte payloads are leaked into `'static` storage on the Rust side — intended for dev-time work, not production.

## Verification

Tree-sitter parsing is total: every byte sequence parses into *some* AST. `instance.diagnostic_count()` reports the number of error nodes; a clean parse has zero. The interstitial preservation property guarantees `emit(parse(bytes)) == bytes`.

## Common mistakes

- Treating the AST as the source of truth for non-syntactic information. Type information, name resolution, control flow are not modelled by the auto-derived theories.
- Assuming language coverage. The 259-language list is in [`crates/panproto-grammars/`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars). Languages not in the list have no parser.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Round-trip with format preservation](./format-preserving.md).
