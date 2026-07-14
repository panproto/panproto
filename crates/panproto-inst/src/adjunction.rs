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

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::FInstance;
use crate::instance_hom::{FInstanceHom, WInstanceHom};
use crate::value::Value;
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

/// The image of a vertex name (as a string key) under the vertex map.
fn map_vertex_str(migration: &CompiledMigration, vertex: &str) -> String {
    migration
        .vertex_remap
        .get(vertex)
        .map_or_else(|| vertex.to_owned(), ToString::to_string)
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

/// Invert the vertex map, failing when two source vertices share an image.
fn invert_vertex_map(
    migration: &CompiledMigration,
) -> Result<HashMap<Name, Name>, AdjunctionError> {
    let mut inverse: HashMap<Name, Name> = HashMap::with_capacity(migration.vertex_remap.len());
    for (src, tgt) in &migration.vertex_remap {
        if let Some(existing) = inverse.insert(tgt.clone(), src.clone()) {
            if existing != *src {
                return Err(AdjunctionError::NonInjectiveVertexMap {
                    first: existing,
                    second: src.clone(),
                    target: tgt.clone(),
                });
            }
        }
    }
    Ok(inverse)
}

/// Invert the edge map, failing when two source edges share an image.
fn invert_edge_map(migration: &CompiledMigration) -> Result<HashMap<Edge, Edge>, AdjunctionError> {
    let mut inverse: HashMap<Edge, Edge> = HashMap::with_capacity(migration.edge_remap.len());
    for (src, tgt) in &migration.edge_remap {
        if let Some(existing) = inverse.insert(tgt.clone(), src.clone()) {
            if existing != *src {
                return Err(AdjunctionError::NonInjectiveEdgeMap {
                    src: tgt.src.clone(),
                    tgt: tgt.tgt.clone(),
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

/// The image of `Sigma_F` together with the row layout needed to build the
/// unit, counit, and transposes.
struct SigmaImage {
    /// The pushed-forward instance.
    instance: FInstance,
    /// For each source vertex `s`, the offset of `X.table(s)`'s rows within
    /// the target table `F(s)` (its block start in the coproduct).
    offset: HashMap<String, usize>,
    /// For each target vertex `t`, the ordered `(source vertex, block
    /// length)` pairs whose concatenation forms `t`'s table.
    groups: HashMap<String, Vec<(String, usize)>>,
}

/// Compute `Sigma_F X` and its coproduct layout.
///
/// Source vertices are visited in sorted order so the block offsets are
/// deterministic, which is what makes the transpose maps well defined.
fn sigma_layout(x: &FInstance, migration: &CompiledMigration) -> SigmaImage {
    let mut source_vertices: Vec<&String> = x.tables.keys().collect();
    source_vertices.sort();

    let mut tables: HashMap<String, Vec<HashMap<String, Value>>> = HashMap::new();
    let mut offset: HashMap<String, usize> = HashMap::with_capacity(source_vertices.len());
    let mut groups: HashMap<String, Vec<(String, usize)>> = HashMap::new();

    for source in source_vertices {
        let target = map_vertex_str(migration, source);
        let rows = &x.tables[source];
        let block = tables.entry(target.clone()).or_default();
        offset.insert(source.clone(), block.len());
        groups
            .entry(target)
            .or_default()
            .push((source.clone(), rows.len()));
        block.extend(rows.iter().cloned());
    }

    let mut foreign_keys: HashMap<Edge, Vec<(usize, usize)>> = HashMap::new();
    for (edge, pairs) in &x.foreign_keys {
        let target_edge = map_edge(migration, edge);
        let src_offset = offset.get(edge.src.as_str()).copied().unwrap_or(0);
        let tgt_offset = offset.get(edge.tgt.as_str()).copied().unwrap_or(0);
        foreign_keys
            .entry(target_edge)
            .or_default()
            .extend(pairs.iter().map(|(i, j)| (i + src_offset, j + tgt_offset)));
    }

    SigmaImage {
        instance: FInstance {
            tables,
            foreign_keys,
        },
        offset,
        groups,
    }
}

/// `Sigma_F` for set-valued functor instances: the left Kan extension.
///
/// For each target vertex `t` it forms the coproduct of the source tables in
/// its fibre `F^{-1}(t)`, offsetting foreign keys into the concatenation.
#[must_use]
pub fn f_sigma(x: &FInstance, migration: &CompiledMigration) -> FInstance {
    sigma_layout(x, migration).instance
}

/// `Delta_F` for set-valued functor instances: precomposition.
///
/// Each source vertex `s` receives a copy of the target table `F(s)`, and
/// each source edge `e` a copy of the target foreign key `F(e)`. This is
/// total and stays within the functor category even for merging maps, where
/// a merged table is duplicated into each source vertex of its fibre.
#[must_use]
pub fn f_delta(y: &FInstance, migration: &CompiledMigration) -> FInstance {
    let mut tables = HashMap::with_capacity(migration.vertex_remap.len());
    for (source, target) in &migration.vertex_remap {
        let rows = y.tables.get(target.as_str()).cloned().unwrap_or_default();
        tables.insert(source.to_string(), rows);
    }

    let mut foreign_keys = HashMap::with_capacity(migration.edge_remap.len());
    for (source_edge, target_edge) in &migration.edge_remap {
        let pairs = y.foreign_keys.get(target_edge).cloned().unwrap_or_default();
        foreign_keys.insert(source_edge.clone(), pairs);
    }

    FInstance {
        tables,
        foreign_keys,
    }
}

/// The unit `eta_X : X -> Delta_F(Sigma_F X)` of the functor adjunction.
///
/// Each row `i` of `X.table(s)` maps to its copy at `offset(s) + i` inside
/// the coproduct table `Sigma_F(X).table(F s) = Delta_F(Sigma_F X).table(s)`.
#[must_use]
pub fn f_unit(x: &FInstance, migration: &CompiledMigration) -> FInstanceHom {
    let sigma = sigma_layout(x, migration);
    let row_maps = x
        .tables
        .iter()
        .map(|(source, rows)| {
            let base = sigma.offset.get(source).copied().unwrap_or(0);
            (source.clone(), (0..rows.len()).map(|i| base + i).collect())
        })
        .collect();
    FInstanceHom::new(row_maps)
}

/// The counit `eps_Y : Sigma_F(Delta_F Y) -> Y` of the functor adjunction.
///
/// `Sigma_F(Delta_F Y).table(t)` is a stack of `|F^{-1}(t)|` copies of
/// `Y.table(t)`; the counit collapses every copy back onto `Y` by taking
/// the row index modulo `|Y.table(t)|`.
#[must_use]
pub fn f_counit(y: &FInstance, migration: &CompiledMigration) -> FInstanceHom {
    let delta = f_delta(y, migration);
    let sigma = sigma_layout(&delta, migration);
    let row_maps = sigma
        .instance
        .tables
        .iter()
        .map(|(target, rows)| {
            let height = y.tables.get(target).map_or(0, Vec::len);
            let map = (0..rows.len())
                .map(|r| if height == 0 { 0 } else { r % height })
                .collect();
            (target.clone(), map)
        })
        .collect();
    FInstanceHom::new(row_maps)
}

/// Transpose `Hom_T(Sigma_F X, Y) -> Hom_S(X, Delta_F Y)`, sending a
/// homomorphism `g` to `Delta_F(g) . eta_X`.
///
/// Concretely, `(psi g).table(s)` at row `i` is `g.table(F s)` at row
/// `offset(s) + i`.
#[must_use]
pub fn f_transpose_left(
    g: &FInstanceHom,
    x: &FInstance,
    migration: &CompiledMigration,
) -> FInstanceHom {
    let sigma = sigma_layout(x, migration);
    let row_maps = x
        .tables
        .iter()
        .map(|(source, rows)| {
            let target = map_vertex_str(migration, source);
            let base = sigma.offset.get(source).copied().unwrap_or(0);
            let image = g.row_maps.get(&target);
            let map = (0..rows.len())
                .map(|i| {
                    image
                        .and_then(|rows| rows.get(base + i))
                        .copied()
                        .unwrap_or(0)
                })
                .collect();
            (source.clone(), map)
        })
        .collect();
    FInstanceHom::new(row_maps)
}

/// Transpose `Hom_S(X, Delta_F Y) -> Hom_T(Sigma_F X, Y)`, sending a
/// homomorphism `f` to `eps_Y . Sigma_F(f)`, the inverse of
/// [`f_transpose_left`].
///
/// Concretely, `(phi f).table(t)` at row `offset(s) + i` is `f.table(s)` at
/// row `i`, for the source vertex `s` owning that block of the coproduct.
#[must_use]
pub fn f_transpose_right(
    f: &FInstanceHom,
    x: &FInstance,
    migration: &CompiledMigration,
) -> FInstanceHom {
    let sigma = sigma_layout(x, migration);
    let row_maps = sigma
        .instance
        .tables
        .keys()
        .map(|target| {
            let mut map: Vec<usize> = Vec::new();
            if let Some(blocks) = sigma.groups.get(target) {
                for (source, length) in blocks {
                    let image = f.row_maps.get(source);
                    map.extend(
                        (0..*length)
                            .map(|i| image.and_then(|rows| rows.get(i)).copied().unwrap_or(0)),
                    );
                }
            }
            (target.clone(), map)
        })
        .collect();
    FInstanceHom::new(row_maps)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use panproto_schema::Vertex;
    use smallvec::SmallVec;

    use super::*;
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
            .with_table("b", vec![row(3)])
            .with_table("c", vec![row(9)]);
        let sigma = f_sigma(&x, &m);
        // a (2 rows) and b (1 row) merge into t (3 rows); c becomes u.
        assert_eq!(sigma.tables.get("t").map(Vec::len), Some(3));
        assert_eq!(sigma.tables.get("u").map(Vec::len), Some(1));
    }

    #[test]
    fn f_unit_and_counit_check_over_merge() {
        let m = merge_migration();
        let x = FInstance::new()
            .with_table("a", vec![row(1), row(2)])
            .with_table("b", vec![row(3)])
            .with_table("c", vec![row(9)])
            .with_foreign_key(edge("a", "c", "ea"), vec![(0, 0), (1, 0)])
            .with_foreign_key(edge("b", "c", "eb"), vec![(0, 0)]);

        let sigma = f_sigma(&x, &m);
        let delta_sigma = f_delta(&sigma, &m);
        let unit = f_unit(&x, &m);
        unit.check(&x, &delta_sigma)
            .expect("unit is a valid homomorphism into Delta(Sigma X)");

        // First triangle identity: phi(eta_X) = id on Sigma X.
        let phi_unit = f_transpose_right(&unit, &x, &m);
        assert_eq!(phi_unit, FInstanceHom::identity(&sigma));

        let y = FInstance::new()
            .with_table("t", vec![row(5), row(6)])
            .with_table("u", vec![row(7)])
            .with_foreign_key(edge("t", "u", "e"), vec![(0, 0), (1, 0)]);
        let delta_y = f_delta(&y, &m);
        let sigma_delta_y = f_sigma(&delta_y, &m);
        let counit = f_counit(&y, &m);
        counit
            .check(&sigma_delta_y, &y)
            .expect("counit is a valid homomorphism onto Y");

        // Second triangle identity: psi(eps_Y) = id on Delta Y.
        let psi_counit = f_transpose_left(&counit, &delta_y, &m);
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

        /// Round-robin foreign-key pairs from a source table of `src_len`
        /// rows into a target table of `tgt_len` rows.
        fn round_robin(src_len: usize, tgt_len: usize) -> Vec<(usize, usize)> {
            if tgt_len == 0 {
                return Vec::new();
            }
            (0..src_len).map(|i| (i, i % tgt_len)).collect()
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
                            x = x.with_table(s_name(i), rows);
                        }
                        for (i, e) in s_edges.iter().enumerate() {
                            let pairs = round_robin(src_rows[i], src_rows[i + 1]);
                            if !pairs.is_empty() {
                                x = x.with_foreign_key(e.clone(), pairs);
                            }
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
                            y = y.with_table(t_name(j), rows);
                        }
                        for (i, _e) in s_edges.iter().enumerate() {
                            let te = edge(
                                &t_name(assign[i]),
                                &t_name(assign[i + 1]),
                                &format!("te{i}"),
                            );
                            let pairs = round_robin(tgt_rows[assign[i]], tgt_rows[assign[i + 1]]);
                            if !pairs.is_empty() {
                                y = y.with_foreign_key(te, pairs);
                            }
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
                let sigma_x = f_sigma(&x, &migration);
                let delta_sigma_x = f_delta(&sigma_x, &migration);
                let unit = f_unit(&x, &migration);
                prop_assert!(unit.check(&x, &delta_sigma_x).is_ok());
                let phi_unit = f_transpose_right(&unit, &x, &migration);
                prop_assert_eq!(phi_unit, FInstanceHom::identity(&sigma_x));

                // Counit and its triangle: psi(eps_Y) = id on Delta Y.
                let delta_y = f_delta(&y, &migration);
                let sigma_delta_y = f_sigma(&delta_y, &migration);
                let counit = f_counit(&y, &migration);
                prop_assert!(counit.check(&sigma_delta_y, &y).is_ok());
                let psi_counit = f_transpose_left(&counit, &delta_y, &migration);
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
                let sigma_x = f_sigma(&x, &migration);
                let mut y_g = FInstance::new();
                for (t, rows) in &sigma_x.tables {
                    let mut doubled = rows.clone();
                    doubled.extend(rows.iter().cloned());
                    y_g = y_g.with_table(t.clone(), doubled);
                }
                for (e, pairs) in &sigma_x.foreign_keys {
                    let len_src = sigma_x.tables.get(e.src.as_str()).map_or(0, Vec::len);
                    let len_tgt = sigma_x.tables.get(e.tgt.as_str()).map_or(0, Vec::len);
                    let mut all = pairs.clone();
                    all.extend(pairs.iter().map(|(i, j)| (i + len_src, j + len_tgt)));
                    y_g = y_g.with_foreign_key(e.clone(), all);
                }
                let g = FInstanceHom::identity(&sigma_x);
                prop_assert!(g.check(&sigma_x, &y_g).is_ok());

                let delta_y_g = f_delta(&y_g, &migration);
                let psi_g = f_transpose_left(&g, &x, &migration);
                prop_assert!(psi_g.check(&x, &delta_y_g).is_ok());
                // Round-trip phi . psi = id.
                let phi_psi_g = f_transpose_right(&psi_g, &x, &migration);
                prop_assert_eq!(phi_psi_g, g);

                // Direction 2. A valid f : X_f -> Delta Y with X_f = Delta Y
                // and f the identity; phi(f) must be a valid Sigma X_f -> Y.
                let delta_y = f_delta(&y, &migration);
                let f = FInstanceHom::identity(&delta_y);
                prop_assert!(f.check(&delta_y, &delta_y).is_ok());
                let phi_f = f_transpose_right(&f, &delta_y, &migration);
                let sigma_delta_y = f_sigma(&delta_y, &migration);
                prop_assert!(phi_f.check(&sigma_delta_y, &y).is_ok());
                // Round-trip psi . phi = id.
                let psi_phi_f = f_transpose_left(&phi_f, &delta_y, &migration);
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
