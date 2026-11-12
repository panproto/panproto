//! The `postgresql` protocol: `PostgreSQL` 17 DDL read into table schemas.
//!
//! The protocol uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`): its syntax and its data are both read as
//! tables.
//!
//! A script of `PostgreSQL` 17 DDL is read in two steps. A reader of the
//! source text (`panproto_io::postgresql::read_ddl`, which parses through the
//! tree-sitter grammar generated from `PostgreSQL`'s own `gram.y`) turns each
//! statement into a [`Statement`](crate::database::postgresql::Statement); [`read_statements`](crate::database::postgresql::read_statements) then resolves the
//! statements in script order into a [`Schema`](panproto_schema::Schema).
//! Each table is a `table` vertex and a hyper-edge over its
//! columns; each column is a vertex `<table>.<column>`, joined to its table
//! by a `prop` edge whose position is the column's, and an array column is
//! an `array` vertex with an `items` edge per dimension to its element.
//! Primary keys and unique constraints are schema [`Key`](panproto_schema::Key)s and foreign keys
//! are schema [`Reference`](panproto_schema::Reference)s, which reference resolution reads. The
//! dialect, the statements read and every feature refused are stated in
//! the book's chapter on SQL DDL.

mod catalog;
mod error;
mod quote;
mod statements;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{
    Cardinality, Edge, EdgeRule, Key, KeyEquality, KeyPart, KeyRole, KeyScope, NullMatch,
    OnUnresolved, Protocol, RefSource, Reference, ResolveMode, Schema, SchemaBuilder, SqlCollation,
    SqlPad,
};

pub use error::SqlDdlError;
pub use quote::quote_identifier;
pub use statements::{
    AlterAction, BaseType, ColumnConstraint, ColumnConstraintKind, ColumnDef, DomainConstraints,
    MAX_IDENTIFIER_BYTES, Name, OtherStatements, References, ReferentialAction, Scalar, Statement,
    StatementNotRead, TableConstraint, TableConstraintKind, TableElement, TypeSpec,
};

use crate::error::ProtocolError;
use crate::theories;
use catalog::{Catalog, Role};

/// The vertex kinds of the protocol.
const OBJ_KINDS: &[&str] = &[
    "table",
    "integer",
    "number",
    "real",
    "double",
    "string",
    "character",
    "boolean",
    "bytes",
    "date",
    "time",
    "timestamp",
    "uuid",
    "json",
    "jsonb",
    "array",
];

/// The constraint sorts of the protocol.
const CONSTRAINT_SORTS: &[&str] = &[
    "NOT NULL",
    "DEFAULT",
    "CHECK",
    "ENUM",
    "TYPE",
    "DOMAIN CHECK",
    "FOREIGN KEY ACTION",
    "int_bits",
    "precision",
    "scale",
    "max_length",
    "with_time_zone",
];

/// Returns the `postgresql` protocol definition.
///
/// Schema theory: `colimit(ThHypergraph, ThConstraint)`.
/// Instance theory: `ThFunctor`.
#[must_use]
pub fn protocol() -> Protocol {
    let mut protocol = Protocol {
        name: "postgresql".into(),
        schema_theory: "ThPostgreSQLSchema".into(),
        instance_theory: "ThPostgreSQLInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: OBJ_KINDS.iter().map(|k| (*k).into()).collect(),
        constraint_sorts: CONSTRAINT_SORTS.iter().map(|k| (*k).into()).collect(),
        has_order: true,
        nominal_identity: true,
        ..Default::default()
    };
    crate::semantics::attach(&mut protocol);
    protocol
}

/// Register the component GATs for `postgresql` with a theory registry.
///
/// Registers `ThHypergraph`, `ThConstraint`, `ThFunctor`, and the
/// composed schema/instance theories.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThPostgreSQLSchema", "ThPostgreSQLInstance");
}

/// What [`read_statements`] read from a script.
#[derive(Debug)]
pub struct DdlReading {
    /// The tables the script declares.
    pub schema: Schema,
    /// The statements that declare nothing a table schema holds, in
    /// script order, when they are listed rather than refused.
    pub not_read: Vec<StatementNotRead>,
}

impl From<SqlDdlError> for ProtocolError {
    fn from(error: SqlDdlError) -> Self {
        Self::SqlDdl(Box::new(error))
    }
}

/// Resolve a script's statements, in script order, into the tables they
/// declare.
///
/// The statements read are `CREATE TABLE`, `CREATE UNIQUE INDEX`, `CREATE
/// TYPE ... AS ENUM`, `CREATE DOMAIN`, `ALTER TABLE ... ADD` and `DROP
/// TABLE`, each taking effect in script order against the catalog the
/// statements before it declared. A [`Statement::NotRead`] (a function,
/// view, trigger, sequence, non-unique index, grant, ownership change,
/// comment or session setting, or `COPY` and `INSERT`) is refused or listed
/// as `others` says. Each item is a statement or the refusal of the
/// statement at that place, so a reader may hand over its statements as it
/// reads them.
///
/// # Errors
///
/// Returns the first [`SqlDdlError`] in script order: a refusal the reader
/// handed over, a statement `others` refuses, a name that does not resolve
/// or is declared twice, or a schema panproto cannot build.
pub fn read_statements<I>(statements: I, others: OtherStatements) -> Result<DdlReading, SqlDdlError>
where
    I: IntoIterator<Item = Result<Statement, SqlDdlError>>,
{
    let catalog = catalog::read(statements, others)?;
    let schema = build(&catalog).map_err(|e| SqlDdlError::Unsupported {
        offset: 0,
        feature: format!("a schema panproto cannot build: {e}"),
    })?;
    Ok(DdlReading {
        schema,
        not_read: catalog.not_read,
    })
}

/// The vertex id of column `column` of `table`.
fn column_id(table: &str, column: &str) -> String {
    format!("{table}.{column}")
}

/// The `prop` edge from `table` to its column `column`.
fn prop_edge(table: &str, column: &str) -> Edge {
    Edge {
        src: table.into(),
        tgt: column_id(table, column).into(),
        kind: "prop".into(),
        name: Some(column.into()),
    }
}

/// The vertex kind and value parameters of an element type.
fn element_kind(scalar: &Scalar) -> (&'static str, Vec<(&'static str, String)>) {
    match scalar {
        Scalar::Int(bits) => ("integer", vec![("int_bits", bits.to_string())]),
        Scalar::Numeric(None) => ("number", vec![]),
        Scalar::Numeric(Some((p, s))) => (
            "number",
            vec![("precision", p.to_string()), ("scale", s.to_string())],
        ),
        Scalar::Real => ("real", vec![]),
        Scalar::Double => ("double", vec![]),
        Scalar::Text => ("string", vec![]),
        Scalar::Varchar(None) => ("string", vec![]),
        Scalar::Varchar(Some(n)) => ("string", vec![("max_length", n.to_string())]),
        Scalar::Character(n) => ("character", vec![("max_length", n.to_string())]),
        Scalar::Boolean => ("boolean", vec![]),
        Scalar::Bytea => ("bytes", vec![]),
        Scalar::Date => ("date", vec![]),
        Scalar::Time => ("time", vec![]),
        Scalar::Timestamp(tz) => ("timestamp", vec![("with_time_zone", tz.to_string())]),
        Scalar::Uuid => ("uuid", vec![]),
        Scalar::Json => ("json", vec![]),
        Scalar::Jsonb => ("jsonb", vec![]),
        Scalar::Enum(_) => ("string", vec![]),
    }
}

/// The schema of a catalog.
fn build(catalog: &Catalog) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut orderings: Vec<(Edge, u32)> = Vec::new();
    // Tables and foreign keys are added in name order, so the schema's
    // adjacency lists, and with them its canonical bytes, do not depend on
    // the order the script declared them in.
    let mut tables: Vec<&catalog::Table> = catalog.tables.iter().collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    for table in &tables {
        let t = table.name.as_str();
        builder = builder.vertex(t, "table", None)?;
        let mut signature = HashMap::new();
        for (position, column) in table.columns.iter().enumerate() {
            builder = build_column(builder, t, column)?;
            orderings.push((
                prop_edge(t, &column.name),
                u32::try_from(position).unwrap_or(u32::MAX),
            ));
            signature.insert(column.name.clone(), column_id(t, &column.name));
        }
        builder = builder.hyper_edge(t, "table", signature, t)?;
        for check in &table.checks {
            builder = builder.constraint(t, "CHECK", check);
        }
        for key in &table.keys {
            builder = builder.key(t, build_key(table, key));
        }
    }
    // A foreign key's edge joins two table vertices, so every table is
    // declared first.
    for table in &tables {
        let mut foreign_keys: Vec<&catalog::ForeignKey> = table.foreign_keys.iter().collect();
        foreign_keys.sort_by(|a, b| a.name.cmp(&b.name));
        for fk in foreign_keys {
            builder = build_foreign_key(builder, table, fk)?;
        }
    }
    let mut schema = builder.build()?;
    schema.orderings.extend(orderings);
    Ok(schema)
}

/// Add column `column` of table `t`: its vertices, its constraints and
/// its `prop` edge.
fn build_column(
    mut builder: SchemaBuilder,
    t: &str,
    column: &catalog::Column,
) -> Result<SchemaBuilder, ProtocolError> {
    let id = column_id(t, &column.name);
    let mut levels = vec![id.clone()];
    for d in 0..column.ty.dims {
        levels.push(format!("{}:items", levels[d]));
    }
    let (kind, parameters) = element_kind(&column.ty.scalar);
    for (d, vertex) in levels.iter().enumerate() {
        let vertex_kind = if d < column.ty.dims { "array" } else { kind };
        builder = builder.vertex(vertex, vertex_kind, None)?;
    }
    for pair in levels.windows(2) {
        builder = builder.edge(&pair[0], &pair[1], "items", None)?;
    }
    let element = levels.last().map_or(id.as_str(), String::as_str);
    for (sort, value) in parameters {
        builder = builder.constraint(element, sort, &value);
    }
    if let Some(user) = &column.ty.user {
        builder = builder.constraint(element, "TYPE", user);
    }
    if let Scalar::Enum(labels) = &column.ty.scalar {
        let json = serde_json::to_string(labels)?;
        builder = builder.constraint(element, "ENUM", &json);
    }
    for check in &column.ty.domain_checks {
        builder = builder.constraint(element, "DOMAIN CHECK", check);
    }
    if column.not_null {
        builder = builder.constraint(&id, "NOT NULL", "true");
    }
    if let Some(default) = &column.default {
        builder = builder.constraint(&id, "DEFAULT", default);
    }
    Ok(builder.edge(t, &id, "prop", Some(&column.name))?)
}

/// The schema key of a primary or unique key: each part compares as SQL
/// compares, a `character(n)` part ignoring trailing spaces.
fn build_key(table: &catalog::Table, key: &catalog::KeyDecl) -> Key {
    let t = table.name.as_str();
    let equality = key
        .columns
        .iter()
        .map(|c| {
            let padded = table.column(c).is_some_and(|col| {
                col.ty.dims == 0 && matches!(col.ty.scalar, Scalar::Character(_))
            });
            KeyEquality::Sql {
                collation: SqlCollation::Binary,
                pad: if padded {
                    SqlPad::PadSpace
                } else {
                    SqlPad::NoPad
                },
            }
        })
        .collect();
    Key {
        id: key.name.as_str().into(),
        parts: key
            .columns
            .iter()
            .map(|c| KeyPart::Path(vec![prop_edge(t, c)]))
            .collect(),
        role: match key.role {
            Role::Primary => KeyRole::Primary,
            Role::Unique => KeyRole::Unique,
        },
        scope: KeyScope::Global,
        equality,
    }
}

/// Add a foreign key: its `foreign-key` edge, its reference and, when an
/// action is not `NO ACTION`, its `FOREIGN KEY ACTION` constraint.
fn build_foreign_key(
    builder: SchemaBuilder,
    table: &catalog::Table,
    fk: &catalog::ForeignKey,
) -> Result<SchemaBuilder, ProtocolError> {
    let t = table.name.as_str();
    let edge = Edge {
        src: t.into(),
        tgt: fk.target.as_str().into(),
        kind: "foreign-key".into(),
        name: Some(fk.name.as_str().into()),
    };
    let mut builder = builder.edge(t, &fk.target, "foreign-key", Some(&fk.name))?;
    let nullable = fk
        .columns
        .iter()
        .any(|c| table.column(c).is_some_and(|col| !col.not_null));
    builder = builder.reference(Reference {
        id: format!("{t}.{}", fk.name).into(),
        holder: t.into(),
        source: RefSource::KeyValue {
            parts: fk.columns.iter().map(|c| vec![prop_edge(t, c)]).collect(),
        },
        target_vertex: fk.target.as_str().into(),
        target_key: Some(fk.target_key.as_str().into()),
        edge: Some(edge),
        cardinality: Cardinality::One,
        nullable,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
        guard: None,
        null_match: if fk.match_full {
            NullMatch::Full
        } else {
            NullMatch::Simple
        },
        null_vertex: None,
        scope_via: None,
    });
    let mut actions = String::new();
    for (verb, action) in [("DELETE", fk.on_delete), ("UPDATE", fk.on_update)] {
        if action != ReferentialAction::NoAction {
            let _ = write!(actions, " ON {verb} {}", action.sql());
        }
    }
    if !actions.is_empty() {
        let value = format!("{}{actions}", quote_identifier(&fk.name));
        builder = builder.constraint(t, "FOREIGN KEY ACTION", &value);
    }
    Ok(builder)
}

fn emit_error(message: impl Into<String>) -> ProtocolError {
    ProtocolError::Emit(message.into())
}

/// The constraint values of sort `sort` on `vertex`, in schema order.
fn values<'a>(schema: &'a Schema, vertex: &str, sort: &str) -> Vec<&'a str> {
    schema
        .constraints
        .get(vertex)
        .map(|cs| {
            cs.iter()
                .filter(|c| c.sort == sort)
                .map(|c| c.value.as_str())
                .collect()
        })
        .unwrap_or_default()
}

fn value<'a>(schema: &'a Schema, vertex: &str, sort: &str) -> Option<&'a str> {
    values(schema, vertex, sort).into_iter().next()
}

/// The built-in type an element vertex holds, from its kind and value
/// parameters.
fn builtin_type(schema: &Schema, element: &str, kind: &str) -> Result<String, ProtocolError> {
    let parameter = |sort: &str| value(schema, element, sort);
    Ok(match kind {
        "integer" => match parameter("int_bits") {
            Some("16") => "smallint".to_owned(),
            Some("32") => "integer".to_owned(),
            Some("64") | None => "bigint".to_owned(),
            Some(other) => {
                return Err(emit_error(format!(
                    "{element}: no integer type of {other} bits"
                )));
            }
        },
        "number" => match (parameter("precision"), parameter("scale")) {
            (Some(p), Some(s)) => format!("numeric({p},{s})"),
            (Some(p), None) => format!("numeric({p})"),
            _ => "numeric".to_owned(),
        },
        "real" => "real".to_owned(),
        "double" => "double precision".to_owned(),
        "string" => match parameter("max_length") {
            Some(n) => format!("character varying({n})"),
            None => "text".to_owned(),
        },
        "character" => format!("character({})", parameter("max_length").unwrap_or("1")),
        "boolean" => "boolean".to_owned(),
        "bytes" => "bytea".to_owned(),
        "date" => "date".to_owned(),
        "time" => "time without time zone".to_owned(),
        "timestamp" => match parameter("with_time_zone") {
            Some("true") => "timestamp with time zone".to_owned(),
            _ => "timestamp without time zone".to_owned(),
        },
        "uuid" => "uuid".to_owned(),
        "json" => "json".to_owned(),
        "jsonb" => "jsonb".to_owned(),
        other => {
            return Err(emit_error(format!(
                "{element}: no SQL type for vertex kind {other:?}"
            )));
        }
    })
}

/// Write `labels` as `('a', 'b')`.
fn enum_labels(labels: &[String]) -> String {
    let quoted: Vec<String> = labels
        .iter()
        .map(|l| format!("'{}'", l.replace('\'', "''")))
        .collect();
    format!("({})", quoted.join(", "))
}

/// Emit a [`Schema`] as canonical `PostgreSQL` 17 DDL.
///
/// The canonical text declares, in this order: each enum and domain type
/// the columns name, by name; each table, by name, with its columns in
/// position order (`NOT NULL` and `DEFAULT` after the type), then its
/// primary key, its unique keys by name and its checks in schema order,
/// each key as a named constraint; and then each foreign key as `ALTER
/// TABLE ... ADD CONSTRAINT`, by table and name, so that tables may
/// reference each other in any order. Reading the text back gives the
/// schema again.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema holds a vertex kind,
/// value parameter or declaration with no SQL form, or two definitions of
/// one type name.
pub fn emit_ddl(schema: &Schema) -> Result<String, ProtocolError> {
    let (mut output, foreign_keys) = emit_ddl_parts(schema)?;
    output.push_str(&foreign_keys);
    Ok(output)
}

/// The canonical DDL of [`emit_ddl`] in its two parts.
///
/// The first part declares the types and tables, the second holds the
/// `ALTER TABLE ... ADD CONSTRAINT ... FOREIGN KEY` statements. A data
/// script places its rows between them, so the rows
/// load before any foreign key is checked.
///
/// # Errors
///
/// As [`emit_ddl`].
pub fn emit_ddl_parts(schema: &Schema) -> Result<(String, String), ProtocolError> {
    let q = quote_identifier;
    let mut tables: Vec<&str> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .map(|v| v.id.as_str())
        .collect();
    tables.sort_unstable();

    let mut types: BTreeMap<String, String> = BTreeMap::new();
    let mut bodies = String::new();
    let mut foreign_keys = String::new();
    for &t in &tables {
        let mut columns: Vec<(u32, &Edge)> = schema
            .outgoing_edges(t)
            .iter()
            .filter(|e| e.kind == "prop")
            .map(|e| (schema.orderings.get(e).copied().unwrap_or(u32::MAX), e))
            .collect();
        columns.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        let mut lines: Vec<String> = Vec::new();
        for (_, edge) in &columns {
            lines.push(column_line(schema, t, edge, &mut types)?);
        }
        let mut keys: Vec<&Key> = schema
            .declarations
            .keys
            .get(t)
            .map(|ks| ks.iter().collect())
            .unwrap_or_default();
        keys.sort_by_key(|k| (k.role != KeyRole::Primary, k.id.clone()));
        for key in keys {
            let role = match key.role {
                KeyRole::Primary => "PRIMARY KEY",
                KeyRole::Unique => "UNIQUE",
                _ => return Err(emit_error(format!("{t}: key {} has no SQL form", key.id))),
            };
            lines.push(format!(
                "    CONSTRAINT {} {role} ({})",
                q(&key.id),
                key_columns(key)?.join(", ")
            ));
        }
        for check in values(schema, t, "CHECK") {
            lines.push(format!("    {check}"));
        }
        let _ = write!(
            bodies,
            "CREATE TABLE {} (\n{}\n);\n\n",
            q(t),
            lines.join(",\n")
        );
        foreign_key_statements(schema, t, &mut foreign_keys)?;
    }
    let mut output: String = types.into_values().collect::<String>();
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(&bodies);
    Ok((output, foreign_keys))
}

/// The quoted column names of a key's parts.
fn key_columns(key: &Key) -> Result<Vec<String>, ProtocolError> {
    key.parts
        .iter()
        .map(|part| match part {
            KeyPart::Path(path) if path.len() == 1 => path[0]
                .name
                .as_deref()
                .map(quote_identifier)
                .ok_or_else(|| emit_error(format!("key {} has an unnamed part", key.id))),
            _ => Err(emit_error(format!("key {} has no SQL form", key.id))),
        })
        .collect()
}

/// The definition line of the column `edge` of table `t`, recording the
/// enum or domain type it names in `types`.
fn column_line(
    schema: &Schema,
    t: &str,
    edge: &Edge,
    types: &mut BTreeMap<String, String>,
) -> Result<String, ProtocolError> {
    let q = quote_identifier;
    let name = edge
        .name
        .as_deref()
        .ok_or_else(|| emit_error(format!("{t}: a column edge without a name")))?;
    let mut element = edge.tgt.as_str();
    let mut dims = String::new();
    while schema
        .vertices
        .get(element)
        .is_some_and(|v| v.kind == "array")
    {
        element = schema
            .outgoing_edges(element)
            .iter()
            .find(|e| e.kind == "items")
            .map(|e| e.tgt.as_str())
            .ok_or_else(|| emit_error(format!("{element}: an array without items")))?;
        dims.push_str("[]");
    }
    let kind = schema
        .vertices
        .get(element)
        .map(|v| v.kind.as_str())
        .ok_or_else(|| emit_error(format!("no vertex {element}")))?;
    let base = builtin_type(schema, element, kind)?;
    let type_name = if let Some(user) = value(schema, element, "TYPE") {
        let definition = if let Some(json) = value(schema, element, "ENUM") {
            let labels: Vec<String> = serde_json::from_str(json)?;
            format!(
                "CREATE TYPE {} AS ENUM {};\n",
                q(user),
                enum_labels(&labels)
            )
        } else {
            let mut text = format!("CREATE DOMAIN {} AS {base}", q(user));
            for check in values(schema, element, "DOMAIN CHECK") {
                let _ = write!(text, " {check}");
            }
            text.push_str(";\n");
            text
        };
        if types.get(user).is_some_and(|d| *d != definition) {
            return Err(emit_error(format!("two definitions of type {user:?}")));
        }
        types.insert(user.to_owned(), definition);
        q(user)
    } else {
        base
    };
    let mut line = format!("    {} {type_name}{dims}", q(name));
    if value(schema, &edge.tgt, "NOT NULL") == Some("true") {
        line.push_str(" NOT NULL");
    }
    if let Some(default) = value(schema, &edge.tgt, "DEFAULT") {
        let _ = write!(line, " DEFAULT {default}");
    }
    Ok(line)
}

/// Write an `ALTER TABLE ... ADD CONSTRAINT ... FOREIGN KEY` statement
/// for each reference table `t` holds, by reference id.
fn foreign_key_statements(schema: &Schema, t: &str, out: &mut String) -> Result<(), ProtocolError> {
    let q = quote_identifier;
    let mut references: Vec<&Reference> = schema
        .declarations
        .references
        .values()
        .filter(|r| r.holder.as_str() == t)
        .collect();
    references.sort_by(|a, b| a.id.cmp(&b.id));
    for r in references {
        let name = r
            .edge
            .as_ref()
            .and_then(|e| e.name.as_deref())
            .ok_or_else(|| emit_error(format!("reference {} has no edge name", r.id)))?;
        let RefSource::KeyValue { parts } = &r.source else {
            return Err(emit_error(format!("reference {} has no SQL form", r.id)));
        };
        let holder =
            parts
                .iter()
                .map(|p| match p.as_slice() {
                    [edge] => edge.name.as_deref().map(q).ok_or_else(|| {
                        emit_error(format!("reference {} has an unnamed part", r.id))
                    }),
                    _ => Err(emit_error(format!("reference {} has no SQL form", r.id))),
                })
                .collect::<Result<Vec<_>, _>>()?;
        let key = r
            .target_key
            .as_ref()
            .and_then(|k| schema.declarations.key(&r.target_vertex, k))
            .ok_or_else(|| emit_error(format!("reference {} names no key", r.id)))?;
        let _ = write!(
            out,
            "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            q(t),
            q(name),
            holder.join(", "),
            q(&r.target_vertex),
            key_columns(key)?.join(", ")
        );
        if r.null_match == NullMatch::Full {
            out.push_str(" MATCH FULL");
        }
        let prefix = format!("{} ", q(name));
        if let Some(actions) = values(schema, t, "FOREIGN KEY ACTION")
            .into_iter()
            .find_map(|v| v.strip_prefix(&prefix))
        {
            let _ = write!(out, " {actions}");
        }
        out.push_str(";\n");
    }
    Ok(())
}

/// Well-formedness rules for the protocol's edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["table".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "foreign-key".into(),
            src_kinds: vec!["table".into()],
            tgt_kinds: vec!["table".into()],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: vec![],
        },
    ]
}
