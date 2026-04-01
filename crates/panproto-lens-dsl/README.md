# panproto-lens-dsl

[![crates.io](https://img.shields.io/crates/v/panproto-lens-dsl.svg)](https://crates.io/crates/panproto-lens-dsl)
[![docs.rs](https://docs.rs/panproto-lens-dsl/badge.svg)](https://docs.rs/panproto-lens-dsl)

Declarative lens DSL for panproto.

Provides a human-readable specification format for lenses, protolenses, and related optical constructs. The primary authoring format is [Nickel](https://nickel-lang.org) (via `nickel-lang` 2.0), a typed configuration language with record merge for composition, functions for parameterized templates, contracts for validation, and imports for modularity. JSON and YAML are also supported for simpler cases.

## Evaluation pipeline

```
*.ncl / *.json / *.yaml   (human-authored)
         │
         ▼
    LensDocument           (normalized record)
         │
         ▼
ProtolensChain + FieldTransforms   (panproto algebra)
```

## Example (Nickel)

```nickel
let L = import "panproto/lens.ncl" in

{
  id = "dev.example.user.db-projection",
  source = "dev.example.user",
  target = "dev.example.user.view",
  steps = [
    L.remove "internalId",
    L.rename "createdAt" "created_at",
    L.add "displayName" "string" "",
    L.add_computed "fullName" "string" ""
      'concat firstName " " lastName',
  ],
} | L.Lens
```

## API

| Item | Description |
|------|-------------|
| `load` | Load a lens document from a `.ncl`, `.json`, `.yaml`, or `.yml` file |
| `load_dir` | Load all lens documents from a directory, returning `LoadDirResult` with documents and per-file errors |
| `compile` | Compile a `LensDocument` to a `ProtolensChain` + `FieldTransform`s via a resolver callback |
| `load_and_compile` | Load and compile in one step |
| `LensDocument` | Deserialized lens specification with four body variants |
| `CompiledLens` | Compilation output: chain, field transforms, extensions, and optional `AutoSpec` |
| `LensDslError` | Diagnostic errors (nickel eval, JSON, YAML, expression parse, unresolved ref, rule compile) |
| `LoadDirResult` | Result of directory loading with both documents and errors |

## Body variants

| Variant | Description |
|---------|-------------|
| `steps` | Sequential pipeline of 19 step types mapping to `panproto_lens::combinators` and `elementary` |
| `rules` | Pattern-match rewrite rules with `passthrough`, `keep_attrs`, `map_attr_value` |
| `compose` | Vertical (pipeline) or horizontal (fuse + `protolens_horizontal`) composition of named lens references |
| `auto` | Delegation to `auto_lens::auto_generate` with caller-visible `AutoSpec` |

## Step types (19)

Field combinators: `remove_field`, `rename_field`, `add_field`. Value-level: `apply_expr`, `compute_field`. Structural: `hoist_field`, `nest_field`, `scoped` (recursive). Theory-level: `pullback`, `coerce_sort`, `merge_sorts`, `add_sort`, `drop_sort`, `rename_sort`, `add_op`, `drop_op`, `rename_op`, `add_equation`, `drop_equation`.

## Nickel contract library

The bundled `contracts/lens.ncl` provides `Lens`, `Step`, `Rule` contracts and combinator functions: `remove`, `rename`, `add`, `add_computed`, `apply`, `compute`, `hoist`, `nest`, `map_items`, `pullback`, `coerce`, `merge`, plus template helpers (`counter_fields`, `string_fields`, `map_name`, `drop_feature`).

## Composition via Nickel record merge

```nickel
let L = import "panproto/lens.ncl" in
let base = import "base.ncl" in
let auth = import "lib/auth.ncl" in

base & auth & {
  id = "composed.v1",
  source = "my.source",
  target = "my.target",
} | L.Lens
```
