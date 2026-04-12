# panproto-theory-dsl

[![crates.io](https://img.shields.io/crates/v/panproto-theory-dsl.svg)](https://crates.io/crates/panproto-theory-dsl)
[![docs.rs](https://docs.rs/panproto-theory-dsl/badge.svg)](https://docs.rs/panproto-theory-dsl)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Write theory definitions (schema language specifications) as config files (Nickel, JSON, or YAML) instead of Rust code.

## What it does

A theory defines what a valid schema looks like for a particular format: what kinds of nodes exist, what relationships are allowed between them, and what equations must hold. In panproto terms, this is a generalized algebraic theory (a GAT): a set of sorts (node kinds), operations (edge kinds between sorts), and equations (constraints that models must satisfy). Protocols are built by combining theories.

Instead of constructing `Theory`, `TheoryMorphism`, and `Protocol` values in Rust by hand, you write a spec file in Nickel, JSON, or YAML. The crate loads the file, validates it against a contract, and compiles it to the same objects. The Nickel format adds parameterized templates (so you can write reusable theory fragments), record merge (so you can compose fragments from multiple files), and typed contracts (so errors surface at load time rather than at runtime).

Five body variants cover the main use cases: a `theory` body defines a single GAT from scratch; a `morphism` body defines a mapping between two theories; a `compose` body builds a new theory by combining others via colimit (sharing their common parts); a `protocol` body bundles a schema theory and instance theory into a full protocol; and a `bundle` body puts multiple theories, morphisms, and protocols into one file.

## Quick example

```rust,ignore
use panproto_theory_dsl::{load, compile, builtin_resolver};

let doc = load(std::path::Path::new("theories/my_format.json"))?;
let resolver = builtin_resolver();
let compiled = compile(&doc, &resolver)?;
// compiled.theories contains the resulting Theory objects.
// compiled.protocols contains any Protocol objects.
```

A minimal JSON spec for a theory looks like:

```json
{
  "id": "com.example.event",
  "theory": "ThEvent",
  "sorts": [
    { "name": "Event" },
    { "name": "Actor" }
  ],
  "ops": [
    { "name": "agent", "input": "Event", "output": "Actor" }
  ]
}
```

## API overview

| Export | What it does |
|--------|-------------|
| `load` | Load a `.ncl`, `.json`, `.yaml`, or `.yml` file into a `TheoryDocument` |
| `load_dir` | Load all spec files from a directory, returning successes and per-file errors separately |
| `compile` | Compile a `TheoryDocument` to a `CompiledTheorySet` via a resolver callback |
| `load_and_compile` | Load and compile in one call |
| `compile_bundle` | Compile a `BundleSpec` in dependency order |
| `builtin_resolver` | Resolver for panproto's 11 built-in theories (ThGraph, ThConstraint, etc.) |
| `TheoryDocument` | Deserialized spec with `id`, `description`, and one body variant |
| `TheoryBody` | The body: `theory`, `morphism`, `compose`, `protocol`, or `bundle` |
| `CompiledTheorySet` | Output: theories, morphisms, protocols, and composition specs |
| `TheoryDslError` | Diagnostics for eval failures, term parse errors, morphism validation |
| `LoadDirResult` | Holds both successfully loaded documents and per-file errors |

## License

[MIT](../../LICENSE)
