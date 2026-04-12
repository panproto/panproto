# panproto-llvm

[![crates.io](https://img.shields.io/crates/v/panproto-llvm.svg)](https://crates.io/crates/panproto-llvm)
[![docs.rs](https://docs.rs/panproto-llvm/badge.svg)](https://docs.rs/panproto-llvm)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Models LLVM IR as a panproto schema language, and treats compilation as a schema transformation.

## What it does

LLVM IR is the intermediate representation that compilers like clang and rustc lower source code into before generating machine code. This crate defines it as a first-class panproto protocol: 31 vertex kinds (modules, functions, basic blocks, the ~56 instruction opcodes) and edge kinds for control flow, data flow (SSA use-def chains), containment, and typing.

The more interesting part is the lowering morphisms. A morphism in panproto is a structure-preserving map between two schema languages; think of it as declaring "every TypeScript function maps to an LLVM function, every branch maps to a conditional branch instruction, and so on." This crate provides those maps from the TypeScript, Python, and Rust AST protocols to LLVM IR. Because the maps preserve structure, schema migrations at the source level compose correctly with the lowering: you can migrate data schemas and have the compiled output stay consistent automatically.

Parsing actual LLVM bitcode (`.ll` and `.bc` files) into panproto schemas is available behind the `inkwell-backend` feature flag, which requires LLVM installed locally. Without that flag, the protocol definition and lowering morphisms are still available at compile time.

## Quick example

```rust,ignore
use panproto_llvm::{llvm_ir_protocol, all_lowering_morphisms};

// Get the LLVM IR protocol definition.
let proto = llvm_ir_protocol();
println!("vertex kinds: {}", proto.obj_kinds.len());

// Inspect available lowering morphisms (TypeScript -> LLVM, etc.).
for m in all_lowering_morphisms() {
    println!("lowers: {} -> llvm_ir", m.source_protocol);
}

// Parse actual LLVM IR (requires --features=inkwell-backend and LLVM installed).
#[cfg(feature = "inkwell-backend")]
{
    use panproto_llvm::parse_llvm_ir;
    let schema = parse_llvm_ir(std::path::Path::new("module.ll"))?;
    println!("{} instructions", schema.vertices.len());
}
```

## API overview

| Export | What it does |
|--------|-------------|
| `llvm_ir_protocol()` | Returns the LLVM IR protocol definition (31 vertex kinds, 56 opcodes) |
| `all_lowering_morphisms()` | Returns lowering morphisms from TypeScript, Python, Rust to LLVM IR |
| `parse_llvm_ir()` | Parses a `.ll` or `.bc` file into a panproto schema (`inkwell-backend` feature) |
| `LlvmError` | Error variants: parse failures, unsupported instruction kinds |

## License

[MIT](../../LICENSE)
