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
            .find(|(_, (t, r))| {
                charge_copy_work(1);
                t.as_ref() == table && *r == row
            })
            .map(|(copy, _)| *copy)
    }

    /// Every copy keyed by its source table and the row it copies: the
    /// lookups of [`DeltaProvenance::copy_of`], each one hash lookup.
    fn copy_index(&self) -> HashMap<(Name, RowId), RowId> {
        charge_copy_work(self.copies.len());
        let mut index = HashMap::with_capacity(self.copies.len());
        for (copy, (table, row)) in &self.copies {
            index.entry((table.clone(), *row)).or_insert(*copy);
        }
        index
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
/// part `S'` of `functor`.
///
/// It holds the rows of `x` at kept tables with their cells at kept
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

#[cfg(test)]
thread_local! {
    /// The Kan extensions of whole instances taken on this thread: each
    /// `Sigma_F`, `Delta_F` and `Pi_F` counts one, so a test can bound how
    /// many an operation takes.
    static KAN_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// The copies of `Delta_F` results read on this thread to find a row's
    /// copy, so a test can bound how many a lookup takes.
    static COPY_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Count `n` copies read to find a row's copy on this thread.
#[cfg(test)]
fn charge_copy_work(n: usize) {
    COPY_WORK.with(|work| work.set(work.get().saturating_add(n)));
}

/// Count `n` copies read to find a row's copy: nothing outside tests.
#[cfg(not(test))]
const fn charge_copy_work(_n: usize) {}

/// Count one Kan extension of a whole instance on this thread.
#[cfg(test)]
fn charge_kan_work() {
    KAN_WORK.with(|work| work.set(work.get().saturating_add(1)));
}

/// Count one Kan extension of a whole instance: nothing outside tests.
#[cfg(not(test))]
const fn charge_kan_work() {}

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
    /// them (`a`'s first, boxed) and leave the classes apart.
    fn union(&mut self, a: &Value, b: &Value) -> Result<(), Box<(Value, Value)>> {
        let (a, b) = (self.insert(a), self.insert(b));
        let (ra, rb) = (self.root(a), self.root(b));
        if ra == rb {
            return Ok(());
        }
        if let (Some(x), Some(y)) = (&self.constant[ra], &self.constant[rb]) {
            return Err(Box::new((x.clone(), y.clone())));
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
    charge_kan_work();
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
                classes.union(first, other).map_err(|clash| {
                    let (left, right) = *clash;
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
            let value = if let Some(alpha) = functor.column_preimage(source, column).first() {
                classes.representative(cells.get(*alpha).unwrap_or(&Value::Null))
            } else {
                let fresh = next.ok_or(AdjunctionError::NullIdsExhausted)?;
                next = fresh.checked_add(1);
                provenance.fresh.insert(fresh, (id, column.clone()));
                Value::LabeledNull(fresh)
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

/// `Sigma_F` of `i`, an instance on the kept part `S'` of the source.
///
/// `i` is in the form [`f_restrict_to_domain`] gives, and fresh nulls are
/// numbered from the least id above `i`'s. This is the `Sigma_F` the units,
/// counits, transposes and [`f_sigma_on_hom`] use.
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
    charge_kan_work();
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
    charge_kan_work();
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

/// `hom` with its null map cut down to `nulls`, the nulls of its domain
/// ([`nulls_of`]), and without identity entries, which is the form
/// homomorphisms are compared in.
fn normalised(mut hom: FInstanceHom, nulls: &BTreeSet<u64>) -> FInstanceHom {
    let null_map = std::mem::take(&mut hom.null_map)
        .into_iter()
        .filter(|(null, _)| nulls.contains(null))
        .collect();
    hom.with_null_map(null_map)
}

/// `g` followed by `h`, normalised on the domain of `g`, whose nulls are
/// `nulls`.
///
/// When neither map renames a table or an edge, which holds for every map
/// the table adjunctions build, the composite is read off directly: each
/// row's image under `h` of its image under `g`, and each null of the
/// domain's image likewise. This is [`FInstanceHom::compose`] followed by
/// [`normalised`] without the null entries outside the domain that the
/// normalisation would drop.
fn then(
    g: &FInstanceHom,
    h: &FInstanceHom,
    nulls: &BTreeSet<u64>,
) -> Result<FInstanceHom, AdjunctionError> {
    let not_natural = || AdjunctionError::NotANaturalMap {
        row: g
            .row_map
            .iter()
            .find(|(_, mid)| h.image(**mid).is_none())
            .map_or(RowId(0), |(row, _)| *row),
    };
    let renames = |m: &FInstanceHom| !m.table_map.is_empty() || !m.edge_map.is_empty();
    if renames(g) || renames(h) {
        let composed = g.compose(h).map_err(|_| not_natural())?;
        return Ok(normalised(composed, nulls));
    }
    let row_map = g
        .row_map
        .iter()
        .map(|(&row, &mid)| h.image(mid).map(|image| (row, image)))
        .collect::<Option<BTreeMap<RowId, RowId>>>()
        .ok_or_else(not_natural)?;
    let null_map = nulls
        .iter()
        .map(|&null| {
            let own = Value::LabeledNull(null);
            (null, h.image_value(g.image_value(&own)).clone())
        })
        .collect();
    Ok(FInstanceHom::new(row_map).with_null_map(null_map))
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
    let index = copies.copy_index();
    let mut row_map = BTreeMap::new();
    for (table, rows) in i.tables() {
        for row in rows.ids() {
            let copy = *index
                .get(&(table.clone(), row))
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
    counit_hom(y, &sigma, &provenance, &copies).map_err(not_natural)
}

/// [`AdjunctionError::NotANaturalMap`] at `row`.
const fn not_natural(row: RowId) -> AdjunctionError {
    AdjunctionError::NotANaturalMap { row }
}

/// [`f_counit`] over `Delta_F J`'s copies and the `Sigma_F` of it, or the
/// row at which it is not natural.
fn counit_hom(
    y: &FInstance,
    sigma: &FInstance,
    provenance: &SigmaProvenance,
    copies: &DeltaProvenance,
) -> Result<FInstanceHom, RowId> {
    let row_map: BTreeMap<RowId, RowId> = copies
        .copies
        .iter()
        .map(|(copy, (_, original))| (*copy, *original))
        .collect();
    let mut null_map = provenance.merged.clone();
    for (null, (copy, column)) in &provenance.fresh {
        let original = row_map.get(copy).ok_or(*copy)?;
        let value = y
            .row(*original)
            .and_then(|cells| cells.get(column))
            .ok_or(*copy)?;
        null_map.insert(*null, value.clone());
    }
    Ok(normalised(
        FInstanceHom::new(row_map).with_null_map(null_map),
        &nulls_of(sigma),
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
    SigmaTransposeLeft::new(i, y, functor)?.apply(g)
}

/// [`f_transpose_left`] at fixed `I` and `J`: what does not depend on the
/// homomorphism (`Sigma_F I`, the unit, both `Delta_F`) is taken once, so
/// that transposing every homomorphism of a hom-set costs each one only its
/// own check and composition. The errors are those of [`f_transpose_left`],
/// in its order.
struct SigmaTransposeLeft<'a> {
    y: &'a FInstance,
    i_nulls: BTreeSet<u64>,
    sigma: FInstance,
    unit: FInstanceHom,
    delta_nulls: BTreeSet<u64>,
    copies: DeltaProvenance,
    index_prime: HashMap<(Name, RowId), RowId>,
}

impl<'a> SigmaTransposeLeft<'a> {
    fn new(
        i: &'a FInstance,
        y: &'a FInstance,
        functor: &MigrationFunctor,
    ) -> Result<Self, AdjunctionError> {
        let (sigma, provenance) = f_sigma_restricted(i, functor)?;
        let (_, unit_copies) = delta_on(&sigma, functor)?;
        let unit = unit_hom(i, &provenance, &unit_copies)?;
        let (delta, copies) = f_delta_with_provenance(&sigma, functor)?;
        let (_, copies_prime) = f_delta_with_provenance(y, functor)?;
        let index_prime = copies_prime.copy_index();
        Ok(Self {
            y,
            i_nulls: nulls_of(i),
            sigma,
            unit,
            delta_nulls: nulls_of(&delta),
            copies,
            index_prime,
        })
    }

    /// `Delta_F(g) . eta_I` for `g: Sigma_F I -> J`.
    fn apply(&self, g: &FInstanceHom) -> Result<FInstanceHom, AdjunctionError> {
        require_hom(g, &self.sigma, self.y)?;
        let delta_g = delta_hom(g, &self.delta_nulls, &self.copies, &self.index_prime)?;
        then(&self.unit, &delta_g, &self.i_nulls)
    }
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
    SigmaTransposeRight::new(i, y, functor)?.apply(f)
}

/// [`f_transpose_right`] at fixed `I` and `J`: `Sigma_F I`, `Delta_F J` and
/// `Sigma_F(Delta_F J)` are taken once. The errors are those of
/// [`f_transpose_right`], in its order.
struct SigmaTransposeRight<'a> {
    i: &'a FInstance,
    sigma: FInstance,
    sigma_nulls: BTreeSet<u64>,
    provenance: SigmaProvenance,
    delta: FInstance,
    sigma_delta: FInstance,
    provenance_delta: SigmaProvenance,
    /// The counit `eps_J`, or the row at which it is not natural; its
    /// error is raised after the homomorphism's own, as
    /// [`f_transpose_right`] raises it.
    counit: Result<FInstanceHom, RowId>,
}

impl<'a> SigmaTransposeRight<'a> {
    fn new(
        i: &'a FInstance,
        y: &'a FInstance,
        functor: &MigrationFunctor,
    ) -> Result<Self, AdjunctionError> {
        let (sigma, provenance) = f_sigma_restricted(i, functor)?;
        let (delta, copies) = f_delta_with_provenance(y, functor)?;
        let (sigma_delta, provenance_delta) = sigma_on(&delta, functor, null_floor(&delta))?;
        let counit = counit_hom(y, &sigma_delta, &provenance_delta, &copies);
        Ok(Self {
            i,
            sigma_nulls: nulls_of(&sigma),
            sigma,
            provenance,
            delta,
            sigma_delta,
            provenance_delta,
            counit,
        })
    }

    /// `eps_J . Sigma_F(f)` for `f: I -> Delta_F J`.
    fn apply(&self, f: &FInstanceHom) -> Result<FInstanceHom, AdjunctionError> {
        require_hom(f, self.i, &self.delta)?;
        let sigma_f = sigma_hom(
            f,
            &self.sigma,
            &self.provenance,
            &self.sigma_delta,
            &self.provenance_delta,
        )?;
        let counit = self.counit.as_ref().map_err(|row| not_natural(*row))?;
        then(&sigma_f, counit, &self.sigma_nulls)
    }
}

/// `Delta_F(h)`: the image under `Delta_F` of a homomorphism `h: J -> J'` of
/// instances on `T`.
///
/// It sends the copy at `s` of a row `r` to the copy at `s` of `h(r)`, with
/// `h`'s null map on the nulls of `Delta_F J`.
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
    delta_hom(h, &nulls_of(&delta), &copies, &copies_prime.copy_index())
}

/// [`f_delta_on_hom`] over the nulls and copies of `Delta_F J` and the index
/// of the copies of `Delta_F J'` ([`DeltaProvenance::copy_index`]).
fn delta_hom(
    h: &FInstanceHom,
    delta_nulls: &BTreeSet<u64>,
    copies: &DeltaProvenance,
    index_prime: &HashMap<(Name, RowId), RowId>,
) -> Result<FInstanceHom, AdjunctionError> {
    let mut row_map = BTreeMap::new();
    for (copy, (table, original)) in &copies.copies {
        let image = h
            .image(*original)
            .and_then(|image| index_prime.get(&(table.clone(), image)).copied())
            .ok_or(AdjunctionError::NotANaturalMap { row: *copy })?;
        row_map.insert(*copy, image);
    }
    let null_map = h
        .null_map
        .iter()
        .filter(|(null, _)| delta_nulls.contains(null))
        .map(|(null, value)| (*null, value.clone()))
        .collect();
    Ok(FInstanceHom::new(row_map).with_null_map(null_map))
}

/// `Pi_F(h)`: the image under `Pi_F` of a homomorphism `h: I -> I'` of
/// instances on `S'`.
///
/// It sends the row of each tuple to the row of the tuple of its
/// components' images (the one row of a table outside the image to the one
/// row), with `h`'s null map on the nulls of `Pi_F I`.
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
    pi_hom(h, &pi, &nulls_of(&pi.instance), &pi_prime)
}

/// [`f_pi_on_hom`] over the two limits already taken and the nulls of the
/// first.
fn pi_hom(
    h: &FInstanceHom,
    pi: &PiLimit,
    pi_nulls: &BTreeSet<u64>,
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
    let null_map = h
        .null_map
        .iter()
        .filter(|(null, _)| pi_nulls.contains(null))
        .map(|(null, value)| (*null, value.clone()))
        .collect();
    Ok(FInstanceHom::new(row_map).with_null_map(null_map))
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
    pi_unit_hom(y, functor, &copies, &pi).map_err(not_natural)
}

/// [`f_pi_unit`] over `Delta_F J`'s copies and the limit of it, or the row
/// at which it is not natural.
fn pi_unit_hom(
    y: &FInstance,
    functor: &MigrationFunctor,
    copies: &DeltaProvenance,
    pi: &PiLimit,
) -> Result<FInstanceHom, RowId> {
    let index = copies.copy_index();
    let mut row_map = BTreeMap::new();
    for (table, rows) in y.tables() {
        let fibre = functor.table_fibre(table);
        for row in rows.ids() {
            let tuple: Option<Vec<RowId>> = fibre
                .iter()
                .map(|source| index.get(&((*source).clone(), row)).copied())
                .collect();
            let image = tuple
                .and_then(|tuple| pi.rows.get(&(table.clone(), tuple)))
                .ok_or(row)?;
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
    pi_counit_hom(functor, &copies, &pi).map_err(not_natural)
}

/// [`f_pi_counit`] over the limit and the copies of its `Delta_F`, or the
/// row at which it is not natural.
fn pi_counit_hom(
    functor: &MigrationFunctor,
    copies: &DeltaProvenance,
    pi: &PiLimit,
) -> Result<FInstanceHom, RowId> {
    let mut row_map = BTreeMap::new();
    for (&copy, (source, original)) in &copies.copies {
        let component = pi.tuples.get(original).and_then(|(table, tuple)| {
            let position = functor
                .table_fibre(table)
                .iter()
                .position(|component| *component == source)?;
            tuple.get(position).copied()
        });
        let component = component.ok_or(copy)?;
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
    PiTransposeLeft::new(y, i, functor, max_product_size)?.apply(g)
}

/// [`f_pi_transpose_left`] at fixed `J` and `I`: `Delta_F J` and the limits
/// `Pi_F(Delta_F J)` and `Pi_F I` are taken once. The errors are those of
/// [`f_pi_transpose_left`], in its order.
struct PiTransposeLeft<'a> {
    y_nulls: BTreeSet<u64>,
    i: &'a FInstance,
    delta: FInstance,
    pi_delta: PiLimit,
    pi_delta_nulls: BTreeSet<u64>,
    pi: PiLimit,
    /// The unit at `J`, or the row at which it is not natural; its error
    /// is raised after the homomorphism's own, as [`f_pi_transpose_left`]
    /// raises it.
    unit: Result<FInstanceHom, RowId>,
}

impl<'a> PiTransposeLeft<'a> {
    fn new(
        y: &'a FInstance,
        i: &'a FInstance,
        functor: &MigrationFunctor,
        max_product_size: usize,
    ) -> Result<Self, AdjunctionError> {
        let (delta, copies) = f_delta_with_provenance(y, functor)?;
        let pi_delta = pi_on_domain(&delta, functor, max_product_size)?;
        let pi = pi_on_domain(i, functor, max_product_size)?;
        let unit = pi_unit_hom(y, functor, &copies, &pi_delta);
        Ok(Self {
            y_nulls: nulls_of(y),
            i,
            delta,
            pi_delta_nulls: nulls_of(&pi_delta.instance),
            pi_delta,
            pi,
            unit,
        })
    }

    /// `Pi_F(g) . unit_J` for `g: Delta_F J -> I`.
    fn apply(&self, g: &FInstanceHom) -> Result<FInstanceHom, AdjunctionError> {
        require_hom(g, &self.delta, self.i)?;
        let unit = self.unit.as_ref().map_err(|row| not_natural(*row))?;
        let pi_g = pi_hom(g, &self.pi_delta, &self.pi_delta_nulls, &self.pi)?;
        then(unit, &pi_g, &self.y_nulls)
    }
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
    PiTransposeRight::new(y, i, functor, max_product_size)?.apply(f)
}

/// [`f_pi_transpose_right`] at fixed `J` and `I`: `Pi_F I`, `Delta_F J` and
/// `Delta_F(Pi_F I)` are taken once. The errors are those of
/// [`f_pi_transpose_right`], in its order.
struct PiTransposeRight<'a> {
    y: &'a FInstance,
    pi: PiLimit,
    delta_nulls: BTreeSet<u64>,
    copies: DeltaProvenance,
    index_pi: HashMap<(Name, RowId), RowId>,
    /// The counit at `I`, or the row at which it is not natural; its error
    /// is raised after the homomorphism's own, as [`f_pi_transpose_right`]
    /// raises it.
    counit: Result<FInstanceHom, RowId>,
}

impl<'a> PiTransposeRight<'a> {
    fn new(
        y: &'a FInstance,
        i: &FInstance,
        functor: &MigrationFunctor,
        max_product_size: usize,
    ) -> Result<Self, AdjunctionError> {
        let pi = pi_on_domain(i, functor, max_product_size)?;
        let (delta, copies) = f_delta_with_provenance(y, functor)?;
        let (_, copies_pi) = f_delta_with_provenance(&pi.instance, functor)?;
        let counit = pi_counit_hom(functor, &copies_pi, &pi);
        let index_pi = copies_pi.copy_index();
        Ok(Self {
            y,
            pi,
            delta_nulls: nulls_of(&delta),
            copies,
            index_pi,
            counit,
        })
    }

    /// `counit_I . Delta_F(f)` for `f: J -> Pi_F I`.
    fn apply(&self, f: &FInstanceHom) -> Result<FInstanceHom, AdjunctionError> {
        require_hom(f, self.y, &self.pi.instance)?;
        let delta_f = delta_hom(f, &self.delta_nulls, &self.copies, &self.index_pi)?;
        let counit = self.counit.as_ref().map_err(|row| not_natural(*row))?;
        then(&delta_f, counit, &self.delta_nulls)
    }
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
        pub(super) struct Side {
            /// Each table with its column names.
            pub(super) tables: BTreeMap<Name, BTreeSet<String>>,
            /// The relation objects.
            pub(super) relations: BTreeSet<Edge>,
        }

        /// A functor `F: S' -> T`, given by its maps on the kept tables,
        /// relations and columns of the source.
        #[derive(Clone, Debug, Default)]
        pub(super) struct Spec {
            /// The whole source `S`.
            pub(super) src: Side,
            /// The target `T`.
            pub(super) tgt: Side,
            /// Each kept table with its image.
            pub(super) tables: BTreeMap<Name, Name>,
            /// Each kept relation with its image.
            pub(super) relations: BTreeMap<Edge, Edge>,
            /// Each kept column `(table, column)` with its image column.
            pub(super) columns: BTreeMap<(Name, String), String>,
        }

        impl Spec {
            /// The kept tables sent to `d`, ascending.
            pub(super) fn fibre(&self, d: &Name) -> Vec<Name> {
                self.tables
                    .iter()
                    .filter(|(_, image)| *image == d)
                    .map(|(s, _)| s.clone())
                    .collect()
            }

            /// The kept relations sent to `e`, ascending.
            pub(super) fn relation_fibre(&self, e: &Edge) -> Vec<Edge> {
                self.relations
                    .iter()
                    .filter(|(_, image)| *image == e)
                    .map(|(s, _)| s.clone())
                    .collect()
            }

            /// The kept columns of `s`, ascending.
            pub(super) fn kept_columns(&self, s: &Name) -> Vec<String> {
                self.columns
                    .keys()
                    .filter(|(t, _)| t == s)
                    .map(|(_, c)| c.clone())
                    .collect()
            }

            /// The kept columns of `s` sent to `beta`, ascending.
            pub(super) fn preimage(&self, s: &Name, beta: &str) -> Vec<String> {
                self.columns
                    .iter()
                    .filter(|((t, _), image)| t == s && image.as_str() == beta)
                    .map(|((_, c), _)| c.clone())
                    .collect()
            }

            /// Whether every column of every target table is the image of a
            /// kept column of its fibre; else the least missing one.
            pub(super) fn missing_column(&self) -> Option<(Name, String)> {
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
        pub(super) fn canon(value: &Value) -> Vec<u8> {
            value.canonical_bytes()
        }

        /// The labelled nulls in the cells of an instance.
        pub(super) fn nulls(instance: &FInstance) -> BTreeSet<u64> {
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
        pub(super) type Found = (BTreeMap<RowId, RowId>, BTreeMap<u64, Value>);

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
        pub(super) fn brute_accepts(
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
        pub(super) fn candidates(a: &FInstance, b: &FInstance) -> Vec<BTreeMap<RowId, RowId>> {
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
        pub(super) fn normalise(nu: BTreeMap<u64, Value>) -> BTreeMap<u64, Value> {
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
        pub(super) fn brute_homs(a: &FInstance, b: &FInstance) -> Vec<Found> {
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

        /// Whether `a` and `b` are isomorphic, read off the definition: some
        /// homomorphism `a -> b` is a bijection on rows and on nulls, and
        /// its inverse is a homomorphism `b -> a`. It lists every
        /// homomorphism, so it is only for small instances; [`brute_iso`]
        /// is checked against it.
        pub(super) fn brute_iso_by_homs(a: &FInstance, b: &FInstance) -> bool {
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

        /// A cell as the isomorphism search reads it: a labelled null, or a
        /// constant by its canonical bytes.
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum Cell {
            Null(u64),
            Constant(Vec<u8>),
        }

        /// An instance indexed for the isomorphism search.
        struct Indexed<'x> {
            /// The rows, ascending.
            rows: Vec<RowId>,
            /// Each row's table.
            table: Vec<&'x Name>,
            /// Each row's cells, by column.
            cells: Vec<Vec<(&'x String, Cell)>>,
            /// Each row's outgoing pairs: relation and target row (index).
            out: Vec<BTreeSet<(&'x Edge, usize)>>,
            /// Each row's incoming pairs: relation and source row (index).
            inc: Vec<BTreeSet<(&'x Edge, usize)>>,
            /// The nulls, ascending.
            nulls: Vec<u64>,
            /// Each null's cells: row (index) and column.
            held: Vec<Vec<(usize, &'x String)>>,
        }

        impl<'x> Indexed<'x> {
            fn of(x: &'x FInstance) -> Self {
                let rows = x.row_ids();
                let at: BTreeMap<RowId, usize> =
                    rows.iter().enumerate().map(|(n, r)| (*r, n)).collect();
                let table = rows
                    .iter()
                    .map(|r| x.table_of(*r).expect("a listed row has a table"))
                    .collect();
                let cells: Vec<Vec<(&String, Cell)>> = rows
                    .iter()
                    .map(|r| {
                        let mut cells: Vec<(&String, Cell)> = x
                            .row(*r)
                            .expect("a listed row has cells")
                            .iter()
                            .map(|(c, v)| {
                                let cell = v
                                    .as_labeled_null()
                                    .map_or_else(|| Cell::Constant(canon(v)), Cell::Null);
                                (c, cell)
                            })
                            .collect();
                        cells.sort();
                        cells
                    })
                    .collect();
                let mut out = vec![BTreeSet::new(); rows.len()];
                let mut inc = vec![BTreeSet::new(); rows.len()];
                for (edge, pairs) in x.foreign_keys() {
                    for (s, t) in pairs {
                        let (s, t) = (at[s], at[t]);
                        out[s].insert((edge, t));
                        inc[t].insert((edge, s));
                    }
                }
                let mut by_null: BTreeMap<u64, Vec<(usize, &String)>> = BTreeMap::new();
                for (n, row) in cells.iter().enumerate() {
                    for (c, cell) in row {
                        if let Cell::Null(m) = cell {
                            by_null.entry(*m).or_default().push((n, *c));
                        }
                    }
                }
                let (nulls, held) = by_null.into_iter().unzip();
                Self {
                    rows,
                    table,
                    cells,
                    out,
                    inc,
                    nulls,
                    held,
                }
            }

            /// Whether rows `y` and `z` are twins: one table, equal cells
            /// (the same nulls), and the same pairs to every other row, so
            /// that swapping them is an automorphism fixing every other row
            /// and every null.
            fn twins(&self, y: usize, z: usize) -> bool {
                let swap = |w: usize| match w {
                    w if w == y => z,
                    w if w == z => y,
                    w => w,
                };
                let swapped = |pairs: &BTreeSet<(&'x Edge, usize)>| -> BTreeSet<(&'x Edge, usize)> {
                    pairs.iter().map(|(e, w)| (*e, swap(*w))).collect()
                };
                self.table[y] == self.table[z]
                    && self.cells[y] == self.cells[z]
                    && swapped(&self.out[y]) == self.out[z]
                    && swapped(&self.inc[y]) == self.inc[z]
            }
        }

        /// Intern `key` in `ids`, returning its id.
        fn intern<K: Ord>(ids: &mut BTreeMap<K, usize>, key: K) -> usize {
            let next = ids.len();
            *ids.entry(key).or_insert(next)
        }

        /// The stable colours of the rows and nulls of `a` and `b`, refined
        /// together so their ids compare across the two: a row starts from
        /// its table and its cells with every null read as one marker, and
        /// each round adds the colours of its pair neighbours (by relation
        /// and direction) and of the nulls in its cells (by column); a null's
        /// colour adds the colours and columns of the cells holding it.
        fn colours(a: &Indexed<'_>, b: &Indexed<'_>) -> [(Vec<usize>, Vec<usize>); 2] {
            /// A row's table and its cells, every null read as one marker.
            type Shape = (Name, Vec<(String, Option<Vec<u8>>)>);
            let mut start: BTreeMap<Shape, usize> = BTreeMap::new();
            let mut first = |x: &Indexed<'_>| -> Vec<usize> {
                (0..x.rows.len())
                    .map(|n| {
                        let shape = x.cells[n]
                            .iter()
                            .map(|(c, cell)| match cell {
                                Cell::Null(_) => ((*c).clone(), None),
                                Cell::Constant(bytes) => ((*c).clone(), Some(bytes.clone())),
                            })
                            .collect();
                        intern(&mut start, (x.table[n].clone(), shape))
                    })
                    .collect()
            };
            let mut rows = [first(a), first(b)];
            let mut nulls = [vec![0; a.nulls.len()], vec![0; b.nulls.len()]];
            let sides = [a, b];
            let classes = |rows: &[Vec<usize>; 2], nulls: &[Vec<usize>; 2]| {
                let r: BTreeSet<usize> = rows.iter().flatten().copied().collect();
                let n: BTreeSet<usize> = nulls.iter().flatten().copied().collect();
                r.len() + n.len()
            };
            let mut seen = classes(&rows, &nulls);
            loop {
                let mut null_ids = BTreeMap::new();
                let next_nulls: [Vec<usize>; 2] = [0, 1].map(|s| {
                    let x = sides[s];
                    (0..x.nulls.len())
                        .map(|m| {
                            let mut around: Vec<(usize, &String)> =
                                x.held[m].iter().map(|(r, c)| (rows[s][*r], *c)).collect();
                            around.sort();
                            intern(&mut null_ids, (nulls[s][m], around))
                        })
                        .collect()
                });
                let mut row_ids = BTreeMap::new();
                let next_rows: [Vec<usize>; 2] = [0, 1].map(|s| {
                    let x = sides[s];
                    let null_at: BTreeMap<u64, usize> =
                        x.nulls.iter().enumerate().map(|(m, n)| (*n, m)).collect();
                    (0..x.rows.len())
                        .map(|r| {
                            let mut around: Vec<(&Edge, bool, usize)> = x.out[r]
                                .iter()
                                .map(|(e, t)| (*e, true, rows[s][*t]))
                                .chain(x.inc[r].iter().map(|(e, t)| (*e, false, rows[s][*t])))
                                .collect();
                            around.sort();
                            let held: Vec<(&String, usize)> = x.cells[r]
                                .iter()
                                .filter_map(|(c, cell)| match cell {
                                    Cell::Null(n) => Some((*c, nulls[s][null_at[n]])),
                                    Cell::Constant(_) => None,
                                })
                                .collect();
                            intern(&mut row_ids, (rows[s][r], around, held))
                        })
                        .collect()
                });
                rows = next_rows;
                nulls = next_nulls;
                let now = classes(&rows, &nulls);
                if now == seen {
                    break;
                }
                seen = now;
            }
            let [ra, rb] = rows;
            let [na, nb] = nulls;
            [(ra, na), (rb, nb)]
        }

        /// The state of the isomorphism search: the rows and nulls of `a`
        /// assigned so far, with their inverses.
        struct Search<'s, 'x> {
            a: &'s Indexed<'x>,
            b: &'s Indexed<'x>,
            /// Each row of `a` with its image, when assigned.
            sigma: Vec<Option<usize>>,
            /// Each row of `b` with its preimage, when assigned.
            sigma_inv: Vec<Option<usize>>,
            /// Each null of `a` with its image, when fixed.
            tau: BTreeMap<u64, u64>,
            /// Each null of `b` with its preimage, when fixed.
            tau_inv: BTreeMap<u64, u64>,
        }

        impl Search<'_, '_> {
            /// Assign row `row` of `a` to row `target` of `b` when their
            /// cells agree under the nulls fixed so far (fixing the new ones)
            /// and their pairs to the rows assigned so far agree both ways;
            /// returns the nulls it fixed, or `None` (with nothing changed)
            /// when they do not.
            fn assign(&mut self, row: usize, target: usize) -> Option<Vec<u64>> {
                let (a, b) = (self.a, self.b);
                if a.table[row] != b.table[target] || a.cells[row].len() != b.cells[target].len() {
                    return None;
                }
                let image = |w: usize, sigma: &[Option<usize>]| {
                    if w == row { Some(target) } else { sigma[w] }
                };
                let preimage =
                    |w: usize, inv: &[Option<usize>]| if w == target { Some(row) } else { inv[w] };
                let forward = a.out[row].iter().all(|(e, t)| {
                    image(*t, &self.sigma).is_none_or(|it| b.out[target].contains(&(*e, it)))
                }) && a.inc[row].iter().all(|(e, s)| {
                    image(*s, &self.sigma).is_none_or(|is| b.inc[target].contains(&(*e, is)))
                });
                let backward = b.out[target].iter().all(|(e, t)| {
                    preimage(*t, &self.sigma_inv).is_none_or(|pt| a.out[row].contains(&(*e, pt)))
                }) && b.inc[target].iter().all(|(e, s)| {
                    preimage(*s, &self.sigma_inv).is_none_or(|ps| a.inc[row].contains(&(*e, ps)))
                });
                if !forward || !backward {
                    return None;
                }
                let mut fixed: Vec<u64> = Vec::new();
                for ((ca, va), (cb, vb)) in a.cells[row].iter().zip(&b.cells[target]) {
                    let agrees = ca == cb
                        && match (va, vb) {
                            (Cell::Constant(x), Cell::Constant(z)) => x == z,
                            (Cell::Null(n), Cell::Null(m)) => {
                                match (self.tau.get(n), self.tau_inv.get(m)) {
                                    (Some(fm), Some(fn_)) => fm == m && fn_ == n,
                                    (None, None) => {
                                        self.tau.insert(*n, *m);
                                        self.tau_inv.insert(*m, *n);
                                        fixed.push(*n);
                                        true
                                    }
                                    _ => false,
                                }
                            }
                            _ => false,
                        };
                    if !agrees {
                        self.unfix(&fixed);
                        return None;
                    }
                }
                self.sigma[row] = Some(target);
                self.sigma_inv[target] = Some(row);
                Some(fixed)
            }

            /// Undo the nulls fixed by one assignment.
            fn unfix(&mut self, fixed: &[u64]) {
                for n in fixed {
                    if let Some(m) = self.tau.remove(n) {
                        self.tau_inv.remove(&m);
                    }
                }
            }

            /// Undo the assignment of row `r` of `a` to `y`.
            fn unassign(&mut self, r: usize, y: usize, fixed: &[u64]) {
                self.sigma[r] = None;
                self.sigma_inv[y] = None;
                self.unfix(fixed);
            }

            /// Whether the rows `order[level..]` can be assigned, each to an
            /// unused row of `b` with its colour, trying one row of each
            /// class of twins among the candidates.
            fn extend(&mut self, level: usize, order: &[usize], colours: &[Vec<usize>; 2]) -> bool {
                let Some(&r) = order.get(level) else {
                    return true;
                };
                let mut tried: Vec<usize> = Vec::new();
                for y in 0..self.b.rows.len() {
                    if self.sigma_inv[y].is_some()
                        || colours[1][y] != colours[0][r]
                        || tried.iter().any(|&z| self.b.twins(z, y))
                    {
                        continue;
                    }
                    tried.push(y);
                    let Some(fixed) = self.assign(r, y) else {
                        continue;
                    };
                    if self.extend(level + 1, order, colours) {
                        return true;
                    }
                    self.unassign(r, y, &fixed);
                }
                false
            }
        }

        /// Whether `a` and `b` are isomorphic: a bijection `sigma` on rows
        /// that keeps each row's table and a bijection `tau` from the nulls
        /// of `a` onto those of `b` such that each row `sigma(r)` holds the
        /// cells of `r` with each null `n` read as `tau(n)` and each constant
        /// equal by canonical bytes, and `sigma` sends the pairs of `a` at
        /// each relation onto those of `b`. This is [`brute_iso_by_homs`]
        /// (a homomorphism bijective on rows and nulls whose inverse is a
        /// homomorphism) searched for directly, and is checked against it.
        ///
        /// Rows of `a` are assigned one at a time, each only to an unused
        /// row of `b` of its colour (see [`colours`]; an isomorphism keeps
        /// every colour), and an assignment whose cells, nulls or pairs to
        /// the rows assigned so far disagree is dropped at once. Of the
        /// candidate rows of `b` that are twins, only the first is tried:
        /// swapping two unused twins is an automorphism of `b` that fixes
        /// everything assigned, so the second leads where the first did.
        /// Rows are taken most-constrained first: the one with the most
        /// pairs and nulls shared with the rows already taken, then the one
        /// of the least colour class.
        pub(super) fn brute_iso(a: &FInstance, b: &FInstance) -> bool {
            let (xa, xb) = (Indexed::of(a), Indexed::of(b));
            if xa.rows.len() != xb.rows.len() || xa.nulls.len() != xb.nulls.len() {
                return false;
            }
            let [(rows_a, nulls_a), (rows_b, nulls_b)] = colours(&xa, &xb);
            let census = |colours: &[usize]| -> BTreeMap<usize, usize> {
                let mut counts = BTreeMap::new();
                for c in colours {
                    *counts.entry(*c).or_insert(0) += 1;
                }
                counts
            };
            if census(&rows_a) != census(&rows_b) || census(&nulls_a) != census(&nulls_b) {
                return false;
            }
            let class = census(&rows_a);
            // Rows of `a` that share a pair or a null with each row.
            let mut linked: Vec<BTreeSet<usize>> = (0..xa.rows.len())
                .map(|r| {
                    xa.out[r]
                        .iter()
                        .chain(&xa.inc[r])
                        .map(|(_, w)| *w)
                        .collect()
                })
                .collect();
            for held in &xa.held {
                for (r, _) in held {
                    linked[*r].extend(held.iter().map(|(w, _)| *w));
                }
            }
            let mut order: Vec<usize> = Vec::with_capacity(xa.rows.len());
            let mut taken = vec![false; xa.rows.len()];
            while order.len() < xa.rows.len() {
                let next = (0..xa.rows.len())
                    .filter(|r| !taken[*r])
                    .min_by_key(|r| {
                        let ties = linked[*r].iter().filter(|w| taken[**w]).count();
                        (std::cmp::Reverse(ties), class[&rows_a[*r]], *r)
                    })
                    .expect("a row is left to take");
                taken[next] = true;
                order.push(next);
            }
            let mut search = Search {
                a: &xa,
                b: &xb,
                sigma: vec![None; xa.rows.len()],
                sigma_inv: vec![None; xb.rows.len()],
                tau: BTreeMap::new(),
                tau_inv: BTreeMap::new(),
            };
            let found = search.extend(0, &order, &[rows_a, rows_b]);
            // Every null of `a` is in some cell, so a full assignment fixes
            // `tau` on all of them, onto the nulls of `b` (equal in number).
            found && search.tau.len() == xa.nulls.len()
        }

        /// `rho_{S'}(x)`: the rows at kept tables with their cells at kept
        /// columns, and the pairs at kept relations.
        pub(super) fn restrict(spec: &Spec, x: &FInstance) -> FInstance {
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

        /// A pair of copies, each named by its table and the row it copies.
        pub(super) type CopyPair = ((Name, RowId), (Name, RowId));

        /// A pair of tuples.
        pub(super) type TuplePair = (Vec<RowId>, Vec<RowId>);

        /// The Delta oracle: the copy of each row `r'` of `J(F s)` at every
        /// kept table `s`, keyed by `(s, r')`, with its cells, and the pairs
        /// of each kept relation between copies.
        pub(super) struct DeltaOracle {
            /// Each copy's cells.
            pub(super) copies: BTreeMap<(Name, RowId), Row>,
            /// Each kept relation's pairs between copies.
            pub(super) pairs: BTreeMap<Edge, BTreeSet<CopyPair>>,
        }

        /// [`DeltaOracle`] by the direct formula.
        pub(super) fn delta(spec: &Spec, j: &FInstance) -> DeltaOracle {
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
        pub(super) struct PiOracle {
            /// Each target table's tuples with their cells.
            pub(super) rows: BTreeMap<Name, BTreeMap<Vec<RowId>, Row>>,
            /// Each target relation's pairs of tuples.
            pub(super) pairs: BTreeMap<Edge, BTreeSet<TuplePair>>,
        }

        /// [`PiOracle`] over `i`, an instance on `S'`.
        pub(super) fn pi(spec: &Spec, i: &FInstance) -> PiOracle {
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
        pub(super) fn sigma(
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
        pub(super) fn attribute(table: &str, column: &str) -> Edge {
            Edge {
                src: table.into(),
                tgt: format!("{table}.{column}").into(),
                kind: "prop".into(),
                name: Some(column.into()),
            }
        }

        /// A relation `src -> tgt` named `name`.
        pub(super) fn fk(src: &str, tgt: &str, name: &str) -> Edge {
            Edge {
                src: src.into(),
                tgt: tgt.into(),
                kind: "fk".into(),
                name: Some(name.into()),
            }
        }

        /// One side of a spec.
        pub(super) fn side(tables: &[(&str, &[&str])], relations: &[Edge]) -> Side {
            Side {
                tables: tables
                    .iter()
                    .map(|(t, cs)| (Name::from(*t), cs.iter().map(|c| (*c).to_owned()).collect()))
                    .collect(),
                relations: relations.iter().cloned().collect(),
            }
        }

        /// A spec from its two sides and its maps.
        pub(super) fn spec(
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
        pub(super) fn schema_of(side: &Side) -> Schema {
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
        pub(super) fn protocol() -> Protocol {
            Protocol {
                value_kinds: BTreeMap::from([("integer".to_owned(), ScalarKind::Any)]),
                non_value_kinds: BTreeSet::from(["table".to_owned()]),
                ..Protocol::default()
            }
        }

        /// The migration stating the spec's functor.
        pub(super) fn migration_of(spec: &Spec) -> CompiledMigration {
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
        pub(super) fn functor(spec: &Spec) -> MigrationFunctor {
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
        pub(super) fn row(cells: &[(&str, Value)]) -> Row {
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
                        if *row == RowId(1)
                            && column == "b"
                            && **left == int(1)
                            && **right == int(2)
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

    /// The laws of the table Kan extensions, each on its stated domain,
    /// checked against the oracles: exhaustively on the tiny family, and on
    /// generated cases.
    mod laws {
        use std::collections::{BTreeMap, BTreeSet};

        use panproto_gat::Name;
        use panproto_schema::Edge;

        use super::fixture;
        use super::oracle::{self, Found, Side, Spec};
        use crate::adjunction::{
            AdjunctionError, f_counit, f_delta, f_delta_on_hom, f_delta_with_provenance,
            f_pi_counit, f_pi_on_hom, f_pi_transpose_left, f_pi_transpose_right, f_pi_unit,
            f_sigma_on_hom, f_sigma_restricted, f_transpose_left, f_transpose_right, f_unit,
        };
        use crate::adjunction::{
            PiTransposeLeft, PiTransposeRight, SigmaTransposeLeft, SigmaTransposeRight,
        };
        use crate::error::RestrictError;
        use crate::functor::{FInstance, Row};
        use crate::instance_hom::FInstanceHom;
        use crate::migration_functor::MigrationFunctor;
        use crate::pi::functor_pi_restricted;
        use crate::value::Value;
        use crate::view_ref::RowId;

        /// A brute-force homomorphism as an [`FInstanceHom`].
        fn hom(found: &Found) -> FInstanceHom {
            FInstanceHom::new(found.0.clone()).with_null_map(found.1.clone())
        }

        /// `g` then `h`, its null map cut to the nulls of `dom`.
        fn compose(g: &FInstanceHom, h: &FInstanceHom, dom: &FInstance) -> FInstanceHom {
            let composed = g.compose(h).unwrap_or_else(|e| panic!("{e}"));
            let nulls = oracle::nulls(dom);
            let null_map = composed
                .null_map
                .into_iter()
                .filter(|(n, _)| nulls.contains(n))
                .collect();
            FInstanceHom::new(composed.row_map).with_null_map(null_map)
        }

        /// A homomorphism as the key [`FInstanceHom::same_as`] compares: its
        /// row map, and its null map's non-identity entries by canonical
        /// bytes.
        type HomKey = (BTreeMap<RowId, RowId>, BTreeMap<u64, Vec<u8>>);

        fn key(h: &FInstanceHom) -> HomKey {
            let nulls = h
                .null_map
                .iter()
                .map(|(n, v)| (*n, oracle::canon(v)))
                .filter(|(n, bytes)| *bytes != oracle::canon(&Value::LabeledNull(*n)))
                .collect();
            (h.row_map.clone(), nulls)
        }

        /// The keys of a list of homomorphisms.
        fn index(list: &[Found]) -> BTreeSet<HomKey> {
            list.iter().map(|found| key(&hom(found))).collect()
        }

        /// Whether the indexed list holds a homomorphism equal to `h`.
        fn holds(list: &BTreeSet<HomKey>, h: &FInstanceHom) -> bool {
            list.contains(&key(h))
        }

        /// The first way an instance fails to conform to the tables, columns
        /// and relations given (rows ascending; for a row: table, then the
        /// least missing or stray column, then the least column holding a
        /// nested null; a pair outside the relations at its source row,
        /// after that row's own violation).
        fn first_violation(
            tables: &BTreeMap<Name, BTreeSet<String>>,
            relations: &BTreeSet<Edge>,
            inst: &FInstance,
        ) -> Option<(RowId, Option<String>)> {
            fn nested(v: &Value) -> bool {
                fn any(v: &Value) -> bool {
                    matches!(v, Value::LabeledNull(_)) || nested(v)
                }
                match v {
                    Value::List(items) => items.iter().any(any),
                    Value::Unknown(fields) | Value::Opaque { fields, .. } => {
                        fields.values().any(any)
                    }
                    _ => false,
                }
            }
            let mut found: Option<(RowId, Option<String>)> = None;
            for id in inst.row_ids() {
                let table = inst.table_of(id)?;
                let Some(columns) = tables.get(table) else {
                    found = Some((id, None));
                    break;
                };
                let cells = inst.row(id)?;
                let keys: BTreeSet<&String> = cells.keys().collect();
                let wrong = columns
                    .iter()
                    .filter(|c| !keys.contains(c))
                    .chain(keys.iter().copied().filter(|k| !columns.contains(*k)))
                    .min();
                if let Some(column) = wrong {
                    found = Some((id, Some(column.clone())));
                    break;
                }
                if let Some(column) = columns.iter().find(|c| cells.get(*c).is_some_and(nested)) {
                    found = Some((id, Some(column.clone())));
                    break;
                }
            }
            let pair = inst
                .foreign_keys()
                .into_iter()
                .filter(|(e, p)| !p.is_empty() && !relations.contains(*e))
                .filter_map(|(_, p)| p.iter().next().map(|(s, _)| *s))
                .min();
            match (found, pair) {
                (Some(row), Some(src)) if row.0 <= src => Some(row),
                (_, Some(src)) => Some((src, None)),
                (row, None) => row,
            }
        }

        /// The tables and columns of a side, with its relations.
        fn whole(side: &Side) -> (BTreeMap<Name, BTreeSet<String>>, BTreeSet<Edge>) {
            (side.tables.clone(), side.relations.clone())
        }

        /// The kept part `S'` of a spec's source.
        fn kept(spec: &Spec) -> (BTreeMap<Name, BTreeSet<String>>, BTreeSet<Edge>) {
            let tables = spec
                .tables
                .keys()
                .map(|s| (s.clone(), spec.kept_columns(s).into_iter().collect()))
                .collect();
            (tables, spec.relations.keys().cloned().collect())
        }

        /// `Sigma_F(i)` checked against the oracle, clash included: the
        /// result, or `None` when both refuse with a clash at one equation.
        fn sigma_checked(spec: &Spec, f: &MigrationFunctor, i: &FInstance) -> Option<FInstance> {
            match f_sigma_restricted(i, f) {
                Err(AdjunctionError::SigmaConstantClash { row, column, .. }) => {
                    assert_eq!(oracle::sigma(spec, i, 0), Err((row, column)), "{i:?}");
                    None
                }
                Err(e) => panic!("Sigma of {i:?}: {e}"),
                Ok((sigma, _)) => {
                    let expected = oracle::sigma(spec, i, 1 << 40)
                        .unwrap_or_else(|clash| panic!("the oracle clashes at {clash:?} on {i:?}"));
                    assert!(
                        oracle::brute_iso(&sigma, &expected),
                        "{sigma:?} vs {expected:?}"
                    );
                    Some(sigma)
                }
            }
        }

        /// L1 and L1' at `(i, j)` given `sigma = Sigma_F(i)` (`None` on a
        /// clash) and `delta = Delta_F(j)`: on a clash `Hom(I, Delta J)` is
        /// empty; otherwise `g -> eps . Sigma g` is a bijection
        /// `Hom(I, Delta J) -> Hom(Sigma I, J)` with inverse
        /// `h -> Delta h . eta`. Returns `|Hom(I, Delta J)|`.
        fn sigma_bijection(
            f: &MigrationFunctor,
            i: &FInstance,
            sigma: Option<&FInstance>,
            delta: &FInstance,
            j: &FInstance,
        ) -> usize {
            let left = oracle::brute_homs(i, delta);
            let Some(sigma) = sigma else {
                assert!(
                    left.is_empty(),
                    "a clash has no homs into Delta: {i:?} -> {delta:?}"
                );
                return 0;
            };
            let right = oracle::brute_homs(sigma, j);
            let (left_keys, right_keys) = (index(&left), index(&right));
            assert_eq!(
                left.len(),
                right.len(),
                "|Hom(I, Delta J)| = |Hom(Sigma I, J)|: {i:?}, {j:?}"
            );
            // The transposes at this `(I, J)`, taken once for the hom-set;
            // the public forms are checked against them on the first hom of
            // each side.
            let to_left = SigmaTransposeLeft::new(i, j, f).unwrap_or_else(|e| panic!("{e}"));
            let to_right = SigmaTransposeRight::new(i, j, f).unwrap_or_else(|e| panic!("{e}"));
            for (n, g) in left.iter().enumerate() {
                let g = hom(g);
                let flat = to_right.apply(&g).unwrap_or_else(|e| panic!("{e}"));
                if n == 0 {
                    let public = f_transpose_right(&g, i, j, f).unwrap_or_else(|e| panic!("{e}"));
                    assert!(public.same_as(&flat), "{public:?} vs {flat:?}");
                }
                assert!(
                    holds(&right_keys, &flat),
                    "{g:?} transposes to {flat:?}, no hom"
                );
                let back = to_left.apply(&flat).unwrap_or_else(|e| panic!("{e}"));
                assert!(back.same_as(&g), "{g:?} -> {flat:?} -> {back:?}");
            }
            for (n, h) in right.iter().enumerate() {
                let h = hom(h);
                let sharp = to_left.apply(&h).unwrap_or_else(|e| panic!("{e}"));
                if n == 0 {
                    let public = f_transpose_left(&h, i, j, f).unwrap_or_else(|e| panic!("{e}"));
                    assert!(public.same_as(&sharp), "{public:?} vs {sharp:?}");
                }
                assert!(
                    holds(&left_keys, &sharp),
                    "{h:?} transposes to {sharp:?}, no hom"
                );
                let back = to_right.apply(&sharp).unwrap_or_else(|e| panic!("{e}"));
                assert!(back.same_as(&h), "{h:?} -> {sharp:?} -> {back:?}");
            }
            left.len()
        }

        /// The Sigma -| Delta laws at `(i, j)`, `i` on `S'` and `j` on `T`:
        /// [`sigma_checked`] and [`sigma_bijection`].
        fn sigma_delta(spec: &Spec, f: &MigrationFunctor, i: &FInstance, j: &FInstance) -> usize {
            let delta = f_delta(j, f).unwrap_or_else(|e| panic!("Delta of {j:?}: {e}"));
            let sigma = sigma_checked(spec, f, i);
            sigma_bijection(f, i, sigma.as_ref(), &delta, j)
        }

        /// The product bound's refusal of `Pi_F(i)` under `max`, as the
        /// vertex it names and the product it reports: the first target
        /// table, ascending, whose fibre has two or more tables with a
        /// product of row counts in `i` above `max`; else the first target
        /// relation, ascending, to which nothing is carried and whose
        /// endpoints' limits (from the oracle) hold more than `max` pairs.
        fn over_bound(
            spec: &Spec,
            i: &FInstance,
            expected: &oracle::PiOracle,
            max: usize,
        ) -> Option<(String, usize)> {
            for d in spec.tgt.tables.keys() {
                let fibre = spec.fibre(d);
                let product = fibre
                    .iter()
                    .try_fold(1_usize, |acc, s| acc.checked_mul(i.rows(s).len()))
                    .unwrap_or(usize::MAX);
                if fibre.len() > 1 && product > max {
                    return Some((d.to_string(), product));
                }
            }
            expected.pairs.keys().find_map(|e| {
                let count = expected.rows[&e.src].len() * expected.rows[&e.tgt].len();
                (spec.relation_fibre(e).is_empty() && count > max)
                    .then(|| (e.src.to_string(), count))
            })
        }

        /// `Pi_F(i)` checked against the oracle: refused exactly when the
        /// functor misses a column (the least one named) or a product
        /// exceeds `max` (the refusal [`over_bound`] gives); otherwise equal
        /// to the oracle through its provenance. Returns it when it is
        /// computed.
        fn pi_checked(
            spec: &Spec,
            f: &MigrationFunctor,
            i: &FInstance,
            max: usize,
        ) -> Option<FInstance> {
            let result = functor_pi_restricted(i, f, max);
            if let Some((table, column)) = spec.missing_column() {
                assert!(
                    matches!(
                        &result,
                        Err(AdjunctionError::Pi(RestrictError::AttributeOutsideImage {
                            table: t,
                            column: c,
                        })) if *t == table && *c == column
                    ),
                    "{result:?}"
                );
                return None;
            }
            let expected = oracle::pi(spec, i);
            let refusal = over_bound(spec, i, &expected, max);
            let (pi, provenance) = match result {
                Err(AdjunctionError::Pi(RestrictError::ProductSizeExceeded {
                    vertex,
                    actual,
                    limit,
                })) => {
                    assert_eq!(
                        Some((vertex, actual)),
                        refusal,
                        "the refusal under {limit} of {i:?}"
                    );
                    assert_eq!(limit, max);
                    return None;
                }
                Err(e) => panic!("Pi of {i:?}: {e}"),
                Ok(ok) => ok,
            };
            assert_eq!(refusal, None, "not refused over the bound: {i:?}");
            let mut seen: BTreeMap<Name, BTreeSet<Vec<RowId>>> = BTreeMap::new();
            let mut by_tuple: BTreeMap<(Name, Vec<RowId>), RowId> = BTreeMap::new();
            for (row, (table, tuple)) in &provenance.tuples {
                let cells = expected.rows[table]
                    .get(tuple)
                    .unwrap_or_else(|| panic!("{tuple:?} is no cone at {table}"));
                let ours = pi.row(*row).unwrap_or_else(|| panic!("row {row:?}"));
                assert_eq!(ours.len(), cells.len());
                for (c, v) in cells {
                    assert_eq!(
                        oracle::canon(&ours[c]),
                        oracle::canon(v),
                        "{table} {tuple:?} {c}"
                    );
                }
                assert_eq!(pi.table_of(*row), Some(table));
                seen.entry(table.clone()).or_default().insert(tuple.clone());
                by_tuple.insert((table.clone(), tuple.clone()), *row);
            }
            for (table, rows) in &expected.rows {
                let ours = seen.remove(table).unwrap_or_default();
                assert_eq!(ours, rows.keys().cloned().collect(), "the cones at {table}");
            }
            for (relation, pairs) in &expected.pairs {
                let theirs: BTreeSet<(RowId, RowId)> = pairs
                    .iter()
                    .map(|(x, y)| {
                        (
                            by_tuple[&(relation.src.clone(), x.clone())],
                            by_tuple[&(relation.tgt.clone(), y.clone())],
                        )
                    })
                    .collect();
                assert_eq!(pi.pairs(relation), &theirs, "the pairs at {relation:?}");
            }
            Some(pi)
        }

        /// L2 at `(i, j)` given `pi = Pi_F(i)` and `delta = Delta_F(j)`:
        /// `|Hom(Delta J, I)| = |Hom(J, Pi I)|`, and, when the unit's codomain
        /// `Pi_F(Delta J)` is within the bound too (`transposes`),
        /// `g -> Pi g . unit` is a bijection with inverse
        /// `f -> counit . Delta f`. Returns `|Hom(J, Pi I)|`.
        fn pi_bijection(
            f: &MigrationFunctor,
            (i, pi): (&FInstance, &FInstance),
            (j, delta): (&FInstance, &FInstance),
            transposes: bool,
            max: usize,
        ) -> usize {
            let left = oracle::brute_homs(delta, i);
            let right = oracle::brute_homs(j, pi);
            let (left_keys, right_keys) = (index(&left), index(&right));
            assert_eq!(
                left.len(),
                right.len(),
                "|Hom(Delta J, I)| = |Hom(J, Pi I)|: {i:?}, {j:?}"
            );
            if !transposes {
                return right.len();
            }
            // The transposes at this `(J, I)`, taken once for the hom-set;
            // the public forms are checked against them on the first hom of
            // each side.
            let to_left = PiTransposeLeft::new(j, i, f, max).unwrap_or_else(|e| panic!("{e}"));
            let to_right = PiTransposeRight::new(j, i, f, max).unwrap_or_else(|e| panic!("{e}"));
            for (n, g) in left.iter().enumerate() {
                let g = hom(g);
                let sharp = to_left.apply(&g).unwrap_or_else(|e| panic!("{e}"));
                if n == 0 {
                    let public =
                        f_pi_transpose_left(&g, j, i, f, max).unwrap_or_else(|e| panic!("{e}"));
                    assert!(public.same_as(&sharp), "{public:?} vs {sharp:?}");
                }
                assert!(
                    holds(&right_keys, &sharp),
                    "{g:?} transposes to {sharp:?}, no hom"
                );
                let back = to_right.apply(&sharp).unwrap_or_else(|e| panic!("{e}"));
                assert!(back.same_as(&g), "{g:?} -> {sharp:?} -> {back:?}");
            }
            for (n, h) in right.iter().enumerate() {
                let h = hom(h);
                let flat = to_right.apply(&h).unwrap_or_else(|e| panic!("{e}"));
                if n == 0 {
                    let public =
                        f_pi_transpose_right(&h, j, i, f, max).unwrap_or_else(|e| panic!("{e}"));
                    assert!(public.same_as(&flat), "{public:?} vs {flat:?}");
                }
                assert!(
                    holds(&left_keys, &flat),
                    "{h:?} transposes to {flat:?}, no hom"
                );
                let back = to_left.apply(&flat).unwrap_or_else(|e| panic!("{e}"));
                assert!(back.same_as(&h), "{h:?} -> {flat:?} -> {back:?}");
            }
            right.len()
        }

        /// The Delta -| Pi laws at `(i, j)`: for an attribute-surjective
        /// functor within the bound, L2 by [`pi_bijection`]; otherwise Pi
        /// refuses with the documented error (L2', checked by
        /// [`pi_checked`]). Returns `|Hom(J, Pi I)|`, or `None` when Pi
        /// refuses.
        fn delta_pi(
            spec: &Spec,
            f: &MigrationFunctor,
            i: &FInstance,
            j: &FInstance,
            max: usize,
        ) -> Option<usize> {
            let pi = pi_checked(spec, f, i, max)?;
            let delta = f_delta(j, f).unwrap_or_else(|e| panic!("{e}"));
            let transposes = pi_checked(spec, f, &delta, max).is_some();
            Some(pi_bijection(f, (i, &pi), (j, &delta), transposes, max))
        }

        /// `Delta_F(j)` checked against the oracle through its provenance,
        /// and its conformance to `S'` (L8).
        fn delta_checked(spec: &Spec, f: &MigrationFunctor, j: &FInstance) -> FInstance {
            let (delta, provenance) =
                f_delta_with_provenance(j, f).unwrap_or_else(|e| panic!("{e}"));
            let expected = oracle::delta(spec, j);
            let mut copy_of = BTreeMap::new();
            for (copy, key) in &provenance.copies {
                let same = match (delta.row(*copy), expected.copies.get(key)) {
                    (Some(ours), Some(theirs)) => crate::functor::same_row(ours, theirs),
                    _ => false,
                };
                assert!(same, "the copy {key:?}: {:?}", delta.row(*copy));
                assert_eq!(delta.table_of(*copy), Some(&key.0));
                copy_of.insert(key.clone(), *copy);
            }
            assert_eq!(copy_of.len(), expected.copies.len());
            assert_eq!(delta.total_row_count(), expected.copies.len());
            for (e, pairs) in &expected.pairs {
                let ours: BTreeSet<(RowId, RowId)> = pairs
                    .iter()
                    .map(|(a, b)| (copy_of[a], copy_of[b]))
                    .collect();
                assert_eq!(delta.pairs(e), &ours, "the pairs at {e:?}");
            }
            let (tables, relations) = kept(spec);
            assert_eq!(
                first_violation(&tables, &relations, &delta),
                None,
                "{delta:?}"
            );
            delta
        }

        /// Whether an error is the product bound's.
        fn over(e: &AdjunctionError) -> bool {
            matches!(
                e,
                AdjunctionError::Pi(RestrictError::ProductSizeExceeded { .. })
            )
        }

        /// The triangle identities of both adjunctions at `(i, j)` (L3), on
        /// their domains: Sigma accepting `i`; `F` surjective on attributes
        /// with every limit involved within `max`.
        fn triangles(spec: &Spec, f: &MigrationFunctor, i: &FInstance, j: &FInstance, max: usize) {
            let delta_j = f_delta(j, f).unwrap_or_else(|e| panic!("{e}"));
            // (Delta eps) . (eta Delta) = id at Delta J.
            let eta = f_unit(&delta_j, f).unwrap_or_else(|e| panic!("{e}"));
            let (sigma_delta, _) =
                f_sigma_restricted(&delta_j, f).unwrap_or_else(|e| panic!("{e}"));
            let eps = f_counit(j, f).unwrap_or_else(|e| panic!("{e}"));
            let delta_eps =
                f_delta_on_hom(&eps, &sigma_delta, j, f).unwrap_or_else(|e| panic!("{e}"));
            let id = FInstanceHom::identity(&delta_j);
            assert!(
                compose(&eta, &delta_eps, &delta_j).same_as(&id),
                "(Delta eps)(eta Delta) at {j:?}"
            );
            // (eps Sigma) . (Sigma eta) = id at Sigma I.
            if let Ok((sigma, _)) = f_sigma_restricted(i, f) {
                let eta = f_unit(i, f).unwrap_or_else(|e| panic!("{e}"));
                let delta_sigma = f_delta(&sigma, f).unwrap_or_else(|e| panic!("{e}"));
                let sigma_eta =
                    f_sigma_on_hom(&eta, i, &delta_sigma, f).unwrap_or_else(|e| panic!("{e}"));
                let eps = f_counit(&sigma, f).unwrap_or_else(|e| panic!("{e}"));
                let id = FInstanceHom::identity(&sigma);
                assert!(
                    compose(&sigma_eta, &eps, &sigma).same_as(&id),
                    "(eps Sigma)(Sigma eta) at {i:?}"
                );
            }
            if spec.missing_column().is_some() {
                return;
            }
            // (counit Delta) . (Delta unit) = id at Delta J.
            let first = (|| {
                let unit = f_pi_unit(j, f, max)?;
                let (pi_delta, _) = functor_pi_restricted(&delta_j, f, max)?;
                let delta_unit = f_delta_on_hom(&unit, j, &pi_delta, f)?;
                let counit = f_pi_counit(&delta_j, f, max)?;
                Ok::<_, AdjunctionError>(compose(&delta_unit, &counit, &delta_j))
            })();
            match first {
                Ok(composite) => assert!(
                    composite.same_as(&FInstanceHom::identity(&delta_j)),
                    "at {j:?}"
                ),
                Err(e) => assert!(over(&e), "{e}"),
            }
            // (Pi counit) . (unit Pi) = id at Pi I.
            let second = (|| {
                let (pi, _) = functor_pi_restricted(i, f, max)?;
                let unit = f_pi_unit(&pi, f, max)?;
                let counit = f_pi_counit(i, f, max)?;
                let delta_pi = f_delta(&pi, f)?;
                let pi_counit = f_pi_on_hom(&counit, &delta_pi, i, f, max)?;
                Ok::<_, AdjunctionError>((
                    compose(&unit, &pi_counit, &pi),
                    FInstanceHom::identity(&pi),
                ))
            })();
            match second {
                Ok((composite, id)) => assert!(composite.same_as(&id), "at {i:?}"),
                Err(e) => assert!(over(&e), "{e}"),
            }
        }

        /// L4: along the identity functor of a side, Sigma, Delta and Pi
        /// return `x` exactly, and the units and counits are identities.
        fn identity_laws(side: &Side, x: &FInstance) {
            let f = MigrationFunctor::identity(&fixture::schema_of(side), &fixture::protocol())
                .unwrap_or_else(|e| panic!("{e}"));
            let max = usize::MAX;
            assert_eq!(
                &crate::adjunction::f_sigma(x, &f).unwrap_or_else(|e| panic!("{e}")),
                x
            );
            assert_eq!(&f_delta(x, &f).unwrap_or_else(|e| panic!("{e}")), x);
            assert_eq!(
                &crate::pi::functor_pi_along(x, &f, max).unwrap_or_else(|e| panic!("{e}")),
                x
            );
            let id = FInstanceHom::identity(x);
            for unit in [
                f_unit(x, &f),
                f_counit(x, &f),
                f_pi_unit(x, &f, max),
                f_pi_counit(x, &f, max),
            ] {
                assert!(unit.unwrap_or_else(|e| panic!("{e}")).same_as(&id));
            }
        }

        /// L6 for Delta: `j` within `j_big` gives `Delta j` within
        /// `Delta j_big`, by the injective homomorphism keeping copies.
        fn delta_monotone(f: &MigrationFunctor, j: &FInstance, j_big: &FInstance) {
            let (small, copies) = f_delta_with_provenance(j, f).unwrap_or_else(|e| panic!("{e}"));
            let (big, copies_big) =
                f_delta_with_provenance(j_big, f).unwrap_or_else(|e| panic!("{e}"));
            let row_map: BTreeMap<RowId, RowId> = copies
                .copies
                .iter()
                .map(|(copy, (table, original))| {
                    (
                        *copy,
                        copies_big
                            .copy_of(table, *original)
                            .unwrap_or_else(|| panic!("{original:?}")),
                    )
                })
                .collect();
            let images: BTreeSet<&RowId> = row_map.values().collect();
            assert_eq!(images.len(), row_map.len());
            let witness = FInstanceHom::new(row_map);
            assert!(witness.check(&small, &big).is_ok(), "{small:?} -> {big:?}");
        }

        /// L6 for Sigma: with both inputs accepted, rows keep their ids,
        /// each fresh null goes to the fresh null of its row and column, and
        /// each null of `i` to its representative in `Sigma i_big`; when
        /// only the larger input is refused, the test asserts the refusal.
        fn sigma_monotone(f: &MigrationFunctor, i: &FInstance, i_big: &FInstance) {
            let big = f_sigma_restricted(i_big, f);
            let (small, provenance) = match f_sigma_restricted(i, f) {
                Ok(ok) => ok,
                Err(AdjunctionError::SigmaConstantClash { .. }) => {
                    assert!(matches!(
                        big,
                        Err(AdjunctionError::SigmaConstantClash { .. })
                    ));
                    return;
                }
                Err(e) => panic!("{e}"),
            };
            let (big, provenance_big) = match big {
                Ok(ok) => ok,
                Err(AdjunctionError::SigmaConstantClash { .. }) => return,
                Err(e) => panic!("{e}"),
            };
            let fresh_big: BTreeMap<&(RowId, String), u64> = provenance_big
                .fresh
                .iter()
                .map(|(n, at)| (at, *n))
                .collect();
            let null_map = oracle::nulls(&small)
                .into_iter()
                .map(|n| {
                    let image = match provenance.fresh.get(&n) {
                        Some(at) => Value::LabeledNull(fresh_big[at]),
                        None => provenance_big
                            .merged
                            .get(&n)
                            .cloned()
                            .unwrap_or(Value::LabeledNull(n)),
                    };
                    (n, image)
                })
                .collect();
            let witness = FInstanceHom::identity(&small).with_null_map(null_map);
            assert!(witness.check(&small, &big).is_ok(), "{small:?} -> {big:?}");
        }

        /// L6 for Pi: within the bound, each row of `Pi i` goes to the row of
        /// `Pi i_big` with its tuple, an injective homomorphism.
        fn pi_monotone(f: &MigrationFunctor, i: &FInstance, i_big: &FInstance, max: usize) {
            let (Ok((small, tuples)), Ok((big, tuples_big))) = (
                functor_pi_restricted(i, f, max),
                functor_pi_restricted(i_big, f, max),
            ) else {
                return;
            };
            let by_tuple: BTreeMap<&(Name, Vec<RowId>), RowId> =
                tuples_big.tuples.iter().map(|(row, t)| (t, *row)).collect();
            let row_map: BTreeMap<RowId, RowId> = tuples
                .tuples
                .iter()
                .map(|(row, t)| {
                    (
                        *row,
                        by_tuple.get(t).copied().unwrap_or_else(|| panic!("{t:?}")),
                    )
                })
                .collect();
            let images: BTreeSet<&RowId> = row_map.values().collect();
            assert_eq!(images.len(), row_map.len());
            assert!(FInstanceHom::new(row_map).check(&small, &big).is_ok());
        }

        /// Whether `small` is within `big`: every row of `small` is a row of
        /// `big` with the same id, table and cells, and every pair a pair.
        fn within(small: &FInstance, big: &FInstance) -> bool {
            small.row_ids().into_iter().all(|id| {
                small.table_of(id) == big.table_of(id)
                    && match (small.row(id), big.row(id)) {
                        (Some(a), Some(b)) => crate::functor::same_row(a, b),
                        _ => false,
                    }
            }) && small
                .foreign_keys()
                .into_iter()
                .all(|(e, pairs)| pairs.iter().all(|p| big.pairs(e).contains(p)))
        }

        /// L7: each action on homomorphisms keeps identities, and composites
        /// along `h1: a -> b` and `h2: b -> c` (instances on `S'` for Sigma
        /// and Pi; their Delta images are checked through `f_delta_on_hom`
        /// on the target instances `ta -> tb -> tc` when given).
        fn functorial(
            f: &MigrationFunctor,
            (a, b, c): (&FInstance, &FInstance, &FInstance),
            (h1, h2): (&FInstanceHom, &FInstanceHom),
            max: usize,
        ) {
            let h12 = compose(h1, h2, a);
            if let Ok((sa, _)) = f_sigma_restricted(a, f) {
                if f_sigma_restricted(b, f).is_ok() && f_sigma_restricted(c, f).is_ok() {
                    let id = f_sigma_on_hom(&FInstanceHom::identity(a), a, a, f)
                        .unwrap_or_else(|e| panic!("{e}"));
                    assert!(id.same_as(&FInstanceHom::identity(&sa)));
                    let one = f_sigma_on_hom(h1, a, b, f).unwrap_or_else(|e| panic!("{e}"));
                    let two = f_sigma_on_hom(h2, b, c, f).unwrap_or_else(|e| panic!("{e}"));
                    let both = f_sigma_on_hom(&h12, a, c, f).unwrap_or_else(|e| panic!("{e}"));
                    assert!(both.same_as(&compose(&one, &two, &sa)), "Sigma({h12:?})");
                }
            }
            if let Ok((pa, _)) = functor_pi_restricted(a, f, max) {
                if functor_pi_restricted(b, f, max).is_ok()
                    && functor_pi_restricted(c, f, max).is_ok()
                {
                    let id = f_pi_on_hom(&FInstanceHom::identity(a), a, a, f, max)
                        .unwrap_or_else(|e| panic!("{e}"));
                    assert!(id.same_as(&FInstanceHom::identity(&pa)));
                    let one = f_pi_on_hom(h1, a, b, f, max).unwrap_or_else(|e| panic!("{e}"));
                    let two = f_pi_on_hom(h2, b, c, f, max).unwrap_or_else(|e| panic!("{e}"));
                    let both = f_pi_on_hom(&h12, a, c, f, max).unwrap_or_else(|e| panic!("{e}"));
                    assert!(both.same_as(&compose(&one, &two, &pa)), "Pi({h12:?})");
                }
            }
        }

        /// L7 for Delta along `h1: a -> b`, `h2: b -> c` of instances on `T`.
        fn delta_functorial(
            f: &MigrationFunctor,
            (a, b, c): (&FInstance, &FInstance, &FInstance),
            (h1, h2): (&FInstanceHom, &FInstanceHom),
        ) {
            let da = f_delta(a, f).unwrap_or_else(|e| panic!("{e}"));
            let id = f_delta_on_hom(&FInstanceHom::identity(a), a, a, f)
                .unwrap_or_else(|e| panic!("{e}"));
            assert!(id.same_as(&FInstanceHom::identity(&da)));
            let one = f_delta_on_hom(h1, a, b, f).unwrap_or_else(|e| panic!("{e}"));
            let two = f_delta_on_hom(h2, b, c, f).unwrap_or_else(|e| panic!("{e}"));
            let both =
                f_delta_on_hom(&compose(h1, h2, a), a, c, f).unwrap_or_else(|e| panic!("{e}"));
            assert!(both.same_as(&compose(&one, &two, &da)));
        }

        /// L8 for Sigma and Pi: Sigma's result conforms to `T` (its tables
        /// outside the image are empty), Pi's too.
        fn outputs_conform(spec: &Spec, f: &MigrationFunctor, i: &FInstance, max: usize) {
            let (tables, relations) = whole(&spec.tgt);
            if let Ok((sigma, _)) = f_sigma_restricted(i, f) {
                assert_eq!(
                    first_violation(&tables, &relations, &sigma),
                    None,
                    "{sigma:?}"
                );
                let image: BTreeSet<&Name> = spec.tables.values().collect();
                assert!(sigma.table_names().into_iter().all(|t| image.contains(t)));
            }
            if let Ok((pi, _)) = functor_pi_restricted(i, f, max) {
                assert_eq!(first_violation(&tables, &relations, &pi), None, "{pi:?}");
            }
        }

        /// A relation with a set of its pairs.
        type RelationPairs = (Edge, Vec<(RowId, RowId)>);

        /// Every instance of a side: at most `rows` rows per table numbered
        /// in table order, every cell drawn from `cells`, and every subset of
        /// the possible pairs of each relation.
        fn every_instance(side: &Side, rows: usize, cells: &[Value]) -> Vec<FInstance> {
            // Each table's possible row lists (cells only), table by table.
            let mut tables: Vec<(Name, Vec<Vec<Row>>)> = Vec::new();
            for (table, columns) in &side.tables {
                let mut options: Vec<Vec<Row>> = Vec::new();
                for n in 0..=rows {
                    let mut lists: Vec<Vec<Row>> = vec![Vec::new()];
                    for _ in 0..n {
                        let mut rows_with_cells: Vec<Row> = vec![Row::new()];
                        for column in columns {
                            rows_with_cells = rows_with_cells
                                .into_iter()
                                .flat_map(|r| {
                                    cells.iter().map(move |v| {
                                        let mut r = r.clone();
                                        r.insert(column.clone(), v.clone());
                                        r
                                    })
                                })
                                .collect();
                        }
                        lists = lists
                            .into_iter()
                            .flat_map(|l| {
                                rows_with_cells.iter().map(move |r| {
                                    let mut l = l.clone();
                                    l.push(r.clone());
                                    l
                                })
                            })
                            .collect();
                    }
                    options.extend(lists);
                }
                tables.push((table.clone(), options));
            }
            let mut layouts: Vec<Vec<(Name, Vec<Row>)>> = vec![Vec::new()];
            for (table, options) in &tables {
                layouts = layouts
                    .into_iter()
                    .flat_map(|l| {
                        options.iter().map(move |o| {
                            let mut l = l.clone();
                            l.push((table.clone(), o.clone()));
                            l
                        })
                    })
                    .collect();
            }
            let mut out = Vec::new();
            for layout in layouts {
                let mut next = 0u32;
                let mut ids: BTreeMap<Name, Vec<RowId>> = BTreeMap::new();
                let mut parts = Vec::new();
                for (table, rows) in layout {
                    let mut placed = Vec::new();
                    for row in rows {
                        placed.push((RowId(next), row));
                        ids.entry(table.clone()).or_default().push(RowId(next));
                        next += 1;
                    }
                    parts.push((table, placed));
                }
                let mut pair_sets: Vec<Vec<RelationPairs>> = vec![Vec::new()];
                for relation in &side.relations {
                    let empty = Vec::new();
                    let (from, to) = (
                        ids.get(&relation.src).unwrap_or(&empty),
                        ids.get(&relation.tgt).unwrap_or(&empty),
                    );
                    let possible: Vec<(RowId, RowId)> = from
                        .iter()
                        .flat_map(|a| to.iter().map(move |b| (*a, *b)))
                        .collect();
                    let subsets: Vec<Vec<(RowId, RowId)>> = (0u32..(1u32 << possible.len()))
                        .map(|mask| {
                            possible
                                .iter()
                                .enumerate()
                                .filter(|(k, _)| mask & (1 << k) != 0)
                                .map(|(_, p)| *p)
                                .collect()
                        })
                        .collect();
                    pair_sets = pair_sets
                        .into_iter()
                        .flat_map(|s| {
                            subsets.iter().map(move |sub| {
                                let mut s = s.clone();
                                s.push((relation.clone(), sub.clone()));
                                s
                            })
                        })
                        .collect();
                }
                for pairs in pair_sets {
                    out.push(
                        FInstance::from_parts(parts.clone(), pairs)
                            .unwrap_or_else(|e| panic!("{e}")),
                    );
                }
            }
            out
        }

        /// The cells of the exhaustive family.
        fn tiny_cells() -> Vec<Value> {
            vec![
                Value::Int(0),
                Value::Int(1),
                Value::LabeledNull(0),
                Value::LabeledNull(1),
            ]
        }

        /// The float cells of family (8').
        fn float_cells() -> Vec<Value> {
            vec![
                Value::Float(0.0),
                Value::Float(-0.0),
                Value::Float(f64::NAN),
                Value::Float(-f64::NAN),
            ]
        }

        /// The schema pairs of the exhaustive family, (1) to (9).
        fn family() -> Vec<(&'static str, Spec)> {
            use fixture::{fk, side, spec};
            let l = fk("a", "b", "l");
            let big_l = fk("m", "m", "L");
            let k = fk("e", "e", "k");
            let r = fk("a", "b", "r");
            vec![
                (
                    "(1) a merge of tables with disjoint columns",
                    spec(
                        side(&[("a", &["x"]), ("b", &["y"])], &[]),
                        side(&[("m", &["x", "y"])], &[]),
                        &[("a", "m"), ("b", "m")],
                        &[],
                        &[(("a", "x"), "x"), (("b", "y"), "y")],
                    ),
                ),
                (
                    "(2) a merge with a shared column",
                    spec(
                        side(&[("a", &["x"]), ("b", &["x"])], &[]),
                        side(&[("m", &["x"])], &[]),
                        &[("a", "m"), ("b", "m")],
                        &[],
                        &[(("a", "x"), "x"), (("b", "x"), "x")],
                    ),
                ),
                (
                    "(3) a column rename",
                    spec(
                        side(&[("post", &["title"])], &[]),
                        side(&[("post", &["name"])], &[]),
                        &[("post", "post")],
                        &[],
                        &[(("post", "title"), "name")],
                    ),
                ),
                (
                    "(4) an inclusion that adds a column",
                    spec(
                        side(&[("a", &["x"])], &[]),
                        side(&[("a", &["x", "y"])], &[]),
                        &[("a", "a")],
                        &[],
                        &[(("a", "x"), "x")],
                    ),
                ),
                (
                    "(5) a target-only table without columns",
                    spec(
                        side(&[("a", &["x"])], &[]),
                        side(&[("a", &["x"]), ("z", &[])], &[]),
                        &[("a", "a")],
                        &[],
                        &[(("a", "x"), "x")],
                    ),
                ),
                (
                    "(6) an intra-fibre relation carried to a loop",
                    spec(
                        side(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&l)),
                        side(&[("m", &["x"])], std::slice::from_ref(&big_l)),
                        &[("a", "m"), ("b", "m")],
                        &[(l.clone(), big_l.clone())],
                        &[(("a", "x"), "x")],
                    ),
                ),
                (
                    "(7) a partial, many-valued self key",
                    spec(
                        side(&[("e", &["x"])], std::slice::from_ref(&k)),
                        side(&[("e", &["x"])], std::slice::from_ref(&k)),
                        &[("e", "e")],
                        &[(k.clone(), k.clone())],
                        &[(("e", "x"), "x")],
                    ),
                ),
                (
                    "(8) two columns of one table sent to one column",
                    spec(
                        side(&[("t", &["a1", "a2"])], &[]),
                        side(&[("t", &["b"])], &[]),
                        &[("t", "t")],
                        &[],
                        &[(("t", "a1"), "b"), (("t", "a2"), "b")],
                    ),
                ),
                (
                    "(9) the identity",
                    spec(
                        side(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&r)),
                        side(&[("a", &["x"]), ("b", &[])], std::slice::from_ref(&r)),
                        &[("a", "a"), ("b", "b")],
                        &[(r.clone(), r.clone())],
                        &[(("a", "x"), "x")],
                    ),
                ),
            ]
        }

        /// The family with its cells: (1) to (9) over the integer and null
        /// cells, and (8') over the float cells.
        fn family_with_cells() -> Vec<(&'static str, Spec, Vec<Value>)> {
            let mut out: Vec<(&'static str, Spec, Vec<Value>)> = family()
                .into_iter()
                .map(|(name, spec)| (name, spec, tiny_cells()))
                .collect();
            let eight = family()
                .into_iter()
                .find(|(name, _)| name.starts_with("(8)"))
                .map(|(_, spec)| spec);
            if let Some(spec) = eight {
                out.push(("(8') family (8) over float cells", spec, float_cells()));
            }
            out
        }

        /// Run `check` on every element of `items`, spread over the
        /// machine's threads.
        fn par<T: Sync>(items: &[T], check: impl Fn(&T) + Sync) {
            let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
            let chunk = items.len().div_ceil(threads).max(1);
            std::thread::scope(|scope| {
                for part in items.chunks(chunk) {
                    let check = &check;
                    scope.spawn(move || part.iter().for_each(check));
                }
            });
        }

        /// The bound of the exhaustive family: no product there exceeds it.
        const TINY_MAX: usize = 81;

        /// L1, L1', L2, L2', L3, L4, L6 and L8 on every pair of tiny
        /// instances of every schema pair of the family.
        #[test]
        fn adjunctions_hold_on_every_tiny_instance() {
            for (name, spec, cells) in family_with_cells() {
                let f = fixture::functor(&spec);
                let sources = every_instance(&spec.src, 2, &cells);
                let targets = every_instance(&spec.tgt, 2, &cells);
                let empty = FInstance::new();
                // Everything that depends on one instance is checked once.
                let prepared: Vec<(&FInstance, FInstance, bool)> = targets
                    .iter()
                    .map(|j| {
                        let delta = delta_checked(&spec, &f, j);
                        identity_laws(&spec.tgt, j);
                        triangles(&spec, &f, &empty, j, TINY_MAX);
                        let within_bound = spec.missing_column().is_none()
                            && pi_checked(&spec, &f, &delta, TINY_MAX).is_some();
                        (j, delta, within_bound)
                    })
                    .collect();
                par(&sources, |i| {
                    identity_laws(&spec.src, i);
                    outputs_conform(&spec, &f, i, TINY_MAX);
                    triangles(&spec, &f, i, &empty, TINY_MAX);
                    let sigma = sigma_checked(&spec, &f, i);
                    let pi = pi_checked(&spec, &f, i, TINY_MAX);
                    assert_eq!(
                        pi.is_none(),
                        spec.missing_column().is_some(),
                        "{name}: {i:?}"
                    );
                    for (j, delta, within_bound) in &prepared {
                        sigma_bijection(&f, i, sigma.as_ref(), delta, j);
                        if let Some(pi) = &pi {
                            pi_bijection(&f, (i, pi), (j, delta), *within_bound, TINY_MAX);
                        }
                    }
                    for big in sources.iter().filter(|big| within(i, big)) {
                        sigma_monotone(&f, i, big);
                        pi_monotone(&f, i, big, TINY_MAX);
                    }
                });
                par(&targets, |j| {
                    for big in targets.iter().filter(|big| within(j, big)) {
                        delta_monotone(&f, j, big);
                    }
                });
            }
        }

        /// L1' on the family: an instance Sigma refuses with a clash of
        /// constants has no homomorphism into any `Delta J`.
        #[test]
        fn a_sigma_clash_has_no_homs_into_delta() {
            let mut clashes = 0usize;
            for (_, spec, cells) in family_with_cells() {
                let f = fixture::functor(&spec);
                let targets: Vec<FInstance> = every_instance(&spec.tgt, 2, &cells)
                    .iter()
                    .map(|j| f_delta(j, &f).unwrap_or_else(|e| panic!("{e}")))
                    .collect();
                for i in every_instance(&spec.src, 2, &cells) {
                    if let Err(AdjunctionError::SigmaConstantClash { .. }) =
                        f_sigma_restricted(&i, &f)
                    {
                        clashes += 1;
                        for delta in &targets {
                            assert!(
                                oracle::brute_homs(&i, delta).is_empty(),
                                "{i:?} -> {delta:?}"
                            );
                        }
                    }
                }
            }
            assert!(clashes > 0, "the family has clashing instances");
        }

        /// `FInstanceHom::check` accepts a table-respecting row map exactly
        /// when the brute-force enumerator does, for every candidate between
        /// any two instances of each schema of the family.
        #[test]
        fn hom_check_agrees_with_the_brute_force_enumerator() {
            for (_, spec, cells) in family_with_cells() {
                for side in [&spec.src, &spec.tgt] {
                    let instances = every_instance(side, 2, &cells);
                    par(&instances, |a| {
                        for b in &instances {
                            for candidate in oracle::candidates(a, b) {
                                let (accepted, nu) = oracle::brute_accepts(a, b, &candidate);
                                let h = FInstanceHom::new(candidate).with_null_map(nu);
                                assert_eq!(
                                    h.check(a, b).is_ok(),
                                    accepted,
                                    "{a:?} -> {b:?}: {h:?}"
                                );
                            }
                        }
                    });
                }
            }
        }

        /// Sigma, Delta and Pi agree with their oracles on every tiny
        /// instance of the family.
        #[test]
        fn sigma_delta_pi_agree_with_their_oracles() {
            for (_, spec, cells) in family_with_cells() {
                let f = fixture::functor(&spec);
                for j in every_instance(&spec.tgt, 2, &cells) {
                    delta_checked(&spec, &f, &j);
                }
                for i in every_instance(&spec.src, 2, &cells) {
                    match (
                        f_sigma_restricted(&i, &f),
                        oracle::sigma(&spec, &i, 1 << 40),
                    ) {
                        (Ok((sigma, _)), Ok(expected)) => {
                            assert!(oracle::brute_iso(&sigma, &expected));
                        }
                        (Err(AdjunctionError::SigmaConstantClash { row, column, .. }), Err(at)) => {
                            assert_eq!((row, column), at);
                        }
                        (ours, theirs) => panic!("{i:?}: {ours:?} vs {theirs:?}"),
                    }
                    pi_checked(&spec, &f, &i, TINY_MAX);
                }
            }
        }

        /// The larger of two inputs may clash while the smaller does not:
        /// Sigma's monotonicity is then vacuous, and the refusal is asserted.
        #[test]
        fn sigma_monotonicity_is_vacuous_when_the_larger_input_clashes() {
            let spec = family()
                .into_iter()
                .find(|(name, _)| name.starts_with("(8)"))
                .map_or_else(|| panic!("family (8)"), |(_, spec)| spec);
            let f = fixture::functor(&spec);
            let row = |a1: Value, a2: Value| fixture::row(&[("a1", a1), ("a2", a2)]);
            let small = FInstance::new()
                .with_rows("t", [(RowId(1), row(Value::Int(0), Value::LabeledNull(5)))])
                .unwrap_or_else(|e| panic!("{e}"));
            let big = small
                .clone()
                .with_rows("t", [(RowId(2), row(Value::LabeledNull(5), Value::Int(1)))])
                .unwrap_or_else(|e| panic!("{e}"));
            assert!(within(&small, &big));
            assert!(f_sigma_restricted(&small, &f).is_ok());
            assert!(matches!(
                f_sigma_restricted(&big, &f),
                Err(AdjunctionError::SigmaConstantClash { row, .. }) if row == RowId(2)
            ));
            sigma_monotone(&f, &small, &big);
        }

        /// A bijective renaming of a spec's target: tables, columns and
        /// relation names each sent to a fresh name in the reverse of their
        /// order, so the renaming also reorders them.
        struct Renaming {
            tables: BTreeMap<Name, Name>,
            columns: BTreeMap<String, String>,
            relations: BTreeMap<Edge, Edge>,
        }

        impl Renaming {
            fn of(side: &Side) -> Self {
                let reversed = |names: Vec<String>, prefix: &str| -> BTreeMap<String, String> {
                    let n = names.len();
                    names
                        .into_iter()
                        .enumerate()
                        .map(|(k, name)| (name, format!("{prefix}{:02}", n - k)))
                        .collect()
                };
                let tables: BTreeMap<Name, Name> =
                    reversed(side.tables.keys().map(ToString::to_string).collect(), "t")
                        .into_iter()
                        .map(|(a, b)| (Name::from(a), Name::from(b)))
                        .collect();
                let columns: BTreeSet<String> = side.tables.values().flatten().cloned().collect();
                let columns = reversed(columns.into_iter().collect(), "c");
                let names = reversed(
                    side.relations.iter().map(|e| format!("{e:?}")).collect(),
                    "r",
                );
                let relations = side
                    .relations
                    .iter()
                    .map(|e| {
                        let image = Edge {
                            src: tables[&e.src].clone(),
                            tgt: tables[&e.tgt].clone(),
                            kind: e.kind.clone(),
                            name: Some(names[&format!("{e:?}")].clone().into()),
                        };
                        (e.clone(), image)
                    })
                    .collect();
                Self {
                    tables,
                    columns,
                    relations,
                }
            }

            fn side(&self, side: &Side) -> Side {
                Side {
                    tables: side
                        .tables
                        .iter()
                        .map(|(t, cs)| {
                            (
                                self.tables[t].clone(),
                                cs.iter().map(|c| self.columns[c].clone()).collect(),
                            )
                        })
                        .collect(),
                    relations: side
                        .relations
                        .iter()
                        .map(|e| self.relations[e].clone())
                        .collect(),
                }
            }

            fn spec(&self, spec: &Spec) -> Spec {
                Spec {
                    src: spec.src.clone(),
                    tgt: self.side(&spec.tgt),
                    tables: spec
                        .tables
                        .iter()
                        .map(|(s, d)| (s.clone(), self.tables[d].clone()))
                        .collect(),
                    relations: spec
                        .relations
                        .iter()
                        .map(|(e, image)| (e.clone(), self.relations[image].clone()))
                        .collect(),
                    columns: spec
                        .columns
                        .iter()
                        .map(|(k, b)| (k.clone(), self.columns[b].clone()))
                        .collect(),
                }
            }

            /// A target instance renamed, its nulls sent along `nulls`.
            fn instance(&self, inst: &FInstance, nulls: &BTreeMap<u64, u64>) -> FInstance {
                let tables = inst.tables().into_iter().map(|(t, table)| {
                    let rows: Vec<(RowId, Row)> = table
                        .rows()
                        .into_iter()
                        .map(|(id, row)| {
                            let cells = row
                                .iter()
                                .map(|(c, v)| {
                                    let v = match v {
                                        Value::LabeledNull(n) => {
                                            Value::LabeledNull(nulls.get(n).copied().unwrap_or(*n))
                                        }
                                        other => other.clone(),
                                    };
                                    (self.columns[c].clone(), v)
                                })
                                .collect();
                            (id, cells)
                        })
                        .collect();
                    (self.tables[t].clone(), rows)
                });
                let pairs = inst.foreign_keys().into_iter().map(|(e, p)| {
                    (
                        self.relations[e].clone(),
                        p.iter().copied().collect::<Vec<_>>(),
                    )
                });
                FInstance::from_parts(tables, pairs).unwrap_or_else(|e| panic!("{e}"))
            }
        }

        /// L5 at `(i, j)`: along `F' = rho . F`, Delta of `rho J` is Delta of
        /// `J`; Sigma is the renaming of Sigma up to permuting each row's fresh
        /// nulls; Pi is the renaming of Pi up to the ids of the one row of each
        /// table outside the image; and the units and counits agree under the
        /// same correspondence.
        fn renaming_laws(
            spec: &Spec,
            f: &MigrationFunctor,
            i: &FInstance,
            j: &FInstance,
            max: usize,
        ) {
            let rho = Renaming::of(&spec.tgt);
            let renamed = rho.spec(spec);
            let g = fixture::functor(&renamed);
            let none = BTreeMap::new();
            let rho_j = rho.instance(j, &none);
            assert_eq!(
                f_delta(&rho_j, &g).unwrap_or_else(|e| panic!("{e}")),
                f_delta(j, f).unwrap_or_else(|e| panic!("{e}")),
                "Delta"
            );
            match (f_sigma_restricted(i, f), f_sigma_restricted(i, &g)) {
                (Ok((sigma, provenance)), Ok((sigma_r, provenance_r))) => {
                    let at: BTreeMap<(RowId, String), u64> = provenance_r
                        .fresh
                        .iter()
                        .map(|(n, (r, c))| ((*r, c.clone()), *n))
                        .collect();
                    let phi: BTreeMap<u64, u64> = provenance
                        .fresh
                        .iter()
                        .map(|(n, (r, c))| (*n, at[&(*r, rho.columns[c].clone())]))
                        .collect();
                    assert_eq!(rho.instance(&sigma, &phi), sigma_r, "Sigma");
                    let canonical = |m: &BTreeMap<u64, Value>| -> BTreeMap<u64, Vec<u8>> {
                        m.iter().map(|(n, v)| (*n, oracle::canon(v))).collect()
                    };
                    assert_eq!(
                        canonical(&provenance.merged),
                        canonical(&provenance_r.merged)
                    );
                    let (eta, eta_r) = (f_unit(i, f), f_unit(i, &g));
                    assert!(
                        eta.unwrap_or_else(|e| panic!("{e}"))
                            .same_as(&eta_r.unwrap_or_else(|e| panic!("{e}")))
                    );
                }
                (
                    Err(AdjunctionError::SigmaConstantClash { row, .. }),
                    Err(AdjunctionError::SigmaConstantClash { row: row_r, .. }),
                ) => {
                    assert_eq!(row, row_r);
                }
                (ours, theirs) => panic!("{ours:?} vs {theirs:?}"),
            }
            // The counits: Sigma of Delta J against J, and of Delta rho J.
            let (eps, eps_r) = (
                f_counit(j, f).unwrap_or_else(|e| panic!("{e}")),
                f_counit(&rho_j, &g).unwrap_or_else(|e| panic!("{e}")),
            );
            assert_eq!(eps.row_map, eps_r.row_map);
            let delta = f_delta(j, f).unwrap_or_else(|e| panic!("{e}"));
            let (_, fresh) = f_sigma_restricted(&delta, f).unwrap_or_else(|e| panic!("{e}"));
            let (_, fresh_r) = f_sigma_restricted(&delta, &g).unwrap_or_else(|e| panic!("{e}"));
            let at: BTreeMap<(RowId, String), u64> = fresh_r
                .fresh
                .iter()
                .map(|(n, (r, c))| ((*r, c.clone()), *n))
                .collect();
            // A null with no entry goes to itself.
            let image_of = |h: &FInstanceHom, n: u64| {
                oracle::canon(h.null_map.get(&n).unwrap_or(&Value::LabeledNull(n)))
            };
            for (n, (r, c)) in &fresh.fresh {
                let image = at[&(*r, rho.columns[c].clone())];
                assert_eq!(
                    image_of(&eps, *n),
                    image_of(&eps_r, image),
                    "the counit at {n}"
                );
            }
            if spec.missing_column().is_some() {
                return;
            }
            let (Ok((pi, _)), Ok((pi_r, _))) = (
                functor_pi_restricted(i, f, max),
                functor_pi_restricted(i, &g, max),
            ) else {
                return;
            };
            let outside: BTreeSet<Name> = spec
                .tgt
                .tables
                .keys()
                .filter(|d| spec.fibre(d).is_empty())
                .map(|d| rho.tables[d].clone())
                .collect();
            let renamed_pi = rho.instance(&pi, &none);
            assert!(oracle::brute_iso(&renamed_pi, &pi_r), "Pi");
            for (table, rows) in renamed_pi.tables() {
                if !outside.contains(table) {
                    assert_eq!(Some(rows), pi_r.table(table), "Pi at {table}");
                }
            }
            if let (Ok(counit), Ok(counit_r)) = (f_pi_counit(i, f, max), f_pi_counit(i, &g, max)) {
                assert!(counit.same_as(&counit_r), "the Pi counit");
            }
            if let (Ok(unit), Ok(unit_r)) = (f_pi_unit(j, f, max), f_pi_unit(&rho_j, &g, max)) {
                for (row, image) in &unit.row_map {
                    let table = j.table_of(*row).unwrap_or_else(|| panic!("{row:?}"));
                    let image_r = unit_r.row_map[row];
                    if spec.fibre(table).is_empty() {
                        assert_eq!(
                            pi_r_table(&unit_r, &rho_j, *row, &g, max),
                            Some(rho.tables[table].clone())
                        );
                    } else {
                        assert_eq!(*image, image_r, "the Pi unit at {row:?}");
                    }
                }
            }
        }

        /// The table of the image of `row` under the Pi unit of `j`.
        fn pi_r_table(
            unit: &FInstanceHom,
            j: &FInstance,
            row: RowId,
            f: &MigrationFunctor,
            max: usize,
        ) -> Option<Name> {
            let delta = f_delta(j, f).ok()?;
            let (pi, _) = functor_pi_restricted(&delta, f, max).ok()?;
            pi.table_of(unit.image(row)?).cloned()
        }

        /// A small deterministic generator (splitmix64), so a proptest seed
        /// names one whole case.
        struct Rng(u64);

        impl Rng {
            fn next(&mut self) -> u64 {
                self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
                let mut z = self.0;
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                z ^ (z >> 31)
            }

            /// A number below `n` (`0` when `n` is `0`).
            fn below(&mut self, n: usize) -> usize {
                if n == 0 {
                    return 0;
                }
                usize::try_from(self.next() % u64::try_from(n).unwrap_or(u64::MAX)).unwrap_or(0)
            }

            fn chance(&mut self, percent: usize) -> bool {
                self.below(100) < percent
            }

            fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
                if items.is_empty() {
                    None
                } else {
                    items.get(self.below(items.len()))
                }
            }
        }

        /// The cells of the generated instances: integers, labelled nulls,
        /// both zeros and both NaNs (only `f64::NAN` and `-f64::NAN`).
        fn generated_cells() -> Vec<Value> {
            vec![
                Value::Int(0),
                Value::Int(1),
                Value::LabeledNull(0),
                Value::LabeledNull(1),
                Value::LabeledNull(2),
                Value::Float(0.0),
                Value::Float(-0.0),
                Value::Float(f64::NAN),
                Value::Float(-f64::NAN),
            ]
        }

        /// A generated side: at most 4 tables with at most 3 columns each,
        /// and at most 4 relations (self-references included).
        fn gen_side(rng: &mut Rng, prefix: &str) -> Side {
            let tables: BTreeMap<Name, BTreeSet<String>> = (0..=rng.below(4))
                .map(|k| {
                    let columns = (0..rng.below(4)).map(|c| format!("c{c}")).collect();
                    (Name::from(format!("{prefix}{k}")), columns)
                })
                .collect();
            let names: Vec<Name> = tables.keys().cloned().collect();
            let mut relations = BTreeSet::new();
            for k in 0..rng.below(5) {
                if let (Some(a), Some(b)) = (rng.pick(&names).cloned(), rng.pick(&names).cloned()) {
                    relations.insert(fixture::fk(&a, &b, &format!("{prefix}e{k}")));
                }
            }
            Side { tables, relations }
        }

        /// A generated spec: a source and target side, a table map that may
        /// merge tables and drop some, relations carried to target relations
        /// between the images of their endpoints (some added to the target
        /// for it, up to 4) or dropped, and columns sent to columns of the
        /// image table or dropped, so column maps may be non-injective and
        /// non-surjective and the target may have objects outside the image.
        fn gen_spec(rng: &mut Rng) -> Spec {
            let src = gen_side(rng, "s");
            let mut tgt = gen_side(rng, "d");
            let targets: Vec<Name> = tgt.tables.keys().cloned().collect();
            let mut tables = BTreeMap::new();
            for s in src.tables.keys() {
                if rng.chance(85) {
                    if let Some(d) = rng.pick(&targets) {
                        tables.insert(s.clone(), d.clone());
                    }
                }
            }
            let mut relations = BTreeMap::new();
            for (k, e) in src.relations.iter().enumerate() {
                let (Some(a), Some(b)) = (tables.get(&e.src), tables.get(&e.tgt)) else {
                    continue;
                };
                if tgt.relations.len() < 4 && rng.chance(50) {
                    tgt.relations.insert(fixture::fk(a, b, &format!("dk{k}")));
                }
                let candidates: Vec<Edge> = tgt
                    .relations
                    .iter()
                    .filter(|r| r.src == *a && r.tgt == *b)
                    .cloned()
                    .collect();
                if rng.chance(80) {
                    if let Some(image) = rng.pick(&candidates) {
                        relations.insert(e.clone(), image.clone());
                    }
                }
            }
            let mut columns = BTreeMap::new();
            for (s, d) in &tables {
                let image_columns: Vec<String> = tgt.tables[d].iter().cloned().collect();
                for alpha in &src.tables[s] {
                    if rng.chance(80) {
                        if let Some(beta) = rng.pick(&image_columns) {
                            columns.insert((s.clone(), alpha.clone()), beta.clone());
                        }
                    }
                }
            }
            // Half the specs are made surjective on attributes, so the
            // laws of Pi, which hold exactly there, are exercised as often
            // as its refusals: every target column no kept column reaches
            // is removed from the target.
            if rng.chance(50) {
                let hit: BTreeSet<(Name, String)> = columns
                    .iter()
                    .map(|((s, _), beta)| (tables[s].clone(), beta.clone()))
                    .collect();
                for (d, target_columns) in &mut tgt.tables {
                    target_columns.retain(|beta| hit.contains(&(d.clone(), beta.clone())));
                }
            }
            Spec {
                src,
                tgt,
                tables,
                relations,
                columns,
            }
        }

        /// A generated instance of a side: at most 3 rows per table and 6 in
        /// all, cells from [`generated_cells`], and a random set of pairs per
        /// relation (partial and many-valued). Row ids start at `first`.
        fn gen_instance(rng: &mut Rng, side: &Side, first: u32) -> FInstance {
            let cells = generated_cells();
            let mut next = first;
            let mut total = 0usize;
            let mut ids: BTreeMap<Name, Vec<RowId>> = BTreeMap::new();
            let mut parts = Vec::new();
            for (table, columns) in &side.tables {
                let n = rng.below(4).min(6 - total);
                total += n;
                let mut rows = Vec::new();
                for _ in 0..n {
                    let row: Row = columns
                        .iter()
                        .map(|c| {
                            (
                                c.clone(),
                                rng.pick(&cells).cloned().unwrap_or(Value::Int(0)),
                            )
                        })
                        .collect();
                    rows.push((RowId(next), row));
                    ids.entry(table.clone()).or_default().push(RowId(next));
                    next += 1;
                }
                parts.push((table.clone(), rows));
            }
            let empty = Vec::new();
            let pairs: Vec<(Edge, Vec<(RowId, RowId)>)> = side
                .relations
                .iter()
                .map(|e| {
                    let from = ids.get(&e.src).unwrap_or(&empty);
                    let to = ids.get(&e.tgt).unwrap_or(&empty);
                    let chosen = from
                        .iter()
                        .flat_map(|a| to.iter().map(move |b| (*a, *b)))
                        .filter(|_| rng.chance(35))
                        .collect();
                    (e.clone(), chosen)
                })
                .collect();
            FInstance::from_parts(parts, pairs).unwrap_or_else(|e| panic!("{e}"))
        }

        /// `small` with up to two more rows and some more pairs: an instance
        /// it is within.
        fn grow(rng: &mut Rng, side: &Side, small: &FInstance) -> FInstance {
            let extra = gen_instance(rng, side, small.max_row_id().map_or(0, |r| r.0 + 1));
            let mut big = small.clone();
            let mut added = 0;
            for (table, rows) in extra.tables() {
                for (id, row) in rows.rows() {
                    let room = big.row_count(table) < 3 && big.total_row_count() < 6;
                    if room && added < 2 && rng.chance(60) {
                        big = big
                            .with_rows(table.clone(), [(id, row.clone())])
                            .unwrap_or_else(|e| panic!("{e}"));
                        added += 1;
                    }
                }
            }
            for e in &side.relations {
                let from: Vec<RowId> = big.rows(&e.src).into_iter().map(|(id, _)| id).collect();
                let to: Vec<RowId> = big.rows(&e.tgt).into_iter().map(|(id, _)| id).collect();
                for a in &from {
                    for b in &to {
                        if !big.contains_pair(e, *a, *b) && rng.chance(20) {
                            big.insert_foreign_key(e.clone(), *a, *b)
                                .unwrap_or_else(|e| panic!("{e}"));
                        }
                    }
                }
            }
            big
        }

        /// A generated case: a spec, its functor, an instance on `S'` (the
        /// restriction of one on `S`), one on `T`, and the product bound
        /// drawn from {4, 81}.
        struct Case {
            spec: Spec,
            f: MigrationFunctor,
            x: FInstance,
            i: FInstance,
            j: FInstance,
            max: usize,
            rng: Rng,
        }

        fn case(seed: u64) -> Case {
            let mut rng = Rng(seed);
            let spec = gen_spec(&mut rng);
            let f = fixture::functor(&spec);
            let x = gen_instance(&mut rng, &spec.src, 0);
            let i =
                crate::adjunction::f_restrict_to_domain(&x, &f).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(i, oracle::restrict(&spec, &x), "the restriction");
            let first = u32::try_from(rng.below(8)).unwrap_or(0);
            let j = gen_instance(&mut rng, &spec.tgt, first);
            let max = if rng.chance(50) { 4 } else { 81 };
            Case {
                spec,
                f,
                x,
                i,
                j,
                max,
                rng,
            }
        }

        /// A homomorphism out of `a`: `a` with one null replaced by a value
        /// and then grown, reached by the identity on rows and that one null
        /// entry.
        fn hom_out(rng: &mut Rng, side: &Side, a: &FInstance) -> (FInstance, FInstanceHom) {
            let nulls: Vec<u64> = oracle::nulls(a).into_iter().collect();
            let mut null_map = BTreeMap::new();
            let mut b = a.clone();
            if let Some(&n) = rng.pick(&nulls) {
                let cells = generated_cells();
                let value = rng.pick(&cells).cloned().unwrap_or(Value::Int(0));
                for id in a.row_ids() {
                    if let Some(row) = b.row_mut(id) {
                        for cell in row.values_mut() {
                            if *cell == Value::LabeledNull(n) {
                                *cell = value.clone();
                            }
                        }
                    }
                }
                null_map.insert(n, value);
            }
            let b = grow(rng, side, &b);
            let h = FInstanceHom::identity(a).with_null_map(null_map);
            assert!(h.check(a, &b).is_ok(), "{a:?} -> {b:?}");
            (b, h)
        }

        /// The unit and counit of both adjunctions and `Delta_F` on a
        /// homomorphism find each row's copy in a `Delta_F` result through
        /// an index built once, so along the identity functor over 2 000
        /// rows all five read each copy a few times (counted, not timed); a
        /// search of the copies for every row reads about `2 * 10^6`.
        #[test]
        fn homs_through_delta_copies_are_linear_in_the_rows() {
            const ROWS: usize = 2_000;
            /// One of the five, run at `x`.
            type Run<'r> = &'r dyn Fn() -> Result<FInstanceHom, AdjunctionError>;
            let side = fixture::side(&[("a", &["n"])], &[]);
            let f = MigrationFunctor::identity(&fixture::schema_of(&side), &fixture::protocol())
                .unwrap_or_else(|e| panic!("{e}"));
            let x = FInstance::new()
                .with_table(
                    "a",
                    (0..ROWS).map(|n| {
                        let n = i64::try_from(n).unwrap_or_else(|e| panic!("{e}"));
                        fixture::row(&[("n", Value::Int(n))])
                    }),
                )
                .unwrap_or_else(|e| panic!("{e}"));
            let id = FInstanceHom::identity(&x);
            let copies = |run: Run<'_>| {
                let before = crate::adjunction::COPY_WORK.with(std::cell::Cell::get);
                let hom = run();
                (
                    hom,
                    crate::adjunction::COPY_WORK.with(std::cell::Cell::get) - before,
                )
            };
            let runs: [(&str, Run<'_>); 5] = [
                ("unit", &|| f_unit(&x, &f)),
                ("counit", &|| f_counit(&x, &f)),
                ("Delta on a hom", &|| f_delta_on_hom(&id, &x, &x, &f)),
                ("Pi unit", &|| f_pi_unit(&x, &f, usize::MAX)),
                ("Pi counit", &|| f_pi_counit(&x, &f, usize::MAX)),
            ];
            for (name, run) in runs {
                let (hom, work) = copies(run);
                let hom = hom.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(hom.same_as(&id), "the {name} along the identity");
                assert!(
                    work <= 4 * ROWS,
                    "the {name} read {work} copies to find {ROWS} rows' copies"
                );
            }
        }

        mod generated {
            use proptest::prelude::*;

            use super::*;

            proptest! {
                #![proptest_config(ProptestConfig::with_cases(256))]

                /// L1 and L1' on generated cases: at a random target
                /// instance, and at Sigma I itself, where the identity makes
                /// the hom-sets non-empty.
                #[test]
                fn sigma_is_left_adjoint_to_delta(seed in any::<u64>()) {
                    sigma_laws(&case(seed));
                }

                /// L2 and L2' on generated cases: at a random source
                /// instance, and at Delta J.
                #[test]
                fn pi_is_right_adjoint_to_delta(seed in any::<u64>()) {
                    let case = case(seed);
                    delta_pi(&case.spec, &case.f, &case.i, &case.j, case.max);
                    // At Delta J itself, where the identity makes the
                    // hom-sets non-empty, when Delta J is within the bounds
                    // of a generated instance.
                    let delta = f_delta(&case.j, &case.f).unwrap_or_else(|e| panic!("{e}"));
                    let bounded = delta.total_row_count() <= 6
                        && delta.tables().iter().all(|(_, rows)| rows.len() <= 3);
                    if bounded {
                        let count = delta_pi(&case.spec, &case.f, &delta, &case.j, case.max);
                        if let Some(count) = count {
                            prop_assert!(count > 0);
                        }
                    }
                }

                #[test]
                fn triangle_identities_hold(seed in any::<u64>()) {
                    let case = case(seed);
                    triangles(&case.spec, &case.f, &case.i, &case.j, case.max);
                }

                #[test]
                fn kan_extensions_along_the_identity_are_identities(seed in any::<u64>()) {
                    let case = case(seed);
                    identity_laws(&case.spec.src, &case.x);
                    identity_laws(&case.spec.tgt, &case.j);
                }

                #[test]
                fn results_do_not_depend_on_target_names(seed in any::<u64>()) {
                    let case = case(seed);
                    renaming_laws(&case.spec, &case.f, &case.i, &case.j, case.max);
                }

                #[test]
                fn pi_is_monotone(seed in any::<u64>()) {
                    let mut case = case(seed);
                    let (tables, relations) = kept(&case.spec);
                    let kept_side = Side { tables, relations };
                    let big = grow(&mut case.rng, &kept_side, &case.i);
                    prop_assert!(within(&case.i, &big));
                    pi_monotone(&case.f, &case.i, &big, case.max);
                }

                #[test]
                fn sigma_and_delta_are_monotone(seed in any::<u64>()) {
                    let mut case = case(seed);
                    let (tables, relations) = kept(&case.spec);
                    let kept_side = Side { tables, relations };
                    let big = grow(&mut case.rng, &kept_side, &case.i);
                    sigma_monotone(&case.f, &case.i, &big);
                    let big_j = grow(&mut case.rng, &case.spec.tgt, &case.j);
                    prop_assert!(within(&case.j, &big_j));
                    delta_monotone(&case.f, &case.j, &big_j);
                }

                /// L7: identities and composites of homomorphisms that fill
                /// nulls and add rows.
                #[test]
                fn kan_extensions_are_functorial(seed in any::<u64>()) {
                    let mut case = case(seed);
                    let (tables, relations) = kept(&case.spec);
                    let kept_side = Side { tables, relations };
                    let (b, h1) = hom_out(&mut case.rng, &kept_side, &case.i);
                    let (c, h2) = hom_out(&mut case.rng, &kept_side, &b);
                    functorial(&case.f, (&case.i, &b, &c), (&h1, &h2), case.max);
                    let (tb, t1) = hom_out(&mut case.rng, &case.spec.tgt, &case.j);
                    let (tc, t2) = hom_out(&mut case.rng, &case.spec.tgt, &tb);
                    delta_functorial(&case.f, (&case.j, &tb, &tc), (&t1, &t2));
                }

                /// L8: outputs conform, and an input that does not is refused
                /// with the first offender the oracle names.
                #[test]
                fn outputs_conform_and_refusals_are_typed(seed in any::<u64>()) {
                    let mut case = case(seed);
                    outputs_conform(&case.spec, &case.f, &case.i, case.max);
                    delta_checked(&case.spec, &case.f, &case.j);
                    let (src_tables, src_relations) = whole(&case.spec.src);
                    let bad = corrupt(&mut case.rng, &case.x);
                    let expected = first_violation(&src_tables, &src_relations, &bad);
                    let refused = |result: Result<(), AdjunctionError>| match result {
                        Err(AdjunctionError::NonConforming { row, column, .. }) => {
                            Some((row, column))
                        }
                        _ => None,
                    };
                    let sigma = refused(crate::adjunction::f_sigma(&bad, &case.f).map(|_| ()));
                    prop_assert_eq!(&sigma, &expected);
                    if case.spec.missing_column().is_none() {
                        let pi = crate::pi::functor_pi_along(&bad, &case.f, 81).map(|_| ());
                        prop_assert_eq!(refused(pi), expected);
                    }
                    let (tgt_tables, tgt_relations) = whole(&case.spec.tgt);
                    let bad_j = corrupt(&mut case.rng, &case.j);
                    let expected = first_violation(&tgt_tables, &tgt_relations, &bad_j);
                    prop_assert_eq!(refused(f_delta(&bad_j, &case.f).map(|_| ())), expected);
                }
            }

            /// L5 on the generated cases whose Pi results hold the most rows
            /// with equal cells: about `4e8` table-respecting row maps each,
            /// which an isomorphism search that does not prune by
            /// injectivity, by pairs and by twins cannot get through. Each
            /// case must finish well inside the time the test runner allows
            /// a test.
            #[test]
            fn renaming_laws_finish_on_pi_results_with_many_equal_rows() {
                for seed in [1606_u64, 1703, 1873, 2932, 3193] {
                    let (done, finished) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let case = case(seed);
                        renaming_laws(&case.spec, &case.f, &case.i, &case.j, case.max);
                        let _ = done.send(());
                    });
                    assert!(
                        finished
                            .recv_timeout(std::time::Duration::from_secs(30))
                            .is_ok(),
                        "the renaming laws at seed {seed} did not finish within 30 s"
                    );
                }
            }

            /// L1 and L1' at one generated case: at its random target
            /// instance, and at Sigma I itself, where the identity makes the
            /// hom-sets non-empty.
            fn sigma_laws(case: &Case) {
                sigma_delta(&case.spec, &case.f, &case.i, &case.j);
                if let Ok((sigma, _)) = f_sigma_restricted(&case.i, &case.f) {
                    assert!(sigma_delta(&case.spec, &case.f, &case.i, &sigma) > 0);
                }
            }

            /// The Kan extensions of whole instances taken by `run` on this
            /// thread.
            fn kan_work<T>(run: impl FnOnce() -> T) -> (T, usize) {
                let before = crate::adjunction::KAN_WORK.with(std::cell::Cell::get);
                let out = run();
                (
                    out,
                    crate::adjunction::KAN_WORK.with(std::cell::Cell::get) - before,
                )
            }

            /// The transposes at one pair of instances take Sigma, Delta
            /// and Pi of the two instances once, not once per homomorphism,
            /// so the laws take a few dozen Kan extensions however large the
            /// hom-sets are (counted, not timed; once per homomorphism would
            /// be over `10^4`). L1 and L1' run on generated cases whose
            /// Sigma I holds six rows in one table, so that
            /// `Hom(I, Delta Sigma I)` holds `6^6` homomorphisms, each
            /// transposed both ways; L2 runs along the identity on five rows
            /// with equal cells, so that `Hom(J, Pi I)` holds `5^5`.
            #[test]
            fn transposing_a_hom_set_takes_each_kan_extension_once() {
                for seed in [752_u64, 487] {
                    let case = case(seed);
                    let sigma = f_sigma_restricted(&case.i, &case.f)
                        .unwrap_or_else(|e| panic!("{e}"))
                        .0;
                    let delta = f_delta(&sigma, &case.f).unwrap_or_else(|e| panic!("{e}"));
                    assert_eq!(
                        oracle::brute_homs(&case.i, &delta).len(),
                        6_usize.pow(6),
                        "|Hom(I, Delta Sigma I)| at seed {seed}"
                    );
                    let ((), work) = kan_work(|| sigma_laws(&case));
                    assert!(work <= 64, "{work} Kan extensions for L1 at seed {seed}");
                }
                let side = fixture::side(&[("a", &["n"])], &[]);
                let spec =
                    fixture::spec(side.clone(), side, &[("a", "a")], &[], &[(("a", "n"), "n")]);
                let f = fixture::functor(&spec);
                let x = FInstance::new()
                    .with_table("a", (0..5).map(|_| fixture::row(&[("n", Value::Int(0))])))
                    .unwrap_or_else(|e| panic!("{e}"));
                let (homs, work) = kan_work(|| delta_pi(&spec, &f, &x, &x, 81));
                assert_eq!(homs, Some(5_usize.pow(5)), "|Hom(J, Pi I)|");
                assert!(work <= 64, "{work} Kan extensions for L2");
            }

            /// `x` with its rows renumbered in reverse above 100 and each
            /// labelled null `n` read as `n + 50`: an isomorphic copy.
            fn relabel(x: &FInstance) -> FInstance {
                let top = x.max_row_id().map_or(0, |r| r.0);
                let id = |r: RowId| RowId(100 + top - r.0);
                let tables: Vec<(Name, Vec<(RowId, Row)>)> = x
                    .table_names()
                    .into_iter()
                    .map(|name| {
                        let rows = x
                            .rows(name)
                            .into_iter()
                            .map(|(r, cells)| {
                                let cells: Row = cells
                                    .iter()
                                    .map(|(c, v)| {
                                        let v = v.as_labeled_null().map_or_else(
                                            || v.clone(),
                                            |n| Value::LabeledNull(n + 50),
                                        );
                                        (c.clone(), v)
                                    })
                                    .collect();
                                (id(r), cells)
                            })
                            .collect();
                        (name.clone(), rows)
                    })
                    .collect();
                let pairs = x.foreign_keys().into_iter().map(|(e, pairs)| {
                    (
                        e.clone(),
                        pairs
                            .iter()
                            .map(|(s, t)| (id(*s), id(*t)))
                            .collect::<Vec<_>>(),
                    )
                });
                FInstance::from_parts(tables, pairs).unwrap_or_else(|e| panic!("{e}"))
            }

            /// The isomorphism search agrees with its definition by
            /// homomorphisms, both ways round, on generated small instances:
            /// each against itself, a relabelled copy, a homomorphic image
            /// and another generated instance of its schema.
            #[test]
            fn brute_iso_agrees_with_its_definition() {
                let (mut isomorphic, mut not) = (0, 0);
                for seed in 0..2000_u64 {
                    let mut case = case(seed);
                    let side = case.spec.src.clone();
                    let x = case.x.clone();
                    let (image, _) = hom_out(&mut case.rng, &side, &x);
                    let other = gen_instance(&mut case.rng, &side, 0);
                    for y in [x.clone(), relabel(&x), image, other] {
                        for (one, two) in [(&x, &y), (&y, &x)] {
                            let expected = oracle::brute_iso_by_homs(one, two);
                            assert_eq!(
                                oracle::brute_iso(one, two),
                                expected,
                                "seed {seed}: {one:?} against {two:?}"
                            );
                            if expected {
                                isomorphic += 1;
                            } else {
                                not += 1;
                            }
                        }
                    }
                }
                assert!(
                    isomorphic > 0 && not > 0,
                    "{isomorphic} isomorphic, {not} not"
                );
                println!("{isomorphic} isomorphic pairs, {not} not");
            }

            /// `inst` with one row corrupted: a stray cell, a missing cell or
            /// a labelled null nested in a list; unchanged when it has no row.
            fn corrupt(rng: &mut super::Rng, inst: &FInstance) -> FInstance {
                let mut out = inst.clone();
                let rows = inst.row_ids();
                let Some(&row) = rng.pick(&rows) else {
                    return out;
                };
                if let Some(cells) = out.row_mut(row) {
                    let columns: Vec<String> = cells.keys().cloned().collect();
                    match (rng.below(3), rng.pick(&columns)) {
                        (0, _) | (_, None) => {
                            cells.insert("zz-stray".to_owned(), Value::Int(0));
                        }
                        (1, Some(column)) => {
                            cells.remove(column);
                        }
                        (_, Some(column)) => {
                            cells.insert(column.clone(), Value::List(vec![Value::LabeledNull(9)]));
                        }
                    }
                }
                out
            }
        }
    }
}
