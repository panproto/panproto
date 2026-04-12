# panproto-lens-dsl

[![crates.io](https://img.shields.io/crates/v/panproto-lens-dsl.svg)](https://crates.io/crates/panproto-lens-dsl)
[![docs.rs](https://docs.rs/panproto-lens-dsl/badge.svg)](https://docs.rs/panproto-lens-dsl)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Write lens specifications as config files (Nickel, JSON, or YAML) instead of Rust code.

## What it does

Instead of constructing a `ProtolensChain` in Rust by calling combinators by hand, you write a declarative spec file describing what the lens does: rename this field, remove that one, add a computed field from this expression. The crate loads the file, validates it, and compiles it to the same `ProtolensChain` and `FieldTransform` values that Rust code would produce.

The primary format is [Nickel](https://nickel-lang.org), a typed configuration language. Nickel validates your spec against a contract before compilation, gives precise error messages when a field is missing or has the wrong type, and lets you compose multiple spec files together using record merge. JSON and YAML are also accepted for simpler cases.

Specs support 19 step types covering field-level changes (`rename_field`, `remove_field`, `add_field`), value-level transforms (`apply_expr`, `compute_field`), structural changes (`hoist_field`, `nest_field`, `scoped`), and lower-level theory operations (`coerce_sort`, `merge_sorts`, `add_sort`, `drop_sort`, and others). The `auto` body variant delegates to `auto_generate` so you can auto-derive a lens from two schemas without listing any steps.

## Quick example

```rust,ignore
use panproto_lens_dsl::{load_and_compile};

let compiled = load_and_compile(
    std::path::Path::new("migrations/v1_to_v2.ncl"),
    "record:body",
)?;
// compiled.chain is a ProtolensChain ready for instantiation.
// compiled.field_transforms contains value-level transforms.
let lens = compiled.chain.instantiate(&v1_schema, &v2_schema)?;
```

The Nickel source for the spec above might look like:

```nickel
let L = import "panproto/lens.ncl" in
{
  id = "com.example.user.v1-to-v2",
  source = "com.example.user.v1",
  target = "com.example.user.v2",
  steps = [
    L.rename "createdAt" "created_at",
    L.remove "internalId",
    L.add "displayName" "string" "",
  ],
} | L.Lens
```

## API overview

| Export | What it does |
|--------|-------------|
| `load` | Load a `.ncl`, `.json`, `.yaml`, or `.yml` file into a `LensDocument` |
| `load_dir` | Load all spec files from a directory, returning successes and per-file errors separately |
| `compile` | Compile a `LensDocument` to a `CompiledLens` (chain + transforms) |
| `load_and_compile` | Load and compile in one call using the built-in resolver |
| `LensDocument` | Deserialized spec with fields `id`, `source`, `target`, and one body variant |
| `CompiledLens` | Output: `ProtolensChain`, `FieldTransform`s, and optional `AutoSpec` |
| `LensDslError` | Diagnostics for eval failures, expression parse errors, unresolved references |
| `LoadDirResult` | Holds both successfully loaded documents and per-file errors |

## Body variants

| Variant | When to use |
|---------|-------------|
| `steps` | List 19 available step types in order |
| `rules` | Pattern-match on field names or type IDs with replacement rules |
| `compose` | Combine named lens references vertically (pipeline) or horizontally (parallel) |
| `auto` | Auto-derive the lens from the two schemas; optionally pass quality hints |

## License

[MIT](../../LICENSE)
