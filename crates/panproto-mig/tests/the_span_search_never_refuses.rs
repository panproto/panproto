//! Whether the span search really never refuses for want of a match.
//!
//! [`SpanSearch::run`]'s contract is that the assignment leaving every source
//! vertex out of the apex is always feasible, so a pair with nothing in common
//! comes back as a span with an empty apex rather than as an error. The
//! argument for it is that every hard constraint the network carries is
//! satisfied by all-`⊥`. This attacks that argument from both ends:
//!
//! 1. **The network.** A source is built carrying every one of the five apex
//!    hard constraints (a required edge, a coproduct with two arms, a fixpoint
//!    marker, a schema span and a hyper-edge signature) and searched under
//!    every option that collapses domains: incompatible pins on every vertex,
//!    `excluded_sources` naming every vertex, `restricted_domains` empty for
//!    every vertex, and every target excluded. If any of those made all-`⊥`
//!    infeasible, the search would have nothing to return.
//! 2. **The apex.** The one error the contract admits past the network is
//!    [`SpanError::Apex`], whose doc says it is "unreachable from a feasible
//!    assignment and reaching it means a hard constraint is missing". Inducing
//!    validates the apex against the protocol, so that sentence is a claim
//!    about induction inventing no finding the parent did not already carry.
//!    The last test here holds it to that: a source the protocol rejects is
//!    refused, and the refusal is the parent's finding rather than a missing
//!    constraint.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_mig::hom_search::{DomainConstraints, SearchOptions};
use panproto_mig::{SpanError, SpanSearch};
use panproto_schema::{
    EdgeRule, HyperEdge, Protocol, RecursionPoint, Schema, SchemaBuilder, Span, Variant,
};

fn protocol() -> Protocol {
    Protocol {
        name: "refusal".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![
            EdgeRule {
                edge_kind: "prop".to_owned(),
                src_kinds: vec!["object".to_owned()],
                tgt_kinds: vec!["string".to_owned(), "object".to_owned()],
            },
            EdgeRule {
                edge_kind: "variant".to_owned(),
                src_kinds: vec!["object".to_owned()],
                tgt_kinds: vec!["string".to_owned(), "object".to_owned()],
            },
        ],
        obj_kinds: vec!["object".to_owned(), "string".to_owned()],
        constraint_sorts: vec!["maxLength".to_owned()],
        ..Protocol::default()
    }
}

/// A source carrying all five apex hard constraints at once.
///
/// The five are added by direct field assignment because no builder method
/// reaches them, and because the network reads the fields rather than any
/// invariant a builder would enforce.
fn loaded_source() -> Schema {
    let proto = protocol();
    let mut schema = SchemaBuilder::new(&proto)
        .vertex("body", "object", None::<&str>)
        .expect("body")
        .vertex("body.text", "string", None::<&str>)
        .expect("text")
        .vertex("body.alt", "string", None::<&str>)
        .expect("alt")
        .vertex("shape", "object", None::<&str>)
        .expect("shape")
        .vertex("shape.circle", "string", None::<&str>)
        .expect("circle")
        .vertex("shape.square", "string", None::<&str>)
        .expect("square")
        .vertex("node", "object", None::<&str>)
        .expect("node")
        .edge("body", "body.text", "prop", Some("text"))
        .expect("text edge")
        .edge("body", "body.alt", "prop", Some("alt"))
        .expect("alt edge")
        .edge("shape", "shape.circle", "variant", Some("circle"))
        .expect("circle edge")
        .edge("shape", "shape.square", "variant", Some("square"))
        .expect("square edge")
        .edge("body", "node", "prop", Some("child"))
        .expect("child edge")
        .entry("body")
        .build()
        .expect("build");

    // 1. a required edge
    let required = schema
        .edges
        .keys()
        .find(|edge| edge.name.as_deref() == Some("text"))
        .expect("the text edge")
        .clone();
    schema.required.insert(Name::from("body"), vec![required]);

    // 2. a coproduct with two arms
    schema.variants.insert(
        Name::from("shape"),
        vec![
            Variant {
                id: Name::from("shape.circle"),
                parent_vertex: Name::from("shape"),
                tag: Some(Name::from("circle")),
            },
            Variant {
                id: Name::from("shape.square"),
                parent_vertex: Name::from("shape"),
                tag: Some(Name::from("square")),
            },
        ],
    );

    // 3. a fixpoint marker
    schema.recursion_points.insert(
        Name::from("node"),
        RecursionPoint {
            target_vertex: Name::from("body"),
        },
    );

    // 4. a schema span
    schema.spans.insert(
        Name::from("link"),
        Span {
            id: Name::from("link"),
            left: Name::from("body.text"),
            right: Name::from("body.alt"),
        },
    );

    // 5. a hyper-edge signature over three vertices
    let mut signature = HashMap::new();
    signature.insert(Name::from("a"), Name::from("body"));
    signature.insert(Name::from("b"), Name::from("shape"));
    signature.insert(Name::from("c"), Name::from("node"));
    schema.hyper_edges.insert(
        Name::from("sig"),
        HyperEdge {
            id: Name::from("sig"),
            signature,
            kind: Name::from("relation"),
            parent_label: Name::from("a"),
        },
    );

    schema
}

/// A target sharing nothing with the source: one vertex of each kind, named so
/// that no name, degree or property set agrees.
fn stranger() -> Schema {
    SchemaBuilder::new(&protocol())
        .vertex("zzz", "object", None::<&str>)
        .expect("zzz")
        .vertex("zzz.qqq", "string", None::<&str>)
        .expect("qqq")
        .edge("zzz", "zzz.qqq", "prop", Some("qqq"))
        .expect("edge")
        .build()
        .expect("build")
}

/// Every option that collapses a domain, against a loaded source.
///
/// Split out of the test so that the list of collapses reads as a list rather
/// than as the first hundred lines of a loop.
fn domain_collapse_cases(
    src: &Schema,
    tgt: &Schema,
) -> Vec<(&'static str, SearchOptions, DomainConstraints)> {
    let sources: Vec<Name> = {
        let mut names: Vec<Name> = src.vertices.keys().cloned().collect();
        names.sort_unstable();
        names
    };

    // Every source vertex pinned to a target of the wrong kind, which leaves
    // each of them `⊥` and nothing else.
    let incompatible: HashMap<Name, Name> = sources
        .iter()
        .map(|name| {
            let wrong = if src.vertices[name].kind.as_str() == "object" {
                "zzz.qqq"
            } else {
                "zzz"
            };
            (name.clone(), Name::from(wrong))
        })
        .collect();

    // Every source vertex pinned to a target that does not exist at all.
    let absent: HashMap<Name, Name> = sources
        .iter()
        .map(|name| (name.clone(), Name::from("nowhere")))
        .collect();

    vec![
        (
            "nothing in common",
            SearchOptions::default(),
            DomainConstraints::default(),
        ),
        (
            "every vertex pinned to the wrong kind",
            SearchOptions {
                hard_pins: incompatible,
                ..SearchOptions::default()
            },
            DomainConstraints::default(),
        ),
        (
            "every vertex pinned to a vertex that is not there",
            SearchOptions {
                hard_pins: absent,
                ..SearchOptions::default()
            },
            DomainConstraints::default(),
        ),
        (
            "every source excluded",
            SearchOptions::default(),
            DomainConstraints {
                excluded_sources: sources.iter().cloned().collect(),
                ..DomainConstraints::default()
            },
        ),
        (
            "every domain restricted to nothing",
            SearchOptions::default(),
            DomainConstraints {
                restricted_domains: sources
                    .iter()
                    .map(|name| (name.clone(), Vec::new()))
                    .collect(),
                ..DomainConstraints::default()
            },
        ),
        (
            "every target excluded",
            SearchOptions::default(),
            DomainConstraints {
                excluded_targets: tgt.vertices.keys().cloned().collect(),
                ..DomainConstraints::default()
            },
        ),
    ]
}

/// One member of each apex hard constraint forced out, the rest left in.
///
/// These are the cases the constraints exist for: an implication whose
/// antecedent survives while its consequent cannot, which is what would make
/// the all-`⊥` assignment infeasible if the constraint were an equality rather
/// than an implication.
fn one_member_out_cases() -> Vec<(&'static str, SearchOptions, DomainConstraints)> {
    ["shape.circle", "body.text", "body", "node"]
        .into_iter()
        .zip([
            "one arm of the coproduct forced out, the coproduct left in",
            "one end of the schema span forced out",
            "the fixpoint's unfolding forced out",
            "one member of the hyper-edge signature forced out",
        ])
        .map(|(vertex, label)| {
            (
                label,
                SearchOptions::default(),
                DomainConstraints {
                    excluded_sources: HashSet::from_iter([Name::from(vertex)]),
                    ..DomainConstraints::default()
                },
            )
        })
        .collect()
}

/// None of them makes the all-`⊥` assignment infeasible, so none of them
/// makes the search refuse.
#[test]
fn no_domain_collapse_makes_the_all_bottom_assignment_infeasible() {
    let src = loaded_source();
    let tgt = stranger();
    let proto = protocol();

    let cases = domain_collapse_cases(&src, &tgt)
        .into_iter()
        .chain(one_member_out_cases());
    for (name, opts, constraints) in cases {
        let span = SpanSearch::new(&proto)
            .with_options(opts)
            .with_constraints(constraints)
            .run(&src, &tgt)
            .unwrap_or_else(|error| {
                panic!("the span search refused `{name}`, which the contract forbids: {error}")
            });
        assert!(
            span.certificate.legs_are_functorial,
            "`{name}` produced legs that are not functors"
        );
        assert!(
            (0.0..=1.0).contains(&span.apex_coverage),
            "`{name}` reported a coverage of {}",
            span.apex_coverage
        );
        assert!(
            span.quality.is_finite(),
            "`{name}` reported a quality of {}",
            span.quality
        );
    }
}

/// The empty source and the empty target are both answered, in both
/// directions.
#[test]
fn the_degenerate_pairs_are_answered() {
    let proto = protocol();
    let empty = {
        let mut only = stranger();
        only.vertices.clear();
        only.edges.clear();
        only.entries.clear();
        only.outgoing.clear();
        only.incoming.clear();
        only.between.clear();
        only.nsids.clear();
        only
    };
    let real = stranger();

    for (name, src, tgt) in [
        ("empty to empty", &empty, &empty),
        ("empty to real", &empty, &real),
        ("real to empty", &real, &empty),
    ] {
        let span = SpanSearch::new(&proto)
            .run(src, tgt)
            .unwrap_or_else(|error| panic!("the span search refused `{name}`: {error}"));
        assert!(
            (0.0..=1.0).contains(&span.apex_coverage),
            "`{name}` reported a coverage of {}",
            span.apex_coverage
        );
    }
}

/// The one refusal past the network, and what it is really about.
///
/// [`SpanError::Apex`]'s doc says reaching it means a hard constraint is
/// missing. It does not: inducing validates the apex against the protocol and
/// the span search validates neither of its inputs, so a source the protocol
/// rejects refuses here with the *parent's* finding and no constraint is
/// missing at all. The test records which of the two it is, so that a future
/// reader chasing the error message is not sent after a constraint that was
/// never absent.
#[test]
fn a_source_the_protocol_rejects_refuses_at_the_apex_not_at_the_network() {
    let proto = protocol();
    // A kind the protocol does not know, on both sides, so that the vertex has
    // a kind-compatible target and the optimum keeps it. With the kind on the
    // source alone its domain is empty, the optimum drops it, and the apex
    // validates, which is the search answering, not refusing.
    let unknown_kind = |mut schema: Schema| -> Schema {
        schema
            .vertices
            .get_mut(&Name::from("zzz.qqq"))
            .expect("qqq is there")
            .kind = Name::from("blob");
        schema
    };
    let src = unknown_kind(stranger());
    let tgt = unknown_kind(stranger());

    let refused = SpanSearch::new(&proto).run(&src, &tgt);
    let Err(SpanError::Apex { .. }) = refused else {
        // If this ever starts succeeding, the search grew an input check and
        // the note above is stale rather than wrong.
        panic!(
            "a source carrying a kind the protocol does not know was answered rather than \
             refused: {refused:?}"
        );
    };

    // And the finding is the parent's: the same schema fails `validate`
    // directly, so nothing about the apex construction produced it.
    assert!(
        !panproto_schema::validate(&src, &proto).is_empty(),
        "the refusal must be inherited from the source, not invented by inducing"
    );
}
