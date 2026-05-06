# Apply field transforms

A *field transform* is a value-level expression applied during migration: a way to compute the new field's value from the old data. Transforms are written in the [expression language](../reference/expression-language.md).

## Prerequisites

A migration mapping between two schemas. The expression language reference for available builtins.

## The task

### Inline a transform in a mapping file

```json
{
  "src": "schemas/v1.json",
  "tgt": "schemas/v2.json",
  "renames": [{ "from": "first", "to": "given" }],
  "field_transforms": [
    {
      "at": "user.full_name",
      "forward": "\\record -> Concat(record.first, ' ', record.last)",
      "backward": "\\new -> { first: Head(Split(new.full_name, ' ')), last: Join(Tail(Split(new.full_name, ' ')), ' ') }"
    }
  ]
}
```

`forward` is applied during lift (old → new). `backward` is applied during put (new → old) and is required for the migration to be a lawful lens.

### From the SDKs

```ts
const mig = p.migration(src, tgt, {
  fieldTransforms: [{
    at: 'user.full_name',
    forward: '\\r -> Concat(r.first, " ", r.last)',
    backward: '\\n -> { first: Head(Split(n.full_name, " ")), last: Join(Tail(Split(n.full_name, " ")), " ") }',
  }],
});
```

## Verification

```sh
schema check --src schemas/v1.json --tgt schemas/v2.json --mapping migration.json --typecheck
```

`check --typecheck` ensures the transforms type-check against the source and target schemas. Property tests in CI then verify the lens laws on sampled data.

## Common mistakes

- Omitting `backward`. Without it, the migration is one-way and cannot satisfy the round-trip laws. CI tests will reject it.
- Using IO or random functions in the expression. The language is bounded-pure; non-deterministic builtins are not exposed.
- Letting the budget exceed. Long string operations on large records can hit the step budget. Expressions that hit the budget raise `BudgetExceeded` at runtime.

## See also

- [Reference: expression language](../reference/expression-language.md) for builtins and types.
- [Build a migration](./build-migration.md) for the surrounding workflow.
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md) for why `backward` matters.
