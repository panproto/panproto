# Write lenses in the lens DSL

The lens DSL is a declarative way to specify a lens between two schemas. Specs are written in Nickel, JSON, or YAML and compile to the lens combinator algebra.

## Prerequisites

A pair of schemas to bridge. The `schema` CLI or [`panproto-lens-dsl`](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl) crate.

## The task

### Write the spec (Nickel)

```nickel
# lenses/user-v1-to-v2.ncl
{
  id = "user.v1-to-v2",
  description = "Rename name fields and add display_name",
  steps = [
    { rename_field = { from = "first_name", to = "given_name" } },
    { rename_field = { from = "last_name",  to = "family_name" } },
    { add_field = {
        name = "display_name",
        default = "",
        expr = "Concat(record.given_name, \" \", record.family_name)",
      } },
  ],
}
```

Each step is a single-key object. The key picks the variant; the value carries its parameters. The DSL applies steps left-to-right against the source schema, producing a target schema and a `CompiledLens` between them. Full step grammar: [`crates/panproto-lens-dsl/src/document.rs`](https://github.com/panproto/panproto/blob/main/crates/panproto-lens-dsl/src/document.rs).

### Compile a chain

`schema lens` does not consume `.ncl`/`.json`/`.yaml` lens documents directly through the CLI. The DSL is compiled by the `panproto-lens-dsl` library; the CLI works on the compiled `ProtolensChain` it produces. To go from a pair of schemas to a chain via the CLI, use:

```sh
schema lens generate \
  --protocol json-schema \
  schemas/user-v1.json \
  schemas/user-v2.json \
  --save lenses/user-v1-to-v2.json
```

This auto-derives a chain. To author a chain by hand, write the lens DSL and call `panproto_lens_dsl::load` and `panproto_lens_dsl::compile` from Rust (or the equivalent SDK calls).

### Apply

```sh
schema lens apply --protocol json-schema lenses/user-v1-to-v2.json data/users.json
```

### Inspect

```sh
schema lens inspect --protocol json-schema lenses/user-v1-to-v2.json
```

Prints the combinator chain. `--protocol` is required.

## Verification

```sh
# Check the chain is applicable to every schema in a directory.
schema lens check --protocol json-schema lenses/user-v1-to-v2.json schemas/

# Verify the lens laws on test data.
schema lens verify --protocol json-schema data/users.json schemas/user-v2.json
```

`lens check` reports applicability without instantiating; `lens verify` checks the round-trip laws on actual data.

## Common mistakes

- Step ordering. The DSL deliberately exposes ordering; some sequences commute and produce the same result, others do not. When in doubt, run `schema lens check` with high sample counts.
- Forgetting `backward` on a `field_transform` step. Without it, the step is half a lens; compilation rejects.
- Writing schemas inline. The DSL expects path references; embedding a schema as a literal works for small examples but loses VCS tracking.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use lenses](./use-lenses.md).
