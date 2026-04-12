# panproto-project

[![crates.io](https://img.shields.io/crates/v/panproto-project.svg)](https://crates.io/crates/panproto-project)
[![docs.rs](https://docs.rs/panproto-project/badge.svg)](https://docs.rs/panproto-project)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Assembles a directory of source files into a single unified schema graph.

## What it does

When you have a project with dozens of files across multiple languages, each file is its own island of type information. `panproto-project` scans a directory tree, parses every file it recognizes (TypeScript, Python, Rust, and 245 other languages via tree-sitter), and joins the results into one schema that spans the whole project. Files that are not a recognized language are stored as raw text or binary nodes so nothing is dropped.

The joining works by prefixing each file's vertex names with its path: a function called `add` in `src/utils.ts` becomes `src/utils.ts::add`, so it never collides with an `add` in `lib/math.py`. After prefixing, the crate walks import statements and emits cross-file edges wherever one file references a name exported by another. The result is one schema where intra-file structure and inter-file dependencies are both visible.

An optional `panproto.toml` manifest lets you exclude directories (like `node_modules` or `build`), override which parser to use for a subtree, and configure per-package settings. An incremental parsing cache (keyed on file path, mtime, size, and content hash) skips re-parsing files that have not changed since the last build.

## Quick example

```rust,ignore
use panproto_project::{ProjectBuilder, ProjectConfig};
use std::path::Path;

// Scan a directory and build a unified schema.
let mut builder = ProjectBuilder::new();
builder.add_directory(Path::new("my-project"))?;
let project = builder.build()?;

println!("{} files, {} vertices",
    project.file_map.len(),
    project.schema.vertices.len());

// Which protocol parsed each file?
for (path, proto) in &project.protocol_map {
    println!("{}: {}", path.display(), proto);
}
```

## API overview

| Export | What it does |
|--------|-------------|
| `ProjectBuilder` | Accumulates files and builds the final schema |
| `ProjectBuilder::new()` | Creates a builder with the default parser registry |
| `ProjectBuilder::with_config()` | Creates a builder from a `panproto.toml` manifest |
| `ProjectBuilder::with_config_and_cache()` | Same, plus an incremental parse cache |
| `ProjectBuilder::add_file()` | Parses one file and adds it to the builder |
| `ProjectBuilder::add_directory()` | Recursively scans a directory tree |
| `ProjectBuilder::build()` | Runs import resolution and returns the unified `ProjectSchema` |
| `ProjectSchema` | The assembled result: schema, file map, protocol map |
| `ProjectConfig` | Parsed `panproto.toml` manifest |
| `DetectedPackage` | Language or package detected for a path |
| `ProjectError` | Error variants: parse failure, bad glob pattern, coproduct failure |

## License

[MIT](../../LICENSE)
