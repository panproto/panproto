//! The shared JSON route reads exactly one RFC 8259 value.
//!
//! The tree-sitter JSON grammar also accepts comments and a sequence of
//! values, which RFC 8259 does not; the route refuses both rather than
//! dropping the comment or every value after the first.

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
