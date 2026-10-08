//! Exhaustive small-scope enumerators of labelled structures.
//!
//! Every enumerator lists labelled structures (ids, positions and orders
//! are part of the structure) with no pruning of isomorphic copies, and has
//! beside it a closed formula for its count, computed independently of the
//! enumeration, so a test can check the enumeration's size against it.

pub mod dsl_semantics;
pub mod edit_lenses;
pub mod edits;
pub mod formats;
pub mod kan_extensions;
pub mod keys_and_registration;
pub mod lenses;
pub mod migration_solver;
pub mod parsing;
pub mod representation;
pub mod resolution;
pub mod restrict_and_contraction;
pub mod segment_execution;
pub mod segments;
pub mod theories;
pub mod validation;
pub mod value_io;
pub mod values;
pub mod vcs;

use std::collections::{BTreeMap, BTreeSet};

use crate::edit::{AttrKey, AttrVal, GElem, GPayload, GraphDelta, NodeMeta};
use crate::migration::Migration;
use crate::model::{GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::schema::Schema;
use crate::value::Presence;
use crate::value::Value;

/// `n` choose `k`.
#[must_use]
pub fn choose(n: u64, k: u64) -> u64 {
    if k > n {
        return 0;
    }
    (1..=k).fold(1u64, |acc, i| acc * (n - k + i) / i)
}

/// Every subset of `items` of size at most `max`, each in item order.
#[must_use]
pub fn subsets<T: Clone>(items: &[T], max: usize) -> Vec<Vec<T>> {
    let mut out = vec![Vec::new()];
    for x in items {
        let grown: Vec<Vec<T>> = out
            .iter()
            .filter(|s| s.len() < max)
            .map(|s| {
                let mut t = s.clone();
                t.push(x.clone());
                t
            })
            .collect();
        out.extend(grown);
    }
    out
}

/// Every word over `alphabet` of length at most `max_len`.
#[must_use]
pub fn sequences<T: Clone>(alphabet: &[T], max_len: usize) -> Vec<Vec<T>> {
    let mut out = vec![Vec::new()];
    let mut level = vec![Vec::new()];
    for _ in 0..max_len {
        level = level
            .iter()
            .flat_map(|w: &Vec<T>| {
                alphabet.iter().map(move |a| {
                    let mut x = w.clone();
                    x.push(a.clone());
                    x
                })
            })
            .collect();
        out.extend(level.iter().cloned());
    }
    out
}

/// The count of [`sequences`]: `sum_{l <= max} |A|^l`.
#[must_use]
pub fn sequences_count(alphabet: u64, max_len: u32) -> u64 {
    (0..=max_len).map(|l| alphabet.pow(l)).sum()
}

/// Every function from `0..n` to `0..k`, as vectors.
fn functions(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    for _ in 0..n {
        out = out
            .into_iter()
            .flat_map(|f: Vec<usize>| {
                (0..k).map(move |v| {
                    let mut g = f.clone();
                    g.push(v);
                    g
                })
            })
            .collect();
    }
    out
}

/// Every schema on vertices `v0 .. v(n-1)` with a kind from `kinds` each
/// and a set of at most `max_edges` edges of kind `prop`, drawn from every
/// ordered vertex pair and every name in `names`.
#[must_use]
pub fn schemas(n: usize, kinds: &[&str], names: &[&str], max_edges: usize) -> Vec<Schema> {
    let verts: Vec<String> = (0..n).map(|i| format!("v{i}")).collect();
    let mut candidates = Vec::new();
    for a in &verts {
        for b in &verts {
            for l in names {
                candidates.push(SEdge::new(a, b, "prop", Some(l)));
            }
        }
    }
    let mut out = Vec::new();
    for ks in functions(n, kinds.len()) {
        for es in subsets(&candidates, max_edges) {
            let mut s = Schema::default();
            for (v, k) in verts.iter().zip(&ks) {
                s.vertices.insert(v.clone(), kinds[*k].to_owned());
            }
            s.edges = es.into_iter().collect();
            out.push(s);
        }
    }
    out
}

/// The count of [`schemas`]: `k^n * sum_{j <= m} C(n^2 l, j)`.
#[must_use]
pub fn schemas_count(n: u64, kinds: u64, names: u64, max_edges: u64) -> u64 {
    kinds.pow(u32::try_from(n).unwrap_or(0))
        * (0..=max_edges)
            .map(|j| choose(n * n * names, j))
            .sum::<u64>()
}

/// Every index vector of a mixed radix: each position `i` ranges over
/// `0 .. radices[i]`, the last position varying fastest.
fn mixed(radices: &[usize]) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    for r in radices {
        out = out
            .into_iter()
            .flat_map(|prefix: Vec<usize>| {
                (0..*r).map(move |v| {
                    let mut p = prefix.clone();
                    p.push(v);
                    p
                })
            })
            .collect();
    }
    out
}

/// Which fields of a migration [`migrations`] enumerates beyond the vertex
/// map, which it always enumerates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MigrationScope {
    flags: u8,
    /// The expansion paths: each ordered pair of target vertices without a
    /// path or with a word over the target vertices of length at most this
    /// bound (`None`: no paths).
    pub max_expansion: Option<usize>,
}

impl MigrationScope {
    const EDGES: u8 = 1;
    const RESOLVER: u8 = 2;
    const HYPER: u8 = 4;
    const CONDITIONAL: u8 = 8;
    const TRANSFORMED: u8 = 16;

    /// Every field.
    pub const ALL: Self = Self {
        // The five disjoint flags above occupy the low five bits.
        flags: 31,
        max_expansion: Some(1),
    };

    /// The vertex map alone.
    pub const VERTICES: Self = Self {
        flags: 0,
        max_expansion: None,
    };

    const fn has(self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// One choice of a map from `keys` to `images` or nothing: index 0 leaves a
/// key out, index `k` sends it to `images[k - 1]`.
fn remapped<T: Clone + Ord>(
    keys: &[T],
    images: &[T],
    choice: &[usize],
    surviving: &mut BTreeSet<T>,
    remap: &mut BTreeMap<T, T>,
) {
    for (k, c) in keys.iter().zip(choice) {
        if *c > 0 {
            let image = images[c - 1].clone();
            surviving.insert(image.clone());
            if image != *k {
                remap.insert(k.clone(), image);
            }
        }
    }
}

/// The hyper-resolver keys of a schema: each hyper-edge with each nonempty
/// set of its signature labels, ascending.
fn hyper_keys(src: &Schema) -> Vec<crate::migration::HyperShape> {
    let mut out = Vec::new();
    for (h, he) in &src.hyper_edges {
        let labels: Vec<String> = he.signature.keys().cloned().collect();
        for set in subsets(&labels, labels.len()) {
            if !set.is_empty() {
                out.push((h.clone(), set.into_iter().collect()));
            }
        }
    }
    out
}

/// The hyper-resolver images of one key: a target hyper-edge with each
/// partial map of the key's labels to that hyper-edge's labels.
fn hyper_images(
    tgt: &Schema,
    labels: &BTreeSet<String>,
) -> Vec<(String, BTreeMap<String, String>)> {
    let mut out = Vec::new();
    for (h, he) in &tgt.hyper_edges {
        let targets: Vec<&String> = he.signature.keys().collect();
        let keys: Vec<&String> = labels.iter().collect();
        for choice in functions(keys.len(), targets.len() + 1) {
            let map = keys
                .iter()
                .zip(&choice)
                .filter(|(_, c)| **c > 0)
                .map(|(k, c)| ((*k).clone(), targets[c - 1].clone()))
                .collect();
            out.push((h.clone(), map));
        }
    }
    out
}

/// Every migration from `src` to `tgt` in `scope`, labelled (no two
/// migrations of the list are equal, and none is pruned as isomorphic to
/// another).
///
/// Each source vertex is sent to a target vertex or dropped (the image
/// survives, and a vertex sent elsewhere than to its own name is in
/// `vertex_remap`); with `edges`, each source edge likewise to a target
/// edge or dropped (`surviving_edges`, `edge_remap`); with `resolver`, each
/// ordered pair of target vertices has no entry or any target edge; with
/// `max_expansion`, each ordered pair of target vertices has no path or a
/// word over the target vertices of length at most the bound; with `hyper`,
/// each key of `hyper_keys` has no entry or an image of
/// `hyper_images`; with `conditional` and `transformed`, every set of
/// source vertices. Coherent and incoherent migrations alike are listed,
/// so that refusals are enumerated too.
#[must_use]
pub fn migrations(src: &Schema, tgt: &Schema, scope: MigrationScope) -> Vec<Migration> {
    let sv: Vec<String> = src.vertices.keys().cloned().collect();
    let tv: Vec<String> = tgt.vertices.keys().cloned().collect();
    let se: Vec<crate::model::SEdge> = src.edges.iter().cloned().collect();
    let te: Vec<crate::model::SEdge> = tgt.edges.iter().cloned().collect();
    let pairs: Vec<(String, String)> = tv
        .iter()
        .flat_map(|a| tv.iter().map(move |b| (a.clone(), b.clone())))
        .collect();
    let paths: Vec<Vec<String>> = scope
        .max_expansion
        .map(|l| sequences(&tv, l))
        .unwrap_or_default();
    let hkeys = if scope.has(MigrationScope::HYPER) {
        hyper_keys(src)
    } else {
        Vec::new()
    };
    let himages: Vec<Vec<(String, BTreeMap<String, String>)>> =
        hkeys.iter().map(|(_, ls)| hyper_images(tgt, ls)).collect();
    let mut radices: Vec<usize> = vec![tv.len() + 1; sv.len()];
    if scope.has(MigrationScope::EDGES) {
        radices.extend(std::iter::repeat_n(te.len() + 1, se.len()));
    }
    if scope.has(MigrationScope::RESOLVER) {
        radices.extend(std::iter::repeat_n(te.len() + 1, pairs.len()));
    }
    if scope.max_expansion.is_some() {
        radices.extend(std::iter::repeat_n(paths.len() + 1, pairs.len()));
    }
    radices.extend(himages.iter().map(|i| i.len() + 1));
    for flag in [
        scope.has(MigrationScope::CONDITIONAL),
        scope.has(MigrationScope::TRANSFORMED),
    ] {
        if flag {
            radices.extend(std::iter::repeat_n(2, sv.len()));
        }
    }
    mixed(&radices)
        .into_iter()
        .map(|choice| {
            let mut m = Migration::default();
            let mut at = 0;
            let mut take = |n: usize| {
                let part = choice[at..at + n].to_vec();
                at += n;
                part
            };
            remapped(
                &sv,
                &tv,
                &take(sv.len()),
                &mut m.surviving_verts,
                &mut m.vertex_remap,
            );
            if scope.has(MigrationScope::EDGES) {
                remapped(
                    &se,
                    &te,
                    &take(se.len()),
                    &mut m.surviving_edges,
                    &mut m.edge_remap,
                );
            }
            if scope.has(MigrationScope::RESOLVER) {
                for ((a, b), c) in pairs.iter().zip(take(pairs.len())) {
                    if c > 0 {
                        m.resolver.insert((a.clone(), b.clone()), te[c - 1].clone());
                    }
                }
            }
            if scope.max_expansion.is_some() {
                for ((a, b), c) in pairs.iter().zip(take(pairs.len())) {
                    if c > 0 {
                        m.expansion_path
                            .insert((a.clone(), b.clone()), paths[c - 1].clone());
                    }
                }
            }
            for ((key, images), c) in hkeys.iter().zip(&himages).zip(take(hkeys.len())) {
                if c > 0 {
                    m.hyper_resolver.insert(key.clone(), images[c - 1].clone());
                }
            }
            for (flag, set) in [
                (scope.has(MigrationScope::CONDITIONAL), &mut m.conditional),
                (scope.has(MigrationScope::TRANSFORMED), &mut m.transformed),
            ] {
                if flag {
                    for (v, c) in sv.iter().zip(take(sv.len())) {
                        if c.checked_sub(1).is_some() {
                            set.insert(v.clone());
                        }
                    }
                }
            }
            m
        })
        .collect()
}

/// The sizes [`migrations_count`] reads from two schemas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationSizes {
    /// Source vertices.
    pub src_vertices: u64,
    /// Target vertices.
    pub tgt_vertices: u64,
    /// Source edges.
    pub src_edges: u64,
    /// Target edges.
    pub tgt_edges: u64,
    /// The signature size of each source hyper-edge.
    pub src_hyper_labels: Vec<u64>,
    /// The signature size of each target hyper-edge.
    pub tgt_hyper_labels: Vec<u64>,
}

/// The count of [`migrations`], by a closed formula over the schemas'
/// sizes.
///
/// `(|Vt| + 1)^|Vs|`, times `(|Et| + 1)^|Es|` with edges, times
/// `(|Et| + 1)^(|Vt|^2)` with the resolver, times `(1 + sum_{l <= L}
/// |Vt|^l)^(|Vt|^2)` with paths of length at most L, times, with the hyper
/// resolver, the product over source hyper-edges h with n labels and over
/// `1 <= k <= n` of `(1 + sum_{h'} (m_{h'} + 1)^k)^C(n, k)`, where m is a
/// target hyper-edge's signature size, times `2^|Vs|` for each of the
/// survival predicates and the transforms.
#[must_use]
pub fn migrations_count(sizes: &MigrationSizes, scope: MigrationScope) -> u64 {
    let pow = |b: u64, e: u64| b.pow(u32::try_from(e).unwrap_or(u32::MAX));
    let pairs = sizes.tgt_vertices * sizes.tgt_vertices;
    let mut n = pow(sizes.tgt_vertices + 1, sizes.src_vertices);
    if scope.has(MigrationScope::EDGES) {
        n *= pow(sizes.tgt_edges + 1, sizes.src_edges);
    }
    if scope.has(MigrationScope::RESOLVER) {
        n *= pow(sizes.tgt_edges + 1, pairs);
    }
    if let Some(l) = scope.max_expansion {
        let words: u64 = (0..=u64::try_from(l).unwrap_or(u64::MAX))
            .map(|k| pow(sizes.tgt_vertices, k))
            .sum();
        n *= pow(1 + words, pairs);
    }
    if scope.has(MigrationScope::HYPER) {
        for labels in &sizes.src_hyper_labels {
            for k in 1..=*labels {
                let images: u64 = sizes.tgt_hyper_labels.iter().map(|m| pow(m + 1, k)).sum();
                n *= pow(1 + images, choose(*labels, k));
            }
        }
    }
    for flag in [
        scope.has(MigrationScope::CONDITIONAL),
        scope.has(MigrationScope::TRANSFORMED),
    ] {
        if flag {
            n *= pow(2, sizes.src_vertices);
        }
    }
    n
}

/// The alphabets of [`graphs`].
#[derive(Clone, Copy, Debug)]
pub struct GraphScope<'a> {
    /// Node anchors.
    pub anchors: &'a [&'a str],
    /// Node values (a node holds one or none).
    pub values: &'a [Value],
    /// Node shapes (the hyper-edge shape makes fan nodes).
    pub shapes: &'a [NodeShape],
    /// Edge labels (reserved kinds make fan edges).
    pub labels: &'a [SEdge],
    /// Edge positions.
    pub positions: &'a [Option<u32>],
    /// The most nodes.
    pub max_nodes: usize,
    /// The most edges.
    pub max_edges: usize,
}

/// Every labelled graph of the scope.
///
/// Nodes `0 .. n-1` (n at most `max_nodes`), each anchored at one of
/// `anchors`, holding no value or one of `values`, and of one of `shapes`;
/// edges `0 .. m-1` (m at most `max_edges`), each with any endpoints, any
/// label of `labels` and any position of `positions`: parallel edges,
/// self-loops, two-cycles and fans (fan nodes with their parent and child
/// edges, well formed or not, when the scope holds the hyper-edge shape and
/// the reserved kinds) included.
#[must_use]
pub fn graphs(scope: &GraphScope<'_>) -> Vec<Graph> {
    let (anchor_count, value_count, shape_count) = (
        scope.anchors.len(),
        scope.values.len() + 1,
        scope.shapes.len(),
    );
    let (label_count, position_count) = (scope.labels.len(), scope.positions.len());
    let mut out = Vec::new();
    for nodes in 0..=scope.max_nodes {
        for nc in mixed(&vec![anchor_count * value_count * shape_count; nodes]) {
            for edges in 0..=scope.max_edges {
                for ec in mixed(&vec![nodes * nodes * label_count * position_count; edges]) {
                    let mut graph = Graph::default();
                    for (index, choice) in nc.iter().enumerate() {
                        let id = u32::try_from(index).unwrap_or(u32::MAX);
                        let (anchor, rest) = (
                            choice / (value_count * shape_count),
                            choice % (value_count * shape_count),
                        );
                        let (value, shape) = (rest / shape_count, rest % shape_count);
                        let mut node = Node::new(id, scope.anchors[anchor]);
                        if value > 0 {
                            node = node.with_value(scope.values[value - 1].clone());
                        }
                        node.shape = scope.shapes[shape].clone();
                        graph.nodes.insert(id, node);
                    }
                    for (index, choice) in ec.iter().enumerate() {
                        let (src, rest) = (
                            choice / (nodes * label_count * position_count),
                            choice % (nodes * label_count * position_count),
                        );
                        let (tgt, rest) = (
                            rest / (label_count * position_count),
                            rest % (label_count * position_count),
                        );
                        let (label, position) = (rest / position_count, rest % position_count);
                        let edge = GEdge {
                            src: u32::try_from(src).unwrap_or(u32::MAX),
                            tgt: u32::try_from(tgt).unwrap_or(u32::MAX),
                            label: scope.labels[label].clone(),
                            position: scope.positions[position],
                        };
                        graph
                            .edges
                            .insert(u32::try_from(index).unwrap_or(u32::MAX), edge);
                    }
                    out.push(graph);
                }
            }
        }
    }
    out
}

/// The count of [`graphs`]: `sum_{n <= N} (a (v + 1) s)^n sum_{m <= M}
/// (n^2 l p)^m`, for a anchors, v values, s shapes, l labels and p
/// positions.
#[must_use]
pub fn graphs_count(
    anchors: u64,
    values: u64,
    shapes: u64,
    labels: u64,
    positions: u64,
    max_nodes: u32,
    max_edges: u32,
) -> u64 {
    (0..=max_nodes)
        .map(|n| {
            (anchors * (values + 1) * shapes).pow(n)
                * (0..=max_edges)
                    .map(|m| (u64::from(n) * u64::from(n) * labels * positions).pow(m))
                    .sum::<u64>()
        })
        .sum()
}

/// Every labelled table instance of the scope.
///
/// Rows `0 .. r-1` (r at most `max_rows`), each at one of `tables` with a cell from `values` at
/// each of its table's `columns`, and every set of pairs of `relations` between rows of the
/// relation's endpoint tables.
#[must_use]
pub fn tables(
    tables: &[&str],
    columns: &BTreeMap<&str, Vec<&str>>,
    values: &[Value],
    relations: &[SEdge],
    max_rows: usize,
) -> Vec<Table> {
    let mut out = Vec::new();
    for r in 0..=max_rows {
        for tc in functions(r, tables.len()) {
            let cell_slots: Vec<(usize, &str)> = tc
                .iter()
                .enumerate()
                .flat_map(|(i, t)| {
                    columns
                        .get(tables[*t])
                        .into_iter()
                        .flatten()
                        .map(move |c| (i, *c))
                })
                .collect();
            for cells in functions(cell_slots.len(), values.len()) {
                let mut base = Table::default();
                for (i, t) in tc.iter().enumerate() {
                    base.rows.insert(
                        u32::try_from(i).unwrap_or(0),
                        (tables[*t].to_owned(), BTreeMap::new()),
                    );
                }
                for ((i, c), v) in cell_slots.iter().zip(&cells) {
                    if let Some(row) = base.rows.get_mut(&u32::try_from(*i).unwrap_or(0)) {
                        row.1.insert((*c).to_owned(), values[*v].clone());
                    }
                }
                let possible: Vec<(SEdge, u32, u32)> = relations
                    .iter()
                    .flat_map(|e| {
                        let rows = &base.rows;
                        rows.iter().filter(move |(_, (t, _))| *t == e.src).flat_map(
                            move |(s, _)| {
                                rows.iter()
                                    .filter(move |(_, (t, _))| *t == e.tgt)
                                    .map(move |(d, _)| (e.clone(), *s, *d))
                            },
                        )
                    })
                    .collect();
                for ps in subsets(&possible, possible.len()) {
                    let mut t = base.clone();
                    t.pairs = ps.into_iter().collect::<BTreeSet<_>>();
                    out.push(t);
                }
            }
        }
    }
    out
}

/// The count of [`tables`] by a separate product: over every row count
/// and table assignment, `|values|^(cells) * 2^(possible pairs)`.
#[must_use]
pub fn tables_count(
    tables: &[&str],
    columns: &BTreeMap<&str, Vec<&str>>,
    values: u64,
    relations: &[SEdge],
    max_rows: usize,
) -> u64 {
    fn row_count_vectors(
        remaining: usize,
        bins: usize,
        prefix: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if bins == 0 {
            if remaining == 0 {
                out.push(prefix.clone());
            }
            return;
        }
        for count in 0..=remaining {
            prefix.push(count);
            row_count_vectors(remaining - count, bins - 1, prefix, out);
            prefix.pop();
        }
    }

    const fn factorial(n: usize) -> u64 {
        let mut result = 1u64;
        let mut factor = 2usize;
        while factor <= n {
            result *= factor as u64;
            factor += 1;
        }
        result
    }

    let mut total = 0u64;
    for r in 0..=max_rows {
        let mut row_counts = Vec::new();
        row_count_vectors(r, tables.len(), &mut Vec::new(), &mut row_counts);
        for counts in row_counts {
            let cells: u64 = counts
                .iter()
                .zip(tables)
                .map(|(count, table)| {
                    (*count as u64) * columns.get(table).map_or(0, |c| c.len() as u64)
                })
                .sum();
            let count = |table: &str| {
                tables
                    .iter()
                    .position(|candidate| *candidate == table)
                    .map_or(0, |index| counts[index] as u64)
            };
            let pairs: u64 = relations
                .iter()
                .map(|e| count(&e.src) * count(&e.tgt))
                .sum();
            let assignments = factorial(r)
                / counts
                    .iter()
                    .map(|count| factorial(*count))
                    .product::<u64>();
            total += assignments
                * values.pow(u32::try_from(cells).unwrap_or(0))
                * 2u64.pow(u32::try_from(pairs).unwrap_or(0));
        }
    }
    total
}

/// Every permutation of `0 .. n`, in lexicographic order.
fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for rest in permutations(n - 1) {
        for at in 0..=rest.len() {
            let mut p = rest.clone();
            p.insert(at, n - 1);
            out.push(p);
        }
    }
    out.sort();
    out
}

/// The alphabets of [`trees`].
#[derive(Clone, Copy, Debug)]
pub struct TreeScope<'a> {
    /// The number of nodes (node 0 is the root).
    pub nodes: usize,
    /// Node anchors.
    pub anchors: &'a [&'a str],
    /// Node values (a node holds one or none).
    pub values: &'a [Value],
    /// Arc names (an arc carries one or none).
    pub arc_names: &'a [&'a str],
    /// Hyper-edges with their labels, for fans.
    pub hyper_edges: &'a [(&'a str, &'a [&'a str])],
    /// The most fans.
    pub max_fans: usize,
}

/// Every labelled tree of the scope, rooted at node 0.
///
/// Every parent map that sends each node other than 0 to a node and
/// reaches 0 (a labelled tree rooted at 0); every order of its arcs (the
/// arc list's order is the child order, and decides the edge ids of the
/// graph form, so no order is pruned); every anchoring with or without a
/// value per node; every arc name or none per arc (an arc's label runs
/// from its parent's anchor to its child's, of kind `prop`); and every list
/// of at most `max_fans` fans, each with any parent node, any hyper-edge of
/// the scope and a partial map of its labels to nodes.
#[must_use]
pub fn trees(scope: &TreeScope<'_>) -> Vec<Tree> {
    let n = scope.nodes;
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let mut fan_choices: Vec<crate::model::Fan> = Vec::new();
    for parent in 0..n {
        for (h, labels) in scope.hyper_edges {
            for map in functions(labels.len(), n + 1) {
                fan_choices.push(crate::model::Fan {
                    parent: u32::try_from(parent).unwrap_or(u32::MAX),
                    hyper_edge: (*h).to_owned(),
                    children: labels
                        .iter()
                        .zip(&map)
                        .filter(|(_, c)| **c > 0)
                        .map(|(l, c)| ((*l).to_owned(), u32::try_from(c - 1).unwrap_or(u32::MAX)))
                        .collect(),
                });
            }
        }
    }
    let fan_lists = sequences(&fan_choices, scope.max_fans);
    let id = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let v = scope.values.len() + 1;
    for parents in functions(n - 1, n) {
        let parent = |c: usize| parents[c - 1];
        let reaches = (1..n).all(|c| {
            let mut x = c;
            for _ in 0..n {
                if x == 0 {
                    return true;
                }
                x = parent(x);
            }
            false
        });
        if !reaches {
            continue;
        }
        for order in permutations(n - 1) {
            for nc in mixed(&vec![scope.anchors.len() * v; n]) {
                let anchor = |i: usize| scope.anchors[nc[i] / v];
                let nodes: BTreeMap<u32, Node> = (0..n)
                    .map(|i| {
                        let mut node = Node::new(id(i), anchor(i));
                        if nc[i] % v > 0 {
                            node = node.with_value(scope.values[nc[i] % v - 1].clone());
                        }
                        (id(i), node)
                    })
                    .collect();
                for names in mixed(&vec![scope.arc_names.len() + 1; n - 1]) {
                    let arcs: Vec<(u32, u32, SEdge)> = order
                        .iter()
                        .map(|k| {
                            let c = k + 1;
                            let name = (names[*k] > 0).then(|| scope.arc_names[names[*k] - 1]);
                            (
                                id(parent(c)),
                                id(c),
                                SEdge::new(anchor(parent(c)), anchor(c), "prop", name),
                            )
                        })
                        .collect();
                    for fans in &fan_lists {
                        out.push(Tree {
                            nodes: nodes.clone(),
                            arcs: arcs.clone(),
                            fans: fans.clone(),
                            root: 0,
                        });
                    }
                }
            }
        }
    }
    out
}

/// The count of [`trees`], by a closed formula.
///
/// Cayley's `n^(n-2)` labelled trees (OEIS A000272; one for n = 1), each
/// rooted at node 0 by one parent map, times `(n-1)!` arc orders, times
/// `(a (v + 1))^n` anchorings with values, times `(k + 1)^(n-1)` arc
/// namings, times `sum_{j <= F} (n * sum_h (n + 1)^{|L_h|})^j` fan lists,
/// for a anchors, v values, k arc names and hyper-edges h with label sets
/// `L_h`.
#[must_use]
pub fn trees_count(
    n: u32,
    anchors: u64,
    values: u64,
    arc_names: u64,
    hyper_label_counts: &[u32],
    max_fans: u32,
) -> u64 {
    if n == 0 {
        return 0;
    }
    let shapes = if n == 1 { 1 } else { u64::from(n).pow(n - 2) };
    let orders: u64 = (1..u64::from(n)).product();
    let per_fan: u64 = u64::from(n)
        * hyper_label_counts
            .iter()
            .map(|l| (u64::from(n) + 1).pow(*l))
            .sum::<u64>();
    let fan_lists: u64 = (0..=max_fans).map(|j| per_fan.pow(j)).sum();
    shapes * orders * (anchors * (values + 1)).pow(n) * (arc_names + 1).pow(n - 1) * fan_lists
}

/// Every numeral `[-]<s>e<x>` with significand `0 <= s < 10^digits` and
/// exponent `lo <= x <= hi`.
#[must_use]
pub fn numerals(digits: u32, lo: i64, hi: i64) -> Vec<String> {
    let top = 10u64.pow(digits);
    let mut out = Vec::new();
    for sign in ["", "-"] {
        for s in 0..top {
            for x in lo..=hi {
                out.push(format!("{sign}{s}e{x}"));
            }
        }
    }
    out
}

/// The count of [`numerals`]: `2 * 10^digits * (hi - lo + 1)`.
#[must_use]
pub fn numerals_count(digits: u32, lo: i64, hi: i64) -> u64 {
    2 * 10u64.pow(digits) * u64::try_from(hi - lo + 1).unwrap_or(0)
}

/// The boundary bit patterns of a format: either sign, each exponent field
/// of `exponents` (every field when `None`), and each fraction of
/// `fractions` (masked to the fraction width).
#[must_use]
pub fn boundary_bits(
    format: crate::float::Format,
    exponents: Option<&[u64]>,
    fractions: &[u64],
) -> Vec<u64> {
    let p = format.precision() - 1;
    let w = format.exponent_bits();
    let mask = (1u64 << p) - 1;
    let fr: BTreeSet<u64> = fractions.iter().map(|f| f & mask).collect();
    let all: Vec<u64> = (0..(1u64 << w)).collect();
    let es: BTreeSet<u64> = exponents
        .unwrap_or(&all)
        .iter()
        .map(|e| e & ((1u64 << w) - 1))
        .collect();
    let mut out = Vec::new();
    for sign in 0..2u64 {
        for e in &es {
            let e = *e;
            for f in &fr {
                // These masked fields are disjoint, so arithmetic assembly
                // states the unique positional representation directly.
                out.push((sign << (format.width() - 1)) + (e << p) + f);
            }
        }
    }
    out
}

/// The binary64 fraction set of the narrowing rules: each boundary of the
/// low 29 bits and of the quiet bit, and their neighbours.
#[must_use]
pub fn binary64_fractions() -> Vec<u64> {
    let t = |k: u32| 1u64 << k;
    vec![
        0,
        1,
        t(28) - 1,
        t(28),
        t(28) + 1,
        t(29) - 1,
        t(29),
        t(52) - t(29),
        t(52) - t(28) - 1,
        t(52) - t(28),
        t(52) - 1,
        t(51) - t(29),
        t(51),
        t(51) + t(29),
    ]
}

/// The binary32 fraction set of the boundary patterns: each end of the
/// 23-bit fraction field, its middle (the quiet bit of a NaN) and their
/// neighbours.
#[must_use]
pub fn binary32_fractions() -> Vec<u64> {
    let t = |k: u32| 1u64 << k;
    vec![0, 1, 2, t(22) - 1, t(22), t(22) + 1, t(23) - 2, t(23) - 1]
}

/// The alphabets of [`graph_deltas`].
#[derive(Clone, Copy, Debug)]
pub struct DeltaScope<'a> {
    /// Node ids.
    pub nodes: &'a [u32],
    /// Node anchors.
    pub anchors: &'a [&'a str],
    /// Values (a node or field holds one or none).
    pub values: &'a [Value],
    /// Field names.
    pub fields: &'a [&'a str],
    /// Edge ids.
    pub edges: &'a [u32],
    /// Edge labels.
    pub labels: &'a [SEdge],
    /// Edge positions.
    pub positions: &'a [Option<u32>],
}

/// Every atomic graph delta of the scope, and the identity.
///
/// The identity; adding, and deleting, each node id with each anchor and
/// no value or one value; adding, and deleting, each edge id with any
/// endpoints of the scope's nodes, any label and no position; and each
/// single attribute change from one value to another: a node's anchor, a
/// node's value presence (absent or one value), a node's field (absent or
/// one value), a node's shape between the plain and the hyper-edge shape,
/// an edge's label, and an edge's position.
#[must_use]
pub fn graph_deltas(scope: &DeltaScope<'_>) -> Vec<GraphDelta> {
    let mut out = vec![GraphDelta::identity()];
    let presences: Vec<Option<Presence>> = std::iter::once(None)
        .chain(
            scope
                .values
                .iter()
                .map(|v| Some(Presence::Present(v.clone()))),
        )
        .collect();
    let mut nodes = Vec::new();
    for n in scope.nodes {
        for a in scope.anchors {
            for p in &presences {
                let mut node = Node::new(*n, a);
                node.value.clone_from(p);
                nodes.push((*n, node));
            }
        }
    }
    for (n, node) in &nodes {
        let mut add = GraphDelta::identity();
        add.add
            .insert(GElem::Node(*n), GPayload::Node(node.clone()));
        out.push(add.invert());
        out.push(add);
    }
    for e in scope.edges {
        for s in scope.nodes {
            for t in scope.nodes {
                for l in scope.labels {
                    let mut add = GraphDelta::identity();
                    let edge = GEdge {
                        src: *s,
                        tgt: *t,
                        label: l.clone(),
                        position: None,
                    };
                    add.add.insert(GElem::Edge(*e), GPayload::Edge(edge));
                    out.push(add.invert());
                    out.push(add);
                }
            }
        }
    }
    let changes = |key: AttrKey, vals: &[AttrVal], out: &mut Vec<GraphDelta>| {
        for x in vals {
            for y in vals {
                if x != y {
                    let mut d = GraphDelta::identity();
                    d.attrs.insert(key.clone(), (x.clone(), y.clone()));
                    out.push(d);
                }
            }
        }
    };
    let field_vals: Vec<AttrVal> = std::iter::once(AttrVal::Field(None))
        .chain(scope.values.iter().map(|v| AttrVal::Field(Some(v.clone()))))
        .collect();
    let metas: Vec<AttrVal> = [NodeShape::Plain, NodeShape::HyperEdge]
        .into_iter()
        .map(|shape| {
            AttrVal::Meta(NodeMeta {
                discriminator: None,
                position: None,
                shape,
                annotations: BTreeMap::new(),
            })
        })
        .collect();
    for n in scope.nodes {
        let anchors: Vec<AttrVal> = scope
            .anchors
            .iter()
            .map(|a| AttrVal::Anchor((*a).to_owned()))
            .collect();
        changes(AttrKey::Anchor(*n), &anchors, &mut out);
        let pres: Vec<AttrVal> = presences.iter().cloned().map(AttrVal::Presence).collect();
        changes(AttrKey::Value(*n), &pres, &mut out);
        for f in scope.fields {
            changes(AttrKey::Field(*n, (*f).to_owned()), &field_vals, &mut out);
        }
        changes(AttrKey::Meta(*n), &metas, &mut out);
    }
    for e in scope.edges {
        let labels: Vec<AttrVal> = scope.labels.iter().cloned().map(AttrVal::Label).collect();
        changes(AttrKey::EdgeLabel(*e), &labels, &mut out);
        let positions: Vec<AttrVal> = scope
            .positions
            .iter()
            .copied()
            .map(AttrVal::Position)
            .collect();
        changes(AttrKey::EdgePosition(*e), &positions, &mut out);
    }
    out
}

/// The count of [`graph_deltas`], by a closed formula.
///
/// With N nodes, A anchors, V values, F fields, E edge ids, L labels and P
/// positions, the formula is:
///
/// ```text
/// 1 + 2 N A (V + 1) + 2 E N^2 L + N (A (A - 1) + (V + 1) V
/// + F (V + 1) V + 2) + E (L (L - 1) + P (P - 1))
/// ```
#[must_use]
pub const fn graph_deltas_count(
    nodes: u64,
    anchors: u64,
    values: u64,
    fields: u64,
    edges: u64,
    labels: u64,
    positions: u64,
) -> u64 {
    let pres = values + 1;
    1 + 2 * nodes * anchors * pres
        + 2 * edges * nodes * nodes * labels
        + nodes * (anchors * anchors.saturating_sub(1) + pres * values + fields * pres * values + 2)
        + edges * (labels * labels.saturating_sub(1) + positions * positions.saturating_sub(1))
}

/// The temporal lexemes of the order's exhaustive scope.
///
/// Every shape over years {-1, 0, 1, 2000}, months {1, 2, 12}, days {1, 28, 29, 31} (impossible dates such as
/// `0001-02-31` included, for the reader to refuse),
/// hours {0, 23}, minutes {0, 59}, seconds {0, 59, 60}, fractions {none, .0, .5}, and offsets
/// {local, Z, z, +00:00, -00:00, +04, +0400, +04:00, -14:00}.
#[must_use]
pub fn temporal_lexemes() -> Vec<String> {
    let years = ["-0001", "0000", "0001", "2000"];
    let months = ["01", "02", "12"];
    let days = ["01", "28", "29", "31"];
    let hours = ["00", "23"];
    let minutes = ["00", "59"];
    let seconds = ["00", "59", "60"];
    let fracs = ["", ".0", ".5"];
    let offsets = [
        "", "Z", "z", "+00:00", "-00:00", "+04", "+0400", "+04:00", "-14:00",
    ];
    let mut dates = Vec::new();
    for y in years {
        dates.push(y.to_owned());
        for m in months {
            dates.push(format!("{y}-{m}"));
            for d in days {
                dates.push(format!("{y}-{m}-{d}"));
            }
        }
    }
    let mut times = Vec::new();
    for h in hours {
        times.push(h.to_owned());
        for mi in minutes {
            times.push(format!("{h}:{mi}"));
            for s in seconds {
                for f in fracs {
                    times.push(format!("{h}:{mi}:{s}{f}"));
                }
            }
        }
    }
    let mut out = Vec::new();
    for o in offsets {
        out.extend(dates.iter().map(|d| format!("{d}{o}")));
        out.extend(times.iter().map(|t| format!("{t}{o}")));
        for d in dates.iter().filter(|d| d.len() >= 10) {
            out.extend(times.iter().map(|t| format!("{d}T{t}{o}")));
        }
    }
    out
}

/// The count of [`temporal_lexemes`]: per offset, the dates
/// `4 (1 + 3 (1 + 4))`, the times `2 (1 + 2 (1 + 3 * 3))` and the
/// date-times (full dates `4 * 3 * 4` times the times), over 9 offsets.
#[must_use]
pub const fn temporal_lexemes_count() -> u64 {
    let dates = 4 * (1 + 3 * (1 + 4));
    let times = 2 * (1 + 2 * (1 + 3 * 3));
    9 * (dates + times + 4 * 3 * 4 * times)
}

/// Iterator over every byte string of length at most a bound, shortest
/// first and lexicographically within a length.
#[derive(Clone, Debug)]
pub struct ByteStrings {
    next: u64,
    end: u64,
}

impl Iterator for ByteStrings {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.end {
            return None;
        }
        let mut ordinal = self.next;
        self.next += 1;
        let mut length = 0u32;
        let mut block = 1u64;
        while ordinal >= block {
            ordinal -= block;
            length += 1;
            block = block.checked_mul(256)?;
        }
        let mut bytes = vec![0; usize::try_from(length).ok()?];
        for byte in bytes.iter_mut().rev() {
            *byte = u8::try_from(ordinal % 256).ok()?;
            ordinal /= 256;
        }
        Some(bytes)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::try_from(self.end - self.next).unwrap_or(usize::MAX);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ByteStrings {}

/// Count every byte string of length at most `max_len`.
#[must_use]
pub fn byte_strings_count(max_len: u8) -> Option<u64> {
    (0..=u32::from(max_len)).try_fold(0u64, |total, length| {
        total.checked_add(256u64.checked_pow(length)?)
    })
}

/// Enumerate every byte string of length at most `max_len`.
#[must_use]
pub fn byte_strings(max_len: u8) -> Option<ByteStrings> {
    // Derive the generator's terminal ordinal directly from its blocks.
    // `byte_strings_count` is a separate oracle and is not called here.
    let mut block = 1u64;
    let mut end = 1u64;
    for _ in 0..max_len {
        block = block.checked_mul(256)?;
        end = end.checked_add(block)?;
    }
    Some(ByteStrings { next: 0, end })
}
