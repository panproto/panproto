# Migrations as morphisms

## In plain terms

A migration is what gets you from version 3 of your schema to version 4, with all your existing version-3 data carried forward into version-4 shape. Hand-written migrations are usually a script that walks each record, renames fields, fills in defaults, drops what is no longer there.

panproto represents a migration as a structured map between the two schemas: for every vertex (record kind) in the new schema, where does it come from in the old one; for every edge (field, item, variant), how is it derived. The structured map is called a *morphism*. Once you have a morphism, two operations follow:

- **Restrict** moves the morphism backwards: it tells you what part of the old schema is needed to produce a given part of the new one.
- **Lift** moves data forwards: it takes a record that conforms to the old schema and produces a record that conforms to the new one, using the morphism to know what to put where.

Lift is the operation you usually want; restrict is the operation panproto uses internally to figure out which old fields are required for the migration to succeed. Both are total functions on the things they apply to; if a migration cannot be lifted (because some required input is missing), `panproto-check` says so up front rather than failing partway through.

## The formal picture

Schemas live in a category whose objects are schema theories and whose morphisms are theory morphisms. A *migration* from schema $S$ to schema $T$ is a morphism $f : S \to T$ in this category. The migration engine is split into two functors:

- **Restrict** $\Delta_f : T\text{-Inst} \to S\text{-Inst}$: pulls a $T$-instance back to an $S$-instance along $f$. Used to check existence conditions: which $S$-records does $T$ require to be present?
- **Lift** $\Sigma_f : S\text{-Inst} \to T\text{-Inst}$: pushes an $S$-instance forward to a $T$-instance along $f$. Used to actually migrate data.

The pair forms an adjunction $\Sigma_f \dashv \Delta_f$ in the categories of instances: lift is left adjoint to restrict. (This follows the convention in @spivakwisnesky2015relational, where $\Sigma_f$ is the dependent sum over the fibres of $f$ and $\Delta_f$ is its right adjoint by base change.) The adjunction is the structure that lets panproto check, before any data moves, whether a migration is well-defined.

A migration may also include a value-level transform: not just *where* a field comes from, but *how* its value is computed. These are written in the [expression language](./semantics/expression-language.md) and applied during lift.

## Three classes of migration

| Class | Diff classification | Example | Lift behaviour |
|---|---|---|---|
| Fully compatible | Refinement | Add an optional field with a default. | Total; old records lift unchanged. |
| Backward compatible | Inclusion | Add a required field whose value is computed from existing fields. | Total; old records lift via the value-level transform. |
| Breaking | Neither | Remove a required field with no recovery. | Partial; some old records cannot be lifted. `panproto-check` flags this. |

`panproto-check` runs the existence check on a migration without applying it, so CI can gate on the classification before merge. See [Breaking-change gate](../how-to/ci/breaking-change-gate.md).

## What this gives you

- You write the migration *map* (what goes where), not the migration *script* (how to execute it). The script is generated from the map and the schema theories.
- You get a precise classification of whether the change is breaking, before any data is touched.
- You get a *bidirectional* artifact: the lift forward is paired with a put backward, so the migration is also a [lens](./lenses-roundtrip.md). Round-trip laws apply.
- Migrations compose. If $f : S \to T$ and $g : T \to U$, then $g \circ f : S \to U$ is a migration whose lift agrees with applying $f$'s lift then $g$'s. The migration history of a schema is a chain of these compositions.

## See also

- [Lenses and round-trip laws](./lenses-roundtrip.md) for the bidirectional half.
- [Apply field transforms](../how-to/field-transforms.md) for value-level transforms.
- [What panproto verifies](./what-is-verified.md) for the existence-check guarantee.
- @spivakwisnesky2015relational for the categorical data-migration framework this builds on.
