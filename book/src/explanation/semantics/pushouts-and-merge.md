# Pushouts and merge

When two schema branches diverge from a common ancestor, panproto models their combination using a *pushout*. Colimits and pushouts have been used both to assemble structured specifications [@burstallgoguen1977putting] and to model the merge of coinitial patches [@mimramdigiusto2013categorical]. The categorical definition and the concrete checks must be kept separate: the GAT layer constructs an amalgamated union with explicit inclusion maps, while the VCS layer checks a finite set of cocone conditions on a resolved schema merge.

A pushout combines both branches while identifying their common image. Its universal property says that every other compatible cocone receives a unique morphism from the pushout. This property characterizes a pushout up to isomorphism; it does not prescribe a textual merge format or a conflict-resolution policy.

[Theory DSL](./theory-dsl.md) introduces the theory presentations and morphisms used below.

## The categorical definition

Let $\mathsf{Th}$ be the category of GAT presentations and theory morphisms. Write $B$ for the base presentation, $O$ for the first branch, and $T$ for the second. Given morphisms $i_O:B\to O$ and $i_T:B\to T$, a pushout consists of a presentation $M$ and morphisms $j_O:O\to M$ and $j_T:T\to M$.

The cocone equation is

$$
j_O\circ i_O=j_T\circ i_T.
$$

For any other presentation $M'$ and compatible morphisms $k_O:O\to M'$ and $k_T:T\to M'$, the universal property requires a unique mediator $m:M\to M'$ such that $m\circ j_O=k_O$ and $m\circ j_T=k_T$. The corresponding square is

$$
\begin{CD}
  B  @>{i_O}>>  O      \\
  @V{i_T}VV     @VV{j_O}V \\
  T  @>>{j_T}>  M.
\end{CD}
$$

## The GAT construction

The constructor identifies $i_O(s)$ with $i_T(s)$ for every base sort and operation $s$, then merges the remaining sorts, operations, equations, directed equations, and policies. It is an *amalgamated union*, not an implementation of a general coproduct followed by a coequalizer. Same-name elements outside the base image are also identified when their signatures agree. Incompatible definitions produce `SortConflict` or `OpConflict`, and equations that are alpha-equivalent are deduplicated even when their names differ.

A leg that maps distinct base elements to one target element is rejected with `NonInjectiveIdentification`. The implementation does not compute the quotient required for such a non-injective span.

[`colimit`](https://docs.rs/panproto-gat/latest/panproto_gat/fn.colimit.html) returns a `ColimitResult` containing the combined theory and both inclusions. Construction checks the cocone equation. It does not call `check_morphism` on both inclusions, since some registered building-block instance theories refer to sorts supplied only after composition and are not standalone-total.

The raw `colimit_by_name` function returns only a merged theory. It builds no inclusion morphisms and thus performs no cocone or factorization check. `pushout_by_name` builds identity-on-name base legs and delegates to `colimit`.

## The on-demand factorization check

`ColimitResult::verify_universal` takes a caller-supplied alternative cocone $(M',k_O,k_T)$ and constructs a mediator $m:M\to M'$ from the assignments made by its legs. It rejects conflicting assignments and any pushout generator not covered by an inclusion. It then validates the mediator with `check_morphism` and compares both factorization equations.

The function does not enumerate possible mediators. Uniqueness follows from coverage of the pushout generators by the inclusions, which determines every mediator assignment from $k_O$ and $k_T$. Thus the runtime check validates the constructed mediator and its factorization for one supplied cocone; it is not a machine-checked proof over all alternative cocones.

`verify_universal_identity` applies this check to the canonical cocone and also requires the mediator to be the identity. It is suitable only when the combined theory and its inclusions are total.

## Schema merge

At the resolved boundaries that call `verify_pushout`, both branch-to-merge vertex maps must be total, every merged vertex must have a preimage, each base vertex retained by either branch must survive, and the vertex and mapped-base-edge cocone paths must agree. `apply_resolutions`, rebase, cherry-pick, and the ordinary clean merge-commit path call this check. A clean merge requested with `no_commit` or `squash` does not use the merge-commit path. Repository callers wrap failure as `VcsError::PushoutVerification`.

These are necessary cocone conditions, not the complete universal property. The separate `verify_pushout_universal` function checks factorization through a caller-supplied alternative cocone, but only for vertices; the alternative-cocone API has no edge maps. Schema merge does not call it.

The schema merge identifies same-name additions, exactly as the GAT construction identifies same-name sorts and operations outside the base image. Two branches that independently add a vertex $x$ of the same kind, or an edge with the same endpoints, kind, and label, produce one element in the merged schema. A free pushout would produce two, so a cocone that keeps both copies distinct receives no mediator from the merge. The merge computes that free pushout modulo same-name identification, which matches the interpretation of a name-addressed schema. Additions that share a name but disagree about the element are not identified: they are reported as conflicts. `MergeResult::identified_additions` lists every identification the merge made, so a caller who needs the two copies kept apart can rename one side before merging.

The separate [`panproto_schema::schema_pushout`](https://docs.rs/panproto-schema/latest/panproto_schema/fn.schema_pushout.html) function accepts explicit vertex and edge pairs. It closes their endpoint identifications to an equivalence relation, builds a quotient schema, and returns morphisms from both inputs. It rejects overlap pairs that name absent elements. `SchemaSpan::pushout` calls this function only after rejecting a right leg that contracts vertices. Neither constructor invokes the VCS verifier or an on-demand universal-property checker.

## Limits

The pushout construction does not choose a resolution for incompatible edits. Repository histories are not themselves proved to present the base-to-branch span assumed above. The implementation also publishes no asymptotic or wall-clock bound for the complete merge procedure.

## See also

- [Schema version control semantics](../vcs-semantics.md)
- [Composing protocols by colimit](../protocol-colimits.md)
- [What panproto verifies](../what-is-verified.md)
- [Theory DSL](./theory-dsl.md)
