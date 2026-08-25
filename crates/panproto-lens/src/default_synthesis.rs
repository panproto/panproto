//! Compile defaults for newly added schema vertices into executable field transforms.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::FieldTransform;
use panproto_inst::value::Value;
use panproto_schema::{Edge, Schema};

use crate::Lens;
use crate::error::LensError;
use crate::protolens::ComplementConstructor;

/// Attach defaults to the nearest source-owned parent as executable
/// [`FieldTransform::AddField`] operations.
///
/// Defaults resolve against a newly added target vertex in this order:
/// its full vertex id, the label of an eligible incoming edge, then its
/// vertex kind. The target schema supplies the placement information that a
/// bare sort default lacks. A default is executable only when the added
/// vertex has one named incoming edge from a vertex represented in the
/// source instance.
#[allow(
    clippy::too_many_lines,
    reason = "the ordered default-resolution policy is clearer as one linear pass"
)]
pub fn attach_defaults(
    lens: &mut Lens,
    target_schema: &Schema,
    defaults: &HashMap<Name, Value>,
    require_all: bool,
) -> Result<(), LensError> {
    if defaults.is_empty() {
        return Ok(());
    }

    let remapped_targets: HashSet<&Name> = lens.compiled.vertex_remap.values().collect();
    let mut added_vertices: Vec<_> = target_schema
        .vertices
        .iter()
        .filter(|(id, _)| {
            !lens.src_schema.vertices.contains_key(*id) && !remapped_targets.contains(*id)
        })
        .collect();
    added_vertices.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    // Edge-label keys are shorthand, so they are safe only when the label
    // identifies one added target vertex across the entire migration. Count
    // distinct target vertices rather than edges: parallel edges into one
    // vertex are diagnosed separately below.
    let mut label_targets: HashMap<Name, HashSet<Name>> = HashMap::new();
    for (vertex_id, _) in &added_vertices {
        for edge in target_schema.incoming_edges(vertex_id) {
            if source_anchor_for_target(lens, &edge.src).is_some()
                && let Some(label) = &edge.name
                && defaults.contains_key(label)
            {
                label_targets
                    .entry(label.clone())
                    .or_default()
                    .insert((*vertex_id).clone());
            }
        }
    }

    let mut used = HashSet::new();
    let mut additions: Vec<(Name, String, Value)> = Vec::new();

    for (vertex_id, vertex) in added_vertices {
        let mut eligible: Vec<(&Edge, Name)> = target_schema
            .incoming_edges(vertex_id)
            .iter()
            .filter_map(|edge| {
                source_anchor_for_target(lens, &edge.src).map(|anchor| (edge, anchor))
            })
            .collect();
        eligible.sort_by(|a, b| a.0.cmp(b.0));

        let exact = defaults
            .get_key_value(vertex_id)
            .map(|(key, value)| (key, value, None));
        let by_edge: Vec<_> = eligible
            .iter()
            .filter_map(|(edge, _)| {
                edge.name
                    .as_ref()
                    .and_then(|name| defaults.get_key_value(name))
                    .map(|(key, value)| (key, value, Some(*edge)))
            })
            .collect();
        let by_kind = defaults
            .get_key_value(&vertex.kind)
            .map(|(key, value)| (key, value, None));

        let selected = if let Some(exact) = exact {
            Some(exact)
        } else if by_edge.len() == 1 {
            let candidate = by_edge.into_iter().next();
            let globally_ambiguous = candidate.is_some_and(|(key, _, _)| {
                label_targets
                    .get(key)
                    .is_some_and(|targets| targets.len() > 1)
            });
            if globally_ambiguous {
                if !require_all {
                    continue;
                }
                return Err(default_error(&format!(
                    "edge-label default for added vertex `{vertex_id}` also matches another added target vertex"
                )));
            }
            candidate
        } else if by_edge.len() > 1 {
            if !require_all {
                continue;
            }
            return Err(default_error(&format!(
                "default for added vertex `{vertex_id}` matches more than one incoming edge"
            )));
        } else {
            by_kind
        };

        let Some((default_key, value, selected_edge)) = selected else {
            continue;
        };

        if !panproto_inst::attributes::kind_accepts(&vertex.kind, value) {
            return Err(default_error(&format!(
                "default `{default_key}` for added vertex `{vertex_id}` has value kind `{}`, \
                 but the target vertex kind is `{}`",
                value.type_name(),
                vertex.kind
            )));
        }

        let (edge, source_anchor) = if let Some(selected_edge) = selected_edge {
            eligible
                .iter()
                .find(|(edge, _)| *edge == selected_edge)
                .map(|(edge, anchor)| (*edge, anchor.clone()))
                .ok_or_else(|| {
                    default_error(&format!(
                        "default for added vertex `{vertex_id}` lost its target edge"
                    ))
                })?
        } else {
            match eligible.as_slice() {
                [(edge, anchor)] => (*edge, anchor.clone()),
                [] => {
                    if !require_all {
                        continue;
                    }
                    return Err(default_error(&format!(
                        "default `{default_key}` for added vertex `{vertex_id}` cannot be placed: \
                         the vertex has no incoming edge from the source instance"
                    )));
                }
                _ => {
                    if !require_all {
                        continue;
                    }
                    return Err(default_error(&format!(
                        "default `{default_key}` for added vertex `{vertex_id}` is ambiguous \
                         across {} incoming edges",
                        eligible.len()
                    )));
                }
            }
        };

        let Some(field_name) = edge.name.as_ref() else {
            if !require_all {
                continue;
            }
            return Err(default_error(&format!(
                "default `{default_key}` for added vertex `{vertex_id}` cannot be placed on an \
                 unnamed incoming edge"
            )));
        };

        used.insert(default_key.clone());
        additions.push((source_anchor, field_name.to_string(), value.clone()));
    }

    if require_all {
        let mut unused: Vec<_> = defaults
            .keys()
            .filter(|key| !used.contains(*key))
            .map(ToString::to_string)
            .collect();
        unused.sort();
        if !unused.is_empty() {
            return Err(default_error(&format!(
                "defaults did not match an added target vertex, incoming field, or vertex kind: {}",
                unused.join(", ")
            )));
        }
    }

    for (anchor, key, value) in additions {
        let transforms = lens
            .compiled
            .field_transforms
            .entry(anchor.clone())
            .or_default();
        if let Some(existing) = transforms.iter().find_map(|transform| match transform {
            FieldTransform::AddField {
                key: existing_key,
                value,
            } if existing_key == &key => Some(value),
            _ => None,
        }) {
            if existing != &value {
                return Err(default_error(&format!(
                    "conflicting defaults target field `{key}` on source vertex `{anchor}`"
                )));
            }
            continue;
        }
        transforms.push(FieldTransform::AddField { key, value });
    }

    Ok(())
}

/// Collect literal defaults retained by a protolens complement constructor.
pub fn constructor_defaults(
    constructor: &ComplementConstructor,
) -> Result<HashMap<Name, Value>, LensError> {
    fn collect(
        constructor: &ComplementConstructor,
        defaults: &mut HashMap<Name, Value>,
    ) -> Result<(), LensError> {
        match constructor {
            ComplementConstructor::AddedElement {
                element_name,
                default_value: Some(value),
                ..
            } => {
                if let Some(existing) = defaults.get(element_name)
                    && existing != value
                {
                    return Err(default_error(&format!(
                        "conflicting defaults were retained for added element `{element_name}`"
                    )));
                }
                defaults.insert(element_name.clone(), value.clone());
            }
            ComplementConstructor::Composite(parts) => {
                for part in parts {
                    collect(part, defaults)?;
                }
            }
            ComplementConstructor::Scoped { inner, .. } => collect(inner, defaults)?,
            _ => {}
        }
        Ok(())
    }

    let mut defaults = HashMap::new();
    collect(constructor, &mut defaults)?;
    Ok(defaults)
}

fn source_anchor_for_target(lens: &Lens, target: &Name) -> Option<Name> {
    if lens.src_schema.vertices.contains_key(target)
        && lens.compiled.surviving_verts.contains(target)
    {
        return Some(target.clone());
    }
    lens.compiled
        .vertex_remap
        .iter()
        .find_map(|(source, mapped)| {
            (mapped == target && lens.compiled.surviving_verts.contains(source))
                .then(|| source.clone())
        })
}

fn default_error(detail: &str) -> LensError {
    LensError::ProtolensError(format!("cannot compile supplied defaults: {detail}"))
}
