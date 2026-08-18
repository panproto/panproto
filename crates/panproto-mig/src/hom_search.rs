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
//! domain. Both return a `Result`, and the distinction it carries is the point:
//! `Ok([])`/`Ok(None)` means no total morphism exists, and `Err` means the
//! search could not be posed or run at all. Spelling the second as the first is
//! a wrong answer, not a conservative one — a caller told "no morphism exists"
//! about a pair whose identity is perfect has no way to tell. There is still no
//! second search.
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
    Assignment, Cfn, CfnBuilder, Cost, LimitKind, SearchBudget, SolveOutcome, all_optima_traced,
    dispatch_plan, eliminate, solve, solve_epic, solve_iso, solve_monic,
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
    /// about it. It is nonetheless a constraint the *search* enforces, not a
    /// filter over its answer — the search optimises over the surjective
    /// assignments, so a surjective total morphism is returned whenever one
    /// exists, whether or not it is also an argmin of the unconstrained
    /// objective. Filtering the argmins instead would report "none exists"
    /// whenever the optimum happened not to be onto.
    ///
    /// Satisfiable only when the source has at least as many vertices as the
    /// target, and on the injective path only when the two counts are equal.
    /// Both are tested before any search runs.
    ///
    /// **Total morphisms only.** Surjectivity is not a property a span can
    /// promise: a span's right leg is deliberately partial, the empty apex is
    /// always feasible, and [`find_span`] is documented never to refuse for want
    /// of a match. [`find_span`] and [`find_span_constrained`] therefore reject
    /// this flag with [`SpanError::EpicIsNotASpanProperty`] rather than ignoring
    /// it.
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

    /// How many results to return, capped at
    /// [`DEFAULT_OPTIMA_CAP`].
    ///
    /// `0` asks for everything the search enumerates. The cap applies to every
    /// value, not only to `0`: a request for more is answered with the cap and
    /// [`MorphismList::truncated`] says so. It has to, because the enumeration
    /// materialises one [`FoundMorphism`] per optimum with no memory accounting
    /// of its own, and the count is not bounded by the pair's size. Two
    /// eight-vertex schemas with no edges and no shared name characters tie the
    /// whole hom-set at the optimum, which is `8^8` morphisms: 4.6 GB and 164
    /// seconds uncapped, 4.6 MB and 11 ms capped.
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
    ///
    /// # One edge to one edge, which is the length-1 fragment of a functor
    ///
    /// A functor between the free categories on two schemas sends a source edge
    /// to a *path* in the target, of any length including zero. This sends it to
    /// a single target edge, so the only functors expressible here are those
    /// whose action on morphisms lands in the generators. Two consequences, both
    /// live:
    ///
    /// 1. A source edge that corresponds to a composite in the target — a field
    ///    the target reaches through an intermediate record, which is the shape
    ///    of every flattening or nesting change — has no image, so
    ///    [`find_morphisms`] reports no total morphism where one exists at
    ///    length 2.
    /// 2. A 1:n correspondence, one source field standing for several target
    ///    fields, is not expressible at all, in either direction.
    ///
    /// Widening this to paths is not a change of type alone. The naturality
    /// constraint the search enforces is stated over single target edges between
    /// the images of an edge's endpoints, and over paths it becomes a
    /// reachability question whose cost is not a table lookup; the objective's
    /// edge-name preservation component has no reading on a path, since a path
    /// has no name; and the search's variable set, one variable per source
    /// vertex, does not name the intermediate vertices a path would traverse.
    ///
    /// What would settle the scope is a count of how much is lost: the fraction
    /// of corpus pairs on which a length-2 target path would map a source edge
    /// that no length-1 edge maps. That measurement needs no solver change, only
    /// a reachability pass over each pair's target under the optimal vertex map,
    /// and it decides whether the cost above buys anything.
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
/// "Never refuses for want of a match" is a statement about the *answer*, not
/// about every input: posing the network can still fail. The one reachable case
/// is memory. No domain size is refused, so a wide record type and a
/// line-per-vertex parse pose like anything else; what is refused is the cost
/// tables the pair implies exceeding the memory budget, reported as
/// `SpanError::Build` wrapping `BuildError::Network` over
/// [`CfnError::OverMemoryBudget`](crate::solve::CfnError::OverMemoryBudget),
/// which names the bytes it measured and the budget it checked them against.
/// [`SpanSearch::with_budget`](crate::SpanSearch::with_budget) moves that
/// budget. Every entry point reports the refusal rather than spelling it
/// "nothing found".
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
/// inference, so a pair whose network is too wide for that, and any injective or
/// surjective search, yields the single canonical answer. Ties are broken by
/// taking the lexicographically smallest assignment vector in **decode** order,
/// which is the *reverse* of the elimination order;
/// [`SpanCertificate::tie_break_order`](crate::span::SpanCertificate::tie_break_order)
/// reports that sequence for a span.
///
/// # Errors
///
/// [`SpanError::Build`] when the network could not be posed, which the domain
/// ceiling reaches on a target schema wide enough in one kind;
/// [`SpanError::Iso`] when `opts.iso` is set and the maximum common sub-schema
/// search refused the network; [`SpanError::Stopped`] when branch and bound
/// spent a budget before reaching any complete assignment.
///
/// `Ok(vec![])` means no total morphism exists, which on the measured schema
/// corpus is the common case, and it means **only** that: a search that could
/// not run or could not finish reports `Err`, so a caller can tell the two
/// apart. That last case is what [`SpanError::Stopped`] exists for. The
/// distinction is not decoration: the empty answer is a statement about the
/// pair, while a stopped search is a statement about the budget, and the same
/// pair answered under a larger one.
/// [`find_span`] is the entry point that answers with what the two schemas *do*
/// share.
pub fn find_morphisms(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
) -> Result<MorphismList, SpanError> {
    find_morphisms_constrained(src, tgt, opts, &DomainConstraints::default())
}

/// The single best total schema morphism from `src` to `tgt`.
///
/// `Ok(None)` exactly when no total morphism exists. A search that stopped
/// before it could tell reports [`SpanError::Stopped`] rather than `Ok(None)`,
/// so this really is a statement about the pair.
///
/// # Errors
///
/// As [`find_morphisms`].
pub fn find_best_morphism(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
) -> Result<Option<FoundMorphism>, SpanError> {
    let mut opts = opts.clone();
    opts.max_results = 1;
    Ok(find_morphisms(src, tgt, &opts)?
        .morphisms
        .into_iter()
        .next())
}

/// [`find_morphisms`], with the caller's hard domain restrictions applied.
///
/// An excluded source is forced out of the apex, and a total morphism must map
/// every source vertex, so excluding any source vertex leaves no total morphism
/// and this returns empty. That is the honest reading of a request for a total
/// morphism that omits part of its domain; [`find_span_constrained`] is the
/// entry point that answers it.
///
/// # Errors
///
/// As [`find_morphisms`].
pub fn find_morphisms_constrained(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
) -> Result<MorphismList, SpanError> {
    find_morphisms_budgeted(src, tgt, opts, constraints, &SearchBudget::default())
}

/// [`find_morphisms_constrained`], against a budget the caller sets.
///
/// The other four entry points spend [`SearchBudget::default`]. This is the one
/// that does not, and it is what [`SpanSearch::with_budget`] is for the span
/// search: a host that has to bound the total-morphism search, and a test that
/// has to reach the stop the default budget takes minutes to reach, both need
/// the figure to be an argument rather than a constant.
///
/// The budget changes what is *reported*, never what is true. A search that
/// spends it before any complete assignment reports [`SpanError::Stopped`]
/// rather than an empty list, so shrinking the budget turns answers into
/// refusals and never into wrong answers.
///
/// # Errors
///
/// As [`find_morphisms`].
pub fn find_morphisms_budgeted(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
    budget: &SearchBudget,
) -> Result<MorphismList, SpanError> {
    // Surjectivity is decided by cardinality before anything is built. A vertex
    // map is a function, so its image is no larger than its domain; injectively,
    // no smaller either. Answering these here keeps the majority case free: most
    // corpus pairs that admit a total morphism admit no surjective one, and that
    // is the case a leaf-filtered branch and bound has no incumbent to prune
    // with.
    if opts.epic {
        let (source, target) = (src.vertices.len(), tgt.vertices.len());
        if source < target || (opts.monic && source != target) {
            return Ok(MorphismList::exhaustive(Vec::new()));
        }
    }

    let weights = constraints.scoring_weights.unwrap_or(DEFAULT_WEIGHTS);
    // A network that will not build is not the same answer as a network with no
    // solution. Laundering the first into the second is what tells a caller "no
    // morphism exists" about a pair whose identity morphism is perfect.
    let cfn = build_cfn(
        src,
        tgt,
        opts,
        constraints,
        &NoEvidence,
        weights,
        budget.mem_bytes,
    )?;
    // The cap binds whatever the caller asked for, including a number larger
    // than it. Zero is the request for everything and so reads as the cap; any
    // other figure is honoured up to the cap and reported as cut when it is not.
    let limit = if opts.max_results == 0 {
        DEFAULT_OPTIMA_CAP
    } else {
        opts.max_results.min(DEFAULT_OPTIMA_CAP)
    };

    if opts.iso {
        // At most one isomorphism is ever returned, so the cap cannot bind.
        return isomorphisms(&cfn, src, tgt, budget).map(MorphismList::exhaustive);
    }

    // The total-morphism search is the span search with `⊥` removed from every
    // domain, which is what makes the two one search rather than two.
    let total = without_bottom(&cfn, budget.mem_bytes);
    let (assignments, truncated) = if opts.epic {
        // Surjectivity is enforced inside the search rather than filtered from
        // its answer, so this cannot take the exact-inference route: a leaf
        // predicate has no counterpart in a message table.
        answered(solve_epic(&total, budget, tgt.vertices.len(), opts.monic))
    } else if opts.monic {
        answered(solve_monic(&total, budget))
    } else {
        optimal_assignments(&total, budget, limit)
    }
    .map_err(|limit| SpanError::Stopped { limit })?;

    Ok(MorphismList {
        morphisms: assignments
            .iter()
            .filter_map(|assignment| morphism_of(&total, src, tgt, assignment, opts))
            .collect(),
        truncated,
    })
}

/// The morphisms an enumeration returned, and whether it returned all of them.
///
/// The two are separate facts and a bare vector conflates them. A list of
/// [`DEFAULT_OPTIMA_CAP`] morphisms with `truncated` clear is a statement about
/// the pair: it has exactly that many optima. The same list with `truncated`
/// set is a statement about the cap, and says only that the pair has at least
/// that many.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct MorphismList {
    /// The morphisms, in the search's canonical order.
    ///
    /// Shorter than the enumeration when an assignment's induced edge map is
    /// not total, since such an assignment is no morphism and is dropped. That
    /// is a different shortfall from [`Self::truncated`], which reports the
    /// enumeration itself stopping early.
    pub morphisms: Vec<FoundMorphism>,

    /// Whether the optimum has more assignments than were enumerated.
    ///
    /// Set only by the exact-inference route, which is the only one that
    /// enumerates a tie rather than keeping one member of it. The branch and
    /// bound and isomorphism routes return at most one answer and so never
    /// truncate.
    pub truncated: bool,
}

impl MorphismList {
    /// A list nothing cut short.
    #[must_use]
    const fn exhaustive(morphisms: Vec<FoundMorphism>) -> Self {
        Self {
            morphisms,
            truncated: false,
        }
    }
}

/// [`find_best_morphism`], with the caller's hard domain restrictions applied.
///
/// # Errors
///
/// As [`find_morphisms`].
pub fn find_best_morphism_constrained(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
) -> Result<Option<FoundMorphism>, SpanError> {
    find_best_morphism_budgeted(src, tgt, opts, constraints, &SearchBudget::default())
}

/// [`find_best_morphism_constrained`], against a budget the caller sets.
///
/// # Errors
///
/// As [`find_morphisms`].
pub fn find_best_morphism_budgeted(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
    budget: &SearchBudget,
) -> Result<Option<FoundMorphism>, SpanError> {
    let mut opts = opts.clone();
    opts.max_results = 1;
    Ok(
        find_morphisms_budgeted(src, tgt, &opts, constraints, budget)?
            .morphisms
            .into_iter()
            .next(),
    )
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

/// The same network with `⊥` forbidden on every variable.
///
/// This is the total-morphism restriction of the span network, and it is what
/// [`find_morphisms`] searches. A caller measuring a corpus rather than solving
/// it wants the same network the search would have run on, since the domains,
/// the emptiness and the constraints are the thing being measured; posing a
/// second one by hand would measure something adjacent to it.
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
/// maps it anywhere. `⊥` is forbidden wherever the variable carries a unary
/// table, which [`build_cfn`] writes for every variable it poses.
///
/// `mem_bytes` is the budget the network in hand was built against. Rebuilding
/// it costs exactly what it cost the first time, so passing the same figure is
/// what makes the fallback below unreachable rather than merely unlikely.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::{DEFAULT_MEM_BYTES, ProductVerdict, detect_product};
/// use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchOptions, without_bottom};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into()],
///     ..Protocol::default()
/// };
/// let src = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.label", "string", None::<&str>)?
///     .edge("root", "root.label", "prop", Some("label"))?
///     .build()?;
/// let tgt = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .build()?;
///
/// let span = build_cfn(
///     &src,
///     &tgt,
///     &SearchOptions::default(),
///     &DomainConstraints::default(),
///     &NoEvidence,
///     DEFAULT_WEIGHTS,
///     DEFAULT_MEM_BYTES,
/// )?;
///
/// // Dropping `root.label` is feasible, so the span network is not empty.
/// assert!(!matches!(detect_product(&span), ProductVerdict::Empty { .. }));
///
/// // There is no total morphism, though: the target holds no `string` vertex,
/// // so `root.label` has nowhere to go.
/// let hom = without_bottom(&span, DEFAULT_MEM_BYTES);
/// assert!(matches!(detect_product(&hom), ProductVerdict::Empty { .. }));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn without_bottom(cfn: &Cfn, mem_bytes: usize) -> Cfn {
    let spec: Vec<(Name, Vec<Name>)> = cfn
        .variables()
        .iter()
        .map(|variable| (variable.name().clone(), variable.values().to_vec()))
        .collect();
    let Ok(mut builder) = CfnBuilder::with_mem_bytes(spec, cfn.weights(), mem_bytes) else {
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

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Assignments attaining the optimum, up to `limit` of them.
///
/// Enumerating more than one needs the message tables exact inference leaves
/// behind, so a network too wide for it contributes the single answer branch and
/// bound found.
///
/// The order eliminated under is [`dispatch_plan`]'s, which is the order
/// [`solve`] will follow, component by component where the network decomposes.
/// Choosing one independently would not merely cost the decomposition: the
/// tie-break among equally good assignments is stated relative to the order
/// used, so this entry point and [`SpanSearch::run`] would name different
/// canonical answers for one network.
fn optimal_assignments(
    cfn: &Cfn,
    budget: &SearchBudget,
    limit: usize,
) -> Result<(Vec<Assignment>, bool), LimitKind> {
    let plan = dispatch_plan(cfn, budget);
    if plan.exact {
        let buckets = eliminate(cfn, &plan.order);
        // No stop to report on this route. `dispatch_plan` prices exact
        // inference against the budget before running it, and `eliminate` then
        // runs to completion, so an empty result here is a proof that no
        // assignment is feasible rather than a search that gave up. What it can
        // report is the walk stopping for want of room, which is the cap
        // binding rather than the budget.
        let (optima, trace) = all_optima_traced(cfn, &buckets, limit);
        return Ok((optima, trace.truncated));
    }
    answered(solve(cfn, budget))
}

/// What a solve is entitled to report, separating "none" from "unknown".
///
/// A search that reached no complete assignment *and* stopped on a limit has no
/// answer: the empty vector its absent incumbent would collect into reads as
/// "no total morphism exists", which is a claim it never established. One that
/// reached a leaf does have an answer even when it could not prove optimality,
/// because in the `⊥`-removed network feasibility is exactly totality, so the
/// incumbent is a real total morphism. Refusing that one would discard a
/// correct answer, which is why the predicate is the conjunction rather than
/// `limit_hit.is_some()` alone.
fn answered(outcome: SolveOutcome) -> Result<(Vec<Assignment>, bool), LimitKind> {
    match (outcome.best, outcome.limit_hit) {
        (None, Some(limit)) => Err(limit),
        // Branch and bound keeps one incumbent rather than enumerating the tie
        // it belongs to, so there is nothing for the cap to cut here.
        (best, _) => Ok((best.into_iter().collect(), false)),
    }
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
    // Surjectivity is enforced by the search, not here. Restating it as a
    // filter would reintroduce the defect this replaced: a filter cannot tell
    // "the optimum is not onto" from "nothing is onto".
    debug_assert!(
        !opts.epic || is_surjective(&vertex_map, tgt),
        "the surjective search returned an assignment that is not onto"
    );
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
) -> Result<Vec<FoundMorphism>, SpanError> {
    // A refused network is reported, not spelled "the two are not isomorphic".
    let outcome = solve_iso(cfn, src, tgt, budget)?;
    let Some(assignment) = outcome.best else {
        return Ok(Vec::new());
    };
    let vertex_map = image_map(cfn, &assignment);
    if vertex_map.len() != src.vertices.len() || src.vertices.len() != tgt.vertices.len() {
        return Ok(Vec::new());
    }
    if !is_surjective(&vertex_map, tgt) {
        return Ok(Vec::new());
    }
    let Some(edge_map) = bijective_edge_map(src, tgt, &vertex_map) else {
        return Ok(Vec::new());
    };
    Ok(vec![FoundMorphism {
        vertex_map,
        edge_map,
        quality: cfn.quality_of(&assignment),
    }])
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
    use crate::solve::DEFAULT_MEM_BYTES;
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

        let results = find_morphisms(&schema, &schema, &SearchOptions::default())
            .unwrap()
            .morphisms;
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

        let results = find_morphisms(&old, &new, &SearchOptions::default())
            .unwrap()
            .morphisms;
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
            find_morphisms(&a, &b, &SearchOptions::default())
                .unwrap()
                .morphisms
                .is_empty(),
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
            find_morphisms(&src, &tgt, &opts)
                .unwrap()
                .morphisms
                .is_empty(),
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
            !find_morphisms(&a, &b, &opts).unwrap().morphisms.is_empty(),
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
            find_morphisms(&src, &tgt, &opts)
                .unwrap()
                .morphisms
                .is_empty(),
            "an edge map that is not injective is not an isomorphism"
        );
        assert!(
            find_morphisms(&tgt, &src, &opts)
                .unwrap()
                .morphisms
                .is_empty(),
            "an edge map that is not surjective is not an isomorphism"
        );

        // The same pair is a perfectly good homomorphism, and stays one.
        assert!(
            !find_morphisms(&src, &tgt, &SearchOptions::default())
                .unwrap()
                .morphisms
                .is_empty()
        );
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
        let results = find_morphisms(&src, &tgt, &opts).unwrap().morphisms;
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
        let results = find_morphisms(&schema, &schema, &opts).unwrap().morphisms;
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

        let results = find_morphisms(&src, &tgt, &SearchOptions::default())
            .unwrap()
            .morphisms;
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

        let results = find_morphisms(&src, &tgt, &SearchOptions::default())
            .unwrap()
            .morphisms;
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
        assert_eq!(
            find_morphisms(&src, &tgt, &opts).unwrap().morphisms.len(),
            1
        );
        assert!(
            find_morphisms(&src, &tgt, &SearchOptions::default())
                .unwrap()
                .morphisms
                .len()
                >= 2
        );
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
            .expect("the network poses")
            .expect("a total morphism exists");
        let all = find_morphisms(&src, &tgt, &SearchOptions::default())
            .unwrap()
            .morphisms;
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

        let results = find_morphisms(&schema, &schema, &SearchOptions::default())
            .unwrap()
            .morphisms;
        assert!(!results.is_empty());

        let mig = morphism_to_migration(&results[0]);
        assert_eq!(mig.vertex_map.len(), 2);
        assert_eq!(mig.edge_map.len(), 1);
    }

    #[test]
    fn empty_schema_morphism() {
        let empty = crate::span::empty_schema("test");

        let results = find_morphisms(&empty, &empty, &SearchOptions::default())
            .unwrap()
            .morphisms;
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
                .unwrap()
                .morphisms
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
            find_morphisms_constrained(&src, &tgt, &SearchOptions::default(), &constraints)
                .unwrap()
                .morphisms;
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
            find_morphisms(&src, &tgt, &opts)
                .unwrap()
                .morphisms
                .is_empty(),
            "two source vertices cannot cover three targets"
        );
        assert!(
            !find_morphisms(&src, &src, &opts)
                .unwrap()
                .morphisms
                .is_empty(),
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
            DEFAULT_MEM_BYTES,
        )
        .unwrap();
        let total = without_bottom(&cfn, DEFAULT_MEM_BYTES);

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

    /// [`optimal_assignments`] names the same tied optimum [`solve`] does.
    ///
    /// The fixture is the one `solve::dispatch` uses for the same question, and
    /// it is built so that the whole-network order and the per-component orders
    /// genuinely disagree: component A is a star whose hub sorts last by name,
    /// so on the whole graph min-fill beats descending name; component B is a
    /// tied pair on which the two tie on width, so descending name keeps it.
    /// Whole-network min-fill then settles B in the opposite sequence, and the
    /// tie-break among equally good assignments is stated relative to that
    /// sequence. Both answers are optima, so neither is wrong in isolation;
    /// what would be wrong is this entry point and [`SpanSearch::run`] calling
    /// different ones canonical.
    #[test]
    fn the_optima_of_a_decomposing_network_follow_the_dispatch_order() {
        use crate::solve::{VarId, decode, primal_graph};

        let names = ["a0", "a1", "a2", "a3", "a9hub", "b0", "b1"];
        let spec: Vec<(Name, Vec<Name>)> = names
            .iter()
            .map(|name| (Name::from(*name), vec![Name::from("t0"), Name::from("t1")]))
            .collect();
        let mut b = CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap();

        // Component A: a star on {a0..a3, a9hub}, every cost zero.
        for leaf in 0..4u32 {
            let scope = [VarId::new(leaf), VarId::new(4)];
            let len = b.table_length(&scope).unwrap();
            b.add_function(&scope, vec![Cost::BOT; len]).unwrap();
        }

        // Component B: b0 and b1 must differ and neither may drop, so the two
        // ways of satisfying that tie exactly.
        let pen = Cost::from_raw(100);
        b.add_unary_table(VarId::new(5), &[Cost::BOT, Cost::BOT, pen])
            .unwrap();
        b.add_unary_table(VarId::new(6), &[Cost::BOT, Cost::BOT, pen])
            .unwrap();
        let mut table = vec![Cost::BOT; 9];
        table[0] = Cost::TOP_SENTINEL; // (t0, t0)
        table[4] = Cost::TOP_SENTINEL; // (t1, t1)
        b.add_function(&[VarId::new(5), VarId::new(6)], table)
            .unwrap();
        let cfn = b.build();

        assert_eq!(
            primal_graph(&cfn).components().len(),
            2,
            "the fixture must decompose, or there is nothing to disagree about"
        );

        let budget = SearchBudget::default();
        let plan = dispatch_plan(&cfn, &budget);
        assert!(plan.exact, "both components price inside the budget");

        // The fixture's premise: eliminating the whole network under one order
        // settles the tie the other way.
        let whole_order = crate::solve::choose_order(&cfn).0;
        let whole = decode(&cfn, &eliminate(&cfn, &whole_order), &whole_order);
        let dispatched = solve(&cfn, &budget).best.unwrap();
        assert_eq!(
            cfn.evaluate(&whole),
            cfn.evaluate(&dispatched),
            "both are optima"
        );
        assert_ne!(
            whole, dispatched,
            "the fixture must make the two orders disagree, or the assertion below holds for the wrong reason"
        );

        let (found, _) = optimal_assignments(&cfn, &budget, 1).unwrap();
        assert_eq!(
            found.first(),
            Some(&dispatched),
            "the canonical answer is the one `solve` names, not the one a whole-network order names"
        );
    }
}
