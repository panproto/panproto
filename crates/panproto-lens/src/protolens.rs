//! Protolenses: schema-parameterized families of lenses.
//!
//! A [`Lens`] is a concrete bidirectional transformation between two
//! *specific* schemas, a pair (`get`, `put`) with complement satisfying
//! the `GetPut` and `PutGet` laws. A protolens is **not** a lens. It is
//! a *dependent function* from schemas to lenses:
//!
//! ```text
//! Protolens ≡ Π(S : Schema | P(S)). Lens(F(S), G(S))
//! ```
//!
//! where `P` is a precondition on schemas, `F` and `G` are theory
//! endofunctors, and the result is a concrete [`Lens`] between `F(S)`
//! and `G(S)`. Calling [`Protolens::instantiate`] applies this
//! dependent function to a specific schema.
//!
//! The practical value is **reusability**: a single protolens works on
//! any schema satisfying its precondition, whereas a `Lens` is bound
//! to the exact schemas it was constructed for.
//!
//! The endofunctor framing (`source: F`, `target: G`) means protolenses
//! have the *structure* of natural transformations. For the elementary
//! constructors this holds by construction, but naturality is not
//! verified at runtime in the current implementation.
//!
//! # Elementary constructors
//!
//! The [`elementary`] module provides atomic protolens constructors:
//!
//! - [`elementary::add_sort`]: `S ↦ Lens(S, S + {τ})`
//! - [`elementary::drop_sort`]: `S ↦ Lens(S, S \ {τ})`
//! - [`elementary::rename_sort`]: `S ↦ Lens(S, S[old/new])`
//! - [`elementary::add_op`]: `S ↦ Lens(S, S + {op})`
//! - [`elementary::drop_op`]: `S ↦ Lens(S, S \ {op})`
//! - [`elementary::rename_op`]: `S ↦ Lens(S, S[old/new])`
//! - [`elementary::add_equation`]: `S ↦ Lens(S, S + {eq})`
//! - [`elementary::drop_equation`]: `S ↦ Lens(S, S \ {eq})`
//! - [`elementary::pullback`]: `S ↦ Lens(S, φ*(S))`
//!
//! # Composition
//!
//! Protolenses compose vertically (sequential) and horizontally
//! (parallel). Vertical composition chains: first apply η to get a
//! lens `S → G(S)`, then apply θ to `G(S)` to get `G(S) → H(G(S))`.
//!
//! - [`vertical_compose`]: `(η, θ) ↦ λS. compose(η(S), θ(G(S)))`
//! - [`horizontal_compose`]: `(η, θ) ↦ λS. η(S) applied in parallel with θ(S)`

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use panproto_gat::{Name, Operation, Sort, Theory, TheoryEndofunctor, TheoryTransform};
use panproto_inst::CompiledMigration;
use panproto_inst::value::Value;
use panproto_schema::{Edge, Protocol, Schema, Vertex};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::Lens;
use crate::error::LensError;

// ---------------------------------------------------------------------------
// Helper: extract the inner Arc<str> from a Name
// ---------------------------------------------------------------------------

/// Clone the inner `Arc<str>` from a `Name`.
#[inline]
fn name_arc_clone(n: &Name) -> Arc<str> {
    Arc::clone(&n.0)
}

/// How the complement type depends on the schema at instantiation time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ComplementConstructor {
    /// Complement is always empty (lossless protolens).
    Empty,
    /// Complement captures dropped sort data.
    DroppedSortData {
        /// The sort whose data is captured.
        sort: Name,
    },
    /// Complement captures dropped edge data.
    DroppedOpData {
        /// The operation whose data is captured.
        op: Name,
    },
    /// Complement captures a single dropped edge (by `(src, tgt, name)` triple).
    ///
    /// Used by `elementary::drop_edge` to record enough information to
    /// restore the specific edge in the `put` direction.
    DroppedEdge {
        /// Source vertex id of the dropped edge.
        src: Name,
        /// Target vertex id of the dropped edge.
        tgt: Name,
        /// Label of the dropped edge.
        edge_name: Option<Name>,
        /// Kind of the dropped edge (recorded so `put` can re-add it).
        edge_kind: Name,
    },
    /// Complement is the kernel of a natural transformation.
    NatTransKernel {
        /// Name of the natural transformation.
        nat_trans_name: Name,
    },
    /// Forward direction requires a default for an added element.
    AddedElement {
        /// Name of the element being added.
        element_name: Name,
        /// What kind of element (e.g. `"string"`, `"record"`).
        element_kind: String,
        /// Default value to use when instantiating.
        default_value: Option<Value>,
    },
    /// Complement captures coerced sort data with a specific round-trip class.
    CoercedSortData {
        /// The sort whose values are coerced.
        sort: Name,
        /// The round-trip classification of the coercion.
        class: panproto_gat::CoercionClass,
    },
    /// Composite complement from a chain.
    Composite(Vec<Self>),
    /// Scoped complement: the inner complement is tracked per-element
    /// when the focus vertex is reached via an array (item) edge.
    ///
    /// For prop edges (single focus), the inner complement is applied once.
    /// For item edges (traversal), a list of inner complements is built,
    /// one per array element. This is the dependent product in the slice
    /// topos: `C(s) = Π_{i : elements(s)} C_inner(element_i)`.
    Scoped {
        /// The focus vertex name.
        focus: Name,
        /// The inner complement constructor.
        inner: Box<Self>,
    },
    /// Names a schema-enrichment fibre and the synthesis driver
    /// registered to populate it.
    ///
    /// This variant is descriptive only: the `Complement` struct in
    /// [`crate::asymmetric`] holds `WInstance`-level discarded data
    /// (dropped nodes, dropped arcs, contraction choices) and does
    /// not have a per-vertex constraint-fibre field. For protolenses
    /// whose source / target endofunctors are
    /// [`TheoryTransform::StripEnrichment`] / `AddEnrichment`, the
    /// schema-level fibre-shuffling happens in
    /// `apply_theory_transform_to_schema` (via the registered
    /// [`LayoutEnricher`](crate::enrichment_registry::LayoutEnricher));
    /// the operational entry points for the parse/decorate/emit lens
    /// live in `panproto-parse` rather than the asymmetric
    /// `get` / `put` pair.
    Enrichment {
        /// The enrichment fibre being captured.
        kind: panproto_gat::EnrichmentKind,
        /// Name of the registered synthesis driver, looked up in the
        /// `enrichment_registry` (e.g. a grammar name for `Layout`).
        enricher: Arc<str>,
    },
}

/// A protolens: a dependent function from schemas to lenses.
///
/// A `Protolens` is **not** a lens. A [`Lens`] is a concrete pair
/// (`get`, `put`) between two fixed schemas. A `Protolens` is a
/// *function* that, given any schema satisfying its precondition,
/// *produces* a `Lens`.
///
/// ```text
/// Protolens ≡ Π(S : Schema | P(S)). Lens(F(S), G(S))
/// ```
///
/// where `F` (source) and `G` (target) are theory endofunctors.
/// The key operation is [`instantiate`](Self::instantiate), which
/// applies this dependent function to a specific schema.
///
/// The endofunctor framing means protolenses have the structure of
/// natural transformations (each elementary constructor is natural
/// by construction), but naturality is not runtime-verified.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Protolens {
    /// Human-readable name.
    pub name: Name,
    /// Source endofunctor `F`.
    pub source: TheoryEndofunctor,
    /// Target endofunctor `G`.
    pub target: TheoryEndofunctor,
    /// How the complement type depends on the schema.
    pub complement_constructor: ComplementConstructor,
}

impl Protolens {
    /// Check if this protolens can be instantiated at the given schema.
    ///
    /// A protolens applies when the source endofunctor's precondition
    /// is satisfied by the schema's implicit theory (vertex kinds as
    /// sorts, edge kinds as operations).
    #[must_use]
    pub fn applicable_to(&self, schema: &Schema) -> bool {
        self.check_applicability(schema).is_ok()
    }

    /// Check applicability with failure reasons.
    ///
    /// # Errors
    ///
    /// Returns a list of human-readable reasons why the precondition
    /// is not satisfied.
    pub fn check_applicability(&self, schema: &Schema) -> Result<(), Vec<String>> {
        let constraint = SchemaConstraint::from_theory_constraint(&self.source.precondition);
        let reasons = constraint.check(schema);
        if reasons.is_empty() {
            Ok(())
        } else {
            Err(reasons)
        }
    }

    /// Instantiate this protolens at a specific schema, producing a concrete
    /// [`Lens`].
    ///
    /// This is Π-type elimination: applying the dependent function to a
    /// specific schema. Callers that need the source precondition —
    /// including any precondition retained from vertical composition with
    /// an `Identity`-source protolens — enforced should gate on
    /// [`Self::check_applicability`] first; `instantiate` itself applies
    /// the transforms directly, so a not-applicable protolens (e.g. one
    /// dropping a sort the schema lacks) instantiates to a no-op lens
    /// rather than erroring.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if either endofunctor's
    /// transform fails to apply.
    pub fn instantiate(&self, schema: &Schema, protocol: &Protocol) -> Result<Lens, LensError> {
        // 1. Compute source schema: F(S)
        let src_schema = if matches!(self.source.transform, TheoryTransform::Identity) {
            schema.clone()
        } else {
            apply_theory_transform_to_schema(&self.source.transform, schema, protocol)?
        };

        // 2. Compute target schema: G(S)
        let tgt_schema =
            apply_theory_transform_to_schema(&self.target.transform, schema, protocol)?;

        // 3. Compute the migration from F(S) to G(S)
        let compiled = compute_migration_between(&src_schema, &tgt_schema);

        Ok(Lens {
            compiled,
            src_schema,
            tgt_schema,
        })
    }

    /// Instantiate this protolens as an [`crate::EditLens`] at a specific schema.
    ///
    /// This is a convenience method that calls [`instantiate`](Self::instantiate)
    /// and wraps the result with [`crate::EditLens::from_lens`].
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if instantiation fails.
    pub fn instantiate_edit(
        &self,
        schema: &Schema,
        protocol: &Protocol,
    ) -> Result<crate::EditLens, LensError> {
        let base_lens = self.instantiate(schema, protocol)?;
        Ok(crate::EditLens::from_lens(base_lens, protocol.clone()))
    }

    /// Compute the target schema without building a full lens.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if the target transform
    /// cannot be applied.
    pub fn target_schema(&self, schema: &Schema, protocol: &Protocol) -> Result<Schema, LensError> {
        apply_theory_transform_to_schema(&self.target.transform, schema, protocol)
    }

    /// Return the optic kind this protolens classifies as.
    ///
    /// Equivalent to [`crate::optic::classify_transform`] applied to
    /// `self.target.transform`, but discoverable from the [`Protolens`]
    /// type itself so callers don't need to reach into internal fields.
    #[must_use]
    pub fn optic_kind(&self) -> crate::optic::OpticKind {
        crate::optic::classify_transform(&self.target.transform)
    }

    /// Returns `true` if this protolens produces lossless lenses
    /// (empty complement).
    #[must_use]
    pub const fn is_lossless(&self) -> bool {
        matches!(
            self.complement_constructor,
            ComplementConstructor::Empty
                | ComplementConstructor::CoercedSortData {
                    class: panproto_gat::CoercionClass::Iso,
                    ..
                }
        )
    }

    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl ProtolensChain {
    /// Serialize to JSON.
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// A predicate on schemas for precondition checking.
///
/// Checks schema structure directly, unlike `TheoryConstraint` which
/// operates on the implicit theory extracted from a schema (lossy).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SchemaConstraint {
    /// Any schema satisfies this.
    Unconstrained,
    /// Schema must have at least one vertex of this kind.
    HasVertexKind(Name),
    /// Schema must have a vertex with this name.
    HasVertex(Name),
    /// Schema must have at least one edge of this kind.
    HasEdgeKind(Name),
    /// Schema must have an edge between these vertices.
    HasEdgeBetween {
        /// Source vertex name.
        src: Name,
        /// Target vertex name.
        tgt: Name,
    },
    /// Delegate to a theory-level constraint on the implicit theory.
    Theory(panproto_gat::TheoryConstraint),
    /// Conjunction: all sub-constraints must hold.
    All(Vec<Self>),
    /// Disjunction: at least one sub-constraint must hold.
    Any(Vec<Self>),
    /// Negation: the sub-constraint must not hold.
    Not(Box<Self>),
}

impl SchemaConstraint {
    /// Check if a schema satisfies this constraint.
    #[must_use]
    pub fn satisfied_by(&self, schema: &Schema) -> bool {
        match self {
            Self::Unconstrained => true,
            Self::HasVertexKind(kind) => schema.vertices.values().any(|v| v.kind == *kind),
            Self::HasVertex(name) => schema.vertices.contains_key(name),
            Self::HasEdgeKind(kind) => schema.edges.keys().any(|e| e.kind == *kind),
            Self::HasEdgeBetween { src, tgt } => {
                schema.edges.keys().any(|e| e.src == *src && e.tgt == *tgt)
            }
            Self::Theory(tc) => {
                let implicit = schema_to_implicit_theory(schema);
                tc.satisfied_by(&implicit)
            }
            Self::All(cs) => cs.iter().all(|c| c.satisfied_by(schema)),
            Self::Any(cs) => cs.iter().any(|c| c.satisfied_by(schema)),
            Self::Not(c) => !c.satisfied_by(schema),
        }
    }

    /// Return human-readable reasons why this constraint is NOT satisfied.
    /// Empty vec if satisfied.
    #[must_use]
    pub fn check(&self, schema: &Schema) -> Vec<String> {
        match self {
            Self::Unconstrained => vec![],
            Self::HasVertexKind(kind) => {
                if schema.vertices.values().any(|v| v.kind == *kind) {
                    vec![]
                } else {
                    vec![format!("Schema has no vertex of kind '{kind}'.")]
                }
            }
            Self::HasVertex(name) => {
                if schema.vertices.contains_key(name) {
                    vec![]
                } else {
                    vec![format!("Schema has no vertex named '{name}'.")]
                }
            }
            Self::HasEdgeKind(kind) => {
                if schema.edges.keys().any(|e| e.kind == *kind) {
                    vec![]
                } else {
                    vec![format!("Schema has no edge of kind '{kind}'.")]
                }
            }
            Self::HasEdgeBetween { src, tgt } => {
                if schema.edges.keys().any(|e| e.src == *src && e.tgt == *tgt) {
                    vec![]
                } else {
                    vec![format!("Schema has no edge from '{src}' to '{tgt}'.")]
                }
            }
            Self::Theory(tc) => {
                let implicit = schema_to_implicit_theory(schema);
                if tc.satisfied_by(&implicit) {
                    vec![]
                } else {
                    vec![format!("TheoryConstraint not satisfied: {tc:?}")]
                }
            }
            Self::All(cs) => cs.iter().flat_map(|c| c.check(schema)).collect(),
            Self::Any(cs) => {
                if cs.iter().any(|c| c.satisfied_by(schema)) {
                    vec![]
                } else {
                    let reasons: Vec<String> = cs.iter().flat_map(|c| c.check(schema)).collect();
                    vec![format!(
                        "None of the alternatives were satisfied: {}",
                        reasons.join("; ")
                    )]
                }
            }
            Self::Not(c) => {
                if c.satisfied_by(schema) {
                    vec![format!("Constraint should NOT be satisfied but is: {c:?}")]
                } else {
                    vec![]
                }
            }
        }
    }

    /// Lift a `TheoryConstraint` to a `SchemaConstraint`.
    #[must_use]
    pub fn from_theory_constraint(tc: &panproto_gat::TheoryConstraint) -> Self {
        match tc {
            panproto_gat::TheoryConstraint::Unconstrained => Self::Unconstrained,
            panproto_gat::TheoryConstraint::HasSort(name) => {
                Self::HasVertexKind(Name::from(&**name))
            }
            panproto_gat::TheoryConstraint::HasOp(name) => Self::HasEdgeKind(Name::from(&**name)),
            panproto_gat::TheoryConstraint::HasEquation(name) => Self::Theory(
                panproto_gat::TheoryConstraint::HasEquation(Arc::clone(name)),
            ),
            panproto_gat::TheoryConstraint::All(cs) => {
                Self::All(cs.iter().map(Self::from_theory_constraint).collect())
            }
            panproto_gat::TheoryConstraint::Any(cs) => {
                Self::Any(cs.iter().map(Self::from_theory_constraint).collect())
            }
            panproto_gat::TheoryConstraint::Not(c) => {
                Self::Not(Box::new(Self::from_theory_constraint(c)))
            }
            // Enriched theory constraints delegate to the theory-level checker.
            other @ (panproto_gat::TheoryConstraint::HasDirectedEq(_)
            | panproto_gat::TheoryConstraint::HasValSort(_)
            | panproto_gat::TheoryConstraint::HasCoercion { .. }
            | panproto_gat::TheoryConstraint::HasMerger(_)
            | panproto_gat::TheoryConstraint::HasPolicy(_)) => Self::Theory(other.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------

/// Structural equivalence of theory endofunctors, ignoring `name`.
///
/// Two endofunctors are equivalent when their preconditions and
/// transforms are structurally equal. This is the relation used to
/// verify natural-transformation composability.
#[must_use]
pub fn theory_endofunctor_equiv(a: &TheoryEndofunctor, b: &TheoryEndofunctor) -> bool {
    a.precondition == b.precondition && a.transform == b.transform
}

/// Fold two theory constraints into a conjunction, flattening nested
/// `All`s, dropping `Unconstrained` operands, and deduplicating.
///
/// The result is order-stable — `base`'s atoms first, then `extra`'s —
/// so conjoining preconditions during vertical composition stays
/// associative: `(a·b)·c` and `a·(b·c)` yield the same flattened
/// conjunction.
fn conjoin_preconditions(
    base: &panproto_gat::TheoryConstraint,
    extra: &panproto_gat::TheoryConstraint,
) -> panproto_gat::TheoryConstraint {
    fn push_atoms(
        c: &panproto_gat::TheoryConstraint,
        out: &mut Vec<panproto_gat::TheoryConstraint>,
    ) {
        match c {
            panproto_gat::TheoryConstraint::Unconstrained => {}
            panproto_gat::TheoryConstraint::All(cs) => {
                for sub in cs {
                    push_atoms(sub, out);
                }
            }
            other => {
                if !out.iter().any(|x| x == other) {
                    out.push(other.clone());
                }
            }
        }
    }

    let mut atoms = Vec::new();
    push_atoms(base, &mut atoms);
    push_atoms(extra, &mut atoms);
    match atoms.len() {
        0 => panproto_gat::TheoryConstraint::Unconstrained,
        1 => atoms.swap_remove(0),
        _ => panproto_gat::TheoryConstraint::All(atoms),
    }
}

/// Composability predicate for [`Protolens`] in vertical composition.
///
/// Two protolenses `η : F ⟹ G` and `θ : H ⟹ K` are composable when
/// one of:
///
/// 1. `G ≡ H` as theory endofunctors (genuine natural-transformation
///    composition; the categorical guarantee).
/// 2. `H.transform = Identity` (θ is constructed as "applied at the
///    running schema": its source endofunctor is the identity, so for
///    every schema `S` we have `H(G(S)) = G(S)` and θ's migration
///    starts from `G(S)` directly).
///
/// Case (2) is a *schema-level* composability — it asserts that the
/// resulting lens is well-typed at the schema boundary, but it is
/// strictly weaker than the natural-transformation condition on the
/// transform component alone. An `Identity`-source θ may carry a
/// non-trivial `theta.source.precondition` (e.g. `HasSort`); this
/// predicate does not inspect it, but [`vertical_compose`] retains it
/// by conjoining it into the composed source endofunctor's
/// precondition, so the retained obligation is surfaced by
/// [`Protolens::check_applicability`] (the gate the fleet and chain
/// APIs consult) rather than silently dropped. Naturality squares are
/// still not certified to commute on every schema.
///
/// Code that relies on the *categorical* guarantee should use
/// [`theory_endofunctor_equiv`] directly. Code that relies on the
/// *operational* guarantee across a multi-step chain should use
/// [`ProtolensChain::check_applicability_with`], which threads the
/// running schema through every step and re-evaluates each step's
/// precondition against the intermediate schema it is actually
/// presented with.
///
/// `vertical_compose` returning `Ok` means "the schema-level types
/// align and θ's precondition is retained"; it does not certify
/// naturality squares commute on every schema.
#[must_use]
pub fn protolens_composable(eta: &Protolens, theta: &Protolens) -> bool {
    matches!(theta.source.transform, TheoryTransform::Identity)
        || theory_endofunctor_equiv(&eta.target, &theta.source)
}

/// Vertical composition of protolenses: given `η : F ⟹ G` and
/// `θ : G ⟹ H`, produce `θ ∘ η : F ⟹ H`.
///
/// Composition requires [`protolens_composable`]: either `eta.target ≡
/// theta.source` as theory endofunctors (the standard natural
/// transformation condition), or `theta.source.transform = Identity`
/// (θ applied at the running schema).
///
/// In the `Identity`-source case θ carries its own source precondition.
/// The composed protolens keeps `eta.source` as its source endofunctor,
/// so that precondition is retained by conjoining it into the composed
/// source precondition rather than dropped: [`Protolens::applicable_to`]
/// / [`Protolens::check_applicability`] on the composite report `false` /
/// `Err` at a schema that fails θ's precondition.
///
/// # Errors
///
/// Returns [`LensError::CompositionMismatch`] when the intermediate
/// endofunctors do not agree.
pub fn vertical_compose(eta: &Protolens, theta: &Protolens) -> Result<Protolens, LensError> {
    if !protolens_composable(eta, theta) {
        return Err(LensError::CompositionMismatch);
    }

    // Retain θ's source precondition in the `Identity`-source case; it
    // would otherwise be silently discarded because the composite's
    // source endofunctor is `eta.source`.
    let mut source = eta.source.clone();
    if matches!(theta.source.transform, TheoryTransform::Identity) {
        source.precondition =
            conjoin_preconditions(&eta.source.precondition, &theta.source.precondition);
    }

    let complement = ComplementConstructor::Composite(vec![
        eta.complement_constructor.clone(),
        theta.complement_constructor.clone(),
    ]);

    Ok(Protolens {
        name: Name::from(format!("{}.{}", theta.name, eta.name)),
        source,
        target: theta.target.clone(),
        complement_constructor: complement,
    })
}

/// Horizontal composition of protolenses: given `η : F ⟹ G` and
/// `θ : F' ⟹ G'`, produce `η * θ : F∘F' ⟹ G∘G'`.
///
/// # Errors
///
/// Currently infallible, but returns `Result` for future compatibility
/// with static compatibility checks.
pub fn horizontal_compose(eta: &Protolens, theta: &Protolens) -> Result<Protolens, LensError> {
    let source = eta.source.compose(&theta.source);
    let target = eta.target.compose(&theta.target);
    let complement = ComplementConstructor::Composite(vec![
        eta.complement_constructor.clone(),
        theta.complement_constructor.clone(),
    ]);

    Ok(Protolens {
        name: Name::from(format!("{}*{}", theta.name, eta.name)),
        source,
        target,
        complement_constructor: complement,
    })
}

// ---------------------------------------------------------------------------
// ProtolensChain
// ---------------------------------------------------------------------------

/// A chain of protolenses for vertical composition.
///
/// Each step's target endofunctor feeds into the next step's source.
/// Instantiating the chain at a schema produces a composed lens.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtolensChain {
    /// The individual protolens steps.
    pub steps: Vec<Protolens>,
}

impl ProtolensChain {
    /// Create a new chain from steps.
    #[must_use]
    pub const fn new(steps: Vec<Protolens>) -> Self {
        Self { steps }
    }

    /// Return the composed optic kind of all steps in this chain.
    ///
    /// Each step's optic kind is composed via [`crate::optic::OpticKind::compose`].
    /// An empty chain returns [`crate::optic::OpticKind::Iso`] (the identity
    /// of composition).
    #[must_use]
    pub fn composed_optic_kind(&self) -> crate::optic::OpticKind {
        self.steps.iter().map(Protolens::optic_kind).fold(
            crate::optic::OpticKind::Iso,
            crate::optic::OpticKind::compose,
        )
    }

    /// Check if the chain can be instantiated at the given schema.
    ///
    /// An empty chain (identity) is applicable to any schema. Otherwise,
    /// every step must be applicable at the schema produced by the
    /// preceding step's target endofunctor (or the initial schema for
    /// the first step). This requires a `Protocol` to apply intermediate
    /// transforms.
    #[must_use]
    pub fn applicable_to_with(&self, schema: &Schema, protocol: &Protocol) -> bool {
        self.check_applicability_with(schema, protocol).is_ok()
    }

    /// Cheap pre-flight applicability check that only consults the first
    /// step's precondition. Use [`Self::applicable_to_with`] when a
    /// [`Protocol`] is in hand and per-step checks against the running
    /// schema are required.
    #[must_use]
    pub fn applicable_to(&self, schema: &Schema) -> bool {
        if self.steps.is_empty() {
            return true;
        }
        self.steps[0].applicable_to(schema)
    }

    /// Instantiate the chain at a specific schema, producing a composed
    /// [`Lens`] via fused single-pass migration computation.
    ///
    /// All steps are collapsed into one composite endofunctor whose
    /// migration is then computed in one pass. This preserves
    /// migration-level metadata (e.g. `expansion_path`) that sequential
    /// composition aggregates only partially. The fused form and the
    /// sequential form agree under the round-trip lens laws — the
    /// sequential form is exposed as [`Self::instantiate_sequential`]
    /// and is exercised by property tests in the laws module.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if any step fails or the
    /// chain is empty (in the multi-step case), or
    /// [`LensError::CompositionMismatch`] if adjacent steps'
    /// endofunctors are not composable.
    pub fn instantiate(&self, schema: &Schema, protocol: &Protocol) -> Result<Lens, LensError> {
        if self.steps.is_empty() {
            return Ok(identity_lens(schema));
        }
        if self.steps.len() == 1 {
            return self.steps[0].instantiate(schema, protocol);
        }
        let fused = self.fuse()?;
        fused.instantiate(schema, protocol)
    }

    /// Sequential instantiation: instantiate each step at the running
    /// schema and fold the resulting lenses via
    /// [`crate::compose::compose`]. Exercises the lens laws end-to-end
    /// on real intermediate states. Some migration metadata that the
    /// fused form computes globally (e.g. `expansion_path`) is not
    /// recovered by sequential composition; for that reason
    /// [`Self::instantiate`] uses the fused form by default.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if any step's transform
    /// fails, or [`LensError::CompositionMismatch`] if adjacent steps'
    /// endofunctors are not composable.
    pub fn instantiate_sequential(
        &self,
        schema: &Schema,
        protocol: &Protocol,
    ) -> Result<Lens, LensError> {
        if self.steps.is_empty() {
            return Ok(identity_lens(schema));
        }
        if self.steps.len() == 1 {
            return self.steps[0].instantiate(schema, protocol);
        }

        for window in self.steps.windows(2) {
            if !protolens_composable(&window[0], &window[1]) {
                return Err(LensError::CompositionMismatch);
            }
        }

        let mut running_schema = schema.clone();
        let mut steps_iter = self.steps.iter();
        // Bootstrap with the first step (chain is non-empty by the
        // early-return above).
        let first = steps_iter.next().ok_or_else(|| {
            LensError::ProtolensError("chain unexpectedly empty after non-empty check".into())
        })?;
        if !first.applicable_to(&running_schema) {
            return Err(LensError::ProtolensError(format!(
                "step `{}` not applicable to running schema",
                first.name,
            )));
        }
        let mut composed: Lens = first.instantiate(&running_schema, protocol)?;
        running_schema = composed.tgt_schema.clone();
        for step in steps_iter {
            if !step.applicable_to(&running_schema) {
                return Err(LensError::ProtolensError(format!(
                    "step `{}` not applicable to running schema",
                    step.name,
                )));
            }
            let step_lens = step.instantiate(&running_schema, protocol)?;
            running_schema = step_lens.tgt_schema.clone();
            composed = crate::compose::compose(&composed, &step_lens)?;
        }
        Ok(composed)
    }

    /// Instantiate this chain as an [`crate::EditLens`] at a specific schema.
    ///
    /// This is a convenience method that calls [`instantiate`](Self::instantiate)
    /// and wraps the result with [`crate::EditLens::from_lens`].
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if instantiation fails.
    pub fn instantiate_edit(
        &self,
        schema: &Schema,
        protocol: &Protocol,
    ) -> Result<crate::EditLens, LensError> {
        let base_lens = self.instantiate(schema, protocol)?;
        Ok(crate::EditLens::from_lens(base_lens, protocol.clone()))
    }

    /// Returns `true` if the chain is empty (identity).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Check if the chain can be instantiated at the given schema,
    /// returning failure reasons on error.
    ///
    /// An empty chain (identity) is applicable to any schema. Otherwise,
    /// the first step must be applicable.
    ///
    /// # Errors
    ///
    /// Returns a `Vec<String>` of reasons if the chain's precondition
    /// is not satisfied by the schema.
    pub fn check_applicability(&self, schema: &Schema) -> Result<(), Vec<String>> {
        if self.steps.is_empty() {
            return Ok(());
        }
        self.steps[0].check_applicability(schema)
    }

    /// Like [`Self::check_applicability`] but threads the running schema
    /// through every step so each step's precondition is checked
    /// against the schema actually presented to it.
    ///
    /// # Errors
    ///
    /// Returns reasons for the first step that fails. Adjacent-step
    /// endofunctor disagreement is reported as a single reason.
    pub fn check_applicability_with(
        &self,
        schema: &Schema,
        protocol: &Protocol,
    ) -> Result<(), Vec<String>> {
        if self.steps.is_empty() {
            return Ok(());
        }
        for window in self.steps.windows(2) {
            if !protolens_composable(&window[0], &window[1]) {
                return Err(vec![format!(
                    "adjacent steps disagree: `{}.target` ≢ `{}.source`",
                    window[0].name, window[1].name,
                )]);
            }
        }
        let mut running = schema.clone();
        for step in &self.steps {
            step.check_applicability(&running)?;
            running = step
                .target_schema(&running, protocol)
                .map_err(|e| vec![format!("step `{}` transform failed: {e}", step.name)])?;
        }
        Ok(())
    }

    /// Fuse all steps into a single `Protolens` by composing endofunctors.
    ///
    /// The fused protolens applies all transforms in one pass, avoiding
    /// intermediate schema materialization. The complement constructor
    /// becomes `Composite` of all individual complements.
    ///
    /// Adjacent endofunctors must agree: for every pair `(stepᵢ,
    /// stepᵢ₊₁)`, `stepᵢ.target ≡ stepᵢ₊₁.source` (structural equality
    /// modulo name).
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if the chain is empty, or
    /// [`LensError::CompositionMismatch`] if adjacent endofunctors
    /// disagree.
    pub fn fuse(&self) -> Result<Protolens, LensError> {
        if self.steps.is_empty() {
            return Err(LensError::ProtolensError("cannot fuse empty chain".into()));
        }
        if self.steps.len() == 1 {
            return Ok(self.steps[0].clone());
        }

        for window in self.steps.windows(2) {
            if !protolens_composable(&window[0], &window[1]) {
                return Err(LensError::CompositionMismatch);
            }
        }

        let source = self.steps[0].source.clone();

        // Compose all target transforms into a single Compose tree
        let mut combined_transform = self.steps[0].target.transform.clone();
        for step in &self.steps[1..] {
            combined_transform = TheoryTransform::Compose(
                Box::new(combined_transform),
                Box::new(step.target.transform.clone()),
            );
        }

        let target = TheoryEndofunctor {
            name: Arc::from(
                self.steps
                    .iter()
                    .map(|s| s.target.name.to_string())
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            precondition: source.precondition.clone(),
            transform: combined_transform,
        };

        let sub_complements: Vec<_> = self
            .steps
            .iter()
            .map(|s| s.complement_constructor.clone())
            .collect();
        let complement = if sub_complements
            .iter()
            .all(|c| matches!(c, ComplementConstructor::Empty))
        {
            ComplementConstructor::Empty
        } else {
            ComplementConstructor::Composite(sub_complements)
        };

        let name = Name::from(
            self.steps
                .iter()
                .map(|s| s.name.to_string())
                .collect::<Vec<_>>()
                .join("."),
        );

        Ok(Protolens {
            name,
            source,
            target,
            complement_constructor: complement,
        })
    }
}

// ---------------------------------------------------------------------------
// Fleet API
// ---------------------------------------------------------------------------

/// Result of applying a protolens chain to a fleet of schemas.
pub struct FleetResult {
    /// Schemas where the chain was successfully instantiated.
    pub applied: Vec<(Name, Lens)>,
    /// Schemas that were skipped, with reasons.
    pub skipped: Vec<(Name, Vec<String>)>,
}

/// Apply a protolens chain to every schema in a fleet.
///
/// For each `(name, schema)` pair, checks applicability. If the chain's
/// precondition is satisfied, instantiates to produce a lens. Otherwise
/// collects the schema name and failure reasons in `skipped`.
#[must_use]
pub fn apply_to_fleet(
    chain: &ProtolensChain,
    schemas: &[(Name, Schema)],
    protocol: &Protocol,
) -> FleetResult {
    let mut applied = Vec::new();
    let mut skipped = Vec::new();

    for (name, schema) in schemas {
        let check = if chain.steps.is_empty() {
            Ok(())
        } else {
            chain.steps[0].check_applicability(schema)
        };

        match check {
            Err(reasons) => {
                skipped.push((name.clone(), reasons));
            }
            Ok(()) => match chain.instantiate(schema, protocol) {
                Ok(lens) => applied.push((name.clone(), lens)),
                Err(e) => skipped.push((name.clone(), vec![format!("instantiation failed: {e}")])),
            },
        }
    }

    FleetResult { applied, skipped }
}

// ---------------------------------------------------------------------------
// Functorial Lifting
// ---------------------------------------------------------------------------

/// Lift a theory constraint along a morphism.
///
/// Renames sort/op references according to the morphism's maps.
fn lift_constraint(
    constraint: &panproto_gat::TheoryConstraint,
    morphism: &panproto_gat::TheoryMorphism,
) -> panproto_gat::TheoryConstraint {
    use panproto_gat::TheoryConstraint as TC;
    match constraint {
        TC::Unconstrained => TC::Unconstrained,
        TC::HasSort(s) => {
            let lifted = morphism.sort_map.get(s).unwrap_or(s);
            TC::HasSort(Arc::clone(lifted))
        }
        TC::HasOp(o) => {
            let lifted = morphism
                .op_map
                .get(o)
                .and_then(panproto_gat::OpAssignment::as_op)
                .unwrap_or(o);
            TC::HasOp(Arc::clone(lifted))
        }
        TC::HasEquation(e) => TC::HasEquation(Arc::clone(e)),
        TC::All(cs) => TC::All(cs.iter().map(|c| lift_constraint(c, morphism)).collect()),
        TC::Any(cs) => TC::Any(cs.iter().map(|c| lift_constraint(c, morphism)).collect()),
        TC::Not(c) => TC::Not(Box::new(lift_constraint(c, morphism))),
        // Enriched constraints pass through unchanged.
        TC::HasDirectedEq(_)
        | TC::HasValSort(_)
        | TC::HasCoercion { .. }
        | TC::HasMerger(_)
        | TC::HasPolicy(_) => constraint.clone(),
    }
}

/// Lift a theory endofunctor along a morphism.
fn lift_endofunctor(
    ef: &TheoryEndofunctor,
    morphism: &panproto_gat::TheoryMorphism,
) -> TheoryEndofunctor {
    let lifted_precondition = lift_constraint(&ef.precondition, morphism);
    let pullback_transform = TheoryTransform::Pullback(morphism.clone());
    let lifted_transform = if matches!(ef.transform, TheoryTransform::Identity) {
        pullback_transform
    } else {
        TheoryTransform::Compose(Box::new(pullback_transform), Box::new(ef.transform.clone()))
    };

    TheoryEndofunctor {
        name: Arc::from(format!("{}[{}]", ef.name, morphism.name)),
        precondition: lifted_precondition,
        transform: lifted_transform,
    }
}

/// Lift a protolens along a theory morphism.
///
/// Given protolens `η` and morphism `φ : T1 → T2`, produces a protolens
/// that operates on schemas of T2 instead of T1. The endofunctors are
/// composed with the morphism's renames, and the precondition is lifted
/// (sort/op references renamed according to the morphism).
#[must_use]
pub fn lift_protolens(protolens: &Protolens, morphism: &panproto_gat::TheoryMorphism) -> Protolens {
    Protolens {
        name: Name::from(format!("{}[{}]", protolens.name, morphism.name)),
        source: lift_endofunctor(&protolens.source, morphism),
        target: lift_endofunctor(&protolens.target, morphism),
        complement_constructor: protolens.complement_constructor.clone(),
    }
}

/// Lift an entire protolens chain along a theory morphism.
#[must_use]
pub fn lift_chain(
    chain: &ProtolensChain,
    morphism: &panproto_gat::TheoryMorphism,
) -> ProtolensChain {
    ProtolensChain::new(
        chain
            .steps
            .iter()
            .map(|s| lift_protolens(s, morphism))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Elementary protolens constructors
// ---------------------------------------------------------------------------

/// Built-in protolens constructors: the "atoms" from which all
/// protolenses are composed.
pub mod elementary {
    use panproto_gat::{
        DirectedEquation, Equation, Name, Operation, Sort, TheoryConstraint, TheoryEndofunctor,
        TheoryMorphism, TheoryTransform,
    };
    use panproto_inst::value::Value;
    use std::sync::Arc;

    use super::{ComplementConstructor, Protolens, name_arc_clone};

    /// Short, lowercase slug for a [`panproto_gat::ValueKind`] used to
    /// disambiguate `sort_coerce` protolens names by target carrier.
    /// Kept in sync with `panproto_mig::coerce::value_kind_label` but
    /// intentionally private to avoid a cross-crate dependency.
    const fn value_kind_slug(kind: panproto_gat::ValueKind) -> &'static str {
        match kind {
            panproto_gat::ValueKind::Bool => "bool",
            panproto_gat::ValueKind::Int => "int",
            panproto_gat::ValueKind::Float => "float",
            panproto_gat::ValueKind::Str => "str",
            panproto_gat::ValueKind::Bytes => "bytes",
            panproto_gat::ValueKind::Token => "token",
            panproto_gat::ValueKind::Null => "null",
            panproto_gat::ValueKind::Any => "any",
        }
    }

    /// `η : Id ⟹ AddSort(τ, d)`: for each `S`, `η_S` is a lens
    /// `S → S+{τ}` that adds a vertex kind with default.
    #[must_use]
    pub fn add_sort(
        sort_name: impl Into<Name>,
        vertex_kind: impl Into<Name>,
        default: Value,
    ) -> Protolens {
        let sort_name = sort_name.into();
        let vertex_kind = vertex_kind.into();
        Protolens {
            name: Name::from(format!("add_sort_{sort_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{sort_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddSort {
                    sort: Sort::simple(name_arc_clone(&sort_name)),
                    vertex_kind: Some(Arc::from(&*vertex_kind)),
                },
            },
            complement_constructor: ComplementConstructor::AddedElement {
                element_name: sort_name,
                element_kind: format!("{vertex_kind}"),
                default_value: Some(default),
            },
        }
    }

    /// `η : Id ⟹ AddSortWithDefault(τ, d_expr)`: adds a vertex kind and
    /// carries a symbolic default expression (evaluated downstream) so the
    /// zero-element of the pushout is not lost.
    ///
    /// The theory-level payload retains the original `panproto_expr::Expr`;
    /// this is the variant factorization emits when the source schema
    /// does not witness the new sort but a default expression was attached
    /// to it.
    #[must_use]
    pub fn add_sort_with_default(
        sort_name: impl Into<Name>,
        vertex_kind: impl Into<Name>,
        default_expr: panproto_expr::Expr,
    ) -> Protolens {
        let sort_name = sort_name.into();
        let vertex_kind = vertex_kind.into();
        Protolens {
            name: Name::from(format!("add_sort_with_default_{sort_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{sort_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddSortWithDefault {
                    sort: Sort::simple(name_arc_clone(&sort_name)),
                    vertex_kind: Some(Arc::from(&*vertex_kind)),
                    default_expr,
                },
            },
            // Data-level default is evaluated from the expression at
            // migration time; store `None` here to avoid a stale cached
            // value ever diverging from the source-of-truth expression.
            complement_constructor: ComplementConstructor::AddedElement {
                element_name: sort_name,
                element_kind: format!("{vertex_kind}"),
                default_value: None,
            },
        }
    }

    /// `η : Id ⟹ DropSort(τ)`: for each `S` containing sort `τ`,
    /// `η_S` is a lens `S → S \ {τ}`.
    #[must_use]
    pub fn drop_sort(sort_name: impl Into<Name>) -> Protolens {
        let sort_name = sort_name.into();
        let arc = name_arc_clone(&sort_name);
        Protolens {
            name: Name::from(format!("drop_sort_{sort_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasSort(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_{sort_name}")),
                precondition: TheoryConstraint::HasSort(Arc::clone(&arc)),
                transform: TheoryTransform::DropSort(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::DroppedSortData { sort: sort_name },
        }
    }

    /// `η : Id ⟹ RenameSort(old, new)`: for each `S` containing sort
    /// `old`, `η_S` is a lossless lens `S → S[old↦new]`.
    #[must_use]
    pub fn rename_sort(old: impl Into<Name>, new: impl Into<Name>) -> Protolens {
        let old = old.into();
        let new = new.into();
        let old_arc = name_arc_clone(&old);
        let new_arc = name_arc_clone(&new);
        Protolens {
            name: Name::from(format!("rename_sort_{old}_{new}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasSort(Arc::clone(&old_arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("rename_{old}")),
                precondition: TheoryConstraint::HasSort(Arc::clone(&old_arc)),
                transform: TheoryTransform::RenameSort {
                    old: old_arc,
                    new: new_arc,
                },
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ AddOp(op)`: adds an operation to the theory.
    #[must_use]
    pub fn add_op(
        op_name: impl Into<Name>,
        src_sort: impl Into<Name>,
        tgt_sort: impl Into<Name>,
        kind: impl Into<Name>,
    ) -> Protolens {
        let op_name = op_name.into();
        let src_sort = src_sort.into();
        let tgt_sort = tgt_sort.into();
        let kind = kind.into();
        Protolens {
            name: Name::from(format!("add_op_{op_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{op_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddOp(Operation::unary(
                    name_arc_clone(&op_name),
                    name_arc_clone(&kind),
                    name_arc_clone(&src_sort),
                    name_arc_clone(&tgt_sort),
                )),
            },
            complement_constructor: ComplementConstructor::AddedElement {
                element_name: op_name,
                element_kind: format!("{kind}"),
                default_value: None,
            },
        }
    }

    /// `η : Id ⟹ DropOp(op)`: drops an operation from the theory.
    #[must_use]
    pub fn drop_op(op_name: impl Into<Name>) -> Protolens {
        let op_name = op_name.into();
        let arc = name_arc_clone(&op_name);
        Protolens {
            name: Name::from(format!("drop_op_{op_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasOp(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_{op_name}")),
                precondition: TheoryConstraint::HasOp(Arc::clone(&arc)),
                transform: TheoryTransform::DropOp(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::DroppedOpData { op: op_name },
        }
    }

    /// `η : Id ⟹ AddEdge(src, tgt, name, kind)`: adds a single edge.
    ///
    /// Fiber-level operation: the underlying theory is unchanged (edges
    /// sharing a kind share a single theory op), only schema metadata is
    /// extended. Unlike [`add_op`], which forces the new edge's label to
    /// equal its kind, `add_edge` lets the caller specify label and kind
    /// independently, which is necessary for schemas with qualified
    /// vertex ids where edge labels are short JSON keys.
    #[must_use]
    pub fn add_edge(
        src_sort: impl Into<Name>,
        tgt_sort: impl Into<Name>,
        edge_name: impl Into<Name>,
        edge_kind: impl Into<Name>,
    ) -> Protolens {
        let src_sort = src_sort.into();
        let tgt_sort = tgt_sort.into();
        let edge_name = edge_name.into();
        let edge_kind = edge_kind.into();
        let src_arc = name_arc_clone(&src_sort);
        let tgt_arc = name_arc_clone(&tgt_sort);
        let name_arc = name_arc_clone(&edge_name);
        let kind_arc = name_arc_clone(&edge_kind);
        Protolens {
            name: Name::from(format!("add_edge_{src_sort}_{tgt_sort}_{edge_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_edge_{edge_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddEdge {
                    src_sort: src_arc,
                    tgt_sort: tgt_arc,
                    edge_name: name_arc,
                    edge_kind: kind_arc,
                },
            },
            complement_constructor: ComplementConstructor::AddedElement {
                element_name: edge_name,
                element_kind: format!("{edge_kind}"),
                default_value: None,
            },
        }
    }

    /// `η : Id ⟹ DropEdge(src, tgt, name)`: drops a single edge by
    /// its `(src, tgt, name)` triple.
    ///
    /// Fiber-level operation: the underlying theory is unchanged. Unlike
    /// [`drop_op`], which removes every edge of a given kind, `drop_edge`
    /// targets a specific edge instance. The complement captures the
    /// dropped edge's kind so `put` can restore it.
    #[must_use]
    pub fn drop_edge(
        src_sort: impl Into<Name>,
        tgt_sort: impl Into<Name>,
        edge_name: Option<Name>,
    ) -> Protolens {
        let src_sort = src_sort.into();
        let tgt_sort = tgt_sort.into();
        let src_arc = name_arc_clone(&src_sort);
        let tgt_arc = name_arc_clone(&tgt_sort);
        let name_arc: Option<Arc<str>> = edge_name.as_ref().map(name_arc_clone);
        let label_display = edge_name
            .as_ref()
            .map_or_else(|| "unnamed".to_string(), ToString::to_string);
        Protolens {
            name: Name::from(format!("drop_edge_{src_sort}_{tgt_sort}_{label_display}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_edge_{label_display}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::DropEdge {
                    src_sort: src_arc,
                    tgt_sort: tgt_arc,
                    edge_name: name_arc,
                },
            },
            // The actual dropped edge's kind is filled in at instantiate
            // time (via schema inspection) and stored on the Lens complement.
            // For the `ComplementConstructor` we only record the targeting
            // tuple; the kind is looked up in `apply_drop_edge_from_schema`.
            complement_constructor: ComplementConstructor::DroppedEdge {
                src: src_sort,
                tgt: tgt_sort,
                edge_name,
                edge_kind: Name::from(""),
            },
        }
    }

    /// `η : Id ⟹ RenameOp(old, new)`: renames an operation.
    #[must_use]
    pub fn rename_op(old: impl Into<Name>, new: impl Into<Name>) -> Protolens {
        let old = old.into();
        let new = new.into();
        let old_arc = name_arc_clone(&old);
        let new_arc = name_arc_clone(&new);
        Protolens {
            name: Name::from(format!("rename_op_{old}_{new}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasOp(Arc::clone(&old_arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("rename_{old}")),
                precondition: TheoryConstraint::HasOp(Arc::clone(&old_arc)),
                transform: TheoryTransform::RenameOp {
                    old: old_arc,
                    new: new_arc,
                },
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ AddEquation(eq)`: adds an equation (constraint).
    #[must_use]
    pub fn add_equation(eq: Equation) -> Protolens {
        let eq_name = Arc::clone(&eq.name);
        Protolens {
            name: Name::from(format!("add_eq_{eq_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_{eq_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddEquation(eq),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ DropEquation(eq_name)`: drops an equation.
    #[must_use]
    pub fn drop_equation(eq_name: impl Into<Name>) -> Protolens {
        let eq_name = eq_name.into();
        let arc = name_arc_clone(&eq_name);
        Protolens {
            name: Name::from(format!("drop_eq_{eq_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasEquation(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_{eq_name}")),
                precondition: TheoryConstraint::HasEquation(Arc::clone(&arc)),
                transform: TheoryTransform::DropEquation(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// Pullback along a theory morphism.
    #[must_use]
    pub fn pullback(morphism: TheoryMorphism) -> Protolens {
        let morph_name = Arc::clone(&morphism.name);
        Protolens {
            name: Name::from(format!("pullback_{morph_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("pullback_{morph_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Pullback(morphism),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// Add a directed equation (lax natural transformation component).
    ///
    /// A protolens with a directed equation is a lax natural transformation:
    /// the naturality square commutes up to the directed equation's
    /// computation. The `impl_term` provides the forward direction; the
    /// complement captures the pre-image when the inverse is absent.
    #[must_use]
    pub fn directed_eq(deq: DirectedEquation) -> Protolens {
        let deq_name = Arc::clone(&deq.name);
        let has_inverse = deq.inverse.is_some();
        Protolens {
            name: Name::from(format!("directed_eq_{deq_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("add_deq_{deq_name}")),
                precondition: TheoryConstraint::Unconstrained,
                transform: TheoryTransform::AddDirectedEquation(deq),
            },
            complement_constructor: if has_inverse {
                ComplementConstructor::Empty
            } else {
                ComplementConstructor::DroppedOpData {
                    op: Name::from(&*deq_name),
                }
            },
        }
    }

    /// Honesty-checked [`directed_eq`].
    ///
    /// Verifies the directed equation's declared
    /// [`CoercionClass`](panproto_gat::CoercionClass) against samples drawn
    /// from `registry` (using `deq.source_kind`) before building the
    /// protolens.
    ///
    /// This is the construction-time coercion-honesty gate: a `directed_eq`
    /// whose `impl_term` / `inverse` do not round-trip under its declared
    /// class on the supplied samples is rejected here rather than being
    /// silently accepted. The plain [`directed_eq`] remains the unchecked
    /// escape hatch. The check is *evidence, not proof* — see
    /// [`crate::coercion_laws::check_coercion_honesty`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::coercion_laws::CoercionHonestyError`] when the
    /// declared class fails its round-trip laws on the drawn samples.
    pub fn directed_eq_checked(
        deq: DirectedEquation,
        var_name: &str,
        registry: &crate::coercion_laws::CoercionSampleRegistry,
    ) -> Result<Protolens, crate::coercion_laws::CoercionHonestyError> {
        let violations =
            crate::coercion_laws::check_directed_equation_with_registry(&deq, registry, var_name);
        if !violations.is_empty() {
            return Err(crate::coercion_laws::CoercionHonestyError {
                class: deq.coercion_class,
                violations,
            });
        }
        Ok(directed_eq(deq))
    }

    /// Drop a directed equation.
    #[must_use]
    pub fn drop_directed_eq(deq_name: impl Into<Name>) -> Protolens {
        let deq_name = deq_name.into();
        let arc = name_arc_clone(&deq_name);
        Protolens {
            name: Name::from(format!("drop_deq_{deq_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasDirectedEq(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("drop_deq_{deq_name}")),
                precondition: TheoryConstraint::HasDirectedEq(Arc::clone(&arc)),
                transform: TheoryTransform::DropDirectedEquation(Arc::clone(&arc)),
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ RenameEdgeName(src, tgt, old, new)`: rename a JSON
    /// property key (edge label) without changing the theory structure.
    ///
    /// This is a fiber-level natural isomorphism in the Grothendieck
    /// fibration: the theory and schema graph structure are unchanged,
    /// only the `name` attribute on the edge between `src_sort` and
    /// `tgt_sort` is relabeled. Always classified as `Iso` (empty
    /// complement, bijective relabeling).
    #[must_use]
    pub fn rename_edge_name(
        src_sort: impl Into<Name>,
        tgt_sort: impl Into<Name>,
        old_name: impl Into<Name>,
        new_name: impl Into<Name>,
    ) -> Protolens {
        let src_sort = src_sort.into();
        let tgt_sort = tgt_sort.into();
        let old_name = old_name.into();
        let new_name = new_name.into();
        let src_arc = name_arc_clone(&src_sort);
        let tgt_arc = name_arc_clone(&tgt_sort);
        let old_arc = name_arc_clone(&old_name);
        let new_arc = name_arc_clone(&new_name);
        Protolens {
            name: Name::from(format!("rename_edge_{old_name}_{new_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::All(vec![
                    TheoryConstraint::HasSort(Arc::clone(&src_arc)),
                    TheoryConstraint::HasSort(Arc::clone(&tgt_arc)),
                ]),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("rename_edge_{old_name}_{new_name}")),
                precondition: TheoryConstraint::All(vec![
                    TheoryConstraint::HasSort(Arc::clone(&src_arc)),
                    TheoryConstraint::HasSort(Arc::clone(&tgt_arc)),
                ]),
                transform: TheoryTransform::RenameEdgeName {
                    src_sort: src_arc,
                    tgt_sort: tgt_arc,
                    old_name: old_arc,
                    new_name: new_arc,
                },
            },
            complement_constructor: ComplementConstructor::Empty,
        }
    }

    /// `η : Id ⟹ CoerceSort(S ↦ T, ℓ)`: apply a witness lens to every
    /// value of sort `S`.
    ///
    /// For each schema `S'` containing sort `S`, `η_{S'}` is a lens
    /// that runs the witness `ℓ = (forward, inverse)` pointwise over
    /// values of sort `S`, producing an instance over `S'[S ↦ T]`.
    ///
    /// Categorically this is pushout-along-`SortLens`: the theory is
    /// rewritten by substituting `S` with `T` everywhere, and the
    /// instance-level change is witnessed by the Cambria-style lens
    /// `ℓ`. Round-trip fidelity is classified by
    /// [`CoercionClass`](panproto_gat::CoercionClass):
    ///
    /// - `Iso`: `ℓ.inverse(ℓ.forward(v)) = v` AND
    ///   `ℓ.forward(ℓ.inverse(w)) = w`. Complement is empty.
    /// - `Retraction`: `ℓ.inverse(ℓ.forward(v)) = v` but the other
    ///   direction may not hold. Complement captures the residual so
    ///   `put` can recover the original value.
    /// - `Projection`: neither direction round-trips without external
    ///   data; the forward image is a function of the source, but no
    ///   inverse recovers the source. Complement stores the original
    ///   value.
    ///
    /// The CSP / naturality check enforces that every op mentioning
    /// `S` has an interpretation in the pushed-out theory that
    /// commutes with `ℓ`; callers in `panproto-mig::coerce` perform
    /// this check before emitting a `CoerceSort` endofunctor.
    #[must_use]
    pub fn sort_coerce(
        sort_name: impl Into<Name>,
        target_kind: panproto_gat::ValueKind,
        coercion_expr: panproto_expr::Expr,
        inverse_expr: Option<panproto_expr::Expr>,
        coercion_class: panproto_gat::CoercionClass,
    ) -> Protolens {
        let sort_name = sort_name.into();
        let arc = name_arc_clone(&sort_name);
        // For Iso witnesses the lens is lossless in both directions,
        // so the complement is empty. For every other class we need
        // to retain the dropped carrier data (Retraction / Projection
        // / Opaque) so `put` can recover the source value.
        let complement_constructor = if matches!(coercion_class, panproto_gat::CoercionClass::Iso) {
            ComplementConstructor::Empty
        } else {
            ComplementConstructor::CoercedSortData {
                sort: sort_name.clone(),
                class: coercion_class,
            }
        };
        // Include the target kind in the protolens name so that two
        // witnesses bridging the same source sort to different carriers
        // (e.g. `n: int → str` vs `n: int → float`) yield distinct
        // protolens identities. Without this tag, downstream consumers
        // that key on `Protolens::name` would conflate the two.
        let target_kind_label = value_kind_slug(target_kind);
        Protolens {
            name: Name::from(format!("sort_coerce_{sort_name}_to_{target_kind_label}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasSort(Arc::clone(&arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("coerce_{sort_name}_to_{target_kind_label}")),
                precondition: TheoryConstraint::HasSort(Arc::clone(&arc)),
                transform: TheoryTransform::CoerceSort {
                    sort_name: Arc::clone(&arc),
                    target_kind,
                    coercion_expr,
                    inverse_expr,
                    coercion_class,
                },
            },
            complement_constructor,
        }
    }

    /// Honesty-checked [`sort_coerce`].
    ///
    /// Verifies the declared
    /// [`CoercionClass`](panproto_gat::CoercionClass) round-trips on samples
    /// of `source_kind` (drawn from `registry`, bound under `var_name`)
    /// before building the protolens.
    ///
    /// This is the construction-time coercion-honesty gate. A `CoerceSort`
    /// declaring `Iso` with a non-invertible `coercion_expr` (or a
    /// `Retraction` whose inverse is not a left inverse on the samples)
    /// is rejected here rather than accepted everywhere. The plain
    /// [`sort_coerce`] remains the unchecked escape hatch. The check is
    /// *evidence, not proof* — see
    /// [`crate::coercion_laws::check_coercion_honesty`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::coercion_laws::CoercionHonestyError`] when the
    /// declared class fails its round-trip laws on the drawn samples.
    #[allow(clippy::too_many_arguments)]
    pub fn sort_coerce_checked(
        sort_name: impl Into<Name>,
        target_kind: panproto_gat::ValueKind,
        coercion_expr: panproto_expr::Expr,
        inverse_expr: Option<panproto_expr::Expr>,
        coercion_class: panproto_gat::CoercionClass,
        source_kind: panproto_gat::ValueKind,
        var_name: &str,
        registry: &crate::coercion_laws::CoercionSampleRegistry,
    ) -> Result<Protolens, crate::coercion_laws::CoercionHonestyError> {
        crate::coercion_laws::check_coercion_honesty(
            &coercion_expr,
            inverse_expr.as_ref(),
            coercion_class,
            source_kind,
            var_name,
            registry,
        )?;
        Ok(sort_coerce(
            sort_name,
            target_kind,
            coercion_expr,
            inverse_expr,
            coercion_class,
        ))
    }

    /// `η : Id ⟹ Scope(focus, inner)`: apply a protolens within the
    /// sub-schema rooted at the focus vertex.
    ///
    /// Categorically, this is the left Kan extension of the inner
    /// protolens along the inclusion `ι : Sub(S, focus) ↪ S`.
    ///
    /// At the instance level, the optic class depends on the edge kind
    /// connecting the parent to the focus vertex:
    ///   - `prop` edge → Lens (apply once, single-element focus)
    ///   - `item` edge → Traversal (apply per array element)
    ///   - `variant` edge → Prism (apply if variant present)
    ///
    /// The complement is indexed by the focus: for traversals (item edges),
    /// a list of per-element inner complements is built.
    #[must_use]
    pub fn scoped(focus: impl Into<Name>, inner: Protolens) -> Protolens {
        let focus = focus.into();
        let focus_arc = name_arc_clone(&focus);
        let inner_name = inner.name;
        let inner_transform = inner.target.transform;
        let inner_complement = inner.complement_constructor;
        Protolens {
            name: Name::from(format!("scoped_{focus}_{inner_name}")),
            source: TheoryEndofunctor {
                name: Arc::from("id"),
                precondition: TheoryConstraint::HasSort(Arc::clone(&focus_arc)),
                transform: TheoryTransform::Identity,
            },
            target: TheoryEndofunctor {
                name: Arc::from(&*format!("scope_{focus}")),
                precondition: TheoryConstraint::HasSort(Arc::clone(&focus_arc)),
                transform: TheoryTransform::ScopedTransform {
                    focus: focus_arc,
                    inner: Box::new(inner_transform),
                },
            },
            complement_constructor: ComplementConstructor::Scoped {
                focus,
                inner: Box::new(inner_complement),
            },
        }
    }
}

/// Derived lens combinators composed from elementary protolens operations.
///
/// Each combinator constructs a [`ProtolensChain`] from elementary steps.
/// Composition preserves lens laws by naturality: each step satisfies
/// `GetPut`/`PutGet`, and sequential composition of lawful lenses is lawful.
pub mod combinators {
    use panproto_gat::Name;
    use panproto_inst::value::Value;

    use super::ProtolensChain;
    use super::elementary;

    /// Rename a field's JSON property key.
    ///
    /// This renames the edge label (the `name` attribute on the edge from
    /// `parent` to `field`) which controls the JSON property key during
    /// serialization. The vertex ID and kind are unchanged: the rename
    /// operates purely at the fiber level of the Grothendieck fibration.
    ///
    /// Categorically, this is a natural isomorphism on the fiber category
    /// over the base theory. The result is an `Iso` (lossless, empty complement).
    ///
    /// The `field` parameter is the vertex ID of the field being renamed
    /// (i.e., the target of the edge from `parent`).
    #[must_use]
    pub fn rename_field(
        parent: impl Into<Name>,
        field: impl Into<Name>,
        old_name: impl Into<Name>,
        new_name: impl Into<Name>,
    ) -> ProtolensChain {
        let parent = parent.into();
        let field = field.into();
        let old_name = old_name.into();
        let new_name = new_name.into();
        ProtolensChain::new(vec![elementary::rename_edge_name(
            parent, field, old_name, new_name,
        )])
    }

    /// Remove a field (drop a sort and its incoming edges).
    ///
    /// The complement captures the dropped vertex data.
    #[must_use]
    pub fn remove_field(field: impl Into<Name>) -> ProtolensChain {
        let field = field.into();
        ProtolensChain::new(vec![elementary::drop_sort(field)])
    }

    /// Add a field with a default value.
    ///
    /// The complement records the default so that `put` can restore the
    /// source instance without the added field.
    #[must_use]
    pub fn add_field(
        parent: impl Into<Name>,
        field_name: impl Into<Name>,
        field_kind: impl Into<Name>,
        default: Value,
    ) -> ProtolensChain {
        let parent = parent.into();
        let field_name = field_name.into();
        let field_kind = field_kind.into();
        ProtolensChain::new(vec![
            elementary::add_sort(field_name.clone(), field_kind, default),
            elementary::add_op(field_name.clone(), parent, field_name.clone(), field_name),
        ])
    }

    /// Hoist a nested field up one level, collapsing the intermediate vertex.
    ///
    /// Given a path `parent →(e₁) intermediate →(e₂) child`, this produces
    /// `parent →(e') child` by adding a direct edge and dropping the
    /// intermediate sort (which cascades removal of its incident edges).
    ///
    /// The complement captures the intermediate vertex data and any other
    /// children of the intermediate that are not the hoisted child.
    #[must_use]
    pub fn hoist_field(
        parent: impl Into<Name>,
        intermediate: impl Into<Name>,
        child: impl Into<Name>,
    ) -> ProtolensChain {
        let parent = parent.into();
        let intermediate = intermediate.into();
        let child = child.into();
        ProtolensChain::new(vec![
            // First add the direct edge from parent to child.
            elementary::add_op(child.clone(), parent, child.clone(), child),
            // Then drop the intermediate, which cascades its edges.
            elementary::drop_sort(intermediate),
        ])
    }

    /// Nest a direct child under a new intermediate vertex.
    ///
    /// Given an existing edge `parent --(old_edge_name)--> child`, produces
    /// `parent --(parent_to_intermediate)--> new_intermediate --(intermediate_to_child)--> child`
    /// by inserting a new intermediate vertex and relocating the original
    /// edge as two edges through the intermediate.
    ///
    /// The new edges carry `edge_kind` as their kind (typically `"prop"`),
    /// while their labels (JSON property keys) are taken from
    /// `parent_to_intermediate` and `intermediate_to_child` respectively.
    /// This lets the combinator work correctly on schemas where vertex ids
    /// are path-qualified (e.g., `user.name`) and therefore distinct from
    /// short edge labels (e.g., `"name"`).
    ///
    /// The original direct edge is identified by the
    /// `(parent, child, old_edge_name)` triple and removed via
    /// [`elementary::drop_edge`]. Pass `None` as `old_edge_name` if the
    /// original edge had no label.
    ///
    /// The `child` vertex id and the edge labels are independent: the child
    /// vertex id need not equal the edge label, and the original edge is
    /// dropped by name (via the triple above) rather than by edge *kind*.
    /// This holds for schemas built via `SchemaBuilder::add_prop`, `ATProto`
    /// lexicons, or any protocol where edge labels are short JSON keys and
    /// vertex ids are path-qualified.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn nest_field(
        parent: impl Into<Name>,
        child: impl Into<Name>,
        new_intermediate: impl Into<Name>,
        intermediate_kind: impl Into<Name>,
        edge_kind: impl Into<Name>,
        old_edge_name: Option<Name>,
        parent_to_intermediate: impl Into<Name>,
        intermediate_to_child: impl Into<Name>,
    ) -> ProtolensChain {
        let parent = parent.into();
        let child = child.into();
        let new_intermediate = new_intermediate.into();
        let intermediate_kind = intermediate_kind.into();
        let edge_kind = edge_kind.into();
        let parent_to_intermediate = parent_to_intermediate.into();
        let intermediate_to_child = intermediate_to_child.into();
        ProtolensChain::new(vec![
            // 1. Add the new intermediate vertex.
            elementary::add_sort(new_intermediate.clone(), intermediate_kind, Value::Null),
            // 2. parent --(parent_to_intermediate, kind=edge_kind)--> new_intermediate
            elementary::add_edge(
                parent.clone(),
                new_intermediate.clone(),
                parent_to_intermediate,
                edge_kind.clone(),
            ),
            // 3. new_intermediate --(intermediate_to_child, kind=edge_kind)--> child
            elementary::add_edge(
                new_intermediate,
                child.clone(),
                intermediate_to_child,
                edge_kind,
            ),
            // 4. Drop the original parent --(old_edge_name)--> child edge.
            elementary::drop_edge(parent, child, old_edge_name),
        ])
    }

    /// Build a pipeline from a sequence of protolens chains.
    ///
    /// Flattens all steps into a single `ProtolensChain`. This is
    /// vertical composition: the target schema of each chain feeds
    /// into the source of the next.
    #[must_use]
    pub fn pipeline(chains: Vec<ProtolensChain>) -> ProtolensChain {
        let steps = chains.into_iter().flat_map(|c| c.steps).collect();
        ProtolensChain::new(steps)
    }

    /// Apply a protolens to each element of an array.
    ///
    /// Wraps the inner protolens in a `scoped` transform targeting the
    /// given focus vertex (the array element's schema vertex). At the
    /// instance level, this produces a traversal: the inner lens is
    /// applied independently to each array element, with per-element
    /// complement tracking.
    #[must_use]
    pub fn map_items(focus: impl Into<Name>, inner: super::Protolens) -> super::Protolens {
        elementary::scoped(focus, inner)
    }
}

/// Build a [`CompiledMigration`] between two schemas by comparing their
/// structures.
fn compute_migration_between(src: &Schema, tgt: &Schema) -> CompiledMigration {
    let mut surviving_verts: HashSet<Name> = src
        .vertices
        .keys()
        .filter(|v| tgt.vertices.contains_key(&**v))
        .cloned()
        .collect();

    let surviving_edges: HashSet<Edge> = src
        .edges
        .keys()
        .filter(|e| tgt.edges.contains_key(*e))
        .cloned()
        .collect();

    // Build vertex remap: vertices in src that were renamed in tgt.
    let mut vertex_remap = HashMap::new();
    let unmapped_src: Vec<&Name> = src
        .vertices
        .keys()
        .filter(|v| !tgt.vertices.contains_key(&**v))
        .collect();
    let unmapped_tgt: Vec<&Name> = tgt
        .vertices
        .keys()
        .filter(|v| !src.vertices.contains_key(&**v))
        .collect();

    // Match unmapped vertices by structural similarity (same kind).
    for src_id in &unmapped_src {
        if let Some(src_v) = src.vertices.get(*src_id) {
            for tgt_id in &unmapped_tgt {
                if let Some(tgt_v) = tgt.vertices.get(*tgt_id) {
                    if src_v.kind == tgt_v.kind
                        && !vertex_remap.values().any(|v: &Name| v == *tgt_id)
                    {
                        // Renamed vertex survives: add TARGET name to surviving_verts
                        // (wtype_restrict checks target_anchor against surviving_verts)
                        surviving_verts.insert((*tgt_id).clone());
                        vertex_remap.insert((*src_id).clone(), (*tgt_id).clone());
                        break;
                    }
                }
            }
        }
    }

    // Include remapped vertices in surviving set
    let mut final_surviving = surviving_verts;
    for src_id in vertex_remap.keys() {
        final_surviving.insert(src_id.clone());
    }

    // Build resolver for edges between surviving vertices in target
    let mut resolver = HashMap::new();
    for edge in tgt.edges.keys() {
        let src_in =
            final_surviving.contains(&edge.src) || vertex_remap.values().any(|v| *v == edge.src);
        let tgt_in =
            final_surviving.contains(&edge.tgt) || vertex_remap.values().any(|v| *v == edge.tgt);
        if src_in && tgt_in {
            resolver.insert((edge.src.clone(), edge.tgt.clone()), edge.clone());
        }
    }

    // Detect expansion paths: direct arcs `(A, B)` that existed in the
    // source schema but are no longer present in the target, yet reachable
    // via a multi-hop path through vertices newly introduced in the
    // target. This is the forward-eval side of `combinators::nest_field`.
    let expansion_path = compute_expansion_paths(src, tgt);

    CompiledMigration {
        surviving_verts: final_surviving,
        surviving_edges,
        vertex_remap,
        edge_remap: HashMap::new(),
        resolver,
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path,
    }
}

/// Detect `(src_parent, src_child)` pairs that had a direct arc in the
/// source schema but only a multi-hop path in the target, and record the
/// sequence of intermediate target anchor ids to insert during forward
/// evaluation.
///
/// The algorithm:
///
/// 1. Collect vertex ids that exist in `tgt` but not `src` (the "new"
///    intermediates introduced by nest-style transforms).
/// 2. For every `(u, v)` pair such that `src` has an edge `u -> v` that
///    no longer exists in `tgt`, BFS outward from `u` in `tgt`, walking
///    only through new intermediates, until `v` is reached.
/// 3. Record the intermediate path (endpoints excluded) in
///    `expansion_path[(u, v)]`.
fn compute_expansion_paths(src: &Schema, tgt: &Schema) -> HashMap<(Name, Name), Vec<Name>> {
    let mut paths: HashMap<(Name, Name), Vec<Name>> = HashMap::new();

    // Vertices added by the migration (present in tgt but not src).
    // These are the only vertices we'll route through when synthesizing
    // an expansion path; otherwise we'd pick up pre-existing paths that
    // were never meant as nest intermediates.
    let new_in_tgt: HashSet<Name> = tgt
        .vertices
        .keys()
        .filter(|v| !src.vertices.contains_key(*v))
        .cloned()
        .collect();

    if new_in_tgt.is_empty() {
        return paths;
    }

    // Source vertex pairs that had a direct arc.
    let mut src_pairs: HashSet<(Name, Name)> = HashSet::new();
    for edge in src.edges.keys() {
        src_pairs.insert((edge.src.clone(), edge.tgt.clone()));
    }

    // Target vertex pairs that still have a direct arc.
    let mut tgt_pairs: HashSet<(Name, Name)> = HashSet::new();
    for edge in tgt.edges.keys() {
        tgt_pairs.insert((edge.src.clone(), edge.tgt.clone()));
    }

    for (src_v, tgt_v) in src_pairs {
        // Only consider pairs that survived as vertices in tgt (otherwise
        // the arc has nothing to expand into).
        if !tgt.vertices.contains_key(&src_v) || !tgt.vertices.contains_key(&tgt_v) {
            continue;
        }
        // If the direct arc still exists in tgt, no expansion is needed.
        if tgt_pairs.contains(&(src_v.clone(), tgt_v.clone())) {
            continue;
        }
        // BFS from `src_v` to `tgt_v` in tgt, restricted to interior hops
        // through new-in-tgt intermediates.
        if let Some(intermediates) = bfs_through_new(tgt, &src_v, &tgt_v, &new_in_tgt) {
            paths.insert((src_v, tgt_v), intermediates);
        }
    }

    paths
}

/// BFS in `tgt` from `start` to `end`, allowing interior hops only
/// through vertices in `new_verts`. Returns the interior of the path
/// (start and end excluded). Returns `None` if no such path exists or
/// if the only path is a direct arc (in which case no expansion is
/// needed and the caller handles it via `resolve_edge`).
fn bfs_through_new(
    tgt: &Schema,
    start: &Name,
    end: &Name,
    new_verts: &HashSet<Name>,
) -> Option<Vec<Name>> {
    use std::collections::VecDeque;

    let mut prev: HashMap<Name, Name> = HashMap::new();
    let mut visited: HashSet<Name> = HashSet::new();
    let mut queue: VecDeque<Name> = VecDeque::new();
    visited.insert(start.clone());
    queue.push_back(start.clone());

    while let Some(v) = queue.pop_front() {
        if v == *end {
            // Reconstruct the interior by walking `prev` backwards.
            let mut interior: Vec<Name> = Vec::new();
            let mut cursor = v;
            while let Some(p) = prev.get(&cursor) {
                if *p == *start {
                    break;
                }
                interior.push(p.clone());
                cursor = p.clone();
            }
            interior.reverse();
            return if interior.is_empty() {
                // No interior: the only path is a direct arc, which
                // resolve_edge already handles.
                None
            } else {
                Some(interior)
            };
        }
        if let Some(out_edges) = tgt.outgoing.get(&v) {
            for edge in out_edges {
                let next = &edge.tgt;
                if visited.contains(next) {
                    continue;
                }
                // The terminal step must land on `end`. Interior hops
                // must go through vertices newly added in tgt.
                let interior_ok = *next == *end || new_verts.contains(next);
                if !interior_ok {
                    continue;
                }
                visited.insert(next.clone());
                prev.insert(next.clone(), v.clone());
                queue.push_back(next.clone());
            }
        }
    }
    None
}

/// Apply a theory transform to a schema, producing a new schema.
///
/// This is the bridge between GAT-level (Theory) and schema-level (Schema).
/// The `protocol` parameter is threaded through for recursive calls but
/// is not directly consulted by the current transform implementations.
#[allow(clippy::only_used_in_recursion, clippy::too_many_lines)]
fn apply_theory_transform_to_schema(
    transform: &TheoryTransform,
    schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, LensError> {
    match transform {
        // Identity, directed equations, and equations leave the schema
        // graph structure unchanged.
        TheoryTransform::Identity
        | TheoryTransform::AddDirectedEquation(_)
        | TheoryTransform::DropDirectedEquation(_)
        | TheoryTransform::AddEquation(_)
        | TheoryTransform::DropEquation(_) => Ok(schema.clone()),
        TheoryTransform::CoerceSort {
            sort_name,
            coercion_expr,
            inverse_expr,
            coercion_class,
            ..
        } => Ok(apply_coerce_sort_to_schema(
            schema,
            sort_name,
            coercion_expr,
            inverse_expr.as_ref(),
            *coercion_class,
        )),
        TheoryTransform::MergeSorts {
            sort_a,
            sort_b,
            merged_name,
            merger_expr,
        } => Ok(apply_merge_sorts_to_schema(
            schema,
            sort_a,
            sort_b,
            merged_name,
            merger_expr,
        )),
        TheoryTransform::RenameSort { old, new } => {
            Ok(apply_rename_sort_to_schema(schema, old, new))
        }
        TheoryTransform::RenameOp { old, new } => Ok(apply_rename_op_to_schema(schema, old, new)),
        TheoryTransform::DropSort(name) => Ok(apply_drop_sort_from_schema(schema, name)),
        TheoryTransform::AddSort { sort, vertex_kind } => {
            Ok(apply_add_sort(schema, sort, vertex_kind.as_ref(), None))
        }
        TheoryTransform::AddSortWithDefault {
            sort,
            vertex_kind,
            default_expr,
        } => Ok(apply_add_sort(
            schema,
            sort,
            vertex_kind.as_ref(),
            Some(default_expr),
        )),
        TheoryTransform::DropOp(name) => Ok(apply_drop_op_from_schema(schema, name)),
        TheoryTransform::AddOp(op) => Ok(apply_add_op(schema, op)),
        TheoryTransform::Pullback(morphism) => {
            let mut result = schema.clone();
            for (old, new) in &morphism.sort_map {
                if old != new {
                    result = apply_rename_sort_to_schema(&result, old, new);
                }
            }
            for (old, assignment) in &morphism.op_map {
                // Renaming an operation renames its schema edge kind; a
                // derived-term assignment is not a rename.
                if let Some(new) = assignment.as_op() {
                    if old != new {
                        result = apply_rename_op_to_schema(&result, old, new);
                    }
                }
            }
            Ok(result)
        }
        TheoryTransform::RenameEdgeName {
            src_sort,
            tgt_sort,
            old_name,
            new_name,
        } => Ok(apply_rename_edge_name(
            schema, src_sort, tgt_sort, old_name, new_name,
        )),
        TheoryTransform::AddEdge {
            src_sort,
            tgt_sort,
            edge_name,
            edge_kind,
        } => Ok(apply_add_edge_to_schema(
            schema, src_sort, tgt_sort, edge_name, edge_kind,
        )),
        TheoryTransform::DropEdge {
            src_sort,
            tgt_sort,
            edge_name,
        } => Ok(apply_drop_edge_from_schema(
            schema,
            src_sort,
            tgt_sort,
            edge_name.as_ref(),
        )),
        TheoryTransform::ScopedTransform { focus, inner } => {
            apply_scoped_schema_transform(schema, focus, inner, protocol)
        }
        TheoryTransform::Compose(first, second) => {
            let intermediate = apply_theory_transform_to_schema(first, schema, protocol)?;
            apply_theory_transform_to_schema(second, &intermediate, protocol)
        }
        TheoryTransform::StripEnrichment(kind) => Ok(apply_strip_enrichment(schema, *kind)),
        TheoryTransform::AddEnrichment {
            kind,
            enricher,
            policy,
        } => {
            let driver = crate::enrichment_registry::lookup_enricher(*kind, enricher)?;
            driver.enrich(schema, policy)
        }
    }
}

/// Drop every constraint whose sort is in `kind`'s fibre, and prune
/// the now-empty per-vertex entries so equality is structural.
fn apply_strip_enrichment(schema: &Schema, kind: panproto_gat::EnrichmentKind) -> Schema {
    let mut out = schema.clone();
    for cs in out.constraints.values_mut() {
        cs.retain(|c| !kind.is_member_sort(c.sort.as_ref()));
    }
    out.constraints.retain(|_, cs| !cs.is_empty());
    out
}

/// Schema-level counterpart of
/// [`crate::schema_functor::apply_scoped_transform`]:
/// extract the sub-schema reachable from `focus`, apply `inner` to it,
/// and pushout-merge the result back into the full schema. Cross-boundary
/// edges are preserved; adjacency indices are rebuilt.
#[allow(clippy::only_used_in_recursion)]
fn apply_scoped_schema_transform(
    schema: &Schema,
    focus: &Arc<str>,
    inner: &TheoryTransform,
    protocol: &Protocol,
) -> Result<Schema, LensError> {
    // 1. Find all vertices reachable from focus via outgoing edges (BFS).
    let mut reachable: std::collections::HashSet<Name> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<Name> = std::collections::VecDeque::new();
    let focus_name = Name::from(&**focus);
    if schema.vertices.contains_key(&focus_name) {
        reachable.insert(focus_name.clone());
        queue.push_back(focus_name);
    }
    while let Some(v) = queue.pop_front() {
        for edge in schema.outgoing_edges(&v) {
            if reachable.insert(edge.tgt.clone()) {
                queue.push_back(edge.tgt.clone());
            }
        }
    }

    // 2. Build the sub-schema from reachable vertices and edges.
    let sub_vertices: HashMap<Name, Vertex> = schema
        .vertices
        .iter()
        .filter(|(id, _)| reachable.contains(*id))
        .map(|(id, v)| (id.clone(), v.clone()))
        .collect();
    let sub_edges: HashMap<panproto_schema::Edge, Name> = schema
        .edges
        .iter()
        .filter(|(e, _)| reachable.contains(&e.src) && reachable.contains(&e.tgt))
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();
    let sub_constraints: HashMap<Name, Vec<panproto_schema::Constraint>> = schema
        .constraints
        .iter()
        .filter(|(id, _)| reachable.contains(*id))
        .map(|(id, c)| (id.clone(), c.clone()))
        .collect();
    let sub_defaults: HashMap<Name, panproto_expr::Expr> = schema
        .defaults
        .iter()
        .filter(|(id, _)| reachable.contains(*id))
        .map(|(id, d)| (id.clone(), d.clone()))
        .collect();
    let mut sub_schema = schema.clone();
    sub_schema.vertices = sub_vertices;
    sub_schema.edges = sub_edges;
    sub_schema.constraints = sub_constraints;
    sub_schema.defaults = sub_defaults;

    // 3. Apply inner transform to the sub-schema.
    let transformed_sub = apply_theory_transform_to_schema(inner, &sub_schema, protocol)?;

    // 4. Pushout: replace the reachable sub-schema with its transformed version,
    //    preserving cross-boundary edges.
    let mut result = schema.clone();
    result.vertices.retain(|id, _| !reachable.contains(id));
    result
        .edges
        .retain(|e, _| !(reachable.contains(&e.src) && reachable.contains(&e.tgt)));
    result.constraints.retain(|id, _| !reachable.contains(id));
    result.defaults.retain(|id, _| !reachable.contains(id));
    result.vertices.extend(transformed_sub.vertices);
    result.edges.extend(transformed_sub.edges);
    result.constraints.extend(transformed_sub.constraints);
    result.defaults.extend(transformed_sub.defaults);

    // 5. Rebuild adjacency indices.
    rebuild_adjacency(&mut result);
    Ok(result)
}

/// Install a coercion spec keyed on `(sort, sort)` in the schema's
/// enrichment map, leaving the vertex/edge graph unchanged.
fn apply_coerce_sort_to_schema(
    schema: &Schema,
    sort_name: &Arc<str>,
    coercion_expr: &panproto_expr::Expr,
    inverse_expr: Option<&panproto_expr::Expr>,
    coercion_class: panproto_gat::CoercionClass,
) -> Schema {
    let mut new_schema = schema.clone();
    let name = Name::from(&**sort_name);
    new_schema.coercions.insert(
        (name.clone(), name),
        panproto_schema::CoercionSpec {
            forward: coercion_expr.clone(),
            inverse: inverse_expr.cloned(),
            class: coercion_class,
        },
    );
    new_schema
}

/// Add a vertex for a new sort, optionally installing a default expression
/// so the migration engine can compute initial values for lifted instances.
fn apply_add_sort(
    schema: &Schema,
    sort: &panproto_gat::Sort,
    vertex_kind: Option<&Arc<str>>,
    default_expr: Option<&panproto_expr::Expr>,
) -> Schema {
    let mut new_schema = schema.clone();
    let name = Name::from(&*sort.name);
    let kind = vertex_kind.map_or_else(|| sort.default_vertex_kind(), Arc::clone);
    let vertex = Vertex {
        id: name.clone(),
        kind: Name::from(&*kind),
        nsid: None,
    };
    new_schema.vertices.insert(name.clone(), vertex);
    if let Some(expr) = default_expr {
        new_schema.defaults.insert(name, expr.clone());
    }
    new_schema
}

/// Rename a specific edge label (between two given sorts) throughout the
/// schema, then rebuild adjacency indices against the new edge set.
fn apply_rename_edge_name(
    schema: &Schema,
    src_sort: &Arc<str>,
    tgt_sort: &Arc<str>,
    old_name: &Arc<str>,
    new_name: &Arc<str>,
) -> Schema {
    let mut new_edges = HashMap::new();
    for (edge, kind) in &schema.edges {
        let mut e = edge.clone();
        if *e.src == **src_sort && *e.tgt == **tgt_sort && e.name.as_deref() == Some(&**old_name) {
            e.name = Some(Name::from(&**new_name));
        }
        new_edges.insert(e, kind.clone());
    }
    let mut new_schema = schema.clone();
    new_schema.edges = new_edges;
    rebuild_adjacency(&mut new_schema);
    new_schema
}

/// Interpret an Op addition as an edge addition: the op's first input
/// sort is the edge source, its output sort is the edge target. Missing
/// endpoints are silently ignored (the schema is unchanged).
fn apply_add_op(schema: &Schema, op: &panproto_gat::Operation) -> Schema {
    let mut new_schema = schema.clone();
    let Some((_, src_sort, _)) = op.inputs.first() else {
        return new_schema;
    };
    let src = Name::from(src_sort.head().as_ref());
    let tgt = Name::from(op.output.head().as_ref());
    if !new_schema.vertices.contains_key(&src) || !new_schema.vertices.contains_key(&tgt) {
        return new_schema;
    }
    let edge = Edge {
        src: src.clone(),
        tgt: tgt.clone(),
        kind: Name::from(&*op.name),
        name: Some(Name::from(&*op.name)),
    };
    new_schema.edges.insert(edge.clone(), Name::from(&*op.name));
    new_schema
        .outgoing
        .entry(src)
        .or_default()
        .push(edge.clone());
    new_schema.incoming.entry(tgt).or_default().push(edge);
    new_schema
}

/// Merge two sort-vertices into a single vertex with the supplied merger
/// expression installed.
fn apply_merge_sorts_to_schema(
    schema: &Schema,
    sort_a: &Arc<str>,
    sort_b: &Arc<str>,
    merged_name: &Arc<str>,
    merger_expr: &panproto_expr::Expr,
) -> Schema {
    let mut new_schema = apply_drop_sort_from_schema(schema, sort_a);
    new_schema = apply_drop_sort_from_schema(&new_schema, sort_b);
    let vertex = Vertex {
        id: Name::from(&**merged_name),
        kind: Name::from(&**merged_name),
        nsid: None,
    };
    new_schema
        .vertices
        .insert(Name::from(&**merged_name), vertex);
    new_schema
        .mergers
        .insert(Name::from(&**merged_name), merger_expr.clone());
    new_schema
}

/// Rebuild `outgoing`, `incoming`, and `between` adjacency indices from
/// `schema.edges`.
fn rebuild_adjacency(schema: &mut Schema) {
    let mut outgoing: HashMap<Name, SmallVec<panproto_schema::Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<panproto_schema::Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<panproto_schema::Edge, 2>> = HashMap::new();
    for edge in schema.edges.keys() {
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }
    schema.outgoing = outgoing;
    schema.incoming = incoming;
    schema.between = between;
}

/// Rename a sort (vertex kind) within a schema.
fn apply_rename_sort_to_schema(schema: &Schema, old: &Arc<str>, new: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();

    // Rename vertex kinds (not IDs) that match the old sort name
    let mut new_vertices = HashMap::new();
    for (id, vertex) in &new_schema.vertices {
        let mut v = vertex.clone();
        if *v.kind == **old {
            v.kind = Name::from(&**new);
        }
        new_vertices.insert(id.clone(), v);
    }
    new_schema.vertices = new_vertices;

    // Rename edge kinds that match the old sort name
    let mut new_edges = HashMap::new();
    for (edge, kind) in &new_schema.edges {
        let e = edge.clone();
        let k = if **kind == **old {
            Name::from(&**new)
        } else {
            kind.clone()
        };
        new_edges.insert(e, k);
    }
    new_schema.edges = new_edges;

    // Rename coercion keys that reference the old sort name
    let mut new_coercions = HashMap::new();
    for ((from, to), spec) in &new_schema.coercions {
        let new_from = if **from == **old {
            Name::from(&**new)
        } else {
            from.clone()
        };
        let new_to = if **to == **old {
            Name::from(&**new)
        } else {
            to.clone()
        };
        new_coercions.insert((new_from, new_to), spec.clone());
    }
    new_schema.coercions = new_coercions;

    // Rename hyper-edge sort references
    for he in new_schema.hyper_edges.values_mut() {
        he.signature = he
            .signature
            .iter()
            .map(|(label, vid)| {
                let new_vid = if **vid == **old {
                    Name::from(&**new)
                } else {
                    vid.clone()
                };
                (label.clone(), new_vid)
            })
            .collect();
    }

    // Rename constraint sort references
    let mut new_constraints = HashMap::new();
    for (cid, cs) in &new_schema.constraints {
        let new_cs: Vec<_> = cs
            .iter()
            .map(|c| {
                let mut c2 = c.clone();
                if *c2.sort == **old {
                    c2.sort = Name::from(&**new);
                }
                c2
            })
            .collect();
        new_constraints.insert(cid.clone(), new_cs);
    }
    new_schema.constraints = new_constraints;

    rebuild_indices(&mut new_schema);
    new_schema
}

/// Rename an operation (edge kind) within a schema.
fn apply_rename_op_to_schema(schema: &Schema, old: &Arc<str>, new: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let mut new_edges = HashMap::new();
    for (edge, kind) in &new_schema.edges {
        let mut e = edge.clone();
        if *e.kind == **old {
            e.kind = Name::from(&**new);
        }
        let k = if **kind == **old {
            Name::from(&**new)
        } else {
            kind.clone()
        };
        new_edges.insert(e, k);
    }
    new_schema.edges = new_edges;
    rebuild_indices(&mut new_schema);
    new_schema
}

/// Drop a sort (vertex ID or kind) and all dependent edges from a schema.
fn apply_drop_sort_from_schema(schema: &Schema, name: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let to_remove: Vec<Name> = new_schema
        .vertices
        .iter()
        .filter(|(id, v)| **id == **name || *v.kind == **name)
        .map(|(id, _)| id.clone())
        .collect();
    for id in &to_remove {
        new_schema.vertices.remove(id);
    }
    let new_edges: HashMap<Edge, Name> = new_schema
        .edges
        .iter()
        .filter(|(e, _)| !to_remove.contains(&e.src) && !to_remove.contains(&e.tgt))
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();
    new_schema.edges = new_edges;

    // Remove coercions where either key references the dropped sort
    new_schema
        .coercions
        .retain(|(from, to), _| *from != **name && *to != **name);

    // Remove mergers keyed by the dropped sort
    new_schema
        .mergers
        .retain(|k, _| !to_remove.contains(k) && **k != **name);

    // Remove defaults keyed by the dropped sort
    new_schema
        .defaults
        .retain(|k, _| !to_remove.contains(k) && **k != **name);

    // Remove policies keyed by the dropped sort
    new_schema
        .policies
        .retain(|k, _| !to_remove.contains(k) && **k != **name);

    // Remove constraints keyed by dropped vertices
    for id in &to_remove {
        new_schema.constraints.remove(id);
    }

    rebuild_indices(&mut new_schema);
    new_schema
}

/// Drop an operation (edge kind) from a schema.
fn apply_drop_op_from_schema(schema: &Schema, name: &Arc<str>) -> Schema {
    let mut new_schema = schema.clone();
    let new_edges: HashMap<Edge, Name> = new_schema
        .edges
        .iter()
        .filter(|(e, _)| *e.kind != **name)
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();
    new_schema.edges = new_edges;
    rebuild_indices(&mut new_schema);
    new_schema
}

/// Add a single edge identified by its `(src, tgt, name, kind)` tuple.
///
/// Mirrors the fiber-level semantics of `TheoryTransform::AddEdge`: the
/// theory is unchanged, only the schema's edge set is extended. Silently
/// no-ops if either endpoint vertex is missing, matching `AddOp`.
fn apply_add_edge_to_schema(
    schema: &Schema,
    src_sort: &Arc<str>,
    tgt_sort: &Arc<str>,
    edge_name: &Arc<str>,
    edge_kind: &Arc<str>,
) -> Schema {
    let mut new_schema = schema.clone();
    let src = Name::from(&**src_sort);
    let tgt = Name::from(&**tgt_sort);
    if !new_schema.vertices.contains_key(&src) || !new_schema.vertices.contains_key(&tgt) {
        return new_schema;
    }
    let edge = Edge {
        src: src.clone(),
        tgt: tgt.clone(),
        kind: Name::from(&**edge_kind),
        name: Some(Name::from(&**edge_name)),
    };
    new_schema
        .edges
        .insert(edge.clone(), Name::from(&**edge_kind));
    new_schema
        .outgoing
        .entry(src.clone())
        .or_default()
        .push(edge.clone());
    new_schema
        .incoming
        .entry(tgt.clone())
        .or_default()
        .push(edge.clone());
    new_schema.between.entry((src, tgt)).or_default().push(edge);
    new_schema
}

/// Drop a single edge identified by its `(src, tgt, name)` triple.
///
/// Unlike `apply_drop_op_from_schema`, which removes every edge of a given
/// kind, this removes exactly one edge (or all edges with the matching
/// triple, which should be at most one in a well-formed schema).
fn apply_drop_edge_from_schema(
    schema: &Schema,
    src_sort: &Arc<str>,
    tgt_sort: &Arc<str>,
    edge_name: Option<&Arc<str>>,
) -> Schema {
    let mut new_schema = schema.clone();
    let target_name: Option<&str> = edge_name.map(|a| &**a);
    let new_edges: HashMap<Edge, Name> = new_schema
        .edges
        .iter()
        .filter(|(e, _)| {
            let matches =
                *e.src == **src_sort && *e.tgt == **tgt_sort && e.name.as_deref() == target_name;
            !matches
        })
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();
    new_schema.edges = new_edges;
    rebuild_indices(&mut new_schema);
    new_schema
}

/// Build an implicit theory from a schema (sorts = vertex kinds,
/// ops = edge kinds).
pub(crate) fn schema_to_implicit_theory(schema: &Schema) -> Theory {
    // `schema.vertices` / `schema.edges` are HashMaps with a process-
    // randomized hasher: iterating them directly would let sort / op
    // order drift across runs, and `factorize` iterates
    // `theory.sorts` / `theory.ops` in order. Sort before folding so
    // the resulting `Theory` is identical across process instances on
    // equal input, which the factorization pipeline depends on for
    // reproducible endofunctor sequences.
    let mut vertex_ids: Vec<&Name> = schema.vertices.keys().collect();
    vertex_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut sort_names: HashSet<&str> = HashSet::new();
    let mut sorts = Vec::new();
    for vid in vertex_ids {
        let vertex = &schema.vertices[vid];
        if sort_names.insert(&vertex.kind) {
            sorts.push(Sort::simple(name_arc_clone(&vertex.kind)));
        }
    }

    let mut edges: Vec<&Edge> = schema.edges.keys().collect();
    edges.sort_by(|a, b| {
        a.src
            .as_str()
            .cmp(b.src.as_str())
            .then_with(|| a.tgt.as_str().cmp(b.tgt.as_str()))
            .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
    });
    let mut op_names: HashSet<&str> = HashSet::new();
    let mut ops = Vec::new();
    for edge in edges {
        if op_names.insert(&edge.kind) {
            let src_kind = schema
                .vertices
                .get(&edge.src)
                .map_or_else(|| Arc::from("unknown"), |v| name_arc_clone(&v.kind));
            let tgt_kind = schema
                .vertices
                .get(&edge.tgt)
                .map_or_else(|| Arc::from("unknown"), |v| name_arc_clone(&v.kind));
            ops.push(Operation::unary(
                name_arc_clone(&edge.kind),
                "x",
                src_kind,
                tgt_kind,
            ));
        }
    }

    Theory::new("implicit", sorts, ops, Vec::new())
}

/// Rebuild the precomputed adjacency indices on a schema after mutating
/// vertices/edges.
pub(crate) fn rebuild_indices(schema: &mut Schema) {
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    // `schema.edges` is a HashMap with a process-randomized hasher;
    // iterating its keys directly would let the order inside each
    // adjacency SmallVec drift across process runs, which in turn
    // drifts downstream consumers that pick the "first compatible
    // edge" (e.g. `build_morphism_weighted` in hom_search). Collect
    // and sort by `(src, tgt, kind, name)` so the per-vertex adjacency
    // order is a pure function of schema content.
    let mut edges: Vec<&Edge> = schema.edges.keys().collect();
    edges.sort_by(|a, b| {
        a.src
            .as_str()
            .cmp(b.src.as_str())
            .then_with(|| a.tgt.as_str().cmp(b.tgt.as_str()))
            .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
            .then_with(|| a.name.as_deref().cmp(&b.name.as_deref()))
    });

    for edge in edges {
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    schema.outgoing = outgoing;
    schema.incoming = incoming;
    schema.between = between;
}

/// Build an identity lens for the given schema.
fn identity_lens(schema: &Schema) -> Lens {
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges = schema.edges.keys().cloned().collect();

    Lens {
        compiled: CompiledMigration {
            surviving_verts,
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use panproto_inst::value::Value;
    use panproto_schema::Protocol;

    use super::{
        ComplementConstructor, ProtolensChain, elementary, horizontal_compose, identity_lens,
        schema_to_implicit_theory, theory_endofunctor_equiv, vertical_compose,
    };
    use crate::tests::{three_node_instance, three_node_schema};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "array".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    #[test]
    fn schema_to_implicit_theory_deterministic_sort_op_order() {
        // `Schema::vertices` / `Schema::edges` are HashMaps; iterating
        // them directly would let sort / op order depend on hasher
        // state. `factorize` iterates `theory.sorts` / `theory.ops`
        // in order, so drift here would propagate to drift in the
        // factorized endofunctor chain. Pin the order by content.
        use panproto_schema::SchemaBuilder;
        let protocol = Protocol {
            name: "t".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["record".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        // Build a schema with enough vertices/edges that HashMap
        // iteration order is very likely to diverge from insertion.
        let s = SchemaBuilder::new(&protocol)
            .vertex("zzz", "record", None::<&str>)
            .unwrap()
            .vertex("aaa", "string", None::<&str>)
            .unwrap()
            .vertex("mmm", "integer", None::<&str>)
            .unwrap()
            .vertex("bbb", "string", None::<&str>)
            .unwrap()
            .edge("zzz", "aaa", "prop", Some("x"))
            .unwrap()
            .edge("zzz", "mmm", "field", Some("y"))
            .unwrap()
            .edge("zzz", "bbb", "attr", Some("z"))
            .unwrap()
            .build()
            .unwrap();
        // Repeat enough times that any hasher-driven reordering would
        // show up at least once across runs.
        let baseline = schema_to_implicit_theory(&s);
        for _ in 0..16 {
            let t = schema_to_implicit_theory(&s);
            let baseline_sorts: Vec<String> =
                baseline.sorts.iter().map(|x| x.name.to_string()).collect();
            let t_sorts: Vec<String> = t.sorts.iter().map(|x| x.name.to_string()).collect();
            assert_eq!(baseline_sorts, t_sorts, "sort order drift");
            let baseline_ops: Vec<String> =
                baseline.ops.iter().map(|x| x.name.to_string()).collect();
            let t_ops: Vec<String> = t.ops.iter().map(|x| x.name.to_string()).collect();
            assert_eq!(baseline_ops, t_ops, "op order drift");
        }
    }

    #[test]
    fn elementary_rename_sort_applicable() {
        let schema = three_node_schema();
        let p = elementary::rename_sort("string", "text");
        assert!(p.applicable_to(&schema));
    }

    #[test]
    fn elementary_rename_sort_not_applicable() {
        let schema = three_node_schema();
        let p = elementary::rename_sort("nonexistent", "text");
        assert!(!p.applicable_to(&schema));
    }

    #[test]
    fn elementary_drop_sort_applicable() {
        let schema = three_node_schema();
        let p = elementary::drop_sort("string");
        assert!(p.applicable_to(&schema));
    }

    #[test]
    fn elementary_add_sort_always_applicable() {
        let schema = three_node_schema();
        let p = elementary::add_sort("tags", "array", Value::Null);
        assert!(p.applicable_to(&schema));
    }

    #[test]
    fn elementary_rename_sort_instantiate() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::rename_sort("string", "text");
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_ne!(lens.src_schema.vertices.len(), 0);
    }

    #[test]
    fn chain_empty_is_identity() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![]);
        let lens = chain.instantiate(&schema, &protocol).unwrap();
        assert_eq!(
            lens.src_schema.vertices.len(),
            lens.tgt_schema.vertices.len()
        );
    }

    #[test]
    fn chain_single_step() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![elementary::add_sort("tags", "array", Value::Null)]);
        let lens = chain.instantiate(&schema, &protocol).unwrap();
        assert_eq!(
            lens.tgt_schema.vertices.len(),
            lens.src_schema.vertices.len() + 1
        );
    }

    #[test]
    fn vertical_compose_works() {
        let p1 = elementary::rename_sort("string", "text");
        let p2 = elementary::add_sort("tags", "array", Value::Null);
        let composed = vertical_compose(&p1, &p2).unwrap();
        assert_eq!(&*composed.name, "add_sort_tags.rename_sort_string_text");
    }

    #[test]
    fn identity_source_precondition_enforced() {
        let schema = three_node_schema();

        // η renames an existing sort; θ is `Identity`-source but its own
        // source precondition requires a sort the schema lacks.
        let eta = elementary::rename_sort("string", "text");
        let theta = elementary::rename_sort("missing", "gone");
        assert!(
            matches!(
                theta.source.transform,
                panproto_gat::TheoryTransform::Identity
            ),
            "θ must be Identity-source for this test to exercise the retention path"
        );

        // η alone is applicable at the schema (it has a `string` sort), so
        // the composite would pass were θ's precondition dropped.
        assert!(eta.applicable_to(&schema));

        let composed = vertical_compose(&eta, &theta).unwrap();

        // θ's precondition (`HasSort(missing)`) is conjoined into the
        // composite's source, so the composite is inapplicable at a schema
        // lacking that sort.
        assert!(
            !composed.applicable_to(&schema),
            "composite must be inapplicable where θ's precondition is unmet"
        );
        let reasons = composed
            .check_applicability(&schema)
            .expect_err("composite must report the unmet retained precondition");
        assert!(
            reasons.iter().any(|r| r.contains("missing")),
            "failure reasons must name the missing sort: {reasons:?}"
        );
    }

    /// A small pool of composable elementary steps (all `Identity`-source,
    /// so any two compose) whose target transforms apply cleanly to
    /// [`three_node_schema`].
    fn associativity_pool() -> Vec<super::Protolens> {
        vec![
            elementary::rename_sort("string", "text"),
            elementary::rename_sort("object", "obj"),
            elementary::add_sort("extra", "string", Value::Null),
            elementary::rename_op("prop", "field"),
        ]
    }

    /// `vertical_compose` is associative — `(h∘g)∘f` and
    /// `h∘(g∘f)` agree both as theory endofunctors and as instantiated
    /// lenses (equal target schemas and equal views), not merely by name.
    #[test]
    fn vertical_compose_associative() {
        use std::collections::BTreeSet;

        let step_a = elementary::rename_sort("object", "obj");
        let step_b = elementary::add_sort("tags", "array", Value::Null);
        let step_c = elementary::rename_sort("string", "text");

        let left = vertical_compose(&vertical_compose(&step_a, &step_b).unwrap(), &step_c).unwrap();
        let right =
            vertical_compose(&step_a, &vertical_compose(&step_b, &step_c).unwrap()).unwrap();

        // Endofunctor-level associativity (source and target agree).
        assert!(
            theory_endofunctor_equiv(&left.source, &right.source),
            "source endofunctors must agree under re-association",
        );
        assert!(
            theory_endofunctor_equiv(&left.target, &right.target),
            "target endofunctors must agree under re-association",
        );

        // Instantiated-behavior associativity: equal target schemas.
        let schema = three_node_schema();
        let proto = test_protocol();
        let lens_l = left.instantiate(&schema, &proto).unwrap();
        let lens_r = right.instantiate(&schema, &proto).unwrap();

        let lv: BTreeSet<_> = lens_l.tgt_schema.vertices.keys().cloned().collect();
        let rv: BTreeSet<_> = lens_r.tgt_schema.vertices.keys().cloned().collect();
        assert_eq!(lv, rv, "target vertex sets must match");
        let le: BTreeSet<_> = lens_l.tgt_schema.edges.keys().cloned().collect();
        let re: BTreeSet<_> = lens_r.tgt_schema.edges.keys().cloned().collect();
        assert_eq!(le, re, "target edge sets must match");

        // Equal views on a fixture instance.
        let instance = three_node_instance();
        let (lview, _) = crate::asymmetric::get(&lens_l, &instance).unwrap();
        let (rview, _) = crate::asymmetric::get(&lens_r, &instance).unwrap();
        assert!(
            crate::laws::instances_equivalent(&lview, &rview),
            "instantiated views must match under re-association",
        );
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// Sampled-triple associativity over the elementary pool.
        #[test]
        fn vertical_compose_associative_sampled(
            i in 0usize..4,
            j in 0usize..4,
            k in 0usize..4,
        ) {
            use proptest::prelude::*;
            use std::collections::BTreeSet;

            let pool = associativity_pool();
            let (pa, pb, pc) = (pool[i].clone(), pool[j].clone(), pool[k].clone());

            let left = vertical_compose(&vertical_compose(&pa, &pb).unwrap(), &pc).unwrap();
            let right = vertical_compose(&pa, &vertical_compose(&pb, &pc).unwrap()).unwrap();

            prop_assert!(theory_endofunctor_equiv(&left.source, &right.source));
            prop_assert!(theory_endofunctor_equiv(&left.target, &right.target));

            let schema = three_node_schema();
            let proto = test_protocol();
            let ll = left.instantiate(&schema, &proto);
            let rr = right.instantiate(&schema, &proto);
            prop_assert_eq!(ll.is_ok(), rr.is_ok());
            if let (Ok(lens_l), Ok(lens_r)) = (&ll, &rr) {
                let lv: BTreeSet<_> = lens_l.tgt_schema.vertices.keys().cloned().collect();
                let rv: BTreeSet<_> = lens_r.tgt_schema.vertices.keys().cloned().collect();
                prop_assert_eq!(lv, rv);
                let le: BTreeSet<_> = lens_l.tgt_schema.edges.keys().cloned().collect();
                let re: BTreeSet<_> = lens_r.tgt_schema.edges.keys().cloned().collect();
                prop_assert_eq!(le, re);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Construction-time coercion honesty checks.
    // -----------------------------------------------------------------------

    #[test]
    fn coercion_checked_sort_honest_iso_passes() {
        use crate::coercion_laws::CoercionSampleRegistry;
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use panproto_gat::{CoercionClass, ValueKind};

        // forward = x + 1, inverse = x - 1: a genuine integer isomorphism.
        let forward = Expr::Builtin(
            BuiltinOp::Add,
            vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
        );
        let inverse = Expr::Builtin(
            BuiltinOp::Sub,
            vec![Expr::var("x"), Expr::Lit(Literal::Int(1))],
        );

        // Use a registry with non-overflowing integer samples so the
        // honest x±1 witness round-trips (the default set includes
        // i64::MAX/MIN, where checked arithmetic errors by design).
        let mut registry = CoercionSampleRegistry::new();
        registry.register(
            ValueKind::Int,
            vec![
                Literal::Int(0),
                Literal::Int(1),
                Literal::Int(-1),
                Literal::Int(42),
                Literal::Int(-100),
            ],
        );

        let result = elementary::sort_coerce_checked(
            "count",
            ValueKind::Int,
            forward,
            Some(inverse),
            CoercionClass::Iso,
            ValueKind::Int,
            "x",
            &registry,
        );
        assert!(
            result.is_ok(),
            "honest x±1 iso should construct: {result:?}"
        );
    }

    #[test]
    fn coercion_checked_sort_dishonest_iso_fails() {
        use crate::coercion_laws::CoercionSampleRegistry;
        use panproto_expr::{BuiltinOp, Expr};
        use panproto_gat::{CoercionClass, ValueKind};

        // forward = upper(x) is lossy (collapses case); declaring it Iso
        // with an identity inverse is dishonest: inverse(forward(s)) =
        // upper(s) != s for any lower-cased sample.
        let forward = Expr::Builtin(BuiltinOp::Upper, vec![Expr::var("x")]);
        let inverse = Expr::var("x");
        let registry = CoercionSampleRegistry::with_defaults();

        let result = elementary::sort_coerce_checked(
            "name",
            ValueKind::Str,
            forward,
            Some(inverse),
            CoercionClass::Iso,
            ValueKind::Str,
            "x",
            &registry,
        );
        let Err(err) = result else {
            panic!("dishonest upper-as-iso coercion must be rejected at construction");
        };
        assert_eq!(err.class, CoercionClass::Iso);
        assert!(!err.violations.is_empty(), "must carry the violations");
        // The rendered diagnostic must surface the evidence-not-proof caveat.
        assert!(
            err.to_string().contains("evidence, not proof"),
            "diagnostic should carry the caveat: {err}"
        );
    }

    #[test]
    fn coercion_checked_directed_eq_honest_passes() {
        use crate::coercion_laws::CoercionSampleRegistry;
        use panproto_expr::Expr;
        use panproto_gat::{CoercionClass, DirectedEquation, Term, ValueKind};
        use std::sync::Arc;

        let deq = DirectedEquation {
            name: Arc::from("identity_str"),
            lhs: Term::var("x"),
            rhs: Term::var("x"),
            impl_term: Expr::var("x"),
            inverse: Some(Expr::var("x")),
            source_kind: Some(ValueKind::Str),
            target_kind: Some(ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        };
        let registry = CoercionSampleRegistry::with_defaults();
        let result = elementary::directed_eq_checked(deq, "x", &registry);
        assert!(result.is_ok(), "honest identity iso deq: {result:?}");
    }

    #[test]
    fn coercion_checked_directed_eq_dishonest_fails() {
        use crate::coercion_laws::CoercionSampleRegistry;
        use panproto_expr::{BuiltinOp, Expr};
        use panproto_gat::{CoercionClass, DirectedEquation, Term, ValueKind};
        use std::sync::Arc;

        let deq = DirectedEquation {
            name: Arc::from("upper_lying_iso"),
            lhs: Term::var("x"),
            rhs: Term::app("upper", vec![Term::var("x")]),
            impl_term: Expr::Builtin(BuiltinOp::Upper, vec![Expr::var("x")]),
            inverse: Some(Expr::var("x")),
            source_kind: Some(ValueKind::Str),
            target_kind: Some(ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        };
        let registry = CoercionSampleRegistry::with_defaults();
        let Err(err) = elementary::directed_eq_checked(deq, "x", &registry) else {
            panic!("dishonest upper-as-iso directed equation must be rejected");
        };
        assert!(!err.violations.is_empty());
    }

    #[test]
    fn is_lossless() {
        assert!(elementary::rename_sort("a", "b").is_lossless());
        assert!(elementary::rename_op("a", "b").is_lossless());
        assert!(!elementary::add_sort("a", "b", Value::Null).is_lossless());
        assert!(!elementary::drop_sort("a").is_lossless());
        assert!(!elementary::drop_op("a").is_lossless());
    }

    #[test]
    fn complement_constructor_types() {
        assert!(matches!(
            elementary::rename_sort("a", "b").complement_constructor,
            ComplementConstructor::Empty
        ));
        assert!(matches!(
            elementary::drop_sort("a").complement_constructor,
            ComplementConstructor::DroppedSortData { .. }
        ));
        assert!(matches!(
            elementary::drop_op("a").complement_constructor,
            ComplementConstructor::DroppedOpData { .. }
        ));
        assert!(matches!(
            elementary::add_sort("a", "b", Value::Null).complement_constructor,
            ComplementConstructor::AddedElement { .. }
        ));
    }

    #[test]
    fn protolens_chain_applicable() {
        let schema = three_node_schema();
        let chain = ProtolensChain::new(vec![elementary::rename_sort("string", "text")]);
        assert!(chain.applicable_to(&schema));
    }

    #[test]
    fn schema_to_theory_extracts_kinds() {
        let schema = three_node_schema();
        let theory = schema_to_implicit_theory(&schema);
        assert!(theory.has_sort("object"));
        assert!(theory.has_sort("string"));
        assert!(theory.has_op("prop"));
    }

    #[test]
    fn horizontal_compose_works() {
        let p1 = elementary::rename_sort("a", "b");
        let p2 = elementary::rename_sort("c", "d");
        let composed = horizontal_compose(&p1, &p2).unwrap();
        assert!(composed.name.contains('*'));
    }

    #[test]
    fn chain_len_and_is_empty() {
        let empty = ProtolensChain::new(vec![]);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let chain = ProtolensChain::new(vec![elementary::rename_sort("a", "b")]);
        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn drop_sort_instantiate() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::drop_sort("string");
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_eq!(lens.src_schema.vertices.len(), 3);
        assert_eq!(lens.tgt_schema.vertices.len(), 1);
    }

    #[test]
    fn add_sort_instantiate() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::add_sort("tags", "array", Value::Null);
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_eq!(lens.src_schema.vertices.len(), 3);
        assert_eq!(lens.tgt_schema.vertices.len(), 4);
        assert!(lens.tgt_schema.vertices.contains_key("tags"));
    }

    #[test]
    fn target_schema_without_full_lens() {
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::add_sort("tags", "array", Value::Null);
        let tgt = p.target_schema(&schema, &protocol).unwrap();
        assert_eq!(tgt.vertices.len(), 4);
    }

    #[test]
    fn identity_lens_preserves_schema() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        assert_eq!(
            lens.src_schema.vertices.len(),
            lens.tgt_schema.vertices.len()
        );
        assert_eq!(lens.src_schema.edges.len(), lens.tgt_schema.edges.len());
    }

    // -----------------------------------------------------------------------
    // Serialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn serde_round_trip_protolens() {
        let p = elementary::rename_sort("old", "new");
        let json = p.to_json().unwrap();
        let p2 = super::Protolens::from_json(&json).unwrap();
        assert_eq!(&*p.name, &*p2.name);
    }

    #[test]
    fn serde_round_trip_chain() {
        let chain = ProtolensChain::new(vec![
            elementary::rename_sort("a", "b"),
            elementary::add_sort("c", "d", Value::Null),
            elementary::drop_sort("e"),
        ]);
        let json = chain.to_json().unwrap();
        let chain2 = ProtolensChain::from_json(&json).unwrap();
        assert_eq!(chain2.len(), 3);
        assert_eq!(&*chain2.steps[0].name, &*chain.steps[0].name);
        assert_eq!(&*chain2.steps[1].name, &*chain.steps[1].name);
        assert_eq!(&*chain2.steps[2].name, &*chain.steps[2].name);
    }

    #[test]
    fn serde_round_trip_pullback() {
        use std::collections::HashMap;
        let morphism = panproto_gat::TheoryMorphism {
            name: std::sync::Arc::from("test_morph"),
            domain: std::sync::Arc::from("T1"),
            codomain: std::sync::Arc::from("T2"),
            sort_map: HashMap::new(),
            op_map: HashMap::new(),
        };
        let chain = ProtolensChain::new(vec![elementary::pullback(morphism)]);
        let json = chain.to_json().unwrap();
        let chain2 = ProtolensChain::from_json(&json).unwrap();
        assert_eq!(chain2.len(), 1);
        assert!(chain2.steps[0].name.contains("pullback"));
    }

    #[test]
    fn serde_round_trip_composite_complement() {
        let chain = ProtolensChain::new(vec![elementary::drop_sort("a"), elementary::drop_op("b")]);
        let json = chain.to_json().unwrap();
        let chain2 = ProtolensChain::from_json(&json).unwrap();
        assert_eq!(chain2.len(), 2);
        assert!(matches!(
            chain2.steps[0].complement_constructor,
            ComplementConstructor::DroppedSortData { .. }
        ));
        assert!(matches!(
            chain2.steps[1].complement_constructor,
            ComplementConstructor::DroppedOpData { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // SchemaConstraint tests
    // -----------------------------------------------------------------------

    #[test]
    fn schema_constraint_has_vertex_kind() {
        use super::SchemaConstraint;
        let schema = three_node_schema();
        assert!(SchemaConstraint::HasVertexKind("object".into()).satisfied_by(&schema));
        assert!(SchemaConstraint::HasVertexKind("string".into()).satisfied_by(&schema));
        assert!(!SchemaConstraint::HasVertexKind("missing".into()).satisfied_by(&schema));
    }

    #[test]
    fn schema_constraint_has_vertex() {
        use super::SchemaConstraint;
        let schema = three_node_schema();
        assert!(SchemaConstraint::HasVertex("post:body".into()).satisfied_by(&schema));
        assert!(!SchemaConstraint::HasVertex("nonexistent".into()).satisfied_by(&schema));
    }

    #[test]
    fn schema_constraint_has_edge_kind() {
        use super::SchemaConstraint;
        let schema = three_node_schema();
        assert!(SchemaConstraint::HasEdgeKind("prop".into()).satisfied_by(&schema));
        assert!(!SchemaConstraint::HasEdgeKind("missing".into()).satisfied_by(&schema));
    }

    #[test]
    fn schema_constraint_all_conjunction() {
        use super::SchemaConstraint;
        let schema = three_node_schema();
        let both = SchemaConstraint::All(vec![
            SchemaConstraint::HasVertexKind("object".into()),
            SchemaConstraint::HasVertexKind("string".into()),
        ]);
        assert!(both.satisfied_by(&schema));

        let one_bad = SchemaConstraint::All(vec![
            SchemaConstraint::HasVertexKind("object".into()),
            SchemaConstraint::HasVertexKind("missing".into()),
        ]);
        assert!(!one_bad.satisfied_by(&schema));
    }

    #[test]
    fn check_applicability_returns_reasons() {
        let schema = three_node_schema();
        // Build a protolens requiring HasSort("missing"); will fail
        let p = super::Protolens {
            name: panproto_gat::Name::from("test"),
            source: panproto_gat::TheoryEndofunctor {
                name: std::sync::Arc::from("id"),
                precondition: panproto_gat::TheoryConstraint::HasSort(std::sync::Arc::from(
                    "missing",
                )),
                transform: panproto_gat::TheoryTransform::Identity,
            },
            target: panproto_gat::TheoryEndofunctor {
                name: std::sync::Arc::from("id"),
                precondition: panproto_gat::TheoryConstraint::Unconstrained,
                transform: panproto_gat::TheoryTransform::Identity,
            },
            complement_constructor: ComplementConstructor::Empty,
        };
        let result = p.check_applicability(&schema);
        assert!(result.is_err());
        let reasons = result.unwrap_err();
        assert!(!reasons.is_empty());
        assert!(reasons[0].contains("missing"));
    }

    #[test]
    fn from_theory_constraint_maps_has_sort() {
        use super::SchemaConstraint;
        let tc = panproto_gat::TheoryConstraint::HasSort(std::sync::Arc::from("Vertex"));
        let sc = SchemaConstraint::from_theory_constraint(&tc);
        assert!(matches!(sc, SchemaConstraint::HasVertexKind(ref n) if &**n == "Vertex"));
    }

    // -----------------------------------------------------------------------
    // Fleet API tests
    // -----------------------------------------------------------------------

    fn make_schema_with_kind(
        name: &str,
        kind: &str,
    ) -> (panproto_gat::Name, panproto_schema::Schema) {
        use panproto_schema::Vertex;
        use std::collections::HashMap;
        let mut vertices = HashMap::new();
        vertices.insert(
            panproto_gat::Name::from(format!("{name}:v1")),
            Vertex {
                id: format!("{name}:v1").into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        let schema = panproto_schema::Schema {
            protocol: String::new(),
            vertices,
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
        };
        (panproto_gat::Name::from(name), schema)
    }

    fn make_string_schema(name: &str) -> (panproto_gat::Name, panproto_schema::Schema) {
        make_schema_with_kind(name, "string")
    }

    fn make_non_string_schema(name: &str) -> (panproto_gat::Name, panproto_schema::Schema) {
        make_schema_with_kind(name, "integer")
    }

    #[test]
    fn fleet_all_applicable() {
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![elementary::rename_sort("string", "text")]);
        let schemas: Vec<_> = vec![
            make_string_schema("a"),
            make_string_schema("b"),
            make_string_schema("c"),
        ];
        let result = super::apply_to_fleet(&chain, &schemas, &protocol);
        assert_eq!(result.applied.len(), 3);
        assert_eq!(result.skipped.len(), 0);
    }

    #[test]
    fn fleet_partial_applicable() {
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![elementary::rename_sort("string", "text")]);
        let schemas: Vec<_> = vec![
            make_string_schema("a"),
            make_string_schema("b"),
            make_non_string_schema("c"),
        ];
        let result = super::apply_to_fleet(&chain, &schemas, &protocol);
        assert_eq!(result.applied.len(), 2);
        assert_eq!(result.skipped.len(), 1);
    }

    #[test]
    fn fleet_empty_chain() {
        let protocol = test_protocol();
        let chain = ProtolensChain::new(vec![]);
        let schemas: Vec<_> = vec![
            make_string_schema("a"),
            make_string_schema("b"),
            make_string_schema("c"),
        ];
        let result = super::apply_to_fleet(&chain, &schemas, &protocol);
        assert_eq!(result.applied.len(), 3);
        assert_eq!(result.skipped.len(), 0);
    }

    #[test]
    fn check_applicability_chain_delegates() {
        let schema = three_node_schema();
        let chain = ProtolensChain::new(vec![elementary::rename_sort("string", "text")]);
        assert!(chain.check_applicability(&schema).is_ok());

        let bad_chain = ProtolensChain::new(vec![elementary::rename_sort("nonexistent", "text")]);
        assert!(bad_chain.check_applicability(&schema).is_err());

        let empty_chain = ProtolensChain::new(vec![]);
        assert!(empty_chain.check_applicability(&schema).is_ok());
    }

    // -----------------------------------------------------------------------
    // Fuse tests
    // -----------------------------------------------------------------------

    #[test]
    fn fuse_single_step() {
        let chain = ProtolensChain::new(vec![elementary::rename_sort("string", "text")]);
        let fused = chain.fuse().unwrap_or_else(|e| panic!("fuse failed: {e}"));
        assert_eq!(&*fused.name, "rename_sort_string_text");
    }

    #[test]
    fn fuse_two_steps() {
        let chain = ProtolensChain::new(vec![
            elementary::rename_sort("string", "text"),
            elementary::add_sort("tags", "array", Value::Null),
        ]);
        let fused = chain.fuse().unwrap_or_else(|e| panic!("fuse failed: {e}"));
        assert!(
            fused.name.contains("rename_sort_string_text"),
            "fused name should contain first step name, got: {}",
            fused.name
        );
        assert!(
            fused.name.contains("add_sort_tags"),
            "fused name should contain second step name, got: {}",
            fused.name
        );
    }

    #[test]
    fn fuse_empty_chain_errors() {
        let chain = ProtolensChain::new(vec![]);
        let result = chain.fuse();
        assert!(result.is_err());
    }

    #[test]
    fn fused_preserves_complement() {
        let chain =
            ProtolensChain::new(vec![elementary::drop_sort("a"), elementary::drop_sort("b")]);
        let fused = chain.fuse().unwrap_or_else(|e| panic!("fuse failed: {e}"));
        assert!(
            matches!(fused.complement_constructor, ComplementConstructor::Composite(ref v) if v.len() == 2),
            "expected Composite complement with 2 entries"
        );
    }

    // -----------------------------------------------------------------------
    // Functorial lifting tests
    // -----------------------------------------------------------------------

    fn test_morphism_vertex_to_node() -> panproto_gat::TheoryMorphism {
        use std::collections::HashMap;
        let mut sort_map = HashMap::new();
        sort_map.insert(std::sync::Arc::from("Vertex"), std::sync::Arc::from("Node"));
        panproto_gat::TheoryMorphism {
            name: std::sync::Arc::from("rename_vertex_node"),
            domain: std::sync::Arc::from("T1"),
            codomain: std::sync::Arc::from("T2"),
            sort_map,
            op_map: HashMap::new(),
        }
    }

    fn identity_morphism() -> panproto_gat::TheoryMorphism {
        use std::collections::HashMap;
        panproto_gat::TheoryMorphism {
            name: std::sync::Arc::from("id"),
            domain: std::sync::Arc::from("T"),
            codomain: std::sync::Arc::from("T"),
            sort_map: HashMap::new(),
            op_map: HashMap::new(),
        }
    }

    #[test]
    fn lift_protolens_renames_precondition() {
        let p = elementary::drop_sort("Vertex");
        let morphism = test_morphism_vertex_to_node();
        let lifted = super::lift_protolens(&p, &morphism);

        // The source precondition was HasSort("Vertex"), should now be HasSort("Node")
        match &lifted.source.precondition {
            panproto_gat::TheoryConstraint::HasSort(s) => {
                assert_eq!(&**s, "Node", "lifted precondition should reference 'Node'");
            }
            other => panic!("expected HasSort, got: {other:?}"),
        }
    }

    #[test]
    fn lift_protolens_identity_morphism() {
        let p = elementary::drop_sort("Vertex");
        let morphism = identity_morphism();
        let lifted = super::lift_protolens(&p, &morphism);

        // Precondition should still reference "Vertex" since identity morphism has no mappings
        match &lifted.source.precondition {
            panproto_gat::TheoryConstraint::HasSort(s) => {
                assert_eq!(&**s, "Vertex", "identity lift should preserve precondition");
            }
            other => panic!("expected HasSort, got: {other:?}"),
        }
    }

    #[test]
    fn lift_chain_preserves_length() {
        let chain = ProtolensChain::new(vec![
            elementary::rename_sort("a", "b"),
            elementary::drop_sort("c"),
            elementary::add_sort("d", "e", Value::Null),
        ]);
        let morphism = identity_morphism();
        let lifted = super::lift_chain(&chain, &morphism);
        assert_eq!(lifted.len(), 3);
    }

    #[test]
    fn lift_preserves_complement() {
        let p = elementary::drop_sort("Vertex");
        let morphism = test_morphism_vertex_to_node();
        let lifted = super::lift_protolens(&p, &morphism);
        assert!(
            matches!(
                lifted.complement_constructor,
                ComplementConstructor::DroppedSortData { .. }
            ),
            "complement should be preserved as DroppedSortData"
        );
    }

    #[test]
    fn lift_protolens_name() {
        let p = elementary::drop_sort("Vertex");
        let morphism = test_morphism_vertex_to_node();
        let lifted = super::lift_protolens(&p, &morphism);
        assert!(
            lifted.name.contains("rename_vertex_node"),
            "lifted name should include morphism name, got: {}",
            lifted.name
        );
    }

    // -----------------------------------------------------------------
    // `combinators::nest_field` against schemas with qualified vertex
    // ids (where the vertex id, e.g. `post:body.text`, is distinct from
    // the short edge label, e.g. `"text"`).
    // -----------------------------------------------------------------

    use panproto_gat::Name as GatName;
    use panproto_schema::{Edge as SchemaEdge, Vertex};
    use smallvec::{SmallVec, smallvec};

    #[test]
    fn elementary_drop_edge_targets_only_the_named_edge() {
        // `three_node_schema` has two parallel prop edges from `post:body`:
        // one labeled "text" and one labeled "createdAt". `drop_edge` with
        // the "text" triple should remove exactly the text edge and leave
        // createdAt in place.
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::drop_edge("post:body", "post:body.text", Some(GatName::from("text")));
        let lens = p.instantiate(&schema, &protocol).unwrap();
        let edges: Vec<_> = lens
            .tgt_schema
            .edges
            .keys()
            .map(|e| (e.src.clone(), e.tgt.clone(), e.name.clone()))
            .collect();
        // createdAt still present, text gone.
        assert!(
            edges.iter().any(|(_, t, n)| {
                **t == *"post:body.createdAt" && n.as_deref() == Some("createdAt")
            }),
            "createdAt edge should survive drop_edge, got {edges:?}"
        );
        assert!(
            !edges
                .iter()
                .any(|(_, t, n)| **t == *"post:body.text" && n.as_deref() == Some("text")),
            "text edge should have been dropped, got {edges:?}"
        );
    }

    #[test]
    fn elementary_add_edge_separates_name_from_kind() {
        // `add_edge` must let Edge.name differ from Edge.kind, unlike
        // `add_op` which forces them equal. This exercises the core
        // capability that made the nest_field fix possible.
        let schema = three_node_schema();
        let protocol = test_protocol();
        let p = elementary::add_edge(
            "post:body",
            "post:body.text",
            "displayText", // edge label distinct from kind
            "prop",        // edge kind
        );
        let lens = p.instantiate(&schema, &protocol).unwrap();
        let new_edge = lens
            .tgt_schema
            .edges
            .keys()
            .find(|e| {
                *e.src == *"post:body"
                    && *e.tgt == *"post:body.text"
                    && e.name.as_deref() == Some("displayText")
            })
            .expect("new edge should exist");
        assert_eq!(&*new_edge.kind, "prop", "kind should be preserved as prop");
        assert_eq!(
            new_edge.name.as_deref(),
            Some("displayText"),
            "name should be the caller-supplied label"
        );
    }

    #[test]
    fn nest_field_handles_qualified_vertex_ids() {
        // `nest_field` must not assume child vertex id equals edge
        // label, and must drop the original edge by name rather than
        // by kind. This test uses a qualified child id
        // (`post:body.text`) with a short edge label (`"text"`), the
        // shape produced by `SchemaBuilder::add_prop`.
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = super::combinators::nest_field(
            "post:body",                 // parent
            "post:body.text",            // child (qualified id)
            "post:body.profile",         // new intermediate
            "object",                    // intermediate kind
            "prop",                      // edge kind for the two new edges
            Some(GatName::from("text")), // original edge label to drop
            "profile",                   // parent → intermediate edge label
            "text",                      // intermediate → child edge label
        );
        let lens = chain
            .instantiate(&schema, &protocol)
            .expect("nest_field should instantiate against qualified ids");

        // Expected target schema edges:
        //   post:body -(name=profile)-> post:body.profile
        //   post:body.profile -(name=text)-> post:body.text
        //   post:body -(name=createdAt)-> post:body.createdAt   (untouched)
        let edges: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();

        assert!(
            edges.iter().any(|e| {
                *e.src == *"post:body"
                    && *e.tgt == *"post:body.profile"
                    && e.name.as_deref() == Some("profile")
                    && &*e.kind == "prop"
            }),
            "target schema should contain post:body -(profile)-> post:body.profile, got {edges:?}"
        );
        assert!(
            edges.iter().any(|e| {
                *e.src == *"post:body.profile"
                    && *e.tgt == *"post:body.text"
                    && e.name.as_deref() == Some("text")
                    && &*e.kind == "prop"
            }),
            "target schema should contain post:body.profile -(text)-> post:body.text, got {edges:?}"
        );
        // The original direct edge must be gone.
        assert!(
            !edges.iter().any(|e| {
                *e.src == *"post:body"
                    && *e.tgt == *"post:body.text"
                    && e.name.as_deref() == Some("text")
            }),
            "original post:body -(text)-> post:body.text edge should be removed, got {edges:?}"
        );
        // Sibling createdAt edge must survive (regression guard against
        // the old drop-by-kind bug which would have nuked it).
        assert!(
            edges.iter().any(|e| {
                *e.src == *"post:body"
                    && *e.tgt == *"post:body.createdAt"
                    && e.name.as_deref() == Some("createdAt")
            }),
            "sibling createdAt edge should survive nest_field, got {edges:?}"
        );
        // The new intermediate vertex should be present with kind=object.
        let intermediate = lens
            .tgt_schema
            .vertices
            .get(&GatName::from("post:body.profile"))
            .expect("intermediate vertex should exist");
        assert_eq!(&*intermediate.kind, "object");
    }

    #[test]
    fn nest_field_preserves_sibling_prop_edges() {
        // Under the old implementation, `drop_op("prop")` would have
        // nuked every prop edge (including the sibling `createdAt`).
        // This explicit test pins that regression.
        let schema = three_node_schema();
        let protocol = test_protocol();
        let chain = super::combinators::nest_field(
            "post:body",
            "post:body.text",
            "post:body.wrapper",
            "object",
            "prop",
            Some(GatName::from("text")),
            "wrapper",
            "text",
        );
        let lens = chain.instantiate(&schema, &protocol).unwrap();
        let prop_edge_count = lens
            .tgt_schema
            .edges
            .keys()
            .filter(|e| &*e.kind == "prop")
            .count();
        // Original: 2 prop edges (text, createdAt).
        // After nest: createdAt untouched, text replaced by two new prop edges.
        // Expected total: 1 (createdAt) + 2 (new) = 3.
        assert_eq!(
            prop_edge_count, 3,
            "expected 3 prop edges after nest_field (createdAt + 2 new), got {prop_edge_count}"
        );
    }

    #[test]
    fn nest_field_forward_eval_synthesizes_intermediate_node() {
        // `asymmetric::get` must synthesize a fresh intermediate view
        // node when a `nest_field` chain turns a direct source arc
        // into a two-hop target path.
        use crate::asymmetric;
        use crate::tests::three_node_instance;

        let schema = three_node_schema();
        let instance = three_node_instance();
        let protocol = test_protocol();
        let chain = super::combinators::nest_field(
            "post:body",
            "post:body.text",
            "post:body.profile", // new intermediate
            "object",
            "prop",
            Some(GatName::from("text")),
            "profile",
            "text",
        );
        let lens = chain.instantiate(&schema, &protocol).unwrap();

        // Expansion path should have been discovered during compilation.
        assert!(
            !lens.compiled.expansion_path.is_empty(),
            "compiled migration should contain an expansion_path entry"
        );
        let key = (GatName::from("post:body"), GatName::from("post:body.text"));
        let intermediates = lens
            .compiled
            .expansion_path
            .get(&key)
            .expect("expansion_path should cover the dropped direct arc");
        assert_eq!(
            intermediates,
            &vec![GatName::from("post:body.profile")],
            "expansion should route through the new intermediate"
        );

        // Forward eval: this is the exact call site that failed before the fix.
        let (view, complement) = asymmetric::get(&lens, &instance)
            .expect("forward eval should succeed on a nest_field chain");

        // The view must contain a synthesized node anchored at the
        // intermediate, plus both original surviving nodes.
        let has_intermediate = view
            .nodes
            .values()
            .any(|n| &*n.anchor == "post:body.profile");
        assert!(
            has_intermediate,
            "view should contain a synthesized node anchored at post:body.profile, got {:?}",
            view.nodes
                .values()
                .map(|n| n.anchor.clone())
                .collect::<Vec<_>>()
        );

        // Synthesized node must be recorded in the complement so `put` drops it.
        assert_eq!(
            complement.synthesized_nodes.len(),
            1,
            "exactly one node should have been synthesized"
        );
        let synth_id = *complement.synthesized_nodes.iter().next().unwrap();
        assert!(!instance.nodes.contains_key(&synth_id));

        // The two-hop chain must exist: someone --(profile)--> synth, synth --(text)--> text_node.
        let text_node_id = view
            .nodes
            .iter()
            .find(|(_, n)| &*n.anchor == "post:body.text")
            .map(|(id, _)| *id)
            .expect("surviving text node");
        let arc_to_text = view
            .arcs
            .iter()
            .find(|(_, c, _)| *c == text_node_id)
            .expect("arc pointing at text_node");
        assert_eq!(
            arc_to_text.0, synth_id,
            "text should be downstream of synth"
        );
        assert_eq!(arc_to_text.2.name.as_deref(), Some("text"));

        let arc_to_synth = view
            .arcs
            .iter()
            .find(|(_, c, _)| *c == synth_id)
            .expect("arc pointing at synth node");
        assert_eq!(arc_to_synth.2.name.as_deref(), Some("profile"));

        // Sibling createdAt edge must survive forward eval.
        let has_createdat = view
            .arcs
            .iter()
            .any(|(_, _, e)| e.name.as_deref() == Some("createdAt"));
        assert!(
            has_createdat,
            "createdAt sibling arc should survive nest forward eval"
        );
    }

    #[test]
    fn nest_field_get_put_round_trip_recovers_source() {
        // After `get` synthesizes the intermediate, `put` must collapse
        // it back and reproduce the original flat source.
        use crate::asymmetric;
        use crate::tests::three_node_instance;

        let schema = three_node_schema();
        let instance = three_node_instance();
        let protocol = test_protocol();
        let chain = super::combinators::nest_field(
            "post:body",
            "post:body.text",
            "post:body.profile",
            "object",
            "prop",
            Some(GatName::from("text")),
            "profile",
            "text",
        );
        let lens = chain.instantiate(&schema, &protocol).unwrap();

        let (view, complement) = asymmetric::get(&lens, &instance).unwrap();
        let restored = asymmetric::put(&lens, &view, &complement).unwrap();

        // The restored instance should have exactly the same node set and
        // arc set as the source (up to node id equality, which is the
        // expectation since put preserves ids for surviving nodes).
        assert_eq!(
            restored.nodes.len(),
            instance.nodes.len(),
            "restored instance should have the same number of nodes as source"
        );
        for id in instance.nodes.keys() {
            assert!(
                restored.nodes.contains_key(id),
                "source node {id} should be restored"
            );
        }
        // The synthesized intermediate must NOT appear in the restored instance.
        assert!(
            !restored
                .nodes
                .values()
                .any(|n| &*n.anchor == "post:body.profile"),
            "synthesized intermediate must be dropped by put"
        );

        // The original direct text arc must be back.
        let has_direct_text_arc = restored.arcs.iter().any(|(p, c, e)| {
            instance
                .nodes
                .get(p)
                .is_some_and(|n| &*n.anchor == "post:body")
                && instance
                    .nodes
                    .get(c)
                    .is_some_and(|n| &*n.anchor == "post:body.text")
                && e.name.as_deref() == Some("text")
        });
        assert!(
            has_direct_text_arc,
            "put should restore the original direct `text` arc"
        );
    }

    /// Build a nested `root --(intermediate)--> intermediate --(leaf)--> leaf`
    /// schema, mirroring protolab's `nested_schema` helper. Both edges are
    /// `prop` edges whose name equals the target vertex id.
    fn nested_schema(
        root: &str,
        intermediate: &str,
        leaf: &str,
        leaf_kind: &str,
    ) -> panproto_schema::Schema {
        use std::collections::HashMap;
        let mut vertices: HashMap<GatName, Vertex> = HashMap::new();
        let mut edges: HashMap<SchemaEdge, GatName> = HashMap::new();
        let mut outgoing: HashMap<GatName, smallvec::SmallVec<SchemaEdge, 4>> = HashMap::new();
        let mut incoming: HashMap<GatName, smallvec::SmallVec<SchemaEdge, 4>> = HashMap::new();
        let mut between: HashMap<(GatName, GatName), smallvec::SmallVec<SchemaEdge, 2>> =
            HashMap::new();

        for (id, kind) in [
            (root, "object"),
            (intermediate, "object"),
            (leaf, leaf_kind),
        ] {
            vertices.insert(
                GatName::from(id),
                Vertex {
                    id: id.into(),
                    kind: kind.into(),
                    nsid: None,
                },
            );
        }

        let mut add_edge = |src: &str, tgt: &str, name: &str| {
            let e = SchemaEdge {
                src: GatName::from(src),
                tgt: GatName::from(tgt),
                kind: "prop".into(),
                name: Some(GatName::from(name)),
            };
            outgoing
                .entry(GatName::from(src))
                .or_default()
                .push(e.clone());
            incoming
                .entry(GatName::from(tgt))
                .or_default()
                .push(e.clone());
            between
                .entry((GatName::from(src), GatName::from(tgt)))
                .or_default()
                .push(e.clone());
            edges.insert(e, GatName::from("prop"));
        };
        add_edge(root, intermediate, intermediate);
        add_edge(intermediate, leaf, leaf);

        panproto_schema::Schema {
            protocol: format!("test-{root}"),
            vertices,
            edges,
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
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            entries: vec![GatName::from(root)],
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn hoist_field_get_put_json_round_trip_restores_nested_record() {
        // Regression for the protolab `hoist_field_round_trip_unmodified_recovers_input`
        // failure: hoisting `name` out of `profile` under `user`, then `put`,
        // must restore `{"profile":{"name":"Alice"}}` byte-for-byte. The
        // 0.52.0 first-class `Value::List` change exposed a latent `put`
        // double-arc bug that made the reconstructed `profile` record
        // serialize as a duplicated 2-element list `["Alice","Alice"]`.
        //
        // This locks the panproto-level contract: `get` then `put` on the
        // hoist lens reproduces the source instance, and serializing that
        // source yields the original nested JSON exactly.
        use crate::asymmetric;
        use panproto_inst::parse::{parse_json, to_json};

        let source = nested_schema("user", "profile", "name", "string");
        let protocol = test_protocol();
        let chain = super::combinators::hoist_field("user", "profile", "name");
        let lens = chain
            .instantiate(&source, &protocol)
            .expect("hoist chain instantiate");

        let input: serde_json::Value = serde_json::json!({"profile": {"name": "Alice"}});
        let instance = parse_json(&source, "user", &input).expect("parse input");

        let (view, complement) = asymmetric::get(&lens, &instance).expect("get");
        let restored = asymmetric::put(&lens, &view, &complement).expect("put");

        let restored_json = to_json(&source, &restored);
        assert_eq!(
            restored_json, input,
            "hoist get/put must restore the source JSON byte-for-byte, got {restored_json}"
        );
        assert!(
            restored_json["profile"].is_object(),
            "restored profile must be a JSON object (record), not a list: {restored_json}"
        );
        assert_eq!(restored_json["profile"]["name"], serde_json::json!("Alice"));
    }

    #[test]
    fn hoist_field_put_then_json_does_not_listify_record() {
        // Tighter variant that exercises the exact get/put path (no
        // view re-parse) and inspects the JSON shape of the dropped sort.
        use crate::asymmetric;
        use panproto_inst::parse::{parse_json, to_json};

        let source = nested_schema("user", "profile", "name", "string");
        let protocol = test_protocol();
        let chain = super::combinators::hoist_field("user", "profile", "name");
        let lens = chain
            .instantiate(&source, &protocol)
            .expect("hoist chain instantiate");

        let input: serde_json::Value = serde_json::json!({"profile": {"name": "Alice"}});
        let instance = parse_json(&source, "user", &input).expect("parse input");

        let (view, complement) = asymmetric::get(&lens, &instance).expect("get");
        let restored = asymmetric::put(&lens, &view, &complement).expect("put");
        let restored_json = to_json(&source, &restored);

        assert!(
            restored_json["profile"].is_object(),
            "dropped `profile` sort must reconstruct as a record, not a Value::List: {restored_json}"
        );
        assert_eq!(restored_json["profile"]["name"], serde_json::json!("Alice"));
    }

    #[test]
    fn drop_edge_schema_apply_rebuilds_indices() {
        // Build a tiny 2-vertex schema with a single named edge and
        // verify drop_edge rebuilds `outgoing`/`incoming`/`between`.
        use std::collections::HashMap;
        let mut vertices = HashMap::new();
        vertices.insert(
            GatName::from("a"),
            Vertex {
                id: "a".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        vertices.insert(
            GatName::from("b"),
            Vertex {
                id: "b".into(),
                kind: "string".into(),
                nsid: None,
            },
        );
        let edge = SchemaEdge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("label".into()),
        };
        let mut edges = HashMap::new();
        edges.insert(edge.clone(), GatName::from("prop"));
        let mut outgoing = HashMap::new();
        outgoing.insert(GatName::from("a"), smallvec![edge.clone()]);
        let mut incoming = HashMap::new();
        incoming.insert(GatName::from("b"), smallvec![edge.clone()]);
        let mut between = HashMap::new();
        between.insert((GatName::from("a"), GatName::from("b")), smallvec![edge]);
        let schema = panproto_schema::Schema {
            protocol: "test".into(),
            vertices,
            edges,
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
            outgoing,
            incoming,
            between,
        };

        let protocol = test_protocol();
        let p = elementary::drop_edge("a", "b", Some(GatName::from("label")));
        let lens = p.instantiate(&schema, &protocol).unwrap();
        assert_eq!(lens.tgt_schema.edges.len(), 0);
        // rebuild_indices should have cleared out the adjacency maps.
        let out = lens.tgt_schema.outgoing.get(&GatName::from("a"));
        assert!(
            out.is_none_or(SmallVec::is_empty),
            "outgoing should be empty"
        );
        let inc = lens.tgt_schema.incoming.get(&GatName::from("b"));
        assert!(
            inc.is_none_or(SmallVec::is_empty),
            "incoming should be empty"
        );
    }
}

/// Law-check the panproto-lens realization
/// of every DSL `Step` constructor.
///
/// The DSL `Step` enum (`panproto-lens-dsl::document::Step`) has 19
/// variants; each compiles to a panproto-lens construct — an elementary
/// protolens, a `combinators` chain, or a `CompiledMigration`
/// `FieldTransform`. `panproto-lens` sits *below* `panproto-lens-dsl` in
/// the dependency graph, so it cannot import the `Step` enum or
/// `compile_steps` directly; the DSL-front-end test
/// (`panproto-lens-dsl/tests/step_laws.rs`, which compiles real
/// `LensDocument`s) is a follow-up for the lane that owns that crate.
///
/// This module instead instantiates each step's *compile target* against
/// a fixture and runs [`crate::laws::check_get_put`], so every construct
/// the DSL emits is law-checked where it lives. Coverage of the 19 step
/// kinds:
///
/// - Structural/elementary steps are law-checked here: `rename_sort`,
///   `add_sort`, `drop_sort` (= `remove_field`), `rename_op`, `add_op`,
///   `drop_op`, `add_equation`, `drop_equation`, `rename_field`
///   (= `rename_edge_name`), `coerce_sort`, `scoped`, `pullback`,
///   `hoist_field`, `nest_field`, `add_field`, and `merge_sorts`
///   (the lossy case, asserted via its complement).
/// - The field-level steps `apply_expr` and `compute_field` are
///   law-checked as proptests over generated instances in
///   [`crate::laws`] (`field_apply_expr_satisfies_get_put`,
///   `identity_lens_with_compute_field_satisfies_getput`), alongside the
///   `RemoveField`/`RenameField`/`AddField` field-transform proptests.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod step_laws {
    use std::collections::HashMap;

    use panproto_gat::{CoercionClass, Equation, Name, Term, TheoryMorphism, ValueKind};
    use panproto_inst::value::{FieldPresence, Value};
    use panproto_inst::{Node, WInstance};
    use panproto_schema::{Edge, Protocol, Schema, Vertex};
    use smallvec::SmallVec;

    use super::{ComplementConstructor, Protolens, ProtolensChain, combinators, elementary};

    fn step_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
        let mut vertices = HashMap::new();
        let mut edges = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in verts {
            vertices.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        for e in edge_list {
            edges.insert(e.clone(), e.kind.clone());
            outgoing.entry(e.src.clone()).or_default().push(e.clone());
            incoming.entry(e.tgt.clone()).or_default().push(e.clone());
            between
                .entry((e.src.clone(), e.tgt.clone()))
                .or_default()
                .push(e.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
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
            outgoing,
            incoming,
            between,
        }
    }

    fn edge(src: &str, tgt: &str, kind: &str, label: &str) -> Edge {
        Edge {
            src: Name::from(src),
            tgt: Name::from(tgt),
            kind: Name::from(kind),
            name: Some(Name::from(label)),
        }
    }

    /// Fixture: `doc` object with a `title` string child and a nested
    /// `meta` object carrying an `author` string grandchild.
    fn nested_fixture() -> (Schema, WInstance) {
        let verts = [
            ("doc", "object"),
            ("doc.title", "string"),
            ("doc.meta", "object"),
            ("doc.author", "string"),
        ];
        let edges = vec![
            edge("doc", "doc.title", "prop", "title"),
            edge("doc", "doc.meta", "prop", "meta"),
            edge("doc.meta", "doc.author", "prop", "author"),
        ];
        let schema = make_schema(&verts, &edges);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "doc"));
        nodes.insert(
            1,
            Node::new(1, "doc.title").with_value(FieldPresence::Present(Value::Str("T".into()))),
        );
        nodes.insert(2, Node::new(2, "doc.meta"));
        nodes.insert(
            3,
            Node::new(3, "doc.author").with_value(FieldPresence::Present(Value::Str("A".into()))),
        );
        let arcs = vec![
            (0, 1, edges[0].clone()),
            (0, 2, edges[1].clone()),
            (2, 3, edges[2].clone()),
        ];
        let instance = WInstance::new(nodes, arcs, vec![], 0, Name::from("doc"));
        (schema, instance)
    }

    /// Fixture with a `ghost` vertex reachable by an `attr`-kind edge that
    /// has *no* instance node. Dropping that edge or its op therefore does
    /// not orphan any surviving node, so the drop round-trips cleanly.
    fn detachable_fixture() -> (Schema, WInstance) {
        let verts = [
            ("doc", "object"),
            ("doc.title", "string"),
            ("doc.ghost", "string"),
        ];
        let edges = vec![
            edge("doc", "doc.title", "prop", "title"),
            edge("doc", "doc.ghost", "attr", "ghost"),
        ];
        let schema = make_schema(&verts, &edges);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "doc"));
        nodes.insert(
            1,
            Node::new(1, "doc.title").with_value(FieldPresence::Present(Value::Str("T".into()))),
        );
        // No node for `doc.ghost`: the attr edge is schema-only.
        let arcs = vec![(0, 1, edges[0].clone())];
        let instance = WInstance::new(nodes, arcs, vec![], 0, Name::from("doc"));
        (schema, instance)
    }

    /// Instantiate a single protolens and check `GetPut` on the nested
    /// fixture.
    fn assert_step_law(p: Protolens) {
        let (schema, instance) = nested_fixture();
        assert_law(&ProtolensChain::new(vec![p]), &schema, &instance);
    }

    /// Instantiate a chain and check `GetPut` on the nested fixture.
    fn assert_chain_law(chain: &ProtolensChain) {
        let (schema, instance) = nested_fixture();
        assert_law(chain, &schema, &instance);
    }

    /// Instantiate a single protolens and check `GetPut` on a supplied
    /// fixture.
    fn assert_step_law_on(p: Protolens, schema: &Schema, instance: &WInstance) {
        assert_law(&ProtolensChain::new(vec![p]), schema, instance);
    }

    /// Instantiate a chain and check `GetPut` on a supplied fixture.
    fn assert_law(chain: &ProtolensChain, schema: &Schema, instance: &WInstance) {
        let proto = step_protocol();
        let lens = chain
            .instantiate(schema, &proto)
            .expect("chain should instantiate on the fixture");
        let result = crate::laws::check_get_put(&lens, instance);
        assert!(result.is_ok(), "GetPut should hold: {result:?}");
    }

    // --- sort-level steps ---

    #[test]
    fn step_law_rename_sort() {
        assert_step_law(elementary::rename_sort("string", "text"));
    }

    #[test]
    fn step_law_add_sort() {
        assert_step_law(elementary::add_sort("extra", "string", Value::Null));
    }

    #[test]
    fn step_law_drop_sort() {
        // Drop a single leaf vertex by id; the complement restores it.
        assert_step_law(elementary::drop_sort("doc.title"));
    }

    #[test]
    fn step_law_remove_field() {
        assert_chain_law(&combinators::remove_field("doc.title"));
    }

    // --- op/edge-level steps ---

    #[test]
    fn step_law_rename_op() {
        assert_step_law(elementary::rename_op("prop", "field"));
    }

    #[test]
    fn step_law_add_op() {
        assert_step_law(elementary::add_op("extra_op", "doc", "doc.title", "prop"));
    }

    #[test]
    fn step_law_drop_op() {
        // Drop the `attr` op, whose only edge targets a vertex with no
        // instance node, so no surviving node is orphaned.
        let (schema, instance) = detachable_fixture();
        assert_step_law_on(elementary::drop_op("attr"), &schema, &instance);
    }

    #[test]
    fn step_law_add_edge() {
        assert_step_law(elementary::add_edge("doc", "doc.title", "alt", "prop"));
    }

    #[test]
    fn step_law_drop_edge() {
        // Drop the schema-only `attr` edge (its target has no instance
        // node), which round-trips cleanly.
        let (schema, instance) = detachable_fixture();
        assert_step_law_on(
            elementary::drop_edge("doc", "doc.ghost", Some(Name::from("ghost"))),
            &schema,
            &instance,
        );
    }

    #[test]
    fn step_law_rename_field() {
        // The DSL `RenameField` step compiles to `rename_edge_name`.
        assert_chain_law(&combinators::rename_field(
            "doc",
            "doc.title",
            "title",
            "heading",
        ));
    }

    // --- equation-level steps ---

    #[test]
    fn step_law_add_equation() {
        let eq = Equation::new("refl", Term::var("x"), Term::var("x"));
        assert_step_law(elementary::add_equation(eq));
    }

    #[test]
    fn step_law_drop_equation() {
        // No such equation is present; the transform is a theory-level
        // no-op with an empty complement, so the lens is the identity.
        assert_step_law(elementary::drop_equation("nonexistent_eq"));
    }

    #[test]
    fn step_law_directed_eq() {
        let deq = panproto_gat::DirectedEquation {
            name: std::sync::Arc::from("id_deq"),
            lhs: Term::var("x"),
            rhs: Term::var("x"),
            impl_term: panproto_expr::Expr::var("x"),
            inverse: Some(panproto_expr::Expr::var("x")),
            source_kind: Some(ValueKind::Str),
            target_kind: Some(ValueKind::Str),
            coercion_class: CoercionClass::Iso,
        };
        assert_step_law(elementary::directed_eq(deq));
    }

    #[test]
    fn step_law_drop_directed_eq() {
        assert_step_law(elementary::drop_directed_eq("nonexistent_deq"));
    }

    // --- structural / higher-order steps ---

    #[test]
    fn step_law_scoped() {
        // Scope a rename within the `doc.meta` sub-schema.
        let inner = elementary::rename_sort("string", "text");
        assert_step_law(elementary::scoped("doc.meta", inner));
    }

    #[test]
    fn step_law_pullback() {
        // Pullback along the identity morphism of the fixture's implicit
        // theory is a lossless no-op.
        let (schema, _) = nested_fixture();
        let theory = super::schema_to_implicit_theory(&schema);
        let morphism = TheoryMorphism::identity(&theory);
        assert_step_law(elementary::pullback(morphism));
    }

    #[test]
    fn step_law_coerce_sort() {
        // An Iso identity coercion on the `string` sort is lossless.
        assert_step_law(elementary::sort_coerce(
            "string",
            ValueKind::Str,
            panproto_expr::Expr::var("x"),
            Some(panproto_expr::Expr::var("x")),
            CoercionClass::Iso,
        ));
    }

    #[test]
    fn step_law_hoist_field() {
        // Hoist `doc.author` from under `doc.meta` up to `doc`.
        assert_chain_law(&combinators::hoist_field("doc", "doc.meta", "doc.author"));
    }

    #[test]
    fn step_law_nest_field() {
        // Nest `doc.title` under a fresh intermediate object.
        assert_chain_law(&combinators::nest_field(
            "doc",
            "doc.title",
            "doc.wrapper",
            "object",
            "prop",
            Some(Name::from("title")),
            "wrapper",
            "title",
        ));
    }

    #[test]
    fn step_law_add_field() {
        assert_chain_law(&combinators::add_field(
            "doc",
            "doc.note",
            "string",
            Value::Str("default".into()),
        ));
    }

    #[test]
    fn step_law_merge_sorts() {
        // The DSL `MergeSorts` step is the lossy case: merging distinct
        // carriers into one is not generally invertible, so its compile
        // target declares an `Opaque` coercion whose complement retains
        // the pre-merge data. Rather than assert PutGet (which does not
        // hold for the merged component), assert the honest lossy shape:
        // the protolens is not lossless and its complement captures the
        // coerced sort data.
        let merge = elementary::sort_coerce(
            "string",
            ValueKind::Str,
            panproto_expr::Expr::Lit(panproto_expr::Literal::Str("merged".into())),
            None,
            CoercionClass::Opaque,
        );
        assert!(!merge.is_lossless(), "a merge/opaque coercion is lossy");
        assert!(
            matches!(
                merge.complement_constructor,
                ComplementConstructor::CoercedSortData {
                    class: CoercionClass::Opaque,
                    ..
                }
            ),
            "opaque coercion must retain coerced-sort complement data",
        );
    }
}
