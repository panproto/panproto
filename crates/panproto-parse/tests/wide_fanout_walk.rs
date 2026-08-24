//! Walking and replaying a node with a large fan-out costs time proportional
//! to the number of children, not to its square.
//!
//! Two scans used to sit inside the per-child loops. The walk resolved a
//! child's tree-sitter field name by asking the parent for its children one
//! at a time until it found the one it already held, and replay resolved each
//! interstitial run's recorded span by scanning the whole constraint list of
//! the vertex that carries it. Both are linear in the fan-out and both run
//! once per child, so a wide node — a long array literal, a spreadsheet row
//! with thousands of columns — costs quadratic time.
//!
//! A wide node is ordinary data, so the bound below is what keeps this path
//! usable at all.

#![cfg(all(feature = "grammars", feature = "lang-rust"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use panproto_parse::ParserRegistry;

/// Rust source holding a single array literal of `elements` integers.
fn wide_array_source(elements: usize) -> String {
    let mut src = String::from("fn main() {\n    let xs = [");
    for i in 0..elements {
        if i > 0 {
            src.push_str(", ");
        }
        let _ = write!(src, "{i}");
    }
    src.push_str("];\n}\n");
    src
}

#[test]
fn a_wide_node_walks_and_replays_in_linear_time() {
    const ELEMENTS: usize = 20_000;

    let reg = ParserRegistry::new();
    let src = wide_array_source(ELEMENTS);

    let started = Instant::now();
    let parsed = reg
        .parse_with_protocol("rust", src.as_bytes(), "wide.rs")
        .expect("parse");
    let emitted = reg.emit_with_protocol("rust", &parsed).expect("emit");
    let elapsed = started.elapsed();

    assert_eq!(
        emitted,
        src.as_bytes(),
        "an untouched wide node did not replay byte-identically"
    );

    // Loose enough for a loaded machine, and far below what either quadratic
    // term costs at this width.
    assert!(
        elapsed < Duration::from_secs(60),
        "a {ELEMENTS}-element array took {elapsed:?} to parse and replay, which is quadratic territory"
    );
}
