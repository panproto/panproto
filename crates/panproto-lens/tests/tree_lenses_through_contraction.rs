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

/// A tree compared as data: its nodes with their anchors and values, by
/// id, and its arcs, sorted.
type Shape = (
    Vec<(u32, Name, Option<FieldPresence>)>,
    Vec<(u32, u32, Edge)>,
);

/// The arcs of a tree, sorted, for comparing trees whose arc order is not
/// under test.
fn sorted_arcs(tree: &WInstance) -> Vec<(u32, u32, Edge)> {
    let mut arcs = tree.arcs.clone();
    arcs.sort_by_key(|a| (a.0, a.1));
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
            matches!(
                put,
                Err(LensError::PutDomain(
                    PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Arc {
                            parent: 0,
                            child: 3
                        }
                    }
                ))
            ),
            "{put:?}"
        );
    }

    /// A view arc whose label is the image of no source edge between its
    /// endpoints' source anchors, and is not a contracted pair's label,
    /// has nowhere to go in the source: `put` refuses it rather than
    /// dropping the arc and the node under it.
    #[test]
    fn a_view_arc_whose_label_no_source_edge_carries_is_rejected() {
        let src = schema(&[("r", "object"), ("a", "string")], &[prop("r", "a", "a")]);
        let m = migration(
            &[("r", "r"), ("a", "a")],
            vec![(prop("r", "a", "a"), prop("r", "a", "a"))],
            vec![],
        );
        let lens = lens(src.clone(), src, &m);
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "r")), (1, Node::new(1, "a"))]),
            vec![(0, 1, prop("r", "a", "a"))],
            vec![],
            0,
            "r".into(),
        );
        let (mut view, complement) = panproto_lens::get(&lens, &source).unwrap();
        view.nodes.insert(6, Node::new(6, "a"));
        view.arcs.push((0, 6, prop("r", "a", "bogus")));
        let view = WInstance::new(
            view.nodes,
            view.arcs,
            view.fans,
            view.root,
            view.schema_root,
        );

        let expected = PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Arc {
                parent: 0,
                child: 6,
            },
        };
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Err(vec![expected.clone()])
        );
        let put = panproto_lens::put(&lens, &view, &complement);
        assert!(
            matches!(&put, Err(LensError::PutDomain(first)) if *first == expected),
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
        expected: &PutDomainError,
    ) {
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(lens, view, complement),
            Err(vec![expected.clone()])
        );
        let put = panproto_lens::put(lens, view, complement);
        assert!(
            matches!(&put, Err(LensError::PutDomain(first)) if first == expected),
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
            &PutDomainError::SurvivalPredicateFails {
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
            &PutDomainError::DerivedCellEdited {
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
            &PutDomainError::TransformNotInvertible {
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

    /// `double = n + n` on `r`, with no inverse.
    fn doubling(lens: &mut Lens) {
        lens.compiled.field_transforms.insert(
            "r".into(),
            vec![FieldTransform::ComputeField {
                target_key: "double".into(),
                expr: Expr::Builtin(BuiltinOp::Add, vec![var("n"), var("n")]),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
            }],
        );
    }

    /// A survival predicate failing on one node does not hide an edited
    /// computed field on another: both are reported.
    #[test]
    fn check_put_domain_reports_a_failed_predicate_and_an_edited_derived_cell() {
        let mut lens = plain_lens();
        lens.compiled.conditional_survival.insert(
            "c".into(),
            Expr::Builtin(
                BuiltinOp::Eq,
                vec![var("level"), Expr::Lit(Literal::Int(2))],
            ),
        );
        doubling(&mut lens);
        let (view, complement) = panproto_lens::get(&lens, &source()).unwrap();
        let view = edited(&view, 1, "level", Value::Int(3));
        let view = edited(&view, 0, "double", Value::Int(10));
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Err(vec![
                PutDomainError::SurvivalPredicateFails {
                    elem: ViewRef::Node(1),
                },
                PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Node(0),
                    field: "double".into(),
                },
            ])
        );
    }

    /// A structural violation does not hide the value violations of the
    /// nodes put can still restore: an arc under a label no source edge
    /// carries and an edited computed field are both reported, and `put`
    /// returns the first.
    #[test]
    fn check_put_domain_reports_structural_and_value_violations_together() {
        let mut lens = plain_lens();
        doubling(&mut lens);
        let (view, complement) = panproto_lens::get(&lens, &source()).unwrap();
        let mut view = edited(&view, 0, "double", Value::Int(10));
        view.nodes.insert(5, Node::new(5, "c"));
        view.arcs.push((0, 5, prop("r", "c", "stray")));
        let view = WInstance::new(view.nodes, view.arcs, vec![], 0, "r".into());
        let unrepresentable = PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Arc {
                parent: 0,
                child: 5,
            },
        };
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Err(vec![
                unrepresentable.clone(),
                PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Node(0),
                    field: "double".into(),
                },
            ])
        );
        let put = panproto_lens::put(&lens, &view, &complement);
        assert!(
            matches!(&put, Err(LensError::PutDomain(first)) if *first == unrepresentable),
            "{put:?}"
        );
    }

    /// An id collision, which stops the reassembly `put` makes, does not
    /// hide an edited computed field on another node.
    #[test]
    fn check_put_domain_reports_an_id_collision_and_an_edited_derived_cell() {
        let src = schema(
            &[("r", "object"), ("c", "object"), ("d", "object")],
            &[prop("r", "c", "c"), prop("r", "d", "d")],
        );
        let tgt = schema(&[("r", "object"), ("c", "object")], &[prop("r", "c", "c")]);
        let m = migration(
            &[("r", "r"), ("c", "c")],
            vec![(prop("r", "c", "c"), prop("r", "c", "c"))],
            vec![],
        );
        let mut lens = lens(src, tgt, &m);
        doubling(&mut lens);
        let mut r = Node::new(0, "r");
        r.extra_fields.insert("n".into(), Value::Int(1));
        let source = WInstance::new(
            HashMap::from([(0, r), (1, Node::new(1, "c")), (2, Node::new(2, "d"))]),
            vec![(0, 1, prop("r", "c", "c")), (0, 2, prop("r", "d", "d"))],
            vec![],
            0,
            "r".into(),
        );
        let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
        assert!(complement.dropped_nodes.contains_key(&2));
        let mut view = edited(&view, 0, "double", Value::Int(10));
        view.nodes.insert(2, Node::new(2, "c"));
        view.arcs.push((0, 2, prop("r", "c", "c")));
        let view = WInstance::new(view.nodes, view.arcs, vec![], 0, "r".into());
        assert_eq!(
            panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement),
            Err(vec![
                PutDomainError::IdCollision {
                    elem: ViewRef::Node(2),
                },
                PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Node(0),
                    field: "double".into(),
                },
            ])
        );
    }
}

/// What a tree complement counts as residue: the information in the source
/// that the view lacks, by the same criteria as the graph complement.
mod residue {
    use super::*;

    /// The static classification and `put_without_complement` agree on
    /// `lens`: the lens is an isomorphism exactly when a source is rebuilt
    /// from `view` alone.
    fn assert_classification_agrees(lens: &Lens, view: &WInstance) {
        let rebuilt = panproto_lens::put_without_complement(lens, view);
        assert_eq!(
            lens.is_isomorphism(),
            rebuilt.is_ok(),
            "classification {:?} disagrees with put_without_complement {:?}",
            lens.obstruction_to_isomorphism(),
            rebuilt.as_ref().err()
        );
    }

    /// A contracting lens drops the interior node, which is residue, so it
    /// is not an isomorphism and the classification and
    /// `put_without_complement` both say so. What `get` records about the
    /// contraction itself (which pair contracted to which label, and which
    /// dropped node collapsed into which ancestor) is bookkeeping: on its
    /// own it leaves the residue trivial, since it is derivable from the
    /// view, the dropped nodes, the parent map and the resolver.
    #[test]
    fn tree_residue_classification_agrees_on_contraction_bookkeeping() {
        let lens = contracting_lens(false);
        let (view, complement) = panproto_lens::get(&lens, &contracting_source()).unwrap();
        assert!(!lens.is_isomorphism());
        assert!(!complement.residue_is_trivial(), "{complement:?}");
        assert_classification_agrees(&lens, &view);

        assert_eq!(
            complement.contraction_choices,
            HashMap::from([((0, 2), prop("r", "leaf", "label"))])
        );
        let mut bookkeeping = panproto_lens::Complement::empty();
        bookkeeping
            .contraction_choices
            .clone_from(&complement.contraction_choices);
        bookkeeping
            .contracted_into
            .clone_from(&complement.contracted_into);
        assert!(bookkeeping.residue_is_trivial(), "{bookkeeping:?}");
    }

    /// An injective vertex rename loses nothing: the residue is trivial and
    /// the source is rebuilt from the view alone.
    #[test]
    fn tree_residue_classification_agrees_on_an_injective_rename() {
        let src = schema(&[("r", "object"), ("a", "string")], &[prop("r", "a", "a")]);
        let tgt = schema(&[("r", "object"), ("b", "string")], &[prop("r", "b", "a")]);
        let m = migration(
            &[("r", "r"), ("a", "b")],
            vec![(prop("r", "a", "a"), prop("r", "b", "a"))],
            vec![],
        );
        let lens = lens(src, tgt, &m);
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (
                    1,
                    Node::new(1, "a").with_value(FieldPresence::Present(Value::Str("x".into()))),
                ),
            ]),
            vec![(0, 1, prop("r", "a", "a"))],
            vec![],
            0,
            "r".into(),
        );
        let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
        assert!(complement.residue_is_trivial(), "{complement:?}");
        assert!(
            lens.is_isomorphism(),
            "{:?}",
            lens.obstruction_to_isomorphism()
        );
        assert_classification_agrees(&lens, &view);
        let back = panproto_lens::put_without_complement(&lens, &view)
            .expect("an injective rename is an isomorphism");
        assert_eq!(sorted_arcs(&back), sorted_arcs(&source));
        assert_eq!(back.nodes[&1].anchor, Name::from("a"));
    }

    /// Two parallel source edges merged onto one target edge: the view
    /// cannot say which one an arc came from, so that is residue and the
    /// source cannot be rebuilt from the view alone.
    #[test]
    fn tree_residue_classification_agrees_on_merged_parallel_edges() {
        let src = schema(
            &[("r", "object"), ("a", "string")],
            &[prop("r", "a", "x"), prop("r", "a", "y")],
        );
        let tgt = schema(&[("r", "object"), ("a", "string")], &[prop("r", "a", "x")]);
        let m = migration(
            &[("r", "r"), ("a", "a")],
            vec![
                (prop("r", "a", "x"), prop("r", "a", "x")),
                (prop("r", "a", "y"), prop("r", "a", "x")),
            ],
            vec![],
        );
        let lens = lens(src, tgt, &m);
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "a")),
            ]),
            vec![(0, 1, prop("r", "a", "x")), (0, 2, prop("r", "a", "y"))],
            vec![],
            0,
            "r".into(),
        );
        let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
        assert!(!complement.residue_is_trivial(), "{complement:?}");
        assert_eq!(complement.arc_edges.len(), 2);
        assert!(!lens.is_isomorphism());
        assert_classification_agrees(&lens, &view);
        // With the complement the source comes back exactly.
        let back = panproto_lens::put(&lens, &view, &complement).unwrap();
        assert_eq!(sorted_arcs(&back), sorted_arcs(&source));
    }

    /// Only arcs whose label the view does not determine are recorded.
    #[test]
    fn an_arc_whose_label_the_view_determines_is_not_residue() {
        let lens = contracting_lens(false);
        let source = WInstance::new(
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
        );
        let (_, complement) = panproto_lens::get(&lens, &source).unwrap();
        assert!(
            complement.arc_edges.is_empty(),
            "{:?}",
            complement.arc_edges
        );
    }
}

/// Fresh ids for nodes a tree view adds.
mod fresh_ids {
    use super::*;
    use panproto_inst::Fan;
    use panproto_lens::asymmetric::fresh_view_node_id;

    fn two_leaves() -> (Lens, WInstance) {
        let s = schema(
            &[("r", "object"), ("leaf", "string")],
            &[prop("r", "leaf", "leaf")],
        );
        let m = migration(
            &[("r", "r"), ("leaf", "leaf")],
            vec![(prop("r", "leaf", "leaf"), prop("r", "leaf", "leaf"))],
            vec![],
        );
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "r")), (1, leaf(1, "a")), (2, leaf(2, "b"))]),
            vec![
                (0, 1, prop("r", "leaf", "leaf")),
                (0, 2, prop("r", "leaf", "leaf")),
            ],
            vec![Fan::new("rel", 0).with_child("x", 1)],
            0,
            "r".into(),
        );
        (lens(s.clone(), s, &m), source)
    }

    /// Get records the floor of the source's graph form: one above the
    /// largest node id and the fan node its embedding allocates.
    #[test]
    fn tree_get_sets_the_id_floor() {
        let (lens, source) = two_leaves();
        let (_, complement) = panproto_lens::get(&lens, &source).unwrap();
        let (_, embedding) = panproto_inst::GInstance::from_wtype_embedded(&source, None).unwrap();
        assert_eq!(complement.id_floor, 4);
        assert_eq!(complement.id_floor, embedding.next_node);
    }

    /// A view that deletes the largest source node and adds one does not
    /// hand the new node the deleted id.
    #[test]
    fn a_fresh_view_node_never_reuses_a_deleted_source_id() {
        let (lens, source) = two_leaves();
        let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
        let mut nodes = view.nodes.clone();
        nodes.remove(&2);
        let arcs: Vec<_> = view.arcs.iter().filter(|a| a.1 != 2).cloned().collect();
        let edited = WInstance::new(nodes, arcs, view.fans, 0, "r".into());
        let fresh = fresh_view_node_id(&edited, &complement).unwrap();
        assert_ne!(fresh, 2);
        assert!(fresh >= complement.id_floor);

        let mut nodes = edited.nodes.clone();
        nodes.insert(fresh, leaf(fresh, "new"));
        let mut arcs = edited.arcs.clone();
        arcs.push((0, fresh, prop("r", "leaf", "leaf")));
        let added = WInstance::new(nodes, arcs, edited.fans, 0, "r".into());
        let back = panproto_lens::put(&lens, &added, &complement).unwrap();
        assert!(!back.nodes.contains_key(&2));
        assert_eq!(
            back.nodes[&fresh].value,
            Some(FieldPresence::Present(Value::Str("new".into())))
        );
    }

    /// `r(0) -> leaf(top)`.
    fn leaf_at(top: u32) -> (Lens, WInstance) {
        let (lens, _) = two_leaves();
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "r")), (top, leaf(top, "a"))]),
            vec![(0, top, prop("r", "leaf", "leaf"))],
            vec![],
            0,
            "r".into(),
        );
        (lens, source)
    }

    /// The fresh id for a view of `r(0) -> leaf(top)` that deletes the leaf,
    /// and whether the source's graph form can allocate one more node.
    fn fresh_after_deleting(top: u32) -> (Result<u32, panproto_inst::InstError>, bool) {
        let (lens, source) = leaf_at(top);
        let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
        let edited = WInstance::new(
            HashMap::from([(0, view.nodes[&0].clone())]),
            vec![],
            vec![],
            0,
            "r".into(),
        );
        // A source whose graph form cannot be built for want of ids can
        // allocate none.
        let graph_allocates = panproto_inst::GInstance::from_wtype_embedded(&source, None)
            .is_ok_and(|(_, mut embedding)| embedding.allocate_node().is_ok());
        (fresh_view_node_id(&edited, &complement), graph_allocates)
    }

    /// A source holding the largest node id leaves no fresh id: allocation
    /// reports exhaustion rather than handing out the deleted id.
    #[test]
    fn a_source_holding_the_largest_id_leaves_no_fresh_id() {
        let (fresh, graph_allocates) = fresh_after_deleting(u32::MAX);
        assert!(
            matches!(fresh, Err(panproto_inst::InstError::NodeIdsExhausted)),
            "{fresh:?}"
        );
        assert!(!graph_allocates);
    }

    /// Near the top of the id space the tree floor allocates exactly as the
    /// source's graph form does: no id when the graph form has none left,
    /// the floor otherwise.
    #[test]
    fn fresh_ids_near_the_top_follow_the_graph_form() {
        for top in [u32::MAX - 2, u32::MAX - 1] {
            let (fresh, graph_allocates) = fresh_after_deleting(top);
            assert_eq!(fresh.is_ok(), graph_allocates, "top {top}: {fresh:?}");
            if let Ok(id) = fresh {
                assert_eq!(id, top + 1);
            } else {
                assert!(
                    matches!(fresh, Err(panproto_inst::InstError::NodeIdsExhausted)),
                    "{fresh:?}"
                );
            }
        }
    }
}

/// Edit translation through a lens that drops interior vertices: an edit
/// under a dropped node lands, in the view, where whole-state `get` puts it.
mod edits {
    use proptest::prelude::*;

    use super::*;
    use panproto_inst::TreeEdit;
    use panproto_lens::EditLens;

    fn edit_lens(lens: Lens, source: &WInstance) -> EditLens {
        let mut edit = EditLens::from_lens(lens, Protocol::default());
        edit.initialize(source).unwrap();
        edit
    }

    fn state_lens(edit: &EditLens) -> Lens {
        Lens {
            compiled: edit.compiled.clone(),
            src_schema: edit.src_schema.clone(),
            tgt_schema: edit.tgt_schema.clone(),
        }
    }

    /// A view compared as data: nodes with their anchors and values, and
    /// arcs as a set. Sibling order under contraction is not compared here.
    fn shape(view: &WInstance) -> Shape {
        let mut nodes: Vec<_> = view
            .nodes
            .values()
            .map(|n| (n.id, n.anchor.clone(), n.value.clone()))
            .collect();
        nodes.sort_by_key(|n| n.0);
        (nodes, sorted_arcs(view))
    }

    #[test]
    fn a_tree_edit_under_a_dropped_ancestor_translates_through_contraction() {
        let source = contracting_source();
        let mut lens = edit_lens(contracting_lens(false), &source);
        let (view, _) = panproto_lens::get(&state_lens(&lens), &source).unwrap();

        let insert = TreeEdit::InsertNode {
            parent: 1,
            child_id: 3,
            node: leaf(3, "y"),
            edge: prop("m", "leaf", "leaf"),
        };
        let translated = lens.get_edit(insert.clone()).unwrap();
        let TreeEdit::InsertNode {
            parent,
            child_id,
            node,
            edge,
        } = &translated
        else {
            panic!("expected an insert, got {translated:?}");
        };
        assert_eq!((*parent, *child_id), (0, 3));
        assert_eq!(
            (&node.anchor, &node.value),
            (&leaf(3, "y").anchor, &leaf(3, "y").value)
        );
        assert_eq!(edge, &prop("r", "leaf", "label"));
        let mut edited_view = view;
        translated.apply(&mut edited_view).unwrap();
        let mut edited_source = source;
        insert.apply(&mut edited_source).unwrap();
        let (expected, _) = panproto_lens::get(&state_lens(&lens), &edited_source).unwrap();
        assert_eq!(shape(&edited_view), shape(&expected));
    }

    /// `r -> m -> m -> ... -> leaf` with `m` dropped (a recursive wrapper).
    fn recursive_lens() -> Lens {
        let src = schema(
            &[("r", "object"), ("m", "object"), ("leaf", "string")],
            &[
                prop("r", "m", "m"),
                prop("m", "m", "m"),
                prop("m", "leaf", "leaf"),
            ],
        );
        let tgt = schema(
            &[("r", "object"), ("leaf", "string")],
            &[prop("r", "leaf", "label")],
        );
        let m = migration(
            &[("r", "r"), ("leaf", "leaf")],
            vec![],
            vec![(("r", "leaf"), prop("r", "leaf", "label"))],
        );
        lens(src, tgt, &m)
    }

    /// `r -> m -> leaf` where `m` survives only when its `keep` field is
    /// true, and the pair `(r, leaf)` resolves to `r -> leaf`.
    fn conditional_lens() -> Lens {
        let s = schema(
            &[("r", "object"), ("m", "object"), ("leaf", "string")],
            &[
                prop("r", "m", "m"),
                prop("m", "leaf", "leaf"),
                prop("r", "leaf", "label"),
            ],
        );
        let m = migration(
            &[("r", "r"), ("m", "m"), ("leaf", "leaf")],
            vec![
                (prop("r", "m", "m"), prop("r", "m", "m")),
                (prop("m", "leaf", "leaf"), prop("m", "leaf", "leaf")),
                (prop("r", "leaf", "label"), prop("r", "leaf", "label")),
            ],
            vec![(("r", "leaf"), prop("r", "leaf", "label"))],
        );
        let mut l = lens(s.clone(), s, &m);
        l.compiled.conditional_survival.insert(
            "m".into(),
            panproto_expr::Expr::Var(std::sync::Arc::from("keep")),
        );
        l
    }

    fn m_node(id: u32, keep: bool) -> Node {
        let mut n = Node::new(id, "m");
        n.extra_fields.insert("keep".into(), Value::Bool(keep));
        n
    }

    /// One step of a random edit script, over the node ids present.
    #[derive(Clone, Debug)]
    enum Step {
        /// Insert a leaf (`true`) or an `m` node under the `usize`-th node.
        Insert(usize, bool, bool),
        /// Delete the `usize`-th node if it is a leaf.
        Delete(usize),
        /// Move the first node under the second.
        Move(usize, usize),
    }

    fn arb_steps() -> impl Strategy<Value = Vec<Step>> {
        prop::collection::vec(
            prop_oneof![
                (0usize..16, any::<bool>(), any::<bool>())
                    .prop_map(|(p, l, k)| Step::Insert(p, l, k)),
                (0usize..16).prop_map(Step::Delete),
                (0usize..16, 0usize..16).prop_map(|(a, b)| Step::Move(a, b)),
            ],
            1..10,
        )
    }

    /// The source edge between two nodes by their anchors.
    fn source_edge(src: &Schema, parent: &Name, child: &Name) -> Option<Edge> {
        src.edges_between(parent, child).first().cloned()
    }

    fn is_descendant(tree: &WInstance, node: u32, of: u32) -> bool {
        let mut current = Some(node);
        while let Some(n) = current {
            if n == of {
                return true;
            }
            current = tree.parent_map.get(&n).copied();
        }
        false
    }

    /// The concrete edit a step names on `source`, or `None` when it does
    /// not apply (a delete of a non-leaf, a move into its own subtree, a
    /// pair with no schema edge).
    fn concrete(
        step: &Step,
        source: &WInstance,
        src: &Schema,
        next: u32,
        conditional: bool,
    ) -> Option<TreeEdit> {
        let mut ids: Vec<u32> = source.nodes.keys().copied().collect();
        ids.sort_unstable();
        let pick = |i: usize| ids[i % ids.len()];
        match *step {
            Step::Insert(p, as_leaf, keep) => {
                let parent = pick(p);
                let node = if as_leaf {
                    leaf(next, "n")
                } else if conditional {
                    m_node(next, keep)
                } else {
                    Node::new(next, "m")
                };
                let edge = source_edge(src, &source.nodes[&parent].anchor, &node.anchor)?;
                Some(TreeEdit::InsertNode {
                    parent,
                    child_id: next,
                    node,
                    edge,
                })
            }
            Step::Delete(i) => {
                let id = pick(i);
                (id != source.root && !source.arcs.iter().any(|&(parent, _, _)| parent == id))
                    .then_some(TreeEdit::DeleteNode { id })
            }
            Step::Move(a, b) => {
                let (node_id, new_parent) = (pick(a), pick(b));
                if node_id == source.root || is_descendant(source, new_parent, node_id) {
                    return None;
                }
                let edge = source_edge(
                    src,
                    &source.nodes[&new_parent].anchor,
                    &source.nodes[&node_id].anchor,
                )?;
                Some(TreeEdit::MoveSubtree {
                    node_id,
                    new_parent,
                    edge,
                })
            }
        }
    }

    /// Law (1): translating each edit and applying it to the view gives the
    /// view of the edited source.
    fn check_consistency(
        lens: Lens,
        mut source: WInstance,
        steps: &[Step],
        conditional: bool,
    ) -> Result<(), TestCaseError> {
        let src = lens.src_schema.clone();
        let mut edit = edit_lens(lens, &source);
        let (mut view, _) = panproto_lens::get(&state_lens(&edit), &source).unwrap();
        let mut next = source.nodes.keys().max().map_or(0, |m| m + 1);
        for step in steps {
            let Some(e) = concrete(step, &source, &src, next, conditional) else {
                continue;
            };
            next += 1;
            let translated = edit
                .get_edit(e.clone())
                .map_err(|err| TestCaseError::fail(format!("{e:?} did not translate: {err}")))?;
            e.apply(&mut source).unwrap();
            translated.apply(&mut view).map_err(|err| {
                TestCaseError::fail(format!(
                    "{translated:?} (from {e:?}) did not apply to the view: {err}"
                ))
            })?;
            let (expected, _) = panproto_lens::get(&state_lens(&edit), &source).unwrap();
            prop_assert_eq!(
                shape(&view),
                shape(&expected),
                "after {:?} translated to {:?}",
                e,
                translated
            );
        }
        Ok(())
    }

    proptest! {
        #[test]
        fn tree_get_edit_consistency_with_dropped_interiors(steps in arb_steps()) {
            check_consistency(contracting_lens(false), contracting_source(), &steps, false)?;
        }

        #[test]
        fn tree_get_edit_consistency_through_a_recursive_wrapper(steps in arb_steps()) {
            let source = WInstance::new(
                HashMap::from([
                    (0, Node::new(0, "r")),
                    (1, Node::new(1, "m")),
                    (2, Node::new(2, "m")),
                    (3, Node::new(3, "m")),
                    (4, leaf(4, "x")),
                ]),
                vec![
                    (0, 1, prop("r", "m", "m")),
                    (1, 2, prop("m", "m", "m")),
                    (2, 3, prop("m", "m", "m")),
                    (3, 4, prop("m", "leaf", "leaf")),
                ],
                vec![],
                0,
                "r".into(),
            );
            check_consistency(recursive_lens(), source, &steps, false)?;
        }

        #[test]
        fn tree_get_edit_consistency_with_a_conditionally_dropped_interior(steps in arb_steps()) {
            let source = WInstance::new(
                HashMap::from([
                    (0, Node::new(0, "r")),
                    (1, m_node(1, false)),
                    (2, leaf(2, "x")),
                    (3, m_node(3, true)),
                    (4, leaf(4, "z")),
                ]),
                vec![
                    (0, 1, prop("r", "m", "m")),
                    (1, 2, prop("m", "leaf", "leaf")),
                    (0, 3, prop("r", "m", "m")),
                    (3, 4, prop("m", "leaf", "leaf")),
                ],
                vec![],
                0,
                "r".into(),
            );
            check_consistency(conditional_lens(), source, &steps, true)?;
        }
    }
}

mod composition_rule {
    use proptest::prelude::*;

    use super::*;
    use panproto_lens::{SequentialTreeLens, TreeComposite};

    /// The identity lens on `schema`.
    fn identity(schema: &Schema) -> Lens {
        let vertices: Vec<(&str, &str)> = schema
            .vertices
            .keys()
            .map(|v| (v.as_ref(), v.as_ref()))
            .collect();
        let edges = schema
            .edges
            .keys()
            .map(|e| (e.clone(), e.clone()))
            .collect();
        lens(
            schema.clone(),
            schema.clone(),
            &migration(&vertices, edges, vec![]),
        )
    }

    /// `renaming_lens` of the composition tests, over the parallel target.
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

    /// Pairs whose composite contracts `m`: a contraction followed by a
    /// rename, and an identity followed by a contraction.
    fn contracting_pairs() -> Vec<(Lens, Lens)> {
        let second = contracting_lens(false);
        vec![
            (contracting_lens(true), renaming_lens()),
            (identity(&second.src_schema), second),
        ]
    }

    /// `r(0)` with `m` children, each with leaves, as `counts` gives them.
    fn source(counts: &[Vec<String>]) -> WInstance {
        let mut nodes = HashMap::from([(0, Node::new(0, "r"))]);
        let mut arcs = Vec::new();
        let mut next = 1;
        for leaves in counts {
            let m = next;
            next += 1;
            nodes.insert(m, Node::new(m, "m"));
            arcs.push((0, m, prop("r", "m", "m")));
            for value in leaves {
                nodes.insert(next, leaf(next, value));
                arcs.push((m, next, prop("m", "leaf", "leaf")));
                next += 1;
            }
        }
        WInstance::new(nodes, arcs, vec![], 0, "r".into())
    }

    /// Every leaf value of `view` replaced, as an in-domain edit.
    fn edited(view: &WInstance) -> WInstance {
        let mut view = view.clone();
        for node in view.nodes.values_mut() {
            if node.value.is_some() {
                node.value = Some(FieldPresence::Present(Value::Str(format!("e{}", node.id))));
            }
        }
        view
    }

    /// Every node field of `a` and `b`, by id, or the first difference.
    fn node_difference(a: &WInstance, b: &WInstance) -> Option<String> {
        let mut ids: Vec<u32> = a.nodes.keys().chain(b.nodes.keys()).copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .find_map(|id| match (a.nodes.get(&id), b.nodes.get(&id)) {
                (Some(x), Some(y))
                    if x.id == y.id
                        && x.anchor == y.anchor
                        && x.value == y.value
                        && x.discriminator == y.discriminator
                        && x.extra_fields == y.extra_fields
                        && x.position == y.position
                        && x.shape == y.shape
                        && x.annotations == y.annotations =>
                {
                    None
                }
                (x, y) => Some(format!("node {id}: {x:?} against {y:?}")),
            })
    }

    /// The children of every parent, in arc order, with their edges.
    fn child_sequences(tree: &WInstance) -> std::collections::BTreeMap<u32, Vec<(u32, Edge)>> {
        let mut sequences: std::collections::BTreeMap<u32, Vec<(u32, Edge)>> =
            std::collections::BTreeMap::new();
        for (parent, child, edge) in &tree.arcs {
            sequences
                .entry(*parent)
                .or_default()
                .push((*child, edge.clone()));
        }
        sequences
    }

    /// Two views compared as the tree view equivalence compares views with
    /// no synthesized node: exactly, on every node field, the children of
    /// every parent in order, the fans, and the roots.
    fn view_difference(a: &WInstance, b: &WInstance) -> Option<String> {
        if (a.root, &a.schema_root) != (b.root, &b.schema_root) {
            return Some(format!(
                "roots {:?} against {:?}",
                (a.root, &a.schema_root),
                (b.root, &b.schema_root)
            ));
        }
        node_difference(a, b)
            .or_else(|| {
                (child_sequences(a) != child_sequences(b)).then(|| {
                    format!(
                        "children {:?} against {:?}",
                        child_sequences(a),
                        child_sequences(b)
                    )
                })
            })
            .or_else(|| {
                (a.fans != b.fans).then(|| format!("fans {:?} against {:?}", a.fans, b.fans))
            })
    }

    /// Two sources compared as `identical` compares instances: their graph
    /// forms have the same nodes and the same edges under the same ids (the
    /// arc positions), and the trees have the same roots.
    fn source_difference(a: &WInstance, b: &WInstance) -> Option<String> {
        let (ga, gb) = (
            panproto_inst::GInstance::from_wtype(a).unwrap(),
            panproto_inst::GInstance::from_wtype(b).unwrap(),
        );
        if (a.root, &a.schema_root) == (b.root, &b.schema_root)
            && panproto_inst::GInstance::identical(&ga, &gb)
        {
            None
        } else {
            Some(format!("{a:?} against {b:?}"))
        }
    }

    /// The fused lens and the sequential pair agree on `source`: views
    /// equivalent (neither synthesizes a node, so equivalence is exact,
    /// child order included), and sources written back identical for the
    /// view and an edit of it.
    fn agree(
        fused: &Lens,
        sequential: &SequentialTreeLens,
        source: &WInstance,
    ) -> Result<(), TestCaseError> {
        let (seq_view, seq_c) = sequential.get(source).unwrap();
        let (view, c) = panproto_lens::get(fused, source)
            .map_err(|e| TestCaseError::fail(format!("the fused get failed: {e}")))?;
        prop_assert!(c.synthesized_nodes.is_empty() && seq_c.1.synthesized_nodes.is_empty());
        if let Some(difference) = view_difference(&view, &seq_view) {
            return Err(TestCaseError::fail(format!("views differ: {difference}")));
        }
        for v in [seq_view.clone(), edited(&seq_view)] {
            let back = sequential.put(&v, &seq_c).unwrap();
            let fused_back = panproto_lens::put(fused, &v, &c)
                .map_err(|e| TestCaseError::fail(format!("the fused put failed: {e}")))?;
            if let Some(difference) = source_difference(&fused_back, &back) {
                return Err(TestCaseError::fail(format!("puts differ: {difference}")));
            }
        }
        Ok(())
    }

    /// The comparisons the composition law uses see what a projection onto
    /// ids, anchors, values and sorted arcs does not: child order, extra
    /// fields and fans.
    #[test]
    fn the_law_comparisons_see_child_order_fields_and_fans() {
        let tree = two_label_source(&["a".into(), "b".into()]);
        assert_eq!(view_difference(&tree, &tree), None);
        assert_eq!(source_difference(&tree, &tree), None);

        let mut reordered = tree.clone();
        reordered.arcs.reverse();
        let reordered = WInstance::new(reordered.nodes, reordered.arcs, vec![], 0, "r".into());
        assert_eq!(sorted_arcs(&reordered), sorted_arcs(&tree));
        assert!(view_difference(&reordered, &tree).is_some());
        assert!(source_difference(&reordered, &tree).is_some());

        let mut annotated = tree.clone();
        annotated
            .nodes
            .get_mut(&1)
            .unwrap()
            .extra_fields
            .insert("k".into(), Value::Int(1));
        assert!(view_difference(&annotated, &tree).is_some());
        assert!(source_difference(&annotated, &tree).is_some());

        let mut fanned = tree.clone();
        fanned
            .fans
            .push(panproto_inst::Fan::new("rel", 0).with_child("x", 1));
        assert!(view_difference(&fanned, &tree).is_some());
    }

    /// `r -> leaf` under the edge `leaf`.
    fn direct_schema() -> Schema {
        schema(
            &[("r", "object"), ("leaf", "string")],
            &[prop("r", "leaf", "leaf")],
        )
    }

    /// `r -> mid -> leaf`.
    fn expanded_schema() -> Schema {
        schema(
            &[("r", "object"), ("mid", "object"), ("leaf", "string")],
            &[prop("r", "mid", "mid"), prop("mid", "leaf", "leaf")],
        )
    }

    /// The lens from `direct_schema` to `expanded_schema` that expands every
    /// `r -> leaf` arc into `r -> mid -> leaf` through a synthesized `mid`
    /// node.
    fn expanding_lens() -> Lens {
        let compiled = panproto_inst::CompiledMigration {
            surviving_verts: ["r", "leaf"].into_iter().map(Name::from).collect(),
            expansion_path: HashMap::from([(
                (Name::from("r"), Name::from("leaf")),
                vec![Name::from("mid")],
            )]),
            ..Default::default()
        };
        Lens {
            compiled,
            src_schema: direct_schema(),
            tgt_schema: expanded_schema(),
        }
    }

    /// A lens out of `expanded_schema`: `1` drops `leaf`, truncating an
    /// expansion after `mid`; `2` drops `mid`, contracting the path back to
    /// `r -> leaf`; `3` renames `leaf` to `item`; `4` renames `mid` to
    /// `hub`; `5` drops `mid` and `leaf`; `6` drops `leaf` and renames `mid`
    /// to `hub`; anything else is the identity.
    fn after_expansion(kind: u8) -> Lens {
        let src = expanded_schema();
        let (r_mid, mid_leaf) = (prop("r", "mid", "mid"), prop("mid", "leaf", "leaf"));
        let (r_hub, hub_leaf) = (prop("r", "hub", "mid"), prop("hub", "leaf", "leaf"));
        match kind {
            1 => lens(
                src,
                schema(
                    &[("r", "object"), ("mid", "object")],
                    std::slice::from_ref(&r_mid),
                ),
                &migration(
                    &[("r", "r"), ("mid", "mid")],
                    vec![(r_mid.clone(), r_mid)],
                    vec![],
                ),
            ),
            2 => lens(
                src,
                direct_schema(),
                &migration(
                    &[("r", "r"), ("leaf", "leaf")],
                    vec![],
                    vec![(("r", "leaf"), prop("r", "leaf", "leaf"))],
                ),
            ),
            3 => {
                let mid_item = prop("mid", "item", "leaf");
                lens(
                    src,
                    schema(
                        &[("r", "object"), ("mid", "object"), ("item", "string")],
                        &[r_mid.clone(), mid_item.clone()],
                    ),
                    &migration(
                        &[("r", "r"), ("mid", "mid"), ("leaf", "item")],
                        vec![(r_mid.clone(), r_mid), (mid_leaf, mid_item)],
                        vec![],
                    ),
                )
            }
            4 => lens(
                src,
                schema(
                    &[("r", "object"), ("hub", "object"), ("leaf", "string")],
                    &[r_hub.clone(), hub_leaf.clone()],
                ),
                &migration(
                    &[("r", "r"), ("mid", "hub"), ("leaf", "leaf")],
                    vec![(r_mid, r_hub), (mid_leaf, hub_leaf)],
                    vec![],
                ),
            ),
            5 => lens(
                src,
                schema(&[("r", "object")], &[]),
                &migration(&[("r", "r")], vec![], vec![]),
            ),
            6 => lens(
                src,
                schema(
                    &[("r", "object"), ("hub", "object")],
                    std::slice::from_ref(&r_hub),
                ),
                &migration(&[("r", "r"), ("mid", "hub")], vec![(r_mid, r_hub)], vec![]),
            ),
            _ => identity(&src),
        }
    }

    /// A pair around an expansion: with `first_expands`, `expanding_lens`
    /// then `after_expansion(kind)`; otherwise the identity on
    /// `direct_schema` then `expanding_lens`. Both read `direct_source`.
    fn expansion_pair(kind: u8, first_expands: bool) -> (Lens, Lens) {
        if first_expands {
            (expanding_lens(), after_expansion(kind))
        } else {
            (identity(&direct_schema()), expanding_lens())
        }
    }

    /// `r(0)` with one `leaf` child per value.
    fn direct_source(values: &[String]) -> WInstance {
        let mut nodes = HashMap::from([(0, Node::new(0, "r"))]);
        let mut arcs = Vec::new();
        for (i, value) in values.iter().enumerate() {
            let id = u32::try_from(i).unwrap() + 1;
            nodes.insert(id, leaf(id, value));
            arcs.push((0, id, prop("r", "leaf", "leaf")));
        }
        WInstance::new(nodes, arcs, vec![], 0, "r".into())
    }

    /// The pair the composition rule was found unsound on when it checked
    /// only the composite's plan: the first lens expands `r -> leaf` into
    /// `r -> mid -> leaf` and the second drops `leaf`. The composite carries
    /// no expansion, since its arc's child is dropped, yet the pair
    /// synthesizes `mid` and the second lens keeps it. The pair is kept in
    /// sequence, and the fused lens would lose `mid`.
    #[test]
    fn a_second_lens_truncating_an_expansion_is_kept_in_sequence() {
        let (first, second) = expansion_pair(1, true);
        let source = direct_source(&["x".into()]);
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        let TreeComposite::Sequential(sequential) = composite else {
            panic!("an expansion the second lens truncates was fused: {composite:?}");
        };
        let (view, (c1, _)) = sequential.get(&source).unwrap();
        let mids: Vec<u32> = view
            .nodes
            .values()
            .filter(|n| n.anchor.as_ref() == "mid")
            .map(|n| n.id)
            .collect();
        assert_eq!(
            mids.len(),
            1,
            "the pair keeps the synthesized mid: {view:?}"
        );
        assert!(c1.synthesized_nodes.contains(&mids[0]));
        assert!(view.arcs.iter().any(|(p, c, _)| (*p, *c) == (0, mids[0])));

        let fused = panproto_lens::compose(&first, &second).unwrap();
        let (fused_view, _) = panproto_lens::get(&fused, &source).unwrap();
        assert!(
            fused_view
                .nodes
                .values()
                .all(|n| n.anchor.as_ref() != "mid")
        );
    }

    /// The schema `r -> anchor`, with the edges `label` and, when `both`
    /// is set, `other`.
    fn leaf_schema(anchor: &str, both: bool) -> Schema {
        let mut edges = vec![prop("r", anchor, "label")];
        if both {
            edges.push(prop("r", anchor, "other"));
        }
        schema(&[("r", "object"), (anchor, "string")], &edges)
    }

    /// A contraction-free step out of `leaf_schema(anchor, both)`: `0` the
    /// identity, `1` a rename of the leaf vertex, `2` a swap of the two
    /// edge labels, `3` a drop of the `other` edge (the last two only when
    /// both edges exist). With `explicit` unset, the edge map leaves out
    /// every edge whose kind and label carry over unchanged, which the
    /// migration then maps implicitly. Returns the lens and its target's
    /// `(anchor, both)`.
    fn step(
        anchor: &'static str,
        both: bool,
        kind: u8,
        explicit: bool,
    ) -> (Lens, &'static str, bool) {
        let src = leaf_schema(anchor, both);
        let renamed = if anchor == "leaf" { "item" } else { "leaf" };
        let labels: &[&str] = if both {
            &["label", "other"]
        } else {
            &["label"]
        };
        let (to, keep_both, edges): (&'static str, bool, Vec<(Edge, Edge)>) = match kind {
            1 => (
                renamed,
                both,
                labels
                    .iter()
                    .map(|l| (prop("r", anchor, l), prop("r", renamed, l)))
                    .collect(),
            ),
            2 if both => (
                anchor,
                true,
                vec![
                    (prop("r", anchor, "label"), prop("r", anchor, "other")),
                    (prop("r", anchor, "other"), prop("r", anchor, "label")),
                ],
            ),
            3 if both => (
                anchor,
                false,
                vec![(prop("r", anchor, "label"), prop("r", anchor, "label"))],
            ),
            _ => (
                anchor,
                both,
                labels
                    .iter()
                    .map(|l| (prop("r", anchor, l), prop("r", anchor, l)))
                    .collect(),
            ),
        };
        let edges = edges
            .into_iter()
            .filter(|(s, t)| explicit || s.name != t.name)
            .collect();
        let m = migration(&[("r", "r"), (anchor, to)], edges, vec![]);
        (lens(src, leaf_schema(to, keep_both), &m), to, keep_both)
    }

    /// `r(0)` with one leaf per value, alternating the `label` and `other`
    /// edges.
    fn two_label_source(values: &[String]) -> WInstance {
        let mut nodes = HashMap::from([(0, Node::new(0, "r"))]);
        let mut arcs = Vec::new();
        for (i, value) in values.iter().enumerate() {
            let id = u32::try_from(i).unwrap() + 1;
            nodes.insert(id, leaf(id, value));
            let name = if i % 2 == 0 { "label" } else { "other" };
            arcs.push((0, id, prop("r", "leaf", name)));
        }
        WInstance::new(nodes, arcs, vec![], 0, "r".into())
    }

    /// A first lens that maps its edges implicitly (an empty edge map)
    /// followed by one that swaps the two labels: the fused lens must swap
    /// them too, as the pair in sequence does.
    #[test]
    fn a_fused_composite_keeps_the_second_lens_edge_renames() {
        let (first, mid, both) = step("leaf", true, 0, false);
        let (second, _, _) = step(mid, both, 2, true);
        let source = two_label_source(&["a".into(), "b".into()]);
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&source).unwrap();
        let fused = panproto_lens::compose(&sequential.first, &sequential.second).unwrap();
        let (view, _) = panproto_lens::get(&fused, &source).unwrap();
        assert_eq!(view_difference(&view, &seq_view), None);
        assert!(matches!(
            panproto_lens::compose_tree(&sequential.first, &sequential.second).unwrap(),
            TreeComposite::Compiled(_)
        ));
    }

    /// A first lens that renames the `label` edge to `other` followed by one
    /// that drops `other`: the target still has a `label` edge, which the
    /// fused lens would carry the arc to, so the pair is not fused.
    #[test]
    fn a_pair_dropping_a_renamed_edge_is_kept_in_sequence() {
        let (first, mid, both) = step("leaf", true, 2, true);
        let (second, _, _) = step(mid, both, 3, true);
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        assert!(
            matches!(composite, TreeComposite::Sequential(_)),
            "{composite:?}"
        );
        // The fused lens would keep the `label` arc the pair drops.
        let source = two_label_source(&["a".into(), "b".into()]);
        let fused = panproto_lens::compose(&first, &second).unwrap();
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&source).unwrap();
        let (view, _) = panproto_lens::get(&fused, &source).unwrap();
        assert_eq!(
            seq_view.arcs.len(),
            1,
            "the pair keeps only the `other` arc"
        );
        assert!(view_difference(&view, &seq_view).is_some());
    }

    proptest! {
        /// Over generated pairs of contraction-free steps (identities,
        /// renames, label swaps and edge drops), the rule fuses exactly the
        /// pairs one migration can state, and the fused lens is
        /// observationally the pair in sequence: the same
        /// view, the same source written back for the view and an edit of
        /// it, and the same view again after that write.
        #[test]
        fn generated_contraction_free_pairs_fuse_and_agree(
            k1 in 0u8..4,
            k2 in 0u8..4,
            e1 in any::<bool>(),
            e2 in any::<bool>(),
            values in prop::collection::vec("[a-z]{0,3}", 0..6),
        ) {
            let (first, mid, both) = step("leaf", true, k1, e1);
            let (second, _, _) = step(mid, both, k2, e2);
            let composite = panproto_lens::compose_tree(&first, &second).unwrap();
            // A swap followed by a drop of `other` drops the arcs the source
            // holds under `label`, which the target could still carry as
            // `label`: no single migration states that, so it stays in
            // sequence. Every other pair is fused.
            let unfusable = k1 == 2 && k2 == 3;
            let fused = match composite {
                TreeComposite::Compiled(fused) if !unfusable => fused,
                TreeComposite::Sequential(_) if unfusable => return Ok(()),
                other => {
                    return Err(TestCaseError::fail(format!(
                        "steps {k1} then {k2} composed to {other:?}"
                    )));
                }
            };
            let source = two_label_source(&values);
            let sequential = SequentialTreeLens { first, second };
            agree(&fused, &sequential, &source)?;

            let (view, c) = panproto_lens::get(&fused, &source).unwrap();
            let (_, seq_c) = sequential.get(&source).unwrap();
            let edit = edited(&view);
            let written = panproto_lens::put(&fused, &edit, &c).unwrap();
            let seq_written = sequential.put(&edit, &seq_c).unwrap();
            let (again, _) = panproto_lens::get(&fused, &written).unwrap();
            let (seq_again, _) = sequential.get(&seq_written).unwrap();
            if let Some(difference) = view_difference(&again, &seq_again) {
                return Err(TestCaseError::fail(format!("views after a put differ: {difference}")));
            }
            if let Some(difference) = view_difference(&again, &edit) {
                return Err(TestCaseError::fail(format!("PutGet through the fused lens: {difference}")));
            }
        }
    }

    proptest! {
        /// Over pairs around an expansion (a first lens that expands followed
        /// by one that keeps, drops or renames the expansion's endpoints and
        /// intermediate, and an identity followed by an expansion) and
        /// contracting pairs, the lens `compose_tree` fuses is the lens
        /// `compose` returns, and it is observationally the pair in
        /// sequence. `compose` fuses a contracting pair into a lens that also
        /// behaves as the pair, though the rule keeps such pairs in sequence.
        #[test]
        fn tree_compiled_composite_matches_sequential(
            pick in 0u8..10,
            values in prop::collection::vec("[a-z]{0,3}", 0..4),
            counts in prop::collection::vec(prop::collection::vec("[a-z]{0,3}", 0..3), 0..4),
        ) {
            let (first, second, source, contracting) = if pick < 8 {
                let (first, second) = expansion_pair(pick, pick < 7);
                (first, second, direct_source(&values), false)
            } else {
                let (first, second) = contracting_pairs().swap_remove(usize::from(pick - 8));
                (first, second, source(&counts), true)
            };
            let fused = panproto_lens::compose(&first, &second).unwrap();
            let composite = panproto_lens::compose_tree(&first, &second).unwrap();
            let sequential = SequentialTreeLens { first, second };
            match composite {
                TreeComposite::Compiled(by_rule) => {
                    let (a, _) = panproto_lens::get(&by_rule, &source).unwrap();
                    let (b, _) = panproto_lens::get(&fused, &source).unwrap();
                    prop_assert_eq!(view_difference(&a, &b), None);
                    agree(&by_rule, &sequential, &source)?;
                }
                TreeComposite::Sequential(_) if contracting => {
                    agree(&fused, &sequential, &source)?;
                }
                TreeComposite::Sequential(_) => {}
            }
        }

        /// Whenever the rule fuses a pair, the fused lens is observationally
        /// the pair in sequence, and a pair kept in sequence runs its two
        /// stages. The pairs are drawn from six families: around an
        /// expansion (expanding lenses, and second lenses that drop or
        /// rename an expansion's endpoints and intermediate), contracting
        /// pairs, contraction-free renames, swaps and edge drops, value
        /// layers on both lenses, fan retargets on both lenses (chained, and
        /// chosen by the children that survive), and a first lens that
        /// merges a renamed vertex onto one that keeps its name, with value
        /// layers on the merged anchor.
        #[test]
        fn compiled_and_sequential_tree_composites_agree(
            family in 0u8..6,
            k1 in 0u8..7,
            k2 in 0u8..4,
            e1 in any::<bool>(),
            e2 in any::<bool>(),
            values in prop::collection::vec("[a-z]{0,3}", 0..5),
            counts in prop::collection::vec(prop::collection::vec("[a-z]{0,3}", 0..3), 0..4),
            levels in prop::collection::vec(0i64..5, 0..5),
        ) {
            let (first, second, instance) = match family {
                0 => {
                    let (first, second) = expansion_pair(k1, e1);
                    (first, second, direct_source(&values))
                }
                1 => {
                    let (first, second) = contracting_pairs().swap_remove(usize::from(e1));
                    (first, second, source(&counts))
                }
                2 => {
                    let (first, mid, both) = step("leaf", true, k2, e1);
                    let (second, _, _) = step(mid, both, k1 % 4, e2);
                    (first, second, two_label_source(&values))
                }
                3 => (valued_identity(k1 % 4), valued_identity(k2), levelled_source(&levels)),
                4 => (
                    fan_lens([0, 1, 3][usize::from(k1 % 3)]),
                    fan_lens([0, 2, 4][usize::from(k2 % 3)]),
                    fan_source(e1),
                ),
                _ => {
                    let (first, second) = merge_pair([0, 2, 3][usize::from(k1 % 3)], k2);
                    let level = |i: usize| levels.get(i).copied().unwrap_or(3);
                    (first, second, merge_source((level(0), level(1))))
                }
            };
            match panproto_lens::compose_tree(&first, &second).unwrap() {
                TreeComposite::Compiled(fused) => {
                    agree(&fused, &SequentialTreeLens { first, second }, &instance)?;
                }
                TreeComposite::Sequential(sequential) => {
                    let (view, _) = sequential.get(&instance).unwrap();
                    let (middle, _) = panproto_lens::get(&first, &instance).unwrap();
                    let (expected, _) = panproto_lens::get(&second, &middle).unwrap();
                    prop_assert_eq!(view_difference(&view, &expected), None);
                }
            }
        }
    }

    /// Value layers for the value-composition tests, on `anchor`: `0`
    /// none, `1` the invertible transform `level := level + 1`, `2` the
    /// survival predicate `level == 3`, `3` the survival predicate
    /// `missing == 1`, which fails to evaluate on every node and so keeps
    /// it.
    fn value_layer(lens: &mut Lens, anchor: &str, layer: u8) {
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use panproto_inst::FieldTransform;
        let var = |name: &str| Expr::Var(std::sync::Arc::from(name));
        let int = |n: i64| Expr::Lit(Literal::Int(n));
        match layer {
            1 => {
                lens.compiled.field_transforms.insert(
                    anchor.into(),
                    vec![FieldTransform::ApplyExpr {
                        key: "level".into(),
                        expr: Expr::Builtin(BuiltinOp::Add, vec![var("level"), int(1)]),
                        inverse: Some(Expr::Builtin(BuiltinOp::Sub, vec![var("level"), int(1)])),
                        coercion_class: panproto_gat::CoercionClass::Iso,
                    }],
                );
            }
            2 => {
                lens.compiled.conditional_survival.insert(
                    anchor.into(),
                    Expr::Builtin(BuiltinOp::Eq, vec![var("level"), int(3)]),
                );
            }
            3 => {
                lens.compiled.conditional_survival.insert(
                    anchor.into(),
                    Expr::Builtin(BuiltinOp::Eq, vec![var("missing"), int(1)]),
                );
            }
            _ => {}
        }
    }

    /// The identity lens on `r -> c` with the value layer `layer`.
    fn valued_identity(layer: u8) -> Lens {
        let src = schema(&[("r", "object"), ("c", "string")], &[prop("r", "c", "c")]);
        let mut lens = identity(&src);
        value_layer(&mut lens, "c", layer);
        lens
    }

    /// `r(0)` with one `c` child per level, each holding the value `v<id>`
    /// and the field `level`.
    fn levelled_source(levels: &[i64]) -> WInstance {
        let mut nodes = HashMap::from([(0, Node::new(0, "r"))]);
        let mut arcs = Vec::new();
        for (i, level) in levels.iter().enumerate() {
            let id = u32::try_from(i).unwrap() + 1;
            let mut c =
                Node::new(id, "c").with_value(FieldPresence::Present(Value::Str(format!("v{id}"))));
            c.extra_fields.insert("level".into(), Value::Int(*level));
            nodes.insert(id, c);
            arcs.push((0, id, prop("r", "c", "c")));
        }
        WInstance::new(nodes, arcs, vec![], 0, "r".into())
    }

    /// A first lens that adds one to `level` followed by one that keeps
    /// only the nodes whose `level` is 3: the pair keeps a node of level 2,
    /// which one migration would test before adding one and so drop. The
    /// pair is kept in sequence.
    #[test]
    fn a_second_predicate_after_a_first_transform_is_kept_in_sequence() {
        let (first, second) = (valued_identity(1), valued_identity(2));
        let source = levelled_source(&[2]);
        let fused = panproto_lens::compose(&first, &second).unwrap();
        let (view, _) = panproto_lens::get(&fused, &source).unwrap();
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&source).unwrap();
        assert!(
            seq_view.nodes.contains_key(&1),
            "the pair keeps level 2 + 1"
        );
        assert!(!view.nodes.contains_key(&1), "one migration tests level 2");
        assert!(
            matches!(composite, TreeComposite::Sequential(_)),
            "{composite:?}"
        );
    }

    /// A predicate of each lens on one anchor: the first fails to evaluate,
    /// which keeps the node, and the second is false, which drops it. One
    /// migration conjoins them, and the failed conjunct keeps the node.
    #[test]
    fn predicates_of_both_lenses_on_one_anchor_are_kept_in_sequence() {
        let (first, second) = (valued_identity(3), valued_identity(2));
        let source = levelled_source(&[2]);
        let fused = panproto_lens::compose(&first, &second).unwrap();
        let (view, _) = panproto_lens::get(&fused, &source).unwrap();
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&source).unwrap();
        assert!(!seq_view.nodes.contains_key(&1), "the second lens drops it");
        assert!(view.nodes.contains_key(&1), "the conjunction keeps it");
        assert!(
            matches!(composite, TreeComposite::Sequential(_)),
            "{composite:?}"
        );
    }

    proptest! {
        /// Over pairs of identity lenses on `r -> c` carrying every
        /// combination of no value layer, an invertible transform and two
        /// survival predicates, the rule fuses exactly the pairs whose value
        /// layers do not meet, and every fused lens is observationally the
        /// pair in sequence.
        #[test]
        fn value_layers_fuse_only_where_they_do_not_meet(
            a in 0u8..4,
            b in 0u8..4,
            levels in prop::collection::vec(0i64..5, 0..5),
        ) {
            let (first, second) = (valued_identity(a), valued_identity(b));
            let composite = panproto_lens::compose_tree(&first, &second).unwrap();
            let fusable = b == 0 || (a == 0 && b != 1);
            let fused = match composite {
                TreeComposite::Compiled(fused) if fusable => fused,
                TreeComposite::Sequential(_) if !fusable => return Ok(()),
                other => {
                    return Err(TestCaseError::fail(format!(
                        "layers {a} then {b} composed to {other:?}"
                    )));
                }
            };
            let sequential = SequentialTreeLens { first, second };
            agree(&fused, &sequential, &levelled_source(&levels))?;
        }
    }

    /// `r -> a` under `x` and `r -> b` under `y`, or, `merged`, both edges
    /// into `a`.
    fn merge_schema(merged: bool) -> Schema {
        if merged {
            schema(
                &[("r", "object"), ("a", "string")],
                &[prop("r", "a", "x"), prop("r", "a", "y")],
            )
        } else {
            schema(
                &[("r", "object"), ("a", "string"), ("b", "string")],
                &[prop("r", "a", "x"), prop("r", "b", "y")],
            )
        }
    }

    /// A first lens that merges `b` onto `a`, which keeps its own name (so
    /// only `b` is in its vertex remap), with the value layer `first_layer`
    /// on its source anchor `a`, and the identity on the merged schema with
    /// the value layer `second_layer` on `a`.
    fn merge_pair(first_layer: u8, second_layer: u8) -> (Lens, Lens) {
        let m = migration(
            &[("r", "r"), ("a", "a"), ("b", "a")],
            vec![
                (prop("r", "a", "x"), prop("r", "a", "x")),
                (prop("r", "b", "y"), prop("r", "a", "y")),
            ],
            vec![],
        );
        let mut first = lens(merge_schema(false), merge_schema(true), &m);
        // The remap of identity lenses and protolens instantiations: a
        // vertex that keeps its name has no entry.
        first
            .compiled
            .vertex_remap
            .retain(|source, image| source != image);
        assert_eq!(first.compiled.vertex_remap.len(), 1);
        value_layer(&mut first, "a", first_layer);
        let mut second = identity(&merge_schema(true));
        value_layer(&mut second, "a", second_layer);
        (first, second)
    }

    /// `r(0)` with `a(1)` and `b(2)`, whose fields `level` are `levels`.
    fn merge_source(levels: (i64, i64)) -> WInstance {
        let node = |id: u32, anchor: &str, level: i64| {
            let mut node = Node::new(id, anchor)
                .with_value(FieldPresence::Present(Value::Str(format!("v{id}"))));
            node.extra_fields.insert("level".into(), Value::Int(level));
            node
        };
        WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, node(1, "a", levels.0)),
                (2, node(2, "b", levels.1)),
            ]),
            vec![(0, 1, prop("r", "a", "x")), (0, 2, prop("r", "b", "y"))],
            vec![],
            0,
            "r".into(),
        )
    }

    /// A first lens that merges `b` onto `a`, which keeps its name,
    /// followed by a survival predicate on `a`: the pair tests the
    /// predicate on the nodes of both source anchors, and so does the fused
    /// lens.
    #[test]
    fn a_predicate_on_a_merged_anchor_reaches_every_preimage() {
        let (first, second) = merge_pair(0, 2);
        let fused = panproto_lens::compose(&first, &second).unwrap();
        for anchor in ["a", "b"] {
            assert!(
                fused.compiled.conditional_survival.contains_key(anchor),
                "no predicate on {anchor}: {:?}",
                fused.compiled.conditional_survival
            );
        }
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        let TreeComposite::Compiled(by_rule) = composite else {
            panic!("the pair meets every condition: {composite:?}");
        };
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&merge_source((2, 2))).unwrap();
        assert_eq!(
            seq_view.nodes.keys().copied().collect::<Vec<_>>(),
            vec![0],
            "the pair drops both nodes of level 2"
        );
        for levels in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            agree(&by_rule, &sequential, &merge_source(levels)).unwrap();
        }
    }

    /// Hyper-edges over `par`'s children `left` and `right`: `h0` labels
    /// them `left` and `right`, `h1` `a` and `b`, `h2` `p` and `q`, and `hA`
    /// labels `left` alone `a`. Without `right`, each spans `left` alone.
    fn fan_schema(with_right: bool) -> Schema {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol)
            .vertex("par", "object", None)
            .unwrap()
            .vertex("left", "string", None)
            .unwrap()
            .edge("par", "left", "prop", Some("left"))
            .unwrap();
        if with_right {
            builder = builder
                .vertex("right", "string", None)
                .unwrap()
                .edge("par", "right", "prop", Some("right"))
                .unwrap();
        }
        let signature = |labels: &[(&str, &str)]| {
            labels
                .iter()
                .filter(|(_, vertex)| with_right || *vertex != "right")
                .chain(std::iter::once(&("par", "par")))
                .map(|(label, vertex)| ((*label).to_owned(), (*vertex).to_owned()))
                .collect::<HashMap<String, String>>()
        };
        for (id, labels) in [
            ("h0", vec![("left", "left"), ("right", "right")]),
            ("h1", vec![("a", "left"), ("b", "right")]),
            ("h2", vec![("p", "left"), ("q", "right")]),
            ("hA", vec![("a", "left")]),
        ] {
            builder = builder
                .hyper_edge(id, "rel", signature(&labels), "par")
                .unwrap();
        }
        builder.build().unwrap()
    }

    /// A lens on the fan schema: `0` the identity, `1` retargets `h0`'s fans
    /// to `h1` (`left` to `a`, `right` to `b`), `2` retargets `h1`'s fans to
    /// `h2` (`a` to `p`, `b` to `q`), `3` retargets an `h0` fan to `hA` when
    /// only `left` survives and to `h1` otherwise, and `4` drops `right`.
    fn fan_lens(kind: u8) -> Lens {
        let mut fan_lens = if kind == 4 {
            let m = migration(
                &[("par", "par"), ("left", "left")],
                vec![(prop("par", "left", "left"), prop("par", "left", "left"))],
                vec![],
            );
            lens(fan_schema(true), fan_schema(false), &m)
        } else {
            identity(&fan_schema(true))
        };
        let entry = |he: &str, shape: &[&str], target: &str, labels: &[(&str, &str)]| {
            (
                (
                    Name::from(he),
                    shape.iter().map(|l| Name::from(*l)).collect::<Vec<_>>(),
                ),
                (
                    Name::from(target),
                    labels
                        .iter()
                        .map(|(a, b)| (Name::from(*a), Name::from(*b)))
                        .collect::<HashMap<_, _>>(),
                ),
            )
        };
        let entries = match kind {
            1 => vec![entry(
                "h0",
                &["left", "right"],
                "h1",
                &[("left", "a"), ("right", "b")],
            )],
            2 => vec![entry("h1", &["a", "b"], "h2", &[("a", "p"), ("b", "q")])],
            3 => vec![
                entry("h0", &["left"], "hA", &[("left", "a")]),
                entry(
                    "h0",
                    &["left", "right"],
                    "h1",
                    &[("left", "a"), ("right", "b")],
                ),
            ],
            _ => vec![],
        };
        fan_lens.compiled.hyper_resolver.extend(entries);
        fan_lens
    }

    /// `par(0)` with `left(1)` and `right(2)`, joined by an `h0` fan when
    /// `fan` is set.
    fn fan_source(fan: bool) -> WInstance {
        let fans = if fan {
            vec![
                panproto_inst::Fan::new("h0", 0)
                    .with_child("left", 1)
                    .with_child("right", 2),
            ]
        } else {
            vec![]
        };
        WInstance::new(
            HashMap::from([
                (0, Node::new(0, "par")),
                (
                    1,
                    Node::new(1, "left").with_value(FieldPresence::Present(Value::Str("l".into()))),
                ),
                (
                    2,
                    Node::new(2, "right")
                        .with_value(FieldPresence::Present(Value::Str("r".into()))),
                ),
            ]),
            vec![
                (0, 1, prop("par", "left", "left")),
                (0, 2, prop("par", "right", "right")),
            ],
            fans,
            0,
            "par".into(),
        )
    }

    /// Pairs whose fan layers retarget are kept in sequence. A fan's
    /// resolver entry is chosen by the children that survive, which the pair
    /// decides in two stages and one migration in one, so no single
    /// resolver table stands for the pair. `compose` chains the retargets
    /// of the first lens through the second, which gives the pair's fans
    /// when both stages see the same children, and not otherwise.
    #[test]
    fn pairs_whose_fan_layers_retarget_are_kept_in_sequence() {
        let source = fan_source(true);
        let fans = |view: &WInstance| {
            view.fans
                .iter()
                .map(|fan| {
                    let mut children: Vec<(String, u32)> =
                        fan.children.iter().map(|(l, n)| (l.clone(), *n)).collect();
                    children.sort();
                    (fan.hyper_edge_id.clone(), children)
                })
                .collect::<Vec<_>>()
        };

        // `h0` to `h1`, then `h1` to `h2`.
        let (first, second) = (fan_lens(1), fan_lens(2));
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        assert!(
            matches!(composite, TreeComposite::Sequential(_)),
            "{composite:?}"
        );
        let fused = panproto_lens::compose(&first, &second).unwrap();
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&source).unwrap();
        let (view, _) = panproto_lens::get(&fused, &source).unwrap();
        assert_eq!(
            fans(&seq_view),
            vec![(
                "h2".to_owned(),
                vec![("p".to_owned(), 1), ("q".to_owned(), 2)]
            )]
        );
        assert_eq!(fans(&view), fans(&seq_view), "compose chains the retargets");

        // An `h0` fan goes to `h1` while both children survive the first
        // stage, and the second drops `right`; one migration sees only
        // `left` survive and picks `hA`.
        let (first, second) = (fan_lens(3), fan_lens(4));
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        assert!(
            matches!(composite, TreeComposite::Sequential(_)),
            "{composite:?}"
        );
        let fused = panproto_lens::compose(&first, &second).unwrap();
        let sequential = SequentialTreeLens { first, second };
        let (seq_view, _) = sequential.get(&source).unwrap();
        let (view, _) = panproto_lens::get(&fused, &source).unwrap();
        assert_eq!(
            fans(&seq_view),
            vec![("h1".to_owned(), vec![("a".to_owned(), 1)])]
        );
        assert_eq!(
            fans(&view),
            vec![("hA".to_owned(), vec![("a".to_owned(), 1)])]
        );

        // Fans that neither lens retargets: the pair is fused.
        let (first, second) = (fan_lens(0), fan_lens(4));
        let composite = panproto_lens::compose_tree(&first, &second).unwrap();
        let TreeComposite::Compiled(fused) = composite else {
            panic!("no fan is retargeted: {composite:?}");
        };
        agree(&fused, &SequentialTreeLens { first, second }, &source).unwrap();
    }

    /// Every pair of fan lenses and every merge pair, on every source of
    /// their families: the rule fuses exactly the pairs in which no fan is
    /// retargeted and the value layers do not meet, every fused lens is
    /// observationally the pair in sequence, and every pair kept in
    /// sequence runs its two stages.
    #[test]
    fn every_fan_and_merge_pair_follows_the_rule() {
        let check =
            |first: Lens, second: Lens, fusable: bool, sources: &[WInstance], case: &str| {
                match panproto_lens::compose_tree(&first, &second).unwrap() {
                    TreeComposite::Compiled(fused) => {
                        assert!(fusable, "{case} was fused");
                        let sequential = SequentialTreeLens { first, second };
                        for source in sources {
                            agree(&fused, &sequential, source)
                                .unwrap_or_else(|e| panic!("{case}: {e}"));
                        }
                    }
                    TreeComposite::Sequential(sequential) => {
                        assert!(!fusable, "{case} was kept in sequence");
                        for source in sources {
                            let (view, _) = sequential.get(source).unwrap();
                            let (middle, _) = panproto_lens::get(&first, source).unwrap();
                            let (expected, _) = panproto_lens::get(&second, &middle).unwrap();
                            assert_eq!(view_difference(&view, &expected), None, "{case}");
                        }
                    }
                }
            };
        let fan_sources = [fan_source(true), fan_source(false)];
        for a in [0, 1, 3] {
            for b in [0, 2, 4] {
                let fusable = a == 0 && b != 2;
                check(
                    fan_lens(a),
                    fan_lens(b),
                    fusable,
                    &fan_sources,
                    &format!("fans {a} then {b}"),
                );
            }
        }
        let merge_sources: Vec<WInstance> = [(2, 2), (3, 2), (2, 3), (3, 3)]
            .into_iter()
            .map(merge_source)
            .collect();
        for a in [0, 2, 3] {
            for b in 0..4 {
                let (first, second) = merge_pair(a, b);
                let fusable = b == 0 || (a == 0 && b != 1);
                check(
                    first,
                    second,
                    fusable,
                    &merge_sources,
                    &format!("merge {a} then {b}"),
                );
            }
        }
    }

    /// One case per condition of the composition rule, each kept in
    /// sequence, and a pair meeting every condition, which is fused:
    /// (i) a contracting lens on either side; (ii) migrations that do not
    /// compose; (iii) an edge the pair drops that the composite would carry;
    /// (iv) a non-empty plan in the first lens, the second, or the
    /// composite, including an expansion the second lens truncates; (v)
    /// value layers that meet; (vi) a fan retarget in either lens.
    #[test]
    fn tree_compose_follows_the_l6_rule() {
        let kept = |first: &Lens, second: &Lens, case: &str| {
            let composite = panproto_lens::compose_tree(first, second).unwrap();
            assert!(
                matches!(composite, TreeComposite::Sequential(_)),
                "{case}: {composite:?}"
            );
        };

        // (i)
        for (first, second) in contracting_pairs() {
            kept(&first, &second, "a contracting pair");
        }

        // (ii): the first lens drops `level`, which the second one reads.
        let mut first = valued_identity(0);
        first.compiled.field_transforms.insert(
            "c".into(),
            vec![panproto_inst::FieldTransform::DropField {
                key: "level".into(),
            }],
        );
        let second = valued_identity(1);
        assert!(matches!(
            panproto_lens::compose(&first, &second),
            Err(panproto_lens::LensError::ComposeUnboundField { .. })
        ));
        kept(&first, &second, "migrations that do not compose");

        // (iii)
        let (first, mid, both) = step("leaf", true, 2, true);
        let (second, _, _) = step(mid, both, 3, true);
        kept(&first, &second, "an edge the composite would carry");

        // (iv): the first lens's plan alone (the composite's is empty), the
        // composite's (and the first lens's), and the second lens's.
        for kind in [1, 5, 6] {
            let (first, second) = expansion_pair(kind, true);
            let fused = panproto_lens::compose(&first, &second).unwrap();
            assert!(fused.compiled.expansion_path.is_empty(), "kind {kind}");
            kept(&first, &second, "an expansion the second lens truncates");
        }
        for kind in [0, 3, 4] {
            let (first, second) = expansion_pair(kind, true);
            kept(&first, &second, "an expansion the second lens keeps");
        }
        let (first, second) = expansion_pair(0, false);
        kept(&first, &second, "an expanding second lens");

        // (v)
        kept(
            &valued_identity(1),
            &valued_identity(2),
            "value layers that meet",
        );

        // (vi)
        kept(&fan_lens(1), &fan_lens(2), "fan retargets that chain");
        kept(
            &fan_lens(3),
            &fan_lens(0),
            "a first lens that retargets a fan",
        );
        kept(
            &fan_lens(0),
            &fan_lens(2),
            "a second lens that retargets a fan",
        );

        // Every condition met.
        let first = identity(&renaming_lens().src_schema);
        let composite = panproto_lens::compose_tree(&first, &renaming_lens()).unwrap();
        assert!(
            matches!(composite, TreeComposite::Compiled(_)),
            "{composite:?}"
        );
    }
}
