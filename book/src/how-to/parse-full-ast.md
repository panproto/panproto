# Parse full ASTs

panproto can parse source code in 261 languages via tree-sitter and treat the full AST as a schema instance. The resulting instance can be queried, diffed, migrated, and version-controlled like any other schema.

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

```rust,no_run
use panproto_core::parse::ParserRegistry;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let registry = ParserRegistry::new();
let schema = registry.parse_with_protocol(
    "rust",
    std::fs::read("src/main.rs")?.as_slice(),
    "src/main.rs",
)?;
# let _ = schema;
# Ok(()) }
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

If a parser is already registered under `name`, it is dropped first (along with any extension mappings). Cannot run while a `ParseEmitLens` produced by `reg.lens(...)` is alive: drop outstanding lens handles, or construct a fresh registry, first. The byte payloads are leaked into `'static` storage on the Rust side (intended for dev-time work, not production).

## Going the other way

The schema you get back from `parse_with_protocol` carries a complete *layout fibre*: byte spans, the whitespace between every pair of adjacent tokens, and discriminators recording which CHOICE alternative the parser took at each branch point. The emitter consumes those constraints to render bytes back. A schema you build by hand from `SchemaBuilder` carries none of them; `emit_pretty_with_protocol` falls back to a grammar walk driven by the structural acceptance predicate, a layered cassette system, and per-position interstitial scoring (see [Source-code emission](../explanation/emit-pretty.md) for the mechanics). The grammar walk produces structurally valid output for the verified set and best-effort output for the rest.

For generators that build a schema from scratch and want to render it to source bytes, see [Decorate an abstract schema](./decorate-schemas.md). The `decorate` operation takes an `AbstractSchema` (the hand-built half), attaches the layout fibre via a grammar walk, and returns a `DecoratedSchema` the emitter can render byte-for-byte.

### Verifying the emitter for a protocol

Before relying on `emit_pretty_with_protocol` in a downstream pipeline, ask the registry which tier the protocol falls into:

```rust,no_run
use panproto_parse::{EmitVerificationStatus, ParserRegistry};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let reg = ParserRegistry::new();
match reg.emit_verification_status("python") {
    EmitVerificationStatus::Verified => { /* round-trips its full corpus */ }
    EmitVerificationStatus::Generic  => { /* registered, but unverified */ }
    EmitVerificationStatus::Unsupported => { /* not registered */ }
}
# Ok(()) }
```

The 186 protocols currently in the `Verified` set are listed in [`crates/panproto-parse/src/registry.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-parse/src/registry.rs) under `VERIFIED_EMIT_PROTOCOLS`. A protocol earns the tier by round-tripping its grammar author's full `test/corpus/` under the strict `emit_corpus_audit` oracle (`emit(parse(emit(s))) == emit(s)` plus vertex-kind and edge-shape multiset preservation on every entry), or by being pinned to a quivers transpile backend test.

## Verification

Tree-sitter parsing is total: every byte sequence parses into *some* AST, with error nodes inserted around unparseable spans. The verified guarantee is a round-trip up to the vertex-kind and edge-shape multiset: `emit(parse(bytes))` re-parses to the same abstract syntax tree, which `check_parse_emit` in `panproto_parse::parse_emit_lens` asserts. Interstitial preservation makes that emit reproduce the original whitespace byte-for-byte for most inputs, but some legitimately reformat (json re-indents arrays), so byte equality is not promised universally. `schema parse emit <file>` is the smoke test.

## Common mistakes

- Treating the AST as the source of truth for non-syntactic information. Type information, name resolution, control flow are not modelled by the auto-derived theories.
- Assuming language coverage. The 261-language list is in [`crates/panproto-grammars/`](https://github.com/panproto/panproto/tree/main/crates/panproto-grammars). Languages not in the list have no parser.

## See also

- [Decorate an abstract schema](./decorate-schemas.md) for the put-direction of the parse / emit lens.
- [Source-code emission](../explanation/emit-pretty.md) for the emitter's structural pipeline.
- [Reference: protocol catalogue](../reference/protocols.md).
- [Round-trip with format preservation](./format-preserving.md).
- [Layout enrichment](../explanation/layout-enrichment.md) for the categorical framing of the parse / decorate / emit pair.
