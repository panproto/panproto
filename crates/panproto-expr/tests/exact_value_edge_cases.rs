#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Exact-value edge cases: the empty decimal mantissa, the sign of an
//! integer zero, the tie between two shortest decimals, and zero as a wide
//! integer.

use std::sync::mpsc;
use std::time::Duration;

use panproto_expr::exact::{DecimalValue, WideInt};

#[test]
fn a_decimal_lexeme_needs_a_digit_on_one_side_of_the_point() {
    for text in [".", "-.", "+.", "e5", ".e5", "-e1", ".E-2"] {
        assert!(
            DecimalValue::parse(text).is_err(),
            "{text:?} read as {:?}",
            DecimalValue::parse(text)
        );
    }
    // Either side alone is enough.
    assert_eq!(
        DecimalValue::parse(".5").unwrap(),
        DecimalValue::new(false, "5", -1_i64).unwrap()
    );
    assert_eq!(
        DecimalValue::parse("5.").unwrap(),
        DecimalValue::new(false, "5", 0_i64).unwrap()
    );
}

#[test]
fn the_decimal_of_an_integer_zero_is_the_positive_zero() {
    let zero = DecimalValue::from_i64(0);
    assert!(!zero.negative);
    assert_eq!(zero, DecimalValue::new(false, "0", 0_i64).unwrap());
    assert_eq!(zero.to_string(), "0");
    assert!(DecimalValue::from_i64(-1).negative);
    assert!(!DecimalValue::from_i64(1).negative);
}

#[test]
fn a_tie_between_two_shortest_decimals_takes_the_even_one() {
    // 2^50 + 1/4: both 1125899906842624.2 and .3 read back; .2 is even.
    let tie = f64::from_bits(0x4310_0000_0000_0001);
    assert_eq!(
        DecimalValue::from_f64_shortest(tie).map(|d| d.to_string()),
        Some("1125899906842624.2".to_owned())
    );
    assert_eq!(
        DecimalValue::from_f64_shortest(-tie).map(|d| d.to_string()),
        Some("-1125899906842624.2".to_owned())
    );
    // The binary32 value 52401.3125: .312 and .313 both read back.
    let tie32 = 838_421.0_f32 / 16.0;
    assert_eq!(
        DecimalValue::from_f32_shortest(tie32).map(|d| d.to_string()),
        Some("52401.312".to_owned())
    );
}

#[test]
fn zero_is_the_non_negative_wide_integer_zero() {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(WideInt::zero());
    });
    let zero = receive
        .recv_timeout(Duration::from_secs(10))
        .expect("WideInt::zero did not return within ten seconds");
    assert!(zero.is_zero());
    assert!(!zero.is_negative());
    assert_eq!(zero.magnitude(), "0");
    assert_eq!(zero.to_string(), "0");
    assert_eq!(zero, WideInt::from(0_i64));
}
