//! Generalized algebraic theories: signature type checking and the colimit
//! of a diagram of theories with explicit identification.
//!
//! Domain: sorts with parameters, operations with dependent inputs and
//! outputs, and equations over variables and applications; theories with
//! no directed equations, policies or case terms.

use std::collections::{BTreeMap, BTreeSet};

/// A name.
pub type Name = String;

/// A term: a variable or an operation applied to terms.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Term {
    /// A variable.
    Var(Name),
    /// An application.
    App(Name, Vec<Term>),
}

/// A sort expression: a sort name (for a dependent sort, the unapplied
/// form) or a sort applied to argument terms.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SortExpr {
    /// A sort name.
    Name(Name),
    /// A sort applied to terms.
    App(Name, Vec<Term>),
}

impl SortExpr {
    /// The sort named.
    #[must_use]
    pub fn head(&self) -> &str {
        match self {
            Self::Name(n) | Self::App(n, _) => n,
        }
    }
}

/// A sort declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sort {
    /// Name.
    pub name: Name,
    /// Parameters `(name, sort)` in telescope order.
    pub params: Vec<(Name, SortExpr)>,
    /// Kind (compared, not interpreted).
    pub kind: Name,
    /// Closure (compared, not interpreted).
    pub closed: bool,
}

/// An operation declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Op {
    /// Name.
    pub name: Name,
    /// Inputs `(name, sort)` in telescope order.
    pub inputs: Vec<(Name, SortExpr)>,
    /// Output sort.
    pub output: SortExpr,
}

/// An equation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Equation {
    /// Name.
    pub name: Name,
    /// Left side.
    pub lhs: Term,
    /// Right side.
    pub rhs: Term,
}

/// A theory.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Theory {
    /// Name.
    pub name: Name,
    /// Sorts.
    pub sorts: Vec<Sort>,
    /// Operations.
    pub ops: Vec<Op>,
    /// Equations.
    pub eqs: Vec<Equation>,
}

/// Where a signature defect sits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Site {
    /// The `index`-th parameter of a sort.
    SortParam(Name, usize),
    /// The `index`-th input of an operation.
    OpInput(Name, usize),
    /// The output of an operation.
    OpOutput(Name),
}

/// A signature defect.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Defect {
    /// A sort name not declared (or not declared earlier, for a parameter).
    UndeclaredSort(Site, Name),
    /// An application with the wrong number of arguments.
    SortArity(Site, Name, usize, usize),
    /// An argument whose sort is not the parameter's: `(site, sort, index,
    /// expected head, found head)`.
    ParamIllTyped(Site, Name, usize, Name, Name),
    /// A variable bound by no earlier binder.
    UnboundParamVar(Site, Name),
    /// Two sorts or two operations with one name: `(is sort, name)`.
    DuplicateSymbol(bool, Name),
    /// Two binders of one telescope with one name.
    DuplicateBinder(Site, Name),
}

/// The sort of a term in a context, by its head: a variable's binder sort,
/// an application's output with the operation's inputs substituted.
fn term_sort(t: &Theory, ctx: &[(Name, SortExpr)], term: &Term) -> Option<SortExpr> {
    match term {
        Term::Var(x) => ctx.iter().rev().find(|(n, _)| n == x).map(|(_, s)| s.clone()),
        Term::App(o, args) => {
            let op = t.ops.iter().find(|op| op.name == *o)?;
            (op.inputs.len() == args.len()).then(|| subst_sort(&op.output, &op.inputs, args))
        }
    }
}

fn subst_term(term: &Term, binders: &[(Name, SortExpr)], args: &[Term]) -> Term {
    match term {
        Term::Var(x) => binders.iter().rposition(|(n, _)| n == x).and_then(|i| args.get(i).cloned()).unwrap_or_else(|| term.clone()),
        Term::App(o, xs) => Term::App(o.clone(), xs.iter().map(|a| subst_term(a, binders, args)).collect()),
    }
}

fn subst_sort(s: &SortExpr, binders: &[(Name, SortExpr)], args: &[Term]) -> SortExpr {
    match s {
        SortExpr::Name(n) => SortExpr::Name(n.clone()),
        SortExpr::App(n, xs) => SortExpr::App(n.clone(), xs.iter().map(|a| subst_term(a, binders, args)).collect()),
    }
}

/// Whether a term's variables are all bound in the context.
fn unbound(ctx: &[(Name, SortExpr)], term: &Term) -> Option<Name> {
    match term {
        Term::Var(x) => (!ctx.iter().any(|(n, _)| n == x)).then(|| x.clone()),
        Term::App(_, xs) => xs.iter().find_map(|a| unbound(ctx, a)),
    }
}

/// Check one sort expression in a context: the sort is declared (among
/// `declared`), an application has the declared arity, every argument's
/// variables are bound and its sort's head is the parameter's.
fn check_expr(t: &Theory, declared: &[&Sort], ctx: &[(Name, SortExpr)], site: &Site, s: &SortExpr, out: &mut Vec<Defect>) {
    let Some(sort) = declared.iter().find(|d| d.name == s.head()) else {
        out.push(Defect::UndeclaredSort(site.clone(), s.head().to_owned()));
        return;
    };
    let SortExpr::App(_, args) = s else { return };
    if args.len() != sort.params.len() {
        out.push(Defect::SortArity(site.clone(), sort.name.clone(), sort.params.len(), args.len()));
        return;
    }
    for (i, a) in args.iter().enumerate() {
        if let Some(x) = unbound(ctx, a) {
            out.push(Defect::UnboundParamVar(site.clone(), x));
            continue;
        }
        let expected = subst_sort(&sort.params[i].1, &sort.params[..i], &args[..i]);
        if let Some(found) = term_sort(t, ctx, a)
            && found.head() != expected.head()
        {
            out.push(Defect::ParamIllTyped(site.clone(), sort.name.clone(), i, expected.head().to_owned(), found.head().to_owned()));
        }
    }
}

fn duplicates(names: &[&Name]) -> Vec<Name> {
    let mut seen = BTreeSet::new();
    names.iter().filter(|n| !seen.insert((**n).clone())).map(|n| (*n).clone()).collect()
}

/// Signature well-formedness: every sort parameter's sort is declared
/// earlier; every sort expression of an operation names a declared sort,
/// an application with the declared arity whose argument terms are typed
/// against the parameters (with substitution) and whose variables are bound
/// by earlier inputs; binder names and symbol names are distinct.
///
/// # Errors
///
/// Every defect, sorted.
pub fn typecheck_signature(t: &Theory) -> Result<(), Vec<Defect>> {
    let mut out = Vec::new();
    for n in duplicates(&t.sorts.iter().map(|s| &s.name).collect::<Vec<_>>()) {
        out.push(Defect::DuplicateSymbol(true, n));
    }
    for n in duplicates(&t.ops.iter().map(|o| &o.name).collect::<Vec<_>>()) {
        out.push(Defect::DuplicateSymbol(false, n));
    }
    for (k, s) in t.sorts.iter().enumerate() {
        let earlier: Vec<&Sort> = t.sorts[..k].iter().collect();
        for (i, (p, ps)) in s.params.iter().enumerate() {
            let site = Site::SortParam(s.name.clone(), i);
            if s.params[..i].iter().any(|(q, _)| q == p) {
                out.push(Defect::DuplicateBinder(site.clone(), p.clone()));
            }
            check_expr(t, &earlier, &s.params[..i], &site, ps, &mut out);
        }
    }
    let all: Vec<&Sort> = t.sorts.iter().collect();
    for o in &t.ops {
        for (i, (x, xs)) in o.inputs.iter().enumerate() {
            let site = Site::OpInput(o.name.clone(), i);
            if o.inputs[..i].iter().any(|(q, _)| q == x) {
                out.push(Defect::DuplicateBinder(site.clone(), x.clone()));
            }
            check_expr(t, &all, &o.inputs[..i], &site, xs, &mut out);
        }
        check_expr(t, &all, &o.inputs, &Site::OpOutput(o.name.clone()), &o.output, &mut out);
    }
    out.sort();
    if out.is_empty() { Ok(()) } else { Err(out) }
}

/// An operation's image under a morphism.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpImage {
    /// An operation.
    Op(Name),
    /// A term over the operation's input names.
    Term(Term),
}

/// A theory morphism: total maps on sorts and operations.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Morphism {
    /// Sort map.
    pub sorts: BTreeMap<Name, Name>,
    /// Operation map.
    pub ops: BTreeMap<Name, OpImage>,
}

/// A symbol of a diagram node.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymRef {
    /// Node label.
    pub node: Name,
    /// Symbol name.
    pub name: Name,
}

/// A symbol kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymKind {
    /// A sort.
    Sort,
    /// An operation.
    Op,
    /// An equation.
    Eq,
}

/// How symbols of different nodes are identified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Legs and same names identify.
    Amalgamate,
    /// Only legs identify.
    ExplicitIdentity,
}

/// What a name clash does in explicit-identity mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClashPolicy {
    /// Rename every clashing class.
    Freshen,
    /// Refuse.
    Reject,
}

/// A composition refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposeError {
    /// Two nodes share a label.
    DuplicateNodeLabel(Name),
    /// A label is empty or holds `.` or `#`.
    InvalidNodeLabel(Name),
    /// An edge names no node.
    UnknownNodeLabel(Name),
    /// A node's signature is ill-formed.
    InvalidNode(Name, Vec<Defect>),
    /// A leg is not a morphism: its edge index.
    InvalidLeg(usize),
    /// A sort class disagrees.
    SortSignatureMismatch(SymRef, SymRef),
    /// An operation class disagrees.
    OpSignatureMismatch(SymRef, SymRef),
    /// A class formed by the name rule disagrees (amalgamation).
    NameConflict(SymKind, Name, SymRef, SymRef),
    /// Two classes of one kind get one name (rejecting policy).
    NameClash(SymKind, Name, Vec<Vec<SymRef>>),
}

/// The colimit: the theory and where every source symbol went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    /// The result theory.
    pub theory: Theory,
    /// Every source sort, operation and equation to its result name.
    pub provenance: BTreeMap<(SymKind, SymRef), Name>,
}

/// A name-free form: every variable replaced by the index of its binder in
/// the telescope (`None` for a variable bound nowhere).
fn name_free_term(binders: &[Name], t: &Term, class: &dyn Fn(&str) -> Name) -> Term {
    match t {
        Term::Var(x) => Term::Var(binders.iter().rposition(|b| b == x).map_or_else(|| format!("free:{x}"), |i| format!("#{i}"))),
        Term::App(o, xs) => Term::App(class(o), xs.iter().map(|a| name_free_term(binders, a, class)).collect()),
    }
}

fn name_free_sort(binders: &[Name], s: &SortExpr, sort_class: &dyn Fn(&str) -> Name, op_class: &dyn Fn(&str) -> Name) -> SortExpr {
    match s {
        SortExpr::Name(n) => SortExpr::Name(sort_class(n)),
        SortExpr::App(n, xs) => SortExpr::App(sort_class(n), xs.iter().map(|a| name_free_term(binders, a, op_class)).collect()),
    }
}

fn op_signature(o: &Op, sort_class: &dyn Fn(&str) -> Name, op_class: &dyn Fn(&str) -> Name) -> (Vec<SortExpr>, SortExpr) {
    let names: Vec<Name> = o.inputs.iter().map(|(n, _)| n.clone()).collect();
    let ins = o.inputs.iter().enumerate().map(|(i, (_, s))| name_free_sort(&names[..i], s, sort_class, op_class)).collect();
    (ins, name_free_sort(&names, &o.output, sort_class, op_class))
}

fn sort_signature(s: &Sort, sort_class: &dyn Fn(&str) -> Name, op_class: &dyn Fn(&str) -> Name) -> (usize, Name, bool, Vec<SortExpr>) {
    let names: Vec<Name> = s.params.iter().map(|(n, _)| n.clone()).collect();
    let params = s.params.iter().enumerate().map(|(i, (_, x))| name_free_sort(&names[..i], x, sort_class, op_class)).collect();
    (s.params.len(), s.kind.clone(), s.closed, params)
}

/// Whether a leg is a morphism: every source sort maps to a declared target
/// sort of the same arity, and every source operation to a target
/// operation whose signature is the translated source signature (name-free)
/// or to a term whose variables are the operation's inputs.
fn is_morphism(src: &Theory, tgt: &Theory, m: &Morphism) -> bool {
    let sort_img = |n: &str| m.sorts.get(n).cloned().unwrap_or_else(|| format!("unmapped:{n}"));
    let sorts_ok = src.sorts.iter().all(|s| m.sorts.get(&s.name).is_some_and(|t| tgt.sorts.iter().any(|ts| ts.name == *t && ts.params.len() == s.params.len())));
    let op_img = |n: &str| match m.ops.get(n) {
        Some(OpImage::Op(o)) => o.clone(),
        _ => format!("term:{n}"),
    };
    let ops_ok = src.ops.iter().all(|o| match m.ops.get(&o.name) {
        Some(OpImage::Op(o2)) => tgt.ops.iter().find(|t| t.name == *o2).is_some_and(|t| op_signature(o, &sort_img, &op_img) == op_signature(t, &|x| x.to_owned(), &|x| x.to_owned())),
        Some(OpImage::Term(t)) => unbound(&o.inputs, t).is_none(),
        None => false,
    });
    sorts_ok && ops_ok
}

/// Naive classes: each member's class label; joining relabels.
struct Classes {
    label: BTreeMap<(SymKind, SymRef), usize>,
}

impl Classes {
    fn join(&mut self, a: &(SymKind, SymRef), b: &(SymKind, SymRef)) {
        let (Some(la), Some(lb)) = (self.label.get(a).copied(), self.label.get(b).copied()) else { return };
        for v in self.label.values_mut() {
            if *v == lb {
                *v = la;
            }
        }
    }

    fn members(&self, kind: SymKind) -> Vec<Vec<SymRef>> {
        let mut by: BTreeMap<usize, Vec<SymRef>> = BTreeMap::new();
        for ((k, r), l) in &self.label {
            if *k == kind {
                by.entry(*l).or_default().push(r.clone());
            }
        }
        let mut out: Vec<Vec<SymRef>> = by.into_values().collect();
        out.sort();
        out
    }
}

/// The candidate name of a class: the member name occurring most often,
/// ties to the least name by bytes.
fn majority(members: &[SymRef]) -> Name {
    let mut count: BTreeMap<&Name, usize> = BTreeMap::new();
    for m in members {
        *count.entry(&m.name).or_default() += 1;
    }
    let best = count.values().copied().max().unwrap_or(0);
    count.into_iter().find(|(_, c)| *c == best).map(|(n, _)| n.clone()).unwrap_or_default()
}

/// Name every class of one kind: its candidate; under explicit identity, a
/// candidate shared by several classes is a clash, refused or freshened as
/// `{least member label}.{candidate}`, any remaining collision suffixed
/// `#1`, `#2`, ... in ascending class order.
fn name_classes(kind: SymKind, classes: &[Vec<SymRef>], mode: Mode, clash: ClashPolicy) -> Result<Vec<Name>, ComposeError> {
    let cands: Vec<Name> = classes.iter().map(|c| majority(c)).collect();
    let mut names = cands.clone();
    if mode == Mode::ExplicitIdentity {
        for (i, c) in cands.iter().enumerate() {
            let sharing: Vec<usize> = (0..cands.len()).filter(|j| cands[*j] == *c).collect();
            if sharing.len() > 1 {
                if clash == ClashPolicy::Reject {
                    return Err(ComposeError::NameClash(kind, c.clone(), sharing.iter().map(|j| classes[*j].clone()).collect()));
                }
                let label = classes[i].iter().map(|m| m.node.clone()).min().unwrap_or_default();
                names[i] = format!("{label}.{c}");
            }
        }
        let mut seen: BTreeMap<Name, usize> = BTreeMap::new();
        for n in &mut names {
            let k = seen.entry(n.clone()).or_insert(0);
            if *k > 0 {
                *n = format!("{n}#{k}");
            }
            *k += 1;
        }
    }
    Ok(names)
}

/// The colimit of a diagram of theories.
///
/// Steps: check labels, edges, node signatures and legs; identify each
/// leg's source sort and operation with its image (and, when amalgamating,
/// same-kind symbols of one name); check that each class's members agree
/// (sort arity, kind, closure and parameter sorts; operation signatures,
/// name-free and translated to classes); name the classes; translate each
/// class's least member and every equation, adding for each derived-term
/// leg the equation `class(o)(inputs) = image` named
/// `{source label}.{o}~{edge index}`.
///
/// # Errors
///
/// The first [`ComposeError`]; `Ok(None)` for a diagram outside the
/// reference's domain (an ill-typed node whose defects are only undeclared
/// sorts).
pub fn compose(nodes: &[(Name, Theory)], edges: &[(Name, Name, Morphism)], mode: Mode, clash: ClashPolicy, result_name: &str) -> Result<Option<Composition>, ComposeError> {
    let mut labels = BTreeSet::new();
    for (l, t) in nodes {
        if l.is_empty() || l.contains('.') || l.contains('#') {
            return Err(ComposeError::InvalidNodeLabel(l.clone()));
        }
        if !labels.insert(l.clone()) {
            return Err(ComposeError::DuplicateNodeLabel(l.clone()));
        }
        if let Err(d) = typecheck_signature(t) {
            if d.iter().all(|x| matches!(x, Defect::UndeclaredSort(..))) {
                return Ok(None);
            }
            return Err(ComposeError::InvalidNode(l.clone(), d));
        }
    }
    let theory = |l: &str| nodes.iter().find(|(x, _)| x == l).map(|(_, t)| t);
    for (i, (s, t, m)) in edges.iter().enumerate() {
        for l in [s, t] {
            if theory(l).is_none() {
                return Err(ComposeError::UnknownNodeLabel(l.clone()));
            }
        }
        if !is_morphism(theory(s).unwrap_or(&Theory::default()), theory(t).unwrap_or(&Theory::default()), m) {
            return Err(ComposeError::InvalidLeg(i));
        }
    }
    let sym = |k: SymKind, n: &str, x: &str| (k, SymRef { node: n.to_owned(), name: x.to_owned() });
    let mut cl = Classes { label: BTreeMap::new() };
    for (l, t) in nodes {
        for s in &t.sorts {
            let n = cl.label.len();
            cl.label.insert(sym(SymKind::Sort, l, &s.name), n);
        }
        for o in &t.ops {
            let n = cl.label.len();
            cl.label.insert(sym(SymKind::Op, l, &o.name), n);
        }
    }
    let leg_only = |cl: &mut Classes| {
        for (s, t, m) in edges {
            for (a, b) in &m.sorts {
                cl.join(&sym(SymKind::Sort, s, a), &sym(SymKind::Sort, t, b));
            }
            for (a, img) in &m.ops {
                if let OpImage::Op(b) = img {
                    cl.join(&sym(SymKind::Op, s, a), &sym(SymKind::Op, t, b));
                }
            }
        }
    };
    leg_only(&mut cl);
    let legs = Classes { label: cl.label.clone() };
    if mode == Mode::Amalgamate {
        let keys: Vec<(SymKind, SymRef)> = cl.label.keys().cloned().collect();
        for a in &keys {
            for b in keys.iter().filter(|b| b.0 == a.0 && b.1.name == a.1.name && b.1.node != a.1.node) {
                cl.join(a, b);
            }
        }
    }
    finish(nodes, edges, &cl, &legs, mode, clash, result_name).map(Some)
}

fn rename_term(t: &Term, op: &dyn Fn(&str) -> Name) -> Term {
    match t {
        Term::Var(x) => Term::Var(x.clone()),
        Term::App(o, xs) => Term::App(op(o), xs.iter().map(|a| rename_term(a, op)).collect()),
    }
}

fn rename_sort(s: &SortExpr, sort: &dyn Fn(&str) -> Name, op: &dyn Fn(&str) -> Name) -> SortExpr {
    match s {
        SortExpr::Name(n) => SortExpr::Name(sort(n)),
        SortExpr::App(n, xs) => SortExpr::App(sort(n), xs.iter().map(|a| rename_term(a, op)).collect()),
    }
}

/// An equation with its variables renamed by order of first occurrence.
fn alpha(lhs: &Term, rhs: &Term) -> (Term, Term) {
    fn vars(t: &Term, out: &mut Vec<Name>) {
        match t {
            Term::Var(x) => {
                if !out.contains(x) {
                    out.push(x.clone());
                }
            }
            Term::App(_, xs) => xs.iter().for_each(|a| vars(a, out)),
        }
    }
    let mut order = Vec::new();
    vars(lhs, &mut order);
    vars(rhs, &mut order);
    let re = |t: &Term| name_free_term(&order, t, &|o| o.to_owned());
    (re(lhs), re(rhs))
}

fn finish(
    nodes: &[(Name, Theory)],
    edges: &[(Name, Name, Morphism)],
    cl: &Classes,
    legs: &Classes,
    mode: Mode,
    clash: ClashPolicy,
    result_name: &str,
) -> Result<Composition, ComposeError> {
    let theory = |l: &str| nodes.iter().find(|(x, _)| x == l).map(|(_, t)| t.clone()).unwrap_or_default();
    let class_id = |k: SymKind, node: &str, name: &str| {
        cl.label.get(&(k, SymRef { node: node.to_owned(), name: name.to_owned() })).map_or_else(|| format!("?{name}"), |l| format!("c{l}"))
    };
    for kind in [SymKind::Sort, SymKind::Op] {
        for members in cl.members(kind) {
            let sig = |m: &SymRef| {
                let t = theory(&m.node);
                let (sc, oc) = (|n: &str| class_id(SymKind::Sort, &m.node, n), |n: &str| class_id(SymKind::Op, &m.node, n));
                match kind {
                    SymKind::Sort => t.sorts.iter().find(|s| s.name == m.name).map(|s| format!("{:?}", sort_signature(s, &sc, &oc))),
                    _ => t.ops.iter().find(|o| o.name == m.name).map(|o| format!("{:?}", op_signature(o, &sc, &oc))),
                }
            };
            let first = &members[0];
            if let Some(second) = members.iter().find(|m| sig(m) != sig(first)) {
                let same_leg = legs.label.get(&(kind, first.clone())) == legs.label.get(&(kind, second.clone()));
                return Err(if mode == Mode::Amalgamate && !same_leg {
                    ComposeError::NameConflict(kind, first.name.clone(), first.clone(), second.clone())
                } else if kind == SymKind::Sort {
                    ComposeError::SortSignatureMismatch(first.clone(), second.clone())
                } else {
                    ComposeError::OpSignatureMismatch(first.clone(), second.clone())
                });
            }
        }
    }
    let mut provenance = BTreeMap::new();
    let mut result_of: BTreeMap<(SymKind, String), Name> = BTreeMap::new();
    for kind in [SymKind::Sort, SymKind::Op] {
        let classes = cl.members(kind);
        let names = name_classes(kind, &classes, mode, clash)?;
        for (c, n) in classes.iter().zip(names) {
            for m in c {
                provenance.insert((kind, m.clone()), n.clone());
                result_of.insert((kind, class_id(kind, &m.node, &m.name)), n.clone());
            }
        }
    }
    let res = |k: SymKind, node: &str, n: &str| result_of.get(&(k, class_id(k, node, n))).cloned().unwrap_or_else(|| n.to_owned());
    let mut out = Theory { name: result_name.to_owned(), ..Theory::default() };
    for c in cl.members(SymKind::Sort) {
        let m = &c[0];
        if let Some(s) = theory(&m.node).sorts.iter().find(|s| s.name == m.name) {
            let params = s.params.iter().map(|(p, x)| (p.clone(), rename_sort(x, &|n| res(SymKind::Sort, &m.node, n), &|n| res(SymKind::Op, &m.node, n)))).collect();
            out.sorts.push(Sort { name: res(SymKind::Sort, &m.node, &m.name), params, kind: s.kind.clone(), closed: s.closed });
        }
    }
    for c in cl.members(SymKind::Op) {
        let m = &c[0];
        if let Some(o) = theory(&m.node).ops.iter().find(|o| o.name == m.name) {
            let (sr, or) = (|n: &str| res(SymKind::Sort, &m.node, n), |n: &str| res(SymKind::Op, &m.node, n));
            let inputs = o.inputs.iter().map(|(p, x)| (p.clone(), rename_sort(x, &sr, &or))).collect();
            out.ops.push(Op { name: or(&m.name), inputs, output: rename_sort(&o.output, &sr, &or) });
        }
    }
    let mut eqs: Vec<(SymRef, Name, Term, Term)> = Vec::new();
    for (l, t) in nodes {
        for e in &t.eqs {
            let op = |n: &str| res(SymKind::Op, l, n);
            eqs.push((SymRef { node: l.clone(), name: e.name.clone() }, e.name.clone(), rename_term(&e.lhs, &op), rename_term(&e.rhs, &op)));
        }
    }
    let mut ordered: Vec<(usize, &(Name, Name, Morphism))> = edges.iter().enumerate().collect();
    ordered.sort_by(|a, b| (&a.1.0, &a.1.1, format!("{:?}", a.1.2)).cmp(&(&b.1.0, &b.1.1, format!("{:?}", b.1.2))));
    for (rank, (_, (s, t, m))) in ordered.iter().enumerate() {
        for (o, img) in &m.ops {
            let OpImage::Term(term) = img else { continue };
            let Some(op) = theory(s).ops.into_iter().find(|x| x.name == *o) else { continue };
            let lhs = Term::App(res(SymKind::Op, s, o), op.inputs.iter().map(|(x, _)| Term::Var(x.clone())).collect());
            let rhs = rename_term(term, &|n| res(SymKind::Op, t, n));
            let name = format!("{s}.{o}~{rank}");
            eqs.push((SymRef { node: s.clone(), name: name.clone() }, name, lhs, rhs));
        }
    }
    out.eqs = equations(&eqs, mode, clash, &mut provenance)?;
    out.sorts.sort_by(|a, b| a.name.cmp(&b.name));
    out.ops.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Composition { theory: out, provenance })
}

/// The result equations: explicit identity merges alpha-equivalent
/// equations into one class each; amalgamation merges only same-named
/// alpha-equivalent ones and refuses same-named different ones. Classes are
/// named as symbols are.
fn equations(
    eqs: &[(SymRef, Name, Term, Term)],
    mode: Mode,
    clash: ClashPolicy,
    provenance: &mut BTreeMap<(SymKind, SymRef), Name>,
) -> Result<Vec<Equation>, ComposeError> {
    let mut classes: Vec<(Vec<SymRef>, (Term, Term), Term, Term)> = Vec::new();
    for (r, name, l, rr) in eqs {
        let key = alpha(l, rr);
        let found = classes.iter_mut().find(|c| c.1 == key && (mode == Mode::ExplicitIdentity || c.0.iter().any(|m| m.name == *name)));
        match found {
            Some(c) => c.0.push(r.clone()),
            None => {
                if mode == Mode::Amalgamate
                    && let Some(c) = classes.iter().find(|c| c.0.iter().any(|m| m.name == *name))
                {
                    return Err(ComposeError::NameConflict(SymKind::Eq, name.clone(), c.0[0].clone(), r.clone()));
                }
                classes.push((vec![r.clone()], key, l.clone(), rr.clone()));
            }
        }
    }
    for c in &mut classes {
        c.0.sort();
    }
    classes.sort_by(|a, b| a.0.cmp(&b.0));
    let members: Vec<Vec<SymRef>> = classes.iter().map(|c| c.0.clone()).collect();
    let names = name_classes(SymKind::Eq, &members, mode, clash)?;
    let mut out = Vec::new();
    for (c, n) in classes.into_iter().zip(names) {
        for m in &c.0 {
            provenance.insert((SymKind::Eq, m.clone()), n.clone());
        }
        out.push(Equation { name: n, lhs: c.2, rhs: c.3 });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
