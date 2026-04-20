//! Lex + parse benchmarks for real migration field-transform expressions.

#![allow(clippy::expect_used)]

use panproto_expr_parser::{parse, tokenize};

fn main() {
    divan::main();
}

/// Field-transform expressions of the shape that appear in real panproto
/// migration specs for AT Protocol Lexicon evolutions.
const REAL_EXPRS: &[&str] = &[
    // Rename: take the old field, emit it under the new name.
    "\\record -> record.displayName",
    // Coerce int → string on a constrained numeric field.
    "\\value -> show value",
    // Concatenate two text fields (e.g., post text + alt text fallback).
    "\\record -> record.text ++ \" (alt: \" ++ record.alt ++ \")\"",
    // Filter a list of posts by language.
    "\\posts -> filter (\\p -> p.langs `contains` \"en\") posts",
    // Map: project display string from each profile.
    "\\profiles -> map (\\p -> p.displayName) profiles",
];

#[divan::bench(args = [0_usize, 1, 2, 3, 4])]
fn tokenize_real_expr(bencher: divan::Bencher, i: usize) {
    let src = REAL_EXPRS[i];
    bencher.bench(|| tokenize(src));
}

#[divan::bench(args = [0_usize, 1, 2, 3, 4])]
fn lex_and_parse_real_expr(bencher: divan::Bencher, i: usize) {
    let src = REAL_EXPRS[i];
    bencher.bench(|| {
        let tokens = tokenize(src).expect("lex");
        parse(&tokens)
    });
}
