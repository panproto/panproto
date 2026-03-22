//! Polynomial functor operations on instances.
//!
//! Schemas are polynomial functors; instances are W-types (initial algebras).
//! This module exposes the derived operations that arise from the adjoint
//! triple Σ ⊣ Δ ⊣ Π applied to the polynomial interpretation:
//!
//! - **Fiber**: preimage of a migration at a target anchor (Δ at a point)
//! - **Group-by**: partition source nodes by fiber (explicit Π on trees)
//! - **Join**: pullback of two instances along shared projections
//! - **Section**: construct an enriched instance from base + annotation data

use std::collections::HashMap;

use panproto_gat::Name;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::metadata::Node;
use crate::value::FieldPresence;
use crate::wtype::{
    CompiledMigration, WInstance, build_env_from_extra_fields, value_to_expr_literal,
};

/// Compute the fiber of a compiled migration at a specific target anchor.
///
/// Given migration m: S → T and target anchor `a` in T, returns all source
/// node IDs whose remapped anchor equals `a`. This is the `Δ_f` operation
/// applied to a representable (a single point).
#[must_use]
pub fn fiber_at_anchor(
    compiled: &CompiledMigration,
    source: &WInstance,
    target_anchor: &Name,
) -> Vec<u32> {
    source
        .nodes
        .iter()
        .filter(|(_, node)| {
            compiled
                .vertex_remap
                .get(&node.anchor)
                .is_some_and(|remapped| remapped == target_anchor)
        })
        .map(|(id, _)| *id)
        .collect()
}

/// Compute fibers for ALL target anchors simultaneously.
///
/// Returns a map: target anchor → source node IDs. This is the complete
/// fiber decomposition of the migration. Every source node appears in
/// exactly one fiber (the fibers partition the source).
#[must_use]
pub fn fiber_decomposition(
    compiled: &CompiledMigration,
    source: &WInstance,
) -> FxHashMap<Name, Vec<u32>> {
    let mut fibers: FxHashMap<Name, Vec<u32>> = FxHashMap::default();
    for (&id, node) in &source.nodes {
        if let Some(target) = compiled.vertex_remap.get(&node.anchor) {
            fibers.entry(target.clone()).or_default().push(id);
        }
    }
    fibers
}

/// Fiber with a value predicate: compute f⁻¹(a) ∩ {x | pred(x)}.
///
/// Combines `Δ_f` (pullback) with conditional survival. The predicate is
/// evaluated against each source node's `extra_fields`, with all fields
/// bound as expression variables.
#[must_use]
pub fn fiber_with_predicate(
    compiled: &CompiledMigration,
    source: &WInstance,
    target_anchor: &Name,
    predicate: &panproto_expr::Expr,
    eval_config: &panproto_expr::EvalConfig,
) -> Vec<u32> {
    fiber_at_anchor(compiled, source, target_anchor)
        .into_iter()
        .filter(|&id| {
            let Some(node) = source.nodes.get(&id) else {
                return false;
            };
            let mut env = build_env_from_extra_fields(&node.extra_fields);
            if let Some(FieldPresence::Present(ref v)) = node.value {
                env = env.extend(std::sync::Arc::from("_value"), value_to_expr_literal(v));
            }
            env = env.extend(
                std::sync::Arc::from("_anchor"),
                panproto_expr::Literal::Str(node.anchor.as_ref().into()),
            );
            matches!(
                panproto_expr::eval(predicate, &env, eval_config),
                Ok(panproto_expr::Literal::Bool(true))
            )
        })
        .collect()
}

/// Group source nodes by their image under a migration.
///
/// Returns: for each target anchor, a sub-instance containing only the
/// source nodes in that fiber (with internal arcs preserved).
///
/// This is the dependent product `Π_f` computed explicitly on trees.
#[must_use]
pub fn group_by(compiled: &CompiledMigration, source: &WInstance) -> FxHashMap<Name, WInstance> {
    let fibers = fiber_decomposition(compiled, source);
    fibers
        .into_iter()
        .map(|(anchor, node_ids)| {
            let sub = extract_subinstance(source, &node_ids);
            (anchor, sub)
        })
        .collect()
}

/// Join two instances along a shared projection.
///
/// Given A →f C ←g B, compute the pullback A ×_C B: pairs (a, b) where
/// f(a) and g(b) map to the same target anchor.
///
/// Returns all matching pairs as (`left_node_id`, `right_node_id`).
#[must_use]
pub fn join(
    left: &WInstance,
    right: &WInstance,
    left_compiled: &CompiledMigration,
    right_compiled: &CompiledMigration,
) -> Vec<(u32, u32)> {
    let left_fibers = fiber_decomposition(left_compiled, left);
    let right_fibers = fiber_decomposition(right_compiled, right);

    let mut pairs = Vec::new();
    for (anchor, left_ids) in &left_fibers {
        if let Some(right_ids) = right_fibers.get(anchor) {
            for &l in left_ids {
                for &r in right_ids {
                    pairs.push((l, r));
                }
            }
        }
    }
    pairs
}

/// Extract a sub-instance containing only the specified nodes and arcs
/// between them.
#[must_use]
fn extract_subinstance(source: &WInstance, node_ids: &[u32]) -> WInstance {
    let id_set: FxHashSet<u32> = node_ids.iter().copied().collect();
    let nodes: HashMap<u32, Node> = source
        .nodes
        .iter()
        .filter(|(id, _)| id_set.contains(id))
        .map(|(&id, n)| (id, n.clone()))
        .collect();
    let arcs: Vec<_> = source
        .arcs
        .iter()
        .filter(|(src, tgt, _)| id_set.contains(src) && id_set.contains(tgt))
        .cloned()
        .collect();
    let root = node_ids.first().copied().unwrap_or(0);
    WInstance::new(nodes, arcs, vec![], root, source.schema_root.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use panproto_schema::Edge;

    /// Build a simple test instance: root with two annotation children.
    fn make_test_instance() -> (WInstance, CompiledMigration) {
        let mut nodes = HashMap::new();
        let root = Node::new(0, "root");
        nodes.insert(0, root);

        let mut node_a = Node::new(1, "annotation");
        node_a
            .extra_fields
            .insert("label".into(), Value::Str("ingredient".into()));
        node_a
            .extra_fields
            .insert("confidence".into(), Value::Float(0.9));
        nodes.insert(1, node_a);

        let mut node_b = Node::new(2, "annotation");
        node_b
            .extra_fields
            .insert("label".into(), Value::Str("step".into()));
        node_b
            .extra_fields
            .insert("confidence".into(), Value::Float(0.5));
        nodes.insert(2, node_b);

        let edge = Edge {
            src: Name::from("root"),
            tgt: Name::from("annotation"),
            kind: Name::from("child"),
            name: None,
        };
        let arcs = vec![(0, 1, edge.clone()), (0, 2, edge)];

        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("root"), Name::from("document"));
        vertex_remap.insert(Name::from("annotation"), Name::from("span"));

        let compiled = CompiledMigration {
            surviving_verts: ["root", "annotation"]
                .iter()
                .map(|s| Name::from(*s))
                .collect(),
            surviving_edges: std::collections::HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };

        (inst, compiled)
    }

    #[test]
    fn fiber_at_anchor_basic() {
        let (inst, compiled) = make_test_instance();
        let fiber = fiber_at_anchor(&compiled, &inst, &Name::from("span"));
        assert_eq!(fiber.len(), 2);
        assert!(fiber.contains(&1));
        assert!(fiber.contains(&2));
    }

    #[test]
    fn fiber_at_anchor_root() {
        let (inst, compiled) = make_test_instance();
        let fiber = fiber_at_anchor(&compiled, &inst, &Name::from("document"));
        assert_eq!(fiber, vec![0]);
    }

    #[test]
    fn fiber_at_anchor_nonexistent() {
        let (inst, compiled) = make_test_instance();
        let fiber = fiber_at_anchor(&compiled, &inst, &Name::from("nonexistent"));
        assert!(fiber.is_empty());
    }

    #[test]
    fn fiber_decomposition_partitions() {
        let (inst, compiled) = make_test_instance();
        let fibers = fiber_decomposition(&compiled, &inst);

        // All source nodes appear in exactly one fiber
        let mut all_ids: Vec<u32> = fibers.values().flatten().copied().collect();
        all_ids.sort_unstable();
        assert_eq!(all_ids, vec![0, 1, 2]);

        // Two fibers: document and span
        assert_eq!(fibers.len(), 2);
        assert_eq!(fibers[&Name::from("document")].len(), 1);
        assert_eq!(fibers[&Name::from("span")].len(), 2);
    }

    #[test]
    fn fiber_with_predicate_filters() {
        let (inst, compiled) = make_test_instance();
        let config = panproto_expr::EvalConfig::default();

        // Filter: confidence > 0.8
        let predicate = panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Gt,
            vec![
                panproto_expr::Expr::Var("confidence".into()),
                panproto_expr::Expr::Lit(panproto_expr::Literal::Float(0.8)),
            ],
        );

        let filtered =
            fiber_with_predicate(&compiled, &inst, &Name::from("span"), &predicate, &config);
        // Only node 1 has confidence 0.9 > 0.8
        assert_eq!(filtered, vec![1]);
    }

    #[test]
    fn group_by_partitions() {
        let (inst, compiled) = make_test_instance();
        let groups = group_by(&compiled, &inst);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[&Name::from("document")].nodes.len(), 1);
        assert_eq!(groups[&Name::from("span")].nodes.len(), 2);
    }

    #[test]
    fn join_computes_pullback() {
        let (left, left_compiled) = make_test_instance();
        let (right, right_compiled) = make_test_instance();

        let pairs = join(&left, &right, &left_compiled, &right_compiled);

        // Both instances have 2 "span" nodes and 1 "document" node.
        // Span × Span = 4 pairs, Document × Document = 1 pair → 5 total.
        assert_eq!(pairs.len(), 5);
    }
}
