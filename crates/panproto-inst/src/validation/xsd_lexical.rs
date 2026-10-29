//! The lexical spaces of the XSD datatypes a lexical-space check reads.
//!
//! A value kind's reader may accept more than an XSD datatype's lexical
//! space: the civil-time reader holds a leap second, and the integer reader
//! takes a year written with a leading zero. A lexeme tagged with one of the
//! datatypes below is therefore checked against the datatype's own lexical
//! production (XSD 1.1 Part 2) before it is coerced under the value kind.

/// The XML Schema datatype namespace.
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// Whether `text` is in the lexical space of the XSD datatype `datatype`,
/// or `None` when `datatype` is not one of the datatypes this module
/// covers (`integer`, `decimal`, `double`, `float`, `boolean`, `date`,
/// `dateTime`, `time` and `gYear`).
#[must_use]
pub fn in_lexical_space(datatype: &str, text: &str) -> Option<bool> {
    let local = datatype.strip_prefix(XSD)?;
    Some(match local {
        "integer" => integer(text),
        "decimal" => decimal(text),
        "double" | "float" => double(text),
        "boolean" => matches!(text, "true" | "false" | "1" | "0"),
        "date" => date_time(text, true, false),
        "dateTime" => date_time(text, true, true),
        "time" => date_time(text, false, true),
        "gYear" => year(text).is_some_and(|(_, rest)| timezone(rest)),
        _ => return None,
    })
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn unsigned_decimal(text: &str) -> bool {
    match text.split_once('.') {
        None => digits(text),
        Some((whole, fraction)) => {
            (digits(whole) && (fraction.is_empty() || digits(fraction)))
                || (whole.is_empty() && digits(fraction))
        }
    }
}

fn signed(text: &str) -> &str {
    text.strip_prefix(['+', '-']).unwrap_or(text)
}

fn integer(text: &str) -> bool {
    digits(signed(text))
}

fn decimal(text: &str) -> bool {
    unsigned_decimal(signed(text))
}

fn double(text: &str) -> bool {
    if matches!(text, "INF" | "+INF" | "-INF" | "NaN") {
        return true;
    }
    let body = signed(text);
    match body.split_once(['e', 'E']) {
        None => unsigned_decimal(body),
        Some((mantissa, exponent)) => unsigned_decimal(mantissa) && integer(exponent),
    }
}

/// The year fragment at the start of `text` and the rest: an optional
/// minus, then four digits with no leading zero beyond four, and the year
/// value with its sign.
fn year(text: &str) -> Option<(i64, &str)> {
    let (negative, body) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    let end = body
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(body.len());
    let year_digits = &body[..end];
    if year_digits.len() < 4 || (year_digits.len() > 4 && year_digits.starts_with('0')) {
        return None;
    }
    let value: i64 = year_digits.parse().ok()?;
    Some((if negative { -value } else { value }, &body[end..]))
}

fn two_digits(text: &str) -> Option<(u32, &str)> {
    let head = text.get(..2)?;
    if !digits(head) {
        return None;
    }
    Some((head.parse().ok()?, &text[2..]))
}

const fn leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn timezone(text: &str) -> bool {
    if text.is_empty() || text == "Z" {
        return true;
    }
    let Some(rest) = text.strip_prefix(['+', '-']) else {
        return false;
    };
    let Some((hours, rest)) = two_digits(rest) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(':') else {
        return false;
    };
    let Some((minutes, rest)) = two_digits(rest) else {
        return false;
    };
    rest.is_empty() && ((hours <= 13 && minutes <= 59) || (hours == 14 && minutes == 0))
}

/// The time of day at the start of `text` and the rest: `hh:mm:ss` with an
/// optional fraction, seconds below 60, or `24:00:00` with a zero fraction.
fn time_of_day(text: &str) -> Option<&str> {
    let (hours, rest) = two_digits(text)?;
    let (minutes, rest) = two_digits(rest.strip_prefix(':')?)?;
    let (seconds, rest) = two_digits(rest.strip_prefix(':')?)?;
    let (fraction, rest) = match rest.strip_prefix('.') {
        Some(after) => {
            let end = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            if end == 0 {
                return None;
            }
            (Some(&after[..end]), &after[end..])
        }
        None => (None, rest),
    };
    let midnight_end = hours == 24
        && minutes == 0
        && seconds == 0
        && fraction.is_none_or(|f| f.bytes().all(|b| b == b'0'));
    (midnight_end || (hours <= 23 && minutes <= 59 && seconds <= 59)).then_some(rest)
}

fn date_time(text: &str, with_date: bool, with_time: bool) -> bool {
    let mut rest = text;
    if with_date {
        let Some((year_value, after)) = year(rest) else {
            return false;
        };
        let Some((month, after)) = after.strip_prefix('-').and_then(two_digits) else {
            return false;
        };
        let Some((day, after)) = after.strip_prefix('-').and_then(two_digits) else {
            return false;
        };
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year_value, month) {
            return false;
        }
        rest = after;
        if with_time {
            let Some(after) = rest.strip_prefix('T') else {
                return false;
            };
            rest = after;
        }
    }
    if with_time {
        let Some(after) = time_of_day(rest) else {
            return false;
        };
        rest = after;
    }
    timezone(rest)
}

#[cfg(test)]
mod tests {
    use super::in_lexical_space;

    fn xsd(local: &str) -> String {
        format!("http://www.w3.org/2001/XMLSchema#{local}")
    }

    #[test]
    fn the_datatype_amendment_cases_fall_on_the_stated_sides() {
        for (datatype, text, inside) in [
            ("integer", "abc", false),
            ("date", "2026-02-30", false),
            ("dateTime", "2026-01-01T23:59:60Z", false),
            ("gYear", "01999", false),
            ("double", "inf", false),
            ("dateTime", "2026-01-01T24:00:00Z", true),
            ("double", "INF", true),
            ("decimal", "1.", true),
            ("decimal", ".5", true),
            ("date", "2024-02-29", true),
            ("gYear", "1999", true),
        ] {
            assert_eq!(
                in_lexical_space(&xsd(datatype), text),
                Some(inside),
                "{datatype} {text}"
            );
        }
        assert_eq!(in_lexical_space(&xsd("QName"), "x"), None);
        assert_eq!(in_lexical_space("http://e/unknown", "x"), None);
    }
}
