#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A key-value reference resolved to an edge, and its put.

use std::collections::BTreeMap;

use panproto_reference::model::{GEdge, Node, SEdge, Tree};
use panproto_reference::resolution::{self, ResolveError};
use panproto_reference::schema::{
    Cardinality, Key, KeyPart, KeyRole, KeyScope, OffsetUnit, OnUnresolved, PathSyntax, RefSource,
    RefTextSource, Reference, ResolveMode, Schema, SpanEnd, TargetSet, UriBase, VariableBinding,
};
use panproto_reference::value::{Presence, Value};

fn setup(policy: OnUnresolved) -> (Schema, Tree) {
    setup_with(policy, Cardinality::One, ResolveMode::AddEdge)
}

fn setup_with(policy: OnUnresolved, cardinality: Cardinality, mode: ResolveMode) -> (Schema, Tree) {
    let lead = SEdge::new("film", "s", "prop", Some("lead"));
    let name = SEdge::new("actor", "s", "prop", Some("name"));
    let link = SEdge::new("film", "actor", "ref", Some("lead_actor"));
    let mut s = Schema::default()
        .vertex("doc", "obj")
        .vertex("film", "obj")
        .vertex("actor", "obj")
        .vertex("s", "str")
        .edge("doc", "film", "prop", Some("film"))
        .edge("doc", "actor", "prop", Some("actor"));
    s.edges.extend([lead.clone(), name.clone(), link.clone()]);
    s.keys.insert(
        "actor".into(),
        vec![Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![name])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
            equality: Vec::new(),
        }],
    );
    s.references.insert(
        "film_lead".into(),
        Reference {
            id: "film_lead".into(),
            holder: "film".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![lead]],
            },
            target_vertex: "actor".into(),
            target_key: Some("pk".into()),
            edge: Some(link),
            cardinality,
            nullable: false,
            on_unresolved: policy,
            mode,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let nodes = BTreeMap::from([
        (0, Node::new(0, "doc")),
        (
            1,
            Node::new(1, "film").with_field("lead", Value::Str("Ann".into())),
        ),
        (
            2,
            Node::new(2, "actor").with_field("name", Value::Str("Ann".into())),
        ),
        (
            3,
            Node::new(3, "actor").with_field("name", Value::Str("Bob".into())),
        ),
    ]);
    let arcs = vec![
        (0, 1, SEdge::new("doc", "film", "prop", Some("film"))),
        (0, 2, SEdge::new("doc", "actor", "prop", Some("actor"))),
        (0, 3, SEdge::new("doc", "actor", "prop", Some("actor"))),
    ];
    (
        s,
        Tree {
            nodes,
            arcs,
            fans: vec![],
            root: 0,
        },
    )
}

#[test]
fn a_reference_resolves_and_a_reconnected_edge_rewrites_the_key() {
    let (s, t) = setup(OnUnresolved::Error);
    let (view, c) = resolution::get(&s, &t, false).unwrap();
    assert_eq!(view.edges[&3].src, 1);
    assert_eq!(view.edges[&3].tgt, 2);
    assert_eq!(resolution::put(&s, &view, &c), Ok(t));
    let mut moved = view;
    moved.edges.insert(
        3,
        GEdge {
            tgt: 3,
            ..moved.edges[&3].clone()
        },
    );
    let back = resolution::put(&s, &moved, &c).unwrap();
    assert_eq!(back.nodes[&1].fields["lead"], Value::Str("Bob".into()));
}

#[test]
fn get_put_writes_the_first_nonnull_target_site_of_a_key_path() {
    let source = SEdge::new("holder", "scalar", "prop", Some("code"));
    let target_part = SEdge::new("target", "scalar", "prop", Some("id"));
    let resolved = SEdge::new("holder", "target", "ref", Some("target"));
    let mut schema = Schema::default()
        .vertex("root", "obj")
        .vertex("holder", "obj")
        .vertex("target", "obj")
        .vertex("scalar", "int")
        .edge("root", "holder", "prop", Some("holder"))
        .edge("root", "target", "prop", Some("target"));
    schema
        .edges
        .extend([source.clone(), target_part.clone(), resolved.clone()]);
    schema.keys.insert(
        "target".into(),
        vec![Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![target_part])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "by_code".into(),
        Reference {
            id: "by_code".into(),
            holder: "holder".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![source.clone()]],
            },
            target_vertex: "target".into(),
            target_key: Some("pk".into()),
            edge: Some(resolved),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let tree = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "root")),
            (1, Node::new(1, "holder")),
            (2, Node::new(2, "scalar").with_value(Value::Null)),
            (3, Node::new(3, "scalar").with_value(Value::Int(1))),
            (4, Node::new(4, "target").with_field("id", Value::Int(1))),
        ]),
        arcs: vec![
            (0, 1, SEdge::new("root", "holder", "prop", Some("holder"))),
            (1, 2, source.clone()),
            (1, 3, source),
            (0, 4, SEdge::new("root", "target", "prop", Some("target"))),
        ],
        fans: vec![],
        root: 0,
    };

    let (view, complement) = resolution::get(&schema, &tree, false).unwrap();
    assert_eq!(resolution::put(&schema, &view, &complement), Ok(tree));
}

#[test]
fn a_dangling_reference_is_strict_or_recorded() {
    let (s, mut t) = setup(OnUnresolved::Error);
    t.nodes.insert(
        1,
        Node::new(1, "film").with_field("lead", Value::Str("Cy".into())),
    );
    assert!(matches!(
        resolution::get(&s, &t, false),
        Err(ResolveError::DanglingReference { .. })
    ));
    let (_, c) = resolution::get(&s, &t, true).unwrap();
    assert_eq!(c.unresolved.len(), 1);
}

#[test]
fn a_reference_edge_from_a_missing_node_is_unrepresentable_not_a_panic() {
    let (s, t) = setup(OnUnresolved::Error);
    let (mut view, c) = resolution::get(&s, &t, false).unwrap();
    view.edges.insert(
        3,
        GEdge {
            src: 7,
            ..view.edges[&3].clone()
        },
    );
    assert_eq!(
        format!("{:?}", resolution::put(&s, &view, &c)),
        "Err(UnrepresentableReference { carrier: Edge(3), reference: None })"
    );
}

fn many_tree(t: &mut Tree, leads: &[&str]) {
    t.nodes.insert(
        1,
        Node::new(1, "film").with_field(
            "lead",
            Value::List(leads.iter().map(|l| Value::Str((*l).into())).collect()),
        ),
    );
}

#[test]
fn a_many_reference_resolves_each_position_and_writes_its_sequence_back() {
    let (s, mut t) = setup_with(OnUnresolved::Error, Cardinality::Many, ResolveMode::AddEdge);
    many_tree(&mut t, &["Bob", "Ann", "Bob"]);
    let (view, c) = resolution::get(&s, &t, false).unwrap();
    // One edge per position, duplicates kept, in occurrence order.
    let targets: Vec<(u32, u32)> = view
        .edges
        .iter()
        .filter(|(i, _)| c.resolved.contains_key(i))
        .map(|(_, e)| (e.src, e.tgt))
        .collect();
    assert_eq!(targets, vec![(1, 3), (1, 2), (1, 3)]);
    assert_eq!(resolution::put(&s, &view, &c), Ok(t.clone()));
    // Removing the middle occurrence's edge removes it from the sequence.
    let middle = *c.resolved.iter().find(|(_, o)| o.index == 1).unwrap().0;
    let mut removed = view.clone();
    removed.edges.remove(&middle);
    let back = resolution::put(&s, &removed, &c).unwrap();
    assert_eq!(
        back.nodes[&1].fields["lead"],
        Value::List(vec![Value::Str("Bob".into()), Value::Str("Bob".into())])
    );
    // Reconnecting the first occurrence rewrites its position.
    let first = *c.resolved.iter().find(|(_, o)| o.index == 0).unwrap().0;
    let mut moved = view;
    moved.edges.get_mut(&first).unwrap().tgt = 2;
    let back = resolution::put(&s, &moved, &c).unwrap();
    assert_eq!(
        back.nodes[&1].fields["lead"],
        Value::List(vec![
            Value::Str("Ann".into()),
            Value::Str("Ann".into()),
            Value::Str("Bob".into())
        ])
    );
    // A non-nullable Many with no occurrence is a cardinality mismatch.
    many_tree(&mut t, &[]);
    assert_eq!(
        resolution::get(&s, &t, false).map(|_| ()),
        Err(ResolveError::ReferenceCardinalityMismatch {
            reference: "film_lead".into(),
            at: panproto_reference::lens::ViewRef::Node(1),
            lengths: vec![0],
        })
    );
}

#[test]
fn a_redirect_removes_the_holder_and_points_its_arc_at_the_target() {
    let (s, t) = setup_with(OnUnresolved::Error, Cardinality::One, ResolveMode::Redirect);
    let (view, c) = resolution::get(&s, &t, false).unwrap();
    // The film node is gone, and the reference edge runs from its parent.
    assert!(!view.nodes.contains_key(&1));
    let (id, edge) = view
        .edges
        .iter()
        .find(|(i, _)| c.resolved.contains_key(i))
        .map(|(i, e)| (*i, e.clone()))
        .unwrap();
    assert_eq!((edge.src, edge.tgt), (0, 2));
    assert_eq!(c.redirected[&1].parent, 0);
    assert_eq!(resolution::put(&s, &view, &c), Ok(t));
    // Reconnecting the edge rewrites the restored holder's key.
    let mut moved = view.clone();
    moved.edges.get_mut(&id).unwrap().tgt = 3;
    let back = resolution::put(&s, &moved, &c).unwrap();
    assert_eq!(back.nodes[&1].fields["lead"], Value::Str("Bob".into()));
    // Removing the edge removes the occurrence and its subtree.
    let mut removed = view;
    removed.edges.remove(&id);
    let back = resolution::put(&s, &removed, &c).unwrap();
    assert!(!back.nodes.contains_key(&1));
    assert_eq!(back.arcs.len(), 2);
}

#[test]
fn nearest_preceding_variable_binding_selects_the_shadowing_definition() {
    let name = SEdge::new("def", "scalar", "prop", Some("name"));
    let variable = SEdge::new("use", "scalar", "prop", Some("variable"));
    let resolved = SEdge::new("use", "def", "ref", Some("binding"));
    let mut schema = Schema::default()
        .vertex("root", "obj")
        .vertex("def", "obj")
        .vertex("use", "obj")
        .vertex("scalar", "str")
        .edge("root", "def", "child", Some("def"))
        .edge("root", "use", "child", Some("use"));
    schema
        .edges
        .extend([name.clone(), variable.clone(), resolved.clone()]);
    schema.keys.insert(
        "def".into(),
        vec![Key {
            id: "binder".into(),
            parts: vec![KeyPart::Path(vec![name])],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.references.insert(
        "variable".into(),
        Reference {
            id: "variable".into(),
            holder: "use".into(),
            source: RefSource::Variable {
                field: vec![variable],
                binder_key: "binder".into(),
                binding: VariableBinding::NearestPreceding,
            },
            target_vertex: "def".into(),
            target_key: None,
            edge: Some(resolved),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let tree = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "root")),
            (
                1,
                Node::new(1, "def").with_field("name", Value::Str("x".into())),
            ),
            (
                2,
                Node::new(2, "def").with_field("name", Value::Str("x".into())),
            ),
            (
                3,
                Node::new(3, "use").with_field("variable", Value::Str("x".into())),
            ),
        ]),
        arcs: vec![
            (0, 1, SEdge::new("root", "def", "child", Some("def"))),
            (0, 2, SEdge::new("root", "def", "child", Some("def"))),
            (0, 3, SEdge::new("root", "use", "child", Some("use"))),
        ],
        fans: vec![],
        root: 0,
    };
    let (view, complement) = resolution::get(&schema, &tree, false).unwrap();
    let edge = view
        .edges
        .iter()
        .find(|(id, _)| complement.resolved.contains_key(id))
        .unwrap()
        .1;
    assert_eq!(edge.tgt, 2);
    assert_eq!(resolution::put(&schema, &view, &complement), Ok(tree));
}

#[test]
fn path_forms_round_trip_and_utf16_standoff_resolves_to_byte_span() {
    path_form_round_trips();
    utf16_standoff_resolves_to_byte_span();
}

fn path_form_round_trips() {
    let path = SEdge::new("holder", "scalar", "prop", Some("path"));
    let base = SEdge::new("holder", "scalar", "prop", Some("base"));
    let iri = SEdge::new("target", "scalar", "prop", Some("iri"));
    let path_edge = SEdge::new("holder", "target", "ref", Some("path_target"));
    let mut path_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("holder", "obj")
        .vertex("target", "obj")
        .vertex("scalar", "str")
        .edge("root", "holder", "child", Some("holder"))
        .edge("root", "target", "child", Some("target"));
    path_schema
        .edges
        .extend([path.clone(), base.clone(), iri.clone(), path_edge.clone()]);
    path_schema.keys.insert(
        "target".into(),
        vec![Key {
            id: "iri".into(),
            parts: vec![KeyPart::Path(vec![iri])],
            role: KeyRole::Identifier,
            scope: KeyScope::Global,
            equality: Vec::new(),
        }],
    );
    path_schema.references.insert(
        "path".into(),
        Reference {
            id: "path".into(),
            holder: "holder".into(),
            source: RefSource::Path {
                field: vec![path],
                syntax: PathSyntax::Uri {
                    base: UriBase::Field(vec![base]),
                },
            },
            target_vertex: "target".into(),
            target_key: Some("iri".into()),
            edge: Some(path_edge),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let path_tree = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "root")),
            (
                1,
                Node::new(1, "holder")
                    .with_field("base", Value::Str("http://a/b/".into()))
                    .with_field("path", Value::Str("c".into())),
            ),
            (
                2,
                Node::new(2, "target").with_field("iri", Value::Str("http://a/b/c".into())),
            ),
        ]),
        arcs: vec![
            (0, 1, SEdge::new("root", "holder", "child", Some("holder"))),
            (0, 2, SEdge::new("root", "target", "child", Some("target"))),
        ],
        fans: vec![],
        root: 0,
    };
    let (view, complement) = resolution::get(&path_schema, &path_tree, false).unwrap();
    assert_eq!(
        resolution::put(&path_schema, &view, &complement),
        Ok(path_tree)
    );
}

fn utf16_standoff_resolves_to_byte_span() {
    let (span_schema, span_tree) = standoff_fixture();
    materialized_spans_have_site_identity(&span_schema, &span_tree);
    external_context_is_separate_from_identity(&span_schema, &span_tree);
    let (view, complement) = resolution::get(&span_schema, &span_tree, false).unwrap();
    assert_eq!(
        resolution::put(&span_schema, &view, &complement),
        Ok(span_tree)
    );
}

fn standoff_fixture() -> (Schema, Tree) {
    let text = SEdge::new("annotation", "scalar", "prop", Some("text"));
    let start = SEdge::new("annotation", "scalar", "prop", Some("start"));
    let end = SEdge::new("annotation", "scalar", "prop", Some("end"));
    let span_start = SEdge::new("span", "scalar", "prop", Some("start"));
    let span_end = SEdge::new("span", "scalar", "prop", Some("end"));
    let span_edge = SEdge::new("annotation", "span", "ref", Some("span"));
    let mut span_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("annotation", "obj")
        .vertex("span", "obj")
        .vertex("scalar", "value")
        .edge("root", "annotation", "child", Some("annotation"))
        .edge("root", "span", "child", Some("span"));
    span_schema.edges.extend([
        text.clone(),
        start.clone(),
        end.clone(),
        span_start.clone(),
        span_end.clone(),
        span_edge.clone(),
    ]);
    span_schema.keys.insert(
        "span".into(),
        vec![Key {
            id: "bytes".into(),
            parts: vec![
                KeyPart::Path(vec![span_start]),
                KeyPart::Path(vec![span_end]),
            ],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: Vec::new(),
        }],
    );
    span_schema.references.insert(
        "span".into(),
        Reference {
            id: "span".into(),
            holder: "annotation".into(),
            source: RefSource::Standoff {
                text: RefTextSource::Local(vec![text]),
                start: vec![start],
                end: vec![end],
                unit: OffsetUnit::Utf16,
                end_inclusive: false,
                end_form: SpanEnd::Offset,
                origin: 0,
                fragments: None,
            },
            target_vertex: "span".into(),
            target_key: Some("bytes".into()),
            edge: Some(span_edge),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let span_tree = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "root")),
            (
                1,
                Node::new(1, "annotation")
                    .with_field("text", Value::Str("😀a".into()))
                    .with_field("start", Value::Int(2))
                    .with_field("end", Value::Int(3)),
            ),
            (
                2,
                Node::new(2, "span")
                    .with_field("start", Value::Int(4))
                    .with_field("end", Value::Int(5)),
            ),
        ]),
        arcs: vec![
            (
                0,
                1,
                SEdge::new("root", "annotation", "child", Some("annotation")),
            ),
            (0, 2, SEdge::new("root", "span", "child", Some("span"))),
        ],
        fans: vec![],
        root: 0,
    };
    (span_schema, span_tree)
}

fn materialized_spans_have_site_identity(span_schema: &Schema, span_tree: &Tree) {
    let mut materialized_tree = span_tree.clone();
    materialized_tree.nodes.remove(&2);
    materialized_tree.arcs.retain(|(_, target, _)| *target != 2);
    materialized_tree.nodes.insert(
        3,
        Node::new(3, "annotation")
            .with_field("text", Value::Str("😀a".into()))
            .with_field("start", Value::Int(2))
            .with_field("end", Value::Int(3)),
    );
    materialized_tree.arcs.push((
        0,
        3,
        SEdge::new("root", "annotation", "child", Some("annotation")),
    ));
    let (materialized_view, materialized_complement) =
        resolution::get(span_schema, &materialized_tree, false).unwrap();
    assert_eq!(materialized_complement.span_nodes.len(), 2);
    let spans: Vec<_> = materialized_complement.span_nodes.values().collect();
    assert_eq!((spans[0].start, spans[0].end), (4, 5));
    assert_eq!((spans[1].start, spans[1].end), (4, 5));
    assert_ne!(spans[0].text, spans[1].text);
    assert_eq!(
        materialized_complement
            .targets
            .values()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        2
    );
    assert_eq!(
        resolution::put(span_schema, &materialized_view, &materialized_complement),
        Ok(materialized_tree)
    );
}

fn external_context_is_separate_from_identity(span_schema: &Schema, span_tree: &Tree) {
    let site = resolution::ResolutionValueSite::Field {
        holder: 1,
        field: "text".into(),
    };
    assert_eq!(
        resolution::text_identity(1, "text", Some(("route-a", b"key-a"))),
        resolution::text_identity(1, "text", Some(("route-b", b"key-b")))
    );
    assert_ne!(
        resolution::ResolutionTextId::ExternalKey(site),
        resolution::text_identity(3, "text", Some(("route-a", b"key-a")))
    );

    let mut external_schema = span_schema.clone();
    let first = external_schema.references.get_mut("span").unwrap();
    if let RefSource::Standoff { text, .. } = &mut first.source {
        *text = RefTextSource::External(vec![SEdge::new(
            "annotation",
            "scalar",
            "prop",
            Some("text"),
        )]);
    }
    let mut second = first.clone();
    second.id = "span2".into();
    second.edge = Some(SEdge::new("annotation", "span", "ref", Some("span2")));
    external_schema.edges.insert(second.edge.clone().unwrap());
    external_schema.references.insert(second.id.clone(), second);
    let mut one_external_site = span_tree.clone();
    one_external_site.nodes.remove(&2);
    one_external_site.arcs.retain(|(_, target, _)| *target != 2);
    let (mut external_view, external_complement) =
        resolution::get(&external_schema, &one_external_site, false).unwrap();
    assert_eq!(external_complement.span_nodes.len(), 1);
    assert_eq!(external_complement.span_contexts.len(), 1);
    assert_eq!(external_complement.resolved.len(), 2);
    external_view
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("text".into(), Value::Str("😀b".into()));
    assert!(matches!(
        resolution::put(&external_schema, &external_view, &external_complement),
        Err(ResolveError::ContextMismatch { .. })
    ));
}

fn inline_fixture(conflict: bool) -> (Schema, Tree) {
    let holder = SEdge::new("root", "holder", "child", Some("holder"));
    let item = SEdge::new("holder", "item", "child", Some("item"));
    let id = SEdge::new("item", "scalar", "prop", Some("id"));
    let mut schema = Schema::default()
        .vertex("root", "object")
        .vertex("holder", "object")
        .vertex("item", "object")
        .vertex("scalar", "value")
        .edge("root", "holder", "child", Some("holder"));
    schema.edges.extend([item.clone(), id.clone()]);
    schema.keys.insert(
        "item".into(),
        vec![Key {
            id: "identity".into(),
            parts: vec![KeyPart::Path(vec![id])],
            role: KeyRole::Identifier,
            scope: KeyScope::Document,
            equality: Vec::new(),
        }],
    );
    schema.target_sets.insert(
        "entities".into(),
        TargetSet {
            members: vec![("item".into(), "identity".into())],
            scope: KeyScope::Document,
        },
    );
    schema.references.insert(
        "inline".into(),
        Reference {
            id: "inline".into(),
            holder: "holder".into(),
            source: RefSource::Inline {
                field: vec![item.clone()],
            },
            target_vertex: "entities".into(),
            target_key: None,
            edge: Some(SEdge::new("holder", "entities", "child", Some("item"))),
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::Redirect,
            guard: None,
            null_match: panproto_reference::schema::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    let second = if conflict {
        Node::new(4, "item")
            .with_field("id", Value::Int(1))
            .with_field("a", Value::Int(9))
            .with_field("b", Value::Int(2))
    } else {
        Node::new(4, "item")
            .with_field("id", Value::Int(1))
            .with_field("b", Value::Int(2))
    };
    let tree = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "root")),
            (1, Node::new(1, "holder")),
            (2, Node::new(2, "holder")),
            (
                3,
                Node::new(3, "item")
                    .with_field("id", Value::Int(1))
                    .with_field("a", Value::Int(1)),
            ),
            (4, second),
        ]),
        arcs: vec![
            (0, 1, holder.clone()),
            (0, 2, holder),
            (1, 3, item.clone()),
            (2, 4, item),
        ],
        fans: vec![],
        root: 0,
    };
    (schema, tree)
}

#[test]
fn identified_inline_occurrences_merge_and_restore_exact_field_sets() {
    let (schema, tree) = inline_fixture(false);
    let (mut view, complement) = resolution::get(&schema, &tree, false).unwrap();
    assert!(!view.nodes.contains_key(&4));
    assert_eq!(view.nodes[&3].fields["a"], Value::Int(1));
    assert_eq!(view.nodes[&3].fields["b"], Value::Int(2));
    assert_eq!(resolution::put(&schema, &view, &complement), Ok(tree));

    view.nodes
        .get_mut(&3)
        .unwrap()
        .fields
        .insert("a".into(), Value::Int(7));
    view.nodes.get_mut(&3).unwrap().fields.remove("b");
    view.nodes
        .get_mut(&3)
        .unwrap()
        .fields
        .insert("new".into(), Value::Int(8));
    let edited = resolution::put(&schema, &view, &complement).unwrap();
    assert_eq!(edited.nodes[&3].fields["a"], Value::Int(7));
    assert_eq!(edited.nodes[&3].fields["new"], Value::Int(8));
    assert!(!edited.nodes[&4].fields.contains_key("a"));
    assert!(!edited.nodes[&4].fields.contains_key("b"));
    assert!(!edited.nodes[&4].fields.contains_key("new"));
    let (again, _) = resolution::get(&schema, &edited, false).unwrap();
    assert_eq!(again, view);
}

#[test]
fn identified_inline_occurrences_refuse_conflicting_fields() {
    let (schema, tree) = inline_fixture(true);
    assert_eq!(
        resolution::get(&schema, &tree, false),
        Err(ResolveError::InlineConflict {
            reference: "inline".into(),
            at: panproto_reference::lens::ViewRef::Node(4),
            field: "a".into(),
        })
    );
}

fn nested_inline_fixture(second_value: i64) -> (Schema, Tree) {
    let (mut schema, mut tree) = inline_fixture(false);
    let detail = SEdge::new("item", "scalar", "prop", Some("detail"));
    schema.edges.insert(detail.clone());
    tree.nodes
        .insert(5, Node::new(5, "scalar").with_value(Value::Int(1)));
    tree.nodes.insert(
        6,
        Node::new(6, "scalar").with_value(Value::Int(second_value)),
    );
    tree.arcs.push((3, 5, detail.clone()));
    tree.arcs.push((4, 6, detail));
    (schema, tree)
}

#[test]
fn identified_inline_nested_fields_merge_restore_and_propagate_edits() {
    let (schema, tree) = nested_inline_fixture(1);
    let (mut view, complement) = resolution::get(&schema, &tree, false).unwrap();
    assert!(!view.nodes.contains_key(&6));
    assert_eq!(resolution::put(&schema, &view, &complement), Ok(tree));

    view.nodes.get_mut(&5).unwrap().value = Some(Presence::Present(Value::Int(7)));
    let edited = resolution::put(&schema, &view, &complement).unwrap();
    assert_eq!(
        edited.nodes[&6].value,
        Some(Presence::Present(Value::Int(7)))
    );
    let (again, _) = resolution::get(&schema, &edited, false).unwrap();
    assert_eq!(again, view);
}

#[test]
fn identified_inline_nested_field_conflicts_are_refused() {
    let (schema, tree) = nested_inline_fixture(2);
    assert_eq!(
        resolution::get(&schema, &tree, false),
        Err(ResolveError::InlineConflict {
            reference: "inline".into(),
            at: panproto_reference::lens::ViewRef::Node(4),
            field: "detail".into(),
        })
    );
}
