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

### Compile

```sh
schema lens generate --spec lenses/user-v1-to-v2.ncl --out lenses/user-v1-to-v2.json
```

The output is the compiled lens, ready to apply with `schema migrate` or to load from any SDK.

### Inspect

```sh
schema lens inspect lenses/user-v1-to-v2.json
```

Prints the combinator chain, the optic kinds, and the law-check status.

## Verification

```sh
schema lens check lenses/user-v1-to-v2.json --samples 1000
```

Runs property tests for the three round-trip laws against 1000 sampled records. Exits non-zero with a shrunk counterexample on failure.

## Common mistakes

- Step ordering. The DSL deliberately exposes ordering; some sequences commute and produce the same result, others do not. When in doubt, run `schema lens check` with high sample counts.
- Forgetting `backward` on a `field_transform` step. Without it, the step is half a lens; compilation rejects.
- Writing schemas inline. The DSL expects path references; embedding a schema as a literal works for small examples but loses VCS tracking.

## See also

- [Reference: lens combinators](../reference/lens-combinators.md).
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md).
- [Use lenses](./use-lenses.md).
