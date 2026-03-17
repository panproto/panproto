use std::collections::HashMap;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::eq::Term;
use crate::error::GatError;
use crate::morphism::TheoryMorphism;
use crate::theory::Theory;

/// A natural transformation between two theory morphisms F, G: T1 -> T2.
///
/// For each sort S in the domain theory, a component maps a term of sort F(S)
/// to a term of sort G(S). Components are expressed as terms with a single
/// free variable `"x"` standing for the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaturalTransformation {
    /// A human-readable name for this natural transformation.
    pub name: Arc<str>,
    /// The name of the source morphism F.
    pub source: Arc<str>,
    /// The name of the target morphism G.
    pub target: Arc<str>,
    /// For each domain sort S, a term with free variable "x" mapping F(S) -> G(S).
    pub components: HashMap<Arc<str>, Term>,
}

/// Check that a natural transformation is valid.
///
/// Verifies that:
/// 1. F and G have the same domain and codomain.
/// 2. Every sort in the domain has a component.
/// 3. Each component term only uses operations that exist in the codomain.
/// 4. The naturality condition holds for unary operations.
///
/// # Errors
///
/// Returns a [`GatError`] describing the first violation found.
pub fn check_natural_transformation(
    nt: &NaturalTransformation,
    f: &TheoryMorphism,
    g: &TheoryMorphism,
    domain: &Theory,
    codomain: &Theory,
) -> Result<(), GatError> {
    // 1. Verify F and G have same domain and codomain.
    if f.domain != g.domain || f.codomain != g.codomain {
        return Err(GatError::NatTransDomainMismatch {
            source_morphism: f.name.to_string(),
            target_morphism: g.name.to_string(),
        });
    }

    // 2. Verify all domain sorts have components.
    for sort in &domain.sorts {
        if !nt.components.contains_key(&sort.name) {
            return Err(GatError::MissingNatTransComponent(sort.name.to_string()));
        }
    }

    // 3. Validate component term ops exist in codomain.
    for (sort_name, term) in &nt.components {
        validate_term_ops(term, codomain).map_err(|detail| GatError::NatTransComponentInvalid {
            sort: sort_name.to_string(),
            detail,
        })?;
    }

    // 4. Check naturality squares for all operations.
    //
    // For a unary op `op: S -> T`, the square is:
    //   alpha_T[x := F(op)(x)] == G(op)(alpha_S(x))
    //
    // For an n-ary op `op: (S1, ..., Sn) -> T` we use variables x0..xn-1:
    //   alpha_T[x := F(op)(x0, ..., xn-1)] == G(op)(alpha_{S1}[x:=x0], ..., alpha_{Sn}[x:=xn-1])
    //
    // Nullary ops (constants) reduce to:
    //   alpha_T[x := F(op)()] == G(op)()
    for op in &domain.ops {
        let output_sort = &op.output;
        let alpha_output = nt
            .components
            .get(output_sort)
            .ok_or_else(|| GatError::MissingNatTransComponent(output_sort.to_string()))?;

        let f_op = f
            .op_map
            .get(&op.name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&op.name));
        let g_op = g
            .op_map
            .get(&op.name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&op.name));

        // Build variable names for each input: x0, x1, ... (or just "x" for unary).
        let var_names: Vec<Arc<str>> = if op.inputs.len() == 1 {
            vec![Arc::from("x")]
        } else {
            (0..op.inputs.len())
                .map(|i| Arc::from(format!("x{i}")))
                .collect()
        };

        // LHS: alpha_T[x := F(op)(x0, ..., xn-1)]
        let f_op_applied = Term::app(
            Arc::clone(&f_op),
            var_names.iter().map(|v| Term::var(Arc::clone(v))).collect(),
        );
        let mut subst_lhs = FxHashMap::default();
        subst_lhs.insert(Arc::from("x"), f_op_applied);
        let lhs = alpha_output.substitute(&subst_lhs);

        // RHS: G(op)(alpha_{S1}[x:=x0], ..., alpha_{Sn}[x:=xn-1])
        let mut rhs_args = Vec::with_capacity(op.inputs.len());
        for (i, (_, input_sort)) in op.inputs.iter().enumerate() {
            let alpha_input = nt
                .components
                .get(input_sort)
                .ok_or_else(|| GatError::MissingNatTransComponent(input_sort.to_string()))?;
            let mut subst_arg = FxHashMap::default();
            subst_arg.insert(Arc::from("x"), Term::var(Arc::clone(&var_names[i])));
            rhs_args.push(alpha_input.substitute(&subst_arg));
        }
        let rhs = Term::app(g_op, rhs_args);

        if lhs != rhs {
            return Err(GatError::NaturalityViolation {
                op: op.name.to_string(),
                lhs: format!("{lhs:?}"),
                rhs: format!("{rhs:?}"),
            });
        }
    }

    Ok(())
}

/// Recursively validate that all operations used in a term exist in the codomain theory.
fn validate_term_ops(term: &Term, codomain: &Theory) -> Result<(), String> {
    match term {
        Term::Var(_) => Ok(()),
        Term::App { op, args } => {
            if !codomain.has_op(op) {
                return Err(format!("operation {op} not found in codomain"));
            }
            for arg in args {
                validate_term_ops(arg, codomain)?;
            }
            Ok(())
        }
    }
}

/// Vertical composition: given alpha: F => G and beta: G => H, produce beta . alpha: F => H.
///
/// For each sort S, `(beta . alpha)_S = beta_S[x := alpha_S(x)]`.
///
/// # Errors
///
/// Returns [`GatError::NatTransComposeMismatch`] if alpha's target differs from beta's source.
pub fn vertical_compose(
    alpha: &NaturalTransformation,
    beta: &NaturalTransformation,
    domain: &Theory,
) -> Result<NaturalTransformation, GatError> {
    // Check alpha.target == beta.source.
    if alpha.target != beta.source {
        return Err(GatError::NatTransComposeMismatch {
            alpha_target: alpha.target.to_string(),
            beta_source: beta.source.to_string(),
        });
    }

    let mut components = HashMap::new();
    for sort in &domain.sorts {
        let alpha_s = alpha
            .components
            .get(&sort.name)
            .ok_or_else(|| GatError::MissingNatTransComponent(sort.name.to_string()))?;
        let beta_s = beta
            .components
            .get(&sort.name)
            .ok_or_else(|| GatError::MissingNatTransComponent(sort.name.to_string()))?;

        // (beta . alpha)_S = beta_S[x := alpha_S]
        let mut subst = FxHashMap::default();
        subst.insert(Arc::from("x"), alpha_s.clone());
        let composed = beta_s.substitute(&subst);
        components.insert(Arc::clone(&sort.name), composed);
    }

    Ok(NaturalTransformation {
        name: Arc::from(format!("{}.{}", beta.name, alpha.name)),
        source: Arc::clone(&alpha.source),
        target: Arc::clone(&beta.target),
        components,
    })
}

/// Horizontal composition: given alpha: F => G and beta: H => K where G's codomain is H's domain,
/// produce (beta * alpha): H.F => K.G.
///
/// For each sort S in the domain, `(beta * alpha)_S = beta_{G(S)}[x := H(alpha_S)]`.
///
/// # Arguments
///
/// * `alpha` - Natural transformation F => G
/// * `beta` - Natural transformation H => K
/// * `f` - Morphism F (alpha's source, unused but kept for API symmetry)
/// * `g` - Morphism G (alpha's target, used to look up sort mappings)
/// * `h` - Morphism H (beta's source, used to apply to alpha components)
/// * `domain` - The domain theory of F and G
///
/// # Errors
///
/// Returns [`GatError::MissingNatTransComponent`] if a required component is missing.
pub fn horizontal_compose(
    alpha: &NaturalTransformation,
    beta: &NaturalTransformation,
    _f: &TheoryMorphism,
    g: &TheoryMorphism,
    h: &TheoryMorphism,
    domain: &Theory,
) -> Result<NaturalTransformation, GatError> {
    let mut components = HashMap::new();
    for sort in &domain.sorts {
        // G(S): the sort that G maps S to.
        let g_s = g
            .sort_map
            .get(&sort.name)
            .ok_or_else(|| GatError::MissingSortMapping(sort.name.to_string()))?;

        // beta_{G(S)}: component of beta at G(S).
        let beta_gs = beta
            .components
            .get(g_s)
            .ok_or_else(|| GatError::MissingNatTransComponent(g_s.to_string()))?;

        // alpha_S: component of alpha at S.
        let alpha_s = alpha
            .components
            .get(&sort.name)
            .ok_or_else(|| GatError::MissingNatTransComponent(sort.name.to_string()))?;

        // H(alpha_S): apply H's op renaming to alpha_S.
        let h_alpha_s = h.apply_to_term(alpha_s);

        // (beta * alpha)_S = beta_{G(S)}[x := H(alpha_S)]
        let mut subst = FxHashMap::default();
        subst.insert(Arc::from("x"), h_alpha_s);
        let composed = beta_gs.substitute(&subst);
        components.insert(Arc::clone(&sort.name), composed);
    }

    Ok(NaturalTransformation {
        name: Arc::from(format!("{}*{}", beta.name, alpha.name)),
        source: Arc::from(format!("{}.{}", beta.source, alpha.source)),
        target: Arc::from(format!("{}.{}", beta.target, alpha.target)),
        components,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morphism::TheoryMorphism;
    use crate::op::Operation;
    use crate::sort::Sort;
    use crate::theory::Theory;

    /// Build a simple theory with two sorts and a unary op for testing.
    fn two_sort_theory() -> Theory {
        Theory::new(
            "T",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        )
    }

    /// Build an identity morphism on the given theory.
    fn identity_morphism(theory: &Theory, name: &str) -> TheoryMorphism {
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
        TheoryMorphism::new(name, &*theory.name, &*theory.name, sort_map, op_map)
    }

    /// Build an identity natural transformation on the identity morphism.
    fn identity_nat_trans(
        theory: &Theory,
        source: &str,
        target: &str,
        name: &str,
    ) -> NaturalTransformation {
        let components: HashMap<Arc<str>, Term> = theory
            .sorts
            .iter()
            .map(|s| (Arc::clone(&s.name), Term::var("x")))
            .collect();
        NaturalTransformation {
            name: Arc::from(name),
            source: Arc::from(source),
            target: Arc::from(target),
            components,
        }
    }

    #[test]
    fn identity_nat_trans_validates() {
        let theory = two_sort_theory();
        let morph = identity_morphism(&theory, "id");
        let nt = identity_nat_trans(&theory, "id", "id", "id_nt");

        let result = check_natural_transformation(&nt, &morph, &morph, &theory, &theory);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn vertical_compose_identities_yields_identity() -> Result<(), Box<dyn std::error::Error>> {
        let theory = two_sort_theory();
        let alpha = identity_nat_trans(&theory, "id", "id", "alpha");
        let beta = identity_nat_trans(&theory, "id", "id", "beta");

        let composed = vertical_compose(&alpha, &beta, &theory)?;

        // Each component should be Var("x") since id[x := id] = id.
        for sort in &theory.sorts {
            let comp = composed
                .components
                .get(&sort.name)
                .ok_or("missing component for sort")?;
            assert_eq!(comp, &Term::var("x"));
        }
        Ok(())
    }

    #[test]
    fn naturality_violation_detected() {
        let theory = two_sort_theory();
        let _morph = identity_morphism(&theory, "id");

        // Bad component: alpha_A = x, alpha_B = constant("bad")
        // Naturality for f: A -> B requires alpha_B[x := f(x)] == f(alpha_A(x))
        // LHS: bad()  (constant, no x to substitute)
        // RHS: f(x)
        // These are not equal.
        let mut components = HashMap::new();
        components.insert(Arc::from("A"), Term::var("x"));
        // Use a constant that exists in codomain -- we need to add it.
        // Actually, let's use a theory with a constant.
        let theory_with_const = Theory::new(
            "T",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![
                Operation::unary("f", "x", "A", "B"),
                Operation::nullary("bad", "B"),
            ],
            Vec::new(),
        );
        let morph2 = identity_morphism(&theory_with_const, "id");

        components.insert(Arc::from("B"), Term::constant("bad"));
        let nt = NaturalTransformation {
            name: Arc::from("bad_nt"),
            source: Arc::from("id"),
            target: Arc::from("id"),
            components,
        };

        // Domain is the original theory (with just f), codomain has the constant.
        let result =
            check_natural_transformation(&nt, &morph2, &morph2, &theory, &theory_with_const);
        assert!(
            matches!(result, Err(GatError::NaturalityViolation { .. })),
            "expected NaturalityViolation, got {result:?}"
        );
    }

    #[test]
    fn missing_component_detected() {
        let theory = two_sort_theory();
        let morph = identity_morphism(&theory, "id");

        // Only provide component for A, not B.
        let mut components = HashMap::new();
        components.insert(Arc::from("A"), Term::var("x"));
        let nt = NaturalTransformation {
            name: Arc::from("partial"),
            source: Arc::from("id"),
            target: Arc::from("id"),
            components,
        };

        let result = check_natural_transformation(&nt, &morph, &morph, &theory, &theory);
        assert!(
            matches!(result, Err(GatError::MissingNatTransComponent(_))),
            "expected MissingNatTransComponent, got {result:?}"
        );
    }

    #[test]
    fn domain_mismatch_detected() {
        let t1 = Theory::new("T1", vec![Sort::simple("A")], Vec::new(), Vec::new());
        let _t2 = Theory::new("T2", vec![Sort::simple("B")], Vec::new(), Vec::new());

        let f = TheoryMorphism::new(
            "f",
            "T1",
            "T1",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::new(),
        );
        let g = TheoryMorphism::new(
            "g",
            "T2",
            "T2",
            HashMap::from([(Arc::from("B"), Arc::from("B"))]),
            HashMap::new(),
        );

        let nt = NaturalTransformation {
            name: Arc::from("bad"),
            source: Arc::from("f"),
            target: Arc::from("g"),
            components: HashMap::new(),
        };

        let result = check_natural_transformation(&nt, &f, &g, &t1, &t1);
        assert!(
            matches!(result, Err(GatError::NatTransDomainMismatch { .. })),
            "expected NatTransDomainMismatch, got {result:?}"
        );
    }

    #[test]
    fn composition_mismatch_detected() {
        let theory = two_sort_theory();
        let alpha = identity_nat_trans(&theory, "f", "g", "alpha");
        let beta = identity_nat_trans(&theory, "h", "k", "beta");

        let result = vertical_compose(&alpha, &beta, &theory);
        assert!(
            matches!(result, Err(GatError::NatTransComposeMismatch { .. })),
            "expected NatTransComposeMismatch, got {result:?}"
        );
    }
}
