//! The boundary of a schema-taking restrict or extend: the protocols its two
//! schemas are read under, and the side a boundary failure is on.
//!
//! Every schema-taking restrict and extend, in every shape, validates its
//! boundary: a restrict validates its input against the source schema under
//! `protocols.src` and its output against the target schema under
//! `protocols.tgt`; an extend validates its output against the target schema
//! under the target protocol. A report that is not `Passed` is
//! [`RestrictError::Validation`](crate::RestrictError::Validation) naming the
//! [`BoundarySide`]. The plan-taking engines (`*_with_plan`) validate
//! nothing, so that a caller holding plans can validate once and restrict
//! many times.

use panproto_schema::Protocol;
use serde::{Deserialize, Serialize};

/// The protocols the two schemas of a restrict are read under.
///
/// A schema says which vertices and edges exist; the protocol says what its
/// vertex kinds and constraint sorts mean. Validation needs both, and so
/// does the table restrict, which classifies tables and columns through the
/// protocol ([`RelSchema::new`](crate::RelSchema::new)).
///
/// ```
/// use panproto_inst::BoundaryProtocols;
/// use panproto_schema::Protocol;
///
/// let protocol = Protocol::default();
/// let both = BoundaryProtocols { src: &protocol, tgt: &protocol };
/// assert!(std::ptr::eq(both.src, both.tgt));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct BoundaryProtocols<'a> {
    /// The protocol of the source schema.
    pub src: &'a Protocol,
    /// The protocol of the target schema.
    pub tgt: &'a Protocol,
}

/// The side of a restrict or extend boundary.
///
/// Serialized in `snake_case`: `"input"` or `"output"`.
///
/// ```
/// use panproto_inst::BoundarySide;
///
/// assert_eq!(serde_json::to_string(&BoundarySide::Output)?, r#""output""#);
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundarySide {
    /// The instance handed to the operation, read under the source schema.
    Input,
    /// The instance the operation produced, read under the target schema.
    Output,
}

impl std::fmt::Display for BoundarySide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input => "input",
            Self::Output => "output",
        })
    }
}

/// The validation plan of a boundary: `schema` under `protocol`, with the
/// default options.
pub(crate) fn boundary_plan(
    schema: &panproto_schema::Schema,
    protocol: &Protocol,
) -> crate::validation::ValidationPlan {
    #[cfg(test)]
    compilations::record();
    crate::validation::ValidationPlan::compile(
        schema,
        protocol,
        &crate::validation::ValidationOptions::default(),
    )
}

/// The validation plan of the output boundary, given the input boundary's
/// plan `input` and the digest `tgt_digest` of `tgt_schema`.
///
/// A plan is a function of its schema, its protocol and its options, and
/// both boundaries use the default options. So when the target schema has
/// the source schema's digest and the two protocols are equal, as in a
/// migration from a schema to itself, the output plan is `input` and is not
/// compiled a second time; otherwise it is `tgt_schema` under
/// `protocols.tgt`.
pub(crate) fn output_plan<'a>(
    input: &'a crate::validation::ValidationPlan,
    tgt_schema: &panproto_schema::Schema,
    tgt_digest: &crate::identity::Digest,
    protocols: BoundaryProtocols<'_>,
) -> std::borrow::Cow<'a, crate::validation::ValidationPlan> {
    if input.schema_digest() == tgt_digest
        && (std::ptr::eq(protocols.src, protocols.tgt) || protocols.src == protocols.tgt)
    {
        std::borrow::Cow::Borrowed(input)
    } else {
        std::borrow::Cow::Owned(boundary_plan(tgt_schema, protocols.tgt))
    }
}

/// How many boundary plans this thread has compiled, for the tests that
/// count them.
#[cfg(test)]
pub(crate) mod compilations {
    use std::cell::Cell;

    thread_local! {
        static COMPILED: Cell<usize> = const { Cell::new(0) };
    }

    /// Count one compilation.
    pub fn record() {
        COMPILED.with(|count| count.set(count.get() + 1));
    }

    /// The number of compilations `f` makes on this thread.
    pub fn during<T>(f: impl FnOnce() -> T) -> (T, usize) {
        let before = COMPILED.with(Cell::get);
        let value = f();
        (value, COMPILED.with(Cell::get) - before)
    }
}

/// `Ok` when `report` passed, else the boundary failure on `side`.
pub(crate) fn require_passed(
    report: crate::validation::ValidationReport,
    side: BoundarySide,
) -> Result<(), crate::error::RestrictError> {
    if report.status == panproto_gat::CheckStatus::Passed {
        Ok(())
    } else {
        Err(crate::error::RestrictError::Validation {
            side,
            report: Box::new(report),
        })
    }
}

/// `Ok` when `plan` was compiled for the schema whose digest is `expected`,
/// else [`PlanSchemaMismatch`](crate::error::RestrictError::PlanSchemaMismatch)
/// on `side`.
pub(crate) fn require_schema(
    plan: &crate::validation::ValidationPlan,
    expected: &crate::identity::Digest,
    side: BoundarySide,
) -> Result<(), crate::error::RestrictError> {
    if plan.schema_digest() == expected {
        Ok(())
    } else {
        Err(crate::error::RestrictError::PlanSchemaMismatch { side })
    }
}

/// The default budget and no external verdicts: what a boundary validates
/// with.
pub(crate) fn unlimited() -> (
    crate::validation::ValidationBudget,
    crate::validation::ExternalVerdicts,
) {
    (
        crate::validation::ValidationBudget::default(),
        crate::validation::ExternalVerdicts::default(),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::borrow::Cow;
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use panproto_gat::Name;
    use panproto_schema::{Edge, ScalarKind, Schema, SchemaBuilder};

    use super::*;
    use crate::identity::schema_digest;
    use crate::metadata::Node;
    use crate::value::{FieldPresence, Value};
    use crate::{CompiledMigration, FInstance, GInstance, RestrictError, WInstance};

    type BoundaryOp<'a> = (
        &'static str,
        Box<dyn Fn() -> Result<(), RestrictError> + 'a>,
    );

    fn prop(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    fn knows() -> Edge {
        Edge {
            src: "person".into(),
            tgt: "person".into(),
            kind: "knows".into(),
            name: None,
        }
    }

    fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut builder = SchemaBuilder::new(&Protocol::default());
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None).unwrap();
        }
        for edge in edges {
            builder = builder
                .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
                .unwrap();
        }
        builder.build().unwrap()
    }

    /// The migration that keeps every vertex and edge of `schema`.
    fn identity(schema: &Schema) -> CompiledMigration {
        CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        }
    }

    fn tree_schema() -> Schema {
        schema(
            &[("body", "object"), ("body.text", "string")],
            &[prop("body", "body.text", "text")],
        )
    }

    fn tree() -> WInstance {
        let nodes = HashMap::from([
            (0, Node::new(0, "body")),
            (
                1,
                Node::new(1, "body.text")
                    .with_value(FieldPresence::Present(Value::Str("hello".into()))),
            ),
        ]);
        let arcs = vec![(0, 1, prop("body", "body.text", "text"))];
        WInstance::new(nodes, arcs, vec![], 0, Name::from("body"))
    }

    fn graph_schema() -> Schema {
        schema(&[("person", "object")], &[knows()])
    }

    fn graph() -> GInstance {
        GInstance::new()
            .with_node(Node::new(0, "person"))
            .with_node(Node::new(1, "person"))
            .with_edge(0, 1, knows())
    }

    fn table_schema() -> Schema {
        schema(
            &[("users", "record"), ("users.name", "string")],
            &[prop("users", "users.name", "name")],
        )
    }

    fn table() -> FInstance {
        let mut table = FInstance::new();
        table.place_row(
            Name::from("users"),
            crate::RowId(0),
            HashMap::from([("name".to_owned(), Value::Str("Alice".into()))]),
        );
        table
    }

    /// A protocol under which the three fixtures validate.
    fn protocol() -> Protocol {
        Protocol {
            value_kinds: BTreeMap::from([("string".to_owned(), ScalarKind::Str)]),
            non_value_kinds: BTreeSet::from(["record".to_owned(), "object".to_owned()]),
            ..Protocol::default()
        }
    }

    /// Every schema-taking operation that validates both of its boundaries,
    /// run along the identity of its fixture's schema from the schema to a
    /// separate copy of it: the operation, the number of boundary plans it
    /// compiled on this thread, and its answer.
    fn runs(
        protocols: BoundaryProtocols<'_>,
    ) -> Vec<(&'static str, usize, Result<(), RestrictError>)> {
        let (w, g, f) = (tree(), graph(), table());
        let (ws, gs, fs) = (tree_schema(), graph_schema(), table_schema());
        let (wt, gt, ft) = (ws.clone(), gs.clone(), fs.clone());
        let (wm, gm, fm) = (identity(&ws), identity(&gs), identity(&fs));
        let ops: Vec<BoundaryOp<'_>> = vec![
            (
                "wtype_restrict",
                Box::new(|| crate::wtype_restrict(&w, &ws, &wt, &wm, protocols).map(drop)),
            ),
            (
                "wtype_extend",
                Box::new(|| crate::wtype_extend(&w, &ws, &wt, &wm, protocols).map(drop)),
            ),
            (
                "graph_restrict",
                Box::new(|| crate::graph_restrict(&g, &gs, &gt, &gm, protocols).map(drop)),
            ),
            (
                "graph_extend",
                Box::new(|| crate::graph_extend(&g, &gs, &gt, &gm, protocols).map(drop)),
            ),
            (
                "graph_extend_partial",
                Box::new(|| crate::graph_extend_partial(&g, &gs, &gt, &gm, protocols).map(drop)),
            ),
            (
                "functor_restrict",
                Box::new(|| crate::functor_restrict(&f, &fs, &ft, &fm, protocols).map(drop)),
            ),
            (
                "functor_extend",
                Box::new(|| crate::functor_extend(&f, &fs, &ft, &fm, protocols).map(drop)),
            ),
            (
                "functor_extend_partial",
                Box::new(|| crate::functor_extend_partial(&f, &fs, &ft, &fm, protocols).map(drop)),
            ),
        ];
        ops.into_iter()
            .map(|(name, op)| {
                let (answer, compiled) = compilations::during(op);
                (name, compiled, answer)
            })
            .collect()
    }

    /// The operations of `runs` that did not succeed after compiling
    /// `expected` boundary plans.
    fn unexpected(protocols: BoundaryProtocols<'_>, expected: usize) -> Vec<String> {
        runs(protocols)
            .into_iter()
            .filter(|(_, compiled, answer)| *compiled != expected || answer.is_err())
            .map(|(name, compiled, answer)| format!("{name}: {compiled} plans, {answer:?}"))
            .collect()
    }

    #[test]
    fn equal_boundaries_compile_one_validation_plan() {
        let protocol = protocol();
        let same = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let wrong = unexpected(same, 1);
        assert!(wrong.is_empty(), "one protocol for both sides: {wrong:#?}");

        let copy = protocol.clone();
        let equal = BoundaryProtocols {
            src: &protocol,
            tgt: &copy,
        };
        let wrong = unexpected(equal, 1);
        assert!(wrong.is_empty(), "two equal protocols: {wrong:#?}");
    }

    #[test]
    fn different_protocols_compile_a_plan_for_each_boundary() {
        let protocol = protocol();
        let other = Protocol {
            name: "other".into(),
            ..protocol.clone()
        };
        let different = BoundaryProtocols {
            src: &protocol,
            tgt: &other,
        };
        let wrong = unexpected(different, 2);
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn the_output_plan_is_the_plan_the_target_compiles_to() {
        let protocol = protocol();
        let other = Protocol {
            name: "other".into(),
            ..protocol.clone()
        };
        let tree = tree_schema();
        let distinct = graph_schema();
        for (tgt, tgt_protocol, reused) in [
            (tree.clone(), &protocol, true),
            (tree.clone(), &other, false),
            (distinct, &protocol, false),
        ] {
            let protocols = BoundaryProtocols {
                src: &protocol,
                tgt: tgt_protocol,
            };
            let input = boundary_plan(&tree, &protocol);
            let output = output_plan(&input, &tgt, &schema_digest(&tgt), protocols);
            assert_eq!(matches!(output, Cow::Borrowed(_)), reused);
            assert_eq!(
                output.digest(),
                boundary_plan(&tgt, tgt_protocol).digest(),
                "the output plan is the target's own plan"
            );
        }
    }
}
