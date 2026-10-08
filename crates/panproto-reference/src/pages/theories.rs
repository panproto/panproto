//! Complete reference operations assigned to `theories`.

pub mod theory {
    //! Generalized algebraic theories: signature type checking and the colimit
    //! of a diagram of theories with explicit identification.
    //!
    //! A theory holds sorts with parameters, operations with dependent inputs
    //! and outputs, equations, directed equations and conflict policies; terms
    //! are variables, applications, case analyses, holes and local bindings.
    //! The expression-language parts of a directed equation (its
    //! implementation and inverse) and of a policy (its strategy) are carried
    //! as opaque text: composition transports them unchanged and compares them
    //! for equality, and never evaluates them.

    use std::collections::{BTreeMap, BTreeSet};

    /// A name.
    pub type Name = String;

    /// One branch of a case analysis: a constructor, one binder per input of
    /// the constructor, and the body in which the binders are bound.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct CaseBranch {
        /// The constructor operation.
        pub constructor: Name,
        /// The binders.
        pub binders: Vec<Name>,
        /// The body.
        pub body: Term,
    }

    /// A term.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Term {
        /// A variable.
        Var(Name),
        /// An application.
        App(Name, Vec<Self>),
        /// A case analysis of the scrutinee, one branch per constructor.
        Case(Box<Self>, Vec<CaseBranch>),
        /// A typed hole, named or anonymous.
        Hole(Option<Name>),
        /// `let name = bound in body`.
        Let(Name, Box<Self>, Box<Self>),
    }

    /// A directed equation: a named rewrite from `lhs` to `rhs`, with its
    /// implementation, optional inverse and value kinds as opaque text.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct DirectedEquation {
        /// Name.
        pub name: Name,
        /// The pattern.
        pub lhs: Term,
        /// The rewrite target.
        pub rhs: Term,
        /// The implementation (opaque).
        pub implementation: String,
        /// The inverse (opaque), if any.
        pub inverse: Option<String>,
        /// The source value kind, if any.
        pub source_kind: Option<Name>,
        /// The target value kind, if any.
        pub target_kind: Option<Name>,
        /// The round-trip classification (opaque).
        pub coercion_class: Name,
    }

    /// A conflict policy: a name, the value kind it applies to and its
    /// strategy (opaque). It mentions no sort or operation.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Policy {
        /// Name.
        pub name: Name,
        /// The value kind.
        pub value_kind: Name,
        /// The strategy (opaque).
        pub strategy: String,
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
        /// The constructors of a closed sort; `None` for an open sort.
        pub constructors: Option<Vec<Name>>,
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
        /// Directed equations.
        pub directed_eqs: Vec<DirectedEquation>,
        /// Conflict policies.
        pub policies: Vec<Policy>,
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
    /// an application's output with the operation's inputs substituted, a
    /// local binding's body in the context extended by the bound term's sort,
    /// and a case analysis's first branch body in the context extended by its
    /// constructor's inputs; a hole, or a term whose sort cannot be read, has
    /// none.
    fn term_sort(t: &Theory, ctx: &[(Name, SortExpr)], term: &Term) -> Option<SortExpr> {
        match term {
            Term::Var(x) => ctx
                .iter()
                .rev()
                .find(|(n, _)| n == x)
                .map(|(_, s)| s.clone()),
            Term::App(o, args) => {
                let op = t.ops.iter().find(|op| op.name == *o)?;
                if op.inputs.len() != args.len() {
                    return None;
                }
                for (i, arg) in args.iter().enumerate() {
                    let found = term_sort(t, ctx, arg)?;
                    let expected = subst_sort(&op.inputs[i].1, &op.inputs[..i], &args[..i]);
                    if found != expected {
                        return None;
                    }
                }
                Some(subst_sort(&op.output, &op.inputs, args))
            }
            Term::Hole(_) => None,
            Term::Let(x, bound, body) => {
                let mut inner = ctx.to_vec();
                inner.push((x.clone(), term_sort(t, ctx, bound)?));
                term_sort(t, &inner, body)
            }
            Term::Case(scrutinee, branches) => {
                let scrutinee_sort = term_sort(t, ctx, scrutinee)?;
                let mut result = None;
                for branch in branches {
                    let op = t.ops.iter().find(|op| op.name == branch.constructor)?;
                    if branch.binders.len() != op.inputs.len() {
                        return None;
                    }
                    let binder_terms: Vec<Term> =
                        branch.binders.iter().cloned().map(Term::Var).collect();
                    if subst_sort(&op.output, &op.inputs, &binder_terms) != scrutinee_sort {
                        return None;
                    }
                    let mut inner = ctx.to_vec();
                    for (i, binder) in branch.binders.iter().enumerate() {
                        inner.push((
                            binder.clone(),
                            subst_sort(&op.inputs[i].1, &op.inputs[..i], &binder_terms[..i]),
                        ));
                    }
                    let found = term_sort(t, &inner, &branch.body)?;
                    if result.as_ref().is_some_and(|expected| *expected != found) {
                        return None;
                    }
                    result = Some(found);
                }
                result
            }
        }
    }

    /// Infer the sorts of an equation's free variables from operation-input
    /// positions, then require both sides to be fully well typed at one sort.
    fn equation_well_typed(t: &Theory, lhs: &Term, rhs: &Term) -> Result<(), String> {
        fn collect(
            t: &Theory,
            term: &Term,
            bound: &BTreeSet<Name>,
            ctx: &mut BTreeMap<Name, SortExpr>,
        ) -> Result<(), String> {
            match term {
                Term::Var(_) => Ok(()),
                Term::Hole(_) => Err("an equation contains a hole".to_owned()),
                Term::App(name, args) => {
                    let op = t
                        .ops
                        .iter()
                        .find(|op| op.name == *name)
                        .ok_or_else(|| format!("operation {name} is not declared"))?;
                    if args.len() != op.inputs.len() {
                        return Err(format!(
                            "operation {name} expects {} arguments but has {}",
                            op.inputs.len(),
                            args.len()
                        ));
                    }
                    for (i, arg) in args.iter().enumerate() {
                        let expected = subst_sort(&op.inputs[i].1, &op.inputs[..i], &args[..i]);
                        if let Term::Var(variable) = arg
                            && !bound.contains(variable)
                        {
                            if let Some(previous) = ctx.get(variable) {
                                if previous != &expected {
                                    return Err(format!(
                                        "variable {variable} is constrained at both {previous:?} and {expected:?}"
                                    ));
                                }
                            } else {
                                ctx.insert(variable.clone(), expected);
                            }
                        }
                        collect(t, arg, bound, ctx)?;
                    }
                    Ok(())
                }
                Term::Let(name, value, body) => {
                    collect(t, value, bound, ctx)?;
                    let mut inner = bound.clone();
                    inner.insert(name.clone());
                    collect(t, body, &inner, ctx)
                }
                Term::Case(scrutinee, branches) => {
                    collect(t, scrutinee, bound, ctx)?;
                    for branch in branches {
                        let mut inner = bound.clone();
                        inner.extend(branch.binders.iter().cloned());
                        collect(t, &branch.body, &inner, ctx)?;
                    }
                    Ok(())
                }
            }
        }

        let mut inferred = BTreeMap::new();
        collect(t, lhs, &BTreeSet::new(), &mut inferred)?;
        collect(t, rhs, &BTreeSet::new(), &mut inferred)?;
        let ctx: Vec<(Name, SortExpr)> = inferred.into_iter().collect();
        let left = term_sort(t, &ctx, lhs)
            .ok_or_else(|| "the left side is not a well-typed term".to_owned())?;
        let right = term_sort(t, &ctx, rhs)
            .ok_or_else(|| "the right side is not a well-typed term".to_owned())?;
        if left == right {
            Ok(())
        } else {
            Err(format!(
                "the equation relates terms of different sorts: {left:?} and {right:?}"
            ))
        }
    }

    /// The free variables of a term.
    fn free_vars(term: &Term, out: &mut BTreeSet<Name>) {
        match term {
            Term::Var(x) => {
                out.insert(x.clone());
            }
            Term::App(_, xs) => xs.iter().for_each(|a| free_vars(a, out)),
            Term::Hole(_) => {}
            Term::Let(x, bound, body) => {
                free_vars(bound, out);
                let mut inner = BTreeSet::new();
                free_vars(body, &mut inner);
                inner.remove(x);
                out.extend(inner);
            }
            Term::Case(scrutinee, branches) => {
                free_vars(scrutinee, out);
                for br in branches {
                    let mut inner = BTreeSet::new();
                    free_vars(&br.body, &mut inner);
                    for b in &br.binders {
                        inner.remove(b);
                    }
                    out.extend(inner);
                }
            }
        }
    }

    /// A binder renamed away from `avoid` by appending `'` until it is fresh.
    fn fresh_binder(x: &str, avoid: &BTreeSet<Name>) -> Name {
        let mut y = x.to_owned();
        while avoid.contains(&y) {
            y.push('\'');
        }
        y
    }

    /// Substitute `args` for the variables `binders` bind, capture-avoiding:
    /// a binder of a case branch or a local binding shadows the substitution
    /// in its scope, and a binder that would capture a free variable of a
    /// substituted term is first renamed to a fresh name.
    fn subst_term(term: &Term, binders: &[(Name, SortExpr)], args: &[Term]) -> Term {
        // The substitution restricted to the names a scope does not rebind.
        let restrict = |names: &[Name]| -> (Vec<(Name, SortExpr)>, Vec<Term>) {
            binders
                .iter()
                .zip(args)
                .filter(|((n, _), _)| !names.contains(n))
                .map(|(b, a)| (b.clone(), a.clone()))
                .unzip()
        };
        // Rename the scope's binders that would capture a free variable of the
        // substituted terms, and the scope's body accordingly.
        let rename = |names: &[Name], body: &Term| -> (Vec<Name>, Term) {
            let (_, kept) = restrict(names);
            let mut captured = BTreeSet::new();
            for argument in &kept {
                free_vars(argument, &mut captured);
            }
            let mut avoid = captured.clone();
            free_vars(body, &mut avoid);
            avoid.extend(names.iter().cloned());
            let mut renamed = Vec::new();
            let mut body = body.clone();
            for n in names {
                if captured.contains(n) {
                    let fresh = fresh_binder(n, &avoid);
                    avoid.insert(fresh.clone());
                    body = subst_term(
                        &body,
                        &[(n.clone(), SortExpr::Name(String::new()))],
                        &[Term::Var(fresh.clone())],
                    );
                    renamed.push(fresh);
                } else {
                    renamed.push(n.clone());
                }
            }
            (renamed, body)
        };
        match term {
            Term::Var(x) => binders
                .iter()
                .rposition(|(n, _)| n == x)
                .and_then(|i| args.get(i).cloned())
                .unwrap_or_else(|| term.clone()),
            Term::App(o, xs) => Term::App(
                o.clone(),
                xs.iter().map(|a| subst_term(a, binders, args)).collect(),
            ),
            Term::Hole(_) => term.clone(),
            Term::Let(x, bound, body) => {
                let (names, body) = rename(std::slice::from_ref(x), body);
                let (b, a) = restrict(&names);
                Term::Let(
                    names[0].clone(),
                    Box::new(subst_term(bound, binders, args)),
                    Box::new(subst_term(&body, &b, &a)),
                )
            }
            Term::Case(scrutinee, branches) => Term::Case(
                Box::new(subst_term(scrutinee, binders, args)),
                branches
                    .iter()
                    .map(|br| {
                        let (names, body) = rename(&br.binders, &br.body);
                        let (b, a) = restrict(&names);
                        CaseBranch {
                            constructor: br.constructor.clone(),
                            binders: names,
                            body: subst_term(&body, &b, &a),
                        }
                    })
                    .collect(),
            ),
        }
    }

    fn subst_sort(s: &SortExpr, binders: &[(Name, SortExpr)], args: &[Term]) -> SortExpr {
        match s {
            SortExpr::Name(n) => SortExpr::Name(n.clone()),
            SortExpr::App(n, xs) => SortExpr::App(
                n.clone(),
                xs.iter().map(|a| subst_term(a, binders, args)).collect(),
            ),
        }
    }

    /// The first variable of a term that the context does not bind (a case
    /// branch's binders and a local binding's name bind in their bodies).
    fn unbound(ctx: &[(Name, SortExpr)], term: &Term) -> Option<Name> {
        let mut free = BTreeSet::new();
        free_vars(term, &mut free);
        // Report in order of first occurrence.
        first_unbound(term, &|x: &str| ctx.iter().any(|(n, _)| n == x), &free)
    }

    fn first_unbound(
        term: &Term,
        bound: &dyn Fn(&str) -> bool,
        free: &BTreeSet<Name>,
    ) -> Option<Name> {
        match term {
            Term::Var(x) => (free.contains(x) && !bound(x)).then(|| x.clone()),
            Term::App(_, xs) => xs.iter().find_map(|a| first_unbound(a, bound, free)),
            Term::Hole(_) => None,
            Term::Let(x, b, body) => first_unbound(b, bound, free)
                .or_else(|| first_unbound(body, &|y: &str| y == x || bound(y), free)),
            Term::Case(s, branches) => first_unbound(s, bound, free).or_else(|| {
                branches.iter().find_map(|br| {
                    first_unbound(
                        &br.body,
                        &|y: &str| br.binders.iter().any(|b| b == y) || bound(y),
                        free,
                    )
                })
            }),
        }
    }

    /// Check one sort expression in a context: the sort is declared (among
    /// `declared`), an application has the declared arity, every argument's
    /// variables are bound and its sort's head is the parameter's.
    fn check_expr(
        t: &Theory,
        declared: &[&Sort],
        ctx: &[(Name, SortExpr)],
        site: &Site,
        s: &SortExpr,
        out: &mut Vec<Defect>,
    ) {
        let Some(sort) = declared.iter().find(|d| d.name == s.head()) else {
            out.push(Defect::UndeclaredSort(site.clone(), s.head().to_owned()));
            return;
        };
        let SortExpr::App(_, args) = s else { return };
        if args.len() != sort.params.len() {
            out.push(Defect::SortArity(
                site.clone(),
                sort.name.clone(),
                sort.params.len(),
                args.len(),
            ));
            return;
        }
        for (i, a) in args.iter().enumerate() {
            if let Some(x) = unbound(ctx, a) {
                out.push(Defect::UnboundParamVar(site.clone(), x));
                continue;
            }
            let expected = subst_sort(&sort.params[i].1, &sort.params[..i], &args[..i]);
            match term_sort(t, ctx, a) {
                Some(found) if found != expected => out.push(Defect::ParamIllTyped(
                    site.clone(),
                    sort.name.clone(),
                    i,
                    expected.head().to_owned(),
                    found.head().to_owned(),
                )),
                None => out.push(Defect::ParamIllTyped(
                    site.clone(),
                    sort.name.clone(),
                    i,
                    expected.head().to_owned(),
                    "ill-typed".to_owned(),
                )),
                Some(_) => {}
            }
        }
    }

    fn duplicates(names: &[&Name]) -> Vec<Name> {
        let mut seen = BTreeSet::new();
        names
            .iter()
            .filter(|n| !seen.insert((**n).clone()))
            .map(|n| (*n).clone())
            .collect()
    }

    /// Definition: Signature well-formedness.
    ///
    /// Every sort parameter's sort is declared earlier; every sort expression of an operation names a
    /// declared sort, an application with the declared arity whose argument terms are typed against the
    /// parameters (with substitution) and whose variables are bound by earlier inputs; binder names are
    /// pairwise distinct within each telescope, and sort names and operation names are pairwise distinct
    /// within their respective namespaces.
    ///
    /// # Errors
    ///
    /// Every defect, sorted.
    ///
    /// Book anchors: `L-theories-1`, `L-theories-2`.
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
            check_expr(
                t,
                &all,
                &o.inputs,
                &Site::OpOutput(o.name.clone()),
                &o.output,
                &mut out,
            );
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
        /// A directed equation.
        DirectedEq,
        /// A conflict policy.
        Policy,
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
        NameClash(SymKind, Name, Box<Vec<Vec<SymRef>>>),
        /// A transported or generated equation is not well typed.
        IllTypedTransport {
            /// The source equation, or the generated equation's source op.
            source: SymRef,
            /// The independent typing judgement's failure.
            detail: String,
        },
        /// The result fails signature checking with defects other than the
        /// free names of implicit sorts.
        IllTypedResult(Vec<Defect>),
        /// Proposed cocone legs disagree on one result class.
        NotACocone(Vec<SymRef>),
    }

    /// A generated equation and the diagram operation whose term image
    /// generated it.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct AddedEquation {
        /// The edge's index in the caller's diagram.
        pub edge: usize,
        /// The source operation.
        pub op: SymRef,
    }

    /// Total source-to-result provenance, separated by symbol kind.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct Provenance {
        /// Source sorts.
        pub sorts: BTreeMap<SymRef, Name>,
        /// Source operations.
        pub ops: BTreeMap<SymRef, Name>,
        /// Source equations.
        pub eqs: BTreeMap<SymRef, Name>,
        /// Source directed equations.
        pub directed_eqs: BTreeMap<SymRef, Name>,
        /// Source conflict policies.
        pub policies: BTreeMap<SymRef, Name>,
        /// Equations generated by term-valued legs.
        pub added_eqs: BTreeMap<Name, AddedEquation>,
    }

    impl Provenance {
        const fn map(&self, kind: SymKind) -> &BTreeMap<SymRef, Name> {
            match kind {
                SymKind::Sort => &self.sorts,
                SymKind::Op => &self.ops,
                SymKind::Eq => &self.eqs,
                SymKind::DirectedEq => &self.directed_eqs,
                SymKind::Policy => &self.policies,
            }
        }

        const fn map_mut(&mut self, kind: SymKind) -> &mut BTreeMap<SymRef, Name> {
            match kind {
                SymKind::Sort => &mut self.sorts,
                SymKind::Op => &mut self.ops,
                SymKind::Eq => &mut self.eqs,
                SymKind::DirectedEq => &mut self.directed_eqs,
                SymKind::Policy => &mut self.policies,
            }
        }

        /// The image of one source symbol.
        #[must_use]
        pub fn image(&self, kind: SymKind, source: &SymRef) -> Option<&Name> {
            self.map(kind).get(source)
        }

        /// The source members of a result class, in ascending order.
        #[must_use]
        pub fn members(&self, kind: SymKind, result: &str) -> Vec<SymRef> {
            self.map(kind)
                .iter()
                .filter(|(_, image)| image.as_str() == result)
                .map(|(source, _)| source.clone())
                .collect()
        }

        /// Compatibility lookup by the former flat provenance key.
        #[must_use]
        pub fn get(&self, key: &(SymKind, SymRef)) -> Option<&Name> {
            self.image(key.0, &key.1)
        }
    }

    /// One result-name freshening.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Freshening {
        /// Symbol kind.
        pub kind: SymKind,
        /// Complete source class.
        pub members: Vec<SymRef>,
        /// Majority candidate.
        pub candidate: Name,
        /// Fresh result name.
        pub result: Name,
    }

    /// A composition check.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CompositionCheckKind {
        /// A diagram edge is a morphism.
        LegIsMorphism,
        /// A diagram node is well formed.
        NodeWellFormed,
        /// Quotient-class signatures agree.
        ClassSignaturesAgree,
        /// A cocone square commutes.
        CoconeCommutes,
        /// A generated inclusion is a morphism.
        InclusionIsMorphism,
        /// A node equation has a result image.
        EquationImage,
        /// A mediator preserves result equations.
        EquationEntailed,
        /// The identity cocone has the universal property.
        UniversalIdentity,
        /// A supplied cocone has the universal property.
        UniversalAgainst,
        /// The result theory is well typed.
        ResultWellTyped,
        /// Edge equivalence relates every shared source name.
        NameClashFree,
    }

    /// The subject of a composition check.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CheckSubject {
        /// The whole diagram.
        Diagram,
        /// One node label.
        Node(Name),
        /// One edge index.
        Edge(usize),
        /// One cocone leg index.
        Cocone(usize),
    }

    /// Why a check does not apply.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum InapplicableReason {
        /// Open signatures name undeclared result sorts.
        OpenTheory {
            /// Free sort names that prevent a total inclusion.
            missing_sorts: Vec<Name>,
        },
        /// The target theory contains no equations.
        NoEquations,
    }

    /// Why a composition check did not decide.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CompositionIncomplete {
        /// Rewriting exhausted its explicit step budget.
        RewriteBudgetExhausted {
            /// Equation whose entailment was being checked.
            equation: Name,
            /// Rewrite steps consumed.
            steps: u32,
        },
        /// No terminating rewrite system is available.
        NoTerminatingRewriteSystem,
    }

    /// The outcome of one composition check.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CheckOutcome {
        /// Established directly.
        Passed,
        /// Established by a generated equation.
        PassedByEquation {
            /// Equation that establishes the check.
            equation: Name,
        },
        /// Refuted.
        Failed {
            /// Deterministic failure explanation.
            detail: String,
        },
        /// Outside the check's domain.
        Inapplicable {
            /// Why this check has no subject in this composition.
            reason: InapplicableReason,
        },
        /// The bounded procedure did not decide.
        Incomplete {
            /// Why the check could not be completed.
            reason: CompositionIncomplete,
        },
    }

    /// One recorded composition check.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct CheckRecord {
        /// Check kind.
        pub check: CompositionCheckKind,
        /// Check subject.
        pub subject: CheckSubject,
        /// Check outcome.
        pub outcome: CheckOutcome,
    }

    /// Kleene-style aggregate status of the composition report.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CheckStatus {
        /// Every applicable check passed.
        Passed,
        /// At least one check failed.
        Failed,
        /// No check failed and at least one was incomplete or inapplicable.
        Incomplete,
    }

    /// Structured evidence emitted by composition.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct CompositionReport {
        /// Composition mode.
        pub mode: Mode,
        /// Aggregate of `checks`.
        pub status: CheckStatus,
        /// Whether all shared source names are related by diagram edges.
        pub name_clash_free: bool,
        /// Checks sorted by kind and subject.
        pub checks: Vec<CheckRecord>,
        /// Freshenings sorted by kind and source class.
        pub freshened: Vec<Freshening>,
    }

    /// The colimit, its inclusions, total provenance and structured report.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Composition {
        /// The result theory.
        pub theory: Theory,
        /// One cocone inclusion per input node, in input order.
        pub inclusions: Vec<Morphism>,
        /// Total source and generated-equation provenance.
        pub provenance: Provenance,
        /// Check outcomes and name choices.
        pub report: CompositionReport,
    }

    /// A name-free form: every variable replaced by the index of its binder in
    /// the telescope (`None` for a variable bound nowhere).
    fn name_free_term(binders: &[Name], t: &Term, class: &dyn Fn(&str) -> Name) -> Term {
        match t {
            Term::Var(x) => Term::Var(
                binders
                    .iter()
                    .rposition(|b| b == x)
                    .map_or_else(|| format!("free:{x}"), |i| format!("#{i}")),
            ),
            Term::App(o, xs) => Term::App(
                class(o),
                xs.iter()
                    .map(|a| name_free_term(binders, a, class))
                    .collect(),
            ),
            Term::Hole(name) => Term::Hole(name.clone()),
            Term::Let(x, bound, body) => {
                let mut inner = binders.to_vec();
                inner.push(x.clone());
                Term::Let(
                    String::new(),
                    Box::new(name_free_term(binders, bound, class)),
                    Box::new(name_free_term(&inner, body, class)),
                )
            }
            Term::Case(scrutinee, branches) => Term::Case(
                Box::new(name_free_term(binders, scrutinee, class)),
                branches
                    .iter()
                    .map(|br| {
                        let mut inner = binders.to_vec();
                        inner.extend(br.binders.iter().cloned());
                        CaseBranch {
                            constructor: class(&br.constructor),
                            binders: vec![String::new(); br.binders.len()],
                            body: name_free_term(&inner, &br.body, class),
                        }
                    })
                    .collect(),
            ),
        }
    }

    fn name_free_sort(
        binders: &[Name],
        s: &SortExpr,
        sort_class: &dyn Fn(&str) -> Name,
        op_class: &dyn Fn(&str) -> Name,
    ) -> SortExpr {
        match s {
            SortExpr::Name(n) => SortExpr::Name(sort_class(n)),
            SortExpr::App(n, xs) => SortExpr::App(
                sort_class(n),
                xs.iter()
                    .map(|a| name_free_term(binders, a, op_class))
                    .collect(),
            ),
        }
    }

    fn op_signature(
        o: &Op,
        sort_class: &dyn Fn(&str) -> Name,
        op_class: &dyn Fn(&str) -> Name,
    ) -> (Vec<SortExpr>, SortExpr) {
        let names: Vec<Name> = o.inputs.iter().map(|(n, _)| n.clone()).collect();
        let ins = o
            .inputs
            .iter()
            .enumerate()
            .map(|(i, (_, s))| name_free_sort(&names[..i], s, sort_class, op_class))
            .collect();
        (ins, name_free_sort(&names, &o.output, sort_class, op_class))
    }

    /// A sort's signature by class: arity, kind, constructors by operation
    /// class (in their order; `None` open) and name-free parameter sorts.
    type SortSignature = (usize, Name, Option<Vec<Name>>, Vec<SortExpr>);

    fn sort_signature(
        s: &Sort,
        sort_class: &dyn Fn(&str) -> Name,
        op_class: &dyn Fn(&str) -> Name,
    ) -> SortSignature {
        let names: Vec<Name> = s.params.iter().map(|(n, _)| n.clone()).collect();
        let params = s
            .params
            .iter()
            .enumerate()
            .map(|(i, (_, x))| name_free_sort(&names[..i], x, sort_class, op_class))
            .collect();
        let constructors = s
            .constructors
            .as_ref()
            .map(|cs| cs.iter().map(|c| op_class(c)).collect());
        (s.params.len(), s.kind.clone(), constructors, params)
    }

    /// A source term translated along a morphism: an operation with an
    /// operation image renamed, one with a term image replaced by that term
    /// with the operation's inputs substituted by the translated arguments.
    fn translate_term(src: &Theory, m: &Morphism, t: &Term) -> Term {
        match t {
            Term::Var(_) | Term::Hole(_) => t.clone(),
            Term::App(o, xs) => {
                let args: Vec<Term> = xs.iter().map(|a| translate_term(src, m, a)).collect();
                match (m.ops.get(o), src.ops.iter().find(|op| op.name == *o)) {
                    (Some(OpImage::Term(image)), Some(op)) if op.inputs.len() == args.len() => {
                        subst_term(image, &op.inputs, &args)
                    }
                    (Some(OpImage::Op(o2)), _) => Term::App(o2.clone(), args),
                    _ => Term::App(format!("unmapped:{o}"), args),
                }
            }
            Term::Let(x, bound, body) => Term::Let(
                x.clone(),
                Box::new(translate_term(src, m, bound)),
                Box::new(translate_term(src, m, body)),
            ),
            Term::Case(scrutinee, branches) => Term::Case(
                Box::new(translate_term(src, m, scrutinee)),
                branches
                    .iter()
                    .map(|br| CaseBranch {
                        constructor: match m.ops.get(&br.constructor) {
                            Some(OpImage::Op(o2)) => o2.clone(),
                            _ => format!("unmapped:{}", br.constructor),
                        },
                        binders: br.binders.clone(),
                        body: translate_term(src, m, &br.body),
                    })
                    .collect(),
            ),
        }
    }

    /// A source sort expression translated along a morphism: its sort renamed
    /// and its argument terms translated.
    fn translate_sort(src: &Theory, m: &Morphism, s: &SortExpr) -> SortExpr {
        let sort = |n: &str| {
            m.sorts
                .get(n)
                .cloned()
                .unwrap_or_else(|| format!("unmapped:{n}"))
        };
        match s {
            SortExpr::Name(n) => SortExpr::Name(sort(n)),
            SortExpr::App(n, xs) => SortExpr::App(
                sort(n),
                xs.iter().map(|a| translate_term(src, m, a)).collect(),
            ),
        }
    }

    /// The constructor operation denoted by a constructor image. Besides a
    /// plain operation rename, the image may be its eta expansion over the
    /// constructor's own inputs.
    fn constructor_image(src: &Theory, m: &Morphism, constructor: &str) -> Option<Name> {
        match m.ops.get(constructor)? {
            OpImage::Op(image) => Some(image.clone()),
            OpImage::Term(Term::App(image, args)) => {
                let op = src.ops.iter().find(|op| op.name == constructor)?;
                (args.len() == op.inputs.len()
                    && args.iter().enumerate().all(|(i, arg)| {
                        matches!(arg, Term::Var(name)
                            if op.inputs.iter().rposition(|(binder, _)| binder == name) == Some(i))
                    }))
                .then(|| image.clone())
            }
            OpImage::Term(_) => None,
        }
    }

    fn match_rule(pattern: &Term, value: &Term, substitution: &mut BTreeMap<Name, Term>) -> bool {
        match (pattern, value) {
            (Term::Var(name), value) => match substitution.entry(name.clone()) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.get() == value,
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(value.clone());
                    true
                }
            },
            (Term::App(left, left_args), Term::App(right, right_args)) => {
                left == right
                    && left_args.len() == right_args.len()
                    && left_args
                        .iter()
                        .zip(right_args)
                        .all(|(left, right)| match_rule(left, right, substitution))
            }
            (Term::Hole(left), Term::Hole(right)) => left == right,
            (
                Term::Let(left_name, left_bound, left_body),
                Term::Let(right_name, right_bound, right_body),
            ) => {
                if !match_rule(left_bound, right_bound, substitution) {
                    return false;
                }
                let saved = substitution.clone();
                substitution.insert(left_name.clone(), Term::Var(right_name.clone()));
                let matched = match_rule(left_body, right_body, substitution);
                *substitution = saved;
                matched
            }
            (
                Term::Case(left_scrutinee, left_branches),
                Term::Case(right_scrutinee, right_branches),
            ) => {
                match_rule(left_scrutinee, right_scrutinee, substitution)
                    && left_branches.len() == right_branches.len()
                    && left_branches
                        .iter()
                        .zip(right_branches)
                        .all(|(left, right)| {
                            if left.constructor != right.constructor
                                || left.binders.len() != right.binders.len()
                            {
                                return false;
                            }
                            let saved = substitution.clone();
                            for (left, right) in left.binders.iter().zip(&right.binders) {
                                substitution.insert(left.clone(), Term::Var(right.clone()));
                            }
                            let matched = match_rule(&left.body, &right.body, substitution);
                            *substitution = saved;
                            matched
                        })
            }
            _ => false,
        }
    }

    fn instantiate_rule(template: &Term, substitution: &BTreeMap<Name, Term>) -> Term {
        let binders: Vec<(Name, SortExpr)> = substitution
            .keys()
            .cloned()
            .map(|name| (name, SortExpr::Name(String::new())))
            .collect();
        let arguments: Vec<Term> = binders
            .iter()
            .filter_map(|(name, _)| substitution.get(name).cloned())
            .collect();
        subst_term(template, &binders, &arguments)
    }

    fn rewrite_root(term: &Term, rules: &[(Term, Term)]) -> Option<Term> {
        for (lhs, rhs) in rules {
            let mut substitution = BTreeMap::new();
            if match_rule(lhs, term, &mut substitution) {
                return Some(instantiate_rule(rhs, &substitution));
            }
        }
        None
    }

    fn term_depth(term: &Term) -> usize {
        match term {
            Term::Var(_) | Term::Hole(_) => 1,
            Term::App(_, arguments) => 1 + arguments.iter().map(term_depth).max().unwrap_or(0),
            Term::Let(_, bound, body) => 1 + term_depth(bound).max(term_depth(body)),
            Term::Case(scrutinee, branches) => {
                1 + branches
                    .iter()
                    .map(|branch| term_depth(&branch.body))
                    .fold(term_depth(scrutinee), usize::max)
            }
        }
    }

    fn normalize_bounded(term: &Term, rules: &[(Term, Term)]) -> Option<Term> {
        fn inner(
            term: &Term,
            rules: &[(Term, Term)],
            steps: &mut usize,
            depth_limit: usize,
        ) -> Option<Term> {
            let mut current = term.clone();
            loop {
                if *steps >= 512 || term_depth(&current) > depth_limit {
                    return None;
                }
                let normalized = match current {
                    Term::Var(_) | Term::Hole(_) => current,
                    Term::App(operation, arguments) => Term::App(
                        operation,
                        arguments
                            .iter()
                            .map(|argument| inner(argument, rules, steps, depth_limit))
                            .collect::<Option<Vec<_>>>()?,
                    ),
                    Term::Let(name, bound, body) => {
                        let bound = inner(&bound, rules, steps, depth_limit)?;
                        current =
                            subst_term(&body, &[(name, SortExpr::Name(String::new()))], &[bound]);
                        continue;
                    }
                    Term::Case(scrutinee, branches) => {
                        let scrutinee = inner(&scrutinee, rules, steps, depth_limit)?;
                        if let Term::App(operation, arguments) = &scrutinee
                            && let Some(branch) = branches.iter().find(|branch| {
                                branch.constructor == *operation
                                    && branch.binders.len() == arguments.len()
                            })
                        {
                            let binders = branch
                                .binders
                                .iter()
                                .cloned()
                                .map(|name| (name, SortExpr::Name(String::new())))
                                .collect::<Vec<_>>();
                            current = subst_term(&branch.body, &binders, arguments);
                            continue;
                        }
                        Term::Case(
                            Box::new(scrutinee),
                            branches
                                .iter()
                                .map(|branch| {
                                    Some(CaseBranch {
                                        constructor: branch.constructor.clone(),
                                        binders: branch.binders.clone(),
                                        body: inner(&branch.body, rules, steps, depth_limit)?,
                                    })
                                })
                                .collect::<Option<Vec<_>>>()?,
                        )
                    }
                };
                if *steps >= 512 {
                    return None;
                }
                let Some(next) = rewrite_root(&normalized, rules) else {
                    return Some(normalized);
                };
                *steps += 1;
                current = next;
            }
        }

        let depth_limit = rules
            .iter()
            .map(|(lhs, rhs)| term_depth(lhs).max(term_depth(rhs)))
            .fold(term_depth(term), usize::max)
            + 64;
        let mut steps = 0;
        inner(term, rules, &mut steps, depth_limit)
    }

    fn derivable(lhs: &Term, rhs: &Term, target: &Theory) -> bool {
        let directed: Vec<(Term, Term)> = target
            .directed_eqs
            .iter()
            .map(|equation| (equation.lhs.clone(), equation.rhs.clone()))
            .collect();
        let common = |rules: &[(Term, Term)]| {
            normalize_bounded(lhs, rules)
                .zip(normalize_bounded(rhs, rules))
                .is_some_and(|(left, right)| alpha(&left, &left).0 == alpha(&right, &right).0)
        };
        if common(&directed) {
            return true;
        }
        let mut full = directed;
        for equation in &target.eqs {
            full.push((equation.lhs.clone(), equation.rhs.clone()));
            full.push((equation.rhs.clone(), equation.lhs.clone()));
        }
        common(&full)
    }

    /// Whether every ordinary equation and directed equation of `src` has
    /// its translated axiom in `tgt`. Ordinary equations are unoriented;
    /// directed equations retain their orientation.
    fn equations_preserved(src: &Theory, tgt: &Theory, m: &Morphism) -> bool {
        let ordinary = src.eqs.iter().all(|equation| {
            let lhs = translate_term(src, m, &equation.lhs);
            let rhs = translate_term(src, m, &equation.rhs);
            let image = alpha(&lhs, &rhs);
            tgt.eqs.iter().any(|candidate| {
                alpha(&candidate.lhs, &candidate.rhs) == image
                    || alpha(&candidate.rhs, &candidate.lhs) == image
            }) || derivable(&lhs, &rhs, tgt)
        });
        let directed = src.directed_eqs.iter().all(|equation| {
            let lhs = translate_term(src, m, &equation.lhs);
            let rhs = translate_term(src, m, &equation.rhs);
            let image = alpha(&lhs, &rhs);
            tgt.directed_eqs
                .iter()
                .any(|candidate| alpha(&candidate.lhs, &candidate.rhs) == image)
        });
        ordinary && directed
    }

    /// Definition: Whether a leg is a morphism.
    ///
    /// Definition: every source sort maps to a declared target sort of the
    /// same arity whose parameter sorts are the images of the source's, and a
    /// closed source sort to a closed target sort whose constructors are the
    /// images of the source's; an open source sort imposes no closure
    /// condition. Every source
    /// operation maps to a target operation whose name-free signature is the
    /// image of the source signature, or to a term whose variables are the
    /// operation's inputs and whose sort, in the context of the translated
    /// inputs, is the translated output. The image of a sort expression is its
    /// sort renamed with every argument term translated, derived-term images
    /// substituted into sort arguments.
    ///
    /// Book anchors: `L-theories-7`.
    #[must_use]
    pub fn check_morphism(src: &Theory, tgt: &Theory, m: &Morphism) -> bool {
        let id = |x: &str| x.to_owned();
        let name_free = |binders: &[Name], s: &SortExpr| name_free_sort(binders, s, &id, &id);
        let sorts_ok = src.sorts.iter().all(|s| {
            let Some(ts) = m
                .sorts
                .get(&s.name)
                .and_then(|t| tgt.sorts.iter().find(|ts| ts.name == *t))
            else {
                return false;
            };
            let names: Vec<Name> = s.params.iter().map(|(n, _)| n.clone()).collect();
            let tnames: Vec<Name> = ts.params.iter().map(|(n, _)| n.clone()).collect();
            let params_ok = ts.params.len() == s.params.len()
                && s.params
                    .iter()
                    .zip(&ts.params)
                    .enumerate()
                    .all(|(i, ((_, x), (_, y)))| {
                        name_free(&names[..i], &translate_sort(src, m, x))
                            == name_free(&tnames[..i], y)
                    });
            let closure_ok = s.constructors.as_ref().is_none_or(|cs| {
                ts.constructors.as_ref().is_some_and(|ds| {
                    let images: Option<BTreeSet<Name>> = cs
                        .iter()
                        .map(|constructor| constructor_image(src, m, constructor))
                        .collect();
                    images
                        .is_some_and(|images| images == ds.iter().cloned().collect::<BTreeSet<_>>())
                })
            });
            s.kind == ts.kind && params_ok && closure_ok
        });
        let ops_ok = src.ops.iter().all(|o| {
            let names: Vec<Name> = o.inputs.iter().map(|(n, _)| n.clone()).collect();
            let inputs: Vec<(Name, SortExpr)> = o
                .inputs
                .iter()
                .map(|(x, s)| (x.clone(), translate_sort(src, m, s)))
                .collect();
            let output = translate_sort(src, m, &o.output);
            let image_sig = (
                inputs
                    .iter()
                    .enumerate()
                    .map(|(i, (_, s))| name_free(&names[..i], s))
                    .collect::<Vec<_>>(),
                name_free(&names, &output),
            );
            match m.ops.get(&o.name) {
                Some(OpImage::Op(o2)) => tgt.ops.iter().find(|t| t.name == *o2).is_some_and(|t| {
                    let tn: Vec<Name> = t.inputs.iter().map(|(n, _)| n.clone()).collect();
                    let tsig = (
                        t.inputs
                            .iter()
                            .enumerate()
                            .map(|(i, (_, s))| name_free(&tn[..i], s))
                            .collect::<Vec<_>>(),
                        name_free(&tn, &t.output),
                    );
                    tsig == image_sig
                }),
                Some(OpImage::Term(t)) => {
                    unbound(&o.inputs, t).is_none()
                        && term_sort(tgt, &inputs, t).is_some_and(|found| {
                            name_free(&names, &found) == name_free(&names, &output)
                        })
                }
                None => false,
            }
        });
        sorts_ok && ops_ok && equations_preserved(src, tgt, m)
    }

    /// Naive classes: each member's class label; joining relabels.
    struct Classes {
        label: BTreeMap<(SymKind, SymRef), usize>,
    }

    impl Classes {
        fn join(&mut self, a: &(SymKind, SymRef), b: &(SymKind, SymRef)) {
            let (Some(la), Some(lb)) = (self.label.get(a).copied(), self.label.get(b).copied())
            else {
                return;
            };
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
        count
            .into_iter()
            .find(|(_, c)| *c == best)
            .map(|(n, _)| n.clone())
            .unwrap_or_default()
    }

    /// Name every class of one kind: its candidate; under explicit identity, a
    /// candidate shared by several classes is a clash, refused or freshened as
    /// `{least member label}.{candidate}`, any remaining collision suffixed
    /// `#1`, `#2`, ... in ascending class order.
    fn name_classes(
        kind: SymKind,
        classes: &[Vec<SymRef>],
        mode: Mode,
        clash: ClashPolicy,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<Name>, ComposeError> {
        let cands: Vec<Name> = classes.iter().map(|c| majority(c)).collect();
        let mut names = cands.clone();
        if mode == Mode::ExplicitIdentity {
            for (i, c) in cands.iter().enumerate() {
                let sharing: Vec<usize> = (0..cands.len()).filter(|j| cands[*j] == *c).collect();
                if sharing.len() > 1 {
                    if clash == ClashPolicy::Reject {
                        return Err(ComposeError::NameClash(
                            kind,
                            c.clone(),
                            Box::new(sharing.iter().map(|j| classes[*j].clone()).collect()),
                        ));
                    }
                    let label = classes[i]
                        .iter()
                        .map(|m| m.node.clone())
                        .min()
                        .unwrap_or_default();
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
        for ((members, candidate), result) in classes.iter().zip(cands).zip(&names) {
            if candidate != *result {
                freshened.push(Freshening {
                    kind,
                    members: members.clone(),
                    candidate,
                    result: result.clone(),
                });
            }
        }
        Ok(names)
    }

    /// The sort names a node's signatures use without declaring them,
    /// ascending: the sorts of its `UndeclaredSort` defects.
    fn undeclared_sorts(t: &Theory) -> BTreeSet<Name> {
        typecheck_signature(t)
            .err()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|d| match d {
                Defect::UndeclaredSort(_, s) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// Definition: Naive quotient closure of `0..size` by finite identifications.
    ///
    /// Each join relabels the entire current class. A member's final label is
    /// the least member of its equivalence class.
    ///
    /// Book anchors: `L-theories-6`.
    #[must_use]
    pub fn quotient(size: usize, identifications: &[(usize, usize)]) -> Option<Vec<usize>> {
        let mut labels: Vec<usize> = (0..size).collect();
        for (left, right) in identifications {
            if *left >= size || *right >= size {
                return None;
            }
            let (left_label, right_label) = (labels[*left], labels[*right]);
            let representative = left_label.min(right_label);
            for label in &mut labels {
                if *label == left_label || *label == right_label {
                    *label = representative;
                }
            }
        }
        Some(labels)
    }

    /// Replay a recorded composition from the same finite diagram.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-theories-8`.
    pub use compose as recompose;

    /// Definition: The colimit of a diagram of theories.
    ///
    /// Definition (the construction, both modes, one engine): (1) validate
    /// the diagram: labels unique and well formed, edge endpoints exist, every
    /// node passes [`typecheck_signature`] (defects other than
    /// `UndeclaredSort` give `InvalidNode`), every edge morphism is a morphism
    /// between its endpoint theories (`InvalidLeg`); (2) symbols: the sorts
    /// and operations of all nodes, and, for each undeclared sort name `s`
    /// node `n` uses, an implicit sort symbol `(n, s)` of arity 0, joined by
    /// name in both modes to every declared sort named `s` of another node,
    /// and with the other nodes' implicit symbols named `s` (a class of
    /// implicit symbols only stays a free name, not added to the result's
    /// sorts); (3) identification: each leg's sort and operation with its
    /// image, and in `Amalgamate` mode every pair of same-kind, same-name
    /// symbols of two nodes; (4) class checks: sort classes agree on arity,
    /// kind, closure (constructors by class) and parameter sorts by class
    /// (`SortSignatureMismatch`), operation classes on their name-free
    /// signatures by class (`OpSignatureMismatch`), a class formed by the name
    /// rule alone that fails being `NameConflict` in `Amalgamate` mode; (5)
    /// for each edge and operation with a term image, the equation
    /// `class(o)(inputs) = translate(t)` named `"{src label}.{o}~{rank}"`, the
    /// rank of the edge in ascending (source label, target label, morphism)
    /// order; (6) one result sort and operation per class, named by majority
    /// (ties to the least name; under `ExplicitIdentity` a shared name is a
    /// clash, refused or freshened as `{least label}.{name}` and then suffixed
    /// `#k`); equations and directed equations translated along the cocone, a
    /// set under `ExplicitIdentity` (alpha-equivalent translations become one,
    /// directed ones also equal in their opaque parts), merged by name under
    /// `Amalgamate` (same name and different content is `NameConflict`);
    /// policies a set by content (value kind and strategy) named as symbols
    /// are, merged by name under `Amalgamate` (same name and different content
    /// is `NameConflict`); (7) the result passes [`typecheck_signature`], its
    /// `UndeclaredSort` defects on free names admitted (`IllTypedResult`
    /// otherwise).
    ///
    /// # Errors
    ///
    /// The first [`ComposeError`] of the steps in that order.
    ///
    /// Book anchors: `L-theories-3`, `L-theories-4`, `L-theories-5`.
    pub fn compose(
        nodes: &[(Name, Theory)],
        edges: &[(Name, Name, Morphism)],
        mode: Mode,
        clash: ClashPolicy,
        result_name: &str,
    ) -> Result<Composition, ComposeError> {
        let mut labels = BTreeSet::new();
        for (l, t) in nodes {
            if l.is_empty() || l.contains('.') || l.contains('#') {
                return Err(ComposeError::InvalidNodeLabel(l.clone()));
            }
            if !labels.insert(l.clone()) {
                return Err(ComposeError::DuplicateNodeLabel(l.clone()));
            }
            if let Err(d) = typecheck_signature(t)
                && !d.iter().all(|x| matches!(x, Defect::UndeclaredSort(..)))
            {
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
            let (Some(source), Some(target)) = (theory(s), theory(t)) else {
                return Err(ComposeError::InvalidLeg(i));
            };
            if !check_morphism(source, target, m) {
                return Err(ComposeError::InvalidLeg(i));
            }
        }
        let mut cl = Classes {
            label: BTreeMap::new(),
        };
        let mut implicit: BTreeSet<(SymKind, SymRef)> = BTreeSet::new();
        for (l, t) in nodes {
            for s in &t.sorts {
                let n = cl.label.len();
                cl.label.insert(sym(SymKind::Sort, l, &s.name), n);
            }
            for o in &t.ops {
                let n = cl.label.len();
                cl.label.insert(sym(SymKind::Op, l, &o.name), n);
            }
            for s in undeclared_sorts(t) {
                let n = cl.label.len();
                let key = sym(SymKind::Sort, l, &s);
                cl.label.insert(key.clone(), n);
                implicit.insert(key);
            }
        }
        let keys: Vec<(SymKind, SymRef)> = cl.label.keys().cloned().collect();
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
        join_implicit(&mut cl, &keys, &implicit, mode)?;
        let legs = Classes {
            label: cl.label.clone(),
        };
        if mode == Mode::Amalgamate {
            for a in &keys {
                for b in keys
                    .iter()
                    .filter(|b| b.0 == a.0 && b.1.name == a.1.name && b.1.node != a.1.node)
                {
                    cl.join(a, b);
                }
            }
        }
        let input = Input {
            nodes,
            edges,
            mode,
            clash,
        };
        finish(&input, &cl, &legs, &implicit, result_name)
    }

    /// Join each implicit sort symbol by name, after the legs: with the
    /// declared sorts of that name when some node declares it, else with the
    /// other nodes' implicit symbols of that name. Under explicit identity,
    /// declared sorts of that name lying in two or more classes after the legs
    /// make the name ambiguous, which is a `NameClash` of those classes.
    fn join_implicit(
        cl: &mut Classes,
        keys: &[(SymKind, SymRef)],
        implicit: &BTreeSet<(SymKind, SymRef)>,
        mode: Mode,
    ) -> Result<(), ComposeError> {
        let names: BTreeSet<&Name> = implicit.iter().map(|(_, r)| &r.name).collect();
        for name in names {
            let users: Vec<&(SymKind, SymRef)> =
                implicit.iter().filter(|(_, r)| r.name == *name).collect();
            let decls: Vec<&(SymKind, SymRef)> = keys
                .iter()
                .filter(|k| k.0 == SymKind::Sort && k.1.name == *name && !implicit.contains(*k))
                .collect();
            if decls.is_empty() {
                for pair in users.windows(2) {
                    cl.join(pair[0], pair[1]);
                }
                continue;
            }
            let classes: BTreeSet<usize> = decls
                .iter()
                .filter_map(|d| cl.label.get(*d).copied())
                .collect();
            if mode == Mode::ExplicitIdentity && classes.len() > 1 {
                let mut clashing: Vec<Vec<SymRef>> = classes
                    .iter()
                    .map(|l| {
                        let mut members: Vec<SymRef> = cl
                            .label
                            .iter()
                            .filter(|((k, _), x)| *k == SymKind::Sort && *x == l)
                            .map(|((_, r), _)| r.clone())
                            .collect();
                        members.sort();
                        members
                    })
                    .collect();
                clashing.sort();
                return Err(ComposeError::NameClash(
                    SymKind::Sort,
                    name.clone(),
                    Box::new(clashing),
                ));
            }
            for user in &users {
                for decl in &decls {
                    cl.join(user, decl);
                }
            }
        }
        Ok(())
    }

    /// A symbol key.
    fn sym(k: SymKind, n: &str, x: &str) -> (SymKind, SymRef) {
        (
            k,
            SymRef {
                node: n.to_owned(),
                name: x.to_owned(),
            },
        )
    }

    /// A term with every operation (applications and case constructors)
    /// renamed by `op`; variables, binders and holes are kept.
    fn rename_term(t: &Term, op: &dyn Fn(&str) -> Name) -> Term {
        match t {
            Term::Var(_) | Term::Hole(_) => t.clone(),
            Term::App(o, xs) => Term::App(op(o), xs.iter().map(|a| rename_term(a, op)).collect()),
            Term::Let(x, bound, body) => Term::Let(
                x.clone(),
                Box::new(rename_term(bound, op)),
                Box::new(rename_term(body, op)),
            ),
            Term::Case(scrutinee, branches) => Term::Case(
                Box::new(rename_term(scrutinee, op)),
                branches
                    .iter()
                    .map(|br| CaseBranch {
                        constructor: op(&br.constructor),
                        binders: br.binders.clone(),
                        body: rename_term(&br.body, op),
                    })
                    .collect(),
            ),
        }
    }

    fn rename_sort(
        s: &SortExpr,
        sort: &dyn Fn(&str) -> Name,
        op: &dyn Fn(&str) -> Name,
    ) -> SortExpr {
        match s {
            SortExpr::Name(n) => SortExpr::Name(sort(n)),
            SortExpr::App(n, xs) => {
                SortExpr::App(sort(n), xs.iter().map(|a| rename_term(a, op)).collect())
            }
        }
    }

    /// An equation with its variables renamed by order of first occurrence.
    fn alpha(lhs: &Term, rhs: &Term) -> (Term, Term) {
        /// The free variables of `t` in order of first occurrence, a binder's
        /// own occurrences in its scope left out.
        fn vars(t: &Term, bound: &[Name], out: &mut Vec<Name>) {
            match t {
                Term::Var(x) => {
                    if !bound.contains(x) && !out.contains(x) {
                        out.push(x.clone());
                    }
                }
                Term::App(_, xs) => xs.iter().for_each(|a| vars(a, bound, out)),
                Term::Hole(_) => {}
                Term::Let(x, b, body) => {
                    vars(b, bound, out);
                    let mut inner = bound.to_vec();
                    inner.push(x.clone());
                    vars(body, &inner, out);
                }
                Term::Case(s, branches) => {
                    vars(s, bound, out);
                    for br in branches {
                        let mut inner = bound.to_vec();
                        inner.extend(br.binders.iter().cloned());
                        vars(&br.body, &inner, out);
                    }
                }
            }
        }
        let mut order = Vec::new();
        vars(lhs, &[], &mut order);
        vars(rhs, &[], &mut order);
        let re = |t: &Term| name_free_term(&order, t, &|o| o.to_owned());
        (re(lhs), re(rhs))
    }

    /// The inputs of a composition that its later steps read.
    struct Input<'a> {
        nodes: &'a [(Name, Theory)],
        edges: &'a [(Name, Name, Morphism)],
        mode: Mode,
        clash: ClashPolicy,
    }

    #[allow(clippy::too_many_lines)]
    fn finish(
        input: &Input<'_>,
        cl: &Classes,
        legs: &Classes,
        implicit: &BTreeSet<(SymKind, SymRef)>,
        result_name: &str,
    ) -> Result<Composition, ComposeError> {
        let (nodes, edges, mode, clash) = (input.nodes, input.edges, input.mode, input.clash);
        check_signatures(nodes, cl, legs, implicit, mode)?;
        let theory = |l: &str| node_theory(nodes, l);
        let class_id = |k: SymKind, node: &str, name: &str| class_id(cl, k, node, name);
        let mut provenance = Provenance::default();
        let mut freshened = Vec::new();
        let mut result_of: BTreeMap<(SymKind, String), Name> = BTreeMap::new();
        for kind in [SymKind::Sort, SymKind::Op] {
            let classes = cl.members(kind);
            let names = name_classes(kind, &classes, mode, clash, &mut freshened)?;
            for (c, n) in classes.iter().zip(names) {
                for m in c {
                    provenance.map_mut(kind).insert(m.clone(), n.clone());
                    result_of.insert((kind, class_id(kind, &m.node, &m.name)), n.clone());
                }
            }
        }
        let res = |k: SymKind, node: &str, n: &str| {
            result_of
                .get(&(k, class_id(k, node, n)))
                .cloned()
                .unwrap_or_else(|| n.to_owned())
        };
        let mut out = Theory {
            name: result_name.to_owned(),
            ..Theory::default()
        };
        let mut free_sorts = BTreeSet::new();
        for c in cl.members(SymKind::Sort) {
            let declared = c
                .iter()
                .find(|m| !implicit.contains(&(SymKind::Sort, (*m).clone())));
            let Some(m) = declared else {
                free_sorts.insert(res(SymKind::Sort, &c[0].node, &c[0].name));
                continue;
            };
            if let Some(s) = theory(&m.node).sorts.iter().find(|s| s.name == m.name) {
                let (sr, or) = (
                    |n: &str| res(SymKind::Sort, &m.node, n),
                    |n: &str| res(SymKind::Op, &m.node, n),
                );
                out.sorts.push(Sort {
                    name: sr(&m.name),
                    params: s
                        .params
                        .iter()
                        .map(|(p, x)| (p.clone(), rename_sort(x, &sr, &or)))
                        .collect(),
                    kind: s.kind.clone(),
                    constructors: s
                        .constructors
                        .as_ref()
                        .map(|cs| cs.iter().map(|c| or(c)).collect()),
                });
            }
        }
        for c in cl.members(SymKind::Op) {
            let m = &c[0];
            if let Some(o) = theory(&m.node).ops.iter().find(|o| o.name == m.name) {
                let (sr, or) = (
                    |n: &str| res(SymKind::Sort, &m.node, n),
                    |n: &str| res(SymKind::Op, &m.node, n),
                );
                out.ops.push(Op {
                    name: or(&m.name),
                    inputs: o
                        .inputs
                        .iter()
                        .map(|(p, x)| (p.clone(), rename_sort(x, &sr, &or)))
                        .collect(),
                    output: rename_sort(&o.output, &sr, &or),
                });
            }
        }
        let (eqs, added_eqs) = equation_inputs(nodes, edges, &res);
        provenance.added_eqs = added_eqs;
        for (source, _, lhs, rhs) in &eqs {
            equation_well_typed(&out, lhs, rhs).map_err(|detail| {
                ComposeError::IllTypedTransport {
                    source: source.clone(),
                    detail,
                }
            })?;
        }
        for (label, node) in nodes {
            let op = |name: &str| res(SymKind::Op, label, name);
            for equation in &node.directed_eqs {
                let lhs = rename_term(&equation.lhs, &op);
                let rhs = rename_term(&equation.rhs, &op);
                equation_well_typed(&out, &lhs, &rhs).map_err(|detail| {
                    ComposeError::IllTypedTransport {
                        source: SymRef {
                            node: label.clone(),
                            name: equation.name.clone(),
                        },
                        detail,
                    }
                })?;
            }
        }
        out.eqs = equations(&eqs, mode, clash, &mut provenance, &mut freshened)?;
        out.directed_eqs =
            directed_equations(nodes, &res, mode, clash, &mut provenance, &mut freshened)?;
        out.policies = policies(nodes, mode, clash, &mut provenance, &mut freshened)?;
        out.sorts.sort_by(|a, b| a.name.cmp(&b.name));
        out.ops.sort_by(|a, b| a.name.cmp(&b.name));
        if let Err(defects) = typecheck_signature(&out) {
            let ill: Vec<Defect> = defects
                .into_iter()
                .filter(|d| !matches!(d, Defect::UndeclaredSort(_, s) if free_sorts.contains(s)))
                .collect();
            if !ill.is_empty() {
                return Err(ComposeError::IllTypedResult(ill));
            }
        }
        let inclusions: Vec<Morphism> = nodes
            .iter()
            .map(|(label, node)| Morphism {
                sorts: node
                    .sorts
                    .iter()
                    .filter_map(|sort| {
                        provenance
                            .sorts
                            .get(&SymRef {
                                node: label.clone(),
                                name: sort.name.clone(),
                            })
                            .map(|image| (sort.name.clone(), image.clone()))
                    })
                    .collect(),
                ops: node
                    .ops
                    .iter()
                    .filter_map(|op| {
                        provenance
                            .ops
                            .get(&SymRef {
                                node: label.clone(),
                                name: op.name.clone(),
                            })
                            .map(|image| (op.name.clone(), OpImage::Op(image.clone())))
                    })
                    .collect(),
            })
            .collect();
        let name_clash_free = source_names_are_related(legs);
        let mut checks = Vec::new();
        for (label, _) in nodes {
            checks.push(CheckRecord {
                check: CompositionCheckKind::NodeWellFormed,
                subject: CheckSubject::Node(label.clone()),
                outcome: CheckOutcome::Passed,
            });
        }
        for (edge, _) in edges.iter().enumerate() {
            checks.push(CheckRecord {
                check: CompositionCheckKind::LegIsMorphism,
                subject: CheckSubject::Edge(edge),
                outcome: CheckOutcome::Passed,
            });
            let equation = provenance
                .added_eqs
                .iter()
                .find(|(_, added)| added.edge == edge)
                .map(|(name, _)| name.clone());
            checks.push(CheckRecord {
                check: CompositionCheckKind::CoconeCommutes,
                subject: CheckSubject::Edge(edge),
                outcome: equation.map_or(CheckOutcome::Passed, |equation| {
                    CheckOutcome::PassedByEquation { equation }
                }),
            });
        }
        for (index, (label, node)) in nodes.iter().enumerate() {
            let missing_sorts: Vec<Name> = undeclared_sorts(node)
                .into_iter()
                .filter_map(|name| {
                    provenance
                        .sorts
                        .get(&SymRef {
                            node: label.clone(),
                            name,
                        })
                        .filter(|image| free_sorts.contains(*image))
                        .cloned()
                })
                .collect();
            let outcome = if missing_sorts.is_empty() {
                CheckOutcome::Passed
            } else {
                CheckOutcome::Inapplicable {
                    reason: InapplicableReason::OpenTheory { missing_sorts },
                }
            };
            checks.push(CheckRecord {
                check: CompositionCheckKind::InclusionIsMorphism,
                subject: CheckSubject::Cocone(index),
                outcome,
            });
            checks.push(CheckRecord {
                check: CompositionCheckKind::EquationImage,
                subject: CheckSubject::Node(label.clone()),
                outcome: CheckOutcome::Passed,
            });
        }
        checks.push(CheckRecord {
            check: CompositionCheckKind::ClassSignaturesAgree,
            subject: CheckSubject::Diagram,
            outcome: CheckOutcome::Passed,
        });
        checks.push(CheckRecord {
            check: CompositionCheckKind::UniversalIdentity,
            subject: CheckSubject::Diagram,
            outcome: if free_sorts.is_empty() {
                CheckOutcome::Passed
            } else {
                CheckOutcome::Inapplicable {
                    reason: InapplicableReason::OpenTheory {
                        missing_sorts: free_sorts.iter().cloned().collect(),
                    },
                }
            },
        });
        checks.push(CheckRecord {
            check: CompositionCheckKind::ResultWellTyped,
            subject: CheckSubject::Diagram,
            outcome: CheckOutcome::Passed,
        });
        checks.push(CheckRecord {
            check: CompositionCheckKind::NameClashFree,
            subject: CheckSubject::Diagram,
            outcome: if name_clash_free {
                CheckOutcome::Passed
            } else {
                CheckOutcome::Failed {
                    detail: "two source nodes share an unrelated symbol name".to_owned(),
                }
            },
        });
        checks.sort();
        freshened.sort();
        let status = aggregate_status(&checks);
        Ok(Composition {
            theory: out,
            inclusions,
            provenance,
            report: CompositionReport {
                mode,
                status,
                name_clash_free,
                checks,
                freshened,
            },
        })
    }

    /// Whether every same-kind, same-name pair is already related by the
    /// diagram's edge equivalence, before amalgamation adds name joins.
    fn source_names_are_related(legs: &Classes) -> bool {
        let keys: Vec<&(SymKind, SymRef)> = legs.label.keys().collect();
        keys.iter().all(|left| {
            keys.iter().all(|right| {
                left.0 != right.0
                    || left.1.name != right.1.name
                    || left.1.node == right.1.node
                    || legs.label.get(*left) == legs.label.get(*right)
            })
        })
    }

    fn aggregate_status(checks: &[CheckRecord]) -> CheckStatus {
        if checks
            .iter()
            .any(|record| matches!(record.outcome, CheckOutcome::Failed { .. }))
        {
            CheckStatus::Failed
        } else if checks.iter().any(|record| {
            matches!(
                record.outcome,
                CheckOutcome::Incomplete { .. } | CheckOutcome::Inapplicable { .. }
            )
        }) {
            CheckStatus::Incomplete
        } else {
            CheckStatus::Passed
        }
    }

    fn op_images_equal(left: &OpImage, right: &OpImage) -> bool {
        match (left, right) {
            (OpImage::Op(left), OpImage::Op(right)) => left == right,
            (OpImage::Term(left), OpImage::Term(right)) => {
                alpha(left, left).0 == alpha(right, right).0
            }
            _ => false,
        }
    }

    fn image_through(target: &Theory, leg: &Morphism, image: &OpImage) -> Option<OpImage> {
        match image {
            OpImage::Op(operation) => leg.ops.get(operation).cloned(),
            OpImage::Term(term) => Some(OpImage::Term(translate_term(target, leg, term))),
        }
    }

    impl Composition {
        /// Verify the universal arrow against one proposed cocone and return
        /// the uniquely induced mediator together with the extended report.
        ///
        /// Every proposed leg must be a morphism, every diagram square must
        /// commute under the whole action on terms, and all members of a
        /// result class must have one image. The mediator is then checked as a
        /// morphism, including equation preservation.
        ///
        /// # Errors
        ///
        /// Returns [`ComposeError::NotACocone`] if the supplied legs do not
        /// commute or disagree on a result class, or
        /// [`ComposeError::InvalidLeg`] if the induced mediator is not a
        /// morphism.
        #[allow(clippy::too_many_lines)]
        pub fn verify_universal(
            &self,
            nodes: &[(Name, Theory)],
            edges: &[(Name, Name, Morphism)],
            target: &Theory,
            legs: &[Morphism],
        ) -> Result<(Morphism, CompositionReport), ComposeError> {
            if legs.len() != nodes.len() {
                return Err(ComposeError::NotACocone(Vec::new()));
            }
            for ((_, node), leg) in nodes.iter().zip(legs) {
                if !check_morphism(node, target, leg) {
                    return Err(ComposeError::NotACocone(Vec::new()));
                }
            }
            let positions: BTreeMap<&str, usize> = nodes
                .iter()
                .enumerate()
                .map(|(index, (label, _))| (label.as_str(), index))
                .collect();
            for (source, target_label, edge) in edges {
                let Some(&source_index) = positions.get(source.as_str()) else {
                    return Err(ComposeError::NotACocone(Vec::new()));
                };
                let Some(&target_index) = positions.get(target_label.as_str()) else {
                    return Err(ComposeError::NotACocone(Vec::new()));
                };
                let target_node = &nodes[target_index].1;
                for (sort, image) in &edge.sorts {
                    if legs[source_index].sorts.get(sort) != legs[target_index].sorts.get(image) {
                        let class = self
                            .provenance
                            .image(
                                SymKind::Sort,
                                &SymRef {
                                    node: source.clone(),
                                    name: sort.clone(),
                                },
                            )
                            .map_or_else(Vec::new, |result| {
                                self.provenance.members(SymKind::Sort, result)
                            });
                        return Err(ComposeError::NotACocone(class));
                    }
                }
                for (operation, image) in &edge.ops {
                    let left = legs[source_index].ops.get(operation);
                    let right = image_through(target_node, &legs[target_index], image);
                    if left.is_none_or(|left| {
                        right
                            .as_ref()
                            .is_none_or(|right| !op_images_equal(left, right))
                    }) {
                        let class = self
                            .provenance
                            .image(
                                SymKind::Op,
                                &SymRef {
                                    node: source.clone(),
                                    name: operation.clone(),
                                },
                            )
                            .map_or_else(Vec::new, |result| {
                                self.provenance.members(SymKind::Op, result)
                            });
                        return Err(ComposeError::NotACocone(class));
                    }
                }
            }

            let mut mediator = Morphism::default();
            for result_sort in &self.theory.sorts {
                let members = self.provenance.members(SymKind::Sort, &result_sort.name);
                let mut image: Option<Name> = None;
                for member in &members {
                    let Some(&index) = positions.get(member.node.as_str()) else {
                        return Err(ComposeError::NotACocone(members.clone()));
                    };
                    let Some(candidate) = legs[index].sorts.get(&member.name) else {
                        return Err(ComposeError::NotACocone(members.clone()));
                    };
                    if image.as_ref().is_some_and(|first| first != candidate) {
                        return Err(ComposeError::NotACocone(members.clone()));
                    }
                    image = Some(candidate.clone());
                }
                if let Some(image) = image {
                    mediator.sorts.insert(result_sort.name.clone(), image);
                }
            }
            for result_op in &self.theory.ops {
                let members = self.provenance.members(SymKind::Op, &result_op.name);
                let mut image: Option<OpImage> = None;
                for member in &members {
                    let Some(&index) = positions.get(member.node.as_str()) else {
                        return Err(ComposeError::NotACocone(members.clone()));
                    };
                    let Some(source_op) = nodes[index]
                        .1
                        .ops
                        .iter()
                        .find(|operation| operation.name == member.name)
                    else {
                        return Err(ComposeError::NotACocone(members.clone()));
                    };
                    let Some(candidate) = legs[index].ops.get(&member.name) else {
                        return Err(ComposeError::NotACocone(members.clone()));
                    };
                    let rebound = match candidate {
                        OpImage::Op(name) => OpImage::Op(name.clone()),
                        OpImage::Term(term) => OpImage::Term(subst_term(
                            term,
                            &source_op.inputs,
                            &result_op
                                .inputs
                                .iter()
                                .map(|(name, _)| Term::Var(name.clone()))
                                .collect::<Vec<_>>(),
                        )),
                    };
                    if image
                        .as_ref()
                        .is_some_and(|first| !op_images_equal(first, &rebound))
                    {
                        return Err(ComposeError::NotACocone(members.clone()));
                    }
                    image = Some(rebound);
                }
                if let Some(image) = image {
                    mediator.ops.insert(result_op.name.clone(), image);
                }
            }
            if !check_morphism(&self.theory, target, &mediator) {
                let members = self.theory.ops.first().map_or_else(Vec::new, |operation| {
                    self.provenance.members(SymKind::Op, &operation.name)
                });
                return Err(ComposeError::NotACocone(members));
            }

            let mut report = self.report.clone();
            report.checks.push(CheckRecord {
                check: CompositionCheckKind::EquationEntailed,
                subject: CheckSubject::Diagram,
                outcome: if target.eqs.is_empty() && target.directed_eqs.is_empty() {
                    CheckOutcome::Inapplicable {
                        reason: InapplicableReason::NoEquations,
                    }
                } else {
                    CheckOutcome::Passed
                },
            });
            report.checks.push(CheckRecord {
                check: CompositionCheckKind::UniversalAgainst,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Passed,
            });
            report.checks.sort();
            report.status = aggregate_status(&report.checks);
            Ok((mediator, report))
        }
    }

    /// The theory at node label `l` (empty when there is none).
    fn node_theory(nodes: &[(Name, Theory)], l: &str) -> Theory {
        nodes
            .iter()
            .find(|(x, _)| x == l)
            .map(|(_, t)| t.clone())
            .unwrap_or_default()
    }

    /// The class id of a symbol, or `?name` for a symbol in no class.
    fn class_id(cl: &Classes, k: SymKind, node: &str, name: &str) -> String {
        cl.label
            .get(&(
                k,
                SymRef {
                    node: node.to_owned(),
                    name: name.to_owned(),
                },
            ))
            .map_or_else(|| format!("?{name}"), |l| format!("c{l}"))
    }

    /// Refuse a class whose members' signatures (read through the classes)
    /// differ: a name conflict under amalgamation across legs, else a
    /// signature mismatch. An implicit sort symbol's signature is its arity,
    /// 0; it agrees with a declared member of arity 0 whatever that member's
    /// kind and closure.
    fn check_signatures(
        nodes: &[(Name, Theory)],
        cl: &Classes,
        legs: &Classes,
        implicit: &BTreeSet<(SymKind, SymRef)>,
        mode: Mode,
    ) -> Result<(), ComposeError> {
        let theory = |l: &str| node_theory(nodes, l);
        let class_id = |k: SymKind, node: &str, name: &str| class_id(cl, k, node, name);
        for kind in [SymKind::Sort, SymKind::Op] {
            for members in cl.members(kind) {
                let sig = |m: &SymRef| {
                    let t = theory(&m.node);
                    let (sc, oc) = (
                        |n: &str| class_id(SymKind::Sort, &m.node, n),
                        |n: &str| class_id(SymKind::Op, &m.node, n),
                    );
                    match kind {
                        SymKind::Sort => t
                            .sorts
                            .iter()
                            .find(|s| s.name == m.name)
                            .map(|s| format!("{:?}", sort_signature(s, &sc, &oc))),
                        _ => t
                            .ops
                            .iter()
                            .find(|o| o.name == m.name)
                            .map(|o| format!("{:?}", op_signature(o, &sc, &oc))),
                    }
                };
                let arity = |m: &SymRef| {
                    theory(&m.node)
                        .sorts
                        .iter()
                        .find(|s| s.name == m.name)
                        .map_or(0, |s| s.params.len())
                };
                let is_implicit = |m: &SymRef| implicit.contains(&(kind, m.clone()));
                let declared: Vec<&SymRef> = members.iter().filter(|m| !is_implicit(m)).collect();
                let mismatch = declared.first().and_then(|first| {
                    declared
                        .iter()
                        .find(|m| sig(m) != sig(first))
                        .map(|second| ((*first).clone(), (*second).clone()))
                        .or_else(|| {
                            members
                                .iter()
                                .find(|m| is_implicit(m) && arity(first) != 0)
                                .map(|m| ((*first).clone(), m.clone()))
                        })
                });
                if let Some((first, second)) = mismatch {
                    let same_leg = legs.label.get(&(kind, first.clone()))
                        == legs.label.get(&(kind, second.clone()));
                    return Err(if mode == Mode::Amalgamate && !same_leg {
                        ComposeError::NameConflict(kind, first.name.clone(), first, second)
                    } else if kind == SymKind::Sort {
                        ComposeError::SortSignatureMismatch(first, second)
                    } else {
                        ComposeError::OpSignatureMismatch(first, second)
                    });
                }
            }
        }
        Ok(())
    }

    type EquationInputs = (
        Vec<(SymRef, Name, Term, Term)>,
        BTreeMap<Name, AddedEquation>,
    );

    /// The equations to classify: every node theory's equations renamed into
    /// the result, then one per term-valued operation image, for the edges in
    /// (source, target, morphism) order.
    fn equation_inputs(
        nodes: &[(Name, Theory)],
        edges: &[(Name, Name, Morphism)],
        res: &dyn Fn(SymKind, &str, &str) -> Name,
    ) -> EquationInputs {
        let theory = |l: &str| node_theory(nodes, l);
        let mut eqs: Vec<(SymRef, Name, Term, Term)> = Vec::new();
        let mut added = BTreeMap::new();
        for (l, t) in nodes {
            for e in &t.eqs {
                let op = |n: &str| res(SymKind::Op, l, n);
                eqs.push((
                    SymRef {
                        node: l.clone(),
                        name: e.name.clone(),
                    },
                    e.name.clone(),
                    rename_term(&e.lhs, &op),
                    rename_term(&e.rhs, &op),
                ));
            }
        }
        let mut ordered: Vec<(usize, &(Name, Name, Morphism))> = edges.iter().enumerate().collect();
        ordered.sort_by(|a, b| {
            (&a.1.0, &a.1.1, format!("{:?}", a.1.2)).cmp(&(&b.1.0, &b.1.1, format!("{:?}", b.1.2)))
        });
        for (rank, (edge_index, (s, t, m))) in ordered.iter().enumerate() {
            for (o, img) in &m.ops {
                let OpImage::Term(term) = img else { continue };
                let Some(op) = theory(s).ops.into_iter().find(|x| x.name == *o) else {
                    continue;
                };
                let lhs = Term::App(
                    res(SymKind::Op, s, o),
                    op.inputs
                        .iter()
                        .map(|(x, _)| Term::Var(x.clone()))
                        .collect(),
                );
                let rhs = rename_term(term, &|n| res(SymKind::Op, t, n));
                let name = format!("{s}.{o}~{rank}");
                added.insert(
                    name.clone(),
                    AddedEquation {
                        edge: *edge_index,
                        op: SymRef {
                            node: s.clone(),
                            name: o.clone(),
                        },
                    },
                );
                eqs.push((
                    SymRef {
                        node: s.clone(),
                        name: name.clone(),
                    },
                    name,
                    lhs,
                    rhs,
                ));
            }
        }
        (eqs, added)
    }

    /// A class of equation-like items: its members, its content key, and the
    /// representative item.
    type ItemClass<K, T> = (Vec<SymRef>, K, T);

    /// Classify named items by content: under explicit identity every item
    /// joins the class of equal content; under amalgamation an item joins the
    /// class of equal content holding its name, and an item whose name a class
    /// of other content holds is a name conflict. Classes are then named as
    /// symbols are.
    fn classify_items<K: PartialEq, T: Clone>(
        kind: SymKind,
        items: Vec<(SymRef, Name, K, T)>,
        mode: Mode,
        clash: ClashPolicy,
        provenance: &mut Provenance,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<(Name, T)>, ComposeError> {
        let mut classes: Vec<ItemClass<K, T>> = Vec::new();
        for (r, name, key, item) in items {
            let found = classes.iter_mut().find(|c| {
                c.1 == key && (mode == Mode::ExplicitIdentity || c.0.iter().any(|m| m.name == name))
            });
            if let Some(c) = found {
                c.0.push(r);
                continue;
            }
            if mode == Mode::Amalgamate
                && let Some(c) = classes.iter().find(|c| c.0.iter().any(|m| m.name == name))
            {
                return Err(ComposeError::NameConflict(kind, name, c.0[0].clone(), r));
            }
            classes.push((vec![r], key, item));
        }
        for c in &mut classes {
            c.0.sort();
        }
        classes.sort_by(|a, b| a.0.cmp(&b.0));
        let members: Vec<Vec<SymRef>> = classes.iter().map(|c| c.0.clone()).collect();
        let names = name_classes(kind, &members, mode, clash, freshened)?;
        let mut out = Vec::new();
        for (c, n) in classes.into_iter().zip(names) {
            for m in &c.0 {
                provenance.map_mut(kind).insert(m.clone(), n.clone());
            }
            out.push((n, c.2));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// The result equations: alpha-equivalent translations are one equation
    /// under explicit identity, and under amalgamation same-named
    /// alpha-equivalent ones merge and same-named different ones conflict.
    fn equations(
        eqs: &[(SymRef, Name, Term, Term)],
        mode: Mode,
        clash: ClashPolicy,
        provenance: &mut Provenance,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<Equation>, ComposeError> {
        let items = eqs
            .iter()
            .map(|(r, name, l, rr)| {
                (
                    r.clone(),
                    name.clone(),
                    alpha(l, rr),
                    (l.clone(), rr.clone()),
                )
            })
            .collect();
        Ok(
            classify_items(SymKind::Eq, items, mode, clash, provenance, freshened)?
                .into_iter()
                .map(|(name, (lhs, rhs))| Equation { name, lhs, rhs })
                .collect(),
        )
    }

    /// The result directed equations: each node's directed equations with
    /// their operations translated along the cocone, classified as equations
    /// are, the content being the alpha-normal sides together with the opaque
    /// implementation, inverse, value kinds and classification.
    fn directed_equations(
        nodes: &[(Name, Theory)],
        res: &dyn Fn(SymKind, &str, &str) -> Name,
        mode: Mode,
        clash: ClashPolicy,
        provenance: &mut Provenance,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<DirectedEquation>, ComposeError> {
        let mut items = Vec::new();
        for (l, t) in nodes {
            let op = |n: &str| res(SymKind::Op, l, n);
            for d in &t.directed_eqs {
                let translated = DirectedEquation {
                    lhs: rename_term(&d.lhs, &op),
                    rhs: rename_term(&d.rhs, &op),
                    ..d.clone()
                };
                let key = (
                    alpha(&translated.lhs, &translated.rhs),
                    translated.implementation.clone(),
                    translated.inverse.clone(),
                    translated.source_kind.clone(),
                    translated.target_kind.clone(),
                    translated.coercion_class.clone(),
                );
                let r = SymRef {
                    node: l.clone(),
                    name: d.name.clone(),
                };
                items.push((r, d.name.clone(), key, translated));
            }
        }
        Ok(classify_items(
            SymKind::DirectedEq,
            items,
            mode,
            clash,
            provenance,
            freshened,
        )?
        .into_iter()
        .map(|(name, d)| DirectedEquation { name, ..d })
        .collect())
    }

    /// The result policies: transported unchanged (a policy mentions no sort
    /// or operation), classified by content (value kind and strategy) as
    /// equations are.
    fn policies(
        nodes: &[(Name, Theory)],
        mode: Mode,
        clash: ClashPolicy,
        provenance: &mut Provenance,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<Policy>, ComposeError> {
        let mut items = Vec::new();
        for (l, t) in nodes {
            for p in &t.policies {
                let r = SymRef {
                    node: l.clone(),
                    name: p.name.clone(),
                };
                let key = (p.value_kind.clone(), p.strategy.clone());
                items.push((r, p.name.clone(), key, p.clone()));
            }
        }
        Ok(
            classify_items(SymKind::Policy, items, mode, clash, provenance, freshened)?
                .into_iter()
                .map(|(name, p)| Policy { name, ..p })
                .collect(),
        )
    }
}

pub use self::theory::{check_morphism, compose, quotient, recompose, typecheck_signature};
