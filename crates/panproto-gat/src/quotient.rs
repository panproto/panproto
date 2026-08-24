use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::eq::{DirectedEquation, Equation, alpha_equivalent_equation};
use crate::error::GatError;
use crate::op::Operation;
use crate::sort::{Sort, SortParam};
use crate::theory::Theory;

/// HashMap-based union-find over `Arc<str>` with path compression
/// and alphabetically-first representative selection.
struct NameUnionFind {
    parent: FxHashMap<Arc<str>, Arc<str>>,
}

impl NameUnionFind {
    fn new() -> Self {
        Self {
            parent: FxHashMap::default(),
        }
    }

    /// Ensure a name exists in the union-find.
    fn insert(&mut self, name: Arc<str>) {
        self.parent.entry(name.clone()).or_insert(name);
    }

    /// Find the representative for `name` with path compression.
    fn find(&mut self, name: &Arc<str>) -> Arc<str> {
        if !self.parent.contains_key(name) {
            self.parent.insert(name.clone(), name.clone());
        }
        // Safety: we just ensured the key exists above.
        let p = self.parent[name].clone();
        if &p == name {
            return p;
        }
        let root = self.find(&p);
        self.parent.insert(name.clone(), root.clone());
        root
    }

    /// Union two names, choosing the alphabetically-first as representative.
    fn union(&mut self, a: &Arc<str>, b: &Arc<str>) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if ra <= rb {
            self.parent.insert(rb, ra);
        } else {
            self.parent.insert(ra, rb);
        }
    }

    /// Get the rename map: for each name, its representative.
    /// Only includes entries where the name differs from its representative.
    fn rename_map(&mut self) -> FxHashMap<Arc<str>, Arc<str>> {
        let keys: Vec<Arc<str>> = self.parent.keys().cloned().collect();
        let mut map = FxHashMap::default();
        for k in keys {
            let rep = self.find(&k);
            if rep != k {
                map.insert(k, rep);
            }
        }
        map
    }

    /// Get all equivalence classes as a map from representative to members.
    fn classes(&mut self) -> FxHashMap<Arc<str>, Vec<Arc<str>>> {
        let keys: Vec<Arc<str>> = self.parent.keys().cloned().collect();
        let mut classes: FxHashMap<Arc<str>, Vec<Arc<str>>> = FxHashMap::default();
        for k in keys {
            let rep = self.find(&k);
            classes.entry(rep).or_default().push(k);
        }
        classes
    }
}

/// Look up a sort name, returning `GatError::SortNotFound` on miss.
fn get_sort<'a>(theory: &'a Theory, name: &str) -> Result<&'a Sort, GatError> {
    theory
        .find_sort(name)
        .ok_or_else(|| GatError::SortNotFound(name.to_owned()))
}

/// Look up an op name, returning `GatError::OpNotFound` on miss.
fn get_op<'a>(theory: &'a Theory, name: &str) -> Result<&'a Operation, GatError> {
    theory
        .find_op(name)
        .ok_or_else(|| GatError::OpNotFound(name.to_owned()))
}

/// Rename a sort reference through the rename map.
fn apply_sort_rename(name: &Arc<str>, rename: &RenameMap) -> Arc<str> {
    rename.get(name).cloned().unwrap_or_else(|| name.clone())
}

/// Rename an op reference through the rename map.
fn apply_op_rename(name: &Arc<str>, rename: &RenameMap) -> Arc<str> {
    rename.get(name).cloned().unwrap_or_else(|| name.clone())
}

/// Convert a rename map to the `std::collections::HashMap` form that the
/// sort-expression rewriting API takes.
fn as_std_map(rename: &RenameMap) -> std::collections::HashMap<Arc<str>, Arc<str>> {
    rename.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Compute the quotiented signature of an operation: its input sort list
/// and output sort, with every sort head sent to its representative and
/// every operation applied inside a dependent sort's argument terms sent
/// to its representative.
///
/// Both maps matter. A dependent sort argument is a term, so two
/// operations whose signatures differ only in operations the quotient
/// identifies — `Hom(pt1(), pt1())` against `Hom(pt2(), pt2())` under
/// `pt1 ~ pt2` — have the same signature in the quotient.
fn renamed_op_signature(
    op: &Operation,
    sort_rename: &RenameMap,
    op_rename: &RenameMap,
) -> (Vec<crate::sort::SortExpr>, crate::sort::SortExpr) {
    let sort_std = as_std_map(sort_rename);
    let op_std = as_std_map(op_rename);
    let inputs: Vec<crate::sort::SortExpr> = op
        .inputs
        .iter()
        .map(|(_, s, _)| s.apply_maps(&sort_std, &op_std))
        .collect();
    let output = op.output.apply_maps(&sort_std, &op_std);
    (inputs, output)
}

/// A mapping from original names to their equivalence-class representatives.
type RenameMap = FxHashMap<Arc<str>, Arc<str>>;

/// Classify identifications, build union-finds, and verify compatibility.
/// Returns the sort and op rename maps.
fn build_rename_maps(
    theory: &Theory,
    identifications: &[(Arc<str>, Arc<str>)],
) -> Result<(RenameMap, RenameMap), GatError> {
    let mut sort_ids: Vec<(Arc<str>, Arc<str>)> = Vec::new();
    let mut op_ids: Vec<(Arc<str>, Arc<str>)> = Vec::new();

    for (a, b) in identifications {
        if theory.has_sort(a) && theory.has_sort(b) {
            sort_ids.push((a.clone(), b.clone()));
        } else if theory.has_op(a) && theory.has_op(b) {
            op_ids.push((a.clone(), b.clone()));
        } else {
            return Err(GatError::QuotientIncompatible {
                name_a: a.to_string(),
                name_b: b.to_string(),
                detail: "names are not both sorts or both operations in the theory".into(),
            });
        }
    }

    // Build sort union-find.
    let mut sort_uf = NameUnionFind::new();
    for s in &theory.sorts {
        sort_uf.insert(s.name.clone());
    }
    for (a, b) in &sort_ids {
        sort_uf.union(a, b);
    }

    // Verify that every member of a sort class agrees with the class
    // representative on arity, kind, and closure. Identifying two sorts
    // collapses them to a single representative, so any field that differs
    // between members would be silently discarded; reject it instead.
    for (rep, members) in &sort_uf.classes() {
        let rep_sort = get_sort(theory, rep)?;
        let rep_arity = rep_sort.arity();
        for member in members {
            if member == rep {
                continue;
            }
            let member_sort = get_sort(theory, member)?;
            let member_arity = member_sort.arity();
            if member_arity != rep_arity {
                return Err(GatError::QuotientIncompatible {
                    name_a: rep.to_string(),
                    name_b: member.to_string(),
                    detail: format!("sort arities differ ({rep_arity} vs {member_arity})"),
                });
            }
            if member_sort.kind != rep_sort.kind {
                return Err(GatError::QuotientIncompatible {
                    name_a: rep.to_string(),
                    name_b: member.to_string(),
                    detail: format!(
                        "sort kinds differ ({:?} vs {:?})",
                        rep_sort.kind, member_sort.kind
                    ),
                });
            }
            if member_sort.closure != rep_sort.closure {
                return Err(GatError::QuotientIncompatible {
                    name_a: rep.to_string(),
                    name_b: member.to_string(),
                    detail: format!(
                        "sort closures differ ({:?} vs {:?})",
                        rep_sort.closure, member_sort.closure
                    ),
                });
            }
        }
    }

    let sort_rename = sort_uf.rename_map();

    // Build op union-find.
    let mut op_uf = NameUnionFind::new();
    for op in &theory.ops {
        op_uf.insert(op.name.clone());
    }
    for (a, b) in &op_ids {
        op_uf.union(a, b);
    }

    let op_rename = op_uf.rename_map();

    // Verify op signature compatibility in the quotient, i.e. after both
    // the sort and the operation identifications are applied.
    for (rep, members) in &op_uf.classes() {
        let rep_sig = renamed_op_signature(get_op(theory, rep)?, &sort_rename, &op_rename);
        for member in members {
            if member == rep {
                continue;
            }
            let member_sig =
                renamed_op_signature(get_op(theory, member)?, &sort_rename, &op_rename);
            if rep_sig != member_sig {
                return Err(GatError::QuotientIncompatible {
                    name_a: rep.to_string(),
                    name_b: member.to_string(),
                    detail: "operation signatures differ in the quotient".into(),
                });
            }
        }
    }

    Ok((sort_rename, op_rename))
}

/// Rebuild theory components using the computed rename maps.
///
/// Constructs the quotiented theory with [`Theory::full`] so that the
/// directed equations and conflict policies survive quotienting. The
/// directed equations have their op references renamed through
/// `op_rename` (the `impl_term`, `inverse`, `source_kind`, `target_kind`,
/// and `coercion_class` fields ride through unchanged), and are
/// deduplicated by renamed lhs/rhs. Policies reference neither sorts nor
/// ops, so they are carried through unchanged. `extends` is left empty:
/// the quotiented theory declares no parents.
fn rebuild_theory(
    theory: &Theory,
    sort_rename: &RenameMap,
    op_rename: &RenameMap,
) -> Result<Theory, GatError> {
    let new_sorts = rebuild_sorts(theory, sort_rename, op_rename)?;
    let new_ops = rebuild_ops(theory, sort_rename, op_rename)?;
    let new_eqs = rebuild_eqs(&theory.eqs, op_rename);
    let new_directed_eqs = rebuild_directed_eqs(&theory.directed_eqs, op_rename);
    Ok(Theory::full(
        theory.name.clone(),
        Vec::new(),
        new_sorts,
        new_ops,
        new_eqs,
        new_directed_eqs,
        theory.policies.clone(),
    ))
}

/// One sort per equivalence class, with each dependent parameter's sort
/// rewritten into the quotient's namespace.
fn rebuild_sorts(
    theory: &Theory,
    sort_rename: &RenameMap,
    op_rename: &RenameMap,
) -> Result<Vec<Sort>, GatError> {
    let sort_std = as_std_map(sort_rename);
    let op_std = as_std_map(op_rename);
    let mut result = Vec::new();
    let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
    for sort in &theory.sorts {
        let rep = apply_sort_rename(&sort.name, sort_rename);
        if seen.insert(rep.clone()) {
            let rep_sort = get_sort(theory, &rep)?;
            let params: Vec<SortParam> = rep_sort
                .params
                .iter()
                .map(|p| SortParam {
                    name: p.name.clone(),
                    sort: p.sort.apply_maps(&sort_std, &op_std),
                })
                .collect();
            result.push(Sort {
                name: rep,
                params,
                kind: rep_sort.kind.clone(),
                closure: rep_sort.closure.clone(),
            });
        }
    }
    Ok(result)
}

/// One op per equivalence class, with its signature rewritten into the
/// quotient's namespace.
///
/// Sort heads follow the sort identifications and every operation applied
/// inside a dependent sort's argument terms follows the operation
/// identifications, so a surviving signature never names an operation the
/// quotient collapsed away.
fn rebuild_ops(
    theory: &Theory,
    sort_rename: &RenameMap,
    op_rename: &RenameMap,
) -> Result<Vec<Operation>, GatError> {
    let sort_std = as_std_map(sort_rename);
    let op_std = as_std_map(op_rename);
    let mut result = Vec::new();
    let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
    for op in &theory.ops {
        let rep = apply_op_rename(&op.name, op_rename);
        if seen.insert(rep.clone()) {
            let rep_op = get_op(theory, &rep)?;
            let inputs: Vec<(Arc<str>, crate::sort::SortExpr, crate::op::Implicit)> = rep_op
                .inputs
                .iter()
                .map(|(pname, psort, imp)| {
                    (pname.clone(), psort.apply_maps(&sort_std, &op_std), *imp)
                })
                .collect();
            result.push(Operation::with_implicit(
                rep,
                inputs,
                rep_op.output.apply_maps(&sort_std, &op_std),
            ));
        }
    }
    Ok(result)
}

/// Rename ops in equations and deduplicate modulo alpha-equivalence.
///
/// Two renamed equations that differ only in the names of their
/// universally-quantified variables denote the same axiom, so the second
/// is dropped. Undirected equations are symmetric (`lhs = rhs` and
/// `rhs = lhs` denote the same axiom), so both orientations are compared.
fn rebuild_eqs(eqs: &[Equation], op_rename: &RenameMap) -> Vec<Equation> {
    let op_rename_std: std::collections::HashMap<Arc<str>, Arc<str>> = op_rename
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut result: Vec<Equation> = Vec::new();
    for eq in eqs {
        let renamed = eq.rename_ops(&op_rename_std);
        let is_dup = result.iter().any(|kept| {
            alpha_equivalent_equation(&kept.lhs, &kept.rhs, &renamed.lhs, &renamed.rhs)
                || alpha_equivalent_equation(&kept.lhs, &kept.rhs, &renamed.rhs, &renamed.lhs)
        });
        if !is_dup {
            result.push(renamed);
        }
    }
    result
}

/// Rename ops in directed equations and deduplicate modulo
/// alpha-equivalence, preserving orientation.
///
/// Mirrors [`rebuild_eqs`], but compares only the `lhs`-to-`rhs`
/// orientation: directed equations are oriented (lhs rewrites to rhs), so
/// unlike undirected equations their two sides are not interchangeable.
/// The `impl_term`, `inverse`, `source_kind`, `target_kind`, and
/// `coercion_class` fields are carried through unchanged by
/// [`DirectedEquation::rename_ops`].
fn rebuild_directed_eqs(
    directed_eqs: &[DirectedEquation],
    op_rename: &RenameMap,
) -> Vec<DirectedEquation> {
    let op_rename_std: std::collections::HashMap<Arc<str>, Arc<str>> = op_rename
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut result: Vec<DirectedEquation> = Vec::new();
    for de in directed_eqs {
        let renamed = de.rename_ops(&op_rename_std);
        let is_dup = result.iter().any(|kept| {
            alpha_equivalent_equation(&kept.lhs, &kept.rhs, &renamed.lhs, &renamed.rhs)
        });
        if !is_dup {
            result.push(renamed);
        }
    }
    result
}

/// Quotient a theory by identifying sorts and/or operations.
///
/// Each pair `(a, b)` specifies that names `a` and `b` should be merged.
/// Transitive closure is computed automatically via union-find.
///
/// The quotiented theory preserves the input's directed equations and
/// conflict policies: directed equations have their op references renamed
/// through the op quotient (and are deduplicated by renamed lhs/rhs),
/// while policies, which reference neither sorts nor ops, ride through
/// unchanged.
///
/// # Errors
///
/// Returns [`GatError::QuotientIncompatible`] if identified sorts have
/// different arities or identified operations have incompatible signatures
/// (after applying sort renaming). Returns [`GatError::SortNotFound`] or
/// [`GatError::OpNotFound`] if a name referenced internally is missing.
pub fn quotient(
    theory: &Theory,
    identifications: &[(Arc<str>, Arc<str>)],
) -> Result<Theory, GatError> {
    if identifications.is_empty() {
        return Ok(theory.clone());
    }
    let (sort_rename, op_rename) = build_rename_maps(theory, identifications)?;
    rebuild_theory(theory, &sort_rename, &op_rename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::Term;

    /// Build a theory with two sorts and operations referencing them.
    fn two_sort_theory() -> Theory {
        let s_a = Sort::simple("A");
        let s_b = Sort::simple("B");
        let op_f = Operation::unary("f", "x", "A", "A");
        let op_g = Operation::unary("g", "x", "B", "B");
        let eq1 = Equation::new(
            "f_idem",
            Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
            Term::app("f", vec![Term::var("x")]),
        );
        Theory::full(
            "TwoSort",
            Vec::new(),
            vec![s_a, s_b],
            vec![op_f, op_g],
            vec![eq1],
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn empty_identifications_returns_isomorphic() -> Result<(), Box<dyn std::error::Error>> {
        let t = two_sort_theory();
        let q = quotient(&t, &[])?;
        assert_eq!(q.sorts.len(), t.sorts.len());
        assert_eq!(q.ops.len(), t.ops.len());
        assert_eq!(q.eqs.len(), t.eqs.len());
        assert_eq!(&*q.name, &*t.name);
        Ok(())
    }

    #[test]
    fn merge_two_sorts() -> Result<(), Box<dyn std::error::Error>> {
        let t = two_sort_theory();
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.sorts.len(), 1);
        assert!(q.find_sort("A").is_some());
        assert!(q.find_sort("B").is_none());
        assert_eq!(q.ops.len(), 2);
        let g = q.find_op("g").ok_or("op g not found")?;
        assert_eq!(&**g.output.head(), "A");
        assert_eq!(&**g.inputs[0].1.head(), "A");
        Ok(())
    }

    #[test]
    fn merge_two_ops() -> Result<(), Box<dyn std::error::Error>> {
        let s = Sort::simple("S");
        let op_f = Operation::unary("f", "x", "S", "S");
        let op_g = Operation::unary("g", "x", "S", "S");
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s],
            vec![op_f, op_g],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("f"), Arc::from("g"))];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.ops.len(), 1);
        assert!(q.find_op("f").is_some());
        assert!(q.find_op("g").is_none());
        Ok(())
    }

    #[test]
    fn transitive_closure() -> Result<(), Box<dyn std::error::Error>> {
        let s_a = Sort::simple("A");
        let s_b = Sort::simple("B");
        let s_c = Sort::simple("C");
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b, s_c],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![
            (Arc::from("A"), Arc::from("B")),
            (Arc::from("B"), Arc::from("C")),
        ];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.sorts.len(), 1);
        assert!(q.find_sort("A").is_some());
        Ok(())
    }

    #[test]
    fn incompatible_sort_arities_error() {
        let s_simple = Sort::simple("A");
        let s_dep = Sort::dependent("B", vec![SortParam::new("x", "A")]);
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_simple, s_dep],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        let result = quotient(&t, &ids);
        assert!(result.is_err());
        match result {
            Err(GatError::QuotientIncompatible { detail, .. }) => {
                assert!(detail.contains("arities differ"));
            }
            other => panic!("expected QuotientIncompatible, got {other:?}"),
        }
    }

    #[test]
    fn incompatible_op_signatures_error() {
        let s_a = Sort::simple("A");
        let s_b = Sort::simple("B");
        let op_f = Operation::unary("f", "x", "A", "A");
        let op_g = Operation::unary("g", "x", "A", "B");
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b],
            vec![op_f, op_g],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("f"), Arc::from("g"))];
        let result = quotient(&t, &ids);
        assert!(result.is_err());
        match result {
            Err(GatError::QuotientIncompatible { detail, .. }) => {
                assert!(detail.contains("signatures differ"));
            }
            other => panic!("expected QuotientIncompatible, got {other:?}"),
        }
    }

    #[test]
    fn equations_renamed_and_deduplicated() -> Result<(), Box<dyn std::error::Error>> {
        let s = Sort::simple("S");
        let op_f = Operation::unary("f", "x", "S", "S");
        let op_g = Operation::unary("g", "x", "S", "S");
        let eq1 = Equation::new("eq_f", Term::app("f", vec![Term::var("x")]), Term::var("x"));
        let eq2 = Equation::new("eq_g", Term::app("g", vec![Term::var("x")]), Term::var("x"));
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s],
            vec![op_f, op_g],
            vec![eq1, eq2],
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("f"), Arc::from("g"))];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.eqs.len(), 1);
        assert_eq!(&*q.eqs[0].name, "eq_f");
        Ok(())
    }

    #[test]
    fn mixed_sort_and_op_identifications() -> Result<(), Box<dyn std::error::Error>> {
        let s_a = Sort::simple("A");
        let s_b = Sort::simple("B");
        let op_f = Operation::unary("f", "x", "A", "A");
        let op_g = Operation::unary("g", "x", "B", "B");
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b],
            vec![op_f, op_g],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![
            (Arc::from("A"), Arc::from("B")),
            (Arc::from("f"), Arc::from("g")),
        ];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.sorts.len(), 1);
        assert_eq!(q.ops.len(), 1);
        assert!(q.find_sort("A").is_some());
        assert!(q.find_op("f").is_some());
        Ok(())
    }

    #[test]
    fn sort_params_renamed_in_dependent_sorts() -> Result<(), Box<dyn std::error::Error>> {
        let s_a = Sort::simple("A");
        let s_b = Sort::simple("B");
        let s_dep = Sort::dependent("D", vec![SortParam::new("x", "B")]);
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b, s_dep],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.sorts.len(), 2);
        let d = q.find_sort("D").ok_or("sort D not found")?;
        assert_eq!(&**d.params[0].sort.head(), "A");
        Ok(())
    }

    #[test]
    fn directed_eqs_and_policies_survive_quotient() -> Result<(), Box<dyn std::error::Error>> {
        use crate::eq::DirectedEquation;
        use crate::sort::ValueKind;
        use crate::theory::{ConflictPolicy, ConflictStrategy};

        // A theory with two ops f, g, one directed equation referencing g,
        // and one conflict policy. Identifying f and g must not strip the
        // directed equation or the policy.
        let s = Sort::simple("S");
        let op_f = Operation::unary("f", "x", "S", "S");
        let op_g = Operation::unary("g", "x", "S", "S");
        let de = DirectedEquation::new(
            "g_to_x",
            Term::app("g", vec![Term::var("x")]),
            Term::var("x"),
            panproto_expr::Expr::Var("_".into()),
        );
        let policy = ConflictPolicy {
            name: "keep_left_str".into(),
            value_kind: ValueKind::Str,
            strategy: ConflictStrategy::KeepLeft,
        };
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s],
            vec![op_f, op_g],
            Vec::new(),
            vec![de],
            vec![policy],
        );

        // f is alphabetically first, so g is renamed to f.
        let ids = vec![(Arc::from("f"), Arc::from("g"))];
        let q = quotient(&t, &ids)?;

        // The directed equation survives, with its op reference renamed.
        assert_eq!(q.directed_eqs.len(), 1);
        let survived = q
            .find_directed_eq("g_to_x")
            .ok_or("directed equation g_to_x not found")?;
        assert_eq!(survived.lhs, Term::app("f", vec![Term::var("x")]));
        assert_eq!(survived.rhs, Term::var("x"));

        // The policy survives unchanged.
        assert_eq!(q.policies.len(), 1);
        assert!(q.find_policy("keep_left_str").is_some());
        Ok(())
    }

    #[test]
    fn identifying_sorts_with_differing_kind_fails() {
        use crate::sort::{SortKind, ValueKind};

        // A is a value sort, B is structural; both are nullary, so arity
        // agrees but kind does not.
        let s_a = Sort::with_kind("A", SortKind::Val(ValueKind::Str));
        let s_b = Sort::simple("B");
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        match quotient(&t, &ids) {
            Err(GatError::QuotientIncompatible { detail, .. }) => {
                assert!(detail.contains("kinds differ"), "got detail: {detail}");
            }
            other => panic!("expected QuotientIncompatible on kind mismatch, got {other:?}"),
        }
    }

    #[test]
    fn identifying_sorts_with_differing_closure_fails() {
        use crate::sort::SortClosure;

        // A is open, B is closed; both are nullary structural sorts, so
        // only their closure differs.
        let s_a = Sort::simple("A");
        let s_b = Sort {
            name: Arc::from("B"),
            params: Vec::new(),
            kind: crate::sort::SortKind::default(),
            closure: SortClosure::Closed(vec![Arc::from("mk_b")]),
        };
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        match quotient(&t, &ids) {
            Err(GatError::QuotientIncompatible { detail, .. }) => {
                assert!(detail.contains("closures differ"), "got detail: {detail}");
            }
            other => panic!("expected QuotientIncompatible on closure mismatch, got {other:?}"),
        }
    }

    #[test]
    fn identifying_sorts_with_matching_kind_and_closure_succeeds()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::sort::{SortKind, ValueKind};

        // Both sorts share kind and closure, so the identification is
        // accepted and collapses them to one representative.
        let s_a = Sort::with_kind("A", SortKind::Val(ValueKind::Int));
        let s_b = Sort::with_kind("B", SortKind::Val(ValueKind::Int));
        let t = Theory::full(
            "T",
            Vec::new(),
            vec![s_a, s_b],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.sorts.len(), 1);
        assert!(q.find_sort("A").is_some());
        Ok(())
    }

    #[test]
    fn alpha_variant_equations_deduplicated() -> Result<(), Box<dyn std::error::Error>> {
        // Two equations that differ only in their bound variable name are
        // alpha-variants of one axiom. A non-empty identification forces
        // the rebuild path (an empty one short-circuits), after which the
        // renamed equations must collapse to exactly one.
        let sort_s = Sort::simple("S");
        let sort_a = Sort::simple("A");
        let sort_b = Sort::simple("B");
        let op_f = Operation::unary("f", "z", "S", "S");
        let eq_x = Equation::new("e1", Term::app("f", vec![Term::var("x")]), Term::var("x"));
        let eq_y = Equation::new("e2", Term::app("f", vec![Term::var("y")]), Term::var("y"));
        let theory = Theory::full(
            "T",
            Vec::new(),
            vec![sort_s, sort_a, sort_b],
            vec![op_f],
            vec![eq_x, eq_y],
            Vec::new(),
            Vec::new(),
        );
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        let quotiented = quotient(&theory, &ids)?;
        assert_eq!(quotiented.eqs.len(), 1);
        Ok(())
    }

    // --- operation renaming inside dependent-sort argument terms ---

    /// A theory with two points and a loop at each, where the loop sorts
    /// are dependent on the point terms.
    fn two_point_loop_theory() -> Theory {
        Theory::new(
            "Pointed",
            vec![
                Sort::simple("Pt"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Pt"), SortParam::new("b", "Pt")],
                ),
            ],
            vec![
                Operation::new("pt1", vec![], "Pt"),
                Operation::new("pt2", vec![], "Pt"),
                Operation::new(
                    "loop1",
                    vec![],
                    crate::sort::SortExpr::app(
                        "Hom",
                        vec![Term::constant("pt1"), Term::constant("pt1")],
                    ),
                ),
                Operation::new(
                    "loop2",
                    vec![],
                    crate::sort::SortExpr::app(
                        "Hom",
                        vec![Term::constant("pt2"), Term::constant("pt2")],
                    ),
                ),
            ],
            vec![],
        )
    }

    /// Every operation name applied anywhere inside a term.
    fn collect_referenced_ops(term: &Term, names: &mut Vec<Arc<str>>) {
        match term {
            Term::Var(_) | Term::Hole { .. } => {}
            Term::App { op, args } => {
                names.push(Arc::clone(op));
                for arg in args {
                    collect_referenced_ops(arg, names);
                }
            }
            Term::Case {
                scrutinee,
                branches,
            } => {
                collect_referenced_ops(scrutinee, names);
                for branch in branches {
                    names.push(Arc::clone(&branch.constructor));
                    collect_referenced_ops(&branch.body, names);
                }
            }
            Term::Let { bound, body, .. } => {
                collect_referenced_ops(bound, names);
                collect_referenced_ops(body, names);
            }
        }
    }

    fn assert_no_dangling_op_references(theory: &Theory) {
        for op in &theory.ops {
            let sorts = op
                .inputs
                .iter()
                .map(|(_, s, _)| s)
                .chain(std::iter::once(&op.output));
            for sort in sorts {
                let mut names = Vec::new();
                for arg in sort.args() {
                    collect_referenced_ops(arg, &mut names);
                }
                for name in names {
                    assert!(
                        theory.has_op(&name),
                        "signature of `{}` mentions `{name}`, absent from the quotient: {sort:?}",
                        op.name,
                    );
                }
            }
        }
    }

    #[test]
    fn quotient_renames_ops_inside_dependent_sort_arguments() {
        let theory = two_point_loop_theory();
        let Ok(quotiented) = quotient(&theory, &[(Arc::from("pt1"), Arc::from("pt2"))]) else {
            panic!("identifying two points of the same sort must be compatible");
        };
        assert_no_dangling_op_references(&quotiented);
    }

    #[test]
    fn quotient_identifies_ops_compatible_after_op_renaming() {
        // `loop1 : Hom(pt1(), pt1())` and `loop2 : Hom(pt2(), pt2())` have
        // the same signature once pt1 and pt2 are identified, so the two
        // may be identified with them.
        let theory = two_point_loop_theory();
        let Ok(quotiented) = quotient(
            &theory,
            &[
                (Arc::from("pt1"), Arc::from("pt2")),
                (Arc::from("loop1"), Arc::from("loop2")),
            ],
        ) else {
            panic!("loop signatures must agree once the points are identified");
        };
        assert_no_dangling_op_references(&quotiented);
        assert_eq!(
            quotiented.ops.len(),
            2,
            "one point and one loop survive: {:?}",
            quotiented.ops,
        );
    }
}
