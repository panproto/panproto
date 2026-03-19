use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::eq::Equation;
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

/// Compute the renamed signature of an operation (input sort list + output sort).
fn renamed_op_signature(op: &Operation, sort_rename: &RenameMap) -> (Vec<Arc<str>>, Arc<str>) {
    let inputs: Vec<Arc<str>> = op
        .inputs
        .iter()
        .map(|(_, s)| apply_sort_rename(s, sort_rename))
        .collect();
    let output = apply_sort_rename(&op.output, sort_rename);
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

    // Verify sort arity compatibility.
    for (rep, members) in &sort_uf.classes() {
        let rep_arity = get_sort(theory, rep)?.arity();
        for member in members {
            if member == rep {
                continue;
            }
            let member_arity = get_sort(theory, member)?.arity();
            if member_arity != rep_arity {
                return Err(GatError::QuotientIncompatible {
                    name_a: rep.to_string(),
                    name_b: member.to_string(),
                    detail: format!("sort arities differ ({rep_arity} vs {member_arity})"),
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

    // Verify op signature compatibility (after sort renaming).
    for (rep, members) in &op_uf.classes() {
        let rep_sig = renamed_op_signature(get_op(theory, rep)?, &sort_rename);
        for member in members {
            if member == rep {
                continue;
            }
            let member_sig = renamed_op_signature(get_op(theory, member)?, &sort_rename);
            if rep_sig != member_sig {
                return Err(GatError::QuotientIncompatible {
                    name_a: rep.to_string(),
                    name_b: member.to_string(),
                    detail: "operation signatures differ after sort renaming".into(),
                });
            }
        }
    }

    let op_rename = op_uf.rename_map();
    Ok((sort_rename, op_rename))
}

/// Rebuild theory components using the computed rename maps.
fn rebuild_theory(
    theory: &Theory,
    sort_rename: &RenameMap,
    op_rename: &RenameMap,
) -> Result<Theory, GatError> {
    let new_sorts = rebuild_sorts(theory, sort_rename)?;
    let new_ops = rebuild_ops(theory, sort_rename, op_rename)?;
    let new_eqs = rebuild_eqs(&theory.eqs, op_rename);
    Ok(Theory::new(
        theory.name.clone(),
        new_sorts,
        new_ops,
        new_eqs,
    ))
}

/// One sort per equivalence class with sort params renamed.
fn rebuild_sorts(theory: &Theory, sort_rename: &RenameMap) -> Result<Vec<Sort>, GatError> {
    let mut result = Vec::new();
    let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
    for sort in &theory.sorts {
        let rep = apply_sort_rename(&sort.name, sort_rename);
        if seen.insert(rep.clone()) {
            let rep_sort = get_sort(theory, &rep)?;
            let params: Vec<SortParam> = rep_sort
                .params
                .iter()
                .map(|p| SortParam::new(p.name.clone(), apply_sort_rename(&p.sort, sort_rename)))
                .collect();
            result.push(Sort {
                name: rep,
                params,
                kind: rep_sort.kind.clone(),
            });
        }
    }
    Ok(result)
}

/// One op per equivalence class with sort references renamed.
fn rebuild_ops(
    theory: &Theory,
    sort_rename: &RenameMap,
    op_rename: &RenameMap,
) -> Result<Vec<Operation>, GatError> {
    let mut result = Vec::new();
    let mut seen: FxHashSet<Arc<str>> = FxHashSet::default();
    for op in &theory.ops {
        let rep = apply_op_rename(&op.name, op_rename);
        if seen.insert(rep.clone()) {
            let rep_op = get_op(theory, &rep)?;
            let inputs: Vec<(Arc<str>, Arc<str>)> = rep_op
                .inputs
                .iter()
                .map(|(pname, psort)| (pname.clone(), apply_sort_rename(psort, sort_rename)))
                .collect();
            result.push(Operation::new(
                rep,
                inputs,
                apply_sort_rename(&rep_op.output, sort_rename),
            ));
        }
    }
    Ok(result)
}

/// Rename ops in equations and deduplicate.
fn rebuild_eqs(eqs: &[Equation], op_rename: &RenameMap) -> Vec<Equation> {
    let op_rename_std: std::collections::HashMap<Arc<str>, Arc<str>> = op_rename
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut result = Vec::new();
    let mut seen: FxHashSet<(Arc<str>, Arc<str>)> = FxHashSet::default();
    for eq in eqs {
        let renamed = eq.rename_ops(&op_rename_std);
        let lhs_str: Arc<str> = Arc::from(format!("{:?}", renamed.lhs));
        let rhs_str: Arc<str> = Arc::from(format!("{:?}", renamed.rhs));
        // Normalize order for dedup (lhs=rhs and rhs=lhs are the same equation).
        let key = if lhs_str <= rhs_str {
            (lhs_str, rhs_str)
        } else {
            (rhs_str, lhs_str)
        };
        if seen.insert(key) {
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
        Theory::new("TwoSort", vec![s_a, s_b], vec![op_f, op_g], vec![eq1])
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
        assert_eq!(&*g.output, "A");
        assert_eq!(&*g.inputs[0].1, "A");
        Ok(())
    }

    #[test]
    fn merge_two_ops() -> Result<(), Box<dyn std::error::Error>> {
        let s = Sort::simple("S");
        let op_f = Operation::unary("f", "x", "S", "S");
        let op_g = Operation::unary("g", "x", "S", "S");
        let t = Theory::new("T", vec![s], vec![op_f, op_g], vec![]);
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
        let t = Theory::new("T", vec![s_a, s_b, s_c], vec![], vec![]);
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
        let t = Theory::new("T", vec![s_simple, s_dep], vec![], vec![]);
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
        let t = Theory::new("T", vec![s_a, s_b], vec![op_f, op_g], vec![]);
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
        let t = Theory::new("T", vec![s], vec![op_f, op_g], vec![eq1, eq2]);
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
        let t = Theory::new("T", vec![s_a, s_b], vec![op_f, op_g], vec![]);
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
        let t = Theory::new("T", vec![s_a, s_b, s_dep], vec![], vec![]);
        let ids = vec![(Arc::from("A"), Arc::from("B"))];
        let q = quotient(&t, &ids)?;
        assert_eq!(q.sorts.len(), 2);
        let d = q.find_sort("D").ok_or("sort D not found")?;
        assert_eq!(&*d.params[0].sort, "A");
        Ok(())
    }
}
