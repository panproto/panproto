//! What a protocol's constraint sorts mean.
//!
//! A [`Protocol`](crate::Protocol) maps each of its constraint sorts to a
//! [`ConstraintSemantics`] in
//! [`Protocol::constraint_semantics`](crate::Protocol::constraint_semantics).
//! Identifiers and uniqueness are not constraint sorts: a schema declares
//! them in [`SchemaDeclarations`](crate::SchemaDeclarations).
//!
//! A [`Constraint`](crate::Constraint) carries its value as text.
//! [`ConstraintSemantics::parse_value`] reads that text as the argument its
//! meaning takes, so `maxLength = "3000"` under
//! [`ConstraintSemantics::MaxLength`] is a length bound of 3000 and
//! `maxLength = "abc"` is a [`ConstraintValueError`]:
//!
//! ```
//! use panproto_schema::{ConstraintSemantics, ConstraintValue, LengthUnit};
//!
//! let semantics = ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes);
//! assert_eq!(
//!     semantics.parse_value("maxLength", "3000"),
//!     Ok(ConstraintValue::Length { unit: LengthUnit::Utf8Bytes, limit: 3000 }),
//! );
//! assert!(semantics.parse_value("maxLength", "abc").is_err());
//! ```
//!
//! Enums serialize externally tagged in `snake_case`.

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

/// The meaning of one constraint sort.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSemantics {
    /// Descriptive only; no check.
    Annotation,
    /// An upper bound on a value's length, counted in the given unit.
    MaxLength(LengthUnit),
    /// A lower bound on a value's length, counted in the given unit.
    MinLength(LengthUnit),
    /// An inclusive lower bound on a value.
    Minimum,
    /// An inclusive upper bound on a value.
    Maximum,
    /// The set of permitted values.
    Enum,
    /// The one permitted value.
    Const,
    /// A regular expression a value matches.
    Pattern,
    /// A value format of the named format family (the schema language whose
    /// vocabulary the format name comes from). The family is descriptive: no
    /// check reads it.
    ///
    /// The constraint's value names the format
    /// ([`ConstraintValue::Format`]'s `format`), from a fixed set of sixteen
    /// names: `date-time` (also read as `datetime`), `date`, `time`, `uri`,
    /// `uuid`, `ipv4`, `ipv6`, `did`, and the AT Protocol Lexicon string
    /// formats (<https://atproto.com/specs/lexicon>, "String Formats")
    /// `at-identifier`, `at-uri`, `cid`, `handle`, `nsid`, `tid`,
    /// `record-key` and `language` (a well-formed BCP 47 language tag).
    /// Each is checked as the specification it points to defines its
    /// syntax, and a value that fails is a failed check.
    /// Validation compiles a constraint
    /// whose value names any other format to an unsupported declaration
    /// (`UnsupportedDeclaration`), never to a pass, whatever the family. The
    /// evaluator is `panproto_inst::validation::ValueFormat`, whose rustdoc
    /// lists the same set; a change to the set edits both lists together.
    Format(Name),
    /// A lower bound on the number of out-edges of a node.
    MinOutDegree,
    /// An upper bound on the number of out-edges of a node.
    MaxOutDegree,
    /// A value must be present.
    RequiredValue,
    /// The edges labelled by the named self-edge form no cycle.
    Acyclic,
    /// Checked outside panproto.
    External,
    /// A parameter of value coercion (`precision`, `scale`, `max_length`,
    /// `with_time_zone`, `int_bits`, `unsigned`); no check.
    ValueParameter,
}

/// The unit a length constraint counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    /// Unicode scalar values.
    Chars,
    /// UTF-8 bytes.
    Utf8Bytes,
    /// Extended grapheme clusters.
    Graphemes,
}

impl ConstraintSemantics {
    /// Whether a validation check is compiled from a constraint with this
    /// meaning.
    ///
    /// [`Annotation`](Self::Annotation) is descriptive and
    /// [`ValueParameter`](Self::ValueParameter) parameterizes value
    /// coercion (read by [`ValueSpec::for_vertex`](crate::ValueSpec::for_vertex)),
    /// so neither compiles to a check; every other meaning does.
    #[must_use]
    pub const fn compiles_to_check(&self) -> bool {
        !matches!(self, Self::Annotation | Self::ValueParameter)
    }

    /// Reads the text value of a constraint of sort `sort` as the argument
    /// this meaning takes.
    ///
    /// - [`MaxLength`](Self::MaxLength) and [`MinLength`](Self::MinLength)
    ///   read a non-negative decimal integer, the length limit in their
    ///   unit.
    /// - [`MinOutDegree`](Self::MinOutDegree) and
    ///   [`MaxOutDegree`](Self::MaxOutDegree) read a non-negative decimal
    ///   integer, the out-edge count.
    /// - [`Minimum`](Self::Minimum) and [`Maximum`](Self::Maximum) read a
    ///   non-empty bound, kept as written: it is compared with instance
    ///   values in their exact kind, which the bound's text alone does not
    ///   fix.
    /// - [`RequiredValue`](Self::RequiredValue) reads `true` or `false`.
    /// - [`Format`](Self::Format) reads a non-empty format name, paired
    ///   with the format family the meaning names.
    /// - [`Acyclic`](Self::Acyclic) reads a non-empty edge name, the name
    ///   of the self-edge whose instances must form no cycle.
    /// - [`ValueParameter`](Self::ValueParameter) reads the parameter that
    ///   `sort` names, as [`ValueParameter::parse`] does.
    /// - [`Annotation`](Self::Annotation), [`Enum`](Self::Enum),
    ///   [`Const`](Self::Const), [`Pattern`](Self::Pattern) and
    ///   [`External`](Self::External) keep the text as written: how a
    ///   protocol lists enum members or writes a pattern is its own, and
    ///   the check that reads them interprets it.
    ///
    /// Integers are ASCII digits only, with no sign, space or separator.
    ///
    /// # Errors
    ///
    /// Returns [`ConstraintValueError`] naming the sort, the value and why
    /// the value does not read under this meaning.
    pub fn parse_value(
        &self,
        sort: &str,
        value: &str,
    ) -> Result<ConstraintValue, ConstraintValueError> {
        let fail = |reason| ConstraintValueError {
            sort: sort.to_owned(),
            value: value.to_owned(),
            reason,
        };
        match self {
            Self::MaxLength(unit) | Self::MinLength(unit) => Ok(ConstraintValue::Length {
                unit: unit.clone(),
                limit: parse_count(value).map_err(fail)?,
            }),
            Self::MinOutDegree | Self::MaxOutDegree => {
                Ok(ConstraintValue::Degree(parse_count(value).map_err(fail)?))
            }
            Self::Minimum | Self::Maximum => {
                non_empty(value).map_err(fail)?;
                Ok(ConstraintValue::Bound(value.to_owned()))
            }
            Self::RequiredValue => match value {
                "true" => Ok(ConstraintValue::Required(true)),
                "false" => Ok(ConstraintValue::Required(false)),
                _ => Err(fail(ConstraintValueReason::NotABool)),
            },
            Self::Format(family) => {
                non_empty(value).map_err(fail)?;
                Ok(ConstraintValue::Format {
                    family: family.clone(),
                    format: Name::from(value),
                })
            }
            Self::Acyclic => {
                non_empty(value).map_err(fail)?;
                Ok(ConstraintValue::Acyclic(Name::from(value)))
            }
            Self::ValueParameter => {
                ValueParameter::parse(sort, value).map(ConstraintValue::Parameter)
            }
            Self::Annotation | Self::Enum | Self::Const | Self::Pattern | Self::External => {
                Ok(ConstraintValue::Text(value.to_owned()))
            }
        }
    }
}

/// The argument of one constraint, read under its meaning by
/// [`ConstraintSemantics::parse_value`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintValue {
    /// The value as written, for meanings whose check interprets it.
    Text(String),
    /// A length limit, counted in `unit`.
    Length {
        /// The unit the limit counts.
        unit: LengthUnit,
        /// The limit.
        limit: u64,
    },
    /// An out-edge count.
    Degree(u64),
    /// A lower or upper bound on a value, as written.
    Bound(String),
    /// Whether a value is required.
    Required(bool),
    /// A named format of a format family.
    Format {
        /// The format family the meaning names.
        family: Name,
        /// The format the constraint names.
        format: Name,
    },
    /// The name of the self-edge whose instances form no cycle.
    Acyclic(Name),
    /// A parameter of value coercion.
    Parameter(ValueParameter),
}

/// A parameter of value coercion, the argument of a constraint whose
/// meaning is [`ConstraintSemantics::ValueParameter`].
///
/// The constraint's sort names the parameter; the eight sorts are
/// `precision`, `scale`, `max_length`, `with_time_zone`, `int_bits`,
/// `unsigned`, `time_unit` and `time_zone`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueParameter {
    /// The total number of significant digits (sort `precision`).
    Precision(u32),
    /// The number of digits after the decimal point (sort `scale`).
    Scale(u32),
    /// The maximum length (sort `max_length`).
    MaxLength(u32),
    /// Whether a temporal value carries a time zone (sort
    /// `with_time_zone`).
    WithTimeZone(bool),
    /// The integer width in bits (sort `int_bits`).
    IntBits(u8),
    /// Whether an integer is unsigned (sort `unsigned`).
    Unsigned(bool),
    /// The unit of an epoch count (sort `time_unit`), read from `s`, `ms`,
    /// `us` or `ns`.
    TimeUnit(panproto_expr::exact::TimeUnit),
    /// The time zone of a temporal column (sort `time_zone`): the declared
    /// text, non-empty, kept byte for byte. No zone database is consulted,
    /// so the text is neither checked against a list of zones nor used to
    /// convert a value.
    TimeZone(String),
}

impl ValueParameter {
    /// The constraint sorts that name a parameter, in declaration order.
    pub const SORTS: [&'static str; 8] = [
        "precision",
        "scale",
        "max_length",
        "with_time_zone",
        "int_bits",
        "unsigned",
        "time_unit",
        "time_zone",
    ];

    /// Reads the parameter that `sort` names from its text `value`.
    ///
    /// Counts are non-negative decimal integers of ASCII digits; flags are
    /// `true` or `false`; a time unit is `s`, `ms`, `us` or `ns`; a time
    /// zone is any non-empty text, kept as written.
    ///
    /// ```
    /// use panproto_schema::ValueParameter;
    ///
    /// assert_eq!(ValueParameter::parse("scale", "2"), Ok(ValueParameter::Scale(2)));
    /// assert!(ValueParameter::parse("int_bits", "300").is_err());
    /// assert!(ValueParameter::parse("width", "2").is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`ConstraintValueError`] when `sort` names no parameter or
    /// `value` does not read as that parameter's argument.
    pub fn parse(sort: &str, value: &str) -> Result<Self, ConstraintValueError> {
        let fail = |reason| ConstraintValueError {
            sort: sort.to_owned(),
            value: value.to_owned(),
            reason,
        };
        let flag = || match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(fail(ConstraintValueReason::NotABool)),
        };
        match sort {
            "precision" => Ok(Self::Precision(
                parse_bounded(value, u32::MAX).map_err(fail)?,
            )),
            "scale" => Ok(Self::Scale(parse_bounded(value, u32::MAX).map_err(fail)?)),
            "max_length" => Ok(Self::MaxLength(
                parse_bounded(value, u32::MAX).map_err(fail)?,
            )),
            "with_time_zone" => Ok(Self::WithTimeZone(flag()?)),
            "int_bits" => Ok(Self::IntBits(parse_bounded(value, u8::MAX).map_err(fail)?)),
            "unsigned" => Ok(Self::Unsigned(flag()?)),
            "time_unit" => panproto_expr::exact::TimeUnit::parse(value)
                .map(Self::TimeUnit)
                .ok_or_else(|| fail(ConstraintValueReason::NotATimeUnit)),
            "time_zone" => {
                non_empty(value).map_err(fail)?;
                Ok(Self::TimeZone(value.to_owned()))
            }
            _ => Err(fail(ConstraintValueReason::UnknownParameter)),
        }
    }
}

/// A constraint value that does not read under its meaning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("constraint {sort:?} = {value:?} is ill-formed: {reason}")]
pub struct ConstraintValueError {
    /// The constraint sort.
    pub sort: String,
    /// The value as written.
    pub value: String,
    /// Why the value does not read.
    pub reason: ConstraintValueReason,
}

/// Why a constraint value does not read under its meaning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintValueReason {
    /// A count was expected: ASCII digits with no sign.
    #[error("not a non-negative integer")]
    NotACount,
    /// The count is larger than its argument allows.
    #[error("larger than {max}")]
    OutOfRange {
        /// The largest value the argument allows.
        max: u64,
    },
    /// `true` or `false` was expected.
    #[error("not `true` or `false`")]
    NotABool,
    /// The value is empty, and this meaning needs an argument.
    #[error("empty")]
    Empty,
    /// The sort of a value-parameter constraint names no parameter.
    #[error(
        "not a value parameter (expected one of precision, scale, max_length, with_time_zone, int_bits, unsigned, time_unit, time_zone)"
    )]
    UnknownParameter,
    /// A time unit was expected: `s`, `ms`, `us` or `ns`.
    #[error("not a time unit (expected one of s, ms, us, ns)")]
    NotATimeUnit,
}

/// Reads a non-negative decimal integer of ASCII digits.
fn parse_count(value: &str) -> Result<u64, ConstraintValueReason> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ConstraintValueReason::NotACount);
    }
    value
        .parse()
        .map_err(|_| ConstraintValueReason::OutOfRange { max: u64::MAX })
}

/// Reads a count no larger than `max`, the largest value of `T`.
fn parse_bounded<T>(value: &str, max: T) -> Result<T, ConstraintValueReason>
where
    T: TryFrom<u64>,
    u64: From<T>,
{
    let n = parse_count(value)?;
    T::try_from(n).map_err(|_| ConstraintValueReason::OutOfRange {
        max: u64::from(max),
    })
}

/// Rejects an empty argument.
const fn non_empty(value: &str) -> Result<(), ConstraintValueReason> {
    if value.is_empty() {
        Err(ConstraintValueReason::Empty)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use panproto_gat::Name;

    use super::{
        ConstraintSemantics, ConstraintValue, ConstraintValueError, ConstraintValueReason,
        LengthUnit, ValueParameter,
    };

    /// Every variant, each unit included; the match below stops compiling
    /// when a variant is added without being listed here.
    fn every_variant() -> Vec<ConstraintSemantics> {
        let all = vec![
            ConstraintSemantics::Annotation,
            ConstraintSemantics::MaxLength(LengthUnit::Chars),
            ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes),
            ConstraintSemantics::MaxLength(LengthUnit::Graphemes),
            ConstraintSemantics::MinLength(LengthUnit::Chars),
            ConstraintSemantics::MinLength(LengthUnit::Utf8Bytes),
            ConstraintSemantics::MinLength(LengthUnit::Graphemes),
            ConstraintSemantics::Minimum,
            ConstraintSemantics::Maximum,
            ConstraintSemantics::Enum,
            ConstraintSemantics::Const,
            ConstraintSemantics::Pattern,
            ConstraintSemantics::Format(Name::from("family")),
            ConstraintSemantics::MinOutDegree,
            ConstraintSemantics::MaxOutDegree,
            ConstraintSemantics::RequiredValue,
            ConstraintSemantics::Acyclic,
            ConstraintSemantics::External,
            ConstraintSemantics::ValueParameter,
        ];
        for s in &all {
            match s {
                ConstraintSemantics::Annotation
                | ConstraintSemantics::MaxLength(_)
                | ConstraintSemantics::MinLength(_)
                | ConstraintSemantics::Minimum
                | ConstraintSemantics::Maximum
                | ConstraintSemantics::Enum
                | ConstraintSemantics::Const
                | ConstraintSemantics::Pattern
                | ConstraintSemantics::Format(_)
                | ConstraintSemantics::MinOutDegree
                | ConstraintSemantics::MaxOutDegree
                | ConstraintSemantics::RequiredValue
                | ConstraintSemantics::Acyclic
                | ConstraintSemantics::External
                | ConstraintSemantics::ValueParameter => {}
            }
        }
        all
    }

    #[test]
    fn constraint_semantics_round_trips_every_variant() -> Result<(), Box<dyn std::error::Error>> {
        for semantics in every_variant() {
            let json = serde_json::to_string(&semantics)?;
            assert_eq!(
                serde_json::from_str::<ConstraintSemantics>(&json)?,
                semantics,
                "{json}"
            );
            let named = rmp_serde::to_vec_named(&semantics)?;
            assert_eq!(
                rmp_serde::from_slice::<ConstraintSemantics>(&named)?,
                semantics
            );
            let positional = rmp_serde::to_vec(&semantics)?;
            assert_eq!(
                rmp_serde::from_slice::<ConstraintSemantics>(&positional)?,
                semantics
            );
        }
        Ok(())
    }

    #[test]
    fn constraint_semantics_serde_names_are_snake_case() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_string(&ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes))?,
            r#"{"max_length":"utf8_bytes"}"#
        );
        assert_eq!(
            serde_json::to_string(&ConstraintSemantics::Format(Name::from("family")))?,
            r#"{"format":"family"}"#
        );
        assert_eq!(
            serde_json::to_string(&ConstraintSemantics::RequiredValue)?,
            r#""required_value""#
        );
        assert_eq!(
            serde_json::to_string(&ConstraintSemantics::ValueParameter)?,
            r#""value_parameter""#
        );
        Ok(())
    }

    #[test]
    fn only_annotations_and_value_parameters_compile_to_no_check() {
        for semantics in every_variant() {
            let expected = !matches!(
                semantics,
                ConstraintSemantics::Annotation | ConstraintSemantics::ValueParameter
            );
            assert_eq!(semantics.compiles_to_check(), expected, "{semantics:?}");
        }
    }

    fn reason(
        semantics: &ConstraintSemantics,
        sort: &str,
        value: &str,
    ) -> Option<ConstraintValueReason> {
        semantics
            .parse_value(sort, value)
            .err()
            .map(|e: ConstraintValueError| e.reason)
    }

    #[test]
    fn each_meaning_reads_its_argument() {
        let cases = [
            (
                ConstraintSemantics::MaxLength(LengthUnit::Graphemes),
                "maxGraphemes",
                "300",
                ConstraintValue::Length {
                    unit: LengthUnit::Graphemes,
                    limit: 300,
                },
            ),
            (
                ConstraintSemantics::MinLength(LengthUnit::Chars),
                "minLength",
                "0",
                ConstraintValue::Length {
                    unit: LengthUnit::Chars,
                    limit: 0,
                },
            ),
            (
                ConstraintSemantics::MaxOutDegree,
                "maxItems",
                "4",
                ConstraintValue::Degree(4),
            ),
            (
                ConstraintSemantics::Minimum,
                "minimum",
                "-1.5e3",
                ConstraintValue::Bound("-1.5e3".into()),
            ),
            (
                ConstraintSemantics::RequiredValue,
                "NOT NULL",
                "true",
                ConstraintValue::Required(true),
            ),
            (
                ConstraintSemantics::Format(Name::from("family")),
                "format",
                "record-uri",
                ConstraintValue::Format {
                    family: Name::from("family"),
                    format: Name::from("record-uri"),
                },
            ),
            (
                ConstraintSemantics::Acyclic,
                "acyclic",
                "parent",
                ConstraintValue::Acyclic(Name::from("parent")),
            ),
            (
                ConstraintSemantics::ValueParameter,
                "precision",
                "12",
                ConstraintValue::Parameter(ValueParameter::Precision(12)),
            ),
            (
                ConstraintSemantics::Enum,
                "enum",
                "a,b",
                ConstraintValue::Text("a,b".into()),
            ),
            (
                ConstraintSemantics::Annotation,
                "description",
                "",
                ConstraintValue::Text(String::new()),
            ),
        ];
        for (semantics, sort, value, expected) in cases {
            assert_eq!(
                semantics.parse_value(sort, value),
                Ok(expected),
                "{semantics:?} {sort} = {value:?}"
            );
        }
    }

    #[test]
    fn an_argument_that_does_not_read_is_ill_formed() {
        let max_len = ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes);
        assert_eq!(
            reason(&max_len, "maxLength", "abc"),
            Some(ConstraintValueReason::NotACount)
        );
        assert_eq!(
            reason(&max_len, "maxLength", "-1"),
            Some(ConstraintValueReason::NotACount)
        );
        assert_eq!(
            reason(&max_len, "maxLength", " 3"),
            Some(ConstraintValueReason::NotACount)
        );
        assert_eq!(
            reason(&max_len, "maxLength", "99999999999999999999"),
            Some(ConstraintValueReason::OutOfRange { max: u64::MAX })
        );
        assert_eq!(
            reason(&ConstraintSemantics::MinOutDegree, "minItems", ""),
            Some(ConstraintValueReason::NotACount)
        );
        assert_eq!(
            reason(&ConstraintSemantics::Maximum, "maximum", ""),
            Some(ConstraintValueReason::Empty)
        );
        assert_eq!(
            reason(&ConstraintSemantics::RequiredValue, "required", "yes"),
            Some(ConstraintValueReason::NotABool)
        );
        assert_eq!(
            reason(&ConstraintSemantics::Acyclic, "acyclic", ""),
            Some(ConstraintValueReason::Empty)
        );
        assert_eq!(
            reason(&ConstraintSemantics::ValueParameter, "width", "3"),
            Some(ConstraintValueReason::UnknownParameter)
        );
        let Err(err) = max_len.parse_value("maxLength", "abc") else {
            panic!("abc is not a length");
        };
        assert_eq!(err.sort, "maxLength");
        assert_eq!(err.value, "abc");
        assert_eq!(
            err.to_string(),
            r#"constraint "maxLength" = "abc" is ill-formed: not a non-negative integer"#
        );
    }

    #[test]
    fn each_value_parameter_reads_its_sort() {
        assert_eq!(
            ValueParameter::SORTS
                .iter()
                .zip(["10", "2", "255", "true", "16", "false"])
                .map(|(sort, value)| ValueParameter::parse(sort, value))
                .collect::<Result<Vec<_>, _>>(),
            Ok(vec![
                ValueParameter::Precision(10),
                ValueParameter::Scale(2),
                ValueParameter::MaxLength(255),
                ValueParameter::WithTimeZone(true),
                ValueParameter::IntBits(16),
                ValueParameter::Unsigned(false),
            ])
        );
        assert_eq!(
            ValueParameter::parse("int_bits", "256").map_err(|e| e.reason),
            Err(ConstraintValueReason::OutOfRange { max: 255 })
        );
        assert_eq!(
            ValueParameter::parse("precision", "4294967296").map_err(|e| e.reason),
            Err(ConstraintValueReason::OutOfRange {
                max: u64::from(u32::MAX)
            })
        );
        assert_eq!(
            ValueParameter::parse("unsigned", "1").map_err(|e| e.reason),
            Err(ConstraintValueReason::NotABool)
        );
    }

    #[test]
    fn constraint_values_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let values = [
            ConstraintValue::Text("x".into()),
            ConstraintValue::Length {
                unit: LengthUnit::Chars,
                limit: 3,
            },
            ConstraintValue::Degree(1),
            ConstraintValue::Bound("2020-01-01".into()),
            ConstraintValue::Required(false),
            ConstraintValue::Format {
                family: Name::from("f"),
                format: Name::from("uri"),
            },
            ConstraintValue::Acyclic(Name::from("next")),
            ConstraintValue::Parameter(ValueParameter::IntBits(8)),
        ];
        for value in values {
            let json = serde_json::to_string(&value)?;
            assert_eq!(serde_json::from_str::<ConstraintValue>(&json)?, value);
            let bytes = rmp_serde::to_vec(&value)?;
            assert_eq!(rmp_serde::from_slice::<ConstraintValue>(&bytes)?, value);
        }
        Ok(())
    }
}
