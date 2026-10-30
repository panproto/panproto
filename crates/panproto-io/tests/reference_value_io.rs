//! Same-input differential witnesses for the value-I/O correctness laws.
//!
//! The independent oracle lives in `panproto-reference`. These tests feed one
//! input to the oracle and the public production codec, then compare either
//! the successful observable or the typed refusal class. Keeping the tests in
//! `panproto-io` prevents the oracle from reaching crate-private production
//! helpers.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use panproto_inst::metadata::{Node, NodeShape};
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce};
use panproto_inst::value::{DecimalValue, FieldPresence, Temporal, TemporalForm, Value};
use panproto_inst::{FInstance, WInstance};
use panproto_io::error::{EmitInstanceError, EmitValueReason, ParseInstanceError};
use panproto_io::json_pathway::{emit_json_bytes, parse_json_bytes};
use panproto_io::tabular_pathway::{emit_tsv_with_schema, parse_tsv};
use panproto_io::toml_pathway::emit_toml_bytes;
#[cfg(feature = "tree-sitter")]
use panproto_io::unified_codec::UnifiedCodec;
use panproto_io::xml_pathway::{
    MAX_XML_ENTITY_DEPTH, MAX_XML_ENTITY_EXPANSION_BYTES, XmlEntityError, emit_xml_bytes,
    expand_xml_entities, parse_xml_bytes,
};
use panproto_reference::enumerate::value_io as scope;
use panproto_reference::value::Value as ReferenceValue;
use panproto_reference::value_io::{
    Presence as ReferencePresence, PresenceOutcome, Route, ValueIoError,
};
use panproto_schema::{Edge, ScalarKind, Schema, SchemaBuilder, ValueSpec};

trait TestUnwrap<T> {
    fn test_unwrap(self, context: &str) -> T;
}

impl<T, E: std::fmt::Debug> TestUnwrap<T> for Result<T, E> {
    #[track_caller]
    fn test_unwrap(self, context: &str) -> T {
        self.unwrap_or_else(|error| panic!("{context}: {error:?}"))
    }
}

fn one_text_column(root: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .test_unwrap("root vertex")
        .vertex("value", "text", None)
        .test_unwrap("value vertex")
        .edge(root, "value", "prop", Some("value"))
        .test_unwrap("value edge")
        .build()
        .test_unwrap("one-column schema")
}

fn one_kind_column(root: &str, column: &str, kind: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .test_unwrap("root vertex")
        .vertex(column, kind, None)
        .test_unwrap("column vertex")
        .edge(root, column, "prop", Some(column))
        .test_unwrap("column edge")
        .build()
        .test_unwrap("one-column schema")
}

fn two_text_columns(root: &str, first: &str, second: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .test_unwrap("root vertex")
        .vertex(first, "text", None)
        .test_unwrap("first column vertex")
        .vertex(second, "text", None)
        .test_unwrap("second column vertex")
        .edge(root, first, "prop", Some(first))
        .test_unwrap("first column edge")
        .edge(root, second, "prop", Some(second))
        .test_unwrap("second column edge")
        .build()
        .test_unwrap("two-column schema")
}

/// The default canonical CSV or TSV route of the `rows` table: no comment
/// prefix, the configuration that `TabularCodec::tsv` and `TabularCodec::csv`
/// build, read by `parse_tsv` and written by `emit_tsv_with_schema`.
#[derive(Clone, Copy)]
struct DelimitedRoute {
    delimiter: u8,
}

impl DelimitedRoute {
    const CSV: Self = Self { delimiter: b',' };
    const TSV: Self = Self { delimiter: b'\t' };

    fn parse_functor(self, schema: &Schema, input: &[u8]) -> Result<FInstance, ParseInstanceError> {
        parse_tsv(schema, input, "cassandra", "rows", self.delimiter, None)
    }

    fn emit_functor(
        self,
        schema: &Schema,
        instance: &FInstance,
    ) -> Result<Vec<u8>, EmitInstanceError> {
        emit_tsv_with_schema(schema, instance, "cassandra", "rows", self.delimiter)
    }
}

fn one_temporal_column(root: &str, kind: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .test_unwrap("root vertex")
        .vertex("value", kind, None)
        .test_unwrap("value vertex")
        .edge(root, "value", "prop", Some("value"))
        .test_unwrap("value edge")
        .build()
        .test_unwrap("one-column schema")
}

fn one_decimal_column(root: &str) -> Schema {
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .test_unwrap("root vertex")
        .vertex("value", "decimal", None)
        .test_unwrap("value vertex")
        .edge(root, "value", "prop", Some("value"))
        .test_unwrap("value edge")
        .build()
        .test_unwrap("one-column schema")
}

fn one_tree_value(root: &str, presence: FieldPresence) -> WInstance {
    WInstance::new(
        HashMap::from([
            (0, Node::new(0, root)),
            (1, Node::new(1, "value").with_value(presence)),
        ]),
        vec![(
            0,
            1,
            Edge {
                src: root.into(),
                tgt: "value".into(),
                kind: "prop".into(),
                name: Some("value".into()),
            },
        )],
        Vec::new(),
        0,
        root.into(),
    )
}

fn open_object_chain(value_depth: usize, key: &str, value: Value) -> (Schema, WInstance) {
    assert!(value_depth > 0, "an extra field needs an enclosing object");
    let protocol = panproto_schema::Protocol {
        name: "reference-value-io".into(),
        schema_theory: "ThReferenceValueIoSchema".into(),
        instance_theory: "ThReferenceValueIoInstance".into(),
        obj_kinds: vec!["object".into()],
        ..Default::default()
    };
    let anchors: Vec<_> = (0..value_depth)
        .map(|depth| format!("level-{depth}"))
        .collect();
    let mut builder = SchemaBuilder::new(&protocol);
    for anchor in &anchors {
        builder = builder
            .vertex(anchor, "object", None)
            .test_unwrap("open-chain vertex");
    }

    let mut nodes = HashMap::new();
    for (depth, anchor) in anchors.iter().enumerate() {
        let id = u32::try_from(depth).test_unwrap("open-chain node id");
        nodes.insert(id, Node::new(id, anchor.as_str()));
    }
    let mut arcs = Vec::new();
    for (depth, pair) in anchors.windows(2).enumerate() {
        let edge = Edge {
            src: pair[0].as_str().into(),
            tgt: pair[1].as_str().into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        builder = builder
            .edge(&pair[0], &pair[1], "prop", Some("child"))
            .test_unwrap("open-chain edge");
        arcs.push((
            u32::try_from(depth).test_unwrap("open-chain parent id"),
            u32::try_from(depth + 1).test_unwrap("open-chain child id"),
            edge,
        ));
    }
    nodes
        .get_mut(&u32::try_from(value_depth - 1).test_unwrap("open-chain final id"))
        .expect("the open chain has a final object")
        .extra_fields
        .insert(key.to_owned(), value);

    let schema = builder.build().test_unwrap("open-chain schema");
    let instance = WInstance::new(nodes, arcs, Vec::new(), 0, anchors[0].as_str().into());
    (schema, instance)
}

fn one_xml_tree_value(root: &str, presence: FieldPresence) -> WInstance {
    let mut instance = one_tree_value(root, presence);
    for node in instance.nodes.values_mut() {
        node.shape = NodeShape::XmlElement {
            tag: node.anchor.clone(),
        };
    }
    instance
}

fn one_table_value(value: &str) -> FInstance {
    one_table_scalar(Value::Str(value.to_owned()))
}

fn one_table_scalar(value: Value) -> FInstance {
    FInstance::new()
        .with_table("rows", vec![HashMap::from([("value".to_owned(), value)])])
        .test_unwrap("one-row table")
}

fn tree_value<'a>(instance: &'a WInstance, anchor: &str) -> &'a FieldPresence {
    instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == anchor)
        .and_then(|node| node.value.as_ref())
        .unwrap_or_else(|| panic!("missing value at {anchor}"))
}

fn table_text(instance: &FInstance) -> Option<&str> {
    instance
        .rows("rows")
        .first()
        .and_then(|(_, row)| row.get("value"))
        .and_then(|value| match value {
            Value::Str(text) => Some(text.as_str()),
            _ => None,
        })
}

fn table_value(instance: &FInstance) -> Option<&Value> {
    instance
        .rows("rows")
        .first()
        .and_then(|(_, row)| row.get("value"))
}

fn json_wrapped_scalar(bytes: &[u8]) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("production emitted UTF-8 JSON");
    text.strip_prefix("{\"value\":")
        .and_then(|scalar| scalar.strip_suffix('}'))
        .map(str::as_bytes)
        .map_or_else(
            || panic!("missing deterministic JSON value wrapper in {text:?}"),
            ToOwned::to_owned,
        )
}

#[derive(Debug, PartialEq, Eq)]
enum EntityOutcome {
    Expanded(String),
    Undeclared(String),
    Recursive(String),
    Depth,
    Bytes,
    Malformed,
    Unknown(String),
}

fn production_entity_outcome(result: Result<String, XmlEntityError>) -> EntityOutcome {
    match result {
        Ok(value) => EntityOutcome::Expanded(value),
        Err(XmlEntityError::Undeclared(name)) => EntityOutcome::Undeclared(name),
        Err(XmlEntityError::Recursive(name)) => EntityOutcome::Recursive(name),
        Err(XmlEntityError::Depth) => EntityOutcome::Depth,
        Err(XmlEntityError::Bytes) => EntityOutcome::Bytes,
        Err(XmlEntityError::Malformed) => EntityOutcome::Malformed,
        Err(error) => EntityOutcome::Unknown(error.to_string()),
    }
}

fn xml_entity_declarations(entities: &BTreeMap<String, String>) -> String {
    entities
        .iter()
        .fold(String::new(), |mut output, (name, replacement)| {
            let _ = write!(output, "<!ENTITY {name} \"{replacement}\">");
            output
        })
}

fn reference_entity_outcome(
    result: Result<String, panproto_reference::xml::EntityError>,
) -> EntityOutcome {
    match result {
        Ok(value) => EntityOutcome::Expanded(value),
        Err(panproto_reference::xml::EntityError::Undeclared(name)) => {
            EntityOutcome::Undeclared(name)
        }
        Err(panproto_reference::xml::EntityError::Recursive(name)) => {
            EntityOutcome::Recursive(name)
        }
        Err(panproto_reference::xml::EntityError::Depth) => EntityOutcome::Depth,
        Err(panproto_reference::xml::EntityError::Bytes) => EntityOutcome::Bytes,
        Err(panproto_reference::xml::EntityError::Malformed) => EntityOutcome::Malformed,
    }
}

fn production_xml_document_entity_outcome(
    result: Result<WInstance, ParseInstanceError>,
) -> EntityOutcome {
    match result {
        Ok(instance) => match tree_value(&instance, "value") {
            FieldPresence::Present(Value::Str(value)) => EntityOutcome::Expanded(value.clone()),
            other => EntityOutcome::Unknown(format!("unexpected XML value {other:?}")),
        },
        Err(ParseInstanceError::Parse { protocol, message }) => {
            assert_eq!(protocol, "cassandra");
            let Some((quoted_name, refusal)) = message
                .strip_prefix("XML entity \"")
                .and_then(|tail| tail.split_once('"'))
            else {
                return if message.starts_with("malformed XML entity or character reference at byte")
                    || message.starts_with("XML character reference error")
                {
                    EntityOutcome::Malformed
                } else {
                    EntityOutcome::Unknown(message)
                };
            };
            if refusal.starts_with(" is undeclared at byte Some(") {
                EntityOutcome::Undeclared(quoted_name.to_owned())
            } else if refusal.starts_with(" is recursive at byte Some(") {
                EntityOutcome::Recursive(quoted_name.to_owned())
            } else {
                EntityOutcome::Unknown(message)
            }
        }
        Err(ParseInstanceError::LimitExceeded {
            protocol,
            resource,
            bound,
            byte_offset,
        }) => {
            assert_eq!(protocol, "cassandra");
            assert!(byte_offset.is_some(), "XML entity limit omitted its offset");
            match resource.as_str() {
                "XML entity nesting depth" => {
                    assert_eq!(
                        bound,
                        u64::try_from(MAX_XML_ENTITY_DEPTH).unwrap_or(u64::MAX)
                    );
                    EntityOutcome::Depth
                }
                "XML entity expansion bytes" => {
                    assert_eq!(
                        bound,
                        u64::try_from(MAX_XML_ENTITY_EXPANSION_BYTES).unwrap_or(u64::MAX)
                    );
                    EntityOutcome::Bytes
                }
                _ => EntityOutcome::Unknown(resource),
            }
        }
        Err(error) => EntityOutcome::Unknown(error.to_string()),
    }
}

fn production_representable(result: Result<Vec<u8>, EmitInstanceError>, context: &str) -> bool {
    match result {
        Ok(_) => true,
        Err(EmitInstanceError::Value(error))
            if error.reason == EmitValueReason::NotRepresentable =>
        {
            false
        }
        Err(error) => panic!("unexpected production emission failure for {context}: {error:?}"),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ScalarEmission {
    Emitted(Vec<u8>),
    NotRepresentable,
    NonFinite,
    LimitExceeded,
}

#[derive(Debug, PartialEq, Eq)]
enum ParsedScalar {
    Missing,
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
}

#[derive(Debug, PartialEq, Eq)]
enum ScalarParseOutcome {
    Parsed(ParsedScalar),
    Malformed,
    Fidelity(String),
}

fn reference_scalar(value: &ReferenceValue) -> ParsedScalar {
    match value {
        ReferenceValue::Null => ParsedScalar::Null,
        ReferenceValue::Bool(value) => ParsedScalar::Bool(*value),
        ReferenceValue::Int(value) => ParsedScalar::Int(*value),
        ReferenceValue::Str(value) => ParsedScalar::Str(value.clone()),
        other => panic!("unexpected value in the length-two scalar scope: {other:?}"),
    }
}

fn production_tree_scalar(instance: &WInstance) -> ParsedScalar {
    let value = instance
        .nodes
        .values()
        .find(|node| node.anchor.as_ref() == "value")
        .and_then(|node| node.value.as_ref());
    match value {
        None | Some(FieldPresence::Absent) => ParsedScalar::Missing,
        Some(FieldPresence::Null) => ParsedScalar::Null,
        Some(FieldPresence::Present(Value::Bool(value))) => ParsedScalar::Bool(*value),
        Some(FieldPresence::Present(Value::Int(value))) => ParsedScalar::Int(*value),
        Some(FieldPresence::Present(Value::Str(value))) => ParsedScalar::Str(value.clone()),
        other => panic!("unexpected production tree scalar: {other:?}"),
    }
}

fn production_table_scalar(instance: &FInstance) -> ParsedScalar {
    match table_value(instance) {
        None => ParsedScalar::Missing,
        Some(Value::Bool(value)) => ParsedScalar::Bool(*value),
        Some(Value::Int(value)) => ParsedScalar::Int(*value),
        Some(Value::Str(value)) => ParsedScalar::Str(value.clone()),
        other => panic!("unexpected production table scalar: {other:?}"),
    }
}

/// Interpret a canonical tabular document as the stipulated one-cell scalar
/// carrier. A syntactically valid multi-row or multi-cell document is not a
/// successful parse of that carrier and therefore maps to scalar `Malformed`.
fn production_one_cell_scalar(instance: &FInstance) -> ScalarParseOutcome {
    let rows = instance.rows("rows");
    if rows.len() != 1 {
        return ScalarParseOutcome::Malformed;
    }
    let row = rows[0].1;
    if row.len() != 1 || !row.contains_key("value") {
        return ScalarParseOutcome::Malformed;
    }
    ScalarParseOutcome::Parsed(production_table_scalar(instance))
}

/// Decide whether the unchanged scalar bytes occupy one lexical cell in the
/// deterministic document wrapper. The production parser is still run first;
/// this guard only prevents a delimiter or record terminator from being
/// mistaken for scalar content when the parsed table is projected afterward.
fn scalar_occupies_one_cell(route: Route, scalar: &[u8]) -> bool {
    match route {
        Route::Csv if scalar.starts_with(b"\"") && scalar.ends_with(b"\"") => true,
        Route::Csv => {
            !scalar.contains(&b',') && !scalar.contains(&b'\r') && !scalar.contains(&b'\n')
        }
        Route::Tsv => {
            !scalar.contains(&b'\t') && !scalar.contains(&b'\r') && !scalar.contains(&b'\n')
        }
        _ => panic!("{route:?} is not a tabular scalar route"),
    }
}

fn reference_parse_outcome(result: Result<ReferenceValue, ValueIoError>) -> ScalarParseOutcome {
    match result {
        Ok(value) => ScalarParseOutcome::Parsed(reference_scalar(&value)),
        Err(ValueIoError::Malformed) => ScalarParseOutcome::Malformed,
        Err(ValueIoError::Fidelity(reason)) => ScalarParseOutcome::Fidelity(format!("{reason:?}")),
        Err(
            ValueIoError::NotRepresentable | ValueIoError::NonFinite | ValueIoError::LimitExceeded,
        ) => {
            panic!("the reference parser returned an emission-only refusal")
        }
    }
}

fn production_parse_error(error: ParseInstanceError) -> ScalarParseOutcome {
    match error {
        ParseInstanceError::Fidelity(errors) => ScalarParseOutcome::Fidelity(
            errors
                .iter()
                .map(|error| format!("{:?}", error.reason))
                .collect::<Vec<_>>()
                .join(","),
        ),
        _ => ScalarParseOutcome::Malformed,
    }
}

fn scalar_schema(root: &str, expected: &ScalarParseOutcome) -> Schema {
    let kind = match expected {
        ScalarParseOutcome::Parsed(ParsedScalar::Bool(_)) => "boolean",
        ScalarParseOutcome::Parsed(ParsedScalar::Int(_)) => "bigint",
        ScalarParseOutcome::Fidelity(reason) if reason != "InvalidEncoding" => "decimal",
        ScalarParseOutcome::Parsed(
            ParsedScalar::Missing | ParsedScalar::Null | ParsedScalar::Str(_),
        )
        | ScalarParseOutcome::Malformed
        | ScalarParseOutcome::Fidelity(_) => "text",
    };
    let protocol = panproto_protocols::database::cassandra::protocol();
    SchemaBuilder::new(&protocol)
        .vertex(root, "table", None)
        .test_unwrap("scalar root")
        .vertex("value", kind, None)
        .test_unwrap("scalar value")
        .edge(root, "value", "prop", Some("value"))
        .test_unwrap("scalar edge")
        .build()
        .test_unwrap("scalar schema")
}

fn select_scalar_schema<'a>(
    expected: &ScalarParseOutcome,
    text: &'a Schema,
    boolean: &'a Schema,
    integer: &'a Schema,
    decimal: &'a Schema,
) -> &'a Schema {
    match expected {
        ScalarParseOutcome::Parsed(ParsedScalar::Bool(_)) => boolean,
        ScalarParseOutcome::Parsed(ParsedScalar::Int(_)) => integer,
        ScalarParseOutcome::Fidelity(reason) if reason != "InvalidEncoding" => decimal,
        ScalarParseOutcome::Parsed(
            ParsedScalar::Missing | ParsedScalar::Null | ParsedScalar::Str(_),
        )
        | ScalarParseOutcome::Malformed
        | ScalarParseOutcome::Fidelity(_) => text,
    }
}

fn wrap_scalar(route: Route, scalar: &[u8]) -> Vec<u8> {
    let (prefix, suffix): (&[u8], &[u8]) = match route {
        Route::Json => (b"{\"value\":", b"}"),
        Route::Toml => (b"value = ", b"\n"),
        Route::Yaml => (b"value: ", b"\n"),
        Route::Xml => (b"<root><value>", b"</value></root>"),
        Route::Csv | Route::Tsv => (b"value\n", b"\n"),
    };
    [prefix, scalar, suffix].concat()
}

fn production_emission(
    result: Result<Vec<u8>, EmitInstanceError>,
    context: &str,
) -> ScalarEmission {
    match result {
        Ok(bytes) => ScalarEmission::Emitted(bytes),
        Err(EmitInstanceError::Value(error)) => match error.reason {
            EmitValueReason::NotRepresentable => ScalarEmission::NotRepresentable,
            EmitValueReason::NonFinite => ScalarEmission::NonFinite,
            EmitValueReason::LimitExceeded(_) => ScalarEmission::LimitExceeded,
            other => panic!("unexpected production emission refusal for {context}: {other:?}"),
        },
        Err(error) => panic!("unexpected production emission failure for {context}: {error:?}"),
    }
}

fn reference_emission(result: Result<Vec<u8>, ValueIoError>, context: &str) -> ScalarEmission {
    match result {
        Ok(bytes) => ScalarEmission::Emitted(bytes),
        Err(ValueIoError::NotRepresentable) => ScalarEmission::NotRepresentable,
        Err(ValueIoError::NonFinite) => ScalarEmission::NonFinite,
        Err(ValueIoError::LimitExceeded) => ScalarEmission::LimitExceeded,
        Err(error) => panic!("unexpected reference emission failure for {context}: {error:?}"),
    }
}

/// Law 3 on each canonical/default writer exposed by production. The
/// successful cases inspect the route syntax, while refusals inspect the
/// typed reason; the comparison therefore observes more than `is_ok()`.
#[test]
fn default_routes_match_null_and_absence_reference() {
    let tree_schema = one_text_column("root");

    let json_null = emit_json_bytes(
        &tree_schema,
        &one_tree_value("root", FieldPresence::Null),
        "cassandra",
    )
    .test_unwrap("JSON null");
    assert_eq!(json_null, br#"{"value":null}"#);
    assert_eq!(
        panproto_reference::value_io::presence(Route::Json, ReferencePresence::Null),
        PresenceOutcome::NullSyntax
    );
    let json_absent = emit_json_bytes(
        &tree_schema,
        &one_tree_value("root", FieldPresence::Absent),
        "cassandra",
    )
    .test_unwrap("JSON absence");
    assert_eq!(json_absent, b"{}");
    assert_eq!(
        panproto_reference::value_io::presence(Route::Json, ReferencePresence::Absent),
        PresenceOutcome::Omitted
    );

    let toml_null = emit_toml_bytes(
        &tree_schema,
        &one_tree_value("root", FieldPresence::Null),
        "cassandra",
    );
    assert!(matches!(
        toml_null,
        Err(EmitInstanceError::Value(ref error))
            if error.reason == EmitValueReason::NullNotRepresentable
    ));
    assert_eq!(
        panproto_reference::value_io::presence(Route::Toml, ReferencePresence::Null),
        PresenceOutcome::NullNotRepresentable
    );
    let toml_absent = emit_toml_bytes(
        &tree_schema,
        &one_tree_value("root", FieldPresence::Absent),
        "cassandra",
    )
    .test_unwrap("TOML absence");
    assert!(
        !String::from_utf8(toml_absent)
            .expect("canonical TOML is UTF-8")
            .contains("value")
    );
    assert_eq!(
        panproto_reference::value_io::presence(Route::Toml, ReferencePresence::Absent),
        PresenceOutcome::Omitted
    );

    let xml_null = emit_xml_bytes(
        &tree_schema,
        &one_xml_tree_value("root", FieldPresence::Null),
        "cassandra",
    )
    .test_unwrap("XML null");
    assert!(
        String::from_utf8(xml_null)
            .expect("canonical XML is UTF-8")
            .contains("xsi:nil=\"true\"")
    );
    assert_eq!(
        panproto_reference::value_io::presence(Route::Xml, ReferencePresence::Null),
        PresenceOutcome::NullSyntax
    );
    let xml_absent = emit_xml_bytes(
        &tree_schema,
        &one_xml_tree_value("root", FieldPresence::Absent),
        "cassandra",
    )
    .test_unwrap("XML absence");
    assert!(
        !String::from_utf8(xml_absent)
            .expect("canonical XML is UTF-8")
            .contains("<value")
    );
    assert_eq!(
        panproto_reference::value_io::presence(Route::Xml, ReferencePresence::Absent),
        PresenceOutcome::Omitted
    );

    let table_schema = one_text_column("rows");
    for (route, delimiter) in [(Route::Csv, b','), (Route::Tsv, b'\t')] {
        let null = emit_tsv_with_schema(
            &table_schema,
            &one_table_scalar(Value::Null),
            "cassandra",
            "rows",
            delimiter,
        );
        assert!(matches!(
            null,
            Err(EmitInstanceError::Value(ref error))
                if error.reason == EmitValueReason::NullNotRepresentable
        ));
        assert_eq!(
            panproto_reference::value_io::presence(route, ReferencePresence::Null),
            PresenceOutcome::NullNotRepresentable
        );

        let absent = FInstance::new()
            .with_table("rows", vec![HashMap::new()])
            .test_unwrap("one absent row");
        let absent = emit_tsv_with_schema(&table_schema, &absent, "cassandra", "rows", delimiter);
        assert!(matches!(
            absent,
            Err(EmitInstanceError::Value(ref error))
                if error.reason == EmitValueReason::AbsentNotRepresentable
        ));
        assert_eq!(
            panproto_reference::value_io::presence(route, ReferencePresence::Absent),
            PresenceOutcome::AbsentNotRepresentable
        );
    }

    #[cfg(feature = "tree-sitter")]
    {
        let yaml = UnifiedCodec::yaml("cassandra").expect("YAML canonical codec");
        for (presence, expected, bytes) in [
            (
                ReferencePresence::Null,
                PresenceOutcome::NullSyntax,
                br#"{"value":null}"#.as_slice(),
            ),
            (
                ReferencePresence::Absent,
                PresenceOutcome::Omitted,
                b"{}".as_slice(),
            ),
        ] {
            let instance = one_tree_value(
                "root",
                match presence {
                    ReferencePresence::Null => FieldPresence::Null,
                    ReferencePresence::Absent => FieldPresence::Absent,
                },
            );
            let emitted = panproto_io::InstanceEmitter::emit_wtype(&yaml, &tree_schema, &instance)
                .test_unwrap("YAML presence");
            assert_eq!(emitted, bytes);
            assert_eq!(
                panproto_reference::value_io::presence(Route::Yaml, presence),
                expected
            );
        }
    }
}

/// Laws 10 and 13 over the XML encodings required by C55. Each admitted
/// source is decoded through the public canonical route, then normalized to
/// UTF-8 without a byte-order mark or a foreign encoding declaration. The
/// refusal cases independently derive their first offending source byte.
#[test]
fn canonical_xml_normalizes_encodings_and_reports_typed_refusals() {
    let schema = one_text_column("root");
    for case in scope::canonical_xml_encoding_cases() {
        let source = case.source;
        let expected = case.expected;
        let instance = parse_xml_bytes(&schema, &source, "cassandra")
            .test_unwrap("required XML source encoding");
        assert_eq!(
            tree_value(&instance, "value"),
            &FieldPresence::Present(Value::Str(expected.to_owned()))
        );
        let emitted =
            emit_xml_bytes(&schema, &instance, "cassandra").test_unwrap("canonical XML emit");
        assert!(
            panproto_reference::value_io::encoding(&emitted),
            "canonical route did not normalize source {source:?}: {emitted:?}"
        );
        assert!(
            panproto_reference::value_io::xml_encoding(&emitted),
            "canonical XML retained a foreign encoding marker for {source:?}: {emitted:?}"
        );
        assert_ne!(source, emitted, "canonical route retained source bytes");
        let reparsed =
            parse_xml_bytes(&schema, &emitted, "cassandra").test_unwrap("normalized XML reparses");
        assert_eq!(
            tree_value(&reparsed, "value"),
            &FieldPresence::Present(Value::Str(expected.to_owned()))
        );
    }

    let mut invalid_ascii = b"<?xml version=\"1.0\" encoding=\"US-ASCII\"?><root><value>".to_vec();
    let invalid_offset = u64::try_from(invalid_ascii.len()).expect("source offset fits u64");
    invalid_ascii.push(0x80);
    invalid_ascii.extend_from_slice(b"</value></root>");
    assert!(matches!(
        parse_xml_bytes(&schema, &invalid_ascii, "cassandra"),
        Err(ParseInstanceError::Fidelity(errors))
            if matches!(errors.as_slice(), [error]
                if error.reason == panproto_inst::FidelityReason::InvalidEncoding
                    && error.site.byte_offset == Some(invalid_offset))
    ));

    // A processing instruction whose target merely begins with `xml` is not
    // the XML declaration, whatever its content mentions.
    let stylesheet = br#"<?xml-stylesheet href="encoding.xsl"?><d/>"#;
    assert!(panproto_reference::value_io::xml_encoding(stylesheet));
    assert!(panproto_reference::value_io::xml_encoding(
        br#"<?xml version="1.0" encoding = 'utf-8'?><d/>"#
    ));
    assert!(!panproto_reference::value_io::xml_encoding(
        br#"<?xml version="1.0" encoding="ISO-8859-1"?><d/>"#
    ));

    let unknown = b"<?xml version=\"1.0\" encoding=\"x-unknown\"?><root><value>x</value></root>";
    assert!(matches!(
        parse_xml_bytes(&schema, unknown, "cassandra"),
        Err(ParseInstanceError::UnsupportedFeature {
            protocol,
            feature,
            byte_offset: Some(0),
        }) if protocol == "cassandra" && feature == "XML encoding \"x-unknown\""
    ));
}

/// Law 19 compares the independent reference layout with the public JSON
/// route, then applies the public parser to establish the Law 1 section
/// equation. The nested `$link` object is part of the ATProto blob form.
#[test]
fn canonical_blob_writer_matches_independent_atproto_layout() {
    let protocol = panproto_schema::Protocol {
        name: "reference-value-io".into(),
        schema_theory: "ThReferenceValueIoSchema".into(),
        instance_theory: "ThReferenceValueIoInstance".into(),
        obj_kinds: vec!["object".into()],
        ..Default::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .test_unwrap("blob root vertex")
        .build()
        .test_unwrap("blob schema");
    let direct_blob = Value::Blob {
        ref_: "direct".to_owned(),
        mime: "application/octet-stream".to_owned(),
        size: 1,
    };
    let direct_instance = WInstance::new(
        HashMap::from([(
            0,
            Node::new(0, "root").with_value(FieldPresence::Present(direct_blob)),
        )]),
        Vec::new(),
        Vec::new(),
        0,
        "root".into(),
    );
    assert!(matches!(
        emit_json_bytes(&schema, &direct_instance, "reference-value-io"),
        Err(EmitInstanceError::Value(ref error))
            if error.reason == EmitValueReason::NotRepresentable
    ));

    for fields in scope::blob_fields() {
        let expected_value = Value::Blob {
            ref_: fields.ref_.to_owned(),
            mime: fields.mime.to_owned(),
            size: fields.size,
        };
        let mut root = Node::new(0, "root");
        root.extra_fields
            .insert("blob".to_owned(), expected_value.clone());
        let instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let emitted = emit_json_bytes(&schema, &instance, "reference-value-io")
            .test_unwrap("canonical blob JSON");
        let mut expected = br#"{"blob":"#.to_vec();
        expected.extend_from_slice(&panproto_reference::value_io::blob_writer(
            fields.ref_,
            fields.mime,
            fields.size,
        ));
        expected.push(b'}');
        assert_eq!(emitted, expected, "fields={fields:?}");

        let reparsed = parse_json_bytes(&schema, "root", &emitted, "reference-value-io")
            .test_unwrap("canonical blob JSON reparses");
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.extra_fields.get("blob")),
            Some(&expected_value),
            "fields={fields:?}"
        );
    }

    for input in [
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"}}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":1,"extra":null}}"#.as_slice(),
        br#"{"blob":{"$type":"other","mimeType":"x","ref":{"$link":"r"},"size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mime":"x","ref":{"$link":"r"},"size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":7,"ref":{"$link":"r"},"size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":"r","size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"link":"r"},"size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r","extra":null},"size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":7},"size":1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":-1}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":1.0}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":1e0}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":-0}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":"1"}}"#.as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r"},"size":18446744073709551616}}"#.as_slice(),
    ] {
        let reparsed = parse_json_bytes(&schema, "root", input, "reference-value-io")
            .test_unwrap("a non-blob reserved-shape near miss remains ordinary data");
        assert!(
            matches!(
                reparsed
                    .node(reparsed.root)
                    .and_then(|node| node.extra_fields.get("blob")),
                Some(Value::Unknown(_))
            ),
            "near miss became a reserved Blob: {}",
            String::from_utf8_lossy(input)
        );
    }

    for input in [
        br#"{"blob":{"$type":"blob","mimeType":"x","mimeType":"y","ref":{"$link":"r"},"size":1}}"#
            .as_slice(),
        br#"{"blob":{"$type":"blob","mimeType":"x","ref":{"$link":"r","$link":"s"},"size":1}}"#
            .as_slice(),
    ] {
        assert!(
            matches!(
                parse_json_bytes(&schema, "root", input, "reference-value-io"),
                Err(ParseInstanceError::DuplicateMember { ref name, .. })
                    if name == "mimeType" || name == "$link"
            ),
            "duplicate reserved fields must follow the JSON parser's typed refusal policy: {}",
            String::from_utf8_lossy(input)
        );
    }

    let boundary_blob = Value::Blob {
        ref_: "boundary".to_owned(),
        mime: "application/octet-stream".to_owned(),
        size: u64::MAX,
    };
    let (deep_blob_schema, deep_blob_instance) =
        open_object_chain(125, "blob", boundary_blob.clone());
    let deep_blob_bytes =
        emit_json_bytes(&deep_blob_schema, &deep_blob_instance, "reference-value-io")
            .test_unwrap("a Blob at parser depth 125 emits");
    let deep_blob_reparsed = parse_json_bytes(
        &deep_blob_schema,
        "level-0",
        &deep_blob_bytes,
        "reference-value-io",
    )
    .test_unwrap("a Blob at parser depth 125 reparses");
    assert_eq!(
        deep_blob_reparsed.node_count(),
        deep_blob_instance.node_count()
    );
    assert!(
        panproto_lens::instances_equivalent(&deep_blob_reparsed, &deep_blob_instance),
        "the depth-125 Blob tree must preserve its arc multiset and every parent's child order"
    );
    assert_eq!(
        deep_blob_reparsed
            .node(124)
            .and_then(|node| node.extra_fields.get("blob")),
        Some(&boundary_blob)
    );
    assert_eq!(
        emit_json_bytes(&deep_blob_schema, &deep_blob_reparsed, "reference-value-io")
            .test_unwrap("the depth-125 Blob reparsed instance re-emits"),
        deep_blob_bytes
    );

    let (too_deep_blob_schema, too_deep_blob_instance) =
        open_object_chain(126, "blob", boundary_blob);
    assert!(matches!(
        emit_json_bytes(
            &too_deep_blob_schema,
            &too_deep_blob_instance,
            "reference-value-io"
        ),
        Err(EmitInstanceError::Value(ref error))
            if error.reason == EmitValueReason::NotRepresentable
    ));

    let (deep_scalar_schema, deep_scalar_instance) =
        open_object_chain(127, "scalar", Value::Str("boundary".to_owned()));
    let deep_scalar_bytes = emit_json_bytes(
        &deep_scalar_schema,
        &deep_scalar_instance,
        "reference-value-io",
    )
    .test_unwrap("a scalar at parser depth 127 emits");
    let deep_scalar_reparsed = parse_json_bytes(
        &deep_scalar_schema,
        "level-0",
        &deep_scalar_bytes,
        "reference-value-io",
    )
    .test_unwrap("a scalar at parser depth 127 reparses");
    assert!(
        panproto_lens::instances_equivalent(&deep_scalar_reparsed, &deep_scalar_instance),
        "the depth-127 scalar tree must preserve its arc multiset and every parent's child order"
    );
    assert_eq!(
        emit_json_bytes(
            &deep_scalar_schema,
            &deep_scalar_reparsed,
            "reference-value-io"
        )
        .test_unwrap("the depth-127 scalar reparsed instance re-emits"),
        deep_scalar_bytes
    );

    let (too_deep_scalar_schema, too_deep_scalar_instance) =
        open_object_chain(128, "scalar", Value::Str("boundary".to_owned()));
    assert!(matches!(
        emit_json_bytes(
            &too_deep_scalar_schema,
            &too_deep_scalar_instance,
            "reference-value-io"
        ),
        Err(EmitInstanceError::Value(ref error))
            if error.reason == EmitValueReason::NotRepresentable
    ));
}

/// Law 4 over the complete byte-string scope on the public complement-free
/// canonical readers available without tree-sitter: JSON, XML, CSV and TSV.
/// TOML and YAML have no independent complement-free production reader and
/// are therefore exercised with the final-gate CST routes below.
#[test]
fn bounded_canonical_scalar_parsing_matches_reference() {
    let tree_text = scalar_schema(
        "root",
        &ScalarParseOutcome::Parsed(ParsedScalar::Str(String::new())),
    );
    let tree_boolean = scalar_schema(
        "root",
        &ScalarParseOutcome::Parsed(ParsedScalar::Bool(false)),
    );
    let tree_integer = scalar_schema("root", &ScalarParseOutcome::Parsed(ParsedScalar::Int(0)));
    let tree_decimal = scalar_schema("root", &ScalarParseOutcome::Fidelity("NotANumber".into()));
    let table_text = scalar_schema(
        "rows",
        &ScalarParseOutcome::Parsed(ParsedScalar::Str(String::new())),
    );
    let table_boolean = scalar_schema(
        "rows",
        &ScalarParseOutcome::Parsed(ParsedScalar::Bool(false)),
    );
    let table_integer = scalar_schema("rows", &ScalarParseOutcome::Parsed(ParsedScalar::Int(0)));
    let table_decimal = scalar_schema("rows", &ScalarParseOutcome::Fidelity("NotANumber".into()));
    let inputs = scope::byte_strings().expect("the length-two scope fits its enumeration budget");

    for input in inputs {
        for route in [Route::Json, Route::Xml] {
            let expected =
                reference_parse_outcome(panproto_reference::value_io::parse(route, &input));
            let schema = select_scalar_schema(
                &expected,
                &tree_text,
                &tree_boolean,
                &tree_integer,
                &tree_decimal,
            );
            let wrapped = wrap_scalar(route, &input);
            let production = match route {
                Route::Json => parse_json_bytes(schema, "root", &wrapped, "cassandra"),
                Route::Xml => parse_xml_bytes(schema, &wrapped, "cassandra"),
                _ => unreachable!("the canonical tree scope is fixed above"),
            };
            let actual = production.map_or_else(production_parse_error, |instance| {
                ScalarParseOutcome::Parsed(production_tree_scalar(&instance))
            });
            assert_eq!(
                actual, expected,
                "canonical scalar parse differs: route={route:?}, input={input:?}"
            );
        }

        for (route, delimiter) in [(Route::Csv, b','), (Route::Tsv, b'\t')] {
            let expected =
                reference_parse_outcome(panproto_reference::value_io::parse(route, &input));
            let schema = select_scalar_schema(
                &expected,
                &table_text,
                &table_boolean,
                &table_integer,
                &table_decimal,
            );
            let wrapped = wrap_scalar(route, &input);
            let production = parse_tsv(schema, &wrapped, "cassandra", "rows", delimiter, None);
            let actual = match production {
                Ok(instance) if scalar_occupies_one_cell(route, &input) => {
                    production_one_cell_scalar(&instance)
                }
                Ok(_) => ScalarParseOutcome::Malformed,
                Err(error) => production_parse_error(error),
            };
            assert_eq!(
                actual, expected,
                "canonical scalar parse differs: route={route:?}, input={input:?}"
            );
        }
    }
}

/// Law 4's fixed witnesses beyond the two-byte scope: literal DEL, C1 and
/// C0 characters inside JSON strings, signed and otherwise malformed XML
/// character references, and XML line ends. Each production outcome on the
/// always-compiled reader equals the reference outcome, which is also pinned.
#[test]
fn fixed_scalar_reading_witnesses_match_reference() {
    let text = |value: &str| ScalarParseOutcome::Parsed(ParsedScalar::Str(value.to_owned()));
    let tree_text = scalar_schema(
        "root",
        &ScalarParseOutcome::Parsed(ParsedScalar::Str(String::new())),
    );
    let witnesses: [(Route, &[u8], ScalarParseOutcome); 18] = [
        (Route::Json, b"\"\x7f\"", text("\u{7f}")),
        (Route::Json, "\"\u{80}\"".as_bytes(), text("\u{80}")),
        (Route::Json, "\"\u{85}\"".as_bytes(), text("\u{85}")),
        (Route::Json, "\"\u{9f}\"".as_bytes(), text("\u{9f}")),
        (Route::Json, b"\"a\x7fb\"", text("a\u{7f}b")),
        (Route::Json, b"\"\t\"", ScalarParseOutcome::Malformed),
        (Route::Json, b"\"\x1f\"", ScalarParseOutcome::Malformed),
        (Route::Xml, b"&#+65;", ScalarParseOutcome::Malformed),
        (Route::Xml, b"&#x+41;", ScalarParseOutcome::Malformed),
        (Route::Xml, b"&#-65;", ScalarParseOutcome::Malformed),
        (Route::Xml, b"&#65;", text("A")),
        (Route::Xml, b"&#x41;", text("A")),
        (Route::Xml, b"\r", text("\n")),
        (Route::Xml, b"\r\n", text("\n")),
        (Route::Xml, b"a\rb", text("a\nb")),
        (Route::Xml, b"&#13;", text("\r")),
        (Route::Xml, b"&#xD;", text("\r")),
        (Route::Xml, b"a&#xD;\nb", text("a\r\nb")),
    ];
    for (route, input, pinned) in witnesses {
        let expected = reference_parse_outcome(panproto_reference::value_io::parse(route, input));
        assert_eq!(
            expected, pinned,
            "reference: route={route:?}, input={input:?}"
        );
        let wrapped = wrap_scalar(route, input);
        let production = match route {
            Route::Json => parse_json_bytes(&tree_text, "root", &wrapped, "cassandra"),
            Route::Xml => parse_xml_bytes(&tree_text, &wrapped, "cassandra"),
            _ => unreachable!("the witnesses use the two tree readers"),
        };
        let actual = production.map_or_else(production_parse_error, |instance| {
            ScalarParseOutcome::Parsed(production_tree_scalar(&instance))
        });
        assert_eq!(
            actual, expected,
            "production: route={route:?}, input={input:?}"
        );
    }

    // TOML and YAML double-quoted strings, read by the feature-gated
    // production readers: TOML admits a literal tab and refuses DEL, and
    // YAML's `nb-json` admits both.
    #[cfg(feature = "tree-sitter")]
    for (route, codec, input, pinned) in [
        (
            Route::Toml,
            UnifiedCodec::toml("cassandra"),
            b"\"\t\"".as_slice(),
            text("\t"),
        ),
        (
            Route::Toml,
            UnifiedCodec::toml("cassandra"),
            b"\"a\tb\"".as_slice(),
            text("a\tb"),
        ),
        (
            Route::Toml,
            UnifiedCodec::toml("cassandra"),
            b"\"\x7f\"".as_slice(),
            ScalarParseOutcome::Malformed,
        ),
        (
            Route::Yaml,
            UnifiedCodec::yaml("cassandra"),
            b"\"\t\"".as_slice(),
            text("\t"),
        ),
        (
            Route::Yaml,
            UnifiedCodec::yaml("cassandra"),
            b"\"a\tb\"".as_slice(),
            text("a\tb"),
        ),
    ] {
        let codec = codec.expect("feature-gated canonical codec");
        let expected = reference_parse_outcome(panproto_reference::value_io::parse(route, input));
        assert_eq!(
            expected, pinned,
            "reference: route={route:?}, input={input:?}"
        );
        let wrapped = wrap_scalar(route, input);
        let actual = codec
            .parse_wtype_preserving(&tree_text, &wrapped)
            .map_or_else(production_parse_error, |(instance, _)| {
                ScalarParseOutcome::Parsed(production_tree_scalar(&instance))
            });
        assert_eq!(
            actual, expected,
            "production: route={route:?}, input={input:?}"
        );
    }
}

/// Laws 14 through 17 over their complete word scopes, on the default
/// routes, which have no comment prefix and no quoting on write. Each parse is
/// checked for its exact row count and cell, and each admitted table is
/// written back and compared byte for byte with its source.
#[test]
#[allow(clippy::too_many_lines)]
fn bounded_tabular_observables_match_reference() {
    let schema = one_text_column("rows");
    let tsv = DelimitedRoute::TSV;
    let csv = DelimitedRoute::CSV;
    let one_row = |parsed: &FInstance| -> Option<String> {
        let rows = parsed.rows("rows");
        (rows.len() == 1 && rows[0].1.len() == 1)
            .then(|| table_text(parsed).map(str::to_owned))
            .flatten()
    };

    // Law 14: the default TSV route has no comment lines.
    let mut comment_lines = 0_usize;
    for symbols in scope::sequences(&['#', 'x']) {
        let line: String = symbols.into_iter().collect();
        let input = format!("value\n{line}\n");
        let parsed = tsv
            .parse_functor(&schema, input.as_bytes())
            .test_unwrap("TSV comment-scope parse");
        let production_is_data = !line.is_empty() && one_row(&parsed).as_deref() == Some(&line);
        assert_eq!(
            panproto_reference::value_io::tsv_comment(&line),
            production_is_data,
            "line={line:?}"
        );
        if production_is_data {
            if line.starts_with('#') {
                comment_lines += 1;
            }
            assert_eq!(
                tsv.emit_functor(&schema, &parsed)
                    .test_unwrap("TSV data emit"),
                input.as_bytes(),
                "line={line:?}"
            );
        }
    }
    // `#`, `##` and `#x` begin with the comment byte of an opted-in route.
    assert_eq!(comment_lines, 3);
    let header = one_kind_column("rows", "#header", "text");
    let input = b"#header\nx\n";
    let parsed = tsv
        .parse_functor(&header, input)
        .test_unwrap("a `#` header is data");
    let rows = parsed.rows("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1.get("#header"), Some(&Value::Str("x".to_owned())));
    assert_eq!(
        tsv.emit_functor(&header, &parsed)
            .test_unwrap("`#` header emit"),
        input
    );

    // Law 15 on CSV and TSV: `_` is text and written back as `_`.
    let mut underscores = 0_usize;
    for codec in [&csv, &tsv] {
        for symbols in scope::sequences(&['_', 'x']) {
            let cell: String = symbols.into_iter().collect();
            let input = format!("value\n{cell}\n");
            let parsed = codec
                .parse_functor(&schema, input.as_bytes())
                .test_unwrap("missing-token scope parse");
            let production_is_literal_missing = one_row(&parsed).as_deref() == Some("_");
            assert_eq!(
                panproto_reference::value_io::tabular_missing(&cell),
                production_is_literal_missing,
                "cell={cell:?}"
            );
            if production_is_literal_missing {
                underscores += 1;
            }
            if !cell.is_empty() {
                assert_eq!(
                    one_row(&parsed).as_deref(),
                    Some(cell.as_str()),
                    "cell={cell:?}"
                );
                assert_eq!(
                    codec
                        .emit_functor(&schema, &one_table_value(&cell))
                        .test_unwrap("missing-token scope emit"),
                    input.as_bytes(),
                    "cell={cell:?}"
                );
            }
        }
    }
    assert_eq!(underscores, 2);

    // Law 16 on CSV and TSV: after a one-column header an empty line is one
    // row holding "", written back as an empty line; with two columns it is
    // not a row.
    let pair = two_text_columns("rows", "value", "other");
    let mut empty_rows = 0_usize;
    for (codec, delimiter) in [(&csv, ','), (&tsv, '\t')] {
        for column_count in 1..=2 {
            for symbols in scope::sequences(&["", "x"]) {
                let line = symbols.concat();
                let (schema, header) = if column_count == 1 {
                    (&schema, "value".to_owned())
                } else {
                    (&pair, format!("value{delimiter}other"))
                };
                let input = format!("{header}\n{line}\n");
                let parsed = codec.parse_functor(schema, input.as_bytes());
                let production_is_one_empty = parsed.as_ref().is_ok_and(|parsed| {
                    let rows = parsed.rows("rows");
                    rows.len() == 1
                        && rows[0].1.len() == 1
                        && rows[0].1.get("value") == Some(&Value::Str(String::new()))
                });
                assert_eq!(
                    panproto_reference::value_io::tabular_empty_row(column_count, &line),
                    production_is_one_empty,
                    "columns={column_count}, line={line:?}, result={parsed:?}"
                );
                match (column_count, line.as_str(), &parsed) {
                    (1, _, Ok(parsed)) => {
                        assert_eq!(one_row(parsed).as_deref(), Some(line.as_str()));
                        assert_eq!(
                            codec
                                .emit_functor(schema, parsed)
                                .test_unwrap("one-column emit"),
                            input.as_bytes(),
                            "line={line:?}"
                        );
                        if line.is_empty() {
                            empty_rows += 1;
                        }
                    }
                    (2, "", Ok(parsed)) => assert!(parsed.rows("rows").is_empty()),
                    (2, _, Err(ParseInstanceError::Parse { message, .. })) => {
                        assert!(message.contains("fields"), "line={line:?}: {message}");
                    }
                    other => panic!("unexpected tabular outcome {other:?}"),
                }
            }
        }
    }
    // The empty word, [""] and ["", ""] concatenate to the empty line, on
    // two routes.
    assert_eq!(empty_rows, 6);

    // Law 17: a TSV cell is written exactly, quote included, unless it holds
    // a tab or a line break.
    let mut writable = 0_usize;
    for symbols in scope::sequences(&['"', '\t', '\r', '\n']) {
        let cell: String = symbols.into_iter().collect();
        let production = tsv.emit_functor(&schema, &one_table_value(&cell));
        let expected = panproto_reference::value_io::tsv_quote(&cell);
        match production {
            Ok(bytes) => {
                assert!(expected, "cell={cell:?}");
                assert_eq!(
                    bytes,
                    format!("value\n{cell}\n").into_bytes(),
                    "cell={cell:?}"
                );
                let parsed = tsv
                    .parse_functor(&schema, &bytes)
                    .test_unwrap("TSV quote reread");
                assert_eq!(one_row(&parsed).as_deref(), Some(cell.as_str()));
                writable += 1;
            }
            Err(EmitInstanceError::Value(ref error))
                if error.reason == EmitValueReason::NotRepresentable =>
            {
                assert!(!expected, "cell={cell:?}");
            }
            Err(error) => panic!("unexpected TSV emission failure for {cell:?}: {error:?}"),
        }
    }
    // The empty cell, `"` and `""`; the empty cell is written as an empty
    // line under a one-column header.
    assert_eq!(writable, 3);
}

/// Law 9 over member sequences of length at most two. JSON members are
/// spelled plainly or by a `\u` escape and compared after unescaping; XML
/// attributes are spelled unprefixed or under two prefixes bound to one
/// namespace and compared by expanded name. Every duplicate must be the
/// typed `DuplicateMember` refusal naming the decoded name at the second
/// name token, and no other sequence may receive it.
#[test]
fn bounded_duplicate_member_refusals_match_reference() {
    let schema = one_text_column("root");
    let json_spellings = [("a", "\"a\""), ("a", "\"\\u0061\""), ("b", "\"b\"")];
    let xml_spellings = [("a", "a"), ("{urn:u}a", "p:a"), ("{urn:u}a", "q:a")];
    let mut duplicates = [0_usize; 2];
    for (route, spellings) in [(Route::Json, json_spellings), (Route::Xml, xml_spellings)] {
        let sequences = scope::sequences(&spellings);
        // The empty sequence, three of length one and nine of length two.
        assert_eq!(sequences.len(), 13);
        for members in sequences {
            let decoded: Vec<_> = members
                .iter()
                .enumerate()
                .map(|(index, (name, _))| ((*name).to_owned(), index))
                .collect();
            let (input, offsets) = if route == Route::Json {
                let mut input = String::from("{");
                let mut offsets = Vec::new();
                for (index, (_, spelling)) in members.iter().enumerate() {
                    if index > 0 {
                        input.push(',');
                    }
                    offsets.push(input.len());
                    input.push_str(spelling);
                    input.push_str(":\"x\"");
                }
                input.push('}');
                (input, offsets)
            } else {
                let mut input = String::from("<root xmlns:p=\"urn:u\" xmlns:q=\"urn:u\"");
                let mut offsets = Vec::new();
                for (_, spelling) in &members {
                    input.push(' ');
                    offsets.push(input.len());
                    input.push_str(spelling);
                    input.push_str("=\"x\"");
                }
                input.push_str("><value>x</value></root>");
                (input, offsets)
            };
            let production = if route == Route::Json {
                parse_json_bytes(&schema, "root", input.as_bytes(), "cassandra")
            } else {
                parse_xml_bytes(&schema, input.as_bytes(), "cassandra")
            };
            if panproto_reference::value_io::duplicate_member(&decoded) {
                duplicates[usize::from(route == Route::Xml)] += 1;
                let expected_offset = u64::try_from(offsets[1]).ok();
                assert!(
                    matches!(
                        production,
                        Err(ParseInstanceError::DuplicateMember {
                            ref protocol,
                            ref name,
                            byte_offset,
                        }) if protocol == "cassandra"
                            && name == members[1].0
                            && byte_offset == expected_offset
                    ),
                    "route={route:?}, input={input}, result={production:?}"
                );
            } else {
                assert!(
                    !matches!(production, Err(ParseInstanceError::DuplicateMember { .. })),
                    "route={route:?}, input={input}, result={production:?}"
                );
            }
        }
    }
    // Of the nine pairs on each route, five repeat a decoded name: on JSON
    // the four pairs over the two spellings of `a` and (b, b); on XML (a, a)
    // and the four pairs over `p:a` and `q:a`.
    assert_eq!(duplicates, [5, 5]);
}

/// Law 23 on the JSON CST production path, separately from the canonical
/// duplicate refusal covered above.
#[cfg(feature = "tree-sitter")]
#[test]
fn bounded_cst_duplicate_member_refusals_match_reference() {
    let schema = one_text_column("root");
    let codec = UnifiedCodec::json("cassandra").expect("JSON CST codec");
    for names in scope::sequences(&["a", "b"]) {
        let members: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| ((*name).to_owned(), index))
            .collect();
        let body = names
            .iter()
            .map(|name| format!("\"{name}\":\"x\""))
            .collect::<Vec<_>>()
            .join(",");
        let input = format!("{{{body}}}");
        let production = codec.parse_wtype_preserving(&schema, input.as_bytes());
        let production_duplicate = matches!(
            production,
            Err(ParseInstanceError::Parse { ref message, .. })
                if message.contains("duplicate JSON object key")
        );
        assert_eq!(
            panproto_reference::value_io::duplicate_member_cst(&members),
            production_duplicate,
            "members={members:?}, result={production:?}"
        );
    }
}

/// Laws 7 and 11 over the declared exact-numeral scope. Production's TOML
/// emitter supplies the route spelling and the reference emitter the expected
/// one; the oracle decides writer agreement and representation equality
/// under the declared `Decimal` kind on that pair of strings. Every special
/// decimal is then offered to the five always-compiled writers, each of which
/// must refuse it with `NonFinite`, as the reference does, since no route
/// reads a special spelling back as the same `Decimal`.
#[test]
fn bounded_decimal_writer_spellings_match_reference() {
    let schema = one_decimal_column("root");
    let numerals = scope::numerals();
    // Independent count: two signs, the 10 one-digit and 90 two-digit
    // significands, and three exponents (-1, 0, 1): 2 * 100 * 3.
    assert_eq!(numerals.len(), 600);
    for numeral in numerals {
        let decimal = DecimalValue::parse(&numeral)
            .unwrap_or_else(|error| panic!("production decimal scope {numeral:?}: {error:?}"));
        let reference_value = ReferenceValue::Decimal(
            panproto_reference::exact::parse_decimal(&numeral)
                .unwrap_or_else(|| panic!("reference decimal scope rejected {numeral:?}")),
        );
        let ScalarEmission::Emitted(reference) = reference_emission(
            panproto_reference::value_io::emit(Route::Toml, &reference_value),
            &format!("TOML decimal {numeral:?}"),
        ) else {
            panic!("the reference TOML writer refused the finite decimal {numeral:?}");
        };
        let reference = String::from_utf8(reference).expect("reference TOML token is UTF-8");
        let instance = one_tree_value("root", FieldPresence::Present(Value::Decimal(decimal)));
        let bytes = emit_toml_bytes(&schema, &instance, "cassandra")
            .unwrap_or_else(|error| panic!("TOML refused admitted numeral {numeral:?}: {error:?}"));
        let text = std::str::from_utf8(&bytes).expect("TOML output is UTF-8");
        let emitted = text
            .lines()
            .find_map(|line| line.strip_prefix("value = "))
            .unwrap_or_else(|| panic!("missing scalar in {text:?}"));

        assert!(
            panproto_reference::value_io::decimal(&reference, emitted),
            "numeral={numeral:?}, reference={reference:?}, emitted={emitted:?}"
        );
        assert!(
            panproto_reference::value_io::toml_scalar(
                &panproto_reference::coerce::ScalarKind::Decimal,
                &numeral,
                emitted
            ),
            "numeral={numeral:?}, emitted={emitted:?}"
        );
    }

    let tree = one_kind_column("root", "value", "decimal");
    let table = one_kind_column("rows", "value", "decimal");
    let mut refusals = 0_usize;
    for lexeme in scope::special_decimal_lexemes() {
        let special = DecimalValue::parse(lexeme)
            .unwrap_or_else(|error| panic!("production special decimal {lexeme}: {error:?}"));
        let reference = panproto_reference::exact::parse_decimal(lexeme)
            .unwrap_or_else(|| panic!("reference special decimal {lexeme}"));
        for route in CANONICAL_EMIT_ROUTES {
            let schema = if matches!(route, Route::Csv | Route::Tsv) {
                &table
            } else {
                &tree
            };
            let value = Value::Decimal(special.clone());
            let (production, _) = production_scalar(route, schema, &value);
            let expected = reference_emission(
                panproto_reference::value_io::emit(
                    route,
                    &ReferenceValue::Decimal(reference.clone()),
                ),
                &format!("{route:?} special decimal {lexeme}"),
            );
            assert_eq!(production, expected, "route={route:?}, lexeme={lexeme}");
            assert_eq!(
                production,
                ScalarEmission::NonFinite,
                "route={route:?}, lexeme={lexeme}"
            );
            refusals += 1;
        }
    }
    // Ten special values refused on five routes.
    assert_eq!(refusals, 50);
}

/// The five always-compiled canonical writers.
const CANONICAL_EMIT_ROUTES: [Route; 5] =
    [Route::Json, Route::Toml, Route::Xml, Route::Csv, Route::Tsv];

/// Emit `value` in the one-scalar document of `route` (a `root` object with a
/// `value` member, or a `rows` table with a `value` column).
fn production_document(
    route: Route,
    schema: &Schema,
    value: &Value,
) -> Result<Vec<u8>, EmitInstanceError> {
    let presence = FieldPresence::Present(value.clone());
    match route {
        Route::Json => emit_json_bytes(schema, &one_tree_value("root", presence), "cassandra"),
        Route::Toml => emit_toml_bytes(schema, &one_tree_value("root", presence), "cassandra"),
        Route::Xml => emit_xml_bytes(schema, &one_xml_tree_value("root", presence), "cassandra"),
        Route::Csv => emit_tsv_with_schema(
            schema,
            &one_table_scalar(value.clone()),
            "cassandra",
            "rows",
            b',',
        ),
        Route::Tsv => emit_tsv_with_schema(
            schema,
            &one_table_scalar(value.clone()),
            "cassandra",
            "rows",
            b'\t',
        ),
        Route::Yaml => panic!("YAML has no always-compiled canonical writer"),
    }
}

/// The scalar token inside a production one-scalar document.
fn document_scalar(route: Route, bytes: &[u8]) -> Vec<u8> {
    match route {
        Route::Json => json_wrapped_scalar(bytes),
        Route::Toml => {
            let text = std::str::from_utf8(bytes).expect("production emitted UTF-8 TOML");
            let token = text
                .strip_prefix("value = ")
                .unwrap_or_else(|| panic!("missing TOML scalar in {text:?}"));
            token
                .strip_suffix('\n')
                .unwrap_or_else(|| panic!("unterminated TOML scalar in {text:?}"))
                .as_bytes()
                .to_vec()
        }
        Route::Xml => {
            let text = std::str::from_utf8(bytes).expect("production emitted UTF-8 XML");
            text.split_once("<value>")
                .and_then(|(_, tail)| tail.split_once("</value>"))
                .map_or_else(
                    || panic!("missing XML scalar in {text:?}"),
                    |(scalar, _)| scalar.as_bytes().to_vec(),
                )
        }
        Route::Csv | Route::Tsv => {
            let body = bytes
                .strip_prefix(b"value\n")
                .and_then(|tail| tail.strip_suffix(b"\n"))
                .unwrap_or_else(|| panic!("missing tabular scalar in {bytes:?}"));
            body.to_vec()
        }
        Route::Yaml => panic!("YAML has no always-compiled canonical writer"),
    }
}

/// Read a production one-scalar document back through the matching
/// always-compiled reader. TOML has none in the default build.
fn production_reread(route: Route, schema: &Schema, bytes: &[u8]) -> Option<Value> {
    let tree = |parsed: Result<WInstance, ParseInstanceError>| {
        parsed.ok().and_then(|instance| {
            instance
                .nodes
                .values()
                .find(|node| node.anchor.as_ref() == "value")
                .and_then(|node| match &node.value {
                    Some(FieldPresence::Present(value)) => Some(value.clone()),
                    _ => None,
                })
        })
    };
    match route {
        Route::Json => tree(parse_json_bytes(schema, "root", bytes, "cassandra")),
        Route::Xml => tree(parse_xml_bytes(schema, bytes, "cassandra")),
        Route::Csv | Route::Tsv => {
            let delimiter = if route == Route::Csv { b',' } else { b'\t' };
            parse_tsv(schema, bytes, "cassandra", "rows", delimiter, None)
                .ok()
                .filter(|parsed| parsed.rows("rows").len() == 1)
                .and_then(|parsed| table_value(&parsed).cloned())
        }
        Route::Toml | Route::Yaml => None,
    }
}

/// Production emission of one scalar, its scalar token on success, and the
/// value its document reads back as, when the route has a default reader.
fn production_scalar(
    route: Route,
    schema: &Schema,
    value: &Value,
) -> (ScalarEmission, Option<Value>) {
    match production_emission(
        production_document(route, schema, value),
        &format!("{route:?} {value:?}"),
    ) {
        ScalarEmission::Emitted(bytes) => (
            ScalarEmission::Emitted(document_scalar(route, &bytes)),
            production_reread(route, schema, &bytes),
        ),
        refusal => (refusal, None),
    }
}

/// Equality of carriers, binary floats by their bits.
fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Float(left), Value::Float(right)) => left.to_bits() == right.to_bits(),
        _ => left == right,
    }
}

fn tree_scalar_bytes(route: Route, schema: &Schema, instance: &WInstance) -> ScalarEmission {
    match route {
        Route::Json => {
            let bytes = match production_emission(
                emit_json_bytes(schema, instance, "cassandra"),
                "JSON scalar",
            ) {
                ScalarEmission::Emitted(bytes) => bytes,
                refusal => return refusal,
            };
            ScalarEmission::Emitted(json_wrapped_scalar(&bytes))
        }
        Route::Toml => {
            let bytes = match production_emission(
                emit_toml_bytes(schema, instance, "cassandra"),
                "TOML scalar",
            ) {
                ScalarEmission::Emitted(bytes) => bytes,
                refusal => return refusal,
            };
            let text = std::str::from_utf8(&bytes).expect("production emitted UTF-8 TOML");
            ScalarEmission::Emitted(
                text.lines()
                    .find_map(|line| line.strip_prefix("value = "))
                    .map(str::as_bytes)
                    .map_or_else(
                        || panic!("missing TOML scalar in {text:?}"),
                        ToOwned::to_owned,
                    ),
            )
        }
        Route::Xml => {
            let bytes = match production_emission(
                panproto_io::xml_pathway::emit_xml_bytes(schema, instance, "cassandra"),
                "XML scalar",
            ) {
                ScalarEmission::Emitted(bytes) => bytes,
                refusal => return refusal,
            };
            let text = std::str::from_utf8(&bytes).expect("production emitted UTF-8 XML");
            ScalarEmission::Emitted(
                text.split_once("<value>")
                    .and_then(|(_, tail)| tail.split_once("</value>"))
                    .map_or_else(
                        || panic!("missing XML scalar in {text:?}"),
                        |(scalar, _)| scalar.as_bytes().to_vec(),
                    ),
            )
        }
        Route::Yaml | Route::Csv | Route::Tsv => {
            panic!("{route:?} is not a tree route in this helper")
        }
    }
}

fn tabular_scalar_bytes(route: Route, schema: &Schema, instance: &FInstance) -> ScalarEmission {
    let delimiter = match route {
        Route::Csv => b',',
        Route::Tsv => b'\t',
        _ => panic!("{route:?} is not a tabular route"),
    };
    let bytes = match production_emission(
        emit_tsv_with_schema(schema, instance, "cassandra", "rows", delimiter),
        "tabular scalar",
    ) {
        ScalarEmission::Emitted(bytes) => bytes,
        refusal => return refusal,
    };
    let mut lines = bytes.split(|byte| *byte == b'\n');
    let _header = lines.next();
    ScalarEmission::Emitted(lines.next().map_or_else(
        || panic!("missing tabular scalar in {bytes:?}"),
        ToOwned::to_owned,
    ))
}

/// Law 2 over the complete two-digit, three-exponent decimal scope on every
/// public canonical route. YAML is included when its public codec is enabled.
#[test]
fn bounded_decimal_emission_matches_reference() {
    let tree_schema = one_decimal_column("root");
    let table_schema = one_decimal_column("rows");
    #[cfg(feature = "tree-sitter")]
    let yaml_codec = UnifiedCodec::yaml("cassandra").expect("YAML canonical codec");
    for numeral in scope::numerals() {
        let reference_decimal = panproto_reference::exact::parse_decimal(&numeral)
            .unwrap_or_else(|| panic!("reference decimal scope rejected {numeral:?}"));
        let reference_value = ReferenceValue::Decimal(reference_decimal);
        let production_decimal = panproto_inst::value::DecimalValue::parse(&numeral)
            .unwrap_or_else(|error| panic!("production decimal scope {numeral:?}: {error:?}"));

        for route in [Route::Json, Route::Toml, Route::Xml] {
            let presence = FieldPresence::Present(Value::Decimal(production_decimal.clone()));
            let instance = if route == Route::Xml {
                one_xml_tree_value("root", presence)
            } else {
                one_tree_value("root", presence)
            };
            let production = tree_scalar_bytes(route, &tree_schema, &instance);
            let reference = reference_emission(
                panproto_reference::value_io::emit(route, &reference_value),
                &format!("{route:?} decimal {numeral:?}"),
            );
            assert_eq!(
                production, reference,
                "route={route:?}, numeral={numeral:?}, production_decimal={production_decimal:?}"
            );
        }

        #[cfg(feature = "tree-sitter")]
        {
            let instance = one_tree_value(
                "root",
                FieldPresence::Present(Value::Decimal(production_decimal.clone())),
            );
            let production = production_emission(
                panproto_io::InstanceEmitter::emit_wtype(&yaml_codec, &tree_schema, &instance),
                &format!("YAML decimal {numeral:?}"),
            );
            let production = match production {
                ScalarEmission::Emitted(bytes) => {
                    ScalarEmission::Emitted(json_wrapped_scalar(&bytes))
                }
                refusal => refusal,
            };
            let reference = reference_emission(
                panproto_reference::value_io::emit(Route::Yaml, &reference_value),
                &format!("YAML decimal {numeral:?}"),
            );
            assert_eq!(production, reference, "route=Yaml, numeral={numeral:?}");
        }

        for route in [Route::Csv, Route::Tsv] {
            let instance = one_table_scalar(Value::Decimal(production_decimal.clone()));
            let production = tabular_scalar_bytes(route, &table_schema, &instance);
            let reference = reference_emission(
                panproto_reference::value_io::emit(route, &reference_value),
                &format!("{route:?} decimal {numeral:?}"),
            );
            assert_eq!(
                production, reference,
                "route={route:?}, numeral={numeral:?}, production_decimal={production_decimal:?}"
            );
        }
    }
}

/// Law 1 over the complete numeral scope on the four public complement-free
/// canonical readers: JSON, XML, CSV and TSV. TOML and YAML reading delegates
/// to the preserving CST machinery and is therefore a final-gate obligation.
/// Each successful production emission is parsed back through the matching
/// production reader and compared with the exact input carrier.
#[test]
fn bounded_decimal_round_trips_match_reference() {
    let tree_schema = one_decimal_column("root");
    let table_schema = one_decimal_column("rows");

    for numeral in scope::numerals() {
        let reference_decimal = panproto_reference::exact::parse_decimal(&numeral)
            .unwrap_or_else(|| panic!("reference decimal scope rejected {numeral:?}"));
        let reference_value = ReferenceValue::Decimal(reference_decimal);
        let production_decimal = panproto_inst::value::DecimalValue::parse(&numeral)
            .unwrap_or_else(|error| panic!("production decimal scope {numeral:?}: {error:?}"));

        for route in [Route::Json, Route::Xml] {
            let presence = FieldPresence::Present(Value::Decimal(production_decimal.clone()));
            let instance = if route == Route::Xml {
                one_xml_tree_value("root", presence)
            } else {
                one_tree_value("root", presence)
            };
            let expected = panproto_reference::value_io::round_trip(route, &reference_value);
            let emitted = match route {
                Route::Json => emit_json_bytes(&tree_schema, &instance, "cassandra"),
                Route::Xml => emit_xml_bytes(&tree_schema, &instance, "cassandra"),
                _ => unreachable!("the canonical tree routes are fixed above"),
            };
            match emitted {
                Ok(bytes) => {
                    assert!(expected, "reference refused {route:?} decimal {numeral:?}");
                    let parsed = match route {
                        Route::Json => parse_json_bytes(&tree_schema, "root", &bytes, "cassandra"),
                        Route::Xml => parse_xml_bytes(&tree_schema, &bytes, "cassandra"),
                        _ => unreachable!("the canonical tree routes are fixed above"),
                    }
                    .unwrap_or_else(|error| {
                        panic!(
                            "production failed to reparse {route:?} decimal {numeral:?}: {error:?}"
                        )
                    });
                    assert_eq!(
                        tree_value(&parsed, "value"),
                        &FieldPresence::Present(Value::Decimal(production_decimal.clone())),
                        "route={route:?}, numeral={numeral:?}"
                    );
                }
                Err(EmitInstanceError::Value(error))
                    if error.reason == EmitValueReason::NotRepresentable =>
                {
                    assert!(
                        !expected,
                        "production refused {route:?} decimal {numeral:?}"
                    );
                }
                Err(error) => panic!(
                    "unexpected production emission failure for {route:?} decimal {numeral:?}: {error:?}"
                ),
            }
        }

        for (route, delimiter) in [(Route::Csv, b','), (Route::Tsv, b'\t')] {
            let instance = one_table_scalar(Value::Decimal(production_decimal.clone()));
            let expected = panproto_reference::value_io::round_trip(route, &reference_value);
            match emit_tsv_with_schema(&table_schema, &instance, "cassandra", "rows", delimiter) {
                Ok(bytes) => {
                    assert!(expected, "reference refused {route:?} decimal {numeral:?}");
                    let parsed = parse_tsv(
                        &table_schema,
                        &bytes,
                        "cassandra",
                        "rows",
                        delimiter,
                        None,
                    )
                    .unwrap_or_else(|error| {
                        panic!(
                            "production failed to reparse {route:?} decimal {numeral:?}: {error:?}"
                        )
                    });
                    assert_eq!(
                        table_value(&parsed),
                        Some(&Value::Decimal(production_decimal.clone())),
                        "route={route:?}, numeral={numeral:?}"
                    );
                }
                Err(EmitInstanceError::Value(error))
                    if error.reason == EmitValueReason::NotRepresentable =>
                {
                    assert!(
                        !expected,
                        "production refused {route:?} decimal {numeral:?}"
                    );
                }
                Err(error) => panic!(
                    "unexpected production emission failure for {route:?} decimal {numeral:?}: {error:?}"
                ),
            }
        }
    }
}

/// Laws 4, 20 and 22 over every byte string of length at most two on all six
/// public preserving routes. The exact scalar bytes are embedded unchanged in
/// a deterministic one-field or one-cell document wrapper. Successful
/// production CST parses must retain the wrapper byte-for-byte.
#[cfg(feature = "tree-sitter")]
#[test]
#[ignore = "final gate: YAML recovery must reject uncovered source bytes"]
fn bounded_scalar_and_cst_parsing_matches_reference() {
    let tree_codecs = [
        (
            Route::Json,
            UnifiedCodec::json("cassandra").expect("JSON codec"),
        ),
        (
            Route::Toml,
            UnifiedCodec::toml("cassandra").expect("TOML codec"),
        ),
        (
            Route::Yaml,
            UnifiedCodec::yaml("cassandra").expect("YAML codec"),
        ),
        (
            Route::Xml,
            UnifiedCodec::xml("cassandra").expect("XML codec"),
        ),
    ];
    let table_codecs = [
        (
            Route::Csv,
            UnifiedCodec::csv("cassandra").expect("CSV codec"),
        ),
        (
            Route::Tsv,
            UnifiedCodec::tsv("cassandra", "rows").expect("TSV codec"),
        ),
    ];
    let tree_text = scalar_schema(
        "root",
        &ScalarParseOutcome::Parsed(ParsedScalar::Str(String::new())),
    );
    let tree_boolean = scalar_schema(
        "root",
        &ScalarParseOutcome::Parsed(ParsedScalar::Bool(false)),
    );
    let tree_integer = scalar_schema("root", &ScalarParseOutcome::Parsed(ParsedScalar::Int(0)));
    let tree_decimal = scalar_schema("root", &ScalarParseOutcome::Fidelity("NotANumber".into()));
    let table_text = scalar_schema(
        "rows",
        &ScalarParseOutcome::Parsed(ParsedScalar::Str(String::new())),
    );
    let table_boolean = scalar_schema(
        "rows",
        &ScalarParseOutcome::Parsed(ParsedScalar::Bool(false)),
    );
    let table_integer = scalar_schema("rows", &ScalarParseOutcome::Parsed(ParsedScalar::Int(0)));
    let table_decimal = scalar_schema("rows", &ScalarParseOutcome::Fidelity("NotANumber".into()));
    let inputs = scope::byte_strings().expect("the length-two scope fits its enumeration budget");

    for input in inputs {
        for (route, codec) in &tree_codecs {
            let expected =
                reference_parse_outcome(panproto_reference::value_io::parse(*route, &input));
            let reference_cst = panproto_reference::value_io::parse_cst(*route, &input);
            assert_eq!(
                reference_cst
                    .as_ref()
                    .map(|cst| ScalarParseOutcome::Parsed(reference_scalar(&cst.value)))
                    .unwrap_or_else(|error| reference_parse_outcome(Err(error.clone()))),
                expected,
                "reference CST parse differs from scalar parse: route={route:?}, input={input:?}"
            );
            if let Ok(cst) = &reference_cst {
                assert_eq!(cst.source, input, "route={route:?}");
            }

            let schema = select_scalar_schema(
                &expected,
                &tree_text,
                &tree_boolean,
                &tree_integer,
                &tree_decimal,
            );
            let wrapped = wrap_scalar(*route, &input);
            let production = codec.parse_wtype_preserving(&schema, &wrapped);
            let (actual, production_round_trip) = match production {
                Ok((instance, complement)) => {
                    let emitted = codec
                        .emit_wtype_preserving(&schema, &instance, &complement)
                        .unwrap_or_else(|error| {
                            panic!("CST emit failed: route={route:?}, input={input:?}: {error:?}")
                        });
                    (
                        ScalarParseOutcome::Parsed(production_tree_scalar(&instance)),
                        emitted == wrapped,
                    )
                }
                Err(error) => (production_parse_error(error), false),
            };
            assert_eq!(
                panproto_reference::value_io::round_trip_cst(*route, &input),
                production_round_trip,
                "CST round trip differs: route={route:?}, input={input:?}"
            );
            assert_eq!(
                actual, expected,
                "scalar parse differs: route={route:?}, input={input:?}"
            );
        }

        for (route, codec) in &table_codecs {
            let expected =
                reference_parse_outcome(panproto_reference::value_io::parse(*route, &input));
            let reference_cst = panproto_reference::value_io::parse_cst(*route, &input);
            assert_eq!(
                reference_cst
                    .as_ref()
                    .map(|cst| ScalarParseOutcome::Parsed(reference_scalar(&cst.value)))
                    .unwrap_or_else(|error| reference_parse_outcome(Err(error.clone()))),
                expected,
                "reference CST parse differs from scalar parse: route={route:?}, input={input:?}"
            );
            if let Ok(cst) = &reference_cst {
                assert_eq!(cst.source, input, "route={route:?}");
            }

            let schema = select_scalar_schema(
                &expected,
                &table_text,
                &table_boolean,
                &table_integer,
                &table_decimal,
            );
            let wrapped = wrap_scalar(*route, &input);
            let production = codec.parse_functor_preserving(&schema, &wrapped);
            let (actual, production_round_trip) = match production {
                Ok((instance, complement)) => {
                    let emitted = codec
                        .emit_functor_preserving(&schema, &instance, &complement)
                        .unwrap_or_else(|error| {
                            panic!("CST emit failed: route={route:?}, input={input:?}: {error:?}")
                        });
                    (
                        ScalarParseOutcome::Parsed(production_table_scalar(&instance)),
                        emitted == wrapped,
                    )
                }
                Err(error) => (production_parse_error(error), false),
            };
            assert_eq!(
                panproto_reference::value_io::round_trip_cst(*route, &input),
                production_round_trip,
                "CST round trip differs: route={route:?}, input={input:?}"
            );
            assert_eq!(
                actual, expected,
                "scalar parse differs: route={route:?}, input={input:?}"
            );
        }
    }
}

/// Law 25 on the preserving TOML production path.
#[cfg(feature = "tree-sitter")]
#[test]
fn bounded_toml_cst_numeric_spellings_match_reference() {
    let schema = one_decimal_column("root");
    let codec = UnifiedCodec::toml("cassandra").expect("TOML CST codec");
    for numeral in scope::numerals() {
        if panproto_inst::value::DecimalValue::parse(&numeral).is_err() {
            continue;
        }
        let input = format!("value = {numeral}\n");
        let (instance, complement) = codec
            .parse_wtype_preserving(&schema, input.as_bytes())
            .unwrap_or_else(|error| panic!("TOML CST parse {numeral:?}: {error:?}"));
        let emitted = codec
            .emit_wtype_preserving(&schema, &instance, &complement)
            .unwrap_or_else(|error| panic!("TOML CST emit {numeral:?}: {error:?}"));
        let emitted = std::str::from_utf8(&emitted).expect("TOML CST output is UTF-8");
        let emitted = emitted
            .lines()
            .find_map(|line| line.strip_prefix("value = "))
            .unwrap_or_else(|| panic!("missing CST scalar in {emitted:?}"));
        assert!(
            panproto_reference::toml::decimal_bigint_cst(&numeral, emitted),
            "numeral={numeral:?}"
        );
        assert!(
            panproto_reference::value_io::raw_numeric(&numeral, emitted),
            "numeral={numeral:?}"
        );
    }
}

/// Laws 28 through 30 on the preserving TSV production path.
#[cfg(feature = "tree-sitter")]
#[test]
fn bounded_tabular_cst_observables_match_reference() {
    let schema = one_text_column("rows");
    let codec = UnifiedCodec::tsv("cassandra", "rows").expect("TSV CST codec");

    for symbols in scope::sequences(&['_', 'x']) {
        let cell: String = symbols.into_iter().collect();
        let input = format!("value\n{cell}\n");
        let (instance, complement) = codec
            .parse_functor_preserving(&schema, input.as_bytes())
            .unwrap_or_else(|error| panic!("TSV CST parse {cell:?}: {error:?}"));
        let emitted = codec
            .emit_functor_preserving(&schema, &instance, &complement)
            .unwrap_or_else(|error| panic!("TSV CST emit {cell:?}: {error:?}"));
        let production_is_literal_missing =
            table_text(&instance) == Some("_") && emitted == input.as_bytes();
        assert_eq!(
            panproto_reference::tabular::missing_token_cst(&cell),
            production_is_literal_missing,
            "cell={cell:?}"
        );
    }

    for symbols in scope::sequences(&["", "x"]) {
        let line = symbols.concat();
        let input = format!("value\n{line}\n");
        let (instance, complement) = codec
            .parse_functor_preserving(&schema, input.as_bytes())
            .unwrap_or_else(|error| panic!("TSV CST empty-row parse {line:?}: {error:?}"));
        let emitted = codec
            .emit_functor_preserving(&schema, &instance, &complement)
            .unwrap_or_else(|error| panic!("TSV CST empty-row emit {line:?}: {error:?}"));
        let production_is_one_empty =
            table_text(&instance) == Some("") && emitted == input.as_bytes();
        assert_eq!(
            panproto_reference::tabular::one_column_empty_cst(1, &line),
            production_is_one_empty,
            "line={line:?}"
        );
    }

    for symbols in scope::sequences(&['"', '\t', '\r', '\n']) {
        let cell: String = symbols.into_iter().collect();
        let input = format!("value\n{cell}\n");
        // The cell is one TSV cell exactly when the document reads back as
        // one row holding it and is written back byte for byte. A document
        // that parses as something else, such as `value\n\r\n`, whose CR LF
        // ends an empty record, does not hold the cell.
        let production = codec
            .parse_functor_preserving(&schema, input.as_bytes())
            .map_err(|error| error.to_string())
            .and_then(|(instance, complement)| {
                codec
                    .emit_functor_preserving(&schema, &instance, &complement)
                    .map(|emitted| (instance, emitted))
                    .map_err(|error| error.to_string())
            });
        let production_holds_cell = production.as_ref().is_ok_and(|(instance, emitted)| {
            instance.rows("rows").len() == 1
                && table_text(instance) == Some(cell.as_str())
                && emitted == input.as_bytes()
        });
        assert_eq!(
            panproto_reference::tabular::tsv_quote_cst(&cell),
            production_holds_cell,
            "cell={cell:?}, result={production:?}"
        );
    }
}

/// Laws 1, 2, 4 and their typed refusal side over representative public JSON
/// production paths. Both sides receive the same scalar after removing only
/// the schema-required object wrapper.
#[test]
fn json_scalar_successes_and_typed_refusals_match_reference() {
    let schema = one_text_column("root");
    for text in ["", "plain", "x\\ny", "\"", "_"] {
        let reference_value = ReferenceValue::Str(text.to_owned());
        let reference_bytes = panproto_reference::value_io::emit(Route::Json, &reference_value)
            .test_unwrap("reference JSON string emit");
        assert!(panproto_reference::value_io::round_trip(
            Route::Json,
            &reference_value
        ));

        let instance = one_tree_value("root", FieldPresence::Present(Value::Str(text.to_owned())));
        let production = emit_json_bytes(&schema, &instance, "cassandra")
            .test_unwrap("production JSON string emit");
        let scalar = json_wrapped_scalar(&production);
        assert_eq!(scalar, reference_bytes, "text={text:?}");

        let reference_parsed = panproto_reference::value_io::parse(Route::Json, &scalar)
            .test_unwrap("reference JSON string parse");
        let production_parsed = parse_json_bytes(&schema, "root", &production, "cassandra")
            .test_unwrap("production JSON string parse");
        assert_eq!(reference_parsed, reference_value, "text={text:?}");
        assert_eq!(
            tree_value(&production_parsed, "value"),
            &FieldPresence::Present(Value::Str(text.to_owned())),
            "text={text:?}"
        );
    }

    let reference_refusal = panproto_reference::value_io::emit(
        Route::Json,
        &ReferenceValue::Float(panproto_reference::float::CANONICAL_NAN_BITS),
    );
    let production_refusal = emit_json_bytes(
        &schema,
        &one_tree_value("root", FieldPresence::Present(Value::Float(f64::NAN))),
        "cassandra",
    );
    assert_eq!(reference_refusal, Err(ValueIoError::NonFinite));
    assert!(matches!(
        production_refusal,
        Err(EmitInstanceError::Value(ref error))
            if error.reason == EmitValueReason::NonFinite
    ));
}

/// Law 5 over the page's complete temporal lexical product for the five
/// canonical emitters in the phase-3 contract: JSON, TOML, XML, CSV and TSV.
/// The feature-gated YAML CST emitter belongs to the final-gate route proof.
/// A successfully parsed temporal is offered to both route predicates;
/// malformed lexemes are refusals on both sides.
#[test]
#[allow(clippy::too_many_lines)]
fn temporal_route_representability_matches_public_emitters() {
    let json_schemas = [
        (TemporalForm::Date, one_temporal_column("root", "date")),
        (TemporalForm::Time, one_temporal_column("root", "time")),
        (
            TemporalForm::DateTime,
            one_temporal_column("root", "timestamp"),
        ),
    ];
    for lexeme in scope::temporal_lexemes() {
        let reference_value = panproto_reference::temporal::parse_union(&lexeme);
        let production_value = Temporal::parse_union(&lexeme);
        assert_eq!(
            reference_value.is_ok(),
            production_value.is_ok(),
            "parse classification differs for {lexeme:?}"
        );
        let (Ok(reference_value), Ok(production_value)) = (reference_value, production_value)
        else {
            continue;
        };
        let form = production_value
            .lexical_form()
            .expect("parsed temporal has a lexical descriptor")
            .kind;
        let schema = json_schemas
            .iter()
            .find_map(|(candidate, schema)| (*candidate == form).then_some(schema))
            .expect("all temporal forms have schemas");
        let instance = one_tree_value(
            "root",
            FieldPresence::Present(Value::Temporal(production_value.clone())),
        );
        let xml_instance = one_xml_tree_value(
            "root",
            FieldPresence::Present(Value::Temporal(production_value.clone())),
        );
        let table_schema = one_temporal_column(
            "rows",
            match form {
                TemporalForm::Date => "date",
                TemporalForm::Time => "time",
                TemporalForm::DateTime => "timestamp",
            },
        );
        let table_instance = one_table_scalar(Value::Temporal(production_value.clone()));

        let json_reference = panproto_reference::value_io::temporal(Route::Json, &reference_value);
        let json_production = production_representable(
            emit_json_bytes(schema, &instance, "cassandra"),
            &format!("JSON temporal {lexeme:?}"),
        );
        assert_eq!(
            json_reference, json_production,
            "JSON temporal {lexeme:?}: reference={reference_value:?}, production={production_value:?}"
        );
        let toml_reference = panproto_reference::value_io::temporal(Route::Toml, &reference_value);
        let toml_production = production_representable(
            emit_toml_bytes(schema, &instance, "cassandra"),
            &format!("TOML temporal {lexeme:?}"),
        );
        assert_eq!(
            toml_reference, toml_production,
            "TOML temporal {lexeme:?}: reference={reference_value:?}, production={production_value:?}"
        );
        let xml_reference = panproto_reference::value_io::temporal(Route::Xml, &reference_value);
        let xml_production = production_representable(
            panproto_io::xml_pathway::emit_xml_bytes(schema, &xml_instance, "cassandra"),
            &format!("XML temporal {lexeme:?}"),
        );
        assert_eq!(
            xml_reference, xml_production,
            "XML temporal {lexeme:?}: reference={reference_value:?}, production={production_value:?}"
        );
        for (route, delimiter) in [(Route::Csv, b','), (Route::Tsv, b'\t')] {
            let production = production_representable(
                emit_tsv_with_schema(
                    &table_schema,
                    &table_instance,
                    "cassandra",
                    "rows",
                    delimiter,
                ),
                &format!("{route:?} temporal {lexeme:?}"),
            );
            assert_eq!(
                panproto_reference::value_io::temporal(route, &reference_value),
                production,
                "{route:?} temporal {lexeme:?}: reference={reference_value:?}, production={production_value:?}"
            );
        }
    }
}

/// Law 18 on the production parse path. Element names: a schema edge in
/// Clark notation selects an element of that namespace and local name under
/// either of two prefixes. Attribute names: two attributes of one start tag,
/// each unprefixed or under prefix `p` or `q` with both prefixes bound over
/// the namespace scope, collide exactly when the reference expanded names are
/// equal; a collision is the typed `DuplicateMember` naming `{uri}local` at
/// the second name token, and otherwise both are stored under their Clark
/// names.
#[test]
#[allow(clippy::too_many_lines)]
fn bounded_xml_expanded_names_match_reference() {
    let protocol = panproto_protocols::database::cassandra::protocol();
    #[cfg(feature = "tree-sitter")]
    let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
    let clark = |(namespace, local): &(String, String)| {
        if namespace.is_empty() {
            local.clone()
        } else {
            format!("{{{namespace}}}{local}")
        }
    };
    let mut element_cases = 0_usize;
    for namespace_parts in scope::sequences(&["urn:a", "urn:b"]) {
        let namespace = namespace_parts.concat();
        for local_parts in scope::sequences(&["a", "b"]) {
            let local = local_parts.concat();
            if local.is_empty() {
                continue;
            }
            let expanded = panproto_reference::value_io::xml_expanded_name(&namespace, &local);
            let schema = SchemaBuilder::new(&protocol)
                .vertex("doc", "table", None)
                .test_unwrap("XML expanded-name root")
                .vertex("value", "text", None)
                .test_unwrap("XML expanded-name value")
                .edge("doc", "value", "prop", Some(clark(&expanded).as_str()))
                .test_unwrap("XML expanded-name edge")
                .build()
                .test_unwrap("XML expanded-name schema");
            let inputs = if namespace.is_empty() {
                vec![format!("<doc><{local}>x</{local}></doc>")]
            } else {
                ["p", "q"]
                    .iter()
                    .map(|prefix| {
                        format!(
                            "<doc xmlns:{prefix}=\"{namespace}\"><{prefix}:{local}>x</{prefix}:{local}></doc>"
                        )
                    })
                    .collect()
            };
            for input in inputs {
                let parsed = parse_xml_bytes(&schema, input.as_bytes(), "cassandra")
                    .test_unwrap("canonical XML expanded-name parse");
                assert_eq!(
                    tree_value(&parsed, "value"),
                    &FieldPresence::Present(Value::Str("x".to_owned())),
                    "canonical input={input}"
                );
                element_cases += 1;
                #[cfg(feature = "tree-sitter")]
                {
                    let (parsed, _) = codec
                        .parse_wtype_preserving(&schema, input.as_bytes())
                        .test_unwrap("CST XML expanded-name parse");
                    let value = parsed
                        .nodes
                        .values()
                        .find(|node| node.anchor.as_ref() == "value")
                        .and_then(|node| node.value.as_ref());
                    assert_eq!(
                        value,
                        Some(&FieldPresence::Present(Value::Str("x".to_owned()))),
                        "CST input={input}, anchors={:?}",
                        parsed
                            .nodes
                            .values()
                            .map(|node| node.anchor.to_string())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
    // Local names a, b, aa, ab, ba, bb (six) under the empty namespace once
    // and under each of the six nonempty namespaces with two prefixes.
    assert_eq!(element_cases, 6 + 6 * 6 * 2);

    let schema = one_text_column("root");
    let attributes = [
        (None, "a"),
        (None, "b"),
        (Some("p"), "a"),
        (Some("p"), "b"),
        (Some("q"), "a"),
        (Some("q"), "b"),
    ];
    let mut outcomes = [0_usize; 2];
    for p_namespace in ["urn:a", "urn:b"] {
        for q_namespace in ["urn:a", "urn:b"] {
            for first in attributes {
                for second in attributes {
                    let expanded = |(prefix, local): (Option<&str>, &str)| {
                        let namespace = match prefix {
                            None => "",
                            Some("p") => p_namespace,
                            Some(_) => q_namespace,
                        };
                        panproto_reference::value_io::xml_expanded_name(namespace, local)
                    };
                    let spelled = |(prefix, local): (Option<&str>, &str)| {
                        prefix
                            .map_or_else(|| local.to_owned(), |prefix| format!("{prefix}:{local}"))
                    };
                    let head = format!(
                        "<root xmlns:p=\"{p_namespace}\" xmlns:q=\"{q_namespace}\" {}=\"1\" ",
                        spelled(first)
                    );
                    let second_offset = u64::try_from(head.len()).expect("offset fits u64");
                    let input = format!("{head}{}=\"2\"><value>x</value></root>", spelled(second));
                    let production = parse_xml_bytes(&schema, input.as_bytes(), "cassandra");
                    let (first_name, second_name) = (expanded(first), expanded(second));
                    if first_name == second_name {
                        outcomes[0] += 1;
                        assert!(
                            matches!(
                                production,
                                Err(ParseInstanceError::DuplicateMember {
                                    ref protocol,
                                    ref name,
                                    byte_offset: Some(offset),
                                }) if protocol == "cassandra"
                                    && *name == clark(&second_name)
                                    && offset == second_offset
                            ),
                            "input={input}, result={production:?}"
                        );
                    } else {
                        outcomes[1] += 1;
                        let parsed = production.test_unwrap("distinct expanded attribute names");
                        let root = parsed.node(parsed.root).expect("root element");
                        assert_eq!(
                            root.extra_fields.get(&clark(&first_name)),
                            Some(&Value::Str("1".to_owned())),
                            "input={input}"
                        );
                        assert_eq!(
                            root.extra_fields.get(&clark(&second_name)),
                            Some(&Value::Str("2".to_owned())),
                            "input={input}"
                        );
                    }
                }
            }
        }
    }
    // For each of the four bindings, a pair collides when both are
    // unprefixed with one local name (2 pairs), or both are prefixed with one
    // local name and namespaces that agree: 2 locals times the prefix pairs
    // (p,p), (q,q), and (p,q) and (q,p) when p and q share a namespace. So
    // 2 + 2*2 = 6 collisions when they differ and 2 + 2*4 = 10 when they
    // agree: 6 + 10 + 10 + 6 = 32 of 4 * 36 = 144 documents.
    assert_eq!(outcomes, [32, 112]);
}

/// Law 12 over all words of length at most two over two declared entities,
/// plus predefined/numeric references and every typed refusal class. Law 26
/// additionally requires a preserving production route and is a final-gate
/// residual rather than a claim of this canonical test.
#[test]
#[allow(clippy::too_many_lines)]
fn bounded_xml_entity_expansion_matches_reference() {
    let schema = one_text_column("root");
    let empty = BTreeMap::new();
    for (source, expected) in [
        ("&lt;", "<"),
        ("&amp;", "&"),
        ("&#65;", "A"),
        ("&#x1f600;", "😀"),
    ] {
        assert_eq!(
            panproto_reference::value_io::xml_entity(source, &empty),
            Ok(expected.to_owned())
        );
        assert_eq!(
            panproto_reference::xml::expand_entities(source, &empty),
            Ok(expected.to_owned())
        );
        assert_eq!(expand_xml_entities(source, &empty), Ok(expected.to_owned()));
        let input = format!("<root><value>{source}</value></root>");
        let parsed =
            parse_xml_bytes(&schema, input.as_bytes(), "cassandra").test_unwrap("XML entity parse");
        assert_eq!(
            tree_value(&parsed, "value"),
            &FieldPresence::Present(Value::Str(expected.to_owned())),
            "source={source:?}"
        );
    }

    let replacement_choices = [None, Some("x"), Some("&a;"), Some("&b;"), Some("&amp;")];
    let source_atoms = ["&a;", "&b;"];
    for a in replacement_choices {
        for b in replacement_choices {
            let mut declarations = BTreeMap::new();
            if let Some(replacement) = a {
                declarations.insert("a".to_owned(), replacement.to_owned());
            }
            if let Some(replacement) = b {
                declarations.insert("b".to_owned(), replacement.to_owned());
            }
            for atoms in scope::sequences(&source_atoms) {
                let source = atoms.concat();
                let production =
                    production_entity_outcome(expand_xml_entities(&source, &declarations));
                let direct = reference_entity_outcome(panproto_reference::value_io::xml_entity(
                    &source,
                    &declarations,
                ));
                let public = reference_entity_outcome(panproto_reference::xml::expand_entities(
                    &source,
                    &declarations,
                ));
                assert_eq!(
                    production, direct,
                    "source={source:?}, decls={declarations:?}"
                );
                assert_eq!(
                    production, public,
                    "source={source:?}, decls={declarations:?}"
                );

                let declarations_text = xml_entity_declarations(&declarations);
                let wrapped = format!("L{source}R");
                let document = format!(
                    "<!DOCTYPE root [{declarations_text}]><root><value>{wrapped}</value></root>"
                );
                let document_production = production_xml_document_entity_outcome(parse_xml_bytes(
                    &schema,
                    document.as_bytes(),
                    "cassandra",
                ));
                let document_reference = reference_entity_outcome(
                    panproto_reference::value_io::xml_entity(&wrapped, &declarations),
                );
                assert_eq!(
                    document_production, document_reference,
                    "document source={source:?}, decls={declarations:?}"
                );
            }
        }
    }

    for source in ["&missing;", "&unfinished", "&#0;", "&#x110000;"] {
        assert_eq!(
            production_entity_outcome(expand_xml_entities(source, &empty)),
            reference_entity_outcome(panproto_reference::value_io::xml_entity(source, &empty)),
            "source={source:?}"
        );
    }

    let mut nested = BTreeMap::new();
    for depth in 0..=MAX_XML_ENTITY_DEPTH {
        nested.insert(format!("e{depth}"), format!("&e{};", depth + 1));
    }
    nested.insert(format!("e{}", MAX_XML_ENTITY_DEPTH + 1), "x".to_owned());
    assert_eq!(
        production_entity_outcome(expand_xml_entities("&e0;", &nested)),
        reference_entity_outcome(panproto_reference::value_io::xml_entity("&e0;", &nested))
    );
    let nested_declarations = xml_entity_declarations(&nested);
    let nested_document =
        format!("<!DOCTYPE root [{nested_declarations}]><root><value>L&e0;R</value></root>");
    assert_eq!(
        production_xml_document_entity_outcome(parse_xml_bytes(
            &schema,
            nested_document.as_bytes(),
            "cassandra",
        )),
        reference_entity_outcome(panproto_reference::value_io::xml_entity("L&e0;R", &nested))
    );

    let over_budget = BTreeMap::from([(
        "large".to_owned(),
        "x".repeat(MAX_XML_ENTITY_EXPANSION_BYTES + 1),
    )]);
    assert_eq!(
        production_entity_outcome(expand_xml_entities("&large;", &over_budget)),
        reference_entity_outcome(panproto_reference::value_io::xml_entity(
            "&large;",
            &over_budget
        ))
    );
    let large_replacement = over_budget.get("large").expect("large declaration");
    let large_document = format!(
        "<!DOCTYPE root [<!ENTITY large \"{large_replacement}\">]><root><value>&large;</value></root>"
    );
    assert_eq!(
        production_xml_document_entity_outcome(parse_xml_bytes(
            &schema,
            large_document.as_bytes(),
            "cassandra",
        )),
        reference_entity_outcome(panproto_reference::value_io::xml_entity(
            "&large;",
            &over_budget
        ))
    );

    // XML 1.0's CharRef admits only unsigned ASCII digits of its radix.
    for source in [
        "&#+65;", "&#x+41;", "&#-65;", "&# 65;", "&#X41;", "&#x;", "&#;", "&#65 ;",
    ] {
        let reference =
            reference_entity_outcome(panproto_reference::value_io::xml_entity(source, &empty));
        assert_eq!(reference, EntityOutcome::Malformed, "source={source:?}");
        assert_eq!(
            production_entity_outcome(expand_xml_entities(source, &empty)),
            reference,
            "source={source:?}"
        );
    }
    for source in ["&#+65;", "&#x+41;", "&#-65;"] {
        let wrapped = format!("L{source}R");
        let reference =
            reference_entity_outcome(panproto_reference::value_io::xml_entity(&wrapped, &empty));
        for document in [
            format!("<root><value>{wrapped}</value></root>"),
            format!("<!DOCTYPE root [<!ENTITY s \"{source}\">]><root><value>L&s;R</value></root>"),
        ] {
            assert_eq!(
                production_xml_document_entity_outcome(parse_xml_bytes(
                    &schema,
                    document.as_bytes(),
                    "cassandra",
                )),
                reference,
                "document={document:?}"
            );
        }
    }

    // The admitted side of both bounds. A chain of n declarations, each but
    // the last referring to the next, nests n references, so a chain of
    // exactly MAX_XML_ENTITY_DEPTH declarations is admitted and one more is
    // refused; a replacement of exactly MAX_XML_ENTITY_EXPANSION_BYTES bytes
    // is admitted.
    let chain = |length: usize| {
        let mut declarations = BTreeMap::new();
        for index in 0..length {
            let replacement = if index + 1 == length {
                "x".to_owned()
            } else {
                format!("&e{};", index + 1)
            };
            declarations.insert(format!("e{index}"), replacement);
        }
        declarations
    };
    for (length, expected) in [
        (
            MAX_XML_ENTITY_DEPTH,
            EntityOutcome::Expanded("LxR".to_owned()),
        ),
        (MAX_XML_ENTITY_DEPTH + 1, EntityOutcome::Depth),
    ] {
        let declarations = chain(length);
        let reference = reference_entity_outcome(panproto_reference::value_io::xml_entity(
            "L&e0;R",
            &declarations,
        ));
        assert_eq!(reference, expected, "chain length {length}");
        assert_eq!(
            production_entity_outcome(expand_xml_entities("L&e0;R", &declarations)),
            reference,
            "chain length {length}"
        );
        let document = format!(
            "<!DOCTYPE root [{}]><root><value>L&e0;R</value></root>",
            xml_entity_declarations(&declarations)
        );
        assert_eq!(
            production_xml_document_entity_outcome(parse_xml_bytes(
                &schema,
                document.as_bytes(),
                "cassandra",
            )),
            reference,
            "document chain length {length}"
        );
    }
    let at_budget = BTreeMap::from([(
        "large".to_owned(),
        "x".repeat(MAX_XML_ENTITY_EXPANSION_BYTES),
    )]);
    let reference = reference_entity_outcome(panproto_reference::value_io::xml_entity(
        "&large;", &at_budget,
    ));
    assert_eq!(
        reference,
        EntityOutcome::Expanded("x".repeat(MAX_XML_ENTITY_EXPANSION_BYTES))
    );
    assert_eq!(
        production_entity_outcome(expand_xml_entities("&large;", &at_budget)),
        reference
    );
    let at_budget_document = format!(
        "<!DOCTYPE root [<!ENTITY large \"{}\">]><root><value>&large;</value></root>",
        at_budget.get("large").expect("large declaration")
    );
    assert_eq!(
        production_xml_document_entity_outcome(parse_xml_bytes(
            &schema,
            at_budget_document.as_bytes(),
            "cassandra",
        )),
        reference
    );

    for (input, feature, needle) in [
        (
            "<!DOCTYPE root SYSTEM \"external.dtd\"><root/>",
            "external XML DTD subset",
            "SYSTEM",
        ),
        (
            "<!DOCTYPE root [<!ENTITY ext SYSTEM \"ext.ent\">]><root/>",
            "external XML entity \"ext\"",
            "<!ENTITY ext",
        ),
        (
            "<!DOCTYPE root [<!ENTITY % p \"x\">]><root/>",
            "XML parameter entity construct \"p\"",
            "<!ENTITY %",
        ),
        (
            "<!DOCTYPE root [%p;]><root/>",
            "XML parameter entity construct \"p\"",
            "%p;",
        ),
    ] {
        let expected_offset = input
            .find(needle)
            .and_then(|offset| u64::try_from(offset).ok());
        assert!(matches!(
            parse_xml_bytes(&schema, input.as_bytes(), "cassandra"),
            Err(ParseInstanceError::UnsupportedFeature {
                protocol,
                feature: actual_feature,
                byte_offset,
            }) if protocol == "cassandra"
                && actual_feature == feature
                && byte_offset == expected_offset
        ));
    }
}

/// Laws 1 and 2 over every one-scalar string on the five always-compiled
/// canonical writers. JSON, XML, CSV and TSV compare the production scalar
/// token, or its typed refusal, with the reference emitter byte for byte, and
/// compare the production round trip through the matching reader with the
/// reference `round_trip`. The TOML writer chooses among basic, literal and
/// multi-line string forms, so its token is instead read by the reference
/// TOML grammar, which models single-line forms only. The counted exceptions
/// are derived independently in the page's certificate notes.
#[test]
fn every_single_scalar_string_matches_reference_on_the_canonical_routes() {
    let tree = one_text_column("root");
    let table = one_text_column("rows");
    let scalars: Vec<char> = scope::single_scalars().collect();
    // U+0000 to U+10FFFF less the 2,048 surrogates.
    assert_eq!(scalars.len(), 0x11_0000 - 0x800);
    let tally = parallel_tally(&scalars, |&character| {
        let mut counts = [0_usize; 5];
        let text = character.to_string();
        let production_value = Value::Str(text.clone());
        let reference_value = ReferenceValue::Str(text);
        for (slot, route) in [
            (0, Route::Json),
            (1, Route::Xml),
            (2, Route::Tsv),
            (3, Route::Csv),
        ] {
            let schema = if matches!(route, Route::Csv | Route::Tsv) {
                &table
            } else {
                &tree
            };
            let (production, reread) = production_scalar(route, schema, &production_value);
            let expected = reference_emission(
                panproto_reference::value_io::emit(route, &reference_value),
                "single scalar",
            );
            assert_eq!(
                production,
                expected,
                "route={route:?}, scalar=U+{:04X}",
                u32::from(character)
            );
            if !matches!(production, ScalarEmission::Emitted(_)) {
                counts[slot] += 1;
            }
            assert_eq!(
                panproto_reference::value_io::round_trip(route, &reference_value),
                reread.is_some_and(|value| same_value(&value, &production_value)),
                "round trip: route={route:?}, scalar=U+{:04X}",
                u32::from(character)
            );
        }
        let (production, _) = production_scalar(Route::Toml, &tree, &production_value);
        let ScalarEmission::Emitted(token) = production else {
            panic!(
                "TOML refused the string U+{:04X}: {production:?}",
                u32::from(character)
            );
        };
        assert!(
            panproto_reference::value_io::round_trip(Route::Toml, &reference_value),
            "reference TOML round trip U+{:04X}",
            u32::from(character)
        );
        if token.contains(&b'\n') {
            counts[4] += 1;
        } else {
            assert_eq!(
                panproto_reference::value_io::parse(Route::Toml, &token),
                Ok(reference_value),
                "TOML token {:?} for U+{:04X}",
                String::from_utf8_lossy(&token),
                u32::from(character)
            );
        }
        counts
    });
    // JSON refuses none; XML refuses the 31 scalars outside XML 1.0 `Char`
    // (29 C0 controls other than TAB, LF and CR, and U+FFFE and U+FFFF); TSV
    // refuses TAB, LF and CR; CSV, which writes no quoted field, refuses
    // comma, quote, CR and LF; and TOML writes only U+000A in a multi-line
    // form.
    assert_eq!(tally, [0, 31, 3, 4, 1]);

    // The one string of length zero, which XML writes as an empty element
    // and CSV and TSV as an empty line under the one-column header.
    let empty = Value::Str(String::new());
    let reference_empty = ReferenceValue::Str(String::new());
    for route in [Route::Json, Route::Xml, Route::Csv, Route::Tsv] {
        let schema = if matches!(route, Route::Csv | Route::Tsv) {
            &table
        } else {
            &tree
        };
        let (production, reread) = production_scalar(route, schema, &empty);
        let expected = reference_emission(
            panproto_reference::value_io::emit(route, &reference_empty),
            "empty string",
        );
        assert_eq!(production, expected, "route={route:?}");
        assert!(panproto_reference::value_io::round_trip(
            route,
            &reference_empty
        ));
        assert!(
            reread.is_some_and(|value| same_value(&value, &empty)),
            "route={route:?}"
        );
    }
    let (production, _) = production_scalar(Route::Toml, &tree, &empty);
    let ScalarEmission::Emitted(token) = production else {
        panic!("TOML refused the empty string: {production:?}");
    };
    assert_eq!(
        panproto_reference::value_io::parse(Route::Toml, &token),
        Ok(reference_empty)
    );
}

/// Sum `check` over `items` on the available worker threads.
fn parallel_tally<T: Sync, const N: usize>(
    items: &[T],
    check: impl Fn(&T) -> [usize; N] + Sync,
) -> [usize; N] {
    let workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .clamp(1, 8);
    let chunk = items.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| {
                scope.spawn(|| {
                    part.iter().fold([0_usize; N], |mut total, item| {
                        for (sum, count) in total.iter_mut().zip(check(item)) {
                            *sum += count;
                        }
                        total
                    })
                })
            })
            .collect();
        handles.into_iter().fold([0_usize; N], |mut total, handle| {
            for (sum, count) in total.iter_mut().zip(handle.join().expect("worker thread")) {
                *sum += count;
            }
            total
        })
    })
}

/// Laws 1 and 2 over exact integers at and beyond the `i64` boundary, the
/// special decimals and the non-finite binary floats, on the five
/// always-compiled canonical writers. Each production token or typed refusal
/// equals the reference emitter's, and each production round trip through the
/// matching reader agrees with the reference `round_trip`.
#[test]
fn exact_integer_and_special_numeric_scalars_match_reference() {
    let mut cases: Vec<(&str, Value, ReferenceValue)> = Vec::new();
    for lexeme in scope::exact_integer_lexemes() {
        let production = coerce(
            &Lexeme::new(lexeme, LexClass::Number),
            &ValueSpec::new(ScalarKind::Any),
        )
        .unwrap_or_else(|error| panic!("production integer {lexeme}: {error:?}"));
        let reference = panproto_reference::exact::parse_numeral(lexeme)
            .and_then(|numeral| panproto_reference::exact::integer_of_numeral(&numeral))
            .map(|integer| match integer {
                panproto_reference::exact::ExactInteger::Int(value) => ReferenceValue::Int(value),
                panproto_reference::exact::ExactInteger::Big(value) => {
                    ReferenceValue::BigInt(value)
                }
            })
            .unwrap_or_else(|| panic!("reference integer {lexeme}"));
        let kind = match (&production, &reference) {
            (Value::Int(_), ReferenceValue::Int(_)) => "bigint",
            (Value::BigInt(_), ReferenceValue::BigInt(_)) => "varint",
            other => panic!("integer carriers disagree for {lexeme}: {other:?}"),
        };
        cases.push((kind, production, reference));
    }
    for lexeme in scope::special_decimal_lexemes() {
        let production = DecimalValue::parse(lexeme)
            .unwrap_or_else(|error| panic!("production special decimal {lexeme}: {error:?}"));
        let reference = panproto_reference::exact::parse_decimal(lexeme)
            .unwrap_or_else(|| panic!("reference special decimal {lexeme}"));
        cases.push((
            "decimal",
            Value::Decimal(production),
            ReferenceValue::Decimal(reference),
        ));
    }
    for bits in scope::nonfinite_float_bits() {
        cases.push((
            "double",
            Value::Float(f64::from_bits(bits)),
            ReferenceValue::Float(bits),
        ));
    }
    // 13 integers, 10 special decimals and 6 non-finite floats.
    assert_eq!(cases.len(), 29);
    let mut compared = 0_usize;
    for (kind, production_value, reference_value) in &cases {
        let tree = one_kind_column("root", "value", kind);
        let table = one_kind_column("rows", "value", kind);
        for route in CANONICAL_EMIT_ROUTES {
            let schema = if matches!(route, Route::Csv | Route::Tsv) {
                &table
            } else {
                &tree
            };
            let (production, reread) = production_scalar(route, schema, production_value);
            let expected = reference_emission(
                panproto_reference::value_io::emit(route, reference_value),
                &format!("{route:?} {reference_value:?}"),
            );
            assert_eq!(
                production, expected,
                "route={route:?}, value={production_value:?}"
            );
            if route != Route::Toml {
                assert_eq!(
                    panproto_reference::value_io::round_trip(route, reference_value),
                    reread.is_some_and(|value| same_value(&value, production_value)),
                    "round trip: route={route:?}, value={production_value:?}"
                );
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 29 * 5);
}

/// Law 10 on the canonical JSON, CSV and TSV readers: a leading UTF-8 byte
/// order mark is read past, offsets still count from byte 0, and the canonical
/// writer emits UTF-8 without a mark.
#[test]
fn canonical_json_and_tabular_routes_read_past_a_leading_byte_order_mark() {
    const MARK: &[u8] = &[0xEF, 0xBB, 0xBF];
    let schema = one_text_column("root");
    let source = [MARK, br#"{"value":"x"}"#.as_slice()].concat();
    let parsed = parse_json_bytes(&schema, "root", &source, "cassandra")
        .test_unwrap("JSON after a byte order mark");
    assert_eq!(
        tree_value(&parsed, "value"),
        &FieldPresence::Present(Value::Str("x".to_owned()))
    );
    let emitted = emit_json_bytes(&schema, &parsed, "cassandra").test_unwrap("JSON emit");
    assert!(panproto_reference::value_io::encoding(&emitted));
    assert_eq!(emitted, br#"{"value":"x"}"#);
    // `{"value":` is nine bytes, so the number token starts at 3 + 9.
    let integer = one_kind_column("root", "value", "int");
    let refused = [MARK, br#"{"value":1.5}"#.as_slice()].concat();
    assert!(matches!(
        parse_json_bytes(&integer, "root", &refused, "cassandra"),
        Err(ParseInstanceError::Fidelity(ref errors))
            if errors.len() == 1 && errors[0].site.byte_offset == Some(12)
    ));

    let table = two_text_columns("rows", "a", "b");
    for (codec, text) in [
        (DelimitedRoute::TSV, b"a\tb\n1\t2\n".as_slice()),
        (DelimitedRoute::CSV, b"a,b\n1,2\n".as_slice()),
    ] {
        let parsed = codec
            .parse_functor(&table, &[MARK, text].concat())
            .test_unwrap("delimited text after a byte order mark");
        let rows = parsed.rows("rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.get("a"), Some(&Value::Str("1".to_owned())));
        assert_eq!(rows[0].1.get("b"), Some(&Value::Str("2".to_owned())));
        assert_eq!(rows[0].1.len(), 2);
        let emitted = codec
            .emit_functor(&table, &parsed)
            .test_unwrap("delimited emit");
        assert!(panproto_reference::value_io::encoding(&emitted));
        assert_eq!(emitted, text);
    }
}

/// Law 19's string classes, exhaustively: every one-scalar string as both the
/// link and the media type of a blob is written by the production JSON route
/// exactly as the independent layout writes it, and reads back as the same
/// blob. This discharges the escape-set agreement of the route's JSON string
/// writer with the page's rule over every scalar.
#[test]
fn every_single_scalar_blob_field_matches_the_reference_layout() {
    let protocol = panproto_schema::Protocol {
        name: "reference-value-io".into(),
        schema_theory: "ThReferenceValueIoSchema".into(),
        instance_theory: "ThReferenceValueIoInstance".into(),
        obj_kinds: vec!["object".into()],
        ..Default::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .test_unwrap("blob root vertex")
        .build()
        .test_unwrap("blob schema");
    let scalars: Vec<char> = scope::single_scalars().collect();
    assert_eq!(scalars.len(), 0x11_0000 - 0x800);
    let [checked] = parallel_tally(&scalars, |&character| {
        let text = character.to_string();
        let blob = Value::Blob {
            ref_: text.clone(),
            mime: text.clone(),
            size: 0,
        };
        let mut root = Node::new(0, "root");
        root.extra_fields.insert("blob".to_owned(), blob.clone());
        let instance = WInstance::new(
            HashMap::from([(0, root)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let emitted = emit_json_bytes(&schema, &instance, "reference-value-io")
            .test_unwrap("canonical blob JSON");
        let expected = [
            br#"{"blob":"#.as_slice(),
            &panproto_reference::value_io::blob_writer(&text, &text, 0),
            b"}",
        ]
        .concat();
        assert_eq!(emitted, expected, "scalar=U+{:04X}", u32::from(character));
        let reparsed = parse_json_bytes(&schema, "root", &emitted, "reference-value-io")
            .test_unwrap("canonical blob JSON reparses");
        assert_eq!(
            reparsed
                .node(reparsed.root)
                .and_then(|node| node.extra_fields.get("blob")),
            Some(&blob),
            "scalar=U+{:04X}",
            u32::from(character)
        );
        [1]
    });
    assert_eq!(checked, 1_112_064);
}
