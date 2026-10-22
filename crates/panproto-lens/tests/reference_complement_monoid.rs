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
use panproto_inst::identity::{ComplementShape, IdentityCarrier, IdentityError};
use panproto_inst::{
    CompiledMigration, Complement, EdgeId, Fan, FieldPresence, Node, SynthKey, Value,
};
use panproto_lens::{ComplementCompose, Lens, LensError};
use panproto_reference::enumerate::lenses::{
    ComplementMonoidBinding, ComplementMonoidCase, ComplementMonoidRefusal,
    complement_monoid_count_certificate, for_each_complement_monoid_case,
};
use panproto_schema::{Edge, Protocol, SchemaBuilder};
use proptest::prelude::*;

const LEGACY_FINGERPRINT: u64 = 0xC011_EC70;

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

fn bind(complement: &mut Complement, binding: ComplementMonoidBinding, lens: &Lens) {
    match binding {
        ComplementMonoidBinding::Legacy => {
            complement.source_fingerprint = LEGACY_FINGERPRINT;
        }
        ComplementMonoidBinding::Verified => {
            complement.stamp(lens.identity(ComplementShape::WType), &lens.src_schema);
        }
    }
}

/// Add one canonical resource bundle. The atom's key determines every value,
/// so overlapping masks agree on their overlap by construction.
fn populate_atom(complement: &mut Complement, atom: u32) {
    let key = atom + 1;
    let child = key + 10;
    let parent = key + 100;
    let canonical_edge = edge(key, "canonical");

    complement.dropped_nodes.insert(
        key,
        Node::new(key, format!("node-{key}"))
            .with_value(FieldPresence::Present(Value::Int(i64::from(key)))),
    );
    complement
        .dropped_arcs
        .push((key, child, canonical_edge.clone()));
    complement
        .dropped_fans
        .push(Fan::new(format!("fan-{key}"), key).with_child(format!("slot-{key}"), child));
    complement
        .contraction_choices
        .insert((key, child), canonical_edge.clone());
    complement.original_parent.insert(key, parent);
    complement.original_extra_fields.insert(
        key,
        HashMap::from([("payload".to_owned(), Value::Int(i64::from(key)))]),
    );
    complement.arc_edges.insert((key, child), canonical_edge);
    complement.original_values.insert(
        key,
        Some(FieldPresence::Present(Value::Str(format!("value-{key}")))),
    );
    complement.synthesized_nodes.insert(key);
    complement.contracted_into.insert(key, parent);
    complement
        .original_anchors
        .insert(key, Name::from(format!("anchor-{key}")));
    complement.synthesized_keys.insert(
        key,
        SynthKey::Expand {
            source_edge: EdgeId(key),
            hop: u16::try_from(atom).expect("three atoms fit in u16"),
        },
    );
    complement.id_floor = complement.id_floor.max(parent + 1);
}

fn complement_for(binding: ComplementMonoidBinding, mask: u8, lens: &Lens) -> Complement {
    let mut complement = Complement::empty();
    for atom in 0_u32..3 {
        if mask & (1 << atom) != 0 {
            populate_atom(&mut complement, atom);
        }
    }
    if mask != 0 {
        // Arc order is a single global resource rather than a keyed map. A
        // common nonempty value exercises agreement; mask zero exercises its
        // unit. A separate refusal case supplies a disagreeing order.
        complement.arc_order = vec![(1, 11), (2, 12), (3, 13)];
    }
    bind(&mut complement, binding, lens);
    complement
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

fn matches_refusal(refusal: ComplementMonoidRefusal, error: &LensError) -> bool {
    match refusal {
        ComplementMonoidRefusal::DroppedNodes => matches!(
            error,
            LensError::ComplementConflict {
                kind: "dropped_nodes",
                ..
            }
        ),
        ComplementMonoidRefusal::ContractionChoices => matches!(
            error,
            LensError::ComplementConflict {
                kind: "contraction_choices",
                ..
            }
        ),
        ComplementMonoidRefusal::OriginalParent => matches!(
            error,
            LensError::ComplementConflict {
                kind: "original_parent",
                ..
            }
        ),
        ComplementMonoidRefusal::OriginalExtraFields => matches!(
            error,
            LensError::ComplementConflict {
                kind: "original_extra_fields",
                ..
            }
        ),
        ComplementMonoidRefusal::ArcEdges => matches!(
            error,
            LensError::ComplementConflict {
                kind: "arc_edges",
                ..
            }
        ),
        ComplementMonoidRefusal::OriginalValues => matches!(
            error,
            LensError::ComplementConflict {
                kind: "original_values",
                ..
            }
        ),
        ComplementMonoidRefusal::ContractedInto => matches!(
            error,
            LensError::ComplementConflict {
                kind: "contracted_into",
                ..
            }
        ),
        ComplementMonoidRefusal::OriginalAnchors => matches!(
            error,
            LensError::ComplementConflict {
                kind: "original_anchors",
                ..
            }
        ),
        ComplementMonoidRefusal::SynthesizedKeyById
        | ComplementMonoidRefusal::SynthesizedIdByKey => matches!(
            error,
            LensError::ComplementConflict {
                kind: "synthesized_keys",
                ..
            }
        ),
        ComplementMonoidRefusal::ArcOrder => matches!(
            error,
            LensError::ComplementConflict {
                kind: "arc_order",
                ..
            }
        ),
        ComplementMonoidRefusal::UnboundData => {
            matches!(error, LensError::Identity(IdentityError::UnboundComplement))
        }
        ComplementMonoidRefusal::LegacyFingerprint => matches!(
            error,
            LensError::Identity(IdentityError::LegacyComplementRejected { .. })
        ),
        ComplementMonoidRefusal::MixedProvenance => matches!(
            error,
            LensError::Identity(IdentityError::LegacyComplementRejected { .. })
        ),
        ComplementMonoidRefusal::EnvelopeMismatch => {
            matches!(error, LensError::Identity(IdentityError::Mismatch { .. }))
        }
    }
}

#[test]
fn complement_partial_monoid_law_certificate() {
    let verified = identity_lens("verified");
    let other_verified = identity_lens("other-verified");
    let unit = Complement::empty();
    let mut compatible = 0_usize;
    let mut refusals = 0_usize;

    for_each_complement_monoid_case(|case| match case {
        ComplementMonoidCase::Compatible(triple) => {
            compatible += 1;
            let left = complement_for(triple.binding, triple.left, &verified);
            let middle = complement_for(triple.binding, triple.middle, &verified);
            let right = complement_for(triple.binding, triple.right, &verified);
            let pair_expected =
                complement_for(triple.binding, triple.left | triple.middle, &verified);
            let expected = complement_for(triple.binding, triple.union, &verified);

            assert_equivalent(
                unit.compose(&left).expect("left unit is defined"),
                left.clone(),
                "left identity",
            );
            assert_equivalent(
                left.compose(&unit).expect("right unit is defined"),
                left.clone(),
                "right identity",
            );
            assert_equivalent(
                left.compose(&left).expect("idempotence is defined"),
                left.clone(),
                "idempotence",
            );

            assert!(left.is_compatible(&middle));
            assert!(middle.is_compatible(&left));
            assert_equivalent(
                left.compose(&middle).expect("compatible pair"),
                pair_expected.clone(),
                "reference union",
            );
            assert_equivalent(
                middle.compose(&left).expect("reverse compatible pair"),
                pair_expected,
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
            assert_equivalent(left_associated, expected.clone(), "left association");
            assert_equivalent(right_associated, expected, "right association");
        }
        ComplementMonoidCase::Refusal(refusal) => {
            refusals += 1;
            let (left, right) = refusal_pair(refusal, &verified, &other_verified);
            for (first, second) in [(&left, &right), (&right, &left)] {
                let error = first.compose(second).expect_err("named pair is off-domain");
                assert!(
                    matches_refusal(refusal, &error),
                    "wrong refusal for {refusal:?}: {error:?}"
                );
                assert!(!first.is_compatible(second));
            }
        }
    });

    assert_eq!(compatible, 2 * 8 * 8 * 8);
    assert_eq!(refusals, ComplementMonoidRefusal::ALL.len());
    assert_eq!(compatible + refusals, complement_monoid_count_certificate());
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
        let left = complement_for(binding, left_mask, &lens);
        let middle = complement_for(binding, middle_mask, &lens);
        let right = complement_for(binding, right_mask, &lens);
        let expected = complement_for(binding, left_mask | middle_mask | right_mask, &lens);
        let left_associated = left
            .compose(&middle)
            .and_then(|pair| pair.compose(&right))
            .expect("reference masks always agree on overlap");
        let right_associated = middle
            .compose(&right)
            .and_then(|pair| left.compose(&pair))
            .expect("reference masks always agree on overlap");

        prop_assert_eq!(normal_form(left_associated), normal_form(expected.clone()));
        prop_assert_eq!(normal_form(right_associated), normal_form(expected));
    }
}
