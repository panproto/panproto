//! LLVM IR protocol definition.
//!
//! Defines the GAT theory and protocol for representing LLVM IR modules
//! as panproto schemas. LLVM IR is modeled with vertex kinds for the
//! major structural elements (Module, Function, `BasicBlock`, Instruction,
//! Type, Value) and edge kinds for structural relationships (containment,
//! data flow, control flow, typing).
//!
//! ## Theory composition
//!
//! ```text
//! ThLLVMIR = colimit(
//!     ThGraph,         // Vertex + Edge + src/tgt
//!     ThConstraint,    // Constraint sorts for opcodes, linkage, etc.
//!     ThOrder,         // Ordered instructions within basic blocks
//!     shared = Vertex ∪ Edge
//! )
//! ```

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol};

/// Returns the LLVM IR protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "llvm_ir".into(),
        schema_theory: "ThLLVMIRSchema".into(),
        instance_theory: "ThLLVMIRInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vertex_kinds(),
        constraint_sorts: constraint_sorts(),
        schema_composition: None,
        instance_composition: None,
        has_order: true,
        has_coproducts: false,
        has_recursion: false,
        has_causal: false,
        nominal_identity: false,
        has_defaults: false,
        has_coercions: false,
        has_mergers: false,
        has_policies: false,
    }
}

/// Register the LLVM IR theory pair.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    panproto_protocols::theories::register_constrained_multigraph_wtype(
        registry,
        "ThLLVMIRSchema",
        "ThLLVMIRInstance",
    );
}

/// LLVM IR vertex kinds.
fn vertex_kinds() -> Vec<String> {
    vec![
        // Module-level
        "module".into(),
        "function".into(),
        "global-variable".into(),
        "alias".into(),
        "comdat".into(),
        "metadata-node".into(),
        "named-metadata".into(),
        "attribute-group".into(),
        // Function-level
        "basic-block".into(),
        "parameter".into(),
        // Instruction categories
        "instruction".into(),
        // Type system
        "void-type".into(),
        "integer-type".into(),
        "float-type".into(),
        "pointer-type".into(),
        "array-type".into(),
        "vector-type".into(),
        "struct-type".into(),
        "function-type".into(),
        "label-type".into(),
        "metadata-type".into(),
        "token-type".into(),
        // Values
        "constant".into(),
        "undef".into(),
        "poison".into(),
        "null".into(),
        "zero-initializer".into(),
        "constant-array".into(),
        "constant-struct".into(),
        "constant-vector".into(),
        "constant-expr".into(),
        "block-address".into(),
        "inline-asm".into(),
    ]
}

/// LLVM IR constraint sorts.
fn constraint_sorts() -> Vec<String> {
    vec![
        // Instruction opcodes (stored as constraints on instruction vertices)
        "opcode".into(),
        // Linkage types
        "linkage".into(),
        // Visibility
        "visibility".into(),
        // Calling conventions
        "calling-convention".into(),
        // Parameter attributes
        "param-attribute".into(),
        // Function attributes
        "function-attribute".into(),
        // Return attributes
        "return-attribute".into(),
        // Alignment
        "alignment".into(),
        // Address space
        "address-space".into(),
        // Integer bit width
        "bit-width".into(),
        // Float type kind (half, float, double, fp128, etc.)
        "float-kind".into(),
        // Struct name
        "struct-name".into(),
        // Array/vector element count
        "element-count".into(),
        // Constant value
        "constant-value".into(),
        // Global variable initializer
        "is-constant".into(),
        // Thread local mode
        "thread-local-mode".into(),
        // Unnamed address
        "unnamed-addr".into(),
        // Fast math flags
        "fast-math-flags".into(),
        // Metadata kind
        "metadata-kind".into(),
        // Basic block name/label
        "block-label".into(),
        // SSA name (e.g. %1, %foo)
        "ssa-name".into(),
        // Comparison predicate for icmp/fcmp
        "predicate".into(),
        // Atomic ordering
        "ordering".into(),
    ]
}

/// LLVM IR edge rules.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        // Containment
        EdgeRule {
            edge_kind: "contains-function".into(),
            src_kinds: vec!["module".into()],
            tgt_kinds: vec!["function".into()],
        },
        EdgeRule {
            edge_kind: "contains-global".into(),
            src_kinds: vec!["module".into()],
            tgt_kinds: vec!["global-variable".into()],
        },
        EdgeRule {
            edge_kind: "contains-block".into(),
            src_kinds: vec!["function".into()],
            tgt_kinds: vec!["basic-block".into()],
        },
        EdgeRule {
            edge_kind: "contains-instruction".into(),
            src_kinds: vec!["basic-block".into()],
            tgt_kinds: vec!["instruction".into()],
        },
        EdgeRule {
            edge_kind: "contains-parameter".into(),
            src_kinds: vec!["function".into()],
            tgt_kinds: vec!["parameter".into()],
        },
        // Data flow (SSA use-def chains)
        EdgeRule {
            edge_kind: "operand".into(),
            src_kinds: vec!["instruction".into()],
            tgt_kinds: vec![], // Any value-producing vertex
        },
        // Control flow
        EdgeRule {
            edge_kind: "successor".into(),
            src_kinds: vec!["basic-block".into()],
            tgt_kinds: vec!["basic-block".into()],
        },
        EdgeRule {
            edge_kind: "entry-block".into(),
            src_kinds: vec!["function".into()],
            tgt_kinds: vec!["basic-block".into()],
        },
        // Typing
        EdgeRule {
            edge_kind: "type-of".into(),
            src_kinds: vec![], // Any value
            tgt_kinds: vec![], // Any type vertex
        },
        EdgeRule {
            edge_kind: "element-type".into(),
            src_kinds: vec![
                "array-type".into(),
                "vector-type".into(),
                "pointer-type".into(),
            ],
            tgt_kinds: vec![], // Any type
        },
        EdgeRule {
            edge_kind: "return-type".into(),
            src_kinds: vec!["function-type".into()],
            tgt_kinds: vec![], // Any type
        },
        EdgeRule {
            edge_kind: "param-type".into(),
            src_kinds: vec!["function-type".into()],
            tgt_kinds: vec![], // Any type (ordered)
        },
        EdgeRule {
            edge_kind: "struct-field-type".into(),
            src_kinds: vec!["struct-type".into()],
            tgt_kinds: vec![], // Any type (ordered)
        },
    ]
}

/// All LLVM IR instruction opcodes.
#[must_use]
pub fn instruction_opcodes() -> Vec<&'static str> {
    vec![
        // Terminator instructions
        "ret",
        "br",
        "switch",
        "indirectbr",
        "invoke",
        "resume",
        "unreachable",
        "cleanupret",
        "catchret",
        "catchswitch",
        "callbr",
        // Unary operators
        "fneg",
        // Binary operators
        "add",
        "fadd",
        "sub",
        "fsub",
        "mul",
        "fmul",
        "udiv",
        "sdiv",
        "fdiv",
        "urem",
        "srem",
        "frem",
        // Bitwise binary operators
        "shl",
        "lshr",
        "ashr",
        "and",
        "or",
        "xor",
        // Memory instructions
        "alloca",
        "load",
        "store",
        "fence",
        "cmpxchg",
        "atomicrmw",
        "getelementptr",
        // Conversion instructions
        "trunc",
        "zext",
        "sext",
        "fptrunc",
        "fpext",
        "fptoui",
        "fptosi",
        "uitofp",
        "sitofp",
        "ptrtoint",
        "inttoptr",
        "bitcast",
        "addrspacecast",
        // Other instructions
        "icmp",
        "fcmp",
        "phi",
        "select",
        "freeze",
        "call",
        "va_arg",
        "landingpad",
        "catchpad",
        "cleanuppad",
        // Aggregate instructions
        "extractvalue",
        "insertvalue",
        // Vector instructions
        "extractelement",
        "insertelement",
        "shufflevector",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let proto = protocol();
        assert_eq!(proto.name, "llvm_ir");
        assert!(proto.obj_kinds.len() > 20, "expected 20+ vertex kinds");
        assert!(proto.edge_rules.len() > 10, "expected 10+ edge rules");
        assert!(
            proto.constraint_sorts.len() > 15,
            "expected 15+ constraint sorts"
        );
        assert!(proto.has_order);
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThLLVMIRSchema"));
        assert!(registry.contains_key("ThLLVMIRInstance"));
    }

    #[test]
    fn instruction_opcode_coverage() {
        let opcodes = instruction_opcodes();
        // LLVM has ~60 instruction opcodes.
        assert!(
            opcodes.len() > 50,
            "expected 50+ opcodes, got {}",
            opcodes.len()
        );
        assert!(opcodes.contains(&"add"));
        assert!(opcodes.contains(&"ret"));
        assert!(opcodes.contains(&"phi"));
        assert!(opcodes.contains(&"getelementptr"));
        assert!(opcodes.contains(&"call"));
    }
}
