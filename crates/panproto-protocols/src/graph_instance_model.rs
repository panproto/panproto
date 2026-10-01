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
//! | `Edge` | every schema edge and every `IEdge`'s label, each as its JSON text |
//! | `FanNode` | every node of the fan shape, as an integer |
//! | `FanPort` | every `hyper_label` edge leaving a fan node, as an integer |
//! | `HyperEdge` | every schema hyper-edge and every fan node's anchor, by name |
//! | `Label` | every signature label of a schema hyper-edge and every port's label, by name |
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

use panproto_gat::{GatError, Model, ModelValue};
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

/// The carriers and operation tables of a graph's model, as read off the
/// graph and its schema.
#[derive(Default)]
struct Tables {
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
                self.vertices.insert(e.edge.src.to_string());
                self.vertices.insert(e.edge.tgt.to_string());
                let name = edge_name(&e.edge);
                self.labels.insert(
                    name.clone(),
                    (e.edge.src.to_string(), e.edge.tgt.to_string()),
                );
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
/// are anchored at the endpoints of the schema edge it is labelled with.
#[must_use]
pub fn graph_instance_model(graph: &GInstance, schema: &Schema) -> Model {
    let mut tables = Tables::of_schema(schema);
    tables.add_nodes(graph);
    tables.add_edges(graph);
    tables.into_model()
}
