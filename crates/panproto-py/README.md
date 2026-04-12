# panproto-py

[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Native Python bindings for panproto via PyO3, with no WASM layer or subprocess.

## What it does

This crate compiles to a native `.so`/`.pyd` extension (packaged by maturin as `panproto._native`) that Python imports directly. Unlike the TypeScript SDK, which goes through a WASM boundary with MessagePack serialization, these bindings own the underlying Rust data in PyO3 `#[pyclass]` structs. Python's garbage collector handles lifetimes; there are no handles to free. Data crosses the boundary via `pythonize`, which converts Rust structs to and from Python dicts through serde, so you work with plain Python objects.

The bindings cover the full panproto API surface. Schema building and protocol selection work the same as in Rust. Migrations are compiled once and applied to instances. Breaking change detection (`diff_schemas`, `diff_and_classify`) returns Python dicts you can inspect or serialize. The VCS module exposes a `VcsRepository` class with `init`, `add`, `commit`, `branch`, `merge`, `log`, and `status` methods. The parse module wraps tree-sitter grammars for 248 languages. The project module provides `ProjectBuilder` and `build_project` for multi-file assembly. GAT operations (`create_theory`, `colimit`, `check_morphism`, `migrate_model`) are available for advanced theory-level work.

Requires Python 3.13 or later and Rust 1.85 or later to build from source. Install the pre-built wheel from PyPI instead when possible.

## Quick example

```sh
pip install panproto
```

```python
import panproto

# Build a schema.
proto = panproto.get_builtin_protocol("atproto")
builder = panproto.SchemaBuilder(proto)
builder.vertex("Post", "Record")
builder.vertex("Author", "Record")
builder.edge("Post", "Author", "HasAuthor")
schema = builder.build()

# Diff two schema versions.
diff = panproto.diff_schemas(schema_v1, schema_v2)
report = panproto.diff_and_classify(diff)
print(report["compatibility"])  # "BackwardCompatible", "Breaking", etc.
```

## API overview

| Module | What it exposes |
|--------|----------------|
| `schema` | `Protocol`, `Schema`, `SchemaBuilder`, `Vertex`, `Edge` |
| `protocols` | `list_builtin_protocols()`, `get_builtin_protocol()`, `define_protocol()` |
| `mig` | `Migration`, `MigrationBuilder`, `CompiledMigration`, `compile()`, `check_existence()` |
| `check` | `SchemaDiff`, `CompatReport`, `diff_schemas()`, `diff_and_classify()` |
| `inst` | `Instance` (the W-type instance container) |
| `io` | `IoRegistry` (77 format codecs for parse and emit) |
| `lens` | `Lens`, `auto_generate_lens()`, `classify_transform()` |
| `gat` | `Theory`, `Model`, `create_theory()`, `colimit()`, `check_morphism()`, `migrate_model()` |
| `expr` | `Expr`, `parse_expr()`, `eval_with_instance()` |
| `vcs` | `VcsRepository` with git-style VCS commands |
| `parse` | `AstParserRegistry`, `parse_source_file()` |
| `project` | `ProjectBuilder`, `ProjectSchema`, `build_project()`, `parse_project()` |
| `git` | `import_git_repo()`, `import_git_repo_incremental()`, `export_to_git()` |

## License

[MIT](../../LICENSE)
