//! Integer-pair ordering stays exact without using decimal fallback.

use panproto_expr::exact::BigIntValue;
use panproto_expr::{BuiltinOp, Literal, apply_builtin};

#[test]
fn bigint_pair_ordering_compares_sign_length_and_significand()
-> Result<(), Box<dyn std::error::Error>> {
    for (lower, upper) in [
        ("9223372036854775808", "9223372036854775809"),
        ("99999999999999999999", "100000000000000000000"),
        ("-100000000000000000000", "-99999999999999999999"),
        ("-9223372036854775809", "9223372036854775808"),
    ] {
        let lower = Literal::integer(BigIntValue::parse(lower)?);
        let upper = Literal::integer(BigIntValue::parse(upper)?);
        assert_eq!(
            apply_builtin(BuiltinOp::Lt, &[lower.clone(), upper.clone()])?,
            Literal::Bool(true)
        );
        assert_eq!(
            apply_builtin(BuiltinOp::Gt, &[upper, lower.clone()])?,
            Literal::Bool(true)
        );
        assert_eq!(
            apply_builtin(BuiltinOp::Lte, &[lower.clone(), lower])?,
            Literal::Bool(true)
        );
    }
    Ok(())
}
