//! # panproto-llvm
//!
//! LLVM IR protocol definition and lowering morphisms for panproto.
//!
//! This crate provides:
//!
//! 1. **LLVM IR protocol**: A GAT theory and protocol definition for
//!    representing LLVM IR modules as panproto schemas. Vertex kinds cover
//!    modules, functions, basic blocks, instructions, types, and values.
//!    Edge kinds cover containment, data flow (SSA use-def chains),
//!    control flow (successors), and typing.
//!
//! 2. **Lowering morphisms**: Theory morphisms from language AST protocols
//!    (TypeScript, Python, Rust) to LLVM IR. These express compilation as
//!    structure-preserving maps, enabling cross-level migration via
//!    functoriality.
//!
//! 3. **inkwell backend** (optional, feature-gated): LLVM IR parsing from
//!    `.ll`/`.bc` files, emission, and C/C++ compilation via clang.
//!    Requires LLVM installed locally. Enable with `--features=inkwell-backend`.
//!
//! ## Category theory
//!
//! Lowering morphisms are theory morphisms in the GAT system. If
//! `lower: ThTypeScriptFullAST → ThLLVMIR` is the lowering morphism, then:
//!
//! - `restrict(lower) ∘ restrict(mig) = restrict(lower ∘ mig)` (functoriality)
//! - The complement of the lowering captures source-level info lost in IR
//! - LLVM passes (DCE, inlining) are protolenses on ThLLVMIR

/// Error types for LLVM IR operations.
pub mod error;

/// LLVM IR protocol definition (theory, vertex kinds, edge rules).
pub mod protocol;

/// Theory morphisms from language ASTs to LLVM IR.
pub mod lowering;

pub use error::LlvmError;
pub use lowering::all_lowering_morphisms;
pub use protocol::protocol as llvm_ir_protocol;
