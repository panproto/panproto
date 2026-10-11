//! Complete reference operations assigned to `representation`.

pub mod canonical {
    //! Representation-specific canonical encoders and left inverses.

    use std::collections::{BTreeMap, BTreeSet};

    use crate::model::{Fan, GEdge, Graph, Node, NodeShape, Table, Tree};
    use crate::pages::values::canonical::{
        Reader, Writer, read_map, read_value, value_bytes, write_map, write_value,
    };
    use crate::value::{Presence, Value};

    /// The instance shape whose complement an identity envelope accompanies.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum ComplementShape {
        /// A tree/W-type complement.
        WType,
        /// A graph complement.
        Graph,
        /// A table/functor complement.
        Functor,
        /// A reference-resolution complement.
        Resolution,
    }

    /// The inputs to the version-2 identity-envelope encoding.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct IdentityEnvelope {
        /// The source schema digest.
        pub source_schema: [u8; 32],
        /// The target schema digest.
        pub target_schema: [u8; 32],
        /// The compiled lens digest.
        pub lens: [u8; 32],
        /// The complement shape.
        pub shape: ComplementShape,
        /// The shape-specific complement-format version.
        pub format_version: u16,
    }

    /// Definition: Encode a version-2 identity envelope exactly as the segmented canonical
    /// sink does.
    ///
    /// The layout is little-endian version 2, three 32-byte digests, the shape
    /// tag (`WType = 0`, `Graph = 1`, `Functor = 2`, `Resolution = 3`), and the
    /// little-endian complement-format version.
    ///
    /// Book anchors: `L-representation-5`.
    #[must_use]
    pub fn identity(envelope: &IdentityEnvelope) -> [u8; 101] {
        let mut bytes = [0; 101];
        bytes[..2].copy_from_slice(&2u16.to_le_bytes());
        bytes[2..34].copy_from_slice(&envelope.source_schema);
        bytes[34..66].copy_from_slice(&envelope.target_schema);
        bytes[66..98].copy_from_slice(&envelope.lens);
        bytes[98] = match envelope.shape {
            ComplementShape::WType => 0,
            ComplementShape::Graph => 1,
            ComplementShape::Functor => 2,
            ComplementShape::Resolution => 3,
        };
        bytes[99..].copy_from_slice(&envelope.format_version.to_le_bytes());
        bytes
    }

    /// The canonical order of values: the byte order of their canonical bytes
    /// (a value outside the layout sorts before every value inside it).
    #[must_use]
    pub fn canonical_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
        value_bytes(a).cmp(&value_bytes(b))
    }

    /// A node record.
    ///
    /// Id, anchor, value (0 none, 1 present and the value, 2 null, 3 absent), discriminator, fields,
    /// position, shape (0 plain, 1 list, 2 XML element and its tag, 3 text segment, 4 hyper-edge) and
    /// annotations.
    pub fn write_node(w: &mut Writer, n: &Node) -> Option<()> {
        w.u32(n.id);
        w.str(&n.anchor);
        match &n.value {
            None => w.tag(0),
            Some(Presence::Present(v)) => {
                w.tag(1);
                write_value(w, v)?;
            }
            Some(Presence::Null) => w.tag(2),
            Some(Presence::Absent) => w.tag(3),
        }
        w.opt_name(n.discriminator.as_deref());
        write_map(w, &n.fields)?;
        w.opt_u32(n.position);
        match &n.shape {
            NodeShape::Plain => w.tag(0),
            NodeShape::List => w.tag(1),
            NodeShape::XmlElement(tag) => {
                w.tag(2);
                w.str(tag);
            }
            NodeShape::XmlTextSegment => w.tag(3),
            NodeShape::HyperEdge => w.tag(4),
        }
        write_map(w, &n.annotations)
    }

    /// Write one canonical graph edge record `(id, src, tgt, label, position)`.
    pub fn write_graph_edge(w: &mut Writer, id: u32, edge: &GEdge) {
        w.u32(id);
        w.u32(edge.src);
        w.u32(edge.tgt);
        w.edge(&edge.label);
        w.opt_u32(edge.position);
    }

    /// Read one canonical graph edge record.
    pub fn read_graph_edge(r: &mut Reader<'_>) -> Option<(u32, GEdge)> {
        let id = r.u32()?;
        Some((
            id,
            GEdge {
                src: r.u32()?,
                tgt: r.u32()?,
                label: r.edge()?,
                position: r.opt_u32()?,
            },
        ))
    }

    /// The inverse of [`write_node`].
    pub fn read_node(r: &mut Reader<'_>) -> Option<Node> {
        let id = r.u32()?;
        let anchor = r.str()?;
        let value = match r.tag()? {
            0 => None,
            1 => Some(Presence::Present(read_value(r)?)),
            2 => Some(Presence::Null),
            3 => Some(Presence::Absent),
            _ => return None,
        };
        let discriminator = r.opt_name()?;
        let fields = read_map(r)?;
        let position = r.opt_u32()?;
        let shape = match r.tag()? {
            0 => NodeShape::Plain,
            1 => NodeShape::List,
            2 => NodeShape::XmlElement(r.str()?),
            3 => NodeShape::XmlTextSegment,
            4 => NodeShape::HyperEdge,
            _ => return None,
        };
        let annotations = read_map(r)?;
        Some(Node {
            id,
            anchor,
            value,
            discriminator,
            fields,
            position,
            shape,
            annotations,
        })
    }

    /// The canonical bytes of a graph: node and edge counts, node records in
    /// ascending id, edge records `(id, src, tgt, label, position)` in
    /// ascending edge id.
    #[must_use]
    pub fn graph_bytes(g: &Graph) -> Option<Vec<u8>> {
        let mut w = Writer::default();
        w.len(g.nodes.len());
        w.len(g.edges.len());
        for n in g.nodes.values() {
            write_node(&mut w, n)?;
        }
        for (id, e) in &g.edges {
            write_graph_edge(&mut w, *id, e);
        }
        Some(w.bytes)
    }

    /// Definition: The inverse of [`graph_bytes`], refusing records out of ascending order,
    /// dangling endpoints and trailing bytes.
    ///
    /// Book anchors: `L-representation-4`.
    #[must_use]
    pub fn decode_graph(bytes: &[u8]) -> Option<Graph> {
        let mut r = Reader::new(bytes);
        let (nn, ne) = (r.count()?, r.count()?);
        let mut g = Graph::default();
        for _ in 0..nn {
            let n = read_node(&mut r)?;
            if g.nodes.keys().next_back().is_some_and(|last| *last >= n.id) {
                return None;
            }
            g.nodes.insert(n.id, n);
        }
        for _ in 0..ne {
            let (id, e) = read_graph_edge(&mut r)?;
            if g.edges.keys().next_back().is_some_and(|last| *last >= id) {
                return None;
            }
            g.edges.insert(id, e);
        }
        (r.done() && g.well_formed()).then_some(g)
    }

    /// The canonical bytes of a table: row and pair counts, row records
    /// `(id, table, cells)` in ascending id, pair records `(edge, src, tgt)` in
    /// ascending order.
    #[must_use]
    pub fn table_bytes(t: &Table) -> Option<Vec<u8>> {
        let mut w = Writer::default();
        w.len(t.rows.len());
        w.len(t.pairs.len());
        for (id, (table, cells)) in &t.rows {
            w.u32(*id);
            w.str(table);
            write_map(&mut w, cells)?;
        }
        for (e, s, d) in &t.pairs {
            w.edge(e);
            w.u32(*s);
            w.u32(*d);
        }
        Some(w.bytes)
    }

    /// Definition: The inverse of [`table_bytes`].
    ///
    /// Book anchors: `L-representation-4`.
    #[must_use]
    pub fn decode_table(bytes: &[u8]) -> Option<Table> {
        let mut r = Reader::new(bytes);
        let (nr, np) = (r.count()?, r.count()?);
        let mut t = Table::default();
        for _ in 0..nr {
            let id = r.u32()?;
            if t.rows.keys().next_back().is_some_and(|last| *last >= id) {
                return None;
            }
            let table = r.str()?;
            t.rows.insert(id, (table, read_map(&mut r)?));
        }
        let mut pairs = Vec::new();
        for _ in 0..np {
            pairs.push((r.edge()?, r.u32()?, r.u32()?));
        }
        if pairs.windows(2).any(|w| w[0] >= w[1]) {
            return None;
        }
        t.pairs = pairs.into_iter().collect::<BTreeSet<_>>();
        (r.done() && t.well_formed()).then_some(t)
    }

    /// A fan's canonical record: parent, hyper-edge, children by label.
    fn fan_record(f: &Fan) -> (u32, String, Vec<(String, u32)>) {
        (
            f.parent,
            f.hyper_edge.clone(),
            f.children.iter().map(|(l, n)| (l.clone(), *n)).collect(),
        )
    }

    /// The canonical bytes of a tree.
    ///
    /// Node, arc and fan counts, node records in ascending id, arc records `(parent, child, label)` in
    /// arc order, fan records in ascending record order. The root and the fans' listed order are not
    /// written.
    #[must_use]
    pub fn tree_bytes(t: &Tree) -> Option<Vec<u8>> {
        let mut w = Writer::default();
        w.len(t.nodes.len());
        w.len(t.arcs.len());
        w.len(t.fans.len());
        for n in t.nodes.values() {
            write_node(&mut w, n)?;
        }
        for (p, c, e) in &t.arcs {
            w.u32(*p);
            w.u32(*c);
            w.edge(e);
        }
        let mut fans: Vec<_> = t.fans.iter().map(fan_record).collect();
        fans.sort();
        for (p, h, ch) in fans {
            w.u32(p);
            w.str(&h);
            w.len(ch.len());
            for (l, n) in ch {
                w.str(&l);
                w.u32(n);
            }
        }
        Some(w.bytes)
    }

    /// Definition: The inverse of [`tree_bytes`] up to what the bytes do not hold: the
    /// fans come back in record order and the root is the one node no arc
    /// enters (`None` when that node is not unique).
    ///
    /// Book anchors: `L-representation-4`.
    #[must_use]
    pub fn decode_tree(bytes: &[u8]) -> Option<Tree> {
        let mut r = Reader::new(bytes);
        let (nn, na, nf) = (r.count()?, r.count()?, r.count()?);
        let mut nodes = BTreeMap::new();
        for _ in 0..nn {
            let n = read_node(&mut r)?;
            if nodes
                .keys()
                .next_back()
                .is_some_and(|last: &u32| *last >= n.id)
            {
                return None;
            }
            nodes.insert(n.id, n);
        }
        let mut arcs = Vec::new();
        for _ in 0..na {
            arcs.push((r.u32()?, r.u32()?, r.edge()?));
        }
        let mut fans = Vec::new();
        let mut last = None;
        for _ in 0..nf {
            let parent = r.u32()?;
            let hyper_edge = r.str()?;
            let k = r.count()?;
            let mut children = Vec::new();
            for _ in 0..k {
                children.push((r.str()?, r.u32()?));
            }
            if children.windows(2).any(|w| w[0].0 >= w[1].0) {
                return None;
            }
            let rec = (parent, hyper_edge.clone(), children.clone());
            if last.as_ref().is_some_and(|l| *l > rec) {
                return None;
            }
            last = Some(rec);
            fans.push(Fan {
                parent,
                hyper_edge,
                children: children.into_iter().collect(),
            });
        }
        let roots: Vec<u32> = nodes
            .keys()
            .copied()
            .filter(|n| !arcs.iter().any(|(_, c, _)| c == n))
            .collect();
        let [root] = roots.as_slice() else {
            return None;
        };
        r.done().then_some(Tree {
            nodes,
            arcs,
            fans,
            root: *root,
        })
    }
}

pub mod encode {
    //! Encodings between shapes: trees and tables as graphs, and graphs as
    //! attributed presheaves for the Kan extensions (encode, presheaf
    //! operation, decode).

    use std::collections::BTreeMap;

    use crate::model::{Fan, GEdge, Graph, Name, Node, NodeShape, SEdge, Table, Tree};
    use crate::restrict::{HYPER_LABEL, HYPER_PARENT, is_fan_kind};
    use crate::schema::Schema;

    /// How a tree's arcs and fans sit in its graph.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct TreeEmbedding {
        /// Child node to the edge id of its arc.
        pub arcs: BTreeMap<u32, u32>,
        /// Per fan, in the tree's fan order: its node, its parent edge and its
        /// child edges by label.
        pub fans: Vec<(u32, u32, BTreeMap<Name, u32>)>,
        /// The next free node id.
        pub next_node: u32,
        /// The next free edge id.
        pub next_edge: u32,
    }

    /// Why a tree has no graph form.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TreeEncodeError {
        /// A node of the fan shape or an arc of a reserved kind.
        ReservedVocabulary,
        /// A node with two parent arcs.
        SecondParent {
            /// The node.
            node: u32,
        },
        /// A fan names a node the tree does not hold.
        DanglingFan,
        /// An arc endpoint is not a node.
        DanglingArc,
        /// No node id below the reserved maximum remains for a fan.
        NodeIdsExhausted,
        /// No edge id below the reserved maximum remains for an arc or fan edge.
        EdgeIdsExhausted,
    }

    /// Draw the counter's current id and advance it, reserving `u32::MAX` as
    /// the exhausted sentinel because no following `u32` counter exists.
    fn take_id<E>(counter: &mut u32, exhausted: E) -> Result<u32, E> {
        let id = *counter;
        let next = id.checked_add(1).ok_or(exhausted)?;
        *counter = next;
        Ok(id)
    }

    /// Definition: The graph form of a tree.
    ///
    /// Arc i (in arc order) becomes edge i with no position; fan k becomes node `max node id + 1 + k`
    /// of the hyper-edge shape, anchored at its hyper-edge, with its edges after every arc's, fan by
    /// fan, the parent edge first (named by the hyper-edge's parent label when `schema` declares the
    /// hyper-edge) and then the child edges in ascending label; a fan edge's label runs from the
    /// hyper-edge to the anchor of the node it reaches.
    ///
    /// # Errors
    ///
    /// [`TreeEncodeError`] for reserved vocabulary, a second parent, a
    /// dangling arc or fan, or exhausted node or edge ids. The id `u32::MAX`
    /// is reserved and is never allocated.
    ///
    /// Book anchors: `L-representation-1`.
    pub fn tree_to_graph(
        t: &Tree,
        schema: Option<&Schema>,
    ) -> Result<(Graph, TreeEmbedding), TreeEncodeError> {
        let mut g = Graph {
            nodes: t.nodes.clone(),
            edges: BTreeMap::new(),
        };
        if t.nodes.values().any(|n| n.shape == NodeShape::HyperEdge)
            || t.arcs.iter().any(|(_, _, e)| is_fan_kind(&e.kind))
        {
            return Err(TreeEncodeError::ReservedVocabulary);
        }
        let mut emb = TreeEmbedding::default();
        let mut next_edge = 0u32;
        for (p, c, e) in &t.arcs {
            if !t.nodes.contains_key(p) || !t.nodes.contains_key(c) {
                return Err(TreeEncodeError::DanglingArc);
            }
            if emb.arcs.contains_key(c) {
                return Err(TreeEncodeError::SecondParent { node: *c });
            }
            let edge_id = take_id(&mut next_edge, TreeEncodeError::EdgeIdsExhausted)?;
            g.edges.insert(
                edge_id,
                GEdge {
                    src: *p,
                    tgt: *c,
                    label: e.clone(),
                    position: None,
                },
            );
            emb.arcs.insert(*c, edge_id);
        }
        let mut next_node = crate::restrict::floor_of(t.nodes.keys().copied());
        for f in &t.fans {
            let id = take_id(&mut next_node, TreeEncodeError::NodeIdsExhausted)?;
            let parent_name = schema
                .and_then(|s| s.hyper_edges.get(&f.hyper_edge))
                .map(|h| h.parent_label.clone());
            let anchor_of = |n: u32| {
                t.nodes
                    .get(&n)
                    .map(|x| x.anchor.clone())
                    .ok_or(TreeEncodeError::DanglingFan)
            };
            let parent_label = SEdge {
                src: f.hyper_edge.clone(),
                tgt: anchor_of(f.parent)?,
                kind: HYPER_PARENT.into(),
                name: parent_name,
            };
            let parent_edge = take_id(&mut next_edge, TreeEncodeError::EdgeIdsExhausted)?;
            g.edges.insert(
                parent_edge,
                GEdge {
                    src: id,
                    tgt: f.parent,
                    label: parent_label,
                    position: None,
                },
            );
            let mut children = BTreeMap::new();
            for (l, c) in &f.children {
                let label = SEdge {
                    src: f.hyper_edge.clone(),
                    tgt: anchor_of(*c)?,
                    kind: HYPER_LABEL.into(),
                    name: Some(l.clone()),
                };
                let child_edge = take_id(&mut next_edge, TreeEncodeError::EdgeIdsExhausted)?;
                g.edges.insert(
                    child_edge,
                    GEdge {
                        src: id,
                        tgt: *c,
                        label,
                        position: None,
                    },
                );
                children.insert(l.clone(), child_edge);
            }
            let mut node = Node::new(id, &f.hyper_edge);
            node.shape = NodeShape::HyperEdge;
            g.nodes.insert(id, node);
            emb.fans.push((id, parent_edge, children));
        }
        emb.next_node = next_node;
        emb.next_edge = next_edge;
        Ok((g, emb))
    }

    /// Why a graph has no tree form.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum NotATree {
        /// The root is not a node of the graph, or is a fan node.
        MissingRoot,
        /// An edge whose source or target is not a node.
        DanglingEdge {
            /// The edge.
            edge: u32,
        },
        /// An edge with a position (the graph form of a tree positions none).
        PositionedEdge {
            /// The edge.
            edge: u32,
        },
        /// An edge of a reserved kind leaving a node that is not a fan node.
        ReservedArc {
            /// The edge.
            edge: u32,
        },
        /// An edge from a node that is not a fan node into a fan node.
        ArcIntoFan {
            /// The edge.
            edge: u32,
        },
        /// An arc into the root.
        RootHasParent {
            /// The edge.
            edge: u32,
        },
        /// A node with two parent arcs.
        SecondParent {
            /// The node.
            node: u32,
        },
        /// A fan edge whose label is not the one the graph form writes: from
        /// the fan's hyper-edge to the anchor of the node it reaches, a parent
        /// edge named as the hyper-edge's parent label.
        FanLabel {
            /// The edge.
            edge: u32,
        },
        /// A fan node carrying a value, discriminator, field, position or
        /// annotation, which the graph form never writes.
        FanData {
            /// The fan node.
            node: u32,
        },
        /// A fan node that is not a well-formed fan.
        Fan {
            /// The fan node.
            node: u32,
            /// Its first fan violation.
            violation: crate::fan::FanViolation,
        },
        /// A node that does not reach the root along its parent arcs.
        Unreached {
            /// The node.
            node: u32,
        },
    }

    /// The parent-edge names of a schema's hyper-edges, as [`tree_to_graph`]
    /// writes them: the hyper-edge's parent label when the schema declares the
    /// hyper-edge, else none.
    pub fn schema_parent_name(schema: Option<&Schema>) -> impl Fn(&str) -> Option<Name> + '_ {
        move |hyper: &str| {
            schema
                .and_then(|s| s.hyper_edges.get(hyper))
                .map(|h| h.parent_label.clone())
        }
    }

    /// The checks of [`graph_to_tree`] on each edge, ascending.
    pub(crate) fn tree_form_edges(
        g: &Graph,
        root: u32,
        is_fan: &dyn Fn(u32) -> bool,
        parent_name: &dyn Fn(&str) -> Option<Name>,
    ) -> Result<BTreeMap<u32, u32>, NotATree> {
        let mut parent: BTreeMap<u32, u32> = BTreeMap::new();
        for (id, e) in &g.edges {
            let edge = *id;
            let (Some(from), Some(to)) = (g.nodes.get(&e.src), g.nodes.get(&e.tgt)) else {
                return Err(NotATree::DanglingEdge { edge });
            };
            if e.position.is_some() {
                return Err(NotATree::PositionedEdge { edge });
            }
            if is_fan(e.src) {
                let expected_name = if e.label.kind == HYPER_PARENT {
                    parent_name(&from.anchor)
                } else {
                    e.label.name.clone()
                };
                if e.label.src != from.anchor
                    || e.label.tgt != to.anchor
                    || e.label.name != expected_name
                {
                    return Err(NotATree::FanLabel { edge });
                }
                continue;
            }
            if is_fan_kind(&e.label.kind) {
                return Err(NotATree::ReservedArc { edge });
            }
            if is_fan(e.tgt) {
                return Err(NotATree::ArcIntoFan { edge });
            }
            if e.tgt == root {
                return Err(NotATree::RootHasParent { edge });
            }
            if parent.insert(e.tgt, e.src).is_some() {
                return Err(NotATree::SecondParent { node: e.tgt });
            }
        }
        Ok(parent)
    }

    /// Definition: The tree form of a graph with this root.
    ///
    /// Definition: the inverse of [`tree_to_graph`] on its image, up to the
    /// ids of fan nodes and edges. A graph has a tree form iff the root is a
    /// node that is no fan node; every edge joins two nodes and has no
    /// position; an edge out of a fan node (a node of the hyper-edge shape) is
    /// labelled from the fan's anchor to the anchor of its target, a parent
    /// edge named by `parent_name` of the fan's anchor (as [`tree_to_graph`]
    /// names it under the same schema, [`schema_parent_name`]); every other
    /// edge is an arc: of no reserved kind, into a node that is neither a fan
    /// node nor the root, and the only arc into its target; every fan node
    /// carries no data and is a well-formed fan ([`crate::fan::violations_at`],
    /// no schema); and every node that is no fan node reaches the root along
    /// its parent arcs. Fan nodes become fans in ascending fan node id (parent
    /// the target of its parent edge, children by label), and arcs keep
    /// ascending edge id order.
    ///
    /// # Errors
    ///
    /// The first [`NotATree`] reason: the root, then each edge in ascending
    /// id, then each fan node in ascending id, then each other node.
    ///
    /// Book anchors: `L-representation-2`.
    pub fn graph_to_tree(
        g: &Graph,
        root: u32,
        parent_name: &dyn Fn(&str) -> Option<Name>,
    ) -> Result<Tree, NotATree> {
        let is_fan = |n: u32| {
            g.nodes
                .get(&n)
                .is_some_and(|x| x.shape == NodeShape::HyperEdge)
        };
        if !g.nodes.contains_key(&root) || is_fan(root) {
            return Err(NotATree::MissingRoot);
        }
        let parent = tree_form_edges(g, root, &is_fan, parent_name)?;
        let mut fans = Vec::new();
        for (f, node) in g.nodes.iter().filter(|(i, _)| is_fan(**i)) {
            let bare = node.value.is_none()
                && node.discriminator.is_none()
                && node.fields.is_empty()
                && node.position.is_none()
                && node.annotations.is_empty();
            if !bare {
                return Err(NotATree::FanData { node: *f });
            }
            if let Some(violation) = crate::fan::violations_at(g, *f, None).into_iter().next() {
                return Err(NotATree::Fan {
                    node: *f,
                    violation,
                });
            }
            let out: Vec<&GEdge> = g.edges.values().filter(|e| e.src == *f).collect();
            let parent_of_fan = out
                .iter()
                .find(|e| e.label.kind == HYPER_PARENT)
                .map_or(*f, |e| e.tgt);
            let children: BTreeMap<Name, u32> = out
                .iter()
                .filter(|e| e.label.kind == HYPER_LABEL)
                .filter_map(|e| e.label.name.clone().map(|l| (l, e.tgt)))
                .collect();
            fans.push(Fan {
                parent: parent_of_fan,
                hyper_edge: node.anchor.clone(),
                children,
            });
        }
        let nodes: BTreeMap<u32, Node> = g
            .nodes
            .iter()
            .filter(|(i, _)| !is_fan(**i))
            .map(|(i, n)| (*i, n.clone()))
            .collect();
        for n in nodes.keys() {
            let mut cur = *n;
            let mut steps = 0usize;
            while cur != root {
                match parent.get(&cur) {
                    Some(p) if steps < nodes.len() => {
                        cur = *p;
                        steps += 1;
                    }
                    _ => return Err(NotATree::Unreached { node: *n }),
                }
            }
        }
        let arcs = g
            .edges
            .values()
            .filter(|e| !is_fan(e.src))
            .map(|e| (e.src, e.tgt, e.label.clone()))
            .collect();
        Ok(Tree {
            nodes,
            arcs,
            fans,
            root,
        })
    }

    /// Definition: Reify every fan in a tree through its graph representation.
    ///
    /// The result is defined exactly when both representation directions are
    /// defined; this makes the image round trip directly executable.
    ///
    /// Book anchors: `L-representation-3`.
    #[must_use]
    pub fn reify(tree: &Tree, schema: Option<&Schema>) -> Option<Tree> {
        let (graph, _) = tree_to_graph(tree, schema).ok()?;
        let parent_name = schema_parent_name(schema);
        graph_to_tree(&graph, tree.root, &parent_name).ok()
    }

    /// Why a table has no graph form.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum TableEncodeError {
        /// A relation pair names a row absent from the table.
        MalformedTable {
            /// The first dangling pair in ascending order.
            pair: Box<(SEdge, u32, u32)>,
        },
        /// The first pair for which no edge id below the reserved maximum
        /// remains.
        EdgeIdsExhausted {
            /// The unrepresentable pair.
            pair: Box<(SEdge, u32, u32)>,
        },
    }

    /// Pair-to-edge-id record returned by [`table_to_graph`].
    pub type TableEmbedding = BTreeMap<(SEdge, u32, u32), u32>;

    /// The graph form of a table.
    ///
    /// Definition: on a [`Table::well_formed`] input, row r is node r anchored at its
    /// table with its cells as fields; pair (e, s, t) is an edge s -> t labelled
    /// e with no position, edge ids from 0 in ascending `(e, s, t)`. Returns the
    /// pair ids as the embedding. The id `u32::MAX` is reserved and is never
    /// allocated.
    ///
    /// # Errors
    ///
    /// [`TableEncodeError::MalformedTable`] for the first dangling pair, then
    /// [`TableEncodeError::EdgeIdsExhausted`] for the first pair beyond the
    /// representable edge-id carrier.
    ///
    /// Book anchors: `L-representation-6`.
    pub fn table_to_graph(t: &Table) -> Result<(Graph, TableEmbedding), TableEncodeError> {
        if let Some(pair) = t
            .pairs
            .iter()
            .find(|(_, src, tgt)| !t.rows.contains_key(src) || !t.rows.contains_key(tgt))
        {
            return Err(TableEncodeError::MalformedTable {
                pair: Box::new(pair.clone()),
            });
        }
        let mut g = Graph::default();
        for (id, (table, cells)) in &t.rows {
            let mut n = Node::new(*id, table);
            n.fields.clone_from(cells);
            g.nodes.insert(*id, n);
        }
        let mut emb = BTreeMap::new();
        let mut next_edge = 0;
        for (e, s, d) in &t.pairs {
            let id = take_id(
                &mut next_edge,
                TableEncodeError::EdgeIdsExhausted {
                    pair: Box::new((e.clone(), *s, *d)),
                },
            )?;
            g.edges.insert(
                id,
                GEdge {
                    src: *s,
                    tgt: *d,
                    label: e.clone(),
                    position: None,
                },
            );
            emb.insert((e.clone(), *s, *d), id);
        }
        Ok((g, emb))
    }

    /// Definition: The table form of a table-shaped graph.
    ///
    /// Every node a plain row (no value, discriminator, position, annotations or non-plain shape),
    /// every edge unpositioned, no two edges with one label and endpoints. `None` otherwise.
    ///
    /// Book anchors: `L-representation-7`.
    #[must_use]
    pub fn graph_to_table(g: &Graph) -> Option<Table> {
        if !g.well_formed() {
            return None;
        }
        let mut t = Table::default();
        for (id, n) in &g.nodes {
            let plain = n.value.is_none()
                && n.discriminator.is_none()
                && n.position.is_none()
                && n.annotations.is_empty()
                && n.shape == NodeShape::Plain;
            if !plain {
                return None;
            }
            t.rows.insert(*id, (n.anchor.clone(), n.fields.clone()));
        }
        for e in g.edges.values() {
            if e.position.is_some() || !t.pairs.insert((e.label.clone(), e.src, e.tgt)) {
                return None;
            }
        }
        Some(t)
    }

    #[cfg(test)]
    mod allocation_tests {
        use super::{TableEncodeError, TreeEncodeError, take_id};

        fn table_exhausted() -> TableEncodeError {
            TableEncodeError::EdgeIdsExhausted {
                pair: Box::new((crate::model::SEdge::new("a", "b", "e", None), 0, 1)),
            }
        }

        #[test]
        fn representation_counters_allocate_max_minus_one_but_never_max() {
            let mut nodes = u32::MAX - 1;
            assert_eq!(
                take_id(&mut nodes, TreeEncodeError::NodeIdsExhausted),
                Ok(u32::MAX - 1)
            );
            assert_eq!(nodes, u32::MAX);
            assert_eq!(
                take_id(&mut nodes, TreeEncodeError::NodeIdsExhausted),
                Err(TreeEncodeError::NodeIdsExhausted)
            );

            let mut edges = u32::MAX - 1;
            assert_eq!(take_id(&mut edges, table_exhausted()), Ok(u32::MAX - 1));
            assert_eq!(edges, u32::MAX);
            assert_eq!(
                take_id(&mut edges, table_exhausted()),
                Err(table_exhausted())
            );
        }
    }
}

pub mod fan {
    //! The fan domain: reified hyper-edge occurrences in graphs.
    //!
    //! A fan node is well formed iff it has exactly one out-edge of kind
    //! `hyper_parent`, at most one out-edge of kind `hyper_label` per label, no
    //! other out-edge, and no in-edge. The fan domain is the set of graphs in
    //! which every node of the hyper-edge shape, and every source of a reserved
    //! edge kind, is a well-formed fan; with a schema, a fan's hyper-edge must
    //! be declared and its labels must be in its signature.

    use std::collections::BTreeMap;

    use crate::model::{Graph, Name, NodeShape};
    use crate::restrict::{HYPER_LABEL, HYPER_PARENT, is_fan_kind};
    use crate::schema::Schema;

    /// A violation of the fan domain at one node.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum FanViolation {
        /// No parent edge.
        MissingParent,
        /// Several parent edges.
        MultipleParents {
            /// Their ids.
            edges: Vec<u32>,
        },
        /// Several child edges with one label.
        DuplicateLabel {
            /// The label.
            label: Name,
            /// Their ids.
            edges: Vec<u32>,
        },
        /// A label outside the hyper-edge's signature.
        UnknownLabel {
            /// The label.
            label: Name,
        },
        /// A reserved-kind edge out of a node that is not of the fan shape.
        ReservedKindOutsideFan {
            /// The edge.
            edge: u32,
        },
        /// An in-edge, or an out-edge of no fan kind (or an unnamed child edge).
        ForeignIncidentEdge {
            /// The edge.
            edge: u32,
        },
        /// The fan's hyper-edge is not declared.
        UnknownHyperEdge {
            /// The hyper-edge.
            hyper_edge: Name,
        },
    }

    /// Definition: Every violation of the fan rules at `node`, in this order.
    ///
    /// For a node not of the fan shape, each reserved-kind out-edge; otherwise each in-edge, each
    /// out-edge of no fan kind, the parent count, each duplicated label in label order, and with a
    /// schema an undeclared hyper-edge or each label outside its signature.
    ///
    /// Book anchors: `L-representation-8`.
    #[must_use]
    pub fn violations_at(graph: &Graph, node: u32, schema: Option<&Schema>) -> Vec<FanViolation> {
        let Some(fan) = graph.nodes.get(&node) else {
            return Vec::new();
        };
        let out: Vec<u32> = graph
            .edges
            .iter()
            .filter(|(_, edge)| edge.src == node)
            .map(|(id, _)| *id)
            .collect();
        if fan.shape != NodeShape::HyperEdge {
            return out
                .into_iter()
                .filter(|id| is_fan_kind(&graph.edges[id].label.kind))
                .map(|edge| FanViolation::ReservedKindOutsideFan { edge })
                .collect();
        }
        let mut found: Vec<FanViolation> = graph
            .edges
            .iter()
            .filter(|(_, edge)| edge.tgt == node)
            .map(|(id, _)| FanViolation::ForeignIncidentEdge { edge: *id })
            .collect();
        let mut parents = Vec::new();
        let mut labels: BTreeMap<Name, Vec<u32>> = BTreeMap::new();
        for id in out {
            let edge = &graph.edges[&id];
            match (edge.label.kind.as_str(), &edge.label.name) {
                (HYPER_PARENT, _) => parents.push(id),
                (HYPER_LABEL, Some(label)) => labels.entry(label.clone()).or_default().push(id),
                _ => found.push(FanViolation::ForeignIncidentEdge { edge: id }),
            }
        }
        match parents.len() {
            0 => found.push(FanViolation::MissingParent),
            1 => {}
            _ => found.push(FanViolation::MultipleParents { edges: parents }),
        }
        for (label, edges) in &labels {
            if edges.len() > 1 {
                found.push(FanViolation::DuplicateLabel {
                    label: label.clone(),
                    edges: edges.clone(),
                });
            }
        }
        if let Some(decl) = schema {
            match decl.hyper_edges.get(&fan.anchor) {
                None => found.push(FanViolation::UnknownHyperEdge {
                    hyper_edge: fan.anchor.clone(),
                }),
                Some(hyper) => found.extend(
                    labels
                        .keys()
                        .filter(|label| !hyper.signature.contains_key(*label))
                        .map(|label| FanViolation::UnknownLabel {
                            label: label.clone(),
                        }),
                ),
            }
        }
        found
    }

    /// The nodes that must be fans: every node of the fan shape and every
    /// source of a reserved-kind edge, ascending.
    #[must_use]
    pub fn candidates(g: &Graph) -> Vec<u32> {
        g.nodes
            .keys()
            .copied()
            .filter(|n| {
                g.nodes[n].shape == NodeShape::HyperEdge
                    || g.edges
                        .values()
                        .any(|e| e.src == *n && is_fan_kind(&e.label.kind))
            })
            .collect()
    }

    /// The first violation of the fan domain: the least offending node and its
    /// first violation.
    #[must_use]
    pub fn first_violation(g: &Graph, schema: Option<&Schema>) -> Option<(u32, FanViolation)> {
        candidates(g).into_iter().find_map(|n| {
            violations_at(g, n, schema)
                .into_iter()
                .next()
                .map(|v| (n, v))
        })
    }
}

pub use self::canonical::{decode_graph, decode_table, decode_tree, identity};
pub use self::encode::{graph_to_table, graph_to_tree, reify, table_to_graph, tree_to_graph};
pub use crate::canonical::decode_value;
