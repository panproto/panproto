//! Production BSON against the bson reference: decoding, canonical binary
//! writing, the BSON tree encoding, and Extended JSON writing and reading,
//! compared outcome for outcome on the bounded scopes and on generated
//! documents.

#![cfg(feature = "tree-sitter")]

use std::collections::HashMap;

use panproto_expr::exact::{DecimalClass, DecimalValue, WideInt};
use panproto_inst::{FieldPresence, Value, WInstance};
use panproto_io::data_schema::bson::{
    BsonDecodeErrorKind as K, BsonDocument, BsonValue, BsonWriteErrorKind as W,
    ExtJsonErrorKind as E, Mode, decode, encode, from_instance, read_extended_json, to_instance,
    write_extended_json,
};
use panproto_reference::enumerate::bson::{
    byte_strings, documents, documents_count, texts, values,
};
use panproto_reference::pages::bson::{
    self as reference, Bson, Decimal, DecodeRefusal, Document, ReadRefusal, TreeNode, WriteRefusal,
};
use proptest::prelude::*;

fn to_reference_decimal(value: &DecimalValue) -> Decimal {
    let number = |digits: &str| digits.parse::<u128>().unwrap_or_else(|e| panic!("{e}"));
    match &value.class {
        DecimalClass::Finite => Decimal::Finite {
            negative: value.negative,
            coefficient: number(&value.digits),
            exponent: value
                .exponent
                .to_i64()
                .and_then(|e| i32::try_from(e).ok())
                .unwrap_or_else(|| panic!("{value:?}")),
        },
        DecimalClass::Infinite => Decimal::Infinity(value.negative),
        DecimalClass::NaN {
            signalling,
            payload,
        } => Decimal::NaN(value.negative, *signalling, number(payload)),
    }
}

fn to_production_decimal(value: &Decimal) -> DecimalValue {
    match *value {
        Decimal::Finite {
            negative,
            coefficient,
            exponent,
        } => DecimalValue::new(
            negative,
            coefficient.to_string(),
            WideInt::from(i64::from(exponent)),
        )
        .unwrap_or_else(|e| panic!("{e}")),
        Decimal::Infinity(negative) => DecimalValue::infinity(negative),
        Decimal::NaN(negative, signalling, payload) => {
            DecimalValue::nan(negative, signalling, payload.to_string())
        }
    }
}

fn to_reference(document: &BsonDocument) -> Document {
    document
        .0
        .iter()
        .map(|(k, v)| (k.clone(), to_reference_value(v)))
        .collect()
}

fn to_reference_value(value: &BsonValue) -> Bson {
    match value {
        BsonValue::Double(x) => Bson::Double(x.to_bits()),
        BsonValue::String(s) => Bson::String(s.clone()),
        BsonValue::Document(d) => Bson::Document(to_reference(d)),
        BsonValue::Array(items) => Bson::Array(items.iter().map(to_reference_value).collect()),
        BsonValue::Binary { subtype, bytes } => Bson::Binary(*subtype, bytes.clone()),
        BsonValue::Undefined => Bson::Undefined,
        BsonValue::ObjectId(id) => Bson::ObjectId(*id),
        BsonValue::Boolean(b) => Bson::Boolean(*b),
        BsonValue::DateTime(ms) => Bson::DateTime(*ms),
        BsonValue::Null => Bson::Null,
        BsonValue::Regex { pattern, options } => Bson::Regex(pattern.clone(), options.clone()),
        BsonValue::DbPointer { namespace, id } => Bson::DbPointer(namespace.clone(), *id),
        BsonValue::Code(s) => Bson::Code(s.clone()),
        BsonValue::Symbol(s) => Bson::Symbol(s.clone()),
        BsonValue::CodeWithScope { code, scope } => {
            Bson::CodeWithScope(code.clone(), to_reference(scope))
        }
        BsonValue::Int32(n) => Bson::Int32(*n),
        BsonValue::Timestamp { time, increment } => Bson::Timestamp(*time, *increment),
        BsonValue::Int64(n) => Bson::Int64(*n),
        BsonValue::Decimal128(d) => Bson::Decimal128(to_reference_decimal(d)),
        BsonValue::MinKey => Bson::MinKey,
        BsonValue::MaxKey => Bson::MaxKey,
    }
}

fn to_production(document: &Document) -> BsonDocument {
    BsonDocument(
        document
            .iter()
            .map(|(k, v)| (k.clone(), to_production_value(v)))
            .collect(),
    )
}

fn to_production_value(value: &Bson) -> BsonValue {
    match value {
        Bson::Double(bits) => BsonValue::Double(f64::from_bits(*bits)),
        Bson::String(s) => BsonValue::String(s.clone()),
        Bson::Document(d) => BsonValue::Document(to_production(d)),
        Bson::Array(items) => BsonValue::Array(items.iter().map(to_production_value).collect()),
        Bson::Binary(subtype, bytes) => BsonValue::Binary {
            subtype: *subtype,
            bytes: bytes.clone(),
        },
        Bson::Undefined => BsonValue::Undefined,
        Bson::ObjectId(id) => BsonValue::ObjectId(*id),
        Bson::Boolean(b) => BsonValue::Boolean(*b),
        Bson::DateTime(ms) => BsonValue::DateTime(*ms),
        Bson::Null => BsonValue::Null,
        Bson::Regex(pattern, options) => BsonValue::Regex {
            pattern: pattern.clone(),
            options: options.clone(),
        },
        Bson::DbPointer(namespace, id) => BsonValue::DbPointer {
            namespace: namespace.clone(),
            id: *id,
        },
        Bson::Code(s) => BsonValue::Code(s.clone()),
        Bson::Symbol(s) => BsonValue::Symbol(s.clone()),
        Bson::CodeWithScope(code, scope) => BsonValue::CodeWithScope {
            code: code.clone(),
            scope: to_production(scope),
        },
        Bson::Int32(n) => BsonValue::Int32(*n),
        Bson::Timestamp(time, increment) => BsonValue::Timestamp {
            time: *time,
            increment: *increment,
        },
        Bson::Int64(n) => BsonValue::Int64(*n),
        Bson::Decimal128(d) => BsonValue::Decimal128(to_production_decimal(d)),
        Bson::MinKey => BsonValue::MinKey,
        Bson::MaxKey => BsonValue::MaxKey,
    }
}

fn decode_refusal(kind: K) -> DecodeRefusal {
    match kind {
        K::DocumentLength { declared } => DecodeRefusal::DocumentLength(declared),
        K::Truncated => DecodeRefusal::Truncated,
        K::Terminator { byte } => DecodeRefusal::Terminator(byte),
        K::EarlyTerminator => DecodeRefusal::EarlyTerminator,
        K::UnknownType { byte } => DecodeRefusal::UnknownType(byte),
        K::UnterminatedCString => DecodeRefusal::UnterminatedCString,
        K::InvalidUtf8 => DecodeRefusal::InvalidUtf8,
        K::StringLength { declared } => DecodeRefusal::StringLength(declared),
        K::StringTerminator { byte } => DecodeRefusal::StringTerminator(byte),
        K::Boolean { byte } => DecodeRefusal::Boolean(byte),
        K::BinaryLength { declared } => DecodeRefusal::BinaryLength(declared),
        K::OldBinaryLength { declared } => DecodeRefusal::OldBinaryLength(declared),
        K::CodeWithScopeLength { declared } => DecodeRefusal::CodeWithScopeLength(declared),
        K::ArrayKey { expected } => DecodeRefusal::ArrayKey(expected),
        K::RegexOptions => DecodeRefusal::RegexOptions,
        K::NonCanonicalDecimal128 => DecodeRefusal::NonCanonicalDecimal128,
        K::TrailingBytes => DecodeRefusal::TrailingBytes,
        K::Depth => DecodeRefusal::Depth,
        other => panic!("no reference refusal for {other:?}"),
    }
}

fn write_refusal(kind: &W) -> WriteRefusal {
    match kind {
        W::NulInKey => WriteRefusal::NulInKey,
        W::DuplicateKey(_) => WriteRefusal::DuplicateKey,
        W::WrapperKey(_) => WriteRefusal::WrapperKey,
        W::NulInRegex => WriteRefusal::NulInRegex,
        W::RegexOptions => WriteRefusal::RegexOptions,
        W::NaN => WriteRefusal::NaN,
        W::Depth => WriteRefusal::Depth,
        other => panic!("no reference refusal for {other:?}"),
    }
}

fn read_refusal(kind: E) -> ReadRefusal {
    match kind {
        E::Json(_) => ReadRefusal::Json,
        E::NotADocument => ReadRefusal::NotADocument,
        E::NulInKey => ReadRefusal::NulInKey,
        E::WrapperKeys(key) => ReadRefusal::WrapperKeys(key),
        E::WrapperValue(key) => ReadRefusal::WrapperValue(key),
        E::NulInRegex => ReadRefusal::NulInRegex,
        E::RegexOptions => ReadRefusal::RegexOptions,
        E::DecimalRepresentation(_) => ReadRefusal::DecimalRepresentation,
        E::IntegerRange => ReadRefusal::IntegerRange,
        E::InexactNumber => ReadRefusal::InexactNumber,
        other => panic!("no reference refusal for {other:?}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn opaque_field<'v>(fields: &'v HashMap<String, Value>, name: &str) -> &'v Value {
    fields
        .get(name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn held_text(value: &Value) -> String {
    match value {
        Value::Float(x) => format!("float {:016x}", x.to_bits()),
        Value::Str(s) => format!("str {s}"),
        Value::Bytes(b) => format!("bytes {}", hex(b)),
        Value::Bool(b) => format!("bool {b}"),
        Value::Temporal(t) => format!("temporal {t}"),
        Value::Null => "null".to_owned(),
        Value::Int(n) => format!("int {n}"),
        Value::Decimal(d) => format!(
            "decimal {}",
            match &d.class {
                DecimalClass::Finite => format!(
                    "{}{}e{}",
                    if d.negative { "-" } else { "" },
                    d.digits,
                    d.exponent
                ),
                DecimalClass::Infinite => format!("{}inf", if d.negative { "-" } else { "" }),
                DecimalClass::NaN {
                    signalling,
                    payload,
                } => format!(
                    "{}{}nan{payload}",
                    if d.negative { "-" } else { "" },
                    if *signalling { "s" } else { "" }
                ),
            }
        ),
        Value::Opaque { type_, fields } => {
            let text = |name| match opaque_field(fields, name) {
                Value::Str(s) => s.clone(),
                Value::Int(n) => n.to_string(),
                Value::Bytes(b) => hex(b),
                other => panic!("{other:?}"),
            };
            match type_.as_str() {
                "binary" => format!("binary subtype={} bytes={}", text("subtype"), text("bytes")),
                "regex" => format!(
                    "regex options={} pattern={}",
                    text("options"),
                    text("pattern")
                ),
                "db-pointer" => {
                    format!(
                        "db-pointer id={} namespace={}",
                        text("id"),
                        text("namespace")
                    )
                }
                "timestamp" => format!(
                    "timestamp increment={} time={}",
                    text("increment"),
                    text("time")
                ),
                other => panic!("opaque {other}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

/// A production instance in the reference's tree form.
fn to_reference_tree(instance: &WInstance) -> Vec<TreeNode> {
    let anchors = panproto_protocols::data_schema::bson::obj_kinds();
    let anchor = |name: &str| -> &'static str {
        let found = anchors
            .iter()
            .position(|a| a == name)
            .unwrap_or_else(|| panic!("anchor {name}"));
        if found == 0 {
            "element"
        } else {
            panproto_protocols::data_schema::bson::VALUE_VERTICES[found - 1]
        }
    };
    let edge_name = |name: &str| -> &'static str {
        ["element", "value", "item", "scope"]
            .into_iter()
            .find(|n| *n == name)
            .unwrap_or_else(|| panic!("edge {name}"))
    };
    let parents: HashMap<u32, (usize, &'static str)> = instance
        .arcs
        .iter()
        .map(|(parent, child, edge)| {
            let index = usize::try_from(*parent).unwrap_or_else(|e| panic!("{e}"));
            (
                *child,
                (index, edge_name(edge.name.as_deref().unwrap_or_default())),
            )
        })
        .collect();
    let mut ids: Vec<u32> = instance.nodes.keys().copied().collect();
    ids.sort_unstable();
    ids.iter()
        .map(|id| {
            let node = &instance.nodes[id];
            TreeNode {
                parent: parents.get(id).copied(),
                anchor: anchor(&node.anchor),
                held: match &node.value {
                    Some(FieldPresence::Present(value)) => Some(held_text(value)),
                    None => None,
                    other => panic!("{other:?}"),
                },
            }
        })
        .collect()
}

#[test]
fn bson_decoding_matches_the_reference_on_every_bounded_byte_string() {
    let strings = byte_strings();
    assert_eq!(strings.len(), 3291);
    let mut accepted = 0;
    for bytes in &strings {
        let production = decode(bytes)
            .map(|document| to_reference(&document))
            .map_err(|error| (error.offset, decode_refusal(error.kind)));
        accepted += usize::from(production.is_ok());
        assert_eq!(production, reference::decode(bytes), "{}", hex(bytes));
    }
    assert_eq!(accepted, 1632);
}

#[test]
fn bson_writing_and_the_tree_encoding_match_the_reference_on_every_bounded_document() {
    let documents = documents();
    assert_eq!(documents.len(), documents_count(values().len()));
    assert_eq!(documents.len(), 994);
    for document in &documents {
        let production = to_production(document);
        let bytes = encode(&production).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(bytes, reference::encode(document), "{document:?}");
        let decoded = decode(&bytes).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(to_reference(&decoded), *document);
        let instance = to_instance(&production).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            to_reference_tree(&instance),
            reference::tree(document),
            "{document:?}"
        );
        let back = from_instance(&instance).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(to_reference(&back), *document);
    }
}

fn assert_texts_agree(document: &Document) {
    let production = to_production(document);
    for (mode, expected) in [
        (Mode::Canonical, reference::canonical_text(document)),
        (Mode::Relaxed, reference::relaxed_text(document)),
    ] {
        let written = write_extended_json(&production, mode)
            .map_err(|error| (error.path, write_refusal(&error.kind)));
        assert_eq!(written, expected, "{mode:?} {document:?}");
        if let (Mode::Canonical, Ok(text)) = (mode, &written) {
            let read = read_extended_json(text.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(to_reference(&read), *document, "{text}");
        }
    }
}

#[test]
fn extended_json_matches_the_reference_on_every_bounded_document_and_text() {
    let mut written = 0;
    for document in &documents() {
        assert_texts_agree(document);
        written += usize::from(reference::canonical_text(document).is_ok());
    }
    assert_eq!(written, 813);
    let texts = texts();
    assert_eq!(texts.len(), 1688);
    let mut accepted = 0;
    for text in &texts {
        let production = read_extended_json(text.as_bytes())
            .map(|document| to_reference(&document))
            .map_err(|error| {
                let path = if matches!(error.kind, E::Json(_)) {
                    String::new()
                } else {
                    error.path
                };
                (path, read_refusal(error.kind))
            });
        accepted += usize::from(production.is_ok());
        assert_eq!(production, reference::read_text(text), "{text}");
    }
    assert_eq!(accepted, 1650);
}

fn value_strategy() -> impl Strategy<Value = Bson> {
    let leaf = prop::sample::select(values());
    leaf.prop_recursive(3, 24, 3, |inner| {
        let keys = prop::sample::select(vec!["a", "b", "$c", "x.y", "é"]);
        prop_oneof![
            prop::collection::vec((keys, inner.clone()), 0..3).prop_map(|members| {
                Bson::Document(
                    members
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect(),
                )
            }),
            prop::collection::vec(inner, 0..3).prop_map(Bson::Array),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_documents_encode_decode_and_write_as_the_reference(
        members in prop::collection::vec(
            (prop::sample::select(vec!["a", "b", "c"]), value_strategy()),
            0..4,
        )
    ) {
        let document: Document = members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        let production = to_production(&document);
        let bytes = encode(&production).unwrap_or_else(|e| panic!("{e}"));
        prop_assert_eq!(&bytes, &reference::encode(&document));
        let decoded = decode(&bytes)
            .map(|d| to_reference(&d))
            .map_err(|e| (e.offset, decode_refusal(e.kind)));
        prop_assert_eq!(decoded, reference::decode(&bytes));
        let instance = to_instance(&production).unwrap_or_else(|e| panic!("{e}"));
        prop_assert_eq!(to_reference_tree(&instance), reference::tree(&document));
        assert_texts_agree(&document);
    }
}
