//! Dates, times and date-times with every written form kept, and their
//! partial chronological order.
//!
//! A value records exactly what was written: which fields are present (a
//! year, month and day; an hour, minute and second; a fractional second of
//! any length), the separator between a date and a time, the written case
//! of `Z`, the written form of a numeric offset and a written `-` on year
//! zero. Values that differ only in written forms are distinct values that
//! denote one moment, and [`cmp_chronological`] compares them `Equal`.

use std::cmp::Ordering;
use std::fmt;

/// The case of a written `Z` offset, or none written (a YAML timestamp with
/// no zone reads as UTC with its zone unwritten).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ZuluCase {
    /// `Z`.
    Upper,
    /// `z`.
    Lower,
    /// No zone was written; the value is read as UTC.
    Unwritten,
}

/// The written form of a numeric offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OffsetForm {
    /// `+hh:mm`.
    Extended,
    /// `+hhmm`.
    Basic,
    /// `+hh`.
    HourOnly,
}

/// The offset a value was written with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Offset {
    /// No offset: a local value, incomparable with a zoned one.
    Local,
    /// `Z` or `z` (or an unwritten UTC zone).
    Zulu(ZuluCase),
    /// A numeric offset in minutes east of UTC, with its written form; zero
    /// minutes is `+00:00` (or `+0000`, `+00`).
    Fixed(i16, OffsetForm),
    /// `-00:00` (or `-0000`, `-00`): UTC known, local offset unknown.
    NegZero(OffsetForm),
}

impl Offset {
    /// Minutes east of UTC; `None` for a local value.
    #[must_use]
    pub const fn minutes(self) -> Option<i64> {
        match self {
            Self::Local => None,
            Self::Zulu(_) | Self::NegZero(_) => Some(0),
            Self::Fixed(m, _) => Some(m as i64),
        }
    }

    /// Whether the value is zoned (carries an offset).
    #[must_use]
    pub const fn is_zoned(self) -> bool {
        !matches!(self, Self::Local)
    }
}

/// The year as written: its value and whether `-` was written on zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Year {
    /// The year (year 0 is 1 BCE, proleptic Gregorian).
    pub value: i64,
    /// Whether the lexeme was `-0000` (a sign its value does not fix).
    pub negative_zero: bool,
    /// The number of digits written (at least four).
    pub width: usize,
}

/// The date fields present.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DateFields {
    /// The year, absent in a front-truncated date.
    pub year: Option<Year>,
    /// The month, absent in a year or a day-only date.
    pub month: Option<u8>,
    /// The day, absent in a year or year-month.
    pub day: Option<u8>,
}

/// The time fields present.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimeFields {
    /// The hour, absent in a front-truncated time.
    pub hour: Option<u8>,
    /// The minute, absent in an hour-only or second-only time.
    pub minute: Option<u8>,
    /// The second (60 for a leap second), absent at reduced precision.
    pub second: Option<u8>,
    /// The fractional-second digits as written (empty when none).
    pub fraction: String,
}

/// The separator written between a date and a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Separator {
    /// `T`.
    Upper,
    /// `t`.
    Lower,
    /// A space.
    Space,
}

/// A temporal value: a date, a time, or both, and an offset.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Temporal {
    /// The date part.
    pub date: Option<DateFields>,
    /// The time part.
    pub time: Option<TimeFields>,
    /// The separator, present exactly when both parts are.
    pub separator: Option<Separator>,
    /// The offset.
    pub offset: Offset,
}

/// Which fields a value has: the comparability classes of the order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shape {
    /// Date fields present (year, month, day).
    pub date: Option<(bool, bool, bool)>,
    /// Time fields present (hour, minute, second).
    pub time: Option<(bool, bool, bool)>,
}

impl Temporal {
    /// The shape: which fields are present.
    #[must_use]
    pub fn shape(&self) -> Shape {
        Shape {
            date: self
                .date
                .map(|d| (d.year.is_some(), d.month.is_some(), d.day.is_some())),
            time: self
                .time
                .as_ref()
                .map(|t| (t.hour.is_some(), t.minute.is_some(), t.second.is_some())),
        }
    }

    /// Whether the shape is front-truncated (a date without its year, or a
    /// time-only value without its hour).
    #[must_use]
    pub fn is_truncated(&self) -> bool {
        self.date.is_some_and(|d| d.year.is_none())
            || (self.date.is_none() && self.time.as_ref().is_some_and(|t| t.hour.is_none()))
    }

    /// Whether this is one of the complete forms: a full date, a full time,
    /// or a full date and full time.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        let full_date = self
            .date
            .is_none_or(|d| d.year.is_some() && d.month.is_some() && d.day.is_some());
        let full_time = self
            .time
            .as_ref()
            .is_none_or(|t| t.hour.is_some() && t.minute.is_some() && t.second.is_some());
        full_date && full_time && (self.date.is_some() || self.time.is_some())
    }
}

/// Days from 0000-03-01 style counting avoided: the number of days before
/// January 1 of year `y` since January 1 of year 0, proleptic Gregorian,
/// from the leap rule (divisible by 4, except by 100 unless by 400).
fn days_before_year(y: i64) -> i64 {
    // Leap years in [0, y): ceil(y/4) - ceil(y/100) + ceil(y/400), which for
    // negative y counts backwards; div_euclid keeps it exact both ways.
    let leaps = |k: i64| (y + k - 1).div_euclid(k);
    365 * y + leaps(4) - leaps(100) + leaps(400)
}

/// Whether `y` is a leap year.
#[must_use]
pub const fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// The number of days of month `m` (1 to 12) in year `y`.
#[must_use]
pub const fn month_length(y: i64, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// The day number of a full date since January 1 of year 0.
#[must_use]
pub fn day_number(y: i64, m: u8, d: u8) -> i64 {
    let months: i64 = (1..m).map(|k| i64::from(month_length(y, k))).sum();
    days_before_year(y) + months + i64::from(d) - 1
}

/// The key a value is ordered by within its comparability class.
///
/// For a value with a year: the minute index of its first moment (days of
/// its first day times 1440 plus hour times 60 plus minute, missing fields
/// taken at their least value) less its offset in minutes when zoned, then
/// the second and the fraction. A leap second is the second 60 of its
/// minute, after second 59 and before the next minute's second 0. For a
/// value without a year that is not truncated (a time): the same with no
/// days. For a truncated value: its present fields, most significant first.
fn order_key(v: &Temporal) -> (Vec<i64>, String) {
    let fraction = v
        .time
        .as_ref()
        .map(|t| t.fraction.clone())
        .unwrap_or_default();
    let second = v.time.as_ref().and_then(|t| t.second).map_or(0, i64::from);
    if v.is_truncated() {
        let mut fields = Vec::new();
        if let Some(d) = v.date {
            fields.extend(d.month.map(i64::from));
            fields.extend(d.day.map(i64::from));
        }
        if let Some(t) = &v.time {
            fields.extend(t.hour.map(i64::from));
            fields.extend(t.minute.map(i64::from));
            fields.extend(t.second.map(i64::from));
        }
        return (fields, fraction);
    }
    let days = v.date.and_then(|d| {
        d.year
            .map(|y| day_number(y.value, d.month.unwrap_or(1), d.day.unwrap_or(1)))
    });
    let hour = v.time.as_ref().and_then(|t| t.hour).map_or(0, i64::from);
    let minute = v.time.as_ref().and_then(|t| t.minute).map_or(0, i64::from);
    let shift = v.offset.minutes().unwrap_or(0);
    let minutes = days.unwrap_or(0) * 1440 + hour * 60 + minute - shift;
    (vec![minutes, second], fraction)
}

/// Compare two fractional-second digit strings as decimal fractions: the
/// shorter is padded with zeros, then the digit strings compare as
/// integers over the common power of ten.
#[must_use]
pub fn cmp_fraction(a: &str, b: &str) -> Ordering {
    let n = a.len().max(b.len());
    format!("{a:0<n$}").cmp(&format!("{b:0<n$}"))
}

/// Whether two values are comparable: one shape, both local or both zoned,
/// and, for a truncated shape, offsets of equal minutes.
#[must_use]
pub fn comparable(a: &Temporal, b: &Temporal) -> bool {
    a.shape() == b.shape()
        && a.offset.is_zoned() == b.offset.is_zoned()
        && (!a.is_truncated() || a.offset.minutes() == b.offset.minutes())
}

/// The partial chronological order: `None` for incomparable values, else
/// the order of `order_key` with the fraction compared as a decimal
/// fraction.
#[must_use]
pub fn cmp_chronological(a: &Temporal, b: &Temporal) -> Option<Ordering> {
    if !comparable(a, b) {
        return None;
    }
    let (ka, fa) = order_key(a);
    let (kb, fb) = order_key(b);
    Some(ka.cmp(&kb).then_with(|| cmp_fraction(&fa, &fb)))
}

/// The kind a parser reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemporalKind {
    /// Date forms (complete, reduced, front-truncated).
    Date,
    /// Time forms (complete, reduced, front-truncated).
    Time,
    /// Date-time forms.
    DateTime,
}

fn digits_n(s: &str, n: usize) -> Option<u8> {
    if s.len() == n && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

/// A year: `'-'? ([1-9] d d d+ | '0' d d d)` (XML Schema `yearFrag`).
fn parse_year(s: &str) -> Option<Year> {
    let (neg, body) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let bytes = body.as_bytes();
    let ok = bytes.iter().all(u8::is_ascii_digit)
        && ((bytes.len() == 4) || (bytes.len() > 4 && bytes[0] != b'0'));
    if !ok {
        return None;
    }
    let magnitude: i64 = body.parse().ok()?;
    Some(Year {
        value: if neg { -magnitude } else { magnitude },
        negative_zero: neg && magnitude == 0,
        width: body.len(),
    })
}

fn valid_date(d: &DateFields) -> bool {
    let month_ok = d.month.is_none_or(|m| (1..=12).contains(&m));
    let day_ok = d.day.is_none_or(|day| {
        let limit = match (d.year, d.month) {
            (Some(y), Some(m)) => month_length(y.value, m),
            (None, Some(m)) => month_length(0, m),
            _ => 31,
        };
        day >= 1 && day <= limit
    });
    month_ok && day_ok
}

fn parse_date_body(s: &str) -> Option<DateFields> {
    let d = if let Some(rest) = s.strip_prefix("---") {
        DateFields {
            year: None,
            month: None,
            day: Some(digits_n(rest, 2)?),
        }
    } else if let Some(rest) = s.strip_prefix("--") {
        match rest.split_once('-') {
            Some((m, d)) => DateFields {
                year: None,
                month: Some(digits_n(m, 2)?),
                day: Some(digits_n(d, 2)?),
            },
            None => DateFields {
                year: None,
                month: Some(digits_n(rest, 2)?),
                day: None,
            },
        }
    } else {
        let (sign, body) = s.strip_prefix('-').map_or(("", s), |r| ("-", r));
        let mut parts = body.splitn(3, '-');
        let year = parse_year(&format!("{sign}{}", parts.next()?))?;
        let month = parts
            .next()
            .map(|m| digits_n(m, 2))
            .map_or(Some(None), |m| m.map(Some))?;
        let day = parts
            .next()
            .map(|d| digits_n(d, 2))
            .map_or(Some(None), |d| d.map(Some))?;
        DateFields {
            year: Some(year),
            month,
            day,
        }
    };
    valid_date(&d).then_some(d)
}

fn parse_seconds(s: &str) -> Option<(u8, String)> {
    let (sec, frac) = s.split_once('.').map_or((s, None), |(a, b)| (a, Some(b)));
    let second = digits_n(sec, 2)?;
    let fraction = match frac {
        None => String::new(),
        Some(f) if !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) => f.to_owned(),
        Some(_) => return None,
    };
    (second <= 60).then_some((second, fraction))
}

fn parse_time_body(s: &str) -> Option<TimeFields> {
    let t = if let Some(rest) = s.strip_prefix("--") {
        let (second, fraction) = parse_seconds(rest)?;
        TimeFields {
            hour: None,
            minute: None,
            second: Some(second),
            fraction,
        }
    } else if let Some(rest) = s.strip_prefix('-') {
        match rest.split_once(':') {
            Some((m, sec)) => {
                let (second, fraction) = parse_seconds(sec)?;
                TimeFields {
                    hour: None,
                    minute: Some(digits_n(m, 2)?),
                    second: Some(second),
                    fraction,
                }
            }
            None => TimeFields {
                hour: None,
                minute: Some(digits_n(rest, 2)?),
                second: None,
                fraction: String::new(),
            },
        }
    } else {
        let mut parts = s.splitn(3, ':');
        let hour = digits_n(parts.next()?, 2)?;
        let minute = parts
            .next()
            .map(|m| digits_n(m, 2))
            .map_or(Some(None), |m| m.map(Some))?;
        let (second, fraction) = match parts.next() {
            None => (None, String::new()),
            Some(sec) => {
                let (second, fraction) = parse_seconds(sec)?;
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
    let ok = t.hour.is_none_or(|h| h <= 23) && t.minute.is_none_or(|m| m <= 59);
    ok.then_some(t)
}

fn parse_offset(s: &str) -> Option<Offset> {
    match s {
        "" => return Some(Offset::Local),
        "Z" => return Some(Offset::Zulu(ZuluCase::Upper)),
        "z" => return Some(Offset::Zulu(ZuluCase::Lower)),
        _ => {}
    }
    let (negative, body) = match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => return None,
    };
    let (h, m, form) = if let Some((h, m)) = body.split_once(':') {
        (digits_n(h, 2)?, digits_n(m, 2)?, OffsetForm::Extended)
    } else if body.len() == 4 {
        (
            digits_n(&body[..2], 2)?,
            digits_n(&body[2..], 2)?,
            OffsetForm::Basic,
        )
    } else {
        (digits_n(body, 2)?, 0, OffsetForm::HourOnly)
    };
    if h > 23 || m > 59 {
        return None;
    }
    let minutes = i16::from(h) * 60 + i16::from(m);
    Some(if minutes == 0 && negative {
        Offset::NegZero(form)
    } else {
        Offset::Fixed(if negative { -minutes } else { minutes }, form)
    })
}

/// Every split of `s` into a body and a trailing offset lexeme (possibly
/// empty) that both parse.
fn splits<T>(s: &str, body: impl Fn(&str) -> Option<T>) -> Vec<(T, Offset)> {
    let mut out = Vec::new();
    for cut in (0..=s.len()).rev() {
        if !s.is_char_boundary(cut) {
            continue;
        }
        if let (Some(b), Some(o)) = (body(&s[..cut]), parse_offset(&s[cut..])) {
            out.push((b, o));
        }
    }
    out
}

fn single<T>(mut v: Vec<T>) -> Option<T> {
    if v.len() == 1 { v.pop() } else { None }
}

fn longest_body<T>(v: Vec<T>) -> Option<T> {
    v.into_iter().next()
}

/// Read a lexeme of `kind`; `None` when it is not one form of that kind.
///
/// When a lexeme splits into a body and an offset in more than one way the
/// longest body is read: `1973-06` is a year and month, not the year 1973
/// at offset `-06`. (A year or year-month with an hour-only or basic
/// offset whose digits could continue the date is therefore not
/// readable; its `Display` reads back as the longer date.)
#[must_use]
pub fn parse(kind: TemporalKind, s: &str) -> Option<Temporal> {
    match kind {
        TemporalKind::Date => {
            longest_body(splits(s, parse_date_body)).map(|(d, offset)| Temporal {
                date: Some(d),
                time: None,
                separator: None,
                offset,
            })
        }
        TemporalKind::Time => {
            longest_body(splits(s, parse_time_body)).map(|(t, offset)| Temporal {
                date: None,
                time: Some(t),
                separator: None,
                offset,
            })
        }
        TemporalKind::DateTime => {
            let parsed = splits(s, |body| {
                let at = body.find(['T', 't', ' '])?;
                let separator = match &body[at..=at] {
                    "T" => Separator::Upper,
                    "t" => Separator::Lower,
                    _ => Separator::Space,
                };
                let date = parse_date_body(&body[..at])?;
                let full = date.year.is_some() && date.month.is_some() && date.day.is_some();
                let day_only = date.year.is_none() && date.month.is_none() && date.day.is_some();
                let time = parse_time_body(&body[at + 1..])?;
                (time.hour.is_some() && (full || day_only)).then_some((date, separator, time))
            });
            longest_body(parsed).map(|((date, separator, time), offset)| Temporal {
                date: Some(date),
                time: Some(time),
                separator: Some(separator),
                offset,
            })
        }
    }
}

/// Read a lexeme with no declared kind: exactly one kind must read it.
#[must_use]
pub fn parse_any(s: &str) -> Option<Temporal> {
    let all: Vec<Temporal> = [
        TemporalKind::Date,
        TemporalKind::Time,
        TemporalKind::DateTime,
    ]
    .into_iter()
    .filter_map(|k| parse(k, s))
    .collect();
    single(all)
}

/// The parse-then-display law for a kind-directed accepted lexeme.
#[must_use]
pub fn parse_display(kind: TemporalKind, text: &str) -> bool {
    parse(kind, text).is_some_and(|value| value.to_string() == text)
}

/// The display-then-parse law for a value of `kind`.
#[must_use]
pub fn display_parse(kind: TemporalKind, value: &Temporal) -> bool {
    parse(kind, &value.to_string()).as_ref() == Some(value)
}

/// Canonical bytes of a temporal value.
#[must_use]
pub fn canonical_bytes(value: &Temporal) -> Option<Vec<u8>> {
    crate::canonical::value_bytes(&crate::value::Value::Temporal(value.clone()))
}

/// Whether a value records YAML's unwritten-UTC representation.
#[must_use]
pub const fn unwritten_zulu(value: &Temporal) -> bool {
    matches!(value.offset, Offset::Zulu(ZuluCase::Unwritten))
}

/// Unit of an integral epoch count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeUnit {
    /// Seconds.
    Seconds,
    /// Milliseconds.
    Millis,
    /// Microseconds.
    Micros,
    /// Nanoseconds.
    Nanos,
}

impl TimeUnit {
    const fn digits(self) -> u32 {
        match self {
            Self::Seconds => 0,
            Self::Millis => 3,
            Self::Micros => 6,
            Self::Nanos => 9,
        }
    }

    const fn ticks(self) -> i128 {
        10i128.pow(self.digits())
    }
}

fn days_before_year_wide(year: i128) -> i128 {
    let leaps = |divisor: i128| (year + divisor - 1).div_euclid(divisor);
    365 * year + leaps(4) - leaps(100) + leaps(400)
}

fn date_from_day(day: i128) -> Option<(i64, u8, u8)> {
    let cycle = day.div_euclid(146_097);
    let mut year = cycle.checked_mul(400)?;
    while days_before_year_wide(year + 1) <= day {
        year += 1;
    }
    while days_before_year_wide(year) > day {
        year -= 1;
    }
    let year_i64 = i64::try_from(year).ok()?;
    let mut within = day - days_before_year_wide(year);
    let mut month = 1u8;
    while month <= 12 {
        let length = i128::from(month_length(year_i64, month));
        if within < length {
            return Some((year_i64, month, u8::try_from(within + 1).ok()?));
        }
        within -= length;
        month += 1;
    }
    None
}

/// Convert an epoch count to the exact date-time in its unit.
///
/// The epoch is 1970-01-01T00:00:00. An instant column writes `Z`; a
/// wall-clock column writes no offset.
#[must_use]
pub fn from_epoch_count(count: i128, unit: TimeUnit, instant: bool) -> Option<Temporal> {
    let ticks = unit.ticks();
    let seconds = count.div_euclid(ticks);
    let fraction = count.rem_euclid(ticks);
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
    let epoch = i128::from(day_number(1970, 1, 1));
    let (year, month, day) = date_from_day(epoch.checked_add(days)?)?;
    let width = usize::try_from(unit.digits()).ok()?;
    Some(Temporal {
        date: Some(DateFields {
            year: Some(Year {
                value: year,
                negative_zero: false,
                width: 4,
            }),
            month: Some(month),
            day: Some(day),
        }),
        time: Some(TimeFields {
            hour: Some(u8::try_from(in_day / 3_600).ok()?),
            minute: Some(u8::try_from(in_day % 3_600 / 60).ok()?),
            second: Some(u8::try_from(in_day % 60).ok()?),
            fraction: if width == 0 {
                String::new()
            } else {
                format!("{fraction:0width$}")
            },
        }),
        separator: Some(Separator::Upper),
        offset: if instant {
            Offset::Zulu(ZuluCase::Upper)
        } else {
            Offset::Local
        },
    })
}

/// Convert a date-time in the image of [`from_epoch_count`] back to its
/// count.
#[must_use]
pub fn to_epoch_count(value: &Temporal, unit: TimeUnit, instant: bool) -> Option<i128> {
    let date = value.date?;
    let time = value.time.as_ref()?;
    let year = date.year?;
    if year.negative_zero
        || value.separator != Some(Separator::Upper)
        || value.offset
            != if instant {
                Offset::Zulu(ZuluCase::Upper)
            } else {
                Offset::Local
            }
    {
        return None;
    }
    let (month, day, hour, minute, second) = (
        date.month?,
        date.day?,
        time.hour?,
        time.minute?,
        time.second?,
    );
    let digits = usize::try_from(unit.digits()).ok()?;
    if time.fraction.len() != digits || !time.fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = if digits == 0 {
        0
    } else {
        time.fraction.parse::<i128>().ok()?
    };
    let epoch = i128::from(day_number(1970, 1, 1));
    let days = i128::from(day_number(year.value, month, day)).checked_sub(epoch)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add(i128::from(hour) * 3_600)?
        .checked_add(i128::from(minute) * 60)?
        .checked_add(i128::from(second))?;
    seconds.checked_mul(unit.ticks())?.checked_add(fraction)
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let numeric = |f: &mut fmt::Formatter<'_>, sign: char, m: i64, form: OffsetForm| {
            let (h, mm) = (m / 60, m % 60);
            match form {
                OffsetForm::Extended => write!(f, "{sign}{h:02}:{mm:02}"),
                OffsetForm::Basic => write!(f, "{sign}{h:02}{mm:02}"),
                OffsetForm::HourOnly => write!(f, "{sign}{h:02}"),
            }
        };
        match *self {
            Self::Local | Self::Zulu(ZuluCase::Unwritten) => Ok(()),
            Self::Zulu(ZuluCase::Upper) => f.write_str("Z"),
            Self::Zulu(ZuluCase::Lower) => f.write_str("z"),
            Self::NegZero(form) => numeric(f, '-', 0, form),
            Self::Fixed(m, form) => {
                let sign = if m < 0 { '-' } else { '+' };
                numeric(f, sign, i64::from(m).abs(), form)
            }
        }
    }
}

fn write_date(f: &mut fmt::Formatter<'_>, d: &DateFields) -> fmt::Result {
    match d.year {
        Some(y) => {
            let sign = if y.value < 0 || y.negative_zero {
                "-"
            } else {
                ""
            };
            write!(f, "{sign}{:0w$}", y.value.abs(), w = y.width)?;
            if let Some(m) = d.month {
                write!(f, "-{m:02}")?;
            }
            if let Some(day) = d.day {
                write!(f, "-{day:02}")?;
            }
            Ok(())
        }
        None => match (d.month, d.day) {
            (Some(m), Some(day)) => write!(f, "--{m:02}-{day:02}"),
            (Some(m), None) => write!(f, "--{m:02}"),
            (None, Some(day)) => write!(f, "---{day:02}"),
            (None, None) => Ok(()),
        },
    }
}

fn write_time(out: &mut fmt::Formatter<'_>, time: &TimeFields) -> fmt::Result {
    let frac = |out: &mut fmt::Formatter<'_>| {
        if time.fraction.is_empty() {
            Ok(())
        } else {
            write!(out, ".{}", time.fraction)
        }
    };
    match (time.hour, time.minute, time.second) {
        (Some(hour), None, _) => write!(out, "{hour:02}"),
        (Some(hour), Some(minute), None) => write!(out, "{hour:02}:{minute:02}"),
        (Some(hour), Some(minute), Some(second)) => {
            write!(out, "{hour:02}:{minute:02}:{second:02}")?;
            frac(out)
        }
        (None, Some(minute), Some(second)) => {
            write!(out, "-{minute:02}:{second:02}")?;
            frac(out)
        }
        (None, Some(minute), None) => write!(out, "-{minute:02}"),
        (None, None, Some(second)) => {
            write!(out, "--{second:02}")?;
            frac(out)
        }
        (None, None, None) => Ok(()),
    }
}

impl fmt::Display for Temporal {
    /// The lexeme back, in every written form the value records.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(d) = &self.date {
            write_date(f, d)?;
        }
        if let Some(s) = self.separator {
            f.write_str(match s {
                Separator::Upper => "T",
                Separator::Lower => "t",
                Separator::Space => " ",
            })?;
        }
        if let Some(t) = &self.time {
            write_time(f, t)?;
        }
        write!(f, "{}", self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> Option<Temporal> {
        parse(TemporalKind::DateTime, s)
    }

    #[test]
    fn every_written_form_displays_back() {
        for s in [
            "2024-01-02T03:04:05z",
            "2024-01-02t03:04:05Z",
            "2024-01-02 03:04:05Z",
            "2024-01-02T03:04:05.120+05:30",
            "1985-04-12T23",
            "1979-05-27 07:32Z",
            "---15T09:45:00+01:00",
            "2024-01-02T03:04:05-0000",
        ] {
            assert_eq!(dt(s).map(|v| v.to_string()), Some(s.to_owned()), "{s}");
        }
        for s in [
            "2018",
            "-0044",
            "1973-06",
            "--04-12",
            "---12",
            "-0000",
            "2018Z",
            "2002-10-10+13:00",
        ] {
            assert_eq!(
                parse(TemporalKind::Date, s).map(|v| v.to_string()),
                Some(s.to_owned()),
                "{s}"
            );
        }
        for s in [
            "03:04:05z",
            "23",
            "23:20",
            "-20:50",
            "-20",
            "--50",
            "12:00:00.1234567891",
            "23:20+04",
        ] {
            assert_eq!(
                parse(TemporalKind::Time, s).map(|v| v.to_string()),
                Some(s.to_owned()),
                "{s}"
            );
        }
        assert_eq!(parse_any("--04"), None);
        assert!(
            parse(TemporalKind::Date, "--04").is_some()
                && parse(TemporalKind::Time, "--04").is_some()
        );
        assert_eq!(parse(TemporalKind::Date, "+2018"), None);
        assert_eq!(parse(TemporalKind::Date, "2023-02-29"), None);
    }

    #[test]
    fn written_forms_of_one_moment_compare_equal() {
        let zulu = dt("2024-01-02T03:04:05Z");
        let lower = dt("2024-01-02t03:04:05z");
        let plus = dt("2024-01-02T03:04:05+00:00");
        let negzero = dt("2024-01-02T03:04:05-00:00");
        let shifted = dt("2024-01-02T08:34:05+05:30");
        let Some(base) = zulu else { return };
        for other in [lower, plus, negzero, shifted].into_iter().flatten() {
            assert_eq!(cmp_chronological(&base, &other), Some(Ordering::Equal));
        }
        let local = dt("2024-01-02T03:04:05");
        assert_eq!(
            local.map(|local| cmp_chronological(&base, &local)),
            Some(None)
        );
    }

    #[test]
    fn fractions_compare_as_decimal_fractions_and_a_leap_second_precedes_the_next_minute() {
        let p = |s: &str| parse(TemporalKind::Time, s);
        assert_eq!(
            p("12:00:00.1")
                .zip(p("12:00:00.10"))
                .map(|(a, b)| cmp_chronological(&a, &b)),
            Some(Some(Ordering::Equal))
        );
        assert_eq!(
            p("12:00:00.09")
                .zip(p("12:00:00.1"))
                .map(|(a, b)| cmp_chronological(&a, &b)),
            Some(Some(Ordering::Less))
        );
        let leap = dt("2016-12-31T23:59:60Z");
        let next = dt("2017-01-01T00:00:00Z");
        let before = dt("2016-12-31T23:59:59.999Z");
        assert_eq!(
            leap.clone()
                .zip(next)
                .map(|(a, b)| cmp_chronological(&a, &b)),
            Some(Some(Ordering::Less))
        );
        assert_eq!(
            before.zip(leap).map(|(a, b)| cmp_chronological(&a, &b)),
            Some(Some(Ordering::Less))
        );
        // A year is placed by the UTC instant of its first moment.
        let y = parse(TemporalKind::Date, "2018+05:00");
        let z = parse(TemporalKind::Date, "2017Z");
        assert_eq!(
            y.zip(z).map(|(a, b)| cmp_chronological(&a, &b)),
            Some(Some(Ordering::Greater))
        );
    }
}
