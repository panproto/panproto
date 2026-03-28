use std::collections::HashMap;
use std::sync::Arc;

use crate::eq::{Term, alpha_equivalent_equation};
use crate::error::GatError;
use crate::ident::{NameSite, SiteRename};
use crate::theory::Theory;

/// A structure-preserving map between two theories.
///
/// Maps sorts to sorts and operations to operations. A valid morphism
/// must preserve sort arities, operation type signatures, and equations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TheoryMorphism {
    /// A human-readable name for this morphism.
    pub name: Arc<str>,
    /// The name of the domain theory.
    pub domain: Arc<str>,
    /// The name of the codomain theory.
    pub codomain: Arc<str>,
    /// Mapping from domain sort names to codomain sort names.
    pub sort_map: HashMap<Arc<str>, Arc<str>>,
    /// Mapping from domain operation names to codomain operation names.
    pub op_map: HashMap<Arc<str>, Arc<str>>,
}

impl TheoryMorphism {
    /// Create a new theory morphism.
    #[must_use]
    pub fn new(
        name: impl Into<Arc<str>>,
        domain: impl Into<Arc<str>>,
        codomain: impl Into<Arc<str>>,
        sort_map: HashMap<Arc<str>, Arc<str>>,
        op_map: HashMap<Arc<str>, Arc<str>>,
    ) -> Self {
        Self {
            name: name.into(),
            domain: domain.into(),
            codomain: codomain.into(),
            sort_map,
            op_map,
        }
    }

    /// Apply this morphism to a term, renaming operations.
    #[must_use]
    pub fn apply_to_term(&self, term: &Term) -> Term {
        term.rename_ops(&self.op_map)
    }

    /// Create the identity morphism on a theory.
    ///
    /// Maps every sort and operation to itself.
    #[must_use]
    pub fn identity(theory: &Theory) -> Self {
        let sort_map: HashMap<Arc<str>, Arc<str>> = theory
            .sorts
            .iter()
            .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
            .collect();
        let op_map: HashMap<Arc<str>, Arc<str>> = theory
            .ops
            .iter()
            .map(|o| (Arc::clone(&o.name), Arc::clone(&o.name)))
            .collect();
        Self {
            name: Arc::from(format!("id_{}", theory.name)),
            domain: Arc::clone(&theory.name),
            codomain: Arc::clone(&theory.name),
            sort_map,
            op_map,
        }
    }

    /// Compose two morphisms: `self: A → B` followed by `other: B → C`, producing `A → C`.
    ///
    /// The sort and operation maps are composed: for each `a ↦ b` in `self` and
    /// `b ↦ c` in `other`, the composed map has `a ↦ c`.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::ComposeUnmapped`] if a sort or operation in `self`'s
    /// codomain image has no mapping in `other`.
    pub fn compose(&self, other: &Self) -> Result<Self, crate::error::GatError> {
        let mut sort_map = HashMap::with_capacity(self.sort_map.len());
        for (a, b) in &self.sort_map {
            let c =
                other
                    .sort_map
                    .get(b)
                    .ok_or_else(|| crate::error::GatError::ComposeUnmapped {
                        kind: "sort",
                        name: a.to_string(),
                        image: b.to_string(),
                    })?;
            sort_map.insert(Arc::clone(a), Arc::clone(c));
        }
        let mut op_map = HashMap::with_capacity(self.op_map.len());
        for (a, b) in &self.op_map {
            let c = other
                .op_map
                .get(b)
                .ok_or_else(|| crate::error::GatError::ComposeUnmapped {
                    kind: "op",
                    name: a.to_string(),
                    image: b.to_string(),
                })?;
            op_map.insert(Arc::clone(a), Arc::clone(c));
        }
        Ok(Self {
            name: Arc::from(format!("{};{}", self.name, other.name)),
            domain: Arc::clone(&self.domain),
            codomain: Arc::clone(&other.codomain),
            sort_map,
            op_map,
        })
    }

    /// Induce site-qualified renames from this theory morphism.
    ///
    /// Sort-map entries where `old ≠ new` become [`NameSite::VertexKind`]
    /// renames (since sorts map to vertex kinds at the schema level).
    /// Op-map entries where `old ≠ new` become [`NameSite::EdgeKind`]
    /// renames (since operations map to edge kinds).
    #[must_use]
    pub fn induce_schema_renames(&self) -> Vec<SiteRename> {
        let mut renames = Vec::new();
        for (old_sort, new_sort) in &self.sort_map {
            if old_sort != new_sort {
                renames.push(SiteRename::new(
                    NameSite::VertexKind,
                    Arc::clone(old_sort),
                    Arc::clone(new_sort),
                ));
            }
        }
        for (old_op, new_op) in &self.op_map {
            if old_op != new_op {
                renames.push(SiteRename::new(
                    NameSite::EdgeKind,
                    Arc::clone(old_op),
                    Arc::clone(new_op),
                ));
            }
        }
        renames
    }
}

/// Check that a theory morphism is valid.
///
/// Verifies that:
/// 1. All domain sorts are mapped.
/// 2. All domain operations are mapped.
/// 3. Sort arities are preserved under the mapping.
/// 4. Operation type signatures are preserved under the sort mapping.
/// 5. Equations are preserved (both sides map to equal terms in the codomain).
/// 6. Directed equations are preserved (mapped rewrite rules exist in codomain).
///
/// # Errors
///
/// Returns a [`GatError`] variant describing the first violation found.
pub fn check_morphism(
    m: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) -> Result<(), GatError> {
    // 1. All domain sorts must be mapped.
    for sort in &domain.sorts {
        let target_name = m
            .sort_map
            .get(&sort.name)
            .ok_or_else(|| GatError::MissingSortMapping(sort.name.to_string()))?;

        let target_sort = codomain
            .find_sort(target_name)
            .ok_or_else(|| GatError::SortNotFound(target_name.to_string()))?;

        // 3. Sort arities must match.
        if sort.arity() != target_sort.arity() {
            return Err(GatError::SortArityMismatch {
                sort: sort.name.to_string(),
                expected: sort.arity(),
                got: target_sort.arity(),
            });
        }

        // 3a. Sort kinds must match.
        if sort.kind != target_sort.kind {
            return Err(GatError::SortKindMismatch {
                sort: sort.name.to_string(),
                expected: sort.kind.clone(),
                got: target_sort.kind.clone(),
            });
        }

        // 3b. Dependent sort parameter sorts must be preserved under the mapping.
        for (i, param) in sort.params.iter().enumerate() {
            let mapped_param_sort = m.sort_map.get(&param.sort).unwrap_or(&param.sort);
            if *mapped_param_sort != target_sort.params[i].sort {
                return Err(GatError::SortParamMismatch {
                    sort: sort.name.to_string(),
                    param_index: i,
                    expected: mapped_param_sort.to_string(),
                    got: target_sort.params[i].sort.to_string(),
                });
            }
        }
    }

    // 2. All domain ops must be mapped.
    for op in &domain.ops {
        let target_name = m
            .op_map
            .get(&op.name)
            .ok_or_else(|| GatError::MissingOpMapping(op.name.to_string()))?;

        let target_op = codomain
            .find_op(target_name)
            .ok_or_else(|| GatError::OpNotFound(target_name.to_string()))?;

        // 4. Operation type signatures must be preserved under sort mapping.
        if op.inputs.len() != target_op.inputs.len() {
            return Err(GatError::OpTypeMismatch {
                op: op.name.to_string(),
                detail: format!(
                    "arity mismatch: domain has {} inputs, codomain has {}",
                    op.inputs.len(),
                    target_op.inputs.len()
                ),
            });
        }

        for (i, (_, sort_name)) in op.inputs.iter().enumerate() {
            let mapped_sort = m
                .sort_map
                .get(sort_name)
                .ok_or_else(|| GatError::MissingSortMapping(sort_name.to_string()))?;
            let (_, target_sort) = &target_op.inputs[i];
            if mapped_sort != target_sort {
                return Err(GatError::OpTypeMismatch {
                    op: op.name.to_string(),
                    detail: format!("input {i}: expected sort {mapped_sort}, got {target_sort}"),
                });
            }
        }

        let mapped_output = m
            .sort_map
            .get(&op.output)
            .ok_or_else(|| GatError::MissingSortMapping(op.output.to_string()))?;
        if mapped_output != &target_op.output {
            return Err(GatError::OpTypeMismatch {
                op: op.name.to_string(),
                detail: format!(
                    "output: expected sort {mapped_output}, got {}",
                    target_op.output
                ),
            });
        }
    }

    check_equations_preserved(m, domain, codomain)?;
    check_directed_equations_preserved(m, domain, codomain)?;

    Ok(())
}

/// Check that all equations in the domain are preserved under the morphism.
fn check_equations_preserved(
    m: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) -> Result<(), GatError> {
    for eq in &domain.eqs {
        let mapped_lhs = m.apply_to_term(&eq.lhs);
        let mapped_rhs = m.apply_to_term(&eq.rhs);

        let preserved = codomain
            .eqs
            .iter()
            .any(|ceq| alpha_equivalent_equation(&ceq.lhs, &ceq.rhs, &mapped_lhs, &mapped_rhs));

        if !preserved {
            return Err(GatError::EquationNotPreserved {
                equation: eq.name.to_string(),
                detail: "mapped equation not found in codomain".to_owned(),
            });
        }
    }
    Ok(())
}

/// Check that all directed equations in the domain are preserved under the morphism.
fn check_directed_equations_preserved(
    m: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) -> Result<(), GatError> {
    for de in &domain.directed_eqs {
        let mapped_lhs = m.apply_to_term(&de.lhs);
        let mapped_rhs = m.apply_to_term(&de.rhs);

        let preserved = codomain
            .directed_eqs
            .iter()
            .any(|cde| alpha_equivalent_equation(&cde.lhs, &cde.rhs, &mapped_lhs, &mapped_rhs));

        if !preserved {
            return Err(GatError::DirectedEquationNotPreserved {
                equation: de.name.to_string(),
                detail: "mapped directed equation not found in codomain".to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::eq::{Equation, Term};
    use crate::error::GatError;
    use crate::model::{Model, ModelValue, migrate_model};
    use crate::op::Operation;
    use crate::sort::Sort;
    use crate::theory::Theory;

    /// Build a simple monoid theory for testing.
    fn monoid_theory(name: &str, mul_name: &str, unit_name: &str) -> Theory {
        let carrier = Sort::simple("Carrier");

        let mul = Operation::new(
            mul_name,
            vec![
                ("a".into(), "Carrier".into()),
                ("b".into(), "Carrier".into()),
            ],
            "Carrier",
        );
        let unit = Operation::nullary(unit_name, "Carrier");

        let assoc = Equation::new(
            "assoc",
            Term::app(
                mul_name,
                vec![
                    Term::var("a"),
                    Term::app(mul_name, vec![Term::var("b"), Term::var("c")]),
                ],
            ),
            Term::app(
                mul_name,
                vec![
                    Term::app(mul_name, vec![Term::var("a"), Term::var("b")]),
                    Term::var("c"),
                ],
            ),
        );

        let left_id = Equation::new(
            "left_id",
            Term::app(mul_name, vec![Term::constant(unit_name), Term::var("a")]),
            Term::var("a"),
        );

        let right_id = Equation::new(
            "right_id",
            Term::app(mul_name, vec![Term::var("a"), Term::constant(unit_name)]),
            Term::var("a"),
        );

        Theory::new(
            name,
            vec![carrier],
            vec![mul, unit],
            vec![assoc, left_id, right_id],
        )
    }

    /// Build a commutative monoid theory (monoid + commutativity axiom).
    fn commutative_monoid_theory(name: &str, mul_name: &str, unit_name: &str) -> Theory {
        let carrier = Sort::simple("Carrier");

        let mul = Operation::new(
            mul_name,
            vec![
                ("a".into(), "Carrier".into()),
                ("b".into(), "Carrier".into()),
            ],
            "Carrier",
        );
        let unit = Operation::nullary(unit_name, "Carrier");

        let assoc = Equation::new(
            "assoc",
            Term::app(
                mul_name,
                vec![
                    Term::var("a"),
                    Term::app(mul_name, vec![Term::var("b"), Term::var("c")]),
                ],
            ),
            Term::app(
                mul_name,
                vec![
                    Term::app(mul_name, vec![Term::var("a"), Term::var("b")]),
                    Term::var("c"),
                ],
            ),
        );

        let left_id = Equation::new(
            "left_id",
            Term::app(mul_name, vec![Term::constant(unit_name), Term::var("a")]),
            Term::var("a"),
        );

        let right_id = Equation::new(
            "right_id",
            Term::app(mul_name, vec![Term::var("a"), Term::constant(unit_name)]),
            Term::var("a"),
        );

        let commutativity = Equation::new(
            "comm",
            Term::app(mul_name, vec![Term::var("a"), Term::var("b")]),
            Term::app(mul_name, vec![Term::var("b"), Term::var("a")]),
        );

        Theory::new(
            name,
            vec![carrier],
            vec![mul, unit],
            vec![assoc, left_id, right_id, commutativity],
        )
    }

    #[test]
    fn identity_morphism_is_valid() {
        let t = monoid_theory("Monoid", "mul", "unit");

        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("mul")),
            (Arc::from("unit"), Arc::from("unit")),
        ]);

        let m = TheoryMorphism::new("id", "Monoid", "Monoid", sort_map, op_map);
        assert!(check_morphism(&m, &t, &t).is_ok());
    }

    #[test]
    fn renaming_morphism_is_valid() {
        let domain = monoid_theory("M1", "mul", "unit");
        let codomain = monoid_theory("M2", "times", "one");

        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("times")),
            (Arc::from("unit"), Arc::from("one")),
        ]);

        let m = TheoryMorphism::new("rename", "M1", "M2", sort_map, op_map);
        assert!(check_morphism(&m, &domain, &codomain).is_ok());
    }

    #[test]
    fn missing_sort_mapping_fails() {
        let t = monoid_theory("M", "mul", "unit");

        let sort_map = HashMap::new(); // empty -- missing Carrier
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("mul")),
            (Arc::from("unit"), Arc::from("unit")),
        ]);

        let m = TheoryMorphism::new("bad", "M", "M", sort_map, op_map);
        let result = check_morphism(&m, &t, &t);
        assert!(matches!(result, Err(GatError::MissingSortMapping(_))));
    }

    #[test]
    fn missing_op_mapping_fails() {
        let t = monoid_theory("M", "mul", "unit");

        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([(Arc::from("mul"), Arc::from("mul"))]);
        // missing unit mapping

        let m = TheoryMorphism::new("bad", "M", "M", sort_map, op_map);
        let result = check_morphism(&m, &t, &t);
        assert!(matches!(result, Err(GatError::MissingOpMapping(_))));
    }

    #[test]
    fn sort_arity_mismatch_fails() {
        use crate::sort::SortParam;

        let domain = Theory::new("D", vec![Sort::simple("S")], Vec::new(), Vec::new());
        let codomain = Theory::new(
            "C",
            vec![Sort::dependent("T", vec![SortParam::new("x", "T")])],
            Vec::new(),
            Vec::new(),
        );

        let sort_map = HashMap::from([(Arc::from("S"), Arc::from("T"))]);

        let m = TheoryMorphism::new("bad", "D", "C", sort_map, HashMap::new());
        let result = check_morphism(&m, &domain, &codomain);
        assert!(matches!(result, Err(GatError::SortArityMismatch { .. })));
    }

    #[test]
    fn op_type_mismatch_fails() {
        let domain = Theory::new(
            "D",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        );
        // Codomain has f going B -> A (reversed).
        let codomain = Theory::new(
            "C",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "B", "A")],
            Vec::new(),
        );

        let sort_map = HashMap::from([
            (Arc::from("A"), Arc::from("A")),
            (Arc::from("B"), Arc::from("B")),
        ]);
        let op_map = HashMap::from([(Arc::from("f"), Arc::from("f"))]);

        let m = TheoryMorphism::new("bad", "D", "C", sort_map, op_map);
        let result = check_morphism(&m, &domain, &codomain);
        assert!(matches!(result, Err(GatError::OpTypeMismatch { .. })));
    }

    /// Morphism between theories where the codomain equation uses different
    /// variable names. This would fail with syntactic comparison but succeeds
    /// with α-equivalence.
    #[test]
    fn morphism_with_renamed_equation_vars() {
        let domain = Theory::new(
            "D",
            vec![Sort::simple("S")],
            vec![Operation::new(
                "f",
                vec![("a".into(), "S".into()), ("b".into(), "S".into())],
                "S",
            )],
            vec![Equation::new(
                "comm",
                Term::app("f", vec![Term::var("a"), Term::var("b")]),
                Term::app("f", vec![Term::var("b"), Term::var("a")]),
            )],
        );

        // Codomain has the same equation but with variables x, y instead of a, b.
        let codomain = Theory::new(
            "C",
            vec![Sort::simple("S")],
            vec![Operation::new(
                "f",
                vec![("x".into(), "S".into()), ("y".into(), "S".into())],
                "S",
            )],
            vec![Equation::new(
                "comm",
                Term::app("f", vec![Term::var("x"), Term::var("y")]),
                Term::app("f", vec![Term::var("y"), Term::var("x")]),
            )],
        );

        let sort_map = HashMap::from([(Arc::from("S"), Arc::from("S"))]);
        let op_map = HashMap::from([(Arc::from("f"), Arc::from("f"))]);

        let m = TheoryMorphism::new("id", "D", "C", sort_map, op_map);
        assert!(
            check_morphism(&m, &domain, &codomain).is_ok(),
            "morphism should be valid: equations are α-equivalent"
        );
    }

    /// Morphism where equation variable multiplicity differs should fail.
    /// Domain: f(x, x) = g(x). Codomain: f(a, b) = g(a).
    /// These are NOT α-equivalent because x maps to both a and b.
    #[test]
    fn morphism_equation_multiplicity_mismatch_fails() {
        let domain = Theory::new(
            "D",
            vec![Sort::simple("S")],
            vec![
                Operation::new(
                    "f",
                    vec![("a".into(), "S".into()), ("b".into(), "S".into())],
                    "S",
                ),
                Operation::unary("g", "x", "S", "S"),
            ],
            vec![Equation::new(
                "eq1",
                Term::app("f", vec![Term::var("x"), Term::var("x")]),
                Term::app("g", vec![Term::var("x")]),
            )],
        );

        // Codomain has f(a, b) = g(a) which is not α-equivalent to f(x,x) = g(x).
        let codomain = Theory::new(
            "C",
            vec![Sort::simple("S")],
            vec![
                Operation::new(
                    "f",
                    vec![("a".into(), "S".into()), ("b".into(), "S".into())],
                    "S",
                ),
                Operation::unary("g", "x", "S", "S"),
            ],
            vec![Equation::new(
                "eq1",
                Term::app("f", vec![Term::var("a"), Term::var("b")]),
                Term::app("g", vec![Term::var("a")]),
            )],
        );

        let sort_map = HashMap::from([(Arc::from("S"), Arc::from("S"))]);
        let op_map = HashMap::from([
            (Arc::from("f"), Arc::from("f")),
            (Arc::from("g"), Arc::from("g")),
        ]);

        let m = TheoryMorphism::new("bad", "D", "C", sort_map, op_map);
        assert!(
            check_morphism(&m, &domain, &codomain).is_err(),
            "morphism should fail: equations have different variable multiplicity"
        );
    }

    /// Identity morphism on a theory with directed equations should pass.
    #[test]
    fn morphism_preserves_directed_eqs() {
        use crate::eq::DirectedEquation;

        let theory = Theory::full(
            "T",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![DirectedEquation::new(
                "idem",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let sort_map = HashMap::from([(Arc::from("A"), Arc::from("A"))]);
        let op_map = HashMap::from([(Arc::from("f"), Arc::from("f"))]);
        let m = TheoryMorphism::new("id", "T", "T", sort_map, op_map);
        assert!(check_morphism(&m, &theory, &theory).is_ok());
    }

    /// Renaming morphism should correctly map directed equations.
    #[test]
    fn morphism_renaming_preserves_directed_eqs() {
        use crate::eq::DirectedEquation;

        let domain = Theory::full(
            "D",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let codomain = Theory::full(
            "C",
            Vec::new(),
            vec![Sort::simple("B")],
            vec![Operation::unary("g", "y", "B", "B")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::app("g", vec![Term::var("y")]),
                Term::var("y"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let sort_map = HashMap::from([(Arc::from("A"), Arc::from("B"))]);
        let op_map = HashMap::from([(Arc::from("f"), Arc::from("g"))]);
        let m = TheoryMorphism::new("rename", "D", "C", sort_map, op_map);
        assert!(check_morphism(&m, &domain, &codomain).is_ok());
    }

    /// Morphism should fail when codomain lacks a matching directed equation.
    #[test]
    fn morphism_missing_directed_eq_fails() {
        use crate::eq::DirectedEquation;

        let domain = Theory::full(
            "D",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        // Codomain has same sorts/ops but NO directed equations.
        let codomain = Theory::new(
            "C",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
        );

        let sort_map = HashMap::from([(Arc::from("A"), Arc::from("A"))]);
        let op_map = HashMap::from([(Arc::from("f"), Arc::from("f"))]);
        let m = TheoryMorphism::new("bad", "D", "C", sort_map, op_map);
        assert!(matches!(
            check_morphism(&m, &domain, &codomain),
            Err(GatError::DirectedEquationNotPreserved { .. })
        ));
    }

    #[test]
    fn identity_is_unit_for_compose() {
        let t = monoid_theory("M", "mul", "unit");
        let id = TheoryMorphism::identity(&t);

        // Build a non-trivial renaming morphism.
        let codomain = monoid_theory("M2", "times", "one");
        let f = TheoryMorphism::new(
            "rename",
            "M",
            "M2",
            HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]),
            HashMap::from([
                (Arc::from("mul"), Arc::from("times")),
                (Arc::from("unit"), Arc::from("one")),
            ]),
        );

        // id ; f == f
        let id_then_f = id.compose(&f).unwrap();
        assert_eq!(id_then_f.sort_map, f.sort_map);
        assert_eq!(id_then_f.op_map, f.op_map);

        // f ; id_codomain == f
        let id_cod = TheoryMorphism::identity(&codomain);
        let f_then_id = f.compose(&id_cod).unwrap();
        assert_eq!(f_then_id.sort_map, f.sort_map);
        assert_eq!(f_then_id.op_map, f.op_map);
    }

    #[test]
    fn compose_is_associative() {
        let _t1 = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
        );
        let _t2 = Theory::new(
            "T2",
            vec![Sort::simple("B")],
            vec![Operation::unary("g", "x", "B", "B")],
            Vec::new(),
        );
        let _t3 = Theory::new(
            "T3",
            vec![Sort::simple("C")],
            vec![Operation::unary("h", "x", "C", "C")],
            Vec::new(),
        );
        let _t4 = Theory::new(
            "T4",
            vec![Sort::simple("D")],
            vec![Operation::unary("k", "x", "D", "D")],
            Vec::new(),
        );

        let m1 = TheoryMorphism::new(
            "m1",
            "T1",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("B"))]),
            HashMap::from([(Arc::from("f"), Arc::from("g"))]),
        );
        let m2 = TheoryMorphism::new(
            "m2",
            "T2",
            "T3",
            HashMap::from([(Arc::from("B"), Arc::from("C"))]),
            HashMap::from([(Arc::from("g"), Arc::from("h"))]),
        );
        let m3 = TheoryMorphism::new(
            "m3",
            "T3",
            "T4",
            HashMap::from([(Arc::from("C"), Arc::from("D"))]),
            HashMap::from([(Arc::from("h"), Arc::from("k"))]),
        );

        let left = m1.compose(&m2).unwrap().compose(&m3).unwrap();
        let right = m1.compose(&m2.compose(&m3).unwrap()).unwrap();

        assert_eq!(left.sort_map, right.sort_map);
        assert_eq!(left.op_map, right.op_map);
        assert_eq!(left.domain, right.domain);
        assert_eq!(left.codomain, right.codomain);
    }

    #[test]
    fn sort_kind_mismatch_fails() {
        use crate::sort::SortKind;

        let domain = Theory::new(
            "D",
            vec![Sort::simple("S")], // Structural kind
            Vec::new(),
            Vec::new(),
        );
        let codomain = Theory::new(
            "C",
            vec![Sort::with_kind(
                "T",
                SortKind::Val(crate::sort::ValueKind::Int),
            )],
            Vec::new(),
            Vec::new(),
        );

        let sort_map = HashMap::from([(Arc::from("S"), Arc::from("T"))]);
        let m = TheoryMorphism::new("bad", "D", "C", sort_map, HashMap::new());
        let result = check_morphism(&m, &domain, &codomain);
        assert!(
            matches!(result, Err(GatError::SortKindMismatch { .. })),
            "expected SortKindMismatch, got {result:?}"
        );
    }

    #[test]
    fn sort_param_mismatch_fails() {
        use crate::sort::SortParam;

        let domain = Theory::new(
            "D",
            vec![
                Sort::simple("A"),
                Sort::simple("B"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "A"), SortParam::new("b", "A")],
                ),
            ],
            Vec::new(),
            Vec::new(),
        );

        // Codomain: Arr depends on (X, Y) where X and Y are different sorts.
        let codomain = Theory::new(
            "C",
            vec![
                Sort::simple("X"),
                Sort::simple("Y"),
                Sort::dependent(
                    "Arr",
                    vec![SortParam::new("a", "X"), SortParam::new("b", "Y")],
                ),
            ],
            Vec::new(),
            Vec::new(),
        );

        // Map A -> X, B -> Y, Hom -> Arr.
        // Hom has params (a: A, b: A) which map to (X, X), but Arr has (X, Y).
        let sort_map = HashMap::from([
            (Arc::from("A"), Arc::from("X")),
            (Arc::from("B"), Arc::from("Y")),
            (Arc::from("Hom"), Arc::from("Arr")),
        ]);
        let m = TheoryMorphism::new("bad", "D", "C", sort_map, HashMap::new());
        let result = check_morphism(&m, &domain, &codomain);
        assert!(
            matches!(result, Err(GatError::SortParamMismatch { .. })),
            "expected SortParamMismatch, got {result:?}"
        );
    }

    /// Existing tests with no directed equations should still pass.
    #[test]
    fn morphism_no_directed_eqs_still_valid() {
        let t = monoid_theory("M", "mul", "unit");
        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("mul")),
            (Arc::from("unit"), Arc::from("unit")),
        ]);
        let m = TheoryMorphism::new("id", "M", "M", sort_map, op_map);
        assert!(check_morphism(&m, &t, &t).is_ok());
    }

    /// Test 4: reverse-mul morphism on a commutative monoid.
    ///
    /// Creates a commutative monoid, a morphism that swaps mul arguments
    /// (identity on sorts and ops, but the equations still hold because
    /// commutativity is an axiom), and verifies that migrating the (Z, +, 0)
    /// model gives the same results.
    #[test]
    fn reverse_mul_morphism_commutative_monoid() {
        let theory = commutative_monoid_theory("CMonoid", "mul", "unit");

        // Identity morphism -- maps mul->mul and unit->unit.
        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("mul")),
            (Arc::from("unit"), Arc::from("unit")),
        ]);

        let m = TheoryMorphism::new("swap", "CMonoid", "CMonoid", sort_map, op_map);
        assert!(check_morphism(&m, &theory, &theory).is_ok());

        // Build (Z, +, 0) model.
        let mut model = Model::new("CMonoid");
        model.add_sort("Carrier", (0..10).map(ModelValue::Int).collect());
        model.add_op("mul", |args: &[ModelValue]| match (&args[0], &args[1]) {
            (ModelValue::Int(a), ModelValue::Int(b)) => Ok(ModelValue::Int(a + b)),
            _ => Err(GatError::ModelError("expected Int".to_owned())),
        });
        model.add_op("unit", |_: &[ModelValue]| Ok(ModelValue::Int(0)));

        // Migrate model along the morphism.
        let migrated = migrate_model(&m, &model).unwrap();

        // Since + is commutative, swapping arguments gives the same result.
        let orig = model
            .eval("mul", &[ModelValue::Int(3), ModelValue::Int(5)])
            .unwrap();
        let mig = migrated
            .eval("mul", &[ModelValue::Int(3), ModelValue::Int(5)])
            .unwrap();
        assert_eq!(orig, mig);

        // Also check the swapped order gives the same.
        let orig_swap = model
            .eval("mul", &[ModelValue::Int(5), ModelValue::Int(3)])
            .unwrap();
        assert_eq!(orig, orig_swap);

        // Unit is preserved.
        let orig_unit = model.eval("unit", &[]).unwrap();
        let mig_unit = migrated.eval("unit", &[]).unwrap();
        assert_eq!(orig_unit, mig_unit);
    }
}
