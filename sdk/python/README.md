# panproto

[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![Python](https://img.shields.io/pypi/pyversions/panproto)](https://pypi.org/project/panproto/)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/panproto/panproto/blob/main/LICENSE)

Native Python bindings for [panproto](https://panproto.dev). Define schemas, detect breaking changes, and automatically convert data between schema versions. Supports 51 schema languages (OpenAPI, ATProto, Protobuf, JSON Schema, and more) and can parse source code in 248 programming languages via tree-sitter.

Built with PyO3 for direct access to the Rust engine (no WASM, no subprocess). Requires Python 3.13+.

## Installation

```bash
pip install panproto
```

## Quick start

```python
import panproto as pp

# Build two versions of a schema
builder = pp.SchemaBuilder("atproto")
builder.vertex("post", "record", nsid="app.bsky.feed.post")
builder.vertex("post:body", "object")
builder.vertex("post:body.text", "string")
builder.edge("post", "post:body", "record-schema")
builder.edge("post:body", "post:body.text", "prop", name="text")
builder.constraint("post:body.text", "maxLength", "3000")
old_schema = builder.build()

# ... build new_schema similarly, with renamed fields ...

# Check whether the change is breaking
report = pp.diff_and_classify(old_schema, new_schema, protocol)
print(report.compatible)        # True or False
print(report.report_text())     # human-readable summary

# Auto-generate a bidirectional converter.
# Returns (lens, alignment_quality, coerce_proposals); the last is empty
# unless `stringency="exploratory"` is passed.
lens, quality, coerce_proposals = pp.auto_generate_lens(old_schema, new_schema, protocol)
view, complement = lens.get(instance)
restored = lens.put(view, complement)

# Parse and emit data in any supported format
io = pp.IoRegistry()
schema = io.parse_schema("json-schema", json_schema_str)
output = io.emit_schema("json-schema", schema)

# Version control
repo = pp.VcsRepository.init("/path/to/repo")
repo.add(schema)
repo.commit("add post schema", "your-name")
repo.branch("feature")
repo.merge("feature", "your-name")
```

## API overview

### Schema and protocols

| Function / Class | What it does |
|------------------|--------------|
| `list_builtin_protocols()` | List all 76+ built-in protocol names. |
| `get_builtin_protocol(name)` | Load a protocol definition by name. |
| `define_protocol(...)` | Define a custom protocol. |
| `SchemaBuilder(protocol)` | Build schemas with `vertex()`, `edge()`, `constraint()`, then `build()`. |
| `Schema` | An immutable schema with vertex, edge, and constraint accessors. |
| `Protocol` | A schema language definition with edge rules and feature flags. |

### Breaking change detection

| Function / Class | What it does |
|------------------|--------------|
| `diff_schemas(old, new)` | Compute a structural diff between two schemas. |
| `diff_and_classify(old, new, protocol)` | Diff and classify changes as breaking or non-breaking. |
| `SchemaDiff` | The diff result: added, removed, and changed elements. |
| `CompatReport` | Compatibility report with details on each breaking change. |

### Migration and lenses

| Function / Class | What it does |
|------------------|--------------|
| `auto_generate_lens(src, tgt, protocol)` | Generate a bidirectional converter between two schemas. |
| `Lens` | A converter with `get(instance)` (forward) and `put(view, complement)` (backward). |
| `Complement` | The data dropped during forward conversion, needed for lossless backward conversion. |
| `MigrationBuilder` | Build a migration by specifying vertex and edge mappings. |
| `compile_migration(src, tgt, migration)` | Compile a migration for application. |
| `compose_migrations(m1, m2)` | Chain two migrations into one. |
| `check_existence(src, tgt, migration, protocol)` | Validate that a migration is well-formed. |

### Data I/O

| Function / Class | What it does |
|------------------|--------------|
| `Instance` | A data record with `from_json()`, `to_json()`, and `validate()`. |
| `IoRegistry` | Parse and emit schemas and data in any of the 76+ supported formats. |

### Theory engine

| Function / Class | What it does |
|------------------|--------------|
| `create_theory(name, sorts, ops, eqs)` | Define a custom schema language theory. |
| `Theory` | A theory with sort, operation, and equation accessors. |
| `colimit_theories(t1, t2, shared)` | Combine two theories into one. |
| `check_morphism(morphism, domain, codomain)` | Validate a structure-preserving map between theories. |
| `Model` | A concrete model of a theory. |

### Version control

| Function / Class | What it does |
|------------------|--------------|
| `VcsRepository.init(path)` | Initialize a new panproto repository. |
| `VcsRepository.open(path)` | Open an existing repository. |
| `repo.add(schema)` | Stage a schema for commit. |
| `repo.commit(message, author)` | Create a new commit. |
| `repo.branch(name)` | Create a branch. |
| `repo.merge(branch, author)` | Merge a branch into the current one. |

## Documentation

Full documentation at [panproto.dev](https://panproto.dev).

## License

[MIT](https://github.com/panproto/panproto/blob/main/LICENSE)
