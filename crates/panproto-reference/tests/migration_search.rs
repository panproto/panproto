//! The migration-search reference on worked examples, and its scope counts.

use std::collections::BTreeSet;

use panproto_reference::enumerate::migration_search::{schemas, schemas_count};
use panproto_reference::pages::migration_search::{
    Assignment, Cost, Defect, Edge, EdgeMap, Request, Schema, VertexMap, assignments, check_span,
    check_total, cost, digest, edge_image, edit_distance, optimal_span, optimal_total,
    terms_clear_of_ties,
};

fn schema(vertices: &[(&str, &str)], edges: &[(&str, &str, Option<&str>)]) -> Schema {
    Schema {
        vertices: vertices
            .iter()
            .map(|(id, kind)| ((*id).to_owned(), (*kind).to_owned()))
            .collect(),
        edges: edges
            .iter()
            .map(|(src, tgt, name)| Edge::new(src, tgt, "prop", *name))
            .collect(),
    }
}

fn assign(pairs: &[(&str, Option<&str>)]) -> Assignment {
    pairs
        .iter()
        .map(|(from, to)| ((*from).to_owned(), to.map(str::to_owned)))
        .collect()
}

#[test]
fn the_scope_counts_agree_with_their_formula() {
    for (vertices, edges, expected) in [(1, 0, 6), (2, 1, 126), (2, 2, 468)] {
        let listed = schemas(vertices, edges);
        let distinct: BTreeSet<String> = listed.iter().map(|s| format!("{s:?}")).collect();
        assert_eq!(listed.len(), distinct.len(), "the schemas are distinct");
        assert_eq!(
            listed.len() as u64,
            schemas_count(vertices as u64, edges as u64)
        );
        assert_eq!(listed.len(), expected);
    }
}

#[test]
fn edit_distance_counts_byte_edits() {
    assert_eq!(edit_distance("", ""), 0);
    assert_eq!(edit_distance("a", "ab"), 1);
    assert_eq!(edit_distance("ab", "ba"), 2);
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert_eq!(edit_distance("flaw", "lawn"), 2);
}

#[test]
fn an_edge_maps_to_its_named_image_and_otherwise_to_the_least() {
    let target = schema(
        &[("x", "object"), ("y", "string")],
        &[
            ("x", "y", Some("m")),
            ("x", "y", Some("n")),
            ("x", "y", None),
        ],
    );
    let named = Edge::new("p", "q", "prop", Some("n"));
    assert_eq!(
        edge_image(&target, &named, "x", "y"),
        Some(Edge::new("x", "y", "prop", Some("n")))
    );
    let other = Edge::new("p", "q", "prop", Some("k"));
    assert_eq!(
        edge_image(&target, &other, "x", "y"),
        Some(Edge::new("x", "y", "prop", None))
    );
    let item = Edge::new("p", "q", "item", None);
    assert_eq!(edge_image(&target, &item, "x", "y"), None);
}

#[test]
fn the_cost_rounds_each_term_and_counts_drops() {
    // `post -> post.text` (named `text`) and a dropped `post.likes`.
    let source = schema(
        &[("p", "object"), ("p.t", "string"), ("p.l", "string")],
        &[("p", "p.t", Some("text")), ("p", "p.l", Some("likes"))],
    );
    let target = schema(
        &[("p", "object"), ("p.t", "string")],
        &[("p", "p.t", Some("text"))],
    );
    let partial = assign(&[("p", Some("p")), ("p.t", Some("p.t")), ("p.l", None)]);
    // `p`: two outgoing edges against one, 1/5 * 1/2 * 1/3, and one of two
    // outgoing names kept, 3/10 * 1/2 over the one named source vertex, which
    // is 11/60 rounded once. `p.t` costs nothing. The dropped `p.l` pays the
    // name and degree shares, 1/4 * 1/3 + 1/5 * 1/3. The edge to the dropped
    // endpoint pays its share, 1/4 * 1/2, and the kept edge keeps its name.
    let expected = 183_333_333 + 150_000_000 + 125_000_000;
    assert_eq!(
        cost(&source, &target, &partial),
        Some(Cost {
            quality_units: expected,
            drops: 1
        })
    );
    assert!(terms_clear_of_ties(&source, &target));
    let wrong_kind = assign(&[("p", Some("p.t")), ("p.t", None), ("p.l", None)]);
    assert_eq!(cost(&source, &target, &wrong_kind), None);
}

#[test]
fn the_identity_is_the_unique_optimum_of_a_rigid_schema() {
    let source = schema(
        &[("a", "object"), ("ab", "object")],
        &[("a", "ab", Some("n"))],
    );
    let Some(optimum) = optimal_total(&source, &source, Request::default()) else {
        panic!("the identity is a total morphism");
    };
    assert_eq!(
        optimum.cost,
        Cost {
            quality_units: 0,
            drops: 0
        }
    );
    assert_eq!(
        optimum.assignments,
        vec![assign(&[("a", Some("a")), ("ab", Some("ab"))])]
    );
    let span = optimal_span(&source, &source, Request::default());
    assert_eq!(
        span.map(|optimum| optimum.assignments),
        Some(optimum.assignments)
    );
}

#[test]
fn a_missing_kind_leaves_no_total_morphism_and_a_span_that_drops() {
    let source = schema(&[("a", "object"), ("b", "string")], &[("a", "b", None)]);
    let target = schema(&[("a", "object")], &[]);
    assert_eq!(optimal_total(&source, &target, Request::default()), None);
    let Some(span) = optimal_span(&source, &target, Request::default()) else {
        panic!("a span always exists");
    };
    assert_eq!(
        span.assignments,
        vec![assign(&[("a", Some("a")), ("b", None)])]
    );
    assert_eq!(span.cost.drops, 1);
    assert_eq!(
        optimal_span(
            &source,
            &target,
            Request {
                epic: true,
                ..Request::default()
            }
        ),
        None
    );
}

#[test]
fn requests_restrict_the_morphisms_ranged_over() {
    let source = schema(&[("a", "string"), ("b", "string")], &[]);
    let target = schema(&[("a", "string")], &[]);
    let monic = Request {
        monic: true,
        ..Request::default()
    };
    assert_eq!(optimal_total(&source, &target, monic), None);
    assert!(optimal_total(&source, &target, Request::default()).is_some());
    let epic = Request {
        epic: true,
        ..Request::default()
    };
    assert_eq!(optimal_total(&target, &source, epic), None);
    // Two loops onto one: a homomorphism and no isomorphism.
    let loops = schema(
        &[("a", "object")],
        &[("a", "a", None), ("a", "a", Some("n"))],
    );
    let one = schema(&[("a", "object")], &[("a", "a", None)]);
    let iso = Request {
        iso: true,
        ..Request::default()
    };
    assert!(optimal_total(&loops, &one, Request::default()).is_some());
    assert_eq!(optimal_total(&loops, &one, iso), None);
    assert!(optimal_total(&loops, &loops, iso).is_some());
}

#[test]
fn the_assignments_are_every_kind_preserving_choice() {
    let source = schema(&[("a", "object"), ("b", "string")], &[]);
    let target = schema(&[("x", "object"), ("y", "string"), ("z", "string")], &[]);
    assert_eq!(assignments(&source, &target, false).len(), 2);
    assert_eq!(assignments(&source, &target, true).len(), 2 * 3);
}

#[test]
fn the_checker_finds_each_defect() {
    let source = schema(
        &[("a", "object"), ("b", "string")],
        &[("a", "b", Some("n"))],
    );
    let target = schema(
        &[("x", "object"), ("y", "string"), ("z", "string")],
        &[("x", "y", Some("n")), ("x", "y", None), ("x", "z", None)],
    );
    let vertices: VertexMap = [("a", "x"), ("b", "y")]
        .iter()
        .map(|(f, t)| ((*f).to_owned(), (*t).to_owned()))
        .collect();
    let good: EdgeMap = [(
        Edge::new("a", "b", "prop", Some("n")),
        Edge::new("x", "y", "prop", Some("n")),
    )]
    .into_iter()
    .collect();
    let request = Request::default();
    assert_eq!(
        check_total(&source, &target, request, &vertices, &good),
        Ok(())
    );
    let unnamed: EdgeMap = [(
        Edge::new("a", "b", "prop", Some("n")),
        Edge::new("x", "y", "prop", None),
    )]
    .into_iter()
    .collect();
    assert_eq!(
        check_total(&source, &target, request, &vertices, &unnamed),
        Err(Defect::NameNotKept(Edge::new("a", "b", "prop", Some("n"))))
    );
    let elsewhere: EdgeMap = [(
        Edge::new("a", "b", "prop", Some("n")),
        Edge::new("x", "z", "prop", None),
    )]
    .into_iter()
    .collect();
    assert_eq!(
        check_total(&source, &target, request, &vertices, &elsewhere),
        Err(Defect::EdgeImage(Edge::new("a", "b", "prop", Some("n"))))
    );
    let short: VertexMap = [("a", "x")]
        .iter()
        .map(|(f, t)| ((*f).to_owned(), (*t).to_owned()))
        .collect();
    assert_eq!(
        check_total(&source, &target, request, &short, &good),
        Err(Defect::VertexDomain)
    );
    let epic = Request {
        epic: true,
        ..Request::default()
    };
    assert_eq!(
        check_total(&source, &target, epic, &vertices, &good),
        Err(Defect::NotSurjective)
    );
    let iso = Request {
        iso: true,
        ..Request::default()
    };
    assert_eq!(
        check_total(&source, &target, iso, &vertices, &good),
        Err(Defect::NotSurjective)
    );

    // A span that keeps `a` alone, and one whose apex forgets the edge.
    let kept: BTreeSet<String> = std::iter::once("a".to_owned()).collect();
    let apex = source.induced(&kept);
    let left_vertices: VertexMap = std::iter::once(("a".to_owned(), "a".to_owned())).collect();
    let right_vertices: VertexMap = std::iter::once(("a".to_owned(), "x".to_owned())).collect();
    let empty = EdgeMap::new();
    assert_eq!(
        check_span(
            &source,
            &target,
            request,
            &apex,
            (&left_vertices, &empty),
            (&right_vertices, &empty)
        ),
        Ok(())
    );
    let both: VertexMap = [("a", "a"), ("b", "b")]
        .iter()
        .map(|(f, t)| ((*f).to_owned(), (*t).to_owned()))
        .collect();
    let without_edge = Schema {
        vertices: source.vertices.clone(),
        edges: BTreeSet::new(),
    };
    assert_eq!(
        check_span(
            &source,
            &target,
            request,
            &without_edge,
            (&both, &empty),
            (&vertices, &empty)
        ),
        Err(Defect::ApexNotInduced)
    );
}

#[test]
fn the_digest_is_fnv_1a() {
    assert_eq!(digest(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(digest(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(digest(b"foobar"), 0x8594_4171_f739_67e8);
}
