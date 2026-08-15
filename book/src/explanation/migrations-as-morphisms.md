# Migrations as morphisms

## In plain terms

A migration is what gets you from version 3 of your schema to version 4, with all your existing version-3 data carried forward into version-4 shape. Hand-written migrations are usually a script that walks each record, renames fields, fills in defaults, drops what is no longer there.

panproto represents a migration as a structured map between the two schemas: for every vertex (record kind) in the old schema, where does it land in the new one; for every edge (field, item, variant), how is it derived. The structured map is called a *morphism*. Once you have a morphism, two operations follow:

- **Restrict** moves the morphism backwards: it tells you what part of the old schema is needed to produce a given part of the new one.
- **Lift** moves data forwards: it takes a record that conforms to the old schema and produces a record that conforms to the new one, using the morphism to know what to put where.

Lift is the operation you usually want; restrict is the operation panproto uses internally to figure out which old fields are required for the migration to succeed. Both are total functions on the things they apply to; if a migration cannot be lifted (because some required input is missing), `panproto-check` says so up front rather than failing partway through.

The map is not obliged to cover the whole old schema. A field with nowhere to go in the new schema has nowhere to go, and no amount of searching invents a home for it. panproto returns a *span* in that case: the part of the old schema that did find a home, paired with the two maps out of it. A migration that covers everything is then the special case, rather than the price of getting an answer at all.

## The formal picture

Schemas live in a category whose objects are schema theories and whose morphisms are theory morphisms. A *migration* from schema $S$ to schema $T$ is a morphism $f : S \to T$ in this category. The migration engine is split into two functors:

- **Restrict** $\Delta_f : T\text{-Inst} \to S\text{-Inst}$: pulls a $T$-instance back to an $S$-instance along $f$. Used to check existence conditions: which $S$-records does $T$ require to be present?
- **Lift** $\Sigma_f : S\text{-Inst} \to T\text{-Inst}$: pushes an $S$-instance forward to a $T$-instance along $f$. Used to actually migrate data.

In panproto, $\Sigma_f$ and $\Delta_f$ are implemented as operations, not as a verified adjunction: lift is [`panproto-mig::lift`](https://docs.rs/panproto-mig/latest/panproto_mig/lift/) (built on [`panproto-inst::wtype_extend`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/fn.wtype_extend.html)) and restrict is [`panproto-inst::wtype_restrict`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/fn.wtype_restrict.html). The intended semantics is the adjunction $\Sigma_f \dashv \Delta_f$ on the categories of instances, following the convention in @spivakwisnesky2015relational, where $\Sigma_f$ is the dependent sum over the fibres of $f$ and $\Delta_f$ is its right adjoint by base change. That adjunction is scoped to migrations that are total on vertices, and its unit, counit, and hom-set bijection are not yet constructed or property-tested in code. What panproto actually checks before any data moves is `panproto-check`'s existence condition: whether every input a migration requires is present. This scoping can be restored to a tested-for-total-vertex-maps claim once the unit and counit are constructed and the hom bijection is property-tested.

A migration may also include a value-level transform: not just *where* a field comes from, but *how* its value is computed. These are written in the [expression language](./semantics/expression-language.md) and applied during lift.

## Spans, and why partiality is the ordinary case

A *span* from $S$ to $T$ is a pair of morphisms out of a shared domain,

$$S \xleftarrow{\;\ell\;} A \xrightarrow{\;r\;} T,$$

whose shared domain $A$ is the **apex** and whose two arrows are the **legs** [@johnsonrosebrugh2014spans]. (Johnson and Rosebrugh write *peak* for what we are calling the apex; the two words name the same object.)

In panproto, $A$ is the sub-schema of $S$ induced on exactly those vertices [the search](./morphism-search.md) found an image for, which makes the left leg an inclusion: apex vertex identifiers *are* source vertex identifiers, and $\ell$ is the identity on both vertices and edges. Cutting that sub-schema out is less mechanical than it sounds, and [`panproto_schema::induce`](https://docs.rs/panproto-schema/latest/panproto_schema/induce/fn.induce.html) is the one supported way to do it. A `Schema` carries twenty-one fields spread over four key spaces, of which only three are derived, so copying `vertices` and `edges` and cloning the rest leaves required-edge lists naming removed edges, hyper-edge signatures and recursion points naming removed vertices, and adjacency indices handing callers arcs into vertices that are gone. `induce` restricts each field in its own key space and then re-validates the result against the protocol.

The total morphism falls out as the degenerate case. A span is a morphism exactly when $\ell$ is invertible, and an inclusion is invertible exactly when it is surjective; where surjectivity holds, the composite $r \circ \ell^{-1} : S \to T$ is a migration in the sense of the previous section, and [`SchemaSpan::is_total`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) is the test for it.

That case is the minority one on the schemas panproto is measured against, and by a wide margin. A source vertex whose kind the target schema never uses has nowhere at all to go, and the pair that motivated the rewrite had six of them: `app.bsky.feed.post` declares four `array` and two `integer` vertices, and `app.bsky.actor.profile` holds neither kind, so no total morphism between those two exists. The rate is measured over the whole corpus by `panproto-mig/tests/lexicon_domain_shapes.rs`: of the 5852 ordered pairs of the 77 ATProto lexicons, 735 admit a total morphism and 5117 do not, so the degenerate case arises on 12.6% of them. On 4950 of those 5117 the reason is the one above, a source vertex with no kind-compatible target anywhere; on the remaining 167 every domain is non-empty and naturality empties the hom-set on its own. [Searching for a morphism](./morphism-search.md#what-the-corpus-measures) gives the breakdown. A search that answers only with total morphisms refuses on all 5117, and a refusal is a poor rendering of "these two schemas overlap partially".

This is why partiality is a value the search assigns rather than a condition it fails on. Leaving every source vertex out of the apex is always feasible, so an answer exists for every pair, and "these two share nothing" comes back as an empty apex rather than as an error.

## Span equivalence collapses here

Classical span equivalence asks for an isomorphism between two apices commuting with both legs [@johnsonrosebrugh2014spans]. That relation collapses in panproto's setting, and the collapse is convenient. Because $\ell$ is an inclusion, an apex is determined by its vertex set, so equivalence classes of spans are in bijection with pairs $(A \subseteq V_S,\; r : A \to V_T)$: a subset of the source vertices, and a map out of it. No quotient is taken and no graph-isomorphism test runs; a span's identity is the content digest of its apex together with the two leg maps.

Johnson and Rosebrugh themselves replace classical span equivalence with a weaker relation, on the grounds that the classical one is far too strong once the legs are lenses rather than bare morphisms. That weakening is a claim about lenses between apices. panproto does not attempt it at the schema layer, where there is no lens structure available to check it against.

The bijection also explains a preference of the search that would otherwise read as an arbitrary tie-break: at equal quality it returns the largest apex. The part of the source that the apex leaves out is a complement in the sense of @bancilhonspyratos1981update, that is, the further view alongside which the user's own view determines the whole database, and it is the object on which their characterisation of translatable updates turns. Preferring the largest apex is the schema-level form of preferring the smallest complement. We are borrowing the vocabulary rather than importing the theorem, though: panproto's spans run between two schemas rather than between a database and a view of it, and no constant-complement result has been carried across that gap.

## Two apices, and why they are not the same object

A span is the input a pushout wants, and merging the two schemas along the apex completes the square

$$
\begin{CD}
  A  @>{r}>>   T          \\
  @V{\ell}VV   @VV{j}V    \\
  S  @>>{i}>   S \sqcup_A T
\end{CD}
$$

The merged schema is the pushout, written $S \sqcup_A T$, and $i$ and $j$ are its two injections; the square commutes, so an apex vertex reaches the same merged vertex whichever leg it travels.

Write $\mathrm{Mod}(S) = [S, \mathbf{Set}]$ for the category of instances of a schema. Because $\mathrm{Fun}(-, \mathbf{Set})$ carries colimits to limits,

$$\mathrm{Mod}(S \sqcup_A T) \;\cong\; \mathrm{Mod}(S) \times_{\mathrm{Mod}(A)} \mathrm{Mod}(T),$$

the pullback on the right being the category of pairs of instances, one on each side, that agree after restriction to the shared part. Restricting along the two pushout injections is thus a span of *get* functions, the forward direction of a [lens](./lenses-roundtrip.md), whose apex is the category of consistent pairs. That object is the *consistent triples* of @johnsonrosebrugh2014spans, defined there as an equalizer (their Proposition 9), and it is what a symmetric lens has for an apex.

It is a different object from $A$. Call $A$ the **schema apex** and $\mathrm{Mod}(S \sqcup_A T)$ the **instance apex**: the first is a shared schema, the second a space of consistent instances, and the two are related by pushout followed by $\mathrm{Mod}$ rather than being two names for one thing. panproto keeps them apart in code as well as in prose: the search returns the span of schema morphisms, and the merge is derived on demand.

The square imposes a precondition on the merge. An apex vertex has to reach the same merged vertex through either leg, and a right leg sending two apex vertices to one target vertex makes that impossible, since the merge identifies elements by a map keyed on the target element and a repeated key names only one preimage. A contracting right leg is an ordinary answer from the default search: a source with four string fields and a target with one produces one. `SchemaSpan::pushout` thus reports the contraction rather than returning a square that does not commute, and a caller who wants a symmetric lens out of the span runs the injective search, which rules the case out.

Turning the intended adjunction of the previous section into a theorem would take a construction panproto does not yet have. @johnsonrosebrughwood2012lenses identify c-lenses with Grothendieck opfibrations and show that this supplies a universal solution to the view-update problem for functorial views. That is the shape the verified account would take. The code today has the pair of operations and the existence check, and nothing beyond them.

## The length-1 fragment

@spivak2012functorial takes a schema to be a small category and an instance to be a set-valued functor on it, with a translation between schemas a functor between the categories. A functor may send a generating arrow of the source to any arrow of the target, and in a category presented by generators and relations the arrows of the target include composites of generators, so one field in the source may perfectly well translate to a path of fields in the target.

panproto's [`SchemaMorphism`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.SchemaMorphism.html) does not go that far. Its `edge_map` is a map from `Edge` to `Edge`, so an edge goes to an edge and never to a path. Call this the **length-1 fragment** of the morphism class: the translations whose image on each generating arrow has length one. [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) carries an `edge_map` of the same shape, so every span the search returns lives in the fragment, and so does every migration read from a mapping file.

The cost is that 1:n correspondences are not expressible as morphisms. A source field that ought to become three target fields, or that ought to be reached through a chain of two edges on the target side, has no morphism to be. panproto handles those at the value level instead, through [`FieldTransform::ComputeField`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/enum.FieldTransform.html), which writes a target key from an expression over the record's fields and carries an optional inverse expression together with a [`CoercionClass`](https://docs.rs/panproto-gat/latest/panproto_gat/enum.CoercionClass.html) recording what a round trip recovers. Absent an inverse, the class is `Projection` where the value is deterministically re-derivable and `Opaque` where it is not, and the complement then carries what the forward direction cannot reconstruct. The data moves correctly. The correspondence, though, has stopped being a statement in the schema category, so nothing at the schema layer can reason about it.

The fragment is not the only place the implementation is narrower than the mathematics it sits in. First, the objective the span search minimises reads four of `Schema`'s twenty-one fields: `vertices`, `edges`, and the `outgoing` and `between` adjacency indices derived from `edges`. The `incoming` index is not among them, and the other seventeen fields never move a score. Five of the seventeen (required edges, variants, recursion points, spans, and hyper-edge signatures) enter as feasibility constraints and so decide which apices are well formed at all, while the remaining twelve are restricted by `induce` and are otherwise invisible to the search. Second, the weights trading the objective's four components off against one another have never been calibrated against a labelled corpus of correct alignments. They encode a judgement about what a good match looks like, and that judgement is so far untested.

## Compatibility tiers

The shipped classifier sorts a migration into three tiers, recorded as the `classification` field on `CompatReport`. It reads a migration that already exists, whether that came from a mapping file or from a span search. The two non-breaking tiers split apart the case where nothing of consequence changed from the case where old data still lifts through a non-breaking change.

| Tier | `classification` | Example | Lift behaviour |
|---|---|---|---|
| Fully compatible | `fully-compatible` | No breaking and no non-breaking changes; the two schemas agree in shape. | Total; old records lift unchanged. |
| Backward compatible | `backward-compatible` | Add an optional field with a default, or add a required field whose value is computed from existing fields. | Total; old records lift, either unchanged or via the value-level transform. |
| Breaking | `breaking` | Remove a required field with no recovery. | Partial; some old records cannot be lifted. `panproto-check` flags this. |

`panproto-check` runs the existence check on a migration without applying it, so CI can gate on the result before merge. See [Breaking-change gate](../how-to/ci/breaking-change-gate.md).

## What this gives you

- You write the migration *map* (what goes where), not the migration *script* (how to execute it). The script is generated from the map and the schema theories.
- You get a precise classification of the change into one of three compatibility tiers, before any data is touched.
- You get a *bidirectional* artifact: the lift forward is paired with a put backward, so the migration is also a [lens](./lenses-roundtrip.md). Round-trip laws apply.
- Migrations compose. If $f : S \to T$ and $g : T \to U$, then $g \circ f : S \to U$ is a migration whose lift agrees with applying $f$'s lift then $g$'s. The migration history of a schema is a chain of these compositions.
- You get an answer for every ordered pair of schemas, since a span search cannot fail for want of a match. What varies between pairs is how much of the source the apex covers, and the span reports that as a number rather than leaving you to infer it from a refusal.

## See also

- [Searching for a morphism](./morphism-search.md) for how the span is found and what the search proves about it.
- [Find a span between two schemas](../how-to/spans.md) for running the search and reading its certificate.
- [Lenses and round-trip laws](./lenses-roundtrip.md) for the bidirectional half.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the universal property the merge square satisfies.
- [Apply field transforms](../how-to/field-transforms.md) for value-level transforms.
- [What panproto verifies](./what-is-verified.md) for the existence-check guarantee.
- @spivakwisnesky2015relational for the categorical data-migration framework this builds on.
