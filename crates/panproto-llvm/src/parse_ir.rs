//! LLVM IR text (`.ll`) parsing into panproto schemas via inkwell.
//!
//! Uses inkwell's `Module::create_module_from_ir` to parse LLVM IR text
//! and then walks the module structure to emit panproto vertices and edges.

#[cfg(feature = "inkwell-backend")]
mod inner {
    use inkwell::context::Context;
    use inkwell::memory_buffer::MemoryBuffer;
    use inkwell::module::Module;
    use inkwell::values::FunctionValue;

    use panproto_schema::{Schema, SchemaBuilder};

    use crate::error::LlvmError;
    use crate::protocol;

    /// Parse LLVM IR text (`.ll` format) into a panproto [`Schema`].
    ///
    /// Uses inkwell to parse the IR, then walks the module's functions,
    /// basic blocks, and instructions to create vertices and edges.
    ///
    /// # Errors
    ///
    /// Returns [`LlvmError`] if IR parsing fails or schema construction fails.
    pub fn parse_llvm_ir(ir_text: &str, module_name: &str) -> Result<Schema, LlvmError> {
        let context = Context::create();
        let buffer = MemoryBuffer::create_from_memory_range_copy(ir_text.as_bytes(), module_name);
        let module = context
            .create_module_from_ir(buffer)
            .map_err(|e| LlvmError::ParseFailed {
                reason: e.to_string(),
            })?;

        walk_module(&module, module_name)
    }

    /// Walk an LLVM module and produce a panproto schema.
    fn walk_module(module: &Module<'_>, module_name: &str) -> Result<Schema, LlvmError> {
        let proto = protocol::protocol();
        let mut builder = SchemaBuilder::new(&proto);

        // Root module vertex.
        builder = builder.vertex(module_name, "module", None)?;

        let triple = module.get_triple().to_string();
        if !triple.is_empty() {
            builder = builder.constraint(module_name, "target-triple", &triple);
        }

        // Walk functions.
        let mut func = module.get_first_function();
        while let Some(f) = func {
            builder = walk_function(f, module_name, builder)?;
            func = f.get_next_function();
        }

        // Walk global variables.
        let mut global = module.get_first_global();
        while let Some(g) = global {
            let name = g.get_name().to_str().unwrap_or("unnamed_global");
            let gv_id = format!("{module_name}::@{name}");
            builder = builder.vertex(&gv_id, "global-variable", None)?;
            builder = builder.edge(module_name, &gv_id, "contains-global", None)?;

            if g.is_constant() {
                builder = builder.constraint(&gv_id, "is-constant", "true");
            }

            global = g.get_next_global();
        }

        builder.build().map_err(LlvmError::from)
    }

    /// Walk a function and emit vertices for its basic blocks and instructions.
    fn walk_function(
        func: FunctionValue<'_>,
        module_name: &str,
        mut builder: SchemaBuilder,
    ) -> Result<SchemaBuilder, LlvmError> {
        let name = func.get_name().to_str().unwrap_or("unnamed_fn");
        let fn_id = format!("{module_name}::@{name}");

        builder = builder.vertex(&fn_id, "function", None)?;
        builder = builder.edge(module_name, &fn_id, "contains-function", None)?;

        // Linkage.
        let linkage = format!("{:?}", func.get_linkage());
        builder = builder.constraint(&fn_id, "linkage", &linkage);

        // Parameters.
        for (i, param) in func.get_params().iter().enumerate() {
            let param_id = format!("{fn_id}::param_{i}");
            builder = builder.vertex(&param_id, "parameter", None)?;
            builder = builder.edge(&fn_id, &param_id, "contains-parameter", None)?;
            let type_str = param.get_type().to_string();
            builder = builder.constraint(&param_id, "type-of", &type_str);
        }

        // Basic blocks.
        let mut is_entry = true;
        for bb in func.get_basic_blocks() {
            let bb_name = bb.get_name().to_str().unwrap_or("unnamed_bb");
            let bb_id = format!("{fn_id}::{bb_name}");

            builder = builder.vertex(&bb_id, "basic-block", None)?;
            builder = builder.edge(&fn_id, &bb_id, "contains-block", None)?;
            builder = builder.constraint(&bb_id, "block-label", bb_name);

            if is_entry {
                builder = builder.edge(&fn_id, &bb_id, "entry-block", None)?;
                is_entry = false;
            }

            // Instructions in this basic block.
            let mut inst_opt = bb.get_first_instruction();
            let mut inst_idx = 0;
            while let Some(inst) = inst_opt {
                let inst_id = format!("{bb_id}::i{inst_idx}");
                builder = builder.vertex(&inst_id, "instruction", None)?;
                builder = builder.edge(&bb_id, &inst_id, "contains-instruction", None)?;

                let opcode = format!("{:?}", inst.get_opcode());
                builder = builder.constraint(&inst_id, "opcode", &opcode);

                // SSA name if the instruction produces a value.
                if let Some(inst_name_ref) = inst.get_name() {
                    let inst_name = inst_name_ref.to_str().unwrap_or("");
                    if !inst_name.is_empty() {
                        builder = builder.constraint(&inst_id, "ssa-name", inst_name);
                    }
                }

                inst_opt = inst.get_next_instruction();
                inst_idx += 1;
            }
        }

        Ok(builder)
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod tests {
        use super::*;

        #[test]
        fn parse_simple_ir() {
            let ir = r"
define i32 @add(i32 %a, i32 %b) {
entry:
  %result = add i32 %a, %b
  ret i32 %result
}
";
            let schema = parse_llvm_ir(ir, "test_module").unwrap();
            assert!(!schema.vertices.is_empty(), "schema should have vertices");

            // Should have: module, function @add, 2 parameters, basic block entry, 2 instructions.
            assert!(
                schema.vertices.len() >= 6,
                "expected at least 6 vertices, got {}",
                schema.vertices.len()
            );
        }

        #[test]
        fn parse_multi_function_ir() {
            let ir = r"
define i32 @add(i32 %a, i32 %b) {
entry:
  %result = add i32 %a, %b
  ret i32 %result
}

define i32 @mul(i32 %a, i32 %b) {
entry:
  %result = mul i32 %a, %b
  ret i32 %result
}

@global_var = global i32 42
";
            let schema = parse_llvm_ir(ir, "multi_fn").unwrap();
            // 1 module + 2 functions + 4 params + 2 basic blocks + 4 instructions + 1 global = 14.
            assert!(
                schema.vertices.len() >= 14,
                "expected at least 14 vertices, got {}",
                schema.vertices.len()
            );
        }

        #[test]
        fn parse_branching_ir() {
            let ir = r"
define i32 @max(i32 %a, i32 %b) {
entry:
  %cmp = icmp sgt i32 %a, %b
  br i1 %cmp, label %then, label %else

then:
  ret i32 %a

else:
  ret i32 %b
}
";
            let schema = parse_llvm_ir(ir, "branch_test").unwrap();
            // 1 module + 1 function + 2 params + 3 basic blocks + 4 instructions = 11.
            assert!(
                schema.vertices.len() >= 11,
                "expected at least 11 vertices, got {}",
                schema.vertices.len()
            );
        }
    }
}

#[cfg(feature = "inkwell-backend")]
pub use inner::parse_llvm_ir;
