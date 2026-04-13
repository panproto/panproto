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
    /// specific schema.
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

/// Vertical composition of protolenses: given `η : F ⟹ G` and
/// `θ : G ⟹ H`, produce `θ ∘ η : F ⟹ H`.
///
/// The target endofunctor of `eta` must match the source of `theta`.
/// This is checked dynamically at instantiation time.
///
/// # Errors
///
/// Currently infallible, but returns `Result` for future compatibility
/// with static compatibility checks.
pub fn vertical_compose(eta: &Protolens, theta: &Protolens) -> Result<Protolens, LensError> {
    // Note: in this codebase, vertical composition computes the direct
    // migration between eta.source(S) and theta.target(S) at
    // instantiation time, rather than composing individual migrations
    // through an intermediate schema. This means eta.target and
    // theta.source do NOT need to match structurally (unlike standard
    // natural transformation composition). Compatibility is verified
    // implicitly when instantiate() calls compute_migration_between().

    let complement = ComplementConstructor::Composite(vec![
        eta.complement_constructor.clone(),
        theta.complement_constructor.clone(),
    ]);

    Ok(Protolens {
        name: Name::from(format!("{}.{}", theta.name, eta.name)),
        source: eta.source.clone(),
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
    /// the first step must be applicable.
    #[must_use]
    pub fn applicable_to(&self, schema: &Schema) -> bool {
        if self.steps.is_empty() {
            return true;
        }
        self.steps[0].applicable_to(schema)
    }

    /// Instantiate the chain at a specific schema, producing a composed
    /// [`Lens`].
    ///
    /// Each step is instantiated at the current schema, and the resulting
    /// lenses are composed sequentially.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ProtolensError`] if any step fails, or
    /// [`LensError::CompositionMismatch`] if lens composition fails.
    pub fn instantiate(&self, schema: &Schema, protocol: &Protocol) -> Result<Lens, LensError> {
        if self.steps.is_empty() {
            return Ok(identity_lens(schema));
        }
        if self.steps.len() == 1 {
            return self.steps[0].instantiate(schema, protocol);
        }
        // Fuse and instantiate as a single protolens
        let fused = self.fuse()?;
        fused.instantiate(schema, protocol)
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

    /// Fuse all steps into a single `Protolens` by composing endofunctors.
    ///
    /// The fused protolens applies all transforms in one pass, avoiding
    /// intermediate schema materialization. The complement constructor
    /// becomes `Composite` of all individual complements.
    ///
    /// # Errors
    ///
    /// Returns `LensError::ProtolensError` if the chain is empty.
    pub fn fuse(&self) -> Result<Protolens, LensError> {
        if self.steps.is_empty() {
            return Err(LensError::ProtolensError("cannot fuse empty chain".into()));
        }
        if self.steps.len() == 1 {
            return Ok(self.steps[0].clone());
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
            let lifted = morphism.op_map.get(o).unwrap_or(o);
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
    /// # Historical note
    ///
    /// Prior to this signature, `nest_field` silently assumed that the
    /// child vertex id equalled the edge label (so a single `Name` stood
    /// in for both), and dropped the original edge by edge *kind* rather
    /// than by name. Neither assumption holds for schemas built via
    /// `SchemaBuilder::add_prop`, `ATProto` lexicons, or any protocol where
    /// edge labels are short JSON keys and vertex ids are path-qualified.
    /// See panproto/panproto#23.
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
#[allow(clippy::too_many_lines, clippy::only_used_in_recursion)]
fn apply_theory_transform_to_schema(
    transform: &TheoryTransform,
    schema: &Schema,
    protocol: &Protocol,
) -> Result<Schema, LensError> {
    match transform {
        TheoryTransform::Identity => Ok(schema.clone()),
        TheoryTransform::CoerceSort {
            sort_name,
            coercion_expr,
            inverse_expr,
            coercion_class,
            ..
        } => {
            // Install the coercion expression in the schema's enrichment map.
            let mut new_schema = schema.clone();
            let name = Name::from(&**sort_name);
            new_schema.coercions.insert(
                (name.clone(), name),
                panproto_schema::CoercionSpec {
                    forward: coercion_expr.clone(),
                    inverse: inverse_expr.clone(),
                    class: *coercion_class,
                },
            );
            Ok(new_schema)
        }
        TheoryTransform::MergeSorts {
            sort_a,
            sort_b,
            merged_name,
            merger_expr,
        } => {
            // Merge two sorts into one: remove the source sorts, add the merged sort,
            // and install the merger expression.
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
            Ok(new_schema)
        }
        TheoryTransform::AddDirectedEquation(_) | TheoryTransform::DropDirectedEquation(_) => {
            // Directed equations modify the theory's rewrite rules but
            // do not change the schema's vertex/edge graph structure.
            Ok(schema.clone())
        }
        TheoryTransform::RenameSort { old, new } => {
            Ok(apply_rename_sort_to_schema(schema, old, new))
        }
        TheoryTransform::RenameOp { old, new } => Ok(apply_rename_op_to_schema(schema, old, new)),
        TheoryTransform::DropSort(name) => Ok(apply_drop_sort_from_schema(schema, name)),
        TheoryTransform::AddSort { sort, vertex_kind } => {
            let mut new_schema = schema.clone();
            let kind = vertex_kind
                .as_ref()
                .map_or_else(|| sort.default_vertex_kind(), Arc::clone);
            let vertex = Vertex {
                id: Name::from(&*sort.name),
                kind: Name::from(&*kind),
                nsid: None,
            };
            new_schema.vertices.insert(Name::from(&*sort.name), vertex);
            Ok(new_schema)
        }
        TheoryTransform::AddSortWithDefault {
            sort,
            vertex_kind,
            default_expr,
        } => {
            let mut new_schema = schema.clone();
            let name = Name::from(&*sort.name);
            let kind = vertex_kind
                .as_ref()
                .map_or_else(|| sort.default_vertex_kind(), Arc::clone);
            let vertex = Vertex {
                id: name.clone(),
                kind: Name::from(&*kind),
                nsid: None,
            };
            new_schema.vertices.insert(name.clone(), vertex);
            // Install the default expression so the migration engine can
            // compute values for this sort when lifting instances.
            new_schema.defaults.insert(name, default_expr.clone());
            Ok(new_schema)
        }
        TheoryTransform::DropOp(name) => Ok(apply_drop_op_from_schema(schema, name)),
        TheoryTransform::AddOp(op) => {
            // Adding an operation adds an edge to the schema. The operation's
            // first input sort is the source vertex, and the output sort is
            // the target vertex. If both endpoints exist, add the edge.
            let mut new_schema = schema.clone();
            if let Some((_, src_sort)) = op.inputs.first() {
                let src = Name::from(&**src_sort);
                let tgt = Name::from(&*op.output);
                if new_schema.vertices.contains_key(&src) && new_schema.vertices.contains_key(&tgt)
                {
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
                }
            }
            Ok(new_schema)
        }
        TheoryTransform::AddEquation(_) | TheoryTransform::DropEquation(_) => {
            // Equations don't change schema structure, only constraints.
            Ok(schema.clone())
        }
        TheoryTransform::Pullback(morphism) => {
            let mut result = schema.clone();
            for (old, new) in &morphism.sort_map {
                if old != new {
                    result = apply_rename_sort_to_schema(&result, old, new);
                }
            }
            for (old, new) in &morphism.op_map {
                if old != new {
                    result = apply_rename_op_to_schema(&result, old, new);
                }
            }
            Ok(result)
        }
        TheoryTransform::RenameEdgeName {
            src_sort,
            tgt_sort,
            old_name,
            new_name,
        } => {
            // Find and rename the edge label between src_sort and tgt_sort vertices.
            // Rebuild the schema with the renamed edge and fresh adjacency indices.
            let mut new_edges = HashMap::new();
            for (edge, kind) in &schema.edges {
                let mut e = edge.clone();
                if *e.src == **src_sort
                    && *e.tgt == **tgt_sort
                    && e.name.as_deref() == Some(&**old_name)
                {
                    e.name = Some(Name::from(&**new_name));
                }
                new_edges.insert(e, kind.clone());
            }
            // Rebuild adjacency indices from the new edge set.
            let mut outgoing: HashMap<Name, SmallVec<panproto_schema::Edge, 4>> = HashMap::new();
            let mut incoming: HashMap<Name, SmallVec<panproto_schema::Edge, 4>> = HashMap::new();
            let mut between: HashMap<(Name, Name), SmallVec<panproto_schema::Edge, 2>> =
                HashMap::new();
            for edge in new_edges.keys() {
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
            let mut new_schema = schema.clone();
            new_schema.edges = new_edges;
            new_schema.outgoing = outgoing;
            new_schema.incoming = incoming;
            new_schema.between = between;
            Ok(new_schema)
        }
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
            // Pushout construction: apply the inner transform to the sub-schema
            // reachable from the focus vertex, then merge back into the full schema.
            //
            // Given inclusion ι : Sub(S, focus) ↪ S and transform η on Sub(S, focus),
            // the result is the pushout S ∪_{Sub(S,focus)} η(Sub(S, focus)).

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
            //    Include only the vertices, edges, constraints, and defaults
            //    within the reachable set to form a well-formed sub-schema.
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

            // 4. Merge via pushout: start from the original schema, replace the
            //    reachable sub-schema with its transformed version.
            let mut result = schema.clone();
            // Remove old reachable vertices, edges, constraints, and defaults.
            result.vertices.retain(|id, _| !reachable.contains(id));
            result
                .edges
                .retain(|e, _| !(reachable.contains(&e.src) && reachable.contains(&e.tgt)));
            result.constraints.retain(|id, _| !reachable.contains(id));
            result.defaults.retain(|id, _| !reachable.contains(id));
            // Insert transformed sub-schema data.
            result.vertices.extend(transformed_sub.vertices);
            result.edges.extend(transformed_sub.edges);
            result.constraints.extend(transformed_sub.constraints);
            result.defaults.extend(transformed_sub.defaults);
            // Cross-boundary edges (from outside → reachable or reachable → outside)
            // are preserved since we only removed edges fully inside the reachable set.

            // 5. Rebuild adjacency indices.
            let mut outgoing: HashMap<Name, SmallVec<panproto_schema::Edge, 4>> = HashMap::new();
            let mut incoming: HashMap<Name, SmallVec<panproto_schema::Edge, 4>> = HashMap::new();
            let mut between: HashMap<(Name, Name), SmallVec<panproto_schema::Edge, 2>> =
                HashMap::new();
            for edge in result.edges.keys() {
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
            result.outgoing = outgoing;
            result.incoming = incoming;
            result.between = between;
            Ok(result)
        }
        TheoryTransform::Compose(first, second) => {
            let intermediate = apply_theory_transform_to_schema(first, schema, protocol)?;
            apply_theory_transform_to_schema(second, &intermediate, protocol)
        }
    }
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
    let mut sort_names: HashSet<&str> = HashSet::new();
    let mut sorts = Vec::new();
    for vertex in schema.vertices.values() {
        if sort_names.insert(&vertex.kind) {
            sorts.push(Sort::simple(name_arc_clone(&vertex.kind)));
        }
    }

    let mut op_names: HashSet<&str> = HashSet::new();
    let mut ops = Vec::new();
    for edge in schema.edges.keys() {
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
        schema_to_implicit_theory, vertical_compose,
    };
    use crate::tests::three_node_schema;

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
    // Regression tests for panproto/panproto#23:
    // `combinators::nest_field` against schemas with qualified vertex ids
    // (where the vertex id, e.g. `post:body.text`, is distinct from the
    // short edge label, e.g. `"text"`).
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
        // Regression for panproto/panproto#23. The original nest_field
        // assumed child vertex id == edge name and dropped edges by kind.
        // Here we use a qualified child id (`post:body.text`) with short
        // edge label (`"text"`), which broke the old implementation.
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
        // Regression for panproto/panproto#24: `asymmetric::get` must
        // synthesize a fresh intermediate view node when a nest_field
        // chain turns a direct source arc into a two-hop target path.
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
