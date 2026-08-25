# Migrations as morphisms

Suppose a source schema contains a field named `age` and a target schema contains the corresponding field under the name `years`. A migration records the map from the source field to the target field. If the value also changes representation, the migration may associate that correspondence with an expression that computes the target value.

This representation separates a structural map from its execution. The map can be validated, stored, and composed before it is applied to data. A compiled migration contains the tables and value resolvers needed by the instance layer.

## Maps between concrete schemas

Let $S$ and $T$ be concrete schemas. In the functorial account of data migration, a schema morphism $f:S\to T$ maps source structure to target structure and induces operations between their instance categories [@spivak2012functorial; @spivakwisnesky2015relational]. In panproto's concrete representation, $f$ maps source vertices and edges to target vertices and edges. It must preserve edge endpoints and land in the target schema. The free function [`panproto_mig::compile`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.compile.html) checks this mapped fragment and builds the tables used during instance migration. It does not run the separate migration-existence check.

Data can then move along the compiled map. The names *lift* and *restrict* are overloaded in the current APIs, so the function and its input direction matter.

- [`lift_wtype`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.lift_wtype.html) and [`lift_functor`](https://docs.rs/panproto-mig/latest/panproto_mig/fn.lift_functor.html) take an $S$-instance and return a $T$-instance containing the fragment that survives the compiled migration. When several source vertices map to one target vertex, `lift_functor` concatenates their row sets. These functions are forward projections. They are not the categorical restriction $\Delta_f$.
- `lift_wtype_sigma` and `lift_functor_sigma` also run from $S$ to $T$. The W-type operation requires every source anchor to have an image. The functor operation applies `functor_extend` and may then run the term-level chase supplied by its caller.
- `lift_wtype_pi` is implemented only for vertex-injective migrations and relabels rather than constructing a product. `lift_functor_pi` computes Cartesian products over fibers and enforces its product-size limit.

The `schema lift` command always parses its input under `--src-schema` and emits under `--tgt-schema`. This direction is unchanged by `--direction restrict`, `sigma`, or `pi`. The default `restrict` label selects the forward surviving-fragment projection described above. It must not be read as $\Delta_f$.

The categorical vocabulary organizes a more specific fragment. Given $f:S\to T$, restriction is written

$$
\Delta_f:T\text{-Inst}\to S\text{-Inst},
$$

and its left adjoint is written

$$
\Sigma_f:S\text{-Inst}\to T\text{-Inst}.
$$

The explicit constructions live in [`panproto_inst::adjunction`](https://docs.rs/panproto-inst/latest/panproto_inst/adjunction/). For set-valued `FInstance`s, `f_sigma` runs from $S$ to $T$ and `f_delta` runs from $T$ to $S$. The implementation also supplies the unit, counit, and hom-set transposes for total vertex maps, including maps that merge vertices. For `WInstance`, `w_sigma` runs from $S$ to $T$, while `w_delta` runs from $T$ to $S$ and requires vertex- and edge-injective maps whose target anchors lie in the image. Property tests exercise the triangle identities and hom-set transposes in these fragments. They do not prove an adjunction for arbitrary partial migrations.

Migration composition also acts on values. `compose` combines carried coercion expressions in execution order and uses partial-map semantics for structural elements omitted by the second migration. `invert` requires bijective coverage of target vertices, edges, and hyperedges, and it refuses a carried coercion without an inverse expression. It reverses the remaining coercions and swaps the recorded schema endpoints. Hand-built `expr_resolvers` are not inverted and are dropped from the inverse.

Value-level transforms are expressions in the [expression language](./semantics/expression-language.md). They determine how a target value is computed when a structural correspondence alone is insufficient.

## Partial correspondence as a span

A source schema and a target schema need not admit a total morphism. Search thus returns a span

$$
S \xleftarrow{\;\ell\;} A \xrightarrow{\;r\;} T,
$$

where $A$ is the **apex** and the two arrows are its **legs** [@johnsonrosebrugh2014spans]. Johnson and Rosebrugh use *peak* for the same object.

In panproto, $A$ is the sub-schema of $S$ induced by the source vertices that [the search](./morphism-search.md) matched. The left leg is the resulting inclusion, and the right leg records the match into $T$. [`panproto_schema::induce`](https://docs.rs/panproto-schema/latest/panproto_schema/induce/fn.induce.html) restricts the schema's element tables and protocol metadata to this sub-schema, rebuilds its derived indices, and validates the result. Copying only the vertex and edge maps would leave other tables referring to removed elements.

The span is total when the inclusion covers all of $S$. [`SchemaSpan::is_total`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) tests this condition, after which the span can yield a total schema morphism. The empty apex is a feasible match when the schemas share no compatible structure. Search may still report malformed inputs or construction errors, but it does not fail solely because no nonempty match exists.

Classical span equivalence uses an isomorphism between apices that commutes with both legs [@johnsonrosebrugh2014spans]. Because panproto's left leg is an inclusion, the apex is determined by its selected source vertices. A returned span can consequently be represented by that selected sub-schema and its right-leg map, without a separate graph-isomorphism quotient.

The omitted portion of the source resembles a complement in the sense of @bancilhonspyratos1981update: it is information outside the selected view that may be needed to recover the source. This analogy motivates a preference for larger apices at equal search quality. It does not transfer the constant-complement results of that work to panproto's schema spans.

## Pushout and the two apices

A span supplies the input to a pushout:

$$
\begin{CD}
  A  @>{r}>>   T \\
  @V{\ell}VV   @VV{j}V \\
  S  @>>{i}>   S \sqcup_A T.
\end{CD}
$$

The **schema apex** $A$ records shared schema structure. The search returns this schema span. It does not construct pairs of instances that agree on $A$ or establish a model-level pullback theorem; those are separate claims that the current search API does not check.

The implemented schema pushout requires an injective right leg on vertices. A default search result may map two apex vertices to one target vertex, which is a contracting right leg. [`SchemaSpan::pushout`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) rejects that case. Callers that require a merge can request a monic or isomorphic search result. The underlying `schema_pushout` closes the supplied vertex and edge identifications to an equivalence relation and returns two `SchemaMorphism` values. This constructor does not run a separate universal-property checker.

The data-migration adjunction does not by itself construct a symmetric lens for this pushout. @johnsonrosebrughwood2012lenses relate c-lenses to Grothendieck opfibrations and use that structure to formulate universal view updates. panproto checks its scoped adjunction and its lens laws as separate implementation properties; it does not formalize that equivalence.

## The length-1 fragment

In the broader functorial account, a schema translation may send a generating source arrow to a path in the target category. [`SchemaMorphism`](https://docs.rs/panproto-schema/latest/panproto_schema/struct.SchemaMorphism.html) has a narrower representation: its `edge_map` maps each source edge to one target edge. [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) uses the same shape. These maps form the **length-1 fragment** of that account.

A one-to-many value correspondence thus does not appear as a schema morphism. [`FieldTransform::ComputeField`](https://docs.rs/panproto-inst/latest/panproto_inst/wtype/enum.FieldTransform.html) can instead compute a target key with an expression. It may also carry an inverse expression, and its declared [`CoercionClass`](https://docs.rs/panproto-gat/latest/panproto_gat/enum.CoercionClass.html) records the intended recovery behavior. Because callers supply that class, the presence or absence of an inverse does not by itself determine the classification.

Search also considers only a projection of the full `Schema` representation when scoring candidates. Other fields participate in feasibility checks or are restricted by `induce`, but they do not all affect the objective. The weights express a preference over alignments and have not been calibrated against a labeled corpus of correct matches. [Searching for a morphism](./morphism-search.md) gives the exact objective and constraints.

## Compatibility classification

`CompatReport` records a `classification` together with separate `breaking` and `non_breaking` findings. The classifier uses the following conditions:

| Classification | Condition |
|---|---|
| `fully-compatible` | Both finding lists are empty. |
| `backward-compatible` | The breaking list is empty and the non-breaking list is nonempty. |
| `breaking` | The breaking list is nonempty. |

These labels classify structural change. They do not by themselves state that every source record can be transformed, since executable migration may also depend on defaults, value expressions, and protocol-specific requirements. The [Breaking-change gate](../how-to/ci/breaking-change-gate.md) shows how to apply a compatibility policy in CI.

## See also

- [Searching for a morphism](./morphism-search.md) for span construction and its certificate.
- [Find a span between two schemas](../how-to/spans.md) for the corresponding command.
- [Lenses and round-trip laws](./lenses-roundtrip.md) for bidirectional updates and complements.
- [Pushouts and merge](./semantics/pushouts-and-merge.md) for the merge construction.
- [Apply field transforms](../how-to/field-transforms.md) for value-level transforms.
- [What panproto verifies](./what-is-verified.md) for the scope of runtime checks.
