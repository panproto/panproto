//! Compilation mapping from panproto expressions to LLVM IR constructs.
//!
//! Defines the correspondence between `panproto_expr::Expr` variants and
//! the LLVM IR constructs they compile to. This mapping is used by the
//! inkwell-based JIT backend (when enabled) and serves as documentation
//! for the compilation semantics.

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};

/// Classification of how an `Expr` maps to LLVM IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprMapping {
    /// Maps to an LLVM constant value.
    Constant {
        /// The LLVM type (i64, f64, ptr, etc.)
        llvm_type: LlvmType,
    },
    /// Maps to a load from the environment/closure struct.
    EnvLoad {
        /// Variable name being loaded.
        var_name: Arc<str>,
    },
    /// Maps to a direct or indirect function call.
    FunctionCall {
        /// Whether this is a direct call (known target) or indirect (via closure).
        is_indirect: bool,
    },
    /// Maps to a closure struct (captures + function pointer).
    Closure {
        /// Number of captured variables.
        capture_count: usize,
    },
    /// Maps to alloca + store + body evaluation.
    LetBinding,
    /// Maps to a switch or cascading branch.
    PatternMatch {
        /// Number of match arms.
        arm_count: usize,
    },
    /// Maps to an LLVM arithmetic instruction.
    ArithmeticOp {
        /// The LLVM instruction name (add, sub, mul, etc.)
        instruction: &'static str,
    },
    /// Maps to a call to a runtime support function.
    RuntimeCall {
        /// The runtime function name.
        function: &'static str,
    },
    /// Maps to a loop over an array.
    ArrayLoop {
        /// The operation being applied per element (map, filter, fold).
        operation: &'static str,
    },
    /// Maps to struct construction or GEP.
    StructOp {
        /// Whether this is construction or field access.
        is_construction: bool,
    },
    /// Maps to array allocation + element stores.
    ArrayConstruction,
}

/// LLVM type classification for the JIT value representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlvmType {
    /// 64-bit integer.
    I64,
    /// 64-bit floating point.
    F64,
    /// 1-bit integer (boolean).
    I1,
    /// Pointer to heap-allocated data (strings, lists, records).
    Ptr,
    /// Tagged union (the universal JIT value type).
    TaggedUnion,
}

/// Classify how an expression would compile to LLVM IR.
///
/// This is a static analysis pass that determines the compilation strategy
/// for each expression node without actually generating LLVM IR.
#[must_use]
pub fn classify_expr(expr: &Expr) -> ExprMapping {
    match expr {
        Expr::Lit(lit) => {
            let llvm_type = classify_literal_type(lit);
            ExprMapping::Constant { llvm_type }
        }
        Expr::Var(name) => ExprMapping::EnvLoad {
            var_name: name.clone(),
        },
        Expr::App(_, _) => ExprMapping::FunctionCall { is_indirect: false },
        Expr::Lam(_, body) => {
            let captures = count_free_vars(body);
            ExprMapping::Closure {
                capture_count: captures,
            }
        }
        Expr::Record(_) => ExprMapping::StructOp {
            is_construction: true,
        },
        Expr::List(_) => ExprMapping::ArrayConstruction,
        Expr::Field(_, _) => ExprMapping::StructOp {
            is_construction: false,
        },
        Expr::Index(_, _) => ExprMapping::RuntimeCall {
            function: "panproto_rt_index",
        },
        Expr::Match { arms, .. } => ExprMapping::PatternMatch {
            arm_count: arms.len(),
        },
        Expr::Let { .. } => ExprMapping::LetBinding,
        Expr::Builtin(op, _) => classify_builtin(*op),
    }
}

/// Classify the LLVM type for a literal.
fn classify_literal_type(lit: &Literal) -> LlvmType {
    match lit {
        Literal::Int(_) => LlvmType::I64,
        Literal::Float(_) => LlvmType::F64,
        Literal::Bool(_) => LlvmType::I1,
        Literal::Str(_) => LlvmType::Ptr,
        Literal::Bytes(_) => LlvmType::Ptr,
        Literal::Null => LlvmType::Ptr,
        Literal::Record(_) => LlvmType::Ptr,
        Literal::List(_) => LlvmType::Ptr,
        Literal::Closure { .. } => LlvmType::Ptr,
    }
}

/// Classify a builtin operation.
fn classify_builtin(op: BuiltinOp) -> ExprMapping {
    match op {
        // Arithmetic (direct LLVM instructions).
        BuiltinOp::Add => ExprMapping::ArithmeticOp { instruction: "add" },
        BuiltinOp::Sub => ExprMapping::ArithmeticOp { instruction: "sub" },
        BuiltinOp::Mul => ExprMapping::ArithmeticOp { instruction: "mul" },
        BuiltinOp::Div => ExprMapping::ArithmeticOp { instruction: "sdiv" },
        BuiltinOp::Mod => ExprMapping::ArithmeticOp { instruction: "srem" },
        BuiltinOp::Neg => ExprMapping::ArithmeticOp { instruction: "neg" },
        BuiltinOp::Abs => ExprMapping::RuntimeCall { function: "panproto_rt_abs" },

        // Rounding.
        BuiltinOp::Floor => ExprMapping::RuntimeCall { function: "floor" },
        BuiltinOp::Ceil => ExprMapping::RuntimeCall { function: "ceil" },

        // Comparison (icmp/fcmp instructions).
        BuiltinOp::Eq => ExprMapping::ArithmeticOp { instruction: "icmp eq" },
        BuiltinOp::Neq => ExprMapping::ArithmeticOp { instruction: "icmp ne" },
        BuiltinOp::Lt => ExprMapping::ArithmeticOp { instruction: "icmp slt" },
        BuiltinOp::Lte => ExprMapping::ArithmeticOp { instruction: "icmp sle" },
        BuiltinOp::Gt => ExprMapping::ArithmeticOp { instruction: "icmp sgt" },
        BuiltinOp::Gte => ExprMapping::ArithmeticOp { instruction: "icmp sge" },

        // Boolean (LLVM and/or/xor).
        BuiltinOp::And => ExprMapping::ArithmeticOp { instruction: "and" },
        BuiltinOp::Or => ExprMapping::ArithmeticOp { instruction: "or" },
        BuiltinOp::Not => ExprMapping::ArithmeticOp { instruction: "xor" },

        // String operations (runtime calls).
        BuiltinOp::Concat => ExprMapping::RuntimeCall { function: "panproto_rt_str_concat" },
        BuiltinOp::Len => ExprMapping::RuntimeCall { function: "panproto_rt_str_len" },
        BuiltinOp::Slice => ExprMapping::RuntimeCall { function: "panproto_rt_str_slice" },
        BuiltinOp::Upper => ExprMapping::RuntimeCall { function: "panproto_rt_str_upper" },
        BuiltinOp::Lower => ExprMapping::RuntimeCall { function: "panproto_rt_str_lower" },
        BuiltinOp::Trim => ExprMapping::RuntimeCall { function: "panproto_rt_str_trim" },
        BuiltinOp::Split => ExprMapping::RuntimeCall { function: "panproto_rt_str_split" },
        BuiltinOp::Join => ExprMapping::RuntimeCall { function: "panproto_rt_str_join" },
        BuiltinOp::Replace => ExprMapping::RuntimeCall { function: "panproto_rt_str_replace" },
        BuiltinOp::Contains => ExprMapping::RuntimeCall { function: "panproto_rt_str_contains" },

        // List operations (array loops).
        BuiltinOp::Map => ExprMapping::ArrayLoop { operation: "map" },
        BuiltinOp::Filter => ExprMapping::ArrayLoop { operation: "filter" },
        BuiltinOp::Fold => ExprMapping::ArrayLoop { operation: "fold" },
        BuiltinOp::FlatMap => ExprMapping::ArrayLoop { operation: "flat_map" },
        BuiltinOp::Head => ExprMapping::RuntimeCall { function: "panproto_rt_head" },
        BuiltinOp::Tail => ExprMapping::RuntimeCall { function: "panproto_rt_tail" },
        BuiltinOp::Reverse => ExprMapping::RuntimeCall { function: "panproto_rt_reverse" },
        BuiltinOp::Append => ExprMapping::RuntimeCall { function: "panproto_rt_append" },
        BuiltinOp::Length => ExprMapping::RuntimeCall { function: "panproto_rt_length" },

        // Record operations.
        BuiltinOp::MergeRecords => ExprMapping::RuntimeCall { function: "panproto_rt_record_merge" },
        BuiltinOp::Keys => ExprMapping::RuntimeCall { function: "panproto_rt_keys" },
        BuiltinOp::Values => ExprMapping::RuntimeCall { function: "panproto_rt_values" },
        BuiltinOp::HasField => ExprMapping::RuntimeCall { function: "panproto_rt_has_field" },

        // Type coercions.
        BuiltinOp::IntToFloat => ExprMapping::ArithmeticOp { instruction: "sitofp" },
        BuiltinOp::FloatToInt => ExprMapping::ArithmeticOp { instruction: "fptosi" },
        BuiltinOp::IntToStr => ExprMapping::RuntimeCall { function: "panproto_rt_int_to_str" },
        BuiltinOp::StrToInt => ExprMapping::RuntimeCall { function: "panproto_rt_str_to_int" },
        BuiltinOp::FloatToStr => ExprMapping::RuntimeCall { function: "panproto_rt_float_to_str" },
        BuiltinOp::StrToFloat => ExprMapping::RuntimeCall { function: "panproto_rt_str_to_float" },

        // Type inspection.
        BuiltinOp::TypeOf => ExprMapping::RuntimeCall { function: "panproto_rt_type_of" },
        BuiltinOp::IsNull => ExprMapping::RuntimeCall { function: "panproto_rt_is_null" },
        BuiltinOp::IsList => ExprMapping::RuntimeCall { function: "panproto_rt_is_list" },

        // Graph traversal.
        BuiltinOp::Edge => ExprMapping::RuntimeCall { function: "panproto_rt_edge" },
        BuiltinOp::Children => ExprMapping::RuntimeCall { function: "panproto_rt_children" },
        BuiltinOp::HasEdge => ExprMapping::RuntimeCall { function: "panproto_rt_has_edge" },
        BuiltinOp::EdgeCount => ExprMapping::RuntimeCall { function: "panproto_rt_edge_count" },
        BuiltinOp::Anchor => ExprMapping::RuntimeCall { function: "panproto_rt_anchor" },
    }
}

/// Count the approximate number of free variables in an expression.
fn count_free_vars(expr: &Expr) -> usize {
    let mut vars = std::collections::HashSet::new();
    collect_vars(expr, &mut vars);
    vars.len()
}

/// Collect all variable references in an expression.
fn collect_vars(expr: &Expr, vars: &mut std::collections::HashSet<String>) {
    match expr {
        Expr::Var(name) => {
            vars.insert(name.to_string());
        }
        Expr::Lit(_) => {}
        Expr::App(f, arg) => {
            collect_vars(f, vars);
            collect_vars(arg, vars);
        }
        Expr::Lam(_, body) => {
            collect_vars(body, vars);
        }
        Expr::Record(fields) => {
            for (_, val) in fields {
                collect_vars(val, vars);
            }
        }
        Expr::List(elems) => {
            for elem in elems {
                collect_vars(elem, vars);
            }
        }
        Expr::Field(base, _) => {
            collect_vars(base, vars);
        }
        Expr::Index(base, idx) => {
            collect_vars(base, vars);
            collect_vars(idx, vars);
        }
        Expr::Match { scrutinee, arms } => {
            collect_vars(scrutinee, vars);
            for (_, body) in arms {
                collect_vars(body, vars);
            }
        }
        Expr::Let { value, body, .. } => {
            collect_vars(value, vars);
            collect_vars(body, vars);
        }
        Expr::Builtin(_, args) => {
            for arg in args {
                collect_vars(arg, vars);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_int_literal() {
        let expr = Expr::Lit(Literal::Int(42));
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::Constant { llvm_type: LlvmType::I64 }
        );
    }

    #[test]
    fn classify_string_literal() {
        let expr = Expr::Lit(Literal::Str("hello".into()));
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::Constant { llvm_type: LlvmType::Ptr }
        );
    }

    #[test]
    fn classify_add() {
        let expr = Expr::Builtin(BuiltinOp::Add, vec![
            Expr::Lit(Literal::Int(1)),
            Expr::Lit(Literal::Int(2)),
        ]);
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::ArithmeticOp { instruction: "add" }
        );
    }

    #[test]
    fn classify_string_concat() {
        let expr = Expr::Builtin(BuiltinOp::Concat, vec![
            Expr::Lit(Literal::Str("a".into())),
            Expr::Lit(Literal::Str("b".into())),
        ]);
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::RuntimeCall { function: "panproto_rt_str_concat" }
        );
    }

    #[test]
    fn classify_map_loop() {
        let expr = Expr::Builtin(BuiltinOp::Map, vec![
            Expr::Lam("x".into(), Box::new(Expr::Var("x".into()))),
            Expr::List(vec![Expr::Lit(Literal::Int(1))]),
        ]);
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::ArrayLoop { operation: "map" }
        );
    }

    #[test]
    fn classify_pattern_match() {
        let expr = Expr::Match {
            scrutinee: Box::new(Expr::Var("x".into())),
            arms: vec![
                (panproto_expr::Pattern::Lit(Literal::Int(0)), Expr::Lit(Literal::Str("zero".into()))),
                (panproto_expr::Pattern::Wildcard, Expr::Lit(Literal::Str("other".into()))),
            ],
        };
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::PatternMatch { arm_count: 2 }
        );
    }

    #[test]
    fn classify_all_builtins() {
        // Verify every builtin has a classification (no panics).
        let builtins = vec![
            BuiltinOp::Add, BuiltinOp::Sub, BuiltinOp::Mul, BuiltinOp::Div, BuiltinOp::Mod,
            BuiltinOp::Neg, BuiltinOp::Abs,
            BuiltinOp::Floor, BuiltinOp::Ceil,
            BuiltinOp::Eq, BuiltinOp::Neq, BuiltinOp::Lt, BuiltinOp::Lte, BuiltinOp::Gt, BuiltinOp::Gte,
            BuiltinOp::And, BuiltinOp::Or, BuiltinOp::Not,
            BuiltinOp::Concat, BuiltinOp::Len, BuiltinOp::Slice, BuiltinOp::Upper, BuiltinOp::Lower,
            BuiltinOp::Trim, BuiltinOp::Split, BuiltinOp::Join, BuiltinOp::Replace, BuiltinOp::Contains,
            BuiltinOp::Map, BuiltinOp::Filter, BuiltinOp::Fold, BuiltinOp::FlatMap,
            BuiltinOp::Head, BuiltinOp::Tail, BuiltinOp::Reverse, BuiltinOp::Append, BuiltinOp::Length,
            BuiltinOp::MergeRecords, BuiltinOp::Keys, BuiltinOp::Values, BuiltinOp::HasField,
            BuiltinOp::IntToFloat, BuiltinOp::FloatToInt,
            BuiltinOp::IntToStr, BuiltinOp::StrToInt, BuiltinOp::FloatToStr, BuiltinOp::StrToFloat,
            BuiltinOp::TypeOf, BuiltinOp::IsNull, BuiltinOp::IsList,
            BuiltinOp::Edge, BuiltinOp::Children, BuiltinOp::HasEdge, BuiltinOp::EdgeCount, BuiltinOp::Anchor,
        ];

        for op in builtins {
            let expr = Expr::Builtin(op, vec![]);
            let _ = classify_expr(&expr);
        }
    }

    #[test]
    fn classify_lambda_captures() {
        let expr = Expr::Lam(
            "x".into(),
            Box::new(Expr::App(
                Box::new(Expr::Var("f".into())),
                Box::new(Expr::Var("x".into())),
            )),
        );
        let mapping = classify_expr(&expr);
        match mapping {
            ExprMapping::Closure { capture_count } => {
                // "f" and "x" are referenced.
                assert_eq!(capture_count, 2);
            }
            other => panic!("expected Closure, got {other:?}"),
        }
    }
}
