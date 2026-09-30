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
