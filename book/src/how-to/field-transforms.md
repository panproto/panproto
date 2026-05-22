# Apply field transforms

A *field transform* is a value-level expression applied during migration: a way to compute the new field's value from the old data. Transforms are written in the [expression language](../reference/expression-language.md).

## Prerequisites

A migration mapping between two schemas. The expression language reference for available builtins.

## The task

### Inline a transform in a mapping file

The mapping JSON consumed by `schema check` is a serialized [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/struct.Migration.html). Field-level value transforms attach as **expression resolvers** indexed by the `(src_vertex, tgt_vertex)` pair they bridge:

```json
{
  "vertex_map": {
    "user": "user",
    "user:first": "user:given_name"
  },
  "edge_map": [],
  "hyper_edge_map": {},
  "label_map": [],
  "resolver": [],
  "hyper_resolver": [],
  "expr_resolvers": [
    [["user:full_name", "user:full_name"],
     "\\r -> concat(r.first, \" \", r.last)"]
  ]
}
```

Each `expr_resolvers` entry is `[[src_vertex, tgt_vertex], expression]`. The expression is parsed by `panproto-expr-parser` and applied during lift (old to new). Backward direction is supplied by the lens/protolens layer rather than the mapping file: pair an `ApplyExpr` field transform with its inverse on the corresponding `Protolens` step, or annotate a coercion on the schema and let the migration compiler emit both directions.

### From the SDKs

The TypeScript and Python SDKs do not yet expose per-field transforms on the `MigrationBuilder`. To compose a migration with a value-level rewrite, build a `ProtolensChain` directly (`combinators::rename_field`, `elementary::apply_expr`, ...) and call `compile_migration` on it:

```python
import panproto

# `panproto.rename_field`, `add_field`, `remove_field`, `hoist_field`,
# and `pipeline` each return a `ProtolensChain`. Serialize and apply via
# `chain.to_json()` and the lens APIs.
chain = panproto.rename_field("user", "full_name", "full_name", "name")
print(chain.to_json())
```

To attach a value-level expression resolver to a `Migration` between two specific schemas, write the mapping JSON by hand (see above) and call `panproto.compile_migration(migration, src_schema, tgt_schema)`.

## Verification

```sh
schema check --src schemas/v1.json --tgt schemas/v2.json --mapping migration.json --typecheck
```

`check --typecheck` ensures the transforms type-check against the source and target schemas. Property tests in CI then verify the lens laws on sampled data.

## Common mistakes

- Expecting `expr_resolvers` to supply both directions. The mapping carries forward-only expressions; the backward leg comes from the lens/protolens layer (pair an `ApplyExpr` field transform with its inverse on the corresponding protolens step, or annotate a coercion on the schema and let the migration compiler emit both directions).
- Using IO or random functions in the expression. The language is bounded-pure; non-deterministic builtins are not exposed.
- Letting the budget exceed. Long string operations on large records can hit the step budget. Expressions that hit the budget raise `ExprError::StepLimitExceeded` at runtime.

## See also

- [Reference: expression language](../reference/expression-language.md) for builtins and types.
- [Build a migration](./build-migration.md) for the surrounding workflow.
- [Lenses and round-trip laws](../explanation/lenses-roundtrip.md) for why `backward` matters.
