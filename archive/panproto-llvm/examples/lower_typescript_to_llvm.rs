//! Build the TypeScript → LLVM IR lowering morphism and print its shape.

fn main() {
    let proto = panproto_llvm::llvm_ir_protocol();
    println!(
        "LLVM IR protocol: {} object kinds, {} edge rules",
        proto.obj_kinds.len(),
        proto.edge_rules.len()
    );

    let opcodes = panproto_llvm::protocol::instruction_opcodes();
    println!("{} LLVM instruction opcodes", opcodes.len());

    let lowering = panproto_llvm::lowering::lower_typescript();
    println!(
        "TypeScript → LLVM morphism: {} sort mappings, {} op mappings",
        lowering.sort_map.len(),
        lowering.op_map.len()
    );
}
