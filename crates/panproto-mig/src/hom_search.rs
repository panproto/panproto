//! Automatic schema morphism discovery via backtracking search.
//!
//! Given two schemas A and B, enumerate all valid schema morphisms
//! A → B by reducing to a constraint satisfaction problem (CSP) and
//! solving via backtracking with forward checking.
//!
//! This follows the approach of `Catlab.jl` (`AlgebraicJulia`) where
//! C-set homomorphism finding is reduced to CSP with naturality
//! constraints. The MRV (Minimum Remaining Values) heuristic orders
//! variable selection for efficient pruning.
//!
//! # References
//!
//! - AlgebraicJulia/Catlab.jl: backtracking search for C-set
//!   homomorphisms with monic/iso constraints
//! - Spivak 2012: functorial data migration via schema morphisms

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

/// Options controlling the homomorphism search.
#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    /// Require injective vertex map (no two source vertices map to
    /// the same target vertex).
    pub monic: bool,
    /// Require surjective vertex map (every target vertex is hit).
    pub epic: bool,
    /// Require bijective vertex map (isomorphism).
    pub iso: bool,
    /// Stop after finding this many morphisms (0 = unlimited).
    pub max_results: usize,
    /// Pre-assigned vertex mappings. The search extends this partial
    /// morphism to a total one.
    pub initial: HashMap<Name, Name>,
}

/// A discovered schema morphism with a quality score.
#[derive(Clone, Debug)]
pub struct FoundMorphism {
    /// Vertex mapping: source vertex ID → target vertex ID.
    pub vertex_map: HashMap<Name, Name>,
    /// Edge mapping: source edge → target edge.
    pub edge_map: HashMap<Edge, Edge>,
    /// Quality score in \[0.0, 1.0\], based on name similarity and
    /// structural overlap.
    pub quality: f64,
}

/// Find all valid schema morphisms from `src` to `tgt`.
///
/// Returns morphisms sorted by descending quality score. If
/// `opts.max_results` is non-zero, returns at most that many.
///
/// # Algorithm
///
/// Reduces to CSP:
/// - **Variables**: one per vertex in `src`
/// - **Domains**: compatible vertices in `tgt` (same kind)
/// - **Constraints**: naturality (edge-preserving) + optional
///   monic/epic/iso
///
/// Solves via backtracking with forward checking and MRV heuristic.
#[must_use]
pub fn find_morphisms(src: &Schema, tgt: &Schema, opts: &SearchOptions) -> Vec<FoundMorphism> {
    let mut state = BacktrackState::new(src, tgt, opts);
    let mut results = Vec::new();

    backtrack(&mut state, 0, &mut results, opts);

    // Sort by quality descending
    results.sort_by(|a, b| {
        b.quality
            .partial_cmp(&a.quality)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if opts.max_results > 0 {
        results.truncate(opts.max_results);
    }

    results
}

/// Find the single best schema morphism from `src` to `tgt`.
///
/// Returns `None` if no valid morphism exists.
#[must_use]
pub fn find_best_morphism(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
) -> Option<FoundMorphism> {
    let mut search_opts = opts.clone();
    // Find all morphisms to rank them (could optimize with branch-and-bound
    // but schemas are small enough that this is fine)
    search_opts.max_results = 0;
    let results = find_morphisms(src, tgt, &search_opts);
    results.into_iter().next()
}

// ---------------------------------------------------------------------------
// Internal: backtracking state
// ---------------------------------------------------------------------------

/// The order in which source vertices will be assigned.
struct VertexOrder {
    /// Source vertex IDs in assignment order (MRV: smallest domain first).
    order: Vec<Name>,
}

/// State for the backtracking search.
struct BacktrackState<'a> {
    src: &'a Schema,
    tgt: &'a Schema,
    /// For each source vertex, the set of compatible target vertices.
    domains: HashMap<Name, Vec<Name>>,
    /// Current partial assignment: source vertex → target vertex.
    assignment: HashMap<Name, Name>,
    /// Assignment order (MRV).
    vertex_order: VertexOrder,
    /// Target vertices already used (for monic constraint).
    used_targets: std::collections::HashSet<Name>,
}

impl<'a> BacktrackState<'a> {
    fn new(src: &'a Schema, tgt: &'a Schema, opts: &SearchOptions) -> Self {
        // Compute initial domains: for each source vertex, find all
        // target vertices with compatible kind.
        let mut domains: HashMap<Name, Vec<Name>> = HashMap::new();

        for (src_id, src_vertex) in &src.vertices {
            let compatible: Vec<Name> = opts.initial.get(src_id).map_or_else(
                || {
                    tgt.vertices
                        .iter()
                        .filter(|(_, tv)| tv.kind == src_vertex.kind)
                        .map(|(tid, _)| tid.clone())
                        .collect()
                },
                |tgt_id| vec![tgt_id.clone()],
            );
            domains.insert(src_id.clone(), compatible);
        }

        // MRV order: sort source vertices by domain size (smallest first)
        let mut order: Vec<Name> = domains.keys().cloned().collect();
        order.sort_by_key(|v| domains.get(v).map_or(0, Vec::len));

        let assignment: HashMap<Name, Name> = opts.initial.clone();
        let used_targets: std::collections::HashSet<Name> =
            opts.initial.values().cloned().collect();

        BacktrackState {
            src,
            tgt,
            domains,
            assignment,
            vertex_order: VertexOrder { order },
            used_targets,
        }
    }
}

/// Recursive backtracking search.
fn backtrack(
    state: &mut BacktrackState<'_>,
    depth: usize,
    results: &mut Vec<FoundMorphism>,
    opts: &SearchOptions,
) {
    // Check result limit
    if opts.max_results > 0 && results.len() >= opts.max_results {
        return;
    }

    // Base case: all vertices assigned
    if depth >= state.vertex_order.order.len() {
        // Check epic constraint
        if opts.epic || opts.iso {
            let assigned_targets: std::collections::HashSet<&Name> =
                state.assignment.values().collect();
            if assigned_targets.len() != state.tgt.vertices.len() {
                return; // Not surjective
            }
        }

        // Build the edge map from the vertex assignment
        if let Some(morphism) = build_morphism(state) {
            results.push(morphism);
        }
        return;
    }

    let src_vertex = state.vertex_order.order[depth].clone();

    // Skip if already assigned (from initial)
    if state.assignment.contains_key(&src_vertex) {
        backtrack(state, depth + 1, results, opts);
        return;
    }

    // Try each value in the domain
    let domain = state.domains.get(&src_vertex).cloned().unwrap_or_default();
    for tgt_vertex in domain {
        // Monic check: target not already used
        if (opts.monic || opts.iso) && state.used_targets.contains(&tgt_vertex) {
            continue;
        }

        // Forward check: does this assignment leave valid domains for
        // all unassigned neighbors?
        if !forward_check(state, &src_vertex, &tgt_vertex, depth) {
            continue;
        }

        // Assign
        state
            .assignment
            .insert(src_vertex.clone(), tgt_vertex.clone());
        state.used_targets.insert(tgt_vertex.clone());

        // Recurse
        backtrack(state, depth + 1, results, opts);

        // Unassign
        state.assignment.remove(&src_vertex);
        state.used_targets.remove(&tgt_vertex);

        if opts.max_results > 0 && results.len() >= opts.max_results {
            return;
        }
    }
}

/// Forward checking: verify that assigning `src_v → tgt_v` doesn't
/// make any unassigned neighbor's domain empty.
fn forward_check(state: &BacktrackState<'_>, src_v: &Name, tgt_v: &Name, depth: usize) -> bool {
    // Check edges: for every edge from src_v, there must exist a
    // compatible edge from tgt_v in the target schema.
    for src_edge in state.src.outgoing_edges(src_v) {
        let neighbor = &src_edge.tgt;
        if let Some(assigned_tgt) = state.assignment.get(neighbor) {
            // Neighbor already assigned — check that a compatible edge exists
            if !has_compatible_edge(state.tgt, tgt_v, assigned_tgt, src_edge) {
                return false;
            }
        } else {
            // Neighbor unassigned — check that at least one domain value
            // has a compatible edge from tgt_v
            let neighbor_domain = state.domains.get(neighbor);
            if let Some(domain) = neighbor_domain {
                let has_any = domain
                    .iter()
                    .any(|candidate| has_compatible_edge(state.tgt, tgt_v, candidate, src_edge));
                if !has_any {
                    return false;
                }
            }
        }
    }

    // Check incoming edges to src_v
    for src_edge in state.src.incoming_edges(src_v) {
        let neighbor = &src_edge.src;
        if let Some(assigned_tgt) = state.assignment.get(neighbor) {
            if !has_compatible_edge(state.tgt, assigned_tgt, tgt_v, src_edge) {
                return false;
            }
        } else {
            let neighbor_domain = state.domains.get(neighbor);
            if let Some(domain) = neighbor_domain {
                let has_any = domain
                    .iter()
                    .any(|candidate| has_compatible_edge(state.tgt, candidate, tgt_v, src_edge));
                if !has_any {
                    return false;
                }
            }
        }
    }

    // Check that unassigned vertices later in the order still have non-empty domains
    // given the monic constraint (if the target is now used up)
    if state.used_targets.len() + 1 > state.tgt.vertices.len() {
        // More assignments needed than available targets (with monic)
        // This is caught by domain emptiness above
    }

    let _ = depth; // Used for potential future optimizations
    true
}

/// Check if the target schema has an edge compatible with `src_edge`
/// from `tgt_src` to `tgt_tgt`.
///
/// An edge is compatible if it has the same kind. Names don't need to
/// match — a morphism can map an edge to a different-named edge (this
/// is what renaming IS). Name matching only affects quality scoring.
fn has_compatible_edge(
    tgt_schema: &Schema,
    tgt_src: &Name,
    tgt_tgt: &Name,
    src_edge: &Edge,
) -> bool {
    tgt_schema
        .edges_between(tgt_src, tgt_tgt)
        .iter()
        .any(|tgt_edge| tgt_edge.kind == src_edge.kind)
}

/// Build a complete morphism from the vertex assignment by deriving
/// the edge map.
fn build_morphism(state: &BacktrackState<'_>) -> Option<FoundMorphism> {
    let mut edge_map: HashMap<Edge, Edge> = HashMap::new();

    for src_edge in state.src.edges.keys() {
        let tgt_src = state.assignment.get(&src_edge.src)?;
        let tgt_tgt = state.assignment.get(&src_edge.tgt)?;

        // Find a compatible target edge (same kind between mapped vertices).
        // Prefer name-matching edges, fall back to any kind-matching edge.
        let candidates = state.tgt.edges_between(tgt_src, tgt_tgt);
        let tgt_edge = candidates
            .iter()
            .find(|te| te.kind == src_edge.kind && te.name == src_edge.name)
            .or_else(|| candidates.iter().find(|te| te.kind == src_edge.kind))?;

        edge_map.insert(src_edge.clone(), tgt_edge.clone());
    }

    let quality = compute_quality(&state.assignment, &edge_map, state.src, state.tgt);

    Some(FoundMorphism {
        vertex_map: state.assignment.clone(),
        edge_map,
        quality,
    })
}

/// Compute a quality score for a morphism.
///
/// Higher is better. Components:
/// - Name similarity: 1.0 - (avg edit distance / max name length)
/// - Edge name preservation: fraction of edges where source and
///   target have the same label name
fn compute_quality(
    vertex_map: &HashMap<Name, Name>,
    edge_map: &HashMap<Edge, Edge>,
    _src: &Schema,
    _tgt: &Schema,
) -> f64 {
    if vertex_map.is_empty() {
        return 1.0;
    }

    // Name similarity component (weight 0.5)
    let name_score: f64 = {
        let mut total = 0.0;
        for (src_id, tgt_id) in vertex_map {
            let dist = edit_distance(src_id.as_str(), tgt_id.as_str());
            let max_len = src_id.len().max(tgt_id.len()).max(1);
            #[allow(clippy::cast_precision_loss)]
            {
                total += 1.0 - (dist as f64 / max_len as f64);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            total / vertex_map.len() as f64
        }
    };

    // Edge name preservation component (weight 0.5)
    let edge_score: f64 = if edge_map.is_empty() {
        1.0
    } else {
        let matching = edge_map
            .iter()
            .filter(|(src_e, tgt_e)| src_e.name == tgt_e.name)
            .count();
        #[allow(clippy::cast_precision_loss)]
        {
            matching as f64 / edge_map.len() as f64
        }
    };

    0.5f64.mul_add(name_score, 0.5 * edge_score)
}

/// Simple edit distance (Levenshtein).
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a_bytes[i - 1] != b_bytes[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Convert a [`FoundMorphism`] into a [`crate::Migration`].
#[must_use]
pub fn morphism_to_migration(found: &FoundMorphism) -> crate::Migration {
    crate::Migration {
        vertex_map: found.vertex_map.clone(),
        edge_map: found.edge_map.clone(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, Schema, SchemaBuilder};

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
    fn identity_morphism_found() {
        let schema = build_schema(
            &[("root", "object"), ("root.name", "string")],
            &[("root", "root.name", "prop", "name")],
        );

        let results = find_morphisms(&schema, &schema, &SearchOptions::default());
        assert!(!results.is_empty(), "should find at least the identity");

        // The identity morphism should be among the results
        let has_identity = results.iter().any(|m| {
            m.vertex_map
                .iter()
                .all(|(src, tgt)| src.as_str() == tgt.as_str())
        });
        assert!(has_identity, "identity morphism should be found");
    }

    #[test]
    fn renamed_schema_morphism() {
        let old = build_schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", "text")],
        );
        let new = build_schema(
            &[("root", "object"), ("root.body", "string")],
            &[("root", "root.body", "prop", "body")],
        );

        let results = find_morphisms(&old, &new, &SearchOptions::default());
        assert!(
            !results.is_empty(),
            "should find morphism for renamed schema"
        );

        // root should map to root (same kind, has outgoing edges)
        let best = &results[0];
        assert_eq!(
            best.vertex_map.get("root").map(Name::as_str),
            Some("root"),
            "root should map to root"
        );
    }

    #[test]
    fn no_morphism_incompatible() {
        let a = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        // b has no string vertex — no valid mapping for root.x
        let b = build_schema(
            &[("root", "object"), ("root.y", "integer")],
            &[("root", "root.y", "prop", "y")],
        );

        let results = find_morphisms(&a, &b, &SearchOptions::default());
        assert!(
            results.is_empty(),
            "no morphism should exist between incompatible schemas"
        );
    }

    #[test]
    fn monic_rejects_non_injective() {
        // Two source string vertices, one target string vertex
        let src = build_schema(
            &[
                ("root", "object"),
                ("root.a", "string"),
                ("root.b", "string"),
            ],
            &[
                ("root", "root.a", "prop", "a"),
                ("root", "root.b", "prop", "b"),
            ],
        );
        let tgt = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );

        let opts = SearchOptions {
            monic: true,
            ..SearchOptions::default()
        };
        let results = find_morphisms(&src, &tgt, &opts);
        // With monic, both root.a and root.b can't map to root.x
        assert!(results.is_empty(), "monic should reject non-injective maps");
    }

    #[test]
    fn iso_finds_isomorphism() {
        let a = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let b = build_schema(
            &[("root", "object"), ("root.y", "string")],
            &[("root", "root.y", "prop", "y")],
        );

        let opts = SearchOptions {
            iso: true,
            ..SearchOptions::default()
        };
        let results = find_morphisms(&a, &b, &opts);
        assert!(
            !results.is_empty(),
            "isomorphism should exist between structurally identical schemas"
        );
    }

    #[test]
    fn initial_assignment_respected() {
        let schema = build_schema(
            &[
                ("root", "object"),
                ("root.a", "string"),
                ("root.b", "string"),
            ],
            &[
                ("root", "root.a", "prop", "a"),
                ("root", "root.b", "prop", "b"),
            ],
        );

        let mut initial = HashMap::new();
        initial.insert(Name::from("root.a"), Name::from("root.b"));
        initial.insert(Name::from("root.b"), Name::from("root.a"));
        initial.insert(Name::from("root"), Name::from("root"));

        let opts = SearchOptions {
            initial,
            ..SearchOptions::default()
        };
        let results = find_morphisms(&schema, &schema, &opts);
        assert!(
            !results.is_empty(),
            "should find morphism with initial assignment"
        );

        let m = &results[0];
        assert_eq!(m.vertex_map.get("root.a").map(Name::as_str), Some("root.b"));
        assert_eq!(m.vertex_map.get("root.b").map(Name::as_str), Some("root.a"));
    }

    #[test]
    fn quality_scoring_prefers_name_match() {
        let src = build_schema(
            &[("root", "object"), ("root.name", "string")],
            &[("root", "root.name", "prop", "name")],
        );
        // Target has two string vertices — one with matching name
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

        let results = find_morphisms(&src, &tgt, &SearchOptions::default());
        assert!(results.len() >= 2, "should find multiple morphisms");

        // Best morphism should map root.name → root.name (exact name match)
        let best = &results[0];
        assert_eq!(
            best.vertex_map.get("root.name").map(Name::as_str),
            Some("root.name"),
            "best morphism should prefer name-matching target"
        );
    }

    #[test]
    fn morphism_to_migration_conversion() {
        let schema = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );

        let results = find_morphisms(&schema, &schema, &SearchOptions::default());
        assert!(!results.is_empty());

        let mig = morphism_to_migration(&results[0]);
        assert_eq!(mig.vertex_map.len(), 2);
        assert_eq!(mig.edge_map.len(), 1);
    }

    #[test]
    fn empty_schema_morphism() {
        let empty = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        };

        let results = find_morphisms(&empty, &empty, &SearchOptions::default());
        assert_eq!(
            results.len(),
            1,
            "empty schema has exactly one self-morphism"
        );
    }

    #[test]
    fn find_best_returns_highest_quality() {
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

        let best = find_best_morphism(&src, &tgt, &SearchOptions::default());
        assert!(best.is_some());
        let m = best.unwrap();
        // Should pick the name-matching one
        assert_eq!(
            m.vertex_map.get("root.name").map(Name::as_str),
            Some("root.name")
        );
    }
}
