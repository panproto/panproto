//! DynamoDB protocol definition.
//!
//! DynamoDB uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, find_roots};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `DynamoDB` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "dynamodb".into(),
        schema_theory: "ThDynamoDBSchema".into(),
        instance_theory: "ThDynamoDBInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "table".into(),
            "attribute".into(),
            "string".into(),
            "number".into(),
            "binary".into(),
            "gsi".into(),
            "lsi".into(),
        ],
        constraint_sorts: vec![
            "key-type".into(),
            "projection-type".into(),
            "read-capacity".into(),
            "write-capacity".into(),
        ],
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `DynamoDB` with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThDynamoDBSchema", "ThDynamoDBInstance");
}

/// Parse a `DynamoDB` `CreateTable` JSON into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is invalid.
pub fn parse_dynamodb(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let table_name = json
        .get("TableName")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProtocolError::MissingField("TableName".into()))?;

    builder = builder.vertex(table_name, "table", None)?;

    let (b, sig) = parse_attributes(builder, json, table_name)?;
    builder = b;
    builder = parse_key_schema(builder, json, &sig);
    builder = parse_throughput(builder, json, table_name);
    builder = parse_gsis(builder, json, table_name)?;
    builder = parse_lsis(builder, json, table_name)?;

    if !sig.is_empty() {
        builder = builder.hyper_edge("he_0", "table", sig, table_name)?;
    }

    let schema = builder.build()?;
    Ok(schema)
}

fn parse_attributes(
    mut builder: SchemaBuilder,
    json: &serde_json::Value,
    table_name: &str,
) -> Result<(SchemaBuilder, HashMap<String, String>), ProtocolError> {
    let mut attr_types: HashMap<String, String> = HashMap::new();
    if let Some(attrs) = json
        .get("AttributeDefinitions")
        .and_then(serde_json::Value::as_array)
    {
        for attr in attrs {
            let attr_name = attr
                .get("AttributeName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let attr_type = attr
                .get("AttributeType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("S");
            if !attr_name.is_empty() {
                attr_types.insert(attr_name.to_string(), attr_type.to_string());
            }
        }
    }

    let mut sig = HashMap::new();
    for (attr_name, attr_type) in &attr_types {
        let attr_id = format!("{table_name}.{attr_name}");
        let kind = dynamodb_type_to_kind(attr_type);
        builder = builder.vertex(&attr_id, &kind, None)?;
        builder = builder.edge(table_name, &attr_id, "prop", Some(attr_name))?;
        sig.insert(attr_name.clone(), attr_id);
    }

    Ok((builder, sig))
}

fn parse_key_schema(
    mut builder: SchemaBuilder,
    json: &serde_json::Value,
    sig: &HashMap<String, String>,
) -> SchemaBuilder {
    if let Some(keys) = json.get("KeySchema").and_then(serde_json::Value::as_array) {
        for key in keys {
            let key_name = key
                .get("AttributeName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let key_type = key
                .get("KeyType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("HASH");
            if let Some(attr_id) = sig.get(key_name) {
                builder = builder.constraint(attr_id, "key-type", key_type);
            }
        }
    }
    builder
}

fn parse_throughput(
    mut builder: SchemaBuilder,
    json: &serde_json::Value,
    table_name: &str,
) -> SchemaBuilder {
    if let Some(throughput) = json.get("ProvisionedThroughput") {
        if let Some(rcu) = throughput
            .get("ReadCapacityUnits")
            .and_then(serde_json::Value::as_u64)
        {
            builder = builder.constraint(table_name, "read-capacity", &rcu.to_string());
        }
        if let Some(wcu) = throughput
            .get("WriteCapacityUnits")
            .and_then(serde_json::Value::as_u64)
        {
            builder = builder.constraint(table_name, "write-capacity", &wcu.to_string());
        }
    }
    builder
}

fn parse_gsis(
    mut builder: SchemaBuilder,
    json: &serde_json::Value,
    table_name: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(gsis) = json
        .get("GlobalSecondaryIndexes")
        .and_then(serde_json::Value::as_array)
    {
        for gsi in gsis {
            let index_name = gsi
                .get("IndexName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unnamed_gsi");
            let gsi_id = format!("{table_name}:gsi:{index_name}");
            builder = builder.vertex(&gsi_id, "gsi", None)?;
            builder = builder.edge(table_name, &gsi_id, "prop", Some(index_name))?;

            if let Some(proj) = gsi.get("Projection") {
                if let Some(proj_type) = proj
                    .get("ProjectionType")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&gsi_id, "projection-type", proj_type);
                }
            }

            if let Some(keys) = gsi.get("KeySchema").and_then(serde_json::Value::as_array) {
                for key in keys {
                    let key_name = key
                        .get("AttributeName")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    let key_type = key
                        .get("KeyType")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("HASH");
                    builder =
                        builder.constraint(&gsi_id, "key-type", &format!("{key_name}:{key_type}"));
                }
            }
        }
    }
    Ok(builder)
}

fn parse_lsis(
    mut builder: SchemaBuilder,
    json: &serde_json::Value,
    table_name: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(lsis) = json
        .get("LocalSecondaryIndexes")
        .and_then(serde_json::Value::as_array)
    {
        for lsi in lsis {
            let index_name = lsi
                .get("IndexName")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unnamed_lsi");
            let lsi_id = format!("{table_name}:lsi:{index_name}");
            builder = builder.vertex(&lsi_id, "lsi", None)?;
            builder = builder.edge(table_name, &lsi_id, "prop", Some(index_name))?;

            if let Some(proj) = lsi.get("Projection") {
                if let Some(proj_type) = proj
                    .get("ProjectionType")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(&lsi_id, "projection-type", proj_type);
                }
            }
        }
    }
    Ok(builder)
}

/// Emit a [`Schema`] as `DynamoDB` `CreateTable` JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_dynamodb(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let tables: Vec<_> = find_roots(schema, &["prop"]);
    let table = tables
        .into_iter()
        .find(|v| v.kind == "table")
        .ok_or_else(|| ProtocolError::Emit("no table vertex found".into()))?;

    let children = children_by_edge(schema, &table.id, "prop");

    let mut attr_defs = Vec::new();
    let mut key_schema = Vec::new();
    let mut gsis = Vec::new();

    for (edge, vertex) in &children {
        let attr_name = edge.name.as_deref().unwrap_or(&vertex.id);
        match vertex.kind.as_str() {
            "gsi" => {
                let mut gsi_obj = serde_json::json!({
                    "IndexName": attr_name,
                    "KeySchema": [],
                    "Projection": { "ProjectionType": "ALL" }
                });
                if let Some(proj) = constraint_value(schema, &vertex.id, "projection-type") {
                    gsi_obj["Projection"]["ProjectionType"] =
                        serde_json::Value::String(proj.to_string());
                }
                gsis.push(gsi_obj);
            }
            "lsi" => {
                // LSIs emitted similarly but we keep it simple.
            }
            _ => {
                let ddb_type = kind_to_dynamodb_type(&vertex.kind);
                attr_defs.push(serde_json::json!({
                    "AttributeName": attr_name,
                    "AttributeType": ddb_type
                }));
                if let Some(kt) = constraint_value(schema, &vertex.id, "key-type") {
                    key_schema.push(serde_json::json!({
                        "AttributeName": attr_name,
                        "KeyType": kt
                    }));
                }
            }
        }
    }

    let mut result = serde_json::json!({
        "TableName": table.id,
        "AttributeDefinitions": attr_defs,
        "KeySchema": key_schema
    });

    if let Some(rcu) = constraint_value(schema, &table.id, "read-capacity") {
        if let Some(wcu) = constraint_value(schema, &table.id, "write-capacity") {
            result["ProvisionedThroughput"] = serde_json::json!({
                "ReadCapacityUnits": rcu.parse::<u64>().unwrap_or(5),
                "WriteCapacityUnits": wcu.parse::<u64>().unwrap_or(5)
            });
        }
    }

    if !gsis.is_empty() {
        result["GlobalSecondaryIndexes"] = serde_json::Value::Array(gsis);
    }

    Ok(result)
}

fn dynamodb_type_to_kind(ddb_type: &str) -> String {
    match ddb_type {
        "N" => "number",
        "B" => "binary",
        _ => "string",
    }
    .into()
}

fn kind_to_dynamodb_type(kind: &str) -> &'static str {
    match kind {
        "number" => "N",
        "binary" => "B",
        _ => "S",
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["table".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "foreign-key".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "dynamodb");
        assert_eq!(p.schema_theory, "ThDynamoDBSchema");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThDynamoDBSchema"));
        assert!(registry.contains_key("ThDynamoDBInstance"));
    }

    #[test]
    fn parse_simple_table() {
        let json = serde_json::json!({
            "TableName": "users",
            "AttributeDefinitions": [
                { "AttributeName": "user_id", "AttributeType": "S" },
                { "AttributeName": "sort_key", "AttributeType": "N" }
            ],
            "KeySchema": [
                { "AttributeName": "user_id", "KeyType": "HASH" },
                { "AttributeName": "sort_key", "KeyType": "RANGE" }
            ],
            "ProvisionedThroughput": {
                "ReadCapacityUnits": 10,
                "WriteCapacityUnits": 5
            }
        });
        let schema = parse_dynamodb(&json).expect("should parse");
        assert!(schema.has_vertex("users"));
        assert!(schema.has_vertex("users.user_id"));
        assert_eq!(schema.vertices.get("users.user_id").unwrap().kind, "string");
        assert_eq!(
            schema.vertices.get("users.sort_key").unwrap().kind,
            "number"
        );
    }

    #[test]
    fn parse_with_gsi() {
        let json = serde_json::json!({
            "TableName": "orders",
            "AttributeDefinitions": [
                { "AttributeName": "order_id", "AttributeType": "S" },
                { "AttributeName": "customer_id", "AttributeType": "S" }
            ],
            "KeySchema": [
                { "AttributeName": "order_id", "KeyType": "HASH" }
            ],
            "GlobalSecondaryIndexes": [{
                "IndexName": "customer_index",
                "KeySchema": [
                    { "AttributeName": "customer_id", "KeyType": "HASH" }
                ],
                "Projection": { "ProjectionType": "ALL" }
            }]
        });
        let schema = parse_dynamodb(&json).expect("should parse");
        assert!(schema.has_vertex("orders:gsi:customer_index"));
    }

    #[test]
    fn emit_roundtrip() {
        let json = serde_json::json!({
            "TableName": "items",
            "AttributeDefinitions": [
                { "AttributeName": "item_id", "AttributeType": "S" }
            ],
            "KeySchema": [
                { "AttributeName": "item_id", "KeyType": "HASH" }
            ]
        });
        let schema = parse_dynamodb(&json).expect("parse");
        let emitted = emit_dynamodb(&schema).expect("emit");
        assert_eq!(emitted["TableName"], "items");
        assert!(
            !emitted["AttributeDefinitions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn parse_missing_table_name_fails() {
        let json = serde_json::json!({});
        let result = parse_dynamodb(&json);
        assert!(result.is_err());
    }
}
