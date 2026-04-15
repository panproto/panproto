//! Property-based tests for the pointed-schema invariants introduced
//! by panproto#35.
//!
//! The pointed schema is a schema equipped with a finite family
//! `E → Ob(C_S)` of basepoints (the sorts at which an instance may be
//! rooted). These tests exercise the algebraic laws of that structure:
//!
//! * **Well-pointedness**: every declared entry names an actual vertex.
//! * **Idempotence of `.entry()`**: declaring the same entry twice
//!   leaves the family unchanged.
//! * **`primary_entry` preference**: when entries are non-empty, the
//!   helper returns the first declared basepoint (never the
//!   heuristic fallback).
//! * **`primary_entry` purity**: determinism over repeated calls.
//! * **Normalization composes with the basepoint map**: when no
//!   entry intersects with a consumed ref chain, normalization is the
//!   identity on entries.
//! * **Three-way merge** on entries implements the pushout of pointed
//!   schemas: unilateral deletions propagate, unilateral additions
//!   land, and every surviving entry names a surviving vertex.
//!
//! Uses `proptest` to sample over randomly generated schemas and
//! entry configurations so each law is exercised across a broad
//! cross-section of inputs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{
    EdgeRule, Protocol, Schema, SchemaBuilder, SchemaError, Vertex, normalize, primary_entry,
};
use panproto_vcs::merge::three_way_merge_entries;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// A permissive protocol that accepts any vertex kind and any edge
/// kind — lets the generator explore arbitrary schemas without the
/// builder rejecting them for protocol-mismatch reasons unrelated to
/// the properties we want to test.
fn open_protocol() -> Protocol {
    Protocol {
        name: "test-open".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        }],
        obj_kinds: vec!["object".into(), "string".into(), "ref".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// A vertex kind.
fn arb_kind() -> impl Strategy<Value = &'static str> {
    prop_oneof!(Just("object"), Just("string"), Just("ref"))
}

/// A built schema with between 1 and 6 vertices, an arbitrary subset
/// of edges between them, and an arbitrary (possibly empty) subset of
/// vertex ids flagged as entries. Returns `None` if the sampled shape
/// is unbuildable (e.g. produces a duplicate edge with the same name),
/// which the caller filters out.
fn arb_schema() -> impl Strategy<Value = Schema> {
    // 1..=6 vertices.
    (1usize..=6)
        .prop_flat_map(|n| {
            // Per-vertex kind.
            let kinds = prop::collection::vec(arb_kind(), n);
            // Edges: bounded subset of (src_idx, tgt_idx, name?).
            let edges = prop::collection::vec((0..n, 0..n, prop::option::of(0u32..5)), 0..=n * 2);
            // Entries: subset of vertex indices.
            let entry_idxs = prop::collection::vec(0..n, 0..=n);

            (Just(n), kinds, edges, entry_idxs)
        })
        .prop_filter_map(
            "unbuildable schema shape",
            |(n, kinds, edges, entry_idxs)| {
                let proto = open_protocol();
                let mut b = SchemaBuilder::new(&proto);
                for (i, k) in kinds.iter().enumerate() {
                    b = b.vertex(&format!("v{i}"), k, None).ok()?;
                }
                let mut seen_edges = HashSet::new();
                for (s, t, name_idx) in edges {
                    let src = format!("v{s}");
                    let tgt = format!("v{t}");
                    let name = name_idx.map(|n| format!("e{n}"));
                    // Avoid the builder's DuplicateEdge rejection by
                    // tracking (src, tgt, name) locally.
                    if !seen_edges.insert((src.clone(), tgt.clone(), name.clone())) {
                        continue;
                    }
                    b = b.edge(&src, &tgt, "prop", name.as_deref()).ok()?;
                }
                let mut seen_entries = HashSet::new();
                for i in entry_idxs {
                    let v = format!("v{i}");
                    if seen_entries.insert(v.clone()) {
                        b = b.entry(&v);
                    }
                }
                let _ = n;
                b.build().ok()
            },
        )
}

// ---------------------------------------------------------------------------
// Laws
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Post-build, every declared entry names a vertex in the schema.
    /// This is the well-pointedness axiom: the basepoint family is a
    /// map `E → Ob(C_S)`, not a partial map.
    #[test]
    fn well_pointed_after_build(schema in arb_schema()) {
        for e in schema.entry_vertices() {
            prop_assert!(
                schema.has_vertex(e.as_ref()),
                "entry {e:?} must be a vertex"
            );
        }
    }

    /// Post-build entries contain no duplicates — the family is a set
    /// (the builder deduplicates on insert).
    #[test]
    fn entries_are_unique_after_build(schema in arb_schema()) {
        let mut seen: HashSet<&Name> = HashSet::new();
        for e in schema.entry_vertices() {
            prop_assert!(seen.insert(e), "duplicate entry {e:?}");
        }
    }

    /// `.entry(x)` is idempotent: declaring the same basepoint twice
    /// leaves the entries sequence unchanged.
    #[test]
    fn entry_idempotent(
        vertex_kinds in prop::collection::vec(arb_kind(), 1..=4),
        entry_idx in any::<prop::sample::Index>()
    ) {
        let proto = open_protocol();
        let n = vertex_kinds.len();
        let idx = entry_idx.index(n);
        let vid = format!("v{idx}");

        let mut once = SchemaBuilder::new(&proto);
        let mut twice = SchemaBuilder::new(&proto);
        for (i, k) in vertex_kinds.iter().enumerate() {
            once = once.vertex(&format!("v{i}"), k, None).unwrap();
            twice = twice.vertex(&format!("v{i}"), k, None).unwrap();
        }
        once = once.entry(&vid);
        twice = twice.entry(&vid).entry(&vid).entry(&vid);

        let s_once = once.build().unwrap();
        let s_twice = twice.build().unwrap();
        prop_assert_eq!(s_once.entry_vertices(), s_twice.entry_vertices());
    }

    /// `primary_entry` on a pointed schema returns the first declared
    /// basepoint; the fallback heuristic never fires when entries are
    /// non-empty.
    #[test]
    fn primary_entry_prefers_declared(schema in arb_schema()) {
        if let Some(first) = schema.entry_vertices().first() {
            let chosen = primary_entry(&schema);
            prop_assert_eq!(chosen, Some(first));
        }
    }

    /// `primary_entry` is a pure function: repeated calls on the same
    /// schema yield the same result.
    #[test]
    fn primary_entry_is_deterministic(schema in arb_schema()) {
        let a = primary_entry(&schema).cloned();
        let b = primary_entry(&schema).cloned();
        prop_assert_eq!(a, b);
    }

    /// `primary_entry`'s return, if `Some`, names a vertex of the
    /// schema. The helper never hands back a dangling name.
    #[test]
    fn primary_entry_points_into_schema(schema in arb_schema()) {
        if let Some(e) = primary_entry(&schema) {
            prop_assert!(schema.has_vertex(e.as_ref()));
        }
    }

    /// Declaring an entry with an unknown vertex name must cause
    /// `build()` to fail with `UnknownEntryVertex`. The well-
    /// pointedness check is total — it rejects every ill-pointed
    /// construction, not just some.
    #[test]
    fn ill_pointed_schema_is_rejected(
        vertex_kinds in prop::collection::vec(arb_kind(), 1..=4),
        bogus_name in "not_v[0-9]+"
    ) {
        let proto = open_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (i, k) in vertex_kinds.iter().enumerate() {
            b = b.vertex(&format!("v{i}"), k, None).unwrap();
        }
        let result = b.entry(&bogus_name).build();
        prop_assert!(matches!(result, Err(SchemaError::UnknownEntryVertex(_))));
    }

    /// Normalization acts as the identity on the entries of a schema
    /// whose vertices contain no refs — nothing is collapsed, so the
    /// basepoint map is unchanged.
    #[test]
    fn normalize_preserves_entries_when_no_refs(schema in arb_schema()) {
        // Build a copy of the schema restricted to non-ref vertices
        // by rejecting any schema that contains a ref vertex. This
        // keeps the property statement honest without conditioning on
        // post-normalization behaviour.
        let has_ref = schema.vertices.values().any(|v| v.kind.as_ref() == "ref");
        prop_assume!(!has_ref);

        let normalized = normalize(&schema);
        prop_assert_eq!(
            normalized.entry_vertices(),
            schema.entry_vertices(),
        );
    }
}

// ---------------------------------------------------------------------------
// Three-way merge laws (hand-built schemas; exercise the entries-only
// pushout logic without dragging in the full VCS merge surface).
// ---------------------------------------------------------------------------

/// Build an empty schema whose `entries` field holds the given names.
/// Used to exercise `three_way_merge_entries` directly on specific
/// entry configurations without constructing full schemas.
fn schema_with_entries(entries: &[&str]) -> Schema {
    Schema {
        protocol: "test".into(),
        vertices: HashMap::new(),
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: entries.iter().map(|s| Name::from(*s)).collect(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
    }
}

/// Build a vertex universe `HashMap` from string ids, all of kind
/// `"object"`. `three_way_merge_entries` only reads `.contains_key`.
fn vertex_universe(names: &HashSet<&str>) -> HashMap<Name, Vertex> {
    names
        .iter()
        .map(|n| {
            (
                Name::from(*n),
                Vertex {
                    id: Name::from(*n),
                    kind: "object".into(),
                    nsid: None,
                },
            )
        })
        .collect()
}

/// Thin wrapper that lifts three string slices into the exact public
/// entries merge and returns the result as owned `String`s for easy
/// comparison against literals in property assertions.
fn run_entries_merge(
    base: &[&str],
    ours: &[&str],
    theirs: &[&str],
    universe: &HashSet<&str>,
) -> Vec<String> {
    let verts = vertex_universe(universe);
    three_way_merge_entries(
        &schema_with_entries(base),
        &schema_with_entries(ours),
        &schema_with_entries(theirs),
        &verts,
    )
    .into_iter()
    .map(|n: Name| n.to_string())
    .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// If an entry exists in base and at least one side deleted it,
    /// the merged entries must not contain it. Delete-propagation is
    /// the load-bearing guarantee that prevents the classic "revived
    /// after deletion" bug.
    #[test]
    fn delete_propagation(
        base_entries in prop::collection::hash_set("[a-z]", 0..=4),
        delete_side in prop::bool::ANY,
        delete_which in prop::option::of(any::<prop::sample::Index>()),
    ) {
        let base: Vec<&str> = base_entries.iter().map(String::as_str).collect();
        prop_assume!(!base.is_empty());

        let deleted_idx = delete_which.map_or(0, |i| i.index(base.len()));
        let deleted = base[deleted_idx];

        // Build `ours` and `theirs`: exactly one side drops `deleted`.
        let without_deleted: Vec<&str> =
            base.iter().copied().filter(|e| *e != deleted).collect();
        let (ours, theirs): (Vec<&str>, Vec<&str>) = if delete_side {
            (without_deleted, base.clone())
        } else {
            (base.clone(), without_deleted)
        };

        let universe: HashSet<&str> = base.iter().copied().collect();
        let merged = run_entries_merge(&base, &ours, &theirs, &universe);
        prop_assert!(
            !merged.iter().any(|e| e == deleted),
            "delete on one side must propagate — {deleted:?} survived"
        );
    }

    /// If a side adds an entry not present in base, and the other
    /// side does not delete the corresponding vertex, the addition
    /// lands in the merge.
    #[test]
    fn addition_propagation(
        base_entries in prop::collection::hash_set("[a-z]", 0..=3),
        added in "[A-Z]",
        from_ours in prop::bool::ANY,
    ) {
        let base: Vec<&str> = base_entries.iter().map(String::as_str).collect();
        prop_assume!(!base.iter().any(|e| *e == added));

        let mut ours: Vec<&str> = base.clone();
        let mut theirs: Vec<&str> = base.clone();
        if from_ours {
            ours.push(&added);
        } else {
            theirs.push(&added);
        }

        let mut universe: HashSet<&str> = base.iter().copied().collect();
        universe.insert(&added);

        let merged = run_entries_merge(&base, &ours, &theirs, &universe);
        prop_assert!(
            merged.iter().any(|e| e == &added),
            "unilateral addition must land — {added:?} missing"
        );
    }

    /// Every entry in the merge names a vertex in the merged-vertex
    /// universe. The basepoint map targets `Ob(C_merged)`, not the
    /// inputs' disjoint union.
    #[test]
    fn merged_entries_target_merged_universe(
        base_entries in prop::collection::hash_set("[a-z]", 0..=4),
        ours_extra in prop::collection::hash_set("[a-z]", 0..=2),
        theirs_extra in prop::collection::hash_set("[a-z]", 0..=2),
        keep_some in prop::collection::vec(prop::bool::ANY, 0..=10),
    ) {
        let base: Vec<&str> = base_entries.iter().map(String::as_str).collect();
        let ours_adds: Vec<&str> = ours_extra.iter().map(String::as_str).collect();
        let theirs_adds: Vec<&str> = theirs_extra.iter().map(String::as_str).collect();

        let mut ours: Vec<&str> = base.clone();
        ours.extend(&ours_adds);
        let mut theirs: Vec<&str> = base.clone();
        theirs.extend(&theirs_adds);

        // Randomly thin the universe to force some entries to be
        // dropped by the membership check.
        let mut universe: HashSet<&str> = base
            .iter()
            .chain(ours_adds.iter())
            .chain(theirs_adds.iter())
            .copied()
            .collect();
        for (i, keep) in keep_some.iter().enumerate() {
            if !*keep {
                let to_drop: Option<&str> = universe
                    .iter()
                    .nth(i % universe.len().max(1))
                    .copied();
                if let Some(x) = to_drop {
                    universe.remove(x);
                }
            }
        }

        let merged = run_entries_merge(&base, &ours, &theirs, &universe);
        for e in &merged {
            prop_assert!(
                universe.contains(e.as_str()),
                "merged entry {e:?} escapes the merged vertex universe"
            );
        }
    }

    /// The entries merge is an identity on the diagonal: merging a
    /// schema with itself (base = ours = theirs) yields the same
    /// entries. This is a sanity check on the case analysis and also
    /// ensures every entry still lands in the universe.
    #[test]
    fn diagonal_is_identity(entries in prop::collection::hash_set("[a-z]", 0..=5)) {
        let v: Vec<&str> = entries.iter().map(String::as_str).collect();
        let universe: HashSet<&str> = v.iter().copied().collect();
        let merged = run_entries_merge(&v, &v, &v, &universe);

        let merged_set: HashSet<String> = merged.into_iter().collect();
        let original_set: HashSet<String> = v.iter().map(|s| (*s).to_owned()).collect();
        prop_assert_eq!(merged_set, original_set);
    }
}

// ---------------------------------------------------------------------------
// Sanity unit cases that pin the canonical three-way-merge truth
// table. Running through the real public function means these tests
// fail if the production logic ever drifts from the categorical spec.
// ---------------------------------------------------------------------------

#[test]
fn entries_merge_canonical_truth_table() {
    // base = {A}, ours = {A}, theirs = {A} → {A}
    let u: HashSet<&str> = std::iter::once("A").collect();
    assert_eq!(
        run_entries_merge(&["A"], &["A"], &["A"], &u),
        vec!["A".to_owned()]
    );
    // base = {A}, ours = {}, theirs = {A} → {} (deletion propagates)
    assert_eq!(
        run_entries_merge(&["A"], &[], &["A"], &u),
        Vec::<String>::new()
    );
    // base = {}, ours = {A}, theirs = {} → {A} (unilateral addition)
    assert_eq!(
        run_entries_merge(&[], &["A"], &[], &u),
        vec!["A".to_owned()]
    );
    // Entry not in universe is dropped (basepoint must land in the
    // merged object's vertex set).
    let empty_u: HashSet<&str> = HashSet::new();
    assert_eq!(
        run_entries_merge(&["A"], &["A"], &["A"], &empty_u),
        Vec::<String>::new()
    );
}
