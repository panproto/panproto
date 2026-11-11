//! Complete reference operations assigned to `sql-ddl`.
//!
//! The `PostgreSQL` 17 DDL route modelled directly from the documentation:
//! column types from Chapter 8, and keys and foreign keys from the
//! `CREATE TABLE` reference page, over a script model of tables with
//! integer columns. The model shares no code with the production reader,
//! which lexes and parses the rendered text.

use std::fmt::Write as _;

/// What a column type spelling reads as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnTypeReading {
    /// The vertex kind of the element.
    pub kind: &'static str,
    /// The element's value parameters, `(sort, value)`, sorted.
    pub parameters: Vec<(&'static str, String)>,
    /// The number of array dimensions.
    pub dims: usize,
    /// The column's own constraints that a serial type adds, `(sort,
    /// value)`, sorted.
    pub column: Vec<(&'static str, String)>,
}

/// Why a column type spelling is refused, by the refusal's class name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeRefusal {
    /// A `PostgreSQL` type or modifier the route does not carry.
    Unsupported,
    /// A name that is no built-in type.
    UnknownType,
    /// A modifier outside the range `PostgreSQL` accepts.
    Syntax,
}

/// The type of a column named `c` of table `t` declared with `spelling`.
///
/// `spelling` is lowercase with single spaces, as the enumerator writes it.
/// Table 8.1 gives the names and aliases; a serial type is an integer
/// column, `NOT NULL`, whose default draws from `t_c_seq` (Sec 8.1.4).
///
/// # Errors
///
/// Returns the [`TypeRefusal`] the route gives `spelling`.
pub fn column_type(spelling: &str) -> Result<ColumnTypeReading, TypeRefusal> {
    let (base, dims) = split_dimensions(spelling);
    let (name, modifiers) = match base.split_once('(') {
        Some((name, rest)) => (
            name.trim(),
            rest.trim_end_matches(')')
                .split(',')
                .map(|m| m.trim().parse::<u32>().map_err(|_| TypeRefusal::Syntax))
                .collect::<Result<Vec<u32>, _>>()?,
        ),
        None => (base.trim(), Vec::new()),
    };
    let bits = match name {
        "smallserial" | "serial2" => Some(16),
        "serial" | "serial4" => Some(32),
        "bigserial" | "serial8" => Some(64),
        _ => None,
    };
    if let Some(bits) = bits {
        if dims > 0 || !modifiers.is_empty() {
            return Err(TypeRefusal::Unsupported);
        }
        return Ok(ColumnTypeReading {
            kind: "integer",
            parameters: vec![("int_bits", bits.to_string())],
            dims,
            column: vec![
                ("DEFAULT", "nextval('t_c_seq'::regclass)".to_owned()),
                ("NOT NULL", "true".to_owned()),
            ],
        });
    }
    let (kind, parameters) = element(name, &modifiers)?;
    Ok(ColumnTypeReading {
        kind,
        parameters,
        dims,
        column: Vec::new(),
    })
}

/// Value parameters, `(sort, value)`.
type Parameters = Vec<(&'static str, String)>;

/// A length modifier, which `PostgreSQL` bounds by 1 and 10485760.
fn length(n: u32) -> Result<String, TypeRefusal> {
    if (1..=10_485_760).contains(&n) {
        Ok(n.to_string())
    } else {
        Err(TypeRefusal::Syntax)
    }
}

/// The element kind and value parameters of the built-in type `name` with
/// type modifiers `modifiers`.
fn element(name: &str, modifiers: &[u32]) -> Result<(&'static str, Parameters), TypeRefusal> {
    let plain = |kind| Ok((kind, Vec::new()));
    match (name, modifiers) {
        ("smallint" | "int2", []) => Ok(("integer", vec![("int_bits", "16".into())])),
        ("integer" | "int" | "int4", []) => Ok(("integer", vec![("int_bits", "32".into())])),
        ("bigint" | "int8", []) => Ok(("integer", vec![("int_bits", "64".into())])),
        ("numeric" | "decimal", []) => plain("number"),
        ("numeric" | "decimal", [p, rest @ ..]) if rest.len() <= 1 => {
            let s = rest.first().copied().unwrap_or(0);
            if !(1..=1000).contains(p) {
                Err(TypeRefusal::Syntax)
            } else if s > *p {
                Err(TypeRefusal::Unsupported)
            } else {
                Ok((
                    "number",
                    vec![("precision", p.to_string()), ("scale", s.to_string())],
                ))
            }
        }
        ("real" | "float4", []) => plain("real"),
        ("double precision" | "float8" | "float", []) => plain("double"),
        ("float", [p]) => match p {
            1..=24 => plain("real"),
            25..=53 => plain("double"),
            _ => Err(TypeRefusal::Syntax),
        },
        ("text" | "varchar" | "character varying", []) => plain("string"),
        ("varchar" | "character varying", [n]) => Ok(("string", vec![("max_length", length(*n)?)])),
        ("char" | "character", []) => Ok(("character", vec![("max_length", "1".into())])),
        ("char" | "character" | "bpchar", [n]) => {
            Ok(("character", vec![("max_length", length(*n)?)]))
        }
        ("boolean" | "bool", []) => plain("boolean"),
        ("bytea", []) => plain("bytes"),
        ("date", []) => plain("date"),
        ("time" | "time without time zone", []) => plain("time"),
        ("timestamp" | "timestamp without time zone", []) => {
            Ok(("timestamp", vec![("with_time_zone", "false".into())]))
        }
        ("timestamptz" | "timestamp with time zone", []) => {
            Ok(("timestamp", vec![("with_time_zone", "true".into())]))
        }
        ("uuid", []) => plain("uuid"),
        ("json", []) => plain("json"),
        ("jsonb", []) => plain("jsonb"),
        // A blank-padded type with no length, fractional-second precision,
        // `time with time zone`, and the types of Chapter 8 the route does
        // not carry.
        ("bpchar", [])
        | ("time" | "timestamp" | "timestamptz", [_])
        | (
            "time with time zone"
            | "timetz"
            | "money"
            | "interval"
            | "xml"
            | "tsvector"
            | "inet"
            | "point",
            _,
        ) => Err(TypeRefusal::Unsupported),
        (
            "smallint" | "int2" | "integer" | "int" | "int4" | "bigint" | "int8" | "real"
            | "float4" | "float8" | "text" | "boolean" | "bool" | "bytea" | "date" | "uuid"
            | "json" | "jsonb" | "double precision",
            _,
        ) => Err(TypeRefusal::Syntax),
        _ => Err(TypeRefusal::UnknownType),
    }
}

/// A spelling without its array suffix, and the number of dimensions the
/// suffix declares: `[]` and `[n]` one each, `array` one.
fn split_dimensions(spelling: &str) -> (&str, usize) {
    if let Some(base) = spelling.strip_suffix(" array") {
        return (base, 1);
    }
    let mut base = spelling;
    let mut dims = 0;
    while let Some(inner) = base.strip_suffix(']') {
        let Some(open) = inner.rfind('[') else { break };
        base = &inner[..open];
        dims += 1;
    }
    (base, dims)
}

/// A column's own constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnConstraint {
    /// None.
    None,
    /// `NOT NULL`.
    NotNull,
    /// `PRIMARY KEY`.
    PrimaryKey,
    /// `UNIQUE`.
    Unique,
}

/// An `integer` column: its constraint and the table its `REFERENCES`
/// clause names, by index, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    /// The column constraint.
    pub constraint: ColumnConstraint,
    /// The table referenced with no column list.
    pub references: Option<usize>,
}

/// A table key over the first two columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableKey {
    /// None.
    None,
    /// `PRIMARY KEY (c0, c1)`.
    PrimaryKey,
    /// `UNIQUE (c0, c1)`.
    Unique,
}

/// A table foreign key over the first two columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableForeignKey {
    /// None.
    None,
    /// `FOREIGN KEY (c0, c1) REFERENCES <table>`.
    Implicit(usize),
    /// `FOREIGN KEY (c1, c0) REFERENCES <table> (c0, c1)`.
    Swapped(usize),
}

/// A table: its columns and the table constraints after them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    /// The columns, named `a`, `b`, ... in order.
    pub columns: Vec<Column>,
    /// The table key.
    pub key: TableKey,
    /// The table foreign key.
    pub foreign_key: TableForeignKey,
}

/// A script: one `CREATE TABLE` per table, the tables named `t`, `u`, `v`,
/// `w` in order.
pub type Script = Vec<Table>;

const TABLE_NAMES: [&str; 4] = ["t", "u", "v", "w"];
const COLUMN_NAMES: [&str; 4] = ["a", "b", "c", "d"];

/// The script's text: one statement per line.
#[must_use]
pub fn render(script: &Script) -> String {
    let mut out = String::new();
    for (i, table) in script.iter().enumerate() {
        let mut elements: Vec<String> = Vec::new();
        for (k, column) in table.columns.iter().enumerate() {
            let mut element = format!("{} integer", COLUMN_NAMES[k]);
            element.push_str(match column.constraint {
                ColumnConstraint::None => "",
                ColumnConstraint::NotNull => " NOT NULL",
                ColumnConstraint::PrimaryKey => " PRIMARY KEY",
                ColumnConstraint::Unique => " UNIQUE",
            });
            if let Some(j) = column.references {
                let _ = write!(element, " REFERENCES {}", TABLE_NAMES[j]);
            }
            elements.push(element);
        }
        match table.key {
            TableKey::None => {}
            TableKey::PrimaryKey => elements.push("PRIMARY KEY (a, b)".to_owned()),
            TableKey::Unique => elements.push("UNIQUE (a, b)".to_owned()),
        }
        match table.foreign_key {
            TableForeignKey::None => {}
            TableForeignKey::Implicit(j) => {
                elements.push(format!("FOREIGN KEY (a, b) REFERENCES {}", TABLE_NAMES[j]));
            }
            TableForeignKey::Swapped(j) => {
                elements.push(format!(
                    "FOREIGN KEY (b, a) REFERENCES {} (a, b)",
                    TABLE_NAMES[j]
                ));
            }
        }
        let _ = writeln!(
            out,
            "CREATE TABLE {} ({});",
            TABLE_NAMES[i],
            elements.join(", ")
        );
    }
    out
}

/// A key a table declares.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyReading {
    /// Its name.
    pub name: String,
    /// Whether it is the primary key.
    pub primary: bool,
    /// Its columns, in key order.
    pub columns: Vec<String>,
}

/// A foreign key a table declares.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReferenceReading {
    /// Its name.
    pub name: String,
    /// The referencing columns, in the order of the target key's parts.
    pub columns: Vec<String>,
    /// The referenced table.
    pub target: String,
    /// The referenced key.
    pub target_key: String,
    /// Whether some referencing column may be null.
    pub nullable: bool,
}

/// A table as read.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableReading {
    /// Its name.
    pub name: String,
    /// Its columns, in order, with whether each is `NOT NULL`.
    pub columns: Vec<(String, bool)>,
    /// Its keys, by name.
    pub keys: Vec<KeyReading>,
    /// Its foreign keys, by name.
    pub references: Vec<ReferenceReading>,
}

/// Why a script is refused, by the refusal's class name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptRefusal {
    /// A second primary key.
    Duplicate,
    /// A table not declared before the statement.
    UnknownTable,
    /// A referenced column the referenced table lacks.
    UnknownColumn,
    /// No primary or unique key on the referenced columns.
    NoMatchingKey,
}

/// The tables a script declares, sorted by name, or the index of the
/// first refused statement and why.
///
/// Each statement first declares its columns, then its keys in textual
/// order (a primary key makes its columns `NOT NULL`; a second one is
/// refused), then its foreign keys in textual order. A key is named as
/// `PostgreSQL` names it: `<table>_pkey`, `<table>_<columns>_key`, and a
/// foreign key `<table>_<columns>_fkey`, with the columns as written. A
/// foreign key names a table declared by an earlier statement or its own.
/// With no column list it references the table's primary key; with one,
/// the primary key, else the first unique key, whose columns are exactly
/// those listed; the column counts must agree. Its columns pair with the
/// referenced ones as written and are listed in the key's order.
///
/// # Errors
///
/// Returns the index of the first refused statement and its
/// [`ScriptRefusal`].
pub fn read_script(script: &Script) -> Result<Vec<TableReading>, (usize, ScriptRefusal)> {
    let mut tables: Vec<TableReading> = Vec::new();
    for (i, table) in script.iter().enumerate() {
        let name = TABLE_NAMES[i].to_owned();
        let mut reading = TableReading {
            name: name.clone(),
            columns: table
                .columns
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    (
                        COLUMN_NAMES[k].to_owned(),
                        c.constraint == ColumnConstraint::NotNull,
                    )
                })
                .collect(),
            keys: Vec::new(),
            references: Vec::new(),
        };
        let mut keys: Vec<(bool, Vec<String>)> = Vec::new();
        for (k, c) in table.columns.iter().enumerate() {
            match c.constraint {
                ColumnConstraint::PrimaryKey => keys.push((true, vec![COLUMN_NAMES[k].into()])),
                ColumnConstraint::Unique => keys.push((false, vec![COLUMN_NAMES[k].into()])),
                _ => {}
            }
        }
        let pair = vec!["a".to_owned(), "b".to_owned()];
        match table.key {
            TableKey::PrimaryKey => keys.push((true, pair.clone())),
            TableKey::Unique => keys.push((false, pair.clone())),
            TableKey::None => {}
        }
        for (primary, columns) in keys {
            if primary {
                if reading.keys.iter().any(|k| k.primary) {
                    return Err((i, ScriptRefusal::Duplicate));
                }
                for column in &mut reading.columns {
                    if columns.contains(&column.0) {
                        column.1 = true;
                    }
                }
            }
            let key_name = if primary {
                format!("{name}_pkey")
            } else {
                format!("{name}_{}_key", columns.join("_"))
            };
            reading.keys.push(KeyReading {
                name: key_name,
                primary,
                columns,
            });
        }
        let mut foreign: Vec<(Vec<String>, usize, Option<Vec<String>>)> = Vec::new();
        for (k, c) in table.columns.iter().enumerate() {
            if let Some(j) = c.references {
                foreign.push((vec![COLUMN_NAMES[k].into()], j, None));
            }
        }
        match table.foreign_key {
            TableForeignKey::None => {}
            TableForeignKey::Implicit(j) => foreign.push((pair.clone(), j, None)),
            TableForeignKey::Swapped(j) => {
                foreign.push((vec!["b".into(), "a".into()], j, Some(pair.clone())));
            }
        }
        for (columns, j, listed) in foreign {
            let target = match j.cmp(&i) {
                std::cmp::Ordering::Equal => &reading,
                std::cmp::Ordering::Less => &tables[j],
                std::cmp::Ordering::Greater => return Err((i, ScriptRefusal::UnknownTable)),
            };
            let reference = resolve(&reading, target, &columns, listed).map_err(|r| (i, r))?;
            reading.references.push(reference);
        }
        reading.keys.sort();
        reading.references.sort();
        tables.push(reading);
    }
    tables.sort();
    Ok(tables)
}

/// The foreign key of `holder` on `columns` to `target`, whose columns
/// are `listed` or, when `None`, its primary key's.
fn resolve(
    holder: &TableReading,
    target: &TableReading,
    columns: &[String],
    listed: Option<Vec<String>>,
) -> Result<ReferenceReading, ScriptRefusal> {
    let key = match &listed {
        None => target.keys.iter().find(|k| k.primary),
        Some(listed) => {
            if listed
                .iter()
                .any(|c| !target.columns.iter().any(|(n, _)| n == c))
            {
                return Err(ScriptRefusal::UnknownColumn);
            }
            let mut wanted = listed.clone();
            wanted.sort();
            let matches = |k: &&KeyReading| {
                let mut have = k.columns.clone();
                have.sort();
                have == wanted
            };
            target
                .keys
                .iter()
                .filter(|k| k.primary)
                .find(matches)
                .or_else(|| target.keys.iter().filter(|k| !k.primary).find(matches))
        }
    };
    let key = key.ok_or(ScriptRefusal::NoMatchingKey)?;
    let referenced = listed.unwrap_or_else(|| key.columns.clone());
    if referenced.len() != columns.len() {
        return Err(ScriptRefusal::NoMatchingKey);
    }
    let ordered: Vec<String> = key
        .columns
        .iter()
        .map(|kc| {
            let at = referenced.iter().position(|r| r == kc).unwrap_or(0);
            columns[at].clone()
        })
        .collect();
    let nullable = columns.iter().any(|c| {
        holder
            .columns
            .iter()
            .any(|(n, not_null)| n == c && !not_null)
    });
    Ok(ReferenceReading {
        name: format!("{}_{}_fkey", holder.name, columns.join("_")),
        columns: ordered,
        target: target.name.clone(),
        target_key: key.name.clone(),
        nullable,
    })
}

/// The canonical text of the tables read.
///
/// Each table by name, its columns in order, then its primary key and its
/// unique keys by name, each a named constraint; then each foreign key, by
/// table and name, as an `ALTER TABLE ... ADD CONSTRAINT`.
#[must_use]
pub fn canonical_text(tables: &[TableReading]) -> String {
    let mut sorted: Vec<&TableReading> = tables.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut bodies = String::new();
    let mut foreign = String::new();
    for table in sorted {
        let mut lines: Vec<String> = table
            .columns
            .iter()
            .map(|(name, not_null)| {
                format!(
                    "    {name} integer{}",
                    if *not_null { " NOT NULL" } else { "" }
                )
            })
            .collect();
        let mut keys: Vec<&KeyReading> = table.keys.iter().collect();
        keys.sort_by(|a, b| (!a.primary, &a.name).cmp(&(!b.primary, &b.name)));
        for key in keys {
            lines.push(format!(
                "    CONSTRAINT {} {} ({})",
                key.name,
                if key.primary { "PRIMARY KEY" } else { "UNIQUE" },
                key.columns.join(", ")
            ));
        }
        let _ = write!(
            bodies,
            "CREATE TABLE {} (\n{}\n);\n\n",
            table.name,
            lines.join(",\n")
        );
        let mut references: Vec<&ReferenceReading> = table.references.iter().collect();
        references.sort_by(|a, b| a.name.cmp(&b.name));
        for r in references {
            let key = tables
                .iter()
                .find(|t| t.name == r.target)
                .and_then(|t| t.keys.iter().find(|k| k.name == r.target_key))
                .map(|k| k.columns.join(", "))
                .unwrap_or_default();
            let _ = writeln!(
                foreign,
                "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({key});",
                table.name,
                r.name,
                r.columns.join(", "),
                r.target
            );
        }
    }
    bodies + &foreign
}
