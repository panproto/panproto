//! Differential test of lens DSL compilation against the reference model.
//!
//! Every document of the finite scopes of `panproto_reference::enumerate::
//! lens_dsl` is compiled from its JSON text by production and by the
//! reference's `compile_document`. The two must agree: on an accepted
//! document, every stage (each protolens of its chain, elementary transform
//! by elementary transform with the residue it keeps, and each value-level
//! transform by anchor), the flat chain and transform summaries, and a
//! symmetric body's two legs; on a refused one, the refusal class, its
//! document path, and the byte range of the refused value in the text.
//!
//! The reference takes the expression grammar and the sampled coercion
//! honesty checks as parameters; the differential passes production's, so
//! those judgements are the boundary the agreement is stated relative to.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};
use panproto_gat::{CoercionClass, Term, TheoryTransform, ValueKind};
use panproto_inst::FieldTransform;
use panproto_inst::value::Value;
use panproto_lens::{ComplementConstructor, Protolens, coercion_laws};
use panproto_lens_dsl::shape::{Expected, ShapeProblem};
use panproto_lens_dsl::{
    CompiledLens, DocumentPath, LensDocument, LensDslError, PathSegment, SourceFormat,
    compile_source, compile_with_refs, read_source,
};
use panproto_reference::dsl_path::{Path, Segment, locate};
use panproto_reference::enumerate::lens_dsl as scope;
use panproto_reference::pages::lens_dsl as rd;
use proptest::prelude::*;

// ─── Oracles ────────────────────────────────────────────────────────────

fn parse_expr(text: &str) -> Option<Expr> {
    let tokens = panproto_expr_parser::tokenize(text).ok()?;
    panproto_expr_parser::parse(&tokens).ok()
}

const fn class(class: rd::Class) -> CoercionClass {
    match class {
        rd::Class::Iso => CoercionClass::Iso,
        rd::Class::Retraction => CoercionClass::Retraction,
        rd::Class::Projection => CoercionClass::Projection,
        rd::Class::Opaque => CoercionClass::Opaque,
    }
}

const fn value_kind(kind: rd::Kind) -> ValueKind {
    match kind {
        rd::Kind::Bool => ValueKind::Bool,
        rd::Kind::Int => ValueKind::Int,
        rd::Kind::Float => ValueKind::Float,
        rd::Kind::Str => ValueKind::Str,
        rd::Kind::Bytes => ValueKind::Bytes,
        rd::Kind::Token => ValueKind::Token,
        rd::Kind::Null => ValueKind::Null,
        rd::Kind::Any => ValueKind::Any,
    }
}

fn expression(text: &str) -> bool {
    parse_expr(text).is_some()
}

fn coercion(expr: &str, inverse: Option<&str>, declared: rd::Class, source: rd::Kind) -> bool {
    let (Some(forward), inverse) = (parse_expr(expr), inverse.map(parse_expr)) else {
        return false;
    };
    let registry = coercion_laws::CoercionSampleRegistry::with_defaults();
    coercion_laws::check_coercion_honesty(
        &forward,
        inverse.flatten().as_ref(),
        class(declared),
        value_kind(source),
        "v",
        &registry,
    )
    .is_ok()
}

fn term(term: &rd::Term) -> Term {
    match term {
        rd::Term::Var(name) => Term::Var(Arc::from(name.as_str())),
        rd::Term::App(op, args) => Term::App {
            op: Arc::from(op.as_str()),
            args: args.iter().map(self::term).collect(),
        },
    }
}

fn directed(equation: &rd::Elementary) -> bool {
    let rd::Elementary::DirectedEq {
        name,
        lhs,
        rhs,
        implementation,
        inverse,
        source_kind,
        target_kind,
        class: declared,
    } = equation
    else {
        return false;
    };
    let Some(impl_term) = parse_expr(implementation) else {
        return false;
    };
    let equation = panproto_gat::DirectedEquation {
        name: Arc::from(name.as_str()),
        lhs: term(lhs),
        rhs: term(rhs),
        impl_term,
        inverse: inverse.as_deref().and_then(parse_expr),
        source_kind: source_kind.map(value_kind),
        target_kind: target_kind.map(value_kind),
        coercion_class: class(*declared),
    };
    let var = match lhs {
        rd::Term::Var(name) => name.clone(),
        rd::Term::App(..) => "x".to_owned(),
    };
    let registry = coercion_laws::CoercionSampleRegistry::with_defaults();
    coercion_laws::check_directed_equation_with_registry(&equation, &registry, &var).is_empty()
}

const ORACLES: rd::Oracles<'static> = rd::Oracles {
    expression: &expression,
    coercion: &coercion,
    directed: &directed,
};

fn context(documents: &dyn Fn(&str) -> Option<panproto_reference::json::Value>) -> rd::Context<'_> {
    rd::Context {
        body: scope::BODY,
        oracles: &ORACLES,
        documents,
    }
}

// ─── Refusals ───────────────────────────────────────────────────────────

fn path(p: &DocumentPath) -> Path {
    Path(
        p.0.iter()
            .map(|s| match s {
                PathSegment::Key(k) => Segment::Key(k.clone()),
                PathSegment::Index(i) => Segment::Index(*i),
            })
            .collect(),
    )
}

const fn expected(e: Expected) -> rd::Expected {
    match e {
        Expected::Object => rd::Expected::Object,
        Expected::Array => rd::Expected::Array,
        Expected::String => rd::Expected::String,
        Expected::Boolean => rd::Expected::Boolean,
        Expected::Number => rd::Expected::Number,
        Expected::Natural => rd::Expected::Natural,
        Expected::StringOrObject => rd::Expected::StringOrObject,
    }
}

fn shape_problem(problem: &ShapeProblem) -> rd::ShapeProblem {
    match problem {
        ShapeProblem::WrongType { expected: e } => rd::ShapeProblem::WrongType(expected(*e)),
        ShapeProblem::UnknownField => rd::ShapeProblem::UnknownField,
        ShapeProblem::MissingField { field } => rd::ShapeProblem::MissingField(field.clone()),
        ShapeProblem::UnknownVariant { found } => rd::ShapeProblem::UnknownVariant(found.clone()),
        ShapeProblem::NoVariant => rd::ShapeProblem::NoVariant,
        ShapeProblem::SeveralVariants { keys } => rd::ShapeProblem::SeveralVariants(keys.clone()),
        ShapeProblem::NoBody => rd::ShapeProblem::NoBody,
        ShapeProblem::SeveralBodies { keys } => rd::ShapeProblem::SeveralBodies(keys.clone()),
    }
}

/// Whether a production refusal, with its location removed, is the
/// reference's refusal.
fn same_refusal(production: &LensDslError, reference: &rd::Refusal) -> bool {
    match (production, reference) {
        (LensDslError::Shape { problem, .. }, rd::Refusal::Shape(expected)) => {
            shape_problem(problem) == *expected
        }
        (LensDslError::ExprParse { .. }, rd::Refusal::ExprParse)
        | (LensDslError::TermParse { .. }, rd::Refusal::TermParse)
        | (LensDslError::DefaultValue { .. }, rd::Refusal::DefaultValue)
        | (LensDslError::RuleCompile { .. }, rd::Refusal::RuleCompile)
        | (LensDslError::EmptyHorizontalPart, rd::Refusal::EmptyHorizontalPart)
        | (LensDslError::ValueStepInSymmetricLeg, rd::Refusal::ValueStepInSymmetricLeg)
        | (LensDslError::AutoRequiresSchemas { .. }, rd::Refusal::AutoRequiresSchemas)
        | (LensDslError::NotInvertible { .. }, rd::Refusal::NotInvertible)
        | (LensDslError::CoercionNotHonest { .. }, rd::Refusal::CoercionNotHonest) => true,
        (LensDslError::UnresolvedRef { lens_ref }, rd::Refusal::UnresolvedRef(id))
        | (
            LensDslError::SymmetricRefInDirectionalCompose { lens_ref },
            rd::Refusal::SymmetricRef(id),
        ) => lens_ref == id,
        (LensDslError::RefCycle { cycle }, rd::Refusal::RefCycle(expected)) => cycle == expected,
        (LensDslError::InRef { lens_ref, error }, rd::Refusal::InRef(id, inner)) => {
            lens_ref == id
                && error.path().map(path).as_ref() == Some(&inner.0)
                && same_refusal(error.unlocated(), &inner.1)
        }
        _ => false,
    }
}

// ─── Stages ─────────────────────────────────────────────────────────────

const fn literal(operand: rd::Operand) -> Literal {
    match operand {
        rd::Operand::Int(i) => Literal::Int(i),
        rd::Operand::Float(bits) => Literal::Float(f64::from_bits(bits)),
    }
}

/// The expression an attribute operation on `field` denotes.
fn attr_expr(field: &str, op: &rd::AttrOp) -> Expr {
    let subject = || Expr::var(field);
    let binary = |op: BuiltinOp, a: Expr, b: Expr| Expr::Builtin(op, vec![a, b]);
    match op {
        rd::AttrOp::Add(n) => binary(BuiltinOp::Add, subject(), Expr::Lit(literal(*n))),
        rd::AttrOp::Subtract(n) => binary(BuiltinOp::Sub, subject(), Expr::Lit(literal(*n))),
        rd::AttrOp::Multiply(n) => binary(BuiltinOp::Mul, subject(), Expr::Lit(literal(*n))),
        rd::AttrOp::Prefix(s) => binary(
            BuiltinOp::Concat,
            Expr::Lit(Literal::Str(s.clone())),
            subject(),
        ),
        rd::AttrOp::Suffix(s) => binary(
            BuiltinOp::Concat,
            subject(),
            Expr::Lit(Literal::Str(s.clone())),
        ),
        rd::AttrOp::Negate => Expr::Builtin(BuiltinOp::Not, vec![subject()]),
        rd::AttrOp::ToString => Expr::int_to_str(subject()),
        rd::AttrOp::ToNumber => Expr::str_to_int(subject()),
        rd::AttrOp::ToBoolean => Expr::Match {
            scrutinee: Box::new(Expr::Builtin(BuiltinOp::TypeOf, vec![subject()])),
            arms: vec![
                (
                    Pattern::Lit(Literal::Str("string".to_owned())),
                    binary(
                        BuiltinOp::Neq,
                        subject(),
                        Expr::Lit(Literal::Str(String::new())),
                    ),
                ),
                (
                    Pattern::Lit(Literal::Str("number".to_owned())),
                    binary(BuiltinOp::Neq, subject(), Expr::Lit(Literal::Int(0))),
                ),
                (Pattern::Wildcard, subject()),
            ],
        },
    }
}

/// The expression a reference expression denotes, in the expression
/// language.
fn expr(spec: &rd::ExprSpec) -> Option<Expr> {
    match spec {
        rd::ExprSpec::Text(text) => parse_expr(text),
        rd::ExprSpec::Template(parts) => {
            let mut exprs = parts.iter().map(|part| match part {
                rd::TemplatePart::Lit(s) => Expr::Lit(Literal::Str(s.clone())),
                rd::TemplatePart::Var(v) => Expr::int_to_str(Expr::var(v.as_str())),
            });
            let first = exprs
                .next()
                .unwrap_or_else(|| Expr::Lit(Literal::Str(String::new())));
            Some(exprs.fold(first, |acc, part| {
                Expr::Builtin(BuiltinOp::Concat, vec![acc, part])
            }))
        }
        rd::ExprSpec::AttrOp(field, op) => Some(attr_expr(field, op)),
    }
}

fn same_expr(production: &Expr, reference: &rd::ExprSpec) -> bool {
    expr(reference).as_ref() == Some(production)
}

fn same_opt_expr(production: Option<&Expr>, reference: Option<&rd::ExprSpec>) -> bool {
    match (production, reference) {
        (None, None) => true,
        (Some(p), Some(r)) => same_expr(p, r),
        _ => false,
    }
}

fn same_transform(production: &FieldTransform, reference: &rd::FieldTransform) -> bool {
    match (production, reference) {
        (
            FieldTransform::ApplyExpr {
                key,
                expr,
                inverse,
                coercion_class,
            },
            rd::FieldTransform::Apply {
                key: k,
                expr: e,
                inverse: i,
                class: c,
            },
        )
        | (
            FieldTransform::ComputeField {
                target_key: key,
                expr,
                inverse,
                coercion_class,
            },
            rd::FieldTransform::Compute {
                target: k,
                expr: e,
                inverse: i,
                class: c,
            },
        ) => {
            key == k
                && same_expr(expr, e)
                && same_opt_expr(inverse.as_ref(), i.as_ref())
                && *coercion_class == class(*c)
        }
        (FieldTransform::KeepFields { keys }, rd::FieldTransform::Keep(expected)) => {
            keys == expected
        }
        _ => false,
    }
}

/// One leaf of a production protolens: an elementary transform, or a scoped
/// one with its own leaves.
enum Leaf<'a> {
    Transform(&'a TheoryTransform),
    Scoped(&'a str, Vec<Self>),
}

fn leaves<'a>(transform: &'a TheoryTransform, out: &mut Vec<Leaf<'a>>) {
    match transform {
        TheoryTransform::Compose(a, b) => {
            leaves(a, out);
            leaves(b, out);
        }
        TheoryTransform::ScopedTransform { focus, inner } => {
            let mut inside = Vec::new();
            leaves(inner, &mut inside);
            out.push(Leaf::Scoped(focus, inside));
        }
        other => out.push(Leaf::Transform(other)),
    }
}

/// Whether two complement constructors are equal, compared by their
/// serialized form.
fn same_complement(a: &ComplementConstructor, b: &ComplementConstructor) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// The complement atoms of a constructor: its non-empty leaves in order.
fn atoms<'a>(complement: &'a ComplementConstructor, out: &mut Vec<&'a ComplementConstructor>) {
    match complement {
        ComplementConstructor::Composite(parts) => {
            for part in parts {
                atoms(part, out);
            }
        }
        ComplementConstructor::Empty => {}
        other => out.push(other),
    }
}

/// The reference's elementary protolenses of a protolens, in order, scoped
/// ones with their focus.
enum Flat<'a> {
    Elementary(&'a rd::Elementary),
    Scoped(&'a str, Vec<Self>),
}

fn flatten<'a>(proto: &'a rd::Proto, out: &mut Vec<Flat<'a>>) {
    match proto {
        rd::Proto::Elementary(e) => out.push(Flat::Elementary(e)),
        rd::Proto::Scoped { focus, inner } => {
            let mut inside = Vec::new();
            flatten(inner, &mut inside);
            out.push(Flat::Scoped(focus, inside));
        }
        rd::Proto::Fused(chain) => chain.iter().for_each(|p| flatten(p, out)),
        rd::Proto::Horizontal(eta, theta) => {
            flatten(eta, out);
            flatten(theta, out);
        }
    }
}

fn name(n: &str) -> Arc<str> {
    Arc::from(n)
}

/// The complement atoms the reference's elementary protolens keeps.
fn residue(elementary: &rd::Elementary) -> Vec<ComplementConstructor> {
    let added = |element: &str, kind: &str, default: Option<Value>| {
        vec![ComplementConstructor::AddedElement {
            element_name: element.into(),
            element_kind: kind.to_owned(),
            default_value: default,
        }]
    };
    match elementary {
        rd::Elementary::AddSort {
            sort,
            kind,
            default,
        } => added(sort, kind, default.as_ref().map(datum_value)),
        rd::Elementary::AddEdge { name, kind, .. } | rd::Elementary::AddOp { name, kind, .. } => {
            added(name, kind, None)
        }
        rd::Elementary::DropEdge { src, tgt, name } => vec![ComplementConstructor::DroppedEdge {
            src: src.as_str().into(),
            tgt: tgt.as_str().into(),
            edge_name: name.as_deref().map(Into::into),
            edge_kind: "".into(),
        }],
        rd::Elementary::DropSort(sort) => {
            vec![ComplementConstructor::DroppedSortData {
                sort: sort.as_str().into(),
            }]
        }
        rd::Elementary::DropOp(op) => {
            vec![ComplementConstructor::DroppedOpData {
                op: op.as_str().into(),
            }]
        }
        rd::Elementary::CoerceSort { sort, class: c, .. } => {
            vec![ComplementConstructor::CoercedSortData {
                sort: sort.as_str().into(),
                class: class(*c),
            }]
        }
        rd::Elementary::MergeSorts { sort_a, sort_b, .. } => vec![
            ComplementConstructor::DroppedSortData {
                sort: sort_a.as_str().into(),
            },
            ComplementConstructor::DroppedSortData {
                sort: sort_b.as_str().into(),
            },
        ],
        rd::Elementary::DirectedEq {
            name,
            inverse: None,
            ..
        } => {
            vec![ComplementConstructor::DroppedOpData {
                op: name.as_str().into(),
            }]
        }
        _ => Vec::new(),
    }
}

fn datum_value(datum: &rd::Datum) -> Value {
    match datum {
        rd::Datum::Null => Value::Null,
        rd::Datum::Bool(b) => Value::Bool(*b),
        rd::Datum::Int(i) => Value::Int(*i),
        rd::Datum::Float(bits) => Value::Float(f64::from_bits(*bits)),
        rd::Datum::Str(s) => Value::Str(s.clone()),
        rd::Datum::List(items) => Value::List(items.iter().map(datum_value).collect()),
        rd::Datum::Record(fields) => Value::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), datum_value(v)))
                .collect(),
        ),
    }
}

/// Whether a production elementary transform is the reference's.
fn same_elementary(production: &TheoryTransform, reference: &rd::Elementary) -> bool {
    use TheoryTransform as T;
    use rd::Elementary as E;
    match (production, reference) {
        (T::AddSort { sort, vertex_kind }, E::AddSort { sort: s, kind, .. }) => {
            *sort.name == **s
                && sort.params.is_empty()
                && vertex_kind.as_deref() == Some(kind.as_str())
        }
        (
            T::AddEdge {
                src_sort,
                tgt_sort,
                edge_name,
                edge_kind,
            },
            E::AddEdge {
                src,
                tgt,
                name: n,
                kind,
            },
        ) => {
            **src_sort == **src
                && **tgt_sort == **tgt
                && edge_name.as_deref() == Some(n.as_str())
                && **edge_kind == **kind
        }
        (
            T::DropEdge {
                src_sort,
                tgt_sort,
                edge_name,
                edge_kind: None,
            },
            E::DropEdge { src, tgt, name: n },
        ) => **src_sort == **src && **tgt_sort == **tgt && edge_name.as_deref() == n.as_deref(),
        (T::DropSort(a), E::DropSort(b))
        | (T::DropOp(a), E::DropOp(b))
        | (T::DropEquation(a), E::DropEquation(b)) => **a == **b,
        (T::RenameSort { old, new }, E::RenameSort { old: o, new: n })
        | (T::RenameOp { old, new }, E::RenameOp { old: o, new: n }) => {
            **old == **o && **new == **n
        }
        (
            T::AddOp(op),
            E::AddOp {
                name: n,
                src,
                tgt,
                kind,
            },
        ) => *op == panproto_gat::Operation::unary(name(n), name(kind), name(src), name(tgt)),
        (
            T::RenameEdgeName {
                src_sort,
                tgt_sort,
                old_name,
                new_name,
            },
            E::RenameEdgeName { src, tgt, old, new },
        ) => {
            **src_sort == **src && **tgt_sort == **tgt && **old_name == **old && **new_name == **new
        }
        (T::AddEquation(eq), E::AddEquation { name: n, lhs, rhs }) => {
            *eq.name == **n && eq.lhs == term(lhs) && eq.rhs == term(rhs)
        }
        _ => same_valued_elementary(production, reference),
    }
}

/// Whether a production pullback, coercion, merge or directed equation is
/// the reference's.
fn same_valued_elementary(production: &TheoryTransform, reference: &rd::Elementary) -> bool {
    use TheoryTransform as T;
    use rd::Elementary as E;
    match (production, reference) {
        (
            T::Pullback(m),
            E::Pullback {
                name: n,
                domain,
                codomain,
                sort_map,
                op_map,
            },
        ) => {
            *m.name == **n
                && *m.domain == **domain
                && *m.codomain == **codomain
                && m.sort_map.len() == sort_map.len()
                && sort_map
                    .iter()
                    .all(|(k, v)| m.sort_map.get(k.as_str()).map(|x| &**x) == Some(v.as_str()))
                && m.op_map.len() == op_map.len()
                && op_map.iter().all(|(k, v)| {
                    m.op_map
                        .get(k.as_str())
                        .and_then(panproto_gat::OpAssignment::as_op)
                        .map(|x| &**x)
                        == Some(v.as_str())
                })
        }
        (
            T::CoerceSort {
                sort_name,
                target_kind,
                coercion_expr,
                inverse_expr,
                coercion_class,
            },
            E::CoerceSort {
                sort,
                target_kind: k,
                expr: e,
                inverse,
                class: c,
            },
        ) => {
            **sort_name == **sort
                && *target_kind == value_kind(*k)
                && parse_expr(e).as_ref() == Some(coercion_expr)
                && inverse_expr.as_ref() == inverse.as_deref().and_then(parse_expr).as_ref()
                && *coercion_class == class(*c)
        }
        (
            T::MergeSorts {
                sort_a,
                sort_b,
                merged_name,
                merger_expr,
            },
            E::MergeSorts {
                sort_a: a,
                sort_b: b,
                merged,
                expr: e,
            },
        ) => {
            **sort_a == **a
                && **sort_b == **b
                && **merged_name == **merged
                && parse_expr(e).as_ref() == Some(merger_expr)
        }
        (
            T::AddDirectedEquation(deq),
            E::DirectedEq {
                name: n,
                lhs,
                rhs,
                implementation,
                inverse,
                source_kind,
                target_kind,
                class: c,
            },
        ) => {
            *deq.name == **n
                && deq.lhs == term(lhs)
                && deq.rhs == term(rhs)
                && parse_expr(implementation).as_ref() == Some(&deq.impl_term)
                && deq.inverse.as_ref() == inverse.as_deref().and_then(parse_expr).as_ref()
                && deq.source_kind == source_kind.map(value_kind)
                && deq.target_kind == target_kind.map(value_kind)
                && deq.coercion_class == class(*c)
        }
        _ => false,
    }
}

/// Compare production leaves and their complement atoms with the
/// reference's flattened protolenses.
fn same_leaves(
    production: &[Leaf<'_>],
    complement: &[&ComplementConstructor],
    reference: &[Flat<'_>],
) -> Result<(), String> {
    if production.len() != reference.len() {
        return Err(format!(
            "{} leaves, the reference has {}",
            production.len(),
            reference.len()
        ));
    }
    let mut atoms_left = complement;
    for (leaf, flat) in production.iter().zip(reference) {
        match (leaf, flat) {
            (Leaf::Transform(t), Flat::Elementary(e)) => {
                if !same_elementary(t, e) {
                    return Err(format!("{t:?} is not {e:?}"));
                }
                let expected = residue(e);
                let Some((taken, rest)) = atoms_left.split_at_checked(expected.len()) else {
                    return Err(format!("{e:?} keeps residue the complement lacks"));
                };
                if !taken
                    .iter()
                    .zip(&expected)
                    .all(|(t, e)| same_complement(t, e))
                {
                    return Err(format!("{e:?} keeps {taken:?}, the reference {expected:?}"));
                }
                atoms_left = rest;
            }
            (Leaf::Scoped(focus, inner), Flat::Scoped(f, inside)) => {
                if focus != f {
                    return Err(format!("scoped at {focus}, the reference at {f}"));
                }
                let Some((
                    ComplementConstructor::Scoped {
                        focus: cf,
                        inner: inner_complement,
                    },
                    rest,
                )) = atoms_left.split_first()
                else {
                    return Err("a scoped protolens keeps no scoped residue".to_owned());
                };
                if **cf != **focus {
                    return Err(format!("scoped residue at {cf}, protolens at {focus}"));
                }
                let mut inner_atoms = Vec::new();
                atoms(inner_complement, &mut inner_atoms);
                same_leaves(inner, &inner_atoms, inside)?;
                atoms_left = rest;
            }
            _ => {
                return Err(
                    "a scoped leaf where the reference has none, or none where it has one"
                        .to_owned(),
                );
            }
        }
    }
    if atoms_left.is_empty() {
        Ok(())
    } else {
        Err(format!("residue left over: {atoms_left:?}"))
    }
}

fn same_proto(production: &Protolens, reference: &rd::Proto) -> Result<(), String> {
    if !matches!(production.source.transform, TheoryTransform::Identity) {
        return Err(format!(
            "source {:?} is not the identity",
            production.source.transform
        ));
    }
    let mut prod = Vec::new();
    leaves(&production.target.transform, &mut prod);
    let mut complement = Vec::new();
    atoms(&production.complement_constructor, &mut complement);
    let mut flat = Vec::new();
    flatten(reference, &mut flat);
    same_leaves(&prod, &complement, &flat)
}

fn same_chain(production: &[Protolens], reference: &[rd::Proto]) -> Result<(), String> {
    if production.len() != reference.len() {
        return Err(format!(
            "chain of {} protolenses, the reference {}",
            production.len(),
            reference.len()
        ));
    }
    production
        .iter()
        .zip(reference)
        .try_for_each(|(p, r)| same_proto(p, r))
}

fn same_transforms(
    production: &HashMap<panproto_gat::Name, Vec<FieldTransform>>,
    reference: &rd::Transforms,
) -> Result<(), String> {
    let agree = production.len() == reference.len()
        && reference.iter().all(|(anchor, expected)| {
            production.get(anchor.as_str()).is_some_and(|found| {
                found.len() == expected.len()
                    && found
                        .iter()
                        .zip(expected)
                        .all(|(p, r)| same_transform(p, r))
            })
        });
    if agree {
        Ok(())
    } else {
        Err(format!(
            "transforms {production:?}, the reference {reference:?}"
        ))
    }
}

/// Whether production's compiled lens is the reference's.
fn same_lens(production: &CompiledLens, reference: &rd::CompiledLens) -> Result<(), String> {
    if production.stages.len() != reference.stages.len() {
        return Err(format!(
            "{} stages, the reference has {}",
            production.stages.len(),
            reference.stages.len()
        ));
    }
    for (i, (p, r)) in production.stages.iter().zip(&reference.stages).enumerate() {
        same_chain(&p.chain.steps, &r.chain).map_err(|e| format!("stage {i}: {e}"))?;
        same_transforms(&p.field_transforms, &r.transforms)
            .map_err(|e| format!("stage {i}: {e}"))?;
    }
    let reference_chain = match &reference.symmetric {
        Some((left, _)) => left.clone(),
        None => rd::chain_of(&reference.stages),
    };
    same_chain(&production.chain.steps, &reference_chain)
        .map_err(|e| format!("flat chain: {e}"))?;
    let reference_transforms = if reference.symmetric.is_some() {
        rd::Transforms::new()
    } else {
        rd::transforms_of(&reference.stages)
    };
    same_transforms(&production.field_transforms, &reference_transforms)
        .map_err(|e| format!("flat transforms: {e}"))?;
    match (&production.symmetric, &reference.symmetric) {
        (None, None) => Ok(()),
        (Some(legs), Some((left, right))) => {
            same_chain(&legs.left.steps, left).map_err(|e| format!("left leg: {e}"))?;
            same_chain(&legs.right.steps, right).map_err(|e| format!("right leg: {e}"))
        }
        _ => Err("one side is symmetric and the other is not".to_owned()),
    }
}

// ─── Agreement on documents ─────────────────────────────────────────────

fn production_library() -> HashMap<String, LensDocument> {
    scope::LIBRARY
        .iter()
        .filter_map(|(_, text)| read_source(text, SourceFormat::Json).ok())
        .map(|doc| (doc.id.clone(), doc))
        .collect()
}

const fn span(error: &LensDslError) -> Option<(usize, usize)> {
    match error {
        LensDslError::Spanned { span, .. } => Some((span.offset(), span.len())),
        _ => None,
    }
}

/// How production compiles a document of a scope.
#[derive(Clone, Copy)]
enum Mode {
    /// `compile_source`, every refusal located in the text.
    Source,
    /// `read_source` then `compile_with_refs` over the library; a compile
    /// refusal's span is the one `panproto_dsl_eval` locates.
    WithRefs,
}

const fn no_lenses(_id: &str) -> Option<CompiledLens> {
    None
}

/// Production's compilation of a document and the byte range of its
/// refusal.
type Production = (Result<CompiledLens, LensDslError>, Option<(usize, usize)>);

fn production(text: &str, mode: Mode, library: &HashMap<String, LensDocument>) -> Production {
    let result = match mode {
        Mode::Source => compile_source(text, SourceFormat::Json, scope::BODY, &no_lenses),
        Mode::WithRefs => read_source(text, SourceFormat::Json)
            .and_then(|doc| compile_with_refs(&doc, scope::BODY, library)),
    };
    let located = match (&result, mode) {
        (Err(error), Mode::WithRefs) if span(error).is_none() => error
            .path()
            .and_then(|p| panproto_lens_dsl::path::locate_json(text, p)),
        (Err(error), _) => span(error),
        (Ok(_), _) => None,
    };
    (result, located)
}

/// Whether `compile_source` refuses the JSON text `text` read as YAML, which
/// JSON text also is, at `at` with the reference's refusal, its span the
/// first character of the value the reference locates there.
fn yaml_refusal_agrees(text: &str, at: &Path, refusal: &rd::Refusal) -> bool {
    let expected = locate(text, at).map(|(offset, _)| {
        let length = text[offset..].chars().next().map_or(0, char::len_utf8);
        (offset, length)
    });
    compile_source(text, SourceFormat::Yaml, scope::BODY, &no_lenses)
        .err()
        .is_some_and(|error| {
            error.path().map(path).as_ref() == Some(at)
                && same_refusal(error.unlocated(), refusal)
                && span(&error) == expected
        })
}

/// The disagreement between production and the reference on one JSON
/// document, if any; documents outside the reference's domain are counted.
/// A document compiled from source and refused is also read as YAML.
fn disagreement(
    text: &str,
    mode: Mode,
    library: &HashMap<String, LensDocument>,
    outside: &mut usize,
) -> Option<String> {
    let Some(value) = panproto_reference::json::parse(text) else {
        return Some(format!("the reference cannot read {text}"));
    };
    let reference = rd::compile_document(&value, &context(&scope::library));
    let (result, located) = production(text, mode, library);
    match (reference, result) {
        (rd::Outcome::OutsideDomain(_), _) => {
            *outside += 1;
            None
        }
        (rd::Outcome::Accepted(expected), Ok(compiled)) => same_lens(&compiled, &expected)
            .err()
            .map(|why| format!("{text}\n  {why}")),
        (rd::Outcome::Refused(at, refusal), Err(error)) => {
            let expected_span = locate(text, &at);
            let agrees = error.path().map(path).as_ref() == Some(&at)
                && same_refusal(error.unlocated(), &refusal)
                && located == expected_span
                && (matches!(mode, Mode::WithRefs) || yaml_refusal_agrees(text, &at, &refusal));
            (!agrees).then(|| {
                format!(
                    "{text}\n  reference {refusal:?} at {at:?} span {expected_span:?}\n  production {error:?} span {located:?}"
                )
            })
        }
        (reference, production) => Some(format!(
            "{text}\n  reference {reference:?}\n  production {:?}",
            production.map(|c| c.stages.len())
        )),
    }
}

fn assert_agreement(documents: &[String], mode: Mode, expected_count: usize) {
    assert_eq!(documents.len(), expected_count, "the scope's size");
    let library = production_library();
    let mut outside = 0;
    let found: Vec<String> = documents
        .iter()
        .filter_map(|text| disagreement(text, mode, &library, &mut outside))
        .collect();
    assert!(
        found.is_empty(),
        "{} of {} documents disagree; first:\n{}",
        found.len(),
        expected_count,
        found.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(outside, 0, "no document of the scope is outside the domain");
}

#[test]
fn step_lists_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        &scope::step_documents(),
        Mode::Source,
        scope::sequence_count(scope::STEPS.len(), 2)
            * scope::INVERTIBLE.len()
            * scope::DIRECTED.len(),
    );
}

#[test]
fn rules_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        &scope::rule_documents(),
        Mode::Source,
        scope::sequence_count(scope::RULES.len(), 2)
            * scope::PASSTHROUGH.len()
            * scope::INVERTIBLE.len(),
    );
}

#[test]
fn compositions_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        &scope::compose_documents(),
        Mode::WithRefs,
        scope::sequence_count(scope::PARTS.len(), 3) * scope::MODES.len() * scope::INVERTIBLE.len(),
    );
}

#[test]
fn symmetric_bodies_compile_as_the_reference_on_every_bounded_document() {
    assert_agreement(
        &scope::symmetric_documents(),
        Mode::Source,
        scope::LEGS.len().pow(2) * scope::FOCUS.len() * scope::INVERTIBLE.len(),
    );
}

/// The grammar check of one mutated document: production's reading of the
/// text against the reference's grammar, refusal, path and span.
fn grammar_disagreement(text: &str) -> Option<String> {
    let Some(value) = panproto_reference::json::parse(text) else {
        return Some(format!("the reference cannot read {text}"));
    };
    match (
        rd::check_document(&value),
        read_source(text, SourceFormat::Json),
    ) {
        (Ok(_), Ok(_)) => None,
        (Err((at, problem)), Err(error)) => {
            let refusal = rd::Refusal::Shape(problem.clone());
            let agrees = error.path().map(path).as_ref() == Some(&at)
                && same_refusal(error.unlocated(), &refusal)
                && span(&error) == locate(text, &at)
                && yaml_refusal_agrees(text, &at, &refusal);
            (!agrees).then(|| {
                format!("{text}\n  reference {problem:?} at {at:?}\n  production {error:?}")
            })
        }
        (reference, production) => Some(format!(
            "{text}\n  reference {reference:?}\n  production {:?}",
            production.map(|d| d.id)
        )),
    }
}

#[test]
fn the_grammar_refuses_as_the_reference_on_every_mutated_document() {
    let (replaced, removed, added) = scope::shape_documents();
    assert_eq!(replaced.len(), SHAPE_VALUES * scope::REPLACEMENTS.len());
    assert_eq!(removed.len(), SHAPE_MEMBERS);
    assert_eq!(added.len(), SHAPE_OBJECTS);
    let found: Vec<String> = replaced
        .iter()
        .chain(&removed)
        .chain(&added)
        .filter_map(|text| grammar_disagreement(text))
        .collect();
    assert!(
        found.is_empty(),
        "{} documents disagree; first:\n{}",
        found.len(),
        found.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// The values of the six grammar bases, counted by hand: 232 values, of
/// which 187 are object members and 78 are objects.
const SHAPE_VALUES: usize = 232;
/// The object members of the six grammar bases.
const SHAPE_MEMBERS: usize = 187;
/// The objects of the six grammar bases.
const SHAPE_OBJECTS: usize = 78;

// ─── Composition laws ───────────────────────────────────────────────────

/// The step lists of at most one step of the alphabet.
fn short_lists() -> Vec<Vec<&'static str>> {
    std::iter::once(Vec::new())
        .chain(scope::STEPS.iter().map(|step| vec![*step]))
        .collect()
}

fn list_text(steps: &[&str]) -> String {
    format!("[{}]", steps.join(","))
}

fn compiled(text: &str, library: &HashMap<String, LensDocument>) -> Option<CompiledLens> {
    read_source(text, SourceFormat::Json)
        .ok()
        .and_then(|doc| compile_with_refs(&doc, scope::BODY, library).ok())
}

fn steps_text(steps: &[&str]) -> String {
    scope::document(&format!(r#""steps":{}"#, list_text(steps)))
}

fn inline(steps: &[&str]) -> String {
    format!(r#"{{"inline":{{"steps":{}}}}}"#, list_text(steps))
}

fn reference_to(id: &str) -> String {
    format!(r#"{{"ref":"{id}"}}"#)
}

fn compose_text(mode: &str, parts: &[String]) -> String {
    scope::document(&format!(
        r#""compose":{{"mode":"{mode}","lenses":[{}]}}"#,
        parts.join(",")
    ))
}

/// The library with a composition of `parts` added under `id`.
fn with_composition(
    library: &HashMap<String, LensDocument>,
    id: &str,
    mode: &str,
    parts: &[String],
) -> HashMap<String, LensDocument> {
    let text = compose_text(mode, parts).replacen(
        &format!(r#""id":"{}""#, scope::TOP),
        &format!(r#""id":"{id}""#),
        1,
    );
    let Ok(doc) = read_source(&text, SourceFormat::Json) else {
        panic!("{text} reads");
    };
    let mut library = library.clone();
    library.insert(id.to_owned(), doc);
    library
}

/// The observable structure of a compiled lens: each stage's protolenses
/// as JSON, and its transforms by anchor.
/// One stage's protolenses as JSON and its transforms by anchor.
type StageStructure = (Vec<String>, Vec<(String, String)>);

fn structure(lens: &CompiledLens) -> Vec<StageStructure> {
    lens.stages
        .iter()
        .map(|stage| {
            let chain = stage
                .chain
                .steps
                .iter()
                .map(|p| p.to_json().unwrap_or_default())
                .collect();
            let mut transforms: Vec<(String, String)> = stage
                .field_transforms
                .iter()
                .map(|(anchor, entries)| (anchor.to_string(), format!("{entries:?}")))
                .collect();
            transforms.sort();
            (chain, transforms)
        })
        .collect()
}

#[test]
fn vertical_composition_is_concatenation_on_every_pair_of_short_step_lists() {
    let library = production_library();
    let lists = short_lists();
    let mut checked = 0;
    for a in &lists {
        for b in &lists {
            let joined: Vec<&str> = a.iter().chain(b).copied().collect();
            let whole = compiled(&steps_text(&joined), &library);
            let parts = compiled(&compose_text("vertical", &[inline(a), inline(b)]), &library);
            let (Some(left), Some(right)) = (
                compiled(&steps_text(a), &library),
                compiled(&steps_text(b), &library),
            ) else {
                assert!(
                    whole.is_none() && parts.is_none(),
                    "{a:?} ; {b:?} compiles though a part is refused"
                );
                continue;
            };
            let (Some(whole), Some(parts)) = (whole, parts) else {
                panic!("{a:?} ; {b:?} is refused though both parts compile");
            };
            let mut concatenated = structure(&left);
            concatenated.extend(structure(&right));
            assert_eq!(structure(&whole), concatenated, "compile({a:?} ++ {b:?})");
            assert_eq!(structure(&parts), concatenated, "compose [{a:?}, {b:?}]");
            checked += 1;
        }
    }
    assert_eq!(checked, ACCEPTED_SHORT_LISTS.pow(2));
}

/// The step lists of at most one step that compile: the empty list and the
/// 25 steps of the alphabet that the semantics page accepts (the other 11
/// carry a malformed expression, term or default, or a dishonest class).
const ACCEPTED_SHORT_LISTS: usize = 26;

#[test]
fn vertical_composition_is_associative_with_the_empty_composition_as_identity() {
    let base = production_library();
    let empty = compiled(&compose_text("vertical", &[]), &base);
    assert!(
        empty.is_some_and(|lens| lens.stages.is_empty()),
        "the empty composition is the empty lens"
    );
    let lists = short_lists();
    let mut checked = 0;
    for a in &lists {
        for b in &lists {
            let with_ab = with_composition(&base, "ab", "vertical", &[inline(a), inline(b)]);
            for c in &lists {
                let library = with_composition(&with_ab, "bc", "vertical", &[inline(b), inline(c)]);
                let left = compiled(
                    &compose_text("vertical", &[reference_to("ab"), inline(c)]),
                    &library,
                );
                let right = compiled(
                    &compose_text("vertical", &[inline(a), reference_to("bc")]),
                    &library,
                );
                let flat = compiled(
                    &compose_text("vertical", &[inline(a), inline(b), inline(c)]),
                    &library,
                );
                let padded = compiled(
                    &compose_text(
                        "vertical",
                        &[
                            inline(&[]),
                            inline(a),
                            inline(&[]),
                            inline(b),
                            inline(c),
                            inline(&[]),
                        ],
                    ),
                    &library,
                );
                match (left, right, flat, padded) {
                    (Some(l), Some(r), Some(f), Some(p)) => {
                        assert_eq!(structure(&l), structure(&r), "({a:?} ; {b:?}) ; {c:?}");
                        assert_eq!(structure(&l), structure(&f), "({a:?} ; {b:?}) ; {c:?}");
                        assert_eq!(
                            structure(&p),
                            structure(&f),
                            "empty parts around {a:?} ; {b:?} ; {c:?}"
                        );
                        checked += 1;
                    }
                    (None, None, None, None) => {}
                    other => panic!(
                        "{a:?} ; {b:?} ; {c:?} compiles on some bracketing only: {:?}",
                        (
                            other.0.is_some(),
                            other.1.is_some(),
                            other.2.is_some(),
                            other.3.is_some()
                        )
                    ),
                }
            }
        }
    }
    assert_eq!(checked, ACCEPTED_SHORT_LISTS.pow(3));
}

/// The horizontal composite's observable structure: its stage chains'
/// elementary transforms in order, with the residue each keeps.
fn horizontal_structure(lens: &CompiledLens) -> Vec<String> {
    lens.stages
        .iter()
        .flat_map(|stage| stage.chain.steps.iter())
        .map(|p| {
            let mut out = Vec::new();
            leaves(&p.target.transform, &mut out);
            let mut complement = Vec::new();
            atoms(&p.complement_constructor, &mut complement);
            format!(
                "{:?} {complement:?} {:?}",
                out.iter().map(leaf_text).collect::<Vec<_>>(),
                p.source.transform
            )
        })
        .collect()
}

fn leaf_text(leaf: &Leaf<'_>) -> String {
    match leaf {
        Leaf::Transform(t) => format!("{t:?}"),
        Leaf::Scoped(focus, inner) => format!(
            "scoped {focus} {:?}",
            inner.iter().map(leaf_text).collect::<Vec<_>>()
        ),
    }
}

#[test]
fn horizontal_composition_is_associative_up_to_its_elementary_structure() {
    let base = production_library();
    let lists: Vec<Vec<&str>> = short_lists().into_iter().skip(1).collect();
    let mut checked = 0;
    for a in &lists {
        for b in &lists {
            let with_ab = with_composition(&base, "ab", "horizontal", &[inline(a), inline(b)]);
            for c in &lists {
                let library =
                    with_composition(&with_ab, "bc", "horizontal", &[inline(b), inline(c)]);
                let left = compiled(
                    &compose_text("horizontal", &[reference_to("ab"), inline(c)]),
                    &library,
                );
                let right = compiled(
                    &compose_text("horizontal", &[inline(a), reference_to("bc")]),
                    &library,
                );
                let flat = compiled(
                    &compose_text("horizontal", &[inline(a), inline(b), inline(c)]),
                    &library,
                );
                match (left, right, flat) {
                    (Some(l), Some(r), Some(f)) => {
                        assert_eq!(
                            horizontal_structure(&l),
                            horizontal_structure(&r),
                            "({a:?} * {b:?}) * {c:?}"
                        );
                        assert_eq!(
                            horizontal_structure(&l),
                            horizontal_structure(&f),
                            "({a:?} * {b:?}) * {c:?}"
                        );
                        checked += 1;
                    }
                    (None, None, None) => {}
                    other => panic!(
                        "{a:?} * {b:?} * {c:?} compiles on some bracketing only: {:?}",
                        (other.0.is_some(), other.1.is_some(), other.2.is_some())
                    ),
                }
            }
        }
    }
    assert_eq!(checked, ACCEPTED_CHAIN_STEPS.pow(3));
}

/// The steps of the alphabet that compile to a non-empty chain: the 25
/// accepted steps but `apply_expr`, `compute_field` and the `scoped` step
/// with no inner step.
const ACCEPTED_CHAIN_STEPS: usize = 22;

// ─── Longer documents ───────────────────────────────────────────────────

fn longer_steps() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(prop::sample::select(scope::STEPS), 0..9),
        prop::sample::select(scope::INVERTIBLE),
        prop::sample::select(scope::DIRECTED),
    )
        .prop_map(|(steps, invertible, directed)| {
            scope::document(&format!(
                r#""steps":[{}]{invertible}{directed}"#,
                steps.join(",")
            ))
        })
}

fn longer_compositions() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(prop::sample::select(scope::PARTS), 0..6),
        prop::sample::select(scope::MODES),
    )
        .prop_map(|(parts, mode)| {
            scope::document(&format!(
                r#""compose":{{"mode":"{mode}","lenses":[{}]}}"#,
                parts.join(",")
            ))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn longer_step_lists_compile_as_the_reference(text in longer_steps()) {
        let mut outside = 0;
        let found = disagreement(&text, Mode::Source, &HashMap::new(), &mut outside);
        prop_assert!(found.is_none(), "{}", found.unwrap_or_default());
        prop_assert_eq!(outside, 0);
    }

    #[test]
    fn longer_compositions_compile_as_the_reference(text in longer_compositions()) {
        let mut outside = 0;
        let found = disagreement(&text, Mode::WithRefs, &production_library(), &mut outside);
        prop_assert!(found.is_none(), "{}", found.unwrap_or_default());
        prop_assert_eq!(outside, 0);
    }
}
