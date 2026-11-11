//! The SQL DDL reader reads `PostgreSQL` 17 DDL exactly or refuses it with a
//! typed error naming the place: column types map to vertex kinds with
//! their value parameters, keys and foreign keys become schema keys and
//! references, statements outside a table schema are refused or listed,
//! and the canonical text it writes reads back as the same schema.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_protocols::database::sql::{
    OtherStatements, SqlDdlError, emit_ddl, parse_ddl, read_ddl,
};
use panproto_schema::{
    KeyEquality, KeyPart, KeyRole, NullMatch, RefSource, Schema, SqlCollation, SqlPad,
    canonical_bytes,
};

fn read(ddl: &str) -> Schema {
    read_ddl(ddl, OtherStatements::Refuse)
        .unwrap_or_else(|e| panic!("{ddl}: {e}"))
        .schema
}

fn refusal(ddl: &str) -> SqlDdlError {
    read_ddl(ddl, OtherStatements::Refuse).expect_err(ddl)
}

fn kind<'a>(schema: &'a Schema, vertex: &str) -> &'a str {
    schema
        .vertices
        .get(vertex)
        .unwrap_or_else(|| panic!("no vertex {vertex}"))
        .kind
        .as_ref()
}

fn constraints(schema: &Schema, vertex: &str) -> Vec<(String, String)> {
    schema
        .constraints
        .get(vertex)
        .map(|cs| {
            cs.iter()
                .map(|c| (c.sort.to_string(), c.value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

/// The schema reads back from its canonical text, and the text is a fixed
/// point.
fn assert_round_trip(schema: &Schema) {
    let text = emit_ddl(schema).expect("emit");
    let again = parse_ddl(&text).unwrap_or_else(|e| panic!("{text}\n{e}"));
    assert_eq!(canonical_bytes(schema), canonical_bytes(&again), "{text}");
    assert_eq!(emit_ddl(&again).expect("emit"), text);
}

#[test]
fn each_built_in_type_reads_as_its_kind_and_parameters() {
    let schema = read(
        "CREATE TABLE t (a smallint, b int, c bigint, d numeric(7,2), e numeric, f real, \
         g double precision, h float(10), i text, j varchar(5), k character(3), l char, \
         m boolean, n bytea, o date, p time, q timestamp, r timestamptz, s uuid, \
         u json, v jsonb, w int4[][], x float);",
    );
    let expect = [
        ("t.a", "integer", vec![("int_bits", "16")]),
        ("t.b", "integer", vec![("int_bits", "32")]),
        ("t.c", "integer", vec![("int_bits", "64")]),
        ("t.d", "number", vec![("precision", "7"), ("scale", "2")]),
        ("t.e", "number", vec![]),
        ("t.f", "real", vec![]),
        ("t.g", "double", vec![]),
        ("t.h", "real", vec![]),
        ("t.i", "string", vec![]),
        ("t.j", "string", vec![("max_length", "5")]),
        ("t.k", "character", vec![("max_length", "3")]),
        ("t.l", "character", vec![("max_length", "1")]),
        ("t.m", "boolean", vec![]),
        ("t.n", "bytes", vec![]),
        ("t.o", "date", vec![]),
        ("t.p", "time", vec![]),
        ("t.q", "timestamp", vec![("with_time_zone", "false")]),
        ("t.r", "timestamp", vec![("with_time_zone", "true")]),
        ("t.s", "uuid", vec![]),
        ("t.u", "json", vec![]),
        ("t.v", "jsonb", vec![]),
        ("t.w:items:items", "integer", vec![("int_bits", "32")]),
        ("t.x", "double", vec![]),
    ];
    for (vertex, expected_kind, parameters) in expect {
        assert_eq!(kind(&schema, vertex), expected_kind, "{vertex}");
        assert_eq!(constraints(&schema, vertex), pairs(&parameters), "{vertex}");
    }
    assert_eq!(kind(&schema, "t.w"), "array");
    assert_eq!(kind(&schema, "t.w:items"), "array");
    assert_round_trip(&schema);
}

#[test]
fn columns_keep_their_declared_positions() {
    let schema = read("CREATE TABLE t (z int, a int, m int);");
    let mut order: Vec<(u32, String)> = schema
        .orderings
        .iter()
        .map(|(e, p)| (*p, e.name.as_deref().unwrap_or_default().to_owned()))
        .collect();
    order.sort();
    let names: Vec<&str> = order.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, ["z", "a", "m"]);
    let text = emit_ddl(&schema).expect("emit");
    assert!(
        text.contains("    z integer,\n    a integer,\n    m integer\n"),
        "{text}"
    );
}

#[test]
fn keys_are_named_as_postgresql_names_them_and_imply_not_null() {
    let schema = read(
        "CREATE TABLE t (a int PRIMARY KEY, b int UNIQUE, c char(2), d int, UNIQUE (c, d));\n\
         CREATE UNIQUE INDEX ON t (d);",
    );
    let keys = &schema.declarations.keys["t"];
    let mut ids: Vec<&str> = keys.iter().map(|k| k.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["t_b_key", "t_c_d_key", "t_d_idx", "t_pkey"]);
    let pkey = schema.declarations.key("t", "t_pkey").expect("primary key");
    assert_eq!(pkey.role, KeyRole::Primary);
    assert!(constraints(&schema, "t.a").contains(&("NOT NULL".into(), "true".into())));
    let composite = schema
        .declarations
        .key("t", "t_c_d_key")
        .expect("unique key");
    assert_eq!(composite.role, KeyRole::Unique);
    assert_eq!(
        composite.equality,
        vec![
            KeyEquality::Sql {
                collation: SqlCollation::Binary,
                pad: SqlPad::PadSpace
            },
            KeyEquality::Sql {
                collation: SqlCollation::Binary,
                pad: SqlPad::NoPad
            },
        ]
    );
    assert_round_trip(&schema);
}

#[test]
fn a_foreign_key_resolves_to_the_key_its_columns_name() {
    let schema = read(
        "CREATE TABLE p (x int, y text, PRIMARY KEY (x, y));\n\
         CREATE TABLE c (a text, b int, n int REFERENCES c, id int PRIMARY KEY,\n\
           CONSTRAINT to_p FOREIGN KEY (a, b) REFERENCES p (y, x) MATCH FULL ON DELETE CASCADE);",
    );
    let r = &schema.declarations.references["c.to_p"];
    assert_eq!(r.target_vertex.as_str(), "p");
    assert_eq!(r.target_key.as_deref(), Some("p_pkey"));
    assert_eq!(r.null_match, NullMatch::Full);
    assert!(r.nullable);
    let RefSource::KeyValue { parts } = &r.source else {
        panic!("a key-value reference")
    };
    // Paired as written, (a, y) and (b, x), and ordered by the key (x, y).
    let holder: Vec<&str> = parts
        .iter()
        .map(|p| p[0].name.as_deref().unwrap_or_default())
        .collect();
    assert_eq!(holder, ["b", "a"]);
    let self_ref = &schema.declarations.references["c.c_n_fkey"];
    assert_eq!(self_ref.target_key.as_deref(), Some("c_pkey"));
    assert!(
        constraints(&schema, "c")
            .contains(&("FOREIGN KEY ACTION".into(), "to_p ON DELETE CASCADE".into()))
    );
    let pkey = schema.declarations.key("p", "p_pkey").expect("key");
    assert!(matches!(&pkey.parts[0], KeyPart::Path(p) if p[0].name.as_deref() == Some("x")));
    assert_round_trip(&schema);
}

#[test]
fn enums_domains_serials_checks_and_defaults_round_trip() {
    let schema = read(
        "CREATE TYPE mood AS ENUM ('sad', 'it''s ok', E'a\\tb');\n\
         CREATE DOMAIN year AS integer CONSTRAINT year_check CHECK (VALUE >= 1901) NOT NULL;\n\
         CREATE TABLE \"Odd Name\" (id serial, m mood[], born year, \"select\" text DEFAULT 'x'::text,\n\
           note varchar(10) CHECK (length(note) > 0), CONSTRAINT positive CHECK (id > 0));",
    );
    let t = "Odd Name";
    assert_eq!(kind(&schema, "Odd Name.m:items"), "string");
    assert_eq!(
        constraints(&schema, "Odd Name.m:items"),
        pairs(&[("TYPE", "mood"), ("ENUM", r#"["sad","it's ok","a\tb"]"#)])
    );
    assert_eq!(
        constraints(&schema, "Odd Name.born"),
        pairs(&[
            ("int_bits", "32"),
            ("TYPE", "year"),
            (
                "DOMAIN CHECK",
                "CONSTRAINT year_check CHECK (VALUE >= 1901)"
            ),
            ("NOT NULL", "true"),
        ])
    );
    assert_eq!(
        constraints(&schema, "Odd Name.id"),
        pairs(&[
            ("int_bits", "32"),
            ("NOT NULL", "true"),
            ("DEFAULT", r#"nextval('"Odd Name_id_seq"'::regclass)"#),
        ])
    );
    assert_eq!(
        constraints(&schema, t),
        pairs(&[
            ("CHECK", "CHECK (length(note) > 0)"),
            ("CHECK", "CONSTRAINT positive CHECK (id > 0)"),
        ])
    );
    assert_round_trip(&schema);
}

#[test]
fn statements_take_effect_in_script_order() {
    let schema = read(
        "DROP TABLE IF EXISTS t;\nCREATE TABLE t (id int);\nCREATE TABLE IF NOT EXISTS t (other int);\n\
         ALTER TABLE ONLY public.t ADD COLUMN name text NOT NULL, ADD CONSTRAINT t_pk PRIMARY KEY (id);\n\
         CREATE TABLE gone (x int);\nDROP TABLE gone;",
    );
    assert!(schema.has_vertex("t.name"));
    assert!(!schema.has_vertex("t.other"));
    assert!(!schema.has_vertex("gone"));
    assert!(schema.declarations.key("t", "t_pk").is_some());
}

#[test]
fn unquoted_names_fold_to_lowercase_and_quoted_names_do_not() {
    let schema = read("CREATE TABLE Users (Id int, \"Id\" text);");
    assert!(schema.has_vertex("users.id"));
    assert!(schema.has_vertex("users.Id"));
    assert_round_trip(&schema);
}

#[test]
fn every_refusal_names_its_class_and_place() {
    let cases: &[(&str, &str, &str)] = &[
        ("CREATE TABLE t (a money);", "Unsupported", "money"),
        ("CREATE TABLE t (a geometry);", "UnknownType", "geometry"),
        ("CREATE TABLE t (a int REFERENCES u);", "UnknownTable", "u"),
        (
            "CREATE TABLE u (x int);\nCREATE TABLE t (a int REFERENCES u);",
            "NoMatchingKey",
            "u",
        ),
        (
            "CREATE TABLE u (x int UNIQUE, y int);\nCREATE TABLE t (a int REFERENCES u (y));",
            "NoMatchingKey",
            "u",
        ),
        ("CREATE TABLE t (a int, a text);", "Duplicate", "a"),
        (
            "CREATE TABLE t (a int, PRIMARY KEY (b));",
            "UnknownColumn",
            "b",
        ),
        (
            "CREATE TABLE t (a int PRIMARY KEY, b int PRIMARY KEY);",
            "Duplicate",
            "PRIMARY",
        ),
        ("CREATE TABLE t (`a` int);", "Unsupported", "`a`"),
        ("CREATE TABLE t ([a] int);", "Unsupported", "[a]"),
        (
            "CREATE TABLE t (a int) INHERITS (u);",
            "Unsupported",
            "INHERITS",
        ),
        (
            "CREATE TABLE t (a text COLLATE \"en_US\");",
            "Unsupported",
            "\"en_US\"",
        ),
        (
            "CREATE TABLE t (a int GENERATED ALWAYS AS IDENTITY);",
            "Unsupported",
            "GENERATED",
        ),
        ("CREATE TABLE t (a timestamp(3));", "Unsupported", "(3)"),
        (
            "CREATE TABLE t (a int);\nCREATE VIEW v AS SELECT 1;",
            "Unsupported",
            "CREATE VIEW",
        ),
        ("CREATE TABLE t (a int,);", "Syntax", ")"),
        ("CREATE TABLE t (a text DEFAULT 'x);", "Lex", "'x);"),
        ("CREATE TABLE other.t (a int);", "Unsupported", "other.t"),
        (
            "CREATE TABLE t (a int, CONSTRAINT e EXCLUDE USING gist (a WITH =));",
            "Unsupported",
            "EXCLUDE",
        ),
        (
            "CREATE TABLE u (x int PRIMARY KEY);\nCREATE TABLE t (a int REFERENCES u);\nDROP TABLE u;",
            "Unsupported",
            "u;",
        ),
    ];
    for (ddl, class, at) in cases {
        let error = refusal(ddl);
        assert_eq!(error.class(), *class, "{ddl}: {error}");
        assert!(
            ddl[error.offset()..].starts_with(at),
            "{ddl}: {error} names byte {}, not {at:?}",
            error.offset()
        );
    }
}

#[test]
fn statements_outside_a_table_schema_are_listed_when_asked() {
    let ddl = "SET search_path = public;\nCREATE TABLE t (a int);\n\
               COPY t (a) FROM stdin;\n1\n2\n\\.\n\
               CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; $$ LANGUAGE sql;\n\
               ALTER TABLE t OWNER TO postgres;\nCREATE INDEX t_a ON t (a);";
    let reading = read_ddl(ddl, OtherStatements::List).expect("listed");
    let listed: Vec<(&str, &str)> = reading
        .not_read
        .iter()
        .map(|n| (n.statement, &ddl[n.offset..n.offset + 4]))
        .collect();
    assert_eq!(
        listed,
        [
            ("SET", "SET "),
            ("COPY", "COPY"),
            ("CREATE FUNCTION", "CREA"),
            ("ALTER TABLE OWNER TO", "ALTE"),
            ("CREATE INDEX", "CREA"),
        ]
    );
    assert!(reading.schema.has_vertex("t.a"));
    assert_eq!(refusal(ddl).class(), "Unsupported");
    let other_path = read_ddl("SET search_path = app;", OtherStatements::List).expect_err("path");
    assert_eq!(other_path.class(), "Unsupported");
}
