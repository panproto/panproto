//! Complete reference operations assigned to `kan-extensions`.

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

    fn rooted(g: &Graph, side: &GraphSide, n: u32) -> bool {
        let mut cur = n;
        for _ in 0..=g.nodes.len() {
            let ins: Vec<&GEdge> = g.edges.values().filter(|e| e.tgt == cur).collect();
            let [one] = ins.as_slice() else { return false };
            let Some(parent) = g.nodes.get(&one.src) else {
                return false;
            };
            if side.tables.contains(&parent.anchor) {
                return true;
            }
            cur = one.src;
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
    pub fn encode_graph(g: &Graph, side: &GraphSide) -> Result<Table, NotEncodable> {
        pass_nodes(g, side)?;
        pass_edges(g, side)?;
        pass_cells(g, side)?;
        pass_values(g, side)?;
        let mut t = Table::default();
        for (id, n) in g
            .nodes
            .iter()
            .filter(|(_, n)| side.tables.contains(&n.anchor))
        {
            let mut cells = BTreeMap::new();
            for (column, arrow) in side.columns.get(&n.anchor).into_iter().flatten() {
                if let Some(e) = g.edges.values().find(|e| e.src == *id && e.label == *arrow) {
                    cells.insert(column.clone(), fold(g, e.tgt, 0));
                }
            }
            t.rows.insert(*id, (n.anchor.clone(), cells));
        }
        for e in g
            .edges
            .values()
            .filter(|e| side.relations.contains(&e.label))
        {
            t.pairs.insert((e.label.clone(), e.src, e.tgt));
        }
        Ok(t)
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
    }

    /// Definition: Decode a conforming, well-formed table over a side without a record.
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
    pub fn decode_table(t: &Table, side: &GraphSide) -> Result<Graph, DecodeTableError> {
        if !t.well_formed() {
            return Err(DecodeTableError::MalformedTable);
        }
        rel::conform(t, &side.table_side()).map_err(DecodeTableError::NonConforming)?;
        let mut g = Graph::default();
        let mut next_node = t
            .rows
            .keys()
            .next_back()
            .map_or(Some(0), |id| id.checked_add(1))
            .filter(|id| *id != u32::MAX);
        let mut next_edge = Some(0u32);
        for (id, (table, _)) in &t.rows {
            g.nodes.insert(*id, Node::new(*id, table));
        }
        for (id, (table, cells)) in &t.rows {
            for (column, v) in cells {
                let Some(arrow) = side.columns.get(table).and_then(|c| c.get(column)) else {
                    continue;
                };
                let node_id = take_id(&mut next_node).ok_or(DecodeTableError::NodeIdsExhausted)?;
                let edge_id = take_id(&mut next_edge).ok_or(DecodeTableError::EdgeIdsExhausted)?;
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
        for (e, s, d) in &t.pairs {
            let edge_id = take_id(&mut next_edge).ok_or(DecodeTableError::EdgeIdsExhausted)?;
            g.edges.insert(
                edge_id,
                GEdge {
                    src: *s,
                    tgt: *d,
                    label: e.clone(),
                    position: None,
                },
            );
        }
        Ok(g)
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
        /// The table functor refused.
        Kan(Box<KanError>),
        /// Decoding the result refused a malformed or nonconforming table, or
        /// exhausted a reserved graph id.
        Decode(DecodeTableError),
    }

    /// Definition: Graph Delta is decode over the kept part of Delta of the encoding over
    /// the target.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`].
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_delta(f: &RelFunctor, h: &Graph) -> Result<Graph, GraphKanError> {
        let t = encode_graph(h, &GraphSide::whole(&f.tgt))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (d, _) = rel::delta(f, &t).map_err(|e| GraphKanError::Kan(Box::new(e)))?;
        decode_table(&d, &GraphSide::kept(f)).map_err(GraphKanError::Decode)
    }

    /// Definition: Graph Sigma is decode over the target of Sigma of the encoding over the
    /// source.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`].
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_sigma(f: &RelFunctor, g: &Graph) -> Result<Graph, GraphKanError> {
        let t = encode_graph(g, &GraphSide::whole(&f.src))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (s, _) = rel::sigma(f, &t).map_err(|e| GraphKanError::Kan(Box::new(e)))?;
        decode_table(&s, &GraphSide::whole(&f.tgt)).map_err(GraphKanError::Decode)
    }

    /// Definition: Graph Pi is decode over the target of Pi of the encoding over the source.
    ///
    /// # Errors
    ///
    /// [`GraphKanError`].
    ///
    /// Book anchors: `L-kan-extensions-8`.
    pub fn g_pi(f: &RelFunctor, g: &Graph) -> Result<Graph, GraphKanError> {
        let t = encode_graph(g, &GraphSide::whole(&f.src))
            .map_err(|e| GraphKanError::NotEncodable(Box::new(e)))?;
        let (p, _) = rel::pi(f, &t).map_err(|e| GraphKanError::Kan(Box::new(e)))?;
        decode_table(&p, &GraphSide::whole(&f.tgt)).map_err(GraphKanError::Decode)
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
        let r = g_sigma(f, &g).map_err(TreeKanError::Graph)?;
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
    pub fn t_pi(f: &RelFunctor, t: &Tree) -> Result<Tree, TreeKanError> {
        let g = tree_graph(t)?;
        let r = g_pi(f, &g).map_err(TreeKanError::Graph)?;
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
        let mut vertices: Vec<(Name, Name)> = f
            .tables
            .iter()
            .map(|(s, d)| (s.clone(), d.clone()))
            .collect();
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
            vertices.push((image.tgt.clone(), tgt.tgt.clone()));
            edges.push((image, tgt));
        }
        vertices.sort();
        vertices.dedup();
        edges.sort();
        edges.dedup();
        if let Some((first, second, target)) = first_collision(vertices) {
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
        let r = g_delta(f, &g).map_err(TreeKanError::Graph)?;
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

    /// A functor between relation-ified schemas, restricted to its kept part.
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
    /// value vertex's image; an unmentioned column is dropped; (4) remaps of
    /// inner value edges are recorded, and one whose image is no target inner
    /// edge, or changes the name, the kind, or an endpoint's kind, is
    /// `ValueStructureRemap`.
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
    ) -> Result<RelFunctor, FunctorError> {
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
            let image = m.edge_remap.get(e).cloned().unwrap_or_else(|| e.clone());
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
        for e in &src.inner_edges {
            if let Some(image) = m.edge_remap.get(e) {
                let kind_of = |schema: &RelSchema, v: &str| schema.kinds.get(v).cloned();
                let ok = tgt.inner_edges.contains(image)
                    && image.name == e.name
                    && image.kind == e.kind
                    && kind_of(tgt, &image.src) == kind_of(src, &e.src)
                    && kind_of(tgt, &image.tgt) == kind_of(src, &e.tgt);
                if !ok {
                    return Err(FunctorError::ValueStructureRemap {
                        edge: Box::new(e.clone()),
                    });
                }
                inner_edges.insert(e.clone(), image.clone());
            }
        }
        Ok(RelFunctor {
            src: src.clone(),
            tgt: tgt.clone(),
            tables,
            relations,
            columns,
            inner_edges,
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
    /// [`KanError::RowIdsExhausted`] when a fresh id would pass `u32::MAX`.
    ///
    /// Book anchors: `L-kan-extensions-3`, `L-kan-extensions-5`.
    pub fn pi(f: &RelFunctor, i: &Table) -> Result<(Table, PiProvenance), KanError> {
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
            for (u, uid) in rows_at(&rho.src) {
                for (v, vid) in rows_at(&rho.tgt) {
                    let ok = carried.iter().all(|x| {
                        let a = fs.iter().position(|s| *s == x.src).map(|k| u[k]);
                        let b = ft.iter().position(|s| *s == x.tgt).map(|k| v[k]);
                        matches!((a, b), (Some(a), Some(b)) if i.pairs.contains(&((*x).clone(), a, b)))
                    });
                    if ok {
                        out.pairs.insert((rho.clone(), uid, vid));
                    }
                }
            }
        }
        Ok((out, prov))
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
    pub fn row_bijection(a: &Table, b: &Table) -> Option<BTreeMap<u32, u32>> {
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
        search_bijection(a, b, &order, 0, &mut map, &mut nulls).then_some(map)
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
    /// every other value compared by canonical bytes. Enumerated naively over all row maps.
    ///
    /// Book anchors: `L-kan-extensions-4`, `L-kan-extensions-5`, `L-kan-extensions-6`, `L-kan-extensions-7`.
    #[must_use]
    pub fn homs(a: &Table, b: &Table) -> Vec<BTreeMap<u32, u32>> {
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
        out: &mut Vec<BTreeMap<u32, u32>>,
    ) {
        let Some(&row) = order.get(pos) else {
            if dom
                .pairs
                .iter()
                .all(|(edge, from, to)| cod.pairs.contains(&(edge.clone(), map[from], map[to])))
            {
                out.push(map.clone());
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

pub use self::encode::{g_delta, g_pi, g_sigma, t_delta, t_pi, t_sigma};
pub use self::migration::{graph_square, table_square, tree_square};
pub use self::rel::{delta, homs, pi, sigma};
