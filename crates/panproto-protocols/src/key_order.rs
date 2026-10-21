//! The order in which a document parser meets an object's members.
//!
//! A parser builds a schema by walking a JSON document, and the schema
//! keeps the order it was built in: the adjacency buckets that
//! [`Schema::outgoing_edges`](panproto_schema::Schema::outgoing_edges)
//! returns list a vertex's edges in insertion order, and
//! [`canonical_bytes`](panproto_schema::canonical_bytes), and with it the
//! schema's identity, covers that order. A parser that walks an object in
//! the order its [`serde_json::Map`] iterates therefore lets the map decide
//! the schema's identity.
//!
//! What that order is depends on the build: `serde_json` sorts a map's keys,
//! unless its `preserve_order` feature is on, in which case the map keeps
//! the order the document wrote them in. The feature is unified across a
//! build, so any crate that turns it on changes the identity of every
//! schema a document parser in this crate reads, and two documents that
//! differ only in the order of their members, which JSON does not
//! distinguish (RFC 8259, Sec. 4), would get different identities.
//!
//! Every document parser therefore reads its input through [`in_key_order`],
//! which presents each object with its members in ascending key order: the
//! order a default build already iterates in, so a default build reads the
//! input it was given, unchanged and uncopied.

use std::borrow::Cow;

use serde_json::{Map, Value};

/// `value` with the members of every object, at every depth, in ascending
/// key order.
///
/// Borrowed when every object already iterates in that order (always the
/// case unless `serde_json`'s `preserve_order` feature is on); otherwise a
/// copy with the objects rebuilt in order. Arrays keep their order, which
/// JSON gives meaning to.
#[must_use]
pub fn in_key_order(value: &Value) -> Cow<'_, Value> {
    if is_in_key_order(value) {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(sorted(value))
    }
}

/// [`in_key_order`] over each document of a bundle.
#[must_use]
pub fn all_in_key_order(values: &[Value]) -> Cow<'_, [Value]> {
    if values.iter().all(is_in_key_order) {
        Cow::Borrowed(values)
    } else {
        Cow::Owned(values.iter().map(sorted).collect())
    }
}

/// Whether every object in `value` iterates in ascending key order.
fn is_in_key_order(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().all(is_in_key_order),
        Value::Object(members) => {
            members
                .keys()
                .zip(members.keys().skip(1))
                .all(|(a, b)| a < b)
                && members.values().all(is_in_key_order)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
    }
}

/// A copy of `value` with every object's members inserted in ascending key
/// order.
fn sorted(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        Value::Object(members) => {
            let mut entries: Vec<(&String, &Value)> = members.iter().collect();
            entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
            let mut out = Map::with_capacity(entries.len());
            for (key, member) in entries {
                out.insert(key.clone(), sorted(member));
            }
            Value::Object(out)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Every object of `value`, at every depth, with its keys in iteration
    /// order.
    fn key_orders(value: &Value, out: &mut Vec<Vec<String>>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| key_orders(item, out)),
            Value::Object(members) => {
                out.push(members.keys().cloned().collect());
                members.values().for_each(|member| key_orders(member, out));
            }
            _ => {}
        }
    }

    /// An object built by inserting its members in descending key order,
    /// nested inside an array and another object.
    fn descending() -> Value {
        let mut inner = Map::new();
        for key in ["z", "m", "a"] {
            inner.insert(key.to_owned(), Value::from(key));
        }
        let mut outer = Map::new();
        outer.insert("y".to_owned(), Value::Array(vec![Value::Object(inner)]));
        outer.insert("b".to_owned(), Value::from(1));
        Value::Object(outer)
    }

    #[test]
    fn every_object_is_read_in_ascending_key_order() {
        let value = descending();
        let read = in_key_order(&value);
        let mut orders = Vec::new();
        key_orders(&read, &mut orders);
        assert_eq!(orders, [vec!["b", "y"], vec!["a", "m", "z"]]);
        assert_eq!(*read, value, "reordering changes no member");
    }

    #[test]
    fn an_ordered_document_is_borrowed() {
        let value = serde_json::json!({ "a": [{ "c": 1, "d": 2 }], "b": null });
        assert!(matches!(in_key_order(&value), Cow::Borrowed(_)));
        let values = [value, Value::from("s")];
        assert!(matches!(all_in_key_order(&values), Cow::Borrowed(_)));
    }

    #[test]
    fn a_bundle_is_read_document_by_document() {
        let values = [Value::from(0), descending()];
        let read = all_in_key_order(&values);
        assert_eq!(read.as_ref(), values.as_slice());
        let mut orders = Vec::new();
        read.iter().for_each(|value| key_orders(value, &mut orders));
        assert_eq!(orders, [vec!["b", "y"], vec!["a", "m", "z"]]);
    }
}
