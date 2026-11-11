//! The `PostgreSQL` 17 DDL statements the reader reads, into a catalog.
//!
//! The catalog is what the script declares once every statement has taken
//! effect in order: its tables with their columns, keys, foreign keys and
//! checks, and its enum and domain types. Each statement resolves the
//! names it uses against the catalog as it stands before the statement,
//! as `PostgreSQL` does, and the first statement that cannot be read
//! refuses the script.

use std::collections::{BTreeMap, BTreeSet};

use super::error::SqlDdlError;
use super::lex::{Kind, Lexer, MAX_IDENTIFIER_BYTES, Token};

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

/// A statement the reader read past, by its class and where it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementNotRead {
    /// The statement's class, as its leading keywords spell it
    /// (`CREATE FUNCTION`, `GRANT`, `ALTER TABLE OWNER TO`).
    pub statement: &'static str,
    /// The byte offset of its first token.
    pub offset: usize,
}

/// A built-in scalar type, or an enum type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Scalar {
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

/// A column's type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ColumnType {
    /// The element type.
    pub scalar: Scalar,
    /// The enum or domain type the column names, if any.
    pub user: Option<String>,
    /// A domain's `CHECK` clauses, as written.
    pub domain_checks: Vec<String>,
    /// The number of array dimensions around the element.
    pub dims: usize,
}

/// A column.
#[derive(Clone, Debug)]
pub(super) struct Column {
    pub name: String,
    pub ty: ColumnType,
    pub not_null: bool,
    /// The `DEFAULT` expression, as written.
    pub default: Option<String>,
}

/// Whether a key is the primary key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    Primary,
    Unique,
}

/// A primary key, unique constraint or unique index.
#[derive(Clone, Debug)]
pub(super) struct KeyDecl {
    pub name: String,
    pub role: Role,
    pub columns: Vec<String>,
}

/// A referential action (Sec "ON DELETE" of `CREATE TABLE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull,
    SetDefault,
}

impl ReferentialAction {
    pub(super) const fn sql(self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }
}

/// A foreign key, its column pairs in the order of the target key's parts.
#[derive(Clone, Debug)]
pub(super) struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub target: String,
    pub target_key: String,
    pub match_full: bool,
    pub on_delete: ReferentialAction,
    pub on_update: ReferentialAction,
}

/// A table.
#[derive(Clone, Debug, Default)]
pub(super) struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    /// `CHECK` clauses, column checks included, as
    /// `[CONSTRAINT name] CHECK (expr)`.
    pub checks: Vec<String>,
    /// Names of the table's named `CHECK` constraints.
    pub check_names: BTreeSet<String>,
    pub keys: Vec<KeyDecl>,
    pub foreign_keys: Vec<ForeignKey>,
}

impl Table {
    pub(super) fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// A `CREATE DOMAIN`.
#[derive(Clone, Debug)]
struct Domain {
    scalar: Scalar,
    not_null: bool,
    default: Option<String>,
    checks: Vec<String>,
}

/// An enum or domain type.
#[derive(Clone, Debug)]
enum UserType {
    Enum(Vec<String>),
    Domain(Domain),
}

/// What a script declares.
#[derive(Debug, Default)]
pub(super) struct Catalog {
    pub tables: Vec<Table>,
    types: BTreeMap<String, UserType>,
    /// Index-backed key names, which share the namespace of tables.
    relations: BTreeSet<String>,
    pub not_read: Vec<StatementNotRead>,
}

/// Read a script into its catalog.
pub(super) fn read(src: &str, others: OtherStatements) -> Result<Catalog, SqlDdlError> {
    let mut catalog = Catalog::default();
    let mut lexer = Lexer::new(src);
    while let Some(statement) = lexer.statement()? {
        let mut p = Parser {
            src,
            toks: &statement.tokens,
            i: 0,
            end: statement.end,
        };
        if let Some(class) = p.statement(&mut catalog)? {
            let offset = statement.tokens[0].start;
            match others {
                OtherStatements::Refuse => {
                    return Err(SqlDdlError::Unsupported {
                        offset,
                        feature: class.to_owned(),
                    });
                }
                OtherStatements::List => catalog.not_read.push(StatementNotRead {
                    statement: class,
                    offset,
                }),
            }
        }
    }
    Ok(catalog)
}

/// Statements that declare nothing a table schema holds, by their leading
/// words. They are listed or refused, never read.
const OTHER_STATEMENTS: &[(&[&str], &str)] = &[
    (&["set"], "SET"),
    (&["reset"], "RESET"),
    (&["select"], "SELECT"),
    (&["grant"], "GRANT"),
    (&["revoke"], "REVOKE"),
    (&["comment", "on"], "COMMENT ON"),
    (&["begin"], "BEGIN"),
    (&["commit"], "COMMIT"),
    (&["start", "transaction"], "START TRANSACTION"),
    (&["copy"], "COPY"),
    (&["insert"], "INSERT"),
    (&["create", "index"], "CREATE INDEX"),
    (&["create", "view"], "CREATE VIEW"),
    (&["create", "or", "replace", "view"], "CREATE VIEW"),
    (
        &["create", "materialized", "view"],
        "CREATE MATERIALIZED VIEW",
    ),
    (&["create", "function"], "CREATE FUNCTION"),
    (&["create", "or", "replace", "function"], "CREATE FUNCTION"),
    (&["create", "procedure"], "CREATE PROCEDURE"),
    (
        &["create", "or", "replace", "procedure"],
        "CREATE PROCEDURE",
    ),
    (&["create", "trigger"], "CREATE TRIGGER"),
    (&["create", "or", "replace", "trigger"], "CREATE TRIGGER"),
    (&["create", "rule"], "CREATE RULE"),
    (&["create", "or", "replace", "rule"], "CREATE RULE"),
    (&["create", "aggregate"], "CREATE AGGREGATE"),
    (
        &["create", "or", "replace", "aggregate"],
        "CREATE AGGREGATE",
    ),
    (&["create", "sequence"], "CREATE SEQUENCE"),
    (&["create", "extension"], "CREATE EXTENSION"),
    (&["create", "language"], "CREATE LANGUAGE"),
    (&["create", "procedural", "language"], "CREATE LANGUAGE"),
    (&["create", "or", "replace", "language"], "CREATE LANGUAGE"),
    (
        &["create", "or", "replace", "procedural", "language"],
        "CREATE LANGUAGE",
    ),
    (&["create", "schema"], "CREATE SCHEMA"),
    (&["create", "type"], "CREATE TYPE"),
    (&["alter", "function"], "ALTER FUNCTION"),
    (&["alter", "procedure"], "ALTER PROCEDURE"),
    (&["alter", "aggregate"], "ALTER AGGREGATE"),
    (&["alter", "sequence"], "ALTER SEQUENCE"),
    (&["alter", "view"], "ALTER VIEW"),
    (&["alter", "schema"], "ALTER SCHEMA"),
    (&["alter", "language"], "ALTER LANGUAGE"),
    (&["alter", "procedural", "language"], "ALTER LANGUAGE"),
];

/// Built-in `PostgreSQL` 17 types (Chapter 8) the reader does not carry.
const UNSUPPORTED_TYPES: &[&str] = &[
    "money",
    "interval",
    "timetz",
    "xml",
    "inet",
    "cidr",
    "macaddr",
    "macaddr8",
    "point",
    "line",
    "lseg",
    "box",
    "path",
    "polygon",
    "circle",
    "bit",
    "varbit",
    "tsvector",
    "tsquery",
    "pg_lsn",
    "pg_snapshot",
    "txid_snapshot",
    "int4range",
    "int8range",
    "numrange",
    "tsrange",
    "tstzrange",
    "daterange",
    "int4multirange",
    "int8multirange",
    "nummultirange",
    "tsmultirange",
    "tstzmultirange",
    "datemultirange",
    "oid",
    "regclass",
    "regtype",
    "regproc",
    "name",
    "char",
    "jsonpath",
    "record",
];

/// The type names a quoted identifier may spell: the catalog names of the
/// built-in types the reader carries.
const CATALOG_TYPE_NAMES: &[&str] = &[
    "int2",
    "int4",
    "int8",
    "numeric",
    "float4",
    "float8",
    "text",
    "varchar",
    "bpchar",
    "bool",
    "bytea",
    "date",
    "time",
    "timestamp",
    "timestamptz",
    "uuid",
    "json",
    "jsonb",
];

/// A recursive-descent reader over one statement's tokens.
struct Parser<'s, 't> {
    src: &'s str,
    toks: &'t [Token],
    i: usize,
    end: usize,
}

fn unsupported(offset: usize, feature: impl Into<String>) -> SqlDdlError {
    SqlDdlError::Unsupported {
        offset,
        feature: feature.into(),
    }
}

impl Parser<'_, '_> {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.i)
    }

    fn peek_at(&self, ahead: usize) -> Option<&Token> {
        self.toks.get(self.i + ahead)
    }

    /// The offset of the current token, or of the statement's end.
    fn offset(&self) -> usize {
        self.peek().map_or(self.end, |t| t.start)
    }

    fn syntax(&self, expected: &'static str) -> SqlDdlError {
        SqlDdlError::Syntax {
            offset: self.offset(),
            expected,
            found: self.peek().map_or_else(
                || "end of statement".to_owned(),
                |t| self.src[t.start..t.end].to_owned(),
            ),
        }
    }

    fn at_word(&self, word: &str) -> bool {
        self.peek().is_some_and(|t| t.is_word(word))
    }

    fn at_words(&self, words: &[&str]) -> bool {
        words
            .iter()
            .enumerate()
            .all(|(k, w)| self.peek_at(k).is_some_and(|t| t.is_word(w)))
    }

    fn at_punct(&self, p: &str) -> bool {
        self.peek().is_some_and(|t| t.is_punct(p))
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let found = self.at_word(word);
        if found {
            self.i += 1;
        }
        found
    }

    fn eat_words(&mut self, words: &[&str]) -> bool {
        let found = self.at_words(words);
        if found {
            self.i += words.len();
        }
        found
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        let found = self.at_punct(p);
        if found {
            self.i += 1;
        }
        found
    }

    fn expect_word(&mut self, word: &str, expected: &'static str) -> Result<(), SqlDdlError> {
        if self.eat_word(word) {
            Ok(())
        } else {
            Err(self.syntax(expected))
        }
    }

    fn expect_punct(&mut self, p: &str, expected: &'static str) -> Result<(), SqlDdlError> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            Err(self.syntax(expected))
        }
    }

    fn expect_end(&self) -> Result<(), SqlDdlError> {
        if self.peek().is_none() {
            Ok(())
        } else {
            Err(self.syntax("the end of the statement"))
        }
    }

    /// One identifier: an unquoted word or a quoted identifier.
    fn identifier(&mut self) -> Result<(String, usize), SqlDdlError> {
        let Some(t) = self.peek() else {
            return Err(self.syntax("a name"));
        };
        if t.kind == Kind::Punct && t.text == "[" {
            return Err(unsupported(t.start, "T-SQL bracket-quoted identifier"));
        }
        if !matches!(t.kind, Kind::Word | Kind::Quoted) {
            return Err(self.syntax("a name"));
        }
        let (name, offset) = (t.text.clone(), t.start);
        if name.len() > MAX_IDENTIFIER_BYTES {
            return Err(unsupported(offset, "identifier longer than 63 bytes"));
        }
        if name.contains(['.', ':']) {
            return Err(unsupported(offset, "identifier containing `.` or `:`"));
        }
        self.i += 1;
        Ok((name, offset))
    }

    /// A possibly qualified object name: `name` or `public.name`.
    fn object_name(&mut self) -> Result<(String, usize), SqlDdlError> {
        let (first, offset) = self.identifier()?;
        if !self.eat_punct(".") {
            return Ok((first, offset));
        }
        let (second, at) = self.identifier()?;
        if self.at_punct(".") {
            return Err(unsupported(offset, "a name qualified by a database"));
        }
        if first != "public" {
            return Err(unsupported(offset, "a schema other than public"));
        }
        Ok((second, at))
    }

    /// A parenthesized list of column names.
    fn column_list(&mut self) -> Result<Vec<(String, usize)>, SqlDdlError> {
        self.expect_punct("(", "`(` opening a column list")?;
        let mut out = Vec::new();
        loop {
            out.push(self.identifier()?);
            if self.eat_punct(")") {
                return Ok(out);
            }
            self.expect_punct(",", "`,` or `)` in a column list")?;
        }
    }

    /// The source text of a parenthesized expression, without its
    /// parentheses; the parser stands after the `)`.
    fn parenthesized(&mut self) -> Result<String, SqlDdlError> {
        self.expect_punct("(", "`(` opening an expression")?;
        let first = self.i;
        let mut depth = 1usize;
        while let Some(t) = self.peek() {
            if t.is_punct("(") {
                depth += 1;
            } else if t.is_punct(")") {
                depth -= 1;
                if depth == 0 {
                    if self.i == first {
                        return Err(self.syntax("an expression"));
                    }
                    let text =
                        self.src[self.toks[first].start..self.toks[self.i - 1].end].to_owned();
                    self.i += 1;
                    return Ok(text);
                }
            }
            self.i += 1;
        }
        Err(self.syntax("`)` closing an expression"))
    }

    /// The source text of a `DEFAULT` expression: the tokens up to a `,`
    /// or `)` outside brackets, or a word that starts another column
    /// constraint.
    fn default_expression(&mut self) -> Result<String, SqlDdlError> {
        const STOPS: &[&str] = &[
            "constraint",
            "check",
            "default",
            "unique",
            "primary",
            "references",
            "generated",
            "collate",
            "deferrable",
            "initially",
        ];
        let first = self.i;
        let mut depth = 0usize;
        while let Some(t) = self.peek() {
            if t.is_punct("(") || t.is_punct("[") {
                depth += 1;
            } else if t.is_punct(")") || t.is_punct("]") {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if depth == 0 && self.i > first {
                let after_is = self.toks[self.i - 1].is_word("is");
                if t.is_punct(",")
                    || STOPS.iter().any(|w| t.is_word(w))
                    || (t.is_word("not") || t.is_word("null")) && !after_is
                {
                    break;
                }
            } else if depth == 0 && t.is_punct(",") {
                break;
            }
            self.i += 1;
        }
        if self.i == first {
            return Err(self.syntax("a DEFAULT expression"));
        }
        Ok(self.src[self.toks[first].start..self.toks[self.i - 1].end].to_owned())
    }

    /// An unsigned integer constant.
    fn integer(&mut self) -> Result<(u32, usize), SqlDdlError> {
        let Some(t) = self.peek() else {
            return Err(self.syntax("an integer"));
        };
        let offset = t.start;
        let value = (t.kind == Kind::Num)
            .then(|| t.text.parse::<u32>().ok())
            .flatten()
            .ok_or_else(|| self.syntax("an integer"))?;
        self.i += 1;
        Ok((value, offset))
    }

    /// An optional `(n)` type modifier.
    fn modifier(&mut self) -> Result<Option<(u32, usize)>, SqlDdlError> {
        if !self.eat_punct("(") {
            return Ok(None);
        }
        let n = self.integer()?;
        self.expect_punct(")", "`)` closing a type modifier")?;
        Ok(Some(n))
    }

    /// The statement at the start of the tokens; `Some(class)` for a
    /// statement the reader does not read.
    fn statement(&mut self, catalog: &mut Catalog) -> Result<Option<&'static str>, SqlDdlError> {
        if self.at_words(&["create", "table"]) {
            self.i += 2;
            self.create_table(catalog)?;
        } else if self.at_words(&["create", "unique", "index"]) {
            self.i += 3;
            self.create_unique_index(catalog)?;
        } else if self.at_words(&["create", "domain"]) {
            self.i += 2;
            self.create_domain(catalog)?;
        } else if self.at_words(&["create", "type"]) && self.is_enum_type() {
            self.i += 2;
            self.create_enum(catalog)?;
        } else if self.at_words(&["alter", "table"]) {
            self.i += 2;
            return self.alter_table(catalog);
        } else if self.at_words(&["drop", "table"]) {
            self.i += 2;
            self.drop_table(catalog)?;
        } else if self.at_word("create")
            && ["temp", "temporary", "unlogged", "global", "local"]
                .iter()
                .any(|w| self.peek_at(1).is_some_and(|t| t.is_word(w)))
        {
            return Err(unsupported(
                self.offset(),
                "CREATE TEMPORARY or UNLOGGED TABLE",
            ));
        } else if self.is_owner_change(&["type", "domain"]) {
            return Ok(Some("ALTER OWNER TO"));
        } else if let Some(class) = self.other_statement()? {
            return Ok(Some(class));
        } else {
            let start = self.offset();
            let words: Vec<String> = self
                .toks
                .iter()
                .take(3)
                .take_while(|t| t.kind == Kind::Word)
                .map(|t| t.text.to_ascii_uppercase())
                .collect();
            return Err(unsupported(start, format!("statement {}", words.join(" "))));
        }
        Ok(None)
    }

    /// The class of a statement the reader reads past.
    fn other_statement(&self) -> Result<Option<&'static str>, SqlDdlError> {
        let class = OTHER_STATEMENTS
            .iter()
            .filter(|(words, _)| self.at_words(words))
            .max_by_key(|(words, _)| words.len())
            .map(|(_, class)| *class);
        // A statement that sets the schema search path changes how the
        // names after it resolve, unless it sets it to `public` or empty.
        if matches!(class, Some("SET" | "SELECT")) && self.sets_another_search_path() {
            return Err(unsupported(
                self.offset(),
                "a search_path other than public",
            ));
        }
        Ok(class)
    }

    /// Whether this `SET search_path ...` or `SELECT
    /// pg_catalog.set_config('search_path', ...)` names a schema other
    /// than `public` and `pg_catalog`.
    fn sets_another_search_path(&self) -> bool {
        let mentions = self
            .toks
            .iter()
            .any(|t| t.is_word("search_path") || t.kind == Kind::Str && t.text == "search_path");
        if !mentions {
            return false;
        }
        self.toks.iter().skip(1).any(|t| match t.kind {
            Kind::Word | Kind::Quoted => ![
                "search_path",
                "to",
                "public",
                "pg_catalog",
                "set_config",
                "false",
                "true",
                "session",
                "local",
            ]
            .contains(&t.text.as_str()),
            Kind::Str => t
                .text
                .split(',')
                .map(str::trim)
                .any(|s| !["", "search_path", "public", "pg_catalog"].contains(&s)),
            _ => false,
        })
    }

    /// Whether the statement is `ALTER <kind> name OWNER TO role` for one
    /// of `kinds`.
    fn is_owner_change(&self, kinds: &[&str]) -> bool {
        self.at_word("alter")
            && kinds
                .iter()
                .any(|k| self.peek_at(1).is_some_and(|t| t.is_word(k)))
            && self.toks.len() >= 4
            && self.toks[self.toks.len() - 3].is_word("owner")
            && self.toks[self.toks.len() - 2].is_word("to")
    }

    /// Whether this `CREATE TYPE` declares an enum.
    fn is_enum_type(&self) -> bool {
        self.toks
            .windows(2)
            .any(|w| w[0].is_word("as") && w[1].is_word("enum"))
    }

    fn create_enum(&mut self, catalog: &mut Catalog) -> Result<(), SqlDdlError> {
        let (name, at) = self.object_name()?;
        self.expect_word("as", "AS")?;
        self.expect_word("enum", "ENUM")?;
        self.expect_punct("(", "`(` opening the enum labels")?;
        let mut labels: Vec<String> = Vec::new();
        if !self.eat_punct(")") {
            loop {
                let Some(t) = self.peek() else {
                    return Err(self.syntax("an enum label"));
                };
                if t.kind != Kind::Str || !t.decoded || t.text.len() > MAX_IDENTIFIER_BYTES {
                    return Err(self.syntax("an enum label: a string of at most 63 bytes"));
                }
                if labels.contains(&t.text) {
                    return Err(SqlDdlError::Duplicate {
                        offset: t.start,
                        what: "enum label",
                        name: t.text.clone(),
                    });
                }
                labels.push(t.text.clone());
                self.i += 1;
                if self.eat_punct(")") {
                    break;
                }
                self.expect_punct(",", "`,` or `)` after an enum label")?;
            }
        }
        self.expect_end()?;
        catalog.declare_type(name, at, UserType::Enum(labels))
    }

    fn create_domain(&mut self, catalog: &mut Catalog) -> Result<(), SqlDdlError> {
        let (name, at) = self.object_name()?;
        self.eat_word("as");
        let type_at = self.offset();
        let ty = self.column_type(catalog)?;
        if ty.user.is_some() || ty.dims > 0 {
            return Err(unsupported(
                type_at,
                "a domain over an array, enum or domain type",
            ));
        }
        let mut domain = Domain {
            scalar: ty.scalar,
            not_null: false,
            default: None,
            checks: Vec::new(),
        };
        let mut null_declared = false;
        while self.peek().is_some() {
            let name = if self.eat_word("constraint") {
                Some(self.identifier()?.0)
            } else {
                None
            };
            let at = self.offset();
            if self.at_words(&["not", "null"]) || self.at_word("null") {
                if null_declared {
                    return Err(duplicate(at, "NULL declaration", "NULL"));
                }
                null_declared = true;
                domain.not_null = self.eat_words(&["not", "null"]);
                self.eat_word("null");
            } else if self.eat_word("check") {
                let expr = self.parenthesized()?;
                domain.checks.push(check_clause(name.as_deref(), &expr));
            } else if name.is_none() && self.eat_word("default") {
                if domain.default.is_some() {
                    return Err(duplicate(at, "DEFAULT", "DEFAULT"));
                }
                domain.default = Some(self.default_expression()?);
            } else if self.at_word("collate") {
                return Err(unsupported(at, "COLLATE"));
            } else {
                return Err(self.syntax("a domain constraint"));
            }
        }
        catalog.declare_type(name, at, UserType::Domain(domain))
    }

    /// A column type: a built-in type, an enum or a domain, with array
    /// dimensions.
    fn column_type(&mut self, catalog: &Catalog) -> Result<ColumnType, SqlDdlError> {
        let at = self.offset();
        let Some(t) = self.peek() else {
            return Err(self.syntax("a type"));
        };
        if t.kind == Kind::Punct && t.text == "[" {
            return Err(unsupported(at, "T-SQL bracket-quoted identifier"));
        }
        let quoted = t.kind == Kind::Quoted;
        let (mut name, _) = self.identifier()?;
        if self.eat_punct(".") {
            let (second, _) = self.identifier()?;
            if !matches!(name.as_str(), "public" | "pg_catalog") {
                return Err(unsupported(at, "a schema other than public"));
            }
            name = second;
        }
        let scalar = if quoted && !CATALOG_TYPE_NAMES.contains(&name.as_str()) {
            None
        } else {
            self.builtin(&name, at)?
        };
        let mut ty = match scalar {
            Some(Builtin::Plain(scalar)) => ColumnType {
                scalar,
                user: None,
                domain_checks: Vec::new(),
                dims: 0,
            },
            Some(Builtin::Serial) => {
                return Err(unsupported(at, "a serial type outside a column definition"));
            }
            None => match catalog.types.get(&name) {
                Some(UserType::Enum(labels)) => ColumnType {
                    scalar: Scalar::Enum(labels.clone()),
                    user: Some(name),
                    domain_checks: Vec::new(),
                    dims: 0,
                },
                Some(UserType::Domain(domain)) => ColumnType {
                    scalar: domain.scalar.clone(),
                    user: Some(name),
                    domain_checks: domain.checks.clone(),
                    dims: 0,
                },
                None if UNSUPPORTED_TYPES.contains(&name.as_str()) => {
                    return Err(unsupported(at, format!("type {name}")));
                }
                None => return Err(SqlDdlError::UnknownType { offset: at, name }),
            },
        };
        ty.dims = self.array_dimensions()?;
        Ok(ty)
    }

    /// `[]`, `[n]` repeated, or `ARRAY` / `ARRAY[n]`: the number of
    /// dimensions declared.
    fn array_dimensions(&mut self) -> Result<usize, SqlDdlError> {
        if self.eat_word("array") {
            if self.eat_punct("[") {
                self.integer()?;
                self.expect_punct("]", "`]` closing an array bound")?;
            }
            return Ok(1);
        }
        let mut dims = 0;
        while self.eat_punct("[") {
            if !self.eat_punct("]") {
                self.integer()?;
                self.expect_punct("]", "`]` closing an array bound")?;
            }
            dims += 1;
        }
        Ok(dims)
    }

    /// The built-in type whose (first) word `name` the parser has just
    /// read, reading the words and modifiers after it; `None` when `name`
    /// names no built-in type the reader carries.
    fn builtin(&mut self, name: &str, at: usize) -> Result<Option<Builtin>, SqlDdlError> {
        let plain = |s: Scalar| Ok(Some(Builtin::Plain(s)));
        let no_modifier = |p: &mut Self| -> Result<(), SqlDdlError> {
            if p.at_punct("(") {
                Err(p.syntax("no type modifier"))
            } else {
                Ok(())
            }
        };
        match name {
            "smallint" | "int2" => no_modifier(self).and_then(|()| plain(Scalar::Int(16))),
            "integer" | "int" | "int4" => no_modifier(self).and_then(|()| plain(Scalar::Int(32))),
            "bigint" | "int8" => no_modifier(self).and_then(|()| plain(Scalar::Int(64))),
            "smallserial" | "serial2" => Ok(Some(Builtin::Serial)),
            "serial" | "serial4" => Ok(Some(Builtin::Serial)),
            "bigserial" | "serial8" => Ok(Some(Builtin::Serial)),
            "real" | "float4" => no_modifier(self).and_then(|()| plain(Scalar::Real)),
            "float8" => no_modifier(self).and_then(|()| plain(Scalar::Double)),
            "double" => {
                self.expect_word("precision", "PRECISION after DOUBLE")?;
                plain(Scalar::Double)
            }
            "float" => match self.modifier()? {
                None => plain(Scalar::Double),
                Some((1..=24, _)) => plain(Scalar::Real),
                Some((25..=53, _)) => plain(Scalar::Double),
                Some((_, at)) => Err(SqlDdlError::Syntax {
                    offset: at,
                    expected: "a float precision from 1 to 53",
                    found: self.src[at..self.toks[self.i - 2].end].to_owned(),
                }),
            },
            "numeric" | "decimal" => Ok(Some(Builtin::Plain(self.numeric()?))),
            "text" => no_modifier(self).and_then(|()| plain(Scalar::Text)),
            "varchar" => Ok(Some(Builtin::Plain(Scalar::Varchar(self.length()?)))),
            "national" => {
                if !(self.eat_word("character") || self.eat_word("char")) {
                    return Err(self.syntax("CHARACTER after NATIONAL"));
                }
                self.character()
            }
            "character" | "char" => self.character(),
            "bpchar" => match self.length()? {
                Some(n) => plain(Scalar::Character(n)),
                None => Err(unsupported(at, "type bpchar without a length")),
            },
            "boolean" | "bool" => no_modifier(self).and_then(|()| plain(Scalar::Boolean)),
            "bytea" => no_modifier(self).and_then(|()| plain(Scalar::Bytea)),
            "date" => no_modifier(self).and_then(|()| plain(Scalar::Date)),
            "time" => {
                self.no_fractional_precision()?;
                if self.eat_words(&["with", "time", "zone"]) {
                    return Err(unsupported(at, "type time with time zone"));
                }
                self.eat_words(&["without", "time", "zone"]);
                plain(Scalar::Time)
            }
            "timestamp" => {
                self.no_fractional_precision()?;
                if self.eat_words(&["with", "time", "zone"]) {
                    return plain(Scalar::Timestamp(true));
                }
                self.eat_words(&["without", "time", "zone"]);
                plain(Scalar::Timestamp(false))
            }
            "timestamptz" => {
                self.no_fractional_precision()?;
                plain(Scalar::Timestamp(true))
            }
            "uuid" => no_modifier(self).and_then(|()| plain(Scalar::Uuid)),
            "json" => no_modifier(self).and_then(|()| plain(Scalar::Json)),
            "jsonb" => no_modifier(self).and_then(|()| plain(Scalar::Jsonb)),
            "bit" | "interval" => Err(unsupported(at, format!("type {name}"))),
            _ => Ok(None),
        }
    }

    /// The modifier of `numeric` after the word: `(p)` or `(p, s)`.
    fn numeric(&mut self) -> Result<Scalar, SqlDdlError> {
        if !self.eat_punct("(") {
            return Ok(Scalar::Numeric(None));
        }
        let (precision, p_at) = self.integer()?;
        if !(1..=1000).contains(&precision) {
            return Err(SqlDdlError::Syntax {
                offset: p_at,
                expected: "a numeric precision from 1 to 1000",
                found: precision.to_string(),
            });
        }
        let scale = if self.eat_punct(",") {
            if self.at_punct("-") {
                return Err(unsupported(self.offset(), "numeric with a negative scale"));
            }
            let (scale, s_at) = self.integer()?;
            if scale > precision {
                return Err(unsupported(
                    s_at,
                    "numeric with a scale above its precision",
                ));
            }
            scale
        } else {
            0
        };
        self.expect_punct(")", "`)` closing a numeric modifier")?;
        Ok(Scalar::Numeric(Some((precision, scale))))
    }

    /// `CHARACTER [VARYING] [(n)]` after its first word.
    fn character(&mut self) -> Result<Option<Builtin>, SqlDdlError> {
        let varying = self.eat_word("varying");
        let length = self.length()?;
        if varying {
            return Ok(Some(Builtin::Plain(Scalar::Varchar(length))));
        }
        Ok(Some(Builtin::Plain(Scalar::Character(length.unwrap_or(1)))))
    }

    /// An optional length modifier from 1 to 10485760.
    fn length(&mut self) -> Result<Option<u32>, SqlDdlError> {
        match self.modifier()? {
            Some((n @ 1..=10_485_760, _)) => Ok(Some(n)),
            Some((n, at)) => Err(SqlDdlError::Syntax {
                offset: at,
                expected: "a length from 1 to 10485760",
                found: n.to_string(),
            }),
            None => Ok(None),
        }
    }

    fn no_fractional_precision(&self) -> Result<(), SqlDdlError> {
        if self.at_punct("(") {
            return Err(unsupported(self.offset(), "fractional-second precision"));
        }
        Ok(())
    }

    fn create_table(&mut self, catalog: &mut Catalog) -> Result<(), SqlDdlError> {
        let if_not_exists = self.eat_words(&["if", "not", "exists"]);
        let (name, at) = self.object_name()?;
        if self.at_word("of") {
            return Err(unsupported(self.offset(), "typed table (OF type)"));
        }
        if self.at_word("partition") {
            return Err(unsupported(self.offset(), "PARTITION OF"));
        }
        if self.at_word("as") {
            return Err(unsupported(self.offset(), "CREATE TABLE AS"));
        }
        if catalog.table_index(&name).is_some() || catalog.relations.contains(&name) {
            if if_not_exists {
                return Ok(());
            }
            return Err(duplicate(at, "table", &name));
        }
        if catalog.types.contains_key(&name) {
            return Err(duplicate(at, "type", &name));
        }
        self.expect_punct("(", "`(` opening the table's columns")?;
        let mut table = Table {
            name,
            ..Table::default()
        };
        let mut pending = Pending::default();
        if !self.eat_punct(")") {
            loop {
                self.table_element(catalog, &mut table, &mut pending)?;
                if self.eat_punct(")") {
                    break;
                }
                self.expect_punct(",", "`,` or `)` after a table element")?;
            }
        }
        if let Some(t) = self.peek() {
            let feature = match t.text.as_str() {
                "inherits" => "INHERITS",
                "partition" => "PARTITION BY",
                "using" => "USING (table access method)",
                "with" | "without" => "table storage parameters",
                "on" => "ON COMMIT",
                "tablespace" => "TABLESPACE",
                _ => return Err(self.syntax("the end of CREATE TABLE")),
            };
            return Err(unsupported(t.start, feature));
        }
        catalog.tables.push(table);
        let index = catalog.tables.len() - 1;
        catalog.apply(index, pending)
    }

    /// One element of a table body: a column or a table constraint.
    fn table_element(
        &mut self,
        catalog: &Catalog,
        table: &mut Table,
        pending: &mut Pending,
    ) -> Result<(), SqlDdlError> {
        let constraint_start = self.at_word("constraint")
            || self.at_word("check")
            || self.at_word("unique")
            || self.at_words(&["primary", "key"])
            || self.at_words(&["foreign", "key"])
            || self.at_word("exclude")
                && self
                    .peek_at(1)
                    .is_some_and(|t| t.is_punct("(") || t.is_word("using"));
        if constraint_start {
            return self.table_constraint(pending);
        }
        if self.at_word("like") {
            return Err(unsupported(self.offset(), "LIKE"));
        }
        let column = self.column_definition(catalog, &table.name, pending)?;
        if table.column(&column.0.name).is_some() {
            return Err(duplicate(column.1, "column", &column.0.name));
        }
        table.columns.push(column.0);
        Ok(())
    }

    /// A column definition: name, type and column constraints. Its keys,
    /// checks and foreign keys go to `pending`.
    fn column_definition(
        &mut self,
        catalog: &Catalog,
        table: &str,
        pending: &mut Pending,
    ) -> Result<(Column, usize), SqlDdlError> {
        let (name, at) = self.identifier()?;
        let type_at = self.offset();
        let serial = match self.peek() {
            Some(t) if t.kind == Kind::Word => match t.text.as_str() {
                "smallserial" | "serial2" => Some(16),
                "serial" | "serial4" => Some(32),
                "bigserial" | "serial8" => Some(64),
                _ => None,
            },
            _ => None,
        };
        let mut column = if let Some(bits) = serial {
            self.i += 1;
            self.serial_column(table, &name, bits, type_at)?
        } else {
            Self::typed_column(self.column_type(catalog)?, catalog, &name)
        };
        let mut null_declared = false;
        let mut default_declared = false;
        while let Some(t) = self.peek() {
            if t.is_punct(",") || t.is_punct(")") {
                break;
            }
            let constraint_name = if self.eat_word("constraint") {
                Some(self.identifier()?)
            } else {
                None
            };
            let at = self.offset();
            if self.at_words(&["not", "null"]) || self.at_word("null") {
                if null_declared {
                    return Err(duplicate(at, "NULL declaration", &name));
                }
                null_declared = true;
                // `NULL` declares nothing: it neither lifts a domain's
                // `NOT NULL` nor a serial column's.
                if self.eat_words(&["not", "null"]) {
                    column.not_null = true;
                }
                self.eat_word("null");
            } else if self.eat_word("default") {
                if default_declared || serial.is_some() {
                    return Err(duplicate(at, "DEFAULT", &name));
                }
                default_declared = true;
                column.default = Some(self.default_expression()?);
            } else if self.eat_word("check") {
                let expr = self.parenthesized()?;
                if self.at_words(&["no", "inherit"]) {
                    return Err(unsupported(self.offset(), "NO INHERIT"));
                }
                pending.checks.push((constraint_name.clone(), expr));
            } else if self.eat_word("unique") {
                self.unique_options()?;
                pending.keys.push(PendingKey {
                    name: constraint_name.clone(),
                    role: Role::Unique,
                    columns: vec![(name.clone(), at)],
                    at,
                });
            } else if self.eat_words(&["primary", "key"]) {
                self.index_parameters()?;
                pending.keys.push(PendingKey {
                    name: constraint_name.clone(),
                    role: Role::Primary,
                    columns: vec![(name.clone(), at)],
                    at,
                });
            } else if self.eat_word("references") {
                let fk = self.references(constraint_name.clone(), vec![(name.clone(), at)])?;
                pending.foreign_keys.push(fk);
            } else if self.at_word("generated") {
                let identity = self.toks[self.i..].iter().any(|t| t.is_word("identity"));
                return Err(unsupported(
                    at,
                    if identity {
                        "identity column"
                    } else {
                        "generated column"
                    },
                ));
            } else if self.eat_word("collate") {
                self.collation()?;
            } else if self.at_word("storage") || self.at_word("compression") {
                return Err(unsupported(at, "column storage options"));
            } else {
                return Err(self.syntax("a column constraint"));
            }
            self.deferral()?;
        }
        Ok((column, at))
    }

    /// A column of type `ty`, which takes a domain's `NOT NULL` and
    /// default.
    fn typed_column(ty: ColumnType, catalog: &Catalog, name: &str) -> Column {
        let (not_null, default) = match &ty.user {
            Some(user) => match catalog.types.get(user) {
                Some(UserType::Domain(d)) => (d.not_null, d.default.clone()),
                _ => (false, None),
            },
            None => (false, None),
        };
        Column {
            name: name.to_owned(),
            ty,
            not_null,
            default,
        }
    }

    /// The column a serial pseudo-type declares (Sec 8.1.4): an integer
    /// of `bits` bits, `NOT NULL`, whose default draws from the sequence
    /// `<table>_<column>_seq`.
    fn serial_column(
        &self,
        table: &str,
        name: &str,
        bits: u8,
        type_at: usize,
    ) -> Result<Column, SqlDdlError> {
        if self.at_punct("[") || self.at_word("array") {
            return Err(unsupported(type_at, "an array of a serial type"));
        }
        let sequence = format!("{table}_{name}_seq");
        if sequence.len() > MAX_IDENTIFIER_BYTES {
            return Err(unsupported(
                type_at,
                "a generated sequence name longer than 63 bytes",
            ));
        }
        Ok(Column {
            name: name.to_owned(),
            ty: ColumnType {
                scalar: Scalar::Int(bits),
                user: None,
                domain_checks: Vec::new(),
                dims: 0,
            },
            not_null: true,
            default: Some(format!(
                "nextval('{}'::regclass)",
                quote_identifier(&sequence).replace('\'', "''")
            )),
        })
    }

    /// The `COLLATE` name after the keyword: `"default"`, `"C"` and
    /// `"POSIX"` compare by bytes, as keys do; any other collation is
    /// refused.
    fn collation(&mut self) -> Result<(), SqlDdlError> {
        let at = self.offset();
        let (mut name, _) = self.identifier()?;
        if self.eat_punct(".") {
            if name != "pg_catalog" {
                return Err(unsupported(at, "a collation outside pg_catalog"));
            }
            name = self.identifier()?.0;
        }
        if matches!(name.as_str(), "default" | "C" | "POSIX") {
            Ok(())
        } else {
            Err(unsupported(at, format!("COLLATE {name}")))
        }
    }

    /// `[NOT] DEFERRABLE` and `INITIALLY {IMMEDIATE | DEFERRED}` after a
    /// constraint: only the defaults are read.
    fn deferral(&mut self) -> Result<(), SqlDdlError> {
        loop {
            let at = self.offset();
            if self.eat_words(&["not", "deferrable"]) || self.eat_words(&["initially", "immediate"])
            {
                continue;
            }
            if self.at_word("deferrable") || self.at_words(&["initially", "deferred"]) {
                return Err(unsupported(at, "a deferrable constraint"));
            }
            return Ok(());
        }
    }

    /// The options after `UNIQUE`.
    fn unique_options(&mut self) -> Result<(), SqlDdlError> {
        if self.at_words(&["nulls", "not", "distinct"]) {
            return Err(unsupported(self.offset(), "UNIQUE NULLS NOT DISTINCT"));
        }
        self.eat_words(&["nulls", "distinct"]);
        Ok(())
    }

    fn index_parameters(&self) -> Result<(), SqlDdlError> {
        if self.at_word("include") || self.at_word("with") || self.at_words(&["using", "index"]) {
            return Err(unsupported(self.offset(), "index parameters"));
        }
        Ok(())
    }

    /// A table constraint, into `pending`.
    fn table_constraint(&mut self, pending: &mut Pending) -> Result<(), SqlDdlError> {
        let name = if self.eat_word("constraint") {
            Some(self.identifier()?)
        } else {
            None
        };
        let at = self.offset();
        if self.eat_word("check") {
            let expr = self.parenthesized()?;
            if self.at_words(&["no", "inherit"]) {
                return Err(unsupported(self.offset(), "NO INHERIT"));
            }
            pending.checks.push((name, expr));
        } else if self.eat_word("unique") {
            self.unique_options()?;
            let columns = self.column_list()?;
            self.index_parameters()?;
            pending.keys.push(PendingKey {
                name,
                role: Role::Unique,
                columns,
                at,
            });
        } else if self.eat_words(&["primary", "key"]) {
            let columns = self.column_list()?;
            self.index_parameters()?;
            pending.keys.push(PendingKey {
                name,
                role: Role::Primary,
                columns,
                at,
            });
        } else if self.eat_words(&["foreign", "key"]) {
            let columns = self.column_list()?;
            self.expect_word("references", "REFERENCES")?;
            let fk = self.references(name, columns)?;
            pending.foreign_keys.push(fk);
        } else if self.at_word("exclude") {
            return Err(unsupported(at, "EXCLUDE"));
        } else {
            return Err(self.syntax("a table constraint"));
        }
        self.deferral()?;
        if self.at_word("not") && self.peek_at(1).is_some_and(|t| t.is_word("valid")) {
            return Err(unsupported(self.offset(), "NOT VALID"));
        }
        Ok(())
    }

    /// The rest of a `REFERENCES` clause.
    fn references(
        &mut self,
        name: Option<(String, usize)>,
        columns: Vec<(String, usize)>,
    ) -> Result<PendingForeignKey, SqlDdlError> {
        let target = self.object_name()?;
        let target_columns = if self.at_punct("(") {
            Some(self.column_list()?)
        } else {
            None
        };
        let mut match_full = false;
        let mut on_delete = ReferentialAction::NoAction;
        let mut on_update = ReferentialAction::NoAction;
        loop {
            let at = self.offset();
            if self.eat_words(&["match", "full"]) {
                match_full = true;
            } else if self.eat_words(&["match", "simple"]) {
                match_full = false;
            } else if self.at_words(&["match", "partial"]) {
                return Err(unsupported(at, "MATCH PARTIAL"));
            } else if self.eat_words(&["on", "delete"]) {
                on_delete = self.action()?;
            } else if self.eat_words(&["on", "update"]) {
                on_update = self.action()?;
            } else {
                break;
            }
        }
        Ok(PendingForeignKey {
            name,
            columns,
            target,
            target_columns,
            match_full,
            on_delete,
            on_update,
        })
    }

    fn action(&mut self) -> Result<ReferentialAction, SqlDdlError> {
        let action = if self.eat_words(&["no", "action"]) {
            ReferentialAction::NoAction
        } else if self.eat_word("restrict") {
            ReferentialAction::Restrict
        } else if self.eat_word("cascade") {
            ReferentialAction::Cascade
        } else if self.eat_words(&["set", "null"]) {
            ReferentialAction::SetNull
        } else if self.eat_words(&["set", "default"]) {
            ReferentialAction::SetDefault
        } else {
            return Err(self.syntax("a referential action"));
        };
        if self.at_punct("(") {
            return Err(unsupported(
                self.offset(),
                "a column list on SET NULL or SET DEFAULT",
            ));
        }
        Ok(action)
    }

    /// `CREATE UNIQUE INDEX [IF NOT EXISTS] [name] ON [ONLY] table [USING
    /// btree] (column, ...)`, read as a unique key.
    fn create_unique_index(&mut self, catalog: &mut Catalog) -> Result<(), SqlDdlError> {
        if self.at_word("concurrently") {
            self.i += 1;
        }
        let if_not_exists = self.eat_words(&["if", "not", "exists"]);
        let name = if self.at_word("on") {
            None
        } else {
            Some(self.identifier()?)
        };
        self.expect_word("on", "ON")?;
        self.eat_word("only");
        let (table, table_at) = self.object_name()?;
        let index = catalog
            .table_index(&table)
            .ok_or_else(|| SqlDdlError::UnknownTable {
                offset: table_at,
                name: table.clone(),
            })?;
        if self.eat_word("using") {
            let at = self.offset();
            let (method, _) = self.identifier()?;
            if method != "btree" {
                return Err(unsupported(at, format!("index method {method}")));
            }
        }
        self.expect_punct("(", "`(` opening the index columns")?;
        let mut columns = Vec::new();
        loop {
            let at = self.offset();
            let Some(t) = self.peek() else {
                return Err(self.syntax("an index column"));
            };
            if !matches!(t.kind, Kind::Word | Kind::Quoted) {
                return Err(unsupported(at, "an index on an expression"));
            }
            columns.push(self.identifier()?);
            if !(self.at_punct(",") || self.at_punct(")")) {
                return Err(unsupported(
                    self.offset(),
                    "an index column with an operator class, collation or ordering",
                ));
            }
            if self.eat_punct(")") {
                break;
            }
            self.expect_punct(",", "`,` or `)` in the index columns")?;
        }
        if let Some(t) = self.peek() {
            let feature = match t.text.as_str() {
                "where" => "a partial index",
                "include" => "INCLUDE",
                "nulls" => "UNIQUE NULLS NOT DISTINCT",
                _ => "index parameters",
            };
            return Err(unsupported(t.start, feature));
        }
        if let Some((name, at)) = &name
            && (catalog.relations.contains(name) || catalog.table_index(name).is_some())
        {
            if if_not_exists {
                return Ok(());
            }
            return Err(duplicate(*at, "relation", name));
        }
        let at = name.as_ref().map_or(table_at, |n| n.1);
        let generated = name.is_none();
        let pending = Pending {
            keys: vec![PendingKey {
                name,
                role: Role::Unique,
                columns,
                at,
            }],
            index_name: generated,
            ..Pending::default()
        };
        catalog.apply(index, pending)
    }

    /// `ALTER TABLE [IF EXISTS] [ONLY] name [*] action, ...`.
    fn alter_table(&mut self, catalog: &mut Catalog) -> Result<Option<&'static str>, SqlDdlError> {
        let if_exists = self.eat_words(&["if", "exists"]);
        self.eat_word("only");
        let (table, at) = self.object_name()?;
        self.eat_punct("*");
        if self.at_words(&["owner", "to"]) {
            self.i += 2;
            self.identifier()?;
            self.expect_end()?;
            return Ok(Some("ALTER TABLE OWNER TO"));
        }
        let Some(index) = catalog.table_index(&table) else {
            if if_exists {
                return Ok(None);
            }
            return Err(SqlDdlError::UnknownTable {
                offset: at,
                name: table,
            });
        };
        loop {
            let at = self.offset();
            if !self.eat_word("add") {
                let words: Vec<String> = self.toks[self.i..]
                    .iter()
                    .take(2)
                    .take_while(|t| t.kind == Kind::Word)
                    .map(|t| t.text.to_ascii_uppercase())
                    .collect();
                if words.is_empty() {
                    return Err(self.syntax("an ALTER TABLE action"));
                }
                return Err(unsupported(at, format!("ALTER TABLE {}", words.join(" "))));
            }
            let mut pending = Pending::default();
            let is_constraint = self.at_word("constraint")
                || self.at_word("check")
                || self.at_word("unique")
                || self.at_words(&["primary", "key"])
                || self.at_words(&["foreign", "key"])
                || self.at_word("exclude");
            if is_constraint {
                self.table_constraint(&mut pending)?;
                catalog.apply(index, pending)?;
            } else {
                self.eat_word("column");
                let if_not_exists = self.eat_words(&["if", "not", "exists"]);
                let table_name = catalog.tables[index].name.clone();
                let (column, col_at) =
                    self.column_definition(catalog, &table_name, &mut pending)?;
                if catalog.tables[index].column(&column.name).is_some() {
                    if !if_not_exists {
                        return Err(duplicate(col_at, "column", &column.name));
                    }
                } else {
                    catalog.tables[index].columns.push(column);
                    catalog.apply(index, pending)?;
                }
            }
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_end()?;
        Ok(None)
    }

    /// `DROP TABLE [IF EXISTS] name, ... [RESTRICT]`.
    fn drop_table(&mut self, catalog: &mut Catalog) -> Result<(), SqlDdlError> {
        let if_exists = self.eat_words(&["if", "exists"]);
        let mut names = Vec::new();
        loop {
            names.push(self.object_name()?);
            if !self.eat_punct(",") {
                break;
            }
        }
        if self.at_word("cascade") {
            return Err(unsupported(self.offset(), "DROP TABLE CASCADE"));
        }
        self.eat_word("restrict");
        self.expect_end()?;
        for (name, at) in names {
            let Some(index) = catalog.table_index(&name) else {
                if if_exists {
                    continue;
                }
                return Err(SqlDdlError::UnknownTable { offset: at, name });
            };
            let referenced = catalog
                .tables
                .iter()
                .any(|t| t.name != name && t.foreign_keys.iter().any(|fk| fk.target == name));
            if referenced {
                return Err(unsupported(
                    at,
                    "DROP TABLE of a table another table references",
                ));
            }
            let table = catalog.tables.remove(index);
            for key in &table.keys {
                catalog.relations.remove(&key.name);
            }
        }
        Ok(())
    }
}

/// A built-in type, or a serial pseudo-type.
enum Builtin {
    Plain(Scalar),
    Serial,
}

/// A key not yet named and checked.
#[derive(Debug)]
struct PendingKey {
    name: Option<(String, usize)>,
    role: Role,
    columns: Vec<(String, usize)>,
    at: usize,
}

/// A foreign key not yet resolved.
#[derive(Debug)]
struct PendingForeignKey {
    name: Option<(String, usize)>,
    columns: Vec<(String, usize)>,
    target: (String, usize),
    target_columns: Option<Vec<(String, usize)>>,
    match_full: bool,
    on_delete: ReferentialAction,
    on_update: ReferentialAction,
}

/// A `CHECK` not yet named: its name with the name's offset, and its
/// expression.
type PendingCheck = (Option<(String, usize)>, String);

/// The constraints one statement declares on one table, applied once the
/// statement's columns are known.
#[derive(Debug, Default)]
struct Pending {
    keys: Vec<PendingKey>,
    foreign_keys: Vec<PendingForeignKey>,
    checks: Vec<PendingCheck>,
    /// Whether an unnamed key is an index (`<table>_<columns>_idx`) rather
    /// than a constraint.
    index_name: bool,
}

fn duplicate(offset: usize, what: &'static str, name: &str) -> SqlDdlError {
    SqlDdlError::Duplicate {
        offset,
        what,
        name: name.to_owned(),
    }
}

/// A `CHECK` clause as the schema carries it.
fn check_clause(name: Option<&str>, expr: &str) -> String {
    match name {
        Some(name) => format!("CONSTRAINT {} CHECK ({expr})", quote_identifier(name)),
        None => format!("CHECK ({expr})"),
    }
}

/// The words `PostgreSQL` 17 reserves (Appendix C, reserved and reserved
/// that can be a function or type name), and the words that open a table
/// element, all of which a name must be quoted to be.
const RESERVED: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asymmetric",
    "authorization",
    "binary",
    "both",
    "case",
    "cast",
    "check",
    "collate",
    "collation",
    "column",
    "concurrently",
    "constraint",
    "create",
    "cross",
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "deferrable",
    "desc",
    "distinct",
    "do",
    "else",
    "end",
    "except",
    "exclude",
    "false",
    "fetch",
    "for",
    "foreign",
    "freeze",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "ilike",
    "in",
    "initially",
    "inner",
    "intersect",
    "into",
    "is",
    "isnull",
    "join",
    "lateral",
    "leading",
    "left",
    "like",
    "limit",
    "localtime",
    "localtimestamp",
    "natural",
    "not",
    "notnull",
    "null",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "outer",
    "overlaps",
    "placing",
    "primary",
    "references",
    "returning",
    "right",
    "select",
    "session_user",
    "similar",
    "some",
    "symmetric",
    "system_user",
    "table",
    "tablesample",
    "then",
    "to",
    "trailing",
    "true",
    "union",
    "unique",
    "user",
    "using",
    "variadic",
    "verbose",
    "when",
    "where",
    "window",
    "with",
];

/// `name` as an SQL identifier that reads back as `name`: bare when it is
/// a lowercase identifier and no reserved word, double-quoted otherwise.
pub(super) fn quote_identifier(name: &str) -> String {
    let bare = name
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'$')
        && !RESERVED.contains(&name);
    if bare {
        name.to_owned()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

impl Catalog {
    fn table_index(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.name == name)
    }

    fn declare_type(&mut self, name: String, at: usize, ty: UserType) -> Result<(), SqlDdlError> {
        if CATALOG_TYPE_NAMES.contains(&name.as_str())
            || UNSUPPORTED_TYPES.contains(&name.as_str())
            || matches!(
                name.as_str(),
                "integer"
                    | "int"
                    | "smallint"
                    | "bigint"
                    | "real"
                    | "float"
                    | "decimal"
                    | "boolean"
                    | "character"
                    | "varchar"
                    | "serial"
            )
        {
            return Err(unsupported(at, "a type named as a built-in type"));
        }
        if self.types.contains_key(&name) || self.table_index(&name).is_some() {
            return Err(duplicate(at, "type", &name));
        }
        self.types.insert(name, ty);
        Ok(())
    }

    /// Every constraint name in the schema, which a generated foreign key
    /// name avoids.
    fn constraint_names(&self) -> BTreeSet<String> {
        self.tables
            .iter()
            .flat_map(|t| {
                t.keys
                    .iter()
                    .map(|k| k.name.clone())
                    .chain(t.foreign_keys.iter().map(|f| f.name.clone()))
                    .chain(t.check_names.iter().cloned())
            })
            .collect()
    }

    /// Choose `<table>_<columns>_<label>`, or with a number after the
    /// label when that is taken, as `ChooseConstraintName` does.
    fn choose_name(
        base: &str,
        label: &str,
        taken: &BTreeSet<String>,
        at: usize,
    ) -> Result<String, SqlDdlError> {
        let mut pass = 0u32;
        loop {
            let name = if pass == 0 {
                format!("{base}_{label}")
            } else {
                format!("{base}_{label}{pass}")
            };
            if name.len() > MAX_IDENTIFIER_BYTES {
                return Err(unsupported(
                    at,
                    "a generated constraint name longer than 63 bytes",
                ));
            }
            if !taken.contains(&name) {
                return Ok(name);
            }
            pass += 1;
        }
    }

    /// Resolve the columns `columns` of table `index`.
    fn resolve_columns(
        &self,
        index: usize,
        columns: &[(String, usize)],
    ) -> Result<Vec<String>, SqlDdlError> {
        let table = &self.tables[index];
        let mut out: Vec<String> = Vec::new();
        for (name, at) in columns {
            if table.column(name).is_none() {
                return Err(SqlDdlError::UnknownColumn {
                    offset: *at,
                    table: table.name.clone(),
                    column: name.clone(),
                });
            }
            if out.contains(name) {
                return Err(duplicate(*at, "column", name));
            }
            out.push(name.clone());
        }
        Ok(out)
    }

    /// Apply the keys, foreign keys and checks one statement declares on
    /// table `index`, in that order, so a foreign key may reference a key
    /// of its own table.
    fn apply(&mut self, index: usize, pending: Pending) -> Result<(), SqlDdlError> {
        let table_name = self.tables[index].name.clone();
        for key in pending.keys {
            let columns = self.resolve_columns(index, &key.columns)?;
            let mut taken: BTreeSet<String> = self.relations.clone();
            taken.extend(self.tables.iter().map(|t| t.name.clone()));
            let name = match key.name {
                Some((name, at)) => {
                    if taken.contains(&name) {
                        return Err(duplicate(at, "relation", &name));
                    }
                    name
                }
                None => match key.role {
                    Role::Primary => Self::choose_name(&table_name, "pkey", &taken, key.at)?,
                    Role::Unique => Self::choose_name(
                        &format!("{table_name}_{}", columns.join("_")),
                        if pending.index_name { "idx" } else { "key" },
                        &taken,
                        key.at,
                    )?,
                },
            };
            let table = &mut self.tables[index];
            if key.role == Role::Primary {
                if table.keys.iter().any(|k| k.role == Role::Primary) {
                    return Err(duplicate(key.at, "primary key", &table_name));
                }
                for column in &mut table.columns {
                    if columns.contains(&column.name) {
                        column.not_null = true;
                    }
                }
            }
            self.relations.insert(name.clone());
            table.keys.push(KeyDecl {
                name,
                role: key.role,
                columns,
            });
        }
        for fk in pending.foreign_keys {
            let resolved = self.resolve_foreign_key(index, fk)?;
            self.tables[index].foreign_keys.push(resolved);
        }
        for (name, expr) in pending.checks {
            let table = &mut self.tables[index];
            if let Some((name, at)) = &name
                && (!table.check_names.insert(name.clone())
                    || table.foreign_keys.iter().any(|f| &f.name == name))
            {
                return Err(duplicate(*at, "constraint", name));
            }
            table
                .checks
                .push(check_clause(name.as_ref().map(|n| n.0.as_str()), &expr));
        }
        Ok(())
    }

    fn resolve_foreign_key(
        &self,
        index: usize,
        fk: PendingForeignKey,
    ) -> Result<ForeignKey, SqlDdlError> {
        let table_name = &self.tables[index].name;
        let columns = self.resolve_columns(index, &fk.columns)?;
        let (target, target_at) = fk.target;
        let target_index = self
            .table_index(&target)
            .ok_or_else(|| SqlDdlError::UnknownTable {
                offset: target_at,
                name: target.clone(),
            })?;
        let target_table = &self.tables[target_index];
        let no_key = |columns: Vec<String>| SqlDdlError::NoMatchingKey {
            offset: target_at,
            table: target.clone(),
            columns,
        };
        let (key, target_columns) = match fk.target_columns {
            None => {
                let key = target_table
                    .keys
                    .iter()
                    .find(|k| k.role == Role::Primary)
                    .ok_or_else(|| no_key(Vec::new()))?;
                (key, key.columns.clone())
            }
            Some(written) => {
                let written = self.resolve_columns(target_index, &written)?;
                let set: BTreeSet<&String> = written.iter().collect();
                let key = target_table
                    .keys
                    .iter()
                    .filter(|k| k.columns.len() == written.len())
                    .filter(|k| k.columns.iter().collect::<BTreeSet<_>>() == set)
                    .min_by_key(|k| k.role != Role::Primary)
                    .ok_or_else(|| no_key(written.clone()))?;
                (key, written)
            }
        };
        if target_columns.len() != columns.len() {
            return Err(no_key(target_columns));
        }
        // Pair the columns as written, then order the pairs by the key.
        let pairs: Vec<(&String, &String)> = columns.iter().zip(&target_columns).collect();
        let ordered: Vec<String> = key
            .columns
            .iter()
            .filter_map(|k| {
                pairs
                    .iter()
                    .find(|(_, t)| *t == k)
                    .map(|(c, _)| (*c).clone())
            })
            .collect();
        let name = match fk.name {
            Some((name, at)) => {
                let table = &self.tables[index];
                if table.foreign_keys.iter().any(|f| f.name == name)
                    || table.check_names.contains(&name)
                    || table.keys.iter().any(|k| k.name == name)
                {
                    return Err(duplicate(at, "constraint", &name));
                }
                name
            }
            None => Self::choose_name(
                &format!("{table_name}_{}", columns.join("_")),
                "fkey",
                &self.constraint_names(),
                fk.columns.first().map_or(target_at, |c| c.1),
            )?,
        };
        Ok(ForeignKey {
            name,
            columns: ordered,
            target,
            target_key: key.name.clone(),
            match_full: fk.match_full,
            on_delete: fk.on_delete,
            on_update: fk.on_update,
        })
    }
}
