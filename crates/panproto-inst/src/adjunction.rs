//! The Sigma/Delta migration adjunction on instances.
//!
//! A total vertex map `F: S -> T` (the vertex remap of a
//! [`CompiledMigration`]) induces two instance transports:
//!
//! - **`Sigma_F`** (left Kan extension / pushforward): an `S`-instance
//!   becomes a `T`-instance by relabelling anchors `v` to `F(v)`. For
//!   set-valued functor instances this is the coproduct over each fibre
//!   `F^{-1}(t)`; for W-type instances it is [`wtype_extend`].
//! - **`Delta_F`** (precomposition / pullback): a `T`-instance becomes an
//!   `S`-instance by reindexing, `(Delta_F Y)(s) = Y(F s)`.
//!
//! `Sigma_F` is left adjoint to `Delta_F`, so their hom-sets correspond:
//!
//! ```text
//! Hom_T(Sigma_F X, Y)  ~=  Hom_S(X, Delta_F Y).
//! ```
//!
//! This module realises the adjunction concretely. It builds the **unit**
//! `eta_X : X -> Delta_F(Sigma_F X)` and **counit**
//! `eps_Y : Sigma_F(Delta_F Y) -> Y` as instance homomorphisms (the
//! morphisms from [`crate::instance_hom`]), the two **transpose** maps
//! that witness the hom-set bijection, and it property-tests the triangle
//! identities and the bijection.
//!
//! # Two shapes, two scopes
//!
//! Set-valued functor instances ([`FInstance`]) are closed under both
//! transports for *every* total vertex map, including vertex-merging
//! (non-injective) maps: precomposition duplicates a merged table into
//! each source vertex, which is a perfectly good table. The functor-side
//! adjunction below therefore holds over renamed, identity, and merging
//! maps.
//!
//! W-type instances ([`WInstance`]) are trees, and precomposition along a
//! merging map would duplicate a node into several anchors at once, which
//! is no longer a tree. The W-type adjunction is thus **scoped to
//! vertex-injective total maps** (renamings and the identity), where
//! `Sigma_F` and `Delta_F` are mutually inverse relabellings that preserve
//! node identity. On that class the adjunction is an adjoint equivalence
//! onto the image of `F`: the unit and counit are identity-on-nodes
//! natural isomorphisms and the transpose is the identity on the node map.
//! Merging maps are rejected by [`w_delta`] with
//! [`AdjunctionError::NonInjectiveVertexMap`]; the functor-side functions
//! handle them.
//!
//! # What `F` is defined on
//!
//! A compiled migration states a vertex's image in one of two ways: it names
//! the image, or it lets the vertex survive under its own name. Both
//! transports read the domain of `F` that way, so a vertex `Sigma_F` carries
//! forward is a vertex `Delta_F` reads back and the unit has a codomain at
//! every table of `X`. An instance carrying a table at a vertex that is
//! neither — a vertex `F` says nothing about — is refused by [`f_sigma`] with
//! [`AdjunctionError::VertexOutsideDomain`] rather than passed through under
//! its own name into an instance the counit could not come back from.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::{InstError, RestrictError};
use crate::functor::{FInstance, allocate_row, row_allocator};
use crate::ginstance::{GEdge, GInstance};
use crate::instance_hom::{FInstanceHom, WInstanceHom};
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

    /// The instance carries a table at a vertex the migration is not defined
    /// on, so `Sigma_F` has nowhere to send its rows.
    #[error(
        "vertex `{0}` is neither remapped nor surviving, so the migration is \
         not defined on it and the pushforward has no image for its rows"
    )]
    VertexOutsideDomain(Name),

    /// The underlying left Kan extension ([`wtype_extend`]) failed.
    #[error("Sigma (left Kan extension) failed: {0}")]
    Sigma(#[from] RestrictError),

    /// A transport that copies rows found no row id free above the largest
    /// in use.
    #[error("no row id is free above the largest in use")]
    RowIdsExhausted,

    /// Building the reindexed graph failed.
    #[error("the reindexed graph is malformed: {0}")]
    Graph(InstError),
}

// ---------------------------------------------------------------------------
// Shared vertex / edge map helpers
// ---------------------------------------------------------------------------

/// The image of a vertex under the migration's vertex map, defaulting to
/// the identity for vertices the map leaves untouched.
fn map_vertex(migration: &CompiledMigration, vertex: &Name) -> Name {
    migration
        .vertex_remap
        .get(vertex)
        .cloned()
        .unwrap_or_else(|| vertex.clone())
}

/// The image of an edge under the migration's edge map, defaulting to the
/// edge relabelled by the vertex map when no explicit remap is recorded.
fn map_edge(migration: &CompiledMigration, edge: &Edge) -> Edge {
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
/// This is the same reading of a compiled migration that [`wtype_extend`] and
/// its fiber map take, so `Sigma_F` and `Delta_F` agree on which vertices they
/// travel between.
fn source_vertices(migration: &CompiledMigration) -> BTreeSet<Name> {
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
/// [`source_vertices`] reads vertices.
fn source_edges(migration: &CompiledMigration) -> BTreeSet<Edge> {
    let images: HashSet<&Edge> = migration.edge_remap.values().collect();
    let mut domain: BTreeSet<Edge> = migration.edge_remap.keys().cloned().collect();
    for edge in &migration.surviving_edges {
        if !migration.edge_remap.contains_key(edge) && !images.contains(edge) {
            domain.insert(edge.clone());
        }
    }
    domain
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
// Functor (set-valued) adjunction (all total maps, including merging)
// ---------------------------------------------------------------------------

/// Check that every table of `x` sits at a vertex the migration is defined
/// on.
///
/// `Sigma_F` is a transport along `F`, so a table at a vertex outside `F`'s
/// domain has no image to travel to. Carrying it across under its own name
/// would put a table in `Sigma_F X` that `Delta_F` never reads back, and the
/// unit at that vertex would have no codomain.
fn check_domain(x: &FInstance, migration: &CompiledMigration) -> Result<(), AdjunctionError> {
    let domain = source_vertices(migration);
    x.table_names()
        .into_iter()
        .find(|vertex| !domain.contains(*vertex))
        .map_or(Ok(()), |vertex| {
            Err(AdjunctionError::VertexOutsideDomain(vertex.clone()))
        })
}

/// `Sigma_F` for set-valued functor instances: the left Kan extension.
///
/// For each target vertex `t` it forms the coproduct of the source tables in
/// its fibre `F^{-1}(t)`. Row ids are unique across an instance, so the
/// coproduct is the disjoint union of the fibre's rows under their own ids,
/// and each foreign-key pair travels to the image edge unchanged.
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] when `x` carries a table
/// at a vertex the migration neither remaps nor lets survive, so `F` names no
/// image for its rows.
pub fn f_sigma(x: &FInstance, migration: &CompiledMigration) -> Result<FInstance, AdjunctionError> {
    check_domain(x, migration)?;
    let mut sigma = FInstance::new();
    for (source, table) in x.tables() {
        let target = map_vertex(migration, source);
        for (id, row) in table.rows() {
            sigma.place_row(target.clone(), id, row.clone());
        }
    }
    for (edge, pairs) in x.foreign_keys() {
        let target_edge = map_edge(migration, edge);
        for &(s, t) in pairs {
            if sigma.joins(&target_edge, s, t) {
                sigma.place_pair(target_edge.clone(), s, t);
            }
        }
    }
    Ok(sigma)
}

/// `Delta_F` for set-valued functor instances: precomposition.
///
/// The first projection of [`f_delta_with_hom`]; see there.
///
/// # Errors
///
/// As [`f_delta_with_hom`].
pub fn f_delta(y: &FInstance, migration: &CompiledMigration) -> Result<FInstance, AdjunctionError> {
    f_delta_with_hom(y, migration).map(|(delta, _)| delta)
}

/// `Delta_F` for set-valued functor instances, with the row map from each
/// copy to the row of `y` it copies.
///
/// Each source vertex `s` receives a copy of the target table `F(s)`, and
/// each source edge `e` a copy of the target foreign key `F(e)` between the
/// copies of its rows. This is total and stays within the functor category
/// even for merging maps, where a merged table is copied into each source
/// vertex of its fibre.
///
/// Row ids: the copies are made in ascending source vertex name and, within
/// one source vertex, in ascending target row id. The first copy of a target
/// row keeps its id, so over an injective fibre every row keeps its id; every
/// later copy takes a fresh id, allocated from the least id above every row
/// of `y`. The returned [`FInstanceHom`] sends each row of the result to the
/// row of `y` it copies, along `F`: its table map sends each source vertex
/// `s` to `F(s)` and its edge map each source edge `e` to `F(e)`.
///
/// The source vertices are the whole domain of `F` — the vertices the
/// migration remaps together with those it lets survive under their own name
/// — so a vertex that travels through `Sigma_F` travels back through
/// `Delta_F`.
///
/// # Errors
///
/// Returns [`AdjunctionError::RowIdsExhausted`] when the fresh row ids run
/// out.
pub fn f_delta_with_hom(
    y: &FInstance,
    migration: &CompiledMigration,
) -> Result<(FInstance, FInstanceHom), AdjunctionError> {
    let mut allocator = row_allocator(y.row_ids());
    let mut delta = FInstance::new();
    let mut copy_of: HashMap<(Name, RowId), RowId> = HashMap::new();
    let mut to_original: BTreeMap<RowId, RowId> = BTreeMap::new();
    let mut copied: HashSet<RowId> = HashSet::new();
    for source in source_vertices(migration) {
        let target = map_vertex(migration, &source);
        for (original, row) in y.rows(&target) {
            let id = if copied.insert(original) {
                original
            } else {
                allocate_row(&mut allocator).map_err(|_| AdjunctionError::RowIdsExhausted)?
            };
            delta.place_row(source.clone(), id, row.clone());
            copy_of.insert((source.clone(), original), id);
            to_original.insert(id, original);
        }
    }

    for source_edge in source_edges(migration) {
        let target_edge = map_edge(migration, &source_edge);
        for &(s, t) in y.pairs(&target_edge) {
            let (Some(&cs), Some(&ct)) = (
                copy_of.get(&(source_edge.src.clone(), s)),
                copy_of.get(&(source_edge.tgt.clone(), t)),
            ) else {
                continue;
            };
            delta.place_pair(source_edge.clone(), cs, ct);
        }
    }

    let table_map = source_vertices(migration)
        .into_iter()
        .map(|source| {
            let target = map_vertex(migration, &source);
            (source, target)
        })
        .collect();
    let edge_map = source_edges(migration)
        .into_iter()
        .map(|source| {
            let target = map_edge(migration, &source);
            (source, target)
        })
        .collect();
    let hom = FInstanceHom::new(to_original)
        .with_table_map(table_map)
        .with_edge_map(edge_map);
    Ok((delta, hom))
}

/// Where each row of `Y` is copied in `Delta_F Y`: `(source vertex, row of
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

/// The unit `eta_X : X -> Delta_F(Sigma_F X)` of the functor adjunction.
///
/// `Sigma_F` keeps every row's id, so row `r` of `X.table(s)` maps to the
/// copy of `r` that `Delta_F` places at `s`, read off the row map of
/// [`f_delta_with_hom`].
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] when `x` carries a table
/// the migration is not defined on, exactly as [`f_sigma`] does, and
/// [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`] does.
pub fn f_unit(
    x: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstanceHom, AdjunctionError> {
    let sigma = f_sigma(x, migration)?;
    let (delta, to_original) = f_delta_with_hom(&sigma, migration)?;
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

/// The counit `eps_Y : Sigma_F(Delta_F Y) -> Y` of the functor adjunction.
///
/// `Sigma_F(Delta_F Y).table(t)` holds one copy of `Y.table(t)` per vertex
/// of the fibre `F^{-1}(t)`, under the copies' own ids; the counit sends each
/// copy back to the row it copies.
///
/// # Errors
///
/// Returns [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`]
/// does.
pub fn f_counit(
    y: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstanceHom, AdjunctionError> {
    let (_, to_original) = f_delta_with_hom(y, migration)?;
    Ok(FInstanceHom::new(to_original.row_map))
}

/// Transpose `Hom_T(Sigma_F X, Y) -> Hom_S(X, Delta_F Y)`, sending a
/// homomorphism `g` to `Delta_F(g) . eta_X`.
///
/// Concretely, row `r` of `X.table(s)` maps to the copy at `s` of `g(r)`
/// (`Sigma_F` keeps `r`'s id).
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] when `x` carries a table
/// the migration is not defined on, exactly as [`f_sigma`] does, and
/// [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`] does.
pub fn f_transpose_left(
    g: &FInstanceHom,
    x: &FInstance,
    y: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstanceHom, AdjunctionError> {
    check_domain(x, migration)?;
    let (delta, to_original) = f_delta_with_hom(y, migration)?;
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

/// Transpose `Hom_S(X, Delta_F Y) -> Hom_T(Sigma_F X, Y)`, sending a
/// homomorphism `f` to `eps_Y . Sigma_F(f)`, the inverse of
/// [`f_transpose_left`].
///
/// Concretely, row `r` of `Sigma_F X` (row `r` of `X`, under its own id)
/// maps to the row of `Y` that `f(r)` copies.
///
/// # Errors
///
/// Returns [`AdjunctionError::VertexOutsideDomain`] when `x` carries a table
/// the migration is not defined on, exactly as [`f_sigma`] does, and
/// [`AdjunctionError::RowIdsExhausted`] as [`f_delta_with_hom`] does.
pub fn f_transpose_right(
    f: &FInstanceHom,
    x: &FInstance,
    y: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstanceHom, AdjunctionError> {
    check_domain(x, migration)?;
    let (_, to_original) = f_delta_with_hom(y, migration)?;
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

    fn row(v: i64) -> HashMap<String, Value> {
        HashMap::from([("v".to_owned(), Value::Int(v))])
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

    #[test]
    fn f_sigma_merges_fibres_into_coproduct() {
        let m = merge_migration();
        let x = FInstance::new()
            .with_table("a", vec![row(1), row(2)])
            .and_then(|f| f.with_table("b", vec![row(3)]))
            .and_then(|f| f.with_table("c", vec![row(9)]))
            .unwrap();
        let sigma = f_sigma(&x, &m).expect("sigma");
        // a (2 rows) and b (1 row) merge into t (3 rows); c becomes u; every
        // row keeps its id.
        assert_eq!(
            sigma.table("t").map(Table::ids),
            Some(vec![RowId(0), RowId(1), RowId(2)])
        );
        assert_eq!(sigma.table("u").map(Table::ids), Some(vec![RowId(3)]));
    }

    #[test]
    fn f_unit_and_counit_check_over_merge() {
        let m = merge_migration();
        // a holds rows 0 and 1, b row 2, c row 3.
        let x = FInstance::new()
            .with_table("a", vec![row(1), row(2)])
            .and_then(|f| f.with_table("b", vec![row(3)]))
            .and_then(|f| f.with_table("c", vec![row(9)]))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("a", "c", "ea"),
                    [(RowId(0), RowId(3)), (RowId(1), RowId(3))],
                )
            })
            .and_then(|f| f.with_foreign_key(edge("b", "c", "eb"), [(RowId(2), RowId(3))]))
            .unwrap();

        let sigma = f_sigma(&x, &m).expect("sigma");
        let delta_sigma = f_delta(&sigma, &m).expect("delta");
        let unit = f_unit(&x, &m).expect("unit");
        unit.check(&x, &delta_sigma)
            .expect("unit is a valid homomorphism into Delta(Sigma X)");

        // First triangle identity: phi(eta_X) = id on Sigma X.
        let phi_unit = f_transpose_right(&unit, &x, &sigma, &m).expect("transpose");
        assert_eq!(phi_unit, FInstanceHom::identity(&sigma));

        // t holds rows 0 and 1, u row 2.
        let y = FInstance::new()
            .with_table("t", vec![row(5), row(6)])
            .and_then(|f| f.with_table("u", vec![row(7)]))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("t", "u", "e"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .unwrap();
        let delta_y = f_delta(&y, &m).expect("delta");
        let sigma_delta_y = f_sigma(&delta_y, &m).expect("sigma");
        let counit = f_counit(&y, &m).expect("counit");
        counit
            .check(&sigma_delta_y, &y)
            .expect("counit is a valid homomorphism onto Y");

        // Second triangle identity: psi(eps_Y) = id on Delta Y.
        let psi_counit = f_transpose_left(&counit, &delta_y, &y, &m).expect("transpose");
        assert_eq!(psi_counit, FInstanceHom::identity(&delta_y));
    }

    // -- a concrete injective example (W-type side) ----------------------

    /// A rename migration: `root -> box`, `leaf -> item`.
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

    /// `a` holds rows 0 and 1, `c` row 2; both `a` rows reference it.
    fn omission_source() -> FInstance {
        FInstance::new()
            .with_table("a", vec![row(1), row(2)])
            .and_then(|f| f.with_table("c", vec![row(9)]))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("a", "c", "ea"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .unwrap()
    }

    #[test]
    fn f_delta_reindexes_the_vertices_surviving_by_omission() {
        let m = omission_migration();
        let x = omission_source();
        let sigma = f_sigma(&x, &m).expect("every table of X is anchored in the domain");
        let delta_sigma = f_delta(&sigma, &m).expect("delta");
        assert_eq!(
            delta_sigma.row_count("c"),
            1,
            "a vertex surviving under its own name is reindexed like any other",
        );
        let unit = f_unit(&x, &m).expect("the unit is defined on X");
        unit.check(&x, &delta_sigma)
            .expect("unit is a valid homomorphism into Delta(Sigma X)");
    }

    #[test]
    fn f_transposes_invert_each_other_over_omission() {
        let m = omission_migration();
        let x = omission_source();
        let sigma = f_sigma(&x, &m).expect("sigma");
        let unit = f_unit(&x, &m).expect("unit");
        let phi_unit = f_transpose_right(&unit, &x, &sigma, &m).expect("transpose");
        assert_eq!(phi_unit, FInstanceHom::identity(&sigma));
    }

    #[test]
    fn f_sigma_refuses_a_table_outside_the_domain() {
        let m = omission_migration();
        let x = omission_source().with_table("stray", vec![row(4)]).unwrap();
        let err = f_sigma(&x, &m).expect_err("`stray` is neither remapped nor surviving");
        assert!(matches!(err, AdjunctionError::VertexOutsideDomain(v) if v == "stray"));
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
    fn f_delta_allocates_copies_in_source_name_then_row_order() {
        let m = merge_migration();
        // t holds rows 0 and 1, u row 2.
        let y = FInstance::new()
            .with_table("t", vec![row(5), row(6)])
            .and_then(|f| f.with_table("u", vec![row(7)]))
            .unwrap();
        let (delta, to_original) = f_delta_with_hom(&y, &m).unwrap();
        // The domain is a, b, c in name order. `a` takes the first copy of
        // t's rows and keeps their ids; `b` takes the second copy under fresh
        // ids from 3 up, in t's row order; `c` is an injective fibre and
        // keeps u's id.
        assert_eq!(
            delta.table("a").map(Table::ids),
            Some(vec![RowId(0), RowId(1)])
        );
        assert_eq!(
            delta.table("b").map(Table::ids),
            Some(vec![RowId(3), RowId(4)])
        );
        assert_eq!(delta.table("c").map(Table::ids), Some(vec![RowId(2)]));
        assert_eq!(
            to_original.row_map,
            BTreeMap::from([
                (RowId(0), RowId(0)),
                (RowId(1), RowId(1)),
                (RowId(2), RowId(2)),
                (RowId(3), RowId(0)),
                (RowId(4), RowId(1)),
            ])
        );
        assert_eq!(delta.row(RowId(4)), y.row(RowId(1)));
    }

    #[test]
    fn a_cross_schema_row_map_lands_in_the_image_table() {
        let m = merge_migration();
        let y = FInstance::new()
            .with_table("t", vec![row(5), row(6)])
            .and_then(|f| f.with_table("u", vec![row(7)]))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("t", "u", "e"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .unwrap();
        // Delta's row map sends the copies at `a`, `b` and `c` into the
        // tables `t` and `u` of `y`, the images of their own tables.
        let (delta, to_original) = f_delta_with_hom(&y, &m).unwrap();
        to_original
            .check_tables(&delta, &y)
            .expect("each copy lands in the image of its own table");
        to_original
            .check(&delta, &y)
            .expect("Delta's row map is a homomorphism along F");
        FInstanceHom::from_row_map_along(
            to_original.row_map.clone(),
            to_original.table_map.clone(),
            &delta,
            &y,
        )
        .expect("the same map decodes along F");

        // A copy at `a` sent to the row of `u` lands outside `F(a) = t`.
        let wrong_copy = delta.rows(&Name::from("a"))[0].0;
        let mut wrong = to_original.row_map.clone();
        wrong.insert(wrong_copy, RowId(2));
        let err = FInstanceHom::from_row_map_along(wrong, to_original.table_map, &delta, &y)
            .expect_err("a row outside its image table is refused");
        assert!(matches!(err, InstError::HomTableMismatch { row } if row == wrong_copy));

        // Pi's projections send the rows of `t` into `a` and `b`.
        let x = FInstance::new()
            .with_table("a", vec![row(1)])
            .and_then(|f| f.with_table("b", vec![row(1)]))
            .unwrap();
        let mut pi_migration = CompiledMigration::default();
        pi_migration.surviving_verts.insert(Name::from("t"));
        for source in ["a", "b"] {
            pi_migration
                .vertex_remap
                .insert(Name::from(source), Name::from("t"));
        }
        let (pi, projections) = crate::pi::functor_pi_with_hom(
            &x,
            &Schema::default(),
            &Schema::default(),
            &pi_migration,
            16,
        )
        .unwrap();
        for source in ["a", "b"] {
            let projection = &projections[&Name::from(source)];
            projection
                .check_tables(&pi, &x)
                .expect("a projection lands in its own fibre vertex");
            projection
                .check(&pi, &x)
                .expect("a projection whose components agree is a homomorphism");
        }
    }

    #[test]
    fn every_row_creating_operation_returns_its_row_map() {
        let m = merge_migration();
        let y = FInstance::new()
            .with_table("t", vec![row(5), row(6)])
            .and_then(|f| f.with_table("u", vec![row(7)]))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("t", "u", "e"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .unwrap();
        // Delta copies rows: its row map sends every copy to the row it
        // copies, cells and foreign keys included.
        let (delta, to_original) = f_delta_with_hom(&y, &m).unwrap();
        assert_eq!(to_original.row_map.len(), delta.total_row_count());
        for (copy, original) in &to_original.row_map {
            assert_eq!(delta.row(*copy), y.row(*original));
        }
        for (edge, s, t) in delta.all_pairs() {
            let image = map_edge(&m, &edge);
            assert!(y.contains_pair(
                &image,
                to_original.image(s).unwrap(),
                to_original.image(t).unwrap()
            ));
        }
        // The counit is that row map, read on Sigma(Delta Y), whose tables
        // already sit at the targets.
        assert_eq!(f_counit(&y, &m).unwrap().row_map, to_original.row_map);

        // Pi creates product rows: its projections send every row to the
        // component it drew from each fibre vertex.
        let x = FInstance::new()
            .with_table("a", vec![row(1), row(2)])
            .and_then(|f| f.with_table("b", vec![HashMap::from([("w".to_owned(), Value::Int(3))])]))
            .unwrap();
        let mut pi_migration = CompiledMigration::default();
        pi_migration.surviving_verts.insert(Name::from("t"));
        pi_migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        pi_migration
            .vertex_remap
            .insert(Name::from("b"), Name::from("t"));
        let (pi, projections) = crate::pi::functor_pi_with_hom(
            &x,
            &Schema::default(),
            &Schema::default(),
            &pi_migration,
            16,
        )
        .unwrap();
        for source in ["a", "b"] {
            let projection = &projections[&Name::from(source)];
            assert_eq!(projection.row_map.len(), pi.total_row_count());
            for (product_row, component) in &projection.row_map {
                let product = pi.row(*product_row).unwrap();
                for (column, value) in x.row(*component).unwrap() {
                    assert_eq!(product.get(column), Some(value));
                }
            }
        }
    }

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
        let (table_then_graph, _) = GInstance::from_finstance(&f_delta(&y, &m).unwrap()).unwrap();
        let (graph, _) = GInstance::from_finstance(&y).unwrap();
        let graph_then_delta = g_delta(&graph, &m).unwrap();
        assert_eq!(graph_then_delta.edge_count(), 1);
        assert!(
            GInstance::identical(&graph_then_delta, &table_then_graph),
            "{graph_then_delta:?} != {table_then_graph:?}"
        );
    }

    // -- property tests --------------------------------------------------

    mod property {
        use proptest::prelude::*;

        use super::*;

        /// A generated functor-side scenario: an `S`-instance `x`, a
        /// `T`-instance `y`, and a total vertex map (possibly merging).
        #[derive(Debug, Clone)]
        struct FScenario {
            x: FInstance,
            y: FInstance,
            migration: CompiledMigration,
        }

        /// Round-robin foreign-key pairs from the rows of `src` into the rows
        /// of `tgt`: the `i`th source row to the `i mod n`th target row.
        fn round_robin(f: &FInstance, src: &str, tgt: &str) -> Vec<(RowId, RowId)> {
            let sources = f.table(src).map(Table::ids).unwrap_or_default();
            let targets = f.table(tgt).map(Table::ids).unwrap_or_default();
            if targets.is_empty() {
                return Vec::new();
            }
            sources
                .iter()
                .enumerate()
                .map(|(i, s)| (*s, targets[i % targets.len()]))
                .collect()
        }

        /// Generate a scenario with `n` source vertices mapped onto `k`
        /// target vertices (merging whenever two share an image), one
        /// source edge per adjacent source-vertex pair, and small tables.
        fn arb_scenario() -> impl Strategy<Value = FScenario> {
            (2usize..=4, 1usize..=4).prop_flat_map(|(n, k)| {
                let k = k.min(n);
                // Assignment of each source vertex to a target index.
                let assign = prop::collection::vec(0..k, n);
                // Row counts for source and target tables.
                let src_rows = prop::collection::vec(0usize..=3, n);
                let tgt_rows = prop::collection::vec(0usize..=3, k);
                (Just(n), Just(k), assign, src_rows, tgt_rows).prop_map(
                    |(n, k, assign, src_rows, tgt_rows)| {
                        let s_name = |i: usize| format!("s{i}");
                        let t_name = |j: usize| format!("t{j}");

                        // Vertex + edge maps.
                        let mut vertex_remap = HashMap::new();
                        for (i, &a) in assign.iter().enumerate() {
                            vertex_remap.insert(Name::from(s_name(i)), Name::from(t_name(a)));
                        }
                        // One edge s{i} -> s{i+1} per adjacent pair.
                        let s_edges: Vec<Edge> = (0..n.saturating_sub(1))
                            .map(|i| edge(&s_name(i), &s_name(i + 1), &format!("e{i}")))
                            .collect();
                        let mut edge_remap = HashMap::new();
                        for (i, e) in s_edges.iter().enumerate() {
                            edge_remap.insert(
                                e.clone(),
                                edge(
                                    &t_name(assign[i]),
                                    &t_name(assign[i + 1]),
                                    &format!("te{i}"),
                                ),
                            );
                        }
                        let surviving_verts: HashSet<Name> =
                            (0..k).map(|j| Name::from(t_name(j))).collect();

                        let migration = CompiledMigration {
                            surviving_verts,
                            surviving_edges: HashSet::new(),
                            vertex_remap,
                            edge_remap: edge_remap.clone(),
                            resolver: HashMap::new(),
                            hyper_resolver: HashMap::new(),
                            field_transforms: HashMap::new(),
                            conditional_survival: HashMap::new(),
                            op_term_assignments: HashMap::new(),
                            expansion_path: HashMap::new(),
                        };

                        // Source instance X.
                        let mut counter = 0i64;
                        let mut x = FInstance::new();
                        for (i, &count) in src_rows.iter().enumerate() {
                            let rows: Vec<_> = (0..count)
                                .map(|_| {
                                    counter += 1;
                                    row(counter)
                                })
                                .collect();
                            x = x.with_table(s_name(i), rows).expect("fresh ids");
                        }
                        for (i, e) in s_edges.iter().enumerate() {
                            let pairs = round_robin(&x, &s_name(i), &s_name(i + 1));
                            x = x
                                .with_foreign_key(e.clone(), pairs)
                                .expect("round robin pairs join");
                        }

                        // Target instance Y (independent).
                        let mut y = FInstance::new();
                        for (j, &count) in tgt_rows.iter().enumerate() {
                            let rows: Vec<_> = (0..count)
                                .map(|_| {
                                    counter += 1;
                                    row(counter)
                                })
                                .collect();
                            y = y.with_table(t_name(j), rows).expect("fresh ids");
                        }
                        for (i, _e) in s_edges.iter().enumerate() {
                            let te = edge(
                                &t_name(assign[i]),
                                &t_name(assign[i + 1]),
                                &format!("te{i}"),
                            );
                            let pairs = round_robin(&y, &t_name(assign[i]), &t_name(assign[i + 1]));
                            y = y
                                .with_foreign_key(te, pairs)
                                .expect("round robin pairs join");
                        }

                        FScenario { x, y, migration }
                    },
                )
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            /// Both triangle identities hold for the functor adjunction over
            /// renamed, identity, and vertex-merging total maps.
            #[test]
            fn sigma_delta_triangle_identities(scenario in arb_scenario()) {
                let FScenario { x, y, migration } = scenario;

                // Unit and its triangle: phi(eta_X) = id on Sigma X.
                let sigma_x = f_sigma(&x, &migration).expect("the generated instance is anchored in the migration's domain");
                let delta_sigma_x = f_delta(&sigma_x, &migration).expect("ids are small");
                let unit = f_unit(&x, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert!(unit.check(&x, &delta_sigma_x).is_ok());
                let phi_unit = f_transpose_right(&unit, &x, &sigma_x, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert_eq!(phi_unit, FInstanceHom::identity(&sigma_x));

                // Counit and its triangle: psi(eps_Y) = id on Delta Y.
                let delta_y = f_delta(&y, &migration).expect("ids are small");
                let sigma_delta_y = f_sigma(&delta_y, &migration).expect("the generated instance is anchored in the migration's domain");
                let counit = f_counit(&y, &migration).expect("ids are small");
                prop_assert!(counit.check(&sigma_delta_y, &y).is_ok());
                let psi_counit = f_transpose_left(&counit, &delta_y, &y, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert_eq!(psi_counit, FInstanceHom::identity(&delta_y));
            }

            /// The transpose maps are mutually inverse and carry valid
            /// homomorphisms to valid homomorphisms, both directions of
            /// `Hom_T(Sigma X, Y) ~= Hom_S(X, Delta Y)`.
            #[test]
            fn sigma_delta_hom_bijection(scenario in arb_scenario()) {
                let FScenario { x, y, migration } = scenario;

                // Direction 1. A valid g : Sigma X -> Y_g, built as the
                // inclusion of Sigma X into a copy of itself with every
                // table's rows (and foreign keys) duplicated once.
                let sigma_x = f_sigma(&x, &migration).expect("the generated instance is anchored in the migration's domain");
                let mut y_g = sigma_x.clone();
                let mut duplicate: HashMap<RowId, RowId> = HashMap::new();
                for id in sigma_x.row_ids() {
                    let table = sigma_x.table_of(id).expect("a row of Sigma X").clone();
                    let copy = y_g
                        .push_row(table, sigma_x.row(id).expect("a row of Sigma X").clone())
                        .expect("ids are small");
                    duplicate.insert(id, copy);
                }
                for (e, s, t) in sigma_x.all_pairs() {
                    y_g.insert_foreign_key(e, duplicate[&s], duplicate[&t]).expect("the copies join");
                }
                let g = FInstanceHom::identity(&sigma_x);
                prop_assert!(g.check(&sigma_x, &y_g).is_ok());

                let delta_y_g = f_delta(&y_g, &migration).expect("ids are small");
                let psi_g = f_transpose_left(&g, &x, &y_g, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert!(psi_g.check(&x, &delta_y_g).is_ok());
                // Round-trip phi . psi = id.
                let phi_psi_g = f_transpose_right(&psi_g, &x, &y_g, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert_eq!(phi_psi_g, g);

                // Direction 2. A valid f : X_f -> Delta Y with X_f = Delta Y
                // and f the identity; phi(f) must be a valid Sigma X_f -> Y.
                let delta_y = f_delta(&y, &migration).expect("ids are small");
                let f = FInstanceHom::identity(&delta_y);
                prop_assert!(f.check(&delta_y, &delta_y).is_ok());
                let phi_f = f_transpose_right(&f, &delta_y, &y, &migration).expect("the generated instance is anchored in the migration's domain");
                let sigma_delta_y = f_sigma(&delta_y, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert!(phi_f.check(&sigma_delta_y, &y).is_ok());
                // Round-trip psi . phi = id.
                let psi_phi_f = f_transpose_left(&phi_f, &delta_y, &y, &migration).expect("the generated instance is anchored in the migration's domain");
                prop_assert_eq!(psi_phi_f, f);
            }
        }

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
