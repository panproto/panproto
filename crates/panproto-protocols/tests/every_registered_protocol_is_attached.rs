//! Every protocol carries the declarations the semantics tables hold for it.
//!
//! The shapes, constraint semantics and value kinds of a protocol are kept
//! in the tables of `panproto_protocols::semantics`, not in its constructor,
//! and each `protocol()` constructor attaches them. These properties check
//! that the attachment reaches every registered protocol and that every
//! table row names a protocol, once.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use panproto_protocols::registry::{descriptor, descriptors};
use panproto_protocols::semantics::{attach, constraint_semantics, shapes, value_kinds};
use panproto_schema::InstanceShapeKind;

/// Whether a row's protocol name is one a row may carry: a registered
/// canonical name, or the `code:` name of a full-AST protocol, which the
/// parse layer builds outside the registry.
fn is_row_name(name: &str) -> bool {
    name.starts_with("code:") || descriptor(name).is_some_and(|d| d.name == name)
}

#[test]
fn every_registered_protocol_is_attached() {
    for d in descriptors() {
        let built = (d.protocol)();
        let mut again = built.clone();
        attach(&mut again);
        assert_eq!(
            again, built,
            "{}: attaching twice changes the protocol",
            d.name
        );

        let (syntactic, semantic) = shapes::declared(d.name).map_or_else(
            || (InstanceShapeKind::default(), InstanceShapeKind::default()),
            |row| (row.syntactic, row.semantic),
        );
        assert_eq!(built.syntactic_shape, syntactic, "{}", d.name);
        assert_eq!(built.semantic_shape, semantic, "{}", d.name);
        assert_eq!(
            built.constraint_semantics,
            constraint_semantics::declared(d.name),
            "{}",
            d.name
        );
        assert_eq!(
            built.value_kinds,
            value_kinds::declared(d.name),
            "{}",
            d.name
        );
    }
}

#[test]
fn every_protocol_carries_its_value_kinds() {
    for d in descriptors() {
        let built = (d.protocol)();
        for row in value_kinds::rows().filter(|row| row.protocol == d.name) {
            assert_eq!(
                built.value_kinds.get(row.vertex_kind),
                Some(&row.kind),
                "{}: vertex kind {:?}",
                d.name,
                row.vertex_kind
            );
        }
    }
}

#[test]
fn every_protocol_carries_its_constraint_semantics() {
    for d in descriptors() {
        let built = (d.protocol)();
        for row in constraint_semantics::rows().filter(|row| row.protocol == d.name) {
            assert_eq!(
                built.constraint_semantics.get(row.sort),
                Some(&(row.semantics)()),
                "{}: sort {:?}",
                d.name,
                row.sort
            );
        }
    }
}

#[test]
fn every_declared_shape_reaches_its_protocol() {
    for row in shapes::rows() {
        let Some(d) = descriptor(row.protocol) else {
            continue;
        };
        let built = (d.protocol)();
        assert_eq!(
            (built.syntactic_shape, built.semantic_shape),
            (row.syntactic, row.semantic),
            "{}",
            row.protocol
        );
    }
}

#[test]
fn every_shape_row_names_one_protocol_once() {
    let mut seen = BTreeSet::new();
    for row in shapes::rows() {
        assert!(
            is_row_name(row.protocol),
            "shape row {:?} names no protocol by its canonical name",
            row.protocol
        );
        assert!(
            seen.insert(row.protocol),
            "protocol {:?} has two shape rows",
            row.protocol
        );
    }
}

#[test]
fn every_constraint_row_names_one_sort_of_a_protocol_once() {
    let mut seen = BTreeSet::new();
    for row in constraint_semantics::rows() {
        assert!(
            is_row_name(row.protocol),
            "constraint row {:?} names no protocol by its canonical name",
            row.protocol
        );
        assert!(
            seen.insert((row.protocol, row.sort)),
            "sort {:?} of {:?} has two rows",
            row.sort,
            row.protocol
        );
    }
}

#[test]
fn every_value_kind_row_names_one_vertex_kind_of_a_protocol_once() {
    let mut seen = BTreeSet::new();
    for row in value_kinds::rows() {
        assert!(
            is_row_name(row.protocol),
            "value-kind row {:?} names no protocol by its canonical name",
            row.protocol
        );
        assert!(
            seen.insert((row.protocol, row.vertex_kind)),
            "vertex kind {:?} of {:?} has two rows",
            row.vertex_kind,
            row.protocol
        );
    }
}
