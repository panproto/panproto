# panproto-llvm

LLVM IR protocol definition and lowering morphisms for panproto.

## Overview

This crate provides:

1. **LLVM IR protocol**: A GAT theory and protocol definition for representing LLVM IR modules as panproto schemas. 31 vertex kinds cover modules, functions, basic blocks, instructions, types, and values. 13 edge rules cover containment, data flow (SSA use-def chains), control flow (successors), and typing. 56 instruction opcodes are enumerated as constraint sorts.

2. **Lowering morphisms**: Theory morphisms from language AST protocols (TypeScript, Python, Rust) to LLVM IR. These express compilation as structure-preserving maps, enabling cross-level migration via functoriality: `restrict(lower) . restrict(mig) = restrict(lower . mig)`.

3. **inkwell backend** (feature-gated): LLVM IR parsing from `.ll` text files via `parse_llvm_ir()`. Walks the module's functions, basic blocks, instructions, parameters, and global variables into panproto vertices and edges.

## Category theory

Lowering morphisms are theory morphisms in the GAT system. The complement of the lowering captures source-level information lost in IR (variable names, formatting, comments). LLVM optimization passes (DCE, inlining) are protolenses on the LLVM IR theory.

## Features

- `inkwell-backend` (default): enables LLVM IR parsing via inkwell. Requires LLVM 20 installed.

## License

MIT
