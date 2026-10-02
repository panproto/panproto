//! The boundary of a schema-taking restrict or extend: the protocols its two
//! schemas are read under, and the side a boundary failure is on.
//!
//! Every schema-taking restrict and extend, in every shape, validates its
//! boundary: a restrict validates its input against the source schema under
//! `protocols.src` and its output against the target schema under
//! `protocols.tgt`; an extend validates its output against the target schema
//! under the target protocol. A report that is not `Passed` is
//! [`RestrictError::Validation`](crate::RestrictError::Validation) naming the
//! [`BoundarySide`]. The plan-taking engines (`*_with_plan`) validate
//! nothing, so that a caller holding plans can validate once and restrict
//! many times.

use panproto_schema::Protocol;
use serde::{Deserialize, Serialize};

/// The protocols the two schemas of a restrict are read under.
///
/// A schema says which vertices and edges exist; the protocol says what its
/// vertex kinds and constraint sorts mean. Validation needs both, and so
/// does the table restrict, which classifies tables and columns through the
/// protocol ([`RelSchema::new`](crate::RelSchema::new)).
///
/// ```
/// use panproto_inst::BoundaryProtocols;
/// use panproto_schema::Protocol;
///
/// let protocol = Protocol::default();
/// let both = BoundaryProtocols { src: &protocol, tgt: &protocol };
/// assert!(std::ptr::eq(both.src, both.tgt));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct BoundaryProtocols<'a> {
    /// The protocol of the source schema.
    pub src: &'a Protocol,
    /// The protocol of the target schema.
    pub tgt: &'a Protocol,
}

/// The side of a restrict or extend boundary.
///
/// Serialized in `snake_case`: `"input"` or `"output"`.
///
/// ```
/// use panproto_inst::BoundarySide;
///
/// assert_eq!(serde_json::to_string(&BoundarySide::Output)?, r#""output""#);
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundarySide {
    /// The instance handed to the operation, read under the source schema.
    Input,
    /// The instance the operation produced, read under the target schema.
    Output,
}

impl std::fmt::Display for BoundarySide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input => "input",
            Self::Output => "output",
        })
    }
}

/// The validation plan of a boundary: `schema` under `protocol`, with the
/// default options.
pub(crate) fn boundary_plan(
    schema: &panproto_schema::Schema,
    protocol: &Protocol,
) -> crate::validation::ValidationPlan {
    crate::validation::ValidationPlan::compile(
        schema,
        protocol,
        &crate::validation::ValidationOptions::default(),
    )
}

/// `Ok` when `report` passed, else the boundary failure on `side`.
pub(crate) fn require_passed(
    report: crate::validation::ValidationReport,
    side: BoundarySide,
) -> Result<(), crate::error::RestrictError> {
    if report.status == panproto_gat::CheckStatus::Passed {
        Ok(())
    } else {
        Err(crate::error::RestrictError::Validation {
            side,
            report: Box::new(report),
        })
    }
}

/// `Ok` when `plan` was compiled for the schema whose digest is `expected`,
/// else [`PlanSchemaMismatch`](crate::error::RestrictError::PlanSchemaMismatch)
/// on `side`.
pub(crate) fn require_schema(
    plan: &crate::validation::ValidationPlan,
    expected: &crate::identity::Digest,
    side: BoundarySide,
) -> Result<(), crate::error::RestrictError> {
    if plan.schema_digest() == expected {
        Ok(())
    } else {
        Err(crate::error::RestrictError::PlanSchemaMismatch { side })
    }
}

/// The default budget and no external verdicts: what a boundary validates
/// with.
pub(crate) fn unlimited() -> (
    crate::validation::ValidationBudget,
    crate::validation::ExternalVerdicts,
) {
    (
        crate::validation::ValidationBudget::default(),
        crate::validation::ExternalVerdicts::default(),
    )
}
