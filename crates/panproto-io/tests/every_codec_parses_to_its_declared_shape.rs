//! A codec parses to the syntactic shape its protocol declares.
//!
//! A protocol declares the shape of its syntax in
//! `Protocol::syntactic_shape`, from the shape table of
//! `panproto_protocols::semantics`. A plain parse through the codec
//! registry returns the codec's native representation, so the two must
//! name the same shape, and a protocol whose syntax is a graph must reach
//! its graph through `parse_graph`.

#![allow(clippy::expect_used)]

use panproto_io::default_registry;
use panproto_protocols::registry::descriptor;
use panproto_protocols::semantics::shapes;
use panproto_schema::InstanceShapeKind;

#[test]
fn every_codec_parses_to_its_declared_shape() {
    let registry = default_registry();
    for name in registry.protocol_names() {
        let Some(d) = descriptor(name) else {
            continue;
        };
        if shapes::declared(d.name).is_none() {
            continue;
        }
        let protocol = (d.protocol)();
        let native = registry.native_repr(name).expect("a listed name resolves");
        assert_eq!(
            InstanceShapeKind::from(native),
            protocol.syntactic_shape,
            "{name}: the codec parses to {native:?}, the protocol declares {:?}",
            protocol.syntactic_shape
        );
        if protocol.syntactic_shape == InstanceShapeKind::Graph {
            assert!(
                registry.supports_graph(name),
                "{name}: a graph syntax with no graph codec"
            );
        }
        if protocol.syntactic_shape == InstanceShapeKind::Table {
            assert!(
                registry.supports_table(name),
                "{name}: a table syntax with no table entry"
            );
        }
    }
}

/// Every declared shape row whose protocol has a codec names a codec the
/// registry lists, so the property above reaches it.
#[test]
fn every_declared_shape_with_a_codec_is_checked() {
    let registry = default_registry();
    let listed: Vec<&str> = registry.protocol_names().collect();
    for row in shapes::rows() {
        if registry.native_repr(row.protocol).is_ok() {
            assert!(
                listed.contains(&row.protocol),
                "{} resolves but is listed under another name",
                row.protocol
            );
        }
    }
}
