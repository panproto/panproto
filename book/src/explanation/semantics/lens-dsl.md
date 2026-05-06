# Lens DSL: denotational semantics

## In plain terms

A lens DSL spec is a recipe for building a bidirectional transform between two schemas. You declare which fields map to which (with optional value-level expressions), and the compiler produces a triple of functions that satisfies three round-trip laws by construction. This page pins down what the spec compiles to and what "satisfies the laws" means.

## Surface syntax

The Nickel surface (canonical form). JSON and YAML surfaces are isomorphic.

```nickel
{
  source = "schema/v3.json",
  target = "schema/v4.json",
  steps = [
    { kind = "rename_edge", from = "first", to = "given" },
    { kind = "split_field", source = "name", target = ["first", "last"], by = " " },
    { kind = "field_transform", at = "age", forward = "x => x + 1", backward = "y => y - 1" },
  ],
}
```

The full step grammar is in [`crates/panproto-lens-dsl`](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl).

## Abstract syntax

```rust
pub enum Step {
    RenameEdge { from: EdgeName, to: EdgeName },
    SplitField { source: EdgeName, target: Vec<EdgeName>, by: SplitRule },
    JoinFields { sources: Vec<EdgeName>, target: EdgeName, sep: String },
    FieldTransform { at: Path, forward: Expr, backward: Expr },
    AddField { at: Path, default: Expr },
    DropField { at: Path },
    /* ... full list in panproto-lens-dsl */
}

pub struct LensSpec {
    pub source: SchemaRef,
    pub target: SchemaRef,
    pub steps: Vec<Step>,
}
```

## Semantic domain

A *lens* between source $S$, view $V$, and complement $C$ is a triple of functions

$$
\llbracket l \rrbracket = (\mathsf{get},\; \mathsf{put},\; \mathsf{complement})
$$

with

$$
\mathsf{get} : S \to V, \quad
\mathsf{put} : S \times V \times C \to S, \quad
\mathsf{complement} : S \to C.
$$

The domain of all lenses on $(S, V, C)$ is denoted $\mathsf{Lens}(S, V, C)$.

## The three laws

A lens $l = (\mathsf{get}, \mathsf{put}, \mathsf{complement})$ is *lawful* iff for all $s \in S$ and $v \in V$:

$$
\textbf{GetPut:} \quad \mathsf{put}(s, \mathsf{get}(s), \mathsf{complement}(s)) = s
$$

$$
\textbf{PutGet:} \quad \mathsf{get}(\mathsf{put}(s, v, c)) = v
$$

$$
\textbf{PutPut:} \quad \mathsf{put}(\mathsf{put}(s, v_1, c), v_2, c) = \mathsf{put}(s, v_2, c)
$$

`panproto_lens::laws::check_get_put`, `check_put_get`, and `check_put_put` are property-test runners that sample $s$, $v$, $v_1$, $v_2$ from the schema's value space and assert each equation.

## Compilation

A `LensSpec` compiles to a chain of lens combinators by translating each `Step` to a lens that targets the affected sub-schema and lifting it into the full schema via the lens fibration:

$$
\llbracket \mathsf{LensSpec}(\mathsf{source}=S, \mathsf{target}=T, \mathsf{steps}=[s_1, \ldots, s_n]) \rrbracket = \llbracket s_n \rrbracket \mathbin{;} \cdots \mathbin{;} \llbracket s_1 \rrbracket
$$

where $\mathbin{;}$ is sequential lens composition and $\llbracket s_i \rrbracket$ is the combinator chosen for step $s_i$. The sequential-composition rule for lenses is

$$
(\mathsf{get}_1; \mathsf{get}_2)(s) = \mathsf{get}_2(\mathsf{get}_1(s))
$$

with the corresponding $\mathsf{put}$ and $\mathsf{complement}$ assembled by the [combinator algebra](../../reference/lens-combinators.md).

## Complement composition

Sequential composition of lenses requires composing their complements. `Complement::compose` is a *partial commutative monoid*:

- **Identity.** The empty complement $\varepsilon$ satisfies $\varepsilon \cdot c = c \cdot \varepsilon = c$.
- **Commutativity.** When defined, $c_1 \cdot c_2 = c_2 \cdot c_1$.
- **Associativity.** When defined, $(c_1 \cdot c_2) \cdot c_3 = c_1 \cdot (c_2 \cdot c_3)$.
- **Partiality:** $c_1 \cdot c_2$ is defined iff:
  - $c_1$ and $c_2$ have the same source-schema fingerprint (otherwise `ComplementFingerprintMismatch`); and
  - For every key $k$ in both, $c_1(k) = c_2(k)$ (otherwise `ComplementConflict` with the offending key).

Pre-flight predicate: `Complement::is_compatible(c1, c2)`.

The fingerprint is a blake3 hash of the source schema's normal form, so complements computed against syntactically distinct but structurally equal schemas are still compatible.

## Soundness

The compilation function preserves lawfulness: if every step compiles to a lawful lens (which the combinator algebra guarantees), the composed result is lawful. Property tests in [`crates/panproto-lens/src/laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens/src/laws.rs) verify each combinator against random inputs sampled from the schema's value space.

## What is intentionally not modelled

- **Lossy migrations as full lenses.** A migration that drops information cannot satisfy GetPut. The DSL allows `DropField`, but the resulting object is a *partial* lens; the laws hold only on the surviving structure, and CI tests skip the GetPut law for steps annotated as lossy.
- **Time complexity of `put`.** Some combinators have linear `put` cost in the size of the source; the semantics fixes the value, not the cost.
- **Equivalence of two distinct DSL specs that compile to the same lens.** The DSL deliberately exposes step ordering even when steps commute; canonicalisation is left to the user.

## See also

- [Reference: lens combinators](../../reference/lens-combinators.md) for the combinator algebra.
- [How-to: write lenses in the lens DSL](../../how-to/lens-dsl.md).
- [Lenses and round-trip laws (plain-terms version)](../lenses-roundtrip.md).
- [Protolens composition](./protolens-composition.md) for schema-parameterised lenses.
- @foster2007combinators and @littvanhardenberghenry2020cambria.
