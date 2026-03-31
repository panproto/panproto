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

use std::collections::{HashMap, VecDeque};

use panproto_gat::Name;
use panproto_schema::Edge;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::metadata::Node;
use crate::value::{FieldPresence, Value};
use crate::wtype::{
    CompiledMigration, WInstance, apply_field_transforms, build_env_from_extra_fields,
    collect_scalar_child_values, reconstruct_fans, resolve_edge, value_to_expr_literal,
};

// ---------------------------------------------------------------------------
// Complement infrastructure
// ---------------------------------------------------------------------------

/// A node that was dropped during restriction, with provenance info.
#[derive(Debug, Clone)]
pub struct DroppedNode {
    /// Original node ID in the source instance.
    pub original_id: u32,
    /// Anchor of the dropped node.
    pub anchor: Name,
    /// The surviving node this was contracted into (nearest surviving ancestor).
    pub contracted_into: Option<u32>,
}

/// Complement data from a restrict operation. Stores everything needed
/// to reconstruct the original instance from the restricted result.
#[derive(Debug, Clone, Default)]
pub struct Complement {
    /// Nodes that were dropped during restriction.
    pub dropped_nodes: Vec<DroppedNode>,
    /// Arcs that were dropped (both endpoints must have been in the source).
    pub dropped_arcs: Vec<(u32, u32, Edge)>,
    /// Pre-transform `extra_fields` for nodes that had `field_transforms` applied.
    /// Used by backward migration to restore original field values.
    pub original_extra_fields: HashMap<u32, HashMap<String, crate::value::Value>>,
}

/// An enrichment to add when constructing a section.
#[derive(Debug, Clone)]
pub struct SectionEnrichment {
    /// Node ID in the base instance that this enrichment annotates.
    pub base_node_id: u32,
    /// Anchor for the new enrichment node (must be a vertex in the
    /// source schema but not in the target schema).
    pub anchor: Name,
    /// Edge connecting the base node to this enrichment.
    pub edge: Edge,
    /// Value for the enrichment node.
    pub value: Option<FieldPresence>,
    /// Extra fields for the enrichment node.
    pub extra_fields: FxHashMap<String, Value>,
}

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

// ---------------------------------------------------------------------------
// Restrict with complement
// ---------------------------------------------------------------------------

/// Restrict an instance and collect complement data.
///
/// Like `wtype_restrict` but also returns a [`Complement`] recording all
/// dropped nodes and their nearest surviving ancestors. This complement
/// is needed for [`fiber_at_node`] and for backward data migration.
///
/// # Errors
///
/// Returns [`RestrictError`](crate::error::RestrictError) if the root is
/// pruned or edge resolution fails during ancestor contraction.
pub fn restrict_with_complement(
    instance: &WInstance,
    _src_schema: &panproto_schema::Schema,
    tgt_schema: &panproto_schema::Schema,
    migration: &CompiledMigration,
) -> Result<(WInstance, Complement), crate::error::RestrictError> {
    use crate::error::RestrictError;

    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;
    let root_target_anchor = migration
        .vertex_remap
        .get(&root_node.anchor)
        .unwrap_or(&root_node.anchor);
    if !migration.surviving_verts.contains(root_target_anchor) {
        return Err(RestrictError::RootPruned);
    }

    let mut new_nodes: HashMap<u32, Node> = HashMap::new();
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::new();
    let mut surviving_set: FxHashSet<u32> = FxHashSet::default();
    let mut complement = Complement::default();
    let mut queue: VecDeque<(u32, Option<u32>)> = VecDeque::new();

    // Process root
    let mut root_node_cloned = root_node.clone();
    if let Some(remapped) = migration.vertex_remap.get(&root_node.anchor) {
        root_node_cloned.anchor.clone_from(remapped);
    }
    // Check conditional survival for root
    if let Some(pred) = migration.conditional_survival.get(&root_node.anchor) {
        let env = build_env_from_extra_fields(&root_node.extra_fields);
        let config = panproto_expr::EvalConfig::default();
        if matches!(
            panproto_expr::eval(pred, &env, &config),
            Ok(panproto_expr::Literal::Bool(false))
        ) {
            return Err(RestrictError::RootPruned);
        }
    }
    // Apply field transforms to root
    if let Some(transforms) = migration.field_transforms.get(&root_node.anchor) {
        complement
            .original_extra_fields
            .insert(instance.root, root_node.extra_fields.clone());
        let scalars = collect_scalar_child_values(instance, instance.root);
        apply_field_transforms(&mut root_node_cloned, transforms, &scalars);
    }
    new_nodes.insert(instance.root, root_node_cloned);
    surviving_set.insert(instance.root);
    queue.push_back((instance.root, None));

    // BFS: visit each node, tracking nearest surviving ancestor
    while let Some((current_id, ancestor_id)) = queue.pop_front() {
        let child_ancestor = if surviving_set.contains(&current_id) {
            Some(current_id)
        } else {
            ancestor_id
        };
        restrict_bfs_step(
            instance,
            tgt_schema,
            migration,
            current_id,
            child_ancestor,
            &mut new_nodes,
            &mut new_arcs,
            &mut surviving_set,
            &mut complement,
            &mut queue,
        )?;
    }

    // Record dropped arcs
    collect_dropped_arcs(instance, &surviving_set, &mut complement);

    // Fan reconstruction
    let empty_ancestors = FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        &surviving_set,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

    let new_schema_root = migration
        .vertex_remap
        .get(&instance.schema_root)
        .cloned()
        .unwrap_or_else(|| instance.schema_root.clone());

    let restricted = WInstance::new(
        new_nodes,
        new_arcs,
        new_fans,
        instance.root,
        new_schema_root,
    );
    Ok((restricted, complement))
}

/// Process one BFS level: check children of `current_id` for survival.
#[allow(clippy::too_many_arguments)]
fn restrict_bfs_step(
    instance: &WInstance,
    tgt_schema: &panproto_schema::Schema,
    migration: &CompiledMigration,
    current_id: u32,
    child_ancestor: Option<u32>,
    new_nodes: &mut HashMap<u32, Node>,
    new_arcs: &mut Vec<(u32, u32, Edge)>,
    surviving_set: &mut FxHashSet<u32>,
    complement: &mut Complement,
    queue: &mut VecDeque<(u32, Option<u32>)>,
) -> Result<(), crate::error::RestrictError> {
    use crate::error::RestrictError;

    for &child_id in instance.children(current_id) {
        let Some(child_node) = instance.nodes.get(&child_id) else {
            continue;
        };

        let target_anchor = migration
            .vertex_remap
            .get(&child_node.anchor)
            .unwrap_or(&child_node.anchor);
        let mut child_survives = migration.surviving_verts.contains(target_anchor);

        if child_survives {
            if let Some(pred) = migration.conditional_survival.get(&child_node.anchor) {
                let env = build_env_from_extra_fields(&child_node.extra_fields);
                let config = panproto_expr::EvalConfig::default();
                if matches!(
                    panproto_expr::eval(pred, &env, &config),
                    Ok(panproto_expr::Literal::Bool(false))
                ) {
                    child_survives = false;
                }
            }
        }

        if child_survives {
            surviving_set.insert(child_id);
            let mut new_node = child_node.clone();
            if let Some(remapped) = migration.vertex_remap.get(&child_node.anchor) {
                new_node.anchor.clone_from(remapped);
            }
            if let Some(transforms) = migration.field_transforms.get(&child_node.anchor) {
                // Capture pre-transform extra_fields before applying transforms
                complement
                    .original_extra_fields
                    .insert(child_id, child_node.extra_fields.clone());
                let scalars = collect_scalar_child_values(instance, child_id);
                apply_field_transforms(&mut new_node, transforms, &scalars);
            }
            new_nodes.insert(child_id, new_node.clone());

            if let Some(anc_id) = child_ancestor {
                let anc_node = new_nodes.get(&anc_id).ok_or(RestrictError::RootPruned)?;
                let edge = resolve_edge(
                    tgt_schema,
                    &migration.resolver,
                    &anc_node.anchor,
                    &new_node.anchor,
                )?;
                new_arcs.push((anc_id, child_id, edge));
            }
        } else {
            complement.dropped_nodes.push(DroppedNode {
                original_id: child_id,
                anchor: child_node.anchor.clone(),
                contracted_into: child_ancestor,
            });
        }

        queue.push_back((child_id, child_ancestor));
    }
    Ok(())
}

/// Collect arcs from the source instance where at least one endpoint
/// did not survive restriction.
fn collect_dropped_arcs(
    instance: &WInstance,
    surviving_set: &FxHashSet<u32>,
    complement: &mut Complement,
) {
    for (src, tgt, edge) in &instance.arcs {
        if !surviving_set.contains(src) || !surviving_set.contains(tgt) {
            complement.dropped_arcs.push((*src, *tgt, edge.clone()));
        }
    }
}

// ---------------------------------------------------------------------------
// Fiber at node
// ---------------------------------------------------------------------------

/// Compute fiber at a specific node ID in the restricted (target) instance.
///
/// Given source instance S, target instance T = restrict(S, migration),
/// and a node n in T, find all nodes in S that were either:
/// (a) remapped to n's anchor, or
/// (b) contracted into n during ancestor contraction.
#[must_use]
pub fn fiber_at_node(
    source: &WInstance,
    target: &WInstance,
    target_node_id: u32,
    complement: &Complement,
) -> Vec<u32> {
    let Some(target_node) = target.nodes.get(&target_node_id) else {
        return vec![];
    };

    let mut result = Vec::new();

    // Direct preimage: source nodes with matching anchor
    for (&id, node) in &source.nodes {
        if node.anchor == target_node.anchor {
            result.push(id);
        }
    }

    // Contracted nodes
    for dropped in &complement.dropped_nodes {
        if dropped.contracted_into == Some(target_node_id) {
            result.push(dropped.original_id);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Section construction
// ---------------------------------------------------------------------------

/// Construct a section of a projection.
///
/// Given:
/// - `base`: an instance of the target schema T
/// - `projection`: a compiled migration S -> T
/// - `enrichments`: nodes to add in the S-instance fibers
///
/// Produces an S-instance that:
/// 1. Contains all base nodes (with anchors inverse-mapped to source schema)
/// 2. Contains all enrichment nodes attached at the correct positions
/// 3. Projects back to base under the migration:
///    restrict(section(base, projection, enrichments), projection) = base
///
/// # Errors
///
/// Returns [`InstError::NodeNotFound`](crate::error::InstError::NodeNotFound)
/// if an enrichment references a base node ID that does not exist.
pub fn section(
    base: &WInstance,
    projection: &CompiledMigration,
    enrichments: Vec<SectionEnrichment>,
) -> Result<WInstance, crate::error::InstError> {
    // Build inverse vertex remap: target_anchor -> source_anchor
    let inverse_remap: HashMap<Name, Name> = projection
        .vertex_remap
        .iter()
        .map(|(src, tgt)| (tgt.clone(), src.clone()))
        .collect();

    // Step 1: copy base nodes, remapping anchors back to source schema
    let mut nodes: HashMap<u32, Node> = HashMap::new();
    let mut next_id: u32 = base.nodes.keys().max().copied().unwrap_or(0) + 1;

    for (&id, node) in &base.nodes {
        let mut new_node = node.clone();
        if let Some(src_anchor) = inverse_remap.get(&node.anchor) {
            new_node.anchor = src_anchor.clone();
        }
        nodes.insert(id, new_node);
    }

    // Remap arcs to use source anchors
    let arcs: Vec<_> = base
        .arcs
        .iter()
        .map(|(src_id, tgt_id, edge)| {
            let mut new_edge = edge.clone();
            if let Some(src_anchor) = inverse_remap.get(&new_edge.src) {
                new_edge.src = src_anchor.clone();
            }
            if let Some(tgt_anchor) = inverse_remap.get(&new_edge.tgt) {
                new_edge.tgt = tgt_anchor.clone();
            }
            (*src_id, *tgt_id, new_edge)
        })
        .collect();

    let mut all_arcs = arcs;

    // Step 2: add enrichment nodes
    for enrichment in enrichments {
        if !base.nodes.contains_key(&enrichment.base_node_id) {
            return Err(crate::error::InstError::NodeNotFound(
                enrichment.base_node_id,
            ));
        }

        let enrichment_id = next_id;
        next_id += 1;

        let mut new_node = Node::new(enrichment_id, enrichment.anchor.clone());
        if let Some(value) = enrichment.value {
            new_node = new_node.with_value(value);
        }
        for (k, v) in enrichment.extra_fields {
            new_node.extra_fields.insert(k, v);
        }

        nodes.insert(enrichment_id, new_node);
        all_arcs.push((enrichment.base_node_id, enrichment_id, enrichment.edge));
    }

    let schema_root = inverse_remap
        .get(&base.schema_root)
        .cloned()
        .unwrap_or_else(|| base.schema_root.clone());

    Ok(WInstance::new(
        nodes,
        all_arcs,
        base.fans.clone(),
        base.root,
        schema_root,
    ))
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
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::value::Value;
    use crate::wtype::wtype_restrict;
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

    #[test]
    fn fiber_at_node_basic() {
        // Source: root(0) -> annotation(1), root(0) -> annotation(2), root(0) -> text(3)
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "annotation"));
        nodes.insert(2, Node::new(2, "annotation"));
        nodes.insert(3, Node::new(3, "text"));

        let edge_ann = Edge {
            src: Name::from("root"),
            tgt: Name::from("annotation"),
            kind: Name::from("child"),
            name: None,
        };
        let edge_txt = Edge {
            src: Name::from("root"),
            tgt: Name::from("text"),
            kind: Name::from("child"),
            name: None,
        };
        let source = WInstance::new(
            nodes,
            vec![(0, 1, edge_ann.clone()), (0, 2, edge_ann), (0, 3, edge_txt)],
            vec![],
            0,
            Name::from("root"),
        );

        // Target: root(0) -> text(3), annotation nodes dropped
        let mut tgt_nodes = HashMap::new();
        tgt_nodes.insert(0, Node::new(0, "root"));
        tgt_nodes.insert(3, Node::new(3, "text"));

        let tgt_edge = Edge {
            src: Name::from("root"),
            tgt: Name::from("text"),
            kind: Name::from("child"),
            name: None,
        };
        let target = WInstance::new(
            tgt_nodes,
            vec![(0, 3, tgt_edge)],
            vec![],
            0,
            Name::from("root"),
        );

        // Complement: annotations contracted into root
        let complement = Complement {
            dropped_nodes: vec![
                DroppedNode {
                    original_id: 1,
                    anchor: Name::from("annotation"),
                    contracted_into: Some(0),
                },
                DroppedNode {
                    original_id: 2,
                    anchor: Name::from("annotation"),
                    contracted_into: Some(0),
                },
            ],
            dropped_arcs: vec![],
            original_extra_fields: HashMap::new(),
        };

        // Fiber at root (id=0): direct match on anchor "root" (node 0) + contracted (1, 2)
        let fiber = fiber_at_node(&source, &target, 0, &complement);
        assert!(fiber.contains(&0)); // direct preimage
        assert!(fiber.contains(&1)); // contracted
        assert!(fiber.contains(&2)); // contracted
        assert_eq!(fiber.len(), 3);

        // Fiber at text (id=3): direct match only
        let fiber_text = fiber_at_node(&source, &target, 3, &complement);
        assert!(fiber_text.contains(&3));
        assert_eq!(fiber_text.len(), 1);
    }

    /// Build a minimal test schema with the given vertex names and edges.
    fn make_schema(vertices: &[&str], edges: &[Edge]) -> panproto_schema::Schema {
        use smallvec::smallvec;
        let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();
        for edge in edges {
            between
                .entry((Name::from(&*edge.src), Name::from(&*edge.tgt)))
                .or_insert_with(|| smallvec![])
                .push(edge.clone());
        }
        panproto_schema::Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        panproto_schema::Vertex {
                            id: Name::from(v),
                            kind: Name::from("object"),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: HashMap::new(),
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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between,
        }
    }

    #[test]
    fn restrict_with_complement_tracks_drops() {
        let doc_ann_edge = Edge {
            src: Name::from("doc"),
            tgt: Name::from("annotation"),
            kind: Name::from("child"),
            name: None,
        };
        let doc_text_edge = Edge {
            src: Name::from("doc"),
            tgt: Name::from("text"),
            kind: Name::from("child"),
            name: None,
        };

        let tgt_schema = make_schema(&["doc", "text"], std::slice::from_ref(&doc_text_edge));
        let src_schema = make_schema(
            &["doc", "annotation", "text"],
            &[doc_ann_edge, doc_text_edge],
        );

        // Migration: doc -> doc, text -> text, annotation not in surviving_verts
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("doc"), Name::from("doc"));
        vertex_remap.insert(Name::from("text"), Name::from("text"));
        let migration = CompiledMigration {
            surviving_verts: ["doc", "text"].iter().map(|s| Name::from(*s)).collect(),
            surviving_edges: std::collections::HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };

        // Instance: doc(0) -> annotation(1), doc(0) -> text(2)
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "doc"));
        nodes.insert(1, Node::new(1, "annotation"));
        nodes.insert(2, Node::new(2, "text"));

        let instance = WInstance::new(
            nodes,
            vec![
                (
                    0,
                    1,
                    Edge {
                        src: Name::from("doc"),
                        tgt: Name::from("annotation"),
                        kind: Name::from("child"),
                        name: None,
                    },
                ),
                (
                    0,
                    2,
                    Edge {
                        src: Name::from("doc"),
                        tgt: Name::from("text"),
                        kind: Name::from("child"),
                        name: None,
                    },
                ),
            ],
            vec![],
            0,
            Name::from("doc"),
        );

        let (restricted, complement) =
            restrict_with_complement(&instance, &src_schema, &tgt_schema, &migration).unwrap();

        // Restricted should have 2 nodes: doc and text
        assert_eq!(restricted.nodes.len(), 2);
        assert!(restricted.nodes.contains_key(&0));
        assert!(restricted.nodes.contains_key(&2));

        // Complement should have 1 dropped node: annotation
        assert_eq!(complement.dropped_nodes.len(), 1);
        assert_eq!(complement.dropped_nodes[0].original_id, 1);
        assert_eq!(complement.dropped_nodes[0].anchor, Name::from("annotation"));
        assert_eq!(complement.dropped_nodes[0].contracted_into, Some(0));

        // Dropped arcs: the arc from doc -> annotation
        assert_eq!(complement.dropped_arcs.len(), 1);
    }

    #[test]
    fn section_roundtrip() {
        let doc_ann_edge = Edge {
            src: Name::from("doc"),
            tgt: Name::from("annotation"),
            kind: Name::from("child"),
            name: None,
        };
        let doc_text_edge = Edge {
            src: Name::from("doc"),
            tgt: Name::from("text"),
            kind: Name::from("child"),
            name: None,
        };

        let tgt_schema = make_schema(&["doc", "text"], std::slice::from_ref(&doc_text_edge));
        let src_schema = make_schema(
            &["doc", "annotation", "text"],
            &[doc_ann_edge, doc_text_edge],
        );

        // Migration: doc -> doc, text -> text
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("doc"), Name::from("doc"));
        vertex_remap.insert(Name::from("text"), Name::from("text"));
        let migration = CompiledMigration {
            surviving_verts: ["doc", "text"].iter().map(|s| Name::from(*s)).collect(),
            surviving_edges: std::collections::HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };

        // Base instance (target schema): doc(0) -> text(1)
        let mut base_nodes = HashMap::new();
        base_nodes.insert(0, Node::new(0, "doc"));
        base_nodes.insert(1, Node::new(1, "text"));

        let base = WInstance::new(
            base_nodes,
            vec![(
                0,
                1,
                Edge {
                    src: Name::from("doc"),
                    tgt: Name::from("text"),
                    kind: Name::from("child"),
                    name: None,
                },
            )],
            vec![],
            0,
            Name::from("doc"),
        );

        // Add one enrichment: an annotation node attached to doc
        let enrichments = vec![SectionEnrichment {
            base_node_id: 0,
            anchor: Name::from("annotation"),
            edge: Edge {
                src: Name::from("doc"),
                tgt: Name::from("annotation"),
                kind: Name::from("child"),
                name: None,
            },
            value: Some(FieldPresence::Present(Value::Str("test".into()))),
            extra_fields: FxHashMap::default(),
        }];

        let section_inst = section(&base, &migration, enrichments).unwrap();

        // Section should have 3 nodes: doc, text, annotation
        assert_eq!(section_inst.nodes.len(), 3);

        // Restricting the section back should match the base
        let restricted =
            wtype_restrict(&section_inst, &src_schema, &tgt_schema, &migration).unwrap();
        assert_eq!(restricted.nodes.len(), base.nodes.len());

        // Verify anchors match: both should have doc and text
        let restricted_anchors: FxHashSet<_> = restricted
            .nodes
            .values()
            .map(|n| n.anchor.clone())
            .collect();
        let base_anchors: FxHashSet<_> = base.nodes.values().map(|n| n.anchor.clone()).collect();
        assert_eq!(restricted_anchors, base_anchors);
    }
}
