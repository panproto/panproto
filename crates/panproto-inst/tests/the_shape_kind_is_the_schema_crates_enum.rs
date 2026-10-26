//! `panproto_inst::InstanceShapeKind` is the enum `panproto_schema` defines,
//! not a copy, so a shape read from a `Protocol` compares directly with one
//! an instance reports.

use panproto_inst::InstanceShapeKind;
use panproto_schema::InstanceShapeKind as S;
use panproto_schema::Protocol;

#[test]
fn the_shape_kind_is_the_schema_crates_enum() {
    let protocol = Protocol {
        semantic_shape: S::Graph,
        ..Default::default()
    };
    let shape: InstanceShapeKind = protocol.semantic_shape;
    assert_eq!(shape, InstanceShapeKind::Graph);
    assert_eq!(InstanceShapeKind::default(), S::Tree);
}
