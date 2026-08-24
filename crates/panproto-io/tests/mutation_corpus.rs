//! Mutation round-trip corpus for the format-preserving `UnifiedCodec`.
//!
//! `format_preserving_corpus.rs` proves the *unmodified* half of format
//! preservation: `emit(parse(bytes)) == bytes`. This file proves the
//! *modified* half, which is the half that actually gets used — an
//! instance is parsed, edited, and written back.
//!
//! The property, for every fixture and every scalar the fixture carries:
//!
//! 1. parse the bytes into an instance plus its layout complement,
//! 2. perturb exactly one value (and no structure),
//! 3. emit,
//! 4. reparse the emitted bytes with the same codec and schema.
//!
//! Step 4 succeeding is the *byte-well-formedness* half of the claim: a
//! corrupted emit does not reparse. Comparing the reparsed instance
//! against the perturbed one is the *AST-equality* half: the edit landed,
//! landed in the right place, and nothing else moved.
//!
//! Perturbations deliberately *grow* the token (a longer string, a larger
//! integer). A shrinking edit leaves slack that a cursor-arithmetic
//! emitter can absorb; a growing edit is what exposes coverage decided by
//! rewritten lengths rather than by recorded spans.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::FInstance;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::wtype::WInstance;
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn open_schema(protocol_name: &str) -> Schema {
    let proto = Protocol {
        name: protocol_name.into(),
        schema_theory: format!("Th{protocol_name}Schema"),
        instance_theory: format!("Th{protocol_name}Instance"),
        edge_rules: vec![],
        obj_kinds: vec![],
        constraint_sorts: vec![],
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema")
}

/// A canonical, order-independent rendering of a W-type instance's tree.
///
/// Two instances share a summary exactly when they carry the same nodes
/// (anchor, value, discriminator, shape, position) under the same arcs.
fn ast_summary(inst: &WInstance) -> String {
    let mut nodes: Vec<String> = inst
        .nodes
        .values()
        .map(|n| {
            format!(
                "{} anchor={} value={:?} disc={:?} shape={:?} pos={:?}",
                n.id, n.anchor, n.value, n.discriminator, n.shape, n.position
            )
        })
        .collect();
    nodes.sort();
    let mut arcs: Vec<String> = inst
        .arcs
        .iter()
        .map(|(p, c, e)| format!("{p}->{c} kind={} name={:?}", e.kind, e.name))
        .collect();
    arcs.sort();
    format!(
        "root={}\nnodes:\n{}\narcs:\n{}",
        inst.root,
        nodes.join("\n"),
        arcs.join("\n")
    )
}

/// A canonical rendering of a functor instance's tables.
fn functor_summary(inst: &FInstance) -> String {
    let mut tables: Vec<String> = inst
        .tables
        .iter()
        .map(|(name, rows)| {
            let rendered: Vec<String> = rows
                .iter()
                .map(|row| {
                    let mut cells: Vec<String> =
                        row.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                    cells.sort();
                    cells.join(",")
                })
                .collect();
            format!("{name}:\n{}", rendered.join("\n"))
        })
        .collect();
    tables.sort();
    tables.join("\n")
}

/// Perturb a scalar into a *different* scalar of the same kind, choosing a
/// longer lexical form wherever the kind allows one.
fn perturb(presence: &FieldPresence) -> Option<FieldPresence> {
    let FieldPresence::Present(value) = presence else {
        return None;
    };
    let perturbed = match value {
        Value::Str(s) => Value::Str(format!("{s}-perturbed")),
        Value::Int(i) => Value::Int(i.wrapping_add(1_234_567_890)),
        Value::Float(f) => Value::Float(f + 1.5),
        Value::Bool(b) => Value::Bool(!b),
        _ => return None,
    };
    Some(FieldPresence::Present(perturbed))
}

/// Assert the mutation round-trip property for every perturbable scalar in
/// `input` under `codec`.
///
/// Returns the number of perturbations exercised so a caller can assert the
/// fixture actually carries scalars.
fn assert_mutation_round_trip(codec: &UnifiedCodec, schema: &Schema, input: &[u8], label: &str) {
    let (instance, complement) = codec
        .parse_wtype_preserving(schema, input)
        .unwrap_or_else(|e| panic!("[{label}] parse failed: {e}"));

    let mut targets: Vec<u32> = instance
        .nodes
        .iter()
        .filter(|(_, n)| n.value.as_ref().and_then(perturb).is_some())
        .map(|(&id, _)| id)
        .collect();
    targets.sort_unstable();
    assert!(
        !targets.is_empty(),
        "[{label}] fixture carries no perturbable scalar"
    );

    for id in targets {
        let mut mutated = instance.clone();
        let node = mutated.nodes.get_mut(&id).expect("target node");
        let anchor = node.anchor.to_string();
        let before = node.value.clone();
        node.value = node.value.as_ref().and_then(perturb);
        let after = mutated.nodes[&id].value.clone();

        let emitted = codec
            .emit_wtype_preserving(schema, &mutated, &complement)
            .unwrap_or_else(|e| {
                panic!("[{label}] emit failed after editing node {id} ({anchor}): {e}")
            });

        let (reparsed, _) = codec
            .parse_wtype_preserving(schema, &emitted)
            .unwrap_or_else(|e| {
                panic!(
                    "[{label}] emitted bytes are not well-formed after editing node {id} \
                     ({anchor}) from {before:?} to {after:?}: {e}\n\
                     emitted:\n{}",
                    String::from_utf8_lossy(&emitted)
                )
            });

        let expected = ast_summary(&mutated);
        let actual = ast_summary(&reparsed);
        assert_eq!(
            actual,
            expected,
            "[{label}] editing node {id} ({anchor}) from {before:?} to {after:?} did not \
             round-trip\nemitted:\n{}",
            String::from_utf8_lossy(&emitted)
        );
    }
}

/// The tabular twin of [`assert_mutation_round_trip`], over the functor path.
fn assert_tabular_mutation_round_trip(
    codec: &UnifiedCodec,
    schema: &Schema,
    input: &[u8],
    label: &str,
) {
    let (instance, complement) = codec
        .parse_functor_preserving(schema, input)
        .unwrap_or_else(|e| panic!("[{label}] parse failed: {e}"));

    let mut coords: Vec<(String, usize, String)> = Vec::new();
    for (table, rows) in &instance.tables {
        for (r, row) in rows.iter().enumerate() {
            for key in row.keys() {
                coords.push((table.clone(), r, key.clone()));
            }
        }
    }
    coords.sort();
    assert!(!coords.is_empty(), "[{label}] fixture carries no cells");

    for (table, r, key) in coords {
        let mut mutated = instance.clone();
        let rows = mutated.tables.get_mut(&table).expect("table");
        let cell = rows[r].get(&key).expect("cell").clone();
        let Some(FieldPresence::Present(new_value)) =
            perturb(&FieldPresence::Present(cell.clone()))
        else {
            continue;
        };
        rows[r].insert(key.clone(), new_value.clone());

        let emitted = codec
            .emit_functor_preserving(schema, &mutated, &complement)
            .unwrap_or_else(|e| {
                panic!("[{label}] emit failed after editing {table}[{r}].{key}: {e}")
            });

        let (reparsed, _) = codec
            .parse_functor_preserving(schema, &emitted)
            .unwrap_or_else(|e| {
                panic!(
                    "[{label}] emitted bytes are not well-formed after editing \
                     {table}[{r}].{key} from {cell:?} to {new_value:?}: {e}\nemitted:\n{}",
                    String::from_utf8_lossy(&emitted)
                )
            });

        assert_eq!(
            functor_summary(&reparsed),
            functor_summary(&mutated),
            "[{label}] editing {table}[{r}].{key} from {cell:?} to {new_value:?} did not \
             round-trip\nemitted:\n{}",
            String::from_utf8_lossy(&emitted)
        );
    }
}

fn tabular_schema() -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("rows", "object", None)
        .expect("rows vertex")
        .build()
        .expect("build schema")
}

// ─── JSON ─────────────────────────────────────────────────────────────

macro_rules! json_mutation {
    ($name:ident, $protocol:expr, $input:expr) => {
        #[test]
        fn $name() {
            let codec = UnifiedCodec::json($protocol).expect("json codec");
            let schema = open_schema($protocol);
            assert_mutation_round_trip(&codec, &schema, $input, stringify!($name));
        }
    };
}

json_mutation!(
    mutate_json_minimal_object,
    "test",
    br#"{"a":42,"b":"x","c":true}"#
);
json_mutation!(
    mutate_json_pretty_printed,
    "test",
    b"{\n  \"name\": \"Alice\",\n  \"value\": 42\n}\n"
);
json_mutation!(
    mutate_json_escaped_string,
    "test",
    br#"{"a":"x\ny","b":"quote\"here","c":1}"#
);
json_mutation!(
    mutate_json_unicode_escape,
    "test",
    "{\"a\":\"caf\u{e9}\",\"b\":\"\\u00e9\",\"c\":2}".as_bytes()
);
json_mutation!(
    mutate_json_surrogate_pair,
    "test",
    // A raw byte string, so the source really carries the two escapes.
    // A surrogate pair is the only way JSON can spell an astral character.
    br#"{"a":"x\ud83d\ude00y","b":"plain","c":3}"#
);
json_mutation!(
    mutate_json_nested_array,
    "test",
    br#"{"matrix":[[1,2,3],[4,5,6]]}"#
);
json_mutation!(
    mutate_json_openapi_fixture,
    "openapi",
    include_bytes!("../fixtures/api/openapi_response.json")
);
json_mutation!(
    mutate_json_geojson_fixture,
    "geojson",
    include_bytes!("../fixtures/domain/geojson_features.json")
);

// ─── XML ──────────────────────────────────────────────────────────────

#[test]
fn mutate_xml_elements() {
    let codec = UnifiedCodec::xml("test").expect("xml codec");
    let schema = open_schema("test");
    assert_mutation_round_trip(
        &codec,
        &schema,
        b"<doc><title>Hello</title><n>7</n></doc>",
        "xml_elements",
    );
}

// ─── CSV / TSV ────────────────────────────────────────────────────────

#[test]
fn mutate_csv_simple() {
    let codec = UnifiedCodec::csv("test").expect("csv codec");
    let schema = tabular_schema();
    assert_tabular_mutation_round_trip(
        &codec,
        &schema,
        b"name,age\nAlice,30\nBob,25\n",
        "csv_simple",
    );
}

#[test]
fn mutate_tsv_simple() {
    let codec = UnifiedCodec::tsv("test", "rows").expect("tsv codec");
    let schema = tabular_schema();
    assert_tabular_mutation_round_trip(
        &codec,
        &schema,
        b"name\tage\nAlice\t30\nBob\t25\n",
        "tsv_simple",
    );
}
