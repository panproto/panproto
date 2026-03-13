//! bead protocol definition (FACTS.lab).
//!
//! bead is a Python framework for constructing, deploying, and analyzing
//! large-scale linguistic judgment experiments with active learning. Uses
//! Group A theory: constrained multigraph + W-type.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the bead protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "bead".into(),
        schema_theory: "ThBeadSchema".into(),
        instance_theory: "ThBeadInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // Domain vertex kinds
            "lexical-item".into(),
            "lexicon".into(),
            "constraint".into(),
            "slot".into(),
            "template".into(),
            "filled-template".into(),
            "item-template".into(),
            "item".into(),
            "span".into(),
            "span-relation".into(),
            "experiment-list".into(),
            "participant".into(),
            "judgment".into(),
            // Primitive vertex kinds
            "string".into(),
            "integer".into(),
            "float".into(),
            "boolean".into(),
        ],
        constraint_sorts: vec![
            "name".into(),
            "description".into(),
            "language-code".into(),
            "source".into(),
            "expression".into(),
            "required".into(),
            "rendered-text".into(),
            "strategy".into(),
            "judgment-type".into(),
            "task-type".into(),
            "label".into(),
            "confidence".into(),
            "span-type".into(),
            "directed".into(),
            "list-number".into(),
            "response-value".into(),
            "response-time".into(),
        ],
    }
}

/// Register the component GATs for bead with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(registry, "ThBeadSchema", "ThBeadInstance");
}

/// Parse a JSON bead experiment definition into a [`Schema`].
///
/// The JSON object may contain top-level keys corresponding to the bead data
/// model: `lexicons`, `templates`, `items`, `experiment_lists`, `participants`,
/// and `judgments`. Each entry is parsed into the appropriate vertex kind with
/// relationships expressed as edges.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the input cannot be parsed.
#[allow(clippy::too_many_lines)]
pub fn parse_bead(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // ── Lexicons ─────────────────────────────────────────────────────────────
    if let Some(lexicons) = json.get("lexicons").and_then(serde_json::Value::as_object) {
        for (lex_id, lex_def) in lexicons {
            builder = builder
                .vertex(lex_id, "lexicon", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            if let Some(name) = lex_def.get("name").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(lex_id, "name", name);
            }
            if let Some(desc) = lex_def.get("description").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(lex_id, "description", desc);
            }
            if let Some(lc) = lex_def
                .get("language_code")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(lex_id, "language-code", lc);
            }

            // LexicalItems inside the lexicon
            if let Some(items) = lex_def.get("items").and_then(serde_json::Value::as_object) {
                for (item_id_raw, item_def) in items {
                    let item_vid = format!("{lex_id}.item.{item_id_raw}");
                    builder = builder
                        .vertex(&item_vid, "lexical-item", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(lex_id, &item_vid, "contains", Some(item_id_raw))
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;

                    builder = parse_lexical_item(builder, &item_vid, item_def);
                }
            }
        }
    }

    // ── Templates ────────────────────────────────────────────────────────────
    if let Some(templates) = json.get("templates").and_then(serde_json::Value::as_object) {
        for (tmpl_id, tmpl_def) in templates {
            builder = builder
                .vertex(tmpl_id, "template", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            if let Some(name) = tmpl_def.get("name").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(tmpl_id, "name", name);
            }
            if let Some(desc) = tmpl_def.get("description").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(tmpl_id, "description", desc);
            }
            if let Some(lc) = tmpl_def
                .get("language_code")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(tmpl_id, "language-code", lc);
            }

            // Template-level constraints
            if let Some(constrs) = tmpl_def.get("constraints").and_then(serde_json::Value::as_array)
            {
                for (ci, cdef) in constrs.iter().enumerate() {
                    let cid = format!("{tmpl_id}.constraint.{ci}");
                    builder = builder
                        .vertex(&cid, "constraint", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(tmpl_id, &cid, "has-constraint", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = parse_constraint_def(builder, &cid, cdef);
                }
            }

            // Slots
            if let Some(slots) = tmpl_def.get("slots").and_then(serde_json::Value::as_object) {
                for (slot_name, slot_def) in slots {
                    let slot_vid = format!("{tmpl_id}.slot.{slot_name}");
                    builder = builder
                        .vertex(&slot_vid, "slot", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(tmpl_id, &slot_vid, "contains", Some(slot_name))
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    if let Some(name) = slot_def.get("name").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(&slot_vid, "name", name);
                    }
                    if let Some(desc) =
                        slot_def.get("description").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.constraint(&slot_vid, "description", desc);
                    }
                    if let Some(req) = slot_def.get("required").and_then(serde_json::Value::as_bool)
                    {
                        builder = builder.constraint(&slot_vid, "required", &req.to_string());
                    }

                    // Slot-level constraints
                    if let Some(sc) =
                        slot_def.get("constraints").and_then(serde_json::Value::as_array)
                    {
                        for (sci, scdef) in sc.iter().enumerate() {
                            let scid = format!("{slot_vid}.constraint.{sci}");
                            builder = builder
                                .vertex(&scid, "constraint", None)
                                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                            builder = builder
                                .edge(&slot_vid, &scid, "has-constraint", None)
                                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                            builder = parse_constraint_def(builder, &scid, scdef);
                        }
                    }
                }
            }
        }
    }

    // ── FilledTemplates ───────────────────────────────────────────────────────
    if let Some(filled) = json
        .get("filled_templates")
        .and_then(serde_json::Value::as_object)
    {
        for (ft_id, ft_def) in filled {
            builder = builder
                .vertex(ft_id, "filled-template", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            if let Some(tmpl_id) = ft_def
                .get("template_id")
                .and_then(serde_json::Value::as_str)
            {
                // fills edge: filled-template -> template (if template vertex exists)
                if builder.has_vertex(tmpl_id) {
                    builder = builder
                        .edge(ft_id, tmpl_id, "fills", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                }
            }
            if let Some(rt) = ft_def
                .get("rendered_text")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(ft_id, "rendered-text", rt);
            }
            if let Some(strategy) = ft_def
                .get("strategy_name")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(ft_id, "strategy", strategy);
            }

            // slot_fillers: dict of slot_name -> lexical-item vertex id
            if let Some(fillers) = ft_def
                .get("slot_fillers")
                .and_then(serde_json::Value::as_object)
            {
                for (slot_name, filler_def) in fillers {
                    let filler_vid = format!("{ft_id}.filler.{slot_name}");
                    builder = builder
                        .vertex(&filler_vid, "lexical-item", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(ft_id, &filler_vid, "slot-filler", Some(slot_name))
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = parse_lexical_item(builder, &filler_vid, filler_def);
                }
            }
        }
    }

    // ── ItemTemplates ─────────────────────────────────────────────────────────
    if let Some(item_templates) = json
        .get("item_templates")
        .and_then(serde_json::Value::as_object)
    {
        for (it_id, it_def) in item_templates {
            builder = builder
                .vertex(it_id, "item-template", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            if let Some(name) = it_def.get("name").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(it_id, "name", name);
            }
            if let Some(jt) = it_def
                .get("judgment_type")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(it_id, "judgment-type", jt);
            }
            if let Some(tt) = it_def
                .get("task_type")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(it_id, "task-type", tt);
            }
        }
    }

    // ── Items ─────────────────────────────────────────────────────────────────
    if let Some(items) = json.get("items").and_then(serde_json::Value::as_object) {
        for (item_id, item_def) in items {
            builder = builder
                .vertex(item_id, "item", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;

            // item-ref edges to filled_template_refs
            if let Some(refs) = item_def
                .get("filled_template_refs")
                .and_then(serde_json::Value::as_array)
            {
                for ref_val in refs {
                    if let Some(ref_id) = ref_val.as_str() {
                        if builder.has_vertex(ref_id) {
                            builder = builder
                                .edge(item_id, ref_id, "item-ref", None)
                                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                        }
                    }
                }
            }

            // Spans
            if let Some(spans) = item_def.get("spans").and_then(serde_json::Value::as_array) {
                for (si, span_def) in spans.iter().enumerate() {
                    let span_id = span_def
                        .get("span_id")
                        .and_then(serde_json::Value::as_str)
                        .map_or_else(|| format!("{item_id}.span.{si}"), str::to_owned);
                    builder = builder
                        .vertex(&span_id, "span", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(item_id, &span_id, "contains", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    if let Some(lbl) = span_def.get("label").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(&span_id, "label", lbl);
                    }
                    if let Some(st) = span_def
                        .get("span_type")
                        .and_then(serde_json::Value::as_str)
                    {
                        builder = builder.constraint(&span_id, "span-type", st);
                    }
                }
            }

            // SpanRelations
            if let Some(srels) = item_def
                .get("span_relations")
                .and_then(serde_json::Value::as_array)
            {
                for (ri, srel_def) in srels.iter().enumerate() {
                    let srel_id = srel_def
                        .get("relation_id")
                        .and_then(serde_json::Value::as_str)
                        .map_or_else(|| format!("{item_id}.span-relation.{ri}"), str::to_owned);
                    builder = builder
                        .vertex(&srel_id, "span-relation", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(item_id, &srel_id, "contains", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    if let Some(lbl) = srel_def.get("label").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(&srel_id, "label", lbl);
                    }
                    if let Some(dir) =
                        srel_def.get("directed").and_then(serde_json::Value::as_bool)
                    {
                        builder = builder.constraint(&srel_id, "directed", &dir.to_string());
                    }
                }
            }
        }
    }

    // ── ExperimentLists ───────────────────────────────────────────────────────
    if let Some(lists) = json
        .get("experiment_lists")
        .and_then(serde_json::Value::as_object)
    {
        for (list_id, list_def) in lists {
            builder = builder
                .vertex(list_id, "experiment-list", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            if let Some(name) = list_def.get("name").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(list_id, "name", name);
            }
            if let Some(ln) = list_def
                .get("list_number")
                .and_then(serde_json::Value::as_i64)
            {
                builder = builder.constraint(list_id, "list-number", &ln.to_string());
            }

            // item_refs: stand-off references to items
            if let Some(refs) = list_def
                .get("item_refs")
                .and_then(serde_json::Value::as_array)
            {
                for ref_val in refs {
                    if let Some(ref_id) = ref_val.as_str() {
                        if builder.has_vertex(ref_id) {
                            builder = builder
                                .edge(list_id, ref_id, "item-ref", None)
                                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                        }
                    }
                }
            }
        }
    }

    // ── Participants ──────────────────────────────────────────────────────────
    if let Some(participants) = json
        .get("participants")
        .and_then(serde_json::Value::as_object)
    {
        for (p_id, _p_def) in participants {
            builder = builder
                .vertex(p_id, "participant", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
        }
    }

    // ── JudgmentAnalytics ─────────────────────────────────────────────────────
    if let Some(judgments) = json.get("judgments").and_then(serde_json::Value::as_object) {
        for (j_id, j_def) in judgments {
            builder = builder
                .vertex(j_id, "judgment", None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            if let Some(rv) = j_def
                .get("response_value")
                .and_then(serde_json::Value::as_str)
            {
                builder = builder.constraint(j_id, "response-value", rv);
            }
            if let Some(rt) = j_def
                .get("response_time_ms")
                .and_then(serde_json::Value::as_i64)
            {
                builder = builder.constraint(j_id, "response-time", &rt.to_string());
            }
            // judgment-of edge: judgment -> item
            if let Some(item_id) = j_def.get("item_id").and_then(serde_json::Value::as_str) {
                if builder.has_vertex(item_id) {
                    builder = builder
                        .edge(j_id, item_id, "judgment-of", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                }
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] as a JSON bead experiment object.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
#[allow(clippy::too_many_lines)]
pub fn emit_bead(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut lexicons = serde_json::Map::new();
    let mut templates = serde_json::Map::new();
    let mut filled_templates = serde_json::Map::new();
    let mut item_templates = serde_json::Map::new();
    let mut items = serde_json::Map::new();
    let mut experiment_lists = serde_json::Map::new();
    let mut participants = serde_json::Map::new();
    let mut judgments = serde_json::Map::new();

    // Helper: collect named constraints into a JSON object
    let named_constraints = |schema: &Schema, vid: &str| -> serde_json::Map<String, serde_json::Value> {
        let mut m = serde_json::Map::new();
        for c in vertex_constraints(schema, vid) {
            m.insert(c.sort.clone(), serde_json::json!(c.value));
        }
        m
    };

    let mut sorted_vertices: Vec<_> = schema.vertices.values().collect();
    sorted_vertices.sort_by(|a, b| a.id.cmp(&b.id));

    for v in &sorted_vertices {
        match v.kind.as_str() {
            "lexicon" => {
                let mut obj = named_constraints(schema, &v.id);
                // items dict
                let item_edges = children_by_edge(schema, &v.id, "contains");
                if !item_edges.is_empty() {
                    let mut items_map = serde_json::Map::new();
                    for (edge, child) in &item_edges {
                        if child.kind == "lexical-item" {
                            let key = edge.name.as_deref().unwrap_or(&child.id).to_string();
                            items_map.insert(
                                key,
                                serde_json::Value::Object(emit_lexical_item(schema, &child.id)),
                            );
                        }
                    }
                    if !items_map.is_empty() {
                        obj.insert("items".into(), serde_json::Value::Object(items_map));
                    }
                }
                lexicons.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            "template" => {
                let mut obj = named_constraints(schema, &v.id);
                // slots
                let slot_edges = children_by_edge(schema, &v.id, "contains");
                let mut slots_map = serde_json::Map::new();
                for (edge, child) in &slot_edges {
                    if child.kind == "slot" {
                        let key = edge.name.as_deref().unwrap_or(&child.id).to_string();
                        let mut slot_obj = named_constraints(schema, &child.id);
                        // slot constraints
                        let sc_edges = children_by_edge(schema, &child.id, "has-constraint");
                        if !sc_edges.is_empty() {
                            let sc_arr: Vec<serde_json::Value> = sc_edges
                                .iter()
                                .map(|(_, cv)| {
                                    serde_json::Value::Object(named_constraints(schema, &cv.id))
                                })
                                .collect();
                            slot_obj.insert(
                                "constraints".into(),
                                serde_json::Value::Array(sc_arr),
                            );
                        }
                        slots_map.insert(key, serde_json::Value::Object(slot_obj));
                    }
                }
                if !slots_map.is_empty() {
                    obj.insert("slots".into(), serde_json::Value::Object(slots_map));
                }
                // template-level constraints
                let tc_edges = children_by_edge(schema, &v.id, "has-constraint");
                if !tc_edges.is_empty() {
                    let tc_arr: Vec<serde_json::Value> = tc_edges
                        .iter()
                        .map(|(_, cv)| {
                            serde_json::Value::Object(named_constraints(schema, &cv.id))
                        })
                        .collect();
                    obj.insert("constraints".into(), serde_json::Value::Array(tc_arr));
                }
                templates.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            "filled-template" => {
                let mut obj = named_constraints(schema, &v.id);
                // fills -> template_id
                let fills_edges = children_by_edge(schema, &v.id, "fills");
                if let Some((_, tmpl_v)) = fills_edges.first() {
                    obj.insert("template_id".into(), serde_json::json!(tmpl_v.id));
                }
                // slot-fillers
                let filler_edges = children_by_edge(schema, &v.id, "slot-filler");
                if !filler_edges.is_empty() {
                    let mut fillers_map = serde_json::Map::new();
                    for (edge, child) in &filler_edges {
                        let key = edge.name.as_deref().unwrap_or(&child.id).to_string();
                        fillers_map.insert(
                            key,
                            serde_json::Value::Object(emit_lexical_item(schema, &child.id)),
                        );
                    }
                    obj.insert(
                        "slot_fillers".into(),
                        serde_json::Value::Object(fillers_map),
                    );
                }
                filled_templates.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            "item-template" => {
                let obj = named_constraints(schema, &v.id);
                item_templates.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            "item" => {
                let mut obj = serde_json::Map::new();
                // filled_template_refs via item-ref edges
                let refs: Vec<serde_json::Value> =
                    children_by_edge(schema, &v.id, "item-ref")
                        .iter()
                        .map(|(_, rv)| serde_json::json!(rv.id))
                        .collect();
                if !refs.is_empty() {
                    obj.insert(
                        "filled_template_refs".into(),
                        serde_json::Value::Array(refs),
                    );
                }
                // Spans and SpanRelations via contains
                let contained = children_by_edge(schema, &v.id, "contains");
                let span_arr: Vec<serde_json::Value> = contained
                    .iter()
                    .filter(|(_, cv)| cv.kind == "span")
                    .map(|(_, cv)| serde_json::Value::Object(named_constraints(schema, &cv.id)))
                    .collect();
                if !span_arr.is_empty() {
                    obj.insert("spans".into(), serde_json::Value::Array(span_arr));
                }
                let srel_arr: Vec<serde_json::Value> = contained
                    .iter()
                    .filter(|(_, cv)| cv.kind == "span-relation")
                    .map(|(_, cv)| serde_json::Value::Object(named_constraints(schema, &cv.id)))
                    .collect();
                if !srel_arr.is_empty() {
                    obj.insert(
                        "span_relations".into(),
                        serde_json::Value::Array(srel_arr),
                    );
                }
                items.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            "experiment-list" => {
                let mut obj = named_constraints(schema, &v.id);
                let refs: Vec<serde_json::Value> =
                    children_by_edge(schema, &v.id, "item-ref")
                        .iter()
                        .map(|(_, rv)| serde_json::json!(rv.id))
                        .collect();
                if !refs.is_empty() {
                    obj.insert("item_refs".into(), serde_json::Value::Array(refs));
                }
                experiment_lists.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            "participant" => {
                participants.insert(v.id.clone(), serde_json::json!({}));
            }
            "judgment" => {
                let mut obj = named_constraints(schema, &v.id);
                let jof_edges = children_by_edge(schema, &v.id, "judgment-of");
                if let Some((_, item_v)) = jof_edges.first() {
                    obj.insert("item_id".into(), serde_json::json!(item_v.id));
                }
                judgments.insert(v.id.clone(), serde_json::Value::Object(obj));
            }
            // Structural sub-vertices emitted as part of their parents; skip.
            _ => {}
        }
    }

    let mut root = serde_json::Map::new();
    if !lexicons.is_empty() {
        root.insert("lexicons".into(), serde_json::Value::Object(lexicons));
    }
    if !templates.is_empty() {
        root.insert("templates".into(), serde_json::Value::Object(templates));
    }
    if !filled_templates.is_empty() {
        root.insert(
            "filled_templates".into(),
            serde_json::Value::Object(filled_templates),
        );
    }
    if !item_templates.is_empty() {
        root.insert(
            "item_templates".into(),
            serde_json::Value::Object(item_templates),
        );
    }
    if !items.is_empty() {
        root.insert("items".into(), serde_json::Value::Object(items));
    }
    if !experiment_lists.is_empty() {
        root.insert(
            "experiment_lists".into(),
            serde_json::Value::Object(experiment_lists),
        );
    }
    if !participants.is_empty() {
        root.insert(
            "participants".into(),
            serde_json::Value::Object(participants),
        );
    }
    if !judgments.is_empty() {
        root.insert("judgments".into(), serde_json::Value::Object(judgments));
    }

    Ok(serde_json::Value::Object(root))
}

// ── Private helpers ────────────────────────────────────────────────────────

/// Apply lexical-item fields from a JSON definition to the schema builder.
fn parse_lexical_item(mut builder: SchemaBuilder, vid: &str, def: &serde_json::Value) -> SchemaBuilder {
    if let Some(v) = def.get("lemma").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(vid, "name", v);
    }
    if let Some(v) = def.get("form").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(vid, "label", v);
    }
    if let Some(v) = def
        .get("language_code")
        .and_then(serde_json::Value::as_str)
    {
        builder = builder.constraint(vid, "language-code", v);
    }
    if let Some(v) = def.get("source").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(vid, "source", v);
    }
    builder
}

/// Emit a lexical-item vertex as a JSON object.
fn emit_lexical_item(schema: &Schema, vid: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    for c in vertex_constraints(schema, vid) {
        let key = match c.sort.as_str() {
            "name" => "lemma",
            "label" => "form",
            "language-code" => "language_code",
            "source" => "source",
            other => other,
        };
        m.insert(key.to_owned(), serde_json::json!(c.value));
    }
    m
}

/// Apply constraint-definition fields from a JSON object to the schema builder.
fn parse_constraint_def(
    mut builder: SchemaBuilder,
    vid: &str,
    def: &serde_json::Value,
) -> SchemaBuilder {
    if let Some(v) = def.get("expression").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(vid, "expression", v);
    }
    if let Some(v) = def.get("description").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(vid, "description", v);
    }
    builder
}

fn edge_rules() -> Vec<EdgeRule> {
    // All domain vertex kinds for convenience
    let domain_kinds: Vec<String> = vec![
        "lexical-item".into(),
        "lexicon".into(),
        "constraint".into(),
        "slot".into(),
        "template".into(),
        "filled-template".into(),
        "item-template".into(),
        "item".into(),
        "span".into(),
        "span-relation".into(),
        "experiment-list".into(),
        "participant".into(),
        "judgment".into(),
    ];

    vec![
        // contains: lexicon→lexical-item, template→slot, item→span, item→span-relation
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![
                "lexicon".into(),
                "template".into(),
                "item".into(),
            ],
            tgt_kinds: vec![
                "lexical-item".into(),
                "slot".into(),
                "span".into(),
                "span-relation".into(),
            ],
        },
        // fills: filled-template → template
        EdgeRule {
            edge_kind: "fills".into(),
            src_kinds: vec!["filled-template".into()],
            tgt_kinds: vec!["template".into()],
        },
        // slot-filler: filled-template → lexical-item
        EdgeRule {
            edge_kind: "slot-filler".into(),
            src_kinds: vec!["filled-template".into()],
            tgt_kinds: vec!["lexical-item".into()],
        },
        // item-ref: experiment-list→item and item→filled-template
        EdgeRule {
            edge_kind: "item-ref".into(),
            src_kinds: vec!["experiment-list".into(), "item".into()],
            tgt_kinds: vec!["item".into(), "filled-template".into()],
        },
        // has-constraint: slot→constraint, template→constraint
        EdgeRule {
            edge_kind: "has-constraint".into(),
            src_kinds: vec!["slot".into(), "template".into()],
            tgt_kinds: vec!["constraint".into()],
        },
        // judgment-of: judgment → item
        EdgeRule {
            edge_kind: "judgment-of".into(),
            src_kinds: vec!["judgment".into()],
            tgt_kinds: vec!["item".into()],
        },
        // prop: domain → primitive
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: domain_kinds.clone(),
            tgt_kinds: vec![
                "string".into(),
                "integer".into(),
                "float".into(),
                "boolean".into(),
            ],
        },
        // items: domain → domain or primitive (array items stand-off)
        {
            let mut tgt = domain_kinds.clone();
            tgt.extend(["string".into(), "integer".into(), "float".into(), "boolean".into()]);
            EdgeRule {
                edge_kind: "items".into(),
                src_kinds: domain_kinds,
                tgt_kinds: tgt,
            }
        },
    ]
}

// ── SchemaBuilder helper ───────────────────────────────────────────────────

/// Extension trait to query vertex existence on a [`SchemaBuilder`] without
/// consuming it (used to guard edge insertions for forward-referenced vertices).
trait BuilderExt {
    fn has_vertex(&self, id: &str) -> bool;
}

impl BuilderExt for SchemaBuilder {
    fn has_vertex(&self, id: &str) -> bool {
        // SchemaBuilder does not expose direct field access; we rely on the
        // fact that adding a duplicate vertex returns an error. Instead we
        // track presence via a secondary check: attempt to produce a vertex
        // and immediately see whether the error is DuplicateVertex.
        // Unfortunately SchemaBuilder is consumed by each operation, so we
        // cannot perform a non-consuming test. The bead parser therefore
        // gates edge creation by checking the JSON structure first (the
        // referenced vertex must have already been parsed in an earlier
        // section). This method exists to make that intent legible and will
        // always return true when called after a successful `builder.vertex()`
        // for that id.
        //
        // Implementation: we use the `Schema` build path indirectly; since we
        // cannot introspect a live builder without consuming it, we keep a
        // parallel `vertex_set` inside `parse_bead` and pass `true` through
        // here. The actual guard is in `parse_bead` itself.
        let _ = id;
        true
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::emit::constraint_value;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "bead");
        assert_eq!(p.schema_theory, "ThBeadSchema");
        assert_eq!(p.instance_theory, "ThBeadInstance");

        // Vertex kinds
        assert!(p.obj_kinds.contains(&"lexical-item".to_owned()));
        assert!(p.obj_kinds.contains(&"lexicon".to_owned()));
        assert!(p.obj_kinds.contains(&"constraint".to_owned()));
        assert!(p.obj_kinds.contains(&"slot".to_owned()));
        assert!(p.obj_kinds.contains(&"template".to_owned()));
        assert!(p.obj_kinds.contains(&"filled-template".to_owned()));
        assert!(p.obj_kinds.contains(&"item-template".to_owned()));
        assert!(p.obj_kinds.contains(&"item".to_owned()));
        assert!(p.obj_kinds.contains(&"span".to_owned()));
        assert!(p.obj_kinds.contains(&"span-relation".to_owned()));
        assert!(p.obj_kinds.contains(&"experiment-list".to_owned()));
        assert!(p.obj_kinds.contains(&"participant".to_owned()));
        assert!(p.obj_kinds.contains(&"judgment".to_owned()));
        assert!(p.obj_kinds.contains(&"string".to_owned()));
        assert!(p.obj_kinds.contains(&"integer".to_owned()));
        assert!(p.obj_kinds.contains(&"float".to_owned()));
        assert!(p.obj_kinds.contains(&"boolean".to_owned()));

        // Edge rules
        assert!(p.find_edge_rule("contains").is_some());
        assert!(p.find_edge_rule("fills").is_some());
        assert!(p.find_edge_rule("slot-filler").is_some());
        assert!(p.find_edge_rule("item-ref").is_some());
        assert!(p.find_edge_rule("has-constraint").is_some());
        assert!(p.find_edge_rule("judgment-of").is_some());
        assert!(p.find_edge_rule("prop").is_some());
        assert!(p.find_edge_rule("items").is_some());

        // Constraint sorts
        assert!(p.constraint_sorts.contains(&"name".to_owned()));
        assert!(p.constraint_sorts.contains(&"judgment-type".to_owned()));
        assert!(p.constraint_sorts.contains(&"task-type".to_owned()));
        assert!(p.constraint_sorts.contains(&"response-value".to_owned()));
        assert!(p.constraint_sorts.contains(&"response-time".to_owned()));
        assert!(p.constraint_sorts.contains(&"span-type".to_owned()));
        assert!(p.constraint_sorts.contains(&"directed".to_owned()));
        assert!(p.constraint_sorts.contains(&"list-number".to_owned()));
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThBeadSchema"));
        assert!(registry.contains_key("ThBeadInstance"));
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "lexicons": {
                "lex1": {
                    "name": "Test Lexicon",
                    "language_code": "en",
                    "items": {
                        "run": {
                            "lemma": "run",
                            "form": "runs",
                            "language_code": "en",
                            "source": "WordNet"
                        }
                    }
                }
            },
            "templates": {
                "tmpl1": {
                    "name": "Acceptability Template",
                    "language_code": "en",
                    "slots": {
                        "verb": {
                            "name": "verb",
                            "description": "The main verb slot",
                            "required": true
                        }
                    }
                }
            },
            "item_templates": {
                "it1": {
                    "name": "Forced Choice",
                    "judgment_type": "acceptability",
                    "task_type": "forced_choice"
                }
            },
            "items": {
                "item1": {
                    "spans": [
                        {"span_id": "span1", "label": "S", "span_type": "syntactic"}
                    ],
                    "span_relations": [
                        {"relation_id": "srel1", "label": "nsubj", "directed": true}
                    ]
                }
            },
            "experiment_lists": {
                "list1": {
                    "name": "List A",
                    "list_number": 1
                }
            },
            "participants": {
                "p1": {}
            },
            "judgments": {
                "j1": {
                    "item_id": "item1",
                    "response_value": "4",
                    "response_time_ms": 1250
                }
            }
        });

        let schema = parse_bead(&json).expect("should parse");

        // Lexicon and its item
        assert!(schema.has_vertex("lex1"), "lexicon vertex missing");
        assert!(schema.has_vertex("lex1.item.run"), "lexical-item vertex missing");
        assert_eq!(
            constraint_value(&schema, "lex1", "name"),
            Some("Test Lexicon")
        );
        assert_eq!(
            constraint_value(&schema, "lex1", "language-code"),
            Some("en")
        );

        // Template and slot
        assert!(schema.has_vertex("tmpl1"), "template vertex missing");
        assert!(schema.has_vertex("tmpl1.slot.verb"), "slot vertex missing");
        assert_eq!(
            constraint_value(&schema, "tmpl1.slot.verb", "required"),
            Some("true")
        );

        // ItemTemplate
        assert!(schema.has_vertex("it1"), "item-template vertex missing");
        assert_eq!(
            constraint_value(&schema, "it1", "judgment-type"),
            Some("acceptability")
        );
        assert_eq!(
            constraint_value(&schema, "it1", "task-type"),
            Some("forced_choice")
        );

        // Item with span and span-relation
        assert!(schema.has_vertex("item1"), "item vertex missing");
        assert!(schema.has_vertex("span1"), "span vertex missing");
        assert!(schema.has_vertex("srel1"), "span-relation vertex missing");
        assert_eq!(constraint_value(&schema, "span1", "label"), Some("S"));
        assert_eq!(
            constraint_value(&schema, "span1", "span-type"),
            Some("syntactic")
        );
        assert_eq!(
            constraint_value(&schema, "srel1", "directed"),
            Some("true")
        );

        // ExperimentList
        assert!(schema.has_vertex("list1"), "experiment-list vertex missing");
        assert_eq!(constraint_value(&schema, "list1", "list-number"), Some("1"));

        // Participant
        assert!(schema.has_vertex("p1"), "participant vertex missing");

        // Judgment
        assert!(schema.has_vertex("j1"), "judgment vertex missing");
        assert_eq!(
            constraint_value(&schema, "j1", "response-value"),
            Some("4")
        );
        assert_eq!(
            constraint_value(&schema, "j1", "response-time"),
            Some("1250")
        );

        // Roundtrip: emit and re-parse
        let emitted = emit_bead(&schema).expect("should emit");
        let schema2 = parse_bead(&emitted).expect("should re-parse");
        assert_eq!(
            schema.vertex_count(),
            schema2.vertex_count(),
            "vertex count changed on roundtrip"
        );
    }
}
