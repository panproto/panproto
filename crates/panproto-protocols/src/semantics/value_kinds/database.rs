//! Value-kind rows for the database protocols (other than `sql`).

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[];
