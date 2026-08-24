# Lens DSL: denotational semantics

A lens DSL document describes a bidirectional transformation between schemas. A document may contain field or sort edits, value-level expressions, a composition, a symmetric pair of pipelines, or a schema-dependent generated body. Compilation produces a `CompiledLens` containing a protolens chain and any value-level field transforms. Deserialization and compilation do not establish the lens laws for every instance.

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

The top-level type is `LensDocument`. Its `source` and `target` fields are schema identifiers, and exactly one of `steps`, `rules`, `compose`, `symmetric`, `auto`, or `from_diff` must be present. A `directed_equations` modifier may accompany that body and appends oriented rewrites to the compiled chain.

Compilation has two entry points. `compile` handles the schema-parametric bodies and rejects `auto` and `from_diff` with `LensDslError::AutoRequiresSchemas`. `compile_with_schemas` also handles those generated bodies. For every nonsymmetric body, it instantiates the chain at the supplied source schema and compares the declared target with the NSID of the output schema's primary entry. A mismatch yields `LensDslError::TargetMismatch`. The check is skipped when the declared target is empty or the output schema has no primary-entry NSID, so successful compilation does not always verify the target identifier.

## Semantic domain

For schemas $S$, $V$ and complement type $C$, the set of *lenses* on $(S, V, C)$ is

$$
\mathsf{Lens}(S, V, C) \;=\; (S \to V) \times (S \times V \times C \to S) \times (S \to C)
$$

with elements written as triples $(\mathsf{get},\mathsf{put},\mathsf{complement})$. The concrete `panproto_lens::Lens` stores source and target schemas, a compiled migration, and field transforms; the functions in the triple describe the behavior of its `get` and `put` operations. This is the asymmetric-lens model of @foster2007combinators, with complements used in the style of @littvanhardenberghenry2020cambria.

For a steps body, compilation is better described as a function into a schema-parameterized chain and a map of field transforms:

$$
\mathsf{compile}_{\mathsf{steps}} : \mathsf{LensDocument}
\to \mathsf{ProtolensChain}\times\mathsf{FieldTransforms}.
$$

Instantiating the chain at a source schema produces a concrete lens. `auto` and `from_diff` require the source schema, target schema, and protocol during compilation as well.

## The three laws

A lens $l=(\mathsf{get},\mathsf{put},\mathsf{complement})$ is *lawful* when the following equations hold for the relevant source values and views.

$$
\textbf{GetPut:} \quad \mathsf{put}(s, \mathsf{get}(s), \mathsf{complement}(s)) = s
$$

$$
\textbf{PutGet:} \quad \mathsf{get}(\mathsf{put}(s, v, c)) = v
$$

For PutPut, complement state may change after the first update. The checker thus uses $c_0=\mathsf{complement}(s)$ and $c_1=\mathsf{complement}(\mathsf{put}(s,v_1,c_0))$:

$$
\textbf{PutPut:} \quad
\mathsf{put}(\mathsf{put}(s,v_1,c_0),v_2,c_1)
=
\mathsf{put}(s,v_2,c_0).
$$

`panproto_lens::laws::check_get_put`, `check_put_get`, and `check_put_put` check supplied instances deterministically. `check_put_put` obtains the intermediate complement $c_1$ as shown above. The checkers compare complete instance structure through the crate's instance-equivalence predicate, but each invocation covers only its supplied case.

## Semantic equations

Schema-level steps compile to protolenses, which are schema-parameterized lenses described in [Protolens composition](./protolens-composition.md). Value-level `apply_expr` and `compute_field` steps instead compile to `FieldTransform` values keyed by the body vertex. An `add_field` step contributes a schema-level protolens and, when it has an expression, a `ComputeField` transform. The steps are processed from left to right.

$$
\llbracket \mathsf{LensDocument}(\mathsf{id} = d,\, \mathsf{steps} = [s_1, \ldots, s_k]) \rrbracket\, S
  \;=\; \llbracket s_k \rrbracket\,S_{k-1}\ \mathbin{;}\ \cdots\ \mathbin{;}\ \llbracket s_1 \rrbracket\,S_0
$$

Here $S_0=S$, and $S_i$ is the target schema after step $s_i$ is applied to $S_{i-1}$. The semicolon denotes sequential lens composition:

$$
\begin{aligned}
\mathsf{get}_{l_1; l_2}(s)              &= \mathsf{get}_{l_2}(\mathsf{get}_{l_1}(s)) \\
\mathsf{complement}_{l_1; l_2}(s)       &= (\mathsf{complement}_{l_1}(s),\ \mathsf{complement}_{l_2}(\mathsf{get}_{l_1}(s))) \\
\mathsf{put}_{l_1; l_2}(s, v, (c_1, c_2)) &= \mathsf{put}_{l_1}(s,\ \mathsf{put}_{l_2}(\mathsf{get}_{l_1}(s), v, c_2),\ c_1)
\end{aligned}
$$

The [step compiler](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl/src/steps) is the authoritative mapping from every `Step` variant to these two outputs. Adjacent schema-level steps must satisfy `protolens_composable`; [Protolens composition](./protolens-composition.md) gives the exact predicate.

## Complement composition

Sequential composition combines complements through the partial operation supplied by `ComplementCompose`. The empty complement $\varepsilon$ is a two-sided identity. Property tests check commutativity and associativity on generated compatible complements. Composition rejects two nonzero, unequal source fingerprints with `ComplementFingerprintMismatch`; it rejects incompatible values stored under the same keyed complement field with `ComplementConflict`. Vector and set-like fields are merged with deduplication. `ComplementCompose::is_compatible` tests whether composition would succeed without allocating the result.

The fingerprint is a 64-bit hash computed by `panproto_lens::asymmetric::schema_fingerprint`. Compatibility means equality of the resulting nonzero fingerprints, not a proof that distinct schema values are isomorphic.

## Checks and limits

Lawful lens composition preserves the three equations when its premises hold. Property tests in [`crates/panproto-lens/src/laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens/src/laws.rs) and constructed DSL cases in [`crates/panproto-lens-dsl/tests/step_laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/tests/step_laws.rs) test selected generated inputs and step constructors. Finite generated samples do not prove lawfulness for every document or instance.

A `coerce_sort` step and directed equations are also checked for honesty against registered samples. A detected violation returns `LensDslError::CoercionNotHonest`. Passing means that the sampled values satisfied the declared round-trip class.

The runtime checkers test one supplied case at a time, while property tests cover generated cases. Documents containing lossy or opaque transforms require scoped law claims; compilation alone does not prove them lawful. The semantics fixes the result of `put`, not its running time, and preserves step order without defining an equivalence on distinct documents that compile to the same lens.

## See also

- [Reference: lens combinators](../../reference/lens-combinators.md) for the combinator algebra.
- [How-to: write lenses in the lens DSL](../../how-to/lens-dsl.md).
- [Lenses and round-trip laws (plain-terms version)](../lenses-roundtrip.md).
- [Protolens composition](./protolens-composition.md) for schema-parameterized lenses.
- @foster2007combinators and @littvanhardenberghenry2020cambria.
