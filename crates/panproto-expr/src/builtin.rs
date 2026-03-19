//! Implementations of built-in operations.
//!
//! Each builtin is a pure function from `&[Literal]` to `Result<Literal, ExprError>`.
//! Type checking is done at evaluation time — arguments must have the expected types.

use std::sync::Arc;

use crate::error::ExprError;
use crate::expr::BuiltinOp;
use crate::literal::Literal;

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
        | BuiltinOp::Ceil => apply_arithmetic(op, args),

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
        | BuiltinOp::Length => apply_list(op, args),

        // --- Record ---
        BuiltinOp::MergeRecords | BuiltinOp::Keys | BuiltinOp::Values | BuiltinOp::HasField => {
            apply_record(op, args)
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
        BuiltinOp::Mod => match (&args[0], &args[1]) {
            (Literal::Int(_), Literal::Int(0)) => Err(ExprError::DivisionByZero),
            (Literal::Int(a), Literal::Int(b)) => Ok(Literal::Int(a % b)),
            _ => Err(type_err("int", &args[0])),
        },
        BuiltinOp::Neg => match &args[0] {
            Literal::Int(n) => Ok(Literal::Int(-n)),
            Literal::Float(f) => Ok(Literal::Float(-f)),
            other => Err(type_err("int|float", other)),
        },
        BuiltinOp::Abs => match &args[0] {
            Literal::Int(n) => Ok(Literal::Int(n.abs())),
            Literal::Float(f) => Ok(Literal::Float(f.abs())),
            other => Err(type_err("int|float", other)),
        },
        #[allow(clippy::cast_possible_truncation)]
        BuiltinOp::Floor => match &args[0] {
            Literal::Float(f) => Ok(Literal::Int(f.floor() as i64)),
            other => Err(type_err("float", other)),
        },
        #[allow(clippy::cast_possible_truncation)]
        BuiltinOp::Ceil => match &args[0] {
            Literal::Float(f) => Ok(Literal::Int(f.ceil() as i64)),
            other => Err(type_err("float", other)),
        },
        _ => unreachable!(),
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
        _ => unreachable!(),
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
        _ => unreachable!(),
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
                let start = (*start).max(0) as usize;
                let end = (*end).max(0) as usize;
                let end = end.min(s.len());
                let start = start.min(end);
                Ok(Literal::Str(s[start..end].to_string()))
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
        BuiltinOp::Contains => match (&args[0], &args[1]) {
            (Literal::Str(s), Literal::Str(substr)) => Ok(Literal::Bool(s.contains(&**substr))),
            _ => Err(type_err("(string, string)", &args[0])),
        },
        _ => unreachable!(),
    }
}

/// List operations.
#[allow(clippy::cast_possible_wrap)]
fn apply_list(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        // Map, Filter, Fold, FlatMap require lambda evaluation — handled in eval.rs.
        BuiltinOp::Map | BuiltinOp::Filter | BuiltinOp::Fold | BuiltinOp::FlatMap => {
            Err(ExprError::TypeError {
                expected: "handled in evaluator".into(),
                got: "direct builtin call".into(),
            })
        }
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
        _ => unreachable!(),
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
        _ => unreachable!(),
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
            #[allow(clippy::cast_possible_truncation)]
            Literal::Float(f) => Ok(Literal::Int(*f as i64)),
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
            Literal::Str(s) => {
                s.parse::<f64>()
                    .map(Literal::Float)
                    .map_err(|_| ExprError::ParseError {
                        value: s.clone(),
                        target_type: "float".into(),
                    })
            }
            other => Err(type_err("string", other)),
        },
        _ => unreachable!(),
    }
}

/// Type inspection operations.
fn apply_inspection(op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
    match op {
        BuiltinOp::TypeOf => Ok(Literal::Str(args[0].type_name().to_string())),
        BuiltinOp::IsNull => Ok(Literal::Bool(args[0].is_null())),
        BuiltinOp::IsList => Ok(Literal::Bool(matches!(args[0], Literal::List(_)))),
        _ => unreachable!(),
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
            int_op(*x, *y)
                .map(Literal::Int)
                .ok_or_else(|| ExprError::TypeError {
                    expected: "non-overflowing arithmetic".into(),
                    got: "integer overflow".into(),
                })
        }
        (Literal::Float(x), Literal::Float(y)) => Ok(Literal::Float(float_op(*x, *y))),
        #[allow(clippy::cast_precision_loss)]
        (Literal::Int(x), Literal::Float(y)) => Ok(Literal::Float(float_op(*x as f64, *y))),
        #[allow(clippy::cast_precision_loss)]
        (Literal::Float(x), Literal::Int(y)) => Ok(Literal::Float(float_op(*x, *y as f64))),
        _ => Err(type_err("int|float", a)),
    }
}

/// Ordering comparison for numeric and string types.
fn compare(
    a: &Literal,
    b: &Literal,
    pred: fn(std::cmp::Ordering) -> bool,
) -> Result<Literal, ExprError> {
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
}
