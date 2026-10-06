//! `check_morphism` and the whole action on terms against a reference
//! semantics.
//!
//! The reference is written from the definitions, not from the code: `m^`
//! renames an operation sent to an operation and replaces one sent to a
//! derived term by that term with the mapped arguments substituted for
//! its inputs; `F(S(t_1, ..., t_n)) = sort_map(S)(m^(t_1), ..., m^(t_n))`;
//! two sort expressions agree when their name-free forms are equal (each
//! variable replaced by the binder it resolves to, the nearest of its
//! name); a constructor `c`'s image is `k` iff `m^(c(x_1, ..., x_n)) =
//! k(x_1, ..., x_n)`. A map is a morphism iff every domain sort is mapped
//! with its arity, kind, parameter sorts (under `F`) and closure (by
//! constructor images) preserved, and every domain operation is mapped to
//! an operation with the same signature under `F` or to a term of sort
//! `F(output)` in the context of its inputs at sorts `F(input)`. The scope
//! has no equations, whose check this repair does not change. The
//! reference calls none of `check_morphism`,
//! `apply_to_term_substituting`, `SortExpr::alpha_eq`, `SortExpr::subst`,
//! `Term::substitute` or the signature comparisons.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use proptest::prelude::*;

use panproto_gat::{
    CaseBranch, OpAssignment, Operation, Sort, SortClosure, SortExpr, SortParam, Term, Theory,
    TheoryMorphism, check_morphism,
};

// ─── reference semantics ───────────────────────────────────────────

/// A term with each variable replaced by the binder it resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Nf {
    Sig(usize),
    Loc(usize),
    Free(String),
    App(String, Vec<Self>),
    Hole(Option<String>),
    Let(Box<Self>, Box<Self>),
    Case(Box<Self>, Vec<(String, usize, Self)>),
}

fn last_position(names: &[String], x: &str) -> Option<usize> {
    names.iter().rposition(|n| n == x)
}

fn nf_term(t: &Term, sig: &[String], local: &mut Vec<String>) -> Nf {
    match t {
        Term::Var(x) => last_position(local, x).map_or_else(
            || last_position(sig, x).map_or_else(|| Nf::Free(x.to_string()), Nf::Sig),
            Nf::Loc,
        ),
        Term::App { op, args } => Nf::App(
            op.to_string(),
            args.iter().map(|a| nf_term(a, sig, local)).collect(),
        ),
        Term::Hole { name } => Nf::Hole(name.as_ref().map(ToString::to_string)),
        Term::Let { name, bound, body } => {
            let bound = nf_term(bound, sig, local);
            local.push(name.to_string());
            let body = nf_term(body, sig, local);
            local.pop();
            Nf::Let(Box::new(bound), Box::new(body))
        }
        Term::Case {
            scrutinee,
            branches,
        } => {
            let scrutinee = nf_term(scrutinee, sig, local);
            let branches = branches
                .iter()
                .map(|b| {
                    let depth = local.len();
                    local.extend(b.binders.iter().map(ToString::to_string));
                    let body = nf_term(&b.body, sig, local);
                    local.truncate(depth);
                    (b.constructor.to_string(), b.binders.len(), body)
                })
                .collect();
            Nf::Case(Box::new(scrutinee), branches)
        }
    }
}

/// A sort expression as head and argument list (both spellings of a bare
/// name give an empty list).
type RSort = (Arc<str>, Vec<Term>);

fn rsort(s: &SortExpr) -> RSort {
    (Arc::clone(s.head()), s.args().to_vec())
}

fn nf_sort(s: &RSort, sig: &[String]) -> (String, Vec<Nf>) {
    (
        s.0.to_string(),
        s.1.iter()
            .map(|t| nf_term(t, sig, &mut Vec::new()))
            .collect(),
    )
}

/// Substitution without binders: every term the scope substitutes into
/// is binder free (asserted). The last binding of a name wins.
fn subst(t: &Term, map: &[(Arc<str>, Term)]) -> Term {
    match t {
        Term::Var(x) => map
            .iter()
            .rev()
            .find(|(n, _)| n == x)
            .map_or_else(|| t.clone(), |(_, v)| v.clone()),
        Term::App { op, args } => Term::App {
            op: Arc::clone(op),
            args: args.iter().map(|a| subst(a, map)).collect(),
        },
        Term::Hole { .. } => t.clone(),
        Term::Let { .. } | Term::Case { .. } => {
            panic!("the reference substitutes only into binder-free terms: {t}")
        }
    }
}

fn subst_sort(s: &RSort, map: &[(Arc<str>, Term)]) -> RSort {
    (
        Arc::clone(&s.0),
        s.1.iter().map(|t| subst(t, map)).collect(),
    )
}

fn find_op<'a>(theory: &'a Theory, name: &str) -> Option<&'a Operation> {
    theory.ops.iter().find(|o| &*o.name == name)
}

fn find_sort<'a>(theory: &'a Theory, name: &str) -> Option<&'a Sort> {
    theory.sorts.iter().find(|s| &*s.name == name)
}

/// `m^`, the whole action on terms.
fn ref_apply(m: &TheoryMorphism, t: &Term, domain: &Theory) -> Term {
    match t {
        Term::Var(_) | Term::Hole { .. } => t.clone(),
        Term::App { op, args } => {
            let args: Vec<Term> = args.iter().map(|a| ref_apply(m, a, domain)).collect();
            match m.op_map.get(op) {
                None => Term::App {
                    op: Arc::clone(op),
                    args,
                },
                Some(OpAssignment::Op(k)) => Term::App {
                    op: Arc::clone(k),
                    args,
                },
                Some(OpAssignment::Term(image)) => match find_op(domain, op) {
                    Some(o) => {
                        let map: Vec<(Arc<str>, Term)> = o
                            .inputs
                            .iter()
                            .map(|(n, _, _)| Arc::clone(n))
                            .zip(args)
                            .collect();
                        subst(image, &map)
                    }
                    None => Term::App {
                        op: Arc::clone(op),
                        args,
                    },
                },
            }
        }
        Term::Let { name, bound, body } => Term::Let {
            name: Arc::clone(name),
            bound: Box::new(ref_apply(m, bound, domain)),
            body: Box::new(ref_apply(m, body, domain)),
        },
        Term::Case {
            scrutinee,
            branches,
        } => Term::Case {
            scrutinee: Box::new(ref_apply(m, scrutinee, domain)),
            branches: branches
                .iter()
                .map(|b| CaseBranch {
                    constructor: ref_constructor_image(m, &b.constructor, domain)
                        .unwrap_or_else(|| Arc::clone(&b.constructor)),
                    binders: b.binders.clone(),
                    body: ref_apply(m, &b.body, domain),
                })
                .collect(),
        },
    }
}

/// The constructor image of `c`, if it has one.
fn ref_constructor_image(m: &TheoryMorphism, c: &Arc<str>, domain: &Theory) -> Option<Arc<str>> {
    let Some(op) = find_op(domain, c) else {
        return match m.op_map.get(c) {
            Some(OpAssignment::Op(k)) => Some(Arc::clone(k)),
            Some(OpAssignment::Term(_)) => None,
            None => Some(Arc::clone(c)),
        };
    };
    let vars: Vec<Term> = op
        .inputs
        .iter()
        .map(|(n, _, _)| Term::Var(Arc::clone(n)))
        .collect();
    match ref_apply(
        m,
        &Term::App {
            op: Arc::clone(c),
            args: vars.clone(),
        },
        domain,
    ) {
        Term::App { op: k, args } if args == vars => Some(k),
        _ => None,
    }
}

/// `F` on sort expressions.
fn ref_f(m: &TheoryMorphism, s: &SortExpr, domain: &Theory) -> RSort {
    let head = m
        .sort_map
        .get(s.head())
        .cloned()
        .unwrap_or_else(|| Arc::clone(s.head()));
    (
        head,
        s.args().iter().map(|t| ref_apply(m, t, domain)).collect(),
    )
}

/// The sort of a binder-free term in `ctx` (last binding wins), with
/// dependent output sorts instantiated by the arguments.
fn ref_type(t: &Term, ctx: &[(Arc<str>, RSort)], codomain: &Theory) -> Option<RSort> {
    match t {
        Term::Var(x) => ctx
            .iter()
            .rev()
            .find(|(n, _)| n == x)
            .map(|(_, s)| s.clone()),
        Term::App { op, args } => {
            let o = find_op(codomain, op)?;
            if o.inputs.len() != args.len() {
                return None;
            }
            let mut bound: Vec<(Arc<str>, Term)> = Vec::new();
            for ((name, sort, _), arg) in o.inputs.iter().zip(args) {
                if ref_type(arg, ctx, codomain)? != subst_sort(&rsort(sort), &bound) {
                    return None;
                }
                bound.push((Arc::clone(name), arg.clone()));
            }
            Some(subst_sort(&rsort(&o.output), &bound))
        }
        Term::Hole { .. } | Term::Let { .. } | Term::Case { .. } => {
            panic!("the scope assigns only binder-free terms: {t}")
        }
    }
}

fn names_of<'a>(names: impl Iterator<Item = &'a Arc<str>>) -> Vec<String> {
    names.map(ToString::to_string).collect()
}

/// Whether the reference semantics accepts `m : domain -> codomain`.
fn ref_accepts(m: &TheoryMorphism, domain: &Theory, codomain: &Theory) -> bool {
    assert!(domain.eqs.is_empty() && domain.directed_eqs.is_empty());
    for sort in &domain.sorts {
        let Some(target) = m.sort_map.get(&sort.name) else {
            return false;
        };
        let Some(target) = find_sort(codomain, target) else {
            return false;
        };
        if sort.params.len() != target.params.len() || sort.kind != target.kind {
            return false;
        }
        let dn = names_of(sort.params.iter().map(|p| &p.name));
        let tn = names_of(target.params.iter().map(|p| &p.name));
        for (i, (p, q)) in sort.params.iter().zip(&target.params).enumerate() {
            if nf_sort(&ref_f(m, &p.sort, domain), &dn[..i]) != nf_sort(&rsort(&q.sort), &tn[..i]) {
                return false;
            }
        }
        if let SortClosure::Closed(ctors) = &sort.closure {
            let mut images = std::collections::BTreeSet::new();
            for c in ctors {
                let Some(k) = ref_constructor_image(m, c, domain) else {
                    return false;
                };
                images.insert(k);
            }
            let wanted: std::collections::BTreeSet<Arc<str>> = match &target.closure {
                SortClosure::Closed(cs) => cs.iter().cloned().collect(),
                SortClosure::Open => std::collections::BTreeSet::new(),
            };
            if images != wanted {
                return false;
            }
        }
    }
    for op in &domain.ops {
        let mapped = |s: &SortExpr| m.sort_map.contains_key(s.head());
        if !op.inputs.iter().all(|(_, s, _)| mapped(s)) || !mapped(&op.output) {
            return false;
        }
        match m.op_map.get(&op.name) {
            None => return false,
            Some(OpAssignment::Op(k)) => {
                let Some(target) = find_op(codomain, k) else {
                    return false;
                };
                if op.inputs.len() != target.inputs.len() {
                    return false;
                }
                let dn = names_of(op.inputs.iter().map(|(n, _, _)| n));
                let tn = names_of(target.inputs.iter().map(|(n, _, _)| n));
                for (i, ((_, s, _), (_, t, _))) in op.inputs.iter().zip(&target.inputs).enumerate()
                {
                    if nf_sort(&ref_f(m, s, domain), &dn[..i]) != nf_sort(&rsort(t), &tn[..i]) {
                        return false;
                    }
                }
                if nf_sort(&ref_f(m, &op.output, domain), &dn)
                    != nf_sort(&rsort(&target.output), &tn)
                {
                    return false;
                }
            }
            Some(OpAssignment::Term(t)) => {
                let ctx: Vec<(Arc<str>, RSort)> = op
                    .inputs
                    .iter()
                    .map(|(n, s, _)| (Arc::clone(n), ref_f(m, s, domain)))
                    .collect();
                if ref_type(t, &ctx, codomain) != Some(ref_f(m, &op.output, domain)) {
                    return false;
                }
            }
        }
    }
    true
}

// ─── the exhaustive scope ──────────────────────────────────────────

fn x() -> SortExpr {
    SortExpr::from("X")
}

fn sort_app(head: &str, args: Vec<Term>) -> SortExpr {
    SortExpr::app(head, args)
}

fn c(name: &str) -> Term {
    Term::constant(name)
}

fn v(name: &str) -> Term {
    Term::var(name)
}

/// `X`, `Y(x : X)`, `H(x : X, y : X)`, the closed `B` over `closure`, and
/// `Z(w : Y(z_arg))`.
fn scope_sorts(closure: &[&str], z_arg: &str) -> Vec<Sort> {
    vec![
        Sort::simple("X"),
        Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        Sort::dependent(
            "H",
            vec![SortParam::new("x", "X"), SortParam::new("y", "X")],
        ),
        Sort::closed("B", vec![], closure.iter().copied()),
        Sort::dependent(
            "Z",
            vec![SortParam::new("w", sort_app("Y", vec![c(z_arg)]))],
        ),
    ]
}

/// The domain: constants `c, d : X`, constructors `t, f : B`,
/// `p : Y(p_arg)`, `r(z : X) : r_out` and `q(u : X, v : X) : H(u, v)`.
fn domain(p_arg: &str, r_out: &SortExpr) -> Theory {
    Theory::new(
        "D",
        scope_sorts(&["t", "f"], "c"),
        vec![
            Operation::new("c", vec![], "X"),
            Operation::new("d", vec![], "X"),
            Operation::new("t", vec![], "B"),
            Operation::new("f", vec![], "B"),
            Operation::new("p", vec![], sort_app("Y", vec![c(p_arg)])),
            Operation::new("r", vec![("z".into(), x())], r_out.clone()),
            Operation::new(
                "q",
                vec![("u".into(), x()), ("v".into(), x())],
                sort_app("H", vec![v("u"), v("v")]),
            ),
        ],
        vec![],
    )
}

/// The outputs `r` takes in the domain family.
fn r_outputs() -> Vec<SortExpr> {
    vec![
        sort_app("H", vec![v("z"), c("c")]),
        sort_app("H", vec![c("c"), v("z")]),
        sort_app("H", vec![v("z"), v("z")]),
    ]
}

/// The codomain, with `B` closed over `closure` and `Z(w : Y(z_arg))`:
/// `c, d : X`, `t, f : B`, `p : Y(c)`, `p2 : Y(d)`,
/// `r(z : X) : H(z, c)`, `r2(w : X) : H(w, d)`,
/// `q(z : X, z : X) : H(z, z)`, `q2(a : X, b : X) : H(a, b)` and
/// `neg(b : B) : B`.
fn codomain(closure: &[&str], z_arg: &str) -> Theory {
    Theory::new(
        "C",
        scope_sorts(closure, z_arg),
        vec![
            Operation::new("c", vec![], "X"),
            Operation::new("d", vec![], "X"),
            Operation::new("t", vec![], "B"),
            Operation::new("f", vec![], "B"),
            Operation::new("p", vec![], sort_app("Y", vec![c("c")])),
            Operation::new("p2", vec![], sort_app("Y", vec![c("d")])),
            Operation::new(
                "r",
                vec![("z".into(), x())],
                sort_app("H", vec![v("z"), c("c")]),
            ),
            Operation::new(
                "r2",
                vec![("w".into(), x())],
                sort_app("H", vec![v("w"), c("d")]),
            ),
            Operation::new(
                "q",
                vec![("z".into(), x()), ("z".into(), x())],
                sort_app("H", vec![v("z"), v("z")]),
            ),
            Operation::new(
                "q2",
                vec![("a".into(), x()), ("b".into(), x())],
                sort_app("H", vec![v("a"), v("b")]),
            ),
            Operation::new("neg", vec![("b".into(), "B".into())], "B"),
        ],
        vec![],
    )
}

fn op(name: &str) -> OpAssignment {
    OpAssignment::op(name)
}

const fn term(t: Term) -> OpAssignment {
    OpAssignment::Term(t)
}

/// The candidate images of each domain operation: renames and derived
/// terms built from constants, constructors and the operation's inputs.
fn candidate_images() -> Vec<(&'static str, Vec<OpAssignment>)> {
    vec![
        ("c", vec![op("c"), op("d"), term(c("c")), term(c("d"))]),
        ("d", vec![op("d"), op("c"), term(c("d"))]),
        (
            "t",
            vec![
                op("t"),
                op("f"),
                term(c("t")),
                term(c("f")),
                term(Term::app("neg", vec![c("f")])),
            ],
        ),
        ("f", vec![op("f"), op("t"), term(c("f"))]),
        ("p", vec![op("p"), op("p2")]),
        (
            "r",
            vec![
                op("r"),
                op("r2"),
                term(Term::app("r", vec![v("z")])),
                term(Term::app("r2", vec![v("z")])),
                term(Term::app("r", vec![c("c")])),
            ],
        ),
        (
            "q",
            vec![
                op("q"),
                op("q2"),
                term(Term::app("q2", vec![v("u"), v("v")])),
                term(Term::app("q2", vec![v("v"), v("u")])),
            ],
        ),
    ]
}

fn leg(images: &[(&'static str, OpAssignment)]) -> TheoryMorphism {
    TheoryMorphism::new(
        "m",
        "D",
        "C",
        ["X", "Y", "H", "B", "Z"]
            .into_iter()
            .map(|s| (Arc::from(s), Arc::from(s)))
            .collect(),
        images
            .iter()
            .map(|(o, a)| (Arc::from(*o), a.clone()))
            .collect::<HashMap<Arc<str>, OpAssignment>>(),
    )
}

/// Every leg: one candidate image per domain operation.
fn every_leg() -> Vec<TheoryMorphism> {
    let mut legs: Vec<Vec<(&'static str, OpAssignment)>> = vec![vec![]];
    for (name, images) in candidate_images() {
        legs = legs
            .into_iter()
            .flat_map(|prefix| {
                images.iter().map(move |image| {
                    let mut next = prefix.clone();
                    next.push((name, image.clone()));
                    next
                })
            })
            .collect();
    }
    legs.iter().map(|images| leg(images)).collect()
}

/// Domain terms on which the whole action is compared: a case analysis
/// over `B`, uses of `r` and `q`, and a `let`.
fn probe_terms() -> Vec<Term> {
    vec![
        Term::Case {
            scrutinee: Box::new(v("b")),
            branches: vec![
                CaseBranch {
                    constructor: "t".into(),
                    binders: vec![],
                    body: c("f"),
                },
                CaseBranch {
                    constructor: "f".into(),
                    binders: vec![],
                    body: Term::app("r", vec![c("c")]),
                },
            ],
        },
        Term::app("q", vec![c("d"), Term::app("r", vec![c("c")])]),
        Term::Let {
            name: "n".into(),
            bound: Box::new(c("c")),
            body: Box::new(Term::app("q", vec![v("n"), c("d")])),
        },
    ]
}

/// Compare `check_morphism` and the whole action with the reference on
/// one instance; returns whether the reference accepts.
fn agree(m: &TheoryMorphism, dom: &Theory, cod: &Theory) -> Result<bool, String> {
    let expected = ref_accepts(m, dom, cod);
    let observed = check_morphism(m, dom, cod);
    if observed.is_ok() != expected {
        return Err(format!(
            "reference accepts: {expected}, check_morphism: {observed:?}, leg {:?}, domain {:?}, codomain {:?}",
            m.op_map, dom.ops, cod.sorts
        ));
    }
    for t in probe_terms() {
        let (engine, reference) = (m.apply_to_term_substituting(&t, dom), ref_apply(m, &t, dom));
        if engine != reference {
            return Err(format!(
                "whole action on {t}: engine {engine}, reference {reference}, leg {:?}",
                m.op_map
            ));
        }
    }
    Ok(expected)
}

#[test]
fn check_morphism_agrees_with_the_reference_semantics_on_the_exhaustive_scope() {
    let legs = every_leg();
    let mut instances = 0_usize;
    let mut accepted = 0_usize;
    let mut accepted_with_a_term_image = 0_usize;
    for p_arg in ["c", "d"] {
        for r_out in r_outputs() {
            let dom = domain(p_arg, &r_out);
            for closure in [&["t", "f"][..], &["t"][..], &["f"][..]] {
                for z_arg in ["c", "d"] {
                    let cod = codomain(closure, z_arg);
                    for m in &legs {
                        instances += 1;
                        if agree(m, &dom, &cod).unwrap() {
                            accepted += 1;
                            if m.op_map.values().any(|a| !a.is_op()) {
                                accepted_with_a_term_image += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "morphism scope: {} legs x 6 domains x 6 codomains = {instances} instances; accepted {accepted} ({accepted_with_a_term_image} with a derived-term image), refused {}",
        legs.len(),
        instances - accepted
    );
    assert_eq!(instances, legs.len() * 36);
    assert!(accepted_with_a_term_image > 0 && accepted < instances);
}

// ─── the property twin ─────────────────────────────────────────────

/// A binder-free term over the codomain's operations and the given
/// variables, of depth at most two.
fn gen_image(vars: &'static [&'static str]) -> impl Strategy<Value = Term> {
    let mut leaves: Vec<Term> = ["c", "d", "t", "f", "p", "p2"]
        .iter()
        .map(|n| c(n))
        .collect();
    leaves.extend(vars.iter().map(|n| v(n)));
    let leaf = proptest::sample::select(leaves);
    leaf.prop_recursive(2, 8, 2, |inner| {
        (
            proptest::sample::select(vec!["r", "r2", "q", "q2", "c", "neg"]),
            proptest::collection::vec(inner, 0..=2),
        )
            .prop_map(|(op, args)| Term::app(op, args))
    })
}

fn gen_assignment(
    renames: Vec<&'static str>,
    vars: &'static [&'static str],
) -> impl Strategy<Value = OpAssignment> {
    prop_oneof![
        proptest::sample::select(renames).prop_map(op),
        gen_image(vars).prop_map(term),
    ]
}

const ALL_OPS: [&str; 11] = ["c", "d", "t", "f", "p", "p2", "r", "r2", "q", "q2", "neg"];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn check_morphism_agrees_with_the_reference_semantics(
        p_arg in proptest::sample::select(vec!["c", "d"]),
        r_out in proptest::sample::select(r_outputs()),
        closure in proptest::sample::select(vec![vec!["t", "f"], vec!["t"], vec!["f", "t"], vec![]]),
        z_arg in proptest::sample::select(vec!["c", "d"]),
        images in (
            gen_assignment(ALL_OPS.to_vec(), &[]),
            gen_assignment(ALL_OPS.to_vec(), &[]),
            gen_assignment(ALL_OPS.to_vec(), &[]),
            gen_assignment(ALL_OPS.to_vec(), &[]),
            gen_assignment(ALL_OPS.to_vec(), &[]),
            gen_assignment(ALL_OPS.to_vec(), &["z"]),
            gen_assignment(ALL_OPS.to_vec(), &["u", "v"]),
        ),
    ) {
        let (ic, id, it, i_f, ip, ir, iq) = images;
        let m = leg(&[("c", ic), ("d", id), ("t", it), ("f", i_f), ("p", ip), ("r", ir), ("q", iq)]);
        let dom = domain(p_arg, &r_out);
        let cod = codomain(&closure, z_arg);
        let verdict = agree(&m, &dom, &cod);
        prop_assert!(verdict.is_ok(), "{}", verdict.unwrap_err());
    }
}
