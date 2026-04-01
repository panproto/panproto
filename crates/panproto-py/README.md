# panproto-py

Native Python bindings for [panproto](https://github.com/panproto/panproto) via [PyO3](https://pyo3.rs). Compiles to a `cdylib` that maturin packages as `panproto._native`. The `panproto` PyPI package re-exports everything from this module.

## What this crate provides

All panproto functionality exposed to Python, with zero serialization overhead:

| Module | Types and functions |
|--------|-------------------|
| **Schema** | `Protocol`, `Schema`, `SchemaBuilder`, `Vertex`, `Edge`, `Constraint`, `HyperEdge` |
| **Protocols** | `list_builtin_protocols()` (76 protocols), `get_builtin_protocol()`, `define_protocol()` |
| **Migration** | `Migration`, `MigrationBuilder`, `CompiledMigration`, `compile_migration()`, `compose_migrations()`, `invert_migration()`, `check_existence()`, `check_coverage()` |
| **Check** | `SchemaDiff`, `CompatReport`, `diff_schemas()`, `diff_and_classify()` |
| **Instance** | `Instance` (W-type) with `from_json()`, `to_json()`, `validate()` |
| **I/O** | `IoRegistry` (76 codecs) with `parse()`, `emit()` |
| **Lens** | `Lens`, `Complement`, `auto_generate_lens()`, `auto_generate_with_hints()`, `ProtolensChain` with `fuse()`, `instantiate()`, `compose()`, `to_json()`, `from_json()`, `check_laws()`, `check_get_put()`, `check_put_get()` |
| **Combinators** | `rename_field()`, `remove_field()`, `add_field()`, `hoist_field()`, `pipeline()` |
| **Expression** | `eval_expr()`, `tokenize()`, `parse_expr()` |
| **GAT** | `Theory`, `Model`, `create_theory()`, `colimit_theories()`, `check_morphism()`, `migrate_model()`, `free_model()`, `check_model()` |
| **Expr** | `Expr`, `parse_expr()`, `pretty_print_expr()` |
| **VCS** | `VcsRepository` with `add()`, `list_refs()` |
| **Errors** | `PanprotoError` hierarchy with 10 exception classes |

## Architecture

Unlike `panproto-wasm` (thread-local slab allocator, opaque `u32` handles, MessagePack IPC), the PyO3 bindings use `#[pyclass]` structs that own or `Arc`-share the underlying Rust data directly. Python's garbage collector manages lifetimes. Data crosses the boundary via `pythonize` (serde to Python dicts) instead of MessagePack.

## Building

Requires Python 3.13+ and Rust 1.85+.

```bash
# Development build (installs into current venv)
maturin develop --manifest-path crates/panproto-py/Cargo.toml

# Release wheel
maturin build --release --manifest-path crates/panproto-py/Cargo.toml
```

## Usage

```python
import panproto

sql = panproto.get_builtin_protocol("sql")
builder = sql.schema()
builder.vertex("users", "table")
builder.vertex("users.id", "integer")
builder.edge("users", "users.id", "prop", "id")
schema = builder.build()

print(schema)  # Schema(protocol="sql", vertices=2, edges=1)
```

## License

[MIT](../../LICENSE)
