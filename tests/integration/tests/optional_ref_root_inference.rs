//! Primary-entry inference for atproto lexicons with an optional `ref`
//! property targeting a sibling sub-def.
//!
//! The test lexicon mirrors `app.bsky.feed.post`: a `main` record with
//! a required `text` and `createdAt` plus an optional `reply` ref into
//! a `#replyRef` sub-def that itself requires `root` and `parent`. A
//! naive "fallback to whichever sub-def has the most required fields"
//! root-inference heuristic selects `#replyRef` as the primary entry,
//! at which point a canonical reply-less post fails required-edge
//! validation because `root` and `parent` are missing.
//!
//! `parse_lexicon` must place an incoming `ref` edge on `#replyRef` so
//! that `primary_entry` prefers the `main` record over it, and the
//! lexicon's pointed schema must declare the record as the sole entry.
//! Two checks below: a canonical post without `reply` validates clean,
//! and `primary_entry` resolves to the record rather than the sub-def.

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
/// well-formed, so the required-edge predicate at the root anchor
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

/// Primary entry must be the record vertex, not the `#replyRef`
/// sub-def that a naive sub-def-preference heuristic would select.
#[test]
fn primary_entry_is_the_record_not_the_replyref() {
    let schema = parse_lexicon(&lexicon()).expect("parse lexicon");
    let root = primary_entry(&schema).expect("schema has a primary entry");
    assert_eq!(root.as_ref(), "app.bsky.feed.post");
}
