//! [`PutDomainError`] and [`ErrorDetail`]: their variants, their order and
//! their wire form, which every binding decodes instead of parsing messages.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Debug;

use panproto_inst::identity::{IdentityField, LegacyRejectReason};
use panproto_inst::{
    EdgeId, ErrorDetail, FanViolation, PredicateRefusal, PutDomainError, RowId, Value, ViewRef,
};
use panproto_schema::Edge;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// One error of each variant, in declaration order, with its serde tag.
fn every_put_domain_error() -> Vec<(PutDomainError, &'static str)> {
    let node = ViewRef::Node(3);
    vec![
        (
            PutDomainError::IdCollision { elem: node.clone() },
            "id_collision",
        ),
        (
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(EdgeId(2)),
            },
            "synthesized_edited",
        ),
        (
            PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 1,
                },
            },
            "contracted_edge_inserted",
        ),
        (
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Pair {
                    edge: Edge {
                        src: "a".into(),
                        tgt: "b".into(),
                        kind: "fk".into(),
                        name: None,
                    },
                    src: RowId(1),
                    tgt: RowId(2),
                },
            },
            "unrepresentable_view_edge",
        ),
        (
            PutDomainError::AmbiguousReverseAnchor {
                elem: node.clone(),
                candidates: vec!["x".into(), "y".into()],
            },
            "ambiguous_reverse_anchor",
        ),
        (
            PutDomainError::SurvivalPredicateFails { elem: node.clone() },
            "survival_predicate_fails",
        ),
        (
            PutDomainError::DerivedCellEdited {
                elem: ViewRef::Row(RowId(4)),
                field: "total".into(),
            },
            "derived_cell_edited",
        ),
        (
            PutDomainError::TransformNotInvertible {
                elem: node.clone(),
                field: "f".into(),
            },
            "transform_not_invertible",
        ),
        (
            PutDomainError::DuplicateKey {
                table: "users".into(),
                key: vec![("id".into(), Value::Int(7)), ("org".into(), Value::Null)],
                elems: vec![ViewRef::Row(RowId(1)), ViewRef::Row(RowId(5))],
            },
            "duplicate_key",
        ),
        (
            PutDomainError::MissingRequiredCell {
                elem: node,
                field: "name".into(),
            },
            "missing_required_cell",
        ),
        (
            PutDomainError::MalformedFan {
                node: 9,
                reason: FanViolation::MultipleParents {
                    edges: vec![EdgeId(1), EdgeId(2)],
                },
            },
            "malformed_fan",
        ),
        (
            PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(1),
                refusal: PredicateRefusal::NotBoolean { got: "int".into() },
            },
            "survival_predicate_refused",
        ),
        (
            PutDomainError::PlanCompilation {
                detail: "incoherent edge image".into(),
            },
            "plan_compilation",
        ),
    ]
}

/// One refusal of each variant, in declaration order, with its serde tag.
fn every_predicate_refusal() -> Vec<(PredicateRefusal, &'static str)> {
    vec![
        (
            PredicateRefusal::AmbiguousChildField { field: "a".into() },
            "ambiguous_child_field",
        ),
        (
            PredicateRefusal::StructuralChildNotATree { field: "b".into() },
            "structural_child_not_a_tree",
        ),
        (
            PredicateRefusal::ValueNotExpressible {
                field: "c".into(),
                value_type: "bytes".into(),
            },
            "value_not_expressible",
        ),
        (
            PredicateRefusal::EvaluationFailed {
                message: "unbound variable: x".into(),
            },
            "evaluation_failed",
        ),
        (
            PredicateRefusal::NotBoolean { got: "int".into() },
            "not_boolean",
        ),
    ]
}

/// JSON, msgpack (named and positional) and CBOR round trips.
fn round_trips<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: &T) {
    let json = serde_json::to_vec(value).unwrap();
    assert_eq!(&serde_json::from_slice::<T>(&json).unwrap(), value, "JSON");
    let named = rmp_serde::to_vec_named(value).unwrap();
    assert_eq!(
        &rmp_serde::from_slice::<T>(&named).unwrap(),
        value,
        "msgpack named"
    );
    let positional = rmp_serde::to_vec(value).unwrap();
    assert_eq!(
        &rmp_serde::from_slice::<T>(&positional).unwrap(),
        value,
        "msgpack positional"
    );
    let mut cbor = Vec::new();
    ciborium::ser::into_writer(value, &mut cbor).unwrap();
    assert_eq!(
        &ciborium::de::from_reader::<T, _>(cbor.as_slice()).unwrap(),
        value,
        "CBOR"
    );
}

/// The single key of an externally tagged JSON object.
fn tag_of<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_value(value).unwrap();
    let object = json.as_object().expect("externally tagged as an object");
    assert_eq!(object.len(), 1, "{json}");
    object.keys().next().unwrap().clone()
}

#[test]
fn put_domain_error_serde_tags_and_order_are_fixed() {
    let errors = every_put_domain_error();
    assert_eq!(errors.len(), 13);
    for (index, (error, tag)) in errors.iter().enumerate() {
        assert_eq!(usize::from(error.variant_index()), index, "{tag}");
        assert_eq!(tag_of(error), *tag);
        round_trips(error);
    }

    // `DuplicateKey` carries its rows last, after the table and the key.
    let (duplicate, _) = &errors[8];
    let json = serde_json::to_string(duplicate).unwrap();
    assert_eq!(
        json,
        r#"{"duplicate_key":{"table":"users","key":[["id",{"Int":7}],["org","Null"]],"elems":[{"row":1},{"row":5}]}}"#
    );
    let positional: serde_json::Value =
        rmp_serde::from_slice(&rmp_serde::to_vec(duplicate).unwrap()).unwrap();
    let fields = positional["duplicate_key"].as_array().unwrap();
    assert_eq!(fields.len(), 3);
    assert_eq!(
        fields[2],
        serde_json::json!([{"row": 1}, {"row": 5}]),
        "elems is the last positional field"
    );

    // A refused survival predicate carries its refusal whole, itself
    // externally tagged.
    let (refused, _) = &errors[11];
    assert_eq!(
        serde_json::to_string(refused).unwrap(),
        r#"{"survival_predicate_refused":{"elem":{"node":1},"refusal":{"not_boolean":{"got":"int"}}}}"#
    );
    for (refusal, tag) in every_predicate_refusal() {
        assert_eq!(tag_of(&refusal), tag);
        round_trips(&refusal);
        round_trips(&PutDomainError::SurvivalPredicateRefused {
            elem: ViewRef::Node(0),
            refusal,
        });
    }

    let (plan, _) = &errors[12];
    assert_eq!(
        serde_json::to_string(plan).unwrap(),
        r#"{"plan_compilation":{"detail":"incoherent edge image"}}"#
    );

    // The fan's reason is the shared `FanViolation`, carried whole.
    let (fan, _) = &errors[10];
    assert_eq!(
        serde_json::to_string(fan).unwrap(),
        r#"{"malformed_fan":{"node":9,"reason":{"multiple_parents":{"edges":[1,2]}}}}"#
    );
}

#[test]
fn a_put_domain_error_says_what_is_wrong() {
    let e = PutDomainError::MalformedFan {
        node: 9,
        reason: FanViolation::MissingParent,
    };
    assert!(
        e.to_string().starts_with("fan node 9 is malformed: "),
        "{e}"
    );
    let e = PutDomainError::DerivedCellEdited {
        elem: ViewRef::Row(RowId(4)),
        field: "total".into(),
    };
    assert!(e.to_string().contains("total"), "{e}");
}

fn identity_mismatch() -> ErrorDetail {
    ErrorDetail::IdentityMismatch {
        field: IdentityField::Lens,
        expected: "ab".repeat(32),
        found: "00".repeat(32),
    }
}

const fn legacy_rejected() -> ErrorDetail {
    ErrorDetail::LegacyRejected {
        reason: LegacyRejectReason::PolicyStrict,
        fingerprint: u64::MAX - 3,
    }
}

#[test]
fn error_detail_serializes_externally_tagged_snake_case() {
    for (detail, tag) in [
        (identity_mismatch(), "identity_mismatch"),
        (legacy_rejected(), "legacy_rejected"),
        (ErrorDetail::UnboundComplement {}, "unbound_complement"),
        (
            ErrorDetail::PutDomain {
                error: PutDomainError::IdCollision {
                    elem: ViewRef::Node(0),
                },
            },
            "put_domain",
        ),
    ] {
        assert_eq!(tag_of(&detail), tag);
    }
    assert_eq!(
        serde_json::to_string(&identity_mismatch()).unwrap(),
        format!(
            r#"{{"identity_mismatch":{{"field":"lens","expected":"{}","found":"{}"}}}}"#,
            "ab".repeat(32),
            "00".repeat(32)
        )
    );
    assert_eq!(
        serde_json::to_string(&legacy_rejected()).unwrap(),
        format!(
            r#"{{"legacy_rejected":{{"reason":"policy_strict","fingerprint":{}}}}}"#,
            u64::MAX - 3
        )
    );
    assert_eq!(
        serde_json::to_string(&ErrorDetail::UnboundComplement {}).unwrap(),
        r#"{"unbound_complement":{}}"#
    );
}

#[test]
fn error_detail_identity_mismatch_round_trips() {
    round_trips(&identity_mismatch());
}

#[test]
fn error_detail_legacy_rejected_round_trips() {
    for reason in [
        LegacyRejectReason::PolicyStrict,
        LegacyRejectReason::MixedWithVerified,
        LegacyRejectReason::FingerprintMismatch,
    ] {
        round_trips(&ErrorDetail::LegacyRejected {
            reason,
            fingerprint: 5_545_373_909_098_406_705,
        });
    }
}

#[test]
fn error_detail_unbound_complement_round_trips() {
    round_trips(&ErrorDetail::UnboundComplement {});
}

#[test]
fn error_detail_put_domain_round_trips() {
    for (error, _) in every_put_domain_error() {
        let detail = ErrorDetail::PutDomain { error };
        assert_eq!(tag_of(&detail), "put_domain");
        round_trips(&detail);
    }
}

/// The error-detail variants that `put_domain` replaced are not variants:
/// a detail tagged with one of their names does not decode.
#[test]
fn replaced_error_detail_variants_do_not_decode() {
    for replaced in [
        r#"{"id_collision":{"kind":"node","id":1}}"#,
        r#"{"synthesized_edge_edited":{"elem":{"node":1}}}"#,
        r#"{"unrepresentable_view_edge":{"elem":{"node":1}}}"#,
    ] {
        assert!(
            serde_json::from_str::<ErrorDetail>(replaced).is_err(),
            "{replaced} decoded"
        );
    }
}
