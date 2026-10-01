//! A graph instance as a model of the graph instance theory.
//!
//! [`th_graph_instance`](crate::theories::th_graph_instance) states what a
//! graph instance is over its schema: instance vertices and edges, the
//! schema element each is anchored at, node values, and the fans of its
//! n-ary relations, with the coherence equations that an instance edge's
//! endpoints are anchored at the endpoints of the schema edge it is
//! anchored at. [`graph_instance_model`] reads a [`GInstance`] and its
//! [`Schema`] as a [`Model`] of that theory, so
//! [`verify_model`](panproto_gat::verify_model) checks the coherence
//! equations on it, independently of the instance validator.
//!
//! The carriers are:
//!
//! | sort | carrier |
//! |---|---|
//! | `IVertex` | every node id, fan nodes included, as an integer |
//! | `IEdge` | every edge id other than a fan edge (an edge of a reserved fan kind), as an integer |
//! | `Value` | every node's value, written out, and null |
//! | `Vertex` | every schema vertex, every node's anchor, and both endpoints of every `IEdge`'s label, by name |
//! | `Edge` | every schema edge and every `IEdge`'s label as read through its endpoints' families (below), each as its JSON text |
//! | `FanNode` | every node of the fan shape, as an integer |
//! | `FanPort` | every `hyper_label` edge leaving a fan node, as an integer |
//! | `HyperEdge` | every schema hyper-edge and every fan node's anchor, by name |
//! | `Label` | every signature label of a schema hyper-edge and every port's label, by name |
//!
//! An edge endpoint may be anchored anywhere in the endpoint's family, as
//! the validator's endpoint check reads it: a union vertex stands for each
//! of its variants (and a variant's parent vertex for the variant), a
//! recursion point for the vertex it unfolds to, and a reference vertex for
//! the target of each of its outgoing `ref` edges, and an anchor is in an
//! endpoint's family when either is reachable from the other along these
//! links. Acceptance is not transitive (two variants of one union are not in
//! each other's family), so the model does not identify vertices. It reads
//! each instance edge instead as an instance of its label specialised to
//! its endpoints: `ie_anchor(e)` is the label with each endpoint replaced by
//! the anchor of `e`'s node on that side when that anchor is in the
//! endpoint's family, and kept otherwise. An edge `a -> u` into the union
//! `u` whose target node is anchored at the variant `w` is thus read as an
//! instance of `a -> w`, and its coherence equations hold; were the target
//! anchored at an unrelated vertex, the label keeps `u` and the target
//! equation fails. On a schema without unions, recursion points or
//! reference vertices every family is the endpoint alone, and `ie_anchor`
//! is the label itself.
//!
//! Every operation is total on its carrier except `fan_parent`, which is
//! undefined (an error) at a fan node without exactly one `hyper_parent`
//! out-edge; no equation of the theory applies it. An instance vertex with
//! no value has the value null.
//!
//! ```
//! use panproto_gat::{CheckModelOptions, verify_model};
//! use panproto_inst::{GInstance, Node};
//! use panproto_protocols::graph_instance_model;
//! use panproto_protocols::theories::th_graph_instance;
//! use panproto_schema::{Edge, Schema};
//!
//! let label = Edge { src: "a".into(), tgt: "b".into(), kind: "prop".into(), name: None };
//! let coherent = GInstance::new()
//!     .with_node(Node::new(0, "a"))
//!     .with_node(Node::new(1, "b"))
//!     .with_edge(0, 1, label.clone());
//! let report = verify_model(
//!     &graph_instance_model(&coherent, &Schema::default()),
//!     &th_graph_instance(),
//!     &CheckModelOptions::default(),
//! );
//! assert!(report.status().is_verified());
//!
//! let backwards = GInstance::new()
//!     .with_node(Node::new(0, "b"))
//!     .with_node(Node::new(1, "a"))
//!     .with_edge(0, 1, label);
//! let report = verify_model(
//!     &graph_instance_model(&backwards, &Schema::default()),
//!     &th_graph_instance(),
//!     &CheckModelOptions::default(),
//! );
//! assert_eq!(report.violations.len(), 2);
//! ```

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::{GatError, Model, ModelValue, Name};
use panproto_inst::ginstance::{HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use panproto_inst::wtype_check::is_reserved_edge_kind;
use panproto_inst::{FieldPresence, GInstance, NodeShape};
use panproto_schema::{Edge, Schema};

/// The name an edge has in the model: its JSON text.
fn edge_name(edge: &Edge) -> String {
    serde_json::to_string(edge).unwrap_or_else(|_| format!("{edge:?}"))
}

/// The name a node's value has in the model: its JSON text, or null.
fn value_of(value: Option<&FieldPresence>) -> ModelValue {
    match value {
        Some(FieldPresence::Present(v)) => {
            ModelValue::Str(serde_json::to_string(v).unwrap_or_else(|_| format!("{v:?}")))
        }
        Some(FieldPresence::Absent | FieldPresence::Null) | None => ModelValue::Null,
    }
}

/// An integer of the model, from an id.
fn int(id: u32) -> ModelValue {
    ModelValue::Int(i64::from(id))
}

/// The id an integer of the model names.
fn id_of(op: &str, args: &[ModelValue]) -> Result<u32, GatError> {
    match args {
        [ModelValue::Int(n)] => {
            u32::try_from(*n).map_err(|_| GatError::ModelError(format!("{op}: {n} is not an id")))
        }
        other => Err(GatError::ModelError(format!(
            "{op}: expected one id, got {other:?}"
        ))),
    }
}

/// The name a string of the model carries.
fn str_of<'a>(op: &str, args: &'a [ModelValue]) -> Result<&'a str, GatError> {
    match args {
        [ModelValue::Str(s)] => Ok(s),
        other => Err(GatError::ModelError(format!(
            "{op}: expected one name, got {other:?}"
        ))),
    }
}

/// Add the unary operation `name`, reading its argument with `key` and
/// looking it up in `table`.
fn add_table<K, F>(model: &mut Model, name: &'static str, table: BTreeMap<K, ModelValue>, key: F)
where
    K: Ord + Send + Sync + std::fmt::Debug + 'static,
    F: Fn(&'static str, &[ModelValue]) -> Result<K, GatError> + Send + Sync + 'static,
{
    model.add_op(name, move |args| {
        let k = key(name, args)?;
        table
            .get(&k)
            .cloned()
            .ok_or_else(|| GatError::ModelError(format!("{name}: {k:?} is not in its domain")))
    });
}

/// The placeholder links of a schema, each from a placeholder to what it
/// stands for: a union vertex to each of its variants (and a variant's
/// parent vertex to the variant), a recursion point to the vertex it
/// unfolds to, and a reference vertex to the target of each of its outgoing
/// `ref` edges.
#[derive(Default)]
struct Families {
    stands_for: BTreeMap<String, BTreeSet<String>>,
}

impl Families {
    fn of(schema: &Schema) -> Self {
        let mut stands_for: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut link = |placeholder: &str, member: &str| {
            stands_for
                .entry(placeholder.to_owned())
                .or_default()
                .insert(member.to_owned());
        };
        for (union_vertex, variants) in &schema.variants {
            for variant in variants {
                link(union_vertex, &variant.id);
                link(&variant.parent_vertex, &variant.id);
            }
        }
        for (point, unfolding) in &schema.recursion_points {
            link(point, &unfolding.target_vertex);
        }
        for (id, vertex) in &schema.vertices {
            if vertex.kind != "ref" {
                continue;
            }
            for edge in schema.outgoing.get(id).into_iter().flatten() {
                if edge.kind == "ref" {
                    link(id, &edge.tgt);
                }
            }
        }
        Self { stands_for }
    }

    /// Whether `to` is reachable from `from` along the links.
    fn reaches(&self, from: &str, to: &str) -> bool {
        let mut seen: BTreeSet<&str> = BTreeSet::from([from]);
        let mut frontier: Vec<&str> = vec![from];
        while let Some(current) = frontier.pop() {
            for member in self.stands_for.get(current).into_iter().flatten() {
                let member = member.as_str();
                if member == to {
                    return true;
                }
                if seen.insert(member) {
                    frontier.push(member);
                }
            }
        }
        false
    }

    /// Whether `anchor` is in the family of the endpoint `endpoint`: equal
    /// to it, or one reachable from the other.
    fn accepts(&self, endpoint: &str, anchor: &str) -> bool {
        endpoint == anchor || self.reaches(endpoint, anchor) || self.reaches(anchor, endpoint)
    }

    /// The vertex an endpoint expecting `endpoint` is read at when its node
    /// is anchored at `anchor`: the anchor when it is in the endpoint's
    /// family, and the endpoint otherwise.
    fn read(&self, endpoint: &Name, anchor: Option<&Name>) -> Name {
        match anchor {
            Some(anchor) if self.accepts(endpoint, anchor) => anchor.clone(),
            _ => endpoint.clone(),
        }
    }
}

/// The carriers and operation tables of a graph's model, as read off the
/// graph and its schema.
#[derive(Default)]
struct Tables {
    families: Families,
    nodes: Vec<u32>,
    edges: Vec<u32>,
    fans: Vec<u32>,
    ports: Vec<u32>,
    vertices: BTreeSet<String>,
    labels: BTreeMap<String, (String, String)>,
    values: BTreeSet<String>,
    hyper_edges: BTreeSet<String>,
    port_labels: BTreeSet<String>,
    anchor: BTreeMap<u32, ModelValue>,
    value: BTreeMap<u32, ModelValue>,
    i_src: BTreeMap<u32, ModelValue>,
    i_tgt: BTreeMap<u32, ModelValue>,
    ie_anchor: BTreeMap<u32, ModelValue>,
    fan_anchor: BTreeMap<u32, ModelValue>,
    fan_parent: BTreeMap<u32, ModelValue>,
    port_fan: BTreeMap<u32, ModelValue>,
    port_label: BTreeMap<u32, ModelValue>,
    port_target: BTreeMap<u32, ModelValue>,
}

impl Tables {
    /// The schema's vertices, edges, hyper-edges and signature labels.
    fn of_schema(schema: &Schema) -> Self {
        Self {
            families: Families::of(schema),
            vertices: schema.vertices.keys().map(ToString::to_string).collect(),
            labels: schema
                .edges
                .keys()
                .map(|e| (edge_name(e), (e.src.to_string(), e.tgt.to_string())))
                .collect(),
            hyper_edges: schema.hyper_edges.keys().map(ToString::to_string).collect(),
            port_labels: schema
                .hyper_edges
                .values()
                .flat_map(|h| h.signature.keys().map(ToString::to_string))
                .collect(),
            ..Self::default()
        }
    }

    /// Add the graph's nodes with their anchors and values.
    fn add_nodes(&mut self, graph: &GInstance) {
        self.nodes = graph.nodes().keys().copied().collect();
        self.nodes.sort_unstable();
        for id in &self.nodes {
            let Some(node) = graph.node(*id) else {
                continue;
            };
            self.vertices.insert(node.anchor.to_string());
            self.anchor
                .insert(*id, ModelValue::Str(node.anchor.to_string()));
            let v = value_of(node.value.as_ref());
            if let ModelValue::Str(s) = &v {
                self.values.insert(s.clone());
            }
            self.value.insert(*id, v);
            if node.shape == NodeShape::HyperEdge {
                self.fans.push(*id);
                self.hyper_edges.insert(node.anchor.to_string());
                self.fan_anchor
                    .insert(*id, ModelValue::Str(node.anchor.to_string()));
            }
        }
    }

    /// Add the graph's edges: non-fan edges as instance edges, the
    /// `hyper_label` edges of fan nodes as ports, and each fan node's single
    /// `hyper_parent` edge as its parent.
    fn add_edges(&mut self, graph: &GInstance) {
        let mut parents: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (id, e) in graph.edges_sorted() {
            let id = id.0;
            if !is_reserved_edge_kind(&e.edge.kind) {
                let anchor = |n: u32| graph.node(n).map(|node| &node.anchor);
                let label = Edge {
                    src: self.families.read(&e.edge.src, anchor(e.src)),
                    tgt: self.families.read(&e.edge.tgt, anchor(e.tgt)),
                    ..e.edge.clone()
                };
                self.vertices.insert(label.src.to_string());
                self.vertices.insert(label.tgt.to_string());
                let name = edge_name(&label);
                self.labels
                    .insert(name.clone(), (label.src.to_string(), label.tgt.to_string()));
                self.edges.push(id);
                self.i_src.insert(id, int(e.src));
                self.i_tgt.insert(id, int(e.tgt));
                self.ie_anchor.insert(id, ModelValue::Str(name));
            } else if self.fans.contains(&e.src) {
                if e.edge.kind == HYPER_LABEL_KIND {
                    let label = e
                        .edge
                        .name
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    self.port_labels.insert(label.clone());
                    self.ports.push(id);
                    self.port_fan.insert(id, int(e.src));
                    self.port_label.insert(id, ModelValue::Str(label));
                    self.port_target.insert(id, int(e.tgt));
                } else if e.edge.kind == HYPER_PARENT_KIND {
                    parents.entry(e.src).or_default().push(e.tgt);
                }
            }
        }
        for (fan, targets) in parents {
            if let [parent] = targets.as_slice() {
                self.fan_parent.insert(fan, int(*parent));
            }
        }
    }

    /// The model these tables make.
    fn into_model(self) -> Model {
        let mut model = Model::new("ThGraphInstance");
        let strs = |set: BTreeSet<String>| set.into_iter().map(ModelValue::Str).collect();
        let ints = |ids: &[u32]| ids.iter().map(|id| int(*id)).collect();
        model.add_sort("IVertex", ints(&self.nodes));
        model.add_sort("IEdge", ints(&self.edges));
        let mut values: Vec<ModelValue> = strs(self.values);
        values.push(ModelValue::Null);
        model.add_sort("Value", values);
        model.add_sort("Vertex", strs(self.vertices));
        model.add_sort(
            "Edge",
            self.labels.keys().cloned().map(ModelValue::Str).collect(),
        );
        model.add_sort("FanNode", ints(&self.fans));
        model.add_sort("FanPort", ints(&self.ports));
        model.add_sort("HyperEdge", strs(self.hyper_edges));
        model.add_sort("Label", strs(self.port_labels));
        let (src, tgt): (BTreeMap<String, ModelValue>, BTreeMap<String, ModelValue>) = self
            .labels
            .into_iter()
            .map(|(name, (s, t))| {
                (
                    (name.clone(), ModelValue::Str(s)),
                    (name, ModelValue::Str(t)),
                )
            })
            .unzip();
        let by_id = |op: &'static str, args: &[ModelValue]| id_of(op, args);
        let by_name = |op: &'static str, args: &[ModelValue]| str_of(op, args).map(str::to_owned);
        add_table(&mut model, "i_src", self.i_src, by_id);
        add_table(&mut model, "i_tgt", self.i_tgt, by_id);
        add_table(&mut model, "iv_anchor", self.anchor, by_id);
        add_table(&mut model, "ie_anchor", self.ie_anchor, by_id);
        add_table(&mut model, "iv_value", self.value, by_id);
        add_table(&mut model, "src", src, by_name);
        add_table(&mut model, "tgt", tgt, by_name);
        add_table(&mut model, "fan_anchor", self.fan_anchor, by_id);
        add_table(&mut model, "fan_parent", self.fan_parent, by_id);
        add_table(&mut model, "port_fan", self.port_fan, by_id);
        add_table(&mut model, "port_label", self.port_label, by_id);
        add_table(&mut model, "port_target", self.port_target, by_id);
        model
    }
}

/// The model of `th_graph_instance` that `graph`, over `schema`, is.
///
/// See the [module documentation](self) for the carriers and operations.
/// The model's equations hold exactly when every non-fan edge's endpoints
/// are anchored in the families of the endpoints of the schema edge it is
/// labelled with.
#[must_use]
pub fn graph_instance_model(graph: &GInstance, schema: &Schema) -> Model {
    let mut tables = Tables::of_schema(schema);
    tables.add_nodes(graph);
    tables.add_edges(graph);
    tables.into_model()
}
