#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A query answers alike in every shape: path steps map sets to sets, the
//! predicate runs in ascending element id up to the limit, the answer is in
//! ascending element id, and a predicate that reads what no expression can
//! read is refused rather than evaluated.

use std::collections::HashMap;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    FInstance, FieldPresence, GInstance, InstanceQuery, Node, QueryError, RowId, Value, WInstance,
    execute_functor, execute_graph, execute_query,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn schema() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("r", "record", None)
        .unwrap()
        .edge("r", "r", "c", Some("c"))
        .unwrap()
        .build()
        .unwrap()
}

fn c() -> Edge {
    Edge {
        src: "r".into(),
        tgt: "r".into(),
        kind: "c".into(),
        name: Some("c".into()),
    }
}

fn ids(matches: &[panproto_inst::QueryMatch]) -> Vec<u32> {
    matches.iter().map(|m| m.node_id).collect()
}

#[test]
fn the_tree_and_its_graph_answer_a_two_step_query_alike() {
    // Arcs [(0, 1), (0, 2), (2, 4), (1, 3)]: the two-step query reaches 3 and
    // 4, listed in ascending id by both.
    let nodes: HashMap<u32, Node> = (0..5).map(|i| (i, Node::new(i, "r"))).collect();
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, c()), (0, 2, c()), (2, 4, c()), (1, 3, c())],
        vec![],
        0,
        "r".into(),
    );
    let query = InstanceQuery {
        anchor: Name::from("r"),
        path: vec!["c".into(), "c".into()],
        ..Default::default()
    };
    let tree_answer = ids(&execute_query(&query, &tree, &schema()).unwrap());
    let graph_answer =
        ids(&execute_graph(&query, &GInstance::from_wtype(&tree).unwrap(), &schema()).unwrap());
    assert_eq!(tree_answer, vec![3, 4]);
    assert_eq!(graph_answer, vec![3, 4]);
}

#[test]
fn limit_takes_the_least_node_ids() {
    let g = (0..10).fold(GInstance::new(), |g, i| g.with_node(Node::new(9 - i, "r")));
    let query = InstanceQuery {
        anchor: Name::from("r"),
        limit: Some(3),
        ..Default::default()
    };
    assert_eq!(
        ids(&execute_graph(&query, &g, &schema()).unwrap()),
        vec![0, 1, 2]
    );
}

#[test]
fn a_query_predicate_on_an_unexpressible_value_is_refused() {
    let g = GInstance::new()
        .with_node(Node::new(0, "r").with_extra_field("n", Value::Int(1)))
        .with_node(Node::new(1, "r").with_extra_field("t", Value::Token("red".into())));
    let reads_t = InstanceQuery {
        anchor: Name::from("r"),
        predicate: Some(Expr::Builtin(
            BuiltinOp::Eq,
            vec![Expr::Var("t".into()), Expr::Lit(Literal::Null)],
        )),
        ..Default::default()
    };
    assert_eq!(
        execute_graph(&reads_t, &g, &schema()).unwrap_err(),
        QueryError::ValueNotExpressible {
            node: 1,
            field: "t".into(),
            value_type: "token".into()
        }
    );
    // A predicate that does not read the token runs: only what it names is
    // converted.
    let reads_n = InstanceQuery {
        anchor: Name::from("r"),
        predicate: Some(Expr::Builtin(
            BuiltinOp::Eq,
            vec![Expr::Var("n".into()), Expr::Lit(Literal::Int(1))],
        )),
        ..Default::default()
    };
    assert_eq!(
        ids(&execute_graph(&reads_n, &g, &schema()).unwrap()),
        vec![0]
    );
}

#[test]
fn a_predicate_on_a_repeated_name_is_refused_in_every_shape() {
    let x = Edge {
        src: "r".into(),
        tgt: "r".into(),
        kind: "x".into(),
        name: Some("x".into()),
    };
    let predicate = Expr::Builtin(
        BuiltinOp::Eq,
        vec![Expr::Var("x".into()), Expr::Lit(Literal::Int(2))],
    );
    let query = InstanceQuery {
        anchor: Name::from("r"),
        predicate: Some(predicate),
        ..Default::default()
    };
    let refused = QueryError::AmbiguousChildField {
        node: 0,
        field: "x".into(),
    };
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "r")),
        (
            1,
            Node::new(1, "r").with_value(FieldPresence::Present(Value::Int(1))),
        ),
        (
            2,
            Node::new(2, "r").with_value(FieldPresence::Present(Value::Int(2))),
        ),
    ]
    .into();
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, x.clone()), (0, 2, x.clone())],
        vec![],
        0,
        "r".into(),
    );
    assert_eq!(
        execute_query(&query, &tree, &schema()).unwrap_err(),
        refused
    );
    assert_eq!(
        execute_graph(&query, &GInstance::from_wtype(&tree).unwrap(), &schema()).unwrap_err(),
        refused
    );
    let table = FInstance::new()
        .with_rows(
            "r",
            [
                (RowId(0), HashMap::new()),
                (RowId(1), HashMap::new()),
                (RowId(2), HashMap::new()),
            ],
        )
        .and_then(|f| f.with_foreign_key(x, [(RowId(0), RowId(1)), (RowId(0), RowId(2))]))
        .unwrap();
    assert_eq!(
        execute_functor(&query, &table, &schema()).unwrap_err(),
        refused
    );
}

/// A random tree over anchor `r` with arcs of kind `c` (parents precede
/// their children), as parent indices.
fn arb_tree() -> impl Strategy<Value = WInstance> {
    prop::collection::vec(any::<prop::sample::Index>(), 0..12).prop_map(|parents| {
        let mut nodes: HashMap<u32, Node> = HashMap::from([(0, Node::new(0, "r"))]);
        let mut arcs = Vec::new();
        for (i, parent) in parents.iter().enumerate() {
            let id = u32::try_from(i + 1).unwrap();
            let parent = u32::try_from(parent.index(i + 1)).unwrap();
            nodes.insert(id, Node::new(id, "r"));
            arcs.push((parent, id, c()));
        }
        // Arc order is shuffled against id order by reversing it.
        arcs.reverse();
        WInstance::new(nodes, arcs, vec![], 0, "r".into())
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A tree, its graph form, and the table of its nodes and arcs answer
    /// every path query alike, in ascending id.
    #[test]
    fn query_answers_agree_across_shapes(
        tree in arb_tree(),
        steps in 0usize..4,
        limit in prop::option::of(0usize..5),
    ) {
        let query = InstanceQuery {
            anchor: Name::from("r"),
            path: vec!["c".into(); steps],
            limit,
            ..Default::default()
        };
        let tree_answer = ids(&execute_query(&query, &tree, &schema()).unwrap());
        let graph = GInstance::from_wtype(&tree).unwrap();
        let graph_answer = ids(&execute_graph(&query, &graph, &schema()).unwrap());
        let mut table = FInstance::new()
            .with_rows("r", tree.nodes.keys().map(|id| (RowId(*id), HashMap::new())))
            .unwrap();
        table = table
            .with_foreign_key(c(), tree.arcs.iter().map(|(p, ch, _)| (RowId(*p), RowId(*ch))))
            .unwrap();
        let table_answer = ids(&execute_functor(&query, &table, &schema()).unwrap());
        prop_assert!(tree_answer.windows(2).all(|w| w[0] < w[1]));
        prop_assert_eq!(&tree_answer, &graph_answer);
        prop_assert_eq!(&tree_answer, &table_answer);
    }
}
