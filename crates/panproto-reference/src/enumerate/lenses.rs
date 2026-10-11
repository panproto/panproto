//! Complete finite scopes for the lens-law page.

use std::collections::BTreeMap;

use crate::migration::Migration;
use crate::model::{GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::path::{FusionCondition, FusionInputs, FusionSemantics};
use crate::schema::Schema;
use crate::value::Value;

/// Every two-object schema with at most one labelled edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every vertex-only migration between the first and last bounded schemas.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    let schemas = schemas();
    super::migrations(
        &schemas[0],
        &schemas[schemas.len() - 1],
        super::MigrationScope::VERTICES,
    )
}

/// Labelled graphs with at most two nodes and one edge.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 2,
        max_edges: 1,
    })
}

/// Labelled one-table instances with at most one row.
#[must_use]
pub fn tables() -> Vec<Table> {
    super::tables(
        &["t"],
        &BTreeMap::from([("t", vec!["c"])]),
        &[Value::Int(0)],
        &[],
        1,
    )
}

/// Labelled two-node rooted trees with no fans.
#[must_use]
pub fn trees() -> Vec<Tree> {
    super::trees(&super::TreeScope {
        nodes: 2,
        anchors: &["a"],
        values: &[],
        arc_names: &["e"],
        hyper_edges: &[],
        max_fans: 0,
    })
}

/// Number of source states in the finite partial-lens carrier.
pub const PARTIAL_LENS_SOURCES: usize = 2;
/// Number of view states in the finite partial-lens carrier.
pub const PARTIAL_LENS_VIEWS: usize = 2;
/// Number of complement states in the finite partial-lens carrier.
pub const PARTIAL_LENS_COMPLEMENTS: usize = 3;
/// Exact alphabet of one partial-put cell: undefined, source zero, source one.
pub const PARTIAL_PUT_ALPHABET: [Option<usize>; 3] = [None, Some(0), Some(1)];

/// One finite partial lens used by the C11 law census.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialLens {
    get: [(usize, usize); PARTIAL_LENS_SOURCES],
    put: [Option<usize>; PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS],
}

impl PartialLens {
    const fn cell(view: usize, complement: usize) -> usize {
        view * PARTIAL_LENS_COMPLEMENTS + complement
    }

    /// The ordered injective total get table.
    #[must_use]
    pub const fn get_table(&self) -> &[(usize, usize); PARTIAL_LENS_SOURCES] {
        &self.get
    }

    /// The partial put table in view-major, then complement-major order.
    #[must_use]
    pub const fn put_table(
        &self,
    ) -> &[Option<usize>; PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS] {
        &self.put
    }

    /// Read one partial put cell.
    #[must_use]
    pub const fn put(&self, view: usize, complement: usize) -> Option<usize> {
        self.put[Self::cell(view, complement)]
    }

    /// Whether this finite lens satisfies `GetPut`.
    #[must_use]
    pub fn get_put(&self) -> bool {
        self.get
            .iter()
            .enumerate()
            .all(|(source, &(view, complement))| self.put(view, complement) == Some(source))
    }

    /// Whether this finite lens satisfies complement-form `WeakPutGet`.
    #[must_use]
    pub fn weak_put_get(&self) -> bool {
        (0..PARTIAL_LENS_VIEWS).all(|view| {
            (0..PARTIAL_LENS_COMPLEMENTS).all(|complement| {
                self.put(view, complement).is_none_or(|source| {
                    let (observed, _) = self.get[source];
                    self.put(observed, complement) == Some(source)
                })
            })
        })
    }

    /// Whether every defined put re-reads the supplied view exactly.
    #[must_use]
    pub fn exact_put_get(&self) -> bool {
        (0..PARTIAL_LENS_VIEWS).all(|view| {
            (0..PARTIAL_LENS_COMPLEMENTS).all(|complement| {
                self.put(view, complement)
                    .is_none_or(|source| self.get[source].0 == view)
            })
        })
    }

    /// Whether this finite lens satisfies raw `PutTwice`.
    #[must_use]
    pub fn put_twice(&self) -> bool {
        (0..PARTIAL_LENS_VIEWS).all(|view| {
            (0..PARTIAL_LENS_COMPLEMENTS).all(|complement| {
                self.put(view, complement).is_none_or(|source| {
                    let next_complement = self.get[source].1;
                    self.put(view, next_complement) == Some(source)
                })
            })
        })
    }
}

fn decode_partial_put(mut code: usize) -> [Option<usize>; 6] {
    let mut put = [None; 6];
    for cell in &mut put {
        *cell = PARTIAL_PUT_ALPHABET[code % PARTIAL_PUT_ALPHABET.len()];
        code /= PARTIAL_PUT_ALPHABET.len();
    }
    put
}

/// Stream all ordered-injective-get/partial-put lenses of the C11 carrier.
pub fn for_each_partial_lens(mut visit: impl FnMut(PartialLens)) {
    let get_cells = PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS;
    for first in 0..get_cells {
        for second in (0..get_cells).filter(|second| *second != first) {
            let get = [
                (
                    first / PARTIAL_LENS_COMPLEMENTS,
                    first % PARTIAL_LENS_COMPLEMENTS,
                ),
                (
                    second / PARTIAL_LENS_COMPLEMENTS,
                    second % PARTIAL_LENS_COMPLEMENTS,
                ),
            ];
            for code in 0..PARTIAL_PUT_ALPHABET.len().pow(6) {
                visit(PartialLens {
                    get,
                    put: decode_partial_put(code),
                });
            }
        }
    }
}

const fn power(mut base: usize, mut exponent: usize) -> usize {
    let mut result = 1;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result *= base;
        }
        exponent >>= 1;
        base *= base;
    }
    result
}

const fn falling_factorial(n: usize, k: usize) -> usize {
    let mut result = 1;
    let mut offset = 0;
    while offset < k {
        result *= n - offset;
        offset += 1;
    }
    result
}

fn bounded_id(value: usize) -> u32 {
    let Ok(id) = u32::try_from(value) else {
        unreachable!("the finite reference bounds fit in a u32 identifier");
    };
    id
}

/// Independent closed-form count `P(2*3, 2) * 3^(2*3)`.
#[must_use]
pub const fn partial_lens_count_certificate() -> usize {
    let get_cells = PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS;
    falling_factorial(get_cells, PARTIAL_LENS_SOURCES)
        * power(
            PARTIAL_PUT_ALPHABET.len(),
            PARTIAL_LENS_VIEWS * PARTIAL_LENS_COMPLEMENTS,
        )
}

/// Exact law histogram of the C11 partial-lens carrier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PartialLensHistogram {
    /// Every lens.
    pub total: usize,
    /// Lenses satisfying `GetPut`.
    pub get_put: usize,
    /// Lenses satisfying `WeakPutGet`.
    pub weak_put_get: usize,
    /// Lenses satisfying exact-view `PutGet`.
    pub exact_put_get: usize,
    /// Lenses satisfying raw `PutTwice`.
    pub put_twice: usize,
    /// Lenses satisfying both `GetPut` and `WeakPutGet`.
    pub get_put_and_weak_put_get: usize,
    /// `GetPut` and `WeakPutGet` lenses failing `PutTwice`.
    pub get_put_and_weak_put_get_without_put_twice: usize,
    /// `GetPut` and exact `PutGet` lenses failing `PutTwice`.
    pub get_put_and_exact_put_get_without_put_twice: usize,
}

fn coded_put(code: usize, view: usize, complement: usize) -> Option<usize> {
    match (code / power(3, view * 3 + complement)) % 3 {
        0 => None,
        source @ 1..=2 => Some(source - 1),
        _ => unreachable!("a base-three digit is in 0..=2"),
    }
}

/// Independently walk the integer code space and derive the exact law
/// histogram without invoking [`for_each_partial_lens`] or its table decoder.
#[must_use]
pub fn partial_lens_histogram() -> PartialLensHistogram {
    let mut histogram = PartialLensHistogram::default();
    for first in 0..6 {
        for second in (0..6).filter(|second| *second != first) {
            let get = [(first / 3, first % 3), (second / 3, second % 3)];
            for code in 0..power(3, 6) {
                let gp = get.iter().enumerate().all(|(source, &(view, complement))| {
                    coded_put(code, view, complement) == Some(source)
                });
                let weak = (0..2).all(|view| {
                    (0..3).all(|complement| {
                        coded_put(code, view, complement).is_none_or(|source| {
                            let (observed, _) = get[source];
                            coded_put(code, observed, complement) == Some(source)
                        })
                    })
                });
                let exact = (0..2).all(|view| {
                    (0..3).all(|complement| {
                        coded_put(code, view, complement).is_none_or(|source| get[source].0 == view)
                    })
                });
                let twice = (0..2).all(|view| {
                    (0..3).all(|complement| {
                        coded_put(code, view, complement).is_none_or(|source| {
                            coded_put(code, view, get[source].1) == Some(source)
                        })
                    })
                });
                histogram.total += 1;
                histogram.get_put += usize::from(gp);
                histogram.weak_put_get += usize::from(weak);
                histogram.exact_put_get += usize::from(exact);
                histogram.put_twice += usize::from(twice);
                histogram.get_put_and_weak_put_get += usize::from(gp && weak);
                histogram.get_put_and_weak_put_get_without_put_twice +=
                    usize::from(gp && weak && !twice);
                histogram.get_put_and_exact_put_get_without_put_twice +=
                    usize::from(gp && exact && !twice);
            }
        }
    }
    histogram
}

/// Largest column and canonical-row cardinalities of the C11 table carrier.
pub const TABLE_FIXTURE_BOUND: usize = 2;

/// Exact table-cell alphabet: absence, Null, F, T, Int(0), Int(1), Str("").
#[must_use]
pub const fn table_cell_alphabet() -> [Option<Value>; 7] {
    [
        None,
        Some(Value::Null),
        Some(Value::Bool(false)),
        Some(Value::Bool(true)),
        Some(Value::Int(0)),
        Some(Value::Int(1)),
        Some(Value::Str(String::new())),
    ]
}

/// One labelled table source paired with one projection lens, represented by
/// its retained-column bit mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableFixture {
    /// Number of source columns `c0..c{columns-1}`.
    pub columns: usize,
    /// Retained source-column bits.
    pub retained: usize,
    /// Canonical rows `0..rows`, all anchored at `row`.
    pub source: Table,
}

/// Stream every expressible table fixture in the exact C11 alphabet.
pub fn for_each_table_fixture(mut visit: impl FnMut(TableFixture)) {
    for columns in 0..=TABLE_FIXTURE_BOUND {
        for retained in 0..power(2, columns) {
            for rows in 0..=TABLE_FIXTURE_BOUND {
                let assignments = power(7, rows * columns);
                for mut code in 0..assignments {
                    let mut source = Table::default();
                    for row in 0..rows {
                        let mut cells = BTreeMap::new();
                        for column in 0..columns {
                            if let Some(value) = table_cell_alphabet()[code % 7].clone() {
                                cells.insert(format!("c{column}"), value);
                            }
                            code /= 7;
                        }
                        source
                            .rows
                            .insert(bounded_id(row), ("row".to_owned(), cells));
                    }
                    visit(TableFixture {
                        columns,
                        retained,
                        source,
                    });
                }
            }
        }
    }
}

/// Independent closed-form counts for every `(columns, rows)` cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableFixtureCounts {
    /// Source assignments indexed by `[columns][rows]`.
    pub sources_by_columns_and_rows: [[usize; 3]; 3],
    /// Projection lenses indexed by `columns`.
    pub lenses_by_columns: [usize; 3],
    /// Projection-lens/source cases indexed by `[columns][rows]`.
    pub by_columns_and_rows: [[usize; 3]; 3],
    /// Sums for zero, one and two columns.
    pub by_columns: [usize; 3],
    /// Sum of all nine cells.
    pub total: usize,
}

/// Derive table case counts from `2^columns * 7^(columns*rows)`, independently
/// of [`for_each_table_fixture`].
#[must_use]
pub const fn table_fixture_count_certificate() -> TableFixtureCounts {
    let mut sources = [[0; 3]; 3];
    let mut cases = [[0; 3]; 3];
    let mut lenses = [0; 3];
    let mut columns_totals = [0; 3];
    let mut total = 0;
    let mut columns = 0;
    while columns <= TABLE_FIXTURE_BOUND {
        lenses[columns] = power(2, columns);
        let mut rows = 0;
        while rows <= TABLE_FIXTURE_BOUND {
            let source_count = power(7, columns * rows);
            let count = lenses[columns] * source_count;
            sources[columns][rows] = source_count;
            cases[columns][rows] = count;
            columns_totals[columns] += count;
            total += count;
            rows += 1;
        }
        columns += 1;
    }
    TableFixtureCounts {
        sources_by_columns_and_rows: sources,
        lenses_by_columns: lenses,
        by_columns_and_rows: cases,
        by_columns: columns_totals,
        total,
    }
}

/// One of the two exact finite Delta-fragment fixtures of GF5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gf5Fixture {
    /// `a -> b`, with only `a` visible.
    DropTarget,
    /// `a -visible-> c -hidden-> b`, with `a`, `c` and `visible` visible.
    VisibleThenHidden,
}

impl Gf5Fixture {
    /// The source schema of this exact fixture.
    #[must_use]
    pub fn source_schema(self) -> Schema {
        match self {
            Self::DropTarget => Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .edge("a", "b", "rel", Some("b")),
            Self::VisibleThenHidden => Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .vertex("c", "record")
                .edge("a", "c", "rel", Some("visible"))
                .edge("c", "b", "rel", Some("hidden")),
        }
    }

    /// The target schema of this exact fixture.
    #[must_use]
    pub fn target_schema(self) -> Schema {
        match self {
            Self::DropTarget => Schema::default().vertex("a", "record"),
            Self::VisibleThenHidden => Schema::default()
                .vertex("a", "record")
                .vertex("c", "record")
                .edge("a", "c", "rel", Some("visible")),
        }
    }

    /// The monic deletion migration of this exact fixture.
    #[must_use]
    pub fn migration(self) -> Migration {
        let target = self.target_schema();
        let surviving_verts = target.vertices.keys().cloned().collect();
        Migration {
            surviving_verts,
            surviving_edges: target.edges,
            ..Migration::default()
        }
    }
}

#[derive(Clone, Copy)]
struct Gf5Edge {
    src: usize,
    tgt: usize,
    name: &'static str,
}

const DROP_TARGET_ANCHORS: [&str; 2] = ["a", "b"];
const DROP_TARGET_EDGES: [Gf5Edge; 1] = [Gf5Edge {
    src: 0,
    tgt: 1,
    name: "b",
}];
const VISIBLE_HIDDEN_ANCHORS: [&str; 3] = ["a", "b", "c"];
const VISIBLE_HIDDEN_EDGES: [Gf5Edge; 2] = [
    Gf5Edge {
        src: 0,
        tgt: 2,
        name: "visible",
    },
    Gf5Edge {
        src: 2,
        tgt: 1,
        name: "hidden",
    },
];

const fn gf5_carrier(fixture: Gf5Fixture) -> (&'static [&'static str], &'static [Gf5Edge]) {
    match fixture {
        Gf5Fixture::DropTarget => (&DROP_TARGET_ANCHORS, &DROP_TARGET_EDGES),
        Gf5Fixture::VisibleThenHidden => (&VISIBLE_HIDDEN_ANCHORS, &VISIBLE_HIDDEN_EDGES),
    }
}

fn gf5_keeps_anchor(fixture: Gf5Fixture, anchor: &str) -> bool {
    match fixture {
        Gf5Fixture::DropTarget => anchor == "a",
        Gf5Fixture::VisibleThenHidden => matches!(anchor, "a" | "c"),
    }
}

fn gf5_keeps_edge(fixture: Gf5Fixture, edge: &SEdge) -> bool {
    fixture == Gf5Fixture::VisibleThenHidden
        && *edge == SEdge::new("a", "c", "rel", Some("visible"))
}

fn for_each_gf5_source_of(fixture: Gf5Fixture, mut visit: impl FnMut(&Graph)) {
    let (anchors, edges) = gf5_carrier(fixture);
    for nodes in 0..=3 {
        for mut assignment in 0..power(anchors.len(), nodes) {
            let mut source = Graph::default();
            let mut node_anchors = Vec::with_capacity(nodes);
            for id in 0..nodes {
                let anchor = anchors[assignment % anchors.len()];
                assignment /= anchors.len();
                node_anchors.push(anchor);
                let id = bounded_id(id);
                source.nodes.insert(id, Node::new(id, anchor));
            }
            let mut candidates = Vec::new();
            for src in 0..nodes {
                for tgt in 0..nodes {
                    for (edge_index, edge) in edges.iter().enumerate() {
                        if anchors[edge.src] == node_anchors[src]
                            && anchors[edge.tgt] == node_anchors[tgt]
                        {
                            candidates.push((src, tgt, edge_index));
                        }
                    }
                }
            }
            for mask in 0..power(2, candidates.len()) {
                if mask.count_ones() > 3 {
                    continue;
                }
                let mut graph = source.clone();
                for (index, &(src, tgt, edge_index)) in candidates.iter().enumerate() {
                    if mask & (1 << index) == 0 {
                        continue;
                    }
                    let edge = edges[edge_index];
                    graph.edges.insert(
                        bounded_id(index),
                        GEdge {
                            src: bounded_id(src),
                            tgt: bounded_id(tgt),
                            label: SEdge::new(
                                anchors[edge.src],
                                anchors[edge.tgt],
                                "rel",
                                Some(edge.name),
                            ),
                            position: None,
                        },
                    );
                }
                visit(&graph);
            }
        }
    }
}

fn gf5_restrict(fixture: Gf5Fixture, source: &Graph) -> Graph {
    let mut view = Graph::default();
    for (id, node) in &source.nodes {
        if gf5_keeps_anchor(fixture, &node.anchor) {
            view.nodes.insert(*id, node.clone());
        }
    }
    for (id, edge) in &source.edges {
        if view.nodes.contains_key(&edge.src)
            && view.nodes.contains_key(&edge.tgt)
            && gf5_keeps_edge(fixture, &edge.label)
        {
            view.edges.insert(*id, edge.clone());
        }
    }
    view
}

fn for_each_subinstance(graph: &Graph, mut visit: impl FnMut(&Graph)) {
    let nodes: Vec<_> = graph.nodes.iter().collect();
    let edges: Vec<_> = graph.edges.iter().collect();
    for node_mask in 0..power(2, nodes.len()) {
        let mut base = Graph::default();
        for (index, (id, node)) in nodes.iter().enumerate() {
            if node_mask & (1 << index) != 0 {
                base.nodes.insert(**id, (*node).clone());
            }
        }
        let eligible: Vec<_> = edges
            .iter()
            .filter(|(_, edge)| {
                base.nodes.contains_key(&edge.src) && base.nodes.contains_key(&edge.tgt)
            })
            .collect();
        for edge_mask in 0..power(2, eligible.len()) {
            let mut subinstance = base.clone();
            for (index, (id, edge)) in eligible.iter().enumerate() {
                if edge_mask & (1 << index) != 0 {
                    subinstance.edges.insert(**id, (*edge).clone());
                }
            }
            visit(&subinstance);
        }
    }
}

/// Stream the exact 133 GF5 source graphs.
pub fn for_each_gf5_source(mut visit: impl FnMut(Gf5Fixture, &Graph)) {
    for fixture in [Gf5Fixture::DropTarget, Gf5Fixture::VisibleThenHidden] {
        for_each_gf5_source_of(fixture, |source| visit(fixture, source));
    }
}

/// Stream every source and deletion-only edited view of the GF5 carrier.
pub fn for_each_gf5_deletion_edit(mut visit: impl FnMut(Gf5Fixture, &Graph, &Graph)) {
    for_each_gf5_source(|fixture, source| {
        let view = gf5_restrict(fixture, source);
        for_each_subinstance(&view, |edited_view| visit(fixture, source, edited_view));
    });
}

/// Stream every GF5 source/deletion-edit/source-subinstance probe.
pub fn for_each_gf5_source_subinstance_probe(
    mut visit: impl FnMut(Gf5Fixture, &Graph, &Graph, &Graph),
) {
    for_each_gf5_deletion_edit(|fixture, source, edited_view| {
        for_each_subinstance(source, |candidate| {
            visit(fixture, source, edited_view, candidate);
        });
    });
}

/// Independent counts for the two exact GF5 fixtures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gf5Counts {
    /// Source graphs, by fixture order.
    pub sources: [usize; 2],
    /// Deletion-only edited views, by fixture order.
    pub deletion_edits: [usize; 2],
    /// Source-subinstance probes, by fixture order.
    pub source_subinstance_probes: [usize; 2],
}

const fn certificate_anchor(fixture: Gf5Fixture, word: usize, node: usize) -> usize {
    let alphabet = match fixture {
        Gf5Fixture::DropTarget => 2,
        Gf5Fixture::VisibleThenHidden => 3,
    };
    (word / power(alphabet, node)) % alphabet
}

fn certificate_edge_at(
    fixture: Gf5Fixture,
    word: usize,
    nodes: usize,
    wanted: usize,
) -> Option<(usize, usize, bool)> {
    let mut found = 0;
    for src in 0..nodes {
        for tgt in 0..nodes {
            let src_anchor = certificate_anchor(fixture, word, src);
            let tgt_anchor = certificate_anchor(fixture, word, tgt);
            let possibilities: &[(usize, usize, bool)] = match fixture {
                Gf5Fixture::DropTarget => &[(0, 1, false)],
                Gf5Fixture::VisibleThenHidden => &[(0, 2, true), (2, 1, false)],
            };
            for &(expected_src, expected_tgt, visible) in possibilities {
                if src_anchor == expected_src && tgt_anchor == expected_tgt {
                    if found == wanted {
                        return Some((src, tgt, visible));
                    }
                    found += 1;
                }
            }
        }
    }
    None
}

fn certificate_edge_count(fixture: Gf5Fixture, word: usize, nodes: usize) -> usize {
    let mut count = 0;
    while certificate_edge_at(fixture, word, nodes, count).is_some() {
        count += 1;
    }
    count
}

fn certificate_subinstance_count(
    fixture: Gf5Fixture,
    word: usize,
    nodes: usize,
    selected_edges: usize,
    allowed_nodes: usize,
    visible_edges_only: bool,
) -> usize {
    let edge_count = certificate_edge_count(fixture, word, nodes);
    let mut count = 0;
    for kept_nodes in 0..power(2, nodes) {
        if kept_nodes & !allowed_nodes != 0 {
            continue;
        }
        let eligible_edges = (0..edge_count)
            .filter(|edge| selected_edges & (1 << edge) != 0)
            .filter(|edge| {
                let Some((src, tgt, visible)) = certificate_edge_at(fixture, word, nodes, *edge)
                else {
                    return false;
                };
                (!visible_edges_only || visible)
                    && kept_nodes & (1 << src) != 0
                    && kept_nodes & (1 << tgt) != 0
            })
            .count();
        count += power(2, eligible_edges);
    }
    count
}

fn gf5_counts_for(fixture: Gf5Fixture) -> (usize, usize, usize) {
    let anchor_count = match fixture {
        Gf5Fixture::DropTarget => 2,
        Gf5Fixture::VisibleThenHidden => 3,
    };
    let mut sources = 0;
    let mut edits = 0;
    let mut probes = 0;
    for nodes in 0..=3 {
        for word in 0..power(anchor_count, nodes) {
            let edge_count = certificate_edge_count(fixture, word, nodes);
            for edge_mask in 0..power(2, edge_count) {
                if edge_mask.count_ones() > 3 {
                    continue;
                }
                sources += 1;
                let visible_nodes = (0..nodes).fold(0, |mask, node| {
                    let anchor = certificate_anchor(fixture, word, node);
                    let visible = match fixture {
                        Gf5Fixture::DropTarget => anchor == 0,
                        Gf5Fixture::VisibleThenHidden => matches!(anchor, 0 | 2),
                    };
                    if visible { mask | (1 << node) } else { mask }
                });
                let source_subinstances = certificate_subinstance_count(
                    fixture,
                    word,
                    nodes,
                    edge_mask,
                    power(2, nodes) - 1,
                    false,
                );
                let deletion_edits = certificate_subinstance_count(
                    fixture,
                    word,
                    nodes,
                    edge_mask,
                    visible_nodes,
                    true,
                );
                edits += deletion_edits;
                probes += deletion_edits * source_subinstances;
            }
        }
    }
    (sources, edits, probes)
}

/// Derive the GF5 counts from anchor assignments, edge-subset masks and the
/// endpoint-closed subinstance formula, without constructing a [`Graph`] or
/// invoking the streamed enumerators.
#[must_use]
pub fn gf5_count_certificate() -> Gf5Counts {
    let first = gf5_counts_for(Gf5Fixture::DropTarget);
    let second = gf5_counts_for(Gf5Fixture::VisibleThenHidden);
    Gf5Counts {
        sources: [first.0, second.0],
        deletion_edits: [first.1, second.1],
        source_subinstance_probes: [first.2, second.2],
    }
}

/// Number of endpoint states in the closed path-contract subcarrier.
///
/// This reuses the two-state source/view alphabet of [`PartialLens`]. It
/// does not select production graph or tree schemas.
pub const PATH_CONTRACT_ENDPOINTS: usize = PARTIAL_LENS_SOURCES;

/// Maximum word length in the closed path-contract subcarrier.
pub const PATH_CONTRACT_MAX_LENGTH: usize = 3;

/// Atomic behaviours in the closed two-state path-contract subcarrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathContractKind {
    /// Preserve the state.
    Identity,
    /// Exchange zero and one.
    Toggle,
    /// Return zero and retain the source state in the complement.
    Clear,
}

impl PathContractKind {
    /// Contract order of the three behaviours.
    pub const ALL: [Self; 3] = [Self::Identity, Self::Toggle, Self::Clear];

    const fn ordinal(self) -> usize {
        match self {
            Self::Identity => 0,
            Self::Toggle => 1,
            Self::Clear => 2,
        }
    }

    /// Execute get over the two-state carrier. Complement `2` is the
    /// complement-free marker already present in the three-state alphabet.
    #[must_use]
    pub const fn get(self, source: usize) -> (usize, usize) {
        match self {
            Self::Identity => (source, 2),
            Self::Toggle => (1 - source, 2),
            Self::Clear => (0, source),
        }
    }

    /// Execute put over the exact three-state complement alphabet.
    #[must_use]
    pub const fn put(self, view: usize, complement: usize) -> Option<usize> {
        if view >= PARTIAL_LENS_VIEWS || complement >= PARTIAL_LENS_COMPLEMENTS {
            return None;
        }
        match self {
            Self::Identity => {
                if complement == 2 {
                    Some(view)
                } else {
                    None
                }
            }
            Self::Toggle => {
                if complement == 2 {
                    Some(1 - view)
                } else {
                    None
                }
            }
            Self::Clear => {
                if complement < 2 {
                    Some(complement)
                } else {
                    None
                }
            }
        }
    }

    /// Exact composition when the result is represented by this closed
    /// atomic behaviour alphabet.
    ///
    /// # Errors
    ///
    /// `Clear` followed by `Toggle` is constant one, which is deliberately
    /// absent from the fixed alphabet. The sequence must therefore remain
    /// explicit.
    pub const fn fuse(self, next: Self) -> Result<Self, PathContractFusionObstruction> {
        match (self, next) {
            (Self::Identity, right) => Ok(right),
            (left, Self::Identity) => Ok(left),
            (Self::Toggle, Self::Toggle) => Ok(Self::Identity),
            (Self::Toggle | Self::Clear, Self::Clear) => Ok(Self::Clear),
            (Self::Clear, Self::Toggle) => {
                Err(PathContractFusionObstruction::ConstantOneOutsideAlphabet)
            }
        }
    }
}

/// Independently computed premise results for one proposed scalar fusion.
///
/// This is deliberately not a complete L.6 certificate. The scalar carrier
/// exercises migration/value compatibility nonvacuously; five structural
/// premises have empty alphabets and the context premise is supplied by the
/// caller. [`path_contract_fusion_coverage`] records that limitation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractFusionPremises {
    values: [bool; 8],
    context_applicable: bool,
}

impl PathContractFusionPremises {
    /// Result for the premise at its L.6 contract ordinal.
    ///
    /// Ordinals are contraction, migration, edge image, empty plans, value
    /// layer, resolver, keep/drop compilation, and context.
    #[must_use]
    pub const fn holds(&self, ordinal: usize) -> bool {
        self.values[ordinal]
    }

    /// Whether the boundary-context premise applies.
    #[must_use]
    pub const fn context_applicable(&self) -> bool {
        self.context_applicable
    }
}

/// Nonvacuity ledger for the scalar fusion-premise subcarrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractFusionCoverage {
    /// Independently evaluated observations by premise ordinal.
    pub observations: [usize; 8],
}

impl PathContractFusionCoverage {
    /// Whether a premise has at least one independently evaluated witness.
    #[must_use]
    pub const fn nonvacuous(&self, ordinal: usize) -> bool {
        self.observations[ordinal] != 0
    }
}

/// Evaluate a proposed fusion independently of [`PathContractKind::fuse`].
///
/// The operation check exhausts both source states and both possible edited
/// views. Structural premises evaluate true only because this explicitly
/// scalar carrier has no graph paths, edges, restrict plans, or fan resolvers;
/// they are not counted as witnesses for the full C11/L.6 claim.
#[must_use]
pub fn path_contract_fusion_premises(
    first: PathContractKind,
    second: PathContractKind,
    fused: PathContractKind,
    context_compiles: Option<bool>,
) -> PathContractFusionPremises {
    // These cardinalities are part of the scalar carrier definition above,
    // not conclusions drawn from the proposed fused atom.
    const CONTRACTED_PATHS: usize = 0;
    const EDGE_IMAGE_DISAGREEMENTS: usize = 0;
    const RESTRICT_PLAN_STEPS: usize = 0;
    const FAN_RESOLVERS: usize = 0;
    const NON_KEEP_DROP_FATES: usize = 0;

    let mut migrations_compose = true;
    let mut value_layers_compatible = true;
    for source in 0..PARTIAL_LENS_SOURCES {
        let (middle, first_complement) = first.get(source);
        let (sequential_view, second_complement) = second.get(middle);
        let (fused_view, fused_complement) = fused.get(source);
        migrations_compose &= sequential_view == fused_view;
        for edited_view in 0..PARTIAL_LENS_VIEWS {
            let sequential_put = second
                .put(edited_view, second_complement)
                .and_then(|middle_source| first.put(middle_source, first_complement));
            let fused_put = fused.put(edited_view, fused_complement);
            value_layers_compatible &= sequential_put == fused_put;
        }
    }
    PathContractFusionPremises {
        values: [
            CONTRACTED_PATHS == 0,
            migrations_compose,
            EDGE_IMAGE_DISAGREEMENTS == 0,
            RESTRICT_PLAN_STEPS == 0,
            value_layers_compatible,
            FAN_RESOLVERS == 0,
            NON_KEEP_DROP_FATES == 0,
            context_compiles.unwrap_or(true),
        ],
        context_applicable: context_compiles.is_some(),
    }
}

/// Exact nonvacuity ledger for [`path_contract_fusion_premises`].
///
/// Only migration composition and value-layer compatibility are exercised on
/// nonempty alphabets. Thus this subcarrier cannot discharge the full C11
/// fusion obligation by itself.
#[must_use]
pub const fn path_contract_fusion_coverage() -> PathContractFusionCoverage {
    PathContractFusionCoverage {
        observations: [
            0,
            PARTIAL_LENS_SOURCES,
            0,
            0,
            PARTIAL_LENS_SOURCES * PARTIAL_LENS_VIEWS,
            0,
            0,
            0,
        ],
    }
}

fn derive_path_contract_fusion(
    condition: FusionCondition,
    inputs: &FusionInputs<'_, PathContractKind>,
) -> bool {
    let context = inputs.context().map(|bytes| bytes == [1]);
    path_contract_fusion_premises(
        *inputs.left().stage().payload(),
        *inputs.right().stage().payload(),
        *inputs.payload(),
        context,
    )
    .holds(condition.ordinal())
}

/// Trusted bounded semantics used to derive scalar fusion evidence.
///
/// The returned value is opaque. Its constructor is unavailable outside the
/// crate, so a caller cannot substitute an all-true premise function.
#[must_use]
pub const fn path_contract_fusion_semantics() -> FusionSemantics<PathContractKind> {
    FusionSemantics::trusted(derive_path_contract_fusion)
}

/// Independent exact domain oracle for [`PathContractKind::put`].
#[must_use]
pub const fn path_contract_put_domain(
    kind: PathContractKind,
    view: usize,
    complement: usize,
) -> bool {
    if view >= PARTIAL_LENS_VIEWS || complement >= PARTIAL_LENS_COMPLEMENTS {
        return false;
    }
    match kind {
        PathContractKind::Identity | PathContractKind::Toggle => complement == 2,
        PathContractKind::Clear => complement < 2,
    }
}

/// Typed reason the closed contract algebra cannot fuse an atomic pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathContractFusionObstruction {
    /// `Clear ; Toggle` is constant one, not an atomic behaviour in this
    /// explicitly fixed subcarrier.
    ConstantOneOutsideAlphabet,
}

/// One typed atomic arrow in the path-contract subcarrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathContractStage {
    /// Source endpoint.
    pub source: usize,
    /// Target endpoint.
    pub target: usize,
    /// Two-state behaviour.
    pub kind: PathContractKind,
}

/// One composable word in the exact path-contract subcarrier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathContractWord {
    /// Source of the empty word or first stage.
    pub source: usize,
    /// Composable atomic stages.
    pub stages: Vec<PathContractStage>,
}

fn path_contract_byte(value: usize) -> u8 {
    let Ok(byte) = u8::try_from(value) else {
        unreachable!("the closed path-contract carrier fits in one byte")
    };
    byte
}

impl PathContractWord {
    /// Target endpoint after every stage.
    #[must_use]
    pub fn target(&self) -> usize {
        self.stages.last().map_or(self.source, |stage| stage.target)
    }

    /// Ordinal within words of this exact length.
    #[must_use]
    pub fn ordinal(&self) -> usize {
        let radix = PATH_CONTRACT_ENDPOINTS * PathContractKind::ALL.len();
        let mut place = 1;
        let mut digits = 0;
        for stage in &self.stages {
            let digit = stage.kind.ordinal() * PATH_CONTRACT_ENDPOINTS + stage.target;
            digits += digit * place;
            place *= radix;
        }
        self.source * place + digits
    }

    /// Canonical injective byte word used only by the finite enumerator.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.stages.len() * 2);
        bytes.push(path_contract_byte(self.stages.len()));
        bytes.push(path_contract_byte(self.source));
        for stage in &self.stages {
            bytes.push(path_contract_byte(stage.kind.ordinal()));
            bytes.push(path_contract_byte(stage.target));
        }
        bytes
    }
}

/// Count words of one length from `2 * (3 * 2)^length`.
#[must_use]
pub const fn path_contract_count(length: usize) -> usize {
    PATH_CONTRACT_ENDPOINTS
        * power(
            PathContractKind::ALL.len() * PATH_CONTRACT_ENDPOINTS,
            length,
        )
}

/// Independent counts for lengths zero through three and their sum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractCounts {
    /// Counts by word length.
    pub by_length: [usize; PATH_CONTRACT_MAX_LENGTH + 1],
    /// Total number of typed words.
    pub total: usize,
}

/// Derive exact path counts without constructing a path.
#[must_use]
pub const fn path_contract_count_certificate() -> PathContractCounts {
    let mut by_length = [0; PATH_CONTRACT_MAX_LENGTH + 1];
    let mut total = 0;
    let mut length = 0;
    while length <= PATH_CONTRACT_MAX_LENGTH {
        by_length[length] = path_contract_count(length);
        total += by_length[length];
        length += 1;
    }
    PathContractCounts { by_length, total }
}

/// Decode one length-local ordinal into a typed composable word.
#[must_use]
pub fn path_contract_word(length: usize, ordinal: usize) -> Option<PathContractWord> {
    if length > PATH_CONTRACT_MAX_LENGTH || ordinal >= path_contract_count(length) {
        return None;
    }
    let radix = PATH_CONTRACT_ENDPOINTS * PathContractKind::ALL.len();
    let word_radix = power(radix, length);
    let source = ordinal / word_radix;
    let mut code = ordinal % word_radix;
    let mut current = source;
    let mut stages = Vec::with_capacity(length);
    for _ in 0..length {
        let digit = code % radix;
        code /= radix;
        let target = digit % PATH_CONTRACT_ENDPOINTS;
        let kind = PathContractKind::ALL[digit / PATH_CONTRACT_ENDPOINTS];
        stages.push(PathContractStage {
            source: current,
            target,
            kind,
        });
        current = target;
    }
    Some(PathContractWord { source, stages })
}

/// Stream every path-contract word through length three in
/// `(length, ordinal)` order.
pub fn for_each_path_contract_word(mut visit: impl FnMut(usize, usize, PathContractWord)) {
    for length in 0..=PATH_CONTRACT_MAX_LENGTH {
        for ordinal in 0..path_contract_count(length) {
            let Some(word) = path_contract_word(length, ordinal) else {
                unreachable!("ordinal is inside the certified length-local bound")
            };
            visit(length, ordinal, word);
        }
    }
}

/// Five-gate classification of one atomic behaviour and source state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractLawClass {
    /// Truth values in `GetPut`, `PutGet`, Totality, `PutTwice`, Congruence order.
    pub gates: [bool; 5],
}

/// Classify one atom with a one-element Single-Step Basis: the other view.
#[must_use]
pub const fn path_contract_law_class(
    kind: PathContractKind,
    source: usize,
) -> PathContractLawClass {
    let (view, complement) = kind.get(source);
    let edited = 1 - view;
    let get_put = match kind.put(view, complement) {
        Some(restored) => restored == source,
        None => false,
    };
    let restored = kind.put(edited, complement);
    let put_get = match restored {
        Some(next_source) => kind.get(next_source).0 == edited,
        None => false,
    };
    let put_twice = match restored {
        Some(next_source) => {
            let next_complement = kind.get(next_source).1;
            match kind.put(edited, next_complement) {
                Some(repeated) => repeated == next_source,
                None => false,
            }
        }
        None => false,
    };
    PathContractLawClass {
        gates: [
            get_put,
            put_get,
            restored.is_some(),
            put_twice,
            restored.is_some(),
        ],
    }
}

/// Independently derived aggregate for all twelve typed atoms and both
/// source states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathContractLawCounts {
    /// Typed atoms (`2 * 2 * 3`).
    pub stages: usize,
    /// Atom/source cases.
    pub source_cases: usize,
    /// SSB cases (one alternate view per source case).
    pub basis_cases: usize,
    /// Passing cases by gate order.
    pub gates: [usize; 5],
    /// Cases passing all five gates.
    pub all_five: usize,
}

/// Closed-form law counts, independent of the streamed evaluator.
#[must_use]
pub const fn path_contract_law_count_certificate() -> PathContractLawCounts {
    let stages = PATH_CONTRACT_ENDPOINTS * PATH_CONTRACT_ENDPOINTS * PathContractKind::ALL.len();
    let source_cases = stages * PARTIAL_LENS_SOURCES;
    let strict_put_get =
        PATH_CONTRACT_ENDPOINTS * PATH_CONTRACT_ENDPOINTS * 2 * PARTIAL_LENS_SOURCES;
    PathContractLawCounts {
        stages,
        source_cases,
        basis_cases: source_cases,
        gates: [
            source_cases,
            strict_put_get,
            source_cases,
            source_cases,
            source_cases,
        ],
        all_five: strict_put_get,
    }
}

/// Owner-selected alphabets still required for the full C11 claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C11MissingOwnerAlphabet {
    /// Graph/tree schema names and declaration constructors.
    SchemaDeclarations,
    /// Node values, presence states and field payloads.
    ValuesAndFields,
    /// Node shapes, metadata and edge positions.
    ShapesMetadataAndPositions,
    /// Tree fan signatures and fan occurrences.
    TreeFans,
    /// One-step graph/tree/table view edits and negative cases.
    OneStepViewEdits,
    /// Production atomic lens stages rather than the contract subcarrier.
    ProductionAtomicStages,
    /// Shape-specific graph/tree/table fusion contexts and candidates.
    ShapeFusionContexts,
}

/// Explicit full-C11 owner gaps retained after closing the structural path
/// contract subcarrier.
pub const C11_MISSING_OWNER_ALPHABETS: [C11MissingOwnerAlphabet; 7] = [
    C11MissingOwnerAlphabet::SchemaDeclarations,
    C11MissingOwnerAlphabet::ValuesAndFields,
    C11MissingOwnerAlphabet::ShapesMetadataAndPositions,
    C11MissingOwnerAlphabet::TreeFans,
    C11MissingOwnerAlphabet::OneStepViewEdits,
    C11MissingOwnerAlphabet::ProductionAtomicStages,
    C11MissingOwnerAlphabet::ShapeFusionContexts,
];

/// The remaining C11 graph/tree/path census is on hold until its owner fixes
/// the schema/name, value, node-shape, edge-position, fan, one-step-edit,
/// atomic-stage and fusion alphabets.
///
/// The partial-lens, table and GF5 carriers closed before the structural
/// path-contract subcarrier do not choose those missing parameters by
/// analogy with current production fixtures. The path-contract subcarrier
/// fixes only abstract two-state arrows, not the production alphabets below.
pub const C11_GRAPH_TREE_PATH_HOLD: &str = "HOLD: graph/tree schema and carrier alphabets, one-step edit alphabet, and LensPath atomic-stage/fusion alphabet require owner parameters";

/// A missing finite alphabet needed before the full C11 census is defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C11MissingDimension {
    /// Graph/tree schema names, values, node shapes, positions and fan data.
    GraphAndTreeCarrierAlphabet,
    /// The finite set of one-step view edits.
    OneStepViewEditAlphabet,
    /// Production atomic and fusible stages from which paths arise.
    LensPathStageAndFusionAlphabet,
}

/// Structured completion status for the full C11 finite census.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct C11ScopeStatus {
    /// False until every finite alphabet named by C11 has an owner ruling.
    pub scope_complete: bool,
    /// Closed subcarriers supplied by this reference page.
    pub closed_subcarriers: usize,
    /// Dimensions that still lack a binding finite alphabet.
    pub missing_dimensions: &'static [C11MissingDimension],
}

/// The closed partial-lens, table and GF5 carriers do not complete C11.
pub const C11_SCOPE_STATUS: C11ScopeStatus = C11ScopeStatus {
    scope_complete: false,
    closed_subcarriers: 3,
    missing_dimensions: &[
        C11MissingDimension::GraphAndTreeCarrierAlphabet,
        C11MissingDimension::OneStepViewEditAlphabet,
        C11MissingDimension::LensPathStageAndFusionAlphabet,
    ],
};
