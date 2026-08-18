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

The default feature set re-exports thirteen always-on crates: `panproto-gat`, `panproto-schema`, `panproto-inst`, `panproto-mig`, `panproto-lens`, `panproto-check`, `panproto-protocols`, `panproto-io`, `panproto-vcs`, `panproto-expr`, `panproto-expr-parser`, `panproto-lens-dsl`, and `panproto-theory-dsl`. The feature flags above add the optional crates.

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
| Decorate an abstract schema with a layout fiber | `panproto-parse` (`ParserRegistry::decorate`, `ParserRegistry::pretty_with_protocol`) |
| Distinguish abstract and decorated schemas at the type level | `panproto-schema` (`AbstractSchema`, `DecoratedSchema`, `LayoutWitness`) |
| Search for a morphism or a span between two schemas | `panproto-mig` (`hom_search`, `span`, `solve`) |
| Cut a well-formed sub-schema out of a schema | `panproto-schema` (`induce`, `induce_on_vertices`) |
| Take a schema's content identity | `panproto-schema` (`canonical_bytes`, `canonical_digest`) |

## Morphism and span search

Searching for a map between two schemas goes through [`panproto_mig::hom_search`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/). The primary entry point is `find_span`, which returns a [`SchemaSpan`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) of the form $\mathit{src} \leftarrow A \to \mathit{tgt}$. The apex $A$ is the sub-schema of `src` induced on the vertices the search assigned a target. Its signature is:

```text
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
| `left: Migration` | Inclusion from the apex into `src`; both maps are the identity on the apex. |
| `right: Migration` | Assignment from the apex into `tgt`, with each apex edge sent to the target edge between the images of its endpoints. |
| `quality: f64` | How well the covered part matches, in `[0, 1]`, with the drop count excluded. |
| `quality_bounds: (f64, f64)` | The interval bracketing `quality`. Its ends are equal exactly when `certificate.proven_optimal` holds. |
| `apex_coverage: f64` | `apex.vertices.len()` over `src.vertices.len()`, or one when the source has no vertices. |
| `certificate: SpanCertificate` | What the construction measured rather than assumed: `proven_optimal`, the `LegShape` of the two legs, functoriality of both, an `ExistenceReport` for each, whether the apex is pointed, the apex's `canonical_digest`, which `SolverPath` ran, the tie-break order, and any `LimitKind` that stopped the search. |

`quality` ranks spans over *one* source schema and nothing else, because every denominator of the objective is fixed by `src`. An empty apex charges the full penalty on each component the source gives mass to, and the source decides which components those are: name and degree are per source vertex and always charge, while the edge and Jaccard components are per source edge and per source vertex with a named outgoing edge. So an empty apex scores `0.0` over a source with at least one named edge, `0.30` over a source whose edges are all unnamed, `0.55` over an edgeless source, and `1.0` over an empty source. All four say that the schemas share nothing, on four different scales, which is why a caller ranking pairs reads `apex_coverage` alongside the score.

`SchemaSpan::is_total` holds when the left leg is onto, and `as_total_morphism` then converts the span to the `FoundMorphism` shape. `to_overlap` yields the pair lists `panproto_schema::schema_pushout` takes, and `pushout` merges the two schemas along the apex. For anything beyond the four arguments above, [`SpanSearch`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SpanSearch.html) is the builder: `with_constraints` states hard domain restrictions, `with_evidence` supplies the alignment anchors the objective's anchor term reads, `with_weights` sets the five component weights, `with_budget` moves what the search may spend, and `with_theories` gives the existence check something to run its conditional obligations against.

### What `find_morphisms` returns

`find_morphisms` returns the morphisms **attaining the optimum**, and nothing else. Every element carries the same quality, which is the maximum over all total morphisms, so the list is trivially in non-increasing quality order and `results[0]` is the best answer there is. A caller walking further down the list for a suboptimal alternative will not find one, because there is no k-best over distinct quality levels.

The return type is [`MorphismList`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/struct.MorphismList.html) rather than a bare `Vec`, and the second field is why. `DEFAULT_OPTIMA_CAP`, which is 1024, bounds **every** request rather than only the unbounded-sounding `max_results = 0`: a larger figure is answered with the cap and `MorphismList::truncated` says so. It has to, because the enumeration materializes one `FoundMorphism` per optimum and the count of optima is a property of the pair rather than of its size. Two eight-vertex schemas with no edges and no shared name characters tie the whole hom-set at the optimum, which is `8^8` morphisms: 4.6 GB and 164 seconds uncapped against 4.6 MB and 11 ms capped. `truncated` is the only way to tell a list the cap cut from a list the pair exhausted, and it does not cross any other SDK.

`Ok(MorphismList { morphisms: vec![], .. })` means that no total morphism exists, and only that. A search that could not be posed reports `SpanError::Build`, and one that spent its budget before reaching any complete assignment reports `SpanError::Stopped` carrying the `LimitKind` that ran out, so an empty answer is always a statement about the pair rather than about the budget. `find_span` is the entry point that answers with what the two schemas do share.

The four ordinary entry points spend `SearchBudget::default`. [`find_morphisms_budgeted`](https://docs.rs/panproto-mig/latest/panproto_mig/hom_search/fn.find_morphisms_budgeted.html) and `find_best_morphism_budgeted` take the budget as an argument instead, which is what a host that has to bound the search needs and what `SpanSearch::with_budget` already is for the span search. Shrinking a budget turns answers into refusals and never into wrong answers.

Three things a caller might reach for are deliberately absent. There is no preferred-mapping setting, because a preference can only change which optimum is found first while a unary cost changes which assignment is optimal, and evidence enters as a cost. There is no edge-name domain pruner, because edge-name agreement already enters the objective and a soft signal used as a hard filter removes correct answers. There is no name-similarity threshold, for the same reason applied to full path-like identifiers. A caller wanting a hard restriction states it in `DomainConstraints::restricted_domains`, and a node budget lives on `SearchBudget`, where exhausting it is reported through `SpanCertificate::limit_hit` rather than absorbed.

## See also

- [Install the Rust SDK](../how-to/install/rust.md) for setup.
- [Define a schema from Rust](../how-to/define-schema/rust.md) for the canonical entry point.
- [Crate map](./crate-map.md) for the complete workspace.
- [Find a span between two schemas](../how-to/spans.md) for the task, and [Searching for a morphism](../explanation/morphism-search.md) for what the search is doing.
