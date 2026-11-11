//! The locality vocabulary: how much of a partitioned instance an operation
//! must see.
//!
//! One vocabulary serves segmented execution, which classifies each clause of
//! a lens, and validation, which classifies each compiled check. A class is
//! one of four, in increasing order of what it needs:
//!
//! - [`Locality::Local`]: the segment alone, with the anchors of its ghosts
//!   (the remote endpoints of its own edges).
//! - [`Locality::Exchange`]: one round of boundary messages between segments
//!   that share edges.
//! - [`Locality::Summary`]: per-segment summaries combined by one coordinator
//!   task whose memory is bounded by the boundary or by the number of
//!   segments, not by the instance; [`SummaryKind`] says which combination.
//! - [`Locality::Global`]: not supported segment by segment. The whole-state
//!   path computes it, and a segmented run declines it before reading any
//!   segment; [`GlobalReason`] says why.
//!
//! An operation's class is the greatest class of its clauses:
//!
//! ```
//! use panproto_inst::locality::{GlobalReason, Locality, SummaryKind};
//!
//! let clauses = [
//!     Locality::Local,
//!     Locality::Summary(SummaryKind::Reach),
//!     Locality::Exchange,
//! ];
//! assert_eq!(Locality::of_clauses(clauses), Locality::Summary(SummaryKind::Reach));
//! assert_eq!(Locality::of_clauses([]), Locality::Local);
//! assert!(Locality::Summary(SummaryKind::Fold) < Locality::Global(GlobalReason::Query));
//! ```

use panproto_gat::Name;
use panproto_schema::InstanceShapeKind;
use serde::{Deserialize, Serialize};

/// How much of a partitioned instance an operation or a check must see.
///
/// The derived order is declaration order (`Local < Exchange < Summary <
/// Global`), then the payload; an operation's class is the maximum over its
/// clauses ([`Locality::join`], [`Locality::of_clauses`]). Serialized
/// externally tagged in `snake_case`, for instance `"local"` or
/// `{"summary": "sorted_merge"}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Locality {
    /// The segment alone, with its ghosts' anchors.
    Local,
    /// One round of boundary messages keyed by in-stubs and ghosts.
    Exchange,
    /// Per-segment summaries combined by one coordinator task.
    Summary(SummaryKind),
    /// Not supported segment by segment.
    Global(GlobalReason),
}

impl Locality {
    /// The class of two clauses together: the greater of the two.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        self.max(other)
    }

    /// The class of an operation made of `clauses`: the greatest clause
    /// class, and [`Locality::Local`] for an operation with no clause.
    #[must_use]
    pub fn of_clauses(clauses: impl IntoIterator<Item = Self>) -> Self {
        clauses.into_iter().fold(Self::Local, Self::join)
    }

    /// Whether a segmented run can perform the operation at all (every class
    /// but [`Locality::Global`]).
    #[must_use]
    pub const fn is_segmentable(&self) -> bool {
        !matches!(self, Self::Global(_))
    }
}

/// How per-segment summaries are combined.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryKind {
    /// Reachability over the boundary: which boundary nodes reach which, solved
    /// once from the per-segment reach summaries.
    Reach,
    /// A k-way merge of per-segment sorted runs, in memory proportional to the
    /// number of segments.
    SortedMerge,
    /// An associative fold of per-segment scalars (maxima, counts, sums).
    Fold,
}

/// Why an operation is not supported segment by segment.
///
/// Serialized externally tagged in `snake_case`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalReason {
    /// An expression anchored at `vertex` uses a graph traversal builtin,
    /// which can reach any node.
    TraversalBuiltin {
        /// The schema vertex the expression is attached to.
        vertex: Name,
    },
    /// A transform at `vertex` reads `field` as a structural child, which
    /// materializes the whole subtree below it.
    StructuralChildDemand {
        /// The schema vertex the transform is attached to.
        vertex: Name,
        /// The field it reads.
        field: String,
    },
    /// A dependent product (Pi) over the instance.
    PiProduct,
    /// Contraction witnesses of a closure contraction, whose order depends on
    /// the whole instance.
    ClosureWitnessOrder,
    /// A query over the instance.
    Query,
    /// A composite whose steps cannot be fused into one segment-wise pass.
    SequentialComposite,
}

/// An operation a segmented run can be asked to perform.
///
/// Serialized in `snake_case`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentOp {
    /// Validate the instance against its schema.
    Validate,
    /// Restrict the instance along a migration.
    Restrict,
    /// Extend the instance along a migration.
    Extend,
    /// A lens `get`.
    Get,
    /// A lens `put`.
    Put,
    /// Check that a view lies in the lens's put domain.
    CheckPutDomain,
    /// An edit lens `get` of an edit.
    GetEdit,
    /// An edit lens `put` of an edit.
    PutEdit,
    /// The instance digest.
    Digest,
}

/// The locality class of each operation of a lens on one instance shape,
/// with the clause behind each class.
///
/// `reasons` lists, for each operation, the class of each clause that
/// contributes to it with a human-readable description of the clause; each
/// of `get`, `put`, `get_edit`, `put_edit` and `validate` is the greatest class
/// its operation's reasons carry.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocalityReport {
    /// The instance shape the lens is classified on.
    pub shape: InstanceShapeKind,
    /// The class of `get`.
    pub get: Locality,
    /// The class of `put`.
    pub put: Locality,
    /// The class of the edit lens `get`.
    pub get_edit: Locality,
    /// The class of the edit lens `put`.
    pub put_edit: Locality,
    /// The class of validation.
    pub validate: Locality,
    /// Each contributing clause: the operation, the clause's class, and a
    /// description of the clause.
    pub reasons: Vec<(SegmentOp, Locality, String)>,
}

impl LocalityReport {
    /// The class of `op` as the report states it: the field for `get`,
    /// `put`, `get_edit`, `put_edit` and `validate`, and for the other
    /// operations the greatest class among their `reasons` ([`Locality::Local`]
    /// when none is listed).
    #[must_use]
    pub fn class(&self, op: SegmentOp) -> Locality {
        match op {
            SegmentOp::Get => self.get.clone(),
            SegmentOp::Put => self.put.clone(),
            SegmentOp::GetEdit => self.get_edit.clone(),
            SegmentOp::PutEdit => self.put_edit.clone(),
            SegmentOp::Validate => self.validate.clone(),
            SegmentOp::Restrict
            | SegmentOp::Extend
            | SegmentOp::CheckPutDomain
            | SegmentOp::Digest => Locality::of_clauses(
                self.reasons
                    .iter()
                    .filter(|(reason_op, _, _)| *reason_op == op)
                    .map(|(_, class, _)| class.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use panproto_schema::InstanceShapeKind;
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::{GlobalReason, Locality, LocalityReport, SegmentOp, SummaryKind};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn round_trip<T>(value: &T) -> TestResult
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let json = serde_json::to_string(value)?;
        assert_eq!(&serde_json::from_str::<T>(&json)?, value, "json");
        let dynamic = serde_json::to_value(value)?;
        assert_eq!(&serde_json::from_value::<T>(dynamic)?, value, "value");
        let named = rmp_serde::to_vec_named(value)?;
        assert_eq!(&rmp_serde::from_slice::<T>(&named)?, value, "msgpack");
        let mut cbor = Vec::new();
        ciborium::into_writer(value, &mut cbor)?;
        assert_eq!(
            &ciborium::from_reader::<T, _>(cbor.as_slice())?,
            value,
            "cbor"
        );
        Ok(())
    }

    fn all_reasons() -> Vec<GlobalReason> {
        vec![
            GlobalReason::TraversalBuiltin {
                vertex: "post".into(),
            },
            GlobalReason::StructuralChildDemand {
                vertex: "post".into(),
                field: "body".into(),
            },
            GlobalReason::PiProduct,
            GlobalReason::ClosureWitnessOrder,
            GlobalReason::Query,
            GlobalReason::SequentialComposite,
        ]
    }

    const ALL_OPS: [SegmentOp; 9] = [
        SegmentOp::Validate,
        SegmentOp::Restrict,
        SegmentOp::Extend,
        SegmentOp::Get,
        SegmentOp::Put,
        SegmentOp::CheckPutDomain,
        SegmentOp::GetEdit,
        SegmentOp::PutEdit,
        SegmentOp::Digest,
    ];

    #[test]
    fn locality_order_is_local_exchange_summary_global() {
        let ascending = [
            Locality::Local,
            Locality::Exchange,
            Locality::Summary(SummaryKind::Reach),
            Locality::Summary(SummaryKind::SortedMerge),
            Locality::Summary(SummaryKind::Fold),
            Locality::Global(GlobalReason::TraversalBuiltin { vertex: "a".into() }),
            Locality::Global(GlobalReason::SequentialComposite),
        ];
        for pair in ascending.windows(2) {
            assert!(pair[0] < pair[1], "{:?} < {:?}", pair[0], pair[1]);
        }
        let mut reasons = all_reasons();
        reasons.reverse();
        reasons.sort();
        assert_eq!(reasons, all_reasons());
    }

    #[test]
    fn an_operation_takes_the_greatest_class_of_its_clauses() {
        assert_eq!(Locality::of_clauses([]), Locality::Local);
        assert_eq!(
            Locality::of_clauses([Locality::Exchange, Locality::Local]),
            Locality::Exchange
        );
        let global = Locality::Global(GlobalReason::Query);
        assert_eq!(
            Locality::of_clauses([
                Locality::Summary(SummaryKind::Fold),
                global.clone(),
                Locality::Exchange,
            ]),
            global
        );
        assert_eq!(Locality::Exchange.join(Locality::Local), Locality::Exchange);
        assert!(!global.is_segmentable());
        assert!(Locality::Summary(SummaryKind::Reach).is_segmentable());
    }

    #[test]
    fn locality_vocabulary_serializes_in_snake_case() -> TestResult {
        assert_eq!(serde_json::to_string(&Locality::Local)?, r#""local""#);
        assert_eq!(
            serde_json::to_string(&Locality::Summary(SummaryKind::SortedMerge))?,
            r#"{"summary":"sorted_merge"}"#
        );
        assert_eq!(
            serde_json::to_string(&Locality::Global(GlobalReason::StructuralChildDemand {
                vertex: "post".into(),
                field: "body".into(),
            }))?,
            r#"{"global":{"structural_child_demand":{"vertex":"post","field":"body"}}}"#
        );
        assert_eq!(
            serde_json::to_string(&SegmentOp::CheckPutDomain)?,
            r#""check_put_domain""#
        );
        for reason in all_reasons() {
            round_trip(&Locality::Global(reason))?;
        }
        for kind in [
            SummaryKind::Reach,
            SummaryKind::SortedMerge,
            SummaryKind::Fold,
        ] {
            round_trip(&Locality::Summary(kind))?;
        }
        for op in ALL_OPS {
            round_trip(&op)?;
        }
        Ok(())
    }

    #[test]
    fn a_locality_report_round_trips_and_names_each_class() -> TestResult {
        let report = LocalityReport {
            shape: InstanceShapeKind::Graph,
            get: Locality::Exchange,
            put: Locality::Summary(SummaryKind::SortedMerge),
            get_edit: Locality::Exchange,
            put_edit: Locality::Summary(SummaryKind::SortedMerge),
            validate: Locality::Local,
            reasons: vec![
                (
                    SegmentOp::Get,
                    Locality::Exchange,
                    "edge fate from both endpoints".into(),
                ),
                (
                    SegmentOp::Digest,
                    Locality::Summary(SummaryKind::SortedMerge),
                    "instance digest".into(),
                ),
                (
                    SegmentOp::Restrict,
                    Locality::Global(GlobalReason::Query),
                    "query".into(),
                ),
            ],
        };
        round_trip(&report)?;
        // Field order is exact.
        let json = serde_json::to_string(&report)?;
        assert!(
            json.starts_with(concat!(
                r#"{"shape":"graph","get":"exchange","put":{"summary":"sorted_merge"},"#,
                r#""get_edit":"exchange","put_edit":{"summary":"sorted_merge"},"#,
                r#""validate":"local","reasons":[["get","exchange","#,
            )),
            "{json}"
        );
        assert_eq!(report.class(SegmentOp::Put), report.put);
        assert_eq!(
            report.class(SegmentOp::Digest),
            Locality::Summary(SummaryKind::SortedMerge)
        );
        assert_eq!(
            report.class(SegmentOp::Restrict),
            Locality::Global(GlobalReason::Query)
        );
        assert_eq!(report.class(SegmentOp::Extend), Locality::Local);
        assert_eq!(ALL_OPS.len(), 9);
        Ok(())
    }
}
