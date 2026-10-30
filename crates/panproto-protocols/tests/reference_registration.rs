//! Every registered protocol theory, run through the independent
//! reference: the reference signature check accepts it, the reference replay
//! of its registration recipe reproduces its canonical bytes, and the
//! reference composition of that recipe's diagram passes every check.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_protocols::registry::descriptors;
use panproto_reference::theory as r;
use panproto_schema::canonical_theory_bytes;

mod convert {
    //! Structural conversions from production theories to the reference model.
    //!
    //! Each one copies fields; none calls production composition.

    use super::r;
    use panproto_gat as p;
    use panproto_reference::{exact as rx, num::Nat, temporal as rt, value as rv};

    const fn back_value_kind(kind: p::ValueKind) -> r::ValueKind {
        match kind {
            p::ValueKind::Bool => r::ValueKind::Bool,
            p::ValueKind::Int => r::ValueKind::Int,
            p::ValueKind::Float => r::ValueKind::Float,
            p::ValueKind::Str => r::ValueKind::Str,
            p::ValueKind::Bytes => r::ValueKind::Bytes,
            p::ValueKind::Token => r::ValueKind::Token,
            p::ValueKind::Null => r::ValueKind::Null,
            p::ValueKind::Any => r::ValueKind::Any,
            p::ValueKind::DateTime => r::ValueKind::DateTime,
            p::ValueKind::Date => r::ValueKind::Date,
            p::ValueKind::Time => r::ValueKind::Time,
            p::ValueKind::Decimal => r::ValueKind::Decimal,
            p::ValueKind::Uuid => r::ValueKind::Uuid,
        }
    }

    fn back_coercion_class(class: p::CoercionClass) -> r::CoercionClass {
        match class {
            p::CoercionClass::Iso => r::CoercionClass::Iso,
            p::CoercionClass::Retraction => r::CoercionClass::Retraction,
            p::CoercionClass::Projection => r::CoercionClass::Projection,
            p::CoercionClass::Opaque => r::CoercionClass::Opaque,
            _ => panic!("unknown production coercion class: {class:?}"),
        }
    }

    pub fn back_kind(kind: &p::SortKind) -> r::SortKind {
        match kind {
            p::SortKind::Structural => r::SortKind::Structural,
            p::SortKind::Val(value) => r::SortKind::Val(back_value_kind(*value)),
            p::SortKind::Coercion { from, to, class } => r::SortKind::Coercion {
                from: back_value_kind(*from),
                to: back_value_kind(*to),
                class: back_coercion_class(*class),
            },
            p::SortKind::Merger(value) => r::SortKind::Merger(back_value_kind(*value)),
        }
    }

    pub fn back_term(t: &p::Term) -> r::Term {
        match t {
            p::Term::Var(x) => r::Term::Var(x.to_string()),
            p::Term::App { op, args } => {
                r::Term::App(op.to_string(), args.iter().map(back_term).collect())
            }
            p::Term::Case {
                scrutinee,
                branches,
            } => r::Term::Case(
                Box::new(back_term(scrutinee)),
                branches
                    .iter()
                    .map(|b| r::CaseBranch {
                        constructor: b.constructor.to_string(),
                        binders: b.binders.iter().map(ToString::to_string).collect(),
                        body: back_term(&b.body),
                    })
                    .collect(),
            ),
            p::Term::Hole { name } => r::Term::Hole(name.as_ref().map(ToString::to_string)),
            p::Term::Let { name, bound, body } => r::Term::Let(
                name.to_string(),
                Box::new(back_term(bound)),
                Box::new(back_term(body)),
            ),
        }
    }

    pub fn back_sort_expr(s: &p::SortExpr) -> r::SortExpr {
        match s {
            p::SortExpr::Name(n) => r::SortExpr::Name(n.to_string()),
            p::SortExpr::App { name, args } => {
                r::SortExpr::App(name.to_string(), args.iter().map(back_term).collect())
            }
        }
    }

    const fn back_implicit(i: p::Implicit) -> r::Implicit {
        match i {
            p::Implicit::No => r::Implicit::No,
            p::Implicit::Yes => r::Implicit::Yes,
        }
    }

    macro_rules! builtin_conversions {
        ($( $production:ident => $reference:ident ),+ $(,)?) => {
            const fn back_builtin(operation: panproto_expr::BuiltinOp) -> rv::BuiltinOp {
                match operation {
                    $(panproto_expr::BuiltinOp::$production => rv::BuiltinOp::$reference,)+
                }
            }
        };
    }

    builtin_conversions! {
        Add => Add,
        Sub => Sub,
        Mul => Mul,
        Div => Div,
        Mod => Mod,
        Neg => Neg,
        Abs => Abs,
        Floor => Floor,
        Ceil => Ceil,
        Round => Round,
        Eq => Eq,
        Neq => Ne,
        Lt => Lt,
        Lte => Lte,
        Gt => Gt,
        Gte => Gte,
        And => And,
        Or => Or,
        Not => Not,
        Concat => Concat,
        Len => Len,
        Slice => Slice,
        Upper => Upper,
        Lower => Lower,
        Trim => Trim,
        Split => Split,
        Join => Join,
        Replace => Replace,
        Contains => Contains,
        Map => Map,
        Filter => Filter,
        Fold => Fold,
        Append => Append,
        Head => Head,
        Tail => Tail,
        Reverse => Reverse,
        FlatMap => FlatMap,
        Length => Length,
        Range => Range,
        MergeRecords => MergeRecords,
        Keys => Keys,
        Values => Values,
        HasField => HasField,
        DefaultVal => DefaultVal,
        Clamp => Clamp,
        TruncateStr => TruncateStr,
        IntToFloat => IntToFloat,
        FloatToInt => FloatToInt,
        IntToStr => IntToStr,
        FloatToStr => FloatToStr,
        StrToInt => StrToInt,
        StrToFloat => StrToFloat,
        TypeOf => TypeOf,
        IsNull => IsNull,
        IsList => IsList,
        Edge => Edge,
        Children => Children,
        HasEdge => HasEdge,
        EdgeCount => EdgeCount,
        Anchor => Anchor,
        Edges => Edges,
        ToString => ToString,
        ParseDecimal => ParseDecimal,
        DecimalToFloat => DecimalToFloat,
    }

    fn back_decimal(value: &panproto_expr::exact::DecimalValue) -> rx::Decimal {
        rx::parse_decimal(&value.to_string()).expect("production decimal display is canonical")
    }

    fn back_big_int(value: &panproto_expr::exact::BigIntValue) -> rx::BigIntValue {
        rx::BigIntValue {
            negative: value.negative,
            significand: Nat::parse(&value.significand)
                .expect("production bigint significand is canonical"),
            zeros: Nat::parse(value.zeros.magnitude())
                .expect("production bigint zero count is canonical"),
        }
    }

    fn back_temporal(value: &panproto_expr::exact::Temporal) -> r::TemporalLiteral {
        let reference = rt::parse_any(&value.to_string())
            .expect("production temporal display is accepted by the reference");
        r::TemporalLiteral::new(reference).expect("parsed reference temporal is well formed")
    }

    fn back_literal(value: &panproto_expr::Literal) -> r::Literal {
        match value {
            panproto_expr::Literal::Bool(value) => r::Literal::Bool(*value),
            panproto_expr::Literal::Int(value) => r::Literal::Int(*value),
            panproto_expr::Literal::Float(value) => r::Literal::Float(value.to_bits()),
            panproto_expr::Literal::Str(value) => r::Literal::Str(value.clone()),
            panproto_expr::Literal::Bytes(value) => r::Literal::Bytes(value.clone()),
            panproto_expr::Literal::Null => r::Literal::Null,
            panproto_expr::Literal::Record(fields) => r::Literal::Record(
                fields
                    .iter()
                    .map(|(name, value)| (name.to_string(), back_literal(value)))
                    .collect(),
            ),
            panproto_expr::Literal::List(items) => {
                r::Literal::List(items.iter().map(back_literal).collect())
            }
            panproto_expr::Literal::Closure { param, body, env } => r::Literal::Closure {
                param: param.to_string(),
                body: Box::new(back_expression(body)),
                env: env
                    .iter()
                    .map(|(name, value)| (name.to_string(), back_literal(value)))
                    .collect(),
            },
            panproto_expr::Literal::Decimal(value) => r::Literal::Decimal(back_decimal(value)),
            panproto_expr::Literal::BigInt(value) => r::Literal::BigInt(back_big_int(value)),
            panproto_expr::Literal::Temporal(value) => r::Literal::Temporal(back_temporal(value)),
        }
    }

    fn back_pattern(pattern: &panproto_expr::Pattern) -> r::Pattern {
        match pattern {
            panproto_expr::Pattern::Wildcard => r::Pattern::Wildcard,
            panproto_expr::Pattern::Var(name) => r::Pattern::Var(name.to_string()),
            panproto_expr::Pattern::Lit(value) => r::Pattern::Lit(back_literal(value)),
            panproto_expr::Pattern::Record(fields) => r::Pattern::Record(
                fields
                    .iter()
                    .map(|(name, pattern)| (name.to_string(), self::back_pattern(pattern)))
                    .collect(),
            ),
            panproto_expr::Pattern::List(items) => {
                r::Pattern::List(items.iter().map(self::back_pattern).collect())
            }
            panproto_expr::Pattern::Constructor(name, items) => r::Pattern::Constructor(
                name.to_string(),
                items.iter().map(self::back_pattern).collect(),
            ),
        }
    }

    fn back_expression(expression: &panproto_expr::Expr) -> r::Expression {
        match expression {
            panproto_expr::Expr::Var(name) => r::Expression::Var(name.to_string()),
            panproto_expr::Expr::Lam(name, body) => {
                r::Expression::Lam(name.to_string(), Box::new(self::back_expression(body)))
            }
            panproto_expr::Expr::App(function, argument) => r::Expression::App(
                Box::new(self::back_expression(function)),
                Box::new(self::back_expression(argument)),
            ),
            panproto_expr::Expr::Lit(value) => r::Expression::Lit(back_literal(value)),
            panproto_expr::Expr::Record(fields) => r::Expression::Record(
                fields
                    .iter()
                    .map(|(name, value)| (name.to_string(), self::back_expression(value)))
                    .collect(),
            ),
            panproto_expr::Expr::List(items) => {
                r::Expression::List(items.iter().map(self::back_expression).collect())
            }
            panproto_expr::Expr::Field(base, field) => {
                r::Expression::Field(Box::new(self::back_expression(base)), field.to_string())
            }
            panproto_expr::Expr::Index(base, index) => r::Expression::Index(
                Box::new(self::back_expression(base)),
                Box::new(self::back_expression(index)),
            ),
            panproto_expr::Expr::Match { scrutinee, arms } => r::Expression::Match {
                scrutinee: Box::new(self::back_expression(scrutinee)),
                arms: arms
                    .iter()
                    .map(|(pattern, body)| {
                        (self::back_pattern(pattern), self::back_expression(body))
                    })
                    .collect(),
            },
            panproto_expr::Expr::Let { name, value, body } => r::Expression::Let {
                name: name.to_string(),
                value: Box::new(self::back_expression(value)),
                body: Box::new(self::back_expression(body)),
            },
            panproto_expr::Expr::Builtin(operation, arguments) => r::Expression::Builtin(
                back_builtin(*operation),
                arguments.iter().map(self::back_expression).collect(),
            ),
        }
    }

    fn back_strategy(strategy: &p::ConflictStrategy) -> r::ConflictStrategy {
        match strategy {
            p::ConflictStrategy::KeepLeft => r::ConflictStrategy::KeepLeft,
            p::ConflictStrategy::KeepRight => r::ConflictStrategy::KeepRight,
            p::ConflictStrategy::Fail => r::ConflictStrategy::Fail,
            p::ConflictStrategy::Custom(expression) => {
                r::ConflictStrategy::Custom(back_expression(expression))
            }
        }
    }

    pub fn back_theory(t: &p::Theory) -> r::Theory {
        r::Theory {
            name: t.name.to_string(),
            extends: t.extends.iter().map(ToString::to_string).collect(),
            sorts: t
                .sorts
                .iter()
                .map(|s| r::Sort {
                    name: s.name.to_string(),
                    params: s
                        .params
                        .iter()
                        .map(|q| (q.name.to_string(), back_sort_expr(&q.sort)))
                        .collect(),
                    kind: back_kind(&s.kind),
                    constructors: match &s.closure {
                        p::SortClosure::Open => None,
                        p::SortClosure::Closed(cs) => {
                            Some(cs.iter().map(ToString::to_string).collect())
                        }
                    },
                })
                .collect(),
            ops: t
                .ops
                .iter()
                .map(|o| r::Op {
                    name: o.name.to_string(),
                    inputs: o
                        .inputs
                        .iter()
                        .map(|(n, s, i)| (n.to_string(), back_sort_expr(s), back_implicit(*i)))
                        .collect(),
                    output: back_sort_expr(&o.output),
                })
                .collect(),
            eqs: t
                .eqs
                .iter()
                .map(|e| r::Equation {
                    name: e.name.to_string(),
                    lhs: back_term(&e.lhs),
                    rhs: back_term(&e.rhs),
                })
                .collect(),
            directed_eqs: t
                .directed_eqs
                .iter()
                .map(|d| r::DirectedEquation {
                    name: d.name.to_string(),
                    lhs: back_term(&d.lhs),
                    rhs: back_term(&d.rhs),
                    implementation: back_expression(&d.impl_term),
                    inverse: d.inverse.as_ref().map(back_expression),
                    source_kind: d.source_kind.map(back_value_kind),
                    target_kind: d.target_kind.map(back_value_kind),
                    coercion_class: back_coercion_class(d.coercion_class),
                })
                .collect(),
            policies: t
                .policies
                .iter()
                .map(|q| r::Policy {
                    name: q.name.to_string(),
                    value_kind: back_value_kind(q.value_kind),
                    strategy: back_strategy(&q.strategy),
                })
                .collect(),
        }
    }

    const fn back_mode(mode: p::CompositionMode) -> r::CompositionMode {
        match mode {
            p::CompositionMode::Amalgamate => r::CompositionMode::Amalgamate,
            p::CompositionMode::ExplicitIdentity => r::CompositionMode::ExplicitIdentity,
        }
    }

    const fn back_clash(clash: p::ClashPolicy) -> r::ClashPolicy {
        match clash {
            p::ClashPolicy::Freshen => r::ClashPolicy::Freshen,
            p::ClashPolicy::Reject => r::ClashPolicy::Reject,
        }
    }

    /// A production recipe as the reference recipe it records.
    pub fn back_spec(spec: &p::CompositionSpec) -> r::CompositionSpec {
        r::CompositionSpec {
            result_name: spec.result_name.clone(),
            steps: spec
                .steps
                .iter()
                .map(|step| match step {
                    p::CompositionStep::Base(name) => r::CompositionStep::Base(name.clone()),
                    p::CompositionStep::Colimit {
                        left,
                        right,
                        shared_sorts,
                        shared_ops,
                        mode,
                    } => r::CompositionStep::Colimit {
                        left: left.clone(),
                        right: right.clone(),
                        shared_sorts: shared_sorts.clone(),
                        shared_ops: shared_ops.clone(),
                        mode: back_mode(*mode),
                    },
                    p::CompositionStep::Diagram {
                        nodes,
                        edges,
                        mode,
                        clash,
                        rewrite_budget,
                        inline,
                    } => r::CompositionStep::Diagram {
                        nodes: nodes.clone(),
                        edges: edges
                            .iter()
                            .map(|edge| r::RecipeEdge {
                                src: edge.src.clone(),
                                tgt: edge.tgt.clone(),
                                sorts: edge.sorts.clone(),
                                ops: edge
                                    .ops
                                    .iter()
                                    .map(|(name, image)| {
                                        (
                                            name.clone(),
                                            match image {
                                                p::RecipeOpImage::Op(op) => {
                                                    r::RecipeOpImage::Op(op.clone())
                                                }
                                                p::RecipeOpImage::Term(term) => {
                                                    r::RecipeOpImage::Term(back_term(term))
                                                }
                                            },
                                        )
                                    })
                                    .collect(),
                            })
                            .collect(),
                        mode: back_mode(*mode),
                        clash: back_clash(*clash),
                        rewrite_budget: *rewrite_budget,
                        inline: inline.iter().map(back_theory).collect(),
                    },
                })
                .collect(),
            version: spec.version,
        }
    }
}

/// The reference diagram a one-step diagram recipe records: each node is
/// the step's inline theory of that name, else the registry's; each edge
/// sends the named sorts and operations.
fn diagram_of(
    step: &r::CompositionStep,
    registry: &BTreeMap<String, r::Theory>,
) -> (r::TheoryDiagram, r::ComposeOptions) {
    let r::CompositionStep::Diagram {
        nodes,
        edges,
        mode,
        clash,
        rewrite_budget,
        inline,
    } = step
    else {
        panic!("a registration recipe is one diagram step: {step:?}");
    };
    let theory = |name: &str| {
        inline
            .iter()
            .find(|theory| theory.name == name)
            .or_else(|| registry.get(name))
            .cloned()
            .unwrap_or_else(|| panic!("{name} names no inline or registered theory"))
    };
    let diagram = r::TheoryDiagram {
        nodes: nodes
            .iter()
            .map(|label| r::DiagramNode {
                label: label.clone(),
                theory: theory(label),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|edge| r::DiagramEdge {
                src: edge.src.clone(),
                tgt: edge.tgt.clone(),
                morphism: r::Morphism {
                    name: format!("{}->{}", edge.src, edge.tgt),
                    domain: theory(&edge.src).name,
                    codomain: theory(&edge.tgt).name,
                    sorts: edge.sorts.iter().cloned().collect(),
                    ops: edge
                        .ops
                        .iter()
                        .map(|(name, image)| {
                            (
                                name.clone(),
                                match image {
                                    r::RecipeOpImage::Op(op) => r::OpImage::Op(op.clone()),
                                    r::RecipeOpImage::Term(term) => r::OpImage::Term(term.clone()),
                                },
                            )
                        })
                        .collect(),
                },
            })
            .collect(),
    };
    let options = r::ComposeOptions {
        mode: *mode,
        result_name: String::new(),
        clash: *clash,
        rewrite_budget: *rewrite_budget,
    };
    (diagram, options)
}

#[test]
fn registered_theories_agree_with_the_reference_typecheck_compose_and_recompose() {
    let mut theories = BTreeSet::new();
    let mut recipes = 0_usize;
    for descriptor in descriptors() {
        let name = descriptor.name;
        let protocol = (descriptor.protocol)();
        let mut registry = HashMap::new();
        (descriptor.register_theories)(&mut registry).unwrap();
        let reference_registry: BTreeMap<String, r::Theory> = registry
            .iter()
            .map(|(theory_name, theory)| (theory_name.clone(), convert::back_theory(theory)))
            .collect();
        for (theory_name, theory) in &reference_registry {
            // The conversion is faithful: the reference's canonical bytes
            // of the converted theory are production's of the original.
            assert_eq!(
                r::canonical_theory_bytes(theory),
                canonical_theory_bytes(&registry[theory_name]),
                "{name}: {theory_name} converts faithfully"
            );
            assert_eq!(
                r::typecheck_signature(theory),
                Ok(()),
                "{name}: {theory_name} typechecks in the reference"
            );
            theories.insert(theory_name.clone());
        }
        for (theory_name, carried) in [
            (&protocol.schema_theory, &protocol.schema_composition),
            (&protocol.instance_theory, &protocol.instance_composition),
        ] {
            let spec = convert::back_spec(
                carried
                    .as_ref()
                    .unwrap_or_else(|| panic!("{name}: {theory_name} has no recipe")),
            );
            let expected = canonical_theory_bytes(&registry[theory_name.as_str()]);
            let replayed = r::recompose(&spec, &reference_registry)
                .unwrap_or_else(|error| panic!("{name}: {theory_name}: {error:?}"));
            assert_eq!(
                r::canonical_theory_bytes(&replayed),
                expected,
                "{name}: {theory_name} replays in the reference"
            );
            assert_eq!(spec.steps.len(), 1, "{name}: {theory_name}: {spec:?}");
            let (diagram, mut options) = diagram_of(&spec.steps[0], &reference_registry);
            options.result_name.clone_from(&spec.result_name);
            assert_eq!(options.mode, r::CompositionMode::ExplicitIdentity, "{name}");
            assert_eq!(options.clash, r::ClashPolicy::Reject, "{name}");
            let composition = r::compose(&diagram, &options)
                .unwrap_or_else(|error| panic!("{name}: {theory_name}: {error:?}"));
            assert_eq!(
                composition.report.status,
                r::CheckStatus::Passed,
                "{name}: {theory_name}"
            );
            assert!(composition.report.name_clash_free, "{name}: {theory_name}");
            for record in &composition.report.checks {
                assert!(
                    matches!(
                        record.outcome,
                        r::CheckOutcome::Passed | r::CheckOutcome::PassedByEquation { .. }
                    ),
                    "{name}: {theory_name}: {record:?}"
                );
            }
            assert_eq!(
                r::canonical_theory_bytes(&composition.theory),
                expected,
                "{name}: {theory_name} composes in the reference"
            );
            recipes += 1;
        }
    }
    assert_eq!(recipes, 2 * descriptors().len());
    assert_eq!(
        (descriptors().len(), theories.len(), recipes),
        (57, 125, 114)
    );
    eprintln!(
        "reference registration: {} protocols, {} distinct registered theories, {recipes} recipes",
        descriptors().len(),
        theories.len()
    );
}
