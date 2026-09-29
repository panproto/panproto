//! Per-protocol semantic declarations, attached to each
//! [`Protocol`](panproto_schema::Protocol) as it is built.
//!
//! Every `protocol()` constructor in this crate passes its
//! [`Protocol`](panproto_schema::Protocol) to
//! [`attach`](crate::semantics::attach) before returning it, so the
//! declarations of a protocol are kept in tables here rather than in its
//! constructor:
//!
//! - [`shapes`](crate::semantics::shapes): the syntactic and semantic
//!   [`InstanceShapeKind`](panproto_schema::InstanceShapeKind) of each
//!   protocol;
//! - [`constraint_semantics`](crate::semantics::constraint_semantics): the
//!   [`ConstraintSemantics`](panproto_schema::ConstraintSemantics) of each
//!   constraint sort;
//! - [`value_kinds`](crate::semantics::value_kinds): the
//!   [`ScalarKind`](panproto_schema::ScalarKind) of each scalar vertex
//!   kind.
//!
//! Each table keeps its rows in one file per protocol family and keys them
//! by the protocol's canonical name, the name its
//! [`ProtocolDescriptor`](crate::registry::ProtocolDescriptor) carries.

use panproto_schema::Protocol;

pub mod constraint_semantics;
pub mod shapes;
pub mod value_kinds;

use constraint_semantics::ConstraintRow;
use shapes::ShapeRow;
use value_kinds::ValueKindRow;

/// Attaches the semantic declarations of `protocol` (its syntactic and
/// semantic shapes, the semantics of its constraint sorts and the scalar
/// kinds of its vertex kinds) to it.
///
/// The protocol is looked up by [`canonical_name`] of its `name`, so a
/// protocol whose `name` is an alias or an underscore spelling of its
/// canonical name receives the rows of that canonical name. A declared
/// shape row sets both shapes; a protocol with none keeps the shapes its
/// constructor gave it. Constraint-semantics and value-kind rows are added
/// to the maps the constructor built, a row replacing an entry of the same
/// key.
pub fn attach(protocol: &mut Protocol) {
    attach_from(
        protocol,
        shapes::rows(),
        constraint_semantics::rows(),
        value_kinds::rows(),
    );
}

/// The canonical name `name` resolves to: the name of the descriptor it
/// matches (by canonical name or alias, with `_` read as `-`), or else
/// `name` with `_` read as `-`.
#[must_use]
pub fn canonical_name(name: &str) -> String {
    crate::registry::descriptor(name).map_or_else(|| name.replace('_', "-"), |d| d.name.to_owned())
}

/// [`attach`] over the given rows.
fn attach_from<'a>(
    protocol: &mut Protocol,
    shapes: impl IntoIterator<Item = &'a ShapeRow>,
    constraints: impl IntoIterator<Item = &'a ConstraintRow>,
    kinds: impl IntoIterator<Item = &'a ValueKindRow>,
) {
    let name = canonical_name(&protocol.name);
    if let Some(row) = shapes.into_iter().find(|row| row.protocol == name) {
        protocol.syntactic_shape = row.syntactic;
        protocol.semantic_shape = row.semantic;
    }
    for row in constraints.into_iter().filter(|row| row.protocol == name) {
        protocol
            .constraint_semantics
            .insert(row.sort.to_owned(), (row.semantics)());
    }
    for row in kinds.into_iter().filter(|row| row.protocol == name) {
        protocol
            .value_kinds
            .insert(row.vertex_kind.to_owned(), row.kind.clone());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use panproto_gat::Name;
    use panproto_schema::{
        ConstraintSemantics, InstanceShapeKind, LengthUnit, Protocol, ScalarKind,
    };

    use super::{ConstraintRow, ShapeRow, ValueKindRow, attach_from, canonical_name};

    const SHAPES: &[ShapeRow] = &[
        ShapeRow {
            protocol: "uima",
            syntactic: InstanceShapeKind::Tree,
            semantic: InstanceShapeKind::Graph,
        },
        ShapeRow {
            protocol: "redis",
            syntactic: InstanceShapeKind::Table,
            semantic: InstanceShapeKind::Table,
        },
    ];

    const CONSTRAINTS: &[ConstraintRow] = &[
        ConstraintRow {
            protocol: "uima",
            sort: "begin",
            semantics: || ConstraintSemantics::Minimum,
        },
        ConstraintRow {
            protocol: "uima",
            sort: "type",
            semantics: || ConstraintSemantics::Format(Name::from("uima")),
        },
        ConstraintRow {
            protocol: "redis",
            sort: "maxLength",
            semantics: || ConstraintSemantics::MaxLength(LengthUnit::Chars),
        },
    ];

    fn kinds() -> Vec<ValueKindRow> {
        vec![
            ValueKindRow {
                protocol: "uima",
                vertex_kind: "integer",
                kind: ScalarKind::Int {
                    bits: 32,
                    signed: true,
                },
            },
            ValueKindRow {
                protocol: "redis",
                vertex_kind: "string",
                kind: ScalarKind::Str,
            },
        ]
    }

    fn attached(name: &str) -> Protocol {
        let mut protocol = Protocol {
            name: name.into(),
            ..Protocol::default()
        };
        attach_from(&mut protocol, SHAPES, CONSTRAINTS, &kinds());
        protocol
    }

    #[test]
    fn attach_fills_the_four_declarations_from_the_rows() {
        let protocol = attached("uima");
        assert_eq!(protocol.syntactic_shape, InstanceShapeKind::Tree);
        assert_eq!(protocol.semantic_shape, InstanceShapeKind::Graph);
        assert_eq!(
            protocol.constraint_semantics,
            BTreeMap::from([
                ("begin".to_owned(), ConstraintSemantics::Minimum),
                (
                    "type".to_owned(),
                    ConstraintSemantics::Format(Name::from("uima"))
                ),
            ])
        );
        assert_eq!(
            protocol.value_kinds,
            BTreeMap::from([(
                "integer".to_owned(),
                ScalarKind::Int {
                    bits: 32,
                    signed: true
                }
            )])
        );
        let redis = attached("redis");
        assert_eq!(redis.syntactic_shape, InstanceShapeKind::Table);
        assert_eq!(redis.constraint_semantics.len(), 1);
        assert_eq!(redis.value_kinds.len(), 1);
    }

    /// The uima protocol calls itself `uima-cas`, an alias of the canonical
    /// name `uima` its rows are keyed by.
    #[test]
    fn attach_reads_the_rows_of_the_canonical_name() {
        assert_eq!(canonical_name("uima-cas"), "uima");
        assert_eq!(canonical_name("uima_cas"), "uima");
        assert_eq!(canonical_name("k8s_crd"), "k8s-crd");
        assert_eq!(canonical_name("code:sql"), "code:sql");
        let alias = attached("uima-cas");
        assert_eq!(
            Protocol {
                name: "uima".into(),
                ..alias
            },
            attached("uima")
        );
        assert_eq!(
            attached("uima_cas").semantic_shape,
            InstanceShapeKind::Graph
        );
    }

    #[test]
    fn a_protocol_with_no_rows_keeps_its_constructor_values() {
        let mut protocol = Protocol {
            name: "openapi".into(),
            syntactic_shape: InstanceShapeKind::Table,
            value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
            ..Protocol::default()
        };
        let before = protocol.clone();
        attach_from(&mut protocol, SHAPES, CONSTRAINTS, &kinds());
        assert_eq!(protocol, before);
    }

    #[test]
    fn attach_is_idempotent() {
        let once = attached("uima");
        let mut twice = once.clone();
        attach_from(&mut twice, SHAPES, CONSTRAINTS, &kinds());
        assert_eq!(twice, once);
    }
}
