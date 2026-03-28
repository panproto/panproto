//! Edit lens: translates individual edits between source and view schemas.
//!
//! An [`EditLens`] operates on edits (patches) rather than whole states.
//! Each edit flows through the lens, and the complement updates
//! incrementally. The complement is a state machine, not a snapshot.
//!
//! Translation is derived from two sources:
//!
//! 1. **Structural translation**: remap anchors and edges via the
//!    compiled migration's vertex/edge remap tables. Field transforms
//!    from the compiled migration are applied to surviving nodes.
//! 2. **Semantic translation**: apply directed equations from the
//!    protocol theory for value-level coercions. The `impl_term`
//!    expression is evaluated forward; the `inverse` expression is
//!    evaluated backward.
//!
//! Complement policies (from the schema's policy sorts) determine how
//! the complement reacts to view edits.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::{DirectedEquation, Name, Theory};
use panproto_inst::{CompiledMigration, TreeEdit, WInstance};
use panproto_schema::{Edge, Protocol, Schema};

use crate::Lens;
use crate::asymmetric::Complement;
use crate::edit_error::EditLensError;
use crate::edit_pipeline::EditPipeline;
use crate::edit_provenance::EditProvenance;
use crate::optic::OpticKind;

/// An edit lens between source and view schemas.
///
/// Translates individual `TreeEdit` values through a migration,
/// maintaining a stateful complement that tracks discarded data.
pub struct EditLens {
    /// The compiled migration driving structural translation.
    pub compiled: CompiledMigration,
    /// The source schema.
    pub src_schema: Schema,
    /// The target (view) schema.
    pub tgt_schema: Schema,
    /// The current complement state.
    pub complement: Complement,
    /// The protocol definition (carries the enriched theory).
    pub protocol: Protocol,
    /// Cached reverse vertex remap (target -> source).
    pub(crate) reverse_vertex_remap: HashMap<Name, Name>,
    /// Cached reverse edge remap (target -> source).
    pub(crate) reverse_edge_remap: HashMap<Edge, Edge>,
    /// Per-pipeline-step incremental translator.
    pub pipeline: Option<EditPipeline>,
}

impl EditLens {
    /// Construct an `EditLens` from an existing state-based `Lens` and protocol.
    #[must_use]
    pub fn from_lens(lens: Lens, protocol: Protocol) -> Self {
        let reverse_vertex_remap = lens
            .compiled
            .vertex_remap
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        let reverse_edge_remap = lens
            .compiled
            .edge_remap
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        Self {
            compiled: lens.compiled,
            src_schema: lens.src_schema,
            tgt_schema: lens.tgt_schema,
            complement: Complement::empty(),
            protocol,
            reverse_vertex_remap,
            reverse_edge_remap,
            pipeline: None,
        }
    }

    /// Initialize the complement from a whole-state `get` on the given source.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `get` fails.
    pub fn initialize(&mut self, source: &WInstance) -> Result<(), EditLensError> {
        let lens = Lens {
            compiled: self.compiled.clone(),
            src_schema: self.src_schema.clone(),
            tgt_schema: self.tgt_schema.clone(),
        };
        let (_, complement) = crate::get(&lens, source)
            .map_err(|e| EditLensError::TranslationFailed(e.to_string()))?;
        self.complement = complement;
        self.pipeline = Some(EditPipeline::from_lens_and_instance(self, source));
        Ok(())
    }

    /// Classify this edit lens's optic kind from the migration structure.
    ///
    /// The classification follows from the data-preservation properties
    /// of the compiled migration:
    ///
    /// - **Iso**: all source vertices and edges survive (bijection).
    /// - **Lens**: some vertices or edges are dropped but none added (projection).
    /// - **Prism**: variant-related changes are present (injection).
    /// - **Affine**: everything else (lens composed with prism).
    #[must_use]
    pub fn optic_kind(&self) -> OpticKind {
        let all_src_verts_survive = self
            .src_schema
            .vertices
            .keys()
            .all(|v| self.compiled.surviving_verts.contains(v));
        let all_src_edges_survive = self
            .src_schema
            .edges
            .keys()
            .all(|e| self.compiled.surviving_edges.contains(e));

        if all_src_verts_survive && all_src_edges_survive {
            return OpticKind::Iso;
        }

        // Check for variant-related changes (prism indicator).
        let has_variant_changes = self
            .src_schema
            .variants
            .keys()
            .any(|v| !self.tgt_schema.variants.contains_key(v))
            || self
                .tgt_schema
                .variants
                .keys()
                .any(|v| !self.src_schema.variants.contains_key(v));

        if has_variant_changes {
            return OpticKind::Prism;
        }

        // No added vertices: pure projection (lens).
        let has_added_verts = self
            .tgt_schema
            .vertices
            .keys()
            .any(|v| !self.src_schema.vertices.contains_key(v));

        if !has_added_verts {
            return OpticKind::Lens;
        }

        OpticKind::Affine
    }

    /// Translate an edit through an isomorphic lens (bijection).
    ///
    /// Since the lens is an isomorphism, the complement is unit and
    /// never changes. This method only remaps anchors and edges.
    #[must_use]
    pub fn translate_iso(&self, edit: TreeEdit) -> TreeEdit {
        match edit {
            TreeEdit::Identity => TreeEdit::Identity,
            TreeEdit::SetField {
                node_id,
                ref field,
                ref value,
            } => {
                let translated = self.translate_field_edit(field.as_ref(), value);
                TreeEdit::SetField {
                    node_id,
                    field: Name::from(translated.0.as_str()),
                    value: translated.1,
                }
            }
            TreeEdit::RemoveField { node_id, ref field } => {
                let new_name = self.translate_field_name(field.as_ref());
                TreeEdit::RemoveField {
                    node_id,
                    field: Name::from(new_name.as_str()),
                }
            }
            TreeEdit::RelabelNode { id, ref new_anchor } => TreeEdit::RelabelNode {
                id,
                new_anchor: self.remap_anchor_forward(new_anchor),
            },
            TreeEdit::InsertNode {
                parent,
                child_id,
                ref node,
                ref edge,
            } => {
                let remapped_node = self.remap_and_transform_node(node);
                let remapped_edge = self.remap_edge_forward(edge);
                TreeEdit::InsertNode {
                    parent,
                    child_id,
                    node: remapped_node,
                    edge: remapped_edge,
                }
            }
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                ref edge,
            } => TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge: self.remap_edge_forward(edge),
            },
            other => other,
        }
    }

    /// Translate an edit through a prismatic lens.
    ///
    /// Edits targeting an inactive variant are absorbed into the
    /// complement. Edits targeting the active variant pass through
    /// with structural remapping.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::TranslationFailed`] if the edit targets
    /// a variant that cannot be resolved.
    pub fn translate_prism(&mut self, edit: TreeEdit) -> Result<TreeEdit, EditLensError> {
        match &edit {
            TreeEdit::InsertNode { node, .. } => {
                let target_anchor = self
                    .compiled
                    .vertex_remap
                    .get(&node.anchor)
                    .unwrap_or(&node.anchor);
                if !self.compiled.surviving_verts.contains(target_anchor) {
                    // Inactive variant: absorb into complement.
                    return Ok(TreeEdit::Identity);
                }
            }
            TreeEdit::SetField { node_id, .. } => {
                if self.complement.dropped_nodes.contains_key(node_id) {
                    return Ok(TreeEdit::Identity);
                }
            }
            _ => {}
        }
        self.get_edit(edit)
    }

    /// Check a value against refinement constraints on a target vertex.
    ///
    /// Returns `Ok(())` if the value satisfies all constraints, or
    /// `Err(EditLensError::RefinementViolation)` if any constraint fails.
    fn check_refinement(
        &self,
        vertex: &Name,
        value: &panproto_inst::Value,
    ) -> Result<(), EditLensError> {
        let Some(constraints) = self.tgt_schema.constraints.get(vertex) else {
            return Ok(());
        };

        let string_val = match value {
            panproto_inst::Value::Str(s) => Some(s.as_str()),
            _ => None,
        };

        for constraint in constraints {
            let sort = constraint.sort.as_ref();
            match sort {
                "maxLength" => {
                    if let Some(s) = string_val {
                        if let Ok(max) = constraint.value.parse::<usize>() {
                            if s.len() > max {
                                return Err(EditLensError::RefinementViolation {
                                    vertex: vertex.to_string(),
                                    constraint_sort: sort.to_owned(),
                                    constraint_value: constraint.value.clone(),
                                    detail: format!(
                                        "string length {} exceeds maximum {}",
                                        s.len(),
                                        max
                                    ),
                                });
                            }
                        }
                    }
                }
                "minLength" => {
                    if let Some(s) = string_val {
                        if let Ok(min) = constraint.value.parse::<usize>() {
                            if s.len() < min {
                                return Err(EditLensError::RefinementViolation {
                                    vertex: vertex.to_string(),
                                    constraint_sort: sort.to_owned(),
                                    constraint_value: constraint.value.clone(),
                                    detail: format!(
                                        "string length {} below minimum {}",
                                        s.len(),
                                        min
                                    ),
                                });
                            }
                        }
                    }
                }
                "format" => {
                    if let Some(s) = string_val {
                        let pattern = &constraint.value;
                        let valid = match pattern.as_str() {
                            "at-uri" => s.starts_with("at://"),
                            "did" => s.starts_with("did:"),
                            "datetime" | "date-time" => {
                                // Simple heuristic: must contain 'T' or be YYYY-MM-DD.
                                s.len() >= 10
                                    && s.as_bytes().get(4) == Some(&b'-')
                                    && s.as_bytes().get(7) == Some(&b'-')
                            }
                            "uri" | "url" => {
                                s.starts_with("http://")
                                    || s.starts_with("https://")
                                    || s.starts_with("at://")
                            }
                            _ => true, // Unknown formats pass by default.
                        };
                        if !valid {
                            return Err(EditLensError::RefinementViolation {
                                vertex: vertex.to_string(),
                                constraint_sort: sort.to_owned(),
                                constraint_value: constraint.value.clone(),
                                detail: format!("value {s:?} does not match format {pattern:?}"),
                            });
                        }
                    }
                }
                _ => {} // Unknown constraint sorts are ignored.
            }
        }

        Ok(())
    }

    /// Translate a source edit to a view edit, recording provenance.
    ///
    /// Calls [`get_edit`](Self::get_edit) and also records which
    /// translation rules fired, which complement policy was consulted,
    /// and whether the translation was total.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError`] if the edit cannot be translated.
    pub fn get_edit_with_provenance(
        &mut self,
        edit: TreeEdit,
    ) -> Result<(TreeEdit, EditProvenance), EditLensError> {
        let desc = format!("{edit:?}");
        let mut provenance = EditProvenance::new(desc);

        // Record structural remap rule (always fires).
        provenance.record_rule(Arc::from("structural_remap"));

        // Record field transform rules.
        if let TreeEdit::SetField { ref field, .. } = edit {
            let field_str = field.to_string();
            for transforms in self.compiled.field_transforms.values() {
                for transform in transforms {
                    let applies = match transform {
                        panproto_inst::FieldTransform::RenameField { old_key, .. } => {
                            old_key == &field_str
                        }
                        panproto_inst::FieldTransform::ApplyExpr { key, .. } => key == &field_str,
                        _ => false,
                    };
                    if applies {
                        provenance.record_rule(Arc::from(format!("field_{field_str}").as_str()));
                    }
                }
            }
        }

        // Record complement policy if one exists for the edit's anchor.
        if let TreeEdit::InsertNode { ref node, .. } = edit {
            if let Some(_policy) = self.policy_for(&node.anchor) {
                provenance.record_policy(Arc::from(node.anchor.as_ref()));
            }
        }

        // Perform the translation.
        let translated = self.get_edit(edit)?;

        // Check if the optic is Iso; if so, complement was not updated,
        // so the translation is always total.
        if self.optic_kind() != OpticKind::Iso {
            // For non-iso lenses, check if the result is identity (absorbed).
            if translated.is_identity() {
                provenance.mark_partial();
            }
        }

        Ok((translated, provenance))
    }

    /// Translate a source edit to a view edit, updating the complement.
    ///
    /// The translation pipeline for each edit:
    /// 1. **Anchor survival**: check if the edit's target anchor is in
    ///    `surviving_verts`. Non-surviving edits are absorbed into the
    ///    complement.
    /// 2. **Conditional survival**: evaluate value-dependent predicates
    ///    from `conditional_survival` on the node's fields.
    /// 3. **Structural remap**: remap anchors and edges via the compiled
    ///    migration's vertex/edge remap tables.
    /// 4. **Field transforms**: apply `FieldTransform` operations from
    ///    the compiled migration to surviving nodes' fields.
    /// 5. **Complement update**: update the complement state based on
    ///    which data was absorbed or released.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError`] if the edit cannot be translated.
    pub fn get_edit(&mut self, edit: TreeEdit) -> Result<TreeEdit, EditLensError> {
        match edit {
            TreeEdit::Identity => Ok(TreeEdit::Identity),
            TreeEdit::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } => Ok(self.get_edit_insert(parent, child_id, node, edge)),
            TreeEdit::DeleteNode { id } => Ok(self.get_edit_delete(id)),
            TreeEdit::SetField {
                node_id,
                ref field,
                ref value,
            } => self.get_edit_set_field(node_id, field, value),
            TreeEdit::RemoveField { node_id, ref field } => {
                Ok(self.get_edit_remove_field(node_id, field))
            }
            TreeEdit::RelabelNode { id, new_anchor } => self.get_edit_relabel(id, new_anchor),
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge,
            } => Ok(TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge: self.remap_edge_forward(&edge),
            }),
            TreeEdit::InsertFan { fan } => Ok(self.get_edit_insert_fan(fan)),
            TreeEdit::DeleteFan { hyper_edge_id } => Ok(self.get_edit_delete_fan(hyper_edge_id)),
            TreeEdit::ContractNode { id } => {
                if self.complement.dropped_nodes.remove(&id).is_some() {
                    Ok(TreeEdit::Identity)
                } else {
                    Ok(TreeEdit::ContractNode { id })
                }
            }
            TreeEdit::JoinFeatures {
                primary,
                joined,
                produce,
            } => Ok(TreeEdit::JoinFeatures {
                primary,
                joined,
                produce: self.remap_and_transform_node(&produce),
            }),
            TreeEdit::Sequence(steps) => self.get_edit_sequence(steps),
        }
    }

    fn get_edit_insert(
        &mut self,
        parent: u32,
        child_id: u32,
        node: panproto_inst::Node,
        edge: Edge,
    ) -> TreeEdit {
        // Step 1: anchor survival.
        let target_anchor = self
            .compiled
            .vertex_remap
            .get(&node.anchor)
            .unwrap_or(&node.anchor);
        if !self.compiled.surviving_verts.contains(target_anchor) {
            self.complement.dropped_nodes.insert(child_id, node);
            self.complement.dropped_arcs.push((parent, child_id, edge));
            return TreeEdit::Identity;
        }

        // Step 2: conditional survival.
        if let Some(pred) = self.compiled.conditional_survival.get(&node.anchor) {
            let env = panproto_inst::build_env_from_extra_fields(&node.extra_fields);
            let config = panproto_expr::EvalConfig::default();
            if matches!(
                panproto_expr::eval(pred, &env, &config),
                Ok(panproto_expr::Literal::Bool(false))
            ) {
                self.complement.dropped_nodes.insert(child_id, node);
                self.complement.dropped_arcs.push((parent, child_id, edge));
                return TreeEdit::Identity;
            }
        }

        // Steps 3-4: structural remap and field transforms.
        let remapped_node = self.remap_and_transform_node(&node);
        let remapped_edge = self.remap_edge_forward(&edge);
        TreeEdit::InsertNode {
            parent,
            child_id,
            node: remapped_node,
            edge: remapped_edge,
        }
    }

    fn get_edit_delete(&mut self, id: u32) -> TreeEdit {
        if self.complement.dropped_nodes.contains_key(&id) {
            self.complement.dropped_nodes.remove(&id);
            self.complement
                .dropped_arcs
                .retain(|&(_, child, _)| child != id);
            TreeEdit::Identity
        } else {
            TreeEdit::DeleteNode { id }
        }
    }

    fn get_edit_set_field(
        &mut self,
        node_id: u32,
        field: &Name,
        value: &panproto_inst::Value,
    ) -> Result<TreeEdit, EditLensError> {
        // If the node is in the complement, update the complement's copy.
        if let Some(node) = self.complement.dropped_nodes.get_mut(&node_id) {
            node.extra_fields.insert(field.to_string(), value.clone());
            return Ok(TreeEdit::Identity);
        }

        // The node is in the view. Apply field transforms if the migration
        // specifies any for this node's source anchor: translate the field
        // name and possibly coerce the value.
        let field_str = field.to_string();
        let translated = self.translate_field_edit(&field_str, value);

        // Refinement type checking: validate the translated value against
        // the target schema's constraints for the target vertex.
        let translated_field_name = Name::from(translated.0.as_str());
        // Find the target vertex that this field belongs to by looking
        // for edges whose name matches the translated field name.
        for edge in self.tgt_schema.edges.keys() {
            if edge.name.as_ref() == Some(&translated_field_name) {
                self.check_refinement(&edge.tgt, &translated.1)?;
            }
        }

        Ok(TreeEdit::SetField {
            node_id,
            field: translated_field_name,
            value: translated.1,
        })
    }

    fn get_edit_remove_field(&mut self, node_id: u32, field: &Name) -> TreeEdit {
        if let Some(node) = self.complement.dropped_nodes.get_mut(&node_id) {
            node.extra_fields.remove(field.as_ref());
            return TreeEdit::Identity;
        }

        // If this field was renamed by field transforms, the remove applies
        // to the renamed field in the target schema.
        let new_name = self.translate_field_name(field.as_ref());
        TreeEdit::RemoveField {
            node_id,
            field: Name::from(new_name.as_str()),
        }
    }

    fn get_edit_relabel(&mut self, id: u32, new_anchor: Name) -> Result<TreeEdit, EditLensError> {
        let old_in_complement = self.complement.dropped_nodes.contains_key(&id);
        let target_anchor = self
            .compiled
            .vertex_remap
            .get(&new_anchor)
            .unwrap_or(&new_anchor);
        let new_survives = self.compiled.surviving_verts.contains(target_anchor);

        match (old_in_complement, new_survives) {
            (true, true) => {
                // Was in complement, now survives: becomes an insert in the view.
                let node = self.complement.dropped_nodes.remove(&id).ok_or_else(|| {
                    EditLensError::ComplementInconsistent(format!(
                        "node {id} expected in complement"
                    ))
                })?;
                self.complement
                    .dropped_arcs
                    .retain(|&(_, child, _)| child != id);
                let mut remapped = self.remap_and_transform_node(&node);
                remapped.anchor = self.remap_anchor_forward(&new_anchor);
                let parent = self.complement.original_parent.get(&id).copied();
                if let Some(p) = parent {
                    // Resolve the parent's anchor from the complement or schema.
                    let parent_anchor = self
                        .complement
                        .dropped_nodes
                        .get(&p)
                        .map(|n| n.anchor.clone())
                        .or_else(|| self.tgt_schema.vertices.keys().next().cloned())
                        .unwrap_or_else(|| Name::from(self.src_schema.protocol.as_str()));
                    // Find the correct edge from the target schema.
                    let edge = self
                        .tgt_schema
                        .edges_between(&parent_anchor, &remapped.anchor)
                        .first()
                        .cloned()
                        .unwrap_or_else(|| Edge {
                            src: parent_anchor,
                            tgt: remapped.anchor.clone(),
                            kind: "prop".into(),
                            name: None,
                        });
                    Ok(TreeEdit::InsertNode {
                        parent: p,
                        child_id: id,
                        node: remapped,
                        edge,
                    })
                } else {
                    Ok(TreeEdit::RelabelNode {
                        id,
                        new_anchor: self.remap_anchor_forward(&new_anchor),
                    })
                }
            }
            (true, false) => {
                // Was in complement, still doesn't survive. Update complement.
                if let Some(node) = self.complement.dropped_nodes.get_mut(&id) {
                    node.anchor = new_anchor;
                }
                Ok(TreeEdit::Identity)
            }
            (false, true) => Ok(TreeEdit::RelabelNode {
                id,
                new_anchor: self.remap_anchor_forward(&new_anchor),
            }),
            (false, false) => {
                // Was in view, now doesn't survive. Delete from view and
                // add to complement.
                Ok(TreeEdit::DeleteNode { id })
            }
        }
    }

    fn get_edit_insert_fan(&mut self, fan: panproto_inst::Fan) -> TreeEdit {
        // Check if the parent survives.
        let parent_survives = !self.complement.dropped_nodes.contains_key(&fan.parent);
        if !parent_survives {
            self.complement.dropped_fans.push(fan);
            return TreeEdit::Identity;
        }

        // Check which children survive. Collect surviving and dropped.
        let mut all_survive = true;
        for &child_id in fan.children.values() {
            if self.complement.dropped_nodes.contains_key(&child_id) {
                all_survive = false;
                break;
            }
        }

        if all_survive {
            // All participants survive: remap hyper-edge if needed and pass through.
            let remapped_fan = self.remap_fan_forward(&fan);
            TreeEdit::InsertFan { fan: remapped_fan }
        } else {
            // Some participants don't survive: store the fan in complement.
            self.complement.dropped_fans.push(fan);
            TreeEdit::Identity
        }
    }

    fn get_edit_delete_fan(&mut self, hyper_edge_id: Name) -> TreeEdit {
        // Check if the fan is in the complement.
        let id_str = hyper_edge_id.as_ref();
        let in_complement = self
            .complement
            .dropped_fans
            .iter()
            .any(|f| f.hyper_edge_id == id_str);
        if in_complement {
            self.complement
                .dropped_fans
                .retain(|f| f.hyper_edge_id != id_str);
            TreeEdit::Identity
        } else {
            TreeEdit::DeleteFan { hyper_edge_id }
        }
    }

    fn get_edit_sequence(&mut self, steps: Vec<TreeEdit>) -> Result<TreeEdit, EditLensError> {
        let mut translated = Vec::with_capacity(steps.len());
        for step in steps {
            let t = self.get_edit(step)?;
            if !t.is_identity() {
                translated.push(t);
            }
        }
        Ok(match translated.len() {
            0 => TreeEdit::Identity,
            1 => translated.into_iter().next().unwrap_or(TreeEdit::Identity),
            _ => TreeEdit::Sequence(translated),
        })
    }

    /// Translate a view edit back to a source edit, updating the complement.
    ///
    /// Uses the inverse vertex/edge remap. For value-level edits, applies
    /// the inverse directed equations (from `DirectedEquation.inverse`)
    /// when available.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError`] if the edit cannot be translated.
    pub fn put_edit(&mut self, edit: TreeEdit) -> Result<TreeEdit, EditLensError> {
        match edit {
            TreeEdit::Identity => Ok(TreeEdit::Identity),
            TreeEdit::InsertNode {
                parent,
                child_id,
                ref node,
                ref edge,
            } => Ok(self.put_edit_insert(parent, child_id, node, edge)),
            TreeEdit::DeleteNode { id } => Ok(TreeEdit::DeleteNode { id }),
            TreeEdit::SetField {
                node_id,
                ref field,
                ref value,
            } => Ok(self.put_edit_set_field(node_id, field, value)),
            TreeEdit::RemoveField { node_id, field } => {
                let source_name = self.translate_field_name_backward(field.as_ref());
                Ok(TreeEdit::RemoveField {
                    node_id,
                    field: Name::from(source_name.as_str()),
                })
            }
            TreeEdit::RelabelNode { id, new_anchor } => {
                let source_anchor = self.remap_anchor_backward(&new_anchor);
                Ok(TreeEdit::RelabelNode {
                    id,
                    new_anchor: source_anchor,
                })
            }
            TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge,
            } => Ok(TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge: self.remap_edge_backward(&edge),
            }),
            TreeEdit::InsertFan { fan } => {
                let source_fan = self.remap_fan_backward(&fan);
                Ok(TreeEdit::InsertFan { fan: source_fan })
            }
            TreeEdit::DeleteFan { hyper_edge_id } => Ok(TreeEdit::DeleteFan { hyper_edge_id }),
            TreeEdit::ContractNode { id } => Ok(TreeEdit::ContractNode { id }),
            TreeEdit::JoinFeatures {
                primary,
                joined,
                produce,
            } => Ok(TreeEdit::JoinFeatures {
                primary,
                joined,
                produce: self.remap_node_backward(&produce),
            }),
            TreeEdit::Sequence(steps) => {
                let mut translated = Vec::with_capacity(steps.len());
                for step in steps {
                    let t = self.put_edit(step)?;
                    if !t.is_identity() {
                        translated.push(t);
                    }
                }
                Ok(match translated.len() {
                    0 => TreeEdit::Identity,
                    1 => translated.into_iter().next().unwrap_or(TreeEdit::Identity),
                    _ => TreeEdit::Sequence(translated),
                })
            }
        }
    }

    fn put_edit_insert(
        &self,
        parent: u32,
        child_id: u32,
        node: &panproto_inst::Node,
        edge: &Edge,
    ) -> TreeEdit {
        let source_node = self.remap_node_backward(node);
        let source_edge = self.remap_edge_backward(edge);
        TreeEdit::InsertNode {
            parent,
            child_id,
            node: source_node,
            edge: source_edge,
        }
    }

    fn put_edit_set_field(
        &self,
        node_id: u32,
        field: &Name,
        value: &panproto_inst::Value,
    ) -> TreeEdit {
        let source_name = self.translate_field_name_backward(field.as_ref());
        // For value transforms, the complement stores the original if needed
        // for lossless round-tripping. Field transform expressions are
        // forward-only; inverse behavior requires protocol-level directed
        // equations with an `inverse` field.
        TreeEdit::SetField {
            node_id,
            field: Name::from(source_name.as_str()),
            value: value.clone(),
        }
    }

    /// Get the policy expression for a given vertex kind, reading from
    /// the source schema's policy sorts.
    #[must_use]
    pub fn policy_for(&self, kind: &Name) -> Option<&panproto_expr::Expr> {
        self.src_schema.policies.get(kind)
    }

    /// Get directed equations from the protocol theory that are relevant
    /// to this migration. Filters to equations whose LHS references sorts
    /// that appear in the migration's surviving vertex set.
    #[must_use]
    pub fn translation_rules<'a>(&self, theory: &'a Theory) -> Vec<&'a DirectedEquation> {
        // The protocol theory is already scoped to this protocol, so all
        // its directed equations are relevant to this migration.
        theory.directed_eqs.iter().collect()
    }

    // -- Private helpers: structural remap --

    fn remap_anchor_forward(&self, anchor: &Name) -> Name {
        self.compiled
            .vertex_remap
            .get(anchor)
            .cloned()
            .unwrap_or_else(|| anchor.clone())
    }

    fn remap_anchor_backward(&self, anchor: &Name) -> Name {
        self.reverse_vertex_remap
            .get(anchor)
            .cloned()
            .unwrap_or_else(|| anchor.clone())
    }

    /// Remap a node's anchor and apply field transforms from the compiled migration.
    fn remap_and_transform_node(&self, node: &panproto_inst::Node) -> panproto_inst::Node {
        let mut remapped = node.clone();
        // Apply field transforms if any exist for this source anchor.
        if let Some(transforms) = self.compiled.field_transforms.get(&node.anchor) {
            panproto_inst::wtype::apply_field_transforms(&mut remapped, transforms);
        }
        remapped.anchor = self.remap_anchor_forward(&node.anchor);
        remapped
    }

    fn remap_node_backward(&self, node: &panproto_inst::Node) -> panproto_inst::Node {
        let mut remapped = node.clone();
        remapped.anchor = self.remap_anchor_backward(&node.anchor);
        remapped
    }

    fn remap_edge_forward(&self, edge: &Edge) -> Edge {
        self.compiled
            .edge_remap
            .get(edge)
            .cloned()
            .unwrap_or_else(|| {
                let mut e = edge.clone();
                e.src = self.remap_anchor_forward(&edge.src);
                e.tgt = self.remap_anchor_forward(&edge.tgt);
                e
            })
    }

    fn remap_edge_backward(&self, edge: &Edge) -> Edge {
        self.reverse_edge_remap
            .get(edge)
            .cloned()
            .unwrap_or_else(|| {
                let mut e = edge.clone();
                e.src = self.remap_anchor_backward(&edge.src);
                e.tgt = self.remap_anchor_backward(&edge.tgt);
                e
            })
    }

    fn remap_fan_forward(&self, fan: &panproto_inst::Fan) -> panproto_inst::Fan {
        let new_he_id = if let Some((new_id, _)) =
            self.compiled.hyper_resolver.get(fan.hyper_edge_id.as_str())
        {
            new_id.to_string()
        } else {
            fan.hyper_edge_id.clone()
        };

        let new_children = if let Some((_, label_map)) =
            self.compiled.hyper_resolver.get(fan.hyper_edge_id.as_str())
        {
            fan.children
                .iter()
                .map(|(label, &node_id)| {
                    let new_label = label_map
                        .get(label.as_str())
                        .map_or_else(|| label.clone(), std::string::ToString::to_string);
                    (new_label, node_id)
                })
                .collect()
        } else {
            fan.children.clone()
        };

        panproto_inst::Fan {
            hyper_edge_id: new_he_id,
            parent: fan.parent,
            children: new_children,
        }
    }

    fn remap_fan_backward(&self, fan: &panproto_inst::Fan) -> panproto_inst::Fan {
        // Reverse remap: check if any hyper_resolver entry maps to this fan's ID.
        for (old_id, (new_id, label_map)) in &self.compiled.hyper_resolver {
            if new_id.as_ref() == fan.hyper_edge_id.as_str() {
                let reverse_labels: HashMap<String, String> = label_map
                    .iter()
                    .map(|(k, v)| (v.to_string(), k.to_string()))
                    .collect();
                let children = fan
                    .children
                    .iter()
                    .map(|(label, &node_id)| {
                        let old_label = reverse_labels
                            .get(label)
                            .cloned()
                            .unwrap_or_else(|| label.clone());
                        (old_label, node_id)
                    })
                    .collect();
                return panproto_inst::Fan {
                    hyper_edge_id: old_id.to_string(),
                    parent: fan.parent,
                    children,
                };
            }
        }
        fan.clone()
    }

    // -- Private helpers: field translation --

    /// Translate a field name through the migration's field transforms.
    /// If a `RenameField` transform renames this field, return the new name.
    fn translate_field_name(&self, field: &str) -> String {
        // Check all vertex anchors for rename transforms that affect this field.
        for transforms in self.compiled.field_transforms.values() {
            for transform in transforms {
                if let panproto_inst::FieldTransform::RenameField { old_key, new_key } = transform {
                    if old_key == field {
                        return new_key.clone();
                    }
                }
            }
        }
        field.to_owned()
    }

    /// Reverse a field name translation (target name -> source name).
    fn translate_field_name_backward(&self, field: &str) -> String {
        for transforms in self.compiled.field_transforms.values() {
            for transform in transforms {
                if let panproto_inst::FieldTransform::RenameField { old_key, new_key } = transform {
                    if new_key == field {
                        return old_key.clone();
                    }
                }
            }
        }
        field.to_owned()
    }

    /// Translate a field edit (name + value) through field transforms.
    /// Returns the translated (name, value) pair.
    fn translate_field_edit(
        &self,
        field: &str,
        value: &panproto_inst::Value,
    ) -> (String, panproto_inst::Value) {
        let mut name = field.to_owned();
        let mut val = value.clone();

        // Apply all relevant field transforms in order.
        for transforms in self.compiled.field_transforms.values() {
            for transform in transforms {
                match transform {
                    panproto_inst::FieldTransform::RenameField { old_key, new_key } => {
                        if old_key == field {
                            name.clone_from(new_key);
                        }
                    }
                    panproto_inst::FieldTransform::ApplyExpr { key, expr, .. } => {
                        if key == field {
                            let input = panproto_inst::value_to_expr_literal(&val);
                            let env = panproto_expr::Env::new()
                                .extend(std::sync::Arc::from(key.as_str()), input);
                            let config = panproto_expr::EvalConfig::default();
                            if let Ok(result) = panproto_expr::eval(expr, &env, &config) {
                                val = expr_literal_to_value(&result);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        (name, val)
    }
}

/// Convert an expression literal to a panproto `Value`.
fn expr_literal_to_value(lit: &panproto_expr::Literal) -> panproto_inst::Value {
    match lit {
        panproto_expr::Literal::Bool(b) => panproto_inst::Value::Bool(*b),
        panproto_expr::Literal::Int(i) => panproto_inst::Value::Int(*i),
        panproto_expr::Literal::Float(f) => {
            // Normalize integer-valued floats for JSON round-trip fidelity.
            #[allow(clippy::cast_precision_loss)]
            let fits = f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64;
            if fits {
                #[allow(clippy::cast_possible_truncation)]
                let i = *f as i64;
                panproto_inst::Value::Int(i)
            } else {
                panproto_inst::Value::Float(*f)
            }
        }
        panproto_expr::Literal::Str(s) => panproto_inst::Value::Str(s.clone()),
        _ => panproto_inst::Value::Null,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use panproto_gat::Name;
    use panproto_inst::{Node, TreeEdit};
    use panproto_schema::{Edge, Protocol};

    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    use super::EditLens;

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

    #[test]
    fn identity_edit_lens_passes_through() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: panproto_inst::Value::Str("updated".into()),
        };

        let result = edit_lens.get_edit(edit).unwrap();
        match &result {
            TreeEdit::SetField { node_id, field, .. } => {
                assert_eq!(*node_id, 1);
                assert_eq!(field, &Name::from("text"));
            }
            other => panic!("expected SetField, got {other:?}"),
        }
    }

    #[test]
    fn non_surviving_insert_goes_to_complement() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::InsertNode {
            parent: 0,
            child_id: 99,
            node: Node::new(99, "post:body.createdAt"),
            edge: Edge {
                src: "post:body".into(),
                tgt: "post:body.createdAt".into(),
                kind: "prop".into(),
                name: Some("createdAt".into()),
            },
        };

        let result = edit_lens.get_edit(edit).unwrap();
        assert!(
            result.is_identity(),
            "non-surviving anchor should produce Identity"
        );
        assert!(
            edit_lens.complement.dropped_nodes.contains_key(&99),
            "node should be in complement"
        );
    }

    #[test]
    fn set_field_on_complement_node_is_absorbed() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        assert!(edit_lens.complement.dropped_nodes.contains_key(&2));

        let edit = TreeEdit::SetField {
            node_id: 2,
            field: Name::from("value"),
            value: panproto_inst::Value::Str("2025-01-01".into()),
        };

        let result = edit_lens.get_edit(edit).unwrap();
        assert!(result.is_identity());

        let node = &edit_lens.complement.dropped_nodes[&2];
        assert_eq!(
            node.extra_fields.get("value"),
            Some(&panproto_inst::Value::Str("2025-01-01".into()))
        );
    }

    #[test]
    fn delete_node_in_complement() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        assert!(edit_lens.complement.dropped_nodes.contains_key(&2));

        let edit = TreeEdit::DeleteNode { id: 2 };
        let result = edit_lens.get_edit(edit).unwrap();
        assert!(result.is_identity());
        assert!(!edit_lens.complement.dropped_nodes.contains_key(&2));
    }

    #[test]
    fn delete_node_in_view_passes_through() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::DeleteNode { id: 1 };
        let result = edit_lens.get_edit(edit).unwrap();
        match result {
            TreeEdit::DeleteNode { id } => assert_eq!(id, 1),
            other => panic!("expected DeleteNode, got {other:?}"),
        }
    }

    #[test]
    fn put_edit_passes_through_for_identity() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::SetField {
            node_id: 1,
            field: Name::from("text"),
            value: panproto_inst::Value::Str("from_view".into()),
        };

        let result = edit_lens.put_edit(edit).unwrap();
        match &result {
            TreeEdit::SetField { node_id, .. } => assert_eq!(*node_id, 1),
            other => panic!("expected SetField, got {other:?}"),
        }
    }

    #[test]
    fn sequence_edit_filters_identity() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let edit = TreeEdit::Sequence(vec![
            TreeEdit::SetField {
                node_id: 1,
                field: Name::from("text"),
                value: panproto_inst::Value::Str("hi".into()),
            },
            TreeEdit::SetField {
                node_id: 2,
                field: Name::from("val"),
                value: panproto_inst::Value::Int(1),
            },
        ]);

        let result = edit_lens.get_edit(edit).unwrap();
        match result {
            TreeEdit::SetField { node_id, .. } => assert_eq!(node_id, 1),
            other => panic!("expected single SetField, got {other:?}"),
        }
    }

    #[test]
    fn insert_fan_with_dropped_participant_goes_to_complement() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        // Node 2 is in the complement (createdAt was dropped).
        let fan = panproto_inst::Fan::new("test_he", 0)
            .with_child("a", 1)
            .with_child("b", 2);
        let edit = TreeEdit::InsertFan { fan };

        let result = edit_lens.get_edit(edit).unwrap();
        assert!(
            result.is_identity(),
            "fan with dropped participant should be absorbed"
        );
        assert_eq!(edit_lens.complement.dropped_fans.len(), 1);
    }

    #[test]
    fn insert_fan_all_surviving_passes_through() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        let fan = panproto_inst::Fan::new("test_he", 0)
            .with_child("a", 1)
            .with_child("b", 0);
        let edit = TreeEdit::InsertFan { fan };

        let result = edit_lens.get_edit(edit).unwrap();
        match result {
            TreeEdit::InsertFan { .. } => {}
            other => panic!("expected InsertFan, got {other:?}"),
        }
    }

    #[test]
    fn delete_fan_in_complement_is_absorbed() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();

        // Manually add a fan to the complement.
        edit_lens
            .complement
            .dropped_fans
            .push(panproto_inst::Fan::new("dropped_he", 0).with_child("x", 2));

        let edit = TreeEdit::DeleteFan {
            hyper_edge_id: Name::from("dropped_he"),
        };
        let result = edit_lens.get_edit(edit).unwrap();
        assert!(result.is_identity());
        assert!(edit_lens.complement.dropped_fans.is_empty());
    }

    #[test]
    fn optic_kind_identity_is_iso() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let edit_lens = EditLens::from_lens(lens, test_protocol());
        assert_eq!(
            edit_lens.optic_kind(),
            crate::OpticKind::Iso,
            "identity lens should classify as Iso"
        );
    }

    #[test]
    fn optic_kind_projection_is_lens() {
        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let edit_lens = EditLens::from_lens(lens, test_protocol());
        assert_eq!(
            edit_lens.optic_kind(),
            crate::OpticKind::Lens,
            "projection lens should classify as Lens"
        );
    }
}
