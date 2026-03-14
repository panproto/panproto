use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::eq::Equation;
use crate::error::GatError;
use crate::op::Operation;
use crate::sort::Sort;

/// A generalized algebraic theory (GAT).
///
/// Theories are named collections of sorts, operations, and equations,
/// with optional inheritance via `extends`. When a theory extends another,
/// it inherits all of the parent's sorts, operations, and equations.
///
/// # Index cache
///
/// Precomputed `FxHashMap` indices provide O(1) lookup by name for sorts,
/// operations, and equations. These are rebuilt from the vectors at
/// construction and deserialization time.
///
/// # Examples
///
/// A monoid theory declares one sort (`Carrier`), two operations (`mul`, `unit`),
/// and three equations (associativity, left identity, right identity).
#[derive(Debug, Clone)]
pub struct Theory {
    /// The theory name (e.g., "Monoid", "Category").
    pub name: Arc<str>,
    /// Names of parent theories this theory extends.
    pub extends: Vec<Arc<str>>,
    /// Sort declarations.
    pub sorts: Vec<Sort>,
    /// Operation declarations.
    pub ops: Vec<Operation>,
    /// Equations (axioms).
    pub eqs: Vec<Equation>,
    /// O(1) sort lookup by name.
    sort_idx: FxHashMap<Arc<str>, usize>,
    /// O(1) operation lookup by name.
    op_idx: FxHashMap<Arc<str>, usize>,
    /// O(1) equation lookup by name.
    eq_idx: FxHashMap<Arc<str>, usize>,
}

impl PartialEq for Theory {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.extends == other.extends
            && self.sorts == other.sorts
            && self.ops == other.ops
            && self.eqs == other.eqs
    }
}

impl Eq for Theory {}

impl serde::Serialize for Theory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Theory", 5)?;
        s.serialize_field("name", &self.name)?;
        s.serialize_field("extends", &self.extends)?;
        s.serialize_field("sorts", &self.sorts)?;
        s.serialize_field("ops", &self.ops)?;
        s.serialize_field("eqs", &self.eqs)?;
        s.end()
    }
}

impl<'de> serde::Deserialize<'de> for Theory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Raw {
            name: Arc<str>,
            extends: Vec<Arc<str>>,
            sorts: Vec<Sort>,
            ops: Vec<Operation>,
            eqs: Vec<Equation>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self::extending(
            raw.name,
            raw.extends,
            raw.sorts,
            raw.ops,
            raw.eqs,
        ))
    }
}

/// Build index maps from vectors.
fn build_sort_idx(sorts: &[Sort]) -> FxHashMap<Arc<str>, usize> {
    sorts
        .iter()
        .enumerate()
        .map(|(i, s)| (Arc::clone(&s.name), i))
        .collect()
}

fn build_op_idx(ops: &[Operation]) -> FxHashMap<Arc<str>, usize> {
    ops.iter()
        .enumerate()
        .map(|(i, o)| (Arc::clone(&o.name), i))
        .collect()
}

fn build_eq_idx(eqs: &[Equation]) -> FxHashMap<Arc<str>, usize> {
    eqs.iter()
        .enumerate()
        .map(|(i, e)| (Arc::clone(&e.name), i))
        .collect()
}

impl Theory {
    /// Create a new theory with no parents.
    #[must_use]
    pub fn new(
        name: impl Into<Arc<str>>,
        sorts: Vec<Sort>,
        ops: Vec<Operation>,
        eqs: Vec<Equation>,
    ) -> Self {
        let sort_idx = build_sort_idx(&sorts);
        let op_idx = build_op_idx(&ops);
        let eq_idx = build_eq_idx(&eqs);
        Self {
            name: name.into(),
            extends: Vec::new(),
            sorts,
            ops,
            eqs,
            sort_idx,
            op_idx,
            eq_idx,
        }
    }

    /// Create a theory that extends one or more parent theories.
    #[must_use]
    pub fn extending(
        name: impl Into<Arc<str>>,
        extends: Vec<Arc<str>>,
        sorts: Vec<Sort>,
        ops: Vec<Operation>,
        eqs: Vec<Equation>,
    ) -> Self {
        let sort_idx = build_sort_idx(&sorts);
        let op_idx = build_op_idx(&ops);
        let eq_idx = build_eq_idx(&eqs);
        Self {
            name: name.into(),
            extends,
            sorts,
            ops,
            eqs,
            sort_idx,
            op_idx,
            eq_idx,
        }
    }

    /// Look up a sort by name. O(1) via index cache.
    #[inline]
    #[must_use]
    pub fn find_sort(&self, name: &str) -> Option<&Sort> {
        self.sort_idx.get(name).map(|&i| &self.sorts[i])
    }

    /// Look up an operation by name. O(1) via index cache.
    #[inline]
    #[must_use]
    pub fn find_op(&self, name: &str) -> Option<&Operation> {
        self.op_idx.get(name).map(|&i| &self.ops[i])
    }

    /// Look up an equation by name. O(1) via index cache.
    #[inline]
    #[must_use]
    pub fn find_eq(&self, name: &str) -> Option<&Equation> {
        self.eq_idx.get(name).map(|&i| &self.eqs[i])
    }

    /// Check if a sort with the given name exists. O(1).
    #[inline]
    #[must_use]
    pub fn has_sort(&self, name: &str) -> bool {
        self.sort_idx.contains_key(name)
    }

    /// Check if an operation with the given name exists. O(1).
    #[inline]
    #[must_use]
    pub fn has_op(&self, name: &str) -> bool {
        self.op_idx.contains_key(name)
    }
}

/// Resolve a theory by computing the transitive closure of its `extends` chain.
///
/// Merges all ancestor sorts, operations, and equations into a single resolved
/// theory. Detects cyclic dependencies in the inheritance chain.
///
/// # Errors
///
/// Returns [`GatError::TheoryNotFound`] if a referenced parent theory is missing
/// from the registry, or [`GatError::CyclicDependency`] if the extends chain
/// contains a cycle.
pub fn resolve_theory<S: std::hash::BuildHasher>(
    name: &str,
    registry: &std::collections::HashMap<String, Theory, S>,
) -> Result<Theory, GatError> {
    let mut visited = FxHashSet::default();
    let mut in_stack = FxHashSet::default();
    resolve_recursive(name, registry, &mut visited, &mut in_stack)
}

fn resolve_recursive<S: std::hash::BuildHasher>(
    name: &str,
    registry: &std::collections::HashMap<String, Theory, S>,
    visited: &mut FxHashSet<String>,
    in_stack: &mut FxHashSet<String>,
) -> Result<Theory, GatError> {
    if in_stack.contains(name) {
        return Err(GatError::CyclicDependency(name.to_owned()));
    }

    let theory = registry
        .get(name)
        .ok_or_else(|| GatError::TheoryNotFound(name.to_owned()))?;

    if visited.contains(name) {
        return Ok(theory.clone());
    }

    in_stack.insert(name.to_owned());

    let mut sort_names: FxHashSet<Arc<str>> = FxHashSet::default();
    let mut op_names: FxHashSet<Arc<str>> = FxHashSet::default();
    let mut eq_names: FxHashSet<Arc<str>> = FxHashSet::default();

    let mut merged_sorts = Vec::new();
    let mut merged_ops = Vec::new();
    let mut merged_eqs = Vec::new();

    // Resolve all parents first.
    for parent_name in &theory.extends {
        let resolved_parent = resolve_recursive(parent_name, registry, visited, in_stack)?;
        for sort in resolved_parent.sorts {
            if sort_names.insert(Arc::clone(&sort.name)) {
                merged_sorts.push(sort);
            }
        }
        for op in resolved_parent.ops {
            if op_names.insert(Arc::clone(&op.name)) {
                merged_ops.push(op);
            }
        }
        for eq in resolved_parent.eqs {
            if eq_names.insert(Arc::clone(&eq.name)) {
                merged_eqs.push(eq);
            }
        }
    }

    // Add this theory's own declarations.
    for sort in &theory.sorts {
        if sort_names.insert(Arc::clone(&sort.name)) {
            merged_sorts.push(sort.clone());
        }
    }
    for op in &theory.ops {
        if op_names.insert(Arc::clone(&op.name)) {
            merged_ops.push(op.clone());
        }
    }
    for eq in &theory.eqs {
        if eq_names.insert(Arc::clone(&eq.name)) {
            merged_eqs.push(eq.clone());
        }
    }

    in_stack.remove(name);
    visited.insert(name.to_owned());

    Ok(Theory::new(
        Arc::from(name),
        merged_sorts,
        merged_ops,
        merged_eqs,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::eq::Term;
    use std::collections::HashMap;

    /// Helper: build the classic monoid theory.
    fn monoid_theory() -> Theory {
        let carrier = Sort::simple("Carrier");

        let mul = Operation::new(
            "mul",
            vec![
                ("a".into(), "Carrier".into()),
                ("b".into(), "Carrier".into()),
            ],
            "Carrier",
        );
        let unit = Operation::nullary("unit", "Carrier");

        // assoc: mul(a, mul(b, c)) = mul(mul(a, b), c)
        let assoc = Equation::new(
            "assoc",
            Term::app(
                "mul",
                vec![
                    Term::var("a"),
                    Term::app("mul", vec![Term::var("b"), Term::var("c")]),
                ],
            ),
            Term::app(
                "mul",
                vec![
                    Term::app("mul", vec![Term::var("a"), Term::var("b")]),
                    Term::var("c"),
                ],
            ),
        );

        // left_id: mul(unit(), a) = a
        let left_id = Equation::new(
            "left_id",
            Term::app("mul", vec![Term::constant("unit"), Term::var("a")]),
            Term::var("a"),
        );

        // right_id: mul(a, unit()) = a
        let right_id = Equation::new(
            "right_id",
            Term::app("mul", vec![Term::var("a"), Term::constant("unit")]),
            Term::var("a"),
        );

        Theory::new(
            "Monoid",
            vec![carrier],
            vec![mul, unit],
            vec![assoc, left_id, right_id],
        )
    }

    #[test]
    fn create_monoid_theory() {
        let t = monoid_theory();
        assert_eq!(&*t.name, "Monoid");
        assert_eq!(t.sorts.len(), 1);
        assert_eq!(t.ops.len(), 2);
        assert_eq!(t.eqs.len(), 3);
        assert!(t.find_sort("Carrier").is_some());
        assert!(t.find_op("mul").is_some());
        assert!(t.find_op("unit").is_some());
        assert!(t.find_eq("assoc").is_some());
    }

    #[test]
    fn resolve_theory_simple() {
        let mut registry = HashMap::new();
        let t = monoid_theory();
        registry.insert("Monoid".to_owned(), t);

        let resolved = resolve_theory("Monoid", &registry).unwrap();
        assert_eq!(resolved.sorts.len(), 1);
        assert_eq!(resolved.ops.len(), 2);
        assert_eq!(resolved.eqs.len(), 3);
    }

    #[test]
    fn resolve_theory_with_inheritance() {
        let mut registry = HashMap::new();

        // Base: a pointed set (sort + constant).
        let base = Theory::new(
            "PointedSet",
            vec![Sort::simple("Carrier")],
            vec![Operation::nullary("unit", "Carrier")],
            Vec::new(),
        );
        registry.insert("PointedSet".to_owned(), base);

        // Child: extends PointedSet and adds mul + equations.
        let child = Theory::extending(
            "Monoid",
            vec![Arc::from("PointedSet")],
            Vec::new(),
            vec![Operation::new(
                "mul",
                vec![
                    ("a".into(), "Carrier".into()),
                    ("b".into(), "Carrier".into()),
                ],
                "Carrier",
            )],
            vec![Equation::new(
                "assoc",
                Term::app(
                    "mul",
                    vec![
                        Term::var("a"),
                        Term::app("mul", vec![Term::var("b"), Term::var("c")]),
                    ],
                ),
                Term::app(
                    "mul",
                    vec![
                        Term::app("mul", vec![Term::var("a"), Term::var("b")]),
                        Term::var("c"),
                    ],
                ),
            )],
        );
        registry.insert("Monoid".to_owned(), child);

        let resolved = resolve_theory("Monoid", &registry).unwrap();
        // Should have inherited Carrier and unit from PointedSet.
        assert_eq!(resolved.sorts.len(), 1);
        assert_eq!(resolved.ops.len(), 2);
        assert_eq!(resolved.eqs.len(), 1);
        assert!(resolved.find_sort("Carrier").is_some());
        assert!(resolved.find_op("unit").is_some());
        assert!(resolved.find_op("mul").is_some());
    }

    #[test]
    fn resolve_theory_transitive_inheritance() {
        let mut registry = HashMap::new();

        let a = Theory::new("A", vec![Sort::simple("S")], Vec::new(), Vec::new());
        registry.insert("A".to_owned(), a);

        let b = Theory::extending(
            "B",
            vec![Arc::from("A")],
            vec![Sort::simple("T")],
            Vec::new(),
            Vec::new(),
        );
        registry.insert("B".to_owned(), b);

        let c = Theory::extending(
            "C",
            vec![Arc::from("B")],
            vec![Sort::simple("U")],
            Vec::new(),
            Vec::new(),
        );
        registry.insert("C".to_owned(), c);

        let resolved = resolve_theory("C", &registry).unwrap();
        assert_eq!(resolved.sorts.len(), 3);
        assert!(resolved.find_sort("S").is_some());
        assert!(resolved.find_sort("T").is_some());
        assert!(resolved.find_sort("U").is_some());
    }

    #[test]
    fn resolve_theory_cycle_detection() {
        let mut registry = HashMap::new();

        let a = Theory::extending(
            "A",
            vec![Arc::from("B")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let b = Theory::extending(
            "B",
            vec![Arc::from("A")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        registry.insert("A".to_owned(), a);
        registry.insert("B".to_owned(), b);

        let result = resolve_theory("A", &registry);
        assert!(result.is_err());
        assert!(
            matches!(result, Err(GatError::CyclicDependency(_))),
            "expected CyclicDependency error"
        );
    }

    #[test]
    fn resolve_theory_not_found() {
        let registry = HashMap::new();
        let result = resolve_theory("Nonexistent", &registry);
        assert!(matches!(result, Err(GatError::TheoryNotFound(_))));
    }

    #[test]
    fn resolve_theory_diamond_inheritance() {
        let mut registry = HashMap::new();

        let base = Theory::new("Base", vec![Sort::simple("S")], Vec::new(), Vec::new());
        registry.insert("Base".to_owned(), base);

        let left = Theory::extending(
            "Left",
            vec![Arc::from("Base")],
            vec![Sort::simple("L")],
            Vec::new(),
            Vec::new(),
        );
        registry.insert("Left".to_owned(), left);

        let right = Theory::extending(
            "Right",
            vec![Arc::from("Base")],
            vec![Sort::simple("R")],
            Vec::new(),
            Vec::new(),
        );
        registry.insert("Right".to_owned(), right);

        let diamond = Theory::extending(
            "Diamond",
            vec![Arc::from("Left"), Arc::from("Right")],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        registry.insert("Diamond".to_owned(), diamond);

        let resolved = resolve_theory("Diamond", &registry).unwrap();
        // S should not be duplicated despite appearing via both Left and Right.
        assert_eq!(resolved.sorts.len(), 3);
    }

    /// Test 3: `ThGAT` -- the theory of GATs describes itself.
    ///
    /// `ThGAT` has sorts: `Sort`, `Op`, `Eq`, `Theory`.
    /// It has operations like `sort_name`, `op_name`, `op_output`, etc.
    /// This test verifies that `ThGAT` is a well-formed GAT.
    #[test]
    fn theory_of_gats_is_valid() {
        use crate::sort::SortParam;

        // Sorts of ThGAT.
        let sort_sort = Sort::simple("Sort");
        let op_sort = Sort::simple("Op");
        let eq_sort = Sort::simple("Eq");
        let theory_sort = Sort::simple("Theory");
        // A dependent sort: Param(s: Sort) -- the parameters of a sort.
        let param_sort = Sort::dependent("Param", vec![SortParam::new("s", "Sort")]);

        // Operations of ThGAT.
        // sort_name : Sort -> Name (we model Name as a simple sort)
        let name_sort = Sort::simple("Name");
        let sort_name_op = Operation::unary("sort_name", "s", "Sort", "Name");
        // op_name : Op -> Name
        let op_name_op = Operation::unary("op_name", "o", "Op", "Name");
        // op_output : Op -> Sort
        let op_output_op = Operation::unary("op_output", "o", "Op", "Sort");
        // eq_name : Eq -> Name
        let eq_name_op = Operation::unary("eq_name", "e", "Eq", "Name");
        // theory_name : Theory -> Name
        let theory_name_op = Operation::unary("theory_name", "t", "Theory", "Name");

        let th_gat = Theory::new(
            "ThGAT",
            vec![
                sort_sort,
                op_sort,
                eq_sort,
                theory_sort,
                param_sort,
                name_sort,
            ],
            vec![
                sort_name_op,
                op_name_op,
                op_output_op,
                eq_name_op,
                theory_name_op,
            ],
            Vec::new(), // No equations needed for this structural test.
        );

        // Verify it is a well-formed GAT: has the expected structure.
        assert_eq!(&*th_gat.name, "ThGAT");
        assert_eq!(th_gat.sorts.len(), 6);
        assert_eq!(th_gat.ops.len(), 5);

        // All sorts are findable (O(1) via index).
        assert!(th_gat.find_sort("Sort").is_some());
        assert!(th_gat.find_sort("Op").is_some());
        assert!(th_gat.find_sort("Eq").is_some());
        assert!(th_gat.find_sort("Theory").is_some());
        assert!(th_gat.find_sort("Param").is_some());
        assert!(th_gat.find_sort("Name").is_some());

        // The dependent sort Param has arity 1.
        let param = th_gat.find_sort("Param").unwrap();
        assert_eq!(param.arity(), 1);
        assert_eq!(&*param.params[0].sort, "Sort");

        // All ops are findable and have correct signatures.
        let sn = th_gat.find_op("sort_name").unwrap();
        assert_eq!(sn.inputs.len(), 1);
        assert_eq!(&*sn.inputs[0].1, "Sort");
        assert_eq!(&*sn.output, "Name");

        let on = th_gat.find_op("op_name").unwrap();
        assert_eq!(&*on.output, "Name");

        let oo = th_gat.find_op("op_output").unwrap();
        assert_eq!(&*oo.output, "Sort");

        // ThGAT can resolve itself in a registry.
        let mut registry = HashMap::new();
        registry.insert("ThGAT".to_owned(), th_gat);
        let resolved = resolve_theory("ThGAT", &registry).unwrap();
        assert_eq!(resolved.sorts.len(), 6);
        assert_eq!(resolved.ops.len(), 5);
    }
}
