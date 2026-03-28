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
                if node_a.extra_fields != node_b.extra_fields {
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

    // --- proptest strategies and property tests ---

    #[allow(clippy::unwrap_used)]
    mod property {
        use super::*;
        use panproto_gat::Name;
        use panproto_inst::value::{FieldPresence, Value};
        use panproto_inst::{CompiledMigration, Node, WInstance};
        use panproto_schema::{Edge, Schema, Vertex};
        use proptest::prelude::*;
        use smallvec::SmallVec;
        use std::collections::{HashMap, HashSet};

        const LEAF_KINDS: &[&str] = &["string", "integer", "boolean"];

        fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
            let mut vertices = HashMap::new();
            let mut edges = HashMap::new();
            let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
            let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
            let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

            for (id, kind) in verts {
                vertices.insert(
                    Name::from(*id),
                    Vertex {
                        id: Name::from(*id),
                        kind: Name::from(*kind),
                        nsid: None,
                    },
                );
            }
            for e in edge_list {
                edges.insert(e.clone(), e.kind.clone());
                outgoing.entry(e.src.clone()).or_default().push(e.clone());
                incoming.entry(e.tgt.clone()).or_default().push(e.clone());
                between
                    .entry((e.src.clone(), e.tgt.clone()))
                    .or_default()
                    .push(e.clone());
            }

            Schema {
                protocol: "test".into(),
                vertices,
                edges,
                hyper_edges: HashMap::new(),
                constraints: HashMap::new(),
                required: HashMap::new(),
                nsids: HashMap::new(),
                variants: HashMap::new(),
                orderings: HashMap::new(),
                recursion_points: HashMap::new(),
                spans: HashMap::new(),
                usage_modes: HashMap::new(),
                nominal: HashMap::new(),
                coercions: HashMap::new(),
                mergers: HashMap::new(),
                defaults: HashMap::new(),
                policies: HashMap::new(),
                outgoing,
                incoming,
                between,
            }
        }

        /// Generate a random schema + instance + identity lens.
        fn arb_identity_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 1-4 leaf children under a root object.
            (1..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec(
                    prop::sample::select(LEAF_KINDS).prop_map(ToOwned::to_owned),
                    n_children..=n_children,
                )
                .prop_flat_map(move |kinds| {
                    // Generate random string values for each leaf.
                    prop::collection::vec(
                        "[a-z]{1,8}".prop_map(String::from),
                        n_children..=n_children,
                    )
                    .prop_map(move |values| {
                        let kinds = kinds.clone();
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..kinds.len()).map(|i| format!("child{i}")).collect();

                        // Build schema.
                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut edges = Vec::new();
                        for (i, kind) in kinds.iter().enumerate() {
                            vert_specs.push((child_names[i].clone(), kind.clone()));
                            edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(child_names[i].as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(child_names[i].as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let schema = make_schema(&vert_refs, &edges);

                        // Build instance.
                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Build identity lens.
                        let surviving_verts: HashSet<Name> =
                            schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms: HashMap::new(),
                                conditional_survival: HashMap::new(),
                            },
                            src_schema: schema.clone(),
                            tgt_schema: schema,
                        };

                        (lens, instance)
                    })
                })
            })
        }

        /// Generate a projection lens scenario: schema with root + N children,
        /// lens drops one child.
        fn arb_projection_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 2-4 leaf children; we'll drop the last one.
            (2..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n_children..=n_children)
                    .prop_map(move |values| {
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..n_children).map(|i| format!("child{i}")).collect();

                        // Build full schema.
                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut all_edges = Vec::new();
                        for name in &child_names {
                            vert_specs.push((name.clone(), "string".to_owned()));
                            all_edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(name.as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(name.as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let src_schema = make_schema(&vert_refs, &all_edges);

                        // Target schema: drop last child.
                        let tgt_vert_refs: Vec<(&str, &str)> =
                            vert_refs[..vert_refs.len() - 1].to_vec();
                        let tgt_edges: Vec<Edge> = all_edges[..all_edges.len() - 1].to_vec();
                        let tgt_schema = make_schema(&tgt_vert_refs, &tgt_edges);

                        // Build instance.
                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = all_edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Build projection lens.
                        let surviving_verts: HashSet<Name> =
                            tgt_schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> =
                            tgt_schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms: HashMap::new(),
                                conditional_survival: HashMap::new(),
                            },
                            src_schema,
                            tgt_schema,
                        };

                        (lens, instance)
                    })
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn identity_lens_satisfies_laws_proptest(
                (lens, instance) in arb_identity_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "identity lens should satisfy all laws",
                );
            }

            #[test]
            fn projection_lens_satisfies_get_put_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "projection lens should satisfy GetPut",
                );
            }

            #[test]
            fn projection_lens_satisfies_put_get_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_put_get(&lens, &instance).is_ok(),
                    "projection lens should satisfy PutGet",
                );
            }

            #[test]
            fn projection_lens_satisfies_full_laws_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "projection lens should satisfy all laws",
                );
            }
        }
    }
}
