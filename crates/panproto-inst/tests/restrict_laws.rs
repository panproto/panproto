#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Laws of the restrict engines, checked over generated inputs: the
//! protocol layer is a per-node post-pass, tree and graph contraction
//! choose the same pairs on tree-shaped input, and the complements record
//! exactly the source data the view does not determine.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{Expr, Literal};
use panproto_gat::Name;
use panproto_inst::value::ValueSite;
use panproto_inst::value::coerce::coerce_value;
use panproto_inst::{
    BoundaryProtocols, CellFate, CompiledMigration, EdgeId, GInstance, Node, RestrictError,
    RestrictPlan, Value, WInstance, graph_restrict_with_plan, wtype_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};
use proptest::prelude::*;

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

/// A table `t` with the given int columns.
fn table_schema(columns: &[&str]) -> Schema {
    let p = protocol();
    let mut builder = SchemaBuilder::new(&p).vertex("t", "record", None).unwrap();
    for column in columns {
        let vertex = format!("t.{column}");
        builder = builder
            .vertex(&vertex, "int", None)
            .unwrap()
            .edge("t", &vertex, "prop", Some(column))
            .unwrap();
    }
    builder.build().unwrap()
}

/// Whether two rows hold the same values under the same keys, by
/// canonical bytes.
fn same_row(a: &HashMap<String, Value>, b: &HashMap<String, Value>) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| v.canonical_cmp(w).is_eq()))
}

/// What the protocol layer does to one node of the plan-free view: the cell
/// fates of its table, then the coercion of the keys the transform wrote.
fn post_pass(
    node: &mut Node,
    fates: &[CellFate],
    written: &str,
    plan: &RestrictPlan,
) -> Result<(), String> {
    let mut written: Option<String> =
        Some(written.to_owned()).filter(|k| node.extra_fields.contains_key(k));
    let mut moved = Vec::new();
    for fate in fates {
        match fate {
            CellFate::Drop(column) => {
                node.extra_fields.remove(column);
                if written.as_deref() == Some(column) {
                    written = None;
                }
            }
            CellFate::Rename(from, to) => {
                if let Some(value) = node.extra_fields.remove(from) {
                    let was = written.as_deref() == Some(from);
                    moved.push((to.clone(), value, was));
                }
            }
        }
    }
    for (to, value, was) in moved {
        node.extra_fields.insert(to.clone(), value);
        if was {
            written = Some(to);
        } else if written.as_deref() == Some(&to) {
            written = None;
        }
    }
    let Some(key) = written else {
        return Ok(());
    };
    let cells = plan.cells().unwrap();
    if let Some(spec) = cells.specs.get(&(node.anchor.clone(), key.clone())) {
        let value = node.extra_fields[&key].clone();
        let coerced = coerce_value(&value, spec, ValueSite::new(format!("t.{key}"), None))
            .map_err(|e| e.to_string())?;
        node.extra_fields.insert(key, coerced);
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// If the engine returns a view under the plan with the protocol layer,
    /// it returns one under the plan without it, and the first is the
    /// second with each surviving node's cell fates and coercions applied.
    #[test]
    fn the_protocol_layer_is_a_per_node_post_pass(
        cells in prop::collection::vec((prop::option::of(-3i64..3), prop::option::of(-3i64..3), prop::option::of(-3i64..3)), 0..5),
        drop_b in any::<bool>(),
        rename_c in any::<bool>(),
        output in prop::sample::select(vec![
            Literal::Int(4), Literal::Float(2.0), Literal::Float(2.5), Literal::Str("x".into()),
        ]),
        written in prop::sample::select(vec!["a", "b", "c", "z"]),
    ) {
        let src = table_schema(&["a", "b", "c"]);
        let mut target_columns = vec!["a"];
        if !drop_b {
            target_columns.push("b");
        }
        target_columns.push(if rename_c { "d" } else { "c" });
        let tgt = table_schema(&target_columns);
        let mut m = CompiledMigration::default();
        for vertex in tgt.vertices.keys() {
            m.surviving_verts.insert(vertex.clone());
        }
        if rename_c {
            m.vertex_remap.insert("t.c".into(), "t.d".into());
            m.edge_remap.insert(e("t", "t.c", "c"), e("t", "t.d", "d"));
        }
        m.add_computed_field("t", written, Expr::Lit(output));
        let mut g = GInstance::new();
        for (i, (a, b, c)) in cells.iter().enumerate() {
            let mut node = Node::new(u32::try_from(i).unwrap(), "t");
            for (key, value) in [("a", a), ("b", b), ("c", c)] {
                if let Some(v) = value {
                    node.extra_fields.insert(key.into(), Value::Int(*v));
                }
            }
            g = g.with_node(node);
        }
        let p = protocol();
        let protocols = BoundaryProtocols { src: &p, tgt: &p };
        let bare = RestrictPlan::compile(&src, &tgt, &m).unwrap();
        let layered = RestrictPlan::compile_with_protocols(&src, &tgt, &m, protocols).unwrap();
        prop_assert!(layered.cells().is_some());
        let with_layer = graph_restrict_with_plan(&g, &layered);
        let without = graph_restrict_with_plan(&g, &bare);
        match with_layer {
            Ok((layered_view, _)) => {
                let (bare_view, _) = without.unwrap();
                let fates = layered.cells().unwrap().fates.get("t").cloned().unwrap_or_default();
                for (id, node) in bare_view.nodes() {
                    let mut expected = node.clone();
                    post_pass(&mut expected, &fates, written, &layered).unwrap();
                    let actual = layered_view.node(*id).unwrap();
                    prop_assert!(
                        same_row(&expected.extra_fields, &actual.extra_fields),
                        "node {}: {:?} against {:?}", id, expected.extra_fields, actual.extra_fields
                    );
                }
                prop_assert_eq!(bare_view.node_count(), layered_view.node_count());
            }
            Err(error) => {
                // The converse fails only through a coercion refusal.
                prop_assert!(
                    matches!(error.unlocated(), RestrictError::ValueFidelity { .. }),
                    "{:?}", error
                );
                prop_assert!(without.is_ok());
            }
        }
    }
}

/// The schema of the contraction tests: `r` with children `m` (dropped,
/// recursive) and `leaf`; `m` holds `m` and `leaf`.
fn contraction_schemas() -> (Schema, Schema, CompiledMigration) {
    let protocol = Protocol::default();
    let src = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|b| b.vertex("m", "record", None))
        .and_then(|b| b.vertex("leaf", "record", None))
        .and_then(|b| b.edge("r", "m", "prop", Some("m")))
        .and_then(|b| b.edge("m", "m", "prop", Some("mm")))
        .and_then(|b| b.edge("m", "leaf", "prop", Some("leaf")))
        .and_then(|b| b.edge("r", "leaf", "prop", Some("leaf")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let tgt = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .and_then(|b| b.vertex("leaf", "record", None))
        .and_then(|b| b.edge("r", "leaf", "prop", Some("leaf")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let m = CompiledMigration {
        surviving_verts: ["r", "leaf"].iter().map(|v| Name::from(*v)).collect(),
        ..CompiledMigration::default()
    };
    (src, tgt, m)
}

/// A random tree under that schema: each new node is an `m` or a `leaf`
/// under an earlier `r` or `m`.
fn arb_contraction_tree() -> impl Strategy<Value = WInstance> {
    prop::collection::vec((any::<prop::sample::Index>(), any::<bool>()), 0..10).prop_map(|spec| {
        let mut nodes: HashMap<u32, Node> = HashMap::from([(0, Node::new(0, "r"))]);
        let mut parents: Vec<(u32, &str)> = vec![(0, "r")];
        let mut arcs = Vec::new();
        for (i, (parent, is_leaf)) in spec.iter().enumerate() {
            let id = u32::try_from(i + 1).unwrap();
            let (parent_id, parent_anchor) = parents[parent.index(parents.len())];
            let anchor = if *is_leaf { "leaf" } else { "m" };
            let label = match (parent_anchor, anchor) {
                ("r", "m") => e("r", "m", "m"),
                ("m", "m") => e("m", "m", "mm"),
                ("m", "leaf") => e("m", "leaf", "leaf"),
                _ => e("r", "leaf", "leaf"),
            };
            nodes.insert(id, Node::new(id, anchor));
            arcs.push((parent_id, id, label));
            if !*is_leaf {
                parents.push((id, "m"));
            }
        }
        WInstance::new(nodes, arcs, vec![], 0, "r".into())
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// On tree-shaped input, tree ancestor contraction and the graph
    /// contraction stage join the same pairs with the same labels, and each
    /// parent's children come in the same order.
    #[test]
    fn tree_and_graph_contraction_agree_on_tree_shaped_input(tree in arb_contraction_tree()) {
        let (src, tgt, m) = contraction_schemas();
        let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
        let (tree_view, _) = wtype_restrict_with_plan(&tree, &plan).unwrap();
        let graph = GInstance::from_wtype(&tree).unwrap();
        let (graph_view, _) = graph_restrict_with_plan(&graph, &plan).unwrap();
        let tree_pairs: BTreeSet<(u32, u32, Edge)> = tree_view.arcs.iter().cloned().collect();
        let graph_pairs: BTreeSet<(u32, u32, Edge)> = graph_view
            .edges()
            .values()
            .map(|e| (e.src, e.tgt, e.edge.clone()))
            .collect();
        prop_assert_eq!(&tree_pairs, &graph_pairs);
        for id in tree_view.nodes.keys() {
            let tree_children: Vec<u32> = tree_view.arcs.iter().filter(|(p, _, _)| p == id).map(|(_, c, _)| *c).collect();
            let graph_children: Vec<u32> = graph_view
                .ordered_out_edges(*id)
                .into_iter()
                .map(|edge| graph_view.edge(edge).unwrap().tgt)
                .collect();
            prop_assert_eq!(tree_children, graph_children);
        }
    }

    /// A kept edge is recorded in `original_edges` exactly when its view
    /// label does not map back to its source label by the unique preimage.
    #[test]
    fn original_edges_records_exactly_the_edges_that_would_not_map_back(
        merge in any::<bool>(),
        labels in prop::collection::vec(0usize..3, 0..6),
    ) {
        let protocol = Protocol::default();
        let names = ["x", "y", "k"];
        let src = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None)
            .and_then(|b| b.vertex("a", "record", None))
            .and_then(|b| b.edge("r", "a", "prop", Some("x")))
            .and_then(|b| b.edge("r", "a", "prop", Some("y")))
            .and_then(|b| b.edge("r", "a", "prop", Some("k")))
            .and_then(SchemaBuilder::build)
            .unwrap();
        let tgt = SchemaBuilder::new(&protocol)
            .vertex("r", "record", None)
            .and_then(|b| b.vertex("a", "record", None))
            .and_then(|b| b.edge("r", "a", "prop", Some("z")))
            .and_then(|b| b.edge("r", "a", "prop", Some("y")))
            .and_then(|b| b.edge("r", "a", "prop", Some("k")))
            .and_then(SchemaBuilder::build)
            .unwrap();
        let mut m = CompiledMigration {
            surviving_verts: ["r", "a"].iter().map(|v| Name::from(*v)).collect(),
            ..CompiledMigration::default()
        };
        m.edge_remap.insert(e("r", "a", "x"), e("r", "a", "z"));
        if merge {
            m.edge_remap.insert(e("r", "a", "y"), e("r", "a", "z"));
        }
        let mut g = GInstance::new().with_node(Node::new(0, "r")).with_node(Node::new(1, "a"));
        for label in &labels {
            g = g.with_edge(0, 1, e("r", "a", names[*label]));
        }
        let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
        let (view, complement) = graph_restrict_with_plan(&g, &plan).unwrap();
        for (id, edge) in view.edges() {
            let source = g.edge(*id).unwrap();
            let maps_back = plan.edge_image.preimages(&edge.edge) == vec![&source.edge];
            prop_assert_eq!(complement.original_edges.contains_key(id), !maps_back);
            if let Some(recorded) = complement.original_edges.get(id) {
                prop_assert_eq!(recorded, source);
            }
        }
        prop_assert!(complement.original_edges.keys().all(|id: &EdgeId| view.edge(*id).is_some()));
    }
}
