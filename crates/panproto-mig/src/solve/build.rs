//! The point where schema semantics become numbers.
//!
//! [`build_cfn`] turns an ordered schema pair into the cost function network
//! the search minimises over. It is the only module that reads a
//! [`Schema`] and produces a [`Cost`]: kind filtering,
//! adjacency lookups, edit distance, Jaccard overlap, out-degree agreement,
//! anchor evidence, the fixed denominators, and the one float to fixed point
//! rounding all live here. Everything downstream sums integers.
//!
//! # The decomposition
//!
//! The quality score [`reference_quality`](crate::quality::reference_quality)
//! computes is a weighted mean of four components. Three of them are already
//! separable over source vertices and the fourth is separable over source
//! edges, so the whole score decomposes into one unary cost function per source
//! vertex and one binary cost function per constrained source vertex pair:
//!
//! | Component | Arity | Denominator |
//! |---|---|---|
//! | vertex name similarity | unary in `(v, a)` | `\|V_s\|` |
//! | edge name preservation | binary in `(x_src, x_tgt)` | `\|E_s\|` |
//! | outgoing edge name Jaccard | unary in `(v, a)` | `\|C_src\|` |
//! | out-degree agreement | unary in `(v, a)` | `\|V_s\|` |
//!
//! `|V_s|` is the source vertex count and `|E_s|` the number of source edges
//! both of whose endpoints are source vertices. The qualification matters
//! because [`Schema`]'s fields are public: an edge naming a vertex the schema
//! does not hold has no variable at that end, so no cost function can carry it,
//! and counting it in the denominator would score it as perfectly preserved.
//!
//! **Every denominator is a function of the source schema alone.** The
//! reference score divides by the size of the *assignment*, which makes two
//! partial assignments of different sizes incomparable: dropping a badly
//! matched vertex would raise the mean of what is left, so the best score would
//! go to the emptiest apex. Dividing by `|V_s|` instead means an unassigned
//! vertex contributes nothing to the numerator while still counting in the
//! denominator, so a span that covers half the source scores strictly worse
//! than one that covers all of it at the same per-pair quality.
//!
//! On a total morphism the change is not a semantic one for three of the four
//! components, since the assignment then has exactly `|V_s|` pairs and every
//! source edge counted by `|E_s|` has both endpoints assigned.
//!
//! # The Jaccard denominator
//!
//! The fourth is a genuine correction. The reference score normalises the
//! Jaccard component by the number of mapped pairs `(s, t)` where *either* side
//! has a named outgoing edge, which depends on the assignment. Here the
//! normaliser is
//!
//! ```text
//! C_src = { v ∈ V_s : v has at least one named outgoing edge }
//! ```
//!
//! which depends on the source alone, and the numerator runs over `C_src`
//! rather than over the mapped pairs.
//!
//! The two numerators agree on every total morphism. `C_src` is contained in
//! the source side of the reference's pair set, because a source vertex with a
//! named outgoing edge puts the union non-empty whatever it maps to; and a pair
//! the reference counts whose source is outside `C_src` has an empty source
//! name set, hence an empty intersection, hence Jaccard exactly zero. So the
//! extra pairs the reference counts contribute nothing to its numerator, and
//! only the normaliser differs.
//!
//! That difference is the point. Under the reference, a source leaf mapped onto
//! a childless target *drops out of the denominator* and raises the mean, while
//! the same leaf mapped onto a target with children enters the mean at zero and
//! lowers it; the score therefore rewards mapping leaves onto childless targets
//! for no structural reason. Under `C_src` a source leaf is outside the sum
//! whatever its image, and the incentive is gone.
//!
//! # `⊥`
//!
//! Every domain carries `⊥`, meaning the source vertex is left out of the apex.
//! `u(v, ⊥)` is the full quality penalty for that vertex, exactly as if it had
//! matched nothing, plus one [`DROP_UNIT`] in the coverage field of the packed
//! cost. A source edge with a dropped endpoint costs the full edge penalty
//! rather than `⊤`: it is unpreserved, not infeasible.
//!
//! # Rounding
//!
//! Each cost function entry is rounded to fixed point exactly once, by
//! [`quality_units`], after its components have been summed in `f64`. A total
//! assignment selects one unary entry per source vertex and at most one binary
//! entry per source edge, so the quality read back out of the integer objective
//! differs from an `f64` accumulation of the same terms by at most
//! `(|V_s| + |E_s|) / (2 · COST_SCALE)`. That expression is the bound and it is
//! attained: a 256-vertex source whose per-vertex name term is exactly `2⁻¹⁰`
//! of the scale reads `1.28 × 10⁻⁷`. It is under `4 × 10⁻⁸` at the measured
//! corpus sizes, which is why that constant appears in the tests here, but the
//! constant is a reading of the bound rather than a ceiling, and a test over
//! arbitrarily sized pairs computes the expression.
//!
//! # Hard constraints
//!
//! Seven constraints take the value `⊤`, and together they make the apex well
//! formed by construction rather than by repair, so that inducing the apex on
//! `{ v : x_v ≠ ⊥ }` never has to drop anything:
//!
//! | Constraint | Encoding |
//! |---|---|
//! | kind compatibility | absence from the domain |
//! | naturality | `⊤` when both endpoints are mapped and no kind-compatible target edge runs between their images |
//! | required edge | `⊤` when the owning vertex survives and an endpoint of one of its required edges does not |
//! | variant member | `⊤` when a coproduct survives and one of its variants does not |
//! | recursion point | `⊤` when a fixpoint marker survives and its target does not |
//! | span | `⊤` when exactly one of a span's two endpoints survives |
//! | hyper-edge signature | `⊤` when a signature is partly dropped, as a clique of pairwise constraints |
//!
//! None of them reads the anchor evidence, and the evidence term is bounded by
//! its weight, so which assignments are feasible is independent of the
//! evidence. That is what makes the optimum monotone in how much evidence the
//! caller supplies.
//!
//! **Three of them can add primal graph edges that are not schema edges.** A
//! recursion point, a span and a hyper-edge signature each constrain a set of
//! vertices that need not be joined by an edge, so the induced width of the
//! network can exceed the induced width of the source schema's own primal
//! graph. The width must therefore be measured on the network that comes out of
//! here rather than on the schema that went in.
//!
//! Injectivity and surjectivity are **not** encoded here. They are global
//! rather than local constraints, and the path that enforces them owns them.

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::hom_search::{DomainConstraints, SearchOptions};
use crate::quality::edit_distance;

use super::VarId;
use super::cfn::{Cfn, CfnBuilder, CfnError};
use super::cost::{Cost, CostWeights, DROP_UNIT, quality_units};

/// The cost of a component whose denominator is empty.
///
/// A component with nothing to average over reads `1.0`, its best value, and a
/// component at its best value costs nothing. The constant is named rather than
/// inlined because the two vacuous branches are stated as contributions to
/// `c_∅` and a named zero keeps that statement checkable at the point it takes
/// effect.
const VACUOUS_COMPONENT: Cost = Cost::BOT;

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

/// Aggregated alignment evidence, read as a reward-only unary cost.
///
/// Evidence enters the objective through exactly one term,
/// `w_anchor · (1 − confidence) / |V_s|`, and through nothing else. Three
/// prohibitions follow from that, and the builder holds to all three:
///
/// 1. **Evidence never removes a value from a domain.** Domains are computed
///    from kinds and from the caller's own hard restrictions, before any
///    evidence is read.
/// 2. **Evidence never produces a `⊤`-valued cost.** Confidence is bounded by
///    one and the anchor weight is finite, so the term is bounded by
///    `w_anchor / |V_s|`, which is far below `⊤`.
/// 3. **Evidence never reorders a variable or bounds a budget.** It changes
///    which assignment is optimal, not which one is found first.
///
/// The three together make the feasible set independent of the evidence, and
/// the objective non-increasing in it: supplying more evidence can improve the
/// optimum and can never make a previously feasible assignment infeasible.
pub trait Evidence {
    /// How strongly the evidence supports mapping `source` onto `target`, in
    /// `[0, 1]`.
    ///
    /// Zero means the evidence says nothing about the pair, which is what a
    /// table with no anchors returns for every pair. One is as strong as
    /// evidence gets.
    ///
    /// A value outside the range is a contract violation and [`build_cfn`]
    /// rejects it rather than clamping it, because clamping would make a table
    /// that returns `1.5` indistinguishable from one that returns `1.0` while
    /// leaving the bug in place.
    fn confidence(&self, source: &Name, target: &Name) -> f64;
}

/// The evidence table that has read nothing.
///
/// Reports zero confidence for every pair, so the anchor term is the same
/// constant `w_anchor / |V_s|` on every value of every variable, including `⊥`.
/// A term that is constant across a variable's whole domain cannot change which
/// assignment is optimal, so building against this is the same search as
/// building with the anchor weight set to zero.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct NoEvidence;

impl Evidence for NoEvidence {
    #[inline]
    fn confidence(&self, _source: &Name, _target: &Name) -> f64 {
        0.0
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a network could not be built from a schema pair.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
    /// The network refused what the decomposition asked it to hold.
    ///
    /// In practice this is
    /// [`CfnError::OverMemoryBudget`]:
    /// the cost tables the pair implies come to more than the caller's memory
    /// budget. No domain size is refused, so a wide record type or a
    /// line-per-vertex parse poses like anything else, and what is left is a
    /// measurement the caller can move. It is reported, never absorbed: a
    /// search that could not be posed is not the same answer as a search that
    /// found nothing, and the entry points keep the two apart.
    #[error("the network could not hold the decomposition: {source}")]
    Network {
        /// What the network refused.
        #[from]
        source: CfnError,
    },

    /// An evidence table reported a confidence outside `[0, 1]`.
    #[error(
        "evidence for `{source_vertex}` onto `{target_vertex}` is {confidence}, outside [0, 1]"
    )]
    EvidenceOutOfRange {
        /// The source vertex the evidence was read for.
        source_vertex: Name,
        /// The target vertex the evidence was read for.
        target_vertex: Name,
        /// What the table returned.
        confidence: f64,
    },
}

// ---------------------------------------------------------------------------
// The edge image
// ---------------------------------------------------------------------------

/// How well a source edge is preserved between two target vertices.
///
/// Ordered from best to worst, so comparison is "at least as well preserved
/// as".
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeMatch {
    /// A target edge of the same kind and the same name runs between the
    /// images.
    Named,
    /// A target edge of the same kind runs between the images, under a
    /// different name. This is what a rename looks like: the morphism is
    /// natural, the label moved.
    KindOnly,
    /// No target edge of the same kind runs between the images, so no morphism
    /// maps this edge and the assignment violates naturality.
    Absent,
}

/// The target edge a source edge maps to between two target vertices.
///
/// Prefers a target edge of the same kind *and* the same name, and falls back
/// to any target edge of the same kind. `None` means naturality fails: there is
/// nothing of the right kind between the two images for the edge to map to.
///
/// This is the edge map the span's right leg is built from, and it is also what
/// the naturality constraint and the edge component of the objective are read
/// off, so all three agree by sharing one function rather than by three
/// implementations happening to match.
#[must_use]
pub fn edge_image<'t>(
    tgt: &'t Schema,
    edge: &Edge,
    source_image: &Name,
    target_image: &Name,
) -> Option<&'t Edge> {
    let candidates = tgt.edges_between(source_image, target_image);
    candidates
        .iter()
        .find(|candidate| candidate.kind == edge.kind && candidate.name == edge.name)
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| candidate.kind == edge.kind)
        })
}

/// How well [`edge_image`] preserved a source edge.
///
/// Defined in terms of [`edge_image`] rather than beside it, so the indicator
/// the objective reads and the edge the span's right leg carries cannot drift
/// apart.
///
/// The name test on the returned edge is exactly the first stage of
/// [`edge_image`]'s search: if the fallback returned an edge whose name matches,
/// the first search would already have returned it, so a name match on the
/// result holds precisely when the first stage succeeded.
#[must_use]
pub fn edge_match(
    tgt: &Schema,
    edge: &Edge,
    source_image: &Name,
    target_image: &Name,
) -> EdgeMatch {
    match edge_image(tgt, edge, source_image, target_image) {
        Some(image) if image.name == edge.name => EdgeMatch::Named,
        Some(_) => EdgeMatch::KindOnly,
        None => EdgeMatch::Absent,
    }
}

// ---------------------------------------------------------------------------
// The builder entry point
// ---------------------------------------------------------------------------

/// Build the cost function network for one ordered schema pair.
///
/// One variable per source vertex, numbered in ascending vertex name order; one
/// value per kind-compatible target vertex, plus `⊥`; the four quality
/// components of the module docs as unary and binary cost functions; and the
/// seven `⊤`-valued constraints that keep the apex well formed.
///
/// `opts` is read for its hard pins alone. A pinned source vertex keeps that
/// one target in its domain, and only if the pin is kind compatible; an
/// incompatible pin leaves the vertex with `⊥` as its only value, which drops it
/// from the apex rather than failing the whole search. Search shape settings
/// (result limits, node limits, injectivity) belong to the solver rather than to
/// the network and are not read here.
///
/// `constraints` is read for `restricted_domains`, `excluded_targets` and
/// `excluded_sources`. An excluded source **forces** `x_v = ⊥` rather than
/// removing the variable, which keeps the variable set a pure function of the
/// source schema; the two are equivalent for the objective, since a variable
/// with `⊥` as its only value contributes one fixed cost whatever else happens.
/// Its scoring weights are superseded by the `weights` argument, which is
/// checked and normalised.
///
/// # Reading a quality back out
///
/// [`Cfn::quality_of`] reads the whole packed quality cost, anchor term
/// included. With the default weights the anchor weight is zero and the reading
/// is the four-component quality; a caller that raises the anchor weight is
/// steering the search with a term that then also shows up in the reading, and
/// wants to separate the two.
///
/// # The memory budget
///
/// `mem_bytes` is the caller's ceiling on the cost tables the network holds,
/// the same figure
/// [`SearchBudget::mem_bytes`](crate::solve::SearchBudget::mem_bytes) bounds
/// exact inference with. No domain size is refused: a source vertex offered a
/// thousand kind-compatible targets is an ordinary variable. What is refused is
/// a measured allocation, and the refusal names it.
///
/// # Errors
///
/// [`BuildError::Network`] if the network's cost tables would exceed
/// `mem_bytes`, and [`BuildError::EvidenceOutOfRange`] if the evidence table
/// reports a confidence outside `[0, 1]`.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::DEFAULT_MEM_BYTES;
/// use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchOptions};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into()],
///     ..Protocol::default()
/// };
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.label", "string", None::<&str>)?
///     .edge("root", "root.label", "prop", Some("label"))?
///     .build()?;
///
/// let cfn = build_cfn(
///     &schema,
///     &schema,
///     &SearchOptions::default(),
///     &DomainConstraints::default(),
///     &NoEvidence,
///     DEFAULT_WEIGHTS,
///     DEFAULT_MEM_BYTES,
/// )?;
///
/// // One variable per source vertex, and `⊥` alongside the one kind-compatible
/// // target each of them has.
/// assert_eq!(cfn.n_variables(), 2);
/// assert_eq!(cfn.max_domain(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_cfn(
    src: &Schema,
    tgt: &Schema,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
    evidence: &dyn Evidence,
    weights: CostWeights,
    mem_bytes: usize,
) -> Result<Cfn, BuildError> {
    let names = source_variable_names(src);
    let domains: Vec<Vec<Name>> = names
        .iter()
        .map(|source| domain_of(src, tgt, source, opts, constraints))
        .collect();

    let mut builder = CfnBuilder::with_mem_bytes(
        names.iter().cloned().zip(domains).collect::<Vec<_>>(),
        weights,
        mem_bytes,
    )?;
    let model = Model::new(src, tgt, &builder, names);

    add_constants(&mut builder, &model);
    add_unary_costs(&mut builder, &model, evidence)?;
    add_edge_costs(&mut builder, &model)?;
    add_apex_constraints(&mut builder, &model)?;

    Ok(builder.build())
}

// ---------------------------------------------------------------------------
// Variables and domains
// ---------------------------------------------------------------------------

/// The source vertices, in the ascending name order that fixes the numbering.
fn source_variable_names(src: &Schema) -> Vec<Name> {
    let mut names: Vec<Name> = src.vertices.keys().cloned().collect();
    names.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
    names
}

/// The target vertices one source vertex may map to, `⊥` excluded.
///
/// Kind compatibility first, then the caller's hard restrictions. Each step is
/// an intersection, so the order they are applied in does not matter.
fn domain_of(
    src: &Schema,
    tgt: &Schema,
    source: &Name,
    opts: &SearchOptions,
    constraints: &DomainConstraints,
) -> Vec<Name> {
    if constraints.excluded_sources.contains(source) {
        return Vec::new();
    }
    let Some(vertex) = src.vertices.get(source) else {
        return Vec::new();
    };

    // A pin is hard but not exempt. Every other path into a domain admits only
    // kind-compatible targets, and the edge map relies on that, so an
    // incompatible pin leaves the vertex with `⊥` alone rather than handing back
    // something that is not a morphism.
    let mut candidates: Vec<Name> = opts.hard_pins.get(source).map_or_else(
        || {
            tgt.vertices
                .iter()
                .filter(|(_, target)| target.kind == vertex.kind)
                .map(|(id, _)| id.clone())
                .collect()
        },
        |pinned| {
            tgt.vertices
                .get(pinned)
                .filter(|target| target.kind == vertex.kind)
                .map(|_| vec![pinned.clone()])
                .unwrap_or_default()
        },
    );

    if let Some(allowed) = constraints.restricted_domains.get(source) {
        let allowed: FxHashSet<&Name> = allowed.iter().collect();
        candidates.retain(|candidate| allowed.contains(candidate));
    }
    candidates.retain(|candidate| !constraints.excluded_targets.contains(candidate));

    candidates.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));
    candidates.dedup();
    candidates
}

// ---------------------------------------------------------------------------
// Precomputed schema facts
// ---------------------------------------------------------------------------

/// What the objective reads off one vertex.
struct VertexFacts<'a> {
    /// The distinct names on the vertex's outgoing edges.
    edge_names: FxHashSet<&'a str>,
    /// The number of outgoing edges, counted with multiplicity, which is the
    /// out-degree the reference score compares.
    degree: usize,
}

/// Read one vertex's facts off a schema.
fn vertex_facts<'a>(schema: &'a Schema, vertex: &str) -> VertexFacts<'a> {
    let outgoing = schema.outgoing_edges(vertex);
    VertexFacts {
        edge_names: outgoing
            .iter()
            .filter_map(|edge| edge.name.as_deref())
            .collect(),
        degree: outgoing.len(),
    }
}

/// Everything a cost function entry is computed from, gathered once.
struct Model<'a> {
    src: &'a Schema,
    tgt: &'a Schema,
    weights: CostWeights,
    radix: u64,
    /// `1 / |V_s|`, or zero when the source has no vertices.
    per_vertex: f64,
    /// `1 / |E_s|`, or zero when the source has no edges.
    per_edge: f64,
    /// `1 / |C_src|`, or zero when no source vertex has a named outgoing edge.
    per_prop: f64,
    /// How many source vertices have at least one named outgoing edge.
    prop_class: usize,
    /// How many source edges have a variable at both ends, which is the `|E_s|`
    /// the edge component divides by.
    edge_count: usize,
    /// Source vertex names, in variable order.
    names: Vec<Name>,
    /// Source vertex name to variable.
    index: FxHashMap<Name, VarId>,
    /// Each variable's target values, in slot order.
    values: Vec<Vec<Name>>,
    /// Each variable's source vertex facts, in variable order.
    src_facts: Vec<VertexFacts<'a>>,
    /// Every target vertex's facts.
    tgt_facts: FxHashMap<&'a Name, VertexFacts<'a>>,
}

impl<'a> Model<'a> {
    /// Gather the facts, taking the value lists from the builder so that slot
    /// order is read off the network rather than assumed of it.
    fn new(src: &'a Schema, tgt: &'a Schema, builder: &CfnBuilder, names: Vec<Name>) -> Self {
        let values: Vec<Vec<Name>> = builder
            .variables()
            .iter()
            .map(|variable| variable.values().to_vec())
            .collect();
        let index: FxHashMap<Name, VarId> = names
            .iter()
            .enumerate()
            .filter_map(|(position, name)| {
                u32::try_from(position)
                    .ok()
                    .map(|raw| (name.clone(), VarId::new(raw)))
            })
            .collect();
        let src_facts: Vec<VertexFacts<'a>> =
            names.iter().map(|name| vertex_facts(src, name)).collect();
        let tgt_facts: FxHashMap<&'a Name, VertexFacts<'a>> = tgt
            .vertices
            .keys()
            .map(|name| (name, vertex_facts(tgt, name)))
            .collect();

        let prop_class = src_facts
            .iter()
            .filter(|facts| !facts.edge_names.is_empty())
            .count();
        // `|E_s|` counts the source edges the network can charge for, which is
        // the edges both of whose endpoints are source vertices. An edge naming
        // a vertex `src.vertices` does not hold has no variable at that end, so
        // no cost function can carry it, and counting it in the denominator
        // would hand every assignment that edge's whole share of the edge
        // component unearned. The count stays a function of `src` alone, which
        // is what the decomposition's agreement with the reference score needs.
        let edge_count = src
            .edges
            .keys()
            .filter(|edge| index.contains_key(&edge.src) && index.contains_key(&edge.tgt))
            .count();

        Self {
            src,
            tgt,
            weights: builder.weights(),
            radix: builder.radix(),
            per_vertex: reciprocal(names.len()),
            per_edge: reciprocal(edge_count),
            per_prop: reciprocal(prop_class),
            prop_class,
            edge_count,
            names,
            index,
            values,
            src_facts,
            tgt_facts,
        }
    }

    /// The variable standing for a source vertex, if it has one.
    fn var(&self, source: &Name) -> Option<VarId> {
        self.index.get(source).copied()
    }

    /// One variable's target values, in slot order.
    fn values(&self, var: VarId) -> &[Name] {
        self.values.get(var.index()).map_or(&[], Vec::as_slice)
    }

    /// The unary cost of one value of one variable.
    ///
    /// `None` is `⊥`, which takes the full penalty on every component plus one
    /// [`DROP_UNIT`].
    fn unary_entry(
        &self,
        var: VarId,
        target: Option<&Name>,
        evidence: &dyn Evidence,
    ) -> Result<Cost, BuildError> {
        let (Some(source), Some(facts)) =
            (self.names.get(var.index()), self.src_facts.get(var.index()))
        else {
            return Ok(Cost::BOT);
        };
        let image = target.and_then(|name| self.tgt_facts.get(name).map(|facts| (name, facts)));

        let name_term = image.map_or(1.0, |(name, _)| name_dissimilarity(source, name));
        let degree_term = image.map_or(1.0, |(_, other)| {
            degree_dissimilarity(facts.degree, other.degree)
        });
        // The Jaccard component runs over `C_src` alone: a source vertex with no
        // named outgoing edge scores zero against every target, so leaving it out
        // of both numerator and denominator changes no total morphism's score
        // while removing the reference's incentive to prefer childless targets.
        let prop_term = if facts.edge_names.is_empty() {
            0.0
        } else {
            image.map_or(1.0, |(_, other)| {
                jaccard_dissimilarity(&facts.edge_names, &other.edge_names)
            })
        };
        let anchor_term = 1.0 - confidence(source, target, evidence)?;

        let name_cost = self.weights.name() * name_term * self.per_vertex;
        let degree_cost = self.weights.degree() * degree_term * self.per_vertex;
        let prop_cost = self.weights.prop() * prop_term * self.per_prop;
        let anchor_cost = self.weights.anchor() * anchor_term * self.per_vertex;

        let quality = name_cost + degree_cost + prop_cost + anchor_cost;
        let entry = Cost::packed(quality_units(quality), 0, self.radix);
        Ok(if target.is_none() {
            entry.combine(DROP_UNIT, Cost::TOP_SENTINEL)
        } else {
            entry
        })
    }

    /// The cost of one source edge under one pair of endpoint images.
    ///
    /// `None` on either side is `⊥`.
    fn edge_entry(&self, edge: &Edge, from: Option<&Name>, to: Option<&Name>) -> Cost {
        let (Some(from), Some(to)) = (from, to) else {
            // An endpoint left the apex, so the edge did too. It is unpreserved
            // rather than infeasible: dropping a vertex is what a span is for.
            return self.edge_cost(1.0);
        };
        match edge_match(self.tgt, edge, from, to) {
            EdgeMatch::Named => self.edge_cost(0.0),
            EdgeMatch::KindOnly => self.edge_cost(1.0),
            EdgeMatch::Absent => Cost::TOP_SENTINEL,
        }
    }

    /// One source edge's share of the edge component, at a given penalty.
    fn edge_cost(&self, penalty: f64) -> Cost {
        let quality = self.weights.edge() * penalty * self.per_edge;
        Cost::packed(quality_units(quality), 0, self.radix)
    }
}

// ---------------------------------------------------------------------------
// The four components
// ---------------------------------------------------------------------------

/// The evidence for one pair, checked to be in range.
///
/// `⊥` is not a target vertex, so no evidence bears on it and it takes the full
/// anchor penalty, exactly as it takes the full penalty on every other
/// component.
fn confidence(
    source: &Name,
    target: Option<&Name>,
    evidence: &dyn Evidence,
) -> Result<f64, BuildError> {
    let Some(target) = target else {
        return Ok(0.0);
    };
    let confidence = evidence.confidence(source, target);
    if (0.0..=1.0).contains(&confidence) {
        Ok(confidence)
    } else {
        Err(BuildError::EvidenceOutOfRange {
            source_vertex: source.clone(),
            target_vertex: target.clone(),
            confidence,
        })
    }
}

/// `1 / count`, and zero when there is nothing to divide by.
///
/// A zero denominator only ever multiplies an empty sum, so the value it
/// returns there is never read; returning zero rather than an infinity keeps
/// that from mattering if it ever were.
fn reciprocal(count: usize) -> f64 {
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    if count == 0 {
        0.0
    } else {
        1.0 / f64::from(count)
    }
}

/// `numerator / denominator`, and zero when the denominator is zero.
///
/// Both arguments are widened through `u32` rather than cast, so the conversion
/// is exact by construction on every value a schema can produce.
fn ratio(numerator: usize, denominator: usize) -> f64 {
    let denominator = u32::try_from(denominator).unwrap_or(u32::MAX);
    if denominator == 0 {
        return 0.0;
    }
    let numerator = u32::try_from(numerator).unwrap_or(u32::MAX);
    f64::from(numerator) / f64::from(denominator)
}

/// `1 − name_sim`: the byte-level edit distance between two vertex ids over the
/// longer of the two lengths.
fn name_dissimilarity(source: &Name, target: &Name) -> f64 {
    let source = source.as_str();
    let target = target.as_str();
    let span = source.len().max(target.len()).max(1);
    ratio(edit_distance(source, target), span)
}

/// `1 − deg_sim`: the out-degree gap over the larger out-degree.
///
/// Two vertices that both have no outgoing edges agree perfectly, which is why
/// the zero case reads zero rather than one.
fn degree_dissimilarity(source: usize, target: usize) -> f64 {
    ratio(source.abs_diff(target), source.max(target))
}

/// `1 − J`: the symmetric difference of two outgoing edge name sets over their
/// union.
///
/// Computed as an integer ratio rather than as `1 − intersection/union` so that
/// the subtraction happens on counts rather than on a float.
fn jaccard_dissimilarity(source: &FxHashSet<&str>, target: &FxHashSet<&str>) -> f64 {
    let shared = source.intersection(target).count();
    let union = source.len() + target.len() - shared;
    ratio(union - shared, union)
}

// ---------------------------------------------------------------------------
// Filling the network
// ---------------------------------------------------------------------------

/// Add the parts of the objective no assignment can change.
const fn add_constants(builder: &mut CfnBuilder, model: &Model<'_>) {
    if model.prop_class == 0 {
        builder.add_empty(VACUOUS_COMPONENT);
    }
    if model.edge_count == 0 {
        builder.add_empty(VACUOUS_COMPONENT);
    }
}

/// Add the three unary components, one table per variable.
fn add_unary_costs(
    builder: &mut CfnBuilder,
    model: &Model<'_>,
    evidence: &dyn Evidence,
) -> Result<(), BuildError> {
    for position in 0..model.values.len() {
        let Ok(raw) = u32::try_from(position) else {
            continue;
        };
        let var = VarId::new(raw);
        let values = model.values(var);
        let mut table = Vec::with_capacity(values.len() + 1);
        for value in values {
            table.push(model.unary_entry(var, Some(value), evidence)?);
        }
        table.push(model.unary_entry(var, None, evidence)?);
        builder.add_unary_table(var, &table)?;
    }
    Ok(())
}

/// Add the edge component and the naturality constraint, which are the same
/// cost function.
///
/// Source edges are visited in sorted order so that the network is a function
/// of the schema pair rather than of a hash seed. Parallel edges between one
/// vertex pair land on one scope and are merged there by the builder, which is
/// what keeps at most one cost function per scope.
fn add_edge_costs(builder: &mut CfnBuilder, model: &Model<'_>) -> Result<(), BuildError> {
    let mut edges: Vec<&Edge> = model.src.edges.keys().collect();
    edges.sort_unstable();

    for edge in edges {
        // An edge with no variable at one end names a vertex the source schema
        // does not hold, so there is no scope to put it on. It is left out of
        // `Model::edge_count` for the same reason, which is what keeps the
        // denominator equal to the number of edges actually charged.
        let (Some(from), Some(to)) = (model.var(&edge.src), model.var(&edge.tgt)) else {
            continue;
        };
        if from == to {
            // A self-loop constrains one variable along the diagonal of a binary
            // table, so its cost belongs in that variable's unary table. Reading
            // it as a binary function would put two functions on a one-variable
            // scope and double count the diagonal.
            let values = model.values(from);
            let mut table = Vec::with_capacity(values.len() + 1);
            for value in values {
                table.push(model.edge_entry(edge, Some(value), Some(value)));
            }
            table.push(model.edge_entry(edge, None, None));
            builder.add_unary_table(from, &table)?;
        } else {
            add_binary(builder, model, from, to, |from, to| {
                model.edge_entry(edge, from, to)
            })?;
        }
    }
    Ok(())
}

/// Add the five structural constraints that keep the apex well formed.
fn add_apex_constraints(builder: &mut CfnBuilder, model: &Model<'_>) -> Result<(), BuildError> {
    add_required_constraints(builder, model)?;
    add_variant_constraints(builder, model)?;
    add_recursion_constraints(builder, model)?;
    add_span_constraints(builder, model)?;
    add_hyper_edge_constraints(builder, model)
}

/// A surviving vertex keeps every edge it requires, so neither endpoint of a
/// required edge may be dropped while its owner survives.
fn add_required_constraints(builder: &mut CfnBuilder, model: &Model<'_>) -> Result<(), BuildError> {
    let mut owners: Vec<(&Name, &Vec<Edge>)> = model.src.required.iter().collect();
    owners.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (owner, required) in owners {
        let owner_var = model.var(owner);
        let mut edges: Vec<&Edge> = required.iter().collect();
        edges.sort_unstable();
        for edge in edges {
            add_pair(builder, model, owner_var, model.var(&edge.src), implication)?;
            add_pair(builder, model, owner_var, model.var(&edge.tgt), implication)?;
        }
    }
    Ok(())
}

/// A surviving coproduct keeps every variant injected into it, since a variant
/// naming a dropped vertex is a dangling reference.
fn add_variant_constraints(builder: &mut CfnBuilder, model: &Model<'_>) -> Result<(), BuildError> {
    let mut coproducts: Vec<(&Name, &Vec<panproto_schema::Variant>)> =
        model.src.variants.iter().collect();
    coproducts.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (coproduct, variants) in coproducts {
        let coproduct_var = model.var(coproduct);
        for variant in variants {
            add_pair(
                builder,
                model,
                coproduct_var,
                model.var(&variant.id),
                implication,
            )?;
            add_pair(
                builder,
                model,
                coproduct_var,
                model.var(&variant.parent_vertex),
                implication,
            )?;
        }
    }
    Ok(())
}

/// A surviving fixpoint marker keeps the vertex it unfolds to.
fn add_recursion_constraints(
    builder: &mut CfnBuilder,
    model: &Model<'_>,
) -> Result<(), BuildError> {
    let mut points: Vec<(&Name, &panproto_schema::RecursionPoint)> =
        model.src.recursion_points.iter().collect();
    points.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (_, point) in points {
        add_pair(
            builder,
            model,
            model.var(&point.mu_id),
            model.var(&point.target_vertex),
            implication,
        )?;
    }
    Ok(())
}

/// A span is kept whole or dropped whole: it names two vertices, and one of
/// them alone is a dangling reference either way.
fn add_span_constraints(builder: &mut CfnBuilder, model: &Model<'_>) -> Result<(), BuildError> {
    let mut spans: Vec<(&Name, &panproto_schema::Span)> = model.src.spans.iter().collect();
    spans.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (_, span) in spans {
        add_pair(
            builder,
            model,
            model.var(&span.left),
            model.var(&span.right),
            together,
        )?;
    }
    Ok(())
}

/// A hyper-edge signature is kept whole or dropped whole, as a clique of
/// pairwise constraints.
///
/// The clique is equivalent to the one constraint of full arity it stands in
/// for: a signature is partly dropped exactly when some pair of its members
/// disagrees about whether to survive.
fn add_hyper_edge_constraints(
    builder: &mut CfnBuilder,
    model: &Model<'_>,
) -> Result<(), BuildError> {
    let mut hyper_edges: Vec<(&Name, &panproto_schema::HyperEdge)> =
        model.src.hyper_edges.iter().collect();
    hyper_edges.sort_unstable_by(|left, right| left.0.as_str().cmp(right.0.as_str()));

    for (_, hyper_edge) in hyper_edges {
        let mut members: Vec<VarId> = hyper_edge
            .signature
            .values()
            .filter_map(|vertex| model.var(vertex))
            .collect();
        members.sort_unstable();
        members.dedup();

        for (position, &first) in members.iter().enumerate() {
            for &second in members.iter().skip(position + 1) {
                add_pair(builder, model, Some(first), Some(second), together)?;
            }
        }
    }
    Ok(())
}

/// `⊤` when the antecedent survives and the consequent does not.
const fn implication(antecedent: Option<&Name>, consequent: Option<&Name>) -> Cost {
    if antecedent.is_some() && consequent.is_none() {
        Cost::TOP_SENTINEL
    } else {
        Cost::BOT
    }
}

/// `⊤` when exactly one of the two survives.
const fn together(left: Option<&Name>, right: Option<&Name>) -> Cost {
    if left.is_none() == right.is_none() {
        Cost::BOT
    } else {
        Cost::TOP_SENTINEL
    }
}

/// Add a binary cost function over two variables that may be absent or equal.
///
/// A constraint on one variable with itself is vacuous under both shapes above:
/// a vertex cannot both survive and be dropped. A constraint naming a vertex the
/// source schema does not have is skipped, since there is no variable to
/// constrain.
fn add_pair<F>(
    builder: &mut CfnBuilder,
    model: &Model<'_>,
    first: Option<VarId>,
    second: Option<VarId>,
    entry: F,
) -> Result<(), BuildError>
where
    F: Fn(Option<&Name>, Option<&Name>) -> Cost,
{
    let (Some(first), Some(second)) = (first, second) else {
        return Ok(());
    };
    if first == second {
        return Ok(());
    }
    add_binary(builder, model, first, second, entry)
}

/// Add a binary cost function, given in terms of the two variables in the order
/// the constraint names them.
///
/// The scope handed to the network is ascending, as it must be, and the entry
/// function is transposed onto it here so that no caller has to know which of
/// its two variables came first.
fn add_binary<F>(
    builder: &mut CfnBuilder,
    model: &Model<'_>,
    first: VarId,
    second: VarId,
    entry: F,
) -> Result<(), BuildError>
where
    F: Fn(Option<&Name>, Option<&Name>) -> Cost,
{
    debug_assert_ne!(first, second, "a binary scope needs two distinct variables");
    let (low, high) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    let low_values = model.values(low);
    let high_values = model.values(high);
    let transposed = first != low;

    let mut table = Vec::with_capacity((low_values.len() + 1) * (high_values.len() + 1));
    for low_value in slots(low_values) {
        for high_value in slots(high_values) {
            let (left, right) = if transposed {
                (high_value, low_value)
            } else {
                (low_value, high_value)
            };
            table.push(entry(left, right));
        }
    }
    builder.add_function(&[low, high], table)?;
    Ok(())
}

/// One variable's values in slot order, with `⊥` last.
fn slots(values: &[Name]) -> impl Iterator<Item = Option<&Name>> {
    values.iter().map(Some).chain(std::iter::once(None))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::quality::reference_quality;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use crate::solve::{Assignment, DEFAULT_MEM_BYTES, ValId};
    use panproto_schema::{HyperEdge, Protocol, RecursionPoint, SchemaBuilder, Span, Variant};
    use std::collections::HashMap;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec![
                "object".into(),
                "string".into(),
                "integer".into(),
                "union".into(),
            ],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(
        vertices: &[(&str, &str)],
        edges: &[(&str, &str, &str, Option<&str>)],
    ) -> Schema {
        let protocol = test_protocol();
        let mut builder = SchemaBuilder::new(&protocol);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            builder = builder.edge(src, tgt, kind, *name).unwrap();
        }
        builder.build().unwrap()
    }

    /// A schema with no vertices at all.
    ///
    /// [`SchemaBuilder`] refuses to build one, so it is cut down from a schema
    /// that has one vertex and nothing else.
    fn empty_schema() -> Schema {
        let mut schema = build_schema(&[("root", "object")], &[]);
        schema.vertices.clear();
        schema.entries.clear();
        schema.outgoing.clear();
        schema.incoming.clear();
        schema.between.clear();
        schema
    }

    fn cfn_of(src: &Schema, tgt: &Schema) -> Cfn {
        build_cfn(
            src,
            tgt,
            &SearchOptions::default(),
            &DomainConstraints::default(),
            &NoEvidence,
            DEFAULT_WEIGHTS,
            DEFAULT_MEM_BYTES,
        )
        .unwrap()
    }

    fn var_of(cfn: &Cfn, source: &str) -> VarId {
        let position = cfn
            .variables()
            .iter()
            .position(|variable| variable.name().as_str() == source)
            .unwrap();
        VarId::new(u32::try_from(position).unwrap())
    }

    /// An assignment giving every named source vertex its named image, and `⊥`
    /// to everything else.
    fn assign(cfn: &Cfn, pairs: &[(&str, &str)]) -> Assignment {
        let mut assignment = Assignment::all_bottom(cfn.n_variables());
        for (source, target) in pairs {
            let var = var_of(cfn, source);
            let value = cfn
                .variable(var)
                .unwrap()
                .value_id(&Name::from(*target))
                .unwrap();
            assignment.set(var, value);
        }
        assignment
    }

    /// The identity assignment, which exists whenever a schema is searched
    /// against itself.
    fn identity(cfn: &Cfn) -> Assignment {
        let pairs: Vec<(String, String)> = cfn
            .variables()
            .iter()
            .map(|variable| (variable.name().to_string(), variable.name().to_string()))
            .collect();
        let borrowed: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(source, target)| (source.as_str(), target.as_str()))
            .collect();
        assign(cfn, &borrowed)
    }

    /// The reference score of a total morphism, with the edge map derived the
    /// way the old search derived it.
    fn reference_of(src: &Schema, tgt: &Schema, pairs: &[(&str, &str)]) -> f64 {
        let vertex_map: HashMap<Name, Name> = pairs
            .iter()
            .map(|(source, target)| (Name::from(*source), Name::from(*target)))
            .collect();
        assert_eq!(
            vertex_map.len(),
            src.vertices.len(),
            "the reference score is only claimed to agree on total morphisms"
        );

        let mut edge_map: HashMap<Edge, Edge> = HashMap::new();
        for edge in src.edges.keys() {
            let (Some(from), Some(to)) = (vertex_map.get(&edge.src), vertex_map.get(&edge.tgt))
            else {
                continue;
            };
            if let Some(image) = edge_image(tgt, edge, from, to) {
                edge_map.insert(edge.clone(), image.clone());
            }
        }
        reference_quality(&vertex_map, &edge_map, src, tgt, DEFAULT_WEIGHTS.as_array())
    }

    /// The reference score of the identity morphism on a schema.
    fn identity_reference(schema: &Schema) -> f64 {
        let names: Vec<String> = schema
            .vertices
            .keys()
            .map(|id| id.as_str().to_owned())
            .collect();
        let pairs: Vec<(&str, &str)> = names
            .iter()
            .map(|name| (name.as_str(), name.as_str()))
            .collect();
        reference_of(schema, schema, &pairs)
    }

    // -- the four vacuous branches -----------------------------------------

    #[test]
    fn a_source_with_no_vertices_builds_an_empty_network() {
        let src = empty_schema();
        let tgt = build_schema(&[("root", "object")], &[]);
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_variables(), 0);
        assert_eq!(cfn.n_functions(), 0);
        // Nothing to divide by anywhere, so both vacuous branches fire and both
        // contribute nothing.
        assert_eq!(cfn.c_empty(), Cost::BOT);
        let assignment = Assignment::all_bottom(0);
        assert_eq!(cfn.evaluate(&assignment), Cost::BOT);
        assert_eq!(cfn.quality_of(&assignment), 1.0);
    }

    #[test]
    fn a_source_with_no_edges_leaves_the_edge_component_constant() {
        let schema = build_schema(&[("root", "object"), ("leaf", "string")], &[]);
        let cfn = cfn_of(&schema, &schema);

        assert_eq!(cfn.n_functions(), 0, "no source edge, no binary function");
        assert_eq!(cfn.c_empty(), Cost::BOT);
        // Every component is at its best on the identity, and the edge component
        // is fixed at its best by having nothing to average.
        assert_eq!(cfn.quality_of(&identity(&cfn)), 1.0);
        assert_eq!(identity_reference(&schema), 1.0);
    }

    #[test]
    fn an_empty_prop_class_leaves_the_prop_component_constant() {
        // Every edge is unnamed, so no source vertex has a named outgoing edge.
        let schema = build_schema(
            &[("root", "object"), ("leaf", "string")],
            &[("root", "leaf", "prop", None)],
        );
        let cfn = cfn_of(&schema, &schema);

        assert_eq!(cfn.c_empty(), Cost::BOT);
        // The prop component is the constant 1.0 under both definitions, so the
        // identity still reads a perfect score under each.
        assert_eq!(cfn.quality_of(&identity(&cfn)), 1.0);
        assert_eq!(identity_reference(&schema), 1.0);
    }

    #[test]
    fn an_empty_pair_class_agrees_with_the_reference() {
        // Neither side of any mapped pair has a named outgoing edge, so the
        // reference's own pair class is empty too and both definitions read the
        // prop component as the constant 1.0.
        let schema = build_schema(
            &[("root", "object"), ("leaf", "string")],
            &[("root", "leaf", "prop", None)],
        );
        let cfn = cfn_of(&schema, &schema);

        let decomposed = cfn.quality_of(&identity(&cfn));
        let reference = identity_reference(&schema);
        assert!(
            (decomposed - reference).abs() <= 4e-8,
            "{decomposed} against {reference}"
        );
    }

    #[test]
    fn the_prop_component_is_absent_outside_the_prop_class() {
        // `leaf` has no outgoing edge at all, so it is outside `C_src` and its
        // unary cost carries no prop term; `root` is inside and carries one.
        let schema = build_schema(
            &[("root", "object"), ("leaf", "string")],
            &[("root", "leaf", "prop", Some("label"))],
        );
        let other = build_schema(
            &[("root", "object"), ("other", "string")],
            &[("root", "other", "prop", Some("other"))],
        );
        let cfn = cfn_of(&schema, &other);

        let leaf = var_of(&cfn, "leaf");
        let value = cfn
            .variable(leaf)
            .unwrap()
            .value_id(&Name::from("other"))
            .unwrap();
        let mapped = cfn.unary_cost(leaf, value).unwrap();
        let dropped = cfn.unary_cost(leaf, ValId::BOTTOM).unwrap();

        // `⊥` differs from the mapped value by the name, degree and coverage
        // terms alone. Both out-degrees are zero, so the degree term is the full
        // penalty on `⊥` and nothing on the mapped value.
        let radix = cfn.radix();
        let gap = name_dissimilarity(&Name::from("leaf"), &Name::from("other"));
        let expected = DEFAULT_WEIGHTS.name() * gap * 0.5;
        assert_eq!(mapped, Cost::packed(quality_units(expected), 0, radix));
        assert!(dropped > mapped, "dropping must cost at least as much");
        assert_eq!(dropped.drop_part(radix), 1, "one dropped vertex");
    }

    #[test]
    fn an_edge_with_no_variable_at_one_end_is_out_of_the_denominator() {
        // A source edge naming a vertex the schema no longer holds. No cost
        // function can carry it, so counting it in `|E_s|` would score it as
        // perfectly preserved and hand the assignment its whole share of the
        // edge component for nothing.
        let mut src = build_schema(
            &[("s_a", "object"), ("s_b", "object"), ("s_ghost", "object")],
            &[
                ("s_a", "s_b", "prop", Some("alpha")),
                ("s_a", "s_ghost", "prop", Some("alpha")),
            ],
        );
        src.vertices.remove(&Name::from("s_ghost"));
        let tgt = build_schema(
            &[("s_a", "object"), ("s_b", "object")],
            &[("s_a", "s_b", "prop", Some("omega"))],
        );

        let cfn = cfn_of(&src, &tgt);
        let pairs = [("s_a", "s_a"), ("s_b", "s_b")];
        let decomposed = cfn.quality_of(&assign(&cfn, &pairs));
        let reference = reference_of(&src, &tgt, &pairs);

        // A total vertex map whose pair class matches the source's, so the two
        // definitions are claimed to agree exactly.
        assert!(
            (decomposed - reference).abs() <= 4e-8,
            "{decomposed} against {reference}"
        );
        // And the surviving edge is a rename, so neither reads a perfect score.
        assert!(decomposed < 1.0, "the surviving edge is unpreserved");
    }

    // -- the edge image -----------------------------------------------------

    /// The two-stage find `build_morphism_weighted` performs, written out again
    /// so the assertion compares two independent expressions of it.
    fn two_stage_find<'t>(tgt: &'t Schema, edge: &Edge, a: &Name, b: &Name) -> Option<&'t Edge> {
        let candidates = tgt.edges_between(a, b);
        candidates
            .iter()
            .find(|te| te.kind == edge.kind && te.name == edge.name)
            .or_else(|| candidates.iter().find(|te| te.kind == edge.kind))
    }

    #[test]
    fn edge_image_reproduces_the_two_stage_find() {
        // Two parallel edges of one kind under different names, plus a third of
        // another kind, so both stages of the find have something to choose
        // between.
        let src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[
                ("a", "b", "prop", Some("text")),
                ("a", "b", "prop", Some("body")),
                ("a", "b", "item", Some("text")),
            ],
        );
        let tgt = build_schema(
            &[("x", "object"), ("y", "string")],
            &[
                ("x", "y", "prop", Some("body")),
                ("x", "y", "prop", Some("caption")),
                ("x", "y", "item", Some("other")),
            ],
        );

        let images: Vec<(Name, Name)> = vec![
            (Name::from("x"), Name::from("y")),
            (Name::from("y"), Name::from("x")),
            (Name::from("x"), Name::from("x")),
        ];

        for edge in src.edges.keys() {
            for (a, b) in &images {
                assert_eq!(
                    edge_image(&tgt, edge, a, b),
                    two_stage_find(&tgt, edge, a, b),
                    "edge {edge:?} between {a} and {b}"
                );
            }
        }
    }

    #[test]
    fn edge_match_reads_the_stage_the_find_stopped_at() {
        let src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[
                ("a", "b", "prop", Some("text")),
                ("a", "b", "prop", Some("body")),
            ],
        );
        let tgt = build_schema(
            &[("x", "object"), ("y", "string")],
            &[("x", "y", "prop", Some("body"))],
        );
        let x = Name::from("x");
        let y = Name::from("y");

        let text = src
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("text"))
            .unwrap();
        let body = src
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("body"))
            .unwrap();

        assert_eq!(edge_match(&tgt, body, &x, &y), EdgeMatch::Named);
        assert_eq!(edge_match(&tgt, text, &x, &y), EdgeMatch::KindOnly);
        assert_eq!(edge_match(&tgt, text, &y, &x), EdgeMatch::Absent);
    }

    #[test]
    fn parallel_source_edges_fold_into_one_binary_function() {
        let src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[
                ("a", "b", "prop", Some("text")),
                ("a", "b", "prop", Some("body")),
            ],
        );
        let tgt = build_schema(
            &[("x", "object"), ("y", "string")],
            &[("x", "y", "prop", Some("body"))],
        );
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(
            cfn.n_functions(),
            1,
            "two parallel edges share one scope, hence one function"
        );
        let mut scope = vec![var_of(&cfn, "a"), var_of(&cfn, "b")];
        scope.sort_unstable();
        assert!(cfn.function_for(&scope).is_some());
    }

    // -- naturality ---------------------------------------------------------

    #[test]
    fn naturality_is_top_without_a_kind_compatible_target_edge() {
        // The target's edge runs the other way, so mapping both endpoints leaves
        // the source edge nothing to land on.
        let src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[("a", "b", "prop", Some("label"))],
        );
        let tgt = build_schema(
            &[("x", "object"), ("y", "string")],
            &[("y", "x", "prop", Some("label"))],
        );
        let cfn = cfn_of(&src, &tgt);

        let violation = assign(&cfn, &[("a", "x"), ("b", "y")]);
        assert_eq!(cfn.evaluate(&violation), Cost::TOP_SENTINEL);

        // Dropping either endpoint is feasible: the edge leaves the apex with it.
        let dropped = assign(&cfn, &[("a", "x")]);
        assert!(cfn.evaluate(&dropped) < Cost::TOP_SENTINEL);
        assert!(cfn.evaluate(&Assignment::all_bottom(cfn.n_variables())) < Cost::TOP_SENTINEL);
    }

    #[test]
    fn a_self_loop_folds_into_the_unary_table() {
        let src = build_schema(&[("a", "object")], &[("a", "a", "prop", Some("next"))]);
        let tgt = build_schema(&[("x", "object")], &[("x", "x", "prop", Some("next"))]);
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_functions(), 0, "a self-loop is not a binary function");
        let a = var_of(&cfn, "a");
        let x = cfn.variable(a).unwrap().value_id(&Name::from("x")).unwrap();
        // The loop is preserved by name, so it adds nothing to the mapped slot.
        let mapped = cfn.unary_cost(a, x).unwrap();
        let dropped = cfn.unary_cost(a, ValId::BOTTOM).unwrap();
        assert!(dropped > mapped);
    }

    #[test]
    fn a_self_loop_without_an_image_is_top() {
        let src = build_schema(&[("a", "object")], &[("a", "a", "prop", Some("next"))]);
        let tgt = build_schema(&[("x", "object")], &[]);
        let cfn = cfn_of(&src, &tgt);

        let a = var_of(&cfn, "a");
        let x = cfn.variable(a).unwrap().value_id(&Name::from("x")).unwrap();
        assert_eq!(cfn.unary_cost(a, x).unwrap(), Cost::TOP_SENTINEL);
        assert!(cfn.unary_cost(a, ValId::BOTTOM).unwrap() < Cost::TOP_SENTINEL);
    }

    // -- kind compatibility -------------------------------------------------

    #[test]
    fn kind_incompatibility_keeps_a_target_out_of_the_domain() {
        let src = build_schema(&[("a", "object"), ("b", "string")], &[]);
        let tgt = build_schema(&[("x", "object"), ("y", "integer")], &[]);
        let cfn = cfn_of(&src, &tgt);

        let a = var_of(&cfn, "a");
        let b = var_of(&cfn, "b");
        assert_eq!(cfn.variable(a).unwrap().values(), &[Name::from("x")]);
        assert!(
            cfn.variable(b).unwrap().values().is_empty(),
            "no string vertex in the target, so `⊥` alone"
        );
        assert_eq!(cfn.domain(b).unwrap().len(), 1);
        assert!(cfn.domain(b).unwrap().contains(ValId::BOTTOM));
    }

    // -- the five structural constraints ------------------------------------

    #[test]
    fn a_required_edge_forbids_dropping_its_endpoint() {
        let mut src = build_schema(
            &[("a", "object"), ("b", "string")],
            &[("a", "b", "prop", Some("label"))],
        );
        let required = src.edges.keys().next().unwrap().clone();
        src.required.insert(Name::from("a"), vec![required]);
        let tgt = src.clone();
        let cfn = cfn_of(&src, &tgt);

        // The naturality function and the required-edge constraint share the
        // scope `{a, b}`, so they are one function rather than two.
        assert_eq!(cfn.n_functions(), 1);

        // `a` survives while `b` does not: the required edge would be dropped.
        let violation = assign(&cfn, &[("a", "a")]);
        assert_eq!(cfn.evaluate(&violation), Cost::TOP_SENTINEL);
        // Both surviving, and neither surviving, are both fine.
        assert!(cfn.evaluate(&assign(&cfn, &[("a", "a"), ("b", "b")])) < Cost::TOP_SENTINEL);
        assert!(cfn.evaluate(&assign(&cfn, &[("b", "b")])) < Cost::TOP_SENTINEL);
    }

    #[test]
    fn a_variant_member_cannot_be_dropped_while_its_coproduct_survives() {
        let mut src = build_schema(
            &[("choice", "union"), ("member", "string")],
            &[("choice", "member", "variant", Some("member"))],
        );
        src.variants.insert(
            Name::from("choice"),
            vec![Variant {
                id: Name::from("member"),
                parent_vertex: Name::from("choice"),
                tag: None,
            }],
        );
        let tgt = src.clone();
        let cfn = cfn_of(&src, &tgt);

        let violation = assign(&cfn, &[("choice", "choice")]);
        assert_eq!(cfn.evaluate(&violation), Cost::TOP_SENTINEL);
        assert!(
            cfn.evaluate(&assign(&cfn, &[("choice", "choice"), ("member", "member")]))
                < Cost::TOP_SENTINEL
        );
    }

    #[test]
    fn a_recursion_point_cannot_outlive_its_target() {
        // No edge joins the marker to its target, so this constraint adds a
        // primal graph edge the schema's own edges do not carry.
        let mut src = build_schema(&[("mu", "object"), ("body", "object")], &[]);
        src.recursion_points.insert(
            Name::from("mu"),
            RecursionPoint {
                mu_id: Name::from("mu"),
                target_vertex: Name::from("body"),
            },
        );
        let tgt = src.clone();
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_functions(), 1, "a constraint with no schema edge");
        let violation = assign(&cfn, &[("mu", "mu")]);
        assert_eq!(cfn.evaluate(&violation), Cost::TOP_SENTINEL);
        // The marker may be dropped while its target survives: the recursion
        // point goes with the marker.
        assert!(cfn.evaluate(&assign(&cfn, &[("body", "body")])) < Cost::TOP_SENTINEL);
    }

    #[test]
    fn a_schema_span_keeps_both_endpoints_or_neither() {
        let mut src = build_schema(&[("left", "object"), ("right", "object")], &[]);
        src.spans.insert(
            Name::from("correspondence"),
            Span {
                id: Name::from("correspondence"),
                left: Name::from("left"),
                right: Name::from("right"),
            },
        );
        let tgt = src.clone();
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(
            cfn.evaluate(&assign(&cfn, &[("left", "left")])),
            Cost::TOP_SENTINEL
        );
        assert_eq!(
            cfn.evaluate(&assign(&cfn, &[("right", "right")])),
            Cost::TOP_SENTINEL
        );
        assert!(
            cfn.evaluate(&assign(&cfn, &[("left", "left"), ("right", "right")]))
                < Cost::TOP_SENTINEL
        );
        assert!(cfn.evaluate(&Assignment::all_bottom(cfn.n_variables())) < Cost::TOP_SENTINEL);
    }

    #[test]
    fn a_hyper_edge_signature_is_kept_whole_or_dropped_whole() {
        let mut src = build_schema(
            &[("parent", "object"), ("one", "string"), ("two", "string")],
            &[],
        );
        let mut signature = HashMap::new();
        signature.insert(Name::from("parent"), Name::from("parent"));
        signature.insert(Name::from("first"), Name::from("one"));
        signature.insert(Name::from("second"), Name::from("two"));
        src.hyper_edges.insert(
            Name::from("triple"),
            HyperEdge {
                id: Name::from("triple"),
                kind: Name::from("record"),
                signature,
                parent_label: Name::from("parent"),
            },
        );
        let tgt = src.clone();
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_functions(), 3, "a clique over three members");
        let partial = assign(&cfn, &[("parent", "parent"), ("one", "one")]);
        assert_eq!(cfn.evaluate(&partial), Cost::TOP_SENTINEL);

        let whole = assign(
            &cfn,
            &[("parent", "parent"), ("one", "one"), ("two", "two")],
        );
        assert!(cfn.evaluate(&whole) < Cost::TOP_SENTINEL);
        assert!(cfn.evaluate(&Assignment::all_bottom(cfn.n_variables())) < Cost::TOP_SENTINEL);
    }

    // -- domain constraints -------------------------------------------------

    #[test]
    fn an_excluded_source_is_forced_to_bottom() {
        let schema = build_schema(
            &[("root", "object"), ("leaf", "string")],
            &[("root", "leaf", "prop", Some("label"))],
        );
        let mut constraints = DomainConstraints::default();
        constraints.excluded_sources.insert(Name::from("leaf"));

        let cfn = build_cfn(
            &schema,
            &schema,
            &SearchOptions::default(),
            &constraints,
            &NoEvidence,
            DEFAULT_WEIGHTS,
            DEFAULT_MEM_BYTES,
        )
        .unwrap();

        // The variable is still there, which keeps the variable set a function of
        // the source alone; it just has nothing but `⊥` to take.
        assert_eq!(cfn.n_variables(), 2);
        let leaf = var_of(&cfn, "leaf");
        assert!(cfn.variable(leaf).unwrap().values().is_empty());
        assert_eq!(cfn.domain(leaf).unwrap().only(), Some(ValId::BOTTOM));

        // Every feasible assignment drops it, so every cost carries one drop.
        let best = assign(&cfn, &[("root", "root")]);
        assert_eq!(cfn.evaluate(&best).drop_part(cfn.radix()), 1);
    }

    #[test]
    fn restricted_domains_and_excluded_targets_cut_the_domain() {
        let src = build_schema(&[("a", "object")], &[]);
        let tgt = build_schema(&[("x", "object"), ("y", "object"), ("z", "object")], &[]);

        let mut constraints = DomainConstraints::default();
        constraints
            .restricted_domains
            .insert(Name::from("a"), vec![Name::from("x"), Name::from("y")]);
        constraints.excluded_targets.insert(Name::from("y"));

        let cfn = build_cfn(
            &src,
            &tgt,
            &SearchOptions::default(),
            &constraints,
            &NoEvidence,
            DEFAULT_WEIGHTS,
            DEFAULT_MEM_BYTES,
        )
        .unwrap();

        let a = var_of(&cfn, "a");
        assert_eq!(cfn.variable(a).unwrap().values(), &[Name::from("x")]);
    }

    #[test]
    fn a_hard_pin_collapses_the_domain_and_an_incompatible_one_empties_it() {
        let src = build_schema(&[("a", "object"), ("b", "object")], &[]);
        let tgt = build_schema(&[("x", "object"), ("y", "object"), ("n", "integer")], &[]);

        let mut opts = SearchOptions::default();
        opts.hard_pins.insert(Name::from("a"), Name::from("y"));
        opts.hard_pins.insert(Name::from("b"), Name::from("n"));

        let cfn = build_cfn(
            &src,
            &tgt,
            &opts,
            &DomainConstraints::default(),
            &NoEvidence,
            DEFAULT_WEIGHTS,
            DEFAULT_MEM_BYTES,
        )
        .unwrap();

        assert_eq!(
            cfn.variable(var_of(&cfn, "a")).unwrap().values(),
            &[Name::from("y")]
        );
        assert!(
            cfn.variable(var_of(&cfn, "b")).unwrap().values().is_empty(),
            "a kind-incompatible pin drops the vertex rather than failing"
        );
    }

    // -- evidence -----------------------------------------------------------

    struct FixedEvidence(f64);

    impl Evidence for FixedEvidence {
        fn confidence(&self, _source: &Name, _target: &Name) -> f64 {
            self.0
        }
    }

    #[test]
    fn evidence_outside_the_unit_interval_is_rejected() {
        let schema = build_schema(&[("root", "object")], &[]);
        let weights = CostWeights::new(0.25, 0.25, 0.30, 0.20, 0.10).unwrap();

        for confidence in [-0.5, 1.5, f64::NAN] {
            let error = build_cfn(
                &schema,
                &schema,
                &SearchOptions::default(),
                &DomainConstraints::default(),
                &FixedEvidence(confidence),
                weights,
                DEFAULT_MEM_BYTES,
            )
            .unwrap_err();
            assert!(matches!(error, BuildError::EvidenceOutOfRange { .. }));
        }
    }

    #[test]
    fn evidence_lowers_the_cost_it_supports_and_never_reaches_top() {
        let src = build_schema(&[("a", "object")], &[]);
        let tgt = build_schema(&[("x", "object")], &[]);
        let weights = CostWeights::new(0.25, 0.25, 0.30, 0.20, 0.10).unwrap();

        let build = |confidence: f64| {
            build_cfn(
                &src,
                &tgt,
                &SearchOptions::default(),
                &DomainConstraints::default(),
                &FixedEvidence(confidence),
                weights,
                DEFAULT_MEM_BYTES,
            )
            .unwrap()
        };

        let without = build(0.0);
        let with = build(1.0);
        let a = VarId::new(0);
        let x = without
            .variable(a)
            .unwrap()
            .value_id(&Name::from("x"))
            .unwrap();

        assert!(with.unary_cost(a, x).unwrap() < without.unary_cost(a, x).unwrap());
        assert!(without.unary_cost(a, x).unwrap() < Cost::TOP_SENTINEL);
        // The feasible set is unchanged: nothing the evidence touches is `⊤`.
        assert!(with.evaluate(&Assignment::all_bottom(1)) < Cost::TOP_SENTINEL);
        assert!(without.evaluate(&Assignment::all_bottom(1)) < Cost::TOP_SENTINEL);
    }

    // -- agreement with the reference ---------------------------------------

    #[test]
    fn the_identity_reads_a_perfect_score_under_both_definitions() {
        let schema = build_schema(
            &[
                ("root", "object"),
                ("root.text", "string"),
                ("root.count", "integer"),
            ],
            &[
                ("root", "root.text", "prop", Some("text")),
                ("root", "root.count", "prop", Some("count")),
            ],
        );
        let cfn = cfn_of(&schema, &schema);
        let decomposed = cfn.quality_of(&identity(&cfn));
        let reference = identity_reference(&schema);

        assert!(
            (decomposed - reference).abs() <= 4e-8,
            "{decomposed} against {reference}"
        );
        assert_eq!(cfn.evaluate(&identity(&cfn)).drop_part(cfn.radix()), 0);
    }

    #[test]
    fn a_renaming_total_morphism_agrees_with_the_reference() {
        // `root` has a named outgoing edge and `root.text` has none, so the
        // prop class is `{root}`; and the only pair the reference counts is
        // `(root, top)`, since neither `root.text` nor `top.body` has a named
        // outgoing edge. The two normalisers coincide, so the two definitions
        // must agree to the rounding bound.
        let src = build_schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", Some("text"))],
        );
        let tgt = build_schema(
            &[("top", "object"), ("top.body", "string")],
            &[("top", "top.body", "prop", Some("body"))],
        );
        let cfn = cfn_of(&src, &tgt);

        let pairs = [("root", "top"), ("root.text", "top.body")];
        let decomposed = cfn.quality_of(&assign(&cfn, &pairs));
        let reference = reference_of(&src, &tgt, &pairs);

        assert!(
            (decomposed - reference).abs() <= 4e-8,
            "{decomposed} against {reference}"
        );
        assert!(decomposed < 1.0, "a rename is not a perfect match");
    }

    #[test]
    fn the_decomposition_dominates_the_reference_when_the_normalisers_differ() {
        // `leaf` has no named outgoing edge, so it is outside the prop class,
        // but its image `mid` has one, so the reference counts the pair and
        // scores it Jaccard zero. The reference therefore averages over two
        // pairs where the decomposition averages over one, and the
        // decomposition reads the higher quality.
        let src = build_schema(
            &[("root", "object"), ("leaf", "object")],
            &[("root", "leaf", "prop", Some("a"))],
        );
        let tgt = build_schema(
            &[("top", "object"), ("mid", "object"), ("deep", "object")],
            &[
                ("top", "mid", "prop", Some("a")),
                ("mid", "deep", "prop", Some("z")),
            ],
        );
        let cfn = cfn_of(&src, &tgt);

        let pairs = [("root", "top"), ("leaf", "mid")];
        let decomposed = cfn.quality_of(&assign(&cfn, &pairs));
        let reference = reference_of(&src, &tgt, &pairs);

        assert!(
            decomposed >= reference - 4e-8,
            "the decomposition must dominate: {decomposed} against {reference}"
        );
        assert!(
            decomposed - reference > 1e-3,
            "the two normalisers differ here, so the gap should be visible: \
             {decomposed} against {reference}"
        );
    }

    #[test]
    fn dropping_a_vertex_costs_strictly_more_than_mapping_it_well() {
        let schema = build_schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", Some("text"))],
        );
        let cfn = cfn_of(&schema, &schema);

        let total = cfn.evaluate(&identity(&cfn));
        let partial = cfn.evaluate(&assign(&cfn, &[("root", "root")]));
        assert!(total < partial, "{total:?} against {partial:?}");
        assert!(
            cfn.quality_of(&identity(&cfn)) > cfn.quality_of(&assign(&cfn, &[("root", "root")]))
        );
    }

    #[test]
    fn the_all_bottom_assignment_is_always_feasible() {
        let mut src = build_schema(
            &[("mu", "object"), ("body", "object"), ("leaf", "string")],
            &[("body", "leaf", "prop", Some("label"))],
        );
        src.recursion_points.insert(
            Name::from("mu"),
            RecursionPoint {
                mu_id: Name::from("mu"),
                target_vertex: Name::from("body"),
            },
        );
        src.spans.insert(
            Name::from("pair"),
            Span {
                id: Name::from("pair"),
                left: Name::from("mu"),
                right: Name::from("leaf"),
            },
        );
        let required = src.edges.keys().next().unwrap().clone();
        src.required.insert(Name::from("body"), vec![required]);

        // A target sharing not one kind with the source.
        let tgt = build_schema(&[("n", "integer")], &[]);
        let cfn = cfn_of(&src, &tgt);

        let nothing = Assignment::all_bottom(cfn.n_variables());
        assert!(cfn.evaluate(&nothing) < Cost::TOP_SENTINEL);
        assert_eq!(
            cfn.evaluate(&nothing).drop_part(cfn.radix()),
            u64::try_from(cfn.n_variables()).unwrap()
        );
    }

    #[test]
    fn the_network_is_a_function_of_the_schema_pair() {
        let src = build_schema(
            &[("root", "object"), ("a", "string"), ("b", "string")],
            &[
                ("root", "a", "prop", Some("a")),
                ("root", "b", "prop", Some("b")),
                ("root", "a", "item", Some("a")),
            ],
        );
        let tgt = build_schema(
            &[("top", "object"), ("x", "string"), ("y", "string")],
            &[
                ("top", "x", "prop", Some("a")),
                ("top", "y", "prop", Some("z")),
                ("top", "x", "item", Some("a")),
            ],
        );

        let first = cfn_of(&src, &tgt);
        for _ in 0..8 {
            assert_eq!(cfn_of(&src, &tgt), first);
        }
    }

    #[test]
    fn a_vertex_offered_no_target_still_has_a_variable() {
        let src = build_schema(&[("a", "object"), ("b", "string")], &[]);
        let tgt = build_schema(&[("x", "object")], &[]);
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_variables(), 2);
        let b = var_of(&cfn, "b");
        assert_eq!(cfn.domain(b).unwrap().only(), Some(ValId::BOTTOM));
    }

    #[test]
    fn a_kind_mismatched_vertex_map_is_unreachable_rather_than_top() {
        // The kind constraint is the absence of the value, so there is no entry
        // to evaluate rather than a `⊤`-valued one.
        let src = build_schema(&[("a", "object")], &[]);
        let tgt = build_schema(&[("n", "integer")], &[]);
        let cfn = cfn_of(&src, &tgt);

        let a = var_of(&cfn, "a");
        assert!(
            cfn.variable(a)
                .unwrap()
                .value_id(&Name::from("n"))
                .is_none()
        );
    }

    // -- the components in isolation ----------------------------------------

    #[test]
    fn the_dissimilarity_helpers_match_the_reference_arithmetic() {
        assert_eq!(
            name_dissimilarity(&Name::from("abc"), &Name::from("abc")),
            0.0
        );
        assert_eq!(
            name_dissimilarity(&Name::from("abc"), &Name::from("abd")),
            1.0 / 3.0
        );
        assert_eq!(name_dissimilarity(&Name::from(""), &Name::from("")), 0.0);

        assert_eq!(degree_dissimilarity(0, 0), 0.0);
        assert_eq!(degree_dissimilarity(2, 2), 0.0);
        assert_eq!(degree_dissimilarity(1, 3), 2.0 / 3.0);

        let left: FxHashSet<&str> = ["a", "b"].into_iter().collect();
        let right: FxHashSet<&str> = ["b", "c"].into_iter().collect();
        assert_eq!(jaccard_dissimilarity(&left, &right), 2.0 / 3.0);
        assert_eq!(jaccard_dissimilarity(&left, &left), 0.0);
    }

    #[test]
    fn a_vertex_carries_its_own_kind_into_the_domain() {
        // Two source vertices of the same kind see the same candidate list, in
        // the same ascending order.
        let src = build_schema(&[("a", "object"), ("b", "object")], &[]);
        let tgt = build_schema(&[("z", "object"), ("m", "object")], &[]);
        let cfn = cfn_of(&src, &tgt);

        let expected = vec![Name::from("m"), Name::from("z")];
        assert_eq!(cfn.variable(var_of(&cfn, "a")).unwrap().values(), expected);
        assert_eq!(cfn.variable(var_of(&cfn, "b")).unwrap().values(), expected);
    }

    #[test]
    fn a_dangling_constraint_reference_is_skipped() {
        // A span naming a vertex the schema does not have has no variable to
        // constrain, so it contributes no cost function rather than failing.
        let mut src = build_schema(&[("left", "object")], &[]);
        src.spans.insert(
            Name::from("dangling"),
            Span {
                id: Name::from("dangling"),
                left: Name::from("left"),
                right: Name::from("absent"),
            },
        );
        let tgt = build_schema(&[("left", "object")], &[]);
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_functions(), 0);
        assert!(cfn.evaluate(&assign(&cfn, &[("left", "left")])) < Cost::TOP_SENTINEL);
    }

    #[test]
    fn a_span_on_one_vertex_is_vacuous() {
        let mut src = build_schema(&[("only", "object")], &[]);
        src.spans.insert(
            Name::from("loop"),
            Span {
                id: Name::from("loop"),
                left: Name::from("only"),
                right: Name::from("only"),
            },
        );
        let tgt = src.clone();
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_functions(), 0);
        assert!(cfn.evaluate(&Assignment::all_bottom(1)) < Cost::TOP_SENTINEL);
        assert!(cfn.evaluate(&assign(&cfn, &[("only", "only")])) < Cost::TOP_SENTINEL);
    }

    #[test]
    fn a_vertex_added_outside_the_builder_still_gets_a_variable() {
        // Guards the assumption that the variable set is read from
        // `src.vertices` rather than from anything the builder happened to index.
        let mut src = build_schema(&[("a", "object")], &[]);
        let donor = build_schema(&[("b", "object")], &[]);
        let vertex = donor.vertices.get(&Name::from("b")).unwrap().clone();
        src.vertices.insert(Name::from("b"), vertex);
        let tgt = build_schema(&[("x", "object")], &[]);
        let cfn = cfn_of(&src, &tgt);

        assert_eq!(cfn.n_variables(), 2);
        assert_eq!(cfn.variables()[0].name().as_str(), "a");
        assert_eq!(cfn.variables()[1].name().as_str(), "b");
    }
}
