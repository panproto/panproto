//! Staging area (index) for the next commit.
//!
//! The index tracks a proposed schema change before it is committed.
//! The staging pipeline validates the migration and produces a
//! compatibility report.

use serde::{Deserialize, Serialize};

use crate::gat_validate::GatDiagnostics;
use crate::hash::ObjectId;

/// The staging area for the next commit.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Index {
    /// The schema staged for the next commit, if any.
    pub staged: Option<StagedSchema>,
}

/// A schema that has been staged for commit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StagedSchema {
    /// Object ID of the staged schema.
    pub schema_id: ObjectId,
    /// Object ID of the migration from HEAD's schema (if HEAD exists).
    pub migration_id: Option<ObjectId>,
    /// Whether the migration was automatically derived.
    pub auto_derived: bool,
    /// Validation status from the staging pipeline.
    pub validation: ValidationStatus,
    /// GAT-level diagnostics from type-checking and equation verification.
    #[serde(default)]
    pub gat_diagnostics: Option<GatDiagnostics>,
}

/// Result of the staging validation pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// Validation has not been run.
    Pending,
    /// The staged schema and migration are valid.
    Valid,
    /// Validation found errors.
    Invalid(Vec<String>),
}

impl Index {
    /// Returns `true` if something is staged.
    #[must_use]
    pub const fn has_staged(&self) -> bool {
        self.staged.is_some()
    }

    /// Clear the staging area.
    pub fn clear(&mut self) {
        self.staged = None;
    }
}
