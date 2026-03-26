# panproto-project

Multi-file project assembly via schema coproduct for panproto.

## Overview

Orchestrates parsing all files in a project directory into a unified project-level schema. The project schema is the coproduct (disjoint union) of per-file schemas, with cross-file edges for imports and type references.

## Two-pass approach

1. **Parse pass**: For each file, detect language, parse via `ParserRegistry`, prefix vertex IDs with the file path.
2. **Resolve pass**: Walk `import` vertices, match against exports in other file schemas, emit `imports` edges connecting them.

## Coproduct construction

The schema-level coproduct prefixes each file's vertex names with the file path. Edges within a file retain their local structure. The result is a single `Schema` spanning the entire project.

The coproduct is universal: any morphism out of the project schema restricts to per-file morphisms. This means per-file diffs compose into project-level diffs automatically.

## Usage

```rust
use panproto_project::ProjectBuilder;

let mut builder = ProjectBuilder::new();
builder.add_directory(std::path::Path::new("./src"))?;
let project = builder.build()?;

println!("{} files, {} vertices", project.file_map.len(), project.schema.vertices.len());
```

## Language detection

File extensions are mapped to protocols: `.ts` to TypeScript, `.py` to Python, `.rs` to Rust, etc. Unrecognized files fall back to the `raw_file` protocol (text as ordered line vertices, binary as chunk vertices).

## License

MIT
