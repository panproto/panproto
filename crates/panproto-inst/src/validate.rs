//! Validation of W-type instances against schemas.
//!
//! Checks axioms I1-I7 from the theory:
//! - I1: All node anchors exist in the schema
//! - I2: All arc edges exist in the schema
//! - I3: Root exists in node set
//! - I4: All nodes reachable from root
//! - I5: Required edges present
//! - I6: Parent map consistency
//! - I7: Fan validity

use std::collections::{HashSet, VecDeque};

use panproto_schema::Schema;

use crate::error::ValidationError;
use crate::wtype::WInstance;

/// Validate a W-type instance against a schema.
///
/// Checks all structural axioms and returns a list of violations.
/// An empty list means the instance is valid.
#[must_use]
pub fn validate_wtype(schema: &Schema, instance: &WInstance) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // I1: All node anchors exist in the schema
    check_anchors(schema, instance, &mut errors);

    // I2: All arc edges exist in the schema
    check_edges(schema, instance, &mut errors);

    // I3: Root exists in node set
    check_root(instance, &mut errors);

    // I4: All nodes reachable from root
    check_reachability(instance, &mut errors);

    // I5: Required edges present
    check_required_edges(schema, instance, &mut errors);

    // I6: Parent map consistency
    check_parent_map(instance, &mut errors);

    // I7: Fan validity
    check_fans(schema, instance, &mut errors);

    errors
}

/// I1: Verify all node anchors reference vertices in the schema.
fn check_anchors(schema: &Schema, instance: &WInstance, errors: &mut Vec<ValidationError>) {
    for (&id, node) in &instance.nodes {
        if !schema.has_vertex(&node.anchor) {
            errors.push(ValidationError::InvalidAnchor {
                node_id: id,
                anchor: node.anchor.clone(),
            });
        }
    }
}

/// I2: Verify all arc edges exist in the schema.
fn check_edges(schema: &Schema, instance: &WInstance, errors: &mut Vec<ValidationError>) {
    for &(parent, child, ref edge) in &instance.arcs {
        if !schema.edges.contains_key(edge) {
            errors.push(ValidationError::InvalidEdge {
                parent,
                child,
                detail: format!(
                    "edge {} -> {} ({}) not in schema",
                    edge.src, edge.tgt, edge.kind
                ),
            });
        }
    }
}

/// I3: Verify the root node exists.
fn check_root(instance: &WInstance, errors: &mut Vec<ValidationError>) {
    if !instance.nodes.contains_key(&instance.root) {
        errors.push(ValidationError::MissingRoot);
    }
}

/// I4: Verify all nodes are reachable from root via arcs.
fn check_reachability(instance: &WInstance, errors: &mut Vec<ValidationError>) {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();

    if instance.nodes.contains_key(&instance.root) {
        queue.push_back(instance.root);
        reachable.insert(instance.root);
    }

    while let Some(current) = queue.pop_front() {
        for &child in instance.children(current) {
            if reachable.insert(child) {
                queue.push_back(child);
            }
        }
    }

    for &id in instance.nodes.keys() {
        if !reachable.contains(&id) {
            errors.push(ValidationError::UnreachableNode { node_id: id });
        }
    }
}

/// I5: Verify required edges are present for each node.
fn check_required_edges(schema: &Schema, instance: &WInstance, errors: &mut Vec<ValidationError>) {
    for (&node_id, node) in &instance.nodes {
        if let Some(required_edges) = schema.required.get(&node.anchor) {
            for req_edge in required_edges {
                let has_edge = instance.arcs.iter().any(|&(p, _, ref e)| {
                    p == node_id && e.kind == req_edge.kind && e.name == req_edge.name
                });
                if !has_edge {
                    errors.push(ValidationError::MissingRequiredEdge {
                        node_id,
                        edge: format!(
                            "{} ({})",
                            req_edge.name.as_deref().unwrap_or("unnamed"),
                            req_edge.kind
                        ),
                    });
                }
            }
        }
    }
}

/// I6: Verify parent map consistency with arcs.
fn check_parent_map(instance: &WInstance, errors: &mut Vec<ValidationError>) {
    for &(parent, child, _) in &instance.arcs {
        match instance.parent_map.get(&child) {
            Some(&recorded_parent) if recorded_parent != parent => {
                errors.push(ValidationError::ParentMapInconsistent {
                    node_id: child,
                    detail: format!(
                        "arc says parent is {parent}, parent_map says {recorded_parent}"
                    ),
                });
            }
            None => {
                errors.push(ValidationError::ParentMapInconsistent {
                    node_id: child,
                    detail: format!("arc ({parent}, {child}) but child not in parent_map"),
                });
            }
            _ => {} // consistent
        }
    }
}

/// I7: Verify fan validity (all fan nodes exist, hyper-edge exists in schema).
fn check_fans(schema: &Schema, instance: &WInstance, errors: &mut Vec<ValidationError>) {
    for fan in &instance.fans {
        // Check hyper-edge exists in schema
        if !schema.hyper_edges.contains_key(&fan.hyper_edge_id) {
            errors.push(ValidationError::InvalidFan {
                hyper_edge_id: fan.hyper_edge_id.clone(),
                detail: "hyper-edge not in schema".to_string(),
            });
            continue;
        }

        // Check parent node exists
        if !instance.nodes.contains_key(&fan.parent) {
            errors.push(ValidationError::InvalidFan {
                hyper_edge_id: fan.hyper_edge_id.clone(),
                detail: format!("parent node {} not found", fan.parent),
            });
        }

        // Check child nodes exist
        for (label, &child_id) in &fan.children {
            if !instance.nodes.contains_key(&child_id) {
                errors.push(ValidationError::InvalidFan {
                    hyper_edge_id: fan.hyper_edge_id.clone(),
                    detail: format!("child node {child_id} (label: {label}) not found"),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Node;
    use crate::value::{FieldPresence, Value};
    use panproto_schema::Edge;
    use smallvec::smallvec;
    use std::collections::HashMap;

    fn test_schema() -> Schema {
        let mut vertices = HashMap::new();
        vertices.insert(
            "obj".into(),
            panproto_schema::Vertex {
                id: "obj".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        vertices.insert(
            "str1".into(),
            panproto_schema::Vertex {
                id: "str1".into(),
                kind: "string".into(),
                nsid: None,
            },
        );
        vertices.insert(
            "str2".into(),
            panproto_schema::Vertex {
                id: "str2".into(),
                kind: "string".into(),
                nsid: None,
            },
        );

        let e1 = Edge {
            src: "obj".into(),
            tgt: "str1".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        };
        let e2 = Edge {
            src: "obj".into(),
            tgt: "str2".into(),
            kind: "prop".into(),
            name: Some("desc".into()),
        };

        let mut edges = HashMap::new();
        edges.insert(e1.clone(), "prop".into());
        edges.insert(e2.clone(), "prop".into());

        let mut outgoing = HashMap::new();
        outgoing.insert("obj".into(), smallvec![e1, e2]);

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            outgoing,
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn valid_3_node_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "obj"));
        nodes.insert(
            1,
            Node::new(1, "str1").with_value(FieldPresence::Present(Value::Str("Alice".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "str2").with_value(FieldPresence::Present(Value::Str("A person".into()))),
        );

        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "obj".into(),
                    tgt: "str1".into(),
                    kind: "prop".into(),
                    name: Some("name".into()),
                },
            ),
            (
                0,
                2,
                Edge {
                    src: "obj".into(),
                    tgt: "str2".into(),
                    kind: "prop".into(),
                    name: Some("desc".into()),
                },
            ),
        ];

        WInstance::new(nodes, arcs, vec![], 0, "obj".into())
    }

    #[test]
    fn valid_instance_passes_validation() {
        let schema = test_schema();
        let inst = valid_3_node_instance();
        let errors = validate_wtype(&schema, &inst);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn invalid_anchor_detected() {
        let schema = test_schema();
        let mut inst = valid_3_node_instance();
        // Corrupt an anchor
        if let Some(node) = inst.nodes.get_mut(&1) {
            node.anchor = "nonexistent".to_string();
        }
        let errors = validate_wtype(&schema, &inst);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidAnchor { .. }))
        );
    }

    #[test]
    fn missing_root_detected() {
        let schema = test_schema();
        let mut inst = valid_3_node_instance();
        inst.nodes.remove(&0);
        let errors = validate_wtype(&schema, &inst);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::MissingRoot))
        );
    }

    #[test]
    fn unreachable_node_detected() {
        let schema = test_schema();
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "obj"));
        nodes.insert(
            1,
            Node::new(1, "str1").with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        // Node 99 is not connected via any arc
        nodes.insert(
            99,
            Node::new(99, "str2").with_value(FieldPresence::Present(Value::Str("orphan".into()))),
        );

        let arcs = vec![(
            0,
            1,
            Edge {
                src: "obj".into(),
                tgt: "str1".into(),
                kind: "prop".into(),
                name: Some("name".into()),
            },
        )];

        let inst = WInstance::new(nodes, arcs, vec![], 0, "obj".into());
        let errors = validate_wtype(&schema, &inst);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::UnreachableNode { node_id: 99 }))
        );
    }
}
