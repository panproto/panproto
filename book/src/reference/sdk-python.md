# Python SDK reference

The Python SDK is published as [`panproto`](https://pypi.org/project/panproto/) on PyPI. It uses native PyO3 bindings, not WASM.

## Installation

```sh
pip install panproto
```

Python 3.13 or newer is required. The wheel ships with eleven core tree-sitter grammars (Python, JavaScript, TypeScript, Java, C#, C++, PHP, Bash, C, Go, Rust). Additional grammar packs are available as separately-installable companions:

| Pack | Languages |
|---|---|
| `panproto-grammars-functional` | Haskell, OCaml, Elm, Gleam, Erlang, Elixir, PureScript, F#, Clojure, Scheme, Racket |
| `panproto-grammars-web` | HTML, CSS, SCSS, Vue, Svelte, Astro, ... |
| `panproto-grammars-systems` | Zig, Nim, V, Crystal, ... |
| `panproto-grammars-jvm` | Scala, Kotlin, Groovy, ... |
| `panproto-grammars-scripting` | Lua, Perl, R, Julia, ... |
| `panproto-grammars-data` | SQL dialects, GraphQL, JSON variants, ... |
| `panproto-grammars-devops` | Terraform, Dockerfile, Helm, ... |
| `panproto-grammars-mobile` | Swift, Objective-C, Dart, ... |
| `panproto-grammars-music` | Tidal, Strudel, QVR, ... |
| `panproto-grammars-all` | Umbrella package containing every pack above. |

Each pack auto-registers its grammars with `panproto.AstParserRegistry()` on import.

## Top-level surface

```python
import panproto

proto = panproto.get_builtin_protocol("atproto")
b = proto.schema()
b.vertex("post", "record", "app.bsky.feed.post")
b.vertex("post:body", "object")
b.edge("post", "post:body", "record-schema")
schema = b.build()
```

There is no `Panproto` umbrella class; the entry points are free functions on the `panproto` module. The full re-export list (errors, schema types, protocol registry, migration, check, instance, I/O, lens, GAT, expression-language, VCS, parse, project, git bridge) is in [`bindings/python/src/panproto/__init__.py`](https://github.com/panproto/panproto/blob/main/bindings/python/src/panproto/__init__.py). Selected entry points:

| Surface | Entry point |
|---|---|
| Protocol registry | `get_builtin_protocol(name)`, `list_builtin_protocols()`, `define_protocol(...)` |
| Schema construction | `Protocol.schema()` returns a `SchemaBuilder`. Each `.vertex(id, kind)` / `.edge(src, tgt, kind, name=None)` / `.constraint(vid, sort, value)` mutates the builder in place and returns `None`; call `.build()` on the final builder. Chain syntax is a TypeScript-only convenience; Python is statement-by-statement. |
| Migration | `MigrationBuilder`, `compile_migration`, `compose_migrations`, `invert_migration`, `pipeline` |
| Check | `diff_and_classify`, `diff_schemas`, `check_existence`, `check_coverage` |
| Lens | `Lens`, `ProtolensChain` (with `from_dsl_json` / `from_dsl_yaml` / `from_dsl_nickel` / `from_dsl_path` loaders), `auto_generate_lens`, `auto_generate_lens_candidates` |
| GAT | `Theory` (with `from_json` / `from_yaml` / `from_nickel` / `from_path` DSL loaders and `to_yaml` / `from_dict_yaml` for the flat-shape round-trip), `TheoryBuilder`, `Model`, `colimit_theories`, `free_model`, `migrate_model`, `check_model`, `check_morphism` |
| Schema | `Schema.constraints_for(vertex_id)` lists every constraint; `Schema.field_text(vertex_id, field_name)` reads the text of a tree-sitter `field('<name>', anonymous-token)` child |
| Expression language | `Expr`, `parse_expr`, `pretty_print_expr` |
| VCS | `Repository`, `VcsRepository`, `BisectState` |
| Parse | `parse_source_file`, `available_grammars`, `ParseEmitLens`, `AstParserRegistry()` (with `.override_grammar(name, extensions, language_ptr, node_types, ...)` for dev-time grammar swapping) |
| Project | `ProjectBuilder`, `parse_project`, `build_project` |

Full API reference, including every method signature, lives at the dedicated mkdocs site:

- [Python SDK reference](https://panproto.dev/python/) (mkdocs)

The package source is at [`bindings/python/`](https://github.com/panproto/panproto/tree/main/bindings/python).

## Type stubs

The package ships `_native.pyi` so that signatures are visible to type checkers (mypy, pyright). Stub signatures are kept in lockstep with the PyO3 runtime by CI.

## See also

- [Install the Python SDK](../how-to/install/python.md).
- [Define a schema from Python](../how-to/define-schema/python.md).
