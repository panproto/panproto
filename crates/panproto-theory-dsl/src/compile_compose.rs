//! Compile a [`CompositionBody`] into a composed [`Theory`].
//!
//! Converts the DSL composition spec into GAT [`CompositionSpec`]
//! and replays it via [`panproto_gat::recompose`].

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::{CompositionMode, CompositionSpec, CompositionStep, Theory};

use crate::document::{ColimitStepSpec, CompositionBody, CompositionSpec_};
use crate::error::TheoryDslError;

/// Compile a [`CompositionBody`] into a composed [`Theory`] and its
/// [`CompositionSpec`] recipe.
///
/// Builds a local registry from resolved base theories and any
/// theories already compiled in the same document (passed via
/// `local_theories`), then replays the colimit steps.
///
/// # Errors
///
/// Returns [`TheoryDslError::TheoryNotFound`] if a base theory
/// cannot be resolved, or [`TheoryDslError::ColimitFailed`] if
/// a colimit step fails.
pub fn compile_composition<S: BuildHasher>(
    body: &CompositionBody,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<(Theory, CompositionSpec), TheoryDslError> {
    compile_composition_spec(&body.compose, resolver, local_theories)
}

/// Compile a [`CompositionSpec_`] (the inner spec, usable from both
/// `CompositionBody` and inline `TheoryRef::Composed`).
///
/// # Errors
///
/// Returns [`TheoryDslError::TheoryNotFound`] if a referenced theory
/// cannot be resolved, or [`TheoryDslError::ColimitFailed`] if
/// recomposition fails.
pub fn compile_composition_spec<S: BuildHasher>(
    spec: &CompositionSpec_,
    resolver: &dyn Fn(&str) -> Option<Theory>,
    local_theories: &HashMap<String, Theory, S>,
) -> Result<(Theory, CompositionSpec), TheoryDslError> {
    let mut registry: HashMap<String, Theory> = HashMap::new();

    // Seed registry with local theories.
    for (name, theory) in local_theories {
        registry.insert(name.clone(), theory.clone());
    }

    // Resolve and register base theories.
    for base_name in &spec.bases {
        if !registry.contains_key(base_name) {
            let theory = resolver(base_name).ok_or_else(|| TheoryDslError::TheoryNotFound {
                name: base_name.clone(),
                context: format!("composition '{}' bases", spec.result),
            })?;
            registry.insert(base_name.clone(), theory);
        }
    }

    // Also resolve any theories referenced in steps but not in bases.
    for step in &spec.steps {
        for name in [&step.left, &step.right] {
            if !registry.contains_key(name) && !name.starts_with("step_") {
                if let Some(theory) = resolver(name) {
                    registry.insert(name.clone(), theory);
                }
            }
        }
    }

    // Convert DSL steps to GAT steps.
    let gat_steps: Vec<CompositionStep> = spec.steps.iter().map(convert_step).collect();

    let gat_spec = CompositionSpec {
        result_name: spec.result.clone(),
        version: 0,
        steps: gat_steps,
    };

    let theory = panproto_gat::recompose(&gat_spec, &registry).map_err(|e| {
        TheoryDslError::ColimitFailed {
            step: 0,
            message: e.to_string(),
        }
    })?;

    Ok((theory, gat_spec))
}

fn convert_step(step: &ColimitStepSpec) -> CompositionStep {
    CompositionStep::Colimit {
        left: step.left.clone(),
        right: step.right.clone(),
        shared_sorts: step.shared_sorts.clone(),
        shared_ops: step.shared_ops.clone(),
        mode: CompositionMode::Amalgamate,
    }
}
