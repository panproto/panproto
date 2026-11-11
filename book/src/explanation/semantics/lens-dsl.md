# Lens DSL: denotational semantics

A lens DSL document describes a bidirectional transformation between schemas. A document may contain field or sort edits, value-level expressions, a composition, a symmetric pair of pipelines, or a schema-dependent generated body. Compilation produces a `CompiledLens` whose ordered stages place structural chains and value-level field transforms in execution order. Its flat `chain` and `field_transforms` fields are compatibility summaries. Deserialization and compilation do not establish the lens laws for every instance.

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

Compilation has two entry points. `compile` handles the schema-parametric bodies and rejects `auto` and `from_diff` with `LensDslError::AutoRequiresSchemas`. `compile_with_schemas` also handles those generated bodies. For every nonsymmetric body, it instantiates the ordered stages as a path from the supplied source schema and compares the declared target with the NSID of the path's output schema primary entry. A mismatch yields `LensDslError::TargetMismatch`. The check is skipped when the declared target is empty or the output schema has no primary-entry NSID, so successful compilation does not always verify the target identifier.

## Semantic domain

For source instances $S$, view instances $V$, and complement values $C$, panproto's operational lens interface has the shape

$$
\mathsf{get}:S\to V\times C,
\qquad
\mathsf{put}:V\times C\to S.
$$

The complement is returned by `get`; callers do not pass the original source separately to `put`. The concrete `panproto_lens::Lens` stores source and target schemas and a compiled migration containing the field transforms. This is an asymmetric-lens interface with explicit saved state, related to the models of @foster2007combinators and @littvanhardenberghenry2020cambria.

For a steps body, compilation returns an ordered sequence whose stages pair a schema-parameterized chain with the field transforms that execute at that point:

$$
\mathsf{compile}_{\mathsf{steps}} : \mathsf{LensDocument}
\to (\mathsf{ProtolensChain}\times\mathsf{FieldTransforms})^*.
$$

The flat chain and transform map are projections of this sequence and do not determine the order when a value transform lies between structural stages. Instantiating the compiled document at a source schema and protocol instantiates each stage at the schema produced by its predecessor, then composes the resulting concrete lenses into an ordered `LensPath`:

$$
\mathsf{instantiate} : \mathsf{CompiledLens}\times\mathsf{Schema}\times\mathsf{Protocol}
\to \mathsf{LensPath}.
$$

Converting that path to one concrete lens is a partial operation: `into_single` refuses when normalization retains more than one group. `auto` and `from_diff` require the source schema, target schema, and protocol during compilation as well.

## The three laws

A lens is *lawful* when the following equations hold for the relevant source values and views. Write $\mathsf{get}(s)=(v,c)$.

$$
\textbf{GetPut:} \quad \mathsf{put}(v,c)=s
$$

For an edited view $v'$, write $s'=\mathsf{put}(v',c)$ and $\mathsf{get}(s')=(v'',c')$. Then

$$
\textbf{PutGet:} \quad v''=v'.
$$

The current PutPut checker uses the original view for its first put. If $\mathsf{get}(s)=(v_0,c_0)$, $s_1=\mathsf{put}(v_0,c_0)$, and $\mathsf{get}(s_1)=(-,c_1)$, it checks a supplied second view $v_2$ by comparing

$$
\textbf{PutPut:} \quad
\mathsf{put}(v_2,c_1)=\mathsf{put}(v_2,c_0).
$$

`panproto_lens::laws::check_get_put`, `check_put_get`, and `check_put_put` check supplied instances deterministically. `check_put_put` obtains the intermediate complement $c_1$ as shown above. The checkers compare complete instance structure through the crate's instance-equivalence predicate, but each invocation covers only its supplied case.

## Semantic equations

Schema-level steps compile to protolenses, which are schema-parameterized lenses described in [Protolens composition](./protolens-composition.md). Value-level `apply_expr` and `compute_field` steps instead compile to `FieldTransform` values keyed by the body vertex. An `add_field` step contributes a schema-level protolens and, when it has an expression, a `ComputeField` transform. The steps are processed from left to right.

$$
\llbracket \mathsf{LensDocument}(\mathsf{id} = d,\, \mathsf{steps} = [s_1, \ldots, s_k]) \rrbracket\, S
  \;=\; \llbracket s_1 \rrbracket\,S_0\ \mathbin{;}\ \cdots\ \mathbin{;}\ \llbracket s_k \rrbracket\,S_{k-1}
$$

Here $S_0=S$, and $S_i$ is the target schema after step $s_i$ is applied to $S_{i-1}$. The semicolon denotes sequential lens composition:

$$
\begin{aligned}
\mathsf{get}_{l_1}(s) &= (v,c_1), \\
\mathsf{get}_{l_2}(v) &= (w,c_2), \\
\mathsf{get}_{l_1;l_2}(s) &= (w,(c_1,c_2)), \\
\mathsf{put}_{l_1;l_2}(w,(c_1,c_2))
  &= \mathsf{put}_{l_1}(\mathsf{put}_{l_2}(w,c_2),c_1).
\end{aligned}
$$

The [step compiler](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl/src/steps) is the authoritative mapping from every `Step` variant to these two outputs. Adjacent schema-level steps must satisfy `protolens_composable`; [Protolens composition](./protolens-composition.md) gives the exact predicate.

## Path complements and independent merging

Path execution does not merge the complements produced by sequential stages. `LensPath::get` first normalizes adjacent compatible stages into shape-specific groups, then retains one `PathComplementEntry` per group in execution order. Each entry records the group's complement, identity envelope, and stage digests. `LensPath::put` consumes those entries in reverse order and rejects a path complement with the wrong entry count, stage identity, or complement identity.

`ComplementCompose` supplies a separate partial-commutative merge for independently obtained complements of the same shape. The empty complement $\varepsilon$ is a two-sided identity. Property tests check commutativity and associativity on generated compatible complements. Composition rejects incompatible values stored under the same keyed complement field with `LensError::ComplementConflict`. `LensError::Identity` covers incompatible identity envelopes, mixed stamped and legacy identities, unequal legacy fingerprints, and unbound nonempty complements. Its causes include `IdentityError::Mismatch`, `IdentityError::LegacyComplementRejected`, and `IdentityError::UnboundComplement`. Vector and set-like fields are merged with deduplication. `ComplementCompose::is_compatible` tests whether composition would succeed without allocating the result.

A legacy fingerprint is a 64-bit hash of the source schema retained for compatibility with complements written before identity envelopes. Fingerprint equality is not a proof that distinct schema values are isomorphic.

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
