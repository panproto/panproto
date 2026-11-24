//! The statement model: what a DDL script says, statement by statement,
//! before any name in it is resolved.
//!
//! A reader of `PostgreSQL` source text (`panproto-io` reads it through the
//! bundled tree-sitter `PostgreSQL` grammar) produces one [`Statement`] per
//! statement, in script order, and [`read_statements`](super::read_statements)
//! resolves them against the catalog the statements before them declared.
//! Names are decoded (an unquoted name folded to lowercase, a quoted one
//! kept whole), qualified names are reduced to the object they name in the
//! `public` schema, built-in types are reduced to their [`Scalar`], and
//! every expression is kept as the source text that spells it. Each part
//! keeps the byte offset in the script of the token it starts at, which
//! the refusals name.

/// A decoded name and the byte offset of the token that spells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    /// The name as `PostgreSQL` resolves it.
    pub text: String,
    /// The byte offset of its first token.
    pub offset: usize,
}

/// A built-in scalar type, or an enum type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scalar {
    /// `smallint`, `integer`, `bigint`: the width in bits.
    Int(u8),
    /// `numeric`, with its precision and scale when declared.
    Numeric(Option<(u32, u32)>),
    /// `real`.
    Real,
    /// `double precision`.
    Double,
    /// `text`.
    Text,
    /// `character varying`, with its length when declared.
    Varchar(Option<u32>),
    /// `character(n)`.
    Character(u32),
    /// `boolean`.
    Boolean,
    /// `bytea`.
    Bytea,
    /// `date`.
    Date,
    /// `time without time zone`.
    Time,
    /// `timestamp`, with or without time zone.
    Timestamp(bool),
    /// `uuid`.
    Uuid,
    /// `json`.
    Json,
    /// `jsonb`.
    Jsonb,
    /// An enum type's labels.
    Enum(Vec<String>),
}

/// The element type a column or domain names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseType {
    /// A built-in type the reader carries.
    Builtin(Scalar),
    /// A serial pseudo-type (Sec 8.1.4), by the width of its integer.
    Serial(u8),
    /// A type the script declares: an enum or a domain.
    Named(Name),
}

/// A written type: its element type and array dimensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSpec {
    /// The element type.
    pub base: BaseType,
    /// The number of array dimensions around the element.
    pub dims: usize,
    /// The byte offset of the type's first token.
    pub offset: usize,
}

/// A referential action (`ON DELETE` and `ON UPDATE` of a foreign key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferentialAction {
    /// `NO ACTION`, the default.
    NoAction,
    /// `RESTRICT`.
    Restrict,
    /// `CASCADE`.
    Cascade,
    /// `SET NULL`.
    SetNull,
    /// `SET DEFAULT`.
    SetDefault,
}

impl ReferentialAction {
    /// The action as `PostgreSQL` spells it.
    #[must_use]
    pub const fn sql(self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }
}

/// The `REFERENCES` clause of a foreign key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct References {
    /// The referenced table.
    pub target: Name,
    /// The referenced columns as written; `None` names the primary key.
    pub target_columns: Option<Vec<Name>>,
    /// Whether the clause says `MATCH FULL`.
    pub match_full: bool,
    /// The `ON DELETE` action.
    pub on_delete: ReferentialAction,
    /// The `ON UPDATE` action.
    pub on_update: ReferentialAction,
}

/// What a column constraint declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnConstraintKind {
    /// `NOT NULL`.
    NotNull,
    /// `NULL`, which declares nothing.
    Null,
    /// `DEFAULT expr`, the expression as written.
    Default(String),
    /// `CHECK (expr)`, the expression as written.
    Check(String),
    /// `UNIQUE`.
    Unique,
    /// `PRIMARY KEY`.
    PrimaryKey,
    /// `REFERENCES ...`.
    References(References),
}

/// A column constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnConstraint {
    /// The name `CONSTRAINT name` gives it.
    pub name: Option<Name>,
    /// What it declares.
    pub kind: ColumnConstraintKind,
    /// The byte offset of its first keyword after the name.
    pub offset: usize,
}

/// A column definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnDef {
    /// The column's name.
    pub name: Name,
    /// Its type.
    pub ty: TypeSpec,
    /// Its constraints, in the order written.
    pub constraints: Vec<ColumnConstraint>,
}

/// What a table constraint declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableConstraintKind {
    /// `CHECK (expr)`, the expression as written.
    Check(String),
    /// `UNIQUE (columns)`.
    Unique(Vec<Name>),
    /// `PRIMARY KEY (columns)`.
    PrimaryKey(Vec<Name>),
    /// `FOREIGN KEY (columns) REFERENCES ...`.
    ForeignKey(Vec<Name>, References),
}

/// A table constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableConstraint {
    /// The name `CONSTRAINT name` gives it.
    pub name: Option<Name>,
    /// What it declares.
    pub kind: TableConstraintKind,
    /// The byte offset of its first keyword after the name.
    pub offset: usize,
}

/// One element of a `CREATE TABLE` body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableElement {
    /// A column.
    Column(ColumnDef),
    /// A table constraint.
    Constraint(TableConstraint),
}

/// One action of an `ALTER TABLE` the reader reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlterAction {
    /// `ADD [COLUMN] [IF NOT EXISTS] column`.
    AddColumn {
        /// Whether `IF NOT EXISTS` is written.
        if_not_exists: bool,
        /// The column.
        column: ColumnDef,
    },
    /// `ADD table_constraint`.
    AddConstraint(TableConstraint),
}

/// A domain's constraints, gathered as `CREATE DOMAIN` declares them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainConstraints {
    /// Whether `NOT NULL` is declared.
    pub not_null: bool,
    /// The `DEFAULT` expression, as written.
    pub default: Option<String>,
    /// The `CHECK` clauses: the name `CONSTRAINT name` gives each, and its
    /// expression as written.
    pub checks: Vec<(Option<Name>, String)>,
}

/// One statement of a script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// `CREATE TABLE [IF NOT EXISTS] name (elements)`.
    CreateTable {
        /// Whether `IF NOT EXISTS` is written.
        if_not_exists: bool,
        /// The table.
        name: Name,
        /// The body, in the order written.
        elements: Vec<TableElement>,
    },
    /// `CREATE UNIQUE INDEX [IF NOT EXISTS] [name] ON table (columns)`.
    CreateUniqueIndex {
        /// Whether `IF NOT EXISTS` is written.
        if_not_exists: bool,
        /// The index name, when written.
        name: Option<Name>,
        /// The indexed table.
        table: Name,
        /// The indexed columns.
        columns: Vec<Name>,
    },
    /// `CREATE DOMAIN name AS type constraints`.
    CreateDomain {
        /// The domain.
        name: Name,
        /// Its underlying type.
        ty: TypeSpec,
        /// Its constraints.
        constraints: DomainConstraints,
    },
    /// `CREATE TYPE name AS ENUM (labels)`.
    CreateEnum {
        /// The type.
        name: Name,
        /// Its labels, in order.
        labels: Vec<String>,
    },
    /// `ALTER TABLE [IF EXISTS] [ONLY] name actions`.
    AlterTable {
        /// Whether `IF EXISTS` is written.
        if_exists: bool,
        /// The table.
        table: Name,
        /// The actions, in order.
        actions: Vec<AlterAction>,
    },
    /// `DROP TABLE [IF EXISTS] names [RESTRICT]`.
    DropTable {
        /// Whether `IF EXISTS` is written.
        if_exists: bool,
        /// The tables, in order.
        names: Vec<Name>,
    },
    /// A statement that declares nothing a table schema holds.
    NotRead(StatementNotRead),
}

/// A statement the reader read past, by its class and where it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementNotRead {
    /// The statement's class, as its leading keywords spell it
    /// (`CREATE FUNCTION`, `GRANT`, `ALTER TABLE OWNER TO`).
    pub statement: &'static str,
    /// The byte offset of its first token.
    pub offset: usize,
}

/// What a script that declares statements other than its tables does with
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OtherStatements {
    /// Refuse the script at the first such statement, naming it.
    #[default]
    Refuse,
    /// Read past each one and list it in [`DdlReading::not_read`](super::DdlReading::not_read).
    List,
}

/// The longest identifier `PostgreSQL` keeps whole, in bytes (`NAMEDATALEN`
/// less one).
pub const MAX_IDENTIFIER_BYTES: usize = 63;
