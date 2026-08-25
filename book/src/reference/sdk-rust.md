# Rust SDK reference

The [Rust](https://www.rust-lang.org/) entry point is the [`panproto-core`](https://docs.rs/panproto-core/latest/panproto_core/) facade. Version 0.72 requires Rust 1.85 or later.

```toml
[dependencies]
panproto-core = "0.72"
```

The facade re-exports each component crate as a module. For instance, `panproto_core::schema::Schema` is the `Schema` type from `panproto-schema`. Depending on `panproto-schema` directly exposes the same type without the facade.

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

## Instance transport and direction

Let a compiled migration have schema direction \(F:S\to T\). The functorial data-migration names \(\Sigma_F\) and \(\Delta_F\) follow the usual source and target directions [@spivak2012functorial], but the plain `lift` API is a separate operation:

| Rust API | Instance direction | Implemented operation |
|---|---|---|
| [`lift_wtype`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.lift_wtype.html), `lift_functor` | \(S\to T\) | Runs the restrict pipeline. It retains the source fragment that survives the compiled mapping, remaps it into the target schema, and may prune data. |
| [`lift_wtype_sigma`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.lift_wtype_sigma.html), `lift_functor_sigma` | \(S\to T\) | Computes the left Kan extension \(\Sigma_F\). The functor form may then run the supplied chase dependencies. |
| [`lift_wtype_pi`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.lift_wtype_pi.html) | \(S\to T\) | Runs the `pi`-named W-type path. It accepts only vertex-injective maps and relabels the tree rather than constructing a general right Kan extension. Its `max_product_nodes` parameter is retained for signature compatibility and is unused. |
| `lift_functor_pi` | \(S\to T\) | Computes the functor-instance right Kan extension \(\Pi_F\) by products over fibers and applies `max_product_size`. |
| [`w_delta`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/fn.w_delta.html), [`f_delta`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/fn.f_delta.html) | \(T\to S\) | Computes precomposition \(\Delta_F\). The W-type form is defined only for injective vertex and edge maps and for anchors in the image. The functor form also handles vertex-merging maps. |

Thus `panproto_mig::lift_wtype` is not \(\Delta_F\), despite its delegation to `wtype_restrict`. [`Instance::restrict`](https://docs.rs/panproto-inst/latest/panproto_inst/enum.Instance.html#method.restrict) has the same source-to-target surviving-fragment direction. [`Instance::extend`](https://docs.rs/panproto-inst/latest/panproto_inst/enum.Instance.html#method.extend) is a total source-to-target extension path. The W-type \(\Sigma_F\) implementation delegates to that path, but the generic method does not establish an adjunction for every `Instance` variant.

## Morphism search

The [`panproto_core::mig::hom_search`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/) module distinguishes partial overlap from total morphisms. A returned [`SchemaSpan`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) has shape \(S\leftarrow A\to T\): \(A\) is the sub-schema of \(S\) induced by mapped source vertices, the left leg includes \(A\) into \(S\), and the right leg maps \(A\) into \(T\).

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

`find_morphisms` returns total morphisms that attain the optimum, rather than every morphism or a sequence of lower-quality alternatives. `MorphismList::truncated` records whether enumeration of tied optima stopped at the engine cap. An empty `morphisms` vector means that no total morphism exists. A search that could not run returns `Err`.

`SearchOptions::default()` leaves `monic`, `epic`, and `iso` false, uses no hard pins, and sets `max_results` to zero. Here zero requests all optima the implementation will enumerate, subject to its safety cap. Use `find_morphisms_budgeted` or [`SpanSearch`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) when the caller must supply a `SearchBudget`.

## Ownership and errors

Rust values follow ordinary ownership and drop semantics. The Rust API does not expose the opaque-handle lifecycle used by foreign-language bindings. Fallible operations return the error type declared by their component crate. `panproto-core` does not replace those errors with a facade-wide error enum.

## See also

- [Install the Rust SDK](../how-to/install/rust.md)
- [Define a schema from Rust](../how-to/define-schema/rust.md)
- [Find a span between two schemas](../how-to/spans.md)
- [Searching for a morphism](../explanation/morphism-search.md)
