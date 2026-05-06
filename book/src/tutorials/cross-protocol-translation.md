# Cross-protocol translation

You will translate the `user` schema from JSON Schema to Protobuf, round-trip a record through both protocols, and verify the translation is loss-free. About twenty minutes.

By the end you will have: a Protobuf `.proto` file derived from your JSON Schema, a record converted from JSON to Protobuf binary and back, and a verification step proving the round-trip is exact.

## Prerequisites

Completed [Schema version control basics](./schema-vcs-basics.md). Your `my-first-schema/` project with a v2 user schema in `schemas/user.json`.

## Step 1: translate the schema

```sh
schema convert \
  --schema-from json-schema \
  --schema-to protobuf \
  --in schemas/user.json \
  --out schemas/user.proto
```

`schema convert` parses the JSON Schema against its theory, restricts to the building-block theories shared with Protobuf (graph + named + constraint), lifts into the Protobuf-specific extensions, and emits Protobuf source.

Look at `schemas/user.proto`:

```proto
syntax = "proto3";

message User {
  string name  = 1;
  int32  years = 2;
  string email = 3;
}
```

The translation is auto-derived from the shared structure. Field tags are assigned in order; the auto-derivation assumes a fresh `.proto` file (no existing tag layout to preserve).

## Step 2: convert a record

Take a v2-shape JSON record:

```sh
echo '{"name": "Alice", "years": 30, "email": "alice@example.com"}' > data/alice.json

schema convert \
  --from json-schema \
  --to protobuf \
  --in data/alice.json \
  --out data/alice.bin
```

`data/alice.bin` is the Protobuf binary encoding of the same record.

## Step 3: convert back

```sh
schema convert \
  --from protobuf \
  --to json-schema \
  --in data/alice.bin \
  --out data/alice.roundtrip.json

diff data/alice.json data/alice.roundtrip.json
```

The diff is empty. The round-trip is exact for this record because the schemas overlap on enough structure that no information is lost.

## Step 4: verify in one step

```sh
schema convert \
  --from json-schema \
  --to protobuf \
  --in data/alice.json \
  --out /tmp/alice.bin \
  --verify
```

`--verify` runs the round-trip internally and reports the diff. The output prints `lossless: true` for this case.

## Step 5: see what would be lossy

If your JSON Schema had a field with no Protobuf equivalent (say, a JSON Schema `pattern` constraint, which Protobuf does not encode), the verification would flag that field as lost on the JSON → Protobuf direction. The translation still completes; the warning tells you what the conversion drops.

## What you built

A working bridge between two schema languages, with a precise account of what is preserved and what is dropped. The same workflow extends to any pair of protocols panproto recognises ([the catalogue](../reference/protocols.md) lists 51).

## Next

- The plain-terms explanation of cross-protocol translation is at [Composing protocols by colimit](../explanation/protocol-colimits.md).
- For non-trivial pairs of protocols, the auto-derived translation may be a starting point; [Translate across protocols](../how-to/cross-protocol.md) covers when to extend it by hand.
- For the formal account of how the colimit makes this possible: [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md).

## Where to go from here

You have walked through the four core flows of panproto: defining schemas, evolving them via migrations, version-controlling the history, and translating between protocols. From here:

- The [how-to guides](../how-to/index.md) cover specific workflows in depth (CI, lenses, format-preserving codecs, language-model integration).
- The [reference quadrant](../reference/index.md) is the lookup for everything: CLI, SDKs, protocols, expression language, lens combinators, configuration.
- The [explanation quadrant](../explanation/index.md) is for understanding *why* the system is shaped the way it is.
