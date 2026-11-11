//! Structural isomorphism and shared-middle decisions for lens constructions.

use std::collections::BTreeSet;

use crate::migration::Migration;
use crate::model::SEdge;
use crate::schema::{Schema, canonical_bytes};

/// Shape whose classifier ordering is observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructionShape {
    /// Graph classifier.
    Graph,
    /// Tree classifier.
    Tree,
}

/// Declaration and transform coordinates omitted from the structural carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstructionFeature {
    /// At least one nonempty op-term assignment list.
    TermAssignments,
    /// At least one nonempty field-transform list.
    FieldTransforms,
    /// Either endpoint or the migration declares a hyper-edge operation.
    HyperEdges,
    /// The source requires a field without a default.
    SourceRequired,
    /// The source declares a key on a present vertex.
    SourceKey,
    /// The target requires a field without a default.
    TargetRequired,
    /// The target declares a key on a present vertex.
    TargetKey,
}

/// The first obstruction in the structural classifier's declared order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstructionObstruction {
    /// A mapped edge disagrees with its mapped endpoints.
    Plan,
    /// A source vertex has no surviving target image.
    MissingVertex,
    /// A target vertex has no source preimage.
    TargetOnlyVertex,
    /// A source vertex survives conditionally.
    ConditionalVertex,
    /// Distinct source vertices share an image.
    SharedVertexImage,
    /// An op-term assignment records a nontrivial transform.
    TermAssignment,
    /// A field transform prevents complement-free inversion.
    NonInvertibleTransform,
    /// A source edge has no target image.
    MissingEdge,
    /// A target edge has no source preimage.
    TargetOnlyEdge,
    /// Distinct source edges share an image.
    SharedEdgeImage,
    /// A hyper-edge lies outside the certified fragment.
    HyperEdge,
    /// A required-value obligation may refuse restoration.
    RequiredField,
    /// A key obligation may refuse restoration.
    Key,
}

fn vertex_obstruction(
    source: &Schema,
    target: &Schema,
    migration: &Migration,
) -> Option<ConstructionObstruction> {
    let mut images = BTreeSet::new();
    for vertex in source.vertices.keys() {
        let image = migration.vertex_image(vertex);
        if !migration.surviving_verts.contains(image) || !target.vertices.contains_key(image) {
            return Some(ConstructionObstruction::MissingVertex);
        }
        if migration.conditional.contains(vertex) {
            return Some(ConstructionObstruction::ConditionalVertex);
        }
        if !images.insert(image) {
            return Some(ConstructionObstruction::SharedVertexImage);
        }
    }
    target
        .vertices
        .keys()
        .any(|vertex| !images.contains(vertex.as_str()))
        .then_some(ConstructionObstruction::TargetOnlyVertex)
}

fn edge_image(source: &Schema, target: &Schema, migration: &Migration, edge: &SEdge) -> SEdge {
    if let Some(image) = migration.edge_remap.get(edge) {
        return image.clone();
    }
    let carried = SEdge::new(
        migration.vertex_image(&edge.src),
        migration.vertex_image(&edge.tgt),
        &edge.kind,
        edge.name.as_deref(),
    );
    if target.edges.contains(&carried) {
        return carried;
    }
    if source.edges_between(&edge.src, &edge.tgt).len() == 1
        && let Some(resolved) = migration
            .resolver
            .get(&(carried.src.clone(), carried.tgt.clone()))
    {
        return resolved.clone();
    }
    carried
}

fn edge_obstruction(
    source: &Schema,
    target: &Schema,
    migration: &Migration,
) -> Option<ConstructionObstruction> {
    let mut images = BTreeSet::new();
    for edge in &source.edges {
        let image = edge_image(source, target, migration, edge);
        if image.src != migration.vertex_image(&edge.src)
            || image.tgt != migration.vertex_image(&edge.tgt)
        {
            return Some(ConstructionObstruction::Plan);
        }
        if !target.edges.contains(&image) {
            return Some(ConstructionObstruction::MissingEdge);
        }
        if !images.insert(image) {
            return Some(ConstructionObstruction::SharedEdgeImage);
        }
    }
    target
        .edges
        .iter()
        .any(|edge| !images.contains(edge))
        .then_some(ConstructionObstruction::TargetOnlyEdge)
}

/// Classify the explicit structural carrier and declaration coordinates.
///
/// `features` records the presence of nonempty transform programs, hyper-edge
/// operations and required-value/key obligations; it contains observations of
/// the input declarations, never a production classifier's result. Graph and
/// tree classifiers differ only in the position and name of transform refusals.
/// The caller supplies the structural coordinates of a successfully compiled
/// stage; compilation refusals are observed before this classifier is called.
#[must_use]
pub fn construction_obstruction(
    source: &Schema,
    target: &Schema,
    migration: &Migration,
    shape: ConstructionShape,
    features: &BTreeSet<ConstructionFeature>,
) -> Option<ConstructionObstruction> {
    if let Some(error) = vertex_obstruction(source, target, migration) {
        return Some(error);
    }
    if shape == ConstructionShape::Graph {
        if features.contains(&ConstructionFeature::TermAssignments) {
            return Some(ConstructionObstruction::TermAssignment);
        }
        if features.contains(&ConstructionFeature::FieldTransforms) {
            return Some(ConstructionObstruction::NonInvertibleTransform);
        }
    }
    if let Some(error) = edge_obstruction(source, target, migration) {
        return Some(error);
    }
    for (feature, error) in [
        (
            ConstructionFeature::HyperEdges,
            ConstructionObstruction::HyperEdge,
        ),
        (
            ConstructionFeature::SourceRequired,
            ConstructionObstruction::RequiredField,
        ),
        (ConstructionFeature::SourceKey, ConstructionObstruction::Key),
        (
            ConstructionFeature::TargetRequired,
            ConstructionObstruction::RequiredField,
        ),
        (ConstructionFeature::TargetKey, ConstructionObstruction::Key),
    ] {
        if features.contains(&feature) {
            return Some(error);
        }
    }
    if features.contains(&ConstructionFeature::TermAssignments)
        || features.contains(&ConstructionFeature::FieldTransforms)
    {
        return Some(ConstructionObstruction::NonInvertibleTransform);
    }
    None
}

/// Whether two span legs name the same middle schema by canonical identity.
#[must_use]
pub fn span_has_shared_middle(left: &Schema, right: &Schema) -> bool {
    canonical_bytes(left) == canonical_bytes(right)
}
