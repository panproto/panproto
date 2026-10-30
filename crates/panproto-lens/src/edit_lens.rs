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

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use panproto_gat::{CoercionClass, DirectedEquation, Name, Theory};
use panproto_inst::graph_edit::{AttrKey, GraphDeltaParts};
use panproto_inst::identity::{ComplementShape, IdentityCarrier};
use panproto_inst::{
    CompiledMigration, EdgeId, GInstance, GraphDelta, RestrictPlan, TreeEdit, TreeEmbedding,
    WInstance, graph_restrict_with_plan,
};
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
    /// The current complement state, stamped with the identity of the
    /// lens this edit lens was built from.
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

/// Match the edge ids of a restricted source presentation to the edge ids
/// of the caller's current tree presentation.
fn edge_presentation_map(
    restricted: &TreeEmbedding,
    current: &TreeEmbedding,
) -> Result<HashMap<EdgeId, EdgeId>, EditLensError> {
    if restricted.arcs.len() != current.arcs.len() || restricted.fans.len() != current.fans.len() {
        return Err(EditLensError::TranslationFailed(
            "source restriction and current view have different tree embeddings".to_owned(),
        ));
    }

    let mut forward = HashMap::new();
    let mut reverse = HashMap::new();
    let mut record = |from: EdgeId, to: EdgeId| -> Result<(), EditLensError> {
        let inconsistent = forward.get(&from).is_some_and(|known| *known != to)
            || reverse.get(&to).is_some_and(|known| *known != from);
        if inconsistent {
            return Err(EditLensError::TranslationFailed(
                "source and view edge presentations are not bijective".to_owned(),
            ));
        }
        forward.insert(from, to);
        reverse.insert(to, from);
        Ok(())
    };

    for (&child, &from) in &restricted.arcs {
        let to = current.arcs.get(&child).copied().ok_or_else(|| {
            EditLensError::TranslationFailed(format!(
                "current view has no embedded arc into child {child}"
            ))
        })?;
        record(from, to)?;
    }
    for (restricted_fan, current_fan) in restricted.fans.iter().zip(&current.fans) {
        if restricted_fan.node != current_fan.node
            || restricted_fan.child_edges.len() != current_fan.child_edges.len()
        {
            return Err(EditLensError::TranslationFailed(
                "source restriction and current view use different fan presentations".to_owned(),
            ));
        }
        record(restricted_fan.parent_edge, current_fan.parent_edge)?;
        for (label, &from) in &restricted_fan.child_edges {
            let to = current_fan.child_edges.get(label).copied().ok_or_else(|| {
                EditLensError::TranslationFailed(format!(
                    "current view fan has no embedded edge for label {label}"
                ))
            })?;
            record(from, to)?;
        }
    }
    Ok(forward)
}

/// Re-key the part of a restricted graph delta that refers to elements of
/// its before-state into the caller's current view presentation.
fn remap_old_edge_ids(
    delta: GraphDelta,
    edge_ids: &HashMap<EdgeId, EdgeId>,
) -> Result<GraphDelta, EditLensError> {
    let GraphDeltaParts {
        del_nodes,
        del_edges,
        add_nodes,
        add_edges,
        attrs,
    } = delta.into_parts();
    let remap = |id: EdgeId| edge_ids.get(&id).copied().unwrap_or(id);
    let remap_edges = |edges: BTreeMap<EdgeId, panproto_inst::GEdge>| {
        let mut out = BTreeMap::new();
        for (id, edge) in edges {
            let id = remap(id);
            if out.insert(id, edge).is_some() {
                return Err(EditLensError::TranslationFailed(format!(
                    "edge presentation maps two delta elements to id {}",
                    id.0
                )));
            }
        }
        Ok(out)
    };
    let mut remapped_attrs = BTreeMap::new();
    for (key, change) in attrs {
        let key = match key {
            AttrKey::EdgeLabel { edge } => AttrKey::EdgeLabel { edge: remap(edge) },
            AttrKey::EdgePosition { edge } => AttrKey::EdgePosition { edge: remap(edge) },
            other => other,
        };
        if remapped_attrs.insert(key, change).is_some() {
            return Err(EditLensError::TranslationFailed(
                "edge presentation maps two attribution changes to one key".to_owned(),
            ));
        }
    }
    GraphDelta::normalize(GraphDeltaParts {
        del_nodes,
        del_edges: remap_edges(del_edges)?,
        add_nodes,
        add_edges: remap_edges(add_edges)?,
        attrs: remapped_attrs,
    })
    .map_err(|error| EditLensError::TranslationFailed(error.to_string()))
}

/// Keep replacement events carried by the input delta even when their old
/// and new payloads happen to be equal. A state diff cannot recover this
/// provenance, while canonical graph-delta normal form deliberately retains
/// it.
fn preserve_input_replacements(
    delta: GraphDelta,
    input: &GraphDelta,
    before: &GInstance,
    after: &GInstance,
) -> Result<GraphDelta, EditLensError> {
    let mut parts = delta.into_parts();
    for id in input
        .del_nodes()
        .keys()
        .filter(|id| input.add_nodes().contains_key(id))
    {
        if let (Some(old), Some(new)) = (before.node(*id), after.node(*id)) {
            parts.del_nodes.insert(*id, old.clone());
            parts.add_nodes.insert(*id, new.clone());
        }
    }
    for id in input
        .del_edges()
        .keys()
        .filter(|id| input.add_edges().contains_key(id))
    {
        if let (Some(old), Some(new)) = (before.edge(*id), after.edge(*id)) {
            parts.del_edges.insert(*id, old.clone());
            parts.add_edges.insert(*id, new.clone());
        }
    }
    GraphDelta::normalize(parts)
        .map_err(|error| EditLensError::TranslationFailed(error.to_string()))
}

impl EditLens {
    /// Clone the state needed to translate a sequence transactionally.
    fn staged(&self) -> Self {
        Self {
            compiled: self.compiled.clone(),
            src_schema: self.src_schema.clone(),
            tgt_schema: self.tgt_schema.clone(),
            complement: self.complement.clone(),
            protocol: self.protocol.clone(),
            reverse_vertex_remap: self.reverse_vertex_remap.clone(),
            reverse_edge_remap: self.reverse_edge_remap.clone(),
            pipeline: self.pipeline.clone(),
        }
    }

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
        // Stamped from the start, so a complement filled by edits alone,
        // without a whole-state `initialize`, is still bound to this lens.
        let mut complement = Complement::empty();
        complement.stamp(lens.identity(ComplementShape::WType), &lens.src_schema);
        Self {
            compiled: lens.compiled,
            src_schema: lens.src_schema,
            tgt_schema: lens.tgt_schema,
            complement,
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

    /// Translate a canonical graph delta on a tree-shaped source.
    ///
    /// This is the exact edit carrier for changes that [`TreeEdit`] cannot
    /// represent, notably reinserting an arc under its former edge id and
    /// sibling position. `source` is embedded under `source_embedding`,
    /// `edit` is applied to that graph, and both the before and after graphs
    /// are restricted through this lens. `view` and `view_embedding` name the
    /// current view presentation, so deleted and changed edges are returned
    /// in its id namespace rather than the source graph's namespace. The
    /// result is a normalized [`GraphDelta`], preserving edge identity and
    /// position.
    ///
    /// The complement and tracked source advance only after every apply,
    /// restriction, and tree conversion succeeds. A refusal therefore leaves
    /// this edit lens unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::TranslationFailed`] when either supplied tree
    /// embedding is incoherent, the current view is not the restriction of
    /// the source, the edited graph cannot be read as a source tree, or the
    /// whole-state tree translation fails,
    /// [`EditLensError::EditApply`] when `edit` does not apply to the source,
    /// and [`EditLensError::Restrict`] when either graph restriction is
    /// refused.
    pub fn get_graph_edit(
        &mut self,
        source: &WInstance,
        source_embedding: &TreeEmbedding,
        view: &WInstance,
        view_embedding: &TreeEmbedding,
        edit: &GraphDelta,
    ) -> Result<GraphDelta, EditLensError> {
        let source_graph =
            GInstance::from_wtype_under(source, source_embedding, Some(&self.src_schema))
                .map_err(|error| EditLensError::TranslationFailed(error.to_string()))?;
        let current_view =
            GInstance::from_wtype_under(view, view_embedding, Some(&self.tgt_schema))
                .map_err(|error| EditLensError::TranslationFailed(error.to_string()))?;
        let mut edited_graph = source_graph.clone();
        edit.apply(&mut edited_graph)?;

        let plan = RestrictPlan::compile(&self.src_schema, &self.tgt_schema, &self.compiled)?;
        let before_view = graph_restrict_with_plan(&source_graph, &plan)?.0;
        let after_view = graph_restrict_with_plan(&edited_graph, &plan)?.0;
        let (before_tree, before_embedding) = before_view
            .to_wtype_embedded(&self.tgt_schema)
            .map_err(|error| EditLensError::TranslationFailed(error.to_string()))?;
        if !crate::instances_equivalent(&before_tree, view) {
            return Err(EditLensError::TranslationFailed(
                "the supplied view is not the restriction of the supplied source".to_owned(),
            ));
        }
        let edge_ids = edge_presentation_map(&before_embedding, view_embedding)?;
        let translated = preserve_input_replacements(
            GraphDelta::diff(&before_view, &after_view),
            edit,
            &before_view,
            &after_view,
        )?;
        let translated = remap_old_edge_ids(translated, &edge_ids)?;
        let mut translated_view = current_view;
        translated.apply(&mut translated_view)?;
        let (edited_source, _) = edited_graph
            .to_wtype_embedded(&self.src_schema)
            .map_err(|error| EditLensError::TranslationFailed(error.to_string()))?;

        let lens = Lens {
            compiled: self.compiled.clone(),
            src_schema: self.src_schema.clone(),
            tgt_schema: self.tgt_schema.clone(),
        };
        let (expected_view, complement) = crate::get(&lens, &edited_source)
            .map_err(|error| EditLensError::TranslationFailed(error.to_string()))?;
        let (translated_tree, _) = translated_view
            .to_wtype_embedded(&self.tgt_schema)
            .map_err(|error| EditLensError::TranslationFailed(error.to_string()))?;
        if !crate::instances_equivalent(&translated_tree, &expected_view) {
            return Err(EditLensError::TranslationFailed(
                "translated graph delta does not produce the restricted edited source".to_owned(),
            ));
        }

        self.complement = complement;
        self.pipeline = Some(EditPipeline::from_lens_and_instance(self, &edited_source));
        Ok(translated)
    }

    /// Classify this edit lens's optic kind from the migration structure.
    ///
    /// The classification follows from the data-preservation properties
    /// of the compiled migration:
    ///
    /// - **Iso**: all source vertices and edges survive (bijection) *and*
    ///   every value transform is invertible.
    /// - **Lens**: some vertices or edges are dropped but none added
    ///   (projection), or the structure is a bijection but a value
    ///   transform loses information.
    /// - **Prism**: variant-related changes are present (injection).
    /// - **Affine**: everything else (lens composed with prism).
    ///
    /// The structural check alone is not sufficient for `Iso`. A migration
    /// can map every vertex and edge bijectively while carrying a value
    /// transform that computes a field, coerces a scalar, or otherwise
    /// discards information — the schema-level map is a bijection, the
    /// value-level action is not. `Iso` asserts that both round-trip laws
    /// hold and that the complement stores nothing, so it is only returned
    /// when [`panproto_inst::CompiledMigration::coercion_class`] reports
    /// the composite of every value transform as
    /// [`CoercionClass::Iso`]. A migration whose composite is
    /// `Retraction`, `Projection`, or `Opaque` is a bijection on structure
    /// carrying a lossy action on values, which is a lens.
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
        let values_are_invertible = self.compiled.coercion_class() == CoercionClass::Iso;

        if all_src_verts_survive && all_src_edges_survive {
            return if values_are_invertible {
                OpticKind::Iso
            } else {
                // Bijective on structure, lossy on values: a projection
                // that happens to keep every vertex and edge.
                OpticKind::Lens
            };
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
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::Restrict`] if a field transform carried by
    /// the migration fails to evaluate, or, as
    /// [`panproto_inst::RestrictError::ValueNotExpressible`] at the edited
    /// node, if a field edit sets a value with no expression literal that
    /// an `ApplyExpr` transform reads.
    ///
    /// That this is fallible at all is worth stating precisely, since an
    /// isomorphism is total by definition. The partiality is not a claim
    /// that the iso is not an iso: it is the evaluator's, not the
    /// mathematics'. Evaluation runs under an
    /// [`panproto_expr::EvalConfig`] step and depth budget, and a map that
    /// is total as a function is still partial as a computation, so
    /// exhausting the budget has to be reportable rather than silently
    /// yielding an untranslated edit. Skipping the transform instead would
    /// preserve this method's totality while making it return something
    /// that is not the isomorphism's action on the edit.
    ///
    /// The transforms reaching this path are invertible:
    /// [`Self::optic_kind`] only classifies a migration as `Iso` when the
    /// composite of its value transforms is
    /// [`CoercionClass::Iso`], so a lossy transform selects a
    /// different translation path rather than arriving here.
    pub fn translate_iso(&self, edit: TreeEdit) -> Result<TreeEdit, EditLensError> {
        let translated = match edit {
            TreeEdit::Identity => TreeEdit::Identity,
            TreeEdit::SetField {
                node_id,
                ref field,
                ref value,
            } => {
                let (name, val) = self.translate_field_edit(node_id, field.as_ref(), value)?;
                TreeEdit::SetField {
                    node_id,
                    field: Name::from(name.as_str()),
                    value: val,
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
                let remapped_node = self.remap_and_transform_node(node)?;
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
        };
        Ok(translated)
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
            TreeEdit::SetField { node_id, .. }
                if self.complement.dropped_nodes.contains_key(node_id) =>
            {
                return Ok(TreeEdit::Identity);
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
    /// Returns [`EditLensError`] if the edit cannot be translated, and
    /// [`EditLensError::EditApply`] with
    /// [`NodeNotFound`](panproto_inst::EditError::NodeNotFound) when, after
    /// [`initialize`](Self::initialize), a delete names an id the tracked
    /// source does not hold. A refused sequence leaves the lens unchanged.
    pub fn get_edit(&mut self, edit: TreeEdit) -> Result<TreeEdit, EditLensError> {
        // A delete names an existing source node. Once the lens tracks a
        // source, deleting an id that source does not hold is refused before
        // anything is tracked or translated. Inside a sequence this reads the
        // source as the earlier steps left it, and the sequence's staging
        // discards those steps with the refusal.
        if let TreeEdit::DeleteNode { id } = edit {
            if self
                .pipeline
                .as_ref()
                .is_some_and(|pipeline| !pipeline.source().nodes.contains_key(&id))
            {
                return Err(EditLensError::EditApply(
                    panproto_inst::EditError::NodeNotFound(id),
                ));
            }
        }
        // Keep the tracked source in step with the edits this lens has seen, so
        // an edit that takes a node out of the view can still read the payload
        // the complement must absorb. A sequence is tracked step by step
        // through the recursive calls below, so tracking it here as well would
        // apply every step twice.
        if !matches!(edit, TreeEdit::Sequence(_)) {
            if let Some(pipeline) = self.pipeline.as_mut() {
                pipeline.track_source_edit(&edit);
            }
        }
        match edit {
            TreeEdit::Identity => Ok(TreeEdit::Identity),
            TreeEdit::InsertNode {
                parent,
                child_id,
                node,
                edge,
            } => self.get_edit_insert(parent, child_id, node, edge),
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
            } => self.get_edit_move(node_id, new_parent, &edge),
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
                produce: self.remap_and_transform_node(&produce)?,
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
    ) -> Result<TreeEdit, EditLensError> {
        // The source appends the new arc, and the complement keeps the
        // source's arc order whichever ends the view keeps.
        self.complement.arc_order.push((parent, child_id));
        // The id floor is the tracked source's, as it is after a deletion.
        if let Some(pipeline) = &self.pipeline {
            self.complement.id_floor =
                panproto_inst::complement::tree_id_floor(pipeline.source()).unwrap_or(u32::MAX);
        }
        // Step 1: anchor survival.
        let target_anchor = self
            .compiled
            .vertex_remap
            .get(&node.anchor)
            .unwrap_or(&node.anchor);
        if !self.compiled.surviving_verts.contains(target_anchor) {
            self.complement.dropped_nodes.insert(child_id, node);
            self.complement.dropped_arcs.push((parent, child_id, edge));
            return Ok(TreeEdit::Identity);
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
                return Ok(TreeEdit::Identity);
            }
        }

        // Steps 3-4: structural remap and field transforms.
        let remapped_node = self.remap_and_transform_node(&node)?;

        // A surviving node inserted under a node the lens drops hangs, in
        // the view, from the nearest surviving ancestor, through the edge
        // the contracted pair resolves to.
        if self.complement.dropped_nodes.contains_key(&parent) {
            let ancestor = self.nearest_surviving_ancestor(parent)?;
            let label = self.contracted_label(ancestor, &node.anchor)?;
            self.complement
                .contraction_choices
                .insert((ancestor, child_id), label.clone());
            self.complement.original_parent.insert(child_id, parent);
            self.complement.dropped_arcs.push((parent, child_id, edge));
            return Ok(TreeEdit::InsertNode {
                parent: ancestor,
                child_id,
                node: remapped_node,
                edge: label,
            });
        }

        // Whole-state get records the source parent of every node the view
        // keeps, so a surviving insertion records its own.
        self.complement.original_parent.insert(child_id, parent);
        let remapped_edge = self.remap_edge_forward(&edge);
        Ok(TreeEdit::InsertNode {
            parent,
            child_id,
            node: remapped_node,
            edge: remapped_edge,
        })
    }

    /// The source parent of `id` in the source as the edits seen so far
    /// have left it: the tracked source when the lens was initialized, else
    /// the complement's record of the arc into `id`.
    fn source_parent(&self, id: u32) -> Option<u32> {
        if let Some(pipeline) = self.pipeline.as_ref() {
            return pipeline
                .node_snapshot(id)
                .and_then(|(_, arc)| arc.map(|(parent, _)| parent));
        }
        self.complement
            .dropped_arcs
            .iter()
            .find(|&&(_, child, _)| child == id)
            .map(|&(parent, _, _)| parent)
            .or_else(|| self.complement.original_parent.get(&id).copied())
    }

    /// The source anchor of `id` in the source as the edits seen so far
    /// have left it.
    fn source_anchor(&self, id: u32) -> Option<Name> {
        if let Some(node) = self.complement.dropped_nodes.get(&id) {
            return Some(node.anchor.clone());
        }
        self.pipeline
            .as_ref()
            .and_then(|pipeline| pipeline.node_snapshot(id))
            .map(|(node, _)| node.anchor)
    }

    /// The nearest ancestor of `id`, `id` itself included, that the lens
    /// keeps in the view: the node its surviving descendants hang from once
    /// the dropped nodes between are contracted.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::TranslationFailed`] when the walk leaves
    /// the source before reaching a surviving node (the root is dropped).
    fn nearest_surviving_ancestor(&self, id: u32) -> Result<u32, EditLensError> {
        let mut current = id;
        let mut steps = 0usize;
        while self.complement.dropped_nodes.contains_key(&current) {
            current = self.source_parent(current).ok_or_else(|| {
                EditLensError::TranslationFailed(format!(
                    "dropped node {current} has no parent, so the nodes below it have no \
                     surviving ancestor in the view"
                ))
            })?;
            steps += 1;
            if steps > self.complement.dropped_nodes.len() {
                return Err(EditLensError::TranslationFailed(format!(
                    "the ancestors of node {id} form a cycle"
                )));
            }
        }
        Ok(current)
    }

    /// The view edge a contracted pair resolves to: from the surviving
    /// ancestor `ancestor` to a surviving node anchored (in the source) at
    /// `child_anchor`, read off the resolver or the unique target edge
    /// between the images, as whole-state `get` resolves it.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::TranslationFailed`] when the ancestor's
    /// anchor is unknown, and [`EditLensError::Restrict`] when the pair has
    /// no edge or several without a resolver entry.
    fn contracted_label(&self, ancestor: u32, child_anchor: &Name) -> Result<Edge, EditLensError> {
        let ancestor_anchor = self.source_anchor(ancestor).ok_or_else(|| {
            EditLensError::TranslationFailed(format!(
                "the anchor of node {ancestor} is not known to this edit lens; initialize it \
                 from a source first"
            ))
        })?;
        let src = self.remap_anchor_forward(&ancestor_anchor);
        let tgt = self.remap_anchor_forward(child_anchor);
        Ok(panproto_inst::resolve_edge(
            &self.tgt_schema,
            &self.compiled.resolver,
            &src,
            &tgt,
        )?)
    }

    /// Translate a move of `node_id` under `new_parent` along the source
    /// edge `edge`, after the tracked source has taken it.
    ///
    /// A surviving node moved under a dropped node hangs from that node's
    /// nearest surviving ancestor through the contracted label. A dropped
    /// node carries the surviving nodes contracted through it: each of
    /// those is re-parented in the view to its new nearest surviving
    /// ancestor, and a move that leaves one where it was emits nothing for
    /// it.
    fn get_edit_move(
        &mut self,
        node_id: u32,
        new_parent: u32,
        edge: &Edge,
    ) -> Result<TreeEdit, EditLensError> {
        // The source removes the arc into `node_id` and appends the arc the
        // move creates, and the complement keeps the source's arc order.
        self.complement
            .arc_order
            .retain(|&(_, child)| child != node_id);
        self.complement.arc_order.push((new_parent, node_id));
        // The arc into `node_id` is set aside in the complement when either
        // end is dropped; it is replaced by the arc the move creates, which
        // is set aside again when either of its ends is dropped.
        self.complement
            .dropped_arcs
            .retain(|&(_, child, _)| child != node_id);
        let node_dropped = self.complement.dropped_nodes.contains_key(&node_id);
        let parent_dropped = self.complement.dropped_nodes.contains_key(&new_parent);
        if node_dropped || parent_dropped {
            self.complement
                .dropped_arcs
                .push((new_parent, node_id, edge.clone()));
        }
        if node_dropped {
            return self.recontract_below(node_id);
        }
        self.complement
            .contraction_choices
            .retain(|&(_, child), _| child != node_id);
        self.complement.original_parent.insert(node_id, new_parent);
        if parent_dropped {
            let ancestor = self.nearest_surviving_ancestor(new_parent)?;
            let anchor = self.source_anchor(node_id).ok_or_else(|| {
                EditLensError::TranslationFailed(format!(
                    "the anchor of node {node_id} is not known to this edit lens"
                ))
            })?;
            let label = self.contracted_label(ancestor, &anchor)?;
            self.complement
                .contraction_choices
                .insert((ancestor, node_id), label.clone());
            return Ok(TreeEdit::MoveSubtree {
                node_id,
                new_parent: ancestor,
                edge: label,
            });
        }
        Ok(TreeEdit::MoveSubtree {
            node_id,
            new_parent,
            edge: self.remap_edge_forward(edge),
        })
    }

    /// Re-parent, in the view, every surviving node contracted through the
    /// dropped node `dropped`, whose position in the source has changed.
    fn recontract_below(&mut self, dropped: u32) -> Result<TreeEdit, EditLensError> {
        let mut contracted: Vec<((u32, u32), Edge)> = self
            .complement
            .contraction_choices
            .iter()
            .map(|(&pair, edge)| (pair, edge.clone()))
            .collect();
        contracted.sort_by_key(|&((_, child), _)| child);
        let mut moves = Vec::new();
        for ((old_ancestor, child), old_label) in contracted {
            if !self.contracted_through(child, dropped) {
                continue;
            }
            let parent = self.source_parent(child).ok_or_else(|| {
                EditLensError::TranslationFailed(format!("node {child} has no parent"))
            })?;
            let ancestor = self.nearest_surviving_ancestor(parent)?;
            let anchor = self.source_anchor(child).ok_or_else(|| {
                EditLensError::TranslationFailed(format!(
                    "the anchor of node {child} is not known to this edit lens"
                ))
            })?;
            let label = self.contracted_label(ancestor, &anchor)?;
            if (ancestor, &label) == (old_ancestor, &old_label) {
                continue;
            }
            self.complement
                .contraction_choices
                .remove(&(old_ancestor, child));
            self.complement
                .contraction_choices
                .insert((ancestor, child), label.clone());
            moves.push(TreeEdit::MoveSubtree {
                node_id: child,
                new_parent: ancestor,
                edge: label,
            });
        }
        Ok(match moves.len() {
            0 => TreeEdit::Identity,
            1 => moves.remove(0),
            _ => TreeEdit::Sequence(moves),
        })
    }

    /// Whether the path from `child` up to its nearest surviving ancestor
    /// passes through `dropped`.
    fn contracted_through(&self, child: u32, dropped: u32) -> bool {
        let mut current = self.source_parent(child);
        let mut steps = 0usize;
        while let Some(node) = current {
            if node == dropped {
                return true;
            }
            if !self.complement.dropped_nodes.contains_key(&node)
                || steps > self.complement.dropped_nodes.len()
            {
                return false;
            }
            current = self.source_parent(node);
            steps += 1;
        }
        false
    }

    fn get_edit_delete(&mut self, id: u32) -> TreeEdit {
        let was_dropped = self.complement.dropped_nodes.contains_key(&id);
        self.forget_subtree(id);
        if let Some(pipeline) = &self.pipeline {
            self.complement.id_floor =
                panproto_inst::complement::tree_id_floor(pipeline.source()).unwrap_or(u32::MAX);
        }
        if was_dropped {
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
        let translated = self.translate_field_edit(node_id, &field_str, value)?;

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
                let mut remapped = self.remap_and_transform_node(&node)?;
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
                // Was in the view, now anchors at a vertex the migration drops.
                // The view sees a delete, so the complement has to take the
                // payload the view is giving up.
                let snapshot = self.pipeline.as_ref().and_then(|p| p.node_snapshot(id));
                if let Some(snapshot) = snapshot {
                    crate::edit_pipeline::absorb_dropped_node(&mut self.complement, id, snapshot);
                }
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
        let mut staged = self.staged();
        let mut translated = Vec::with_capacity(steps.len());
        for step in steps {
            let t = staged.get_edit(step)?;
            if !t.is_identity() {
                translated.push(t);
            }
        }
        let result = match translated.len() {
            0 => TreeEdit::Identity,
            1 => translated.into_iter().next().unwrap_or(TreeEdit::Identity),
            _ => TreeEdit::Sequence(translated),
        };
        *self = staged;
        Ok(result)
    }

    /// Translate a view edit back to a source edit, updating the complement.
    ///
    /// Uses the inverse vertex/edge remap. For value-level edits, applies
    /// the inverse directed equations (from `DirectedEquation.inverse`)
    /// when available.
    ///
    /// The complement is a state machine rather than a snapshot, so it
    /// moves in this direction too: a view edit changes what the source
    /// is, and any record the complement still holds about the parts the
    /// edit touched would otherwise describe a source that no longer
    /// exists. A later `put` would then reinstate a node under an id the
    /// view has reused, or reattach a child to a parent that is gone.
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
            TreeEdit::DeleteNode { id } => {
                self.forget_subtree(id);
                Ok(TreeEdit::DeleteNode { id })
            }
            TreeEdit::SetField {
                node_id,
                ref field,
                ref value,
            } => Ok(self.put_edit_set_field(node_id, field, value)),
            TreeEdit::RemoveField { node_id, field } => {
                let source_name = self.translate_field_name_backward(field.as_ref());
                if let Some(node) = self.complement.dropped_nodes.get_mut(&node_id) {
                    node.extra_fields.remove(source_name.as_str());
                }
                Ok(TreeEdit::RemoveField {
                    node_id,
                    field: Name::from(source_name.as_str()),
                })
            }
            TreeEdit::RelabelNode { id, new_anchor } => {
                let source_anchor = self.remap_anchor_backward(&new_anchor);
                if let Some(node) = self.complement.dropped_nodes.get_mut(&id) {
                    node.anchor = source_anchor.clone();
                }
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
            TreeEdit::DeleteFan { hyper_edge_id } => {
                self.complement
                    .dropped_fans
                    .retain(|fan| fan.hyper_edge_id.as_str() != hyper_edge_id.as_ref());
                Ok(TreeEdit::DeleteFan { hyper_edge_id })
            }
            TreeEdit::ContractNode { id } => {
                // The node is folded into its parent, so whatever the
                // complement recorded about it as a node of its own no
                // longer describes anything.
                self.complement.dropped_nodes.remove(&id);
                self.complement.original_parent.remove(&id);
                Ok(TreeEdit::ContractNode { id })
            }
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
                let mut staged = self.staged();
                let mut translated = Vec::with_capacity(steps.len());
                for step in steps {
                    let t = staged.put_edit(step)?;
                    if !t.is_identity() {
                        translated.push(t);
                    }
                }
                let result = match translated.len() {
                    0 => TreeEdit::Identity,
                    1 => translated.into_iter().next().unwrap_or(TreeEdit::Identity),
                    _ => TreeEdit::Sequence(translated),
                };
                *self = staged;
                Ok(result)
            }
        }
    }

    fn put_edit_insert(
        &mut self,
        parent: u32,
        child_id: u32,
        node: &panproto_inst::Node,
        edge: &Edge,
    ) -> TreeEdit {
        // The view has put a node at `child_id`. Anything the complement
        // still holds under that id describes a node this insert replaces,
        // and leaving it would have `put` restore the old content on top
        // of the new node.
        self.forget_subtree(child_id);

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
        &mut self,
        node_id: u32,
        field: &Name,
        value: &panproto_inst::Value,
    ) -> TreeEdit {
        let source_name = self.translate_field_name_backward(field.as_ref());
        // For value transforms, the complement stores the original if needed
        // for lossless round-tripping. Field transform expressions are
        // forward-only; inverse behavior requires protocol-level directed
        // equations with an `inverse` field.
        //
        // A node the complement holds keeps its copy in step, mirroring
        // what `get_edit_set_field` does in the forward direction.
        if let Some(node) = self.complement.dropped_nodes.get_mut(&node_id) {
            node.extra_fields.insert(source_name.clone(), value.clone());
        }
        TreeEdit::SetField {
            node_id,
            field: Name::from(source_name.as_str()),
            value: value.clone(),
        }
    }

    /// Drop every complement record naming `id`, and every record for a
    /// node that reached the source only through `id`.
    ///
    /// A node the complement holds is restored by reattaching it to its
    /// recorded parent. Once `id` is gone from the source, a child
    /// recorded under it has nowhere to go, so the record is dropped with
    /// it rather than left to fail or to reattach somewhere arbitrary.
    fn forget_subtree(&mut self, id: u32) {
        let mut doomed = vec![id];
        let mut seen = std::collections::HashSet::from([id]);
        while let Some(current) = doomed.pop() {
            self.complement.dropped_nodes.remove(&current);
            self.complement.original_parent.remove(&current);
            self.complement.original_extra_fields.remove(&current);
            self.complement.original_values.remove(&current);
            self.complement.synthesized_nodes.remove(&current);
            self.complement.synthesized_keys.remove(&current);
            self.complement.original_anchors.remove(&current);

            let mut children: Vec<u32> = self
                .complement
                .dropped_arcs
                .iter()
                .filter(|&&(parent, _, _)| parent == current)
                .map(|&(_, child, _)| child)
                .collect();
            children.extend(
                self.complement
                    .contracted_into
                    .iter()
                    .filter(|&(_, ancestor)| *ancestor == current)
                    .map(|(&child, _)| child),
            );
            self.complement
                .contracted_into
                .retain(|&child, ancestor| child != current && *ancestor != current);
            self.complement
                .dropped_arcs
                .retain(|&(parent, child, _)| parent != current && child != current);
            self.complement
                .arc_edges
                .retain(|&(parent, child), _| parent != current && child != current);
            self.complement
                .arc_order
                .retain(|&(parent, child)| parent != current && child != current);
            self.complement
                .contraction_choices
                .retain(|&(parent, child), _| parent != current && child != current);
            self.complement.dropped_fans.retain(|fan| {
                fan.parent != current && !fan.children.values().any(|&c| c == current)
            });

            for child in children {
                if seen.insert(child) {
                    doomed.push(child);
                }
            }
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
    ///
    /// Note: this operates on an individual node without instance context,
    /// so child scalar values are unavailable. `ComputeField` transforms that
    /// read child scalars will only see `extra_fields` here. For full fiber
    /// access, use the restrict/extend pipeline instead.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::Restrict`] if one of the node's field
    /// transforms fails to evaluate.
    fn remap_and_transform_node(
        &self,
        node: &panproto_inst::Node,
    ) -> Result<panproto_inst::Node, EditLensError> {
        let mut remapped = node.clone();
        // Apply field transforms if any exist for this source anchor.
        if let Some(transforms) = self.compiled.field_transforms.get(&node.anchor) {
            panproto_inst::wtype::apply_field_transforms(
                &mut remapped,
                transforms,
                &panproto_inst::TransformContext::detached(),
            )?;
        }
        remapped.anchor = self.remap_anchor_forward(&node.anchor);
        Ok(remapped)
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
        let shape = panproto_inst::canonical_label_shape(fan.children.keys());
        let resolver = panproto_inst::FanResolver::new(&self.compiled);
        let Some((new_id, label_map)) = resolver.resolve(&fan.hyper_edge_id, &shape, &shape) else {
            return fan.clone();
        };

        let new_children = fan
            .children
            .iter()
            .map(|(label, &node_id)| {
                let new_label = label_map
                    .get(label.as_str())
                    .map_or_else(|| label.clone(), std::string::ToString::to_string);
                (new_label, node_id)
            })
            .collect();

        panproto_inst::Fan {
            hyper_edge_id: new_id.to_string(),
            parent: fan.parent,
            children: new_children,
        }
    }

    fn remap_fan_backward(&self, fan: &panproto_inst::Fan) -> panproto_inst::Fan {
        let shape = panproto_inst::canonical_label_shape(fan.children.keys());
        let resolver = panproto_inst::FanResolver::new(&self.compiled);
        let Some((old_id, (_, label_map))) = resolver.resolve_backward(&fan.hyper_edge_id, &shape)
        else {
            return fan.clone();
        };

        let mut label_pairs: Vec<_> = label_map.iter().collect();
        label_pairs.sort_unstable_by(|(source_a, target_a), (source_b, target_b)| {
            target_a.cmp(target_b).then_with(|| source_a.cmp(source_b))
        });
        let mut reverse_labels = HashMap::new();
        for (source, target) in label_pairs {
            reverse_labels
                .entry(target.to_string())
                .or_insert_with(|| source.to_string());
        }
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
        panproto_inst::Fan {
            hyper_edge_id: old_id.to_string(),
            parent: fan.parent,
            children,
        }
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

    /// Translate a field edit (name + value) on the node `node_id` through
    /// field transforms. Returns the translated (name, value) pair.
    ///
    /// # Errors
    ///
    /// Returns [`EditLensError::Restrict`] if an `ApplyExpr` transform's
    /// expression fails to evaluate, rather than passing the untransformed
    /// value through as though the transform had been a no-op, and, with
    /// [`panproto_inst::RestrictError::ValueNotExpressible`] at `node_id`,
    /// if the value an `ApplyExpr` transform reads has no expression
    /// literal.
    fn translate_field_edit(
        &self,
        node_id: u32,
        field: &str,
        value: &panproto_inst::Value,
    ) -> Result<(String, panproto_inst::Value), EditLensError> {
        let mut name = field.to_owned();
        let mut val = value.clone();

        // Apply all relevant field transforms in order.
        for transforms in self.compiled.field_transforms.values() {
            for transform in transforms {
                match transform {
                    panproto_inst::FieldTransform::RenameField { old_key, new_key }
                        if old_key == field =>
                    {
                        name.clone_from(new_key);
                    }
                    panproto_inst::FieldTransform::ApplyExpr { key, expr, .. } if key == field => {
                        let input = panproto_inst::value_to_expr_literal(&val).map_err(|e| {
                            panproto_inst::RestrictError::ValueNotExpressible {
                                node: node_id,
                                field: key.clone(),
                                value_type: e.value_type.to_owned(),
                            }
                        })?;
                        let env = panproto_expr::Env::new()
                            .extend(std::sync::Arc::from(key.as_str()), input);
                        let config = panproto_expr::EvalConfig::default();
                        let result =
                            panproto_expr::eval(expr, &env, &config).map_err(|source| {
                                panproto_inst::RestrictError::FieldTransformFailed {
                                    key: key.clone(),
                                    source,
                                }
                            })?;
                        val = panproto_inst::expr_literal_to_value(&result);
                    }
                    _ => {}
                }
            }
        }

        Ok((name, val))
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
    use crate::edit_error::EditLensError;

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
            ..Default::default()
        }
    }

    /// An identity lens carrying one value transform of the given class.
    fn lens_with_coercion(class: panproto_gat::CoercionClass) -> EditLens {
        let schema = three_node_schema();
        let mut lens = identity_lens(&schema);
        lens.compiled.field_transforms.insert(
            Name::from("post:body"),
            vec![panproto_inst::FieldTransform::ComputeField {
                target_key: "derived".to_string(),
                expr: panproto_expr::Expr::Var(std::sync::Arc::from("text")),
                inverse: None,
                coercion_class: class,
            }],
        );
        EditLens::from_lens(lens, test_protocol())
    }

    #[test]
    fn structural_bijection_with_no_value_transforms_is_iso() {
        let schema = three_node_schema();
        let edit_lens = EditLens::from_lens(identity_lens(&schema), test_protocol());
        assert_eq!(
            edit_lens.optic_kind(),
            crate::optic::OpticKind::Iso,
            "an identity migration carrying no value transforms is an isomorphism"
        );
    }

    #[test]
    fn structural_bijection_with_invertible_values_stays_iso() {
        let edit_lens = lens_with_coercion(panproto_gat::CoercionClass::Iso);
        assert_eq!(
            edit_lens.optic_kind(),
            crate::optic::OpticKind::Iso,
            "an invertible value transform does not weaken the classification"
        );
    }

    #[test]
    fn structural_bijection_with_lossy_values_is_not_iso() {
        // Every vertex and edge survives, so the structural check alone
        // would say Iso. The value transform is not invertible, so the
        // round-trip laws Iso asserts do not hold.
        for class in [
            panproto_gat::CoercionClass::Retraction,
            panproto_gat::CoercionClass::Projection,
            panproto_gat::CoercionClass::Opaque,
        ] {
            let edit_lens = lens_with_coercion(class);
            assert_eq!(
                edit_lens.optic_kind(),
                crate::optic::OpticKind::Lens,
                "a bijection on structure carrying a {class:?} value transform is a lens, not an iso"
            );
        }
    }

    #[test]
    fn lossy_value_transform_carried_as_a_term_assignment_also_demotes() {
        // `panproto-mig` emits value transforms as term assignments rather
        // than as field transforms, so a composite folded from field
        // transforms alone would report Iso (the identity element of an
        // empty fold) for exactly those migrations.
        let schema = three_node_schema();
        let mut lens = identity_lens(&schema);
        let assignment = panproto_inst::TermAssignment::from_field_transform(
            &panproto_inst::FieldTransform::ComputeField {
                target_key: "derived".to_string(),
                expr: panproto_expr::Expr::Var(std::sync::Arc::from("text")),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
            },
        );
        lens.compiled
            .op_term_assignments
            .insert(Name::from("post:body"), vec![assignment]);
        let edit_lens = EditLens::from_lens(lens, test_protocol());

        assert_eq!(
            edit_lens.compiled.coercion_class(),
            panproto_gat::CoercionClass::Opaque,
            "the composite must fold term assignments, not only field transforms"
        );
        assert_eq!(edit_lens.optic_kind(), crate::optic::OpticKind::Lens);
    }

    #[test]
    fn an_unexpressible_field_edit_is_refused_as_not_expressible() {
        // Under an `ApplyExpr` transform on `text`, a field edit setting a
        // token, which has no expression literal, is refused as not
        // expressible, at the edited node, through both callers.
        let schema = three_node_schema();
        let mut lens = identity_lens(&schema);
        lens.compiled.field_transforms.insert(
            Name::from("post:body"),
            vec![panproto_inst::FieldTransform::ApplyExpr {
                key: "text".to_string(),
                expr: panproto_expr::Expr::Var(std::sync::Arc::from("text")),
                inverse: Some(panproto_expr::Expr::Var(std::sync::Arc::from("text"))),
                coercion_class: panproto_gat::CoercionClass::Iso,
            }],
        );
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&three_node_instance()).unwrap();
        let expected = |node: u32| {
            format!(
                "{:?}",
                crate::edit_error::EditLensError::Restrict(
                    panproto_inst::RestrictError::ValueNotExpressible {
                        node,
                        field: "text".to_owned(),
                        value_type: "token".to_owned(),
                    }
                )
            )
        };
        let edit = |node_id: u32| TreeEdit::SetField {
            node_id,
            field: Name::from("text"),
            value: panproto_inst::Value::Token("t".into()),
        };
        let refused = edit_lens.get_edit(edit(1)).unwrap_err();
        assert_eq!(format!("{refused:?}"), expected(1));
        let refused = edit_lens.translate_iso(edit(2)).unwrap_err();
        assert_eq!(format!("{refused:?}"), expected(2));
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
    fn put_edit_keeps_the_complement_in_step() {
        // `three_node_instance` has a `createdAt` child at node 2; the
        // projection lens drops it, so the complement holds it. A view
        // edit that removes node 2's parent, or reuses id 2, changes what
        // the source is, and the complement has to follow.
        let schema = three_node_schema();
        let instance = three_node_instance();

        // Deleting the node the complement holds must clear its record.
        let mut edit_lens = EditLens::from_lens(
            crate::tests::projection_lens(&schema, "createdAt"),
            test_protocol(),
        );
        edit_lens.initialize(&instance).unwrap();
        assert!(
            edit_lens.complement.dropped_nodes.contains_key(&2),
            "the dropped child must start out in the complement"
        );
        edit_lens.put_edit(TreeEdit::DeleteNode { id: 2 }).unwrap();
        assert!(
            !edit_lens.complement.dropped_nodes.contains_key(&2),
            "a deleted node must not stay in the complement"
        );
        assert!(
            edit_lens
                .complement
                .dropped_arcs
                .iter()
                .all(|&(parent, child, _)| parent != 2 && child != 2),
            "no arc may still name the deleted node: {:?}",
            edit_lens.complement.dropped_arcs,
        );

        // Deleting a parent must take the records for what hung off it.
        let mut edit_lens = EditLens::from_lens(
            crate::tests::projection_lens(&schema, "createdAt"),
            test_protocol(),
        );
        edit_lens.initialize(&instance).unwrap();
        let Some(parent) = edit_lens
            .complement
            .dropped_arcs
            .iter()
            .find(|&&(_, child, _)| child == 2)
            .map(|&(parent, _, _)| parent)
        else {
            panic!("the dropped child must have a recorded parent");
        };
        edit_lens
            .put_edit(TreeEdit::DeleteNode { id: parent })
            .unwrap();
        assert!(
            !edit_lens.complement.dropped_nodes.contains_key(&2),
            "a child recorded under a deleted parent has nowhere to be restored to"
        );

        // Reusing the id for a fresh node must not resurrect the old one.
        let mut edit_lens = EditLens::from_lens(
            crate::tests::projection_lens(&schema, "createdAt"),
            test_protocol(),
        );
        edit_lens.initialize(&instance).unwrap();
        edit_lens
            .put_edit(TreeEdit::InsertNode {
                parent: 0,
                child_id: 2,
                node: Node::new(2, "post:body.text"),
                edge: Edge {
                    src: "post:body".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            })
            .unwrap();
        assert!(
            !edit_lens.complement.dropped_nodes.contains_key(&2),
            "an insert at a reused id must not leave the old record behind"
        );
    }

    #[test]
    fn put_edit_set_field_reaches_a_complement_node() {
        let schema = three_node_schema();
        let instance = three_node_instance();
        let mut edit_lens = EditLens::from_lens(
            crate::tests::projection_lens(&schema, "createdAt"),
            test_protocol(),
        );
        edit_lens.initialize(&instance).unwrap();
        assert!(edit_lens.complement.dropped_nodes.contains_key(&2));

        edit_lens
            .put_edit(TreeEdit::SetField {
                node_id: 2,
                field: Name::from("value"),
                value: panproto_inst::Value::Str("2026-01-01".into()),
            })
            .unwrap();

        assert_eq!(
            edit_lens.complement.dropped_nodes[&2]
                .extra_fields
                .get("value"),
            Some(&panproto_inst::Value::Str("2026-01-01".into())),
            "the backward direction must update the complement's copy the way \
             the forward direction does",
        );
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

    /// An edit lens that was never initialized from a whole state still
    /// produces a complement bound to its lens, and `put` accepts it under
    /// the strict policy once edits have filled it.
    #[test]
    fn a_complement_filled_by_edits_alone_is_stamped() {
        use panproto_inst::identity::{ComplementShape, IdentityPolicy, IdentityStatus};

        use crate::identity::ComplementIdentity;

        let schema = three_node_schema();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let mut edit_lens = EditLens::from_lens(
            crate::tests::projection_lens(&schema, "createdAt"),
            test_protocol(),
        );
        assert_eq!(
            edit_lens.complement.identity,
            Some(lens.identity(ComplementShape::WType))
        );

        edit_lens
            .get_edit(TreeEdit::InsertNode {
                parent: 0,
                child_id: 99,
                node: Node::new(99, "post:body.createdAt"),
                edge: Edge {
                    src: "post:body".into(),
                    tgt: "post:body.createdAt".into(),
                    kind: "prop".into(),
                    name: Some("createdAt".into()),
                },
            })
            .unwrap();
        assert!(!edit_lens.complement.is_empty());
        assert_eq!(
            edit_lens
                .complement
                .verify(&lens, IdentityPolicy::Strict)
                .unwrap(),
            IdentityStatus::Verified
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
    fn deleting_an_absent_id_is_refused_and_leaves_the_lens_unchanged() {
        let schema = three_node_schema();
        let instance = three_node_instance();
        let lens = crate::tests::projection_lens(&schema, "createdAt");
        let mut edit_lens = EditLens::from_lens(lens, test_protocol());
        edit_lens.initialize(&instance).unwrap();
        let snapshot = |lens: &EditLens| serde_json::to_value(&lens.complement).unwrap();
        let before = snapshot(&edit_lens);
        let tracked = edit_lens.pipeline.as_ref().map(|p| p.source().clone());

        let alone = edit_lens.get_edit(TreeEdit::DeleteNode { id: 99 });
        assert!(
            matches!(
                alone,
                Err(EditLensError::EditApply(
                    panproto_inst::EditError::NodeNotFound(99)
                ))
            ),
            "{alone:?}"
        );

        let late = edit_lens.get_edit(TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 2 },
            TreeEdit::DeleteNode { id: 99 },
        ]));
        assert!(
            matches!(
                late,
                Err(EditLensError::EditApply(
                    panproto_inst::EditError::NodeNotFound(99)
                ))
            ),
            "{late:?}"
        );
        assert_eq!(snapshot(&edit_lens), before);
        let after = edit_lens.pipeline.as_ref().map(|p| p.source().clone());
        assert_eq!(
            after.map(|t| crate::instances_equivalent(&t, tracked.as_ref().unwrap())),
            Some(true),
            "a refused edit leaves the tracked source unchanged"
        );

        // A node the sequence itself deleted is absent for its later steps.
        let twice = edit_lens.get_edit(TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 2 },
            TreeEdit::DeleteNode { id: 2 },
        ]));
        assert!(
            matches!(
                twice,
                Err(EditLensError::EditApply(
                    panproto_inst::EditError::NodeNotFound(2)
                ))
            ),
            "{twice:?}"
        );
    }

    #[test]
    fn deleting_either_kind_of_leaf_preserves_exact_complement_coherence() {
        let schema = three_node_schema();
        let instance = three_node_instance();
        for id in [1, 2] {
            let lens = crate::tests::projection_lens(&schema, "createdAt");
            let mut edit_lens = EditLens::from_lens(lens, test_protocol());
            edit_lens.initialize(&instance).unwrap();

            let edit = TreeEdit::DeleteNode { id };
            let result =
                crate::edit_laws::check_complement_coherence(&mut edit_lens, &edit, &instance);
            assert!(
                result.is_ok(),
                "exact complement coherence after deleting node {id}: {result:?}"
            );
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
    fn backward_fan_label_ambiguity_uses_the_least_source_label() {
        for _ in 0..128 {
            let schema = three_node_schema();
            let mut lens = identity_lens(&schema);
            lens.compiled.hyper_resolver.insert(
                ("source".into(), vec!["a".into(), "b".into()]),
                (
                    "target".into(),
                    std::collections::HashMap::from([
                        ("b".into(), "z".into()),
                        ("a".into(), "z".into()),
                    ]),
                ),
            );
            let edit_lens = EditLens::from_lens(lens, test_protocol());
            let restored = edit_lens
                .remap_fan_backward(&panproto_inst::Fan::new("target", 0).with_child("z", 1));
            assert_eq!(
                restored,
                panproto_inst::Fan::new("source", 0).with_child("a", 1)
            );
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
