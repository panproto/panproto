//! The data-level composition law: lifting a composite migration must agree
//! with lifting its two steps in turn.
//!
//! A kind-changing step carries a value-level coercion, and the composite's
//! target schema need not know about a coercion the intermediate schema
//! registered. Deriving the composite's value action from its endpoints alone
//! therefore drops that step's coercion; the composite has to carry it.

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::{CoercionClass, Name};
use panproto_inst::{FieldPresence, Node, Value, WInstance};
use panproto_mig::{Migration, compile, compose, lift_wtype};
use panproto_schema::{CoercionSpec, Edge, Schema, Vertex};

fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
    let mut vert_map = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in vertices {
        vert_map.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for edge in edges {
        edge_map.insert(edge.clone(), edge.kind.clone());
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices: vert_map,
        edges: edge_map,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: Vec::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

fn int_to_str() -> CoercionSpec {
    CoercionSpec {
        forward: panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::IntToStr,
            vec![panproto_expr::Expr::Var(std::sync::Arc::from("__value__"))],
        ),
        inverse: None,
        class: CoercionClass::Retraction,
    }
}

fn prefixed() -> CoercionSpec {
    CoercionSpec {
        forward: panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Concat,
            vec![
                panproto_expr::Expr::Lit(panproto_expr::Literal::Str("v:".into())),
                panproto_expr::Expr::Var(std::sync::Arc::from("__value__")),
            ],
        ),
        inverse: None,
        class: CoercionClass::Retraction,
    }
}

/// `a` is an integer in G1, a string in G2, and a "text" in G3. G2 registers
/// the int-to-string coercion, G3 the string-to-text one, so neither endpoint
/// of the composite knows both, and the composite's own endpoints -- int and
/// text -- name a kind pair no schema has a coercion for.
fn stack() -> (Schema, Schema, Schema, Edge) {
    let edge = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("x".into()),
    };
    let g1 = schema(
        &[("a", "int"), ("b", "string")],
        std::slice::from_ref(&edge),
    );

    let mut g2 = schema(
        &[("a", "string"), ("b", "string")],
        std::slice::from_ref(&edge),
    );
    g2.coercions
        .insert((Name::from("int"), Name::from("string")), int_to_str());

    let mut g3 = schema(
        &[("a", "text"), ("b", "string")],
        std::slice::from_ref(&edge),
    );
    g3.coercions
        .insert((Name::from("string"), Name::from("text")), prefixed());

    (g1, g2, g3, edge)
}

fn one_node(value: Value) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(
        0,
        Node::new(0, "a").with_value(FieldPresence::Present(value)),
    );
    WInstance::new(nodes, Vec::new(), Vec::new(), 0, Name::from("a"))
}

fn value_of(inst: &WInstance) -> Option<Value> {
    match inst.nodes.get(&0)?.value.clone()? {
        FieldPresence::Present(v) => Some(v),
        _ => None,
    }
}

#[test]
fn lifting_a_composite_agrees_with_lifting_its_steps() {
    let (g1, g2, g3, edge) = stack();
    let vertices = [Name::from("a"), Name::from("b")];
    let m1 = Migration::identity(&vertices, std::slice::from_ref(&edge)).with_coercions(&g1, &g2);
    let m2 = Migration::identity(&vertices, std::slice::from_ref(&edge)).with_coercions(&g2, &g3);

    let start = one_node(Value::Int(7));

    let first_step = compile(&g1, &g2, &m1).expect("the first step compiles");
    let step1 = lift_wtype(&first_step, &g1, &g2, &start).expect("the first step lifts");
    let second_step = compile(&g2, &g3, &m2).expect("the second step compiles");
    let stepwise = lift_wtype(&second_step, &g2, &g3, &step1).expect("the second step lifts");

    let m12 = compose(&m1, &m2).expect("the two steps compose");
    let whole = compile(&g1, &g3, &m12).expect("the composite compiles");
    let composite = lift_wtype(&whole, &g1, &g3, &start).expect("the composite lifts");

    assert_eq!(
        value_of(&stepwise),
        Some(Value::Str("v:7".into())),
        "the two steps coerce the integer to a string and then prefix it",
    );
    assert_eq!(
        value_of(&composite),
        value_of(&stepwise),
        "lift(g . f) must agree with lift(g) . lift(f)",
    );
}

/// A step whose coercion the composite's endpoints cannot supply is still
/// carried, so the composite's compiled value action is not empty.
#[test]
fn a_composite_carries_the_coercion_of_each_step() {
    let (g1, g2, g3, edge) = stack();
    let vertices = [Name::from("a"), Name::from("b")];
    let m1 = Migration::identity(&vertices, std::slice::from_ref(&edge)).with_coercions(&g1, &g2);
    let m2 = Migration::identity(&vertices, std::slice::from_ref(&edge)).with_coercions(&g2, &g3);

    let m12 = compose(&m1, &m2).expect("the two steps compose");
    assert!(
        m12.coercions.contains_key(&Name::from("a")),
        "the composite carries a value action at `a`",
    );

    let compiled = compile(&g1, &g3, &m12).expect("the composite compiles");
    assert!(
        compiled.op_term_assignments.contains_key(&Name::from("a")),
        "the compiled composite applies that value action",
    );
}
