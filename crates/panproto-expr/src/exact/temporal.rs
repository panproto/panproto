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
/// assert_eq!((date.year, time.nanos, time.frac_digits), (2026, 789_000_000, 3));
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "TimeParts")]
pub struct CivilTime {
    /// The hour, 0 to 23.
    pub hour: u8,
    /// The minute, 0 to 59.
    pub minute: u8,
    /// The second, 0 to 60 (60 is a leap second).
    pub second: u8,
    /// The fraction of the second in nanoseconds.
    pub nanos: u32,
    /// The number of fractional digits the lexeme wrote, 0 to 9; `nanos`
    /// is a multiple of `10^(9 - frac_digits)`.
    pub frac_digits: u8,
}

/// The UTC offset a lexeme wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "OffsetRepr")]
pub enum Offset {
    /// No offset: a local (floating) time, as TOML local date-times and SQL
    /// `TIMESTAMP WITHOUT TIME ZONE` write.
    Local,
    /// `Z`.
    Utc,
    /// A numeric offset in minutes east of UTC; `+00:00` is `Fixed(0)`.
    Fixed(i16),
    /// RFC 3339's `-00:00`: the time is in UTC and the local offset is
    /// unknown.
    Unknown,
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
    nanos: u32,
    frac_digits: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum OffsetRepr {
    Local,
    Utc,
    Fixed(i16),
    Unknown,
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
        Self::new(p.hour, p.minute, p.second, p.nanos, p.frac_digits)
    }
}

impl TryFrom<OffsetRepr> for Offset {
    type Error = ExactParseError;
    fn try_from(r: OffsetRepr) -> Result<Self, Self::Error> {
        match r {
            OffsetRepr::Local => Ok(Self::Local),
            OffsetRepr::Utc => Ok(Self::Utc),
            OffsetRepr::Unknown => Ok(Self::Unknown),
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

/// `10^(9 - frac_digits)`, the nanosecond step a fraction of
/// `frac_digits` digits moves in.
fn nanos_step(frac_digits: u8) -> u32 {
    10_u32.pow(u32::from(9 - frac_digits.min(9)))
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
    /// The time `hour:minute:second` plus `nanos`, written with
    /// `frac_digits` fractional digits.
    ///
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when a field is out of range or
    /// `nanos` has digits past `frac_digits`.
    pub fn new(
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
        frac_digits: u8,
    ) -> Result<Self, ExactParseError> {
        if hour > 23
            || minute > 59
            || second > 60
            || nanos >= 1_000_000_000
            || frac_digits > 9
            || nanos % nanos_step(frac_digits) != 0
        {
            return Err(invalid());
        }
        Ok(Self {
            hour,
            minute,
            second,
            nanos,
            frac_digits,
        })
    }

    /// Seconds since midnight.
    #[must_use]
    pub fn seconds_of_day(self) -> i64 {
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

    /// Minutes east of UTC; zero for [`Offset::Utc`], [`Offset::Unknown`]
    /// and [`Offset::Local`].
    #[must_use]
    pub const fn minutes(self) -> i16 {
        match self {
            Self::Fixed(m) => m,
            Self::Local | Self::Utc | Self::Unknown => 0,
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
    let (nanos, frac_digits, s) = match s.strip_prefix('.') {
        None => (0, 0, s),
        Some(rest) => {
            let n = rest.bytes().take_while(u8::is_ascii_digit).count();
            if n == 0 {
                return Err(invalid());
            }
            if n > 9 {
                return Err(ExactParseError::new(
                    ExactKind::Temporal,
                    FidelityReason::PrecisionExceeded { max: 9 },
                ));
            }
            let (fraction, rest) = take_digits(rest, n)?;
            let frac_digits = narrow(u32::try_from(n).map_err(|_| invalid())?)?;
            (fraction * nanos_step(frac_digits), frac_digits, rest)
        }
    };
    let time = CivilTime::new(
        narrow(hour)?,
        narrow(minute)?,
        narrow(second)?,
        nanos,
        frac_digits,
    )?;
    Ok((time, s))
}

/// Read the whole of `s` as an offset: empty, `Z`, or `±HH:MM`.
fn read_offset(s: &str) -> Result<Offset, ExactParseError> {
    match s {
        "" => Ok(Offset::Local),
        "Z" | "z" => Ok(Offset::Utc),
        "-00:00" => Ok(Offset::Unknown),
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
    /// # Errors
    ///
    /// [`FidelityReason::InvalidTemporal`] when the lexeme is not one of the
    /// forms or a field is out of range, and
    /// [`FidelityReason::PrecisionExceeded`] for more than nine fractional
    /// digits.
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
    /// Zoned values are compared on the UTC timeline (a `-00:00` time is a
    /// UTC time); local values by their civil fields. A leap second sorts
    /// after second 59 of its minute.
    #[must_use]
    pub fn cmp_chronological(&self, other: &Self) -> Option<Ordering> {
        let instant = |date: Option<&CivilDate>, time: &CivilTime, offset: Offset| {
            let days = date.map_or(0, |d| d.days_since_epoch());
            let seconds = days * 86_400 + time.seconds_of_day() - i64::from(offset.minutes()) * 60;
            (seconds, time.nanos)
        };
        match (self, other) {
            (Self::Date(a), Self::Date(b)) => Some(a.days_since_epoch().cmp(&b.days_since_epoch())),
            (Self::Time(ta, oa), Self::Time(tb, ob)) if oa.is_zoned() == ob.is_zoned() => {
                Some(instant(None, ta, *oa).cmp(&instant(None, tb, *ob)))
            }
            (Self::DateTime(da, ta, oa), Self::DateTime(db, tb, ob))
                if oa.is_zoned() == ob.is_zoned() =>
            {
                Some(instant(Some(da), ta, *oa).cmp(&instant(Some(db), tb, *ob)))
            }
            _ => None,
        }
    }

    /// The canonical payload bytes: a form byte (`0` date, `1` time, `2`
    /// date-time), then the date as a little-endian `i64` year and one byte
    /// each for month and day, the time as one byte each for hour, minute
    /// and second, a little-endian `u32` of nanoseconds and one byte of
    /// fractional digits, and the offset as a byte (`0` local, `1` UTC, `2`
    /// fixed, `3` unknown) followed, for a fixed offset, by its minutes as
    /// a little-endian `i64`.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        fn date(out: &mut Vec<u8>, d: CivilDate) {
            out.extend_from_slice(&i64::from(d.year).to_le_bytes());
            out.extend_from_slice(&[d.month, d.day]);
        }
        fn time(out: &mut Vec<u8>, t: CivilTime) {
            out.extend_from_slice(&[t.hour, t.minute, t.second]);
            out.extend_from_slice(&t.nanos.to_le_bytes());
            out.push(t.frac_digits);
        }
        fn offset(out: &mut Vec<u8>, o: Offset) {
            match o {
                Offset::Local => out.push(0),
                Offset::Utc => out.push(1),
                Offset::Fixed(m) => {
                    out.push(2);
                    out.extend_from_slice(&i64::from(m).to_le_bytes());
                }
                Offset::Unknown => out.push(3),
            }
        }
        let mut out = Vec::with_capacity(28);
        match self {
            Self::Date(d) => {
                out.push(0);
                date(&mut out, *d);
            }
            Self::Time(t, o) => {
                out.push(1);
                time(&mut out, *t);
                offset(&mut out, *o);
            }
            Self::DateTime(d, t, o) => {
                out.push(2);
                date(&mut out, *d);
                time(&mut out, *t);
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
        if self.frac_digits > 0 {
            let fraction = self.nanos / nanos_step(self.frac_digits);
            write!(
                f,
                ".{fraction:0width$}",
                width = usize::from(self.frac_digits)
            )?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Offset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => Ok(()),
            Self::Utc => f.write_str("Z"),
            Self::Unknown => f.write_str("-00:00"),
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
