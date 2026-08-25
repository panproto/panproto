//! A tabular cell is addressed by its position, and the key that encodes
//! that position has to be injective.
//!
//! Packing `(row, column)` into a `u32` as `row * 10_000 + column` is not:
//! row 1 column 0 and row 0 column ten thousand land on the same key, so an
//! edit to one silently rewrote the other, and the arithmetic overflows at
//! around four hundred thousand rows. Both are reachable in real tabular
//! data — a wide feature matrix, a long log export — and neither announces
//! itself.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::Value;
use panproto_io::cst_extract::tabular_cell_key;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn tabular_schema() -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("rows", "object", None)
        .expect("rows vertex")
        .build()
        .expect("build schema")
}

#[test]
fn the_cell_key_is_injective_where_the_packed_one_collided() {
    // The exact collision: row 1 column 0 against row 0 column 10_000.
    assert_ne!(tabular_cell_key(1, 0), tabular_cell_key(0, 10_000));
    // And the overflow: two rows far past what a u32 product can hold.
    assert_ne!(tabular_cell_key(429_497, 0), tabular_cell_key(1, 6_496));
    // Injective over a grid that spans both boundaries.
    let mut seen = std::collections::HashSet::new();
    for row in [0_u32, 1, 2, 429_496, 429_497, u32::MAX] {
        for column in [0_u32, 1, 9_999, 10_000, 10_001, u32::MAX] {
            assert!(
                seen.insert(tabular_cell_key(row, column)),
                "({row}, {column}) collided with an earlier cell"
            );
        }
    }
}

/// A row wider than the old key's ten-thousand-column stride: an edit to row
/// 0, column 10000 must land there, and must not land on row 1, column 0.
///
/// Those two cells packed to the same key under `row * 10_000 + column`, so
/// the edit was written into the other cell's CST vertex: the cell that was
/// edited kept its old text, and the cell that was not edited took the new
/// one.
#[test]
fn a_row_wider_than_the_old_stride_edits_the_right_cell() {
    use std::fmt::Write as _;

    let columns = 10_050;
    let mut input = String::new();
    for c in 0..columns {
        if c > 0 {
            input.push(',');
        }
        let _ = write!(input, "c{c}");
    }
    input.push('\n');
    for row in 0..2 {
        for c in 0..columns {
            if c > 0 {
                input.push(',');
            }
            let _ = write!(input, "r{row}v{c}");
        }
        input.push('\n');
    }

    let codec = UnifiedCodec::csv("test").expect("csv codec");
    let schema = tabular_schema();
    let started = std::time::Instant::now();
    let (instance, complement) = codec
        .parse_functor_preserving(&schema, input.as_bytes())
        .expect("parse");

    let mut mutated = instance;
    let rows = mutated.tables.get_mut("rows").expect("rows table");
    rows[0].insert("c10000".to_owned(), Value::Str("EDITED".to_owned()));

    let emitted = codec
        .emit_functor_preserving(&schema, &mutated, &complement)
        .expect("emit");
    let (reparsed, _) = codec
        .parse_functor_preserving(&schema, &emitted)
        .expect("reparse");

    let out = &reparsed.tables["rows"];
    assert_eq!(
        out[0]["c10000"],
        Value::Str("EDITED".to_owned()),
        "the edit to row 0 column 10000 did not land"
    );
    assert_eq!(
        out[1]["c0"],
        Value::Str("r1v0".to_owned()),
        "editing row 0 column 10000 overwrote row 1 column 0"
    );
    assert_eq!(out[0]["c0"], Value::Str("r0v0".to_owned()));

    // The round trip above must cost time proportional to the row, not to its
    // square. Resolving each child's field name by rescanning its siblings,
    // and each interstitial's span by rescanning its vertex's constraints,
    // both made this quadratic in the fan-out: the same three rows did not
    // finish inside ten minutes. The bound is loose enough for a loaded
    // machine and far below what either quadratic term would cost.
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(120),
        "a {columns}-column round trip took {elapsed:?}, which is quadratic territory"
    );
}
