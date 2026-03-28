//! Free (initial) model construction.
//!
//! Generates the initial model of a theory by enumerating closed terms
//! up to a depth bound and quotienting by the theory's equations.
//! The free model is the minimal model satisfying the theory — useful
//! for generating test instances and computing protocol skeletons.

use std::collections::VecDeque;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::eq::Term;
use crate::error::GatError;
use crate::model::{Model, ModelValue};
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

/// Construct the free (initial) model of a theory.
///
/// The carrier set of each sort is the set of closed terms of that sort,
/// quotiented by the theory's equations using union-find. Operations are
/// defined by term application.
///
/// # Errors
///
/// Returns [`GatError::ModelError`] if the term count exceeds bounds.
pub fn free_model(theory: &Theory, config: &FreeModelConfig) -> Result<Model, GatError> {
    let terms_by_sort = generate_terms(theory, config)?;
    let (term_to_global, total_terms) = assign_global_indices(&terms_by_sort);
    let mut uf = quotient_by_equations(theory, &terms_by_sort, &term_to_global, total_terms);
    Ok(build_model(
        theory,
        &terms_by_sort,
        &term_to_global,
        &mut uf,
    ))
}

/// Topologically sort the theory's sorts so that parameter sorts are
/// ordered before the dependent sorts that reference them. Returns sort
/// names in dependency order.
fn topological_sort_sorts(theory: &Theory) -> Vec<Arc<str>> {
    let sort_names: FxHashSet<Arc<str>> =
        theory.sorts.iter().map(|s| Arc::clone(&s.name)).collect();
    let mut in_degree: FxHashMap<Arc<str>, usize> = FxHashMap::default();
    let mut dependents: FxHashMap<Arc<str>, Vec<Arc<str>>> = FxHashMap::default();

    for sort in &theory.sorts {
        in_degree.entry(Arc::clone(&sort.name)).or_insert(0);
        for param in &sort.params {
            if sort_names.contains(&param.sort) {
                *in_degree.entry(Arc::clone(&sort.name)).or_insert(0) += 1;
                dependents
                    .entry(Arc::clone(&param.sort))
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

    // Any remaining sorts (cyclic dependencies) appended at the end.
    for sort in &theory.sorts {
        if !result.contains(&sort.name) {
            result.push(Arc::clone(&sort.name));
        }
    }

    result
}

/// Phase 1: Generate all closed terms up to `max_depth`, indexed by sort.
///
/// For dependent sorts `S(x1: A1, ..., xn: An)`, terms are generated
/// fiber-by-fiber: for each tuple of parameter values drawn from the
/// carrier sets of A1...An, we find operations whose output sort is S
/// and whose parameter inputs match the fiber. All fiber terms are
/// collected under the base sort name S.
fn generate_terms(
    theory: &Theory,
    config: &FreeModelConfig,
) -> Result<FxHashMap<Arc<str>, Vec<Term>>, GatError> {
    let mut terms_by_sort: FxHashMap<Arc<str>, Vec<Term>> = FxHashMap::default();

    // Initialize in topological order so parameter sorts are ready first.
    let ordered_sorts = topological_sort_sorts(theory);
    for sort_name in &ordered_sorts {
        terms_by_sort.insert(Arc::clone(sort_name), Vec::new());
    }

    // Seed: nullary operations.
    for op in &theory.ops {
        if op.inputs.is_empty() {
            let term = Term::constant(Arc::clone(&op.name));
            if let Some(terms) = terms_by_sort.get_mut(&op.output) {
                if !terms.contains(&term) {
                    terms.push(term);
                }
            }
        }
    }

    // Iterate: generate terms at increasing depths.
    for _depth in 1..=config.max_depth {
        let new_terms = generate_depth(theory, &terms_by_sort);

        for sort_name in &ordered_sorts {
            let Some(new) = new_terms.get(sort_name) else {
                continue;
            };
            let Some(existing) = terms_by_sort.get_mut(sort_name) else {
                continue;
            };
            for t in new {
                if existing.len() >= config.max_terms_per_sort {
                    return Err(GatError::ModelError(format!(
                        "term count for sort '{sort_name}' exceeds limit {}",
                        config.max_terms_per_sort
                    )));
                }
                if !existing.contains(t) {
                    existing.push(t.clone());
                }
            }
        }
    }

    Ok(terms_by_sort)
}

/// Generate one depth level of terms by applying non-nullary ops to existing terms.
fn generate_depth(
    theory: &Theory,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
) -> FxHashMap<Arc<str>, Vec<Term>> {
    let mut new_terms: FxHashMap<Arc<str>, Vec<Term>> = FxHashMap::default();

    for op in &theory.ops {
        if op.inputs.is_empty() {
            continue;
        }

        let input_sorts: Vec<&Arc<str>> = op.inputs.iter().map(|(_, s)| s).collect();

        // Skip if any input sort has no terms.
        if input_sorts
            .iter()
            .any(|s| terms_by_sort.get(*s).is_none_or(Vec::is_empty))
        {
            continue;
        }

        let input_term_lists: Vec<&Vec<Term>> =
            input_sorts.iter().map(|s| &terms_by_sort[*s]).collect();

        for combo in cartesian_product(&input_term_lists) {
            let term = Term::app(Arc::clone(&op.name), combo);
            new_terms
                .entry(Arc::clone(&op.output))
                .or_default()
                .push(term);
        }
    }

    new_terms
}

/// Assign consecutive global indices to all generated terms.
fn assign_global_indices(
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
) -> (FxHashMap<Arc<str>, Vec<usize>>, usize) {
    let mut global_idx = 0usize;
    let mut term_to_global: FxHashMap<Arc<str>, Vec<usize>> = FxHashMap::default();

    for (sort, terms) in terms_by_sort {
        let indices: Vec<usize> = (global_idx..global_idx + terms.len()).collect();
        global_idx += terms.len();
        term_to_global.insert(Arc::clone(sort), indices);
    }

    (term_to_global, global_idx)
}

/// Phase 2: Quotient terms by equations using union-find with congruence closure.
///
/// Runs equation merging in a fixpoint loop: after each pass over all equations,
/// checks if any new merges occurred. Continues until no new merges happen,
/// ensuring the quotient is the full congruence closure.
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

    // Fixpoint loop: keep merging until no new merges occur.
    loop {
        let merges_before = uf.merge_count;

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

        if uf.merge_count == merges_before {
            break;
        }
    }

    uf
}

/// Phase 3: Build the Model from equivalence class representatives.
/// Format a term as a human-readable string (e.g., `mul(unit(), x)`).
///
/// This must be used consistently for both carrier set values and
/// operation results to ensure that `check_model` can match them.
fn term_to_string(term: &Term) -> String {
    match term {
        Term::Var(name) => name.to_string(),
        Term::App { op, args } if args.is_empty() => format!("{op}()"),
        Term::App { op, args } => {
            let arg_strs: Vec<String> = args.iter().map(term_to_string).collect();
            format!("{op}({})", arg_strs.join(", "))
        }
    }
}

fn build_model(
    theory: &Theory,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
    uf: &mut UnionFind,
) -> Model {
    let mut model = Model::new(&*theory.name);

    // Build a lookup table: term_string → representative_string.
    // This ensures operation results land in the carrier set.
    let mut term_string_to_rep: FxHashMap<String, String> = FxHashMap::default();
    let mut class_rep_string: FxHashMap<usize, String> = FxHashMap::default();
    for (sort, terms) in terms_by_sort {
        let indices = &term_to_global[sort];
        let mut seen_classes: FxHashSet<usize> = FxHashSet::default();

        for (i, term) in terms.iter().enumerate() {
            let rep = uf.find(indices[i]);
            let ts = term_to_string(term);
            if seen_classes.insert(rep) {
                // First term in this class becomes the representative string.
                class_rep_string.insert(rep, ts.clone());
            }
            // Map every term string to its class representative string.
            let rep_str = class_rep_string[&rep].clone();
            term_string_to_rep.insert(ts, rep_str);
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
    let lookup = Arc::new(term_string_to_rep);

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
            let arg_strs: Vec<&str> = args
                .iter()
                .map(|a| match a {
                    ModelValue::Str(s) => s.as_str(),
                    _ => "?",
                })
                .collect();
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
    var_sorts: &FxHashMap<Arc<str>, Arc<str>>,
    terms_by_sort: &FxHashMap<Arc<str>, Vec<Term>>,
    term_to_global: &FxHashMap<Arc<str>, Vec<usize>>,
    uf: &mut UnionFind,
) {
    let var_terms: Vec<(&Arc<str>, &Vec<Term>)> = vars
        .iter()
        .filter_map(|v| {
            let sort = var_sorts.get(v)?;
            let terms = terms_by_sort.get(sort)?;
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

/// Compute the cartesian product of multiple term lists.
fn cartesian_product(lists: &[&Vec<Term>]) -> Vec<Vec<Term>> {
    if lists.is_empty() {
        return vec![vec![]];
    }

    let mut result = vec![vec![]];
    for list in lists {
        let mut new_result = Vec::new();
        for existing in &result {
            for item in *list {
                let mut combo = existing.clone();
                combo.push(item.clone());
                new_result.push(combo);
            }
        }
        result = new_result;
    }
    result
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
        let model = free_model(&theory, &FreeModelConfig::default())?;
        assert_eq!(model.sort_interp["Carrier"].len(), 1);
        Ok(())
    }

    #[test]
    fn free_model_empty_theory() -> Result<(), Box<dyn std::error::Error>> {
        let theory = Theory::new("Empty", vec![Sort::simple("S")], vec![], vec![]);
        let model = free_model(&theory, &FreeModelConfig::default())?;
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
        let model = free_model(&theory, &FreeModelConfig::default())?;
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
        let model = free_model(&theory, &FreeModelConfig::default())?;
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
        let model = free_model(&theory, &config)?;
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
        let model = free_model(&theory, &FreeModelConfig::default())?;
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
        let model = free_model(&theory, &config)?;

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

        let model = free_model(&theory, &FreeModelConfig::default())?;
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
        let model = free_model(&theory, &config)?;

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
        let model = free_model(&theory, &FreeModelConfig::default())?;
        let result = model.eval("unit", &[])?;
        assert!(matches!(result, ModelValue::Str(_)));
        Ok(())
    }
}
