# panproto

[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![Python](https://img.shields.io/pypi/pyversions/panproto)](https://pypi.org/project/panproto/)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Universal schema migration engine for Python. Wraps the panproto WASM module via MessagePack IPC.

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
    schema = (
        atproto.schema()
        .vertex("post", "record", nsid="app.bsky.feed.post")
        .vertex("post:body", "object")
        .vertex("post:body.text", "string")
        .edge("post", "post:body", "record-schema")
        .edge("post:body", "post:body.text", "prop", name="text")
        .constraint("post:body.text", "maxLength", "3000")
        .build()
    )

    # Diff two schemas
    diff = pp.diff(old_schema, new_schema)

    # Compile and apply a migration
    migration = pp.migration(src, tgt).map("old_id", "new_id").compile()
    result = migration.lift(record)

    # Bidirectional lens
    view, complement = migration.get(record)
    restored = migration.put(modified_view, complement)
```

## API

| Class | Description |
|-------|-------------|
| `Panproto` | Main entry point; call `Panproto.load()` to initialize |
| `Protocol` | Protocol handle with schema builder factory |
| `SchemaBuilder` / `BuiltSchema` | Fluent schema construction |
| `MigrationBuilder` / `CompiledMigration` | Migration construction and compilation |
| `Instance` | Instance wrapper with JSON conversion and validation |
| `IoRegistry` | Protocol-aware parse/emit for all 76 formats |
| `Repository` | Schematic version control (init, commit, branch, merge) |
| `FullDiffReport` / `CompatReport` | Breaking change analysis |
| `TheoryHandle` / `TheoryBuilder` | GAT theory construction |

## Documentation

[panproto.dev](https://panproto.dev)

## License

[MIT](../../LICENSE)
