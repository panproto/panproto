//! Segment errors, residuals and their structured details round-trip
//! through every serde format the bindings use.

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{TestResult, label};
use crate::error::RestrictError;
use crate::error_detail::{ErrorDetail, PutDomainError};
use crate::ginstance::EdgeId;
use crate::identity::{Digest, IdentityError, IdentityField};
use crate::locality::{GlobalReason, SegmentOp};
use crate::segment::{
    ArtifactKind, ErrorAt, ErrorKey, IdKind, IdRange, PartitionDefect, Phase, Residual, Resource,
    SegmentError, SegmentId,
};
use crate::view_ref::{RowId, ViewRef};
use panproto_schema::InstanceShapeKind;

/// Round-trip `value` through JSON, `MessagePack` and CBOR, each giving back
/// an equal value.
fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(
    value: &T,
) -> TestResult {
    let json: T = serde_json::from_str(&serde_json::to_string(value)?)?;
    assert_eq!(&json, value, "JSON");
    let msgpack: T = rmp_serde::from_slice(&rmp_serde::to_vec_named(value)?)?;
    assert_eq!(&msgpack, value, "MessagePack");
    let mut cbor = Vec::new();
    ciborium::into_writer(value, &mut cbor)?;
    let back: T = ciborium::from_reader(cbor.as_slice())?;
    assert_eq!(&back, value, "CBOR");
    Ok(())
}

/// One value of every [`SegmentError`] variant.
fn every_segment_error() -> Vec<SegmentError> {
    let pair = ViewRef::Pair {
        edge: label("a", "b", "fk", None),
        src: RowId(1),
        tgt: RowId(2),
    };
    let mut all = range_and_digest_errors(&pair);
    all.extend(payload_errors(pair));
    all
}

/// One value of every variant about ranges, ownership, digests and
/// allocation.
fn range_and_digest_errors(pair: &ViewRef) -> Vec<SegmentError> {
    let s = SegmentId;
    vec![
        SegmentError::InvertedRange {
            segment: s(1),
            range: IdRange { lo: 4, hi: 2 },
        },
        SegmentError::OverlappingRanges { a: s(0), b: s(2) },
        SegmentError::Unowned {
            elem: ViewRef::Node(7),
        },
        SegmentError::WrongOwner {
            elem: pair.clone(),
            expected: s(1),
            found: s(0),
        },
        SegmentError::GhostMismatch { node: 3 },
        SegmentError::InStubMismatch {
            elem: ViewRef::Arc {
                parent: 1,
                child: 2,
            },
        },
        SegmentError::DuplicateEdgeId {
            edge: EdgeId(9),
            segments: vec![s(0), s(3)],
        },
        SegmentError::CrossMismatch {
            from: s(0),
            to: s(1),
        },
        SegmentError::DigestMismatch {
            segment: None,
            expected: Digest([1; 32]),
            found: Digest([2; 32]),
        },
        SegmentError::SchemaMismatch {
            expected: Digest([3; 32]),
            found: Digest([4; 32]),
        },
        SegmentError::ShapeMismatch {
            expected: InstanceShapeKind::Tree,
            found: InstanceShapeKind::Graph,
        },
        SegmentError::NoAllocationRange {
            segment: s(2),
            kind: IdKind::Node,
        },
        SegmentError::AllocationExhausted {
            segment: s(2),
            kind: IdKind::Edge,
        },
        SegmentError::AllocationBelowFloor {
            segment: s(1),
            kind: IdKind::Edge,
        },
        SegmentError::EdgeIdOutsideAllocation {
            edge: EdgeId(4),
            segment: s(0),
        },
    ]
}

/// One value of every variant that carries a nested error or a payload, and
/// of the structural variants.
fn payload_errors(pair: ViewRef) -> Vec<SegmentError> {
    let s = SegmentId;
    vec![
        SegmentError::Invalid {
            violations: vec![
                SegmentError::GhostMismatch { node: 1 },
                SegmentError::Unowned { elem: pair },
            ],
        },
        SegmentError::Restrict {
            at: ErrorAt {
                segment: s(1),
                key: ErrorKey {
                    stage: 1,
                    id: 3,
                    id2: 0,
                    kind: 2,
                },
            },
            detail: Box::new(RestrictError::RootPruned.detail()),
        },
        SegmentError::PutDomain(vec![PutDomainError::IdCollision {
            elem: ViewRef::Node(1),
        }]),
        SegmentError::Edit {
            segment: s(0),
            detail: Box::new(ErrorDetail::UnboundComplement {}),
        },
        SegmentError::Identity(IdentityError::Mismatch {
            field: IdentityField::Lens,
            expected: "a".into(),
            found: "b".into(),
        }),
        SegmentError::Decode {
            artifact: ArtifactKind::Manifest,
            message: "trailing bytes".into(),
        },
        SegmentError::MalformedPartition {
            segment: s(1),
            defect: PartitionDefect::NotDense { found: s(4) },
        },
        SegmentError::DanglingEndpoint {
            elem: ViewRef::Edge(EdgeId(2)),
            node: 6,
        },
        SegmentError::NotEmbedded {
            elem: ViewRef::Node(0),
        },
    ]
}

#[test]
fn segment_error_round_trips_every_variant() -> TestResult {
    let errors = every_segment_error();
    let kinds: std::collections::BTreeSet<&str> = errors.iter().map(SegmentError::kind).collect();
    assert_eq!(kinds.len(), errors.len(), "one value per variant");
    for error in &errors {
        round_trip(error)?;
        // The serialized tag is the variant's kind.
        let json = serde_json::to_value(error)?;
        let tag = json
            .as_object()
            .and_then(|o| o.keys().next().cloned())
            .ok_or("an externally tagged error")?;
        assert_eq!(tag, error.kind());
        assert!(!error.to_string().is_empty());
    }
    Ok(())
}

#[test]
fn residuals_are_tagged_in_snake_case_and_round_trip() -> TestResult {
    let global = Residual::Global {
        op: SegmentOp::Get,
        reason: GlobalReason::PiProduct,
    };
    assert_eq!(
        serde_json::to_string(&global)?,
        r#"{"global":{"op":"get","reason":"pi_product"}}"#
    );
    let over = Residual::BudgetExceeded {
        resource: Resource::AffectedSegments,
        phase: Phase::Assemble,
        segment: Some(SegmentId(3)),
        needed: 9,
        budget: 4,
    };
    assert_eq!(
        serde_json::to_string(&over)?,
        r#"{"budget_exceeded":{"resource":"affected_segments","phase":"assemble","segment":3,"needed":9,"budget":4}}"#
    );
    for resource in [
        Resource::WorkingSet,
        Resource::Complement,
        Resource::Summary,
        Resource::AffectedSegments,
    ] {
        round_trip(&resource)?;
    }
    round_trip(&global)?;
    round_trip(&over)?;
    assert_eq!(
        over.to_string(),
        "Assemble of segment 3 needs 9 of affected segments, the budget grants 4"
    );
    Ok(())
}

#[test]
fn error_detail_segment_round_trips() -> TestResult {
    for error in every_segment_error() {
        let detail = ErrorDetail::Segment { error };
        round_trip(&detail)?;
    }
    let detail = ErrorDetail::Segment {
        error: SegmentError::GhostMismatch { node: 3 },
    };
    assert_eq!(
        serde_json::to_string(&detail)?,
        r#"{"segment":{"error":{"ghost_mismatch":{"node":3}}}}"#
    );
    Ok(())
}

#[test]
fn error_detail_residual_round_trips() -> TestResult {
    let detail = ErrorDetail::Residual {
        residual: Residual::Global {
            op: SegmentOp::Put,
            reason: GlobalReason::SequentialComposite,
        },
    };
    assert_eq!(
        serde_json::to_string(&detail)?,
        r#"{"residual":{"residual":{"global":{"op":"put","reason":"sequential_composite"}}}}"#
    );
    round_trip(&detail)?;
    round_trip(&ErrorDetail::Residual {
        residual: Residual::BudgetExceeded {
            resource: Resource::Complement,
            phase: Phase::Emit,
            segment: None,
            needed: 1 << 40,
            budget: 7,
        },
    })?;
    Ok(())
}

#[test]
fn a_restrict_error_projects_onto_its_detail() -> TestResult {
    let errors = [
        RestrictError::RootPruned,
        RestrictError::RowIdsExhausted,
        RestrictError::NoEdgeFound {
            src: "a".into(),
            tgt: "b".into(),
        },
    ];
    let kinds = ["root_pruned", "row_ids_exhausted", "no_edge_found"];
    for (error, kind) in errors.iter().zip(kinds) {
        let detail = error.detail();
        assert_eq!(
            detail,
            ErrorDetail::Restrict {
                key: None,
                kind: kind.into(),
                message: error.to_string()
            }
        );
        round_trip(&detail)?;
    }
    let keyed = ErrorDetail::Restrict {
        key: Some(ErrorKey {
            stage: 4,
            id: 1,
            id2: 0,
            kind: 0,
        }),
        kind: "root_pruned".into(),
        message: String::new(),
    };
    assert_eq!(
        serde_json::to_string(&keyed)?,
        r#"{"restrict":{"key":{"stage":4,"id":1,"id2":0,"kind":0},"kind":"root_pruned","message":""}}"#
    );
    round_trip(&keyed)?;
    Ok(())
}

#[test]
fn segment_error_positions_order_by_their_error_keys() {
    let k = |stage, id, id2, kind| ErrorKey {
        stage,
        id,
        id2,
        kind,
    };
    let mut keys = vec![
        k(1, 0, 0, 0),
        k(0, 9, 0, 3),
        k(0, 2, 5, 3),
        k(0, 99, 0, 1),
        k(4, 0, 0, 0),
        k(2, 1, 2, 0),
        k(2, 1, 5, 1),
        k(2, 2, 3, 0),
    ];
    keys.sort();
    // Within a stage the element decides and the kind never does: node 2
    // comes before node 9 whatever their kinds, and the pair (1, 5) before
    // the pair (2, 3).
    assert_eq!(
        keys,
        vec![
            k(0, 2, 5, 3),
            k(0, 9, 0, 3),
            k(0, 99, 0, 1),
            k(1, 0, 0, 0),
            k(2, 1, 2, 0),
            k(2, 1, 5, 1),
            k(2, 2, 3, 0),
            k(4, 0, 0, 0)
        ]
    );
    let at = |s, key| ErrorAt {
        segment: SegmentId(s),
        key,
    };
    // The first error of a whole instance is the least key, whichever
    // segment holds it.
    let firsts = [
        at(0, k(1, 4, 0, 0)),
        at(1, k(0, 7, 0, 2)),
        at(2, k(0, 3, 0, 9)),
    ];
    assert_eq!(firsts.iter().map(|a| a.key).min(), Some(k(0, 3, 0, 9)));
}

#[test]
fn error_key_bytes_are_stage_id_id2_kind() {
    let key = ErrorKey {
        stage: 2,
        id: 0x0102_0304,
        id2: 0x0506_0708,
        kind: 9,
    };
    let mut sink = panproto_schema::CanonicalWriter::new();
    key.write(&mut sink);
    let mut expected = vec![2_u8];
    expected.extend_from_slice(&0x0102_0304_u32.to_le_bytes());
    expected.extend_from_slice(&0x0506_0708_u32.to_le_bytes());
    expected.push(9);
    assert_eq!(sink.into_bytes(), expected);
}
