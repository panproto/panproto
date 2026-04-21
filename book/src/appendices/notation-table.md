# Notation reference

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

A lookup for the non-standard symbols and typographic conventions used in the book. The table lists each symbol, its meaning, and the chapter in which it is introduced.

## Categorical notation

| Symbol | Meaning | Introduced in |
| --- | --- | --- |
| $\mathcal{C}, \mathcal{D}$ | A category | [Categories](../foundations/categories.md) |
| $\mathrm{Ob}(\mathcal{C})$ | The class of objects of $\mathcal{C}$ | [Categories](../foundations/categories.md) |
| $\mathcal{C}(A, B)$, $\mathrm{Hom}_\mathcal{C}(A, B)$ | The hom-set of morphisms $A \to B$ in $\mathcal{C}$ | [Categories](../foundations/categories.md) |
| $f : A \to B$ | A morphism from $A$ to $B$ | [Categories](../foundations/categories.md) |
| $g \circ f$ | Composition of $f : A \to B$ and $g : B \to C$ | [Categories](../foundations/categories.md) |
| $\mathrm{id}_A$ | The identity morphism on $A$ | [Categories](../foundations/categories.md) |
| $A \cong B$ | $A$ and $B$ are isomorphic | [Categories](../foundations/categories.md) |
| $F : \mathcal{C} \to \mathcal{D}$ | A functor | [Functors and natural transformations](../foundations/functors.md) |
| $\alpha : F \Rightarrow G$ | A natural transformation | [Functors and natural transformations](../foundations/functors.md) |
| $\alpha_A$ | The component of a natural transformation at $A$ | [Functors and natural transformations](../foundations/functors.md) |
| $A \times B$, $\pi_1, \pi_2$ | Product of $A$ and $B$ with its projections | [Universal properties](../foundations/universal-properties.md) |
| $A + B$, $\iota_1, \iota_2$ | Coproduct of $A$ and $B$ with its injections | [Universal properties](../foundations/universal-properties.md) |
| $\langle f, g \rangle$ | Pairing into a product | [Universal properties](../foundations/universal-properties.md) |
| $[f, g]$ | Co-pairing out of a coproduct | [Universal properties](../foundations/universal-properties.md) |
| $1$, $0$ | Terminal and initial objects | [Universal properties](../foundations/universal-properties.md) |
| $D : J \to \mathcal{C}$ | A diagram in $\mathcal{C}$ with shape $J$ | [Colimits and pushouts](../foundations/colimits.md) |
| $\Delta X$ | The constant functor at $X$ | [Colimits and pushouts](../foundations/colimits.md) |

## GAT and model notation

| Symbol | Meaning | Introduced in |
| --- | --- | --- |
| $T$, $P$ | A generalised algebraic theory / a panproto protocol | [Algebraic and generalised algebraic theories](../foundations/gats.md) |
| $\mathrm{Th}(T)$ | The syntactic contextual category of $T$ | [Algebraic and generalised algebraic theories](../foundations/gats.md) |
| $\mathrm{Mod}(T)$ | The category of models of $T$ | [Algebraic and generalised algebraic theories](../foundations/gats.md) |
| $M, S$ | A model / a panproto schema | [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) |
| $f : T_1 \to T_2$ | A theory morphism | [Theory morphisms and instance migration](../core/morphisms-and-migration.md) |
| $\Delta_f$, $\Sigma_f$, $\Pi_f$ | The pullback and pushforward functors along $f$ | [Theory morphisms and instance migration](../core/morphisms-and-migration.md) |
| $m_{ij} : S_i \to S_j$ | A migration between panproto schemas | [Theory morphisms and instance migration](../core/morphisms-and-migration.md) |
| $\mathrm{Inst}_P$ | The instance functor for protocol $P$ | [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) |

## Lens notation

| Symbol | Meaning | Introduced in |
| --- | --- | --- |
| $\mathrm{get}$, $\mathrm{put}$ | The two functions of an asymmetric lens | [Bidirectional lenses](../core/lenses.md) |
| GetPut, PutGet, PutPut | The lens round-trip laws | [Bidirectional lenses](../core/lenses.md) |
| $\mathcal{P}$ | A protolens | [Protolenses](../core/protolenses.md) |
| $F(S), G(S)$ | Source and target of a schema-indexed lens | [Protolenses](../core/protolenses.md) |

## Type-theoretic notation

| Symbol | Meaning | Introduced in |
| --- | --- | --- |
| `a -> b`, $a \to b$ | A function type | [Categories](../foundations/categories.md) |
| `.`, $\circ$ | Function composition (Haskell period, mathematical circle) | [Categories](../foundations/categories.md) |
| `id :: a -> a` | Haskell's polymorphic identity function | [Categories](../foundations/categories.md) |
| `{l1 = e1, ...}` | Record literal (panproto-expr) | [Syntax and semantics](../expr/syntax-semantics.md) |
| `[e1 \| x <- e2]` | List comprehension (panproto-expr) | [Syntax and semantics](../expr/syntax-semantics.md) |
| `e ⟶ e'` | Small-step reduction | [Syntax and semantics](../expr/syntax-semantics.md) |

## Typographic conventions

Inline code and type names appear in monospace: `SchemaBuilder`, `panproto-mig`, `compile`. Italicised terms (*category*, *protolens*, *complement*) mark a definition being introduced on first appearance, and bold terms (**Associativity**, **Identity**) are concept labels the reader may want to scan for.

Every first mention of an external tool or library is a hyperlink to its canonical home page: [Haskell](https://www.haskell.org/), [Rust](https://www.rust-lang.org/), [tree-sitter](https://tree-sitter.github.io/tree-sitter/). Every first mention of a panproto module, type, or function is a hyperlink to the corresponding [docs.rs](https://docs.rs/) page. Later mentions in the same chapter appear as plain prose.

Citations follow Pandoc syntax rendered by [`mdbook-bib`](https://github.com/francisco-perez-sorrosal/mdbook-bib) in Chicago author-date style. Inline: `@key` for textual ("Smith 2010"), `[@key]` for parenthetical ("(Smith 2010)"), `[-@key]` for year only. Every cited work has a BibTeX entry in `book/src/references.bib`.
