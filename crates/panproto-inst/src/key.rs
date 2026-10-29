//! Declared key equality, ordering, and declaration predicates.
//!
//! The functions in this module are the production kernels shared by
//! validation and reference resolution. They operate on exact values: finite
//! binary64 values are compared as dyadic rationals, decimal scale and signed
//! zero do not affect numeric equality, and every NaN belongs to one equality
//! class. A named SQL collation is deliberately not guessed.

use std::cmp::Ordering;

use panproto_expr::exact::{DecimalClass, DecimalValue, WideInt};
use panproto_gat::Name;
use panproto_schema::{Key, KeyEquality, NumericPrimitive, SqlCollation, SqlPad};

use crate::Value;

/// A typed refusal from a declared-key operation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeyingError {
    /// A text value requires a collation whose kernel panproto does not
    /// evaluate.
    #[error("unsupported collation {0}")]
    UnsupportedCollation(Name),
    /// A key's nonempty equality vector, or a supplied tuple, has the wrong
    /// arity.
    #[error("expected {expected} key parts, found {found}")]
    EqualityArity {
        /// Number of key parts.
        expected: usize,
        /// Number of equality declarations or tuple values.
        found: usize,
    },
    /// A value that carries a numeric representation did not satisfy that
    /// representation's invariant.
    #[error("invalid {kind} numeric representation")]
    InvalidNumericRepresentation {
        /// The numeric representation that could not be normalized.
        kind: &'static str,
    },
}

#[derive(Clone, Debug)]
enum NumericKey {
    NegativeInfinity,
    Finite(DecimalValue),
    PositiveInfinity,
    NaN,
}

impl PartialEq for NumericKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::NegativeInfinity, Self::NegativeInfinity)
            | (Self::PositiveInfinity, Self::PositiveInfinity)
            | (Self::NaN, Self::NaN) => true,
            (Self::Finite(left), Self::Finite(right)) => left.cmp_numeric(right).is_eq(),
            _ => false,
        }
    }
}

impl Eq for NumericKey {}

impl PartialOrd for NumericKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NumericKey {
    fn cmp(&self, other: &Self) -> Ordering {
        use NumericKey::{Finite, NaN, NegativeInfinity, PositiveInfinity};
        match (self, other) {
            (NegativeInfinity, NegativeInfinity)
            | (PositiveInfinity, PositiveInfinity)
            | (NaN, NaN) => Ordering::Equal,
            (NegativeInfinity, _) | (_, NaN) => Ordering::Less,
            (_, NegativeInfinity) | (NaN | PositiveInfinity, _) => Ordering::Greater,
            (_, PositiveInfinity) => Ordering::Less,
            (Finite(left), Finite(right)) => left.cmp_numeric(right),
        }
    }
}

fn normalize_decimal(mut value: DecimalValue) -> DecimalValue {
    if value.is_zero() {
        return DecimalValue::from_i64(0);
    }
    while value.digits.ends_with('0') {
        value.digits.pop();
        value.exponent = value.exponent.plus(&WideInt::from(1_usize));
    }
    value
}

fn exact_float(value: f64) -> Result<DecimalValue, KeyingError> {
    // Every finite binary64 has at most 767 significant decimal digits. The
    // precision is intentionally fixed rather than shortest: 0.1 must stay
    // distinct from the mathematical decimal 0.1.
    DecimalValue::parse(&format!("{value:.767e}")).map_err(|_| {
        KeyingError::InvalidNumericRepresentation {
            kind: "finite binary64",
        }
    })
}

fn numeric_key(value: &Value) -> Result<Option<NumericKey>, KeyingError> {
    Ok(match value {
        Value::Int(value) => Some(NumericKey::Finite(DecimalValue::from_i64(*value))),
        Value::BigInt(value) => Some(NumericKey::Finite(normalize_decimal(
            DecimalValue::new(
                value.negative,
                value.significand.clone(),
                value.zeros.clone(),
            )
            .map_err(|_| KeyingError::InvalidNumericRepresentation {
                kind: "big integer",
            })?,
        ))),
        Value::Float(value) if value.is_nan() => Some(NumericKey::NaN),
        Value::Float(value) if *value == f64::NEG_INFINITY => Some(NumericKey::NegativeInfinity),
        Value::Float(value) if *value == f64::INFINITY => Some(NumericKey::PositiveInfinity),
        Value::Float(value) => Some(NumericKey::Finite(normalize_decimal(exact_float(*value)?))),
        Value::Decimal(value) => Some(match value.class {
            DecimalClass::Finite => NumericKey::Finite(normalize_decimal(value.clone())),
            DecimalClass::Infinite if value.negative => NumericKey::NegativeInfinity,
            DecimalClass::Infinite => NumericKey::PositiveInfinity,
            DecimalClass::NaN { .. } => NumericKey::NaN,
        }),
        Value::Bool(_)
        | Value::Str(_)
        | Value::Bytes(_)
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Token(_)
        | Value::Null
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_)
        | Value::Temporal(_) => None,
    })
}

const fn primitive(value: &Value) -> Option<NumericPrimitive> {
    match value {
        Value::Int(_) | Value::BigInt(_) | Value::Decimal(_) => Some(NumericPrimitive::Decimal),
        Value::Float(_) => Some(NumericPrimitive::Float(64)),
        Value::Bool(_)
        | Value::Str(_)
        | Value::Bytes(_)
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Token(_)
        | Value::Null
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_)
        | Value::Temporal(_) => None,
    }
}

fn sql_text(text: &str, collation: &SqlCollation, pad: SqlPad) -> Result<String, KeyingError> {
    let text = if pad == SqlPad::PadSpace {
        text.trim_end_matches(' ')
    } else {
        text
    };
    match collation {
        SqlCollation::Binary => Ok(text.to_owned()),
        SqlCollation::AsciiCaseInsensitive => Ok(text.to_ascii_lowercase()),
        SqlCollation::Named(name) => Err(KeyingError::UnsupportedCollation(name.clone())),
    }
}

/// Normalize XML non-CDATA whitespace after XML attribute processing by
/// collapsing U+0020 runs and trimming U+0020 at both ends.
#[must_use]
pub fn xml_noncdata(text: &str) -> String {
    text.split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compare two part values under one declared equality.
///
/// # Errors
///
/// [`KeyingError::UnsupportedCollation`] when two strings require a named
/// SQL collation, or [`KeyingError::InvalidNumericRepresentation`] when a
/// carried numeric value violates its representation invariant.
pub fn key_equivalence(
    equality: &KeyEquality,
    left: &Value,
    right: &Value,
) -> Result<bool, KeyingError> {
    match equality {
        KeyEquality::Representation => Ok(left.canonical_cmp(right).is_eq()),
        KeyEquality::NumericValue => match (numeric_key(left)?, numeric_key(right)?) {
            (Some(left), Some(right)) => Ok(left == right),
            _ => Ok(left.canonical_cmp(right).is_eq()),
        },
        KeyEquality::NumericValueWithinKind(declared) => {
            match (numeric_key(left)?, numeric_key(right)?) {
                (Some(left_key), Some(right_key)) => {
                    let left_primitive = if *declared == NumericPrimitive::Any {
                        primitive(left)
                    } else {
                        Some(*declared)
                    };
                    let right_primitive = if *declared == NumericPrimitive::Any {
                        primitive(right)
                    } else {
                        Some(*declared)
                    };
                    Ok(left_primitive == right_primitive && left_key == right_key)
                }
                _ => Ok(left.canonical_cmp(right).is_eq()),
            }
        }
        // The key function of text under a named collation is undefined, so
        // every pair holding such text is refused.
        KeyEquality::Sql {
            collation: SqlCollation::Named(name),
            ..
        } if matches!(left, Value::Str(_)) || matches!(right, Value::Str(_)) => {
            Err(KeyingError::UnsupportedCollation(name.clone()))
        }
        KeyEquality::Sql { collation, pad } => match (numeric_key(left)?, numeric_key(right)?) {
            (Some(left), Some(right)) => Ok(left == right),
            (None, None) => match (left, right) {
                (Value::Str(left), Value::Str(right)) => {
                    Ok(sql_text(left, collation, *pad)? == sql_text(right, collation, *pad)?)
                }
                _ => Ok(left.canonical_cmp(right).is_eq()),
            },
            _ => Ok(false),
        },
        KeyEquality::XmlNonCdata => match (left, right) {
            (Value::Str(left), Value::Str(right)) => Ok(xml_noncdata(left) == xml_noncdata(right)),
            _ => Ok(left.canonical_cmp(right).is_eq()),
        },
    }
}

/// Whether representation equality implies the declared equality for this
/// pair.
///
/// # Errors
///
/// The same named-collation refusal as [`key_equivalence`].
pub fn representation_refines(
    equality: &KeyEquality,
    left: &Value,
    right: &Value,
) -> Result<bool, KeyingError> {
    Ok(!left.canonical_cmp(right).is_eq() || key_equivalence(equality, left, right)?)
}

fn equalities(key: &Key) -> Result<Vec<&KeyEquality>, KeyingError> {
    static REPRESENTATION: KeyEquality = KeyEquality::Representation;
    if key.equality.is_empty() {
        return Ok((0..key.parts.len()).map(|_| &REPRESENTATION).collect());
    }
    if key.equality.len() != key.parts.len() {
        return Err(KeyingError::EqualityArity {
            expected: key.parts.len(),
            found: key.equality.len(),
        });
    }
    Ok(key.equality.iter().collect())
}

/// Compare two complete tuples under a key's per-part equality.
///
/// Tuples of the wrong arity are unequal; a malformed key declaration is a
/// typed error.
///
/// # Errors
///
/// A malformed equality vector or named SQL collation.
pub fn tuple_equivalence(key: &Key, left: &[Value], right: &[Value]) -> Result<bool, KeyingError> {
    if left.len() != key.parts.len() || right.len() != key.parts.len() {
        return Ok(false);
    }
    for ((equality, left), right) in equalities(key)?.into_iter().zip(left).zip(right) {
        if !key_equivalence(equality, left, right)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn tag(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn len(&mut self, len: usize) {
        self.u64(u64::try_from(len).unwrap_or(u64::MAX));
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.len(bytes.len());
        self.0.extend_from_slice(bytes);
    }

    fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }
}

fn write_numeric_key(writer: &mut Writer, key: &NumericKey) {
    match key {
        NumericKey::NegativeInfinity => writer.tag(0),
        NumericKey::Finite(value) => {
            writer.tag(1);
            writer.tag(u8::from(value.negative));
            writer.str(&value.digits);
            writer.tag(u8::from(value.exponent.is_negative()));
            writer.str(value.exponent.magnitude());
        }
        NumericKey::PositiveInfinity => writer.tag(2),
        NumericKey::NaN => writer.tag(3),
    }
}

fn write_primitive(writer: &mut Writer, primitive: NumericPrimitive) {
    match primitive {
        NumericPrimitive::Decimal => writer.tag(0),
        NumericPrimitive::Float(width) => {
            writer.tag(1);
            writer.tag(width);
        }
        NumericPrimitive::Any => writer.tag(2),
    }
}

fn equality_key(equality: &KeyEquality, value: &Value) -> Result<Vec<u8>, KeyingError> {
    let mut writer = Writer::default();
    match equality {
        KeyEquality::Representation => {
            writer.tag(0);
            writer.0.extend_from_slice(&value.canonical_bytes());
        }
        KeyEquality::NumericValue => {
            writer.tag(1);
            if let Some(key) = numeric_key(value)? {
                writer.tag(0);
                write_numeric_key(&mut writer, &key);
            } else {
                writer.tag(1);
                writer.0.extend_from_slice(&value.canonical_bytes());
            }
        }
        KeyEquality::NumericValueWithinKind(declared) => {
            writer.tag(2);
            let selected = if *declared == NumericPrimitive::Any {
                primitive(value).unwrap_or(NumericPrimitive::Any)
            } else {
                *declared
            };
            write_primitive(&mut writer, selected);
            if let Some(key) = numeric_key(value)? {
                writer.tag(0);
                write_numeric_key(&mut writer, &key);
            } else {
                writer.tag(1);
                writer.0.extend_from_slice(&value.canonical_bytes());
            }
        }
        KeyEquality::Sql { collation, pad } => {
            writer.tag(3);
            if let Some(key) = numeric_key(value)? {
                writer.tag(0);
                write_numeric_key(&mut writer, &key);
            } else if let Value::Str(text) = value {
                writer.tag(1);
                writer.str(&sql_text(text, collation, *pad)?);
            } else {
                writer.tag(2);
                writer.0.extend_from_slice(&value.canonical_bytes());
            }
        }
        KeyEquality::XmlNonCdata => {
            writer.tag(4);
            if let Value::Str(text) = value {
                writer.tag(0);
                writer.str(&xml_noncdata(text));
            } else {
                writer.tag(1);
                writer.0.extend_from_slice(&value.canonical_bytes());
            }
        }
    }
    Ok(writer.0)
}

/// Canonical bucket bytes for a complete tuple under a key's declared
/// equality.
///
/// # Errors
///
/// A tuple/equality arity mismatch or named SQL collation.
pub fn canonical_key_bytes(key: &Key, values: &[Value]) -> Result<Vec<u8>, KeyingError> {
    if values.len() != key.parts.len() {
        return Err(KeyingError::EqualityArity {
            expected: key.parts.len(),
            found: values.len(),
        });
    }
    let mut writer = Writer::default();
    writer.len(values.len());
    for (equality, value) in equalities(key)?.into_iter().zip(values) {
        writer.bytes(&equality_key(equality, value)?);
    }
    Ok(writer.0)
}

/// Compare mutually comparable sort-key values.
///
/// # Errors
///
/// [`KeyingError::InvalidNumericRepresentation`] when a carried numeric
/// value violates its representation invariant.
pub fn sort_order(left: &Value, right: &Value) -> Result<Option<Ordering>, KeyingError> {
    Ok(match (numeric_key(left)?, numeric_key(right)?) {
        (Some(NumericKey::NaN), _) | (_, Some(NumericKey::NaN)) => None,
        (Some(left), Some(right)) => Some(left.cmp(&right)),
        (None, None) => match (left, right) {
            (Value::Str(left), Value::Str(right)) => Some(left.as_bytes().cmp(right.as_bytes())),
            (Value::Bool(left), Value::Bool(right)) => Some(left.cmp(right)),
            _ => None,
        },
        _ => None,
    })
}

/// Whether a sequence has no adjacent sort-key descent.
///
/// # Errors
///
/// [`KeyingError::InvalidNumericRepresentation`] when a carried numeric
/// value violates its representation invariant.
pub fn key_order(values: &[Value]) -> Result<Option<bool>, KeyingError> {
    let mut all = true;
    for pair in values.windows(2) {
        let Some(order) = sort_order(&pair[0], &pair[1])? else {
            return Ok(None);
        };
        all &= !order.is_gt();
    }
    Ok(Some(all))
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use panproto_expr::exact::DecimalValue;
    use panproto_schema::{KeyEquality, NumericPrimitive, SqlCollation, SqlPad};

    use super::{KeyingError, key_equivalence, key_order, sort_order, xml_noncdata};
    use crate::Value;

    #[test]
    fn exact_numeric_kernel_distinguishes_binary_point_one_from_decimal_point_one()
    -> Result<(), Box<dyn std::error::Error>> {
        let decimal = Value::Decimal(DecimalValue::parse("0.1")?);
        assert!(!key_equivalence(
            &KeyEquality::NumericValue,
            &Value::Float(0.1),
            &decimal
        )?);
        assert!(key_equivalence(
            &KeyEquality::NumericValue,
            &Value::Float(1.0),
            &Value::Int(1)
        )?);
        Ok(())
    }

    #[test]
    fn zeros_and_nans_follow_the_declared_numeric_classes() -> Result<(), Box<dyn std::error::Error>>
    {
        let negative_zero = Value::Decimal(DecimalValue::parse("-0E+3")?);
        assert!(key_equivalence(
            &KeyEquality::NumericValue,
            &Value::Float(-0.0),
            &negative_zero
        )?);
        assert!(key_equivalence(
            &KeyEquality::NumericValue,
            &Value::Float(f64::from_bits(0x7ff8_0000_0000_0001)),
            &Value::Float(f64::from_bits(0xfff8_0000_0000_0002)),
        )?);
        assert!(!key_equivalence(
            &KeyEquality::NumericValueWithinKind(NumericPrimitive::Any),
            &Value::Float(1.0),
            &Value::Int(1),
        )?);
        Ok(())
    }

    #[test]
    fn named_collation_refuses_only_text() -> Result<(), Box<dyn std::error::Error>> {
        let equality = KeyEquality::Sql {
            collation: SqlCollation::Named("locale".into()),
            pad: SqlPad::NoPad,
        };
        assert!(matches!(
            key_equivalence(&equality, &Value::Str("a".into()), &Value::Str("a".into())),
            Err(KeyingError::UnsupportedCollation(name)) if name == "locale"
        ));
        assert!(key_equivalence(
            &equality,
            &Value::Int(1),
            &Value::Float(1.0)
        )?);
        Ok(())
    }

    #[test]
    fn xml_and_sort_kernels_are_exact() {
        assert_eq!(xml_noncdata("  a  b  "), "a b");
        assert_eq!(xml_noncdata("\ta\r\n"), "\ta\r\n");
        assert_eq!(
            sort_order(&Value::Int(1), &Value::Float(1.0)),
            Ok(Some(Ordering::Equal))
        );
        assert_eq!(
            key_order(&[Value::Str("a".into()), Value::Str("b".into())]),
            Ok(Some(true))
        );
        assert_eq!(key_order(&[Value::Float(f64::NAN)]), Ok(Some(true)));
        assert_eq!(
            key_order(&[Value::Float(f64::NAN), Value::Int(0)]),
            Ok(None)
        );
    }
}
