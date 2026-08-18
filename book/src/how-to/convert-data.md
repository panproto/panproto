# Convert data between formats

Data conversion between schemas in one protocol follows the parse/migrate/emit pipeline. It passes through a `panproto-inst` instance graph and does not require a format-pair transcoder.

## Prerequisites

The `schema` CLI installed, or the SDK in your language. The source and target protocol names.

## The task

### Single file or directory (within one protocol)

```sh
schema data convert --protocol atproto \
  --from schemas/user-v1.json --to schemas/user-v2.json \
  data/users.json -o data/users-v2.json
```

`<DATA>` is a positional file or directory. `--from` and `--to` are *schema paths* (within the named protocol), not protocol names. Add `--direction backward` to push data the other way along the lens; add `--defaults k=v,...` to supply complement defaults.

For batch conversion, point `<DATA>` at a directory.

### Across protocols

Cross-protocol conversion goes through a composed theory; see [Translate across protocols](./cross-protocol.md). `schema data convert` is intra-protocol only.

### From the SDKs

```ts
using lens = p.lens(srcSchema, tgtSchema);
const { view, complement } = lens.getJson(inputRecord, "user:body");
const out = view as Record<string, unknown>;
```

`Panproto.lens(from, to)` auto-generates a lens between two `BuiltSchema` arguments. `getJson` accepts an ordinary JavaScript record and returns the converted view together with the complement needed for a backward update. Keep that complement if the application may call `putJson` later.

## Verification

To verify round-trip fidelity, generate a chain explicitly and run `schema lens verify`:

```sh
schema lens generate --protocol atproto schemas/user-v1.json schemas/user-v2.json --save chain.json
schema lens verify --protocol atproto data/users.json schemas/user-v2.json
```

Lens verification on test data exercises the three round-trip laws (GetPut, PutGet, PutPut); a pass means the chain is loss-free for the sampled records.

## Common mistakes

- Assuming all schema pairs are loss-free. They are not. Run `schema lens verify` after conversion to exercise the round-trip laws on representative data, and run `schema compat <old> <new> --protocol <name>` to classify the diff as fully compatible, backward compatible, or breaking.
- Skipping lens verification in CI. Without it, silent loss is possible.

## See also

- [Reference: protocol catalog](../reference/protocols.md).
- [Round-trip with format preservation](./format-preserving.md).
- [Translate across protocols](./cross-protocol.md).
