# Lenses and round-trip laws

Suppose a source record contains `name` and `age`, while a view exposes only `name`. Reading the view discards `age`, but a later update to `name` should preserve it. A lens coordinates this forward observation and backward reconstruction [@foster2007combinators]. In panproto, the forward operation returns both the view and a **complement**, a record of information needed to reconstruct the source. This explicit complement is related to the constant-complement view-update account and later symmetric lenses [@bancilhonspyratos1981update; @hofmann2011symmetric].

A concrete `Lens` stores a compiled migration together with its source and target schemas. Its operations are fallible because migration execution or reconstruction may reject a concrete input. For a fixed lens, their mathematical shape is

$$
\mathit{get}:S\to V\times C
\qquad
\mathit{put}:V\times C\to S.
$$

In Rust, `get` returns a `WInstance` view and a `Complement`, and `put` consumes the edited view and complement to reconstruct a source `WInstance`. The complement represents the original source; callers do not pass it separately to `put`.

## Round-trip laws

Three equations describe the expected interaction between these operations. Writing $\mathit{get}(s)=(v,c)$, **GetPut** requires $\mathit{put}(v,c)=s$. If $\mathit{put}(v',c)=s'$, **PutGet** requires the view component of $\mathit{get}(s')$ to equal $v'$. **PutPut** requires a later update to supersede an earlier update, with the intermediate complement threaded according to the lens.

The implementation provides checks with different scopes. `check_laws` runs GetPut and PutGet for a supplied lens and source instance. Its PutGet check uses the original view and one fixed scalar mutation, so it is a deterministic smoke check rather than a universal quantification over edits. `panproto_lens::laws::check_put_put` is a separate operation. It gets the original complement, performs an initial put, gets the complement of that intermediate source, and compares a sequential second put with a direct second put from the original complement.

Property tests exercise the equations over generated lens and instance families. These tests can reveal failures within those families, but they do not prove lawfulness for every migration or input. Constructing a `Lens` also does not run every law check automatically. [What panproto verifies](./what-is-verified.md) distinguishes these forms of evidence.

## Complement composition

Composed lenses must also combine their complements. The `ComplementCompose` extension trait defines a checked partial composition through `compose` and `is_compatible`. Two nonzero source-schema fingerprints must agree; zero represents an unspecified fingerprint. Conflicting nonzero fingerprints produce `ComplementFingerprintMismatch`. If both complements store a value for the same key, those values must agree or composition returns `ComplementConflict`. Collection-valued fields are combined without duplicate elements.

These conditions prevent composition from selecting arbitrarily between incompatible saved states. They establish compatibility of the concrete complements being composed, rather than lawfulness of the underlying lenses.

## Lens construction

[Migrations as morphisms](./migrations-as-morphisms.md) describes the compiled migration that supplies a concrete lens's transformation tables. A migration and its endpoint schemas can be assembled into a `Lens`, after which callers may run the law checks above. This construction does not imply that every migration is lawful.

The [panproto-lens-dsl](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl) crate compiles declarative Nickel, JSON, or YAML descriptions into lens combinators. The [`panproto-lens::protolens`](https://docs.rs/panproto-lens/latest/panproto_lens/protolens/) module describes schema-parameterized transformations that can be instantiated when their structural preconditions hold. Edge kinds select optic forms such as lenses, prisms, affine traversals, and traversals; this dispatch follows the profunctor-optics account of @pickeringgibbonswu2017profunctor and its categorical formulation in @clarke2020profunctor. The representation of schema migrations as a graph of lenses is related to Cambria [@littvanhardenberghenry2020cambria].

`EditLens::put_edit` translates a view edit back to the source and updates the stored complement at the same time. Edit lenses model changes through edit monoids and actions [@hofmann2012edit]. The subtree-complement rules here are panproto-specific: deleting a subtree clears its complement entries, while inserts, relabels, and field updates maintain the relevant saved state. The edit-law helpers compare this incremental result with whole-state `get` and `put` for one supplied edit.

`SymmetricLens::from_span` requires its two asymmetric legs to share a middle schema. The current equality check compares the protocol, vertices, edges, hyperedges, constraints, required edges, variants, orderings, recursion points, and nominal flags. It deliberately omits byte-layout constraints and usage modes, and it also does not compare NSIDs, entry lists, schema-span annotations, coercions, mergers, defaults, policies, or derived adjacency indices. Acceptance by this constructor is thus equality under that implemented projection, not complete `Schema` equality.

## Layout enrichment

Layout preservation is described at the schema level rather than as an ordinary `WInstance` lens. The `parse_emit_protolens` construction strips `EnrichmentKind::Layout` from its source theory and adds the enrichment to its target theory. Its `ComplementConstructor::Enrichment` records the enrichment kind and synthesis driver. `apply_theory_transform_to_schema` interprets these transformations by removing layout constraints or dispatching to the registered driver that synthesizes them.

This protolens describes a relation between abstract and layout-decorated schemas. It can participate in protolens composition, but instantiating it does not itself parse or emit bytes. The operational parsing and decoration entry point is `ParserRegistry::decorate`; formatted output is produced by `pretty_with_protocol` and `emit_pretty_with_protocol`. [Layout enrichment](./layout-enrichment.md) gives the byte-level and schema-level accounts.

## See also

- [Lens DSL: denotational semantics](./semantics/lens-dsl.md) for the formal lens model and law statements.
- [Protolens composition](./semantics/protolens-composition.md) for vertical and sequential composition.
- [Layout enrichment](./layout-enrichment.md) for abstract and decorated schemas.
- [Lens combinator reference](../reference/lens-combinators.md) for the combinator algebra.
