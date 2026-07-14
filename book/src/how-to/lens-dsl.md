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

`schema lens compile` loads a lens DSL document and compiles it to a `ProtolensChain`, writing the chain JSON to stdout or to `--out`:

```sh
schema lens compile lenses/user-v1-to-v2.ncl \
  --body-vertex record:body \
  --out lenses/user-v1-to-v2.json
```

`--body-vertex` names the parent vertex under which field-level steps attach (default `record:body`). The command compiles the schema-parametric bodies (`steps`, `rules`, `compose`, `symmetric`) and resolves `compose` references by lens `id` against sibling documents in the same directory. The `auto` and `from_diff` bodies need a concrete source and target schema, so they compile through the library entry point `compile_with_schemas` or the SDK bindings rather than this command.

To auto-derive a chain from a pair of schemas instead of authoring a document, use `schema lens generate`:

```sh
schema lens generate \
  --protocol atproto \
  schemas/user-v1.json \
  schemas/user-v2.json \
  --save lenses/user-v1-to-v2.json
```

### Apply

```sh
schema lens apply --protocol atproto lenses/user-v1-to-v2.json data/users.json
```

### Inspect

```sh
schema lens inspect --protocol atproto lenses/user-v1-to-v2.json
```

Prints the combinator chain. `--protocol` is required.

### Compile from Python

`ProtolensChain` exposes loaders that consume a lens-DSL document and compile it directly to a chain anchored at the named body vertex of the source schema:

```python
import panproto

chain = panproto.ProtolensChain.from_dsl_path(
    "lenses/user-v1-to-v2.ncl",
    body_vertex="record:body",
)

# Or from a string:
chain = panproto.ProtolensChain.from_dsl_json(json_source, "record:body")
chain = panproto.ProtolensChain.from_dsl_yaml(yaml_source, "record:body")
chain = panproto.ProtolensChain.from_dsl_nickel(nickel_source, "record:body")
```

`from_dsl_path` dispatches on file extension (`.ncl` / `.json` / `.yaml` / `.yml`). For Nickel, an optional `import_paths` argument extends the import-resolution lookup so user-defined modules can be referenced.

## Verification

```sh
# Check the chain is applicable to every schema in a directory.
schema lens check --protocol atproto lenses/user-v1-to-v2.json schemas/

# Verify the lens laws on test data.
schema lens verify --protocol atproto data/users.json schemas/user-v2.json
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
