//! The elementary protolens kinds of the lens DSL's step alphabet, at the
//! instance level.
//!
//! Each kind is one of three shapes on a schema: a relabelling of vertex
//! kinds and edges, a restriction to a subschema, or an extension by new
//! vertices and edges. A sort merge is a relabelling that sends two kinds to
//! one. Each shape is defined once, directly on the reference
//! schema, as a target boundary and a migration from the source. The
//! instance semantics of a kind at a schema is the graph lens of that
//! boundary and migration: get, put, the complement and every put-domain
//! refusal are those of [`super::graph::GraphLens`].

use std::collections::{BTreeMap, BTreeSet};

use super::protolens::{Protolens, SchemaPrecondition};
use crate::migration::Migration;
use crate::model::{Name, SEdge};
use crate::schema::Schema;

/// One elementary protolens with its parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementaryProtolens {
    /// Rename the vertex kind `old` to `new`.
    RenameSort {
        /// The kind renamed.
        old: Name,
        /// Its new name.
        new: Name,
    },
    /// Rename the edge kind `old` to `new` on every edge of that kind.
    RenameOp {
        /// The kind renamed.
        old: Name,
        /// Its new name.
        new: Name,
    },
    /// Rename every edge from `src` to `tgt` named `old` to `new`, except an
    /// edge whose renamed form is already a schema edge, which stays.
    RenameEdgeName {
        /// Source vertex of the renamed edges.
        src: Name,
        /// Target vertex of the renamed edges.
        tgt: Name,
        /// The name renamed.
        old: Name,
        /// Its new name.
        new: Name,
    },
    /// Pull back along a theory morphism whose sort and operation maps are
    /// renamings: every vertex kind at once, then every edge kind at once.
    Pullback {
        /// Vertex kind renaming.
        sorts: BTreeMap<Name, Name>,
        /// Edge kind renaming.
        ops: BTreeMap<Name, Name>,
    },
    /// Identify the vertex kinds `first` and `second` as `merged`: every
    /// vertex of either kind takes the kind `merged`, all at once.
    MergeSorts {
        /// One merged kind.
        first: Name,
        /// The other merged kind.
        second: Name,
        /// The kind both become.
        merged: Name,
    },
    /// Add an equation to the theory; a schema records no equations.
    AddEquation,
    /// Drop an equation from the theory; a schema records no equations.
    DropEquation,
    /// Drop every vertex whose id or kind is the name, with every edge and
    /// hyper-edge incident to a dropped vertex.
    DropSort(Name),
    /// Drop every edge and hyper-edge of the kind.
    DropOp(Name),
    /// Drop every edge from `src` to `tgt` with the name, of any kind.
    DropEdge {
        /// Source vertex of the dropped edges.
        src: Name,
        /// Target vertex of the dropped edges.
        tgt: Name,
        /// Name of the dropped edges.
        name: Option<Name>,
    },
    /// Add the vertex `sort` of `kind`, replacing the kind of a vertex that
    /// already has that id.
    AddSort {
        /// Id of the added vertex.
        sort: Name,
        /// Its kind.
        kind: Name,
    },
    /// Add the edge when both endpoints are schema vertices; otherwise the
    /// schema is unchanged.
    AddEdge(SEdge),
}

/// The shape of an elementary protolens kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementaryShape {
    /// A bijection on vertex kinds and edges; every vertex and edge survives.
    Relabel,
    /// The target is a subschema; the migration is its inclusion.
    Restrict,
    /// The source is a subschema of the target; every source vertex and edge
    /// survives under its own name.
    Extend,
}

/// A schema at which an elementary protolens has no lens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementaryRefusal {
    /// A relabelling sends two distinct edges to one edge, the least such
    /// pair in edge order.
    IdentifiesEdges {
        /// The lesser edge.
        first: Box<SEdge>,
        /// The greater edge.
        second: Box<SEdge>,
        /// Their common image.
        image: Box<SEdge>,
    },
}

impl ElementaryProtolens {
    /// The shape of this kind.
    #[must_use]
    pub const fn shape(&self) -> ElementaryShape {
        match self {
            Self::RenameSort { .. }
            | Self::MergeSorts { .. }
            | Self::RenameOp { .. }
            | Self::RenameEdgeName { .. }
            | Self::Pullback { .. }
            | Self::AddEquation
            | Self::DropEquation => ElementaryShape::Relabel,
            Self::DropSort(_) | Self::DropOp(_) | Self::DropEdge { .. } => {
                ElementaryShape::Restrict
            }
            Self::AddSort { .. } | Self::AddEdge(_) => ElementaryShape::Extend,
        }
    }

    /// The precondition this kind checks before instantiation.
    #[must_use]
    pub fn precondition(&self) -> SchemaPrecondition {
        match self {
            Self::RenameSort { old, .. } => SchemaPrecondition::HasVertexKind(old.clone()),
            Self::MergeSorts { first, second, .. } => SchemaPrecondition::All(vec![
                SchemaPrecondition::HasVertexKind(first.clone()),
                SchemaPrecondition::HasVertexKind(second.clone()),
            ]),
            Self::RenameOp { old, .. } | Self::DropOp(old) => {
                SchemaPrecondition::HasEdgeKind(old.clone())
            }
            Self::DropSort(name) => SchemaPrecondition::AnyOf(vec![
                SchemaPrecondition::HasVertexKind(name.clone()),
                SchemaPrecondition::HasVertices(BTreeSet::from([name.clone()])),
            ]),
            Self::RenameEdgeName { .. }
            | Self::Pullback { .. }
            | Self::AddEquation
            | Self::DropEquation
            | Self::DropEdge { .. }
            | Self::AddSort { .. }
            | Self::AddEdge(_) => SchemaPrecondition::Any,
        }
    }

    /// Definition: The target boundary and migration of an elementary
    /// protolens at a schema, by its shape.
    ///
    /// # Errors
    ///
    /// [`ElementaryRefusal::IdentifiesEdges`] when a relabelling is not
    /// injective on the schema's edges.
    ///
    /// Book anchors: `L-lenses-21`, `L-lenses-26`.
    pub fn target_and_migration(
        &self,
        schema: &Schema,
    ) -> Result<(Schema, Migration), ElementaryRefusal> {
        match self {
            Self::RenameSort { old, new } => relabel(
                schema,
                &BTreeMap::from([(old.clone(), new.clone())]),
                SEdge::clone,
            ),
            Self::MergeSorts {
                first,
                second,
                merged,
            } => relabel(
                schema,
                &BTreeMap::from([
                    (first.clone(), merged.clone()),
                    (second.clone(), merged.clone()),
                ]),
                SEdge::clone,
            ),
            Self::RenameOp { old, new } => {
                relabel(schema, &BTreeMap::new(), |edge| rename_kind(edge, old, new))
            }
            Self::Pullback { sorts, ops } => relabel(schema, sorts, |edge| {
                ops.get(&edge.kind)
                    .map_or_else(|| edge.clone(), |new| rename_kind(edge, &edge.kind, new))
            }),
            Self::RenameEdgeName { src, tgt, old, new } => {
                relabel(schema, &BTreeMap::new(), |edge| {
                    if edge.src == *src && edge.tgt == *tgt && edge.name.as_ref() == Some(old) {
                        let renamed = SEdge {
                            name: Some(new.clone()),
                            ..edge.clone()
                        };
                        if schema.edges.contains(&renamed) {
                            edge.clone()
                        } else {
                            renamed
                        }
                    } else {
                        edge.clone()
                    }
                })
            }
            Self::AddEquation | Self::DropEquation => {
                relabel(schema, &BTreeMap::new(), SEdge::clone)
            }
            Self::DropSort(name) => Ok(restrict(
                schema,
                |id, kind| id != name && kind != name,
                |_| true,
                |_| true,
            )),
            Self::DropOp(kind) => Ok(restrict(
                schema,
                |_, _| true,
                |edge| edge.kind != *kind,
                |hyper_kind| hyper_kind != kind,
            )),
            Self::DropEdge { src, tgt, name } => Ok(restrict(
                schema,
                |_, _| true,
                |edge| !(edge.src == *src && edge.tgt == *tgt && edge.name == *name),
                |_| true,
            )),
            Self::AddSort { sort, kind } => Ok(extend(schema, Some((sort, kind)), None)),
            Self::AddEdge(edge) => Ok(extend(schema, None, Some(edge))),
        }
    }

    /// Definition: The bounded protolens of an elementary kind at a schema:
    /// its precondition, target boundary and migration.
    ///
    /// # Errors
    ///
    /// As [`Self::target_and_migration`].
    ///
    /// Book anchors: `L-lenses-21`.
    pub fn at(&self, schema: &Schema) -> Result<Protolens, ElementaryRefusal> {
        let (target, migration) = self.target_and_migration(schema)?;
        Ok(Protolens::new(self.precondition(), target, migration))
    }
}

fn rename_kind(edge: &SEdge, old: &Name, new: &Name) -> SEdge {
    if edge.kind == *old {
        SEdge {
            kind: new.clone(),
            ..edge.clone()
        }
    } else {
        edge.clone()
    }
}

/// Definition: A relabelling renames vertex kinds by `kinds` and edges by
/// `edge_image`, simultaneously; every vertex and edge survives, and each
/// edge whose image differs is recorded in the edge remap.
///
/// Declarations keyed by an edge (required edges and orderings) follow the
/// edge to its image.
///
/// # Errors
///
/// [`ElementaryRefusal::IdentifiesEdges`] for the least pair of distinct
/// edges with one image.
///
/// Book anchors: `L-lenses-21`.
pub fn relabel(
    schema: &Schema,
    kinds: &BTreeMap<Name, Name>,
    edge_image: impl Fn(&SEdge) -> SEdge,
) -> Result<(Schema, Migration), ElementaryRefusal> {
    let mut preimage: BTreeMap<SEdge, SEdge> = BTreeMap::new();
    let mut edge_remap = BTreeMap::new();
    for edge in &schema.edges {
        let image = edge_image(edge);
        if let Some(first) = preimage.insert(image.clone(), edge.clone()) {
            return Err(ElementaryRefusal::IdentifiesEdges {
                first: Box::new(first),
                second: Box::new(edge.clone()),
                image: Box::new(image),
            });
        }
        if image != *edge {
            edge_remap.insert(edge.clone(), image);
        }
    }
    let image_of = |edge: &SEdge| {
        edge_remap
            .get(edge)
            .cloned()
            .unwrap_or_else(|| edge.clone())
    };
    let mut target = schema.clone();
    for kind in target.vertices.values_mut() {
        if let Some(new) = kinds.get(kind) {
            new.clone_into(kind);
        }
    }
    target.edges = schema.edges.iter().map(image_of).collect();
    for required in target.required.values_mut() {
        for edge in required {
            *edge = image_of(edge);
        }
    }
    target.orderings = schema
        .orderings
        .iter()
        .map(|(edge, position)| (image_of(edge), *position))
        .collect();
    let mut surviving_edges = target.edges.clone();
    surviving_edges.extend(edge_remap.keys().cloned());
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        surviving_edges,
        edge_remap,
        ..Migration::default()
    };
    Ok((target, migration))
}

/// Definition: A restriction keeps a subschema; the migration is its
/// inclusion.
///
/// It keeps the vertices `keep_vertex` admits (by id and kind), the edges
/// `keep_edge` admits between kept vertices, and the hyper-edges
/// `keep_hyper` admits (by kind) whose signature vertices are all kept.
///
/// Book anchors: `L-lenses-21`.
pub fn restrict(
    schema: &Schema,
    keep_vertex: impl Fn(&Name, &Name) -> bool,
    keep_edge: impl Fn(&SEdge) -> bool,
    keep_hyper: impl Fn(&Name) -> bool,
) -> (Schema, Migration) {
    let mut target = schema.clone();
    target.vertices.retain(|id, kind| keep_vertex(id, kind));
    let kept = |vertex: &Name| target.vertices.contains_key(vertex);
    let edges: BTreeSet<SEdge> = schema
        .edges
        .iter()
        .filter(|edge| keep_edge(edge) && kept(&edge.src) && kept(&edge.tgt))
        .cloned()
        .collect();
    let hyper_edges = schema
        .hyper_edges
        .iter()
        .filter(|(_, hyper)| keep_hyper(&hyper.kind) && hyper.signature.values().all(kept))
        .map(|(id, hyper)| (id.clone(), hyper.clone()))
        .collect();
    target.required = schema
        .required
        .iter()
        .filter(|(vertex, _)| kept(vertex))
        .map(|(vertex, required)| {
            (
                vertex.clone(),
                required
                    .iter()
                    .filter(|edge| edges.contains(*edge))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, required)| !required.is_empty())
        .collect();
    target.orderings = schema
        .orderings
        .iter()
        .filter(|(edge, _)| edges.contains(*edge))
        .map(|(edge, position)| (edge.clone(), *position))
        .collect();
    target.entries.retain(|entry| kept(entry));
    target.edges = edges;
    target.hyper_edges = hyper_edges;
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        surviving_edges: target.edges.clone(),
        ..Migration::default()
    };
    (target, migration)
}

/// Definition: An extension adds a vertex and an edge; every source vertex
/// and edge survives under its own name.
///
/// The added vertex replaces the kind of a vertex with its id, and the edge
/// is added only when both its endpoints are vertices of the extended
/// schema.
///
/// Book anchors: `L-lenses-21`.
#[must_use]
pub fn extend(
    schema: &Schema,
    vertex: Option<(&Name, &Name)>,
    edge: Option<&SEdge>,
) -> (Schema, Migration) {
    let mut target = schema.clone();
    if let Some((id, kind)) = vertex {
        target.vertices.insert(id.clone(), kind.clone());
    }
    if let Some(edge) = edge
        && target.vertices.contains_key(&edge.src)
        && target.vertices.contains_key(&edge.tgt)
    {
        target.edges.insert(edge.clone());
    }
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    };
    (target, migration)
}
