//! Exact observations of the diff and the classification: what a diff
//! records for each kind of schema component, and what verdict each
//! recorded change receives.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_check::classify::{
    BreakingChange, NonBreakingChange, classify, classify_with_schemas,
};
use panproto_check::diff::{HyperEdgeChange, KeyChange, SchemaDiff, diff};
use panproto_check::{ConstraintChange, ConstraintDiff, apply_renames};
use panproto_gat::{CoercionClass, Name};
use panproto_schema::{
    CoercionSpec, Constraint, Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema,
    SchemaBuilder, Span, TargetSet,
};

fn protocol() -> Protocol {
    Protocol {
        name: "closure".to_owned(),
        ..Default::default()
    }
}

/// An expression whose only content is the variable `name`.
fn var<T: serde::de::DeserializeOwned>(name: &str) -> T {
    serde_json::from_value(serde_json::json!({ "Var": name })).unwrap()
}

fn coercion(forward: &str, class: CoercionClass) -> CoercionSpec {
    CoercionSpec {
        forward: var(forward),
        inverse: None,
        class,
    }
}

fn constraint(sort: &str, value: &str) -> Constraint {
    Constraint {
        sort: Name::from(sort),
        value: value.to_owned(),
    }
}

/// Vertices `T`, `c`, `d`, `e`, edges `T -> c` and `T -> d`, and a
/// hyper-edge `H` over `T` with the given label signature.
fn base(labels: &[(&str, &str)]) -> Schema {
    let mut signature: HashMap<String, String> = labels
        .iter()
        .map(|(l, v)| ((*l).to_owned(), (*v).to_owned()))
        .collect();
    signature.insert("parent".to_owned(), "T".to_owned());
    SchemaBuilder::new(&protocol())
        .vertex("T", "record", None)
        .and_then(|b| b.vertex("c", "string", None))
        .and_then(|b| b.vertex("d", "string", None))
        .and_then(|b| b.vertex("e", "string", None))
        .and_then(|b| b.edge("T", "c", "prop", Some("c")))
        .and_then(|b| b.edge("T", "d", "prop", Some("d")))
        .and_then(|b| b.hyper_edge("H", "row", signature, "parent"))
        .unwrap()
        .build()
        .unwrap()
}

/// A schema carrying one of every component a diff compares by value:
/// constraints, a hyper-edge, NSIDs, spans, coercions, mergers, defaults,
/// policies and a target set.
fn populated() -> Schema {
    let mut s = base(&[("a", "c"), ("b", "d")]);
    s.constraints.insert(
        Name::from("c"),
        vec![constraint("maxLength", "10"), constraint("minLength", "1")],
    );
    s.nsids.insert(Name::from("T"), Name::from("app.closure.t"));
    for id in ["s1", "s2"] {
        s.spans.insert(
            Name::from(id),
            Span {
                id: Name::from(id),
                left: Name::from("c"),
                right: Name::from("d"),
            },
        );
    }
    s.coercions.insert(
        (Name::from("string"), Name::from("int")),
        coercion("x", CoercionClass::Retraction),
    );
    s.mergers.insert(Name::from("c"), var("m"));
    s.defaults.insert(Name::from("d"), var("dflt"));
    s.policies.insert(Name::from("p"), var("pol"));
    s.declarations.target_sets.insert(
        Name::from("ts"),
        TargetSet {
            members: vec![(Name::from("T"), Name::from("pk"))],
            scope: KeyScope::Document,
        },
    );
    s
}

#[test]
fn a_schema_diffed_against_itself_records_nothing() {
    let s = populated();
    assert_eq!(diff(&s, &s.clone()), SchemaDiff::default());
}

#[test]
fn a_constraint_added_to_a_surviving_vertex_is_a_modification_of_it() {
    let old = base(&[]);
    let mut new = old.clone();
    new.constraints
        .insert(Name::from("c"), vec![constraint("maxLength", "10")]);
    let expected = SchemaDiff {
        modified_constraints: BTreeMap::from([(
            "c".to_owned(),
            ConstraintDiff {
                added: vec![constraint("maxLength", "10")],
                removed: vec![],
                changed: vec![],
            },
        )]),
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn a_constraint_diff_records_exactly_the_added_removed_and_changed_sorts() {
    let mut old = base(&[]);
    old.constraints.insert(
        Name::from("c"),
        vec![
            constraint("maxLength", "10"),
            constraint("minLength", "1"),
            constraint("pattern", "x"),
        ],
    );
    let mut new = old.clone();
    new.constraints.insert(
        Name::from("c"),
        vec![
            constraint("maxLength", "5"),
            constraint("minLength", "1"),
            constraint("format", "uri"),
        ],
    );
    let expected = SchemaDiff {
        modified_constraints: BTreeMap::from([(
            "c".to_owned(),
            ConstraintDiff {
                added: vec![constraint("format", "uri")],
                removed: vec![constraint("pattern", "x")],
                changed: vec![ConstraintChange {
                    sort: "maxLength".to_owned(),
                    old_value: "10".to_owned(),
                    new_value: "5".to_owned(),
                }],
            },
        )]),
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn a_hyper_edge_change_records_only_the_labels_whose_vertex_moved() {
    let old = base(&[("a", "c"), ("b", "d")]);
    let new = base(&[("a", "e"), ("b", "d")]);
    let expected = SchemaDiff {
        modified_hyper_edges: vec![HyperEdgeChange {
            id: "H".to_owned(),
            kind_change: None,
            signature_added: BTreeMap::new(),
            signature_removed: BTreeMap::new(),
            signature_changed: BTreeMap::from([("a".to_owned(), ("c".to_owned(), "e".to_owned()))]),
            parent_label_change: None,
        }],
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn an_nsid_diff_records_the_changed_and_only_the_changed_nsids() {
    let mut old = base(&[]);
    old.nsids.insert(Name::from("T"), Name::from("app.t"));
    old.nsids.insert(Name::from("c"), Name::from("app.c"));
    let mut new = old.clone();
    new.nsids.insert(Name::from("T"), Name::from("app.t2"));
    let expected = SchemaDiff {
        changed_nsids: vec![("T".to_owned(), "app.t".to_owned(), "app.t2".to_owned())],
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn a_span_diff_records_the_removed_span_alone() {
    let old = populated();
    let mut new = old.clone();
    new.spans.remove("s1");
    let expected = SchemaDiff {
        removed_spans: vec!["s1".to_owned()],
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn a_coercion_diff_records_additions_removals_and_modifications() {
    let key = |a: &str, b: &str| (Name::from(a), Name::from(b));
    let mut old = base(&[]);
    old.coercions
        .insert(key("a", "b"), coercion("x", CoercionClass::Iso));
    old.coercions
        .insert(key("c", "d"), coercion("x", CoercionClass::Iso));
    old.coercions
        .insert(key("g", "h"), coercion("x", CoercionClass::Iso));
    let mut new = base(&[]);
    new.coercions
        .insert(key("a", "b"), coercion("y", CoercionClass::Iso));
    new.coercions
        .insert(key("e", "f"), coercion("x", CoercionClass::Iso));
    new.coercions
        .insert(key("g", "h"), coercion("x", CoercionClass::Iso));
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    let expected = SchemaDiff {
        added_coercions: vec![pair("e", "f")],
        removed_coercions: vec![pair("c", "d")],
        modified_coercions: vec![pair("a", "b")],
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn a_name_keyed_expression_diff_records_additions_removals_and_modifications() {
    let mut old = base(&[]);
    let mut new = base(&[]);
    for (name, before, after) in [
        ("kept", Some("x"), Some("x")),
        ("changed", Some("x"), Some("y")),
        ("gone", Some("x"), None),
        ("fresh", None, Some("x")),
    ] {
        for (schema, body) in [(&mut old, before), (&mut new, after)] {
            if let Some(body) = body {
                schema.mergers.insert(Name::from(name), var(body));
                schema.defaults.insert(Name::from(name), var(body));
                schema.policies.insert(Name::from(name), var(body));
            }
        }
    }
    let one = |s: &str| vec![s.to_owned()];
    let expected = SchemaDiff {
        added_mergers: one("fresh"),
        removed_mergers: one("gone"),
        modified_mergers: one("changed"),
        added_defaults: one("fresh"),
        removed_defaults: one("gone"),
        modified_defaults: one("changed"),
        added_policies: one("fresh"),
        removed_policies: one("gone"),
        modified_policies: one("changed"),
        ..SchemaDiff::default()
    };
    assert_eq!(diff(&old, &new), expected);
}

#[test]
fn a_rename_whose_old_vertex_was_not_removed_is_ignored() {
    let mut d = SchemaDiff {
        removed_vertices: vec!["a".to_owned(), "z".to_owned()],
        added_vertices: vec!["c".to_owned()],
        ..SchemaDiff::default()
    };
    let before = d.clone();
    apply_renames(&mut d, &[("x".to_owned(), "c".to_owned())]);
    assert_eq!(d, before);

    apply_renames(&mut d, &[("a".to_owned(), "c".to_owned())]);
    let expected = SchemaDiff {
        removed_vertices: vec!["z".to_owned()],
        renamed_vertices: vec![("a".to_owned(), "c".to_owned())],
        ..SchemaDiff::default()
    };
    assert_eq!(d, expected);
}

#[test]
fn an_ordered_edge_made_unordered_is_reported_as_such() {
    let edge = Edge {
        src: Name::from("T"),
        tgt: Name::from("c"),
        kind: Name::from("item"),
        name: None,
    };
    let d = SchemaDiff {
        order_changes: vec![(edge.clone(), Some(0), None)],
        ..SchemaDiff::default()
    };
    let report = classify(&d, &protocol());
    assert_eq!(
        report.breaking,
        vec![BreakingChange::OrderToUnordered { edge }]
    );
    assert!(report.non_breaking.is_empty());
}

#[test]
fn a_key_recorded_as_changed_but_identical_is_not_tightened() {
    let key = Key {
        id: Name::from("pk"),
        parts: vec![KeyPart::Path(vec![Edge {
            src: Name::from("T"),
            tgt: Name::from("c"),
            kind: Name::from("prop"),
            name: Some(Name::from("c")),
        }])],
        role: KeyRole::Primary,
        scope: KeyScope::Document,
        equality: vec![],
    };
    let d = SchemaDiff {
        modified_keys: vec![KeyChange {
            vertex_id: "T".to_owned(),
            old: key.clone(),
            new: key,
        }],
        ..SchemaDiff::default()
    };
    let report = classify(&d, &protocol());
    assert!(report.breaking.is_empty(), "{:?}", report.breaking);
    assert_eq!(
        report.non_breaking,
        vec![NonBreakingChange::KeyRelaxed {
            vertex_id: "T".to_owned(),
            key_id: "pk".to_owned(),
        }]
    );
}

#[test]
fn numeric_bound_changes_are_tightened_or_relaxed_by_value_not_by_spelling() {
    let change = |sort: &str, old: &str, new: &str| ConstraintChange {
        sort: sort.to_owned(),
        old_value: old.to_owned(),
        new_value: new.to_owned(),
    };
    let only = |c: ConstraintChange| ConstraintDiff {
        added: vec![],
        removed: vec![],
        changed: vec![c],
    };
    let d = SchemaDiff {
        modified_constraints: BTreeMap::from([
            // Same upper bound, spelled differently: not tighter.
            ("a".to_owned(), only(change("maxLength", "10", "010"))),
            // A lower bound lowered: relaxed.
            ("b".to_owned(), only(change("minLength", "5", "3"))),
            // Same lower bound, spelled differently: not tighter.
            ("c".to_owned(), only(change("minimum", "5", "05"))),
            // A lower bound raised: tightened.
            ("d".to_owned(), only(change("minimum", "3", "5"))),
        ]),
        ..SchemaDiff::default()
    };
    let report = classify(&d, &protocol());
    let relaxed =
        |v: &str, sort: &str, old: &str, new: &str| NonBreakingChange::ConstraintRelaxed {
            vertex_id: v.to_owned(),
            sort: sort.to_owned(),
            old_value: old.to_owned(),
            new_value: new.to_owned(),
        };
    assert_eq!(
        report.breaking,
        vec![BreakingChange::ConstraintTightened {
            vertex_id: "d".to_owned(),
            sort: "minimum".to_owned(),
            old_value: "3".to_owned(),
            new_value: "5".to_owned(),
        }]
    );
    assert_eq!(
        report.non_breaking,
        vec![
            relaxed("a", "maxLength", "10", "010"),
            relaxed("b", "minLength", "5", "3"),
            relaxed("c", "minimum", "5", "05"),
        ]
    );
}

#[test]
fn only_a_strictly_lossier_coercion_class_is_a_downgrade() {
    let key = |a: &str, b: &str| (Name::from(a), Name::from(b));
    let mut old = base(&[]);
    let mut new = base(&[]);
    for (k, before, after) in [
        (
            key("int", "string"),
            CoercionClass::Retraction,
            CoercionClass::Projection,
        ),
        (
            key("a", "b"),
            CoercionClass::Projection,
            CoercionClass::Retraction,
        ),
        (key("c", "d"), CoercionClass::Iso, CoercionClass::Iso),
    ] {
        old.coercions.insert(k.clone(), coercion("x", before));
        new.coercions.insert(k, coercion("x", after));
    }
    let report = classify_with_schemas(&SchemaDiff::default(), &protocol(), &old, &new);
    assert_eq!(
        report.breaking,
        vec![BreakingChange::CoercionClassDowngraded {
            from_kind: "int".to_owned(),
            to_kind: "string".to_owned(),
            old_class: "Retraction".to_owned(),
            new_class: "Projection".to_owned(),
        }]
    );
    assert!(!report.compatible);
}

#[test]
fn a_coercion_enrichment_is_reported_under_its_source_and_target_kinds() {
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    let d = SchemaDiff {
        added_coercions: vec![pair("int", "string")],
        removed_coercions: vec![pair("float", "int")],
        modified_coercions: vec![pair("bool", "string")],
        ..SchemaDiff::default()
    };
    let report = classify(&d, &protocol());
    assert_eq!(
        report.non_breaking,
        vec![NonBreakingChange::EnrichmentAdded {
            category: "coercion".to_owned(),
            key: "int -> string".to_owned(),
        }]
    );
    assert_eq!(
        report.breaking,
        vec![
            BreakingChange::EnrichmentRemoved {
                category: "coercion".to_owned(),
                key: "float -> int".to_owned(),
            },
            BreakingChange::EnrichmentModified {
                category: "coercion".to_owned(),
                key: "bool -> string".to_owned(),
            },
        ]
    );
}
