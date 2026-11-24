//! Fused chains, scoped protolenses and sequential composites of
//! protolenses, at the instance level.
//!
//! A protolens term is an elementary kind, a fused chain of terms, or a term
//! scoped to the part of a schema reachable from a focus vertex. Its target
//! is defined by recursion on the term. A fused chain and a scoped term are
//! one lens each: the graph lens of the structural migration from the
//! source to that target, which keeps what every step keeps, follows each
//! edge through the terms' relabellings, contracts a path whose
//! interior was dropped onto the unique target edge between its ends, and
//! expands a dropped edge along the unique shortest target path through
//! added vertices. A sequential composite is a list of terms instantiated
//! one after another at the running schema; its get and put are the
//! compositions of the parts' and its complement is the list of the parts'
//! complements.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::domain::PutDomainError;
use super::elementary::{ElementaryProtolens, ElementaryRefusal, restrict};
use super::graph::LensError;
use super::protolens::{InstantiatedLens, Protolens, ProtolensError, SchemaPrecondition};
use crate::migration::Migration;
use crate::model::{Graph, Name, SEdge};
use crate::restrict::{Complement, PlanError, RestrictError};
use crate::schema::Schema;

/// A protolens term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtolensTerm {
    /// One elementary kind.
    Elementary(ElementaryProtolens),
    /// The terms fused into one lens, applied left to right.
    Fused(Vec<Self>),
    /// The inner term applied to the part of the schema reachable from the
    /// focus vertex.
    Scoped {
        /// Id of the focus vertex.
        focus: Name,
        /// The term applied within the scope.
        inner: Box<Self>,
    },
    /// A term whose get writes an added field's default on every node of a
    /// parent vertex. The schema is the term's; the value layer that writes
    /// and restores the default is supplied with the lens.
    Defaulted {
        /// The structural term.
        term: Box<Self>,
        /// The vertex whose nodes carry the field.
        parent: Name,
        /// The field written.
        field: Name,
    },
}

/// A schema at which a protolens term has no lens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermRefusal {
    /// A precondition fails: an elementary kind's at its running schema, or
    /// a scoped term's focus is not a vertex.
    Inapplicable,
    /// A relabelling would identify two edges.
    Elementary(ElementaryRefusal),
}

/// A sequential composite refuses at the zero-based index of a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeRefusal<E> {
    /// The part that refused.
    pub part: usize,
    /// Its refusal.
    pub error: E,
}

/// The fate of each source edge under a term: its image edge, or `None` when
/// the term drops it.
pub type EdgeImages = BTreeMap<SEdge, Option<SEdge>>;

impl ProtolensTerm {
    /// Definition: The target of a term at a schema, checking each
    /// precondition at the schema its term is applied to.
    ///
    /// An elementary kind checks its precondition, a fused chain checks each
    /// part at its running schema in order, and a scoped term checks that its
    /// focus is a vertex and applies its inner term within the scope by
    /// [`Self::transform`], which checks no precondition.
    ///
    /// # Errors
    ///
    /// The first refusal in that order.
    ///
    /// Book anchors: `L-lenses-22`, `L-lenses-23`.
    pub fn target(&self, schema: &Schema) -> Result<Schema, TermRefusal> {
        match self {
            Self::Elementary(kind) => {
                if !kind.precondition().holds(schema) {
                    return Err(TermRefusal::Inapplicable);
                }
                Self::transform(self, schema)
            }
            Self::Fused(parts) => parts
                .iter()
                .try_fold(schema.clone(), |running, part| part.target(&running)),
            Self::Scoped { .. } => {
                if !self.precondition().holds(schema) {
                    return Err(TermRefusal::Inapplicable);
                }
                Self::transform(self, schema)
            }
            Self::Defaulted { term, .. } => term.target(schema),
        }
    }

    /// The precondition checked at the source schema: an elementary kind's
    /// own, a fused chain's first part's, and for a scoped term that its
    /// focus is a vertex. A fused chain checks its later parts at their
    /// running schemas in [`Self::target`].
    #[must_use]
    pub fn precondition(&self) -> SchemaPrecondition {
        match self {
            Self::Elementary(kind) => kind.precondition(),
            Self::Fused(parts) => parts
                .first()
                .map_or(SchemaPrecondition::Any, Self::precondition),
            Self::Scoped { focus, .. } => {
                SchemaPrecondition::HasVertices(BTreeSet::from([focus.clone()]))
            }
            Self::Defaulted { term, .. } => term.precondition(),
        }
    }

    /// Definition: The schema transform of a term, checking no
    /// precondition: an elementary kind's target, a fused chain's parts in
    /// order, and for a scoped term the merge of the schema outside its scope
    /// with the inner transform of the scope.
    ///
    /// The scope of a focus is the set of vertices reachable from it along
    /// edges, the focus included, and the scoped subschema keeps those
    /// vertices with the edges between them. The merged target has the
    /// vertices outside the scope and the transformed scope's vertices (the
    /// latter's kind winning on a shared id), the edges with an endpoint
    /// outside the scope and the transformed scope's edges, less every edge
    /// with an endpoint that is no longer a vertex. A focus that is not a
    /// vertex has an empty scope.
    ///
    /// # Errors
    ///
    /// [`TermRefusal::Elementary`] when a relabelling would identify two
    /// edges.
    ///
    /// Book anchors: `L-lenses-22`, `L-lenses-23`.
    pub fn transform(&self, schema: &Schema) -> Result<Schema, TermRefusal> {
        match self {
            Self::Elementary(kind) => kind
                .target_and_migration(schema)
                .map(|(target, _)| target)
                .map_err(TermRefusal::Elementary),
            Self::Fused(parts) => parts
                .iter()
                .try_fold(schema.clone(), |running, part| part.transform(&running)),
            Self::Scoped { focus, inner } => {
                let scope = scope_of(schema, focus);
                let (sub, _) = scoped_subschema(schema, &scope);
                let inner_target = inner.transform(&sub)?;
                Ok(merge_scope(schema, &scope, &inner_target))
            }
            Self::Defaulted { term, .. } => term.transform(schema),
        }
    }

    /// Definition: The source vertices a term keeps.
    ///
    /// No kind renames a vertex id, so a vertex is kept or dropped. An
    /// elementary kind keeps the source vertices of its target; a fused chain
    /// keeps a vertex every part keeps at its running schema, so a vertex one
    /// part drops is dropped even when a later part adds a vertex with its
    /// id; a scoped term keeps the inner term's survivors inside its scope
    /// and every vertex of its target outside it.
    ///
    /// # Errors
    ///
    /// As [`Self::transform`].
    ///
    /// Book anchors: `L-lenses-22`, `L-lenses-23`.
    pub fn surviving_vertices(&self, schema: &Schema) -> Result<BTreeSet<Name>, TermRefusal> {
        match self {
            Self::Elementary(_) => {
                let target = self.transform(schema)?;
                Ok(schema
                    .vertices
                    .keys()
                    .filter(|vertex| target.vertices.contains_key(*vertex))
                    .cloned()
                    .collect())
            }
            Self::Fused(parts) => {
                let mut kept: BTreeSet<Name> = schema.vertices.keys().cloned().collect();
                let mut running = schema.clone();
                for part in parts {
                    let step = part.surviving_vertices(&running)?;
                    kept.retain(|vertex| step.contains(vertex));
                    running = part.transform(&running)?;
                }
                Ok(kept)
            }
            Self::Scoped { focus, inner } => {
                let scope = scope_of(schema, focus);
                let (sub, _) = scoped_subschema(schema, &scope);
                let inside = inner.surviving_vertices(&sub)?;
                let target = self.transform(schema)?;
                Ok(schema
                    .vertices
                    .keys()
                    .filter(|vertex| {
                        target.vertices.contains_key(*vertex)
                            && (!scope.contains(*vertex) || inside.contains(*vertex))
                    })
                    .cloned()
                    .collect())
            }
            Self::Defaulted { term, .. } => term.surviving_vertices(schema),
        }
    }

    /// Definition: The image of each source edge under a term, or `None`
    /// for an edge it drops.
    ///
    /// An elementary kind sends an edge to its edge-remap image when that is
    /// a target edge and drops it otherwise; a fused chain composes its
    /// parts' images at their running schemas; a scoped term takes the inner
    /// images for the edges inside its scope and keeps every other edge as
    /// itself, and drops an image that is not an edge of its target.
    ///
    /// # Errors
    ///
    /// As [`Self::transform`].
    ///
    /// Book anchors: `L-lenses-22`, `L-lenses-23`.
    pub fn edge_images(&self, schema: &Schema) -> Result<EdgeImages, TermRefusal> {
        match self {
            Self::Elementary(kind) => {
                let (target, migration) = kind
                    .target_and_migration(schema)
                    .map_err(TermRefusal::Elementary)?;
                Ok(schema
                    .edges
                    .iter()
                    .map(|edge| {
                        let image = migration
                            .edge_remap
                            .get(edge)
                            .cloned()
                            .unwrap_or_else(|| edge.clone());
                        let kept = target.edges.contains(&image).then_some(image);
                        (edge.clone(), kept)
                    })
                    .collect())
            }
            Self::Fused(parts) => {
                let mut images: EdgeImages = schema
                    .edges
                    .iter()
                    .map(|edge| (edge.clone(), Some(edge.clone())))
                    .collect();
                let mut running = schema.clone();
                for part in parts {
                    let step = part.edge_images(&running)?;
                    for image in images.values_mut() {
                        *image = image
                            .take()
                            .and_then(|edge| step.get(&edge).cloned().flatten());
                    }
                    running = part.transform(&running)?;
                }
                Ok(images)
            }
            Self::Scoped { focus, inner } => {
                let scope = scope_of(schema, focus);
                let (sub, _) = scoped_subschema(schema, &scope);
                let inner_images = inner.edge_images(&sub)?;
                let target = self.transform(schema)?;
                Ok(schema
                    .edges
                    .iter()
                    .map(|edge| {
                        let image = inner_images
                            .get(edge)
                            .cloned()
                            .unwrap_or_else(|| Some(edge.clone()))
                            .filter(|image| target.edges.contains(image));
                        (edge.clone(), image)
                    })
                    .collect())
            }
            Self::Defaulted { term, .. } => term.edge_images(schema),
        }
    }

    /// Definition: The bounded protolens of a term at a schema.
    ///
    /// An elementary kind is its own ([`ElementaryProtolens::at`]). A fused
    /// chain or a scoped term has the precondition [`Self::precondition`],
    /// the target [`Self::target`] and the [`structural_migration`] from the
    /// schema to that target along [`Self::edge_images`]. A defaulted term is
    /// its term's, with the parent recorded as transformed and the field as
    /// its one output when the parent survives.
    ///
    /// # Errors
    ///
    /// As [`Self::target`].
    ///
    /// Book anchors: `L-lenses-22`, `L-lenses-23`.
    pub fn at(&self, schema: &Schema) -> Result<Protolens, TermRefusal> {
        if let Self::Elementary(kind) = self {
            if !kind.precondition().holds(schema) {
                return Err(TermRefusal::Inapplicable);
            }
            return kind.at(schema).map_err(TermRefusal::Elementary);
        }
        if let Self::Defaulted {
            term,
            parent,
            field,
        } = self
        {
            let mut protolens = term.at(schema)?;
            if protolens.migration.surviving_verts.contains(parent) {
                protolens.migration.transformed.insert(parent.clone());
                protolens
                    .migration
                    .output_fields
                    .insert(parent.clone(), vec![field.clone()]);
            }
            return Ok(protolens);
        }
        let target = self.target(schema)?;
        let survivors = self.surviving_vertices(schema)?;
        let images = self.edge_images(schema)?;
        let migration = structural_migration(schema, &target, &survivors, &images);
        Ok(Protolens::new(self.precondition(), target, migration))
    }
}

/// Every vertex reachable from `focus` along edges, the focus included;
/// empty when the focus is not a vertex.
fn scope_of(schema: &Schema, focus: &str) -> BTreeSet<Name> {
    let mut scope = BTreeSet::new();
    if !schema.vertices.contains_key(focus) {
        return scope;
    }
    let mut queue = VecDeque::from([focus.to_owned()]);
    scope.insert(focus.to_owned());
    while let Some(vertex) = queue.pop_front() {
        for edge in schema.edges.iter().filter(|edge| edge.src == vertex) {
            if scope.insert(edge.tgt.clone()) {
                queue.push_back(edge.tgt.clone());
            }
        }
    }
    scope
}

/// The subschema on the scope: its vertices and the edges between them.
fn scoped_subschema(schema: &Schema, scope: &BTreeSet<Name>) -> (Schema, Migration) {
    restrict(schema, |id, _| scope.contains(id), |_| true, |_| true)
}

/// The schema outside the scope merged with the transformed scope, less
/// every edge that lost an endpoint.
fn merge_scope(schema: &Schema, scope: &BTreeSet<Name>, inner_target: &Schema) -> Schema {
    let mut vertices: BTreeMap<Name, Name> = schema
        .vertices
        .iter()
        .filter(|(id, _)| !scope.contains(*id))
        .map(|(id, kind)| (id.clone(), kind.clone()))
        .collect();
    vertices.extend(
        inner_target
            .vertices
            .iter()
            .map(|(id, kind)| (id.clone(), kind.clone())),
    );
    let mut edges: BTreeSet<SEdge> = schema
        .edges
        .iter()
        .filter(|edge| !(scope.contains(&edge.src) && scope.contains(&edge.tgt)))
        .cloned()
        .collect();
    edges.extend(inner_target.edges.iter().cloned());
    let mut merged = schema.clone();
    merged.vertices = vertices;
    merged.edges = edges;
    let (merged, _) = restrict(&merged, |_, _| true, |_| true, |_| true);
    merged
}

/// Definition: The structural migration from `source` to `target` keeping
/// the vertices `survivors` and sending edges along `images`.
///
/// - A vertex survives when it is in `survivors`.
/// - A source edge whose image is a target edge survives, with that image;
///   it is remapped when the image differs from it, and the surviving edges
///   are both ends of every surviving edge.
/// - The contraction resolver sends each pair of surviving vertices that no
///   source edge joins, and that exactly one target edge joins in that
///   direction, to that edge; it thus resolves only a path whose interior
///   was dropped, never a dropped direct edge.
/// - A source edge from `u` to `v` such that `u` and `v` are target vertices
///   and no target edge runs from `u` to `v` expands along the shortest
///   target path from `u` to `v` whose interior vertices are target vertices
///   absent from the source, when there is exactly one such path and its
///   interior is nonempty; the expansion records that interior.
///
/// Book anchors: `L-lenses-22`.
#[must_use]
pub fn structural_migration(
    source: &Schema,
    target: &Schema,
    survivors: &BTreeSet<Name>,
    images: &EdgeImages,
) -> Migration {
    let surviving_verts = survivors.clone();
    let edge_remap: BTreeMap<SEdge, SEdge> = images
        .iter()
        .filter_map(|(edge, image)| {
            image
                .as_ref()
                .filter(|image| *image != edge && target.edges.contains(*image))
                .map(|image| (edge.clone(), image.clone()))
        })
        .collect();
    let surviving_edges: BTreeSet<SEdge> = images
        .iter()
        .filter_map(|(edge, image)| {
            image
                .as_ref()
                .filter(|image| target.edges.contains(*image))
                .map(|image| [edge.clone(), image.clone()])
        })
        .flatten()
        .collect();
    let joined: BTreeSet<(&Name, &Name)> = source
        .edges
        .iter()
        .map(|edge| (&edge.src, &edge.tgt))
        .collect();
    let mut between: BTreeMap<(Name, Name), Vec<&SEdge>> = BTreeMap::new();
    for edge in &target.edges {
        if surviving_verts.contains(&edge.src)
            && surviving_verts.contains(&edge.tgt)
            && !joined.contains(&(&edge.src, &edge.tgt))
        {
            between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_default()
                .push(edge);
        }
    }
    let resolver = between
        .into_iter()
        .filter_map(|(pair, edges)| match edges.as_slice() {
            [edge] => Some((pair, (*edge).clone())),
            _ => None,
        })
        .collect();
    Migration {
        surviving_verts,
        surviving_edges,
        edge_remap,
        resolver,
        expansion_path: expansion_paths(source, target),
        ..Migration::default()
    }
}

/// The expansion of each source edge whose endpoints survive with no target
/// edge between them: the interior of the unique shortest target path
/// through added vertices.
fn expansion_paths(source: &Schema, target: &Schema) -> BTreeMap<(Name, Name), Vec<Name>> {
    let added: BTreeSet<&Name> = target
        .vertices
        .keys()
        .filter(|id| !source.vertices.contains_key(*id))
        .collect();
    let mut out = BTreeMap::new();
    if added.is_empty() {
        return out;
    }
    for edge in &source.edges {
        let pair = (edge.src.clone(), edge.tgt.clone());
        if out.contains_key(&pair)
            || !target.vertices.contains_key(&edge.src)
            || !target.vertices.contains_key(&edge.tgt)
            || target
                .edges
                .iter()
                .any(|found| found.src == edge.src && found.tgt == edge.tgt)
        {
            continue;
        }
        if let Some(interior) = unique_shortest_interior(target, &added, &edge.src, &edge.tgt) {
            out.insert(pair, interior);
        }
    }
    out
}

/// The interior of the unique shortest path from `start` to `end` in
/// `target` whose interior vertices lie in `through`, when it has at least
/// one interior vertex.
fn unique_shortest_interior(
    target: &Schema,
    through: &BTreeSet<&Name>,
    start: &str,
    end: &str,
) -> Option<Vec<Name>> {
    // Walks of interior vertices, grown one hop at a time; a walk visits no
    // vertex twice, so the search ends within the number of added vertices.
    let mut frontier: Vec<Vec<Name>> = vec![Vec::new()];
    for _ in 0..=through.len() {
        let mut reached: Vec<Vec<Name>> = Vec::new();
        let mut grown: Vec<Vec<Name>> = Vec::new();
        for walk in &frontier {
            let at = walk.last().map_or(start, String::as_str);
            for edge in target.edges.iter().filter(|edge| edge.src == at) {
                if edge.tgt == end && !walk.is_empty() {
                    reached.push(walk.clone());
                } else if through.contains(&edge.tgt) && !walk.contains(&edge.tgt) {
                    let mut next = walk.clone();
                    next.push(edge.tgt.clone());
                    grown.push(next);
                }
            }
        }
        reached.dedup();
        if !reached.is_empty() {
            return match reached.as_slice() {
                [interior] => Some(interior.clone()),
                _ => None,
            };
        }
        frontier = grown;
    }
    None
}

/// A sequential composite instantiated at a schema: one lens per part, each
/// at the target of the one before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositeLens {
    stages: Vec<InstantiatedLens>,
}

impl CompositeLens {
    /// Definition: Instantiate each part at the running schema, in order.
    ///
    /// # Errors
    ///
    /// The first part whose term refuses ([`ProtolensTerm::at`]) or whose
    /// plan does not compile.
    ///
    /// Book anchors: `L-lenses-24`.
    pub fn instantiate(
        parts: &[ProtolensTerm],
        source: &Schema,
    ) -> Result<Self, CompositeRefusal<CompositeInstantiationError>> {
        let mut stages = Vec::with_capacity(parts.len());
        let mut running = source.clone();
        for (part, term) in parts.iter().enumerate() {
            let refuse = |error| CompositeRefusal { part, error };
            let protolens = term
                .at(&running)
                .map_err(|error| refuse(CompositeInstantiationError::Term(error)))?;
            let lens = protolens
                .instantiate_schema(&running)
                .map_err(|error| match error {
                    ProtolensError::Plan(plan) => refuse(CompositeInstantiationError::Plan(plan)),
                    ProtolensError::FalsePrecondition { .. } | ProtolensError::Restrict(_) => {
                        refuse(CompositeInstantiationError::Term(TermRefusal::Inapplicable))
                    }
                })?;
            running = lens.target().clone();
            stages.push(lens);
        }
        Ok(Self { stages })
    }

    /// The parts' lenses in order.
    #[must_use]
    pub fn stages(&self) -> &[InstantiatedLens] {
        &self.stages
    }

    /// The target schema: the last part's target, or `None` for no parts.
    #[must_use]
    pub fn target(&self) -> Option<&Schema> {
        self.stages.last().map(InstantiatedLens::target)
    }

    /// Definition: Get through the parts left to right; the complement is
    /// the list of the parts' complements.
    ///
    /// # Errors
    ///
    /// The first part whose get refuses.
    ///
    /// Book anchors: `L-lenses-24`.
    pub fn get(
        &self,
        source: &Graph,
    ) -> Result<(Graph, Vec<Complement>), CompositeRefusal<RestrictError>> {
        let mut view = source.clone();
        let mut complements = Vec::with_capacity(self.stages.len());
        for (part, stage) in self.stages.iter().enumerate() {
            let refuse = |error| CompositeRefusal { part, error };
            let lens = stage
                .as_graph_lens()
                .map_err(|plan| refuse(RestrictError::Plan(plan)))?;
            let (next, complement) = lens.get(&view).map_err(refuse)?;
            complements.push(complement);
            view = next;
        }
        Ok((view, complements))
    }

    /// Definition: Put through the parts right to left, each with its own
    /// complement.
    ///
    /// # Errors
    ///
    /// The rightmost part whose put refuses, all parts to its right having
    /// put; a complement list of the wrong length is refused at the part
    /// count with no put run.
    ///
    /// Book anchors: `L-lenses-24`.
    pub fn put(
        &self,
        view: &Graph,
        complements: &[Complement],
    ) -> Result<Graph, CompositeRefusal<CompositePutError>> {
        if complements.len() != self.stages.len() {
            return Err(CompositeRefusal {
                part: self.stages.len(),
                error: CompositePutError::ComplementCount(complements.len()),
            });
        }
        let mut current = view.clone();
        for (part, (stage, complement)) in self.stages.iter().zip(complements).enumerate().rev() {
            let refuse = |error| CompositeRefusal { part, error };
            let lens = stage
                .as_graph_lens()
                .map_err(|plan| refuse(CompositePutError::Plan(plan)))?;
            current = lens
                .put(&current, complement)
                .map_err(|error| refuse(CompositePutError::Lens(error)))?;
        }
        Ok(current)
    }

    /// Definition: The put domain of the composite: the rightmost part whose
    /// put-domain report is nonempty at the view its right neighbours put,
    /// with that report.
    ///
    /// # Errors
    ///
    /// That part and its report.
    ///
    /// Book anchors: `L-lenses-24`.
    pub fn check_put_domain(
        &self,
        view: &Graph,
        complements: &[Complement],
    ) -> Result<(), CompositeRefusal<Vec<PutDomainError>>> {
        let mut current = view.clone();
        for (part, (stage, complement)) in self.stages.iter().zip(complements).enumerate().rev() {
            let Ok(lens) = stage.as_graph_lens() else {
                return Ok(());
            };
            lens.check_put_domain(&current, complement)
                .map_err(|error| CompositeRefusal { part, error })?;
            match lens.put(&current, complement) {
                Ok(next) => current = next,
                Err(_) => return Ok(()),
            }
        }
        Ok(())
    }
}

/// Why a part of a sequential composite has no lens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositeInstantiationError {
    /// The part's term refuses at its running schema.
    Term(TermRefusal),
    /// The part's plan does not compile.
    Plan(PlanError),
}

/// Why a part of a sequential composite refuses put.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositePutError {
    /// The complement list does not have one entry per part; carries the
    /// length found.
    ComplementCount(usize),
    /// The part's plan does not bind.
    Plan(PlanError),
    /// The part's graph put refuses.
    Lens(LensError),
}
