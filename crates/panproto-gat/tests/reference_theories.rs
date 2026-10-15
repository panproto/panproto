//! Differential tests of the theory page against the reference semantics.
//!
//! Every test here converts its input from the reference model of
//! `panproto_reference::theory` to this crate's types, runs the production
//! function and the reference function on it, converts the production
//! output back, and requires exact agreement through
//! `panproto_reference::harness::agree`: equal values, or refusals with the
//! same typed reason. The exhaustive tests visit every input of their
//! stated finite scope (`panproto_reference::enumerate::theories`) and
//! check its count against a closed formula computed here; the property
//! test draws from the same constructor families.
//!
//! Three fields of the refusals carry free text produced by two different
//! type checkers, so the comparison reads them through their typed part:
//! the `found` sort of a parameter that has no sort (`no sort (...)`), the
//! detail of an ill-typed term assignment, and the detail of an ill-typed
//! transported equation compare by the fact of the refusal and its
//! subject, not by the wording of the checker's message.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use panproto_reference::enumerate::theories as scope;
use panproto_reference::harness::{agree, outcome_eq};
use panproto_reference::theory as r;

mod convert {
    //! Conversions between the reference model and this crate's types.

    use super::{Arc, BTreeMap, HashMap, r};
    use panproto_gat as p;

    /// The reference name of a production sort kind.
    pub fn kind_name(kind: &p::SortKind) -> String {
        match kind {
            p::SortKind::Structural => super::scope::STRUCTURAL.to_owned(),
            p::SortKind::Val(p::ValueKind::Str) => super::scope::VALUE.to_owned(),
            other => format!("{other:?}"),
        }
    }

    fn kind(name: &str) -> p::SortKind {
        if name == super::scope::VALUE {
            p::SortKind::Val(p::ValueKind::Str)
        } else {
            p::SortKind::Structural
        }
    }

    pub fn term(t: &r::Term) -> p::Term {
        match t {
            r::Term::Var(x) => p::Term::Var(Arc::from(x.as_str())),
            r::Term::App(op, args) => p::Term::App {
                op: Arc::from(op.as_str()),
                args: args.iter().map(term).collect(),
            },
            r::Term::Case(scrutinee, branches) => p::Term::Case {
                scrutinee: Box::new(term(scrutinee)),
                branches: branches
                    .iter()
                    .map(|b| p::CaseBranch {
                        constructor: Arc::from(b.constructor.as_str()),
                        binders: b.binders.iter().map(|x| Arc::from(x.as_str())).collect(),
                        body: term(&b.body),
                    })
                    .collect(),
            },
            r::Term::Hole(name) => p::Term::Hole {
                name: name.as_deref().map(Arc::from),
            },
            r::Term::Let(name, bound, body) => p::Term::Let {
                name: Arc::from(name.as_str()),
                bound: Box::new(term(bound)),
                body: Box::new(term(body)),
            },
        }
    }

    pub fn back_term(t: &p::Term) -> r::Term {
        match t {
            p::Term::Var(x) => r::Term::Var(x.to_string()),
            p::Term::App { op, args } => r::Term::App(op.to_string(), args.iter().map(back_term).collect()),
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

    pub fn sort_expr(s: &r::SortExpr) -> p::SortExpr {
        match s {
            r::SortExpr::Name(n) => p::SortExpr::Name(Arc::from(n.as_str())),
            r::SortExpr::App(n, args) => p::SortExpr::app(n.as_str(), args.iter().map(term).collect()),
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

    fn implicit(i: r::Implicit) -> p::Implicit {
        match i {
            r::Implicit::No => p::Implicit::No,
            r::Implicit::Yes => p::Implicit::Yes,
        }
    }

    fn back_implicit(i: p::Implicit) -> r::Implicit {
        match i {
            p::Implicit::No => r::Implicit::No,
            p::Implicit::Yes => r::Implicit::Yes,
        }
    }

    fn expr(text: &str) -> panproto_expr::Expr {
        panproto_expr::Expr::Var(Arc::from(text))
    }

    fn back_expr(e: &panproto_expr::Expr) -> String {
        match e {
            panproto_expr::Expr::Var(text) => text.to_string(),
            other => format!("{other:?}"),
        }
    }

    fn value_kind(name: &str) -> p::ValueKind {
        p::ValueKind::all()
            .iter()
            .copied()
            .find(|k| format!("{k:?}") == name)
            .unwrap_or(p::ValueKind::Any)
    }

    fn coercion_class(name: &str) -> p::CoercionClass {
        match name {
            "Retraction" => p::CoercionClass::Retraction,
            "Projection" => p::CoercionClass::Projection,
            "Opaque" => p::CoercionClass::Opaque,
            _ => p::CoercionClass::Iso,
        }
    }

    fn strategy(text: &str) -> p::ConflictStrategy {
        match text {
            "KeepLeft" => p::ConflictStrategy::KeepLeft,
            "KeepRight" => p::ConflictStrategy::KeepRight,
            "Fail" => p::ConflictStrategy::Fail,
            other => p::ConflictStrategy::Custom(expr(other)),
        }
    }

    fn back_strategy(s: &p::ConflictStrategy) -> String {
        match s {
            p::ConflictStrategy::KeepLeft => "KeepLeft".to_owned(),
            p::ConflictStrategy::KeepRight => "KeepRight".to_owned(),
            p::ConflictStrategy::Fail => "Fail".to_owned(),
            p::ConflictStrategy::Custom(e) => back_expr(e),
        }
    }

    pub fn theory(t: &r::Theory) -> p::Theory {
        p::Theory::full(
            t.name.as_str(),
            t.extends.iter().map(|x| Arc::from(x.as_str())).collect(),
            t.sorts
                .iter()
                .map(|s| p::Sort {
                    name: Arc::from(s.name.as_str()),
                    params: s
                        .params
                        .iter()
                        .map(|(n, e)| p::SortParam::new(n.as_str(), sort_expr(e)))
                        .collect(),
                    kind: kind(&s.kind),
                    closure: s.constructors.as_ref().map_or(p::SortClosure::Open, |cs| {
                        p::SortClosure::Closed(cs.iter().map(|c| Arc::from(c.as_str())).collect())
                    }),
                })
                .collect(),
            t.ops
                .iter()
                .map(|o| {
                    p::Operation::with_implicit(
                        o.name.as_str(),
                        o.inputs
                            .iter()
                            .map(|(n, s, i)| (Arc::from(n.as_str()), sort_expr(s), implicit(*i)))
                            .collect(),
                        sort_expr(&o.output),
                    )
                })
                .collect(),
            t.eqs
                .iter()
                .map(|e| p::Equation::new(e.name.as_str(), term(&e.lhs), term(&e.rhs)))
                .collect(),
            t.directed_eqs
                .iter()
                .map(|d| p::DirectedEquation {
                    name: Arc::from(d.name.as_str()),
                    lhs: term(&d.lhs),
                    rhs: term(&d.rhs),
                    impl_term: expr(&d.implementation),
                    inverse: d.inverse.as_deref().map(expr),
                    source_kind: d.source_kind.as_deref().map(value_kind),
                    target_kind: d.target_kind.as_deref().map(value_kind),
                    coercion_class: coercion_class(&d.coercion_class),
                })
                .collect(),
            t.policies
                .iter()
                .map(|q| p::ConflictPolicy {
                    name: Arc::from(q.name.as_str()),
                    value_kind: value_kind(&q.value_kind),
                    strategy: strategy(&q.strategy),
                })
                .collect(),
        )
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
                    kind: kind_name(&s.kind),
                    constructors: match &s.closure {
                        p::SortClosure::Open => None,
                        p::SortClosure::Closed(cs) => Some(cs.iter().map(ToString::to_string).collect()),
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
                    implementation: back_expr(&d.impl_term),
                    inverse: d.inverse.as_ref().map(back_expr),
                    source_kind: d.source_kind.map(|k| format!("{k:?}")),
                    target_kind: d.target_kind.map(|k| format!("{k:?}")),
                    coercion_class: format!("{:?}", d.coercion_class),
                })
                .collect(),
            policies: t
                .policies
                .iter()
                .map(|q| r::Policy {
                    name: q.name.to_string(),
                    value_kind: format!("{:?}", q.value_kind),
                    strategy: back_strategy(&q.strategy),
                })
                .collect(),
        }
    }

    pub fn morphism(m: &r::Morphism) -> p::TheoryMorphism {
        let sorts: HashMap<Arc<str>, Arc<str>> = m
            .sorts
            .iter()
            .map(|(a, b)| (Arc::from(a.as_str()), Arc::from(b.as_str())))
            .collect();
        let ops: HashMap<Arc<str>, p::OpAssignment> = m
            .ops
            .iter()
            .map(|(a, image)| {
                (
                    Arc::from(a.as_str()),
                    match image {
                        r::OpImage::Op(b) => p::OpAssignment::Op(Arc::from(b.as_str())),
                        r::OpImage::Term(t) => p::OpAssignment::Term(term(t)),
                    },
                )
            })
            .collect();
        p::TheoryMorphism::new(m.name.as_str(), m.domain.as_str(), m.codomain.as_str(), sorts, ops)
    }

    pub fn back_morphism(m: &p::TheoryMorphism) -> r::Morphism {
        r::Morphism {
            name: m.name.to_string(),
            domain: m.domain.to_string(),
            codomain: m.codomain.to_string(),
            sorts: m
                .sort_map
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            ops: m
                .op_map
                .iter()
                .map(|(a, image)| {
                    (
                        a.to_string(),
                        match image {
                            p::OpAssignment::Op(b) => r::OpImage::Op(b.to_string()),
                            p::OpAssignment::Term(t) => r::OpImage::Term(back_term(t)),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn mode(m: r::CompositionMode) -> p::CompositionMode {
        match m {
            r::CompositionMode::Amalgamate => p::CompositionMode::Amalgamate,
            r::CompositionMode::ExplicitIdentity => p::CompositionMode::ExplicitIdentity,
        }
    }

    fn back_mode(m: p::CompositionMode) -> r::CompositionMode {
        match m {
            p::CompositionMode::Amalgamate => r::CompositionMode::Amalgamate,
            p::CompositionMode::ExplicitIdentity => r::CompositionMode::ExplicitIdentity,
        }
    }

    pub fn clash(c: r::ClashPolicy) -> p::ClashPolicy {
        match c {
            r::ClashPolicy::Freshen => p::ClashPolicy::Freshen,
            r::ClashPolicy::Reject => p::ClashPolicy::Reject,
        }
    }

    pub fn options(o: &r::ComposeOptions) -> p::ComposeOptions {
        p::ComposeOptions {
            mode: mode(o.mode),
            result_name: Arc::from(o.result_name.as_str()),
            clash: clash(o.clash),
            rewrite_budget: o.rewrite_budget,
        }
    }

    pub fn diagram(d: &r::TheoryDiagram) -> p::TheoryDiagram {
        p::TheoryDiagram {
            nodes: d
                .nodes
                .iter()
                .map(|n| p::DiagramNode {
                    label: Arc::from(n.label.as_str()),
                    theory: theory(&n.theory),
                })
                .collect(),
            edges: d
                .edges
                .iter()
                .map(|e| p::DiagramEdge {
                    src: Arc::from(e.src.as_str()),
                    tgt: Arc::from(e.tgt.as_str()),
                    morphism: morphism(&e.morphism),
                })
                .collect(),
        }
    }

    pub fn sym_kind(k: p::SymKind) -> r::SymKind {
        match k {
            p::SymKind::Sort => r::SymKind::Sort,
            p::SymKind::Op => r::SymKind::Op,
            p::SymKind::Eq => r::SymKind::Eq,
            p::SymKind::DirectedEq => r::SymKind::DirectedEq,
            p::SymKind::Policy => r::SymKind::Policy,
        }
    }

    pub fn sym(s: &p::SymRef) -> r::SymRef {
        r::SymRef {
            node: s.node.to_string(),
            name: s.name.to_string(),
        }
    }

    fn sym_map(m: &BTreeMap<p::SymRef, Arc<str>>) -> BTreeMap<r::SymRef, String> {
        m.iter().map(|(k, v)| (sym(k), v.to_string())).collect()
    }

    fn check_kind(k: p::CompositionCheckKind) -> r::CompositionCheckKind {
        use p::CompositionCheckKind as P;
        use r::CompositionCheckKind as R;
        match k {
            P::LegIsMorphism => R::LegIsMorphism,
            P::NodeWellFormed => R::NodeWellFormed,
            P::ClassSignaturesAgree => R::ClassSignaturesAgree,
            P::CoconeCommutes => R::CoconeCommutes,
            P::InclusionIsMorphism => R::InclusionIsMorphism,
            P::EquationImage => R::EquationImage,
            P::EquationEntailed => R::EquationEntailed,
            P::UniversalIdentity => R::UniversalIdentity,
            P::UniversalAgainst => R::UniversalAgainst,
            P::ResultWellTyped => R::ResultWellTyped,
            P::NameClashFree => R::NameClashFree,
        }
    }

    fn subject(s: &p::CheckSubject) -> r::CheckSubject {
        match s {
            p::CheckSubject::Diagram => r::CheckSubject::Diagram,
            p::CheckSubject::Node(n) => r::CheckSubject::Node(n.to_string()),
            p::CheckSubject::Edge(i) => r::CheckSubject::Edge(*i),
            p::CheckSubject::Cocone(i) => r::CheckSubject::Cocone(*i),
        }
    }

    fn outcome(o: &p::CheckOutcome) -> r::CheckOutcome {
        match o {
            p::CheckOutcome::Passed => r::CheckOutcome::Passed,
            p::CheckOutcome::PassedByEquation { equation } => r::CheckOutcome::PassedByEquation {
                equation: equation.to_string(),
            },
            p::CheckOutcome::Failed { detail } => r::CheckOutcome::Failed {
                detail: detail.clone(),
            },
            p::CheckOutcome::Inapplicable { reason } => r::CheckOutcome::Inapplicable {
                reason: match reason {
                    p::InapplicableReason::OpenTheory { missing_sorts } => r::InapplicableReason::OpenTheory {
                        missing_sorts: missing_sorts.iter().map(ToString::to_string).collect(),
                    },
                    p::InapplicableReason::NoEquations => r::InapplicableReason::NoEquations,
                },
            },
            p::CheckOutcome::Incomplete { reason } => r::CheckOutcome::Incomplete {
                reason: match reason {
                    p::CompositionIncomplete::RewriteBudgetExhausted { equation, steps } => {
                        r::CompositionIncomplete::RewriteBudgetExhausted {
                            equation: equation.to_string(),
                            steps: *steps,
                        }
                    }
                    p::CompositionIncomplete::NoTerminatingRewriteSystem => {
                        r::CompositionIncomplete::NoTerminatingRewriteSystem
                    }
                },
            },
        }
    }

    fn status(s: p::CheckStatus) -> r::CheckStatus {
        match s {
            p::CheckStatus::Passed => r::CheckStatus::Passed,
            p::CheckStatus::Failed => r::CheckStatus::Failed,
            p::CheckStatus::Incomplete => r::CheckStatus::Incomplete,
        }
    }

    /// The reference spelling `{label}-inclusion` of an inclusion's name,
    /// when the production name is exactly `incl_{label}_{result}` for the
    /// node's label; the production name unchanged otherwise.
    fn inclusion_name(name: &str, label: &str, result: &str) -> String {
        if name == format!("incl_{label}_{result}") {
            format!("{label}-inclusion")
        } else {
            name.to_owned()
        }
    }

    pub fn back_composition(c: &p::Composition, labels: &[String]) -> r::Composition {
        r::Composition {
            theory: back_theory(&c.theory),
            inclusions: c
                .inclusions
                .iter()
                .zip(labels)
                .map(|(m, label)| {
                    let mut back = back_morphism(m);
                    back.name = inclusion_name(&back.name, label, &c.theory.name);
                    back
                })
                .collect(),
            provenance: r::Provenance {
                sorts: sym_map(&c.provenance.sorts),
                ops: sym_map(&c.provenance.ops),
                eqs: sym_map(&c.provenance.eqs),
                directed_eqs: sym_map(&c.provenance.directed_eqs),
                policies: sym_map(&c.provenance.policies),
                added_eqs: c
                    .provenance
                    .added_eqs
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            r::AddedEquation {
                                edge: v.edge,
                                op: sym(&v.op),
                            },
                        )
                    })
                    .collect(),
            },
            report: r::CompositionReport {
                mode: back_mode(c.report.mode),
                status: status(c.report.status),
                name_clash_free: c.report.name_clash_free,
                checks: c
                    .report
                    .checks
                    .iter()
                    .map(|record| r::CheckRecord {
                        check: check_kind(record.check),
                        subject: subject(&record.subject),
                        outcome: outcome(&record.outcome),
                    })
                    .collect(),
                freshened: c
                    .report
                    .freshened
                    .iter()
                    .map(|f| r::Freshening {
                        kind: sym_kind(f.kind),
                        members: f.members.iter().map(sym).collect(),
                        candidate: f.candidate.to_string(),
                        result: f.result.to_string(),
                    })
                    .collect(),
            },
        }
    }

    pub fn site(s: &p::SignatureSite) -> r::Site {
        match s {
            p::SignatureSite::SortParam { sort, index } => r::Site::SortParam(sort.to_string(), *index),
            p::SignatureSite::OpInput { op, index } => r::Site::OpInput(op.to_string(), *index),
            p::SignatureSite::OpOutput { op } => r::Site::OpOutput(op.to_string()),
        }
    }

    /// A defect, with a sortless argument's checker message reduced to the
    /// fact that it has no sort.
    pub fn defect(d: &p::SignatureDefect) -> r::Defect {
        match d {
            p::SignatureDefect::UndeclaredSort { site: s, sort } => {
                r::Defect::UndeclaredSort(site(s), sort.to_string())
            }
            p::SignatureDefect::SortArity {
                site: s,
                sort,
                expected,
                found,
            } => r::Defect::SortArity(site(s), sort.to_string(), *expected, *found),
            p::SignatureDefect::ParamIllTyped {
                site: s,
                sort,
                index,
                expected,
                found,
            } => r::Defect::ParamIllTyped(
                site(s),
                sort.to_string(),
                *index,
                expected.to_string(),
                sortless(found),
            ),
            p::SignatureDefect::UnboundParamVar { site: s, var } => {
                r::Defect::UnboundParamVar(site(s), var.to_string())
            }
            p::SignatureDefect::DuplicateSymbol { kind, name } => {
                r::Defect::DuplicateSymbol(sym_kind(*kind), name.to_string())
            }
            p::SignatureDefect::DuplicateBinder { site: s, name } => {
                r::Defect::DuplicateBinder(site(s), name.to_string())
            }
        }
    }

    /// `no sort` for a checker's account of a sortless argument; the found
    /// sort otherwise.
    pub fn sortless(found: &str) -> String {
        if found.starts_with("no sort (") {
            "no sort".to_owned()
        } else {
            found.to_owned()
        }
    }

    /// The reference defect with the same reduction.
    pub fn reduce_defect(d: &r::Defect) -> r::Defect {
        match d {
            r::Defect::ParamIllTyped(s, sort, index, expected, found) => {
                r::Defect::ParamIllTyped(s.clone(), sort.clone(), *index, expected.clone(), sortless(found))
            }
            other => other.clone(),
        }
    }

    /// The morphism refusal a production error denotes, with an ill-typed
    /// term assignment reduced to its operation; `None` for an error that
    /// denotes no morphism refusal.
    pub fn morphism_error(e: &p::GatError) -> Option<r::MorphismError> {
        use r::MorphismError as M;
        Some(match e {
            p::GatError::MissingSortMapping(s) => M::MissingSortMapping(s.clone()),
            p::GatError::SortNotFound(s) => M::SortNotFound(s.clone()),
            p::GatError::SortArityMismatch {
                sort,
                expected,
                got,
            } => M::SortArityMismatch {
                sort: sort.clone(),
                expected: *expected,
                got: *got,
            },
            p::GatError::SortKindMismatch {
                sort,
                expected,
                got,
            } => M::SortKindMismatch {
                sort: sort.clone(),
                expected: kind_name(expected),
                got: kind_name(got),
            },
            p::GatError::SortParamMismatch {
                sort,
                param_index,
                expected,
                got,
            } => M::SortParamMismatch {
                sort: sort.clone(),
                index: *param_index,
                expected: expected.clone(),
                got: got.clone(),
            },
            p::GatError::MorphismClosureMismatch {
                sort,
                expected,
                got,
            } => M::ClosureMismatch {
                sort: sort.clone(),
                expected: expected.clone(),
                got: got.clone(),
            },
            p::GatError::MissingOpMapping(o) => M::MissingOpMapping(o.clone()),
            p::GatError::OpNotFound(o) => M::OpNotFound(o.clone()),
            p::GatError::OpTypeMismatch { op, detail } => M::OpTypeMismatch {
                op: op.clone(),
                detail: detail.clone(),
            },
            p::GatError::TermAssignmentUnboundVar { op, var } => M::TermAssignmentUnboundVar {
                op: op.clone(),
                var: var.clone(),
            },
            p::GatError::TermAssignmentIllTyped { op, .. } => M::TermAssignmentIllTyped {
                op: op.clone(),
                detail: String::new(),
            },
            p::GatError::TermAssignmentSortMismatch { op, expected, got } => {
                M::TermAssignmentSortMismatch {
                    op: op.clone(),
                    expected: expected.clone(),
                    got: got.clone(),
                }
            }
            p::GatError::EquationNotPreserved { equation, .. } => M::EquationNotPreserved(equation.clone()),
            p::GatError::EquationPreservationUnknown { equation, .. } => M::EquationPreservationUnknown {
                equation: equation.clone(),
                steps: 0,
            },
            p::GatError::DirectedEquationNotPreserved { equation, .. } => {
                M::DirectedEquationNotPreserved(equation.clone())
            }
            _ => return None,
        })
    }

    /// The reference refusal with the same reductions: an ill-typed term
    /// assignment read by its operation, and an undecided equation by its
    /// name (the production refusal does not carry the steps consumed).
    pub fn reduce_morphism_error(e: &r::MorphismError) -> r::MorphismError {
        match e {
            r::MorphismError::EquationPreservationUnknown { equation, .. } => {
                r::MorphismError::EquationPreservationUnknown {
                    equation: equation.clone(),
                    steps: 0,
                }
            }
            r::MorphismError::TermAssignmentIllTyped { op, .. } => r::MorphismError::TermAssignmentIllTyped {
                op: op.clone(),
                detail: String::new(),
            },
            other => other.clone(),
        }
    }
}

// ─── signature checking: L-theories-1 and L-theories-2 ─────────────

/// Production and reference agree on one signature.
fn typecheck_agrees(t: &r::Theory) -> bool {
    let production = panproto_gat::typecheck_signature(&convert::theory(t))
        .map_err(|ds| ds.iter().map(convert::defect).collect::<Vec<_>>());
    let reference = r::typecheck_signature(t)
        .map_err(|ds| ds.iter().map(convert::reduce_defect).collect::<Vec<_>>());
    production == reference
}

/// The count of the signatures of one sort shape and operation list,
/// with and without an equation: `1 + (2 + c)^2` for `c` constants.
fn equations_count(constants: u64) -> u64 {
    1 + (2 + constants).pow(2)
}

/// The number of operation signatures of arity at most two over a shape
/// with `simple` simple sorts and, when `dependent`, the sort `B(p : A)`.
/// With only simple sorts it is `s + s^2 + s^3`; with `A` and the
/// dependent `B` it is `1 + 2 + 5` (outputs `A`; `(x : A)` to `A` or
/// `B(x)`; `(x : A, y : A)` to `A`, `B(x)`, `B(y)` and `(x : A, y : B(x))`
/// to `A` or `B(x)`).
const fn operation_count(simple: u64, dependent: bool) -> u64 {
    if dependent {
        8
    } else {
        simple + simple * simple + simple * simple * simple
    }
}

/// The number of constant-free operation signatures, split by arity:
/// `(nullary, positive)` as in [`operation_count`].
const fn operation_split(simple: u64, dependent: bool) -> (u64, u64) {
    if dependent {
        (1, 7)
    } else {
        (simple, simple * simple + simple * simple * simple)
    }
}

/// The closed-form count of `scope::signatures()` and of
/// `scope::bare_signatures()`.
fn signature_counts() -> (u64, u64) {
    // (sort shapes with this shape, simple sorts, dependent)
    let shapes: [(u64, u64, bool); 4] = [(1, 0, false), (3, 1, false), (2, 2, false), (2, 1, true)];
    let mut bare = 0;
    let mut full = 0;
    for (copies, simple, dependent) in shapes {
        let n = operation_count(simple, dependent);
        let (nullary, positive) = operation_split(simple, dependent);
        bare += copies * (1 + n + n * n);
        // No operation; one operation, constant or not; two operations
        // with 0, 1 or 2 constants.
        let lists = equations_count(0)
            + nullary * equations_count(1)
            + positive * equations_count(0)
            + positive * positive * equations_count(0)
            + 2 * nullary * positive * equations_count(1)
            + nullary * nullary * equations_count(2);
        full += copies * lists;
    }
    (full, bare)
}

/// The closed-form count of `scope::raw_signatures()`: seven sort lists,
/// each with no operation and with each of `7 + 2*7*7 + 4*7*7*7` raw
/// operations, then five duplicate-symbol theories.
const fn raw_signature_count() -> u64 {
    let e = 7;
    7 * (1 + e + 2 * e * e + 4 * e * e * e) + 5
}

#[test]
fn signature_checking_agrees_with_the_reference_on_every_signature() {
    let signatures = scope::signatures();
    let raw = scope::raw_signatures();
    let (full, bare) = signature_counts();
    println!(
        "alphabet: sorts A (open | closed, structural | value), B (simple | B(p : A)); \
         operations f, g of arity <= 2 with inputs x, y; equation e between x, y and constants"
    );
    println!(
        "bound: sorts <= 2, operations <= 2, arity <= 2, equations <= 1, equation term depth <= 1"
    );
    println!(
        "signatures: {} enumerated, {full} by formula ({} bare, {bare} by formula)",
        signatures.len(),
        scope::bare_signatures().len()
    );
    assert_eq!(signatures.len() as u64, full);
    assert_eq!(scope::bare_signatures().len() as u64, bare);
    println!(
        "raw alphabet: sort expressions {:?}; binders x, y with repetition",
        scope::raw_sort_expressions()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    println!(
        "raw signatures: {} enumerated, {} by formula",
        raw.len(),
        raw_signature_count()
    );
    assert_eq!(raw.len() as u64, raw_signature_count());
    let compared = agree(
        "signatures of the two-sort alphabet",
        signatures.iter().chain(&raw),
        |t| {
            panproto_gat::typecheck_signature(&convert::theory(t))
                .map_err(|ds| ds.iter().map(convert::defect).collect::<Vec<_>>())
        },
        |t| {
            r::typecheck_signature(t).map_err(|ds| ds.iter().map(convert::reduce_defect).collect::<Vec<_>>())
        },
        |a, b| a == b,
    )
    .unwrap_or_else(|d| panic!("{d}"));
    let refused = raw.iter().filter(|t| r::typecheck_signature(t).is_err()).count();
    println!("compared {compared}; raw signatures refused {refused}");
    assert_eq!(compared as usize, signatures.len() + raw.len());
}

// ─── union-find closure: L-theories-6 ──────────────────────────────

/// The theory of `size` simple sorts `s0 .. s(size-1)`, each with one
/// constant `c_i : s_i`, so that a quotient's sort classes can be read off
/// the constants' output sorts.
fn indexed_theory(size: usize) -> panproto_gat::Theory {
    panproto_gat::Theory::new(
        "Q",
        (0..size).map(|i| panproto_gat::Sort::simple(format!("s{i}"))).collect(),
        (0..size)
            .map(|i| panproto_gat::Operation::nullary(format!("c{i}"), format!("s{i}")))
            .collect(),
        Vec::new(),
    )
}

/// The production quotient of the indexed theory by the identifications,
/// read as each member's least class member; `None` when an
/// identification names a sort the theory does not declare.
fn production_closure(size: usize, identifications: &[(usize, usize)]) -> Option<Vec<usize>> {
    let pairs: Vec<(Arc<str>, Arc<str>)> = identifications
        .iter()
        .map(|(a, b)| (Arc::from(format!("s{a}")), Arc::from(format!("s{b}"))))
        .collect();
    match panproto_gat::quotient(&indexed_theory(size), &pairs) {
        Ok(quotiented) => Some(
            (0..size)
                .map(|i| {
                    let output = quotiented
                        .find_op(&format!("c{i}"))
                        .map(|op| op.output.head().to_string())
                        .unwrap_or_default();
                    output
                        .strip_prefix('s')
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(usize::MAX)
                })
                .collect(),
        ),
        Err(panproto_gat::GatError::QuotientIncompatible { detail, .. })
            if detail == "names are not both sorts or both operations in the theory" =>
        {
            None
        }
        Err(other) => panic!("unexpected quotient refusal {other}"),
    }
}

#[test]
fn quotient_closure_agrees_with_the_reference_on_every_identification_list() {
    let mut compared = 0u64;
    for size in 0..=4usize {
        let lists = scope::identifications(size, 3);
        let pairs = ((size + 1) * (size + 1)) as u64;
        let formula = 1 + pairs + pairs.pow(2) + pairs.pow(3);
        println!(
            "size {size}: alphabet pairs over 0..={size} ({pairs}, index {size} out of range); \
             lists of length <= 3: {} enumerated, {formula} by formula",
            lists.len()
        );
        assert_eq!(lists.len() as u64, formula);
        compared += agree(
            &format!("size {size}, length <= 3"),
            lists,
            |ids| production_closure(size, ids),
            |ids| r::quotient(size, ids),
            |a, b| a == b,
        )
        .unwrap_or_else(|d| panic!("{d}"));
    }
    println!("compared {compared}");
}

// ─── morphism checking: L-theories-7 ───────────────────────────────

/// A domain and codomain in both models.
struct Endpoints<'a> {
    domain: &'a r::Theory,
    codomain: &'a r::Theory,
    production: (panproto_gat::Theory, panproto_gat::Theory),
}

/// Both endpoints, converted once.
fn endpoints<'a>(domain: &'a r::Theory, codomain: &'a r::Theory) -> Endpoints<'a> {
    Endpoints {
        domain,
        codomain,
        production: (convert::theory(domain), convert::theory(codomain)),
    }
}

/// The production and reference outcomes on one candidate morphism.
fn endpoint_outcomes(
    ends: &Endpoints<'_>,
    m: &r::Morphism,
) -> (Result<(), Option<r::MorphismError>>, Result<(), r::MorphismError>) {
    let production = panproto_gat::check_morphism(&convert::morphism(m), &ends.production.0, &ends.production.1)
        .map_err(|e| convert::morphism_error(&e));
    let reference =
        r::check_morphism(ends.domain, ends.codomain, m).map_err(|e| convert::reduce_morphism_error(&e));
    (production, reference)
}

/// Production and reference outcomes on one candidate morphism.
fn morphism_outcomes(
    domain: &r::Theory,
    codomain: &r::Theory,
    m: &r::Morphism,
) -> (Result<(), Option<r::MorphismError>>, Result<(), r::MorphismError>) {
    endpoint_outcomes(&endpoints(domain, codomain), m)
}

/// Whether two morphism outcomes agree.
fn same_morphism_outcome(
    p: &Result<(), Option<r::MorphismError>>,
    r: &Result<(), r::MorphismError>,
) -> bool {
    outcome_eq(p, r, |(), ()| true, |p, r| p.as_ref() == Some(r))
}

fn morphism_agrees(domain: &r::Theory, codomain: &r::Theory, m: &r::Morphism) -> bool {
    let (production, reference) = morphism_outcomes(domain, codomain, m);
    same_morphism_outcome(&production, &reference)
}

/// The closed-form number of total assignments from `domain` to
/// `codomain`: `|sorts'|^|sorts|` times, per domain operation, the number
/// of codomain operations plus its arity plus the number of codomain
/// constants.
fn assignment_count(domain: &r::Theory, codomain: &r::Theory) -> u64 {
    let constants = codomain.ops.iter().filter(|op| op.inputs.is_empty()).count() as u64;
    let ops = codomain.ops.len() as u64;
    let mut count = (codomain.sorts.len() as u64).pow(domain.sorts.len() as u32);
    for op in &domain.ops {
        count *= ops + op.inputs.len() as u64 + constants;
    }
    count
}

/// Run `check` over `items` on every available thread, each thread taking
/// the next unchecked item, and return the total it reports; the first
/// failure stops every thread.
fn parallel<T: Sync>(items: &[T], check: impl Fn(&T) -> Result<u64, String> + Sync) -> u64 {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let failure: Mutex<Option<String>> = Mutex::new(None);
    let total = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(at) else { break };
                    match check(item) {
                        Ok(n) => {
                            total.fetch_add(usize::try_from(n).unwrap_or(usize::MAX), Ordering::Relaxed);
                        }
                        Err(e) => {
                            stop.store(true, Ordering::Relaxed);
                            failure.lock().expect("failure slot").get_or_insert(e);
                        }
                    }
                }
            });
        }
    });
    if let Some(e) = failure.into_inner().expect("failure slot") {
        panic!("{e}");
    }
    total.into_inner() as u64
}

#[test]
fn morphism_checking_agrees_with_the_reference_on_every_assignment() {
    let bare = scope::bare_signatures();
    let (_, bare_formula) = signature_counts();
    assert_eq!(bare.len() as u64, bare_formula);
    let formula: u64 = bare
        .iter()
        .map(|d| bare.iter().map(|c| assignment_count(d, c)).sum::<u64>())
        .sum();
    println!(
        "bound: domain and codomain every bare signature ({} each, {} ordered pairs); \
         images: codomain operations, input variables, codomain constants",
        bare.len(),
        bare.len() * bare.len()
    );
    let pairs: Vec<(&r::Theory, &r::Theory)> =
        bare.iter().flat_map(|d| bare.iter().map(move |c| (d, c))).collect();
    let started = std::time::Instant::now();
    let compared = parallel(&pairs, |&(domain, codomain)| {
        let mut n = 0u64;
        {
            let candidates = scope::morphisms(domain, codomain);
            if candidates.len() as u64 != assignment_count(domain, codomain) {
                return Err(format!(
                    "count {} != {} for {domain:?} -> {codomain:?}",
                    candidates.len(),
                    assignment_count(domain, codomain)
                ));
            }
            let ends = endpoints(domain, codomain);
            n += agree(
                "total assignments between bare signatures",
                candidates
                    .iter()
                    .chain(&scope::partial_morphisms(domain, codomain))
                    .map(|m| (m, endpoint_outcomes(&ends, m))),
                |(_, outcomes)| outcomes.0.clone(),
                |(_, outcomes)| outcomes.1.clone(),
                same_morphism_outcome,
            )
            .map_err(|d| format!("{d}\n  domain: {domain:?}\n  codomain: {codomain:?}"))?;
        }
        Ok(n)
    });
    let partial: u64 = bare
        .iter()
        .map(|d| {
            bare.iter()
                .map(|_| 2 * (d.sorts.len() + d.ops.len()) as u64)
                .sum::<u64>()
        })
        .sum();
    println!(
        "total assignments: {formula} by formula; partial assignments: {partial}; compared {compared} in {:?}",
        started.elapsed()
    );
    assert_eq!(compared, formula + partial);
}

/// Every equation variant of a bare signature: the bare signature itself
/// and each signature of `scope::signatures()` with its sorts and
/// operations.
fn equation_variants(bare: &r::Theory, signatures: &[r::Theory]) -> Vec<r::Theory> {
    signatures
        .iter()
        .filter(|t| t.sorts == bare.sorts && t.ops == bare.ops)
        .cloned()
        .collect()
}

#[test]
fn equation_preservation_agrees_with_the_reference_on_every_equation_pair() {
    let bare = scope::bare_signatures();
    let signatures = scope::signatures();
    let compared = parallel(&bare, |base| {
        let variants = equation_variants(base, &signatures);
        let constants = base.ops.iter().filter(|op| op.inputs.is_empty()).count() as u64;
        if variants.len() as u64 != equations_count(constants) {
            return Err(format!("{} equation variants of {base:?}", variants.len()));
        }
        let candidates = scope::morphisms(base, base);
        let mut n = 0;
        for domain in &variants {
            for codomain in &variants {
                let ends = endpoints(domain, codomain);
                n += agree(
                    "endomorphism assignments between equation variants",
                    candidates.iter().map(|m| (m, endpoint_outcomes(&ends, m))),
                    |(_, outcomes)| outcomes.0.clone(),
                    |(_, outcomes)| outcomes.1.clone(),
                    same_morphism_outcome,
                )
                .map_err(|d| format!("{d}\n  domain: {domain:?}\n  codomain: {codomain:?}"))?;
            }
        }
        Ok(n)
    });
    let formula: u64 = bare
        .iter()
        .map(|b| {
            let constants = b.ops.iter().filter(|op| op.inputs.is_empty()).count() as u64;
            equations_count(constants).pow(2) * assignment_count(b, b)
        })
        .sum();
    println!(
        "bound: every bare signature with each pair of its equation variants (<= 1 equation, \
         depth <= 1) and every total assignment of the signature to itself"
    );
    println!("compared {compared}; by formula {formula}");
    assert_eq!(compared, formula);
}

// ─── composition: L-theories-3, L-theories-4, L-theories-5 ─────────

/// Production and reference compositions of one diagram, the production
/// side converted, with an invalid leg's rendered refusal read through
/// the production morphism check it renders.
fn compose_outcomes(
    diagram: &r::TheoryDiagram,
    options: &r::ComposeOptions,
) -> (Result<r::Composition, String>, Result<r::Composition, String>) {
    let production = panproto_gat::compose(&convert::diagram(diagram), &convert::options(options))
        .map(|c| {
            let labels: Vec<String> = diagram.nodes.iter().map(|n| n.label.clone()).collect();
            convert::back_composition(&c, &labels)
        })
        .map_err(|e| production_refusal(diagram, &e));
    let reference = r::compose(diagram, options).map_err(|e| reference_refusal(&e));
    (production, reference)
}

fn sym_text(s: &panproto_gat::SymRef) -> String {
    format!("{}.{}", s.node, s.name)
}

fn ref_sym_text(s: &r::SymRef) -> String {
    format!("{}.{}", s.node, s.name)
}

/// A production composition refusal as the text of its typed reason.
fn production_refusal(diagram: &r::TheoryDiagram, e: &panproto_gat::CompositionError) -> String {
    use panproto_gat::CompositionError as E;
    match e {
        E::DuplicateNodeLabel { label } => format!("DuplicateNodeLabel({label})"),
        E::InvalidNodeLabel { label } => format!("InvalidNodeLabel({label})"),
        E::UnknownNodeLabel { label } => format!("UnknownNodeLabel({label})"),
        E::InvalidNode { label, defects } => format!(
            "InvalidNode({label}, {:?})",
            defects.iter().map(convert::defect).collect::<Vec<_>>()
        ),
        E::InvalidLeg { edge, detail } => {
            let Some(leg) = diagram.edges.get(*edge) else {
                return format!("InvalidLeg({edge}, unread {detail:?})");
            };
            let node = |label: &str| {
                diagram
                    .nodes
                    .iter()
                    .find(|n| n.label == label)
                    .map(|n| convert::theory(&n.theory))
                    .expect("endpoint")
            };
            let checked = panproto_gat::check_morphism(
                &convert::morphism(&leg.morphism),
                &node(&leg.src),
                &node(&leg.tgt),
            );
            match checked {
                Err(error) if error.to_string() == *detail => format!(
                    "InvalidLeg({edge}, {:?})",
                    convert::morphism_error(&error)
                ),
                other => format!("InvalidLeg({edge}, unexplained {detail:?}; check gives {other:?})"),
            }
        }
        E::SortSignatureMismatch {
            first,
            second,
            detail,
        } => format!(
            "SortSignatureMismatch({}, {}, {detail:?})",
            sym_text(first),
            sym_text(second)
        ),
        E::OpSignatureMismatch { first, second } => {
            format!("OpSignatureMismatch({}, {})", sym_text(first), sym_text(second))
        }
        E::QuotientNotRepresentable {
            first,
            second,
            reason,
        } => format!(
            "QuotientNotRepresentable({}, {}, {reason:?})",
            sym_text(first),
            sym_text(second)
        ),
        E::NameConflict {
            kind,
            name,
            first,
            second,
        } => format!(
            "NameConflict({:?}, {name}, {}, {})",
            convert::sym_kind(*kind),
            sym_text(first),
            sym_text(second)
        ),
        E::NameClash {
            kind,
            name,
            classes,
        } => format!(
            "NameClash({:?}, {name}, {:?})",
            convert::sym_kind(*kind),
            classes
                .iter()
                .map(|c| c.iter().map(sym_text).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        ),
        E::IllTypedTransport { source, .. } => format!("IllTypedTransport({})", sym_text(source)),
        E::IllTypedResult { defects } => format!(
            "IllTypedResult({:?})",
            defects.iter().map(convert::defect).collect::<Vec<_>>()
        ),
        E::NotACocone { class } => format!(
            "NotACocone({:?})",
            class.iter().map(sym_text).collect::<Vec<_>>()
        ),
    }
}

/// A reference composition refusal as the text of its typed reason.
fn reference_refusal(e: &r::ComposeError) -> String {
    use r::ComposeError as E;
    match e {
        E::DuplicateNodeLabel(label) => format!("DuplicateNodeLabel({label})"),
        E::InvalidNodeLabel(label) => format!("InvalidNodeLabel({label})"),
        E::UnknownNodeLabel(label) => format!("UnknownNodeLabel({label})"),
        E::InvalidNode(label, defects) => format!(
            "InvalidNode({label}, {:?})",
            defects.iter().map(convert::reduce_defect).collect::<Vec<_>>()
        ),
        E::InvalidLeg { edge, detail } => format!(
            "InvalidLeg({edge}, {:?})",
            Some(convert::reduce_morphism_error(detail))
        ),
        E::MissingSortMapping(name) => format!("MissingSortMapping({name})"),
        E::MissingOpMapping(name) => format!("MissingOpMapping({name})"),
        E::SortSignatureMismatch(first, second, detail) => format!(
            "SortSignatureMismatch({}, {}, {detail:?})",
            ref_sym_text(first),
            ref_sym_text(second)
        ),
        E::OpSignatureMismatch(first, second) => format!(
            "OpSignatureMismatch({}, {})",
            ref_sym_text(first),
            ref_sym_text(second)
        ),
        E::QuotientNotRepresentable {
            first,
            second,
            reason,
        } => format!(
            "QuotientNotRepresentable({}, {}, {reason:?})",
            ref_sym_text(first),
            ref_sym_text(second)
        ),
        E::NameConflict(kind, name, first, second) => format!(
            "NameConflict({kind:?}, {name}, {}, {})",
            ref_sym_text(first),
            ref_sym_text(second)
        ),
        E::NameClash(kind, name, classes) => format!(
            "NameClash({kind:?}, {name}, {:?})",
            classes
                .iter()
                .map(|c| c.iter().map(ref_sym_text).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        ),
        E::IllTypedTransport { source, .. } => format!("IllTypedTransport({})", ref_sym_text(source)),
        E::IllTypedResult(defects) => format!(
            "IllTypedResult({:?})",
            defects.iter().map(convert::reduce_defect).collect::<Vec<_>>()
        ),
        E::NotACocone(class) => format!(
            "NotACocone({:?})",
            class.iter().map(ref_sym_text).collect::<Vec<_>>()
        ),
    }
}

fn compose_agrees(diagram: &r::TheoryDiagram, options: &r::ComposeOptions) -> Result<(), String> {
    let (production, reference) = compose_outcomes(diagram, options);
    if production == reference {
        Ok(())
    } else {
        Err(format!(
            "composition disagreement\n  diagram: {diagram:?}\n  options: {options:?}\n  production: {production:?}\n  reference: {reference:?}"
        ))
    }
}

/// The spans of the span scope: apex, feet and both legs, each leg a
/// morphism by the reference check.
fn spans(apexes: &[r::Theory], feet: &[r::Theory]) -> Vec<r::TheoryDiagram> {
    let mut out = Vec::new();
    for apex in apexes {
        let legs: Vec<(&r::Theory, r::Morphism)> = feet
            .iter()
            .flat_map(|foot| {
                scope::morphisms(apex, foot)
                    .into_iter()
                    .filter(|m| r::check_morphism(apex, foot, m).is_ok())
                    .map(move |m| (foot, m))
            })
            .collect();
        for (left, i1) in &legs {
            for (right, i2) in &legs {
                out.push(r::TheoryDiagram::span(
                    ("S".to_owned(), apex.clone()),
                    ("L".to_owned(), (*left).clone()),
                    ("R".to_owned(), (*right).clone()),
                    i1.clone(),
                    i2.clone(),
                ));
            }
        }
    }
    out
}

/// The span scope's signatures: the bare signatures with at most one
/// operation.
fn span_signatures() -> Vec<r::Theory> {
    scope::bare_signatures()
        .into_iter()
        .filter(|t| t.ops.len() <= 1)
        .collect()
}

#[test]
fn composition_agrees_with_the_reference_on_every_span() {
    let signatures = span_signatures();
    let diagrams = spans(&signatures, &signatures);
    let options = scope::option_sets();
    println!(
        "bound: apex and feet every bare signature with at most one operation ({}); legs every \
         total assignment the reference accepts; options: both modes x both clash policies",
        signatures.len()
    );
    println!("spans: {}; compositions: {}", diagrams.len(), diagrams.len() * options.len());
    let compared = parallel(&diagrams, |d| {
        let mut n = 0;
        for o in &options {
            compose_agrees(d, o)?;
            n += 1;
        }
        Ok(n)
    });
    println!("compared {compared}");
    assert_eq!(compared as usize, diagrams.len() * options.len());
}

/// Every discrete diagram of two nodes `L` and `R`, each a signature of
/// the two-sort alphabet with at most one operation, with and without an
/// equation.
fn discrete_diagrams() -> Vec<r::TheoryDiagram> {
    let nodes: Vec<r::Theory> = scope::signatures()
        .into_iter()
        .filter(|t| t.ops.len() <= 1)
        .collect();
    let mut out = Vec::new();
    for left in &nodes {
        for right in &nodes {
            out.push(r::TheoryDiagram::discrete(vec![
                ("L".to_owned(), left.clone()),
                ("R".to_owned(), right.clone()),
            ]));
        }
    }
    out
}

#[test]
fn composition_agrees_with_the_reference_on_every_discrete_pair() {
    let diagrams = discrete_diagrams();
    let options = scope::option_sets();
    println!(
        "bound: two nodes, each a signature with at most one operation and at most one equation; \
         {} diagrams x {} option sets",
        diagrams.len(),
        options.len()
    );
    let compared = parallel(&diagrams, |d| {
        let mut n = 0;
        for o in &options {
            compose_agrees(d, o)?;
            n += 1;
        }
        Ok(n)
    });
    println!("compared {compared}");
    assert_eq!(compared as usize, diagrams.len() * options.len());
}

// ─── recipe replay: L-theories-8 ───────────────────────────────────

fn recipe(spec: &r::CompositionSpec) -> panproto_gat::CompositionSpec {
    use panproto_gat::{CompositionStep as S, RecipeEdge, RecipeOpImage};
    panproto_gat::CompositionSpec {
        result_name: spec.result_name.clone(),
        steps: spec
            .steps
            .iter()
            .map(|step| match step {
                r::CompositionStep::Base(name) => S::Base(name.clone()),
                r::CompositionStep::Colimit {
                    left,
                    right,
                    shared_sorts,
                    shared_ops,
                    mode,
                } => S::Colimit {
                    left: left.clone(),
                    right: right.clone(),
                    shared_sorts: shared_sorts.clone(),
                    shared_ops: shared_ops.clone(),
                    mode: convert::mode(*mode),
                },
                r::CompositionStep::Diagram {
                    nodes,
                    edges,
                    mode,
                    clash,
                    rewrite_budget,
                    inline,
                } => S::Diagram {
                    nodes: nodes.clone(),
                    edges: edges
                        .iter()
                        .map(|e| RecipeEdge {
                            src: e.src.clone(),
                            tgt: e.tgt.clone(),
                            sorts: e.sorts.clone(),
                            ops: e
                                .ops
                                .iter()
                                .map(|(n, image)| {
                                    (
                                        n.clone(),
                                        match image {
                                            r::RecipeOpImage::Op(o) => RecipeOpImage::Op(o.clone()),
                                            r::RecipeOpImage::Term(t) => {
                                                RecipeOpImage::Term(convert::term(t))
                                            }
                                        },
                                    )
                                })
                                .collect(),
                        })
                        .collect(),
                    mode: convert::mode(*mode),
                    clash: convert::clash(*clash),
                    rewrite_budget: *rewrite_budget,
                    inline: inline.iter().map(convert::theory).collect(),
                },
            })
            .collect(),
        version: spec.version,
    }
}

fn recompose_outcomes(
    left: &r::Theory,
    right: &r::Theory,
    spec: &r::CompositionSpec,
) -> (Result<r::Theory, String>, Result<r::Theory, String>) {
    let mut production_registry = HashMap::new();
    production_registry.insert("L".to_owned(), convert::theory(left));
    production_registry.insert("R".to_owned(), convert::theory(right));
    let mut reference_registry = BTreeMap::new();
    reference_registry.insert("L".to_owned(), left.clone());
    reference_registry.insert("R".to_owned(), right.clone());
    let production = panproto_gat::recompose(&recipe(spec), &production_registry)
        .map(|t| convert::back_theory(&t))
        .map_err(|e| production_replay_refusal(&e));
    let reference = r::recompose(spec, &reference_registry).map_err(|e| reference_replay_refusal(&e));
    (production, reference)
}

/// The typed reason of a refused composition inside a replay, with an
/// invalid leg read by its edge alone (the replayed diagram is not at
/// hand; the leg's own refusal is compared exactly by the morphism tests).
fn replay_composition_refusal(text: String) -> String {
    match text.find("InvalidLeg(") {
        Some(0) => text.split(',').next().map(|edge| format!("{edge})")).unwrap_or(text),
        _ => text,
    }
}

/// A production replay refusal as the text of its typed reason.
fn production_replay_refusal(e: &panproto_gat::GatError) -> String {
    use panproto_gat::GatError as G;
    match e {
        G::TheoryNotFound(name) => format!("TheoryNotFound({name})"),
        G::MissingSortMapping(text) | G::MissingOpMapping(text) => {
            let name = text.split('`').nth(1).unwrap_or(text);
            format!("MissingShared({name})")
        }
        G::FactorizationError(text) => {
            let name = text.split('\'').nth(1).unwrap_or_default();
            format!("MissingShared({name})")
        }
        G::Composition(inner) => format!(
            "Composition({})",
            replay_composition_refusal(production_refusal(&r::TheoryDiagram::default(), inner))
        ),
        other => format!("unmapped {other:?}"),
    }
}

/// A reference replay refusal as the text of its typed reason.
fn reference_replay_refusal(e: &r::RecomposeError) -> String {
    match e {
        r::RecomposeError::TheoryNotFound(name) => format!("TheoryNotFound({name})"),
        r::RecomposeError::MissingShared(name) => format!("MissingShared({name})"),
        r::RecomposeError::Composition(inner) => format!(
            "Composition({})",
            replay_composition_refusal(reference_refusal(inner))
        ),
    }
}

#[test]
fn recipe_replay_agrees_with_the_reference_on_every_recipe() {
    let signatures = span_signatures();
    let mut compared = 0u64;
    let mut recipes = 0u64;
    for left in &signatures {
        for right in &signatures {
            for spec in scope::recipes(left, right) {
                recipes += 1;
                let (production, reference) = recompose_outcomes(left, right, &spec);
                assert!(
                    production == reference,
                    "replay disagreement\n  left: {left:?}\n  right: {right:?}\n  spec: {spec:?}\n  production: {production:?}\n  reference: {reference:?}"
                );
                compared += 1;
            }
        }
    }
    println!("recipes: {recipes}; compared {compared}");
}

// ─── the property differential over the same constructor families ──

/// One drawn case: indices into the families, reduced modulo their sizes.
#[derive(Clone, Debug)]
struct Case {
    first: usize,
    second: usize,
    third: usize,
    raw: usize,
    leg: usize,
    other_leg: usize,
    options: usize,
    recipe: usize,
    size: usize,
    identifications: Vec<(usize, usize)>,
}

/// The seed of the property run: `PROPTEST_RNG_SEED` when it is set, a
/// fresh one otherwise; printed either way so a failure can be replayed.
fn property_config() -> proptest::test_runner::Config {
    use proptest::test_runner::{Config, RngSeed};
    let mut config = Config::default();
    if config.rng_seed == RngSeed::Random {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        config.rng_seed = RngSeed::Fixed(u64::try_from(nanos % u128::from(u64::MAX)).unwrap_or(0));
    }
    config
}

#[test]
fn randomized_theory_differential() {
    use proptest::prelude::*;
    use proptest::test_runner::{RngSeed, TestError, TestRunner};

    let signatures = scope::signatures();
    let raw = scope::raw_signatures();
    let options = scope::option_sets();
    let config = property_config();
    let RngSeed::Fixed(seed) = config.rng_seed else {
        unreachable!("the seed is fixed above")
    };
    println!(
        "property run: {} cases, seed {seed} (replay with PROPTEST_RNG_SEED={seed}); \
         families: {} signatures, {} raw signatures, {} option sets, identification lists over \
         sizes <= 6 of length <= 5",
        config.cases,
        signatures.len(),
        raw.len(),
        options.len()
    );
    let strategy = (
        (0..signatures.len(), 0..signatures.len(), 0..signatures.len(), 0..raw.len()),
        (any::<usize>(), any::<usize>(), 0..options.len(), any::<usize>()),
        (0..=6usize).prop_flat_map(|size| {
            (
                Just(size),
                proptest::collection::vec((0..=size, 0..=size), 0..=5),
            )
        }),
    )
        .prop_map(
            |((first, second, third, raw), (leg, other_leg, options, recipe), (size, identifications))| Case {
                first,
                second,
                third,
                raw,
                leg,
                other_leg,
                options,
                recipe,
                size,
                identifications,
            },
        );
    let mut runner = TestRunner::new(config);
    let result = runner.run(&strategy, |case| {
        let (a, b, c) = (&signatures[case.first], &signatures[case.second], &signatures[case.third]);
        prop_assert!(typecheck_agrees(a), "signature checking of {a:?}");
        prop_assert!(typecheck_agrees(&raw[case.raw]), "signature checking of {:?}", raw[case.raw]);

        let mut candidates = scope::morphisms(a, b);
        candidates.extend(scope::partial_morphisms(a, b));
        if !candidates.is_empty() {
            let m = &candidates[case.leg % candidates.len()];
            prop_assert!(morphism_agrees(a, b, m), "morphism check of {m:?}: {a:?} -> {b:?}");
        }

        let valid = |foot: &r::Theory| -> Vec<r::Morphism> {
            scope::morphisms(a, foot)
                .into_iter()
                .filter(|m| r::check_morphism(a, foot, m).is_ok())
                .collect()
        };
        let (left_legs, right_legs) = (valid(b), valid(c));
        let o = &options[case.options];
        if !left_legs.is_empty() && !right_legs.is_empty() {
            let diagram = r::TheoryDiagram::span(
                ("S".to_owned(), a.clone()),
                ("L".to_owned(), b.clone()),
                ("R".to_owned(), c.clone()),
                left_legs[case.leg % left_legs.len()].clone(),
                right_legs[case.other_leg % right_legs.len()].clone(),
            );
            prop_assert!(compose_agrees(&diagram, o).is_ok(), "{}", compose_agrees(&diagram, o).unwrap_err());
        }
        let discrete = r::TheoryDiagram::discrete(vec![("L".to_owned(), b.clone()), ("R".to_owned(), c.clone())]);
        prop_assert!(compose_agrees(&discrete, o).is_ok(), "{}", compose_agrees(&discrete, o).unwrap_err());

        prop_assert_eq!(
            production_closure(case.size, &case.identifications),
            r::quotient(case.size, &case.identifications)
        );

        let recipes = scope::recipes(b, c);
        let spec = &recipes[case.recipe % recipes.len()];
        let (production, reference) = recompose_outcomes(b, c, spec);
        prop_assert_eq!(production, reference, "replay of {:?}", spec);
        Ok(())
    });
    match result {
        Ok(()) => println!("property run passed with seed {seed}"),
        Err(TestError::Fail(reason, case)) => {
            panic!("property run failed with seed {seed}: {reason}\nshrunk counterexample: {case:?}")
        }
        Err(TestError::Abort(reason)) => panic!("property run aborted with seed {seed}: {reason}"),
    }
}
