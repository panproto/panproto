# Rust SDK reference

The [Rust](https://www.rust-lang.org/) entry point is the [`panproto-core`](https://docs.rs/panproto-core/latest/panproto_core/) facade. Version 0.71 requires Rust 1.85 or later.

```toml
[dependencies]
panproto-core = "0.71"
```

The facade re-exports each component crate as a module. For instance, `panproto_core::schema::Schema` is the `Schema` type from `panproto-schema`; depending on `panproto-schema` directly exposes the same type without the facade.

## Modules

| Module | Re-exported crate | Principal surface |
|---|---|---|
| `check` | `panproto-check` | Structural diffs, compatibility classification, and validation |
| `gat` | `panproto-gat` | Theories, models, theory morphisms, and colimits |
| `schema` | `panproto-schema` | Protocols, schemas, builders, morphisms, and pushouts |
| `inst` | `panproto-inst` | Tree, relational, and graph instance representations |
| `mig` | `panproto-mig` | Migration compilation, lifting, composition, and morphism search |
| `lens` | `panproto-lens` | Lenses, protolenses, complements, and law checks |
| `protocols` | `panproto-protocols` | Built-in protocol definitions |
| `io` | `panproto-io` | Instance parsing and emission |
| `vcs` | `panproto-vcs` | Schema and data version control |
| `expr`, `expr_parser` | `panproto-expr`, `panproto-expr-parser` | Expression values, evaluation, parsing, and formatting |
| `lens_dsl`, `theory_dsl` | `panproto-lens-dsl`, `panproto-theory-dsl` | Declarative lens and theory front ends |

The [crate map](./crate-map.md) lists lower-level workspace crates that are not re-exported from `panproto-core`.

## Feature flags

`panproto-core` has no default features.

| Feature | Additional module or behavior | Implied features |
|---|---|---|
| `full-parse` | Re-exports `panproto-parse` as `parse` | none |
| `project` | Re-exports `panproto-project` as `project` | `full-parse` |
| `git` | Re-exports `panproto-git` as `git` | `project`, hence `full-parse` |
| `tree-sitter` | Enables the `panproto-io/tree-sitter` implementation | none |

`full-parse` adds the tree-sitter grammar build dependencies described in the crate manifest. It is separate from the `tree-sitter` feature on `panproto-io`.

## Morphism search

The [`panproto_core::mig::hom_search`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/) module distinguishes partial overlap from total morphisms.

```text
find_span(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    opts: &SearchOptions,
) -> Result<SchemaSpan, SpanError>

find_morphisms(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
) -> Result<MorphismList, SpanError>

find_best_morphism(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
) -> Result<Option<FoundMorphism>, SpanError>
```

`find_span` may return an empty apex when the schemas share no vertices. The protocol argument is used to validate the induced apex. Setting `SearchOptions::epic` is invalid for this partial search and returns `SpanError::EpicIsNotASpanProperty`.

`find_morphisms` returns total morphisms that attain the optimum, rather than every morphism or a sequence of lower-quality alternatives. `MorphismList::truncated` records whether enumeration of tied optima stopped at the engine cap. An empty `morphisms` vector means that no total morphism exists; a search that could not run returns `Err`.

`SearchOptions::default()` leaves `monic`, `epic`, and `iso` false, uses no hard pins, and sets `max_results` to zero. Here zero requests all optima the implementation will enumerate, subject to its safety cap. Use `find_morphisms_budgeted` or [`SpanSearch`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) when the caller must supply a `SearchBudget`.

## Ownership and errors

Rust values follow ordinary ownership and drop semantics; the Rust API does not expose the opaque-handle lifecycle used by foreign-language bindings. Fallible operations return the error type declared by their component crate. `panproto-core` does not replace those errors with a facade-wide error enum.

## See also

- [Install the Rust SDK](../how-to/install/rust.md)
- [Define a schema from Rust](../how-to/define-schema/rust.md)
- [Find a span between two schemas](../how-to/spans.md)
- [Searching for a morphism](../explanation/morphism-search.md)
