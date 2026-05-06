# Cross-protocol translation

You will translate the `user` schema from JSON Schema to Protobuf, round-trip a record through both protocols, and verify the translation is loss-free. About twenty minutes.

By the end you will have: a Protobuf `.proto` file derived from your JSON Schema, a record converted from JSON to Protobuf binary and back, and a verification step proving the round-trip is exact.

## Prerequisites

Completed [Schema version control basics](./schema-vcs-basics.md). Your `my-first-schema/` project with a v2 user schema in `schemas/user.json`.

## Step 1: pick a shared theory

Cross-protocol translation requires both schemas to be expressible against a single theory that the lens generator can align them in. The two ways to get there:

- Use a *built-in* protocol that already covers the structure you need. The protocols in `panproto-protocols` are themselves composed from the building-block theories listed by `builtin_resolver()` (`ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta`, ...). For schema pairs that use only the shared structure, this is enough.
- Author a *custom composition* via the theory DSL when you need to combine theories the built-ins do not already give you.

For this tutorial we author a small composition over the building blocks. Save as `theories/constrained-multigraph.ncl`:

```nickel
{
  id = "dev.example.constrained-multigraph",
  description = "Compose ThGraph, ThConstraint, and ThMulti by identifying their Vertex and Edge sorts",
  compose = {
    result = "ConstrainedMultigraph",
    bases = ["ThGraph", "ThConstraint", "ThMulti"],
    steps = [
      { left = "ThGraph", right = "ThConstraint", shared_sorts = ["Vertex", "Edge"] },
      # Reference the prior step's intermediate result by its generated
      # name (`step_<i>`); the final intermediate is renamed to `result`.
      { left = "step_0", right = "ThMulti", shared_sorts = ["Vertex", "Edge"] },
    ],
  },
}
```

Compile it:

```sh
schema theory compile theories/constrained-multigraph.ncl
```

The compiler runs the colimit construction step by step. Failure here is a build-time bug in the composition (incompatible equations on a shared sort); success produces a registered theory that subsequent commands can target.

## Step 2: express schemas against the composed theory

Author two schemas of the same `user` model, one in each "flavour" your protocols would emit, both targeting the `ConstrainedMultigraph` theory. (For real cross-protocol work between JSON Schema and Protobuf, both protocols would already be built-in; the equivalent composed theory is set up in Rust in `panproto-protocols`.)

## Step 3: generate the chain

```sh
schema lens generate --protocol ConstrainedMultigraph \
  schemas/user-a.json \
  schemas/user-b.json \
  --save lenses/a-to-b.json
```

The chain is the bidirectional bridge between the two schemas; `--direction backward` on `apply` runs it the other way.

## Step 4: convert data

```sh
echo '{"name": "Alice", "years": 30, "email": "alice@example.com"}' > data/alice.json

schema lens apply --protocol ConstrainedMultigraph \
  lenses/a-to-b.json data/alice.json
```

## Step 5: verify

```sh
schema lens verify --protocol ConstrainedMultigraph \
  data/alice.json schemas/user-b.json
```

Verification runs the round-trip laws on the data; a clean run means the chain is loss-free for the input. Lossy spots (a constraint expressible in one schema but not the other) are reported.

## What you built

A small composed theory over panproto's building blocks, two schemas against it, and a verified chain between them. The same pattern, executed in Rust inside `panproto-protocols`, is how the 51 built-in protocols compose. Cross-protocol translation between built-in protocols then reduces to lens generation between schemas in their shared composed theory.

## See also

- [Translate across protocols](../how-to/cross-protocol.md) for the operational how-to.
- [Composing protocols by colimit](../explanation/protocol-colimits.md) for the model.
- [Theory DSL: denotational semantics](../explanation/semantics/theory-dsl.md).

## Next

- The plain-terms explanation of cross-protocol translation is at [Composing protocols by colimit](../explanation/protocol-colimits.md).
- For non-trivial pairs of protocols, the auto-derived translation may be a starting point; [Translate across protocols](../how-to/cross-protocol.md) covers when to extend it by hand.
- For the formal account of how the colimit makes this possible: [Pushouts and merge](../explanation/semantics/pushouts-and-merge.md).

## Where to go from here

You have walked through the four core flows of panproto: defining schemas, evolving them via migrations, version-controlling the history, and translating between protocols. From here:

- The [how-to guides](../how-to/index.md) cover specific workflows in depth (CI, lenses, format-preserving codecs, language-model integration).
- The [reference quadrant](../reference/index.md) is the lookup for everything: CLI, SDKs, protocols, expression language, lens combinators, configuration.
- The [explanation quadrant](../explanation/index.md) is for understanding *why* the system is shaped the way it is.
