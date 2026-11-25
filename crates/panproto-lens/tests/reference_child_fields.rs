//! Tree put against the reference on the fields a record's children hold.
//!
//! Each case is a program of a record's fields, a source record whose
//! children hold them, and an edit of its view, in memory or after a
//! serialization. Production and the reference run the same cases, the two
//! round-trip laws are checked on what production gives without either rule,
//! and a brute-force search over the candidates counts the views that have a
//! source which put refuses.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};
use panproto_gat::CoercionClass;
use panproto_inst::value::FieldPresence;
use panproto_inst::{CaseBranch, CompiledMigration, FieldTransform, Node, Value, WInstance};
use panproto_lens::{Complement, Lens, LensError, check_put_domain, get, put};
use panproto_reference::pages::lenses::{
    Bindings, ChildComplement, Content, Function, Operation, Predicate, Record, child_get,
    child_put, child_put_solutions,
};
use panproto_reference::value::{Presence, Value as RefValue};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

const FIELDS: [&str; 3] = ["k", "m", "n"];

// ---------------------------------------------------------------------------
// Expressions, as production evaluates them and as the reference does
// ---------------------------------------------------------------------------

/// An integer expression over the values of fields.
#[derive(Clone, Debug)]
enum Term {
    Int(i64),
    Null,
    Field(&'static str),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Mul(Box<Self>, Box<Self>),
    Text(Box<Self>),
    Number(Box<Self>),
    If(Test, Box<Self>, Box<Self>),
}

/// A condition on the values of fields.
#[derive(Clone, Debug)]
enum Test {
    True,
    Is(Box<Term>, Box<Term>),
    Below(Box<Term>, Box<Term>),
    Null(&'static str),
}

fn field(name: &'static str) -> Box<Term> {
    Box::new(Term::Field(name))
}

fn int(value: i64) -> Box<Term> {
    Box::new(Term::Int(value))
}

fn builtin(op: BuiltinOp, arguments: Vec<Expr>) -> Expr {
    Expr::Builtin(op, arguments)
}

impl Test {
    fn expr(&self) -> Expr {
        match self {
            Self::True => Expr::Lit(Literal::Bool(true)),
            Self::Is(a, b) => builtin(BuiltinOp::Eq, vec![a.expr(), b.expr()]),
            Self::Below(a, b) => builtin(BuiltinOp::Lt, vec![a.expr(), b.expr()]),
            Self::Null(name) => builtin(BuiltinOp::IsNull, vec![Expr::Var((*name).into())]),
        }
    }

    fn eval(&self, fields: &Bindings) -> Option<bool> {
        match self {
            Self::True => Some(true),
            Self::Is(a, b) => Some(a.eval(fields)? == b.eval(fields)?),
            Self::Below(a, b) => match (a.eval(fields)?, b.eval(fields)?) {
                (RefValue::Int(a), RefValue::Int(b)) => Some(a < b),
                (RefValue::Str(a), RefValue::Str(b)) => Some(a < b),
                _ => None,
            },
            Self::Null(name) => Some(*fields.get(*name)? == RefValue::Null),
        }
    }

    fn reference(&self) -> Predicate {
        let test = self.clone();
        Rc::new(move |fields| test.eval(fields))
    }
}

impl Term {
    fn expr(&self) -> Expr {
        match self {
            Self::Int(value) => Expr::Lit(Literal::Int(*value)),
            Self::Null => Expr::Lit(Literal::Null),
            Self::Field(name) => Expr::Var((*name).into()),
            Self::Add(a, b) => builtin(BuiltinOp::Add, vec![a.expr(), b.expr()]),
            Self::Sub(a, b) => builtin(BuiltinOp::Sub, vec![a.expr(), b.expr()]),
            Self::Mul(a, b) => builtin(BuiltinOp::Mul, vec![a.expr(), b.expr()]),
            Self::Text(a) => builtin(BuiltinOp::IntToStr, vec![a.expr()]),
            Self::Number(a) => builtin(BuiltinOp::StrToInt, vec![a.expr()]),
            Self::If(test, then, or) => Expr::Match {
                scrutinee: Box::new(test.expr()),
                arms: vec![
                    (Pattern::Lit(Literal::Bool(true)), then.expr()),
                    (Pattern::Wildcard, or.expr()),
                ],
            },
        }
    }

    fn eval(&self, fields: &Bindings) -> Option<RefValue> {
        let ints = |a: &Self, b: &Self| match (a.eval(fields)?, b.eval(fields)?) {
            (RefValue::Int(a), RefValue::Int(b)) => Some((a, b)),
            _ => None,
        };
        Some(match self {
            Self::Int(value) => RefValue::Int(*value),
            Self::Null => RefValue::Null,
            Self::Field(name) => fields.get(*name)?.clone(),
            Self::Add(a, b) => RefValue::Int(ints(a, b).map(|(a, b)| a + b)?),
            Self::Sub(a, b) => RefValue::Int(ints(a, b).map(|(a, b)| a - b)?),
            Self::Mul(a, b) => RefValue::Int(ints(a, b).map(|(a, b)| a * b)?),
            Self::Text(a) => match a.eval(fields)? {
                RefValue::Int(a) => RefValue::Str(a.to_string()),
                _ => return None,
            },
            Self::Number(a) => match a.eval(fields)? {
                RefValue::Str(a) => RefValue::Int(a.parse().ok()?),
                _ => return None,
            },
            Self::If(test, then, or) => {
                return if test.eval(fields)? { then } else { or }.eval(fields);
            }
        })
    }

    fn reference(&self) -> Function {
        let term = self.clone();
        Rc::new(move |fields| term.eval(fields))
    }
}

/// One operation of a program, in both forms.
#[derive(Clone, Debug)]
enum Step {
    Rewrite(&'static str, Term, Option<Term>),
    Compute(&'static str, Term, Option<Term>),
    Rename(&'static str, &'static str),
    Drop(&'static str),
    Add(&'static str, i64),
    Keep(&'static [&'static str]),
    Case(Vec<(Test, Vec<Self>)>),
}

impl Step {
    fn transform(&self) -> FieldTransform {
        let class = |inverse: &Option<Term>| {
            if inverse.is_some() {
                CoercionClass::Iso
            } else {
                CoercionClass::Opaque
            }
        };
        match self {
            Self::Rewrite(key, to, from) => FieldTransform::ApplyExpr {
                key: (*key).to_owned(),
                expr: to.expr(),
                inverse: from.as_ref().map(Term::expr),
                coercion_class: class(from),
            },
            Self::Compute(key, to, from) => FieldTransform::ComputeField {
                target_key: (*key).to_owned(),
                expr: to.expr(),
                inverse: from.as_ref().map(Term::expr),
                coercion_class: class(from),
            },
            Self::Rename(old, new) => FieldTransform::RenameField {
                old_key: (*old).to_owned(),
                new_key: (*new).to_owned(),
            },
            Self::Drop(key) => FieldTransform::DropField {
                key: (*key).to_owned(),
            },
            Self::Add(key, value) => FieldTransform::AddField {
                key: (*key).to_owned(),
                value: Value::Int(*value),
            },
            Self::Keep(keys) => FieldTransform::KeepFields {
                keys: keys.iter().map(|key| (*key).to_owned()).collect(),
            },
            Self::Case(branches) => FieldTransform::Case {
                branches: branches
                    .iter()
                    .map(|(test, steps)| CaseBranch {
                        predicate: test.expr(),
                        transforms: steps.iter().map(Self::transform).collect(),
                    })
                    .collect(),
            },
        }
    }

    fn reference(&self) -> Operation {
        match self {
            Self::Rewrite(key, to, from) => Operation::Rewrite {
                key: (*key).to_owned(),
                to: to.reference(),
                from: from.as_ref().map(Term::reference),
            },
            Self::Compute(key, to, from) => Operation::Compute {
                key: (*key).to_owned(),
                to: to.reference(),
                from: from.as_ref().map(Term::reference),
            },
            Self::Rename(old, new) => Operation::Rename {
                from: (*old).to_owned(),
                to: (*new).to_owned(),
            },
            Self::Drop(key) => Operation::Drop((*key).to_owned()),
            Self::Add(key, value) => Operation::Add((*key).to_owned(), RefValue::Int(*value)),
            Self::Keep(keys) => {
                Operation::Keep(keys.iter().map(|key| (*key).to_owned()).collect())
            }
            Self::Case(branches) => Operation::Case(
                branches
                    .iter()
                    .map(|(test, steps)| {
                        (
                            test.reference(),
                            steps.iter().map(Self::reference).collect(),
                        )
                    })
                    .collect(),
            ),
        }
    }
}

fn successor(key: &'static str, by: i64) -> Step {
    Step::Rewrite(
        key,
        Term::Add(field(key), int(by)),
        Some(Term::Sub(field(key), int(by))),
    )
}

fn steps() -> Vec<Step> {
    let double = |key| Term::Mul(field(key), int(2));
    let or_zero = |key| Term::If(Test::Null(key), int(0), field(key));
    let is = |key, value| Test::Is(field(key), int(value));
    vec![
        successor("n", 1),
        Step::Rewrite("n", double("n"), None),
        Step::Rewrite("n", Term::Mul(field("n"), field("n")), Some(Term::Field("n"))),
        Step::Rewrite("n", Term::Int(5), None),
        // Clamped below at zero, without an inverse and with one that is none.
        Step::Rewrite(
            "n",
            Term::If(Test::Below(field("n"), int(0)), int(0), field("n")),
            None,
        ),
        Step::Rewrite(
            "n",
            Term::If(
                Test::Below(field("n"), int(0)),
                Box::new(Term::Sub(int(0), field("n"))),
                field("n"),
            ),
            Some(Term::Sub(int(0), field("n"))),
        ),
        // One value renamed to another.
        Step::Rewrite(
            "n",
            Term::If(is("n", 1), int(2), field("n")),
            Some(Term::If(is("n", 2), int(1), field("n"))),
        ),
        // Null shown as zero, with an inverse and without.
        Step::Rewrite(
            "n",
            or_zero("n"),
            Some(Term::If(is("n", 0), Box::new(Term::Null), field("n"))),
        ),
        Step::Rewrite("n", or_zero("n"), None),
        Step::Rewrite("n", Term::Field("n"), Some(Term::Field("n"))),
        Step::Rewrite("n", Term::If(is("n", 2), Box::new(Term::Null), field("n")), None),
        // Another kind of value, and an inverse that reads a sibling.
        Step::Rewrite(
            "n",
            Term::Text(field("n")),
            Some(Term::Number(field("n"))),
        ),
        Step::Rewrite(
            "n",
            Term::Add(field("n"), int(1)),
            Some(Term::Sub(field("n"), field("m"))),
        ),
        Step::Rewrite("m", Term::Field("m"), Some(Term::Field("m"))),
        successor("m", 10),
        Step::Rewrite("m", double("m"), None),
        Step::Rename("n", "k"),
        Step::Drop("n"),
        Step::Add("n", 4),
        Step::Compute("k", Term::Add(field("n"), int(1)), None),
        Step::Compute("n", Term::Add(field("n"), int(1)), None),
        Step::Compute(
            "n",
            Term::Add(field("n"), int(1)),
            Some(Term::Sub(field("n"), int(1))),
        ),
        Step::Compute("n", Term::Add(field("m"), int(1)), None),
        Step::Keep(&["m", "n"]),
        Step::Case(vec![
            (is("n", 0), vec![Step::Rewrite("n", double("n"), None)]),
            (Test::True, vec![successor("n", 1)]),
        ]),
        Step::Case(vec![(is("m", 1), vec![successor("n", 1)])]),
        Step::Case(vec![
            (is("m", 1), vec![successor("n", 1)]),
            (Test::True, vec![successor("n", 2)]),
        ]),
        Step::Case(vec![(is("n", 4), vec![successor("m", 10)])]),
        Step::Case(vec![(is("n", 1), vec![Step::Rewrite("n", Term::Int(2), None)])]),
    ]
}

// ---------------------------------------------------------------------------
// Records and trees
// ---------------------------------------------------------------------------

fn schema() -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("r", "object", None)
        .unwrap();
    for name in FIELDS {
        let leaf = format!("r.{name}");
        builder = builder
            .vertex(&leaf, "integer", None)
            .unwrap()
            .edge("r", &leaf, "prop", Some(name))
            .unwrap();
    }
    builder.build().unwrap()
}

fn lens(schema: &Schema, program: &[Step]) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect::<HashSet<_>>(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms: [("r".into(), program.iter().map(Step::transform).collect())].into(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

fn value(value: &RefValue) -> Value {
    match value {
        RefValue::Int(value) => Value::Int(*value),
        RefValue::Str(value) => Value::Str(value.clone()),
        RefValue::Null => Value::Null,
        RefValue::Unknown(fields) => {
            Value::Unknown(fields.iter().map(|(k, v)| (k.clone(), self::value(v))).collect())
        }
        other => panic!("{other:?} is outside the carrier"),
    }
}

fn reference_value(value: &Value) -> RefValue {
    match value {
        Value::Int(value) => RefValue::Int(*value),
        Value::Str(value) => RefValue::Str(value.clone()),
        Value::Null => RefValue::Null,
        Value::Unknown(fields) => RefValue::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), reference_value(v)))
                .collect(),
        ),
        other => panic!("{other:?} is outside the carrier"),
    }
}

/// The tree of a record, its children under the ids `ids` gives them.
fn tree(record: &Record, ids: &BTreeMap<String, u32>) -> WInstance {
    let mut root = Node::new(0, "r");
    root.extra_fields = record
        .own
        .iter()
        .map(|(key, held)| (key.clone(), value(held)))
        .collect();
    let mut nodes = HashMap::from([(0, root)]);
    let mut arcs = Vec::new();
    for (key, content) in &record.children {
        let leaf = format!("r.{key}");
        let mut node = Node::new(ids[key], leaf.as_str());
        match content {
            Content::Value(Presence::Present(held)) => {
                node.value = Some(FieldPresence::Present(value(held)));
            }
            Content::Value(Presence::Null) => node.value = Some(FieldPresence::Null),
            Content::Value(Presence::Absent) => node.value = Some(FieldPresence::Absent),
            Content::Fields(fields) => {
                node.extra_fields = fields.iter().map(|(k, v)| (k.clone(), value(v))).collect();
            }
        }
        nodes.insert(ids[key], node);
        arcs.push((
            0,
            ids[key],
            Edge {
                src: "r".into(),
                tgt: leaf.as_str().into(),
                kind: "prop".into(),
                name: Some(key.as_str().into()),
            },
        ));
    }
    WInstance::new(nodes, arcs, Vec::new(), 0, "r".into())
}

fn record(tree: &WInstance) -> Record {
    let mut record = Record {
        own: tree.nodes[&0]
            .extra_fields
            .iter()
            .map(|(key, held)| (key.clone(), reference_value(held)))
            .collect(),
        children: BTreeMap::new(),
    };
    for (_, child, edge) in &tree.arcs {
        let node = &tree.nodes[child];
        let content = match &node.value {
            Some(FieldPresence::Present(held)) => {
                Content::Value(Presence::Present(reference_value(held)))
            }
            Some(FieldPresence::Null) => Content::Value(Presence::Null),
            Some(FieldPresence::Absent) => Content::Value(Presence::Absent),
            None => Content::Fields(
                node.extra_fields
                    .iter()
                    .map(|(k, v)| (k.clone(), reference_value(v)))
                    .collect(),
            ),
        };
        record
            .children
            .insert(edge.name.as_deref().unwrap().to_owned(), content);
    }
    record
}

fn present(held: i64) -> Content {
    Content::Value(Presence::Present(RefValue::Int(held)))
}

fn text(held: &str) -> Content {
    Content::Value(Presence::Present(RefValue::Str(held.to_owned())))
}

fn fields(held: i64) -> Content {
    Content::Fields(BTreeMap::from([("$link".to_owned(), RefValue::Int(held))]))
}

fn children(held: &[(&str, Content)]) -> Record {
    Record {
        own: Bindings::new(),
        children: held
            .iter()
            .map(|(key, content)| ((*key).to_owned(), content.clone()))
            .collect(),
    }
}

fn sources() -> Vec<Record> {
    let null = || Content::Value(Presence::Null);
    vec![
        children(&[("n", present(0))]),
        children(&[("n", present(1)), ("m", present(3))]),
        children(&[("n", present(2)), ("k", present(7))]),
        children(&[("m", present(3))]),
        children(&[("n", present(3)), ("m", present(1))]),
        children(&[("n", null()), ("m", present(1))]),
        children(&[("n", present(-2)), ("m", present(1))]),
        children(&[("n", fields(1)), ("m", present(1))]),
        children(&[("n", Content::Value(Presence::Absent)), ("m", present(1))]),
    ]
}

/// Edits of a view: each a set of children to hold other contents, or
/// nothing.
fn edits() -> Vec<Vec<(&'static str, Option<Content>)>> {
    let null = || Content::Value(Presence::Null);
    let mut edits = vec![Vec::new()];
    for content in [
        present(-1),
        present(0),
        present(1),
        present(2),
        present(4),
        present(9),
        null(),
        text("5"),
        fields(2),
    ] {
        edits.push(vec![("n", Some(content))]);
    }
    for content in [present(1), present(2), present(12), null()] {
        edits.push(vec![("m", Some(content))]);
    }
    edits.push(vec![("k", Some(present(5)))]);
    edits.push(vec![("n", None)]);
    edits.push(vec![("m", None)]);
    for (n, m) in [(9, 2), (4, 11), (8, 2), (5, 2), (4, 2)] {
        edits.push(vec![("n", Some(present(n))), ("m", Some(present(m)))]);
    }
    edits
}

fn edited(view: &Record, edit: &[(&'static str, Option<Content>)]) -> Record {
    let mut edited = view.clone();
    for (key, content) in edit {
        match content {
            Some(content) => {
                edited.children.insert((*key).to_owned(), content.clone());
            }
            None => {
                edited.children.remove(*key);
            }
        }
    }
    edited
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Counts {
    /// Programs and sources of which get is defined, and not.
    got: u32,
    not_got: u32,
    /// Views put accepts and refuses.
    accepted: u32,
    refused: u32,
    /// Views put refuses though a combination of candidates is admitted.
    missed: u32,
}

/// The ids of a source's children, and fresh ids for any the view adds.
fn ids(source: &Record) -> BTreeMap<String, u32> {
    let mut ids: BTreeMap<String, u32> = (1..)
        .zip(source.children.keys())
        .map(|(id, key)| (key.clone(), id))
        .collect();
    for (fresh, name) in (10..).zip(FIELDS) {
        ids.entry(name.to_owned()).or_insert(fresh);
    }
    ids
}

/// Compare the recorded content of production's complement with the
/// reference's.
fn check_recorded(complement: &Complement, reference: &ChildComplement, ids: &BTreeMap<String, u32>) {
    let recorded: BTreeMap<u32, Content> = reference
        .recorded
        .iter()
        .map(|(key, content)| (ids[key], content.clone()))
        .collect();
    let found: BTreeMap<u32, Content> = complement
        .original_values
        .iter()
        .map(|(id, held)| {
            let content = match held {
                Some(FieldPresence::Present(held)) => {
                    Content::Value(Presence::Present(reference_value(held)))
                }
                Some(FieldPresence::Null) => Content::Value(Presence::Null),
                Some(FieldPresence::Absent) => Content::Value(Presence::Absent),
                None => Content::Fields(
                    complement.original_extra_fields[id]
                        .iter()
                        .map(|(k, v)| (k.clone(), reference_value(v)))
                        .collect(),
                ),
            };
            (*id, content)
        })
        .collect();
    assert_eq!(found, recorded);
}

struct Smallest(Option<(usize, String)>);

impl Smallest {
    fn offer(&mut self, size: usize, case: impl Fn() -> String) {
        if self.0.as_ref().is_none_or(|(least, _)| size < *least) {
            self.0 = Some((size, case()));
        }
    }
}

fn check_program(
    schema: &Schema,
    program: &[Step],
    sources: &[Record],
    edits: &[Vec<(&'static str, Option<Content>)>],
    counts: &mut Counts,
    smallest: &mut Smallest,
) {
    let lens = lens(schema, program);
    let operations: Vec<Operation> = program.iter().map(Step::reference).collect();
    for source in sources {
        let ids = ids(source);
        let source_tree = tree(source, &ids);
        let case = || format!("{program:?} over {source:?}");
        let got = get(&lens, &source_tree);
        let Some((reference_view, reference_complement)) = child_get(&operations, source) else {
            assert!(got.is_err(), "the reference refuses get: {}", case());
            counts.not_got += 1;
            continue;
        };
        counts.got += 1;
        let (view, complement) = got.unwrap_or_else(|error| panic!("{error}: {}", case()));
        assert_eq!(record(&view), reference_view, "{}", case());
        check_recorded(&complement, &reference_complement, &ids);

        // GetPut, in memory and through a serialization.
        let restored = put(&lens, &view, &complement)
            .unwrap_or_else(|error| panic!("GetPut refused: {error}: {}", case()));
        assert_eq!(record(&restored), *source, "GetPut: {}", case());
        let serialized = reference_view.serialized();
        let inserted = serialized
            .own
            .keys()
            .any(|key| program_renames_to(program, key));
        if !inserted {
            let restored = put(&lens, &tree(&serialized, &ids), &complement)
                .unwrap_or_else(|error| panic!("serialized GetPut refused: {error}: {}", case()));
            assert_eq!(record(&restored), *source, "serialized GetPut: {}", case());
        }

        let mut views = BTreeSet::new();
        for form in [reference_view.clone(), serialized] {
            for edit in edits {
                let shown = edited(&form, edit);
                if !views.insert(format!("{shown:?}")) {
                    continue;
                }
                let shown_tree = tree(&shown, &ids);
                let expected = child_put(&operations, &shown, &reference_complement);
                let what = || format!("{} showing {shown:?}", case());
                let found = put(&lens, &shown_tree, &complement);
                assert_eq!(
                    found.is_ok(),
                    check_put_domain(&lens, &shown_tree, &complement).is_ok(),
                    "{}",
                    what()
                );
                match (found, expected) {
                    (Ok(found), Some(expected)) => {
                        assert_eq!(record(&found), expected, "{}", what());
                        // PutGet: get shows the view, up to where a field
                        // that a child alone holds is held.
                        let (again, _) = get(&lens, &found).unwrap();
                        let again = record(&again);
                        assert!(again.presents(&shown), "PutGet: {} shows {again:?}", what());
                        for (key, held) in &shown.own {
                            assert_eq!(again.own.get(key), Some(held), "{}", what());
                            assert_eq!(
                                again.children.get(key),
                                shown.children.get(key),
                                "{}",
                                what()
                            );
                        }
                        counts.accepted += 1;
                    }
                    (Err(LensError::PutDomain(_)), None) => {
                        counts.refused += 1;
                        let sources =
                            child_put_solutions(&operations, &shown, &reference_complement);
                        if let Some(solution) = sources.first() {
                            counts.missed += 1;
                            smallest.offer(program.len() * 1000 + source.children.len(), || {
                                format!("{}, admitted: {solution:?}", what())
                            });
                        }
                    }
                    (found, expected) => {
                        panic!("{}: {found:?} against {expected:?}", what())
                    }
                }
            }
        }
    }
}

fn program_renames_to(program: &[Step], key: &str) -> bool {
    program
        .iter()
        .any(|step| matches!(step, Step::Rename(_, new) if *new == key))
}

#[test]
fn tree_put_agrees_with_the_reference_on_fields_of_children() {
    let schema = schema();
    let steps = steps();
    let (sources, edits) = (sources(), edits());
    let mut counts = Counts::default();
    let mut smallest = Smallest(None);
    check_program(&schema, &[], &sources, &edits, &mut counts, &mut smallest);
    for first in &steps {
        let program = [first.clone()];
        check_program(&schema, &program, &sources, &edits, &mut counts, &mut smallest);
        for second in &steps {
            let program = [first.clone(), second.clone()];
            check_program(&schema, &program, &sources, &edits, &mut counts, &mut smallest);
        }
    }
    println!("{counts:?}\nsmallest missed: {:?}", smallest.0);
    assert_eq!(counts, Counts::default());
}
