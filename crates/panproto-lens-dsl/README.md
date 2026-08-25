# panproto-lens-dsl

[![crates.io](https://img.shields.io/crates/v/panproto-lens-dsl.svg)](https://crates.io/crates/panproto-lens-dsl)
[![docs.rs](https://docs.rs/panproto-lens-dsl/badge.svg)](https://docs.rs/panproto-lens-dsl)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Loads declarative lens specifications from Nickel, JSON, or YAML.

## Compilation

`load` selects the evaluator from the file extension and returns a `LensDocument`.
`compile` handles schema-parametric `steps`, `rules`, `compose`, and `symmetric`
bodies. It needs a body vertex and a callback for named lens references. The result is
a `CompiledLens` containing a `ProtolensChain`, value-level `FieldTransform`s,
metadata, and optional symmetric legs.

`auto` and `from_diff` bodies require `compile_with_schemas`, which takes source and
target schemas plus a `Protocol`. After compilation it instantiates the chain at the
source schema and compares the produced target NSID with the document's declared
target. `load_and_compile` is schema-independent and thus rejects these two body
forms.

## Step language

The `Step` enum currently has 19 forms. They include field addition, removal, and
renaming. Other forms cover expression-backed value transforms, hoisting, nesting,
scoped transforms, pullback, sort coercion and merge, and elementary changes to
sorts, operations, or equations. The enum in
[the API documentation](https://docs.rs/panproto-lens-dsl) is the syntax reference.

## Example

```rust,ignore
use panproto_lens_dsl::load_and_compile;
use std::path::Path;

let compiled = load_and_compile(Path::new("migrations/v1_to_v2.ncl"), "record:body")?;
let lens = compiled.instantiate(&source_schema, &protocol)?;
```

## Body forms

| Field | Compilation path |
|-------|------------------|
| `steps` | Compile an ordered list of `Step` values |
| `rules` | Compile pattern and replacement rules |
| `compose` | Resolve and combine named or inline lenses |
| `auto` | Run automatic generation with concrete schemas |
| `from_diff` | Derive steps from a concrete structural diff |
| `symmetric` | Compile left and right protolens chains |

## License

[MIT](../../LICENSE)
