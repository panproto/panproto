# Python SDK reference

The [Python](https://www.python.org/) package is [`panproto`](https://pypi.org/project/panproto/). It requires Python 3.13 or later and contains a native [PyO3](https://pyo3.rs/) extension.

```sh
python -m pip install panproto
```

## Module surface

Public names are re-exported from `panproto`. There is no umbrella engine class. The package source lists those names in [`panproto.__all__`](https://github.com/panproto/panproto/blob/main/bindings/python/src/panproto/__init__.py), and the shipped `_native.pyi` file is the signature authority for the extension.

| Domain | Principal names |
|---|---|
| Protocols and schemas | `get_builtin_protocol`, `list_builtin_protocols`, `define_protocol`, `Protocol`, `SchemaBuilder`, `Schema` |
| Schema parsing | `parse_atproto_lexicon`, `parse_schema_document`, `parse_schema_bundle`, `parse_schema_bundle_project`, `parse_schema_source` |
| Migrations | `MigrationBuilder`, `compile_migration`, `compose_migrations`, `invert_migration`, `CompiledMigration` |
| Morphism search | `find_span`, `find_morphisms`, `find_best_morphism`, `SchemaSpan`, `FoundMorphism` |
| Checking | `diff_schemas`, `diff_and_classify`, `check_existence`, `check_coverage` |
| Lenses | `Lens`, `ProtolensChain`, `auto_generate_lens`, `auto_generate_lens_candidates` |
| Instances and I/O | `Instance`, `IoRegistry` |
| GATs | `Theory`, `TheoryBuilder`, `TheoryMorphism`, `Model`, `colimit_theories` |
| Expressions | `Expr`, `parse_expr`, `pretty_print_expr` |
| Version control | `Repository`, `VcsRepository`, `BisectState` |
| Full-AST parsing | `AstParserRegistry`, `ParseEmitLens`, `parse_source_file`, `available_grammars` |
| Projects and git | `ProjectBuilder`, `ProjectSchema`, `parse_project`, `build_project`, `git_import` |

## Builder contracts

Python builders mutate in place. Their mutation methods return `None`, except where `_native.pyi` declares a fluent return type.

```python
class SchemaBuilder:
    def vertex(self, id: str, kind: str, nsid: str | None = ..., /) -> None: ...
    def edge(
        self,
        src: str,
        tgt: str,
        kind: str,
        name: str | None = ...,
    ) -> None: ...
    def constraint(self, vertex_id: str, sort: str, value: str) -> None: ...
    def build(self) -> Schema: ...
```

`TheoryBuilder.sort`, `TheoryBuilder.op`, and `TheoryBuilder.eq` return the builder and may be chained. Check `_native.pyi` before assuming that a builder follows either convention.

## Migration direction

For a `CompiledMigration` whose schema mapping is \(S\to T\), `lift(instance)` accepts an \(S\)-instance and returns the surviving fragment as a \(T\)-instance. The method calls Rust's restrict-based `lift_wtype`. It is neither the left Kan extension \(\Sigma_F\) nor precomposition \(\Delta_F\). `get` uses the same source-to-target operation and returns a complement with the view. `put` accepts the target view and complement and reconstructs a source instance.

## Morphism search

```python
find_span(
    src: Schema,
    tgt: Schema,
    protocol: Protocol,
    anchors: dict[str, str] | None = None,
    monic: bool = False,
    epic: bool = False,
    iso: bool = False,
) -> SchemaSpan

find_morphisms(
    src: Schema,
    tgt: Schema,
    anchors: dict[str, str] | None = None,
    monic: bool = False,
    epic: bool = False,
    iso: bool = False,
    max_results: int = 0,
) -> list[FoundMorphism]
```

`find_span` returns an empty apex when the schemas share no vertices. It requires a protocol because the induced apex is validated before it is returned. `epic=True` raises `MigrationError` for span search. Surjectivity is defined for the total-morphism functions.

`find_morphisms` returns total morphisms attaining the optimum. An empty list means that no total morphism exists, whereas a search failure raises `MigrationError`. The Python list does not carry the Rust `MorphismList.truncated` field, so this binding cannot report whether the engine stopped enumerating tied optima at its cap.

## Grammar packs

`AstParserRegistry()` constructs a native registry and adds grammars advertised through installed `panproto.grammars` entry points. Discovery occurs when the factory is called. Importing a companion package is not required.

| Package | Group |
|---|---|
| `panproto-grammars-functional` | Functional languages |
| `panproto-grammars-web` | Web languages |
| `panproto-grammars-systems` | Systems languages |
| `panproto-grammars-jvm` | JVM languages |
| `panproto-grammars-scripting` | Scripting languages |
| `panproto-grammars-data` | Data and schema languages |
| `panproto-grammars-devops` | Build and operations languages |
| `panproto-grammars-mobile` | Mobile languages |
| `panproto-grammars-music` | Music languages |
| `panproto-grammars-all` | Aggregate pack |

`panproto._native.AstParserRegistry` bypasses companion discovery and constructs the registry supplied by the core extension alone.

## Ownership, errors, and typing

PyO3 objects follow Python reference ownership. The public stub exposes no `dispose`, `release`, or `close` method for schema, migration, lens, theory, repository, parser, or project objects.

`PanprotoError` is the common exception base. Domain subclasses include `SchemaValidationError`, `MigrationError`, `LensError`, `CheckError`, `ExistenceCheckError`, `ExprError`, `GatError`, `IoError`, `VcsError`, `ParseError`, `ProjectError`, and `GitBridgeError`.

The wheel includes `py.typed` and `_native.pyi`. Repository tests compare the stub's public declarations with the loaded extension and check callable signatures, so generated documentation should follow the stub rather than infer signatures from Rust names.

## See also

- [Install the Python SDK](../how-to/install/python.md)
- [Define a schema from Python](../how-to/define-schema/python.md)
- [Find a span between two schemas](../how-to/spans.md)
- [Python binding source](https://github.com/panproto/panproto/tree/main/bindings/python)
