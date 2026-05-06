# Denotational semantics

This cluster pins panproto's three DSLs and two structural constructions to a precise mathematical specification. Each page opens with an "In plain terms" section and then proceeds through a six-step skeleton:

1. **Surface syntax.** BNF for what a user types.
2. **Abstract syntax.** The Rust enum the parser produces.
3. **Semantic domain.** The mathematical universe the syntax interprets into.
4. **Interpretation function.** Inference rules of the form $\Gamma \vdash e : \tau \Downarrow v$ that define the meaning of every well-formed expression.
5. **Soundness.** Statement of what the implementation guarantees, and which property tests or runtime checks enforce it.
6. **What is intentionally not modelled.** The boundary of the formal account.

The pages in turn:

| Page | What it pins down |
|---|---|
| [Shared notation](./shared-notation.md) | The judgement forms, environments, and meta-notation used across the others. Read this first. |
| [Expression language](./expression-language.md) | `panproto-expr`: terms, types, total evaluation under a step budget. |
| [Lens DSL](./lens-dsl.md) | `panproto-lens-dsl`: the lens triple `(get, put, complement)`, the three round-trip laws, complement composition as a partial commutative monoid. |
| [Theory DSL](./theory-dsl.md) | `panproto-theory-dsl`: GAT presentations, sort/operation/equation judgements, the colimit interpretation. |
| [Pushouts and merge](./pushouts-and-merge.md) | The pushout construction in the category of GATs, the universal property, and what the implementation verifies. |
| [Protolens composition](./protolens-composition.md) | Protolenses as natural transformations between schema endofunctors, the structural-equality criterion for composition, sequential vs fused instantiation. |

The cluster is meant to be read by anyone who wants to know exactly what panproto guarantees. Familiarity with category theory helps but is not required: every page restates its formal content in plain terms before invoking it.
