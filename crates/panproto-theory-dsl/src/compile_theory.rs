//! Compile a [`TheorySpec`] into a [`Theory`].
//!
//! Maps DSL spec types to GAT engine types: sorts, operations,
//! equations, directed equations, and conflict policies. Runs
//! typechecking on the result.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{
    CoercionClass, ConflictPolicy, ConflictStrategy, DirectedEquation, Equation, GatError,
    Operation, SignatureDefect, SignatureSite, Sort, SortExpr, SortKind, SortParam, Theory,
    ValueKind,
};

use crate::document::{
    DirectedEqSpec, EquationSpec, OpSpec, PolicySpec, SortKindSpec, SortSpec, StrategySpec,
    TheorySpec,
};
use crate::error::TheoryDslError;
use crate::path::DocumentPath;

/// Compile a [`TheorySpec`] into a [`Theory`].
///
/// Parses all sorts, operations, equations, directed equations, and
/// policies from spec types into GAT engine types, constructs the
/// theory via [`Theory::full`], runs typechecking, and gates the
/// theory's directed rewrite system on local confluence and LPO
/// termination. When the spec declares imports, the importing crate
/// must call [`compile_theory_with_resolver`] so the imports can be
/// resolved.
///
/// # Errors
///
/// Returns errors for parse failures, unknown value kinds, typechecking
/// violations, and a directed rewrite system that is not provably sound
/// ([`TheoryDslError::UnsoundRewriteSystem`]) or whose soundness could
/// not be decided ([`TheoryDslError::RewriteSystemCheck`]).
pub fn compile_theory(spec: &TheorySpec) -> Result<Theory, TheoryDslError> {
    compile_theory_with_resolver(spec, &|_name| None)
}

/// Compile a [`TheorySpec`] and then run sample-based coercion law
/// checks on every directed equation using `registry`.
///
/// Binds each sample under the default variable name `"x"`. Theories
/// whose directed equations use a different free variable name
/// should call [`compile_theory_with_law_check_and_var`] directly.
///
/// Produces the same [`Theory`] as [`compile_theory`] when every
/// declared coercion class is consistent with the sample evidence.
/// Otherwise returns
/// [`TheoryDslError::CoercionLawViolation`] with one entry per
/// sample-level violation. The plain [`compile_theory`] path does
/// not run this check, preserving the pre-0.38 behavior.
///
/// # Errors
///
/// Same as [`compile_theory`], plus
/// [`TheoryDslError::CoercionLawViolation`] when a declared coercion
/// class is falsified on `registry`'s samples.
pub fn compile_theory_with_law_check(
    spec: &TheorySpec,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
) -> Result<Theory, TheoryDslError> {
    compile_theory_with_law_check_and_var(spec, registry, "x")
}

/// Variable-name-parameterized [`compile_theory_with_law_check`].
///
/// Binds each sample under `var_name` instead of the default `"x"`.
/// Use this when the theory's directed equations share a different
/// free variable name (for example, `"v"` or a field key).
///
/// # Errors
///
/// Same as [`compile_theory_with_law_check`].
pub fn compile_theory_with_law_check_and_var(
    spec: &TheorySpec,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
    var_name: &str,
) -> Result<Theory, TheoryDslError> {
    let theory = compile_theory(spec)?;
    enforce_coercion_laws(&theory, registry, var_name)?;
    Ok(theory)
}

/// Run the sample-based coercion-law check on `theory`, converting any
/// violations into a [`TheoryDslError::CoercionLawViolation`].
///
/// Each directed equation is checked against samples drawn from
/// `registry` and bound under `var_name`. Returns `Ok(())` when every
/// declared coercion class holds on those samples, and vacuously when
/// the theory declares no directed equations.
///
/// # Errors
///
/// Returns [`TheoryDslError::CoercionLawViolation`] carrying one entry
/// per sample-level violation when a declared class is falsified.
pub(crate) fn enforce_coercion_laws(
    theory: &Theory,
    registry: &panproto_lens::coercion_laws::CoercionSampleRegistry,
    var_name: &str,
) -> Result<(), TheoryDslError> {
    let report = panproto_lens::coercion_laws::check_theory_with_var(theory, registry, var_name);
    if report.is_clean() {
        return Ok(());
    }
    let mut violations: Vec<crate::error::CoercionLawViolationDetail> = Vec::new();
    let mut distinct_equations: usize = 0;
    for (name, vs) in report.per_equation {
        if vs.is_empty() {
            continue;
        }
        distinct_equations += 1;
        for v in vs {
            violations.push(crate::error::CoercionLawViolationDetail {
                equation: name.as_ref().to_owned(),
                violation: v,
            });
        }
    }
    Err(TheoryDslError::CoercionLawViolation {
        theory: theory.name.as_ref().to_owned(),
        violations,
        distinct_equations,
    })
}

/// Compile a [`TheorySpec`], resolving its parents and imports.
///
/// The compiled theory's declarations are, in this order: those of each
/// parent named in `extends` (as the resolver returns it) and of each
/// import renamed, a declaration equal to one already included being
/// included once, and then the spec's own declarations. Two declarations
/// of one name that remain are refused by the signature check as a
/// duplicate.
///
/// An import of theory `F` with alias `A` and exposed names `X` renames
/// every sort, operation, equation, directed equation and policy name `n`
/// of `F` to `n` when `n` is in `X`, to `A_n` when there is an alias, and
/// to `F_n` otherwise, applying the renaming to every sort and operation
/// named inside a declaration. In the spec's own declarations a sort head
/// or operation head `t` stands for: `t` when the spec declares `t`;
/// otherwise `ρ(n)` when `t` is `F.n` or `A.n` for an import that declares
/// `n`; otherwise the renamed `n` of the first import without an alias
/// that declares `n` as a symbol of that kind (an exposed `n` is renamed
/// to itself); otherwise `t`.
///
/// Every refusal names the document path of the element refused, relative
/// to the spec: a local declaration by its own path, and a declaration a
/// parent or import contributes by the path of that `extends` or `imports`
/// entry.
///
/// # Errors
///
/// Same as [`compile_theory`], plus [`TheoryDslError::TheoryNotFound`]
/// when a parent or import names a theory the resolver cannot find, and
/// [`TheoryDslError::UnknownExposed`] when an import exposes a name its
/// theory does not declare.
pub fn compile_theory_with_resolver(
    spec: &TheorySpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<Theory, TheoryDslError> {
    let mut union = Presentation::default();
    for (index, parent) in spec.extends.iter().enumerate() {
        let at = DocumentPath::root().key("extends").index(index);
        let theory = resolver(parent).ok_or_else(|| {
            TheoryDslError::TheoryNotFound {
                name: parent.clone(),
                context: format!("parent of theory '{}'", spec.theory),
            }
            .within(&at)
        })?;
        union.include(&theory, &Origin::Included(at));
    }
    let mut scope = Scope::default();
    for (index, import) in spec.imports.iter().enumerate() {
        let at = DocumentPath::root().key("imports").index(index);
        let theory = resolver(&import.from).ok_or_else(|| {
            TheoryDslError::TheoryNotFound {
                name: import.from.clone(),
                context: format!("import in theory '{}'", spec.theory),
            }
            .within(&at.key("from"))
        })?;
        let renamed = scope.import(import, &theory, &at)?;
        union.include(&renamed, &Origin::Included(at));
    }
    let local = compile_local(spec, &scope.local_maps(spec))?;
    union.include_local(local);
    union.check(spec)
}

/// Where a declaration of the compiled theory comes from.
#[derive(Clone)]
enum Origin {
    /// The spec's own declaration at this path.
    Local(DocumentPath),
    /// A declaration of the parent or import at this path.
    Included(DocumentPath),
}

impl Origin {
    /// The path of the declaration, followed by `suffix` when the
    /// declaration is the spec's own.
    fn path(&self, suffix: &[&str]) -> DocumentPath {
        match self {
            Self::Local(path) => suffix.iter().fold(path.clone(), |path, step| {
                step.parse::<usize>()
                    .map_or_else(|_| path.key(step), |index| path.index(index))
            }),
            Self::Included(path) => path.clone(),
        }
    }
}

/// A union of declarations with the origin of each.
#[derive(Default)]
struct Presentation {
    /// The operations the spec declares with the unary `input` shorthand.
    unary_ops: Vec<Arc<str>>,
    sorts: Vec<(Sort, Origin)>,
    ops: Vec<(Operation, Origin)>,
    eqs: Vec<(Equation, Origin)>,
    directed_eqs: Vec<(DirectedEquation, Origin)>,
    policies: Vec<(ConflictPolicy, Origin)>,
}

fn add<T: PartialEq>(into: &mut Vec<(T, Origin)>, item: T, origin: Origin) {
    if !into.iter().any(|(present, _)| *present == item) {
        into.push((item, origin));
    }
}

impl Presentation {
    fn include(&mut self, theory: &Theory, origin: &Origin) {
        for sort in &theory.sorts {
            add(&mut self.sorts, sort.clone(), origin.clone());
        }
        for op in &theory.ops {
            add(&mut self.ops, op.clone(), origin.clone());
        }
        for eq in &theory.eqs {
            add(&mut self.eqs, eq.clone(), origin.clone());
        }
        for eq in &theory.directed_eqs {
            add(&mut self.directed_eqs, eq.clone(), origin.clone());
        }
        for policy in &theory.policies {
            add(&mut self.policies, policy.clone(), origin.clone());
        }
    }

    fn include_local(&mut self, local: Self) {
        self.unary_ops.extend(local.unary_ops);
        self.sorts.extend(local.sorts);
        self.ops.extend(local.ops);
        self.eqs.extend(local.eqs);
        self.directed_eqs.extend(local.directed_eqs);
        self.policies.extend(local.policies);
    }

    fn sort_origin(&self, name: &str) -> Option<&Origin> {
        self.sorts
            .iter()
            .find(|(sort, _)| &*sort.name == name)
            .map(|(_, origin)| origin)
    }

    fn op_origin(&self, name: &str) -> Option<&Origin> {
        self.ops
            .iter()
            .find(|(op, _)| &*op.name == name)
            .map(|(_, origin)| origin)
    }

    /// The path of the first signature defect.
    fn defect_path(&self, defect: &SignatureDefect) -> DocumentPath {
        let site_path = |site: &SignatureSite| match site {
            SignatureSite::SortParam { sort, index } => self
                .sort_origin(sort)
                .map(|origin| origin.path(&["params", &index.to_string(), "sort"])),
            SignatureSite::OpInput { op, index } => self
                .ops
                .iter()
                .find(|(o, _)| o.name == *op)
                .map(|(_, origin)| {
                    if matches!(origin, Origin::Local(_)) && self.unary_ops.contains(op) {
                        origin.path(&["input"])
                    } else {
                        origin.path(&["inputs", &index.to_string(), "sort"])
                    }
                }),
            SignatureSite::OpOutput { op } => {
                self.op_origin(op).map(|origin| origin.path(&["output"]))
            }
        };
        let path = match defect {
            SignatureDefect::UndeclaredSort { site, .. }
            | SignatureDefect::SortArity { site, .. }
            | SignatureDefect::ParamIllTyped { site, .. }
            | SignatureDefect::UnboundParamVar { site, .. }
            | SignatureDefect::DuplicateBinder { site, .. } => site_path(site),
            SignatureDefect::DuplicateSymbol { kind, name } => {
                let origins: Vec<&Origin> = match kind {
                    panproto_gat::SymKind::Sort => self
                        .sorts
                        .iter()
                        .filter(|(d, _)| d.name == *name)
                        .map(|(_, o)| o)
                        .collect(),
                    panproto_gat::SymKind::Op => self
                        .ops
                        .iter()
                        .filter(|(d, _)| d.name == *name)
                        .map(|(_, o)| o)
                        .collect(),
                    panproto_gat::SymKind::Eq => self
                        .eqs
                        .iter()
                        .filter(|(d, _)| d.name == *name)
                        .map(|(_, o)| o)
                        .collect(),
                    panproto_gat::SymKind::DirectedEq => self
                        .directed_eqs
                        .iter()
                        .filter(|(d, _)| d.name == *name)
                        .map(|(_, o)| o)
                        .collect(),
                    panproto_gat::SymKind::Policy => self
                        .policies
                        .iter()
                        .filter(|(d, _)| d.name == *name)
                        .map(|(_, o)| o)
                        .collect(),
                };
                origins.get(1).map(|origin| origin.path(&[]))
            }
        };
        path.unwrap_or_default()
    }

    /// Build the theory and check it: its signature, then the body of
    /// [`panproto_gat::typecheck_theory`] with each equation checked in
    /// order, then the rewrite system.
    fn check(self, spec: &TheorySpec) -> Result<Theory, TheoryDslError> {
        let theory = Theory::full(
            spec.theory.as_str(),
            spec.extends.iter().map(|s| Arc::from(s.as_str())).collect(),
            self.sorts.iter().map(|(d, _)| d.clone()).collect(),
            self.ops.iter().map(|(d, _)| d.clone()).collect(),
            self.eqs.iter().map(|(d, _)| d.clone()).collect(),
            self.directed_eqs.iter().map(|(d, _)| d.clone()).collect(),
            self.policies.iter().map(|(d, _)| d.clone()).collect(),
        );
        let type_error = |error| TheoryDslError::TypeCheck {
            theory: spec.theory.clone(),
            error,
        };
        if let Err(defects) = panproto_gat::typecheck_signature(&theory) {
            let path = defects
                .first()
                .map(|defect| self.defect_path(defect))
                .unwrap_or_default();
            return Err(type_error(GatError::IllTypedSignature { defects }).within(&path));
        }
        let mut without_equations = theory.clone();
        without_equations.eqs.clear();
        if let Err(error) = panproto_gat::typecheck_theory_body(&without_equations) {
            let path = match &error {
                GatError::NonInferrableImplicit { op, .. } => {
                    self.op_origin(op).map(|origin| origin.path(&[]))
                }
                GatError::InvalidClosedSortConstructor { sort, .. } => self
                    .sort_origin(sort)
                    .map(|origin| origin.path(&["closed"])),
                _ => None,
            };
            return Err(type_error(error).within(&path.unwrap_or_default()));
        }
        for (eq, origin) in &self.eqs {
            panproto_gat::typecheck_equation(eq, &theory)
                .map_err(|error| type_error(error).within(&origin.path(&[])))?;
        }

        // Gate the theory's directed rewrite system on local confluence
        // and LPO termination. Normalization decides this theory's
        // equality judgment by running that system, so a system that is
        // not provably sound makes the judgment unsound. Both the analysis
        // failing and the analysis reporting a violation are refusals.
        let report = panproto_gat::validate_rewrite_system(&theory).map_err(|e| {
            TheoryDslError::RewriteSystemCheck {
                theory: spec.theory.clone(),
                message: e.to_string(),
            }
        })?;
        let warnings = report.warnings();
        if !warnings.is_empty() {
            return Err(TheoryDslError::UnsoundRewriteSystem {
                theory: spec.theory.clone(),
                warnings,
            });
        }
        Ok(theory)
    }
}

/// The names the imports bring into scope.
#[derive(Default)]
struct Scope {
    /// `F.n` and `A.n` for every sort `n` of an import, to its new name.
    qualified_sorts: HashMap<String, Arc<str>>,
    /// The same for operations.
    qualified_ops: HashMap<String, Arc<str>>,
    /// Bare names of sorts brought in by an exposing import or an import
    /// without an alias, the first import winning.
    bare_sorts: HashMap<String, Arc<str>>,
    /// The same for operations.
    bare_ops: HashMap<String, Arc<str>>,
}

impl Scope {
    /// Record `import` of `theory` and return the theory renamed.
    fn import(
        &mut self,
        import: &crate::document::ImportSpec,
        theory: &Theory,
        at: &DocumentPath,
    ) -> Result<Theory, TheoryDslError> {
        for (index, name) in import.expose.iter().enumerate() {
            let declared = theory.sorts.iter().any(|s| *s.name == **name)
                || theory.ops.iter().any(|o| *o.name == **name)
                || theory.eqs.iter().any(|e| *e.name == **name)
                || theory.directed_eqs.iter().any(|e| *e.name == **name)
                || theory.policies.iter().any(|p| *p.name == **name);
            if !declared {
                return Err(TheoryDslError::UnknownExposed {
                    from: import.from.clone(),
                    name: name.clone(),
                }
                .within(&at.key("expose").index(index)));
            }
        }
        let rename = |name: &str| -> Arc<str> {
            if import.expose.iter().any(|e| e == name) {
                Arc::from(name)
            } else {
                let prefix = import.alias.as_deref().unwrap_or(&import.from);
                Arc::from(format!("{prefix}_{name}"))
            }
        };
        let sort_map: HashMap<Arc<str>, Arc<str>> = theory
            .sorts
            .iter()
            .map(|s| (Arc::clone(&s.name), rename(&s.name)))
            .collect();
        let op_map: HashMap<Arc<str>, Arc<str>> = theory
            .ops
            .iter()
            .map(|o| (Arc::clone(&o.name), rename(&o.name)))
            .collect();
        let mut qualifiers = vec![import.from.as_str()];
        qualifiers.extend(import.alias.as_deref());
        for (old, new) in &sort_map {
            for q in &qualifiers {
                self.qualified_sorts
                    .entry(format!("{q}.{old}"))
                    .or_insert_with(|| Arc::clone(new));
            }
            if import.alias.is_none() || import.expose.iter().any(|e| **e == **old) {
                self.bare_sorts
                    .entry(old.to_string())
                    .or_insert_with(|| Arc::clone(new));
            }
        }
        for (old, new) in &op_map {
            for q in &qualifiers {
                self.qualified_ops
                    .entry(format!("{q}.{old}"))
                    .or_insert_with(|| Arc::clone(new));
            }
            if import.alias.is_none() || import.expose.iter().any(|e| **e == **old) {
                self.bare_ops
                    .entry(old.to_string())
                    .or_insert_with(|| Arc::clone(new));
            }
        }
        Ok(rename_theory(theory, &sort_map, &op_map, &rename))
    }

    /// The renaming of sort heads and operation heads in the spec's own
    /// declarations.
    fn local_maps(&self, spec: &TheorySpec) -> Maps {
        let local_sorts: Vec<&str> = spec.sorts.iter().map(|s| s.name.as_str()).collect();
        let local_ops: Vec<&str> = spec.ops.iter().map(|o| o.name.as_str()).collect();
        let pick = |qualified: &HashMap<String, Arc<str>>,
                    bare: &HashMap<String, Arc<str>>,
                    local: &[&str]| {
            let mut map: HashMap<Arc<str>, Arc<str>> = bare
                .iter()
                .filter(|(name, _)| !local.contains(&name.as_str()))
                .map(|(name, new)| (Arc::from(name.as_str()), Arc::clone(new)))
                .collect();
            for (name, new) in qualified {
                if !local.contains(&name.as_str()) {
                    map.insert(Arc::from(name.as_str()), Arc::clone(new));
                }
            }
            map
        };
        Maps {
            sorts: pick(&self.qualified_sorts, &self.bare_sorts, &local_sorts),
            ops: pick(&self.qualified_ops, &self.bare_ops, &local_ops),
        }
    }
}

/// A renaming of sort and operation names.
struct Maps {
    sorts: HashMap<Arc<str>, Arc<str>>,
    ops: HashMap<Arc<str>, Arc<str>>,
}

impl Maps {
    fn sort(&self, sort: &SortExpr) -> SortExpr {
        sort.apply_maps(&self.sorts, &self.ops)
    }
}

/// `theory` with every declaration renamed by `rename` and every sort and
/// operation named inside a declaration renamed by the two maps.
fn rename_theory(
    theory: &Theory,
    sort_map: &HashMap<Arc<str>, Arc<str>>,
    op_map: &HashMap<Arc<str>, Arc<str>>,
    rename: &dyn Fn(&str) -> Arc<str>,
) -> Theory {
    let sorts = theory
        .sorts
        .iter()
        .map(|s| Sort {
            name: rename(&s.name),
            params: s
                .params
                .iter()
                .map(|p| SortParam::new(Arc::clone(&p.name), p.sort.apply_maps(sort_map, op_map)))
                .collect(),
            kind: s.kind.clone(),
            closure: match &s.closure {
                panproto_gat::SortClosure::Open => panproto_gat::SortClosure::Open,
                panproto_gat::SortClosure::Closed(ctors) => panproto_gat::SortClosure::Closed(
                    ctors
                        .iter()
                        .map(|c| op_map.get(c).cloned().unwrap_or_else(|| Arc::clone(c)))
                        .collect(),
                ),
            },
        })
        .collect();
    let ops = theory
        .ops
        .iter()
        .map(|o| {
            Operation::with_implicit(
                rename(&o.name),
                o.inputs
                    .iter()
                    .map(|(n, s, i)| (Arc::clone(n), s.apply_maps(sort_map, op_map), *i))
                    .collect(),
                o.output.apply_maps(sort_map, op_map),
            )
        })
        .collect();
    let eqs = theory
        .eqs
        .iter()
        .map(|e| {
            Equation::new(
                rename(&e.name),
                e.lhs.rename_ops(op_map),
                e.rhs.rename_ops(op_map),
            )
        })
        .collect();
    let directed_eqs = theory
        .directed_eqs
        .iter()
        .map(|e| {
            let mut renamed = e.rename_ops(op_map);
            renamed.name = rename(&e.name);
            renamed
        })
        .collect();
    let policies = theory
        .policies
        .iter()
        .map(|p| ConflictPolicy {
            name: rename(&p.name),
            value_kind: p.value_kind,
            strategy: p.strategy.clone(),
        })
        .collect();
    Theory::full(
        Arc::clone(&theory.name),
        theory.extends.clone(),
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
    )
}

/// The spec's own declarations, parsed and with imported names resolved.
fn compile_local(spec: &TheorySpec, maps: &Maps) -> Result<Presentation, TheoryDslError> {
    let mut local = Presentation::default();
    let root = DocumentPath::root();
    for (index, sort) in spec.sorts.iter().enumerate() {
        let at = root.key("sorts").index(index);
        let mut compiled = compile_sort(sort, &spec.theory).map_err(|e| e.within(&at))?;
        compiled.params = compiled
            .params
            .iter()
            .map(|p| SortParam::new(Arc::clone(&p.name), maps.sort(&p.sort)))
            .collect();
        if let panproto_gat::SortClosure::Closed(ctors) = &compiled.closure {
            compiled.closure = panproto_gat::SortClosure::Closed(
                ctors
                    .iter()
                    .map(|c| maps.ops.get(c).cloned().unwrap_or_else(|| Arc::clone(c)))
                    .collect(),
            );
        }
        local.sorts.push((compiled, Origin::Local(at)));
    }
    for (index, op) in spec.ops.iter().enumerate() {
        let at = root.key("ops").index(index);
        let compiled = compile_op(op, &spec.theory).map_err(|e| e.within(&at))?;
        if op.input.is_some() {
            local.unary_ops.push(Arc::clone(&compiled.name));
        }
        let compiled = Operation::with_implicit(
            Arc::clone(&compiled.name),
            compiled
                .inputs
                .iter()
                .map(|(n, s, i)| (Arc::clone(n), maps.sort(s), *i))
                .collect(),
            maps.sort(&compiled.output),
        );
        local.ops.push((compiled, Origin::Local(at)));
    }
    for (index, eq) in spec.equations.iter().enumerate() {
        let at = root.key("equations").index(index);
        let compiled = compile_equation(eq, &spec.theory).map_err(|e| e.within(&at))?;
        local
            .eqs
            .push((compiled.rename_ops(&maps.ops), Origin::Local(at)));
    }
    for (index, eq) in spec.directed_equations.iter().enumerate() {
        let at = root.key("directed_equations").index(index);
        let compiled = compile_directed_eq(eq, &spec.theory).map_err(|e| e.within(&at))?;
        local
            .directed_eqs
            .push((compiled.rename_ops(&maps.ops), Origin::Local(at)));
    }
    for (index, policy) in spec.policies.iter().enumerate() {
        let at = root.key("policies").index(index);
        let compiled = compile_policy(policy, &spec.theory).map_err(|e| e.within(&at))?;
        local.policies.push((compiled, Origin::Local(at)));
    }
    Ok(local)
}

fn compile_sort(spec: &SortSpec, theory_name: &str) -> Result<Sort, TheoryDslError> {
    let params: Vec<SortParam> = spec
        .params
        .iter()
        .enumerate()
        .map(|(index, p)| {
            parse_sort_expr(&p.sort)
                .map(|sort| SortParam::new(p.name.as_str(), sort))
                .map_err(|msg| {
                    TheoryDslError::TermParse {
                        context: format!(
                            "parameter '{pname}' of sort '{sname}' in theory '{theory_name}'",
                            pname = p.name,
                            sname = spec.name,
                        ),
                        message: msg,
                    }
                    .within(&DocumentPath::root().key("params").index(index).key("sort"))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let at = |field: &str| DocumentPath::root().key("kind").key(field);
    let kind = match &spec.kind {
        SortKindSpec::Structural => SortKind::Structural,
        SortKindSpec::Val { value_kind } => {
            SortKind::Val(parse_value_kind(value_kind).map_err(|e| e.within(&at("value_kind")))?)
        }
        SortKindSpec::Coercion { from, to, class } => SortKind::Coercion {
            from: parse_value_kind(from).map_err(|e| e.within(&at("from")))?,
            to: parse_value_kind(to).map_err(|e| e.within(&at("to")))?,
            class: parse_coercion_class(class).map_err(|e| e.within(&at("class")))?,
        },
        SortKindSpec::Merger { value_kind } => {
            SortKind::Merger(parse_value_kind(value_kind).map_err(|e| e.within(&at("value_kind")))?)
        }
    };

    let closure = spec
        .closed
        .as_ref()
        .map_or(panproto_gat::SortClosure::Open, |ctors| {
            panproto_gat::SortClosure::Closed(ctors.iter().map(|c| Arc::from(c.as_str())).collect())
        });

    Ok(Sort {
        name: Arc::from(spec.name.as_str()),
        params,
        kind,
        closure,
    })
}

fn compile_op(spec: &OpSpec, theory_name: &str) -> Result<Operation, TheoryDslError> {
    let op_context = |suffix: &str| -> String {
        format!(
            "{suffix} of op '{opname}' in theory '{theory_name}'",
            opname = spec.name,
        )
    };
    let output = parse_sort_expr(&spec.output).map_err(|msg| {
        TheoryDslError::TermParse {
            context: op_context("output sort"),
            message: msg,
        }
        .within(&DocumentPath::root().key("output"))
    })?;
    Ok(match (&spec.input, &spec.inputs) {
        (Some(_), Some(_)) => {
            return Err(TheoryDslError::AmbiguousOpInputs {
                op: spec.name.clone(),
            });
        }
        (Some(input_sort), _) => {
            // Take the first character safely: byte slicing with [..1]
            // would panic on a multi-byte UTF-8 boundary. Fall back to a
            // conventional placeholder when the sort name is empty or
            // starts with a non-ASCII character (giving it a readable
            // default rather than an empty string).
            let first_char = input_sort.chars().next().filter(char::is_ascii_alphabetic);
            let param_name: String =
                first_char.map_or_else(|| "x".to_string(), |c| c.to_ascii_lowercase().to_string());
            let input = parse_sort_expr(input_sort).map_err(|msg| {
                TheoryDslError::TermParse {
                    context: op_context("input sort"),
                    message: msg,
                }
                .within(&DocumentPath::root().key("input"))
            })?;
            Operation::unary(spec.name.as_str(), param_name.as_str(), input, output)
        }
        (None, Some(inputs)) => {
            let input_triples: Vec<(Arc<str>, SortExpr, panproto_gat::Implicit)> = inputs
                .iter()
                .enumerate()
                .map(|(index, p)| {
                    parse_sort_expr(&p.sort)
                        .map(|sort| {
                            let imp = if p.implicit {
                                panproto_gat::Implicit::Yes
                            } else {
                                panproto_gat::Implicit::No
                            };
                            (Arc::from(p.name.as_str()), sort, imp)
                        })
                        .map_err(|msg| {
                            TheoryDslError::TermParse {
                                context: op_context(&format!("input '{pname}'", pname = p.name)),
                                message: msg,
                            }
                            .within(&DocumentPath::root().key("inputs").index(index).key("sort"))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Operation::with_implicit(spec.name.as_str(), input_triples, output)
        }
        (None, None) => Operation::nullary(spec.name.as_str(), output),
    })
}

/// Parse a sort string into a [`SortExpr`].
///
/// A bare identifier parses as [`SortExpr::Name`]; `Ident(arg1, arg2, ...)`
/// parses as a [`SortExpr::App`] with the argument list parsed as terms
/// via [`parse_term`]. An `Ident()` input with no arguments normalizes
/// to [`SortExpr::Name`] via the smart constructor.
///
/// # Errors
///
/// Returns an error describing the problem for: empty input, malformed
/// identifiers, unclosed parentheses, unexpected trailing input, or any
/// error propagated from parsing an argument term.
pub fn parse_sort_expr(s: &str) -> Result<SortExpr, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("empty sort expression".to_owned());
    }
    match trimmed.find('(') {
        None => {
            validate_symbol(trimmed, "sort name")?;
            Ok(SortExpr::Name(Arc::from(trimmed)))
        }
        Some(paren_pos) => {
            let head = trimmed[..paren_pos].trim();
            validate_symbol(head, "sort head")?;
            let inner = &trimmed[paren_pos + 1..];
            let close = find_matching_paren(inner)
                .ok_or_else(|| format!("unclosed parenthesis in sort expression: {trimmed:?}"))?;
            let trailing = inner[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(format!(
                    "unexpected trailing input after closing paren in sort expression \
                     {trimmed:?}: {trailing:?}"
                ));
            }
            let args_str = &inner[..close];
            let args = split_top_level_commas(args_str)
                .into_iter()
                .map(|arg| parse_term_at_depth(arg, 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SortExpr::app(Arc::from(head), args))
        }
    }
}

fn compile_equation(spec: &EquationSpec, theory_name: &str) -> Result<Equation, TheoryDslError> {
    let side = |text: &str, field: &str| {
        parse_term(text).map_err(|msg| {
            TheoryDslError::TermParse {
                context: format!(
                    "equation '{name}' in theory '{theory_name}'",
                    name = spec.name
                ),
                message: msg,
            }
            .within(&DocumentPath::root().key(field))
        })
    };
    let lhs = side(&spec.lhs, "lhs")?;
    let rhs = side(&spec.rhs, "rhs")?;
    Ok(Equation::new(spec.name.as_str(), lhs, rhs))
}

fn compile_directed_eq(
    spec: &DirectedEqSpec,
    theory_name: &str,
) -> Result<DirectedEquation, TheoryDslError> {
    let ctx = format!(
        "directed equation '{name}' in theory '{theory_name}'",
        name = spec.name
    );
    let at = |field: &str| DocumentPath::root().key(field);
    let side = |text: &str, field: &str| {
        parse_term(text).map_err(|msg| {
            TheoryDslError::TermParse {
                context: ctx.clone(),
                message: msg,
            }
            .within(&at(field))
        })
    };
    let lhs = side(&spec.lhs, "lhs")?;
    let rhs = side(&spec.rhs, "rhs")?;
    let impl_term = parse_expr(&spec.impl_expr, &ctx).map_err(|e| e.within(&at("impl_expr")))?;
    let inverse = spec
        .inverse
        .as_ref()
        .map(|inv| parse_expr(inv, &ctx).map_err(|e| e.within(&at("inverse"))))
        .transpose()?;

    let source_kind = spec
        .source_kind
        .as_deref()
        .map(|kind| parse_value_kind(kind).map_err(|e| e.within(&at("source_kind"))))
        .transpose()?;
    let target_kind = spec
        .target_kind
        .as_deref()
        .map(|kind| parse_value_kind(kind).map_err(|e| e.within(&at("target_kind"))))
        .transpose()?;
    let coercion_class =
        parse_coercion_class(&spec.coercion_class).map_err(|e| e.within(&at("coercion_class")))?;

    Ok(DirectedEquation {
        name: Arc::from(spec.name.as_str()),
        lhs,
        rhs,
        impl_term,
        inverse,
        source_kind,
        target_kind,
        coercion_class,
    })
}

fn compile_policy(spec: &PolicySpec, theory_name: &str) -> Result<ConflictPolicy, TheoryDslError> {
    let strategy = match &spec.strategy {
        StrategySpec::KeepLeft => ConflictStrategy::KeepLeft,
        StrategySpec::KeepRight => ConflictStrategy::KeepRight,
        StrategySpec::Fail => ConflictStrategy::Fail,
        StrategySpec::Custom { expr } => {
            let ctx = format!(
                "policy '{name}' in theory '{theory_name}'",
                name = spec.name
            );
            let parsed = parse_expr(expr, &ctx)
                .map_err(|e| e.within(&DocumentPath::root().key("strategy").key("expr")))?;
            ConflictStrategy::Custom(parsed)
        }
    };

    Ok(ConflictPolicy {
        name: Arc::from(spec.name.as_str()),
        value_kind: parse_value_kind(&spec.value_kind)
            .map_err(|e| e.within(&DocumentPath::root().key("value_kind")))?,
        strategy,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Parsing helpers
// ═══════════════════════════════════════════════════════════════════

fn parse_value_kind(s: &str) -> Result<ValueKind, TheoryDslError> {
    match s {
        "boolean" | "bool" => Ok(ValueKind::Bool),
        "integer" | "int" => Ok(ValueKind::Int),
        "float" | "number" => Ok(ValueKind::Float),
        "string" | "str" => Ok(ValueKind::Str),
        "bytes" => Ok(ValueKind::Bytes),
        "token" => Ok(ValueKind::Token),
        "null" => Ok(ValueKind::Null),
        "date-time" | "datetime" => Ok(ValueKind::DateTime),
        "date" => Ok(ValueKind::Date),
        "time" => Ok(ValueKind::Time),
        "decimal" => Ok(ValueKind::Decimal),
        "uuid" => Ok(ValueKind::Uuid),
        "any" => Ok(ValueKind::Any),
        other => Err(TheoryDslError::UnknownValueKind {
            kind: other.to_owned(),
        }),
    }
}

fn parse_coercion_class(s: &str) -> Result<CoercionClass, TheoryDslError> {
    match s {
        "iso" => Ok(CoercionClass::Iso),
        "retraction" => Ok(CoercionClass::Retraction),
        "projection" => Ok(CoercionClass::Projection),
        "opaque" => Ok(CoercionClass::Opaque),
        other => Err(TheoryDslError::UnknownCoercionClass {
            class: other.to_owned(),
        }),
    }
}

/// Parse an expression string via the panproto expression parser.
fn parse_expr(expr_str: &str, context: &str) -> Result<panproto_expr::Expr, TheoryDslError> {
    let tokens =
        panproto_expr_parser::tokenize(expr_str).map_err(|e| TheoryDslError::ExprParse {
            context: context.to_owned(),
            message: format!("tokenization failed: {e}"),
        })?;

    panproto_expr_parser::parse(&tokens).map_err(|errors| TheoryDslError::ExprParse {
        context: context.to_owned(),
        message: errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    })
}

/// Parse a term string into a GAT [`Term`](panproto_gat::Term).
///
/// Supports two forms:
/// - Variable: `x`, `my_var`
/// - Application: `op(arg1, arg2, ...)` with recursive arguments
///
/// Grammar:
/// ```text
/// term  ::= ident '(' term (',' term)* ')'   -- application
///          | ident                              -- variable
/// ident ::= [a-zA-Z_][a-zA-Z0-9_]*
/// ```
///
/// # Errors
///
/// Returns a descriptive message for parse failures: empty input,
/// malformed identifiers, unclosed parentheses, missing keywords, or
/// errors propagated from nested term parses.
pub fn parse_term(s: &str) -> Result<panproto_gat::Term, String> {
    parse_term_at_depth(s, 0)
}

/// How deeply a term may nest before the parser refuses to descend further.
///
/// The parser is recursive descent and the traversals that consume its
/// output — substitution, free-variable collection, alpha-equivalence,
/// normalisation — are recursive over the same structure. Neither fails on a
/// term deeper than the stack can hold: the process aborts on a signal no
/// caller can handle. A term arrives as text a caller did not necessarily
/// write, so the depth is bounded here, where the structure is built, and
/// every consumer downstream inherits the bound.
///
/// The value leaves several times the headroom the tightest environment
/// needs. An unoptimised build on a two-megabyte thread parses a little under
/// six hundred levels and traverses a little under a thousand, and a term
/// that nests past a hundred and twenty-eight applications is not one anybody
/// wrote by hand.
pub const MAX_TERM_NESTING_DEPTH: usize = 128;

/// [`parse_term`], `depth` levels into the descent.
fn parse_term_at_depth(s: &str, depth: usize) -> Result<panproto_gat::Term, String> {
    if depth > MAX_TERM_NESTING_DEPTH {
        return Err(format!(
            "term nests deeper than the {MAX_TERM_NESTING_DEPTH}-level limit"
        ));
    }

    let s = s.trim();
    if s.is_empty() {
        return Err("empty term string".to_owned());
    }

    if let Some(rest) = s.strip_prefix("case ") {
        return parse_case_term(rest, depth);
    }

    if let Some(rest) = s.strip_prefix("let ") {
        return parse_let_term(rest, depth);
    }

    if let Some(rest) = s.strip_prefix('?') {
        let rest = rest.trim();
        if rest.is_empty() {
            return Ok(panproto_gat::Term::Hole { name: None });
        }
        validate_identifier(rest, "hole name")?;
        return Ok(panproto_gat::Term::Hole {
            name: Some(Arc::from(rest)),
        });
    }

    match s.find('(') {
        None => {
            validate_identifier(s, "term variable")?;
            Ok(panproto_gat::Term::Var(Arc::from(s)))
        }
        Some(paren_pos) => {
            let op_name = s[..paren_pos].trim();
            validate_symbol(op_name, "term operation")?;
            let inner = &s[paren_pos + 1..];
            let close = find_matching_paren(inner)
                .ok_or_else(|| format!("unclosed parenthesis in term: {s:?}"))?;
            let trailing = inner[close + 1..].trim();
            if !trailing.is_empty() {
                return Err(format!(
                    "unexpected trailing input after closing paren in term {s:?}: {trailing:?}"
                ));
            }
            let args_str = &inner[..close];
            let args = split_top_level_commas(args_str)
                .into_iter()
                .map(|arg| parse_term_at_depth(arg, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(panproto_gat::Term::App {
                op: Arc::from(op_name),
                args,
            })
        }
    }
}

/// Parse the body of a `let` term, given the text following the
/// leading `let ` keyword.
///
/// Grammar:
///
/// ```text
/// let_body ::= ident '=' term 'in' term
/// ```
fn parse_let_term(rest: &str, depth: usize) -> Result<panproto_gat::Term, String> {
    let rest = rest.trim();
    let eq_pos = rest
        .find('=')
        .ok_or_else(|| format!("let term missing `=`: {rest:?}"))?;
    let name_part = rest[..eq_pos].trim();
    validate_identifier(name_part, "let binder")?;
    let after_eq = &rest[eq_pos + 1..];
    let in_pos = find_top_level_keyword(after_eq, "in")
        .ok_or_else(|| format!("let term missing `in`: {rest:?}"))?;
    let bound_str = after_eq[..in_pos].trim();
    let body_str = after_eq[in_pos + 2..].trim();
    let bound = parse_term_at_depth(bound_str, depth + 1)?;
    let body = parse_term_at_depth(body_str, depth + 1)?;
    Ok(panproto_gat::Term::Let {
        name: Arc::from(name_part),
        bound: Box::new(bound),
        body: Box::new(body),
    })
}

/// Parse the body of a `case` term, given the text following the
/// leading `case ` keyword.
///
/// Grammar:
///
/// ```text
/// case_body ::= scrutinee 'of' branch ('|' branch)* 'end'
/// branch    ::= ctor '(' binder (',' binder)* ')' '=>' body
/// ```
fn parse_case_term(rest: &str, depth: usize) -> Result<panproto_gat::Term, String> {
    let rest = rest.trim();
    let stripped = rest
        .strip_suffix("end")
        .ok_or_else(|| format!("case term missing trailing `end`: {rest:?}"))?
        .trim_end();
    let of_pos = find_top_level_keyword(stripped, "of")
        .ok_or_else(|| format!("case term missing `of` keyword: {rest:?}"))?;
    let scrutinee_str = stripped[..of_pos].trim();
    let branches_str = stripped[of_pos + 2..].trim();
    let scrutinee = parse_term_at_depth(scrutinee_str, depth + 1)?;

    let branch_parts = split_top_level_pipes(branches_str);
    if branch_parts.is_empty() {
        return Err(format!("case term has no branches: {rest:?}"));
    }
    let branches = branch_parts
        .into_iter()
        .map(|branch| parse_case_branch(branch, depth + 1))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(panproto_gat::Term::Case {
        scrutinee: Box::new(scrutinee),
        branches,
    })
}

fn parse_case_branch(s: &str, depth: usize) -> Result<panproto_gat::CaseBranch, String> {
    let s = s.trim();
    let arrow = s
        .find("=>")
        .ok_or_else(|| format!("case branch missing `=>`: {s:?}"))?;
    let head = s[..arrow].trim();
    let body_str = s[arrow + 2..].trim();
    let body = parse_term_at_depth(body_str, depth)?;

    let paren_pos = head
        .find('(')
        .ok_or_else(|| format!("case branch constructor missing `(`: {head:?}"))?;
    let ctor_name = head[..paren_pos].trim();
    validate_identifier(ctor_name, "case branch constructor")?;
    let inner = &head[paren_pos + 1..];
    let close = find_matching_paren(inner)
        .ok_or_else(|| format!("unclosed paren in case branch: {head:?}"))?;
    let trailing = inner[close + 1..].trim();
    if !trailing.is_empty() {
        return Err(format!(
            "unexpected trailing input in case branch {head:?}: {trailing:?}"
        ));
    }
    let binders_str = &inner[..close];
    let binders = if binders_str.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level_commas(binders_str)
            .into_iter()
            .map(|b| {
                let b = b.trim();
                validate_identifier(b, "case branch binder")?;
                Ok::<Arc<str>, String>(Arc::from(b))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(panproto_gat::CaseBranch {
        constructor: Arc::from(ctor_name),
        binders,
        body,
    })
}

/// Find a whitespace-delimited occurrence of `keyword` at the top level
/// (not inside parens). Returns the byte offset of the keyword start.
///
/// The scan walks characters, not bytes, so a term carrying a non-ASCII
/// character (`case \u{e9} of ...`) is scanned without ever slicing at a
/// position interior to a multi-byte character.
fn find_top_level_keyword(s: &str, keyword: &str) -> Option<usize> {
    debug_assert!(keyword.is_ascii(), "keyword {keyword:?} must be ASCII");
    let klen = keyword.len();
    let mut depth = 0i32;
    // The start of the string counts as a left word boundary.
    let mut prev_is_boundary = true;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && prev_is_boundary
            && s[i..].starts_with(keyword)
            && s[i + klen..].chars().next().is_none_or(char::is_whitespace)
        {
            return Some(i);
        }
        prev_is_boundary = ch.is_whitespace();
    }
    None
}

/// Split by `|` at the top level (not inside parens).
fn split_top_level_pipes(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            '|' if depth == 0 => {
                let p = s[start..i].trim();
                if !p.is_empty() {
                    parts.push(p);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

/// Validate a parsed identifier: non-empty, starts with `_` or a letter,
/// continues with letters, digits, or underscores. Returns a descriptive
/// error naming `kind` (e.g. "sort head", "term variable") when the
/// input violates the grammar.
fn validate_identifier(s: &str, kind: &str) -> Result<(), String> {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(format!("empty {kind}"));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(format!(
            "{kind} {s:?} must start with a letter or underscore"
        ));
    }
    for ch in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            return Err(format!("{kind} {s:?} contains invalid character {ch:?}"));
        }
    }
    Ok(())
}

/// Check a sort or operation name: an identifier, or `Q.n` for identifiers
/// `Q` and `n`, which names the symbol `n` of the import whose theory or
/// alias is `Q`.
fn validate_symbol(s: &str, kind: &str) -> Result<(), String> {
    match s.split_once('.') {
        Some((qualifier, name)) => {
            validate_identifier(qualifier, kind)?;
            validate_identifier(name, kind)
        }
        None => validate_identifier(s, kind),
    }
}

/// Find the position of the closing `)` that matches the opening `(`.
/// The input starts immediately after the opening `(`.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1u32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a string by commas at the top level (not inside parentheses).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_parse_term_var() -> TestResult {
        let term = parse_term("x")?;
        assert!(matches!(term, panproto_gat::Term::Var(ref v) if &**v == "x"));
        Ok(())
    }

    #[test]
    fn test_parse_term_non_ascii_scrutinee() -> TestResult {
        // The `of` and `in` scans walk characters, so a non-ASCII
        // scrutinee or bound term yields a grammar diagnostic instead of
        // slicing at a position interior to a multi-byte character.
        let Err(err) = parse_term("case \u{e9} of C(x) => x end") else {
            return Err("a non-identifier scrutinee is not a term".into());
        };
        assert!(
            err.contains("term variable"),
            "expected an identifier diagnostic, got {err:?}"
        );

        let Err(err) = parse_term("let x = \u{e9} in x") else {
            return Err("a non-identifier bound term is not a term".into());
        };
        assert!(
            err.contains("term variable"),
            "expected an identifier diagnostic, got {err:?}"
        );

        // The character-wise scan keeps the keyword semantics it replaced:
        // a well-formed case term still parses.
        let term = parse_term("case s of C(x) => x end")?;
        assert!(matches!(term, panproto_gat::Term::Case { .. }));
        Ok(())
    }

    #[test]
    fn test_parse_term_app() -> TestResult {
        let term = parse_term("f(x, y)")?;
        let panproto_gat::Term::App { ref op, ref args } = term else {
            return Err("expected App".into());
        };
        assert_eq!(&**op, "f");
        assert_eq!(args.len(), 2);
        Ok(())
    }

    #[test]
    fn test_parse_term_nested() -> TestResult {
        let term = parse_term("f(g(x), y)")?;
        let panproto_gat::Term::App { ref op, ref args } = term else {
            return Err("expected App".into());
        };
        assert_eq!(&**op, "f");
        assert_eq!(args.len(), 2);
        assert!(matches!(&args[0], panproto_gat::Term::App { .. }));
        Ok(())
    }

    #[test]
    fn test_compile_sort_simple() -> TestResult {
        let spec = SortSpec {
            name: "Vertex".to_owned(),
            params: vec![],
            kind: SortKindSpec::Structural,
            closed: None,
        };
        let sort = compile_sort(&spec, "Th")?;
        assert_eq!(&*sort.name, "Vertex");
        assert_eq!(sort.params, []);
        assert!(matches!(sort.kind, SortKind::Structural));
        Ok(())
    }

    #[test]
    fn test_compile_op_unary() -> TestResult {
        let spec = OpSpec {
            name: "src".to_owned(),
            input: Some("Edge".to_owned()),
            inputs: None,
            output: "Vertex".to_owned(),
        };
        let op = compile_op(&spec, "Th")?;
        assert_eq!(&*op.name, "src");
        assert_eq!(op.arity(), 1);
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_bare_name() -> TestResult {
        let e = parse_sort_expr("Ob")?;
        if let SortExpr::Name(n) = e {
            assert_eq!(&*n, "Ob");
        } else {
            return Err("expected Name".into());
        }
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_applied() -> TestResult {
        let e = parse_sort_expr("Hom(x, y)")?;
        if let SortExpr::App { name, args } = e {
            assert_eq!(&*name, "Hom");
            assert_eq!(args.len(), 2);
        } else {
            return Err("expected App".into());
        }
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_nested_args() -> TestResult {
        let e = parse_sort_expr("Tm(extend(G, A), B)")?;
        if let SortExpr::App { name, args } = e {
            assert_eq!(&*name, "Tm");
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0], panproto_gat::Term::App { .. }));
        } else {
            return Err("expected App".into());
        }
        Ok(())
    }

    #[test]
    fn test_parse_sort_expr_empty_input_errors() {
        assert!(parse_sort_expr("").is_err());
        assert!(parse_sort_expr("   ").is_err());
    }

    #[test]
    fn test_parse_sort_expr_unclosed_paren_errors() {
        assert!(parse_sort_expr("Tm(Ctx, A").is_err());
        assert!(parse_sort_expr("Hom(").is_err());
    }

    #[test]
    fn test_parse_sort_expr_trailing_garbage_errors() {
        assert!(parse_sort_expr("Tm(A) junk").is_err());
    }

    #[test]
    fn test_parse_sort_expr_malformed_identifier_errors() {
        assert!(parse_sort_expr("1Hom(x, y)").is_err());
        assert!(parse_sort_expr("Hom(1bad, y)").is_err());
    }

    #[test]
    fn test_parse_sort_expr_empty_arglist_normalizes_to_name() -> TestResult {
        // `Tm()` normalizes to `Name("Tm")` via the smart constructor,
        // so Display and hashing match a bare-name spelling.
        let e = parse_sort_expr("Tm()")?;
        assert_eq!(e, SortExpr::Name(Arc::from("Tm")));
        Ok(())
    }

    #[test]
    fn test_compile_op_with_dependent_output() -> TestResult {
        let spec = OpSpec {
            name: "id".to_owned(),
            input: None,
            inputs: Some(vec![crate::document::ParamSpec {
                name: "x".to_owned(),
                sort: "Ob".to_owned(),
                implicit: false,
            }]),
            output: "Hom(x, x)".to_owned(),
        };
        let op = compile_op(&spec, "Th")?;
        assert_eq!(&*op.name, "id");
        if let SortExpr::App { name, args } = &op.output {
            assert_eq!(&**name, "Hom");
            assert_eq!(args.len(), 2);
        } else {
            return Err("expected dependent Hom output".into());
        }
        Ok(())
    }

    #[test]
    fn test_compile_simple_theory() -> TestResult {
        let spec = TheorySpec {
            theory: "ThTest".to_owned(),
            extends: vec![],
            imports: vec![],
            sorts: vec![
                SortSpec {
                    name: "Vertex".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Structural,
                    closed: None,
                },
                SortSpec {
                    name: "Edge".to_owned(),
                    params: vec![],
                    kind: SortKindSpec::Structural,
                    closed: None,
                },
            ],
            ops: vec![OpSpec {
                name: "src".to_owned(),
                input: Some("Edge".to_owned()),
                inputs: None,
                output: "Vertex".to_owned(),
            }],
            equations: vec![],
            directed_equations: vec![],
            policies: vec![],
        };
        let theory = compile_theory(&spec)?;
        assert_eq!(&*theory.name, "ThTest");
        assert_eq!(theory.sorts.len(), 2);
        assert_eq!(theory.ops.len(), 1);
        Ok(())
    }

    #[test]
    fn an_unsound_rewrite_system_is_refused() -> TestResult {
        use crate::document::DirectedEqSpec;
        // `x -> x` rewrites a term to itself, so rewriting never makes
        // progress and the LPO order cannot orient it. Normalization
        // decides this theory's equality judgment by running the rule,
        // so the theory has to be refused rather than handed back with
        // the finding printed somewhere.
        let spec = TheorySpec {
            theory: "ThLooping".to_owned(),
            extends: vec![],
            imports: vec![],
            sorts: vec![SortSpec {
                name: "Str".to_owned(),
                params: vec![],
                kind: SortKindSpec::Val {
                    value_kind: "string".to_owned(),
                },
                closed: None,
            }],
            ops: vec![],
            equations: vec![],
            directed_equations: vec![DirectedEqSpec {
                name: "goes_nowhere".to_owned(),
                lhs: "x".to_owned(),
                rhs: "x".to_owned(),
                impl_expr: "x".to_owned(),
                inverse: Some("x".to_owned()),
                source_kind: Some("string".to_owned()),
                target_kind: Some("string".to_owned()),
                coercion_class: "iso".to_owned(),
            }],
            policies: vec![],
        };
        match compile_theory(&spec) {
            Err(TheoryDslError::UnsoundRewriteSystem { theory, warnings }) => {
                assert_eq!(theory, "ThLooping");
                assert!(
                    warnings.iter().any(|w| w.contains("goes_nowhere")),
                    "the refusal must name the offending rule: {warnings:?}",
                );
                Ok(())
            }
            other => {
                Err(format!("an unsound rewrite system must be refused, got {other:?}").into())
            }
        }
    }

    fn lying_iso_theory_spec() -> TheorySpec {
        use crate::document::DirectedEqSpec;
        TheorySpec {
            theory: "ThLying".to_owned(),
            extends: vec![],
            imports: vec![],
            sorts: vec![SortSpec {
                name: "Str".to_owned(),
                params: vec![],
                kind: SortKindSpec::Val {
                    value_kind: "string".to_owned(),
                },
                closed: None,
            }],
            ops: vec![OpSpec {
                name: "upper".to_owned(),
                input: Some("Str".to_owned()),
                inputs: None,
                output: "Str".to_owned(),
            }],
            equations: vec![],
            directed_equations: vec![DirectedEqSpec {
                name: "lying_upper_iso".to_owned(),
                lhs: "upper(x)".to_owned(),
                rhs: "x".to_owned(),
                impl_expr: "upper(x)".to_owned(),
                inverse: Some("x".to_owned()),
                source_kind: Some("string".to_owned()),
                target_kind: Some("string".to_owned()),
                coercion_class: "iso".to_owned(),
            }],
            policies: vec![],
        }
    }

    #[test]
    fn compile_theory_accepts_lying_iso_without_law_check() -> TestResult {
        // Baseline: plain compile does not consult coercion classes,
        // so a lying Iso declaration round-trips through the
        // compiler.
        let spec = lying_iso_theory_spec();
        let theory = compile_theory(&spec)?;
        assert_eq!(theory.directed_eqs.len(), 1);
        Ok(())
    }

    #[test]
    fn compile_theory_with_law_check_rejects_lying_iso() -> TestResult {
        let spec = lying_iso_theory_spec();
        let registry = panproto_lens::coercion_laws::CoercionSampleRegistry::with_defaults();
        let result = compile_theory_with_law_check(&spec, &registry);
        let Err(err) = result else {
            return Err("lying iso must be rejected".into());
        };
        match err {
            TheoryDslError::CoercionLawViolation {
                theory,
                violations,
                distinct_equations,
            } => {
                assert_eq!(theory, "ThLying");
                assert!(!violations.is_empty());
                assert!(violations.iter().all(|d| d.equation == "lying_upper_iso"));
                assert_eq!(distinct_equations, 1);
                // The structured payload must be preserved so
                // downstream consumers can match on the variant.
                assert!(
                    violations.iter().any(|d| matches!(
                        d.violation,
                        panproto_lens::coercion_laws::CoercionLawViolation::Backward { .. }
                            | panproto_lens::coercion_laws::CoercionLawViolation::Forward { .. }
                    )),
                    "expected at least one Backward or Forward structured violation, \
                     got {violations:?}",
                );
                Ok(())
            }
            other => Err(format!("expected CoercionLawViolation, got {other:?}").into()),
        }
    }

    #[test]
    fn compile_theory_with_law_check_accepts_honest_iso() -> TestResult {
        let spec = TheorySpec {
            theory: "ThHonest".to_owned(),
            extends: vec![],
            imports: vec![],
            sorts: vec![SortSpec {
                name: "Str".to_owned(),
                params: vec![],
                kind: SortKindSpec::Val {
                    value_kind: "string".to_owned(),
                },
                closed: None,
            }],
            // `id(x) -> x` rather than `x -> x`: a rule whose two sides
            // are the same term never makes progress, so it is not
            // LPO-terminating and the rewrite-system gate refuses it.
            ops: vec![OpSpec {
                name: "id".to_owned(),
                input: Some("Str".to_owned()),
                inputs: None,
                output: "Str".to_owned(),
            }],
            equations: vec![],
            directed_equations: vec![crate::document::DirectedEqSpec {
                name: "identity_iso".to_owned(),
                lhs: "id(x)".to_owned(),
                rhs: "x".to_owned(),
                impl_expr: "x".to_owned(),
                inverse: Some("x".to_owned()),
                source_kind: Some("string".to_owned()),
                target_kind: Some("string".to_owned()),
                coercion_class: "iso".to_owned(),
            }],
            policies: vec![],
        };
        let registry = panproto_lens::coercion_laws::CoercionSampleRegistry::with_defaults();
        let theory = compile_theory_with_law_check(&spec, &registry)?;
        assert_eq!(theory.directed_eqs.len(), 1);
        Ok(())
    }
}
