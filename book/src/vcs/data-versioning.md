# Data versioning

When a schema changes in a panproto-vcs commit, the data stored under the old schema must be lifted to the new one. Often the lift is automatic: the migration from the old schema to the new is uniquely determined, or almost uniquely determined, by the diff between them. Panproto-vcs's auto-migration infrastructure computes this migration without requiring the user to declare it explicitly. This chapter walks through the inference, the cases it handles, and the cases it refuses.

The Rust code is split between [`panproto_vcs::auto_mig`](https://docs.rs/panproto-vcs/latest/panproto_vcs/auto_mig/) (the inference) and [`panproto_vcs::data_mig`](https://docs.rs/panproto-vcs/latest/panproto_vcs/data_mig/) (the application of the inferred migration to stored data). The mathematical background is in [Theory morphisms and instance migration](../core/morphisms-and-migration.md) and [The restrict/lift pipeline](../core/restrict-lift.md).

## When auto-migration is invoked

A schema change enters panproto-vcs through a commit that stages a new schema object at a path where an old one already exists. The old schema is retrieved from the previous commit's tree; the new schema is the one being staged. If the commit does not include an explicit migration declaration, [`panproto_vcs::auto_mig::infer`](https://docs.rs/panproto-vcs/latest/panproto_vcs/auto_mig/fn.infer.html) runs against the pair (old, new) and attempts to produce a migration that carries every record under the old schema to a well-typed record under the new one.

Auto-migration is optional. A user whose migration is non-trivial (a new field whose value requires computation, a renamed field whose values must be rearranged, a dropped field whose data must be preserved elsewhere) supplies the migration declaration explicitly through the migration DSL; auto-migration is skipped when such a declaration is present.

## The diff

The inference starts from the schema diff. Given the old schema $S_1$ and the new schema $S_2$, the diff is a structural description of their difference: which sorts are added, which sorts are removed, which operations are added or removed, which equations are added or removed, and which renames have been applied at any of the above levels. The diff is computed by [`panproto_schema`](https://docs.rs/panproto-schema/latest/panproto_schema/)'s differ, which reuses much of the machinery behind [`panproto-check`](https://docs.rs/panproto-check/latest/panproto_check/) for breaking-change classification.

From the diff, the inference constructs a candidate theory morphism $f : T_1 \to T_2$. Every sort and operation of $T_1$ maps to its correspondent in $T_2$, with renames where recorded and identity mappings elsewhere. The morphism passes existence checking ([`panproto_mig::existence`](https://docs.rs/panproto-mig/latest/panproto_mig/existence/)) whenever the diff contains no contradictions at the theory level.

## The pushforward choice

A theory morphism alone is not a migration; a migration fixes a pushforward choice at every extension site. Auto-migration makes defaults that cover the common cases.

For a field added with a declared default value, the pushforward is a constant lift that supplies the default for every instance. For a field added with no default, auto-migration refuses to proceed; the user must declare the migration explicitly with a non-trivial pushforward. For a field dropped, the pushforward is a $\Delta_f$-pullback that forgets the field. For a field renamed, the pushforward is the identity lift applied to the field's values under the new name.

For a sort added, the pushforward is empty on existing data: no records of the new sort appear in the lifted instance, which is consistent with there being no records of the new sort in the source instance. For a sort removed, auto-migration refuses unless the sort is unreachable from the kept sorts in the source schema; a sort referenced by existing records cannot be dropped without explicit user action.

For an equation added, no data-level action is required, but existence checking runs against the source instance to detect any records that would violate the new equation. Records that violate the new equation cause auto-migration to refuse; the user must explicitly declare how to handle the violating records (by deleting them, by rewriting them to conform, by rejecting the migration entirely).

## Applying the inferred migration

When inference succeeds, the result is a compiled migration in the sense of [The restrict/lift pipeline](../core/restrict-lift.md). [`panproto_vcs::data_mig::apply`](https://docs.rs/panproto-vcs/latest/panproto_vcs/data_mig/fn.apply.html) applies this migration to every instance the commit's tree points at, lifting each to its new-schema form. The lifted instances are written to the object database; the commit's tree is rewritten to point at the lifted instances instead of the originals.

The original instances are not deleted. They remain in the object database, referenced by previous commits, and a checkout of those earlier commits retrieves the data in its original form. This is the same preservation git provides for the byte contents of a file across history: old content is not overwritten, only shadowed by newer content in newer commits.

## When auto-migration refuses

Auto-migration refuses in specific, reported cases. A field added without a default value produces the diagnostic "schema adds field `X` without a default; auto-migration cannot populate this field." A sort removed while data of that sort exists produces "schema removes sort `S`; auto-migration cannot discard the records of sort `S` in the source instance without explicit user declaration." An equation added while the source contains violating records produces "schema adds equation `E`; auto-migration found records that violate it (the first three are listed)."

In each case, the commit is held until the user responds. The response is either a migration declaration that covers the refused case, an edit to the staged schema that avoids the refusal (for example, adding a default value to the new field), or an edit to the source data that removes the obstruction (deleting the violating records manually). Panproto-vcs does not apply a partial migration; either the whole commit lifts cleanly or the commit does not land.

## Further reading

For the auto-migration machinery in the abstract, the references from [Theory morphisms and instance migration](../core/morphisms-and-migration.md) apply: @spivak2012functorial for the functorial-data-migration framework, @wisnesky2013functional for the implementation tradition. The diff-based approach panproto-vcs uses is complementary to the CRDT-based approach of @shapiro2011crdt; the trade-off is that panproto surfaces conflicts to the user rather than resolving them automatically, at the cost of requiring user attention at merge time.

For the Cambria perspective specifically, @littvanhardenberghenry2020cambria is the foundational source; panproto adopts Cambria's complement-tracking approach in its lens machinery (see [Bidirectional lenses](../core/lenses.md)) but applies it differently in the data-versioning setting.

## Closing

The next chapter, [The git bridge](./git-bridge.md), covers the interop layer between panproto-vcs and git: how a panproto-vcs repository can be pushed to and pulled from an ordinary git remote, what is preserved on the round trip, and when the two representations diverge.
