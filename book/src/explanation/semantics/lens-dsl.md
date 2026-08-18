# Lens DSL: denotational semantics

## In plain terms

A lens DSL specification is a recipe for a bidirectional transform between schemas. It can name field and sort edits, value-level expressions, compositions, or an automatic construction. The compiler produces a `CompiledLens` whose protolens chain can be instantiated at a concrete source schema. Lawfulness is then a property to check, not a consequence of deserialization alone.

[Lenses and round-trip laws](../lenses-roundtrip.md) supplies complements and the three laws used here. [Expression language](./expression-language.md) supplies the value-level computations embedded in a lens specification.

## Surface syntax

Nickel is the canonical authoring form. JSON and YAML represent the same structures through `serde`.

```nickel
{
  id = "user.v3-to-v4",
  description = "Rename `name` and replace `age` with `years`",
  source = "dev.example.user.v3",
  target = "dev.example.user.v4",
  steps = [
    { rename_field = { old = "name", new = "display_name" } },
    { remove_field = "age" },
    { add_field = { name = "years", kind = "integer", default = 0, expr = "old.age" } },
  ],
}
```

Each step is a single-key object whose key selects the variant. The full step grammar is in [`crates/panproto-lens-dsl/src/document.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/src/document.rs).

## Abstract syntax

The listing below is a schematic inventory of document fields and step variants. It is `text` because supporting types, derives, imports, and representation details are omitted.

```text
pub struct LensDocument {
    pub id: String,
    pub description: String,
    pub source: String,
    pub target: String,

    // Body: exactly one of the six variants is present.
    pub steps:     Option<Vec<Step>>,
    pub rules:     Option<Vec<Rule>>,
    pub compose:   Option<ComposeSpec>,
    pub auto:      Option<AutoSpec>,
    pub from_diff: Option<FromDiffSpec>,
    pub symmetric: Option<SymmetricSpec>,

    // Modifier: oriented rewrites appended to the compiled chain.
    pub directed_equations: Option<Vec<DirectedEquationSpec>>,

    // Rule-variant metadata.
    pub passthrough: Option<Passthrough>,
    pub invertible:  Option<bool>,

    // Protocol-specific extension metadata.
    pub extensions: HashMap<String, serde_json::Value>,
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

The top-level type is `LensDocument`, not `LensSpec`. The document carries `source` and `target` NSID fields naming the two schemas and exactly one body variant: `steps`, `rules`, `compose`, `symmetric`, `auto`, or `from_diff`. A `directed_equations` modifier may accompany a body, appending oriented rewrites to the compiled chain.

Compilation has two entry points. `compile` is schema-independent: it handles the schema-parametric bodies (`steps`, `rules`, `compose`, `symmetric`) and rejects `auto` and `from_diff`, which need a concrete source and target schema, with `LensDslError::AutoRequiresSchemas`. `compile_with_schemas` handles those two as well, running the auto-generation engine for `auto` and `diff_to_protolens` for `from_diff`. It also verifies the compiled chain against the declared `target`: the chain is instantiated at the source schema and the NSID of its output schema is compared against `target`, yielding `LensDslError::TargetMismatch` on divergence. The schema-independent path cannot run that comparison, since it has no schema to instantiate against.

## Semantic domain

For schemas $S$, $V$ and complement type $C$, the set of *lenses* on $(S, V, C)$ is

$$
\mathsf{Lens}(S, V, C) \;=\; (S \to V) \times (S \times V \times C \to S) \times (S \to C)
$$

with elements written as triples $(\mathsf{get},\, \mathsf{put},\, \mathsf{complement})$. This triple is a denotational interface. The concrete `panproto_lens::Lens` stores a compiled migration and its source and target schemas; its methods implement the forward, backward, and complement-producing behavior. The categorical model is the standard asymmetric-lens construction of @foster2007combinators with complements after @littvanhardenberghenry2020cambria. The semantic function

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

For PutPut, complement state may change after the first update. The checker therefore uses $c_0=\mathsf{complement}(s)$ and $c_1=\mathsf{complement}(\mathsf{put}(s,v_1,c_0))$:

$$
\textbf{PutPut:} \quad
\mathsf{put}(\mathsf{put}(s,v_1,c_0),v_2,c_1)
=
\mathsf{put}(s,v_2,c_0).
$$

`panproto_lens::laws::check_get_put`, `check_put_get`, and `check_put_put` are deterministic checkers for supplied inputs. `check_put_put` extracts the intermediate complement as shown above. These functions return evidence about a case; they do not quantify over all instances.

## Semantic equations

For each step constructor, $\llbracket \cdot \rrbracket$ is a function $\mathsf{Step} \to \mathsf{Sch} \to \mathsf{Protolens}$, where a $\mathsf{Protolens}$ is a schema-parameterized lens (see [Protolens composition](./protolens-composition.md)). The document-level semantics is the left-to-right composition of the per-step semantics, applied to the source schema:

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

where $\varepsilon$ is the trivial complement (a singleton) and $\llbracket \cdot \rrbracket$ on the right-hand side of $\mathsf{ApplyExpr}$ is the [expression-language semantics](./expression-language.md). These equations are denotational sketches of representative behavior, not literal definitions of Rust function fields. The implementation of the step constructors lives under [`crates/panproto-lens-dsl/src/steps`](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl/src/steps).

Composition between adjacent step semantics is gated at construction time by `protolens_composable`: see [Protolens composition](./protolens-composition.md). Compilation in the implementation is handled by `panproto_lens_dsl::compile`, which produces a `CompiledLens` carrying the `ProtolensChain` corresponding to $\llbracket \cdot \rrbracket\, S$ along with the value-level `FieldTransform`s extracted from any $\mathsf{ApplyExpr}$ steps.

## Complement composition

Sequential composition of lenses requires composing their complements. The `ComplementCompose` extension trait supplies a partial operation with the following checked cases:

- **Identity.** The empty complement $\varepsilon$ satisfies $\varepsilon \cdot c = c \cdot \varepsilon = c$.
- **Commutativity.** When defined, $c_1 \cdot c_2 = c_2 \cdot c_1$.
- **Associativity.** When defined, $(c_1 \cdot c_2) \cdot c_3 = c_1 \cdot (c_2 \cdot c_3)$.
- **Partiality:** $c_1 \cdot c_2$ is defined iff:
  - their source-schema fingerprints agree when both are nonzero (otherwise `ComplementFingerprintMismatch`); and
  - For every key $k$ in both, $c_1(k) = c_2(k)$ (otherwise `ComplementConflict` with the offending key).

The zero fingerprint is the unspecified case. The pre-flight predicate is `ComplementCompose::is_compatible`.

The fingerprint is a 64-bit hash computed by `panproto_lens::asymmetric::schema_fingerprint`. Compatibility means equality of the resulting nonzero fingerprints, not a proof that distinct schema values are isomorphic.

## Soundness

Lawful lens composition preserves the three equations when its premises hold. The implementation supports that conditional claim with property tests in [`crates/panproto-lens/src/laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens/src/laws.rs) and DSL step tests in [`crates/panproto-lens-dsl/tests/step_laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/tests/step_laws.rs). Those suites cover selected generated and constructed cases; they are not a universal proof for every step sequence.

A `coerce_sort` step and directed equations are also checked for honesty against registered samples. A detected violation returns `LensDslError::CoercionNotHonest`. Passing means that the sampled values satisfied the declared round-trip class.

The runtime checkers above remain deterministic smoke checks: they assert each law of a supplied lens on one instance, and they are not the sampling layer.

## What is intentionally not modeled

- **Universal lawfulness of arbitrary documents.** Runtime and property checks cover supplied or generated cases. A document containing lossy or opaque transforms requires the corresponding scoped law claim; compilation alone is not a proof.
- **Time complexity of `put`.** Some combinators have linear `put` cost in the size of the source; the semantics fixes the value, not the cost.
- **Equivalence of two distinct DSL specs that compile to the same lens.** The DSL deliberately exposes step ordering even when steps commute; canonicalisation is left to the user.

## See also

- [Reference: lens combinators](../../reference/lens-combinators.md) for the combinator algebra.
- [How-to: write lenses in the lens DSL](../../how-to/lens-dsl.md).
- [Lenses and round-trip laws (plain-terms version)](../lenses-roundtrip.md).
- [Protolens composition](./protolens-composition.md) for schema-parameterized lenses.
- @foster2007combinators and @littvanhardenberghenry2020cambria.
