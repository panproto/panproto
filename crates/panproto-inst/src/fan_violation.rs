//! Why a node of a graph instance is not a well-formed hyper-edge fan.
//!
//! A graph instance encodes a hyper-edge by reifying it as a fan: a node of
//! shape `HyperEdge` with one out-edge of kind `hyper_parent` to the parent
//! and one out-edge of kind `hyper_label` per labelled participant. A fan node
//! is well formed when it has exactly one `hyper_parent` out-edge, at most one
//! `hyper_label` out-edge per label, no other out-edges and no in-edges. The
//! reserved node shape and edge kinds occur nowhere else. [`FanViolation`]
//! says which of these conditions a node breaks.

use std::fmt;

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

use crate::ginstance::EdgeId;

/// The condition a would-be fan node breaks.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `"missing_parent"` or `{"reserved_kind_outside_fan": {"edge": 4}}`.
///
/// ```
/// use panproto_inst::{EdgeId, FanViolation};
///
/// let v = FanViolation::MultipleParents { edges: vec![EdgeId(1), EdgeId(2)] };
/// assert_eq!(
///     serde_json::to_string(&v).unwrap(),
///     r#"{"multiple_parents":{"edges":[1,2]}}"#,
/// );
/// assert_eq!(v.to_string(), "fan node has 2 hyper_parent out-edges: 1, 2");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanViolation {
    /// The fan node has no `hyper_parent` out-edge.
    MissingParent,
    /// The fan node has more than one `hyper_parent` out-edge.
    MultipleParents {
        /// The `hyper_parent` out-edges, ascending.
        edges: Vec<EdgeId>,
    },
    /// The fan node has more than one `hyper_label` out-edge with one label.
    DuplicateLabel {
        /// The repeated label.
        label: Name,
        /// The `hyper_label` out-edges carrying it, ascending.
        edges: Vec<EdgeId>,
    },
    /// A `hyper_label` out-edge carries a label that the hyper-edge's
    /// signature in the schema does not declare.
    UnknownLabel {
        /// The undeclared label.
        label: Name,
    },
    /// An edge of a reserved kind (`hyper_parent` or `hyper_label`) leaves a
    /// node that is not a fan node.
    ReservedKindOutsideFan {
        /// The offending edge.
        edge: EdgeId,
    },
    /// The fan node has an in-edge, or an out-edge of a kind other than
    /// `hyper_parent` and `hyper_label`.
    ForeignIncidentEdge {
        /// The offending edge.
        edge: EdgeId,
    },
}

/// Writes edge ids as a comma-separated list.
fn write_edges(f: &mut fmt::Formatter<'_>, edges: &[EdgeId]) -> fmt::Result {
    for (i, e) in edges.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{}", e.0)?;
    }
    Ok(())
}

impl fmt::Display for FanViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingParent => f.write_str("fan node has no hyper_parent out-edge"),
            Self::MultipleParents { edges } => {
                write!(f, "fan node has {} hyper_parent out-edges: ", edges.len())?;
                write_edges(f, edges)
            }
            Self::DuplicateLabel { label, edges } => {
                write!(
                    f,
                    "fan node has {} hyper_label out-edges labelled {label}: ",
                    edges.len()
                )?;
                write_edges(f, edges)
            }
            Self::UnknownLabel { label } => {
                write!(f, "fan label {label} is not in the hyper-edge's signature")
            }
            Self::ReservedKindOutsideFan { edge } => write!(
                f,
                "edge {} has a reserved fan kind but does not leave a fan node",
                edge.0
            ),
            Self::ForeignIncidentEdge { edge } => write!(
                f,
                "edge {} is incident to a fan node but is not one of its fan edges",
                edge.0
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FanViolation;
    use crate::ginstance::EdgeId;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn all() -> Vec<FanViolation> {
        vec![
            FanViolation::MissingParent,
            FanViolation::MultipleParents {
                edges: vec![EdgeId(1), EdgeId(2)],
            },
            FanViolation::DuplicateLabel {
                label: "arg0".into(),
                edges: vec![EdgeId(3), EdgeId(5)],
            },
            FanViolation::UnknownLabel {
                label: "arg9".into(),
            },
            FanViolation::ReservedKindOutsideFan { edge: EdgeId(7) },
            FanViolation::ForeignIncidentEdge { edge: EdgeId(8) },
        ]
    }

    #[test]
    fn fan_violation_serde_round_trips() -> TestResult {
        let json: Vec<String> = all()
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<_, _>>()?;
        assert_eq!(
            json,
            [
                r#""missing_parent""#,
                r#"{"multiple_parents":{"edges":[1,2]}}"#,
                r#"{"duplicate_label":{"label":"arg0","edges":[3,5]}}"#,
                r#"{"unknown_label":{"label":"arg9"}}"#,
                r#"{"reserved_kind_outside_fan":{"edge":7}}"#,
                r#"{"foreign_incident_edge":{"edge":8}}"#,
            ]
        );
        for v in all() {
            let json = serde_json::to_string(&v)?;
            assert_eq!(serde_json::from_str::<FanViolation>(&json)?, v);
            let value = serde_json::to_value(&v)?;
            assert_eq!(serde_json::from_value::<FanViolation>(value)?, v);
            let named = rmp_serde::to_vec_named(&v)?;
            assert_eq!(rmp_serde::from_slice::<FanViolation>(&named)?, v);
            let mut cbor = Vec::new();
            ciborium::into_writer(&v, &mut cbor)?;
            assert_eq!(
                ciborium::from_reader::<FanViolation, _>(cbor.as_slice())?,
                v
            );
        }
        Ok(())
    }

    #[test]
    fn fan_violation_variants_order_by_declaration() {
        let vs = all();
        assert!(vs.windows(2).all(|w| w[0] < w[1]), "{vs:?}");
    }

    #[test]
    fn fan_violation_display_names_the_edges() {
        let text: Vec<String> = all().iter().map(ToString::to_string).collect();
        assert_eq!(
            text,
            [
                "fan node has no hyper_parent out-edge",
                "fan node has 2 hyper_parent out-edges: 1, 2",
                "fan node has 2 hyper_label out-edges labelled arg0: 3, 5",
                "fan label arg9 is not in the hyper-edge's signature",
                "edge 7 has a reserved fan kind but does not leave a fan node",
                "edge 8 is incident to a fan node but is not one of its fan edges",
            ]
        );
    }
}
