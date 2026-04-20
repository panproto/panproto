//! Coverage for atproto string-refinement fidelity.
//!
//! Asserts that `parse_lexicon` preserves `"format"` and `"knownValues"`
//! on string vertices as structured constraints, and that
//! `panproto-check::diff` surfaces changes to those constraints. Also
//! pins forward-compatibility: unknown format names parse without error
//! and round-trip their raw string.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_gat::Name;
use panproto_protocols::web_document::atproto;
use panproto_schema::Constraint;

fn parse(src: &str) -> panproto_schema::Schema {
    let value: serde_json::Value = serde_json::from_str(src).expect("valid JSON");
    atproto::parse_lexicon(&value).expect("parse_lexicon")
}

fn constraints_on<'a>(schema: &'a panproto_schema::Schema, vertex: &str) -> &'a [Constraint] {
    schema
        .constraints
        .get(&Name::from(vertex))
        .map_or(&[] as &[Constraint], Vec::as_slice)
}

fn has_constraint(list: &[Constraint], sort: &str, value: &str) -> bool {
    list.iter()
        .any(|c| c.sort.as_ref() == sort && c.value == value)
}

// ---------------------------------------------------------------------------
// `format` — string refinement fidelity
// ---------------------------------------------------------------------------

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const CREATE_RECORD: &str =
    include_str!("../../../fixtures/atproto/lexicons/com.atproto.repo.createRecord.json");

#[test]
fn feed_post_created_at_preserves_format_datetime() {
    let schema = parse(FEED_POST);
    let cs = constraints_on(&schema, "app.bsky.feed.post:body.createdAt");
    assert!(
        has_constraint(cs, "format", "datetime"),
        "expected format=datetime on createdAt, got {cs:?}",
    );
}

#[test]
fn feed_post_langs_items_preserves_format_language() {
    let schema = parse(FEED_POST);
    // `langs` is an array of strings with `format: "language"`;
    // array items land on the `…:items` vertex.
    let cs = constraints_on(&schema, "app.bsky.feed.post:body.langs:items");
    assert!(
        has_constraint(cs, "format", "language"),
        "expected format=language on langs items, got {cs:?}",
    );
}

#[test]
fn create_record_input_repo_preserves_format_at_identifier() {
    let schema = parse(CREATE_RECORD);
    let cs = constraints_on(&schema, "com.atproto.repo.createRecord:input.repo");
    assert!(
        has_constraint(cs, "format", "at-identifier"),
        "expected format=at-identifier on input.repo, got {cs:?}",
    );
}

#[test]
fn create_record_input_collection_preserves_format_nsid() {
    let schema = parse(CREATE_RECORD);
    let cs = constraints_on(&schema, "com.atproto.repo.createRecord:input.collection");
    assert!(
        has_constraint(cs, "format", "nsid"),
        "expected format=nsid on input.collection, got {cs:?}",
    );
}

// ---------------------------------------------------------------------------
// `knownValues` — open-enum fidelity
// ---------------------------------------------------------------------------

#[test]
fn known_values_round_trip_preserves_array() {
    let src = r#"{
        "lexicon": 1,
        "id": "test.open.enum",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "knownValues": ["post", "reply", "repost"]
                        }
                    }
                }
            }
        }
    }"#;

    let schema = parse(src);
    let cs = constraints_on(&schema, "test.open.enum:body.category");
    let known = cs
        .iter()
        .find(|c| c.sort.as_ref() == "knownValues")
        .expect("knownValues constraint present");
    let decoded: Vec<String> =
        serde_json::from_str(&known.value).expect("value parses as JSON array");
    assert_eq!(decoded, vec!["post", "reply", "repost"]);
}

// ---------------------------------------------------------------------------
// Diff detection — panproto-check sees the new constraints
// ---------------------------------------------------------------------------

#[test]
fn diff_reports_known_values_change() {
    let before = r#"{
        "lexicon": 1,
        "id": "test.open.enum",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "knownValues": ["post", "reply"]
                        }
                    }
                }
            }
        }
    }"#;
    let after = r#"{
        "lexicon": 1,
        "id": "test.open.enum",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "knownValues": ["post", "reply", "repost"]
                        }
                    }
                }
            }
        }
    }"#;

    let old = parse(before);
    let new = parse(after);
    let diff = panproto_check::diff(&old, &new);

    let key = "test.open.enum:body.category";
    let constraint_diff = diff
        .modified_constraints
        .get(key)
        .unwrap_or_else(|| panic!("expected constraint change on {key}, got {diff:?}"));

    // Either the constraint changed value or was remove+add; both
    // are valid surfacings as long as knownValues is represented.
    let touches_known_values = constraint_diff
        .changed
        .iter()
        .any(|c| c.sort == "knownValues")
        || constraint_diff
            .added
            .iter()
            .any(|c| c.sort.as_ref() == "knownValues")
        || constraint_diff
            .removed
            .iter()
            .any(|c| c.sort.as_ref() == "knownValues");
    assert!(
        touches_known_values,
        "expected diff to mention knownValues, got {constraint_diff:?}",
    );
}

#[test]
fn diff_reports_format_change() {
    let before = r#"{
        "lexicon": 1,
        "id": "test.fmt.change",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "did" }
                    }
                }
            }
        }
    }"#;
    let after = r#"{
        "lexicon": 1,
        "id": "test.fmt.change",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "format": "handle" }
                    }
                }
            }
        }
    }"#;

    let diff = panproto_check::diff(&parse(before), &parse(after));
    let key = "test.fmt.change:body.id";
    let cd = diff
        .modified_constraints
        .get(key)
        .unwrap_or_else(|| panic!("expected format change on {key}"));
    let mentions_format = cd.changed.iter().any(|c| c.sort == "format")
        || cd.added.iter().any(|c| c.sort.as_ref() == "format")
        || cd.removed.iter().any(|c| c.sort.as_ref() == "format");
    assert!(mentions_format, "expected format mention in {cd:?}");
}

// ---------------------------------------------------------------------------
// Forward-compat — unknown format strings round-trip verbatim
// ---------------------------------------------------------------------------

#[test]
fn unknown_format_name_parses_total() {
    let src = r#"{
        "lexicon": 1,
        "id": "test.future.fmt",
        "defs": {
            "main": {
                "type": "record",
                "key": "tid",
                "record": {
                    "type": "object",
                    "properties": {
                        "x": { "type": "string", "format": "future-format-xyz" }
                    }
                }
            }
        }
    }"#;

    let schema = parse(src);
    let cs = constraints_on(&schema, "test.future.fmt:body.x");
    assert!(
        has_constraint(cs, "format", "future-format-xyz"),
        "unknown format should round-trip verbatim, got {cs:?}",
    );
}
