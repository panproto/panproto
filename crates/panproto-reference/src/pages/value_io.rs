//! Complete reference operations assigned to `value-io`.

pub mod tabular {
    //! Exact tabular route edge cases.

    /// A one-column empty record contains one empty cell.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-29`.
    pub use crate::value_io::tabular_empty_row as one_column_empty_cst;
    /// `_` is data, never an implicit missing token.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-28`.
    pub use crate::value_io::tabular_missing as missing_token_cst;
    /// TSV quotes are ordinary data.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-30`.
    pub use crate::value_io::tsv_quote as tsv_quote_cst;
}

pub mod toml {
    //! Exact TOML scalar-route laws.

    /// Decimal and big-integer CST token fidelity.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-25`.
    pub use crate::value_io::toml_scalar as decimal_bigint_cst;
}

pub mod implementation {
    //! Format-independent value-I/O law functions.
    //!
    //! The parser and writer below are independent, deliberately small route
    //! models. Tests compare production routes with these definitions; no
    //! production parser or writer is injected into the oracle.

    use std::collections::BTreeSet;

    use crate::coerce::{FidelityReason, LexClass, NumeralDialect, ScalarKind, ValueSpec};
    use crate::exact::Decimal;
    use crate::float::{
        CANONICAL_NAN_BITS, FloatValue, Format, NEGATIVE_CANONICAL_NAN_BITS, decode, shortest,
    };
    use crate::value::Value;

    /// A value-I/O route whose scalar syntax the reference models.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Route {
        /// JSON scalar syntax.
        Json,
        /// TOML scalar syntax.
        Toml,
        /// YAML 1.2 core scalar syntax.
        Yaml,
        /// XML character data.
        Xml,
        /// CSV cell text.
        Csv,
        /// TSV cell text.
        Tsv,
    }

    /// A typed parse or emission refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum ValueIoError {
        /// The input is not a scalar in this route.
        Malformed,
        /// The scalar cannot be represented on this route.
        NotRepresentable,
        /// Exact coercion refused the lexeme.
        Fidelity(FidelityReason),
    }

    /// A preserving concrete-syntax value.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct CstValue {
        /// Semantic scalar.
        pub value: Value,
        /// Exact source bytes.
        pub source: Vec<u8>,
    }

    const fn dialect(route: Route) -> NumeralDialect {
        match route {
            Route::Json | Route::Csv | Route::Tsv => NumeralDialect::Json,
            Route::Toml => NumeralDialect::Toml,
            Route::Yaml => NumeralDialect::Yaml12,
            Route::Xml => NumeralDialect::Xsd,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn parse_string(route: Route, text: &str) -> Result<String, ValueIoError> {
        if route == Route::Xml {
            return crate::xml::expand_entities(text, &std::collections::BTreeMap::new())
                .map_err(|_| ValueIoError::Malformed);
        }
        if route == Route::Csv {
            if !text.starts_with('"') {
                return (!text.contains([',', '"', '\r', '\n']))
                    .then(|| text.to_owned())
                    .ok_or(ValueIoError::Malformed);
            }
            if text.len() < 2 || !text.ends_with('"') {
                return Err(ValueIoError::Malformed);
            }
            let mut output = String::new();
            let mut characters = text[1..text.len() - 1].chars();
            while let Some(character) = characters.next() {
                if character == '"' && characters.next() != Some('"') {
                    return Err(ValueIoError::Malformed);
                }
                output.push(character);
            }
            return Ok(output);
        }
        if route == Route::Tsv {
            return (!text.contains(['\t', '\r', '\n']))
                .then(|| text.to_owned())
                .ok_or(ValueIoError::Malformed);
        }
        if matches!(route, Route::Toml | Route::Yaml)
            && text.starts_with('\'')
            && text.ends_with('\'')
            && text.len() >= 2
        {
            let body = &text[1..text.len() - 1];
            if route == Route::Toml {
                return (!body.contains(['\'', '\r', '\n']))
                    .then(|| body.to_owned())
                    .ok_or(ValueIoError::Malformed);
            }
            let mut output = String::new();
            let mut characters = body.chars().peekable();
            while let Some(character) = characters.next() {
                if character == '\'' && characters.next_if_eq(&'\'').is_none() {
                    return Err(ValueIoError::Malformed);
                }
                output.push(character);
            }
            return Ok(output);
        }
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
            return Err(ValueIoError::Malformed);
        }
        let mut out = String::new();
        let mut chars = text[1..text.len() - 1].chars().peekable();
        while let Some(character) = chars.next() {
            if character != '\\' {
                if character.is_control() {
                    return Err(ValueIoError::Malformed);
                }
                out.push(character);
                continue;
            }
            let escaped = chars.next().ok_or(ValueIoError::Malformed)?;
            match escaped {
                '"' | '\\' => out.push(escaped),
                '/' if route != Route::Toml => out.push(escaped),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let first = hex_quad(&mut chars)?;
                    let scalar = if (0xd800..=0xdbff).contains(&first) {
                        if chars.next() != Some('\\') || chars.next() != Some('u') {
                            return Err(ValueIoError::Malformed);
                        }
                        let second = hex_quad(&mut chars)?;
                        if !(0xdc00..=0xdfff).contains(&second) {
                            return Err(ValueIoError::Malformed);
                        }
                        0x1_0000
                            + ((u32::from(first) - 0xd800) << 10)
                            + (u32::from(second) - 0xdc00)
                    } else if (0xdc00..=0xdfff).contains(&first) {
                        return Err(ValueIoError::Malformed);
                    } else {
                        u32::from(first)
                    };
                    out.push(char::from_u32(scalar).ok_or(ValueIoError::Malformed)?);
                }
                'U' if route == Route::Toml => {
                    let mut scalar = 0_u32;
                    for _ in 0..8 {
                        scalar = scalar
                            .checked_mul(16)
                            .and_then(|value| {
                                chars
                                    .next()
                                    .and_then(|character| character.to_digit(16))
                                    .and_then(|digit| value.checked_add(digit))
                            })
                            .ok_or(ValueIoError::Malformed)?;
                    }
                    out.push(char::from_u32(scalar).ok_or(ValueIoError::Malformed)?);
                }
                '0' if route == Route::Yaml => out.push('\0'),
                'a' if route == Route::Yaml => out.push('\u{7}'),
                'v' if route == Route::Yaml => out.push('\u{b}'),
                'e' if route == Route::Yaml => out.push('\u{1b}'),
                ' ' if route == Route::Yaml => out.push(' '),
                '_' if route == Route::Yaml => out.push('\u{a0}'),
                'N' if route == Route::Yaml => out.push('\u{85}'),
                'L' if route == Route::Yaml => out.push('\u{2028}'),
                'P' if route == Route::Yaml => out.push('\u{2029}'),
                _ => return Err(ValueIoError::Malformed),
            }
        }
        Ok(out)
    }

    fn hex_quad(
        chars: &mut std::iter::Peekable<impl Iterator<Item = char>>,
    ) -> Result<u16, ValueIoError> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = chars
                .next()
                .and_then(|character| character.to_digit(16))
                .ok_or(ValueIoError::Malformed)?;
            value = value
                .checked_mul(16)
                .and_then(|value| value.checked_add(u16::try_from(digit).ok()?))
                .ok_or(ValueIoError::Malformed)?;
        }
        Ok(value)
    }

    fn parse_number(route: Route, text: &str) -> Result<Value, ValueIoError> {
        crate::coerce::coerce_dialect(
            text,
            &LexClass::Number,
            &ValueSpec {
                numeral: Some(dialect(route)),
                ..ValueSpec::of(ScalarKind::Any)
            },
        )
        .ok_or(ValueIoError::Malformed)?
        .map_err(ValueIoError::Fidelity)
    }

    /// Definition: Parse one scalar using an independent route grammar and
    /// exact numeric coercion.
    ///
    /// # Errors
    ///
    /// [`ValueIoError`] when the bytes are not one representable scalar.
    ///
    /// Book anchors: `L-value-io-4`.
    pub fn parse(route: Route, bytes: &[u8]) -> Result<Value, ValueIoError> {
        let raw = std::str::from_utf8(bytes).map_err(|_| ValueIoError::Malformed)?;
        let text = if matches!(route, Route::Xml | Route::Csv | Route::Tsv) {
            raw
        } else {
            raw.trim()
        };
        match (route, text) {
            (Route::Json, "null") | (Route::Yaml, "null" | "Null" | "NULL" | "~") => {
                Ok(Value::Null)
            }
            (Route::Json | Route::Toml | Route::Yaml, "true") => Ok(Value::Bool(true)),
            (Route::Json | Route::Toml | Route::Yaml, "false") => Ok(Value::Bool(false)),
            (Route::Xml | Route::Csv | Route::Tsv, _) => parse_string(route, text).map(Value::Str),
            (_, value) if value.starts_with('"') => parse_string(route, value).map(Value::Str),
            (Route::Toml, value)
                if crate::temporal::parse_union(value)
                    .is_ok_and(|parsed| temporal(Route::Toml, &parsed)) =>
            {
                crate::temporal::parse_union(value)
                    .map_err(|_| ValueIoError::Malformed)
                    .map(Value::Temporal)
            }
            (Route::Yaml, value) => {
                parse_number(route, value).or_else(|_| Ok(Value::Str(value.to_owned())))
            }
            (_, value) => parse_number(route, value),
        }
    }

    fn decimal_text(value: &Decimal) -> String {
        match value {
            Decimal::Finite(value) => format!(
                "{}{}e{}",
                if value.negative { "-" } else { "" },
                value.digits,
                value.exponent
            ),
            Decimal::Infinite { negative } => {
                if *negative { "-Infinity" } else { "Infinity" }.to_owned()
            }
            Decimal::NaN { negative, .. } => if *negative { "-NaN" } else { "NaN" }.to_owned(),
        }
    }

    fn quote(text: &str) -> String {
        let mut out = String::from("\"");
        for character in text.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                other if other.is_control() => {
                    use std::fmt::Write as _;
                    let _ = write!(&mut out, "\\u{:04x}", u32::from(other));
                }
                other => out.push(other),
            }
        }
        out.push('"');
        out
    }

    fn xml_quote(text: &str) -> String {
        text.chars().fold(String::new(), |mut output, character| {
            output.push_str(match character {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '\'' => "&apos;",
                '"' => "&quot;",
                _ => {
                    output.push(character);
                    return output;
                }
            });
            output
        })
    }

    fn csv_quote(text: &str) -> String {
        if text.contains([',', '"', '\r', '\n']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text.to_owned()
        }
    }

    fn numeric_text(route: Route, value: &Value) -> Result<String, ValueIoError> {
        let text = match value {
            Value::Int(value) => value.to_string(),
            Value::BigInt(value) => format!(
                "{}{}e{}",
                if value.negative { "-" } else { "" },
                value.significand,
                value.zeros
            ),
            Value::Decimal(Decimal::Finite(value)) if route == Route::Xml => {
                let exponent = value
                    .exponent
                    .to_i64()
                    .filter(|exponent| *exponent <= 0)
                    .ok_or(ValueIoError::NotRepresentable)?;
                let digits = value.digits.to_string();
                let scale = usize::try_from(exponent.unsigned_abs())
                    .ok()
                    .filter(|scale| *scale <= 1_048_576)
                    .ok_or(ValueIoError::NotRepresentable)?;
                let body = if scale == 0 {
                    digits
                } else if scale < digits.len() {
                    let at = digits.len() - scale;
                    format!("{}.{}", &digits[..at], &digits[at..])
                } else {
                    format!("0.{}{}", "0".repeat(scale - digits.len()), digits)
                };
                format!("{}{}", if value.negative { "-" } else { "" }, body)
            }
            Value::Decimal(value @ Decimal::Finite(_)) => decimal_text(value),
            Value::Decimal(Decimal::Infinite { .. } | Decimal::NaN { .. }) => {
                return Err(ValueIoError::NotRepresentable);
            }
            Value::Float(bits) => match decode(Format::B64, *bits) {
                FloatValue::Finite { .. } => shortest(Format::B64, *bits)
                    .map(|value| value.to_scientific())
                    .ok_or(ValueIoError::NotRepresentable)?,
                FloatValue::Infinite { negative } => match route {
                    Route::Toml => if negative { "-inf" } else { "inf" }.to_owned(),
                    Route::Yaml => if negative { "-.inf" } else { ".inf" }.to_owned(),
                    Route::Xml => if negative { "-INF" } else { "INF" }.to_owned(),
                    Route::Json | Route::Csv | Route::Tsv => {
                        return Err(ValueIoError::NotRepresentable);
                    }
                },
                FloatValue::NaN { negative, .. }
                    if *bits
                        == if negative {
                            NEGATIVE_CANONICAL_NAN_BITS
                        } else {
                            CANONICAL_NAN_BITS
                        } =>
                {
                    match route {
                        Route::Toml => if negative { "-nan" } else { "nan" }.to_owned(),
                        Route::Yaml => if negative { "-.nan" } else { ".nan" }.to_owned(),
                        Route::Xml => if negative { "-NaN" } else { "NaN" }.to_owned(),
                        Route::Json | Route::Csv | Route::Tsv => {
                            return Err(ValueIoError::NotRepresentable);
                        }
                    }
                }
                FloatValue::NaN { .. } => return Err(ValueIoError::NotRepresentable),
            },
            _ => return Err(ValueIoError::NotRepresentable),
        };
        Ok(text)
    }

    /// Definition: Emit one scalar using the route's canonical scalar syntax.
    ///
    /// # Errors
    ///
    /// [`ValueIoError::NotRepresentable`] when the route has no scalar form.
    ///
    /// Book anchors: `L-value-io-2`.
    pub fn emit(route: Route, value: &Value) -> Result<Vec<u8>, ValueIoError> {
        let text = match value {
            Value::Null if matches!(route, Route::Json | Route::Yaml) => "null".to_owned(),
            Value::Bool(value) if !matches!(route, Route::Xml | Route::Csv | Route::Tsv) => {
                value.to_string()
            }
            Value::Str(value) if route == Route::Xml => xml_quote(value),
            Value::Str(value) if route == Route::Csv => csv_quote(value),
            Value::Str(value) if route == Route::Tsv => {
                if value.contains(['\t', '\r', '\n']) {
                    return Err(ValueIoError::NotRepresentable);
                }
                value.clone()
            }
            Value::Str(value) => quote(value),
            Value::Temporal(value) if temporal(route, value) => match route {
                Route::Json => quote(&value.to_string()),
                Route::Toml | Route::Yaml | Route::Xml | Route::Csv | Route::Tsv => {
                    value.to_string()
                }
            },
            value => numeric_text(route, value)?,
        };
        Ok(text.into_bytes())
    }

    /// Definition: Canonical round trip: parsing emitted values is the identity.
    ///
    /// Book anchors: `L-value-io-1`.
    #[must_use]
    pub fn round_trip(route: Route, value: &Value) -> bool {
        let Ok(bytes) = emit(route, value) else {
            return false;
        };
        if let Value::Temporal(temporal) = value {
            let Some(form) = temporal.lexical_form() else {
                return false;
            };
            let Ok(written) = std::str::from_utf8(&bytes) else {
                return false;
            };
            let written = if route == Route::Json {
                let Ok(decoded) = parse_string(route, written) else {
                    return false;
                };
                decoded
            } else {
                written.to_owned()
            };
            return crate::temporal::parse_as(form, &written).as_ref() == Some(temporal);
        }
        let Ok(written) = std::str::from_utf8(&bytes) else {
            return false;
        };
        let typed = match value {
            Value::BigInt(_) => Some(ScalarKind::BigInt),
            Value::Decimal(_) => Some(ScalarKind::Decimal),
            Value::Float(_) => Some(ScalarKind::Float { bits: 64 }),
            _ => None,
        };
        typed.map_or_else(
            || parse(route, &bytes).is_ok_and(|parsed| parsed == *value),
            |kind| {
                crate::coerce::coerce_dialect(
                    written,
                    &LexClass::Number,
                    &ValueSpec {
                        numeral: Some(dialect(route)),
                        ..ValueSpec::of(kind)
                    },
                )
                .is_some_and(|result| result.as_ref() == Ok(value))
            },
        )
    }

    /// Definition: Preserving round trip: an unedited document is byte-exact.
    ///
    /// Book anchors: `L-value-io-3`.
    #[must_use]
    pub fn preserve(source: &[u8], emitted: &[u8]) -> bool {
        source == emitted
    }

    /// Definition: Whether a route can represent a temporal value by parsing its display
    /// back to the same value.
    ///
    /// Book anchors: `L-value-io-5`.
    #[must_use]
    pub fn temporal(route: Route, value: &crate::temporal::Temporal) -> bool {
        use crate::temporal::{Offset, OffsetForm, TemporalForm};
        if !crate::temporal::display_parse(value) {
            return false;
        }
        match route {
            Route::Toml => {
                if value.infinity.is_some() {
                    return false;
                }
                let full_date = value.date.is_some_and(|date| {
                    date.year
                        .is_some_and(|year| year.width == 4 && !year.negative_zero)
                        && date.month.is_some()
                        && date.day.is_some()
                });
                let full_time = value.time.as_ref().is_some_and(|time| {
                    time.hour.is_some() && time.minute.is_some() && time.second.is_some()
                });
                let offset = matches!(
                    value.offset,
                    Offset::Local
                        | Offset::Zulu(_)
                        | Offset::Fixed(_, OffsetForm::Extended)
                        | Offset::NegZero(OffsetForm::Extended)
                );
                match value.form() {
                    Some(TemporalForm::Date) => full_date && value.offset == Offset::Local,
                    Some(TemporalForm::Time) => full_time && value.offset == Offset::Local,
                    Some(TemporalForm::DateTime) => full_date && full_time && offset,
                    None => false,
                }
            }
            Route::Json => value.is_complete(),
            Route::Yaml | Route::Xml | Route::Csv | Route::Tsv => true,
        }
    }

    /// Parse one scalar while preserving its exact bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ValueIoError`] when the route grammar rejects the bytes.
    ///
    /// Book anchors: `L-value-io-22`.
    pub fn parse_cst(route: Route, bytes: &[u8]) -> Result<CstValue, ValueIoError> {
        Ok(CstValue {
            value: parse(route, bytes)?,
            source: bytes.to_vec(),
        })
    }

    /// Emit an unedited CST byte-for-byte, or canonically emit a replacement.
    ///
    /// # Errors
    ///
    /// Returns [`ValueIoError`] when a replacement is not representable on
    /// the selected route.
    ///
    /// Book anchors: `L-value-io-20`, `L-value-io-21`.
    pub fn emit_cst(
        route: Route,
        cst: &CstValue,
        replacement: Option<&Value>,
    ) -> Result<Vec<u8>, ValueIoError> {
        replacement.map_or_else(|| Ok(cst.source.clone()), |value| emit(route, value))
    }

    /// The independent CST parse/emit round trip.
    #[must_use]
    pub fn round_trip_cst(route: Route, bytes: &[u8]) -> bool {
        parse_cst(route, bytes)
            .and_then(|cst| emit_cst(route, &cst, None))
            .is_ok_and(|emitted| emitted == bytes)
    }

    /// Definition: Agreement of the generic byte writer with the route-specific writer.
    ///
    /// Book anchors: `L-value-io-6`.
    #[must_use]
    pub fn bytes(generic: &[u8], route: &[u8]) -> bool {
        generic == route
    }

    /// Definition: Agreement of the exact-decimal writer with the route-specific writer.
    ///
    /// Book anchors: `L-value-io-7`.
    #[must_use]
    pub fn decimal(generic: &str, route: &str) -> bool {
        generic == route
    }

    /// Definition: Raw numeric fidelity for an unedited preserving cell.
    ///
    /// Book anchors: `L-value-io-8`.
    #[must_use]
    pub fn raw_numeric(source_lexeme: &str, emitted_lexeme: &str) -> bool {
        source_lexeme == emitted_lexeme
    }

    /// Definition: Whether object members contain a duplicate name and therefore require
    /// the route's duplicate-member refusal.
    ///
    /// Book anchors: `L-value-io-9`.
    #[must_use]
    pub fn duplicate_member<T>(members: &[(String, T)]) -> bool {
        let mut names = BTreeSet::new();
        members.iter().any(|(name, _)| !names.insert(name))
    }

    /// Definition: Encoding and byte-order-mark preservation.
    ///
    /// Book anchors: `L-value-io-10`.
    #[must_use]
    pub fn encoding(source: &[u8], emitted: &[u8]) -> bool {
        preserve(source, emitted)
    }

    /// Definition: TOML scalar token fidelity.
    ///
    /// Book anchors: `L-value-io-11`.
    #[must_use]
    pub fn toml_scalar(source_token: &str, emitted_token: &str) -> bool {
        source_token == emitted_token
    }

    /// XML entity expansion delegates to the bounded XML reference.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-12`.
    pub use crate::xml::expand_entities as xml_entity;

    /// XML source-encoding preservation.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-13`.
    pub use encoding as xml_encoding;

    /// Definition: TSV has no implicit comment syntax.
    ///
    /// Book anchors: `L-value-io-14`.
    #[must_use]
    pub const fn tsv_comment(line: &str) -> bool {
        !line.is_empty()
    }

    /// Definition: `_` is an ordinary tabular text cell.
    ///
    /// Book anchors: `L-value-io-15`.
    #[must_use]
    pub fn tabular_missing(cell: &str) -> bool {
        cell == "_"
    }

    /// Definition: An empty line after a one-column header is one empty cell.
    ///
    /// Book anchors: `L-value-io-16`.
    #[must_use]
    pub const fn tabular_empty_row(column_count: usize, line: &str) -> bool {
        column_count == 1 && line.is_empty()
    }

    /// Definition: A TSV cell is writable exactly when it contains no delimiter or line
    /// break; `"` is ordinary data.
    ///
    /// Book anchors: `L-value-io-17`.
    #[must_use]
    pub fn tsv_quote(cell: &str) -> bool {
        !cell
            .bytes()
            .any(|byte| matches!(byte, b'\t' | b'\r' | b'\n'))
    }

    /// Definition: XML expanded-name identity.
    ///
    /// Book anchors: `L-value-io-18`.
    #[must_use]
    pub fn xml_expanded_name(namespace: &str, local: &str) -> (String, String) {
        (namespace.to_owned(), local.to_owned())
    }

    /// Agreement of a generic blob writer with a route arm.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-19`.
    pub use bytes as blob_writer;

    /// CST duplicate-member refusal uses the same member-name predicate.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-23`.
    pub use duplicate_member as duplicate_member_cst;
    /// CST encoding and BOM preservation uses the same byte equality.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-24`.
    pub use encoding as encoding_bom_cst;
    /// CST byte preservation uses the same byte equality.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-21`.
    pub use preserve as preserve_cst;
}

pub mod xml {
    //! Small, bounded XML reference operations.

    use std::collections::BTreeMap;

    /// Maximum recursively expanded bytes.
    pub const MAX_XML_ENTITY_EXPANSION_BYTES: usize = 1_048_576;
    /// Maximum recursive entity nesting.
    pub const MAX_XML_ENTITY_DEPTH: usize = 16;

    /// Why bounded entity expansion is undefined.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum EntityError {
        /// An entity name has no declaration.
        Undeclared(String),
        /// The declaration graph is recursive.
        Recursive(String),
        /// Nesting exceeds [`MAX_XML_ENTITY_DEPTH`].
        Depth,
        /// Expanded bytes exceed [`MAX_XML_ENTITY_EXPANSION_BYTES`].
        Bytes,
        /// A reference is syntactically incomplete.
        Malformed,
    }

    fn append(output: &mut String, text: &str) -> Result<(), EntityError> {
        let new_len = output
            .len()
            .checked_add(text.len())
            .ok_or(EntityError::Bytes)?;
        if new_len > MAX_XML_ENTITY_EXPANSION_BYTES {
            return Err(EntityError::Bytes);
        }
        output.push_str(text);
        Ok(())
    }

    fn character_reference(name: &str) -> Option<Result<char, EntityError>> {
        let digits = name.strip_prefix('#')?;
        let (radix, digits) = digits
            .strip_prefix('x')
            .map_or((10, digits), |hex| (16, hex));
        if digits.is_empty() {
            return Some(Err(EntityError::Malformed));
        }
        let Ok(value) = u32::from_str_radix(digits, radix) else {
            return Some(Err(EntityError::Malformed));
        };
        let valid = matches!(
            value,
            0x9 | 0xa | 0xd | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x0001_0000..=0x0010_ffff
        );
        Some(if valid {
            char::from_u32(value).ok_or(EntityError::Malformed)
        } else {
            Err(EntityError::Malformed)
        })
    }

    const fn xml_name_start(character: char) -> bool {
        matches!(
            character,
            ':' | 'A'..='Z' | '_' | 'a'..='z'
                | '\u{c0}'..='\u{d6}' | '\u{d8}'..='\u{f6}' | '\u{f8}'..='\u{2ff}'
                | '\u{370}'..='\u{37d}' | '\u{37f}'..='\u{1fff}'
                | '\u{200c}'..='\u{200d}' | '\u{2070}'..='\u{218f}'
                | '\u{2c00}'..='\u{2fef}' | '\u{3001}'..='\u{d7ff}'
                | '\u{f900}'..='\u{fdcf}' | '\u{fdf0}'..='\u{fffd}'
                | '\u{10000}'..='\u{effff}'
        )
    }

    const fn xml_name_continue(character: char) -> bool {
        xml_name_start(character)
            || matches!(
                character,
                '-' | '.' | '0'..='9' | '\u{b7}' | '\u{300}'..='\u{36f}' | '\u{203f}'..='\u{2040}'
            )
    }

    fn xml_name(name: &str) -> bool {
        let mut characters = name.chars();
        characters.next().is_some_and(xml_name_start) && characters.all(xml_name_continue)
    }

    fn expand(
        text: &str,
        declarations: &BTreeMap<String, String>,
        stack: &mut Vec<String>,
        output: &mut String,
    ) -> Result<(), EntityError> {
        if stack.len() > MAX_XML_ENTITY_DEPTH {
            return Err(EntityError::Depth);
        }
        let mut rest = text;
        while let Some(start) = rest.find('&') {
            append(output, &rest[..start])?;
            let after = &rest[start + 1..];
            let Some(end) = after.find(';') else {
                return Err(EntityError::Malformed);
            };
            let name = &after[..end];
            if let Some(replacement) = match name {
                "amp" => Some("&"),
                "lt" => Some("<"),
                "gt" => Some(">"),
                "apos" => Some("'"),
                "quot" => Some("\""),
                _ => None,
            } {
                append(output, replacement)?;
                rest = &after[end + 1..];
                continue;
            }
            if let Some(character) = character_reference(name) {
                let mut bytes = [0; 4];
                append(output, character?.encode_utf8(&mut bytes))?;
                rest = &after[end + 1..];
                continue;
            }
            if !xml_name(name) {
                return Err(EntityError::Malformed);
            }
            let replacement = declarations
                .get(name)
                .ok_or_else(|| EntityError::Undeclared(name.to_owned()))?;
            if stack.iter().any(|item| item == name) {
                return Err(EntityError::Recursive(name.to_owned()));
            }
            stack.push(name.to_owned());
            expand(replacement, declarations, stack, output)?;
            stack.pop();
            rest = &after[end + 1..];
        }
        append(output, rest)
    }

    /// Expand internal general entities with explicit byte and depth bounds.
    ///
    /// # Errors
    ///
    /// [`EntityError`] for an undeclared, recursive, malformed or over-budget
    /// expansion.
    pub fn expand_entities(
        text: &str,
        declarations: &BTreeMap<String, String>,
    ) -> Result<String, EntityError> {
        let mut output = String::new();
        expand(text, declarations, &mut Vec::new(), &mut output)?;
        Ok(output)
    }

    /// The CST preserving byte round trip.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-27`.
    pub use crate::value_io::preserve as round_trip_cst;
    /// The CST expanded-name function.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-31`.
    pub use crate::value_io::xml_expanded_name as expanded_name_cst;
    /// The CST bounded-expansion operation.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-26`.
    pub use expand_entities as bounded_expansion_cst;
}

pub use self::implementation::*;
pub use self::tabular::{missing_token_cst, one_column_empty_cst, tsv_quote_cst};
pub use self::toml::decimal_bigint_cst;
pub use self::xml::{bounded_expansion_cst, expanded_name_cst};
