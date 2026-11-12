//! The catalog a script declares, statement by statement.
//!
//! The catalog is what the script declares once every statement has taken
//! effect in order: its tables with their columns, keys, foreign keys and
//! checks, and its enum and domain types. Each statement resolves the
//! names it uses against the catalog as it stands before the statement,
//! as `PostgreSQL` does, and the first statement that cannot be read
//! refuses the script.

use std::collections::{BTreeMap, BTreeSet};

use super::error::SqlDdlError;
use super::quote::quote_identifier;
use super::statements::{
    AlterAction, BaseType, ColumnConstraintKind, ColumnDef, DomainConstraints,
    MAX_IDENTIFIER_BYTES, Name, OtherStatements, References, ReferentialAction, Scalar, Statement,
    StatementNotRead, TableConstraint, TableConstraintKind, TableElement, TypeSpec,
};

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
    /// The `CHECK` clauses, as the schema carries them.
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

/// The type names a script may not declare: the names of the built-in
/// types, which a column type resolves to before any declared type.
const BUILTIN_TYPE_NAMES: &[&str] = &[
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
    "integer",
    "int",
    "smallint",
    "bigint",
    "real",
    "float",
    "decimal",
    "boolean",
    "character",
    "serial",
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

/// Read a script's statements, in script order, into its catalog.
///
/// Each item is a statement or the refusal of the statement at that place;
/// the first refusal, from the statements or from resolving them, ends the
/// reading.
pub(super) fn read<I>(statements: I, others: OtherStatements) -> Result<Catalog, SqlDdlError>
where
    I: IntoIterator<Item = Result<Statement, SqlDdlError>>,
{
    let mut catalog = Catalog::default();
    for statement in statements {
        match statement? {
            Statement::CreateTable {
                if_not_exists,
                name,
                elements,
            } => catalog.create_table(if_not_exists, name, elements)?,
            Statement::CreateUniqueIndex {
                if_not_exists,
                name,
                table,
                columns,
            } => catalog.create_unique_index(if_not_exists, name, &table, columns)?,
            Statement::CreateDomain {
                name,
                ty,
                constraints,
            } => catalog.create_domain(name, &ty, constraints)?,
            Statement::CreateEnum { name, labels } => {
                catalog.declare_type(name, UserType::Enum(labels))?;
            }
            Statement::AlterTable {
                if_exists,
                table,
                actions,
            } => catalog.alter_table(if_exists, table, actions)?,
            Statement::DropTable { if_exists, names } => catalog.drop_table(if_exists, names)?,
            Statement::NotRead(not_read) => match others {
                OtherStatements::Refuse => {
                    return Err(unsupported(not_read.offset, not_read.statement));
                }
                OtherStatements::List => catalog.not_read.push(not_read),
            },
        }
    }
    Ok(catalog)
}

fn unsupported(offset: usize, feature: impl Into<String>) -> SqlDdlError {
    SqlDdlError::Unsupported {
        offset,
        feature: feature.into(),
    }
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

/// A key not yet named and checked.
#[derive(Debug)]
struct PendingKey {
    name: Option<Name>,
    role: Role,
    columns: Vec<Name>,
    at: usize,
}

/// A foreign key not yet resolved.
#[derive(Debug)]
struct PendingForeignKey {
    name: Option<Name>,
    columns: Vec<Name>,
    references: References,
}

/// The constraints one statement declares on one table, applied once the
/// statement's columns are known.
#[derive(Debug, Default)]
struct Pending {
    keys: Vec<PendingKey>,
    foreign_keys: Vec<PendingForeignKey>,
    checks: Vec<(Option<Name>, String)>,
    /// Whether an unnamed key is an index (`<table>_<columns>_idx`) rather
    /// than a constraint.
    index_name: bool,
}

impl Pending {
    fn table_constraint(&mut self, constraint: TableConstraint) {
        let TableConstraint { name, kind, offset } = constraint;
        match kind {
            TableConstraintKind::Check(expr) => self.checks.push((name, expr)),
            TableConstraintKind::Unique(columns) => self.keys.push(PendingKey {
                name,
                role: Role::Unique,
                columns,
                at: offset,
            }),
            TableConstraintKind::PrimaryKey(columns) => self.keys.push(PendingKey {
                name,
                role: Role::Primary,
                columns,
                at: offset,
            }),
            TableConstraintKind::ForeignKey(columns, references) => {
                self.foreign_keys.push(PendingForeignKey {
                    name,
                    columns,
                    references,
                });
            }
        }
    }
}

impl Catalog {
    fn table_index(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.name == name)
    }

    fn declare_type(&mut self, name: Name, ty: UserType) -> Result<(), SqlDdlError> {
        if BUILTIN_TYPE_NAMES.contains(&name.text.as_str()) {
            return Err(unsupported(name.offset, "a type named as a built-in type"));
        }
        if self.types.contains_key(&name.text) || self.table_index(&name.text).is_some() {
            return Err(duplicate(name.offset, "type", &name.text));
        }
        self.types.insert(name.text, ty);
        Ok(())
    }

    fn create_domain(
        &mut self,
        name: Name,
        ty: &TypeSpec,
        constraints: DomainConstraints,
    ) -> Result<(), SqlDdlError> {
        let scalar = match &ty.base {
            BaseType::Builtin(scalar) if ty.dims == 0 => scalar.clone(),
            BaseType::Serial(_) => {
                return Err(unsupported(
                    ty.offset,
                    "a serial type outside a column definition",
                ));
            }
            base => {
                // A named type must exist for the refusal to be about what
                // it is.
                if let BaseType::Named(type_name) = base {
                    self.resolve_type(type_name)?;
                }
                return Err(unsupported(
                    ty.offset,
                    "a domain over an array, enum or domain type",
                ));
            }
        };
        self.declare_type(
            name,
            UserType::Domain(Domain {
                scalar,
                not_null: constraints.not_null,
                default: constraints.default,
                checks: constraints
                    .checks
                    .iter()
                    .map(|(name, expr)| check_clause(name.as_ref().map(|n| n.text.as_str()), expr))
                    .collect(),
            }),
        )
    }

    /// The column type a declared type name gives.
    fn resolve_type(&self, name: &Name) -> Result<ColumnType, SqlDdlError> {
        match self.types.get(&name.text) {
            Some(UserType::Enum(labels)) => Ok(ColumnType {
                scalar: Scalar::Enum(labels.clone()),
                user: Some(name.text.clone()),
                domain_checks: Vec::new(),
                dims: 0,
            }),
            Some(UserType::Domain(domain)) => Ok(ColumnType {
                scalar: domain.scalar.clone(),
                user: Some(name.text.clone()),
                domain_checks: domain.checks.clone(),
                dims: 0,
            }),
            None => Err(SqlDdlError::UnknownType {
                offset: name.offset,
                name: name.text.clone(),
            }),
        }
    }

    /// A column of `table`, its keys, checks and foreign keys put in
    /// `pending`.
    fn column(
        &self,
        table: &str,
        def: ColumnDef,
        pending: &mut Pending,
    ) -> Result<Column, SqlDdlError> {
        let ColumnDef {
            name,
            ty,
            constraints,
        } = def;
        let mut column = match &ty.base {
            BaseType::Builtin(scalar) => Column {
                name: name.text.clone(),
                ty: ColumnType {
                    scalar: scalar.clone(),
                    user: None,
                    domain_checks: Vec::new(),
                    dims: ty.dims,
                },
                not_null: false,
                default: None,
            },
            BaseType::Serial(bits) => serial_column(table, &name.text, *bits, ty.offset)?,
            BaseType::Named(type_name) => {
                let mut resolved = self.resolve_type(type_name)?;
                resolved.dims = ty.dims;
                // A column of a domain takes the domain's `NOT NULL` and
                // default.
                let (not_null, default) = match self.types.get(&type_name.text) {
                    Some(UserType::Domain(d)) => (d.not_null, d.default.clone()),
                    _ => (false, None),
                };
                Column {
                    name: name.text.clone(),
                    ty: resolved,
                    not_null,
                    default,
                }
            }
        };
        for constraint in constraints {
            let at = constraint.offset;
            match constraint.kind {
                // `NULL` declares nothing: it neither lifts a domain's
                // `NOT NULL` nor a serial column's.
                ColumnConstraintKind::NotNull => column.not_null = true,
                ColumnConstraintKind::Null => {}
                ColumnConstraintKind::Default(expr) => column.default = Some(expr),
                ColumnConstraintKind::Check(expr) => pending.checks.push((constraint.name, expr)),
                ColumnConstraintKind::Unique => pending.keys.push(PendingKey {
                    name: constraint.name,
                    role: Role::Unique,
                    columns: vec![Name {
                        text: name.text.clone(),
                        offset: at,
                    }],
                    at,
                }),
                ColumnConstraintKind::PrimaryKey => pending.keys.push(PendingKey {
                    name: constraint.name,
                    role: Role::Primary,
                    columns: vec![Name {
                        text: name.text.clone(),
                        offset: at,
                    }],
                    at,
                }),
                ColumnConstraintKind::References(references) => {
                    pending.foreign_keys.push(PendingForeignKey {
                        name: constraint.name,
                        columns: vec![Name {
                            text: name.text.clone(),
                            offset: at,
                        }],
                        references,
                    });
                }
            }
        }
        Ok(column)
    }

    fn create_table(
        &mut self,
        if_not_exists: bool,
        name: Name,
        elements: Vec<TableElement>,
    ) -> Result<(), SqlDdlError> {
        if self.table_index(&name.text).is_some() || self.relations.contains(&name.text) {
            if if_not_exists {
                return Ok(());
            }
            return Err(duplicate(name.offset, "table", &name.text));
        }
        if self.types.contains_key(&name.text) {
            return Err(duplicate(name.offset, "type", &name.text));
        }
        let mut table = Table {
            name: name.text,
            ..Table::default()
        };
        let mut pending = Pending::default();
        for element in elements {
            match element {
                TableElement::Constraint(constraint) => pending.table_constraint(constraint),
                TableElement::Column(def) => {
                    let at = def.name.offset;
                    let column = self.column(&table.name, def, &mut pending)?;
                    if table.column(&column.name).is_some() {
                        return Err(duplicate(at, "column", &column.name));
                    }
                    table.columns.push(column);
                }
            }
        }
        self.tables.push(table);
        let index = self.tables.len() - 1;
        self.apply(index, pending)
    }

    fn create_unique_index(
        &mut self,
        if_not_exists: bool,
        name: Option<Name>,
        table: &Name,
        columns: Vec<Name>,
    ) -> Result<(), SqlDdlError> {
        let index = self
            .table_index(&table.text)
            .ok_or_else(|| SqlDdlError::UnknownTable {
                offset: table.offset,
                name: table.text.clone(),
            })?;
        if let Some(name) = &name
            && (self.relations.contains(&name.text) || self.table_index(&name.text).is_some())
        {
            if if_not_exists {
                return Ok(());
            }
            return Err(duplicate(name.offset, "relation", &name.text));
        }
        let at = name.as_ref().map_or(table.offset, |n| n.offset);
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
        self.apply(index, pending)
    }

    fn alter_table(
        &mut self,
        if_exists: bool,
        table: Name,
        actions: Vec<AlterAction>,
    ) -> Result<(), SqlDdlError> {
        let Some(index) = self.table_index(&table.text) else {
            if if_exists {
                return Ok(());
            }
            return Err(SqlDdlError::UnknownTable {
                offset: table.offset,
                name: table.text,
            });
        };
        for action in actions {
            let mut pending = Pending::default();
            match action {
                AlterAction::AddConstraint(constraint) => {
                    pending.table_constraint(constraint);
                    self.apply(index, pending)?;
                }
                AlterAction::AddColumn {
                    if_not_exists,
                    column,
                } => {
                    let at = column.name.offset;
                    let table_name = self.tables[index].name.clone();
                    let column = self.column(&table_name, column, &mut pending)?;
                    if self.tables[index].column(&column.name).is_some() {
                        if !if_not_exists {
                            return Err(duplicate(at, "column", &column.name));
                        }
                    } else {
                        self.tables[index].columns.push(column);
                        self.apply(index, pending)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn drop_table(&mut self, if_exists: bool, names: Vec<Name>) -> Result<(), SqlDdlError> {
        for name in names {
            let Some(index) = self.table_index(&name.text) else {
                if if_exists {
                    continue;
                }
                return Err(SqlDdlError::UnknownTable {
                    offset: name.offset,
                    name: name.text,
                });
            };
            let referenced = self.tables.iter().any(|t| {
                t.name != name.text && t.foreign_keys.iter().any(|fk| fk.target == name.text)
            });
            if referenced {
                return Err(unsupported(
                    name.offset,
                    "DROP TABLE of a table another table references",
                ));
            }
            let table = self.tables.remove(index);
            for key in &table.keys {
                self.relations.remove(&key.name);
            }
        }
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
    fn resolve_columns(&self, index: usize, columns: &[Name]) -> Result<Vec<String>, SqlDdlError> {
        let table = &self.tables[index];
        let mut out: Vec<String> = Vec::new();
        for column in columns {
            if table.column(&column.text).is_none() {
                return Err(SqlDdlError::UnknownColumn {
                    offset: column.offset,
                    table: table.name.clone(),
                    column: column.text.clone(),
                });
            }
            if out.contains(&column.text) {
                return Err(duplicate(column.offset, "column", &column.text));
            }
            out.push(column.text.clone());
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
                Some(name) => {
                    if taken.contains(&name.text) {
                        return Err(duplicate(name.offset, "relation", &name.text));
                    }
                    name.text
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
            if let Some(name) = &name
                && (!table.check_names.insert(name.text.clone())
                    || table.foreign_keys.iter().any(|f| f.name == name.text))
            {
                return Err(duplicate(name.offset, "constraint", &name.text));
            }
            table
                .checks
                .push(check_clause(name.as_ref().map(|n| n.text.as_str()), &expr));
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
        let References {
            target,
            target_columns,
            match_full,
            on_delete,
            on_update,
        } = fk.references;
        let target_index =
            self.table_index(&target.text)
                .ok_or_else(|| SqlDdlError::UnknownTable {
                    offset: target.offset,
                    name: target.text.clone(),
                })?;
        let target_table = &self.tables[target_index];
        let no_key = |columns: Vec<String>| SqlDdlError::NoMatchingKey {
            offset: target.offset,
            table: target.text.clone(),
            columns,
        };
        let (key, target_columns) = match target_columns {
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
            Some(name) => {
                let table = &self.tables[index];
                if table.foreign_keys.iter().any(|f| f.name == name.text)
                    || table.check_names.contains(&name.text)
                    || table.keys.iter().any(|k| k.name == name.text)
                {
                    return Err(duplicate(name.offset, "constraint", &name.text));
                }
                name.text
            }
            None => Self::choose_name(
                &format!("{table_name}_{}", columns.join("_")),
                "fkey",
                &self.constraint_names(),
                fk.columns.first().map_or(target.offset, |c| c.offset),
            )?,
        };
        Ok(ForeignKey {
            name,
            columns: ordered,
            target: target.text,
            target_key: key.name.clone(),
            match_full,
            on_delete,
            on_update,
        })
    }
}

/// The column a serial pseudo-type declares (Sec 8.1.4): an integer of
/// `bits` bits, `NOT NULL`, whose default draws from the sequence
/// `<table>_<column>_seq`.
fn serial_column(table: &str, name: &str, bits: u8, type_at: usize) -> Result<Column, SqlDdlError> {
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
