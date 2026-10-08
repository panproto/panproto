//! Exact tabular route edge cases.

/// A one-column empty record contains one empty cell.
pub use crate::value_io::tabular_empty_row as one_column_empty_cst;
/// `_` is data, never an implicit missing token.
pub use crate::value_io::tabular_missing as missing_token_cst;
/// TSV quotes are ordinary data.
pub use crate::value_io::tsv_quote as tsv_quote_cst;
