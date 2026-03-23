//! Declarative query engine for W-type instances.
//!
//! An [`InstanceQuery`] describes a query as a composite of:
//! 1. Anchor selection (which vertex kind to match)
//! 2. Path navigation (follow edges before selecting)
//! 3. Predicate filtering (expression evaluated per node)
//! 4. Grouping (partition results by a field)
//! 5. Projection (select a subset of fields)
//! 6. Limit (truncate results)
//!
//! Queries are schema-typed: the anchor must exist in the schema.
//! Predicates are evaluated via `panproto_expr::eval` with each node's
//! `extra_fields` bound as variables.

use std::collections::HashMap;

use panproto_gat::Name;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::wtype::{WInstance, build_env_from_extra_fields, value_to_expr_literal};

/// A declarative query over a [`WInstance`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceQuery {
    /// Select nodes with this anchor (vertex kind).
    pub anchor: Name,

    /// Optional predicate on node values/fields.
    /// Evaluated in an environment with all `extra_fields` bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<panproto_expr::Expr>,

    /// Optional: group results by this field name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,

    /// Optional: project to these fields only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<Vec<String>>,

    /// Optional: limit results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// Optional: traverse edges before selecting.
    /// Each step follows an edge kind from the current position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<Name>,
}

/// A single match from query execution.
#[derive(Debug, Clone)]
pub struct QueryMatch {
    /// The matched node's ID.
    pub node_id: u32,
    /// The matched node's schema anchor.
    pub anchor: Name,
    /// The matched node's value (if present).
    pub value: Option<FieldPresence>,
    /// The matched node's fields (possibly projected).
    pub fields: FxHashMap<String, Value>,
}

/// Execute a query against a W-type instance.
///
/// Pipeline: anchor filter → path navigation → predicate evaluation
/// → limit → `group_by` → field projection.
///
/// The `schema` parameter provides schema context for future
/// schema-aware operations and instance-aware evaluation.
#[must_use]
pub fn execute(
    query: &InstanceQuery,
    instance: &WInstance,
    _schema: &panproto_schema::Schema,
) -> Vec<QueryMatch> {
    let eval_config = panproto_expr::EvalConfig::default();

    // 1. Find all nodes matching the anchor.
    let candidates: Vec<u32> = instance
        .nodes
        .iter()
        .filter(|(_, n)| n.anchor == query.anchor)
        .map(|(id, _)| *id)
        .collect();

    // 2. Follow path if specified.
    let navigated = if query.path.is_empty() {
        candidates
    } else {
        navigate_path(instance, &candidates, &query.path)
    };

    // 3. Apply predicate (instance-aware evaluation for graph builtins).
    let filtered = if let Some(ref pred) = query.predicate {
        navigated
            .into_iter()
            .filter(|&id| {
                let Some(node) = instance.nodes.get(&id) else {
                    return false;
                };
                let env = build_node_env(node, instance);
                matches!(
                    crate::instance_env::eval_with_instance(
                        pred,
                        &env,
                        &eval_config,
                        instance,
                        Some(id),
                    ),
                    Ok(panproto_expr::Literal::Bool(true))
                )
            })
            .collect()
    } else {
        navigated
    };

    // 4. Apply limit.
    let limited: Vec<u32> = if let Some(limit) = query.limit {
        filtered.into_iter().take(limit).collect()
    } else {
        filtered
    };

    // 5. Build results with optional projection.
    let mut results: Vec<QueryMatch> = limited
        .into_iter()
        .filter_map(|id| {
            let node = instance.nodes.get(&id)?;
            Some(QueryMatch {
                node_id: id,
                anchor: node.anchor.clone(),
                value: node.value.clone(),
                fields: project_fields(&node.extra_fields, query.project.as_ref()),
            })
        })
        .collect();

    // 6. Apply group_by: sort results by the specified field value.
    if let Some(ref group_key) = query.group_by {
        results.sort_by(|a, b| {
            let va = a.fields.get(group_key).map(value_sort_key);
            let vb = b.fields.get(group_key).map(value_sort_key);
            va.cmp(&vb)
        });
    }

    results
}

/// Follow a path of edge kinds from a set of starting nodes.
///
/// Each step collects all children reachable via arcs whose edge kind
/// matches the path element.
fn navigate_path(instance: &WInstance, start_nodes: &[u32], path: &[Name]) -> Vec<u32> {
    let mut current = start_nodes.to_vec();
    for edge_kind in path {
        let mut next = Vec::new();
        for &node_id in &current {
            for &(src, tgt, ref edge) in &instance.arcs {
                if src == node_id && edge.kind == *edge_kind {
                    next.push(tgt);
                }
            }
        }
        current = next;
    }
    current
}

/// Build an expression evaluation environment from a node's fields.
///
/// Binds all `extra_fields` as variables, plus `_anchor`, `_id`,
/// `_value`, and `_children_count` metadata fields.
#[must_use]
pub fn build_node_env(node: &Node, instance: &WInstance) -> panproto_expr::Env {
    let mut env = build_env_from_extra_fields(&node.extra_fields);
    env = env.extend(
        std::sync::Arc::from("_anchor"),
        panproto_expr::Literal::Str(node.anchor.as_ref().into()),
    );
    env = env.extend(
        std::sync::Arc::from("_id"),
        panproto_expr::Literal::Int(i64::from(node.id)),
    );
    if let Some(FieldPresence::Present(ref v)) = node.value {
        env = env.extend(std::sync::Arc::from("_value"), value_to_expr_literal(v));
    }
    // Bind _children_count: number of outgoing arcs from this node.
    let children_count = instance
        .arcs
        .iter()
        .filter(|(src, _, _)| *src == node.id)
        .count();
    #[allow(clippy::cast_possible_wrap)]
    {
        env = env.extend(
            std::sync::Arc::from("_children_count"),
            panproto_expr::Literal::Int(children_count as i64),
        );
    }
    env
}

/// Produce a sortable key from a [`Value`] for `group_by` ordering.
///
/// Converts each variant to a string representation so that values
/// of the same type sort lexicographically.
fn value_sort_key(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Token(t) => t.clone(),
        Value::Null => String::new(),
        _ => format!("{v:?}"),
    }
}

/// Project fields to a subset, or return all if no projection specified.
fn project_fields(
    fields: &HashMap<String, Value>,
    project: Option<&Vec<String>>,
) -> FxHashMap<String, Value> {
    project.map_or_else(
        || fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        |keys| {
            let mut result = FxHashMap::default();
            for key in keys {
                if let Some(val) = fields.get(key) {
                    result.insert(key.clone(), val.clone());
                }
            }
            result
        },
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use panproto_schema::{Edge, Protocol, SchemaBuilder};

    fn make_test_schema() -> panproto_schema::Schema {
        let protocol = Protocol::default();
        SchemaBuilder::new(&protocol)
            .vertex("document", "record", None)
            .unwrap()
            .vertex("layer", "record", None)
            .unwrap()
            .vertex("annotation", "record", None)
            .unwrap()
            .edge("document", "layer", "layers", None)
            .unwrap()
            .edge("layer", "annotation", "annotations", None)
            .unwrap()
            .build()
            .unwrap()
    }

    fn make_test_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "document"));

        let mut ann1 = Node::new(1, "layer");
        ann1.extra_fields
            .insert("kind".into(), Value::Str("span".into()));
        nodes.insert(1, ann1);

        let mut ann2 = Node::new(2, "annotation");
        ann2.extra_fields
            .insert("label".into(), Value::Str("ingredient".into()));
        ann2.extra_fields
            .insert("confidence".into(), Value::Float(0.9));
        nodes.insert(2, ann2);

        let mut ann3 = Node::new(3, "annotation");
        ann3.extra_fields
            .insert("label".into(), Value::Str("step".into()));
        ann3.extra_fields
            .insert("confidence".into(), Value::Float(0.4));
        nodes.insert(3, ann3);

        let edge_layer = Edge {
            src: Name::from("document"),
            tgt: Name::from("layer"),
            kind: Name::from("layers"),
            name: None,
        };
        let edge_ann = Edge {
            src: Name::from("layer"),
            tgt: Name::from("annotation"),
            kind: Name::from("annotations"),
            name: None,
        };

        let arcs = vec![
            (0, 1, edge_layer),
            (1, 2, edge_ann.clone()),
            (1, 3, edge_ann),
        ];

        WInstance::new(nodes, arcs, vec![], 0, Name::from("document"))
    }

    #[test]
    fn query_by_anchor() {
        let inst = make_test_instance();
        let query = InstanceQuery {
            anchor: Name::from("annotation"),
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_with_predicate() {
        let inst = make_test_instance();
        let query = InstanceQuery {
            anchor: Name::from("annotation"),
            predicate: Some(panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Eq,
                vec![
                    panproto_expr::Expr::Var("label".into()),
                    panproto_expr::Expr::Lit(panproto_expr::Literal::Str("ingredient".into())),
                ],
            )),
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].fields.get("label"),
            Some(&Value::Str("ingredient".into()))
        );
    }

    #[test]
    fn query_with_path_navigation() {
        let inst = make_test_instance();
        // Start at document, follow "layers" edge, then "annotations" edge.
        let query = InstanceQuery {
            anchor: Name::from("document"),
            path: vec![Name::from("layers"), Name::from("annotations")],
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        // Path navigation reaches the annotation nodes (2 and 3),
        // but the anchor filter was on "document" which matched node 0,
        // then path navigated to its descendants.
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn query_with_limit() {
        let inst = make_test_instance();
        let query = InstanceQuery {
            anchor: Name::from("annotation"),
            limit: Some(1),
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn query_with_projection() {
        let inst = make_test_instance();
        let query = InstanceQuery {
            anchor: Name::from("annotation"),
            project: Some(vec!["label".into()]),
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        assert_eq!(results.len(), 2);
        // Only "label" field should be present, not "confidence".
        for r in &results {
            assert!(r.fields.contains_key("label"));
            assert!(!r.fields.contains_key("confidence"));
        }
    }

    #[test]
    fn query_no_match() {
        let inst = make_test_instance();
        let query = InstanceQuery {
            anchor: Name::from("nonexistent"),
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        assert!(results.is_empty());
    }

    #[test]
    fn query_with_group_by() {
        // Build an instance with annotations that have different categories.
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "document"));

        let mut layer = Node::new(1, "layer");
        layer
            .extra_fields
            .insert("kind".into(), Value::Str("span".into()));
        nodes.insert(1, layer);

        let categories = ["vegetable", "fruit", "fruit", "vegetable", "grain"];
        for (i, cat) in categories.iter().enumerate() {
            let id = (i as u32) + 2;
            let mut ann = Node::new(id, "annotation");
            ann.extra_fields
                .insert("category".into(), Value::Str((*cat).into()));
            ann.extra_fields
                .insert("label".into(), Value::Str(format!("item_{i}")));
            nodes.insert(id, ann);
        }

        let edge_layer = Edge {
            src: Name::from("document"),
            tgt: Name::from("layer"),
            kind: Name::from("layers"),
            name: None,
        };
        let mut arcs = vec![(0, 1, edge_layer)];
        for i in 0..categories.len() {
            let id = (i as u32) + 2;
            arcs.push((
                1,
                id,
                Edge {
                    src: Name::from("layer"),
                    tgt: Name::from("annotation"),
                    kind: Name::from("annotations"),
                    name: None,
                },
            ));
        }

        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("document"));

        let query = InstanceQuery {
            anchor: Name::from("annotation"),
            group_by: Some("category".into()),
            ..Default::default()
        };
        let results = execute(&query, &inst, &make_test_schema());
        assert_eq!(results.len(), 5);

        // Results should be sorted by category: fruit, fruit, grain, vegetable, vegetable.
        let categories_out: Vec<&str> = results
            .iter()
            .filter_map(|r| match r.fields.get("category") {
                Some(Value::Str(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            categories_out,
            vec!["fruit", "fruit", "grain", "vegetable", "vegetable"]
        );
    }
}
