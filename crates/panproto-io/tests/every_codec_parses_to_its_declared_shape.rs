//! A codec parses to the syntactic shape its protocol declares.
//!
//! A protocol declares the shape of its syntax in
//! `Protocol::syntactic_shape`, from the shape table of
//! `panproto_protocols::semantics`. A plain parse through the codec
//! registry returns the codec's native representation, so the two must
//! name the same shape, a protocol whose syntax is a graph must reach its
//! graph through `parse_graph`, and one whose syntax is a table must have
//! its table entries.

#![allow(clippy::expect_used)]

use panproto_io::{NativeRepr, default_registry};
use panproto_protocols::registry::{descriptor, descriptors};
use panproto_protocols::semantics::shapes;
use panproto_schema::InstanceShapeKind;

/// Protocols whose registered codec does not yet parse to the declared
/// syntactic shape, with the native representation it has today. Each is
/// checked to still disagree in exactly this way, so the entry is removed
/// when the protocol's codec is replaced.
const CODEC_NOT_YET_REPLACED: &[(&str, NativeRepr)] = &[
    ("amr", NativeRepr::Table),
    ("arrow", NativeRepr::Tree),
    ("dataframe", NativeRepr::Tree),
    ("parquet", NativeRepr::Tree),
];

/// Protocols with a declared shape and no instance codec yet. Each is
/// checked to still have none, so the entry is removed when the codec is
/// registered.
const NO_CODEC_YET: &[&str] = &["graphql", "json-schema", "protobuf"];

/// What is wrong with the codec registered as `name`, if anything: its
/// native representation against its protocol's syntactic shape, and the
/// graph or table entry that shape requires.
fn disagreement(name: &str) -> Option<String> {
    let registry = default_registry();
    let protocol = (descriptor(name)
        .expect("a listed codec names a protocol")
        .protocol)();
    let native = registry.native_repr(name).expect("a listed name resolves");
    let declared = protocol.syntactic_shape;
    if InstanceShapeKind::from(native) != declared {
        return Some(format!(
            "{name}: the codec parses to {native:?}, the protocol declares {declared:?}"
        ));
    }
    if declared == InstanceShapeKind::Graph && !registry.supports_graph(name) {
        return Some(format!("{name}: a graph syntax with no graph codec"));
    }
    if declared == InstanceShapeKind::Table && !registry.supports_table(name) {
        return Some(format!("{name}: a table syntax with no table entry"));
    }
    None
}

#[test]
fn every_codec_parses_to_its_declared_shape() {
    let registry = default_registry();
    let mut wrong = Vec::new();
    for name in registry.protocol_names() {
        let Some(d) = descriptor(name) else {
            continue;
        };
        if shapes::declared(d.name).is_none() {
            continue;
        }
        let pending = CODEC_NOT_YET_REPLACED.iter().find(|(n, _)| *n == name);
        match (pending, disagreement(name)) {
            (None, Some(reason)) => wrong.push(reason),
            (Some((_, today)), _) => {
                let native = registry.native_repr(name).expect("a listed name resolves");
                if native != *today {
                    wrong.push(format!(
                        "{name}: the codec now parses to {native:?}, not the listed \
                         {today:?}; update or remove its CODEC_NOT_YET_REPLACED entry"
                    ));
                }
            }
            (None, None) => {}
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every entry of the pending list names a codec that still disagrees
/// with its declaration, so an entry cannot outlive its fix.
#[test]
fn every_codec_not_yet_replaced_disagrees_with_its_declared_shape() {
    for (name, _) in CODEC_NOT_YET_REPLACED {
        assert!(
            disagreement(name).is_some(),
            "{name} parses to its declared shape; remove it from CODEC_NOT_YET_REPLACED"
        );
    }
}

/// Every registered protocol with a declared shape has an instance codec,
/// apart from the listed ones, which still have none.
#[test]
fn every_protocol_with_a_declared_shape_has_a_codec() {
    let registry = default_registry();
    let mut missing = Vec::new();
    for d in descriptors() {
        if shapes::declared(d.name).is_none() {
            continue;
        }
        let has_codec = registry.native_repr(d.name).is_ok();
        let pending = NO_CODEC_YET.contains(&d.name);
        if has_codec == pending {
            missing.push(if has_codec {
                format!("{}: now has a codec; remove it from NO_CODEC_YET", d.name)
            } else {
                format!("{}: declares a shape and has no codec", d.name)
            });
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
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
