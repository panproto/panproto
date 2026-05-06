# Query instances

A panproto instance is a graph of records. Queries filter and project that graph using the [expression language](../reference/expression-language.md).

## Prerequisites

A schema and an instance loaded against it. The expression-language reference for predicates.

## The task

### Filter and project

`executeQuery` is a standalone function in `@panproto/core`:

```ts
import { executeQuery } from '@panproto/core';

const recent = executeQuery(instance, {
  vertex: 'post',
  where: '\\post -> post.created_at > "2024-01-01"',
  select: '\\post -> { id: post.id, title: post.title }',
});
```

`where` is an expression of type `Bool` evaluated against each vertex. Records where the expression returns `false` are excluded. `select` is an expression of type `Record` evaluated against each matching vertex; the result is a list of records with the projected fields.

### Computed fields

```ts
const enriched = executeQuery(instance, {
  vertex: 'user',
  select: '\\u -> { ...u, full_name: Concat(u.first, " ", u.last) }',
});
```

The expression language's record-spread (`...`) and string builtins compose without restriction.

### Graph traversal

To follow edges, use the instance-aware builtins (`Edge`, `Children`, `HasEdge`, `EdgeCount`):

```ts
const usersWithPosts = executeQuery(instance, {
  vertex: 'user',
  where: '\\u -> EdgeCount(u, "authored") > 0',
});
```

## Verification

Predicates are type-checked against the schema before evaluation; an ill-typed predicate raises before any data is touched.

## Common mistakes

- Using `Edge`, `Children`, etc. in the standard evaluator. They return `Null` outside an instance environment. The query API sets the environment correctly; reaching for the bare evaluator does not.
- Hitting the step budget on large records. Heavy string operations on long fields will exceed the budget; either narrow the predicate or raise the budget at the call site.

## See also

- [Reference: expression language](../reference/expression-language.md).
- [Convert data between formats](./convert-data.md).
