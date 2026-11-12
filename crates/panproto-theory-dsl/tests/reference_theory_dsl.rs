//! Differential test of theory DSL compilation against the reference model.
//!
//! Every document of the finite scopes of `panproto_reference::enumerate::
//! theory_dsl` is compiled from its JSON text by [`compile_source`] and by the
//! reference's `compile_document`. The two must agree: on an accepted
//! document, every compiled theory, morphism and protocol; on a refused one,
//! the refusal, its document path, and the byte range the production refusal
//! carries, which must be the reference's location of that path in the text.
//! A refused document is also read as YAML, which JSON text is: production
//! must refuse it at the same path, its span the first character of the
//! value the reference locates there.

use std::collections::BTreeMap;
use std::sync::Arc;

use panproto_gat as p;
use panproto_reference::enumerate::theory_dsl as scope;
use panproto_reference::pages::theories::theory as r;
use panproto_reference::pages::theory_dsl as rd;
use panproto_theory_dsl::shape::{Expected, ShapeProblem};
use panproto_theory_dsl::{
    DocumentPath, PathSegment, SourceFormat, TheoryDslError, compile_source, read_source,
};
use proptest::prelude::*;

mod convert {
    //! Conversions between the reference model and production types, over
    //! the fragment the scopes use: structural open sorts with parameters,
    //! operations with implicit inputs, first-order terms.

    use super::{Arc, BTreeMap, p, r, rd};

    fn sort_expr(s: &r::SortExpr) -> p::SortExpr {
        match s {
            r::SortExpr::Name(n) => p::SortExpr::Name(Arc::from(n.as_str())),
            r::SortExpr::App(n, args) => {
                p::SortExpr::app(Arc::from(n.as_str()), args.iter().map(term).collect())
            }
        }
    }

    fn term(t: &r::Term) -> p::Term {
        match t {
            r::Term::Var(x) => p::Term::Var(Arc::from(x.as_str())),
            r::Term::App(op, args) => p::Term::App {
                op: Arc::from(op.as_str()),
                args: args.iter().map(term).collect(),
            },
            other => panic!("the scopes use first-order terms only: {other}"),
        }
    }

    /// A reference library theory as a production theory.
    pub fn theory(t: &r::Theory) -> p::Theory {
        p::Theory::full(
            t.name.as_str(),
            t.extends.iter().map(|e| Arc::from(e.as_str())).collect(),
            t.sorts
                .iter()
                .map(|s| {
                    assert!(s.kind == r::SortKind::Structural && s.constructors.is_none());
                    p::Sort::dependent(
                        s.name.as_str(),
                        s.params
                            .iter()
                            .map(|(x, q)| p::SortParam::new(x.as_str(), sort_expr(q)))
                            .collect(),
                    )
                })
                .collect(),
            t.ops
                .iter()
                .map(|o| {
                    p::Operation::with_implicit(
                        o.name.as_str(),
                        o.inputs
                            .iter()
                            .map(|(x, s, i)| {
                                let implicit = match i {
                                    r::Implicit::No => p::Implicit::No,
                                    r::Implicit::Yes => p::Implicit::Yes,
                                };
                                (Arc::from(x.as_str()), sort_expr(s), implicit)
                            })
                            .collect(),
                        sort_expr(&o.output),
                    )
                })
                .collect(),
            t.eqs
                .iter()
                .map(|e| p::Equation::new(e.name.as_str(), term(&e.lhs), term(&e.rhs)))
                .collect(),
            Vec::new(),
            Vec::new(),
        )
    }

    fn back_term(t: &p::Term) -> r::Term {
        match t {
            p::Term::Var(x) => r::Term::Var(x.to_string()),
            p::Term::App { op, args } => {
                r::Term::App(op.to_string(), args.iter().map(back_term).collect())
            }
            other => panic!("a compiled theory of the scopes holds first-order terms: {other:?}"),
        }
    }

    fn back_sort_expr(s: &p::SortExpr) -> r::SortExpr {
        match s {
            p::SortExpr::Name(n) => r::SortExpr::Name(n.to_string()),
            p::SortExpr::App { name, args } => {
                r::SortExpr::App(name.to_string(), args.iter().map(back_term).collect())
            }
        }
    }

    /// A compiled production theory in the reference model.
    pub fn back_theory(t: &p::Theory) -> r::Theory {
        assert!(t.directed_eqs.is_empty() && t.policies.is_empty());
        r::Theory {
            name: t.name.to_string(),
            extends: t.extends.iter().map(ToString::to_string).collect(),
            sorts: t
                .sorts
                .iter()
                .map(|s| {
                    assert!(matches!(s.kind, p::SortKind::Structural));
                    assert!(matches!(s.closure, p::SortClosure::Open));
                    r::Sort {
                        name: s.name.to_string(),
                        params: s
                            .params
                            .iter()
                            .map(|q| (q.name.to_string(), back_sort_expr(&q.sort)))
                            .collect(),
                        kind: r::SortKind::Structural,
                        constructors: None,
                    }
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
                        .map(|(x, s, i)| {
                            let implicit = if matches!(i, p::Implicit::Yes) {
                                r::Implicit::Yes
                            } else {
                                r::Implicit::No
                            };
                            (x.to_string(), back_sort_expr(s), implicit)
                        })
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
            directed_eqs: Vec::new(),
            policies: Vec::new(),
        }
    }

    /// A compiled production morphism in the reference model.
    pub fn back_morphism(m: &p::TheoryMorphism) -> r::Morphism {
        r::Morphism {
            name: m.name.to_string(),
            domain: m.domain.to_string(),
            codomain: m.codomain.to_string(),
            sorts: m
                .sort_map
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
            ops: m
                .op_map
                .iter()
                .map(|(k, v)| {
                    let image = match v {
                        p::OpAssignment::Op(o) => r::OpImage::Op(o.to_string()),
                        p::OpAssignment::Term(t) => r::OpImage::Term(back_term(t)),
                    };
                    (k.to_string(), image)
                })
                .collect(),
        }
    }

    fn site(s: &p::SignatureSite) -> r::Site {
        match s {
            p::SignatureSite::SortParam { sort, index } => {
                r::Site::SortParam(sort.to_string(), *index)
            }
            p::SignatureSite::OpInput { op, index } => r::Site::OpInput(op.to_string(), *index),
            p::SignatureSite::OpOutput { op } => r::Site::OpOutput(op.to_string()),
        }
    }

    const fn sym_kind(kind: p::SymKind) -> r::SymKind {
        match kind {
            p::SymKind::Sort => r::SymKind::Sort,
            p::SymKind::Op => r::SymKind::Op,
            p::SymKind::Eq => r::SymKind::Eq,
            p::SymKind::DirectedEq => r::SymKind::DirectedEq,
            p::SymKind::Policy => r::SymKind::Policy,
        }
    }

    /// A production signature defect in the reference model.
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
                found.to_string(),
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

    /// The production protocol a reference protocol denotes: its names and
    /// edge rules, every feature flag off, no vertex kinds, constraint sorts
    /// or composition recipes.
    pub fn protocol(p: &rd::Protocol) -> panproto_schema::Protocol {
        panproto_schema::Protocol {
            name: p.name.clone(),
            schema_theory: p.schema_theory.clone(),
            instance_theory: p.instance_theory.clone(),
            edge_rules: p
                .edge_rules
                .iter()
                .map(|(edge, src, tgt)| panproto_schema::EdgeRule {
                    edge_kind: edge.clone(),
                    src_kinds: vec![src.clone()],
                    tgt_kinds: vec![tgt.clone()],
                })
                .collect(),
            ..panproto_schema::Protocol::default()
        }
    }

    /// A production composition recipe in the reference model; a step the
    /// reference does not record (a base or a diagram) becomes a base step
    /// naming its kind, which no reference recipe holds.
    pub fn recipe(spec: &p::CompositionSpec) -> r::CompositionSpec {
        r::CompositionSpec {
            result_name: spec.result_name.clone(),
            steps: spec
                .steps
                .iter()
                .map(|step| match step {
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
                        mode: match mode {
                            p::CompositionMode::Amalgamate => r::CompositionMode::Amalgamate,
                            p::CompositionMode::ExplicitIdentity => {
                                r::CompositionMode::ExplicitIdentity
                            }
                        },
                    },
                    p::CompositionStep::Base(_) => r::CompositionStep::Base("<base>".to_owned()),
                    p::CompositionStep::Diagram { .. } => {
                        r::CompositionStep::Base("<diagram>".to_owned())
                    }
                })
                .collect(),
            version: spec.version,
        }
    }

    /// The variant of a morphism refusal, by name, in either model.
    pub fn morphism_error(e: &p::GatError) -> String {
        let text = format!("{e:?}");
        let name = text.split(['(', ' ', '{']).next().unwrap_or_default();
        match name {
            "MorphismClosureMismatch" => "ClosureMismatch".to_owned(),
            other => other.to_owned(),
        }
    }
}

fn production_library(name: &str) -> Option<p::Theory> {
    scope::library(name).map(|t| convert::theory(&t))
}

fn path(p: &DocumentPath) -> rd::Path {
    rd::Path(
        p.0.iter()
            .map(|s| match s {
                PathSegment::Key(k) => rd::Segment::Key(k.clone()),
                PathSegment::Index(i) => rd::Segment::Index(*i),
            })
            .collect(),
    )
}

/// The reference's form of a production grammar refusal; `None` for a
/// problem the theory grammar has no place for, which no reference refusal
/// matches.
fn shape_problem(problem: &ShapeProblem) -> Option<rd::ShapeProblem> {
    let expected = |e: &Expected| match e {
        Expected::Object => Some(rd::Expected::Object),
        Expected::Array => Some(rd::Expected::Array),
        Expected::String => Some(rd::Expected::String),
        Expected::Boolean => Some(rd::Expected::Boolean),
        Expected::StringOrObject => Some(rd::Expected::TheoryReference),
        Expected::Number | Expected::Natural => None,
    };
    Some(match problem {
        ShapeProblem::WrongType { expected: e } => rd::ShapeProblem::WrongType(expected(e)?),
        ShapeProblem::UnknownField => rd::ShapeProblem::UnknownField,
        ShapeProblem::MissingField { field } => rd::ShapeProblem::MissingField(field.clone()),
        ShapeProblem::UnknownVariant { found } => rd::ShapeProblem::UnknownVariant(found.clone()),
        ShapeProblem::NoBody => rd::ShapeProblem::NoBody,
        ShapeProblem::SeveralBodies { keys } => rd::ShapeProblem::SeveralBodies(keys.clone()),
        ShapeProblem::NoVariant | ShapeProblem::SeveralVariants { .. } => return None,
    })
}

/// Whether a production refusal is the reference's refusal.
fn same_refusal(production: &TheoryDslError, reference: &rd::Refusal) -> bool {
    match (production, reference) {
        (TheoryDslError::Shape { problem, .. }, rd::Refusal::Shape(expected)) => {
            shape_problem(problem).as_ref() == Some(expected)
        }
        (TheoryDslError::ColimitFailed { .. }, rd::Refusal::ColimitFailed)
        | (TheoryDslError::TermParse { .. }, rd::Refusal::TermParse)
        | (TheoryDslError::AmbiguousOpInputs { .. }, rd::Refusal::AmbiguousOpInputs) => true,
        (TheoryDslError::TheoryNotFound { name, .. }, rd::Refusal::TheoryNotFound(expected))
        | (TheoryDslError::UnknownExposed { name, .. }, rd::Refusal::UnknownExposed(expected))
        | (TheoryDslError::Duplicate { name, .. }, rd::Refusal::Duplicate(expected)) => {
            name == expected
        }
        (
            TheoryDslError::TypeCheck {
                error: p::GatError::IllTypedSignature { defects },
                ..
            },
            rd::Refusal::Signature(expected),
        ) => defects.first().map(convert::defect).as_ref() == Some(expected),
        (
            TheoryDslError::TypeCheck {
                error: p::GatError::NonInferrableImplicit { op, .. },
                ..
            },
            rd::Refusal::NonInferableImplicit(expected),
        ) => op == expected,
        (TheoryDslError::TypeCheck { error, .. }, rd::Refusal::Equation(_)) => !matches!(
            error,
            p::GatError::IllTypedSignature { .. } | p::GatError::NonInferrableImplicit { .. }
        ),
        (
            TheoryDslError::UnknownMapKey { kind, name, .. },
            rd::Refusal::UnknownMapKey(expected_kind, expected_name),
        ) => kind == expected_kind && name == expected_name,
        (TheoryDslError::MorphismCheck { error, .. }, rd::Refusal::MorphismCheck(expected)) => {
            let reference = format!("{expected:?}");
            let reference = reference.split(['(', ' ', '{']).next().unwrap_or_default();
            convert::morphism_error(error) == reference
        }
        _ => false,
    }
}

/// The disagreement between production and the reference on one JSON
/// document, if any. Documents outside the reference's domain are skipped
/// and counted.
fn disagreement(text: &str, outside: &mut usize) -> Option<String> {
    let Some(value) = panproto_reference::json::parse(text) else {
        return Some(format!("the reference cannot read {text}"));
    };
    let reference = rd::compile_document(&value, &scope::library);
    let production = compile_source(text, SourceFormat::Json, &production_library);
    match (reference, production) {
        (rd::Outcome::OutsideDomain(_), _) => {
            *outside += 1;
            None
        }
        (rd::Outcome::Accepted(expected), Ok(compiled)) => {
            let theories: BTreeMap<String, r::Theory> = compiled
                .theories
                .iter()
                .map(|(k, t)| (k.clone(), convert::back_theory(t)))
                .collect();
            let morphisms: BTreeMap<String, r::Morphism> = compiled
                .morphisms
                .iter()
                .map(|(k, m)| (k.clone(), convert::back_morphism(m)))
                .collect();
            let protocols: BTreeMap<String, panproto_schema::Protocol> = compiled
                .protocols
                .iter()
                .map(|(k, p)| (k.clone(), p.clone()))
                .collect();
            let expected_protocols: BTreeMap<String, panproto_schema::Protocol> = expected
                .protocols
                .iter()
                .map(|(k, p)| (k.clone(), convert::protocol(p)))
                .collect();
            let compositions: BTreeMap<String, r::CompositionSpec> = compiled
                .composition_specs
                .iter()
                .map(|(k, spec)| (k.clone(), convert::recipe(spec)))
                .collect();
            let same_theories = theories.len() == expected.theories.len()
                && theories.iter().all(|(name, theory)| {
                    expected.theories.get(name).is_some_and(|reference| {
                        if expected.compositions.contains_key(name) {
                            r::canonical_theory_bytes(theory)
                                == r::canonical_theory_bytes(reference)
                        } else {
                            theory == reference
                        }
                    })
                });
            (!same_theories
                || morphisms != expected.morphisms
                || protocols != expected_protocols
                || compositions != expected.compositions)
                .then(|| {
                    format!(
                        "{text}\n  reference {:?} {:?} {expected_protocols:?}\n  production {theories:?} {morphisms:?} {protocols:?}",
                        expected.theories, expected.morphisms
                    )
                })
        }
        (rd::Outcome::Refused(at, refusal), Err(error)) => {
            let located = error.path().map(path);
            let span = span_of(&error);
            let expected_span = rd::locate(text, &at);
            let yaml = compile_source(text, SourceFormat::Yaml, &production_library);
            let agrees = located.as_ref() == Some(&at)
                && same_refusal(error.unlocated(), &refusal)
                && span == expected_span
                && yaml_refusal_agrees(yaml, text, &at, &refusal);
            (!agrees).then(|| {
                format!(
                    "{text}\n  reference {refusal:?} at {at:?} span {expected_span:?}\n  production {error:?}"
                )
            })
        }
        (reference, production) => Some(format!(
            "{text}\n  reference {reference:?}\n  production {production:?}"
        )),
    }
}

/// The span of a spanned refusal.
const fn span_of(error: &TheoryDslError) -> Option<(usize, usize)> {
    match error {
        TheoryDslError::Spanned { span, .. } => Some((span.offset(), span.len())),
        _ => None,
    }
}

/// The span production gives a refusal of YAML text: the first character
/// of the value the reference locates in the same text read as JSON, which
/// JSON text also is as YAML.
fn yaml_span(text: &str, at: &rd::Path) -> Option<(usize, usize)> {
    rd::locate(text, at).map(|(offset, _)| {
        let length = text[offset..].chars().next().map_or(0, char::len_utf8);
        (offset, length)
    })
}

/// Whether production refuses the JSON text `text`, read as YAML, at `at`
/// with the reference's refusal and the YAML span of `at`.
fn yaml_refusal_agrees<T>(
    production: Result<T, TheoryDslError>,
    text: &str,
    at: &rd::Path,
    refusal: &rd::Refusal,
) -> bool {
    production.err().is_some_and(|error| {
        error.path().map(path).as_ref() == Some(at)
            && same_refusal(error.unlocated(), refusal)
            && span_of(&error) == yaml_span(text, at)
    })
}

/// Run `f` on a thread with a 32 MiB stack: the reference's equation
/// preservation search recurses over terms that the default test thread's
/// stack cannot hold in an unoptimised build.
fn on_a_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    match std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(f)
        .map(std::thread::JoinHandle::join)
    {
        Ok(Ok(value)) => value,
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(error) => panic!("cannot spawn the test thread: {error}"),
    }
}

fn assert_agreement(documents: Vec<String>, expected_count: usize, expected_outside: usize) {
    assert_eq!(documents.len(), expected_count);
    let (disagreements, outside) = on_a_large_stack(move || {
        let mut outside = 0;
        let found: Vec<String> = documents
            .iter()
            .filter_map(|text| disagreement(text, &mut outside))
            .collect();
        (found, outside)
    });
    assert!(
        disagreements.is_empty(),
        "{} of {} documents disagree; first:\n{}",
        disagreements.len(),
        expected_count,
        disagreements
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(outside, expected_outside);
}

#[test]
fn theory_documents_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        scope::theory_documents(),
        scope::EXTENDS.len()
            * scope::IMPORTS.len()
            * scope::SORTS.len()
            * scope::OPS.len()
            * scope::EQUATIONS.len(),
        0,
    );
}

#[test]
fn dependent_theory_documents_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        scope::dependent_documents(),
        scope::DEPENDENT_EXTENDS.len()
            * scope::DEPENDENT_IMPORTS.len()
            * scope::DEPENDENT_SORTS.len()
            * scope::DEPENDENT_OPS.len()
            * scope::DEPENDENT_EQUATIONS.len(),
        0,
    );
}

#[test]
fn morphism_documents_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        scope::morphism_documents(),
        scope::MORPHISM_ENDS.len()
            * scope::SORT_IMAGES.len().pow(2)
            * scope::EXTRA_SORT_ENTRY.len()
            * scope::OP_IMAGES.len().pow(3),
        0,
    );
}

#[test]
fn bundles_compile_their_theories_in_listed_order_as_the_reference() {
    assert_agreement(scope::bundle_documents(), 1 + 4 + 16 + 64, 0);
}

#[test]
fn composition_documents_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        scope::composition_documents(),
        scope::COMPOSITION_SIDES.len()
            * scope::COMPOSITION_SHARED_SORTS.len()
            * scope::COMPOSITION_SHARED_OPS.len()
            * scope::COMPOSITION_MODES.len()
            * scope::COMPOSITION_BASES.len()
            * scope::COMPOSITION_SECOND_STEPS.len(),
        0,
    );
}

#[test]
fn bundled_compositions_compile_in_listed_order_as_the_reference() {
    assert_agreement(scope::composition_bundle_documents(), 1 + 4 + 16 + 64, 0);
}

#[test]
fn protocol_documents_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        scope::protocol_documents(),
        scope::PROTOCOL_SCHEMAS.len()
            * scope::PROTOCOL_INSTANCES.len()
            * scope::PROTOCOL_EDGE_RULES.len(),
        0,
    );
}

#[test]
fn bundled_protocols_record_their_inline_theories_as_the_reference() {
    assert_agreement(scope::protocol_bundle_documents(), 1 + 4 + 16 + 64, 0);
}

/// The grammar check of every mutated document: production's reading of
/// the text against the reference's grammar, refusal, path and span.
fn grammar_disagreement(text: &str) -> Option<String> {
    let Some(value) = panproto_reference::json::parse(text) else {
        return Some(format!("the reference cannot read {text}"));
    };
    let reference = rd::check_document(&value);
    let production = read_source(text, SourceFormat::Json);
    match (reference, production) {
        (Ok(_), Ok(_)) => None,
        (Err((at, problem)), Err(error)) => {
            let refusal = rd::Refusal::Shape(problem.clone());
            let agrees = error.path().map(path).as_ref() == Some(&at)
                && same_refusal(error.unlocated(), &refusal)
                && span_of(&error) == rd::locate(text, &at)
                && yaml_refusal_agrees(read_source(text, SourceFormat::Yaml), text, &at, &refusal);
            (!agrees).then(|| {
                format!("{text}\n  reference {problem:?} at {at:?}\n  production {error:?}")
            })
        }
        (reference, production) => Some(format!(
            "{text}\n  reference {reference:?}\n  production {production:?}"
        )),
    }
}

#[test]
fn the_grammar_refuses_as_the_reference_on_every_mutated_document() {
    let (replaced, removed, added) = scope::shape_documents();
    assert_eq!(replaced.len(), 153 * scope::REPLACEMENTS.len());
    assert_eq!(removed.len(), 122);
    assert_eq!(added.len(), 35);
    let disagreements: Vec<String> = replaced
        .iter()
        .chain(&removed)
        .chain(&added)
        .filter_map(|text| grammar_disagreement(text))
        .collect();
    assert!(
        disagreements.is_empty(),
        "{} documents disagree; first:\n{}",
        disagreements.len(),
        disagreements
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A theory document of several operations and equations drawn from the
/// scope's axes.
fn longer_theory() -> impl Strategy<Value = String> {
    let op_bodies: Vec<String> = scope::OPS[1..]
        .iter()
        .map(|o| {
            o.trim_start_matches(",\"ops\":[")
                .trim_end_matches(']')
                .to_owned()
        })
        .collect();
    let eq_bodies: Vec<String> = scope::EQUATIONS[1..]
        .iter()
        .map(|e| {
            e.trim_start_matches(",\"equations\":[")
                .trim_end_matches(']')
                .to_owned()
        })
        .collect();
    (
        0..scope::EXTENDS.len(),
        0..scope::IMPORTS.len(),
        0..scope::SORTS.len(),
        prop::collection::vec(prop::sample::select(op_bodies), 0..5),
        prop::collection::vec(prop::sample::select(eq_bodies), 0..5),
    )
        .prop_map(|(e, i, s, ops, eqs)| {
            format!(
                r#"{{"id":"t","description":"","theory":"T",{}{}{},"ops":[{}],"equations":[{}]}}"#,
                scope::EXTENDS[e],
                scope::IMPORTS[i],
                scope::SORTS[s],
                ops.join(","),
                eqs.join(",")
            )
        })
}

/// The members of a scope axis as list elements: `,"key":[a]` becomes `a`.
fn axis_elements(axis: &[&str], key: &str) -> Vec<String> {
    let prefix = format!(",\"{key}\":[");
    axis[1..]
        .iter()
        .map(|member| {
            member
                .trim_start_matches(prefix.as_str())
                .trim_end_matches(']')
                .to_owned()
        })
        .collect()
}

/// A dependent theory document of several operations and equations drawn
/// from the dependent scope's axes.
fn longer_dependent_theory() -> impl Strategy<Value = String> {
    (
        0..scope::DEPENDENT_EXTENDS.len(),
        0..scope::DEPENDENT_IMPORTS.len(),
        0..scope::DEPENDENT_SORTS.len(),
        prop::collection::vec(
            prop::sample::select(axis_elements(scope::DEPENDENT_OPS, "ops")),
            0..5,
        ),
        prop::collection::vec(
            prop::sample::select(axis_elements(scope::DEPENDENT_EQUATIONS, "equations")),
            0..5,
        ),
    )
        .prop_map(|(e, i, s, ops, eqs)| {
            format!(
                r#"{{"id":"t","description":"","theory":"T",{}{}{},"ops":[{}],"equations":[{}]}}"#,
                scope::DEPENDENT_EXTENDS[e],
                scope::DEPENDENT_IMPORTS[i],
                scope::DEPENDENT_SORTS[s],
                ops.join(","),
                eqs.join(",")
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn longer_dependent_theory_documents_compile_as_the_reference(
        text in longer_dependent_theory()
    ) {
        let (found, outside) = on_a_large_stack(move || {
            let mut outside = 0;
            (disagreement(&text, &mut outside), outside)
        });
        prop_assert!(found.is_none(), "{}", found.unwrap_or_default());
        prop_assert_eq!(outside, 0);
    }

    #[test]
    fn longer_theory_documents_compile_as_the_reference(text in longer_theory()) {
        let (found, outside) = on_a_large_stack(move || {
            let mut outside = 0;
            (disagreement(&text, &mut outside), outside)
        });
        prop_assert!(found.is_none(), "{}", found.unwrap_or_default());
        prop_assert_eq!(outside, 0);
    }
}
