//! The public face of the schema morphism search.
//!
//! Finding a schema morphism is a valued constraint satisfaction problem, and
//! [`solve`](crate::solve) owns the model and the algorithms. This module is the
//! surface over it: the options a caller sets, the shapes a caller gets back,
//! and the four historical entry points, now backed by an exact optimiser rather
//! than by enumerate-then-sort.
//!
//! # The span is the primary result
//!
//! [`find_span`] returns a [`SchemaSpan`], and it is total: leaving every source
//! vertex out of the apex is always feasible, so a pair with nothing in common
//! gets an empty apex rather than a refusal. On the measured schema corpus most
//! real pairs admit no total morphism at all, which is why the partial answer is
//! the primary one.
//!
//! [`find_morphisms`] and [`find_best_morphism`] are the total-morphism
//! restriction of that same search: the same network with `⊥` removed from every
//! domain. They still return `Option`/`Vec` and are still empty exactly when no
//! total morphism exists. There is no second search.
//!
//! # What changed, and what a caller has to do about it
//!
//! [`find_morphisms`] **no longer enumerates the hom-set.** It returns morphisms
//! attaining the optimum, capped by [`SearchOptions::max_results`]. A caller
//! that relied on getting every morphism, ranked, gets the optimal ones instead.
//! The doc on that function states the new contract in full.
//!
//! Three settings were removed rather than reimplemented. Preferred vertex
//! mappings are gone: their job, soft evidence, is now a unary cost, which is
//! strictly stronger, since a preference can only change which optimum is found
//! first while a cost changes which assignment is optimal. The edge-name domain
//! pruner is gone: it was a soft heuristic used as a hard filter, and edge-name
//! agreement already enters the objective. The name-similarity threshold is
//! gone: it cut a soft signal at a hard edge over full path-like identifiers.
//! Callers wanting a hard restriction use
//! [`DomainConstraints::restricted_domains`], and the node budget now lives on
//! [`SearchBudget`] where exhausting it is reported rather
//! than silently absorbed.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

use crate::error::SpanError;
use crate::solve::build::{NoEvidence, build_cfn};
use crate::solve::cost::{CostWeights, DEFAULT_WEIGHTS};
use crate::solve::{
    Assignment, Cfn, CfnBuilder, Cost, SearchBudget, all_optima, choose_order, eliminate,
    fits_budget, solve, solve_iso, solve_monic,
};
use crate::span::{
    DEFAULT_OPTIMA_CAP, SchemaSpan, SpanSearch, bijective_edge_map, greedy_edge_map, image_map,
    mappable_edges,
};

/// Options controlling the morphism search.
///
/// The three flags are properties of the answer wanted rather than of the
/// network searched: none of them is a cost, and each selects a different
/// algorithm.
#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    /// Require the vertex map to be injective.
    ///
    /// **Injectivity on vertices only.** An injective vertex map may still send
    /// two parallel source edges to one target edge, which is a homomorphism
    /// into a denser target and not an embedding. Ask for [`Self::iso`] when the
    /// edge map must be injective too.
    ///
    /// Injectivity constrains how variables share values, which no cost function
    /// states, so it completes the network's primal graph and rules out exact
    /// inference by construction rather than by budget.
    pub monic: bool,

    /// Require the vertex map to be surjective.
    ///
    /// Checked at the leaf and taking no part in any bound: it constrains the
    /// whole assignment at once, so a partial assignment carries no information
    /// about it, and paying to maintain that would buy pruning the measured
    /// corpus never needs. Satisfiable only when the source has at least as many
    /// vertices as the target, and on the injective path only when the two
    /// counts are equal.
    ///
    /// Because it is a filter on the answer rather than a constraint in the
    /// network, it is applied to the *optimal* morphisms: a surjective morphism
    /// that is not optimal is not searched for, so this can return empty where
    /// one exists. Surjectivity is not part of the objective and cannot be made
    /// so without changing what the objective measures.
    pub epic: bool,

    /// Require an isomorphism.
    ///
    /// Injective and surjective on vertices, **and** an injective, surjective
    /// edge map. A vertex bijection alone is not an isomorphism of schemas:
    /// three parallel arcs mapped onto one satisfy every condition a vertex map
    /// can state and have no inverse.
    ///
    /// This is the maximum common induced sub-schema problem, and it is what
    /// [`discover_overlap`](crate::discover_overlap) and a symmetric lens need.
    pub iso: bool,

    /// How many results to return. `0` means every result the search enumerates,
    /// up to its own cap.
    pub max_results: usize,

    /// Vertex mappings the caller knows and the search may not reconsider.
    ///
    /// A pinned source vertex keeps that one target in its domain, and only if
    /// the pin is kind compatible; an incompatible pin leaves the vertex with
    /// `⊥` as its only value, which drops it from the apex rather than failing
    /// the whole search. Every other path into a domain admits only
    /// kind-compatible targets and the edge map relies on that, so honouring an
    /// incompatible pin would hand back something that is not a morphism.
    ///
    /// This is for mappings a caller *knows*. Mappings something *inferred*
    /// belong in the evidence table [`SpanSearch::with_evidence`] reads, where
    /// they change which assignment is optimal without removing any other from
    /// the search.
    pub hard_pins: HashMap<Name, Name>,
}

/// Hard domain restrictions and an objective override.
///
/// Every field here is the caller stating which assignments are admissible, not
/// a heuristic filter. Soft evidence goes through
/// [`SpanSearch::with_evidence`] instead.
#[derive(Clone, Debug, Default)]
pub struct DomainConstraints {
    /// For each source vertex, restrict its domain to these targets.
    ///
    /// Vertices absent from this map are unrestricted beyond kind
    /// compatibility. Restricting to the empty list leaves `⊥` as the only
    /// value, which drops the vertex.
    pub restricted_domains: HashMap<Name, Vec<Name>>,

    /// Target vertices no source vertex may map to.
    pub excluded_targets: HashSet<Name>,

    /// Source vertices that must be left out of the apex.
    ///
    /// This forces `x_v = ⊥` rather than removing the variable, which keeps the
    /// variable set a pure function of the source schema. The two are equivalent
    /// for the objective: a variable with `⊥` as its only value contributes one
    /// fixed cost whatever else happens.
    pub excluded_sources: HashSet<Name>,

    /// Override the objective's component weights.
    ///
    /// Checked at construction: negative, non-finite and all-zero weight vectors
    /// are rejected by [`CostWeights::new`], so a weight can no longer push the
    /// reported quality outside `[0, 1]` and a `NaN` weight can no longer order
    /// results by its payload.
    pub scoring_weights: Option<CostWeights>,
}

/// A discovered total schema morphism with a quality score.
///
/// The degenerate case of a [`SchemaSpan`] whose left leg is onto, which
/// [`SchemaSpan::as_total_morphism`] converts to.
#[derive(Clone, Debug)]
pub struct FoundMorphism {
    /// Vertex mapping: source vertex identifier to target vertex identifier.
    pub vertex_map: HashMap<Name, Name>,

    /// Edge mapping: source edge to target edge.
    pub edge_map: HashMap<Edge, Edge>,

    /// Quality in `[0, 1]`, comparable only among morphisms out of one source
    /// schema. [`SchemaSpan::quality`] states why.
    pub quality: f64,
}

// ---------------------------------------------------------------------------
// Spans
// ---------------------------------------------------------------------------

/// The optimal span between two schemas.
///
/// Never refuses for want of a match. The module docs state what a span is and
/// [`SchemaSpan`] states what "optimal" means.
///
/// `protocol` is a parameter because the apex is a schema, and a schema is only
/// well formed against a protocol: inducing it re-validates the result rather
/// than assuming it.
///
/// # Errors
///
/// [`SpanError::Build`] if the network could not be posed, [`SpanError::Iso`] if
/// the iso path refused it, and [`SpanError::Apex`] if the induced apex is not a
/// well-formed schema.
///
/// # Examples
///
/// ```
/// use panproto_mig::hom_search::{SearchOptions, find_span};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
///     ..Protocol::default()
/// };
///
/// let old = SchemaBuilder::new(&protocol)
///     .vertex("post", "object", None::<&str>)?
///     .vertex("post.text", "string", None::<&str>)?
///     .vertex("post.likes", "integer", None::<&str>)?
///     .edge("post", "post.text", "prop", Some("text"))?
///     .edge("post", "post.likes", "prop", Some("likes"))?
///     .entry("post")
///     .build()?;
///
/// // The new schema dropped the counter, so no total morphism exists.
/// let new = SchemaBuilder::new(&protocol)
///     .vertex("post", "object", None::<&str>)?
///     .vertex("post.body", "string", None::<&str>)?
///     .edge("post", "post.body", "prop", Some("body"))?
///     .entry("post")
///     .build()?;
///
/// let span = find_span(&old, &new, &protocol, &SearchOptions::default())?;
///
/// assert!(!span.is_total(), "the counter has nowhere to go");
/// assert_eq!(span.apex.vertices.len(), 2);
/// assert!((span.apex_coverage - 2.0 / 3.0).abs() < 1e-12);
/// assert!(span.certificate.proven_optimal);
/// assert!(span.certificate.legs_are_functorial);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn find_span(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    opts: &SearchOptions,
) -> Result<SchemaSpan, SpanError> {
    SpanSearch::new(protocol)
        .with_options(opts.clone())
        .run(src, tgt)
}

/// [`find_span`], with the caller's hard domain restrictions applied.
///
/// [`DomainConstraints::scoring_weights`], when present, sets the objective's
/// component weights for this search.
///
/// # Errors
///
/// As [`find_span`].
pub fn find_span_constrained(
    src: &Schema,
    tgt: &Schema,
    protocol: &Protocol,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
) -> Result<SchemaSpan, SpanError> {
    SpanSearch::new(protocol)
        .with_options(opts.clone())
        .with_constraints(constraints.clone())
        .run(src, tgt)
}

// ---------------------------------------------------------------------------
// Total morphisms
// ---------------------------------------------------------------------------

/// Optimal total schema morphisms from `src` to `tgt`.
///
/// # This is not what it used to be
///
/// It used to enumerate the whole hom-set, score every member, sort, and
/// truncate. It now returns **morphisms attaining the optimum**, and nothing
/// else. Precisely:
///
/// 1. Every returned morphism has the same quality, which is the maximum over
///    all total morphisms. The list is therefore in non-increasing quality
///    order trivially, and a caller reading `results[0]` gets what it always
///    got.
/// 2. `opts.max_results` caps the count. Zero means every optimum the search
///    enumerates, up to [`DEFAULT_OPTIMA_CAP`]; it no longer means "the whole
///    hom-set", because the whole hom-set is no longer computed.
/// 3. A caller wanting a suboptimal alternative will not find one here. There is
///    no k-best over distinct quality levels.
///
/// Enumerating more than one optimum needs the message tables of exact
/// inference, so a pair whose network is too wide for that, and any injective
/// search, yields the single canonical answer. Ties are broken by taking the
/// lexicographically smallest assignment vector in **decode** order, which is
/// the *reverse* of the elimination order;
/// [`SpanCertificate::tie_break_order`](crate::span::SpanCertificate::tie_break_order)
/// reports that sequence for a span.
///
/// Returns empty exactly when no total morphism exists, which on the measured
/// schema corpus is the common case. [`find_span`] is the entry point that
/// answers with what the two schemas *do* share.
#[must_use]
pub fn find_morphisms(src: &Schema, tgt: &Schema, opts: &SearchOptions) -> Vec<FoundMorphism> {
    find_morphisms_constrained(src, tgt, opts, &DomainConstraints::default())
}

/// The single best total schema morphism from `src` to `tgt`.
///
/// `None` exactly when no total morphism exists.
#[must_use]
pub fn find_best_morphism(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
) -> Option<FoundMorphism> {
    let mut opts = opts.clone();
    opts.max_results = 1;
    find_morphisms(src, tgt, &opts).into_iter().next()
}

/// [`find_morphisms`], with the caller's hard domain restrictions applied.
///
/// An excluded source is forced out of the apex, and a total morphism must map
/// every source vertex, so excluding any source vertex leaves no total morphism
/// and this returns empty. That is the honest reading of a request for a total
/// morphism that omits part of its domain; [`find_span_constrained`] is the
/// entry point that answers it.
#[must_use]
pub fn find_morphisms_constrained(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
) -> Vec<FoundMorphism> {
    let weights = constraints.scoring_weights.unwrap_or(DEFAULT_WEIGHTS);
    let Ok(cfn) = build_cfn(src, tgt, opts, constraints, &NoEvidence, weights) else {
        return Vec::new();
    };
    let budget = SearchBudget::default();
    let limit = if opts.max_results == 0 {
        DEFAULT_OPTIMA_CAP
    } else {
        opts.max_results
    };

    if opts.iso {
        return isomorphisms(&cfn, src, tgt, &budget);
    }

    // The total-morphism search is the span search with `⊥` removed from every
    // domain, which is what makes the two one search rather than two.
    let total = without_bottom(&cfn);
    let assignments = if opts.monic {
        solve_monic(&total, &budget).best.into_iter().collect()
    } else {
        optimal_assignments(&total, &budget, limit)
    };

    assignments
        .iter()
        .filter_map(|assignment| morphism_of(&total, src, tgt, assignment, opts))
        .take(limit)
        .collect()
}

/// [`find_best_morphism`], with the caller's hard domain restrictions applied.
#[must_use]
pub fn find_best_morphism_constrained(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
) -> Option<FoundMorphism> {
    let mut opts = opts.clone();
    opts.max_results = 1;
    find_morphisms_constrained(src, tgt, &opts, constraints)
        .into_iter()
        .next()
}

/// Convert a [`FoundMorphism`] into a [`Migration`](crate::Migration).
///
/// The hyper-edge and label maps are left empty, which is a known gap: a
/// morphism search over vertices and edges has nothing to say about them. A span
/// records the same gap in its certificate rather than leaving it silent.
#[must_use]
pub fn morphism_to_migration(found: &FoundMorphism) -> crate::Migration {
    crate::Migration {
        vertex_map: found.vertex_map.clone(),
        edge_map: found.edge_map.clone(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        domain: None,
        codomain: None,
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// The same network with `⊥` forbidden on every variable.
///
/// Forbidding is a `⊤`-valued unary cost rather than a smaller domain, because
/// `⊤` is absorbing: any assignment using `⊥` costs `⊤` and is infeasible, which
/// is exactly what removing the value from the domain would mean. Encoding it as
/// a cost keeps the variable numbering, the table layout and the coverage radix
/// identical to the span network's, so an assignment means the same thing in
/// both.
///
/// A source vertex with no kind-compatible target then has an empty domain and
/// the whole network is infeasible, which is the right answer: no total morphism
/// maps it anywhere.
fn without_bottom(cfn: &Cfn) -> Cfn {
    let spec: Vec<(Name, Vec<Name>)> = cfn
        .variables()
        .iter()
        .map(|variable| (variable.name().clone(), variable.values().to_vec()))
        .collect();
    let Ok(mut builder) = CfnBuilder::new(spec, cfn.weights()) else {
        return cfn.clone();
    };
    builder.add_empty(cfn.c_empty());

    for var in cfn.variable_ids() {
        let Some(table) = cfn.unary(var) else {
            continue;
        };
        let mut table = table.to_vec();
        // The last slot is `⊥`, in every table, by the layout the builder fixes.
        if let Some(bottom) = table.last_mut() {
            *bottom = Cost::TOP_SENTINEL;
        }
        if builder.add_unary_table(var, &table).is_err() {
            return cfn.clone();
        }
    }
    for function in cfn.functions() {
        if builder
            .add_function(function.scope(), function.table().to_vec())
            .is_err()
        {
            return cfn.clone();
        }
    }
    builder.build()
}

/// Assignments attaining the optimum, up to `limit` of them.
///
/// Enumerating more than one needs the message tables exact inference leaves
/// behind, so a network too wide for it contributes the single answer branch and
/// bound found.
fn optimal_assignments(cfn: &Cfn, budget: &SearchBudget, limit: usize) -> Vec<Assignment> {
    let (order, width) = choose_order(cfn);
    if fits_budget(cfn, width, budget) {
        let buckets = eliminate(cfn, &order);
        return all_optima(cfn, &buckets, limit);
    }
    solve(cfn, budget).best.into_iter().collect()
}

/// One assignment as a total morphism, or `None` when it is not one.
fn morphism_of(
    cfn: &Cfn,
    src: &Schema,
    tgt: &Schema,
    assignment: &Assignment,
    opts: &SearchOptions,
) -> Option<FoundMorphism> {
    let vertex_map = image_map(cfn, assignment);
    if vertex_map.len() != src.vertices.len() {
        return None;
    }
    if opts.epic && !is_surjective(&vertex_map, tgt) {
        return None;
    }
    let edge_map = greedy_edge_map(src, tgt, &vertex_map);
    if edge_map.len() != mappable_edges(src) {
        return None;
    }
    Some(FoundMorphism {
        vertex_map,
        edge_map,
        quality: cfn.quality_of(assignment),
    })
}

/// The isomorphisms between two schemas, of which there is at most one here.
///
/// The iso path computes a maximum common induced sub-schema, so it returns an
/// isomorphism exactly when that sub-schema is the whole of both sides. Its
/// network needs `⊥` feasible, since every reward is measured against the cost
/// of dropping everything, so totality is checked on the answer rather than
/// forbidden in the network. That is sound: no cost function charges more for
/// mapping a vertex than for dropping it, so an extension of a feasible
/// assignment never costs more, and if an isomorphism exists the optimum is one.
fn isomorphisms(
    cfn: &Cfn,
    src: &Schema,
    tgt: &Schema,
    budget: &SearchBudget,
) -> Vec<FoundMorphism> {
    let Ok(outcome) = solve_iso(cfn, src, tgt, budget) else {
        return Vec::new();
    };
    let Some(assignment) = outcome.best else {
        return Vec::new();
    };
    let vertex_map = image_map(cfn, &assignment);
    if vertex_map.len() != src.vertices.len() || src.vertices.len() != tgt.vertices.len() {
        return Vec::new();
    }
    if !is_surjective(&vertex_map, tgt) {
        return Vec::new();
    }
    let Some(edge_map) = bijective_edge_map(src, tgt, &vertex_map) else {
        return Vec::new();
    };
    vec![FoundMorphism {
        vertex_map,
        edge_map,
        quality: cfn.quality_of(&assignment),
    }]
}

/// Whether a vertex map covers every target vertex.
fn is_surjective(vertex_map: &HashMap<Name, Name>, tgt: &Schema) -> bool {
    let mut images: Vec<&Name> = vertex_map.values().collect();
    images.sort_unstable();
    images.dedup();
    images.len() == tgt.vertices.len()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use panproto_schema::SchemaBuilder;

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
        assert!(
            results.iter().any(|m| m
                .vertex_map
                .iter()
                .all(|(src, tgt)| src.as_str() == tgt.as_str())),
            "identity morphism should be found"
        );
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
        assert!(!results.is_empty(), "a renamed schema still maps");
        assert_eq!(
            results[0].vertex_map.get("root").map(Name::as_str),
            Some("root")
        );
    }

    #[test]
    fn no_morphism_incompatible() {
        let a = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        // `b` has no string vertex, so `root.x` has nowhere to go.
        let b = build_schema(
            &[("root", "object"), ("root.y", "integer")],
            &[("root", "root.y", "prop", "y")],
        );

        assert!(
            find_morphisms(&a, &b, &SearchOptions::default()).is_empty(),
            "no total morphism exists between incompatible schemas"
        );
    }

    #[test]
    fn monic_rejects_non_injective() {
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
        assert!(
            find_morphisms(&src, &tgt, &opts).is_empty(),
            "two source strings cannot share one target injectively"
        );
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
        assert!(
            !find_morphisms(&a, &b, &opts).is_empty(),
            "structurally identical schemas are isomorphic"
        );
    }

    #[test]
    fn iso_refuses_a_vertex_bijection_whose_edge_map_is_not_one() {
        // Three parallel arcs onto one. The vertex map is injective and onto, so
        // every condition a vertex map can state is met, and the schemas are
        // still not isomorphic: the edge map is three-to-one and has no inverse.
        let src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[
                ("a", "b", "prop", "p"),
                ("a", "b", "prop", "q"),
                ("a", "b", "prop", "r"),
            ],
        );
        let tgt = build_schema(
            &[("x", "object"), ("y", "string")],
            &[("x", "y", "prop", "p")],
        );

        let opts = SearchOptions {
            iso: true,
            ..SearchOptions::default()
        };
        assert!(
            find_morphisms(&src, &tgt, &opts).is_empty(),
            "an edge map that is not injective is not an isomorphism"
        );
        assert!(
            find_morphisms(&tgt, &src, &opts).is_empty(),
            "an edge map that is not surjective is not an isomorphism"
        );

        // The same pair is a perfectly good homomorphism, and stays one.
        assert!(!find_morphisms(&src, &tgt, &SearchOptions::default()).is_empty());
    }

    #[test]
    fn iso_matches_parallel_arcs_by_kind_when_names_disagree() {
        // Two parallel arcs each side, names crossed, so the name-preferring
        // pass cannot pair them all and the fallback to kind-matching has to
        // complete the bijection.
        let src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[("a", "b", "prop", "p"), ("a", "b", "item", "q")],
        );
        let tgt = build_schema(
            &[("x", "object"), ("y", "string")],
            &[("x", "y", "prop", "s"), ("x", "y", "item", "t")],
        );

        let opts = SearchOptions {
            iso: true,
            ..SearchOptions::default()
        };
        let results = find_morphisms(&src, &tgt, &opts);
        assert!(!results.is_empty(), "the kinds match up, so this is an iso");

        let found = &results[0];
        assert_eq!(found.edge_map.len(), 2);
        let mut images: Vec<_> = found.edge_map.values().collect();
        images.sort_unstable();
        images.dedup();
        assert_eq!(images.len(), 2, "the edge map is injective");
        for (source, image) in &found.edge_map {
            assert_eq!(source.kind, image.kind, "arc kinds are preserved");
        }
    }

    #[test]
    fn hard_pins_are_respected() {
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

        let mut hard_pins = HashMap::new();
        hard_pins.insert(Name::from("root.a"), Name::from("root.b"));
        hard_pins.insert(Name::from("root.b"), Name::from("root.a"));
        hard_pins.insert(Name::from("root"), Name::from("root"));

        let opts = SearchOptions {
            hard_pins,
            ..SearchOptions::default()
        };
        let results = find_morphisms(&schema, &schema, &opts);
        assert!(!results.is_empty(), "the pinned assignment is a morphism");

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
        assert!(!results.is_empty());
        // Every returned morphism attains the optimum, and the optimum here is
        // the exact name match, so the wrong one is not among them at all.
        for found in &results {
            assert_eq!(
                found.vertex_map.get("root.name").map(Name::as_str),
                Some("root.name"),
                "an optimal morphism maps the name-matching target"
            );
        }
    }

    #[test]
    fn every_result_attains_the_optimum() {
        // Two indistinguishable string targets, so the assignment of `root.a` is
        // a genuine tie and both members of it are optimal.
        let src = build_schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("root.x", "string"),
                ("root.y", "string"),
            ],
            &[
                ("root", "root.x", "prop", "a"),
                ("root", "root.y", "prop", "a"),
            ],
        );

        let results = find_morphisms(&src, &tgt, &SearchOptions::default());
        assert!(results.len() >= 2, "the tie is enumerated");
        let best = results[0].quality;
        for found in &results {
            assert_eq!(found.quality, best, "every result attains the optimum");
        }
    }

    #[test]
    fn max_results_caps_the_list() {
        let src = build_schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("root.x", "string"),
                ("root.y", "string"),
            ],
            &[
                ("root", "root.x", "prop", "a"),
                ("root", "root.y", "prop", "a"),
            ],
        );

        let opts = SearchOptions {
            max_results: 1,
            ..SearchOptions::default()
        };
        assert_eq!(find_morphisms(&src, &tgt, &opts).len(), 1);
        assert!(find_morphisms(&src, &tgt, &SearchOptions::default()).len() >= 2);
    }

    #[test]
    fn find_best_agrees_with_the_head_of_find_morphisms() {
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

        let best = find_best_morphism(&src, &tgt, &SearchOptions::default())
            .expect("a total morphism exists");
        let all = find_morphisms(&src, &tgt, &SearchOptions::default());
        assert_eq!(best.quality, all[0].quality);
        assert_eq!(
            best.vertex_map.get("root.name").map(Name::as_str),
            Some("root.name")
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
        let empty = crate::span::empty_schema("test");

        let results = find_morphisms(&empty, &empty, &SearchOptions::default());
        assert_eq!(
            results.len(),
            1,
            "the empty schema has exactly one self-morphism"
        );
        assert!(results[0].vertex_map.is_empty());
    }

    #[test]
    fn an_excluded_source_leaves_no_total_morphism() {
        let schema = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let constraints = DomainConstraints {
            excluded_sources: HashSet::from([Name::from("root.x")]),
            ..DomainConstraints::default()
        };

        assert!(
            find_morphisms_constrained(&schema, &schema, &SearchOptions::default(), &constraints)
                .is_empty(),
            "a total morphism cannot omit part of its domain"
        );
    }

    #[test]
    fn a_restricted_domain_is_honoured() {
        let src = build_schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let tgt = build_schema(
            &[
                ("root", "object"),
                ("root.a", "string"),
                ("root.z", "string"),
            ],
            &[
                ("root", "root.a", "prop", "a"),
                ("root", "root.z", "prop", "z"),
            ],
        );
        let constraints = DomainConstraints {
            restricted_domains: HashMap::from([(Name::from("root.a"), vec![Name::from("root.z")])]),
            ..DomainConstraints::default()
        };

        let results =
            find_morphisms_constrained(&src, &tgt, &SearchOptions::default(), &constraints);
        assert!(!results.is_empty());
        for found in &results {
            assert_eq!(
                found.vertex_map.get("root.a").map(Name::as_str),
                Some("root.z"),
                "the restriction rules out the name-matching target"
            );
        }
    }

    #[test]
    fn epic_needs_every_target_covered() {
        let src = build_schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let tgt = build_schema(
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

        let opts = SearchOptions {
            epic: true,
            ..SearchOptions::default()
        };
        assert!(
            find_morphisms(&src, &tgt, &opts).is_empty(),
            "two source vertices cannot cover three targets"
        );
        assert!(
            !find_morphisms(&src, &src, &opts).is_empty(),
            "the identity is onto"
        );
    }

    #[test]
    fn forbidding_bottom_leaves_the_network_otherwise_alone() {
        let schema = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "x")],
        );
        let cfn = build_cfn(
            &schema,
            &schema,
            &SearchOptions::default(),
            &DomainConstraints::default(),
            &NoEvidence,
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        let total = without_bottom(&cfn);

        assert_eq!(total.n_variables(), cfn.n_variables());
        assert_eq!(total.n_functions(), cfn.n_functions());
        assert_eq!(total.radix(), cfn.radix());
        assert_eq!(total.weights(), cfn.weights());
        assert_eq!(total.c_empty(), cfn.c_empty());
        for var in cfn.variable_ids() {
            let before = cfn.unary(var).unwrap();
            let after = total.unary(var).unwrap();
            assert_eq!(before.len(), after.len());
            let (bottom, real) = after.split_last().unwrap();
            assert_eq!(*bottom, Cost::TOP_SENTINEL, "`⊥` is forbidden");
            assert_eq!(
                real,
                &before[..real.len()],
                "every other entry is untouched"
            );
        }
    }
}
