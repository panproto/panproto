//! Civil dates, times of day and date-times, with the UTC offset the
//! lexeme wrote.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{ExactKind, ExactParseError, FidelityReason};

/// A date, a time of day, or both.
///
/// The forms follow RFC 3339 and the date and time forms of SQL, TOML and
/// XML Schema: `2026-09-28`, `12:34:56.789+05:30`,
/// `2026-09-28T12:34:56.789+05:30` (a space may stand for the `T`).
///
/// ```
/// use panproto_expr::exact::{Offset, Temporal};
///
/// let t = Temporal::parse("2026-09-28T12:34:56.789+05:30")?;
/// let Temporal::DateTime(date, time, offset) = &t else { unreachable!() };
/// assert_eq!((date.year, time.fraction.as_str()), (2026, "789"));
/// assert_eq!(*offset, Offset::Fixed(330));
/// assert_eq!(t.to_string(), "2026-09-28T12:34:56.789+05:30");
/// # Ok::<(), panproto_expr::ExactParseError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Temporal {
    /// A calendar date.
    Date(CivilDate),
    /// A time of day with its offset.
    Time(CivilTime, Offset),
    /// A date and a time of day with its offset.
    DateTime(CivilDate, CivilTime, Offset),
}

/// A proleptic Gregorian calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "DateParts")]
pub struct CivilDate {
    /// The year; year 0 is 1 BCE.
    pub year: i32,
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
/// Serialized as `"local"`, `"zulu"`, `{"fixed": minutes}` and
/// `"neg_zero"`.
///
/// Known loss, kept until it is settled how the written case is carried:
/// a lowercase `z` reads as [`Offset::Zulu`] and is written back as `Z`,
/// so `==`, the canonical bytes and every route that writes through
/// `Display` cannot tell the two spellings apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "OffsetRepr")]
pub enum Offset {
    /// No offset: a local (floating) time, as TOML local date-times and SQL
    /// `TIMESTAMP WITHOUT TIME ZONE` write.
    Local,
    /// The form `Z` (read also from `z`, written `Z`). By RFC 9557 Sec 2.2
    /// the time in UTC is known and the offset to local time is unknown.
    Zulu,
    /// A numeric offset in minutes east of UTC. `Fixed(0)` is the form
    /// `+00:00`, which by RFC 9557 Sec 2.2 says that UTC is the preferred
    /// reference point.
    Fixed(i16),
    /// The form `-00:00`. By RFC 9557 Sec 2.2, as by RFC 3339, the time in
    /// UTC is known and the offset to local time is unknown.
    NegZero,
}

#[derive(Deserialize)]
struct DateParts {
    year: i32,
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
    Zulu,
    Fixed(i16),
    NegZero,
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
            OffsetRepr::Zulu => Ok(Self::Zulu),
            OffsetRepr::NegZero => Ok(Self::NegZero),
            OffsetRepr::Fixed(m) => Self::fixed(m),
        }
    }
}

const fn invalid() -> ExactParseError {
    ExactParseError::new(ExactKind::Temporal, FidelityReason::InvalidTemporal)
}

const fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const fn days_in_month(year: i32, month: u8) -> u8 {
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
    pub const fn new(year: i32, month: u8, day: u8) -> Result<Self, ExactParseError> {
        if month == 0 || month > 12 || day == 0 || day > days_in_month(year, month) {
            return Err(ExactParseError::new(
                ExactKind::Temporal,
                FidelityReason::InvalidTemporal,
            ));
        }
        Ok(Self { year, month, day })
    }

    /// Days since 1970-01-01 (negative before it).
    #[must_use]
    pub fn days_since_epoch(self) -> i64 {
        // Howard Hinnant's `days_from_civil`, over the proleptic Gregorian
        // calendar.
        let month = i64::from(self.month);
        let year = i64::from(self.year) - i64::from(month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let shifted_month = (month + 9) % 12;
        let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
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

    /// Whether the offset places the time on the UTC timeline (every offset
    /// but [`Offset::Local`]).
    #[must_use]
    pub const fn is_zoned(self) -> bool {
        !matches!(self, Self::Local)
    }

    /// Minutes east of UTC; zero for [`Offset::Zulu`], [`Offset::NegZero`]
    /// and [`Offset::Local`].
    #[must_use]
    pub const fn minutes(self) -> i16 {
        match self {
            Self::Fixed(m) => m,
            Self::Local | Self::Zulu | Self::NegZero => 0,
        }
    }
}

/// Read exactly `n` ASCII digits at the start of `s`.
fn take_digits(s: &str, n: usize) -> Result<(u32, &str), ExactParseError> {
    let head = s.get(..n).ok_or_else(invalid)?;
    if !head.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    let value = head.parse::<u32>().map_err(|_| invalid())?;
    Ok((value, &s[n..]))
}

/// Read the literal byte `c` at the start of `s`.
fn take_byte(s: &str, c: u8) -> Result<&str, ExactParseError> {
    if s.as_bytes().first() == Some(&c) {
        Ok(&s[1..])
    } else {
        Err(invalid())
    }
}

fn narrow(v: u32) -> Result<u8, ExactParseError> {
    u8::try_from(v).map_err(|_| invalid())
}

/// Read `[sign]YYYY-MM-DD` (four or more year digits) at the start of `s`.
/// A plus sign is read as no sign (a known loss, stated at
/// [`Temporal::parse`]).
fn read_date(s: &str) -> Result<(CivilDate, &str), ExactParseError> {
    let (negative, s) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let year_len = s.bytes().take_while(u8::is_ascii_digit).count();
    // Four digits, or more without a leading zero (XML Schema years).
    if year_len < 4 || (year_len > 4 && s.starts_with('0')) || year_len > 9 {
        return Err(invalid());
    }
    let (year, s) = take_digits(s, year_len)?;
    let year = i32::try_from(year).map_err(|_| invalid())?;
    let year = if negative { -year } else { year };
    let s = take_byte(s, b'-')?;
    let (month, s) = take_digits(s, 2)?;
    let s = take_byte(s, b'-')?;
    let (day, s) = take_digits(s, 2)?;
    Ok((CivilDate::new(year, narrow(month)?, narrow(day)?)?, s))
}

/// Read `HH:MM:SS[.fraction]` at the start of `s`.
fn read_time(s: &str) -> Result<(CivilTime, &str), ExactParseError> {
    let (hour, s) = take_digits(s, 2)?;
    let s = take_byte(s, b':')?;
    let (minute, s) = take_digits(s, 2)?;
    let s = take_byte(s, b':')?;
    let (second, s) = take_digits(s, 2)?;
    let (fraction, s) = match s.strip_prefix('.') {
        None => (Fraction::default(), s),
        Some(rest) => {
            let n = rest.bytes().take_while(u8::is_ascii_digit).count();
            if n == 0 {
                return Err(invalid());
            }
            (Fraction::new(&rest[..n])?, &rest[n..])
        }
    };
    let time = CivilTime::new(narrow(hour)?, narrow(minute)?, narrow(second)?, fraction)?;
    Ok((time, s))
}

/// Read the whole of `s` as an offset: empty, `Z`, or `±HH:MM`.
fn read_offset(s: &str) -> Result<Offset, ExactParseError> {
    match s {
        "" => Ok(Offset::Local),
        "Z" | "z" => Ok(Offset::Zulu),
        "-00:00" => Ok(Offset::NegZero),
        _ => {
            let (sign, rest) = match s.as_bytes().first() {
                Some(b'+') => (1_i16, &s[1..]),
                Some(b'-') => (-1, &s[1..]),
                _ => return Err(invalid()),
            };
            let (hours, rest) = take_digits(rest, 2)?;
            let rest = take_byte(rest, b':')?;
            let (minutes, rest) = take_digits(rest, 2)?;
            if !rest.is_empty() || hours > 23 || minutes > 59 {
                return Err(invalid());
            }
            let total = i16::try_from(hours * 60 + minutes).map_err(|_| invalid())?;
            Offset::fixed(sign * total)
        }
    }
}

impl Temporal {
    /// Read a date, a time or a date-time, deciding the form from the
    /// lexeme: a date-time has a `T`, `t` or space after the date, a date
    /// has a `-` after its year, and anything else is read as a time.
    ///
    /// Known loss, kept until it is settled how the written case is
    /// carried: the separator `t` or a space and a lowercase offset `z` are
    /// read as `T` and `Z` are and written back as `T` and `Z` by
    /// `Display`, so the value does not record which was written. Likewise
    /// a plus sign before the year (`+2024-01-01`) is read as an unsigned
    /// year is and is not written back.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the lexeme is not one of the
    /// forms or a field is out of range. A fraction of any number of digits
    /// is read and kept as written.
    pub fn parse(text: &str) -> Result<Self, ExactParseError> {
        if text.contains(['T', 't', ' ']) {
            Self::parse_date_time(text)
        } else if text.len() >= 10 && text.as_bytes().get(2) != Some(&b':') {
            Self::parse_date(text)
        } else {
            Self::parse_time(text)
        }
    }

    /// Read a date, `[sign]YYYY-MM-DD`.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse).
    pub fn parse_date(text: &str) -> Result<Self, ExactParseError> {
        let (date, rest) = read_date(text)?;
        if !rest.is_empty() {
            return Err(invalid());
        }
        Ok(Self::Date(date))
    }

    /// Read a time of day, `HH:MM:SS[.fraction][offset]`.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse).
    pub fn parse_time(text: &str) -> Result<Self, ExactParseError> {
        let (time, rest) = read_time(text)?;
        Ok(Self::Time(time, read_offset(rest)?))
    }

    /// Read a date-time, a date and a time joined by `T`, `t` or a space.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse).
    pub fn parse_date_time(text: &str) -> Result<Self, ExactParseError> {
        let (date, rest) = read_date(text)?;
        let rest = match rest.as_bytes().first() {
            Some(b'T' | b't' | b' ') => &rest[1..],
            _ => return Err(invalid()),
        };
        let (time, rest) = read_time(rest)?;
        Ok(Self::DateTime(date, time, read_offset(rest)?))
    }

    /// The offset the value was written with; `None` for a date.
    #[must_use]
    pub const fn offset(&self) -> Option<Offset> {
        match self {
            Self::Date(_) => None,
            Self::Time(_, offset) | Self::DateTime(_, _, offset) => Some(*offset),
        }
    }

    /// Compare two values in time, when both are of one form and either
    /// both are local or both are zoned; `None` otherwise.
    ///
    /// Zoned values are compared on the UTC timeline, so the three zero
    /// offsets `Z`, `+00:00` and `-00:00` compare `Equal`; local values by
    /// their civil fields. Fractions of any length compare as decimal
    /// fractions of a second (`.1` and `.10` are equal). A leap second
    /// sorts after second 59 of its minute.
    ///
    /// Known departure from the order of instants, kept so that the order
    /// of values whose fractions have at most nine digits is unchanged: a
    /// value is placed by its whole seconds (second 60 counting as the
    /// sixtieth second of its minute, so `23:59:60` is second 86400 of its
    /// day) and then its fraction, so a leap second compares `Equal` to the
    /// following minute's second 0 with an equal fraction
    /// (`2024-01-01T23:59:60Z` and `2024-01-02T00:00:00Z`), and a leap
    /// second with a fraction sorts after that second 0, though the leap
    /// second precedes it.
    #[must_use]
    pub fn cmp_chronological(&self, other: &Self) -> Option<Ordering> {
        let seconds = |date: Option<&CivilDate>, time: &CivilTime, offset: Offset| {
            let days = date.map_or(0, |d| d.days_since_epoch());
            days * 86_400 + time.seconds_of_day() - i64::from(offset.minutes()) * 60
        };
        let instant = |date, ta: &CivilTime, oa, db, tb: &CivilTime, ob| {
            seconds(date, ta, oa)
                .cmp(&seconds(db, tb, ob))
                .then_with(|| ta.fraction.cmp_numeric(&tb.fraction))
        };
        match (self, other) {
            (Self::Date(a), Self::Date(b)) => Some(a.days_since_epoch().cmp(&b.days_since_epoch())),
            (Self::Time(ta, oa), Self::Time(tb, ob)) if oa.is_zoned() == ob.is_zoned() => {
                Some(instant(None, ta, *oa, None, tb, *ob))
            }
            (Self::DateTime(da, ta, oa), Self::DateTime(db, tb, ob))
                if oa.is_zoned() == ob.is_zoned() =>
            {
                Some(instant(Some(da), ta, *oa, Some(db), tb, *ob))
            }
            _ => None,
        }
    }

    /// The canonical payload bytes: a form byte (`0` date, `1` time, `2`
    /// date-time), then the date as a little-endian `i64` year and one byte
    /// each for month and day, the time as one byte each for hour, minute
    /// and second followed by the fraction's digit count as a little-endian
    /// `u64` and its digits as ASCII bytes (a count of zero and no digits
    /// when the lexeme wrote no fraction), and the offset as a byte (`0`
    /// [`Offset::Local`], `1` [`Offset::Zulu`], `2` [`Offset::Fixed`], `3`
    /// [`Offset::NegZero`]) followed, for a fixed offset, by its minutes as
    /// a little-endian `i64`.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        fn date(out: &mut Vec<u8>, d: CivilDate) {
            out.extend_from_slice(&i64::from(d.year).to_le_bytes());
            out.extend_from_slice(&[d.month, d.day]);
        }
        fn time(out: &mut Vec<u8>, t: &CivilTime) {
            out.extend_from_slice(&[t.hour, t.minute, t.second]);
            super::put_str(out, t.fraction.as_str());
        }
        fn offset(out: &mut Vec<u8>, o: Offset) {
            match o {
                Offset::Local => out.push(0),
                Offset::Zulu => out.push(1),
                Offset::Fixed(m) => {
                    out.push(2);
                    out.extend_from_slice(&i64::from(m).to_le_bytes());
                }
                Offset::NegZero => out.push(3),
            }
        }
        let mut out = Vec::with_capacity(32);
        match self {
            Self::Date(d) => {
                out.push(0);
                date(&mut out, *d);
            }
            Self::Time(t, o) => {
                out.push(1);
                time(&mut out, t);
                offset(&mut out, *o);
            }
            Self::DateTime(d, t, o) => {
                out.push(2);
                date(&mut out, *d);
                time(&mut out, t);
                offset(&mut out, *o);
            }
        }
        out
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
        match self {
            Self::Local => Ok(()),
            Self::Zulu => f.write_str("Z"),
            Self::NegZero => f.write_str("-00:00"),
            Self::Fixed(m) => {
                let sign = if *m < 0 { '-' } else { '+' };
                let m = m.unsigned_abs();
                write!(f, "{sign}{:02}:{:02}", m / 60, m % 60)
            }
        }
    }
}

impl std::fmt::Display for Temporal {
    /// The RFC 3339 form, which [`Temporal::parse`] reads back into the
    /// same value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Date(d) => write!(f, "{d}"),
            Self::Time(t, o) => write!(f, "{t}{o}"),
            Self::DateTime(d, t, o) => write!(f, "{d}T{t}{o}"),
        }
    }
}
