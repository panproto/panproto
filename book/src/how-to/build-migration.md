# Build a migration

A migration maps vertices and edges in a source schema to a target schema. Build or derive the mapping, check it, then apply it to representative data.

## Prerequisites

Two schemas using the same registered protocol and the `schema` CLI. Cross-protocol mappings require a separately constructed shared theory; see [Translate across protocols](./cross-protocol.md).

## Derive a mapping

Inspect the inferred span first:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json
```

Review its coverage and vertex map. Then save the right leg as a migration:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --monic --json \
  > migrations/v1-to-v2.json
```

`--monic` prevents two apex vertices from mapping to the same target vertex. Omit it only when the contraction is intentional and a separate value-level rule will combine the values.

The generated file maps the matched apex into `v2`, not necessarily all of `v1`. Use `--total` if partial coverage is unacceptable:

```sh
schema auto-migrate schemas/v1.json schemas/v2.json --total --json \
  > migrations/v1-to-v2.json
```

## Check the mapping

```sh
schema check \
  --src schemas/v1.json \
  --tgt schemas/v2.json \
  --mapping migrations/v1-to-v2.json \
  --typecheck
```

The existence check validates the migration against the source, target, and protocol theories. `--typecheck` also validates the induced GAT morphism. The command exits non-zero when either enabled check reports an error.

A passing check does not establish source-wide coverage. The mapping may omit unmatched source vertices, so retain the `auto-migrate` report or require `--total` when all source data must move.

## Apply the migration

```sh
schema lift \
  --migration migrations/v1-to-v2.json \
  --src-schema schemas/v1.json \
  --tgt-schema schemas/v2.json \
  data/user.json > data/user-v2.json
```

The default direction is `restrict` and the default instance type is `wtype`. The command infers the record root from the migration's mapped vertices. A mapping with no vertex entries cannot be lifted.

## Build a mapping in TypeScript

```ts
const builder = p
  .migration(srcSchema, tgtSchema)
  .map('user', 'user')
  .map('user:name', 'user:display_name');

const report = p.checkExistence(srcSchema, tgtSchema, builder);
if (!report.valid) throw new Error(JSON.stringify(report.errors));

using migration = builder.compile();
const migrated = migration.liftJson(oldRecord, 'user');
```

`MigrationBuilder` supports vertex maps, edge maps, and contraction resolvers. Per-field expressions are not part of this builder; use the [field-transform guide](./field-transforms.md) for the supported TypeScript lens-document route.

## Classify compatibility separately

Migration validity and compatibility answer different questions. To classify the schema change for CI, run:

```sh
schema compat schemas/v1.json schemas/v2.json --protocol atproto
```

The command exits `0` when it finds no breaking change, `1` for a breaking change, and `2` for a usage or load error. Add `--format json` for machine-readable output.

## Limitations

- `schema check` does not measure how much of the source schema is mapped.
- Automatic alignment ranks candidates; it does not certify that the selected correspondence matches domain intent. Review every generated map.
- A vertex map cannot split one value across several targets or compute a new value. Those operations require field transforms.

## See also

- [Find a span between two schemas](./spans.md) for coverage and search constraints.
- [Apply field transforms](./field-transforms.md) for value-level rewrites.
- [Migrations as morphisms](../explanation/migrations-as-morphisms.md) for the model.
- [CLI reference](../reference/cli.md) for all migration commands.
