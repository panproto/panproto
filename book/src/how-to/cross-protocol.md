# Translate across protocols

Cross-protocol translation moves schemas between schema languages: JSON Schema to Protobuf, ATProto Lexicon to GraphQL SDL, and so on. Translation is a migration whose source and target theories belong to different protocols.

## Prerequisites

Two protocols whose theories overlap on enough building-block theories that a colimit-mediated translation exists. The CLI or one of the SDKs.

## The task

Cross-protocol translation runs through the colimit-of-theories construction in `panproto-protocols`. There is no single CLI subcommand that takes a source-protocol schema and emits a target-protocol schema directly; instead, the workflow is:

1. **Compose the theory.** Use the theory DSL to declare a composition theory whose colimit covers both protocols, or rely on the built-in compositions for the protocol pair you want.
2. **Generate a lens between schemas in the composed theory.** `schema lens generate` produces the chain; both schemas must be expressed against the composed theory.
3. **Apply the lens to convert data.**

The theory DSL exposes a small library of named building-block theories via `builtin_resolver()`: `ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta`, `ThSimpleGraph`, `ThHypergraph`, `ThInterface`, `ThFunctor`, `ThFlat`, `ThGraphInstance`. Whole-protocol theories (the per-protocol GATs that JSON Schema, Protobuf, ATProto, etc. compile to) are constructed in Rust inside `panproto-protocols`, not through the DSL.

The DSL composition shape is `compose = { result, bases, steps }`, with each step a `ColimitStepSpec { left, right, shared_sorts, shared_ops? }`. A toy composition over building blocks:

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

For cross-protocol translation between two existing built-in protocols (say, JSON Schema and Protobuf), the composition is in Rust inside [`crates/panproto-protocols`](https://github.com/panproto/panproto/tree/main/crates/panproto-protocols). Once both protocols are registered, the actual CLI flow is:

```sh
# Generate a chain between two schemas, one in each protocol; both schemas
# must be expressible against the same registered protocol theory.
schema lens generate --protocol <protocol> \
  schemas/user-a.json schemas/user-b.json \
  --save lenses/a-to-b.json

# Apply.
schema lens apply --protocol <protocol> lenses/a-to-b.json data/user.json
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
