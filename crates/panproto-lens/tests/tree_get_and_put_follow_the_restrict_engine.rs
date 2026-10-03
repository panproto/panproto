#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tree `get` records what the tree restrict synthesizes, and tree `put`
//! evaluates a restored node's survival predicate as the next `get` will.
//!
//! `get` runs the restrict engine and records, for every node it
//! synthesizes, the key that names it: an expansion node is keyed by the
//! source arc it expands, named through the tree's embedding into its graph
//! form, and the hop. `put` writes back a source on which every restored
//! node's survival predicate gives `true`; a predicate that gives `false`
//! is `SurvivalPredicateFails` and one that gives neither is
//! `SurvivalPredicateRefused`, reported by `check_put_domain` and returned
//! by `put` as its first violation, so the next `get` reads what `put`
//! wrote.

use std::collections::HashMap;

use panproto_expr::{Expr, Literal};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{
    CompiledMigration, GInstance, Node, PredicateRefusal, PutDomainError, SynthKey, ViewRef,
    WInstance,
};
use panproto_lens::{Lens, LensError};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[&str], edges: &[&Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for v in vertices {
        builder = builder.vertex(v, "record", None).unwrap();
    }
    for e in edges {
        builder = builder
            .edge(&e.src, &e.tgt, &e.kind, e.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

/// `r(0)` with the children `a(1)` and `b(2)`, the `b` arc first.
fn two_children() -> WInstance {
    WInstance::new(
        HashMap::from([
            (0, Node::new(0, "r")),
            (1, Node::new(1, "a")),
            (2, Node::new(2, "b")),
        ]),
        vec![(0, 2, edge("r", "b", "b")), (0, 1, edge("r", "a", "a"))],
        vec![],
        0,
        "r".into(),
    )
}

/// The lens that keeps `r`, `a` and `b` and expands the arc `r -b-> b` into
/// `r -w-> w -b-> b` through a synthesized `w`.
fn expanding_lens() -> Lens {
    let src = schema(
        &["r", "a", "b"],
        &[&edge("r", "a", "a"), &edge("r", "b", "b")],
    );
    let tgt = schema(
        &["r", "a", "b", "w"],
        &[
            &edge("r", "a", "a"),
            &edge("r", "w", "w"),
            &edge("w", "b", "b"),
        ],
    );
    let mut compiled = CompiledMigration {
        surviving_verts: ["r", "a", "b", "w"].iter().map(|v| (*v).into()).collect(),
        surviving_edges: tgt.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled
        .expansion_path
        .insert(("r".into(), "b".into()), vec!["w".into()]);
    Lens {
        compiled,
        src_schema: src,
        tgt_schema: tgt,
    }
}

#[test]
fn tree_expand_keys_name_the_source_arc_through_the_embedding() {
    let lens = expanding_lens();
    let source = two_children();
    let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
    let (_, embedding) = GInstance::from_wtype_embedded(&source, None).unwrap();
    // The one synthesized node is `w`, keyed by the arc into `b` (node 2)
    // under its edge id in the graph form, at hop 0.
    let synthesized: Vec<u32> = view
        .nodes
        .keys()
        .copied()
        .filter(|id| !source.nodes.contains_key(id))
        .collect();
    assert_eq!(synthesized.len(), 1);
    let w = synthesized[0];
    assert_eq!(&*view.nodes[&w].anchor, "w");
    assert_eq!(
        complement.synthesized_keys.get(&w),
        Some(&SynthKey::Expand {
            source_edge: embedding.arcs[&2],
            hop: 0,
        })
    );
    assert_eq!(complement.synthesized_keys.len(), 1);
    // The keys are what the restrict engine allocated them for.
    let plan =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .unwrap();
    let (_, synthesis) = panproto_inst::wtype_restrict_with_plan(&source, &plan).unwrap();
    assert_eq!(complement.synthesized_keys, synthesis.synthesized_keys);
    // And `put` still writes the source back.
    let restored = panproto_lens::put(&lens, &view, &complement).unwrap();
    assert!(panproto_lens::instances_equivalent(&restored, &source));
}

#[test]
fn tree_put_refuses_a_removed_expansion_arc() {
    let lens = expanding_lens();
    let (mut view, complement) = panproto_lens::get(&lens, &two_children()).unwrap();
    let synthesized = *complement.synthesized_nodes.iter().next().unwrap();
    let removed = view
        .arcs
        .iter()
        .find(|(parent, _, _)| *parent == synthesized)
        .map(|(parent, child, _)| (*parent, *child))
        .unwrap();
    view.arcs
        .retain(|(parent, child, _)| (*parent, *child) != removed);

    let refusal = PutDomainError::SynthesizedEdited {
        elem: ViewRef::Arc {
            parent: removed.0,
            child: removed.1,
        },
    };
    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Err(vec![refusal.clone()])
    );
    assert!(matches!(
        panproto_lens::put(&lens, &view, &complement),
        Err(LensError::PutDomain(error)) if *error == refusal
    ));
}

#[test]
fn tree_put_refuses_a_relabelled_expansion_arc() {
    let lens = expanding_lens();
    let (mut view, complement) = panproto_lens::get(&lens, &two_children()).unwrap();
    let synthesized = *complement.synthesized_nodes.iter().next().unwrap();
    let changed = view
        .arcs
        .iter_mut()
        .find(|(parent, _, _)| *parent == synthesized)
        .map(|(parent, child, edge)| {
            edge.name = Some("changed".into());
            (*parent, *child)
        })
        .unwrap();

    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Err(vec![PutDomainError::SynthesizedEdited {
            elem: ViewRef::Arc {
                parent: changed.0,
                child: changed.1,
            },
        }])
    );
}

#[test]
fn tree_put_refuses_an_edited_expansion_node() {
    let lens = expanding_lens();
    let (mut view, complement) = panproto_lens::get(&lens, &two_children()).unwrap();
    let synthesized = *complement.synthesized_nodes.iter().next().unwrap();
    view.nodes.get_mut(&synthesized).unwrap().value = Some(FieldPresence::Present(Value::Int(1)));

    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Err(vec![PutDomainError::SynthesizedEdited {
            elem: ViewRef::Node(synthesized),
        }])
    );
}

#[test]
fn tree_complement_field_order() {
    let lens = expanding_lens();
    let (_, complement) = panproto_lens::get(&lens, &two_children()).unwrap();
    // The serialized field order is the declaration order: the recorded
    // keys come after the floor and before the identity.
    let text = serde_json::to_string(&complement).unwrap();
    let at = |name: &str| {
        text.find(&format!("\"{name}\":"))
            .unwrap_or_else(|| panic!("{name} missing from {text}"))
    };
    assert!(at("id_floor") < at("synthesized_keys"), "{text}");
    assert!(at("synthesized_keys") < at("identity"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        json["synthesized_keys"]
            .as_object()
            .map(serde_json::Map::len),
        Some(1),
        "the recorded key is serialized: {json}"
    );
    let decoded: panproto_inst::Complement = serde_json::from_str(&text).unwrap();
    assert_eq!(decoded.synthesized_keys, complement.synthesized_keys);
}

/// The identity lens on `r -leaf-> leaf` whose `leaf` survives under the
/// predicate `predicate`.
fn predicated_identity(predicate: Expr) -> Lens {
    let s = schema(&["r", "leaf"], &[&edge("r", "leaf", "leaf")]);
    let mut compiled = CompiledMigration {
        surviving_verts: s.vertices.keys().cloned().collect(),
        surviving_edges: s.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.add_conditional_survival("leaf", predicate);
    Lens {
        compiled,
        src_schema: s.clone(),
        tgt_schema: s,
    }
}

/// `r(0) -leaf-> leaf(1)` with `leaf.x = x`.
fn leaf_with(x: Value) -> WInstance {
    let mut leaf = Node::new(1, "leaf");
    leaf.extra_fields.insert("x".to_owned(), x);
    WInstance::new(
        HashMap::from([(0, Node::new(0, "r")), (1, leaf)]),
        vec![(0, 1, edge("r", "leaf", "leaf"))],
        vec![],
        0,
        "r".into(),
    )
}

#[test]
fn tree_put_refuses_a_restored_node_whose_survival_predicate_fails() {
    // The predicate reads the field `x`, true in the source.
    let lens = predicated_identity(Expr::Var("x".into()));
    let (mut view, complement) = panproto_lens::get(&lens, &leaf_with(Value::Bool(true))).unwrap();
    assert!(view.nodes.contains_key(&1));

    // The view makes `x` an integer: the predicate gives no boolean there.
    if let Some(leaf) = view.nodes.get_mut(&1) {
        leaf.extra_fields.insert("x".to_owned(), Value::Int(7));
    }
    let refused = PutDomainError::SurvivalPredicateRefused {
        elem: ViewRef::Node(1),
        refusal: PredicateRefusal::NotBoolean { got: "int".into() },
    };
    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Err(vec![refused.clone()])
    );
    match panproto_lens::put(&lens, &view, &complement) {
        Err(LensError::PutDomain(first)) => assert_eq!(*first, refused),
        other => panic!("put must refuse with the put-domain error, got {other:?}"),
    }

    // `false` is the other violation: the next get would drop the node.
    if let Some(leaf) = view.nodes.get_mut(&1) {
        leaf.extra_fields.insert("x".to_owned(), Value::Bool(false));
    }
    let fails = PutDomainError::SurvivalPredicateFails {
        elem: ViewRef::Node(1),
    };
    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Err(vec![fails.clone()])
    );
    match panproto_lens::put(&lens, &view, &complement) {
        Err(LensError::PutDomain(first)) => assert_eq!(*first, fails),
        other => panic!("put must refuse with the put-domain error, got {other:?}"),
    }

    // `true` is accepted, and the next get reads what put wrote.
    if let Some(leaf) = view.nodes.get_mut(&1) {
        leaf.extra_fields.insert("x".to_owned(), Value::Bool(true));
    }
    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Ok(())
    );
    let source = panproto_lens::put(&lens, &view, &complement).unwrap();
    let (again, _) = panproto_lens::get(&lens, &source).unwrap();
    assert!(again.nodes.contains_key(&1));
}

#[test]
fn a_restored_predicate_reads_the_restored_children() {
    // The predicate on `r` reads its child `leaf` by column name: the
    // child's value, not a field of `r`.
    let s = schema(&["r", "leaf"], &[&edge("r", "leaf", "leaf")]);
    let mut compiled = CompiledMigration {
        surviving_verts: s.vertices.keys().cloned().collect(),
        surviving_edges: s.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.add_conditional_survival(
        "r",
        Expr::Builtin(
            panproto_expr::BuiltinOp::Eq,
            vec![
                Expr::Var("leaf".into()),
                Expr::Lit(Literal::Str("v".into())),
            ],
        ),
    );
    let lens = Lens {
        compiled,
        src_schema: s.clone(),
        tgt_schema: s,
    };
    let source = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "r")),
            (
                1,
                Node::new(1, "leaf").with_value(FieldPresence::Present(Value::Str("v".into()))),
            ),
        ]),
        vec![(0, 1, edge("r", "leaf", "leaf"))],
        vec![],
        0,
        "r".into(),
    );
    let (mut view, complement) = panproto_lens::get(&lens, &source).unwrap();
    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Ok(())
    );
    // Editing the child's value makes the root's predicate false on the
    // restored source.
    if let Some(leaf) = view.nodes.get_mut(&1) {
        leaf.value = Some(FieldPresence::Present(Value::Str("w".into())));
    }
    assert_eq!(
        panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
        Err(vec![PutDomainError::SurvivalPredicateFails {
            elem: ViewRef::Node(0),
        }])
    );
}
