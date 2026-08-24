# Write lenses in the lens DSL

The lens DSL describes schema-level steps and value-level field transforms in Nickel, JSON, or YAML. Exactly one body variant, such as `steps`, `rules`, `compose`, `auto`, `from_diff`, or `symmetric`, may be present.

## Write a document

This JSON document renames one property key:

```json
{
  "id": "dev.example.user-v1-to-v2",
  "description": "Rename the user name property",
  "source": "dev.example.user.v1",
  "target": "dev.example.user.v2",
  "steps": [
    {
      "rename_field": {
        "old": "name",
        "new": "display_name"
      }
    }
  ]
}
```

Each step is a single-key object. `--body-vertex` supplies the parent vertex for field-level steps; its default is `record:body`.

## Compile and apply in TypeScript

The TypeScript handle retains both the structural chain and any value transforms:

```ts
using chain = p.compileLensDocument(
  document,
  'record:body',
  'json',
);
using lens = chain.instantiate(sourceSchema);

const { view, complement } = lens.getJson(
  inputRecord,
  'record:body',
);
const restored = lens.putJson(view, complement, 'record:body');
```

`compileLensDocument` accepts a JavaScript object, text, or UTF-8 bytes. JSON is the default format; pass `'yaml'` for YAML. Nickel is not accepted by this WASM entry point because its imports require filesystem resolution.

Use `chain.fieldTransforms()` to confirm that `apply_expr` or `compute_field` steps survived compilation. Those transforms are stored beside the chain and do not appear in `chain.toJson()`.

## Compile Nickel or files in Rust

[`panproto-lens-dsl`](https://github.com/panproto/panproto/tree/main/crates/panproto-lens-dsl) resolves the bundled Nickel contract and filesystem imports:

```rust,no_run
use std::path::Path;

let compiled = panproto_core::lens_dsl::load_and_compile(
    Path::new("lenses/user-v1-to-v2.ncl"),
    "record:body",
)?;

println!("{} structural steps", compiled.chain.steps.len());
println!("{} transform anchors", compiled.field_transforms.len());
# Ok::<(), panproto_core::lens_dsl::LensDslError>(())
```

`load_and_compile` supports `.ncl`, `.json`, `.yaml`, and `.yml`. Named references in a `compose` body resolve against sibling documents in the same directory. `auto` and `from_diff` require the schema-aware `compile_with_schemas` entry point.

## Current CLI limitation

```sh
schema lens compile lenses/user-v1-to-v2.json \
  --body-vertex record:body \
  --out compiled.json
```

This command validates and compiles the document, but `compiled.json` is a metadata wrapper. Its `chain` member is the structural chain, while value transforms are represented only by a count. `schema lens apply` expects a directly serialized `ProtolensChain` and does not unwrap this output. Do not treat the file from `schema lens compile --out` as an immediately applicable lens artifact.

The CLI `lens generate --save` path also writes a human-oriented chain summary rather than the round-trippable serialization accepted by `ProtolensChain::from_json`. Use the Rust or TypeScript handle paths above for an operational lens.

## Verification

Instantiate the document against the intended source schema, run `get` and `put` on representative instances, and use `LensHandle.checkLaws` or `panproto_lens::laws::check_laws`. Compilation validates document shape and expression syntax; it does not show that a value transform is total on production data.

## See also

- [Apply field transforms](./field-transforms.md) for `apply_expr` and `compute_field`.
- [Use lenses](./use-lenses.md) for runtime operations.
- [Lens DSL: denotational semantics](../explanation/semantics/lens-dsl.md) for the document model.
