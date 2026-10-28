//! Lens paths: the free category on atomic lens stages.
//!
//! A nonidentity path is a nonempty word of explicitly typed atomic
//! stages. The distinguished identity is represented abstractly as the
//! empty word at one schema. Production stores one marked identity-lens
//! stage instead, so comparing the two representations requires a
//! separate adapter proof. `then` checks the shared endpoint and
//! concatenates without fusing. This reference has no owned compiled-lens
//! composer. Shape adapters may nevertheless supply a fusion predicate to
//! [`LensPath::normal_form`]. That derived operation retains the atomic word,
//! admits only certificate-bearing fusions, and leaves every rejected pair
//! in sequence with a typed obstruction.

use std::convert::Infallible;
use std::ops::Range;

use crate::schema::Schema;

/// Boundary identity of a schema used as a path endpoint.
///
/// Path composition equates the protocol and the carrier's vertex, edge and
/// hyper-edge key sets. Validation declarations and other annotations belong
/// to the adjacent stages rather than to the shared path object.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaEndpoint(Vec<u8>);

impl SchemaEndpoint {
    /// Derive an endpoint identity for the reference schema's default protocol.
    #[must_use]
    pub fn of(schema: &Schema) -> Self {
        Self::of_protocol(schema, "")
    }

    /// Derive an endpoint identity from a protocol and the structural carrier.
    #[must_use]
    pub fn of_protocol(schema: &Schema, protocol: &str) -> Self {
        let mut writer = crate::canonical::Writer::default();
        writer.str(protocol);
        writer.len(schema.vertices.len());
        for vertex in schema.vertices.keys() {
            writer.str(vertex);
        }
        writer.len(schema.edges.len());
        for edge in &schema.edges {
            writer.edge(edge);
        }
        writer.len(schema.hyper_edges.len());
        for hyper_edge in schema.hyper_edges.keys() {
            writer.str(hyper_edge);
        }
        Self(writer.bytes)
    }

    /// Canonical path-boundary projection bytes underlying this identity.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// One typed atomic stage of a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathStage<L> {
    payload: L,
    source: SchemaEndpoint,
    target: SchemaEndpoint,
    lens_digest: Vec<u8>,
    envelope: Vec<u8>,
}

/// One condition of the L.6 fusion predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FusionCondition {
    /// Neither input nor the composite contracts a path.
    ContractionFree,
    /// The two compiled migrations compose.
    MigrationComposes,
    /// Direct edge images and endpoint images agree.
    EdgeImagesAgree,
    /// Both input plans and the composite plan are empty.
    PlansEmpty,
    /// The value layers obey the one-layer composition rule.
    ValueLayersCompatible,
    /// Neither input retargets a fan.
    ResolversEmpty,
    /// Every plan compiles with only keep/drop fates.
    PlansCompileKeepOrDrop,
    /// Shape-specific boundary context compilation succeeds.
    ContextCompiles,
}

/// Independently observed premises of production table fusion.
///
/// The fields are facts about the two atomic stages and an explicit proposed
/// composite, not a cached production decision. This classifier does not
/// decide whether the earlier structural-fusion predicate supplies that
/// composite. Keeping the facts explicit lets the reference certificate vary
/// one named table boundary while retaining the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableFusionPremises {
    /// The first stage has no field transform or operation assignment.
    pub first_stage_structurally_pure: bool,
    /// The first, second and proposed composite table plans compile.
    pub plans_compile: bool,
    /// The first table plan is a bijection with unit complement.
    pub first_is_unit_bijection: bool,
    /// The first target and second source relational boundaries are equal.
    pub middle_boundaries_equal: bool,
    /// The second source has no finite-table required-value obligation.
    pub second_source_validation_free: bool,
    /// A second-stage value layer sees identical placeholder contexts.
    pub typed_placeholders_agree: bool,
    /// The two atomic relational functors compose to the candidate functor.
    pub functors_compose: bool,
    /// The second-stage column map is injective within each source table.
    pub second_columns_injective: bool,
}

/// The first failed table-fusion premise, in production evaluation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableFusionObstruction {
    /// The first stage writes value-layer coordinates.
    FirstStageWritesFields,
    /// At least one table plan did not compile.
    PlanCompilation,
    /// The first stage retains non-unit table complement state.
    FirstStageNotUnitBijection,
    /// The two independently classified middle boundaries differ.
    MiddleBoundaryMismatch,
    /// Atomic second put performs a required-value validation fusion skips.
    RequiredValueValidation,
    /// Absent cells receive different typed placeholders across the boundary.
    TypedPlaceholderMismatch,
    /// The candidate relational functor is not the atomic composite.
    FunctorComposition,
    /// Two source columns collide at one second-stage target column.
    ColumnCollision,
}

/// Classify one independently derived table-fusion premise record.
///
/// The order is part of the reference contract: it follows the production
/// table-specific guard sequence. Structural-candidate existence is a
/// separate premise, so a fixture may use this classifier to audit a
/// redundant downstream guard without calling that guard an isolated
/// normalization refusal.
#[must_use]
pub const fn classify_table_fusion(
    premises: TableFusionPremises,
) -> Result<(), TableFusionObstruction> {
    if !premises.first_stage_structurally_pure {
        Err(TableFusionObstruction::FirstStageWritesFields)
    } else if !premises.plans_compile {
        Err(TableFusionObstruction::PlanCompilation)
    } else if !premises.first_is_unit_bijection {
        Err(TableFusionObstruction::FirstStageNotUnitBijection)
    } else if !premises.middle_boundaries_equal {
        Err(TableFusionObstruction::MiddleBoundaryMismatch)
    } else if !premises.second_source_validation_free {
        Err(TableFusionObstruction::RequiredValueValidation)
    } else if !premises.typed_placeholders_agree {
        Err(TableFusionObstruction::TypedPlaceholderMismatch)
    } else if !premises.functors_compose {
        Err(TableFusionObstruction::FunctorComposition)
    } else if !premises.second_columns_injective {
        Err(TableFusionObstruction::ColumnCollision)
    } else {
        Ok(())
    }
}

impl FusionCondition {
    /// Contract ordinal used by bounded semantic derivations.
    #[must_use]
    pub const fn ordinal(self) -> usize {
        match self {
            Self::ContractionFree => 0,
            Self::MigrationComposes => 1,
            Self::EdgeImagesAgree => 2,
            Self::PlansEmpty => 3,
            Self::ValueLayersCompatible => 4,
            Self::ResolversEmpty => 5,
            Self::PlansCompileKeepOrDrop => 6,
            Self::ContextCompiles => 7,
        }
    }
}

const COMMON_FUSION_CONDITIONS: [FusionCondition; 7] = [
    FusionCondition::ContractionFree,
    FusionCondition::MigrationComposes,
    FusionCondition::EdgeImagesAgree,
    FusionCondition::PlansEmpty,
    FusionCondition::ValueLayersCompatible,
    FusionCondition::ResolversEmpty,
    FusionCondition::PlansCompileKeepOrDrop,
];

/// Read-only inputs against which a fusion premise is derived.
///
/// The view is created only inside [`FusionCandidate::derive`]. A premise
/// oracle therefore receives the exact adjacent groups, proposed payload,
/// compiled identity bytes, and optional boundary context that the admitted
/// candidate will retain.
#[derive(Clone, Copy, Debug)]
pub struct FusionInputs<'a, L> {
    left: &'a NormalGroup<L>,
    right: &'a NormalGroup<L>,
    payload: &'a L,
    lens_digest: &'a [u8],
    envelope: &'a [u8],
    context: Option<&'a [u8]>,
}

type PremiseDerivation<L> = for<'a> fn(FusionCondition, &FusionInputs<'a, L>) -> bool;

/// Opaque, crate-derived fusion semantics for one payload family.
///
/// External callers cannot construct this value or supply their own Boolean
/// premise oracle. Each reference carrier exposes only its trusted derivation.
#[derive(Clone, Copy)]
pub struct FusionSemantics<L> {
    derive: PremiseDerivation<L>,
}

impl<L> FusionSemantics<L> {
    pub(crate) const fn trusted(derive: PremiseDerivation<L>) -> Self {
        Self { derive }
    }

    fn holds(&self, condition: FusionCondition, inputs: &FusionInputs<'_, L>) -> bool {
        (self.derive)(condition, inputs)
    }
}

impl<'a, L> FusionInputs<'a, L> {
    /// Left adjacent group.
    #[must_use]
    pub const fn left(&self) -> &'a NormalGroup<L> {
        self.left
    }

    /// Right adjacent group.
    #[must_use]
    pub const fn right(&self) -> &'a NormalGroup<L> {
        self.right
    }

    /// Proposed fused executable payload.
    #[must_use]
    pub const fn payload(&self) -> &'a L {
        self.payload
    }

    /// Proposed compiled-lens digest.
    #[must_use]
    pub const fn lens_digest(&self) -> &'a [u8] {
        self.lens_digest
    }

    /// Proposed complement-identity envelope.
    #[must_use]
    pub const fn envelope(&self) -> &'a [u8] {
        self.envelope
    }

    /// Shape-specific boundary context bytes, when applicable.
    #[must_use]
    pub const fn context(&self) -> Option<&'a [u8]> {
        self.context
    }
}

/// Opaque evidence that every applicable premise held for one exact fusion.
///
/// There is deliberately no public constructor. The witness is derived and
/// bound by [`FusionCandidate::derive`], so evidence for one pair or proposed
/// payload cannot be replayed for another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionWitness<L> {
    left: NormalGroup<L>,
    right: NormalGroup<L>,
    payload: L,
    lens_digest: Vec<u8>,
    envelope: Vec<u8>,
    context: Option<Vec<u8>>,
}

impl<L> FusionWitness<L> {
    /// Optional boundary context to which this evidence is bound.
    #[must_use]
    pub fn context(&self) -> Option<&[u8]> {
        self.context.as_deref()
    }
}

/// One group in a derived leftmost fusion normal form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalGroup<L> {
    /// Contiguous range of atomic stages represented by this group.
    stages: Range<usize>,
    /// Atomic stage or admitted fused stage executed for this group.
    stage: PathStage<L>,
    /// Digests of the covered atomic stages, in their original order.
    atomic_stage_digests: Vec<Vec<u8>>,
}

impl<L> NormalGroup<L> {
    /// Contiguous atomic-stage range represented by this group.
    #[must_use]
    pub const fn stages(&self) -> &Range<usize> {
        &self.stages
    }

    /// Atomic or fused stage executed for this group.
    #[must_use]
    pub const fn stage(&self) -> &PathStage<L> {
        &self.stage
    }

    /// Digests of the covered atomic stages, in their original order.
    #[must_use]
    pub fn atomic_stage_digests(&self) -> &[Vec<u8>] {
        &self.atomic_stage_digests
    }
}

/// A validated fused group candidate.
///
/// Construction fixes the candidate's endpoints from the two adjacent
/// groups and refuses a false fusion certificate. Thus a predicate cannot
/// label concatenation as fusion merely by returning a payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionCandidate<L> {
    left: NormalGroup<L>,
    right: NormalGroup<L>,
    group: NormalGroup<L>,
    witness: FusionWitness<L>,
}

impl<L: Clone> FusionCandidate<L> {
    /// Derive a candidate from a payload and the compiled fused identity.
    ///
    /// `semantics` evaluates each premise in contract order against the same
    /// immutable input bundle that is retained in the opaque witness. Its
    /// constructor is crate-private, so external predicates cannot replace
    /// the derivation with an all-true oracle.
    ///
    /// # Errors
    ///
    /// The first failed L.6 condition.
    pub fn derive(
        left: &NormalGroup<L>,
        right: &NormalGroup<L>,
        payload: L,
        lens_digest: Vec<u8>,
        envelope: Vec<u8>,
        context: Option<Vec<u8>>,
        semantics: &FusionSemantics<L>,
    ) -> Result<Self, FusionCondition> {
        if left.stages.end != right.stages.start || left.stage.target != right.stage.source {
            return Err(FusionCondition::MigrationComposes);
        }
        let inputs = FusionInputs {
            left,
            right,
            payload: &payload,
            lens_digest: &lens_digest,
            envelope: &envelope,
            context: context.as_deref(),
        };
        for condition in COMMON_FUSION_CONDITIONS {
            if !semantics.holds(condition, &inputs) {
                return Err(condition);
            }
        }
        if context.is_some() && !semantics.holds(FusionCondition::ContextCompiles, &inputs) {
            return Err(FusionCondition::ContextCompiles);
        }
        let mut atomic_stage_digests = left.atomic_stage_digests.clone();
        atomic_stage_digests.extend(right.atomic_stage_digests.iter().cloned());
        let witness = FusionWitness {
            left: left.clone(),
            right: right.clone(),
            payload: payload.clone(),
            lens_digest: lens_digest.clone(),
            envelope: envelope.clone(),
            context,
        };
        Ok(Self {
            left: left.clone(),
            right: right.clone(),
            group: NormalGroup {
                stages: left.stages.start..right.stages.end,
                stage: PathStage {
                    payload,
                    source: left.stage.source.clone(),
                    target: right.stage.target.clone(),
                    lens_digest,
                    envelope,
                },
                atomic_stage_digests,
            },
            witness,
        })
    }

    /// The admitted fusion evidence.
    #[must_use]
    pub const fn witness(&self) -> &FusionWitness<L> {
        &self.witness
    }
}

impl<L: PartialEq> FusionCandidate<L> {
    fn into_group_for(
        self,
        left: &NormalGroup<L>,
        right: &NormalGroup<L>,
        context: Option<&[u8]>,
    ) -> Result<NormalGroup<L>, FusionNormalizationError> {
        let witness_matches = self.witness.left == *left
            && self.witness.right == *right
            && self.witness.payload == self.group.stage.payload
            && self.witness.lens_digest == self.group.stage.lens_digest
            && self.witness.envelope == self.group.stage.envelope;
        if self.witness.context.as_deref() != context {
            Err(FusionNormalizationError::CandidateContextMismatch)
        } else if self.left == *left && self.right == *right && witness_matches {
            Ok(self.group)
        } else {
            Err(FusionNormalizationError::CandidateBindingMismatch)
        }
    }
}

/// A fusion predicate returned evidence for a different adjacent pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusionNormalizationError {
    /// The candidate was not admitted from the pair for which it was returned.
    CandidateBindingMismatch,
    /// The candidate was admitted under another boundary context.
    CandidateContextMismatch,
}

/// Decision for one adjacent pair of normal-form groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FusionDecision<L, O> {
    /// Execute the validated fused candidate.
    Fused(Box<FusionCandidate<L>>),
    /// Keep the groups in sequence and retain the typed reason.
    Sequential(O),
}

/// Shape-specific, deterministic fusion predicate.
pub trait FusionPredicate<L> {
    /// Typed reason that an adjacent pair cannot be fused.
    type Obstruction: Clone;

    /// Exact boundary context under which returned candidates were derived.
    fn context(&self) -> Option<&[u8]>;

    /// Decide one adjacent pair from only those groups and the fixed shape
    /// context captured by the predicate.
    fn decide(
        &self,
        left: &NormalGroup<L>,
        right: &NormalGroup<L>,
    ) -> FusionDecision<L, Self::Obstruction>;
}

/// A rejected adjacent pair in the final irreducible normal form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedFusionObstruction<O> {
    /// Atomic stages of the left group.
    pub left: Range<usize>,
    /// Atomic stages of the right group.
    pub right: Range<usize>,
    /// Shape-specific reason for retaining the sequence.
    pub obstruction: O,
}

/// Result of deterministic leftmost normalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalForm<L, O> {
    /// Executable groups in forward order.
    groups: Vec<NormalGroup<L>>,
    /// Reasons every final adjacent pair remains unfused.
    obstructions: Vec<LocatedFusionObstruction<O>>,
    source: SchemaEndpoint,
    target: SchemaEndpoint,
    atomic_stages: Vec<PathStage<L>>,
}

impl<L, O> NormalForm<L, O> {
    /// Executable groups in forward order.
    #[must_use]
    pub fn groups(&self) -> &[NormalGroup<L>] {
        &self.groups
    }

    /// Typed reasons every final adjacent pair remains sequential.
    #[must_use]
    pub fn obstructions(&self) -> &[LocatedFusionObstruction<O>] {
        &self.obstructions
    }
}

impl<L: PartialEq, O> NormalForm<L, O> {
    fn belongs_to(&self, path: &LensPath<L>) -> bool {
        self.source == *path.src_schema()
            && self.target == *path.tgt_schema()
            && self.atomic_stages == path.stages()
    }
}

impl<L> PathStage<L> {
    /// Construct a typed stage with its opaque lens digest and complement
    /// identity envelope.
    #[must_use]
    pub fn new(
        payload: L,
        source: &Schema,
        target: &Schema,
        lens_digest: Vec<u8>,
        envelope: Vec<u8>,
    ) -> Self {
        Self::new_with_protocols(payload, source, "", target, "", lens_digest, envelope)
    }

    /// Construct a typed stage with explicit boundary protocols.
    #[must_use]
    pub fn new_with_protocols(
        payload: L,
        source: &Schema,
        source_protocol: &str,
        target: &Schema,
        target_protocol: &str,
        lens_digest: Vec<u8>,
        envelope: Vec<u8>,
    ) -> Self {
        Self {
            payload,
            source: SchemaEndpoint::of_protocol(source, source_protocol),
            target: SchemaEndpoint::of_protocol(target, target_protocol),
            lens_digest,
            envelope,
        }
    }

    /// The executable stage payload.
    #[must_use]
    pub const fn payload(&self) -> &L {
        &self.payload
    }

    /// Canonical source endpoint.
    #[must_use]
    pub const fn source(&self) -> &SchemaEndpoint {
        &self.source
    }

    /// Canonical target endpoint.
    #[must_use]
    pub const fn target(&self) -> &SchemaEndpoint {
        &self.target
    }

    /// Opaque compiled-lens digest.
    #[must_use]
    pub fn lens_digest(&self) -> &[u8] {
        &self.lens_digest
    }

    /// Opaque complement identity envelope.
    #[must_use]
    pub fn envelope(&self) -> &[u8] {
        &self.envelope
    }
}

/// Location of an error in the reference's unfused normal form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageIndex {
    /// Zero-based normal-form group index.
    pub group: usize,
    /// Contiguous atomic-stage range covered by the group.
    pub stages: Range<usize>,
}

/// An operation refusal inside one normalized group, located at its atomic
/// path stage.
///
/// A fused executor must identify the atomic stage whose operation would
/// refuse in the sequential word. This makes the public [`PathError`] equal
/// across sequential and normalized execution, including its stage boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormalStageError<E> {
    stage: usize,
    error: E,
}

impl<E> NormalStageError<E> {
    /// Locate `error` at one atomic stage of the normalized group.
    #[must_use]
    pub const fn new(stage: usize, error: E) -> Self {
        Self { stage, error }
    }

    /// Atomic stage carrying the refusal.
    #[must_use]
    pub const fn stage(&self) -> usize {
        self.stage
    }

    /// Refusal payload.
    #[must_use]
    pub const fn error(&self) -> &E {
        &self.error
    }
}

/// Which part of a path-complement entry has the wrong identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathIdentityField {
    /// Source schema endpoint.
    SourceSchema,
    /// Target schema endpoint.
    TargetSchema,
    /// Complement identity envelope.
    Envelope,
    /// Ordered atomic lens digests.
    StageDigests,
}

/// An unlocated error from one atomic stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathStageError<E> {
    /// The supplied complement entry belongs to another path group.
    IdentityMismatch {
        /// First mismatching identity field.
        field: PathIdentityField,
    },
    /// The stage callback refused.
    Operation(E),
}

/// Construction or execution failure of a lens path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError<E> {
    /// A nonidentity path was constructed with no atomic stages.
    EmptyNonIdentity,
    /// Adjacent path endpoints differ.
    CompositionMismatch,
    /// A derived normal form belongs to another atomic path.
    NormalFormMismatch,
    /// A normalized callback located a refusal outside its group.
    InvalidNormalStage {
        /// Atomic stages represented by the group.
        group: Range<usize>,
        /// Reported atomic stage.
        stage: usize,
    },
    /// Complement entry count differs from the normal-form group count.
    PathComplementMismatch {
        /// Expected group count.
        expected: usize,
        /// Supplied entry count.
        found: usize,
    },
    /// Complement representation does not match identity versus stages.
    PathComplementShapeMismatch,
    /// A zero-stage identity complement names another schema endpoint.
    IdentityEndpointMismatch {
        /// Endpoint of the identity path receiving the complement.
        expected: SchemaEndpoint,
        /// Endpoint recorded by get.
        found: SchemaEndpoint,
    },
    /// One normal-form group refused.
    AtStage {
        /// Group and atomic-stage range.
        at: StageIndex,
        /// Unlocated group error.
        error: Box<PathStageError<E>>,
    },
}

/// Path construction cannot produce an operation callback error.
pub type PathConstructionError = PathError<Infallible>;

/// One complement entry produced by an atomic stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathComplementEntry<C> {
    /// Stage complement.
    pub complement: C,
    /// Canonical source endpoint recorded by get.
    pub source: SchemaEndpoint,
    /// Canonical target endpoint recorded by get.
    pub target: SchemaEndpoint,
    /// Complement identity envelope recorded by get.
    pub envelope: Vec<u8>,
    /// Atomic lens digests covered by this group, in order.
    pub stage_digests: Vec<Vec<u8>>,
}

/// A typed identity complement or stamped stage complements in forward
/// group order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathComplement<C> {
    /// Complement of the empty identity word.
    Identity {
        /// Exact schema endpoint at which the identity lives.
        endpoint: SchemaEndpoint,
    },
    /// One entry per atomic stage in the unfused reference normal form.
    Stages(Vec<PathComplementEntry<C>>),
}

impl<C> PathComplement<C> {
    /// Construct a path complement from explicit entries.
    #[must_use]
    pub const fn new(entries: Vec<PathComplementEntry<C>>) -> Self {
        Self::Stages(entries)
    }

    /// Entries in forward execution order.
    #[must_use]
    pub fn entries(&self) -> &[PathComplementEntry<C>] {
        match self {
            Self::Identity { .. } => &[],
            Self::Stages(entries) => entries,
        }
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Identity { .. } => 0,
            Self::Stages(entries) => entries.len(),
        }
    }

    /// Whether this complement has no normal-form group entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether this is a typed complement of the distinguished identity.
    #[must_use]
    pub const fn is_identity_complement(&self) -> bool {
        matches!(self, Self::Identity { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PathWord<L> {
    Identity(SchemaEndpoint),
    Stages(Vec<PathStage<L>>),
}

/// A typed free-category word over atomic stages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LensPath<L> {
    word: PathWord<L>,
}

impl<L> LensPath<L> {
    /// The distinguished identity (empty word) at `schema`.
    #[must_use]
    pub fn identity(schema: &Schema) -> Self {
        Self::identity_with_protocol(schema, "")
    }

    /// The distinguished identity at a structural boundary with its protocol.
    #[must_use]
    pub fn identity_with_protocol(schema: &Schema, protocol: &str) -> Self {
        Self {
            word: PathWord::Identity(SchemaEndpoint::of_protocol(schema, protocol)),
        }
    }

    /// A one-stage nonidentity path.
    #[must_use]
    pub fn single(stage: PathStage<L>) -> Self {
        Self {
            word: PathWord::Stages(vec![stage]),
        }
    }

    /// Construct a nonidentity path, checking nonemptiness and every
    /// adjacent boundary.
    ///
    /// # Errors
    ///
    /// [`PathError::EmptyNonIdentity`] for no stages, or
    /// [`PathError::CompositionMismatch`] for the first incompatible
    /// boundary.
    pub fn try_from_stages(stages: Vec<PathStage<L>>) -> Result<Self, PathConstructionError> {
        if stages.is_empty() {
            return Err(PathError::EmptyNonIdentity);
        }
        if stages
            .windows(2)
            .any(|pair| pair[0].target != pair[1].source)
        {
            return Err(PathError::CompositionMismatch);
        }
        Ok(Self {
            word: PathWord::Stages(stages),
        })
    }

    /// Atomic stages in execution order; empty only for the distinguished
    /// identity.
    #[must_use]
    pub fn stages(&self) -> &[PathStage<L>] {
        match &self.word {
            PathWord::Identity(_) => &[],
            PathWord::Stages(stages) => stages,
        }
    }

    /// Number of atomic stages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stages().len()
    }

    /// Whether this is the distinguished identity's empty word.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(&self.word, PathWord::Identity(_))
    }

    /// Whether this path is the distinguished identity.
    #[must_use]
    pub const fn is_identity(&self) -> bool {
        self.is_empty()
    }

    /// Source schema endpoint.
    #[must_use]
    pub fn src_schema(&self) -> &SchemaEndpoint {
        match &self.word {
            PathWord::Identity(endpoint) => endpoint,
            PathWord::Stages(stages) => &stages[0].source,
        }
    }

    /// Target schema endpoint.
    #[must_use]
    pub fn tgt_schema(&self) -> &SchemaEndpoint {
        match &self.word {
            PathWord::Identity(endpoint) => endpoint,
            PathWord::Stages(stages) => &stages[stages.len() - 1].target,
        }
    }
}

impl<L: Clone> LensPath<L> {
    /// Compose two typed words by boundary-checked concatenation.
    ///
    /// The distinguished identity is eliminated only after its endpoint
    /// has been checked against the adjacent boundary. No fusion occurs.
    ///
    /// # Errors
    ///
    /// [`PathError::CompositionMismatch`] when the shared endpoints
    /// differ.
    pub fn then(&self, next: &Self) -> Result<Self, PathConstructionError> {
        if self.tgt_schema() != next.src_schema() {
            return Err(PathError::CompositionMismatch);
        }
        match (&self.word, &next.word) {
            (PathWord::Identity(_), _) => Ok(next.clone()),
            (_, PathWord::Identity(_)) => Ok(self.clone()),
            (PathWord::Stages(left), PathWord::Stages(right)) => Ok(Self {
                word: PathWord::Stages(left.iter().chain(right).cloned().collect()),
            }),
        }
    }

    /// Derive the deterministic leftmost fusion normal form.
    ///
    /// This is a function of the atomic word, not of its construction
    /// bracketing. It is neither claimed minimal nor canonical up to
    /// behaviour. Every final adjacent pair carries the predicate's typed
    /// reason for remaining sequential.
    ///
    /// # Errors
    ///
    /// [`FusionNormalizationError::CandidateBindingMismatch`] when a
    /// predicate returns a candidate admitted for another adjacent pair.
    pub fn normal_form<P>(
        &self,
        predicate: &P,
    ) -> Result<NormalForm<L, P::Obstruction>, FusionNormalizationError>
    where
        P: FusionPredicate<L>,
        L: PartialEq,
    {
        let mut groups: Vec<_> = self
            .stages()
            .iter()
            .enumerate()
            .map(|(index, stage)| NormalGroup {
                stages: index..index + 1,
                stage: stage.clone(),
                atomic_stage_digests: vec![stage.lens_digest.clone()],
            })
            .collect();

        loop {
            let mut obstructions = Vec::with_capacity(groups.len().saturating_sub(1));
            let mut fused = None;
            for (index, pair) in groups.windows(2).enumerate() {
                match predicate.decide(&pair[0], &pair[1]) {
                    FusionDecision::Fused(candidate) => {
                        let group =
                            (*candidate).into_group_for(&pair[0], &pair[1], predicate.context())?;
                        fused = Some((index, group));
                        break;
                    }
                    FusionDecision::Sequential(obstruction) => {
                        obstructions.push(LocatedFusionObstruction {
                            left: pair[0].stages.clone(),
                            right: pair[1].stages.clone(),
                            obstruction,
                        });
                    }
                }
            }
            let Some((index, group)) = fused else {
                return Ok(NormalForm {
                    groups,
                    obstructions,
                    source: self.src_schema().clone(),
                    target: self.tgt_schema().clone(),
                    atomic_stages: self.stages().to_vec(),
                });
            };
            groups.splice(index..=index + 1, [group]);
        }
    }
}

const fn stage_index(index: usize) -> StageIndex {
    StageIndex {
        group: index,
        stages: index..index + 1,
    }
}

/// Definition: Get through the actual path, left to right, producing one
/// stamped complement entry per atomic stage.
///
/// # Errors
///
/// The first stage refusal, located at its exact atomic group.
///
/// Book anchors: `L-lenses-13`, `L-lenses-14`, `L-lenses-16`.
pub fn get_path<L, I: Clone, C, E>(
    path: &LensPath<L>,
    get: impl Fn(usize, &L, &I) -> Result<(I, C), E>,
    source: &I,
) -> Result<(I, PathComplement<C>), PathError<E>> {
    let mut view = source.clone();
    let mut entries = Vec::with_capacity(path.len());
    for (index, stage) in path.stages().iter().enumerate() {
        let (next, complement) =
            get(index, stage.payload(), &view).map_err(|error| PathError::AtStage {
                at: stage_index(index),
                error: Box::new(PathStageError::Operation(error)),
            })?;
        entries.push(PathComplementEntry {
            complement,
            source: stage.source.clone(),
            target: stage.target.clone(),
            envelope: stage.envelope.clone(),
            stage_digests: vec![stage.lens_digest.clone()],
        });
        view = next;
    }
    let complement = match &path.word {
        PathWord::Identity(endpoint) => PathComplement::Identity {
            endpoint: endpoint.clone(),
        },
        PathWord::Stages(_) => PathComplement::Stages(entries),
    };
    Ok((view, complement))
}

/// Run a derived normal form from left to right.
///
/// The complement has one entry per normal group. Each entry records the
/// fused envelope and the exact atomic digest sequence covered by that
/// group.
///
/// # Errors
///
/// The first group refusal, located by both group and atomic-stage range.
pub fn get_normal_form<L: PartialEq, O, I: Clone, C, E>(
    path: &LensPath<L>,
    normal: &NormalForm<L, O>,
    get: impl Fn(usize, &NormalGroup<L>, &I) -> Result<(I, C), NormalStageError<E>>,
    source: &I,
) -> Result<(I, PathComplement<C>), PathError<E>> {
    if !normal.belongs_to(path) {
        return Err(PathError::NormalFormMismatch);
    }
    if path.is_identity() {
        return Ok((
            source.clone(),
            PathComplement::Identity {
                endpoint: path.src_schema().clone(),
            },
        ));
    }
    let mut view = source.clone();
    let mut entries = Vec::with_capacity(normal.groups.len());
    for (index, group) in normal.groups.iter().enumerate() {
        let (next, complement) = get(index, group, &view).map_err(|located| {
            if group.stages.contains(&located.stage) {
                PathError::AtStage {
                    at: stage_index(located.stage),
                    error: Box::new(PathStageError::Operation(located.error)),
                }
            } else {
                PathError::InvalidNormalStage {
                    group: group.stages.clone(),
                    stage: located.stage,
                }
            }
        })?;
        entries.push(PathComplementEntry {
            complement,
            source: group.stage.source.clone(),
            target: group.stage.target.clone(),
            envelope: group.stage.envelope.clone(),
            stage_digests: group.atomic_stage_digests.clone(),
        });
        view = next;
    }
    Ok((view, PathComplement::Stages(entries)))
}

fn identity_mismatch<L, C>(
    stage: &PathStage<L>,
    entry: &PathComplementEntry<C>,
) -> Option<PathIdentityField> {
    if entry.source != stage.source {
        Some(PathIdentityField::SourceSchema)
    } else if entry.target != stage.target {
        Some(PathIdentityField::TargetSchema)
    } else if entry.envelope != stage.envelope {
        Some(PathIdentityField::Envelope)
    } else if entry.stage_digests.len() != 1 || entry.stage_digests[0] != stage.lens_digest {
        Some(PathIdentityField::StageDigests)
    } else {
        None
    }
}

fn group_identity_mismatch<L, C>(
    group: &NormalGroup<L>,
    entry: &PathComplementEntry<C>,
) -> Option<PathIdentityField> {
    if entry.source != group.stage.source {
        Some(PathIdentityField::SourceSchema)
    } else if entry.target != group.stage.target {
        Some(PathIdentityField::TargetSchema)
    } else if entry.envelope != group.stage.envelope {
        Some(PathIdentityField::Envelope)
    } else if entry.stage_digests != group.atomic_stage_digests {
        Some(PathIdentityField::StageDigests)
    } else {
        None
    }
}

/// One atomic stage's backward operation.
pub type StagePut<'a, L, I, C, E> = &'a dyn Fn(usize, &L, &I, &C) -> Result<I, E>;

/// Definition: Put through the actual path, right to left, after checking
/// the exact group count and each group's path identity.
///
/// # Errors
///
/// [`PathError::PathComplementMismatch`] before execution for a truncated
/// or extra complement. A wrong group stamp or callback refusal is
/// [`PathError::AtStage`] at the rightmost failing group whose right
/// neighbours succeeded.
///
/// Book anchors: `L-lenses-13`, `L-lenses-14`, `L-lenses-16`.
pub fn put_path<L, I: Clone, C, E>(
    path: &LensPath<L>,
    put: StagePut<'_, L, I, C, E>,
    view: &I,
    complement: &PathComplement<C>,
) -> Result<I, PathError<E>> {
    if path.len() != complement.len() {
        return Err(PathError::PathComplementMismatch {
            expected: path.len(),
            found: complement.len(),
        });
    }
    match (&path.word, complement) {
        (PathWord::Identity(expected), PathComplement::Identity { endpoint }) => {
            if expected == endpoint {
                return Ok(view.clone());
            }
            return Err(PathError::IdentityEndpointMismatch {
                expected: expected.clone(),
                found: endpoint.clone(),
            });
        }
        (PathWord::Identity(_), PathComplement::Stages(_))
        | (PathWord::Stages(_), PathComplement::Identity { .. }) => {
            return Err(PathError::PathComplementShapeMismatch);
        }
        (PathWord::Stages(_), PathComplement::Stages(_)) => {}
    }
    let PathComplement::Stages(entries) = complement else {
        unreachable!("complement shape was checked above")
    };
    let mut cur = view.clone();
    for index in (0..path.len()).rev() {
        let stage = &path.stages()[index];
        let entry = &entries[index];
        if let Some(field) = identity_mismatch(stage, entry) {
            return Err(PathError::AtStage {
                at: stage_index(index),
                error: Box::new(PathStageError::IdentityMismatch { field }),
            });
        }
        cur = put(index, stage.payload(), &cur, &entry.complement).map_err(|error| {
            PathError::AtStage {
                at: stage_index(index),
                error: Box::new(PathStageError::Operation(error)),
            }
        })?;
    }
    Ok(cur)
}

/// Run a derived normal form from right to left after validating every
/// group identity at the moment that group is reached.
///
/// # Errors
///
/// A complement shape/count mismatch before execution, or the rightmost
/// reached group with a wrong identity or operation refusal.
pub fn put_normal_form<L: PartialEq, O, I: Clone, C, E>(
    path: &LensPath<L>,
    normal: &NormalForm<L, O>,
    put: impl Fn(usize, &NormalGroup<L>, &I, &C) -> Result<I, NormalStageError<E>>,
    view: &I,
    complement: &PathComplement<C>,
) -> Result<I, PathError<E>> {
    if !normal.belongs_to(path) {
        return Err(PathError::NormalFormMismatch);
    }
    let expected = normal.groups.len();
    if expected != complement.len() {
        return Err(PathError::PathComplementMismatch {
            expected,
            found: complement.len(),
        });
    }
    match (&path.word, complement) {
        (PathWord::Identity(expected), PathComplement::Identity { endpoint }) => {
            if expected == endpoint {
                return Ok(view.clone());
            }
            return Err(PathError::IdentityEndpointMismatch {
                expected: expected.clone(),
                found: endpoint.clone(),
            });
        }
        (PathWord::Identity(_), PathComplement::Stages(_))
        | (PathWord::Stages(_), PathComplement::Identity { .. }) => {
            return Err(PathError::PathComplementShapeMismatch);
        }
        (PathWord::Stages(_), PathComplement::Stages(_)) => {}
    }
    let PathComplement::Stages(entries) = complement else {
        unreachable!("complement shape was checked above")
    };
    let mut source = view.clone();
    for index in (0..normal.groups.len()).rev() {
        let group = &normal.groups[index];
        let entry = &entries[index];
        if let Some(field) = group_identity_mismatch(group, entry) {
            return Err(PathError::AtStage {
                at: StageIndex {
                    group: index,
                    stages: group.stages.clone(),
                },
                error: Box::new(PathStageError::IdentityMismatch { field }),
            });
        }
        source = put(index, group, &source, &entry.complement).map_err(|located| {
            if group.stages.contains(&located.stage) {
                PathError::AtStage {
                    at: stage_index(located.stage),
                    error: Box::new(PathStageError::Operation(located.error)),
                }
            } else {
                PathError::InvalidNormalStage {
                    group: group.stages.clone(),
                    stage: located.stage,
                }
            }
        })?;
    }
    Ok(source)
}
