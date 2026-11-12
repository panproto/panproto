//! Complete finite scopes for the postgresql-data page.

use crate::pages::postgresql_data::{Cell, Insert, TYPES};

/// The input texts of the bounded scope.
///
/// They are integers in and out of range and
/// in every radix, numerals with and without scale and exponent, the
/// special values, boolean spellings, bytea in both formats, texts around
/// a length of three, and UUID forms.
pub const SPELLINGS: &[&str] = &[
    "42",
    " -7 ",
    "+0",
    "1_000",
    "1__0",
    "_1",
    "1_",
    "0x1F",
    "0o17",
    "0b101",
    "0x",
    "0x_1",
    "32767",
    "32768",
    "-32769",
    "2147483648",
    "-2147483648",
    "9223372036854775807",
    "9223372036854775808",
    "-9223372036854775808",
    "1.5",
    "1.50",
    "1e2",
    "1.50e1",
    ".5",
    "5.",
    "-0.00",
    "1e-2",
    "123.456",
    "1234.5",
    "999.995",
    "1e1001",
    "1e+3",
    "1e",
    ".",
    "NaN",
    "-inf",
    "Infinity",
    "nan ",
    "t",
    "TRUE",
    "o",
    "of",
    "on",
    "1",
    "0",
    "yes",
    "n",
    "\\x00ff",
    "\\x0",
    "\\x 0a 0B",
    "a\\\\b",
    "\\001",
    "\\400",
    "ab",
    "abcd",
    "abc  ",
    "",
    " ",
    "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
    "{A0EEBC999C0B4EF8BB6D6BB9BD380A11}",
    "a0eebc99-9c0b4ef8",
    "a0ee-bc99-9c0b-4ef8-bb6d-6bb9-bd38-0a11",
    "-a0eebc999c0b4ef8bb6d6bb9bd380a11",
];

/// Every `(type, input text)` pair of the bounded scope.
#[must_use]
pub fn value_cases() -> Vec<(&'static str, &'static str)> {
    TYPES
        .iter()
        .flat_map(|ty| SPELLINGS.iter().map(move |s| (*ty, *s)))
        .collect()
}

/// The constants a statement of the scope holds.
pub const CELLS: &[fn() -> Cell] = &[
    || Cell::Null,
    || Cell::Default,
    || Cell::Int(-1),
    || Cell::Text("it's".to_owned()),
    || Cell::Text("7".to_owned()),
];

/// The target column lists of table `a`.
pub const A_COLUMNS: &[Option<&[usize]>] = &[
    None,
    Some(&[0]),
    Some(&[2]),
    Some(&[1, 0]),
    Some(&[0, 1, 2]),
];

/// The target column lists of table `b`.
pub const B_COLUMNS: &[Option<&[usize]>] = &[None, Some(&[0])];

/// Every row of `len` cells.
fn rows_of(len: usize) -> Vec<Vec<Cell>> {
    let mut out = vec![Vec::new()];
    for _ in 0..len {
        out = out
            .into_iter()
            .flat_map(|row| {
                CELLS.iter().map(move |cell| {
                    let mut next = row.clone();
                    next.push(cell());
                    next
                })
            })
            .collect();
    }
    out
}

/// Every one-row statement of the scope: into `a`, one to three cells
/// under each column list; into `b`, one or two cells under each.
#[must_use]
pub fn statements() -> Vec<Insert> {
    let mut out = Vec::new();
    for (into_b, lists, max) in [(false, A_COLUMNS, 3), (true, B_COLUMNS, 2)] {
        for columns in lists {
            for len in 1..=max {
                for row in rows_of(len) {
                    out.push(Insert {
                        into_b,
                        columns: columns.map(<[usize]>::to_vec),
                        rows: vec![row],
                    });
                }
            }
        }
    }
    out
}

/// The closed-form count of [`statements`]: `a` has 5 column lists of
/// 5 + 25 + 125 rows, `b` 2 of 5 + 25.
pub const STATEMENTS_COUNT: usize = 5 * (5 + 25 + 125) + 2 * (5 + 25);

/// Every script of the scope: each statement alone, and each ordered pair
/// of one-cell statements (the numbering of interleaved tables).
#[must_use]
pub fn scripts() -> Vec<Vec<Insert>> {
    let all = statements();
    let short: Vec<&Insert> = all.iter().filter(|s| s.rows[0].len() == 1).collect();
    let mut out: Vec<Vec<Insert>> = all.iter().map(|s| vec![s.clone()]).collect();
    for first in &short {
        for second in &short {
            out.push(vec![(*first).clone(), (*second).clone()]);
        }
    }
    out
}

/// The closed-form count of [`scripts`]: the statements, and the pairs of
/// the 5 × 5 + 2 × 5 = 35 one-cell statements.
pub const SCRIPTS_COUNT: usize = STATEMENTS_COUNT + 35 * 35;

/// The characters of the bounded field scope: a letter, the characters
/// either dialect quotes or escapes, and the characters of `\.` and `\N`.
pub const FIELD_CHARS: &[char] = &['a', ',', '"', '\n', '\r', '\\', '\t', '.', ' ', 'N'];

/// Every field of the bounded scope: `NULL`, and every text of at most
/// three characters of [`FIELD_CHARS`].
#[must_use]
pub fn fields() -> Vec<Option<String>> {
    let mut texts = vec![String::new()];
    let mut layer = vec![String::new()];
    for _ in 0..3 {
        layer = layer
            .iter()
            .flat_map(|t| {
                FIELD_CHARS.iter().map(move |c| {
                    let mut next = t.clone();
                    next.push(*c);
                    next
                })
            })
            .collect();
        texts.extend(layer.iter().cloned());
    }
    std::iter::once(None)
        .chain(texts.into_iter().map(Some))
        .collect()
}

/// The closed-form count of [`fields`]: `NULL` and 1 + 10 + 100 + 1000
/// texts.
pub const FIELDS_COUNT: usize = 1 + 1 + 10 + 100 + 1000;
