# Translate across protocols

Cross-protocol translation moves schemas between schema languages: JSON Schema to Protobuf, ATProto Lexicon to GraphQL SDL, and so on. Translation is a migration whose source and target theories belong to different protocols.

## Prerequisites

Two protocols whose theories overlap on enough building-block theories that a colimit-mediated translation exists. The CLI or one of the SDKs.

## The task

```sh
schema convert --schema-from json-schema --schema-to protobuf --in schemas/user.json --out schemas/user.proto
```

The pipeline: parse `user.json` against the JSON Schema theory; restrict to the shared building-block theories (graph + constraint + named); lift into the Protobuf-extended theory; emit as Protobuf source.

To translate data alongside the schema:

```sh
schema convert --from json-schema --to protobuf --in data/user.json --out data/user.bin --verify
```

`--verify` round-trips the data through both directions and reports any loss.

## Verification

```sh
schema check --src schemas/user.json --tgt schemas/user.proto --mapping <auto> --typecheck
```

`<auto>` invokes the auto-derived translation. `check --typecheck` confirms the migration is well-defined; `--verify` on the data path confirms round-trip fidelity.

## Common mistakes

- Translating between protocols with non-overlapping required structure. The migration will be partial, and lift will fail on records using the source-only structure. Translation across distant protocols (say, FHIR to MongoDB) may require a hand-written migration on top of the auto-derived skeleton.
- Assuming auto-derived translations preserve every constraint. Constraints that are expressible in one theory but not the other are dropped. `--verify` flags this.

## See also

- [Convert data between formats](./convert-data.md).
- [Reference: protocol catalogue](../reference/protocols.md).
- [Composing protocols by colimit](../explanation/protocol-colimits.md).
