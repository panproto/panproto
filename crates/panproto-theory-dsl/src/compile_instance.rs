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

use crate::compile_morphism::refused_entry;
use crate::document::{InstanceSpec, MorphismSpec};
use crate::error::TheoryDslError;
use crate::path::{DocumentPath, PathSegment};

/// Compile an [`InstanceSpec`] into a [`TheoryMorphism`] from its class to
/// its target.
///
/// The class and then the target are resolved; each binding key, in name
/// order, must name a sort of the class (which a sort and an operation of
/// one name binds as the sort) or else an operation of it, and binds it to
/// the target symbol its value names; the morphism is then checked.
///
/// # Errors
///
/// [`TheoryDslError::TheoryNotFound`] at `class` or `target`,
/// [`TheoryDslError::InstanceBinding`] at the least binding key naming no
/// class symbol, and [`TheoryDslError::MorphismCheck`] at the binding it
/// concerns (at `bindings` for a class symbol left unbound, at the instance
/// for an equation the morphism does not preserve).
pub fn compile_instance(
    spec: &InstanceSpec,
    resolver: &dyn Fn(&str) -> Option<Theory>,
) -> Result<TheoryMorphism, TheoryDslError> {
    let root = DocumentPath::root();
    let class = resolver(&spec.class).ok_or_else(|| {
        TheoryDslError::TheoryNotFound {
            name: spec.class.clone(),
            context: format!("instance '{}' class", spec.instance),
        }
        .within(&root.key("class"))
    })?;

    let target = resolver(&spec.target).ok_or_else(|| {
        TheoryDslError::TheoryNotFound {
            name: spec.target.clone(),
            context: format!("instance '{}' target", spec.instance),
        }
        .within(&root.key("target"))
    })?;

    let mut sort_map: HashMap<String, String> = HashMap::new();
    let mut op_map: HashMap<String, String> = HashMap::new();
    let mut keys: Vec<&String> = spec.bindings.keys().collect();
    keys.sort();
    for key in keys {
        let value = spec.bindings[key].clone();
        if class.find_sort(key).is_some() {
            sort_map.insert(key.clone(), value);
        } else if class.find_op(key).is_some() {
            op_map.insert(key.clone(), value);
        } else {
            return Err(TheoryDslError::InstanceBinding {
                instance: spec.instance.clone(),
                class: spec.class.clone(),
                name: key.clone(),
            }
            .within(&root.key("bindings").key(key)));
        }
    }

    let arcs = |map: &HashMap<String, String>| -> HashMap<Arc<str>, Arc<str>> {
        map.iter()
            .map(|(k, v)| (Arc::from(k.as_str()), Arc::from(v.as_str())))
            .collect()
    };
    let morphism = TheoryMorphism::new(
        spec.instance.as_str(),
        spec.class.as_str(),
        spec.target.as_str(),
        arcs(&sort_map),
        arcs(&op_map),
    );

    panproto_gat::check_morphism(&morphism, &class, &target).map_err(|e| {
        let as_morphism = MorphismSpec {
            morphism: spec.instance.clone(),
            domain: spec.class.clone(),
            codomain: spec.target.clone(),
            sort_map,
            op_map,
        };
        let at = binding_path(&refused_entry(&e, &as_morphism, &class));
        TheoryDslError::MorphismCheck {
            morphism: spec.instance.clone(),
            error: e,
        }
        .within(&at)
    })?;

    Ok(morphism)
}

/// The path in an instance document of the morphism map entry at `path`:
/// both maps are the instance's `bindings`.
fn binding_path(path: &DocumentPath) -> DocumentPath {
    let mut segments = path.0.clone();
    if let Some(PathSegment::Key(first)) = segments.first_mut()
        && (first == "sort_map" || first == "op_map")
    {
        "bindings".clone_into(first);
    }
    DocumentPath(segments)
}
