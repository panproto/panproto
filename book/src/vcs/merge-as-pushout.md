# Merge as pushout

A three-way merge in panproto-vcs is a pushout in the category of schemas. This chapter explains what the identification means concretely, what it buys over git's line-based merge, and what happens when the pushout fails to exist.

The mathematical background is [Colimits and pushouts](../foundations/colimits.md); the object-level setting is [Objects, refs, and the DAG](./objects-and-dag.md); the Rust code is in [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/). @mimramdigiusto2013categorical give a nearby categorical treatment for textual patches, and @angiuli2014homotopical develop a homotopy-type-theoretic variant; the present chapter follows the same overall pattern applied to schemas under a fixed protocol.

## The three-way span

Two branches of a panproto-vcs repository that diverged from a common ancestor present a span of three schemas: the ancestor schema $S_A$, the left branch's schema $S_L$, and the right branch's schema $S_R$, together with schema morphisms
$$S_L \xleftarrow{f} S_A \xrightarrow{g} S_R$$
where $f$ and $g$ are the migrations from the ancestor schema into each branch. The span is constructed by [`panproto_vcs::merge::find_span`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/), which uses [`panproto_vcs::dag`](https://docs.rs/panproto-vcs/latest/panproto_vcs/dag/) to compute the most-recent-common-ancestor commit and extracts the schemas from that commit and from each branch tip.

## The pushout

The pushout of this span in the category of schemas is the schema $S$ together with morphisms $\iota_L : S_L \to S$ and $\iota_R : S_R \to S$ satisfying $\iota_L \circ f = \iota_R \circ g$, universal among such. By the general construction of [Colimits and pushouts](../foundations/colimits.md), $S$ is obtained from the disjoint union of the sorts and operations of $S_L$ and $S_R$, with the sorts and operations in the image of $S_A$ identified along $f$ and $g$. Equations from either branch are inherited; any new equation the merger wants to add applies to the pushout and is checked there.

Concretely, when the left branch adds a field $\mathsf{email}$ to a record type and the right branch adds a field $\mathsf{phone}$ to the same record type, the pushout is a schema in which the record type has both fields. The ancestor's definition of the record type is the shared piece; the two branches' additions are independent.

The construction is implemented in [`panproto_schema::colimit`](https://docs.rs/panproto-schema/latest/panproto_schema/colimit/), called from [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/). The result is a schema value written to the object database as any other schema; the merge commit points at a tree whose root schema is this pushout.

## Data migration at merge time

The pushout at the schema level is only half the work. The branches carry data under their respective schemas, and that data must be carried into the merged schema. Two lift operations run as part of the merge: one lifting left-branch data along $\iota_L$, one lifting right-branch data along $\iota_R$. Both are ordinary migrations through [the restrict/lift pipeline](../core/restrict-lift.md); the merge commit's tree contains the lifted instances, not the originals.

Where the two branches modified the same data (a record's value changed in both branches), the merge reports a data-level conflict. The conflict-free replicated data types of @shapiro2011crdt are a separate line of work on automatic conflict resolution that panproto does not adopt wholesale; the pushout-plus-manual-resolution approach taken here trades automatic convergence for finer-grained user control over every disagreement. The conflict is reported as a triple (record, left-branch value, right-branch value) with the path in the schema where the divergence occurred, and the user is asked to resolve it. Conflict resolution at the data level is independent of the schema-level merge: the schema-level pushout may succeed even when data-level values conflict, since the schema structure is compatible but specific records disagree.

## When the pushout fails

A pushout may fail to exist. The two branches may modify the same sort or operation in incompatible ways: one branch renames a field, the other adds an equation that references the field under its old name; one branch removes a sort, the other adds a constraint that the sort must satisfy. In such cases the pushout's universal property cannot be satisfied with a single well-formed schema, and [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/) reports a conflict at the schema level.

The diagnostic is specific. It names the sort or operation at which the pushout fails, the equation in each branch that contradicts the other, and the minimal subset of both branches' changes that would have to be reconciled for the pushout to exist. A developer sees this as the merge tool asking "the field `email` in the left branch is being renamed to `contactEmail`, and the right branch has added a constraint that depends on the name `email`; which do you want?" with enough structure that a resolution commit (choosing one of the two branches' versions or a third alternative) can be applied and the merge retried.

## What this buys over git's merge

Git's merge algorithm operates on lines of text. When two branches both edited the schema definition file, git reports a line-level conflict with no understanding of what schema change each line represents. The developer resolving the conflict sees `<<<<<<<` markers around a pair of line ranges and is asked to choose between them; what the developer is actually choosing between, most of the time, is two specific schema edits, but git cannot tell the developer that.

Panproto-vcs's merge operates on schemas. When the same scenario arises, the developer sees a report naming the two schema edits explicitly (left branch renames field $X$ to $Y$; right branch adds equation $E$ referencing $X$) and can reason at the level of the edits rather than the level of the lines. The resolution is a commit that adjusts one of the two schema edits and asks the merge to retry; the result is a working merged schema, not a text file with conflict markers that will need a separate schema-validation pass.

The generalisation is that schema merges are closer to the actual operation the developer is performing than line merges are. Line merges are the limit of schema merges when the schema is "arbitrary bytes"; in every case where the content has more structure, operating at the structure's level produces a more actionable merge diagnostic.

## Further reading

@mimramdigiusto2013categorical is the nearest categorical treatment of merge for textual patches; its framework and the one in this chapter differ only in which category the pushout is computed in. @angiuli2014homotopical gives a homotopy-type-theoretic variant and is worth reading for a different perspective on what "merge" can mean. For the CRDT alternative to pushout-based merging, @shapiro2011crdt is the foundational paper; panproto does not adopt CRDTs wholesale but the trade-off with automatic-convergence approaches is worth understanding.

For the general category-theoretic background on pushouts, the references from [Colimits and pushouts](../foundations/colimits.md) apply. For the specific form of schema merge used here, the implementation in [`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/) is its own reference.

## Closing

The next chapter, [Data versioning](./data-versioning.md), works through the counterpart on the data side: how panproto-vcs automatically infers a migration between two schema versions without an explicit migration declaration, and how the inferred migration interacts with the lift operations described above.
