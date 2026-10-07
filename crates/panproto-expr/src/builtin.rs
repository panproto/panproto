//! Implementations of built-in operations.
//!
//! Each builtin is a pure function from `&[Literal]` to `Result<Literal, ExprError>`.
//! Type checking is done at evaluation time; arguments must have the expected types.
//!
//! A NaN that a builtin computes is the canonical quiet NaN,
//! [`CANONICAL_NAN`] (bits `0x7ff8000000000000`): IEEE 754 arithmetic leaves
//! the sign and payload of a NaN result to the platform, and negation and
//! absolute value change a NaN's sign, so the builtins that compute a float
//! (`Add`, `Sub`, `Mul`, `Div`, `Neg`, `Abs`, `Clamp` and `DecimalToFloat`)
//! return the canonical NaN in place of any NaN result, and two computed
//! NaNs are equal bit for bit. `StrToFloat` reads a NaN spelling as a
//! lexeme is read under a float kind: one written with a leading minus
//! gives [`NEGATIVE_CANONICAL_NAN`] (bits `0xfff8000000000000`), every other
//! NaN spelling [`CANONICAL_NAN`], whatever bits the platform's parser
//! gives. A builtin that returns an argument or an element as it is (`Head`,
//! a field access, a branch) passes a NaN along with its bits.

use std::sync::Arc;

use std::cmp::Ordering;

use crate::error::ExprError;
use crate::exact::{CANONICAL_NAN, DecimalValue, NEGATIVE_CANONICAL_NAN};
use crate::expr::BuiltinOp;
use crate::literal::Literal;

/// `x`, or [`CANONICAL_NAN`] when `x` is a NaN: the float a builtin that
/// computes `x` returns.
const fn canonical_float(x: f64) -> f64 {
    if x.is_nan() { CANONICAL_NAN } else { x }
}

/// The float `StrToFloat` returns for the text `s`, which the platform's
/// parser read as `x`: `x` itself unless it is a NaN, and for a NaN
/// spelling [`NEGATIVE_CANONICAL_NAN`] when `s` is written with a leading
/// minus and [`CANONICAL_NAN`] otherwise, as a NaN lexeme is read under a
/// float kind.
fn read_nan_by_sign(s: &str, x: f64) -> f64 {
    match (x.is_nan(), s.starts_with('-')) {
        (false, _) => x,
        (true, true) => NEGATIVE_CANONICAL_NAN,
        (true, false) => CANONICAL_NAN,
    }
}

/// 2^63, the least float above `i64::MAX`; -2^63 is `i64::MIN` exactly.
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;

/// Convert a float to i64 by truncation toward zero, returning an error for
/// NaN, infinity, or a value whose truncation lies outside `i64`.
///
/// The range is [-2^63, 2^63), compared exactly: both bounds are binary64
/// values, so a float is in range iff it is at least -2^63 and below 2^63,
/// and its truncation then lies in `i64` and `as` is exact on it. 2^63
/// itself is refused rather than saturated to `i64::MAX`.
fn float_to_i64(f: f64) -> Result<Literal, ExprError> {
    if !(-TWO_POW_63..TWO_POW_63).contains(&f) {
        return Err(ExprError::FloatNotRepresentable(format!("{f}")));
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(Literal::Int(f as i64))
}

/// Apply a builtin operation to evaluated arguments.
///
/// # Errors
///
/// Returns [`ExprError`] if argument types don't match or a runtime
/// error occurs (division by zero, parse failure, etc.).
pub fn apply_builtin(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    let expected = op.arity();
    if args.len() != expected {
        return Err(ExprError::ArityMismatch {
            op: format!("{op:?}"),
            expected,
            got: args.len(),
        });
    }

    match op {
        // --- Arithmetic ---
        BuiltinOp::Add
        | BuiltinOp::Sub
        | BuiltinOp::Mul
        | BuiltinOp::Div
        | BuiltinOp::Mod
        | BuiltinOp::Neg
        | BuiltinOp::Abs
        | BuiltinOp::Floor
        | BuiltinOp::Ceil
        | BuiltinOp::Round => {
            if let Some(exact) = args.iter().find(|a| is_exact(a)) {
                return Err(ExprError::ExactArithmetic {
                    op: format!("{op:?}"),
                    operand: exact.type_name().into(),
                });
            }
            apply_arithmetic(op, args)
        }

        // --- Comparison ---
        BuiltinOp::Eq
        | BuiltinOp::Neq
        | BuiltinOp::Lt
        | BuiltinOp::Lte
        | BuiltinOp::Gt
        | BuiltinOp::Gte => apply_comparison(op, args),

        // --- Boolean ---
        BuiltinOp::And | BuiltinOp::Or | BuiltinOp::Not => apply_boolean(op, args),

        // --- String ---
        BuiltinOp::Concat
        | BuiltinOp::Len
        | BuiltinOp::Slice
        | BuiltinOp::Upper
        | BuiltinOp::Lower
        | BuiltinOp::Trim
        | BuiltinOp::Split
        | BuiltinOp::Join
        | BuiltinOp::Replace
        | BuiltinOp::Contains => apply_string(op, args),

        // --- List ---
        BuiltinOp::Map
        | BuiltinOp::Filter
        | BuiltinOp::Fold
        | BuiltinOp::FlatMap
        | BuiltinOp::Append
        | BuiltinOp::Head
        | BuiltinOp::Tail
        | BuiltinOp::Reverse
        | BuiltinOp::Length
        | BuiltinOp::Range => apply_list(op, args),

        // --- Record ---
        BuiltinOp::MergeRecords | BuiltinOp::Keys | BuiltinOp::Values | BuiltinOp::HasField => {
            apply_record(op, args)
        }

        // --- Utility ---
        BuiltinOp::DefaultVal | BuiltinOp::Clamp | BuiltinOp::TruncateStr => {
            apply_utility(op, args)
        }

        // --- Type coercions ---
        BuiltinOp::IntToFloat
        | BuiltinOp::FloatToInt
        | BuiltinOp::IntToStr
        | BuiltinOp::FloatToStr
        | BuiltinOp::StrToInt
        | BuiltinOp::StrToFloat => apply_coercion(op, args),

        // --- Type inspection ---
        BuiltinOp::TypeOf | BuiltinOp::IsNull | BuiltinOp::IsList => apply_inspection(op, args),

        // --- Exact values ---
        BuiltinOp::ToString | BuiltinOp::ParseDecimal | BuiltinOp::DecimalToFloat => {
            apply_exact(op, &args[0])
        }
        // Graph traversal builtins read an instance, which this evaluator
        // does not hold. Answering `null` would make "no such edge" and "no
        // graph was consulted" indistinguishable, so the caller is told
        // instead. Supply a resolver — `panproto_inst::eval_with_instance`
        // and `eval_with_element_ops` do — to make these answer.
        BuiltinOp::Edge
        | BuiltinOp::Children
        | BuiltinOp::HasEdge
        | BuiltinOp::EdgeCount
        | BuiltinOp::Anchor
        | BuiltinOp::Edges => Err(ExprError::NoInstanceContext {
            op: format!("{op:?}"),
        }),
    }
}

/// Arithmetic operations.
fn apply_arithmetic(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::Add => numeric_binop(&args[0], &args[1], i64::checked_add, |a, b| a + b),
        BuiltinOp::Sub => numeric_binop(&args[0], &args[1], i64::checked_sub, |a, b| a - b),
        BuiltinOp::Mul => numeric_binop(&args[0], &args[1], i64::checked_mul, |a, b| a * b),
        BuiltinOp::Div => {
            let is_zero = match (&args[0], &args[1]) {
                (_, Literal::Int(0)) => true,
                (_, Literal::Float(b)) if *b == 0.0 => true,
                _ => false,
            };
            if is_zero {
                Err(ExprError::DivisionByZero)
            } else {
                numeric_binop(&args[0], &args[1], i64::checked_div, |a, b| a / b)
            }
        }
        // `i64::MIN % -1` overflows, so the remainder is taken through
        // `checked_rem` like every other integer operation here.
        BuiltinOp::Mod => match (&args[0], &args[1]) {
            (Literal::Int(_), Literal::Int(0)) => Err(ExprError::DivisionByZero),
            (Literal::Int(a), Literal::Int(b)) => a
                .checked_rem(*b)
                .map(Literal::Int)
                .ok_or(ExprError::Overflow),
            _ => Err(type_err("int", &args[0])),
        },
        BuiltinOp::Neg => match &args[0] {
            Literal::Int(n) => n.checked_neg().map(Literal::Int).ok_or(ExprError::Overflow),
            Literal::Float(f) => Ok(Literal::Float(canonical_float(-f))),
            other => Err(type_err("int|float", other)),
        },
        BuiltinOp::Abs => match &args[0] {
            Literal::Int(n) => n.checked_abs().map(Literal::Int).ok_or(ExprError::Overflow),
            Literal::Float(f) => Ok(Literal::Float(canonical_float(f.abs()))),
            other => Err(type_err("int|float", other)),
        },
        BuiltinOp::Floor => match &args[0] {
            Literal::Float(f) => float_to_i64(f.floor()),
            other => Err(type_err("float", other)),
        },
        BuiltinOp::Ceil => match &args[0] {
            Literal::Float(f) => float_to_i64(f.ceil()),
            other => Err(type_err("float", other)),
        },
        BuiltinOp::Round => match &args[0] {
            Literal::Float(f) => float_to_i64(f.round()),
            other => Err(type_err("float", other)),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Comparison operations.
fn apply_comparison(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::Eq => Ok(Literal::Bool(args[0] == args[1])),
        BuiltinOp::Neq => Ok(Literal::Bool(args[0] != args[1])),
        BuiltinOp::Lt => compare(&args[0], &args[1], std::cmp::Ordering::is_lt),
        BuiltinOp::Lte => compare(&args[0], &args[1], std::cmp::Ordering::is_le),
        BuiltinOp::Gt => compare(&args[0], &args[1], std::cmp::Ordering::is_gt),
        BuiltinOp::Gte => compare(&args[0], &args[1], std::cmp::Ordering::is_ge),
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Boolean operations.
fn apply_boolean(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::And => match (&args[0], &args[1]) {
            (Literal::Bool(a), Literal::Bool(b)) => Ok(Literal::Bool(*a && *b)),
            (Literal::Bool(_), other) | (other, _) => Err(type_err("bool", other)),
        },
        BuiltinOp::Or => match (&args[0], &args[1]) {
            (Literal::Bool(a), Literal::Bool(b)) => Ok(Literal::Bool(*a || *b)),
            (Literal::Bool(_), other) | (other, _) => Err(type_err("bool", other)),
        },
        BuiltinOp::Not => match &args[0] {
            Literal::Bool(b) => Ok(Literal::Bool(!b)),
            other => Err(type_err("bool", other)),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// String operations.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn apply_string(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::Concat => match (&args[0], &args[1]) {
            (Literal::Str(a), Literal::Str(b)) => {
                let mut s = a.clone();
                s.push_str(b);
                Ok(Literal::Str(s))
            }
            (Literal::Str(_), other) | (other, _) => Err(type_err("string", other)),
        },
        BuiltinOp::Len => match &args[0] {
            Literal::Str(s) => Ok(Literal::Int(s.len() as i64)),
            other => Err(type_err("string", other)),
        },
        BuiltinOp::Slice => match (&args[0], &args[1], &args[2]) {
            (Literal::Str(s), Literal::Int(start), Literal::Int(end)) => {
                let chars: Vec<char> = s.chars().collect();
                let start = (*start).max(0) as usize;
                let end = (*end).max(0) as usize;
                let end = end.min(chars.len());
                let start = start.min(end);
                let result: String = chars[start..end].iter().collect();
                Ok(Literal::Str(result))
            }
            _ => Err(type_err("(string, int, int)", &args[0])),
        },
        BuiltinOp::Upper => match &args[0] {
            Literal::Str(s) => Ok(Literal::Str(s.to_uppercase())),
            other => Err(type_err("string", other)),
        },
        BuiltinOp::Lower => match &args[0] {
            Literal::Str(s) => Ok(Literal::Str(s.to_lowercase())),
            other => Err(type_err("string", other)),
        },
        BuiltinOp::Trim => match &args[0] {
            Literal::Str(s) => Ok(Literal::Str(s.trim().to_string())),
            other => Err(type_err("string", other)),
        },
        BuiltinOp::Split => match (&args[0], &args[1]) {
            (Literal::Str(s), Literal::Str(delim)) => Ok(Literal::List(
                s.split(&**delim)
                    .map(|p| Literal::Str(p.to_string()))
                    .collect(),
            )),
            _ => Err(type_err("(string, string)", &args[0])),
        },
        BuiltinOp::Join => match (&args[0], &args[1]) {
            (Literal::List(parts), Literal::Str(delim)) => {
                let strs: Result<Vec<_>, _> = parts
                    .iter()
                    .map(|p| match p {
                        Literal::Str(s) => Ok(s.as_str()),
                        other => Err(type_err("string", other)),
                    })
                    .collect();
                Ok(Literal::Str(strs?.join(delim)))
            }
            _ => Err(type_err("([string], string)", &args[0])),
        },
        BuiltinOp::Replace => match (&args[0], &args[1], &args[2]) {
            (Literal::Str(s), Literal::Str(from), Literal::Str(to)) => {
                Ok(Literal::Str(s.replace(&**from, to)))
            }
            _ => Err(type_err("(string, string, string)", &args[0])),
        },
        // `Contains` is overloaded on its first argument: substring
        // containment on a string, element membership on a list. The list
        // case is what predicates over a list-valued field use — the field
        // arrives as a `Literal::List`, so membership is tested directly
        // rather than against a flattened string.
        BuiltinOp::Contains => match (&args[0], &args[1]) {
            (Literal::Str(s), Literal::Str(substr)) => Ok(Literal::Bool(s.contains(&**substr))),
            (Literal::List(items), needle) => Ok(Literal::Bool(items.iter().any(|i| i == needle))),
            _ => Err(type_err("(string, string) or (list, any)", &args[0])),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// List operations.
#[allow(clippy::cast_possible_wrap)]
fn apply_list(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        // Map, Filter, Fold, and FlatMap require lambda evaluation, and
        // Range needs the list-length budget; all are handled in eval.rs.
        BuiltinOp::Map
        | BuiltinOp::Filter
        | BuiltinOp::Fold
        | BuiltinOp::FlatMap
        | BuiltinOp::Range => Err(ExprError::TypeError {
            expected: "handled in evaluator".into(),
            got: "direct builtin call".into(),
        }),
        BuiltinOp::Append => match (&args[0], &args[1]) {
            (Literal::List(items), val) => {
                let mut new_items = items.clone();
                new_items.push(val.clone());
                Ok(Literal::List(new_items))
            }
            (other, _) => Err(type_err("list", other)),
        },
        BuiltinOp::Head => match &args[0] {
            Literal::List(items) if items.is_empty() => {
                Err(ExprError::IndexOutOfBounds { index: 0, len: 0 })
            }
            Literal::List(items) => Ok(items[0].clone()),
            other => Err(type_err("list", other)),
        },
        BuiltinOp::Tail => match &args[0] {
            Literal::List(items) if items.is_empty() => {
                Err(ExprError::IndexOutOfBounds { index: 0, len: 0 })
            }
            Literal::List(items) => Ok(Literal::List(items[1..].to_vec())),
            other => Err(type_err("list", other)),
        },
        BuiltinOp::Reverse => match &args[0] {
            Literal::List(items) => {
                let mut reversed = items.clone();
                reversed.reverse();
                Ok(Literal::List(reversed))
            }
            other => Err(type_err("list", other)),
        },
        BuiltinOp::Length => match &args[0] {
            Literal::List(items) => Ok(Literal::Int(items.len() as i64)),
            other => Err(type_err("list", other)),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Record operations.
fn apply_record(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::MergeRecords => match (&args[0], &args[1]) {
            (Literal::Record(a), Literal::Record(b)) => {
                let mut merged = a.clone();
                for (k, v) in b {
                    if let Some(existing) = merged.iter_mut().find(|(ek, _)| ek == k) {
                        existing.1 = v.clone();
                    } else {
                        merged.push((Arc::clone(k), v.clone()));
                    }
                }
                Ok(Literal::Record(merged))
            }
            (Literal::Record(_), other) | (other, _) => Err(type_err("record", other)),
        },
        BuiltinOp::Keys => match &args[0] {
            Literal::Record(fields) => Ok(Literal::List(
                fields
                    .iter()
                    .map(|(k, _)| Literal::Str(k.to_string()))
                    .collect(),
            )),
            other => Err(type_err("record", other)),
        },
        BuiltinOp::Values => match &args[0] {
            Literal::Record(fields) => Ok(Literal::List(
                fields.iter().map(|(_, v)| v.clone()).collect(),
            )),
            other => Err(type_err("record", other)),
        },
        BuiltinOp::HasField => match (&args[0], &args[1]) {
            (Literal::Record(fields), Literal::Str(name)) => Ok(Literal::Bool(
                fields.iter().any(|(k, _)| &**k == name.as_str()),
            )),
            _ => Err(type_err("(record, string)", &args[0])),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Type coercion operations.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn apply_coercion(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::IntToFloat => match &args[0] {
            Literal::Int(n) => Ok(Literal::Float(*n as f64)),
            other => Err(type_err("int", other)),
        },
        BuiltinOp::FloatToInt => match &args[0] {
            Literal::Float(f) => float_to_i64(*f),
            other => Err(type_err("float", other)),
        },
        BuiltinOp::IntToStr => match &args[0] {
            Literal::Int(n) => Ok(Literal::Str(n.to_string())),
            other => Err(type_err("int", other)),
        },
        BuiltinOp::FloatToStr => match &args[0] {
            Literal::Float(f) => Ok(Literal::Str(f.to_string())),
            other => Err(type_err("float", other)),
        },
        BuiltinOp::StrToInt => match &args[0] {
            Literal::Str(s) => {
                s.parse::<i64>()
                    .map(Literal::Int)
                    .map_err(|_| ExprError::ParseError {
                        value: s.clone(),
                        target_type: "int".into(),
                    })
            }
            other => Err(type_err("string", other)),
        },
        BuiltinOp::StrToFloat => match &args[0] {
            Literal::Str(s) => s
                .parse::<f64>()
                .map(|x| Literal::Float(read_nan_by_sign(s, x)))
                .map_err(|_| ExprError::ParseError {
                    value: s.clone(),
                    target_type: "float".into(),
                }),
            other => Err(type_err("string", other)),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Type inspection operations.
fn apply_inspection(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::TypeOf => Ok(Literal::Str(args[0].type_name().to_string())),
        BuiltinOp::IsNull => Ok(Literal::Bool(args[0].is_null())),
        BuiltinOp::IsList => Ok(Literal::Bool(matches!(args[0], Literal::List(_)))),
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Utility operations: default, clamp, `truncate_str`.
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn apply_utility(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::DefaultVal => {
            if args[0].is_null() {
                Ok(args[1].clone())
            } else {
                Ok(args[0].clone())
            }
        }
        BuiltinOp::Clamp => match (&args[0], &args[1], &args[2]) {
            (Literal::Int(x), Literal::Int(lo), Literal::Int(hi)) if lo <= hi => {
                Ok(Literal::Int((*x).clamp(*lo, *hi)))
            }
            (Literal::Float(x), Literal::Float(lo), Literal::Float(hi)) if lo <= hi => {
                Ok(Literal::Float(canonical_float(x.clamp(*lo, *hi))))
            }
            (Literal::Int(_), Literal::Int(_), Literal::Int(_))
            | (Literal::Float(_), Literal::Float(_), Literal::Float(_)) => {
                Err(ExprError::TypeError {
                    expected: "clamp requires min <= max".into(),
                    got: "min > max".into(),
                })
            }
            _ => Err(type_err(
                "(int, int, int) or (float, float, float)",
                &args[0],
            )),
        },
        BuiltinOp::TruncateStr => match (&args[0], &args[1]) {
            (Literal::Str(s), Literal::Int(max_len)) => {
                let max = (*max_len).max(0) as usize;
                let truncated = if max >= s.len() {
                    s.clone()
                } else {
                    // Find the last char boundary at or before max.
                    let mut end = max;
                    while end > 0 && !s.is_char_boundary(end) {
                        end -= 1;
                    }
                    s[..end].to_string()
                };
                Ok(Literal::Str(truncated))
            }
            _ => Err(type_err("(string, int)", &args[0])),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

/// Apply a numeric binary operation, promoting int+float to float.
fn numeric_binop(
    a: &Literal,
    b: &Literal,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Result<Literal, ExprError> {
    match (a, b) {
        (Literal::Int(x), Literal::Int(y)) => {
            int_op(*x, *y).map(Literal::Int).ok_or(ExprError::Overflow)
        }
        (Literal::Float(x), Literal::Float(y)) => {
            Ok(Literal::Float(canonical_float(float_op(*x, *y))))
        }
        #[allow(clippy::cast_precision_loss)]
        (Literal::Int(x), Literal::Float(y)) => {
            Ok(Literal::Float(canonical_float(float_op(*x as f64, *y))))
        }
        #[allow(clippy::cast_precision_loss)]
        (Literal::Float(x), Literal::Int(y)) => {
            Ok(Literal::Float(canonical_float(float_op(*x, *y as f64))))
        }
        _ => Err(type_err("int|float", a)),
    }
}

/// Ordering comparison for numeric and string types.
fn compare(
    a: &Literal,
    b: &Literal,
    pred: fn(std::cmp::Ordering) -> bool,
) -> Result<Literal, ExprError> {
    if let Some(ord) = compare_exact(a, b) {
        return ord.map(|o| Literal::Bool(pred(o)));
    }
    let ord = match (a, b) {
        (Literal::Int(x), Literal::Int(y)) => x.cmp(y),
        (Literal::Float(x), Literal::Float(y)) => x.total_cmp(y),
        #[allow(clippy::cast_precision_loss)]
        (Literal::Int(x), Literal::Float(y)) => (*x as f64).total_cmp(y),
        #[allow(clippy::cast_precision_loss)]
        (Literal::Float(x), Literal::Int(y)) => x.total_cmp(&(*y as f64)),
        (Literal::Str(x), Literal::Str(y)) => x.cmp(y),
        _ => {
            return Err(ExprError::TypeError {
                expected: "comparable types (int, float, or string)".into(),
                got: format!("({}, {})", a.type_name(), b.type_name()),
            });
        }
    };
    Ok(Literal::Bool(pred(ord)))
}

/// Whether `lit` is one of the exact literals arithmetic is not defined
/// on.
const fn is_exact(lit: &Literal) -> bool {
    matches!(
        lit,
        Literal::Decimal(_) | Literal::BigInt(_) | Literal::Temporal(_)
    )
}

/// `lit` as an exact decimal, when it is an integer or a decimal.
fn as_decimal(lit: &Literal) -> Option<DecimalValue> {
    match lit {
        Literal::Int(n) => Some(DecimalValue::from_i64(*n)),
        Literal::BigInt(b) => {
            DecimalValue::new(b.negative, b.significand.clone(), b.zeros.clone()).ok()
        }
        Literal::Decimal(d) => Some(d.clone()),
        _ => None,
    }
}

/// The ordering of two literals when at least one is exact: integers and
/// decimals compare as numbers, exactly; two temporals compare in time when
/// they are of one form and both local or both zoned. `None` when neither
/// is exact, so the ordinary comparison applies.
fn compare_exact(a: &Literal, b: &Literal) -> Option<Result<Ordering, ExprError>> {
    if !is_exact(a) && !is_exact(b) {
        return None;
    }
    let incomparable = || ExprError::TypeError {
        expected: "comparable exact values (int, bigint and decimal, or temporals of one form)"
            .into(),
        got: format!("({}, {})", a.type_name(), b.type_name()),
    };
    Some(match (a, b) {
        (Literal::BigInt(x), Literal::BigInt(y)) => Ok(x.cmp_numeric(y)),
        (Literal::BigInt(x), Literal::Int(y)) => Ok(x.cmp_i64(*y)),
        (Literal::Int(x), Literal::BigInt(y)) => Ok(y.cmp_i64(*x).reverse()),
        (Literal::Temporal(x), Literal::Temporal(y)) => {
            x.cmp_chronological(y).ok_or_else(incomparable)
        }
        _ => match (as_decimal(a), as_decimal(b)) {
            (Some(x), Some(y)) => Ok(x.cmp_numeric(&y)),
            _ => Err(incomparable()),
        },
    })
}

/// The exact-value builtins: `to_string`, `parse_decimal` and
/// `decimal_to_float`.
fn apply_exact(op: BuiltinOp, arg: &Literal) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::ToString => match arg {
            Literal::Bool(_)
            | Literal::Int(_)
            | Literal::Float(_)
            | Literal::Decimal(_)
            | Literal::BigInt(_)
            | Literal::Temporal(_) => Ok(Literal::Str(arg.to_string())),
            Literal::Str(s) => Ok(Literal::Str(s.clone())),
            other => Err(type_err(
                "bool|int|float|string|decimal|bigint|temporal",
                other,
            )),
        },
        BuiltinOp::ParseDecimal => {
            match arg {
                Literal::Str(s) => DecimalValue::parse(s).map(Literal::Decimal).map_err(|_| {
                    ExprError::ParseError {
                        value: s.clone(),
                        target_type: "decimal".into(),
                    }
                }),
                Literal::Int(_) | Literal::BigInt(_) | Literal::Decimal(_) => as_decimal(arg)
                    .map(Literal::Decimal)
                    .ok_or_else(|| type_err("string|int|bigint|decimal", arg)),
                other => Err(type_err("string|int|bigint|decimal", other)),
            }
        }
        BuiltinOp::DecimalToFloat => match arg {
            Literal::Decimal(d) => Ok(Literal::Float(canonical_float(d.to_f64()))),
            Literal::BigInt(b) => Ok(Literal::Float(canonical_float(b.to_f64()))),
            other => Err(type_err("decimal|bigint", other)),
        },
        _ => Err(ExprError::InternalDispatch {
            op: format!("{op:?}"),
        }),
    }
}

fn type_err(expected: &str, got: &Literal) -> ExprError {
    ExprError::TypeError {
        expected: expected.into(),
        got: got.type_name().into(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn add_ints() {
        let result = apply_builtin(BuiltinOp::Add, &[Literal::Int(2), Literal::Int(3)]);
        assert_eq!(result.unwrap(), Literal::Int(5));
    }

    #[test]
    fn add_int_float_promotion() {
        let result = apply_builtin(BuiltinOp::Add, &[Literal::Int(2), Literal::Float(1.5)]);
        assert_eq!(result.unwrap(), Literal::Float(3.5));
    }

    #[test]
    fn div_by_zero() {
        let result = apply_builtin(BuiltinOp::Div, &[Literal::Int(1), Literal::Int(0)]);
        assert!(matches!(result, Err(ExprError::DivisionByZero)));
    }

    #[test]
    fn string_split_join_roundtrip() {
        let parts = apply_builtin(
            BuiltinOp::Split,
            &[Literal::Str("a,b,c".into()), Literal::Str(",".into())],
        )
        .unwrap();
        let joined = apply_builtin(BuiltinOp::Join, &[parts, Literal::Str(",".into())]).unwrap();
        assert_eq!(joined, Literal::Str("a,b,c".into()));
    }

    #[test]
    fn str_to_int_ok() {
        let result = apply_builtin(BuiltinOp::StrToInt, &[Literal::Str("42".into())]);
        assert_eq!(result.unwrap(), Literal::Int(42));
    }

    #[test]
    fn str_to_int_fail() {
        let result = apply_builtin(BuiltinOp::StrToInt, &[Literal::Str("hello".into())]);
        assert!(matches!(result, Err(ExprError::ParseError { .. })));
    }

    #[test]
    fn record_merge() {
        let a = Literal::Record(vec![
            (Arc::from("x"), Literal::Int(1)),
            (Arc::from("y"), Literal::Int(2)),
        ]);
        let b = Literal::Record(vec![(Arc::from("y"), Literal::Int(99))]);
        let result = apply_builtin(BuiltinOp::MergeRecords, &[a, b]).unwrap();
        assert_eq!(
            result,
            Literal::Record(vec![
                (Arc::from("x"), Literal::Int(1)),
                (Arc::from("y"), Literal::Int(99)),
            ])
        );
    }

    #[test]
    fn list_head_tail() {
        let list = Literal::List(vec![Literal::Int(1), Literal::Int(2), Literal::Int(3)]);
        assert_eq!(
            apply_builtin(BuiltinOp::Head, std::slice::from_ref(&list)).unwrap(),
            Literal::Int(1)
        );
        assert_eq!(
            apply_builtin(BuiltinOp::Tail, &[list]).unwrap(),
            Literal::List(vec![Literal::Int(2), Literal::Int(3)])
        );
    }

    #[test]
    fn empty_list_head_errors() {
        let result = apply_builtin(BuiltinOp::Head, &[Literal::List(vec![])]);
        assert!(matches!(result, Err(ExprError::IndexOutOfBounds { .. })));
    }

    #[test]
    fn comparison_uses_total_cmp() {
        // NaN comparisons should not panic
        let result = apply_builtin(
            BuiltinOp::Lt,
            &[Literal::Float(f64::NAN), Literal::Float(1.0)],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn misrouted_op_returns_internal_dispatch_error() {
        // Calling a category handler with an op outside its category
        // returns an error rather than panicking. `Add` is arithmetic,
        // so routing it to the comparison handler must be rejected.
        let result = apply_comparison(BuiltinOp::Add, &[Literal::Int(1), Literal::Int(2)]);
        assert!(matches!(
            result,
            Err(ExprError::InternalDispatch { ref op }) if op == "Add"
        ));
    }

    /// The value an exact reference gives a float-to-integer builtin: `None`
    /// when the rounded value is not an integer in `i64`'s range. The rounded
    /// value is computed on the float (`trunc`, `floor`, `ceil` and `round`
    /// are exact on binary64) and compared with the range as an exact integer
    /// through its binary decomposition, so the reference uses no float-to-int
    /// cast.
    fn exact_integer(rounded: f64) -> Option<i64> {
        if !rounded.is_finite() {
            return None;
        }
        let bits = rounded.to_bits();
        let negative = bits >> 63 == 1;
        let biased = i32::try_from((bits >> 52) & 0x7ff).unwrap();
        let fraction = bits & ((1 << 52) - 1);
        if biased == 0 {
            // Zero, or a subnormal, which `rounded` (an integer) cannot be.
            assert_eq!(fraction, 0, "{rounded} is an integer");
            return Some(0);
        }
        let significand = i128::from((1_u64 << 52) | fraction);
        let shift = biased - 1075;
        let magnitude = if shift >= 0 {
            if shift > 64 {
                return None;
            }
            significand << shift
        } else {
            assert_eq!(significand % (1 << -shift), 0, "{rounded} is an integer");
            significand >> -shift
        };
        i64::try_from(if negative { -magnitude } else { magnitude }).ok()
    }

    /// A rounding of a float to an integral float.
    type Rounding = fn(f64) -> f64;

    /// Every float-to-integer builtin returns the exact integer when it lies
    /// in `i64`'s range and refuses the float otherwise; it never saturates.
    /// The scope holds 2^63 and -2^63, their binary64 neighbours on both sides,
    /// values near them with fractions, the infinities, a NaN and small values.
    #[test]
    fn a_float_beyond_i64_is_refused_not_saturated() {
        const TWO_63: f64 = 9_223_372_036_854_775_808.0;
        let near = |x: f64, steps: i64| {
            let bits = x.to_bits();
            f64::from_bits(if steps >= 0 {
                bits + steps.unsigned_abs()
            } else {
                bits - steps.unsigned_abs()
            })
        };
        let mut scope = vec![
            0.0,
            -0.0,
            0.5,
            -0.5,
            1.5,
            -1.5,
            2.5,
            -2.7,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            f64::MAX,
            f64::MIN,
            4_503_599_627_370_495.5,
            -4_503_599_627_370_495.5,
        ];
        for base in [TWO_63, -TWO_63, 2.0 * TWO_63, -2.0 * TWO_63] {
            for steps in -3..=3 {
                scope.push(near(base, steps));
            }
        }
        let ops: [(BuiltinOp, Rounding); 4] = [
            (BuiltinOp::FloatToInt, f64::trunc),
            (BuiltinOp::Floor, f64::floor),
            (BuiltinOp::Ceil, f64::ceil),
            (BuiltinOp::Round, f64::round),
        ];
        for (op, round) in ops {
            for &x in &scope {
                let got = apply_builtin(op, &[Literal::Float(x)]);
                match exact_integer(round(x)) {
                    Some(n) => assert!(
                        matches!(got, Ok(Literal::Int(m)) if m == n),
                        "{op:?}({x:e}) gave {got:?}, expected {n}"
                    ),
                    None => assert!(
                        matches!(got, Err(ExprError::FloatNotRepresentable(_))),
                        "{op:?}({x:e}) gave {got:?}"
                    ),
                }
            }
        }
        assert!(apply_builtin(BuiltinOp::FloatToInt, &[Literal::Float(TWO_63)]).is_err());
        assert!(matches!(
            apply_builtin(BuiltinOp::FloatToInt, &[Literal::Float(-TWO_63)]),
            Ok(Literal::Int(i64::MIN))
        ));
    }
}
