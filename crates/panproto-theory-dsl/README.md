# panproto-theory-dsl

[![crates.io](https://img.shields.io/crates/v/panproto-theory-dsl.svg)](https://crates.io/crates/panproto-theory-dsl)
[![docs.rs](https://docs.rs/panproto-theory-dsl/badge.svg)](https://docs.rs/panproto-theory-dsl)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Loads theory, morphism, composition, and protocol documents from Nickel, JSON, or
YAML.

## Processing model

`load` evaluates or deserializes one file into a `TheoryDocument`. Nickel evaluation
uses the bundled Nickel contracts and import paths. JSON and YAML go directly through
their evaluators and serde. `load_dir` reports successful documents and per-file
errors separately.

`compile` dispatches on the document body and returns a `CompiledTheorySet`. By
default it also checks declared directed-equation coercion classes against the built-in
sample registry. `compile_with_registry` accepts other samples, while
`compile_unchecked` skips that finite sample check. A successful sample check is not a
proof over all values.

The eight body forms are `theory`, `morphism`, `compose`, `protocol`, `bundle`,
`class`, `instance`, and `inductive`. Composition is implemented with the colimit
operations from `panproto-gat`, subject to that crate's amalgamation conventions.

The GAT vocabulary follows [Cartmell (1986)](https://doi.org/10.1016/0168-0072(86)90053-9).
The theory-composition design follows [Burstall and Goguen (1977)](https://www.ijcai.org/Proceedings/77-2/Papers/095.pdf).

## Example

```rust,ignore
use panproto_theory_dsl::{builtin_resolver, compile, load};
use std::path::Path;

let document = load(Path::new("theories/my_format.json"))?;
let resolver = builtin_resolver();
let compiled = compile(&document, &resolver)?;
```

## Public API

| Item | Purpose |
|------|---------|
| `load`, `load_dir` | Read documents by file extension |
| `compile`, `compile_with_registry`, `compile_unchecked` | Compile with selected coercion checking |
| `compile_with_source` | Attach best-effort JSON or YAML source spans to type errors |
| `compile_bundle` | Compile bundled definitions in dependency order |
| `load_and_compile` | Load and compile with a caller-supplied resolver |
| `builtin_resolver` | Resolve the 11 built-in theory constructors |
| `TheoryDocument`, `TheoryBody`, `TheorySpec`, `BundleSpec` | Root document types re-exported by the crate |
| `CompiledTheorySet` | Maps of compiled theories, morphisms, protocols, and composition specs |
| `TheoryDslError`, `LoadDirResult` | Diagnostics and directory-load result |

Other specification structs, including class, instance, inductive, and import types,
are public under `panproto_theory_dsl::document` rather than re-exported at the crate
root.

## License

[MIT](../../LICENSE)
