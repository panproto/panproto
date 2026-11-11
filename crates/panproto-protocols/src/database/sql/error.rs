//! The typed refusals of the SQL DDL reader.

/// Why the SQL DDL reader refused a script.
///
/// Every refusal names the byte offset in the script of the token it is
/// about, so a caller can point at the place. The reader stops at the
/// first refusal in source order, so the refusal of a script is
/// deterministic.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SqlDdlError {
    /// The text is not a sequence of `PostgreSQL` tokens: an unterminated
    /// literal, quoted identifier or comment, or a character no token
    /// starts with.
    #[error("SQL lexical error at byte {offset}: {reason}")]
    Lex {
        /// The byte offset of the token.
        offset: usize,
        /// What is wrong.
        reason: &'static str,
    },
    /// The tokens are not a statement of the grammar the reader reads.
    #[error("SQL syntax error at byte {offset}: expected {expected}, found {found}")]
    Syntax {
        /// The byte offset of the token found.
        offset: usize,
        /// What the grammar expects there.
        expected: &'static str,
        /// The token found, as written, or `end of statement`.
        found: String,
    },
    /// A `PostgreSQL` feature the reader does not carry, named.
    #[error("SQL feature not read at byte {offset}: {feature}")]
    Unsupported {
        /// The byte offset of the token that introduces the feature.
        offset: usize,
        /// The feature, named as `PostgreSQL` documents it.
        feature: String,
    },
    /// A column type that is neither a built-in type nor a type the script
    /// declared before it.
    #[error("unknown SQL type {name:?} at byte {offset}")]
    UnknownType {
        /// The byte offset of the type name.
        offset: usize,
        /// The type name as resolved.
        name: String,
    },
    /// A table the script has not declared at that point.
    #[error("unknown SQL table {name:?} at byte {offset}")]
    UnknownTable {
        /// The byte offset of the table name.
        offset: usize,
        /// The table name as resolved.
        name: String,
    },
    /// A column its table does not declare.
    #[error("table {table:?} has no column {column:?} (byte {offset})")]
    UnknownColumn {
        /// The byte offset of the column name.
        offset: usize,
        /// The table.
        table: String,
        /// The column.
        column: String,
    },
    /// A foreign key whose referenced columns are not exactly the columns
    /// of a primary key or unique key of the referenced table.
    #[error(
        "table {table:?} has no primary or unique key on {columns:?} for the foreign key at byte {offset}"
    )]
    NoMatchingKey {
        /// The byte offset of the referenced table's name.
        offset: usize,
        /// The referenced table.
        table: String,
        /// The referenced columns, in the order the foreign key pairs
        /// them; empty when the reference names none and the table has no
        /// primary key.
        columns: Vec<String>,
    },
    /// A name declared twice where `PostgreSQL` refuses a second
    /// declaration, or two declarations that conflict.
    #[error("duplicate SQL {what} {name:?} at byte {offset}")]
    Duplicate {
        /// The byte offset of the second declaration.
        offset: usize,
        /// What kind of name: `table`, `column`, `type`, `constraint`,
        /// `primary key`, `enum label`, `DEFAULT`, `NULL declaration`.
        what: &'static str,
        /// The name declared twice.
        name: String,
    },
}

impl SqlDdlError {
    /// The byte offset in the script the refusal names.
    #[must_use]
    pub const fn offset(&self) -> usize {
        match self {
            Self::Lex { offset, .. }
            | Self::Syntax { offset, .. }
            | Self::Unsupported { offset, .. }
            | Self::UnknownType { offset, .. }
            | Self::UnknownTable { offset, .. }
            | Self::UnknownColumn { offset, .. }
            | Self::NoMatchingKey { offset, .. }
            | Self::Duplicate { offset, .. } => *offset,
        }
    }

    /// The refusal's variant name, which tests and reports compare.
    #[must_use]
    pub const fn class(&self) -> &'static str {
        match self {
            Self::Lex { .. } => "Lex",
            Self::Syntax { .. } => "Syntax",
            Self::Unsupported { .. } => "Unsupported",
            Self::UnknownType { .. } => "UnknownType",
            Self::UnknownTable { .. } => "UnknownTable",
            Self::UnknownColumn { .. } => "UnknownColumn",
            Self::NoMatchingKey { .. } => "NoMatchingKey",
            Self::Duplicate { .. } => "Duplicate",
        }
    }
}
