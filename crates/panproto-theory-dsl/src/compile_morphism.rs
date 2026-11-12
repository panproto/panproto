//! Compile a [`MorphismSpec`] into a [`TheoryMorphism`].
//!
//! Resolves domain and codomain theories, constructs the morphism,
//! and validates it via [`panproto_gat::check_morphism`].

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{Theory, TheoryMorphism};

use crate::document::MorphismSpec;
use crate::error::TheoryDslError;
use crate::path::DocumentPath;

/// Compile a [`MorphismSpec`] into a validated [`TheoryMorphism`].
///
/// The `resolver` callback looks up theories by name (both from the
/// local document and from external/built-in registries).
///
/// # Errors
///
/// Returns [`TheoryDslError::TheoryNotFound`] if domain or codomain
/// cannot be resolved, [`TheoryDslError::UnknownMapKey`] for the least
/// key (sort map before operation map) that names no symbol of the domain,
/// or [`TheoryDslError::MorphismCheck`] if the morphism fails validation.
pub fn compile_morphism(
    spec: &MorphismSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<TheoryMorphism, TheoryDslError> {
    let root = DocumentPath::root();
    let domain = resolver(&spec.domain).ok_or_else(|| {
        TheoryDslError::TheoryNotFound {
            name: spec.domain.clone(),
            context: format!("morphism '{}' domain", spec.morphism),
        }
        .within(&root.key("domain"))
    })?;

    let codomain = resolver(&spec.codomain).ok_or_else(|| {
        TheoryDslError::TheoryNotFound {
            name: spec.codomain.clone(),
            context: format!("morphism '{}' codomain", spec.morphism),
        }
        .within(&root.key("codomain"))
    })?;

    let unknown = |field: &str, kind: &str, keys: Vec<&String>, declared: &dyn Fn(&str) -> bool| {
        let mut keys = keys;
        keys.sort();
        keys.into_iter().find(|key| !declared(key)).map(|key| {
            TheoryDslError::UnknownMapKey {
                morphism: spec.morphism.clone(),
                kind: kind.to_owned(),
                name: key.clone(),
            }
            .within(&root.key(field).key(key))
        })
    };
    if let Some(error) = unknown("sort_map", "sort", spec.sort_map.keys().collect(), &|key| {
        domain.sorts.iter().any(|s| &*s.name == key)
    }) {
        return Err(error);
    }
    if let Some(error) = unknown("op_map", "op", spec.op_map.keys().collect(), &|key| {
        domain.ops.iter().any(|o| &*o.name == key)
    }) {
        return Err(error);
    }

    let sort_map: HashMap<Arc<str>, Arc<str>> = spec
        .sort_map
        .iter()
        .map(|(k, v)| (Arc::from(k.as_str()), Arc::from(v.as_str())))
        .collect();

    let op_map: HashMap<Arc<str>, Arc<str>> = spec
        .op_map
        .iter()
        .map(|(k, v)| (Arc::from(k.as_str()), Arc::from(v.as_str())))
        .collect();

    let morphism = TheoryMorphism::new(
        spec.morphism.as_str(),
        spec.domain.as_str(),
        spec.codomain.as_str(),
        sort_map,
        op_map,
    );

    panproto_gat::check_morphism(&morphism, &domain, &codomain).map_err(|e| {
        TheoryDslError::MorphismCheck {
            morphism: spec.morphism.clone(),
            error: e,
        }
    })?;

    Ok(morphism)
}
