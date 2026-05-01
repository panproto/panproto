# panproto

[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![Python](https://img.shields.io/pypi/pyversions/panproto)](https://pypi.org/project/panproto/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/panproto/panproto/blob/main/LICENSE)

Python bindings for [panproto], a schematic version-control system that
treats every supported schema language (around 50, including ATProto,
OpenAPI, AsyncAPI, Avro, Protobuf, JSON Schema, and Kubernetes CRDs) as
views over a single graph format.

The bindings are built with PyO3 against the Rust core directly: no WASM
runtime, no subprocess, no shelling out. The full panproto surface is
available from Python, including the schema/migration/lens/VCS pipelines
and the tree-sitter-driven parser for ~250 programming languages.

[panproto]: https://panproto.dev

## Status

panproto is pre-1.0. The 0.x series carries arbitrary breaking changes
between minor versions; the `panproto` package version tracks the
workspace version on every release. Python 3.13+ is required (the
wheels are abi3 and forward-compatible across newer Python releases).

## Installation

```bash
pip install panproto
```

Wheels are published on PyPI for Linux x86_64/aarch64, macOS
arm64+x86_64, and Windows x86_64. No Rust toolchain is required to
install.

## Synopsis

```python
import panproto

# Pick a built-in protocol, or define your own with `Protocol.from_theories`.
atproto = panproto.get_builtin_protocol("atproto")

# Build a schema using the fluent builder.
v1 = atproto.schema()
v1.vertex("post", "record", "app.bsky.feed.post")
v1.vertex("post:body", "object")
v1.vertex("post:body.text", "string")
v1.edge("post", "post:body", "record-schema")
v1.edge("post:body", "post:body.text", "prop", "text")
v1.constraint("post:body.text", "maxLength", "3000")
schema_v1 = v1.build()

# (build schema_v2 the same way, with the field renamed to `content` ...)

# Detect breaking changes.
report = panproto.diff_and_classify(schema_v1, schema_v2, atproto)
print(report.compatible)        # True or False
print(report.report_text())     # human-readable summary

# Auto-generate a bidirectional converter.
lens, quality, _ = panproto.auto_generate_lens(schema_v1, schema_v2, atproto)
view, complement = lens.get(instance)
restored = lens.put(view, complement)

# Version-control schemas.
repo = panproto.Repository.init("/path/to/repo")
repo.add(schema_v1)
repo.commit("initial schema")
repo.branch("feature")
repo.merge("feature")
```

## API overview

| Module / class               | Purpose                                                                  |
|------------------------------|--------------------------------------------------------------------------|
| `Schema`, `SchemaBuilder`    | Fluent schema construction; `Schema.validate(protocol)` checks rules.    |
| `Protocol`                   | Schema-language definition. `Protocol.from_theories(...)` builds one from a `Theory`. |
| `get_builtin_protocol(name)` | Load any of the ~50 builtin protocols by name.                           |
| `define_protocol(spec)`      | Define a custom protocol from a dict.                                    |
| `Theory`, `create_theory`    | GAT-level theory construction (sorts, ops, equations, directed_eqs).     |
| `diff_schemas`, `diff_and_classify` | Structural diff and breaking-change classification.               |
| `auto_generate_lens`         | Generate a bidirectional `Lens` from two schemas.                        |
| `Lens`                       | `get(instance) -> (view, complement)`, `put(view, complement) -> instance`. |
| `MigrationBuilder`, `compile_migration`, `compose_migrations` | Hand-rolled migration construction. |
| `Instance`, `IoRegistry`     | Parse/emit data across the 50+ supported formats.                        |
| `Repository`                 | Filesystem-backed VCS: init, commit, branch, merge, log, blame, bisect, stash, tag, plus data versioning. |
| `AstParserRegistry`, `parse_source_file`, `ParseEmitLens` | Full-AST parsing across ~250 languages via tree-sitter. |
| `Expr`, `parse_expr`, `pretty_print_expr` | Embedded expression language.                                  |

## Performance notes

* The `_native` extension talks to the Rust core through PyO3's zero-copy
  pyclass slabs. Schemas, theories, and lenses are reference-counted
  Rust objects on the Python side; mutations go through dedicated
  builder types (`SchemaBuilder`, `MigrationBuilder`) that consume on
  `build()`, so you can't accidentally observe partial state.
* Cross-thread sharing of these objects requires the GIL; for parallel
  work, fan out at the data layer (e.g. parallelise `lens.get` calls
  with `concurrent.futures`) and keep the schema/lens objects per
  worker.
* Wheel-load cost is one-time; the import sets up the protocol registry
  lazily so cold-start is fast.

## Contributing

Source: [bindings/python](https://github.com/panproto/panproto/tree/main/bindings/python).
Issues and pull requests at
[github.com/panproto/panproto/issues](https://github.com/panproto/panproto/issues).

The native extension lives at
[crates/panproto-py](https://github.com/panproto/panproto/tree/main/crates/panproto-py)
on the Rust side; `bindings/python/src/panproto/__init__.py` is the
pure-Python re-export layer that maturin ships alongside the
compiled extension.

## License

[MIT](https://github.com/panproto/panproto/blob/main/LICENSE) © 2026 Aaron Steven White.
