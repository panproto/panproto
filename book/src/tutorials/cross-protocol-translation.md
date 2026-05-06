# Cross-protocol translation

You will translate the `user` schema from JSON Schema to Protobuf, round-trip a record through both protocols, and verify the translation is loss-free. About twenty minutes.

By the end you will have: a Protobuf `.proto` file derived from your JSON Schema, a record converted from JSON to Protobuf binary and back, and a verification step proving the round-trip is exact.

## Prerequisites

Completed [Schema version control basics](./schema-vcs-basics.md). Your `my-first-schema/` project with a v2 user schema in `schemas/user.json`.

## Step 1: compose the theories

Cross-protocol translation runs through a *composed theory*: a single GAT containing both protocols' extensions over the shared building blocks. There is no one-shot CLI command; the composition is the prerequisite.

Author a small theory document `theories/jsonschema-and-protobuf.ncl` that composes the two. The composition body has shape `compose = { result, bases, steps }`, where each step is `{ left, right, shared_sorts, shared_ops? }`:

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

Compile it:

```sh
schema theory compile theories/jsonschema-and-protobuf.ncl
```

The compiler runs the colimit construction over the named components along the shared theories. Failure at this step is a build-time bug in the composition (incompatible equations on a shared sort); success produces a registered protocol whose schemas can mention either protocol's vertex kinds.

## Step 2: write schemas in the composed protocol

Express the same `user` model as both a JSON Schema-flavoured schema and a Protobuf-flavoured schema, both against `JsonSchemaAndProtobuf`:

```sh
# schemas/user-jsonschema.json and schemas/user-protobuf.json
# (each is a panproto schema graph; the protocol field is "JsonSchemaAndProtobuf")
```

## Step 3: generate the chain

```sh
schema lens generate \
  --protocol JsonSchemaAndProtobuf \
  schemas/user-jsonschema.json \
  schemas/user-protobuf.json \
  --save lenses/jsonschema-to-protobuf.json
```

The chain is the bidirectional bridge; `--direction backward` on `apply` runs it the other way.

## Step 4: convert data

```sh
echo '{"name": "Alice", "years": 30, "email": "alice@example.com"}' > data/alice.json

schema lens apply \
  --protocol JsonSchemaAndProtobuf \
  lenses/jsonschema-to-protobuf.json \
  data/alice.json
```

The output is the same record expressed against the Protobuf-flavoured schema.

## Step 5: verify

```sh
schema lens verify \
  --protocol JsonSchemaAndProtobuf \
  data/alice.json \
  schemas/user-protobuf.json
```

Verification runs the three round-trip laws (GetPut, PutGet, PutPut) on the data; a clean run means the chain is loss-free for the input. Lossy spots (a JSON Schema `pattern` constraint that Protobuf does not encode, for example) are reported.

## What you built

A working bridge between two schema languages, with a precise account of what is preserved and what is dropped. The composed-theory pattern extends to any pair of protocols panproto recognises ([the catalogue](../reference/protocols.md) lists 51).

## Next

- The plain-terms explanation of cross-protocol translation is at [Composing protocols by colimit](../explanation/protocol-colimits.md).
- For non-trivial pairs of protocols, the auto-derived translation may be a starting point; [Translate across protocols](../how-to/cross-protocol.md) covers when to extend it by hand.
- For the formal account of how the colimit makes this possible: [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md).

## Where to go from here

You have walked through the four core flows of panproto: defining schemas, evolving them via migrations, version-controlling the history, and translating between protocols. From here:

- The [how-to guides](../how-to/index.md) cover specific workflows in depth (CI, lenses, format-preserving codecs, language-model integration).
- The [reference quadrant](../reference/index.md) is the lookup for everything: CLI, SDKs, protocols, expression language, lens combinators, configuration.
- The [explanation quadrant](../explanation/index.md) is for understanding *why* the system is shaped the way it is.
