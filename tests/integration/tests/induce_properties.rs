//! Properties of sub-schema induction and of the canonical encoding, over
//! schemas in which all 21 [`Schema`] fields are populated.
//!
//! `panproto_schema::induce` is the one supported way to cut a sub-schema, and
//! its whole job is to account for every field in its own key space. A fixture
//! can only demonstrate that on the shapes someone thought to write down, so
//! these draw from [`arb_schema_rich`], which populates every field on every
//! draw, and check the three claims the design makes about the result:
//!
//! 1. **The identity cut is the identity.** Selecting every vertex returns the
//!    parent back, field by field, up to the two normalisations induction is
//!    documented to perform (basepoint de-duplication and the by-kind
//!    `coercions` rule).
//! 2. **Nothing dangles.** Every vertex id and every edge named anywhere in
//!    the apex — inside a hyper-edge signature, a coproduct arm, a fixpoint
//!    marker, a schema span, a required-edge list, an adjacency bucket — is a
//!    vertex or an edge the apex still holds.
//! 3. **The encoding is a content identity.** Two apices that agree encode
//!    alike whatever the hash seed, and the digest separates schemas the VCS
//!    object id deliberately identifies.
//!
//! A cut that keeps every vertex and one that keeps roughly half are both
//! exercised, because the second is the only one that makes the *retention*
//! branch of every field rule run.

#![allow(clippy::expect_used)]

use std::collections::HashSet;

use panproto_gat::Name;
use panproto_integration::arb_schema_rich;
use panproto_schema::{
    Edge, Protocol, Schema, canonical_bytes, canonical_digest, induce_on_vertices, validate,
};
use proptest::prelude::*;
use rustc_hash::FxHashSet;

/// Every vertex id of `schema`, in ascending order.
fn all_vertices(schema: &Schema) -> Vec<Name> {
    let mut ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    ids.sort_unstable();
    ids
}

/// The lexicographically first half of `schema`'s vertices, which is a
/// deterministic function of the draw.
fn first_half(schema: &Schema) -> FxHashSet<Name> {
    let ids = all_vertices(schema);
    let take = ids.len().div_ceil(2);
    ids.into_iter().take(take).collect()
}

/// Assert that no field of `apex` names a vertex or an edge it does not hold.
fn assert_nothing_dangles(apex: &Schema) -> Result<(), TestCaseError> {
    let has_vertex = |id: &Name| apex.vertices.contains_key(id);
    let has_edge = |e: &Edge| apex.edges.contains_key(e);

    for edge in apex.edges.keys() {
        prop_assert!(
            has_vertex(&edge.src) && has_vertex(&edge.tgt),
            "edge {edge:?}"
        );
    }
    for (id, hyper_edge) in &apex.hyper_edges {
        for target in hyper_edge.signature.values() {
            prop_assert!(has_vertex(target), "hyper-edge {id} signature");
        }
    }
    for id in apex.constraints.keys().chain(apex.nsids.keys()) {
        prop_assert!(has_vertex(id));
    }
    for (id, edges) in &apex.required {
        prop_assert!(has_vertex(id));
        for edge in edges {
            prop_assert!(has_edge(edge), "required edge {edge:?}");
        }
        prop_assert!(
            !edges.is_empty(),
            "an emptied `required` key must be dropped"
        );
    }
    for id in &apex.entries {
        prop_assert!(has_vertex(id));
    }
    for (parent, arms) in &apex.variants {
        prop_assert!(has_vertex(parent));
        for arm in arms {
            prop_assert!(has_vertex(&arm.id), "variant arm {}", arm.id);
            prop_assert!(has_vertex(&arm.parent_vertex));
        }
    }
    for (mu, point) in &apex.recursion_points {
        prop_assert!(has_vertex(mu));
        prop_assert!(has_vertex(&point.target_vertex));
    }
    for span in apex.spans.values() {
        prop_assert!(has_vertex(&span.left) && has_vertex(&span.right));
    }
    for edge in apex.orderings.keys().chain(apex.usage_modes.keys()) {
        prop_assert!(has_edge(edge));
    }
    for id in apex
        .nominal
        .keys()
        .chain(apex.mergers.keys())
        .chain(apex.defaults.keys())
    {
        prop_assert!(has_vertex(id));
    }
    // `coercions` is keyed by a pair of kinds, so what must survive is that
    // some surviving vertex still carries each kind.
    let kinds: HashSet<&Name> = apex.vertices.values().map(|v| &v.kind).collect();
    for (source_kind, target_kind) in apex.coercions.keys() {
        prop_assert!(kinds.contains(source_kind) && kinds.contains(target_kind));
    }
    let vertex_index_edges = apex
        .outgoing
        .values()
        .chain(apex.incoming.values())
        .flat_map(|bucket| bucket.as_slice().iter());
    let pair_index_edges = apex
        .between
        .values()
        .flat_map(|bucket| bucket.as_slice().iter());
    for edge in vertex_index_edges.chain(pair_index_edges) {
        prop_assert!(has_edge(edge), "index entry {edge:?}");
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Selecting every vertex returns the parent. The two exceptions are
    /// documented normalisations rather than losses: `entries` is
    /// de-duplicated, and `coercions` is filtered by the kinds surviving
    /// vertices carry, which drops a key naming a kind no vertex holds even
    /// when nothing was cut.
    #[test]
    fn the_identity_cut_is_the_identity((protocol, schema) in arb_schema_rich()) {
        let keep: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
        let apex = induce_on_vertices(&schema, &protocol, &keep)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert_eq!(&apex.protocol, &schema.protocol);
        prop_assert_eq!(&apex.vertices, &schema.vertices);
        prop_assert_eq!(&apex.edges, &schema.edges);
        prop_assert_eq!(&apex.hyper_edges, &schema.hyper_edges);
        prop_assert_eq!(&apex.constraints, &schema.constraints);
        prop_assert_eq!(&apex.required, &schema.required);
        prop_assert_eq!(&apex.nsids, &schema.nsids);
        prop_assert_eq!(&apex.variants, &schema.variants);
        prop_assert_eq!(&apex.orderings, &schema.orderings);
        prop_assert_eq!(&apex.recursion_points, &schema.recursion_points);
        prop_assert_eq!(&apex.spans, &schema.spans);
        prop_assert_eq!(&apex.usage_modes, &schema.usage_modes);
        prop_assert_eq!(&apex.nominal, &schema.nominal);
        prop_assert_eq!(&apex.coercions, &schema.coercions);
        prop_assert_eq!(&apex.mergers, &schema.mergers);
        prop_assert_eq!(&apex.defaults, &schema.defaults);
        prop_assert_eq!(&apex.policies, &schema.policies);
        prop_assert_eq!(&apex.outgoing, &schema.outgoing);
        prop_assert_eq!(&apex.incoming, &schema.incoming);
        prop_assert_eq!(&apex.between, &schema.between);

        // `entries` is de-duplicated, so it is the parent's list with repeats
        // removed rather than the list itself.
        let mut seen = HashSet::new();
        let deduped: Vec<Name> = schema
            .entries
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect();
        prop_assert_eq!(&apex.entries, &deduped);

        // And the identity cut leaves the content identity alone.
        prop_assert_eq!(canonical_digest(&apex), canonical_digest(&schema));
    }

    /// Cutting to half the vertices leaves an apex in which nothing dangles,
    /// which validates, and which is a fixpoint of a second identical cut.
    #[test]
    fn a_partial_cut_leaves_nothing_dangling((protocol, schema) in arb_schema_rich()) {
        let keep = first_half(&schema);
        let apex = induce_on_vertices(&schema, &protocol, &keep)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert_eq!(apex.vertices.len(), keep.len());
        assert_nothing_dangles(&apex)?;
        prop_assert!(validate(&apex, &protocol).is_empty());

        let again = induce_on_vertices(&apex, &protocol, &keep)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(canonical_bytes(&again), canonical_bytes(&apex));
    }

    /// The arm-retention branch of the `variants` rule actually runs.
    ///
    /// Each arm's `id` is a vertex id, so a cut keeps exactly the arms whose
    /// injection vertex survived. A generator whose arms name no vertex at all
    /// would satisfy every other property in this file while leaving the whole
    /// branch untested, and the `variants` map would still read as non-empty
    /// because its key survives with an empty arm list.
    #[test]
    fn variant_arms_are_retained_by_their_own_vertex((protocol, schema) in arb_schema_rich()) {
        let total_arms: usize = schema.variants.values().map(Vec::len).sum();
        prop_assert!(total_arms > 0, "the generator must produce coproduct arms");

        let keep: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
        let whole = induce_on_vertices(&schema, &protocol, &keep)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let kept_arms: usize = whole.variants.values().map(Vec::len).sum();
        prop_assert_eq!(
            kept_arms,
            total_arms,
            "the identity cut must retain every coproduct arm"
        );

        let half = first_half(&schema);
        let cut = induce_on_vertices(&schema, &protocol, &half)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let expected: usize = schema
            .variants
            .iter()
            .filter(|(parent, _)| half.contains(*parent))
            .flat_map(|(_, arms)| arms)
            .filter(|arm| half.contains(&arm.id) && half.contains(&arm.parent_vertex))
            .count();
        let actual: usize = cut.variants.values().map(Vec::len).sum();
        prop_assert_eq!(actual, expected);
    }

    /// The encoding depends on content and on nothing else, and the digest
    /// tracks it. Rebuilding the same cut gives fresh hash maps with fresh
    /// seeds, so equal bytes here is a statement about seed independence.
    #[test]
    fn the_encoding_is_a_content_identity((protocol, schema) in arb_schema_rich()) {
        let keep = first_half(&schema);
        let once = induce_on_vertices(&schema, &protocol, &keep)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let twice = induce_on_vertices(&schema, &protocol, &keep)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;

        prop_assert_eq!(canonical_bytes(&once), canonical_bytes(&twice));
        prop_assert_eq!(canonical_digest(&once), canonical_digest(&twice));
    }
}

/// The two canonical forms in the tree disagree about the pointing, and the
/// disagreement is deliberate: a change of pointing is not a change of schema
/// content for the VCS, whereas for a span's apex the basepoints are what a
/// downstream instance layer roots its data at.
#[test]
fn the_digest_separates_pointings_the_vcs_object_id_identifies() {
    let protocol = Protocol::default();
    let build = || {
        panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("leaf", "string", None)
            .expect("leaf")
            .edge("root", "leaf", "prop", Some("leaf"))
            .expect("edge")
            .build()
            .expect("build")
    };

    let unpointed = build();
    let mut pointed = build();
    pointed.entries.push(Name::from("root"));

    assert_ne!(
        canonical_digest(&unpointed),
        canonical_digest(&pointed),
        "the pointing is part of a schema's content identity"
    );
    assert_eq!(
        panproto_vcs::hash::hash_schema(&unpointed).expect("hash"),
        panproto_vcs::hash::hash_schema(&pointed).expect("hash"),
        "the VCS object id deliberately excludes the pointing"
    );
}
