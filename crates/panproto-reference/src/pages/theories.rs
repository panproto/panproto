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

    use crate::canonical::Writer;

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
    #[derive(Clone, Debug)]
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

        fn arguments(&self) -> &[Term] {
            match self {
                Self::Name(_) => &[],
                Self::App(_, arguments) => arguments,
            }
        }
    }

    impl PartialEq for SortExpr {
        fn eq(&self, other: &Self) -> bool {
            self.head() == other.head() && self.arguments() == other.arguments()
        }
    }

    impl Eq for SortExpr {}

    impl std::hash::Hash for SortExpr {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            std::hash::Hash::hash(self.head(), state);
            std::hash::Hash::hash(self.arguments(), state);
        }
    }

    impl PartialOrd for SortExpr {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for SortExpr {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            (self.head(), self.arguments()).cmp(&(other.head(), other.arguments()))
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

    /// Whether an operation input is caller-supplied or inferred from the
    /// sorts of the caller-supplied inputs.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum Implicit {
        /// The caller supplies this input.
        #[default]
        No,
        /// The input is recovered by first-order unification.
        Yes,
    }

    /// An operation declaration.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Op {
        /// Name.
        pub name: Name,
        /// Inputs `(name, sort, implicit)` in telescope order.
        pub inputs: Vec<(Name, SortExpr, Implicit)>,
        /// Output sort.
        pub output: SortExpr,
    }

    impl Op {
        fn binders(&self) -> Vec<(Name, SortExpr)> {
            self.inputs
                .iter()
                .map(|(name, sort, _)| (name.clone(), sort.clone()))
                .collect()
        }

        fn explicit_arity(&self) -> usize {
            self.inputs
                .iter()
                .filter(|(_, _, implicit)| *implicit == Implicit::No)
                .count()
        }
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
        /// Parent theory names, preserved by registry replay and empty in a
        /// freshly composed result.
        pub extends: Vec<Name>,
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
        /// Two symbols of one kind with one name.
        DuplicateSymbol(SymKind, Name),
        /// Two binders of one telescope with one name.
        DuplicateBinder(Site, Name),
    }

    impl std::fmt::Display for Term {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Var(name) => f.write_str(name),
                Self::App(operation, arguments) => {
                    write!(f, "{operation}(")?;
                    for (index, argument) in arguments.iter().enumerate() {
                        if index > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{argument}")?;
                    }
                    f.write_str(")")
                }
                Self::Hole(Some(name)) => write!(f, "?{name}"),
                Self::Hole(None) => f.write_str("?"),
                Self::Let(name, bound, body) => write!(f, "let {name} = {bound} in {body}"),
                Self::Case(scrutinee, branches) => {
                    write!(f, "case {scrutinee} of ")?;
                    for (index, branch) in branches.iter().enumerate() {
                        if index > 0 {
                            f.write_str(" | ")?;
                        }
                        write!(f, "{}(", branch.constructor)?;
                        for (binder_index, binder) in branch.binders.iter().enumerate() {
                            if binder_index > 0 {
                                f.write_str(", ")?;
                            }
                            f.write_str(binder)?;
                        }
                        write!(f, ") => {}", branch.body)?;
                    }
                    f.write_str(" end")
                }
            }
        }
    }

    impl std::fmt::Display for SortExpr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Name(name) => f.write_str(name),
                Self::App(name, arguments) => {
                    write!(f, "{name}(")?;
                    for (index, argument) in arguments.iter().enumerate() {
                        if index > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{argument}")?;
                    }
                    f.write_str(")")
                }
            }
        }
    }

    fn find_sort<'a>(t: &'a Theory, name: &str) -> Option<&'a Sort> {
        t.sorts.iter().rev().find(|sort| sort.name == name)
    }

    fn find_op<'a>(t: &'a Theory, name: &str) -> Option<&'a Op> {
        t.ops.iter().rev().find(|operation| operation.name == name)
    }

    fn sort_arguments(sort: &SortExpr) -> &[Term] {
        match sort {
            SortExpr::Name(_) => &[],
            SortExpr::App(_, arguments) => arguments,
        }
    }

    fn sorts_agree(
        left: &SortExpr,
        left_binders: &[Name],
        right: &SortExpr,
        right_binders: &[Name],
    ) -> bool {
        let id = |name: &str| name.to_owned();
        name_free_sort(left_binders, left, &id, &id)
            == name_free_sort(right_binders, right, &id, &id)
    }

    fn substitute_term_map(term: &Term, substitution: &BTreeMap<Name, Term>) -> Term {
        let binders = substitution
            .keys()
            .cloned()
            .map(|name| (name, SortExpr::Name(String::new())))
            .collect::<Vec<_>>();
        let arguments = substitution.values().cloned().collect::<Vec<_>>();
        subst_term(term, &binders, &arguments)
    }

    fn substitute_sort_map(sort: &SortExpr, substitution: &BTreeMap<Name, Term>) -> SortExpr {
        let binders = substitution
            .keys()
            .cloned()
            .map(|name| (name, SortExpr::Name(String::new())))
            .collect::<Vec<_>>();
        let arguments = substitution.values().cloned().collect::<Vec<_>>();
        subst_sort(sort, &binders, &arguments)
    }

    fn occurs_in(name: &str, term: &Term) -> bool {
        match term {
            Term::Var(variable) => variable == name,
            Term::App(_, arguments) => arguments.iter().any(|argument| occurs_in(name, argument)),
            Term::Hole(_) => false,
            Term::Let(binder, bound, body) => {
                occurs_in(name, bound) || (binder != name && occurs_in(name, body))
            }
            Term::Case(scrutinee, branches) => {
                occurs_in(name, scrutinee)
                    || branches.iter().any(|branch| {
                        !branch.binders.iter().any(|binder| binder == name)
                            && occurs_in(name, &branch.body)
                    })
            }
        }
    }

    fn unify_terms(mut equalities: Vec<(Term, Term)>) -> Result<BTreeMap<Name, Term>, String> {
        let mut substitution = BTreeMap::new();
        while let Some((left, right)) = equalities.pop() {
            let left = substitute_term_map(&left, &substitution);
            let right = substitute_term_map(&right, &substitution);
            match (left, right) {
                (Term::Var(left), Term::Var(right)) if left == right => {}
                (Term::Var(variable), term) | (term, Term::Var(variable)) => {
                    if occurs_in(&variable, &term) {
                        return Err(format!("occurs check failed: {variable} in {term}"));
                    }
                    for value in substitution.values_mut() {
                        *value = substitute_term_map(
                            value,
                            &BTreeMap::from([(variable.clone(), term.clone())]),
                        );
                    }
                    substitution.insert(variable, term);
                }
                (Term::App(left, left_args), Term::App(right, right_args))
                    if left == right && left_args.len() == right_args.len() =>
                {
                    equalities.extend(left_args.into_iter().zip(right_args));
                }
                (left, right) => {
                    return Err(format!("cannot unify {left} with {right}"));
                }
            }
        }
        Ok(substitution)
    }

    fn unify_sorts(left: &SortExpr, right: &SortExpr) -> Result<BTreeMap<Name, Term>, String> {
        if left.head() != right.head() || sort_arguments(left).len() != sort_arguments(right).len()
        {
            return Err(format!("cannot unify {left} with {right}"));
        }
        unify_terms(
            sort_arguments(left)
                .iter()
                .cloned()
                .zip(sort_arguments(right).iter().cloned())
                .collect(),
        )
    }

    fn application_sort(
        t: &Theory,
        ctx: &[(Name, SortExpr)],
        operation: &Op,
        arguments: &[Term],
    ) -> Result<SortExpr, String> {
        let has_implicit = operation
            .inputs
            .iter()
            .any(|(_, _, implicit)| *implicit == Implicit::Yes);
        let expected_arity = if has_implicit {
            operation.explicit_arity()
        } else {
            operation.inputs.len()
        };
        if arguments.len() != expected_arity {
            return Err(format!(
                "arity mismatch for operation {}: expected {expected_arity} args, got {}",
                operation.name,
                arguments.len()
            ));
        }

        let mut substitution = BTreeMap::new();
        for (index, (name, _, implicit)) in operation.inputs.iter().enumerate() {
            if *implicit == Implicit::Yes {
                substitution.insert(name.clone(), Term::Var(format!("?{name}_{index}")));
            }
        }
        let mut explicit = arguments.iter();
        let mut explicit_index = 0;
        for (name, declared, implicit) in &operation.inputs {
            if *implicit == Implicit::Yes {
                continue;
            }
            let Some(argument) = explicit.next() else {
                return Err(format!(
                    "arity mismatch for operation {}: expected {expected_arity} args, got {}",
                    operation.name,
                    arguments.len()
                ));
            };
            let found = infer_term(t, ctx, argument)?;
            let expected = substitute_sort_map(declared, &substitution);
            if has_implicit {
                if expected.head() != found.head()
                    || sort_arguments(&expected).len() != sort_arguments(&found).len()
                {
                    return Err(format!(
                        "type mismatch for {} argument 0: expected {expected}, got {found}",
                        operation.name
                    ));
                }
                let unifier = unify_sorts(&expected, &found).map_err(|reason| {
                    format!(
                        "sort unification failure: implicit inference for {}: {reason}",
                        operation.name
                    )
                })?;
                for value in substitution.values_mut() {
                    *value = substitute_term_map(value, &unifier);
                }
            } else if !sorts_agree(&found, &[], &expected, &[]) {
                return Err(format!(
                    "type mismatch for {} argument {explicit_index}: expected {expected}, got {found}",
                    operation.name,
                ));
            }
            substitution.insert(name.clone(), argument.clone());
            explicit_index += 1;
        }
        Ok(substitute_sort_map(&operation.output, &substitution))
    }

    fn constructor_rename_apart(operation: &Op) -> BTreeMap<Name, Term> {
        operation
            .inputs
            .iter()
            .map(|(name, _, _)| (name.clone(), Term::Var(format!("{name}'"))))
            .collect()
    }

    fn constructor_admitted(operation: &Op, scrutinee_sort: &SortExpr) -> bool {
        let output = substitute_sort_map(&operation.output, &constructor_rename_apart(operation));
        unify_sorts(&output, scrutinee_sort).is_ok()
    }

    struct BranchRefinement {
        substitution: BTreeMap<Name, Term>,
        binder_sorts: Vec<(Name, SortExpr)>,
    }

    fn branch_refinement(
        operation: &Op,
        binders: &[Name],
        scrutinee: &Term,
        scrutinee_sort: &SortExpr,
    ) -> Result<BranchRefinement, String> {
        if operation.inputs.len() != binders.len() {
            return Err(format!(
                "operation {} expects {} branch binders but has {}",
                operation.name,
                operation.inputs.len(),
                binders.len()
            ));
        }
        let fresh = constructor_rename_apart(operation);
        let output = substitute_sort_map(&operation.output, &fresh);
        let refinement = unify_sorts(&output, scrutinee_sort)?;
        let to_binder = operation
            .inputs
            .iter()
            .zip(binders)
            .map(|((parameter, _, _), binder)| (format!("{parameter}'"), Term::Var(binder.clone())))
            .collect::<BTreeMap<_, _>>();
        let binder_sorts = operation
            .inputs
            .iter()
            .zip(binders)
            .map(|((_, declared, _), binder)| {
                let declared = substitute_sort_map(declared, &fresh);
                let declared = substitute_sort_map(&declared, &refinement);
                (binder.clone(), substitute_sort_map(&declared, &to_binder))
            })
            .collect();
        let mut substitution = refinement
            .into_iter()
            .map(|(key, value)| {
                let key = match to_binder.get(&key) {
                    Some(Term::Var(binder)) => binder.clone(),
                    _ => key,
                };
                (key, substitute_term_map(&value, &to_binder))
            })
            .collect::<BTreeMap<_, _>>();
        if let Term::Var(name) = scrutinee {
            let pattern = Term::App(
                operation.name.clone(),
                operation
                    .inputs
                    .iter()
                    .zip(binders)
                    .filter(|((_, _, implicit), _)| *implicit == Implicit::No)
                    .map(|(_, binder)| Term::Var(binder.clone()))
                    .collect(),
            );
            let single = BTreeMap::from([(name.clone(), pattern.clone())]);
            for value in substitution.values_mut() {
                *value = substitute_term_map(value, &single);
            }
            substitution.insert(name.clone(), pattern);
        }
        Ok(BranchRefinement {
            substitution,
            binder_sorts,
        })
    }

    fn infer_case_with_expected(
        t: &Theory,
        ctx: &[(Name, SortExpr)],
        scrutinee: &Term,
        branches: &[CaseBranch],
        expected: Option<&SortExpr>,
    ) -> Result<SortExpr, String> {
        let scrutinee_sort = infer_term(t, ctx, scrutinee)?;
        let declaration = find_sort(t, scrutinee_sort.head())
            .ok_or_else(|| format!("sort not found: {}", scrutinee_sort.head()))?;
        let constructors = declaration.constructors.as_ref().ok_or_else(|| {
            format!(
                "case scrutinee has open sort {}; pattern matching requires a closed sort",
                scrutinee_sort.head()
            )
        })?;
        let mut admitted = Vec::new();
        for constructor in constructors {
            let operation = find_op(t, constructor)
                .ok_or_else(|| format!("operation not found: {constructor}"))?;
            if constructor_admitted(operation, &scrutinee_sort) {
                admitted.push(constructor.clone());
            }
        }
        let mut seen = BTreeSet::new();
        for branch in branches {
            if !constructors.contains(&branch.constructor) {
                return Err(format!(
                    "case on sort {} references unknown constructor {}",
                    scrutinee_sort.head(),
                    branch.constructor
                ));
            }
            if !admitted.contains(&branch.constructor) {
                return Err(format!(
                    "case on sort {} has an unreachable branch for constructor {}",
                    scrutinee_sort.head(),
                    branch.constructor
                ));
            }
            if !seen.insert(branch.constructor.clone()) {
                return Err(format!(
                    "case on sort {} has a redundant branch for constructor {}",
                    scrutinee_sort.head(),
                    branch.constructor
                ));
            }
        }
        let missing = admitted
            .iter()
            .filter(|constructor| !seen.contains(*constructor))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "case on sort {} missing branches for: {missing:?}",
                scrutinee_sort.head()
            ));
        }
        let mut result = None;
        for branch in branches {
            let operation = find_op(t, &branch.constructor)
                .ok_or_else(|| format!("operation not found: {}", branch.constructor))?;
            let refinement =
                branch_refinement(operation, &branch.binders, scrutinee, &scrutinee_sort)?;
            let mut inner = ctx.to_vec();
            inner.extend(refinement.binder_sorts);
            if let Some(motive) = expected {
                let refined = substitute_sort_map(motive, &refinement.substitution);
                let found = infer_term_with_expected(t, &inner, &branch.body, Some(&refined))?;
                if !sorts_agree(&found, &[], &refined, &[]) {
                    return Err(format!(
                        "case branch `{}` has sort {found}, expected {refined} under this branch's refinement",
                        branch.constructor
                    ));
                }
                result = Some(motive.clone());
                continue;
            }
            let found = infer_term_with_expected(t, &inner, &branch.body, None)?;
            if result
                .as_ref()
                .is_some_and(|expected| !sorts_agree(expected, &[], &found, &[]))
            {
                return Err(format!("case branches have different sorts: {found}"));
            }
            result = Some(found);
        }
        result.ok_or_else(|| {
            format!(
                "case scrutinee sort {scrutinee_sort} admits no constructor of {}",
                scrutinee_sort.head()
            )
        })
    }

    /// Infer a term's sort under the strict production judgement.
    fn infer_term_with_expected(
        t: &Theory,
        ctx: &[(Name, SortExpr)],
        term: &Term,
        expected: Option<&SortExpr>,
    ) -> Result<SortExpr, String> {
        match term {
            Term::Var(x) => ctx
                .iter()
                .rev()
                .find(|(n, _)| n == x)
                .map(|(_, s)| s.clone())
                .ok_or_else(|| format!("unbound variable: {x}")),
            Term::App(o, args) => {
                let operation = find_op(t, o).ok_or_else(|| format!("operation not found: {o}"))?;
                application_sort(t, ctx, operation, args)
            }
            Term::Hole(name) => Ok(SortExpr::Name(format!(
                "?{}",
                name.as_deref().unwrap_or("hole")
            ))),
            Term::Let(x, bound, body) => {
                let mut inner = ctx.to_vec();
                inner.push((x.clone(), infer_term(t, ctx, bound)?));
                infer_term_with_expected(t, &inner, body, expected)
            }
            Term::Case(scrutinee, branches) => {
                infer_case_with_expected(t, ctx, scrutinee, branches, expected)
            }
        }
    }

    fn infer_term(t: &Theory, ctx: &[(Name, SortExpr)], term: &Term) -> Result<SortExpr, String> {
        infer_term_with_expected(t, ctx, term, None)
    }

    fn collect_application_constraints(
        t: &Theory,
        name: &str,
        args: &[Term],
        bound: &BTreeSet<Name>,
        ctx: &mut BTreeMap<Name, SortExpr>,
        equalities: &mut Vec<(Term, Term)>,
        next_implicit_scope: &mut usize,
    ) -> Result<(), String> {
        let op = find_op(t, name).ok_or_else(|| format!("operation {name} is not declared"))?;
        if args.len() != op.explicit_arity() {
            return Err(format!(
                "arity mismatch for operation {name}: expected {} args, got {}",
                op.explicit_arity(),
                args.len()
            ));
        }
        let mut substitution = BTreeMap::new();
        let scope = *next_implicit_scope;
        *next_implicit_scope += 1;
        for (index, (parameter, _, implicit)) in op.inputs.iter().enumerate() {
            if *implicit == Implicit::Yes {
                substitution.insert(
                    parameter.clone(),
                    Term::Var(format!("?{name}_{parameter}_{scope}_{index}")),
                );
            }
        }
        let mut explicit = args.iter();
        for (parameter, declared, implicit) in &op.inputs {
            if *implicit == Implicit::Yes {
                continue;
            }
            let Some(arg) = explicit.next() else {
                return Err(format!(
                    "arity mismatch for operation {name}: expected {} args, got {}",
                    op.explicit_arity(),
                    args.len()
                ));
            };
            let expected = substitute_sort_map(declared, &substitution);
            if let Term::Var(variable) = arg
                && !bound.contains(variable)
            {
                if let Some(previous) = ctx.get(variable) {
                    if previous.head() != expected.head()
                        || sort_arguments(previous).len() != sort_arguments(&expected).len()
                    {
                        return Err(format!(
                            "variable {variable} is constrained at both {previous:?} and {expected:?}"
                        ));
                    }
                    equalities.extend(
                        sort_arguments(previous)
                            .iter()
                            .cloned()
                            .zip(sort_arguments(&expected).iter().cloned()),
                    );
                } else {
                    ctx.insert(variable.clone(), expected);
                }
            }
            collect_equation_constraints(t, arg, bound, ctx, equalities, next_implicit_scope)?;
            substitution.insert(parameter.clone(), arg.clone());
        }
        Ok(())
    }

    fn collect_equation_constraints(
        t: &Theory,
        term: &Term,
        bound: &BTreeSet<Name>,
        ctx: &mut BTreeMap<Name, SortExpr>,
        equalities: &mut Vec<(Term, Term)>,
        next_implicit_scope: &mut usize,
    ) -> Result<(), String> {
        match term {
            Term::Var(_) => Ok(()),
            Term::Hole(_) => Err("an equation contains a hole".to_owned()),
            Term::App(name, args) => collect_application_constraints(
                t,
                name,
                args,
                bound,
                ctx,
                equalities,
                next_implicit_scope,
            ),
            Term::Let(name, value, body) => {
                collect_equation_constraints(
                    t,
                    value,
                    bound,
                    ctx,
                    equalities,
                    next_implicit_scope,
                )?;
                let mut inner = bound.clone();
                inner.insert(name.clone());
                collect_equation_constraints(t, body, &inner, ctx, equalities, next_implicit_scope)
            }
            Term::Case(scrutinee, branches) => {
                collect_equation_constraints(
                    t,
                    scrutinee,
                    bound,
                    ctx,
                    equalities,
                    next_implicit_scope,
                )?;
                for branch in branches {
                    let mut inner = bound.clone();
                    inner.extend(branch.binders.iter().cloned());
                    collect_equation_constraints(
                        t,
                        &branch.body,
                        &inner,
                        ctx,
                        equalities,
                        next_implicit_scope,
                    )?;
                }
                Ok(())
            }
        }
    }

    /// Infer the sorts of an equation's free variables from operation-input
    /// positions, then require both sides to be fully well typed at one sort.
    fn equation_well_typed(t: &Theory, lhs: &Term, rhs: &Term) -> Result<(), String> {
        let mut inferred = BTreeMap::new();
        let mut equalities = Vec::new();
        let mut next_implicit_scope = 0;
        collect_equation_constraints(
            t,
            lhs,
            &BTreeSet::new(),
            &mut inferred,
            &mut equalities,
            &mut next_implicit_scope,
        )?;
        collect_equation_constraints(
            t,
            rhs,
            &BTreeSet::new(),
            &mut inferred,
            &mut equalities,
            &mut next_implicit_scope,
        )?;
        let unifier = unify_terms(equalities)?;
        for sort in inferred.values_mut() {
            *sort = substitute_sort_map(sort, &unifier);
        }
        let ctx: Vec<(Name, SortExpr)> = inferred.into_iter().collect();
        let (left, right) = if matches!(lhs, Term::Case(..)) && !matches!(rhs, Term::Case(..)) {
            let right = infer_term(t, &ctx, rhs)
                .map_err(|error| format!("the right side is not well typed: {error}"))?;
            let left = infer_term_with_expected(t, &ctx, lhs, Some(&right))
                .map_err(|error| format!("the left side is not well typed: {error}"))?;
            (left, right)
        } else {
            let left = infer_term(t, &ctx, lhs)
                .map_err(|error| format!("the left side is not well typed: {error}"))?;
            let right = infer_term_with_expected(t, &ctx, rhs, Some(&left))
                .map_err(|error| format!("the right side is not well typed: {error}"))?;
            (left, right)
        };
        if sorts_agree(&left, &[], &right, &[]) {
            Ok(())
        } else {
            Err(format!(
                "the equation relates terms of different sorts: {left:?} and {right:?}"
            ))
        }
    }

    fn sort_mentions_var(sort: &SortExpr, name: &str) -> bool {
        sort_arguments(sort)
            .iter()
            .any(|argument| occurs_in(name, argument))
    }

    fn typecheck_theory_body(theory: &Theory) -> Result<(), String> {
        for operation in &theory.ops {
            for (parameter, _, implicit) in &operation.inputs {
                if *implicit == Implicit::Yes
                    && !operation.inputs.iter().any(|(_, sort, other)| {
                        *other == Implicit::No && sort_mentions_var(sort, parameter)
                    })
                    && !sort_mentions_var(&operation.output, parameter)
                {
                    return Err(format!(
                        "implicit parameter {parameter} of operation {} cannot be inferred",
                        operation.name
                    ));
                }
            }
        }

        let mut defined = BTreeSet::new();
        for lhs in theory
            .eqs
            .iter()
            .map(|equation| &equation.lhs)
            .chain(theory.directed_eqs.iter().map(|equation| &equation.lhs))
        {
            let Term::App(operation_name, arguments) = lhs else {
                continue;
            };
            let Some(operation) = find_op(theory, operation_name) else {
                continue;
            };
            if arguments.len() != operation.explicit_arity() {
                continue;
            }
            let mut variables = BTreeSet::new();
            if arguments.iter().all(|argument| match argument {
                Term::Var(name) => variables.insert(name.clone()),
                _ => false,
            }) {
                defined.insert(operation_name.clone());
            }
        }
        for sort in &theory.sorts {
            let Some(constructors) = &sort.constructors else {
                continue;
            };
            let constructor_set = constructors.iter().collect::<BTreeSet<_>>();
            for constructor in constructors {
                let operation = find_op(theory, constructor).ok_or_else(|| {
                    format!(
                        "closed sort {} names missing constructor {constructor}",
                        sort.name
                    )
                })?;
                if operation.output.head() != sort.name {
                    return Err(format!(
                        "constructor {constructor} has output {}, expected {}",
                        operation.output, sort.name
                    ));
                }
            }
            for operation in &theory.ops {
                if operation.output.head() == sort.name
                    && !constructor_set.contains(&operation.name)
                    && !defined.contains(&operation.name)
                {
                    return Err(format!(
                        "operation {} produces closed sort {} but is neither a constructor nor defined",
                        operation.name, sort.name
                    ));
                }
            }
        }
        for equation in &theory.eqs {
            equation_well_typed(theory, &equation.lhs, &equation.rhs)?;
        }
        Ok(())
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
        let Some(sort) = declared.iter().rev().find(|d| d.name == s.head()) else {
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
            let mut variables = BTreeSet::new();
            free_vars(a, &mut variables);
            let unbound = variables
                .into_iter()
                .filter(|variable| !ctx.iter().any(|(name, _)| name == variable))
                .collect::<Vec<_>>();
            if !unbound.is_empty() {
                out.extend(
                    unbound
                        .into_iter()
                        .map(|variable| Defect::UnboundParamVar(site.clone(), variable)),
                );
                continue;
            }
            let expected = subst_sort(&sort.params[i].1, &sort.params[..i], &args[..i]);
            match infer_term(t, ctx, a) {
                Ok(found) if !sorts_agree(&found, &[], &expected, &[]) => {
                    out.push(Defect::ParamIllTyped(
                        site.clone(),
                        sort.name.clone(),
                        i,
                        expected.to_string(),
                        found.to_string(),
                    ));
                }
                Err(error) => out.push(Defect::ParamIllTyped(
                    site.clone(),
                    sort.name.clone(),
                    i,
                    expected.to_string(),
                    format!("no sort ({error})"),
                )),
                Ok(_) => {}
            }
        }
    }

    fn duplicate_symbols(t: &Theory) -> Vec<Defect> {
        fn duplicates<'a>(
            kind: SymKind,
            names: impl Iterator<Item = &'a Name>,
            out: &mut Vec<Defect>,
        ) {
            let mut seen = BTreeSet::new();
            let mut repeated = BTreeSet::new();
            for name in names {
                if !seen.insert(name) {
                    repeated.insert(name);
                }
            }
            out.extend(
                repeated
                    .into_iter()
                    .map(|name| Defect::DuplicateSymbol(kind, name.clone())),
            );
        }

        let mut out = Vec::new();
        duplicates(
            SymKind::Sort,
            t.sorts.iter().map(|sort| &sort.name),
            &mut out,
        );
        duplicates(SymKind::Op, t.ops.iter().map(|op| &op.name), &mut out);
        duplicates(
            SymKind::Eq,
            t.eqs.iter().map(|equation| &equation.name),
            &mut out,
        );
        duplicates(
            SymKind::DirectedEq,
            t.directed_eqs.iter().map(|equation| &equation.name),
            &mut out,
        );
        duplicates(
            SymKind::Policy,
            t.policies.iter().map(|policy| &policy.name),
            &mut out,
        );
        out
    }

    /// The declarations admitted by the sort-parameter dependency order.
    /// Vector order is deliberately irrelevant; cycles and their dependants
    /// never enter the fixed point.
    fn sorts_in_dependency_order(t: &Theory) -> BTreeSet<Name> {
        let mut ordered = BTreeSet::new();
        loop {
            let before = ordered.len();
            for sort in &t.sorts {
                if ordered.contains(&sort.name) {
                    continue;
                }
                let ready = sort.params.iter().all(|(_, parameter_sort)| {
                    let head = parameter_sort.head();
                    head != sort.name
                        && (t.sorts.iter().all(|candidate| candidate.name != head)
                            || ordered.contains(head))
                });
                if ready {
                    ordered.insert(sort.name.clone());
                }
            }
            if ordered.len() == before {
                return ordered;
            }
        }
    }

    /// Definition: Signature well-formedness.
    ///
    /// Every sort parameter's sort is declared earlier in dependency order;
    /// every sort expression of an operation names a declared sort, an
    /// application has the declared arity and arguments typed against the
    /// parameters (with substitution), and its variables are bound by earlier
    /// inputs. Binder names are pairwise distinct within each telescope, and
    /// sort, operation, equation, directed-equation and policy names are
    /// pairwise distinct within their respective namespaces.
    ///
    /// # Errors
    ///
    /// Every defect in contract order: duplicate namespaces, then sort
    /// declarations, then operation declarations.
    ///
    /// Book anchors: `L-theories-1`, `L-theories-2`.
    pub fn typecheck_signature(t: &Theory) -> Result<(), Vec<Defect>> {
        let mut out = duplicate_symbols(t);
        let ordered = sorts_in_dependency_order(t);
        let all: Vec<&Sort> = t.sorts.iter().collect();
        for s in &t.sorts {
            for (i, (p, ps)) in s.params.iter().enumerate() {
                let site = Site::SortParam(s.name.clone(), i);
                if s.params[..i].iter().any(|(q, _)| q == p) {
                    out.push(Defect::DuplicateBinder(site.clone(), p.clone()));
                }
                if t.sorts.iter().any(|declared| declared.name == ps.head())
                    && !ordered.contains(ps.head())
                {
                    out.push(Defect::UndeclaredSort(site, ps.head().to_owned()));
                } else {
                    check_expr(t, &all, &s.params[..i], &site, ps, &mut out);
                }
            }
        }
        for o in &t.ops {
            for (i, (x, xs, _)) in o.inputs.iter().enumerate() {
                let site = Site::OpInput(o.name.clone(), i);
                if o.inputs[..i].iter().any(|(q, _, _)| q == x) {
                    out.push(Defect::DuplicateBinder(site.clone(), x.clone()));
                }
                let context = o.inputs[..i]
                    .iter()
                    .map(|(name, sort, _)| (name.clone(), sort.clone()))
                    .collect::<Vec<_>>();
                check_expr(t, &all, &context, &site, xs, &mut out);
            }
            let context = o
                .inputs
                .iter()
                .map(|(name, sort, _)| (name.clone(), sort.clone()))
                .collect::<Vec<_>>();
            check_expr(
                t,
                &all,
                &context,
                &Site::OpOutput(o.name.clone()),
                &o.output,
                &mut out,
            );
        }
        if out.is_empty() { Ok(()) } else { Err(out) }
    }

    /// An operation's image under a morphism.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum OpImage {
        /// An operation.
        Op(Name),
        /// A term over the operation's input names.
        Term(Term),
    }

    /// A theory morphism: total maps on sorts and operations.
    #[derive(Clone, Debug, PartialEq, Eq, Default, PartialOrd, Ord)]
    pub struct Morphism {
        /// Morphism name.
        pub name: Name,
        /// Domain theory name.
        pub domain: Name,
        /// Codomain theory name.
        pub codomain: Name,
        /// Sort map.
        pub sorts: BTreeMap<Name, Name>,
        /// Operation map.
        pub ops: BTreeMap<Name, OpImage>,
    }

    /// The first semantic reason a candidate theory morphism is refused.
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum MorphismError {
        /// A declared domain sort has no assignment.
        MissingSortMapping(Name),
        /// A sort assignment names no codomain declaration.
        SortNotFound(Name),
        /// A mapped sort has a different arity.
        SortArityMismatch {
            /// Domain sort.
            sort: Name,
            /// Domain arity.
            expected: usize,
            /// Codomain arity.
            got: usize,
        },
        /// A mapped sort has another kind.
        SortKindMismatch {
            /// Domain sort.
            sort: Name,
            /// Domain kind.
            expected: Name,
            /// Codomain kind.
            got: Name,
        },
        /// One mapped sort parameter has another name-free sort.
        SortParamMismatch {
            /// Domain sort.
            sort: Name,
            /// Parameter position.
            index: usize,
            /// Mapped domain parameter sort.
            expected: Name,
            /// Codomain parameter sort.
            got: Name,
        },
        /// A closed sort's constructor image set differs.
        ClosureMismatch {
            /// Domain sort.
            sort: Name,
            /// Constructor image set.
            expected: Vec<Name>,
            /// Codomain constructor set.
            got: Vec<Name>,
        },
        /// A declared domain operation has no assignment.
        MissingOpMapping(Name),
        /// An operation assignment names no codomain operation.
        OpNotFound(Name),
        /// A renamed operation has an incompatible signature.
        OpTypeMismatch {
            /// Domain operation.
            op: Name,
            /// Exact mismatch.
            detail: Name,
        },
        /// A derived term names an unbound variable.
        TermAssignmentUnboundVar {
            /// Domain operation.
            op: Name,
            /// Unbound variable.
            var: Name,
        },
        /// A derived term is not typeable.
        TermAssignmentIllTyped {
            /// Domain operation.
            op: Name,
            /// Exact type error.
            detail: Name,
        },
        /// A derived term has another output sort.
        TermAssignmentSortMismatch {
            /// Domain operation.
            op: Name,
            /// Mapped output sort.
            expected: Name,
            /// Inferred term sort.
            got: Name,
        },
        /// An ordinary equation's image is not derivable.
        EquationNotPreserved(Name),
        /// Equation preservation exhausted the fixed morphism budget.
        EquationPreservationUnknown {
            /// Equation name.
            equation: Name,
            /// Consumed steps.
            steps: u32,
        },
        /// A directed equation has no directed image.
        DirectedEquationNotPreserved(Name),
        /// A source implicit sort maps to a name the target neither declares
        /// nor uses.
        ImplicitSortImageUnknown {
            /// Source implicit sort name.
            source: Name,
            /// Target image name.
            target: Name,
        },
    }

    /// A symbol of a diagram node.
    #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct SymRef {
        /// Node label.
        pub node: Name,
        /// Symbol name.
        pub name: Name,
    }

    /// How symbols of different nodes are identified.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CompositionMode {
        /// Legs and same names identify.
        Amalgamate,
        /// Only legs identify.
        ExplicitIdentity,
    }

    /// Compatibility name retained for the original finite test fixtures.
    pub type Mode = CompositionMode;

    /// One labelled theory in a finite diagram.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct DiagramNode {
        /// Stable endpoint label.
        pub label: Name,
        /// Theory at the endpoint.
        pub theory: Theory,
    }

    /// One labelled morphism in a finite diagram.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct DiagramEdge {
        /// Domain node label.
        pub src: Name,
        /// Codomain node label.
        pub tgt: Name,
        /// Whole theory morphism.
        pub morphism: Morphism,
    }

    /// A finite diagram whose edge endpoints are node labels, not positions.
    #[derive(Clone, Debug, PartialEq, Eq, Default)]
    pub struct TheoryDiagram {
        /// Diagram nodes.
        pub nodes: Vec<DiagramNode>,
        /// Diagram edges.
        pub edges: Vec<DiagramEdge>,
    }

    impl TheoryDiagram {
        /// A discrete diagram (finite coproduct input).
        #[must_use]
        pub fn discrete(nodes: Vec<(Name, Theory)>) -> Self {
            Self {
                nodes: nodes
                    .into_iter()
                    .map(|(label, theory)| DiagramNode { label, theory })
                    .collect(),
                edges: Vec::new(),
            }
        }

        /// A span `left <- shared -> right`.
        #[must_use]
        pub fn span(
            shared: (Name, Theory),
            left: (Name, Theory),
            right: (Name, Theory),
            i1: Morphism,
            i2: Morphism,
        ) -> Self {
            let shared_label = shared.0.clone();
            let left_label = left.0.clone();
            let right_label = right.0.clone();
            Self {
                nodes: vec![
                    DiagramNode {
                        label: shared.0,
                        theory: shared.1,
                    },
                    DiagramNode {
                        label: left.0,
                        theory: left.1,
                    },
                    DiagramNode {
                        label: right.0,
                        theory: right.1,
                    },
                ],
                edges: vec![
                    DiagramEdge {
                        src: shared_label.clone(),
                        tgt: left_label,
                        morphism: i1,
                    },
                    DiagramEdge {
                        src: shared_label,
                        tgt: right_label,
                        morphism: i2,
                    },
                ],
            }
        }

        /// Two parallel arrows `src -> tgt` (finite coequalizer input).
        #[must_use]
        pub fn parallel(
            src: (Name, Theory),
            tgt: (Name, Theory),
            first: Morphism,
            second: Morphism,
        ) -> Self {
            let src_label = src.0.clone();
            let tgt_label = tgt.0.clone();
            Self {
                nodes: vec![
                    DiagramNode {
                        label: src.0,
                        theory: src.1,
                    },
                    DiagramNode {
                        label: tgt.0,
                        theory: tgt.1,
                    },
                ],
                edges: vec![
                    DiagramEdge {
                        src: src_label.clone(),
                        tgt: tgt_label.clone(),
                        morphism: first,
                    },
                    DiagramEdge {
                        src: src_label,
                        tgt: tgt_label,
                        morphism: second,
                    },
                ],
            }
        }

        /// A by-name span. Every shared sort and operation must occur on both
        /// legs; the legs are identity assignments on those names.
        ///
        /// # Errors
        ///
        /// [`ComposeError::MissingSortMapping`] or
        /// [`ComposeError::MissingOpMapping`] for the first absent name.
        pub fn by_name(
            shared: (Name, Theory),
            left: (Name, Theory),
            right: (Name, Theory),
        ) -> Result<Self, ComposeError> {
            let leg = |target: &Theory, leg_label: &str| {
                let mut morphism = Morphism {
                    name: format!("{leg_label}_{}_into_{}", shared.1.name, target.name),
                    domain: shared.1.name.clone(),
                    codomain: target.name.clone(),
                    ..Morphism::default()
                };
                for sort in &shared.1.sorts {
                    if !target
                        .sorts
                        .iter()
                        .any(|candidate| candidate.name == sort.name)
                    {
                        return Err(ComposeError::MissingSortMapping(sort.name.clone()));
                    }
                    morphism.sorts.insert(sort.name.clone(), sort.name.clone());
                }
                for sort in undeclared_sorts(&shared.1) {
                    morphism.sorts.insert(sort.clone(), sort);
                }
                for operation in &shared.1.ops {
                    if !target
                        .ops
                        .iter()
                        .any(|candidate| candidate.name == operation.name)
                    {
                        return Err(ComposeError::MissingOpMapping(operation.name.clone()));
                    }
                    morphism
                        .ops
                        .insert(operation.name.clone(), OpImage::Op(operation.name.clone()));
                }
                Ok(morphism)
            };
            let left_leg = leg(&left.1, "i1")?;
            let right_leg = leg(&right.1, "i2")?;
            Ok(Self::span(shared, left, right, left_leg, right_leg))
        }
    }

    /// Every option that affects composition semantics.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ComposeOptions {
        /// Identification convention.
        pub mode: CompositionMode,
        /// Result theory name.
        pub result_name: Name,
        /// Name-clash policy.
        pub clash: ClashPolicy,
        /// Per-equation rewrite budget used by universal verification.
        pub rewrite_budget: u32,
    }

    /// What a name clash does in explicit-identity mode.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ClashPolicy {
        /// Rename every clashing class.
        Freshen,
        /// Refuse.
        Reject,
    }

    /// Why a syntactic quotient cannot be represented by this `Theory`
    /// carrier even though the colimit exists among generalized algebraic
    /// theories.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NotRepresentableReason {
        /// Identified operation fibres would require a sort-equality axiom.
        NeedsSortEquation,
    }

    /// The first difference between two members of a sort class, in the
    /// order arity, parameter sorts by position, kind, closure.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum SortMismatch {
        /// The members' arities.
        Arity {
            /// The first member's arity.
            first: usize,
            /// The second member's arity.
            second: usize,
        },
        /// The first parameter whose sort differs once every sort and
        /// operation is read through its class, with both parameter sorts
        /// as declared.
        ParamSort {
            /// The parameter's position.
            index: usize,
            /// The first member's parameter sort.
            first: Name,
            /// The second member's parameter sort.
            second: Name,
        },
        /// The members' kinds.
        Kind,
        /// The members' constructor classes.
        Closure,
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
        /// A leg is not a morphism.
        InvalidLeg {
            /// Diagram edge index.
            edge: usize,
            /// Exact morphism refusal.
            detail: MorphismError,
        },
        /// A shared sort is absent from one by-name leg.
        MissingSortMapping(Name),
        /// A shared operation is absent from one by-name leg.
        MissingOpMapping(Name),
        /// A sort class disagrees: two members and their first difference
        /// (boxed to keep the refusal small).
        SortSignatureMismatch(SymRef, SymRef, Box<SortMismatch>),
        /// An operation class disagrees.
        OpSignatureMismatch(SymRef, SymRef),
        /// An operation class agrees only modulo a sort equation, which this
        /// finite carrier cannot state.
        QuotientNotRepresentable {
            /// First class member.
            first: SymRef,
            /// Second class member.
            second: SymRef,
            /// Exact representability limit.
            reason: NotRepresentableReason,
        },
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
        /// The edge's position in the canonical edge order: ascending
        /// (source label, target label, canonical morphism bytes).
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

    /// The name-free form of a sort expression: the head through
    /// `sort_class`, each argument through [`name_free_term`]. A sort applied
    /// to no arguments reads as the bare sort name, since the two spellings
    /// denote one sort expression.
    fn name_free_sort(
        binders: &[Name],
        s: &SortExpr,
        sort_class: &dyn Fn(&str) -> Name,
        op_class: &dyn Fn(&str) -> Name,
    ) -> SortExpr {
        match s {
            SortExpr::Name(n) => SortExpr::Name(sort_class(n)),
            SortExpr::App(n, xs) if xs.is_empty() => SortExpr::Name(sort_class(n)),
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
    ) -> (Vec<(SortExpr, Implicit)>, SortExpr) {
        let names: Vec<Name> = o.inputs.iter().map(|(n, _, _)| n.clone()).collect();
        let ins = o
            .inputs
            .iter()
            .enumerate()
            .map(|(i, (_, s, implicit))| {
                (
                    name_free_sort(&names[..i], s, sort_class, op_class),
                    *implicit,
                )
            })
            .collect();
        (ins, name_free_sort(&names, &o.output, sort_class, op_class))
    }

    /// A sort's signature by class: arity, kind, constructors by operation
    /// class (as a set; `None` open) and name-free parameter sorts.
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
        let constructors = s.constructors.as_ref().map(|cs| {
            let mut mapped = cs.iter().map(|c| op_class(c)).collect::<Vec<_>>();
            mapped.sort();
            mapped.dedup();
            mapped
        });
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
                match (m.ops.get(o), find_op(src, o)) {
                    (Some(OpImage::Term(image)), Some(op)) => {
                        subst_term(image, &op.binders(), &args)
                    }
                    (Some(OpImage::Op(o2)), _) => Term::App(o2.clone(), args),
                    _ => Term::App(o.clone(), args),
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
                        constructor: constructor_image(src, m, &br.constructor)
                            .unwrap_or_else(|_| br.constructor.clone()),
                        binders: br.binders.clone(),
                        body: translate_term(src, m, &br.body),
                    })
                    .collect(),
            ),
        }
    }

    /// Definition: The whole action of a morphism on a source term.
    ///
    /// An application of an operation sent to an operation is renamed; one
    /// sent to a term is replaced by that term with the operation's inputs
    /// substituted, capture-avoiding, by the translated arguments; a case
    /// branch's constructor is replaced by its constructor image (kept when
    /// it has none); variables, holes and binders are kept.
    ///
    /// Book anchors: `L-theories-7`.
    #[must_use]
    pub fn apply_morphism(src: &Theory, m: &Morphism, t: &Term) -> Term {
        translate_term(src, m, t)
    }

    /// A source sort expression translated along a morphism: its sort renamed
    /// and its argument terms translated.
    fn translate_sort(src: &Theory, m: &Morphism, s: &SortExpr) -> SortExpr {
        let sort = |n: &str| m.sorts.get(n).cloned().unwrap_or_else(|| n.to_owned());
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
    fn constructor_image(src: &Theory, m: &Morphism, constructor: &str) -> Result<Name, Term> {
        match m.ops.get(constructor) {
            None => Ok(constructor.to_owned()),
            Some(OpImage::Op(image)) => Ok(image.clone()),
            Some(OpImage::Term(term @ Term::App(image, args))) => {
                let Some(op) = find_op(src, constructor) else {
                    return Err(term.clone());
                };
                (args.len() == op.inputs.len()
                    && args.iter().enumerate().all(|(i, arg)| {
                        matches!(arg, Term::Var(name)
                            if op.inputs.iter().rposition(|(binder, _, _)| binder == name) == Some(i))
                    }))
                .then(|| image.clone())
                .ok_or_else(|| Term::App(image.clone(), args.clone()))
            }
            Some(OpImage::Term(term)) => Err(term.clone()),
        }
    }

    fn match_rule(pattern: &Term, value: &Term, substitution: &mut BTreeMap<Name, Term>) -> bool {
        match_rule_under(pattern, value, substitution, &BTreeMap::new())
    }

    fn match_rule_under(
        pattern: &Term,
        value: &Term,
        substitution: &mut BTreeMap<Name, Term>,
        bound: &BTreeMap<Name, Name>,
    ) -> bool {
        match (pattern, value) {
            (Term::Var(name), Term::Var(value_name)) if bound.contains_key(name) => {
                bound.get(name) == Some(value_name)
            }
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
                        .all(|(left, right)| match_rule_under(left, right, substitution, bound))
            }
            (Term::Hole(left), Term::Hole(right)) => left == right,
            (
                Term::Let(left_name, left_bound, left_body),
                Term::Let(right_name, right_bound, right_body),
            ) => {
                if !match_rule_under(left_bound, right_bound, substitution, bound) {
                    return false;
                }
                let mut nested = bound.clone();
                nested.insert(left_name.clone(), right_name.clone());
                match_rule_under(left_body, right_body, substitution, &nested)
            }
            (
                Term::Case(left_scrutinee, left_branches),
                Term::Case(right_scrutinee, right_branches),
            ) => {
                match_rule_under(left_scrutinee, right_scrutinee, substitution, bound)
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
                            let mut nested = bound.clone();
                            for (left, right) in left.binders.iter().zip(&right.binders) {
                                nested.insert(left.clone(), right.clone());
                            }
                            match_rule_under(&left.body, &right.body, substitution, &nested)
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

    fn normalize_bounded(term: &Term, rules: &[(Term, Term)], budget: u32) -> Result<Term, u32> {
        fn inner(
            term: &Term,
            rules: &[(Term, Term)],
            steps: &mut u32,
            budget: u32,
            depth_limit: usize,
        ) -> Result<Term, u32> {
            let mut current = term.clone();
            loop {
                if *steps >= budget || term_depth(&current) > depth_limit {
                    return Err(*steps);
                }
                let normalized = match current {
                    Term::Var(_) | Term::Hole(_) => current,
                    Term::App(operation, arguments) => Term::App(
                        operation,
                        arguments
                            .iter()
                            .map(|argument| inner(argument, rules, steps, budget, depth_limit))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    Term::Let(name, bound, body) => {
                        let bound = inner(&bound, rules, steps, budget, depth_limit)?;
                        current =
                            subst_term(&body, &[(name, SortExpr::Name(String::new()))], &[bound]);
                        continue;
                    }
                    Term::Case(scrutinee, branches) => {
                        let scrutinee = inner(&scrutinee, rules, steps, budget, depth_limit)?;
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
                                    Ok(CaseBranch {
                                        constructor: branch.constructor.clone(),
                                        binders: branch.binders.clone(),
                                        body: inner(
                                            &branch.body,
                                            rules,
                                            steps,
                                            budget,
                                            depth_limit,
                                        )?,
                                    })
                                })
                                .collect::<Result<Vec<_>, u32>>()?,
                        )
                    }
                };
                if *steps >= budget {
                    return Err(*steps);
                }
                let Some(next) = rewrite_root(&normalized, rules) else {
                    return Ok(normalized);
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
        inner(term, rules, &mut steps, budget, depth_limit)
    }

    fn derivable_within(lhs: &Term, rhs: &Term, target: &Theory, budget: u32) -> Result<bool, u32> {
        let initial = alpha(lhs, rhs);
        if initial.0 == initial.1 {
            return Ok(true);
        }
        let directed: Vec<(Term, Term)> = target
            .directed_eqs
            .iter()
            .map(|equation| (equation.lhs.clone(), equation.rhs.clone()))
            .collect();
        let common = |rules: &[(Term, Term)]| -> Result<bool, u32> {
            let left = normalize_bounded(lhs, rules, budget)?;
            let right = normalize_bounded(rhs, rules, budget)?;
            let normalized = alpha(&left, &right);
            Ok(normalized.0 == normalized.1)
        };
        if common(&directed)? {
            return Ok(true);
        }
        let mut full = directed;
        for equation in &target.eqs {
            full.push((equation.lhs.clone(), equation.rhs.clone()));
            full.push((equation.rhs.clone(), equation.lhs.clone()));
        }
        common(&full)
    }

    /// Check every ordinary and directed equation image under the fixed
    /// morphism decision budget.
    fn equations_preserved(src: &Theory, tgt: &Theory, m: &Morphism) -> Result<(), MorphismError> {
        for equation in &src.eqs {
            let lhs = translate_term(src, m, &equation.lhs);
            let rhs = translate_term(src, m, &equation.rhs);
            let image = alpha(&lhs, &rhs);
            let direct = tgt.eqs.iter().any(|candidate| {
                alpha(&candidate.lhs, &candidate.rhs) == image
                    || alpha(&candidate.rhs, &candidate.lhs) == image
            });
            if direct {
                continue;
            }
            match derivable_within(&lhs, &rhs, tgt, 512) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(MorphismError::EquationNotPreserved(equation.name.clone()));
                }
                Err(steps) => {
                    return Err(MorphismError::EquationPreservationUnknown {
                        equation: equation.name.clone(),
                        steps,
                    });
                }
            }
        }
        for equation in &src.directed_eqs {
            let lhs = translate_term(src, m, &equation.lhs);
            let rhs = translate_term(src, m, &equation.rhs);
            let image = alpha(&lhs, &rhs);
            if !tgt
                .directed_eqs
                .iter()
                .any(|candidate| alpha(&candidate.lhs, &candidate.rhs) == image)
            {
                return Err(MorphismError::DirectedEquationNotPreserved(
                    equation.name.clone(),
                ));
            }
        }
        Ok(())
    }

    fn equations_preserved_within(
        src: &Theory,
        tgt: &Theory,
        morphism: &Morphism,
        budget: u32,
    ) -> Result<bool, u32> {
        for equation in &src.eqs {
            let lhs = translate_term(src, morphism, &equation.lhs);
            let rhs = translate_term(src, morphism, &equation.rhs);
            let image = alpha(&lhs, &rhs);
            let direct = tgt.eqs.iter().any(|candidate| {
                alpha(&candidate.lhs, &candidate.rhs) == image
                    || alpha(&candidate.rhs, &candidate.lhs) == image
            });
            if !direct && !derivable_within(&lhs, &rhs, tgt, budget)? {
                return Ok(false);
            }
        }
        let directed = src.directed_eqs.iter().all(|equation| {
            let lhs = translate_term(src, morphism, &equation.lhs);
            let rhs = translate_term(src, morphism, &equation.rhs);
            let image = alpha(&lhs, &rhs);
            tgt.directed_eqs
                .iter()
                .any(|candidate| alpha(&candidate.lhs, &candidate.rhs) == image)
        });
        Ok(directed)
    }

    fn morphism_sort_structure(
        src: &Theory,
        tgt: &Theory,
        morphism: &Morphism,
    ) -> Result<(), MorphismError> {
        let id = |x: &str| x.to_owned();
        let name_free = |binders: &[Name], sort: &SortExpr| name_free_sort(binders, sort, &id, &id);
        for sort in &src.sorts {
            let target_name = morphism
                .sorts
                .get(&sort.name)
                .ok_or_else(|| MorphismError::MissingSortMapping(sort.name.clone()))?;
            let target = find_sort(tgt, target_name)
                .ok_or_else(|| MorphismError::SortNotFound(target_name.clone()))?;
            if sort.params.len() != target.params.len() {
                return Err(MorphismError::SortArityMismatch {
                    sort: sort.name.clone(),
                    expected: sort.params.len(),
                    got: target.params.len(),
                });
            }
            if sort.kind != target.kind {
                return Err(MorphismError::SortKindMismatch {
                    sort: sort.name.clone(),
                    expected: sort.kind.clone(),
                    got: target.kind.clone(),
                });
            }
            let names: Vec<Name> = sort.params.iter().map(|(name, _)| name.clone()).collect();
            let target_names: Vec<Name> =
                target.params.iter().map(|(name, _)| name.clone()).collect();
            for (index, ((_, parameter), (_, target_parameter))) in
                sort.params.iter().zip(&target.params).enumerate()
            {
                let mapped = translate_sort(src, morphism, parameter);
                if name_free(&names[..index], &mapped)
                    != name_free(&target_names[..index], target_parameter)
                {
                    return Err(MorphismError::SortParamMismatch {
                        sort: sort.name.clone(),
                        index,
                        expected: mapped.to_string(),
                        got: target_parameter.to_string(),
                    });
                }
            }
            if let Some(constructors) = &sort.constructors {
                let mut expected = Vec::new();
                for constructor in constructors {
                    match constructor_image(src, morphism, constructor) {
                        Ok(image) => expected.push(image),
                        Err(term) => expected.push(term.to_string()),
                    }
                }
                expected.sort();
                expected.dedup();
                let mut got = target.constructors.clone().unwrap_or_default();
                got.sort();
                got.dedup();
                if expected != got {
                    return Err(MorphismError::ClosureMismatch {
                        sort: sort.name.clone(),
                        expected,
                        got,
                    });
                }
            }
        }
        Ok(())
    }

    fn check_named_op_image(
        operation: &Op,
        target: &Op,
        names: &[Name],
        inputs: &[(Name, SortExpr, Implicit)],
        output: &SortExpr,
    ) -> Result<(), MorphismError> {
        let id = |x: &str| x.to_owned();
        let name_free = |binders: &[Name], sort: &SortExpr| name_free_sort(binders, sort, &id, &id);
        if operation.inputs.len() != target.inputs.len() {
            return Err(MorphismError::OpTypeMismatch {
                op: operation.name.clone(),
                detail: format!(
                    "arity mismatch: domain has {} inputs, codomain has {}",
                    operation.inputs.len(),
                    target.inputs.len()
                ),
            });
        }
        let target_names = target
            .inputs
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect::<Vec<_>>();
        for (index, ((_, mapped_sort, _), (_, target_sort, _))) in
            inputs.iter().zip(&target.inputs).enumerate()
        {
            if name_free(&names[..index], mapped_sort)
                != name_free(&target_names[..index], target_sort)
            {
                return Err(MorphismError::OpTypeMismatch {
                    op: operation.name.clone(),
                    detail: format!(
                        "input {index}: expected sort {mapped_sort}, got {target_sort}"
                    ),
                });
            }
        }
        if name_free(names, output) != name_free(&target_names, &target.output) {
            return Err(MorphismError::OpTypeMismatch {
                op: operation.name.clone(),
                detail: format!("output: expected sort {output}, got {}", target.output),
            });
        }
        Ok(())
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
    fn morphism_structure(src: &Theory, tgt: &Theory, m: &Morphism) -> Result<(), MorphismError> {
        morphism_sort_structure(src, tgt, m)?;
        for operation in &src.ops {
            let image = m
                .ops
                .get(&operation.name)
                .ok_or_else(|| MorphismError::MissingOpMapping(operation.name.clone()))?;
            for (_, sort, _) in &operation.inputs {
                if !m.sorts.contains_key(sort.head()) {
                    return Err(MorphismError::MissingSortMapping(sort.head().to_owned()));
                }
            }
            if !m.sorts.contains_key(operation.output.head()) {
                return Err(MorphismError::MissingSortMapping(
                    operation.output.head().to_owned(),
                ));
            }
            let names: Vec<Name> = operation
                .inputs
                .iter()
                .map(|(name, _, _)| name.clone())
                .collect();
            let inputs = operation
                .inputs
                .iter()
                .map(|(name, sort, implicit)| {
                    (name.clone(), translate_sort(src, m, sort), *implicit)
                })
                .collect::<Vec<_>>();
            let output = translate_sort(src, m, &operation.output);
            match image {
                OpImage::Op(target_name) => {
                    let target = find_op(tgt, target_name)
                        .ok_or_else(|| MorphismError::OpNotFound(target_name.clone()))?;
                    check_named_op_image(operation, target, &names, &inputs, &output)?;
                }
                OpImage::Term(term) => {
                    let context = inputs
                        .iter()
                        .map(|(name, sort, _)| (name.clone(), sort.clone()))
                        .collect::<Vec<_>>();
                    if let Some(variable) = unbound(&context, term) {
                        return Err(MorphismError::TermAssignmentUnboundVar {
                            op: operation.name.clone(),
                            var: variable,
                        });
                    }
                    let found = infer_term(tgt, &context, term).map_err(|detail| {
                        MorphismError::TermAssignmentIllTyped {
                            op: operation.name.clone(),
                            detail,
                        }
                    })?;
                    if !sorts_agree(&found, &names, &output, &names) {
                        return Err(MorphismError::TermAssignmentSortMismatch {
                            op: operation.name.clone(),
                            expected: output.to_string(),
                            got: found.to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Definition: a theory morphism preserves the declared theory structure.
    ///
    /// It preserves sort kinds and parameter telescopes, the constructor image
    /// set of each closed source sort, operation typing under the whole term
    /// action, and every ordinary and directed equation. An open source sort
    /// imposes no closure condition.
    ///
    /// Book anchors: `L-theories-7`.
    ///
    /// # Errors
    ///
    /// Returns the first [`MorphismError`] in sort, operation, ordinary
    /// equation, then directed-equation order.
    pub fn check_morphism(src: &Theory, tgt: &Theory, m: &Morphism) -> Result<(), MorphismError> {
        morphism_structure(src, tgt, m)?;
        equations_preserved(src, tgt, m)
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

    /// Name every class of one kind: its candidate. A candidate shared by
    /// several classes, or equal to a reserved name (the name of an added
    /// equation, held by the operation it identifies), is a clash; the least
    /// clashing name in byte order is the one a refusal reports. Under
    /// amalgamation a clash is a `NameConflict` between the least member of
    /// the first class holding that name and the least member of the next
    /// one, or the reserved name's holder; under
    /// explicit identity refused as `NameClash` or freshened as
    /// `{least member label}.{candidate}`, and then every class of a name
    /// that is still shared, or still reserved, is suffixed `#1`, `#2`, ...
    /// in ascending class order, until no name is shared or reserved.
    fn name_classes(
        kind: SymKind,
        classes: &[Vec<SymRef>],
        reserved: &BTreeMap<Name, SymRef>,
        mode: Mode,
        clash: ClashPolicy,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<Name>, ComposeError> {
        let cands: Vec<Name> = classes.iter().map(|c| majority(c)).collect();
        let mut names = cands.clone();
        // Clashes are met in ascending order of the candidate name, so a
        // refusal reports the least clashing name.
        let mut order: Vec<usize> = (0..cands.len()).collect();
        order.sort_by(|a, b| cands[*a].cmp(&cands[*b]).then(a.cmp(b)));
        for i in order {
            let c = &cands[i];
            let sharing: Vec<usize> = (0..cands.len()).filter(|j| cands[*j] == *c).collect();
            let holder = reserved.get(c);
            if sharing.len() < 2 && holder.is_none() {
                continue;
            }
            if mode == Mode::Amalgamate {
                let other = sharing
                    .iter()
                    .find(|j| **j != i)
                    .map(|j| classes[*j][0].clone())
                    .or_else(|| holder.cloned())
                    .unwrap_or_else(|| classes[i][0].clone());
                return Err(ComposeError::NameConflict(
                    kind,
                    c.clone(),
                    classes[i][0].clone(),
                    other,
                ));
            }
            if clash == ClashPolicy::Reject {
                let mut clashing: Vec<Vec<SymRef>> =
                    sharing.iter().map(|j| classes[*j].clone()).collect();
                if let Some(holder) = holder {
                    clashing.push(vec![holder.clone()]);
                }
                clashing.sort();
                return Err(ComposeError::NameClash(kind, c.clone(), Box::new(clashing)));
            }
            let label = classes[i]
                .iter()
                .map(|m| m.node.clone())
                .min()
                .unwrap_or_default();
            names[i] = format!("{label}.{c}");
        }
        loop {
            let mut sharing: BTreeMap<Name, Vec<usize>> = BTreeMap::new();
            for (index, name) in names.iter().enumerate() {
                sharing.entry(name.clone()).or_default().push(index);
            }
            let shared: Vec<(Name, Vec<usize>)> = sharing
                .into_iter()
                .filter(|(name, indices)| indices.len() > 1 || reserved.contains_key(name))
                .collect();
            if shared.is_empty() {
                break;
            }
            for (name, indices) in shared {
                for (position, index) in indices.into_iter().enumerate() {
                    names[index] = format!("{name}#{}", position + 1);
                }
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
    /// signatures by class (`OpSignatureMismatch`), a disagreeing class that
    /// the name rule alone formed (no two members related by the edges)
    /// being `NameConflict` in `Amalgamate` mode; (5)
    /// for each edge and operation with a term image, the equation
    /// `class(o)(inputs) = translate(t)` named `"{src label}.{o}~{rank}"`, the
    /// rank of the edge in ascending (source label, target label, morphism)
    /// order; (6) one result sort and operation per class, named by majority
    /// (ties to the least name; under `ExplicitIdentity` a shared name is a
    /// clash, refused or freshened as `{least label}.{name}` and then suffixed
    /// `#k`); equations and directed equations translated along the cocone, a
    /// set by content in both modes (translations alpha-equivalent in either
    /// orientation become one, directed ones in their stated orientation and
    /// also equal in their opaque parts), and under `Amalgamate` one name
    /// carried by two classes of different content is `NameConflict`;
    /// policies a set by content (value kind and strategy) named as symbols
    /// are, with the same `Amalgamate` name check; (7) the result passes
    /// [`typecheck_signature`], its
    /// `UndeclaredSort` defects on free names admitted (`IllTypedResult`
    /// otherwise).
    ///
    /// # Errors
    ///
    /// The first [`ComposeError`] of the steps in that order.
    ///
    /// Book anchors: `L-theories-3`, `L-theories-4`, `L-theories-5`.
    /// Compatibility adapter for the pre-diagram reference fixtures. New
    /// callers use [`compose`] so every semantically relevant option is
    /// explicit in one value.
    pub fn compose_legacy(
        nodes: &[(Name, Theory)],
        edges: &[(Name, Name, Morphism)],
        mode: Mode,
        clash: ClashPolicy,
        result_name: &str,
    ) -> Result<Composition, ComposeError> {
        compose_with_budget(nodes, edges, mode, clash, result_name, 512)
    }

    fn diagram_theory<'a>(nodes: &'a [(Name, Theory)], label: &str) -> Option<&'a Theory> {
        nodes
            .iter()
            .find(|(candidate, _)| candidate == label)
            .map(|(_, theory)| theory)
    }

    fn validate_diagram(
        nodes: &[(Name, Theory)],
        edges: &[(Name, Name, Morphism)],
    ) -> Result<(), ComposeError> {
        let mut labels = BTreeSet::new();
        for (label, _) in nodes {
            if label.is_empty() || label.contains('.') || label.contains('#') {
                return Err(ComposeError::InvalidNodeLabel(label.clone()));
            }
            if !labels.insert(label.clone()) {
                return Err(ComposeError::DuplicateNodeLabel(label.clone()));
            }
        }
        for (source, target, _) in edges {
            for label in [source, target] {
                if diagram_theory(nodes, label).is_none() {
                    return Err(ComposeError::UnknownNodeLabel(label.clone()));
                }
            }
        }
        for (label, node) in nodes {
            if let Err(defects) = typecheck_signature(node) {
                let rejected = defects
                    .into_iter()
                    .filter(|defect| match defect {
                        Defect::UndeclaredSort(_, name) => {
                            node.sorts.iter().any(|sort| sort.name == *name)
                        }
                        _ => true,
                    })
                    .collect::<Vec<_>>();
                if !rejected.is_empty() {
                    return Err(ComposeError::InvalidNode(label.clone(), rejected));
                }
            }
        }
        for (index, (source_label, target_label, morphism)) in edges.iter().enumerate() {
            let Some(source) = diagram_theory(nodes, source_label) else {
                return Err(ComposeError::UnknownNodeLabel(source_label.clone()));
            };
            let Some(target) = diagram_theory(nodes, target_label) else {
                return Err(ComposeError::UnknownNodeLabel(target_label.clone()));
            };
            check_morphism(source, target, morphism).map_err(|detail| {
                ComposeError::InvalidLeg {
                    edge: index,
                    detail,
                }
            })?;
            let target_implicit = undeclared_sorts(target);
            for name in undeclared_sorts(source) {
                if let Some(image) = morphism.sorts.get(&name)
                    && !target.sorts.iter().any(|sort| sort.name == *image)
                    && !target_implicit.contains(image)
                {
                    return Err(ComposeError::InvalidLeg {
                        edge: index,
                        detail: MorphismError::ImplicitSortImageUnknown {
                            source: name,
                            target: image.clone(),
                        },
                    });
                }
            }
        }
        Ok(())
    }

    fn compose_with_budget(
        nodes: &[(Name, Theory)],
        edges: &[(Name, Name, Morphism)],
        mode: Mode,
        clash: ClashPolicy,
        result_name: &str,
        _rewrite_budget: u32,
    ) -> Result<Composition, ComposeError> {
        validate_diagram(nodes, edges)?;
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

    /// Definition: Compute the finite diagram colimit under the explicitly
    /// recorded composition options.
    ///
    /// Node and edge vector order affects neither endpoint identity nor class
    /// construction; endpoints are labels and derived-term equation ranks use
    /// canonical morphism bytes.
    ///
    /// # Errors
    ///
    /// Returns the first [`ComposeError`] in Q.2 construction order.
    ///
    /// Book anchors: `L-theories-3`, `L-theories-4`, `L-theories-5`.
    pub fn compose(
        diagram: &TheoryDiagram,
        options: &ComposeOptions,
    ) -> Result<Composition, ComposeError> {
        let nodes: Vec<(Name, Theory)> = diagram
            .nodes
            .iter()
            .map(|node| (node.label.clone(), node.theory.clone()))
            .collect();
        let edges: Vec<(Name, Name, Morphism)> = diagram
            .edges
            .iter()
            .map(|edge| (edge.src.clone(), edge.tgt.clone(), edge.morphism.clone()))
            .collect();
        compose_with_budget(
            &nodes,
            &edges,
            options.mode,
            options.clash,
            &options.result_name,
            options.rewrite_budget,
        )
    }

    /// One operation image in a serialized composition recipe.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RecipeOpImage {
        /// A codomain operation.
        Op(Name),
        /// A derived codomain term.
        Term(Term),
    }

    /// One edge in a serialized diagram recipe.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RecipeEdge {
        /// Domain node label.
        pub src: Name,
        /// Codomain node label.
        pub tgt: Name,
        /// Sort assignments.
        pub sorts: Vec<(Name, Name)>,
        /// Operation assignments.
        pub ops: Vec<(Name, RecipeOpImage)>,
    }

    /// One replayable composition step.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum CompositionStep {
        /// A named registry theory.
        Base(Name),
        /// The by-name colimit of two named theories.
        Colimit {
            /// Left theory.
            left: Name,
            /// Right theory.
            right: Name,
            /// Sorts in the explicit interface.
            shared_sorts: Vec<Name>,
            /// Operations in the explicit interface.
            shared_ops: Vec<Name>,
            /// Identification mode (legacy recipes use amalgamation).
            mode: CompositionMode,
        },
        /// A complete finite diagram step.
        Diagram {
            /// Node names (also their labels).
            nodes: Vec<Name>,
            /// Diagram edges.
            edges: Vec<RecipeEdge>,
            /// Identification mode.
            mode: CompositionMode,
            /// Clash policy.
            clash: ClashPolicy,
            /// Rewrite budget.
            rewrite_budget: u32,
            /// Theories local to this step, resolved before the registry.
            inline: Vec<Theory>,
        },
    }

    /// A versioned, replayable theory-composition recipe.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct CompositionSpec {
        /// Final theory name.
        pub result_name: Name,
        /// Ordered steps.
        pub steps: Vec<CompositionStep>,
        /// Zero for legacy/base-amalgamation recipes, one for diagram or
        /// explicit-identity recipes.
        pub version: u16,
    }

    /// A deterministic replay refusal.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum RecomposeError {
        /// A step names no registry, inline, or prior-step theory.
        TheoryNotFound(Name),
        /// A declared shared symbol is absent from one side.
        MissingShared(Name),
        /// Composition refused the reconstructed diagram.
        Composition(Box<ComposeError>),
    }

    /// Definition: Replay every legacy, mode-bearing and full-diagram step of
    /// a composition recipe against a named theory registry.
    ///
    /// # Errors
    ///
    /// Returns the first [`RecomposeError`] in step order.
    ///
    /// Book anchors: `L-theories-8`.
    pub fn recompose(
        spec: &CompositionSpec,
        registry: &BTreeMap<Name, Theory>,
    ) -> Result<Theory, RecomposeError> {
        let mut theories = registry.clone();
        let mut last = None;
        for (index, step) in spec.steps.iter().enumerate() {
            let step_name = format!("step_{index}");
            let Some(result) = replay_step(&step_name, step, &theories)? else {
                let CompositionStep::Base(name) = step else {
                    unreachable!("only a base step selects an existing theory")
                };
                last = Some(name.clone());
                continue;
            };
            theories.insert(step_name.clone(), result);
            last = Some(step_name);
        }
        let final_name = last.ok_or_else(|| {
            RecomposeError::TheoryNotFound(format!("empty recipe for {}", spec.result_name))
        })?;
        let mut result = theories
            .get(&final_name)
            .cloned()
            .ok_or(RecomposeError::TheoryNotFound(final_name))?;
        result.name.clone_from(&spec.result_name);
        Ok(result)
    }

    fn replay_step(
        step_name: &str,
        step: &CompositionStep,
        theories: &BTreeMap<Name, Theory>,
    ) -> Result<Option<Theory>, RecomposeError> {
        match step {
            CompositionStep::Base(name) => {
                if !theories.contains_key(name) {
                    return Err(RecomposeError::TheoryNotFound(name.clone()));
                }
                Ok(None)
            }
            CompositionStep::Colimit {
                left,
                right,
                shared_sorts,
                shared_ops,
                mode,
            } => replay_colimit(
                step_name,
                theories,
                left,
                right,
                shared_sorts,
                shared_ops,
                *mode,
            )
            .map(Some),
            CompositionStep::Diagram { .. } => replay_diagram(step_name, theories, step).map(Some),
        }
    }

    fn replay_colimit(
        step_name: &str,
        theories: &BTreeMap<Name, Theory>,
        left: &str,
        right: &str,
        shared_sorts: &[Name],
        shared_ops: &[Name],
        mode: CompositionMode,
    ) -> Result<Theory, RecomposeError> {
        let left_theory = theories
            .get(left)
            .cloned()
            .ok_or_else(|| RecomposeError::TheoryNotFound(left.to_owned()))?;
        let right_theory = theories
            .get(right)
            .cloned()
            .ok_or_else(|| RecomposeError::TheoryNotFound(right.to_owned()))?;
        let mut interface = Theory::default();
        for name in shared_sorts {
            let sort = left_theory
                .sorts
                .iter()
                .find(|sort| sort.name == *name)
                .filter(|_| right_theory.sorts.iter().any(|sort| sort.name == *name))
                .cloned()
                .ok_or_else(|| RecomposeError::MissingShared(name.clone()))?;
            interface.sorts.push(sort);
        }
        for name in shared_ops {
            let operation = left_theory
                .ops
                .iter()
                .find(|operation| operation.name == *name)
                .filter(|_| {
                    right_theory
                        .ops
                        .iter()
                        .any(|operation| operation.name == *name)
                })
                .cloned()
                .ok_or_else(|| RecomposeError::MissingShared(name.clone()))?;
            interface.ops.push(operation);
        }
        let interface_label = format!("{step_name}_interface");
        let diagram = TheoryDiagram::by_name(
            (interface_label, interface),
            (left.to_owned(), left_theory),
            (right.to_owned(), right_theory),
        )
        .map_err(|error| RecomposeError::Composition(Box::new(error)))?;
        compose(
            &diagram,
            &ComposeOptions {
                mode,
                result_name: step_name.to_owned(),
                clash: ClashPolicy::Reject,
                rewrite_budget: 512,
            },
        )
        .map_err(|error| RecomposeError::Composition(Box::new(error)))
        .map(|composition| composition.theory)
    }

    fn replay_diagram(
        step_name: &str,
        theories: &BTreeMap<Name, Theory>,
        step: &CompositionStep,
    ) -> Result<Theory, RecomposeError> {
        let CompositionStep::Diagram {
            nodes: node_names,
            edges: edge_specs,
            mode,
            clash,
            rewrite_budget,
            inline: inline_theories,
        } = step
        else {
            unreachable!("caller selected a diagram step");
        };
        let inline: BTreeMap<&str, &Theory> = inline_theories
            .iter()
            .map(|theory| (theory.name.as_str(), theory))
            .collect();
        let nodes = node_names
            .iter()
            .map(|name| {
                inline
                    .get(name.as_str())
                    .copied()
                    .cloned()
                    .or_else(|| theories.get(name).cloned())
                    .map(|theory| DiagramNode {
                        label: name.clone(),
                        theory,
                    })
                    .ok_or_else(|| RecomposeError::TheoryNotFound(name.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let edges = edge_specs
            .iter()
            .map(|edge| DiagramEdge {
                src: edge.src.clone(),
                tgt: edge.tgt.clone(),
                morphism: Morphism {
                    name: format!("{}-to-{}", edge.src, edge.tgt),
                    domain: edge.src.clone(),
                    codomain: edge.tgt.clone(),
                    sorts: edge.sorts.iter().cloned().collect(),
                    ops: edge
                        .ops
                        .iter()
                        .map(|(name, image)| {
                            let image = match image {
                                RecipeOpImage::Op(name) => OpImage::Op(name.clone()),
                                RecipeOpImage::Term(term) => OpImage::Term(term.clone()),
                            };
                            (name.clone(), image)
                        })
                        .collect(),
                },
            })
            .collect();
        compose(
            &TheoryDiagram { nodes, edges },
            &ComposeOptions {
                mode: *mode,
                result_name: step_name.to_owned(),
                clash: *clash,
                rewrite_budget: *rewrite_budget,
            },
        )
        .map_err(|error| RecomposeError::Composition(Box::new(error)))
        .map(|composition| composition.theory)
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

    struct FinishNames {
        provenance: Provenance,
        freshened: Vec<Freshening>,
        result_of: BTreeMap<(SymKind, String), Name>,
    }

    fn finish(
        input: &Input<'_>,
        cl: &Classes,
        legs: &Classes,
        implicit: &BTreeSet<(SymKind, SymRef)>,
        result_name: &str,
    ) -> Result<Composition, ComposeError> {
        let (nodes, edges, mode, clash) = (input.nodes, input.edges, input.mode, input.clash);
        check_signatures(nodes, cl, legs, implicit, mode)?;
        let FinishNames {
            mut provenance,
            mut freshened,
            result_of,
        } = finish_names(cl, mode, clash)?;
        let res =
            |kind: SymKind, node: &str, name: &str| result_symbol(cl, &result_of, kind, node, name);
        let (mut out, free_sorts) = finish_signature(nodes, cl, implicit, result_name, &res);
        let (eqs, added) = equation_inputs(nodes, edges, &res);
        check_transports(&out, nodes, &eqs, &added, &res)?;
        let reserved: BTreeMap<Name, SymRef> = added
            .iter()
            .map(|(name, generated)| (name.clone(), generated.source.op.clone()))
            .collect();
        // Every amalgamation name conflict among equations, directed
        // equations and policies comes before any of them is named.
        let eq_classes = equation_classes(&eqs, mode)?;
        let directed = directed_classes(nodes, &res, mode)?;
        let policy = policy_classes(nodes, mode)?;
        out.eqs = name_items(
            eq_classes,
            &reserved,
            mode,
            clash,
            &mut provenance,
            &mut freshened,
        )?
        .into_iter()
        .map(|(name, (lhs, rhs))| Equation { name, lhs, rhs })
        .collect();
        let closing = least_closing(&added);
        for (name, generated) in added {
            out.eqs.push(Equation {
                name: name.clone(),
                lhs: generated.lhs,
                rhs: generated.rhs,
            });
            provenance.added_eqs.insert(name, generated.source);
        }
        out.eqs.sort_by(|a, b| a.name.cmp(&b.name));
        out.directed_eqs = name_items(
            directed,
            &BTreeMap::new(),
            mode,
            clash,
            &mut provenance,
            &mut freshened,
        )?
        .into_iter()
        .map(|(name, d)| DirectedEquation { name, ..d })
        .collect();
        out.policies = name_items(
            policy,
            &BTreeMap::new(),
            mode,
            clash,
            &mut provenance,
            &mut freshened,
        )?
        .into_iter()
        .map(|(name, p)| Policy { name, ..p })
        .collect();
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
        if typecheck_theory_body(&out).is_err() {
            return Err(ComposeError::IllTypedResult(Vec::new()));
        }
        let inclusions = finish_inclusions(nodes, &provenance, &out.name);
        let (name_clash_free, checks) =
            finish_checks(input, legs, &provenance, &closing, &free_sorts);
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

    /// For each edge that generated equations, the least of their names.
    fn least_closing(added: &[(Name, Generated)]) -> BTreeMap<usize, Name> {
        let mut names: BTreeMap<usize, BTreeSet<Name>> = BTreeMap::new();
        for (name, generated) in added {
            names
                .entry(generated.edge_index)
                .or_default()
                .insert(name.clone());
        }
        names
            .into_iter()
            .filter_map(|(edge, set)| set.into_iter().next().map(|least| (edge, least)))
            .collect()
    }

    /// The transport check: every translated node equation, then every
    /// generated equation, then every translated directed equation (as the
    /// equation between its sides), each well typed against the result's
    /// sorts and operations; the first that is not is `IllTypedTransport`.
    fn check_transports(
        out: &Theory,
        nodes: &[(Name, Theory)],
        eqs: &[(SymRef, Name, Term, Term)],
        added: &[(Name, Generated)],
        res: &dyn Fn(SymKind, &str, &str) -> Name,
    ) -> Result<(), ComposeError> {
        for (source, _, lhs, rhs) in eqs {
            equation_well_typed(out, lhs, rhs).map_err(|detail| {
                ComposeError::IllTypedTransport {
                    source: source.clone(),
                    detail,
                }
            })?;
        }
        for (_, generated) in added {
            equation_well_typed(out, &generated.lhs, &generated.rhs).map_err(|detail| {
                ComposeError::IllTypedTransport {
                    source: generated.source.op.clone(),
                    detail,
                }
            })?;
        }
        for (label, node) in nodes {
            let op = |name: &str| res(SymKind::Op, label, name);
            for equation in &node.directed_eqs {
                let lhs = rename_term(&equation.lhs, &op);
                let rhs = rename_term(&equation.rhs, &op);
                equation_well_typed(out, &lhs, &rhs).map_err(|detail| {
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
        Ok(())
    }

    fn finish_names(
        classes: &Classes,
        mode: Mode,
        clash: ClashPolicy,
    ) -> Result<FinishNames, ComposeError> {
        let mut provenance = Provenance::default();
        let mut freshened = Vec::new();
        let mut result_of = BTreeMap::new();
        for kind in [SymKind::Sort, SymKind::Op] {
            let members = classes.members(kind);
            let names = name_classes(
                kind,
                &members,
                &BTreeMap::new(),
                mode,
                clash,
                &mut freshened,
            )?;
            for (class, name) in members.iter().zip(names) {
                for member in class {
                    provenance
                        .map_mut(kind)
                        .insert(member.clone(), name.clone());
                    result_of.insert(
                        (kind, class_id(classes, kind, &member.node, &member.name)),
                        name.clone(),
                    );
                }
            }
        }
        Ok(FinishNames {
            provenance,
            freshened,
            result_of,
        })
    }

    fn result_symbol(
        classes: &Classes,
        result_of: &BTreeMap<(SymKind, String), Name>,
        kind: SymKind,
        node: &str,
        name: &str,
    ) -> Name {
        result_of
            .get(&(kind, class_id(classes, kind, node, name)))
            .cloned()
            .unwrap_or_else(|| name.to_owned())
    }

    fn finish_signature(
        nodes: &[(Name, Theory)],
        classes: &Classes,
        implicit: &BTreeSet<(SymKind, SymRef)>,
        result_name: &str,
        result_symbol: &dyn Fn(SymKind, &str, &str) -> Name,
    ) -> (Theory, BTreeSet<Name>) {
        let mut output = Theory {
            name: result_name.to_owned(),
            ..Theory::default()
        };
        let mut free_sorts = BTreeSet::new();
        for class in classes.members(SymKind::Sort) {
            let declared = class
                .iter()
                .find(|member| !implicit.contains(&(SymKind::Sort, (*member).clone())));
            let Some(member) = declared else {
                free_sorts.insert(result_symbol(SymKind::Sort, &class[0].node, &class[0].name));
                continue;
            };
            if let Some(sort) = node_theory(nodes, &member.node)
                .sorts
                .iter()
                .find(|sort| sort.name == member.name)
            {
                let sort_name = |name: &str| result_symbol(SymKind::Sort, &member.node, name);
                let op_name = |name: &str| result_symbol(SymKind::Op, &member.node, name);
                output.sorts.push(Sort {
                    name: sort_name(&member.name),
                    params: sort
                        .params
                        .iter()
                        .map(|(parameter, sort)| {
                            (parameter.clone(), rename_sort(sort, &sort_name, &op_name))
                        })
                        .collect(),
                    kind: sort.kind.clone(),
                    constructors: sort.constructors.as_ref().map(|constructors| {
                        let mut mapped = constructors
                            .iter()
                            .map(|name| op_name(name))
                            .collect::<Vec<_>>();
                        mapped.sort();
                        mapped.dedup();
                        mapped
                    }),
                });
            }
        }
        for class in classes.members(SymKind::Op) {
            let member = &class[0];
            if let Some(operation) = node_theory(nodes, &member.node)
                .ops
                .iter()
                .find(|operation| operation.name == member.name)
            {
                let sort_name = |name: &str| result_symbol(SymKind::Sort, &member.node, name);
                let op_name = |name: &str| result_symbol(SymKind::Op, &member.node, name);
                output.ops.push(Op {
                    name: op_name(&member.name),
                    inputs: operation
                        .inputs
                        .iter()
                        .map(|(parameter, sort, implicit)| {
                            (
                                parameter.clone(),
                                rename_sort(sort, &sort_name, &op_name),
                                *implicit,
                            )
                        })
                        .collect(),
                    output: rename_sort(&operation.output, &sort_name, &op_name),
                });
            }
        }
        (output, free_sorts)
    }

    fn finish_inclusions(
        nodes: &[(Name, Theory)],
        provenance: &Provenance,
        result_name: &str,
    ) -> Vec<Morphism> {
        nodes
            .iter()
            .map(|(label, node)| Morphism {
                name: format!("{label}-inclusion"),
                domain: node.name.clone(),
                codomain: result_name.to_owned(),
                sorts: provenance
                    .sorts
                    .iter()
                    .filter(|(source, _)| source.node == *label)
                    .map(|(source, image)| (source.name.clone(), image.clone()))
                    .collect(),
                ops: node
                    .ops
                    .iter()
                    .filter_map(|operation| {
                        provenance
                            .ops
                            .get(&SymRef {
                                node: label.clone(),
                                name: operation.name.clone(),
                            })
                            .map(|image| (operation.name.clone(), OpImage::Op(image.clone())))
                    })
                    .collect(),
            })
            .collect()
    }

    fn finish_checks(
        input: &Input<'_>,
        legs: &Classes,
        provenance: &Provenance,
        closing: &BTreeMap<usize, Name>,
        free_sorts: &BTreeSet<Name>,
    ) -> (bool, Vec<CheckRecord>) {
        let mut checks = node_checks(input.nodes, provenance, free_sorts);
        checks.extend(edge_checks(input, closing));
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
        let name_clash_free = source_names_are_related(legs, provenance);
        checks.push(CheckRecord {
            check: CompositionCheckKind::NameClashFree,
            subject: CheckSubject::Diagram,
            outcome: CheckOutcome::Passed,
        });
        checks.sort();
        (name_clash_free, checks)
    }

    fn node_checks(
        nodes: &[(Name, Theory)],
        provenance: &Provenance,
        free_sorts: &BTreeSet<Name>,
    ) -> Vec<CheckRecord> {
        let mut checks = Vec::new();
        for (label, node) in nodes {
            checks.push(CheckRecord {
                check: CompositionCheckKind::NodeWellFormed,
                subject: CheckSubject::Node(label.clone()),
                outcome: CheckOutcome::Passed,
            });
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
                subject: CheckSubject::Node(label.clone()),
                outcome,
            });
            checks.push(CheckRecord {
                check: CompositionCheckKind::EquationImage,
                subject: CheckSubject::Node(label.clone()),
                outcome: CheckOutcome::Passed,
            });
        }
        checks
    }

    /// The leg and cocone-square records of every edge; `closing` holds,
    /// per diagram edge, the least name of the equations its term-valued
    /// images generated.
    fn edge_checks(input: &Input<'_>, closing: &BTreeMap<usize, Name>) -> Vec<CheckRecord> {
        let mut checks = Vec::new();
        for (edge, _) in input.edges.iter().enumerate() {
            checks.push(CheckRecord {
                check: CompositionCheckKind::LegIsMorphism,
                subject: CheckSubject::Edge(edge),
                outcome: CheckOutcome::Passed,
            });
            let equation = closing.get(&edge).cloned();
            checks.push(CheckRecord {
                check: CompositionCheckKind::CoconeCommutes,
                subject: CheckSubject::Edge(edge),
                outcome: equation.map_or(CheckOutcome::Passed, |equation| {
                    CheckOutcome::PassedByEquation { equation }
                }),
            });
        }
        checks
    }

    /// Whether every same-kind, same-name pair is already related by the
    /// diagram's edge equivalence, before amalgamation adds name joins.
    fn source_names_are_related(legs: &Classes, provenance: &Provenance) -> bool {
        let keys: Vec<&(SymKind, SymRef)> = legs.label.keys().collect();
        let structural = keys.iter().all(|left| {
            keys.iter().all(|right| {
                left.0 != right.0
                    || left.1.name != right.1.name
                    || left.1.node == right.1.node
                    || legs.label.get(*left) == legs.label.get(*right)
            })
        });
        structural
            && [SymKind::Eq, SymKind::DirectedEq, SymKind::Policy]
                .into_iter()
                .all(|kind| {
                    let mut by_name: BTreeMap<&str, Vec<(&SymRef, &Name)>> = BTreeMap::new();
                    for (source, image) in provenance.map(kind) {
                        by_name
                            .entry(source.name.as_str())
                            .or_default()
                            .push((source, image));
                    }
                    by_name.into_values().all(|entries| {
                        entries.iter().all(|(left, left_image)| {
                            entries.iter().all(|(right, right_image)| {
                                left.node == right.node || left_image == right_image
                            })
                        })
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

    fn eta_image(image: &OpImage, inputs: &[Name]) -> Term {
        match image {
            OpImage::Op(operation) => Term::App(
                operation.clone(),
                inputs.iter().cloned().map(Term::Var).collect(),
            ),
            OpImage::Term(term) => term.clone(),
        }
    }

    fn op_images_equal_within(
        left: &OpImage,
        right: &OpImage,
        inputs: &[Name],
        target: &Theory,
        budget: u32,
    ) -> Result<bool, u32> {
        let left = eta_image(left, inputs);
        let right = eta_image(right, inputs);
        let images = alpha(&left, &right);
        if images.0 == images.1 {
            Ok(true)
        } else {
            derivable_within(&left, &right, target, budget)
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
        fn verify_universal_parts(
            &self,
            nodes: &[(Name, Theory)],
            edges: &[(Name, Name, Morphism)],
            target: &Theory,
            legs: &[Morphism],
            rewrite_budget: u32,
        ) -> Result<(Morphism, CompositionReport), ComposeError> {
            let positions: BTreeMap<&str, usize> = nodes
                .iter()
                .enumerate()
                .map(|(index, (label, _))| (label.as_str(), index))
                .collect();
            let input = UniversalInput {
                nodes,
                target,
                legs,
                positions,
                rewrite_budget,
            };
            let mut exhausted = verify_cocone_legs(&input)?;
            verify_cocone_edges(self, &input, edges, &mut exhausted)?;
            let mediator = universal_mediator(self, &input, &mut exhausted)?;
            verify_mediator(self, &input, &mediator, &mut exhausted)?;
            let report = universal_report(&self.report, target, exhausted);
            Ok((mediator, report))
        }

        /// Verify the universal arrow with the reference default rewrite
        /// budget of 512 steps per equality.
        ///
        /// # Errors
        ///
        /// [`ComposeError::NotACocone`] when the proposed legs do not form a
        /// cocone, or [`ComposeError::InvalidLeg`] when the induced mediator
        /// is not a theory morphism.
        pub fn verify_universal(
            &self,
            diagram: &TheoryDiagram,
            target: &Theory,
            legs: &[Morphism],
        ) -> Result<(Morphism, CompositionReport), ComposeError> {
            self.verify_universal_within(diagram, target, legs, 512)
        }

        /// Verify the universal arrow with an explicit per-equality rewrite
        /// budget. Exhaustion is recorded as `Incomplete`, never `Passed`.
        ///
        /// # Errors
        ///
        /// As for [`Self::verify_universal`].
        pub fn verify_universal_within(
            &self,
            diagram: &TheoryDiagram,
            target: &Theory,
            legs: &[Morphism],
            rewrite_budget: u32,
        ) -> Result<(Morphism, CompositionReport), ComposeError> {
            let nodes: Vec<(Name, Theory)> = diagram
                .nodes
                .iter()
                .map(|node| (node.label.clone(), node.theory.clone()))
                .collect();
            let edges: Vec<(Name, Name, Morphism)> = diagram
                .edges
                .iter()
                .map(|edge| (edge.src.clone(), edge.tgt.clone(), edge.morphism.clone()))
                .collect();
            self.verify_universal_parts(&nodes, &edges, target, legs, rewrite_budget)
        }
    }

    struct UniversalInput<'a> {
        nodes: &'a [(Name, Theory)],
        target: &'a Theory,
        legs: &'a [Morphism],
        positions: BTreeMap<&'a str, usize>,
        rewrite_budget: u32,
    }

    fn verify_cocone_legs(input: &UniversalInput<'_>) -> Result<Option<u32>, ComposeError> {
        if input.legs.len() != input.nodes.len() {
            return Err(ComposeError::NotACocone(Vec::new()));
        }
        let mut exhausted = None;
        for ((_, node), leg) in input.nodes.iter().zip(input.legs) {
            if morphism_structure(node, input.target, leg).is_err() {
                return Err(ComposeError::NotACocone(Vec::new()));
            }
            match equations_preserved_within(node, input.target, leg, input.rewrite_budget) {
                Ok(true) => {}
                Ok(false) => return Err(ComposeError::NotACocone(Vec::new())),
                Err(steps) => exhausted = Some(steps),
            }
        }
        Ok(exhausted)
    }

    fn cocone_class(
        composition: &Composition,
        kind: SymKind,
        node: &str,
        name: &str,
    ) -> Vec<SymRef> {
        composition
            .provenance
            .image(
                kind,
                &SymRef {
                    node: node.to_owned(),
                    name: name.to_owned(),
                },
            )
            .map_or_else(Vec::new, |result| {
                composition.provenance.members(kind, result)
            })
    }

    fn verify_cocone_edges(
        composition: &Composition,
        input: &UniversalInput<'_>,
        edges: &[(Name, Name, Morphism)],
        exhausted: &mut Option<u32>,
    ) -> Result<(), ComposeError> {
        for (source, target_label, edge) in edges {
            let Some(&source_index) = input.positions.get(source.as_str()) else {
                return Err(ComposeError::NotACocone(Vec::new()));
            };
            let Some(&target_index) = input.positions.get(target_label.as_str()) else {
                return Err(ComposeError::NotACocone(Vec::new()));
            };
            let target_node = &input.nodes[target_index].1;
            for (sort, image) in &edge.sorts {
                if input.legs[source_index].sorts.get(sort)
                    != input.legs[target_index].sorts.get(image)
                {
                    return Err(ComposeError::NotACocone(cocone_class(
                        composition,
                        SymKind::Sort,
                        source,
                        sort,
                    )));
                }
            }
            for (operation, image) in &edge.ops {
                let Some(source_operation) = input.nodes[source_index]
                    .1
                    .ops
                    .iter()
                    .find(|candidate| candidate.name == *operation)
                else {
                    return Err(ComposeError::NotACocone(Vec::new()));
                };
                let inputs: Vec<Name> = source_operation
                    .inputs
                    .iter()
                    .map(|(name, _, _)| name.clone())
                    .collect();
                let left = input.legs[source_index].ops.get(operation);
                let right = image_through(target_node, &input.legs[target_index], image);
                let agrees = match left.zip(right.as_ref()) {
                    Some((left, right)) => match op_images_equal_within(
                        left,
                        right,
                        &inputs,
                        input.target,
                        input.rewrite_budget,
                    ) {
                        Ok(agrees) => agrees,
                        Err(steps) => {
                            *exhausted = Some(steps);
                            true
                        }
                    },
                    None => false,
                };
                if !agrees {
                    return Err(ComposeError::NotACocone(cocone_class(
                        composition,
                        SymKind::Op,
                        source,
                        operation,
                    )));
                }
            }
        }
        Ok(())
    }

    fn universal_mediator(
        composition: &Composition,
        input: &UniversalInput<'_>,
        exhausted: &mut Option<u32>,
    ) -> Result<Morphism, ComposeError> {
        let mut mediator = Morphism::default();
        mediator_sorts(composition, input, &mut mediator)?;
        mediator_operations(composition, input, exhausted, &mut mediator)?;
        Ok(mediator)
    }

    fn mediator_sorts(
        composition: &Composition,
        input: &UniversalInput<'_>,
        mediator: &mut Morphism,
    ) -> Result<(), ComposeError> {
        for result_sort in &composition.theory.sorts {
            let members = composition
                .provenance
                .members(SymKind::Sort, &result_sort.name);
            let mut image: Option<Name> = None;
            for member in &members {
                let Some(&index) = input.positions.get(member.node.as_str()) else {
                    return Err(ComposeError::NotACocone(members.clone()));
                };
                let Some(candidate) = input.legs[index].sorts.get(&member.name) else {
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
        Ok(())
    }

    fn mediator_operations(
        composition: &Composition,
        input: &UniversalInput<'_>,
        exhausted: &mut Option<u32>,
        mediator: &mut Morphism,
    ) -> Result<(), ComposeError> {
        for result_op in &composition.theory.ops {
            let members = composition.provenance.members(SymKind::Op, &result_op.name);
            let mut image: Option<OpImage> = None;
            for member in &members {
                let Some(&index) = input.positions.get(member.node.as_str()) else {
                    return Err(ComposeError::NotACocone(members.clone()));
                };
                let source_op = input.nodes[index]
                    .1
                    .ops
                    .iter()
                    .find(|operation| operation.name == member.name)
                    .ok_or_else(|| ComposeError::NotACocone(members.clone()))?;
                let candidate = input.legs[index]
                    .ops
                    .get(&member.name)
                    .ok_or_else(|| ComposeError::NotACocone(members.clone()))?;
                let rebound = match candidate {
                    OpImage::Op(name) => OpImage::Op(name.clone()),
                    OpImage::Term(term) => {
                        let source_binders = source_op.binders();
                        OpImage::Term(subst_term(
                            term,
                            &source_binders,
                            &result_op
                                .inputs
                                .iter()
                                .map(|(name, _, _)| Term::Var(name.clone()))
                                .collect::<Vec<_>>(),
                        ))
                    }
                };
                if let Some(first) = image.as_ref() {
                    let inputs: Vec<Name> = result_op
                        .inputs
                        .iter()
                        .map(|(name, _, _)| name.clone())
                        .collect();
                    match op_images_equal_within(
                        first,
                        &rebound,
                        &inputs,
                        input.target,
                        input.rewrite_budget,
                    ) {
                        Ok(true) => {}
                        Ok(false) => return Err(ComposeError::NotACocone(members.clone())),
                        Err(steps) => *exhausted = Some(steps),
                    }
                }
                image = Some(rebound);
            }
            if let Some(image) = image {
                mediator.ops.insert(result_op.name.clone(), image);
            }
        }
        Ok(())
    }

    fn verify_mediator(
        composition: &Composition,
        input: &UniversalInput<'_>,
        mediator: &Morphism,
        exhausted: &mut Option<u32>,
    ) -> Result<(), ComposeError> {
        let members = || {
            composition
                .theory
                .ops
                .first()
                .map_or_else(Vec::new, |operation| {
                    composition.provenance.members(SymKind::Op, &operation.name)
                })
        };
        if morphism_structure(&composition.theory, input.target, mediator).is_err() {
            return Err(ComposeError::NotACocone(members()));
        }
        match equations_preserved_within(
            &composition.theory,
            input.target,
            mediator,
            input.rewrite_budget,
        ) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ComposeError::NotACocone(members())),
            Err(steps) => {
                *exhausted = Some(steps);
                Ok(())
            }
        }
    }

    fn universal_report(
        base: &CompositionReport,
        target: &Theory,
        exhausted: Option<u32>,
    ) -> CompositionReport {
        let mut report = base.clone();
        let equation_outcome = exhausted.map_or_else(
            || {
                if target.eqs.is_empty() && target.directed_eqs.is_empty() {
                    CheckOutcome::Inapplicable {
                        reason: InapplicableReason::NoEquations,
                    }
                } else {
                    CheckOutcome::Passed
                }
            },
            |steps| CheckOutcome::Incomplete {
                reason: CompositionIncomplete::RewriteBudgetExhausted {
                    equation: "mediator".to_owned(),
                    steps,
                },
            },
        );
        report.checks.push(CheckRecord {
            check: CompositionCheckKind::EquationEntailed,
            subject: CheckSubject::Diagram,
            outcome: equation_outcome,
        });
        report.checks.push(CheckRecord {
            check: CompositionCheckKind::UniversalAgainst,
            subject: CheckSubject::Diagram,
            outcome: exhausted.map_or(CheckOutcome::Passed, |steps| CheckOutcome::Incomplete {
                reason: CompositionIncomplete::RewriteBudgetExhausted {
                    equation: "cocone".to_owned(),
                    steps,
                },
            }),
        });
        report.checks.sort();
        report.status = aggregate_status(&report.checks);
        report
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
                    // A class formed only by the name rule: no two of its
                    // members are related by the edges.
                    let name_rule_only = {
                        let mut labels = std::collections::BTreeSet::new();
                        members
                            .iter()
                            .all(|m| labels.insert(legs.label.get(&(kind, m.clone()))))
                    };
                    let head_signatures_match = {
                        let head_signature = |member: &SymRef| {
                            theory(&member.node)
                                .ops
                                .iter()
                                .find(|operation| operation.name == member.name)
                                .map(|operation| {
                                    let head = |sort: &SortExpr| {
                                        (
                                            class_id(SymKind::Sort, &member.node, sort.head()),
                                            sort_arguments(sort).len(),
                                        )
                                    };
                                    (
                                        operation
                                            .inputs
                                            .iter()
                                            .map(|(_, sort, implicit)| (head(sort), *implicit))
                                            .collect::<Vec<_>>(),
                                        head(&operation.output),
                                    )
                                })
                        };
                        head_signature(&first) == head_signature(&second)
                    };
                    return Err(if mode == Mode::Amalgamate && name_rule_only {
                        ComposeError::NameConflict(kind, first.name.clone(), first, second)
                    } else if kind == SymKind::Sort {
                        let detail =
                            sort_mismatch(nodes, cl, &first, &second, is_implicit(&second));
                        ComposeError::SortSignatureMismatch(first, second, Box::new(detail))
                    } else if head_signatures_match {
                        ComposeError::QuotientNotRepresentable {
                            first,
                            second,
                            reason: NotRepresentableReason::NeedsSortEquation,
                        }
                    } else {
                        ComposeError::OpSignatureMismatch(first, second)
                    });
                }
            }
        }
        Ok(())
    }

    /// The first difference between two members of a sort class, each read
    /// through the classes of its own node: arity, then each parameter's
    /// name-free sort, then kind, then the set of constructor classes. An
    /// implicit second member has arity 0 and no other attribute.
    fn sort_mismatch(
        nodes: &[(Name, Theory)],
        cl: &Classes,
        first: &SymRef,
        second: &SymRef,
        second_implicit: bool,
    ) -> SortMismatch {
        let declared = |member: &SymRef| {
            node_theory(nodes, &member.node)
                .sorts
                .iter()
                .find(|sort| sort.name == member.name)
                .cloned()
        };
        let (Some(left), right) = (declared(first), declared(second)) else {
            return SortMismatch::Closure;
        };
        let Some(right) = right.filter(|_| !second_implicit) else {
            return SortMismatch::Arity {
                first: left.params.len(),
                second: 0,
            };
        };
        if left.params.len() != right.params.len() {
            return SortMismatch::Arity {
                first: left.params.len(),
                second: right.params.len(),
            };
        }
        let read = |member: &SymRef, sort: &Sort| {
            let sc = |n: &str| class_id(cl, SymKind::Sort, &member.node, n);
            let oc = |n: &str| class_id(cl, SymKind::Op, &member.node, n);
            sort_signature(sort, &sc, &oc)
        };
        let (left_signature, right_signature) = (read(first, &left), read(second, &right));
        for (index, (left_param, right_param)) in
            left_signature.3.iter().zip(&right_signature.3).enumerate()
        {
            if left_param != right_param {
                // A sort applied to no arguments is written as the bare
                // sort name, the one spelling of that sort expression.
                let written = |sort: &SortExpr| match sort {
                    SortExpr::App(name, arguments) if arguments.is_empty() => name.clone(),
                    other => other.to_string(),
                };
                return SortMismatch::ParamSort {
                    index,
                    first: written(&left.params[index].1),
                    second: written(&right.params[index].1),
                };
            }
        }
        if left_signature.1 != right_signature.1 {
            return SortMismatch::Kind;
        }
        SortMismatch::Closure
    }

    /// An equation generated by a term-valued leg: its two sides in result
    /// names and its provenance.
    struct Generated {
        lhs: Term,
        rhs: Term,
        source: AddedEquation,
        edge_index: usize,
    }

    type EquationInputs = (Vec<(SymRef, Name, Term, Term)>, Vec<(Name, Generated)>);

    type OrderedEdge<'a> = (usize, &'a (Name, Name, Morphism), Vec<u8>);

    /// The equations of the result before classification: every node
    /// theory's equations renamed into the result (nodes and equations in
    /// input order), and, separately, one generated equation per
    /// term-valued operation image, for the edges in (source, target,
    /// morphism) order and the operations in ascending order.
    fn equation_inputs(
        nodes: &[(Name, Theory)],
        edges: &[(Name, Name, Morphism)],
        res: &dyn Fn(SymKind, &str, &str) -> Name,
    ) -> EquationInputs {
        let theory = |l: &str| node_theory(nodes, l);
        let mut eqs: Vec<(SymRef, Name, Term, Term)> = Vec::new();
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
        let mut ordered: Vec<OrderedEdge<'_>> = edges
            .iter()
            .enumerate()
            .map(|(index, edge)| {
                let mut writer = Writer::default();
                write_morphism(&mut writer, &edge.2);
                (index, edge, writer.bytes)
            })
            .collect();
        ordered.sort_by(|a, b| (&a.1.0, &a.1.1, &a.2).cmp(&(&b.1.0, &b.1.1, &b.2)));
        let mut added = Vec::new();
        for (rank, (edge_index, (s, t, m), _)) in ordered.iter().enumerate() {
            for (o, img) in &m.ops {
                let OpImage::Term(term) = img else { continue };
                let Some(op) = theory(s).ops.into_iter().find(|x| x.name == *o) else {
                    continue;
                };
                let lhs = Term::App(
                    res(SymKind::Op, s, o),
                    op.inputs
                        .iter()
                        .filter(|(_, _, implicit)| *implicit == Implicit::No)
                        .map(|(x, _, _)| Term::Var(x.clone()))
                        .collect(),
                );
                let rhs = rename_term(term, &|n| res(SymKind::Op, t, n));
                added.push((
                    format!("{s}.{o}~{rank}"),
                    Generated {
                        lhs,
                        rhs,
                        source: AddedEquation {
                            edge: rank,
                            op: SymRef {
                                node: s.clone(),
                                name: o.clone(),
                            },
                        },
                        edge_index: *edge_index,
                    },
                ));
            }
        }
        (eqs, added)
    }

    /// A class of equation-like items: its members, its content key, and the
    /// representative item.
    type ItemClass<K, T> = (Vec<SymRef>, K, T);

    /// The classes of one kind of equation-like item, before naming: each
    /// class's members (ascending) and representative, classes ascending by
    /// member list.
    struct Classified<T> {
        kind: SymKind,
        classes: Vec<(Vec<SymRef>, T)>,
    }

    /// Classify named items by content: every item joins the class of equal
    /// content, whatever its name, and a class's representative is the item
    /// of its least member. Under amalgamation a name carried by members of
    /// two classes is then a name conflict: scanning the classes in ascending
    /// order of their member lists, and each class's members in ascending
    /// order, the first member whose name an earlier class holds conflicts
    /// with the first member seen of that name, the two in ascending order.
    fn classify_items<K: PartialEq, T>(
        kind: SymKind,
        mut items: Vec<(SymRef, K, T)>,
        mode: Mode,
    ) -> Result<Classified<T>, ComposeError> {
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let mut classes: Vec<ItemClass<K, T>> = Vec::new();
        for (r, key, item) in items {
            match classes.iter_mut().find(|c| c.1 == key) {
                Some(c) => c.0.push(r),
                None => classes.push((vec![r], key, item)),
            }
        }
        for c in &mut classes {
            c.0.sort();
        }
        classes.sort_by(|a, b| a.0.cmp(&b.0));
        if mode == Mode::Amalgamate {
            let mut seen: Vec<(usize, &SymRef)> = Vec::new();
            for (i, class) in classes.iter().enumerate() {
                for m in &class.0 {
                    match seen.iter().find(|(_, s)| s.name == m.name) {
                        Some(&(j, s)) if j != i => {
                            let (first, second) = if s <= m { (s, m) } else { (m, s) };
                            return Err(ComposeError::NameConflict(
                                kind,
                                m.name.clone(),
                                first.clone(),
                                second.clone(),
                            ));
                        }
                        Some(_) => {}
                        None => seen.push((i, m)),
                    }
                }
            }
        }
        Ok(Classified {
            kind,
            classes: classes.into_iter().map(|(m, _, t)| (m, t)).collect(),
        })
    }

    /// Name classified items as symbols are named, record every member's
    /// result name, and return the named representatives in ascending name
    /// order.
    fn name_items<T>(
        classified: Classified<T>,
        reserved: &BTreeMap<Name, SymRef>,
        mode: Mode,
        clash: ClashPolicy,
        provenance: &mut Provenance,
        freshened: &mut Vec<Freshening>,
    ) -> Result<Vec<(Name, T)>, ComposeError> {
        let kind = classified.kind;
        let members: Vec<Vec<SymRef>> = classified.classes.iter().map(|c| c.0.clone()).collect();
        let names = name_classes(kind, &members, reserved, mode, clash, freshened)?;
        let mut out = Vec::new();
        for ((class, item), n) in classified.classes.into_iter().zip(names) {
            for m in &class {
                provenance.map_mut(kind).insert(m.clone(), n.clone());
            }
            out.push((n, item));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// The equation classes: translations that are alpha-equivalent in
    /// either orientation are one equation in both modes, and under
    /// amalgamation one name carried by two different equations is a
    /// conflict.
    fn equation_classes(
        eqs: &[(SymRef, Name, Term, Term)],
        mode: Mode,
    ) -> Result<Classified<(Term, Term)>, ComposeError> {
        let items = eqs
            .iter()
            .map(|(r, _, l, rr)| {
                (
                    r.clone(),
                    alpha(l, rr).min(alpha(rr, l)),
                    (l.clone(), rr.clone()),
                )
            })
            .collect();
        classify_items(SymKind::Eq, items, mode)
    }

    /// The directed-equation classes: each node's directed equations with
    /// their operations translated along the cocone, classified as equations
    /// are, the content being the alpha-normal sides in their stated
    /// orientation together with the opaque implementation, inverse, value
    /// kinds and classification.
    fn directed_classes(
        nodes: &[(Name, Theory)],
        res: &dyn Fn(SymKind, &str, &str) -> Name,
        mode: Mode,
    ) -> Result<Classified<DirectedEquation>, ComposeError> {
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
                items.push((r, key, translated));
            }
        }
        classify_items(SymKind::DirectedEq, items, mode)
    }

    /// The policy classes: transported unchanged (a policy mentions no sort
    /// or operation), classified by content (value kind and strategy) as
    /// equations are.
    fn policy_classes(
        nodes: &[(Name, Theory)],
        mode: Mode,
    ) -> Result<Classified<Policy>, ComposeError> {
        let mut items = Vec::new();
        for (l, t) in nodes {
            for p in &t.policies {
                let r = SymRef {
                    node: l.clone(),
                    name: p.name.clone(),
                };
                let key = (p.value_kind.clone(), p.strategy.clone());
                items.push((r, key, p.clone()));
            }
        }
        classify_items(SymKind::Policy, items, mode)
    }

    /// Domain separator for canonical theory identities.
    pub const THEORY_CONTEXT: &str = "panproto 2026-09 theory identity v1";
    /// Domain separator for canonical composition identities.
    pub const COMPOSITION_CONTEXT: &str = "panproto 2026-09 theory composition v1";

    fn write_term(writer: &mut Writer, term: &Term) {
        match term {
            Term::Var(name) => {
                writer.tag(0);
                writer.str(name);
            }
            Term::App(operation, arguments) => {
                writer.tag(1);
                writer.str(operation);
                writer.len(arguments.len());
                for argument in arguments {
                    write_term(writer, argument);
                }
            }
            Term::Case(scrutinee, branches) => {
                writer.tag(2);
                write_term(writer, scrutinee);
                writer.len(branches.len());
                for branch in branches {
                    writer.str(&branch.constructor);
                    writer.len(branch.binders.len());
                    for binder in &branch.binders {
                        writer.str(binder);
                    }
                    write_term(writer, &branch.body);
                }
            }
            Term::Hole(name) => {
                writer.tag(3);
                writer.opt_name(name.as_deref());
            }
            Term::Let(name, bound, body) => {
                writer.tag(4);
                writer.str(name);
                write_term(writer, bound);
                write_term(writer, body);
            }
        }
    }

    fn write_sort_expr(writer: &mut Writer, sort: &SortExpr) {
        match sort {
            SortExpr::Name(name) => {
                writer.tag(0);
                writer.str(name);
            }
            SortExpr::App(name, arguments) if arguments.is_empty() => {
                writer.tag(0);
                writer.str(name);
            }
            SortExpr::App(name, arguments) => {
                writer.tag(1);
                writer.str(name);
                writer.len(arguments.len());
                for argument in arguments {
                    write_term(writer, argument);
                }
            }
        }
    }

    fn write_morphism(writer: &mut Writer, morphism: &Morphism) {
        writer.str(&morphism.name);
        writer.str(&morphism.domain);
        writer.str(&morphism.codomain);
        writer.len(morphism.sorts.len());
        for (source, target) in &morphism.sorts {
            writer.str(source);
            writer.str(target);
        }
        writer.len(morphism.ops.len());
        for (source, target) in &morphism.ops {
            writer.str(source);
            match target {
                OpImage::Op(operation) => {
                    writer.tag(0);
                    writer.str(operation);
                }
                OpImage::Term(term) => {
                    writer.tag(1);
                    write_term(writer, term);
                }
            }
        }
    }

    fn write_symref(writer: &mut Writer, symbol: &SymRef) {
        writer.str(&symbol.node);
        writer.str(&symbol.name);
    }

    fn write_check(writer: &mut Writer, record: &CheckRecord) {
        writer.tag(match record.check {
            CompositionCheckKind::LegIsMorphism => 0,
            CompositionCheckKind::NodeWellFormed => 1,
            CompositionCheckKind::ClassSignaturesAgree => 2,
            CompositionCheckKind::CoconeCommutes => 3,
            CompositionCheckKind::InclusionIsMorphism => 4,
            CompositionCheckKind::EquationImage => 5,
            CompositionCheckKind::EquationEntailed => 6,
            CompositionCheckKind::UniversalIdentity => 7,
            CompositionCheckKind::UniversalAgainst => 8,
            CompositionCheckKind::ResultWellTyped => 9,
            CompositionCheckKind::NameClashFree => 10,
        });
        match &record.subject {
            CheckSubject::Diagram => writer.tag(0),
            CheckSubject::Node(node) => {
                writer.tag(1);
                writer.str(node);
            }
            CheckSubject::Edge(index) => {
                writer.tag(2);
                writer.len(*index);
            }
            CheckSubject::Cocone(index) => {
                writer.tag(3);
                writer.len(*index);
            }
        }
        match &record.outcome {
            CheckOutcome::Passed => writer.tag(0),
            CheckOutcome::PassedByEquation { equation } => {
                writer.tag(1);
                writer.str(equation);
            }
            CheckOutcome::Failed { detail } => {
                writer.tag(2);
                writer.str(detail);
            }
            CheckOutcome::Inapplicable { reason } => {
                writer.tag(3);
                match reason {
                    InapplicableReason::OpenTheory { missing_sorts } => {
                        writer.tag(0);
                        writer.len(missing_sorts.len());
                        for sort in missing_sorts {
                            writer.str(sort);
                        }
                    }
                    InapplicableReason::NoEquations => writer.tag(1),
                }
            }
            CheckOutcome::Incomplete { reason } => {
                writer.tag(4);
                match reason {
                    CompositionIncomplete::RewriteBudgetExhausted { equation, steps } => {
                        writer.tag(0);
                        writer.str(equation);
                        writer.u32(*steps);
                    }
                    CompositionIncomplete::NoTerminatingRewriteSystem => writer.tag(1),
                }
            }
        }
    }

    /// Canonical, order-insensitive bytes of a theory. Every sum is tagged,
    /// every sequence is length-prefixed, and declaration vectors are sorted
    /// by name before encoding.
    #[must_use]
    pub fn canonical_theory_bytes(theory: &Theory) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.str(&theory.name);
        writer.len(theory.extends.len());
        for parent in &theory.extends {
            writer.str(parent);
        }
        let mut sorts = theory.sorts.iter().collect::<Vec<_>>();
        sorts.sort_by_key(|sort| &sort.name);
        writer.len(sorts.len());
        for sort in sorts {
            writer.str(&sort.name);
            writer.len(sort.params.len());
            for (name, parameter_sort) in &sort.params {
                writer.str(name);
                write_sort_expr(&mut writer, parameter_sort);
            }
            writer.str(&sort.kind);
            match &sort.constructors {
                None => writer.tag(0),
                Some(constructors) => {
                    writer.tag(1);
                    writer.len(constructors.len());
                    for constructor in constructors {
                        writer.str(constructor);
                    }
                }
            }
        }
        let mut operations = theory.ops.iter().collect::<Vec<_>>();
        operations.sort_by_key(|operation| &operation.name);
        writer.len(operations.len());
        for operation in operations {
            writer.str(&operation.name);
            writer.len(operation.inputs.len());
            for (name, sort, implicit) in &operation.inputs {
                writer.str(name);
                write_sort_expr(&mut writer, sort);
                writer.tag(match implicit {
                    Implicit::No => 0,
                    Implicit::Yes => 1,
                });
            }
            write_sort_expr(&mut writer, &operation.output);
        }
        let mut equations = theory.eqs.iter().collect::<Vec<_>>();
        equations.sort_by_key(|equation| &equation.name);
        writer.len(equations.len());
        for equation in equations {
            writer.str(&equation.name);
            write_term(&mut writer, &equation.lhs);
            write_term(&mut writer, &equation.rhs);
        }
        let mut directed = theory.directed_eqs.iter().collect::<Vec<_>>();
        directed.sort_by_key(|equation| &equation.name);
        writer.len(directed.len());
        for equation in directed {
            writer.str(&equation.name);
            write_term(&mut writer, &equation.lhs);
            write_term(&mut writer, &equation.rhs);
            writer.str(&equation.implementation);
            writer.opt_name(equation.inverse.as_deref());
            writer.opt_name(equation.source_kind.as_deref());
            writer.opt_name(equation.target_kind.as_deref());
            writer.str(&equation.coercion_class);
        }
        let mut policies = theory.policies.iter().collect::<Vec<_>>();
        policies.sort_by_key(|policy| &policy.name);
        writer.len(policies.len());
        for policy in policies {
            writer.str(&policy.name);
            writer.str(&policy.value_kind);
            writer.str(&policy.strategy);
        }
        writer.bytes
    }

    /// Canonical bytes of the four-field composition result.
    #[must_use]
    pub fn canonical_composition_bytes(composition: &Composition) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.raw(&canonical_theory_bytes(&composition.theory));
        writer.len(composition.inclusions.len());
        for inclusion in &composition.inclusions {
            write_morphism(&mut writer, inclusion);
        }
        for (tag, map) in [
            (0, &composition.provenance.sorts),
            (1, &composition.provenance.ops),
            (2, &composition.provenance.eqs),
            (3, &composition.provenance.directed_eqs),
            (4, &composition.provenance.policies),
        ] {
            writer.tag(tag);
            writer.len(map.len());
            for (source, image) in map {
                write_symref(&mut writer, source);
                writer.str(image);
            }
        }
        writer.len(composition.provenance.added_eqs.len());
        for (name, added) in &composition.provenance.added_eqs {
            writer.str(name);
            writer.len(added.edge);
            write_symref(&mut writer, &added.op);
        }
        writer.tag(match composition.report.mode {
            CompositionMode::Amalgamate => 0,
            CompositionMode::ExplicitIdentity => 1,
        });
        writer.tag(match composition.report.status {
            CheckStatus::Passed => 0,
            CheckStatus::Failed => 1,
            CheckStatus::Incomplete => 2,
        });
        writer.tag(u8::from(composition.report.name_clash_free));
        writer.len(composition.report.checks.len());
        for check in &composition.report.checks {
            write_check(&mut writer, check);
        }
        writer.len(composition.report.freshened.len());
        for freshening in &composition.report.freshened {
            writer.tag(match freshening.kind {
                SymKind::Sort => 0,
                SymKind::Op => 1,
                SymKind::Eq => 2,
                SymKind::DirectedEq => 3,
                SymKind::Policy => 4,
            });
            writer.len(freshening.members.len());
            for member in &freshening.members {
                write_symref(&mut writer, member);
            }
            writer.str(&freshening.candidate);
            writer.str(&freshening.result);
        }
        writer.bytes
    }

    /// Canonical bytes of a versioned composition recipe, including every
    /// mode, clash, budget, inline theory and derived operation image.
    #[must_use]
    pub fn canonical_composition_spec_bytes(spec: &CompositionSpec) -> Vec<u8> {
        let mut writer = Writer::default();
        writer.str(&spec.result_name);
        writer.u32(u32::from(spec.version));
        writer.len(spec.steps.len());
        for step in &spec.steps {
            match step {
                CompositionStep::Base(name) => {
                    writer.tag(0);
                    writer.str(name);
                }
                CompositionStep::Colimit {
                    left,
                    right,
                    shared_sorts,
                    shared_ops,
                    mode,
                } => {
                    writer.tag(1);
                    writer.str(left);
                    writer.str(right);
                    writer.len(shared_sorts.len());
                    for sort in shared_sorts {
                        writer.str(sort);
                    }
                    writer.len(shared_ops.len());
                    for operation in shared_ops {
                        writer.str(operation);
                    }
                    writer.tag(match mode {
                        CompositionMode::Amalgamate => 0,
                        CompositionMode::ExplicitIdentity => 1,
                    });
                }
                CompositionStep::Diagram {
                    nodes,
                    edges,
                    mode,
                    clash,
                    rewrite_budget,
                    inline,
                } => {
                    writer.tag(2);
                    writer.len(nodes.len());
                    for node in nodes {
                        writer.str(node);
                    }
                    writer.len(edges.len());
                    for edge in edges {
                        writer.str(&edge.src);
                        writer.str(&edge.tgt);
                        writer.len(edge.sorts.len());
                        for (source, target) in &edge.sorts {
                            writer.str(source);
                            writer.str(target);
                        }
                        writer.len(edge.ops.len());
                        for (source, target) in &edge.ops {
                            writer.str(source);
                            match target {
                                RecipeOpImage::Op(operation) => {
                                    writer.tag(0);
                                    writer.str(operation);
                                }
                                RecipeOpImage::Term(term) => {
                                    writer.tag(1);
                                    write_term(&mut writer, term);
                                }
                            }
                        }
                    }
                    writer.tag(match mode {
                        CompositionMode::Amalgamate => 0,
                        CompositionMode::ExplicitIdentity => 1,
                    });
                    writer.tag(match clash {
                        ClashPolicy::Freshen => 0,
                        ClashPolicy::Reject => 1,
                    });
                    writer.u32(*rewrite_budget);
                    writer.len(inline.len());
                    for theory in inline {
                        writer.raw(&canonical_theory_bytes(theory));
                    }
                }
            }
        }
        writer.bytes
    }

    #[cfg(test)]
    mod tests {
        //! Worked examples that pin single clauses of the definitions above.

        use super::*;

        fn simple(name: &str) -> Sort {
            Sort {
                name: name.to_owned(),
                params: Vec::new(),
                kind: "structural".to_owned(),
                constructors: None,
            }
        }

        fn var(name: &str) -> Term {
            Term::Var(name.to_owned())
        }

        fn binary(name: &str) -> Op {
            Op {
                name: name.to_owned(),
                inputs: vec![
                    ("x".to_owned(), SortExpr::Name("A".to_owned()), Implicit::No),
                    ("y".to_owned(), SortExpr::Name("A".to_owned()), Implicit::No),
                ],
                output: SortExpr::Name("A".to_owned()),
            }
        }

        fn sort_map() -> BTreeMap<Name, Name> {
            BTreeMap::from([("A".to_owned(), "A".to_owned())])
        }

        #[test]
        fn an_equation_relating_two_distinct_variables_is_not_preserved_without_an_axiom() {
            let domain = Theory {
                name: "D".to_owned(),
                sorts: vec![simple("A")],
                eqs: vec![Equation {
                    name: "e".to_owned(),
                    lhs: var("x"),
                    rhs: var("y"),
                }],
                ..Theory::default()
            };
            let codomain = Theory {
                name: "C".to_owned(),
                sorts: vec![simple("A")],
                ..Theory::default()
            };
            let m = Morphism {
                name: "m".to_owned(),
                domain: "D".to_owned(),
                codomain: "C".to_owned(),
                sorts: sort_map(),
                ops: BTreeMap::new(),
            };
            assert_eq!(
                check_morphism(&domain, &codomain, &m),
                Err(MorphismError::EquationNotPreserved("e".to_owned()))
            );
            let mut same = domain;
            same.eqs[0].rhs = var("x");
            assert_eq!(check_morphism(&same, &codomain, &m), Ok(()));
        }

        #[test]
        fn a_cocone_sending_identified_operations_to_different_inputs_is_refused() {
            let s = Theory {
                name: "S".to_owned(),
                sorts: vec![simple("A")],
                ops: vec![binary("o")],
                ..Theory::default()
            };
            let t = Theory {
                name: "T".to_owned(),
                sorts: vec![simple("A")],
                ops: vec![binary("p"), binary("q")],
                ..Theory::default()
            };
            let leg = |image: &str| Morphism {
                name: format!("o_to_{image}"),
                domain: "S".to_owned(),
                codomain: "T".to_owned(),
                sorts: sort_map(),
                ops: BTreeMap::from([("o".to_owned(), OpImage::Op(image.to_owned()))]),
            };
            let diagram = TheoryDiagram::parallel(
                ("S".to_owned(), s),
                ("T".to_owned(), t),
                leg("p"),
                leg("q"),
            );
            let options = ComposeOptions {
                mode: CompositionMode::ExplicitIdentity,
                result_name: "R".to_owned(),
                clash: ClashPolicy::Reject,
                rewrite_budget: 512,
            };
            let composition = compose(&diagram, &options)
                .unwrap_or_else(|e| panic!("the parallel pair composes: {e:?}"));
            let q = Theory {
                name: "Q".to_owned(),
                sorts: vec![simple("A")],
                ..Theory::default()
            };
            let to_q = |images: &[(&str, &str)]| Morphism {
                name: "to_q".to_owned(),
                domain: "T".to_owned(),
                codomain: "Q".to_owned(),
                sorts: sort_map(),
                ops: images
                    .iter()
                    .map(|(o, v)| ((*o).to_owned(), OpImage::Term(var(v))))
                    .collect(),
            };
            let legs = [to_q(&[("o", "x")]), to_q(&[("p", "x"), ("q", "y")])];
            assert!(matches!(
                composition.verify_universal(&diagram, &q, &legs),
                Err(ComposeError::NotACocone(_))
            ));
            let agreeing = [to_q(&[("o", "x")]), to_q(&[("p", "x"), ("q", "x")])];
            assert!(
                composition
                    .verify_universal(&diagram, &q, &agreeing)
                    .is_ok()
            );
        }

        fn unary(name: &str) -> Op {
            Op {
                name: name.to_owned(),
                inputs: vec![("x".to_owned(), SortExpr::Name("A".to_owned()), Implicit::No)],
                output: SortExpr::Name("A".to_owned()),
            }
        }

        fn app(op: &str, args: Vec<Term>) -> Term {
            Term::App(op.to_owned(), args)
        }

        /// A theory with the sort `A`, the operation `f : A -> A` and one
        /// equation.
        fn with_equation(theory: &str, name: &str, lhs: Term, rhs: Term) -> Theory {
            Theory {
                name: theory.to_owned(),
                sorts: vec![simple("A")],
                ops: vec![unary("f")],
                eqs: vec![Equation {
                    name: name.to_owned(),
                    lhs,
                    rhs,
                }],
                ..Theory::default()
            }
        }

        fn options(mode: CompositionMode) -> ComposeOptions {
            ComposeOptions {
                mode,
                result_name: "R".to_owned(),
                clash: ClashPolicy::Reject,
                rewrite_budget: 512,
            }
        }

        fn sym(node: &str, name: &str) -> SymRef {
            SymRef {
                node: node.to_owned(),
                name: name.to_owned(),
            }
        }

        /// The by-name span of `left` and `right` over the shared `A` and `f`.
        fn shared_span(left: (String, Theory), right: (String, Theory)) -> TheoryDiagram {
            let shared = Theory {
                name: "TS".to_owned(),
                sorts: vec![simple("A")],
                ops: vec![unary("f")],
                ..Theory::default()
            };
            TheoryDiagram::by_name(("S".to_owned(), shared), left, right)
                .unwrap_or_else(|e| panic!("both sides declare A and f: {e:?}"))
        }

        #[test]
        fn alpha_equivalent_equations_under_different_names_are_one_axiom_in_every_mode() {
            let diagram = shared_span(
                (
                    "L".to_owned(),
                    with_equation("TL", "e1", app("f", vec![var("x")]), var("x")),
                ),
                (
                    "R".to_owned(),
                    with_equation("TR", "e2", app("f", vec![var("y")]), var("y")),
                ),
            );
            for mode in [
                CompositionMode::Amalgamate,
                CompositionMode::ExplicitIdentity,
            ] {
                let composition = compose(&diagram, &options(mode))
                    .unwrap_or_else(|e| panic!("the two theories compose: {e:?}"));
                let names: Vec<&str> = composition
                    .theory
                    .eqs
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect();
                assert_eq!(names, ["e1"], "{mode:?}");
                assert_eq!(
                    composition.provenance.eqs,
                    BTreeMap::from([
                        (sym("L", "e1"), "e1".to_owned()),
                        (sym("R", "e2"), "e1".to_owned()),
                    ]),
                    "{mode:?}"
                );
            }
        }

        #[test]
        fn an_equation_and_its_reverse_are_one_axiom() {
            let diagram = shared_span(
                (
                    "L".to_owned(),
                    with_equation("TL", "e", app("f", vec![var("x")]), var("x")),
                ),
                (
                    "R".to_owned(),
                    with_equation("TR", "e", var("y"), app("f", vec![var("y")])),
                ),
            );
            for mode in [
                CompositionMode::Amalgamate,
                CompositionMode::ExplicitIdentity,
            ] {
                let composition = compose(&diagram, &options(mode))
                    .unwrap_or_else(|e| panic!("the two theories compose: {e:?}"));
                assert_eq!(composition.theory.eqs.len(), 1, "{mode:?}");
                assert_eq!(composition.theory.eqs[0].lhs, app("f", vec![var("x")]));
            }
        }

        #[test]
        fn a_nullary_sort_and_its_empty_application_are_one_sort() {
            let constant = |theory: &str, output: SortExpr| Theory {
                name: theory.to_owned(),
                sorts: vec![simple("A")],
                ops: vec![Op {
                    name: "k".to_owned(),
                    inputs: Vec::new(),
                    output,
                }],
                ..Theory::default()
            };
            let diagram = TheoryDiagram::discrete(vec![
                (
                    "L".to_owned(),
                    constant("TL", SortExpr::Name("A".to_owned())),
                ),
                (
                    "R".to_owned(),
                    constant("TR", SortExpr::App("A".to_owned(), Vec::new())),
                ),
            ]);
            let composition = compose(&diagram, &options(CompositionMode::Amalgamate))
                .unwrap_or_else(|e| panic!("`A` and `A()` are one sort: {e:?}"));
            assert_eq!(composition.theory.ops.len(), 1);
        }

        #[test]
        fn an_unapplied_and_an_applied_dependent_output_differ_in_shape() {
            let constant = |output: SortExpr| Op {
                name: "member".to_owned(),
                inputs: Vec::new(),
                output,
            };
            let unapplied = constant(SortExpr::Name("Y".to_owned()));
            let applied = constant(SortExpr::App(
                "Y".to_owned(),
                vec![Term::App("a".to_owned(), Vec::new())],
            ));
            let diagram = crate::enumerate::theories::operation_class_pair(&unapplied, &applied);
            assert_eq!(
                compose(&diagram, &options(CompositionMode::Amalgamate)).err(),
                Some(ComposeError::OpSignatureMismatch(
                    sym("l", "lead"),
                    sym("r", "member")
                ))
            );
        }

        #[test]
        fn a_disagreeing_pair_of_different_names_is_not_a_name_conflict() {
            // `M.r` is renamed to `L.p` by the edge and glued to `R.r` by name;
            // `L.p` and `R.r` differ only in the order of `H`'s arguments, and
            // they do not share a name.
            let hom = |theory: &str, op: &str, swapped: bool| {
                let args = if swapped {
                    vec![var("y"), var("x")]
                } else {
                    vec![var("x"), var("y")]
                };
                Theory {
                    name: theory.to_owned(),
                    sorts: vec![
                        simple("O"),
                        Sort {
                            params: vec![
                                ("a".to_owned(), SortExpr::Name("O".to_owned())),
                                ("b".to_owned(), SortExpr::Name("O".to_owned())),
                            ],
                            ..simple("H")
                        },
                    ],
                    ops: vec![Op {
                        name: op.to_owned(),
                        inputs: vec![
                            ("x".to_owned(), SortExpr::Name("O".to_owned()), Implicit::No),
                            ("y".to_owned(), SortExpr::Name("O".to_owned()), Implicit::No),
                        ],
                        output: SortExpr::App("H".to_owned(), args),
                    }],
                    ..Theory::default()
                }
            };
            let mut diagram = TheoryDiagram::discrete(vec![
                ("l".to_owned(), hom("TL", "p", false)),
                ("m".to_owned(), hom("TM", "r", false)),
                ("r".to_owned(), hom("TR", "r", true)),
            ]);
            diagram.edges.push(DiagramEdge {
                src: "m".to_owned(),
                tgt: "l".to_owned(),
                morphism: Morphism {
                    name: "m_to_l".to_owned(),
                    domain: "TM".to_owned(),
                    codomain: "TL".to_owned(),
                    sorts: BTreeMap::from([
                        ("O".to_owned(), "O".to_owned()),
                        ("H".to_owned(), "H".to_owned()),
                    ]),
                    ops: BTreeMap::from([("r".to_owned(), OpImage::Op("p".to_owned()))]),
                },
            });
            assert_eq!(
                compose(&diagram, &options(CompositionMode::Amalgamate)).err(),
                Some(ComposeError::QuotientNotRepresentable {
                    first: sym("l", "p"),
                    second: sym("r", "r"),
                    reason: NotRepresentableReason::NeedsSortEquation,
                })
            );
        }

        /// The BLAKE3 digest, as lowercase hex, of the debug rendering of
        /// every item, one per line.
        fn digest<T: std::fmt::Debug>(items: impl IntoIterator<Item = T>) -> String {
            let mut hasher = crate::blake3_ref::Hasher::new_derive_key(
                "panproto-reference theory outcome digest",
            );
            for item in items {
                hasher.update(format!("{item:?}\n").as_bytes());
            }
            hasher
                .finalize()
                .iter()
                .fold(String::new(), |mut hex, byte| {
                    let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{byte:02x}"));
                    hex
                })
        }

        fn assert_digest<T: std::fmt::Debug>(
            what: &str,
            items: impl IntoIterator<Item = T>,
            expected: &str,
        ) {
            assert_eq!(digest(items), expected, "outcome digest of {what}");
        }

        fn name(head: &str) -> SortExpr {
            SortExpr::Name(head.to_owned())
        }

        fn sort_app(head: &str, args: Vec<Term>) -> SortExpr {
            SortExpr::App(head.to_owned(), args)
        }

        fn constant(op: &str) -> Term {
            Term::App(op.to_owned(), Vec::new())
        }

        fn branch(constructor: &str, binders: &[&str], body: Term) -> CaseBranch {
            CaseBranch {
                constructor: constructor.to_owned(),
                binders: binders.iter().map(|binder| (*binder).to_owned()).collect(),
                body,
            }
        }

        fn case(scrutinee: Term, branches: Vec<CaseBranch>) -> Term {
            Term::Case(Box::new(scrutinee), branches)
        }

        fn local(binder: &str, bound: Term, body: Term) -> Term {
            Term::Let(binder.to_owned(), Box::new(bound), Box::new(body))
        }

        fn operation(name: &str, inputs: &[(&str, SortExpr, Implicit)], output: SortExpr) -> Op {
            Op {
                name: name.to_owned(),
                inputs: inputs
                    .iter()
                    .map(|(input, sort, implicit)| ((*input).to_owned(), sort.clone(), *implicit))
                    .collect(),
                output,
            }
        }

        fn explicit(input: &str, sort: SortExpr) -> (&str, SortExpr, Implicit) {
            (input, sort, Implicit::No)
        }

        fn implicit(input: &str, sort: SortExpr) -> (&str, SortExpr, Implicit) {
            (input, sort, Implicit::Yes)
        }

        fn dependent(sort: &str, params: &[(&str, SortExpr)]) -> Sort {
            Sort {
                params: params
                    .iter()
                    .map(|(param, expr)| ((*param).to_owned(), expr.clone()))
                    .collect(),
                ..simple(sort)
            }
        }

        fn theory(name: &str, sorts: Vec<Sort>, ops: Vec<Op>) -> Theory {
            Theory {
                name: name.to_owned(),
                sorts,
                ops,
                ..Theory::default()
            }
        }

        fn identity_on(domain: &Theory, codomain: &Theory) -> Morphism {
            Morphism {
                name: "id".to_owned(),
                domain: domain.name.clone(),
                codomain: codomain.name.clone(),
                sorts: domain
                    .sorts
                    .iter()
                    .map(|sort| (sort.name.clone(), sort.name.clone()))
                    .collect(),
                ops: domain
                    .ops
                    .iter()
                    .map(|op| (op.name.clone(), OpImage::Op(op.name.clone())))
                    .collect(),
            }
        }

        #[test]
        fn terms_and_sort_expressions_display_in_their_written_form() {
            let probe = case(
                var("b"),
                vec![
                    branch("t", &[], constant("f")),
                    branch("pair", &["p", "q"], app("m", vec![var("p"), var("q")])),
                    branch("one", &["u"], var("u")),
                ],
            );
            assert_eq!(
                probe.to_string(),
                "case b of t() => f() | pair(p, q) => m(p, q) | one(u) => u end"
            );
            assert_eq!(app("f", vec![var("x")]).to_string(), "f(x)");
            assert_eq!(
                app("q", vec![constant("d"), app("r", vec![var("x"), var("y")])]).to_string(),
                "q(d(), r(x, y))"
            );
            assert_eq!(Term::Hole(Some("h".to_owned())).to_string(), "?h");
            assert_eq!(Term::Hole(None).to_string(), "?");
            assert_eq!(
                local("n", constant("c"), app("q", vec![var("n"), constant("d")])).to_string(),
                "let n = c() in q(n, d())"
            );
            assert_eq!(name("A").to_string(), "A");
            assert_eq!(sort_app("X", Vec::new()).to_string(), "X()");
            assert_eq!(sort_app("Y", vec![var("x")]).to_string(), "Y(x)");
            assert_eq!(
                sort_app("H", vec![var("x"), constant("c"), var("y")]).to_string(),
                "H(x, c(), y)"
            );
        }

        #[test]
        fn sort_expressions_hash_and_order_by_head_then_arguments() {
            use std::hash::{Hash, Hasher};
            let hash = |sort: &SortExpr| {
                let mut state = std::collections::hash_map::DefaultHasher::new();
                sort.hash(&mut state);
                state.finish()
            };
            assert_eq!(hash(&name("A")), hash(&sort_app("A", Vec::new())));
            assert_ne!(hash(&name("A")), hash(&name("B")));
            assert_ne!(
                hash(&sort_app("B", vec![var("x")])),
                hash(&sort_app("B", vec![var("y")]))
            );
            assert_eq!(
                name("A").partial_cmp(&name("B")),
                Some(std::cmp::Ordering::Less)
            );
            assert_eq!(
                sort_app("B", vec![var("x")]).partial_cmp(&name("B")),
                Some(std::cmp::Ordering::Greater)
            );
            assert!(name("A") < sort_app("A", vec![var("x")]));
        }

        #[test]
        fn occurrence_respects_local_and_branch_binders() {
            let x = || var("x");
            let c = || constant("c");
            assert!(occurs_in("x", &app("f", vec![c(), x()])));
            assert!(!occurs_in("x", &Term::Hole(Some("x".to_owned()))));
            assert!(occurs_in("x", &local("y", x(), c())), "in the bound term");
            assert!(occurs_in("x", &local("y", c(), x())), "in the body");
            assert!(
                !occurs_in("x", &local("x", c(), x())),
                "shadowed in the body"
            );
            assert!(!occurs_in("x", &local("y", c(), c())), "absent");
            assert!(occurs_in("x", &case(x(), Vec::new())), "in the scrutinee");
            assert!(
                occurs_in("x", &case(c(), vec![branch("k", &["p"], x())])),
                "free in a branch"
            );
            assert!(
                !occurs_in("x", &case(c(), vec![branch("k", &["x"], x())])),
                "bound by the branch"
            );
            assert!(
                !occurs_in("x", &case(c(), vec![branch("k", &["p"], c())])),
                "absent from the branch"
            );
        }

        #[test]
        fn unification_solves_variables_and_refuses_clashes() {
            let x = || var("x");
            assert_eq!(unify_terms(vec![(x(), x())]), Ok(BTreeMap::new()));
            assert_eq!(
                unify_terms(vec![(
                    app("f", vec![x(), constant("c")]),
                    app("f", vec![constant("d"), var("y")])
                )]),
                Ok(BTreeMap::from([
                    ("x".to_owned(), constant("d")),
                    ("y".to_owned(), constant("c")),
                ]))
            );
            assert_eq!(
                unify_terms(vec![(app("f", vec![x()]), app("g", vec![x()]))]),
                Err("cannot unify f(x) with g(x)".to_owned())
            );
            assert_eq!(
                unify_terms(vec![(app("f", vec![x()]), app("f", vec![x(), var("y")]))]),
                Err("cannot unify f(x) with f(x, y)".to_owned())
            );
            assert_eq!(
                unify_terms(vec![(x(), app("f", vec![x()]))]),
                Err("occurs check failed: x in f(x)".to_owned())
            );
            assert_eq!(
                unify_sorts(
                    &sort_app("B", vec![x()]),
                    &sort_app("B", vec![x(), var("y")])
                ),
                Err("cannot unify B(x) with B(x, y)".to_owned())
            );
            assert_eq!(
                unify_sorts(&name("A"), &name("B")),
                Err("cannot unify A with B".to_owned())
            );
            assert_eq!(
                unify_sorts(
                    &sort_app("B", vec![x()]),
                    &sort_app("B", vec![constant("c")])
                ),
                Ok(BTreeMap::from([("x".to_owned(), constant("c"))]))
            );
        }

        /// Sorts `A`, `B`, `N`, `W(n : N)` and `U(m : N)`; operations
        /// `a : A`, `z : N`, `g(x : A, y : B) : A`, `k({n : N}, w : W(n)) : N`
        /// and `u : U(z())`.
        fn application_theory() -> Theory {
            let n = || name("N");
            theory(
                "T",
                vec![
                    simple("A"),
                    simple("B"),
                    simple("N"),
                    dependent("W", &[("n", n())]),
                    dependent("U", &[("m", n())]),
                ],
                vec![
                    operation("a", &[], name("A")),
                    operation("z", &[], n()),
                    operation(
                        "g",
                        &[explicit("x", name("A")), explicit("y", name("B"))],
                        name("A"),
                    ),
                    operation(
                        "k",
                        &[
                            implicit("n", n()),
                            explicit("w", sort_app("W", vec![var("n")])),
                        ],
                        n(),
                    ),
                    operation("u", &[], sort_app("U", vec![constant("z")])),
                ],
            )
        }

        #[test]
        fn applications_are_typed_argument_by_argument() {
            let t = application_theory();
            let op = |name: &str| {
                find_op(&t, name).unwrap_or_else(|| panic!("the theory declares {name}"))
            };
            assert_eq!(
                application_sort(&t, &[], op("g"), &[constant("a"), constant("a")]),
                Err("type mismatch for g argument 1: expected B, got A".to_owned())
            );
            assert_eq!(
                application_sort(&t, &[], op("g"), &[constant("a")]),
                Err("arity mismatch for operation g: expected 2 args, got 1".to_owned())
            );
            assert_eq!(
                application_sort(&t, &[], op("k"), &[constant("u")]),
                Err("type mismatch for k argument 0: expected W(?n_0), got U(z())".to_owned())
            );
            let context = [("w".to_owned(), sort_app("W", vec![constant("z")]))];
            assert_eq!(
                format!("{:?}", application_sort(&t, &context, op("k"), &[var("w")])),
                "Ok(Name(\"N\"))"
            );
        }

        #[test]
        fn a_branch_refinement_is_stated_over_the_branch_binders() {
            let cons = operation(
                "cons",
                &[explicit("n", name("N"))],
                sort_app("V", vec![app("s", vec![var("n")])]),
            );
            let refinement = branch_refinement(
                &cons,
                &["j".to_owned()],
                &var("xs"),
                &sort_app("V", vec![app("s", vec![var("k")])]),
            )
            .unwrap_or_else(|error| panic!("the constructor's output unifies: {error}"));
            assert_eq!(
                refinement.substitution,
                BTreeMap::from([
                    ("j".to_owned(), var("k")),
                    ("xs".to_owned(), app("cons", vec![var("j")])),
                ])
            );
            assert_eq!(
                format!("{:?}", refinement.binder_sorts),
                "[(\"j\", Name(\"N\"))]"
            );
        }

        #[test]
        fn equation_typing_reports_the_first_failing_side() {
            let t = theory(
                "T",
                vec![simple("A"), simple("B")],
                vec![
                    operation("f", &[explicit("x", name("A"))], name("A")),
                    operation("g", &[explicit("x", name("B"))], name("A")),
                ],
            );
            let x = || var("x");
            assert_eq!(equation_well_typed(&t, &app("f", vec![x()]), &x()), Ok(()));
            assert_eq!(
                equation_well_typed(&t, &app("f", vec![x()]), &app("g", vec![x()])),
                Err("variable x is constrained at both Name(\"A\") and Name(\"B\")".to_owned())
            );
            assert_eq!(
                equation_well_typed(&t, &x(), &var("y")),
                Err("the left side is not well typed: unbound variable: x".to_owned())
            );
            assert_eq!(
                equation_well_typed(&t, &case(var("w"), Vec::new()), &var("y")),
                Err("the right side is not well typed: unbound variable: y".to_owned())
            );
        }

        #[test]
        fn an_implicit_input_must_be_mentioned_by_an_explicit_input_or_the_output() {
            let with = |op: Op| {
                theory(
                    "T",
                    vec![
                        simple("A"),
                        simple("N"),
                        dependent("V", &[("n", name("N"))]),
                    ],
                    vec![operation("z", &[], name("N")), op],
                )
            };
            let v = || sort_app("V", vec![var("n")]);
            assert_eq!(
                typecheck_theory_body(&with(operation(
                    "len",
                    &[implicit("n", name("N")), explicit("v", v())],
                    name("N"),
                ))),
                Ok(())
            );
            assert_eq!(
                typecheck_theory_body(&with(operation("out", &[implicit("n", name("N"))], v()))),
                Ok(())
            );
            assert_eq!(
                typecheck_theory_body(&with(operation(
                    "bad",
                    &[implicit("n", name("N")), implicit("m", v())],
                    name("N"),
                ))),
                Err("implicit parameter n of operation bad cannot be inferred".to_owned())
            );
            assert_eq!(
                typecheck_theory_body(&with(operation(
                    "bad",
                    &[implicit("n", name("N")), explicit("x", name("A"))],
                    name("A"),
                ))),
                Err("implicit parameter n of operation bad cannot be inferred".to_owned())
            );
        }

        #[test]
        fn substitution_renames_capturing_binders_apart() {
            let avoid = BTreeSet::from(["x".to_owned(), "x'".to_owned()]);
            assert_eq!(fresh_binder("x", &avoid), "x''");
            assert_eq!(fresh_binder("y", &avoid), "y");
            let substitute = |term: &Term, binder: &str, argument: Term| {
                subst_term(term, &[(binder.to_owned(), name(""))], &[argument])
            };
            assert_eq!(
                substitute(
                    &local("y", constant("c"), app("f", vec![var("x"), var("y")])),
                    "x",
                    var("y")
                ),
                local("y'", constant("c"), app("f", vec![var("y"), var("y'")]))
            );
            assert_eq!(
                substitute(&local("z", constant("c"), var("y")), "y", constant("d")),
                local("z", constant("c"), constant("d"))
            );
            assert_eq!(
                substitute(
                    &case(
                        var("x"),
                        vec![branch("k", &["y"], app("f", vec![var("x"), var("y")]))]
                    ),
                    "x",
                    var("y")
                ),
                case(
                    var("y"),
                    vec![branch("k", &["y'"], app("f", vec![var("y"), var("y'")]))]
                )
            );
            assert_eq!(
                substitute(&local("x", var("x"), var("x")), "x", constant("d")),
                local("x", constant("d"), var("x"))
            );
        }

        #[test]
        fn the_first_unbound_variable_respects_local_and_branch_binders() {
            let context = [("x".to_owned(), name("A"))];
            let unbound_in = |term: &Term| unbound(&context, term);
            assert_eq!(unbound_in(&var("z")), Some("z".to_owned()));
            assert_eq!(unbound_in(&app("f", vec![var("x")])), None);
            assert_eq!(unbound_in(&local("y", var("x"), var("y"))), None);
            assert_eq!(
                unbound_in(&local("y", var("x"), var("w"))),
                Some("w".to_owned())
            );
            assert_eq!(
                unbound_in(&case(var("x"), vec![branch("k", &["b"], var("b"))])),
                None
            );
            assert_eq!(
                unbound_in(&case(var("x"), vec![branch("k", &["b"], var("w"))])),
                Some("w".to_owned())
            );
            assert_eq!(
                unbound_in(&app("f", vec![var("v"), var("w")])),
                Some("v".to_owned())
            );
        }

        #[test]
        fn a_parameter_argument_of_a_typeable_wrong_sort_is_ill_typed() {
            let t = theory(
                "T",
                vec![simple("A"), dependent("B", &[("p", name("A"))])],
                vec![operation(
                    "f",
                    &[explicit("x", name("B"))],
                    sort_app("B", vec![var("x")]),
                )],
            );
            assert_eq!(
                typecheck_signature(&t),
                Err(vec![Defect::ParamIllTyped(
                    Site::OpOutput("f".to_owned()),
                    "B".to_owned(),
                    0,
                    "A".to_owned(),
                    "B".to_owned(),
                )])
            );
        }

        #[test]
        fn dependency_order_admits_chains_through_undeclared_and_later_sorts() {
            let chain = theory(
                "T",
                vec![
                    dependent("B", &[("p", name("C"))]),
                    dependent("D", &[("q", name("B"))]),
                ],
                Vec::new(),
            );
            assert_eq!(
                sorts_in_dependency_order(&chain),
                BTreeSet::from(["B".to_owned(), "D".to_owned()])
            );
            let later = theory(
                "T",
                vec![dependent("D", &[("q", name("B"))]), simple("B")],
                Vec::new(),
            );
            assert_eq!(
                sorts_in_dependency_order(&later),
                BTreeSet::from(["B".to_owned(), "D".to_owned()])
            );
            assert_eq!(
                typecheck_signature(&chain),
                Err(vec![Defect::UndeclaredSort(
                    Site::SortParam("B".to_owned(), 0),
                    "C".to_owned()
                )])
            );
        }

        #[test]
        fn rewrite_rules_match_up_to_bound_names_holes_and_branches() {
            let matches = |pattern: &Term, value: &Term| {
                let mut substitution = BTreeMap::new();
                let matched = match_rule(pattern, value, &mut substitution);
                (matched, substitution)
            };
            let a = || constant("a");
            let none = BTreeMap::new;
            assert_eq!(
                matches(&local("u", a(), var("u")), &local("v", a(), var("v"))),
                (true, none())
            );
            assert!(!matches(&local("u", a(), var("u")), &local("v", a(), var("w"))).0);
            assert!(
                !matches(
                    &local("u", a(), var("u")),
                    &local("v", constant("b"), var("v"))
                )
                .0
            );
            assert_eq!(
                matches(
                    &app("f", vec![var("x"), var("x")]),
                    &app("f", vec![a(), a()])
                ),
                (true, BTreeMap::from([("x".to_owned(), a())]))
            );
            assert!(
                !matches(
                    &app("f", vec![var("x"), var("x")]),
                    &app("f", vec![a(), constant("b")])
                )
                .0
            );
            assert!(!matches(&app("f", vec![var("x")]), &app("g", vec![a()])).0);
            assert!(!matches(&app("f", vec![var("x")]), &app("f", vec![a(), a()])).0);
            let hole = |name: Option<&str>| Term::Hole(name.map(str::to_owned));
            assert_eq!(matches(&hole(Some("h")), &hole(Some("h"))), (true, none()));
            assert!(!matches(&hole(None), &hole(Some("h"))).0);
            let identity_case = |scrutinee: Term, constructor: &str, binder: &str| {
                case(scrutinee, vec![branch(constructor, &[binder], var(binder))])
            };
            assert_eq!(
                matches(
                    &identity_case(constant("c"), "k", "p"),
                    &identity_case(constant("c"), "k", "q")
                ),
                (true, none())
            );
            assert!(
                !matches(
                    &identity_case(constant("c"), "k", "p"),
                    &identity_case(constant("d"), "k", "q")
                )
                .0
            );
            assert!(
                !matches(
                    &identity_case(constant("c"), "k", "p"),
                    &identity_case(constant("c"), "m", "q")
                )
                .0
            );
            assert!(
                !matches(
                    &case(var("x"), vec![branch("k", &[], a())]),
                    &case(var("y"), vec![branch("k", &[], a()), branch("m", &[], a())])
                )
                .0
            );
            assert!(
                !matches(
                    &case(constant("c"), vec![branch("k", &["p"], a())]),
                    &case(constant("c"), vec![branch("k", &[], a())])
                )
                .0
            );
        }

        #[test]
        fn term_depth_counts_every_constructor_level() {
            assert_eq!(term_depth(&var("x")), 1);
            assert_eq!(term_depth(&Term::Hole(None)), 1);
            assert_eq!(term_depth(&constant("c")), 1);
            assert_eq!(term_depth(&app("f", vec![var("x")])), 2);
            assert_eq!(
                term_depth(&app("f", vec![var("x"), app("g", vec![var("y")])])),
                3
            );
            assert_eq!(
                term_depth(&local("n", var("x"), app("f", vec![var("n")]))),
                3
            );
            assert_eq!(
                term_depth(&case(
                    var("x"),
                    vec![branch("k", &[], app("f", vec![var("x")]))]
                )),
                3
            );
            assert_eq!(term_depth(&case(app("f", vec![var("x")]), Vec::new())), 3);
        }

        #[test]
        fn bounded_normalization_reduces_and_stops_at_its_bounds() {
            let x = || var("x");
            let growth = vec![(app("g", vec![x()]), app("g", vec![app("f", vec![x()])]))];
            assert_eq!(
                normalize_bounded(&app("g", vec![constant("a")]), &growth, 512),
                Err(66),
                "g(f^k(a())) has depth k + 2, refused once it exceeds 3 + 64"
            );
            assert_eq!(
                normalize_bounded(&app("g", vec![constant("a")]), &growth, 10),
                Err(10)
            );
            assert_eq!(
                normalize_bounded(
                    &case(
                        app("c", vec![constant("a")]),
                        vec![branch("c", &["p"], var("p"))]
                    ),
                    &[],
                    8
                ),
                Ok(constant("a"))
            );
            assert_eq!(
                normalize_bounded(
                    &case(
                        constant("d"),
                        vec![branch("c", &[], x()), branch("d", &[], var("y"))]
                    ),
                    &[],
                    8
                ),
                Ok(var("y"))
            );
            let stuck = case(constant("e"), vec![branch("c", &[], x())]);
            assert_eq!(normalize_bounded(&stuck, &[], 8), Ok(stuck.clone()));
            assert_eq!(
                normalize_bounded(&local("n", constant("a"), app("f", vec![var("n")])), &[], 8),
                Ok(app("f", vec![constant("a")]))
            );
        }

        /// Sort `A`, `f : A -> A`, and the given equations and directed
        /// equations.
        fn unary_theory(
            name: &str,
            eqs: Vec<Equation>,
            directed_eqs: Vec<DirectedEquation>,
        ) -> Theory {
            Theory {
                eqs,
                directed_eqs,
                ..theory(name, vec![simple("A")], vec![unary("f")])
            }
        }

        fn directed(name: &str, lhs: Term, rhs: Term) -> DirectedEquation {
            DirectedEquation {
                name: name.to_owned(),
                lhs,
                rhs,
                implementation: "impl".to_owned(),
                inverse: None,
                source_kind: None,
                target_kind: None,
                coercion_class: "Iso".to_owned(),
            }
        }

        #[test]
        fn bounded_equation_preservation_accepts_stated_images_and_refuses_missing_ones() {
            let fx = || app("f", vec![var("x")]);
            let e = || Equation {
                name: "e".to_owned(),
                lhs: fx(),
                rhs: var("x"),
            };
            let source = unary_theory("S", vec![e()], Vec::new());
            let stated = unary_theory("T", vec![e()], Vec::new());
            let bare = unary_theory("T", Vec::new(), Vec::new());
            assert_eq!(
                equations_preserved_within(&source, &stated, &identity_on(&source, &stated), 64),
                Ok(true)
            );
            assert_eq!(
                equations_preserved_within(&source, &bare, &identity_on(&source, &bare), 64),
                Ok(false)
            );
            let d = || directed("d", fx(), var("x"));
            let directed_source = unary_theory("S", Vec::new(), vec![d()]);
            let directed_target = unary_theory("T", Vec::new(), vec![d()]);
            assert_eq!(
                equations_preserved_within(
                    &directed_source,
                    &directed_target,
                    &identity_on(&directed_source, &directed_target),
                    64
                ),
                Ok(true)
            );
            assert_eq!(
                equations_preserved_within(
                    &directed_source,
                    &bare,
                    &identity_on(&directed_source, &bare),
                    64
                ),
                Ok(false)
            );
        }

        #[test]
        fn class_naming_reports_conflicts_and_freshens_clashes() {
            let classes = vec![vec![sym("l", "a")], vec![sym("r", "a")]];
            let mut freshened = Vec::new();
            assert_eq!(
                name_classes(
                    SymKind::Op,
                    &classes,
                    &BTreeMap::new(),
                    CompositionMode::Amalgamate,
                    ClashPolicy::Freshen,
                    &mut freshened
                ),
                Err(ComposeError::NameConflict(
                    SymKind::Op,
                    "a".to_owned(),
                    sym("l", "a"),
                    sym("r", "a")
                ))
            );
            assert_eq!(
                name_classes(
                    SymKind::Op,
                    &classes,
                    &BTreeMap::new(),
                    CompositionMode::ExplicitIdentity,
                    ClashPolicy::Reject,
                    &mut freshened
                ),
                Err(ComposeError::NameClash(
                    SymKind::Op,
                    "a".to_owned(),
                    Box::new(classes.clone())
                ))
            );
            assert!(freshened.is_empty());
            let majority = vec![
                vec![sym("l", "a")],
                vec![sym("l", "b"), sym("m", "a"), sym("n", "a")],
            ];
            assert_eq!(
                name_classes(
                    SymKind::Op,
                    &majority,
                    &BTreeMap::new(),
                    CompositionMode::ExplicitIdentity,
                    ClashPolicy::Freshen,
                    &mut freshened
                ),
                Ok(vec!["l.a#1".to_owned(), "l.a#2".to_owned()])
            );
            assert_eq!(
                freshened,
                vec![
                    Freshening {
                        kind: SymKind::Op,
                        members: majority[0].clone(),
                        candidate: "a".to_owned(),
                        result: "l.a#1".to_owned(),
                    },
                    Freshening {
                        kind: SymKind::Op,
                        members: majority[1].clone(),
                        candidate: "a".to_owned(),
                        result: "l.a#2".to_owned(),
                    },
                ]
            );
            let reserved = BTreeMap::from([
                ("x".to_owned(), sym("h", "x")),
                ("l.x".to_owned(), sym("h", "y")),
            ]);
            assert_eq!(
                name_classes(
                    SymKind::Eq,
                    &[vec![sym("l", "x")]],
                    &reserved,
                    CompositionMode::ExplicitIdentity,
                    ClashPolicy::Freshen,
                    &mut Vec::new()
                ),
                Ok(vec!["l.x#1".to_owned()])
            );
        }

        #[test]
        fn alpha_normal_forms_number_free_variables_by_first_occurrence() {
            let x = || var("x");
            assert_eq!(
                alpha(&app("f", vec![x(), x()]), &var("y")),
                (app("f", vec![var("#0"), var("#0")]), var("#1"))
            );
            assert_eq!(
                alpha(&local("y", x(), var("y")), &x()),
                (local("", var("#0"), var("#1")), var("#0"))
            );
        }

        #[test]
        fn each_edge_closes_its_square_by_its_least_generated_equation() {
            let generated = |name: &str, edge_index: usize| {
                (
                    name.to_owned(),
                    Generated {
                        lhs: constant("c"),
                        rhs: constant("c"),
                        source: AddedEquation {
                            edge: edge_index,
                            op: sym("S", "c"),
                        },
                        edge_index,
                    },
                )
            };
            assert_eq!(
                least_closing(&[
                    generated("S.f~0", 0),
                    generated("S.f2~0", 0),
                    generated("T.g~1", 1),
                ]),
                BTreeMap::from([(0, "S.f2~0".to_owned()), (1, "T.g~1".to_owned())])
            );
            assert_eq!(least_closing(&[]), BTreeMap::new());
        }

        #[test]
        fn equation_like_items_with_one_name_and_two_contents_conflict_under_amalgamation() {
            let items = || vec![(sym("R", "e"), 2, "right"), (sym("L", "e"), 1, "left")];
            let Err(conflict) = classify_items(SymKind::Eq, items(), CompositionMode::Amalgamate)
            else {
                panic!("one name carried by two contents conflicts");
            };
            assert_eq!(
                conflict,
                ComposeError::NameConflict(
                    SymKind::Eq,
                    "e".to_owned(),
                    sym("L", "e"),
                    sym("R", "e")
                )
            );
            let classified =
                classify_items(SymKind::Eq, items(), CompositionMode::ExplicitIdentity)
                    .unwrap_or_else(|error| panic!("explicit identity keeps both: {error:?}"));
            assert_eq!(
                classified.classes,
                vec![
                    (vec![sym("L", "e")], "left"),
                    (vec![sym("R", "e")], "right")
                ]
            );
            let same = vec![(sym("R", "e"), 1, "right"), (sym("L", "e"), 1, "left")];
            let joined = classify_items(SymKind::Eq, same, CompositionMode::Amalgamate)
                .unwrap_or_else(|error| panic!("one content is one class: {error:?}"));
            assert_eq!(
                joined.classes,
                vec![(vec![sym("L", "e"), sym("R", "e")], "left")]
            );
        }

        #[test]
        fn canonical_bytes_read_an_empty_application_as_the_bare_sort() {
            let with_output = |output: SortExpr| {
                canonical_theory_bytes(&theory(
                    "T",
                    vec![simple("A"), dependent("B", &[("p", name("A"))])],
                    vec![operation("k", &[explicit("x", name("A"))], output)],
                ))
            };
            assert_eq!(
                with_output(sort_app("A", Vec::new())),
                with_output(name("A"))
            );
            assert_ne!(
                with_output(sort_app("B", vec![var("x")])),
                with_output(name("B"))
            );
        }

        fn constant_op(name: &str) -> Op {
            operation(name, &[], name_a())
        }

        fn name_a() -> SortExpr {
            name("A")
        }

        fn leg(domain: &str, codomain: &str, ops: &[(&str, OpImage)]) -> Morphism {
            Morphism {
                name: format!("{domain}_to_{codomain}"),
                domain: domain.to_owned(),
                codomain: codomain.to_owned(),
                sorts: sort_map(),
                ops: ops
                    .iter()
                    .map(|(op, image)| ((*op).to_owned(), image.clone()))
                    .collect(),
            }
        }

        fn rename(op: &str) -> OpImage {
            OpImage::Op(op.to_owned())
        }

        /// The span `L <- S -> R` whose left leg sends `S.c` to the derived
        /// term `d()`, its right leg renaming `c` to itself.
        fn derived_span() -> TheoryDiagram {
            TheoryDiagram::span(
                (
                    "S".to_owned(),
                    theory("TS", vec![simple("A")], vec![constant_op("c")]),
                ),
                (
                    "L".to_owned(),
                    theory("TL", vec![simple("A")], vec![constant_op("d")]),
                ),
                (
                    "R".to_owned(),
                    theory("TR", vec![simple("A")], vec![constant_op("c")]),
                ),
                leg("TS", "TL", &[("c", OpImage::Term(constant("d")))]),
                leg("TS", "TR", &[("c", rename("c"))]),
            )
        }

        /// A diagram's labelled nodes and its edges by endpoint labels.
        type Parts = (Vec<(Name, Theory)>, Vec<(Name, Name, Morphism)>);

        fn parts(diagram: &TheoryDiagram) -> Parts {
            (
                diagram
                    .nodes
                    .iter()
                    .map(|node| (node.label.clone(), node.theory.clone()))
                    .collect(),
                diagram
                    .edges
                    .iter()
                    .map(|edge| (edge.src.clone(), edge.tgt.clone(), edge.morphism.clone()))
                    .collect(),
            )
        }

        #[test]
        fn universal_verification_checks_legs_squares_and_the_mediator_separately() {
            let diagram = derived_span();
            let composition = compose(&diagram, &options(CompositionMode::ExplicitIdentity))
                .unwrap_or_else(|error| panic!("the derived span composes: {error:?}"));
            assert_eq!(
                composition.theory.eqs,
                vec![Equation {
                    name: "S.c~0".to_owned(),
                    lhs: constant("c"),
                    rhs: constant("d"),
                }]
            );
            let (nodes, edges) = parts(&diagram);
            let target = theory(
                "Q",
                vec![simple("A")],
                vec![constant_op("p"), constant_op("q")],
            );
            let legs = vec![
                leg("TS", "Q", &[("c", rename("p"))]),
                leg("TL", "Q", &[("d", rename("q"))]),
                leg("TR", "Q", &[("c", rename("p"))]),
            ];
            let input = |legs| UniversalInput {
                nodes: &nodes,
                target: &target,
                legs,
                positions: nodes
                    .iter()
                    .enumerate()
                    .map(|(index, (label, _))| (label.as_str(), index))
                    .collect(),
                rewrite_budget: 64,
            };
            let class = vec![sym("R", "c"), sym("S", "c")];
            assert_eq!(cocone_class(&composition, SymKind::Op, "S", "c"), class);
            assert_eq!(
                cocone_class(&composition, SymKind::Op, "S", "missing"),
                Vec::<SymRef>::new()
            );
            assert_eq!(verify_cocone_legs(&input(&legs)), Ok(None));
            assert_eq!(
                verify_cocone_legs(&input(&legs[..2])),
                Err(ComposeError::NotACocone(Vec::new()))
            );
            let unknown = [
                legs[0].clone(),
                leg("TL", "Q", &[("d", rename("zz"))]),
                legs[2].clone(),
            ];
            assert_eq!(
                verify_cocone_legs(&input(&unknown)),
                Err(ComposeError::NotACocone(Vec::new()))
            );
            let mut exhausted = None;
            assert_eq!(
                verify_cocone_edges(&composition, &input(&legs), &edges, &mut exhausted),
                Err(ComposeError::NotACocone(class.clone()))
            );
            let agreeing = [
                legs[0].clone(),
                leg("TL", "Q", &[("d", rename("p"))]),
                legs[2].clone(),
            ];
            assert_eq!(
                verify_cocone_edges(&composition, &input(&agreeing), &edges, &mut exhausted),
                Ok(())
            );
            let mediator = |d: &str| Morphism {
                sorts: sort_map(),
                ops: BTreeMap::from([("c".to_owned(), rename("p")), ("d".to_owned(), rename(d))]),
                ..Morphism::default()
            };
            assert_eq!(
                verify_mediator(&composition, &input(&legs), &mediator("q"), &mut exhausted),
                Err(ComposeError::NotACocone(class))
            );
            assert_eq!(
                verify_mediator(
                    &composition,
                    &input(&agreeing),
                    &mediator("p"),
                    &mut exhausted
                ),
                Ok(())
            );
            assert_eq!(exhausted, None);
            let (found, report) = composition
                .verify_universal(&diagram, &target, &agreeing)
                .unwrap_or_else(|error| panic!("the agreeing legs form a cocone: {error:?}"));
            assert_eq!(found, mediator("p"));
            assert_eq!(report.status, CheckStatus::Incomplete);
            assert!(report.checks.contains(&CheckRecord {
                check: CompositionCheckKind::EquationEntailed,
                subject: CheckSubject::Diagram,
                outcome: CheckOutcome::Inapplicable {
                    reason: InapplicableReason::NoEquations,
                },
            }));
        }

        #[test]
        fn a_cocone_leg_whose_equation_check_runs_out_of_budget_is_recorded() {
            let unary_on = |name: &str| unary(name);
            let equation = |name: &str, lhs: &str, rhs: &str| Equation {
                name: name.to_owned(),
                lhs: app(lhs, vec![var("x")]),
                rhs: app(rhs, vec![var("x")]),
            };
            let source = Theory {
                eqs: vec![equation("e", "f", "g")],
                ..theory(
                    "S",
                    vec![simple("A")],
                    vec![unary_on("f"), unary_on("g"), unary_on("h")],
                )
            };
            let target = Theory {
                eqs: vec![equation("e1", "f", "h"), equation("e2", "h", "g")],
                ..theory(
                    "Q",
                    vec![simple("A")],
                    vec![unary_on("f"), unary_on("g"), unary_on("h")],
                )
            };
            let nodes = vec![("S".to_owned(), source.clone())];
            let legs = vec![identity_on(&source, &target)];
            let input = UniversalInput {
                nodes: &nodes,
                target: &target,
                legs: &legs,
                positions: BTreeMap::from([("S", 0)]),
                rewrite_budget: 8,
            };
            assert_eq!(verify_cocone_legs(&input), Ok(Some(8)));
        }

        #[test]
        fn the_universal_report_marks_targets_without_any_equation() {
            let base = CompositionReport {
                mode: CompositionMode::Amalgamate,
                status: CheckStatus::Passed,
                name_clash_free: true,
                checks: Vec::new(),
                freshened: Vec::new(),
            };
            let entailed = |target: &Theory, exhausted| {
                universal_report(&base, target, exhausted)
                    .checks
                    .into_iter()
                    .map(|record| record.outcome)
                    .collect::<Vec<_>>()
            };
            let fx = || app("f", vec![var("x")]);
            let directed_only = unary_theory("Q", Vec::new(), vec![directed("d", fx(), var("x"))]);
            assert_eq!(
                entailed(&directed_only, None),
                [CheckOutcome::Passed, CheckOutcome::Passed]
            );
            assert_eq!(
                entailed(&unary_theory("Q", Vec::new(), Vec::new()), None),
                [
                    CheckOutcome::Inapplicable {
                        reason: InapplicableReason::NoEquations,
                    },
                    CheckOutcome::Passed,
                ]
            );
            let exhausted = |equation: &str| CheckOutcome::Incomplete {
                reason: CompositionIncomplete::RewriteBudgetExhausted {
                    equation: equation.to_owned(),
                    steps: 3,
                },
            };
            assert_eq!(
                entailed(&directed_only, Some(3)),
                [exhausted("mediator"), exhausted("cocone")]
            );
        }

        fn discrete(nodes: Vec<(&str, Theory)>) -> TheoryDiagram {
            TheoryDiagram::discrete(
                nodes
                    .into_iter()
                    .map(|(label, theory)| (label.to_owned(), theory))
                    .collect(),
            )
        }

        #[test]
        fn node_labels_must_be_nonempty_and_free_of_separators() {
            for label in ["", "a.b", "a#b", "#", "."] {
                let diagram = discrete(vec![(label, theory("T", vec![simple("A")], Vec::new()))]);
                assert_eq!(
                    compose(&diagram, &options(CompositionMode::Amalgamate)).err(),
                    Some(ComposeError::InvalidNodeLabel(label.to_owned())),
                    "label {label:?}"
                );
            }
        }

        #[test]
        fn an_undeclared_sort_may_map_to_a_declared_target_sort() {
            let k = |output: &str| operation("k", &[], name(output));
            let mut diagram = discrete(vec![
                ("a", theory("TA", Vec::new(), vec![k("S")])),
                ("b", theory("TB", vec![simple("T")], vec![k("T")])),
            ]);
            diagram.edges.push(DiagramEdge {
                src: "a".to_owned(),
                tgt: "b".to_owned(),
                morphism: Morphism {
                    name: "a_to_b".to_owned(),
                    domain: "TA".to_owned(),
                    codomain: "TB".to_owned(),
                    sorts: BTreeMap::from([("S".to_owned(), "T".to_owned())]),
                    ops: BTreeMap::from([("k".to_owned(), rename("k"))]),
                },
            });
            let composition = compose(&diagram, &options(CompositionMode::Amalgamate))
                .unwrap_or_else(|error| panic!("`S` reaches the declared `T`: {error:?}"));
            assert_eq!(composition.theory.ops, vec![operation("k", &[], name("S"))]);
        }

        #[test]
        fn names_of_different_kinds_or_different_names_never_clash() {
            let report = |right: Theory| {
                let diagram = discrete(vec![
                    ("L", theory("TL", vec![simple("A")], Vec::new())),
                    ("R", right),
                ]);
                compose(&diagram, &options(CompositionMode::Amalgamate))
                    .unwrap_or_else(|error| panic!("the two nodes compose: {error:?}"))
                    .report
                    .name_clash_free
            };
            assert!(report(theory(
                "TR",
                vec![simple("X")],
                vec![operation("A", &[], name("X"))]
            )));
            assert!(report(theory("TR", vec![simple("B")], Vec::new())));
            assert!(!report(theory("TR", vec![simple("A")], Vec::new())));
        }

        #[test]
        fn a_derived_image_generates_its_equation_over_the_operation_inputs() {
            let node =
                |name: &str| theory(name, vec![simple("A")], vec![constant_op("c"), unary("r")]);
            let diagram = TheoryDiagram::span(
                ("S".to_owned(), node("TS")),
                ("L".to_owned(), node("TL")),
                ("R".to_owned(), node("TR")),
                leg(
                    "TS",
                    "TL",
                    &[
                        ("c", rename("c")),
                        ("r", OpImage::Term(app("r", vec![var("x")]))),
                    ],
                ),
                leg("TS", "TR", &[("c", rename("c")), ("r", rename("r"))]),
            );
            let composition = compose(&diagram, &options(CompositionMode::Amalgamate))
                .unwrap_or_else(|error| panic!("the derived image is well typed: {error:?}"));
            assert_eq!(
                composition.theory.eqs,
                vec![Equation {
                    name: "S.r~0".to_owned(),
                    lhs: app("r", vec![var("x")]),
                    rhs: app("r", vec![var("x")]),
                }]
            );
        }

        #[test]
        fn freshened_directed_equations_are_stated_in_result_names() {
            let fc = || constant("f");
            let left = Theory {
                directed_eqs: vec![directed("d", fc(), fc())],
                ..theory("TL", vec![simple("A")], vec![constant_op("f")])
            };
            let right = theory("TR", vec![simple("A")], vec![constant_op("f")]);
            let diagram = discrete(vec![("L", left), ("R", right)]);
            let composition = compose(
                &diagram,
                &ComposeOptions {
                    clash: ClashPolicy::Freshen,
                    ..options(CompositionMode::ExplicitIdentity)
                },
            )
            .unwrap_or_else(|error| panic!("freshening separates the clash: {error:?}"));
            assert_eq!(
                composition.theory.directed_eqs,
                vec![directed("d", constant("L.f"), constant("L.f"))]
            );
        }

        use crate::enumerate::theories as scope;

        /// The bare signatures with at most one operation.
        fn small_signatures() -> Vec<Theory> {
            scope::bare_signatures()
                .into_iter()
                .filter(|theory| theory.ops.len() <= 1)
                .collect()
        }

        #[test]
        fn signature_checking_is_pinned_on_the_whole_signature_scopes() {
            let signatures = scope::signatures();
            assert_digest(
                "signature verdicts",
                signatures.iter().map(typecheck_signature),
                "64363fdc6bd1bd3f7c961741476f30e5ca9f1f76fb61818ac465145bbef65ce2",
            );
            assert_digest(
                "theory body verdicts",
                signatures.iter().map(typecheck_theory_body),
                "a1d71560d2b7f5559d7a3c28272ac2af96cc0fedfe3c9f1a06833b4a1a1b6c89",
            );
            assert_digest(
                "raw signature verdicts",
                scope::raw_signatures().iter().map(typecheck_signature),
                "0e3a52ee52f8080164f0575ab9367620a989483a742dd39f72431fd90eb702d8",
            );
        }

        #[test]
        fn morphism_checks_are_pinned_on_every_small_signature_pair() {
            let small = small_signatures();
            let mut verdicts = Vec::new();
            for domain in &small {
                for codomain in &small {
                    for morphism in scope::morphisms(domain, codomain)
                        .iter()
                        .chain(&scope::partial_morphisms(domain, codomain))
                    {
                        verdicts.push(check_morphism(domain, codomain, morphism));
                    }
                }
            }
            assert_digest(
                "small morphism verdicts",
                &verdicts,
                "feb03b75da2e1b626e5c73e9d0eef363403d5088bf1a559a9afc30f1adfd91f0",
            );
        }

        #[test]
        fn morphism_checks_are_pinned_on_a_sample_of_signatures_with_equations() {
            let signatures = scope::signatures();
            let mut verdicts = Vec::new();
            for domain in signatures.iter().filter(|t| !t.eqs.is_empty()).step_by(53) {
                for codomain in signatures.iter().step_by(59) {
                    for morphism in scope::morphisms(domain, codomain) {
                        verdicts.push(check_morphism(domain, codomain, &morphism));
                    }
                }
            }
            assert_digest(
                "verdicts on every 53rd signature with an equation against every 59th signature",
                &verdicts,
                "42ee418e2d9f6e4260eccddad3bd6440ac9ad70dd6e14790ce4b1f2b9e2981c1",
            );
        }

        #[test]
        fn the_whole_action_and_constructor_families_are_pinned() {
            let whole = scope::whole_action_scope();
            let mut translations = Vec::new();
            for domain in &whole.domains {
                for morphism in whole.legs.iter().step_by(11) {
                    for probe in &whole.probes {
                        translations.push(apply_morphism(domain, morphism, probe));
                    }
                }
            }
            assert_digest(
                "probe translations along every 11th whole-action leg",
                &translations,
                "86bbf2beb3b9807b37f315b5f303a7ff10b797053f2b30b29c56a65869616f94",
            );
            let mut verdicts = Vec::new();
            for domain in &whole.domains {
                for codomain in &whole.codomains {
                    for morphism in whole.legs.iter().step_by(97) {
                        verdicts.push(check_morphism(domain, codomain, morphism));
                    }
                }
            }
            assert_digest(
                "every 97th whole-action leg verdict",
                &verdicts,
                "613b2a8e02fad201696840cf3f83998b8c789932711e7b0146d513a9a25752e7",
            );
            let constructors = scope::constructor_scope();
            let mut outcomes = Vec::new();
            for domain in &constructors.domains {
                for codomain in &constructors.codomains {
                    for morphism in &constructors.legs {
                        outcomes.push(format!("{:?}", check_morphism(domain, codomain, morphism)));
                    }
                }
                for morphism in &constructors.legs {
                    for probe in &constructors.probes {
                        outcomes.push(apply_morphism(domain, morphism, probe).to_string());
                    }
                }
            }
            assert_digest(
                "constructor family outcomes",
                &outcomes,
                "308b9833234a9aecbe152196e5cf0e7535ae66f8cd8e7ddd48b2f881d360b382",
            );
        }

        /// A composition's outcome with its canonical bytes and its universal
        /// verification against its own inclusions.
        fn composition_outcome(diagram: &TheoryDiagram, options: &ComposeOptions) -> String {
            match compose(diagram, options) {
                Ok(composition) => {
                    let universal = composition.verify_universal(
                        diagram,
                        &composition.theory,
                        &composition.inclusions,
                    );
                    format!(
                        "{composition:?}\n{:?}\n{universal:?}",
                        canonical_composition_bytes(&composition)
                    )
                }
                Err(error) => format!("{error:?}"),
            }
        }

        fn outcomes_in_every_option_set(diagrams: &[TheoryDiagram]) -> Vec<String> {
            let mut out = Vec::new();
            for diagram in diagrams {
                for options in scope::option_sets() {
                    out.push(composition_outcome(diagram, &options));
                }
            }
            out
        }

        #[test]
        fn compositions_of_small_signature_pairs_are_pinned() {
            let small = small_signatures();
            let mut discrete = Vec::new();
            let mut spans = Vec::new();
            let interface = theory("I", vec![simple("A")], Vec::new());
            for left in small.iter().step_by(3) {
                for right in small.iter().step_by(4) {
                    discrete.push(TheoryDiagram::discrete(vec![
                        ("L".to_owned(), left.clone()),
                        ("R".to_owned(), right.clone()),
                    ]));
                    spans.push(TheoryDiagram::by_name(
                        ("S".to_owned(), interface.clone()),
                        ("L".to_owned(), left.clone()),
                        ("R".to_owned(), right.clone()),
                    ));
                }
            }
            assert_eq!(discrete.len(), 21 * 16, "every 3rd against every 4th of 61");
            assert_digest(
                "discrete pair compositions",
                outcomes_in_every_option_set(&discrete),
                "1b7c187c94bba7ae304b244f575b7f97676cd73ae4028fe5e6108d87c5244c79",
            );
            let built: Vec<TheoryDiagram> =
                spans.iter().filter_map(|span| span.clone().ok()).collect();
            assert_digest(
                "by-name span compositions",
                spans
                    .iter()
                    .map(|span| span.as_ref().err().map(|error| format!("{error:?}")))
                    .chain(outcomes_in_every_option_set(&built).into_iter().map(Some)),
                "b1d480b1f0bd9aa64902f9e773203275f07b727d96d57c0fd4fc2594d7a2c2c2",
            );
        }

        #[test]
        fn compositions_of_connector_and_fibre_diagrams_are_pinned() {
            let operations = scope::class_operation_scope();
            let operation_pairs: Vec<String> = operations
                .iter()
                .step_by(101)
                .flat_map(|left| {
                    operations.iter().step_by(103).map(move |right| {
                        composition_outcome(
                            &scope::operation_class_pair(left, right),
                            &options(CompositionMode::Amalgamate),
                        )
                    })
                })
                .collect();
            assert_eq!(
                operation_pairs.len(),
                28 * 28,
                "every 101st against every 103rd"
            );
            assert_digest(
                "operation class pair compositions",
                &operation_pairs,
                "36b285147881bc7fc2524170d915912f06e880b6eb9623bf65c9857aa604e59e",
            );
            let sorts = scope::class_sort_scope();
            let sort_pairs: Vec<String> = sorts
                .iter()
                .step_by(9)
                .flat_map(|left| {
                    sorts.iter().step_by(13).map(move |right| {
                        composition_outcome(
                            &scope::sort_class_pair(left, right),
                            &options(CompositionMode::Amalgamate),
                        )
                    })
                })
                .collect();
            assert_eq!(sort_pairs.len(), 29 * 20, "every 9th against every 13th");
            assert_digest(
                "sort class pair compositions",
                &sort_pairs,
                "85f9eaff858d5526927d88727d042629bda932e719fde4ea652d8dcd69614b9c",
            );
            assert_digest(
                "sort mismatch span compositions",
                outcomes_in_every_option_set(&scope::sort_mismatch_spans()),
                "7ef5371f9dcfe302d05ffcafdf293007aadfe35a8501ad2bd3f38b82cbb0af43",
            );
            assert_digest(
                "fibre span compositions",
                outcomes_in_every_option_set(&scope::fibre_spans()),
                "11dd3a4d08dfd71ca3ac4689a4629e912f8e171aed78e050a47a094f7602942b",
            );
            assert_digest(
                "derived span compositions",
                outcomes_in_every_option_set(&[derived_span()]),
                "bf2d22f6bd19b2b538b397ce9cf7847f746818b962ffa05299454937774a2ee5",
            );
        }

        #[test]
        fn sort_classes_report_their_first_difference() {
            let details: Vec<Option<ComposeError>> = scope::sort_mismatch_spans()
                .iter()
                .map(|span| compose(span, &options(CompositionMode::Amalgamate)).err())
                .collect();
            let mismatch = |detail: SortMismatch| {
                Some(ComposeError::SortSignatureMismatch(
                    sym("L", "W"),
                    sym("R", "W"),
                    Box::new(detail),
                ))
            };
            assert_eq!(
                details,
                vec![
                    mismatch(SortMismatch::Arity {
                        first: 0,
                        second: 1
                    }),
                    mismatch(SortMismatch::ParamSort {
                        index: 0,
                        first: "X".to_owned(),
                        second: "Y(a())".to_owned(),
                    }),
                    mismatch(SortMismatch::Kind),
                    mismatch(SortMismatch::Closure),
                ]
            );
        }

        #[test]
        fn an_empty_parameter_application_is_reported_as_the_bare_sort() {
            let declaration = |param: SortExpr| Sort {
                name: "W".to_owned(),
                params: vec![("x".to_owned(), param)],
                kind: "structural".to_owned(),
                constructors: None,
            };
            let diagram = scope::sort_class_pair(
                &declaration(sort_app("X", Vec::new())),
                &declaration(name("Y")),
            );
            assert_eq!(
                compose(&diagram, &options(CompositionMode::Amalgamate)).err(),
                Some(ComposeError::SortSignatureMismatch(
                    sym("l", "lead"),
                    sym("r", "W"),
                    Box::new(SortMismatch::ParamSort {
                        index: 0,
                        first: "X".to_owned(),
                        second: "Y".to_owned(),
                    })
                ))
            );
        }

        #[test]
        fn same_named_operations_of_two_signatures_conflict_under_amalgamation() {
            let node = |name: &str, op: Op| theory(name, vec![simple("A")], vec![op]);
            let diagram = discrete(vec![
                ("L", node("TL", constant_op("f"))),
                ("R", node("TR", unary("f"))),
            ]);
            assert_eq!(
                compose(&diagram, &options(CompositionMode::Amalgamate)).err(),
                Some(ComposeError::NameConflict(
                    SymKind::Op,
                    "f".to_owned(),
                    sym("L", "f"),
                    sym("R", "f")
                ))
            );
        }

        #[test]
        fn quotients_are_pinned_on_every_short_identification_list() {
            assert_eq!(quotient(3, &[(2, 1), (1, 0)]), Some(vec![0, 0, 0]));
            assert_eq!(quotient(4, &[(3, 1)]), Some(vec![0, 1, 2, 1]));
            assert_eq!(quotient(2, &[(0, 2)]), None);
            let mut out = Vec::new();
            for size in 0..=3 {
                for identifications in scope::identifications(size, 2) {
                    out.push(quotient(size, &identifications));
                    out.push(quotient(size + 1, &identifications));
                }
            }
            assert_digest(
                "quotients",
                &out,
                "635f67de025794ec2ad64926d6729c5b7699fd790381cbdd17fd3a9a6aaa3491",
            );
        }

        #[test]
        fn recipe_replay_is_pinned_on_small_signature_pairs() {
            let small = small_signatures();
            let mut out = Vec::new();
            for left in small.iter().step_by(4) {
                for right in small.iter().step_by(5) {
                    let registry = BTreeMap::from([
                        (
                            "L".to_owned(),
                            Theory {
                                name: "L".to_owned(),
                                ..left.clone()
                            },
                        ),
                        (
                            "R".to_owned(),
                            Theory {
                                name: "R".to_owned(),
                                ..right.clone()
                            },
                        ),
                    ]);
                    for spec in scope::recipes(left, right) {
                        out.push(format!(
                            "{:?}\n{:?}",
                            recompose(&spec, &registry),
                            canonical_composition_spec_bytes(&spec)
                        ));
                    }
                }
            }
            assert_digest(
                "replays of every 4th against every 5th pair",
                &out,
                "f7865f7b431183d4130f9654a234bca1e13f1e23265c45103f9716f1f59fdda1",
            );
        }
    }
}

pub use self::theory::{
    apply_morphism, check_morphism, compose, quotient, recompose, typecheck_signature,
};
