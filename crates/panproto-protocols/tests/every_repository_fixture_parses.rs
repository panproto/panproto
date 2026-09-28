//! Every schema fixture under the repository's `fixtures/` directory
//! parses with its protocol's parser, into the schema the file declares.
//!
//! The fixtures are real-world schemas (the GitHub workflow, npm
//! `package.json` and `tsconfig.json` JSON Schemas, protobuf's own
//! `descriptor.proto`, and both dialects of the Sakila database), so each
//! one exercises constructs the parsers' unit tests do not: references to
//! definitions declared later, `type` lists next to `properties`, JSON
//! pointer references into nested subschemas, option values spanning
//! lines, and `PostgreSQL` dumps with comments, dollar-quoted function
//! bodies, `ALTER TABLE ONLY ... ADD CONSTRAINT`, `INHERITS` and domains.
//! Each construct also has a minimal case of its own below.

#![allow(clippy::expect_used)]

use std::path::{Path, PathBuf};

use panproto_protocols::{parse_schema_document, parse_schema_source};
use panproto_schema::Schema;
use serde_json::json;

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
}

fn document(protocol: &str, relative: &str) -> Schema {
    let path = fixture(relative);
    let text = std::fs::read_to_string(&path).expect("the fixture exists");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("the fixture is JSON");
    parse_schema_document(protocol, &doc).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn source(protocol: &str, relative: &str) -> Schema {
    let path = fixture(relative);
    let text = std::fs::read_to_string(&path).expect("the fixture exists");
    parse_schema_source(protocol, &text).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn has_edge(schema: &Schema, src: &str, tgt: &str, kind: &str) -> bool {
    schema
        .outgoing_edges(src)
        .iter()
        .any(|e| e.tgt == tgt && e.kind == kind)
}

fn constraint<'a>(schema: &'a Schema, vertex: &str, sort: &str) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex)?
        .iter()
        .find(|c| c.sort == sort)
        .map(|c| c.value.as_str())
}

fn kind<'a>(schema: &'a Schema, vertex: &str) -> &'a str {
    schema
        .vertices
        .get(vertex)
        .unwrap_or_else(|| panic!("no vertex {vertex}"))
        .kind
        .as_ref()
}

// JSON Schema.

#[test]
fn a_ref_to_a_definition_declared_later_resolves() {
    let schema = parse_schema_document(
        "json-schema",
        &json!({
            "definitions": {
                "a": { "$ref": "#/definitions/b" },
                "b": { "type": "string" }
            }
        }),
    )
    .expect("a forward ref parses");
    assert!(has_edge(
        &schema,
        "root:definitions/a",
        "root:definitions/b",
        "ref"
    ));
}

#[test]
fn properties_of_a_type_list_belong_to_its_object_variant() {
    let schema = parse_schema_document(
        "json-schema",
        &json!({
            "type": ["object", "null"],
            "properties": { "x": { "type": ["array", "null"], "items": { "type": "string" } } }
        }),
    )
    .expect("a type list with properties parses");
    assert_eq!(kind(&schema, "root"), "union");
    assert_eq!(kind(&schema, "root:type0"), "object");
    assert!(has_edge(&schema, "root:type0", "root.x", "prop"));
    assert!(has_edge(&schema, "root.x:type0", "root.x:items", "items"));
}

#[test]
fn a_pointer_ref_lands_on_the_subschema_it_points_at() {
    let schema = parse_schema_document(
        "json-schema",
        &json!({
            "definitions": { "a": { "properties": { "b": { "type": "integer" } } } },
            "properties": {
                "c": { "$ref": "#/definitions/a/properties/b" },
                "d": { "$ref": "#" }
            }
        }),
    )
    .expect("pointer refs parse");
    assert!(has_edge(&schema, "root.c", "root:definitions/a.b", "ref"));
    assert!(has_edge(&schema, "root.d", "root", "ref"));
}

#[test]
fn the_github_workflow_schema_parses() {
    let schema = document("json-schema", "jsonschema/github-workflow.json");
    let id = "https://json.schemastore.org/github-workflow.json";
    // `branch` refers to `globs`, which the file declares after it.
    let globs = format!("{id}:definitions/globs");
    assert!(
        schema
            .edges
            .keys()
            .any(|e| e.kind == "ref" && e.tgt == globs.as_str()),
        "a ref reaches the later definition"
    );
}

#[test]
fn the_package_json_schema_parses() {
    let schema = document("json-schema", "jsonschema/package.json");
    let id = "https://json.schemastore.org/package.json";
    // `person` is `{"type": ["object", "string"], "properties": ...}`.
    let person = format!("{id}:definitions/person");
    assert_eq!(kind(&schema, &person), "union");
    assert!(has_edge(
        &schema,
        &format!("{person}:type0"),
        &format!("{person}.name"),
        "prop"
    ));
}

#[test]
fn the_tsconfig_schema_parses() {
    let schema = document("json-schema", "jsonschema/tsconfig.json");
    let options = "root:definitions/compilerOptionsDefinition.compilerOptions";
    assert_eq!(kind(&schema, options), "union");
    // A pointer into the definition's properties lands on the property.
    assert!(
        schema
            .edges
            .keys()
            .any(|e| e.kind == "ref" && e.tgt == options),
        "a pointer ref reaches compilerOptions"
    );
}

// Protobuf.

#[test]
fn an_option_value_spanning_lines_is_one_statement() {
    let schema = parse_schema_source(
        "protobuf",
        r#"
message M {
  // a comment with a } brace
  optional int32 a = 1
      [default = 3, deprecated = true];
  extensions 100 to 200 [declaration = {
    number: 100
    full_name: ".x.y"
  }];
  /* reserved 5; */ optional string b = 2;
  enum E { option allow_alias = true; X = 0 [deprecated = true]; Y = 1; }
  optional E e = 3;
  map<string, M> children = 4;
}
"#,
    )
    .expect("the message parses");
    for v in ["M.a", "M.b", "M.e", "M.E.X", "M.E.Y", "M.children"] {
        assert!(schema.has_vertex(v), "{v} is declared");
    }
    assert_eq!(constraint(&schema, "M.a", "field_number"), Some("1"));
    assert!(
        has_edge(&schema, "M.e", "M.E", "type-of"),
        "nested enum resolves"
    );
    assert!(has_edge(&schema, "M.children:value", "M", "type-of"));
    assert_eq!(schema.vertices.len(), 10);
}

#[test]
fn the_protobuf_descriptor_parses() {
    let schema = source("protobuf", "protobuf/descriptor.proto");
    // The extension declaration's `{ ... }` does not close the message.
    assert!(schema.has_vertex("FileDescriptorSet.file"));
    assert!(has_edge(
        &schema,
        "FileDescriptorSet.file",
        "FileDescriptorProto",
        "type-of"
    ));
    // A field typed by an enum nested in its message.
    assert!(has_edge(
        &schema,
        "FileOptions.optimize_for",
        "FileOptions.OptimizeMode",
        "type-of"
    ));
    assert_eq!(
        constraint(&schema, "FileOptions.optimize_for", "field_number"),
        Some("9")
    );
    assert!(has_edge(
        &schema,
        "FieldDescriptorProto.type",
        "FieldDescriptorProto.Type",
        "type-of"
    ));
    assert!(
        !schema.vertices.keys().any(|v| v.ends_with(".=")),
        "no option text is read as a field"
    );
}

// SQL.

#[test]
fn a_statement_after_a_comment_is_read() {
    let schema = parse_schema_source(
        "sql",
        "-- the users table; it has one column\nCREATE TABLE users (\n  id INTEGER, -- the key\n  check_in DATE\n);",
    )
    .expect("the table parses");
    assert!(schema.has_vertex("users.id"));
    assert_eq!(kind(&schema, "users.check_in"), "date");
}

#[test]
fn a_postgres_dump_declares_its_tables() {
    let schema = parse_schema_source(
        "sql",
        r"
CREATE DOMAIN year AS integer CONSTRAINT year_check CHECK (VALUE >= 1901);
CREATE FUNCTION f() RETURNS trigger AS $$
BEGIN NEW.x := 1; RETURN NEW; END $$ LANGUAGE plpgsql;
CREATE TABLE parent (id integer NOT NULL, born year);
CREATE TABLE child (CONSTRAINT c CHECK ((id > 0))) INHERITS (parent);
ALTER TABLE public.parent OWNER TO postgres;
ALTER TABLE ONLY parent ADD CONSTRAINT parent_pkey PRIMARY KEY (id);
ALTER TABLE ONLY child ADD CONSTRAINT child_fk FOREIGN KEY (id) REFERENCES parent(id);
COPY parent (id, born) FROM stdin;
1	1999
\.
",
    )
    .expect("the dump parses");
    assert_eq!(
        kind(&schema, "parent.born"),
        "integer",
        "through the domain"
    );
    assert_eq!(constraint(&schema, "parent", "PRIMARY KEY"), Some("id"));
    assert!(schema.has_vertex("child.id"), "inherited");
    assert_eq!(constraint(&schema, "child.id", "NOT NULL"), Some("true"));
    assert_eq!(
        constraint(&schema, "child.id", "FOREIGN KEY"),
        Some("parent.id")
    );
    assert_eq!(constraint(&schema, "child", "CHECK"), Some("(id > 0)"));
    assert!(!schema.has_vertex("parent.CONSTRAINT"));
}

#[test]
fn the_postgres_sakila_schema_parses() {
    let schema = source("sql", "sql/sakila-postgres.sql");
    let tables = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .count();
    // 15 tables and the six `payment` partitions.
    assert_eq!(tables, 21);
    assert_eq!(
        constraint(&schema, "actor", "PRIMARY KEY"),
        Some("actor_id")
    );
    assert_eq!(
        constraint(&schema, "rental.inventory_id", "FOREIGN KEY"),
        Some("inventory.inventory_id")
    );
    assert_eq!(kind(&schema, "film.release_year"), "integer");
    assert_eq!(kind(&schema, "payment_p2007_01.amount"), "number");
}

#[test]
fn the_sqlite_sakila_schema_parses() {
    let schema = source("sql", "sql/sakila-sqlite.sql");
    let tables = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .count();
    assert_eq!(tables, 16);
    assert!(schema.has_vertex("film.title"));
    assert_eq!(
        constraint(&schema, "film.language_id", "FOREIGN KEY"),
        Some("language.language_id")
    );
}
