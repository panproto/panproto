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

The same distinction affects other surfaces. `schema lens compile` reports only the number of field-transform vertices and writes the structural chain, and Python's `ProtolensChain.from_dsl_*` constructors currently return only that structural chain. Use the TypeScript `compileLensDocument` handle or Rust's `panproto_lens_dsl::CompiledLens`, whose `field_transforms` field retains the value-level programs.

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
