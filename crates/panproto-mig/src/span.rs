//! The span the schema morphism search returns, and the obligations it carries.
//!
//! A search over an ordered schema pair returns a span
//!
//! ```text
//!            A
//!          ↙   ↘
//!        ℓ       r
//!      ↙           ↘
//!   src             tgt
//! ```
//!
//! whose apex `A` is the sub-schema of `src` induced on the vertices the search
//! gave a target. The left leg is an inclusion, hence a mono; the right leg is a
//! general schema morphism, and is a mono exactly when the search was run
//! injectively.
//!
//! # Why the span is the result and the total morphism is the special case
//!
//! A total morphism `src → tgt` exists only when every source vertex has an
//! image, which on the measured schema corpus is the minority of pairs. A span
//! always exists: the assignment leaving every vertex out is feasible, so the
//! search never refuses, and the answer to "these two schemas have nothing in
//! common" is an empty apex rather than a failure.
//!
//! The total morphism is recovered as the degenerate case. A span is
//! isomorphic to a morphism precisely when its left leg is invertible (Johnson
//! & Rosebrugh, "Spans of lenses", EDBT/ICDT Workshops 2014, §1), and since the
//! left leg here is an inclusion, invertibility is surjectivity.
//! [`SchemaSpan::is_total`] tests it and [`SchemaSpan::as_total_morphism`]
//! returns the older shape.
//!
//! On the default and injective paths there is no second code path: the total
//! search is this search with `⊥` removed from every domain. The iso path is
//! the exception, and deliberately so. Its network needs `⊥` feasible, because
//! every reward is measured against the cost of dropping everything, so
//! [`find_morphisms`](crate::hom_search::find_morphisms) routes an iso request
//! to a separate entry point that checks totality on the answer instead.
//!
//! The two therefore answer different questions on that path, and neither is
//! wrong. `find_span` with [`SearchOptions::iso`] computes a maximum common
//! induced sub-schema, so `is_total` on its result says *the whole source
//! embeds into the target*; `find_morphisms` with the same options asks for an
//! isomorphism `src ≅ tgt` and returns nothing unless the two schemas are the
//! same size. A source that embeds into a strictly larger target satisfies the
//! first and not the second.
//!
//! # What "maximum" means
//!
//! The returned span minimises the packed lexicographic cost
//! `(quality_cost, drop_count)`: lowest quality cost first, then fewest dropped
//! source vertices, which is to say the largest apex. Maximising the apex is not
//! a cosmetic tie-break. The complement of the apex in the source is exactly the
//! complement of the view in the sense of Bancilhon & Spyratos ("Update
//! semantics of relational views", ACM TODS 6(4):557-575, 1981), whose Theorem
//! 6.1 says a smaller complement admits strictly more translatable updates. The
//! objective and the theory agree on which apex to prefer.
//!
//! Saturation follows from that ordering. Every cost is non-negative and the
//! drop unit is positive, so any extension of the apex that does not raise the
//! quality cost strictly lowers the total, and the returned span therefore
//! admits no cost-preserving extension. That is the "no shortfall" half of the
//! minimal-apex condition; "no junk" holds by construction, since the apex is
//! induced rather than assembled.
//!
//! # The apex is not the symmetric-lens apex
//!
//! [`SchemaSpan::pushout`] merges the two schemas along the apex. Writing
//! `Mod(S) = [S, Set]` for the category of instances of a schema, and using that
//! `Fun(−, Set)` sends colimits to limits,
//!
//! ```text
//! Mod(src ⊔_A tgt)  ≅  Mod(src) ×_{Mod(A)} Mod(tgt)
//! ```
//!
//! so restriction along the two pushout injections is a span of Gets whose apex
//! is the category of consistent pairs. That is the "consistent triples" object
//! of Johnson & Rosebrugh's Proposition 9, and it is what a symmetric lens has
//! for an apex. **It is a different object from `A`.** `A` is the shared
//! *schema*; `Mod(src ⊔_A tgt)` is the space of *consistent instances*. They are
//! related by pushout followed by `Mod`, and this module never conflates them:
//! it returns the span of schema morphisms and derives the merge on demand.
//!
//! Only the iso path yields a span whose right leg is a mono, which is what
//! building a symmetric lens from the span needs, so a caller wanting one runs
//! the search with [`SearchOptions::iso`].
//!
//! # Equivalence
//!
//! Classical span isomorphism, an iso of apices commuting with both legs,
//! collapses here. Because the left leg is an inclusion, the apex is determined
//! by its vertex set, so span equivalence classes are in bijection with pairs
//! `(A ⊆ V_src, r : A → V_tgt)`. No quotient is needed and no
//! graph-isomorphism test is run: content identity is
//! [`SpanCertificate::apex_digest`] together with the two leg maps.
//!
//! The lens-level equivalence `≡_G` of Johnson & Rosebrugh, the zig-zag closure
//! of span morphisms whose Get is a split epi, is **not** attempted at the
//! schema layer. It is a statement about lenses between apices, and this module
//! has no lens structure to check it with.

use std::collections::HashMap;

use panproto_gat::{Name, Theory};
use panproto_schema::{
    Edge, Protocol, Schema, SchemaMorphism, SchemaOverlap, canonical_digest, induce_on_vertices,
    schema_pushout,
};
use rustc_hash::FxHashSet;

use crate::error::SpanError;
use crate::existence::{ExistenceReport, check_existence};
use crate::hom_search::{DomainConstraints, FoundMorphism, SearchOptions};
use crate::migration::Migration;
use crate::schema_theory::check_migration_morphism;
use crate::solve::build::{Evidence, NoEvidence, build_cfn, edge_image};
use crate::solve::cost::{COST_SCALE, Cost, CostWeights, DEFAULT_WEIGHTS};
use crate::solve::{
    Assignment, Cfn, LimitKind, SearchBudget, SolveOutcome, SolverPath, all_optima, dispatch_plan,
    eliminate, solve, solve_iso, solve_monic,
};

/// The cost scale as a float, for turning an integer cost into a quality.
///
/// Pinned against [`COST_SCALE`] by a unit test rather than derived from it,
/// because `u64 as f64` is a lossy cast and this constant is exact.
const COST_SCALE_FLOAT: f64 = 1.0e9;

/// How many optima [`SpanSearch::optima`] enumerates when asked for all of them.
///
/// The count of argmins can be large: on the measured corpus the majority of
/// pairs admit the full Cartesian hom-set, and a genuine quality tie across it
/// is possible on schemas whose vertex names carry no signal. The cap is a
/// guard against handing back a list nothing can use, not a statement about how
/// many optima exist.
pub const DEFAULT_OPTIMA_CAP: usize = 1024;

// ---------------------------------------------------------------------------
// The span
// ---------------------------------------------------------------------------

/// A span of schema morphisms `src ←ℓ─ apex ─r→ tgt`.
///
/// The apex is the sub-schema of `src` induced on the vertices the search
/// assigned a target. The left leg is therefore an inclusion, hence a mono; the
/// right leg is a general schema morphism, and is a mono exactly when the search
/// was run with [`SearchOptions::monic`] or [`SearchOptions::iso`].
///
/// The module docs state what the span is, what "maximum" means, and how the
/// apex relates to the apex of a symmetric lens.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SchemaSpan {
    /// The apex `A`, well formed per [`induce`](fn@panproto_schema::induce).
    pub apex: Schema,

    /// `ℓ : A → src`.
    ///
    /// An inclusion: apex vertex identifiers *are* source vertex identifiers,
    /// so `vertex_map` is the identity on the apex's vertices and `edge_map` the
    /// identity on its edges. That is what makes
    /// [`LegShape::left_is_mono`] true by construction and what makes
    /// span equality decidable without a graph-isomorphism test.
    pub left: Migration,

    /// `r : A → tgt`.
    ///
    /// The vertex map is the search's assignment restricted to the apex, and the
    /// edge map sends each apex edge to the target edge
    /// [`edge_image`] picks between the images
    /// of its endpoints, which is the same choice the objective and the
    /// naturality constraint are read off.
    pub right: Migration,

    /// `1 − quality_cost / COST_SCALE`, excluding the drop count.
    ///
    /// **A ranking signal among spans over one source schema, and nothing
    /// else.** Every denominator of the objective is fixed by `src`, so two
    /// spans over the same source are comparable and two spans over different
    /// sources are not; there is no absolute reading of this number and no
    /// threshold on it is meaningful across pairs.
    ///
    /// The anchor term is **included**, weighted as
    /// [`CostWeights::anchor`](crate::CostWeights::anchor) weights it. The
    /// shipped weight is zero, so under the default weights this reads as the
    /// four structural components and evidence steers the search without
    /// showing up in the number; a caller that raises the anchor weight is
    /// reading a quality that its own evidence contributed to, and wanting the
    /// structural reading alone must recompute it.
    ///
    /// The drop count is excluded, because what this measures is how well the
    /// covered part matches; [`Self::apex_coverage`] separately answers how much
    /// was covered.
    ///
    /// Each cost function entry was rounded to fixed point once, so this reading
    /// differs from an `f64` accumulation of the same terms by at most
    /// `(|V_src| + |E_src|) / (2 · COST_SCALE)`. A test asserting agreement with
    /// [`reference_quality`](crate::quality::reference_quality) must compute
    /// that expression rather than hard-code a constant.
    ///
    /// # The empty cases, and why none of them is a verdict
    ///
    /// An empty apex costs every source vertex the full penalty on each
    /// component that has mass, and a component has mass only when the source
    /// gives it something to measure. Name and degree are normalised per source
    /// vertex, so they always charge. The edge component is normalised per
    /// source edge and the Jaccard component per source vertex with a named
    /// outgoing edge, so a source with no edges charges neither and a source
    /// whose edges are all unnamed charges only the first. Under the default
    /// weights an empty apex therefore reads
    ///
    /// - `0.0` over a source with at least one named edge,
    /// - `0.30` over a source whose edges are all unnamed,
    /// - `0.55` over an edgeless source, and
    /// - `1.0` over an empty source, since there was nothing to pay.
    ///
    /// The first is not the rule for every non-empty source, tempting as that
    /// reading is: it is the reading of the one shape the measured schema
    /// corpus happens to contain, so no corpus test can distinguish it from the
    /// other two. `the_empty_apex_reads_its_own_scale` builds all four sources
    /// and pins all four readings.
    ///
    /// Those readings are floors rather than verdicts. Each is the worst value
    /// on its own source's scale, and the scale is narrower the less structure
    /// the source carries, so "these two schemas share nothing" is `0.0` on one
    /// source and `0.55` on another. This is why a caller ranking pairs must
    /// read [`Self::apex_coverage`] alongside this number. That is the concrete
    /// form of the rule above: this is a ranking signal among spans over one
    /// source schema, and comparing it across sources compares two different
    /// scales.
    pub quality: f64,

    /// `(lower, upper)` bracketing [`Self::quality`].
    ///
    /// Equal when [`SpanCertificate::proven_optimal`] holds. When it does not,
    /// the interval is what separates "0.4, and nothing better exists" from
    /// "0.4, and the search ran out of budget before it could rule out 0.9".
    pub quality_bounds: (f64, f64),

    /// `|apex.vertices| / |src.vertices|`, or one when the source has no
    /// vertices.
    pub apex_coverage: f64,

    /// What the construction proved about this span.
    pub certificate: SpanCertificate,
}

/// Whether a leg's edge map is injective.
///
/// An enumeration rather than a boolean because the two answers are not
/// "yes" and "not yet checked": both are measurements, and a span whose edge
/// map contracts is a real answer rather than a defective one.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EdgeImages {
    /// Distinct apex arcs have distinct images, so the leg embeds on edges.
    Distinct,

    /// Two apex arcs share an image, so the leg contracts on edges.
    ///
    /// The default, because it is the weaker claim: a `LegShape` that was never
    /// measured should not read as an embedding.
    #[default]
    Shared,
}

/// What the two legs are, as morphisms.
///
/// Split out of [`SpanCertificate`] so that neither type carries more booleans
/// than one glance can keep straight. These three answer "what shape is this
/// span"; the certificate's own flags answer "what did the construction check".
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct LegShape {
    /// Whether `ℓ` is injective on vertices and on edges. True by construction:
    /// an inclusion is a mono.
    pub left_is_mono: bool,

    /// Whether `r` is injective on vertices.
    ///
    /// This is injectivity on **vertices only**. An injective vertex map may
    /// still send two parallel source edges to one target edge, which is a
    /// homomorphism into a denser target and not an embedding.
    /// [`Self::right_edge_images`] is the other half.
    pub right_is_mono: bool,

    /// Whether `r` is injective on edges.
    ///
    /// Reported separately from [`Self::right_is_mono`] because the two come
    /// apart: an injective vertex map may still send two parallel apex arcs to
    /// one target arc, and a caller merging along the span needs to know, since
    /// a merge keyed by the target element keeps only one preimage of a
    /// collision. The iso path asks for a bijection on edges and so reports
    /// [`EdgeImages::Distinct`]; the ordinary path takes a greedy image and need
    /// not.
    pub right_edge_images: EdgeImages,

    /// Whether `ℓ` is surjective, hence an isomorphism, hence the span is a
    /// total morphism.
    pub left_is_iso: bool,
}

/// What the construction of a [`SchemaSpan`] proved about it.
///
/// Every field is a measurement taken at construction time, not a claim. Both
/// categorical obligations are recorded on **both** legs rather than assumed,
/// and a failure on either is surfaced here rather than swallowed.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct SpanCertificate {
    /// Whether the search proved its answer optimal.
    pub proven_optimal: bool,

    /// What shape the two legs are.
    pub shape: LegShape,

    /// Whether both legs passed
    /// [`check_migration_morphism`], which is
    /// functoriality on the mapped fragment: every mapped edge lands between the
    /// images of its own endpoints.
    pub legs_are_functorial: bool,

    /// What [`check_existence`] reported about `ℓ`.
    ///
    /// # Why this can fail, and why it is not a construction defect
    ///
    /// The left leg is an inclusion, so it cannot fail *functoriality*, and
    /// [`Self::legs_are_functorial`] asserts that in debug builds. Existence is
    /// a wider obligation than functoriality: several of its conditional checks
    /// read the schemas rather than the map. Reachability is the one that fires
    /// in practice. It asks whether every mapped vertex is reachable from a
    /// vertex with no incoming edge, and an apex whose every vertex sits on a
    /// cycle has no such root, so the check reports every vertex of it at risk
    /// however the leg maps them.
    ///
    /// That is a finding about the apex, not about `ℓ`, which is why it is
    /// recorded rather than panicked on. It is reported separately from
    /// [`Self::right_existence`] because the two legs have different codomains
    /// and can therefore fail different obligations, and a single flag would
    /// have said "the right leg" while reading as "the span".
    pub left_existence: ExistenceReport,

    /// What [`check_existence`] reported about `r`.
    ///
    /// The conditional obligations it can check depend on the theories the
    /// caller supplied. With no theory registry only the unconditional ones run:
    /// the vertex and edge maps landing in the schemas, and kind consistency.
    pub right_existence: ExistenceReport,

    /// Whether the apex has at least one entry point.
    ///
    /// An apex with no entries is a schema nothing can be read into. Inducing
    /// never synthesises one, so this records the fact rather than repairing it.
    pub apex_pointed: bool,

    /// The content digest of the apex, as
    /// [`canonical_digest`] computes it.
    ///
    /// Together with the two leg maps this is the span's identity.
    pub apex_digest: [u8; 32],

    /// Which algorithm produced the assignment.
    pub path: SolverPath,

    /// The source vertices in **decode** order, when the answer came from exact
    /// inference.
    ///
    /// This is the order the tie-break among equally good assignments is read
    /// in, and it is the *reverse* of the elimination order: elimination peels
    /// variables off in one direction and decoding walks back the other way.
    /// [`SpanSearch::optima`] states the rule; this reports the sequence it was
    /// applied to, so a caller can reproduce the choice instead of taking it on
    /// trust.
    ///
    /// `None` on the injective and iso paths and on any network too wide for
    /// exact inference, none of which eliminate.
    pub tie_break_order: Option<Vec<Name>>,

    /// What stopped the search, if anything did.
    pub limit_hit: Option<LimitKind>,
}

impl Default for SpanCertificate {
    /// A certificate that establishes nothing.
    ///
    /// Every claim is false, the existence report is invalid with no findings,
    /// the digest is zero, and the path is the one a network with no variables
    /// takes. It exists so that a caller can name the type; a certificate
    /// produced by the search always overwrites all of it.
    fn default() -> Self {
        Self {
            proven_optimal: false,
            shape: LegShape::default(),
            legs_are_functorial: false,
            left_existence: ExistenceReport {
                valid: false,
                errors: Vec::new(),
            },
            right_existence: ExistenceReport {
                valid: false,
                errors: Vec::new(),
            },
            apex_pointed: false,
            apex_digest: [0u8; 32],
            path: SolverPath::Eliminate { width: 0 },
            tie_break_order: None,
            limit_hit: None,
        }
    }
}

impl SchemaSpan {
    /// Whether the span is a total morphism, i.e. whether `ℓ` is surjective.
    ///
    /// A span is isomorphic to a morphism precisely when its left leg is
    /// invertible, and an inclusion is invertible exactly when it is onto. The
    /// test is `|apex.vertices| == |src.vertices| && |apex.edges| ==
    /// |src.edges|`, taken at construction against the source schema and
    /// recorded in the certificate, which is why it reads a field rather than
    /// recomputing.
    #[inline]
    #[must_use]
    pub const fn is_total(&self) -> bool {
        self.certificate.shape.left_is_iso
    }

    /// The span as a total morphism, or `None` when it is not one.
    ///
    /// This is the older result shape: the right leg's two maps and the quality.
    /// It is exactly the composite `r ∘ ℓ⁻¹`, which is defined precisely when
    /// [`Self::is_total`] holds.
    #[must_use]
    pub fn as_total_morphism(&self) -> Option<FoundMorphism> {
        self.is_total().then(|| FoundMorphism {
            vertex_map: self.right.vertex_map.clone(),
            edge_map: self.right.edge_map.clone(),
            quality: self.quality,
        })
    }

    /// The apex as the pair list [`schema_pushout`] expects.
    ///
    /// Each pair is `(source element, target element)`, matching the
    /// `(left, right)` convention of [`SchemaOverlap`], and both lists are
    /// sorted so that the overlap is a function of the span rather than of a
    /// hash seed.
    #[must_use]
    pub fn to_overlap(&self) -> SchemaOverlap {
        let mut vertex_pairs: Vec<(Name, Name)> = self
            .right
            .vertex_map
            .iter()
            .map(|(source, image)| (source.clone(), image.clone()))
            .collect();
        vertex_pairs.sort_unstable();

        let mut edge_pairs: Vec<(Edge, Edge)> = self
            .right
            .edge_map
            .iter()
            .map(|(source, image)| (source.clone(), image.clone()))
            .collect();
        edge_pairs.sort_unstable();

        SchemaOverlap {
            vertex_pairs,
            edge_pairs,
        }
    }

    /// Merge the two schemas along the apex.
    ///
    /// Returns the pushout `src ⊔_A tgt` together with the two injections. The
    /// module docs state what this object is and, in particular, that
    /// `Mod` of it, rather than the apex, is the apex of the corresponding
    /// symmetric lens.
    ///
    /// The two schemas are parameters rather than fields because a span carries
    /// its apex and its two leg maps, not copies of the schemas it was searched
    /// over. Passing anything other than the pair the span was produced from is
    /// a contract violation that shows up as a missing vertex.
    ///
    /// # The right leg must not contract
    ///
    /// The pushout is the merge of the two schemas *along* the apex, so the
    /// square it completes has to commute: an apex vertex must reach the same
    /// merged vertex through either leg. A right leg that sends two apex
    /// vertices to one target vertex makes that impossible, because
    /// [`schema_pushout`] identifies elements by a map keyed on the *right*
    /// element and a repeated right key can name only one left preimage. The
    /// result is not a cocone over this span, and nothing about it says so.
    ///
    /// A contracting right leg is an ordinary answer from the default search —
    /// a source with four string fields and a target with one gives one — so
    /// this is a precondition a caller meets rather than an impossibility.
    /// [`SearchOptions::iso`](crate::SearchOptions::iso) is the option that
    /// rules it out, and
    /// [`discover_overlap`](crate::discover_overlap) sets it for this reason.
    ///
    /// # Errors
    ///
    /// [`SpanError::ContractingRightLeg`] when the right leg is not injective on
    /// vertices, and [`SpanError::Apex`] wrapping whatever [`schema_pushout`]
    /// reports, which is
    /// [`SchemaError::VertexNotFound`](panproto_schema::SchemaError::VertexNotFound)
    /// when an overlap pair names a vertex the corresponding schema does not
    /// hold.
    pub fn pushout(
        &self,
        src: &Schema,
        tgt: &Schema,
    ) -> Result<(Schema, SchemaMorphism, SchemaMorphism), SpanError> {
        if !self.certificate.shape.right_is_mono {
            return Err(SpanError::ContractingRightLeg);
        }
        Ok(schema_pushout(src, tgt, &self.to_overlap())?)
    }

    /// The apex digest in lower-case hexadecimal.
    ///
    /// This is the identifier the two legs carry as their apex endpoint, so
    /// comparing it against `left.domain` is how a caller checks that a span was
    /// not assembled from parts.
    #[must_use]
    pub fn apex_digest_hex(&self) -> String {
        digest_hex(&self.certificate.apex_digest)
    }
}

// ---------------------------------------------------------------------------
// The search
// ---------------------------------------------------------------------------

/// A configured span search.
///
/// Everything the search reads besides the two schemas: the protocol the apex is
/// validated against, the search options, the caller's hard domain
/// restrictions, the alignment evidence, the objective's component weights, the
/// budget, and the theories the existence check reads.
///
/// [`find_span`](crate::hom_search::find_span) and
/// [`find_span_constrained`](crate::hom_search::find_span_constrained) are the
/// two common configurations of this; build one directly when you need to supply
/// evidence, a budget, or a theory registry.
///
/// # Examples
///
/// ```
/// use panproto_mig::span::SpanSearch;
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
///     .vertex("root.title", "string", None::<&str>)?
///     .edge("root", "root.title", "prop", Some("title"))?
///     .entry("root")
///     .build()?;
///
/// let span = SpanSearch::new(&protocol).run(&src, &src)?;
///
/// // A schema always maps onto itself, so the apex covers all of it and the
/// // span is a total morphism.
/// assert!(span.is_total());
/// assert!((span.apex_coverage - 1.0).abs() < f64::EPSILON);
/// assert!(span.as_total_morphism().is_some());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct SpanSearch<'a> {
    protocol: &'a Protocol,
    options: SearchOptions,
    constraints: DomainConstraints,
    evidence: &'a dyn Evidence,
    weights: CostWeights,
    budget: SearchBudget,
    theories: Option<&'a HashMap<String, Theory>>,
}

impl std::fmt::Debug for SpanSearch<'_> {
    /// The evidence table is a trait object with no `Debug` bound, so it is
    /// reported by presence rather than by content.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpanSearch")
            .field("protocol", &self.protocol.name)
            .field("options", &self.options)
            .field("constraints", &self.constraints)
            .field("weights", &self.weights)
            .field("budget", &self.budget)
            .field("theories", &self.theories.map_or(0, HashMap::len))
            .finish()
    }
}

impl<'a> SpanSearch<'a> {
    /// A search with default options, no restrictions, no evidence, the default
    /// weights, the default budget, and no theory registry.
    #[must_use]
    pub fn new(protocol: &'a Protocol) -> Self {
        Self {
            protocol,
            options: SearchOptions::default(),
            constraints: DomainConstraints::default(),
            evidence: &NoEvidence,
            weights: DEFAULT_WEIGHTS,
            budget: SearchBudget::default(),
            theories: None,
        }
    }

    /// Set the search options.
    #[must_use]
    pub fn with_options(mut self, options: SearchOptions) -> Self {
        self.options = options;
        self
    }

    /// Set the hard domain restrictions.
    ///
    /// [`DomainConstraints::scoring_weights`], when present, supersedes whatever
    /// [`Self::with_weights`] set, since a caller who states weights alongside
    /// domain restrictions is stating them for that search.
    #[must_use]
    pub fn with_constraints(mut self, constraints: DomainConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Set the alignment evidence the anchor term reads.
    #[must_use]
    pub fn with_evidence(mut self, evidence: &'a dyn Evidence) -> Self {
        self.evidence = evidence;
        self
    }

    /// Set the objective's component weights.
    #[must_use]
    pub const fn with_weights(mut self, weights: CostWeights) -> Self {
        self.weights = weights;
        self
    }

    /// Set what the search may spend.
    #[must_use]
    pub const fn with_budget(mut self, budget: SearchBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Set the theory registry the existence check reads.
    ///
    /// Without one, the existence check on the right leg runs its unconditional
    /// obligations alone: the two maps landing in the schemas, and kind
    /// consistency. Every conditional obligation is gated on a conventionally
    /// named sort being present in one of the protocol's theories, so an absent
    /// registry silently skips all of them.
    #[must_use]
    pub const fn with_theories(mut self, theories: &'a HashMap<String, Theory>) -> Self {
        self.theories = Some(theories);
        self
    }

    /// Run the search and return the optimal span.
    ///
    /// Never refuses for want of a match: the assignment leaving every source
    /// vertex out of the apex is always feasible, so a pair with nothing in
    /// common returns a span with an empty apex.
    ///
    /// # Errors
    ///
    /// [`SpanError::Build`] if the network could not be posed, [`SpanError::Iso`]
    /// if the iso path refused it, and [`SpanError::Apex`] if the induced apex is
    /// not a well-formed schema.
    ///
    /// The last is reachable, and its usual cause is an invalid input rather
    /// than a missing constraint. Inducing validates the apex against the
    /// protocol and this search validates neither of its inputs, so a source
    /// the protocol already rejects carries its own findings into every apex
    /// that keeps an offending part, and the refusal is the source's rather
    /// than the network's. Which of the two happens turns on where the optimum
    /// lands: the same invalid source is answered whenever the optimum leaves
    /// the offending part out, so this error is not a function of the source
    /// alone. A caller wanting a refusal that is should run
    /// [`validate`](fn@panproto_schema::validate) on its inputs first.
    ///
    /// A dangling reference is the one exception, and it does indict the
    /// network: the five apex hard constraints exist precisely to forbid the
    /// assignments whose apex would carry one. [`SpanError::Apex`] states which
    /// findings are which.
    ///
    /// [`SpanError::EpicIsNotASpanProperty`] if
    /// [`SearchOptions::epic`](crate::SearchOptions::epic) is set. Surjectivity
    /// is a property of a total morphism: a span's right leg is deliberately
    /// partial and the empty apex is always feasible, so requiring it would
    /// break the "never refuses" contract two paragraphs up. The flag is
    /// rejected rather than ignored, because a search that quietly drops an
    /// option a caller set answers a different question than the one asked.
    pub fn run(&self, src: &Schema, tgt: &Schema) -> Result<SchemaSpan, SpanError> {
        let cfn = self.network(src, tgt)?;
        let outcome = self.dispatch(&cfn, src, tgt)?;
        let assignment = outcome
            .best
            .clone()
            .unwrap_or_else(|| Assignment::all_bottom(cfn.n_variables()));
        self.assemble(src, tgt, &cfn, &outcome, &assignment)
    }

    /// Every span attaining the optimum, up to `limit` of them.
    ///
    /// Ties are possible and the one [`Self::run`] returns is the
    /// lexicographically smallest assignment vector in **decode** order, which
    /// is the *reverse* of the elimination order: elimination peels variables
    /// off in one direction and decoding walks back the other way, so reading
    /// the rule against the elimination order names the wrong sequence and
    /// mispredicts the winner whenever the two disagree.
    /// [`SpanCertificate::tie_break_order`] reports the sequence actually used.
    /// This enumerates the rest, for a caller wanting a different canonical
    /// choice among them.
    ///
    /// Enumeration needs the message tables of exact inference, so a search
    /// routed to branch and bound, or run injectively, yields the one span
    /// [`Self::run`] would have returned. A `limit` of zero is read as
    /// [`DEFAULT_OPTIMA_CAP`].
    ///
    /// The order eliminated under is the one [`Self::run`] would use, taken
    /// from [`dispatch_plan`], and the fallback fires exactly where
    /// [`Self::run`] would decline to eliminate. Both matter for the same
    /// reason: [`solve`] splits the network into
    /// components and chooses an order per component, so choosing one over the
    /// whole network instead settles tied variables in a different sequence and
    /// names a different member of the tie as canonical — and the budget is
    /// priced per component too, so pricing the whole network refuses
    /// enumeration on pairs [`Self::run`] eliminates exactly.
    ///
    /// # Errors
    ///
    /// As [`Self::run`].
    pub fn optima(
        &self,
        src: &Schema,
        tgt: &Schema,
        limit: usize,
    ) -> Result<Vec<SchemaSpan>, SpanError> {
        let limit = if limit == 0 {
            DEFAULT_OPTIMA_CAP
        } else {
            limit
        };
        let cfn = self.network(src, tgt)?;
        let plan = dispatch_plan(&cfn, &self.budget);
        if self.options.iso || self.options.monic || !plan.exact {
            return self.run(src, tgt).map(|span| vec![span]);
        }

        let buckets = eliminate(&cfn, &plan.order);
        let optimum = buckets.optimum();
        let outcome = SolveOutcome {
            best: None,
            lower_bound: optimum,
            upper_bound: optimum,
            proven_optimal: true,
            path: SolverPath::Eliminate { width: plan.width },
            elimination_order: Some(plan.order),
            nodes: 0,
            limit_hit: None,
            warnings: Vec::new(),
        };

        all_optima(&cfn, &buckets, limit)
            .iter()
            .map(|assignment| self.assemble(src, tgt, &cfn, &outcome, assignment))
            .collect()
    }

    /// The network this search minimises over.
    fn network(&self, src: &Schema, tgt: &Schema) -> Result<Cfn, SpanError> {
        if self.options.epic {
            return Err(SpanError::EpicIsNotASpanProperty);
        }
        Ok(build_cfn(
            src,
            tgt,
            &self.options,
            &self.constraints,
            self.evidence,
            self.constraints.scoring_weights.unwrap_or(self.weights),
            self.budget.mem_bytes,
        )?)
    }

    /// Route the network to the algorithm the options ask for.
    ///
    /// Injectivity is not a property of a network, so it is chosen by which
    /// entry point runs rather than by anything the builder encoded.
    fn dispatch(&self, cfn: &Cfn, src: &Schema, tgt: &Schema) -> Result<SolveOutcome, SpanError> {
        if self.options.iso {
            return Ok(solve_iso(cfn, src, tgt, &self.budget)?);
        }
        if self.options.monic {
            return Ok(solve_monic(cfn, &self.budget));
        }
        Ok(solve(cfn, &self.budget))
    }

    /// Turn one assignment into a span.
    fn assemble(
        &self,
        src: &Schema,
        tgt: &Schema,
        cfn: &Cfn,
        outcome: &SolveOutcome,
        assignment: &Assignment,
    ) -> Result<SchemaSpan, SpanError> {
        let images = image_map(cfn, assignment);
        let keep_v: FxHashSet<Name> = images.keys().cloned().collect();
        let apex = induce_on_vertices(src, self.protocol, &keep_v)?;
        debug_assert_apex_lost_nothing(src, &apex, &keep_v);

        let apex_digest = canonical_digest(&apex);
        let apex_id = Name::from(digest_hex(&apex_digest).as_str());
        let src_id = Name::from(digest_hex(&canonical_digest(src)).as_str());
        let tgt_id = Name::from(digest_hex(&canonical_digest(tgt)).as_str());

        let left = inclusion(&apex, apex_id.clone(), src_id);
        let right = right_leg(&apex, tgt, &images, self.options.iso)
            .with_endpoints(Some(apex_id), Some(tgt_id));

        let radix = cfn.radix();
        let certificate = SpanCertificate {
            proven_optimal: outcome.proven_optimal,
            shape: LegShape {
                left_is_mono: is_injective(&left),
                right_is_mono: is_vertex_injective(&right),
                right_edge_images: if is_edge_injective(&right) {
                    EdgeImages::Distinct
                } else {
                    EdgeImages::Shared
                },
                left_is_iso: apex.vertices.len() == src.vertices.len()
                    && apex.edges.len() == mappable_edges(src),
            },
            legs_are_functorial: legs_are_functorial(&apex, src, tgt, &left, &right),
            left_existence: self.existence_of(&apex, src, &left),
            right_existence: self.existence_of(&apex, tgt, &right),
            apex_pointed: !apex.entries.is_empty(),
            apex_digest,
            path: outcome.path,
            tie_break_order: decode_order(cfn, outcome),
            limit_hit: outcome.limit_hit,
        };

        Ok(SchemaSpan {
            apex_coverage: coverage(&apex, src),
            apex,
            left,
            right,
            quality: cfn.quality_of(assignment),
            quality_bounds: (
                quality_of_cost(outcome.upper_bound, radix),
                quality_of_cost(outcome.lower_bound, radix),
            ),
            certificate,
        })
    }

    /// The existence report for one leg, against whatever theories the caller
    /// supplied.
    fn existence_of(&self, apex: &Schema, codomain: &Schema, leg: &Migration) -> ExistenceReport {
        let empty = HashMap::new();
        let theories = self.theories.unwrap_or(&empty);
        check_existence(self.protocol, apex, codomain, leg, theories)
    }
}

// ---------------------------------------------------------------------------
// Construction helpers
// ---------------------------------------------------------------------------

/// Source edges both of whose endpoints are source vertices.
///
/// [`Schema`]'s fields are public, so an edge can name a vertex the schema does
/// not hold. Such an edge has no variable at that end, so no assignment can give
/// it an image and inducing can never place it in the apex. Counting it would
/// make a span that covers the whole source look partial, which is why
/// [`LegShape::left_is_iso`] measures the apex against this rather than against
/// `src.edges.len()`.
pub(crate) fn mappable_edges(src: &Schema) -> usize {
    src.edges
        .keys()
        .filter(|edge| src.vertices.contains_key(&edge.src) && src.vertices.contains_key(&edge.tgt))
        .count()
}

/// The variable names in the order the tie-break reads them.
///
/// `decode_traced` walks the elimination order backwards, so the lexicographic
/// comparison among equally good assignments is over the assignment vector in
/// *reverse* elimination order. Reporting the reversed sequence is what lets a
/// caller reproduce the choice rather than take it on trust.
fn decode_order(cfn: &Cfn, outcome: &SolveOutcome) -> Option<Vec<Name>> {
    let order = outcome.elimination_order.as_ref()?;
    Some(
        order
            .iter()
            .rev()
            .filter_map(|var| Some(cfn.variable(*var)?.name().clone()))
            .collect(),
    )
}

/// The source vertices the assignment gave a target, paired with that target.
pub(crate) fn image_map(cfn: &Cfn, assignment: &Assignment) -> HashMap<Name, Name> {
    let mut images = HashMap::new();
    for (var, value) in assignment.pairs() {
        let Some(variable) = cfn.variable(var) else {
            continue;
        };
        let Some(image) = variable.value_name(value) else {
            continue;
        };
        images.insert(variable.name().clone(), image.clone());
    }
    images
}

/// The left leg: the identity on the apex's own keys.
fn inclusion(apex: &Schema, apex_id: Name, src_id: Name) -> Migration {
    let vertices: Vec<Name> = apex.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = apex.edges.keys().cloned().collect();
    Migration::identity(&vertices, &edges).with_endpoints(Some(apex_id), Some(src_id))
}

/// The right leg: the assignment restricted to the apex, with each apex edge
/// sent to its image.
///
/// `bijective_edges` asks for a kind-preserving bijection from the apex's edges
/// onto the edges of the target between mapped endpoints, which is what an
/// isomorphism means and what a greedy image would not give: two parallel apex
/// arcs of one kind whose names have no counterpart both take the first arc of
/// that kind. When the bijection does not exist the greedy map is returned
/// instead, and [`LegShape::right_edge_images`] records whether what came back
/// is injective on edges. That field rather than [`LegShape::right_is_mono`],
/// which is a statement about vertices only and is true of a map that sends two
/// parallel arcs onto one.
fn right_leg(
    apex: &Schema,
    tgt: &Schema,
    images: &HashMap<Name, Name>,
    bijective_edges: bool,
) -> Migration {
    let vertex_map: HashMap<Name, Name> = apex
        .vertices
        .keys()
        .filter_map(|vertex| Some((vertex.clone(), images.get(vertex)?.clone())))
        .collect();

    let edge_map = if bijective_edges {
        bijective_edge_map(apex, tgt, &vertex_map)
            .unwrap_or_else(|| greedy_edge_map(apex, tgt, &vertex_map))
    } else {
        greedy_edge_map(apex, tgt, &vertex_map)
    };

    Migration {
        vertex_map,
        edge_map,
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        domain: None,
        codomain: None,
    }
}

/// Each apex edge sent to the target edge the objective scored it against.
///
/// The naturality constraint gives every apex edge an image on a feasible
/// assignment, so an edge missing here means the assignment was infeasible.
pub(crate) fn greedy_edge_map(
    apex: &Schema,
    tgt: &Schema,
    vertex_map: &HashMap<Name, Name>,
) -> HashMap<Edge, Edge> {
    let mut edge_map = HashMap::new();
    for edge in apex.edges.keys() {
        let (Some(from), Some(to)) = (vertex_map.get(&edge.src), vertex_map.get(&edge.tgt)) else {
            continue;
        };
        if let Some(image) = edge_image(tgt, edge, from, to) {
            edge_map.insert(edge.clone(), image.clone());
        }
    }
    edge_map
}

/// A kind-preserving bijection from the apex's edges onto the target's edges
/// between mapped endpoints, or `None` when none exists.
///
/// A vertex map that is injective and onto is not an isomorphism of schemas.
/// Three parallel arcs mapped onto one satisfy every condition a vertex map can
/// state, and so does one arc mapped into a pair of parallel arcs; the first is
/// not injective on edges, the second is not surjective, and neither is
/// invertible.
///
/// The map is *built* to be bijective rather than built greedily and checked,
/// because rejecting a greedy assignment that happens to collide would refuse
/// isomorphisms that exist. Apex edges between one mapped vertex pair can only
/// land on target edges between the images of that pair, so the problem
/// decomposes into one bipartite matching per pair, and within a pair the only
/// constraint is the arc kind. Equal counts per pair is therefore exactly the
/// solvability condition, and any pairing within a kind is a witness; name
/// agreement is preferred so that the answer is the expected one where names do
/// agree.
pub(crate) fn bijective_edge_map(
    apex: &Schema,
    tgt: &Schema,
    vertex_map: &HashMap<Name, Name>,
) -> Option<HashMap<Edge, Edge>> {
    let mut groups: std::collections::BTreeMap<(Name, Name), Vec<&Edge>> =
        std::collections::BTreeMap::new();
    for edge in apex.edges.keys() {
        let from = vertex_map.get(&edge.src)?;
        let to = vertex_map.get(&edge.tgt)?;
        groups
            .entry((from.clone(), to.clone()))
            .or_default()
            .push(edge);
    }

    let mut edge_map: HashMap<Edge, Edge> = HashMap::new();
    let mut covered = 0usize;
    for ((from, to), mut sources) in groups {
        sources.sort_unstable();
        let pool = tgt.edges_between(from.as_str(), to.as_str());
        if sources.len() != pool.len() {
            return None;
        }
        covered += pool.len();

        let mut taken = vec![false; pool.len()];
        for edge in sources {
            let named = pool.iter().enumerate().find_map(|(slot, candidate)| {
                let free = taken.get(slot).copied() == Some(false);
                (free && candidate.kind == edge.kind && candidate.name == edge.name).then_some(slot)
            });
            let slot = named.or_else(|| {
                pool.iter().enumerate().find_map(|(slot, candidate)| {
                    let free = taken.get(slot).copied() == Some(false);
                    (free && candidate.kind == edge.kind).then_some(slot)
                })
            })?;
            *taken.get_mut(slot)? = true;
            edge_map.insert(edge.clone(), pool.get(slot)?.clone());
        }
    }

    // Surjectivity, made explicit: every target edge *between mapped endpoints*
    // was consumed. Comparing against the whole target instead would be the
    // same test only when the vertex map is onto the target's vertices, which
    // holds on the isomorphism path and fails on every other caller: the apex
    // of a maximum common induced sub-schema is a proper part of the target for
    // essentially every non-isomorphic pair, so a bijection that had just been
    // built successfully would be discarded and the greedy fallback would send
    // two parallel apex arcs onto one target arc.
    let images: std::collections::BTreeSet<&Name> = vertex_map.values().collect();
    let reachable = tgt
        .edges
        .keys()
        .filter(|edge| images.contains(&edge.src) && images.contains(&edge.tgt))
        .count();
    if covered != reachable {
        return None;
    }
    Some(edge_map)
}

/// Whether a migration's vertex and edge maps are both injective.
fn is_injective(migration: &Migration) -> bool {
    is_vertex_injective(migration) && is_edge_injective(migration)
}

/// Whether a migration's edge map is injective.
fn is_edge_injective(migration: &Migration) -> bool {
    let mut images: Vec<&Edge> = migration.edge_map.values().collect();
    images.sort_unstable();
    let before = images.len();
    images.dedup();
    images.len() == before
}

/// Whether a migration's vertex map is injective.
fn is_vertex_injective(migration: &Migration) -> bool {
    let mut images: Vec<&Name> = migration.vertex_map.values().collect();
    images.sort_unstable();
    let before = images.len();
    images.dedup();
    images.len() == before
}

/// Whether both legs are structure preserving on their mapped fragments.
///
/// # Panics
///
/// In debug builds, if the left leg is not. It is an inclusion, so a failure
/// there is a defect in the construction rather than a property of the search.
fn legs_are_functorial(
    apex: &Schema,
    src: &Schema,
    tgt: &Schema,
    left: &Migration,
    right: &Migration,
) -> bool {
    let left_ok = check_migration_morphism(apex, src, left).is_ok();
    debug_assert!(
        left_ok,
        "the left leg is an inclusion, so it cannot fail to be a morphism"
    );
    left_ok && check_migration_morphism(apex, tgt, right).is_ok()
}

/// The fraction of the source's vertices the apex covers.
fn coverage(apex: &Schema, src: &Schema) -> f64 {
    if src.vertices.is_empty() {
        // Nothing to cover, and every vertex of nothing is covered. Reporting
        // zero here would say an empty source was badly matched.
        return 1.0;
    }
    let kept = u32::try_from(apex.vertices.len()).unwrap_or(u32::MAX);
    let total = u32::try_from(src.vertices.len()).unwrap_or(u32::MAX);
    f64::from(kept) / f64::from(total)
}

/// The quality a packed cost reads as.
///
/// Clamped at the scale so that `⊤`, which is far above it, reads as the worst
/// finite quality rather than as a large negative number.
fn quality_of_cost(cost: Cost, radix: u64) -> f64 {
    let units = cost.quality_part(radix).min(COST_SCALE);
    let units = u32::try_from(units).unwrap_or(u32::MAX);
    1.0 - f64::from(units) / COST_SCALE_FLOAT
}

/// A digest in lower-case hexadecimal.
fn digest_hex(digest: &[u8; 32]) -> String {
    blake3::Hash::from_bytes(*digest).to_hex().to_string()
}

/// The tripwire for the four fields whose dangling-reference cases the hard
/// constraints of the network are supposed to prevent.
///
/// Hyper-edges, variants, recursion points and schema spans each name a set of
/// vertices that need not be joined by an edge, and each has a `⊤`-valued
/// constraint keeping that set whole. If one of those constraints is ever
/// omitted, inducing the apex quietly drops the entry instead of the search
/// refusing the assignment, and the only symptom is an apex smaller than the
/// assignment implies. What is asserted here is therefore the *constraint*, not
/// what inducing did with it: restating inducing's own field rules would be a
/// tautology that can never fire.
///
/// A vertex the source schema does not hold has no variable, so no constraint
/// can mention it and inducing drops the entry naming it whatever the search
/// decided. Those are excluded, so that a malformed source is not read as a
/// missing constraint.
///
/// # Panics
///
/// In debug builds, if an entry survived in part.
fn debug_assert_apex_lost_nothing(src: &Schema, apex: &Schema, keep_v: &FxHashSet<Name>) {
    debug_assert!(
        signatures_survived_whole(src, keep_v),
        "a hyper-edge signature survived in part, so its clique constraint is missing"
    );
    debug_assert!(
        fixpoints_kept_their_targets(src, keep_v),
        "a fixpoint marker survived without its target, so its constraint is missing"
    );
    debug_assert!(
        schema_spans_survived_whole(src, keep_v),
        "a schema span survived at one end only, so its constraint is missing"
    );
    debug_assert!(
        coproducts_kept_their_arms(src, apex, keep_v),
        "a coproduct survived without one of its arms, so its constraint is missing"
    );
}

/// Whether the source schema holds this vertex.
///
/// A vertex it does not hold has no variable, so no constraint mentions it and
/// every predicate below excuses it.
fn holds(src: &Schema, vertex: &Name) -> bool {
    src.vertices.contains_key(vertex)
}

/// Whether every hyper-edge signature survived whole or not at all.
fn signatures_survived_whole(src: &Schema, keep_v: &FxHashSet<Name>) -> bool {
    src.hyper_edges.values().all(|hyper| {
        let members = || hyper.signature.values().filter(|v| holds(src, v));
        members().all(|v| keep_v.contains(v)) || members().all(|v| !keep_v.contains(v))
    })
}

/// Whether every surviving fixpoint marker kept the vertex it unfolds to.
fn fixpoints_kept_their_targets(src: &Schema, keep_v: &FxHashSet<Name>) -> bool {
    src.recursion_points.iter().all(|(mu, point)| {
        !keep_v.contains(mu)
            || !holds(src, &point.target_vertex)
            || keep_v.contains(&point.target_vertex)
    })
}

/// Whether every schema span survived at both ends or at neither.
fn schema_spans_survived_whole(src: &Schema, keep_v: &FxHashSet<Name>) -> bool {
    src.spans.values().all(|span| {
        !holds(src, &span.left)
            || !holds(src, &span.right)
            || keep_v.contains(&span.left) == keep_v.contains(&span.right)
    })
}

/// Whether every surviving coproduct kept every arm injected into it, and
/// whether the apex records them all.
fn coproducts_kept_their_arms(src: &Schema, apex: &Schema, keep_v: &FxHashSet<Name>) -> bool {
    src.variants
        .iter()
        .filter(|(coproduct, _)| keep_v.contains(*coproduct))
        .all(|(coproduct, arms)| {
            let whole = arms
                .iter()
                .filter(|arm| holds(src, &arm.id) && holds(src, &arm.parent_vertex))
                .count();
            arms.iter().all(|arm| {
                (!holds(src, &arm.id) || keep_v.contains(&arm.id))
                    && (!holds(src, &arm.parent_vertex) || keep_v.contains(&arm.parent_vertex))
            }) && apex.variants.get(coproduct).map_or(0, Vec::len) == whole
        })
}

/// A schema with nothing in it.
///
/// [`SchemaBuilder`](panproto_schema::SchemaBuilder) refuses to build one, on
/// the ground that an empty schema is not something a caller means to
/// construct, so a test that needs one as an *input* writes it out. Both the
/// span search and the total search have to answer on it, and the answers
/// differ, which is what the tests here and in
/// [`hom_search`](crate::hom_search) check.
#[cfg(test)]
pub(crate) fn empty_schema(protocol: &str) -> Schema {
    Schema {
        protocol: protocol.to_owned(),
        vertices: HashMap::new(),
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: Vec::new(),
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::too_many_lines
)]
mod tests {
    use super::*;
    use panproto_schema::{SchemaBuilder, validate};

    fn protocol() -> Protocol {
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

    fn schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (from, to, kind, name) in edges {
            builder = builder.edge(from, to, kind, Some(*name)).unwrap();
        }
        if let Some((entry, _)) = vertices.first() {
            builder = builder.entry(entry);
        }
        builder.build().unwrap()
    }

    #[test]
    fn the_cost_scale_float_is_the_cost_scale() {
        assert_eq!(COST_SCALE, 1_000_000_000);
        assert!((COST_SCALE_FLOAT - 1_000_000_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_schema_against_itself_is_a_total_morphism() {
        let s = schema(
            &[("root", "object"), ("root.name", "string")],
            &[("root", "root.name", "prop", "name")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&s, &s).unwrap();

        assert!(span.is_total(), "a schema maps onto itself");
        assert!(span.certificate.shape.left_is_iso);
        assert_eq!(span.apex_coverage, 1.0);
        assert_eq!(span.apex.vertices.len(), s.vertices.len());
        assert_eq!(span.apex.edges.len(), s.edges.len());

        let total = span.as_total_morphism().expect("a total span has a shape");
        assert_eq!(total.vertex_map, span.right.vertex_map);
        assert_eq!(total.edge_map, span.right.edge_map);
        assert_eq!(total.quality, span.quality);
        for (source, image) in &total.vertex_map {
            assert_eq!(source, image, "the identity is the optimum here");
        }
    }

    #[test]
    fn the_left_leg_is_an_inclusion_and_the_legs_carry_endpoints() {
        let s = schema(
            &[("root", "object"), ("root.name", "string")],
            &[("root", "root.name", "prop", "name")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&s, &s).unwrap();

        for (source, image) in &span.left.vertex_map {
            assert_eq!(source, image, "the left leg is the identity on vertices");
        }
        for (source, image) in &span.left.edge_map {
            assert_eq!(source, image, "the left leg is the identity on edges");
        }
        assert!(span.certificate.shape.left_is_mono);
        assert_eq!(
            span.left.domain.as_ref().map(Name::as_str),
            Some(span.apex_digest_hex().as_str()),
            "both legs leave the apex"
        );
        assert_eq!(span.left.domain, span.right.domain);
        assert!(span.left.codomain.is_some());
        assert!(span.right.codomain.is_some());
    }

    #[test]
    fn both_legs_pass_their_checks() {
        let src = schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", "text")],
        );
        let tgt = schema(
            &[("root", "object"), ("root.body", "string")],
            &[("root", "root.body", "prop", "body")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&src, &tgt).unwrap();

        assert!(span.certificate.legs_are_functorial);
        assert!(
            span.certificate.right_existence.valid,
            "existence reported {:?}",
            span.certificate.right_existence.errors
        );
        assert!(check_migration_morphism(&span.apex, &src, &span.left).is_ok());
        assert!(check_migration_morphism(&span.apex, &tgt, &span.right).is_ok());
    }

    #[test]
    fn a_pair_with_no_shared_kinds_returns_an_empty_apex_without_error() {
        let src = schema(
            &[("a", "object"), ("a.x", "string")],
            &[("a", "a.x", "prop", "x")],
        );
        let tgt = schema(&[("b", "integer"), ("c", "integer")], &[]);
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&src, &tgt).unwrap();

        assert!(span.apex.vertices.is_empty(), "nothing can be matched");
        assert!(span.apex.edges.is_empty());
        assert_eq!(span.apex_coverage, 0.0);
        assert!(!span.is_total());
        assert!(span.as_total_morphism().is_none());
        assert!(span.to_overlap().vertex_pairs.is_empty());
    }

    #[test]
    fn an_empty_source_returns_a_span_rather_than_an_error() {
        let empty = super::empty_schema("test");
        let tgt = schema(&[("root", "object")], &[]);
        let protocol = protocol();

        let span = SpanSearch::new(&protocol).run(&empty, &tgt).unwrap();
        assert!(span.apex.vertices.is_empty());
        assert_eq!(span.apex_coverage, 1.0, "nothing to cover is fully covered");
        assert!(span.is_total(), "the empty inclusion is onto");
        assert!(!span.certificate.apex_pointed);
    }

    #[test]
    fn the_apex_validates_and_its_digest_is_stable() {
        let src = schema(
            &[
                ("root", "object"),
                ("root.a", "string"),
                ("root.n", "integer"),
            ],
            &[
                ("root", "root.a", "prop", "a"),
                ("root", "root.n", "prop", "n"),
            ],
        );
        let tgt = schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let protocol = protocol();
        let search = SpanSearch::new(&protocol);

        let first = search.run(&src, &tgt).unwrap();
        let second = search.run(&src, &tgt).unwrap();
        assert_eq!(
            first.certificate.apex_digest,
            second.certificate.apex_digest
        );
        assert_eq!(first.apex_digest_hex(), second.apex_digest_hex());
        assert_eq!(first.apex_digest_hex().len(), 64);
        assert!(validate(&first.apex, &protocol).is_empty());
        assert_eq!(
            canonical_digest(&first.apex),
            first.certificate.apex_digest,
            "the digest is the apex's own"
        );
    }

    #[test]
    fn the_apex_keeps_its_adjacency_indexes() {
        // The regression for a hand-built apex that left `outgoing` empty while
        // `edges` was not, which makes every adjacency query on it answer with
        // nothing.
        let s = schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&s, &s).unwrap();

        assert!(!span.apex.edges.is_empty());
        assert!(!span.apex.outgoing_edges("root").is_empty());
        assert!(!span.apex.incoming_edges("root.a").is_empty());
        assert!(!span.apex.edges_between("root", "root.a").is_empty());
    }

    #[test]
    fn to_overlap_and_pushout_succeed() {
        let src = schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", "text")],
        );
        let tgt = schema(
            &[("root", "object"), ("root.body", "string")],
            &[("root", "root.body", "prop", "body")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&src, &tgt).unwrap();

        let overlap = span.to_overlap();
        assert_eq!(overlap.vertex_pairs.len(), span.apex.vertices.len());
        assert_eq!(overlap.edge_pairs.len(), span.right.edge_map.len());
        let sorted = {
            let mut pairs = overlap.vertex_pairs.clone();
            pairs.sort_unstable();
            pairs
        };
        assert_eq!(overlap.vertex_pairs, sorted, "the pair list is ordered");

        let (merged, left, right) = span.pushout(&src, &tgt).unwrap();
        assert!(!merged.vertices.is_empty());
        assert_eq!(left.vertex_map.len(), src.vertices.len());
        assert_eq!(right.vertex_map.len(), tgt.vertices.len());
    }

    #[test]
    fn iso_makes_the_right_leg_a_mono() {
        let src = schema(
            &[("a", "object"), ("b", "string")],
            &[("a", "b", "prop", "p")],
        );
        let tgt = schema(
            &[("x", "object"), ("y", "string")],
            &[("x", "y", "prop", "q")],
        );
        let protocol = protocol();
        let options = SearchOptions {
            iso: true,
            ..SearchOptions::default()
        };
        let span = SpanSearch::new(&protocol)
            .with_options(options)
            .run(&src, &tgt)
            .unwrap();

        assert!(span.certificate.shape.right_is_mono);
        assert_eq!(span.certificate.path, SolverPath::Iso);
        assert_eq!(
            span.apex.vertices.len(),
            2,
            "the two schemas are isomorphic"
        );
        assert_eq!(span.right.edge_map.len(), 1);
    }

    #[test]
    fn a_partial_match_drops_only_what_it_must() {
        let src = schema(
            &[
                ("root", "object"),
                ("root.a", "string"),
                ("root.n", "integer"),
            ],
            &[
                ("root", "root.a", "prop", "a"),
                ("root", "root.n", "prop", "n"),
            ],
        );
        // No integer vertex on the target, so `root.n` cannot be matched and
        // everything else can.
        let tgt = schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&src, &tgt).unwrap();

        assert!(!span.is_total());
        assert_eq!(span.apex.vertices.len(), 2, "only the integer is dropped");
        assert!(span.apex.vertices.contains_key("root"));
        assert!(span.apex.vertices.contains_key("root.a"));
        assert!(!span.apex.vertices.contains_key("root.n"));
        assert!((span.apex_coverage - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(span.apex.edges.len(), 1, "the dropped edge went with it");
    }

    #[test]
    fn the_apex_keeps_its_non_edge_structure_whole() {
        // The four fields whose members need not be joined by an edge. Each has
        // a hard constraint keeping its set whole, so a partial match may drop
        // the whole group and may never drop part of it. The debug tripwire in
        // `assemble` fires if a constraint is ever omitted, and this is the
        // fixture that reaches it.
        let mut src = schema(
            &[
                ("root", "object"),
                ("shape", "object"),
                ("shape.circle", "string"),
                ("shape.square", "string"),
                ("mu", "object"),
                ("orphan", "integer"),
            ],
            &[
                ("root", "shape", "prop", "shape"),
                ("root", "mu", "prop", "mu"),
                ("root", "orphan", "prop", "orphan"),
            ],
        );
        src.variants.insert(
            Name::from("shape"),
            vec![
                panproto_schema::Variant {
                    id: Name::from("shape.circle"),
                    parent_vertex: Name::from("shape"),
                    tag: None,
                },
                panproto_schema::Variant {
                    id: Name::from("shape.square"),
                    parent_vertex: Name::from("shape"),
                    tag: None,
                },
            ],
        );
        src.recursion_points.insert(
            Name::from("mu"),
            panproto_schema::RecursionPoint {
                target_vertex: Name::from("root"),
            },
        );
        src.spans.insert(
            Name::from("s"),
            panproto_schema::Span {
                id: Name::from("s"),
                left: Name::from("shape.circle"),
                right: Name::from("shape.square"),
            },
        );
        src.hyper_edges.insert(
            Name::from("h"),
            panproto_schema::HyperEdge {
                id: Name::from("h"),
                kind: Name::from("record"),
                signature: HashMap::from([
                    (Name::from("l"), Name::from("shape.circle")),
                    (Name::from("r"), Name::from("shape.square")),
                ]),
                parent_label: Name::from("l"),
            },
        );

        // The target has no integer, so the orphan must be dropped, and it has
        // one string too few to take both variants, so the whole coproduct group
        // has to go with them rather than half of it.
        let tgt = schema(
            &[
                ("root", "object"),
                ("shape", "object"),
                ("shape.circle", "string"),
                ("mu", "object"),
            ],
            &[
                ("root", "shape", "prop", "shape"),
                ("root", "mu", "prop", "mu"),
                ("shape", "shape.circle", "variant", "circle"),
            ],
        );
        let protocol = protocol();
        let opts = SearchOptions {
            monic: true,
            ..SearchOptions::default()
        };
        let span = SpanSearch::new(&protocol)
            .with_options(opts)
            .run(&src, &tgt)
            .unwrap();

        // One arm cannot be placed injectively, so the span and the hyper-edge
        // take the other with it, and the coproduct goes with both rather than
        // being left pointing at nothing.
        let kept = |id: &str| span.apex.vertices.contains_key(id);
        assert!(!kept("shape.circle"), "the pair is dropped whole");
        assert!(!kept("shape.square"));
        assert!(!kept("shape"), "and the coproduct with it");
        assert!(span.apex.variants.is_empty());
        assert!(span.apex.spans.is_empty());
        assert!(span.apex.hyper_edges.is_empty());
        assert!(!kept("orphan"), "the integer has nowhere to go");
        assert!(kept("root") && kept("mu"), "the rest survives");
        assert_eq!(
            span.apex.recursion_points.len(),
            1,
            "the marker and its target both survive, so the point does"
        );
        assert!(validate(&span.apex, &protocol).is_empty());

        // A target with room for both arms keeps the whole group instead, which
        // is the branch that exercises the arm-level count.
        let wide = schema(
            &[
                ("root", "object"),
                ("shape", "object"),
                ("shape.circle", "string"),
                ("shape.square", "string"),
                ("mu", "object"),
            ],
            &[
                ("root", "shape", "prop", "shape"),
                ("root", "mu", "prop", "mu"),
                ("shape", "shape.circle", "variant", "circle"),
                ("shape", "shape.square", "variant", "square"),
            ],
        );
        let span = SpanSearch::new(&protocol)
            .with_options(SearchOptions {
                monic: true,
                ..SearchOptions::default()
            })
            .run(&src, &wide)
            .unwrap();

        assert!(span.apex.vertices.contains_key("shape"));
        assert_eq!(
            span.apex.variants.get("shape").map_or(0, Vec::len),
            2,
            "a surviving coproduct keeps every arm"
        );
        assert_eq!(span.apex.spans.len(), 1);
        assert_eq!(span.apex.hyper_edges.len(), 1);
        assert!(!span.apex.vertices.contains_key("orphan"));
        assert!(validate(&span.apex, &protocol).is_empty());
    }

    #[test]
    fn the_quality_bounds_bracket_the_quality() {
        let src = schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        let tgt = schema(
            &[("root", "object"), ("root.b", "string")],
            &[("root", "root.b", "prop", "b")],
        );
        let protocol = protocol();
        let span = SpanSearch::new(&protocol).run(&src, &tgt).unwrap();

        let (low, high) = span.quality_bounds;
        assert!(low <= span.quality && span.quality <= high);
        assert!(span.certificate.proven_optimal);
        assert_eq!(low, high, "a proven optimum has no interval");
        assert!((0.0..=1.0).contains(&span.quality));
    }

    #[test]
    fn optima_agree_with_the_single_answer() {
        let src = schema(
            &[("root", "object"), ("root.a", "string")],
            &[("root", "root.a", "prop", "a")],
        );
        // Two equally-named string targets, so the assignment of `root.a` is a
        // genuine tie.
        let tgt = schema(
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
        let protocol = protocol();
        let search = SpanSearch::new(&protocol);

        let one = search.run(&src, &tgt).unwrap();
        let all = search.optima(&src, &tgt, 8).unwrap();
        assert!(!all.is_empty());
        for span in &all {
            assert_eq!(
                span.quality, one.quality,
                "every enumerated span attains the optimum"
            );
        }
        assert!(
            all.iter()
                .any(|span| span.right.vertex_map == one.right.vertex_map),
            "the canonical answer is among the optima"
        );
    }

    #[test]
    fn the_default_certificate_establishes_nothing() {
        let certificate = SpanCertificate::default();
        assert!(!certificate.proven_optimal);
        assert!(!certificate.shape.left_is_mono);
        assert!(!certificate.right_existence.valid);
        assert_eq!(certificate.apex_digest, [0u8; 32]);
    }
}
