# Parse full ASTs

Full-AST parsing converts source code into a panproto `Schema` derived from its tree-sitter syntax tree. The available languages are the grammars compiled into the current binary or SDK package.

## Prerequisites

The `schema` CLI or the Rust SDK with `full-parse`. The default `panproto-parse` build enables the eleven-language core grammar group; feature groups and companion Python packages can add more, up to the full grammar catalog.

## Inspect a file from the CLI

```sh
schema parse file src/main.rs
```

The command prints a summary containing the detected language and the vertex and edge counts. It does not serialize the AST schema to stdout.

To inspect a directory:

```sh
schema parse project .
```

This command builds a project schema and prints aggregate counts plus the detected protocol for each recognized path. It also does not emit project-schema JSON. Use the Rust or Python API when the caller needs the `Schema` value itself.

## Check source replay

```sh
schema parse emit src/main.rs > /tmp/main.replayed.rs
cmp src/main.rs /tmp/main.replayed.rs
```

`parse emit` parses the file and calls the parse-side emitter on the resulting schema. It writes only the emitted bytes to stdout, which makes the `cmp` check reliable.

## Parse from Rust

```rust,no_run
use std::path::Path;
use panproto_core::parse::ParserRegistry;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read("src/main.rs")?;
    let registry = ParserRegistry::new();

    let schema = registry.parse_file(Path::new("src/main.rs"), &source)?;
    println!("{} vertices", schema.vertex_count());

    let explicit = registry.parse_with_protocol(
        "rust",
        &source,
        "src/main.rs",
    )?;
    assert_eq!(schema.vertex_count(), explicit.vertex_count());
    Ok(())
}
```

`parse_file(path, content)` detects the protocol from the path extension and returns `ParseError::UnknownLanguage` for an unregistered extension. `parse_with_protocol(protocol, content, file_path)` bypasses extension detection but still requires a registered protocol.

## Parse from Python

```python
from pathlib import Path
import panproto

source = Path("src/app.py").read_bytes()
registry = panproto.AstParserRegistry()
schema = registry.parse_file("src/app.py", source)

print(len(schema.vertices), len(schema.edges))
print(registry.protocol_names())
```

The core Python wheel discovers installed `panproto-grammars-*` companion packages when `panproto.AstParserRegistry()` is constructed. Install the group that contains the required language before creating the registry; for instance, `panproto-grammars-functional` adds Haskell, OCaml, and related grammars.

## Read anonymous-token fields

Named tree-sitter children appear as schema edges. When a grammar attaches a field name to an unnamed token alternative, the walker stores the token text as a `field:<name>` constraint on the parent. Use `Schema.field_text` to read it:

```python
schema = registry.parse_with_protocol(
    "qvr",
    b"let y = log(x)",
    "demo.qvr",
)
let_call = next(v.id for v in schema.vertices if v.kind == "let_call")
assert schema.field_text(let_call, "func") == "log"
```

The Rust accessor is `Schema::field_text(vertex_id, field_name) -> Option<&str>`.

## Verify pretty emission before relying on it

`emit_pretty_with_protocol` renders a schema without replaying its original layout. Query the protocol's verification tier first:

```rust,no_run
use panproto_core::parse::{EmitVerificationStatus, ParserRegistry};

let registry = ParserRegistry::new();
match registry.emit_verification_status("rust") {
    EmitVerificationStatus::Verified => {}
    EmitVerificationStatus::Generic => {
        eprintln!("pretty emission has no protocol-specific verification claim");
    }
    EmitVerificationStatus::Unsupported => {
        eprintln!("pretty emission is unavailable for this protocol");
    }
}
```

`Verified` records coverage by repository tests on representative input or a protocol corpus. It is not a proof over every byte sequence. Use `schema parse emit` on the actual files that enter a pipeline.

## Limitations

- The AST schema contains syntactic structure. It does not add type checking, name resolution, or control-flow information.
- The CLI parse-inspection commands print summaries, not serializable schema values.
- Tree-sitter may include error nodes for malformed source, and panproto's schema construction can still fail. Do not treat parsing as an unconditional total operation.
- Language availability depends on build features and installed grammar packs. Query `protocol_names()` rather than relying on the maximum catalog size.

## See also

- [Decorate an abstract schema](./decorate-schemas.md) for rendering a layout-free schema.
- [Source-code emission](../explanation/emit-pretty.md) for replay and pretty-emission paths.
- [Rust SDK](../reference/sdk-rust.md#feature-flags) for feature selection.
