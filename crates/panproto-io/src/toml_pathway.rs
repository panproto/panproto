//! Canonical TOML emission for a `WInstance`.
//!
//! This is the no-complement path: the caller has an instance and no record
//! of the bytes it came from, so the output is TOML written from scratch
//! rather than the original document with edits threaded through it. The
//! format-preserving path lives in
//! [`cst_extract`](crate::cst_extract)'s TOML section and should be preferred
//! wherever a complement is available.
//!
//! The instance is first rendered as a JSON value by
//! [`json_pathway::emit_json_value`](crate::json_pathway::emit_json_value),
//! which already knows how to walk a `WInstance`'s props, items and variants,
//! and that value is then mapped onto TOML's data model. The two models
//! differ in one place: TOML has no null. A null-valued key carries no TOML
//! spelling at all, so it is omitted, which is TOML's own way of saying a key
//! has no value.

use panproto_inst::wtype::WInstance;
use panproto_schema::Schema;

use crate::error::EmitInstanceError;

/// Emit a `WInstance` as canonical TOML bytes.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Emit`] if the instance does not map onto
/// TOML's data model: a document whose root is not a table, or one holding a
/// value TOML cannot spell.
pub fn emit_toml_bytes(
    schema: &Schema,
    instance: &WInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    let json = crate::json_pathway::emit_json_value(schema, instance);

    let toml_value = json_to_toml(&json).ok_or_else(|| EmitInstanceError::Emit {
        protocol: protocol.to_owned(),
        message: "instance holds no value TOML can represent".to_owned(),
    })?;

    let toml::Value::Table(table) = toml_value else {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: "a TOML document's root must be a table".to_owned(),
        });
    };

    toml::to_string(&toml::Value::Table(table))
        .map(String::into_bytes)
        .map_err(|e| EmitInstanceError::Emit {
            protocol: protocol.to_owned(),
            message: e.to_string(),
        })
}

/// Map a JSON value onto TOML's data model.
///
/// Returns `None` for a value TOML cannot spell — null, and any container
/// that a null is the whole of — so a caller can drop the key rather than
/// invent a stand-in for it.
fn json_to_toml(value: &serde_json::Value) -> Option<toml::Value> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| n.as_f64().filter(|f| f.is_finite()).map(toml::Value::Float)),
        serde_json::Value::Array(items) => Some(toml::Value::Array(
            items.iter().filter_map(json_to_toml).collect(),
        )),
        serde_json::Value::Object(fields) => Some(toml::Value::Table(
            fields
                .iter()
                .filter_map(|(key, v)| Some((key.clone(), json_to_toml(v)?)))
                .collect(),
        )),
    }
}
