# panproto-project

[![crates.io](https://img.shields.io/crates/v/panproto-project.svg)](https://crates.io/crates/panproto-project)
[![docs.rs](https://docs.rs/panproto-project/badge.svg)](https://docs.rs/panproto-project)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Builds a project schema from per-file parser results.

## Processing model

`ProjectBuilder` uses a `panproto-parse::ParserRegistry`, so language coverage is
determined by the grammar features compiled into that registry. The default is the
11-language `group-core` set, not the complete grammar catalog.

`add_file` detects a parser from the path or from a package protocol override. If the
selected parser fails, it falls back to the raw-file parser. Known binary extensions
use `raw_file::parse_binary`; other unmatched files must be UTF-8 and use
`raw_file::parse_text`.

For a multi-file build, vertex IDs are prefixed with the file path and `::`. The
resolver then adds cross-file `imports` edges when its import and export heuristics
find a match. A single-file build returns that file's schema without path-prefixing.

`panproto.toml` supplies workspace exclusion globs and optional protocol overrides for
declared package paths. It does not restrict directory walking to only those package
paths. Without a config, directory walking skips hidden entries and a fixed set of
common build directories.

The optional cache stores mtime, size, content hash, schema, and protocol. Matching
mtime and size take the fast path. A size change invalidates the entry. When only the
mtime differs, the cache hashes the file and compares the stored content hash.

## Example

```rust,ignore
use panproto_project::ProjectBuilder;
use std::path::Path;

let mut builder = ProjectBuilder::new();
builder.add_directory(Path::new("my-project"))?;
let project = builder.build()?;
println!("{}", project.file_map.len());
```

## Public API

| Item | Purpose |
|------|---------|
| `ProjectBuilder` | Add files and build a flat schema or stored schema tree |
| `ProjectSchema` | Flat schema plus file and protocol maps |
| `ProjectSchemaTree` | Stored tree root plus protocol map |
| `ProjectConfig` | Deserialized `panproto.toml` configuration |
| `detect_language`, `scan_packages` | File and package detection helpers |
| `build_project_tree` | Store per-file schemas as a VCS Merkle tree |

## License

[MIT](../../LICENSE)
