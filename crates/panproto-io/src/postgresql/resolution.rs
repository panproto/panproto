//! Foreign keys resolved to the rows they reference, through the shared
//! reference resolution.
//!
//! A table schema the DDL reader builds declares each foreign key as a
//! [`Reference`](panproto_schema::Reference) from the holding table's key
//! columns to a primary or unique key of the referenced table, with its
//! `MATCH` behaviour as the reference's null match and its nullability
//! from its columns. [`ForeignKeyResolution`] is the provider of the
//! [`ResolutionKind::TableToGraph`] lens over such a schema: the semantic
//! view is the table's graph form with one `foreign-key` edge from each
//! row to the row its key values name. A row whose foreign key has a
//! `NULL` part (any part under `MATCH SIMPLE`, every part under `MATCH
//! FULL`) references nothing; a key value no row holds, or one more than
//! one row holds, is refused, or recorded in the complement when the run
//! is lenient.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::{FInstance, GInstance};
use panproto_schema::{Protocol, Schema};

use crate::resolution::{
    AnyInstance, Resolution, ResolutionComplement, ResolutionKind, ResolutionProvider,
    ResolutionProviderId, ResolutionSpec, ResolveContext, ResolveError, ResolveOptions,
};

/// The resolution provider of the `postgresql` protocol's foreign keys.
#[derive(Clone, Debug)]
pub struct ForeignKeyResolution {
    protocol: Protocol,
}

impl ForeignKeyResolution {
    /// The provider over the `postgresql` protocol.
    #[must_use]
    pub fn new() -> Self {
        Self {
            protocol: panproto_protocols::database::postgresql::protocol(),
        }
    }
}

impl Default for ForeignKeyResolution {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolutionProvider for ForeignKeyResolution {
    fn id(&self) -> ResolutionProviderId {
        ResolutionProviderId {
            name: Name::from("postgresql"),
            version: 1,
        }
    }

    fn kind(&self) -> ResolutionKind {
        ResolutionKind::TableToGraph
    }

    fn protocol(&self) -> &Protocol {
        &self.protocol
    }

    fn spec(
        &self,
        _protocol: &Protocol,
        syntactic: &Schema,
    ) -> Result<ResolutionSpec, ResolveError> {
        let writers: BTreeMap<_, Name> = syntactic
            .declarations
            .references
            .values()
            .filter_map(|r| r.edge.clone().map(|edge| (edge, r.id.clone())))
            .collect();
        let reference_edges: BTreeSet<_> = writers.keys().cloned().collect();
        Ok(ResolutionSpec {
            kind: ResolutionKind::TableToGraph,
            provider: self.id(),
            semantic_schema: syntactic.clone(),
            protocol_references: BTreeMap::new(),
            structural_edges: syntactic
                .edges
                .keys()
                .filter(|edge| !reference_edges.contains(*edge))
                .cloned()
                .collect(),
            reference_writers: writers,
            context_requirements: BTreeSet::new(),
            assembly: None,
        })
    }
}

/// Resolve the foreign keys of `instance` under `schema`: the instance's
/// graph form with one `foreign-key` edge per resolved reference, and the
/// complement that writes the view back.
///
/// # Errors
///
/// A [`ResolveError`] when the schema's references do not compile, or,
/// unless `options.lenient`, when a foreign key's value names no row or
/// more than one.
pub fn resolve_foreign_keys(
    schema: &Schema,
    instance: &FInstance,
    options: ResolveOptions,
) -> Result<(GInstance, ResolutionComplement), ResolveError> {
    let provider = ForeignKeyResolution::new();
    let protocol = provider.protocol.clone();
    let resolution = Resolution::for_protocol(&protocol, schema, Arc::new(provider))?;
    let (view, complement) = resolution.get(
        &AnyInstance::Table(instance.clone()),
        &options,
        &ResolveContext::empty(),
    )?;
    match view {
        AnyInstance::Graph(graph) => Ok((graph, complement)),
        other => Err(ResolveError::ShapeMismatch {
            expected: panproto_schema::InstanceShapeKind::Graph,
            found: other.shape(),
        }),
    }
}
