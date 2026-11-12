//! Complete reference operations assigned to `postgresql-data`.
//!
//! The `PostgreSQL` 17 data route modelled directly from the
//! documentation: the input forms of the column types (Chapter 8 and the
//! input functions it describes), and the rows an `INSERT ... VALUES`
//! statement adds (the `INSERT` reference page), over a script model of
//! two fixed tables. Values are written as canonical strings (`int:42`,
//! `numeric:1.50`, `bool:true`, `bytes:00ff`, `text:abc`) so the model
//! shares no value type, and no code, with the production reader.

use std::fmt::Write as _;

/// Why an input text is refused, by the refusal's class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a form the type reads.
    Syntax,
    /// Out of the type's range.
    Overflow,
    /// More digits after the point than a declared scale.
    Scale,
    /// More integer digits than a declared precision allows.
    Precision,
    /// An infinity a declared `numeric(p, s)` cannot hold.
    NonFinite,
    /// Longer than a declared length, other than by spaces.
    TooLong,
}

/// The column types of the bounded scope.
pub const TYPES: &[&str] = &[
    "smallint",
    "integer",
    "bigint",
    "numeric",
    "numeric(5,2)",
    "boolean",
    "bytea",
    "varchar(3)",
    "character(3)",
    "text",
    "uuid",
];

fn is_space(c: char) -> bool {
    [' ', '\t', '\n', '\r', '\u{0B}', '\u{0C}'].contains(&c)
}

/// The digits of a run with `_` allowed only between two digits of
/// `radix`, or right after a radix prefix when `after_prefix`.
fn digits_of(text: &str, radix: u32, after_prefix: bool) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            let before = if i == 0 {
                after_prefix
            } else {
                chars[i - 1].is_digit(radix)
            };
            let after = chars.get(i + 1).is_some_and(|n| n.is_digit(radix));
            if !before || !after {
                return None;
            }
        } else if c.is_digit(radix) {
            out.push(*c);
        } else {
            return None;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// A sign and the rest of a trimmed text.
fn signed(text: &str) -> (bool, &str) {
    if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = text.strip_prefix('+') {
        (false, rest)
    } else {
        (false, text)
    }
}

/// The magnitude of an unsigned integer text with an optional radix
/// prefix.
fn magnitude(body: &str) -> Result<u128, Refusal> {
    let (radix, rest, prefixed) =
        if let Some(r) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
            (16, r, true)
        } else if let Some(r) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
            (8, r, true)
        } else if let Some(r) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
            (2, r, true)
        } else {
            (10, body, false)
        };
    let digits = digits_of(rest, radix, prefixed).ok_or(Refusal::Syntax)?;
    u128::from_str_radix(&digits, radix).map_err(|_| Refusal::Overflow)
}

fn integer(text: &str, bits: u32) -> Result<String, Refusal> {
    let (negative, body) = signed(text.trim_matches(is_space));
    let m = i128::try_from(magnitude(body)?).map_err(|_| Refusal::Overflow)?;
    let value = if negative { -m } else { m };
    let limit = 1_i128 << (bits - 1);
    if value < -limit || value >= limit {
        return Err(Refusal::Overflow);
    }
    Ok(format!("int:{value}"))
}

/// A `numeric` in positional notation with its display scale, or `None`
/// for text that is no decimal numeral.
fn numeric_plain(text: &str) -> Result<(bool, String, usize), Refusal> {
    let (negative, body) = signed(text);
    if body.starts_with("0x") || body.starts_with("0o") || body.starts_with("0b") {
        let m = magnitude(body)?;
        return Ok((negative && m != 0, m.to_string(), 0));
    }
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((m, e)) => {
            let (exp_negative, exp_digits) = signed(e);
            if exp_digits.is_empty() || !exp_digits.chars().all(|c| c.is_ascii_digit()) {
                return Err(Refusal::Syntax);
            }
            let value: i64 = exp_digits.parse().map_err(|_| Refusal::Overflow)?;
            if value > 1000 {
                return Err(Refusal::Overflow);
            }
            (m, if exp_negative { -value } else { value })
        }
        None => (body, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let int_digits = if int_part.is_empty() {
        String::new()
    } else {
        digits_of(int_part, 10, false).ok_or(Refusal::Syntax)?
    };
    let frac_digits = if frac_part.is_empty() {
        String::new()
    } else {
        digits_of(frac_part, 10, false).ok_or(Refusal::Syntax)?
    };
    if int_digits.is_empty() && frac_digits.is_empty() {
        return Err(Refusal::Syntax);
    }
    let mut all = int_digits + &frac_digits;
    let frac = i64::try_from(frac_digits.len()).map_err(|_| Refusal::Overflow)?;
    let scale = if exponent >= frac {
        for _ in 0..(exponent - frac) {
            all.push('0');
        }
        0
    } else {
        usize::try_from(frac - exponent).map_err(|_| Refusal::Overflow)?
    };
    while all.len() < scale + 1 {
        all.insert(0, '0');
    }
    let point = all.len() - scale;
    let integer = all[..point].trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = &all[point..];
    let zero = integer == "0" && fraction.chars().all(|c| c == '0');
    let text = if scale == 0 {
        integer.to_owned()
    } else {
        format!("{integer}.{fraction}")
    };
    Ok((negative && !zero, text, scale))
}

fn numeric(text: &str, typmod: Option<(usize, usize)>) -> Result<String, Refusal> {
    let t = text.trim_matches(is_space);
    let special = match t.to_ascii_lowercase().as_str() {
        "nan" => Some("NaN"),
        "infinity" | "+infinity" | "inf" | "+inf" => Some("Infinity"),
        "-infinity" | "-inf" => Some("-Infinity"),
        _ => None,
    };
    if let Some(special) = special {
        if typmod.is_some() && special != "NaN" {
            return Err(Refusal::NonFinite);
        }
        return Ok(format!("numeric:{special}"));
    }
    let (negative, mut plain, scale) = numeric_plain(t)?;
    if let Some((precision, declared)) = typmod {
        if scale > declared {
            return Err(Refusal::Scale);
        }
        if scale == 0 && declared > 0 {
            plain.push('.');
        }
        for _ in scale..declared {
            plain.push('0');
        }
        let integer_digits = plain
            .split('.')
            .next()
            .map_or(0, |i| if i == "0" { 0 } else { i.len() });
        if integer_digits > precision - declared {
            return Err(Refusal::Precision);
        }
    }
    let sign = if negative { "-" } else { "" };
    Ok(format!("numeric:{sign}{plain}"))
}

fn boolean(text: &str) -> Result<String, Refusal> {
    let t = text.trim_matches(is_space).to_ascii_lowercase();
    let words: &[(&str, usize, bool)] = &[
        ("true", 1, true),
        ("false", 1, false),
        ("yes", 1, true),
        ("no", 1, false),
        ("on", 2, true),
        ("off", 2, false),
    ];
    for (word, least, value) in words {
        if t.len() >= *least && word.starts_with(&t) && !t.is_empty() {
            return Ok(format!("bool:{value}"));
        }
    }
    match t.as_str() {
        "1" => Ok("bool:true".to_owned()),
        "0" => Ok("bool:false".to_owned()),
        _ => Err(Refusal::Syntax),
    }
}

fn bytea(text: &str) -> Result<String, Refusal> {
    let mut out = String::from("bytes:");
    if let Some(hex) = text.strip_prefix("\\x") {
        let digits: Vec<char> = hex.chars().filter(|c| !" \t\n\r".contains(*c)).collect();
        if digits.len() % 2 != 0 || !digits.iter().all(char::is_ascii_hexdigit) {
            return Err(Refusal::Syntax);
        }
        for d in digits {
            out.push(d.to_ascii_lowercase());
        }
        return Ok(out);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' {
            let mut buf = [0; 4];
            for b in chars[i].encode_utf8(&mut buf).bytes() {
                let _ = write!(out, "{b:02x}");
            }
            i += 1;
        } else if chars.get(i + 1) == Some(&'\\') {
            out.push_str("5c");
            i += 2;
        } else {
            let octal: String = chars.iter().skip(i + 1).take(3).collect();
            let ok = octal.len() == 3
                && octal.starts_with(['0', '1', '2', '3'])
                && octal.chars().all(|c| ('0'..='7').contains(&c));
            if !ok {
                return Err(Refusal::Syntax);
            }
            let byte = u8::from_str_radix(&octal, 8).map_err(|_| Refusal::Syntax)?;
            let _ = write!(out, "{byte:02x}");
            i += 4;
        }
    }
    Ok(out)
}

fn sized(text: &str, max: usize, pad: bool) -> Result<String, Refusal> {
    let chars: Vec<char> = text.chars().collect();
    let mut kept: Vec<char> = chars.iter().copied().take(max).collect();
    if chars.len() > max && chars[max..].iter().any(|c| *c != ' ') {
        return Err(Refusal::TooLong);
    }
    while pad && kept.len() < max {
        kept.push(' ');
    }
    Ok(format!("text:{}", kept.into_iter().collect::<String>()))
}

fn uuid(text: &str) -> Result<String, Refusal> {
    let inner = match text.strip_prefix('{') {
        Some(rest) => rest.strip_suffix('}').ok_or(Refusal::Syntax)?,
        None => text,
    };
    let mut hex = String::new();
    let mut previous_hyphen = true;
    for c in inner.chars() {
        if c == '-' {
            if previous_hyphen || hex.len() % 4 != 0 || hex.len() == 32 {
                return Err(Refusal::Syntax);
            }
            previous_hyphen = true;
        } else if c.is_ascii_hexdigit() {
            hex.push(c.to_ascii_lowercase());
            previous_hyphen = false;
        } else {
            return Err(Refusal::Syntax);
        }
    }
    if hex.len() != 32 || previous_hyphen {
        return Err(Refusal::Syntax);
    }
    Ok(format!(
        "text:{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// The value the input text `text` of a column of type `ty` (one of
/// [`TYPES`]) is, as a canonical string.
///
/// # Errors
///
/// The [`Refusal`] the route gives the text.
pub fn input_value(ty: &str, text: &str) -> Result<String, Refusal> {
    match ty {
        "smallint" => integer(text, 16),
        "integer" => integer(text, 32),
        "bigint" => integer(text, 64),
        "numeric" => numeric(text, None),
        "numeric(5,2)" => numeric(text, Some((5, 2))),
        "boolean" => boolean(text),
        "bytea" => bytea(text),
        "varchar(3)" => sized(text, 3, false),
        "character(3)" => sized(text, 3, true),
        "text" => Ok(format!("text:{text}")),
        "uuid" => uuid(text),
        _ => Err(Refusal::Syntax),
    }
}

/// A constant of the script model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    /// `NULL`.
    Null,
    /// `DEFAULT`.
    Default,
    /// An integer constant, written unquoted.
    Int(i64),
    /// A string constant, written quoted.
    Text(String),
}

/// One `INSERT ... VALUES` of the script model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Insert {
    /// `true` for table `b (x integer)`, `false` for table `a (x integer,
    /// y text, z integer DEFAULT 0)`.
    pub into_b: bool,
    /// The target columns, by position, when listed.
    pub columns: Option<Vec<usize>>,
    /// The rows.
    pub rows: Vec<Vec<Cell>>,
}

/// The DDL the script model's statements run against.
pub const SCHEMA: &str =
    "CREATE TABLE a (x integer, y text, z integer DEFAULT 0);\nCREATE TABLE b (x integer);\n";

const A: &[&str] = &["x", "y", "z"];
const B: &[&str] = &["x"];

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn constant(cell: &Cell) -> String {
    match cell {
        Cell::Null => "NULL".to_owned(),
        Cell::Default => "DEFAULT".to_owned(),
        Cell::Int(n) => n.to_string(),
        Cell::Text(t) => quote(t),
    }
}

/// The text of a script of the model.
#[must_use]
pub fn render(script: &[Insert]) -> String {
    let mut out = String::new();
    for insert in script {
        let (table, names) = if insert.into_b { ("b", B) } else { ("a", A) };
        let _ = write!(out, "INSERT INTO {table}");
        if let Some(columns) = &insert.columns {
            let listed: Vec<&str> = columns.iter().map(|c| names[*c]).collect();
            let _ = write!(out, " ({})", listed.join(", "));
        }
        let rows: Vec<String> = insert
            .rows
            .iter()
            .map(|r| {
                format!(
                    "({})",
                    r.iter().map(constant).collect::<Vec<_>>().join(", ")
                )
            })
            .collect();
        let _ = writeln!(out, " VALUES {};", rows.join(", "));
    }
    out
}

/// Why a script of the model is refused, by the refusal's class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptRefusal {
    /// A row with more values than target columns, or fewer than listed.
    Syntax,
    /// A column left to its declared `DEFAULT`.
    Unsupported,
    /// A constant its column's type does not read.
    Value,
}

/// A table's rows: each row's cells in column order, `None` for `NULL`,
/// `Some` canonical value otherwise.
pub type Rows = Vec<Vec<Option<String>>>;

/// The rows a script adds to `a` and `b`, in the order the reader numbers
/// them: table `a` first, then `b`, each in script order.
///
/// # Errors
///
/// The first [`ScriptRefusal`] in script order.
pub fn read_script(script: &[Insert]) -> Result<(Rows, Rows), ScriptRefusal> {
    let mut a = Vec::new();
    let mut b = Vec::new();
    for insert in script {
        let names = if insert.into_b { B } else { A };
        let targets: Vec<usize> = insert
            .columns
            .clone()
            .unwrap_or_else(|| (0..names.len()).collect());
        for row in &insert.rows {
            if row.len() > targets.len() || (insert.columns.is_some() && row.len() < targets.len())
            {
                return Err(ScriptRefusal::Syntax);
            }
            let mut out: Vec<Option<String>> = vec![None; names.len()];
            let mut given = vec![false; names.len()];
            for (cell, &column) in row.iter().zip(&targets) {
                let text_column = !insert.into_b && column == 1;
                let value = match cell {
                    Cell::Null => None,
                    Cell::Default => continue,
                    Cell::Int(_) if text_column => return Err(ScriptRefusal::Value),
                    Cell::Int(n) => Some(format!("int:{n}")),
                    Cell::Text(t) if text_column => Some(format!("text:{t}")),
                    Cell::Text(t) => Some(integer(t, 32).map_err(|_| ScriptRefusal::Value)?),
                };
                out[column] = value;
                given[column] = true;
            }
            // Column z of `a` declares a DEFAULT, which is not evaluated.
            if !insert.into_b && !given[2] {
                return Err(ScriptRefusal::Unsupported);
            }
            if insert.into_b { &mut b } else { &mut a }.push(out);
        }
    }
    Ok((a, b))
}

/// The canonical `INSERT` text of the rows of `a` and `b`: one statement
/// per table holding rows, tables in name order, every column listed in
/// position order, an integer unquoted, a text as a standard string and
/// `NULL` for a null.
#[must_use]
pub fn canonical_text(a: &Rows, b: &Rows) -> String {
    let mut out = String::new();
    for (table, names, rows) in [("a", A, a), ("b", B, b)] {
        if rows.is_empty() {
            continue;
        }
        let _ = writeln!(out, "INSERT INTO {table} ({}) VALUES", names.join(", "));
        for (i, row) in rows.iter().enumerate() {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| match cell {
                    None => "NULL".to_owned(),
                    Some(v) => match v.split_once(':') {
                        Some(("int", n)) => n.to_owned(),
                        Some((_, t)) => quote(t),
                        None => quote(v),
                    },
                })
                .collect();
            let end = if i + 1 == rows.len() { ";" } else { "," };
            let _ = writeln!(out, "    ({}){end}", cells.join(", "));
        }
    }
    out
}
