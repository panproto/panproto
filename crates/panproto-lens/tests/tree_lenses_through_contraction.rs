//! Tree lenses whose migration drops an interior vertex, so that `get`
//! contracts the path through it into one view arc.
//!
//! The fixture throughout is the source schema `r -> m -> leaf` with `m`
//! dropped and the resolver sending the contracted pair `(r, leaf)` to the
//! target edge `r -> leaf`. Each test pins one behaviour of the tree lens on
//! it: composition, put domain, residue classification, fresh ids, edit
//! translation and the composition rule.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Node, WInstance};
use panproto_lens::Lens;
use panproto_mig::Migration;
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for (id, kind) in vertices {
        builder = builder.vertex(id, kind, None).unwrap();
    }
    for e in edges {
        builder = builder
            .edge(&e.src, &e.tgt, &e.kind, e.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

fn migration(
    vertices: &[(&str, &str)],
    edges: Vec<(Edge, Edge)>,
    resolver: Vec<((&str, &str), Edge)>,
) -> Migration {
    Migration {
        vertex_map: vertices
            .iter()
            .map(|(s, t)| (Name::from(*s), Name::from(*t)))
            .collect(),
        edge_map: edges.into_iter().collect(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: resolver
            .into_iter()
            .map(|((a, b), e)| ((Name::from(a), Name::from(b)), e))
            .collect(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        coercions: HashMap::new(),
        domain: None,
        codomain: None,
    }
}

fn lens(src: Schema, tgt: Schema, m: &Migration) -> Lens {
    let compiled = panproto_mig::compile(&src, &tgt, m).unwrap();
    Lens {
        compiled,
        src_schema: src,
        tgt_schema: tgt,
    }
}

/// `r -> m -> leaf`, `m` dropped, `(r, leaf)` resolved to `r -> leaf`
/// named `label`. The target also has the edge `r -> leaf` named `other`
/// when `parallel` is set, so the pair needs the resolver.
fn contracting_lens(parallel: bool) -> Lens {
    let src = schema(
        &[("r", "object"), ("m", "object"), ("leaf", "string")],
        &[prop("r", "m", "m"), prop("m", "leaf", "leaf")],
    );
    let mut tgt_edges = vec![prop("r", "leaf", "label")];
    if parallel {
        tgt_edges.push(prop("r", "leaf", "other"));
    }
    let tgt = schema(&[("r", "object"), ("leaf", "string")], &tgt_edges);
    let m = migration(
        &[("r", "r"), ("leaf", "leaf")],
        vec![],
        vec![(("r", "leaf"), prop("r", "leaf", "label"))],
    );
    lens(src, tgt, &m)
}

fn leaf(id: u32, s: &str) -> Node {
    Node::new(id, "leaf").with_value(FieldPresence::Present(Value::Str(s.into())))
}

/// `r(0) -> m(1) -> leaf(2)`.
fn contracting_source() -> WInstance {
    WInstance::new(
        HashMap::from([
            (0, Node::new(0, "r")),
            (1, Node::new(1, "m")),
            (2, leaf(2, "x")),
        ]),
        vec![
            (0, 1, prop("r", "m", "m")),
            (1, 2, prop("m", "leaf", "leaf")),
        ],
        vec![],
        0,
        "r".into(),
    )
}

/// The arcs of a tree, sorted, for comparing trees whose arc order is not
/// under test.
fn sorted_arcs(tree: &WInstance) -> Vec<(u32, u32, Edge)> {
    let mut arcs = tree.arcs.clone();
    arcs.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    arcs
}

mod composition {
    use super::*;

    /// A second lens renaming `leaf` to `item`, keeping both parallel
    /// edges under the new anchor.
    fn renaming_lens() -> Lens {
        let src = schema(
            &[("r", "object"), ("leaf", "string")],
            &[prop("r", "leaf", "label"), prop("r", "leaf", "other")],
        );
        let tgt = schema(
            &[("r", "object"), ("item", "string")],
            &[prop("r", "item", "label"), prop("r", "item", "other")],
        );
        let m = migration(
            &[("r", "r"), ("leaf", "item")],
            vec![
                (prop("r", "leaf", "label"), prop("r", "item", "label")),
                (prop("r", "leaf", "other"), prop("r", "item", "other")),
            ],
            vec![],
        );
        lens(src, tgt, &m)
    }

    /// The composite contracts the pair under the second lens's name for
    /// the edge the first lens's resolver chose, as running the two lenses
    /// one after the other does.
    #[test]
    fn composed_lens_translates_resolver_keys_into_the_final_target() {
        let first = contracting_lens(true);
        let second = renaming_lens();
        let source = contracting_source();

        let (mid, _) = panproto_lens::get(&first, &source).unwrap();
        let (sequential, _) = panproto_lens::get(&second, &mid).unwrap();
        assert_eq!(sequential.arcs, vec![(0, 2, prop("r", "item", "label"))]);

        let composite = panproto_lens::compose(&first, &second).unwrap();
        let (view, _) = panproto_lens::get(&composite, &source)
            .expect("the composite resolves the contracted pair in its own target space");
        assert_eq!(view.arcs, sequential.arcs);
    }

    /// With a second lens that renames nothing, the composite contracts
    /// exactly as the first lens does.
    #[test]
    fn composing_with_a_non_renaming_lens_keeps_every_contraction_key() {
        let first = contracting_lens(true);
        let identity_tgt = first.tgt_schema.clone();
        let second = lens(
            identity_tgt.clone(),
            identity_tgt,
            &migration(
                &[("r", "r"), ("leaf", "leaf")],
                vec![
                    (prop("r", "leaf", "label"), prop("r", "leaf", "label")),
                    (prop("r", "leaf", "other"), prop("r", "leaf", "other")),
                ],
                vec![],
            ),
        );
        let composite = panproto_lens::compose(&first, &second).unwrap();
        assert_eq!(composite.compiled.resolver, first.compiled.resolver);
        let (view, _) = panproto_lens::get(&composite, &contracting_source()).unwrap();
        assert_eq!(view.arcs, vec![(0, 2, prop("r", "leaf", "label"))]);
    }
}

mod put_domain {
    use super::*;
    use panproto_inst::ViewRef;
    use panproto_lens::{LensError, PutDomainError};

    /// Adding a leaf under `r` with the contracted label has no source path
    /// to go to: `put` refuses it rather than dropping it.
    #[test]
    fn a_view_leaf_added_across_a_contracted_pair_is_rejected() {
        let lens = contracting_lens(false);
        let (mut view, complement) = panproto_lens::get(&lens, &contracting_source()).unwrap();
        assert_eq!(view.arcs, vec![(0, 2, prop("r", "leaf", "label"))]);
        view.nodes.insert(3, leaf(3, "y"));
        view.arcs.push((0, 3, prop("r", "leaf", "label")));
        let view = WInstance::new(
            view.nodes,
            view.arcs,
            view.fans,
            view.root,
            view.schema_root,
        );

        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Err(vec![PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 3
                }
            }])
        );
        let put = panproto_lens::put(&lens, &view, &complement);
        assert!(
            matches!(put, Err(LensError::ComplementMismatch { .. })),
            "{put:?}"
        );
    }

    /// The unchanged view is in the domain.
    #[test]
    fn the_view_get_returns_is_in_the_put_domain() {
        let lens = contracting_lens(false);
        let (view, complement) = panproto_lens::get(&lens, &contracting_source()).unwrap();
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Ok(())
        );
        let back = panproto_lens::put(&lens, &view, &complement).unwrap();
        assert_eq!(sorted_arcs(&back), sorted_arcs(&contracting_source()));
    }
}

/// Put is strict: a view that fails a survival predicate, edits a computed
/// field, or edits a field whose transform has no inverse is refused, since
/// no source would give that view back.
mod strict_put {
    use std::sync::Arc;

    use super::*;
    use panproto_expr::{BuiltinOp, Expr, Literal};
    use panproto_inst::{FieldTransform, ViewRef};
    use panproto_lens::{LensError, PutDomainError};

    fn plain_lens() -> Lens {
        let src = schema(&[("r", "object"), ("c", "object")], &[prop("r", "c", "c")]);
        let m = migration(
            &[("r", "r"), ("c", "c")],
            vec![(prop("r", "c", "c"), prop("r", "c", "c"))],
            vec![],
        );
        lens(src.clone(), src, &m)
    }

    /// `r(0) {n: 1} -> c(1) {level: 2}`.
    fn source() -> WInstance {
        let mut r = Node::new(0, "r");
        r.extra_fields.insert("n".into(), Value::Int(1));
        let mut c = Node::new(1, "c");
        c.extra_fields.insert("level".into(), Value::Int(2));
        WInstance::new(
            HashMap::from([(0, r), (1, c)]),
            vec![(0, 1, prop("r", "c", "c"))],
            vec![],
            0,
            "r".into(),
        )
    }

    fn var(name: &str) -> Expr {
        Expr::Var(Arc::from(name))
    }

    fn edited(view: &WInstance, id: u32, key: &str, value: Value) -> WInstance {
        let mut view = view.clone();
        view.nodes
            .get_mut(&id)
            .unwrap()
            .extra_fields
            .insert(key.into(), value);
        view
    }

    fn assert_refused(
        lens: &Lens,
        view: &WInstance,
        complement: &panproto_lens::Complement,
        expected: PutDomainError,
    ) {
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(lens, view, complement),
            Err(vec![expected])
        );
        let put = panproto_lens::put(lens, view, complement);
        assert!(
            matches!(put, Err(LensError::ComplementMismatch { .. })),
            "{put:?}"
        );
    }

    #[test]
    fn tree_put_rejects_a_view_failing_a_survival_predicate() {
        let mut lens = plain_lens();
        lens.compiled.conditional_survival.insert(
            "c".into(),
            Expr::Builtin(
                BuiltinOp::Eq,
                vec![var("level"), Expr::Lit(Literal::Int(2))],
            ),
        );
        let (view, complement) = panproto_lens::get(&lens, &source()).unwrap();
        assert!(view.nodes.contains_key(&1));
        let view = edited(&view, 1, "level", Value::Int(3));
        assert_refused(
            &lens,
            &view,
            &complement,
            PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Node(1),
            },
        );
    }

    #[test]
    fn tree_put_rejects_an_edited_computed_field() {
        let mut lens = plain_lens();
        lens.compiled.field_transforms.insert(
            "r".into(),
            vec![FieldTransform::ComputeField {
                target_key: "double".into(),
                expr: Expr::Builtin(BuiltinOp::Add, vec![var("n"), var("n")]),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
            }],
        );
        let (view, complement) = panproto_lens::get(&lens, &source()).unwrap();
        assert_eq!(view.nodes[&0].extra_fields["double"], Value::Int(2));
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Ok(())
        );
        let view = edited(&view, 0, "double", Value::Int(10));
        assert_refused(
            &lens,
            &view,
            &complement,
            PutDomainError::DerivedCellEdited {
                elem: ViewRef::Node(0),
                field: "double".into(),
            },
        );
    }

    #[test]
    fn tree_put_rejects_an_edited_non_invertible_field() {
        let mut lens = plain_lens();
        lens.compiled.field_transforms.insert(
            "r".into(),
            vec![FieldTransform::ApplyExpr {
                key: "n".into(),
                expr: Expr::Builtin(BuiltinOp::Add, vec![var("n"), Expr::Lit(Literal::Int(1))]),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
            }],
        );
        let (view, complement) = panproto_lens::get(&lens, &source()).unwrap();
        assert_eq!(view.nodes[&0].extra_fields["n"], Value::Int(2));
        let view = edited(&view, 0, "n", Value::Int(7));
        assert_refused(
            &lens,
            &view,
            &complement,
            PutDomainError::TransformNotInvertible {
                elem: ViewRef::Node(0),
                field: "n".into(),
            },
        );
    }

    /// An edit through a transform that has an inverse stays in the domain.
    #[test]
    fn tree_put_accepts_an_edit_through_an_inverse() {
        let mut lens = plain_lens();
        lens.compiled.field_transforms.insert(
            "r".into(),
            vec![FieldTransform::ApplyExpr {
                key: "n".into(),
                expr: Expr::Builtin(BuiltinOp::Add, vec![var("n"), Expr::Lit(Literal::Int(1))]),
                inverse: Some(Expr::Builtin(
                    BuiltinOp::Sub,
                    vec![var("n"), Expr::Lit(Literal::Int(1))],
                )),
                coercion_class: panproto_gat::CoercionClass::Iso,
            }],
        );
        let (view, complement) = panproto_lens::get(&lens, &source()).unwrap();
        let view = edited(&view, 0, "n", Value::Int(7));
        let back = panproto_lens::put(&lens, &view, &complement).unwrap();
        assert_eq!(back.nodes[&0].extra_fields["n"], Value::Int(6));
    }
}
