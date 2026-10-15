//! The scalar kinds a protocol's vertex kinds hold.
//!
//! A [`Protocol`] maps each scalar vertex kind to a
//! [`ScalarKind`] in [`Protocol::value_kinds`];
//! a [`ValueSpec`] combines that kind with a vertex's value parameters.
//!
//! Enums serialize externally tagged in `snake_case`; structs serialize with
//! named fields in declaration order.

use std::collections::BTreeMap;

use panproto_expr::exact::TimeUnit;
use panproto_gat::Name;
use serde::{Deserialize, Serialize};

use crate::constraint_semantics::{ConstraintSemantics, ValueParameter};
use crate::{Protocol, Schema};

/// The kind of scalar a vertex holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarKind {
    /// A boolean.
    Bool,
    /// A fixed-width integer.
    Int {
        /// The width in bits.
        bits: u8,
        /// Whether the integer is signed.
        signed: bool,
    },
    /// An arbitrary-precision integer.
    BigInt,
    /// A binary floating-point number.
    Float {
        /// The width in bits.
        bits: u8,
    },
    /// An exact decimal number.
    Decimal,
    /// A string.
    Str,
    /// A byte string, with the text encoding it takes in textual formats.
    Bytes(BytesText),
    /// A calendar date.
    Date,
    /// A time of day.
    Time,
    /// A date and time.
    DateTime,
    /// A token from a closed vocabulary.
    Token,
    /// No declared kind.
    Any,
}

/// The text encoding of a byte string.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BytesText {
    /// Standard base64, with padding optional on read.
    Base64,
    /// URL-safe base64, with padding optional on read.
    Base64Url,
    /// Standard base64, with canonical padding required on read.
    Base64Padded,
    /// URL-safe base64, with canonical padding required on read.
    Base64UrlPadded,
    /// Hexadecimal.
    Hex,
    /// The UTF-8 bytes of the text.
    Raw,
    /// One code point U+0000 through U+00FF per byte.
    Latin1,
}

/// How a float column reads a finite numeral.
///
/// Serialized as `"exact"` or `"nearest_keeping_lexeme"`; a spec with the
/// default reading omits the field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloatReading {
    /// A numeral is accepted when it is exactly the float it rounds to or
    /// that float's shortest text, and refused otherwise.
    #[default]
    Exact,
    /// Every finite numeral reads as the float nearest it, an overflow as
    /// the infinity of its sign, and the route keeps the lexeme: the
    /// reading of a column whose source stores binary floats and writes
    /// them as decimal text that need not be shortest (an SQL dump).
    NearestKeepingLexeme,
}

impl FloatReading {
    /// Whether this is [`FloatReading::Exact`].
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        matches!(self, Self::Exact)
    }
}

/// The numeral grammar of the route a lexeme was read from.
///
/// A declared dialect decides which lexemes are numerals and which number
/// each denotes; with none declared a lexeme is read by the default
/// numeral grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumeralDialect {
    /// RFC 8259 JSON numbers: no `+`, leading zero, `_`, radix prefix or
    /// non-finite spelling.
    Json,
    /// TOML 1.0 and 1.1 integers and floats: `_` between digits, `0x`,
    /// `0o` and `0b` integers, a leading `+`, and `inf` and `nan` with an
    /// optional sign.
    Toml,
    /// The YAML 1.2.2 core schema's integer and float forms.
    Yaml12,
    /// The YAML 1.1 type pages' integer and float forms, sexagesimal and
    /// octal with a leading `0` included.
    Yaml11,
    /// XML Schema 1.1's decimal and double lexical forms.
    Xsd,
    /// Java's decimal floating-point literals and its case-sensitive
    /// `NaN`, `Infinity` and `-Infinity`.
    Java,
    /// `ProtoJSON` numbers, with the quoted `NaN`, `Infinity` and
    /// `-Infinity` of floats.
    ProtoJson,
    /// SQL numerals and PostgreSQL's special float and numeric values.
    Sql,
}

/// A scalar kind together with the value parameters of one vertex.
///
/// The fields after `unsigned` default to their absent forms and are left
/// out of the serialized spec when absent, so a spec without them reads and
/// writes as it did before they existed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValueSpec {
    /// The scalar kind.
    pub kind: ScalarKind,
    /// The total number of significant digits, if declared.
    pub precision: Option<u32>,
    /// The number of digits after the decimal point, if declared.
    pub scale: Option<u32>,
    /// The maximum length, if declared.
    pub max_length: Option<u32>,
    /// Whether a temporal value carries a time zone, if declared.
    pub with_time_zone: Option<bool>,
    /// The integer width in bits, if declared.
    pub int_bits: Option<u8>,
    /// Whether an integer is unsigned.
    pub unsigned: bool,
    /// How a float column reads a finite numeral.
    #[serde(default, skip_serializing_if = "FloatReading::is_exact")]
    pub float_reading: FloatReading,
    /// The numeral grammar of the route, when it declares one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeral: Option<NumeralDialect>,
    /// The kinds a sibling's discriminator names: a lexeme tagged with a
    /// key of the table is read under the spec with that kind, and one
    /// tagged with any other tag is kept as an opaque value of the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminators: Option<BTreeMap<String, ScalarKind>>,
    /// The unit of an epoch count, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_unit: Option<TimeUnit>,
    /// The time zone of a temporal column, as declared (an Olson name or a
    /// numeric offset, kept byte for byte), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
}

impl ValueSpec {
    /// A spec of kind `kind` with no value parameters.
    #[must_use]
    pub const fn new(kind: ScalarKind) -> Self {
        Self {
            kind,
            precision: None,
            scale: None,
            max_length: None,
            with_time_zone: None,
            int_bits: None,
            unsigned: false,
            float_reading: FloatReading::Exact,
            numeral: None,
            discriminators: None,
            time_unit: None,
            time_zone: None,
        }
    }

    /// The spec of the values held at `vertex` of `schema`.
    ///
    /// The kind is the protocol's [`ScalarKind`] for the vertex's kind, from
    /// [`Protocol::value_kinds`], or [`ScalarKind::Any`] when the protocol
    /// classifies no such kind or the schema has no such vertex. Every
    /// constraint on the vertex whose sort the protocol means as
    /// [`ConstraintSemantics::ValueParameter`] then sets its parameter, as
    /// [`ValueParameter::parse`] reads it, in the order the schema lists the
    /// constraints, so a later one wins. A value-parameter constraint whose
    /// value does not read sets nothing here; validation reports it as an
    /// ill-formed declaration.
    ///
    /// ```
    /// use std::collections::BTreeMap;
    ///
    /// use panproto_gat::Name;
    /// use panproto_schema::{
    ///     ConstraintSemantics, Protocol, ScalarKind, SchemaBuilder, ValueSpec,
    /// };
    ///
    /// let protocol = Protocol {
    ///     name: "demo".into(),
    ///     obj_kinds: vec!["decimal".into()],
    ///     constraint_sorts: vec!["precision".into(), "scale".into()],
    ///     constraint_semantics: BTreeMap::from([
    ///         ("precision".to_owned(), ConstraintSemantics::ValueParameter),
    ///         ("scale".to_owned(), ConstraintSemantics::ValueParameter),
    ///     ]),
    ///     value_kinds: BTreeMap::from([("decimal".to_owned(), ScalarKind::Decimal)]),
    ///     ..Protocol::default()
    /// };
    /// let schema = SchemaBuilder::new(&protocol)
    ///     .vertex("price", "decimal", None)?
    ///     .constraint("price", "precision", "10")
    ///     .constraint("price", "scale", "2")
    ///     .build()?;
    ///
    /// let spec = ValueSpec::for_vertex(&schema, &protocol, &Name::from("price"));
    /// assert_eq!(spec.kind, ScalarKind::Decimal);
    /// assert_eq!((spec.precision, spec.scale), (Some(10), Some(2)));
    /// # Ok::<(), panproto_schema::SchemaError>(())
    /// ```
    #[must_use]
    pub fn for_vertex(schema: &Schema, protocol: &Protocol, vertex: &Name) -> Self {
        let kind = schema
            .vertices
            .get(vertex)
            .and_then(|v| protocol.value_kinds.get(v.kind.as_str()))
            .cloned()
            .unwrap_or(ScalarKind::Any);
        let mut spec = Self::new(kind);
        for constraint in schema.constraints.get(vertex).into_iter().flatten() {
            let sort = constraint.sort.as_str();
            if protocol.constraint_semantics.get(sort) != Some(&ConstraintSemantics::ValueParameter)
            {
                continue;
            }
            if let Ok(parameter) = ValueParameter::parse(sort, &constraint.value) {
                spec.apply(parameter);
            }
        }
        spec
    }

    /// Sets the field `parameter` names.
    pub fn apply(&mut self, parameter: ValueParameter) {
        match parameter {
            ValueParameter::Precision(n) => self.precision = Some(n),
            ValueParameter::Scale(n) => self.scale = Some(n),
            ValueParameter::MaxLength(n) => self.max_length = Some(n),
            ValueParameter::WithTimeZone(b) => self.with_time_zone = Some(b),
            ValueParameter::IntBits(n) => self.int_bits = Some(n),
            ValueParameter::Unsigned(b) => self.unsigned = b,
            ValueParameter::TimeUnit(unit) => self.time_unit = Some(unit),
            ValueParameter::TimeZone(zone) => self.time_zone = Some(zone),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use panproto_gat::Name;

    use super::{ScalarKind, ValueSpec};
    use crate::constraint_semantics::{ConstraintSemantics, LengthUnit, ValueParameter};
    use crate::{Protocol, Schema, SchemaBuilder, SchemaError};

    fn protocol() -> Protocol {
        let parameters = ValueParameter::SORTS
            .iter()
            .map(|sort| ((*sort).to_owned(), ConstraintSemantics::ValueParameter));
        Protocol {
            name: "demo".into(),
            obj_kinds: vec![
                "integer".into(),
                "timestamp".into(),
                "string".into(),
                "blob".into(),
            ],
            constraint_sorts: ValueParameter::SORTS
                .iter()
                .map(|s| (*s).to_owned())
                .chain(["maxLength".to_owned()])
                .collect(),
            constraint_semantics: parameters
                .chain([(
                    "maxLength".to_owned(),
                    ConstraintSemantics::MaxLength(LengthUnit::Chars),
                )])
                .collect(),
            value_kinds: BTreeMap::from([
                (
                    "integer".to_owned(),
                    ScalarKind::Int {
                        bits: 64,
                        signed: true,
                    },
                ),
                ("timestamp".to_owned(), ScalarKind::DateTime),
                ("string".to_owned(), ScalarKind::Str),
            ]),
            ..Protocol::default()
        }
    }

    fn schema(protocol: &Protocol) -> Result<Schema, SchemaError> {
        SchemaBuilder::new(protocol)
            .vertex("amount", "integer", None)?
            .vertex("at", "timestamp", None)?
            .vertex("label", "string", None)?
            .vertex("payload", "blob", None)?
            .constraint("amount", "precision", "12")
            .constraint("amount", "scale", "3")
            .constraint("amount", "int_bits", "32")
            .constraint("amount", "unsigned", "true")
            .constraint("at", "with_time_zone", "false")
            .constraint("label", "max_length", "80")
            .constraint("label", "maxLength", "5")
            .build()
    }

    #[test]
    fn value_spec_combines_the_kind_with_value_parameters() -> Result<(), SchemaError> {
        let protocol = protocol();
        let schema = schema(&protocol)?;
        let spec = |v: &str| ValueSpec::for_vertex(&schema, &protocol, &Name::from(v));

        assert_eq!(
            spec("amount"),
            ValueSpec {
                kind: ScalarKind::Int {
                    bits: 64,
                    signed: true
                },
                precision: Some(12),
                scale: Some(3),
                max_length: None,
                with_time_zone: None,
                int_bits: Some(32),
                unsigned: true,
                ..ValueSpec::new(ScalarKind::Any)
            }
        );
        assert_eq!(
            spec("at"),
            ValueSpec {
                with_time_zone: Some(false),
                ..ValueSpec::new(ScalarKind::DateTime)
            }
        );
        // `maxLength` means a length check here, not a value parameter, so
        // only `max_length` sets the field.
        assert_eq!(
            spec("label"),
            ValueSpec {
                max_length: Some(80),
                ..ValueSpec::new(ScalarKind::Str)
            }
        );
        Ok(())
    }

    #[test]
    fn a_vertex_of_an_unclassified_kind_holds_any_value() -> Result<(), SchemaError> {
        let protocol = protocol();
        let schema = schema(&protocol)?;
        assert_eq!(
            ValueSpec::for_vertex(&schema, &protocol, &Name::from("payload")),
            ValueSpec::new(ScalarKind::Any)
        );
        assert_eq!(
            ValueSpec::for_vertex(&schema, &protocol, &Name::from("missing")),
            ValueSpec::new(ScalarKind::Any)
        );
        Ok(())
    }

    #[test]
    fn an_ill_formed_parameter_sets_nothing_and_a_later_one_wins() -> Result<(), SchemaError> {
        let protocol = protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("amount", "integer", None)?
            .constraint("amount", "precision", "ten")
            .constraint("amount", "scale", "1")
            .constraint("amount", "scale", "4")
            .build()?;
        let spec = ValueSpec::for_vertex(&schema, &protocol, &Name::from("amount"));
        assert_eq!(spec.precision, None);
        assert_eq!(spec.scale, Some(4));
        Ok(())
    }

    #[test]
    fn a_parameter_sort_the_protocol_does_not_mean_as_one_is_ignored() -> Result<(), SchemaError> {
        let mut protocol = protocol();
        protocol
            .constraint_semantics
            .insert("precision".to_owned(), ConstraintSemantics::Annotation);
        let schema = schema(&protocol)?;
        let spec = ValueSpec::for_vertex(&schema, &protocol, &Name::from("amount"));
        assert_eq!(spec.precision, None);
        assert_eq!(spec.scale, Some(3));
        Ok(())
    }
}
