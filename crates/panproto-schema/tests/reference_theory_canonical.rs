//! Same-input differential for canonical theory and composition bytes.

#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use panproto_expr::exact::{BigIntValue, DecimalValue, Temporal, WideInt};
use panproto_expr::{BuiltinOp, Env, Expr, Literal, Pattern};
use panproto_gat::{
    AddedEquation, CaseBranch, CheckOutcome, CheckRecord, CheckSubject, CoercionClass, Composition,
    CompositionCheckKind, CompositionIncomplete, CompositionMode, CompositionReport,
    ConflictPolicy, ConflictStrategy, DirectedEquation, Equation, Freshening, Implicit,
    InapplicableReason, OpAssignment, Operation, Provenance, Sort, SortClosure, SortExpr, SortKind,
    SortParam, SymKind, SymRef, Term, Theory, TheoryMorphism, ValueKind,
};
use panproto_reference::exact as reference_exact;
use panproto_reference::num::Nat;
use panproto_reference::temporal as reference_temporal;
use panproto_reference::theory as reference;
use panproto_reference::value::BuiltinOp as ReferenceBuiltin;
use panproto_schema::{canonical_composition_bytes, canonical_theory_bytes};

fn empty_theory() -> (Theory, reference::Theory) {
    (
        Theory::new("Empty", Vec::new(), Vec::new(), Vec::new()),
        reference::Theory {
            name: "Empty".to_owned(),
            ..reference::Theory::default()
        },
    )
}

fn unary_theory() -> (Theory, reference::Theory) {
    let production = Theory::extending(
        "Unary",
        vec![Arc::from("Base")],
        vec![Sort::simple("A")],
        vec![Operation::unary("f", "x", "A", "A")],
        vec![Equation::new(
            "f_identity",
            Term::App {
                op: "f".into(),
                args: vec![Term::Var("x".into())],
            },
            Term::Var("x".into()),
        )],
    );
    let oracle = reference::Theory {
        name: "Unary".to_owned(),
        extends: vec!["Base".to_owned()],
        sorts: vec![reference::Sort {
            name: "A".to_owned(),
            params: Vec::new(),
            kind: reference::SortKind::Structural,
            constructors: None,
        }],
        ops: vec![reference::Op {
            name: "f".to_owned(),
            inputs: vec![(
                "x".to_owned(),
                reference::SortExpr::Name("A".to_owned()),
                reference::Implicit::No,
            )],
            output: reference::SortExpr::Name("A".to_owned()),
        }],
        eqs: vec![reference::Equation {
            name: "f_identity".to_owned(),
            lhs: reference::Term::App("f".to_owned(), vec![reference::Term::Var("x".to_owned())]),
            rhs: reference::Term::Var("x".to_owned()),
        }],
        directed_eqs: Vec::new(),
        policies: Vec::new(),
    };
    (production, oracle)
}

fn literal_pair() -> (Literal, reference::Literal) {
    let production_decimal = DecimalValue::parse("1.50").expect("fixture decimal");
    let oracle_decimal = reference_exact::parse_decimal("1.50").expect("fixture decimal");
    let production_bigint = BigIntValue {
        negative: true,
        significand: "18446744073709551615".to_owned(),
        zeros: WideInt::zero(),
    };
    let oracle_bigint = reference_exact::BigIntValue {
        negative: true,
        significand: Nat::parse("18446744073709551615").expect("fixture digits"),
        zeros: Nat::zero(),
    };
    let production_temporal = Temporal::parse("2026-10-26").expect("fixture date");
    let oracle_temporal = reference::TemporalLiteral::new(
        reference_temporal::parse_any("2026-10-26").expect("fixture date"),
    )
    .expect("well-formed reference date");
    let production_closure = Literal::Closure {
        param: "captured".into(),
        body: Box::new(Expr::Var("captured".into())),
        env: Env::new().extend("captured".into(), Literal::Int(7)),
    };
    let oracle_closure = reference::Literal::Closure {
        param: "captured".to_owned(),
        body: Box::new(reference::Expression::Var("captured".to_owned())),
        env: BTreeMap::from([("captured".to_owned(), reference::Literal::Int(7))]),
    };
    (
        Literal::Record(vec![
            ("bool".into(), Literal::Bool(true)),
            ("int".into(), Literal::Int(-9)),
            (
                "float".into(),
                Literal::Float(f64::from_bits(0x7ff8_0000_0000_0042)),
            ),
            ("str".into(), Literal::Str("value".to_owned())),
            ("bytes".into(), Literal::Bytes(vec![0, 1, 255])),
            ("null".into(), Literal::Null),
            (
                "record".into(),
                Literal::Record(vec![("field".into(), Literal::Int(1))]),
            ),
            (
                "list".into(),
                Literal::List(vec![Literal::Bool(false), Literal::Null]),
            ),
            ("closure".into(), production_closure),
            ("decimal".into(), Literal::Decimal(production_decimal)),
            ("bigint".into(), Literal::BigInt(production_bigint)),
            ("temporal".into(), Literal::Temporal(production_temporal)),
        ]),
        reference::Literal::Record(vec![
            ("bool".to_owned(), reference::Literal::Bool(true)),
            ("int".to_owned(), reference::Literal::Int(-9)),
            (
                "float".to_owned(),
                reference::Literal::Float(0x7ff8_0000_0000_0042),
            ),
            (
                "str".to_owned(),
                reference::Literal::Str("value".to_owned()),
            ),
            (
                "bytes".to_owned(),
                reference::Literal::Bytes(vec![0, 1, 255]),
            ),
            ("null".to_owned(), reference::Literal::Null),
            (
                "record".to_owned(),
                reference::Literal::Record(vec![("field".to_owned(), reference::Literal::Int(1))]),
            ),
            (
                "list".to_owned(),
                reference::Literal::List(vec![
                    reference::Literal::Bool(false),
                    reference::Literal::Null,
                ]),
            ),
            ("closure".to_owned(), oracle_closure),
            (
                "decimal".to_owned(),
                reference::Literal::Decimal(oracle_decimal),
            ),
            (
                "bigint".to_owned(),
                reference::Literal::BigInt(oracle_bigint),
            ),
            (
                "temporal".to_owned(),
                reference::Literal::Temporal(oracle_temporal),
            ),
        ]),
    )
}

#[allow(clippy::too_many_lines)]
fn expression_pair() -> (Expr, reference::Expression) {
    let (production_literal, oracle_literal) = literal_pair();
    let production_patterns = vec![
        Pattern::Wildcard,
        Pattern::Var("bound".into()),
        Pattern::Lit(Literal::Int(1)),
        Pattern::Record(vec![("field".into(), Pattern::Wildcard)]),
        Pattern::List(vec![Pattern::Var("item".into())]),
        Pattern::Constructor("Some".into(), vec![Pattern::Var("value".into())]),
    ];
    let oracle_patterns = vec![
        reference::Pattern::Wildcard,
        reference::Pattern::Var("bound".to_owned()),
        reference::Pattern::Lit(reference::Literal::Int(1)),
        reference::Pattern::Record(vec![("field".to_owned(), reference::Pattern::Wildcard)]),
        reference::Pattern::List(vec![reference::Pattern::Var("item".to_owned())]),
        reference::Pattern::Constructor(
            "Some".to_owned(),
            vec![reference::Pattern::Var("value".to_owned())],
        ),
    ];
    let production_match = Expr::Match {
        scrutinee: Box::new(Expr::Var("scrutinee".into())),
        arms: production_patterns
            .into_iter()
            .enumerate()
            .map(|(index, pattern)| {
                (
                    pattern,
                    Expr::Lit(Literal::Int(
                        i64::try_from(index).expect("bounded pattern index"),
                    )),
                )
            })
            .collect(),
    };
    let oracle_match = reference::Expression::Match {
        scrutinee: Box::new(reference::Expression::Var("scrutinee".to_owned())),
        arms: oracle_patterns
            .into_iter()
            .enumerate()
            .map(|(index, pattern)| {
                (
                    pattern,
                    reference::Expression::Lit(reference::Literal::Int(
                        i64::try_from(index).expect("bounded pattern index"),
                    )),
                )
            })
            .collect(),
    };
    (
        Expr::Record(vec![
            ("var".into(), Expr::Var("x".into())),
            (
                "lam".into(),
                Expr::Lam("x".into(), Box::new(Expr::Var("x".into()))),
            ),
            (
                "app".into(),
                Expr::App(
                    Box::new(Expr::Var("f".into())),
                    Box::new(Expr::Var("x".into())),
                ),
            ),
            ("lit".into(), Expr::Lit(production_literal)),
            (
                "record".into(),
                Expr::Record(vec![("field".into(), Expr::Var("x".into()))]),
            ),
            ("list".into(), Expr::List(vec![Expr::Var("x".into())])),
            (
                "field".into(),
                Expr::Field(Box::new(Expr::Var("record".into())), "field".into()),
            ),
            (
                "index".into(),
                Expr::Index(
                    Box::new(Expr::Var("list".into())),
                    Box::new(Expr::Lit(Literal::Int(0))),
                ),
            ),
            ("match".into(), production_match),
            (
                "let".into(),
                Expr::Let {
                    name: "local".into(),
                    value: Box::new(Expr::Lit(Literal::Int(1))),
                    body: Box::new(Expr::Var("local".into())),
                },
            ),
            (
                "builtin".into(),
                Expr::Builtin(
                    BuiltinOp::Neq,
                    vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(2))],
                ),
            ),
        ]),
        reference::Expression::Record(vec![
            ("var".to_owned(), reference::Expression::Var("x".to_owned())),
            (
                "lam".to_owned(),
                reference::Expression::Lam(
                    "x".to_owned(),
                    Box::new(reference::Expression::Var("x".to_owned())),
                ),
            ),
            (
                "app".to_owned(),
                reference::Expression::App(
                    Box::new(reference::Expression::Var("f".to_owned())),
                    Box::new(reference::Expression::Var("x".to_owned())),
                ),
            ),
            ("lit".to_owned(), reference::Expression::Lit(oracle_literal)),
            (
                "record".to_owned(),
                reference::Expression::Record(vec![(
                    "field".to_owned(),
                    reference::Expression::Var("x".to_owned()),
                )]),
            ),
            (
                "list".to_owned(),
                reference::Expression::List(vec![reference::Expression::Var("x".to_owned())]),
            ),
            (
                "field".to_owned(),
                reference::Expression::Field(
                    Box::new(reference::Expression::Var("record".to_owned())),
                    "field".to_owned(),
                ),
            ),
            (
                "index".to_owned(),
                reference::Expression::Index(
                    Box::new(reference::Expression::Var("list".to_owned())),
                    Box::new(reference::Expression::Lit(reference::Literal::Int(0))),
                ),
            ),
            ("match".to_owned(), oracle_match),
            (
                "let".to_owned(),
                reference::Expression::Let {
                    name: "local".to_owned(),
                    value: Box::new(reference::Expression::Lit(reference::Literal::Int(1))),
                    body: Box::new(reference::Expression::Var("local".to_owned())),
                },
            ),
            (
                "builtin".to_owned(),
                reference::Expression::Builtin(
                    ReferenceBuiltin::Ne,
                    vec![
                        reference::Expression::Lit(reference::Literal::Int(1)),
                        reference::Expression::Lit(reference::Literal::Int(2)),
                    ],
                ),
            ),
        ]),
    )
}

fn sort_kind_pairs() -> Vec<(SortKind, reference::SortKind)> {
    let values = [
        (ValueKind::Bool, reference::ValueKind::Bool),
        (ValueKind::Int, reference::ValueKind::Int),
        (ValueKind::Float, reference::ValueKind::Float),
        (ValueKind::Str, reference::ValueKind::Str),
        (ValueKind::Bytes, reference::ValueKind::Bytes),
        (ValueKind::Token, reference::ValueKind::Token),
        (ValueKind::Null, reference::ValueKind::Null),
        (ValueKind::Any, reference::ValueKind::Any),
        (ValueKind::DateTime, reference::ValueKind::DateTime),
        (ValueKind::Date, reference::ValueKind::Date),
        (ValueKind::Time, reference::ValueKind::Time),
        (ValueKind::Decimal, reference::ValueKind::Decimal),
        (ValueKind::Uuid, reference::ValueKind::Uuid),
    ];
    let mut pairs = vec![(SortKind::Structural, reference::SortKind::Structural)];
    pairs.extend(
        values.map(|(production, oracle)| {
            (SortKind::Val(production), reference::SortKind::Val(oracle))
        }),
    );
    pairs.extend([
        (
            SortKind::Coercion {
                from: ValueKind::Int,
                to: ValueKind::Float,
                class: CoercionClass::Iso,
            },
            reference::SortKind::Coercion {
                from: reference::ValueKind::Int,
                to: reference::ValueKind::Float,
                class: reference::CoercionClass::Iso,
            },
        ),
        (
            SortKind::Coercion {
                from: ValueKind::Float,
                to: ValueKind::Str,
                class: CoercionClass::Retraction,
            },
            reference::SortKind::Coercion {
                from: reference::ValueKind::Float,
                to: reference::ValueKind::Str,
                class: reference::CoercionClass::Retraction,
            },
        ),
        (
            SortKind::Coercion {
                from: ValueKind::Str,
                to: ValueKind::Bytes,
                class: CoercionClass::Projection,
            },
            reference::SortKind::Coercion {
                from: reference::ValueKind::Str,
                to: reference::ValueKind::Bytes,
                class: reference::CoercionClass::Projection,
            },
        ),
        (
            SortKind::Coercion {
                from: ValueKind::Bytes,
                to: ValueKind::Any,
                class: CoercionClass::Opaque,
            },
            reference::SortKind::Coercion {
                from: reference::ValueKind::Bytes,
                to: reference::ValueKind::Any,
                class: reference::CoercionClass::Opaque,
            },
        ),
        (
            SortKind::Merger(ValueKind::Uuid),
            reference::SortKind::Merger(reference::ValueKind::Uuid),
        ),
    ]);
    pairs
}

#[allow(clippy::too_many_lines)]
fn exhaustive_theory() -> (Theory, reference::Theory) {
    let kinds = sort_kind_pairs();
    let production_sorts = kinds
        .iter()
        .enumerate()
        .map(|(index, (kind, _))| Sort {
            name: format!("kind_{index}").into(),
            params: if index == 0 {
                vec![SortParam::new(
                    "p",
                    SortExpr::App {
                        name: "kind_0".into(),
                        args: Vec::new(),
                    },
                )]
            } else {
                Vec::new()
            },
            kind: kind.clone(),
            closure: if index == 0 {
                SortClosure::Closed(vec!["constructor".into()])
            } else {
                SortClosure::Open
            },
        })
        .collect::<Vec<_>>();
    let oracle_sorts = kinds
        .iter()
        .enumerate()
        .map(|(index, (_, kind))| reference::Sort {
            name: format!("kind_{index}"),
            params: if index == 0 {
                vec![(
                    "p".to_owned(),
                    reference::SortExpr::App("kind_0".to_owned(), Vec::new()),
                )]
            } else {
                Vec::new()
            },
            kind: *kind,
            constructors: (index == 0).then(|| vec!["constructor".to_owned()]),
        })
        .collect::<Vec<_>>();

    let production_lhs = Term::Case {
        scrutinee: Box::new(Term::Var("scrutinee".into())),
        branches: vec![CaseBranch {
            constructor: "constructor".into(),
            binders: vec!["bound".into()],
            body: Term::Let {
                name: "local".into(),
                bound: Box::new(Term::Hole { name: None }),
                body: Box::new(Term::App {
                    op: "constructor".into(),
                    args: vec![Term::Var("local".into())],
                }),
            },
        }],
    };
    let oracle_lhs = reference::Term::Case(
        Box::new(reference::Term::Var("scrutinee".to_owned())),
        vec![reference::CaseBranch {
            constructor: "constructor".to_owned(),
            binders: vec!["bound".to_owned()],
            body: reference::Term::Let(
                "local".to_owned(),
                Box::new(reference::Term::Hole(None)),
                Box::new(reference::Term::App(
                    "constructor".to_owned(),
                    vec![reference::Term::Var("local".to_owned())],
                )),
            ),
        }],
    );
    let (production_expression, oracle_expression) = expression_pair();
    let production_directed = DirectedEquation {
        name: "rewrite".into(),
        lhs: production_lhs,
        rhs: Term::Hole {
            name: Some("rhs".into()),
        },
        impl_term: production_expression.clone(),
        inverse: Some(Expr::Var("inverse".into())),
        source_kind: Some(ValueKind::Int),
        target_kind: Some(ValueKind::Decimal),
        coercion_class: CoercionClass::Projection,
    };
    let oracle_directed = reference::DirectedEquation {
        name: "rewrite".to_owned(),
        lhs: oracle_lhs,
        rhs: reference::Term::Hole(Some("rhs".to_owned())),
        implementation: oracle_expression.clone(),
        inverse: Some(reference::Expression::Var("inverse".to_owned())),
        source_kind: Some(reference::ValueKind::Int),
        target_kind: Some(reference::ValueKind::Decimal),
        coercion_class: reference::CoercionClass::Projection,
    };
    let production = Theory::full(
        "EveryCarrier",
        vec!["Parent".into()],
        production_sorts,
        vec![
            Operation::with_implicit(
                "tie_op",
                vec![("x".into(), SortExpr::Name("kind_0".into()), Implicit::No)],
                SortExpr::Name("kind_0".into()),
            ),
            Operation::with_implicit(
                "tie_op",
                vec![("x".into(), SortExpr::Name("kind_0".into()), Implicit::Yes)],
                SortExpr::App {
                    name: "kind_0".into(),
                    args: Vec::new(),
                },
            ),
        ],
        vec![
            Equation::new("tie_eq", Term::Var("x".into()), Term::Hole { name: None }),
            Equation::new(
                "tie_eq",
                Term::Hole {
                    name: Some("named".into()),
                },
                Term::Var("x".into()),
            ),
        ],
        vec![
            production_directed,
            DirectedEquation {
                name: "rewrite".into(),
                lhs: Term::Var("x".into()),
                rhs: Term::Var("x".into()),
                impl_term: Expr::Var("other".into()),
                inverse: None,
                source_kind: None,
                target_kind: None,
                coercion_class: CoercionClass::Iso,
            },
        ],
        vec![
            ConflictPolicy {
                name: "keep".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepLeft,
            },
            ConflictPolicy {
                name: "keep".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepRight,
            },
            ConflictPolicy {
                name: "fail".into(),
                value_kind: ValueKind::Bytes,
                strategy: ConflictStrategy::Fail,
            },
            ConflictPolicy {
                name: "custom".into(),
                value_kind: ValueKind::Any,
                strategy: ConflictStrategy::Custom(production_expression),
            },
        ],
    );
    let oracle = reference::Theory {
        name: "EveryCarrier".to_owned(),
        extends: vec!["Parent".to_owned()],
        sorts: oracle_sorts,
        ops: vec![
            reference::Op {
                name: "tie_op".to_owned(),
                inputs: vec![(
                    "x".to_owned(),
                    reference::SortExpr::Name("kind_0".to_owned()),
                    reference::Implicit::No,
                )],
                output: reference::SortExpr::Name("kind_0".to_owned()),
            },
            reference::Op {
                name: "tie_op".to_owned(),
                inputs: vec![(
                    "x".to_owned(),
                    reference::SortExpr::Name("kind_0".to_owned()),
                    reference::Implicit::Yes,
                )],
                output: reference::SortExpr::App("kind_0".to_owned(), Vec::new()),
            },
        ],
        eqs: vec![
            reference::Equation {
                name: "tie_eq".to_owned(),
                lhs: reference::Term::Var("x".to_owned()),
                rhs: reference::Term::Hole(None),
            },
            reference::Equation {
                name: "tie_eq".to_owned(),
                lhs: reference::Term::Hole(Some("named".to_owned())),
                rhs: reference::Term::Var("x".to_owned()),
            },
        ],
        directed_eqs: vec![
            oracle_directed,
            reference::DirectedEquation {
                name: "rewrite".to_owned(),
                lhs: reference::Term::Var("x".to_owned()),
                rhs: reference::Term::Var("x".to_owned()),
                implementation: reference::Expression::Var("other".to_owned()),
                inverse: None,
                source_kind: None,
                target_kind: None,
                coercion_class: reference::CoercionClass::Iso,
            },
        ],
        policies: vec![
            reference::Policy {
                name: "keep".to_owned(),
                value_kind: reference::ValueKind::Str,
                strategy: reference::ConflictStrategy::KeepLeft,
            },
            reference::Policy {
                name: "keep".to_owned(),
                value_kind: reference::ValueKind::Str,
                strategy: reference::ConflictStrategy::KeepRight,
            },
            reference::Policy {
                name: "fail".to_owned(),
                value_kind: reference::ValueKind::Bytes,
                strategy: reference::ConflictStrategy::Fail,
            },
            reference::Policy {
                name: "custom".to_owned(),
                value_kind: reference::ValueKind::Any,
                strategy: reference::ConflictStrategy::Custom(oracle_expression),
            },
        ],
    };
    (production, oracle)
}

fn composition(
    production_theory: Theory,
    oracle_theory: reference::Theory,
    production_mode: CompositionMode,
    oracle_mode: reference::CompositionMode,
) -> (Composition, reference::Composition) {
    let production_inclusion = TheoryMorphism::new(
        "incl_carrier",
        "CarrierNode",
        production_theory.name.clone(),
        HashMap::from([(
            <Arc<str>>::from("SourceSort"),
            <Arc<str>>::from("ResultSort"),
        )]),
        HashMap::from([
            (
                <Arc<str>>::from("renamed"),
                OpAssignment::Op(<Arc<str>>::from("result_op")),
            ),
            (
                <Arc<str>>::from("derived"),
                OpAssignment::Term(Term::Var("x".into())),
            ),
        ]),
    );
    let oracle_inclusion = reference::Morphism {
        name: "incl_carrier".to_owned(),
        domain: "CarrierNode".to_owned(),
        codomain: oracle_theory.name.clone(),
        sorts: BTreeMap::from([("SourceSort".to_owned(), "ResultSort".to_owned())]),
        ops: BTreeMap::from([
            (
                "renamed".to_owned(),
                reference::OpImage::Op("result_op".to_owned()),
            ),
            (
                "derived".to_owned(),
                reference::OpImage::Term(reference::Term::Var("x".to_owned())),
            ),
        ]),
    };

    let source = |name: &str| SymRef::new("CarrierNode", name);
    let oracle_source = |name: &str| reference::SymRef {
        node: "CarrierNode".to_owned(),
        name: name.to_owned(),
    };
    let mut production_provenance = Provenance {
        sorts: BTreeMap::from([(source("SourceSort"), "ResultSort".into())]),
        ops: BTreeMap::from([(source("renamed"), "result_op".into())]),
        eqs: BTreeMap::from([(source("equation"), "result_equation".into())]),
        directed_eqs: BTreeMap::from([(source("directed"), "result_directed".into())]),
        policies: BTreeMap::from([(source("policy"), "result_policy".into())]),
        added_eqs: BTreeMap::new(),
    };
    production_provenance.added_eqs.insert(
        "generated".into(),
        AddedEquation {
            edge: 2,
            op: source("derived"),
        },
    );
    let mut oracle_provenance = reference::Provenance {
        sorts: BTreeMap::from([(oracle_source("SourceSort"), "ResultSort".to_owned())]),
        ops: BTreeMap::from([(oracle_source("renamed"), "result_op".to_owned())]),
        eqs: BTreeMap::from([(oracle_source("equation"), "result_equation".to_owned())]),
        directed_eqs: BTreeMap::from([(oracle_source("directed"), "result_directed".to_owned())]),
        policies: BTreeMap::from([(oracle_source("policy"), "result_policy".to_owned())]),
        added_eqs: BTreeMap::new(),
    };
    oracle_provenance.added_eqs.insert(
        "generated".to_owned(),
        reference::AddedEquation {
            edge: 2,
            op: oracle_source("derived"),
        },
    );

    let production_report = CompositionReport::new(
        production_mode,
        false,
        vec![
            CheckRecord {
                check: CompositionCheckKind::LegIsMorphism,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Passed,
            },
            CheckRecord {
                check: CompositionCheckKind::NodeWellFormed,
                subject: CheckSubject::Node("CarrierNode".into()),
                outcome: CheckOutcome::PassedByEquation {
                    equation: "generated".into(),
                },
            },
            CheckRecord {
                check: CompositionCheckKind::ClassSignaturesAgree,
                subject: CheckSubject::Edge(2),
                outcome: CheckOutcome::Failed {
                    detail: "counterexample".to_owned(),
                },
            },
            CheckRecord {
                check: CompositionCheckKind::CoconeCommutes,
                subject: CheckSubject::Cocone(3),
                outcome: CheckOutcome::Inapplicable {
                    reason: InapplicableReason::OpenTheory {
                        missing_sorts: vec!["Free".into()],
                    },
                },
            },
            CheckRecord {
                check: CompositionCheckKind::InclusionIsMorphism,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Incomplete {
                    reason: CompositionIncomplete::RewriteBudgetExhausted {
                        equation: "bounded".into(),
                        steps: 17,
                    },
                },
            },
            CheckRecord {
                check: CompositionCheckKind::EquationImage,
                subject: CheckSubject::Node("CarrierNode".into()),
                outcome: CheckOutcome::Inapplicable {
                    reason: InapplicableReason::NoEquations,
                },
            },
            CheckRecord {
                check: CompositionCheckKind::EquationEntailed,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Incomplete {
                    reason: CompositionIncomplete::NoTerminatingRewriteSystem,
                },
            },
            CheckRecord {
                check: CompositionCheckKind::UniversalIdentity,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Passed,
            },
            CheckRecord {
                check: CompositionCheckKind::UniversalAgainst,
                subject: CheckSubject::Edge(1),
                outcome: CheckOutcome::PassedByEquation {
                    equation: "generated".into(),
                },
            },
            CheckRecord {
                check: CompositionCheckKind::ResultWellTyped,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Failed {
                    detail: "ill-typed witness".to_owned(),
                },
            },
            CheckRecord {
                check: CompositionCheckKind::NameClashFree,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Passed,
            },
        ],
        vec![Freshening {
            kind: SymKind::Op,
            members: vec![source("derived"), source("renamed")],
            candidate: "clash".into(),
            result: "CarrierNode.clash".into(),
        }],
    );
    let oracle_report = reference::CompositionReport {
        mode: oracle_mode,
        status: reference::CheckStatus::Failed,
        name_clash_free: false,
        checks: vec![
            reference::CheckRecord {
                check: reference::CompositionCheckKind::LegIsMorphism,
                subject: reference::CheckSubject::Diagram,
                outcome: reference::CheckOutcome::Passed,
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::NodeWellFormed,
                subject: reference::CheckSubject::Node("CarrierNode".to_owned()),
                outcome: reference::CheckOutcome::PassedByEquation {
                    equation: "generated".to_owned(),
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::ClassSignaturesAgree,
                subject: reference::CheckSubject::Edge(2),
                outcome: reference::CheckOutcome::Failed {
                    detail: "counterexample".to_owned(),
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::CoconeCommutes,
                subject: reference::CheckSubject::Cocone(3),
                outcome: reference::CheckOutcome::Inapplicable {
                    reason: reference::InapplicableReason::OpenTheory {
                        missing_sorts: vec!["Free".to_owned()],
                    },
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::InclusionIsMorphism,
                subject: reference::CheckSubject::Diagram,
                outcome: reference::CheckOutcome::Incomplete {
                    reason: reference::CompositionIncomplete::RewriteBudgetExhausted {
                        equation: "bounded".to_owned(),
                        steps: 17,
                    },
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::EquationImage,
                subject: reference::CheckSubject::Node("CarrierNode".to_owned()),
                outcome: reference::CheckOutcome::Inapplicable {
                    reason: reference::InapplicableReason::NoEquations,
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::EquationEntailed,
                subject: reference::CheckSubject::Diagram,
                outcome: reference::CheckOutcome::Incomplete {
                    reason: reference::CompositionIncomplete::NoTerminatingRewriteSystem,
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::UniversalIdentity,
                subject: reference::CheckSubject::Diagram,
                outcome: reference::CheckOutcome::Passed,
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::UniversalAgainst,
                subject: reference::CheckSubject::Edge(1),
                outcome: reference::CheckOutcome::PassedByEquation {
                    equation: "generated".to_owned(),
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::ResultWellTyped,
                subject: reference::CheckSubject::Diagram,
                outcome: reference::CheckOutcome::Failed {
                    detail: "ill-typed witness".to_owned(),
                },
            },
            reference::CheckRecord {
                check: reference::CompositionCheckKind::NameClashFree,
                subject: reference::CheckSubject::Diagram,
                outcome: reference::CheckOutcome::Passed,
            },
        ],
        freshened: vec![reference::Freshening {
            kind: reference::SymKind::Op,
            members: vec![oracle_source("derived"), oracle_source("renamed")],
            candidate: "clash".to_owned(),
            result: "CarrierNode.clash".to_owned(),
        }],
    };
    (
        Composition {
            theory: production_theory,
            inclusions: vec![production_inclusion],
            provenance: production_provenance,
            report: production_report,
        },
        reference::Composition {
            theory: oracle_theory,
            inclusions: vec![oracle_inclusion],
            provenance: oracle_provenance,
            report: oracle_report,
        },
    )
}

#[test]
fn canonical_theory_and_composition_bytes_match_reference_on_bounded_fixtures() {
    let fixtures = [empty_theory(), unary_theory(), exhaustive_theory()];
    assert_eq!(fixtures.len(), 3);
    for (production, oracle) in fixtures {
        assert_eq!(
            canonical_theory_bytes(&production),
            reference::canonical_theory_bytes(&oracle),
            "theory {}",
            production.name
        );
        for (production_mode, oracle_mode) in [
            (
                CompositionMode::Amalgamate,
                reference::CompositionMode::Amalgamate,
            ),
            (
                CompositionMode::ExplicitIdentity,
                reference::CompositionMode::ExplicitIdentity,
            ),
        ] {
            let (production_composition, oracle_composition) = composition(
                production.clone(),
                oracle.clone(),
                production_mode,
                oracle_mode,
            );
            assert_eq!(
                canonical_composition_bytes(&production_composition),
                reference::canonical_composition_bytes(&oracle_composition),
                "composition {} under {production_mode:?}",
                production.name
            );
        }
    }
}
