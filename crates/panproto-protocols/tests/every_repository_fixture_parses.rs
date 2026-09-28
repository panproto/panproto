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
//! bodies, `ALTER TABLE ONLY ... ADD CONSTRAINT`, `INHERITS`, domains,
//! enum types, array columns and foreign keys to columns of other names.
//! Each construct also has a minimal case of its own below, and the parse
//! is checked against what the file declares, not only for success: every
//! Sakila foreign key names a column that exists.

use std::path::{Path, PathBuf};

use panproto_protocols::{parse_schema_document, parse_schema_source};
use panproto_schema::Schema;
use serde_json::json;

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

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
}

fn document(protocol: &str, relative: &str) -> Schema {
    let path = fixture(relative);
    let text = std::fs::read_to_string(&path).required("the fixture exists");
    let doc: serde_json::Value = serde_json::from_str(&text).required("the fixture is JSON");
    parse_schema_document(protocol, &doc).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

fn source(protocol: &str, relative: &str) -> Schema {
    let path = fixture(relative);
    let text = std::fs::read_to_string(&path).required("the fixture exists");
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
    .required("a forward ref parses");
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
    .required("a type list with properties parses");
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
    .required("pointer refs parse");
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

#[test]
fn an_untyped_schema_keeps_its_array_keywords() {
    let schema = parse_schema_document(
        "json-schema",
        &json!({
            "properties": {
                "x": { "items": { "type": "string" }, "minItems": 1 },
                "t": {
                    "items": [{ "type": "string" }, { "type": "integer" }],
                    "additionalItems": { "type": "boolean" }
                },
                "both": { "properties": { "p": {} }, "items": {} }
            }
        }),
    )
    .required("untyped schemas parse");
    assert_eq!(kind(&schema, "root.x"), "array");
    assert!(has_edge(&schema, "root.x", "root.x:items", "items"));
    assert_eq!(kind(&schema, "root.x:items"), "string");

    // A tuple `items` gives each position its own item, and
    // `additionalItems` the item past them.
    assert_eq!(kind(&schema, "root.t"), "array");
    assert_eq!(kind(&schema, "root.t:items0"), "string");
    assert_eq!(kind(&schema, "root.t:items1"), "integer");
    assert_eq!(kind(&schema, "root.t:additionalItems"), "boolean");
    assert!(has_edge(&schema, "root.t", "root.t:items1", "items"));
    assert!(has_edge(
        &schema,
        "root.t",
        "root.t:additionalItems",
        "items"
    ));

    // Object and array keywords together: each goes to its variant.
    assert_eq!(kind(&schema, "root.both"), "union");
    assert!(has_edge(&schema, "root.both:object", "root.both.p", "prop"));
    assert!(has_edge(
        &schema,
        "root.both:array",
        "root.both:items",
        "items"
    ));
}

#[test]
fn a_ref_spelled_against_the_documents_id_or_percent_encoded_resolves() {
    let schema = parse_schema_document(
        "json-schema",
        &json!({
            "$id": "https://x.test/s.json",
            "definitions": {
                "a": { "properties": { "b": { "type": "integer" } } },
                "c d": { "type": "string" }
            },
            "properties": {
                "c": { "$ref": "https://x.test/s.json#/definitions/a/properties/b" },
                "q": { "$ref": "#/definitions/c%20d" },
                "r": { "$ref": "https://x.test/s.json#/definitions/c%20d" },
                "s": { "$ref": "https://x.test/s.json" }
            }
        }),
    )
    .required("in-document refs parse");
    let id = "https://x.test/s.json";
    assert!(has_edge(
        &schema,
        &format!("{id}.c"),
        &format!("{id}:definitions/a.b"),
        "ref"
    ));
    assert!(has_edge(
        &schema,
        &format!("{id}.q"),
        &format!("{id}:definitions/c d"),
        "ref"
    ));
    assert!(has_edge(
        &schema,
        &format!("{id}.r"),
        &format!("{id}:definitions/c d"),
        "ref"
    ));
    assert!(has_edge(&schema, &format!("{id}.s"), id, "ref"));
    assert!(
        !schema.vertices.keys().any(|v| v.contains(":ref")),
        "no ref falls back to a placeholder"
    );
}

#[test]
fn the_github_workflow_runner_list_is_an_array() {
    let schema = document("json-schema", "jsonschema/github-workflow.json");
    let id = "https://json.schemastore.org/github-workflow.json";
    // `runs-on` admits a tuple of labels: `{"items": [{"type": "string"}],
    // "additionalItems": {"type": "string"}}` under an `anyOf`.
    let runners = format!("{id}:definitions/normalJob.runs-on:anyOf1:anyOf0");
    assert_eq!(kind(&schema, &runners), "array");
    assert_eq!(kind(&schema, &format!("{runners}:items0")), "string");
    assert_eq!(
        kind(&schema, &format!("{runners}:additionalItems")),
        "string"
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
    .required("the message parses");
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

#[test]
fn an_rpc_refers_to_its_request_and_response_apart() {
    let schema = parse_schema_source(
        "protobuf",
        "message M { string a = 1; }\nmessage N { string b = 1; }\n\
         service S {\n  rpc Echo(M) returns (M);\n  rpc Swap(stream M) returns (N);\n}",
    )
    .required("an rpc with one message both ways parses");
    assert!(has_edge(&schema, "S.Echo", "M", "input-of"));
    assert!(has_edge(&schema, "S.Echo", "M", "output-of"));
    assert!(has_edge(&schema, "S.Swap", "M", "input-of"));
    assert!(has_edge(&schema, "S.Swap", "N", "output-of"));
    assert!(!has_edge(&schema, "S.Swap", "N", "input-of"));
    assert!(
        !schema
            .edges
            .keys()
            .any(|e| e.src == "S.Echo" && e.kind == "type-of"),
        "an rpc's types are not field types"
    );
}

// SQL.

#[test]
fn a_statement_after_a_comment_is_read() {
    let schema = parse_schema_source(
        "sql",
        "-- the users table; it has one column\nCREATE TABLE users (\n  id INTEGER, -- the key\n  check_in DATE\n);",
    )
    .required("the table parses");
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
    .required("the dump parses");
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

/// Every `FOREIGN KEY` constraint of `schema`, as `(vertex, value)`.
fn foreign_keys(schema: &Schema) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = schema
        .constraints
        .iter()
        .flat_map(|(v, cs)| {
            cs.iter()
                .filter(|c| c.sort == "FOREIGN KEY")
                .map(move |c| (v.to_string(), c.value.clone()))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn a_foreign_key_names_the_column_it_references() {
    let schema = parse_schema_source(
        "sql",
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
    assert_eq!(constraint(&schema, "b.x", "FOREIGN KEY"), Some("a.id"));
    // With no column list, the reference is to the primary key.
    assert_eq!(constraint(&schema, "b.w", "FOREIGN KEY"), Some("a.id"));
    assert_eq!(constraint(&schema, "b.y", "FOREIGN KEY"), Some("a.id"));
    assert_eq!(constraint(&schema, "b.z", "FOREIGN KEY"), Some("a.id"));
    assert_eq!(constraint(&schema, "b.c", "FOREIGN KEY"), Some("a.code"));
    assert_eq!(constraint(&schema, "b.m", "FOREIGN KEY"), Some("a.n"));
}

#[test]
fn every_sakila_foreign_key_names_a_column() {
    for fixture in ["sql/sakila-postgres.sql", "sql/sakila-sqlite.sql"] {
        let schema = source("sql", fixture);
        let fks = foreign_keys(&schema);
        assert!(!fks.is_empty(), "{fixture} declares foreign keys");
        for (column, target) in &fks {
            assert!(
                schema.has_vertex(target),
                "{fixture}: {column} references {target}, which is no column"
            );
        }
        assert_eq!(
            constraint(&schema, "film.original_language_id", "FOREIGN KEY"),
            Some("language.language_id"),
            "{fixture}"
        );
        assert_eq!(
            constraint(&schema, "store.manager_staff_id", "FOREIGN KEY"),
            Some("staff.staff_id"),
            "{fixture}"
        );
    }
}

#[test]
fn a_table_dropped_before_it_is_created_is_declared() {
    let schema = parse_schema_source(
        "sql",
        "DROP TABLE IF EXISTS t, u;\nCREATE TABLE t (id INTEGER);\n\
         CREATE TABLE u (id INTEGER);\nDROP TABLE u;\n\
         ALTER TABLE u ADD COLUMN gone INTEGER;",
    )
    .required("the script parses");
    assert!(schema.has_vertex("t.id"));
    assert!(!schema.has_vertex("u"), "dropped after it was created");
    assert!(!schema.has_vertex("u.gone"));
}

#[test]
fn a_quoted_table_name_is_read_whole() {
    let schema = parse_schema_source(
        "sql",
        r#"CREATE TABLE "u;v" (id INTEGER); CREATE TABLE "a (b" ("x y" INTEGER, "id" INTEGER, FOREIGN KEY ("x y") REFERENCES "u;v" ("id"));"#,
    )
    .required("quoted names parse");
    assert!(schema.has_vertex("u;v"));
    assert!(schema.has_vertex("u;v.id"));
    assert!(schema.has_vertex("a (b.x y"));
    assert_eq!(
        constraint(&schema, "a (b.x y", "FOREIGN KEY"),
        Some("u;v.id")
    );
}

#[test]
fn postgres_arrays_and_enums_keep_their_shape() {
    let schema = parse_schema_source(
        "sql",
        r"
CREATE TYPE public.mood AS ENUM ('sad', 'ok', 'it''s fine');
CREATE DOMAIN tags AS text[];
CREATE TABLE t (
  m mood NOT NULL,
  labels text[],
  grid integer[][],
  scores numeric ARRAY,
  more tags
);
",
    )
    .required("arrays and enums parse");
    assert_eq!(kind(&schema, "t.m"), "string");
    assert_eq!(
        constraint(&schema, "t.m", "ENUM"),
        Some("'sad','ok','it''s fine'")
    );
    assert_eq!(constraint(&schema, "t.m", "NOT NULL"), Some("true"));
    assert_eq!(kind(&schema, "t.labels"), "array");
    assert!(has_edge(&schema, "t.labels", "t.labels:items", "items"));
    assert_eq!(kind(&schema, "t.labels:items"), "string");
    assert_eq!(kind(&schema, "t.grid"), "array");
    assert_eq!(kind(&schema, "t.grid:items"), "array");
    assert_eq!(kind(&schema, "t.grid:items:items"), "integer");
    assert_eq!(kind(&schema, "t.scores"), "array");
    assert_eq!(kind(&schema, "t.scores:items"), "number");
    assert_eq!(kind(&schema, "t.more"), "array");
    assert_eq!(kind(&schema, "t.more:items"), "string");
}

#[test]
fn the_sakila_film_keeps_its_rating_enum_and_feature_array() {
    let schema = source("sql", "sql/sakila-postgres.sql");
    assert_eq!(kind(&schema, "film.special_features"), "array");
    assert_eq!(kind(&schema, "film.special_features:items"), "string");
    assert_eq!(kind(&schema, "film.rating"), "string");
    assert_eq!(
        constraint(&schema, "film.rating", "ENUM"),
        Some("'G','PG','PG-13','R','NC-17'")
    );
}
