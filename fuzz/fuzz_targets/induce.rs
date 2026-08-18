#![no_main]
//! Fuzz target for the induced sub-schema construction.
//!
//! `induce_on_vertices` is what turns a solved assignment into an apex, so a
//! defect here is a defect in every span. The invariants:
//!
//! 1. The result validates against the protocol.
//! 2. The adjacency indices agree with the edge set, in both directions and
//!    for the `between` index too.
//! 3. No surviving structure mentions a dropped vertex: not an edge endpoint,
//!    not a required-edge declaration, not a constraint, not an entry point.
//! 4. Induction is idempotent. Inducing on the surviving vertex set of an
//!    already-induced schema returns the same schema, by canonical digest.
//!
//! Run with:
//!
//! ```text
//! cargo fuzz run induce -- -max_total_time=300
//! ```

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use panproto_gat::Name;
use panproto_schema::{Protocol, Schema, canonical_digest, induce_on_vertices, validate};
use rustc_hash::FxHashSet;

mod model;

/// Every structural invariant an induced schema owes.
fn check_induced(apex: &Schema, protocol: &Protocol, keep: &FxHashSet<Name>) {
    let errors = validate(apex, protocol);
    assert!(errors.is_empty(), "induced schema invalid: {errors:?}");

    // Exactly the kept vertices survive.
    for id in apex.vertices.keys() {
        assert!(keep.contains(id), "vertex {id} survived without being kept");
    }

    // No edge dangles, and every edge is between kept vertices.
    for edge in apex.edges.keys() {
        assert!(
            apex.vertices.contains_key(&edge.src),
            "edge source {} was dropped but its edge survived",
            edge.src
        );
        assert!(
            apex.vertices.contains_key(&edge.tgt),
            "edge target {} was dropped but its edge survived",
            edge.tgt
        );
    }

    // The outgoing index lists exactly the edges leaving each vertex.
    for (vertex, edges) in &apex.outgoing {
        assert!(
            apex.vertices.contains_key(vertex),
            "outgoing index keyed on dropped vertex {vertex}"
        );
        for edge in edges {
            assert_eq!(&edge.src, vertex, "outgoing index holds a foreign edge");
            assert!(
                apex.edges.contains_key(edge),
                "outgoing index holds an edge the schema dropped"
            );
        }
    }
    for (vertex, edges) in &apex.incoming {
        assert!(
            apex.vertices.contains_key(vertex),
            "incoming index keyed on dropped vertex {vertex}"
        );
        for edge in edges {
            assert_eq!(&edge.tgt, vertex, "incoming index holds a foreign edge");
            assert!(
                apex.edges.contains_key(edge),
                "incoming index holds an edge the schema dropped"
            );
        }
    }
    for ((src, tgt), edges) in &apex.between {
        assert!(
            apex.vertices.contains_key(src) && apex.vertices.contains_key(tgt),
            "between index keyed on a dropped vertex"
        );
        for edge in edges {
            assert_eq!((&edge.src, &edge.tgt), (src, tgt));
            assert!(
                apex.edges.contains_key(edge),
                "between index holds an edge the schema dropped"
            );
        }
    }

    // Every edge is reachable through both indices, so neither is a subset of
    // the truth: an index that merely holds no junk is not an index.
    for edge in apex.edges.keys() {
        assert!(
            apex.outgoing
                .get(&edge.src)
                .is_some_and(|list| list.contains(edge)),
            "edge {edge:?} is missing from the outgoing index"
        );
        assert!(
            apex.incoming
                .get(&edge.tgt)
                .is_some_and(|list| list.contains(edge)),
            "edge {edge:?} is missing from the incoming index"
        );
    }

    // Nothing else references a dropped key.
    for (vertex, edges) in &apex.required {
        assert!(
            apex.vertices.contains_key(vertex),
            "required-edge list keyed on dropped vertex {vertex}"
        );
        for edge in edges {
            assert!(
                apex.edges.contains_key(edge),
                "vertex {vertex} still requires edge {edge:?}, which induction dropped"
            );
        }
    }
    for vertex in apex.constraints.keys() {
        assert!(
            apex.vertices.contains_key(vertex),
            "constraints keyed on dropped vertex {vertex}"
        );
    }
    for entry in &apex.entries {
        assert!(
            apex.vertices.contains_key(entry),
            "entry point {entry} was dropped but is still declared"
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Ok(shape) = u.arbitrary::<model::Shape>() else {
        return;
    };
    let Some(generated) = model::build(&shape, "s.", 12) else {
        return;
    };
    let protocol = model::protocol();

    // An arbitrary vertex subset, drawn one bit per vertex.
    let mut keep: FxHashSet<Name> = FxHashSet::default();
    for name in &generated.vertex_names {
        if u.arbitrary::<bool>().unwrap_or(false) {
            keep.insert(name.clone());
        }
    }

    let Ok(apex) = induce_on_vertices(&generated.schema, &protocol, &keep) else {
        // A refusal is a legitimate answer; what is not legitimate is a
        // malformed acceptance.
        return;
    };
    check_induced(&apex, &protocol, &keep);

    // 4. Idempotence.
    let survivors: FxHashSet<Name> = apex.vertices.keys().cloned().collect();
    let twice = induce_on_vertices(&apex, &protocol, &survivors)
        .expect("inducing on everything that survived cannot fail");
    assert_eq!(
        canonical_digest(&apex),
        canonical_digest(&twice),
        "induction is not idempotent"
    );
});
