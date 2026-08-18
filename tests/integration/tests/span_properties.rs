//! The exit criteria for making the span the primary search result.
//!
//! A span `src ←ℓ─ A ─r→ tgt` is only worth returning if it is a span: two
//! genuine schema morphisms out of a genuine schema. Everything else the search
//! reports about the answer, the quality, the coverage, the certificate, is
//! commentary on top of that, and none of it means anything if the underlying
//! claim is false. These three properties are that claim, over generated pairs
//! rather than over fixtures:
//!
//! 1. **Both legs are morphisms.** Each passes
//!    [`check_migration_morphism`], which is functoriality on the mapped
//!    fragment, and each passes [`check_existence`] against a theory registry
//!    that turns on every conditional obligation. The certificate's own claims
//!    are re-derived here rather than read, so a certificate that lies fails.
//! 2. **The apex is well formed.** It validates against the protocol, its
//!    adjacency indices agree with a fresh index of its own edges, and no field
//!    of it names a vertex or edge it does not hold. That last is what the hard
//!    constraints of the network exist to guarantee: inducing drops an entry
//!    whose members did not all survive, so a dangling reference would mean a
//!    constraint is missing rather than that inducing misbehaved.
//! 3. **The answer is deterministic.** Repeating one search a hundred times in
//!    one process returns a bit-identical apex digest, leg maps and quality.
//!    `HashMap` iteration order is randomised per process, so a search that
//!    read one anywhere in its answer path would drift across repeats.
//!
//! # What the existence check is allowed to report
//!
//! `SpanCertificate::left_existence` can be invalid, and that is not a
//! failure of the leg. Existence is wider than functoriality: several of its
//! conditional obligations read the schemas rather than the map, and
//! reachability is the one that fires. It asks whether every mapped vertex is
//! reachable from a vertex with no incoming edge, so an apex whose every vertex
//! sits on a cycle has no root and every vertex of it is reported at risk
//! however the leg maps them. What is asserted here is therefore that the
//! certificate *agrees with a fresh check*, on both legs, rather than that
//! either is unconditionally valid.

#![allow(clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use panproto_gat::{Name, Sort, Theory};
use panproto_integration::{arb_schema_rich, arb_small_schema_pair};
use panproto_mig::hom_search::SearchOptions;
use panproto_mig::{SchemaSpan, SpanSearch, check_existence, check_migration_morphism};
use panproto_schema::{Edge, Protocol, Schema, induce_on_vertices, validate};
use proptest::prelude::*;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

/// A theory registry naming every well-known sort, so that
/// [`check_existence`] runs every conditional obligation rather than the three
/// unconditional ones.
///
/// Without this the property would hold vacuously on most of its own surface:
/// each conditional check is gated on a conventionally named sort being present
/// in one of the protocol's theories, and an absent registry skips all of them.
fn full_registry(protocol: &Protocol) -> HashMap<String, Theory> {
    let sorts = || {
        vec![
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
            Sort::simple("Constraint"),
            Sort::simple("HyperEdge"),
            Sort::simple("Node"),
            Sort::simple("Variant"),
            Sort::simple("Position"),
            Sort::simple("Mu"),
            Sort::simple("Usage"),
        ]
    };
    let mut registry = HashMap::new();
    registry.insert(
        protocol.schema_theory.clone(),
        Theory::new(protocol.schema_theory.as_str(), sorts(), vec![], vec![]),
    );
    registry.insert(
        protocol.instance_theory.clone(),
        Theory::new(protocol.instance_theory.as_str(), sorts(), vec![], vec![]),
    );
    registry
}

/// The pair generator, a union of two regimes that no single generator covers.
///
/// # Why a union, with the measurements that forced it
///
/// Neither half is adequate alone, and both failure modes are silent: a
/// property over the wrong corpus passes because it had nothing to check.
///
/// * [`arb_small_schema_pair`] draws two independent edges-only schemas over a
///   shared name space. Over 400 draws it gives 268 apices that are proper
///   non-empty sub-schemas and 229 right legs that are not the identity, which
///   is what makes the leg checks mean anything. But it populates no field
///   beyond vertices and edges, so **0** of those apices carry a hyper edge, a
///   coproduct, a recursion point or a schema span, and every
///   dangling-reference check below is trivially satisfied.
/// * [`arb_schema_rich`] leaves no field empty, and pairing a rich schema with
///   a sub-schema of itself gives 108 apices in 400 that do carry non-edge
///   structure. But it gives **0** proper non-empty apices: the clique
///   constraints that keep a hyper-edge signature, a coproduct or a span whole
///   are all-or-nothing, and on a schema whose non-edge structure spans most of
///   its vertices, dropping any one vertex cascades to dropping them all. So
///   the apex is either the whole source or empty, and the right leg is the
///   identity on 393 of 400.
///
/// Pairing two *independent* rich draws is worse than either: 298 of 300 apices
/// come back empty, because inducing carries every arc between the chosen
/// vertices and one arc present on one side and absent on the other makes an
/// otherwise shared vertex unshareable.
///
/// `the_corpus_exercises_both_regimes` measures the union and fails if either
/// contribution disappears, so this reasoning is checked rather than recorded.
fn arb_span_pair() -> impl Strategy<Value = (Protocol, Schema, Schema)> {
    prop_oneof![arb_small_schema_pair(), arb_rich_subset_pair()]
}

/// A rich schema paired with a sub-schema of itself.
///
/// The target is a sub-schema rather than an independent draw because that is
/// the regime the search actually meets: two versions of one schema rather than
/// two unrelated graphs.
///
/// [`rich_protocol`](panproto_integration::rich_protocol) is a constant, so
/// source and target share it and the pair is searchable.
fn arb_rich_subset_pair() -> impl Strategy<Value = (Protocol, Schema, Schema)> {
    (
        arb_schema_rich(),
        prop::collection::vec(any::<bool>(), 1..=6),
    )
        .prop_filter_map(
            "the kept set induced an empty target",
            |((protocol, src), flags)| {
                let mut ids: Vec<Name> = src.vertices.keys().cloned().collect();
                ids.sort_unstable();
                let keep: FxHashSet<Name> = ids
                    .into_iter()
                    .enumerate()
                    .filter(|(index, _)| flags[index % flags.len()])
                    .map(|(_, id)| id)
                    .collect();
                if keep.is_empty() {
                    return None;
                }
                let tgt = induce_on_vertices(&src, &protocol, &keep).ok()?;
                Some((protocol, src, tgt))
            },
        )
}

/// Whether a schema carries any of the four axes whose entries a `⊤`-valued
/// clique constraint keeps whole.
fn has_non_edge_structure(schema: &Schema) -> bool {
    !schema.hyper_edges.is_empty()
        || !schema.variants.is_empty()
        || !schema.recursion_points.is_empty()
        || !schema.spans.is_empty()
}

/// The three option sets a span can be searched under, so that each property
/// covers the default, injective and iso paths rather than one of them.
fn options_for(case: usize) -> SearchOptions {
    match case % 3 {
        1 => SearchOptions {
            monic: true,
            ..SearchOptions::default()
        },
        2 => SearchOptions {
            iso: true,
            ..SearchOptions::default()
        },
        _ => SearchOptions::default(),
    }
}

/// Everything a span's identity consists of, as a comparable value.
///
/// The apex digest is a content hash of the whole apex, and the two leg maps
/// are sorted, so two spans agree here exactly when they are the same span.
/// The quality is compared as raw bits rather than as a float, because two
/// runs of one deterministic search must agree exactly and `f64` equality
/// would let a `NaN` quality compare unequal to itself and pass as a change.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    apex_digest: String,
    vertex_map: Vec<(Name, Name)>,
    edge_map: Vec<(Edge, Edge)>,
    quality_bits: u64,
}

fn fingerprint(span: &SchemaSpan) -> Fingerprint {
    let mut vertices: Vec<(Name, Name)> = span
        .right
        .vertex_map
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    vertices.sort_unstable();
    let mut edges: Vec<(Edge, Edge)> = span
        .right
        .edge_map
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    edges.sort_unstable();
    Fingerprint {
        apex_digest: span.apex_digest_hex(),
        vertex_map: vertices,
        edge_map: edges,
        quality_bits: span.quality.to_bits(),
    }
}

/// Every vertex identifier the apex names anywhere outside its vertex table.
///
/// A dangling entry in any of these is the symptom the network's `⊤`-valued
/// clique constraints exist to prevent, so they are collected together and
/// checked against the vertex table in one pass.
fn referenced_vertices(apex: &Schema) -> Vec<Name> {
    let mut names = Vec::new();
    for hyper in apex.hyper_edges.values() {
        names.extend(hyper.signature.values().cloned());
    }
    for (coproduct, arms) in &apex.variants {
        names.push(coproduct.clone());
        names.extend(arms.iter().map(|arm| arm.parent_vertex.clone()));
    }
    for (mu, point) in &apex.recursion_points {
        names.push(mu.clone());
        names.push(point.target_vertex.clone());
    }
    for span in apex.spans.values() {
        names.push(span.left.clone());
        names.push(span.right.clone());
    }
    names.extend(apex.entries.iter().cloned());
    names.extend(apex.required.keys().cloned());
    names.extend(apex.nsids.keys().cloned());
    names.extend(apex.constraints.keys().cloned());
    names
}

/// The apex's adjacency indices, rebuilt from its edge table alone.
fn fresh_index(apex: &Schema) -> (HashMap<Name, usize>, HashMap<Name, usize>) {
    let mut outgoing: HashMap<Name, usize> = HashMap::new();
    let mut incoming: HashMap<Name, usize> = HashMap::new();
    for edge in apex.edges.keys() {
        *outgoing.entry(edge.src.clone()).or_insert(0) += 1;
        *incoming.entry(edge.tgt.clone()).or_insert(0) += 1;
    }
    (outgoing, incoming)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Both legs are morphisms, and the certificate says so honestly.
    ///
    /// Functoriality is checked directly on each leg rather than read off
    /// `SpanCertificate::legs_are_functorial`, and then the flag is required
    /// to agree with what was measured. Existence is checked the same way on
    /// each leg separately, against a registry that turns on every conditional
    /// obligation.
    #[test]
    fn span_legs_are_morphisms((protocol, src, tgt) in arb_span_pair(), case in 0usize..3) {
        let theories = full_registry(&protocol);
        let span = SpanSearch::new(&protocol)
            .with_options(options_for(case))
            .with_theories(&theories)
            .run(&src, &tgt)
            .expect("the search never refuses for want of a match");

        // The left leg is an inclusion into the source, so it is a morphism by
        // construction and a failure here is a defect in the construction.
        prop_assert!(
            check_migration_morphism(&span.apex, &src, &span.left).is_ok(),
            "left leg is not a morphism: {:?}",
            check_migration_morphism(&span.apex, &src, &span.left)
        );
        prop_assert!(
            check_migration_morphism(&span.apex, &tgt, &span.right).is_ok(),
            "right leg is not a morphism: {:?}",
            check_migration_morphism(&span.apex, &tgt, &span.right)
        );
        prop_assert!(
            span.certificate.legs_are_functorial,
            "both legs are functorial but the certificate denies it"
        );

        // And the leg the certificate calls an inclusion really is one: the
        // identity on exactly the apex's own keys.
        prop_assert_eq!(span.left.vertex_map.len(), span.apex.vertices.len());
        prop_assert_eq!(span.left.edge_map.len(), span.apex.edges.len());
        for (source, image) in &span.left.vertex_map {
            prop_assert_eq!(source, image, "the left leg renames a vertex");
            prop_assert!(span.apex.vertices.contains_key(source));
        }
        for (source, image) in &span.left.edge_map {
            prop_assert_eq!(source, image, "the left leg renames an edge");
            prop_assert!(span.apex.edges.contains_key(source));
        }
        prop_assert!(span.certificate.shape.left_is_mono);

        // The right leg lands in the target, and is injective on vertices
        // exactly when the certificate claims it.
        for (source, image) in &span.right.vertex_map {
            prop_assert!(span.apex.vertices.contains_key(source));
            prop_assert!(tgt.vertices.contains_key(image), "right leg leaves the target");
        }
        let images: HashSet<&Name> = span.right.vertex_map.values().collect();
        prop_assert_eq!(
            images.len() == span.right.vertex_map.len(),
            span.certificate.shape.right_is_mono
        );

        // Existence, recomputed per leg. The claim is agreement with a fresh
        // check, not unconditional validity: see the module docs on why a left
        // leg finding is a statement about the apex.
        let left = check_existence(&protocol, &span.apex, &src, &span.left, &theories);
        let right = check_existence(&protocol, &span.apex, &tgt, &span.right, &theories);
        prop_assert_eq!(
            left.valid,
            span.certificate.left_existence.valid,
            "certificate disagrees with a fresh existence check on the left leg: {:?}",
            left.errors
        );
        prop_assert_eq!(
            right.valid,
            span.certificate.right_existence.valid,
            "certificate disagrees with a fresh existence check on the right leg: {:?}",
            right.errors
        );
    }

    /// The apex is a schema in its own right.
    #[test]
    fn apex_is_well_formed((protocol, src, tgt) in arb_span_pair(), case in 0usize..3) {
        let span = SpanSearch::new(&protocol)
            .with_options(options_for(case))
            .run(&src, &tgt)
            .expect("the search never refuses for want of a match");
        let apex = &span.apex;

        let findings = validate(apex, &protocol);
        prop_assert!(findings.is_empty(), "apex does not validate: {findings:?}");

        // Every vertex and edge of the apex came from the source, unrenamed.
        for (id, vertex) in &apex.vertices {
            let parent = src.vertices.get(id).expect("apex vertex is a source vertex");
            prop_assert_eq!(&vertex.kind, &parent.kind, "inducing changed a kind");
        }
        for edge in apex.edges.keys() {
            prop_assert!(src.edges.contains_key(edge), "apex edge is not a source edge");
            prop_assert!(apex.vertices.contains_key(&edge.src));
            prop_assert!(apex.vertices.contains_key(&edge.tgt));
        }

        // Nothing dangles. A surviving entry naming a vertex the apex dropped
        // would mean the constraint keeping that entry whole is missing from
        // the network, which is exactly what the induced apex cannot repair.
        for name in referenced_vertices(apex) {
            prop_assert!(
                apex.vertices.contains_key(&name),
                "apex names vertex {name} it does not hold"
            );
        }
        for edges in apex.required.values() {
            for edge in edges {
                prop_assert!(
                    apex.edges.contains_key(edge),
                    "a required-edge list names an edge the apex does not hold"
                );
            }
        }

        // The adjacency indices are a function of the edge table, so a
        // hand-assembled apex that forgot to rebuild them shows up here.
        let (outgoing, incoming) = fresh_index(apex);
        for (vertex, edges) in &apex.outgoing {
            prop_assert_eq!(
                edges.len(),
                outgoing.get(vertex).copied().unwrap_or(0),
                "outgoing index disagrees with the edge table at {}",
                vertex
            );
        }
        for (vertex, edges) in &apex.incoming {
            prop_assert_eq!(
                edges.len(),
                incoming.get(vertex).copied().unwrap_or(0),
                "incoming index disagrees with the edge table at {}",
                vertex
            );
        }
        for (vertex, count) in &outgoing {
            prop_assert_eq!(
                apex.outgoing.get(vertex).map_or(0, SmallVec::len),
                *count,
                "the outgoing index lost a bucket at {}",
                vertex
            );
        }
        for (vertex, count) in &incoming {
            prop_assert_eq!(
                apex.incoming.get(vertex).map_or(0, SmallVec::len),
                *count,
                "the incoming index lost a bucket at {}",
                vertex
            );
        }

        // The certificate's derived readings agree with the apex itself.
        prop_assert_eq!(span.certificate.apex_pointed, !apex.entries.is_empty());
        prop_assert_eq!(
            span.is_total(),
            apex.vertices.len() == src.vertices.len()
                && apex.edges.len()
                    == src
                        .edges
                        .keys()
                        .filter(|e| src.vertices.contains_key(&e.src)
                            && src.vertices.contains_key(&e.tgt))
                        .count()
        );
        prop_assert!((0.0..=1.0).contains(&span.apex_coverage));
        prop_assert!((0.0..=1.0).contains(&span.quality));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// One hundred repeats of one search agree bit for bit.
    ///
    /// The repeats run inside one process, so they share a hash seed; what this
    /// rules out is a search whose answer depends on iteration order at all,
    /// since a `HashMap` walked twice in one process can still be walked in an
    /// order that differs from a `Vec`'s. The seed itself is randomised per
    /// process, so the property is also being re-drawn against a different
    /// order on every run of the suite.
    #[test]
    fn determinism((protocol, src, tgt) in arb_span_pair(), case in 0usize..3) {
        let options = options_for(case);
        let search = SpanSearch::new(&protocol).with_options(options);

        let first = search
            .run(&src, &tgt)
            .expect("the search never refuses for want of a match");
        let expected = fingerprint(&first);

        for repeat in 1..100 {
            let again = search
                .run(&src, &tgt)
                .expect("the search never refuses for want of a match");
            prop_assert_eq!(
                fingerprint(&again),
                expected.clone(),
                "repeat {} disagreed with the first answer",
                repeat
            );
        }
    }
}

/// The corpus above reaches every regime the three properties need.
///
/// A property over the wrong corpus passes because it had nothing to check,
/// and nothing in a `proptest!` block can see across its own cases, so the
/// coverage is measured here instead. Each threshold is far below what
/// [`arb_span_pair`] currently produces over this many draws; they are a floor
/// against a generator change silently emptying a property, not a pin on the
/// exact numbers.
///
/// The four regimes, and what would go untested without each:
///
/// 1. A **proper non-empty apex**. Without it the apex is either the whole
///    source or nothing, and no field rule's drop branch runs.
/// 2. **Non-edge structure in the apex.** Without it every dangling-reference
///    check in `apex_is_well_formed` is trivially satisfied.
/// 3. A **right leg that is not the identity.** Without it functoriality of
///    the right leg is checked only on a map that cannot violate it.
/// 4. An **existence check that reports something.** Without it the agreement
///    assertions in `span_legs_are_morphisms` compare `true` against `true`.
#[test]
fn the_corpus_exercises_both_regimes() {
    use proptest::test_runner::{Config, TestRunner};
    use std::cell::Cell;

    let draws = 400;
    let mut runner = TestRunner::new(Config {
        cases: draws,
        ..Config::default()
    });

    let proper_apex = Cell::new(0u32);
    let structured_apex = Cell::new(0u32);
    let renaming_leg = Cell::new(0u32);
    let existence_finding = Cell::new(0u32);

    runner
        .run(&arb_span_pair(), |(protocol, src, tgt)| {
            let theories = full_registry(&protocol);
            let span = SpanSearch::new(&protocol)
                .with_theories(&theories)
                .run(&src, &tgt)
                .expect("the search never refuses for want of a match");

            let kept = span.apex.vertices.len();
            if kept > 0 && kept < src.vertices.len() {
                proper_apex.set(proper_apex.get() + 1);
            }
            if has_non_edge_structure(&span.apex) {
                structured_apex.set(structured_apex.get() + 1);
            }
            if span.right.vertex_map.iter().any(|(from, to)| from != to) {
                renaming_leg.set(renaming_leg.get() + 1);
            }
            if !span.certificate.left_existence.valid || !span.certificate.right_existence.valid {
                existence_finding.set(existence_finding.get() + 1);
            }
            Ok(())
        })
        .expect("the generator produces searchable pairs");

    assert!(
        proper_apex.get() >= 40,
        "only {} of {draws} draws gave an apex that is a proper non-empty \
         sub-schema, so the drop branch of every field rule is barely exercised",
        proper_apex.get()
    );
    assert!(
        structured_apex.get() >= 10,
        "only {} of {draws} draws gave an apex carrying a hyper edge, a \
         coproduct, a recursion point or a schema span, so the \
         dangling-reference checks are near vacuous",
        structured_apex.get()
    );
    assert!(
        renaming_leg.get() >= 40,
        "only {} of {draws} draws gave a right leg that renames anything, so \
         its functoriality is checked mostly on the identity",
        renaming_leg.get()
    );
    assert!(
        existence_finding.get() >= 5,
        "no existence check reported a finding over {draws} draws, so the \
         certificate-agreement assertions compare true against true"
    );
}
