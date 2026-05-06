# Lens DSL: denotational semantics

## In plain terms

A lens DSL spec is a recipe for building a bidirectional transform between two schemas. You declare which fields map to which (with optional value-level expressions), and the compiler produces a triple of functions that satisfies three round-trip laws by construction. This page pins down what the spec compiles to and what "satisfies the laws" means.

## Surface syntax

The Nickel surface (canonical authoring form). JSON and YAML surfaces are isomorphic via `serde`.

```nickel
{
  id = "user.v3-to-v4",
  description = "Rename `name` and replace `age` with `years`",
  steps = [
    { rename_field = { from = "name", to = "display_name" } },
    { remove_field = "age" },
    { add_field = { name = "years", default = 0, expr = "old.age" } },
  ],
}
```

Each step is a single-key object whose key selects the variant. The full step grammar is in [`crates/panproto-lens-dsl/src/document.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/src/document.rs).

## Abstract syntax

```rust
pub struct LensDocument {
    pub id: String,
    pub description: String,
    pub steps: Vec<Step>,
    pub constraints: Vec<Constraint>,
    pub hints: Vec<HintSpec>,
    pub preferences: Vec<PreferencePredicate>,
}

pub enum Step {
    // High-level field combinators
    RemoveField { remove_field: String },
    RenameField { rename_field: RenameSpec },
    AddField    { add_field: AddFieldSpec },

    // Value-level transforms
    ApplyExpr    { apply_expr: ApplyExprSpec },
    ComputeField { compute_field: ComputeFieldSpec },

    // Structural combinators
    HoistField { hoist_field: HoistSpec },
    NestField  { nest_field: NestSpec },
    Scoped     { scoped: ScopedSpec },
    Pullback   { pullback: PullbackSpec },

    // Sort-level coercions and merges
    CoerceSort { coerce_sort: CoerceSortSpec },
    MergeSorts { merge_sorts: MergeSortsSpec },

    // Elementary theory operations
    AddSort      { add_sort: AddSortSpec },
    DropSort     { drop_sort: String },
    RenameSort   { rename_sort: RenameSpec },
    AddOp        { add_op: AddOpSpec },
    DropOp       { drop_op: String },
    RenameOp     { rename_op: RenameSpec },
    AddEquation  { add_equation: EquationSpec },
    DropEquation { drop_equation: String },
}
```

The top-level type is `LensDocument`, not `LensSpec`. There is no `source`/`target` pair on the document: the source schema is supplied at compile time (via the resolver), and the target schema is computed by applying the steps. See `panproto_lens_dsl::compile`.

## Semantic domain

For schemas $S$, $V$ and complement type $C$, the set of *lenses* on $(S, V, C)$ is

$$
\mathsf{Lens}(S, V, C) \;=\; (S \to V) \times (S \times V \times C \to S) \times (S \to C)
$$

with elements written as triples $(\mathsf{get},\, \mathsf{put},\, \mathsf{complement})$. The implementation in `panproto-lens` represents this triple by the `LensHandle` type; the categorical model is the standard asymmetric-lens construction of @foster2007combinators with complements after @littvanhardenberghenry2020cambria. The semantic function

$$
\llbracket \cdot \rrbracket : \mathsf{LensDocument} \to \mathsf{Sch} \to \mathsf{Lens}
$$

takes a document and a source schema and returns a concrete lens. (The target schema and the complement type are determined by the document and the source.)

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

## Semantic equations

For each step constructor, $\llbracket \cdot \rrbracket$ is a function $\mathsf{Step} \to \mathsf{Sch} \to \mathsf{Protolens}$, where a $\mathsf{Protolens}$ is a schema-parameterised lens (see [Protolens composition](./protolens-composition.md)). The document-level semantics is the left-to-right composition of the per-step semantics, applied to the source schema:

$$
\llbracket \mathsf{LensDocument}(\mathsf{id} = d,\, \mathsf{steps} = [s_1, \ldots, s_k]) \rrbracket\, S
  \;=\; \llbracket s_k \rrbracket\,S_{k-1}\ \mathbin{;}\ \cdots\ \mathbin{;}\ \llbracket s_1 \rrbracket\,S_0
$$

where $S_0 = S$ and $S_i = \mathsf{target}(\llbracket s_i \rrbracket\,S_{i-1})$. The composition operator $\mathbin{;}$ is the sequential lens composition

$$
\begin{aligned}
\mathsf{get}_{l_1; l_2}(s)              &= \mathsf{get}_{l_2}(\mathsf{get}_{l_1}(s)) \\
\mathsf{complement}_{l_1; l_2}(s)       &= (\mathsf{complement}_{l_1}(s),\ \mathsf{complement}_{l_2}(\mathsf{get}_{l_1}(s))) \\
\mathsf{put}_{l_1; l_2}(s, v, (c_1, c_2)) &= \mathsf{put}_{l_1}(s,\ \mathsf{put}_{l_2}(\mathsf{get}_{l_1}(s), v, c_2),\ c_1)
\end{aligned}
$$

For a representative subset of steps:

$$
\begin{aligned}
\llbracket \mathsf{RemoveField}\{f\} \rrbracket\, S &=
  \mathsf{drop}_{f}\,\bigl(\mathsf{get}=\mathsf{forget}_f,\ \mathsf{complement}=\mathsf{capture}_f,\ \mathsf{put}=\mathsf{restore}_f\bigr) \\[2pt]
\llbracket \mathsf{RenameField}\{f \mapsto g\} \rrbracket\, S &=
  \bigl(\mathsf{get} = \mathsf{rename}_{f \mapsto g},\ \mathsf{complement} = \varepsilon,\ \mathsf{put} = \mathsf{rename}_{g \mapsto f}\bigr) \\[2pt]
\llbracket \mathsf{AddField}\{f, d\} \rrbracket\, S &=
  \bigl(\mathsf{get} = \mathsf{insert}_{f, d},\ \mathsf{complement} = \varepsilon,\ \mathsf{put} = \mathsf{forget}_f\bigr) \\[2pt]
\llbracket \mathsf{ApplyExpr}\{e_{\to}, e_{\leftarrow}\} \rrbracket\, S &=
  \bigl(\mathsf{get} = \llbracket e_{\to} \rrbracket,\ \mathsf{complement} = \mathsf{snapshot},\ \mathsf{put} = \llbracket e_{\leftarrow} \rrbracket\bigr)
\end{aligned}
$$

where $\varepsilon$ is the trivial complement (a singleton) and $\llbracket \cdot \rrbracket$ on the right-hand side of $\mathsf{ApplyExpr}$ is the [expression-language semantics](./expression-language.md). The remaining 15 step constructors follow the same pattern; the implementation of each lives under [`crates/panproto-lens-dsl/src/steps`](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl/src/steps).

Composition between adjacent step semantics is gated at construction time by `protolens_composable`: see [Protolens composition](./protolens-composition.md). Compilation in the implementation is handled by `panproto_lens_dsl::compile`, which produces a `CompiledLens` carrying the `ProtolensChain` corresponding to $\llbracket \cdot \rrbracket\, S$ along with the value-level `FieldTransform`s extracted from any $\mathsf{ApplyExpr}$ steps.

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
