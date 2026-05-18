# Convert data between formats

panproto's parse/migrate/emit pipeline converts data between any two protocols whose theories overlap. The intermediate is a `panproto-inst` instance graph; you do not need to write a transcoder per format pair.

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
const out = await p.convert(jsonBytes, {
  from: srcSchema,
  to: tgtSchema,
});
```

`Panproto.convert(data, opts)` auto-generates a lens between the two `BuiltSchema` arguments, applies it forward, and returns the converted record. Pass `defaults` in `opts` to supply complement defaults for fields the source does not determine.

## Verification

To verify round-trip fidelity, generate a chain explicitly and run `schema lens verify`:

```sh
schema lens generate --protocol atproto schemas/user-v1.json schemas/user-v2.json --save chain.json
schema lens verify --protocol atproto data/users.json schemas/user-v2.json
```

Lens verification on test data exercises the three round-trip laws (GetPut, PutGet, PutPut); a pass means the chain is loss-free for the sampled records.

## Common mistakes

- Assuming all schema pairs are loss-free. They are not. Run `schema lens verify` after conversion to exercise the round-trip laws on representative data, and run `schema check --src --tgt --mapping ... --typecheck` to classify the diff as fully compatible, backward compatible, or breaking.
- Skipping lens verification in CI. Without it, silent loss is possible.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Round-trip with format preservation](./format-preserving.md).
- [Translate across protocols](./cross-protocol.md).
