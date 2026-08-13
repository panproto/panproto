//! Forward-chaining hint propagation for auto-lens generation.
//!
//! Takes user-provided anchors (ground facts binding source vertices to
//! target vertices) and derives additional anchors by propagating along
//! unique edge-name matches. This is inspired by Prolog's constraint
//! propagation but implemented as a pure-Rust fixpoint loop.
//!
//! The derivation rules:
//!
//! 1. If `(A, B)` is an anchor and `A` has a unique outgoing edge with
//!    name `N` to child `C`, and `B` has a unique outgoing edge with
//!    name `N` to child `D`, then derive `(C, D)`.
//! 2. Same rule applied to incoming edges.
//! 3. Repeat until fixpoint (no new anchors derived).
//!
//! Conflicts (two derivation paths for the same source vertex yielding
//! different targets) are skipped rather than causing an error.

use std::collections::{HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_mig::align::defaults::W_ANCHOR;
use panproto_mig::{CostWeights, DomainConstraints};
use panproto_schema::Schema;

/// Derive additional anchors from user-provided anchors via forward chaining.
///
/// Starting from the given `anchors`, propagates along edges with unique
/// name matches in both source and target schemas. Returns the expanded
/// anchor set (a superset of the input).
///
/// A derived pair `(C, D)` is only added when:
/// 1. The source and target edges share both name and kind.
/// 2. The target candidate is unique (no ambiguity).
/// 3. The vertex kinds of `C` and `D` are compatible (a schema morphism
///    must preserve sorts).
/// 4. No conflicting derivation exists.
///
/// Terminates in at most `|V_src|` iterations, each adding at least one
/// new anchor. Total work is `O(V * E)` per iteration.
#[must_use]
pub fn derive_anchors(
    anchors: &HashMap<Name, Name>,
    src: &Schema,
    tgt: &Schema,
) -> HashMap<Name, Name> {
    let mut result = anchors.clone();
    let mut changed = true;

    while changed {
        changed = false;
        let snapshot: Vec<(Name, Name)> =
            result.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        for (src_v, tgt_v) in &snapshot {
            // Rule 1: propagate along outgoing edges with matching names
            for src_edge in src.outgoing_edges(src_v) {
                let child = &src_edge.tgt;
                if result.contains_key(child) {
                    continue;
                }

                let Some(edge_name) = &src_edge.name else {
                    continue;
                };

                // Find unique matching outgoing edge from tgt_v
                let tgt_candidates: Vec<&Name> = tgt
                    .outgoing_edges(tgt_v)
                    .iter()
                    .filter(|te| te.name.as_ref() == Some(edge_name) && te.kind == src_edge.kind)
                    .map(|te| &te.tgt)
                    .collect();

                if tgt_candidates.len() == 1 {
                    let tgt_child = tgt_candidates[0];

                    // Kind compatibility: a schema morphism must preserve
                    // vertex sorts. Skip if the vertex kinds don't match.
                    let kinds_compatible = src
                        .vertex(child)
                        .zip(tgt.vertex(tgt_child))
                        .is_some_and(|(sv, tv)| sv.kind == tv.kind);
                    if !kinds_compatible {
                        continue;
                    }

                    // Check for conflict: another derivation path pointing elsewhere
                    if let Some(existing) = result.get(child) {
                        if existing != tgt_child {
                            continue;
                        }
                    }
                    result.insert(child.clone(), tgt_child.clone());
                    changed = true;
                }
            }

            // Rule 2: propagate along incoming edges with matching names
            for src_edge in src.incoming_edges(src_v) {
                let parent = &src_edge.src;
                if result.contains_key(parent) {
                    continue;
                }

                let Some(edge_name) = &src_edge.name else {
                    continue;
                };

                let tgt_candidates: Vec<&Name> = tgt
                    .incoming_edges(tgt_v)
                    .iter()
                    .filter(|te| te.name.as_ref() == Some(edge_name) && te.kind == src_edge.kind)
                    .map(|te| &te.src)
                    .collect();

                if tgt_candidates.len() == 1 {
                    let tgt_parent = tgt_candidates[0];

                    // Kind compatibility check
                    let kinds_compatible = src
                        .vertex(parent)
                        .zip(tgt.vertex(tgt_parent))
                        .is_some_and(|(sv, tv)| sv.kind == tv.kind);
                    if !kinds_compatible {
                        continue;
                    }

                    if let Some(existing) = result.get(parent) {
                        if existing != tgt_parent {
                            continue;
                        }
                    }
                    result.insert(parent.clone(), tgt_parent.clone());
                    changed = true;
                }
            }
        }
    }

    result
}

/// Collect all vertices reachable from `start` via outgoing edges (BFS).
fn reachable_from(schema: &Schema, start: &Name) -> HashSet<Name> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    if schema.has_vertex(start) {
        queue.push_back(start.clone());
        visited.insert(start.clone());
    }

    while let Some(v) = queue.pop_front() {
        for edge in schema.outgoing_edges(&v) {
            if visited.insert(edge.tgt.clone()) {
                queue.push_back(edge.tgt.clone());
            }
        }
    }

    visited
}

/// Build [`DomainConstraints`] from scope, exclusion, and preference hints.
///
/// Scope constraints are conjunctive: if multiple scopes restrict the
/// same source vertex, the resulting domain is their intersection.
///
/// # Parameters
///
/// - `scope_constraints`: pairs of `(source_root, target_root)` for scope restrictions
/// - `excluded_targets`: target vertex names to exclude from all domains
/// - `excluded_sources`: source vertex names to exclude from search
/// - `scoring_weights`: optional override for the objective's component
///   weights, as `[name, edge, prop, degree]`. A vector that is negative,
///   non-finite or all zero is rejected by [`CostWeights::new`] and read as no
///   override, since such a vector cannot order results by anything but its own
///   payload.
#[must_use]
pub fn build_domain_constraints(
    src: &Schema,
    tgt: &Schema,
    scope_constraints: &[(Name, Name)],
    excluded_targets: &[Name],
    excluded_sources: &[Name],
    scoring_weights: Option<[f64; 4]>,
) -> DomainConstraints {
    let mut constraints = DomainConstraints {
        excluded_targets: excluded_targets.iter().cloned().collect(),
        excluded_sources: excluded_sources.iter().cloned().collect(),
        scoring_weights: scoring_weights
            .and_then(|w| CostWeights::new(w[0], w[1], w[2], w[3], W_ANCHOR).ok()),
        ..DomainConstraints::default()
    };

    // For each scope constraint, restrict source vertices reachable from
    // `under` to only map to target vertices reachable from `targets`.
    // Multiple scope constraints are conjunctive: if two scopes restrict
    // the same source vertex, the allowed domain is their intersection.
    for (under, targets) in scope_constraints {
        let src_reachable = reachable_from(src, under);
        let tgt_reachable: HashSet<Name> = reachable_from(tgt, targets);

        for src_v in &src_reachable {
            match constraints.restricted_domains.entry(src_v.clone()) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(tgt_reachable.iter().cloned().collect());
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    // Intersect with the existing restriction
                    e.get_mut().retain(|t| tgt_reachable.contains(t));
                }
            }
        }
    }

    constraints
}

/// String-level hint parts, extracted from a `HintSpec` via its accessor
/// methods. This avoids a circular dependency between `panproto-lens` and
/// `panproto-lens-dsl`.
pub struct HintParts {
    /// Source → target vertex name anchors.
    pub anchors: HashMap<String, String>,
    /// Scope constraint pairs: `(source_root, target_root)`.
    pub scope_pairs: Vec<(String, String)>,
    /// Target vertex names to exclude from all domains.
    pub excluded_targets: Vec<String>,
    /// Source vertex names to exclude from the search.
    pub excluded_sources: Vec<String>,
    /// Override the objective's component weights.
    pub scoring_weights: Option<[f64; 4]>,
}

/// Resolve hint parts into derived anchors and domain constraints.
///
/// This is the canonical entry point for all bindings (CLI, WASM, Python).
/// It runs forward-chaining anchor derivation, builds domain constraints
/// from scope/exclusion/preference data, and returns both.
///
/// Construct `HintParts` from `HintSpec` using its accessor methods:
/// `scoring_weights()`, `scope_pairs()`, `excluded_target_names()`,
/// `excluded_source_names()`.
#[must_use]
pub fn resolve_hints(
    parts: &HintParts,
    src: &Schema,
    tgt: &Schema,
) -> (HashMap<Name, Name>, DomainConstraints) {
    let name_anchors: HashMap<Name, Name> = parts
        .anchors
        .iter()
        .map(|(k, v)| (Name::from(k.as_str()), Name::from(v.as_str())))
        .collect();

    let derived = derive_anchors(&name_anchors, src, tgt);

    let scope_name_pairs: Vec<(Name, Name)> = parts
        .scope_pairs
        .iter()
        .map(|(u, t)| (Name::from(u.as_str()), Name::from(t.as_str())))
        .collect();
    let excl_tgt: Vec<Name> = parts
        .excluded_targets
        .iter()
        .map(|v| Name::from(v.as_str()))
        .collect();
    let excl_src: Vec<Name> = parts
        .excluded_sources
        .iter()
        .map(|v| Name::from(v.as_str()))
        .collect();

    let constraints = build_domain_constraints(
        src,
        tgt,
        &scope_name_pairs,
        &excl_tgt,
        &excl_src,
        parts.scoring_weights,
    );

    (derived, constraints)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut builder = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            builder = builder.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn derive_anchors_propagates_unique_edges() {
        // Source: root -> child_a (name: "a"), root -> child_b (name: "b")
        // Target: root -> child_x (name: "a"), root -> child_y (name: "b")
        let src = build_schema(
            &[
                ("root", "object"),
                ("child_a", "string"),
                ("child_b", "string"),
            ],
            &[
                ("root", "child_a", "prop", "a"),
                ("root", "child_b", "prop", "b"),
            ],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("child_x", "string"),
                ("child_y", "string"),
            ],
            &[
                ("root", "child_x", "prop", "a"),
                ("root", "child_y", "prop", "b"),
            ],
        );

        let mut initial = HashMap::new();
        initial.insert(Name::from("root"), Name::from("root"));

        let derived = derive_anchors(&initial, &src, &tgt);

        assert_eq!(derived.len(), 3, "should derive anchors for both children");
        assert_eq!(
            derived.get(&Name::from("child_a")).map(Name::as_str),
            Some("child_x")
        );
        assert_eq!(
            derived.get(&Name::from("child_b")).map(Name::as_str),
            Some("child_y")
        );
    }

    #[test]
    fn derive_anchors_rejects_incompatible_kinds() {
        // Source: root -> child (name: "x", kind: string)
        // Target: root -> child (name: "x", kind: integer)
        // Even though edge name matches uniquely, kinds are incompatible.
        let src = build_schema(
            &[("root", "object"), ("child", "string")],
            &[("root", "child", "prop", "x")],
        );
        let tgt = build_schema(
            &[("root", "object"), ("child", "integer")],
            &[("root", "child", "prop", "x")],
        );

        let mut initial = HashMap::new();
        initial.insert(Name::from("root"), Name::from("root"));

        let derived = derive_anchors(&initial, &src, &tgt);

        assert_eq!(
            derived.len(),
            1,
            "should NOT derive anchor when vertex kinds are incompatible"
        );
    }

    #[test]
    fn derive_anchors_stops_at_ambiguity() {
        // Source: root -> child (name: "x")
        // Target: root -> child_1 (name: "x"), root -> child_2 (name: "x")
        let src = build_schema(
            &[("root", "object"), ("child", "string")],
            &[("root", "child", "prop", "x")],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("child_1", "string"),
                ("child_2", "string"),
            ],
            &[
                ("root", "child_1", "prop", "x"),
                ("root", "child_2", "prop", "x"),
            ],
        );

        let mut initial = HashMap::new();
        initial.insert(Name::from("root"), Name::from("root"));

        let derived = derive_anchors(&initial, &src, &tgt);

        assert_eq!(derived.len(), 1, "should NOT derive anchor when ambiguous");
    }

    #[test]
    fn derive_anchors_fixpoint_chain() {
        // Chain: a -> b -> c -> d -> e
        // Each edge has a unique name, so anchoring a should derive all.
        let src = build_schema(
            &[
                ("a", "object"),
                ("b", "object"),
                ("c", "object"),
                ("d", "object"),
                ("e", "string"),
            ],
            &[
                ("a", "b", "prop", "x"),
                ("b", "c", "prop", "y"),
                ("c", "d", "prop", "z"),
                ("d", "e", "prop", "w"),
            ],
        );
        let tgt = build_schema(
            &[
                ("A", "object"),
                ("B", "object"),
                ("C", "object"),
                ("D", "object"),
                ("E", "string"),
            ],
            &[
                ("A", "B", "prop", "x"),
                ("B", "C", "prop", "y"),
                ("C", "D", "prop", "z"),
                ("D", "E", "prop", "w"),
            ],
        );

        let mut initial = HashMap::new();
        initial.insert(Name::from("a"), Name::from("A"));

        let derived = derive_anchors(&initial, &src, &tgt);

        assert_eq!(derived.len(), 5);
        assert_eq!(derived.get(&Name::from("b")).map(Name::as_str), Some("B"));
        assert_eq!(derived.get(&Name::from("e")).map(Name::as_str), Some("E"));
    }

    #[test]
    fn build_domain_constraints_scope() {
        let src = build_schema(
            &[("root", "object"), ("child", "string"), ("other", "string")],
            &[
                ("root", "child", "prop", "x"),
                ("root", "other", "prop", "y"),
            ],
        );
        let tgt = build_schema(
            &[
                ("tgt_root", "object"),
                ("tgt_child", "string"),
                ("unrelated", "string"),
            ],
            &[("tgt_root", "tgt_child", "prop", "x")],
        );

        let constraints = build_domain_constraints(
            &src,
            &tgt,
            &[(Name::from("root"), Name::from("tgt_root"))],
            &[],
            &[],
            None,
        );

        // "root" and "child" are reachable from "root" in src
        // "tgt_root" and "tgt_child" are reachable from "tgt_root" in tgt
        // So root's restricted domain should be {tgt_root, tgt_child}
        let root_domain = constraints
            .restricted_domains
            .get(&Name::from("root"))
            .unwrap();
        assert!(root_domain.contains(&Name::from("tgt_root")));
        assert!(root_domain.contains(&Name::from("tgt_child")));
        assert!(!root_domain.contains(&Name::from("unrelated")));
    }

    #[test]
    fn build_domain_constraints_exclusion() {
        let schema = build_schema(&[("root", "object")], &[]);
        let constraints = build_domain_constraints(
            &schema,
            &schema,
            &[],
            &[Name::from("secret")],
            &[Name::from("unused")],
            None,
        );

        assert!(constraints.excluded_targets.contains(&Name::from("secret")));
        assert!(constraints.excluded_sources.contains(&Name::from("unused")));
    }

    #[test]
    fn constrained_search_excludes_targets() {
        use panproto_mig::{SearchOptions, find_morphisms_constrained};

        let src = build_schema(
            &[("root", "object"), ("root.name", "string")],
            &[("root", "root.name", "prop", "name")],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("root.name", "string"),
                ("root.other", "string"),
            ],
            &[
                ("root", "root.name", "prop", "name"),
                ("root", "root.other", "prop", "other"),
            ],
        );

        let mut constraints = DomainConstraints::default();
        constraints
            .excluded_targets
            .insert(Name::from("root.other"));

        let results =
            find_morphisms_constrained(&src, &tgt, &SearchOptions::default(), &constraints)
                .unwrap();

        // All results should map root.name to root.name (root.other excluded)
        for m in &results {
            assert_ne!(
                m.vertex_map.get(&Name::from("root.name")).map(Name::as_str),
                Some("root.other"),
                "excluded target should not appear in morphism"
            );
        }
    }

    /// An excluded source is answered by the span, not by the total search.
    ///
    /// A total morphism maps every source vertex, so a request for one that
    /// also asks to omit part of its domain has no answer, and
    /// `find_morphisms_constrained` says so by returning empty. That is the
    /// honest reading, and it used to be answered with a partial map called a
    /// morphism. `find_span_constrained` is the entry point for the question
    /// actually being asked, and it returns the sub-schema the exclusion
    /// leaves.
    #[test]
    fn an_excluded_source_is_answered_by_the_span() {
        use panproto_mig::{SearchOptions, find_morphisms_constrained, find_span_constrained};

        // The source carries an integer child the target has no vertex for, so
        // no total morphism exists even before anything is excluded.
        let src = build_schema(
            &[
                ("root", "object"),
                ("root.name", "string"),
                ("root.extra", "integer"),
            ],
            &[
                ("root", "root.name", "prop", "name"),
                ("root", "root.extra", "prop", "extra"),
            ],
        );
        let tgt = build_schema(
            &[("root", "object"), ("root.name", "string")],
            &[("root", "root.name", "prop", "name")],
        );

        let unconstrained = find_morphisms_constrained(
            &src,
            &tgt,
            &SearchOptions::default(),
            &DomainConstraints::default(),
        )
        .expect("the network poses");
        assert!(
            unconstrained.is_empty(),
            "no total morphism exists: root.extra has no compatible target"
        );

        let mut constraints = DomainConstraints::default();
        constraints
            .excluded_sources
            .insert(Name::from("root.extra"));

        assert!(
            find_morphisms_constrained(&src, &tgt, &SearchOptions::default(), &constraints)
                .expect("the network poses")
                .is_empty(),
            "excluding a source leaves no total morphism, so this must stay empty"
        );

        let span = find_span_constrained(
            &src,
            &tgt,
            &test_protocol(),
            &SearchOptions::default(),
            &constraints,
        )
        .unwrap_or_else(|e| panic!("the span search never refuses for want of a match: {e}"));

        assert_eq!(
            span.right
                .vertex_map
                .get(&Name::from("root"))
                .map(Name::as_str),
            Some("root")
        );
        assert_eq!(
            span.right
                .vertex_map
                .get(&Name::from("root.name"))
                .map(Name::as_str),
            Some("root.name")
        );
        assert!(
            !span.apex.vertices.contains_key(&Name::from("root.extra")),
            "an excluded source is left out of the apex"
        );
        assert!(
            !span.is_total(),
            "the span drops a vertex, so it is not total"
        );
    }

    #[test]
    fn scope_constraints_intersect_on_same_vertex() {
        let src = build_schema(
            &[("root", "object"), ("child", "string")],
            &[("root", "child", "prop", "x")],
        );
        let tgt = build_schema(
            &[
                ("tgt_a", "object"),
                ("tgt_b", "object"),
                ("tgt_c", "string"),
                ("tgt_d", "string"),
            ],
            &[
                ("tgt_a", "tgt_c", "prop", "x"),
                ("tgt_b", "tgt_d", "prop", "y"),
            ],
        );

        // Scope 1: root -> tgt_a (reachable: {tgt_a, tgt_c})
        // Scope 2: root -> tgt_b (reachable: {tgt_b, tgt_d})
        // Intersection for "root": empty (tgt_a ∩ tgt_b from different scopes)
        // But "root" appears in scope 1's reachable AND scope 2's reachable,
        // so its restricted domain is intersected.
        let constraints = build_domain_constraints(
            &src,
            &tgt,
            &[
                (Name::from("root"), Name::from("tgt_a")),
                (Name::from("root"), Name::from("tgt_b")),
            ],
            &[],
            &[],
            None,
        );

        // "root" is reachable from both scope roots. Its domain should be
        // the intersection of {tgt_a, tgt_c} and {tgt_b, tgt_d} = empty.
        let root_domain = constraints
            .restricted_domains
            .get(&Name::from("root"))
            .unwrap();
        assert!(
            root_domain.is_empty(),
            "intersection of disjoint scope domains should be empty"
        );
    }
}
