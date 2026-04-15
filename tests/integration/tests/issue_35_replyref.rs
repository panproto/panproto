//! Regression test for panproto/panproto#35.
//!
//! End-to-end: parse an `ATProto` lexicon with an optional `ref` property
//! pointing at a sibling sub-def, infer the parse root via the pointed-
//! schema basepoint, parse a canonical instance, and validate it.
//! Before the fix, the root-inference heuristic deterministically
//! selected the referenced sub-def (`#replyRef`) as the instance root,
//! anchoring the instance at a sort whose required-edge predicate
//! demanded `root` and `parent` — fields the canonical post does not
//! carry. After the fix, `#replyRef` has an incoming `ref` edge and
//! the lexicon's pointed schema declares the record as the sole entry,
//! so the instance is rooted correctly and validation is empty.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use panproto_inst::{parse_json, validate_wtype};
use panproto_protocols::web_document::atproto::parse_lexicon;
use panproto_schema::primary_entry;

fn lexicon() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1,
        "id": "app.bsky.feed.post",
        "defs": {
            "main": {
                "type": "record",
                "record": {
                    "type": "object",
                    "required": ["text", "createdAt"],
                    "properties": {
                        "text": {"type": "string"},
                        "createdAt": {"type": "string"},
                        "reply": {"type": "ref", "ref": "#replyRef"}
                    }
                }
            },
            "replyRef": {
                "type": "object",
                "required": ["root", "parent"],
                "properties": {
                    "root": {"type": "string"},
                    "parent": {"type": "string"}
                }
            }
        }
    })
}

/// A canonical post without a `reply` must validate against the
/// lexicon's main record: absence of an optional ref sub-object is
/// well-formed, so the required-edge predicate at the *root* anchor
/// reports no violations.
#[test]
fn canonical_post_without_reply_validates() {
    let schema = parse_lexicon(&lexicon()).expect("parse lexicon");
    let root = primary_entry(&schema).expect("schema has a primary entry");

    let input = serde_json::json!({
        "text": "hello",
        "createdAt": "2026-04-14T00:00:00.000Z"
    });

    let instance = parse_json(&schema, root.as_ref(), &input).expect("parse instance");
    let errors = validate_wtype(&schema, &instance);
    assert!(
        errors.is_empty(),
        "expected zero validation errors, got {errors:?}"
    );
}

/// Primary entry must be the record vertex, not the sub-def that had
/// been the deterministic fallback choice under the old heuristic.
#[test]
fn primary_entry_is_the_record_not_the_replyref() {
    let schema = parse_lexicon(&lexicon()).expect("parse lexicon");
    let root = primary_entry(&schema).expect("schema has a primary entry");
    assert_eq!(root.as_ref(), "app.bsky.feed.post");
}
