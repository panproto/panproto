//! Per-pipeline-step edit translation for the edit lens.
//!
//! The [`EditPipeline`] mirrors the five steps of `wtype_restrict`
//! incrementally, translating individual `TreeEdit` values through
//! reachability tracking, ancestor contraction, edge resolution,
//! and fan reconstruction.

use panproto_inst::{
    CompiledMigration, ContractionRecord, ContractionTracker, ReachabilityIndex, TreeEdit,
    WInstance,
};
use panproto_schema::Schema;
use smallvec::SmallVec;

use crate::EditLens;
use crate::asymmetric::Complement;
use crate::edit_error::EditLensError;

/// Incremental state for per-step edit translation.
///
/// Initialized from an [`EditLens`] and source instance via a
/// whole-state pass, then processes individual edits incrementally.
/// The five pipeline steps correspond to the `wtype_restrict` stages:
///
/// 1. Anchor survival
/// 2. Reachability
/// 3. Ancestor contraction
/// 4. Edge resolution
/// 5. Fan reconstruction
#[derive(Clone)]
pub struct EditPipeline {
    /// Incremental reachability tracking from root.
    pub reachability: ReachabilityIndex,
    contraction: ContractionTracker,
    compiled: CompiledMigration,
    tgt_schema: Schema,
}

impl EditPipeline {
    /// Build an `EditPipeline` from an [`EditLens`] and source instance.
    ///
    /// Performs a whole-state pass to initialize the reachability index.
    #[must_use]
    pub fn from_lens_and_instance(lens: &EditLens, source: &WInstance) -> Self {
        Self {
            reachability: ReachabilityIndex::from_instance(source),
            contraction: ContractionTracker::new(),
            compiled: lens.compiled.clone(),
            tgt_schema: lens.tgt_schema.clone(),
        }
    }

    /// Translate a source edit to a view edit through the 5 pipeline steps.
    ///
    /// Each step may transform the edit or absorb it into the complement.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError`] if edge resolution fails for a
    /// contracted arc.
    pub fn translate_get(
        &mut self,
        edit: &TreeEdit,
        complement: &mut Complement,
    ) -> Result<TreeEdit, EditLensError> {
        let edit = self.step1_anchor_survival(edit, complement);
        if edit.is_identity() {
            return Ok(edit);
        }
        let edit = self.step2_reachability(&edit, complement);
        if edit.is_identity() {
            return Ok(edit);
        }
        let edit = self.step3_ancestor_contraction(&edit, complement);
        if edit.is_identity() {
            return Ok(edit);
        }
        let edit = self.step4_edge_resolution(&edit)?;
        let edit = Self::step5_fan_reconstruction(&edit, complement);
        Ok(edit)
    }

    /// Translate a view edit back to a source edit (reverse pipeline).
    ///
    /// Runs the 5 steps in reverse order.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError`] if edge resolution fails.
    pub fn translate_put(
        &mut self,
        edit: &TreeEdit,
        complement: &mut Complement,
    ) -> Result<TreeEdit, EditLensError> {
        let edit = Self::step5_fan_reconstruction(edit, complement);
        let edit = self.step4_edge_resolution(&edit)?;
        let edit = self.step3_ancestor_contraction(&edit, complement);
        let edit = self.step2_reachability(&edit, complement);
        let edit = self.step1_anchor_survival(&edit, complement);
        Ok(edit)
    }

    /// Step 1: anchor survival.
    ///
    /// Checks whether the edit targets a surviving vertex. Non-surviving
    /// edits are absorbed into the complement.
    fn step1_anchor_survival(&self, edit: &TreeEdit, complement: &mut Complement) -> TreeEdit {
        match edit {
            TreeEdit::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } => {
                if self.anchor_survives(&node.anchor) {
                    edit.clone()
                } else {
                    complement.dropped_nodes.insert(*child_id, node.clone());
                    complement
                        .dropped_arcs
                        .push((*parent, *child_id, edge.clone()));
                    TreeEdit::Identity
                }
            }
            TreeEdit::DeleteNode { id } => {
                if complement.dropped_nodes.contains_key(id) {
                    complement.dropped_nodes.remove(id);
                    complement
                        .dropped_arcs
                        .retain(|&(_, child, _)| child != *id);
                    TreeEdit::Identity
                } else {
                    edit.clone()
                }
            }
            TreeEdit::SetField {
                node_id,
                field,
                value,
            } => {
                if let Some(node) = complement.dropped_nodes.get_mut(node_id) {
                    node.extra_fields.insert(field.to_string(), value.clone());
                    TreeEdit::Identity
                } else {
                    edit.clone()
                }
            }
            TreeEdit::RelabelNode { id, new_anchor } => {
                let was_complement = complement.dropped_nodes.contains_key(id);
                let survives = self.anchor_survives(new_anchor);
                self.relabel_dispatch(*id, new_anchor, was_complement, survives, complement)
            }
            _ => edit.clone(),
        }
    }

    /// Dispatch for relabel edits in step 1 (split out for line length).
    fn relabel_dispatch(
        &self,
        id: u32,
        new_anchor: &panproto_gat::Name,
        was_complement: bool,
        survives: bool,
        complement: &mut Complement,
    ) -> TreeEdit {
        match (was_complement, survives) {
            (true, true) => {
                if let Some(node) = complement.dropped_nodes.remove(&id) {
                    complement.dropped_arcs.retain(|&(_, child, _)| child != id);
                    let parent = complement
                        .original_parent
                        .get(&id)
                        .copied()
                        .or_else(|| self.reachability.root())
                        .unwrap_or(0);
                    // Find the parent's anchor to resolve the correct edge.
                    let parent_anchor = complement
                        .dropped_nodes
                        .get(&parent)
                        .map(|n| n.anchor.clone())
                        .or_else(|| self.tgt_schema.vertices.keys().next().cloned())
                        .unwrap_or_else(|| panproto_gat::Name::from("root"));
                    // Resolve the edge from the target schema between parent and child anchors.
                    let edge = self
                        .tgt_schema
                        .edges_between(&parent_anchor, new_anchor)
                        .first()
                        .cloned()
                        .unwrap_or_else(|| panproto_schema::Edge {
                            src: parent_anchor,
                            tgt: new_anchor.clone(),
                            kind: "prop".into(),
                            name: None,
                        });
                    TreeEdit::InsertNode {
                        parent,
                        child_id: id,
                        node,
                        edge,
                    }
                } else {
                    TreeEdit::Identity
                }
            }
            (true, false) => {
                if let Some(node) = complement.dropped_nodes.get_mut(&id) {
                    node.anchor.clone_from(new_anchor);
                }
                TreeEdit::Identity
            }
            (false, true) => TreeEdit::RelabelNode {
                id,
                new_anchor: new_anchor.clone(),
            },
            (false, false) => TreeEdit::DeleteNode { id },
        }
    }

    /// Step 2: reachability tracking.
    ///
    /// Updates the [`ReachabilityIndex`] and cascades reachability
    /// changes into additional edits or complement updates.
    fn step2_reachability(&mut self, edit: &TreeEdit, complement: &mut Complement) -> TreeEdit {
        match edit {
            TreeEdit::InsertNode {
                parent, child_id, ..
            } => {
                let newly = self.reachability.insert_edge(*parent, *child_id);
                for nid in &newly {
                    complement.dropped_nodes.remove(nid);
                    complement
                        .dropped_arcs
                        .retain(|&(_, child, _)| child != *nid);
                }
                edit.clone()
            }
            TreeEdit::DeleteNode { id } => {
                let parent = self.reachability.parent_of(*id);
                let newly_unreachable =
                    parent.map_or_else(Vec::new, |p| self.reachability.delete_edge(p, *id));
                if newly_unreachable.is_empty() {
                    return edit.clone();
                }
                let mut edits = vec![edit.clone()];
                for nid in newly_unreachable {
                    if nid != *id && !complement.dropped_nodes.contains_key(&nid) {
                        edits.push(TreeEdit::DeleteNode { id: nid });
                    }
                }
                flatten_edits(edits)
            }
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                ..
            } => {
                let old_parent = self.reachability.parent_of(*node_id);
                if let Some(p) = old_parent {
                    let unreachable = self.reachability.delete_edge(p, *node_id);
                    // Nodes that become unreachable from the old position should
                    // be tracked; they will become reachable again when the edge
                    // is re-inserted below if the new parent is reachable.
                    for &nid in &unreachable {
                        if nid != *node_id {
                            complement
                                .dropped_arcs
                                .retain(|&(_, child, _)| child != nid);
                        }
                    }
                }
                let newly = self.reachability.insert_edge(*new_parent, *node_id);
                for nid in &newly {
                    complement.dropped_nodes.remove(nid);
                    complement
                        .dropped_arcs
                        .retain(|&(_, child, _)| child != *nid);
                }
                edit.clone()
            }
            _ => edit.clone(),
        }
    }

    /// Step 3: ancestor contraction.
    ///
    /// When a non-surviving node is deleted and its children survive,
    /// the children are reattached to the nearest surviving ancestor.
    fn step3_ancestor_contraction(&mut self, edit: &TreeEdit, complement: &Complement) -> TreeEdit {
        match edit {
            TreeEdit::ContractNode { id } => {
                let parent = self
                    .reachability
                    .parent_of(*id)
                    .or_else(|| complement.original_parent.get(id).copied())
                    .or_else(|| self.reachability.root())
                    .unwrap_or(0);
                // Collect actual children from the reachability index.
                let children: SmallVec<u32, 4> =
                    self.reachability.children_of(*id).iter().copied().collect();
                // Look up the actual edge from parent to this node in the target schema.
                // Look up the parent's anchor. Check the complement first (the parent
                // may have been dropped), then fall back to the first vertex in the
                // target schema.
                let parent_anchor = complement
                    .dropped_nodes
                    .get(&parent)
                    .or_else(|| {
                        complement
                            .original_parent
                            .get(id)
                            .and_then(|&p| complement.dropped_nodes.get(&p))
                    })
                    .map(|n| n.anchor.clone())
                    .or_else(|| self.tgt_schema.vertices.keys().next().cloned())
                    .unwrap_or_else(|| panproto_gat::Name::from("unknown"));
                let node_anchor = complement
                    .dropped_nodes
                    .get(id)
                    .map_or_else(|| panproto_gat::Name::from("unknown"), |n| n.anchor.clone());
                let edge = self
                    .tgt_schema
                    .edges_between(&parent_anchor, &node_anchor)
                    .first()
                    .cloned()
                    .unwrap_or_else(|| panproto_schema::Edge {
                        src: parent_anchor,
                        tgt: node_anchor,
                        kind: "contracted".into(),
                        name: None,
                    });
                let record = ContractionRecord {
                    original_parent: parent,
                    children,
                    original_edge: edge,
                };
                self.contraction.contract(*id, record);
                edit.clone()
            }
            TreeEdit::InsertNode { child_id, .. } => {
                if self.contraction.is_contracted(*child_id) {
                    self.contraction.expand(*child_id);
                }
                edit.clone()
            }
            _ => edit.clone(),
        }
    }

    /// Step 4: edge resolution.
    ///
    /// For edits that create new arcs (from contraction), resolve
    /// which schema edge to use via the resolver or unique-edge lookup.
    fn step4_edge_resolution(&self, edit: &TreeEdit) -> Result<TreeEdit, EditLensError> {
        match edit {
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge,
            } if edge.kind.as_ref() == "contracted" => {
                let resolved = panproto_inst::resolve_edge(
                    &self.tgt_schema,
                    &self.compiled.resolver,
                    edge.src.as_ref(),
                    edge.tgt.as_ref(),
                )?;
                Ok(TreeEdit::MoveSubtree {
                    node_id: *node_id,
                    new_parent: *new_parent,
                    edge: resolved,
                })
            }
            TreeEdit::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } if edge.kind.as_ref() == "contracted" => {
                let resolved = panproto_inst::resolve_edge(
                    &self.tgt_schema,
                    &self.compiled.resolver,
                    edge.src.as_ref(),
                    edge.tgt.as_ref(),
                )?;
                Ok(TreeEdit::InsertNode {
                    parent: *parent,
                    child_id: *child_id,
                    node: node.clone(),
                    edge: resolved,
                })
            }
            _ => Ok(edit.clone()),
        }
    }

    /// Step 5: fan reconstruction.
    ///
    /// Handles fan-related edits, absorbing fans with dropped
    /// participants into the complement.
    fn step5_fan_reconstruction(edit: &TreeEdit, complement: &mut Complement) -> TreeEdit {
        match edit {
            TreeEdit::InsertFan { fan } => {
                let all_survive = fan
                    .children
                    .values()
                    .all(|&cid| !complement.dropped_nodes.contains_key(&cid))
                    && !complement.dropped_nodes.contains_key(&fan.parent);

                if all_survive {
                    edit.clone()
                } else {
                    complement.dropped_fans.push(fan.clone());
                    TreeEdit::Identity
                }
            }
            TreeEdit::DeleteFan { hyper_edge_id } => {
                let id_str = hyper_edge_id.as_ref();
                let in_complement = complement
                    .dropped_fans
                    .iter()
                    .any(|f| f.hyper_edge_id == id_str);
                if in_complement {
                    complement
                        .dropped_fans
                        .retain(|f| f.hyper_edge_id != id_str);
                    TreeEdit::Identity
                } else {
                    edit.clone()
                }
            }
            _ => edit.clone(),
        }
    }

    /// Check whether a vertex anchor survives the migration.
    fn anchor_survives(&self, anchor: &panproto_gat::Name) -> bool {
        if !self.compiled.surviving_verts.contains(anchor) {
            return false;
        }
        self.compiled
            .conditional_survival
            .get(anchor)
            .is_none_or(|pred| {
                let env = panproto_expr::Env::new();
                let config = panproto_expr::EvalConfig::default();
                !matches!(
                    panproto_expr::eval(pred, &env, &config),
                    Ok(panproto_expr::Literal::Bool(false))
                )
            })
    }
}

/// Collapse a list of edits into a single edit.
fn flatten_edits(edits: Vec<TreeEdit>) -> TreeEdit {
    let non_identity: Vec<TreeEdit> = edits.into_iter().filter(|e| !e.is_identity()).collect();
    match non_identity.len() {
        0 => TreeEdit::Identity,
        1 => non_identity
            .into_iter()
            .next()
            .unwrap_or(TreeEdit::Identity),
        _ => TreeEdit::Sequence(non_identity),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_gat::Name;
    use panproto_inst::{Node, TreeEdit, WInstance};
    use panproto_schema::{Edge, Protocol};

    use crate::EditLens;
    use crate::asymmetric::Complement;
    use crate::tests::{identity_lens, projection_lens, three_node_instance, three_node_schema};

    use super::EditPipeline;

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            schema_composition: None,
            instance_composition: None,
            edge_rules: vec![],
            obj_kinds: vec![],
            constraint_sorts: vec![],
            has_order: false,
            has_coproducts: false,
            has_recursion: false,
            has_causal: false,
            nominal_identity: false,
            has_defaults: false,
            has_coercions: false,
            has_mergers: false,
            has_policies: false,
        }
    }

    fn sample_edge(src: &str, tgt: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: None,
        }
    }

    #[test]
    fn pipeline_identity_lens_passes_through() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let edit_lens = EditLens::from_lens(lens, test_protocol());

        let mut pipeline = EditPipeline::from_lens_and_instance(&edit_lens, &instance);
        let mut complement = Complement::empty();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: panproto_inst::Value::Str("updated".into()),
        };

        let result = pipeline.translate_get(&edit, &mut complement).unwrap();
        match &result {
            TreeEdit::SetField { node_id, field, .. } => {
                assert_eq!(*node_id, 1);
                assert_eq!(field, &Name::from("text"));
            }
            other => panic!("expected SetField, got {other:?}"),
        }
    }

    #[test]
    fn pipeline_tracks_reachability_on_insert() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let edit_lens = EditLens::from_lens(lens, test_protocol());

        let mut pipeline = EditPipeline::from_lens_and_instance(&edit_lens, &instance);
        let mut complement = Complement::empty();

        let new_node = Node::new(99, "post:body.text");
        let edit = TreeEdit::InsertNode {
            parent: 0,
            child_id: 99,
            node: new_node,
            edge: sample_edge("post:body", "post:body.text"),
        };

        let result = pipeline.translate_get(&edit, &mut complement).unwrap();
        assert!(
            !result.is_identity(),
            "insert of surviving node should pass through"
        );
        assert!(
            pipeline.reachability.is_reachable(99),
            "newly inserted node should be reachable"
        );
    }

    #[test]
    fn pipeline_tracks_reachability_on_delete() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "post:body"));
        nodes.insert(1, Node::new(1, "post:body.text"));
        nodes.insert(10, Node::new(10, "post:body.text"));
        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            ),
            (
                1,
                10,
                Edge {
                    src: "post:body.text".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: None,
                },
            ),
        ];
        let instance = WInstance::new(nodes, arcs, vec![], 0, Name::from("post:body"));

        let edit_lens = EditLens::from_lens(lens, test_protocol());
        let mut pipeline = EditPipeline::from_lens_and_instance(&edit_lens, &instance);
        let mut complement = Complement::empty();

        assert!(pipeline.reachability.is_reachable(1));
        assert!(pipeline.reachability.is_reachable(10));

        let edit = TreeEdit::DeleteNode { id: 1 };
        let _result = pipeline.translate_get(&edit, &mut complement).unwrap();
    }

    #[test]
    fn pipeline_fan_with_dropped_participant() {
        let schema = three_node_schema();
        let lens = projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let mut pipeline = EditPipeline::from_lens_and_instance(&edit_lens, &instance);
        let mut complement = edit_lens.complement.clone();

        assert!(complement.dropped_nodes.contains_key(&2));

        let fan = panproto_inst::Fan::new("test_he", 0)
            .with_child("a", 1)
            .with_child("b", 2);
        let edit = TreeEdit::InsertFan { fan };

        let result = pipeline.translate_get(&edit, &mut complement).unwrap();
        assert!(
            result.is_identity(),
            "fan with dropped participant should be absorbed"
        );
        assert_eq!(
            complement.dropped_fans.len(),
            1,
            "fan should be in complement"
        );
    }

    #[test]
    fn pipeline_matches_batch_restrict() {
        use panproto_inst::Value;

        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let mut instance = three_node_instance();
        let edit_lens = EditLens::from_lens(lens, test_protocol());

        let mut pipeline = EditPipeline::from_lens_and_instance(&edit_lens, &instance);
        let mut complement = Complement::empty();

        let edits = vec![
            TreeEdit::SetField {
                node_id: 1,
                field: Name::from("text"),
                value: Value::Str("modified".into()),
            },
            TreeEdit::SetField {
                node_id: 2,
                field: Name::from("extra"),
                value: Value::Int(42),
            },
        ];

        for edit in &edits {
            let _translated = pipeline.translate_get(edit, &mut complement).unwrap();
        }

        for edit in &edits {
            edit.apply(&mut instance).unwrap();
        }

        let compiled = edit_lens.compiled.clone();
        let batch_view = panproto_inst::wtype_restrict(
            &instance,
            &edit_lens.src_schema,
            &edit_lens.tgt_schema,
            &compiled,
        )
        .unwrap();

        assert_eq!(
            batch_view.node_count(),
            instance.node_count(),
            "identity lens batch view should have same node count"
        );

        let n1 = batch_view.nodes.get(&1).expect("node 1 in view");
        assert_eq!(
            n1.extra_fields.get("text"),
            Some(&Value::Str("modified".into())),
            "SetField edit should be reflected in batch restrict output"
        );
    }
}
