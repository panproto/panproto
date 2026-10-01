//! The tree `put` restores what `get` set aside, and refuses a view it
//! cannot restore from rather than guessing.
//!
//! Three ways the backward direction used to guess:
//!
//! 1. A node added in the view under the id of a node the complement holds
//!    was silently overwritten by the complement's node.
//! 2. When the vertex remap sends two source vertices to one target vertex,
//!    the anchor each node came back with was whichever inverse the remap's
//!    hash order produced, so one of the two kinds of node came back
//!    re-anchored.
//! 3. A node added in the view under such a merged anchor was given one of
//!    the candidates arbitrarily.
//!
//! And one way a law checker could not fail: `check_put_put` compared a
//! put with itself. It now puts a genuinely different first view, and the
//! last test here is a lens it catches.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{Node, ViewRef, WInstance};
use panproto_lens::{
    LawViolation, Lens, LensError, PutDomainError, check_get_put, check_put_put, get, put,
};
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
        builder = builder.vertex(id, kind, None).expect("vertex");
    }
    for e in edges {
        builder = builder
            .edge(&e.src, &e.tgt, &e.kind, e.name.as_deref())
            .expect("edge");
    }
    builder.build().expect("schema")
}

fn lens(
    src: Schema,
    tgt: Schema,
    vertex_map: &[(&str, &str)],
    edge_map: Vec<(Edge, Edge)>,
) -> Lens {
    let migration = Migration {
        vertex_map: vertex_map
            .iter()
            .map(|(s, t)| (Name::from(*s), Name::from(*t)))
            .collect(),
        edge_map: edge_map.into_iter().collect(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
        coercions: HashMap::new(),
        domain: None,
        codomain: None,
    };
    let compiled = panproto_mig::compile(&src, &tgt, &migration).expect("compile");
    Lens {
        compiled,
        src_schema: src,
        tgt_schema: tgt,
    }
}

fn leaf(id: u32, anchor: &str, s: &str) -> Node {
    Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(s.into())))
}

fn instance(nodes: Vec<Node>, arcs: Vec<(u32, u32, Edge)>) -> WInstance {
    let root_anchor = nodes[0].anchor.clone();
    WInstance::new(
        nodes.into_iter().map(|n| (n.id, n)).collect(),
        arcs,
        Vec::new(),
        0,
        root_anchor,
    )
}

/// `post` with a kept `text` and a dropped `subtitle`.
fn drop_subtitle() -> (Lens, WInstance) {
    let text = prop("post", "post.text", "text");
    let subtitle = prop("post", "post.subtitle", "subtitle");
    let src = schema(
        &[
            ("post", "object"),
            ("post.text", "string"),
            ("post.subtitle", "string"),
        ],
        &[text.clone(), subtitle.clone()],
    );
    let tgt = schema(
        &[("post", "object"), ("post.text", "string")],
        std::slice::from_ref(&text),
    );
    let lens = lens(
        src,
        tgt,
        &[("post", "post"), ("post.text", "post.text")],
        vec![(text.clone(), text.clone())],
    );
    let source = instance(
        vec![
            Node::new(0, "post"),
            leaf(1, "post.text", "hello"),
            leaf(2, "post.subtitle", "world"),
        ],
        vec![(0, 1, text), (0, 2, subtitle)],
    );
    (lens, source)
}

#[test]
fn a_view_node_under_a_dropped_nodes_id_is_a_collision_not_an_overwrite() {
    let (lens, source) = drop_subtitle();
    let (mut view, complement) = get(&lens, &source).expect("get");
    assert!(complement.dropped_nodes.contains_key(&2));

    // A second text node added in the view, under the id the dropped
    // subtitle still holds.
    view.nodes.insert(2, leaf(2, "post.text", "added"));
    view.arcs.push((0, 2, prop("post", "post.text", "text")));

    match put(&lens, &view, &complement) {
        Err(LensError::PutDomain(PutDomainError::IdCollision {
            elem: ViewRef::Node(2),
        })) => {}
        other => panic!("expected an id collision on node 2, got {other:?}"),
    }
}

/// `a` and `b` both map to `t`: the remap is not injective.
fn merge_two_vertices() -> (Lens, WInstance) {
    let ra = prop("r", "a", "a");
    let rb = prop("r", "b", "b");
    let rta = prop("r", "t", "a");
    let rtb = prop("r", "t", "b");
    let src = schema(
        &[("r", "object"), ("a", "string"), ("b", "string")],
        &[ra.clone(), rb.clone()],
    );
    let tgt = schema(
        &[("r", "object"), ("t", "string")],
        &[rta.clone(), rtb.clone()],
    );
    let lens = lens(
        src,
        tgt,
        &[("r", "r"), ("a", "t"), ("b", "t")],
        vec![(ra.clone(), rta), (rb.clone(), rtb)],
    );
    let source = instance(
        vec![
            Node::new(0, "r"),
            leaf(1, "a", "from a"),
            leaf(2, "b", "from b"),
        ],
        vec![(0, 1, ra), (0, 2, rb)],
    );
    (lens, source)
}

#[test]
fn anchors_merged_by_the_remap_come_back_as_they_were() {
    let (lens, source) = merge_two_vertices();
    let (view, complement) = get(&lens, &source).expect("get");
    assert_eq!(view.nodes[&1].anchor, "t");
    assert_eq!(view.nodes[&2].anchor, "t");

    let restored = put(&lens, &view, &complement).expect("put");
    assert_eq!(restored.nodes[&1].anchor, "a");
    assert_eq!(restored.nodes[&2].anchor, "b");
    check_get_put(&lens, &source).expect("GetPut holds under a non-injective remap");
}

#[test]
fn a_node_added_under_a_merged_anchor_has_no_single_source_anchor() {
    let (lens, source) = merge_two_vertices();
    let (mut view, complement) = get(&lens, &source).expect("get");
    view.nodes.insert(7, leaf(7, "t", "added"));
    view.arcs.push((0, 7, prop("r", "t", "a")));
    view = WInstance::new(
        view.nodes,
        view.arcs,
        view.fans,
        view.root,
        view.schema_root,
    );

    match put(&lens, &view, &complement) {
        Err(LensError::PutDomain(PutDomainError::AmbiguousReverseAnchor {
            elem: ViewRef::Node(7),
            candidates,
        })) => {
            assert_eq!(candidates, vec![Name::from("a"), Name::from("b")]);
        }
        other => panic!("expected an ambiguous reverse anchor for node 7, got {other:?}"),
    }
}

/// `post` with `tag` children interleaved with `note` children the lens
/// drops. A view that loses a tag and a view that has it back do not
/// compose: once the first put has forgotten where the tag sat among the
/// notes, the second cannot put it back.
#[test]
fn put_put_catches_a_lens_whose_put_depends_on_the_previous_source() {
    let tag = prop("post", "post.tag", "tags");
    let note = prop("post", "post.note", "notes");
    let src = schema(
        &[
            ("post", "object"),
            ("post.tag", "string"),
            ("post.note", "string"),
        ],
        &[tag.clone(), note.clone()],
    );
    let tgt = schema(
        &[("post", "object"), ("post.tag", "string")],
        std::slice::from_ref(&tag),
    );
    let lens = lens(
        src,
        tgt,
        &[("post", "post"), ("post.tag", "post.tag")],
        vec![(tag.clone(), tag.clone())],
    );
    let source = instance(
        vec![
            Node::new(0, "post"),
            leaf(1, "post.tag", "a"),
            leaf(2, "post.note", "x"),
            leaf(3, "post.tag", "b"),
            leaf(4, "post.note", "y"),
            leaf(5, "post.tag", "c"),
        ],
        vec![
            (0, 1, tag.clone()),
            (0, 2, note.clone()),
            (0, 3, tag.clone()),
            (0, 4, note),
            (0, 5, tag),
        ],
    );
    let (view, _) = get(&lens, &source).expect("get");

    // First view: the middle tag removed.
    let mut without_b = view.clone();
    without_b.nodes.remove(&3);
    without_b.arcs.retain(|(_, child, _)| *child != 3);
    let without_b = WInstance::new(
        without_b.nodes,
        without_b.arcs,
        without_b.fans,
        without_b.root,
        without_b.schema_root,
    );

    match check_put_put(&lens, &source, &without_b, &view) {
        Err(LawViolation::PutPut { .. }) => {}
        other => panic!("expected a PutPut violation, got {other:?}"),
    }
}

#[test]
fn put_put_refuses_a_first_view_that_changes_nothing() {
    let (lens, source) = drop_subtitle();
    let (view, _) = get(&lens, &source).expect("get");
    assert!(matches!(
        check_put_put(&lens, &source, &view, &view),
        Err(LawViolation::Error(_))
    ));
}
