# panproto-project

Multi-file project assembly via schema coproduct for panproto.

## Overview

Orchestrates parsing all files in a project directory into a unified project-level schema. The project schema is the coproduct (disjoint union) of per-file schemas, with cross-file edges for imports and type references.

## Features

- **Project manifest** (`panproto.toml`): workspace/package configuration with glob-based excludes and per-package protocol overrides
- **Package detection**: auto-detects Rust, TypeScript, Python, Go, Java, Kotlin, Elixir, and C++ packages from filesystem markers
- **Incremental parsing cache**: mtime+size+blake3 invalidation, stored in `.panproto/cache/file_schemas.json`
- **Cross-file import resolution**: BFS constraint lookup with built-in rules for TypeScript, JavaScript, Python, Rust, and Go
- **Coproduct construction**: path-prefixed vertex IDs for global uniqueness, universal property for composable diffs

## Three-pass approach

1. **Parse pass**: For each file, detect language (or use protocol override from `panproto.toml`), check cache, parse via `ParserRegistry`, prefix vertex IDs with the file path.
2. **Coproduct pass**: Merge all per-file schemas into a single `Schema` with path-prefixed vertex/edge names.
3. **Resolve pass**: Walk `import` vertices, match against exports in other file schemas via `find_descendant_constraint` BFS, emit `imports` edges connecting them.

## Usage

```rust
use panproto_project::{ProjectBuilder, config};

// With project manifest
let cfg = config::load_config(std::path::Path::new("."))?.unwrap();
let mut builder = ProjectBuilder::with_config(&cfg, std::path::Path::new("."))?;
builder.add_directory(std::path::Path::new("."))?;
let project = builder.build()?;

println!("{} files, {} vertices", project.file_map.len(), project.schema.vertices.len());
```

```rust
// Without manifest (uses default skip patterns)
let mut builder = ProjectBuilder::new();
builder.add_directory(std::path::Path::new("./src"))?;
let project = builder.build()?;
```

## Modules

| Module | Description |
|--------|-------------|
| `config` | `panproto.toml` loading, generation, and serialization |
| `detect` | Language detection by file extension and package detection by marker files |
| `cache` | Incremental parsing cache with mtime+size+blake3 invalidation |
| `resolve` | Cross-file import resolution with per-language rules |

## Language detection

File extensions are mapped to protocols: `.ts` to TypeScript, `.py` to Python, `.rs` to Rust, etc. Unrecognized files fall back to the `raw_file` protocol (text as ordered line vertices, binary as chunk vertices).

## License

MIT
