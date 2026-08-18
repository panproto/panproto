//! The shape of every span over the record-typed lexicons, as a snapshot.
//!
//! `lexicon_sweep.rs` asserts that the search answers every pair optimally and
//! quickly. It says nothing about *what* it answers, and a change to the
//! objective that made every pair align on nothing would pass it. This file is
//! the other half: one row per pair recording the shape of the answer, so a
//! change in what the search considers optimal shows up as a diff at corpus
//! scale rather than as a surprise in one hand-written case.
//!
//! `lexicon_domain_shapes.rs` is the half before this one: the shape of the
//! network the search was handed, over every ordered pair of all seventy-seven
//! lexicons rather than over this file's unordered subset. Domain sizes, how
//! often no total morphism exists, and how often the constraints forbid nothing
//! are recorded there.
//!
//! Forty-two of the seventy-seven lexicons declare a record as their `main`
//! definition, which gives 861 unordered pairs. Records are taken because a
//! record carries a full property graph, where a query or a procedure carries
//! only its parameters and its output. The direction searched is from the
//! lexicographically smaller NSID, since the apex is induced on the source and
//! the two directions are different problems.
//!
//! # Reading a row
//!
//! ```text
//! app.bsky.feed.like -> app.bsky.feed.post  apex=2v/1e  q=0.412  optimal=true  path=eliminate  w=1
//! ```
//!
//! `apex` is how much of the source survived, `q` is
//! [`SchemaSpan::quality`](panproto_mig::SchemaSpan::quality) in thousandths so
//! that the snapshot does not carry float formatting, `optimal` is the proof
//! obligation, `path` is which algorithm answered, and `w` is the induced width
//! of the elimination order that was used. The last two together are the
//! dispatcher's decision: `eliminate` means the width fitted the budget and the
//! answer is exact by construction.
//!
//! **`cargo insta accept` is not appropriate here.** A row moves only when the
//! objective changed, and which change moved it is a fact worth writing down.
//! Review the diff and attribute each moved row.

#![allow(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]

use panproto_mig::{SolverPath, SpanSearch};

#[path = "support/lexicons.rs"]
mod lexicons;

/// The algorithm that answered, and the width it measured.
///
/// `Monic` and `Iso` carry no width: injectivity completes the primal graph, so
/// there is no elimination order to measure. Neither can be reached from the
/// default options this file searches with, and a row reporting one would be a
/// finding rather than a formatting problem.
fn path_and_width(path: SolverPath) -> (&'static str, String) {
    match path {
        SolverPath::Eliminate { width } => ("eliminate", width.to_string()),
        SolverPath::BranchAndBound { width } => ("branch-and-bound", width.to_string()),
        SolverPath::Monic => ("monic", "-".to_owned()),
        SolverPath::Iso => ("iso", "-".to_owned()),
    }
}

#[test]
fn record_lexicon_span_shapes() {
    let protocol = panproto_protocols::atproto::protocol();
    let records = lexicons::record_typed();
    let search = SpanSearch::new(&protocol);

    let mut rows: Vec<String> = Vec::with_capacity(861);
    let mut path_counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();

    for (i, src) in records.iter().enumerate() {
        for tgt in records.iter().skip(i + 1) {
            let span = search.run(&src.schema, &tgt.schema).unwrap_or_else(|e| {
                panic!("{} -> {}: the span search refused: {e}", src.nsid, tgt.nsid)
            });

            // `quality` lies in `[0, 1]`, so the product lies in `[0, 1000]`
            // and rounding it is exact in `u32`.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "quality is documented to lie in [0, 1], so the rounded product is in \
                          [0, 1000]"
            )]
            let quality_millis = (span.quality * 1000.0).round() as u32;

            let (path, width) = path_and_width(span.certificate.path);
            *path_counts.entry(path).or_default() += 1;

            rows.push(format!(
                "{} -> {}  apex={}v/{}e  q={quality_millis}  optimal={}  path={path}  w={width}",
                src.nsid,
                tgt.nsid,
                span.apex.vertices.len(),
                span.apex.edge_count(),
                span.certificate.proven_optimal,
            ));
        }
    }

    assert_eq!(
        rows.len(),
        861,
        "42 record-typed lexicons give 861 unordered pairs; the corpus moved"
    );
    rows.sort();

    // The distribution is the finding the snapshot is read for: branch and
    // bound firing at all on real lexicons would mean the width measurement is
    // wrong about what these schemas cost.
    eprintln!("solver paths over 861 record pairs: {path_counts:?}");

    insta::assert_yaml_snapshot!("lexicon_span_shapes", rows);
}
