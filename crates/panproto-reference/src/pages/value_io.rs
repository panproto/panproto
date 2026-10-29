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
    /// Definition: An unedited CST re-emits the exact token it received.
    ///
    /// Book anchors: `L-value-io-25`.
    #[must_use]
    pub fn decimal_bigint_cst(source_token: &str, emitted_token: &str) -> bool {
        source_token == emitted_token
    }
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
        /// A special decimal or a non-finite binary float has no spelling on
        /// this route that reads back as the same value.
        NonFinite,
        /// The canonical spelling exceeds the output allowance.
        LimitExceeded,
        /// Exact coercion refused the lexeme.
        Fidelity(FidelityReason),
    }

    /// The two non-value states whose distinction VF8 requires routes to
    /// retain.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Presence {
        /// An explicitly null field or cell.
        Null,
        /// A field or cell not present in the instance.
        Absent,
    }

    /// The canonical/default-route disposition of a null or absent position.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum PresenceOutcome {
        /// The route writes its native null syntax.
        NullSyntax,
        /// The route omits the position.
        Omitted,
        /// The route refuses null because no null token is configured.
        NullNotRepresentable,
        /// The route refuses absence because a positional cell cannot vanish.
        AbsentNotRepresentable,
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

    fn parse_string(route: Route, text: &str) -> Result<String, ValueIoError> {
        if route == Route::Xml {
            if text.contains('<') || text.contains("]]>") {
                return Err(ValueIoError::Malformed);
            }
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            return crate::xml::expand_entities(&normalized, &std::collections::BTreeMap::new())
                .map_err(|_| ValueIoError::Malformed);
        }
        if route == Route::Csv {
            return parse_csv_string(text);
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
            return parse_single_quoted(route, &text[1..text.len() - 1]);
        }
        parse_double_quoted(route, text)
    }

    fn parse_csv_string(text: &str) -> Result<String, ValueIoError> {
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
        Ok(output)
    }

    /// A single-quoted body. TOML's literal string holds TOML 1.0's
    /// `literal-char`: tab and every scalar from U+0020 except `'` and DEL.
    /// YAML's single-quoted scalar doubles `'` and holds `nb-json`.
    fn parse_single_quoted(route: Route, body: &str) -> Result<String, ValueIoError> {
        if route == Route::Toml {
            let literal_char = |character: char| {
                character == '\t'
                    || (character >= '\u{20}' && character != '\'' && character != '\u{7f}')
            };
            return body
                .chars()
                .all(literal_char)
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
        Ok(output)
    }

    fn parse_double_quoted(route: Route, text: &str) -> Result<String, ValueIoError> {
        let bytes = text.as_bytes();
        if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
            return Err(ValueIoError::Malformed);
        }
        let mut out = String::new();
        let mut chars = text[1..text.len() - 1].chars().peekable();
        while let Some(character) = chars.next() {
            if character != '\\' {
                if !literal_in_double_quotes(route, character) {
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

    /// Whether `character` may stand unescaped inside a double-quoted string:
    /// RFC 8259's `unescaped` (every scalar from U+0020) on JSON, TOML 1.0's
    /// `basic-unescaped` with its tab (no C0 control but tab, and no DEL) on
    /// TOML, and YAML 1.2.2's `nb-json` (tab or any scalar from U+0020) on
    /// YAML. Quote and backslash never reach this test. A line break is
    /// refused on YAML as well, since one scalar line is the modelled form.
    fn literal_in_double_quotes(route: Route, character: char) -> bool {
        let code = u32::from(character);
        match route {
            Route::Json => code >= 0x20,
            Route::Toml => (code == 0x09 || code >= 0x20) && code != 0x7f,
            Route::Yaml | Route::Xml | Route::Csv | Route::Tsv => code == 0x09 || code >= 0x20,
        }
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

    /// Whether `text` is one complete RFC 8259 JSON number token.
    fn json_number_token(text: &str) -> bool {
        let bytes = text.as_bytes();
        let mut index = usize::from(bytes.first() == Some(&b'-'));
        match bytes.get(index) {
            Some(b'0') => index += 1,
            Some(b'1'..=b'9') => {
                index += 1;
                while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                    index += 1;
                }
            }
            _ => return false,
        }
        if bytes.get(index) == Some(&b'.') {
            index += 1;
            let fraction_start = index;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if index == fraction_start {
                return false;
            }
        }
        if matches!(bytes.get(index), Some(b'e' | b'E')) {
            index += 1;
            if matches!(bytes.get(index), Some(b'+' | b'-')) {
                index += 1;
            }
            let exponent_start = index;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if index == exponent_start {
                return false;
            }
        }
        index == bytes.len()
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
        let raw = std::str::from_utf8(bytes).map_err(|_| {
            if route == Route::Xml {
                ValueIoError::Fidelity(FidelityReason::InvalidEncoding)
            } else {
                ValueIoError::Malformed
            }
        })?;
        if matches!(route, Route::Json | Route::Toml | Route::Yaml)
            && raw
                .chars()
                .any(|character| matches!(u32::from(character), 0x0..=0x8 | 0xb | 0xc | 0xe..=0x1f))
        {
            return Err(ValueIoError::Malformed);
        }
        let text = if matches!(route, Route::Xml | Route::Csv | Route::Tsv) {
            raw
        } else {
            raw.trim_matches([' ', '\t', '\r', '\n'])
        };
        if text.is_empty() && matches!(route, Route::Json | Route::Toml) {
            return Err(ValueIoError::Malformed);
        }
        match (route, text) {
            (Route::Json, "null") | (Route::Yaml, "null" | "Null" | "NULL" | "~") => {
                Ok(Value::Null)
            }
            (Route::Yaml, "") => Ok(Value::Null),
            (Route::Json | Route::Toml | Route::Yaml, "true") => Ok(Value::Bool(true)),
            (Route::Json | Route::Toml | Route::Yaml, "false") => Ok(Value::Bool(false)),
            (Route::Xml | Route::Csv | Route::Tsv, _) => parse_string(route, text).map(Value::Str),
            (_, value)
                if value.starts_with('"')
                    || (matches!(route, Route::Toml | Route::Yaml) && value.starts_with('\'')) =>
            {
                parse_string(route, value).map(Value::Str)
            }
            (Route::Json, value)
                if (value.starts_with('[') && value.ends_with(']'))
                    || (value.starts_with('{') && value.ends_with('}')) =>
            {
                Err(ValueIoError::Malformed)
            }
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
            (Route::Json, value) if json_number_token(value) => parse_number(route, value),
            (Route::Toml, value)
                if value.starts_with(['+', '-', '.'])
                    || value.as_bytes().first().is_some_and(u8::is_ascii_digit) =>
            {
                parse_number(route, value)
            }
            (Route::Toml, value) if value.starts_with(['i', 'n']) => {
                parse_number(route, value).map_err(|_| ValueIoError::Malformed)
            }
            (Route::Json | Route::Toml, _) => Err(ValueIoError::Malformed),
        }
    }

    fn decimal_text(value: &Decimal) -> String {
        match value {
            Decimal::Finite(value) => {
                let sign = if value.negative { "-" } else { "" };
                let digits = value.digits.to_string();
                let Some(exponent) = value.exponent.to_i64() else {
                    return format!("{sign}{digits}e{}", value.exponent);
                };
                if exponent > 0
                    || exponent < -i64::try_from(digits.len().saturating_add(6)).unwrap_or(i64::MAX)
                {
                    return format!("{sign}{digits}e{exponent}");
                }
                let scale = usize::try_from(exponent.unsigned_abs()).unwrap_or(0);
                if scale == 0 {
                    format!("{sign}{digits}")
                } else if scale >= digits.len() {
                    format!("{sign}0.{}{digits}", "0".repeat(scale - digits.len()))
                } else {
                    let (integer, fraction) = digits.split_at(digits.len() - scale);
                    format!("{sign}{integer}.{fraction}")
                }
            }
            Decimal::Infinite { negative } => {
                if *negative { "-Infinity" } else { "Infinity" }.to_owned()
            }
            Decimal::NaN { negative, .. } => if *negative { "-NaN" } else { "NaN" }.to_owned(),
        }
    }

    /// A double-quoted string: quote and backslash escaped, the five named
    /// short escapes, and every other C0 control as a four-digit `\u`
    /// escape. JSON writes hex digits in lowercase and every other scalar
    /// literally. TOML, whose basic strings forbid a literal DEL, also
    /// escapes U+007F, in TOML's uppercase hex digits. YAML writes the
    /// remaining scalars literally, as `nb-json` admits them.
    fn quote(route: Route, text: &str) -> String {
        use std::fmt::Write as _;
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
                other if other <= '\u{1f}' && route == Route::Toml => {
                    let _ = write!(&mut out, "\\u{:04X}", u32::from(other));
                }
                other if other <= '\u{1f}' => {
                    let _ = write!(&mut out, "\\u{:04x}", u32::from(other));
                }
                '\u{7f}' if route == Route::Toml => out.push_str("\\u007F"),
                other => out.push(other),
            }
        }
        out.push('"');
        out
    }

    /// XML character data: the five markup characters as predefined
    /// entities and carriage return as `&#xD;`, since XML 1.0 Sec 2.11 reads
    /// a literal carriage return as a line feed. A scalar outside XML 1.0
    /// `Char` has no spelling, literal or referenced, and is refused.
    fn xml_quote(text: &str) -> Result<String, ValueIoError> {
        let mut output = String::new();
        for character in text.chars() {
            match character {
                '&' => output.push_str("&amp;"),
                '<' => output.push_str("&lt;"),
                '>' => output.push_str("&gt;"),
                '\'' => output.push_str("&apos;"),
                '"' => output.push_str("&quot;"),
                '\r' => output.push_str("&#xD;"),
                other
                    if matches!(
                        u32::from(other),
                        0x9 | 0xa | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x0001_0000..=0x0010_ffff
                    ) =>
                {
                    output.push(other);
                }
                _ => return Err(ValueIoError::NotRepresentable),
            }
        }
        Ok(output)
    }

    /// A CSV cell written without quoting: a cell holding a comma, quote,
    /// carriage return or line feed has no unquoted spelling and is refused,
    /// since the canonical CSV writer writes no quoted field.
    fn csv_text(text: &str) -> Result<String, ValueIoError> {
        if text.contains([',', '"', '\r', '\n']) {
            Err(ValueIoError::NotRepresentable)
        } else {
            Ok(text.to_owned())
        }
    }

    /// The output allowance of a canonical writer, in bytes.
    pub const OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

    /// A big integer in its digits when they fit [`OUTPUT_BYTES`], else in
    /// the exponent form `[-]{significand}e{zeros}` on a route whose numeral
    /// grammar reads it back under `BigInt` (every route but XML, whose XSD
    /// integer lexemes have no exponent), else refused.
    fn bigint_text(
        route: Route,
        value: &crate::exact::BigIntValue,
    ) -> Result<String, ValueIoError> {
        let sign = if value.negative { "-" } else { "" };
        let significand = value.significand.to_string();
        let zeros = value.zeros.to_u64();
        let written = zeros.and_then(|zeros| {
            zeros
                .checked_add(u64::try_from(significand.len()).ok()?)?
                .checked_add(u64::from(value.negative))
        });
        if written.is_some_and(|length| length <= OUTPUT_BYTES)
            && let Some(zeros) = zeros.and_then(|zeros| usize::try_from(zeros).ok())
        {
            return Ok(format!("{sign}{significand}{}", "0".repeat(zeros)));
        }
        let compact = format!("{sign}{significand}e{}", value.zeros);
        if route != Route::Xml && u64::try_from(compact.len()).is_ok_and(|len| len <= OUTPUT_BYTES)
        {
            return Ok(compact);
        }
        Err(ValueIoError::LimitExceeded)
    }

    fn numeric_text(route: Route, value: &Value) -> Result<String, ValueIoError> {
        let text = match value {
            Value::Int(value) => value.to_string(),
            Value::BigInt(value) => bigint_text(route, value)?,
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
                return Err(ValueIoError::NonFinite);
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
                        return Err(ValueIoError::NonFinite);
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
                            return Err(ValueIoError::NonFinite);
                        }
                    }
                }
                FloatValue::NaN { .. }
                    if matches!(route, Route::Json | Route::Csv | Route::Tsv) =>
                {
                    return Err(ValueIoError::NonFinite);
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
            Value::Str(value) if route == Route::Xml => xml_quote(value)?,
            Value::Str(value) if route == Route::Csv => csv_text(value)?,
            Value::Str(value) if route == Route::Tsv => {
                if value.contains(['\t', '\r', '\n']) {
                    return Err(ValueIoError::NotRepresentable);
                }
                value.clone()
            }
            Value::Str(value) => quote(route, value),
            Value::Temporal(value) if temporal(route, value) => match route {
                Route::Json => quote(route, &value.to_string()),
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
            Value::Int(_) if matches!(route, Route::Xml | Route::Csv | Route::Tsv) => {
                Some(ScalarKind::Int {
                    bits: 64,
                    signed: true,
                })
            }
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

    /// Definition: Canonical/default handling of null and absence under VF8.
    ///
    /// Book anchors: `L-value-io-3`.
    #[must_use]
    pub const fn presence(route: Route, presence: Presence) -> PresenceOutcome {
        match (route, presence) {
            (Route::Json | Route::Yaml | Route::Xml, Presence::Null) => PresenceOutcome::NullSyntax,
            (Route::Json | Route::Toml | Route::Yaml | Route::Xml, Presence::Absent) => {
                PresenceOutcome::Omitted
            }
            (Route::Toml | Route::Csv | Route::Tsv, Presence::Null) => {
                PresenceOutcome::NullNotRepresentable
            }
            (Route::Csv | Route::Tsv, Presence::Absent) => PresenceOutcome::AbsentNotRepresentable,
        }
    }

    /// Definition: An unedited CST document is preserved byte-for-byte.
    ///
    /// Book anchors: `L-value-io-21`.
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
                    date.year.is_some_and(|year| {
                        year.value >= 0 && year.width == 4 && !year.negative_zero
                    }) && date.month.is_some()
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
            Route::Yaml => value.offset != Offset::Local,
            Route::Xml | Route::Csv | Route::Tsv => true,
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

    /// Definition: A canonical/default route emits UTF-8 without a byte-order
    /// mark, independently of the accepted source encoding.
    ///
    /// Book anchors: `L-value-io-10`.
    #[must_use]
    pub fn encoding(emitted: &[u8]) -> bool {
        std::str::from_utf8(emitted).is_ok()
            && !emitted.starts_with(&[0xef, 0xbb, 0xbf])
            && !emitted.starts_with(&[0xff, 0xfe])
            && !emitted.starts_with(&[0xfe, 0xff])
    }

    /// Definition: Canonical TOML scalar emission preserves the exact carrier.
    ///
    /// Both tokens are read under the declared exact kind, whatever spelling
    /// the writer chose: under
    /// `ScalarKind::Decimal` as exact decimal carriers, so the scale and the
    /// sign of zero are observed; under `ScalarKind::BigInt` as exact
    /// integers, whatever their exponent. Every other kind is outside the
    /// predicate's domain and gives `false`.
    ///
    /// Book anchors: `L-value-io-11`.
    #[must_use]
    pub fn toml_scalar(kind: &ScalarKind, source_token: &str, emitted_token: &str) -> bool {
        match kind {
            ScalarKind::Decimal => crate::exact::parse_decimal(source_token)
                .zip(crate::exact::parse_decimal(emitted_token))
                .is_some_and(|(source, emitted)| source == emitted),
            ScalarKind::BigInt => {
                let integer = |token: &str| {
                    crate::exact::parse_numeral(token)
                        .and_then(|numeral| crate::exact::integer_of_numeral(&numeral))
                };
                integer(source_token)
                    .zip(integer(emitted_token))
                    .is_some_and(|(source, emitted)| source == emitted)
            }
            _ => false,
        }
    }

    /// XML entity expansion delegates to the bounded XML reference.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-12`.
    pub use crate::xml::expand_entities as xml_entity;

    /// Definition: Canonical XML is UTF-8 without a byte-order mark or a
    /// declaration naming another encoding.
    ///
    /// The XML declaration is present exactly when the bytes begin with
    /// `<?xml` followed by XML white space (so `<?xml-stylesheet` is a
    /// processing instruction, not a declaration); it ends at the first `?>`
    /// or at the end of the bytes, and its encoding pseudo-attribute is the
    /// name `encoding` preceded by white space and followed by optional white
    /// space and `=`.
    ///
    /// Book anchors: `L-value-io-13`.
    #[must_use]
    pub fn xml_encoding(emitted: &[u8]) -> bool {
        const fn xml_space(byte: u8) -> bool {
            matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
        }
        if !encoding(emitted) {
            return false;
        }
        let Some(after_target) = emitted.strip_prefix(b"<?xml") else {
            return true;
        };
        if !after_target.first().copied().is_some_and(xml_space) {
            return true;
        }
        let end = after_target
            .windows(2)
            .position(|pair| pair == b"?>")
            .unwrap_or(after_target.len());
        let declaration = &after_target[..end];
        let mut index = 0;
        while index < declaration.len() {
            let preceded_by_space = index > 0 && xml_space(declaration[index - 1]);
            if preceded_by_space && declaration[index..].starts_with(b"encoding") {
                let mut rest = &declaration[index + b"encoding".len()..];
                while rest.first().copied().is_some_and(xml_space) {
                    rest = &rest[1..];
                }
                if let Some(mut value) = rest.strip_prefix(b"=") {
                    while value.first().copied().is_some_and(xml_space) {
                        value = &value[1..];
                    }
                    let Some((&quote, value)) = value.split_first() else {
                        return false;
                    };
                    if !matches!(quote, b'"' | b'\'') {
                        return false;
                    }
                    let Some(close) = value.iter().position(|byte| *byte == quote) else {
                        return false;
                    };
                    return value[..close].eq_ignore_ascii_case(b"utf-8")
                        || value[..close].eq_ignore_ascii_case(b"utf8");
                }
            }
            index += 1;
        }
        true
    }

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

    /// Definition: The canonical AT Protocol JSON object for a blob value.
    ///
    /// Book anchors: `L-value-io-19`.
    #[must_use]
    pub fn blob_writer(ref_: &str, mime: &str, size: u64) -> Vec<u8> {
        format!(
            "{{\"$type\":\"blob\",\"mimeType\":{},\"ref\":{{\"$link\":{}}},\"size\":{size}}}",
            quote(Route::Json, mime),
            quote(Route::Json, ref_)
        )
        .into_bytes()
    }

    /// CST duplicate-member refusal uses the same member-name predicate.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-23`.
    pub use duplicate_member as duplicate_member_cst;
    /// Definition: A CST route retains encoding signatures byte-for-byte.
    ///
    /// Book anchors: `L-value-io-24`.
    #[must_use]
    pub fn encoding_bom_cst(source: &[u8], emitted: &[u8]) -> bool {
        source == emitted
    }
    /// CST byte preservation uses the same byte equality.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-21`.
    pub use preserve as preserve_cst;

    #[cfg(test)]
    mod tests {
        use super::{
            FidelityReason, Route, Value, ValueIoError, emit, json_number_token, parse, round_trip,
        };

        #[test]
        fn json_number_grammar_precedes_exact_coercion() {
            for token in ["-", "00", "01", "1.", "1e", "1e+"] {
                assert!(!json_number_token(token), "token={token:?}");
                assert_eq!(
                    parse(Route::Json, token.as_bytes()),
                    Err(ValueIoError::Malformed),
                    "token={token:?}"
                );
            }
            for token in ["0", "-0", "1e+2"] {
                assert!(json_number_token(token), "token={token:?}");
                assert!(
                    parse(Route::Json, token.as_bytes()).is_ok(),
                    "token={token:?}"
                );
            }
        }

        #[test]
        fn json_structural_values_are_not_scalar_numeric_refusals() {
            for input in [b"[]".as_slice(), b"{}".as_slice()] {
                assert_eq!(parse(Route::Json, input), Err(ValueIoError::Malformed));
            }
        }

        #[test]
        fn empty_document_routes_do_not_reach_numeric_coercion() {
            for route in [Route::Json, Route::Toml] {
                for input in [b"".as_slice(), b" \t\r\n".as_slice()] {
                    assert_eq!(parse(route, input), Err(ValueIoError::Malformed));
                }
            }
        }

        #[test]
        fn yaml_empty_plain_scalar_is_null() {
            for input in [b"".as_slice(), b" \t\r\n".as_slice()] {
                assert_eq!(parse(Route::Yaml, input), Ok(super::Value::Null));
            }
        }

        #[test]
        fn invalid_source_bytes_have_route_specific_refusals() {
            let invalid_utf8 = [0x80];
            assert_eq!(
                parse(Route::Xml, &invalid_utf8),
                Err(ValueIoError::Fidelity(FidelityReason::InvalidEncoding))
            );
            for route in [
                Route::Json,
                Route::Toml,
                Route::Yaml,
                Route::Csv,
                Route::Tsv,
            ] {
                assert_eq!(parse(route, &invalid_utf8), Err(ValueIoError::Malformed));
            }
        }

        #[test]
        fn xml_literal_line_endings_normalize_before_references_expand() {
            for (input, expected) in [
                (b"\r".as_slice(), "\n"),
                (b"\r\n".as_slice(), "\n"),
                (b"\n".as_slice(), "\n"),
                (b"&#13;".as_slice(), "\r"),
            ] {
                assert_eq!(
                    parse(Route::Xml, input),
                    Ok(Value::Str(expected.to_owned())),
                    "input={input:?}"
                );
            }
        }

        #[test]
        fn double_quoted_literals_follow_each_route_grammar() {
            // RFC 8259 admits every scalar from U+0020 unescaped, DEL and the
            // C1 controls included, and no C0 control.
            for (input, expected) in [
                ("\"\u{7f}\"", Some("\u{7f}")),
                ("\"\u{80}\u{9f}\"", Some("\u{80}\u{9f}")),
                ("\"\t\"", None),
                ("\"\u{1f}\"", None),
            ] {
                assert_eq!(
                    parse(Route::Json, input.as_bytes()).ok(),
                    expected.map(|text| Value::Str(text.to_owned())),
                    "JSON {input:?}"
                );
            }
            // TOML basic strings admit a literal tab and refuse DEL.
            assert_eq!(
                parse(Route::Toml, b"\"\t\""),
                Ok(Value::Str("\t".to_owned()))
            );
            assert_eq!(
                parse(Route::Toml, b"\"\x7f\""),
                Err(ValueIoError::Malformed)
            );
            assert_eq!(
                parse(Route::Toml, "\"\u{80}\"".as_bytes()),
                Ok(Value::Str("\u{80}".to_owned()))
            );
            // YAML double-quoted scalars admit `nb-json`: tab and every scalar
            // from U+0020.
            for input in ["\"\t\"", "\"\u{7f}\"", "\"\u{85}\""] {
                assert_eq!(
                    parse(Route::Yaml, input.as_bytes()),
                    Ok(Value::Str(input[1..input.len() - 1].to_owned())),
                    "YAML {input:?}"
                );
            }
        }

        #[test]
        fn emitted_strings_read_back_on_every_route() {
            for character in [
                '\0', '\t', '\n', '\r', '"', '\\', '&', '<', '\u{7f}', '\u{85}',
            ] {
                let value = Value::Str(character.to_string());
                for route in [
                    Route::Json,
                    Route::Toml,
                    Route::Yaml,
                    Route::Xml,
                    Route::Csv,
                    Route::Tsv,
                ] {
                    if let Ok(bytes) = emit(route, &value) {
                        assert_eq!(
                            parse(route, &bytes),
                            Ok(value.clone()),
                            "route={route:?}, character={character:?}"
                        );
                    }
                }
            }
            assert_eq!(
                emit(Route::Toml, &Value::Str("\u{7f}".to_owned())),
                Ok(b"\"\\u007F\"".to_vec())
            );
            assert_eq!(
                emit(Route::Json, &Value::Str("\u{7f}".to_owned())),
                Ok(b"\"\x7f\"".to_vec())
            );
            assert_eq!(
                emit(Route::Xml, &Value::Str("\r".to_owned())),
                Ok(b"&#xD;".to_vec())
            );
            assert_eq!(
                emit(Route::Xml, &Value::Str("\0".to_owned())),
                Err(ValueIoError::NotRepresentable)
            );
        }

        #[test]
        fn integers_and_special_values_follow_the_writer_rules() {
            use crate::exact::{Decimal, ExactInteger, parse_decimal};
            use crate::num::Nat;
            let ExactInteger::Big(big) =
                ExactInteger::from_parts(false, &Nat::one(), &Nat::from_u64(20))
            else {
                panic!("10^20 is outside i64");
            };
            for route in [
                Route::Json,
                Route::Toml,
                Route::Yaml,
                Route::Xml,
                Route::Csv,
                Route::Tsv,
            ] {
                assert_eq!(
                    emit(route, &Value::BigInt(big.clone())),
                    Ok(b"100000000000000000000".to_vec()),
                    "route={route:?}"
                );
                assert!(
                    round_trip(route, &Value::BigInt(big.clone())),
                    "route={route:?}"
                );
                assert!(round_trip(route, &Value::Int(-7)), "route={route:?}");
                for special in ["Infinity", "-Infinity", "NaN", "-NaN", "sNaN", "NaN12"] {
                    let Some(decimal) = parse_decimal(special) else {
                        panic!("special decimal {special}");
                    };
                    assert!(!matches!(decimal, Decimal::Finite(_)));
                    assert_eq!(
                        emit(route, &Value::Decimal(decimal)),
                        Err(ValueIoError::NonFinite),
                        "route={route:?}, special={special}"
                    );
                }
            }
            for route in [Route::Json, Route::Csv, Route::Tsv] {
                for bits in [
                    f64::INFINITY.to_bits(),
                    f64::NEG_INFINITY.to_bits(),
                    crate::float::CANONICAL_NAN_BITS,
                    0x7ff8_0000_0000_0001,
                ] {
                    assert_eq!(
                        emit(route, &Value::Float(bits)),
                        Err(ValueIoError::NonFinite),
                        "route={route:?}, bits={bits:#x}"
                    );
                }
            }
        }

        #[test]
        fn yaml_refuses_local_temporals() {
            use crate::temporal::{TemporalForm, parse as parse_temporal};
            let Some(local) = parse_temporal(TemporalForm::Date, "2000-01-01") else {
                panic!("local date");
            };
            assert!(!super::temporal(Route::Yaml, &local));
            assert_eq!(
                emit(Route::Yaml, &Value::Temporal(local)),
                Err(ValueIoError::NotRepresentable)
            );
            let Some(zulu) = parse_temporal(TemporalForm::DateTime, "2000-01-01T00:00:00Z") else {
                panic!("zulu date-time");
            };
            assert!(super::temporal(Route::Yaml, &zulu));
        }

        #[test]
        fn toml_scalar_observes_scale_and_the_sign_of_zero() {
            use super::{ScalarKind, toml_scalar};
            let decimal = ScalarKind::Decimal;
            assert!(!toml_scalar(&decimal, "10e-1", "1e0"));
            assert!(!toml_scalar(&decimal, "-0e-1", "0e0"));
            assert!(!toml_scalar(&decimal, "-0", "0"));
            assert!(toml_scalar(&decimal, "0e-1", "0.0"));
            assert!(toml_scalar(&decimal, "1.50", "150e-2"));
            assert!(toml_scalar(&decimal, "10", "10"));
            assert!(!toml_scalar(&decimal, "10", "1e1"));
            let big = ScalarKind::BigInt;
            assert!(toml_scalar(&big, "1e20", "100000000000000000000"));
            assert!(!toml_scalar(&big, "1e20", "10000000000000000000"));
            assert!(!toml_scalar(&ScalarKind::Str, "1", "1"));
        }

        #[test]
        fn xml_encoding_reads_only_the_xml_declaration() {
            use super::xml_encoding;
            assert!(xml_encoding(
                b"<?xml-stylesheet href=\"encoding.xsl\"?><d/>"
            ));
            assert!(xml_encoding(
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><d/>"
            ));
            assert!(xml_encoding(b"<?xml version='1.0' encoding = 'utf8'?><d/>"));
            assert!(xml_encoding(b"<?xml version=\"1.0\"?><d/>"));
            assert!(xml_encoding(b"<d/>"));
            assert!(!xml_encoding(
                b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><d/>"
            ));
            assert!(!xml_encoding(b"\xef\xbb\xbf<d/>"));
        }

        #[test]
        fn xml_character_data_delimiters_are_distinguished_from_text() {
            assert_eq!(parse(Route::Xml, b"<"), Err(ValueIoError::Malformed));
            assert_eq!(parse(Route::Xml, b"]]>"), Err(ValueIoError::Malformed));
            assert_eq!(parse(Route::Xml, b">"), Ok(Value::Str(">".to_owned())));
            assert_eq!(parse(Route::Xml, b"&lt;"), Ok(Value::Str("<".to_owned())));
        }

        #[test]
        fn document_grammar_refuses_nonlexical_control_and_bare_tokens() {
            for route in [Route::Json, Route::Toml, Route::Yaml] {
                for input in [
                    b"\0".as_slice(),
                    b"\x0b0".as_slice(),
                    b"0\x0b".as_slice(),
                    b"\x0c0".as_slice(),
                    b"0\x0c".as_slice(),
                ] {
                    assert_eq!(
                        parse(route, input),
                        Err(ValueIoError::Malformed),
                        "route={route:?}, input={input:?}"
                    );
                }
            }
            assert_eq!(parse(Route::Json, b"word"), Err(ValueIoError::Malformed));
            assert_eq!(parse(Route::Toml, b"word"), Err(ValueIoError::Malformed));
        }
    }
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

    fn xml_10_character(character: char) -> bool {
        matches!(
            u32::from(character),
            0x9 | 0xa | 0xd | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x0001_0000..=0x0010_ffff
        )
    }

    fn append(output: &mut String, text: &str) -> Result<(), EntityError> {
        if !text.chars().all(xml_10_character) {
            return Err(EntityError::Malformed);
        }
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

    /// The value of one XML 1.0 `CharRef` body: `#` then one or more ASCII
    /// decimal digits, or `#x` then one or more ASCII hexadecimal digits of
    /// either case. Every other byte, a sign included, is malformed.
    fn character_reference(name: &str) -> Option<Result<char, EntityError>> {
        let body = name.strip_prefix('#')?;
        let (radix, digits) = body
            .strip_prefix('x')
            .map_or((10_u32, body), |hexadecimal| (16_u32, hexadecimal));
        if digits.is_empty() {
            return Some(Err(EntityError::Malformed));
        }
        let mut value = 0_u32;
        for byte in digits.bytes() {
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' if radix == 16 => u32::from(byte - b'a') + 10,
                b'A'..=b'F' if radix == 16 => u32::from(byte - b'A') + 10,
                _ => return Some(Err(EntityError::Malformed)),
            };
            let Some(next) = value
                .checked_mul(radix)
                .and_then(|shifted| shifted.checked_add(digit))
            else {
                return Some(Err(EntityError::Malformed));
            };
            value = next;
        }
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

    /// Definition: Expand character references, the five predefined entities,
    /// and declared internal general entities recursively, with the XML 1.0
    /// character domain and explicit byte and depth bounds.
    ///
    /// # Errors
    ///
    /// [`EntityError`] for an undeclared, recursive, malformed or over-budget
    /// expansion.
    ///
    /// Book anchors: `L-value-io-12`, `L-value-io-26`.
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

    #[cfg(test)]
    mod tests {
        use std::collections::BTreeMap;

        use super::{EntityError, expand_entities};

        #[test]
        fn character_references_admit_only_unsigned_ascii_digits() {
            for reference in [
                "&#+65;",
                "&#-65;",
                "&#x+41;",
                "&#x-41;",
                "&# 65;",
                "&#X41;",
                "&#65 ;",
                "&#٦٥;",
                "&#;",
                "&#x;",
                "&#4294967361;",
            ] {
                assert_eq!(
                    expand_entities(reference, &BTreeMap::new()),
                    Err(EntityError::Malformed),
                    "{reference}"
                );
            }
            for reference in ["&#65;", "&#0065;", "&#x41;", "&#x0041;", "&#x4a;", "&#x4A;"] {
                assert!(
                    expand_entities(reference, &BTreeMap::new()).is_ok(),
                    "{reference}"
                );
            }
        }

        #[test]
        fn literal_xml_text_obeys_the_xml_10_character_domain() {
            for forbidden in ['\0', '\u{1}', '\u{b}', '\u{c}', '\u{fffe}', '\u{ffff}'] {
                assert_eq!(
                    expand_entities(&forbidden.to_string(), &BTreeMap::new()),
                    Err(EntityError::Malformed)
                );
            }
            for admitted in ['\t', '\n', '\r', ' ', '\u{d7ff}', '\u{e000}', '\u{10000}'] {
                assert_eq!(
                    expand_entities(&admitted.to_string(), &BTreeMap::new()),
                    Ok(admitted.to_string())
                );
            }
        }
    }
}

pub use self::implementation::*;
pub use self::tabular::{missing_token_cst, one_column_empty_cst, tsv_quote_cst};
pub use self::toml::decimal_bigint_cst;
pub use self::xml::{bounded_expansion_cst, expanded_name_cst};
