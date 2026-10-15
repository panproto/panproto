//! Complete reference operations assigned to `kan-extensions`.
//!
//! # Hom-profile certificate (O14)
//!
//! The bounded checker quotients labelled table instances only by the group
//! `G = (product_t S_rows(t)) x S_nulls`: rows may be permuted independently
//! inside each table and the finite labelled-null alphabet may be permuted.
//! Table names, columns, relation labels, relation incidence, constants, and
//! exact floating-point bits are rigid. The certificate has five parts.
//!
//! 1. `enumerate::kan_extensions::hom_profile` evaluates every element of
//!    `G`, applies it structurally, and selects the least canonical table byte
//!    string. A finite total order has one least element, so every labelled
//!    object has exactly one profile. The separately implemented
//!    orbit--stabilizer counter proves each class multiplicity as
//!    `|G| / |Stab(object)|`; summing the certified class sizes recovers the
//!    labelled object and ordered-pair totals.
//! 2. Two members of a profile carry explicit inverse row and used-null
//!    bijections. They preserve every cell and relation pair, hence are
//!    inverse [`rel::Hom`]s. For isomorphisms `a: I -> I'` and `b: J -> J'`,
//!    conjugation sends `h: I -> J` to `a^-1 ; h ; b`. Its inverse conjugates
//!    by `a` and `b^-1`, establishing bijections on both hom-sets of both
//!    adjunctions.
//! 3. Sigma transports rows by `a`, merged null classes by their selected
//!    representative, and fresh nulls by their recorded `(row,column)` slot.
//!    Delta transports a copied row `(table,row)` to `(table,a(row))`. Pi
//!    transports `(table,[r1,...,rn])` to
//!    `(table,[a(r1),...,a(rn)])`. Substitution in the four transpose formulas
//!    shows that these output isomorphisms commute with both forward and
//!    inverse transposes. Fresh Sigma nulls use the derived output bijection,
//!    not an arbitrary unused-null permutation. A Sigma constant clash is
//!    profile-invariant exactly when its independently computed clash-component
//!    set is nonempty. Its public row and constant-pair payload is the first
//!    witness in numeric row order and is intentionally checked only on each
//!    labelled object, because row relabelling need not preserve that staging
//!    witness.
//! 4. Conjugation preserves identities and composition. Writing `Phi` for the
//!    Sigma--Delta transpose and `Psi` for Delta--Pi, the executable
//!    certificate checks
//!    `Phi(b;h;Sigma(a)) = a;Phi(h);Delta(b)` and
//!    `Psi(b;h;Delta(a)) = a;Psi(h);Pi(b)`, together with both inverse
//!    equations. Thus both identity composites and all naturality squares are
//!    invariant under profile transport.
//! 5. The bounded family has only leaf cells and no fan nodes. Its graph
//!    records therefore have semantic node keys `Row(r)` and `Cell(r,c)`, and
//!    edge keys `Attribute(r,c)` and `Relation(e,r,s)`. Row transport carries
//!    these keys bijectively. Fresh allocation may give corresponding keys
//!    different numeric ids after a retained-id collision, so the executable
//!    certificate joins the two independently materialized final layouts by
//!    key rather than assuming equal ids. That join is proved total and
//!    bijective, transports exact graph contents (including labelled nulls),
//!    and carries every node and edge contributor set through the input and
//!    output id bijections. Thus encoding, collision resolution, provenance,
//!    and decoding commute with each profile transport. The same id
//!    bijection carries roots, nodes and arcs of every admitted fan-free tree;
//!    root-count payloads are exact. For the only two non-tree families in
//!    this bounded carrier, the certificate transports the complete sets of
//!    nodes with a second parent and nodes unreached from the root, and checks
//!    that each returned numeric first witness belongs to its side's set. A
//!    row relabelling need not preserve which set member is first numerically.
//!    Every other tree-form refusal is pinned at zero, while per-labelled
//!    checks retain the exact public staging payload.
//!    The bounded checker executes these comparisons for all 248,552 labelled
//!    members: 516,960 table/graph routes (460,228 successes with provenance
//!    and 56,732 normalized typed refusals) and all 3,412 admitted tree routes
//!    (2,611 successes and 801 typed refusals). Sigma clash *membership* is
//!    transported by its clash-component criterion from clause 3; its numeric
//!    first-witness payload remains an object-level check. Representative-pair
//!    execution then uses only these proved table and adapter isomorphisms.

pub mod encode {
    //! Kan-extension-specific graph and tree encodings.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge, Table, Tree};
    use crate::pages::representation::encode::{
        NotATree, TreeEncodeError, graph_to_tree, schema_parent_name, tree_to_graph,
    };
    use crate::rel::{self, KanError, RelFunctor, RelSchema};
    use crate::restrict::is_fan_kind;
    use crate::value::{Presence, Value};

    /// What a graph is encoded over: tables, relations, columns (attribute
    /// arrows), value vertices and inner value edges.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct GraphSide {
        /// Tables.
        pub tables: BTreeSet<Name>,
        /// Relations.
        pub relations: BTreeSet<SEdge>,
        /// Table to column name to arrow.
        pub columns: BTreeMap<Name, BTreeMap<String, SEdge>>,
        /// Value vertices.
        pub value_vertices: BTreeSet<Name>,
        /// Inner value edges.
        pub inner_edges: BTreeSet<SEdge>,
    }

    /// The protocol information used by strict relational classification.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct KindPartition {
        /// Kinds whose undeclared vertices begin the value closure.
        pub value: BTreeSet<Name>,
        /// Kinds whose vertices are tables.
        pub non_value: BTreeSet<Name>,
    }

    /// Identity and position recorded for an encoded graph edge.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct EdgeLayout {
        /// Original edge id, retained on its first collision-free occurrence.
        pub id: Option<u32>,
        /// Original or selected position.
        pub position: Option<u32>,
        /// Whether this edge has an input contributor.
        pub has_source: bool,
    }

    /// The graph-only shape and decorations of one encoded cell subtree.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct CellLayout {
        /// Root value-node id.
        pub root: u32,
        /// Attribute edge from the table row to the root.
        pub attribute: EdgeLayout,
        /// Every value node of the subtree, by original id.
        pub nodes: BTreeMap<u32, Node>,
        /// Every inner edge of the subtree, by original id.
        pub edges: BTreeMap<u32, GEdge>,
    }

    /// The graph-only record paired with an encoded table.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct GraphLayout {
        /// Full table-node decorations, keyed by row id.
        pub table_nodes: BTreeMap<u32, Node>,
        /// Full value-subtree layouts, keyed by row and column.
        pub cells: BTreeMap<(u32, String), CellLayout>,
        /// Relation-edge identity and position, keyed by relation pair.
        pub relations: BTreeMap<(SEdge, u32, u32), EdgeLayout>,
        /// Every input/layout node id, which contributes to the fresh floor.
        pub reserved_node_ids: BTreeSet<u32>,
        /// Every input/layout edge id, which contributes to the fresh floor.
        pub reserved_edge_ids: BTreeSet<u32>,
    }

    impl GraphSide {
        /// The whole relation-ified schema.
        #[must_use]
        pub fn whole(r: &RelSchema) -> Self {
            Self {
                tables: r.tables.clone(),
                relations: r.relations.clone(),
                columns: r.columns.clone(),
                value_vertices: r.value_vertices.clone(),
                inner_edges: r.inner_edges.clone(),
            }
        }

        /// The kept part of a functor's source: kept tables, relations and
        /// columns, the targets of kept columns and the inner edges below them.
        #[must_use]
        pub fn kept(f: &RelFunctor) -> Self {
            let tables: BTreeSet<Name> = f.tables.keys().cloned().collect();
            let mut columns: BTreeMap<Name, BTreeMap<String, SEdge>> = tables
                .iter()
                .map(|t| (t.clone(), BTreeMap::new()))
                .collect();
            for (s, c) in f.columns.keys() {
                if let Some(a) = f.src.columns.get(s).and_then(|cs| cs.get(c)) {
                    columns
                        .entry(s.clone())
                        .or_default()
                        .insert(c.clone(), a.clone());
                }
            }
            let mut values: BTreeSet<Name> = columns
                .values()
                .flat_map(|cs| cs.values().map(|a| a.tgt.clone()))
                .collect();
            loop {
                let grow: Vec<Name> = f
                    .src
                    .inner_edges
                    .iter()
                    .filter(|e| values.contains(&e.src) && !values.contains(&e.tgt))
                    .map(|e| e.tgt.clone())
                    .collect();
                if grow.is_empty() {
                    break;
                }
                values.extend(grow);
            }
            let inner_edges = f
                .src
                .inner_edges
                .iter()
                .filter(|e| values.contains(&e.src))
                .cloned()
                .collect();
            Self {
                tables,
                relations: f.relations.keys().cloned().collect(),
                columns,
                value_vertices: values,
                inner_edges,
            }
        }

        fn is_column(&self, e: &SEdge) -> bool {
            self.columns
                .get(&e.src)
                .is_some_and(|cs| cs.values().any(|a| a == e))
        }

        fn table_side(&self) -> rel::Side {
            rel::Side {
                tables: self
                    .tables
                    .iter()
                    .map(|table| {
                        (
                            table.clone(),
                            self.columns
                                .get(table)
                                .map(|columns| columns.keys().cloned().collect())
                                .unwrap_or_default(),
                        )
                    })
                    .collect(),
                relations: self.relations.clone(),
            }
        }
    }

    /// Why a graph is not encodable over a side.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum NotEncodable {
        /// A fan node.
        FanNode {
            /// The node.
            node: u32,
        },
        /// An anchor neither a table nor a value vertex.
        AnchorOutsideSide {
            /// The node.
            node: u32,
        },
        /// An edge whose source or target is not a node of the graph.
        DanglingEdge {
            /// The edge.
            edge: u32,
        },
        /// A label neither a relation, a column nor an inner edge.
        EdgeOutsideSide {
            /// The edge.
            edge: u32,
        },
        /// Endpoints not anchored at the label's ends.
        IncoherentEndpoints {
            /// The edge.
            edge: u32,
        },
        /// Edges with one label and one pair of endpoints.
        ParallelEdges {
            /// The group, ascending.
            edges: Vec<u32>,
        },
        /// A column of a table node with no out-edge.
        MissingCell {
            /// The node.
            node: u32,
            /// The column.
            column: String,
        },
        /// As `MissingCell`, the node holding a field of that name.
        CellAsExtraField {
            /// The node.
            node: u32,
            /// The column.
            column: String,
        },
        /// A column with several out-edges.
        RepeatedCell {
            /// The node.
            node: u32,
            /// The column.
            column: String,
            /// The edges.
            edges: Vec<u32>,
        },
        /// A value node with several in-edges.
        SharedValue {
            /// The node.
            node: u32,
            /// The in-edges.
            edges: Vec<u32>,
        },
        /// A value node reaching no table node.
        UnrootedValue {
            /// The node.
            node: u32,
        },
        /// A leaf value node with no value (or an absent one).
        MissingValue {
            /// The node.
            node: u32,
        },
        /// A value node with out-edges of mixed or repeated shape, or holding a
        /// value.
        InnerValueShape {
            /// The node.
            node: u32,
        },
        /// A labelled null below the top level of a cell.
        NestedLabeledNull {
            /// The node.
            node: u32,
        },
    }

    /// The cell a value node folds to: its value for a leaf (`Null` for a null
    /// presence), the record of its children by column name, or the list of its
    /// children by position.
    fn fold(g: &Graph, n: u32, depth: usize) -> Value {
        let out: Vec<&GEdge> = g.edges.values().filter(|e| e.src == n).collect();
        if out.is_empty() || depth > g.nodes.len() {
            return match g.nodes.get(&n).and_then(|x| x.value.clone()) {
                Some(Presence::Present(v)) => v,
                _ => Value::Null,
            };
        }
        if out.iter().all(|e| e.position.is_some()) {
            let mut items: Vec<(Option<u32>, u32)> =
                out.iter().map(|e| (e.position, e.tgt)).collect();
            items.sort_unstable();
            return Value::List(
                items
                    .into_iter()
                    .map(|(_, t)| fold(g, t, depth + 1))
                    .collect(),
            );
        }
        Value::Unknown(
            out.iter()
                .map(|e| (e.label.column().to_owned(), fold(g, e.tgt, depth + 1)))
                .collect(),
        )
    }

    fn pass_nodes(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
        for (id, n) in &g.nodes {
            if n.shape == NodeShape::HyperEdge
                || g.edges
                    .values()
                    .any(|e| e.src == *id && is_fan_kind(&e.label.kind))
            {
                return Err(NotEncodable::FanNode { node: *id });
            }
            if !side.tables.contains(&n.anchor) && !side.value_vertices.contains(&n.anchor) {
                return Err(NotEncodable::AnchorOutsideSide { node: *id });
            }
        }
        Ok(())
    }

    fn pass_edges(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
        for (id, e) in &g.edges {
            let (Some(from), Some(to)) = (g.nodes.get(&e.src), g.nodes.get(&e.tgt)) else {
                return Err(NotEncodable::DanglingEdge { edge: *id });
            };
            let l = &e.label;
            if !(side.relations.contains(l) || side.is_column(l) || side.inner_edges.contains(l)) {
                return Err(NotEncodable::EdgeOutsideSide { edge: *id });
            }
            if from.anchor != l.src || to.anchor != l.tgt {
                return Err(NotEncodable::IncoherentEndpoints { edge: *id });
            }
        }
        let mut groups: BTreeMap<(SEdge, u32, u32), Vec<u32>> = BTreeMap::new();
        for (id, e) in &g.edges {
            groups
                .entry((e.label.clone(), e.src, e.tgt))
                .or_default()
                .push(*id);
        }
        if let Some(edges) = groups
            .into_values()
            .filter(|v| v.len() > 1)
            .min_by_key(|v| v[0])
        {
            return Err(NotEncodable::ParallelEdges { edges });
        }
        Ok(())
    }

    fn pass_cells(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
        for (id, n) in g
            .nodes
            .iter()
            .filter(|(_, n)| side.tables.contains(&n.anchor))
        {
            for (column, arrow) in side.columns.get(&n.anchor).into_iter().flatten() {
                let edges: Vec<u32> = g
                    .edges
                    .iter()
                    .filter(|(_, e)| e.src == *id && e.label == *arrow)
                    .map(|(i, _)| *i)
                    .collect();
                match edges.len() {
                    1 => {}
                    0 if n.fields.contains_key(column) => {
                        return Err(NotEncodable::CellAsExtraField {
                            node: *id,
                            column: column.clone(),
                        });
                    }
                    0 => {
                        return Err(NotEncodable::MissingCell {
                            node: *id,
                            column: column.clone(),
                        });
                    }
                    _ => {
                        return Err(NotEncodable::RepeatedCell {
                            node: *id,
                            column: column.clone(),
                            edges,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether following in-edges up from `n` reaches a table node: at each
    /// node the in-edge with the least id is followed, and the walk fails at
    /// a node without one or on returning to a node it has passed.
    fn rooted(g: &Graph, side: &GraphSide, n: u32) -> bool {
        let mut seen = BTreeSet::new();
        let mut cur = n;
        while seen.insert(cur) {
            let Some(edge) = g.edges.values().find(|e| e.tgt == cur) else {
                return false;
            };
            let Some(parent) = g.nodes.get(&edge.src) else {
                return false;
            };
            if side.tables.contains(&parent.anchor) {
                return true;
            }
            cur = edge.src;
        }
        false
    }

    fn pass_values(g: &Graph, side: &GraphSide) -> Result<(), NotEncodable> {
        for (id, n) in g
            .nodes
            .iter()
            .filter(|(_, n)| side.value_vertices.contains(&n.anchor))
        {
            let ins: Vec<u32> = g
                .edges
                .iter()
                .filter(|(_, e)| e.tgt == *id)
                .map(|(i, _)| *i)
                .collect();
            if ins.len() > 1 {
                return Err(NotEncodable::SharedValue {
                    node: *id,
                    edges: ins,
                });
            }
            if !rooted(g, side, *id) {
                return Err(NotEncodable::UnrootedValue { node: *id });
            }
            let out: Vec<&GEdge> = g.edges.values().filter(|e| e.src == *id).collect();
            if out.is_empty() {
                if !matches!(n.value, Some(Presence::Present(_) | Presence::Null)) {
                    return Err(NotEncodable::MissingValue { node: *id });
                }
            } else {
                let unpositioned = out.iter().all(|e| e.position.is_none());
                let names: BTreeSet<&str> = out.iter().map(|e| e.label.column()).collect();
                let positions: BTreeSet<Option<u32>> = out.iter().map(|e| e.position).collect();
                let shape_ok = n.value.is_none()
                    && ((unpositioned && names.len() == out.len())
                        || (out.iter().all(|e| e.position.is_some())
                            && positions.len() == out.len()));
                if !shape_ok {
                    return Err(NotEncodable::InnerValueShape { node: *id });
                }
            }
            let nested = match &n.value {
                Some(Presence::Present(v)) => {
                    v.has_nested_labeled_null()
                        || (matches!(v, Value::LabeledNull(_))
                            && ins
                                .iter()
                                .any(|i| side.inner_edges.contains(&g.edges[i].label)))
                }
                _ => false,
            };
            if nested {
                return Err(NotEncodable::NestedLabeledNull { node: *id });
            }
        }
        Ok(())
    }

    /// Definition: Encode a graph over a relational side by five checks, then
    /// materialize its rows and relation pairs.
    ///
    /// The checks cover nodes, edges, parallel edges, cells and value nodes.
    /// The result has one row per table node with the folds of its cells, and
    /// one pair per relation edge.
    ///
    /// The edge check refuses first an edge whose source or target is not a
    /// node (`DanglingEdge`).
    ///
    /// # Errors
    ///
    /// The first [`NotEncodable`] reason of the first failing check.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn encode_graph(g: &Graph, side: &GraphSide) -> Result<(Table, GraphLayout), NotEncodable> {
        pass_nodes(g, side)?;
        pass_edges(g, side)?;
        pass_cells(g, side)?;
        pass_values(g, side)?;
        let mut t = Table::default();
        let mut layout = GraphLayout {
            reserved_node_ids: g.nodes.keys().copied().collect(),
            reserved_edge_ids: g.edges.keys().copied().collect(),
            ..GraphLayout::default()
        };
        for (id, n) in g
            .nodes
            .iter()
            .filter(|(_, n)| side.tables.contains(&n.anchor))
        {
            layout.table_nodes.insert(*id, n.clone());
            let mut cells = BTreeMap::new();
            for (column, arrow) in side.columns.get(&n.anchor).into_iter().flatten() {
                if let Some((edge_id, e)) = g
                    .edges
                    .iter()
                    .find(|(_, e)| e.src == *id && e.label == *arrow)
                {
                    cells.insert(column.clone(), fold(g, e.tgt, 0));
                    layout.cells.insert(
                        (*id, column.clone()),
                        collect_cell_layout(g, *edge_id, e.tgt, side),
                    );
                }
            }
            t.rows.insert(*id, (n.anchor.clone(), cells));
        }
        for (edge_id, e) in &g.edges {
            if !side.relations.contains(&e.label) {
                continue;
            }
            t.pairs.insert((e.label.clone(), e.src, e.tgt));
            layout.relations.insert(
                (e.label.clone(), e.src, e.tgt),
                EdgeLayout {
                    id: Some(*edge_id),
                    position: e.position,
                    has_source: true,
                },
            );
        }
        Ok((t, layout))
    }

    fn collect_cell_layout(
        graph: &Graph,
        attribute: u32,
        root: u32,
        side: &GraphSide,
    ) -> CellLayout {
        let mut layout = CellLayout {
            root,
            attribute: EdgeLayout {
                id: Some(attribute),
                position: graph.edges[&attribute].position,
                has_source: true,
            },
            ..CellLayout::default()
        };
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if layout.nodes.contains_key(&node) {
                continue;
            }
            layout.nodes.insert(node, graph.nodes[&node].clone());
            let mut outgoing: Vec<(u32, &GEdge)> = graph
                .edges
                .iter()
                .filter(|(_, edge)| edge.src == node && side.inner_edges.contains(&edge.label))
                .map(|(id, edge)| (*id, edge))
                .collect();
            outgoing.sort_by_key(|(id, edge)| (edge.position, edge.label.column().to_owned(), *id));
            for (id, edge) in outgoing.into_iter().rev() {
                pending.push(edge.tgt);
                layout.edges.insert(id, edge.clone());
            }
        }
        layout
    }

    /// Definition: the restriction of a graph to a side.
    ///
    /// The nodes anchored at a table or a value vertex of the side, with
    /// their ids and data, and the edges between two kept nodes whose label
    /// is a relation, an attribute arrow or an inner value edge of the side,
    /// with their ids and positions. On a graph encodable over a functor's
    /// whole source and the side [`GraphSide::kept`], this is the graph
    /// restriction `rho_S'`: a node at a dropped table, a value node at a
    /// dropped column and every edge they meet go.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    #[must_use]
    pub fn restrict_graph(g: &Graph, side: &GraphSide) -> Graph {
        let nodes: BTreeMap<u32, Node> = g
            .nodes
            .iter()
            .filter(|(_, node)| {
                side.tables.contains(&node.anchor) || side.value_vertices.contains(&node.anchor)
            })
            .map(|(id, node)| (*id, node.clone()))
            .collect();
        let edges = g
            .edges
            .iter()
            .filter(|(_, edge)| {
                (side.relations.contains(&edge.label)
                    || side.is_column(&edge.label)
                    || side.inner_edges.contains(&edge.label))
                    && nodes.contains_key(&edge.src)
                    && nodes.contains_key(&edge.tgt)
            })
            .map(|(id, edge)| (*id, edge.clone()))
            .collect();
        Graph { nodes, edges }
    }

    /// Why decoding a table into its graph carrier can refuse.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum DecodeTableError {
        /// A relation pair names a row absent from the table.
        MalformedTable,
        /// The table does not conform to the relational side.
        NonConforming(Box<rel::NonConforming>),
        /// No non-reserved node id remains for a cell-value node.
        NodeIdsExhausted,
        /// No non-reserved edge id remains for a cell or relation edge.
        EdgeIdsExhausted,
        /// Created outgoing edges cannot be positioned after the carried ones.
        EdgePositionsExhausted {
            /// Least source node at which the position overflows.
            node: u32,
        },
    }

    /// Definition: Decode a conforming, well-formed table over a side through
    /// its graph layout; a default record gives one leaf per cell.
    ///
    /// A table node per row (its id); per cell, in ascending (row, column), one leaf value node (a
    /// fresh id above every row id) holding the cell (a null presence for `Null`) and its attribute
    /// edge; one edge per pair. Attribute edges take ids from 0 in (row, column) order, then relation
    /// edges in (relation, source, target) order.
    ///
    /// # Errors
    ///
    /// [`DecodeTableError::MalformedTable`] when a pair names an absent row,
    /// [`DecodeTableError::NonConforming`] for the first table, cell or pair
    /// outside the side, then an allocation error before an allocation would
    /// use the reserved id `u32::MAX`.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn decode_table(
        t: &Table,
        side: &GraphSide,
        layout: &GraphLayout,
    ) -> Result<Graph, DecodeTableError> {
        if !t.well_formed() {
            return Err(DecodeTableError::MalformedTable);
        }
        rel::conform(t, &side.table_side()).map_err(DecodeTableError::NonConforming)?;
        let mut g = Graph::default();
        let mut claimed_nodes: BTreeSet<u32> = t.rows.keys().copied().collect();
        let mut claimed_edges = BTreeSet::new();
        let mut created_edges = BTreeSet::new();
        let mut next_node = t
            .rows
            .keys()
            .copied()
            .chain(layout.reserved_node_ids.iter().copied())
            .max()
            .map_or(Some(0), |id| id.checked_add(1))
            .filter(|id| *id != u32::MAX);
        let mut next_edge = layout
            .reserved_edge_ids
            .iter()
            .next_back()
            .map_or(Some(0), |id| id.checked_add(1))
            .filter(|id| *id != u32::MAX);
        preflight_allocations(
            t,
            side,
            layout,
            &claimed_nodes,
            &claimed_edges,
            next_node,
            next_edge,
        )?;
        decode_table_nodes(t, layout, &mut g);
        for (id, (table, cells)) in &t.rows {
            for (column, v) in cells {
                let Some(arrow) = side.columns.get(table).and_then(|c| c.get(column)) else {
                    continue;
                };
                if let Some(candidate) = layout
                    .cells
                    .get(&(*id, column.clone()))
                    .filter(|candidate| cell_fits(candidate, v, arrow, side))
                {
                    decode_cell_layout(
                        candidate,
                        *id,
                        arrow,
                        &mut DecodeState {
                            graph: &mut g,
                            claimed_nodes: &mut claimed_nodes,
                            claimed_edges: &mut claimed_edges,
                            next_node: &mut next_node,
                            next_edge: &mut next_edge,
                            created_edges: &mut created_edges,
                        },
                    )?;
                } else {
                    let node_id =
                        take_id(&mut next_node).ok_or(DecodeTableError::NodeIdsExhausted)?;
                    claimed_nodes.insert(node_id);
                    let edge_id =
                        take_id(&mut next_edge).ok_or(DecodeTableError::EdgeIdsExhausted)?;
                    claimed_edges.insert(edge_id);
                    created_edges.insert(edge_id);
                    let mut leaf = Node::new(node_id, &arrow.tgt);
                    leaf.value = Some(if *v == Value::Null {
                        Presence::Null
                    } else {
                        Presence::Present(v.clone())
                    });
                    g.nodes.insert(node_id, leaf);
                    g.edges.insert(
                        edge_id,
                        GEdge {
                            src: *id,
                            tgt: node_id,
                            label: arrow.clone(),
                            position: None,
                        },
                    );
                }
            }
        }
        decode_relations(
            t,
            layout,
            &mut g,
            &mut claimed_edges,
            &mut next_edge,
            &mut created_edges,
        )?;
        assign_created_positions(&mut g, &created_edges)?;
        Ok(g)
    }

    fn decode_table_nodes(table: &Table, layout: &GraphLayout, graph: &mut Graph) {
        for (id, (owner, _)) in &table.rows {
            let mut node = layout
                .table_nodes
                .get(id)
                .cloned()
                .unwrap_or_else(|| Node::new(*id, owner));
            node.id = *id;
            node.anchor.clone_from(owner);
            graph.nodes.insert(*id, node);
        }
    }

    fn decode_relations(
        table: &Table,
        layout: &GraphLayout,
        graph: &mut Graph,
        claimed_edges: &mut BTreeSet<u32>,
        next_edge: &mut Option<u32>,
        created_edges: &mut BTreeSet<u32>,
    ) -> Result<(), DecodeTableError> {
        for (edge, source, target) in &table.pairs {
            let entry = layout
                .relations
                .get(&(edge.clone(), *source, *target))
                .copied()
                .unwrap_or_default();
            let edge_id = choose_id(
                entry.id,
                claimed_edges,
                next_edge,
                DecodeTableError::EdgeIdsExhausted,
            )?;
            if !entry.has_source {
                created_edges.insert(edge_id);
            }
            graph.edges.insert(
                edge_id,
                GEdge {
                    src: *source,
                    tgt: *target,
                    label: edge.clone(),
                    position: entry.position,
                },
            );
        }
        Ok(())
    }

    fn preflight_allocations(
        table: &Table,
        side: &GraphSide,
        layout: &GraphLayout,
        initial_nodes: &BTreeSet<u32>,
        initial_edges: &BTreeSet<u32>,
        initial_next_node: Option<u32>,
        initial_next_edge: Option<u32>,
    ) -> Result<(), DecodeTableError> {
        // S3(11) fixes the refusal order independently of the row/column walk:
        // every node allocation is proved first, then every edge allocation;
        // positions are checked only after materialization.
        let mut claimed_nodes = initial_nodes.clone();
        let mut next_node = initial_next_node;
        for (row, (table_name, cells)) in &table.rows {
            for (column, value) in cells {
                let Some(arrow) = side
                    .columns
                    .get(table_name)
                    .and_then(|columns| columns.get(column))
                else {
                    continue;
                };
                if let Some(candidate) = layout
                    .cells
                    .get(&(*row, column.clone()))
                    .filter(|candidate| cell_fits(candidate, value, arrow, side))
                {
                    let (node_order, _) =
                        cell_order(candidate).map_err(|()| DecodeTableError::MalformedTable)?;
                    for source in node_order {
                        choose_id(
                            Some(source),
                            &mut claimed_nodes,
                            &mut next_node,
                            DecodeTableError::NodeIdsExhausted,
                        )?;
                    }
                } else {
                    let id = take_id(&mut next_node).ok_or(DecodeTableError::NodeIdsExhausted)?;
                    claimed_nodes.insert(id);
                }
            }
        }

        let mut claimed_edges = initial_edges.clone();
        let mut next_edge = initial_next_edge;
        for (row, (table_name, cells)) in &table.rows {
            for (column, value) in cells {
                let Some(arrow) = side
                    .columns
                    .get(table_name)
                    .and_then(|columns| columns.get(column))
                else {
                    continue;
                };
                if let Some(candidate) = layout
                    .cells
                    .get(&(*row, column.clone()))
                    .filter(|candidate| cell_fits(candidate, value, arrow, side))
                {
                    let (_, edge_order) =
                        cell_order(candidate).map_err(|()| DecodeTableError::MalformedTable)?;
                    choose_id(
                        candidate.attribute.id,
                        &mut claimed_edges,
                        &mut next_edge,
                        DecodeTableError::EdgeIdsExhausted,
                    )?;
                    for source in edge_order {
                        choose_id(
                            Some(source),
                            &mut claimed_edges,
                            &mut next_edge,
                            DecodeTableError::EdgeIdsExhausted,
                        )?;
                    }
                } else {
                    let id = take_id(&mut next_edge).ok_or(DecodeTableError::EdgeIdsExhausted)?;
                    claimed_edges.insert(id);
                }
            }
        }
        for (edge, source, target) in &table.pairs {
            let entry = layout
                .relations
                .get(&(edge.clone(), *source, *target))
                .copied()
                .unwrap_or_default();
            choose_id(
                entry.id,
                &mut claimed_edges,
                &mut next_edge,
                DecodeTableError::EdgeIdsExhausted,
            )?;
        }
        Ok(())
    }

    fn assign_created_positions(
        graph: &mut Graph,
        created: &BTreeSet<u32>,
    ) -> Result<(), DecodeTableError> {
        let mut by_source: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (id, edge) in &graph.edges {
            by_source.entry(edge.src).or_default().push(*id);
        }
        for (node, mut ids) in by_source {
            let max_carried = ids
                .iter()
                .filter(|id| !created.contains(id))
                .filter_map(|id| graph.edges[id].position)
                .max();
            let Some(mut position) = max_carried else {
                for id in ids.iter().filter(|id| created.contains(id)) {
                    if let Some(edge) = graph.edges.get_mut(id) {
                        edge.position = None;
                    }
                }
                continue;
            };
            ids.retain(|id| created.contains(id));
            ids.sort_unstable();
            for id in ids {
                position = position
                    .checked_add(1)
                    .ok_or(DecodeTableError::EdgePositionsExhausted { node })?;
                if let Some(edge) = graph.edges.get_mut(&id) {
                    edge.position = Some(position);
                }
            }
        }
        Ok(())
    }

    fn choose_id(
        candidate: Option<u32>,
        claimed: &mut BTreeSet<u32>,
        next: &mut Option<u32>,
        exhausted: DecodeTableError,
    ) -> Result<u32, DecodeTableError> {
        if let Some(id) = candidate.filter(|id| !claimed.contains(id)) {
            claimed.insert(id);
            return Ok(id);
        }
        let id = take_id(next).ok_or(exhausted)?;
        claimed.insert(id);
        Ok(id)
    }

    fn cell_fits(cell: &CellLayout, value: &Value, arrow: &SEdge, side: &GraphSide) -> bool {
        let Some(root) = cell.nodes.get(&cell.root) else {
            return false;
        };
        if root.anchor != arrow.tgt {
            return false;
        }
        let anchored = |node: u32, anchor: &str| {
            cell.nodes
                .get(&node)
                .is_some_and(|candidate| candidate.anchor == anchor)
        };
        if cell.edges.values().any(|edge| {
            !side.inner_edges.contains(&edge.label)
                || !anchored(edge.src, &edge.label.src)
                || !anchored(edge.tgt, &edge.label.tgt)
        }) {
            return false;
        }
        let Ok((nodes, edges)) = cell_order(cell) else {
            return false;
        };
        nodes.len() == cell.nodes.len()
            && edges.len() == cell.edges.len()
            && fold_layout(cell).is_some_and(|folded| {
                crate::canonical::value_bytes(&folded) == crate::canonical::value_bytes(value)
            })
    }

    fn cell_order(cell: &CellLayout) -> Result<(Vec<u32>, Vec<u32>), ()> {
        fn visit(
            cell: &CellLayout,
            node: u32,
            active: &mut BTreeSet<u32>,
            seen: &mut BTreeSet<u32>,
            nodes: &mut Vec<u32>,
            edges: &mut Vec<u32>,
        ) -> Result<(), ()> {
            if active.contains(&node) || seen.contains(&node) || !cell.nodes.contains_key(&node) {
                return Err(());
            }
            active.insert(node);
            seen.insert(node);
            nodes.push(node);
            let mut outgoing: Vec<(u32, &GEdge)> = cell
                .edges
                .iter()
                .filter(|(_, edge)| edge.src == node)
                .map(|(id, edge)| (*id, edge))
                .collect();
            outgoing.sort_by_key(|(id, edge)| (edge.position, edge.label.column().to_owned(), *id));
            for (id, edge) in outgoing {
                edges.push(id);
                visit(cell, edge.tgt, active, seen, nodes, edges)?;
            }
            active.remove(&node);
            Ok(())
        }

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        visit(
            cell,
            cell.root,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut nodes,
            &mut edges,
        )?;
        Ok((nodes, edges))
    }

    fn fold_layout(cell: &CellLayout) -> Option<Value> {
        fn go(cell: &CellLayout, node: u32, active: &mut BTreeSet<u32>) -> Option<Value> {
            if !active.insert(node) {
                return None;
            }
            let value_node = cell.nodes.get(&node)?;
            let mut outgoing: Vec<(u32, &GEdge)> = cell
                .edges
                .iter()
                .filter(|(_, edge)| edge.src == node)
                .map(|(id, edge)| (*id, edge))
                .collect();
            let folded = if outgoing.is_empty() {
                match &value_node.value {
                    Some(Presence::Present(value)) => Some(value.clone()),
                    Some(Presence::Null) => Some(Value::Null),
                    None | Some(Presence::Absent) => None,
                }
            } else if outgoing.iter().all(|(_, edge)| edge.position.is_some()) {
                outgoing.sort_by_key(|(id, edge)| (edge.position, *id));
                let unique: BTreeSet<Option<u32>> =
                    outgoing.iter().map(|(_, edge)| edge.position).collect();
                if unique.len() != outgoing.len() || value_node.value.is_some() {
                    None
                } else {
                    outgoing
                        .into_iter()
                        .map(|(_, edge)| go(cell, edge.tgt, active))
                        .collect::<Option<Vec<_>>>()
                        .map(Value::List)
                }
            } else if outgoing.iter().all(|(_, edge)| edge.position.is_none()) {
                outgoing.sort_by_key(|(id, edge)| (edge.label.column().to_owned(), *id));
                let unique: BTreeSet<String> = outgoing
                    .iter()
                    .map(|(_, edge)| edge.label.column().to_owned())
                    .collect();
                if unique.len() != outgoing.len() || value_node.value.is_some() {
                    None
                } else {
                    outgoing
                        .into_iter()
                        .map(|(_, edge)| {
                            go(cell, edge.tgt, active)
                                .map(|value| (edge.label.column().to_owned(), value))
                        })
                        .collect::<Option<BTreeMap<_, _>>>()
                        .map(Value::Unknown)
                }
            } else {
                None
            };
            active.remove(&node);
            folded
        }
        go(cell, cell.root, &mut BTreeSet::new())
    }

    struct DecodeState<'a> {
        graph: &'a mut Graph,
        claimed_nodes: &'a mut BTreeSet<u32>,
        claimed_edges: &'a mut BTreeSet<u32>,
        next_node: &'a mut Option<u32>,
        next_edge: &'a mut Option<u32>,
        created_edges: &'a mut BTreeSet<u32>,
    }

    fn decode_cell_layout(
        cell: &CellLayout,
        row: u32,
        arrow: &SEdge,
        state: &mut DecodeState<'_>,
    ) -> Result<(), DecodeTableError> {
        let (node_order, edge_order) =
            cell_order(cell).map_err(|()| DecodeTableError::MalformedTable)?;
        let mut node_map = BTreeMap::new();
        for source in node_order {
            let target = choose_id(
                Some(source),
                state.claimed_nodes,
                state.next_node,
                DecodeTableError::NodeIdsExhausted,
            )?;
            let mut node = cell.nodes[&source].clone();
            node.id = target;
            state.graph.nodes.insert(target, node);
            node_map.insert(source, target);
        }
        let attribute_id = choose_id(
            cell.attribute.id,
            state.claimed_edges,
            state.next_edge,
            DecodeTableError::EdgeIdsExhausted,
        )?;
        if !cell.attribute.has_source {
            state.created_edges.insert(attribute_id);
        }
        state.graph.edges.insert(
            attribute_id,
            GEdge {
                src: row,
                tgt: node_map[&cell.root],
                label: arrow.clone(),
                position: cell.attribute.position,
            },
        );
        for source_id in edge_order {
            let edge = &cell.edges[&source_id];
            let target_id = choose_id(
                Some(source_id),
                state.claimed_edges,
                state.next_edge,
                DecodeTableError::EdgeIdsExhausted,
            )?;
            let mut moved = edge.clone();
            moved.src = node_map[&edge.src];
            moved.tgt = node_map[&edge.tgt];
            state.graph.edges.insert(target_id, moved);
        }
        Ok(())
    }

    fn take_id(next: &mut Option<u32>) -> Option<u32> {
        let id = (*next)?;
        *next = id.checked_add(1).filter(|next| *next != u32::MAX);
        Some(id)
    }

    /// Why a graph Kan extension refuses.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum GraphKanError {
        /// The input is not encodable.
        NotEncodable(Box<NotEncodable>),
        /// A table Kan extension or conformance check refused.
        Adjunction(Box<rel::AdjunctionError>),
        /// A schema has no relational classification.
        RelSchema(Box<rel::RelSchemaError>),
        /// The migration has no relational functor.
        Functor(Box<rel::FunctorError>),
        /// A graph Kan homomorphism refused.
        Hom(Box<rel::HomError>),
        /// No non-reserved node id remains for a decoded value node.
        NodeIdsExhausted,
        /// No non-reserved edge id remains for a decoded cell or relation edge.
        EdgeIdsExhausted,
        /// Created outgoing edges cannot be positioned after carried edges.
        EdgePositionsExhausted {
            /// Least source node at which positioning overflows.
            node: u32,
        },
    }

    fn graph_decode_error(error: DecodeTableError) -> GraphKanError {
        match error {
            DecodeTableError::NodeIdsExhausted => GraphKanError::NodeIdsExhausted,
            DecodeTableError::EdgeIdsExhausted => GraphKanError::EdgeIdsExhausted,
            DecodeTableError::EdgePositionsExhausted { node } => {
                GraphKanError::EdgePositionsExhausted { node }
            }
            DecodeTableError::NonConforming(reason) => {
                GraphKanError::Adjunction(Box::new(rel::AdjunctionError::NonConforming(reason)))
            }
            DecodeTableError::MalformedTable => {
                GraphKanError::Adjunction(Box::new(rel::AdjunctionError::MalformedTable))
            }
        }
    }

    fn graph_adjunction_error(error: KanError) -> GraphKanError {
        GraphKanError::Adjunction(Box::new(error.into()))
    }

    /// A homomorphism between encodable graphs over one relational side.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct GraphKanHom {
        /// Map on table nodes; value-subtree and relation-edge images are induced.
        pub node_map: BTreeMap<u32, u32>,
        /// Image of each labelled null of the domain.
        pub null_map: BTreeMap<u64, Value>,
    }

    impl GraphKanHom {
        /// Identity on an encodable graph.
        ///
        /// # Errors
        ///
        /// The graph's first [`NotEncodable`] reason.
        pub fn identity(graph: &Graph, side: &GraphSide) -> Result<Self, GraphKanError> {
            let (table, _) = encode_graph(graph, side)
                .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
            Ok(Self {
                node_map: table.rows.keys().map(|row| (*row, *row)).collect(),
                null_map: BTreeMap::new(),
            })
        }

        /// Check this homomorphism in graph-encoding then table-hom order.
        ///
        /// # Errors
        ///
        /// The domain's first encoding error, the codomain's, then the first
        /// [`rel::HomError`].
        pub fn check(
            &self,
            domain: &Graph,
            codomain: &Graph,
            side: &GraphSide,
        ) -> Result<(), GraphKanError> {
            let (domain, _) = encode_graph(domain, side)
                .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
            let (codomain, _) = encode_graph(codomain, side)
                .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
            self.as_table()
                .check(&domain, &codomain)
                .map_err(|error| GraphKanError::Hom(Box::new(error)))
        }

        /// Compose `self: A -> B` with `after: B -> C`.
        ///
        /// # Errors
        ///
        /// The least missing intermediate row.
        pub fn compose(&self, after: &Self) -> Result<Self, GraphKanError> {
            self.as_table()
                .compose(&after.as_table())
                .map(Self::from_table)
                .map_err(|error| GraphKanError::Hom(Box::new(error)))
        }

        /// Equality after normalising explicit identity null images.
        #[must_use]
        pub fn same_as(&self, other: &Self) -> bool {
            self.as_table().normalized_same_as(&other.as_table())
        }

        fn as_table(&self) -> rel::Hom {
            rel::Hom {
                rows: self.node_map.clone(),
                nulls: self.null_map.clone(),
            }
        }

        fn from_table(hom: rel::Hom) -> Self {
            Self {
                node_map: hom.rows,
                null_map: hom.nulls,
            }
        }
    }

    fn move_cell(
        cell: &CellLayout,
        root: &str,
        mut image: impl FnMut(&SEdge, &str) -> Option<SEdge>,
    ) -> Option<CellLayout> {
        let (_, edge_order) = cell_order(cell).ok()?;
        let mut anchors = BTreeMap::from([(cell.root, root.to_owned())]);
        let mut moved = cell.clone();
        for edge_id in edge_order {
            let edge = cell.edges.get(&edge_id)?;
            let mapped = image(&edge.label, anchors.get(&edge.src)?)?;
            match anchors.entry(edge.tgt) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(mapped.tgt.clone());
                }
                std::collections::btree_map::Entry::Occupied(entry)
                    if entry.get() != &mapped.tgt =>
                {
                    return None;
                }
                std::collections::btree_map::Entry::Occupied(_) => {}
            }
            moved.edges.get_mut(&edge_id)?.label = mapped;
        }
        for (id, node) in &mut moved.nodes {
            node.anchor.clone_from(anchors.get(id)?);
        }
        Some(moved)
    }

    fn forward_inner(f: &RelFunctor, edge: &SEdge, source_image: &str) -> Option<SEdge> {
        if let Some(image) = f.inner_edges.get(edge) {
            return (image.src == source_image).then(|| image.clone());
        }
        let target = f
            .value_vertices
            .get(&edge.tgt)
            .cloned()
            .unwrap_or_else(|| edge.tgt.clone());
        let image = SEdge {
            src: source_image.to_owned(),
            tgt: target,
            kind: edge.kind.clone(),
            name: edge.name.clone(),
        };
        f.tgt.inner_edges.contains(&image).then_some(image)
    }

    fn move_forward(cell: &CellLayout, arrow: &SEdge, f: &RelFunctor) -> Option<CellLayout> {
        move_cell(cell, &arrow.tgt, |edge, source| {
            forward_inner(f, edge, source)
        })
    }

    fn move_back(cell: &CellLayout, arrow: &SEdge, f: &RelFunctor) -> Option<CellLayout> {
        move_cell(cell, &arrow.tgt, |target, source| {
            let mut preimages = f
                .src
                .inner_edges
                .iter()
                .filter(|candidate| candidate.src == source)
                .filter(|candidate| {
                    let source_image = f
                        .value_vertices
                        .get(&candidate.src)
                        .map_or(candidate.src.as_str(), String::as_str);
                    forward_inner(f, candidate, source_image).as_ref() == Some(target)
                });
            let first = preimages.next()?.clone();
            preimages.next().is_none().then_some(first)
        })
    }

    fn relation_is_injective_at(f: &RelFunctor, source: &SEdge) -> bool {
        let Some(image) = f.relations.get(source) else {
            return false;
        };
        f.relations
            .values()
            .filter(|candidate| *candidate == image)
            .count()
            == 1
            && f.fibre(&image.src).len() == 1
            && f.fibre(&image.tgt).len() == 1
    }

    fn selected_relation_layout(
        input: &GraphLayout,
        contributors: &[(SEdge, u32, u32)],
        retain_id: bool,
    ) -> EdgeLayout {
        let entries: Vec<EdgeLayout> = contributors
            .iter()
            .filter_map(|key| input.relations.get(key).copied())
            .collect();
        EdgeLayout {
            id: retain_id
                .then(|| entries.first().and_then(|entry| entry.id))
                .flatten(),
            position: entries.iter().filter_map(|entry| entry.position).min(),
            has_source: !entries.is_empty(),
        }
    }

    /// The input cells Sigma equates with the output slot `(row, column)`:
    /// the cells of the same row at the kept columns of its table that the
    /// functor sends to `column`, ascending.
    fn sigma_slot_sources(input: &Table, row: u32, column: &str, f: &RelFunctor) -> Vec<(u32, String)> {
        let Some((source_table, source_cells)) = input.rows.get(&row) else {
            return Vec::new();
        };
        f.columns
            .iter()
            .filter(|((table, source_column), image)| {
                table == source_table
                    && image.as_str() == column
                    && source_cells.contains_key(source_column)
            })
            .map(|((_, source_column), _)| (row, source_column.clone()))
            .collect()
    }

    fn sigma_layout(
        input: &Table,
        input_layout: &GraphLayout,
        output: &Table,
        f: &RelFunctor,
    ) -> GraphLayout {
        let target = GraphSide::whole(&f.tgt);
        let mut layout = GraphLayout {
            reserved_node_ids: input_layout.reserved_node_ids.clone(),
            reserved_edge_ids: input_layout.reserved_edge_ids.clone(),
            ..GraphLayout::default()
        };
        for (row, (source_table, _)) in &input.rows {
            if output.rows.contains_key(row)
                && let Some(node) = input_layout.table_nodes.get(row)
            {
                layout.table_nodes.insert(*row, node.clone());
            }
            let Some((target_table, target_cells)) = output.rows.get(row) else {
                continue;
            };
            for (target_column, value) in target_cells {
                let Some(arrow) = target
                    .columns
                    .get(target_table)
                    .and_then(|columns| columns.get(target_column))
                else {
                    continue;
                };
                let mut origins = Vec::new();
                for (source_row, source_column) in
                    sigma_slot_sources(input, *row, target_column, f)
                {
                    origins.push((
                        source_table.clone(),
                        source_row,
                        source_column,
                        arrow.clone(),
                    ));
                }
                origins.sort();
                let candidate =
                    origins
                        .into_iter()
                        .find_map(|(_, source_row, source_column, image_arrow)| {
                            input_layout
                                .cells
                                .get(&(source_row, source_column))
                                .and_then(|cell| move_forward(cell, &image_arrow, f))
                                .filter(|cell| cell_fits(cell, value, arrow, &target))
                        });
                if let Some(cell) = candidate {
                    layout.cells.insert((*row, target_column.clone()), cell);
                }
            }
        }
        for (target_edge, source, target_row) in &output.pairs {
            let contributors: Vec<(SEdge, u32, u32)> = f
                .relations
                .iter()
                .filter(|(_, image)| *image == target_edge)
                .filter(|(edge, _)| {
                    input
                        .pairs
                        .contains(&((*edge).clone(), *source, *target_row))
                })
                .map(|(edge, _)| (edge.clone(), *source, *target_row))
                .collect();
            let retain = contributors.len() == 1 && relation_is_injective_at(f, &contributors[0].0);
            layout.relations.insert(
                (target_edge.clone(), *source, *target_row),
                selected_relation_layout(input_layout, &contributors, retain),
            );
        }
        layout
    }

    fn delta_layout(
        input: &Table,
        input_layout: &GraphLayout,
        output: &Table,
        provenance: &rel::DeltaProvenance,
        f: &RelFunctor,
    ) -> GraphLayout {
        let source = GraphSide::kept(f);
        let mut layout = GraphLayout {
            reserved_node_ids: input_layout.reserved_node_ids.clone(),
            reserved_edge_ids: input_layout.reserved_edge_ids.clone(),
            ..GraphLayout::default()
        };
        for (row, (source_table, original)) in provenance {
            if let Some(node) = input_layout.table_nodes.get(original) {
                layout.table_nodes.insert(*row, node.clone());
            }
            let Some((_, cells)) = output.rows.get(row) else {
                continue;
            };
            for (column, value) in cells {
                let Some(source_arrow) = source
                    .columns
                    .get(source_table)
                    .and_then(|columns| columns.get(column))
                else {
                    continue;
                };
                let Some(target_column) = f.columns.get(&(source_table.clone(), column.clone()))
                else {
                    continue;
                };
                let Some((target_table, _)) = input.rows.get(original) else {
                    continue;
                };
                let Some(target_arrow) = f
                    .tgt
                    .columns
                    .get(target_table)
                    .and_then(|columns| columns.get(target_column))
                else {
                    continue;
                };
                let candidate = input_layout
                    .cells
                    .get(&(*original, target_column.clone()))
                    .and_then(|cell| move_back(cell, source_arrow, f))
                    .filter(|cell| cell_fits(cell, value, source_arrow, &source));
                if let Some(cell) = candidate {
                    let _ = target_arrow;
                    layout.cells.insert((*row, column.clone()), cell);
                }
            }
        }
        for (source_edge, from, to) in &output.pairs {
            let contributors = provenance
                .get(from)
                .zip(provenance.get(to))
                .and_then(|((_, original_from), (_, original_to))| {
                    f.relations.get(source_edge).map(|target_edge| {
                        vec![(target_edge.clone(), *original_from, *original_to)]
                    })
                })
                .unwrap_or_default();
            layout.relations.insert(
                (source_edge.clone(), *from, *to),
                selected_relation_layout(
                    input_layout,
                    &contributors,
                    relation_is_injective_at(f, source_edge),
                ),
            );
        }
        layout
    }

    fn pi_layout(
        input: &Table,
        input_layout: &GraphLayout,
        output: &Table,
        provenance: &rel::PiProvenance,
        f: &RelFunctor,
    ) -> GraphLayout {
        let target = GraphSide::whole(&f.tgt);
        let mut layout = GraphLayout {
            reserved_node_ids: input_layout.reserved_node_ids.clone(),
            reserved_edge_ids: input_layout.reserved_edge_ids.clone(),
            ..GraphLayout::default()
        };
        for (row, (target_table, tuple)) in provenance {
            let fibre = f.fibre(target_table);
            if let [only] = tuple.as_slice()
                && let Some(node) = input_layout.table_nodes.get(only)
            {
                layout.table_nodes.insert(*row, node.clone());
            }
            let Some((_, cells)) = output.rows.get(row) else {
                continue;
            };
            for (target_column, value) in cells {
                let Some(arrow) = target
                    .columns
                    .get(target_table)
                    .and_then(|columns| columns.get(target_column))
                else {
                    continue;
                };
                let mut contributors = Vec::new();
                for (source_table, source_row) in fibre.iter().zip(tuple) {
                    for ((table, source_column), image) in &f.columns {
                        if table == source_table && image == target_column {
                            contributors.push((
                                source_table.clone(),
                                *source_row,
                                source_column.clone(),
                            ));
                        }
                    }
                }
                contributors.sort();
                let candidate = contributors
                    .into_iter()
                    .find_map(|(_, source_row, column)| {
                        input_layout
                            .cells
                            .get(&(source_row, column))
                            .and_then(|cell| move_forward(cell, arrow, f))
                            .filter(|cell| cell_fits(cell, value, arrow, &target))
                    });
                if let Some(cell) = candidate {
                    layout.cells.insert((*row, target_column.clone()), cell);
                }
            }
        }
        for (target_edge, from, to) in &output.pairs {
            let contributors: Vec<(SEdge, u32, u32)> = f
                .relations
                .iter()
                .filter(|(_, image)| *image == target_edge)
                .filter_map(|(source_edge, _)| {
                    let (_, from_tuple) = provenance.get(from)?;
                    let (_, to_tuple) = provenance.get(to)?;
                    let from_fibre = f.fibre(&target_edge.src);
                    let to_fibre = f.fibre(&target_edge.tgt);
                    let source_row = from_fibre
                        .iter()
                        .position(|table| table == &source_edge.src)
                        .and_then(|index| from_tuple.get(index))?;
                    let target_row = to_fibre
                        .iter()
                        .position(|table| table == &source_edge.tgt)
                        .and_then(|index| to_tuple.get(index))?;
                    input
                        .pairs
                        .contains(&(source_edge.clone(), *source_row, *target_row))
                        .then(|| (source_edge.clone(), *source_row, *target_row))
                })
                .collect();
            let retain = contributors.len() == 1 && relation_is_injective_at(f, &contributors[0].0);
            layout.relations.insert(
                (target_edge.clone(), *from, *to),
                selected_relation_layout(input_layout, &contributors, retain),
            );
        }
        layout
    }

    type CellKey = (u32, String);
    type RelationKey = (SEdge, u32, u32);

    #[derive(Default)]
    struct GraphOrigins {
        rows: BTreeMap<u32, BTreeSet<u32>>,
        cells: BTreeMap<CellKey, BTreeSet<CellKey>>,
        relations: BTreeMap<RelationKey, BTreeSet<RelationKey>>,
    }

    fn sigma_origins(input: &Table, output: &Table, f: &RelFunctor) -> GraphOrigins {
        let mut origins = GraphOrigins::default();
        for (row, (_, cells)) in &output.rows {
            if input.rows.contains_key(row) {
                origins.rows.insert(*row, BTreeSet::from([*row]));
            }
            for target_column in cells.keys() {
                let contributors = sigma_slot_sources(input, *row, target_column, f)
                    .into_iter()
                    .collect();
                origins
                    .cells
                    .insert((*row, target_column.clone()), contributors);
            }
        }
        for (edge, source, target) in &output.pairs {
            let contributors = f
                .relations
                .iter()
                .filter(|(_, image)| *image == edge)
                .filter(|(source_edge, _)| {
                    input
                        .pairs
                        .contains(&((*source_edge).clone(), *source, *target))
                })
                .map(|(source_edge, _)| (source_edge.clone(), *source, *target))
                .collect();
            origins
                .relations
                .insert((edge.clone(), *source, *target), contributors);
        }
        origins
    }

    fn delta_origins(
        input: &Table,
        output: &Table,
        provenance: &rel::DeltaProvenance,
        f: &RelFunctor,
    ) -> GraphOrigins {
        let mut origins = GraphOrigins::default();
        for (row, (source_table, original)) in provenance {
            origins.rows.insert(*row, BTreeSet::from([*original]));
            if let Some((_, cells)) = output.rows.get(row) {
                for column in cells.keys() {
                    let contributors = f
                        .columns
                        .get(&(source_table.clone(), column.clone()))
                        .map(|target_column| BTreeSet::from([(*original, target_column.clone())]))
                        .unwrap_or_default();
                    origins.cells.insert((*row, column.clone()), contributors);
                }
            }
        }
        for (edge, source, target) in &output.pairs {
            let contributors = provenance
                .get(source)
                .zip(provenance.get(target))
                .and_then(|((_, original_source), (_, original_target))| {
                    f.relations.get(edge).map(|target_edge| {
                        BTreeSet::from([(target_edge.clone(), *original_source, *original_target)])
                    })
                })
                .unwrap_or_default();
            origins
                .relations
                .insert((edge.clone(), *source, *target), contributors);
        }
        let _ = input;
        origins
    }

    fn pi_origins(
        input: &Table,
        output: &Table,
        provenance: &rel::PiProvenance,
        f: &RelFunctor,
    ) -> GraphOrigins {
        let mut origins = GraphOrigins::default();
        for (row, (target_table, tuple)) in provenance {
            origins.rows.insert(*row, tuple.iter().copied().collect());
            let fibre = f.fibre(target_table);
            if let Some((_, cells)) = output.rows.get(row) {
                for target_column in cells.keys() {
                    let contributors = fibre
                        .iter()
                        .zip(tuple)
                        .flat_map(|(source_table, source_row)| {
                            f.columns
                                .iter()
                                .filter(move |((table, _), image)| {
                                    table == source_table && *image == target_column
                                })
                                .map(move |((_, column), _)| (*source_row, column.clone()))
                        })
                        .collect();
                    origins
                        .cells
                        .insert((*row, target_column.clone()), contributors);
                }
            }
        }
        for (edge, source, target) in &output.pairs {
            let contributors = f
                .relations
                .iter()
                .filter(|(_, image)| *image == edge)
                .filter_map(|(source_edge, _)| {
                    let (_, source_tuple) = provenance.get(source)?;
                    let (_, target_tuple) = provenance.get(target)?;
                    let source_fibre = f.fibre(&edge.src);
                    let target_fibre = f.fibre(&edge.tgt);
                    let source_row = source_fibre
                        .iter()
                        .position(|table| table == &source_edge.src)
                        .and_then(|index| source_tuple.get(index))?;
                    let target_row = target_fibre
                        .iter()
                        .position(|table| table == &source_edge.tgt)
                        .and_then(|index| target_tuple.get(index))?;
                    input
                        .pairs
                        .contains(&(source_edge.clone(), *source_row, *target_row))
                        .then(|| (source_edge.clone(), *source_row, *target_row))
                })
                .collect();
            origins
                .relations
                .insert((edge.clone(), *source, *target), contributors);
        }
        origins
    }

    fn contributor_maps(
        graph: &Graph,
        output_layout: &GraphLayout,
        input_layout: &GraphLayout,
        origins: &GraphOrigins,
    ) -> (BTreeMap<u32, BTreeSet<u32>>, BTreeMap<u32, BTreeSet<u32>>) {
        let mut nodes: BTreeMap<u32, BTreeSet<u32>> = graph
            .nodes
            .keys()
            .map(|node| (*node, BTreeSet::new()))
            .collect();
        let mut edges: BTreeMap<u32, BTreeSet<u32>> = graph
            .edges
            .keys()
            .map(|edge| (*edge, BTreeSet::new()))
            .collect();
        for (row, contributors) in &origins.rows {
            nodes.entry(*row).or_default().extend(contributors);
        }
        for (key, output_cell) in &output_layout.cells {
            let source_cells: Vec<&CellLayout> = origins
                .cells
                .get(key)
                .into_iter()
                .flatten()
                .filter_map(|source| input_layout.cells.get(source))
                .collect();
            let source_nodes: BTreeSet<u32> = source_cells
                .iter()
                .flat_map(|cell| cell.nodes.keys().copied())
                .collect();
            let source_edges: BTreeSet<u32> = source_cells
                .iter()
                .flat_map(|cell| {
                    cell.attribute
                        .id
                        .into_iter()
                        .chain(cell.edges.keys().copied())
                })
                .collect();
            for node in output_cell.nodes.keys() {
                nodes.entry(*node).or_default().extend(&source_nodes);
            }
            if let Some(attribute) = output_cell.attribute.id {
                edges.entry(attribute).or_default().extend(&source_edges);
            }
            for edge in output_cell.edges.keys() {
                edges.entry(*edge).or_default().extend(&source_edges);
            }
        }
        for (key, output_edge) in &output_layout.relations {
            let source_edges = origins
                .relations
                .get(key)
                .into_iter()
                .flatten()
                .filter_map(|source| input_layout.relations.get(source)?.id);
            if let Some(output) = output_edge.id {
                edges.entry(output).or_default().extend(source_edges);
            }
        }
        (nodes, edges)
    }

    /// Graph provenance for Sigma.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct GraphSigmaProvenance {
        /// Table Sigma provenance.
        pub table: rel::SigmaProvenance,
        /// Every output node with every contributing input node.
        pub nodes: BTreeMap<u32, BTreeSet<u32>>,
        /// Every output edge with every contributing input edge.
        pub edges: BTreeMap<u32, BTreeSet<u32>>,
    }

    /// Graph provenance for Delta.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct GraphDeltaProvenance {
        /// Table Delta provenance.
        pub table: rel::DeltaProvenance,
        /// Every output node with every contributing input node.
        pub nodes: BTreeMap<u32, BTreeSet<u32>>,
        /// Every output edge with every contributing input edge.
        pub edges: BTreeMap<u32, BTreeSet<u32>>,
    }

    /// Graph provenance for Pi.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct GraphPiProvenance {
        /// Table Pi provenance.
        pub table: rel::PiProvenance,
        /// Every output node with every contributing input node.
        pub nodes: BTreeMap<u32, BTreeSet<u32>>,
        /// Every output edge with every contributing input edge.
        pub edges: BTreeMap<u32, BTreeSet<u32>>,
    }

    fn checked_functor(
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
    ) -> Result<RelFunctor, GraphKanError> {
        let source = rel::classify(
            source_schema,
            &source_protocol.value,
            &source_protocol.non_value,
        )
        .map_err(|error| GraphKanError::RelSchema(Box::new(error)))?;
        let target = rel::classify(
            target_schema,
            &target_protocol.value,
            &target_protocol.non_value,
        )
        .map_err(|error| GraphKanError::RelSchema(Box::new(error)))?;
        rel::entity_functor(&source, &target, migration)
            .and_then(rel::EntityFunctor::into_kan)
            .map_err(|error| GraphKanError::Functor(Box::new(error)))
    }

    /// Graph Sigma for a migration between strictly classified schemas.
    ///
    /// # Errors
    ///
    /// Source classification, target classification, functor construction,
    /// encoding, table Sigma, then decoding, preserving the first reason.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_sigma(
        graph: &Graph,
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
    ) -> Result<Graph, GraphKanError> {
        g_sigma_with_provenance(
            graph,
            source_schema,
            source_protocol,
            target_schema,
            target_protocol,
            migration,
        )
        .map(|(result, _)| result)
    }

    /// Schema-taking graph Sigma with graph and table provenance.
    ///
    /// # Errors
    ///
    /// As [`g_sigma`].
    pub fn g_sigma_with_provenance(
        graph: &Graph,
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
    ) -> Result<(Graph, GraphSigmaProvenance), GraphKanError> {
        let functor = checked_functor(
            source_schema,
            source_protocol,
            target_schema,
            target_protocol,
            migration,
        )?;
        g_sigma_along_with_provenance(&functor, graph)
    }

    /// Graph Delta for a migration between strictly classified schemas.
    ///
    /// # Errors
    ///
    /// Source classification, target classification, functor construction,
    /// encoding, table Delta, then decoding, preserving the first reason.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_delta(
        graph: &Graph,
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
    ) -> Result<Graph, GraphKanError> {
        g_delta_with_provenance(
            graph,
            source_schema,
            source_protocol,
            target_schema,
            target_protocol,
            migration,
        )
        .map(|(result, _)| result)
    }

    /// Schema-taking graph Delta with graph and table provenance.
    ///
    /// # Errors
    ///
    /// As [`g_delta`].
    pub fn g_delta_with_provenance(
        graph: &Graph,
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
    ) -> Result<(Graph, GraphDeltaProvenance), GraphKanError> {
        let functor = checked_functor(
            source_schema,
            source_protocol,
            target_schema,
            target_protocol,
            migration,
        )?;
        g_delta_along_with_provenance(&functor, graph)
    }

    /// Graph Pi for a migration between strictly classified schemas.
    ///
    /// # Errors
    ///
    /// Source classification, target classification, functor construction,
    /// encoding, table Pi, then decoding, preserving the first reason.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_pi(
        graph: &Graph,
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
        max_product_size: usize,
    ) -> Result<Graph, GraphKanError> {
        g_pi_with_provenance(
            graph,
            source_schema,
            source_protocol,
            target_schema,
            target_protocol,
            migration,
            max_product_size,
        )
        .map(|(result, _)| result)
    }

    /// Schema-taking graph Pi with graph and table provenance.
    ///
    /// # Errors
    ///
    /// As [`g_pi`].
    pub fn g_pi_with_provenance(
        graph: &Graph,
        source_schema: &crate::schema::Schema,
        source_protocol: &KindPartition,
        target_schema: &crate::schema::Schema,
        target_protocol: &KindPartition,
        migration: &crate::migration::Migration,
        max_product_size: usize,
    ) -> Result<(Graph, GraphPiProvenance), GraphKanError> {
        let functor = checked_functor(
            source_schema,
            source_protocol,
            target_schema,
            target_protocol,
            migration,
        )?;
        g_pi_along_with_provenance(&functor, graph, max_product_size)
    }

    /// Definition: Graph Delta is decode over the kept part of Delta of the encoding over
    /// the target.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`].
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_delta_along(f: &RelFunctor, h: &Graph) -> Result<Graph, GraphKanError> {
        g_delta_along_with_provenance(f, h).map(|(graph, _)| graph)
    }

    /// Graph Delta along a checked functor, with every graph contributor.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_delta_along_with_provenance(
        f: &RelFunctor,
        h: &Graph,
    ) -> Result<(Graph, GraphDeltaProvenance), GraphKanError> {
        let (t, input_layout) = encode_graph(h, &GraphSide::whole(&f.tgt))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (d, table) = rel::delta(f, &t).map_err(graph_adjunction_error)?;
        let origins = delta_origins(&t, &d, &table, f);
        let layout = delta_layout(&t, &input_layout, &d, &table, f);
        let side = GraphSide::kept(f);
        let graph = decode_table(&d, &side, &layout).map_err(graph_decode_error)?;
        let (_, output_layout) = encode_graph(&graph, &side)
            .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
        let (nodes, edges) = contributor_maps(&graph, &output_layout, &input_layout, &origins);
        Ok((
            graph,
            GraphDeltaProvenance {
                table,
                nodes,
                edges,
            },
        ))
    }

    /// Definition: Graph Sigma is decode over the target of Sigma of the encoding over the
    /// source.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`].
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_sigma_along(f: &RelFunctor, g: &Graph) -> Result<Graph, GraphKanError> {
        g_sigma_along_with_provenance(f, g).map(|(graph, _)| graph)
    }

    /// Graph Sigma along a checked functor, with every graph contributor.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_sigma_along_with_provenance(
        f: &RelFunctor,
        g: &Graph,
    ) -> Result<(Graph, GraphSigmaProvenance), GraphKanError> {
        let (t, input_layout) = encode_graph(g, &GraphSide::whole(&f.src))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (s, table) = rel::sigma(f, &t).map_err(graph_adjunction_error)?;
        let origins = sigma_origins(&t, &s, f);
        let layout = sigma_layout(&t, &input_layout, &s, f);
        let side = GraphSide::whole(&f.tgt);
        let graph = decode_table(&s, &side, &layout).map_err(graph_decode_error)?;
        let (_, output_layout) = encode_graph(&graph, &side)
            .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
        let (nodes, edges) = contributor_maps(&graph, &output_layout, &input_layout, &origins);
        Ok((
            graph,
            GraphSigmaProvenance {
                table,
                nodes,
                edges,
            },
        ))
    }

    /// Graph Sigma of an instance already over the kept source side.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    pub fn g_sigma_restricted(f: &RelFunctor, g: &Graph) -> Result<Graph, GraphKanError> {
        g_sigma_restricted_with_provenance(f, g).map(|(graph, _)| graph)
    }

    /// Restricted graph Sigma with every graph contributor.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    pub fn g_sigma_restricted_with_provenance(
        f: &RelFunctor,
        g: &Graph,
    ) -> Result<(Graph, GraphSigmaProvenance), GraphKanError> {
        let (t, input_layout) = encode_graph(g, &GraphSide::kept(f))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (s, table) = super::rel::sigma_restricted(f, &t).map_err(graph_adjunction_error)?;
        let origins = sigma_origins(&t, &s, f);
        let layout = sigma_layout(&t, &input_layout, &s, f);
        let side = GraphSide::whole(&f.tgt);
        let graph = decode_table(&s, &side, &layout).map_err(graph_decode_error)?;
        let (_, output_layout) = encode_graph(&graph, &side)
            .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
        let (nodes, edges) = contributor_maps(&graph, &output_layout, &input_layout, &origins);
        Ok((
            graph,
            GraphSigmaProvenance {
                table,
                nodes,
                edges,
            },
        ))
    }

    /// Definition: Graph Pi is decode over the target of Pi of the encoding over the source.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`].
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_pi_along(
        f: &RelFunctor,
        g: &Graph,
        max_product_size: usize,
    ) -> Result<Graph, GraphKanError> {
        g_pi_along_with_provenance(f, g, max_product_size).map(|(graph, _)| graph)
    }

    /// Graph Pi along a checked functor, with every graph contributor.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_pi_along_with_provenance(
        f: &RelFunctor,
        g: &Graph,
        max_product_size: usize,
    ) -> Result<(Graph, GraphPiProvenance), GraphKanError> {
        let (t, input_layout) = encode_graph(g, &GraphSide::whole(&f.src))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (p, table) = rel::pi(f, &t, max_product_size).map_err(graph_adjunction_error)?;
        let origins = pi_origins(&t, &p, &table, f);
        let layout = pi_layout(&t, &input_layout, &p, &table, f);
        let side = GraphSide::whole(&f.tgt);
        let graph = decode_table(&p, &side, &layout).map_err(graph_decode_error)?;
        let (_, output_layout) = encode_graph(&graph, &side)
            .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
        let (nodes, edges) = contributor_maps(&graph, &output_layout, &input_layout, &origins);
        Ok((
            graph,
            GraphPiProvenance {
                table,
                nodes,
                edges,
            },
        ))
    }

    /// Graph Pi of an instance already over the kept source side.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    pub fn g_pi_restricted(
        f: &RelFunctor,
        g: &Graph,
        max_product_size: usize,
    ) -> Result<Graph, GraphKanError> {
        g_pi_restricted_with_provenance(f, g, max_product_size).map(|(graph, _)| graph)
    }

    /// Restricted graph Pi with every graph contributor.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`] in encoding, table-extension, then decoding order.
    pub fn g_pi_restricted_with_provenance(
        f: &RelFunctor,
        g: &Graph,
        max_product_size: usize,
    ) -> Result<(Graph, GraphPiProvenance), GraphKanError> {
        let (t, input_layout) = encode_graph(g, &GraphSide::kept(f))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (p, table) =
            super::rel::pi_restricted(f, &t, max_product_size).map_err(graph_adjunction_error)?;
        let origins = pi_origins(&t, &p, &table, f);
        let layout = pi_layout(&t, &input_layout, &p, &table, f);
        let side = GraphSide::whole(&f.tgt);
        let graph = decode_table(&p, &side, &layout).map_err(graph_decode_error)?;
        let (_, output_layout) = encode_graph(&graph, &side)
            .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))?;
        let (nodes, edges) = contributor_maps(&graph, &output_layout, &input_layout, &origins);
        Ok((
            graph,
            GraphPiProvenance {
                table,
                nodes,
                edges,
            },
        ))
    }

    fn inferred_hom(
        domain: &Table,
        codomain: &Table,
        rows: BTreeMap<u32, u32>,
    ) -> Result<rel::Hom, GraphKanError> {
        let mut nulls: BTreeMap<u64, Value> = BTreeMap::new();
        for (source, (_, cells)) in &domain.rows {
            let target = rows.get(source).copied().ok_or_else(|| {
                GraphKanError::Hom(Box::new(rel::HomError::NodeNotMapped(*source)))
            })?;
            let Some((_, target_cells)) = codomain.rows.get(&target) else {
                return Err(GraphKanError::Hom(Box::new(
                    rel::HomError::ImageNodeMissing {
                        src: *source,
                        tgt: target,
                    },
                )));
            };
            for (column, value) in cells {
                let Some(image) = target_cells.get(column) else {
                    return Err(GraphKanError::Hom(Box::new(
                        rel::HomError::AttributeMismatch {
                            src: *source,
                            tgt: target,
                        },
                    )));
                };
                if let Value::LabeledNull(null) = value {
                    if nulls.get(null).is_some_and(|prior| {
                        crate::canonical::value_bytes(prior) != crate::canonical::value_bytes(image)
                    }) {
                        return Err(GraphKanError::Hom(Box::new(
                            rel::HomError::AttributeMismatch {
                                src: *source,
                                tgt: target,
                            },
                        )));
                    }
                    nulls.insert(*null, image.clone());
                }
            }
        }
        let hom = rel::Hom { rows, nulls };
        hom.check(domain, codomain)
            .map_err(|error| GraphKanError::Hom(Box::new(error)))?;
        Ok(hom)
    }

    fn encode_for_hom(graph: &Graph, side: &GraphSide) -> Result<Table, GraphKanError> {
        encode_graph(graph, side)
            .map(|(table, _)| table)
            .map_err(|error| GraphKanError::NotEncodable(Box::new(error)))
    }

    /// Unit of graph Sigma left adjoint to graph Delta on the kept source side.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-6`.
    pub fn g_unit(graph: &Graph, f: &RelFunctor) -> Result<GraphKanHom, GraphKanError> {
        let input = encode_for_hom(graph, &GraphSide::kept(f))?;
        let (sigma, _) = super::rel::sigma_restricted(f, &input).map_err(graph_adjunction_error)?;
        let (delta, copies) = rel::delta(f, &sigma).map_err(graph_adjunction_error)?;
        let rows = input
            .rows
            .iter()
            .filter_map(|(row, (table, _))| {
                copies
                    .iter()
                    .find(|(_, (source_table, original))| source_table == table && original == row)
                    .map(|(copy, _)| (*row, *copy))
            })
            .collect();
        inferred_hom(&input, &delta, rows).map(GraphKanHom::from_table)
    }

    /// Counit of graph Sigma left adjoint to graph Delta on the target side.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-6`.
    pub fn g_counit(graph: &Graph, f: &RelFunctor) -> Result<GraphKanHom, GraphKanError> {
        let target = encode_for_hom(graph, &GraphSide::whole(&f.tgt))?;
        let (delta, copies) = rel::delta(f, &target).map_err(graph_adjunction_error)?;
        let (sigma, _) = super::rel::sigma_restricted(f, &delta).map_err(graph_adjunction_error)?;
        let rows = sigma
            .rows
            .keys()
            .filter_map(|row| copies.get(row).map(|(_, original)| (*row, *original)))
            .collect();
        inferred_hom(&sigma, &target, rows).map(GraphKanHom::from_table)
    }

    /// Transpose `Hom(g_Sigma I, J)` to `Hom(I, g_Delta J)`.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-4`.
    pub fn g_transpose_left(
        hom: &GraphKanHom,
        source: &Graph,
        target: &Graph,
        f: &RelFunctor,
    ) -> Result<GraphKanHom, GraphKanError> {
        let source = encode_for_hom(source, &GraphSide::kept(f))?;
        let target = encode_for_hom(target, &GraphSide::whole(&f.tgt))?;
        let (sigma, _) =
            super::rel::sigma_restricted(f, &source).map_err(graph_adjunction_error)?;
        hom.as_table()
            .check(&sigma, &target)
            .map_err(|error| GraphKanError::Hom(Box::new(error)))?;
        let (delta, copies) = rel::delta(f, &target).map_err(graph_adjunction_error)?;
        let rows = source
            .rows
            .iter()
            .filter_map(|(row, (table, _))| {
                let target_row = hom.node_map.get(row)?;
                copies
                    .iter()
                    .find(|(_, (source_table, original))| {
                        source_table == table && original == target_row
                    })
                    .map(|(copy, _)| (*row, *copy))
            })
            .collect();
        inferred_hom(&source, &delta, rows).map(GraphKanHom::from_table)
    }

    /// Inverse transpose `Hom(I, g_Delta J)` to `Hom(g_Sigma I, J)`.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-4`.
    pub fn g_transpose_right(
        hom: &GraphKanHom,
        source: &Graph,
        target: &Graph,
        f: &RelFunctor,
    ) -> Result<GraphKanHom, GraphKanError> {
        let source = encode_for_hom(source, &GraphSide::kept(f))?;
        let target = encode_for_hom(target, &GraphSide::whole(&f.tgt))?;
        let (delta, copies) = rel::delta(f, &target).map_err(graph_adjunction_error)?;
        hom.as_table()
            .check(&source, &delta)
            .map_err(|error| GraphKanError::Hom(Box::new(error)))?;
        let (sigma, _) =
            super::rel::sigma_restricted(f, &source).map_err(graph_adjunction_error)?;
        let rows = sigma
            .rows
            .keys()
            .filter_map(|row| {
                hom.node_map
                    .get(row)
                    .and_then(|copy| copies.get(copy))
                    .map(|(_, original)| (*row, *original))
            })
            .collect();
        inferred_hom(&sigma, &target, rows).map(GraphKanHom::from_table)
    }

    /// Unit of graph Delta left adjoint to graph Pi on the target side.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-5`, `L-kan-extensions-7`.
    pub fn g_pi_unit(
        graph: &Graph,
        f: &RelFunctor,
        max_product_size: usize,
    ) -> Result<GraphKanHom, GraphKanError> {
        let target = encode_for_hom(graph, &GraphSide::whole(&f.tgt))?;
        let (delta, copies) = rel::delta(f, &target).map_err(graph_adjunction_error)?;
        let (pi, tuples) = super::rel::pi_restricted(f, &delta, max_product_size)
            .map_err(graph_adjunction_error)?;
        let rows = target
            .rows
            .iter()
            .filter_map(|(row, (table, _))| {
                let fibre = f.fibre(table);
                let tuple = fibre
                    .iter()
                    .map(|source_table| {
                        copies
                            .iter()
                            .find(|(_, (candidate, original))| {
                                candidate == source_table && original == row
                            })
                            .map(|(copy, _)| *copy)
                    })
                    .collect::<Option<Vec<_>>>()?;
                tuples
                    .iter()
                    .find(|(_, (candidate, components))| candidate == table && components == &tuple)
                    .map(|(product, _)| (*row, *product))
            })
            .collect();
        inferred_hom(&target, &pi, rows).map(GraphKanHom::from_table)
    }

    /// Counit of graph Delta left adjoint to graph Pi on the kept source side.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-5`, `L-kan-extensions-7`.
    pub fn g_pi_counit(
        graph: &Graph,
        f: &RelFunctor,
        max_product_size: usize,
    ) -> Result<GraphKanHom, GraphKanError> {
        let source = encode_for_hom(graph, &GraphSide::kept(f))?;
        let (pi, tuples) = super::rel::pi_restricted(f, &source, max_product_size)
            .map_err(graph_adjunction_error)?;
        let (delta, copies) = rel::delta(f, &pi).map_err(graph_adjunction_error)?;
        let rows = delta
            .rows
            .iter()
            .filter_map(|(copy, (source_table, _))| {
                let (_, product) = copies.get(copy)?;
                let (target_table, tuple) = tuples.get(product)?;
                let index = f
                    .fibre(target_table)
                    .iter()
                    .position(|table| table == source_table)?;
                tuple.get(index).map(|source_row| (*copy, *source_row))
            })
            .collect();
        inferred_hom(&delta, &source, rows).map(GraphKanHom::from_table)
    }

    /// Transpose `Hom(g_Delta J, I)` to `Hom(J, g_Pi I)`.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-5`.
    pub fn g_pi_transpose_left(
        hom: &GraphKanHom,
        target: &Graph,
        source: &Graph,
        f: &RelFunctor,
        max_product_size: usize,
    ) -> Result<GraphKanHom, GraphKanError> {
        let target = encode_for_hom(target, &GraphSide::whole(&f.tgt))?;
        let source = encode_for_hom(source, &GraphSide::kept(f))?;
        let (delta, copies) = rel::delta(f, &target).map_err(graph_adjunction_error)?;
        hom.as_table()
            .check(&delta, &source)
            .map_err(|error| GraphKanError::Hom(Box::new(error)))?;
        let (pi, tuples) = super::rel::pi_restricted(f, &source, max_product_size)
            .map_err(graph_adjunction_error)?;
        let rows = target
            .rows
            .iter()
            .filter_map(|(row, (target_table, _))| {
                let tuple = f
                    .fibre(target_table)
                    .iter()
                    .map(|source_table| {
                        copies
                            .iter()
                            .find(|(_, (candidate, original))| {
                                candidate == source_table && original == row
                            })
                            .and_then(|(copy, _)| hom.node_map.get(copy))
                            .copied()
                    })
                    .collect::<Option<Vec<_>>>()?;
                tuples
                    .iter()
                    .find(|(_, (candidate, components))| {
                        candidate == target_table && components == &tuple
                    })
                    .map(|(product, _)| (*row, *product))
            })
            .collect();
        inferred_hom(&target, &pi, rows).map(GraphKanHom::from_table)
    }

    /// Inverse transpose `Hom(J, g_Pi I)` to `Hom(g_Delta J, I)`.
    ///
    /// # Errors
    ///
    /// The first encoding, table-extension or homomorphism error.
    ///
    /// Book anchors: `L-kan-extensions-5`.
    pub fn g_pi_transpose_right(
        hom: &GraphKanHom,
        target: &Graph,
        source: &Graph,
        f: &RelFunctor,
        max_product_size: usize,
    ) -> Result<GraphKanHom, GraphKanError> {
        let target = encode_for_hom(target, &GraphSide::whole(&f.tgt))?;
        let source = encode_for_hom(source, &GraphSide::kept(f))?;
        let (pi, tuples) = super::rel::pi_restricted(f, &source, max_product_size)
            .map_err(graph_adjunction_error)?;
        hom.as_table()
            .check(&target, &pi)
            .map_err(|error| GraphKanError::Hom(Box::new(error)))?;
        let (delta, copies) = rel::delta(f, &target).map_err(graph_adjunction_error)?;
        let rows = delta
            .rows
            .iter()
            .filter_map(|(copy, (source_table, _))| {
                let (_, target_row) = copies.get(copy)?;
                let product = hom.node_map.get(target_row)?;
                let (target_table, tuple) = tuples.get(product)?;
                let index = f
                    .fibre(target_table)
                    .iter()
                    .position(|table| table == source_table)?;
                tuple.get(index).map(|source_row| (*copy, *source_row))
            })
            .collect();
        inferred_hom(&delta, &source, rows).map(GraphKanHom::from_table)
    }

    /// Why a tree Kan extension refuses.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TreeKanError {
        /// The input tree has no graph form.
        Encode(TreeEncodeError),
        /// Two source vertices (kept tables, or the value vertices of kept
        /// columns) share an image: the pullback would duplicate a node into
        /// several anchors and leave the trees.
        NonInjectiveVertexMap {
            /// The lesser source vertex sharing the image.
            first: Name,
            /// The greater source vertex sharing the image.
            second: Name,
            /// The shared target vertex.
            target: Name,
        },
        /// Two source edges (kept relations, or the attribute arrows of kept
        /// columns) share an image, so the edge relabelling has no inverse.
        NonInjectiveEdgeMap {
            /// The shared target edge.
            target: Box<SEdge>,
        },
        /// The graph extension refused.
        Graph(GraphKanError),
        /// The result has not exactly one node that is neither a fan node nor
        /// the target of an arc, so it has no root.
        RootCount {
            /// How many such nodes it has.
            count: usize,
        },
        /// The result, read from its root, has no tree form.
        NotATree(NotATree),
    }

    /// The roots of a graph read as a tree: the nodes, other than fan nodes,
    /// that are the target of no edge leaving a node other than a fan node.
    fn roots_of(g: &Graph) -> Vec<u32> {
        let fan = |n: &u32| {
            g.nodes
                .get(n)
                .is_some_and(|x| x.shape == NodeShape::HyperEdge)
        };
        let children: BTreeSet<u32> = g
            .edges
            .values()
            .filter(|e| !fan(&e.src))
            .map(|e| e.tgt)
            .collect();
        g.nodes
            .keys()
            .filter(|n| !fan(n) && !children.contains(n))
            .copied()
            .collect()
    }

    /// A graph read back as a tree from its one root.
    ///
    /// # Errors
    ///
    /// [`TreeKanError::RootCount`] unless the graph has exactly one root, else
    /// [`TreeKanError::NotATree`] when it has no tree form from that root.
    fn read_tree(g: &Graph) -> Result<Tree, TreeKanError> {
        match roots_of(g).as_slice() {
            [root] => {
                graph_to_tree(g, *root, &schema_parent_name(None)).map_err(TreeKanError::NotATree)
            }
            many => Err(TreeKanError::RootCount { count: many.len() }),
        }
    }

    /// The graph form of a tree, for the tree Kan extensions (no schema, so
    /// fan parent edges are unnamed).
    fn tree_graph(t: &Tree) -> Result<Graph, TreeKanError> {
        tree_to_graph(t, None)
            .map(|(g, _)| g)
            .map_err(TreeKanError::Encode)
    }

    /// Tree Sigma through the graph form.
    ///
    /// Definition: tree Sigma of a tree X is the tree whose graph form is
    /// graph Sigma of X's graph form: X is encoded as a graph (no schema),
    /// graph Sigma runs, and the result is read back as a tree from its one
    /// node that no arc reaches. On the tree-Kan domain (X's graph form in the
    /// encoding's image, the functor injective on tables, relations and
    /// columns with every target table, relation and column in its image) the
    /// result is a tree; outside it, a result that is no tree is a typed
    /// refusal.
    ///
    /// # Errors
    ///
    /// [`TreeKanError::Encode`] when X has no graph form,
    /// [`TreeKanError::Graph`] when graph Sigma refuses, and
    /// [`TreeKanError::RootCount`] or [`TreeKanError::NotATree`] when its
    /// result is no tree.
    ///
    /// Book anchors: `L-kan-extensions-10`.
    pub fn t_sigma(f: &RelFunctor, t: &Tree) -> Result<Tree, TreeKanError> {
        let g = tree_graph(t)?;
        let r = g_sigma_restricted(f, &g).map_err(TreeKanError::Graph)?;
        read_tree(&r)
    }

    /// Tree Pi through the graph form.
    ///
    /// Definition: tree Pi of a tree X is the tree whose graph form is graph
    /// Pi of X's graph form, read back from its one node that no arc reaches
    /// (on the tree-Kan domain of [`t_sigma`]). Pi keeps the id of every row
    /// whose target table has a one-table fibre, so along the identity the
    /// root keeps its id.
    ///
    /// # Errors
    ///
    /// As [`t_sigma`], with graph Pi in place of graph Sigma.
    ///
    /// Book anchors: `L-kan-extensions-10`.
    pub fn t_pi(f: &RelFunctor, t: &Tree, max_product_size: usize) -> Result<Tree, TreeKanError> {
        if let Some((first, second, target)) = tree_pi_vertex_collision(f) {
            return Err(TreeKanError::NonInjectiveVertexMap {
                first,
                second,
                target,
            });
        }
        let g = tree_graph(t)?;
        let r = g_pi_restricted(f, &g, max_product_size).map_err(TreeKanError::Graph)?;
        read_tree(&r)
    }

    /// The first collision of a map, in ascending key order: the earlier key,
    /// the later key and their shared image.
    fn first_collision<K: Clone + Ord, V: Clone + Ord>(
        pairs: impl IntoIterator<Item = (K, V)>,
    ) -> Option<(K, K, V)> {
        let mut seen: BTreeMap<V, K> = BTreeMap::new();
        let mut sorted: Vec<(K, V)> = pairs.into_iter().collect();
        sorted.sort();
        for (k, v) in sorted {
            match seen.get(&v) {
                Some(earlier) if *earlier != k => return Some((earlier.clone(), k, v)),
                _ => {
                    seen.insert(v, k);
                }
            }
        }
        None
    }

    fn tree_vertex_collision(f: &RelFunctor) -> Option<(Name, Name, Name)> {
        let arrow = |schema: &RelSchema, table: &str, column: &str| {
            schema
                .columns
                .get(table)
                .and_then(|columns| columns.get(column))
                .cloned()
        };
        let mut vertices: Vec<(Name, Name)> = f
            .tables
            .iter()
            .map(|(source, target)| (source.clone(), target.clone()))
            .collect();
        for ((source_table, source_column), target_column) in &f.columns {
            let Some(target_table) = f.tables.get(source_table) else {
                continue;
            };
            let (Some(source_arrow), Some(target_arrow)) = (
                arrow(&f.src, source_table, source_column),
                arrow(&f.tgt, target_table, target_column),
            ) else {
                continue;
            };
            vertices.push((source_arrow.tgt.clone(), target_arrow.tgt.clone()));
        }
        vertices.sort();
        vertices.dedup();
        first_collision(vertices)
    }

    /// The Pi-specific injectivity check ranges over every vertex in the
    /// restricted source closure.  In particular, it includes value vertices
    /// below a column root; checking only the table and column-root maps would
    /// miss a collision deeper in a structured value.
    fn tree_pi_vertex_collision(f: &RelFunctor) -> Option<(Name, Name, Name)> {
        let kept = GraphSide::kept(f);
        let mut vertices: Vec<(Name, Name)> = f
            .tables
            .iter()
            .map(|(source, target)| (source.clone(), target.clone()))
            .collect();
        vertices.extend(kept.value_vertices.into_iter().map(|source| {
            let target = f
                .value_vertices
                .get(&source)
                .cloned()
                .unwrap_or_else(|| source.clone());
            (source, target)
        }));
        vertices.sort();
        vertices.dedup();
        first_collision(vertices)
    }

    /// Tree Delta through the graph form, on injective maps only.
    ///
    /// Definition: tree Delta is the pullback of a tree along the migration,
    /// defined only when the vertex map and the edge map are injective, since a
    /// merging map would duplicate a node into several anchors and leave the
    /// trees. The vertex map here is the kept table map together with the map
    /// it induces on the value vertices of kept columns (a source column's
    /// arrow target to its image column's arrow target); the edge map is the
    /// kept relation map together with the attribute arrows of kept columns.
    /// On that domain the tree whose graph form is graph Delta of the target
    /// tree's graph form is returned, read back from its one node that no arc
    /// reaches.
    ///
    /// # Errors
    ///
    /// [`TreeKanError::NonInjectiveVertexMap`] and
    /// [`TreeKanError::NonInjectiveEdgeMap`] for a merging map, checked in
    /// that order; then as [`t_sigma`], with graph Delta in place of graph
    /// Sigma.
    ///
    /// Book anchors: `L-kan-extensions-10`.
    pub fn t_delta(f: &RelFunctor, t: &Tree) -> Result<Tree, TreeKanError> {
        let arrow = |schema: &RelSchema, table: &str, column: &str| -> Option<SEdge> {
            schema
                .columns
                .get(table)
                .and_then(|cs| cs.get(column))
                .cloned()
        };
        let mut edges: Vec<(SEdge, SEdge)> = f
            .relations
            .iter()
            .map(|(s, d)| (s.clone(), d.clone()))
            .collect();
        for ((s, c), tc) in &f.columns {
            let (Some(src), Some(image)) = (f.tables.get(s), arrow(&f.src, s, c)) else {
                continue;
            };
            let Some(tgt) = arrow(&f.tgt, src, tc) else {
                continue;
            };
            edges.push((image, tgt));
        }
        edges.sort();
        edges.dedup();
        if let Some((first, second, target)) = tree_vertex_collision(f) {
            return Err(TreeKanError::NonInjectiveVertexMap {
                first,
                second,
                target,
            });
        }
        if let Some((_, _, target)) = first_collision(edges) {
            return Err(TreeKanError::NonInjectiveEdgeMap {
                target: Box::new(target),
            });
        }
        let g = tree_graph(t)?;
        let r = g_delta_along(f, &g).map_err(TreeKanError::Graph)?;
        read_tree(&r)
    }
}

pub mod rel {
    //! Relation-ified schemas, the migration's entity functor, and the three
    //! Kan extensions on finite attributed presheaves.
    //!
    //! A schema's relation-ified category has one object per table and one per
    //! relation (an edge between tables) with its two projections; attribute
    //! arrows (edges from a table to a value vertex) are columns into a free,
    //! discrete value side. An instance is a presheaf on that category with
    //! cells: rows at tables, pairs at relations, and per row one value per
    //! column. Delta is precomposition; Sigma is the quotient of the disjoint
    //! union; Pi enumerates compatible families over the comma category.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::migration::Migration;
    use crate::model::{Name, SEdge, Table};
    use crate::schema::Schema;
    use crate::value::Value;

    /// A relation-ified schema.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct RelSchema {
        /// Tables.
        pub tables: BTreeSet<Name>,
        /// Relation objects: edges between two tables.
        pub relations: BTreeSet<SEdge>,
        /// Per table, column name to attribute arrow.
        pub columns: BTreeMap<Name, BTreeMap<String, SEdge>>,
        /// Value vertices.
        pub value_vertices: BTreeSet<Name>,
        /// Edges out of value vertices (inner structure of one cell).
        pub inner_edges: BTreeSet<SEdge>,
        /// The kind of every vertex.
        pub kinds: BTreeMap<Name, Name>,
    }

    /// Why a schema cannot be relation-ified.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RelSchemaError {
        /// A kind is both a value kind and a non-value kind.
        AmbiguousKind {
            /// The kind.
            kind: Name,
        },
        /// An undeclared vertex whose kind is in neither set.
        UnclassifiedKind {
            /// The vertex.
            vertex: Name,
            /// Its kind.
            kind: Name,
        },
        /// The value closure reaches a table.
        ValueReachesTable {
            /// The least offending edge.
            edge: SEdge,
        },
        /// Two attribute arrows of one table share a column name.
        DuplicateColumn {
            /// The table.
            table: Name,
            /// The column.
            column: String,
        },
    }

    /// Classify a schema (strict classification).
    ///
    /// Definition: with K the value kinds and N the non-value kinds, a kind in
    /// both is `AmbiguousKind` (the least such kind); the declared tables D are
    /// the vertices with a key and the holders and targets of references; an
    /// undeclared vertex with a kind in neither set is `UnclassifiedKind` (the
    /// least vertex); the value vertices are the vertices reachable along edges
    /// from the undeclared vertices of a value kind, and an edge of that closure
    /// reaching a declared table or a vertex of a non-value kind is
    /// `ValueReachesTable` (the least edge); the tables are D and the vertices
    /// of a non-value kind; relations are the edges between tables; attribute
    /// arrows are the edges from a table to a value vertex, named by the edge
    /// name else the target, a repeated name in one table being
    /// `DuplicateColumn`; edges out of value vertices are inner edges.
    ///
    /// # Errors
    ///
    /// The first of the errors above, in that order.
    ///
    /// Book anchors: `L-kan-extensions-12`.
    ///
    pub fn classify(
        schema: &Schema,
        value_kinds: &BTreeSet<Name>,
        non_value_kinds: &BTreeSet<Name>,
    ) -> Result<RelSchema, RelSchemaError> {
        if let Some(kind) = value_kinds.intersection(non_value_kinds).next() {
            return Err(RelSchemaError::AmbiguousKind { kind: kind.clone() });
        }
        let mut declared: BTreeSet<Name> = schema
            .keys
            .iter()
            .filter(|(_, k)| !k.is_empty())
            .map(|(v, _)| v.clone())
            .collect();
        for r in schema.references.values() {
            declared.insert(r.holder.clone());
            declared.insert(r.target_vertex.clone());
        }
        for (v, kind) in &schema.vertices {
            if !declared.contains(v)
                && !value_kinds.contains(kind)
                && !non_value_kinds.contains(kind)
            {
                return Err(RelSchemaError::UnclassifiedKind {
                    vertex: v.clone(),
                    kind: kind.clone(),
                });
            }
        }
        let is_table = |v: &str| {
            declared.contains(v)
                || schema
                    .vertices
                    .get(v)
                    .is_some_and(|k| non_value_kinds.contains(k))
        };
        let mut values: BTreeSet<Name> = schema
            .vertices
            .iter()
            .filter(|(v, k)| !declared.contains(*v) && value_kinds.contains(*k))
            .map(|(v, _)| v.clone())
            .collect();
        loop {
            let grow: Vec<Name> = schema
                .edges
                .iter()
                .filter(|e| {
                    values.contains(&e.src) && !values.contains(&e.tgt) && !is_table(&e.tgt)
                })
                .map(|e| e.tgt.clone())
                .collect();
            if grow.is_empty() {
                break;
            }
            values.extend(grow);
        }
        if let Some(bad) = schema
            .edges
            .iter()
            .find(|e| values.contains(&e.src) && is_table(&e.tgt))
        {
            return Err(RelSchemaError::ValueReachesTable { edge: bad.clone() });
        }
        let tables: BTreeSet<Name> = schema
            .vertices
            .keys()
            .filter(|v| is_table(v))
            .cloned()
            .collect();
        let mut columns: BTreeMap<Name, BTreeMap<String, SEdge>> = tables
            .iter()
            .map(|t| (t.clone(), BTreeMap::new()))
            .collect();
        let mut relations = BTreeSet::new();
        let mut inner_edges = BTreeSet::new();
        for e in &schema.edges {
            if tables.contains(&e.src) && tables.contains(&e.tgt) {
                relations.insert(e.clone());
            } else if tables.contains(&e.src) && values.contains(&e.tgt) {
                let cols = columns.entry(e.src.clone()).or_default();
                let name = e.column().to_owned();
                if cols.insert(name.clone(), e.clone()).is_some() {
                    return Err(RelSchemaError::DuplicateColumn {
                        table: e.src.clone(),
                        column: name,
                    });
                }
            } else if values.contains(&e.src) {
                inner_edges.insert(e.clone());
            }
        }
        Ok(RelSchema {
            tables,
            relations,
            columns,
            value_vertices: values,
            inner_edges,
            kinds: schema.vertices.clone(),
        })
    }

    /// The entity part of a migration, after steps 1--4a and before the
    /// value-structure check required by the Kan extensions.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct EntityFunctor {
        maps: RelFunctor,
        /// Every remap whose source is an inner value edge.
        pub inner_edges: BTreeMap<SEdge, SEdge>,
        /// Every remap whose source is a value vertex.
        pub value_vertices: BTreeMap<Name, Name>,
    }

    impl EntityFunctor {
        /// The table/entity maps, which do not inspect value structure.
        #[must_use]
        pub const fn table_part(&self) -> &RelFunctor {
            &self.maps
        }

        /// Run step 4b and produce the functor accepted by the Kan extensions.
        ///
        /// An inner edge keeps value structure precisely when its image is an
        /// inner target edge with the same kind and written name, and the kind
        /// of each endpoint of the image is the kind of the image, under the
        /// recorded vertex remap, of the corresponding endpoint of the edge
        /// (an endpoint without a remap entry keeps its own kind).
        ///
        /// # Errors
        ///
        /// [`FunctorError::ValueStructureRemap`] for the least bad inner edge.
        pub fn into_kan(mut self) -> Result<RelFunctor, FunctorError> {
            for (edge, image) in &self.inner_edges {
                // The kind an endpoint of the image must have: that of the
                // endpoint's remap image when it has one, else its own.
                let expected = |vertex: &str| {
                    self.value_vertices.get(vertex).map_or_else(
                        || self.maps.src.kinds.get(vertex),
                        |remapped| self.maps.tgt.kinds.get(remapped),
                    )
                };
                let keeps_structure = self.maps.tgt.inner_edges.contains(image)
                    && image.name == edge.name
                    && image.kind == edge.kind
                    && self.maps.tgt.kinds.get(&image.src) == expected(&edge.src)
                    && self.maps.tgt.kinds.get(&image.tgt) == expected(&edge.tgt);
                if !keeps_structure {
                    return Err(FunctorError::ValueStructureRemap {
                        edge: Box::new(edge.clone()),
                    });
                }
            }
            self.maps.inner_edges = self.inner_edges;
            self.maps.value_vertices = self.value_vertices;
            Ok(self.maps)
        }
    }

    /// A functor between relation-ified schemas, restricted to its kept part
    /// and accepted by the step-4b value-structure check.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RelFunctor {
        /// The source schema.
        pub src: RelSchema,
        /// The target schema.
        pub tgt: RelSchema,
        /// Kept source table to target table.
        pub tables: BTreeMap<Name, Name>,
        /// Kept source relation to target relation.
        pub relations: BTreeMap<SEdge, SEdge>,
        /// Kept `(source table, column)` to target column of the image table.
        pub columns: BTreeMap<(Name, String), String>,
        /// Remap entries on inner value edges.
        pub inner_edges: BTreeMap<SEdge, SEdge>,
        /// Remap entries on value vertices.
        pub value_vertices: BTreeMap<Name, Name>,
    }

    /// Why a migration has no entity functor.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum FunctorError {
        /// A kept table's image is not a target table.
        VertexImageNotATable {
            /// The source table.
            vertex: Name,
            /// Its image.
            image: Name,
        },
        /// A kept relation's image is not a target relation between the images.
        IncoherentEdgeImage {
            /// The source edge.
            edge: Box<SEdge>,
            /// Its image.
            image: Box<SEdge>,
        },
        /// An attribute arrow's image is not a target attribute arrow of the
        /// image table.
        IncoherentAttributeImage {
            /// The source arrow.
            edge: Box<SEdge>,
            /// Its image, when it has one.
            image: Option<Box<SEdge>>,
        },
        /// An inner-edge remap changes the value structure.
        ValueStructureRemap {
            /// The source inner edge.
            edge: Box<SEdge>,
        },
    }

    fn is_image<'a, K, V: PartialEq + 'a>(
        map: impl IntoIterator<Item = (K, &'a V)>,
        v: &V,
    ) -> bool {
        map.into_iter().any(|(_, w)| w == v)
    }

    /// Definition: The entity functor of a migration between two relation-ified schemas.
    ///
    /// Steps, each reporting its least offender: (1) a source table is kept iff
    /// `vertex_remap` maps it, or it is listed in `surviving_verts` and is no
    /// remap image; its image (the remap, else itself) must be a target table;
    /// (2) a relation between kept tables is kept iff `edge_remap` maps it, or
    /// it is listed in `surviving_edges` and is no remap image; its image must
    /// be a target relation between the endpoint images; (3) an attribute arrow
    /// of a kept table that `edge_remap` maps must go to a target attribute
    /// arrow from the image table (ending at the value vertex's remap when it
    /// has one); one listed in `surviving_edges` and no image must have a
    /// target arrow from the image table with its kind and name, ending at its
    /// value vertex's image; an unmentioned column is dropped; (4a) every edge
    /// remap is checked not to cross the relation/attribute divide, and remaps
    /// of inner value edges and value vertices are recorded. Step 4b is
    /// [`EntityFunctor::into_kan`], not part of this function.
    ///
    /// # Errors
    ///
    /// The first failing step's [`FunctorError`].
    ///
    /// Book anchors: `L-kan-extensions-11`.
    ///
    pub fn entity_functor(
        src: &RelSchema,
        tgt: &RelSchema,
        m: &Migration,
    ) -> Result<EntityFunctor, FunctorError> {
        let mut tables = BTreeMap::new();
        for s in &src.tables {
            let kept = m.vertex_remap.contains_key(s)
                || (m.surviving_verts.contains(s) && !is_image(&m.vertex_remap, s));
            if !kept {
                continue;
            }
            let image = m.vertex_image(s).to_owned();
            if !tgt.tables.contains(&image) {
                return Err(FunctorError::VertexImageNotATable {
                    vertex: s.clone(),
                    image,
                });
            }
            tables.insert(s.clone(), image);
        }
        let kept_edge = |e: &SEdge| {
            m.edge_remap.contains_key(e)
                || (m.surviving_edges.contains(e) && !is_image(&m.edge_remap, e))
        };
        let mut relations = BTreeMap::new();
        for e in src
            .relations
            .iter()
            .filter(|e| tables.contains_key(&e.src) && tables.contains_key(&e.tgt))
        {
            if !kept_edge(e) {
                continue;
            }
            let image = m.edge_remap.get(e).cloned().unwrap_or_else(|| SEdge {
                src: tables[&e.src].clone(),
                tgt: tables[&e.tgt].clone(),
                kind: e.kind.clone(),
                name: e.name.clone(),
            });
            let coherent = tgt.relations.contains(&image)
                && image.src == tables[&e.src]
                && image.tgt == tables[&e.tgt];
            if !coherent {
                return Err(FunctorError::IncoherentEdgeImage {
                    edge: Box::new(e.clone()),
                    image: Box::new(image),
                });
            }
            relations.insert(e.clone(), image);
        }
        let columns = functor_columns(src, tgt, m, &tables)?;
        let mut inner_edges = BTreeMap::new();
        for (edge, image) in &m.edge_remap {
            if src.relations.contains(edge) && tgt.is_column(image) {
                return Err(FunctorError::IncoherentEdgeImage {
                    edge: Box::new(edge.clone()),
                    image: Box::new(image.clone()),
                });
            }
            if src.is_column(edge) && tgt.relations.contains(image) {
                return Err(FunctorError::IncoherentAttributeImage {
                    edge: Box::new(edge.clone()),
                    image: Some(Box::new(image.clone())),
                });
            }
            if src.inner_edges.contains(edge) {
                inner_edges.insert(edge.clone(), image.clone());
            }
        }
        let value_vertices = m
            .vertex_remap
            .iter()
            .filter(|(vertex, _)| src.value_vertices.contains(*vertex))
            .map(|(vertex, image)| (vertex.clone(), image.clone()))
            .collect();
        Ok(EntityFunctor {
            maps: RelFunctor {
                src: src.clone(),
                tgt: tgt.clone(),
                tables,
                relations,
                columns,
                inner_edges: BTreeMap::new(),
                value_vertices: BTreeMap::new(),
            },
            inner_edges,
            value_vertices,
        })
    }

    fn functor_columns(
        src: &RelSchema,
        tgt: &RelSchema,
        m: &Migration,
        tables: &BTreeMap<Name, Name>,
    ) -> Result<BTreeMap<(Name, String), String>, FunctorError> {
        let mut out = BTreeMap::new();
        let mut arrows: Vec<(&Name, &String, &SEdge)> = Vec::new();
        for (s, cols) in &src.columns {
            if tables.contains_key(s) {
                arrows.extend(cols.iter().map(|(c, a)| (s, c, a)));
            }
        }
        arrows.sort_by(|x, y| x.2.cmp(y.2));
        for (s, c, a) in arrows {
            let image_table = &tables[s];
            let target_cols = tgt.columns.get(image_table);
            let target_of = |e: &SEdge| {
                target_cols.and_then(|cols| {
                    cols.iter()
                        .find(|(_, arrow)| *arrow == e)
                        .map(|(n, _)| n.clone())
                })
            };
            if let Some(image) = m.edge_remap.get(a) {
                let end_ok = m.vertex_remap.get(&a.tgt).is_none_or(|v| *v == image.tgt);
                match target_of(image) {
                    Some(col) if end_ok && image.src == *image_table => {
                        out.insert((s.clone(), c.clone()), col);
                    }
                    _ => {
                        return Err(FunctorError::IncoherentAttributeImage {
                            edge: Box::new(a.clone()),
                            image: Some(Box::new(image.clone())),
                        });
                    }
                }
            } else if m.surviving_edges.contains(a) && !is_image(&m.edge_remap, a) {
                let wanted = SEdge {
                    src: image_table.clone(),
                    tgt: m.vertex_image(&a.tgt).to_owned(),
                    kind: a.kind.clone(),
                    name: a.name.clone(),
                };
                match target_of(&wanted) {
                    Some(col) => {
                        out.insert((s.clone(), c.clone()), col);
                    }
                    None => {
                        return Err(FunctorError::IncoherentAttributeImage {
                            edge: Box::new(a.clone()),
                            image: None,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// A side: tables with their columns, and relations.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Side {
        /// Table to columns.
        pub tables: BTreeMap<Name, BTreeSet<String>>,
        /// Relations.
        pub relations: BTreeSet<SEdge>,
    }

    impl RelSchema {
        fn is_column(&self, edge: &SEdge) -> bool {
            self.columns
                .values()
                .any(|columns| columns.values().any(|candidate| candidate == edge))
        }

        /// The whole schema as a side.
        #[must_use]
        pub fn side(&self) -> Side {
            Side {
                tables: self
                    .tables
                    .iter()
                    .map(|t| {
                        (
                            t.clone(),
                            self.columns
                                .get(t)
                                .map(|c| c.keys().cloned().collect())
                                .unwrap_or_default(),
                        )
                    })
                    .collect(),
                relations: self.relations.clone(),
            }
        }
    }

    impl RelFunctor {
        /// The kept part S' as a side: kept tables with their kept columns, and
        /// kept relations.
        #[must_use]
        pub fn kept_side(&self) -> Side {
            let mut tables: BTreeMap<Name, BTreeSet<String>> = self
                .tables
                .keys()
                .map(|t| (t.clone(), BTreeSet::new()))
                .collect();
            for (s, c) in self.columns.keys() {
                tables.entry(s.clone()).or_default().insert(c.clone());
            }
            Side {
                tables,
                relations: self.relations.keys().cloned().collect(),
            }
        }

        /// The kept source tables mapped to the target table `d`, ascending.
        #[must_use]
        pub fn fibre(&self, d: &str) -> Vec<Name> {
            self.tables
                .iter()
                .filter(|(_, t)| *t == d)
                .map(|(s, _)| s.clone())
                .collect()
        }

        fn restricted_source(&self) -> RelSchema {
            let tables: BTreeSet<Name> = self.tables.keys().cloned().collect();
            let columns: BTreeMap<Name, BTreeMap<String, SEdge>> = tables
                .iter()
                .map(|table| {
                    let kept = self
                        .src
                        .columns
                        .get(table)
                        .into_iter()
                        .flatten()
                        .filter(|(column, _)| {
                            self.columns
                                .contains_key(&(table.clone(), (*column).clone()))
                        })
                        .map(|(column, edge)| (column.clone(), edge.clone()))
                        .collect();
                    (table.clone(), kept)
                })
                .collect();
            let mut value_vertices: BTreeSet<Name> = columns
                .values()
                .flat_map(|items| items.values().map(|edge| edge.tgt.clone()))
                .collect();
            loop {
                let found: Vec<Name> = self
                    .src
                    .inner_edges
                    .iter()
                    .filter(|edge| {
                        value_vertices.contains(&edge.src) && !value_vertices.contains(&edge.tgt)
                    })
                    .map(|edge| edge.tgt.clone())
                    .collect();
                if found.is_empty() {
                    break;
                }
                value_vertices.extend(found);
            }
            let inner_edges = self
                .src
                .inner_edges
                .iter()
                .filter(|edge| value_vertices.contains(&edge.src))
                .cloned()
                .collect();
            RelSchema {
                tables,
                relations: self.relations.keys().cloned().collect(),
                columns,
                value_vertices,
                inner_edges,
                kinds: self.src.kinds.clone(),
            }
        }
    }

    /// Why an instance does not conform to a side.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Conformance {
        /// A row at a table outside the side.
        TableOutside,
        /// A column of the row's table without a cell.
        MissingCell,
        /// A cell outside the row's table's columns.
        StrayCell,
        /// A labelled null below the top level of a cell.
        NestedLabeledNull,
        /// A pair at a relation outside the side.
        RelationOutside(Box<SEdge>),
        /// A pair whose rows are not at its relation's endpoint tables.
        IncoherentPair(Box<SEdge>),
    }

    /// The first violation of conformance.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct NonConforming {
        /// The row (for a pair, its source row).
        pub row: u32,
        /// The column, for a cell violation.
        pub column: Option<String>,
        /// The reason.
        pub reason: Conformance,
    }

    /// Check that an instance conforms to a side.
    ///
    /// Rows at its tables with exactly its columns and no nested labelled null, pairs at its relations
    /// between rows of the relation's endpoint tables. The first violation is the least row; within a
    /// row, the least column; a row's violation comes before a pair whose source row is not smaller.
    ///
    /// # Errors
    ///
    /// The first [`NonConforming`] violation.
    pub fn conform(inst: &Table, side: &Side) -> Result<(), Box<NonConforming>> {
        let mut first: Option<NonConforming> = None;
        for (id, (table, cells)) in &inst.rows {
            let v = row_violation(*id, table, cells, side);
            if v.is_some() {
                first = v;
                break;
            }
        }
        let pair = inst
            .pairs
            .iter()
            .filter_map(|(e, s, t)| {
                if !side.relations.contains(e) {
                    return Some((*s, Conformance::RelationOutside(Box::new(e.clone()))));
                }
                let at = |r: &u32| inst.rows.get(r).map(|(tb, _)| tb.clone());
                (at(s).as_deref() != Some(e.src.as_str())
                    || at(t).as_deref() != Some(e.tgt.as_str()))
                .then(|| (*s, Conformance::IncoherentPair(Box::new(e.clone()))))
            })
            .min_by_key(|(s, _)| *s);
        match (first, pair) {
            (Some(r), Some((s, _))) if r.row <= s => Err(Box::new(r)),
            (_, Some((row, reason))) => Err(Box::new(NonConforming {
                row,
                column: None,
                reason,
            })),
            (Some(r), None) => Err(Box::new(r)),
            (None, None) => Ok(()),
        }
    }

    fn row_violation(
        id: u32,
        table: &str,
        cells: &BTreeMap<String, Value>,
        side: &Side,
    ) -> Option<NonConforming> {
        let Some(columns) = side.tables.get(table) else {
            return Some(NonConforming {
                row: id,
                column: None,
                reason: Conformance::TableOutside,
            });
        };
        let missing = columns
            .iter()
            .find(|c| !cells.contains_key(*c))
            .map(|c| (c.clone(), Conformance::MissingCell));
        let stray = cells
            .keys()
            .find(|k| !columns.contains(*k))
            .map(|k| (k.clone(), Conformance::StrayCell));
        let keyed = match (missing, stray) {
            (Some(m), Some(s)) => Some(if s.0 < m.0 { s } else { m }),
            (m, s) => m.or(s),
        };
        if let Some((column, reason)) = keyed {
            return Some(NonConforming {
                row: id,
                column: Some(column),
                reason,
            });
        }
        cells
            .iter()
            .find(|(_, v)| v.has_nested_labeled_null())
            .map(|(c, _)| NonConforming {
                row: id,
                column: Some(c.clone()),
                reason: Conformance::NestedLabeledNull,
            })
    }

    /// The restriction of a conforming instance to the kept part: rows at kept
    /// tables with their kept cells, pairs at kept relations.
    #[must_use]
    pub fn restrict_to_kept(inst: &Table, f: &RelFunctor) -> Table {
        let side = f.kept_side();
        let rows = inst
            .rows
            .iter()
            .filter_map(|(id, (t, cells))| {
                let cols = side.tables.get(t)?;
                let kept = cells
                    .iter()
                    .filter(|(c, _)| cols.contains(*c))
                    .map(|(c, v)| (c.clone(), v.clone()))
                    .collect();
                Some((*id, (t.clone(), kept)))
            })
            .collect();
        let pairs = inst
            .pairs
            .iter()
            .filter(|(e, _, _)| side.relations.contains(e))
            .cloned()
            .collect();
        Table { rows, pairs }
    }

    /// Why a Kan extension refuses.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum KanError {
        /// The input does not conform.
        NonConforming(Box<NonConforming>),
        /// Two constants were equated in a Sigma cell class.
        SigmaConstantClash {
            /// The output row.
            row: u32,
            /// The target column.
            column: String,
            /// The class's constant before the joining equation.
            left: Box<Value>,
            /// The other constant.
            right: Box<Value>,
        },
        /// A target column is not the image of a kept column of its fibre.
        AttributeOutsideImage {
            /// The target table.
            table: Name,
            /// The column.
            column: String,
        },
        /// A fresh row id would pass `u32::MAX`.
        RowIdsExhausted,
        /// A fresh labelled null would pass `u64::MAX`.
        NullIdsExhausted,
        /// A Pi table-fibre product or an uncarried target relation product
        /// exceeds the configured bound.
        ProductSizeExceeded {
            /// The target table, or the source table of the target relation.
            vertex: Name,
            /// The exact product, saturated only when machine `usize` cannot
            /// represent it.
            actual: usize,
            /// The configured bound.
            limit: usize,
        },
    }

    /// The public table-adjunction refusal carried by graph Kan operations.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum AdjunctionError {
        /// The input is outside the required relational instance category.
        NonConforming(Box<NonConforming>),
        /// Sigma equated two distinct constants.
        SigmaConstantClash {
            /// Output row.
            row: u32,
            /// Target column.
            column: String,
            /// First constant.
            left: Box<Value>,
            /// Second constant.
            right: Box<Value>,
        },
        /// A fresh row id is unavailable.
        RowIdsExhausted,
        /// A fresh labelled-null id is unavailable.
        NullIdsExhausted,
        /// Pi has a target column outside the attribute image.
        AttributeOutsideImage {
            /// Target table.
            table: Name,
            /// Target column.
            column: String,
        },
        /// A bounded Pi product is too large.
        ProductSizeExceeded {
            /// Target table or relation-source table.
            vertex: Name,
            /// Exact saturated cardinality.
            actual: usize,
            /// Bound.
            limit: usize,
        },
        /// An internal table result was malformed before graph decoding.
        MalformedTable,
    }

    impl From<KanError> for AdjunctionError {
        fn from(error: KanError) -> Self {
            match error {
                KanError::NonConforming(reason) => Self::NonConforming(reason),
                KanError::SigmaConstantClash {
                    row,
                    column,
                    left,
                    right,
                } => Self::SigmaConstantClash {
                    row,
                    column,
                    left,
                    right,
                },
                KanError::RowIdsExhausted => Self::RowIdsExhausted,
                KanError::NullIdsExhausted => Self::NullIdsExhausted,
                KanError::AttributeOutsideImage { table, column } => {
                    Self::AttributeOutsideImage { table, column }
                }
                KanError::ProductSizeExceeded {
                    vertex,
                    actual,
                    limit,
                } => Self::ProductSizeExceeded {
                    vertex,
                    actual,
                    limit,
                },
            }
        }
    }

    /// Fresh ids above a set of used ids.
    ///
    /// Allocation starts one above the greatest id (0 for none) and proceeds
    /// upward. The type's greatest value is returned once when it is the
    /// checked successor; only the following allocation exhausts.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Fresh<T> {
        next: Option<T>,
    }

    impl Fresh<u32> {
        /// The fresh row ids above `used`.
        #[must_use]
        pub fn above(used: impl Iterator<Item = u32>) -> Self {
            Self {
                next: used.max().map_or(Some(0), |m| m.checked_add(1)),
            }
        }

        /// The next fresh id.
        pub fn take(&mut self) -> Option<u32> {
            let id = self.next?;
            self.next = id.checked_add(1);
            Some(id)
        }
    }

    impl Fresh<u64> {
        /// The fresh null ids above `used`.
        #[must_use]
        pub fn above_nulls(used: impl Iterator<Item = u64>) -> Self {
            Self {
                next: used.max().map_or(Some(0), |m| m.checked_add(1)),
            }
        }

        /// The next fresh null id.
        pub fn take_null(&mut self) -> Option<u64> {
            let id = self.next?;
            self.next = id.checked_add(1);
            Some(id)
        }
    }

    /// The provenance of a Delta row: the source table and target row copied.
    pub type DeltaProvenance = BTreeMap<u32, (Name, u32)>;

    /// Definition: Delta along `f` (precomposition).
    ///
    /// For each kept source table s and each row r of `j` at `F(s)`, a copy row at s holding, at each
    /// kept column c of s, r's cell at `F(s, c)`; for each kept relation e and each pair `(F(e), a, b)`
    /// of `j`, the pair `(e, copy of a at e.src, copy of b at e.tgt)`. Ids: copies are made in (source
    /// table, target row) order; the first copy of a row keeps its id, every other copy takes the next
    /// id above every id of `j`. No null is created.
    ///
    /// # Errors
    ///
    /// [`KanError::NonConforming`] when `j` does not conform to the target;
    /// [`KanError::RowIdsExhausted`] when a fresh id would pass `u32::MAX`.
    ///
    /// Book anchors: `L-kan-extensions-1`, `L-kan-extensions-4`, `L-kan-extensions-5`.
    pub fn delta(f: &RelFunctor, j: &Table) -> Result<(Table, DeltaProvenance), KanError> {
        conform(j, &f.tgt.side()).map_err(KanError::NonConforming)?;
        let mut fresh = Fresh::above(j.rows.keys().copied());
        let mut used: BTreeSet<u32> = BTreeSet::new();
        let mut copy: BTreeMap<(Name, u32), u32> = BTreeMap::new();
        let mut out = Table::default();
        let mut prov = DeltaProvenance::new();
        for (s, t) in &f.tables {
            for (r, (table, cells)) in &j.rows {
                if table != t {
                    continue;
                }
                let id = if used.insert(*r) {
                    *r
                } else {
                    fresh.take().ok_or(KanError::RowIdsExhausted)?
                };
                let kept: BTreeMap<String, Value> = f
                    .columns
                    .iter()
                    .filter(|((src, _), _)| src == s)
                    .filter_map(|((_, c), tc)| cells.get(tc).map(|v| (c.clone(), v.clone())))
                    .collect();
                out.rows.insert(id, (s.clone(), kept));
                copy.insert((s.clone(), *r), id);
                prov.insert(id, (s.clone(), *r));
            }
        }
        for (e, image) in &f.relations {
            for (pe, a, b) in &j.pairs {
                if pe == image
                    && let (Some(x), Some(y)) = (
                        copy.get(&(e.src.clone(), *a)),
                        copy.get(&(e.tgt.clone(), *b)),
                    )
                {
                    out.pairs.insert((e.clone(), *x, *y));
                }
            }
        }
        Ok((out, prov))
    }

    /// The provenance of Sigma: each fresh null with its `(row, column)`, and
    /// each input null that another value represents, with that value.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct SigmaProvenance {
        /// Fresh null id to the output cell it was made for.
        pub fresh: BTreeMap<u64, (u32, String)>,
        /// Input null id to the value representing its class, for each input
        /// null whose class the equations join with a constant or a lesser
        /// null.
        pub merged: BTreeMap<u64, Value>,
    }

    /// A term of Sigma's cell presentation.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum Term {
        Slot(u32, String),
        Null(u64),
        Const(Vec<u8>),
    }

    fn term_of(v: &Value) -> Term {
        match v {
            Value::LabeledNull(n) => Term::Null(*n),
            other => Term::Const(crate::canonical::value_bytes(other).unwrap_or_default()),
        }
    }

    fn max_null(t: &Table) -> Option<u64> {
        t.rows
            .values()
            .flat_map(|(_, c)| c.values())
            .filter_map(|v| {
                if let Value::LabeledNull(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .max()
    }

    /// Definition: Sigma along `f`: the quotient of the disjoint union.
    ///
    /// Rows: each row of the kept part keeps its id at the image of its table;
    /// pairs: the set of images `(F(e), a, b)`. Cells: each output slot
    /// `(r, β)` is equated with `r`'s value at every kept column α of r's
    /// source table with `F(α) = β` (ascending), and a slot with no preimage
    /// column is a fresh labelled null ("fresh null ids start at max(nulls of
    /// the input) + 1 (0 when none), in ascending (`RowId`, target column)
    /// order, checked": the input is the whole input, before its restriction
    /// to the kept part). The equations generate classes over slots, nulls and
    /// constants (constants by canonical bytes); processed in `(r, β, α)`
    /// order, the first equation that joins two classes holding different
    /// constants is `SigmaConstantClash`. A class is represented by its
    /// constant, else its least null. The input is checked against the whole
    /// source schema and then restricted to the kept part. The provenance
    /// records each fresh null with its cell, and in `merged` each null of the
    /// restricted input whose class is represented by another value (a
    /// constant, or a lesser null), with that value; a null that represents
    /// its own class is not recorded.
    ///
    /// # Errors
    ///
    /// [`KanError::NonConforming`], [`KanError::SigmaConstantClash`], or
    /// [`KanError::NullIdsExhausted`] when a fresh null would pass `u64::MAX`.
    ///
    /// Book anchors: `L-kan-extensions-2`, `L-kan-extensions-4`.
    pub fn sigma(f: &RelFunctor, i: &Table) -> Result<(Table, SigmaProvenance), KanError> {
        conform(i, &f.src.side()).map_err(KanError::NonConforming)?;
        let mut fresh = Fresh::above_nulls(max_null(i).into_iter());
        let i = restrict_to_kept(i, f);
        let mut prov = SigmaProvenance::default();
        // Component label of every term; merging relabels one component.
        let mut label: BTreeMap<Term, usize> = BTreeMap::new();
        let mut next_label = 0usize;
        let mut slots: Vec<(u32, String, Name)> = Vec::new();
        for (r, (s, cells)) in &i.rows {
            let t = &f.tables[s];
            for beta in f
                .tgt
                .columns
                .get(t)
                .map(|c| c.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
            {
                slots.push((*r, beta.clone(), t.clone()));
                let slot = Term::Slot(*r, beta.clone());
                label.insert(slot.clone(), next_label);
                next_label += 1;
                let preimages: Vec<&String> = f
                    .columns
                    .iter()
                    .filter(|((src, _), b)| src == s && **b == beta)
                    .map(|((_, a), _)| a)
                    .collect();
                if preimages.is_empty() {
                    let fresh_id = fresh.take_null().ok_or(KanError::NullIdsExhausted)?;
                    prov.fresh.insert(fresh_id, (*r, beta.clone()));
                    join(&mut label, &mut next_label, &slot, &Term::Null(fresh_id))
                        .map_err(|(l, rt)| clash(*r, &beta, &l, &rt))?;
                }
                for a in preimages {
                    let Some(v) = cells.get(a) else { continue };
                    join(&mut label, &mut next_label, &slot, &term_of(v))
                        .map_err(|(l, rt)| clash(*r, &beta, &l, &rt))?;
                }
            }
        }
        let value_of = |lbl: usize| -> Value {
            let members: Vec<&Term> = label
                .iter()
                .filter(|(_, l)| **l == lbl)
                .map(|(t, _)| t)
                .collect();
            members
                .iter()
                .find_map(|t| {
                    if let Term::Const(b) = t {
                        crate::canonical::decode_value(b)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    members.iter().find_map(|t| {
                        if let Term::Null(n) = t {
                            Some(Value::LabeledNull(*n))
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(Value::Null)
        };
        let mut out = Table::default();
        for (r, (s, _)) in &i.rows {
            out.rows.insert(*r, (f.tables[s].clone(), BTreeMap::new()));
        }
        for (r, beta, _) in &slots {
            let v = value_of(label[&Term::Slot(*r, beta.clone())]);
            if let Some(row) = out.rows.get_mut(r) {
                row.1.insert(beta.clone(), v);
            }
        }
        for (t, l) in &label {
            if let Term::Null(n) = t
                && !prov.fresh.contains_key(n)
            {
                let representative = value_of(*l);
                if representative != Value::LabeledNull(*n) {
                    prov.merged.insert(*n, representative);
                }
            }
        }
        for (e, a, b) in &i.pairs {
            out.pairs.insert((f.relations[e].clone(), *a, *b));
        }
        Ok((out, prov))
    }

    /// Definition: Sigma of an instance of the kept part `S'`.
    ///
    /// [`sigma`] along the same maps with the source replaced by the kept
    /// part: its kept tables with only their kept columns, its kept
    /// relations, and the value vertices and inner edges below the kept
    /// columns. The input is checked against `S'`, not `S`.
    ///
    /// # Errors
    ///
    /// As [`sigma`], conformance read against `S'`.
    ///
    /// Book anchors: `L-kan-extensions-2`, `L-kan-extensions-4`.
    pub fn sigma_restricted(
        f: &RelFunctor,
        i: &Table,
    ) -> Result<(Table, SigmaProvenance), KanError> {
        let mut restricted = f.clone();
        restricted.src = f.restricted_source();
        sigma(&restricted, i)
    }

    fn clash(row: u32, column: &str, left: &[u8], right: &[u8]) -> KanError {
        let dec = |b: &[u8]| crate::canonical::decode_value(b).unwrap_or(Value::Null);
        KanError::SigmaConstantClash {
            row,
            column: column.to_owned(),
            left: Box::new(dec(left)),
            right: Box::new(dec(right)),
        }
    }

    /// Join the classes of `a` and `b`; refuse when both hold a constant and the
    /// constants differ, returning `(a's constant, b's constant)`.
    fn join(
        label: &mut BTreeMap<Term, usize>,
        next: &mut usize,
        a: &Term,
        b: &Term,
    ) -> Result<(), (Vec<u8>, Vec<u8>)> {
        for t in [a, b] {
            if !label.contains_key(t) {
                label.insert(t.clone(), *next);
                *next += 1;
            }
        }
        let (la, lb) = (label[a], label[b]);
        if la == lb {
            return Ok(());
        }
        let constant = |l: usize, label: &BTreeMap<Term, usize>| {
            label.iter().find_map(|(t, x)| {
                if *x == l {
                    if let Term::Const(c) = t {
                        Some(c.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        };
        if let (Some(ca), Some(cb)) = (constant(la, label), constant(lb, label)) {
            return Err((ca, cb));
        }
        for v in label.values_mut() {
            if *v == lb {
                *v = la;
            }
        }
        Ok(())
    }

    /// The provenance of Pi: each output row with its target table and its
    /// tuple of component rows, one per fibre table in ascending order.
    pub type PiProvenance = BTreeMap<u32, (Name, Vec<u32>)>;

    /// Whether every column of every target table is the image of a kept column
    /// of its fibre; the least `(table, column)` that is not, otherwise.
    ///
    /// # Errors
    ///
    /// [`KanError::AttributeOutsideImage`] naming the least such column.
    pub fn attribute_surjective(f: &RelFunctor) -> Result<(), KanError> {
        for (t, cols) in &f.tgt.columns {
            for c in cols.keys() {
                let covered = f
                    .columns
                    .iter()
                    .any(|((s, _), tc)| tc == c && f.tables.get(s) == Some(t));
                if !covered {
                    return Err(KanError::AttributeOutsideImage {
                        table: t.clone(),
                        column: c.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Every tuple choosing one row of `inst` at each table of `fibre`, in
    /// lexicographic order of the chosen row ids.
    fn tuples(inst: &Table, fibre: &[Name]) -> Vec<Vec<u32>> {
        let mut out: Vec<Vec<u32>> = vec![Vec::new()];
        for s in fibre {
            let rows: Vec<u32> = inst
                .rows
                .iter()
                .filter(|(_, (t, _))| t == s)
                .map(|(id, _)| *id)
                .collect();
            out = out
                .into_iter()
                .flat_map(|prefix| {
                    rows.iter().map(move |r| {
                        let mut p = prefix.clone();
                        p.push(*r);
                        p
                    })
                })
                .collect();
        }
        out
    }

    /// Definition: Pi along `f`: compatible families over the comma categories.
    ///
    /// For a target table d the comma category under d holds the kept source
    /// tables over d and no arrow between them, so a family is a tuple of one
    /// row per fibre table; the attributed limit keeps the tuples whose values
    /// at the preimages of each column of d are equal (by canonical bytes),
    /// that value being the cell (a table outside the image has one empty
    /// tuple). For a target relation ρ the families are pairs `(u, v)` of
    /// rows over `ρ.src` and `ρ.tgt` with `(u at x.src, v at x.tgt)` a pair of
    /// the input at every kept relation x carried to ρ (every pair when there
    /// is none). Requires attribute surjectivity; the input is checked against
    /// the whole source schema and restricted to the kept part.
    ///
    /// Row ids ("Fresh `RowIds` come from `max(RowId in input) + 1` (0 for an
    /// empty input), checked, in a fixed order"): a row of a target table
    /// whose fibre is one table keeps the id of the row it projects to, so Pi
    /// along the identity returns its input exactly; every other row takes
    /// the next fresh id, the target tables in the image first, ordered by the
    /// least table of their fibre, each table's tuples ascending, then the
    /// tables outside the image by name.
    ///
    /// # Errors
    ///
    /// [`KanError::AttributeOutsideImage`] or [`KanError::NonConforming`];
    /// [`KanError::ProductSizeExceeded`] before enumerating a product of two
    /// or more table fibres or the pair product of an uncarried relation; or
    /// [`KanError::RowIdsExhausted`] when a fresh id would pass `u32::MAX`.
    ///
    /// Book anchors: `L-kan-extensions-3`, `L-kan-extensions-5`.
    pub fn pi(
        f: &RelFunctor,
        i: &Table,
        max_product_size: usize,
    ) -> Result<(Table, PiProvenance), KanError> {
        attribute_surjective(f)?;
        conform(i, &f.src.side()).map_err(KanError::NonConforming)?;
        let mut fresh = Fresh::above(i.rows.keys().copied());
        let i = restrict_to_kept(i, f);
        let mut out = Table::default();
        let mut prov = PiProvenance::new();
        let mut ids: BTreeMap<(Name, Vec<u32>), u32> = BTreeMap::new();
        let mut order: Vec<(Option<Name>, Name)> = f
            .tgt
            .tables
            .iter()
            .map(|d| (f.fibre(d).first().cloned(), d.clone()))
            .collect();
        order.sort_by(|(fa, da), (fb, db)| match (fa, fb) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => da.cmp(db),
        });
        for (_, d) in order {
            let fibre = f.fibre(&d);
            if fibre.len() > 1 {
                let actual = fibre
                    .iter()
                    .try_fold(1usize, |product, table| {
                        product.checked_mul(
                            i.rows
                                .values()
                                .filter(|(candidate, _)| candidate == table)
                                .count(),
                        )
                    })
                    .unwrap_or(usize::MAX);
                if actual > max_product_size {
                    return Err(KanError::ProductSizeExceeded {
                        vertex: d,
                        actual,
                        limit: max_product_size,
                    });
                }
            }
            for tuple in tuples(&i, &fibre) {
                let Some(cells) = joined_cells(f, &i, &d, &fibre, &tuple) else {
                    continue;
                };
                let id = match tuple.as_slice() {
                    [only] => *only,
                    _ => fresh.take().ok_or(KanError::RowIdsExhausted)?,
                };
                out.rows.insert(id, (d.clone(), cells));
                prov.insert(id, (d.clone(), tuple.clone()));
                ids.insert((d.clone(), tuple), id);
            }
        }
        for rho in &f.tgt.relations {
            let carried: Vec<&SEdge> = f
                .relations
                .iter()
                .filter(|(_, img)| *img == rho)
                .map(|(x, _)| x)
                .collect();
            let (fs, ft) = (f.fibre(&rho.src), f.fibre(&rho.tgt));
            let rows_at = |t: &Name| {
                ids.iter()
                    .filter(move |((d, _), _)| d == t)
                    .map(|((_, tu), id)| (tu.clone(), *id))
                    .collect::<Vec<_>>()
            };
            let source_rows = rows_at(&rho.src);
            let target_rows = rows_at(&rho.tgt);
            if carried.is_empty() {
                let actual = source_rows.len().saturating_mul(target_rows.len());
                if actual > max_product_size {
                    return Err(KanError::ProductSizeExceeded {
                        vertex: rho.src.clone(),
                        actual,
                        limit: max_product_size,
                    });
                }
            }
            for (u, uid) in source_rows {
                for (v, vid) in &target_rows {
                    let ok = carried.iter().all(|x| {
                        let a = fs.iter().position(|s| *s == x.src).map(|k| u[k]);
                        let b = ft.iter().position(|s| *s == x.tgt).map(|k| v[k]);
                        matches!((a, b), (Some(a), Some(b)) if i.pairs.contains(&((*x).clone(), a, b)))
                    });
                    if ok {
                        out.pairs.insert((rho.clone(), uid, *vid));
                    }
                }
            }
        }
        Ok((out, prov))
    }

    /// Definition: Pi of an instance of the kept part `S'`.
    ///
    /// [`pi`] along the same maps with the source replaced by the kept part,
    /// as [`sigma_restricted`] replaces it.
    ///
    /// # Errors
    ///
    /// As [`pi`], conformance read against `S'`.
    ///
    /// Book anchors: `L-kan-extensions-3`, `L-kan-extensions-5`.
    pub fn pi_restricted(
        f: &RelFunctor,
        i: &Table,
        max_product_size: usize,
    ) -> Result<(Table, PiProvenance), KanError> {
        let mut restricted = f.clone();
        restricted.src = f.restricted_source();
        pi(&restricted, i, max_product_size)
    }

    fn joined_cells(
        func: &RelFunctor,
        inst: &Table,
        target: &str,
        fibre: &[Name],
        tuple: &[u32],
    ) -> Option<BTreeMap<String, Value>> {
        let mut cells = BTreeMap::new();
        for beta in func
            .tgt
            .columns
            .get(target)
            .map(|c| c.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            let mut found: Option<Value> = None;
            for (pos, table) in fibre.iter().enumerate() {
                let row = &inst.rows.get(&tuple[pos])?.1;
                for ((src, col), image) in &func.columns {
                    if src == table && *image == beta {
                        let val = row.get(col)?;
                        match &found {
                            None => found = Some(val.clone()),
                            Some(prior)
                                if crate::canonical::value_bytes(prior)
                                    == crate::canonical::value_bytes(val) => {}
                            Some(_) => return None,
                        }
                    }
                }
            }
            cells.insert(beta, found?);
        }
        Some(cells)
    }

    /// Why an attributed-instance homomorphism refuses.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum HomError {
        /// A domain row has no image.
        NodeNotMapped(u32),
        /// A claimed image row is absent.
        ImageNodeMissing {
            /// Domain row.
            src: u32,
            /// Missing image.
            tgt: u32,
        },
        /// A row is sent to a different table.
        AnchorMismatch {
            /// Domain row.
            src: u32,
            /// Image row.
            tgt: u32,
            /// Domain table.
            src_anchor: Name,
            /// Image table.
            tgt_anchor: Name,
        },
        /// A row and its image disagree on a cell after the null map.
        AttributeMismatch {
            /// Domain row.
            src: u32,
            /// Image row.
            tgt: u32,
        },
        /// A relation pair is not preserved.
        ForeignKeyNotPreserved {
            /// Relation.
            edge: SEdge,
            /// Mapped source row.
            mapped_src: u32,
            /// Mapped target row.
            mapped_tgt: u32,
        },
        /// Composition reaches a row outside the second map's domain.
        ComposeNodeMismatch(u32),
    }

    /// A homomorphism of attributed instances: a row map and the image of each
    /// labelled null that occurs in the domain.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Hom {
        /// Domain row to codomain row.
        pub rows: BTreeMap<u32, u32>,
        /// Domain labelled null to its codomain value.
        pub nulls: BTreeMap<u64, Value>,
    }

    impl Hom {
        /// Check the map in row-totality, image, table, cell and relation order.
        ///
        /// # Errors
        ///
        /// The first [`HomError`] in that order.
        pub fn check(&self, domain: &Table, codomain: &Table) -> Result<(), HomError> {
            for (source, (source_table, source_cells)) in &domain.rows {
                let target = self
                    .rows
                    .get(source)
                    .copied()
                    .ok_or(HomError::NodeNotMapped(*source))?;
                let Some((target_table, target_cells)) = codomain.rows.get(&target) else {
                    return Err(HomError::ImageNodeMissing {
                        src: *source,
                        tgt: target,
                    });
                };
                if source_table != target_table {
                    return Err(HomError::AnchorMismatch {
                        src: *source,
                        tgt: target,
                        src_anchor: source_table.clone(),
                        tgt_anchor: target_table.clone(),
                    });
                }
                let cells_match = source_cells.iter().all(|(column, value)| {
                    let Some(image) = target_cells.get(column) else {
                        return false;
                    };
                    let mapped = match value {
                        Value::LabeledNull(null) => self
                            .nulls
                            .get(null)
                            .cloned()
                            .unwrap_or_else(|| value.clone()),
                        _ => value.clone(),
                    };
                    crate::canonical::value_bytes(&mapped) == crate::canonical::value_bytes(image)
                });
                if !cells_match || source_cells.len() != target_cells.len() {
                    return Err(HomError::AttributeMismatch {
                        src: *source,
                        tgt: target,
                    });
                }
            }
            for (edge, source, target) in &domain.pairs {
                let mapped_source = self.rows[source];
                let mapped_target = self.rows[target];
                if !codomain
                    .pairs
                    .contains(&(edge.clone(), mapped_source, mapped_target))
                {
                    return Err(HomError::ForeignKeyNotPreserved {
                        edge: edge.clone(),
                        mapped_src: mapped_source,
                        mapped_tgt: mapped_target,
                    });
                }
            }
            Ok(())
        }

        /// Compose `self: A -> B` with `after: B -> C`, refusing a partial
        /// intermediate row map.
        ///
        /// # Errors
        ///
        /// [`HomError::ComposeNodeMismatch`] at the least missing intermediate row.
        pub fn compose(&self, after: &Self) -> Result<Self, HomError> {
            let rows = self
                .rows
                .iter()
                .map(|(source, middle)| {
                    after
                        .rows
                        .get(middle)
                        .copied()
                        .map(|target| (*source, target))
                        .ok_or(HomError::ComposeNodeMismatch(*middle))
                })
                .collect::<Result<_, _>>()?;
            // An omitted null entry denotes the identity. Thus every entry
            // of `after` is already the image of the corresponding implicit
            // identity entry of `self`; explicit entries of `self` then
            // override those keys after substitution through `after`.
            let mut nulls = after.nulls.clone();
            for (null, value) in &self.nulls {
                let image = match value {
                    Value::LabeledNull(middle) => after
                        .nulls
                        .get(middle)
                        .cloned()
                        .unwrap_or_else(|| value.clone()),
                    _ => value.clone(),
                };
                nulls.insert(*null, image);
            }
            Ok(Self { rows, nulls })
        }

        /// Compose `self: A -> B` with `after: B -> C`.
        #[must_use]
        pub fn then(&self, after: &Self) -> Self {
            self.compose(after).unwrap_or_default()
        }

        /// Extensional equality after dropping explicit identity null images.
        #[must_use]
        pub fn normalized_same_as(&self, other: &Self) -> bool {
            let normalize = |map: &BTreeMap<u64, Value>| {
                map.iter()
                    .filter(|(null, value)| value != &&Value::LabeledNull(**null))
                    .map(|(null, value)| (*null, crate::canonical::value_bytes(value)))
                    .collect::<BTreeMap<_, _>>()
            };
            self.rows == other.rows && normalize(&self.nulls) == normalize(&other.nulls)
        }

        /// Extensional equality after dropping null-map entries unused by the
        /// domain. Values are compared by canonical bytes.
        #[must_use]
        pub fn same_as(&self, other: &Self, domain: &Table) -> bool {
            if self.rows != other.rows {
                return false;
            }
            let used: BTreeSet<u64> = domain
                .rows
                .values()
                .flat_map(|(_, cells)| cells.values())
                .filter_map(|value| match value {
                    Value::LabeledNull(id) => Some(*id),
                    _ => None,
                })
                .collect();
            used.into_iter().all(|id| {
                let left = self
                    .nulls
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::LabeledNull(id));
                let right = other
                    .nulls
                    .get(&id)
                    .cloned()
                    .unwrap_or(Value::LabeledNull(id));
                crate::canonical::value_bytes(&left) == crate::canonical::value_bytes(&right)
            })
        }
    }

    /// Why an adjunction unit, counit or transpose refuses.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TransposeError {
        /// The required Sigma, Delta or Pi object is outside the operation's
        /// domain.
        Adjunction(Box<AdjunctionError>),
        /// The supplied or induced arrow is not a homomorphism.
        Hom(Box<HomError>),
    }

    fn transpose_adjunction(error: KanError) -> TransposeError {
        TransposeError::Adjunction(Box::new(error.into()))
    }

    fn infer_hom(
        domain: &Table,
        codomain: &Table,
        rows: BTreeMap<u32, u32>,
    ) -> Result<Hom, TransposeError> {
        let mut nulls = BTreeMap::new();
        for (source, (_, cells)) in &domain.rows {
            let target = rows
                .get(source)
                .copied()
                .ok_or_else(|| TransposeError::Hom(Box::new(HomError::NodeNotMapped(*source))))?;
            let Some((_, target_cells)) = codomain.rows.get(&target) else {
                return Err(TransposeError::Hom(Box::new(HomError::ImageNodeMissing {
                    src: *source,
                    tgt: target,
                })));
            };
            for (column, value) in cells {
                let Some(image) = target_cells.get(column) else {
                    return Err(TransposeError::Hom(Box::new(HomError::AttributeMismatch {
                        src: *source,
                        tgt: target,
                    })));
                };
                if let Value::LabeledNull(id) = value {
                    if nulls.get(id).is_some_and(|prior: &Value| {
                        crate::canonical::value_bytes(prior) != crate::canonical::value_bytes(image)
                    }) {
                        return Err(TransposeError::Hom(Box::new(HomError::AttributeMismatch {
                            src: *source,
                            tgt: target,
                        })));
                    }
                    nulls.insert(*id, image.clone());
                }
            }
        }
        let hom = Hom { rows, nulls };
        hom.check(domain, codomain)
            .map_err(|error| TransposeError::Hom(Box::new(error)))?;
        Ok(hom)
    }

    /// Unit `I -> Delta(Sigma(I))` of the Sigma–Delta adjunction.
    ///
    /// `I` is an instance over the functor's restricted source. The operation
    /// forms restricted Sigma, then Delta, then the induced homomorphism.
    /// This is the forward Sigma transpose of the identity on `Sigma(I)`:
    /// row `r` is sent to the Delta copy `(table(r), r)`. `infer_hom`
    /// supplies the uniquely forced labelled-null image and verifies every
    /// cell and relation.
    ///
    /// # Errors
    ///
    /// In operation order: the first restricted-Sigma domain reason, then the
    /// first Delta reason, then the first induced row-totality, image, table,
    /// cell, or relation-preservation reason.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-6`.
    pub fn sigma_unit(f: &RelFunctor, input: &Table) -> Result<Hom, TransposeError> {
        let (sigma, _) = sigma_restricted(f, input).map_err(transpose_adjunction)?;
        let (delta, copies) = delta(f, &sigma).map_err(transpose_adjunction)?;
        let rows = input
            .rows
            .iter()
            .filter_map(|(row, (table, _))| {
                copies
                    .iter()
                    .find(|(_, (source_table, original))| source_table == table && original == row)
                    .map(|(copy, _)| (*row, *copy))
            })
            .collect();
        infer_hom(input, &delta, rows)
    }

    /// Counit `Sigma(Delta(J)) -> J` of the Sigma–Delta adjunction.
    ///
    /// `J` is an instance over the target. The operation forms Delta over the
    /// restricted source, then restricted Sigma, then the induced homomorphism.
    /// This is the inverse Sigma transpose of the identity on `Delta(J)`:
    /// each Sigma row is projected through Delta copy provenance to its
    /// original target row. `infer_hom` supplies the forced null image and
    /// checks the result.
    ///
    /// # Errors
    ///
    /// In operation order: the first Delta reason, then the first restricted-
    /// Sigma reason, then the first induced row-totality, image, table, cell,
    /// or relation-preservation reason.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-6`.
    pub fn sigma_counit(f: &RelFunctor, target: &Table) -> Result<Hom, TransposeError> {
        let (delta_table, copies) = delta(f, target).map_err(transpose_adjunction)?;
        let (sigma_table, _) = sigma_restricted(f, &delta_table).map_err(transpose_adjunction)?;
        let rows = sigma_table
            .rows
            .keys()
            .filter_map(|row| copies.get(row).map(|(_, original)| (*row, *original)))
            .collect();
        infer_hom(&sigma_table, target, rows)
    }

    /// Forward transpose `Hom(Sigma(I), J) -> Hom(I, Delta(J))`.
    ///
    /// `I` is over the restricted source and `J` over the target. The
    /// operation forms restricted Sigma, checks the supplied homomorphism,
    /// forms Delta, then infers the result.
    /// For `h: Sigma(I) -> J`, row `r` is sent to the Delta copy
    /// `(table(r), h(r))`. `infer_hom` supplies the uniquely forced
    /// labelled-null image and checks the result.
    ///
    /// # Errors
    ///
    /// In operation order: the first restricted-Sigma reason, the exact
    /// supplied-homomorphism reason, the first Delta reason, or the first
    /// induced-homomorphism reason.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-6`.
    pub fn sigma_transpose_forward(
        f: &RelFunctor,
        input: &Table,
        target: &Table,
        hom: &Hom,
    ) -> Result<Hom, TransposeError> {
        let (sigma_table, _) = sigma_restricted(f, input).map_err(transpose_adjunction)?;
        hom.check(&sigma_table, target)
            .map_err(|error| TransposeError::Hom(Box::new(error)))?;
        let (delta_table, copies) = delta(f, target).map_err(transpose_adjunction)?;
        let rows = input
            .rows
            .iter()
            .filter_map(|(row, (table, _))| {
                let target_row = hom.rows.get(row)?;
                copies
                    .iter()
                    .find(|(_, (source_table, original))| {
                        source_table == table && original == target_row
                    })
                    .map(|(copy, _)| (*row, *copy))
            })
            .collect();
        infer_hom(input, &delta_table, rows)
    }

    /// Inverse transpose `Hom(I, Delta(J)) -> Hom(Sigma(I), J)`.
    ///
    /// `I` is over the restricted source and `J` over the target. The
    /// operation forms Delta, checks the supplied homomorphism, forms
    /// restricted Sigma, then infers the result.
    /// For `h: I -> Delta(J)`, the image copy `h(r)` is projected through
    /// Delta provenance to the original target row. `infer_hom` supplies
    /// the uniquely forced null image and checks the result.
    ///
    /// # Errors
    ///
    /// In operation order: the first Delta reason, the exact supplied-
    /// homomorphism reason, the first restricted-Sigma reason, or the first
    /// induced-homomorphism reason.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-6`.
    pub fn sigma_transpose_inverse(
        f: &RelFunctor,
        input: &Table,
        target: &Table,
        hom: &Hom,
    ) -> Result<Hom, TransposeError> {
        let (delta_table, copies) = delta(f, target).map_err(transpose_adjunction)?;
        hom.check(input, &delta_table)
            .map_err(|error| TransposeError::Hom(Box::new(error)))?;
        let (sigma_table, _) = sigma_restricted(f, input).map_err(transpose_adjunction)?;
        let rows = sigma_table
            .rows
            .keys()
            .filter_map(|row| {
                hom.rows
                    .get(row)
                    .and_then(|copy| copies.get(copy))
                    .map(|(_, original)| (*row, *original))
            })
            .collect();
        infer_hom(&sigma_table, target, rows)
    }

    /// Unit `J -> Pi(Delta(J))` of the Delta–Pi adjunction.
    ///
    /// `J` is over the target. The operation forms Delta over the restricted
    /// source, forms bounded restricted Pi, then infers the result.
    /// This is the forward Pi transpose of the identity on `Delta(J)`: row
    /// `r` is sent to the Pi row whose ordered fibre tuple contains every
    /// Delta copy `(s,r)`. `infer_hom` supplies the forced null image and
    /// checks the result.
    ///
    /// # Errors
    ///
    /// In operation order: the first Delta reason, then the first bounded-
    /// restricted-Pi reason, then the first induced row-totality, image,
    /// table, cell, or relation reason.
    ///
    /// Book anchors: `L-kan-extensions-5`, `L-kan-extensions-7`.
    pub fn pi_unit(
        f: &RelFunctor,
        target: &Table,
        max_product_size: usize,
    ) -> Result<Hom, TransposeError> {
        let (delta_table, copies) = delta(f, target).map_err(transpose_adjunction)?;
        let (pi_table, tuples) =
            pi_restricted(f, &delta_table, max_product_size).map_err(transpose_adjunction)?;
        let rows = target
            .rows
            .iter()
            .filter_map(|(row, (table, _))| {
                let tuple = f
                    .fibre(table)
                    .iter()
                    .map(|source_table| {
                        copies
                            .iter()
                            .find(|(_, (candidate, original))| {
                                candidate == source_table && original == row
                            })
                            .map(|(copy, _)| *copy)
                    })
                    .collect::<Option<Vec<_>>>()?;
                tuples
                    .iter()
                    .find(|(_, (candidate, components))| candidate == table && components == &tuple)
                    .map(|(product, _)| (*row, *product))
            })
            .collect();
        infer_hom(target, &pi_table, rows)
    }

    /// Counit `Delta(Pi(I)) -> I` of the Delta–Pi adjunction.
    ///
    /// `I` is over the restricted source. The operation forms bounded
    /// restricted Pi, forms Delta, then infers the result.
    /// This is the inverse Pi transpose of the identity on `Pi(I)`: a Delta
    /// copy at source table `s` is projected to the `s` component of its Pi
    /// tuple. `infer_hom` supplies the forced null image and checks the
    /// result.
    ///
    /// # Errors
    ///
    /// In operation order: the first bounded-restricted-Pi reason, then the
    /// first Delta reason, then the first induced row-totality, image, table,
    /// cell, or relation reason.
    ///
    /// Book anchors: `L-kan-extensions-5`, `L-kan-extensions-7`.
    pub fn pi_counit(
        f: &RelFunctor,
        input: &Table,
        max_product_size: usize,
    ) -> Result<Hom, TransposeError> {
        let (pi_table, tuples) =
            pi_restricted(f, input, max_product_size).map_err(transpose_adjunction)?;
        let (delta_table, copies) = delta(f, &pi_table).map_err(transpose_adjunction)?;
        let rows = delta_table
            .rows
            .iter()
            .filter_map(|(copy, (source_table, _))| {
                let (_, product) = copies.get(copy)?;
                let (target_table, tuple) = tuples.get(product)?;
                let index = f
                    .fibre(target_table)
                    .iter()
                    .position(|table| table == source_table)?;
                tuple.get(index).map(|source_row| (*copy, *source_row))
            })
            .collect();
        infer_hom(&delta_table, input, rows)
    }

    /// Forward transpose `Hom(Delta(J), I) -> Hom(J, Pi(I))`.
    ///
    /// `J` is over the target and `I` over the restricted source. The
    /// operation forms Delta, checks the supplied homomorphism, forms bounded
    /// restricted Pi, then infers the result.
    /// For `h: Delta(J) -> I`, row `r` is sent to the Pi row whose ordered
    /// fibre tuple is `[h(copy(s,r)) | s in fibre(table(r))]`.
    /// `infer_hom` supplies the uniquely forced null image and checks the
    /// result.
    ///
    /// # Errors
    ///
    /// In operation order: the first Delta reason, the exact supplied-
    /// homomorphism reason, the first bounded-restricted-Pi reason, or the
    /// first induced-homomorphism reason.
    ///
    /// Book anchors: `L-kan-extensions-5`, `L-kan-extensions-7`.
    pub fn pi_transpose_forward(
        f: &RelFunctor,
        target: &Table,
        input: &Table,
        hom: &Hom,
        max_product_size: usize,
    ) -> Result<Hom, TransposeError> {
        let (delta_table, copies) = delta(f, target).map_err(transpose_adjunction)?;
        hom.check(&delta_table, input)
            .map_err(|error| TransposeError::Hom(Box::new(error)))?;
        let (pi_table, tuples) =
            pi_restricted(f, input, max_product_size).map_err(transpose_adjunction)?;
        let rows = target
            .rows
            .iter()
            .filter_map(|(row, (target_table, _))| {
                let tuple = f
                    .fibre(target_table)
                    .iter()
                    .map(|source_table| {
                        copies
                            .iter()
                            .find(|(_, (candidate, original))| {
                                candidate == source_table && original == row
                            })
                            .and_then(|(copy, _)| hom.rows.get(copy))
                            .copied()
                    })
                    .collect::<Option<Vec<_>>>()?;
                tuples
                    .iter()
                    .find(|(_, (candidate, components))| {
                        candidate == target_table && components == &tuple
                    })
                    .map(|(product, _)| (*row, *product))
            })
            .collect();
        infer_hom(target, &pi_table, rows)
    }

    /// Inverse transpose `Hom(J, Pi(I)) -> Hom(Delta(J), I)`.
    ///
    /// `J` is over the target and `I` over the restricted source. The
    /// operation forms bounded restricted Pi, checks the supplied
    /// homomorphism, forms Delta, then infers the result.
    /// For `h: J -> Pi(I)`, a Delta copy at source table `s` is sent to the
    /// `s` component of the tuple recorded for `h(r)`. `infer_hom` supplies
    /// the uniquely forced null image and checks the result.
    ///
    /// # Errors
    ///
    /// In operation order: the first bounded-restricted-Pi reason, the exact
    /// supplied-homomorphism reason, the first Delta reason, or the first
    /// induced-homomorphism reason.
    ///
    /// Book anchors: `L-kan-extensions-5`, `L-kan-extensions-7`.
    pub fn pi_transpose_inverse(
        f: &RelFunctor,
        target: &Table,
        input: &Table,
        hom: &Hom,
        max_product_size: usize,
    ) -> Result<Hom, TransposeError> {
        let (pi_table, tuples) =
            pi_restricted(f, input, max_product_size).map_err(transpose_adjunction)?;
        hom.check(target, &pi_table)
            .map_err(|error| TransposeError::Hom(Box::new(error)))?;
        let (delta_table, copies) = delta(f, target).map_err(transpose_adjunction)?;
        let rows = delta_table
            .rows
            .iter()
            .filter_map(|(copy, (source_table, _))| {
                let (_, target_row) = copies.get(copy)?;
                let product = hom.rows.get(target_row)?;
                let (target_table, tuple) = tuples.get(product)?;
                let index = f
                    .fibre(target_table)
                    .iter()
                    .position(|table| table == source_table)?;
                tuple.get(index).map(|source_row| (*copy, *source_row))
            })
            .collect();
        infer_hom(&delta_table, input, rows)
    }

    /// Definition: A structure-preserving bijection between two attributed instances.
    ///
    /// A bijection of rows preserving tables, pairs and cells, with a bijection of labelled nulls under
    /// which cells agree (every other value compared by canonical bytes).
    ///
    /// Found by naive backtracking over row bijections; `None` when none
    /// exists.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-5`,
    /// `L-kan-extensions-6`, `L-kan-extensions-7`.
    #[must_use]
    pub fn row_bijection(a: &Table, b: &Table) -> Option<Hom> {
        if !a.well_formed()
            || !b.well_formed()
            || a.rows.len() != b.rows.len()
            || a.pairs.len() != b.pairs.len()
        {
            return None;
        }
        let order: Vec<u32> = a.rows.keys().copied().collect();
        let mut map = BTreeMap::new();
        let mut nulls = BTreeMap::new();
        search_bijection(a, b, &order, 0, &mut map, &mut nulls).then(|| Hom {
            rows: map,
            nulls: nulls
                .into_iter()
                .map(|(source, target)| (source, Value::LabeledNull(target)))
                .collect(),
        })
    }

    fn search_bijection(
        left: &Table,
        right: &Table,
        order: &[u32],
        pos: usize,
        map: &mut BTreeMap<u32, u32>,
        nulls: &mut BTreeMap<u64, u64>,
    ) -> bool {
        let Some(&row) = order.get(pos) else {
            return left
                .pairs
                .iter()
                .all(|(edge, from, to)| right.pairs.contains(&(edge.clone(), map[from], map[to])));
        };
        let (ta, ca) = &left.rows[&row];
        for (cand, (tb, cb)) in &right.rows {
            if tb != ta || map.values().any(|val| val == cand) || ca.keys().ne(cb.keys()) {
                continue;
            }
            let saved = nulls.clone();
            if ca
                .iter()
                .all(|(col, val)| match_values(val, &cb[col], nulls))
            {
                map.insert(row, *cand);
                if search_bijection(left, right, order, pos + 1, map, nulls) {
                    return true;
                }
                map.remove(&row);
            }
            *nulls = saved;
        }
        false
    }

    fn match_values(left: &Value, right: &Value, nulls: &mut BTreeMap<u64, u64>) -> bool {
        match (left, right) {
            (Value::LabeledNull(lnull), Value::LabeledNull(rnull)) => {
                if let Some(mapped) = nulls.get(lnull) {
                    return mapped == rnull;
                }
                if nulls.values().any(|mapped| mapped == rnull) {
                    return false;
                }
                nulls.insert(*lnull, *rnull);
                true
            }
            (Value::LabeledNull(_), _) | (_, Value::LabeledNull(_)) => false,
            _ => crate::canonical::value_bytes(left) == crate::canonical::value_bytes(right),
        }
    }

    /// Definition: Every homomorphism between two instances of one schema.
    ///
    /// A map of rows preserving tables and pairs, under which each cell of the domain equals the image
    /// row's cell, a labelled null of the domain being sent to the value it first meets (consistently),
    /// every other value compared by canonical bytes. Enumerated naively over all row maps. Both
    /// tables must be well formed; a dangling pair in either table is refused by the empty result.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-5`, `L-kan-extensions-6`, `L-kan-extensions-7`.
    #[must_use]
    pub fn homs(a: &Table, b: &Table) -> Vec<Hom> {
        if !a.well_formed() || !b.well_formed() {
            return Vec::new();
        }
        let order: Vec<u32> = a.rows.keys().copied().collect();
        let mut out = Vec::new();
        hom_search(
            a,
            b,
            &order,
            0,
            &mut BTreeMap::new(),
            &BTreeMap::new(),
            &mut out,
        );
        out
    }

    fn hom_search(
        dom: &Table,
        cod: &Table,
        order: &[u32],
        pos: usize,
        map: &mut BTreeMap<u32, u32>,
        nulls: &BTreeMap<u64, Vec<u8>>,
        out: &mut Vec<Hom>,
    ) {
        let Some(&row) = order.get(pos) else {
            if dom
                .pairs
                .iter()
                .all(|(edge, from, to)| cod.pairs.contains(&(edge.clone(), map[from], map[to])))
            {
                out.push(Hom {
                    rows: map.clone(),
                    nulls: nulls
                        .iter()
                        .filter_map(|(id, bytes)| {
                            crate::canonical::decode_value(bytes).map(|value| (*id, value))
                        })
                        .collect(),
                });
            }
            return;
        };
        let (ta, ca) = &dom.rows[&row];
        for (cand, (tb, cb)) in &cod.rows {
            if tb != ta {
                continue;
            }
            let mut fixed = nulls.clone();
            let ok = ca.iter().all(|(col, val)| {
                let Some(img) = cb.get(col) else { return false };
                let wb = crate::canonical::value_bytes(img).unwrap_or_default();
                match val {
                    Value::LabeledNull(id) => {
                        if let Some(prev) = fixed.get(id) {
                            return *prev == wb;
                        }
                        fixed.insert(*id, wb);
                        true
                    }
                    other => crate::canonical::value_bytes(other).unwrap_or_default() == wb,
                }
            });
            if ok {
                map.insert(row, *cand);
                hom_search(dom, cod, order, pos + 1, map, &fixed, out);
                map.remove(&row);
            }
        }
    }
    /// An object of the entity category of a relational side: a table, or a
    /// relation (an edge between two tables).
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum EntityObject {
        /// A table.
        Table(Name),
        /// A relation object.
        Relation(SEdge),
    }

    /// An arrow of the entity category: an identity, or one of the two legs
    /// of a relation object.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum EntityArrow {
        /// The identity of an object.
        Id(EntityObject),
        /// The source leg `e -> src(e)` of a relation `e`.
        Src(SEdge),
        /// The target leg `e -> tgt(e)` of a relation `e`.
        Tgt(SEdge),
    }

    impl EntityArrow {
        /// The domain of the arrow.
        #[must_use]
        pub fn dom(&self) -> EntityObject {
            match self {
                Self::Id(object) => object.clone(),
                Self::Src(edge) | Self::Tgt(edge) => EntityObject::Relation(edge.clone()),
            }
        }

        /// The codomain of the arrow.
        #[must_use]
        pub fn cod(&self) -> EntityObject {
            match self {
                Self::Id(object) => object.clone(),
                Self::Src(edge) => EntityObject::Table(edge.src.clone()),
                Self::Tgt(edge) => EntityObject::Table(edge.tgt.clone()),
            }
        }
    }

    /// Definition: composition in the entity category.
    ///
    /// `after . first`, defined when `cod first = dom after`. Besides
    /// identities the category has only legs, each ending at a table, and no
    /// arrow but the identity leaves a table, so two legs never compose.
    /// `None` when the composite is undefined.
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn entity_compose(after: &EntityArrow, first: &EntityArrow) -> Option<EntityArrow> {
        if first.cod() != after.dom() {
            return None;
        }
        match (after, first) {
            (EntityArrow::Id(_), other) | (other, EntityArrow::Id(_)) => Some(other.clone()),
            _ => None,
        }
    }

    /// Definition: the objects and every arrow of the entity category of a
    /// side.
    ///
    /// One object per table and one per relation; the arrows are the
    /// identities and, for each relation `e`, its legs `Src(e)` and `Tgt(e)`.
    /// Objects come tables first, each list ascending, and arrows the
    /// identities in object order followed by the legs relation by relation.
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn entity_category(
        tables: &BTreeSet<Name>,
        relations: &BTreeSet<SEdge>,
    ) -> (Vec<EntityObject>, Vec<EntityArrow>) {
        let mut objects: Vec<EntityObject> =
            tables.iter().cloned().map(EntityObject::Table).collect();
        objects.extend(relations.iter().cloned().map(EntityObject::Relation));
        let mut arrows: Vec<EntityArrow> = objects.iter().cloned().map(EntityArrow::Id).collect();
        for edge in relations {
            arrows.push(EntityArrow::Src(edge.clone()));
            arrows.push(EntityArrow::Tgt(edge.clone()));
        }
        (objects, arrows)
    }

    /// A finite functor from an entity category to sets: the set of
    /// elements at each object and the action of each leg. Elements are
    /// numbers.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct EntityInstance {
        /// The elements at each object.
        pub sets: BTreeMap<EntityObject, BTreeSet<usize>>,
        /// The image of each element under each leg leaving its object.
        pub legs: BTreeMap<(EntityArrow, usize), usize>,
    }

    impl EntityInstance {
        /// `I(u)(x)`: the identity returns `x`, a leg its recorded image;
        /// `None` when a leg has no recorded image of `x`.
        #[must_use]
        pub fn act(&self, arrow: &EntityArrow, element: usize) -> Option<usize> {
            match arrow {
                EntityArrow::Id(_) => Some(element),
                leg => self.legs.get(&(leg.clone(), element)).copied(),
            }
        }
    }

    /// Definition: the entity instance of a graph over a side.
    ///
    /// At each table of the side, the ids of the graph's nodes anchored
    /// there; at each relation, the ids of the graph's edges labelled by it,
    /// each edge its own element (so parallel edges stay apart); each leg
    /// sends an edge to the corresponding endpoint. Nodes and edges at no
    /// object of the side are left out.
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn entity_instance(
        graph: &crate::model::Graph,
        tables: &BTreeSet<Name>,
        relations: &BTreeSet<SEdge>,
    ) -> EntityInstance {
        let mut instance = EntityInstance::default();
        for table in tables {
            instance
                .sets
                .insert(EntityObject::Table(table.clone()), BTreeSet::new());
        }
        for relation in relations {
            instance
                .sets
                .insert(EntityObject::Relation(relation.clone()), BTreeSet::new());
        }
        for node in graph.nodes.values() {
            if let Some(set) = instance
                .sets
                .get_mut(&EntityObject::Table(node.anchor.clone()))
            {
                set.insert(node.id as usize);
            }
        }
        for (id, edge) in &graph.edges {
            if let Some(set) = instance
                .sets
                .get_mut(&EntityObject::Relation(edge.label.clone()))
            {
                let element = *id as usize;
                set.insert(element);
                instance.legs.insert(
                    (EntityArrow::Src(edge.label.clone()), element),
                    edge.src as usize,
                );
                instance.legs.insert(
                    (EntityArrow::Tgt(edge.label.clone()), element),
                    edge.tgt as usize,
                );
            }
        }
        instance
    }

    /// The image of an entity object under the table and relation maps of a
    /// functor; `None` when the map omits it.
    fn entity_image(f: &RelFunctor, object: &EntityObject) -> Option<EntityObject> {
        match object {
            EntityObject::Table(table) => f.tables.get(table).cloned().map(EntityObject::Table),
            EntityObject::Relation(edge) => {
                f.relations.get(edge).cloned().map(EntityObject::Relation)
            }
        }
    }

    /// The image of an entity arrow: identities to identities, each leg of
    /// `e` to the same leg of `F(e)`; `None` when the map omits it.
    fn entity_arrow_image(f: &RelFunctor, arrow: &EntityArrow) -> Option<EntityArrow> {
        match arrow {
            EntityArrow::Id(object) => entity_image(f, object).map(EntityArrow::Id),
            EntityArrow::Src(edge) => f.relations.get(edge).cloned().map(EntityArrow::Src),
            EntityArrow::Tgt(edge) => f.relations.get(edge).cloned().map(EntityArrow::Tgt),
        }
    }

    const fn find_root(parent: &mut [usize], mut element: usize) -> usize {
        while parent[element] != element {
            parent[element] = parent[parent[element]];
            element = parent[element];
        }
        element
    }

    /// Definition: the pointwise left Kan extension of an entity instance
    /// along the entity part of a functor.
    ///
    /// For each target object `d` of the entity category on `target_tables`
    /// and `target_relations`, the colimit over the comma category `(F, d)`:
    /// the disjoint union of the sets `I(c)` indexed by the comma objects
    /// `(c, f: F(c) -> d)`, quotiented by joining `x` at `(c, f)` with
    /// `I(u)(x)` at `(c', f')` for every source arrow `u: c -> c'` with
    /// `f' . F(u) = f`, the pointwise formula. Each class is named by the index
    /// of its least member in comma order. A target leg `l: r -> d` sends the
    /// class of `x` at `(c, f)` to the class of `x` at `(c, l . f)`.
    ///
    /// The source category is the one on the functor's kept tables and kept
    /// relations (`f.tables`, `f.relations`). `None` when the instance lacks
    /// an object of the source category, a leg image is missing, a functor
    /// image lies outside the target, or a leg action depends on the
    /// representative (which the colimit rules out).
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn presheaf_left_kan(
        instance: &EntityInstance,
        f: &RelFunctor,
        target_tables: &BTreeSet<Name>,
        target_relations: &BTreeSet<SEdge>,
    ) -> Option<EntityInstance> {
        let source_tables: BTreeSet<Name> = f.tables.keys().cloned().collect();
        let source_relations: BTreeSet<SEdge> = f.relations.keys().cloned().collect();
        let (source_objects, source_arrows) = entity_category(&source_tables, &source_relations);
        let (target_objects, target_arrows) = entity_category(target_tables, target_relations);
        let mut result = EntityInstance::default();
        let mut classes: BTreeMap<EntityObject, BTreeMap<(usize, usize), usize>> = BTreeMap::new();
        let mut commas: BTreeMap<EntityObject, Vec<(EntityObject, EntityArrow)>> = BTreeMap::new();
        for d in &target_objects {
            let mut comma: Vec<(EntityObject, EntityArrow)> = Vec::new();
            for c in &source_objects {
                let image = entity_image(f, c)?;
                if !target_objects.contains(&image) {
                    return None;
                }
                for arrow in &target_arrows {
                    if arrow.dom() == image && arrow.cod() == *d {
                        comma.push((c.clone(), arrow.clone()));
                    }
                }
            }
            let mut elements: Vec<(usize, usize)> = Vec::new();
            for (k, (c, _)) in comma.iter().enumerate() {
                for x in instance.sets.get(c)? {
                    elements.push((k, *x));
                }
            }
            let index: BTreeMap<(usize, usize), usize> = elements
                .iter()
                .enumerate()
                .map(|(position, element)| (*element, position))
                .collect();
            let mut parent: Vec<usize> = (0..elements.len()).collect();
            for (k, (c, along)) in comma.iter().enumerate() {
                for (k2, (c2, along2)) in comma.iter().enumerate() {
                    for u in source_arrows
                        .iter()
                        .filter(|u| u.dom() == *c && u.cod() == *c2)
                    {
                        let image = entity_arrow_image(f, u)?;
                        if entity_compose(along2, &image).as_ref() != Some(along) {
                            continue;
                        }
                        for x in instance.sets.get(c)? {
                            let moved = instance.act(u, *x)?;
                            let a = find_root(&mut parent, *index.get(&(k, *x))?);
                            let b = find_root(&mut parent, *index.get(&(k2, moved))?);
                            let (low, high) = (a.min(b), a.max(b));
                            parent[high] = low;
                        }
                    }
                }
            }
            let mut class = BTreeMap::new();
            for element in &elements {
                let root = find_root(&mut parent, *index.get(element)?);
                class.insert(*element, root);
            }
            result
                .sets
                .insert(d.clone(), class.values().copied().collect());
            classes.insert(d.clone(), class);
            commas.insert(d.clone(), comma);
        }
        for leg in target_arrows
            .iter()
            .filter(|arrow| !matches!(arrow, EntityArrow::Id(_)))
        {
            let (r, d) = (leg.dom(), leg.cod());
            let into_d = commas.get(&d)?;
            for (&(k, x), &class) in classes.get(&r)? {
                let (c, along) = commas.get(&r)?.get(k)?;
                let composite = entity_compose(leg, along)?;
                let k2 = into_d
                    .iter()
                    .position(|(c2, along2)| c2 == c && *along2 == composite)?;
                let image = *classes.get(&d)?.get(&(k2, x))?;
                if let Some(previous) = result.legs.insert((leg.clone(), class), image) {
                    if previous != image {
                        return None;
                    }
                }
            }
        }
        Some(result)
    }

    /// Each relation's multiset of endpoint pairs, a count per pair.
    pub type EndpointPairs = BTreeMap<SEdge, BTreeMap<(usize, usize), usize>>;

    /// Definition: the separated reflection `L_sep` of an entity instance on
    /// its relations.
    ///
    /// The quotient identifying two elements of one relation whose two legs
    /// agree; the tables are unchanged. Returned as each relation's set of
    /// endpoint pairs (every count is one), with how many elements the
    /// quotient identified. `None` when a relation of `relations` has no set
    /// or a leg image is missing.
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn separated_pairs(
        instance: &EntityInstance,
        relations: &BTreeSet<SEdge>,
    ) -> Option<(EndpointPairs, usize)> {
        let mut pairs = BTreeMap::new();
        let mut identified = 0;
        for relation in relations {
            let mut ends: BTreeMap<(usize, usize), usize> = BTreeMap::new();
            for element in instance
                .sets
                .get(&EntityObject::Relation(relation.clone()))?
            {
                let key = (
                    instance.act(&EntityArrow::Src(relation.clone()), *element)?,
                    instance.act(&EntityArrow::Tgt(relation.clone()), *element)?,
                );
                if ends.insert(key, 1).is_some() {
                    identified += 1;
                }
            }
            pairs.insert(relation.clone(), ends);
        }
        Some((pairs, identified))
    }

    /// Definition: the relation elements of an entity instance as a
    /// multiset of endpoint pairs, without the separated reflection: equal
    /// endpoints keep their multiplicity.
    ///
    /// `None` as in [`separated_pairs`].
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn unseparated_pairs(
        instance: &EntityInstance,
        relations: &BTreeSet<SEdge>,
    ) -> Option<EndpointPairs> {
        let mut pairs = BTreeMap::new();
        for relation in relations {
            let mut ends: BTreeMap<(usize, usize), usize> = BTreeMap::new();
            for element in instance
                .sets
                .get(&EntityObject::Relation(relation.clone()))?
            {
                let key = (
                    instance.act(&EntityArrow::Src(relation.clone()), *element)?,
                    instance.act(&EntityArrow::Tgt(relation.clone()), *element)?,
                );
                *ends.entry(key).or_default() += 1;
            }
            pairs.insert(relation.clone(), ends);
        }
        Some(pairs)
    }

    /// One term of Sigma's value equations: a constant by its canonical
    /// bytes, or a labelled null.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum ValueTerm {
        Constant(Vec<u8>),
        Null(u64),
    }

    /// Definition: whether the value equations of Sigma are inconsistent.
    ///
    /// Each row of a kept table equates the cells of its kept columns that
    /// the functor sends to one target column; a labelled null names one
    /// unknown everywhere, so the equations close under transitivity across
    /// rows; they clash when they join two constants with different
    /// canonical bytes. Rows at tables the functor does not keep, and cells
    /// at columns it drops, contribute no equation. `None` when a cell has
    /// no canonical bytes.
    ///
    /// Book anchors: `L-kan-extensions-2`.
    #[must_use]
    pub fn sigma_values_clash(input: &Table, f: &RelFunctor) -> Option<bool> {
        let mut terms: Vec<ValueTerm> = Vec::new();
        let mut index: BTreeMap<ValueTerm, usize> = BTreeMap::new();
        let mut equations: Vec<(usize, usize)> = Vec::new();
        let mut intern = |term: ValueTerm, terms: &mut Vec<ValueTerm>| {
            *index.entry(term.clone()).or_insert_with(|| {
                terms.push(term);
                terms.len() - 1
            })
        };
        for (table, cells) in input.rows.values() {
            let mut by_image: BTreeMap<&String, Vec<&Value>> = BTreeMap::new();
            for (column, value) in cells {
                if let Some(image) = f.columns.get(&(table.clone(), column.clone())) {
                    by_image.entry(image).or_default().push(value);
                }
            }
            for values in by_image.values() {
                let mut ids = Vec::new();
                for value in values {
                    let term = match value {
                        Value::LabeledNull(null) => ValueTerm::Null(*null),
                        other => ValueTerm::Constant(crate::canonical::value_bytes(other)?),
                    };
                    ids.push(intern(term, &mut terms));
                }
                if let Some((first, rest)) = ids.split_first() {
                    equations.extend(rest.iter().map(|other| (*first, *other)));
                }
            }
        }
        let mut parent: Vec<usize> = (0..terms.len()).collect();
        for (a, b) in equations {
            let (a, b) = (find_root(&mut parent, a), find_root(&mut parent, b));
            parent[a.max(b)] = a.min(b);
        }
        let mut constant_of: BTreeMap<usize, &ValueTerm> = BTreeMap::new();
        for (position, term) in terms.iter().enumerate() {
            if matches!(term, ValueTerm::Constant(_)) {
                let root = find_root(&mut parent, position);
                if constant_of
                    .insert(root, term)
                    .is_some_and(|other| other != term)
                {
                    return Some(true);
                }
            }
        }
        Some(false)
    }
    /// The homomorphism `domain -> codomain` with row map `rows`, its null
    /// map forced by the cells: each labelled null of the domain goes to the
    /// cell it first meets in `(row, column)` order, every later occurrence
    /// and every constant agreeing by canonical bytes. `None` when no such
    /// homomorphism exists.
    fn forced_hom(domain: &Table, codomain: &Table, rows: BTreeMap<u32, u32>) -> Option<Hom> {
        let mut nulls = BTreeMap::new();
        for (source, (_, source_cells)) in &domain.rows {
            let target = rows.get(source)?;
            let (_, target_cells) = codomain.rows.get(target)?;
            for (column, value) in source_cells {
                let image = target_cells.get(column)?;
                let image_bytes = crate::canonical::value_bytes(image)?;
                match value {
                    Value::LabeledNull(id) => {
                        if let Some(prior) = nulls.get(id) {
                            if crate::canonical::value_bytes(prior)? != image_bytes {
                                return None;
                            }
                        }
                        nulls.insert(*id, image.clone());
                    }
                    other => {
                        if crate::canonical::value_bytes(other)? != image_bytes {
                            return None;
                        }
                    }
                }
            }
        }
        let hom = Hom { rows, nulls };
        hom.check(domain, codomain).ok()?;
        Some(hom)
    }

    /// Definition: Sigma on a homomorphism of instances of the kept part.
    ///
    /// For `h: I -> I'`, `Sigma(h): Sigma(I) -> Sigma(I')` sends each row of
    /// `Sigma(I)` (which keeps the ids of the rows of `I`) to the image of
    /// that row under `h`; its null map is forced by the cells, so it is the
    /// unique homomorphism with that row map. Both Sigma results are
    /// [`sigma_restricted`].
    ///
    /// `None` when either Sigma refuses, `h` is no homomorphism `I -> I'`, or
    /// no homomorphism has that row map.
    ///
    /// Book anchors: `L-kan-extensions-2`, `L-kan-extensions-4`.
    #[must_use]
    pub fn sigma_on_hom(
        f: &RelFunctor,
        domain: &Table,
        codomain: &Table,
        hom: &Hom,
    ) -> Option<Hom> {
        let (sigma_domain, _) = sigma_restricted(f, domain).ok()?;
        let (sigma_codomain, _) = sigma_restricted(f, codomain).ok()?;
        hom.check(domain, codomain).ok()?;
        let rows = sigma_domain
            .rows
            .keys()
            .map(|row| Some((*row, *hom.rows.get(row)?)))
            .collect::<Option<_>>()?;
        forced_hom(&sigma_domain, &sigma_codomain, rows)
    }

    /// Definition: Delta on a homomorphism of target instances.
    ///
    /// For `h: J -> J'`, `Delta(h)` sends the copy at `s` of a row `r` to the
    /// copy at `s` of `h(r)`, as [`delta`]'s provenance names the copies; its
    /// null map is forced by the cells.
    ///
    /// `None` when either Delta refuses, `h` is no homomorphism `J -> J'`, or
    /// no homomorphism has that row map.
    ///
    /// Book anchors: `L-kan-extensions-1`, `L-kan-extensions-4`,
    /// `L-kan-extensions-5`.
    #[must_use]
    pub fn delta_on_hom(
        f: &RelFunctor,
        domain: &Table,
        codomain: &Table,
        hom: &Hom,
    ) -> Option<Hom> {
        let (delta_domain, domain_copies) = delta(f, domain).ok()?;
        let (delta_codomain, codomain_copies) = delta(f, codomain).ok()?;
        hom.check(domain, codomain).ok()?;
        let index: BTreeMap<(Name, u32), u32> = codomain_copies
            .iter()
            .map(|(copy, origin)| (origin.clone(), *copy))
            .collect();
        let rows = domain_copies
            .iter()
            .map(|(copy, (table, row))| {
                let image = *hom.rows.get(row)?;
                Some((*copy, *index.get(&(table.clone(), image))?))
            })
            .collect::<Option<_>>()?;
        forced_hom(&delta_domain, &delta_codomain, rows)
    }

    /// Definition: Pi on a homomorphism of instances of the kept part.
    ///
    /// For `h: I -> I'`, `Pi(h)` sends the row of each tuple of
    /// `Pi(I)` to the row of `Pi(I')` whose tuple holds the components'
    /// images under `h`, as [`pi_restricted`]'s provenance names the tuples
    /// (the one row of a table outside the image to the one row); its null
    /// map is forced by the cells.
    ///
    /// `None` when either Pi refuses at `max_product_size`, `h` is no
    /// homomorphism `I -> I'`, or no homomorphism has that row map.
    ///
    /// Book anchors: `L-kan-extensions-3`, `L-kan-extensions-5`.
    #[must_use]
    pub fn pi_on_hom(
        f: &RelFunctor,
        domain: &Table,
        codomain: &Table,
        hom: &Hom,
        max_product_size: usize,
    ) -> Option<Hom> {
        let (pi_domain, domain_tuples) = pi_restricted(f, domain, max_product_size).ok()?;
        let (pi_codomain, codomain_tuples) = pi_restricted(f, codomain, max_product_size).ok()?;
        hom.check(domain, codomain).ok()?;
        let index: BTreeMap<(Name, Vec<u32>), u32> = codomain_tuples
            .iter()
            .map(|(product, origin)| (origin.clone(), *product))
            .collect();
        let rows = domain_tuples
            .iter()
            .map(|(product, (table, tuple))| {
                let image = tuple
                    .iter()
                    .map(|row| hom.rows.get(row).copied())
                    .collect::<Option<Vec<_>>>()?;
                Some((*product, *index.get(&(table.clone(), image))?))
            })
            .collect::<Option<_>>()?;
        forced_hom(&pi_domain, &pi_codomain, rows)
    }
}

pub mod migration {
    //! The compiled migration model: the data a restrict reads.
    //!
    //! The value layer (survival predicates, field transforms, op-term
    //! assignments) is evaluated by the expression language, which the
    //! reference does not re-implement: a migration records only which source
    //! vertices carry a survival predicate and which carry transforms, and the
    //! operations that evaluate them take a [`crate::restrict::ValueLayer`]
    //! supplied by the caller.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::model::{Name, SEdge};

    /// A hyper-edge resolver key: the hyper-edge and the set of child labels a
    /// fan carries.
    pub type HyperShape = (Name, BTreeSet<Name>);

    /// A compiled migration.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Migration {
        /// Vertices that survive (by target name).
        pub surviving_verts: BTreeSet<Name>,
        /// Edges that survive.
        pub surviving_edges: BTreeSet<SEdge>,
        /// Source vertex to target vertex.
        pub vertex_remap: BTreeMap<Name, Name>,
        /// Source edge to target edge.
        pub edge_remap: BTreeMap<SEdge, SEdge>,
        /// Contraction resolver, keyed in the target space.
        pub resolver: BTreeMap<(Name, Name), SEdge>,
        /// Expansion paths: target anchor pair to intermediate target anchors.
        pub expansion_path: BTreeMap<(Name, Name), Vec<Name>>,
        /// Hyper-edge resolver: fan shape to target hyper-edge and label map.
        pub hyper_resolver: BTreeMap<HyperShape, (Name, BTreeMap<Name, Name>)>,
        /// Source vertices with a survival predicate.
        pub conditional: BTreeSet<Name>,
        /// Source vertices with field transforms or op-term assignments.
        pub transformed: BTreeSet<Name>,
    }

    impl Migration {
        /// The image of a source vertex: `vertex_remap` extended by the
        /// identity.
        #[must_use]
        pub fn vertex_image<'a>(&'a self, v: &'a str) -> &'a str {
            self.vertex_remap.get(v).map_or(v, String::as_str)
        }

        /// Whether a source vertex survives: its image is a surviving vertex.
        #[must_use]
        pub fn survives(&self, v: &str) -> bool {
            self.surviving_verts.contains(self.vertex_image(v))
        }
    }

    /// Definition: Observable equality for the graph conjugation square.
    ///
    /// Book anchors: `L-kan-extensions-8`.
    #[must_use]
    pub fn graph_square(left: &crate::model::Graph, right: &crate::model::Graph) -> bool {
        left == right
    }

    /// Definition: Observable equality for the table conjugation square.
    ///
    /// Book anchors: `L-kan-extensions-9`.
    #[must_use]
    pub fn table_square(left: &crate::model::Table, right: &crate::model::Table) -> bool {
        left == right
    }

    /// Definition: Observable equality for the tree conjugation square.
    ///
    /// Book anchors: `L-kan-extensions-10`.
    #[must_use]
    pub fn tree_square(left: &crate::model::Tree, right: &crate::model::Tree) -> bool {
        left == right
    }
}

pub use self::encode::{
    g_counit, g_delta, g_delta_along, g_delta_along_with_provenance, g_delta_with_provenance, g_pi,
    g_pi_along, g_pi_along_with_provenance, g_pi_counit, g_pi_restricted,
    g_pi_restricted_with_provenance, g_pi_transpose_left, g_pi_transpose_right, g_pi_unit,
    g_pi_with_provenance, g_sigma, g_sigma_along, g_sigma_along_with_provenance,
    g_sigma_restricted, g_sigma_restricted_with_provenance, g_sigma_with_provenance,
    g_transpose_left, g_transpose_right, g_unit, t_delta, t_pi, t_sigma,
};
pub use self::migration::{graph_square, table_square, tree_square};
pub use self::rel::{delta, homs, pi, sigma};
