//! Operations assigned to canonical and preserving value I/O.
//!
//! The format-independent scalar definitions are deliberately shared with
//! the value page. Route-specific definitions are added here by the value
//! I/O proof step.

pub use crate::tabular::{missing_token_cst, one_column_empty_cst, tsv_quote_cst};
pub use crate::toml::decimal_bigint_cst;
pub use crate::value_io::*;
pub use crate::xml::{bounded_expansion_cst, expanded_name_cst};
