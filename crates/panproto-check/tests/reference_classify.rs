//! Factorized and bounded non-key differentials for change classification.

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use panproto_check::classify::classify;
use panproto_check::diff::diff;
use panproto_gat::Name;
use panproto_reference::enumerate::keys_and_registration as enumerate;
use panproto_reference::{keys as reference_keys, schema as reference};
use panproto_schema::{
    Cardinality, Edge, Key, KeyEquality, KeyPart, KeyRole, KeyScope, NullMatch, NumericPrimitive,
    OnUnresolved, Protocol, RefGuard, RefSource, Reference, ResolveMode, Schema, ScopeVia,
    SqlCollation, SqlPad, TargetSet, VariableBinding, Vertex,
};

fn protocol() -> Protocol {
    Protocol {
        name: "o17-keys".to_owned(),
        ..Protocol::default()
    }
}

fn edge(edge: &panproto_reference::model::SEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn key_part(part: &reference::KeyPart) -> KeyPart {
    match part {
        reference::KeyPart::Path(path) => KeyPart::Path(path.iter().map(edge).collect()),
        reference::KeyPart::Position { within } => KeyPart::Position {
            within: edge(within),
        },
    }
}

const fn numeric_primitive(primitive: reference_keys::NumericPrimitive) -> NumericPrimitive {
    match primitive {
        reference_keys::NumericPrimitive::Decimal => NumericPrimitive::Decimal,
        reference_keys::NumericPrimitive::Float(width) => NumericPrimitive::Float(width),
        reference_keys::NumericPrimitive::Any => NumericPrimitive::Any,
    }
}

fn key_equality(equality: &reference_keys::KeyEquality) -> KeyEquality {
    match equality {
        reference_keys::KeyEquality::Representation => KeyEquality::Representation,
        reference_keys::KeyEquality::NumericValue => KeyEquality::NumericValue,
        reference_keys::KeyEquality::NumericValueWithinKind(primitive) => {
            KeyEquality::NumericValueWithinKind(numeric_primitive(*primitive))
        }
        reference_keys::KeyEquality::Sql { collation, pad } => KeyEquality::Sql {
            collation: match collation {
                reference_keys::SqlCollation::Binary => SqlCollation::Binary,
                reference_keys::SqlCollation::AsciiCaseInsensitive => {
                    SqlCollation::AsciiCaseInsensitive
                }
                reference_keys::SqlCollation::Named(name) => {
                    SqlCollation::Named(name.as_str().into())
                }
            },
            pad: match pad {
                reference_keys::SqlPad::NoPad => SqlPad::NoPad,
                reference_keys::SqlPad::PadSpace => SqlPad::PadSpace,
            },
        },
        reference_keys::KeyEquality::XmlNonCdata => KeyEquality::XmlNonCdata,
    }
}

const fn key_role(role: reference::KeyRole) -> KeyRole {
    match role {
        reference::KeyRole::Primary => KeyRole::Primary,
        reference::KeyRole::Unique => KeyRole::Unique,
        reference::KeyRole::Identifier => KeyRole::Identifier,
        reference::KeyRole::SortKey => KeyRole::SortKey,
        reference::KeyRole::Index => KeyRole::Index,
    }
}

fn key_scope(scope: &reference::KeyScope) -> KeyScope {
    match scope {
        reference::KeyScope::Document => KeyScope::Document,
        reference::KeyScope::Global => KeyScope::Global,
        reference::KeyScope::Within(vertex) => KeyScope::Within(vertex.as_str().into()),
    }
}

fn key(key: &reference::Key) -> Key {
    Key {
        id: key.id.as_str().into(),
        parts: key.parts.iter().map(key_part).collect(),
        role: key_role(key.role),
        scope: key_scope(&key.scope),
        equality: key.equality.iter().map(key_equality).collect(),
    }
}

fn source(source: &reference::RefSource) -> RefSource {
    match source {
        reference::RefSource::KeyValue { parts } => RefSource::KeyValue {
            parts: parts
                .iter()
                .map(|path| path.iter().map(edge).collect())
                .collect(),
        },
        reference::RefSource::Variable {
            field,
            binder_key,
            binding,
        } => RefSource::Variable {
            field: field.iter().map(edge).collect(),
            binder_key: binder_key.as_str().into(),
            binding: match binding {
                reference::VariableBinding::FirstDefines => VariableBinding::FirstDefines,
                reference::VariableBinding::NearestPreceding => VariableBinding::NearestPreceding,
            },
        },
        reference::RefSource::Inline { field } => RefSource::Inline {
            field: field.iter().map(edge).collect(),
        },
        other => panic!("source outside the bounded classification carrier: {other:?}"),
    }
}

fn guard(guard: &reference::RefGuard) -> RefGuard {
    let equals = match &guard.equals {
        panproto_reference::value::Value::Str(value) => {
            serde_json::from_value(serde_json::json!({"Str": value}))
                .expect("string guard has the production value form")
        }
        value => panic!("guard outside the bounded classification carrier: {value:?}"),
    };
    RefGuard {
        field: guard.field.iter().map(edge).collect(),
        equals,
    }
}

fn reference(declaration: &reference::Reference) -> Reference {
    Reference {
        id: declaration.id.as_str().into(),
        holder: declaration.holder.as_str().into(),
        source: source(&declaration.source),
        target_vertex: declaration.target_vertex.as_str().into(),
        target_key: declaration.target_key.as_deref().map(Into::into),
        edge: declaration.edge.as_ref().map(edge),
        cardinality: match declaration.cardinality {
            reference::Cardinality::One => Cardinality::One,
            reference::Cardinality::Many => Cardinality::Many,
        },
        nullable: declaration.nullable,
        on_unresolved: match declaration.on_unresolved {
            reference::OnUnresolved::Error => OnUnresolved::Error,
            reference::OnUnresolved::Complement => OnUnresolved::Complement,
        },
        mode: match declaration.mode {
            reference::ResolveMode::AddEdge => ResolveMode::AddEdge,
            reference::ResolveMode::Redirect => ResolveMode::Redirect,
        },
        guard: declaration.guard.as_ref().map(guard),
        null_match: match declaration.null_match {
            reference::NullMatch::Simple => NullMatch::Simple,
            reference::NullMatch::Full => NullMatch::Full,
        },
        null_vertex: declaration.null_vertex.as_deref().map(Into::into),
        scope_via: declaration.scope_via.as_ref().map(|via| ScopeVia {
            ancestor: via.ancestor.as_str().into(),
            hops: via.hops.iter().map(|hop| hop.as_str().into()).collect(),
        }),
    }
}

fn production_schema(schema: &reference::Schema) -> Schema {
    let vertices: HashMap<_, _> = schema
        .vertices
        .iter()
        .map(|(id, kind)| {
            let id: Name = id.as_str().into();
            (
                id.clone(),
                Vertex {
                    id,
                    kind: kind.as_str().into(),
                    nsid: None,
                },
            )
        })
        .collect();
    let edges: HashMap<_, _> = schema
        .edges
        .iter()
        .map(|reference_edge| {
            let production_edge = edge(reference_edge);
            (production_edge, reference_edge.kind.as_str().into())
        })
        .collect();
    let mut production = Schema {
        protocol: "o17-keys".into(),
        vertices,
        edges,
        entries: schema
            .entries
            .iter()
            .map(|entry| entry.as_str().into())
            .collect(),
        orderings: schema
            .orderings
            .iter()
            .map(|(reference_edge, position)| (edge(reference_edge), *position))
            .collect(),
        ..Schema::default()
    };
    production.declarations.keys = schema
        .keys
        .iter()
        .map(|(vertex, keys)| {
            (
                vertex.as_str().into(),
                keys.iter().map(key).collect::<Vec<_>>(),
            )
        })
        .collect();
    production.declarations.references = schema
        .references
        .iter()
        .map(|(id, declaration)| (id.as_str().into(), reference(declaration)))
        .collect();
    production.declarations.target_sets = schema
        .target_sets
        .iter()
        .map(|(id, set)| {
            (
                id.as_str().into(),
                TargetSet {
                    members: set
                        .members
                        .iter()
                        .map(|(vertex, key)| (vertex.as_str().into(), key.as_str().into()))
                        .collect(),
                    scope: key_scope(&set.scope),
                },
            )
        })
        .collect();
    production
}

fn production_change(before: &reference::Schema, after: &reference::Schema) -> reference::Change {
    let before = production_schema(before);
    let after = production_schema(after);
    let delta = diff(&before, &after);
    if delta.is_empty() {
        return reference::Change::Identical;
    }
    let report = classify(&delta, &protocol());
    if report.breaking.is_empty() {
        reference::Change::BackwardCompatible
    } else {
        reference::Change::Breaking
    }
}

fn contextual_schemas(
    graph: &reference::Schema,
    slot: usize,
    transition: &enumerate::AxisTransition,
    other: Option<&enumerate::KeyDescriptor>,
) -> (reference::Schema, reference::Schema) {
    let other_slot = 1 - slot;
    let mut before = [None, None];
    let mut after = [None, None];
    before[slot] = transition.before.as_ref();
    after[slot] = transition.after.as_ref();
    before[other_slot] = other;
    after[other_slot] = other;
    (
        enumerate::schema_with_keys(graph.clone(), before),
        enumerate::schema_with_keys(graph.clone(), after),
    )
}

fn assert_agree(before: &reference::Schema, after: &reference::Schema) {
    assert_eq!(
        production_change(before, after),
        reference::classify_change(before, after)
    );
}

fn reference_edge(src: &str, tgt: &str, name: &str) -> panproto_reference::model::SEdge {
    panproto_reference::model::SEdge::new(src, tgt, "prop", Some(name))
}

fn classification_base() -> reference::Schema {
    let mut schema = reference::Schema::default()
        .vertex("holder", "object")
        .vertex("target", "object")
        .vertex("other", "object")
        .vertex("scalar", "scalar")
        .edge("holder", "scalar", "prop", Some("field"))
        .edge("holder", "target", "prop", Some("resolved"))
        .edge("holder", "other", "prop", Some("resolved-other"));
    let declared_key = |id: &str| reference::Key {
        id: id.to_owned(),
        parts: vec![reference::KeyPart::Path(vec![])],
        role: reference::KeyRole::Primary,
        scope: reference::KeyScope::Document,
        equality: vec![],
    };
    schema.keys.insert(
        "target".to_owned(),
        vec![declared_key("id"), declared_key("id2")],
    );
    schema
        .keys
        .insert("other".to_owned(), vec![declared_key("id")]);
    schema.references.insert(
        "r".to_owned(),
        reference::Reference {
            id: "r".to_owned(),
            holder: "holder".to_owned(),
            source: reference::RefSource::KeyValue {
                parts: vec![vec![reference_edge("holder", "scalar", "field")]],
            },
            target_vertex: "target".to_owned(),
            target_key: Some("id".to_owned()),
            edge: Some(reference_edge("holder", "target", "resolved")),
            cardinality: reference::Cardinality::One,
            nullable: false,
            on_unresolved: reference::OnUnresolved::Error,
            mode: reference::ResolveMode::AddEdge,
            guard: None,
            null_match: reference::NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        },
    );
    schema
}

fn target_set(members: &[(&str, &str)], scope: reference::KeyScope) -> reference::TargetSet {
    reference::TargetSet {
        members: members
            .iter()
            .map(|(vertex, key)| ((*vertex).to_owned(), (*key).to_owned()))
            .collect(),
        scope,
    }
}

type ReferenceMutation = fn(&mut reference::Reference);

#[test]
#[allow(clippy::too_many_lines)]
fn non_key_classification_axes_match_the_reference() {
    use reference::Change::{BackwardCompatible, Breaking, Identical};

    let base = classification_base();
    let mut cases = vec![("identical", base.clone(), base.clone(), Identical)];

    let mut equality = base.clone();
    equality.keys.get_mut("target").expect("target keys")[0].equality =
        vec![reference_keys::KeyEquality::Representation];
    cases.push(("key equality", base.clone(), equality, Breaking));

    let mut without_reference = base.clone();
    without_reference.references.clear();
    cases.push((
        "strict reference added",
        without_reference.clone(),
        base.clone(),
        Breaking,
    ));
    cases.push((
        "strict reference removed",
        base.clone(),
        without_reference.clone(),
        BackwardCompatible,
    ));
    let mut relaxed_addition = base.clone();
    let relaxed = relaxed_addition.references.get_mut("r").expect("reference");
    relaxed.nullable = true;
    relaxed.on_unresolved = reference::OnUnresolved::Complement;
    cases.push((
        "relaxed reference added",
        without_reference.clone(),
        relaxed_addition,
        BackwardCompatible,
    ));

    let directional_axes: [(&str, ReferenceMutation); 3] = [
        ("cardinality", |declaration: &mut reference::Reference| {
            declaration.cardinality = reference::Cardinality::Many;
        }),
        ("nullability", |declaration: &mut reference::Reference| {
            declaration.nullable = true;
        }),
        (
            "unresolved policy",
            |declaration: &mut reference::Reference| {
                declaration.on_unresolved = reference::OnUnresolved::Complement;
            },
        ),
    ];
    for (name, relax) in directional_axes {
        let strict = base.clone();
        let mut loose = base.clone();
        relax(loose.references.get_mut("r").expect("reference"));
        cases.push((name, loose.clone(), strict, Breaking));
        cases.push((name, base.clone(), loose, BackwardCompatible));
    }

    for axis in 0..10 {
        let mut changed = base.clone();
        let declaration = changed.references.get_mut("r").expect("reference");
        match axis {
            0 => declaration.holder = "other".to_owned(),
            1 => {
                declaration.source = reference::RefSource::Inline {
                    field: vec![reference_edge("holder", "scalar", "field")],
                };
            }
            2 => declaration.target_vertex = "other".to_owned(),
            3 => declaration.target_key = Some("id2".to_owned()),
            4 => {
                declaration.edge = Some(reference_edge("holder", "other", "resolved-other"));
            }
            5 => declaration.mode = reference::ResolveMode::Redirect,
            6 => declaration.id = "inner-r".to_owned(),
            7 => {
                declaration.guard = Some(reference::RefGuard {
                    field: vec![reference_edge("holder", "scalar", "field")],
                    equals: panproto_reference::value::Value::Str("guard".to_owned()),
                });
            }
            8 => declaration.null_match = reference::NullMatch::Full,
            9 => declaration.null_vertex = Some("scalar".to_owned()),
            _ => unreachable!(),
        }
        cases.push(("reference semantic field", base.clone(), changed, Breaking));
    }
    let mut scope_via = base.clone();
    scope_via
        .references
        .get_mut("r")
        .expect("reference")
        .scope_via = Some(reference::ScopeVia {
        ancestor: "holder".to_owned(),
        hops: vec!["r".to_owned()],
    });
    cases.push(("scope via", base.clone(), scope_via, Breaking));

    let mut no_sets = base.clone();
    no_sets.references.clear();
    let mut one_set = no_sets.clone();
    one_set.target_sets.insert(
        "targets".to_owned(),
        target_set(&[("target", "id")], reference::KeyScope::Document),
    );
    cases.push((
        "target set added",
        no_sets.clone(),
        one_set.clone(),
        Breaking,
    ));
    cases.push((
        "target set removed",
        one_set.clone(),
        no_sets.clone(),
        BackwardCompatible,
    ));
    let mut two_members = one_set.clone();
    two_members
        .target_sets
        .get_mut("targets")
        .expect("target set")
        .members
        .push(("other".to_owned(), "id".to_owned()));
    cases.push((
        "target-set member added",
        one_set.clone(),
        two_members.clone(),
        Breaking,
    ));
    cases.push((
        "target-set member removed",
        two_members,
        one_set.clone(),
        BackwardCompatible,
    ));
    let mut global_set = one_set.clone();
    global_set
        .target_sets
        .get_mut("targets")
        .expect("target set")
        .scope = reference::KeyScope::Global;
    cases.push(("target-set scope", one_set, global_set, Breaking));

    let mut structural = base.clone();
    structural.entries.push("holder".to_owned());
    cases.push(("entry change fails closed", base, structural, Breaking));

    assert_eq!(cases.len(), 28);
    for (name, before, after, expected) in cases {
        assert_eq!(
            reference::classify_change(&before, &after),
            expected,
            "reference: {name}"
        );
        assert_eq!(
            production_change(&before, &after),
            expected,
            "production: {name}"
        );
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_key_change_classifies_through_the_factorization_certificate() {
    assert_eq!(enumerate::classification_count(), 816_223_680);

    for vertices in 1..=3 {
        let graphs = enumerate::structural_schemas(vertices);
        let transitions = enumerate::axis_transitions(vertices);
        let representative = &transitions[0];

        for graph in &graphs {
            for slot in 0..2 {
                let (before, after) = contextual_schemas(graph, slot, representative, None);
                assert_agree(&before, &after);
            }
        }

        let graph = &graphs[0];
        for transition in &transitions {
            for slot in 0..2 {
                let (before, after) = contextual_schemas(graph, slot, transition, None);
                assert_agree(&before, &after);
            }
        }
    }

    let classes = enumerate::factor_classes();
    let mut represented = 0_u64;
    for class in &classes {
        let graphs = enumerate::structural_schemas(class.vertices);
        let transitions = enumerate::axis_transitions(class.vertices);
        let descriptors = enumerate::key_descriptors(class.vertices);
        let graph = &graphs[class.representative_graph];
        let transition = &transitions[class.representative_transition];
        let other = class.representative_other.map(|index| &descriptors[index]);
        let (before, after) = contextual_schemas(graph, class.slot, transition, other);
        assert_agree(&before, &after);
        represented += class.multiplicity;
    }
    assert_eq!(represented, 816_223_680);

    println!(
        "classification certificate: 148 structural graphs; every directed descriptor-axis change; 816,223,680 full-carrier classifications"
    );
}

fn ordered_key_schema(roles: &[reference::KeyRole]) -> reference::Schema {
    let mut schema = reference::Schema::default().vertex("item", "object");
    schema.keys.insert(
        "item".to_owned(),
        roles
            .iter()
            .enumerate()
            .map(|(index, role)| reference::Key {
                id: format!("k{index}"),
                parts: vec![],
                role: *role,
                scope: reference::KeyScope::Document,
                equality: vec![],
            })
            .collect(),
    );
    schema
}

const fn key_roles() -> [reference::KeyRole; 5] {
    use reference::KeyRole::{Identifier, Index, Primary, SortKey, Unique};
    [Primary, Unique, Identifier, SortKey, Index]
}

#[test]
fn stored_key_order_and_primary_removal_match_reference() {
    use reference::Change::{BackwardCompatible, Breaking};
    for first in key_roles() {
        for second in key_roles() {
            let before = ordered_key_schema(&[first, second]);
            let mut reversed = before.clone();
            reversed.keys.get_mut("item").expect("owner").reverse();
            let expected =
                if first == reference::KeyRole::Primary && second == reference::KeyRole::Primary {
                    Breaking
                } else {
                    BackwardCompatible
                };
            assert_eq!(reference::classify_change(&before, &reversed), expected);
            assert_eq!(production_change(&before, &reversed), expected);
            let delta = diff(&production_schema(&before), &production_schema(&reversed));
            assert_eq!(delta.key_sequence_changes.len(), 1);
            assert_eq!(
                delta.key_sequence_changes[0].old,
                Some(vec![Name::from("k0"), Name::from("k1")])
            );
            assert_eq!(
                delta.key_sequence_changes[0].new,
                Some(vec![Name::from("k1"), Name::from("k0")])
            );
            assert_eq!(
                delta.primary_key_changes.len(),
                usize::from(expected == Breaking)
            );
            for index in 0..2 {
                let mut removed = before.clone();
                let old = removed.keys.get_mut("item").expect("owner").remove(index);
                let expected = if old.role == reference::KeyRole::Primary {
                    Breaking
                } else {
                    BackwardCompatible
                };
                assert_eq!(reference::classify_change(&before, &removed), expected);
                assert_eq!(production_change(&before, &removed), expected);
            }
        }
    }
}

#[test]
fn key_owner_presence_and_role_selection_are_recorded() {
    let absent = reference::Schema::default().vertex("item", "object");
    let empty = ordered_key_schema(&[]);
    for (before, after) in [(&absent, &empty), (&empty, &absent)] {
        assert_eq!(
            reference::classify_change(before, after),
            reference::Change::BackwardCompatible
        );
        assert_eq!(
            production_change(before, after),
            reference::Change::BackwardCompatible
        );
        let delta = diff(&production_schema(before), &production_schema(after));
        assert_eq!(delta.key_sequence_changes.len(), 1);
        assert_eq!(
            delta.key_sequence_changes[0].old,
            before.keys.get("item").map(|_| vec![])
        );
        assert_eq!(
            delta.key_sequence_changes[0].new,
            after.keys.get("item").map(|_| vec![])
        );
        let report = classify(&delta, &protocol());
        assert_eq!(report.non_breaking.len(), 1);
        assert_eq!(
            panproto_check::report_json(&report)["non_breaking"],
            serde_json::json!([{
                "type": "key_sequence_changed", "vertex_id": "item",
                "old": delta.key_sequence_changes[0].old,
                "new": delta.key_sequence_changes[0].new,
            }])
        );
        assert!(panproto_check::report_text(&report).contains("Stored key sequence changed: item"));
        assert!(matches!(
            &report.non_breaking[0],
            panproto_check::NonBreakingChange::KeySequenceChanged { .. }
        ));
        assert_eq!(
            serde_json::from_str::<panproto_check::SchemaDiff>(
                &serde_json::to_string(&delta).expect("encode")
            )
            .expect("decode"),
            delta
        );
    }
    let before = ordered_key_schema(&[reference::KeyRole::Primary, reference::KeyRole::Primary]);
    let mut after = before.clone();
    after.keys.get_mut("item").expect("owner")[0].role = reference::KeyRole::Index;
    assert_agree(&before, &after);
    let delta = diff(&production_schema(&before), &production_schema(&after));
    assert_eq!(delta.key_sequence_changes.len(), 0);
    assert_eq!(
        delta.primary_key_changes,
        vec![panproto_check::diff::PrimaryKeyChange {
            vertex_id: "item".to_owned(),
            old: Some("k0".into()),
            new: Some("k1".into()),
        }]
    );
    assert_eq!(classify(&delta, &protocol()).breaking.len(), 1);
}

#[test]
fn isolated_primary_selection_metadata_is_classified_and_rendered() {
    let delta = panproto_check::SchemaDiff {
        primary_key_changes: vec![panproto_check::diff::PrimaryKeyChange {
            vertex_id: "item".to_owned(),
            old: Some("k0".into()),
            new: Some("k1".into()),
        }],
        ..panproto_check::SchemaDiff::default()
    };
    assert!(
        !delta.is_empty(),
        "primary selection is a structural change"
    );
    let report = classify(&delta, &protocol());
    assert_eq!(
        report.breaking,
        vec![panproto_check::BreakingChange::PrimaryKeySelectionChanged {
            vertex_id: "item".to_owned(),
            old_key_id: "k0".to_owned(),
            new_key_id: Some("k1".to_owned()),
        }]
    );
    assert_eq!(
        panproto_check::report_json(&report)["breaking"],
        serde_json::json!([{
            "type": "primary_key_selection_changed", "vertex_id": "item",
            "old_key_id": "k0", "new_key_id": "k1",
        }])
    );
    assert!(
        panproto_check::report_text(&report).contains("Primary key selection changed: item k0")
    );
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::default())]

    #[test]
    fn randomized_key_change_classification(
        vertices in 1_usize..=3,
        graph_pick in proptest::prelude::any::<usize>(),
        transition_pick in proptest::prelude::any::<usize>(),
        other_pick in proptest::option::of(proptest::prelude::any::<usize>()),
        slot in 0_usize..2,
    ) {
        let graphs = enumerate::structural_schemas(vertices);
        let transitions = enumerate::axis_transitions(vertices);
        let descriptors = enumerate::key_descriptors(vertices);
        let graph = &graphs[graph_pick % graphs.len()];
        let transition = &transitions[transition_pick % transitions.len()];
        let other = other_pick.map(|index| &descriptors[index % descriptors.len()]);
        let (before, after) = contextual_schemas(graph, slot, transition, other);
        proptest::prop_assert_eq!(production_change(&before, &after), reference::classify_change(&before, &after));
    }

    #[test]
    fn randomized_stored_key_sequence_classification(
        first in 0_usize..5, second in 0_usize..5,
        reverse in proptest::prelude::any::<bool>(),
        removal in proptest::option::of(0_usize..2),
    ) {
        let roles = key_roles();
        let before = ordered_key_schema(&[roles[first], roles[second]]);
        let mut after = before.clone();
        let keys = after.keys.get_mut("item").expect("owner");
        if reverse { keys.reverse(); }
        if let Some(index) = removal { keys.remove(index); }
        proptest::prop_assert_eq!(production_change(&before, &after), reference::classify_change(&before, &after));
    }

    #[test]
    fn randomized_reference_change_classification(
        nullable in proptest::prelude::any::<bool>(),
        many in proptest::prelude::any::<bool>(),
        complement in proptest::prelude::any::<bool>(),
        remove in proptest::prelude::any::<bool>(),
        reverse in proptest::prelude::any::<bool>(),
    ) {
        let before = classification_base();
        let mut after = before.clone();
        if remove { after.references.clear(); } else {
            let declaration = after.references.get_mut("r").expect("base reference");
            declaration.nullable = nullable;
            declaration.cardinality = if many { reference::Cardinality::Many } else { reference::Cardinality::One };
            declaration.on_unresolved = if complement { reference::OnUnresolved::Complement } else { reference::OnUnresolved::Error };
        }
        let (before, after) = if reverse { (after, before) } else { (before, after) };
        proptest::prop_assert_eq!(production_change(&before, &after), reference::classify_change(&before, &after));
    }
}
