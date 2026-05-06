# Convert data between formats

panproto's parse/migrate/emit pipeline converts data between any two protocols whose theories overlap. The intermediate is a `panproto-inst` instance graph; you do not need to write a transcoder per format pair.

## Prerequisites

The `schema` CLI installed, or the SDK in your language. The source and target protocol names.

## The task

### Single file

```sh
schema convert --from json-schema --to protobuf --in data/users.json --out data/users.bin
```

The pipeline: parse `users.json` against the JSON Schema theory; restrict the resulting instance through the JSON-Schema-to-Protobuf migration; emit against the Protobuf theory.

### Batch

```sh
schema convert --from atproto --to json-schema --in records/ --out records-json/
```

When `--in` is a directory, each file is converted in turn. Errors are collected and reported at the end; a single bad record does not abort the batch.

### From the SDKs

```ts
const out = p.convert({
  from: 'json-schema',
  to: 'protobuf',
  data: jsonBytes,
});
```

## Verification

```sh
schema convert --from json-schema --to protobuf --in data/users.json --out /tmp/out.bin --verify
```

`--verify` round-trips the output back to the source format and diffs the result against the input. A clean diff means the conversion is loss-free for the input. Lossy conversions are flagged.

## Common mistakes

- Assuming all protocol pairs are loss-free. They are not. The diff classification (fully compatible, backward compatible, lossy) is reported in `--verify` output; treat anything other than fully compatible with care.
- Omitting `--verify` in CI. Without it, silent loss is possible.

## See also

- [Reference: protocol catalogue](../reference/protocols.md).
- [Round-trip with format preservation](./format-preserving.md).
- [Translate across protocols](./cross-protocol.md).
