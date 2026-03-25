# panproto

[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![Python](https://img.shields.io/pypi/pyversions/panproto)](https://pypi.org/project/panproto/)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/panproto/panproto/blob/main/LICENSE)

Native Python bindings for [panproto](https://panproto.dev), a schema migration engine grounded in generalized algebraic theories. Built with PyO3 for zero-overhead access to the Rust core.

Requires Python 3.13+.

## Installation

```bash
pip install panproto
```

## Quick start

```python
import panproto as pp

# List all 76+ built-in protocols
protocols = pp.list_builtin_protocols()

# Build a schema using the fluent builder
builder = pp.SchemaBuilder("atproto")
builder.vertex("post", "record", nsid="app.bsky.feed.post")
builder.vertex("post:body", "object")
builder.vertex("post:body.text", "string")
builder.edge("post", "post:body", "record-schema")
builder.edge("post:body", "post:body.text", "prop", name="text")
builder.constraint("post:body.text", "maxLength", "3000")
old_schema = builder.build()

# Auto-generate a lens between two schema versions
lens = pp.auto_generate_lens(old_schema, new_schema, protocol)
view, complement = lens.get(instance)
restored = lens.put(view, complement)

# Diff two schemas and classify changes
diff = pp.diff_schemas(old_schema, new_schema)
report = pp.diff_and_classify(old_schema, new_schema, protocol)

# Build and compile migrations
mig_builder = pp.MigrationBuilder()
mig_builder.map_vertex("post:body.text", "post:body.content")
migration = mig_builder.build()
compiled = pp.compile_migration(old_schema, new_schema, migration)

# Parse and emit data across protocols
io = pp.IoRegistry()
schema = io.parse_schema("json-schema", json_schema_str)
output = io.emit_schema("json-schema", schema)

# GAT operations
theory = pp.create_theory("MyTheory", sorts, operations, equations)
colimit = pp.colimit_theories(theory_a, theory_b, shared)

# Schematic version control
repo = pp.VcsRepository.init("/path/to/repo")
repo.add(schema)
repo.commit("add post schema", "author")
```

## API overview

### Schema and protocols

| Function / Class | Description |
|------------------|-------------|
| `list_builtin_protocols()` | List all 76+ built-in protocol names |
| `get_builtin_protocol(name)` | Get a protocol definition by name |
| `define_protocol(...)` | Define a custom protocol |
| `SchemaBuilder(protocol)` | Fluent schema construction (vertex, edge, constraint, hyper_edge) |
| `Schema` | Immutable schema with vertex/edge/constraint access |
| `Protocol` | Protocol definition with edge rules and feature flags |

### Migration and lenses

| Function / Class | Description |
|------------------|-------------|
| `auto_generate_lens(src, tgt, protocol)` | Auto-generate a bidirectional lens between schemas |
| `Lens` | Lens with `get(instance)` and `put(view, complement)` |
| `Complement` | Data discarded during `get`, needed by `put` |
| `MigrationBuilder` | Build migrations with vertex/edge mappings |
| `compile_migration(src, tgt, migration)` | Compile a migration for application |
| `compose_migrations(m1, m2)` | Compose two migrations sequentially |
| `check_existence(src, tgt, migration, protocol)` | Validate migration existence conditions |

### Breaking change detection

| Function / Class | Description |
|------------------|-------------|
| `diff_schemas(old, new)` | Structural diff between two schemas |
| `diff_and_classify(old, new, protocol)` | Diff with breaking/non-breaking classification |
| `SchemaDiff` | Diff result with added/removed/changed elements |
| `CompatReport` | Compatibility report with breaking change details |

### Instance I/O

| Function / Class | Description |
|------------------|-------------|
| `Instance` | W-type instance with `from_json`, `to_json`, `validate` |
| `IoRegistry` | Parse/emit schemas and instances across all protocols |

### GAT engine

| Function / Class | Description |
|------------------|-------------|
| `create_theory(name, sorts, ops, eqs)` | Create a GAT theory |
| `Theory` | Theory with sort/operation/equation access |
| `colimit_theories(t1, t2, shared)` | Compute theory pushout |
| `check_morphism(morphism, domain, codomain)` | Verify a theory morphism |
| `Model` | Model of a theory with carrier sets |

### Version control

| Function / Class | Description |
|------------------|-------------|
| `VcsRepository.init(path)` | Initialize a panproto repository |
| `VcsRepository.open(path)` | Open an existing repository |
| `repo.add(schema)` | Stage a schema |
| `repo.commit(message, author)` | Create a commit |
| `repo.branch(name)` | Create a branch |
| `repo.merge(branch, author)` | Merge a branch |

## Documentation

Full documentation at [panproto.dev](https://panproto.dev).

## License

[MIT](https://github.com/panproto/panproto/blob/main/LICENSE)
