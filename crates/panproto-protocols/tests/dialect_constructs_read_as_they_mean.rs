//! SQL and JSON Schema constructs whose meaning depends on the dialect
//! read as that dialect defines them, and survive an emit and re-parse.
//!
//! Each case is one construct: a bracket-quoted identifier (SQLite,
//! T-SQL), a clause after a column type that holds a `[`, a `DROP TABLE`
//! that names its table in another case or qualification, enum labels
//! holding a comma or an escaped quote, a `$id` ending in an empty
//! fragment, the draft-04 `id` keyword, 2020-12 `prefixItems` beside
//! `items`, and boolean `items` and `additionalItems`.

use panproto_protocols::data_schema::json_schema::{emit_json_schema, parse_json_schema};
use panproto_protocols::database::sql::{emit_ddl, parse_ddl};
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

fn kind<'a>(schema: &'a Schema, vertex: &str) -> &'a str {
    schema
        .vertices
        .get(vertex)
        .unwrap_or_else(|| panic!("no vertex {vertex}"))
        .kind
        .as_ref()
}

fn constraint(schema: &Schema, vertex: &str, sort: &str) -> Option<String> {
    schema
        .constraints
        .get(vertex)?
        .iter()
        .find(|c| c.sort == sort)
        .map(|c| c.value.clone())
}

/// The target of the one `ref` edge out of `src`.
fn ref_target<'a>(schema: &'a Schema, src: &str) -> &'a str {
    let refs: Vec<_> = schema
        .outgoing_edges(src)
        .iter()
        .filter(|e| e.kind == "ref")
        .collect();
    assert_eq!(refs.len(), 1, "one ref edge out of {src}: {refs:?}");
    refs[0].tgt.as_ref()
}

#[test]
fn a_bracket_quoted_identifier_is_not_an_array() {
    let schema = parse_ddl("CREATE TABLE [t] ([id] [int] NOT NULL, [name] [nvarchar](50));")
        .required("bracket-quoted DDL parses");
    assert!(
        schema.has_vertex("t"),
        "the table is named without brackets"
    );
    assert_eq!(kind(&schema, "t.id"), "integer");
    assert_eq!(kind(&schema, "t.name"), "string");
    assert!(!schema.has_vertex("t.id:items"));
    assert!(!schema.has_vertex("t.name:items"));
}

#[test]
fn a_bracket_after_the_column_type_is_not_an_array() {
    let schema = parse_ddl(
        "CREATE TABLE t (\
            a VARCHAR(10) COMMENT '[x]', \
            b TEXT CHARACTER SET utf8mb4 COMMENT 'list [1]', \
            c INTEGER [3], \
            d INTEGER ARRAY, \
            e TIMESTAMP WITH TIME ZONE[]\
        );",
    )
    .required("DDL with comments parses");
    assert_eq!(kind(&schema, "t.a"), "string");
    assert_eq!(kind(&schema, "t.b"), "string");
    assert!(!schema.has_vertex("t.a:items"));
    assert!(!schema.has_vertex("t.b:items"));
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

    let schema =
        parse_ddl("CREATE TABLE \"Foo\" (id INT); DROP TABLE foo; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(schema.has_vertex("Foo"), "a quoted name keeps its case");
}

#[test]
fn a_drop_removes_its_create_whichever_side_is_qualified() {
    let schema =
        parse_ddl("CREATE TABLE public.t (id INT); DROP TABLE t; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(!schema.has_vertex("public.t"), "t names public.t");

    let schema =
        parse_ddl("CREATE TABLE t (id INT); DROP TABLE public.t; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(!schema.has_vertex("t"), "public.t names t");

    let schema =
        parse_ddl("CREATE TABLE a.t (id INT); DROP TABLE b.t; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(schema.has_vertex("a.t"), "b.t is another table");
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
fn a_ref_written_against_an_id_with_an_empty_fragment_resolves() {
    let schema = parse_json_schema(&json!({
        "$id": "http://x.test/s#",
        "definitions": { "a": { "type": "string" } },
        "type": "object",
        "properties": {
            "p": { "$ref": "http://x.test/s#/definitions/a" },
            "q": { "$ref": "http://x.test/s#" }
        }
    }))
    .required("schema parses");
    let root = schema
        .entries
        .iter()
        .find(|e| !e.contains(":definitions/"))
        .map(ToString::to_string)
        .required_some("the document has a root entry");
    let p = ref_target(&schema, &format!("{root}.p"));
    assert_eq!(kind(&schema, p), "string", "the ref reaches the definition");
    assert_eq!(
        ref_target(&schema, &format!("{root}.q")),
        root,
        "the ref reaches the root"
    );
}

#[test]
fn the_draft_04_id_keyword_is_the_document_identity() {
    let schema = parse_json_schema(&json!({
        "id": "http://x.test/d4",
        "definitions": { "a": { "type": "integer" } },
        "type": "object",
        "properties": { "p": { "$ref": "http://x.test/d4#/definitions/a" } }
    }))
    .required("schema parses");
    let p = ref_target(&schema, "http://x.test/d4.p");
    assert_eq!(
        kind(&schema, p),
        "integer",
        "the ref reaches the definition"
    );
}

#[test]
fn prefix_items_and_items_survive_emit() {
    let doc = json!({
        "type": "array",
        "prefixItems": [{ "type": "string" }, { "type": "boolean" }],
        "items": { "type": "integer" }
    });
    let emitted = emit_json_schema(&parse_json_schema(&doc).required("parse")).required("emit");
    assert_eq!(emitted["prefixItems"][0]["type"], "string", "{emitted}");
    assert_eq!(emitted["prefixItems"][1]["type"], "boolean", "{emitted}");
    assert_eq!(emitted["items"]["type"], "integer", "{emitted}");
}

#[test]
fn a_tuple_items_and_prefix_items_stay_apart() {
    let doc = json!({
        "type": "array",
        "items": [{ "type": "string" }],
        "prefixItems": [{ "type": "boolean" }]
    });
    let emitted = emit_json_schema(&parse_json_schema(&doc).required("parse")).required("emit");
    assert_eq!(emitted["items"][0]["type"], "string", "{emitted}");
    assert_eq!(emitted["prefixItems"][0]["type"], "boolean", "{emitted}");
}

#[test]
fn boolean_items_keep_their_meaning() {
    let none = parse_json_schema(&json!({ "type": "array", "items": false })).required("parse");
    assert!(
        !none.has_vertex("root:items"),
        "false admits no item schema"
    );
    let emitted = emit_json_schema(&none).required("emit");
    assert_eq!(emitted["items"], json!(false), "{emitted}");

    let any = parse_json_schema(&json!({ "type": "array", "items": true })).required("parse");
    assert!(!any.has_vertex("root:items"), "true is the default");
    let emitted = emit_json_schema(&any).required("emit");
    assert!(emitted.get("items").is_none(), "{emitted}");

    let closed = parse_json_schema(&json!({
        "type": "array",
        "items": [{ "type": "string" }],
        "additionalItems": false
    }))
    .required("parse");
    let emitted = emit_json_schema(&closed).required("emit");
    assert_eq!(emitted["items"][0]["type"], "string", "{emitted}");
    assert_eq!(emitted["additionalItems"], json!(false), "{emitted}");
}

#[test]
fn a_document_with_an_identity_emits_from_its_root() {
    for doc in [
        json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "id": "http://x.test/d4",
            "type": "object",
            "properties": { "p": { "type": "string" } }
        }),
        json!({
            "$id": "http://x.test/d7",
            "type": "object",
            "properties": { "p": { "type": "string" } }
        }),
    ] {
        let schema = parse_json_schema(&doc).required("schema parses");
        let emitted = emit_json_schema(&schema).required("a document with an identity emits");
        assert_eq!(emitted["type"], "object", "{emitted}");
        assert_eq!(emitted["properties"]["p"]["type"], "string", "{emitted}");
    }
}

#[test]
fn every_json_schema_fixture_emits() {
    for name in ["tsconfig.json", "package.json", "github-workflow.json"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/jsonschema")
            .join(name);
        let text = std::fs::read_to_string(path).required("the fixture exists");
        let doc: serde_json::Value = serde_json::from_str(&text).required("the fixture is JSON");
        let schema = parse_json_schema(&doc).required(name);
        let emitted = emit_json_schema(&schema).required(name);
        assert!(emitted.is_object(), "{name} emits a schema object");
    }
}

#[test]
fn an_id_outside_draft_04_is_not_the_document_identity() {
    let schema = parse_json_schema(&json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "id": "whatever",
        "type": "object",
        "properties": { "p": { "type": "string" } }
    }))
    .required("schema parses");
    assert!(schema.has_vertex("root"), "a 2020-12 `id` is no keyword");
    assert!(schema.has_vertex("root.p"));
}

#[test]
fn an_identity_that_is_only_a_fragment_leaves_the_document_unnamed() {
    for doc in [
        json!({ "$id": "#", "type": "object", "properties": { "p": { "type": "string" } } }),
        json!({ "id": "#", "type": "object", "properties": { "p": { "type": "string" } } }),
        json!({ "$id": "", "type": "object", "properties": { "p": { "type": "string" } } }),
    ] {
        let schema = parse_json_schema(&doc).required("schema parses");
        assert!(schema.has_vertex("root"), "{doc}: the root is `root`");
        assert!(
            schema.has_vertex("root.p"),
            "{doc}: its property is `root.p`"
        );
        let emitted = emit_json_schema(&schema).required("the document emits");
        assert_eq!(emitted["properties"]["p"]["type"], "string", "{emitted}");
    }
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
               g TEXT CHECK (g IN ('[a]')), [h]]x] INT, s TEXT DEFAULT 'a b');";
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
fn a_drop_removes_a_bracket_quoted_create_in_any_case() {
    let schema =
        parse_ddl("CREATE TABLE [Foo] (id INT); DROP TABLE foo; CREATE TABLE keep (id INT);")
            .required("DDL parses");
    assert!(
        !schema.has_vertex("Foo"),
        "brackets compare without regard to case"
    );
    assert!(schema.has_vertex("keep"));
}

/// An option a test requires to be `Some`.
trait RequiredSome<T> {
    /// The value, or a panic naming `what`.
    fn required_some(self, what: &str) -> T;
}

impl<T> RequiredSome<T> for Option<T> {
    fn required_some(self, what: &str) -> T {
        self.unwrap_or_else(|| panic!("{what}"))
    }
}
