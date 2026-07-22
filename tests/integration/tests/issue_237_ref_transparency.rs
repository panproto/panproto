//! Regression test for issue #237.
//!
//! A `ref` / `record-schema` edge is a transparent type indirection, so
//! a record whose field type is a named definition must parse to the
//! *same* instance graph as the structurally-equivalent schema that
//! inlines the definition with `prop` edges. Before the fix, a protocol
//! that models nesting with `ref` edges parsed to a shallow one-node
//! instance (the referenced object landing in `extra_fields` as an
//! opaque `Unknown`), while the inlined protocol parsed deep — the same
//! data, two different graphs.

use panproto_inst::{parse_json, to_json};
use panproto_protocols::atproto;

/// A lexicon whose record body has one field `kf` of type `ref` to a
/// sibling def `#k` (an object with an integer field `t`).
fn atproto_lexicon() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1, "id": "d.a", "defs": {
            "main": { "type": "record", "key": "tid", "record": {
                "type": "object", "required": ["kf"],
                "properties": { "kf": { "type": "ref", "ref": "#k" } } } },
            "k": { "type": "object", "required": ["t"],
                "properties": { "t": { "type": "integer" } } }
        }
    })
}

/// The same logical shape, inlined (no indirection): an object with a
/// nested object field.
fn json_schema_doc() -> serde_json::Value {
    serde_json::json!({
        "type": "object", "required": ["kf"],
        "properties": { "kf": {
            "type": "object", "required": ["t"],
            "properties": { "t": { "type": "integer" } } } }
    })
}

/// A structural signature of an instance graph that ignores vertex ids
/// (which differ by protocol) but captures shape: node/arc counts, the
/// `(kind, name)` of every arc, every leaf value, and any `extra_fields`
/// keys (a nonempty set is exactly the shallow-parse symptom).
fn signature(schema: &panproto_schema::Schema, root: &str, record: &serde_json::Value) -> String {
    let inst = parse_json(schema, root, record).expect("parse_json");
    let mut arcs: Vec<String> = inst
        .arcs
        .iter()
        .map(|(_, _, e)| format!("{}[{}]", e.kind, e.name.as_deref().unwrap_or("")))
        .collect();
    arcs.sort();
    let mut leaves: Vec<String> = inst
        .nodes
        .values()
        .filter_map(|n| n.value.as_ref().map(|v| format!("{v:?}")))
        .collect();
    leaves.sort();
    let extra: usize = inst.nodes.values().map(|n| n.extra_fields.len()).sum();
    format!(
        "nodes={} arcs={:?} leaves={:?} extra_fields={}",
        inst.nodes.len(),
        arcs,
        leaves,
        extra
    )
}

#[test]
fn ref_and_inlined_schemas_produce_the_same_instance_graph() {
    let at = atproto::parse_lexicon(&atproto_lexicon()).expect("atproto schema");
    let js = panproto_protocols::parse_schema_document("json-schema", &json_schema_doc())
        .expect("json-schema schema");

    let record = serde_json::json!({ "kf": { "t": 5 } });

    let at_sig = signature(&at, "d.a", &record);
    let js_sig = signature(&js, "root", &record);

    assert_eq!(
        at_sig, js_sig,
        "ref-based and inlined schemas must materialize the same instance graph"
    );

    // The shallow-parse symptom: the referenced object must NOT collapse
    // into extra_fields. Signature already checks extra_fields=0, but
    // assert the deep shape explicitly.
    assert!(
        at_sig.contains("nodes=3"),
        "expected a deep 3-node graph, got: {at_sig}"
    );
    assert!(
        at_sig.contains("extra_fields=0"),
        "referenced object must not land in extra_fields, got: {at_sig}"
    );
}

#[test]
fn transparent_indirection_preserves_round_trip() {
    let at = atproto::parse_lexicon(&atproto_lexicon()).expect("atproto schema");
    let record = serde_json::json!({ "kf": { "t": 5 } });
    let inst = parse_json(&at, "d.a", &record).expect("parse_json");
    assert_eq!(
        to_json(&at, &inst),
        record,
        "deep parse must still serialize back to the original record"
    );
}
