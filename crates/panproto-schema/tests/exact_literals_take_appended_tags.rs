//! The exact literals take the canonical tags after every existing literal
//! tag, so no literal written before them changes its bytes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_expr::Literal;
use panproto_expr::exact::{BigIntValue, DecimalValue, ExactInteger, Temporal};
use panproto_schema::CanonicalWriter;

fn bytes(literal: &Literal) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    w.literal(literal);
    w.into_bytes()
}

const fn le64(n: i64) -> [u8; 8] {
    n.to_le_bytes()
}

#[test]
fn exact_literals_take_appended_tags() {
    // -1.50: tag 9, sign 1, digits "150" behind a u64 length, then the
    // exponent -2 as its sign byte and the digits of its magnitude behind a
    // u64 length.
    let decimal = Literal::Decimal(DecimalValue::parse("-1.50").unwrap());
    let mut expected = vec![9, 1];
    expected.extend_from_slice(&3_u64.to_le_bytes());
    expected.extend_from_slice(b"150");
    expected.push(1);
    expected.extend_from_slice(&1_u64.to_le_bytes());
    expected.extend_from_slice(b"2");
    assert_eq!(bytes(&decimal), expected);

    // 2^63: tag 10, sign 0, the significand behind its length, then the
    // count of trailing zeros, "0", behind its length.
    let ExactInteger::Big(big) = BigIntValue::parse("9223372036854775808").unwrap() else {
        panic!("outside i64")
    };
    let mut expected = vec![10, 0];
    expected.extend_from_slice(&19_u64.to_le_bytes());
    expected.extend_from_slice(b"9223372036854775808");
    expected.extend_from_slice(&1_u64.to_le_bytes());
    expected.extend_from_slice(b"0");
    assert_eq!(bytes(&Literal::BigInt(big)), expected);

    // A date-time: tag 11, form 2, year, month, day, the separator `T`,
    // hour, minute, second, the fraction's digit count and digits, then a
    // fixed offset of 330 minutes.
    let when = Temporal::parse("2026-09-28T12:34:56.789+05:30").unwrap();
    let mut expected = vec![11, 2];
    expected.extend_from_slice(&le64(2026));
    expected.extend_from_slice(&[9, 28, b'T', 12, 34, 56]);
    expected.extend_from_slice(&3_u64.to_le_bytes());
    expected.extend_from_slice(b"789");
    expected.push(2);
    expected.extend_from_slice(&le64(330));
    assert_eq!(bytes(&Literal::Temporal(when)), expected);

    // The date and the local and UTC offsets.
    let mut expected = vec![11, 0];
    expected.extend_from_slice(&le64(-44));
    expected.extend_from_slice(&[3, 15]);
    assert_eq!(
        bytes(&Literal::Temporal(Temporal::parse("-0044-03-15").unwrap())),
        expected
    );
    let mut expected = vec![11, 1, 23, 59, 60];
    expected.extend_from_slice(&0_u64.to_le_bytes());
    expected.extend_from_slice(&[1, b'Z']);
    assert_eq!(
        bytes(&Literal::Temporal(Temporal::parse("23:59:60Z").unwrap())),
        expected
    );
    let local = bytes(&Literal::Temporal(Temporal::parse("23:59:59").unwrap()));
    assert_eq!(local.last(), Some(&0));
    let unknown = bytes(&Literal::Temporal(
        Temporal::parse("2026-01-01T00:00:00-00:00").unwrap(),
    ));
    assert_eq!(unknown.last(), Some(&3));
}

#[test]
fn existing_literal_tags_keep_their_bytes() {
    let mut int = vec![1];
    int.extend_from_slice(&le64(5));
    assert_eq!(bytes(&Literal::Int(5)), int);
    assert_eq!(bytes(&Literal::Null), vec![5]);
    assert_eq!(bytes(&Literal::Bool(true)), vec![0, 1]);
    let mut s = vec![3];
    s.extend_from_slice(&1_u64.to_le_bytes());
    s.push(b'x');
    assert_eq!(bytes(&Literal::Str("x".into())), s);
    // Decimal and integer literals of one number stay distinct.
    assert_ne!(
        bytes(&Literal::Decimal(DecimalValue::parse("5").unwrap())),
        bytes(&Literal::Int(5))
    );
    // Scale is part of the bytes.
    assert_ne!(
        bytes(&Literal::Decimal(DecimalValue::parse("1.5").unwrap())),
        bytes(&Literal::Decimal(DecimalValue::parse("1.50").unwrap()))
    );
}
