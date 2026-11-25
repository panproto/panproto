//! Tree put restores a value that a parent's `ApplyExpr` rewrote on a child.
//!
//! A record parsed from JSON holds each field as a child of the record's
//! node, so a rewrite of the field writes the child's own value. These tests
//! run every program of at most three rewrites over such a child against a
//! reference on plain integers, and check the round-trip laws on programs
//! that also rename, drop, add, compute, keep and branch.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::value::FieldPresence;
use panproto_inst::{
    CaseBranch, CompiledMigration, EdgeId, FieldTransform, GEdge, GInstance, Node, PutDomainError,
    Value, ViewRef, WInstance,
};
use panproto_lens::{Lens, LensError, get, graph_lens, put};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

const FIELDS: [&str; 3] = ["n", "m", "k"];

fn schema() -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("r", "object", None)
        .unwrap();
    for field in FIELDS {
        let leaf = format!("r.{field}");
        builder = builder
            .vertex(&leaf, "integer", None)
            .unwrap()
            .edge("r", &leaf, "prop", Some(field))
            .unwrap();
    }
    builder.build().unwrap()
}

fn lens(root: Vec<FieldTransform>, leaf: Vec<FieldTransform>) -> Lens {
    let schema = schema();
    let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
    for (vertex, transforms) in [("r", root), ("r.n", leaf)] {
        if !transforms.is_empty() {
            field_transforms.insert(vertex.into(), transforms);
        }
    }
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect::<HashSet<_>>(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms,
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    }
}

/// A record holding each given field as a child, and each of `own` as a
/// field of the record's own node.
fn record(children: &[(&str, i64)], own: &[(&str, i64)]) -> WInstance {
    let mut root = Node::new(0, "r");
    for (field, value) in own {
        root.extra_fields
            .insert((*field).to_owned(), Value::Int(*value));
    }
    let mut nodes = HashMap::from([(0, root)]);
    let mut arcs = Vec::new();
    for (id, (field, value)) in (1..).zip(children) {
        let leaf = format!("r.{field}");
        let node =
            Node::new(id, leaf.as_str()).with_value(FieldPresence::Present(Value::Int(*value)));
        nodes.insert(id, node);
        arcs.push((
            0,
            id,
            Edge {
                src: "r".into(),
                tgt: leaf.as_str().into(),
                kind: "prop".into(),
                name: Some((*field).into()),
            },
        ));
    }
    WInstance::new(nodes, arcs, Vec::new(), 0, "r".into())
}

/// What a tree holds, in an order of its own.
type Held = Vec<(u32, String, Option<String>, Vec<(String, String)>)>;

fn held(tree: &WInstance) -> Held {
    let mut out: Held = tree
        .nodes
        .values()
        .map(|node| {
            let mut fields: Vec<(String, String)> = node
                .extra_fields
                .iter()
                .map(|(key, value)| (key.clone(), format!("{value:?}")))
                .collect();
            fields.sort();
            (
                node.id,
                node.anchor.to_string(),
                node.value.as_ref().map(|value| format!("{value:?}")),
                fields,
            )
        })
        .collect();
    out.sort();
    out
}

/// The record a tree presents when serialized: each field of the record's
/// own node, and each child no such field shadows.
fn presented(tree: &WInstance) -> Vec<(String, String)> {
    let mut out: BTreeMap<String, String> = tree
        .arcs
        .iter()
        .map(|(_, child, edge)| {
            let name = edge.name.as_deref().unwrap_or(&edge.tgt).to_owned();
            (name, format!("{:?}", tree.nodes[child].value))
        })
        .collect();
    for (key, value) in &tree.nodes[&0].extra_fields {
        out.insert(
            key.clone(),
            format!("{:?}", Some(FieldPresence::Present(value.clone()))),
        );
    }
    out.into_iter().collect()
}

fn int(value: i64) -> Expr {
    Expr::Lit(Literal::Int(value))
}

fn op(op: BuiltinOp, left: Expr, right: Expr) -> Expr {
    Expr::Builtin(op, vec![left, right])
}

/// The rewrites of one field, as production states them and as functions of
/// an integer: what a rewrite gives, and what its inverse gives if it has
/// one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rewrite {
    /// `x + 1`, inverse `x - 1`.
    Successor,
    /// `x * 2`, with no inverse.
    Double,
    /// `x * x`, with no inverse: not injective.
    Square,
    /// `x * x`, whose inverse `x` is none.
    SquareClaimed,
    /// `5`, with no inverse: constant.
    Constant,
}

const REWRITES: [Rewrite; 5] = [
    Rewrite::Successor,
    Rewrite::Double,
    Rewrite::Square,
    Rewrite::SquareClaimed,
    Rewrite::Constant,
];

impl Rewrite {
    fn transform(self, key: &str) -> FieldTransform {
        let x = || Expr::Var(key.into());
        let (expr, inverse) = match self {
            Self::Successor => (
                op(BuiltinOp::Add, x(), int(1)),
                Some(op(BuiltinOp::Sub, x(), int(1))),
            ),
            Self::Double => (op(BuiltinOp::Mul, x(), int(2)), None),
            Self::Square => (op(BuiltinOp::Mul, x(), x()), None),
            Self::SquareClaimed => (op(BuiltinOp::Mul, x(), x()), Some(x())),
            Self::Constant => (int(5), None),
        };
        FieldTransform::ApplyExpr {
            key: key.to_owned(),
            coercion_class: if inverse.is_some() {
                CoercionClass::Iso
            } else {
                CoercionClass::Opaque
            },
            expr,
            inverse,
        }
    }

    const fn forward(self, x: i64) -> i64 {
        match self {
            Self::Successor => x + 1,
            Self::Double => x * 2,
            Self::Square | Self::SquareClaimed => x * x,
            Self::Constant => 5,
        }
    }

    const fn backward(self, x: i64) -> Option<i64> {
        match self {
            Self::Successor => Some(x - 1),
            Self::SquareClaimed => Some(x),
            Self::Double | Self::Square | Self::Constant => None,
        }
    }
}

/// Every sequence of at most `longest` members of `alphabet`.
fn sequences<T: Copy>(alphabet: &[T], longest: usize) -> Vec<Vec<T>> {
    let mut out = vec![Vec::new()];
    let mut layer = out.clone();
    for _ in 0..longest {
        layer = layer
            .iter()
            .flat_map(|prefix| {
                alphabet.iter().map(move |member| {
                    let mut longer = prefix.clone();
                    longer.push(*member);
                    longer
                })
            })
            .collect();
        out.extend(layer.iter().cloned());
    }
    out
}

/// The reference's put of `shown` over the source value `original`.
///
/// The baseline is the original where get recorded it, that is, where get
/// changed it, and the view's value elsewhere. Put gives the baseline when
/// the view shows its image, else `shown` carried back through every
/// inverse, else the baseline; it refuses when the image of what it gives is
/// not `shown`.
fn reference_put(program: &[Rewrite], original: i64, shown: i64) -> Option<i64> {
    let forward = |x: i64| program.iter().fold(x, |x, rewrite| rewrite.forward(x));
    let baseline = if forward(original) == original {
        shown
    } else {
        original
    };
    let source = if forward(baseline) == shown {
        baseline
    } else {
        program
            .iter()
            .rev()
            .try_fold(shown, |x, rewrite| rewrite.backward(x))
            .unwrap_or(baseline)
    };
    (forward(source) == shown).then_some(source)
}

#[test]
fn every_program_of_rewrites_puts_back_as_the_reference_on_integers() {
    let (mut restored, mut refused) = (0_u32, 0_u32);
    for program in sequences(&REWRITES, 3) {
        let transforms: Vec<FieldTransform> = program
            .iter()
            .map(|rewrite| rewrite.transform("n"))
            .collect();
        // A branch that is always taken executes the same operations.
        let branch = vec![FieldTransform::Case {
            branches: vec![CaseBranch {
                predicate: Expr::Lit(Literal::Bool(true)),
                transforms: transforms.clone(),
            }],
        }];
        for transforms in [transforms, branch] {
            let lens = lens(transforms, Vec::new());
            for original in -2..=3 {
                let source = record(&[("n", original), ("m", 9)], &[]);
                let (view, complement) = get(&lens, &source).unwrap();
                let image = program.iter().fold(original, |x, r| r.forward(x));
                assert_eq!(
                    view.nodes[&1].value,
                    Some(FieldPresence::Present(Value::Int(image)))
                );
                for shown in -3..=10 {
                    let mut edited = view.clone();
                    edited.nodes.get_mut(&1).unwrap().value =
                        Some(FieldPresence::Present(Value::Int(shown)));
                    let case = format!("{program:?} at {original} shown {shown}");
                    match (
                        put(&lens, &edited, &complement),
                        reference_put(&program, original, shown),
                    ) {
                        (Ok(put_back), Some(expected)) => {
                            let expected = record(&[("n", expected), ("m", 9)], &[]);
                            assert_eq!(held(&put_back), held(&expected), "{case}");
                            assert_eq!(
                                held(&get(&lens, &put_back).unwrap().0),
                                held(&edited),
                                "PutGet of {case}"
                            );
                            restored += 1;
                        }
                        (Err(LensError::PutDomain(error)), None) => {
                            assert!(
                                matches!(
                                    *error,
                                    PutDomainError::TransformNotInvertible {
                                        elem: ViewRef::Node(1),
                                        ..
                                    }
                                ),
                                "{case}: {error:?}"
                            );
                            refused += 1;
                        }
                        outcomes => panic!("{case}: {outcomes:?}"),
                    }
                }
            }
        }
    }
    assert!(
        restored > 0 && refused > 0,
        "{restored} restored, {refused} refused"
    );
}

/// The operations of the law sweep. Each is total on an integer field.
#[derive(Clone, Copy, Debug)]
enum Operation {
    Rewrite(Rewrite),
    RewriteOther,
    Rename,
    RenameBack,
    Drop,
    Add,
    Compute,
    ComputeInPlace,
    ComputeInverse,
    Keep,
    Case,
}

const OPERATIONS: [Operation; 12] = [
    Operation::Rewrite(Rewrite::Successor),
    Operation::Rewrite(Rewrite::Square),
    Operation::RewriteOther,
    Operation::Rename,
    Operation::RenameBack,
    Operation::Drop,
    Operation::Add,
    Operation::Compute,
    Operation::ComputeInPlace,
    Operation::ComputeInverse,
    Operation::Keep,
    Operation::Case,
];

impl Operation {
    fn transform(self) -> FieldTransform {
        let n = || Expr::Var("n".into());
        let rename = |old: &str, new: &str| FieldTransform::RenameField {
            old_key: old.to_owned(),
            new_key: new.to_owned(),
        };
        let compute = |target: &str, inverse: Option<Expr>| FieldTransform::ComputeField {
            target_key: target.to_owned(),
            expr: op(BuiltinOp::Add, n(), int(1)),
            coercion_class: if inverse.is_some() {
                CoercionClass::Iso
            } else {
                CoercionClass::Projection
            },
            inverse,
        };
        match self {
            Self::Rewrite(rewrite) => rewrite.transform("n"),
            Self::RewriteOther => Rewrite::Double.transform("m"),
            Self::Rename => rename("n", "k"),
            Self::RenameBack => rename("k", "n"),
            Self::Drop => FieldTransform::DropField {
                key: "n".to_owned(),
            },
            Self::Add => FieldTransform::AddField {
                key: "n".to_owned(),
                value: Value::Int(4),
            },
            Self::Compute => compute("k", None),
            Self::ComputeInPlace => compute("n", None),
            Self::ComputeInverse => compute("n", Some(op(BuiltinOp::Sub, n(), int(1)))),
            Self::Keep => FieldTransform::KeepFields {
                keys: vec!["n".to_owned(), "m".to_owned()],
            },
            Self::Case => FieldTransform::Case {
                branches: vec![
                    CaseBranch {
                        predicate: op(BuiltinOp::Eq, n(), int(0)),
                        transforms: vec![Rewrite::Double.transform("n")],
                    },
                    CaseBranch {
                        predicate: Expr::Lit(Literal::Bool(true)),
                        transforms: vec![Rewrite::Successor.transform("n")],
                    },
                ],
            },
        }
    }
}

/// Every edit of one value of `view`, with whether it removes a field: each
/// child's value set to each of a few integers, and each field of the
/// record's own node set or removed.
fn edits(view: &WInstance) -> Vec<(bool, WInstance)> {
    let values = [-1, 0, 1, 2, 4, 9];
    let mut out = vec![(false, view.clone())];
    let children: BTreeSet<u32> = view.nodes.keys().copied().filter(|id| *id != 0).collect();
    for id in children {
        for value in values {
            let mut edited = view.clone();
            edited.nodes.get_mut(&id).unwrap().value =
                Some(FieldPresence::Present(Value::Int(value)));
            out.push((false, edited));
        }
    }
    for field in FIELDS {
        for value in values {
            let mut edited = view.clone();
            edited
                .nodes
                .get_mut(&0)
                .unwrap()
                .extra_fields
                .insert(field.to_owned(), Value::Int(value));
            out.push((false, edited));
        }
        let mut edited = view.clone();
        if edited
            .nodes
            .get_mut(&0)
            .unwrap()
            .extra_fields
            .remove(field)
            .is_some()
        {
            out.push((true, edited));
        }
    }
    out
}

/// Check GetPut, and PutGet on every edit of the view put accepts, for each
/// program over each record and each program of the child's own value.
fn check_laws(
    programs: impl IntoIterator<Item = Vec<Operation>>,
    sources: &[WInstance],
    own: &[Vec<FieldTransform>],
) {
    let (mut accepted, mut refused) = (0_u32, 0_u32);

    for program in programs {
        let transforms: Vec<FieldTransform> = program
            .iter()
            .map(|operation| operation.transform())
            .collect();
        for leaf in own {
            let lens = lens(transforms.clone(), leaf.clone());
            for source in sources {
                let Ok((view, complement)) = get(&lens, source) else {
                    continue;
                };
                let case = || format!("{program:?} over {leaf:?} at {:?}", held(source));
                let restored = put(&lens, &view, &complement)
                    .unwrap_or_else(|error| panic!("GetPut refused: {error:?}: {}", case()));
                assert_eq!(held(&restored), held(source), "GetPut: {}", case());
                for (removal, edited) in edits(&view) {
                    match put(&lens, &edited, &complement) {
                        Ok(put_back) => {
                            let (again, _) = get(&lens, &put_back).unwrap();
                            // A removed field may have shadowed a child of
                            // its name, which then presents the field.
                            let agrees = if removal {
                                presented(&again) == presented(&edited)
                            } else {
                                held(&again) == held(&edited)
                            };
                            assert!(
                                agrees,
                                "PutGet: {} edited {:?} restored {:?} shows {:?}",
                                case(),
                                held(&edited),
                                held(&put_back),
                                held(&again)
                            );
                            accepted += 1;
                        }
                        Err(LensError::PutDomain(_)) => refused += 1,
                        Err(error) => panic!("{error:?}: {}", case()),
                    }
                }
            }
        }
    }
    assert!(
        accepted > 0 && refused > 0,
        "{accepted} accepted, {refused} refused"
    );
}

/// A record whose child holds the field, and one that holds it both ways.
fn decisive_records() -> [WInstance; 2] {
    [
        record(&[("n", 2), ("m", 3)], &[]),
        record(&[("n", 2), ("m", 3)], &[("n", 5)]),
    ]
}

#[test]
fn the_round_trip_laws_hold_for_every_program_of_two_operations() {
    let sources = [
        record(&[("n", 0)], &[]),
        record(&[("n", 1), ("m", 3)], &[]),
        record(&[("n", 2), ("k", 7)], &[]),
        record(&[("m", 3)], &[("n", 2)]),
        record(&[("m", 3)], &[]),
    ];
    let sources: Vec<WInstance> = sources.into_iter().chain(decisive_records()).collect();
    let own = [
        Vec::new(),
        vec![Rewrite::Successor.transform("__value__")],
        vec![Rewrite::Square.transform("__value__")],
    ];
    check_laws(sequences(&OPERATIONS, 2), &sources, &own);
}

#[test]
fn the_round_trip_laws_hold_for_every_program_of_three_operations() {
    let programs = sequences(&OPERATIONS, 3)
        .into_iter()
        .filter(|program| program.len() == 3);
    check_laws(programs, &decisive_records(), &[Vec::new()]);
}

/// What a graph holds, in an order of its own.
fn held_graph(graph: &GInstance) -> Held {
    let nodes = graph.nodes().iter().map(|(id, node)| (*id, node.clone()));
    held(&WInstance::new(
        nodes.collect(),
        Vec::new(),
        Vec::new(),
        0,
        "r".into(),
    ))
}

/// The graph of a record, which holds each child as a node of its own.
fn record_graph(tree: &WInstance) -> GInstance {
    let mut ids: Vec<u32> = tree.nodes.keys().copied().collect();
    ids.sort_unstable();
    let mut graph = GInstance::new();
    for id in ids {
        graph = graph.with_node(tree.nodes[&id].clone());
    }
    for (at, (parent, child, edge)) in (0..).zip(&tree.arcs) {
        graph
            .insert_edge(EdgeId(at), GEdge::new(*parent, *child, edge.clone()))
            .unwrap();
    }
    graph
}

#[test]
fn the_round_trip_laws_hold_on_graphs_for_every_program_of_two_operations() {
    let (mut accepted, mut refused) = (0_u32, 0_u32);
    for program in sequences(&OPERATIONS, 2) {
        let transforms: Vec<FieldTransform> = program
            .iter()
            .map(|operation| operation.transform())
            .collect();
        let lens = lens(transforms, Vec::new());
        for source in decisive_records() {
            let source = record_graph(&source);
            let Ok((view, complement)) = graph_lens::get(&lens, &source) else {
                continue;
            };
            let restored = graph_lens::put(&lens, &view, &complement)
                .unwrap_or_else(|error| panic!("GetPut refused: {error:?}: {program:?}"));
            assert_eq!(
                held_graph(&restored),
                held_graph(&source),
                "GetPut: {program:?}"
            );
            let ids: Vec<u32> = view.nodes().keys().copied().collect();
            for id in ids {
                for value in [-1, 0, 2, 9] {
                    for field in [None, Some("n"), Some("k")] {
                        let mut edited = view.clone();
                        let node = edited.node_mut(id).unwrap();
                        match field {
                            None if id == 0 => continue,
                            None => {
                                node.value = Some(FieldPresence::Present(Value::Int(value)));
                            }
                            Some(_) if id != 0 => continue,
                            Some(field) => {
                                node.extra_fields
                                    .insert(field.to_owned(), Value::Int(value));
                            }
                        }
                        match graph_lens::put(&lens, &edited, &complement) {
                            Ok(put_back) => {
                                let (again, _) = graph_lens::get(&lens, &put_back).unwrap();
                                assert_eq!(
                                    held_graph(&again),
                                    held_graph(&edited),
                                    "PutGet: {program:?} restored {:?}",
                                    held_graph(&put_back)
                                );
                                accepted += 1;
                            }
                            Err(LensError::PutDomain(_)) => refused += 1,
                            Err(error) => panic!("{error:?}: {program:?}"),
                        }
                    }
                }
            }
        }
    }
    assert!(
        accepted > 0 && refused > 0,
        "{accepted} accepted, {refused} refused"
    );
}
