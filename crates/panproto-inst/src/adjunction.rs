//! The migration adjunctions on instances: `Sigma_F -| Delta_F -| Pi_F`.
//!
//! For table instances ([`FInstance`]) the functor is a [`MigrationFunctor`]
//! `F: S' -> T` between attributed relation-ified schemas
//! ([`crate::RelSchema`]). A table instance is a presheaf on the entity
//! side (rows at tables, pairs at relations) together with total cells in
//! a free, discrete value side whose generators are labelled nulls, and a
//! homomorphism keeps rows in their tables, pairs as pairs and cells up to
//! a map on nulls that fixes every constant. `F` is the identity on values.
//! Three transports follow:
//!
//! - **`Delta_F`** (precomposition, [`f_delta`]): a copy of each row of
//!   `J(F s)` at each kept table `s`, its cell at `alpha` being `J`'s at
//!   `F(alpha)`.
//! - **`Sigma_F`** (left Kan extension, [`f_sigma`]): the disjoint union of
//!   each fibre's rows, a fresh labelled null at every column outside the
//!   image of the column map, and the equations of a non-injective column
//!   map solved by union-find.
//! - **`Pi_F`** (right Kan extension, [`crate::functor_pi_along`]): at each
//!   target table the natural join of its fibre on the columns they share,
//!   defined exactly along a functor surjective on attributes.
//!
//! so that, on the domains where each is defined (Sigma not refusing a
//! clash of constants; `F` surjective on attributes for Pi),
//!
//! ```text
//! Hom_T(Sigma_F I, J)  ~=  Hom_S'(I, Delta_F J)
//! Hom_S'(Delta_F J, I)  ~=  Hom_T(J, Pi_F I).
//! ```
//!
//! This module builds the units and counits of both adjunctions as instance
//! homomorphisms ([`crate::instance_hom`]), the transposes that witness the
//! hom-set bijections, and the action of each transport on homomorphisms,
//! and its tests check the bijections, triangle identities, identity
//! functor, renaming invariance, monotonicity, functoriality and
//! conformance against brute-force oracles. Values are compared by their
//! canonical bytes throughout, so a float equals another exactly when their
//! bit patterns are equal.
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
use crate::functor::{FInstance, Row, allocate_row, row_allocator, same_value};
use crate::ginstance::{GEdge, GInstance};
use crate::instance_hom::{FInstanceHom, WInstanceHom};
use crate::migration_functor::MigrationFunctor;
use crate::pi::{PiLimit, pi_limit};
use crate::rel_schema::RelSchema;
use crate::value::Value;
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

    /// A map given as a homomorphism sends a row to no row of the transport
    /// it must land in, so it is no homomorphism between the instances
    /// given.
    #[error("row {row:?} has no image in the transport the map must land in")]
    NotANaturalMap {
        /// The row without an image.
        row: RowId,
    },

    /// The equations `Sigma_F` solves for a column map that sends two
    /// columns of one table to one column join two different constants
    /// (compared by canonical bytes), so no instance satisfies them.
    #[error(
        "Sigma equates the constants {left:?} and {right:?} in column `{column}` of row {row:?}"
    )]
    SigmaConstantClash {
        /// The row of the first equation that joins them.
        row: RowId,
        /// The target column of that equation.
        column: String,
        /// The constant of the class the equation's first value lies in.
        left: Box<Value>,
        /// The constant of the other class.
        right: Box<Value>,
    },

    /// A fresh labelled null found no id left above the largest in use.
    #[error("no labelled null id is free above the largest in use")]
    NullIdsExhausted,

    /// An input does not conform to the schema it must be an instance of:
    /// a row outside its tables, a missing or stray cell, a labelled null
    /// nested inside a cell, or a pair outside its relations.
    #[error("row {row:?} does not conform to the schema (column {column:?}): {reason}")]
    NonConforming {
        /// The least offending row (for a pair, its source row).
        row: RowId,
        /// The offending column, when the violation is at a cell.
        column: Option<String>,
        /// What is wrong.
        reason: NonConformance,
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

/// Why a row of a table instance does not conform to a schema.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NonConformance {
    /// The row lies at a vertex that is no table of the schema.
    TableOutside {
        /// The vertex.
        table: Name,
    },
    /// The row holds no cell at a column of its table (cells are total: an
    /// absent cell and a [`Value::Null`] one differ).
    MissingCell,
    /// The row holds a cell at a key that is no column of its table.
    StrayCell,
    /// The cell holds a labelled null below its top level, which the free
    /// value side has no term for.
    NestedLabeledNull,
    /// A pair whose source is the row lies on an edge that is no relation of
    /// the schema.
    RelationOutside {
        /// The edge (boxed to keep the reason small).
        edge: Box<Edge>,
    },
}

impl std::fmt::Display for NonConformance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TableOutside { table } => write!(f, "`{table}` is no table of the schema"),
            Self::MissingCell => f.write_str("the row holds no cell at a column of its table"),
            Self::StrayCell => f.write_str("the row holds a cell at no column of its table"),
            Self::NestedLabeledNull => f.write_str("the cell holds a labelled null inside a value"),
            Self::RelationOutside { edge } => {
                write!(
                    f,
                    "a pair lies on {edge:?}, which is no relation of the schema"
                )
            }
        }
    }
}

/// Where a `Sigma_F` result's labelled nulls come from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SigmaProvenance {
    /// Each fresh null with the row and target column it fills: a column of
    /// the row's table outside the image of the column map.
    pub fresh: BTreeMap<u64, (RowId, String)>,
    /// Each null of the input that the equations of a non-injective column
    /// map equate with another value, with the value that represents its
    /// class in the result (a constant, or the least null of the class).
    pub merged: BTreeMap<u64, Value>,
}

/// Where the rows of a `Delta_F` result come from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeltaProvenance {
    /// Each row of the result with the source table it lies at and the row
    /// of the input it copies.
    pub copies: BTreeMap<RowId, (Name, RowId)>,
}

impl DeltaProvenance {
    /// The copy at `table` of `row`, if the result holds one.
    #[must_use]
    pub fn copy_of(&self, table: &str, row: RowId) -> Option<RowId> {
        self.copies
            .iter()
            .find(|(_, (t, r))| t.as_ref() == table && *r == row)
            .map(|(copy, _)| *copy)
    }
}

/// The scope an instance is checked against: tables with their columns, and
/// relations.
struct Scope<'a> {
    /// Each table with its columns.
    tables: BTreeMap<&'a Name, BTreeSet<&'a str>>,
    /// The relations.
    relations: BTreeSet<&'a Edge>,
}

impl<'a> Scope<'a> {
    /// Every table, column and relation of `rel`.
    fn whole(rel: &'a RelSchema) -> Self {
        Self {
            tables: rel
                .all_columns()
                .iter()
                .map(|(table, columns)| (table, columns.keys().map(String::as_str).collect()))
                .collect(),
            relations: rel.relations().iter().collect(),
        }
    }

    /// The kept part `S'` of the source of `functor`.
    fn domain(functor: &'a MigrationFunctor) -> Self {
        let mut tables: BTreeMap<&Name, BTreeSet<&str>> = functor
            .table_map()
            .keys()
            .map(|table| (table, BTreeSet::new()))
            .collect();
        for (table, column) in functor.column_map().keys() {
            tables.entry(table).or_default().insert(column.as_str());
        }
        Self {
            tables,
            relations: functor.relation_map().keys().collect(),
        }
    }
}

/// Whether a value holds a labelled null below its top level.
fn holds_nested_null(value: &Value) -> bool {
    fn holds_null(value: &Value) -> bool {
        match value {
            Value::LabeledNull(_) => true,
            other => holds_nested_null(other),
        }
    }
    match value {
        Value::List(items) => items.iter().any(holds_null),
        Value::Unknown(fields) | Value::Opaque { fields, .. } => fields.values().any(holds_null),
        _ => false,
    }
}

/// The first way `instance` fails to conform to `scope`, rows ascending:
/// for a row, a table outside the scope, then the least column whose cell is
/// missing or stray, then the least column holding a nested labelled null;
/// for a pair, an edge outside the scope, at its source row. A row's own
/// violation comes before a pair's at the same row.
fn conform(instance: &FInstance, scope: &Scope<'_>) -> Result<(), AdjunctionError> {
    let row_violation = instance.row_ids().into_iter().find_map(|id| {
        let table = instance.table_of(id)?;
        let Some(columns) = scope.tables.get(table) else {
            return Some((
                id,
                None,
                NonConformance::TableOutside {
                    table: table.clone(),
                },
            ));
        };
        let cells = instance.row(id)?;
        let missing = columns
            .iter()
            .find(|column| !cells.contains_key(**column))
            .map(|column| ((*column).to_owned(), NonConformance::MissingCell));
        let stray = cells
            .keys()
            .filter(|key| !columns.contains(key.as_str()))
            .min()
            .map(|key| (key.clone(), NonConformance::StrayCell));
        let keys = match (missing, stray) {
            (Some(m), Some(s)) => Some(if s.0 < m.0 { s } else { m }),
            (m, s) => m.or(s),
        };
        if let Some((column, reason)) = keys {
            return Some((id, Some(column), reason));
        }
        columns
            .iter()
            .find(|column| cells.get(**column).is_some_and(holds_nested_null))
            .map(|column| {
                (
                    id,
                    Some((*column).to_owned()),
                    NonConformance::NestedLabeledNull,
                )
            })
    });
    let mut keys = instance.foreign_keys();
    keys.sort_by(|a, b| a.0.cmp(b.0));
    let pair_violation = keys
        .into_iter()
        .filter(|(edge, pairs)| !pairs.is_empty() && !scope.relations.contains(edge))
        .filter_map(|(edge, pairs)| pairs.iter().next().map(|&(src, _)| (src, edge)))
        .min_by_key(|(src, _)| *src);
    let violation = match (row_violation, pair_violation) {
        (Some(row), Some((src, _))) if row.0 <= src => Some(row),
        (_, Some((src, edge))) => Some((
            src,
            None,
            NonConformance::RelationOutside {
                edge: Box::new(edge.clone()),
            },
        )),
        (row, None) => row,
    };
    violation.map_or(Ok(()), |(row, column, reason)| {
        Err(AdjunctionError::NonConforming {
            row,
            column,
            reason,
        })
    })
}

/// `rho_{S'}(x)`: the rows of `x` at kept tables with their cells at kept
/// columns, and its pairs at kept relations.
fn restrict_to_domain(x: &FInstance, functor: &MigrationFunctor) -> FInstance {
    let mut out = FInstance::new();
    for table in functor.table_map().keys() {
        let kept: BTreeSet<&String> = functor.kept_columns(table).into_iter().collect();
        for (id, row) in x.rows(table) {
            let cells: Row = row
                .iter()
                .filter(|(column, _)| kept.contains(column))
                .map(|(column, value)| (column.clone(), value.clone()))
                .collect();
            out.place_row(table.clone(), id, cells);
        }
    }
    for edge in functor.relation_map().keys() {
        for &(s, t) in x.pairs(edge) {
            out.place_pair(edge.clone(), s, t);
        }
    }
    out
}

/// The part of `x`, an instance on the whole source schema `S`, on the kept
/// part `S'` of `functor`: its rows at kept tables with their cells at kept
/// columns, and its pairs at kept relations. This is the instance on `S'`
/// that [`f_sigma`] and [`crate::functor_pi_along`] extend, and the form
/// the units, counits, transposes and actions on homomorphisms take their
/// source-side instances in.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `x` does not conform to `S`.
pub fn f_restrict_to_domain(
    x: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstance, AdjunctionError> {
    conform(x, &Scope::whole(functor.source()))?;
    Ok(restrict_to_domain(x, functor))
}

/// The labelled nulls in the cells of `instance` (at the top level of a
/// cell; a conforming instance holds none deeper).
fn nulls_of(instance: &FInstance) -> BTreeSet<u64> {
    instance
        .tables()
        .into_iter()
        .flat_map(|(_, table)| table.rows())
        .flat_map(|(_, row)| row.values().filter_map(Value::as_labeled_null))
        .collect()
}

/// The least null id above every null of `instance` (`0` when it holds
/// none), or `None` when no id is above them.
fn null_floor(instance: &FInstance) -> Option<u64> {
    nulls_of(instance)
        .last()
        .map_or(Some(0), |max| max.checked_add(1))
}

/// An atom of the value side: a labelled null, or a constant by its
/// canonical bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Atom {
    /// A labelled null.
    Null(u64),
    /// A constant.
    Const(Vec<u8>),
}

/// The classes of values the equations of `Sigma_F` join, by union-find.
#[derive(Default)]
struct ValueClasses {
    /// The index of each atom met.
    index: BTreeMap<Atom, usize>,
    /// The parent of each class member.
    parent: Vec<usize>,
    /// At each root: its class's constant, if any.
    constant: Vec<Option<Value>>,
    /// At each root: its class's least null, if any.
    least_null: Vec<Option<u64>>,
}

impl ValueClasses {
    /// The atom of a value.
    fn atom(value: &Value) -> Atom {
        match value {
            Value::LabeledNull(n) => Atom::Null(*n),
            other => Atom::Const(other.canonical_bytes()),
        }
    }

    /// The index of `value`'s atom, adding it as a class of its own.
    fn insert(&mut self, value: &Value) -> usize {
        let atom = Self::atom(value);
        if let Some(&i) = self.index.get(&atom) {
            return i;
        }
        let i = self.parent.len();
        self.parent.push(i);
        self.constant.push(match value {
            Value::LabeledNull(_) => None,
            other => Some(other.clone()),
        });
        self.least_null.push(value.as_labeled_null());
        self.index.insert(atom, i);
        i
    }

    /// The root of `i`'s class.
    fn root(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    /// Join the classes of `a` and `b`; on two different constants, return
    /// them (`a`'s first) and leave the classes apart.
    fn union(&mut self, a: &Value, b: &Value) -> Result<(), (Value, Value)> {
        let (a, b) = (self.insert(a), self.insert(b));
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb {
            return Ok(());
        }
        if let (Some(x), Some(y)) = (&self.constant[ra], &self.constant[rb]) {
            return Err((x.clone(), y.clone()));
        }
        self.parent[rb] = ra;
        if self.constant[ra].is_none() {
            self.constant[ra] = self.constant[rb].take();
        }
        self.least_null[ra] = match (self.least_null[ra], self.least_null[rb]) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (x, y) => x.or(y),
        };
        Ok(())
    }

    /// The value that represents `value`'s class: its constant, else its
    /// least null. A value in no class represents itself.
    fn representative(&mut self, value: &Value) -> Value {
        let Some(&i) = self.index.get(&Self::atom(value)) else {
            return value.clone();
        };
        let root = self.root(i);
        if let Some(constant) = &self.constant[root] {
            return constant.clone();
        }
        self.least_null[root].map_or_else(|| value.clone(), Value::LabeledNull)
    }
}

/// `Sigma_F` of `i`, an instance on `S'`, with fresh nulls numbered from
/// `floor` (`None` when no id is left).
fn sigma_on(
    i: &FInstance,
    functor: &MigrationFunctor,
    floor: Option<u64>,
) -> Result<(FInstance, SigmaProvenance), AdjunctionError> {
    // The equations of non-injective column maps, in (RowId, column) order.
    let mut classes = ValueClasses::default();
    let mut placed: Vec<(RowId, &Name, &Name)> = Vec::new();
    for id in i.row_ids() {
        let (Some(source), Some(cells)) = (i.table_of(id), i.row(id)) else {
            continue;
        };
        let Some(image) = functor.table_image(source) else {
            continue;
        };
        for column in functor.target().columns(image).keys() {
            let preimage = functor.column_preimage(source, column);
            let Some((first, rest)) = preimage.split_first() else {
                continue;
            };
            let Some(first) = cells.get(*first) else {
                continue;
            };
            for alpha in rest {
                let Some(other) = cells.get(*alpha) else {
                    continue;
                };
                classes.union(first, other).map_err(|(left, right)| {
                    AdjunctionError::SigmaConstantClash {
                        row: id,
                        column: column.clone(),
                        left: Box::new(left),
                        right: Box::new(right),
                    }
                })?;
            }
        }
        placed.push((id, source, image));
    }
    let mut provenance = SigmaProvenance::default();
    for null in nulls_of(i) {
        let own = Value::LabeledNull(null);
        let representative = classes.representative(&own);
        if !same_value(&representative, &own) {
            provenance.merged.insert(null, representative);
        }
    }
    let mut next = floor;
    let mut out = FInstance::new();
    for (id, source, image) in placed {
        let cells = i.row(id).cloned().unwrap_or_default();
        let mut row = Row::new();
        for column in functor.target().columns(image).keys() {
            let value = match functor.column_preimage(source, column).first() {
                Some(alpha) => classes.representative(cells.get(*alpha).unwrap_or(&Value::Null)),
                None => {
                    let fresh = next.ok_or(AdjunctionError::NullIdsExhausted)?;
                    next = fresh.checked_add(1);
                    provenance.fresh.insert(fresh, (id, column.clone()));
                    Value::LabeledNull(fresh)
                }
            };
            row.insert(column.clone(), value);
        }
        out.place_row(image.clone(), id, row);
    }
    for (source, image) in functor.relation_map() {
        for &(s, t) in i.pairs(source) {
            out.place_pair(image.clone(), s, t);
        }
    }
    Ok((out, provenance))
}

/// `Sigma_F` for table instances: the left Kan extension.
///
/// The first projection of [`f_sigma_with_provenance`]; see there.
///
/// # Errors
///
/// As [`f_sigma_with_provenance`].
pub fn f_sigma(x: &FInstance, functor: &MigrationFunctor) -> Result<FInstance, AdjunctionError> {
    f_sigma_with_provenance(x, functor).map(|(sigma, _)| sigma)
}

/// `Sigma_F` for table instances, with where its labelled nulls come from.
///
/// `x` must conform to the whole source schema `S`; it is then restricted to
/// `I = rho_{S'}(x)` ([`f_restrict_to_domain`]). Rows: the rows of the
/// fibre's tables at each target table, each keeping its id (a disjoint
/// union, since ids are unique across an instance); a target table outside
/// the image is empty. Pairs: each target relation holds the union of its
/// fibre's pairs. Cells, by a presentation over the free value side: at a
/// column `beta` of `F(s)`, a row `r` of `s` takes `I(r)` at each column
/// `alpha` of `s` with `F(alpha) = beta`, and a fresh labelled null when
/// there is none. Two columns sent to one equate their values; these
/// equations are solved by union-find over the nulls and constants, each
/// class represented by its constant or else its least null, and every cell
/// takes its class's representative. Fresh nulls are numbered from the
/// least id above every null of `x` (`0` when it holds none), in ascending
/// `(RowId, target column)` order. With an injective column map no equation
/// merges anything.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `x` does not conform to `S`;
/// [`AdjunctionError::SigmaConstantClash`] when the equations join two
/// constants of different canonical bytes, at the first joining equation in
/// `(RowId, column)` order; [`AdjunctionError::NullIdsExhausted`] when a
/// fresh null finds no id left.
pub fn f_sigma_with_provenance(
    x: &FInstance,
    functor: &MigrationFunctor,
) -> Result<(FInstance, SigmaProvenance), AdjunctionError> {
    conform(x, &Scope::whole(functor.source()))?;
    sigma_on(&restrict_to_domain(x, functor), functor, null_floor(x))
}

/// `Sigma_F` of `i`, an instance on the kept part `S'` of the source (as
/// [`f_restrict_to_domain`] gives one), with fresh nulls numbered from the
/// least id above `i`'s: the `Sigma_F` the units, counits, transposes and
/// [`f_sigma_on_hom`] use.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `i` does not conform to `S'`, and
/// as [`f_sigma_with_provenance`].
pub fn f_sigma_restricted(
    i: &FInstance,
    functor: &MigrationFunctor,
) -> Result<(FInstance, SigmaProvenance), AdjunctionError> {
    conform(i, &Scope::domain(functor))?;
    sigma_on(i, functor, null_floor(i))
}

/// `Delta_F` of `y`, an instance on `T`.
fn delta_on(
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<(FInstance, DeltaProvenance), AdjunctionError> {
    let mut allocator = row_allocator(y.row_ids());
    let mut delta = FInstance::new();
    let mut provenance = DeltaProvenance::default();
    let mut copy_of: HashMap<(&Name, RowId), RowId> = HashMap::new();
    let mut copied: HashSet<RowId> = HashSet::new();
    for (source, target) in functor.table_map() {
        let kept = functor.kept_columns(source);
        for (original, cells) in y.rows(target) {
            let id = if copied.insert(original) {
                original
            } else {
                allocate_row(&mut allocator).map_err(|_| AdjunctionError::RowIdsExhausted)?
            };
            let row: Row = kept
                .iter()
                .filter_map(|alpha| {
                    let beta = functor.column_image(source, alpha)?;
                    cells.get(beta).map(|v| ((*alpha).clone(), v.clone()))
                })
                .collect();
            delta.place_row(source.clone(), id, row);
            copy_of.insert((source, original), id);
            provenance.copies.insert(id, (source.clone(), original));
        }
    }
    for (source, target) in functor.relation_map() {
        for &(s, t) in y.pairs(target) {
            if let (Some(&cs), Some(&ct)) = (
                copy_of.get(&(&source.src, s)),
                copy_of.get(&(&source.tgt, t)),
            ) {
                delta.place_pair(source.clone(), cs, ct);
            }
        }
    }
    Ok((delta, provenance))
}

/// `Delta_F` for table instances: precomposition.
///
/// The first projection of [`f_delta_with_provenance`]; see there.
///
/// # Errors
///
/// As [`f_delta_with_provenance`].
pub fn f_delta(y: &FInstance, functor: &MigrationFunctor) -> Result<FInstance, AdjunctionError> {
    f_delta_with_provenance(y, functor).map(|(delta, _)| delta)
}

/// `Delta_F` for table instances, with the row each copy copies.
///
/// `y` must conform to the whole target schema `T`. Each kept table `s`
/// receives a copy of each row `r'` of `F(s)`, whose cell at each kept
/// column `alpha` of `s` is `y(r')` at `F(alpha)`; cells at the columns of
/// `s` outside `S'` are not made, and no null is created. Each kept
/// relation `e` receives the pairs of `F(e)` between copies. Tables and
/// relations outside `S'` receive nothing. So the result is an instance on
/// `S'`.
///
/// Row ids: the copies are made in ascending source table and, within one
/// table, in ascending row of `y`. The first copy of a row keeps its id, so
/// over an injective table map every row keeps its id; every later copy
/// takes a fresh id, from the least id above every row of `y`.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `y` does not conform to `T`, and
/// [`AdjunctionError::RowIdsExhausted`] when the fresh row ids run out.
pub fn f_delta_with_provenance(
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<(FInstance, DeltaProvenance), AdjunctionError> {
    conform(y, &Scope::whole(functor.target()))?;
    delta_on(y, functor)
}

/// `Pi_F` of `i`, an instance on `S'`, after the functor's surjectivity on
/// attributes and `i`'s conformance to `S'` are checked; fresh row ids above
/// `i`'s.
pub(crate) fn pi_on_domain(
    i: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<PiLimit, AdjunctionError> {
    functor
        .attribute_surjective()
        .map_err(AdjunctionError::Pi)?;
    conform(i, &Scope::domain(functor))?;
    pi_limit(i, functor, max_product_size, i.row_ids()).map_err(AdjunctionError::Pi)
}

/// The least row of `dom` at which `hom` fails to be a homomorphism into
/// `cod`: a row without an image, one sent outside its table, one whose
/// cells are not kept, or the source of a pair not sent to a pair.
fn first_offending_row(hom: &FInstanceHom, dom: &FInstance, cod: &FInstance) -> Option<RowId> {
    let row = dom.row_ids().into_iter().find(|&row| {
        let Some(image) = hom.image(row) else {
            return true;
        };
        let Some(table) = dom.table_of(row) else {
            return true;
        };
        if cod.table_of(image) != Some(hom.image_table(table)) {
            return true;
        }
        match (dom.row(row), cod.row(image)) {
            (Some(cells), Some(image_cells)) => {
                cells.len() != image_cells.len()
                    || cells.iter().any(|(column, value)| {
                        image_cells
                            .get(column)
                            .is_none_or(|theirs| !same_value(theirs, hom.image_value(value)))
                    })
            }
            _ => true,
        }
    });
    let pair = dom
        .foreign_keys()
        .into_iter()
        .flat_map(|(edge, pairs)| pairs.iter().map(move |pair| (edge, *pair)))
        .filter(|(edge, (s, t))| match (hom.image(*s), hom.image(*t)) {
            (Some(ms), Some(mt)) => !cod.contains_pair(&hom.image_edge(edge), ms, mt),
            _ => true,
        })
        .map(|(_, (s, _))| s)
        .min();
    match (row, pair) {
        (Some(r), Some(p)) => Some(r.min(p)),
        (r, p) => r.or(p),
    }
}

/// Refuse `hom` unless it is a homomorphism from `dom` to `cod`.
fn require_hom(
    hom: &FInstanceHom,
    dom: &FInstance,
    cod: &FInstance,
) -> Result<(), AdjunctionError> {
    if hom.check(dom, cod).is_ok() {
        return Ok(());
    }
    let row = first_offending_row(hom, dom, cod)
        .or_else(|| dom.row_ids().first().copied())
        .unwrap_or(RowId(0));
    Err(AdjunctionError::NotANaturalMap { row })
}

/// `hom` with its null map cut down to the nulls of `dom` and without
/// identity entries, which is the form homomorphisms are compared in.
fn normalised(hom: FInstanceHom, dom: &FInstance) -> FInstanceHom {
    let nulls = nulls_of(dom);
    let null_map = hom
        .null_map
        .iter()
        .filter(|(null, _)| nulls.contains(null))
        .map(|(null, value)| (*null, value.clone()))
        .collect();
    FInstanceHom::new(hom.row_map.clone())
        .with_table_map(hom.table_map.clone())
        .with_edge_map(hom.edge_map.clone())
        .with_null_map(null_map)
}

/// `g` followed by `h`, normalised on `dom`.
fn then(
    g: &FInstanceHom,
    h: &FInstanceHom,
    dom: &FInstance,
) -> Result<FInstanceHom, AdjunctionError> {
    let composed = g.compose(h).map_err(|_| AdjunctionError::NotANaturalMap {
        row: g
            .row_map
            .iter()
            .find(|(_, mid)| h.image(**mid).is_none())
            .map_or(RowId(0), |(row, _)| *row),
    })?;
    Ok(normalised(composed, dom))
}

/// `Sigma_F(h)`: the image under `Sigma_F` of a homomorphism `h: I -> I'`
/// of instances on `S'`.
///
/// `Sigma_F` keeps row ids, so `Sigma_F(h)` sends each row of `Sigma_F I` to
/// the image of that row under `h`. On nulls it sends the fresh null of a
/// row `r` and column `beta` to the fresh null of `h(r)` and `beta`, and a
/// null of `I` that represents its class to the representative, in
/// `Sigma_F I'`, of its image under `h`.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `i` or `i_prime` does not conform
/// to `S'`, [`AdjunctionError::NotANaturalMap`] when `h` is no homomorphism
/// between them, and the errors of [`f_sigma_restricted`].
pub fn f_sigma_on_hom(
    h: &FInstanceHom,
    i: &FInstance,
    i_prime: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (sigma, provenance) = f_sigma_restricted(i, functor)?;
    let (sigma_prime, provenance_prime) = f_sigma_restricted(i_prime, functor)?;
    require_hom(h, i, i_prime)?;
    sigma_hom(h, &sigma, &provenance, &sigma_prime, &provenance_prime)
}

/// [`f_sigma_on_hom`] over the two `Sigma_F` results already taken.
fn sigma_hom(
    h: &FInstanceHom,
    sigma: &FInstance,
    provenance: &SigmaProvenance,
    sigma_prime: &FInstance,
    provenance_prime: &SigmaProvenance,
) -> Result<FInstanceHom, AdjunctionError> {
    let row_map: BTreeMap<RowId, RowId> = sigma
        .row_ids()
        .into_iter()
        .map(|row| {
            h.image(row)
                .map(|image| (row, image))
                .ok_or(AdjunctionError::NotANaturalMap { row })
        })
        .collect::<Result<_, _>>()?;
    let mut null_map = BTreeMap::new();
    for null in nulls_of(sigma) {
        let image = if let Some((row, column)) = provenance.fresh.get(&null) {
            let target = row_map[row];
            sigma_prime
                .row(target)
                .and_then(|cells| cells.get(column))
                .cloned()
                .ok_or(AdjunctionError::NotANaturalMap { row: *row })?
        } else {
            match h.image_value(&Value::LabeledNull(null)) {
                Value::LabeledNull(n) => provenance_prime
                    .merged
                    .get(n)
                    .cloned()
                    .unwrap_or(Value::LabeledNull(*n)),
                constant => constant.clone(),
            }
        };
        null_map.insert(null, image);
    }
    Ok(FInstanceHom::new(row_map).with_null_map(null_map))
}

/// The unit `eta_I: I -> Delta_F(Sigma_F I)` of `Sigma_F -| Delta_F`, for
/// `i` an instance on `S'`.
///
/// `Sigma_F` keeps every row's id, so row `r` of `I` at `s` maps to the copy
/// at `s` of `r`; its null map sends each null `Sigma_F` merges to its
/// class's representative ([`SigmaProvenance::merged`]).
///
/// # Errors
///
/// As [`f_sigma_restricted`] and [`f_delta_with_provenance`].
pub fn f_unit(i: &FInstance, functor: &MigrationFunctor) -> Result<FInstanceHom, AdjunctionError> {
    let (sigma, provenance) = f_sigma_restricted(i, functor)?;
    let (_, copies) = delta_on(&sigma, functor)?;
    unit_hom(i, &provenance, &copies)
}

/// [`f_unit`] over `Sigma_F I`'s provenance and its `Delta_F`'s copies.
fn unit_hom(
    i: &FInstance,
    provenance: &SigmaProvenance,
    copies: &DeltaProvenance,
) -> Result<FInstanceHom, AdjunctionError> {
    let mut row_map = BTreeMap::new();
    for (table, rows) in i.tables() {
        for row in rows.ids() {
            let copy = copies
                .copy_of(table, row)
                .ok_or(AdjunctionError::NotANaturalMap { row })?;
            row_map.insert(row, copy);
        }
    }
    Ok(FInstanceHom::new(row_map).with_null_map(provenance.merged.clone()))
}

/// The counit `eps_J: Sigma_F(Delta_F J) -> J` of `Sigma_F -| Delta_F`, for
/// `y` an instance on `T`.
///
/// `Sigma_F(Delta_F J)` holds the copies of `Delta_F J` under their own ids;
/// the counit sends each copy back to the row it copies, and each fresh null
/// to that row's cell at the null's column.
///
/// # Errors
///
/// As [`f_delta_with_provenance`] and [`f_sigma_restricted`].
pub fn f_counit(
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (delta, copies) = f_delta_with_provenance(y, functor)?;
    let (sigma, provenance) = sigma_on(&delta, functor, null_floor(&delta))?;
    counit_hom(y, &sigma, &provenance, &copies)
}

/// [`f_counit`] over `Delta_F J`'s copies and the `Sigma_F` of it.
fn counit_hom(
    y: &FInstance,
    sigma: &FInstance,
    provenance: &SigmaProvenance,
    copies: &DeltaProvenance,
) -> Result<FInstanceHom, AdjunctionError> {
    let row_map: BTreeMap<RowId, RowId> = copies
        .copies
        .iter()
        .map(|(copy, (_, original))| (*copy, *original))
        .collect();
    let mut null_map = provenance.merged.clone();
    for (null, (copy, column)) in &provenance.fresh {
        let original = row_map
            .get(copy)
            .ok_or(AdjunctionError::NotANaturalMap { row: *copy })?;
        let value = y
            .row(*original)
            .and_then(|cells| cells.get(column))
            .ok_or(AdjunctionError::NotANaturalMap { row: *copy })?;
        null_map.insert(*null, value.clone());
    }
    Ok(normalised(
        FInstanceHom::new(row_map).with_null_map(null_map),
        sigma,
    ))
}

/// Transpose `Hom_T(Sigma_F I, J) -> Hom_S'(I, Delta_F J)`, sending `g` to
/// `Delta_F(g) . eta_I`.
///
/// # Errors
///
/// As [`f_unit`] and [`f_delta_on_hom`].
pub fn f_transpose_left(
    g: &FInstanceHom,
    i: &FInstance,
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (sigma, provenance) = f_sigma_restricted(i, functor)?;
    let (_, copies) = delta_on(&sigma, functor)?;
    let unit = unit_hom(i, &provenance, &copies)?;
    let delta_g = f_delta_on_hom(g, &sigma, y, functor)?;
    then(&unit, &delta_g, i)
}

/// Transpose `Hom_S'(I, Delta_F J) -> Hom_T(Sigma_F I, J)`, sending `f` to
/// `eps_J . Sigma_F(f)`, the inverse of [`f_transpose_left`].
///
/// # Errors
///
/// As [`f_sigma_on_hom`] and [`f_counit`].
pub fn f_transpose_right(
    f: &FInstanceHom,
    i: &FInstance,
    y: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (delta, copies) = f_delta_with_provenance(y, functor)?;
    let sigma_f = f_sigma_on_hom(f, i, &delta, functor)?;
    let (sigma_delta, provenance) = sigma_on(&delta, functor, null_floor(&delta))?;
    let counit = counit_hom(y, &sigma_delta, &provenance, &copies)?;
    let (sigma, _) = f_sigma_restricted(i, functor)?;
    then(&sigma_f, &counit, &sigma)
}

/// `Delta_F(h)`: the image under `Delta_F` of a homomorphism `h: J -> J'` of
/// instances on `T`, sending the copy at `s` of a row `r` to the copy at
/// `s` of `h(r)`, with `h`'s null map on the nulls of `Delta_F J`.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `y` or `y_prime` does not conform
/// to `T`, [`AdjunctionError::NotANaturalMap`] when `h` is no homomorphism
/// between them, and [`AdjunctionError::RowIdsExhausted`] as
/// [`f_delta_with_provenance`].
pub fn f_delta_on_hom(
    h: &FInstanceHom,
    y: &FInstance,
    y_prime: &FInstance,
    functor: &MigrationFunctor,
) -> Result<FInstanceHom, AdjunctionError> {
    let (delta, copies) = f_delta_with_provenance(y, functor)?;
    let (_, copies_prime) = f_delta_with_provenance(y_prime, functor)?;
    require_hom(h, y, y_prime)?;
    let mut row_map = BTreeMap::new();
    for (copy, (table, original)) in &copies.copies {
        let image = h
            .image(*original)
            .and_then(|image| copies_prime.copy_of(table, image))
            .ok_or(AdjunctionError::NotANaturalMap { row: *copy })?;
        row_map.insert(*copy, image);
    }
    Ok(normalised(
        FInstanceHom::new(row_map).with_null_map(h.null_map.clone()),
        &delta,
    ))
}

/// `Pi_F(h)`: the image under `Pi_F` of a homomorphism `h: I -> I'` of
/// instances on `S'`, sending the row of each tuple to the row of the tuple
/// of its components' images (the one row of a table outside the image to
/// the one row), with `h`'s null map on the nulls of `Pi_F I`.
///
/// # Errors
///
/// The errors of [`crate::functor_pi_restricted`] for either instance, and
/// [`AdjunctionError::NotANaturalMap`] when `h` is no homomorphism between
/// them.
pub fn f_pi_on_hom(
    h: &FInstanceHom,
    i: &FInstance,
    i_prime: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    let pi = pi_on_domain(i, functor, max_product_size)?;
    let pi_prime = pi_on_domain(i_prime, functor, max_product_size)?;
    require_hom(h, i, i_prime)?;
    pi_hom(h, &pi, &pi_prime)
}

/// [`f_pi_on_hom`] over the two limits already taken.
fn pi_hom(
    h: &FInstanceHom,
    pi: &PiLimit,
    pi_prime: &PiLimit,
) -> Result<FInstanceHom, AdjunctionError> {
    let mut row_map = BTreeMap::new();
    for (&row, (table, tuple)) in &pi.tuples {
        let image: Option<Vec<RowId>> = tuple.iter().map(|&r| h.image(r)).collect();
        let target = image
            .and_then(|image| pi_prime.rows.get(&(table.clone(), image)))
            .ok_or(AdjunctionError::NotANaturalMap { row })?;
        row_map.insert(row, *target);
    }
    Ok(normalised(
        FInstanceHom::new(row_map).with_null_map(h.null_map.clone()),
        &pi.instance,
    ))
}

/// The unit `J -> Pi_F(Delta_F J)` of `Delta_F -| Pi_F`, for `y` an instance
/// on `T`.
///
/// Row `r` of `J` at a table `d` maps to the row of `Pi_F(Delta_F J)` whose
/// tuple holds the copy of `r` at every table of `d`'s fibre (the one row,
/// when `d` is outside the image). Its null map is the identity.
///
/// # Errors
///
/// [`AdjunctionError::Pi`] wrapping [`RestrictError::AttributeOutsideImage`]
/// when `functor` is not surjective on attributes, and the errors of
/// [`f_delta_with_provenance`] and [`crate::functor_pi_restricted`].
pub fn f_pi_unit(
    y: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    functor
        .attribute_surjective()
        .map_err(AdjunctionError::Pi)?;
    let (delta, copies) = f_delta_with_provenance(y, functor)?;
    let pi = pi_on_domain(&delta, functor, max_product_size)?;
    pi_unit_hom(y, functor, &copies, &pi)
}

/// [`f_pi_unit`] over `Delta_F J`'s copies and the limit of it.
fn pi_unit_hom(
    y: &FInstance,
    functor: &MigrationFunctor,
    copies: &DeltaProvenance,
    pi: &PiLimit,
) -> Result<FInstanceHom, AdjunctionError> {
    let mut row_map = BTreeMap::new();
    for (table, rows) in y.tables() {
        let fibre = functor.table_fibre(table);
        for row in rows.ids() {
            let tuple: Option<Vec<RowId>> = fibre
                .iter()
                .map(|source| copies.copy_of(source, row))
                .collect();
            let image = tuple
                .and_then(|tuple| pi.rows.get(&(table.clone(), tuple)))
                .ok_or(AdjunctionError::NotANaturalMap { row })?;
            row_map.insert(row, *image);
        }
    }
    Ok(FInstanceHom::new(row_map))
}

/// The counit `Delta_F(Pi_F I) -> I` of `Delta_F -| Pi_F`, for `i` an
/// instance on `S'`: the copy at `s` of a row of `Pi_F I` maps to that row's
/// component at `s`. Its null map is the identity.
///
/// # Errors
///
/// The errors of [`crate::functor_pi_restricted`] and
/// [`f_delta_with_provenance`].
pub fn f_pi_counit(
    i: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    let pi = pi_on_domain(i, functor, max_product_size)?;
    let (_, copies) = f_delta_with_provenance(&pi.instance, functor)?;
    pi_counit_hom(functor, &copies, &pi)
}

/// [`f_pi_counit`] over the limit and the copies of its `Delta_F`.
fn pi_counit_hom(
    functor: &MigrationFunctor,
    copies: &DeltaProvenance,
    pi: &PiLimit,
) -> Result<FInstanceHom, AdjunctionError> {
    let mut row_map = BTreeMap::new();
    for (&copy, (source, original)) in &copies.copies {
        let component = pi.tuples.get(original).and_then(|(table, tuple)| {
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

/// Transpose `Hom_S'(Delta_F J, I) -> Hom_T(J, Pi_F I)`, sending `g` to
/// `Pi_F(g) . unit_J`.
///
/// # Errors
///
/// As [`f_pi_unit`] and [`f_pi_on_hom`].
pub fn f_pi_transpose_left(
    g: &FInstanceHom,
    y: &FInstance,
    i: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    let unit = f_pi_unit(y, functor, max_product_size)?;
    let delta = f_delta(y, functor)?;
    let pi_g = f_pi_on_hom(g, &delta, i, functor, max_product_size)?;
    then(&unit, &pi_g, y)
}

/// Transpose `Hom_T(J, Pi_F I) -> Hom_S'(Delta_F J, I)`, sending `f` to
/// `counit_I . Delta_F(f)`, the inverse of [`f_pi_transpose_left`].
///
/// # Errors
///
/// As [`f_delta_on_hom`] and [`f_pi_counit`].
pub fn f_pi_transpose_right(
    f: &FInstanceHom,
    y: &FInstance,
    i: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstanceHom, AdjunctionError> {
    let pi = pi_on_domain(i, functor, max_product_size)?;
    let delta_f = f_delta_on_hom(f, y, &pi.instance, functor)?;
    let counit = f_pi_counit(i, functor, max_product_size)?;
    let delta = f_delta(y, functor)?;
    then(&delta_f, &counit, &delta)
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
    use crate::ginstance::EdgeId;
    use crate::metadata::Node;
    use crate::value::FieldPresence;

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
            &record_protocol(),
            &table_schema(
                &["box", "item"],
                &[edge("box", "item", "child"), edge("box", "item", "other")],
            ),
            &record_protocol(),
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

    // -- tables -----------------------------------------------------------

    /// A schema whose vertices are all tables of kind `record` and whose
    /// edges are the given relations.
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
                            kind: "record".into(),
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

    /// The protocol of [`table_schema`]: `record` is its one, non-value,
    /// kind.
    fn record_protocol() -> panproto_schema::Protocol {
        panproto_schema::Protocol {
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..panproto_schema::Protocol::default()
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

    /// Brute-force oracles for the table Kan extensions. None of them calls
    /// the code under test: each reads a [`Spec`], a plain description of a
    /// functor between attributed relation-ified schemas, and instances
    /// through their public accessors, and compares values by their
    /// canonical bytes.
    pub(super) mod oracle {
        use std::collections::{BTreeMap, BTreeSet};

        use panproto_gat::Name;
        use panproto_schema::Edge;

        use crate::functor::{FInstance, Row};
        use crate::value::Value;
        use crate::view_ref::RowId;

        /// One side of a functor: tables with their columns, and relations.
        #[derive(Clone, Debug, Default)]
        pub(crate) struct Side {
            /// Each table with its column names.
            pub(crate) tables: BTreeMap<Name, BTreeSet<String>>,
            /// The relation objects.
            pub(crate) relations: BTreeSet<Edge>,
        }

        /// A functor `F: S' -> T`, given by its maps on the kept tables,
        /// relations and columns of the source.
        #[derive(Clone, Debug, Default)]
        pub(crate) struct Spec {
            /// The whole source `S`.
            pub(crate) src: Side,
            /// The target `T`.
            pub(crate) tgt: Side,
            /// Each kept table with its image.
            pub(crate) tables: BTreeMap<Name, Name>,
            /// Each kept relation with its image.
            pub(crate) relations: BTreeMap<Edge, Edge>,
            /// Each kept column `(table, column)` with its image column.
            pub(crate) columns: BTreeMap<(Name, String), String>,
        }

        impl Spec {
            /// The kept tables sent to `d`, ascending.
            pub(crate) fn fibre(&self, d: &Name) -> Vec<Name> {
                self.tables
                    .iter()
                    .filter(|(_, image)| *image == d)
                    .map(|(s, _)| s.clone())
                    .collect()
            }

            /// The kept relations sent to `e`, ascending.
            pub(crate) fn relation_fibre(&self, e: &Edge) -> Vec<Edge> {
                self.relations
                    .iter()
                    .filter(|(_, image)| *image == e)
                    .map(|(s, _)| s.clone())
                    .collect()
            }

            /// The kept columns of `s`, ascending.
            pub(crate) fn kept_columns(&self, s: &Name) -> Vec<String> {
                self.columns
                    .keys()
                    .filter(|(t, _)| t == s)
                    .map(|(_, c)| c.clone())
                    .collect()
            }

            /// The kept columns of `s` sent to `beta`, ascending.
            pub(crate) fn preimage(&self, s: &Name, beta: &str) -> Vec<String> {
                self.columns
                    .iter()
                    .filter(|((t, _), image)| t == s && image.as_str() == beta)
                    .map(|((_, c), _)| c.clone())
                    .collect()
            }

            /// Whether every column of every target table is the image of a
            /// kept column of its fibre; else the least missing one.
            pub(crate) fn missing_column(&self) -> Option<(Name, String)> {
                for (d, columns) in &self.tgt.tables {
                    for beta in columns {
                        let hit = self
                            .fibre(d)
                            .iter()
                            .any(|s| !self.preimage(s, beta).is_empty());
                        if !hit {
                            return Some((d.clone(), beta.clone()));
                        }
                    }
                }
                None
            }
        }

        /// The canonical bytes of a value: the equality every oracle uses.
        pub(crate) fn canon(value: &Value) -> Vec<u8> {
            value.canonical_bytes()
        }

        /// The labelled nulls in the cells of an instance.
        pub(crate) fn nulls(instance: &FInstance) -> BTreeSet<u64> {
            instance
                .row_ids()
                .into_iter()
                .filter_map(|id| instance.row(id))
                .flat_map(|row| row.values().filter_map(Value::as_labeled_null))
                .collect()
        }

        /// A row's cells, sorted by column.
        fn sorted(row: &Row) -> Vec<(&String, &Value)> {
            let mut cells: Vec<(&String, &Value)> = row.iter().collect();
            cells.sort_by(|a, b| a.0.cmp(b.0));
            cells
        }

        /// A homomorphism found by brute force: its row map and its null
        /// map, without identity entries.
        pub(crate) type Found = (BTreeMap<RowId, RowId>, BTreeMap<u64, Value>);

        /// Extend `nu` by the cells of `row` sent to `image`: key sets must be
        /// equal, the first occurrence of a null fixes its value, and every
        /// later occurrence and every constant must agree by canonical bytes.
        fn cells_agree(row: &Row, image: &Row, nu: &mut BTreeMap<u64, Value>) -> bool {
            if row.len() != image.len() {
                return false;
            }
            for (column, value) in sorted(row) {
                let Some(theirs) = image.get(column) else {
                    return false;
                };
                match value {
                    Value::LabeledNull(n) => match nu.get(n) {
                        Some(fixed) => {
                            if canon(fixed) != canon(theirs) {
                                return false;
                            }
                        }
                        None => {
                            nu.insert(*n, theirs.clone());
                        }
                    },
                    constant => {
                        if canon(constant) != canon(theirs) {
                            return false;
                        }
                    }
                }
            }
            true
        }

        /// Whether a row map from `a` to `b` is a homomorphism, with the
        /// null map its cells fix (scanned in `(RowId, column)` order, as
        /// far as the scan got).
        pub(crate) fn brute_accepts(
            a: &FInstance,
            b: &FInstance,
            row_map: &BTreeMap<RowId, RowId>,
        ) -> (bool, BTreeMap<u64, Value>) {
            let mut nu = BTreeMap::new();
            for id in a.row_ids() {
                let Some(&image) = row_map.get(&id) else {
                    return (false, nu);
                };
                if a.table_of(id) != b.table_of(image) {
                    return (false, nu);
                }
                let (Some(row), Some(image_row)) = (a.row(id), b.row(image)) else {
                    return (false, nu);
                };
                if !cells_agree(row, image_row, &mut nu) {
                    return (false, nu);
                }
            }
            for (edge, pairs) in a.foreign_keys() {
                for &(s, t) in pairs {
                    let (Some(&ms), Some(&mt)) = (row_map.get(&s), row_map.get(&t)) else {
                        return (false, nu);
                    };
                    if !b.pairs(edge).contains(&(ms, mt)) {
                        return (false, nu);
                    }
                }
            }
            (true, nu)
        }

        /// Every table-respecting row map from `a` to `b`, in ascending
        /// order of the image of each row (rows ascending).
        pub(crate) fn candidates(a: &FInstance, b: &FInstance) -> Vec<BTreeMap<RowId, RowId>> {
            let rows = a.row_ids();
            let choices: Vec<Vec<RowId>> = rows
                .iter()
                .map(|&r| {
                    a.table_of(r)
                        .map(|t| b.rows(t).into_iter().map(|(id, _)| id).collect())
                        .unwrap_or_default()
                })
                .collect();
            let mut out = vec![BTreeMap::new()];
            for (row, options) in rows.iter().zip(&choices) {
                let mut next = Vec::with_capacity(out.len() * options.len());
                for partial in &out {
                    for &option in options {
                        let mut extended: BTreeMap<RowId, RowId> = partial.clone();
                        extended.insert(*row, option);
                        next.push(extended);
                    }
                }
                out = next;
            }
            out
        }

        /// Drop the identity entries of a null map.
        pub(crate) fn normalise(nu: BTreeMap<u64, Value>) -> BTreeMap<u64, Value> {
            nu.into_iter()
                .filter(|(n, v)| canon(v) != canon(&Value::LabeledNull(*n)))
                .collect()
        }

        /// Every homomorphism `a -> b` between instances of one schema:
        /// every function from the rows of `a` to the rows of `b` that sends
        /// each row into its own table, keeps the cells (nulls fixed at their
        /// first occurrence) and sends pairs to pairs. Rows are assigned in
        /// ascending id, and a partial map whose rows already disagree is not
        /// extended, which the `(RowId, column)` scan order makes the same as
        /// testing every complete map.
        pub(crate) fn brute_homs(a: &FInstance, b: &FInstance) -> Vec<Found> {
            fn go(
                i: usize,
                rows: &[RowId],
                a: &FInstance,
                b: &FInstance,
                chosen: &mut BTreeMap<RowId, RowId>,
                nu: &BTreeMap<u64, Value>,
                out: &mut Vec<Found>,
            ) {
                if i == rows.len() {
                    let (ok, nu) = brute_accepts(a, b, chosen);
                    if ok {
                        out.push((chosen.clone(), normalise(nu)));
                    }
                    return;
                }
                let row = rows[i];
                let (Some(table), Some(cells)) = (a.table_of(row), a.row(row)) else {
                    return;
                };
                for (image, image_cells) in b.rows(table) {
                    let mut extended = nu.clone();
                    if cells_agree(cells, image_cells, &mut extended) {
                        chosen.insert(row, image);
                        go(i + 1, rows, a, b, chosen, &extended, out);
                        chosen.remove(&row);
                    }
                }
            }
            let rows = a.row_ids();
            let mut out = Vec::new();
            go(
                0,
                &rows,
                a,
                b,
                &mut BTreeMap::new(),
                &BTreeMap::new(),
                &mut out,
            );
            out
        }

        /// Whether `a` and `b` are isomorphic: some homomorphism `a -> b` is
        /// a bijection on rows and on nulls, and its inverse is a
        /// homomorphism `b -> a`.
        pub(crate) fn brute_iso(a: &FInstance, b: &FInstance) -> bool {
            if a.total_row_count() != b.total_row_count() {
                return false;
            }
            let (na, nb) = (nulls(a), nulls(b));
            brute_homs(a, b).into_iter().any(|(rows, nu)| {
                let images: BTreeSet<RowId> = rows.values().copied().collect();
                if images.len() != rows.len() {
                    return false;
                }
                let mut null_images = BTreeMap::new();
                for &n in &na {
                    let v = nu.get(&n).cloned().unwrap_or(Value::LabeledNull(n));
                    let Value::LabeledNull(m) = v else {
                        return false;
                    };
                    if null_images.insert(m, n).is_some() {
                        return false;
                    }
                }
                if null_images.keys().copied().collect::<BTreeSet<u64>>() != nb {
                    return false;
                }
                let inverse: BTreeMap<RowId, RowId> = rows.iter().map(|(x, y)| (*y, *x)).collect();
                let (ok, back) = brute_accepts(b, a, &inverse);
                ok && null_images.iter().all(|(m, n)| {
                    back.get(m)
                        .map_or(*m == *n, |v| canon(v) == canon(&Value::LabeledNull(*n)))
                })
            })
        }

        /// `rho_{S'}(x)`: the rows at kept tables with their cells at kept
        /// columns, and the pairs at kept relations.
        pub(crate) fn restrict(spec: &Spec, x: &FInstance) -> FInstance {
            let tables = spec.tables.keys().map(|s| {
                let kept = spec.kept_columns(s);
                let rows = x
                    .rows(s)
                    .into_iter()
                    .map(|(id, row)| {
                        let cells: Row = row
                            .iter()
                            .filter(|(c, _)| kept.contains(c))
                            .map(|(c, v)| (c.clone(), v.clone()))
                            .collect();
                        (id, cells)
                    })
                    .collect::<Vec<_>>();
                (s.clone(), rows)
            });
            let pairs = spec
                .relations
                .keys()
                .map(|e| (e.clone(), x.pairs(e).iter().copied().collect::<Vec<_>>()));
            FInstance::from_parts(tables, pairs).expect("a restriction is an instance")
        }

        /// The Delta oracle: the copy of each row `r'` of `J(F s)` at every
        /// kept table `s`, keyed by `(s, r')`, with its cells, and the pairs
        /// of each kept relation between copies.
        pub(crate) struct DeltaOracle {
            /// Each copy's cells.
            pub(crate) copies: BTreeMap<(Name, RowId), Row>,
            /// Each kept relation's pairs between copies.
            pub(crate) pairs: BTreeMap<Edge, BTreeSet<((Name, RowId), (Name, RowId))>>,
        }

        /// [`DeltaOracle`] by the direct formula.
        pub(crate) fn delta(spec: &Spec, j: &FInstance) -> DeltaOracle {
            let mut copies = BTreeMap::new();
            for (s, d) in &spec.tables {
                for (r, row) in j.rows(d) {
                    let cells: Row = spec
                        .kept_columns(s)
                        .into_iter()
                        .filter_map(|alpha| {
                            let beta = &spec.columns[&(s.clone(), alpha.clone())];
                            row.get(beta).map(|v| (alpha, v.clone()))
                        })
                        .collect();
                    copies.insert((s.clone(), r), cells);
                }
            }
            let mut pairs: BTreeMap<Edge, BTreeSet<_>> = BTreeMap::new();
            for (e, image) in &spec.relations {
                let set = pairs.entry(e.clone()).or_default();
                for &(a, b) in j.pairs(image) {
                    set.insert(((e.src.clone(), a), (e.tgt.clone(), b)));
                }
            }
            DeltaOracle { copies, pairs }
        }

        /// The Pi oracle: at each target table the tuples of the whole
        /// product of its fibre's rows whose values at the preimages of each
        /// column agree, with their cells; at each target relation the
        /// pairs of tuples meeting every relation carried to it (every pair
        /// when none is).
        pub(crate) struct PiOracle {
            /// Each target table's tuples with their cells.
            pub(crate) rows: BTreeMap<Name, BTreeMap<Vec<RowId>, Row>>,
            /// Each target relation's pairs of tuples.
            pub(crate) pairs: BTreeMap<Edge, BTreeSet<(Vec<RowId>, Vec<RowId>)>>,
        }

        /// [`PiOracle`] over `i`, an instance on `S'`.
        pub(crate) fn pi(spec: &Spec, i: &FInstance) -> PiOracle {
            let mut rows = BTreeMap::new();
            for (d, columns) in &spec.tgt.tables {
                let fibre = spec.fibre(d);
                let mut product: Vec<Vec<RowId>> = vec![Vec::new()];
                for s in &fibre {
                    let ids: Vec<RowId> = i.rows(s).into_iter().map(|(id, _)| id).collect();
                    product = product
                        .into_iter()
                        .flat_map(|t| {
                            ids.iter().map(move |&id| {
                                let mut t = t.clone();
                                t.push(id);
                                t
                            })
                        })
                        .collect();
                }
                let mut kept = BTreeMap::new();
                for tuple in product {
                    let mut cells = Row::new();
                    let mut cone = true;
                    for beta in columns {
                        let mut values: BTreeMap<Vec<u8>, Value> = BTreeMap::new();
                        for (s, r) in fibre.iter().zip(&tuple) {
                            for alpha in spec.preimage(s, beta) {
                                if let Some(v) = i.row(*r).and_then(|row| row.get(&alpha)) {
                                    values.insert(canon(v), v.clone());
                                }
                            }
                        }
                        if values.len() == 1 {
                            if let Some((_, v)) = values.into_iter().next() {
                                cells.insert(beta.clone(), v);
                            }
                        } else {
                            cone = false;
                        }
                    }
                    if cone {
                        kept.insert(tuple, cells);
                    }
                }
                rows.insert(d.clone(), kept);
            }
            let mut pairs = BTreeMap::new();
            for relation in &spec.tgt.relations {
                let (from, to) = (&rows[&relation.src], &rows[&relation.tgt]);
                let carried = spec.relation_fibre(relation);
                let (fa, fb) = (spec.fibre(&relation.src), spec.fibre(&relation.tgt));
                let mut set = BTreeSet::new();
                for x in from.keys() {
                    for y in to.keys() {
                        let meets = carried.iter().all(|e| {
                            let (Some(p), Some(q)) = (
                                fa.iter().position(|s| *s == e.src),
                                fb.iter().position(|s| *s == e.tgt),
                            ) else {
                                return false;
                            };
                            i.pairs(e).contains(&(x[p], y[q]))
                        });
                        if meets {
                            set.insert((x.clone(), y.clone()));
                        }
                    }
                }
                pairs.insert(relation.clone(), set);
            }
            PiOracle { rows, pairs }
        }

        /// A term of the Sigma presentation.
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum Term {
            /// The cell of a row at a target column.
            Cell(RowId, String),
            /// A labelled null of the input.
            Null(u64),
            /// A constant, by its canonical bytes.
            Const(Vec<u8>),
            /// The fresh null of a row at a non-image column.
            Fresh(RowId, String),
        }

        /// The Sigma oracle over `i`, an instance on `S'`: the presentation
        /// with generators the rows, the nulls of `i` and one fresh null per
        /// row and non-image column, its equations solved by a naive
        /// fixpoint (merge any two classes an equation joins, until nothing
        /// changes, after each equation in `(RowId, column)` order). A class
        /// with two constants is the clash at the first equation that made
        /// it. Fresh nulls are numbered from `fresh_base` in that order.
        pub(crate) fn sigma(
            spec: &Spec,
            i: &FInstance,
            fresh_base: u64,
        ) -> Result<FInstance, (RowId, String)> {
            let mut constants: BTreeMap<Vec<u8>, Value> = BTreeMap::new();
            let mut term_of = |v: &Value| match v {
                Value::LabeledNull(n) => Term::Null(*n),
                other => {
                    let key = canon(other);
                    constants
                        .entry(key.clone())
                        .or_insert_with(|| other.clone());
                    Term::Const(key)
                }
            };
            let mut equations: Vec<(RowId, String, Term, Term)> = Vec::new();
            let mut placed: Vec<(Name, RowId, Vec<String>)> = Vec::new();
            for id in i.row_ids() {
                let Some(s) = i.table_of(id) else { continue };
                let Some(d) = spec.tables.get(s) else {
                    continue;
                };
                let columns: Vec<String> = spec.tgt.tables[d].iter().cloned().collect();
                for beta in &columns {
                    let pre = spec.preimage(s, beta);
                    if pre.is_empty() {
                        equations.push((
                            id,
                            beta.clone(),
                            Term::Cell(id, beta.clone()),
                            Term::Fresh(id, beta.clone()),
                        ));
                    }
                    for alpha in pre {
                        if let Some(v) = i.row(id).and_then(|row| row.get(&alpha)) {
                            let t = term_of(v);
                            equations.push((id, beta.clone(), Term::Cell(id, beta.clone()), t));
                        }
                    }
                }
                placed.push((d.clone(), id, columns));
            }
            let mut classes: Vec<BTreeSet<Term>> = Vec::new();
            for (_, _, a, b) in &equations {
                for t in [a, b] {
                    if !classes.iter().any(|c| c.contains(t)) {
                        classes.push(BTreeSet::from([t.clone()]));
                    }
                }
            }
            for k in 0..equations.len() {
                loop {
                    let mut merge = None;
                    'search: for (_, _, a, b) in &equations[..=k] {
                        let ca = classes.iter().position(|c| c.contains(a));
                        let cb = classes.iter().position(|c| c.contains(b));
                        if let (Some(x), Some(y)) = (ca, cb) {
                            if x != y {
                                merge = Some((x.min(y), x.max(y)));
                                break 'search;
                            }
                        }
                    }
                    let Some((x, y)) = merge else { break };
                    let moved = classes.remove(y);
                    classes[x].extend(moved);
                }
                let clash = classes
                    .iter()
                    .any(|c| c.iter().filter(|t| matches!(t, Term::Const(_))).count() > 1);
                if clash {
                    let (row, column, _, _) = &equations[k];
                    return Err((*row, column.clone()));
                }
            }
            let mut fresh_ids: BTreeMap<(RowId, String), u64> = BTreeMap::new();
            let mut next = fresh_base;
            for (_, _, _, b) in &equations {
                if let Term::Fresh(r, beta) = b {
                    fresh_ids.insert((*r, beta.clone()), next);
                    next += 1;
                }
            }
            let value_of = |cell: &Term| -> Value {
                let Some(class) = classes.iter().find(|c| c.contains(cell)) else {
                    return Value::Null;
                };
                if let Some(Term::Const(k)) = class.iter().find(|t| matches!(t, Term::Const(_))) {
                    return constants[k].clone();
                }
                if let Some(Term::Null(n)) = class.iter().find(|t| matches!(t, Term::Null(_))) {
                    return Value::LabeledNull(*n);
                }
                class
                    .iter()
                    .find_map(|t| match t {
                        Term::Fresh(r, b) => Some(Value::LabeledNull(fresh_ids[&(*r, b.clone())])),
                        _ => None,
                    })
                    .unwrap_or(Value::Null)
            };
            let mut tables: BTreeMap<Name, Vec<(RowId, Row)>> = BTreeMap::new();
            for (d, id, columns) in placed {
                let row: Row = columns
                    .iter()
                    .map(|beta| (beta.clone(), value_of(&Term::Cell(id, beta.clone()))))
                    .collect();
                tables.entry(d).or_default().push((id, row));
            }
            let mut pairs: BTreeMap<Edge, BTreeSet<(RowId, RowId)>> = BTreeMap::new();
            for (e, image) in &spec.relations {
                pairs
                    .entry(image.clone())
                    .or_default()
                    .extend(i.pairs(e).iter().copied());
            }
            Ok(FInstance::from_parts(
                tables,
                pairs.into_iter().map(|(e, p)| (e, p.into_iter().collect())),
            )
            .expect("the presentation's rows and pairs form an instance"))
        }
    }

    /// The code under test, built from an oracle [`oracle::Spec`]: tables of
    /// kind `table` with integer columns `table.column`, relations of kind
    /// `fk`, and the migration stating the spec's functor.
    pub(super) mod fixture {
        use std::collections::{BTreeMap, BTreeSet, HashMap};

        use panproto_gat::Name;
        use panproto_schema::{Edge, Protocol, ScalarKind, Schema, Vertex};

        use super::oracle::{Side, Spec};
        use crate::functor::Row;
        use crate::migration_functor::MigrationFunctor;
        use crate::value::Value;
        use crate::wtype::CompiledMigration;

        /// The attribute arrow of `column` of `table`.
        pub(crate) fn attribute(table: &str, column: &str) -> Edge {
            Edge {
                src: table.into(),
                tgt: format!("{table}.{column}").into(),
                kind: "prop".into(),
                name: Some(column.into()),
            }
        }

        /// A relation `src -> tgt` named `name`.
        pub(crate) fn fk(src: &str, tgt: &str, name: &str) -> Edge {
            Edge {
                src: src.into(),
                tgt: tgt.into(),
                kind: "fk".into(),
                name: Some(name.into()),
            }
        }

        /// One side of a spec.
        pub(crate) fn side(tables: &[(&str, &[&str])], relations: &[Edge]) -> Side {
            Side {
                tables: tables
                    .iter()
                    .map(|(t, cs)| (Name::from(*t), cs.iter().map(|c| (*c).to_owned()).collect()))
                    .collect(),
                relations: relations.iter().cloned().collect(),
            }
        }

        /// A spec from its two sides and its maps.
        pub(crate) fn spec(
            src: Side,
            tgt: Side,
            tables: &[(&str, &str)],
            relations: &[(Edge, Edge)],
            columns: &[((&str, &str), &str)],
        ) -> Spec {
            Spec {
                src,
                tgt,
                tables: tables
                    .iter()
                    .map(|(s, d)| (Name::from(*s), Name::from(*d)))
                    .collect(),
                relations: relations.iter().cloned().collect(),
                columns: columns
                    .iter()
                    .map(|((t, c), b)| ((Name::from(*t), (*c).to_owned()), (*b).to_owned()))
                    .collect(),
            }
        }

        /// The schema of one side.
        pub(crate) fn schema_of(side: &Side) -> Schema {
            let mut vertices = HashMap::new();
            let mut edges = HashMap::new();
            for (t, columns) in &side.tables {
                vertices.insert(
                    t.clone(),
                    Vertex {
                        id: t.clone(),
                        kind: "table".into(),
                        nsid: None,
                    },
                );
                for c in columns {
                    let arrow = attribute(t, c);
                    vertices.insert(
                        arrow.tgt.clone(),
                        Vertex {
                            id: arrow.tgt.clone(),
                            kind: "integer".into(),
                            nsid: None,
                        },
                    );
                    edges.insert(arrow, Name::from("prop"));
                }
            }
            for e in &side.relations {
                edges.insert(e.clone(), e.kind.clone());
            }
            Schema {
                protocol: "test".into(),
                vertices,
                edges,
                ..Schema::default()
            }
        }

        /// The protocol of both sides: `table` non-value, `integer` value.
        pub(crate) fn protocol() -> Protocol {
            Protocol {
                value_kinds: BTreeMap::from([("integer".to_owned(), ScalarKind::Any)]),
                non_value_kinds: BTreeSet::from(["table".to_owned()]),
                ..Protocol::default()
            }
        }

        /// The migration stating the spec's functor.
        pub(crate) fn migration_of(spec: &Spec) -> CompiledMigration {
            let mut m = CompiledMigration::default();
            for (s, d) in &spec.tables {
                m.vertex_remap.insert(s.clone(), d.clone());
            }
            for ((s, alpha), beta) in &spec.columns {
                let d = &spec.tables[s];
                m.vertex_remap
                    .insert(format!("{s}.{alpha}").into(), format!("{d}.{beta}").into());
                m.edge_remap.insert(attribute(s, alpha), attribute(d, beta));
            }
            for (e, image) in &spec.relations {
                m.edge_remap.insert(e.clone(), image.clone());
            }
            m
        }

        /// The functor of a spec, as the code under test builds it.
        pub(crate) fn functor(spec: &Spec) -> MigrationFunctor {
            MigrationFunctor::new(
                &schema_of(&spec.src),
                &protocol(),
                &schema_of(&spec.tgt),
                &protocol(),
                &migration_of(spec),
            )
            .unwrap_or_else(|e| panic!("{spec:?} states no functor: {e}"))
        }

        /// A row from its cells.
        pub(crate) fn row(cells: &[(&str, Value)]) -> Row {
            cells
                .iter()
                .map(|(c, v)| ((*c).to_owned(), v.clone()))
                .collect()
        }
    }

    /// The table Kan extensions on hand-built cases, each checked against
    /// its oracle.
    mod kan {
        use std::collections::BTreeMap;

        use panproto_gat::Name;

        use super::fixture::{attribute, functor, row, side, spec};
        use super::oracle::{self, Spec};
        use crate::adjunction::{
            AdjunctionError, NonConformance, f_delta, f_delta_with_provenance, f_sigma,
            f_sigma_with_provenance,
        };
        use crate::functor::FInstance;
        use crate::instance_hom::FInstanceHom;
        use crate::pi::functor_pi_along_with_provenance;
        use crate::value::Value;
        use crate::view_ref::RowId;

        /// `post(title) -> post(name)`.
        fn rename() -> Spec {
            spec(
                side(&[("post", &["title"])], &[]),
                side(&[("post", &["name"])], &[]),
                &[("post", "post")],
                &[],
                &[(("post", "title"), "name")],
            )
        }

        /// `t(a1, a2, c) -> t(b, c)` with `a1` and `a2` sent to `b`.
        fn equating() -> Spec {
            spec(
                side(&[("t", &["a1", "a2", "c"])], &[]),
                side(&[("t", &["b", "c"])], &[]),
                &[("t", "t")],
                &[],
                &[(("t", "a1"), "b"), (("t", "a2"), "b"), (("t", "c"), "c")],
            )
        }

        /// `a(x, gone) -> a(x, y)`: `gone` is dropped and `y` is outside the
        /// image.
        fn widening() -> Spec {
            spec(
                side(&[("a", &["x", "gone"])], &[]),
                side(&[("a", &["x", "y"])], &[]),
                &[("a", "a")],
                &[],
                &[(("a", "x"), "x")],
            )
        }

        fn int(v: i64) -> Value {
            Value::Int(v)
        }

        fn null(n: u64) -> Value {
            Value::LabeledNull(n)
        }

        fn one_table(table: &str, rows: Vec<(u32, crate::functor::Row)>) -> FInstance {
            FInstance::new()
                .with_rows(table, rows.into_iter().map(|(i, r)| (RowId(i), r)))
                .unwrap_or_else(|e| panic!("{e}"))
        }

        #[test]
        fn delta_renames_a_column_to_its_source_name() {
            let spec = rename();
            let j = one_table("post", vec![(0, row(&[("name", int(7))]))]);
            let (delta, provenance) = f_delta_with_provenance(&j, &functor(&spec)).unwrap();
            let expected = oracle::delta(&spec, &j);
            assert_eq!(delta.row(RowId(0)), Some(&row(&[("title", int(7))])));
            for (copy, (table, original)) in &provenance.copies {
                assert_eq!(
                    delta.row(*copy),
                    expected.copies.get(&(table.clone(), *original))
                );
            }
            assert_eq!(provenance.copies.len(), expected.copies.len());
        }

        #[test]
        fn sigma_renames_a_column_to_its_target_name() {
            let spec = rename();
            let i = one_table("post", vec![(0, row(&[("title", int(7))]))]);
            let sigma = f_sigma(&i, &functor(&spec)).unwrap();
            assert_eq!(
                sigma,
                one_table("post", vec![(0, row(&[("name", int(7))]))])
            );
            let expected = oracle::sigma(&spec, &i, 1 << 40).unwrap();
            assert!(oracle::brute_iso(&sigma, &expected));
        }

        /// Delta keeps only the cells at the images of the kept columns: a
        /// target column outside the image, and the null only it holds, are
        /// gone.
        #[test]
        fn delta_drops_cells_outside_the_image() {
            let spec = spec(
                side(&[("post", &["title"])], &[]),
                side(&[("post", &["name", "extra"])], &[]),
                &[("post", "post")],
                &[],
                &[(("post", "title"), "name")],
            );
            let j = one_table(
                "post",
                vec![(0, row(&[("name", int(1)), ("extra", null(4))]))],
            );
            let delta = f_delta(&j, &functor(&spec)).unwrap();
            assert_eq!(
                delta,
                one_table("post", vec![(0, row(&[("title", int(1))]))])
            );
            assert!(oracle::nulls(&delta).is_empty());
        }

        #[test]
        fn sigma_fills_a_non_image_column_with_a_fresh_null() {
            let spec = widening();
            let x = one_table(
                "a",
                vec![
                    (0, row(&[("x", int(1)), ("gone", int(0))])),
                    (1, row(&[("x", null(3)), ("gone", int(0))])),
                ],
            );
            let (sigma, provenance) = f_sigma_with_provenance(&x, &functor(&spec)).unwrap();
            assert_eq!(
                sigma,
                one_table(
                    "a",
                    vec![
                        (0, row(&[("x", int(1)), ("y", null(4))])),
                        (1, row(&[("x", null(3)), ("y", null(5))])),
                    ]
                )
            );
            assert_eq!(
                provenance.fresh,
                BTreeMap::from([
                    (4, (RowId(0), "y".to_owned())),
                    (5, (RowId(1), "y".to_owned()))
                ])
            );
            assert!(provenance.merged.is_empty());
            let expected = oracle::sigma(&spec, &oracle::restrict(&spec, &x), 1 << 40).unwrap();
            assert!(oracle::brute_iso(&sigma, &expected));
        }

        /// Two columns sent to one equate their values; the classes are
        /// solved by union-find, each represented by its constant or least
        /// null, and every cell, in every column, takes its class's
        /// representative.
        #[test]
        fn sigma_solves_equated_columns_by_union_find() {
            let spec = equating();
            let x = one_table(
                "t",
                vec![
                    (0, row(&[("a1", null(5)), ("a2", int(0)), ("c", null(6))])),
                    (1, row(&[("a1", null(6)), ("a2", null(7)), ("c", null(5))])),
                    (2, row(&[("a1", null(8)), ("a2", null(8)), ("c", int(1))])),
                ],
            );
            let (sigma, provenance) = f_sigma_with_provenance(&x, &functor(&spec)).unwrap();
            assert_eq!(
                sigma,
                one_table(
                    "t",
                    vec![
                        (0, row(&[("b", int(0)), ("c", null(6))])),
                        (1, row(&[("b", null(6)), ("c", int(0))])),
                        (2, row(&[("b", null(8)), ("c", int(1))])),
                    ]
                )
            );
            assert_eq!(
                provenance.merged,
                BTreeMap::from([(5, int(0)), (7, null(6))])
            );
            assert!(provenance.fresh.is_empty());
            let expected = oracle::sigma(&spec, &x, 1 << 40).unwrap();
            assert!(oracle::brute_iso(&sigma, &expected));
        }

        #[test]
        fn sigma_refuses_a_clash_of_constants() {
            let spec = equating();
            let x = one_table(
                "t",
                vec![
                    (0, row(&[("a1", null(0)), ("a2", int(1)), ("c", int(9))])),
                    (1, row(&[("a1", null(0)), ("a2", int(2)), ("c", int(9))])),
                ],
            );
            let refused = f_sigma(&x, &functor(&spec));
            assert!(
                matches!(
                    &refused,
                    Err(AdjunctionError::SigmaConstantClash { row, column, left, right })
                        if *row == RowId(1) && column == "b" && **left == int(1) && **right == int(2)
                ),
                "{refused:?}"
            );
            assert_eq!(oracle::sigma(&spec, &x, 0), Err((RowId(1), "b".to_owned())));
        }

        /// Fresh nulls are numbered above every null of the input, even one
        /// only a dropped column holds; with no null left, Sigma refuses.
        #[test]
        fn null_ids_continue_above_the_largest_in_use() {
            let spec = widening();
            let f = functor(&spec);
            let x = one_table("a", vec![(0, row(&[("x", null(2)), ("gone", null(9))]))]);
            let (sigma, provenance) = f_sigma_with_provenance(&x, &f).unwrap();
            assert_eq!(
                sigma.row(RowId(0)).and_then(|r| r.get("y")),
                Some(&null(10))
            );
            assert_eq!(provenance.fresh.keys().collect::<Vec<_>>(), [&10]);
            let none = one_table("a", vec![(0, row(&[("x", int(2)), ("gone", int(9))]))]);
            assert_eq!(
                f_sigma(&none, &f)
                    .unwrap()
                    .row(RowId(0))
                    .and_then(|r| r.get("y")),
                Some(&null(0))
            );
            let full = one_table(
                "a",
                vec![(0, row(&[("x", int(2)), ("gone", null(u64::MAX))]))],
            );
            assert!(matches!(
                f_sigma(&full, &f),
                Err(AdjunctionError::NullIdsExhausted)
            ));
            // With no fresh null needed, the largest id is no obstacle.
            let identity = spec_identity_on_a();
            let kept = one_table("a", vec![(0, row(&[("x", null(u64::MAX))]))]);
            assert_eq!(f_sigma(&kept, &functor(&identity)).unwrap(), kept);
        }

        fn spec_identity_on_a() -> Spec {
            spec(
                side(&[("a", &["x"])], &[]),
                side(&[("a", &["x"])], &[]),
                &[("a", "a")],
                &[],
                &[(("a", "x"), "x")],
            )
        }

        /// The input checks of the three operations: Sigma and Pi against
        /// the whole source, Delta against the whole target.
        fn assert_refused(
            x: &FInstance,
            spec: &Spec,
            row: u32,
            column: Option<&str>,
            reason: &NonConformance,
        ) {
            let f = functor(spec);
            let expect = |result: Result<(), AdjunctionError>| match result {
                Err(AdjunctionError::NonConforming {
                    row: r,
                    column: c,
                    reason: why,
                }) => {
                    assert_eq!((r, c.as_deref(), &why), (RowId(row), column, reason));
                }
                other => panic!("expected NonConforming, got {other:?}"),
            };
            expect(f_sigma(x, &f).map(|_| ()));
            expect(functor_pi_along_with_provenance(x, &f, 100).map(|_| ()));
            // The same rows as a target instance of the identity functor.
            let identity = functor(&spec_identity_on_a());
            expect(f_delta(x, &identity).map(|_| ()));
        }

        #[test]
        fn a_stray_cell_is_non_conforming() {
            let x = one_table(
                "a",
                vec![
                    (0, row(&[("x", int(1))])),
                    (1, row(&[("x", int(1)), ("zz", int(2)), ("aa", int(3))])),
                ],
            );
            assert_refused(
                &x,
                &spec_identity_on_a(),
                1,
                Some("aa"),
                &NonConformance::StrayCell,
            );
        }

        #[test]
        fn a_partial_row_is_non_conforming() {
            let x = one_table("a", vec![(3, row(&[("x", int(1))])), (4, row(&[]))]);
            assert_refused(
                &x,
                &spec_identity_on_a(),
                4,
                Some("x"),
                &NonConformance::MissingCell,
            );
            // An absent cell and a `Null` one differ: a `Null` cell conforms.
            let f = functor(&spec_identity_on_a());
            let with_null = one_table("a", vec![(4, row(&[("x", Value::Null)]))]);
            assert_eq!(f_sigma(&with_null, &f).unwrap(), with_null);
            // A row at no table of the schema, and a pair at no relation.
            let outside = one_table("b", vec![(0, row(&[]))]);
            assert_refused(
                &outside,
                &spec_identity_on_a(),
                0,
                None,
                &NonConformance::TableOutside { table: "b".into() },
            );
            let looped = one_table("a", vec![(0, row(&[("x", int(1))]))])
                .with_foreign_key(super::fixture::fk("a", "a", "loop"), [(RowId(0), RowId(0))])
                .unwrap();
            assert_refused(
                &looped,
                &spec_identity_on_a(),
                0,
                None,
                &NonConformance::RelationOutside {
                    edge: Box::new(super::fixture::fk("a", "a", "loop")),
                },
            );
        }

        #[test]
        fn a_nested_null_is_non_conforming() {
            let nested = Value::List(vec![
                int(1),
                Value::Unknown([("k".to_owned(), null(3))].into()),
            ]);
            let x = one_table(
                "a",
                vec![(0, row(&[("x", null(2))])), (1, row(&[("x", nested)]))],
            );
            assert_refused(
                &x,
                &spec_identity_on_a(),
                1,
                Some("x"),
                &NonConformance::NestedLabeledNull,
            );
            // A list holding no null is a constant like any other.
            let list = one_table("a", vec![(0, row(&[("x", Value::List(vec![int(1)]))]))]);
            assert_eq!(
                f_sigma(&list, &functor(&spec_identity_on_a())).unwrap(),
                list
            );
        }

        /// Delta records the row each copy copies, Sigma each fresh null's
        /// row and column, and Pi each row's tuple.
        #[test]
        fn provenance_records_copies_fresh_nulls_and_tuples() {
            let merge = spec(
                side(&[("a", &["x"]), ("b", &["y"])], &[]),
                side(&[("m", &["x", "y"])], &[]),
                &[("a", "m"), ("b", "m")],
                &[],
                &[(("a", "x"), "x"), (("b", "y"), "y")],
            );
            let f = functor(&merge);
            let j = one_table(
                "m",
                vec![
                    (0, row(&[("x", int(1)), ("y", int(2))])),
                    (1, row(&[("x", int(3)), ("y", int(4))])),
                ],
            );
            let (delta, copies) = f_delta_with_provenance(&j, &f).unwrap();
            let a: Name = "a".into();
            let b: Name = "b".into();
            assert_eq!(
                copies.copies,
                BTreeMap::from([
                    (RowId(0), (a.clone(), RowId(0))),
                    (RowId(1), (a, RowId(1))),
                    (RowId(2), (b.clone(), RowId(0))),
                    (RowId(3), (b, RowId(1))),
                ])
            );
            assert_eq!(delta.row(RowId(2)), Some(&row(&[("y", int(2))])));
            let (pi, tuples) = functor_pi_along_with_provenance(&delta, &f, 100).unwrap();
            assert_eq!(pi.row_count("m"), 4);
            for (id, (table, tuple)) in &tuples.tuples {
                assert_eq!(table, "m");
                let (ra, rb) = (delta.row(tuple[0]).unwrap(), delta.row(tuple[1]).unwrap());
                assert_eq!(
                    pi.row(*id),
                    Some(&row(&[("x", ra["x"].clone()), ("y", rb["y"].clone())]))
                );
            }
            let (_, fresh) = f_sigma_with_provenance(
                &one_table("a", vec![(5, row(&[("x", int(0)), ("gone", null(1))]))]),
                &functor(&widening()),
            )
            .unwrap();
            assert_eq!(
                fresh.fresh,
                BTreeMap::from([(2, (RowId(5), "y".to_owned()))])
            );
            let _ = attribute;
        }

        #[test]
        fn same_as_compares_null_maps_by_canonical_bytes() {
            let rows = BTreeMap::from([(RowId(0), RowId(1))]);
            let with =
                |v: Value| FInstanceHom::new(rows.clone()).with_null_map(BTreeMap::from([(0, v)]));
            let nan = with(Value::Float(f64::NAN));
            assert_ne!(
                nan,
                nan.clone(),
                "the derived == finds NaN unequal to itself"
            );
            assert!(nan.same_as(&nan.clone()));
            let (zero, minus_zero) = (with(Value::Float(0.0)), with(Value::Float(-0.0)));
            assert_eq!(zero, minus_zero, "the derived == finds 0.0 equal to -0.0");
            assert!(!zero.same_as(&minus_zero));
            // An identity entry is no entry.
            let mut identity = FInstanceHom::new(rows.clone());
            identity.null_map.insert(0, null(0));
            assert!(identity.same_as(&FInstanceHom::new(rows.clone())));
            assert!(!FInstanceHom::new(rows).same_as(&FInstanceHom::new(BTreeMap::new())));
        }

        /// Family (8'): floats are equal exactly when their bit patterns are.
        #[test]
        fn float_cells_compare_by_canonical_bytes() {
            let spec = equating();
            let f = functor(&spec);
            let float = Value::Float;
            let pair = |a: f64, b: f64| {
                one_table(
                    "t",
                    vec![(0, row(&[("a1", float(a)), ("a2", float(b)), ("c", int(0))]))],
                )
            };
            for (a, b) in [(0.0, -0.0), (f64::NAN, -f64::NAN)] {
                let x = pair(a, b);
                assert!(
                    matches!(
                        f_sigma(&x, &f),
                        Err(AdjunctionError::SigmaConstantClash { .. })
                    ),
                    "{a} and {b} clash"
                );
                assert!(oracle::sigma(&spec, &x, 0).is_err());
            }
            let x = pair(f64::NAN, f64::NAN);
            let sigma = f_sigma(&x, &f).unwrap();
            assert!(oracle::brute_iso(
                &sigma,
                &oracle::sigma(&spec, &x, 1 << 40).unwrap()
            ));
            let j = |b: f64| one_table("t", vec![(7, row(&[("b", float(b)), ("c", int(0))]))]);
            let homs = |b: f64| oracle::brute_homs(&x, &f_delta(&j(b), &f).unwrap()).len();
            assert_eq!(homs(f64::NAN), 1);
            assert_eq!(homs(-f64::NAN), 0);
        }

        #[test]
        fn nan_payloads_are_different_values() {
            let payload = f64::from_bits(f64::NAN.to_bits() | 1);
            assert!(payload.is_nan());
            let spec = equating();
            let x = one_table(
                "t",
                vec![(
                    0,
                    row(&[
                        ("a1", Value::Float(f64::NAN)),
                        ("a2", Value::Float(payload)),
                        ("c", int(0)),
                    ]),
                )],
            );
            assert!(matches!(
                f_sigma(&x, &functor(&spec)),
                Err(AdjunctionError::SigmaConstantClash { .. })
            ));
            let one = one_table("t", vec![(0, row(&[("b", Value::Float(f64::NAN))]))]);
            let other = one_table("t", vec![(0, row(&[("b", Value::Float(payload))]))]);
            assert!(oracle::brute_homs(&one, &other).is_empty());
            assert!(FInstanceHom::identity(&one).check(&one, &other).is_err());
            assert!(FInstanceHom::identity(&one).check(&one, &one).is_ok());
        }
    }
}
