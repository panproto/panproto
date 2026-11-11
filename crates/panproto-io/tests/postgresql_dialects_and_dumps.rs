//! `PostgreSQL` dialect constructs and dump constructs read as `PostgreSQL`
//! defines them, and survive an emit and re-parse.
//!
//! The dialect cases: an identifier quoted as another SQL dialect quotes
//! it, refused, an array bound after a column type, a `DROP TABLE` that
//! names its table in another case or qualification, and enum labels
//! holding a comma or an escaped quote. The dump cases are the constructs
//! a `pg_dump` script holds: comments, dollar-quoted function bodies,
//! `ALTER TABLE ONLY ... ADD CONSTRAINT`, domains, enum types, array
//! columns and foreign keys to keys of other names.

#![cfg(feature = "tree-sitter")]

use panproto_io::postgresql::{parse_ddl, read_ddl};
use panproto_protocols::database::sql::{OtherStatements, emit_ddl};
use panproto_schema::Schema;

/// A result a test requires to be `Ok`.
trait Required<T> {
    /// The value, or a panic naming `what` and the error.
    fn required(self, what: &str) -> T;
}

impl<T, E: std::fmt::Display> Required<T> for Result<T, E> {
    fn required(self, what: &str) -> T {
        self.unwrap_or_else(|e| panic!("{what}: {e}"))
    }
}

fn has_edge(schema: &Schema, src: &str, tgt: &str, kind: &str) -> bool {
    schema
        .outgoing_edges(src)
        .iter()
        .any(|e| e.tgt == tgt && e.kind == kind)
}

fn kind<'a>(schema: &'a Schema, vertex: &str) -> &'a str {
    schema
        .vertices
        .get(vertex)
        .unwrap_or_else(|| panic!("no vertex {vertex}"))
        .kind
        .as_ref()
}

/// The ids of a schema's vertices, in order.
fn vertex_ids(schema: &Schema) -> std::collections::BTreeSet<String> {
    schema.vertices.keys().map(ToString::to_string).collect()
}

fn constraint(schema: &Schema, vertex: &str, sort: &str) -> Option<String> {
    constraint_str(schema, vertex, sort).map(str::to_owned)
}

fn constraint_str<'a>(schema: &'a Schema, vertex: &str, sort: &str) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex)?
        .iter()
        .find(|c| c.sort == sort)
        .map(|c| c.value.as_str())
}

// Dialect constructs.

#[test]
fn a_bracket_quoted_identifier_is_refused_as_another_dialect() {
    let error = parse_ddl("CREATE TABLE [t] ([id] [int] NOT NULL);")
        .err()
        .unwrap_or_else(|| panic!("T-SQL quoting is not PostgreSQL"));
    assert!(
        error
            .to_string()
            .contains("T-SQL bracket-quoted identifier"),
        "{error}"
    );
    let error = parse_ddl("CREATE TABLE t (`id` int);")
        .err()
        .unwrap_or_else(|| panic!("MySQL quoting"));
    assert!(
        error.to_string().contains("MySQL backquoted identifier"),
        "{error}"
    );
}
#[test]
fn a_bracket_after_the_column_type_is_an_array_bound() {
    let schema = parse_ddl(
        "CREATE TABLE t (\
            a VARCHAR(10) CHECK (a <> '[x]'), \
            c INTEGER [3], \
            d INTEGER ARRAY, \
            e TIMESTAMP WITH TIME ZONE[]\
        );",
    )
    .required("DDL parses");
    assert_eq!(kind(&schema, "t.a"), "string");
    assert!(!schema.has_vertex("t.a:items"));
    for array in ["t.c", "t.d", "t.e"] {
        assert_eq!(kind(&schema, array), "array", "{array}");
        assert!(
            !schema.has_vertex(&format!("{array}:items:items")),
            "{array}"
        );
    }
    assert_eq!(kind(&schema, "t.c:items"), "integer");
    assert_eq!(kind(&schema, "t.e:items"), "timestamp");
}
#[test]
fn a_drop_removes_its_create_whatever_the_case_of_an_unquoted_name() {
    let schema =
        parse_ddl("CREATE TABLE Foo (id INT); DROP TABLE foo; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(!schema.has_vertex("Foo"), "FOO and foo name one table");
    assert!(schema.has_vertex("keep"));

    let error = parse_ddl("CREATE TABLE \"Foo\" (id INT); DROP TABLE foo;")
        .err()
        .unwrap_or_else(|| panic!("a quoted name keeps its case, so foo names no table"));
    assert!(
        error.to_string().contains("unknown SQL table \"foo\""),
        "{error}"
    );
}

#[test]
fn a_drop_removes_its_create_whichever_side_is_qualified() {
    let schema =
        parse_ddl("CREATE TABLE public.t (id INT); DROP TABLE t; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(!schema.has_vertex("t"), "t names public.t");

    let schema =
        parse_ddl("CREATE TABLE t (id INT); DROP TABLE public.t; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(!schema.has_vertex("t"), "public.t names t");

    let error = parse_ddl("CREATE TABLE a.t (id INT);")
        .err()
        .unwrap_or_else(|| panic!("schema a is not read"));
    assert!(
        error.to_string().contains("a schema other than public"),
        "{error}"
    );
}
#[test]
fn an_enum_label_reads_the_same_however_it_is_spelled() {
    let doubled = parse_ddl("CREATE TYPE q AS ENUM ('it''s', 'a,b'); CREATE TABLE t (m q);")
        .required("DDL parses");
    let escaped = parse_ddl("CREATE TYPE q AS ENUM (E'it\\'s' , 'a,b'); CREATE TABLE t (m q);")
        .required("DDL parses");
    assert_eq!(
        constraint(&doubled, "t.m", "ENUM"),
        constraint(&escaped, "t.m", "ENUM"),
    );
}

#[test]
fn an_enum_survives_emit_and_reparse() {
    let ddl = "CREATE TYPE mood AS ENUM ('sad', 'a,b', 'it''s');\n\
               CREATE TABLE t (m mood NOT NULL, ms mood[]);";
    let first = parse_ddl(ddl).required("first parse");
    let emitted = emit_ddl(&first).required("emit");
    let second = parse_ddl(&emitted).required("re-parse");
    let labels = constraint(&first, "t.m", "ENUM");
    assert!(labels.is_some(), "the column carries its labels");
    assert_eq!(constraint(&second, "t.m", "ENUM"), labels, "{emitted}");
    assert_eq!(
        constraint(&second, "t.ms:items", "ENUM"),
        labels,
        "{emitted}"
    );
    assert_eq!(kind(&second, "t.ms"), "array", "{emitted}");
    assert_eq!(
        constraint(&second, "t.m", "NOT NULL").as_deref(),
        Some("true"),
        "{emitted}"
    );
}

#[test]
fn an_array_of_a_multi_word_character_type_is_an_array() {
    let schema = parse_ddl("CREATE TABLE t (c NATIONAL CHARACTER VARYING(5)[], d CHARACTER(3)[]);")
        .required("DDL parses");
    assert_eq!(kind(&schema, "t.c"), "array");
    assert_eq!(kind(&schema, "t.c:items"), "string");
    assert_eq!(kind(&schema, "t.d"), "array");
}

#[test]
fn an_array_default_and_an_odd_column_name_survive_emit_and_reparse() {
    let ddl = "CREATE TABLE t (id INT, f INT[] DEFAULT ARRAY[1, 2], \
               g TEXT CHECK (g IN ('[a]')), \"h]x\" INT, s TEXT DEFAULT 'a b');";
    let first = parse_ddl(ddl).required("first parse");
    assert_eq!(
        constraint(&first, "t.f", "DEFAULT").as_deref(),
        Some("ARRAY[1, 2]")
    );
    assert_eq!(
        constraint(&first, "t.s", "DEFAULT").as_deref(),
        Some("'a b'")
    );
    assert!(first.has_vertex("t.h]x"));
    let emitted = emit_ddl(&first).required("emit");
    let second = parse_ddl(&emitted).required("re-parse");
    for column in ["t.id", "t.f", "t.g", "t.h]x", "t.s"] {
        assert!(second.has_vertex(column), "{column} survives: {emitted}");
    }
    assert_eq!(
        constraint(&second, "t.f", "DEFAULT"),
        constraint(&first, "t.f", "DEFAULT"),
        "{emitted}"
    );
    assert_eq!(
        constraint(&second, "t.s", "DEFAULT"),
        constraint(&first, "t.s", "DEFAULT"),
        "{emitted}"
    );
}

#[test]
fn an_odd_table_name_survives_emit_and_reparse() {
    let ddl = "CREATE TABLE \"h]x\" (id INT); CREATE TABLE \"my table\" (id INT); \
               CREATE TABLE public.\"T\" (id INT);";
    let first = parse_ddl(ddl).required("first parse");
    for vertex in ["h]x", "h]x.id", "my table", "my table.id", "T", "T.id"] {
        assert!(first.has_vertex(vertex), "{vertex} parses");
    }
    let emitted = emit_ddl(&first).required("emit");
    let second = parse_ddl(&emitted).required("re-parse");
    assert_eq!(vertex_ids(&second), vertex_ids(&first), "{emitted}");
}
#[test]
fn an_enum_column_of_an_odd_table_survives_emit_and_reparse() {
    let ddl = "CREATE TYPE mood AS ENUM ('sad', 'ok'); \
               CREATE TABLE \"my table\" (\"a b\" mood, c mood[]);";
    let first = parse_ddl(ddl).required("first parse");
    let labels = constraint(&first, "my table.a b", "ENUM");
    assert!(labels.is_some(), "the column carries its labels");
    let emitted = emit_ddl(&first).required("emit");
    let second = parse_ddl(&emitted).required("re-parse");
    assert_eq!(
        constraint(&second, "my table.a b", "ENUM"),
        labels,
        "{emitted}"
    );
    assert_eq!(
        constraint(&second, "my table.c:items", "ENUM"),
        labels,
        "{emitted}"
    );
}

#[test]
fn a_quote_inside_a_quoted_identifier_is_part_of_its_name() {
    let ddl = "CREATE TABLE \"t\"\"x\" (\"a\"\"b\" INT, \"c'd\" TEXT DEFAULT 'q;r', \"e(f\" INT, \
               g INT[] DEFAULT ARRAY[[1, 2], [3, 4]]); CREATE TABLE u (\";\" INT);";
    let first = parse_ddl(ddl).required("first parse");
    for vertex in [
        "t\"x",
        "t\"x.a\"b",
        "t\"x.c'd",
        "t\"x.e(f",
        "t\"x.g",
        "u",
        "u.;",
    ] {
        assert!(first.has_vertex(vertex), "{vertex} parses");
    }
    assert_eq!(
        constraint(&first, "t\"x.g", "DEFAULT").as_deref(),
        Some("ARRAY[[1, 2], [3, 4]]")
    );
    let emitted = emit_ddl(&first).required("emit");
    let second = parse_ddl(&emitted).required("re-parse");
    assert_eq!(vertex_ids(&second), vertex_ids(&first), "{emitted}");
}

// Dump constructs.

#[test]
fn a_statement_after_a_comment_is_read() {
    let schema = parse_ddl(
        "-- the users table; it has one column\nCREATE TABLE users (\n  id INTEGER, -- the key\n  check_in DATE\n);",
    )
    .required("the table parses");
    assert!(schema.has_vertex("users.id"));
    assert_eq!(kind(&schema, "users.check_in"), "date");
}

#[test]
fn a_postgres_dump_declares_its_tables_and_lists_the_rest() {
    let ddl = r"
CREATE DOMAIN year AS integer CONSTRAINT year_check CHECK (VALUE >= 1901);
CREATE FUNCTION f() RETURNS trigger AS $$
BEGIN NEW.x := 1; RETURN NEW; END $$ LANGUAGE plpgsql;
CREATE TABLE parent (id integer NOT NULL, born year);
CREATE TABLE child (id integer, CONSTRAINT c CHECK ((id > 0)));
ALTER TABLE public.parent OWNER TO postgres;
ALTER TABLE ONLY parent ADD CONSTRAINT parent_pkey PRIMARY KEY (id);
ALTER TABLE ONLY child ADD CONSTRAINT child_fk FOREIGN KEY (id) REFERENCES parent(id);
COPY parent (id, born) FROM '/srv/parent.dat';
";
    let reading = read_ddl(ddl, OtherStatements::List).required("the dump reads");
    let schema = reading.schema;
    let listed: Vec<&str> = reading.not_read.iter().map(|n| n.statement).collect();
    assert_eq!(listed, ["CREATE FUNCTION", "ALTER TABLE OWNER TO", "COPY"]);
    assert_eq!(
        kind(&schema, "parent.born"),
        "integer",
        "through the domain"
    );
    assert!(schema.declarations.key("parent", "parent_pkey").is_some());
    let fk = &schema.declarations.references["child.child_fk"];
    assert_eq!(fk.target_key.as_deref(), Some("parent_pkey"));
    assert_eq!(
        constraint_str(&schema, "child", "CHECK"),
        Some("CONSTRAINT c CHECK ((id > 0))")
    );
    assert!(parse_ddl(ddl).is_err(), "refused when not listed");
}

#[test]
fn a_foreign_key_names_the_key_it_references() {
    let schema = parse_ddl(
        r"
CREATE TABLE a (id INTEGER PRIMARY KEY, code TEXT, n INTEGER, UNIQUE (code, n));
CREATE TABLE b (
  x INTEGER REFERENCES a(id),
  w INTEGER REFERENCES a,
  y INTEGER,
  z INTEGER,
  c TEXT,
  m INTEGER,
  FOREIGN KEY (y) REFERENCES a (id),
  FOREIGN KEY (c, m) REFERENCES a (code, n)
);
ALTER TABLE ONLY b ADD CONSTRAINT b_z_fkey FOREIGN KEY (z) REFERENCES a(id);
",
    )
    .required("the foreign keys parse");
    let target = |id: &str| {
        schema.declarations.references[id]
            .target_key
            .as_deref()
            .map(str::to_owned)
    };
    assert_eq!(target("b.b_x_fkey").as_deref(), Some("a_pkey"));
    // With no column list, the reference is to the primary key.
    assert_eq!(target("b.b_w_fkey").as_deref(), Some("a_pkey"));
    assert_eq!(target("b.b_y_fkey").as_deref(), Some("a_pkey"));
    assert_eq!(target("b.b_z_fkey").as_deref(), Some("a_pkey"));
    assert_eq!(target("b.b_c_m_fkey").as_deref(), Some("a_code_n_key"));
}

#[test]
fn a_table_dropped_before_it_is_created_is_declared() {
    let schema = parse_ddl(
        "DROP TABLE IF EXISTS t, u;\nCREATE TABLE t (id INTEGER);\n\
         CREATE TABLE u (id INTEGER);\nDROP TABLE u;",
    )
    .required("the script parses");
    assert!(schema.has_vertex("t.id"));
    assert!(!schema.has_vertex("u"), "dropped after it was created");
    let error = parse_ddl(
        "CREATE TABLE u (id INTEGER);\nDROP TABLE u;\nALTER TABLE u ADD COLUMN gone INTEGER;",
    )
    .err()
    .unwrap_or_else(|| panic!("a dropped table is unknown"));
    assert!(error.to_string().contains("unknown SQL table"), "{error}");
}

#[test]
fn a_quoted_table_name_is_read_whole() {
    let schema = parse_ddl(
        r#"CREATE TABLE "u;v" (id INTEGER PRIMARY KEY); CREATE TABLE "a (b" ("x y" INTEGER, "id" INTEGER, FOREIGN KEY ("x y") REFERENCES "u;v" ("id"));"#,
    )
    .required("quoted names parse");
    assert!(schema.has_vertex("u;v"));
    assert!(schema.has_vertex("u;v.id"));
    assert!(schema.has_vertex("a (b.x y"));
    let fk = &schema.declarations.references["a (b.a (b_x y_fkey"];
    assert_eq!(fk.target_vertex.as_str(), "u;v");
}

#[test]
fn postgres_arrays_and_enums_keep_their_shape() {
    let schema = parse_ddl(
        r"
CREATE TYPE public.mood AS ENUM ('sad', 'ok', 'it''s fine');
CREATE TABLE t (
  m mood NOT NULL,
  labels text[],
  grid integer[][],
  scores numeric ARRAY
);
",
    )
    .required("arrays and enums parse");
    assert_eq!(kind(&schema, "t.m"), "string");
    assert_eq!(
        constraint_str(&schema, "t.m", "ENUM"),
        Some(r#"["sad","ok","it's fine"]"#)
    );
    assert_eq!(constraint_str(&schema, "t.m", "NOT NULL"), Some("true"));
    assert_eq!(kind(&schema, "t.labels"), "array");
    assert!(has_edge(&schema, "t.labels", "t.labels:items", "items"));
    assert_eq!(kind(&schema, "t.labels:items"), "string");
    assert_eq!(kind(&schema, "t.grid"), "array");
    assert_eq!(kind(&schema, "t.grid:items"), "array");
    assert_eq!(kind(&schema, "t.grid:items:items"), "integer");
    assert_eq!(kind(&schema, "t.scores"), "array");
    assert_eq!(kind(&schema, "t.scores:items"), "number");
}
