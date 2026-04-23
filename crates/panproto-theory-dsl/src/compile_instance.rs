//! Compile an [`InstanceSpec`] into a [`TheoryMorphism`].
//!
//! An instance declaration desugars to a theory morphism from the class
//! theory to the target theory. The compiler splits the `bindings` map
//! into `sort_map` entries (for each class param name) and `op_map`
//! entries (for each class op name), then validates the morphism via
//! [`panproto_gat::check_morphism`].

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{Theory, TheoryMorphism};

use crate::document::InstanceSpec;
use crate::error::TheoryDslError;

/// Compile an [`InstanceSpec`] into a validated [`TheoryMorphism`].
///
/// Resolves the class and target theories through `resolver`, splits the
/// `bindings` map by classifying each key as either a sort param or an
/// op of the class, constructs the morphism, and validates it via
/// [`panproto_gat::check_morphism`].
///
/// # Errors
///
/// Returns [`TheoryDslError::TheoryNotFound`] if the class or target
/// theory cannot be resolved, [`TheoryDslError::InstanceBinding`] if a
/// binding names neither a class sort nor a class operation, or
/// [`TheoryDslError::MorphismCheck`] if the resulting morphism fails
/// validation.
pub fn compile_instance(
    spec: &InstanceSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<TheoryMorphism, TheoryDslError> {
    let class = resolver(&spec.class).ok_or_else(|| TheoryDslError::TheoryNotFound {
        name: spec.class.clone(),
        context: format!("instance '{}' class", spec.instance),
    })?;

    let target = resolver(&spec.target).ok_or_else(|| TheoryDslError::TheoryNotFound {
        name: spec.target.clone(),
        context: format!("instance '{}' target", spec.instance),
    })?;

    let mut sort_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();
    let mut op_map: HashMap<Arc<str>, Arc<str>> = HashMap::new();

    for (key, value) in &spec.bindings {
        if class.find_sort(key).is_some() {
            sort_map.insert(Arc::from(key.as_str()), Arc::from(value.as_str()));
        } else if class.find_op(key).is_some() {
            op_map.insert(Arc::from(key.as_str()), Arc::from(value.as_str()));
        } else {
            return Err(TheoryDslError::InstanceBinding {
                instance: spec.instance.clone(),
                class: spec.class.clone(),
                name: key.clone(),
            });
        }
    }

    let morphism = TheoryMorphism::new(
        spec.instance.as_str(),
        spec.class.as_str(),
        spec.target.as_str(),
        sort_map,
        op_map,
    );

    panproto_gat::check_morphism(&morphism, &class, &target).map_err(|e| {
        TheoryDslError::MorphismCheck {
            morphism: spec.instance.clone(),
            message: e.to_string(),
        }
    })?;

    Ok(morphism)
}
