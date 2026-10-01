//! The migration adjunctions on instances: `Sigma_F -| Delta_F -| Pi_F`.
//!
//! For table instances ([`FInstance`]) the functor is a [`MigrationFunctor`]
//! `F: S' -> T` between relation-ified schemas ([`crate::RelSchema`]), on
//! which a table instance is a presheaf: rows at tables, pairs at foreign
//! keys read as relations. Three transports follow (formally the Kan
//! extensions between presheaf categories, which always exist and are
//! adjoint; stated):
//!
//! - **`Delta_F`** (precomposition): `(Delta_F Y)(c) = Y(F c)`.
//! - **`Sigma_F`** (left Kan extension): the colimit over the comma
//!   category `F/d` at each target object `d`.
//! - **`Pi_F`** (right Kan extension, [`crate::functor_pi_along`]): the
//!   limit over `d/F`.
//!
//! so that
//!
//! ```text
//! Hom_T(Sigma_F X, Y)  ~=  Hom_S'(X, Delta_F Y)
//! Hom_S'(Delta_F Y, X)  ~=  Hom_T(Y, Pi_F X).
//! ```
//!
//! This module builds the units and counits of both adjunctions as instance
//! homomorphisms ([`crate::instance_hom`]), the transposes that witness the
//! hom-set bijections, and the action of each transport on homomorphisms,
//! and its tests check the bijections (counted and witnessed), the triangle
//! identities, the identity functor, renaming and monotonicity. Cells are
//! labels every homomorphism preserves exactly (attribute vertices are not
//! objects), so the target instances of `Delta_F -| Pi_F` are those whose
//! rows at tables outside the image of `F` carry no cells: the limit of the
//! empty diagram there is one row, which carries none.
//!
//! W-type instances ([`WInstance`]) are trees, and precomposition along a
//! merging map would duplicate a node into several anchors at once, which
//! is no longer a tree. The W-type adjunction is thus scoped to
//! vertex-injective total maps (renamings and the identity), where
//! `Sigma_F` and `Delta_F` are mutually inverse relabellings that preserve
//! node identity. Merging maps are rejected by [`w_delta`] with
//! [`AdjunctionError::NonInjectiveVertexMap`].

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::{InstError, RestrictError};
use crate::functor::{FInstance, allocate_row, row_allocator};
use crate::ginstance::{GEdge, GInstance};
use crate::instance_hom::{FInstanceHom, WInstanceHom};
use crate::migration_functor::MigrationFunctor;
use crate::pi::pi_limit;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, WInstance, wtype_extend};

/// Error raised when an adjunction transport is undefined for the given
/// migration or instance.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AdjunctionError {
    /// The vertex map merges two source vertices, so the W-type `Delta_F`
    /// (which would duplicate a node into several anchors and leave the
    /// tree category) is undefined. Use the functor-side adjunction for
    /// merging maps.
    #[error(
        "vertex map is not injective: source vertices `{first}` and `{second}` \
         both map to `{target}`, so the W-type pullback is undefined"
    )]
    NonInjectiveVertexMap {
        /// First source vertex sharing the image.
        first: Name,
        /// Second source vertex sharing the image.
        second: Name,
        /// The shared target vertex.
        target: Name,
    },

    /// The edge map merges two source edges, so the W-type pullback cannot
    /// invert the edge relabelling unambiguously.
    #[error(
        "edge map is not injective: two source edges both map to \
         `{src} -> {tgt}`, so the W-type pullback is undefined"
    )]
    NonInjectiveEdgeMap {
        /// Source anchor of the shared target edge.
        src: Name,
        /// Target anchor of the shared target edge.
        tgt: Name,
    },

    /// A node's anchor lies outside the image of `F`, so the pullback has
    /// no source vertex to reindex it to.
    #[error(
        "anchor `{0}` lies outside the image of the migration; the pullback is undefined there"
    )]
    AnchorOutsideImage(Name),

    /// The instance holds rows at a table outside the domain of the
    /// homomorphism asked for (a source table outside `S'`, or a vertex that
    /// is no table of the target), so the homomorphism would not be total.
    #[error("table `{0}` lies outside the schema the homomorphism is defined on")]
    VertexOutsideDomain(Name),

    /// The instance holds pairs on a foreign key outside the domain of the
    /// homomorphism asked for, so the homomorphism would not preserve them.
    #[error("foreign key {0:?} lies outside the schema the homomorphism is defined on")]
    EdgeOutsideDomain(Edge),

    /// The underlying left Kan extension ([`wtype_extend`]) failed.
    #[error("Sigma (left Kan extension) failed: {0}")]
    Sigma(#[from] RestrictError),

    /// The right Kan extension ([`crate::functor_pi_along`]) failed.
    #[error("Pi (right Kan extension) failed: {0}")]
    Pi(RestrictError),

    /// A transport that copies rows found no row id free above the largest
    /// in use.
    #[error("no row id is free above the largest in use")]
    RowIdsExhausted,

    /// Building the reindexed graph failed.
    #[error("the reindexed graph is malformed: {0}")]
    Graph(InstError),

    /// A row of a target instance at a table outside the image of the
    /// functor carries cells, so no homomorphism sends it to the one row,
    /// without cells, of the limit there.
    #[error("row {row:?} of table `{table}`, outside the image of the functor, carries cells")]
    CellsOutsideImage {
        /// The table outside the image.
        table: Name,
        /// The row carrying cells.
        row: RowId,
    },

    /// A map given as a homomorphism sends a row to no row of the transport
    /// it must land in, so it is no homomorphism between the instances
    /// given.
    #[error("row {row:?} has no image in the transport the map must land in")]
    NotANaturalMap {
        /// The row without an image.
        row: RowId,
    },
}

// ---------------------------------------------------------------------------
// Shared vertex / edge map helpers
// ---------------------------------------------------------------------------

/// The image of a vertex under the migration's vertex map, defaulting to
/// the identity for vertices the map leaves untouched.
pub(crate) fn map_vertex(migration: &CompiledMigration, vertex: &Name) -> Name {
    migration
        .vertex_remap
        .get(vertex)
        .cloned()
        .unwrap_or_else(|| vertex.clone())
}

/// The image of an edge under the migration's edge map, defaulting to the
/// edge relabelled by the vertex map when no explicit remap is recorded.
pub(crate) fn map_edge(migration: &CompiledMigration, edge: &Edge) -> Edge {
    migration
        .edge_remap
        .get(edge)
        .cloned()
        .unwrap_or_else(|| Edge {
            src: map_vertex(migration, &edge.src),
            tgt: map_vertex(migration, &edge.tgt),
            kind: edge.kind.clone(),
            name: edge.name.clone(),
        })
}

/// The source vertices `F` is defined on.
///
/// A migration states a vertex's image either by naming it — an entry in
/// `vertex_remap` — or by letting the vertex survive under its own name, which
/// it says by listing the vertex among the surviving vertices and naming no
/// image for it. Both are in the domain, and `map_vertex` sends each to its
/// image. A surviving vertex that is some other vertex's image is a target of
/// `F`, not a source, and is in the domain only if it is also a key.
///
/// Every table operation reads a compiled migration through this one
/// function: `Sigma_F`, `Delta_F`, the fibres of `Pi_F`
/// ([`crate::functor_pi`]), [`crate::functor_restrict`] and
/// [`crate::functor_extend`], so they agree on which vertices they travel
/// between.
pub(crate) fn source_vertices(migration: &CompiledMigration) -> BTreeSet<Name> {
    let images: HashSet<&Name> = migration.vertex_remap.values().collect();
    let mut domain: BTreeSet<Name> = migration.vertex_remap.keys().cloned().collect();
    for vertex in &migration.surviving_verts {
        if !migration.vertex_remap.contains_key(vertex) && !images.contains(vertex) {
            domain.insert(vertex.clone());
        }
    }
    domain
}

/// The source edges `F` is defined on, read the same way as
/// [`source_vertices`] reads vertices: the edges `edge_remap` names an image
/// for, together with the surviving edges it neither remaps nor names as some
/// other edge's image. An edge the migration does not mention is not in the
/// domain.
///
/// Every table operation reads the edges a compiled migration carries through
/// this one function, by way of [`edge_images`].
pub(crate) fn source_edges(migration: &CompiledMigration) -> BTreeSet<Edge> {
    let images: HashSet<&Edge> = migration.edge_remap.values().collect();
    let mut domain: BTreeSet<Edge> = migration.edge_remap.keys().cloned().collect();
    for edge in &migration.surviving_edges {
        if !migration.edge_remap.contains_key(edge) && !images.contains(edge) {
            domain.insert(edge.clone());
        }
    }
    domain
}

/// Each source edge ([`source_edges`]) with its image under `F`: the edge
/// `edge_remap` names, or, for an edge that survives unremapped, the edge of
/// the same kind and name between the images of its endpoints.
///
/// `Sigma_F`, `Delta_F`, [`crate::functor_pi`], [`crate::functor_restrict`]
/// and [`crate::functor_extend`] carry a foreign key exactly when its edge is
/// a key of this map, and carry it to the edge this map names.
pub(crate) fn edge_images(migration: &CompiledMigration) -> HashMap<Edge, Edge> {
    source_edges(migration)
        .into_iter()
        .map(|source| {
            let target = map_edge(migration, &source);
            (source, target)
        })
        .collect()
}

/// The first source edge ([`source_edges`], ascending) whose image under
/// `F` does not join the images of its endpoints, with that image.
///
/// Only an edge `edge_remap` remaps can be such an edge: a surviving edge is
/// relabelled onto the images of its endpoints. A migration with such an
/// edge states no functor between the two schemas (`panproto_mig::compile`
/// refuses one as no morphism), so every table operation refuses it rather
/// than drop the edge's pairs or return a row map that is no homomorphism.
pub(crate) fn edge_off_endpoints(migration: &CompiledMigration) -> Option<(Edge, Edge)> {
    source_edges(migration).into_iter().find_map(|edge| {
        let image = map_edge(migration, &edge);
        let joins = image.src == map_vertex(migration, &edge.src)
            && image.tgt == map_vertex(migration, &edge.tgt);
        (!joins).then_some((edge, image))
    })
}

/// [`edge_off_endpoints`] as the refusal of an operation returning
/// [`RestrictError`]: [`RestrictError::NoEdgeFound`] between the images of
/// the edge's endpoints, since the migration names no edge between them
/// for it.
///
/// # Errors
///
/// As described.
pub(crate) fn check_edge_endpoints(migration: &CompiledMigration) -> Result<(), RestrictError> {
    edge_off_endpoints(migration).map_or(Ok(()), |(edge, _)| {
        Err(RestrictError::NoEdgeFound {
            src: map_vertex(migration, &edge.src).to_string(),
            tgt: map_vertex(migration, &edge.tgt).to_string(),
        })
    })
}

/// The edges a migration names only as targets: images of `edge_remap` that
/// are not themselves source edges.
pub(crate) fn target_only_edges(migration: &CompiledMigration) -> HashSet<&Edge> {
    migration
        .edge_remap
        .values()
        .filter(|edge| !migration.edge_remap.contains_key(*edge))
        .collect()
}

/// Invert the vertex map over its whole domain, failing when two source
/// vertices share an image.
fn invert_vertex_map(
    migration: &CompiledMigration,
) -> Result<HashMap<Name, Name>, AdjunctionError> {
    let domain = source_vertices(migration);
    let mut inverse: HashMap<Name, Name> = HashMap::with_capacity(domain.len());
    for src in domain {
        let tgt = map_vertex(migration, &src);
        if let Some(existing) = inverse.insert(tgt.clone(), src.clone()) {
            if existing != src {
                return Err(AdjunctionError::NonInjectiveVertexMap {
                    first: existing,
                    second: src,
                    target: tgt,
                });
            }
        }
    }
    Ok(inverse)
}

/// Invert the edge map over its whole domain, failing when two source edges
/// share an image.
fn invert_edge_map(migration: &CompiledMigration) -> Result<HashMap<Edge, Edge>, AdjunctionError> {
    let domain = source_edges(migration);
    let mut inverse: HashMap<Edge, Edge> = HashMap::with_capacity(domain.len());
    for src in domain {
        let tgt = map_edge(migration, &src);
        if let Some(existing) = inverse.insert(tgt.clone(), src.clone()) {
            if existing != src {
                return Err(AdjunctionError::NonInjectiveEdgeMap {
                    src: tgt.src,
                    tgt: tgt.tgt,
                });
            }
        }
    }
    Ok(inverse)
}

// ---------------------------------------------------------------------------
// W-type adjunction (injective total maps)
// ---------------------------------------------------------------------------

/// `Sigma_F` for W-type instances: the left Kan extension that relabels
/// each node's anchor `v` to `F(v)` and each arc's edge accordingly,
/// preserving node identity.
///
/// This delegates to [`wtype_extend`]; `tgt_schema` is consulted only when
/// an arc's edge is neither remapped nor surviving (never the case for the
/// full relabelling migrations this adjunction targets).
///
/// # Errors
///
/// Returns [`AdjunctionError::Sigma`] if the underlying extension fails,
/// for instance when the root anchor has no image in the target schema.
pub fn w_sigma(
    x: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<WInstance, AdjunctionError> {
    Ok(wtype_extend(x, tgt_schema, migration)?)
}

/// `Delta_F` for W-type instances, defined for vertex-injective total maps.
///
/// Reindexes a `T`-instance to an `S`-instance by relabelling each node's
/// anchor `t` back to the unique `s` with `F(s) = t` and each arc's edge to
/// its unique preimage, preserving node identity, arcs, fans, and the root.
///
/// # Errors
///
/// Returns [`AdjunctionError::NonInjectiveVertexMap`] or
/// [`AdjunctionError::NonInjectiveEdgeMap`] when the map merges vertices or
/// edges (the pullback then leaves the tree category), or
/// [`AdjunctionError::AnchorOutsideImage`] when a node's anchor is not in
/// the image of `F`.
pub fn w_delta(y: &WInstance, migration: &CompiledMigration) -> Result<WInstance, AdjunctionError> {
    let inverse_vertex = invert_vertex_map(migration)?;
    let inverse_edge = invert_edge_map(migration)?;

    let mut nodes = HashMap::with_capacity(y.nodes.len());
    for (&id, node) in &y.nodes {
        let source_anchor = inverse_vertex
            .get(&node.anchor)
            .ok_or_else(|| AdjunctionError::AnchorOutsideImage(node.anchor.clone()))?;
        let mut reindexed = node.clone();
        reindexed.anchor = source_anchor.clone();
        nodes.insert(id, reindexed);
    }

    let mut arcs = Vec::with_capacity(y.arcs.len());
    for (parent, child, edge) in &y.arcs {
        let source_edge = inverse_edge.get(edge).cloned().unwrap_or_else(|| Edge {
            src: inverse_vertex
                .get(&edge.src)
                .cloned()
                .unwrap_or_else(|| edge.src.clone()),
            tgt: inverse_vertex
                .get(&edge.tgt)
                .cloned()
                .unwrap_or_else(|| edge.tgt.clone()),
            kind: edge.kind.clone(),
            name: edge.name.clone(),
        });
        arcs.push((*parent, *child, source_edge));
    }

    let schema_root = inverse_vertex
        .get(&y.schema_root)
        .ok_or_else(|| AdjunctionError::AnchorOutsideImage(y.schema_root.clone()))?
        .clone();

    Ok(WInstance::new(
        nodes,
        arcs,
        y.fans.clone(),
        y.root,
        schema_root,
    ))
}

/// The unit `eta_X : X -> Delta_F(Sigma_F X)` of the W-type adjunction.
///
/// For an injective total map, `Delta_F(Sigma_F X)` is `X` relabelled
/// forward then back, hence anchor-for-anchor equal to `X`; the canonical
/// comparison is the identity on node identifiers.
#[must_use]
pub fn w_unit(x: &WInstance) -> WInstanceHom {
    WInstanceHom::identity(x)
}

/// The counit `eps_Y : Sigma_F(Delta_F Y) -> Y` of the W-type adjunction.
///
/// For an injective total map with `Y` anchored inside the image of `F`,
/// `Sigma_F(Delta_F Y)` is `Y` relabelled back then forward, hence equal to
/// `Y`; the canonical comparison is the identity on node identifiers.
#[must_use]
pub fn w_counit(y: &WInstance) -> WInstanceHom {
    WInstanceHom::identity(y)
}

/// Transpose `Hom_T(Sigma_F X, Y) -> Hom_S(X, Delta_F Y)`.
///
/// Because `Sigma_F` and `Delta_F` preserve node identity for injective
/// maps, the same node map is a homomorphism on both sides; the transpose
/// carries it across unchanged.
#[must_use]
pub fn w_transpose_left(g: &WInstanceHom) -> WInstanceHom {
    g.clone()
}

/// Transpose `Hom_S(X, Delta_F Y) -> Hom_T(Sigma_F X, Y)`, the inverse of
/// [`w_transpose_left`].
#[must_use]
pub fn w_transpose_right(f: &WInstanceHom) -> WInstanceHom {
    f.clone()
}

// ---------------------------------------------------------------------------
// Functor (set-valued) Kan extensions along a migration functor
// ---------------------------------------------------------------------------

/// Check that every table of `x` is a table of `S'` and every foreign key
/// with pairs a relation of `S'`, so that a homomorphism out of `x` along
/// `F` is total.
fn check_source_instance(x: &FInstance, functor: &MigrationFunctor) -> Result<(), AdjunctionError> {
    if let Some(table) = x
        .table_names()
        .into_iter()
        .find(|table| functor.table_image(table).is_none())
    {
        return Err(AdjunctionError::VertexOutsideDomain(table.clone()));
    }
    x.foreign_keys()
        .into_iter()
        .find(|(edge, _)| functor.relation_image(edge).is_none())
        .map_or(Ok(()), |(edge, _)| {
            Err(AdjunctionError::EdgeOutsideDomain(edge.clone()))
        })
}

/// Check that every table of `y` is a table of `T` and every foreign key with
/// pairs a relation of `T`, and that no row of a table outside the image of
/// `F` carries cells, so that a homomorphism out of `y` into `Pi_F` is total.
fn check_target_instance(y: &FInstance, functor: &MigrationFunctor) -> Result<(), AdjunctionError> {
    if let Some(table) = y
        .table_names()
        .into_iter()
        .find(|table| !functor.target().is_table(table))
    {
        return Err(AdjunctionError::VertexOutsideDomain(table.clone()));
    }
    if let Some((edge, _)) = y
        .foreign_keys()
        .into_iter()
        .find(|(edge, _)| !functor.target().is_relation(edge))
    {
        return Err(AdjunctionError::EdgeOutsideDomain(edge.clone()));
    }
    for table in functor.tables_outside_image() {
        if let Some((row, _)) = y
            .rows(table)
            .into_iter()
            .find(|(_, cells)| !cells.is_empty())
        {
            return Err(AdjunctionError::CellsOutsideImage {
                table: table.clone(),
                row,
            });
        }
    }
    Ok(())
}

/// `Sigma_F` for set-valued functor instances: the left Kan extension.
///
/// The first projection of [`f_sigma_with_hom`]; see there.
///
/// # Errors
///
/// As [`f_sigma_with_hom`].
pub fn f_sigma(x: &FInstance, functor: &MigrationFunctor) -> Result<FInstance, AdjunctionError> {
    f_sigma_with_hom(x, functor).map(|(sigma, _)| sigma)
}

/// `Sigma_F` for set-valued functor instances, with the map from the part of
/// `x` it reads to the result.
///
/// The instance is read as a presheaf `I` on `S'`, the part of the source the
/// functor keeps (rows and pairs outside `S'` are not read). `Sigma_F(I)` at
/// a target object `d` is the colimit of `I` over the comma category `F/d`.
/// At a table `d` its objects are the fibre tables and the relations sent to
/// a relation at `d`, and each pair is glued to its row at `d`, so the
/// colimit is the disjoint union of the fibre's rows: every row keeps its id
/// and cells (ids are unique across an instance). At a relation `E` the
/// objects are the relations sent to `E`, with no arrows between them, so
/// the colimit is the union of their pairs, read as a set of pairs (the
/// reflection onto relational instances, which keeps the adjunction, since
/// every target instance is relational). A target object outside the image
/// gets the colimit over the empty diagram: no rows, no pairs.
///
/// The returned [`FInstanceHom`] sends each row of the restriction of `x` to
/// `S'` to itself, along `F`.
///
/// # Errors
///
/// None at present; the `Result` keeps the signature of the other
/// transports.
pub fn f_sigma_with_hom(
    x: &FInstance,
    functor: &MigrationFunctor,
) -> Result<(FInstance, FInstanceHom), AdjunctionError> {
    let mut sigma = FInstance::new();
    let mut row_map = BTreeMap::new();
    for (source, image) in functor.table_map() {
        for (id, row) in x.rows(source) {
            sigma.place_row(image.clone(), id, row.clone());
            row_map.insert(id, id);
        }
    }
    for (source, image) in functor.relation_map() {
        for &(s, t) in x.pairs(source) {
            sigma.place_pair(image.clone(), s, t);
        }
    }
    let hom = FInstanceHom::new(row_map)
        .with_table_map(functor.table_map().clone())
        .with_edge_map(functor.relation_map().clone());
    Ok((sigma, hom))
}

/// `Sigma_F(h)`: the image under `Sigma_F` of a homomorphism `h: X -> X'`
/// of instances on `S'`.
///
/// `Sigma_F` keeps row ids, so `Sigma_F(h)` sends each row of `Sigma_F X` to
/// the image of that row under `h`.
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] or
/// [`AdjunctionError::EdgeOutsideDomain`] when `x` is not an instance on
/// `S'`.
pub fn f_sigma_on_hom(
    h: &FInstanceHom,
    x: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    check_source_instance(x, functor)?;
    let row_map = x
        .row_ids()
        .into_iter()
        .filter_map(|id| h.image(id).map(|image| (id, image)))
        .collect();
    Ok(FInstanceHom::new(row_map))
}

/// `Delta_F` for set-valued functor instances: precomposition.
///
/// The first projection of [`f_delta_with_hom`]; see there.
///
/// # Errors
///
/// As [`f_delta_with_hom`].
pub fn f_delta(y: &FInstance, functor: &MigrationFunctor) -> Result<FInstance, AdjunctionError> {
    f_delta_with_hom(y, functor).map(|(delta, _)| delta)
}

/// `Delta_F` for set-valued functor instances, with the row map from each
/// copy to the row of `y` it copies.
///
/// `(Delta_F Y)(c) = Y(F c)` for every object `c` of `S'`: each table `s` of
/// `S'` receives a copy of the target table `F(s)`, and each relation `e` of
/// `S'` a copy of the pairs of `F(e)` between the copies of their rows, with
/// the cells of the rows they copy. Tables and relations of the source
/// outside `S'` receive nothing.
///
/// Row ids: the copies are made in ascending source table name and, within
/// one table, in ascending target row id. The first copy of a target row
/// keeps its id, so over an injective fibre every row keeps its id; every
/// later copy takes a fresh id, allocated from the least id above every row
/// of `y`. The returned [`FInstanceHom`] sends each row of the result to the
/// row of `y` it copies, along `F`: its table map sends each table `s` to
/// `F(s)` and its edge map each relation `e` to `F(e)`.
///
/// # Errors
///
/// Returns [`AdjunctionError::RowIdsExhausted`] when the fresh row ids run
/// out.
pub fn f_delta_with_hom(
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<(FInstance, FInstanceHom), AdjunctionError> {
    let mut allocator = row_allocator(y.row_ids());
    let mut delta = FInstance::new();
    let mut copy_of: HashMap<(&Name, RowId), RowId> = HashMap::new();
    let mut to_original: BTreeMap<RowId, RowId> = BTreeMap::new();
    let mut copied: HashSet<RowId> = HashSet::new();
    for (source, target) in functor.table_map() {
        for (original, row) in y.rows(target) {
            let id = if copied.insert(original) {
                original
            } else {
                allocate_row(&mut allocator).map_err(|_| AdjunctionError::RowIdsExhausted)?
            };
            delta.place_row(source.clone(), id, row.clone());
            copy_of.insert((source, original), id);
            to_original.insert(id, original);
        }
    }
    for (source_edge, target_edge) in functor.relation_map() {
        for &(s, t) in y.pairs(target_edge) {
            if let (Some(&cs), Some(&ct)) = (
                copy_of.get(&(&source_edge.src, s)),
                copy_of.get(&(&source_edge.tgt, t)),
            ) {
                delta.place_pair(source_edge.clone(), cs, ct);
            }
        }
    }
    let hom = FInstanceHom::new(to_original)
        .with_table_map(functor.table_map().clone())
        .with_edge_map(functor.relation_map().clone());
    Ok((delta, hom))
}

/// Where each row of `Y` is copied in `Delta_F Y`: `(source table, row of
/// Y)` to the copy's id, read off the row map of [`f_delta_with_hom`].
fn copies(delta: &FInstance, to_original: &FInstanceHom) -> HashMap<(Name, RowId), RowId> {
    to_original
        .row_map
        .iter()
        .filter_map(|(&copy, &original)| {
            delta
                .table_of(copy)
                .map(|source| ((source.clone(), original), copy))
        })
        .collect()
}

/// `Delta_F(h)`: the image under `Delta_F` of a homomorphism `h: Y -> Y'` of
/// target instances, sending the copy at `s` of a row `r` to the copy at `s`
/// of `h(r)`.
///
/// # Errors
///
/// Returns [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`]
/// does.
pub fn f_delta_on_hom(
    h: &FInstanceHom,
    y: &FInstance,
    y_prime: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (delta, to_original) = f_delta_with_hom(y, functor)?;
    let (delta_prime, to_original_prime) = f_delta_with_hom(y_prime, functor)?;
    let copies_prime = copies(&delta_prime, &to_original_prime);
    let row_map = to_original
        .row_map
        .iter()
        .filter_map(|(&copy, &original)| {
            let source = delta.table_of(copy)?;
            let image = h.image(original)?;
            copies_prime
                .get(&(source.clone(), image))
                .map(|&target| (copy, target))
        })
        .collect();
    Ok(FInstanceHom::new(row_map))
}

/// The unit `eta_X : X -> Delta_F(Sigma_F X)` of `Sigma_F -| Delta_F`.
///
/// `Sigma_F` keeps every row's id, so row `r` of `X` at `s` maps to the copy
/// of `r` that `Delta_F` places at `s`.
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] or
/// [`AdjunctionError::EdgeOutsideDomain`] when `x` is not an instance on
/// `S'` (a row or pair outside it would have no image), and
/// [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`] does.
pub fn f_unit(x: &FInstance, functor: &MigrationFunctor) -> Result<FInstanceHom, AdjunctionError> {
    check_source_instance(x, functor)?;
    let sigma = f_sigma(x, functor)?;
    let (delta, to_original) = f_delta_with_hom(&sigma, functor)?;
    let copies = copies(&delta, &to_original);
    let mut row_map = BTreeMap::new();
    for (source, table) in x.tables() {
        for id in table.ids() {
            if let Some(&copy) = copies.get(&(source.clone(), id)) {
                row_map.insert(id, copy);
            }
        }
    }
    Ok(FInstanceHom::new(row_map))
}

/// The counit `eps_Y : Sigma_F(Delta_F Y) -> Y` of `Sigma_F -| Delta_F`.
///
/// `Sigma_F(Delta_F Y)` holds the copies of `Delta_F Y` under their own ids;
/// the counit sends each copy back to the row it copies.
///
/// # Errors
///
/// Returns [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`]
/// does.
pub fn f_counit(
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (_, to_original) = f_delta_with_hom(y, functor)?;
    Ok(FInstanceHom::new(to_original.row_map))
}

/// Transpose `Hom_T(Sigma_F X, Y) -> Hom_S'(X, Delta_F Y)`, sending `g` to
/// `Delta_F(g) . eta_X`: row `r` of `X` at `s` maps to the copy at `s` of
/// `g(r)`.
///
/// # Errors
///
/// As [`f_unit`].
pub fn f_transpose_left(
    g: &FInstanceHom,
    x: &FInstance,
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    check_source_instance(x, functor)?;
    let (delta, to_original) = f_delta_with_hom(y, functor)?;
    let copies = copies(&delta, &to_original);
    let mut row_map = BTreeMap::new();
    for (source, table) in x.tables() {
        for id in table.ids() {
            if let Some(&copy) = g
                .image(id)
                .and_then(|image| copies.get(&(source.clone(), image)))
            {
                row_map.insert(id, copy);
            }
        }
    }
    Ok(FInstanceHom::new(row_map))
}

/// Transpose `Hom_S'(X, Delta_F Y) -> Hom_T(Sigma_F X, Y)`, sending `f` to
/// `eps_Y . Sigma_F(f)`, the inverse of [`f_transpose_left`]: row `r` of
/// `Sigma_F X` maps to the row of `Y` that `f(r)` copies.
///
/// # Errors
///
/// As [`f_unit`].
pub fn f_transpose_right(
    f: &FInstanceHom,
    x: &FInstance,
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    check_source_instance(x, functor)?;
    let (_, to_original) = f_delta_with_hom(y, functor)?;
    let row_map = x
        .row_ids()
        .into_iter()
        .filter_map(|id| {
            f.image(id)
                .and_then(|copy| to_original.image(copy))
                .map(|original| (id, original))
        })
        .collect();
    Ok(FInstanceHom::new(row_map))
}

/// `Pi_F(h)`: the image under `Pi_F` of a homomorphism `h: X -> X'` of
/// instances on `S'`.
///
/// It sends the row of each tuple to the row of the tuple of its images (the
/// one row of a table outside the image to the one row).
///
/// # Errors
///
/// Returns [`AdjunctionError::Pi`] when either limit fails
/// ([`crate::pi::functor_pi_along_with_hom`]) and
/// [`AdjunctionError::NotANaturalMap`] when `h` sends a tuple to one that is
/// not a row of `Pi_F X'` (it is then no homomorphism).
pub fn f_pi_on_hom(
    h: &FInstanceHom,
    x: &FInstance,
    x_prime: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    let pi = pi_limit(x, functor, max_product_size).map_err(AdjunctionError::Pi)?;
    let pi_prime = pi_limit(x_prime, functor, max_product_size).map_err(AdjunctionError::Pi)?;
    let mut row_map = BTreeMap::new();
    for (&row, (table, tuple)) in &pi.tuples {
        let image: Option<Vec<RowId>> = tuple.iter().map(|&r| h.image(r)).collect();
        let target = image
            .and_then(|image| pi_prime.rows.get(&(table.clone(), image)))
            .ok_or(AdjunctionError::NotANaturalMap { row })?;
        row_map.insert(row, *target);
    }
    Ok(FInstanceHom::new(row_map))
}

/// The unit `eta_Y : Y -> Pi_F(Delta_F Y)` of `Delta_F -| Pi_F`.
///
/// Row `r` of `Y` at a table `d` maps to the row of `Pi_F(Delta_F Y)` whose
/// tuple holds the copy of `r` at every table of `d`'s fibre (the one row,
/// when `d` is outside the image).
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] or
/// [`AdjunctionError::EdgeOutsideDomain`] when `y` holds rows or pairs
/// outside `T`, [`AdjunctionError::CellsOutsideImage`] when a row at a table
/// outside the image of `F` carries cells (the one row of the limit there
/// carries none), and the errors of [`f_delta_with_hom`] and
/// [`f_pi_on_hom`].
pub fn f_pi_unit(
    y: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    check_target_instance(y, functor)?;
    let (delta, to_original) = f_delta_with_hom(y, functor)?;
    let copies = copies(&delta, &to_original);
    let pi = pi_limit(&delta, functor, max_product_size).map_err(AdjunctionError::Pi)?;
    let mut row_map = BTreeMap::new();
    for (table, rows) in y.tables() {
        let fibre = functor.table_fibre(table);
        for id in rows.ids() {
            let tuple: Option<Vec<RowId>> = fibre
                .iter()
                .map(|&source| copies.get(&(source.clone(), id)).copied())
                .collect();
            let image = tuple
                .and_then(|tuple| pi.rows.get(&(table.clone(), tuple)))
                .ok_or(AdjunctionError::NotANaturalMap { row: id })?;
            row_map.insert(id, *image);
        }
    }
    Ok(FInstanceHom::new(row_map))
}

/// The counit `eps_X : Delta_F(Pi_F X) -> X` of `Delta_F -| Pi_F`: the copy
/// at `s` of a row of `Pi_F X` maps to that row's component at `s`.
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] or
/// [`AdjunctionError::EdgeOutsideDomain`] when `x` is not an instance on
/// `S'`, and the errors of [`f_delta_with_hom`] and [`f_pi_on_hom`].
pub fn f_pi_counit(
    x: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    check_source_instance(x, functor)?;
    let pi = pi_limit(x, functor, max_product_size).map_err(AdjunctionError::Pi)?;
    let (delta, to_original) = f_delta_with_hom(&pi.instance, functor)?;
    let mut row_map = BTreeMap::new();
    for (&copy, original) in &to_original.row_map {
        let component = delta.table_of(copy).and_then(|source| {
            let (table, tuple) = pi.tuples.get(original)?;
            let position = functor
                .table_fibre(table)
                .iter()
                .position(|component| *component == source)?;
            tuple.get(position).copied()
        });
        let component = component.ok_or(AdjunctionError::NotANaturalMap { row: copy })?;
        row_map.insert(copy, component);
    }
    Ok(FInstanceHom::new(row_map))
}

/// Transpose `Hom_S'(Delta_F Y, X) -> Hom_T(Y, Pi_F X)`, sending `g` to
/// `Pi_F(g) . eta_Y`.
///
/// Row `r` of `Y` at `d` maps to the row of `Pi_F X` whose tuple holds `g`
/// of the copy of `r` at every table of `d`'s fibre.
///
/// # Errors
///
/// As [`f_pi_unit`].
pub fn f_pi_transpose_left(
    g: &FInstanceHom,
    y: &FInstance,
    x: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    check_target_instance(y, functor)?;
    let (delta, to_original) = f_delta_with_hom(y, functor)?;
    let copies = copies(&delta, &to_original);
    let pi = pi_limit(x, functor, max_product_size).map_err(AdjunctionError::Pi)?;
    let mut row_map = BTreeMap::new();
    for (table, rows) in y.tables() {
        let fibre = functor.table_fibre(table);
        for id in rows.ids() {
            let tuple: Option<Vec<RowId>> = fibre
                .iter()
                .map(|&source| {
                    copies
                        .get(&(source.clone(), id))
                        .and_then(|&copy| g.image(copy))
                })
                .collect();
            let image = tuple
                .and_then(|tuple| pi.rows.get(&(table.clone(), tuple)))
                .ok_or(AdjunctionError::NotANaturalMap { row: id })?;
            row_map.insert(id, *image);
        }
    }
    Ok(FInstanceHom::new(row_map))
}

/// Transpose `Hom_T(Y, Pi_F X) -> Hom_S'(Delta_F Y, X)`, sending `f` to
/// `eps_X . Delta_F(f)`, the inverse of [`f_pi_transpose_left`].
///
/// The copy at `s` of a row `r` of `Y` maps to the component at `s` of
/// `f(r)`.
///
/// # Errors
///
/// As [`f_pi_counit`], and [`AdjunctionError::NotANaturalMap`] when `f`
/// sends a row outside `Pi_F X`.
pub fn f_pi_transpose_right(
    f: &FInstanceHom,
    y: &FInstance,
    x: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    let (delta, to_original) = f_delta_with_hom(y, functor)?;
    let pi = pi_limit(x, functor, max_product_size).map_err(AdjunctionError::Pi)?;
    let mut row_map = BTreeMap::new();
    for (&copy, &original) in &to_original.row_map {
        let component = delta.table_of(copy).and_then(|source| {
            let (table, tuple) = pi.tuples.get(&f.image(original)?)?;
            let position = functor
                .table_fibre(table)
                .iter()
                .position(|component| *component == source)?;
            tuple.get(position).copied()
        });
        let component = component.ok_or(AdjunctionError::NotANaturalMap { row: copy })?;
        row_map.insert(copy, component);
    }
    Ok(FInstanceHom::new(row_map))
}

// ---------------------------------------------------------------------------
// Graph Delta (vertex-injective total maps)
// ---------------------------------------------------------------------------

/// `Delta_F` for graph instances, defined for vertex-injective total maps,
/// as [`w_delta`] is for trees.
///
/// Reindexes a `T`-instance to an `S`-instance by relabelling each node's
/// anchor `t` back to the unique `s` with `F(s) = t` and each edge's label
/// to its unique preimage, preserving node ids, edge ids, cells, values and
/// positions. An edge whose label lies outside the image of the edge map has
/// no `S` label to take, so the pullback drops it (the edges of a reified fan,
/// labelled by the fan, keep their label with its endpoints relabelled), as [`f_delta`] drops the
/// pairs of such a foreign key; Delta thus commutes with the graph form of
/// a table whose rows all sit at vertices in the image of `F`. A row at a
/// vertex outside that image is dropped by [`f_delta`] but makes this
/// pullback fail with [`AdjunctionError::AnchorOutsideImage`], so the two
/// differ on such a table. A reified fan node (of shape
/// [`NodeShape::HyperEdge`](crate::metadata::NodeShape::HyperEdge)) is
/// anchored at its hyper-edge, not at a vertex, and keeps its anchor.
///
/// # Errors
///
/// Returns [`AdjunctionError::NonInjectiveVertexMap`] or
/// [`AdjunctionError::NonInjectiveEdgeMap`] when the map merges vertices or
/// edges (the pullback would then copy nodes, which this reindexing does not
/// do), and [`AdjunctionError::AnchorOutsideImage`] when a node's anchor is
/// not in the image of `F`.
pub fn g_delta(y: &GInstance, migration: &CompiledMigration) -> Result<GInstance, AdjunctionError> {
    let inverse_vertex = invert_vertex_map(migration)?;
    let inverse_edge = invert_edge_map(migration)?;

    let mut ids: Vec<u32> = y.nodes().keys().copied().collect();
    ids.sort_unstable();
    let mut out = GInstance::new();
    for id in ids {
        let mut node = y.nodes()[&id].clone();
        if node.shape != crate::metadata::NodeShape::HyperEdge {
            node.anchor = inverse_vertex
                .get(&node.anchor)
                .ok_or_else(|| AdjunctionError::AnchorOutsideImage(node.anchor.clone()))?
                .clone();
        }
        out.insert_node(node).map_err(AdjunctionError::Graph)?;
    }
    for (id, edge) in y.edges_sorted() {
        let label = if let Some(preimage) = inverse_edge.get(&edge.edge) {
            preimage.clone()
        } else if is_fan_kind(&edge.edge.kind) {
            // The edges of a reified fan are labelled by the fan, not by a
            // schema edge; they travel with their endpoints relabelled.
            Edge {
                src: relabel(&inverse_vertex, &edge.edge.src),
                tgt: relabel(&inverse_vertex, &edge.edge.tgt),
                kind: edge.edge.kind.clone(),
                name: edge.edge.name.clone(),
            }
        } else {
            continue;
        };
        out.insert_edge(
            id,
            GEdge::new(edge.src, edge.tgt, label).with_position(edge.position),
        )
        .map_err(AdjunctionError::Graph)?;
    }
    Ok(out)
}

/// Whether an edge kind is one of the reserved kinds of a reified fan.
fn is_fan_kind(kind: &Name) -> bool {
    kind.as_ref() == crate::ginstance::HYPER_PARENT_KIND
        || kind.as_ref() == crate::ginstance::HYPER_LABEL_KIND
}

/// A vertex's preimage under the inverted vertex map, or the vertex itself
/// when it has none (a fan's hyper-edge anchor).
fn relabel(inverse_vertex: &HashMap<Name, Name>, vertex: &Name) -> Name {
    inverse_vertex
        .get(vertex)
        .cloned()
        .unwrap_or_else(|| vertex.clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use panproto_schema::Vertex;
    use smallvec::SmallVec;

    use super::*;
    use crate::functor::Table;
    use crate::ginstance::EdgeId;
    use crate::metadata::Node;
    use crate::pi::{functor_pi_along, functor_pi_along_with_hom};
    use crate::rel_schema::RelSchema;
    use crate::value::{FieldPresence, Value};

    // -- shared builders -------------------------------------------------

    fn edge(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    /// A minimal schema over the given vertices and edges, enough for
    /// [`wtype_extend`] (which only consults it on the resolve fallback).
    fn schema_of(vertices: &[&str], edges: &[Edge]) -> Schema {
        let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        let mut edge_map = HashMap::new();
        for e in edges {
            between
                .entry((e.src.clone(), e.tgt.clone()))
                .or_default()
                .push(e.clone());
            outgoing.entry(e.src.clone()).or_default().push(e.clone());
            incoming.entry(e.tgt.clone()).or_default().push(e.clone());
            edge_map.insert(e.clone(), e.kind.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        Vertex {
                            id: Name::from(v),
                            kind: "object".into(),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: edge_map,
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
            ..Default::default()
        }
    }

    // -- a concrete merging example (functor side) -----------------------

    /// Migration merging source vertices `a` and `b` onto target `t`, with
    /// edges `e_a: a -> c` and `e_b: b -> c` onto `e: t -> u` and `c -> u`.
    fn merge_migration() -> CompiledMigration {
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("a"), Name::from("t"));
        vertex_remap.insert(Name::from("b"), Name::from("t"));
        vertex_remap.insert(Name::from("c"), Name::from("u"));

        let mut edge_remap = HashMap::new();
        edge_remap.insert(edge("a", "c", "ea"), edge("t", "u", "e"));
        edge_remap.insert(edge("b", "c", "eb"), edge("t", "u", "e"));

        CompiledMigration {
            surviving_verts: HashSet::from([Name::from("t"), Name::from("u")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        }
    }

    fn rename_migration() -> CompiledMigration {
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("root"), Name::from("box"));
        vertex_remap.insert(Name::from("leaf"), Name::from("item"));
        let mut edge_remap = HashMap::new();
        edge_remap.insert(edge("root", "leaf", "child"), edge("box", "item", "child"));
        CompiledMigration {
            surviving_verts: HashSet::from([Name::from("box"), Name::from("item")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        }
    }

    fn w_pair() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(
            1,
            Node::new(1, "leaf").with_value(FieldPresence::Present(Value::Str("x".into()))),
        );
        WInstance::new(
            nodes,
            vec![(0, 1, edge("root", "leaf", "child"))],
            vec![],
            0,
            "root".into(),
        )
    }

    #[test]
    fn w_delta_inverts_sigma_on_injective_maps() {
        let m = rename_migration();
        let tgt = schema_of(&["box", "item"], &[edge("box", "item", "child")]);
        let x = w_pair();
        let sigma = w_sigma(&x, &tgt, &m).expect("sigma");
        // Sigma relabels anchors forward.
        assert_eq!(sigma.nodes[&0].anchor, Name::from("box"));
        assert_eq!(sigma.nodes[&1].anchor, Name::from("item"));
        // Delta inverts it exactly.
        let round = w_delta(&sigma, &m).expect("delta");
        assert_eq!(round.nodes[&0].anchor, Name::from("root"));
        assert_eq!(round.nodes[&1].anchor, Name::from("leaf"));

        let unit = w_unit(&x);
        unit.check(&x, &round).expect("unit checks");
    }

    #[test]
    fn w_delta_rejects_merging_maps() {
        let m = merge_migration();
        // A T-instance anchored at the merged image `t`.
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "t"));
        let y = WInstance::new(nodes, vec![], vec![], 0, "t".into());
        let err = w_delta(&y, &m).expect_err("merging map has no W-type pullback");
        assert!(matches!(err, AdjunctionError::NonInjectiveVertexMap { .. }));
    }

    // -- identity by omission --------------------------------------------

    /// A migration renaming `a` to `t` and leaving `c` to survive under its
    /// own name, which it states by listing `c` among the surviving vertices
    /// and naming no image for it.
    fn omission_migration() -> CompiledMigration {
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("a"), Name::from("t"));

        let mut edge_remap = HashMap::new();
        edge_remap.insert(edge("a", "c", "ea"), edge("t", "c", "ea"));

        CompiledMigration {
            surviving_verts: HashSet::from([Name::from("t"), Name::from("c")]),
            surviving_edges: HashSet::from([edge("t", "c", "ea")]),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        }
    }

    #[test]
    fn w_delta_inverts_a_vertex_surviving_by_omission() {
        let m = omission_migration();
        let tgt = schema_of(&["t", "c"], &[edge("t", "c", "ea")]);
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "a"));
        nodes.insert(
            1,
            Node::new(1, "c").with_value(FieldPresence::Present(Value::Str("x".into()))),
        );
        let x = WInstance::new(
            nodes,
            vec![(0, 1, edge("a", "c", "ea"))],
            vec![],
            0,
            "a".into(),
        );
        let sigma = w_sigma(&x, &tgt, &m).expect("sigma");
        let round = w_delta(&sigma, &m).expect("delta inverts sigma on the whole domain");
        assert_eq!(round.nodes[&0].anchor, Name::from("a"));
        assert_eq!(round.nodes[&1].anchor, Name::from("c"));
        w_unit(&x).check(&x, &round).expect("unit checks");
    }

    // -- row allocation and row maps --------------------------------------

    #[test]
    fn graph_delta_rejects_a_non_injective_map() {
        let m = merge_migration();
        let y = GInstance::new().with_node(Node::new(0, "t"));
        let err = g_delta(&y, &m).expect_err("a merging map has no reindexing pullback");
        assert!(matches!(err, AdjunctionError::NonInjectiveVertexMap { .. }));
    }

    #[test]
    fn graph_delta_relabels_along_an_injective_map() {
        let m = rename_migration();
        let y = GInstance::new()
            .with_node(Node::new(3, "box"))
            .with_node(Node::new(8, "item").with_extra_field("n", Value::Int(1)))
            .with_edge_id(EdgeId(5), 3, 8, edge("box", "item", "child"));
        let delta = g_delta(&y, &m).unwrap();
        assert_eq!(
            delta.node(3).map(|n| n.anchor.clone()),
            Some(Name::from("root"))
        );
        assert_eq!(
            delta.node(8).map(|n| n.anchor.clone()),
            Some(Name::from("leaf"))
        );
        assert_eq!(
            delta.node(8).and_then(|n| n.extra_fields.get("n")),
            Some(&Value::Int(1))
        );
        assert_eq!(
            delta.edge(EdgeId(5)).map(|e| e.edge.clone()),
            Some(edge("root", "leaf", "child"))
        );
        let outside = GInstance::new().with_node(Node::new(0, "elsewhere"));
        assert!(matches!(
            g_delta(&outside, &m),
            Err(AdjunctionError::AnchorOutsideImage(a)) if a == "elsewhere"
        ));
    }

    /// Delta commutes with the graph form of a table: pulling a table back
    /// and then taking its graph gives the graph of the pullback. An edge
    /// whose label lies outside the image of the edge map has no `S` label,
    /// so the graph Delta drops it exactly as the table Delta drops its
    /// pairs.
    #[test]
    fn graph_delta_commutes_with_the_graph_form_of_a_table() {
        let m = rename_migration();
        let y = FInstance::new()
            .with_rows("box", [(RowId(0), crate::functor::Row::new())])
            .and_then(|f| f.with_rows("item", [(RowId(1), crate::functor::Row::new())]))
            .and_then(|f| f.with_foreign_key(edge("box", "item", "child"), [(RowId(0), RowId(1))]))
            .and_then(|f| f.with_foreign_key(edge("box", "item", "other"), [(RowId(0), RowId(1))]))
            .unwrap();
        let functor = MigrationFunctor::new(
            &table_schema(
                &["root", "leaf"],
                &[edge("root", "leaf", "child"), edge("root", "leaf", "other")],
            ),
            &table_schema(
                &["box", "item"],
                &[edge("box", "item", "child"), edge("box", "item", "other")],
            ),
            &m,
        )
        .unwrap();
        let (table_then_graph, _) =
            GInstance::from_finstance(&f_delta(&y, &functor).unwrap()).unwrap();
        let (graph, _) = GInstance::from_finstance(&y).unwrap();
        let graph_then_delta = g_delta(&graph, &m).unwrap();
        assert_eq!(graph_then_delta.edge_count(), 1);
        assert!(
            GInstance::identical(&graph_then_delta, &table_then_graph),
            "{graph_then_delta:?} != {table_then_graph:?}"
        );
    }

    // -- table Kan extensions along a migration functor -------------------

    /// A foreign key `src -> tgt` named `name`.
    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// A schema whose vertices are all tables and whose edges are the given
    /// foreign keys.
    fn table_schema(tables: &[&str], relations: &[Edge]) -> Schema {
        Schema {
            protocol: "test".into(),
            vertices: tables
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        Vertex {
                            id: Name::from(v),
                            kind: crate::rel_schema::TABLE_KIND.into(),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: relations
                .iter()
                .map(|e| (e.clone(), e.kind.clone()))
                .collect(),
            ..Schema::default()
        }
    }

    /// Rows without cells, with the given ids.
    fn blank(ids: &[u32]) -> Vec<(RowId, crate::functor::Row)> {
        ids.iter()
            .map(|&i| (RowId(i), crate::functor::Row::new()))
            .collect()
    }

    /// Every homomorphism `dom -> cod` between instances of one schema,
    /// found by assigning rows in ascending id and pruning on each pair whose
    /// two rows are assigned.
    fn homs(dom: &FInstance, cod: &FInstance) -> Vec<FInstanceHom> {
        fn go(
            i: usize,
            rows: &[RowId],
            candidates: &[Vec<RowId>],
            pairs: &[(Edge, RowId, RowId)],
            cod: &FInstance,
            chosen: &mut BTreeMap<RowId, RowId>,
            out: &mut Vec<FInstanceHom>,
        ) {
            let consistent = pairs
                .iter()
                .all(|(e, s, t)| match (chosen.get(s), chosen.get(t)) {
                    (Some(&ms), Some(&mt)) => cod.contains_pair(e, ms, mt),
                    _ => true,
                });
            if !consistent {
                return;
            }
            if i == rows.len() {
                out.push(FInstanceHom::new(chosen.clone()));
                return;
            }
            for &c in &candidates[i] {
                chosen.insert(rows[i], c);
                go(i + 1, rows, candidates, pairs, cod, chosen, out);
                chosen.remove(&rows[i]);
            }
        }
        let rows = dom.row_ids();
        let candidates: Vec<Vec<RowId>> = rows
            .iter()
            .map(|&r| {
                let table = dom.table_of(r).cloned().unwrap_or_default();
                let cells = dom.row(r).cloned().unwrap_or_default();
                cod.rows(&table)
                    .into_iter()
                    .filter(|(_, c)| crate::functor::same_row(c, &cells))
                    .map(|(id, _)| id)
                    .collect()
            })
            .collect();
        let pairs = dom.all_pairs();
        let mut out = Vec::new();
        let mut chosen: BTreeMap<RowId, RowId> = BTreeMap::new();
        go(0, &rows, &candidates, &pairs, cod, &mut chosen, &mut out);
        out
    }

    /// Whether `hom` is the identity on `instance`.
    fn is_identity(hom: &FInstanceHom, instance: &FInstance) -> bool {
        hom.row_map == FInstanceHom::identity(instance).row_map
    }

    /// The Hom bijection of `Sigma -| Delta` on `(x, y)`, counted and
    /// witnessed: the transposes land in the hom-sets they claim and are
    /// mutually inverse. Returns `|Hom(Sigma x, y)|`.
    fn check_sigma_bijection(
        f: &MigrationFunctor,
        x: &FInstance,
        y: &FInstance,
    ) -> Result<usize, String> {
        let sigma = f_sigma(x, f).map_err(|e| e.to_string())?;
        let delta = f_delta(y, f).map_err(|e| e.to_string())?;
        let left = homs(&sigma, y);
        let right = homs(x, &delta);
        if left.len() != right.len() {
            return Err(format!(
                "|Hom(Sigma x, y)| = {} but |Hom(x, Delta y)| = {}",
                left.len(),
                right.len()
            ));
        }
        for g in &left {
            let t = f_transpose_left(g, x, y, f).map_err(|e| e.to_string())?;
            t.check(x, &delta)
                .map_err(|e| format!("transpose of {g:?}: {e}"))?;
            let back = f_transpose_right(&t, x, y, f).map_err(|e| e.to_string())?;
            if back.row_map != g.row_map {
                return Err(format!("the Sigma transposes do not invert {g:?}"));
            }
        }
        for h in &right {
            let t = f_transpose_right(h, x, y, f).map_err(|e| e.to_string())?;
            t.check(&sigma, y)
                .map_err(|e| format!("transpose of {h:?}: {e}"))?;
            let back = f_transpose_left(&t, x, y, f).map_err(|e| e.to_string())?;
            if back.row_map != h.row_map {
                return Err(format!("the Sigma transposes do not invert {h:?}"));
            }
        }
        Ok(left.len())
    }

    /// The Hom bijection of `Delta -| Pi` on `(y, x)`, counted and
    /// witnessed. Returns `|Hom(Delta y, x)|`.
    fn check_pi_bijection(
        f: &MigrationFunctor,
        x: &FInstance,
        y: &FInstance,
        max: usize,
    ) -> Result<usize, String> {
        let delta = f_delta(y, f).map_err(|e| e.to_string())?;
        let pi = functor_pi_along(x, f, max).map_err(|e| e.to_string())?;
        let left = homs(&delta, x);
        let right = homs(y, &pi);
        if left.len() != right.len() {
            return Err(format!(
                "|Hom(Delta y, x)| = {} but |Hom(y, Pi x)| = {}",
                left.len(),
                right.len()
            ));
        }
        for g in &left {
            let t = f_pi_transpose_left(g, y, x, f, max).map_err(|e| e.to_string())?;
            t.check(y, &pi)
                .map_err(|e| format!("transpose of {g:?}: {e}"))?;
            let back = f_pi_transpose_right(&t, y, x, f, max).map_err(|e| e.to_string())?;
            if back.row_map != g.row_map {
                return Err(format!("the Pi transposes do not invert {g:?}"));
            }
        }
        for h in &right {
            let t = f_pi_transpose_right(h, y, x, f, max).map_err(|e| e.to_string())?;
            t.check(&delta, x)
                .map_err(|e| format!("transpose of {h:?}: {e}"))?;
            let back = f_pi_transpose_left(&t, y, x, f, max).map_err(|e| e.to_string())?;
            if back.row_map != h.row_map {
                return Err(format!("the Pi transposes do not invert {h:?}"));
            }
        }
        Ok(left.len())
    }

    /// Both triangle identities of both adjunctions at `x` and `y`, with
    /// every unit and counit checked as a homomorphism.
    fn check_triangles(
        f: &MigrationFunctor,
        x: &FInstance,
        y: &FInstance,
        max: usize,
    ) -> Result<(), String> {
        let err = |e: AdjunctionError| e.to_string();
        // Sigma -| Delta: eps_{Sigma x} . Sigma(eta_x) = id.
        let sigma_x = f_sigma(x, f).map_err(err)?;
        let eta_x = f_unit(x, f).map_err(err)?;
        let delta_sigma_x = f_delta(&sigma_x, f).map_err(err)?;
        eta_x.check(x, &delta_sigma_x).map_err(|e| e.to_string())?;
        let sigma_eta = f_sigma_on_hom(&eta_x, x, f).map_err(err)?;
        let sigma_delta_sigma_x = f_sigma(&delta_sigma_x, f).map_err(err)?;
        sigma_eta
            .check(&sigma_x, &sigma_delta_sigma_x)
            .map_err(|e| e.to_string())?;
        let eps = f_counit(&sigma_x, f).map_err(err)?;
        eps.check(&sigma_delta_sigma_x, &sigma_x)
            .map_err(|e| e.to_string())?;
        let first = sigma_eta.compose(&eps).map_err(|e| e.to_string())?;
        if !is_identity(&first, &sigma_x) {
            return Err(format!("eps . Sigma(eta) = {first:?}"));
        }
        // Delta(eps_y) . eta_{Delta y} = id.
        let delta_y = f_delta(y, f).map_err(err)?;
        let eta = f_unit(&delta_y, f).map_err(err)?;
        let sigma_delta_y = f_sigma(&delta_y, f).map_err(err)?;
        let eps_y = f_counit(y, f).map_err(err)?;
        eps_y.check(&sigma_delta_y, y).map_err(|e| e.to_string())?;
        let delta_eps = f_delta_on_hom(&eps_y, &sigma_delta_y, y, f).map_err(err)?;
        let second = eta.compose(&delta_eps).map_err(|e| e.to_string())?;
        if !is_identity(&second, &delta_y) {
            return Err(format!("Delta(eps) . eta = {second:?}"));
        }
        // Delta -| Pi: eps'_{Delta y} . Delta(eta'_y) = id.
        let eta_p = f_pi_unit(y, f, max).map_err(err)?;
        let pi_delta_y = functor_pi_along(&delta_y, f, max).map_err(|e| e.to_string())?;
        eta_p.check(y, &pi_delta_y).map_err(|e| e.to_string())?;
        let delta_eta = f_delta_on_hom(&eta_p, y, &pi_delta_y, f).map_err(err)?;
        let eps_p = f_pi_counit(&delta_y, f, max).map_err(err)?;
        let delta_pi_delta_y = f_delta(&pi_delta_y, f).map_err(err)?;
        eps_p
            .check(&delta_pi_delta_y, &delta_y)
            .map_err(|e| e.to_string())?;
        let third = delta_eta.compose(&eps_p).map_err(|e| e.to_string())?;
        if !is_identity(&third, &delta_y) {
            return Err(format!("eps' . Delta(eta') = {third:?}"));
        }
        // Pi(eps'_x) . eta'_{Pi x} = id.
        let pi_x = functor_pi_along(x, f, max).map_err(|e| e.to_string())?;
        let eta_pi = f_pi_unit(&pi_x, f, max).map_err(err)?;
        let eps_x = f_pi_counit(x, f, max).map_err(err)?;
        let delta_pi_x = f_delta(&pi_x, f).map_err(err)?;
        eps_x.check(&delta_pi_x, x).map_err(|e| e.to_string())?;
        let pi_eps = f_pi_on_hom(&eps_x, &delta_pi_x, x, f, max).map_err(err)?;
        let fourth = eta_pi.compose(&pi_eps).map_err(|e| e.to_string())?;
        if !is_identity(&fourth, &pi_x) {
            return Err(format!("Pi(eps') . eta' = {fourth:?}"));
        }
        Ok(())
    }

    /// Every tuple with one row of each table of `fibre` (in order), by
    /// brute force over the whole product, keeping those whose rows carry
    /// the same cells. The empty fibre has the one empty tuple.
    fn brute_cones(x: &FInstance, fibre: &[&Name]) -> BTreeSet<Vec<RowId>> {
        let mut tuples: Vec<Vec<RowId>> = vec![Vec::new()];
        for table in fibre {
            let rows: Vec<RowId> = x.rows(table).into_iter().map(|(id, _)| id).collect();
            tuples = tuples
                .into_iter()
                .flat_map(|t| {
                    rows.iter().map(move |&r| {
                        let mut longer = t.clone();
                        longer.push(r);
                        longer
                    })
                })
                .collect();
        }
        tuples
            .into_iter()
            .filter(|t| {
                t.windows(2)
                    .all(|w| crate::functor::same_row(x.row(w[0]).unwrap(), x.row(w[1]).unwrap()))
            })
            .collect()
    }

    /// Check `pi` against the limit formula over the comma categories,
    /// computed independently by brute force: the projections must send the
    /// rows of each target table bijectively onto its cones, each row
    /// carrying its components' cells, and the pairs of each target relation
    /// exactly onto the pairs of cones every relation sent to it holds.
    fn check_limit(
        f: &MigrationFunctor,
        x: &FInstance,
        pi: &FInstance,
        projections: &BTreeMap<Name, FInstanceHom>,
    ) -> Result<(), String> {
        let tuple_of = |row: RowId, table: &Name| -> Option<Vec<RowId>> {
            f.table_fibre(table)
                .into_iter()
                .map(|s| projections.get(s)?.image(row))
                .collect()
        };
        let outside: BTreeSet<&Name> = pi
            .table_names()
            .into_iter()
            .filter(|t| !f.target().is_table(t))
            .collect();
        if !outside.is_empty() {
            return Err(format!(
                "rows at vertices that are no target tables: {outside:?}"
            ));
        }
        for d in f.target().tables() {
            let fibre = f.table_fibre(d);
            let cones = brute_cones(x, &fibre);
            let mut seen = BTreeSet::new();
            for (row, cells) in pi.rows(d) {
                let tuple =
                    tuple_of(row, d).ok_or_else(|| format!("row {row:?} has no projection"))?;
                let expected = tuple
                    .first()
                    .map_or_else(crate::functor::Row::new, |&r| x.row(r).unwrap().clone());
                if !crate::functor::same_row(cells, &expected) {
                    return Err(format!("row {row:?} of {d} carries {cells:?}"));
                }
                if !seen.insert(tuple.clone()) {
                    return Err(format!("two rows of {d} project to {tuple:?}"));
                }
            }
            if seen != cones {
                return Err(format!(
                    "{d}: rows project to {seen:?}, cones are {cones:?}"
                ));
            }
        }
        for relation in f.target().relations() {
            let from = f.table_fibre(&relation.src);
            let to = f.table_fibre(&relation.tgt);
            let carried = f.relation_fibre(relation);
            let mut expected = BTreeSet::new();
            for tx in brute_cones(x, &from) {
                for ty in brute_cones(x, &to) {
                    let holds = carried.iter().all(|e| {
                        let s = from.iter().position(|t| *t == &e.src).unwrap();
                        let t = to.iter().position(|t| *t == &e.tgt).unwrap();
                        x.contains_pair(e, tx[s], ty[t])
                    });
                    if holds {
                        expected.insert((tx.clone(), ty.clone()));
                    }
                }
            }
            let actual: BTreeSet<(Vec<RowId>, Vec<RowId>)> = pi
                .pairs(relation)
                .iter()
                .map(|&(s, t)| {
                    (
                        tuple_of(s, &relation.src).unwrap_or_default(),
                        tuple_of(t, &relation.tgt).unwrap_or_default(),
                    )
                })
                .collect();
            if actual != expected || actual.len() != pi.pairs(relation).len() {
                return Err(format!(
                    "{relation:?}: pairs {actual:?}, cones {expected:?}"
                ));
            }
        }
        let stray: Vec<&Edge> = pi
            .foreign_keys()
            .into_iter()
            .map(|(e, _)| e)
            .filter(|e| !f.target().is_relation(e))
            .collect();
        if stray.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "pairs on edges that are no target relations: {stray:?}"
            ))
        }
    }

    /// A generated case: a functor between small relation-ified schemas, an
    /// instance on its domain `S'` and one on its target.
    #[derive(Clone, Debug)]
    struct KanCase {
        src: Schema,
        tgt: Schema,
        migration: CompiledMigration,
        x: FInstance,
        y: FInstance,
    }

    /// A small linear congruential generator, so a case is a function of a
    /// seed.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % n.max(1)
        }

        fn chance(&mut self, percent: u64) -> bool {
            self.below(100) < percent
        }
    }

    /// Rows for `table`, numbered from `next`, carrying cells only when
    /// `cells` allows (one in five rows holds `v = 0` or `v = 1`).
    fn gen_rows(
        r: &mut Lcg,
        next: &mut u32,
        count: u64,
        cells: bool,
    ) -> Vec<(RowId, crate::functor::Row)> {
        (0..count)
            .map(|_| {
                let id = RowId(*next);
                *next += 1;
                let mut row = crate::functor::Row::new();
                if cells && r.chance(20) {
                    row.insert(
                        "v".to_owned(),
                        Value::Int(i64::try_from(r.below(2)).unwrap()),
                    );
                }
                (id, row)
            })
            .collect()
    }

    /// A random subset of the pairs of rows of `a` and rows of `b`, so the
    /// relation may be partial and many-valued.
    fn gen_pairs(
        r: &mut Lcg,
        inst: &FInstance,
        a: &str,
        b: &str,
        percent: u64,
    ) -> Vec<(RowId, RowId)> {
        let mut pairs = Vec::new();
        for (s, _) in inst.rows(a) {
            for (t, _) in inst.rows(b) {
                if r.chance(percent) {
                    pairs.push((s, t));
                }
            }
        }
        pairs
    }

    /// The case of a seed: up to three source tables (one may be dropped),
    /// up to three target tables (some may lie outside the image), foreign
    /// keys between any two tables including self-references, some carried
    /// (possibly onto one image), some unmentioned, and target-only foreign
    /// keys; a target-only foreign key is also listed as surviving, as
    /// `panproto_mig::compile` lists surviving image edges.
    fn kan_case(seed: u64) -> KanCase {
        let mut r = Lcg(seed);
        let s_count = 1 + r.below(3);
        let t_count = 1 + r.below(3);
        let s_tables: Vec<String> = (0..s_count).map(|i| format!("s{i}")).collect();
        let t_tables: Vec<String> = (0..t_count).map(|i| format!("t{i}")).collect();
        let mut migration = CompiledMigration::default();
        for s in &s_tables {
            if r.chance(85) {
                let t = &t_tables[usize::try_from(r.below(t_count)).unwrap()];
                migration
                    .vertex_remap
                    .insert(s.as_str().into(), t.as_str().into());
                migration.surviving_verts.insert(t.as_str().into());
            }
        }
        let mut s_edges = Vec::new();
        let mut t_edges = BTreeSet::new();
        for i in 0..r.below(4) {
            let a = &s_tables[usize::try_from(r.below(s_count)).unwrap()];
            let b = &s_tables[usize::try_from(r.below(s_count)).unwrap()];
            let e = fk(a, b, &format!("r{i}"));
            s_edges.push(e.clone());
            let images = (
                migration.vertex_remap.get(a.as_str()).cloned(),
                migration.vertex_remap.get(b.as_str()).cloned(),
            );
            if let (Some(fa), Some(fb)) = images {
                if r.chance(75) {
                    let image = fk(&fa, &fb, &format!("E{}", r.below(2)));
                    t_edges.insert(image.clone());
                    migration.edge_remap.insert(e, image);
                }
            }
        }
        for i in 0..r.below(3) {
            let a = &t_tables[usize::try_from(r.below(t_count)).unwrap()];
            let b = &t_tables[usize::try_from(r.below(t_count)).unwrap()];
            let e = fk(a, b, &format!("X{i}"));
            migration.surviving_edges.insert(e.clone());
            t_edges.insert(e);
        }
        let s_refs: Vec<&str> = s_tables.iter().map(String::as_str).collect();
        let t_refs: Vec<&str> = t_tables.iter().map(String::as_str).collect();
        let t_edges: Vec<Edge> = t_edges.into_iter().collect();
        let src = table_schema(&s_refs, &s_edges);
        let tgt = table_schema(&t_refs, &t_edges);
        let f = MigrationFunctor::new(&src, &tgt, &migration).unwrap();

        let mut next = 0u32;
        let mut x = FInstance::new();
        for s in f.table_map().keys() {
            let count = r.below(3);
            x = x
                .with_rows(s.clone(), gen_rows(&mut r, &mut next, count, true))
                .unwrap();
        }
        for e in f.relation_map().keys() {
            let pairs = gen_pairs(&mut r, &x, &e.src, &e.tgt, 40);
            x = x.with_foreign_key(e.clone(), pairs).unwrap();
        }
        let outside = f.tables_outside_image();
        let mut y = FInstance::new();
        for t in f.target().tables() {
            let count = r.below(3);
            let cells = !outside.contains(t);
            y = y
                .with_rows(t.clone(), gen_rows(&mut r, &mut next, count, cells))
                .unwrap();
        }
        for e in f.target().relations() {
            let pairs = gen_pairs(&mut r, &y, &e.src, &e.tgt, 40);
            y = y.with_foreign_key(e.clone(), pairs).unwrap();
        }
        KanCase {
            src,
            tgt,
            migration,
            x,
            y,
        }
    }

    impl KanCase {
        fn functor(&self) -> MigrationFunctor {
            MigrationFunctor::new(&self.src, &self.tgt, &self.migration).unwrap()
        }
    }

    /// A merge of `a` and `b` onto `m`, the foreign key `l: a -> b` carried
    /// to a loop on `m`, the unmentioned `u: b -> a`, and `z`, a target table
    /// outside the image, with the target-only `w: m -> z`.
    fn merge_with_loop() -> (Schema, Schema, CompiledMigration) {
        let l = fk("a", "b", "l");
        let src = table_schema(&["a", "b"], &[l.clone(), fk("b", "a", "u")]);
        let tgt = table_schema(&["m", "z"], &[fk("m", "m", "L"), fk("m", "z", "w")]);
        let mut migration = CompiledMigration::default();
        migration.vertex_remap.insert("a".into(), "m".into());
        migration.vertex_remap.insert("b".into(), "m".into());
        migration.surviving_verts.insert("m".into());
        migration.edge_remap.insert(l, fk("m", "m", "L"));
        (src, tgt, migration)
    }

    /// Every instance on `S'` of [`merge_with_loop`] with at most one row per
    /// table, and on its target with at most two rows of `m` and one of `z`,
    /// all without cells, over every set of pairs.
    fn tiny_instances() -> (Vec<FInstance>, Vec<FInstance>) {
        let subsets = |pairs: &[(RowId, RowId)]| -> Vec<Vec<(RowId, RowId)>> {
            (0..1u32 << pairs.len())
                .map(|mask| {
                    pairs
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask & (1 << i) != 0)
                        .map(|(_, p)| *p)
                        .collect()
                })
                .collect()
        };
        let mut xs = Vec::new();
        for a in 0..=1u32 {
            for b in 0..=1u32 {
                let base = FInstance::new()
                    .with_rows("a", blank(&(0..a).collect::<Vec<_>>()))
                    .and_then(|f| f.with_rows("b", blank(&(10..10 + b).collect::<Vec<_>>())))
                    .unwrap();
                let possible: Vec<(RowId, RowId)> = base
                    .rows("a")
                    .into_iter()
                    .flat_map(|(s, _)| base.rows("b").into_iter().map(move |(t, _)| (s, t)))
                    .collect();
                for pairs in subsets(&possible) {
                    xs.push(
                        base.clone()
                            .with_foreign_key(fk("a", "b", "l"), pairs)
                            .unwrap(),
                    );
                }
            }
        }
        let mut ys = Vec::new();
        for m in 0..=2u32 {
            for z in 0..=1u32 {
                let base = FInstance::new()
                    .with_rows("m", blank(&(20..20 + m).collect::<Vec<_>>()))
                    .and_then(|f| f.with_rows("z", blank(&(30..30 + z).collect::<Vec<_>>())))
                    .unwrap();
                let loops: Vec<(RowId, RowId)> = base
                    .rows("m")
                    .into_iter()
                    .flat_map(|(s, _)| base.rows("m").into_iter().map(move |(t, _)| (s, t)))
                    .collect();
                let to_z: Vec<(RowId, RowId)> = base
                    .rows("m")
                    .into_iter()
                    .flat_map(|(s, _)| base.rows("z").into_iter().map(move |(t, _)| (s, t)))
                    .collect();
                for l in subsets(&loops) {
                    for w in subsets(&to_z) {
                        ys.push(
                            base.clone()
                                .with_foreign_key(fk("m", "m", "L"), l.clone())
                                .and_then(|f| f.with_foreign_key(fk("m", "z", "w"), w))
                                .unwrap(),
                        );
                    }
                }
            }
        }
        (xs, ys)
    }

    #[test]
    fn adjunctions_hold_on_every_tiny_instance() {
        let (src, tgt, migration) = merge_with_loop();
        let f = MigrationFunctor::new(&src, &tgt, &migration).unwrap();
        let (xs, ys) = tiny_instances();
        let mut checked = 0usize;
        let mut nonempty = [0usize; 2];
        for x in &xs {
            for y in &ys {
                let sigma = check_sigma_bijection(&f, x, y).unwrap();
                let pi = check_pi_bijection(&f, x, y, 1000).unwrap();
                check_triangles(&f, x, y, 1000).unwrap();
                nonempty[0] += usize::from(sigma > 0);
                nonempty[1] += usize::from(pi > 0);
                checked += 1;
            }
        }
        assert_eq!(checked, xs.len() * ys.len());
        assert!(
            nonempty.iter().all(|&n| n > 0),
            "some hom-sets are inhabited: {nonempty:?}"
        );
        eprintln!(
            "checked {checked} pairs ({} source, {} target instances); inhabited: {nonempty:?}",
            xs.len(),
            ys.len()
        );
    }

    /// Rename every target table and foreign key of a case (`t` to `q`, and
    /// the names of foreign keys reversed), returning the renamed target
    /// schema and migration and the renaming of tables.
    fn rename_target(case: &KanCase) -> (Schema, CompiledMigration, BTreeMap<Name, Name>) {
        // Reverse the table order, so a renaming changes every ascending
        // order the target names could induce.
        let tables: Vec<Name> = RelSchema::new(&case.tgt).tables().iter().cloned().collect();
        let renaming: BTreeMap<Name, Name> = tables
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), Name::from(format!("q{}", tables.len() - i))))
            .collect();
        let edge = |e: &Edge| Edge {
            src: renaming[&e.src].clone(),
            tgt: renaming[&e.tgt].clone(),
            kind: e.kind.clone(),
            name: e.name.as_ref().map(|n| Name::from(format!("~{n}"))),
        };
        let renamed_tables: Vec<String> = renaming.values().map(ToString::to_string).collect();
        let renamed_edges: Vec<Edge> = case.tgt.edges.keys().map(edge).collect();
        let refs: Vec<&str> = renamed_tables.iter().map(String::as_str).collect();
        let tgt = table_schema(&refs, &renamed_edges);
        let mut migration = case.migration.clone();
        migration.vertex_remap = migration
            .vertex_remap
            .iter()
            .map(|(s, t)| (s.clone(), renaming[t].clone()))
            .collect();
        migration.surviving_verts = migration
            .surviving_verts
            .iter()
            .map(|t| renaming[t].clone())
            .collect();
        migration.edge_remap = migration
            .edge_remap
            .iter()
            .map(|(s, t)| (s.clone(), edge(t)))
            .collect();
        migration.surviving_edges = migration.surviving_edges.iter().map(edge).collect();
        (tgt, migration, renaming)
    }

    /// An instance on the target with its tables and foreign keys renamed.
    fn rename_instance(y: &FInstance, renaming: &BTreeMap<Name, Name>) -> FInstance {
        let tables = y.tables().into_iter().map(|(name, table)| {
            (
                renaming[name].clone(),
                table
                    .rows()
                    .into_iter()
                    .map(|(id, row)| (id, row.clone()))
                    .collect(),
            )
        });
        let keys = y.foreign_keys().into_iter().map(|(e, pairs)| {
            (
                Edge {
                    src: renaming[&e.src].clone(),
                    tgt: renaming[&e.tgt].clone(),
                    kind: e.kind.clone(),
                    name: e.name.as_ref().map(|n| Name::from(format!("~{n}"))),
                },
                pairs.iter().copied().collect(),
            )
        });
        FInstance::from_parts(tables, keys).unwrap()
    }

    mod kan_laws {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn sigma_is_left_adjoint_to_delta(seed in any::<u64>()) {
                let case = kan_case(seed);
                let f = case.functor();
                let count = check_sigma_bijection(&f, &case.x, &case.y);
                prop_assert!(count.is_ok(), "{:?} on {:?}", count, case);
            }

            #[test]
            fn pi_is_right_adjoint_to_delta(seed in any::<u64>()) {
                let case = kan_case(seed);
                let f = case.functor();
                let count = check_pi_bijection(&f, &case.x, &case.y, 10_000);
                prop_assert!(count.is_ok(), "{:?} on {:?}", count, case);
            }

            #[test]
            fn pi_is_the_limit_over_the_comma_category(seed in any::<u64>()) {
                let case = kan_case(seed);
                let f = case.functor();
                let (pi, projections) = functor_pi_along_with_hom(&case.x, &f, 10_000).unwrap();
                let result = check_limit(&f, &case.x, &pi, &projections);
                prop_assert!(result.is_ok(), "{:?} on {:?}", result, case);
            }

            /// The bound admits the largest iterated limit a case can ask for:
            /// three two-row tables merged into one give `Pi x` eight rows,
            /// `Pi (Delta (Pi x))` 512, and a target-only loop on it every
            /// pair of those, 262 144.
            #[test]
            fn triangle_identities_hold(seed in any::<u64>()) {
                let case = kan_case(seed);
                let f = case.functor();
                let result = check_triangles(&f, &case.x, &case.y, 1 << 18);
                prop_assert!(result.is_ok(), "{:?} on {:?}", result, case);
            }

            /// Along the identity functor, Sigma and Pi return the instance
            /// itself, and the returned homs are its identity.
            #[test]
            fn kan_extensions_along_the_identity_are_identities(seed in any::<u64>()) {
                let case = kan_case(seed);
                let f = MigrationFunctor::identity(&case.tgt);
                let (sigma, to_sigma) = f_sigma_with_hom(&case.y, &f).unwrap();
                prop_assert_eq!(&sigma, &case.y);
                prop_assert!(is_identity(&to_sigma, &case.y));
                let (pi, projections) = functor_pi_along_with_hom(&case.y, &f, 10_000).unwrap();
                prop_assert_eq!(&pi, &case.y);
                for (table, hom) in &projections {
                    for (row, image) in &hom.row_map {
                        prop_assert_eq!(row, image, "projection onto {}", table);
                    }
                }
                let delta = f_delta(&case.y, &f).unwrap();
                prop_assert_eq!(&delta, &case.y);
            }

            /// Renaming the target's tables and foreign keys renames the
            /// results of Sigma and Delta exactly, and Pi's exactly up to the
            /// one row of each table outside the image.
            #[test]
            fn results_do_not_depend_on_target_names(seed in any::<u64>()) {
                let case = kan_case(seed);
                let f = case.functor();
                let (tgt, migration, renaming) = rename_target(&case);
                let g = MigrationFunctor::new(&case.src, &tgt, &migration).unwrap();
                prop_assert_eq!(
                    rename_instance(&f_sigma(&case.x, &f).unwrap(), &renaming),
                    f_sigma(&case.x, &g).unwrap()
                );
                prop_assert_eq!(
                    f_delta(&case.y, &f).unwrap(),
                    f_delta(&rename_instance(&case.y, &renaming), &g).unwrap()
                );
                let pi = functor_pi_along(&case.x, &f, 10_000).unwrap();
                let renamed = functor_pi_along(&case.x, &g, 10_000).unwrap();
                let outside: BTreeSet<Name> = f.tables_outside_image().into_iter().cloned().collect();
                if outside.len() <= 1 {
                    prop_assert_eq!(rename_instance(&pi, &renaming), renamed);
                } else {
                    // The rows of the image tables keep their ids; the rows of
                    // the tables outside it are one each, without cells.
                    for (table, rows) in pi.tables() {
                        let theirs = renamed.table(&renaming[table]).map_or(0, Table::len);
                        prop_assert_eq!(rows.len(), theirs);
                        if !outside.contains(table) {
                            prop_assert_eq!(Some(rows), renamed.table(&renaming[table]));
                        }
                    }
                    prop_assert_eq!(pi.pair_count(), renamed.pair_count());
                }
            }

            /// Adding rows and pairs to the input embeds Pi of the smaller
            /// instance into Pi of the larger: `Pi(inclusion)` is an injective
            /// homomorphism.
            #[test]
            fn pi_is_monotone(seed in any::<u64>(), grow in any::<u64>()) {
                let case = kan_case(seed);
                let f = case.functor();
                let mut r = Lcg(grow);
                let mut next = case.x.max_row_id().map_or(0, |m| m.0 + 1);
                let mut larger = case.x.clone();
                for table in f.table_map().keys() {
                    let count = r.below(2);
                    larger = larger.with_rows(table.clone(), gen_rows(&mut r, &mut next, count, true)).unwrap();
                }
                for edge in f.relation_map().keys() {
                    let extra: Vec<(RowId, RowId)> = gen_pairs(&mut r, &larger, &edge.src, &edge.tgt, 30)
                        .into_iter()
                        .filter(|&(s, t)| !larger.contains_pair(edge, s, t))
                        .collect();
                    larger = larger.with_foreign_key(edge.clone(), extra).unwrap();
                }
                let inclusion = FInstanceHom::identity(&case.x);
                prop_assert!(inclusion.check(&case.x, &larger).is_ok());
                let small = functor_pi_along(&case.x, &f, 10_000).unwrap();
                let big = functor_pi_along(&larger, &f, 10_000).unwrap();
                let embedded = f_pi_on_hom(&inclusion, &case.x, &larger, &f, 10_000).unwrap();
                prop_assert!(embedded.check(&small, &big).is_ok());
                let images: BTreeSet<RowId> = embedded.row_map.values().copied().collect();
                prop_assert_eq!(images.len(), small.total_row_count(), "Pi(inclusion) is injective");
            }
        }
    }

    mod property {
        use proptest::prelude::*;

        use super::*;

        // -- W-type property tests (injective total maps) ----------------

        /// A generated W-type scenario over an injective (renaming or
        /// identity) total map: an `S`-instance tree and the schemas /
        /// migration relabelling it.
        #[derive(Debug, Clone)]
        struct WScenario {
            x: WInstance,
            tgt_schema: Schema,
            migration: CompiledMigration,
        }

        /// Generate a rooted tree of `n` leaf children under one root, plus
        /// an injective relabelling of its two anchors (identity or rename).
        fn arb_w_scenario() -> impl Strategy<Value = WScenario> {
            (1usize..=4, any::<bool>()).prop_map(|(n, rename)| {
                let (s_root, s_leaf, t_root, t_leaf) = if rename {
                    ("root", "leaf", "box", "item")
                } else {
                    ("root", "leaf", "root", "leaf")
                };

                let mut nodes = HashMap::new();
                nodes.insert(0, Node::new(0, s_root));
                let mut arcs = Vec::new();
                for i in 0..n {
                    let id = u32::try_from(i + 1).unwrap();
                    nodes.insert(
                        id,
                        Node::new(id, s_leaf)
                            .with_value(FieldPresence::Present(Value::Int(i64::from(id)))),
                    );
                    arcs.push((0, id, edge(s_root, s_leaf, "child")));
                }
                let x = WInstance::new(nodes, arcs, vec![], 0, Name::from(s_root));

                let mut vertex_remap = HashMap::new();
                vertex_remap.insert(Name::from(s_root), Name::from(t_root));
                vertex_remap.insert(Name::from(s_leaf), Name::from(t_leaf));
                let mut edge_remap = HashMap::new();
                edge_remap.insert(edge(s_root, s_leaf, "child"), edge(t_root, t_leaf, "child"));

                let migration = CompiledMigration {
                    surviving_verts: HashSet::from([Name::from(t_root), Name::from(t_leaf)]),
                    surviving_edges: HashSet::new(),
                    vertex_remap,
                    edge_remap,
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms: HashMap::new(),
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                };
                let tgt_schema = schema_of(&[t_root, t_leaf], &[edge(t_root, t_leaf, "child")]);
                WScenario {
                    x,
                    tgt_schema,
                    migration,
                }
            })
        }

        /// Renumber every node id of `y` by `+shift`, returning the shifted
        /// instance and the isomorphism `y -> shifted` as a node map.
        fn shift_nodes(y: &WInstance, shift: u32) -> (WInstance, WInstanceHom) {
            let nodes = y
                .nodes
                .iter()
                .map(|(&id, node)| {
                    let mut moved = node.clone();
                    moved.id = id + shift;
                    (id + shift, moved)
                })
                .collect();
            let arcs = y
                .arcs
                .iter()
                .map(|(p, c, e)| (p + shift, c + shift, e.clone()))
                .collect();
            let shifted = WInstance::new(
                nodes,
                arcs,
                y.fans.clone(),
                y.root + shift,
                y.schema_root.clone(),
            );
            let hom = WInstanceHom::new(y.nodes.keys().map(|&id| (id, id + shift)).collect());
            (shifted, hom)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            /// Both triangle identities hold for the W-type adjunction over
            /// injective (renamed / identity) total maps.
            #[test]
            fn w_sigma_delta_triangle_identities(scenario in arb_w_scenario()) {
                let WScenario { x, tgt_schema, migration } = scenario;

                let sigma_x = w_sigma(&x, &tgt_schema, &migration).unwrap();
                let delta_sigma_x = w_delta(&sigma_x, &migration).unwrap();
                let unit = w_unit(&x);
                prop_assert!(unit.check(&x, &delta_sigma_x).is_ok());
                // First triangle: phi(eta_X) is the identity on Sigma X.
                let phi_unit = w_transpose_right(&unit);
                prop_assert!(phi_unit.check(&sigma_x, &sigma_x).is_ok());
                prop_assert_eq!(phi_unit, WInstanceHom::identity(&sigma_x));

                // Counit over Y = Sigma X (anchored inside the image of F).
                let y = sigma_x;
                let delta_y = w_delta(&y, &migration).unwrap();
                let sigma_delta_y = w_sigma(&delta_y, &tgt_schema, &migration).unwrap();
                let counit = w_counit(&y);
                prop_assert!(counit.check(&sigma_delta_y, &y).is_ok());
                // Second triangle: psi(eps_Y) is the identity on Delta Y.
                let psi_counit = w_transpose_left(&counit);
                prop_assert!(psi_counit.check(&delta_y, &delta_y).is_ok());
                prop_assert_eq!(psi_counit, WInstanceHom::identity(&delta_y));
            }

            /// The transpose carries valid W-type homomorphisms across the
            /// bijection `Hom_T(Sigma X, Y) ~= Hom_S(X, Delta Y)` and back,
            /// for injective total maps.
            #[test]
            fn w_sigma_delta_hom_bijection(scenario in arb_w_scenario()) {
                let WScenario { x, tgt_schema, migration } = scenario;

                let sigma_x = w_sigma(&x, &tgt_schema, &migration).unwrap();
                // A non-trivial g : Sigma X -> Y, the renumbering iso onto a
                // shifted copy of Sigma X.
                let (y, g) = shift_nodes(&sigma_x, 100);
                prop_assert!(g.check(&sigma_x, &y).is_ok());

                let delta_y = w_delta(&y, &migration).unwrap();
                let psi_g = w_transpose_left(&g);
                prop_assert!(psi_g.check(&x, &delta_y).is_ok());
                // Round-trip phi . psi = id.
                let phi_psi_g = w_transpose_right(&psi_g);
                prop_assert_eq!(phi_psi_g, g);

                // Reverse direction from f = eta_X : X -> Delta(Sigma X).
                let delta_sigma_x = w_delta(&sigma_x, &migration).unwrap();
                let f = w_unit(&x);
                prop_assert!(f.check(&x, &delta_sigma_x).is_ok());
                let phi_f = w_transpose_right(&f);
                prop_assert!(phi_f.check(&sigma_x, &sigma_x).is_ok());
                let psi_phi_f = w_transpose_left(&phi_f);
                prop_assert_eq!(psi_phi_f, f);
            }
        }
    }
}
