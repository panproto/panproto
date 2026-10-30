//! Bounded protolens applicability and instance transformation.
//!
//! A protolens is not a raw migration alias. It is a schema-indexed
//! construction whose precondition is checked before either schema or
//! instance instantiation. Successful instance instantiation returns the
//! concrete compiled lens together with the view and its identity-bound
//! complement.

use std::collections::{BTreeMap, BTreeSet};

use super::graph::GraphLens;
use crate::migration::Migration;
use crate::model::{Graph, Name, SEdge};
use crate::restrict::{Complement, NoValueLayer, Plan, PlanError, RestrictError};
use crate::schema::Schema;
use crate::validation::Semantics;

/// A finite, decidable precondition on a schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaPrecondition {
    /// Every schema is admitted.
    Any,
    /// The complete canonical schema identity must be this byte string.
    Exact(Vec<u8>),
    /// Every named vertex must be present.
    HasVertices(BTreeSet<Name>),
    /// Every named edge must be present.
    HasEdges(BTreeSet<SEdge>),
    /// Every component precondition must hold, in listed order.
    All(Vec<Self>),
}

impl SchemaPrecondition {
    /// Test the precondition without compiling a migration.
    #[must_use]
    pub fn holds(&self, schema: &Schema) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => crate::schema::canonical_bytes(schema) == *expected,
            Self::HasVertices(required) => required
                .iter()
                .all(|name| schema.vertices.contains_key(name)),
            Self::HasEdges(required) => required.is_subset(&schema.edges),
            Self::All(required) => required.iter().all(|condition| condition.holds(schema)),
        }
    }
}

/// A reusable schema-to-lens construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Protolens {
    /// Applicability condition checked before compilation.
    pub precondition: SchemaPrecondition,
    /// Target boundary produced by the schema transformation.
    pub target: Schema,
    /// Migration instantiated between the supplied source and target.
    pub migration: Migration,
}

impl Protolens {
    /// Construct a bounded protolens.
    #[must_use]
    pub const fn new(
        precondition: SchemaPrecondition,
        target: Schema,
        migration: Migration,
    ) -> Self {
        Self {
            precondition,
            target,
            migration,
        }
    }

    /// Construct the exact-boundary protolens corresponding to one concrete
    /// source, target, and migration.
    #[must_use]
    pub fn exact(source: &Schema, target: &Schema, migration: &Migration) -> Self {
        Self::new(
            SchemaPrecondition::Exact(crate::schema::canonical_bytes(source)),
            target.clone(),
            migration.clone(),
        )
    }

    /// Whether this protolens applies to `source`.
    #[must_use]
    pub fn applicable_to(&self, source: &Schema) -> bool {
        self.precondition.holds(source)
    }

    /// Instantiate the schema transformation as a concrete compiled lens.
    ///
    /// # Errors
    ///
    /// [`ProtolensError::FalsePrecondition`] precedes plan compilation.
    pub fn instantiate_schema(&self, source: &Schema) -> Result<InstantiatedLens, ProtolensError> {
        if !self.applicable_to(source) {
            return Err(ProtolensError::FalsePrecondition {
                precondition: self.precondition.clone(),
                found_schema: crate::schema::canonical_bytes(source),
            });
        }
        let plan = crate::restrict::compile(source, &self.target, &self.migration)
            .map_err(ProtolensError::Plan)?;
        Ok(InstantiatedLens {
            precondition: self.precondition.clone(),
            source: source.clone(),
            target: self.target.clone(),
            migration: self.migration.clone(),
            plan,
            constraint_semantics: BTreeMap::new(),
        })
    }

    /// Instantiate and apply this protolens to an instance.
    ///
    /// The returned application owns the instantiated lens and an exact
    /// identity-bound complement, so a later put cannot detach residue from
    /// the schema and migration that produced it.
    ///
    /// # Errors
    ///
    /// A false schema precondition, plan refusal, or restrict refusal.
    pub fn instantiate_instance(
        &self,
        source_schema: &Schema,
        source: &Graph,
    ) -> Result<ProtolensApplication, ProtolensError> {
        let lens = self.instantiate_schema(source_schema)?;
        let (view, complement) = lens
            .as_graph_lens()
            .map_err(ProtolensError::Plan)?
            .get(source)
            .map_err(ProtolensError::Restrict)?;
        Ok(ProtolensApplication {
            lens,
            view,
            complement,
        })
    }
}

/// A protolens bound to one exact source schema, target schema, migration,
/// and compiled plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstantiatedLens {
    precondition: SchemaPrecondition,
    source: Schema,
    target: Schema,
    migration: Migration,
    plan: Plan,
    constraint_semantics: BTreeMap<Name, Semantics>,
}

impl InstantiatedLens {
    /// Source boundary to which this lens is bound.
    #[must_use]
    pub const fn source(&self) -> &Schema {
        &self.source
    }

    /// Target boundary to which this lens is bound.
    #[must_use]
    pub const fn target(&self) -> &Schema {
        &self.target
    }

    /// Checked applicability condition retained from the protolens.
    #[must_use]
    pub const fn precondition(&self) -> &SchemaPrecondition {
        &self.precondition
    }

    /// Borrow this owned binding as the graph lens used by get and put.
    ///
    /// # Errors
    ///
    /// A defensive plan-binding mismatch.
    pub fn as_graph_lens(&self) -> Result<GraphLens<'_>, PlanError> {
        GraphLens::new(
            &self.source,
            &self.target,
            &self.migration,
            &self.plan,
            &self.constraint_semantics,
            &NoValueLayer,
        )
    }
}

/// Successful instance instantiation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtolensApplication {
    /// The concrete lens selected by schema instantiation.
    pub lens: InstantiatedLens,
    /// Forward instance image.
    pub view: Graph,
    /// Exact complement bound to `lens`'s identity.
    pub complement: Complement,
}

/// A protolens refuses before producing a partially bound application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtolensError {
    /// The supplied schema does not satisfy the declared precondition.
    FalsePrecondition {
        /// The failed condition.
        precondition: SchemaPrecondition,
        /// Complete canonical identity of the supplied schema.
        found_schema: Vec<u8>,
    },
    /// The source/target/migration triple could not compile.
    Plan(PlanError),
    /// Instance transformation refused.
    Restrict(RestrictError),
}

/// Definition: Instantiate a protolens's schema transformation only after
/// checking its precondition.
///
/// # Errors
///
/// False precondition or plan compilation refusal.
///
/// Book anchors: `L-lenses-19`.
pub fn protolens_schema(
    protolens: &Protolens,
    source: &Schema,
) -> Result<InstantiatedLens, ProtolensError> {
    protolens.instantiate_schema(source)
}

/// Definition: Instantiate and apply a protolens only after checking its
/// schema precondition.
///
/// # Errors
///
/// False precondition, plan refusal, or restrict refusal.
///
/// Book anchors: `L-lenses-20`.
pub fn protolens_instance(
    protolens: &Protolens,
    source_schema: &Schema,
    source: &Graph,
) -> Result<ProtolensApplication, ProtolensError> {
    protolens.instantiate_instance(source_schema, source)
}

/// The identity-stamping graph-lens get operation.
///
/// # Errors
///
/// The first restrict refusal.
///
/// Book anchors: `L-lenses-1`, `L-lenses-2`, `L-lenses-3`, `L-lenses-4`,
/// `L-lenses-5`, `L-lenses-6`, `L-lenses-7`, `L-lenses-8`, `L-lenses-9`,
/// `L-lenses-10`.
pub fn get(lens: &GraphLens<'_>, source: &Graph) -> Result<(Graph, Complement), RestrictError> {
    lens.get(source)
}
