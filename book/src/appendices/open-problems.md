# Open problems

An honest list of questions where panproto's implementation and the book's mathematical account are not yet in full agreement, or where the authors do not know of prior work on a topic panproto addresses. Each entry names the question, the current state, and what would count as progress.

## Where the software is ahead of the theory

### Protolenses as a first-class abstraction

Panproto's protolens construction, developed in [Protolenses](../core/protolenses.md), is a schema-indexed lens family
$$\mathcal{P} : \Pi(S : \mathrm{Schema}).\, \mathrm{Lens}(F(S), G(S))$$
that admits auto-generation, symbolic simplification, and an explicit cost model. The authors are not aware of a published treatment of this exact construction. The closest analogues (profunctor optics, indexed lenses in the sense of Pacheco, Cunha, and Hu) differ in the indexing structure.

Progress would be a writeup placing protolenses in the optics literature, either as a known construction under another name or as a genuine novelty. A contribution from a reader familiar with recent optics work would be welcome.

### The instance functor's symbolic simplifier

[`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/) performs term-rewriting simplification of lens specifications according to an identity base we have extended incrementally as cases arose. The set of rewrite rules currently in the simplifier is finite, terminating, and confluent on the cases the test suite covers. We do not have a proof that it is confluent on all well-formed inputs, and we do not have a characterisation of the lens specifications that reach a normal form consisting of the identity lens.

Progress would be either a proof of confluence on the full specification language or a counterexample that would motivate a reformulation.

### Cross-protocol symmetric lenses at scale

Panproto's symmetric lenses ([Bidirectional lenses](../core/lenses.md)) handle cross-protocol translation for the cases the test suite covers. The underlying mathematical framework is that of Hofmann, Pierce, and Wagner, extended to the schema-dependent setting in the obvious way. We do not know of a published treatment of the extension, and the specific lens compositions panproto performs (a chain of symmetric lenses that collectively translate between three or more protocols) would benefit from a more thorough theoretical development.

## Where the theory is ahead of the software

### The Hofmann-Pierce-Wagner laws on arbitrary symmetric lenses

The symmetric-lens laws of Hofmann, Pierce, and Wagner are known and well-characterised. Panproto verifies them on the cases it handles, through the same property-based approach as for asymmetric lenses. What the panproto implementation does not yet do is symbolically verify the laws on user-constructed symmetric lens specifications, in the way that [`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/) verifies the asymmetric-lens laws on a restricted fragment. The mathematical tools exist; the implementation does not.

### Functorial data migration with equations

Spivak's functorial data migration framework, which [Theory morphisms and instance migration](../core/morphisms-and-migration.md) cites, is presented primarily in the setting of Lawvere theories. Panproto generalises to GATs, which add dependent sorts. The two pushforward functors $\Sigma_f$ and $\Pi_f$ behave as expected on the dependent-sort-free fragment. On the full GAT fragment, the behaviour of $\Pi_f$ in particular becomes more subtle; the current implementation handles the cases we have encountered without a full-generality treatment.

A full generalisation of Spivak's framework to GATs, possibly drawing on Fiore-Plotkin-Turi's algebraic theories with binding, would close the gap. The authors suspect this is a tractable research question and would welcome a contribution.

## Where prior work may exist and the authors do not know it

### Schema-version merge as pushout

Panproto's three-way merge ([Merge as pushout](../vcs/merge-as-pushout.md)) is the pushout of the span of schema morphisms from the most-recent-common-ancestor. The construction is categorically standard, and its application to version control is not original to panproto: the patch-theory tradition of Pijul and Darcs, and the category-theoretic merge work of Mimram, Di Giusto, and others, have treated similar constructions in adjacent settings.

What we do not know is whether the specific combination panproto assembles (pushout in the category of schemas under a protocol, with the pushout's instances lifted through a computed migration) appears in the published literature under a different name. A pointer from a reader familiar with the version-control-as-category tradition would save us the alternative, which is to write up the construction ourselves and invite critique.

### Tree-sitter grammars as GATs

The identification of a tree-sitter grammar's `node-types.json` metadata with a GAT is, as far as we know, specific to panproto. The mathematical content is modest (the identification is almost mechanical), but the consequence (auto-derivation of protocols for 248 languages) is practically significant. We would welcome a comparison with any prior work on grammar-as-theory that treats the concrete-syntax-tree case rather than the abstract-syntax-tree case.

## Operational open problems

These are not theoretical gaps but operational limitations we hope to close with engineering work.

### Stored-config remote management

Panproto-vcs currently supports programmatic remote operations (push, fetch, clone through the [git bridge](../vcs/git-bridge.md)) but does not yet store remote configuration in the repository itself the way git stores `.git/config`. A user who clones a panproto-vcs repository through the git bridge and wants to push back to a related remote must re-specify the remote URL on every push. The fix is mechanical but not yet implemented.

### Streaming incremental merge

[`panproto_vcs::merge`](https://docs.rs/panproto-vcs/latest/panproto_vcs/merge/) operates in batch mode: the merge algorithm reads both branches' schemas in full before producing the pushout. For repositories with very large schemas (hundreds of thousands of sorts), this can exhaust memory. A streaming merge that produces pushout components incrementally is possible in principle and would resolve the memory issue.

### Interactive conflict resolution UI

The colimit-based merge algorithm detects conflicts as failures of the pushout's universal property. It reports them as structured diagnostics. It does not yet include an interactive resolution UI that would let a developer accept, reject, or modify each conflict individually. The diagnostics contain the information a UI would need; the UI itself is not written.

### Full LLVM integration

The experimental [`panproto-llvm`](https://docs.rs/panproto-llvm/latest/panproto_llvm/) crate has the acceptance criteria for promotion named in [Experimental and feature-gated subsystems](../contributing/experimental.md). The work is in scaling the theory's existence-checker to the full LLVM IR specification rather than the well-typed subset.

## How to contribute

An open problem listed above is an invitation to contribute, and panproto accepts contributions on any of them. The [Extending panproto chapter](../contributing/extending.md) describes the mechanics; the `panproto-contributing` skill gives the development workflow. For research-flavoured contributions (a proof, a counterexample, a pointer to prior work), the main issue tracker is the right venue.
