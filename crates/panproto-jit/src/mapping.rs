//! Compilation mapping from panproto expressions to LLVM IR constructs.
//!
//! Defines the correspondence between `panproto_expr::Expr` variants and
//! the LLVM IR constructs they compile to. This mapping is used by the
//! inkwell-based JIT backend (when enabled) and serves as documentation
//! for the compilation semantics.

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};

/// Classification of how an `Expr` maps to LLVM IR.
#[non_exhaustive]
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
#[non_exhaustive]
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
        Expr::Lam(param, body) => {
            let mut free = std::collections::HashSet::new();
            let mut bound = std::collections::HashSet::new();
            bound.insert(param.to_string());
            collect_free_vars(body, &bound, &mut free);
            let captures = free.len();
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
const fn classify_literal_type(lit: &Literal) -> LlvmType {
    match lit {
        Literal::Int(_) => LlvmType::I64,
        Literal::Float(_) => LlvmType::F64,
        Literal::Bool(_) => LlvmType::I1,
        Literal::Str(_)
        | Literal::Bytes(_)
        | Literal::Null
        | Literal::Record(_)
        | Literal::List(_)
        | Literal::Closure { .. } => LlvmType::Ptr,
    }
}

/// Classify a builtin: dispatch to JIT-compiled or runtime-call classification.
fn classify_builtin(op: BuiltinOp) -> ExprMapping {
    classify_jittable_builtin(op).unwrap_or_else(|| classify_runtime_builtin(op))
}

/// Builtins that compile to LLVM instructions or array loops.
#[rustfmt::skip]
const fn classify_jittable_builtin(op: BuiltinOp) -> Option<ExprMapping> {
    match op {
        BuiltinOp::Add => Some(ExprMapping::ArithmeticOp { instruction: "add" }),
        BuiltinOp::Sub => Some(ExprMapping::ArithmeticOp { instruction: "sub" }),
        BuiltinOp::Mul => Some(ExprMapping::ArithmeticOp { instruction: "mul" }),
        BuiltinOp::Div => Some(ExprMapping::ArithmeticOp { instruction: "sdiv" }),
        BuiltinOp::Mod => Some(ExprMapping::ArithmeticOp { instruction: "srem" }),
        BuiltinOp::Neg => Some(ExprMapping::ArithmeticOp { instruction: "neg" }),
        BuiltinOp::Abs => Some(ExprMapping::ArithmeticOp { instruction: "select" }),
        BuiltinOp::Eq  => Some(ExprMapping::ArithmeticOp { instruction: "icmp eq" }),
        BuiltinOp::Neq => Some(ExprMapping::ArithmeticOp { instruction: "icmp ne" }),
        BuiltinOp::Lt  => Some(ExprMapping::ArithmeticOp { instruction: "icmp slt" }),
        BuiltinOp::Lte => Some(ExprMapping::ArithmeticOp { instruction: "icmp sle" }),
        BuiltinOp::Gt  => Some(ExprMapping::ArithmeticOp { instruction: "icmp sgt" }),
        BuiltinOp::Gte => Some(ExprMapping::ArithmeticOp { instruction: "icmp sge" }),
        BuiltinOp::And => Some(ExprMapping::ArithmeticOp { instruction: "and" }),
        BuiltinOp::Or  => Some(ExprMapping::ArithmeticOp { instruction: "or" }),
        BuiltinOp::Not => Some(ExprMapping::ArithmeticOp { instruction: "xor" }),
        BuiltinOp::IntToFloat => Some(ExprMapping::ArithmeticOp { instruction: "sitofp" }),
        BuiltinOp::FloatToInt => Some(ExprMapping::ArithmeticOp { instruction: "fptosi" }),
        BuiltinOp::Map     => Some(ExprMapping::ArrayLoop { operation: "map" }),
        BuiltinOp::Filter  => Some(ExprMapping::ArrayLoop { operation: "filter" }),
        BuiltinOp::Fold    => Some(ExprMapping::ArrayLoop { operation: "fold" }),
        BuiltinOp::FlatMap => Some(ExprMapping::ArrayLoop { operation: "flat_map" }),
        _ => None,
    }
}

/// Builtins that require runtime support functions.
#[rustfmt::skip]
fn classify_runtime_builtin(op: BuiltinOp) -> ExprMapping {
    match op {
        BuiltinOp::Floor       => ExprMapping::RuntimeCall { function: "floor" },
        BuiltinOp::Ceil        => ExprMapping::RuntimeCall { function: "ceil" },
        BuiltinOp::Concat      => ExprMapping::RuntimeCall { function: "panproto_rt_str_concat" },
        BuiltinOp::Len         => ExprMapping::RuntimeCall { function: "panproto_rt_str_len" },
        BuiltinOp::Slice       => ExprMapping::RuntimeCall { function: "panproto_rt_str_slice" },
        BuiltinOp::Upper       => ExprMapping::RuntimeCall { function: "panproto_rt_str_upper" },
        BuiltinOp::Lower       => ExprMapping::RuntimeCall { function: "panproto_rt_str_lower" },
        BuiltinOp::Trim        => ExprMapping::RuntimeCall { function: "panproto_rt_str_trim" },
        BuiltinOp::Split       => ExprMapping::RuntimeCall { function: "panproto_rt_str_split" },
        BuiltinOp::Join        => ExprMapping::RuntimeCall { function: "panproto_rt_str_join" },
        BuiltinOp::Replace     => ExprMapping::RuntimeCall { function: "panproto_rt_str_replace" },
        BuiltinOp::Contains    => ExprMapping::RuntimeCall { function: "panproto_rt_str_contains" },
        BuiltinOp::Head        => ExprMapping::RuntimeCall { function: "panproto_rt_head" },
        BuiltinOp::Tail        => ExprMapping::RuntimeCall { function: "panproto_rt_tail" },
        BuiltinOp::Reverse     => ExprMapping::RuntimeCall { function: "panproto_rt_reverse" },
        BuiltinOp::Append      => ExprMapping::RuntimeCall { function: "panproto_rt_append" },
        BuiltinOp::Length      => ExprMapping::RuntimeCall { function: "panproto_rt_length" },
        BuiltinOp::MergeRecords => ExprMapping::RuntimeCall { function: "panproto_rt_record_merge" },
        BuiltinOp::Keys        => ExprMapping::RuntimeCall { function: "panproto_rt_keys" },
        BuiltinOp::Values      => ExprMapping::RuntimeCall { function: "panproto_rt_values" },
        BuiltinOp::HasField    => ExprMapping::RuntimeCall { function: "panproto_rt_has_field" },
        BuiltinOp::IntToStr    => ExprMapping::RuntimeCall { function: "panproto_rt_int_to_str" },
        BuiltinOp::StrToInt    => ExprMapping::RuntimeCall { function: "panproto_rt_str_to_int" },
        BuiltinOp::FloatToStr  => ExprMapping::RuntimeCall { function: "panproto_rt_float_to_str" },
        BuiltinOp::StrToFloat  => ExprMapping::RuntimeCall { function: "panproto_rt_str_to_float" },
        BuiltinOp::TypeOf      => ExprMapping::RuntimeCall { function: "panproto_rt_type_of" },
        BuiltinOp::IsNull      => ExprMapping::RuntimeCall { function: "panproto_rt_is_null" },
        BuiltinOp::IsList      => ExprMapping::RuntimeCall { function: "panproto_rt_is_list" },
        BuiltinOp::Edge        => ExprMapping::RuntimeCall { function: "panproto_rt_edge" },
        BuiltinOp::Children    => ExprMapping::RuntimeCall { function: "panproto_rt_children" },
        BuiltinOp::HasEdge     => ExprMapping::RuntimeCall { function: "panproto_rt_has_edge" },
        BuiltinOp::EdgeCount   => ExprMapping::RuntimeCall { function: "panproto_rt_edge_count" },
        BuiltinOp::Anchor      => ExprMapping::RuntimeCall { function: "panproto_rt_anchor" },
        _ => unreachable!(),
    }
}

/// Collect all free variable references in an expression.
fn collect_free_vars(
    expr: &Expr,
    bound: &std::collections::HashSet<String>,
    free: &mut std::collections::HashSet<String>,
) {
    match expr {
        Expr::Var(name) => {
            let name_str = name.to_string();
            if !bound.contains(&name_str) {
                free.insert(name_str);
            }
        }
        Expr::Lit(_) => {}
        Expr::App(f, arg) => {
            collect_free_vars(f, bound, free);
            collect_free_vars(arg, bound, free);
        }
        Expr::Lam(param, body) => {
            let mut inner_bound = bound.clone();
            inner_bound.insert(param.to_string());
            collect_free_vars(body, &inner_bound, free);
        }
        Expr::Record(fields) => {
            for (_, val) in fields {
                collect_free_vars(val, bound, free);
            }
        }
        Expr::List(elems) => {
            for elem in elems {
                collect_free_vars(elem, bound, free);
            }
        }
        Expr::Field(base, _) => {
            collect_free_vars(base, bound, free);
        }
        Expr::Index(base, idx) => {
            collect_free_vars(base, bound, free);
            collect_free_vars(idx, bound, free);
        }
        Expr::Match { scrutinee, arms } => {
            collect_free_vars(scrutinee, bound, free);
            for (pattern, body) in arms {
                let mut arm_bound = bound.clone();
                collect_pattern_bindings(pattern, &mut arm_bound);
                collect_free_vars(body, &arm_bound, free);
            }
        }
        Expr::Let { name, value, body } => {
            collect_free_vars(value, bound, free);
            let mut inner_bound = bound.clone();
            inner_bound.insert(name.to_string());
            collect_free_vars(body, &inner_bound, free);
        }
        Expr::Builtin(_, args) => {
            for arg in args {
                collect_free_vars(arg, bound, free);
            }
        }
    }
}

/// Extract variable names bound by a pattern and add them to the bound set.
fn collect_pattern_bindings(
    pattern: &panproto_expr::Pattern,
    bound: &mut std::collections::HashSet<String>,
) {
    match pattern {
        panproto_expr::Pattern::Wildcard | panproto_expr::Pattern::Lit(_) => {}
        panproto_expr::Pattern::Var(name) => {
            bound.insert(name.to_string());
        }
        panproto_expr::Pattern::Record(fields) => {
            for (_, sub_pat) in fields {
                collect_pattern_bindings(sub_pat, bound);
            }
        }
        panproto_expr::Pattern::List(elems) => {
            for sub_pat in elems {
                collect_pattern_bindings(sub_pat, bound);
            }
        }
        panproto_expr::Pattern::Constructor(_, args) => {
            for sub_pat in args {
                collect_pattern_bindings(sub_pat, bound);
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
            ExprMapping::Constant {
                llvm_type: LlvmType::I64
            }
        );
    }

    #[test]
    fn classify_string_literal() {
        let expr = Expr::Lit(Literal::Str("hello".into()));
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::Constant {
                llvm_type: LlvmType::Ptr
            }
        );
    }

    #[test]
    fn classify_add() {
        let expr = Expr::Builtin(
            BuiltinOp::Add,
            vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(2))],
        );
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::ArithmeticOp { instruction: "add" }
        );
    }

    #[test]
    fn classify_string_concat() {
        let expr = Expr::Builtin(
            BuiltinOp::Concat,
            vec![
                Expr::Lit(Literal::Str("a".into())),
                Expr::Lit(Literal::Str("b".into())),
            ],
        );
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::RuntimeCall {
                function: "panproto_rt_str_concat"
            }
        );
    }

    #[test]
    fn classify_map_loop() {
        let expr = Expr::Builtin(
            BuiltinOp::Map,
            vec![
                Expr::Lam("x".into(), Box::new(Expr::Var("x".into()))),
                Expr::List(vec![Expr::Lit(Literal::Int(1))]),
            ],
        );
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
                (
                    panproto_expr::Pattern::Lit(Literal::Int(0)),
                    Expr::Lit(Literal::Str("zero".into())),
                ),
                (
                    panproto_expr::Pattern::Wildcard,
                    Expr::Lit(Literal::Str("other".into())),
                ),
            ],
        };
        assert_eq!(
            classify_expr(&expr),
            ExprMapping::PatternMatch { arm_count: 2 }
        );
    }

    #[test]
    fn classify_all_builtins() {
        let builtins = vec![
            BuiltinOp::Add,
            BuiltinOp::Sub,
            BuiltinOp::Mul,
            BuiltinOp::Div,
            BuiltinOp::Mod,
            BuiltinOp::Neg,
            BuiltinOp::Abs,
            BuiltinOp::Floor,
            BuiltinOp::Ceil,
            BuiltinOp::Eq,
            BuiltinOp::Neq,
            BuiltinOp::Lt,
            BuiltinOp::Lte,
            BuiltinOp::Gt,
            BuiltinOp::Gte,
            BuiltinOp::And,
            BuiltinOp::Or,
            BuiltinOp::Not,
            BuiltinOp::Concat,
            BuiltinOp::Len,
            BuiltinOp::Slice,
            BuiltinOp::Upper,
            BuiltinOp::Lower,
            BuiltinOp::Trim,
            BuiltinOp::Split,
            BuiltinOp::Join,
            BuiltinOp::Replace,
            BuiltinOp::Contains,
            BuiltinOp::Map,
            BuiltinOp::Filter,
            BuiltinOp::Fold,
            BuiltinOp::FlatMap,
            BuiltinOp::Head,
            BuiltinOp::Tail,
            BuiltinOp::Reverse,
            BuiltinOp::Append,
            BuiltinOp::Length,
            BuiltinOp::MergeRecords,
            BuiltinOp::Keys,
            BuiltinOp::Values,
            BuiltinOp::HasField,
            BuiltinOp::IntToFloat,
            BuiltinOp::FloatToInt,
            BuiltinOp::IntToStr,
            BuiltinOp::StrToInt,
            BuiltinOp::FloatToStr,
            BuiltinOp::StrToFloat,
            BuiltinOp::TypeOf,
            BuiltinOp::IsNull,
            BuiltinOp::IsList,
            BuiltinOp::Edge,
            BuiltinOp::Children,
            BuiltinOp::HasEdge,
            BuiltinOp::EdgeCount,
            BuiltinOp::Anchor,
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
                // Only "f" is free; "x" is bound by the lambda.
                assert_eq!(capture_count, 1);
            }
            other => panic!("expected Closure, got {other:?}"),
        }
    }
}
