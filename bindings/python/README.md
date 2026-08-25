# panproto

[![PyPI](https://img.shields.io/pypi/v/panproto)](https://pypi.org/project/panproto/)
[![Python](https://img.shields.io/pypi/pyversions/panproto)](https://pypi.org/project/panproto/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

This package exposes panproto's Rust libraries to Python through PyO3. It does
not start a subprocess or load the WebAssembly build. Python 3.13 or newer is
required.

The package is pre-1.0. A minor release may change the Python API, and the
package version follows the Rust workspace version.

## Install

```sh
pip install panproto
```

Release workflows build wheels for Linux on x86-64 and AArch64, macOS on Apple
Silicon and x86-64, and Windows on x86-64. A source build requires Rust and
[`maturin`](https://www.maturin.rs/).

## Build and compare schemas

`Protocol.schema()` returns a mutable `SchemaBuilder`. Builder methods update
the builder and return `None`; call `build()` after all changes have been added.

```python
import panproto

atproto = panproto.get_builtin_protocol("atproto")

old_builder = atproto.schema()
old_builder.vertex("post", "record", "app.bsky.feed.post")
old_builder.vertex("post:body", "object")
old_builder.vertex("post:body.text", "string")
old_builder.edge("post", "post:body", "record-schema")
old_builder.edge("post:body", "post:body.text", "prop", "text")
old_schema = old_builder.build()

new_builder = atproto.schema()
new_builder.vertex("post", "record", "app.bsky.feed.post")
new_builder.vertex("post:body", "object")
new_builder.vertex("post:body.content", "string")
new_builder.edge("post", "post:body", "record-schema")
new_builder.edge("post:body", "post:body.content", "prop", "content")
new_schema = new_builder.build()

report = panproto.diff_and_classify(old_schema, new_schema, atproto)
print(report.classification)
print(report.report_text())
```

`CompatReport.classification` is one of `"fully-compatible"`,
`"backward-compatible"`, or `"breaking"`. The object also exposes
`compatible`, `breaking_changes`, `non_breaking_changes`, `report_json()`, and
`to_dict()`.

## Migrations and lenses

`MigrationBuilder` records a source-to-target vertex map. Compile the result
against its source and target schemas:

```python
builder = panproto.MigrationBuilder()
builder.map_vertex("post", "post")
builder.map_vertex("post:body", "post:body")
migration = builder.build()

compiled = panproto.compile_migration(migration, old_schema, new_schema)
```

`CompiledMigration.lift(instance)` constructs a target instance from the
mapped part of a source instance. This operation is the library's
source-to-target surviving-fragment transfer. The categorical transports are
separate: `Delta` reindexes a target instance back to the source, while a general
left Kan extension computes the source-to-target `Sigma` transport. [The vocabulary
in plain terms](../../book/src/explanation/decoder-ring.md) defines both.

`CompiledMigration.get(instance)` and `Lens.get(instance)` project a source
instance to a target-shaped view and return an opaque complement. Their
corresponding `put(view, complement)` methods reconstruct a source instance.

```python
lens, quality, proposals = panproto.auto_generate_lens(
    old_schema, new_schema, atproto
)
view, complement = lens.get(source_instance)
restored_source = lens.put(view, complement)
```

`find_morphisms()` and `find_best_morphism()` search for total schema
morphisms. `find_best_morphism()` returns `None` when none exists.
`find_span()` instead returns a `SchemaSpan` in every no-overlap case, with an
empty apex representing the absence of shared structure.

The `ProtolensChain.from_dsl_*()` constructors retain both the structural chain
and the lens-DSL compiler's value-level field transforms. Instantiating the
chain installs those transforms in the compiled migration. `to_json()` and
`from_json()` preserve them in the optional `field_transforms` member.

## Other API groups

| API | Current behavior |
|---|---|
| `parse_schema_document`, `parse_schema_source`, `parse_schema_bundle` | Parse schema documents or source-language definitions through the registered Rust parsers. |
| `IoRegistry` | Parse and emit instances. Use `len(registry)` or `list_protocols()` to inspect the codecs in the installed build. |
| `Theory`, `TheoryBuilder`, `create_theory`, `colimit_theories` | Construct and combine generalized algebraic theories. |
| `Repository` | Open or create a filesystem-backed `.panproto` repository with commits, branches, tags, merge, rebase, stash, blame, bisect, data tracking, and garbage collection. |
| `VcsRepository` | A separate in-memory wrapper with only `add()` and `list_refs()`. It is not a `Repository` subclass. |
| `ProjectBuilder`, `build_project`, `parse_project` | Assemble multi-file projects. |
| `AstParserRegistry`, `parse_source_file`, `ParseEmitLens` | Parse source files with the tree-sitter grammars present in the build or supplied by companion packages. |
| `Expr`, `parse_expr`, `pretty_print_expr` | Parse, inspect, and evaluate the expression language through methods on `Expr`. |

The type stub at
[`src/panproto/_native.pyi`](src/panproto/_native.pyi) gives the complete public
signatures.

## Tree-sitter grammar packages

The default source build enables the 11 `group-core` grammars: Python,
JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, and Rust. Optional
companion wheels register other grammar groups through the
`panproto.grammars` Python entry-point group. `panproto-grammars-all` contains
all 261 grammars currently declared by `panproto-grammars`.

```sh
pip install panproto-grammars-functional
```

Constructing `panproto.AstParserRegistry()` loads metadata from installed
companions. `panproto._native.AstParserRegistry()` constructs only the native
registry. A companion whose metadata cannot be registered produces a
`RuntimeWarning` for that grammar and does not stop the other grammars from
loading.

## Object ownership

PyO3 classes own Rust values directly. Several classes share immutable schemas
through Rust reference counting, and Python releases those values when their
wrappers are collected. There is no numeric handle API and no manual free
function.

## References

- John Cartmell, [Generalised algebraic theories and contextual
  categories](https://doi.org/10.1016/0168-0072(86)90053-9), *Annals of Pure
  and Applied Logic* 32, 209-243, 1986.
- J. Nathan Foster et al., [Combinators for bidirectional tree
  transformations](https://doi.org/10.1145/1232420.1232424), *ACM
  Transactions on Programming Languages and Systems* 29(3), article 17, 2007.
- David I. Spivak, [Functorial data
  migration](https://doi.org/10.1016/j.ic.2012.05.001), *Information and
  Computation* 217, 31-51, 2012.

## License

[MIT](../../LICENSE)
