//! Contraction as a compiled plan, derived from the migration.
//!
//! A restrict that drops a vertex of the source schema may leave two
//! surviving vertices joined only through the dropped one. Contraction joins
//! them again in the view: every surviving element reached from a surviving
//! element through dropped elements gets an edge (or an expanded path) to
//! it. Which pairs of schema vertices can be joined this way, and by which
//! target edge, depends only on the two schemas and the migration, so it is
//! compiled once into a [`ContractionPlan`] that the tree, table and graph
//! engines all read.
//!
//! # Readings
//!
//! A pair whose region of dropped vertices is acyclic ([`PlannedPair::bounded`])
//! joins its endpoints through walks of bounded length, so its realized pairs
//! are the answer to a finite union of fixed-length select-project-join
//! queries with duplicate elimination, one query per walk shape through the
//! region: the joins follow the dropped edges, the projection keeps the two
//! endpoints, and the survival predicates of conditionally dropped interior
//! vertices are the selections (a union of conjunctive queries when no
//! interior vertex is conditional; inferred). A pair whose region holds a
//! cycle joins its endpoints through walks of every length, and its realized
//! pairs are a transitive closure through the dropped region, which no finite
//! union of such queries expresses. The contraction of a path to an arrow in
//! spivak2012functorial (Def 3.3.1, stated) applies only where the dropped
//! schema edges are functional, which panproto edges in general are not
//! (brown2022computational, stated: typed graphs encode relations), so that
//! definition is not the semantics here.
//!
//! # Cost
//!
//! [`ContractionPlan::compile`] runs one forward search from each surviving
//! vertex and one backward search into each surviving vertex, each inside the
//! dropped part of the schema graph: `O(V (V + E))` for `V` vertices and `E`
//! edges. It then reads each planned pair's region off the two searches in
//! `O(E)` per pair, so the whole compilation is `O(V (V + E) + P E)` for `P`
//! planned pairs, which is the size of the regions it returns. No path is
//! ever enumerated.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use serde::{Deserialize, Serialize};

use crate::error::RestrictError;
use crate::reachability::strongly_connected_components;
use crate::wtype::{CompiledMigration, resolve_edge};

/// The pairs of source vertices that contraction can join, each with its
/// target, its region of dropped edges and its resolution.
///
/// Keyed by the source vertices `(a, b)`; serialized as a sequence of
/// `(key, value)` entries in key order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractionPlan {
    /// Every planned pair, by its source vertices `(a, b)`.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub pairs: BTreeMap<(Name, Name), PlannedPair>,
}

/// One pair of source vertices that contraction can join.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedPair {
    /// The images `(a', b')` of the pair under the vertex map.
    pub target: (Name, Name),
    /// The source schema edges on some walk from `a` to `b` whose inner
    /// vertices are all interior-capable.
    pub region: BTreeSet<Edge>,
    /// Whether every such walk has bounded length: false exactly when the
    /// interior-capable part of the region holds a cycle.
    pub bounded: bool,
    /// The target edge or path the pair resolves to.
    pub resolution: PairResolution,
}

/// What a contracted pair resolves to in the target schema.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairResolution {
    /// One target edge from `a'` to `b'`.
    Label(Edge),
    /// A path of target edges from `a'` through `intermediates` to `b'`:
    /// `labels[h]` enters `intermediates[h]`, and the last label enters
    /// `b'`.
    Expand {
        /// The anchors of the inner vertices of the path, from `a'`'s side.
        intermediates: Vec<Name>,
        /// The target edges of the path, one more than the intermediates.
        labels: Vec<Edge>,
    },
    /// No target edge or path is determined.
    Unresolved(UnresolvedReason),
}

/// Why a contracted pair, or one hop of an expansion, is unresolved.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// The target schema has no edge between the two anchors.
    NoEdge,
    /// The target schema has several edges between the two anchors and the
    /// migration's resolver chooses none.
    Ambiguous {
        /// How many edges join the two anchors.
        count: usize,
    },
}

/// How a lens contracts: not at all, through bounded regions only, or
/// through a region holding a cycle.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractionMode {
    /// The plan has no pairs.
    None,
    /// Every pair is bounded.
    Declared,
    /// Some pair is unbounded: contraction computes a transitive closure.
    Closure,
}

/// The image of a source vertex under the migration's vertex map.
pub(crate) fn remap<'a>(migration: &'a CompiledMigration, vertex: &'a Name) -> &'a Name {
    migration.vertex_remap.get(vertex).unwrap_or(vertex)
}

/// Whether a source vertex survives the migration: its image is a
/// surviving vertex.
pub(crate) fn survives(migration: &CompiledMigration, vertex: &Name) -> bool {
    migration.surviving_verts.contains(remap(migration, vertex))
}

/// Whether a source vertex can be the inner vertex of a contracted walk:
/// it does not survive, or a predicate may drop its elements.
pub(crate) fn interior_capable(migration: &CompiledMigration, vertex: &Name) -> bool {
    !survives(migration, vertex) || migration.conditional_survival.contains_key(vertex)
}

/// The resolution of the contracted pair of target anchors `(a', b')`.
///
/// In this order: the resolver's entry for `(a', b')`; the unique target
/// edge from `a'` to `b'`; when there is no such edge or several, the
/// expansion path the migration records for `(a', b')`, each hop resolved by
/// its resolver entry or else by the unique target edge between its anchors
/// ([`resolve_edge`]), never by another expansion path, giving
/// [`PairResolution::Expand`], or the reason of the first hop that fails;
/// with no expansion path, [`UnresolvedReason::NoEdge`] or
/// [`UnresolvedReason::Ambiguous`]. This is the order in which tree
/// contraction has always resolved a contracted arc, and every shape uses
/// this one function.
///
/// The result is not checked against the target schema; the plan compilers
/// check it.
#[must_use]
pub fn resolve_pair(
    tgt: &Schema,
    migration: &CompiledMigration,
    a: &Name,
    b: &Name,
) -> PairResolution {
    if let Some(edge) = migration.resolver.get(&(a.clone(), b.clone())) {
        return PairResolution::Label(edge.clone());
    }
    let direct = tgt.edges_between(a, b);
    if direct.len() == 1 {
        return PairResolution::Label(direct[0].clone());
    }
    if let Some(intermediates) = migration.expansion_path.get(&(a.clone(), b.clone())) {
        let mut labels = Vec::with_capacity(intermediates.len() + 1);
        for (x, y) in hops(a, intermediates, b) {
            match resolve_edge(tgt, &migration.resolver, x, y) {
                Ok(edge) => labels.push(edge),
                Err(error) => return PairResolution::Unresolved(unresolved_reason(&error)),
            }
        }
        return PairResolution::Expand {
            intermediates: intermediates.clone(),
            labels,
        };
    }
    PairResolution::Unresolved(match direct.len() {
        0 => UnresolvedReason::NoEdge,
        count => UnresolvedReason::Ambiguous { count },
    })
}

/// The anchors of the hop of `(a', b')` that leaves the pair unresolved: the
/// first failing hop of its expansion path, or the pair itself when the
/// migration records none.
#[must_use]
pub fn failing_hop(
    tgt: &Schema,
    migration: &CompiledMigration,
    a: &Name,
    b: &Name,
) -> (Name, Name) {
    if let Some(intermediates) = migration.expansion_path.get(&(a.clone(), b.clone())) {
        for (x, y) in hops(a, intermediates, b) {
            if resolve_edge(tgt, &migration.resolver, x, y).is_err() {
                return (x.clone(), y.clone());
            }
        }
    }
    (a.clone(), b.clone())
}

/// The consecutive anchor pairs of the path `a, intermediates.., b`.
fn hops<'a>(
    a: &'a Name,
    intermediates: &'a [Name],
    b: &'a Name,
) -> impl Iterator<Item = (&'a Name, &'a Name)> + 'a {
    let anchors: Vec<&'a Name> = std::iter::once(a)
        .chain(intermediates.iter())
        .chain(std::iter::once(b))
        .collect();
    (0..anchors.len().saturating_sub(1)).map(move |i| (anchors[i], anchors[i + 1]))
}

/// The reason a hop's resolution failed.
const fn unresolved_reason(error: &RestrictError) -> UnresolvedReason {
    match error {
        RestrictError::AmbiguousEdge { count, .. } => UnresolvedReason::Ambiguous { count: *count },
        _ => UnresolvedReason::NoEdge,
    }
}

/// The first label of a resolution that is not a target edge joining the
/// anchors it stands between, `a'` and `b'` for a label, consecutive anchors
/// of `[a', intermediates.., b']` for an expansion.
#[must_use]
pub(crate) fn incoherent_label<'r>(
    tgt: &Schema,
    a: &Name,
    b: &Name,
    resolution: &'r PairResolution,
) -> Option<&'r Edge> {
    let joins = |edge: &Edge, x: &Name, y: &Name| {
        edge.src == *x && edge.tgt == *y && tgt.edges.contains_key(edge)
    };
    match resolution {
        PairResolution::Label(edge) => (!joins(edge, a, b)).then_some(edge),
        PairResolution::Expand {
            intermediates,
            labels,
        } => hops(a, intermediates, b)
            .zip(labels)
            .find(|((x, y), edge)| !joins(edge, x, y))
            .map(|(_, edge)| edge)
            .or_else(|| labels.get(intermediates.len() + 1)),
        PairResolution::Unresolved(_) => None,
    }
}

/// The source schema as a graph: its vertices, ascending, and each vertex's
/// out-edges and in-edges, ascending.
struct SchemaGraph<'s> {
    vertices: BTreeSet<&'s Name>,
    edges: Vec<&'s Edge>,
    out: BTreeMap<&'s Name, Vec<&'s Edge>>,
    into: BTreeMap<&'s Name, Vec<&'s Edge>>,
}

impl<'s> SchemaGraph<'s> {
    fn new(src: &'s Schema) -> Self {
        let mut edges: Vec<&Edge> = src.edges.keys().collect();
        edges.sort_unstable();
        let mut vertices: BTreeSet<&Name> = src.vertices.keys().collect();
        let mut out: BTreeMap<&Name, Vec<&Edge>> = BTreeMap::new();
        let mut into: BTreeMap<&Name, Vec<&Edge>> = BTreeMap::new();
        for &edge in &edges {
            vertices.insert(&edge.src);
            vertices.insert(&edge.tgt);
            out.entry(&edge.src).or_default().push(edge);
            into.entry(&edge.tgt).or_default().push(edge);
        }
        Self {
            vertices,
            edges,
            out,
            into,
        }
    }

    /// The interior-capable vertices reachable from `start` by a non-empty
    /// walk whose vertices after `start` are all interior-capable.
    fn forward(&self, start: &'s Name, interior: &BTreeSet<&'s Name>) -> BTreeSet<&'s Name> {
        Self::search(start, interior, &self.out, |e| &e.tgt)
    }

    /// The interior-capable vertices that reach `end` by a non-empty walk
    /// whose vertices before `end` are all interior-capable.
    fn backward(&self, end: &'s Name, interior: &BTreeSet<&'s Name>) -> BTreeSet<&'s Name> {
        Self::search(end, interior, &self.into, |e| &e.src)
    }

    fn search(
        start: &'s Name,
        interior: &BTreeSet<&'s Name>,
        adjacency: &BTreeMap<&'s Name, Vec<&'s Edge>>,
        step: impl Fn(&'s Edge) -> &'s Name,
    ) -> BTreeSet<&'s Name> {
        let mut seen: BTreeSet<&Name> = BTreeSet::new();
        let mut frontier: Vec<&Name> = vec![start];
        while let Some(at) = frontier.pop() {
            for &edge in adjacency.get(at).map_or(&[][..], Vec::as_slice) {
                let next = step(edge);
                if interior.contains(next) && seen.insert(next) {
                    frontier.push(next);
                }
            }
        }
        seen
    }
}

impl ContractionPlan {
    /// Compile the contraction plan of `migration` from `src` to `tgt`.
    ///
    /// A source vertex is endpoint-capable when it survives the migration
    /// (its image under the vertex map is a surviving vertex), and
    /// interior-capable when it does not survive or carries a survival
    /// predicate, so a conditional vertex is both. For each endpoint-capable
    /// `a`, a forward search through interior-capable vertices collects
    /// every endpoint-capable `b` (`b == a` allowed) reached through at least
    /// one interior-capable vertex; each such `(a, b)` is a planned pair.
    ///
    /// With `I` the interior-capable vertices, `F` those of `I` reachable
    /// from `a` by a non-empty walk whose vertices after `a` lie in `I`, and
    /// `B` those of `I` that reach `b` by a non-empty walk whose vertices
    /// before `b` lie in `I`, the pair's region is the set of source edges
    /// `u -> v` with `u` in `F` and (`v = b` or `v` in `B`), or with `u = a`
    /// and `v` in `B`: exactly the edges on some walk from `a` to `b` of
    /// length at least two whose inner vertices lie in `I`. The pair is
    /// bounded unless the region's edges between interior-capable vertices
    /// hold a cycle (a strongly connected component with two vertices, or a
    /// loop). Its target is the image of `(a, b)`, and its resolution is
    /// [`resolve_pair`] of the target.
    ///
    /// # Errors
    ///
    /// [`RestrictError::IncoherentPairImage`] for the first pair, in
    /// ascending source pair, whose resolution names an edge that is not a
    /// target edge joining the anchors it stands between.
    pub fn compile(
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
    ) -> Result<Self, RestrictError> {
        let graph = SchemaGraph::new(src);
        let interior: BTreeSet<&Name> = graph
            .vertices
            .iter()
            .copied()
            .filter(|v| interior_capable(migration, v))
            .collect();
        let endpoint = |v: &Name| survives(migration, v);
        let mut backward: BTreeMap<&Name, BTreeSet<&Name>> = BTreeMap::new();
        let mut pairs = BTreeMap::new();
        for &a in graph.vertices.iter().filter(|v| endpoint(v)) {
            let forward = graph.forward(a, &interior);
            let mut ends: BTreeSet<&Name> = BTreeSet::new();
            for &x in &forward {
                for &edge in graph.out.get(x).map_or(&[][..], Vec::as_slice) {
                    if endpoint(&edge.tgt) {
                        ends.insert(&edge.tgt);
                    }
                }
            }
            for b in ends {
                let into_b = backward
                    .entry(b)
                    .or_insert_with(|| graph.backward(b, &interior));
                let region: BTreeSet<Edge> = graph
                    .edges
                    .iter()
                    .filter(|e| {
                        let to_b = e.tgt == *b || into_b.contains(&e.tgt);
                        (forward.contains(&e.src) && to_b)
                            || (e.src == *a && into_b.contains(&e.tgt))
                    })
                    .map(|e| (*e).clone())
                    .collect();
                let bounded = !has_cycle(&region, &interior);
                let target = (remap(migration, a).clone(), remap(migration, b).clone());
                let resolution = resolve_pair(tgt, migration, &target.0, &target.1);
                if let Some(image) = incoherent_label(tgt, &target.0, &target.1, &resolution) {
                    return Err(RestrictError::IncoherentPairImage {
                        pair: Box::new(target),
                        image: Box::new(image.clone()),
                    });
                }
                pairs.insert(
                    (a.clone(), b.clone()),
                    PlannedPair {
                        target,
                        region,
                        bounded,
                        resolution,
                    },
                );
            }
        }
        Ok(Self { pairs })
    }

    /// The plan's mode: [`ContractionMode::None`] when it has no pairs,
    /// [`ContractionMode::Declared`] when every pair is bounded, and
    /// [`ContractionMode::Closure`] otherwise.
    #[must_use]
    pub fn mode(&self) -> ContractionMode {
        if self.pairs.is_empty() {
            ContractionMode::None
        } else if self.pairs.values().all(|p| p.bounded) {
            ContractionMode::Declared
        } else {
            ContractionMode::Closure
        }
    }

    /// The planned pair of the source vertices `(a, b)`.
    #[must_use]
    pub fn pair(&self, a: &Name, b: &Name) -> Option<&PlannedPair> {
        self.pairs.get(&(a.clone(), b.clone()))
    }
}

/// Whether the edges of `region` between interior-capable vertices hold a
/// cycle.
fn has_cycle(region: &BTreeSet<Edge>, interior: &BTreeSet<&Name>) -> bool {
    let inner: Vec<(&Name, &Name)> = region
        .iter()
        .filter(|e| interior.contains(&e.src) && interior.contains(&e.tgt))
        .map(|e| (&e.src, &e.tgt))
        .collect();
    if inner.iter().any(|(u, v)| u == v) {
        return true;
    }
    let nodes: BTreeSet<&Name> = inner.iter().flat_map(|(u, v)| [*u, *v]).collect();
    strongly_connected_components(nodes, inner.iter().copied())
        .iter()
        .any(|component| component.len() > 1)
}
