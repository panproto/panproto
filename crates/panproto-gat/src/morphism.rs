use std::collections::HashMap;

use crate::eq::Term;
use crate::error::GatError;
use crate::theory::Theory;

/// A structure-preserving map between two theories.
///
/// Maps sorts to sorts and operations to operations. A valid morphism
/// must preserve sort arities, operation type signatures, and equations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TheoryMorphism {
    /// A human-readable name for this morphism.
    pub name: String,
    /// The name of the domain theory.
    pub domain: String,
    /// The name of the codomain theory.
    pub codomain: String,
    /// Mapping from domain sort names to codomain sort names.
    pub sort_map: HashMap<String, String>,
    /// Mapping from domain operation names to codomain operation names.
    pub op_map: HashMap<String, String>,
}

impl TheoryMorphism {
    /// Create a new theory morphism.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        domain: impl Into<String>,
        codomain: impl Into<String>,
        sort_map: HashMap<String, String>,
        op_map: HashMap<String, String>,
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
}

/// Check that a theory morphism is valid.
///
/// Verifies that:
/// 1. All domain sorts are mapped.
/// 2. All domain operations are mapped.
/// 3. Sort arities are preserved under the mapping.
/// 4. Operation type signatures are preserved under the sort mapping.
/// 5. Equations are preserved (both sides map to equal terms in the codomain).
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
            .ok_or_else(|| GatError::MissingSortMapping(sort.name.clone()))?;

        let target_sort = codomain
            .find_sort(target_name)
            .ok_or_else(|| GatError::SortNotFound(target_name.clone()))?;

        // 3. Sort arities must match.
        if sort.arity() != target_sort.arity() {
            return Err(GatError::SortArityMismatch {
                sort: sort.name.clone(),
                expected: sort.arity(),
                got: target_sort.arity(),
            });
        }
    }

    // 2. All domain ops must be mapped.
    for op in &domain.ops {
        let target_name = m
            .op_map
            .get(&op.name)
            .ok_or_else(|| GatError::MissingOpMapping(op.name.clone()))?;

        let target_op = codomain
            .find_op(target_name)
            .ok_or_else(|| GatError::OpNotFound(target_name.clone()))?;

        // 4. Operation type signatures must be preserved under sort mapping.
        if op.inputs.len() != target_op.inputs.len() {
            return Err(GatError::OpTypeMismatch {
                op: op.name.clone(),
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
                .ok_or_else(|| GatError::MissingSortMapping(sort_name.clone()))?;
            let (_, target_sort) = &target_op.inputs[i];
            if mapped_sort != target_sort {
                return Err(GatError::OpTypeMismatch {
                    op: op.name.clone(),
                    detail: format!("input {i}: expected sort {mapped_sort}, got {target_sort}"),
                });
            }
        }

        let mapped_output = m
            .sort_map
            .get(&op.output)
            .ok_or_else(|| GatError::MissingSortMapping(op.output.clone()))?;
        if mapped_output != &target_op.output {
            return Err(GatError::OpTypeMismatch {
                op: op.name.clone(),
                detail: format!(
                    "output: expected sort {mapped_output}, got {}",
                    target_op.output
                ),
            });
        }
    }

    // 5. Equations must be preserved.
    for eq in &domain.eqs {
        let mapped_lhs = m.apply_to_term(&eq.lhs);
        let mapped_rhs = m.apply_to_term(&eq.rhs);

        // Check that the codomain has an equation matching the mapped terms.
        let preserved = codomain
            .eqs
            .iter()
            .any(|ceq| ceq.lhs == mapped_lhs && ceq.rhs == mapped_rhs);

        if !preserved {
            return Err(GatError::EquationNotPreserved {
                equation: eq.name.clone(),
                detail: "mapped equation not found in codomain".to_owned(),
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

        let sort_map = HashMap::from([("Carrier".to_owned(), "Carrier".to_owned())]);
        let op_map = HashMap::from([
            ("mul".to_owned(), "mul".to_owned()),
            ("unit".to_owned(), "unit".to_owned()),
        ]);

        let m = TheoryMorphism::new("id", "Monoid", "Monoid", sort_map, op_map);
        assert!(check_morphism(&m, &t, &t).is_ok());
    }

    #[test]
    fn renaming_morphism_is_valid() {
        let domain = monoid_theory("M1", "mul", "unit");
        let codomain = monoid_theory("M2", "times", "one");

        let sort_map = HashMap::from([("Carrier".to_owned(), "Carrier".to_owned())]);
        let op_map = HashMap::from([
            ("mul".to_owned(), "times".to_owned()),
            ("unit".to_owned(), "one".to_owned()),
        ]);

        let m = TheoryMorphism::new("rename", "M1", "M2", sort_map, op_map);
        assert!(check_morphism(&m, &domain, &codomain).is_ok());
    }

    #[test]
    fn missing_sort_mapping_fails() {
        let t = monoid_theory("M", "mul", "unit");

        let sort_map = HashMap::new(); // empty -- missing Carrier
        let op_map = HashMap::from([
            ("mul".to_owned(), "mul".to_owned()),
            ("unit".to_owned(), "unit".to_owned()),
        ]);

        let m = TheoryMorphism::new("bad", "M", "M", sort_map, op_map);
        let result = check_morphism(&m, &t, &t);
        assert!(matches!(result, Err(GatError::MissingSortMapping(_))));
    }

    #[test]
    fn missing_op_mapping_fails() {
        let t = monoid_theory("M", "mul", "unit");

        let sort_map = HashMap::from([("Carrier".to_owned(), "Carrier".to_owned())]);
        let op_map = HashMap::from([("mul".to_owned(), "mul".to_owned())]);
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

        let sort_map = HashMap::from([("S".to_owned(), "T".to_owned())]);

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
            ("A".to_owned(), "A".to_owned()),
            ("B".to_owned(), "B".to_owned()),
        ]);
        let op_map = HashMap::from([("f".to_owned(), "f".to_owned())]);

        let m = TheoryMorphism::new("bad", "D", "C", sort_map, op_map);
        let result = check_morphism(&m, &domain, &codomain);
        assert!(matches!(result, Err(GatError::OpTypeMismatch { .. })));
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
        // In a commutative monoid, mul(a,b) = mul(b,a) is an axiom,
        // so mapping mul to itself is valid even though conceptually
        // we think of "swapping arguments".
        let sort_map = HashMap::from([("Carrier".to_owned(), "Carrier".to_owned())]);
        let op_map = HashMap::from([
            ("mul".to_owned(), "mul".to_owned()),
            ("unit".to_owned(), "unit".to_owned()),
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
