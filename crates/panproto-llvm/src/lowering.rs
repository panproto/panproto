//! Theory morphisms from language AST protocols to LLVM IR.
//!
//! Compilation is a theory morphism: a structure-preserving map from
//! a language's AST theory to the LLVM IR theory. These morphisms
//! enable cross-level migration (change a TypeScript type, and the
//! LLVM IR migration is automatically derived via functoriality).
//!
//! Each language lowering defines a [`TheoryMorphism`] mapping AST sorts
//! and operations to LLVM IR sorts and operations.

use std::sync::Arc;

use panproto_gat::TheoryMorphism;
use rustc_hash::FxHashMap;

/// Create a theory morphism lowering TypeScript AST to LLVM IR.
///
/// Maps TypeScript's function/expression/control flow structure to
/// LLVM IR's function/basic block/instruction structure.
#[must_use]
pub fn lower_typescript() -> TheoryMorphism {
    let mut sort_map = FxHashMap::default();
    let mut op_map = FxHashMap::default();

    // Sort mappings: TypeScript AST sorts → LLVM IR sorts.
    // Structural sorts are preserved (Vertex→Vertex, Edge→Edge).
    sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
    sort_map.insert(Arc::from("Edge"), Arc::from("Edge"));

    // TypeScript function-level → LLVM function-level.
    sort_map.insert(Arc::from("function_declaration"), Arc::from("function"));
    sort_map.insert(Arc::from("method_definition"), Arc::from("function"));
    sort_map.insert(Arc::from("arrow_function"), Arc::from("function"));
    sort_map.insert(Arc::from("formal_parameters"), Arc::from("parameter"));
    sort_map.insert(Arc::from("required_parameter"), Arc::from("parameter"));
    sort_map.insert(Arc::from("optional_parameter"), Arc::from("parameter"));

    // TypeScript blocks → LLVM basic blocks.
    sort_map.insert(Arc::from("statement_block"), Arc::from("basic-block"));

    // TypeScript expressions → LLVM instructions.
    sort_map.insert(Arc::from("binary_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("call_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("member_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("assignment_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("new_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("unary_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("update_expression"), Arc::from("instruction"));

    // TypeScript control flow → LLVM terminators.
    sort_map.insert(Arc::from("if_statement"), Arc::from("instruction"));
    sort_map.insert(Arc::from("return_statement"), Arc::from("instruction"));
    sort_map.insert(Arc::from("for_statement"), Arc::from("basic-block"));
    sort_map.insert(Arc::from("while_statement"), Arc::from("basic-block"));
    sort_map.insert(Arc::from("switch_statement"), Arc::from("instruction"));

    // TypeScript literals → LLVM constants.
    sort_map.insert(Arc::from("number"), Arc::from("constant"));
    sort_map.insert(Arc::from("string"), Arc::from("constant"));
    sort_map.insert(Arc::from("true"), Arc::from("constant"));
    sort_map.insert(Arc::from("false"), Arc::from("constant"));

    // TypeScript types → LLVM types.
    sort_map.insert(Arc::from("type_annotation"), Arc::from("function-type"));

    // Operation mappings: AST edge kinds → LLVM edge kinds.
    op_map.insert(Arc::from("body"), Arc::from("entry-block"));
    op_map.insert(Arc::from("parameters"), Arc::from("contains-parameter"));
    op_map.insert(Arc::from("return_type"), Arc::from("return-type"));
    op_map.insert(Arc::from("left"), Arc::from("operand"));
    op_map.insert(Arc::from("right"), Arc::from("operand"));
    op_map.insert(Arc::from("function"), Arc::from("operand"));
    op_map.insert(Arc::from("arguments"), Arc::from("operand"));
    op_map.insert(Arc::from("condition"), Arc::from("operand"));
    op_map.insert(Arc::from("consequence"), Arc::from("successor"));
    op_map.insert(Arc::from("alternative"), Arc::from("successor"));
    op_map.insert(Arc::from("value"), Arc::from("operand"));

    TheoryMorphism {
        name: Arc::from("lower_typescript_to_llvm"),
        domain: Arc::from("ThTypeScriptFullAST"),
        codomain: Arc::from("ThLLVMIRSchema"),
        sort_map: sort_map.into_iter().collect(),
        op_map: op_map.into_iter().collect(),
    }
}

/// Create a theory morphism lowering Python AST to LLVM IR.
#[must_use]
pub fn lower_python() -> TheoryMorphism {
    let mut sort_map = FxHashMap::default();
    let mut op_map = FxHashMap::default();

    sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
    sort_map.insert(Arc::from("Edge"), Arc::from("Edge"));

    sort_map.insert(Arc::from("function_definition"), Arc::from("function"));
    sort_map.insert(Arc::from("class_definition"), Arc::from("struct-type"));
    sort_map.insert(Arc::from("parameters"), Arc::from("parameter"));
    sort_map.insert(Arc::from("block"), Arc::from("basic-block"));

    sort_map.insert(Arc::from("binary_operator"), Arc::from("instruction"));
    sort_map.insert(Arc::from("call"), Arc::from("instruction"));
    sort_map.insert(Arc::from("assignment"), Arc::from("instruction"));
    sort_map.insert(Arc::from("return_statement"), Arc::from("instruction"));
    sort_map.insert(Arc::from("if_statement"), Arc::from("instruction"));
    sort_map.insert(Arc::from("for_statement"), Arc::from("basic-block"));
    sort_map.insert(Arc::from("while_statement"), Arc::from("basic-block"));

    sort_map.insert(Arc::from("integer"), Arc::from("constant"));
    sort_map.insert(Arc::from("float"), Arc::from("constant"));
    sort_map.insert(Arc::from("string"), Arc::from("constant"));
    sort_map.insert(Arc::from("true"), Arc::from("constant"));
    sort_map.insert(Arc::from("false"), Arc::from("constant"));

    op_map.insert(Arc::from("body"), Arc::from("entry-block"));
    op_map.insert(Arc::from("parameters"), Arc::from("contains-parameter"));
    op_map.insert(Arc::from("return_type"), Arc::from("return-type"));
    op_map.insert(Arc::from("left"), Arc::from("operand"));
    op_map.insert(Arc::from("right"), Arc::from("operand"));
    op_map.insert(Arc::from("function"), Arc::from("operand"));
    op_map.insert(Arc::from("arguments"), Arc::from("operand"));
    op_map.insert(Arc::from("condition"), Arc::from("operand"));
    op_map.insert(Arc::from("consequence"), Arc::from("successor"));
    op_map.insert(Arc::from("alternative"), Arc::from("successor"));

    TheoryMorphism {
        name: Arc::from("lower_python_to_llvm"),
        domain: Arc::from("ThPythonFullAST"),
        codomain: Arc::from("ThLLVMIRSchema"),
        sort_map: sort_map.into_iter().collect(),
        op_map: op_map.into_iter().collect(),
    }
}

/// Create a theory morphism lowering Rust AST to LLVM IR.
#[must_use]
pub fn lower_rust() -> TheoryMorphism {
    let mut sort_map = FxHashMap::default();
    let mut op_map = FxHashMap::default();

    sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
    sort_map.insert(Arc::from("Edge"), Arc::from("Edge"));

    sort_map.insert(Arc::from("function_item"), Arc::from("function"));
    sort_map.insert(Arc::from("struct_item"), Arc::from("struct-type"));
    sort_map.insert(Arc::from("enum_item"), Arc::from("struct-type"));
    sort_map.insert(Arc::from("impl_item"), Arc::from("module"));
    sort_map.insert(Arc::from("trait_item"), Arc::from("module"));
    sort_map.insert(Arc::from("parameters"), Arc::from("parameter"));
    sort_map.insert(Arc::from("block"), Arc::from("basic-block"));

    sort_map.insert(Arc::from("binary_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("call_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("field_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("assignment_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("return_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("if_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("match_expression"), Arc::from("instruction"));
    sort_map.insert(Arc::from("for_expression"), Arc::from("basic-block"));
    sort_map.insert(Arc::from("while_expression"), Arc::from("basic-block"));
    sort_map.insert(Arc::from("loop_expression"), Arc::from("basic-block"));

    sort_map.insert(Arc::from("integer_literal"), Arc::from("constant"));
    sort_map.insert(Arc::from("float_literal"), Arc::from("constant"));
    sort_map.insert(Arc::from("string_literal"), Arc::from("constant"));
    sort_map.insert(Arc::from("boolean_literal"), Arc::from("constant"));

    op_map.insert(Arc::from("body"), Arc::from("entry-block"));
    op_map.insert(Arc::from("parameters"), Arc::from("contains-parameter"));
    op_map.insert(Arc::from("return_type"), Arc::from("return-type"));
    op_map.insert(Arc::from("left"), Arc::from("operand"));
    op_map.insert(Arc::from("right"), Arc::from("operand"));
    op_map.insert(Arc::from("function"), Arc::from("operand"));
    op_map.insert(Arc::from("arguments"), Arc::from("operand"));
    op_map.insert(Arc::from("condition"), Arc::from("operand"));
    op_map.insert(Arc::from("consequence"), Arc::from("successor"));
    op_map.insert(Arc::from("alternative"), Arc::from("successor"));
    op_map.insert(Arc::from("value"), Arc::from("operand"));

    TheoryMorphism {
        name: Arc::from("lower_rust_to_llvm"),
        domain: Arc::from("ThRustFullAST"),
        codomain: Arc::from("ThLLVMIRSchema"),
        sort_map: sort_map.into_iter().collect(),
        op_map: op_map.into_iter().collect(),
    }
}

/// Get all registered lowering morphisms.
#[must_use]
pub fn all_lowering_morphisms() -> Vec<TheoryMorphism> {
    vec![lower_typescript(), lower_python(), lower_rust()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_lowering_maps_sorts() {
        let morph = lower_typescript();
        assert_eq!(morph.domain.as_ref(), "ThTypeScriptFullAST");
        assert_eq!(morph.codomain.as_ref(), "ThLLVMIRSchema");
        assert!(!morph.sort_map.is_empty());
        assert!(!morph.op_map.is_empty());

        // Function declarations map to LLVM functions.
        let func_target = morph.sort_map.get(&Arc::from("function_declaration"));
        assert_eq!(func_target, Some(&Arc::from("function")));

        // Binary expressions map to LLVM instructions.
        let binop_target = morph.sort_map.get(&Arc::from("binary_expression"));
        assert_eq!(binop_target, Some(&Arc::from("instruction")));
    }

    #[test]
    fn python_lowering_maps_sorts() {
        let morph = lower_python();
        assert_eq!(morph.domain.as_ref(), "ThPythonFullAST");
        let func_target = morph.sort_map.get(&Arc::from("function_definition"));
        assert_eq!(func_target, Some(&Arc::from("function")));
    }

    #[test]
    fn rust_lowering_maps_sorts() {
        let morph = lower_rust();
        assert_eq!(morph.domain.as_ref(), "ThRustFullAST");
        let func_target = morph.sort_map.get(&Arc::from("function_item"));
        assert_eq!(func_target, Some(&Arc::from("function")));
    }

    #[test]
    fn all_morphisms_registered() {
        let morphisms = all_lowering_morphisms();
        assert_eq!(morphisms.len(), 3);
    }
}
