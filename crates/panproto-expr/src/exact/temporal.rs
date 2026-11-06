//! Civil dates, times of day and date-times, with the UTC offset the
//! lexeme wrote.

use std::cmp::Ordering;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::{ExactKind, ExactParseError, FidelityReason};

/// A date, a time of day, or both.
///
/// The forms follow RFC 3339 and the date and time forms of SQL, TOML and
/// XML Schema: `2026-09-28`, `12:34:56.789+05:30`,
/// `2026-09-28T12:34:56.789+05:30` (a `t` or a space may stand for the
/// `T`, and a `z` for the `Z`). A value keeps how it was written: the
/// separator of a date-time ([`DateTimeSeparator`]), the case of a `Z`
/// ([`ZuluCase`]), the zero-offset form ([`Offset`]) and every fractional
/// digit, so `Display` writes back the lexeme it was read from.
///
/// The variants and their fields are public, so manual construction can
/// produce a value outside the admitted carrier. [`Temporal::well_formed`]
/// tests admission; parsing and deserialization produce only admitted
/// values.
///
/// ```
/// use panproto_expr::exact::{DateTimeSeparator, Offset, Temporal};
///
/// let t = Temporal::parse("2026-09-28T12:34:56.789+05:30")?;
/// let Temporal::DateTime(date, time, offset, separator) = &t else { unreachable!() };
/// assert_eq!((date.year, time.fraction.as_str()), (2026, "789"));
/// assert_eq!((*offset, *separator), (Offset::Fixed(330), DateTimeSeparator::Upper));
/// assert_eq!(t.to_string(), "2026-09-28T12:34:56.789+05:30");
/// let lower = Temporal::parse("2026-09-28t12:34:56z")?;
/// assert_eq!(lower.to_string(), "2026-09-28t12:34:56z");
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Temporal {
    /// A calendar date.
    Date(CivilDate),
    /// A time of day with its offset.
    Time(CivilTime, Offset),
    /// A date and a time of day with its offset, and the separator written
    /// between the date and the time.
    DateTime(CivilDate, CivilTime, Offset, DateTimeSeparator),
    /// A reduced-precision, front-truncated, specially offset, or infinite
    /// temporal value. Complete historical forms continue to use the three
    /// variants above so their serde and canonical representations remain
    /// unchanged.
    Extended(ExtendedTemporal),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TemporalRepr {
    Date(CivilDate),
    Time(CivilTime, Offset),
    DateTime(CivilDate, CivilTime, Offset, DateTimeSeparator),
    Extended(ExtendedTemporal),
}

impl TryFrom<TemporalRepr> for Temporal {
    type Error = &'static str;

    fn try_from(value: TemporalRepr) -> Result<Self, Self::Error> {
        let temporal = match value {
            TemporalRepr::Date(date) => Self::Date(date),
            TemporalRepr::Time(time, offset) => Self::Time(time, offset),
            TemporalRepr::DateTime(date, time, offset, separator) => {
                Self::DateTime(date, time, offset, separator)
            }
            TemporalRepr::Extended(value) => Self::Extended(value),
        };
        temporal
            .well_formed()
            .then_some(temporal)
            .ok_or("a noncanonical temporal representation")
    }
}

impl<'de> Deserialize<'de> for Temporal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::try_from(TemporalRepr::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The separator a date-time was written with between its date and its
/// time: `T`, `t` (RFC 3339 Sec 5.6 allows either case) or a space (RFC
/// 3339 Sec 5.6 NOTE, TOML 1.0 and SQL).
///
/// Serialized as `"upper"`, `"lower"` and `"space"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateTimeSeparator {
    /// `T`.
    Upper,
    /// `t`.
    Lower,
    /// A space.
    Space,
}

impl DateTimeSeparator {
    /// The byte written: `T`, `t` or a space.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Upper => b'T',
            Self::Lower => b't',
            Self::Space => b' ',
        }
    }

    /// The separator `byte` writes, if it writes one.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'T' => Some(Self::Upper),
            b't' => Some(Self::Lower),
            b' ' => Some(Self::Space),
            _ => None,
        }
    }
}

/// The case a `Z` offset was written in: `Z` or `z` (RFC 3339 Sec 5.6
/// allows either), or an unwritten UTC zone supplied by a format such as
/// YAML.
///
/// Serialized as `"upper"`, `"lower"` and `"unwritten"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZuluCase {
    /// `Z`.
    Upper,
    /// `z`.
    Lower,
    /// No zone was written, though the value is read as UTC (the YAML
    /// timestamp convention).
    Unwritten,
}

impl ZuluCase {
    /// The byte written: `Z` or `z`; zero for an unwritten zone. Display and
    /// canonical encoding handle [`Self::Unwritten`] explicitly and never
    /// emit this sentinel.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Upper => b'Z',
            Self::Lower => b'z',
            Self::Unwritten => 0,
        }
    }

    /// The case `byte` writes, if it is `Z` or `z`.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'Z' => Some(Self::Upper),
            b'z' => Some(Self::Lower),
            _ => None,
        }
    }
}

/// The written form of a numeric UTC offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetForm {
    /// `+hh:mm`.
    Extended,
    /// `+hhmm`.
    Basic,
    /// `+hh`.
    HourOnly,
    /// `+hh:mm:00`, retaining the written seconds field.
    ExtendedSeconds,
    /// No offset text was written, though the value carries this offset.
    Unwritten,
}

/// A year together with the distinctions not carried by [`CivilDate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemporalYear {
    /// The year; year zero is 1 BCE.
    pub value: i64,
    /// Whether the lexeme wrote `-0000`.
    pub negative_zero: bool,
    /// The number of year digits written, at least four.
    pub width: usize,
}

/// The date fields present in a reduced or front-truncated value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DateFields {
    /// The year, absent in a front-truncated date.
    pub year: Option<TemporalYear>,
    /// The month, absent in a year or day-only date.
    pub month: Option<u8>,
    /// The day, absent in a year or year-month date.
    pub day: Option<u8>,
}

/// The time fields present in a reduced or front-truncated value.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeFields {
    /// The hour, absent in a front-truncated time.
    pub hour: Option<u8>,
    /// The minute, absent in an hour-only or second-only time.
    pub minute: Option<u8>,
    /// The second, absent at reduced precision.
    pub second: Option<u8>,
    /// Fractional-second digits as written.
    pub fraction: Fraction,
}

/// A date, time, or date-time temporal carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalForm {
    /// A date.
    Date,
    /// A time.
    Time,
    /// A date-time.
    DateTime,
}

/// A date or date-time infinity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalInfinity {
    /// Negative infinity.
    Negative,
    /// Positive infinity.
    Positive,
}

/// The exact descriptor used by the parse/display laws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemporalLexicalForm {
    /// The temporal carrier kind.
    pub kind: TemporalForm,
    /// Presence of `(year, month, day)`, or no date part.
    pub date_shape: Option<(bool, bool, bool)>,
    /// Presence of `(hour, minute, second)`, or no time part.
    pub time_shape: Option<(bool, bool, bool)>,
    /// The date/time separator, when present.
    pub separator: Option<DateTimeSeparator>,
    /// The exact written or unwritten offset.
    pub offset: Offset,
    /// The infinity sign, when this describes an infinity.
    pub infinity: Option<TemporalInfinity>,
}

/// The non-historical temporal forms carried by [`Temporal::Extended`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExtendedTemporal {
    /// Date fields, when present.
    pub date: Option<DateFields>,
    /// Time fields, when present.
    pub time: Option<TimeFields>,
    /// The separator between date and time, when both are present.
    pub separator: Option<DateTimeSeparator>,
    /// The value's written or unwritten offset.
    pub offset: Offset,
    /// The infinity sign and carrier form, for an infinity.
    pub infinity: Option<(TemporalInfinity, TemporalForm)>,
}

/// Why descriptor-free temporal parsing failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalParseError {
    /// No temporal form accepts the lexeme.
    InvalidTemporal,
    /// More than one distinct temporal value accepts the lexeme.
    AmbiguousTemporal,
}

/// A proleptic Gregorian calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DateParts")]
pub struct CivilDate {
    /// The year; year 0 is 1 BCE.
    pub year: i64,
    /// The month, 1 to 12.
    pub month: u8,
    /// The day of the month, 1 to the month's length.
    pub day: u8,
}

/// A time of day.
///
/// The fraction of the second is kept as the digits the lexeme wrote, of
/// any number, trailing zeros included: `12:00:00.1` and `12:00:00.10` are
/// different values under `==`, and
/// [`Temporal::cmp_chronological`] compares them as the same instant.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TimeParts")]
pub struct CivilTime {
    /// The hour, 0 to 23.
    pub hour: u8,
    /// The minute, 0 to 59.
    pub minute: u8,
    /// The second, 0 to 60 (60 is a leap second).
    pub second: u8,
    /// The fractional-second digits as written; empty when the lexeme
    /// wrote no fraction.
    pub fraction: Fraction,
}

/// The fractional-second digits of a time, as the lexeme wrote them.
///
/// Any number of ASCII digits, trailing zeros kept (so the written
/// precision survives), and empty when the lexeme wrote no fraction. The
/// digits `d1 d2 ... dn` denote the fraction `0.d1d2...dn` of a second.
/// Serialized as the digit string.
///
/// ```
/// use panproto_expr::exact::Fraction;
///
/// let f = Fraction::new("1230")?;
/// assert_eq!(f.as_str(), "1230");
/// assert!(Fraction::new("12a").is_err());
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Fraction(Box<str>);

impl Fraction {
    /// The fraction written `digits`.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when `digits` holds a character
    /// that is not an ASCII digit.
    pub fn new(digits: &str) -> Result<Self, ExactParseError> {
        if digits.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self(digits.into()))
        } else {
            Err(invalid())
        }
    }

    /// The digits as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the lexeme wrote no fraction.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Compare two fractions as the numbers they denote: digit by digit,
    /// the shorter padded with zeros, so `1` and `10` are equal.
    #[must_use]
    pub fn cmp_numeric(&self, other: &Self) -> Ordering {
        let (a, b) = (self.0.as_bytes(), other.0.as_bytes());
        (0..a.len().max(b.len()))
            .map(|i| {
                let digit = |s: &[u8]| s.get(i).copied().unwrap_or(b'0');
                digit(a).cmp(&digit(b))
            })
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    }
}

impl TryFrom<String> for Fraction {
    type Error = ExactParseError;
    fn try_from(digits: String) -> Result<Self, Self::Error> {
        Self::new(&digits)
    }
}

impl From<Fraction> for String {
    fn from(fraction: Fraction) -> Self {
        fraction.0.into()
    }
}

/// The UTC offset a lexeme wrote, named by its written form.
///
/// The three zero offsets are three distinct values that denote one
/// instant. RFC 9557 (Sharma and Bormann 2024, Sec 2.2, stated) updates RFC
/// 3339 so that `Z` means what `-00:00` means: the time in UTC is known and
/// the offset to local time is unknown; only `+00:00` says that UTC is the
/// preferred reference. The variants are named after the forms rather than
/// after a meaning because implementations disagree on what the forms
/// mean: java.time reads all three as one UTC offset (Oracle, Java SE 25
/// API, `java.time.OffsetDateTime`, stated), the Temporal proposal reads
/// `-00:00` as `+00:00` (Ecma TC39, Temporal proposal specification, Sec
/// 13.31, stated), and RFC 9557 reads `Z` as `-00:00`. Representation
/// equality keeps the form; [`Temporal::cmp_chronological`] compares the
/// instants, under which the three are `Equal`.
///
/// The case of a `Z` is kept too ([`ZuluCase`]), so `Z` and `z` are two
/// values of one instant, as the zero offsets are.
///
/// Historical forms serialize as `"local"`, `{"zulu": "upper"}` or
/// `{"zulu": "lower"}`, `{"fixed": minutes}` and `"neg_zero"`. The
/// additional variants carry their [`OffsetForm`] or second count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "OffsetRepr")]
pub enum Offset {
    /// No offset: a local (floating) time, as TOML local date-times and SQL
    /// `TIMESTAMP WITHOUT TIME ZONE` write.
    Local,
    /// The form `Z`, or `z`, in the case written. By RFC 9557 Sec 2.2 the
    /// time in UTC is known and the offset to local time is unknown.
    Zulu(ZuluCase),
    /// A numeric offset in minutes east of UTC. `Fixed(0)` is the form
    /// `+00:00`, which by RFC 9557 Sec 2.2 says that UTC is the preferred
    /// reference point.
    Fixed(i16),
    /// The form `-00:00`. By RFC 9557 Sec 2.2, as by RFC 3339, the time in
    /// UTC is known and the offset to local time is unknown.
    NegZero,
    /// A numeric minute offset whose non-historical spelling is retained.
    FixedForm(i16, OffsetForm),
    /// A numeric offset in seconds east of UTC with a nonzero seconds field.
    FixedSeconds(i32),
    /// A negative-zero offset in a non-historical written form.
    NegZeroForm(OffsetForm),
}

#[derive(Deserialize)]
struct DateParts {
    year: i64,
    month: u8,
    day: u8,
}

#[derive(Deserialize)]
struct TimeParts {
    hour: u8,
    minute: u8,
    second: u8,
    fraction: Fraction,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum OffsetRepr {
    Local,
    Zulu(ZuluCase),
    Fixed(i16),
    NegZero,
    FixedForm(i16, OffsetForm),
    FixedSeconds(i32),
    NegZeroForm(OffsetForm),
}

impl TryFrom<DateParts> for CivilDate {
    type Error = ExactParseError;
    fn try_from(p: DateParts) -> Result<Self, Self::Error> {
        Self::new(p.year, p.month, p.day)
    }
}

impl TryFrom<TimeParts> for CivilTime {
    type Error = ExactParseError;
    fn try_from(p: TimeParts) -> Result<Self, Self::Error> {
        Self::new(p.hour, p.minute, p.second, p.fraction)
    }
}

impl TryFrom<OffsetRepr> for Offset {
    type Error = ExactParseError;
    fn try_from(r: OffsetRepr) -> Result<Self, Self::Error> {
        match r {
            OffsetRepr::Local => Ok(Self::Local),
            OffsetRepr::Zulu(case) => Ok(Self::Zulu(case)),
            OffsetRepr::NegZero => Ok(Self::NegZero),
            OffsetRepr::Fixed(m) => Self::fixed(m),
            OffsetRepr::FixedForm(m, form) => Self::fixed_form(m, form),
            OffsetRepr::FixedSeconds(s) => Self::fixed_seconds(s),
            OffsetRepr::NegZeroForm(form) if form != OffsetForm::Unwritten => {
                Ok(Self::NegZeroForm(form))
            }
            OffsetRepr::NegZeroForm(_) => Err(invalid()),
        }
    }
}

const fn invalid() -> ExactParseError {
    ExactParseError::new(ExactKind::Temporal, FidelityReason::InvalidTemporal)
}

const fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl CivilDate {
    /// The date `year-month-day`.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the month or day is out of
    /// range.
    pub const fn new(year: i64, month: u8, day: u8) -> Result<Self, ExactParseError> {
        if month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
            return Err(ExactParseError::new(
                ExactKind::Temporal,
                FidelityReason::InvalidTemporal,
            ));
        }
        Ok(Self { year, month, day })
    }

    /// Days since 1970-01-01 (negative before it), exact for every year.
    #[must_use]
    pub fn days_since_epoch(self) -> i128 {
        // Howard Hinnant's `days_from_civil`, over the proleptic Gregorian
        // calendar, in `i128` so that no year overflows it.
        let month = i128::from(self.month);
        let year = i128::from(self.year) - i128::from(month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let shifted_month = (month + 9) % 12;
        let day_of_year = (153 * shifted_month + 2) / 5 + i128::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// The date `days` days after 1970-01-01 (before it when negative), or
    /// `None` when its year leaves `i64`.
    #[must_use]
    pub fn from_days_since_epoch(days: i128) -> Option<Self> {
        // Howard Hinnant's `civil_from_days`, in `i128`.
        let shifted = days.checked_add(719_468)?;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = era.checked_mul(400)?.checked_add(year_of_era)? + i128::from(month <= 2);
        Some(Self {
            year: i64::try_from(year).ok()?,
            month: u8::try_from(month).ok()?,
            day: u8::try_from(day).ok()?,
        })
    }
}

impl CivilTime {
    /// The time `hour:minute:second` and `fraction` of a second.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when a field is out of range.
    pub fn new(
        hour: u8,
        minute: u8,
        second: u8,
        fraction: Fraction,
    ) -> Result<Self, ExactParseError> {
        if hour > 23 || minute > 59 || second > 60 {
            return Err(invalid());
        }
        Ok(Self {
            hour,
            minute,
            second,
            fraction,
        })
    }

    /// Whole seconds since midnight, the fraction left out.
    #[must_use]
    pub fn seconds_of_day(&self) -> i64 {
        i64::from(self.hour) * 3600 + i64::from(self.minute) * 60 + i64::from(self.second)
    }
}

impl Offset {
    /// A numeric offset of `minutes` east of UTC.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the offset is a day or more.
    pub const fn fixed(minutes: i16) -> Result<Self, ExactParseError> {
        if minutes.unsigned_abs() >= 24 * 60 {
            return Err(invalid());
        }
        Ok(Self::Fixed(minutes))
    }

    /// A numeric minute offset with the exact `form` that was written.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the magnitude is a day or
    /// more, an hour-only form has nonzero minutes, or `form` is the legacy
    /// extended spelling (use [`fixed`](Self::fixed) for that spelling).
    pub const fn fixed_form(minutes: i16, form: OffsetForm) -> Result<Self, ExactParseError> {
        if minutes.unsigned_abs() >= 24 * 60
            || (matches!(form, OffsetForm::HourOnly) && minutes % 60 != 0)
            || matches!(form, OffsetForm::Extended)
        {
            return Err(invalid());
        }
        Ok(Self::FixedForm(minutes, form))
    }

    /// A numeric offset in seconds east of UTC.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the magnitude is a day or
    /// more or the seconds component is zero.
    pub const fn fixed_seconds(seconds: i32) -> Result<Self, ExactParseError> {
        if seconds.unsigned_abs() >= 24 * 60 * 60 || seconds % 60 == 0 {
            return Err(invalid());
        }
        Ok(Self::FixedSeconds(seconds))
    }

    /// Whether the offset places the time on the UTC timeline (every offset
    /// but [`Offset::Local`]).
    #[must_use]
    pub const fn is_zoned(self) -> bool {
        !matches!(self, Self::Local)
    }

    /// Minutes east of UTC; zero for [`Offset::Zulu`], [`Offset::NegZero`]
    /// and [`Offset::Local`].
    #[must_use]
    pub fn minutes(self) -> i16 {
        match self {
            Self::Fixed(m) | Self::FixedForm(m, _) => m,
            Self::FixedSeconds(s) => i16::try_from(s / 60).unwrap_or_default(),
            Self::Local | Self::Zulu(_) | Self::NegZero | Self::NegZeroForm(_) => 0,
        }
    }

    /// Seconds east of UTC; zero for local and zero-offset forms.
    #[must_use]
    pub fn seconds(self) -> i32 {
        match self {
            Self::Fixed(m) | Self::FixedForm(m, _) => i32::from(m) * 60,
            Self::FixedSeconds(s) => s,
            Self::Local | Self::Zulu(_) | Self::NegZero | Self::NegZeroForm(_) => 0,
        }
    }
}

impl ExtendedTemporal {
    /// The value's declared carrier form.
    #[must_use]
    pub const fn form(&self) -> Option<TemporalForm> {
        if let Some((_, form)) = self.infinity {
            return Some(form);
        }
        match (self.date.is_some(), self.time.is_some()) {
            (true, false) => Some(TemporalForm::Date),
            (false, true) => Some(TemporalForm::Time),
            (true, true) => Some(TemporalForm::DateTime),
            (false, false) => None,
        }
    }

    /// The exact lexical descriptor for this value.
    #[must_use]
    pub fn lexical_form(&self) -> Option<TemporalLexicalForm> {
        if !self.well_formed() {
            return None;
        }
        self.structural_lexical_form()
    }

    fn structural_lexical_form(&self) -> Option<TemporalLexicalForm> {
        Some(TemporalLexicalForm {
            kind: self.form()?,
            date_shape: self.date.map(|date| {
                (
                    date.year.is_some(),
                    date.month.is_some(),
                    date.day.is_some(),
                )
            }),
            time_shape: self.time.as_ref().map(|time| {
                (
                    time.hour.is_some(),
                    time.minute.is_some(),
                    time.second.is_some(),
                )
            }),
            separator: self.separator,
            offset: self.offset,
            infinity: self.infinity.map(|(sign, _)| sign),
        })
    }

    /// Construct a date or date-time infinity.
    #[must_use]
    pub fn infinite(sign: TemporalInfinity, form: TemporalForm, offset: Offset) -> Option<Self> {
        (form != TemporalForm::Time).then_some(Self {
            date: None,
            time: None,
            separator: None,
            offset,
            infinity: Some((sign, form)),
        })
    }

    /// Whether the public fields denote an admitted temporal form.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        self.structurally_well_formed() && !self.has_legacy_alias()
    }

    fn structurally_well_formed(&self) -> bool {
        if !offset_well_formed(self.offset) {
            return false;
        }
        if let Some((_, form)) = self.infinity {
            return form != TemporalForm::Time
                && self.date.is_none()
                && self.time.is_none()
                && self.separator.is_none();
        }
        let Some(form) = self.form() else {
            return false;
        };
        if self.separator.is_some() != (form == TemporalForm::DateTime) {
            return false;
        }
        if let Some(date) = self.date {
            let shape = (
                date.year.is_some(),
                date.month.is_some(),
                date.day.is_some(),
            );
            if !matches!(
                shape,
                (true, false | true, false)
                    | (true | false, true, true)
                    | (false, true, false)
                    | (false, false, true)
            ) || !valid_date_fields(&date)
            {
                return false;
            }
            if let Some(year) = date.year {
                let width = year.value.unsigned_abs().to_string().len().max(4);
                if year.width != width || (year.negative_zero && year.value != 0) {
                    return false;
                }
            }
        }
        if let Some(time) = &self.time {
            let shape = (
                time.hour.is_some(),
                time.minute.is_some(),
                time.second.is_some(),
            );
            if !matches!(
                shape,
                (true, false | true, false)
                    | (true | false, true, true)
                    | (false, true, false)
                    | (false, false, true)
            ) || time.hour.is_some_and(|hour| hour > 23)
                || time.minute.is_some_and(|minute| minute > 59)
                || time.second.is_some_and(|second| second > 60)
                || (!time.fraction.is_empty() && time.second.is_none())
            {
                return false;
            }
        }
        form != TemporalForm::DateTime
            || (self.date.is_some_and(|date| {
                let full = date.year.is_some() && date.month.is_some() && date.day.is_some();
                let day_only = date.year.is_none() && date.month.is_none() && date.day.is_some();
                full || day_only
            }) && self.time.as_ref().is_some_and(|time| time.hour.is_some()))
    }

    fn has_legacy_alias(&self) -> bool {
        if self.infinity.is_some() {
            return false;
        }
        match (&self.date, &self.time, self.separator) {
            (Some(date), None, None) => self.offset == Offset::Local && complete_date_fields(date),
            (None, Some(time), None) => legacy_offset(self.offset) && complete_time_fields(time),
            (Some(date), Some(time), Some(_)) => {
                legacy_offset(self.offset)
                    && complete_date_fields(date)
                    && complete_time_fields(time)
            }
            _ => false,
        }
    }

    fn is_truncated(&self) -> bool {
        self.infinity.is_none()
            && (self.date.is_some_and(|date| date.year.is_none())
                || (self.date.is_none()
                    && self.time.as_ref().is_some_and(|time| time.hour.is_none())))
    }
}

const fn offset_well_formed(offset: Offset) -> bool {
    match offset {
        Offset::Local | Offset::Zulu(_) | Offset::NegZero => true,
        Offset::Fixed(minutes) => minutes.unsigned_abs() < 24 * 60,
        Offset::FixedForm(minutes, form) => {
            minutes.unsigned_abs() < 24 * 60
                && (!matches!(form, OffsetForm::HourOnly) || minutes % 60 == 0)
                && !matches!(form, OffsetForm::Extended)
        }
        Offset::FixedSeconds(seconds) => seconds.unsigned_abs() < 24 * 60 * 60 && seconds % 60 != 0,
        Offset::NegZeroForm(form) => !matches!(form, OffsetForm::Extended | OffsetForm::Unwritten),
    }
}

fn digits_n(s: &str, n: usize) -> Option<u8> {
    if s.len() == n && s.bytes().all(|byte| byte.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

fn parse_temporal_year(s: &str) -> Option<TemporalYear> {
    let (negative, body) = s.strip_prefix('-').map_or((false, s), |rest| (true, rest));
    let bytes = body.as_bytes();
    if !bytes.iter().all(u8::is_ascii_digit)
        || !((bytes.len() == 4) || (bytes.len() > 4 && bytes[0] != b'0'))
    {
        return None;
    }
    let magnitude: u64 = body.parse().ok()?;
    let value = if negative {
        i64::try_from(-i128::from(magnitude)).ok()?
    } else {
        i64::try_from(magnitude).ok()?
    };
    Some(TemporalYear {
        value,
        negative_zero: negative && magnitude == 0,
        width: body.len(),
    })
}

fn valid_date_fields(date: &DateFields) -> bool {
    let month_ok = date.month.is_none_or(|month| (1..=12).contains(&month));
    let day_ok = date.day.is_none_or(|day| {
        let limit = match (date.year, date.month) {
            (Some(year), Some(month)) => days_in_month(year.value, month),
            (None, Some(month)) => days_in_month(0, month),
            _ => 31,
        };
        day >= 1 && day <= limit
    });
    month_ok && day_ok
}

fn parse_date_body(text: &str) -> Option<DateFields> {
    let date = if let Some(rest) = text.strip_prefix("---") {
        DateFields {
            year: None,
            month: None,
            day: Some(digits_n(rest, 2)?),
        }
    } else if let Some(rest) = text.strip_prefix("--") {
        match rest.split_once('-') {
            Some((month, day)) => DateFields {
                year: None,
                month: Some(digits_n(month, 2)?),
                day: Some(digits_n(day, 2)?),
            },
            None => DateFields {
                year: None,
                month: Some(digits_n(rest, 2)?),
                day: None,
            },
        }
    } else {
        let (sign, body) = text
            .strip_prefix('-')
            .map_or(("", text), |rest| ("-", rest));
        let mut parts = body.splitn(3, '-');
        let year = parse_temporal_year(&format!("{sign}{}", parts.next()?))?;
        let month = parts
            .next()
            .map(|part| digits_n(part, 2))
            .map_or(Some(None), |month| month.map(Some))?;
        let day = parts
            .next()
            .map(|part| digits_n(part, 2))
            .map_or(Some(None), |day| day.map(Some))?;
        DateFields {
            year: Some(year),
            month,
            day,
        }
    };
    valid_date_fields(&date).then_some(date)
}

fn parse_seconds(text: &str) -> Option<(u8, Fraction)> {
    let (second, fraction) = text
        .split_once('.')
        .map_or((text, None), |(whole, fraction)| (whole, Some(fraction)));
    let second = digits_n(second, 2)?;
    let fraction = match fraction {
        None => Fraction::default(),
        Some(digits) if !digits.is_empty() => Fraction::new(digits).ok()?,
        Some(_) => return None,
    };
    (second <= 60).then_some((second, fraction))
}

fn parse_time_body(text: &str) -> Option<TimeFields> {
    let time = if let Some(rest) = text.strip_prefix("--") {
        let (second, fraction) = parse_seconds(rest)?;
        TimeFields {
            hour: None,
            minute: None,
            second: Some(second),
            fraction,
        }
    } else if let Some(rest) = text.strip_prefix('-') {
        match rest.split_once(':') {
            Some((minute, second)) => {
                let (second, fraction) = parse_seconds(second)?;
                TimeFields {
                    hour: None,
                    minute: Some(digits_n(minute, 2)?),
                    second: Some(second),
                    fraction,
                }
            }
            None => TimeFields {
                hour: None,
                minute: Some(digits_n(rest, 2)?),
                second: None,
                fraction: Fraction::default(),
            },
        }
    } else {
        let mut parts = text.splitn(3, ':');
        let hour = digits_n(parts.next()?, 2)?;
        let minute = parts
            .next()
            .map(|part| digits_n(part, 2))
            .map_or(Some(None), |minute| minute.map(Some))?;
        let (second, fraction) = match parts.next() {
            None => (None, Fraction::default()),
            Some(second) => {
                let (second, fraction) = parse_seconds(second)?;
                (Some(second), fraction)
            }
        };
        TimeFields {
            hour: Some(hour),
            minute,
            second,
            fraction,
        }
    };
    (time.hour.is_none_or(|hour| hour <= 23) && time.minute.is_none_or(|minute| minute <= 59))
        .then_some(time)
}

fn parse_written_offset(text: &str) -> Option<Offset> {
    match text {
        "" => return Some(Offset::Local),
        "Z" => return Some(Offset::Zulu(ZuluCase::Upper)),
        "z" => return Some(Offset::Zulu(ZuluCase::Lower)),
        _ => {}
    }
    let (negative, body) = match text.as_bytes().first() {
        Some(b'+') => (false, &text[1..]),
        Some(b'-') => (true, &text[1..]),
        _ => return None,
    };
    let pieces: Vec<&str> = body.split(':').collect();
    if pieces.len() == 3 {
        let (hour, minute, second) = (
            digits_n(pieces[0], 2)?,
            digits_n(pieces[1], 2)?,
            digits_n(pieces[2], 2)?,
        );
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        let minutes = i16::from(hour) * 60 + i16::from(minute);
        if second == 0 {
            return Some(if minutes == 0 && negative {
                Offset::NegZeroForm(OffsetForm::ExtendedSeconds)
            } else {
                Offset::FixedForm(
                    if negative { -minutes } else { minutes },
                    OffsetForm::ExtendedSeconds,
                )
            });
        }
        let seconds = i32::from(minutes) * 60 + i32::from(second);
        return Some(Offset::FixedSeconds(if negative {
            -seconds
        } else {
            seconds
        }));
    }
    let (hour, minute, form) = if pieces.len() == 2 {
        (
            digits_n(pieces[0], 2)?,
            digits_n(pieces[1], 2)?,
            OffsetForm::Extended,
        )
    } else if body.len() == 4 {
        (
            digits_n(body.get(..2)?, 2)?,
            digits_n(body.get(2..)?, 2)?,
            OffsetForm::Basic,
        )
    } else {
        (digits_n(body, 2)?, 0, OffsetForm::HourOnly)
    };
    if hour > 23 || minute > 59 {
        return None;
    }
    let minutes = i16::from(hour) * 60 + i16::from(minute);
    Some(if minutes == 0 && negative {
        if form == OffsetForm::Extended {
            Offset::NegZero
        } else {
            Offset::NegZeroForm(form)
        }
    } else if form == OffsetForm::Extended {
        Offset::Fixed(if negative { -minutes } else { minutes })
    } else {
        Offset::FixedForm(if negative { -minutes } else { minutes }, form)
    })
}

fn temporal_splits<T>(text: &str, body: impl Fn(&str) -> Option<T>) -> Vec<(T, Offset)> {
    let mut out = Vec::new();
    for cut in (0..=text.len()).rev() {
        if text.is_char_boundary(cut)
            && let (Some(body), Some(offset)) =
                (body(&text[..cut]), parse_written_offset(&text[cut..]))
        {
            out.push((body, offset));
        }
    }
    out
}

fn parse_candidates(form: TemporalForm, text: &str) -> Vec<ExtendedTemporal> {
    let infinity = match text {
        "-infinity" => Some(TemporalInfinity::Negative),
        "infinity" => Some(TemporalInfinity::Positive),
        _ => None,
    };
    if let Some(sign) = infinity {
        return ExtendedTemporal::infinite(sign, form, Offset::Local)
            .into_iter()
            .collect();
    }
    match form {
        TemporalForm::Date => temporal_splits(text, parse_date_body)
            .into_iter()
            .map(|(date, offset)| ExtendedTemporal {
                date: Some(date),
                time: None,
                separator: None,
                offset,
                infinity: None,
            })
            .collect(),
        TemporalForm::Time => temporal_splits(text, parse_time_body)
            .into_iter()
            .map(|(time, offset)| ExtendedTemporal {
                date: None,
                time: Some(time),
                separator: None,
                offset,
                infinity: None,
            })
            .collect(),
        TemporalForm::DateTime => temporal_splits(text, |body| {
            let at = body.find(['T', 't', ' '])?;
            let separator = DateTimeSeparator::from_byte(body.as_bytes()[at])?;
            let date = parse_date_body(&body[..at])?;
            let full = date.year.is_some() && date.month.is_some() && date.day.is_some();
            let day_only = date.year.is_none() && date.month.is_none() && date.day.is_some();
            let time = parse_time_body(&body[at + 1..])?;
            (time.hour.is_some() && (full || day_only)).then_some((date, separator, time))
        })
        .into_iter()
        .map(|((date, separator, time), offset)| ExtendedTemporal {
            date: Some(date),
            time: Some(time),
            separator: Some(separator),
            offset,
            infinity: None,
        })
        .collect(),
    }
}

const fn legacy_offset(offset: Offset) -> bool {
    matches!(
        offset,
        Offset::Local
            | Offset::Zulu(ZuluCase::Upper | ZuluCase::Lower)
            | Offset::Fixed(_)
            | Offset::NegZero
    )
}

fn extended_from_legacy(value: &Temporal) -> ExtendedTemporal {
    let year = |date: CivilDate| TemporalYear {
        value: date.year,
        negative_zero: false,
        width: date.year.unsigned_abs().to_string().len().max(4),
    };
    let date = |date: CivilDate| DateFields {
        year: Some(year(date)),
        month: Some(date.month),
        day: Some(date.day),
    };
    let time = |time: &CivilTime| TimeFields {
        hour: Some(time.hour),
        minute: Some(time.minute),
        second: Some(time.second),
        fraction: time.fraction.clone(),
    };
    match value {
        Temporal::Date(value) => ExtendedTemporal {
            date: Some(date(*value)),
            time: None,
            separator: None,
            offset: Offset::Local,
            infinity: None,
        },
        Temporal::Time(value, offset) => ExtendedTemporal {
            date: None,
            time: Some(time(value)),
            separator: None,
            offset: *offset,
            infinity: None,
        },
        Temporal::DateTime(date_value, time_value, offset, separator) => ExtendedTemporal {
            date: Some(date(*date_value)),
            time: Some(time(time_value)),
            separator: Some(*separator),
            offset: *offset,
            infinity: None,
        },
        Temporal::Extended(value) => value.clone(),
    }
}

fn date_fields_as_civil(date: DateFields) -> Option<CivilDate> {
    let year = date.year?;
    if year.negative_zero || year.width != year.value.unsigned_abs().to_string().len().max(4) {
        return None;
    }
    CivilDate::new(year.value, date.month?, date.day?).ok()
}

fn time_fields_as_civil(time: &TimeFields) -> Option<CivilTime> {
    CivilTime::new(
        time.hour?,
        time.minute?,
        time.second?,
        time.fraction.clone(),
    )
    .ok()
}

fn civil_time_well_formed(time: &CivilTime) -> bool {
    CivilTime::new(time.hour, time.minute, time.second, time.fraction.clone()).is_ok()
}

fn complete_date_fields(date: &DateFields) -> bool {
    date_fields_as_civil(*date).is_some()
}

fn complete_time_fields(time: &TimeFields) -> bool {
    time_fields_as_civil(time).is_some()
}

fn temporal_from_extended(value: ExtendedTemporal) -> Temporal {
    if value.infinity.is_none() && value.structurally_well_formed() {
        match (&value.date, &value.time, value.separator) {
            (Some(date), None, None) if value.offset == Offset::Local => {
                if let Some(date) = date_fields_as_civil(*date) {
                    return Temporal::Date(date);
                }
            }
            (None, Some(time), None) if legacy_offset(value.offset) => {
                if let Some(time) = time_fields_as_civil(time) {
                    return Temporal::Time(time, value.offset);
                }
            }
            (Some(date), Some(time), Some(separator)) if legacy_offset(value.offset) => {
                if let (Some(date), Some(time)) =
                    (date_fields_as_civil(*date), time_fields_as_civil(time))
                {
                    return Temporal::DateTime(date, time, value.offset, separator);
                }
            }
            _ => {}
        }
    }
    Temporal::Extended(value)
}

/// Which of three components a reduced field group writes.
type ComponentPresence = (bool, bool, bool);

/// The written components of an extended value's date and time, and its form.
type ExtendedShape = (
    Option<ComponentPresence>,
    Option<ComponentPresence>,
    Option<TemporalForm>,
);

fn extended_shape(value: &ExtendedTemporal) -> ExtendedShape {
    (
        value.date.map(|date| {
            (
                date.year.is_some(),
                date.month.is_some(),
                date.day.is_some(),
            )
        }),
        value.time.as_ref().map(|time| {
            (
                time.hour.is_some(),
                time.minute.is_some(),
                time.second.is_some(),
            )
        }),
        value.infinity.map(|(_, form)| form),
    )
}

fn extended_order_key(value: &ExtendedTemporal) -> Option<(Vec<i128>, Fraction)> {
    let fraction = value
        .time
        .as_ref()
        .map(|time| time.fraction.clone())
        .unwrap_or_default();
    let second = value
        .time
        .as_ref()
        .and_then(|time| time.second)
        .map_or(0, i128::from);
    if value.is_truncated() {
        let mut fields = Vec::new();
        if let Some(date) = value.date {
            fields.extend(date.month.map(i128::from));
            fields.extend(date.day.map(i128::from));
        }
        if let Some(time) = &value.time {
            fields.extend(time.hour.map(i128::from));
            fields.extend(time.minute.map(i128::from));
            fields.extend(time.second.map(i128::from));
        }
        return Some((fields, fraction));
    }
    let days = value.date.and_then(|date| {
        let year = date.year?;
        CivilDate::new(year.value, date.month.unwrap_or(1), date.day.unwrap_or(1))
            .ok()
            .map(CivilDate::days_since_epoch)
    });
    let hour = value
        .time
        .as_ref()
        .and_then(|time| time.hour)
        .map_or(0, i128::from);
    let minute = value
        .time
        .as_ref()
        .and_then(|time| time.minute)
        .map_or(0, i128::from);
    let shift = i128::from(value.offset.seconds());
    let minute_index = days
        .unwrap_or(0)
        .checked_mul(1_440)?
        .checked_add(hour.checked_mul(60)?)?
        .checked_add(minute)?;
    let ordinary_second = second.min(59);
    let seconds_twice = minute_index
        .checked_mul(120)?
        .checked_add(ordinary_second.checked_mul(2)?)?
        .checked_add(i128::from(second == 60))?
        .checked_sub(shift.checked_mul(2)?)?;
    Some((vec![seconds_twice], fraction))
}

fn cmp_extended(left: &ExtendedTemporal, right: &ExtendedTemporal) -> Option<Ordering> {
    // An infinity lies beyond every value of its form and zoning; two finite
    // values compare within one shape, and a truncated shape is not on the
    // UTC timeline, so its offsets must have equal minutes.
    let comparable = if left.infinity.is_some() || right.infinity.is_some() {
        left.form() == right.form()
    } else {
        extended_shape(left) == extended_shape(right)
            && (!left.is_truncated() || left.offset.minutes() == right.offset.minutes())
    };
    if !comparable || left.offset.is_zoned() != right.offset.is_zoned() {
        return None;
    }
    match (left.infinity, right.infinity) {
        (Some((left, _)), Some((right, _))) => return Some(left.cmp(&right)),
        (Some((TemporalInfinity::Negative, _)), None)
        | (None, Some((TemporalInfinity::Positive, _))) => return Some(Ordering::Less),
        (Some((TemporalInfinity::Positive, _)), None)
        | (None, Some((TemporalInfinity::Negative, _))) => return Some(Ordering::Greater),
        (None, None) => {}
    }
    let (left_key, left_fraction) = extended_order_key(left)?;
    let (right_key, right_fraction) = extended_order_key(right)?;
    Some(
        left_key
            .cmp(&right_key)
            .then_with(|| left_fraction.cmp_numeric(&right_fraction)),
    )
}

impl Temporal {
    /// Read a date, a time or a date-time, deciding the form from the
    /// lexeme: a date-time has a `T`, `t` or space after the date, a date
    /// has a `-` after its year, and anything else is read as a time.
    ///
    /// The value records the separator, the case of a `Z` and a written
    /// minus sign on year zero. A leading plus on a year is not an admitted
    /// XML Schema year form and is refused.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the lexeme is not one of the
    /// forms or a field is out of range. A fraction of any number of digits
    /// is read and kept as written.
    pub fn parse(text: &str) -> Result<Self, ExactParseError> {
        // The infinity spellings contain a `t` but have a date reading,
        // which wins as it does for every lexeme with one.
        if matches!(text, "infinity" | "-infinity") {
            return Self::parse_date(text);
        }
        if text.contains(['T', 't', ' ']) {
            Self::parse_date_time(text)
        } else {
            // A calendar-date reading wins when a lexeme also admits an
            // offset reading (for instance `1973-06`).
            Self::parse_date(text).or_else(|_| Self::parse_time(text))
        }
    }

    /// Read a date, `[sign]YYYY-MM-DD`.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse).
    pub fn parse_date(text: &str) -> Result<Self, ExactParseError> {
        if let Some(value) = parse_candidates(TemporalForm::Date, text)
            .into_iter()
            .next()
        {
            return Ok(temporal_from_extended(value));
        }
        Err(invalid())
    }

    /// Read a time of day, `HH:MM:SS[.fraction][offset]`.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse).
    pub fn parse_time(text: &str) -> Result<Self, ExactParseError> {
        parse_candidates(TemporalForm::Time, text)
            .into_iter()
            .next()
            .map(temporal_from_extended)
            .ok_or_else(invalid)
    }

    /// Read a date-time, a date and a time joined by `T`, `t` or a space.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse).
    pub fn parse_date_time(text: &str) -> Result<Self, ExactParseError> {
        if let Some(value) = parse_candidates(TemporalForm::DateTime, text)
            .into_iter()
            .next()
        {
            return Ok(temporal_from_extended(value));
        }
        Err(invalid())
    }

    /// Read a lexeme under its exact lexical descriptor.
    #[must_use]
    pub fn parse_as(form: TemporalLexicalForm, text: &str) -> Option<Self> {
        if let Some(sign) = form.infinity {
            let spelling = match sign {
                TemporalInfinity::Negative => "-infinity",
                TemporalInfinity::Positive => "infinity",
            };
            if text != spelling {
                return None;
            }
            return ExtendedTemporal::infinite(sign, form.kind, form.offset)
                .map(temporal_from_extended)
                .filter(|value| value.lexical_form() == Some(form));
        }
        let unwritten = matches!(
            form.offset,
            Offset::Zulu(ZuluCase::Unwritten) | Offset::FixedForm(_, OffsetForm::Unwritten)
        );
        let mut matches = parse_candidates(form.kind, text)
            .into_iter()
            .filter_map(|mut value| {
                if unwritten && value.offset == Offset::Local {
                    value.offset = form.offset;
                }
                let value = temporal_from_extended(value);
                (value.lexical_form() == Some(form)).then_some(value)
            });
        let value = matches.next()?;
        matches.next().is_none().then_some(value)
    }

    /// Read without a descriptor, rejecting ambiguity across carrier kinds
    /// and body/offset splits.
    ///
    /// # Errors
    ///
    /// [`TemporalParseError::InvalidTemporal`] when no carrier accepts the
    /// text, or [`TemporalParseError::AmbiguousTemporal`] when more than one
    /// representation accepts it.
    pub fn parse_union(text: &str) -> Result<Self, TemporalParseError> {
        let mut all = Vec::new();
        for form in [
            TemporalForm::Date,
            TemporalForm::Time,
            TemporalForm::DateTime,
        ] {
            for value in parse_candidates(form, text)
                .into_iter()
                .map(temporal_from_extended)
            {
                if !all.contains(&value) {
                    all.push(value);
                }
            }
        }
        match all.as_slice() {
            [one] => Ok(one.clone()),
            [] => Err(TemporalParseError::InvalidTemporal),
            _ => Err(TemporalParseError::AmbiguousTemporal),
        }
    }

    /// The exact descriptor that makes display/parse reversible.
    #[must_use]
    pub fn lexical_form(&self) -> Option<TemporalLexicalForm> {
        if !self.well_formed() {
            return None;
        }
        extended_from_legacy(self).structural_lexical_form()
    }

    /// Whether this value uses the one admitted carrier for its temporal
    /// representation.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        match self {
            Self::Date(date) => CivilDate::new(date.year, date.month, date.day).is_ok(),
            Self::Time(time, offset) => {
                civil_time_well_formed(time)
                    && legacy_offset(*offset)
                    && offset_well_formed(*offset)
            }
            Self::DateTime(date, time, offset, _) => {
                CivilDate::new(date.year, date.month, date.day).is_ok()
                    && civil_time_well_formed(time)
                    && legacy_offset(*offset)
                    && offset_well_formed(*offset)
            }
            Self::Extended(value) => value.well_formed(),
        }
    }

    /// Whether the value is a finite full date, full time, or full date-time.
    ///
    /// Reduced-precision, front-truncated, and infinite forms are admitted by
    /// the exact temporal carrier but are not complete.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        let Some(form) = self.lexical_form() else {
            return false;
        };
        let full_date = form
            .date_shape
            .is_none_or(|(year, month, day)| year && month && day);
        let full_time = form
            .time_shape
            .is_none_or(|(hour, minute, second)| hour && minute && second);
        form.infinity.is_none()
            && full_date
            && full_time
            && (form.date_shape.is_some() || form.time_shape.is_some())
    }

    /// The offset the value was written with; `None` for a date.
    #[must_use]
    pub const fn offset(&self) -> Option<Offset> {
        match self {
            Self::Date(_) => None,
            Self::Time(_, offset) | Self::DateTime(_, _, offset, _) => Some(*offset),
            Self::Extended(value) => Some(value.offset),
        }
    }

    /// Compare two values in time, when both are of one form and either
    /// both are local or both are zoned; `None` otherwise.
    ///
    /// Two finite values compare only within one shape (form, precision and
    /// truncation), and two values of a truncated shape, which is not placed
    /// on the UTC timeline, only when their offsets have equal minutes.
    /// An infinity compares with every value of its form and zoning: the
    /// negative one below it and the positive one above.
    ///
    /// Zoned values are compared on the UTC timeline, so the three zero
    /// offsets `Z`, `+00:00` and `-00:00` compare `Equal`; local values by
    /// their civil fields. How a value was written (the case of `Z` or of
    /// the separator) does not enter the comparison. Fractions of any
    /// length compare as decimal fractions of a second (`.1` and `.10` are
    /// equal).
    ///
    /// A leap second is its own instant, after second 59 of its minute and
    /// before the next minute's second 0, whatever the fractions: a value is
    /// placed by twice its whole seconds on the timeline, second 60 taking
    /// the odd point between second 59 and the next minute, and then by its
    /// fraction. So `2024-01-01T23:59:60Z` and `2024-01-01T23:59:60.5Z` both
    /// precede `2024-01-02T00:00:00Z`, and both follow
    /// `2024-01-01T23:59:59.999Z`.
    #[must_use]
    pub fn cmp_chronological(&self, other: &Self) -> Option<Ordering> {
        cmp_extended(&extended_from_legacy(self), &extended_from_legacy(other))
    }

    /// The date-time `count` units of `unit` after 1970-01-01T00:00:00,
    /// before it when `count` is negative, as an epoch-count column holds
    /// it: the date-time with exactly as many fractional-second digits as
    /// the unit fixes (none, 3, 6 or 9), the separator `T`, and the offset
    /// `Z` for an instant column or none for a wall-clock column (`instant`
    /// false).
    ///
    /// [`to_epoch_count`](Self::to_epoch_count) is its inverse on its image.
    ///
    /// ```
    /// use panproto_expr::exact::{Temporal, TimeUnit};
    ///
    /// let t = Temporal::from_epoch_count(-1, TimeUnit::Millis, true)?;
    /// assert_eq!(t.to_string(), "1969-12-31T23:59:59.999Z");
    /// assert_eq!(t.to_epoch_count(TimeUnit::Millis, true), Some(-1));
    /// # Ok::<(), panproto_expr::ExactParseError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`FidelityReason::Overflow`] when the date-time's year leaves the
    /// range of [`CivilDate`]'s year, `i64`.
    pub fn from_epoch_count(
        count: i128,
        unit: TimeUnit,
        instant: bool,
    ) -> Result<Self, ExactParseError> {
        let overflow = || ExactParseError::new(ExactKind::Temporal, FidelityReason::Overflow);
        let digits = unit.fraction_digits();
        let ticks = 10_i128.pow(digits);
        let seconds = count.div_euclid(ticks);
        let within = count.rem_euclid(ticks);
        let date =
            CivilDate::from_days_since_epoch(seconds.div_euclid(86_400)).ok_or_else(overflow)?;
        let of_day = seconds.rem_euclid(86_400);
        let field = |n: i128| u8::try_from(n).map_err(|_| overflow());
        let fraction = if digits == 0 {
            Fraction::default()
        } else {
            let width = usize::try_from(digits).map_err(|_| overflow())?;
            Fraction::new(&format!("{within:0width$}"))?
        };
        let time = CivilTime::new(
            field(of_day / 3600)?,
            field(of_day % 3600 / 60)?,
            field(of_day % 60)?,
            fraction,
        )?;
        let offset = if instant {
            Offset::Zulu(ZuluCase::Upper)
        } else {
            Offset::Local
        };
        Ok(Self::DateTime(date, time, offset, DateTimeSeparator::Upper))
    }

    /// The epoch count of `unit` that
    /// [`from_epoch_count`](Self::from_epoch_count) gives this value from,
    /// or `None` when the value is outside that function's image: not a
    /// date-time; another separator than `T`; another offset than `Z` for an
    /// instant column or none for a wall-clock one; second 60; or another
    /// number of fractional digits than the unit fixes. `None` also when the
    /// count leaves `i128`.
    #[must_use]
    pub fn to_epoch_count(&self, unit: TimeUnit, instant: bool) -> Option<i128> {
        let expected = if instant {
            Offset::Zulu(ZuluCase::Upper)
        } else {
            Offset::Local
        };
        let Self::DateTime(date, time, offset, DateTimeSeparator::Upper) = self else {
            return None;
        };
        let digits = usize::try_from(unit.fraction_digits()).ok()?;
        if *offset != expected || time.second > 59 || time.fraction.as_str().len() != digits {
            return None;
        }
        let within = if digits == 0 {
            0
        } else {
            time.fraction.as_str().parse::<i128>().ok()?
        };
        let seconds = date
            .days_since_epoch()
            .checked_mul(86_400)?
            .checked_add(i128::from(time.hour) * 3600 + i128::from(time.minute) * 60)?
            .checked_add(i128::from(time.second))?;
        seconds
            .checked_mul(10_i128.pow(unit.fraction_digits()))?
            .checked_add(within)
    }

    /// The canonical payload bytes. Historical complete forms retain their
    /// byte-for-byte layouts under form tags `0` (date), `1` (time) and `2`
    /// (date-time): fixed date and time fields followed by the historical
    /// offset tags `0` through `3`.
    ///
    /// New finite forms use tags `3` (date), `4` (time) and `5`
    /// (date-time); date and time fields carry presence tags, and offsets
    /// carry their spelling tag, an unwritten-zone tag or second count.
    /// Date and date-time infinities use tags `6` and `7`, followed by their
    /// sign and offset. Thus every admitted representation has distinct
    /// bytes without changing an existing complete-form byte.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        match self {
            Self::Date(d) => {
                out.push(0);
                put_civil_date(&mut out, *d);
            }
            Self::Time(t, o) => {
                out.push(1);
                put_civil_time(&mut out, t);
                legacy_offset_bytes(&mut out, *o);
            }
            Self::DateTime(d, t, o, separator) => {
                out.push(2);
                put_civil_date(&mut out, *d);
                out.push(separator.byte());
                put_civil_time(&mut out, t);
                legacy_offset_bytes(&mut out, *o);
            }
            Self::Extended(value) => put_extended(&mut out, value),
        }
        out
    }
}

fn put_civil_date(out: &mut Vec<u8>, d: CivilDate) {
    out.extend_from_slice(&d.year.to_le_bytes());
    out.extend_from_slice(&[d.month, d.day]);
}
fn put_civil_time(out: &mut Vec<u8>, t: &CivilTime) {
    out.extend_from_slice(&[t.hour, t.minute, t.second]);
    super::put_str(out, t.fraction.as_str());
}
fn legacy_offset_bytes(out: &mut Vec<u8>, o: Offset) {
    match o {
        Offset::Local => out.push(0),
        Offset::Zulu(ZuluCase::Upper) => out.extend_from_slice(&[1, b'Z']),
        Offset::Zulu(ZuluCase::Lower) => out.extend_from_slice(&[1, b'z']),
        Offset::Fixed(m) => {
            out.push(2);
            out.extend_from_slice(&i64::from(m).to_le_bytes());
        }
        Offset::NegZero => out.push(3),
        Offset::Zulu(ZuluCase::Unwritten)
        | Offset::FixedForm(_, _)
        | Offset::FixedSeconds(_)
        | Offset::NegZeroForm(_) => {}
    }
}
fn put_option_u8(out: &mut Vec<u8>, value: Option<u8>) {
    match value {
        None => out.push(0),
        Some(value) => out.extend_from_slice(&[1, value]),
    }
}
fn put_date_fields(out: &mut Vec<u8>, date: DateFields) {
    match date.year {
        None => out.push(0),
        Some(year) => {
            out.push(1);
            out.extend_from_slice(&year.value.to_le_bytes());
            out.push(u8::from(year.negative_zero));
            out.extend_from_slice(&u64::try_from(year.width).unwrap_or(u64::MAX).to_le_bytes());
        }
    }
    put_option_u8(out, date.month);
    put_option_u8(out, date.day);
}
fn put_time_fields(out: &mut Vec<u8>, time: &TimeFields) {
    put_option_u8(out, time.hour);
    put_option_u8(out, time.minute);
    put_option_u8(out, time.second);
    super::put_str(out, time.fraction.as_str());
}
const fn offset_form_tag(form: OffsetForm) -> u8 {
    match form {
        OffsetForm::Extended => 0,
        OffsetForm::Basic => 1,
        OffsetForm::HourOnly => 2,
        OffsetForm::ExtendedSeconds => 3,
        OffsetForm::Unwritten => 4,
    }
}
fn extended_offset_bytes(out: &mut Vec<u8>, offset: Offset) {
    match offset {
        Offset::Local => out.push(0),
        Offset::Zulu(case) => {
            out.extend_from_slice(&[
                1,
                match case {
                    ZuluCase::Upper => 0,
                    ZuluCase::Lower => 1,
                    ZuluCase::Unwritten => 2,
                },
            ]);
        }
        Offset::Fixed(minutes) => {
            out.push(2);
            out.extend_from_slice(&i64::from(minutes).to_le_bytes());
            out.push(offset_form_tag(OffsetForm::Extended));
        }
        Offset::FixedForm(minutes, form) => {
            out.push(2);
            out.extend_from_slice(&i64::from(minutes).to_le_bytes());
            out.push(offset_form_tag(form));
        }
        Offset::NegZero => out.extend_from_slice(&[3, offset_form_tag(OffsetForm::Extended)]),
        Offset::NegZeroForm(form) => out.extend_from_slice(&[3, offset_form_tag(form)]),
        Offset::FixedSeconds(seconds) => {
            out.push(4);
            out.extend_from_slice(&i64::from(seconds).to_le_bytes());
        }
    }
}
fn put_extended(out: &mut Vec<u8>, value: &ExtendedTemporal) {
    if let Some((infinity, form)) = value.infinity {
        out.push(match form {
            TemporalForm::Date => 6,
            TemporalForm::DateTime => 7,
            TemporalForm::Time => return,
        });
        out.push(match infinity {
            TemporalInfinity::Negative => 0,
            TemporalInfinity::Positive => 1,
        });
        extended_offset_bytes(out, value.offset);
        return;
    }
    let Some(form) = value.form() else {
        return;
    };
    out.push(match form {
        TemporalForm::Date => 3,
        TemporalForm::Time => 4,
        TemporalForm::DateTime => 5,
    });
    if let Some(date) = value.date {
        put_date_fields(out, date);
    }
    if let Some(separator) = value.separator {
        out.push(separator.byte());
    }
    if let Some(time) = &value.time {
        put_time_fields(out, time);
    }
    extended_offset_bytes(out, value.offset);
}

/// The unit of an integral epoch count: seconds, milliseconds,
/// microseconds or nanoseconds since 1970-01-01T00:00:00.
///
/// Read from `s`, `ms`, `us` and `ns`, and serialized as `"seconds"`,
/// `"millis"`, `"micros"` and `"nanos"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    /// Seconds (`s`).
    Seconds,
    /// Milliseconds (`ms`).
    Millis,
    /// Microseconds (`us`).
    Micros,
    /// Nanoseconds (`ns`).
    Nanos,
}

impl TimeUnit {
    /// The unit `text` names: `s`, `ms`, `us` or `ns`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "s" => Some(Self::Seconds),
            "ms" => Some(Self::Millis),
            "us" => Some(Self::Micros),
            "ns" => Some(Self::Nanos),
            _ => None,
        }
    }

    /// The unit's name: `s`, `ms`, `us` or `ns`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Seconds => "s",
            Self::Millis => "ms",
            Self::Micros => "us",
            Self::Nanos => "ns",
        }
    }

    /// The number of fractional-second digits a count of this unit fixes:
    /// 0, 3, 6 or 9.
    #[must_use]
    pub const fn fraction_digits(self) -> u32 {
        match self {
            Self::Seconds => 0,
            Self::Millis => 3,
            Self::Micros => 6,
            Self::Nanos => 9,
        }
    }
}

impl std::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.year < 0 {
            f.write_str("-")?;
        }
        write!(
            f,
            "{:04}-{:02}-{:02}",
            self.year.unsigned_abs(),
            self.month,
            self.day
        )
    }
}

impl std::fmt::Display for CivilTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}:{:02}:{:02}", self.hour, self.minute, self.second)?;
        if !self.fraction.is_empty() {
            write!(f, ".{}", self.fraction.as_str())?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Offset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let numeric = |f: &mut std::fmt::Formatter<'_>, sign: char, minutes: u16, form| {
            let (hour, minute) = (minutes / 60, minutes % 60);
            match form {
                OffsetForm::Extended => write!(f, "{sign}{hour:02}:{minute:02}"),
                OffsetForm::Basic => write!(f, "{sign}{hour:02}{minute:02}"),
                OffsetForm::HourOnly => write!(f, "{sign}{hour:02}"),
                OffsetForm::ExtendedSeconds => {
                    write!(f, "{sign}{hour:02}:{minute:02}:00")
                }
                OffsetForm::Unwritten => Ok(()),
            }
        };
        match self {
            Self::Local | Self::Zulu(ZuluCase::Unwritten) => Ok(()),
            Self::Zulu(ZuluCase::Upper) => f.write_char('Z'),
            Self::Zulu(ZuluCase::Lower) => f.write_char('z'),
            Self::NegZero => f.write_str("-00:00"),
            Self::Fixed(m) => {
                let sign = if *m < 0 { '-' } else { '+' };
                let m = m.unsigned_abs();
                write!(f, "{sign}{:02}:{:02}", m / 60, m % 60)
            }
            Self::FixedForm(minutes, form) => {
                let sign = if *minutes < 0 { '-' } else { '+' };
                numeric(f, sign, minutes.unsigned_abs(), *form)
            }
            Self::NegZeroForm(form) => numeric(f, '-', 0, *form),
            Self::FixedSeconds(seconds) => {
                let sign = if *seconds < 0 { '-' } else { '+' };
                let magnitude = seconds.unsigned_abs();
                write!(
                    f,
                    "{sign}{:02}:{:02}:{:02}",
                    magnitude / 3_600,
                    magnitude % 3_600 / 60,
                    magnitude % 60
                )
            }
        }
    }
}

fn write_date_fields(f: &mut std::fmt::Formatter<'_>, date: &DateFields) -> std::fmt::Result {
    match date.year {
        Some(year) => {
            let sign = if year.value < 0 || year.negative_zero {
                "-"
            } else {
                ""
            };
            write!(
                f,
                "{sign}{:0width$}",
                year.value.unsigned_abs(),
                width = year.width
            )?;
            if let Some(month) = date.month {
                write!(f, "-{month:02}")?;
            }
            if let Some(day) = date.day {
                write!(f, "-{day:02}")?;
            }
            Ok(())
        }
        None => match (date.month, date.day) {
            (Some(month), Some(day)) => write!(f, "--{month:02}-{day:02}"),
            (Some(month), None) => write!(f, "--{month:02}"),
            (None, Some(day)) => write!(f, "---{day:02}"),
            (None, None) => Ok(()),
        },
    }
}

fn write_time_fields(f: &mut std::fmt::Formatter<'_>, time: &TimeFields) -> std::fmt::Result {
    let fraction = |f: &mut std::fmt::Formatter<'_>| {
        if time.fraction.is_empty() {
            Ok(())
        } else {
            write!(f, ".{}", time.fraction.as_str())
        }
    };
    match (time.hour, time.minute, time.second) {
        (Some(hour), None, _) => write!(f, "{hour:02}"),
        (Some(hour), Some(minute), None) => write!(f, "{hour:02}:{minute:02}"),
        (Some(hour), Some(minute), Some(second)) => {
            write!(f, "{hour:02}:{minute:02}:{second:02}")?;
            fraction(f)
        }
        (None, Some(minute), Some(second)) => {
            write!(f, "-{minute:02}:{second:02}")?;
            fraction(f)
        }
        (None, Some(minute), None) => write!(f, "-{minute:02}"),
        (None, None, Some(second)) => {
            write!(f, "--{second:02}")?;
            fraction(f)
        }
        (None, None, None) => Ok(()),
    }
}

impl std::fmt::Display for ExtendedTemporal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some((infinity, _)) = self.infinity {
            return f.write_str(match infinity {
                TemporalInfinity::Negative => "-infinity",
                TemporalInfinity::Positive => "infinity",
            });
        }
        if let Some(date) = &self.date {
            write_date_fields(f, date)?;
        }
        if let Some(separator) = self.separator {
            f.write_char(char::from(separator.byte()))?;
        }
        if let Some(time) = &self.time {
            write_time_fields(f, time)?;
        }
        write!(f, "{}", self.offset)
    }
}

impl std::fmt::Display for Temporal {
    /// The RFC 3339 form, with the separator and the case of `Z` as
    /// written, which [`Temporal::parse`] reads back into the same value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Date(d) => write!(f, "{d}"),
            Self::Time(t, o) => write!(f, "{t}{o}"),
            Self::DateTime(d, t, o, separator) => {
                write!(f, "{d}{}{t}{o}", char::from(separator.byte()))
            }
            Self::Extended(value) => write!(f, "{value}"),
        }
    }
}
