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
| `panproto-grammars-music` | SuperCollider, LilyPond, ABC, Csound, ChucK, Glicol, Tidal, Strudel |
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
| Morphism and span search | `find_span`, `find_morphisms`, `find_best_morphism`, and the `SchemaSpan` / `FoundMorphism` result classes |
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

## Morphism and span search

`find_span` returns a `SchemaSpan` of the form $\mathit{src} \leftarrow A \to \mathit{tgt}$, where the apex $A$ is the sub-schema of `src` induced on the vertices assigned a target. It does not return `None` or raise an exception merely because the schemas have nothing in common; that case produces an empty apex.

```python
span = panproto.find_span(old, new, proto, anchors={"post": "post"}, monic=True)

print(span.apex_coverage)      # 0.777... : 7 of the 9 source vertices
print(span.quality_bounds)     # (0.812, 0.812) when proven_optimal
if span.is_total:
    morphism = span.as_total_morphism()
```

`protocol` is a positional argument because the apex is a schema, and a schema is well formed only against a protocol; inducing the apex re-validates it rather than assuming it, and a `Schema` carries its protocol's name alone. `anchors` are mappings the caller *knows*, which the search may not reconsider. Setting `epic=True` raises `MigrationError`, since surjectivity is a property of a total morphism and a span's right leg is deliberately partial.

| Attribute of `SchemaSpan` | What it holds |
|---|---|
| `apex: Schema` | The sub-schema of `src` the search covered. |
| `left: Migration` | Inclusion from the apex into `src`. |
| `right: Migration` | Assignment from the apex into `tgt`; this field carries the identification. |
| `quality: float` | How well the covered part matches, in `[0, 1]`, with the drop count excluded. |
| `quality_bounds: tuple[float, float]` | The interval bracketing `quality`. Its ends are equal exactly when `proven_optimal` holds. |
| `apex_coverage: float` | The share of the source's vertices the apex covers, or one on an empty source. |
| `proven_optimal: bool` | Whether the search proved its answer optimal. |
| `is_total: bool` | Whether the apex is the whole source, which makes the span a total morphism. |
| `legs_are_functorial: bool` | Whether both legs passed the functoriality check. |
| `apex_digest: str` | The apex's content digest, lower-case hexadecimal. |

`as_total_morphism()` returns a `FoundMorphism` when `is_total` holds and `None` otherwise, `to_overlap()` gives the sorted pair lists a pushout takes, and `to_dict()` flattens the whole span. `quality` ranks spans over *one* source schema and nothing else, because every denominator of the objective is fixed by `src`; read `apex_coverage` alongside it.

### What `find_morphisms` returns

`find_morphisms` returns the morphisms **attaining the optimum**, and nothing else. Every element carries the same quality, so `results[0]` is the best answer there is and iterating further for a suboptimal alternative will not find one. The engine's cap of 1024 bounds every request rather than only `max_results=0`, so asking for more is answered with the cap. Python receives a plain list, so the flag the engine sets when the cap cut the answer short does not cross this surface; a caller that needs to tell a cut list from an exhausted one compares `len(results)` against 1024.

An empty list means that no total morphism exists, and only that. A search that could not be posed raises `MigrationError` instead, so the two are distinguishable; `find_span` is the function that answers with what the two schemas do share.

## Type stubs

The package ships `_native.pyi` so that signatures are visible to type checkers (mypy, pyright). Stub signatures are kept in lockstep with the PyO3 runtime by CI.

## See also

- [Install the Python SDK](../how-to/install/python.md).
- [Define a schema from Python](../how-to/define-schema/python.md).
- [Find a span between two schemas](../how-to/spans.md), and [Searching for a morphism](../explanation/morphism-search.md) for what the search is doing.
