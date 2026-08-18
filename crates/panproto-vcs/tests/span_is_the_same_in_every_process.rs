//! The span a pair yields must not depend on the hash seed the process started
//! with.
//!
//! The apex digest is a canonical hash and sorts everything it reads, so it is
//! stable by construction and pinning it proves nothing about the rest of the
//! span. The right leg's **edge map** is not canonicalised: `edge_image`
//! picks the first target edge of the source edge's kind out of
//! `Schema::edges_between`, and that slice's order is whatever the schema's
//! `between` index recorded. Whether the answer is a function of the two
//! schemas therefore depends on every construction that builds that index.
//!
//! `three_way_merge` rebuilds it by iterating `edges`, which is a
//! `HashMap`, so its bucket order is the process's
//! hash seed. This runs the same merge and the same span search in many
//! separate processes and compares the whole span, edge map included.
//!
//! Run the child directly with `PP_SPAN_DUMP=1` to see one process's answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::BTreeMap;
use std::process::Command;

use panproto_mig::hom_search::{SearchOptions, find_span};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};
use panproto_vcs::merge::three_way_merge;

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 64;

fn protocol() -> Protocol {
    Protocol {
        name: "determinism".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["string".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned(), "string".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// One object and one string, joined by the named property edges.
fn schema_with(names: &[&str]) -> Schema {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto)
        .vertex("rec", "object", None::<&str>)
        .expect("rec")
        .vertex("rec.value", "string", None::<&str>)
        .expect("value");
    for name in names {
        builder = builder
            .edge("rec", "rec.value", "prop", Some(*name))
            .expect("edge");
    }
    builder.entry("rec").build().expect("build")
}

/// The target: a merge of two sides that each added two parallel `prop` edges.
///
/// Four parallel edges of one kind between one vertex pair is the shape that
/// makes `edge_image`'s fallback a choice rather than a lookup, and the merge
/// is what files them in hash order.
fn merged_target() -> Schema {
    let base = schema_with(&[]);
    let ours = schema_with(&["alpha", "beta"]);
    let theirs = schema_with(&["gamma", "delta"]);
    let merged = three_way_merge(&base, &ours, &theirs);
    assert!(
        merged.conflicts.is_empty(),
        "the two sides add disjoint edges, so nothing conflicts: {:?}",
        merged.conflicts
    );
    assert_eq!(
        merged.merged_schema.edges.len(),
        4,
        "the merge keeps all four parallel edges"
    );
    merged.merged_schema
}

/// The source names its edge something the target does not hold, so
/// `edge_image`'s name-matching stage cannot decide and the kind-only
/// fallback picks whichever parallel edge comes first.
fn source() -> Schema {
    schema_with(&["omega"])
}

/// The whole span, written out so that two runs can be compared literally.
fn dump(span: &panproto_mig::SchemaSpan, target: &Schema) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let bucket: Vec<String> = target
        .edges_between("rec", "rec.value")
        .iter()
        .map(|edge| {
            edge.name
                .as_ref()
                .map_or_else(|| "-".to_owned(), ToString::to_string)
        })
        .collect();
    let _ = writeln!(out, "target edges_between(rec, rec.value) = {bucket:?}");

    let _ = writeln!(out, "quality = {:.12}", span.quality);
    let _ = writeln!(out, "coverage = {:.12}", span.apex_coverage);
    let _ = writeln!(
        out,
        "apex digest = {}",
        span.certificate
            .apex_digest
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    );

    let vertices: BTreeMap<_, _> = span
        .right
        .vertex_map
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let _ = writeln!(out, "right vertex map = {vertices:?}");

    let edges: BTreeMap<_, _> = span
        .right
        .edge_map
        .iter()
        .map(|(k, v)| {
            (
                format!("{}->{}({}:{:?})", k.src, k.tgt, k.kind, k.name),
                format!("{}->{}({}:{:?})", v.src, v.tgt, v.kind, v.name),
            )
        })
        .collect();
    let _ = writeln!(out, "right edge map = {edges:?}");
    out
}

/// The child half: one process's answer on the fixture, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_SPAN_DUMP").is_none() {
        // Nothing to do: the parent below is what runs this with the variable
        // set. Left as a normal test so it type-checks in every run.
        return;
    }
    let target = merged_target();
    let span = find_span(&source(), &target, &protocol(), &SearchOptions::default())
        .expect("the fixture poses");
    print!("<<<{}>>>", dump(&span, &target));
}

/// Extract what the child printed between the markers.
fn child_answer(exe: &std::path::Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_SPAN_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

/// The same pair, searched in sixty-four separate processes, must give one
/// answer.
#[test]
fn the_span_does_not_depend_on_the_hash_seed() {
    if std::env::var_os("PP_SPAN_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let first = child_answer(&exe);
    let mut distinct: BTreeMap<String, usize> = BTreeMap::new();
    distinct.insert(first, 1);

    for _ in 1..PROCESSES {
        *distinct.entry(child_answer(&exe)).or_insert(0) += 1;
    }

    assert_eq!(
        distinct.len(),
        1,
        "{PROCESSES} processes gave {} different spans for one schema pair; the answer is not a \
         function of the two schemas.\n{}",
        distinct.len(),
        distinct
            .iter()
            .map(|(answer, count)| format!("--- seen {count} times ---\n{answer}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
