//! The canonical encoding of an instance [`Value`], byte for byte.
//!
//! Every canonical instance encoding (lens digests today, instance digests,
//! deltas and segments as they land) writes values through one encoder, so
//! its bytes are pinned here against a hand-decoded expectation: the
//! discriminant of each of the thirteen value tags and the layout of its
//! payload. A change to any of them moves every digest over a value and must
//! bump the contexts that name the encoding.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_inst::Value;
use panproto_inst::identity::{write_value, write_value_map};
use panproto_schema::CanonicalWriter;

/// A little-endian `u64` length or count prefix.
fn len(n: u64) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// A length-prefixed UTF-8 string.
fn str_bytes(s: &str) -> Vec<u8> {
    let mut out = len(s.len() as u64);
    out.extend_from_slice(s.as_bytes());
    out
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// One value of every tag, each with the bytes it encodes to, written out by
/// hand from the layout: a one-byte tag, then the payload.
fn every_tag() -> Vec<(Value, Vec<u8>)> {
    let fields: HashMap<String, Value> = [
        ("b".to_owned(), Value::Int(2)),
        ("a".to_owned(), Value::Bool(false)),
    ]
    .into_iter()
    .collect();
    // Maps are written in ascending key order whatever the hash seed.
    let sorted_fields = cat(&[
        &len(2),
        &str_bytes("a"),
        &[0, 0],
        &str_bytes("b"),
        &[1],
        &2i64.to_le_bytes(),
    ]);
    vec![
        (Value::Bool(true), vec![0, 1]),
        (Value::Int(-2), cat(&[&[1], &(-2i64).to_le_bytes()])),
        (
            Value::Float(-0.0),
            cat(&[&[2], &(-0.0f64).to_bits().to_le_bytes()]),
        ),
        (Value::Str("hé".into()), cat(&[&[3], &str_bytes("hé")])),
        (
            Value::Bytes(vec![0xde, 0xad]),
            cat(&[&[4], &len(2), &[0xde, 0xad]]),
        ),
        (
            Value::CidLink("bafy".into()),
            cat(&[&[5], &str_bytes("bafy")]),
        ),
        (
            Value::Blob {
                ref_: "r".into(),
                mime: "m/t".into(),
                size: 7,
            },
            cat(&[
                &[6],
                &str_bytes("r"),
                &str_bytes("m/t"),
                &7u64.to_le_bytes(),
            ]),
        ),
        (Value::Token("tok".into()), cat(&[&[7], &str_bytes("tok")])),
        (Value::Null, vec![8]),
        (
            Value::Opaque {
                type_: "ty".into(),
                fields: fields.clone(),
            },
            cat(&[&[9], &str_bytes("ty"), &sorted_fields]),
        ),
        (Value::Unknown(fields), cat(&[&[10], &sorted_fields])),
        (
            Value::List(vec![Value::Null, Value::Bool(false)]),
            cat(&[&[11], &len(2), &[8], &[0, 0]]),
        ),
        (Value::LabeledNull(9), cat(&[&[12], &9u64.to_le_bytes()])),
    ]
}

#[test]
fn every_value_tag_writes_its_hand_decoded_bytes() {
    let cases = every_tag();
    assert_eq!(cases.len(), 13, "one case per tag 0 to 12");
    for (value, expected) in cases {
        let mut w = CanonicalWriter::new();
        write_value(&mut w, &value);
        assert_eq!(w.into_bytes(), expected, "{value:?}");
    }
}

#[test]
fn a_value_map_is_written_in_ascending_key_order() {
    let map: HashMap<String, Value> = [
        ("z".to_owned(), Value::Null),
        ("m".to_owned(), Value::Null),
        ("a".to_owned(), Value::Null),
    ]
    .into_iter()
    .collect();
    let mut w = CanonicalWriter::new();
    write_value_map(&mut w, &map);
    assert_eq!(
        w.into_bytes(),
        cat(&[
            &len(3),
            &str_bytes("a"),
            &[8],
            &str_bytes("m"),
            &[8],
            &str_bytes("z"),
            &[8],
        ])
    );
}
