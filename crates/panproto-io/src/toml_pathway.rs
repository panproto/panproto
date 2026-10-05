//! Canonical TOML emission for a `WInstance`.
//!
//! This is the no-complement path: the caller has an instance and no record
//! of the bytes it came from, so the output is TOML written from scratch
//! rather than the original document with edits threaded through it. The
//! format-preserving path lives in
//! [`cst_extract`](crate::cst_extract)'s TOML section and should be preferred
//! wherever a complement is available.
//!
//! The instance is mapped directly onto TOML's data model. Going through
//! `serde_json::Value` would first narrow exact integers and decimals and
//! would collapse absence with null. TOML has no null literal, so an explicit
//! null is a typed error while an absent child is omitted.

use std::collections::BTreeMap;

use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{ValueSite, WInstance};
use panproto_schema::{Protocol, ScalarKind, Schema, ValueSpec};

use crate::error::{EmitInstanceError, EmitValueError, EmitValueReason};
use crate::json_pathway::{encode_bytes, protocol_definition, validate_canonical_tree_structure};

/// Emit a `WInstance` as canonical TOML bytes.
///
/// # Errors
///
/// Returns [`EmitInstanceError::Value`] when a value has no faithful TOML
/// representation, or [`EmitInstanceError::Emit`] when the root is not a
/// table or serialization fails.
pub fn emit_toml_bytes(
    schema: &Schema,
    instance: &WInstance,
    protocol: &str,
) -> Result<Vec<u8>, EmitInstanceError> {
    validate_canonical_tree_structure(schema, instance, protocol, "TOML")?;
    let protocol_definition = protocol_definition(protocol);
    let toml_value = build_toml_node(
        schema,
        &protocol_definition,
        instance,
        instance.root,
        false,
        true,
    )?
    .ok_or_else(|| {
        EmitValueError::new(
            ValueSite::new(instance.schema_root.clone(), None),
            "absent",
            "toml",
            EmitValueReason::AbsentNotRepresentable,
        )
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

fn build_toml_node(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    allow_absent: bool,
    schema_guided: bool,
) -> Result<Option<toml::Value>, EmitInstanceError> {
    let node = instance
        .node(node_id)
        .ok_or_else(|| EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("node {node_id} does not exist"),
        })?;
    validate_toml_node_invariants(protocol, instance, node_id, node)?;
    let site = ValueSite::new(node.anchor.clone(), None);
    if let Some(presence) = &node.value {
        return match presence {
            FieldPresence::Present(value @ (Value::Unknown(_) | Value::List(_))) => {
                Err(EmitValueError::new(
                    site,
                    value.type_name(),
                    "toml",
                    EmitValueReason::NotRepresentable,
                )
                .into())
            }
            FieldPresence::Present(value) => {
                let spec = if schema_guided {
                    ValueSpec::for_vertex(schema, protocol, &node.anchor)
                } else {
                    ValueSpec::new(ScalarKind::Any)
                };
                value_to_toml(value, &spec, &site).map(Some)
            }
            FieldPresence::Null => Err(EmitValueError::new(
                site,
                "null",
                "toml",
                EmitValueReason::NullNotRepresentable,
            )
            .into()),
            FieldPresence::Absent if allow_absent => Ok(None),
            FieldPresence::Absent => Err(EmitValueError::new(
                site,
                "absent",
                "toml",
                EmitValueReason::AbsentNotRepresentable,
            )
            .into()),
        };
    }
    if node.is_list() {
        return build_toml_sequence(schema, protocol, instance, node_id, node, schema_guided)
            .map(Some);
    }
    build_toml_mapping(schema, protocol, instance, node_id, node, schema_guided).map(Some)
}

fn validate_toml_node_invariants(
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    node: &panproto_inst::metadata::Node,
) -> Result<(), EmitInstanceError> {
    let has_children = instance
        .children_map
        .get(&node_id)
        .is_some_and(|children| !children.is_empty());
    if !matches!(
        &node.shape,
        panproto_inst::NodeShape::Plain | panproto_inst::NodeShape::List
    ) {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("TOML node {node_id} carries an incompatible structural shape"),
        });
    }
    if node.value.is_some()
        && (node.is_list()
            || has_children
            || node.discriminator.is_some()
            || !node.extra_fields.is_empty())
    {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!(
                "TOML scalar node {node_id} also carries table fields, a discriminator, or children"
            ),
        });
    }
    if node.discriminator.is_some() {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("TOML node {node_id} carries an unsupported discriminator"),
        });
    }
    if node.is_list() && !node.extra_fields.is_empty() {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("TOML list node {node_id} also carries table fields"),
        });
    }
    Ok(())
}

fn build_toml_sequence(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    node: &panproto_inst::metadata::Node,
    schema_guided: bool,
) -> Result<toml::Value, EmitInstanceError> {
    let items_are_schema_guided = schema_guided
        && schema
            .outgoing_edges(&node.anchor)
            .iter()
            .any(|edge| edge.name.is_none());
    let mut items = Vec::new();
    for child in instance.children(node_id) {
        if let Some(item) = build_toml_node(
            schema,
            protocol,
            instance,
            *child,
            false,
            items_are_schema_guided,
        )? {
            items.push(item);
        }
    }
    Ok(toml::Value::Array(items))
}

fn build_toml_mapping(
    schema: &Schema,
    protocol: &Protocol,
    instance: &WInstance,
    node_id: u32,
    node: &panproto_inst::metadata::Node,
    schema_guided: bool,
) -> Result<toml::Value, EmitInstanceError> {
    let site = ValueSite::new(node.anchor.clone(), None);

    let mut fields = BTreeMap::new();
    for (parent, child, edge) in &instance.arcs {
        if *parent != node_id {
            continue;
        }
        let key = edge.name.as_deref().unwrap_or(&edge.tgt).to_owned();
        let child_is_schema_guided = schema_guided && schema.edges.contains_key(edge);
        if let Some(value) = build_toml_node(
            schema,
            protocol,
            instance,
            *child,
            true,
            child_is_schema_guided,
        )? {
            ensure_unique_toml_key(&fields, &key, protocol)?;
            fields.insert(key, value);
        }
    }
    let mut extra_fields: Vec<_> = node.extra_fields.iter().collect();
    extra_fields.sort_by_key(|(key, _)| *key);
    for (key, value) in extra_fields {
        ensure_unique_toml_key(&fields, key, protocol)?;
        fields.insert(
            key.clone(),
            value_to_toml(value, &ValueSpec::new(ScalarKind::Any), &site)?,
        );
    }

    let mut table = toml::map::Map::new();
    for (key, value) in fields {
        table.insert(key, value);
    }
    Ok(toml::Value::Table(table))
}

fn ensure_unique_toml_key(
    fields: &BTreeMap<String, toml::Value>,
    key: &str,
    protocol: &Protocol,
) -> Result<(), EmitInstanceError> {
    if fields.contains_key(key) {
        return Err(EmitInstanceError::Emit {
            protocol: protocol.name.clone(),
            message: format!("duplicate emitted TOML table key {key:?}"),
        });
    }
    Ok(())
}

fn value_to_toml(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<toml::Value, EmitInstanceError> {
    value_to_toml_at_depth(value, spec, site, 0)
}

fn value_to_toml_at_depth(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
    depth: usize,
) -> Result<toml::Value, EmitInstanceError> {
    validate_toml_value_representability(value, spec, site, depth)?;
    encode_toml_value(value, spec, site, depth)
}

fn validate_toml_value_representability(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
    depth: usize,
) -> Result<(), EmitInstanceError> {
    if depth >= crate::json_pathway::MAX_VALUE_EMIT_DEPTH
        && matches!(value, Value::Unknown(_) | Value::List(_))
    {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "toml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    if matches!(value, Value::Null) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "toml",
            EmitValueReason::NullNotRepresentable,
        )
        .into());
    }
    // `toml::to_string` canonicalizes both NaN signs to `nan`. The CST
    // pathway can write `-nan` directly, but the canonical data-model writer
    // must refuse the bit it cannot retain.
    if matches!(value, Value::Float(number) if number.to_bits() == (-f64::NAN).to_bits()) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "toml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    let scalar = !matches!(value, Value::Unknown(_) | Value::List(_));
    if scalar && !toml_scalar_roundtrips(value, spec) {
        return Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "toml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    }
    Ok(())
}

fn encode_toml_value(
    value: &Value,
    spec: &ValueSpec,
    site: &ValueSite,
    depth: usize,
) -> Result<toml::Value, EmitInstanceError> {
    match value {
        Value::Str(text) | Value::Token(text) => Ok(toml::Value::String(text.clone())),
        Value::Int(number) => Ok(toml::Value::Integer(*number)),
        Value::Float(number) => Ok(toml::Value::Float(*number)),
        Value::Bool(value) => Ok(toml::Value::Boolean(*value)),
        Value::Bytes(bytes) => toml_bytes_value(bytes, spec, site),
        Value::Temporal(temporal) => temporal
            .to_string()
            .parse::<toml::value::Datetime>()
            .map(toml::Value::Datetime)
            .map_err(|_| {
                EmitValueError::new(
                    site.clone(),
                    value.type_name(),
                    "toml",
                    EmitValueReason::NotRepresentable,
                )
                .into()
            }),
        Value::Unknown(fields) if spec.kind == ScalarKind::Any => {
            let mut ordered: Vec<_> = fields.iter().collect();
            ordered.sort_by_key(|(key, _)| *key);
            let mut table = toml::map::Map::new();
            for (key, value) in ordered {
                table.insert(
                    key.clone(),
                    value_to_toml_at_depth(
                        value,
                        &ValueSpec::new(ScalarKind::Any),
                        site,
                        depth + 1,
                    )?,
                );
            }
            Ok(toml::Value::Table(table))
        }
        Value::List(items) if spec.kind == ScalarKind::Any => items
            .iter()
            .map(|value| {
                value_to_toml_at_depth(value, &ValueSpec::new(ScalarKind::Any), site, depth + 1)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(toml::Value::Array),
        Value::Null => Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "toml",
            EmitValueReason::NullNotRepresentable,
        )
        .into()),
        Value::Decimal(_)
        | Value::BigInt(_)
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_) => Err(EmitValueError::new(
            site.clone(),
            value.type_name(),
            "toml",
            EmitValueReason::NotRepresentable,
        )
        .into()),
    }
}

fn toml_bytes_value(
    bytes: &[u8],
    spec: &ValueSpec,
    site: &ValueSite,
) -> Result<toml::Value, EmitInstanceError> {
    let ScalarKind::Bytes(encoding) = &spec.kind else {
        return Err(EmitValueError::new(
            site.clone(),
            "bytes",
            "toml",
            EmitValueReason::NotRepresentable,
        )
        .into());
    };
    let text = encode_bytes(bytes, encoding).ok_or_else(|| {
        EmitValueError::new(
            site.clone(),
            "bytes",
            "toml",
            EmitValueReason::NotRepresentable,
        )
    })?;
    Ok(toml::Value::String(text))
}

/// Whether TOML's scalar spelling reparses to the same typed value.
pub(crate) fn toml_scalar_roundtrips(value: &Value, spec: &ValueSpec) -> bool {
    let reparsed = match value {
        Value::Str(text) | Value::Token(text) => coerce(&Lexeme::new(text, LexClass::String), spec),
        Value::Int(number) => coerce(&Lexeme::new(&number.to_string(), LexClass::Number), spec),
        Value::Float(number) => {
            let Some(text) = toml_float_lexeme(*number) else {
                return false;
            };
            crate::json_pathway::coerce_exact_lexeme(&Lexeme::new(&text, LexClass::Number), spec)
        }
        Value::Bool(value) => coerce(
            &Lexeme::new(if *value { "true" } else { "false" }, LexClass::Bool),
            spec,
        ),
        Value::Bytes(bytes) => {
            let ScalarKind::Bytes(encoding) = &spec.kind else {
                return false;
            };
            let Some(text) = encode_bytes(bytes, encoding) else {
                return false;
            };
            coerce(&Lexeme::new(&text, LexClass::String), spec)
        }
        Value::Temporal(value) => {
            coerce(&Lexeme::new(&value.to_string(), LexClass::DateTime), spec)
        }
        Value::Null
        | Value::Decimal(_)
        | Value::BigInt(_)
        | Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Opaque { .. }
        | Value::Unknown(_)
        | Value::List(_)
        | Value::LabeledNull(_) => return false,
    };
    reparsed.is_ok_and(|actual| match (value, actual) {
        (Value::Float(expected), Value::Float(actual)) => expected.to_bits() == actual.to_bits(),
        (_, actual) => value == &actual,
    })
}

/// The TOML spelling of a floating-point value when it preserves IEEE bits.
pub(crate) fn toml_float_lexeme(value: f64) -> Option<String> {
    if value.to_bits() == f64::NAN.to_bits() {
        Some("nan".to_owned())
    } else if value.to_bits() == (-f64::NAN).to_bits() {
        Some("-nan".to_owned())
    } else if value.is_nan() {
        None
    } else if value == f64::INFINITY {
        Some("inf".to_owned())
    } else if value == f64::NEG_INFINITY {
        Some("-inf".to_owned())
    } else {
        let mut text = value.to_string();
        if !text.contains(['.', 'e', 'E']) {
            text.push_str(".0");
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt::Display;

    use panproto_gat::Name;
    use panproto_inst::metadata::Node;
    use panproto_schema::{Edge, Protocol, SchemaBuilder};

    use super::*;

    fn success<T, E: Display>(result: Result<T, E>, context: &str) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error}"),
        }
    }

    fn failure<T, E: Display>(result: Result<T, E>, context: &str) -> E {
        match result {
            Err(error) => error,
            Ok(_) => panic!("{context}"),
        }
    }

    fn some<T>(value: Option<T>, context: &str) -> T {
        value.unwrap_or_else(|| panic!("{context}"))
    }

    fn schema() -> Schema {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("root", "object", None),
            "root",
        );
        let builder = success(builder.vertex("root:value", "value", None), "value");
        let builder = success(
            builder.edge("root", "root:value", "prop", Some("value")),
            "edge",
        );
        success(builder.build(), "schema")
    }

    fn instance_with(presence: FieldPresence) -> WInstance {
        let nodes = HashMap::from([
            (0, Node::new(0, "root")),
            (1, Node::new(1, "root:value").with_value(presence)),
        ]);
        let edge = Edge {
            src: Name::from("root"),
            tgt: Name::from("root:value"),
            kind: Name::from("prop"),
            name: Some(Name::from("value")),
        };
        WInstance::new(nodes, vec![(0, 1, edge)], Vec::new(), 0, "root".into())
    }

    #[test]
    fn toml_null_is_a_typed_error() {
        for presence in [FieldPresence::Null, FieldPresence::Present(Value::Null)] {
            let error = failure(
                emit_toml_bytes(&schema(), &instance_with(presence), "test"),
                "TOML unexpectedly emitted null",
            );
            assert!(matches!(
                error,
                EmitInstanceError::Value(EmitValueError {
                    reason: EmitValueReason::NullNotRepresentable,
                    ..
                })
            ));
        }
    }

    #[test]
    fn toml_absent_child_is_omitted() {
        let emitted = success(
            emit_toml_bytes(&schema(), &instance_with(FieldPresence::Absent), "test"),
            "absent field is omitted",
        );
        let text = success(std::str::from_utf8(&emitted), "TOML UTF-8");
        assert!(!text.contains("value"));
    }

    #[test]
    fn toml_refuses_table_key_collisions() {
        let mut instance = instance_with(FieldPresence::Present(Value::Str("child".into())));
        some(instance.nodes.get_mut(&instance.root), "root")
            .extra_fields
            .insert("value".to_owned(), Value::Str("extra".into()));

        let error = failure(
            emit_toml_bytes(&schema(), &instance, "test"),
            "TOML unexpectedly overwrote a table key",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Emit { message, .. }
                if message.contains("claimed by a declared field route")
        ));
    }

    #[test]
    fn canonical_toml_refuses_schema_claimed_and_ambiguous_table_keys() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "value".into()],
            ..Protocol::default()
        };
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("root", "object", None),
            "root",
        );
        let builder = success(builder.vertex("first", "value", None), "first");
        let builder = success(builder.vertex("second", "value", None), "second");
        let builder = success(
            builder.edge("root", "first", "prop", Some("field")),
            "first route",
        );
        let builder = success(
            builder.edge("root", "second", "prop", Some("field")),
            "second route",
        );
        let schema = success(builder.build(), "schema");

        let mut extra_root = Node::new(0, "root");
        extra_root
            .extra_fields
            .insert("field".into(), Value::Str("extra".into()));
        let extra = WInstance::new(
            HashMap::from([(0, extra_root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let error = failure(
            emit_toml_bytes(&schema, &extra, "test"),
            "TOML unexpectedly moved an extra field into a declared child carrier",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Emit { message, .. }
                if message.contains("claimed by a declared field route")
        ));

        let first_edge = schema.outgoing_edges("root")[0].clone();
        let ambiguous = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (
                    1,
                    Node::new(1, "first")
                        .with_value(FieldPresence::Present(Value::Str("value".into()))),
                ),
            ]),
            vec![(0, 1, first_edge)],
            Vec::new(),
            0,
            "root".into(),
        );
        let error = failure(
            emit_toml_bytes(&schema, &ambiguous, "test"),
            "TOML unexpectedly chose one of two parser routes",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Emit { message, .. }
                if message.contains("one unambiguous parser route")
        ));
    }

    #[test]
    fn canonical_toml_accepts_exact_nested_open_parser_carriers() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("root", "object", None),
            "root",
        );
        let schema = success(builder.build(), "open schema");
        let mut list = Node::new(2, "root:x:list");
        list.shape = panproto_inst::NodeShape::List;
        let instance = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (1, Node::new(1, "root:x")),
                (2, list),
                (
                    3,
                    Node::new(3, "root:x:list:items")
                        .with_value(FieldPresence::Present(Value::Str("a".into()))),
                ),
                (
                    4,
                    Node::new(4, "root:x:list:items")
                        .with_value(FieldPresence::Present(Value::Int(2))),
                ),
            ]),
            vec![
                (
                    0,
                    1,
                    Edge {
                        src: "root".into(),
                        tgt: "root:x".into(),
                        kind: "prop".into(),
                        name: Some("x".into()),
                    },
                ),
                (
                    1,
                    2,
                    Edge {
                        src: "root:x".into(),
                        tgt: "root:x:list".into(),
                        kind: "prop".into(),
                        name: Some("list".into()),
                    },
                ),
                (
                    2,
                    3,
                    Edge {
                        src: "root:x:list".into(),
                        tgt: "root:x:list:items".into(),
                        kind: "item".into(),
                        name: Some("item".into()),
                    },
                ),
                (
                    2,
                    4,
                    Edge {
                        src: "root:x:list".into(),
                        tgt: "root:x:list:items".into(),
                        kind: "item".into(),
                        name: Some("item".into()),
                    },
                ),
            ],
            Vec::new(),
            0,
            "root".into(),
        );

        let emitted = success(
            emit_toml_bytes(&schema, &instance, "test"),
            "emit exact nested open TOML carriers",
        );
        let text = success(std::str::from_utf8(&emitted), "TOML UTF-8");
        let parsed: toml::Table = success(text.parse(), "emitted TOML syntax");
        assert_eq!(parsed["x"]["list"][0].as_str(), Some("a"));
        assert_eq!(parsed["x"]["list"][1].as_integer(), Some(2));
    }

    #[test]
    fn canonical_toml_refuses_extra_fields_in_open_tables() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into()],
            ..Protocol::default()
        };
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("root", "object", None),
            "root",
        );
        let schema = success(builder.build(), "open schema");
        let mut root = Node::new(0, "root");
        root.extra_fields
            .insert("x".into(), Value::Str("value".into()));
        let instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );

        assert!(matches!(
            emit_toml_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Emit { message, .. })
                if message.contains("would reparse as child nodes")
        ));
    }

    #[test]
    fn toml_refuses_scalar_nodes_with_unrepresentable_structure() {
        let mut instance = instance_with(FieldPresence::Present(Value::Str("scalar".into())));
        some(instance.nodes.get_mut(&1), "value node")
            .extra_fields
            .insert("lost".to_owned(), Value::Str("field".into()));
        let error = failure(
            emit_toml_bytes(&schema(), &instance, "test"),
            "TOML unexpectedly discarded scalar structure",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Emit { message, .. } if message.contains("scalar node")
        ));

        let node = some(instance.nodes.get_mut(&1), "value node");
        node.extra_fields.clear();
        node.shape = panproto_inst::NodeShape::List;
        let error = failure(
            emit_toml_bytes(&schema(), &instance, "test"),
            "TOML unexpectedly emitted a scalar with list shape",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Emit { message, .. } if message.contains("scalar node")
        ));
    }

    #[test]
    fn toml_list_shape_refuses_property_edge_carriers() {
        let mut instance = instance_with(FieldPresence::Present(Value::Str("item".into())));
        some(instance.nodes.get_mut(&instance.root), "root").shape = panproto_inst::NodeShape::List;
        let error = failure(
            emit_toml_bytes(&schema(), &instance, "test"),
            "TOML unexpectedly discarded a property carrier in an array",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Emit { message, .. }
                if message.contains("parser-selected list carrier")
        ));
    }

    #[test]
    fn toml_lists_use_the_exact_synthetic_carrier_when_no_item_is_declared() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "array".into(), "value".into()],
            ..Protocol::default()
        };
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("root", "object", None),
            "root",
        );
        let builder = success(builder.vertex("arr", "array", None), "array");
        let builder = success(
            builder.edge("root", "arr", "prop", Some("arr")),
            "array edge",
        );
        let schema = success(builder.build(), "schema");
        let mut list = Node::new(1, "arr");
        list.shape = panproto_inst::NodeShape::List;
        let instance = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (1, list),
                (
                    2,
                    Node::new(2, "arr:items")
                        .with_value(FieldPresence::Present(Value::Str("value".into()))),
                ),
            ]),
            vec![
                (
                    0,
                    1,
                    Edge {
                        src: "root".into(),
                        tgt: "arr".into(),
                        kind: "prop".into(),
                        name: Some("arr".into()),
                    },
                ),
                (
                    1,
                    2,
                    Edge {
                        src: "arr".into(),
                        tgt: "arr:items".into(),
                        kind: "item".into(),
                        name: Some("item".into()),
                    },
                ),
            ],
            Vec::new(),
            0,
            "root".into(),
        );
        assert_eq!(
            success(
                emit_toml_bytes(&schema, &instance, "test"),
                "synthetic TOML list",
            ),
            b"arr = [\"value\"]\n"
        );
    }

    #[test]
    fn toml_synthetic_list_items_use_open_value_semantics() {
        let protocol = Protocol {
            name: "test".into(),
            obj_kinds: vec!["array".into(), "token".into()],
            value_kinds: std::collections::BTreeMap::from([("token".into(), ScalarKind::Token)]),
            ..Protocol::default()
        };
        let builder = success(
            SchemaBuilder::new(&protocol).vertex("arr", "array", None),
            "array",
        );
        let builder = success(
            builder.vertex("arr:items", "token", None),
            "colliding token vertex",
        );
        let builder = success(
            builder.edge("arr", "arr:items", "item", Some("item")),
            "named edge is not a list parser route",
        );
        let schema = success(builder.build(), "schema");
        let mut root = Node::new(0, "arr");
        root.shape = panproto_inst::NodeShape::List;
        let instance = WInstance::new(
            HashMap::from([
                (0, root),
                (
                    1,
                    Node::new(1, "arr:items")
                        .with_value(FieldPresence::Present(Value::Token("x".into()))),
                ),
            ]),
            vec![(
                0,
                1,
                Edge {
                    src: "arr".into(),
                    tgt: "arr:items".into(),
                    kind: "item".into(),
                    name: Some("item".into()),
                },
            )],
            Vec::new(),
            0,
            "arr".into(),
        );

        assert!(matches!(
            emit_toml_bytes(&schema, &instance, "test"),
            Err(EmitInstanceError::Value(_))
        ));
    }

    #[test]
    fn untyped_string_like_variants_are_refused() {
        for value in [Value::Token("closed".into()), Value::Bytes(vec![0, 1, 2])] {
            let error = failure(
                emit_toml_bytes(
                    &schema(),
                    &instance_with(FieldPresence::Present(value)),
                    "test",
                ),
                "open TOML unexpectedly recovered the exact variant",
            );
            assert!(matches!(error, EmitInstanceError::Value(_)));
        }
    }

    #[test]
    fn a_declared_token_has_a_reversible_toml_form() {
        let value = Value::Token("closed".into());
        let emitted = success(
            value_to_toml(
                &value,
                &ValueSpec::new(ScalarKind::Token),
                &ValueSite::new("token", None),
            ),
            "declared token",
        );
        assert_eq!(emitted, toml::Value::String("closed".into()));
    }

    #[test]
    fn toml_refuses_nan_bits_its_text_cannot_preserve() {
        let value = f64::from_bits(f64::NAN.to_bits().wrapping_add(1));
        let error = failure(
            emit_toml_bytes(
                &schema(),
                &instance_with(FieldPresence::Present(Value::Float(value))),
                "test",
            ),
            "TOML unexpectedly collapsed a NaN representation",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));
    }

    #[test]
    fn canonical_toml_refuses_negative_nan_sign_loss() {
        let error = failure(
            emit_toml_bytes(
                &schema(),
                &instance_with(FieldPresence::Present(Value::Float(-f64::NAN))),
                "test",
            ),
            "canonical TOML unexpectedly collapsed negative NaN",
        );
        assert!(matches!(
            error,
            EmitInstanceError::Value(EmitValueError {
                reason: EmitValueReason::NotRepresentable,
                ..
            })
        ));
    }

    #[test]
    fn toml_refuses_container_values_in_scalar_node_slots() {
        for value in [Value::Unknown(HashMap::new()), Value::List(Vec::new())] {
            let error = failure(
                emit_toml_bytes(
                    &schema(),
                    &instance_with(FieldPresence::Present(value)),
                    "test",
                ),
                "TOML unexpectedly changed a scalar value slot into structure",
            );
            assert!(matches!(
                error,
                EmitInstanceError::Value(EmitValueError {
                    reason: EmitValueReason::NotRepresentable,
                    ..
                })
            ));
        }
    }
}
