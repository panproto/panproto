//! DataFrame schema protocol definition (pandera-style).
//!
//! DataFrame uses a constrained hypergraph schema theory
//! (`colimit(ThHypergraph, ThConstraint)`) and a set-valued functor
//! instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the `DataFrame` protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "dataframe".into(),
        schema_theory: "ThDataFrameSchema".into(),
        instance_theory: "ThDataFrameInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "dataframe".into(),
            "column".into(),
            "index".into(),
            "string".into(),
            "int64".into(),
            "float64".into(),
            "bool".into(),
            "datetime".into(),
            "timedelta".into(),
            "category".into(),
            "object".into(),
        ],
        constraint_sorts: vec![
            "nullable".into(),
            "unique".into(),
            "coerce".into(),
            "regex".into(),
            "ge".into(),
            "le".into(),
            "gt".into(),
            "lt".into(),
            "isin".into(),
        ],
        has_order: true,
        nominal_identity: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `DataFrame` with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThDataFrameSchema", "ThDataFrameInstance");
}

/// Parse a pandera-style `DataFrame` schema JSON into a [`Schema`].
///
/// Expects a JSON object with `columns` (object mapping names to column defs)
/// and optional `index` array.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is invalid.
pub fn parse_dataframe_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    let df_name = json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("dataframe");

    builder = builder.vertex(df_name, "dataframe", None)?;

    let mut sig = HashMap::new();

    // Parse columns.
    if let Some(columns) = json.get("columns").and_then(serde_json::Value::as_object) {
        for (col_name, col_def) in columns {
            let col_id = format!("{df_name}.{col_name}");
            let dtype = col_def
                .get("dtype")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let kind = df_type_to_kind(dtype);

            builder = builder.vertex(&col_id, &kind, None)?;
            builder = builder.edge(df_name, &col_id, "prop", Some(col_name))?;
            sig.insert(col_name.clone(), col_id.clone());

            // Parse column constraints.
            if let Some(nullable) = col_def.get("nullable").and_then(serde_json::Value::as_bool) {
                builder = builder.constraint(&col_id, "nullable", &nullable.to_string());
            }
            if let Some(unique) = col_def.get("unique").and_then(serde_json::Value::as_bool) {
                if unique {
                    builder = builder.constraint(&col_id, "unique", "true");
                }
            }
            if let Some(coerce) = col_def.get("coerce").and_then(serde_json::Value::as_bool) {
                if coerce {
                    builder = builder.constraint(&col_id, "coerce", "true");
                }
            }
            if let Some(regex) = col_def.get("regex").and_then(serde_json::Value::as_bool) {
                if regex {
                    builder = builder.constraint(&col_id, "regex", "true");
                }
            }

            // Parse checks.
            if let Some(checks) = col_def.get("checks").and_then(serde_json::Value::as_object) {
                for (check_name, check_val) in checks {
                    match check_name.as_str() {
                        "ge" | "le" | "gt" | "lt" => {
                            builder = builder.constraint(
                                &col_id,
                                check_name,
                                &json_val_to_string(check_val),
                            );
                        }
                        "isin" => {
                            if let Some(arr) = check_val.as_array() {
                                let vals: Vec<String> = arr
                                    .iter()
                                    .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
                                    .collect();
                                builder = builder.constraint(&col_id, "isin", &vals.join(","));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    } else {
        return Err(ProtocolError::MissingField("columns".into()));
    }

    // Parse index columns.
    if let Some(index) = json.get("index").and_then(serde_json::Value::as_array) {
        for idx_def in index {
            let idx_name = idx_def
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("index");
            let idx_id = format!("{df_name}:idx:{idx_name}");
            let dtype = idx_def
                .get("dtype")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("int64");
            let kind = df_type_to_kind(dtype);

            builder = builder.vertex(&idx_id, &kind, None)?;
            builder = builder.edge(df_name, &idx_id, "prop", Some(idx_name))?;
            sig.insert(idx_name.to_string(), idx_id);
        }
    }

    if !sig.is_empty() {
        let he_id = format!("he_{he_counter}");
        he_counter += 1;
        builder = builder.hyper_edge(&he_id, "dataframe", sig, df_name)?;
    }
    let _ = he_counter;

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as pandera-style `DataFrame` schema JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_dataframe_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let roots: Vec<_> = find_roots(schema, &["prop"]);
    let df = roots
        .into_iter()
        .find(|v| v.kind == "dataframe")
        .ok_or_else(|| ProtocolError::Emit("no dataframe vertex found".into()))?;

    let children = children_by_edge(schema, &df.id, "prop");
    let mut columns = serde_json::Map::new();

    for (edge, vertex) in &children {
        let col_name = edge.name.as_deref().unwrap_or(&vertex.id);
        let dtype = kind_to_df_type(&vertex.kind);

        let mut col_obj = serde_json::json!({ "dtype": dtype });

        let constraints = vertex_constraints(schema, &vertex.id);
        let mut checks = serde_json::Map::new();

        for c in &constraints {
            match c.sort.as_str() {
                "nullable" => {
                    col_obj["nullable"] = serde_json::Value::Bool(c.value == "true");
                }
                "unique" if c.value == "true" => {
                    col_obj["unique"] = serde_json::Value::Bool(true);
                }
                "coerce" if c.value == "true" => {
                    col_obj["coerce"] = serde_json::Value::Bool(true);
                }
                "regex" if c.value == "true" => {
                    col_obj["regex"] = serde_json::Value::Bool(true);
                }
                "ge" | "le" | "gt" | "lt" => {
                    let val = c.value.parse::<f64>().map_or_else(
                        |_| serde_json::Value::String(c.value.clone()),
                        |n| serde_json::json!(n),
                    );
                    checks.insert(c.sort.to_string(), val);
                }
                "isin" => {
                    let vals: Vec<serde_json::Value> = c
                        .value
                        .split(',')
                        .map(|s| serde_json::Value::String(s.to_string()))
                        .collect();
                    checks.insert("isin".into(), serde_json::Value::Array(vals));
                }
                _ => {}
            }
        }

        if !checks.is_empty() {
            col_obj["checks"] = serde_json::Value::Object(checks);
        }

        columns.insert(col_name.to_string(), col_obj);
    }

    Ok(serde_json::json!({
        "name": df.id,
        "columns": columns
    }))
}

fn df_type_to_kind(dtype: &str) -> String {
    match dtype.to_lowercase().as_str() {
        "str" | "string" | "object" => "string",
        "int" | "int64" | "int32" | "int16" | "int8" => "int64",
        "float" | "float64" | "float32" => "float64",
        "bool" | "boolean" => "bool",
        "datetime" | "datetime64" | "datetime64[ns]" => "datetime",
        "timedelta" | "timedelta64" | "timedelta64[ns]" => "timedelta",
        "category" => "category",
        _ => "object",
    }
    .into()
}

fn kind_to_df_type(kind: &str) -> &'static str {
    match kind {
        "string" => "string",
        "int64" => "int64",
        "float64" => "float64",
        "bool" => "bool",
        "datetime" => "datetime64[ns]",
        "timedelta" => "timedelta64[ns]",
        "category" => "category",
        _ => "object",
    }
}

fn json_val_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => val.to_string(),
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![EdgeRule {
        edge_kind: "prop".into(),
        src_kinds: vec!["dataframe".into()],
        tgt_kinds: vec![],
    }]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "dataframe");
        assert_eq!(p.schema_theory, "ThDataFrameSchema");
        assert!(p.find_edge_rule("prop").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThDataFrameSchema"));
        assert!(registry.contains_key("ThDataFrameInstance"));
    }

    #[test]
    fn parse_simple_schema() {
        let json = serde_json::json!({
            "name": "users",
            "columns": {
                "name": { "dtype": "string", "nullable": false },
                "age": { "dtype": "int64", "nullable": false, "checks": { "ge": 0, "le": 150 } },
                "score": { "dtype": "float64", "nullable": true }
            }
        });
        let schema = parse_dataframe_schema(&json).expect("should parse");
        assert!(schema.has_vertex("users"));
        assert!(schema.has_vertex("users.name"));
        assert_eq!(schema.vertices.get("users.name").unwrap().kind, "string");
        assert_eq!(schema.vertices.get("users.age").unwrap().kind, "int64");
    }

    #[test]
    fn parse_with_checks() {
        let json = serde_json::json!({
            "columns": {
                "status": {
                    "dtype": "string",
                    "checks": { "isin": ["active", "inactive", "pending"] }
                }
            }
        });
        let schema = parse_dataframe_schema(&json).expect("should parse");
        assert!(schema.has_vertex("dataframe.status"));
    }

    #[test]
    fn emit_roundtrip() {
        let json = serde_json::json!({
            "columns": {
                "x": { "dtype": "int64" },
                "y": { "dtype": "float64" }
            }
        });
        let schema = parse_dataframe_schema(&json).expect("parse");
        let emitted = emit_dataframe_schema(&schema).expect("emit");
        assert_eq!(emitted["columns"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn parse_missing_columns_fails() {
        let json = serde_json::json!({ "name": "broken" });
        let result = parse_dataframe_schema(&json);
        assert!(result.is_err());
    }
}
