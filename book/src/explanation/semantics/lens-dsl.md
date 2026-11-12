# Lens DSL: denotational semantics

A lens DSL document describes a bidirectional transformation between schemas. A document may contain field or sort edits, value-level expressions, a composition, a symmetric pair of pipelines, or a schema-dependent generated body. Compilation produces a `CompiledLens` whose ordered stages place structural chains and value-level field transforms in execution order. Its flat `chain` and `field_transforms` fields are compatibility summaries. Deserialization and compilation do not establish the lens laws for every instance.

[Lenses and round-trip laws](../lenses-roundtrip.md) supplies complements and the three laws used here. [Expression language](./expression-language.md) supplies the value-level computations embedded in a lens specification.

## Surface syntax

Nickel is the canonical authoring form. JSON and YAML represent the same structures. Since `default` is a Nickel keyword, a Nickel document writes the `add_field` and `add_sort` member as the quoted field name `"default"`.

```nickel
{
  id = "user.v3-to-v4",
  description = "Rename `name` and replace `age` with `years`",
  source = "dev.example.user.v3",
  target = "dev.example.user.v4",
  steps = [
    { rename_field = { old = "name", new = "display_name" } },
    { remove_field = "age" },
    { add_field = { name = "years", kind = "integer", "default" = 0, expr = "old.age" } },
  ],
}
```

Each step is a single-key object whose key selects the variant. The full step grammar is the table in [`crates/panproto-lens-dsl/src/shape.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/src/shape.rs); the serde types of [`document.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/src/document.rs) read what that grammar admits.

## Abstract syntax

The listing below is a schematic inventory of document fields and step variants. It is `text` because supporting types, derives, imports, and representation details are omitted.

```text
pub struct LensDocument {
    pub id: String,
    pub description: String,          // optional, empty when absent
    pub source: String,
    pub target: String,

    // Body: exactly one of the six variants is present.
    pub steps:     Option<Vec<Step>>,
    pub rules:     Option<Vec<Rule>>,
    pub compose:   Option<ComposeSpec>,
    pub auto:      Option<AutoSpec>,
    pub from_diff: Option<FromDiffSpec>,
    pub symmetric: Option<SymmetricSpec>,

    // Modifier: oriented rewrites appended to the compiled chain
    // (every body except `symmetric`).
    pub directed_equations: Option<Vec<DirectedEquationSpec>>,

    // `passthrough` belongs to the `rules` body; `invertible` to every body.
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

## Document grammar

Before a document value is deserialized, [`read_document`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/shape/fn.read_document.html) checks it against a closed grammar. A document is an object carrying `id`, `source` and `target` strings, an optional `description` string, exactly one of the body keys `steps`, `rules`, `compose`, `auto`, `from_diff` and `symmetric`, and the members of that body's field table: `passthrough` belongs to the `rules` table only, `directed_equations` to every table except `symmetric`, and `invertible` and `extensions` to every table. A step, and an entry of a `compose` body's `lenses`, is an object with exactly one member whose name selects the variant (`ref` or `inline` for a lens reference). Every expression is a string; a value kind in `coerce_sort` or a directed equation is one of `boolean`, `bool`, `integer`, `int`, `float`, `number`, `string`, `str`, `bytes`, `token`, `null` and `any`; a `map_attr_value` entry is an object whose `op` selects its operator and the type of its `value`; and a rule's `pattern` is an object whose only member is the matched `name`. No member has a second spelling: the grammar admits `default` and not `fallback`, `pattern` and not `match`, `impl` and not `impl_term`.

The check refuses the first value that breaks the grammar, in an order fixed by the grammar and not by the source. A root that is not an object is refused first, then a missing or second body key (at the root), then the header and the body table field by field. An object is checked for an unknown member first (the least such name, comparing names byte by byte), then field by field in table order, a missing required field being refused at the object. A single-member variant object is refused at the least member name that names no variant, then for naming no variant or several (listed in grammar order). A map's members are checked in the order of their names and an array's elements in order. When an object names a member twice, the last occurrence is the member read. Each refusal is a `LensDslError::Shape` carrying the path of the refused value and a `ShapeProblem`; the checker is the one [`panproto-dsl-eval`](https://docs.rs/panproto-dsl-eval) supplies to both DSLs.

## Compilation

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

## Bodies, rules and composition

A step list compiles step by step: each step yields one stage (a `scoped` step yields one per stage of each inner step, its chain fused and mapped over the focus with `map_items`), a step whose chain and transforms are both empty yields none, and the list's stages are the concatenation of its steps' stages in order. The flat `chain` is the pipeline of those stages' chains and `field_transforms` collects their transforms by anchor in stage order.

A rule `{ pattern: { name: n }, replace: r }` expands, in this order, to a `rename_sort` from $n$ to `r.name` when that is a literal other than $n$ (a `compute_field` of `name` when it is a template), one `rename_field` per `rename_attrs` entry, one `remove_field` per `drop_attrs` entry, one `add_field` per `add_attrs` entry and one `apply_expr` per `map_attr_value` entry, each map in the order of its keys. A rule without `replace`, or with `replace: null`, expands to a `drop_sort` of $n$. The rules compile as the steps they expand to, followed by one `KeepFields` stage holding every rule's `keep_attrs` and, under `passthrough: drop`, one holding the name each replacing rule keeps (its literal new name, else $n$). An `add_attrs` entry's kind is read off its value: `integer` for a number an `i64` holds, `number` for any other number, then `boolean`, `string` (also for `null`), `array` and `object`.

A `compose` body with mode `vertical` concatenates its parts' stages, a part being an inline step list or a referenced lens's compiled stages. Vertical composition is therefore a monoid homomorphism from step lists under concatenation:

$$
\mathsf{compile}(a \mathbin{+\!\!+} b) = \mathsf{compile}(a) \mathbin{;} \mathsf{compile}(b),
$$

so it is associative and the empty composition is its identity. Mode `horizontal` compiles every part, then fuses each part's chain into one protolens (refusing the first part whose chain is empty with `EmptyHorizontalPart`) and folds the fused protolenses from the left with `protolens_horizontal`, which takes $\eta : F \Rightarrow G$ and $\theta : F' \Rightarrow G'$ to $\eta * \theta : F \circ F' \Rightarrow G \circ G'$; the result is one stage carrying every part's value transforms in part order, and zero parts give the empty lens. Every protolens a document compiles to has the identity as its source, and so does a horizontal composite of such protolenses, since $\mathrm{Id} \circ \mathrm{Id} = \mathrm{Id}$; adjacent protolenses therefore always compose, and fusion never refuses a compiled chain.

A `symmetric` body compiles each leg's steps at the focus vertex. A symmetric lens is assembled from the two legs' protolens chains alone, so a leg step that carries a value-level transform (`apply_expr`, `compute_field`, `add_field` with an `expr`) is refused with `ValueStepInSymmetricLeg` rather than dropped. The compiled lens's `chain` and `stages` are the left leg's.

Each directed equation becomes a `directed_eq` step, and its declared coercion class is sample-checked against the default coercion registry, every sample bound to the left-hand side when that side is a variable and to `x` otherwise (a binding the equation's form does not determine, and which this page fixes). A falsified class is refused with `CoercionNotHonest`. An equation's sides are terms, `ident` or `ident(term, ..., term)` with white space between tokens, so an empty argument (`f(x,)`) is refused with `TermParse`.

Under `invertible: true` a lens is refused with `NotInvertible` when a protolens of a stage or of a symmetric body's right leg, or a value-level transform, is lossy. A protolens is lossless when its complement keeps no residue: it renames a sort, an operation or an edge, adds or drops an equation, pulls back along a morphism, coerces with class `iso`, or is a directed equation with an inverse; a scoped protolens, a fused chain and a horizontal composite are lossless when every part is. A value-level transform is lossless when its class is `iso`.

[`compile_with_refs`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/resolver/fn.compile_with_refs.html) resolves `ref` entries against a set of documents, transitively: a referenced document's own references resolve against the same set. A reference to a document already open along the current chain (the compiled document included) is refused with `RefCycle`, an absent one with `UnresolvedRef`, and a referenced document's own refusal is reported as `InRef`, located in that document. A referenced lens with a `symmetric` body is refused by a `compose` part.

## Refusals and their location

Every refusal names the document path of the element refused, written as a JSON Pointer: a `Shape` refusal carries its path, and every other refusal is wrapped in `LensDslError::At`. A step's refusal is located at the member it concerns (an expression at its `expr` or `inverse`, a default at `default`, an equation term at `lhs` or `rhs`), a `coerce_sort` honesty failure at the step's spec, a refusal of a step a rule expands to at the rule member it came from (`/rules/i/replace/add_attrs/<key>`, say), a reference refusal at `/compose/lenses/i/ref`, a horizontal part refusal at `/compose/lenses/i`, a directed equation refusal at `/directed_equations/i`, `NotInvertible` at `/invertible`, `TargetMismatch` at `/target` and `AutoRequiresSchemas` at the body key. [`read_source`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/compile/fn.read_source.html), [`compile_source`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/compile/fn.compile_source.html) and `compile_with_source` locate a refusal of JSON text by the byte range of the value at that path, quotes and brackets included, in `LensDslError::Spanned`; a JSON syntax error is located at the byte where the reader stopped. A refusal of YAML text is located by the first character of the value at that path, and a YAML syntax error by the character where the YAML reader stopped: the YAML reader reports the position at which it began reading a node and no end position, so the extent of a YAML value is not available. A Nickel evaluation error, a contract breach among them, keeps the span Nickel reports in `NickelEvalSpanned`.

## Path complements and independent merging

Path execution does not merge the complements produced by sequential stages. `LensPath::get` first normalizes adjacent compatible stages into shape-specific groups, then retains one `PathComplementEntry` per group in execution order. Each entry records the group's complement, identity envelope, and stage digests. `LensPath::put` consumes those entries in reverse order and rejects a path complement with the wrong entry count, stage identity, or complement identity.

`ComplementCompose` supplies a separate partial-commutative merge for independently obtained complements of the same shape. The empty complement $\varepsilon$ is a two-sided identity. Property tests check commutativity and associativity on generated compatible complements. Composition rejects incompatible values stored under the same keyed complement field with `LensError::ComplementConflict`. `LensError::Identity` covers incompatible identity envelopes, mixed stamped and legacy identities, unequal legacy fingerprints, and unbound nonempty complements. Its causes include `IdentityError::Mismatch`, `IdentityError::LegacyComplementRejected`, and `IdentityError::UnboundComplement`. Vector and set-like fields are merged with deduplication. `ComplementCompose::is_compatible` tests whether composition would succeed without allocating the result.

A legacy fingerprint is a 64-bit hash of the source schema retained for compatibility with complements written before identity envelopes. Fingerprint equality is not a proof that distinct schema values are isomorphic.

## Checks and limits

The [Lens DSL correctness page](../../correctness/lens-dsl.md) proves that the schema-free compiler computes the stages this page defines, against an independent reference on finite scopes; it compares compiled lenses at the level of their protolenses, not their instances.

Lawful lens composition preserves the three equations when its premises hold. Property tests in [`crates/panproto-lens/src/laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens/src/laws.rs) and constructed DSL cases in [`crates/panproto-lens-dsl/tests/step_laws.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/tests/step_laws.rs) test selected generated inputs and step constructors. Finite generated samples do not prove lawfulness for every document or instance.

A `coerce_sort` step and every directed equation are also checked for honesty against registered samples. A detected violation returns `LensDslError::CoercionNotHonest`. Passing means that the sampled values satisfied the declared round-trip class.

The runtime checkers test one supplied case at a time, while property tests cover generated cases. Documents containing lossy or opaque transforms require scoped law claims; compilation alone does not prove them lawful. The semantics fixes the result of `put`, not its running time, and preserves step order without defining an equivalence on distinct documents that compile to the same lens.

## See also

- [Reference: lens combinators](../../reference/lens-combinators.md) for the combinator algebra.
- [How-to: write lenses in the lens DSL](../../how-to/lens-dsl.md).
- [Lenses and round-trip laws (plain-terms version)](../lenses-roundtrip.md).
- [Protolens composition](./protolens-composition.md) for schema-parameterized lenses.
- @foster2007combinators and @littvanhardenberghenry2020cambria.
