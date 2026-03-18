# panproto

[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![Python](https://img.shields.io/pypi/pyversions/panproto)](https://pypi.org/project/panproto/)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Universal schema migration engine for Python. Wraps the panproto WASM module via MessagePack IPC, with automatic lens generation via [protolenses](https://ncatlab.org/nlab/show/natural+transformation).

Requires Python 3.13+.

## Installation

```bash
pip install panproto
```

## Quick start

```python
from panproto import Panproto

with Panproto.load() as pp:
    atproto = pp.protocol("atproto")
    old_schema = (
        atproto.schema()
        .vertex("post", "record", nsid="app.bsky.feed.post")
        .vertex("post:body", "object")
        .vertex("post:body.text", "string")
        .edge("post", "post:body", "record-schema")
        .edge("post:body", "post:body.text", "prop", name="text")
        .constraint("post:body.text", "maxLength", "3000")
        .build()
    )

    # One-liner data conversion between schema versions
    converted = pp.convert(record, old_schema, new_schema)

    # Auto-generate a lens with full control
    lens = pp.lens(old_schema, new_schema)
    view, complement = lens.get(record)
    restored = lens.put(modified_view, complement)

    # Build a reusable protolens chain (schema-independent)
    chain = pp.protolens_chain(old_schema, new_schema)
    result = chain.apply(record)

    # Diff two schemas
    diff = pp.diff(old_schema, new_schema)
```

## API

| Class | Description |
|-------|-------------|
| `Panproto` | Main entry point; call `Panproto.load()` to initialize |
| `Panproto.convert()` | One-liner data conversion between two schemas via auto-generated protolens |
| `Panproto.lens()` | Auto-generate a lens between two schemas |
| `Panproto.protolens_chain()` | Build a reusable, schema-independent protolens chain |
| `Protocol` | Protocol handle with schema builder factory |
| `SchemaBuilder` / `BuiltSchema` | Fluent schema construction |
| `MigrationBuilder` / `CompiledMigration` | Migration construction and compilation |
| `Instance` | Instance wrapper with JSON conversion and validation |
| `IoRegistry` | Protocol-aware parse/emit for all 76 formats |
| `Repository` | Schematic version control (init, commit, branch, merge) |
| `LensHandle` | Lens with `get`, `put`, and `auto_generate()` for automatic lens derivation |
| `ProtolensChainHandle` | Reusable, schema-independent protolens chain with `apply` and `instantiate` |
| `ProtolensChainHandle.fuse()` | Fuse chain into single step |
| `ProtolensChainHandle.check_applicability()` | Check applicability with reasons |
| `ProtolensChainHandle.lift()` | Lift along theory morphism |
| `ProtolensChainHandle.from_json()` | Deserialize from JSON |
| `SymmetricLensHandle` | Symmetric (bidirectional) lens for two-way synchronization |
| `DataSetHandle` | Handle to versioned data set with migrate/staleness methods |
| `Panproto.data_set()` | Store and track a data set |
| `Panproto.migrate_data()` | Migrate data between schemas |
| `FullDiffReport` / `CompatReport` | Breaking change analysis |
| `TheoryHandle` / `TheoryBuilder` | GAT theory construction |

## Documentation

[panproto.dev](https://panproto.dev)

## License

[MIT](../../LICENSE)
