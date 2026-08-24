# Migrations as morphisms

Suppose a source schema contains a field named `age` and a target schema contains the corresponding field under the name `years`. A migration records the map from the source field to the target field. If the value also changes representation, the migration may associate that correspondence with an expression that computes the target value.

This representation separates a structural map from its execution. The map can be validated, stored, and composed before it is applied to data. A compiled migration contains the tables and value resolvers needed by the instance layer.

## Maps between concrete schemas

Let $S$ and $T$ be concrete schemas. In the functorial account of data migration, a schema morphism $f:S\to T$ maps source structure to target structure and induces operations between their instance categories [@spivak2012functorial; @spivakwisnesky2015relational]. In panproto's concrete representation, $f$ maps source vertices and edges to target vertices and edges. It must preserve edge endpoints and the other structural conditions enforced for the protocols involved. `Migration::compile` checks the corresponding morphism condition and rejects a map that does not preserve this structure.

Data can then move along the compiled map. The migration API exposes several operations whose historical names use *lift* and *restrict* in different ways, so the concrete entry point matters. `lift_wtype` applies the compiled W-type mapping tables. `lift_wtype_sigma` invokes the W-type dependent-sum construction, while `lift_functor` uses functor restriction. The `schema lift` command selects `restrict` by default and also accepts `sigma` and `pi`. A caller should thus choose an operation from its documented behavior rather than infer its direction from the word *lift* alone.

The categorical vocabulary organizes a more specific fragment. Given $f:S\to T$, restriction is written

$$
\Delta_f:T\text{-Inst}\to S\text{-Inst},
$$

and its left adjoint is written

$$
\Sigma_f:S\text{-Inst}\to T\text{-Inst}.
$$

The explicit constructions live in `panproto-inst::adjunction`. For set-valued `FInstance`s, the implementation constructs the unit, counit, and hom-set transposes for total vertex maps, including maps that merge vertices. Property tests exercise the triangle identities and the hom-set bijection. The corresponding `WInstance` construction has a narrower domain: it requires total vertex-injective maps together with the implemented edge-image conditions. These tests provide evidence for the adjunction in those generated fragments, rather than a proof for arbitrary partial migrations.

Value-level transforms are expressions in the [expression language](./semantics/expression-language.md). They determine how a target value is computed when a structural correspondence alone is insufficient.

## Partial correspondence as a span

A source schema and a target schema need not admit a total morphism. Search thus returns a span

$$
S \xleftarrow{\;\ell\;} A \xrightarrow{\;r\;} T,
$$

where $A$ is the **apex** and the two arrows are its **legs** [@johnsonrosebrugh2014spans]. Johnson and Rosebrugh use *peak* for the same object.

In panproto, $A$ is the sub-schema of $S$ induced by the source vertices that [the search](./morphism-search.md) matched. The left leg is the resulting inclusion, and the right leg records the match into $T$. [`panproto_schema::induce`](https://docs.rs/panproto-schema/latest/panproto_schema/induce/fn.induce.html) restricts the schema's element tables and protocol metadata to this sub-schema, rebuilds its derived indices, and validates the result. Copying only the vertex and edge maps would leave other tables referring to removed elements.

The span is total when the inclusion covers all of $S$. [`SchemaSpan::is_total`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) tests this condition, after which the span can yield a total schema morphism. The empty apex is a feasible match when the schemas share no compatible structure. Search may still report malformed inputs or construction errors, but it does not fail solely because no nonempty match exists.

The ATProto corpus illustrates why partiality matters. Among the 5,852 ordered pairs formed from 77 Lexicons, 735 admit a total morphism and 5,117 do not. In 4,950 of the latter pairs, at least one source vertex lacks a kind-compatible target; naturality rules out the remaining 167. Thus 12.6 percent of the measured pairs admit a total morphism. [Searching for a morphism](./morphism-search.md#what-the-corpus-measures) describes the corpus test and its interpretation.

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

The implemented schema pushout requires an injective right leg. A default search result may map two apex vertices to one target vertex, which is a contracting right leg. [`SchemaSpan::pushout`](https://docs.rs/panproto-mig/latest/panproto_mig/span/struct.SchemaSpan.html) rejects that case. Callers that require a merge can request a monic or isomorphic search result.

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
