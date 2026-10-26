//! Independent finite certificate for the tree-complement partial monoid.
//!
//! The reference carrier is the powerset of three canonical resource
//! bundles. Its operation is bitwise union, not the production merge. Every
//! bundle populates every complement coordinate, so the complete ordered
//! triple census checks identity, idempotence, commutativity and associativity
//! against an independently computed normal form. Named cases cover every
//! coordinate on which the partial operation may refuse a pair.

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::unwrap_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::identity::{
    ComplementShape, IdentityCarrier, IdentityError, LegacyRejectReason,
};
use panproto_inst::{
    CompiledMigration, Complement, EdgeId, Fan, FieldPresence, Node, SynthKey, Value,
};
use panproto_lens::{ComplementCompose, Lens, LensError};
use panproto_reference::enumerate::lenses::{
    ComplementMonoidBinding, ComplementMonoidCase, ComplementMonoidRefusal,
    complement_monoid_count_certificate, for_each_complement_monoid_case,
};
use panproto_reference::lens::{ComplementComposeRefusal, ComplementModel, compose_complement};
use panproto_schema::{Edge, Protocol, SchemaBuilder};
use proptest::prelude::*;

const LEGACY_FINGERPRINT: u64 = 0xC011_EC70;
const VERIFIED_IDENTITY: u32 = 1;
const OTHER_VERIFIED_IDENTITY: u32 = 2;

fn edge(atom: u32, tag: &str) -> Edge {
    Edge {
        src: format!("s{atom}").into(),
        tgt: format!("t{atom}").into(),
        kind: format!("kind-{tag}").into(),
        name: Some(format!("name-{atom}").into()),
    }
}

fn identity_lens(vertex: &str) -> Lens {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex(vertex, "record", None)
        .expect("one vertex")
        .build()
        .expect("one-vertex schema");
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    }
}

fn populate_model_atom(complement: &mut ComplementModel, atom: u32) {
    let key = atom + 1;
    let child = key + 10;
    let parent = key + 100;

    complement.dropped_nodes.insert(key, key);
    complement.dropped_arcs.insert(key);
    complement.dropped_fans.insert(key);
    complement.contraction_choices.insert((key, child), key);
    complement.original_parent.insert(key, parent);
    complement.original_extra_fields.insert(key, key);
    complement.arc_edges.insert((key, child), key);
    complement.original_values.insert(key, key);
    complement.synthesized_nodes.insert(key);
    complement.contracted_into.insert(key, parent);
    complement.original_anchors.insert(key, key);
    complement.synthesized_keys.insert(key, key);
    complement.id_floor = complement.id_floor.max(parent + 1);
}

fn model_for(binding: ComplementMonoidBinding, mask: u8) -> ComplementModel {
    let mut complement = ComplementModel::default();
    for atom in 0_u32..3 {
        if mask & (1 << atom) != 0 {
            populate_model_atom(&mut complement, atom);
        }
    }
    if mask != 0 {
        // Arc order is a single global resource rather than a keyed map. A
        // common nonempty value exercises agreement; mask zero exercises its
        // unit. A separate refusal case supplies a disagreeing order.
        complement.arc_order = vec![(1, 11), (2, 12), (3, 13)];
    }
    complement.source_fingerprint = LEGACY_FINGERPRINT;
    if binding == ComplementMonoidBinding::Verified {
        complement.identity = Some(VERIFIED_IDENTITY);
    }
    complement
}

fn materialize_model(model: &ComplementModel, lens: &Lens) -> Complement {
    let mut complement = Complement::empty();
    for (&id, &atom) in &model.dropped_nodes {
        complement.dropped_nodes.insert(
            id,
            Node::new(id, format!("node-{atom}"))
                .with_value(FieldPresence::Present(Value::Int(i64::from(atom)))),
        );
    }
    for &atom in &model.dropped_arcs {
        complement
            .dropped_arcs
            .push((atom, atom + 10, edge(atom, "canonical")));
    }
    for &atom in &model.dropped_fans {
        complement.dropped_fans.push(
            Fan::new(format!("fan-{atom}"), atom).with_child(format!("slot-{atom}"), atom + 10),
        );
    }
    for (&endpoints, &atom) in &model.contraction_choices {
        complement
            .contraction_choices
            .insert(endpoints, edge(atom, "canonical"));
    }
    complement.original_parent = model.original_parent.clone().into_iter().collect();
    for (&id, &atom) in &model.original_extra_fields {
        complement.original_extra_fields.insert(
            id,
            HashMap::from([("payload".to_owned(), Value::Int(i64::from(atom)))]),
        );
    }
    for (&endpoints, &atom) in &model.arc_edges {
        complement
            .arc_edges
            .insert(endpoints, edge(atom, "canonical"));
    }
    complement.arc_order.clone_from(&model.arc_order);
    for (&id, &atom) in &model.original_values {
        complement.original_values.insert(
            id,
            Some(FieldPresence::Present(Value::Str(format!("value-{atom}")))),
        );
    }
    complement
        .synthesized_nodes
        .extend(model.synthesized_nodes.iter().copied());
    complement.contracted_into = model.contracted_into.clone().into_iter().collect();
    for (&id, &atom) in &model.original_anchors {
        complement
            .original_anchors
            .insert(id, Name::from(format!("anchor-{atom}")));
    }
    complement.id_floor = model.id_floor;
    for (&id, &atom) in &model.synthesized_keys {
        complement.synthesized_keys.insert(
            id,
            SynthKey::Expand {
                source_edge: EdgeId(atom),
                hop: u16::try_from(atom - 1).expect("canonical atoms are positive"),
            },
        );
    }
    if model.identity.is_some() {
        complement.stamp(lens.identity(ComplementShape::WType), &lens.src_schema);
    } else {
        complement.source_fingerprint = model.source_fingerprint;
    }
    complement
}

fn complement_for(binding: ComplementMonoidBinding, mask: u8, lens: &Lens) -> Complement {
    materialize_model(&model_for(binding, mask), lens)
}

/// Equality in the complement algebra quotients the two set-valued vector
/// coordinates by presentation order. All map serializers are canonical.
fn normal_form(mut complement: Complement) -> serde_json::Value {
    complement
        .dropped_arcs
        .sort_by_key(|(parent, child, edge)| (*parent, *child, format!("{edge:?}")));
    complement.dropped_arcs.dedup();
    complement.dropped_fans.sort_by_key(|fan| {
        (
            fan.parent,
            fan.hyper_edge_id.clone(),
            format!("{:?}", fan.children),
        )
    });
    complement.dropped_fans.dedup();
    serde_json::to_value(complement).expect("complement serializes")
}

fn assert_equivalent(actual: Complement, expected: Complement, context: &str) {
    assert_eq!(normal_form(actual), normal_form(expected), "{context}");
}

fn legacy_pair() -> (Complement, Complement) {
    let mut left = Complement::empty();
    let mut right = Complement::empty();
    left.source_fingerprint = LEGACY_FINGERPRINT;
    right.source_fingerprint = LEGACY_FINGERPRINT;
    (left, right)
}

fn refusal_pair(
    refusal: ComplementMonoidRefusal,
    verified: &Lens,
    other_verified: &Lens,
) -> (Complement, Complement) {
    let (mut left, mut right) = legacy_pair();
    match refusal {
        ComplementMonoidRefusal::DroppedNodes => {
            left.dropped_nodes.insert(1, Node::new(1, "left"));
            right.dropped_nodes.insert(1, Node::new(1, "right"));
        }
        ComplementMonoidRefusal::ContractionChoices => {
            left.contraction_choices.insert((1, 2), edge(1, "left"));
            right.contraction_choices.insert((1, 2), edge(1, "right"));
        }
        ComplementMonoidRefusal::OriginalParent => {
            left.original_parent.insert(1, 2);
            right.original_parent.insert(1, 3);
        }
        ComplementMonoidRefusal::OriginalExtraFields => {
            left.original_extra_fields
                .insert(1, HashMap::from([("x".to_owned(), Value::Int(1))]));
            right
                .original_extra_fields
                .insert(1, HashMap::from([("x".to_owned(), Value::Int(2))]));
        }
        ComplementMonoidRefusal::ArcEdges => {
            left.arc_edges.insert((1, 2), edge(1, "left"));
            right.arc_edges.insert((1, 2), edge(1, "right"));
        }
        ComplementMonoidRefusal::OriginalValues => {
            left.original_values
                .insert(1, Some(FieldPresence::Present(Value::Int(1))));
            right
                .original_values
                .insert(1, Some(FieldPresence::Present(Value::Int(2))));
        }
        ComplementMonoidRefusal::ContractedInto => {
            left.contracted_into.insert(1, 2);
            right.contracted_into.insert(1, 3);
        }
        ComplementMonoidRefusal::OriginalAnchors => {
            left.original_anchors.insert(1, "left".into());
            right.original_anchors.insert(1, "right".into());
        }
        ComplementMonoidRefusal::SynthesizedKeyById => {
            left.synthesized_keys.insert(
                1,
                SynthKey::Expand {
                    source_edge: EdgeId(1),
                    hop: 0,
                },
            );
            right.synthesized_keys.insert(
                1,
                SynthKey::Expand {
                    source_edge: EdgeId(2),
                    hop: 0,
                },
            );
        }
        ComplementMonoidRefusal::SynthesizedIdByKey => {
            let key = SynthKey::Expand {
                source_edge: EdgeId(1),
                hop: 0,
            };
            left.synthesized_keys.insert(1, key.clone());
            right.synthesized_keys.insert(2, key);
        }
        ComplementMonoidRefusal::ArcOrder => {
            left.arc_order = vec![(1, 2), (1, 3)];
            right.arc_order = vec![(1, 3), (1, 2)];
        }
        ComplementMonoidRefusal::UnboundData => {
            left = Complement::empty();
            left.dropped_nodes.insert(1, Node::new(1, "unbound"));
            right = Complement::empty();
        }
        ComplementMonoidRefusal::LegacyFingerprint => {
            right.source_fingerprint = LEGACY_FINGERPRINT + 1;
        }
        ComplementMonoidRefusal::MixedProvenance => {
            left = Complement::empty();
            left.stamp(
                verified.identity(ComplementShape::WType),
                &verified.src_schema,
            );
        }
        ComplementMonoidRefusal::EnvelopeMismatch => {
            left = Complement::empty();
            right = Complement::empty();
            left.stamp(
                verified.identity(ComplementShape::WType),
                &verified.src_schema,
            );
            right.stamp(
                other_verified.identity(ComplementShape::WType),
                &other_verified.src_schema,
            );
        }
    }
    (left, right)
}

fn reference_legacy_pair() -> (ComplementModel, ComplementModel) {
    let mut left = ComplementModel::default();
    let mut right = ComplementModel::default();
    left.source_fingerprint = LEGACY_FINGERPRINT;
    right.source_fingerprint = LEGACY_FINGERPRINT;
    (left, right)
}

fn reference_refusal_pair(refusal: ComplementMonoidRefusal) -> (ComplementModel, ComplementModel) {
    let (mut left, mut right) = reference_legacy_pair();
    match refusal {
        ComplementMonoidRefusal::DroppedNodes => {
            left.dropped_nodes.insert(1, 1);
            right.dropped_nodes.insert(1, 2);
        }
        ComplementMonoidRefusal::ContractionChoices => {
            left.contraction_choices.insert((1, 2), 1);
            right.contraction_choices.insert((1, 2), 2);
        }
        ComplementMonoidRefusal::OriginalParent => {
            left.original_parent.insert(1, 2);
            right.original_parent.insert(1, 3);
        }
        ComplementMonoidRefusal::OriginalExtraFields => {
            left.original_extra_fields.insert(1, 1);
            right.original_extra_fields.insert(1, 2);
        }
        ComplementMonoidRefusal::ArcEdges => {
            left.arc_edges.insert((1, 2), 1);
            right.arc_edges.insert((1, 2), 2);
        }
        ComplementMonoidRefusal::OriginalValues => {
            left.original_values.insert(1, 1);
            right.original_values.insert(1, 2);
        }
        ComplementMonoidRefusal::ContractedInto => {
            left.contracted_into.insert(1, 2);
            right.contracted_into.insert(1, 3);
        }
        ComplementMonoidRefusal::OriginalAnchors => {
            left.original_anchors.insert(1, 1);
            right.original_anchors.insert(1, 2);
        }
        ComplementMonoidRefusal::SynthesizedKeyById => {
            left.synthesized_keys.insert(1, 1);
            right.synthesized_keys.insert(1, 2);
        }
        ComplementMonoidRefusal::SynthesizedIdByKey => {
            left.synthesized_keys.insert(1, 1);
            right.synthesized_keys.insert(2, 1);
        }
        ComplementMonoidRefusal::ArcOrder => {
            left.arc_order = vec![(1, 2), (1, 3)];
            right.arc_order = vec![(1, 3), (1, 2)];
        }
        ComplementMonoidRefusal::UnboundData => {
            left = ComplementModel::default();
            left.dropped_nodes.insert(1, 1);
            right = ComplementModel::default();
        }
        ComplementMonoidRefusal::LegacyFingerprint => {
            right.source_fingerprint = LEGACY_FINGERPRINT + 1;
        }
        ComplementMonoidRefusal::MixedProvenance => {
            left.identity = Some(VERIFIED_IDENTITY);
        }
        ComplementMonoidRefusal::EnvelopeMismatch => {
            left.identity = Some(VERIFIED_IDENTITY);
            right.identity = Some(OTHER_VERIFIED_IDENTITY);
        }
    }
    (left, right)
}

const fn reference_refusal(refusal: ComplementMonoidRefusal) -> ComplementComposeRefusal {
    match refusal {
        ComplementMonoidRefusal::DroppedNodes => ComplementComposeRefusal::DroppedNodes,
        ComplementMonoidRefusal::ContractionChoices => ComplementComposeRefusal::ContractionChoices,
        ComplementMonoidRefusal::OriginalParent => ComplementComposeRefusal::OriginalParent,
        ComplementMonoidRefusal::OriginalExtraFields => {
            ComplementComposeRefusal::OriginalExtraFields
        }
        ComplementMonoidRefusal::ArcEdges => ComplementComposeRefusal::ArcEdges,
        ComplementMonoidRefusal::OriginalValues => ComplementComposeRefusal::OriginalValues,
        ComplementMonoidRefusal::ContractedInto => ComplementComposeRefusal::ContractedInto,
        ComplementMonoidRefusal::OriginalAnchors => ComplementComposeRefusal::OriginalAnchors,
        ComplementMonoidRefusal::SynthesizedKeyById => ComplementComposeRefusal::SynthesizedKeyById,
        ComplementMonoidRefusal::SynthesizedIdByKey => ComplementComposeRefusal::SynthesizedIdByKey,
        ComplementMonoidRefusal::ArcOrder => ComplementComposeRefusal::ArcOrder,
        ComplementMonoidRefusal::UnboundData => ComplementComposeRefusal::UnboundData,
        ComplementMonoidRefusal::LegacyFingerprint => ComplementComposeRefusal::LegacyFingerprint,
        ComplementMonoidRefusal::MixedProvenance => ComplementComposeRefusal::MixedProvenance,
        ComplementMonoidRefusal::EnvelopeMismatch => ComplementComposeRefusal::EnvelopeMismatch,
    }
}

fn assert_conflict(error: LensError, expected_kind: &'static str, expected_key: &str) {
    let LensError::ComplementConflict { kind, key } = error else {
        panic!("expected {expected_kind} conflict on {expected_key}, got {error:?}");
    };
    assert_eq!(kind, expected_kind);
    assert_eq!(key, expected_key);
}

fn assert_production_refusal(
    refusal: ComplementMonoidRefusal,
    error: LensError,
    first: &Complement,
    second: &Complement,
) {
    match refusal {
        ComplementMonoidRefusal::DroppedNodes => {
            assert_conflict(error, "dropped_nodes", "1");
        }
        ComplementMonoidRefusal::ContractionChoices => {
            assert_conflict(error, "contraction_choices", "(1, 2)");
        }
        ComplementMonoidRefusal::OriginalParent => {
            assert_conflict(error, "original_parent", "1");
        }
        ComplementMonoidRefusal::OriginalExtraFields => {
            assert_conflict(error, "original_extra_fields", "1");
        }
        ComplementMonoidRefusal::ArcEdges => {
            assert_conflict(error, "arc_edges", "(1, 2)");
        }
        ComplementMonoidRefusal::OriginalValues => {
            assert_conflict(error, "original_values", "1");
        }
        ComplementMonoidRefusal::ContractedInto => {
            assert_conflict(error, "contracted_into", "1");
        }
        ComplementMonoidRefusal::OriginalAnchors => {
            assert_conflict(error, "original_anchors", "1");
        }
        ComplementMonoidRefusal::SynthesizedKeyById => {
            let (&id, incoming) = second
                .synthesized_keys
                .iter()
                .find(|(id, key)| {
                    first
                        .synthesized_keys
                        .get(id)
                        .is_some_and(|mine| mine != *key)
                })
                .expect("one incoming id carries a distinct key");
            assert_conflict(error, "synthesized_keys", &format!("{id:?} {incoming:?}"));
        }
        ComplementMonoidRefusal::SynthesizedIdByKey => {
            let (&id, incoming) = second
                .synthesized_keys
                .iter()
                .find(|(id, key)| {
                    first
                        .synthesized_keys
                        .iter()
                        .any(|(mine, mine_key)| mine != *id && mine_key == *key)
                })
                .expect("one incoming id reuses an existing key");
            assert_conflict(error, "synthesized_keys", &format!("{id:?} {incoming:?}"));
        }
        ComplementMonoidRefusal::ArcOrder => {
            assert_conflict(error, "arc_order", "ordered sequence");
        }
        ComplementMonoidRefusal::UnboundData => {
            assert!(matches!(
                error,
                LensError::Identity(IdentityError::UnboundComplement)
            ));
        }
        ComplementMonoidRefusal::LegacyFingerprint => {
            assert!(matches!(
                error,
                LensError::Identity(IdentityError::LegacyComplementRejected {
                    reason: LegacyRejectReason::FingerprintMismatch,
                })
            ));
        }
        ComplementMonoidRefusal::MixedProvenance => {
            assert!(matches!(
                error,
                LensError::Identity(IdentityError::LegacyComplementRejected {
                    reason: LegacyRejectReason::MixedWithVerified,
                })
            ));
        }
        ComplementMonoidRefusal::EnvelopeMismatch => {
            let expected = first
                .identity
                .as_ref()
                .expect("first envelope")
                .mismatch(second.identity.as_ref().expect("second envelope"))
                .expect("the envelopes differ");
            let LensError::Identity(actual) = error else {
                panic!("expected envelope mismatch {expected:?}, got {error:?}");
            };
            assert_eq!(actual, expected);
        }
    }
}

fn assert_refusal_step(
    refusal: ComplementMonoidRefusal,
    reference_left: &ComplementModel,
    reference_right: &ComplementModel,
    left: &Complement,
    right: &Complement,
) {
    assert_eq!(
        compose_complement(reference_left, reference_right)
            .expect_err("combined reference pair is off-domain"),
        reference_refusal(refusal),
    );
    let error = left
        .compose(right)
        .expect_err("combined production pair is off-domain");
    assert_production_refusal(refusal, error, left, right);
    assert!(!left.is_compatible(right));
}

fn assert_reference_carrier_rejects_internal_synthesis_duplicates_symmetrically() {
    let mut malformed = ComplementModel {
        source_fingerprint: LEGACY_FINGERPRINT,
        ..ComplementModel::default()
    };
    malformed.synthesized_keys.insert(1, 7);
    malformed.synthesized_keys.insert(2, 7);
    let unit = ComplementModel::default();

    assert_eq!(
        malformed.check_internal_coherence(),
        Err(ComplementComposeRefusal::SynthesizedIdByKey)
    );
    assert_eq!(
        compose_complement(&malformed, &unit),
        Err(ComplementComposeRefusal::SynthesizedIdByKey)
    );
    assert_eq!(
        compose_complement(&unit, &malformed),
        Err(ComplementComposeRefusal::SynthesizedIdByKey)
    );
}

#[test]
fn reference_carrier_rejects_internal_synthesis_duplicates_symmetrically() {
    assert_reference_carrier_rejects_internal_synthesis_duplicates_symmetrically();
}

fn assert_combined_conflicts_follow_the_declared_first_error_order() {
    let (mut reference_left, mut reference_right) = reference_legacy_pair();
    reference_right.source_fingerprint += 1;
    reference_left.dropped_nodes.insert(1, 1);
    reference_right.dropped_nodes.insert(1, 2);
    reference_left.contraction_choices.insert((1, 2), 1);
    reference_right.contraction_choices.insert((1, 2), 2);
    reference_left.original_parent.insert(1, 2);
    reference_right.original_parent.insert(1, 3);
    reference_left.original_extra_fields.insert(1, 1);
    reference_right.original_extra_fields.insert(1, 2);
    reference_left.arc_edges.insert((1, 2), 1);
    reference_right.arc_edges.insert((1, 2), 2);
    reference_left.original_values.insert(1, 1);
    reference_right.original_values.insert(1, 2);
    reference_left.contracted_into.insert(1, 2);
    reference_right.contracted_into.insert(1, 3);
    reference_left.original_anchors.insert(1, 1);
    reference_right.original_anchors.insert(1, 2);
    reference_left.arc_order = vec![(1, 2), (1, 3)];
    reference_right.arc_order = vec![(1, 3), (1, 2)];
    reference_left.synthesized_keys.insert(1, 1);
    reference_left.synthesized_keys.insert(2, 2);
    reference_right.synthesized_keys.insert(1, 3);
    reference_right.synthesized_keys.insert(3, 2);

    let (mut left, mut right) = legacy_pair();
    right.source_fingerprint += 1;
    left.dropped_nodes.insert(1, Node::new(1, "left"));
    right.dropped_nodes.insert(1, Node::new(1, "right"));
    left.contraction_choices.insert((1, 2), edge(1, "left"));
    right.contraction_choices.insert((1, 2), edge(1, "right"));
    left.original_parent.insert(1, 2);
    right.original_parent.insert(1, 3);
    left.original_extra_fields
        .insert(1, HashMap::from([("x".to_owned(), Value::Int(1))]));
    right
        .original_extra_fields
        .insert(1, HashMap::from([("x".to_owned(), Value::Int(2))]));
    left.arc_edges.insert((1, 2), edge(1, "left"));
    right.arc_edges.insert((1, 2), edge(1, "right"));
    left.original_values
        .insert(1, Some(FieldPresence::Present(Value::Int(1))));
    right
        .original_values
        .insert(1, Some(FieldPresence::Present(Value::Int(2))));
    left.contracted_into.insert(1, 2);
    right.contracted_into.insert(1, 3);
    left.original_anchors.insert(1, "left".into());
    right.original_anchors.insert(1, "right".into());
    left.arc_order = vec![(1, 2), (1, 3)];
    right.arc_order = vec![(1, 3), (1, 2)];
    let key_1 = SynthKey::Expand {
        source_edge: EdgeId(1),
        hop: 0,
    };
    let key_2 = SynthKey::Expand {
        source_edge: EdgeId(2),
        hop: 0,
    };
    let key_3 = SynthKey::Expand {
        source_edge: EdgeId(3),
        hop: 0,
    };
    left.synthesized_keys.insert(1, key_1);
    left.synthesized_keys.insert(2, key_2.clone());
    right.synthesized_keys.insert(1, key_3);
    right.synthesized_keys.insert(3, key_2);

    assert_refusal_step(
        ComplementMonoidRefusal::LegacyFingerprint,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_right.source_fingerprint = LEGACY_FINGERPRINT;
    right.source_fingerprint = LEGACY_FINGERPRINT;

    assert_refusal_step(
        ComplementMonoidRefusal::DroppedNodes,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.dropped_nodes.clear();
    reference_right.dropped_nodes.clear();
    left.dropped_nodes.clear();
    right.dropped_nodes.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::ContractionChoices,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.contraction_choices.clear();
    reference_right.contraction_choices.clear();
    left.contraction_choices.clear();
    right.contraction_choices.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::OriginalParent,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.original_parent.clear();
    reference_right.original_parent.clear();
    left.original_parent.clear();
    right.original_parent.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::OriginalExtraFields,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.original_extra_fields.clear();
    reference_right.original_extra_fields.clear();
    left.original_extra_fields.clear();
    right.original_extra_fields.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::ArcEdges,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.arc_edges.clear();
    reference_right.arc_edges.clear();
    left.arc_edges.clear();
    right.arc_edges.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::OriginalValues,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.original_values.clear();
    reference_right.original_values.clear();
    left.original_values.clear();
    right.original_values.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::ContractedInto,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.contracted_into.clear();
    reference_right.contracted_into.clear();
    left.contracted_into.clear();
    right.contracted_into.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::OriginalAnchors,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.original_anchors.clear();
    reference_right.original_anchors.clear();
    left.original_anchors.clear();
    right.original_anchors.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::ArcOrder,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_left.arc_order.clear();
    reference_right.arc_order.clear();
    left.arc_order.clear();
    right.arc_order.clear();

    assert_refusal_step(
        ComplementMonoidRefusal::SynthesizedKeyById,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_right.synthesized_keys.remove(&1);
    right.synthesized_keys.remove(&1);

    assert_refusal_step(
        ComplementMonoidRefusal::SynthesizedIdByKey,
        &reference_left,
        &reference_right,
        &left,
        &right,
    );
    reference_right.synthesized_keys.remove(&3);
    right.synthesized_keys.remove(&3);

    assert!(compose_complement(&reference_left, &reference_right).is_ok());
    assert!(left.compose(&right).is_ok());
}

#[test]
fn combined_conflicts_follow_the_declared_first_error_order() {
    assert_combined_conflicts_follow_the_declared_first_error_order();
}

const LEFT_NAN_BITS: u64 = 0x7ff8_0000_0000_0042;
const RIGHT_NAN_BITS: u64 = 0xfff8_0000_0000_1729;

fn semantic_float_pair(nan_bits: u64, zero: f64) -> Value {
    Value::List(vec![
        Value::Float(f64::from_bits(nan_bits)),
        Value::Float(zero),
    ])
}

fn float_pair_bits(value: &Value) -> (u64, u64) {
    let Value::List(values) = value else {
        panic!("expected the two-float witness, got {value:?}");
    };
    let [Value::Float(nan), Value::Float(zero)] = values.as_slice() else {
        panic!("expected the two-float witness, got {value:?}");
    };
    (nan.to_bits(), zero.to_bits())
}

fn node_float_pair_bits(complement: &Complement) -> (u64, u64) {
    let value = complement
        .dropped_nodes
        .get(&7)
        .and_then(|node| node.value.as_ref())
        .and_then(FieldPresence::as_value)
        .expect("dropped-node value witness");
    float_pair_bits(value)
}

fn fields_float_pair_bits(complement: &Complement) -> (u64, u64) {
    float_pair_bits(
        complement.original_extra_fields[&8]
            .get("payload")
            .expect("extra-field witness"),
    )
}

fn presence_float_pair_bits(complement: &Complement) -> (u64, u64) {
    let value = complement.original_values[&9]
        .as_ref()
        .and_then(FieldPresence::as_value)
        .expect("original-value witness");
    float_pair_bits(value)
}

fn assert_equality_class_atoms_bridge_semantic_payloads_and_retain_the_left_representative() {
    let mut reference_left = ComplementModel {
        source_fingerprint: LEGACY_FINGERPRINT,
        ..ComplementModel::default()
    };
    reference_left.dropped_nodes.insert(7, 42);
    reference_left.original_extra_fields.insert(8, 42);
    reference_left.original_values.insert(9, 42);
    let reference_right = reference_left.clone();
    assert_eq!(
        compose_complement(&reference_left, &reference_right).expect("one equality class"),
        reference_left,
    );

    let mut left = Complement::empty();
    left.source_fingerprint = LEGACY_FINGERPRINT;
    left.dropped_nodes.insert(
        7,
        Node::new(7, "semantic").with_value(FieldPresence::Present(semantic_float_pair(
            LEFT_NAN_BITS,
            0.0,
        ))),
    );
    left.original_extra_fields.insert(
        8,
        HashMap::from([(
            "payload".to_owned(),
            semantic_float_pair(LEFT_NAN_BITS, 0.0),
        )]),
    );
    left.original_values.insert(
        9,
        Some(FieldPresence::Present(semantic_float_pair(
            LEFT_NAN_BITS,
            0.0,
        ))),
    );

    let mut right = Complement::empty();
    right.source_fingerprint = LEGACY_FINGERPRINT;
    right.dropped_nodes.insert(
        7,
        Node::new(7, "semantic").with_value(FieldPresence::Present(semantic_float_pair(
            RIGHT_NAN_BITS,
            -0.0,
        ))),
    );
    right.original_extra_fields.insert(
        8,
        HashMap::from([(
            "payload".to_owned(),
            semantic_float_pair(RIGHT_NAN_BITS, -0.0),
        )]),
    );
    right.original_values.insert(
        9,
        Some(FieldPresence::Present(semantic_float_pair(
            RIGHT_NAN_BITS,
            -0.0,
        ))),
    );

    // The concrete values are not literally `PartialEq`: each includes a
    // NaN. They nevertheless inhabit one reference atom because the
    // production equivalence identifies every pair of NaNs and uses numeric
    // zero equality at all three payload-bearing coordinates.
    assert_ne!(LEFT_NAN_BITS, RIGHT_NAN_BITS);
    assert!(f64::from_bits(LEFT_NAN_BITS).is_nan());
    assert!(f64::from_bits(RIGHT_NAN_BITS).is_nan());
    assert_ne!(left.dropped_nodes[&7].value, right.dropped_nodes[&7].value);
    assert_ne!(
        left.original_extra_fields[&8],
        right.original_extra_fields[&8]
    );
    assert_ne!(left.original_values[&9], right.original_values[&9]);

    let left_right = left.compose(&right).expect("semantic equality is admitted");
    let right_left = right
        .compose(&left)
        .expect("semantic equality is symmetric");
    let positive_zero = 0.0_f64.to_bits();
    let negative_zero = (-0.0_f64).to_bits();

    assert_eq!(
        node_float_pair_bits(&left_right),
        (LEFT_NAN_BITS, positive_zero)
    );
    assert_eq!(
        fields_float_pair_bits(&left_right),
        (LEFT_NAN_BITS, positive_zero)
    );
    assert_eq!(
        presence_float_pair_bits(&left_right),
        (LEFT_NAN_BITS, positive_zero)
    );
    assert_eq!(
        node_float_pair_bits(&right_left),
        (RIGHT_NAN_BITS, negative_zero)
    );
    assert_eq!(
        fields_float_pair_bits(&right_left),
        (RIGHT_NAN_BITS, negative_zero)
    );
    assert_eq!(
        presence_float_pair_bits(&right_left),
        (RIGHT_NAN_BITS, negative_zero)
    );
}

#[test]
fn equality_class_atoms_bridge_semantic_payloads_and_retain_the_left_representative() {
    assert_equality_class_atoms_bridge_semantic_payloads_and_retain_the_left_representative();
}

fn assert_complement_partial_monoid_law_certificate() {
    let verified = identity_lens("verified");
    let other_verified = identity_lens("other-verified");
    let unit = Complement::empty();
    let mut compatible = 0_usize;
    let mut refusals = 0_usize;
    let mut focused_witnesses = 0_usize;

    for_each_complement_monoid_case(|case| match case {
        ComplementMonoidCase::Compatible(triple) => {
            compatible += 1;
            let reference_unit = ComplementModel::default();
            let reference_left = model_for(triple.binding, triple.left);
            let reference_middle = model_for(triple.binding, triple.middle);
            let reference_right = model_for(triple.binding, triple.right);
            let reference_pair =
                compose_complement(&reference_left, &reference_middle).expect("compatible pair");
            let reference_reverse =
                compose_complement(&reference_middle, &reference_left).expect("reverse pair");
            let reference_left_associated = compose_complement(&reference_pair, &reference_right)
                .expect("left-associated reference triple");
            let reference_middle_right = compose_complement(&reference_middle, &reference_right)
                .expect("middle-right reference pair");
            let reference_right_associated =
                compose_complement(&reference_left, &reference_middle_right)
                    .expect("right-associated reference triple");
            let reference_expected = model_for(triple.binding, triple.union);

            assert_eq!(
                compose_complement(&reference_unit, &reference_left).expect("reference left unit"),
                reference_left
            );
            assert_eq!(
                compose_complement(&reference_left, &reference_unit).expect("reference right unit"),
                reference_left
            );
            assert_eq!(
                compose_complement(&reference_left, &reference_left)
                    .expect("reference idempotence"),
                reference_left
            );
            assert_eq!(
                reference_pair,
                model_for(triple.binding, triple.left | triple.middle)
            );
            assert_eq!(reference_reverse, reference_pair);
            assert_eq!(reference_left_associated, reference_expected);
            assert_eq!(reference_right_associated, reference_expected);

            let left = complement_for(triple.binding, triple.left, &verified);
            let middle = complement_for(triple.binding, triple.middle, &verified);
            let right = complement_for(triple.binding, triple.right, &verified);

            assert_equivalent(
                unit.compose(&left).expect("left unit is defined"),
                materialize_model(&reference_left, &verified),
                "left identity",
            );
            assert_equivalent(
                left.compose(&unit).expect("right unit is defined"),
                materialize_model(&reference_left, &verified),
                "right identity",
            );
            assert_equivalent(
                left.compose(&left).expect("idempotence is defined"),
                materialize_model(&reference_left, &verified),
                "idempotence",
            );

            assert_eq!(
                left.is_compatible(&middle),
                compose_complement(&reference_left, &reference_middle).is_ok()
            );
            assert_eq!(
                middle.is_compatible(&left),
                compose_complement(&reference_middle, &reference_left).is_ok()
            );
            assert_equivalent(
                left.compose(&middle).expect("compatible pair"),
                materialize_model(&reference_pair, &verified),
                "reference union",
            );
            assert_equivalent(
                middle.compose(&left).expect("reverse compatible pair"),
                materialize_model(&reference_reverse, &verified),
                "commutativity",
            );

            let left_associated = left
                .compose(&middle)
                .and_then(|pair| pair.compose(&right))
                .expect("left-associated triple");
            let right_associated = middle
                .compose(&right)
                .and_then(|pair| left.compose(&pair))
                .expect("right-associated triple");
            assert_equivalent(
                left_associated,
                materialize_model(&reference_left_associated, &verified),
                "left association",
            );
            assert_equivalent(
                right_associated,
                materialize_model(&reference_right_associated, &verified),
                "right association",
            );
        }
        ComplementMonoidCase::Refusal(refusal) => {
            refusals += 1;
            let (left, right) = refusal_pair(refusal, &verified, &other_verified);
            let (reference_left, reference_right) = reference_refusal_pair(refusal);
            for ((first, second), (reference_first, reference_second)) in [
                ((&left, &right), (&reference_left, &reference_right)),
                ((&right, &left), (&reference_right, &reference_left)),
            ] {
                let reference_error = compose_complement(reference_first, reference_second)
                    .expect_err("reference named pair is off-domain");
                assert_eq!(reference_error, reference_refusal(refusal));
                let error = first.compose(second).expect_err("named pair is off-domain");
                assert_production_refusal(refusal, error, first, second);
                assert_eq!(
                    first.is_compatible(second),
                    compose_complement(reference_first, reference_second).is_ok()
                );
            }
        }
        ComplementMonoidCase::InternalSynthesisDuplicates => {
            focused_witnesses += 1;
            assert_reference_carrier_rejects_internal_synthesis_duplicates_symmetrically();
        }
        ComplementMonoidCase::CombinedConflictOrdering => {
            focused_witnesses += 1;
            assert_combined_conflicts_follow_the_declared_first_error_order();
        }
        ComplementMonoidCase::EqualityClassRepresentatives => {
            focused_witnesses += 1;
            assert_equality_class_atoms_bridge_semantic_payloads_and_retain_the_left_representative(
            );
        }
    });

    assert_eq!(compatible, 2 * 8 * 8 * 8);
    assert_eq!(refusals, ComplementMonoidRefusal::ALL.len());
    assert_eq!(focused_witnesses, 3);
    assert_eq!(
        compatible + refusals + focused_witnesses,
        complement_monoid_count_certificate()
    );
}

#[test]
fn complement_partial_monoid_law_certificate() {
    assert_complement_partial_monoid_law_certificate();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// A separately sampled check of the same reference union over arbitrary
    /// triples. This remains useful when the finite carrier is later enlarged:
    /// the property runner will continue to vary all operand positions.
    #[test]
    fn randomized_complement_partial_monoid_differential(
        verified_binding in any::<bool>(),
        left_mask in 0_u8..8,
        middle_mask in 0_u8..8,
        right_mask in 0_u8..8,
    ) {
        let lens = identity_lens("property");
        let binding = if verified_binding {
            ComplementMonoidBinding::Verified
        } else {
            ComplementMonoidBinding::Legacy
        };
        let reference_left = model_for(binding, left_mask);
        let reference_middle = model_for(binding, middle_mask);
        let reference_right = model_for(binding, right_mask);
        let reference_left_associated = compose_complement(
            &compose_complement(&reference_left, &reference_middle)
                .expect("reference left-middle pair"),
            &reference_right,
        )
        .expect("reference left-associated triple");
        let reference_right_associated = compose_complement(
            &reference_left,
            &compose_complement(&reference_middle, &reference_right)
                .expect("reference middle-right pair"),
        )
        .expect("reference right-associated triple");
        let reference_expected = model_for(binding, left_mask | middle_mask | right_mask);
        prop_assert_eq!(&reference_left_associated, &reference_expected);
        prop_assert_eq!(&reference_right_associated, &reference_expected);

        let left = complement_for(binding, left_mask, &lens);
        let middle = complement_for(binding, middle_mask, &lens);
        let right = complement_for(binding, right_mask, &lens);
        let left_associated = left
            .compose(&middle)
            .and_then(|pair| pair.compose(&right))
            .expect("reference masks always agree on overlap");
        let right_associated = middle
            .compose(&right)
            .and_then(|pair| left.compose(&pair))
            .expect("reference masks always agree on overlap");

        prop_assert_eq!(
            normal_form(left_associated),
            normal_form(materialize_model(&reference_left_associated, &lens))
        );
        prop_assert_eq!(
            normal_form(right_associated),
            normal_form(materialize_model(&reference_right_associated, &lens))
        );

        // Keep every off-domain constructor live in the randomized consumer,
        // rather than relying on a selector that may miss a refusal family in
        // one property run.
        let other_verified = identity_lens("other-property");
        for refusal in ComplementMonoidRefusal::ALL {
            let (left, right) = refusal_pair(refusal, &lens, &other_verified);
            let (reference_left, reference_right) = reference_refusal_pair(refusal);
            assert_refusal_step(
                refusal,
                &reference_left,
                &reference_right,
                &left,
                &right,
            );
            assert_refusal_step(
                refusal,
                &reference_right,
                &reference_left,
                &right,
                &left,
            );
        }

        // These singleton families do not depend on the sampled masks. Run
        // all three in every property case so the randomized consumer covers
        // the same constructor families as the exact carrier.
        assert_reference_carrier_rejects_internal_synthesis_duplicates_symmetrically();
        assert_combined_conflicts_follow_the_declared_first_error_order();
        assert_equality_class_atoms_bridge_semantic_payloads_and_retain_the_left_representative();
    }
}
