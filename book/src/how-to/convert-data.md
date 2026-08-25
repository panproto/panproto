# Convert data between schemas

`schema data convert` auto-generates a lens between two schemas in one protocol and applies it to JSON records.

## Prerequisites

The `schema` CLI, two files in panproto's internal schema JSON format, and input records rooted at a vertex the CLI can infer from the source schema. The current command resolves only the `atproto` protocol.

## Convert one file

```sh
schema data convert --protocol atproto \
  --from schemas/user-v1.json \
  --to schemas/user-v2.json \
  data/user.json \
  -o data/user-v2.json
```

`--from` and `--to` are schema paths. With no `--chain`, the command generates a lens using `AutoLensConfig::default()`, parses the input as a W-type instance, runs `get`, and serializes the view against the target schema.

Defaults use comma-separated `key=value` pairs:

```sh
schema data convert --protocol atproto \
  --from schemas/user-v1.json \
  --to schemas/user-v2.json \
  --defaults status=active,locale=en \
  data/user.json
```

The CLI parses all default values as strings. A key resolves against an added
target vertex ID first, then a unique incoming field label, and finally the
vertex kind. The generated lens adds the value only when the source-backed
parent does not already carry that field. Generation reports an error when a
default is unused, ambiguous, or cannot be placed from the target schema. With
`--chain`, defaults must already be embedded in the serialized chain;
`--defaults` cannot override them.

## Convert a directory

Directory mode reads the immediate `*.json` files and requires an output directory:

```sh
schema data convert --protocol atproto \
  --from schemas/user-v1.json \
  --to schemas/user-v2.json \
  data/users \
  -o data/users-v2
```

Files that fail to load or convert are reported as skipped, and the final line prints converted and skipped counts. Inspect that count in automation; directory mode can finish after skipping individual files.

## Convert from TypeScript

```ts
using lens = p.lens(srcSchema, tgtSchema);
const { view, complement } = lens.getJson(inputRecord, 'user');
const output = view as Record<string, unknown>;
```

Retain `complement` if the application may later call `putJson` to propagate an edited view backward.

## Verify the conversion

Check the actual record with the SDK law checker:

```ts
const instance = p.parseJson(srcSchema, JSON.stringify(inputRecord));
const result = lens.checkLaws(instance._bytes);
if (!result.holds) {
  throw new Error(result.violation ?? 'lens law failed');
}
```

`schema lens verify <DATA> <SCHEMA> --protocol <PROTOCOL>` parses the data with the supplied schema and runs the concrete GetPut and PutGet checks. A passing result applies to that record. It is not a proof over every instance of the schema.

Backward CLI conversion uses an empty complement. It can reconstruct only lenses that need no captured source data or for which defaults suffice. Use an SDK `get`/`put` pair when backward conversion depends on the complement produced from a specific source record.

`--chain` selects a saved chain instead of automatic lens generation. It still requires `--from` and `--to`, since a serialized `ProtolensChain` does not contain the concrete schemas needed for instantiation. `schema lens generate --save` writes this round-trippable chain format. The compiled artifact produced by `schema lens compile --out` also carries ordered value-transform stages and is intended for `schema lens apply`, not `schema convert --chain`.

## See also

- [Use lenses](./use-lenses.md) for bidirectional updates.
- [Apply field transforms](./field-transforms.md) for computed values.
- [Translate across protocols](./cross-protocol.md) for the current cross-protocol limitation.
