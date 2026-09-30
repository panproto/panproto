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

/// What a tree complement counts as residue: the information in the source
/// that the view lacks, by the same criteria as the graph complement.
mod residue {
    use super::*;

    /// A complement recording only contraction bookkeeping (which pair
    /// contracted to which label, and which dropped node collapsed into
    /// which ancestor) has trivial residue: both are derivable from the
    /// view, the dropped nodes, the parent map and the resolver.
    #[test]
    fn residue_classification_agrees_across_shapes_on_contraction_bookkeeping() {
        let mut complement = panproto_lens::Complement::empty();
        complement
            .contraction_choices
            .insert((0, 2), prop("r", "leaf", "label"));
        complement.contracted_into.insert(1, 0);
        assert!(complement.residue_is_trivial());
    }

    /// An injective vertex rename loses nothing: the residue is trivial and
    /// the source is rebuilt from the view alone.
    #[test]
    fn residue_classification_agrees_across_shapes_on_an_injective_rename() {
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
        let back = panproto_lens::put_without_complement(&lens, &view)
            .expect("an injective rename is an isomorphism");
        assert_eq!(sorted_arcs(&back), sorted_arcs(&source));
        assert_eq!(back.nodes[&1].anchor, Name::from("a"));
    }

    /// Two parallel source edges merged onto one target edge: the view
    /// cannot say which one an arc came from, so that is residue and the
    /// source cannot be rebuilt from the view alone.
    #[test]
    fn residue_classification_agrees_across_shapes_on_merged_parallel_edges() {
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
        assert!(panproto_lens::put_without_complement(&lens, &view).is_err());
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

    fn arb_source() -> impl Strategy<Value = WInstance> {
        prop::collection::vec(prop::collection::vec("[a-z]{0,3}", 0..3), 0..4)
            .prop_map(|counts| source(&counts))
    }

    /// A tree as data: nodes with anchors and values, and its arcs.
    fn shape(tree: &WInstance) -> Shape {
        let mut nodes: Vec<_> = tree
            .nodes
            .values()
            .map(|n| (n.id, n.anchor.clone(), n.value.clone()))
            .collect();
        nodes.sort_by_key(|n| n.0);
        (nodes, sorted_arcs(tree))
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

    /// The fused lens and the sequential pair agree on `source`: the same
    /// view, and the same source written back for the view and an edit of
    /// it.
    fn agree(
        fused: &Lens,
        sequential: &SequentialTreeLens,
        source: &WInstance,
    ) -> Result<(), TestCaseError> {
        let (seq_view, seq_c) = sequential.get(source).unwrap();
        let (view, c) = panproto_lens::get(fused, source)
            .map_err(|e| TestCaseError::fail(format!("the fused get failed: {e}")))?;
        prop_assert_eq!(shape(&view), shape(&seq_view), "views differ");
        for v in [seq_view.clone(), edited(&seq_view)] {
            let back = sequential.put(&v, &seq_c).unwrap();
            let fused_back = panproto_lens::put(fused, &v, &c)
                .map_err(|e| TestCaseError::fail(format!("the fused put failed: {e}")))?;
            prop_assert_eq!(shape(&fused_back), shape(&back), "puts differ");
        }
        Ok(())
    }

    /// Contracting pairs are kept in sequence; a pair of renames with no
    /// dropped vertex is fused.
    #[test]
    fn tree_compose_follows_the_l6_rule() {
        for (first, second) in contracting_pairs() {
            let composite = panproto_lens::compose_tree(&first, &second).unwrap();
            assert!(
                matches!(composite, TreeComposite::Sequential(_)),
                "{composite:?}"
            );
        }
        let first = identity(&renaming_lens().src_schema);
        let composite = panproto_lens::compose_tree(&first, &renaming_lens()).unwrap();
        assert!(
            matches!(composite, TreeComposite::Compiled(_)),
            "{composite:?}"
        );
    }

    proptest! {
        /// The fused composite of a contracting pair behaves as the pair in
        /// sequence, on the sources the pair's schema admits.
        #[test]
        fn tree_compiled_composite_matches_sequential(source in arb_source(), pick in 0usize..2) {
            let (first, second) = contracting_pairs().swap_remove(pick);
            let fused = panproto_lens::compose(&first, &second).unwrap();
            agree(&fused, &SequentialTreeLens { first, second }, &source)?;
        }

        /// Whenever the rule fuses a pair, the fused lens is observationally
        /// the pair in sequence.
        #[test]
        fn compiled_and_sequential_tree_composites_agree(values in prop::collection::vec("[a-z]{0,3}", 0..5)) {
            let first = identity(&renaming_lens().src_schema);
            let second = renaming_lens();
            let TreeComposite::Compiled(fused) = panproto_lens::compose_tree(&first, &second).unwrap() else {
                return Err(TestCaseError::fail("a rename pair is fused"));
            };
            let mut nodes = HashMap::from([(0, Node::new(0, "r"))]);
            let mut arcs = Vec::new();
            for (i, value) in values.iter().enumerate() {
                let id = u32::try_from(i).unwrap() + 1;
                nodes.insert(id, leaf(id, value));
                let name = if i % 2 == 0 { "label" } else { "other" };
                arcs.push((0, id, prop("r", "leaf", name)));
            }
            let source = WInstance::new(nodes, arcs, vec![], 0, "r".into());
            agree(&fused, &SequentialTreeLens { first, second }, &source)?;
        }
    }
}
