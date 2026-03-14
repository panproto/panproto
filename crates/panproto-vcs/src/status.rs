//! Working state comparison (HEAD vs index vs working file).

use std::path::PathBuf;

use panproto_check::diff::SchemaDiff;

use crate::hash::ObjectId;
use crate::index::ValidationStatus;
use crate::store::HeadState;

/// Summary of repository state.
#[derive(Clone, Debug)]
pub struct Status {
    /// Current HEAD state.
    pub head_ref: HeadState,
    /// Commit that HEAD resolves to (`None` for empty repo).
    pub head_commit: Option<ObjectId>,
    /// State of the staging area, if anything is staged.
    pub staged: Option<StagedStatus>,
    /// State of the working schema file, if one exists.
    pub working: Option<WorkingStatus>,
}

/// Summary of what is staged.
#[derive(Clone, Debug)]
pub struct StagedStatus {
    /// Object ID of the staged schema.
    pub schema_id: ObjectId,
    /// Diff from HEAD's schema to the staged schema.
    pub diff_from_head: SchemaDiff,
    /// Validation result.
    pub validation: ValidationStatus,
}

/// Summary of the working schema file.
#[derive(Clone, Debug)]
pub struct WorkingStatus {
    /// Path to the working schema file.
    pub schema_path: PathBuf,
    /// Diff from HEAD's schema to the working schema.
    pub diff_from_head: Option<SchemaDiff>,
    /// Diff from the staged schema to the working schema.
    pub diff_from_staged: Option<SchemaDiff>,
}
