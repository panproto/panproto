use std::collections::HashMap;
use std::sync::Arc;

use crate::eq::alpha_equivalent_equation;
use crate::error::GatError;
use crate::morphism::TheoryMorphism;
use crate::sort::signatures_equivalent_modulo_param_rename;
use crate::theory::Theory;

/// Result of a categorical pushout (colimit) computation.
///
/// Contains the pushout theory along with inclusion morphisms from
/// both input theories into the pushout. The cocone condition
/// `j1 ∘ i1 = j2 ∘ i2` is verified at construction time.
#[derive(Debug, Clone)]
pub struct ColimitResult {
    /// The pushout theory.
    pub theory: Theory,
    /// Inclusion morphism from the first theory into the pushout: j1: T1 → P.
    pub inclusion1: TheoryMorphism,
    /// Inclusion morphism from the second theory into the pushout: j2: T2 → P.
    pub inclusion2: TheoryMorphism,
}

impl ColimitResult {
    /// Verify the cocone (commutativity) condition: `j1 ∘ i1 = j2 ∘ i2`.
    ///
    /// For every sort and operation in the shared theory, the two paths
    /// through the pushout must agree.
    ///
    /// # Errors
    ///
    /// Returns [`GatError`] if any composition fails or the cocone condition
    /// is violated.
    pub fn verify_cocone(
        &self,
        i1: &TheoryMorphism,
        i2: &TheoryMorphism,
        shared: &Theory,
    ) -> Result<(), GatError> {
        let lhs = i1.compose(&self.inclusion1)?;
        let rhs = i2.compose(&self.inclusion2)?;

        for sort in &shared.sorts {
            let l = lhs.sort_map.get(&sort.name);
            let r = rhs.sort_map.get(&sort.name);
            if l != r {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("cocone sort {}", sort.name),
                    detail: format!(
                        "j1∘i1 maps to {}, j2∘i2 maps to {}",
                        l.map_or("(none)", |s| s.as_ref()),
                        r.map_or("(none)", |s| s.as_ref()),
                    ),
                });
            }
        }

        for op in &shared.ops {
            let l = lhs.op_map.get(&op.name);
            let r = rhs.op_map.get(&op.name);
            if l != r {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("cocone op {}", op.name),
                    detail: format!(
                        "j1∘i1 maps to {}, j2∘i2 maps to {}",
                        l.map_or("(none)", |s| s.as_ref()),
                        r.map_or("(none)", |s| s.as_ref()),
                    ),
                });
            }
        }

        Ok(())
    }
}

/// Compute the pushout (colimit) of two theories over explicit morphisms.
///
/// Given morphisms `i1: S → T1` and `i2: S → T2` from a shared theory S,
/// this produces the pushout P with inclusion morphisms `j1: T1 → P` and
/// `j2: T2 → P` satisfying the universal property: `j1 ∘ i1 = j2 ∘ i2`.
///
/// The pushout identifies `i1(x)` with `i2(x)` for every sort and operation
/// x in the shared theory.
///
/// # Errors
///
/// Returns [`GatError::SortConflict`] if T1 and T2 both declare a sort with
/// the same name but incompatible definitions (different parameter lists) and
/// the sort is not identified via the morphisms.
///
/// Returns [`GatError::OpConflict`] if T1 and T2 both declare an operation
/// with the same name but incompatible signatures and the operation is not
/// identified via the morphisms.
///
/// Returns [`GatError::EqConflict`] if T1 and T2 both declare an equation
/// with the same name but different content and the equation is not identified
/// via the morphisms.
pub fn colimit(
    t1: &Theory,
    t2: &Theory,
    i1: &TheoryMorphism,
    i2: &TheoryMorphism,
) -> Result<ColimitResult, GatError> {
    let (sort_rename, op_rename) = build_rename_maps(i1, i2);

    let sorts = merge_sorts(t1, t2, &sort_rename)?;
    let ops = merge_ops(t1, t2, &sort_rename, &op_rename)?;

    let eqs = merge_equations(t1, t2, &op_rename)?;
    let directed_eqs = merge_directed_equations(t1, t2, &op_rename)?;
    let policies = merge_policies(t1, t2)?;

    let pushout_name: Arc<str> = format!("{}_{}_colimit", t1.name, t2.name).into();
    let theory = Theory::full(
        Arc::clone(&pushout_name),
        Vec::new(),
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
    );

    let j1 = build_inclusion(t1, &pushout_name, &HashMap::new(), &HashMap::new());
    let j2 = build_inclusion(t2, &pushout_name, &sort_rename, &op_rename);

    let result = ColimitResult {
        theory,
        inclusion1: j1,
        inclusion2: j2,
    };

    verify_cocone(i1, i2, &result)?;
    Ok(result)
}

impl ColimitResult {
    /// Verify the universal property of the pushout against an
    /// alternative cocone `(q, k1, k2)` and return the unique mediating
    /// morphism `m : self.theory → q` satisfying
    /// `m ∘ self.inclusion1 = k1` and `m ∘ self.inclusion2 = k2`.
    ///
    /// The mediating morphism is constructed by case-analysis: every
    /// element of the pushout is either an image of T1 (and is sent
    /// to its image under k1) or an image of T2 not already covered
    /// (and is sent to its image under k2). Cocone commutativity
    /// guarantees the two assignments agree on the shared image.
    ///
    /// # Errors
    ///
    /// Returns [`GatError::EquationNotPreserved`] when `(q, k1, k2)`
    /// is not a valid cocone (i.e. `k1` and `k2` disagree on a name
    /// that the pushout identifies). The check is performed locally
    /// via the cocone-commutativity comparison.
    pub fn verify_universal(
        &self,
        q: &Theory,
        k1: &TheoryMorphism,
        k2: &TheoryMorphism,
    ) -> Result<TheoryMorphism, GatError> {
        if k1.codomain != q.name || k2.codomain != q.name {
            return Err(GatError::EquationNotPreserved {
                equation: "universal property".to_string(),
                detail: format!(
                    "alternative cocone codomain mismatch: k1={}, k2={}, q={}",
                    k1.codomain, k2.codomain, q.name,
                ),
            });
        }

        let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        merge_mediator_assignments(
            &mut sort_map,
            &self.inclusion1.sort_map,
            &k1.sort_map,
            "sort",
            "k1",
        )?;
        merge_mediator_assignments(
            &mut sort_map,
            &self.inclusion2.sort_map,
            &k2.sort_map,
            "sort",
            "k2",
        )?;

        let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
        merge_mediator_assignments(&mut op_map, &self.inclusion1.op_map, &k1.op_map, "op", "k1")?;
        merge_mediator_assignments(&mut op_map, &self.inclusion2.op_map, &k2.op_map, "op", "k2")?;

        // Defensive coverage check: every sort/op present in the
        // pushout theory must have a mediator entry. Construction of
        // the pushout from `t1 ⊔ t2` quotient guarantees this in
        // principle (every name comes from one of the two inclusions),
        // but a future refactor that adds free generators to the
        // pushout would break the universal-property contract; we
        // detect that here rather than relying on the downstream
        // `compose` call's `ComposeUnmapped` error message.
        for sort in &self.theory.sorts {
            if !sort_map.contains_key(&sort.name) {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal property sort {}", sort.name),
                    detail: format!(
                        "pushout sort `{}` is not the image of any T1 or T2 sort under the inclusions; \
                         no mediator can be defined on it",
                        sort.name,
                    ),
                });
            }
        }
        for op in &self.theory.ops {
            if !op_map.contains_key(&op.name) {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal property op {}", op.name),
                    detail: format!(
                        "pushout op `{}` is not the image of any T1 or T2 op under the inclusions; \
                         no mediator can be defined on it",
                        op.name,
                    ),
                });
            }
        }

        let mediator = TheoryMorphism::new(
            format!("mediator_{}_to_{}", self.theory.name, q.name),
            Arc::clone(&self.theory.name),
            Arc::clone(&q.name),
            sort_map,
            op_map,
        );
        let m_j1 = self.inclusion1.compose(&mediator)?;
        let m_j2 = self.inclusion2.compose(&mediator)?;
        if m_j1.sort_map != k1.sort_map || m_j1.op_map != k1.op_map {
            return Err(GatError::EquationNotPreserved {
                equation: "universal property: m ∘ j1 = k1".to_string(),
                detail: "mediating morphism does not factor k1 through j1".to_string(),
            });
        }
        if m_j2.sort_map != k2.sort_map || m_j2.op_map != k2.op_map {
            return Err(GatError::EquationNotPreserved {
                equation: "universal property: m ∘ j2 = k2".to_string(),
                detail: "mediating morphism does not factor k2 through j2".to_string(),
            });
        }
        Ok(mediator)
    }
}

/// Helper: thread the mediator assignment for one leg of the cocone.
/// `inclusion` maps T-names to P-names (here-codomain); `k` maps
/// T-names to Q-names. We assemble `mediator: P → Q` by composing.
fn merge_mediator_assignments(
    mediator: &mut HashMap<Arc<str>, Arc<str>>,
    inclusion: &HashMap<Arc<str>, Arc<str>>,
    k: &HashMap<Arc<str>, Arc<str>>,
    kind: &str,
    leg: &str,
) -> Result<(), GatError> {
    for (t_name, p_name) in inclusion {
        let q_name = k
            .get(t_name)
            .ok_or_else(|| GatError::EquationNotPreserved {
                equation: format!("universal property {kind} {p_name}"),
                detail: format!("{leg} has no mapping for {kind} `{t_name}`"),
            })?;
        match mediator.get(p_name) {
            None => {
                mediator.insert(Arc::clone(p_name), Arc::clone(q_name));
            }
            Some(existing) if existing == q_name => {}
            Some(existing) => {
                return Err(GatError::EquationNotPreserved {
                    equation: format!("universal property {kind} {p_name}"),
                    detail: format!(
                        "two T-preimages of `{p_name}` map to distinct Q-images under {leg}: \
                         `{existing}` vs `{q_name}`; the alternative cocone is not commutative",
                    ),
                });
            }
        }
    }
    Ok(())
}

/// A rename map keyed and valued by name (used both for sorts and ops).
type RenameMap = HashMap<Arc<str>, Arc<str>>;

/// Build T2 → T1 rename maps from the shared-theory morphisms.
///
/// For each sort (or op) `s` in the shared theory, we have `i1(s)` in T1
/// and `i2(s)` in T2; the pushout picks T1's name, so we rename `i2(s)`
/// to `i1(s)`.
fn build_rename_maps(i1: &TheoryMorphism, i2: &TheoryMorphism) -> (RenameMap, RenameMap) {
    let mut sort_rename = HashMap::new();
    for (shared_sort, t1_sort) in &i1.sort_map {
        if let Some(t2_sort) = i2.sort_map.get(shared_sort) {
            sort_rename.insert(Arc::clone(t2_sort), Arc::clone(t1_sort));
        }
    }
    let mut op_rename = HashMap::new();
    for (shared_op, t1_op) in &i1.op_map {
        if let Some(t2_op) = i2.op_map.get(shared_op) {
            op_rename.insert(Arc::clone(t2_op), Arc::clone(t1_op));
        }
    }
    (sort_rename, op_rename)
}

/// Merge T2's sorts into T1's, resolving identifications via `sort_rename`.
///
/// Returns [`GatError::SortConflict`] if two independently-declared sorts
/// share a name but disagree on parameters or kind.
fn merge_sorts(
    t1: &Theory,
    t2: &Theory,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
) -> Result<Vec<crate::sort::Sort>, GatError> {
    let mut sorts = t1.sorts.clone();
    for sort in &t2.sorts {
        let effective_name = sort_rename
            .get(&sort.name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&sort.name));
        if t1.has_sort(&effective_name) {
            if sort_rename.contains_key(&sort.name) {
                continue;
            }
            let t1_sort = t1
                .find_sort(&effective_name)
                .ok_or_else(|| GatError::SortConflict {
                    name: effective_name.to_string(),
                })?;
            if t1_sort.params != sort.params || t1_sort.kind != sort.kind {
                return Err(GatError::SortConflict {
                    name: effective_name.to_string(),
                });
            }
        } else {
            sorts.push(rename_sort_refs(sort, sort_rename));
        }
    }
    Ok(sorts)
}

/// Merge T2's operations into T1's, renaming sort references via
/// `sort_rename` and identifying operations via `op_rename`.
///
/// Returns [`GatError::OpConflict`] if two independently-declared
/// operations share a name but disagree on signature.
fn merge_ops(
    t1: &Theory,
    t2: &Theory,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> Result<Vec<crate::op::Operation>, GatError> {
    let mut ops = t1.ops.clone();
    for op in &t2.ops {
        let effective_name = op_rename
            .get(&op.name)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&op.name));
        if t1.has_op(&effective_name) {
            if op_rename.contains_key(&op.name) {
                continue;
            }
            let t1_op = t1
                .find_op(&effective_name)
                .ok_or_else(|| GatError::OpConflict {
                    name: effective_name.to_string(),
                })?;
            let renamed_op = rename_op_sort_refs(op, sort_rename);
            if !signatures_equivalent_modulo_param_rename(
                &t1_op.inputs,
                &t1_op.output,
                &renamed_op.inputs,
                &renamed_op.output,
            ) {
                return Err(GatError::OpConflict {
                    name: effective_name.to_string(),
                });
            }
        } else {
            ops.push(rename_op_sort_refs(op, sort_rename));
        }
    }
    Ok(ops)
}

/// Verify the cocone condition `j1 ∘ i1 = j2 ∘ i2` on every shared sort and op.
fn verify_cocone(
    i1: &TheoryMorphism,
    i2: &TheoryMorphism,
    result: &ColimitResult,
) -> Result<(), GatError> {
    let lhs = i1.compose(&result.inclusion1)?;
    let rhs = i2.compose(&result.inclusion2)?;
    for shared_sort in i1.sort_map.keys() {
        let l = lhs.sort_map.get(shared_sort);
        let r = rhs.sort_map.get(shared_sort);
        if l != r {
            return Err(GatError::EquationNotPreserved {
                equation: format!("cocone sort {shared_sort}"),
                detail: format!(
                    "j1∘i1 maps to {}, j2∘i2 maps to {}",
                    l.map_or("(none)", |s| s.as_ref()),
                    r.map_or("(none)", |s| s.as_ref()),
                ),
            });
        }
    }
    for shared_op in i1.op_map.keys() {
        let l = lhs.op_map.get(shared_op);
        let r = rhs.op_map.get(shared_op);
        if l != r {
            return Err(GatError::EquationNotPreserved {
                equation: format!("cocone op {shared_op}"),
                detail: format!(
                    "j1∘i1 maps to {}, j2∘i2 maps to {}",
                    l.map_or("(none)", |s| s.as_ref()),
                    r.map_or("(none)", |s| s.as_ref()),
                ),
            });
        }
    }
    Ok(())
}

/// Merge equations from t2 into t1's equations, checking alpha-equivalence for conflicts.
///
/// Applies `op_rename` to T2's equation terms before comparison so that
/// operations identified via the morphisms are properly aligned with T1's
/// naming convention.
/// Rename sort references in a sort's dependent parameters using the rename map.
fn rename_sort_refs(
    sort: &crate::sort::Sort,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
) -> crate::sort::Sort {
    let params = sort
        .params
        .iter()
        .map(|p| crate::sort::SortParam {
            name: Arc::clone(&p.name),
            sort: p.sort.rename_head(sort_rename),
        })
        .collect();
    crate::sort::Sort {
        name: Arc::clone(&sort.name),
        params,
        kind: sort.kind.clone(),
        closure: sort.closure.clone(),
    }
}

/// Rename sort references in an operation's input / output sorts using
/// the rename map. Renames only the sort heads; argument terms of
/// dependent sorts refer to parameter names that are local to each
/// operation and are therefore unaffected.
fn rename_op_sort_refs(
    op: &crate::op::Operation,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
) -> crate::op::Operation {
    let inputs: Vec<(Arc<str>, crate::sort::SortExpr, crate::op::Implicit)> = op
        .inputs
        .iter()
        .map(|(name, sort, imp)| (Arc::clone(name), sort.rename_head(sort_rename), *imp))
        .collect();
    let output = op.output.rename_head(sort_rename);
    crate::op::Operation::with_implicit(Arc::clone(&op.name), inputs, output)
}

fn merge_equations(
    t1: &Theory,
    t2: &Theory,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> Result<Vec<crate::eq::Equation>, GatError> {
    let mut eqs = t1.eqs.clone();
    for eq in &t2.eqs {
        let renamed = eq.rename_ops(op_rename);
        if let Some(t1_eq) = t1.find_eq(&eq.name) {
            if !alpha_equivalent_equation(&t1_eq.lhs, &t1_eq.rhs, &renamed.lhs, &renamed.rhs) {
                return Err(GatError::EqConflict {
                    name: eq.name.to_string(),
                });
            }
        } else {
            eqs.push(renamed);
        }
    }
    Ok(eqs)
}

/// Merge directed equations from t2 into t1's directed equations.
///
/// Applies `op_rename` to T2's directed equation terms before comparison.
fn merge_directed_equations(
    t1: &Theory,
    t2: &Theory,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> Result<Vec<crate::eq::DirectedEquation>, GatError> {
    let mut directed_eqs = t1.directed_eqs.clone();
    for de in &t2.directed_eqs {
        let renamed = de.rename_ops(op_rename);
        if let Some(t1_de) = t1.find_directed_eq(&de.name) {
            if !alpha_equivalent_equation(&t1_de.lhs, &t1_de.rhs, &renamed.lhs, &renamed.rhs) {
                return Err(GatError::DirectedEqConflict {
                    name: de.name.to_string(),
                });
            }
        } else {
            directed_eqs.push(renamed);
        }
    }
    Ok(directed_eqs)
}

/// Merge conflict policies from t2 into t1's policies.
fn merge_policies(
    t1: &Theory,
    t2: &Theory,
) -> Result<Vec<crate::theory::ConflictPolicy>, GatError> {
    let mut policies = t1.policies.clone();
    for pol in &t2.policies {
        if let Some(t1_pol) = t1.find_policy(&pol.name) {
            if t1_pol.value_kind != pol.value_kind || t1_pol.strategy != pol.strategy {
                return Err(GatError::PolicyConflict {
                    name: pol.name.to_string(),
                });
            }
        } else {
            policies.push(pol.clone());
        }
    }
    Ok(policies)
}

/// Build an inclusion morphism from `source` into the pushout theory named `pushout_name`.
///
/// Shared sorts/ops are renamed according to the given maps; non-shared sorts/ops
/// map to themselves.
fn build_inclusion(
    source: &Theory,
    pushout_name: &Arc<str>,
    sort_rename: &HashMap<Arc<str>, Arc<str>>,
    op_rename: &HashMap<Arc<str>, Arc<str>>,
) -> TheoryMorphism {
    let sort_map: HashMap<Arc<str>, Arc<str>> = source
        .sorts
        .iter()
        .map(|s| {
            let target = sort_rename
                .get(&s.name)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&s.name));
            (Arc::clone(&s.name), target)
        })
        .collect();
    let op_map: HashMap<Arc<str>, Arc<str>> = source
        .ops
        .iter()
        .map(|o| {
            let target = op_rename
                .get(&o.name)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&o.name));
            (Arc::clone(&o.name), target)
        })
        .collect();
    TheoryMorphism::new(
        format!("incl_{}_{pushout_name}", source.name),
        &*source.name,
        &**pushout_name,
        sort_map,
        op_map,
    )
}

/// Compute the pushout (colimit) of two theories over a shared base theory.
///
/// Given theories `t1` and `t2` that both extend a `shared` theory, this
/// produces a combined theory containing all sorts, operations, and equations
/// from both, with the shared components identified (not duplicated).
///
/// The resulting theory is named `"{t1.name}_{t2.name}_colimit"`.
/// Compute the pushout (colimit) of two theories sharing a base
/// theory, building explicit identity-on-names inclusion morphisms
/// `i1: shared → t1` and `i2: shared → t2` from the `shared` argument.
///
/// Returns the full [`ColimitResult`] including pushout inclusions,
/// so callers can verify the universal property via
/// [`ColimitResult::verify_universal`].
///
/// # Errors
///
/// Same as [`colimit`].
pub fn pushout_by_name(
    t1: &Theory,
    t2: &Theory,
    shared: &Theory,
) -> Result<ColimitResult, GatError> {
    let i1 = identity_inclusion(shared, t1, "i1")?;
    let i2 = identity_inclusion(shared, t2, "i2")?;
    colimit(t1, t2, &i1, &i2)
}

fn identity_inclusion(
    shared: &Theory,
    target: &Theory,
    label: &str,
) -> Result<TheoryMorphism, GatError> {
    let mut sort_map = HashMap::new();
    for sort in &shared.sorts {
        if !target.has_sort(&sort.name) {
            return Err(GatError::MissingSortMapping(format!(
                "{label}: shared sort `{}` is not present in target theory `{}`",
                sort.name, target.name,
            )));
        }
        sort_map.insert(Arc::clone(&sort.name), Arc::clone(&sort.name));
    }
    let mut op_map = HashMap::new();
    for op in &shared.ops {
        if !target.has_op(&op.name) {
            return Err(GatError::MissingOpMapping(format!(
                "{label}: shared op `{}` is not present in target theory `{}`",
                op.name, target.name,
            )));
        }
        op_map.insert(Arc::clone(&op.name), Arc::clone(&op.name));
    }
    Ok(TheoryMorphism::new(
        format!("{label}_{}_into_{}", shared.name, target.name),
        Arc::clone(&shared.name),
        Arc::clone(&target.name),
        sort_map,
        op_map,
    ))
}

/// Theory-only convenience wrapper around [`pushout_by_name`].
///
/// Returns the pushout's underlying [`Theory`]; callers that need
/// the inclusion morphisms or want to verify the universal property
/// should use [`pushout_by_name`] directly.
///
/// # Errors
///
/// Same as [`pushout_by_name`].
pub fn colimit_by_name(t1: &Theory, t2: &Theory, shared: &Theory) -> Result<Theory, GatError> {
    // Start with all sorts from t1.
    let mut sorts = t1.sorts.clone();

    // Add sorts from t2, checking for conflicts.
    // Use the theory's O(1) index for lookups instead of building separate HashSets.
    for sort in &t2.sorts {
        if t1.has_sort(&sort.name) {
            // Present in both; must be identical or shared.
            if shared.has_sort(&sort.name) {
                // Shared sort: already included via t1, skip.
                continue;
            }
            // Both define it independently; check compatibility.
            let t1_sort = t1
                .find_sort(&sort.name)
                .ok_or_else(|| GatError::SortConflict {
                    name: sort.name.to_string(),
                })?;
            if t1_sort.params != sort.params || t1_sort.kind != sort.kind {
                return Err(GatError::SortConflict {
                    name: sort.name.to_string(),
                });
            }
            // Compatible duplicate; already included.
        } else {
            sorts.push(sort.clone());
        }
    }

    // Same for operations.
    let mut ops = t1.ops.clone();

    for op in &t2.ops {
        if t1.has_op(&op.name) {
            if shared.has_op(&op.name) {
                continue;
            }
            let t1_op = t1.find_op(&op.name).ok_or_else(|| GatError::OpConflict {
                name: op.name.to_string(),
            })?;
            if !signatures_equivalent_modulo_param_rename(
                &t1_op.inputs,
                &t1_op.output,
                &op.inputs,
                &op.output,
            ) {
                return Err(GatError::OpConflict {
                    name: op.name.to_string(),
                });
            }
        } else {
            ops.push(op.clone());
        }
    }

    // Same for equations.
    let mut eqs = t1.eqs.clone();

    for eq in &t2.eqs {
        if let Some(t1_eq) = t1.find_eq(&eq.name) {
            if shared.find_eq(&eq.name).is_some() {
                continue;
            }
            if !alpha_equivalent_equation(&t1_eq.lhs, &t1_eq.rhs, &eq.lhs, &eq.rhs) {
                return Err(GatError::EqConflict {
                    name: eq.name.to_string(),
                });
            }
        } else {
            eqs.push(eq.clone());
        }
    }

    // Same for directed equations.
    let mut directed_eqs = t1.directed_eqs.clone();

    for de in &t2.directed_eqs {
        if let Some(t1_de) = t1.find_directed_eq(&de.name) {
            if shared.find_directed_eq(&de.name).is_some() {
                continue;
            }
            if !alpha_equivalent_equation(&t1_de.lhs, &t1_de.rhs, &de.lhs, &de.rhs) {
                return Err(GatError::DirectedEqConflict {
                    name: de.name.to_string(),
                });
            }
        } else {
            directed_eqs.push(de.clone());
        }
    }

    // Same for conflict policies.
    let mut policies = t1.policies.clone();

    for pol in &t2.policies {
        if let Some(t1_pol) = t1.find_policy(&pol.name) {
            if shared.find_policy(&pol.name).is_some() {
                continue;
            }
            if t1_pol.value_kind != pol.value_kind || t1_pol.strategy != pol.strategy {
                return Err(GatError::PolicyConflict {
                    name: pol.name.to_string(),
                });
            }
        } else {
            policies.push(pol.clone());
        }
    }

    let name: Arc<str> = format!("{}_{}_colimit", t1.name, t2.name).into();
    Ok(Theory::full(
        name,
        Vec::new(),
        sorts,
        ops,
        eqs,
        directed_eqs,
        policies,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::eq::{Equation, Term};
    use crate::op::Operation;
    use crate::sort::{Sort, SortParam};

    #[test]
    fn graph_constraint_colimit() {
        // Shared: just Vertex.
        let shared = Theory::new(
            "ThVertex",
            vec![Sort::simple("Vertex")],
            Vec::new(),
            Vec::new(),
        );

        // ThGraph: Vertex + Edge, ops src/tgt.
        let th_graph = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            Vec::new(),
        );

        // ThConstraint: Vertex + Constraint, op target.
        let th_constraint = Theory::new(
            "ThConstraint",
            vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            Vec::new(),
        );

        let result = colimit_by_name(&th_graph, &th_constraint, &shared).unwrap();

        assert_eq!(&*result.name, "ThGraph_ThConstraint_colimit");
        assert_eq!(result.sorts.len(), 3); // Vertex, Edge, Constraint
        assert_eq!(result.ops.len(), 3); // src, tgt, target

        assert!(result.find_sort("Vertex").is_some());
        assert!(result.find_sort("Edge").is_some());
        assert!(result.find_sort("Constraint").is_some());
        assert!(result.find_op("src").is_some());
        assert!(result.find_op("tgt").is_some());
        assert!(result.find_op("target").is_some());
    }

    #[test]
    fn sort_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::new("T1", vec![Sort::simple("X")], Vec::new(), Vec::new());
        let t2 = Theory::new(
            "T2",
            vec![Sort::dependent("X", vec![SortParam::new("a", "S")])],
            Vec::new(),
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::SortConflict { .. })));
    }

    #[test]
    fn op_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "A", "B")],
            Vec::new(),
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A"), Sort::simple("B")],
            vec![Operation::unary("f", "x", "B", "A")], // reversed
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::OpConflict { .. })));
    }

    #[test]
    fn eq_conflict_detected() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // t1: ax says x = y (two distinct variables).
        // t2: ax says x = x (one variable, used twice).
        // These are NOT alpha-equivalent since the variable multiplicity differs.
        let t1 = Theory::new(
            "T1",
            Vec::new(),
            Vec::new(),
            vec![Equation::new("ax", Term::var("x"), Term::var("y"))],
        );
        let t2 = Theory::new(
            "T2",
            Vec::new(),
            Vec::new(),
            vec![Equation::new("ax", Term::var("a"), Term::var("a"))],
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::EqConflict { .. })));
    }

    #[test]
    fn alpha_equivalent_eqs_not_conflicted() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Same equation with different variable names: should NOT conflict.
        let t1 = Theory::new(
            "T1",
            Vec::new(),
            Vec::new(),
            vec![Equation::new("ax", Term::var("x"), Term::var("y"))],
        );
        let t2 = Theory::new(
            "T2",
            Vec::new(),
            Vec::new(),
            vec![Equation::new("ax", Term::var("a"), Term::var("b"))],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.eqs.len(), 1);
    }

    #[test]
    fn compatible_non_shared_duplicates_allowed() {
        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        // Both define identical sort X.
        let t1 = Theory::new("T1", vec![Sort::simple("X")], Vec::new(), Vec::new());
        let t2 = Theory::new("T2", vec![Sort::simple("X")], Vec::new(), Vec::new());

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.sorts.len(), 1);
    }

    #[test]
    fn colimit_merges_directed_eqs() {
        use crate::eq::{DirectedEquation, Term};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule1",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![
                Operation::unary("f", "x", "A", "A"),
                Operation::nullary("c", "A"),
            ],
            Vec::new(),
            vec![DirectedEquation::new(
                "rule2",
                Term::app("f", vec![Term::constant("c")]),
                Term::constant("c"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.directed_eqs.len(), 2);
        assert!(result.find_directed_eq("rule1").is_some());
        assert!(result.find_directed_eq("rule2").is_some());
    }

    #[test]
    fn colimit_shared_directed_eq_not_duplicated() {
        use crate::eq::{DirectedEquation, Term};

        let de = DirectedEquation::new(
            "shared_rule",
            Term::app("f", vec![Term::var("x")]),
            Term::var("x"),
            panproto_expr::Expr::Var("_".into()),
        );

        let shared = Theory::full(
            "Shared",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![de.clone()],
            Vec::new(),
        );

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![de.clone()],
            Vec::new(),
        );
        let t2 = Theory::full(
            "T2",
            Vec::new(),
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            Vec::new(),
            vec![de],
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.directed_eqs.len(), 1);
    }

    #[test]
    fn colimit_directed_eq_conflict() {
        use crate::eq::{DirectedEquation, Term};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::var("x"),
                Term::var("y"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![DirectedEquation::new(
                "rule",
                Term::constant("a"),
                Term::constant("b"),
                panproto_expr::Expr::Var("_".into()),
            )],
            Vec::new(),
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::DirectedEqConflict { .. })));
    }

    #[test]
    fn colimit_merges_policies() {
        use crate::sort::ValueKind;
        use crate::theory::{ConflictPolicy, ConflictStrategy};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p1".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepLeft,
            }],
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p2".into(),
                value_kind: ValueKind::Int,
                strategy: ConflictStrategy::Fail,
            }],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.policies.len(), 2);
        assert!(result.find_policy("p1").is_some());
        assert!(result.find_policy("p2").is_some());
    }

    #[test]
    fn colimit_policy_conflict() {
        use crate::sort::ValueKind;
        use crate::theory::{ConflictPolicy, ConflictStrategy};

        let shared = Theory::new("Empty", Vec::new(), Vec::new(), Vec::new());

        let t1 = Theory::full(
            "T1",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepLeft,
            }],
        );

        let t2 = Theory::full(
            "T2",
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ConflictPolicy {
                name: "p".into(),
                value_kind: ValueKind::Str,
                strategy: ConflictStrategy::KeepRight, // Different strategy
            }],
        );

        let result = colimit_by_name(&t1, &t2, &shared);
        assert!(matches!(result, Err(GatError::PolicyConflict { .. })));
    }

    #[test]
    fn colimit_equations_with_renamed_ops() {
        // Shared theory S has sort A and op f: A → A with equation e: f(f(x)) = f(x).
        // T1 keeps the names as-is.
        // T2 renames f → g but has the same equation.
        // Morphisms: i1 maps f→f; i2 maps f→g.
        // The colimit should identify them and the equation should NOT conflict.
        let _shared = Theory::new(
            "Shared",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
            )],
        );

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("A")],
            vec![Operation::unary("f", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("f", vec![Term::app("f", vec![Term::var("x")])]),
                Term::app("f", vec![Term::var("x")]),
            )],
        );

        // T2 renames f → g
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("A")],
            vec![Operation::unary("g", "x", "A", "A")],
            vec![Equation::new(
                "idem",
                Term::app("g", vec![Term::app("g", vec![Term::var("x")])]),
                Term::app("g", vec![Term::var("x")]),
            )],
        );

        // Morphisms from Shared into T1 and T2.
        let i1 = TheoryMorphism::new(
            "i1",
            "Shared",
            "T1",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([(Arc::from("f"), Arc::from("f"))]),
        );
        let i2 = TheoryMorphism::new(
            "i2",
            "Shared",
            "T2",
            HashMap::from([(Arc::from("A"), Arc::from("A"))]),
            HashMap::from([(Arc::from("f"), Arc::from("g"))]),
        );

        let result = colimit(&t1, &t2, &i1, &i2).unwrap();
        // The equation should be included exactly once (g renamed to f).
        assert_eq!(result.theory.eqs.len(), 1);
        assert!(result.theory.find_eq("idem").is_some());
        // The pushout should have op f (from T1's naming convention).
        assert!(result.theory.find_op("f").is_some());
        // g should NOT appear as a separate op (it was renamed to f).
        assert!(result.theory.find_op("g").is_none());
    }

    #[test]
    fn shared_declarations_not_duplicated() {
        let shared = Theory::new(
            "Shared",
            vec![Sort::simple("S")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::var("x"), Term::var("x"))],
        );

        let t1 = Theory::new(
            "T1",
            vec![Sort::simple("S"), Sort::simple("A")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::var("x"), Term::var("x"))],
        );
        let t2 = Theory::new(
            "T2",
            vec![Sort::simple("S"), Sort::simple("B")],
            vec![Operation::nullary("c", "S")],
            vec![Equation::new("e", Term::var("x"), Term::var("x"))],
        );

        let result = colimit_by_name(&t1, &t2, &shared).unwrap();
        assert_eq!(result.sorts.len(), 3); // S, A, B
        assert_eq!(result.ops.len(), 1); // c
        assert_eq!(result.eqs.len(), 1); // e
    }

    // --- proptest strategies and property tests ---

    mod property {
        use super::*;
        use proptest::prelude::*;

        /// Generate a colimit input: shared theory with 1-2 sorts, extended
        /// independently to T1 and T2 with 1-2 additional sorts/ops each.
        fn arb_colimit_input() -> impl Strategy<Value = (Theory, Theory, Theory)> {
            // Shared: 1-2 sorts, no ops
            let shared_sort_count = 1..=2usize;
            shared_sort_count
                .prop_flat_map(|n| {
                    let shared_sorts: Vec<Sort> =
                        (0..n).map(|i| Sort::simple(format!("Shared{i}"))).collect();
                    let shared = Theory::new("Shared", shared_sorts, Vec::new(), Vec::new());

                    // T1: shared sorts + 1-2 extra sorts + 0-2 ops
                    let extra1_count = 1..=2usize;
                    let extra2_count = 1..=2usize;
                    let op1_count = 0..=2usize;
                    let op2_count = 0..=2usize;
                    (
                        Just(shared),
                        extra1_count,
                        extra2_count,
                        op1_count,
                        op2_count,
                    )
                })
                .prop_map(|(shared, extra1, extra2, ops1, ops2)| {
                    let mut sorts1 = shared.sorts.clone();
                    for i in 0..extra1 {
                        sorts1.push(Sort::simple(format!("T1Extra{i}")));
                    }
                    let mut t1_ops = Vec::new();
                    for i in 0..std::cmp::min(ops1, sorts1.len()) {
                        t1_ops.push(Operation::unary(
                            format!("t1op{i}"),
                            "x",
                            &*sorts1[i % sorts1.len()].name,
                            &*sorts1[0].name,
                        ));
                    }
                    let t1 = Theory::new("T1", sorts1, t1_ops, Vec::new());

                    let mut sorts2 = shared.sorts.clone();
                    for i in 0..extra2 {
                        sorts2.push(Sort::simple(format!("T2Extra{i}")));
                    }
                    let mut t2_ops = Vec::new();
                    for i in 0..std::cmp::min(ops2, sorts2.len()) {
                        t2_ops.push(Operation::unary(
                            format!("t2op{i}"),
                            "x",
                            &*sorts2[i % sorts2.len()].name,
                            &*sorts2[0].name,
                        ));
                    }
                    let t2 = Theory::new("T2", sorts2, t2_ops, Vec::new());

                    (shared, t1, t2)
                })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            #[test]
            fn colimit_contains_all_sorts((shared, t1, t2) in arb_colimit_input()) {
                let result = colimit_by_name(&t1, &t2, &shared).unwrap();
                // All sorts from T1 should be in the colimit.
                for sort in &t1.sorts {
                    prop_assert!(
                        result.find_sort(&sort.name).is_some(),
                        "T1 sort {:?} missing from colimit",
                        sort.name,
                    );
                }
                // All sorts from T2 should be in the colimit.
                for sort in &t2.sorts {
                    prop_assert!(
                        result.find_sort(&sort.name).is_some(),
                        "T2 sort {:?} missing from colimit",
                        sort.name,
                    );
                }
            }

            #[test]
            fn colimit_contains_all_ops((shared, t1, t2) in arb_colimit_input()) {
                let result = colimit_by_name(&t1, &t2, &shared).unwrap();
                for op in &t1.ops {
                    prop_assert!(
                        result.find_op(&op.name).is_some(),
                        "T1 op {:?} missing from colimit",
                        op.name,
                    );
                }
                for op in &t2.ops {
                    prop_assert!(
                        result.find_op(&op.name).is_some(),
                        "T2 op {:?} missing from colimit",
                        op.name,
                    );
                }
            }

            #[test]
            fn colimit_shared_not_duplicated((shared, t1, t2) in arb_colimit_input()) {
                let result = colimit_by_name(&t1, &t2, &shared).unwrap();
                // Each shared sort appears exactly once.
                for sort in &shared.sorts {
                    let count = result.sorts.iter().filter(|s| s.name == sort.name).count();
                    prop_assert_eq!(count, 1, "shared sort {:?} duplicated", sort.name);
                }
            }

            #[test]
            fn colimit_is_commutative((shared, t1, t2) in arb_colimit_input()) {
                let result_12 = colimit_by_name(&t1, &t2, &shared).unwrap();
                let result_21 = colimit_by_name(&t2, &t1, &shared).unwrap();
                prop_assert_eq!(
                    result_12.sorts.len(),
                    result_21.sorts.len(),
                    "commutative: same sort count",
                );
                prop_assert_eq!(
                    result_12.ops.len(),
                    result_21.ops.len(),
                    "commutative: same op count",
                );
            }

            /// Universal property: identity inclusions of `shared`
            /// give the trivial cocone `(shared, shared)`, and the
            /// identity-on-pushout cocone `(P, j1, j2)` factors through
            /// the pushout itself via the identity mediator.
            #[test]
            fn pushout_universal_property_identity_cocone(
                (shared, t1, t2) in arb_colimit_input()
            ) {
                let result = pushout_by_name(&t1, &t2, &shared).unwrap();
                // The pushout itself with its inclusions is the canonical cocone.
                // The mediator into the pushout from itself must be the identity.
                let m = result
                    .verify_universal(&result.theory, &result.inclusion1, &result.inclusion2)
                    .expect("universal property: identity cocone factors");
                // The identity mediator sends every sort/op to itself.
                for (k, v) in &m.sort_map {
                    prop_assert_eq!(k, v, "identity mediator must be identity on sorts");
                }
                for (k, v) in &m.op_map {
                    prop_assert_eq!(k, v, "identity mediator must be identity on ops");
                }
            }
        }
    }

    /// Verify the universal property end-to-end on a concrete graph
    /// example: an alternative cocone landing in a larger theory must
    /// factor uniquely through the pushout.
    #[test]
    fn pushout_universal_property_graph_constraint() {
        use crate::sort::Sort;

        let shared = Theory::new(
            "ThVertex",
            vec![Sort::simple("Vertex")],
            Vec::new(),
            Vec::new(),
        );
        let th_graph = Theory::new(
            "ThGraph",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![Operation::unary("src", "e", "Edge", "Vertex")],
            Vec::new(),
        );
        let th_constraint = Theory::new(
            "ThConstraint",
            vec![Sort::simple("Vertex"), Sort::simple("Constraint")],
            vec![Operation::unary("target", "c", "Constraint", "Vertex")],
            Vec::new(),
        );
        let pushout = pushout_by_name(&th_graph, &th_constraint, &shared).unwrap();

        // Build an alternative cocone into a larger theory `Q` that
        // contains every sort/op of the pushout plus some extra
        // material.
        let q = Theory::new(
            "Q",
            vec![
                Sort::simple("Vertex"),
                Sort::simple("Edge"),
                Sort::simple("Constraint"),
                Sort::simple("Extra"),
            ],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("target", "c", "Constraint", "Vertex"),
            ],
            Vec::new(),
        );
        let mut k1_sort_map = HashMap::new();
        k1_sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
        k1_sort_map.insert(Arc::from("Edge"), Arc::from("Edge"));
        let mut k1_op_map = HashMap::new();
        k1_op_map.insert(Arc::from("src"), Arc::from("src"));
        let k1 = TheoryMorphism::new("k1", "ThGraph", "Q", k1_sort_map, k1_op_map);

        let mut k2_sort_map = HashMap::new();
        k2_sort_map.insert(Arc::from("Vertex"), Arc::from("Vertex"));
        k2_sort_map.insert(Arc::from("Constraint"), Arc::from("Constraint"));
        let mut k2_op_map = HashMap::new();
        k2_op_map.insert(Arc::from("target"), Arc::from("target"));
        let k2 = TheoryMorphism::new("k2", "ThConstraint", "Q", k2_sort_map, k2_op_map);

        let mediator = pushout
            .verify_universal(&q, &k1, &k2)
            .expect("alternative cocone factors through pushout");
        // Mediator must send every shared name to its corresponding Q name.
        assert_eq!(
            mediator.sort_map.get("Vertex").map(AsRef::as_ref),
            Some("Vertex")
        );
        assert_eq!(
            mediator.sort_map.get("Edge").map(AsRef::as_ref),
            Some("Edge")
        );
        assert_eq!(
            mediator.sort_map.get("Constraint").map(AsRef::as_ref),
            Some("Constraint")
        );
    }
}
