//! Round-trip law verification for lenses.
//!
//! Two laws characterize well-behaved lenses:
//! - **`GetPut`**: `put(s, get(s)) = s` — round-tripping with an unmodified
//!   view recovers the original source.
//! - **`PutGet`**: `get(put(s, v)) = v` — what you put is what you get back.

use crate::Lens;
use crate::asymmetric::{Complement, get, put};
use crate::error::LawViolation;

use panproto_inst::WInstance;

/// Verify both `GetPut` and `PutGet` laws on a specific instance.
///
/// # Errors
///
/// Returns [`LawViolation::GetPut`] if the round-trip fails, or
/// [`LawViolation::PutGet`] if the put-get cycle fails, or
/// `LawViolation::Error` if an operational error occurs.
pub fn check_laws(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    // GetPut: put(s, get(s)) should recover s
    let (view, complement) = get(lens, instance).map_err(LawViolation::Error)?;
    let restored = put(lens, &view, &complement).map_err(LawViolation::Error)?;

    if !instances_equivalent(instance, &restored) {
        return Err(LawViolation::GetPut {
            detail: format!(
                "original has {} nodes and {} arcs, restored has {} nodes and {} arcs",
                instance.node_count(),
                instance.arc_count(),
                restored.node_count(),
                restored.arc_count(),
            ),
        });
    }

    // PutGet: get(put(s, v, c)) should return v (for arbitrary v).
    // Test with original view.
    check_put_get_with_view(lens, &view, &complement)?;

    // Test with a modified view.
    let modified_view = modify_leaf_values(&view);
    if !instances_equivalent(&view, &modified_view) {
        check_put_get_with_view(lens, &modified_view, &complement)?;
    }

    Ok(())
}

/// Check if two instances are structurally equivalent.
///
/// Since `WInstance` does not derive `PartialEq`, we compare structural
/// properties: node count, arc count, root, schema root, and node anchors.
pub(crate) fn instances_equivalent(a: &WInstance, b: &WInstance) -> bool {
    if a.root != b.root || a.schema_root != b.schema_root {
        return false;
    }

    if a.node_count() != b.node_count() || a.arc_count() != b.arc_count() {
        return false;
    }

    // Check that all node IDs match and anchors are the same
    for (&id, node_a) in &a.nodes {
        match b.nodes.get(&id) {
            Some(node_b) => {
                if node_a.anchor != node_b.anchor {
                    return false;
                }
                // Compare values
                if node_a.value != node_b.value {
                    return false;
                }
            }
            None => return false,
        }
    }

    // Compare arcs (order-independent): sort by (parent, child, edge) then compare.
    let mut arcs_a: Vec<_> = a.arcs.clone();
    let mut arcs_b: Vec<_> = b.arcs.clone();
    arcs_a.sort();
    arcs_b.sort();
    if arcs_a != arcs_b {
        return false;
    }

    // Compare fans (order-independent).
    if a.fans.len() != b.fans.len() {
        return false;
    }
    let mut fans_a: Vec<_> = a.fans.clone();
    let mut fans_b: Vec<_> = b.fans.clone();
    fans_a.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    fans_b.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    if fans_a != fans_b {
        return false;
    }

    true
}

/// Verify only the `GetPut` law.
///
/// # Errors
///
/// Returns [`LawViolation::GetPut`] or [`LawViolation::Error`].
pub fn check_get_put(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    let (view, complement) = get(lens, instance).map_err(LawViolation::Error)?;
    let restored = put(lens, &view, &complement).map_err(LawViolation::Error)?;

    if !instances_equivalent(instance, &restored) {
        return Err(LawViolation::GetPut {
            detail: format!(
                "original has {} nodes, restored has {} nodes",
                instance.node_count(),
                restored.node_count(),
            ),
        });
    }
    Ok(())
}

/// Verify the `PutGet` law: for an arbitrary view `v`,
/// `get(put(s, v, c)) = v`.
///
/// This function tests the law both with the original view (unmodified)
/// and with a modified view that has a changed leaf value, ensuring the
/// law holds for arbitrary views.
///
/// # Errors
///
/// Returns [`LawViolation::PutGet`] or [`LawViolation::Error`].
pub fn check_put_get(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    let (view, complement) = get(lens, instance).map_err(LawViolation::Error)?;

    // Test with original view (identity case).
    check_put_get_with_view(lens, &view, &complement)?;

    // Test with a modified view: change leaf string values to exercise
    // the law with a genuinely different view.
    let modified_view = modify_leaf_values(&view);
    if !instances_equivalent(&view, &modified_view) {
        check_put_get_with_view(lens, &modified_view, &complement)?;
    }

    Ok(())
}

/// Check the `PutGet` law for a specific view: `get(put(s, v, c)) = v`.
fn check_put_get_with_view(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<(), LawViolation> {
    let restored = put(lens, view, complement).map_err(LawViolation::Error)?;
    let (view2, _) = get(lens, &restored).map_err(LawViolation::Error)?;

    if !instances_equivalent(view, &view2) {
        return Err(LawViolation::PutGet {
            detail: format!(
                "view has {} nodes, re-get has {} nodes",
                view.node_count(),
                view2.node_count(),
            ),
        });
    }
    Ok(())
}

/// Create a copy of the instance with leaf string values modified.
fn modify_leaf_values(instance: &WInstance) -> WInstance {
    use panproto_inst::value::{FieldPresence, Value};

    let mut modified = instance.clone();
    for node in modified.nodes.values_mut() {
        if let Some(FieldPresence::Present(Value::Str(ref mut s))) = node.value {
            s.push_str("_modified");
        }
    }
    modified
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    #[test]
    fn identity_lens_satisfies_laws() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_laws(&lens, &instance);
        assert!(
            result.is_ok(),
            "identity lens should satisfy all laws: {result:?}"
        );
    }

    #[test]
    fn identity_lens_satisfies_get_put() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_get_put(&lens, &instance);
        assert!(result.is_ok(), "identity lens should satisfy GetPut");
    }

    #[test]
    fn identity_lens_satisfies_put_get() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_put_get(&lens, &instance);
        assert!(result.is_ok(), "identity lens should satisfy PutGet");
    }

    #[test]
    fn different_arcs_are_not_equivalent() {
        use panproto_schema::Edge;

        let a = three_node_instance();
        let mut b = a.clone();

        // Swap an arc's edge kind in b so arcs differ
        if let Some(arc) = b.arcs.first_mut() {
            arc.2 = Edge {
                src: arc.2.src.clone(),
                tgt: arc.2.tgt.clone(),
                kind: "different_kind".into(),
                name: arc.2.name.clone(),
            };
        }

        assert!(
            !instances_equivalent(&a, &b),
            "instances with different arcs should not be equivalent"
        );
    }
}
