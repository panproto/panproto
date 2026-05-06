# Translate across protocols

Cross-protocol translation moves schemas between schema languages: JSON Schema to Protobuf, ATProto Lexicon to GraphQL SDL, and so on. Translation is a migration whose source and target theories belong to different protocols.

## Prerequisites

Two protocols whose theories overlap on enough building-block theories that a colimit-mediated translation exists. The CLI or one of the SDKs.

## The task

Cross-protocol translation runs through the colimit-of-theories construction in `panproto-protocols`. There is no single CLI subcommand that takes a source-protocol schema and emits a target-protocol schema directly; instead, the workflow is:

1. **Compose the theory.** Use the theory DSL to declare a composition theory whose colimit covers both protocols, or rely on the built-in compositions for the protocol pair you want.
2. **Generate a lens between schemas in the composed theory.** `schema lens generate` produces the chain; both schemas must be expressed against the composed theory.
3. **Apply the lens to convert data.**

The composition body in the theory DSL has shape `compose = { result, bases, steps }` where each step in `steps` is a `ColimitStepSpec { left, right, shared_sorts, shared_ops? }`. For example:

```nickel
{
  id = "dev.example.jsonschema-and-protobuf",
  description = "Composition of JSON Schema and Protobuf along the shared building blocks",
  compose = {
    result = "JsonSchemaAndProtobuf",
    bases = ["JsonSchemaTheory", "ProtobufTheory"],
    steps = [
      { left = "JsonSchemaTheory", right = "ProtobufTheory", shared_sorts = ["Vertex", "Edge"] },
    ],
  },
}
```

Then:

```sh
# Step 1: compose theories (one-time setup, or reuse a built-in).
schema theory compile theories/json-schema-and-protobuf.ncl

# Step 2: generate the chain between schemas in the composed protocol.
schema lens generate --protocol JsonSchemaAndProtobuf \
  schemas/user.jsonschema.json \
  schemas/user.protobuf.json \
  --save lenses/jsonschema-to-protobuf.json

# Step 3: apply.
schema lens apply --protocol JsonSchemaAndProtobuf \
  lenses/jsonschema-to-protobuf.json \
  data/user.json
```

For data conversion *within* a single protocol's schema fleet (different schemas, same protocol), use `schema data convert`:

```sh
schema data convert --protocol json-schema \
  --from schemas/user-v1.json --to schemas/user-v2.json \
  data/users/
```

## Verification

`schema lens verify --protocol <name> <data> <schema>` checks the round-trip laws on the converted data. A clean run means the chain is loss-free for the given samples.

## Common mistakes

- Reaching for a one-shot CLI conversion. The colimit composition step is essential for cross-protocol work; without it, `schema lens generate` has no shared theory to align the schemas against.
- Translating between protocols with non-overlapping required structure. The lens auto-generation will be partial, and `apply` will fail on records using the source-only structure. Distant protocols (say, FHIR to MongoDB) may require a hand-written chain on top of the auto-derived skeleton.
- Assuming auto-derived translations preserve every constraint. Constraints expressible in one theory but not the other are dropped; `schema lens verify` flags this.

## See also

- [Convert data between formats](./convert-data.md).
- [Reference: protocol catalogue](../reference/protocols.md).
- [Composing protocols by colimit](../explanation/protocol-colimits.md).
