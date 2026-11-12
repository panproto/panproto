//! The shared JSON route reads exactly one RFC 8259 value.
//!
//! The tree-sitter JSON grammar also accepts comments, a sequence of
//! values and a number whose decimal point no digit follows, which RFC 8259
//! does not; the route refuses all three rather than dropping the comment
//! or every value after the first, or reading `1.` as a number. A `$type`
//! member, which the route's extraction keeps as an object's discriminator,
//! reads back as a member in its place.

#![cfg(feature = "tree-sitter")]

use panproto_io::config::tree::{Content, read_json};

/// The route's refusal of `text`, or a panic when it reads.
fn refusal(text: &[u8]) -> String {
    match read_json(text) {
        Ok(node) => panic!("{node:?} was read"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_json_text_holding_a_second_value_is_refused() {
    let refused = refusal(br#"{"a":1} {"b":2}"#);
    assert!(refused.contains("JSON text holds 2 values"), "{refused}");
}

#[test]
fn a_json_comment_is_refused_at_its_offset() {
    let after = refusal(b"{\"a\":1}\n// c\n");
    assert!(after.contains("JSON comment at byte 8"), "{after}");
    let inside = refusal(b"{\"a\":/* c */1}");
    assert!(inside.contains("JSON comment at byte 5"), "{inside}");
}

#[test]
fn one_value_between_whitespace_reads() {
    let node = read_json(b" \n{\"a\":[1,true]}\n").map_err(|e| e.to_string());
    assert!(
        matches!(node.as_ref().map(|n| &n.content), Ok(Content::Mapping(entries)) if entries.len() == 1),
        "{node:?}"
    );
}

#[test]
fn a_json_number_whose_decimal_point_no_digit_follows_is_refused() {
    let bare = refusal(b"[1.]");
    assert!(
        bare.contains("JSON number at byte 1 has no digit after its decimal point"),
        "{bare}"
    );
    let exponent = refusal(b"{\"a\":-1.e5}");
    assert!(
        exponent.contains("JSON number at byte 5 has no digit after its decimal point"),
        "{exponent}"
    );
}

#[test]
fn a_json_exponent_reads_with_either_sign_or_none() {
    let node = read_json(b"[1E+2,1.5e+2,15e2,1e-2,-0.0E+0]").map_err(|e| e.to_string());
    let values: Vec<f64> = match node.as_ref().map(|n| &n.content) {
        Ok(Content::Sequence(items)) => items
            .iter()
            .map(|item| match &item.content {
                Content::Scalar(panproto_inst::Value::Float(value)) => *value,
                other => panic!("{other:?}"),
            })
            .collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(values, [100.0, 150.0, 1500.0, 0.01, -0.0]);
    assert!(values[4].is_sign_negative());
}

#[test]
fn a_type_member_reads_as_a_member_in_its_place() {
    let node =
        read_json(br#"{"a":1,"$type":"string","b":{"$type":"x"}}"#).map_err(|e| e.to_string());
    let Ok(Content::Mapping(entries)) = node.as_ref().map(|n| &n.content) else {
        panic!("{node:?}");
    };
    let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["a", "$type", "b"]);
    assert_eq!(entries[1].1.string(), Some("string"));
    assert_eq!(entries[1].1.offset, 15);
    let Content::Mapping(inner) = &entries[2].1.content else {
        panic!("{entries:?}");
    };
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].1.string(), Some("x"));
}
