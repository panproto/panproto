#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A computed field survives a JSON round trip of its view.
//!
//! This is the path a binding takes: `get` on a parsed record, the view
//! written to JSON, the JSON read back under the target schema, the computed
//! field that `parse_json` reads as a child folded back into its node's
//! fields, and `put` with the complement from `get`. Putting back the
//! unedited view must restore the source, and putting back a view whose
//! computed field was edited must be refused, whether the field holds a
//! record or a string. Core `put` itself refuses the view as `parse_json`
//! reads it, before the fold, as the reference lens does.

use panproto_inst::{parse_json, to_json};
use panproto_lens::{Lens, LensError, PutDomainError, fold_computed_children_into_fields};
use panproto_schema::{EdgeRule, Protocol, SchemaBuilder};

/// A protocol that classifies no value kind, as a binding falls back to
/// for a protocol it does not know.
fn protocol() -> Protocol {
    Protocol {
        name: "demo-dsl".into(),
        schema_theory: "ThConstrainedGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![
            EdgeRule {
                edge_kind: "record-schema".into(),
                src_kinds: vec![],
                tgt_kinds: vec![],
            },
            EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec![],
                tgt_kinds: vec![],
            },
        ],
        obj_kinds: [
            "rec",
            "rec:body",
            "rec:body.text",
            "string",
            "record",
            "object",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
        ..Protocol::default()
    }
}

/// The lens computing `g` from `text` by `expr`, over a body whose `g`
/// vertex has kind `g_kind`.
fn lens(protocol: &Protocol, g_kind: &str, expr: &str) -> Lens {
    let schema = SchemaBuilder::new(protocol)
        .vertex("rec", "rec", None)
        .unwrap()
        .vertex("rec:body", "rec:body", None)
        .unwrap()
        .vertex("rec:body.text", "rec:body.text", None)
        .unwrap()
        .vertex("rec:body.g", g_kind, None)
        .unwrap()
        .edge("rec", "rec:body", "record-schema", None)
        .unwrap()
        .edge("rec:body", "rec:body.text", "prop", Some("text"))
        .unwrap()
        .edge("rec:body", "rec:body.g", "prop", Some("g"))
        .unwrap()
        .build()
        .unwrap();
    let document = panproto_lens_dsl::eval::eval_json(
        &serde_json::json!({
            "id": "demo.compute-roundtrip",
            "source": "v1",
            "target": "v2",
            "steps": [{ "compute_field": { "target": "g", "expr": expr } }],
        })
        .to_string(),
    )
    .unwrap();
    panproto_lens_dsl::compile(&document, "rec:body", &|_| None)
        .unwrap()
        .instantiate(&schema, protocol)
        .unwrap()
        .into_single()
        .unwrap()
}

/// Get, write the view to JSON, check it is `expected_view`, then read
/// back `shown` (the view as a client returns it), check that core `put`
/// refuses it as read, and put it after folding its computed field back
/// into its node's fields.
fn round_trip(
    protocol: &Protocol,
    lens: &Lens,
    expected_view: &serde_json::Value,
    shown: &serde_json::Value,
) -> Result<serde_json::Value, LensError> {
    let source = parse_json(
        &lens.src_schema,
        protocol,
        "rec:body",
        &serde_json::json!({ "text": "hello" }),
    )
    .unwrap();
    let (view, complement) = panproto_lens::get(lens, &source).unwrap();
    assert_eq!(&to_json(&lens.tgt_schema, &view).unwrap(), expected_view);
    let reparsed = parse_json(&lens.tgt_schema, protocol, "rec:body", shown).unwrap();
    let direct = panproto_lens::put(lens, &reparsed, &complement);
    assert!(
        matches!(&direct, Err(LensError::PutDomain(_))),
        "core put refuses the computed field read back as a child: {direct:?}"
    );
    let folded = fold_computed_children_into_fields(lens, &reparsed, &complement);
    assert!(folded.nodes.len() < reparsed.nodes.len(), "the fold moves `g` into a field");
    panproto_lens::put(lens, &folded, &complement)
        .map(|restored| to_json(&lens.src_schema, &restored).unwrap())
}

#[test]
fn an_unedited_view_with_a_record_valued_computed_field_puts_back() {
    let protocol = protocol();
    let lens = lens(&protocol, "object", "{ a = text }");
    let view = serde_json::json!({ "text": "hello", "g": { "a": "hello" } });
    assert_eq!(
        round_trip(&protocol, &lens, &view, &view).unwrap(),
        serde_json::json!({ "text": "hello" })
    );
}

#[test]
fn an_edited_record_valued_computed_field_is_refused() {
    let protocol = protocol();
    let lens = lens(&protocol, "object", "{ a = text }");
    let view = serde_json::json!({ "text": "hello", "g": { "a": "hello" } });
    let edited = serde_json::json!({ "text": "hello", "g": { "a": "bye" } });
    let refused = round_trip(&protocol, &lens, &view, &edited).unwrap_err();
    assert!(
        matches!(
            &refused,
            LensError::PutDomain(error)
                if matches!(&**error, PutDomainError::DerivedCellEdited { field, .. } if field == "g")
        ),
        "{refused:?}"
    );
}

#[test]
fn an_unedited_view_with_a_string_valued_computed_field_puts_back() {
    let protocol = protocol();
    let lens = lens(&protocol, "string", "upper text");
    let view = serde_json::json!({ "text": "hello", "g": "HELLO" });
    assert_eq!(
        round_trip(&protocol, &lens, &view, &view).unwrap(),
        serde_json::json!({ "text": "hello" })
    );
}

#[test]
fn an_edited_string_valued_computed_field_is_refused() {
    let protocol = protocol();
    let lens = lens(&protocol, "string", "upper text");
    let view = serde_json::json!({ "text": "hello", "g": "HELLO" });
    let edited = serde_json::json!({ "text": "hello", "g": "BYE" });
    let refused = round_trip(&protocol, &lens, &view, &edited).unwrap_err();
    assert!(
        matches!(
            &refused,
            LensError::PutDomain(error)
                if matches!(&**error, PutDomainError::DerivedCellEdited { field, .. } if field == "g")
        ),
        "{refused:?}"
    );
}

#[test]
fn an_edited_source_beside_an_unedited_computed_field_is_refused() {
    let protocol = protocol();
    let lens = lens(&protocol, "string", "upper text");
    let view = serde_json::json!({ "text": "hello", "g": "HELLO" });
    let edited = serde_json::json!({ "text": "bye", "g": "HELLO" });
    let refused = round_trip(&protocol, &lens, &view, &edited).unwrap_err();
    assert!(
        matches!(
            &refused,
            LensError::PutDomain(error)
                if matches!(&**error, PutDomainError::DerivedCellEdited { field, .. } if field == "g")
        ),
        "{refused:?}"
    );
}

#[test]
fn an_edited_source_with_its_recomputed_field_puts_back() {
    let protocol = protocol();
    let lens = lens(&protocol, "string", "upper text");
    let view = serde_json::json!({ "text": "hello", "g": "HELLO" });
    let edited = serde_json::json!({ "text": "bye", "g": "BYE" });
    assert_eq!(
        round_trip(&protocol, &lens, &view, &edited).unwrap(),
        serde_json::json!({ "text": "bye" })
    );
}
