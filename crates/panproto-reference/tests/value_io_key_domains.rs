//! Canonical scalar emission and key lookup at the boundary of their domains.

use panproto_reference::exact::BigIntValue;
use panproto_reference::model::{Graph, Node};
use panproto_reference::num::Nat;
use panproto_reference::schema::{Key, KeyRole, KeyScope};
use panproto_reference::value::Value;
use panproto_reference::{keys, value_io};

#[test]
fn emitted_big_integer_retains_its_reference_representation() {
    let value = Value::BigInt(BigIntValue {
        negative: false,
        significand: Nat::from_u64(5),
        zeros: Nat::from_u64(0),
    });
    for route in [
        value_io::Route::Json,
        value_io::Route::Toml,
        value_io::Route::Yaml,
        value_io::Route::Xml,
        value_io::Route::Csv,
        value_io::Route::Tsv,
    ] {
        let emitted = value_io::emit(route, &value);
        assert!(
            emitted.is_err() || value_io::round_trip(route, &value),
            "every successfully emitted value must read as the same carrier: {route:?}, {emitted:?}"
        );
    }
}

#[test]
fn absent_start_node_has_no_key_even_when_key_has_no_parts() {
    let mut graph = Graph::default();
    let key = Key {
        id: "empty".to_owned(),
        parts: Vec::new(),
        role: KeyRole::Primary,
        scope: KeyScope::Document,
        equality: Vec::new(),
    };
    assert_eq!(
        keys::key_value(&graph, 123, &key),
        None,
        "the key-reading outside-domain clause refuses an absent start node"
    );
    graph.nodes.insert(123, Node::new(123, "record"));
    assert_eq!(keys::key_value(&graph, 123, &key), Some(Vec::new()));
}

#[test]
fn canonical_big_integer_still_emits_and_round_trips() {
    let value = Value::BigInt(BigIntValue {
        negative: false,
        significand: Nat::from_u64(1),
        zeros: Nat::from_u64(20),
    });
    for route in [
        value_io::Route::Json,
        value_io::Route::Xml,
        value_io::Route::Tsv,
    ] {
        assert_eq!(
            value_io::emit(route, &value),
            Ok(b"100000000000000000000".to_vec())
        );
        assert!(value_io::round_trip(route, &value));
    }
}

#[test]
fn noncanonical_big_integer_shapes_are_refused() {
    for (negative, significand, zeros) in [(false, 0, 100), (true, 0, 0), (false, 10, 20)] {
        let value = Value::BigInt(BigIntValue {
            negative,
            significand: Nat::from_u64(significand),
            zeros: Nat::from_u64(zeros),
        });
        assert_eq!(
            value_io::emit(value_io::Route::Json, &value),
            Err(value_io::ValueIoError::NotRepresentable)
        );
    }
}

#[test]
fn shortest_float_emission_reads_as_the_same_binary_value() {
    let value = Value::Float(0x3fb9_9999_9999_999a);
    for route in [
        value_io::Route::Json,
        value_io::Route::Toml,
        value_io::Route::Yaml,
        value_io::Route::Xml,
        value_io::Route::Csv,
        value_io::Route::Tsv,
    ] {
        let emitted = value_io::emit(route, &value);
        assert!(
            emitted.is_ok(),
            "the finite float has a canonical spelling on {route:?}"
        );
        assert!(
            value_io::round_trip(route, &value),
            "successfully emitted finite float must read back: {route:?}, {emitted:?}"
        );
    }
}
