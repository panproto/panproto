//! Exhaustive small-scope enumerators of labelled structures.
//!
//! Every enumerator lists labelled structures (ids, positions and orders
//! are part of the structure) with no pruning of isomorphic copies, and has
//! beside it a closed formula for its count, computed independently of the
//! enumeration, so a test can check the enumeration's size against it.

use std::collections::{BTreeMap, BTreeSet};

use crate::migration::Migration;
use crate::model::{GEdge, Graph, Node, SEdge, Table, Tree};
use crate::schema::Schema;
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

/// Every vertex-level migration from `src` to `tgt`: each source vertex
/// sent to a target vertex or dropped, the images surviving and every
/// renamed vertex in `vertex_remap`.
#[must_use]
pub fn migrations(src: &Schema, tgt: &Schema) -> Vec<Migration> {
    let sv: Vec<&String> = src.vertices.keys().collect();
    let tv: Vec<&String> = tgt.vertices.keys().collect();
    functions(sv.len(), tv.len() + 1)
        .into_iter()
        .map(|f| {
            let mut m = Migration::default();
            for (v, c) in sv.iter().zip(f) {
                if c < tv.len() {
                    m.surviving_verts.insert(tv[c].clone());
                    if tv[c] != *v {
                        m.vertex_remap.insert((*v).clone(), tv[c].clone());
                    }
                }
            }
            m
        })
        .collect()
}

/// The count of [`migrations`]: `(|Vt| + 1)^|Vs|`.
#[must_use]
pub fn migrations_count(src_vertices: u64, tgt_vertices: u64) -> u64 {
    (tgt_vertices + 1).pow(u32::try_from(src_vertices).unwrap_or(0))
}

/// Every labelled graph of the scope.
///
/// Nodes `0 .. n-1` (n at most `max_nodes`), each anchored at one of `anchors` and optionally
/// holding a value from `values`, and edges `0 .. m-1` (m at most `max_edges`), each with any
/// endpoints and any label of `labels`: parallel edges, self-loops and two-cycles included.
#[must_use]
pub fn graphs(
    anchors: &[&str],
    values: &[Value],
    labels: &[SEdge],
    max_nodes: usize,
    max_edges: usize,
) -> Vec<Graph> {
    let mut out = Vec::new();
    for nodes in 0..=max_nodes {
        let node_choices = functions(nodes, anchors.len() * (values.len() + 1));
        let edge_kinds = nodes * nodes * labels.len();
        for nc in &node_choices {
            for edges in 0..=max_edges {
                for ec in functions(edges, edge_kinds) {
                    let mut graph = Graph::default();
                    for (index_n, choice) in nc.iter().enumerate() {
                        let id = u32::try_from(index_n).unwrap_or(0);
                        let mut node = Node::new(id, anchors[choice / (values.len() + 1)]);
                        let value = choice % (values.len() + 1);
                        if value > 0 {
                            node = node.with_value(values[value - 1].clone());
                        }
                        graph.nodes.insert(id, node);
                    }
                    for (index, choice) in ec.iter().enumerate() {
                        let (src, rest) = (
                            choice / (nodes * labels.len()),
                            choice % (nodes * labels.len()),
                        );
                        let (tgt, label) = (rest / labels.len(), rest % labels.len());
                        let edge = GEdge {
                            src: u32::try_from(src).unwrap_or(0),
                            tgt: u32::try_from(tgt).unwrap_or(0),
                            label: labels[label].clone(),
                            position: None,
                        };
                        graph.edges.insert(u32::try_from(index).unwrap_or(0), edge);
                    }
                    out.push(graph);
                }
            }
        }
    }
    out
}

/// The count of [`graphs`]: `sum_{n <= N} (a (v + 1))^n sum_{m <= M}
/// (n^2 l)^m`.
#[must_use]
pub fn graphs_count(anchors: u64, values: u64, labels: u64, max_nodes: u32, max_edges: u32) -> u64 {
    (0..=max_nodes)
        .map(|n| {
            (anchors * (values + 1)).pow(n)
                * (0..=max_edges)
                    .map(|m| (u64::from(n) * u64::from(n) * labels).pow(m))
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
    let mut total = 0u64;
    for r in 0..=max_rows {
        for tc in functions(r, tables.len()) {
            let cells: u64 = tc
                .iter()
                .map(|t| columns.get(tables[*t]).map_or(0, |c| c.len() as u64))
                .sum();
            let count = |t: &str| tc.iter().filter(|x| tables[**x] == t).count() as u64;
            let pairs: u64 = relations
                .iter()
                .map(|e| count(&e.src) * count(&e.tgt))
                .sum();
            total += values.pow(u32::try_from(cells).unwrap_or(0))
                * 2u64.pow(u32::try_from(pairs).unwrap_or(0));
        }
    }
    total
}

/// Every labelled tree on `n` nodes rooted at node 0.
///
/// Every parent map that sends each other node to a node and reaches the root, arcs in ascending
/// child order with the label from the parent's anchor to the child's, and every anchoring from
/// `anchors`.
#[must_use]
pub fn trees(n: usize, anchors: &[&str]) -> Vec<Tree> {
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
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
            x == 0
        });
        if !reaches || (1..n).any(|c| parent(c) == c) {
            continue;
        }
        for anch in functions(n, anchors.len()) {
            let nodes: BTreeMap<u32, Node> = (0..n)
                .map(|i| {
                    (
                        u32::try_from(i).unwrap_or(0),
                        Node::new(u32::try_from(i).unwrap_or(0), anchors[anch[i]]),
                    )
                })
                .collect();
            let arcs = (1..n)
                .map(|c| {
                    let p = parent(c);
                    (
                        u32::try_from(p).unwrap_or(0),
                        u32::try_from(c).unwrap_or(0),
                        SEdge::new(anchors[anch[p]], anchors[anch[c]], "prop", None),
                    )
                })
                .collect();
            out.push(Tree {
                nodes,
                arcs,
                fans: vec![],
                root: 0,
            });
        }
    }
    out
}

/// The count of [`trees`]: Cayley's `n^(n-2)` labelled trees rooted at a
/// fixed node, times `a^n` anchorings (one tree for `n = 1`).
#[must_use]
pub fn trees_count(n: u32, anchors: u64) -> u64 {
    let shapes = if n <= 1 {
        u64::from(n)
    } else {
        u64::from(n).pow(n - 2)
    };
    shapes * anchors.pow(n)
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
                out.push((sign << (format.width() - 1)) | (e << p) | f);
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
