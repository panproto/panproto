//! Bounded approximation of the free (initial) model.
//!
//! Generates an approximation of the initial model of a theory by
//! enumerating closed terms up to [`FreeModelConfig::max_depth`] and
//! quotienting by the theory's equations. The result is exact (truly
//! initial) when no new terms appear at the final depth level; otherwise
//! it is a finite truncation. Check [`FreeModelResult::is_complete`] to
//! determine whether the bound was sufficient.

use std::collections::VecDeque;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::eq::Term;
use crate::error::GatError;
use crate::model::{Model, ModelValue};
use crate::sort::SortExpr;
use crate::theory::Theory;

/// Configuration for free model construction.
#[derive(Debug, Clone)]
pub struct FreeModelConfig {
    /// Maximum depth of term generation. Default: 3.
    pub max_depth: usize,
    /// Maximum number of terms per sort (safety bound). Default: 1000.
    pub max_terms_per_sort: usize,
}

impl Default for FreeModelConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            max_terms_per_sort: 1000,
        }
    }
}

/// Result of free model construction, including completeness status.
#[derive(Debug)]
pub struct FreeModelResult {
    /// The constructed model.
    pub model: Model,
    /// Whether the model is provably complete (initial). `true` when no
    /// new closed terms were generated at the final depth level, meaning
    /// increasing `max_depth` would not change the model.
    pub is_complete: bool,
}

/// Construct a bounded approximation of the free (initial) model.
///
/// The carrier set of each sort is the set of closed terms of that sort
/// (up to `max_depth`), quotiented by the theory's equations using
/// union-find. Operations are defined by term application.
///
/// When [`FreeModelResult::is_complete`] is `true`, the result is the
/// exact initial model (no new terms would appear at deeper levels).
///
/// # Errors
///
/// Returns [`GatError::ModelError`] if the term count exceeds bounds.
pub fn free_model(theory: &Theory, config: &FreeModelConfig) -> Result<FreeModelResult, GatError> {
    let (terms_by_fiber, is_complete) = generate_terms(theory, config)?;
    // Collapse fiber-indexed terms to head-indexed terms for the
    // downstream model interface, which exposes carriers by sort head
    // name. Seed empty entries for every declared sort so callers can
    // always look up a carrier by name.
    let mut terms_by_sort = collapse_fibers(&terms_by_fiber);
    for sort in &theory.sorts {
        terms_by_sort.entry(Arc::clone(&sort.name)).or_default();
    }
    let (term_to_global, total_terms) = assign_global_indices(&terms_by_sort);
    let mut uf = quotient_by_equations(theory, &terms_by_sort, &term_to_global, total_terms);
    let model = build_model(theory, &terms_by_sort, &term_to_global, &mut uf);
    Ok(FreeModelResult { model, is_complete })
}

/// Collapse a fiber-indexed term map down to a head-indexed term map.
/// All terms with the same head sort are unioned into a single carrier.
///
/// Soundness of the collapse under the downstream quotient: GAT
/// equations are sort-preserving (both sides of every equation have
/// the same output sort, enforced by `typecheck_equation`), and
/// congruence closure over a set of sort-preserving equations only
/// ever relates terms that are already in the same fiber. The
/// head-indexed carrier therefore exposes the fibered free model's
/// underlying set without identifying terms across fibers; consumers
/// that need fiber information should recover it from each term's
/// inferred output sort via `typecheck_term`.
fn collapse_fibers(
    terms_by_fiber: &FxHashMap<SortExpr, Vec<Term>>,
) -> FxHashMap<Arc<str>, Vec<Term>> {
    let mut out: FxHashMap<Arc<str>, Vec<Term>> = FxHashMap::default();
    for (fiber, terms) in terms_by_fiber {
        let head = Arc::clone(fiber.head());
        let bucket = out.entry(head).or_default();
        for t in terms {
            if !bucket.contains(t) {
                bucket.push(t.clone());
            }
        }
    }
    out
}

/// Topologically sort the theory's sorts so that parameter sorts are
/// ordered before the dependent sorts that reference them. Returns sort
/// names in dependency order.
///
/// # Errors
///
/// Returns [`GatError::CyclicSortDependency`] if cyclic dependencies exist.
fn topological_sort_sorts(theory: &Theory) -> Result<Vec<Arc<str>>, GatError> {
    let sort_names: FxHashSet<Arc<str>> =
        theory.sorts.iter().map(|s| Arc::clone(&s.name)).collect();
    let mut in_degree: FxHashMap<Arc<str>, usize> = FxHashMap::default();
    let mut dependents: FxHashMap<Arc<str>, Vec<Arc<str>>> = FxHashMap::default();

    for sort in &theory.sorts {
        in_degree.entry(Arc::clone(&sort.name)).or_insert(0);
        for param in &sort.params {
            let param_head = param.sort.head();
            if sort_names.contains(param_head) {
                *in_degree.entry(Arc::clone(&sort.name)).or_insert(0) += 1;
                dependents
                    .entry(Arc::clone(param_head))
                    .or_default()
                    .push(Arc::clone(&sort.name));
            }
        }
    }

    let mut initial: Vec<Arc<str>> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| Arc::clone(name))
        .collect();
    initial.sort(); // Deterministic ordering.
    let mut queue: VecDeque<Arc<str>> = initial.into_iter().collect();

    let mut result = Vec::new();
    while let Some(name) = queue.pop_front() {
        result.push(Arc::clone(&name));
        if let Some(deps) = dependents.get(&name) {
            for dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(Arc::clone(dep));
                    }
                }
            }
        }
    }

    // Reject cyclic sort dependencies instead of silently appending.
    if result.len() < theory.sorts.len() {
        let cyclic: Vec<String> = theory
            .sorts
            .iter()
            .filter(|s| !result.contains(&s.name))
            .map(|s| s.name.to_string())
            .collect();
        return Err(GatError::CyclicSortDependency(cyclic));
    }

    Ok(result)
}

/// Phase 1: Generate all closed terms up to `max_depth`, indexed by sort.
///
/// For dependent sorts `S(x1: A1, ..., xn: An)`, terms are generated
/// fiber-by-fiber: for each tuple of parameter values drawn from the
/// carrier sets of A1...An, we find operations whose output sort is S
/// and whose parameter inputs match the fiber. All fiber terms are
/// collected under the base sort name S.
/// Returns `(terms_by_sort, is_complete)` where `is_complete` is `true`
/// when no new terms were generated at the final depth level.
fn generate_terms(
    theory: &Theory,
    config: &FreeModelConfig,
) -> Result<(FxHashMap<SortExpr, Vec<Term>>, bool), GatError> {
    #![allow(clippy::type_complexity)]
    let mut terms_by_fiber: FxHashMap<SortExpr, Vec<Term>> = FxHashMap::default();

    // Run topological sort for its cycle check; the head ordering is no
    // longer consulted directly because we file terms under their
    // instantiated output sort.
    let _ = topological_sort_sorts(theory)?;

    // Seed: nullary operations. A nullary op's output sort cannot
    // reference any input (there are none), so `op.output` is already a
    // closed sort expression.
    for op in &theory.ops {
        if op.inputs.is_empty() {
            let term = Term::constant(Arc::clone(&op.name));
            let fiber = op.output.clone();
            let bucket = terms_by_fiber.entry(fiber).or_default();
            if !bucket.contains(&term) {
                bucket.push(term);
            }
        }
    }

    let mut last_depth_added = false;
    for _depth in 1..=config.max_depth {
        let new_terms = generate_depth(theory, &terms_by_fiber);

        let mut added_any = false;
        for (fiber, new) in new_terms {
            let bucket = terms_by_fiber.entry(fiber.clone()).or_default();
            for t in new {
                if bucket.len() >= config.max_terms_per_sort {
                    let head = fiber.head();
                    return Err(GatError::ModelError(format!(
                        "term count for sort '{head}' exceeds limit {}",
                        config.max_terms_per_sort
                    )));
                }
                if !bucket.contains(&t) {
                    bucket.push(t);
                    added_any = true;
                }
            }
        }
        last_depth_added = added_any;
    }

    let is_complete = !last_depth_added;
    Ok((terms_by_fiber, is_complete))
}

/// Generate one depth level of terms by applying non-nullary ops to
/// existing terms, matching argument fibers against the declared input
/// sort expressions under a running substitution.
fn generate_depth(
    theory: &Theory,
    terms_by_fiber: &FxHashMap<SortExpr, Vec<Term>>,
) -> FxHashMap<SortExpr, Vec<Term>> {
    let mut new_terms: FxHashMap<SortExpr, Vec<Term>> = FxHashMap::default();

    for op in &theory.ops {
        if op.inputs.is_empty() {
            continue;
        }
        let mut chosen: Vec<Term> = Vec::with_capacity(op.inputs.len());
        let mut theta: FxHashMap<Arc<str>, Term> = FxHashMap::default();
        extend_op_tuples(
            op,
            0,
            &mut chosen,
            &mut theta,
            terms_by_fiber,
            &mut new_terms,
        );
    }

    new_terms
}

/// Recursive helper: at slot `i`, try every candidate term whose fiber
/// matches `op.inputs[i].1.subst(&theta)` and extend θ with the chosen
/// term, recursing into slot `i + 1`. When `i == op.inputs.len()`,
/// materialise the application and file it under `op.output.subst(&θ)`.
fn extend_op_tuples(
    op: &crate::op::Operation,
    slot: usize,
    chosen: &mut Vec<Term>,
    theta: &mut FxHashMap<Arc<str>, Term>,
    terms_by_fiber: &FxHashMap<SortExpr, Vec<Term>>,
    new_terms: &mut FxHashMap<SortExpr, Vec<Term>>,
) {
    if slot == op.inputs.len() {
        let output_fiber = op.output.subst(theta);
        let term = Term::app(Arc::clone(&op.name), chosen.clone());
        new_terms.entry(output_fiber).or_default().push(term);
        return;
    }
    let (param_name, declared_sort, _implicit) = &op.inputs[slot];
    let expected_fiber = declared_sort.subst(theta);
    let Some(candidates) = terms_by_fiber.get(&expected_fiber) else {
        return;
    };
    for cand in candidates {
        chosen.push(cand.clone());
        theta.insert(Arc::clone(param_name), cand.clone());
        extend_op_tuples(op, slot + 1, chosen, theta, terms_by_fiber, new_terms);
        theta.remove(param_name);
        chosen.pop();
    }
}

/// Assign consecutive global indices to all generated terms.
///
/// Iterates sorts in sort-name order so that the resulting indices are
/// deterministic across runs, regardless of hash-table insertion order
/// upstream. Any downstream consumer that hashes or compares free-model
/// indices (the VCS layer in particular) depends on this determinism.
fn assign_global_indices(
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
) -> (FxHashMap<Arc<str>, Vec<usize>>, usize) {
    let mut global_idx = 0usize;
    let mut term_to_global: FxHashMap<Arc<str>, Vec<usize>> = FxHashMap::default();

    let mut sorted_keys: Vec<&Arc<str>> = terms_by_sort.keys().collect();
    sorted_keys.sort();
    for sort in sorted_keys {
        let terms = &terms_by_sort[sort];
        let indices: Vec<usize> = (global_idx..global_idx + terms.len()).collect();
        global_idx += terms.len();
        term_to_global.insert(Arc::clone(sort), indices);
    }

    (term_to_global, global_idx)
}

/// Phase 2: Quotient terms by equations using union-find with congruence closure.
///
/// Runs equation merging and congruence propagation in a fixpoint loop.
/// Congruence closure ensures that if `t1 ~ t2`, then for every operation
/// `f`, we also get `f(... t1 ...) ~ f(... t2 ...)` when both terms exist
/// in the generated set. This is necessary for the free model to be truly
/// initial (the quotient must be closed under all operation congruences).
fn quotient_by_equations(
    theory: &Theory,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
    total_terms: usize,
) -> UnionFind {
    let mut uf = UnionFind::new(total_terms);

    // Precompute variable sorts for each equation.
    let eq_info: Vec<_> = theory
        .eqs
        .iter()
        .map(|eq| {
            let vars: Vec<Arc<str>> = {
                let mut all = eq.lhs.free_vars();
                all.extend(eq.rhs.free_vars());
                all.into_iter().collect()
            };
            let var_sorts = crate::typecheck::infer_var_sorts(eq, theory).ok();
            (eq, vars, var_sorts)
        })
        .collect();

    // Build a congruence index: for each compound term f(a1, ..., an),
    // record (op_name, [global_idx_of_a1, ..., global_idx_of_an]) -> global_idx.
    // This allows efficient congruence closure propagation.
    let congruence_entries = build_congruence_index(terms_by_sort, term_to_global);

    // Fixpoint loop: keep merging until no new merges occur.
    loop {
        let merges_before = uf.merge_count;

        // Pass 1: equation substitution instances.
        for (eq, vars, var_sorts) in &eq_info {
            if vars.is_empty() {
                merge_constant_eq(eq, terms_by_sort, term_to_global, &mut uf);
                continue;
            }

            let Some(vs) = var_sorts else {
                continue;
            };

            merge_by_equation(eq, vars, vs, terms_by_sort, term_to_global, &mut uf);
        }

        // Pass 2: congruence closure. If t1 ~ t2, then f(..., t1, ...) ~ f(..., t2, ...)
        // for all operations f where both compound terms exist.
        congruence_closure_pass(&congruence_entries, &mut uf);

        if uf.merge_count == merges_before {
            break;
        }
    }

    uf
}

/// Entry in the congruence index: a compound term with its operation name,
/// subterm global indices, and its own global index.
struct CongruenceEntry {
    /// Global index of this term.
    term_idx: usize,
    /// Global indices of each subterm (argument).
    arg_indices: Vec<usize>,
}

/// Build an index of all compound (non-nullary) generated terms, grouped by
/// operation name and arity. This enables efficient congruence closure.
fn build_congruence_index(
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
) -> FxHashMap<Arc<str>, Vec<CongruenceEntry>> {
    let mut index: FxHashMap<Arc<str>, Vec<CongruenceEntry>> = FxHashMap::default();

    // Build a flat lookup: term -> global_idx for subterm resolution.
    let mut term_lookup: FxHashMap<&Term, usize> = FxHashMap::default();
    for (sort, terms) in terms_by_sort {
        let indices = &term_to_global[sort];
        for (i, term) in terms.iter().enumerate() {
            term_lookup.insert(term, indices[i]);
        }
    }

    for (sort, terms) in terms_by_sort {
        let indices = &term_to_global[sort];
        for (i, term) in terms.iter().enumerate() {
            if let Term::App { op, args } = term {
                if args.is_empty() {
                    continue;
                }
                let arg_indices: Vec<usize> = args
                    .iter()
                    .filter_map(|arg| term_lookup.get(arg).copied())
                    .collect();
                // Only include if all subterms were found.
                if arg_indices.len() == args.len() {
                    index
                        .entry(Arc::clone(op))
                        .or_default()
                        .push(CongruenceEntry {
                            term_idx: indices[i],
                            arg_indices,
                        });
                }
            }
        }
    }

    index
}

/// Propagate congruence: for terms sharing the same operation, if their
/// argument tuples are pointwise equivalent under the union-find, merge them.
fn congruence_closure_pass(
    entries: &FxHashMap<Arc<str>, Vec<CongruenceEntry>>,
    uf: &mut UnionFind,
) {
    for group in entries.values() {
        if group.len() < 2 {
            continue;
        }
        // Group entries by their canonical argument tuple.
        let mut canonical_groups: FxHashMap<Vec<usize>, usize> = FxHashMap::default();
        for entry in group {
            let canonical_args: Vec<usize> =
                entry.arg_indices.iter().map(|&i| uf.find(i)).collect();
            if let Some(&representative) = canonical_groups.get(&canonical_args) {
                uf.union(representative, entry.term_idx);
            } else {
                canonical_groups.insert(canonical_args, uf.find(entry.term_idx));
            }
        }
    }
}

/// Phase 3: Build the Model from equivalence class representatives.
/// Format a term as a human-readable string (e.g., `mul(unit(), x)`).
///
/// This must be used consistently for both carrier set values and
/// operation results to ensure that `check_model` can match them.
/// Check whether a term is built entirely from `Var` and `App` nodes.
/// The free-model generator only produces App-only terms, so this
/// invariant holds on every term that reaches `build_model`'s
/// stringification path and makes `term_to_string` injective.
fn is_app_only(term: &Term) -> bool {
    match term {
        Term::Var(_) => true,
        Term::App { args, .. } => args.iter().all(is_app_only),
        Term::Case { .. } | Term::Hole { .. } | Term::Let { .. } => false,
    }
}

fn term_to_string(term: &Term) -> String {
    match term {
        Term::Var(name) => name.to_string(),
        Term::App { op, args } if args.is_empty() => format!("{op}()"),
        Term::App { op, args } => {
            let arg_strs: Vec<String> = args.iter().map(term_to_string).collect();
            format!("{op}({})", arg_strs.join(", "))
        }
        Term::Case {
            scrutinee,
            branches,
        } => {
            let branch_strs: Vec<String> = branches
                .iter()
                .map(|b| {
                    let binders = b
                        .binders
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    format!(
                        "{}({}) => {}",
                        b.constructor,
                        binders.join(", "),
                        term_to_string(&b.body)
                    )
                })
                .collect();
            format!(
                "case {} of {} end",
                term_to_string(scrutinee),
                branch_strs.join(" | ")
            )
        }
        Term::Hole { name } => name
            .as_ref()
            .map_or_else(|| "?".to_string(), |n| format!("?{n}")),
        Term::Let { name, bound, body } => format!(
            "let {name} = {} in {}",
            term_to_string(bound),
            term_to_string(body)
        ),
    }
}

fn build_model(
    theory: &Theory,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
    uf: &mut UnionFind,
) -> Model {
    let mut model = Model::new(&*theory.name);

    // String-keyed representative lookup. Safe because the free-model
    // generator emits only `Term::App` nodes via `extend_op_tuples`;
    // `term_to_string` is injective on App-only terms with App-only
    // arguments, so stringification does not collide across terms.
    // The debug assertion guards the invariant: every term seen here
    // must be App-only (holes, case terms, and let bindings are
    // produced by user input to the typechecker, never by the free
    // model enumerator).
    let mut class_rep_string: FxHashMap<usize, String> = FxHashMap::default();
    let mut string_to_rep: FxHashMap<String, String> = FxHashMap::default();
    for (sort, terms) in terms_by_sort {
        let indices = &term_to_global[sort];
        let mut seen_classes: FxHashSet<usize> = FxHashSet::default();

        for (i, term) in terms.iter().enumerate() {
            debug_assert!(
                is_app_only(term),
                "free-model generator emitted a non-App term: {term:?}",
            );
            let rep = uf.find(indices[i]);
            if seen_classes.insert(rep) {
                // First term in this class becomes the representative string.
                class_rep_string.insert(rep, term_to_string(term));
            }
            let rep_str = class_rep_string[&rep].clone();
            string_to_rep.insert(term_to_string(term), rep_str);
        }
    }

    // Build carrier sets using class representatives.
    for (sort, terms) in terms_by_sort {
        let indices = &term_to_global[sort];
        let mut seen_classes: FxHashSet<usize> = FxHashSet::default();
        let mut carrier = Vec::new();

        for (i, term) in terms.iter().enumerate() {
            let rep = uf.find(indices[i]);
            if seen_classes.insert(rep) {
                carrier.push(ModelValue::Str(term_to_string(term)));
            }
        }
        model.add_sort(sort.to_string(), carrier);
    }

    // Build operation interpretations that map carrier → carrier.
    // The lookup table is shared via Arc for the closures.
    let lookup = Arc::new(string_to_rep);

    for op in &theory.ops {
        let op_name = op.name.to_string();
        let arity = op.arity();
        let table = Arc::clone(&lookup);
        model.add_op(op_name.clone(), move |args: &[ModelValue]| {
            if args.len() != arity {
                return Err(GatError::ModelError(format!(
                    "operation '{op_name}' expects {arity} args, got {}",
                    args.len()
                )));
            }
            // Carrier values are always ModelValue::Str here because
            // free_model emits string carriers via term_to_string. A
            // non-string argument indicates a caller bug; surface it
            // rather than silently rendering as "?".
            let mut arg_strs: Vec<String> = Vec::with_capacity(args.len());
            for (i, a) in args.iter().enumerate() {
                match a {
                    ModelValue::Str(s) => arg_strs.push(s.clone()),
                    other => {
                        return Err(GatError::ModelError(format!(
                            "operation '{op_name}' received non-string argument at index {i}: {other:?}"
                        )));
                    }
                }
            }
            let result_str = format!("{op_name}({})", arg_strs.join(", "));

            // Look up the result in the term table. If found, return the
            // equivalence class representative. If not found (term exceeds
            // depth bound), return the formatted string as-is.
            Ok(ModelValue::Str(
                table.get(&result_str).map_or(result_str, String::clone),
            ))
        });
    }

    model
}

/// Merge terms identified by a constants-only equation.
fn merge_constant_eq(
    eq: &crate::eq::Equation,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
    uf: &mut UnionFind,
) {
    let lhs_idx = find_term_index(&eq.lhs, terms_by_sort, term_to_global);
    let rhs_idx = find_term_index(&eq.rhs, terms_by_sort, term_to_global);
    if let (Some(l), Some(r)) = (lhs_idx, rhs_idx) {
        uf.union(l, r);
    }
}

/// Find the global index of a closed term in the generated term set.
fn find_term_index(
    term: &Term,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
) -> Option<usize> {
    for (sort, terms) in terms_by_sort {
        for (i, t) in terms.iter().enumerate() {
            if t == term {
                return Some(term_to_global[sort][i]);
            }
        }
    }
    None
}

/// Enumerate substitutions and merge LHS/RHS when both match generated terms.
fn merge_by_equation(
    eq: &crate::eq::Equation,
    vars: &[Arc<str>],
    var_sorts: &FxHashMap<Arc<str>, SortExpr>,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
    uf: &mut UnionFind,
) {
    let var_terms: Vec<(&Arc<str>, &Vec<Term>)> = vars
        .iter()
        .filter_map(|v| {
            let sort = var_sorts.get(v)?;
            let terms = terms_by_sort.get(sort.head())?;
            Some((v, terms))
        })
        .collect();

    if var_terms.len() != vars.len() || var_terms.iter().any(|(_, terms)| terms.is_empty()) {
        return;
    }

    let mut indices = vec![0usize; var_terms.len()];

    loop {
        let mut subst = rustc_hash::FxHashMap::default();
        for (i, (var, terms)) in var_terms.iter().enumerate() {
            subst.insert(Arc::clone(var), terms[indices[i]].clone());
        }

        let lhs = eq.lhs.substitute(&subst);
        let rhs = eq.rhs.substitute(&subst);

        let lhs_idx = find_term_index(&lhs, terms_by_sort, term_to_global);
        let rhs_idx = find_term_index(&rhs, terms_by_sort, term_to_global);
        if let (Some(l), Some(r)) = (lhs_idx, rhs_idx) {
            uf.union(l, r);
        }

        let mut carry = true;
        for i in (0..indices.len()).rev() {
            if carry {
                indices[i] += 1;
                if indices[i] < var_terms[i].1.len() {
                    carry = false;
                } else {
                    indices[i] = 0;
                }
            }
        }
        if carry {
            break;
        }
    }
}

/// Simple union-find with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
    /// Total number of union operations that actually merged distinct classes.
    merge_count: usize,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
            merge_count: 0,
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // Path splitting.
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        self.merge_count += 1;
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::Equation;
    use crate::op::Operation;
    use crate::sort::Sort;
    use crate::theory::Theory;

    #[test]
    fn free_model_of_pointed_set() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "PointedSet",
            vec![Sort::simple("Carrier")],
            vec![Operation::nullary("unit", "Carrier")],
            vec![],
        );
        let result = free_model(&theory, &FreeModelConfig::default())?;
        assert_eq!(result.model.sort_interp["Carrier"].len(), 1);
        Ok(())
    }

    #[test]
    fn free_model_empty_theory() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new("Empty", vec![Sort::simple("S")], vec![], vec![]);
        let model = free_model(&theory, &FreeModelConfig::default())?.model;
        assert!(model.sort_interp["S"].is_empty());
        Ok(())
    }

    #[test]
    fn free_model_two_constants() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "TwoPoints",
            vec![Sort::simple("S")],
            vec![Operation::nullary("a", "S"), Operation::nullary("b", "S")],
            vec![],
        );
        let model = free_model(&theory, &FreeModelConfig::default())?.model;
        assert_eq!(model.sort_interp["S"].len(), 2);
        Ok(())
    }

    #[test]
    fn free_model_equation_collapses_constants() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "CollapsedPoints",
            vec![Sort::simple("S")],
            vec![Operation::nullary("a", "S"), Operation::nullary("b", "S")],
            vec![Equation::new(
                "a_eq_b",
                Term::constant("a"),
                Term::constant("b"),
            )],
        );
        let model = free_model(&theory, &FreeModelConfig::default())?.model;
        assert_eq!(model.sort_interp["S"].len(), 1);
        Ok(())
    }

    #[test]
    fn free_model_monoid_identity_collapses() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "Monoid",
            vec![Sort::simple("Carrier")],
            vec![
                Operation::new(
                    "mul",
                    vec![
                        ("a".into(), "Carrier".into()),
                        ("b".into(), "Carrier".into()),
                    ],
                    "Carrier",
                ),
                Operation::nullary("unit", "Carrier"),
            ],
            vec![
                Equation::new(
                    "left_id",
                    Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
                    Term::var("a"),
                ),
                Equation::new(
                    "right_id",
                    Term::app("mul", vec![Term::var("a"), Term::constant("unit")]),
                    Term::var("a"),
                ),
            ],
        );
        let config = FreeModelConfig {
            max_depth: 1,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;
        assert_eq!(model.sort_interp["Carrier"].len(), 1);
        Ok(())
    }

    #[test]
    fn free_model_graph_theory() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "Graph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![],
        );
        let model = free_model(&theory, &FreeModelConfig::default())?.model;
        assert!(model.sort_interp["Vertex"].is_empty());
        assert!(model.sort_interp["Edge"].is_empty());
        Ok(())
    }

    #[test]
    fn free_model_term_count_bounded() {
        let theory = Theory::new(
            "Chain",
            vec![Sort::simple("S")],
            vec![
                Operation::nullary("zero", "S"),
                Operation::unary("succ", "x", "S", "S"),
            ],
            vec![],
        );
        let config = FreeModelConfig {
            max_depth: 10,
            max_terms_per_sort: 5,
        };
        let result = free_model(&theory, &config);
        assert!(matches!(result, Err(GatError::ModelError(_))));
    }

    /// Free model of a category theory with dependent sorts.
    /// Ob (objects), Hom(a: Ob, b: Ob) (morphisms), id: Ob -> Hom(a, a).
    /// With one object constant, should generate the identity morphism.
    #[test]
    fn free_model_category_theory() -> Result<(), Box<dyn std::error::Error>> {
        use crate::sort::SortParam;

        let theory = Theory::new(
            "Category",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![
                Operation::nullary("star", "Ob"),
                // id: Ob -> Hom (in practice produces Hom(x, x))
                Operation::unary("id", "x", "Ob", "Hom"),
            ],
            Vec::new(),
        );

        let config = FreeModelConfig {
            max_depth: 2,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;

        // Ob should have one element: star().
        assert_eq!(model.sort_interp["Ob"].len(), 1);

        // Hom should have at least id(star()).
        assert!(
            !model.sort_interp["Hom"].is_empty(),
            "Hom should have at least the identity morphism"
        );
        Ok(())
    }

    /// Dependent sort with no operations targeting it produces empty carrier.
    #[test]
    fn free_model_dependent_sort_no_ops() -> Result<(), Box<dyn std::error::Error>> {
        use crate::sort::SortParam;

        let theory = Theory::new(
            "T",
            vec![
                Sort::simple("A"),
                Sort::dependent("B", vec![SortParam::new("x", "A")]),
            ],
            vec![Operation::nullary("a", "A")],
            Vec::new(),
        );

        let model = free_model(&theory, &FreeModelConfig::default())?.model;
        assert_eq!(model.sort_interp["A"].len(), 1);
        assert!(
            model.sort_interp["B"].is_empty(),
            "B has no operations targeting it, so carrier should be empty"
        );
        Ok(())
    }

    /// Topological ordering ensures parameter sorts are populated first.
    #[test]
    fn free_model_sort_ordering() -> Result<(), Box<dyn std::error::Error>> {
        use crate::sort::SortParam;

        // Deliberately put the dependent sort first in the list.
        let theory = Theory::new(
            "T",
            vec![
                Sort::dependent("B", vec![SortParam::new("x", "A")]),
                Sort::simple("A"),
            ],
            vec![
                Operation::nullary("a", "A"),
                Operation::unary("f", "x", "A", "B"),
            ],
            Vec::new(),
        );

        let config = FreeModelConfig {
            max_depth: 1,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;

        // A should have a().
        assert_eq!(model.sort_interp["A"].len(), 1);
        // B should have f(a()).
        assert_eq!(model.sort_interp["B"].len(), 1);
        Ok(())
    }

    #[test]
    fn free_model_operations_work() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "PointedSet",
            vec![Sort::simple("Carrier")],
            vec![Operation::nullary("unit", "Carrier")],
            vec![],
        );
        let model = free_model(&theory, &FreeModelConfig::default())?.model;
        let result = model.eval("unit", &[])?;
        assert!(matches!(result, ModelValue::Str(_)));
        Ok(())
    }

    #[test]
    fn free_model_congruence_closure() -> Result<(), Box<dyn std::error::Error>> {
        // Theory with a = b and f: S -> S.
        // Congruence closure requires f(a) ~ f(b), even though no equation
        // directly equates them. The equation a = b combined with the
        // congruence rule for f must produce this.
        let theory = Theory::new(
            "Congruence",
            vec![Sort::simple("S")],
            vec![
                Operation::nullary("a", "S"),
                Operation::nullary("b", "S"),
                Operation::unary("f", "x", "S", "S"),
            ],
            vec![Equation::new(
                "a_eq_b",
                Term::constant("a"),
                Term::constant("b"),
            )],
        );
        let config = FreeModelConfig {
            max_depth: 1,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;
        // a ~ b, so f(a) ~ f(b). The carrier should have at most 2 elements:
        // one equivalence class for {a, b} and one for {f(a), f(b)}.
        assert_eq!(
            model.sort_interp["S"].len(),
            2,
            "a ~ b and f(a) ~ f(b) by congruence: expect 2 classes"
        );
        Ok(())
    }

    /// Free category on two generating morphisms. With one object and
    /// one endo-generator, the expected terms at depth 2 are:
    /// `id(star)`, `f(star)`, `f(f(star))`. Exactly three morphisms in
    /// the `Hom` fiber, demonstrating that fiber matching prevents the
    /// combinatorial blow-up of a cartesian-product model.
    #[test]
    fn free_model_dependent_category() -> Result<(), Box<dyn std::error::Error>> {
        use crate::sort::{SortExpr, SortParam};

        let hom_xx = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("x")],
        };
        let theory = Theory::new(
            "EndoCategory",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![
                Operation::nullary("star", "Ob"),
                Operation::unary("id", "x", "Ob", hom_xx.clone()),
                Operation::unary("f", "x", "Ob", hom_xx),
            ],
            Vec::new(),
        );

        let config = FreeModelConfig {
            max_depth: 2,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;

        // Ob: exactly one element (star).
        assert_eq!(model.sort_interp["Ob"].len(), 1);
        // Hom: id(star), f(star); f(f(star)) exists only if we stack
        // unary ops with matching fibers, which holds here because
        // f: (x: Ob) -> Hom(x, x) maps an Ob to a Hom(x, x), but the
        // argument of f must itself be an Ob. So at depth 2, the Hom
        // carrier holds id(star) and f(star) only.
        assert_eq!(
            model.sort_interp["Hom"].len(),
            2,
            "expected id(star) and f(star) in Hom fiber"
        );
        Ok(())
    }

    /// Free category on two parallel generating arrows `f, g : Hom(a, b)`
    /// at depth 1 has `{id(a), id(b), f(a, b), g(a, b)}` (4 morphisms), not
    /// 6 or more. `compose(f, g)` cannot form because `tgt(f) = b` but
    /// `src(g) = a`, so the middle-object constraint rules out composites
    /// in either order. This is the fiber-matching test that distinguishes
    /// a dependent-sort-aware generator from a cartesian-product
    /// generator.
    #[test]
    fn free_model_parallel_arrows_no_spurious_composites() -> Result<(), Box<dyn std::error::Error>>
    {
        use crate::sort::{SortExpr, SortParam};

        let hom_ab = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::constant("a"), Term::constant("b")],
        };
        let hom_xy = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("y")],
        };
        let theory = Theory::new(
            "ParallelArrows",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![
                Operation::nullary("a", "Ob"),
                Operation::nullary("b", "Ob"),
                Operation::nullary("f", hom_ab.clone()),
                Operation::nullary("g", hom_ab),
                Operation::unary(
                    "id",
                    "x",
                    "Ob",
                    SortExpr::App {
                        name: Arc::from("Hom"),
                        args: vec![Term::var("x"), Term::var("x")],
                    },
                ),
                Operation::new(
                    "compose",
                    vec![
                        (Arc::from("x"), SortExpr::from("Ob")),
                        (Arc::from("y"), SortExpr::from("Ob")),
                        (Arc::from("z"), SortExpr::from("Ob")),
                        (
                            Arc::from("h1"),
                            SortExpr::App {
                                name: Arc::from("Hom"),
                                args: vec![Term::var("x"), Term::var("y")],
                            },
                        ),
                        (
                            Arc::from("h2"),
                            SortExpr::App {
                                name: Arc::from("Hom"),
                                args: vec![Term::var("y"), Term::var("z")],
                            },
                        ),
                    ],
                    hom_xy,
                ),
            ],
            Vec::new(),
        );

        let config = FreeModelConfig {
            max_depth: 1,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;

        // Ob: {a, b}.
        assert_eq!(model.sort_interp["Ob"].len(), 2);
        // Hom at depth 1: id(a), id(b), f, g. No composites because f and
        // g share source/target (a, b), so compose(f, g) would require
        // tgt(f) = b = src(g) = a, which fails.
        assert_eq!(
            model.sort_interp["Hom"].len(),
            4,
            "Hom fiber should contain {{id(a), id(b), f, g}}, got {:?}",
            model.sort_interp["Hom"],
        );
        Ok(())
    }

    /// Every term in the free model has a well-typed output sort.
    #[test]
    fn free_model_every_term_well_typed() -> Result<(), Box<dyn std::error::Error>> {
        use crate::sort::{SortExpr, SortParam};
        use crate::typecheck::{VarContext, typecheck_term};

        let hom_xx = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("x")],
        };
        let theory = Theory::new(
            "EndoCat",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![
                Operation::nullary("star", "Ob"),
                Operation::unary("id", "x", "Ob", hom_xx.clone()),
                Operation::unary("f", "x", "Ob", hom_xx),
            ],
            Vec::new(),
        );

        let config = FreeModelConfig {
            max_depth: 2,
            max_terms_per_sort: 100,
        };
        let (fibers, _) = generate_terms(&theory, &config)?;
        let ctx = VarContext::default();
        for (fiber, terms) in &fibers {
            for term in terms {
                let inferred = typecheck_term(term, &ctx, &theory)?;
                assert!(
                    inferred.alpha_eq(fiber),
                    "term {term} has fiber {fiber} but typecheck inferred {inferred}",
                );
            }
        }
        Ok(())
    }

    /// For theories with only simple sorts, the fiber-matching generator
    /// reduces to the old head-indexed cartesian-product behavior. Verify
    /// that a simple-sort graph theory produces the expected carrier
    /// counts.
    #[test]
    fn free_model_simple_sorts_backward_compat() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new(
            "Graph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::nullary("v0", "Vertex"),
                Operation::nullary("v1", "Vertex"),
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            Vec::new(),
        );
        let config = FreeModelConfig {
            max_depth: 1,
            max_terms_per_sort: 100,
        };
        let model = free_model(&theory, &config)?.model;
        // Two vertices at depth 0; src/tgt need an Edge which is empty.
        // So at any depth: Vertex = {v0, v1}, Edge = {}.
        assert_eq!(model.sort_interp["Vertex"].len(), 2);
        assert!(model.sort_interp["Edge"].is_empty());
        Ok(())
    }

    #[test]
    fn free_model_cyclic_sort_dependency_rejected() {
        use crate::sort::SortParam;

        // Sort A depends on B and B depends on A: cyclic.
        let theory = Theory::new(
            "Cyclic",
            vec![
                Sort::dependent("A", vec![SortParam::new("x", "B")]),
                Sort::dependent("B", vec![SortParam::new("y", "A")]),
            ],
            vec![],
            vec![],
        );
        let result = free_model(&theory, &FreeModelConfig::default());
        assert!(
            matches!(result, Err(GatError::CyclicSortDependency(_))),
            "cyclic sort dependencies should be rejected"
        );
    }
}
