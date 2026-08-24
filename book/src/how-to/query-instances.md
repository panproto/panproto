# Query instances

An instance query can filter records, select fields, or follow schema edges. Predicates and projections use the [expression language](../reference/expression-language.md).

## Prerequisites

A schema and an instance loaded against it. The expression-language reference for predicates.

## The task

### Filter and project

`executeQuery` is exported from `@panproto/core`. Its signature is `executeQuery(query, instance, wasm)` (query first, instance second, WASM module third). The query keys are `anchor`, `predicate` (an `Expr` object, not a string), `projection` (string array), `groupBy`, `limit`, and `path`.

```ts
import { executeQuery, parseExpr } from '@panproto/core';

const recent = executeQuery(
  {
    anchor: 'post',
    predicate: parseExpr('post.created_at > "2024-01-01"', p._wasm),
    projection: ['id', 'title'],
  },
  instance,
  p._wasm,
);
```

The predicate is evaluated against each matched node; `projection` selects the fields included in each `QueryMatch`.

### Following edges

Use the `path` field to traverse from the anchor before predicate matching:

```ts
const userPosts = executeQuery(
  { anchor: 'user', path: ['authored'], projection: ['title'] },
  instance,
  p._wasm,
);
```

To filter on edge presence, build a predicate with the instance-aware builtins (`Edge`, `Children`, `HasEdge`, `EdgeCount`) using `ExprBuilder` or `parseExpr`.

## Verification

`executeQuery` serializes the query and encoded instance directly to the WASM query engine. It does not accept a schema argument and does not perform a separate schema-level predicate type-check. Exercise each predicate against representative instances and treat a thrown `WasmError` as a failed query.

## Common mistakes

- Using `Edge`, `Children`, etc. in the standard evaluator. They return `Null` outside an instance environment. The query API sets the environment correctly; reaching for the bare evaluator does not.
- Hitting the step budget on large records. Heavy string operations on long fields will exceed the budget; either narrow the predicate or raise the budget at the call site.

## See also

- [Reference: expression language](../reference/expression-language.md).
- [Convert data between formats](./convert-data.md).
