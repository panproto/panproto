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
use panproto_inst::value::{FieldPresence, Temporal, TemporalForm, Value};
use panproto_inst::{FInstance, WInstance};
use panproto_io::byte_tabular::ByteTabularCodec;
use panproto_io::error::{EmitInstanceError, EmitValueReason, ParseInstanceError};
use panproto_io::json_pathway::{emit_json_bytes, parse_json_bytes};
use panproto_io::tabular_pathway::{emit_tsv_with_schema, parse_tsv};
use panproto_io::toml_pathway::emit_toml_bytes;
#[cfg(feature = "tree-sitter")]
use panproto_io::unified_codec::UnifiedCodec;
use panproto_io::xml_pathway::{
    MAX_XML_ENTITY_DEPTH, MAX_XML_ENTITY_EXPANSION_BYTES, XmlEntityError, emit_xml_bytes,
    expand_xml_entities, parse_xml_bytes, xml_expanded_name,
};
use panproto_reference::enumerate::value_io as scope;
use panproto_reference::value::Value as ReferenceValue;
use panproto_reference::value_io::{Route, ValueIoError};
use panproto_schema::{Edge, Schema, SchemaBuilder};

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

fn changed(bytes: &[u8]) -> Vec<u8> {
    let mut changed = bytes.to_vec();
    if let Some(first) = changed.first_mut() {
        *first = first.wrapping_add(1);
    } else {
        changed.push(0);
    }
    changed
}

fn utf16_xml(text: &str, little_endian: bool) -> Vec<u8> {
    let mut bytes = if little_endian {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        let encoded = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&encoded);
    }
    bytes
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
        Err(ValueIoError::NotRepresentable) => {
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
        Err(EmitInstanceError::Value(error))
            if error.reason == EmitValueReason::NotRepresentable =>
        {
            ScalarEmission::NotRepresentable
        }
        Err(error) => panic!("unexpected production emission failure for {context}: {error:?}"),
    }
}

fn reference_emission(result: Result<Vec<u8>, ValueIoError>, context: &str) -> ScalarEmission {
    match result {
        Ok(bytes) => ScalarEmission::Emitted(bytes),
        Err(ValueIoError::NotRepresentable) => ScalarEmission::NotRepresentable,
        Err(error) => panic!("unexpected reference emission failure for {context}: {error:?}"),
    }
}

/// Laws 3, 6 and 19 over the declared complete byte-string scope. The
/// byte-tabular production codec is total over bytes, so no UTF-8 precondition
/// narrows this differential. XML encoding laws use the actual XML route in a
/// separate consumer; this generic equality witness does not discharge any
/// CST or XML-encoding law.
#[test]
fn bounded_byte_preservation_observables_match_reference() {
    let codec = ByteTabularCodec::new("reference-value-io", "rows", b'|', None);
    let inputs = scope::byte_strings().expect("the length-two scope fits its enumeration budget");

    for input in inputs {
        let (instance, complement) = codec.parse(&input).test_unwrap("byte parse");
        let emitted = codec.emit(&instance, &complement).test_unwrap("byte emit");
        let altered = changed(&emitted);

        assert_eq!(
            panproto_reference::value_io::preserve(&input, &emitted),
            input == emitted
        );
        assert_eq!(
            panproto_reference::value_io::bytes(&input, &emitted),
            input == emitted
        );
        assert_eq!(
            panproto_reference::value_io::blob_writer(&input, &emitted),
            input == emitted
        );

        assert_eq!(
            panproto_reference::value_io::preserve(&input, &altered),
            input == altered
        );
        assert_eq!(
            panproto_reference::value_io::bytes(&input, &altered),
            input == altered
        );
        assert_eq!(
            panproto_reference::value_io::blob_writer(&input, &altered),
            input == altered
        );
    }
}

/// Laws 10 and 13 over the XML encodings required by C55. Each admitted
/// source is decoded through the public canonical route before its emitted
/// bytes are compared by both independent byte-identity predicates. The
/// refusal cases independently derive their first offending source byte.
#[test]
fn canonical_xml_encodings_and_refusals_match_reference_contract() {
    let schema = one_text_column("root");
    let utf16_text = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><root><value>x</value></root>";
    let mut latin1 = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><root><value>".to_vec();
    latin1.push(0x80);
    latin1.extend_from_slice(b"</value></root>");
    let sources = [
        (
            b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"UTF-8\"?><root><value>x</value></root>"
                .to_vec(),
            "x",
        ),
        (utf16_xml(utf16_text, true), "x"),
        (utf16_xml(utf16_text, false), "x"),
        (latin1, "\u{80}"),
    ];
    for (source, expected) in sources {
        let instance = parse_xml_bytes(&schema, &source, "cassandra")
            .test_unwrap("required XML source encoding");
        assert_eq!(
            tree_value(&instance, "value"),
            &FieldPresence::Present(Value::Str(expected.to_owned()))
        );
        let emitted =
            emit_xml_bytes(&schema, &instance, "cassandra").test_unwrap("canonical XML emit");
        assert_eq!(
            panproto_reference::value_io::encoding(&source, &emitted),
            source == emitted,
            "generic encoding predicate diverged on {source:?}"
        );
        assert_eq!(
            panproto_reference::value_io::xml_encoding(&source, &emitted),
            source == emitted,
            "XML encoding predicate diverged on {source:?}"
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

/// Laws 14 through 17 over their complete word scopes.
#[test]
fn bounded_tabular_observables_match_reference() {
    let schema = one_text_column("rows");

    for symbols in scope::sequences(&['#', 'x']) {
        let line: String = symbols.into_iter().collect();
        let input = format!("value\n{line}\n");
        let parsed = parse_tsv(&schema, input.as_bytes(), "cassandra", "rows", b'\t', None)
            .test_unwrap("TSV comment-scope parse");
        let production_is_data =
            !line.is_empty() && table_text(&parsed).is_some_and(|text| text == line);
        assert_eq!(
            panproto_reference::value_io::tsv_comment(&line),
            production_is_data,
            "line={line:?}"
        );
    }

    for symbols in scope::sequences(&['_', 'x']) {
        let cell: String = symbols.into_iter().collect();
        let input = format!("value\n{cell}\n");
        let parsed = parse_tsv(&schema, input.as_bytes(), "cassandra", "rows", b'\t', None)
            .test_unwrap("TSV missing-token scope parse");
        let production_is_literal_missing = table_text(&parsed) == Some("_");
        assert_eq!(
            panproto_reference::value_io::tabular_missing(&cell),
            production_is_literal_missing,
            "cell={cell:?}"
        );
    }

    for column_count in 1..=2 {
        for symbols in scope::sequences(&["", "x"]) {
            let line = symbols.concat();
            let header = if column_count == 1 {
                "value".to_owned()
            } else {
                "value\tother".to_owned()
            };
            let input = format!("{header}\n{line}\n");
            let production_is_one_empty = if column_count == 1 {
                let parsed = parse_tsv(&schema, input.as_bytes(), "cassandra", "rows", b'\t', None)
                    .test_unwrap("one-column scope parse");
                table_text(&parsed) == Some("")
            } else {
                false
            };
            assert_eq!(
                panproto_reference::value_io::tabular_empty_row(column_count, &line),
                production_is_one_empty,
                "columns={column_count}, line={line:?}"
            );
        }
    }

    for symbols in scope::sequences(&['"', '\t', '\r', '\n']) {
        let cell: String = symbols.into_iter().collect();
        let production =
            emit_tsv_with_schema(&schema, &one_table_value(&cell), "cassandra", "rows", b'\t');
        let production_writable = production.is_ok();
        assert_eq!(
            panproto_reference::value_io::tsv_quote(&cell),
            production_writable,
            "cell={cell:?}"
        );
        if !production_writable {
            assert!(matches!(
                production,
                Err(EmitInstanceError::Value(ref error))
                    if error.reason == EmitValueReason::NotRepresentable
            ));
        }
    }
}

/// Law 9 over all member-name sequences of length at most two over two names.
/// The production refusal class is asserted, not just `is_err()`.
#[test]
fn bounded_duplicate_member_refusals_match_reference() {
    let schema = one_text_column("root");
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
        let production = parse_json_bytes(&schema, "root", input.as_bytes(), "cassandra");
        let production_duplicate = matches!(
            production,
            Err(ParseInstanceError::Json(ref message))
                if message.contains("duplicate JSON object key")
        );
        assert_eq!(
            panproto_reference::value_io::duplicate_member(&members),
            production_duplicate,
            "members={members:?}, result={production:?}"
        );
    }
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
/// emitter supplies the route spelling; the oracle decides writer agreement
/// and representation equality on that same pair of strings.
#[test]
fn bounded_toml_numeric_spellings_match_reference() {
    let schema = one_decimal_column("root");
    for numeral in scope::numerals() {
        let Ok(decimal) = panproto_inst::value::DecimalValue::parse(&numeral) else {
            continue;
        };
        let generic = decimal.to_string();
        let instance = one_tree_value("root", FieldPresence::Present(Value::Decimal(decimal)));
        let bytes = emit_toml_bytes(&schema, &instance, "cassandra")
            .unwrap_or_else(|error| panic!("TOML refused admitted numeral {numeral:?}: {error:?}"));
        let text = std::str::from_utf8(&bytes).expect("TOML output is UTF-8");
        let emitted = text
            .lines()
            .find_map(|line| line.strip_prefix("value = "))
            .unwrap_or_else(|| panic!("missing scalar in {text:?}"));

        assert!(
            panproto_reference::value_io::decimal(&generic, emitted),
            "numeral={numeral:?}, generic={generic:?}, emitted={emitted:?}"
        );
        assert!(
            panproto_reference::value_io::toml_scalar(&numeral, emitted),
            "numeral={numeral:?}, emitted={emitted:?}"
        );
    }
}

fn tree_scalar_bytes(route: Route, schema: &Schema, instance: &WInstance) -> ScalarEmission {
    match route {
        Route::Json => {
            let ScalarEmission::Emitted(bytes) = production_emission(
                emit_json_bytes(schema, instance, "cassandra"),
                "JSON scalar",
            ) else {
                return ScalarEmission::NotRepresentable;
            };
            ScalarEmission::Emitted(json_wrapped_scalar(&bytes))
        }
        Route::Toml => {
            let ScalarEmission::Emitted(bytes) = production_emission(
                emit_toml_bytes(schema, instance, "cassandra"),
                "TOML scalar",
            ) else {
                return ScalarEmission::NotRepresentable;
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
            let ScalarEmission::Emitted(bytes) = production_emission(
                panproto_io::xml_pathway::emit_xml_bytes(schema, instance, "cassandra"),
                "XML scalar",
            ) else {
                return ScalarEmission::NotRepresentable;
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
    let ScalarEmission::Emitted(bytes) = production_emission(
        emit_tsv_with_schema(schema, instance, "cassandra", "rows", delimiter),
        "tabular scalar",
    ) else {
        return ScalarEmission::NotRepresentable;
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
                ScalarEmission::NotRepresentable => ScalarEmission::NotRepresentable,
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
        let production = codec
            .parse_functor_preserving(&schema, input.as_bytes())
            .map_err(|error| error.to_string())
            .and_then(|(instance, complement)| {
                codec
                    .emit_functor_preserving(&schema, &instance, &complement)
                    .map_err(|error| error.to_string())
            });
        assert_eq!(
            panproto_reference::tabular::tsv_quote_cst(&cell),
            production
                .as_ref()
                .is_ok_and(|emitted| emitted == input.as_bytes()),
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
    assert_eq!(reference_refusal, Err(ValueIoError::NotRepresentable));
    assert!(matches!(
        production_refusal,
        Err(EmitInstanceError::Value(ref error))
            if error.reason == EmitValueReason::NonFinite
    ));
}

/// Law 5 over the page's complete temporal lexical product for five public
/// canonical emitters. YAML is exercised through its feature-gated public
/// codec below. A successfully parsed temporal is offered to both route
/// predicates; malformed lexemes are refusals on both sides.
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
    #[cfg(feature = "tree-sitter")]
    let yaml_codec = UnifiedCodec::yaml("cassandra").expect("YAML canonical codec");

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
        #[cfg(feature = "tree-sitter")]
        {
            let yaml_production = production_representable(
                panproto_io::InstanceEmitter::emit_wtype(&yaml_codec, schema, &instance),
                &format!("YAML temporal {lexeme:?}"),
            );
            assert_eq!(
                panproto_reference::value_io::temporal(Route::Yaml, &reference_value),
                yaml_production,
                "YAML temporal {lexeme:?}: reference={reference_value:?}, production={production_value:?}"
            );
        }
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

/// Law 18 over the declared two-name scope. The feature-gated preserving parse
/// below is a useful nonempty-name regression, but it does not discharge law
/// 31's complete CST scope.
#[test]
fn bounded_xml_expanded_names_match_reference() {
    let protocol = panproto_protocols::database::cassandra::protocol();
    #[cfg(feature = "tree-sitter")]
    let codec = UnifiedCodec::xml("cassandra").expect("XML CST codec");
    for namespace_parts in scope::sequences(&["urn:a", "urn:b"]) {
        let namespace = namespace_parts.concat();
        for local_parts in scope::sequences(&["a", "b"]) {
            let local = local_parts.concat();
            let production = xml_expanded_name(&namespace, &local);
            assert_eq!(
                production,
                panproto_reference::value_io::xml_expanded_name(&namespace, &local),
                "namespace={namespace:?}, local={local:?}"
            );
            if local.is_empty() {
                continue;
            }

            let edge_name = if namespace.is_empty() {
                local.clone()
            } else {
                format!("{{{namespace}}}{local}")
            };
            let schema = SchemaBuilder::new(&protocol)
                .vertex("doc", "table", None)
                .test_unwrap("XML expanded-name root")
                .vertex("value", "text", None)
                .test_unwrap("XML expanded-name value")
                .edge("doc", "value", "prop", Some(edge_name.as_str()))
                .test_unwrap("XML expanded-name edge")
                .build()
                .test_unwrap("XML expanded-name schema");
            let input = if namespace.is_empty() {
                format!("<doc><{local}>x</{local}></doc>")
            } else {
                format!("<doc xmlns:p=\"{namespace}\"><p:{local}>x</p:{local}></doc>")
            };
            let parsed = parse_xml_bytes(&schema, input.as_bytes(), "cassandra")
                .test_unwrap("canonical XML expanded-name parse");
            assert_eq!(
                tree_value(&parsed, "value"),
                &FieldPresence::Present(Value::Str("x".to_owned())),
                "canonical namespace={namespace:?}, local={local:?}"
            );
            #[cfg(feature = "tree-sitter")]
            {
                let (parsed, _) = codec
                    .parse_wtype_preserving(&schema, input.as_bytes())
                    .test_unwrap("CST XML expanded-name parse");
                assert_eq!(
                    tree_value(&parsed, "value"),
                    &FieldPresence::Present(Value::Str("x".to_owned())),
                    "CST namespace={namespace:?}, local={local:?}"
                );
            }
        }
    }
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
