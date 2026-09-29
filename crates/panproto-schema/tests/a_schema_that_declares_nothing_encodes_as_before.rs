//! A schema that declares no keys and no references encodes exactly as it
//! did before schemas could declare them.
//!
//! The declarations are one optional trailing element of `Schema`, omitted
//! when empty, so both the named encoding (JSON) and the positional encoding
//! (`MessagePack` arrays) of such a schema are the bytes pinned here, which
//! were written by the release that had no declarations field.
//!
//! Run with `PP_SCHEMA_ENCODING_DUMP=1` to print the current encodings.

use panproto_schema::{Protocol, Schema, SchemaBuilder, SchemaError};

/// One object vertex with one string property: every map holds at most one
/// entry, so the encodings do not depend on hash order.
fn small_schema() -> Result<Schema, SchemaError> {
    let protocol = Protocol {
        name: "demo".into(),
        schema_theory: "ThDemoSchema".into(),
        instance_theory: "ThDemoInstance".into(),
        obj_kinds: vec!["object".into(), "string".into()],
        ..Protocol::default()
    };
    SchemaBuilder::new(&protocol)
        .vertex("post", "object", None::<&str>)?
        .vertex("post.text", "string", None::<&str>)?
        .edge("post", "post.text", "prop", Some("text"))?
        .constraint("post.text", "maxLength", "300")
        .entry("post")
        .build()
}

/// The JSON encoding written by the release without declarations.
const PINNED_JSON: &str = r#"{"protocol":"demo","vertices":{"post":{"id":"post","kind":"object","nsid":null},"post.text":{"id":"post.text","kind":"string","nsid":null}},"edges":[[{"src":"post","tgt":"post.text","kind":"prop","name":"text"},"prop"]],"hyper_edges":{},"constraints":{"post.text":[{"sort":"maxLength","value":"300"}]},"required":{},"nsids":{},"entries":["post"],"variants":{},"orderings":[],"recursion_points":{},"spans":{},"usage_modes":[],"nominal":{},"coercions":[],"mergers":{},"defaults":{},"policies":{},"outgoing":{"post":[{"src":"post","tgt":"post.text","kind":"prop","name":"text"}]},"incoming":{"post.text":[{"src":"post","tgt":"post.text","kind":"prop","name":"text"}]},"between":[[["post","post.text"],[{"src":"post","tgt":"post.text","kind":"prop","name":"text"}]]]}"#;

/// The `MessagePack` encoding written by the release without declarations,
/// as hex.
const PINNED_MSGPACK_HEX: &str = concat!(
    "dc0015a464656d6f82a4706f737493a4706f7374a66f626a656374c0a9706f73742e7465787493a9",
    "706f73742e74657874a6737472696e67c0919294a4706f7374a9706f73742e74657874a470726f70",
    "a474657874a470726f708081a9706f73742e746578749192a96d61784c656e677468a33330308080",
    "91a4706f73748090808090809080808081a4706f73749194a4706f7374a9706f73742e74657874a4",
    "70726f70a47465787481a9706f73742e746578749194a4706f7374a9706f73742e74657874a47072",
    "6f70a474657874919292a4706f7374a9706f73742e746578749194a4706f7374a9706f73742e7465",
    "7874a470726f70a474657874",
);

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[test]
fn a_schema_that_declares_nothing_encodes_as_before() -> Result<(), Box<dyn std::error::Error>> {
    let schema = small_schema()?;
    let json = serde_json::to_string(&schema)?;
    let msgpack = hex(&rmp_serde::to_vec(&schema)?);
    if std::env::var_os("PP_SCHEMA_ENCODING_DUMP").is_some() {
        println!("<<<{json}>>>");
        println!("<<<{msgpack}>>>");
    }
    assert_eq!(json, PINNED_JSON);
    assert_eq!(msgpack, PINNED_MSGPACK_HEX);
    Ok(())
}
