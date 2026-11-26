# Apply field transforms

A field transform computes or rewrites values during a lens operation. Use one when a vertex map cannot express the change, such as deriving a field from a sibling or changing a field's representation.

## Prerequisites

The TypeScript SDK and a source `BuiltSchema`. The current TypeScript lens-document path retains value transforms through compilation and instantiation.

## Transform an existing field

This document increments `count` on `get` and decrements it on `put`:

```ts
const document = {
  id: 'dev.example.increment-count',
  source: 'v1',
  target: 'v2',
  steps: [
    {
      apply_expr: {
        field: 'count',
        expr: 'add count 1',
        inverse: 'sub count 1',
        coercion: 'iso',
      },
    },
  ],
};

using chain = p.compileLensDocument(document, 'record:body');
using lens = chain.instantiate(sourceSchema);

const { view, complement } = lens.getJson(
  { count: 4 },
  'record:body',
);
const restored = lens.putJson(view, complement, 'record:body');
```

`apply_expr` evaluates its expression with the named field bound in the expression environment. An inverse is required only when edits must propagate backward through the transform. Declare `coercion: 'iso'` only when the forward and inverse expressions round-trip for every accepted value.

A record read from JSON holds each field as a child of the record, and `put` restores such a field as follows:

- An unedited field gets back its original value, through `put` and `putJson` alike. The complement records the original value where the view does not determine it: where the transform changed the value, where the result stays on the record because the field held null or a record (a `cid-link` or `bytes` value), and where carrying the shown value back through the inverse does not give the original (the value `new` under a rename of `old` to `new`, say).
- An edited field is tried with, in order: the shown value carried back through the inverses of every rewrite that ran on it, last first (a rewrite without an inverse passes it on); the shown value itself; and the shown value through each inverse alone. `put` keeps the first whose transform shows the edited value, and refuses the edit with `TransformNotInvertible` when none does.
- A view that holds a field on the record, where `get` left it because the child held null, keeps the child as it is: an edit of that field is refused, since `get` would show it on the child.

A complement taken by 0.78.0 or before lacks some of these records. Take it again with `get` when a rewrite's inverse is not an exact inverse (an inverse of `n * n` that returns `n`, or one that maps two values to one), since `put` cannot tell the original value from the shown one there.

## Compute a field from its parent record

`compute_field` evaluates an expression over the scalar fields in the parent fiber and writes the result under `target`:

```ts
const document = {
  id: 'dev.example.double-count',
  source: 'v1',
  target: 'v2',
  steps: [
    {
      compute_field: {
        target: 'double_count',
        expr: 'mul count 2',
        coercion: 'projection',
      },
    },
  ],
};

using chain = p.compileLensDocument(document, 'record:body');
using lens = chain.instantiate(sourceSchema);
const { view, complement } = lens.getJson(
  { count: 4 },
  'record:body',
);
```

The computed field is derived data. With no inverse, `putJson` uses the complement to restore the original source fields; edits made only to `double_count` do not determine a new `count`.

## Verify that compilation retained the transform

```ts
const transforms = chain.fieldTransforms();
if ((transforms['record:body'] ?? []).length === 0) {
  throw new Error('field transform was not compiled');
}

const laws = lens.checkLaws(instanceBytes);
if (!laws.holds) throw new Error(laws.violation ?? 'lens law failed');
```

`fieldTransforms()` reports transforms by parent vertex. `checkLaws` accepts an encoded instance, while `getJson` and `putJson` provide the record-oriented path shown above.

## Current serialization limitation

Value transforms are stored beside the structural `ProtolensChain`; they are not part of `chain.toJson()`. Reconstructing a handle with `ProtolensChainHandle.fromJson(chain.toJson(), wasm)` thus loses them.

The same distinction affects other surfaces. `schema lens compile` writes a versioned compiled-lens artifact whose authoritative `stages` retain each value transform in execution order; its `chain` and `field_transforms` fields are compatibility summaries. Python's `ProtolensChain.from_dsl_*` constructors, the TypeScript `compileLensDocument` handle, and Rust's `panproto_lens_dsl::CompiledLens` also retain the value-level programs and their order relative to structural steps.

Migration mapping JSON has a separate `expr_resolvers` field, but `panproto_mig::compile` does not install those expressions as `FieldTransform` values. Do not place expression source in a migration mapping and expect `schema lift` to execute it.

## Common failures

- Attach a transform to the parent vertex whose fields it reads. A transform anchored at a scalar child cannot see its siblings.
- Keep expressions within the evaluator's step, depth, and list-length budgets. Evaluation errors fail the transform for that record.
- Classify lossy computations as `projection` or `opaque`, not `iso`.
- Test boundary values for partial operations such as division, parsing, `head`, and indexing.

## See also

- [Expression-language reference](../reference/expression-language.md) for syntax, builtins, and evaluator limits.
- [Write lenses in the lens DSL](./lens-dsl.md) for the document structure.
- [Use lenses](./use-lenses.md) for `get`, `put`, and law checking.
