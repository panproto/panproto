//! Schemas of the registered table protocols classify into tables and
//! attributes through their protocols' value-kind and non-value-kind
//! declarations alone.
//!
//! Each schema here comes from the protocol's own registered parser, and its
//! attributed relation-ified form is built under the protocol the registry
//! constructs, so a parser that emits a kind its protocol does not classify,
//! or an attribute that reaches a record, fails here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_inst::{RelSchema, RelSchemaError};
use panproto_protocols::registry::descriptor;
use panproto_protocols::{parse_schema_document, parse_schema_source};
use panproto_schema::{Edge, Schema};

/// The relation-ified form of `schema` under the registered `protocol`.
fn classify(protocol: &str, schema: &Schema) -> Result<RelSchema, RelSchemaError> {
    let built = (descriptor(protocol)
        .expect("a registered protocol")
        .protocol)();
    RelSchema::new(schema, &built)
}

fn tables(rel: &RelSchema) -> Vec<&str> {
    rel.tables().iter().map(AsRef::as_ref).collect()
}

fn columns<'a>(rel: &'a RelSchema, table: &str) -> Vec<&'a str> {
    rel.columns(table).keys().map(String::as_str).collect()
}

/// A JSON array root whose items are objects: both are tables, the `items`
/// edge between them a relation, and the item's scalar property a column.
#[test]
fn a_json_array_root_of_objects_is_two_tables() {
    let doc = serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": { "id": { "type": "integer" } }
        }
    });
    let schema = parse_schema_document("json-schema", &doc).unwrap();
    let rel = classify("json-schema", &schema).unwrap();
    let kinds = |kind: &str| -> Vec<&str> {
        schema
            .vertices
            .values()
            .filter(|v| v.kind == kind)
            .map(|v| v.id.as_ref())
            .collect()
    };
    let (arrays, objects) = (kinds("array"), kinds("object"));
    assert_eq!((arrays.len(), objects.len()), (1, 1), "{schema:?}");
    assert_eq!(tables(&rel).len(), 2, "{rel:?}");
    assert!(rel.is_table(arrays[0]) && rel.is_table(objects[0]));
    assert!(
        rel.relations()
            .iter()
            .any(|e| e.src == arrays[0] && e.tgt == objects[0]),
        "{rel:?}"
    );
    assert_eq!(columns(&rel, objects[0]), ["id"]);
}

const ASN1_MODULE: &str = r"
Directory DEFINITIONS ::=
BEGIN
Color ::= ENUMERATED
Address ::= SEQUENCE {
  city UTF8String,
  zip INTEGER
}
Person ::= SEQUENCE {
  name UTF8String,
  age INTEGER OPTIONAL,
  photo OCTET STRING,
  shade Color,
  home Address
}
Tags ::= SET {
  flag BOOLEAN
}
Shape ::= CHOICE {
  radius INTEGER,
  label UTF8String
}
END
";

/// SEQUENCE and SET types are tables whose builtin components are columns;
/// a component of a named type is a `field` table joined to it by a
/// relation, with a relation on to the named type.
#[test]
fn asn1_sequence_and_set_records_are_tables() {
    let schema = parse_schema_source("asn1", ASN1_MODULE).unwrap();
    let rel = classify("asn1", &schema).unwrap();
    for table in ["Address", "Person", "Tags", "Shape", "Person.home"] {
        assert!(rel.is_table(table), "{table}: {rel:?}");
    }
    assert_eq!(columns(&rel, "Address"), ["city", "zip"]);
    assert_eq!(columns(&rel, "Person"), ["age", "name", "photo"]);
    assert_eq!(columns(&rel, "Tags"), ["flag"]);
    let field_of = Edge {
        src: "Person".into(),
        tgt: "Person.home".into(),
        kind: "field-of".into(),
        name: Some("home".into()),
    };
    assert!(rel.is_relation(&field_of));
    assert!(
        rel.relations()
            .iter()
            .any(|e| e.src == "Person.home" && e.tgt == "Address" && e.kind == "type-of")
    );
}

/// An ENUMERATED type and an OCTET STRING component hold values: the type
/// assignment `Color` is a value vertex (a column of the `field` table of a
/// component typed by it), and `photo` a column of `Person`.
#[test]
fn asn1_enumerated_and_octet_string_are_attributes() {
    let schema = parse_schema_source("asn1", ASN1_MODULE).unwrap();
    let rel = classify("asn1", &schema).unwrap();
    assert!(!rel.is_table("Color"));
    assert!(!rel.is_table("Person.photo"));
    assert_eq!(schema.vertices["Person.photo"].kind, "octet-string");
    assert_eq!(columns(&rel, "Person.shade"), ["Color"]);
}

/// A CHOICE holds one alternative, so its alternatives are tables (the
/// relational encoding of a sum), never columns of the choice.
#[test]
fn asn1_choice_alternatives_are_tables() {
    let schema = parse_schema_source("asn1", ASN1_MODULE).unwrap();
    let rel = classify("asn1", &schema).unwrap();
    assert!(rel.is_table("Shape.radius") && rel.is_table("Shape.label"));
    assert!(columns(&rel, "Shape").is_empty());
}

/// The tags of a CoNLL-U word are its columns, named by column role, so a
/// root word whose DEPREL is `root` and whose DEPS is `0:root` has two
/// distinct columns and no duplicate.
#[test]
fn conllu_tags_are_attributes_of_the_word_table() {
    let text = "1\tThe\tthe\tDET\tDT\tDefinite=Def\t2\tdet\t_\t_\n\
                2\tcat\tcat\tNOUN\tNN\tNumber=Sing\t0\troot\t0:root\t_\n";
    let schema = parse_schema_source("conllu", text).unwrap();
    let rel = classify("conllu", &schema).unwrap();
    assert_eq!(tables(&rel), ["sent_0", "sent_0.tok_1", "sent_0.tok_2"]);
    assert_eq!(
        columns(&rel, "sent_0.tok_2"),
        ["deprel", "deps:0", "feat:Number", "lemma", "upos", "xpos"]
    );
    assert!(
        rel.relations()
            .iter()
            .any(|e| e.src == "sent_0.tok_2" && e.tgt == "sent_0.tok_1" && e.kind == "dep")
    );
}

/// An SQL array column is one attribute arrow, its element vertex part of
/// the cell's value.
#[test]
fn an_sql_array_column_is_an_attribute() {
    let ddl = "CREATE TABLE post (id INTEGER PRIMARY KEY, tags TEXT[]);";
    let schema = parse_schema_source("sql", ddl).unwrap();
    let rel = classify("sql", &schema).unwrap();
    assert_eq!(tables(&rel), ["post"]);
    assert_eq!(columns(&rel, "post"), ["id", "tags"]);
    assert!(!rel.inner_edges().is_empty(), "{schema:?}");
}

/// One schema document of each document-read protocol that gained its
/// value-kind table here, with a table it must have.
fn document_samples() -> Vec<(&'static str, serde_json::Value, &'static str)> {
    vec![
        (
            "arrow",
            serde_json::json!({
                "fields": [
                    { "name": "id", "type": { "name": "int64" }, "nullable": false },
                    {
                        "name": "address",
                        "type": { "name": "struct" },
                        "children": [{ "name": "zip", "type": { "name": "int32" } }]
                    }
                ]
            }),
            "schema",
        ),
        (
            "parquet",
            serde_json::json!({
                "name": "event",
                "fields": [
                    { "name": "id", "type": "INT64", "repetition": "required" },
                    {
                        "name": "address",
                        "type": "GROUP",
                        "fields": [{ "name": "zip", "type": "INT32" }]
                    }
                ]
            }),
            "event",
        ),
        (
            "dataframe",
            serde_json::json!({
                "name": "users",
                "columns": {
                    "name": { "dtype": "string" },
                    "age": { "dtype": "int64" }
                }
            }),
            "users",
        ),
        (
            "dynamodb",
            serde_json::json!({
                "TableName": "users",
                "AttributeDefinitions": [
                    { "AttributeName": "user_id", "AttributeType": "S" },
                    { "AttributeName": "rank", "AttributeType": "N" }
                ],
                "KeySchema": [{ "AttributeName": "user_id", "KeyType": "HASH" }],
                "GlobalSecondaryIndexes": [{
                    "IndexName": "by_rank",
                    "KeySchema": [{ "AttributeName": "rank", "KeyType": "HASH" }]
                }]
            }),
            "users",
        ),
        (
            "edi-x12",
            serde_json::json!({
                "transactionSet": "850",
                "segments": [{
                    "id": "PO1",
                    "elements": [{
                        "ref": "PO101",
                        "type": "AN",
                        "components": [{ "ref": "C001-01", "type": "N" }]
                    }]
                }]
            }),
            "850",
        ),
        (
            "swift-mt",
            serde_json::json!({
                "messages": {
                    "MT103": {
                        "blocks": {
                            "block4": {
                                "fields": {
                                    "20": { "type": "string", "tag": ":20:" },
                                    "32A": { "type": "amount", "tag": ":32A:" }
                                }
                            }
                        }
                    }
                }
            }),
            "MT103",
        ),
    ]
}

/// One schema of each protocol that gained its value-kind table here builds
/// a relation-ified form, with its records as tables and its typed fields
/// as columns.
#[test]
fn each_table_protocol_sample_builds_a_rel_schema() {
    let documents = document_samples();
    for (protocol, doc, table) in documents {
        let schema = parse_schema_document(protocol, &doc).unwrap();
        let rel =
            classify(protocol, &schema).unwrap_or_else(|e| panic!("{protocol}: {e}: {schema:#?}"));
        assert!(rel.is_table(table), "{protocol}: {rel:?}");
        let attributes: usize = rel
            .all_columns()
            .values()
            .map(std::collections::BTreeMap::len)
            .sum();
        assert!(attributes > 0, "{protocol} has columns: {rel:?}");
    }
    let sources = [
        (
            "cassandra",
            "CREATE TYPE address (city text, zip int);\n\
             CREATE TABLE users (user_id uuid, name text, home frozen<address>, \
             tags list<text>, PRIMARY KEY (user_id));",
            "users",
        ),
        (
            "redis",
            "FT.CREATE idx ON HASH SCHEMA title TEXT name TAG age NUMERIC\n",
            "idx",
        ),
    ];
    for (protocol, source, table) in sources {
        let schema = parse_schema_source(protocol, source).unwrap();
        let rel =
            classify(protocol, &schema).unwrap_or_else(|e| panic!("{protocol}: {e}: {schema:#?}"));
        assert!(rel.is_table(table), "{protocol}: {rel:?}");
        assert!(!rel.columns(table).is_empty(), "{protocol}: {rel:?}");
    }
}
