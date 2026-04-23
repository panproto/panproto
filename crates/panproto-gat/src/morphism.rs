use std::collections::HashMap;
use std::sync::Arc;

use crate::eq::{Term, alpha_equivalent_equation};
use crate::error::GatError;
use crate::ident::{NameSite, SiteRename};
use crate::sort::positional_param_rename;
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
    ///
    /// **Limitation**: This only renames operation names in the term via
    /// `op_map`. It does not rename sort annotations because the current
    /// [`Term`] representation is untyped (`Var` / `App`). For theories
    /// with dependent sorts, sort-level information is carried implicitly
    /// through operation signatures, which `check_morphism` validates
    /// separately. If `Term` is later extended with sort annotations,
    /// this method must also apply `sort_map`.
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

/// # Soundness note on equation preservation
///
/// Equation preservation is checked syntactically. For every domain
/// equation `lhs = rhs`, the mapped pair `F(lhs) = F(rhs)` must appear
/// alpha-equivalent to some equation already present in the codomain's
/// equation list. This is a conservative approximation of the true
/// mathematical criterion, which is that `F(lhs) = F(rhs)` hold in the
/// codomain's full equational theory. The current check rejects any
/// morphism whose image is derivable in the codomain via directed
/// rewrites or via chains of other equations but is not literally
/// listed, and this is a known incompleteness: complete preservation
/// via normalization or congruence closure against the codomain's
/// equational theory is a queued follow-up.
///
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

        // 3b. Dependent sort parameter sorts must be preserved under the
        // mapping, modulo positional alpha-renaming of the parameter
        // names. The parameter names are local binders at the sort's
        // declaration site; they are in scope in later parameter sorts.
        let sort_param_rename = positional_param_rename(
            sort.params.iter().map(|p| Arc::clone(&p.name)),
            target_sort.params.iter().map(|p| Arc::clone(&p.name)),
        );
        for (i, param) in sort.params.iter().enumerate() {
            let mapped_param_sort = param
                .sort
                .apply_maps(&m.sort_map, &m.op_map)
                .subst(&sort_param_rename);
            if !mapped_param_sort.alpha_eq(&target_sort.params[i].sort) {
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

        // Parameter names are local binders at the operation's
        // declaration site, in scope in every later input sort and in
        // the output sort. When comparing the mapped signature against
        // the codomain operation we rename the domain's parameter names
        // to the codomain's positionally, so that a morphism from
        // `f : (a : A) -> Hom(a, a)` to `f : (x : A) -> Hom(x, x)` is
        // accepted.
        let op_param_rename = positional_param_rename(
            op.inputs.iter().map(|(n, _, _)| Arc::clone(n)),
            target_op.inputs.iter().map(|(n, _, _)| Arc::clone(n)),
        );

        for (i, (_, sort_expr, _)) in op.inputs.iter().enumerate() {
            // The head of every input sort must have a mapping (this is
            // a structural prerequisite); argument-term renames flow
            // through the op_map.
            if !m.sort_map.contains_key(sort_expr.head()) {
                return Err(GatError::MissingSortMapping(sort_expr.head().to_string()));
            }
            let mapped_sort = sort_expr
                .apply_maps(&m.sort_map, &m.op_map)
                .subst(&op_param_rename);
            let (_, target_sort, _) = &target_op.inputs[i];
            if !mapped_sort.alpha_eq(target_sort) {
                return Err(GatError::OpTypeMismatch {
                    op: op.name.to_string(),
                    detail: format!("input {i}: expected sort {mapped_sort}, got {target_sort}"),
                });
            }
        }

        if !m.sort_map.contains_key(op.output.head()) {
            return Err(GatError::MissingSortMapping(op.output.head().to_string()));
        }
        let mapped_output = op
            .output
            .apply_maps(&m.sort_map, &m.op_map)
            .subst(&op_param_rename);
        if !mapped_output.alpha_eq(&target_op.output) {
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

    // --- A6: naturality tests for dependent sort morphisms ---

    /// Dependent-sort category theory with `Hom(a, b)` and `id` + `compose`.
    fn category_theory_for_morphism() -> Theory {
        use crate::sort::{SortExpr, SortParam};
        let ob = Sort::simple("Ob");
        let hom = Sort::dependent(
            "Hom",
            vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
        );
        let hom_xx = SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("x")],
        };
        let id_op = Operation::unary("id", "x", "Ob", hom_xx);
        Theory::new("Cat", vec![ob, hom], vec![id_op], Vec::new())
    }

    #[test]
    fn identity_morphism_on_dependent_category_is_valid() {
        let cat = category_theory_for_morphism();
        let sort_map = HashMap::from([
            (Arc::from("Ob"), Arc::from("Ob")),
            (Arc::from("Hom"), Arc::from("Hom")),
        ]);
        let op_map = HashMap::from([(Arc::from("id"), Arc::from("id"))]);
        let m = TheoryMorphism::new("id", "Cat", "Cat", sort_map, op_map);
        assert!(
            check_morphism(&m, &cat, &cat).is_ok(),
            "identity on dependent category should be a valid morphism",
        );
    }

    #[test]
    fn morphism_dropping_sort_parameter_is_rejected() {
        use crate::sort::{SortExpr, SortParam};
        // Domain: Hom(a, b) parameterised by two Obs.
        // Codomain: Hom with only one Ob parameter.
        let domain = Theory::new(
            "D",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![Operation::unary(
                "id",
                "x",
                "Ob",
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("x"), Term::var("x")],
                },
            )],
            Vec::new(),
        );
        let codomain = Theory::new(
            "C",
            vec![
                Sort::simple("Ob"),
                Sort::dependent("Hom", vec![SortParam::new("a", "Ob")]),
            ],
            vec![Operation::unary(
                "id",
                "x",
                "Ob",
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("x")],
                },
            )],
            Vec::new(),
        );
        let sort_map = HashMap::from([
            (Arc::from("Ob"), Arc::from("Ob")),
            (Arc::from("Hom"), Arc::from("Hom")),
        ]);
        let op_map = HashMap::from([(Arc::from("id"), Arc::from("id"))]);
        let m = TheoryMorphism::new("bad", "D", "C", sort_map, op_map);
        assert!(
            check_morphism(&m, &domain, &codomain).is_err(),
            "morphism that drops a sort parameter should be rejected",
        );
    }

    // --- bulk sort and op renaming recurses through nested structure ---

    /// Applying a theory morphism that renames every sort and op must
    /// rewrite the head of every `SortExpr::App`, every op-application
    /// term nested inside a sort's arguments, and every op name inside
    /// equation bodies. No pre-rename name should survive anywhere in
    /// the renamed theory's signatures or equations.
    #[test]
    fn rename_recurses_through_dependent_sort_args_and_equations() {
        use crate::sort::{SortExpr, SortParam};

        // Build a little theory with Ctx, Ty(Γ : Ctx), and operations:
        //   empty_ctx : Ctx
        //   ty_over : (g : Ctx) -> Ty(g)
        // Then an equation ty_over(empty_ctx()) = ty_over(empty_ctx()).
        fn ctx_sort() -> Sort {
            Sort::simple("Ctx")
        }
        fn ty_sort() -> Sort {
            Sort::dependent("Ty", vec![SortParam::new("g", "Ctx")])
        }
        let empty_ctx = Operation::nullary("empty_ctx", "Ctx");
        let ty_over = Operation::new(
            "ty_over",
            vec![(Arc::from("g"), SortExpr::from("Ctx"))],
            SortExpr::App {
                name: Arc::from("Ty"),
                args: vec![Term::var("g")],
            },
        );
        let refl = Equation::new(
            "refl",
            Term::app("ty_over", vec![Term::constant("empty_ctx")]),
            Term::app("ty_over", vec![Term::constant("empty_ctx")]),
        );
        let domain = Theory::new(
            "D",
            vec![ctx_sort(), ty_sort()],
            vec![empty_ctx, ty_over.clone()],
            vec![refl],
        );

        // Codomain with every sort and op renamed.
        let empty_ctx2 = Operation::nullary("empty2", "C2");
        let ty_over2 = Operation::new(
            "ty2",
            vec![(Arc::from("g"), SortExpr::from("C2"))],
            SortExpr::App {
                name: Arc::from("T2"),
                args: vec![Term::var("g")],
            },
        );
        let refl2 = Equation::new(
            "refl",
            Term::app("ty2", vec![Term::constant("empty2")]),
            Term::app("ty2", vec![Term::constant("empty2")]),
        );
        let codomain = Theory::new(
            "C",
            vec![
                Sort::simple("C2"),
                Sort::dependent("T2", vec![SortParam::new("g", "C2")]),
            ],
            vec![empty_ctx2, ty_over2],
            vec![refl2],
        );

        let sort_map = HashMap::from([
            (Arc::from("Ctx"), Arc::from("C2")),
            (Arc::from("Ty"), Arc::from("T2")),
        ]);
        let op_map = HashMap::from([
            (Arc::from("empty_ctx"), Arc::from("empty2")),
            (Arc::from("ty_over"), Arc::from("ty2")),
        ]);
        let m = TheoryMorphism::new("retag", "D", "C", sort_map.clone(), op_map.clone());
        assert!(
            check_morphism(&m, &domain, &codomain).is_ok(),
            "renaming morphism on dependent theory must check",
        );

        // Now verify apply_maps / rename_ops actually rewrite every
        // nested occurrence. The output sort of ty_over is Ty(g); after
        // applying the maps (and composing with an empty op_map since
        // g is a variable) it must be T2(g), and no pre-rename name
        // must remain in any argument position.
        let mapped_output = ty_over.output.apply_maps(&sort_map, &op_map);
        assert_eq!(&**mapped_output.head(), "T2", "head must be renamed");
        // The arg term is Term::var("g"), which rename_ops leaves alone
        // (no op to rename), but the head rewrite must have happened.
        // If ty_over had a nested op term in its sort args, rename_ops
        // would need to recurse; we exercise that too.
        let nested = SortExpr::App {
            name: Arc::from("Ty"),
            args: vec![Term::app("empty_ctx", vec![])],
        };
        let nested_mapped = nested.apply_maps(&sort_map, &op_map);
        if let SortExpr::App { ref name, ref args } = nested_mapped {
            assert_eq!(&**name, "T2", "head recursed");
            assert_eq!(args.len(), 1);
            if let Term::App { op, .. } = &args[0] {
                assert_eq!(
                    &**op, "empty2",
                    "nested op inside sort arg must also be renamed",
                );
            } else {
                panic!("expected nested app, got {:?}", &args[0]);
            }
        } else {
            panic!("expected App, got {nested_mapped:?}");
        }

        // Equation bodies must also recurse through nested ops.
        let renamed_eq = domain.eqs[0].rename_ops(&op_map);
        // Both sides' outer and inner ops must be rewritten.
        if let Term::App { op, args } = &renamed_eq.lhs {
            assert_eq!(&**op, "ty2");
            if let Term::App { op: inner, .. } = &args[0] {
                assert_eq!(&**inner, "empty2");
            } else {
                panic!("expected inner app, got {:?}", &args[0]);
            }
        } else {
            panic!("expected outer app, got {:?}", &renamed_eq.lhs);
        }
    }

    // --- equation-variable independence across theories ---

    /// Equation variables are universally quantified per equation and
    /// private to the enclosing theory: two independent theories can
    /// reuse the same variable name in their axioms without any
    /// cross-theory interaction. This test builds two such theories,
    /// typechecks each, and confirms that composing the identity
    /// morphism on one does not pick up any structure from the other.
    #[test]
    fn equation_var_names_do_not_leak_across_theories() {
        let t1 = monoid_theory("M1", "mul", "unit");
        let t2 = monoid_theory("M2", "mul", "unit");
        // Both theories have an assoc equation whose free vars are
        // a, b, c. These are local to each equation / theory.
        crate::typecheck::typecheck_theory(&t1).unwrap();
        crate::typecheck::typecheck_theory(&t2).unwrap();
        // An identity morphism on t1 composes with itself without
        // picking up any structure from t2.
        let id1 = TheoryMorphism::identity(&t1);
        let id1_twice = id1.compose(&id1).unwrap();
        assert_eq!(id1_twice.sort_map, id1.sort_map);
        assert_eq!(id1_twice.op_map, id1.op_map);
        // t1 and t2 do not share mutable state: their equations are
        // independent Vec<Equation> values.
        assert_eq!(t1.eqs.len(), t2.eqs.len());
        assert_eq!(t1.eqs[0].name, t2.eqs[0].name);
        assert!(!std::ptr::eq(t1.eqs.as_ptr(), t2.eqs.as_ptr()));
    }

    // --- axiom preservation under morphisms ---

    /// If the codomain lacks an equation that the domain claims (even
    /// after the morphism's renaming is applied), the morphism must
    /// be rejected. This locks in the behaviour of the syntactic
    /// preservation check, which looks for the mapped equation
    /// literally among the codomain's equations.
    #[test]
    fn morphism_rejected_when_mapped_equation_absent_from_codomain() {
        // Domain: monoid with assoc, left_id, right_id.
        let domain = monoid_theory("Mdom", "mul", "unit");
        // Codomain: same sorts and ops, but no equations at all.
        let codomain = Theory::new(
            "Mcod",
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
            Vec::new(),
        );
        let sort_map = HashMap::from([(Arc::from("Carrier"), Arc::from("Carrier"))]);
        let op_map = HashMap::from([
            (Arc::from("mul"), Arc::from("mul")),
            (Arc::from("unit"), Arc::from("unit")),
        ]);
        let m = TheoryMorphism::new("bad", "Mdom", "Mcod", sort_map, op_map);
        assert!(
            matches!(
                check_morphism(&m, &domain, &codomain),
                Err(GatError::EquationNotPreserved { .. }),
            ),
            "missing mapped equation in codomain must yield EquationNotPreserved",
        );
    }

    /// The preservation check is syntactic: a mapped equation that
    /// is derivable in the codomain via directed rewrites but not
    /// listed as a literal equation is still rejected. This test
    /// records that limitation; lifting it would require normalizing
    /// both sides in the codomain before comparing.
    #[test]
    fn morphism_rejected_when_mapped_equation_only_derivable_not_listed() {
        // Domain: a theory with a derived equation f(f(x)) = x, which
        // follows from the directed rewrite f(x) -> x.
        let domain = Theory::new(
            "D",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::var("x"),
            )],
        );
        // Codomain: the same signature, with a directed rewrite
        // `f(x) -> x` that makes `f(f(x)) = x` a consequence, but
        // without listing `idem` as a literal equation. A
        // normalization-based preservation check would accept; the
        // current syntactic check rejects.
        let codomain = Theory::full(
            "C",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![crate::eq::DirectedEquation::new(
                "rule",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );
        let sort_map = HashMap::from([(Arc::from("A"), Arc::from("A"))]);
        let op_map = HashMap::from([(Arc::from("f"), Arc::from("f"))]);
        let m = TheoryMorphism::new("syntactic", "D", "C", sort_map, op_map);
        assert!(
            matches!(
                check_morphism(&m, &domain, &codomain),
                Err(GatError::EquationNotPreserved { .. }),
            ),
            "the current syntactic check rejects derivable-but-not-listed equations",
        );
    }

    // --- parameter alpha-renaming tests ---

    /// Domain category where `id` binds its argument as `x`.
    /// Codomain category where `id` binds its argument as `y`.
    /// The two theories are categorically identical; the morphism
    /// between them must typecheck despite the parameter name change.
    #[test]
    fn morphism_between_alpha_variant_categories_is_valid() {
        use crate::sort::{SortExpr, SortParam};
        let cat_x = {
            let ob = Sort::simple("Ob");
            let hom = Sort::dependent(
                "Hom",
                vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
            );
            let id_x = Operation::unary(
                "id",
                "x",
                "Ob",
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("x"), Term::var("x")],
                },
            );
            Theory::new("CatX", vec![ob, hom], vec![id_x], Vec::new())
        };
        let cat_y = {
            let ob = Sort::simple("Ob");
            let hom = Sort::dependent(
                "Hom",
                vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
            );
            let id_y = Operation::unary(
                "id",
                "y",
                "Ob",
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("y"), Term::var("y")],
                },
            );
            Theory::new("CatY", vec![ob, hom], vec![id_y], Vec::new())
        };
        let sort_map = HashMap::from([
            (Arc::from("Ob"), Arc::from("Ob")),
            (Arc::from("Hom"), Arc::from("Hom")),
        ]);
        let op_map = HashMap::from([(Arc::from("id"), Arc::from("id"))]);
        let m = TheoryMorphism::new("alpha", "CatX", "CatY", sort_map, op_map);
        assert!(
            check_morphism(&m, &cat_x, &cat_y).is_ok(),
            "morphism between alpha-variant dependent theories should be accepted",
        );
    }

    /// Reordering parameters is NOT the same as alpha-renaming them.
    /// Domain `compose : (x, y, z, f: Hom(x, y), g: Hom(y, z)) -> Hom(x, z)`.
    /// Codomain with swapped first two parameters (`y, x, z, ...`) is a
    /// different theory and the morphism must be rejected.
    #[test]
    fn morphism_reordering_parameters_is_rejected() {
        use crate::sort::{SortExpr, SortParam};
        fn hom(a: &str, b: &str) -> SortExpr {
            SortExpr::App {
                name: Arc::from("Hom"),
                args: vec![Term::var(a), Term::var(b)],
            }
        }
        let hom_sort = Sort::dependent(
            "Hom",
            vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
        );

        let domain = {
            let compose = Operation::new(
                "compose",
                vec![
                    (Arc::from("x"), "Ob".into()),
                    (Arc::from("y"), "Ob".into()),
                    (Arc::from("z"), "Ob".into()),
                    (Arc::from("f"), hom("x", "y")),
                    (Arc::from("g"), hom("y", "z")),
                ],
                hom("x", "z"),
            );
            Theory::new(
                "D",
                vec![Sort::simple("Ob"), hom_sort.clone()],
                vec![compose],
                Vec::new(),
            )
        };
        let codomain = {
            // Parameters reordered: (y, x, z, ...) with the same sort
            // expressions would reference the same binder names but in
            // a different positional binding, producing a genuinely
            // different signature.
            let compose = Operation::new(
                "compose",
                vec![
                    (Arc::from("y"), "Ob".into()),
                    (Arc::from("x"), "Ob".into()),
                    (Arc::from("z"), "Ob".into()),
                    (Arc::from("f"), hom("x", "y")),
                    (Arc::from("g"), hom("y", "z")),
                ],
                hom("x", "z"),
            );
            Theory::new(
                "C",
                vec![Sort::simple("Ob"), hom_sort],
                vec![compose],
                Vec::new(),
            )
        };
        let sort_map = HashMap::from([
            (Arc::from("Ob"), Arc::from("Ob")),
            (Arc::from("Hom"), Arc::from("Hom")),
        ]);
        let op_map = HashMap::from([(Arc::from("compose"), Arc::from("compose"))]);
        let m = TheoryMorphism::new("reorder", "D", "C", sort_map, op_map);
        assert!(
            check_morphism(&m, &domain, &codomain).is_err(),
            "reordering parameter positions is not a valid morphism",
        );
    }

    /// Check the param-rename fix at the sort-declaration site as well:
    /// domain uses `Hom(a, b)` where `a, b : Ob`; codomain uses
    /// `Hom(p, q)` where `p, q : Ob`. The sort declaration comparison
    /// must pass.
    #[test]
    fn morphism_renames_sort_param_names() {
        use crate::sort::{SortExpr, SortParam};
        let domain = Theory::new(
            "D",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![Operation::unary(
                "id",
                "x",
                "Ob",
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("x"), Term::var("x")],
                },
            )],
            Vec::new(),
        );
        let codomain = Theory::new(
            "C",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("p", "Ob"), SortParam::new("q", "Ob")],
                ),
            ],
            vec![Operation::unary(
                "id",
                "v",
                "Ob",
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("v"), Term::var("v")],
                },
            )],
            Vec::new(),
        );
        let sort_map = HashMap::from([
            (Arc::from("Ob"), Arc::from("Ob")),
            (Arc::from("Hom"), Arc::from("Hom")),
        ]);
        let op_map = HashMap::from([(Arc::from("id"), Arc::from("id"))]);
        let m = TheoryMorphism::new("rename_params", "D", "C", sort_map, op_map);
        assert!(
            check_morphism(&m, &domain, &codomain).is_ok(),
            "sort parameter name differences should not block a morphism",
        );
    }

    /// If the theories differ in a way that alpha-renaming cannot
    /// repair (e.g. one argument swapped from `x` to `y` where `y` is
    /// not the positionally-corresponding param), the morphism must
    /// still be rejected. This guards against "alpha-renaming papers
    /// over genuine structural differences".
    #[test]
    fn morphism_with_non_positional_name_swap_is_rejected() {
        use crate::sort::{SortExpr, SortParam};
        let domain = Theory::new(
            "D",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            vec![Operation::new(
                "parallel",
                vec![(Arc::from("x"), "Ob".into()), (Arc::from("y"), "Ob".into())],
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("x"), Term::var("y")],
                },
            )],
            Vec::new(),
        );
        let codomain = Theory::new(
            "C",
            vec![
                Sort::simple("Ob"),
                Sort::dependent(
                    "Hom",
                    vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
                ),
            ],
            // codomain output is Hom(y', x') -- swapped, not just renamed.
            vec![Operation::new(
                "parallel",
                vec![
                    (Arc::from("x_prime"), "Ob".into()),
                    (Arc::from("y_prime"), "Ob".into()),
                ],
                SortExpr::App {
                    name: Arc::from("Hom"),
                    args: vec![Term::var("y_prime"), Term::var("x_prime")],
                },
            )],
            Vec::new(),
        );
        let sort_map = HashMap::from([
            (Arc::from("Ob"), Arc::from("Ob")),
            (Arc::from("Hom"), Arc::from("Hom")),
        ]);
        let op_map = HashMap::from([(Arc::from("parallel"), Arc::from("parallel"))]);
        let m = TheoryMorphism::new("bad_swap", "D", "C", sort_map, op_map);
        assert!(
            check_morphism(&m, &domain, &codomain).is_err(),
            "argument swap is a genuine signature difference, not a rename",
        );
    }

    // --- proptest strategies and property tests ---

    mod property {
        use super::*;
        use proptest::prelude::*;

        const SORT_POOL: &[&str] = &["S0", "S1", "S2", "S3", "S4"];
        const OP_POOL: &[&str] = &["f0", "f1", "f2", "f3"];

        /// Lightweight theory generator for morphism property tests.
        /// Generates 1-4 simple sorts and 0-3 operations (no equations,
        /// since rename morphisms preserve equations by construction).
        fn arb_theory() -> impl Strategy<Value = Theory> {
            prop::sample::subsequence(SORT_POOL, 1..=4).prop_flat_map(|sort_names| {
                let sorts: Vec<Sort> = sort_names.iter().map(|s| Sort::simple(*s)).collect();
                let sn: Vec<String> = sort_names.iter().map(|s| (*s).to_owned()).collect();
                let sn2 = sn.clone();
                (
                    Just(sorts),
                    Just(sn.clone()),
                    prop::collection::vec(
                        (
                            prop::sample::select(OP_POOL),
                            prop::sample::select(sn),
                            prop::sample::select(sn2),
                        ),
                        0..=3,
                    ),
                )
                    .prop_map(|(sorts, _sn, op_specs)| {
                        let mut ops = Vec::new();
                        let mut seen = std::collections::HashSet::new();
                        for (name, input_sort, output_sort) in &op_specs {
                            if !seen.insert(*name) {
                                continue;
                            }
                            ops.push(Operation::unary(
                                *name,
                                "x",
                                input_sort.as_str(),
                                output_sort.as_str(),
                            ));
                        }
                        Theory::new("T", sorts, ops, Vec::new())
                    })
            })
        }

        /// Build a renamed copy of a theory and the morphism between them.
        /// Uses a deterministic offset into `RENAME_POOL` to produce unique names.
        fn rename_theory(theory: &Theory, suffix: &str) -> (Theory, TheoryMorphism) {
            let mut sort_map = HashMap::new();
            let mut new_sorts = Vec::new();
            for sort in &theory.sorts {
                let new_name: Arc<str> = Arc::from(format!("{}_{suffix}", sort.name));
                sort_map.insert(Arc::clone(&sort.name), Arc::clone(&new_name));
                new_sorts.push(Sort::simple(&*new_name));
            }

            let mut op_map = HashMap::new();
            let mut new_ops = Vec::new();
            for op in &theory.ops {
                let new_name: Arc<str> = Arc::from(format!("{}_{suffix}", op.name));
                op_map.insert(Arc::clone(&op.name), Arc::clone(&new_name));
                let new_inputs: Vec<(Arc<str>, crate::sort::SortExpr, crate::op::Implicit)> = op
                    .inputs
                    .iter()
                    .map(|(p, s, imp)| (Arc::clone(p), s.apply_maps(&sort_map, &op_map), *imp))
                    .collect();
                let new_output = op.output.apply_maps(&sort_map, &op_map);
                new_ops.push(Operation::with_implicit(&*new_name, new_inputs, new_output));
            }

            // Rename equations.
            let new_eqs: Vec<Equation> =
                theory.eqs.iter().map(|eq| eq.rename_ops(&op_map)).collect();

            let new_theory_name = format!("{}_{suffix}", theory.name);
            let new_theory = Theory::new(&*new_theory_name, new_sorts, new_ops, new_eqs);

            let morphism = TheoryMorphism::new(
                format!("rename_{suffix}"),
                &*theory.name,
                &*new_theory_name,
                sort_map,
                op_map,
            );

            (new_theory, morphism)
        }

        /// Strategy producing (T1, T2, T3, m1: T1->T2, m2: T2->T3).
        fn arb_composable_pair()
        -> impl Strategy<Value = (Theory, Theory, Theory, TheoryMorphism, TheoryMorphism)> {
            arb_theory().prop_map(|t1| {
                let (t2, m1) = rename_theory(&t1, "a");
                let (t3, m2) = rename_theory(&t2, "b");
                (t1, t2, t3, m1, m2)
            })
        }

        /// Strategy producing a chain of three composable morphisms.
        fn arb_composable_triple() -> impl Strategy<
            Value = (
                Theory,
                Theory,
                Theory,
                Theory,
                TheoryMorphism,
                TheoryMorphism,
                TheoryMorphism,
            ),
        > {
            arb_theory().prop_map(|t1| {
                let (t2, m1) = rename_theory(&t1, "a");
                let (t3, m2) = rename_theory(&t2, "b");
                let (t4, m3) = rename_theory(&t3, "c");
                (t1, t2, t3, t4, m1, m2, m3)
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn composition_is_associative(
                (_t1, _t2, _t3, _t4, m1, m2, m3) in arb_composable_triple()
            ) {
                let left = m1.compose(&m2).unwrap().compose(&m3).unwrap();
                let right = m1.compose(&m2.compose(&m3).unwrap()).unwrap();
                prop_assert_eq!(&left.sort_map, &right.sort_map);
                prop_assert_eq!(&left.op_map, &right.op_map);
                prop_assert_eq!(&left.domain, &right.domain);
                prop_assert_eq!(&left.codomain, &right.codomain);
            }

            #[test]
            fn identity_is_left_unit((t1, _t2, _t3, m1, _m2) in arb_composable_pair()) {
                let id = TheoryMorphism::identity(&t1);
                let id_then_m = id.compose(&m1).unwrap();
                prop_assert_eq!(&id_then_m.sort_map, &m1.sort_map);
                prop_assert_eq!(&id_then_m.op_map, &m1.op_map);
            }

            #[test]
            fn identity_is_right_unit((_t1, t2, _t3, m1, _m2) in arb_composable_pair()) {
                let id = TheoryMorphism::identity(&t2);
                let m_then_id = m1.compose(&id).unwrap();
                prop_assert_eq!(&m_then_id.sort_map, &m1.sort_map);
                prop_assert_eq!(&m_then_id.op_map, &m1.op_map);
            }

            #[test]
            fn renamed_morphism_is_valid(t in arb_theory()) {
                let (t2, m) = rename_theory(&t, "test");
                prop_assert!(
                    check_morphism(&m, &t, &t2).is_ok(),
                    "renaming morphism should be valid",
                );
            }

            #[test]
            fn composition_preserves_validity(
                (t1, t2, t3, m1, m2) in arb_composable_pair()
            ) {
                // Both individual morphisms are valid by construction.
                prop_assert!(check_morphism(&m1, &t1, &t2).is_ok());
                prop_assert!(check_morphism(&m2, &t2, &t3).is_ok());
                // Composition must also be valid.
                let composed = m1.compose(&m2).unwrap();
                prop_assert!(
                    check_morphism(&composed, &t1, &t3).is_ok(),
                    "composed morphism should be valid",
                );
            }
        }
    }
}
