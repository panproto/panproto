# Rust SDK reference

The Rust surface of panproto is the `panproto-core` facade. Add it to your `Cargo.toml`:

```toml
[dependencies]
panproto-core = "0.71"
```

Full type signatures, constructors, and method documentation live on docs.rs:

- [`panproto-core` on docs.rs](https://docs.rs/panproto-core)

## Feature flags

| Feature | Effect |
|---|---|
| `full-parse` | Pulls in `panproto-parse` and tree-sitter-based AST parsing. |
| `project` | Pulls in `panproto-project` for multi-file project assembly. |
| `git` | Pulls in `panproto-git` for the git bridge. |
| `tree-sitter` | Enables tree-sitter-based format-preserving parsing for built-in protocols (forwards to `panproto-io/tree-sitter`). |

The default feature set re-exports the always-on crates: `panproto-gat`, `panproto-schema`, `panproto-inst`, `panproto-mig`, `panproto-lens`, `panproto-check`, `panproto-protocols`, `panproto-io`, and `panproto-vcs`. The feature flags above pull in the optional crates on top.

## Sub-crate lookup

For lower-level work, depend on individual crates rather than the facade. The [crate map](./crate-map.md) lists every workspace member with a one-line description and a link to its docs.rs page.

| Task | Crate |
|---|---|
| Define a GAT in Rust | `panproto-gat`, `panproto-gat-macros` |
| Validate a schema against a protocol | `panproto-schema`, `panproto-protocols` |
| Parse data and produce an instance | `panproto-io`, `panproto-inst` |
| Build and apply a migration | `panproto-mig` |
| Construct or compose lenses | `panproto-lens` |
| Write lenses in the lens DSL | `panproto-lens-dsl` |
| Use the expression language | `panproto-expr`, `panproto-expr-parser` |
| Version-control schemas and data | `panproto-vcs`, `panproto-git` |
| Parse full ASTs across 261 languages | `panproto-parse` |
| Decorate an abstract schema with a layout fibre | `panproto-parse` (`ParserRegistry::decorate`, `ParserRegistry::pretty_with_protocol`) |
| Distinguish abstract and decorated schemas at the type level | `panproto-schema` (`AbstractSchema`, `DecoratedSchema`, `LayoutWitness`) |
| Search for a morphism or a span between two schemas | `panproto-mig` (`hom_search`, `span`, `solve`) |
| Cut a well-formed sub-schema out of a schema | `panproto-schema` (`induce`, `induce_on_vertices`) |
| Take a schema's content identity | `panproto-schema` (`canonical_bytes`, `canonical_digest`) |

## Morphism and span search

Searching for a map between two schemas goes through [`panproto_mig::hom_search`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/). The primary entry point is `find_span`, which answers with a [`SchemaSpan`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html): a span `src ← A → tgt` whose apex `A` is the sub-schema of `src` induced on the vertices the search gave a target.

```rust,ignore
pub fn find_span(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    opts: &SearchOptions,
) -> Result<SchemaSpan, SpanError>
```

The protocol is an argument because the apex is a schema, and a schema is well formed only against a protocol; inducing the apex re-validates it rather than assuming it, and a `Schema` stores its protocol's name alone. `find_span` never refuses for want of a match, since leaving every source vertex out of the apex is always feasible, so a pair with nothing in common returns an empty apex. An `Err` therefore says that the search could not be posed or that the induced apex did not validate. It never says that no morphism exists.

| Field of `SchemaSpan` | What it holds |
|---|---|
| `apex: Schema` | The sub-schema of `src` the search covered, cut with `panproto_schema::induce` and re-validated against `protocol`. |
| `left: Migration` | `A → src`, an inclusion, so both of its maps are the identity on the apex. |
| `right: Migration` | `A → tgt`: the search's assignment restricted to the apex, with each apex edge sent to the target edge lying between the images of its endpoints. |
| `quality: f64` | How well the covered part matches, in `[0, 1]`, with the drop count excluded. |
| `quality_bounds: (f64, f64)` | The interval bracketing `quality`. Its ends are equal exactly when `certificate.proven_optimal` holds. |
| `apex_coverage: f64` | `apex.vertices.len()` over `src.vertices.len()`, or one when the source has no vertices. |
| `certificate: SpanCertificate` | What the construction measured rather than assumed: `proven_optimal`, the `LegShape` of the two legs, functoriality of both, an `ExistenceReport` for each, whether the apex is pointed, the apex's `canonical_digest`, which `SolverPath` ran, the tie-break order, and any `LimitKind` that stopped the search. |

`quality` ranks spans over *one* source schema and nothing else, because every denominator of the objective is fixed by `src`. An empty apex charges the full penalty on each component the source gives mass to, and the source decides which components those are: name and degree are per source vertex and always charge, while the edge and Jaccard components are per source edge and per source vertex with a named outgoing edge. So an empty apex scores `0.0` over a source with at least one named edge, `0.30` over a source whose edges are all unnamed, `0.55` over an edgeless source, and `1.0` over an empty source. All four say that the schemas share nothing, on four different scales, which is why a caller ranking pairs reads `apex_coverage` alongside the score.

`SchemaSpan::is_total` holds when the left leg is onto, and `as_total_morphism` then converts the span to the older `FoundMorphism` shape. `to_overlap` yields the pair lists `panproto_schema::schema_pushout` takes, and `pushout` merges the two schemas along the apex. For anything beyond the four arguments above, [`SpanSearch`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) is the builder: `with_constraints` states hard domain restrictions, `with_evidence` supplies the alignment anchors the objective's anchor term reads, `with_weights` sets the five component weights, `with_budget` moves what the search may spend, and `with_theories` gives the existence check something to run its conditional obligations against.

### `find_morphisms` no longer returns the hom-set

This is a silent behavioural change, so a consumer that upgrades without reading this paragraph will get different answers from the same call. `find_morphisms` used to enumerate the whole hom-set, score every member, sort by quality, and truncate. It now returns the morphisms **attaining the optimum**, capped by `SearchOptions::max_results`, and nothing else. Every element carries the same quality, which is the maximum over all total morphisms, so `results[0]` is what it always was and the list is trivially in non-increasing quality order. A caller walking further down the list for a suboptimal alternative will not find one, because there is no k-best over distinct quality levels. A `max_results` of zero now means every optimum the search enumerates, up to `DEFAULT_OPTIMA_CAP`, rather than the whole hom-set.

`Ok(vec![])` means that no total morphism exists, and only that: a search that could not run reports `Err`, so the two are distinguishable. `find_span` is the entry point that answers with what the two schemas do share.

Three settings went with the change rather than being reimplemented. Preferred vertex mappings gave way to a unary cost, which is the stronger instrument, since a preference can only change which optimum is found first while a cost changes which assignment is optimal. The edge-name domain pruner went because it was a soft heuristic used as a hard filter, and because edge-name agreement already enters the objective anyway. The name-similarity threshold went for a related reason: it cut a soft signal at a hard edge, over full path-like identifiers. A caller wanting a hard restriction states it in `DomainConstraints::restricted_domains`, and the node budget now lives on `SearchBudget`, where exhausting it is reported through `SpanCertificate::limit_hit` rather than absorbed.

## See also

- [Install the Rust SDK](../how-to/install/rust.md) for setup.
- [Define a schema from Rust](../how-to/define-schema/rust.md) for the canonical entry point.
- [Crate map](./crate-map.md) for the complete workspace.
- [Find a span between two schemas](../how-to/spans.md) for the task, and [Searching for a morphism](../explanation/morphism-search.md) for what the search is doing.
