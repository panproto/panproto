//! FOVEA (Flexible Ontology Visual Event Analyzer) protocol definition.
//!
//! FOVEA is a web-based video annotation platform with persona-based ontologies.
//! It uses a Group D theory: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`
//! for the schema and `ThWType` for instances.
//!
//! The data model has five layers:
//!
//! 1. **Ontology layer** (per-persona type definitions): entity-type, event-type,
//!    role-type, relation-type, ontology-relation.
//! 2. **World-state layer** (shared instances): entity, event, location, time,
//!    entity-collection, event-collection.
//! 3. **Annotation layer**: annotation, bounding-box.
//! 4. **Claims layer**: claim, claim-relation.
//! 5. **Supporting kinds**: persona, video, video-summary, string, boolean, float.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the FOVEA protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "fovea".into(),
        schema_theory: "ThFoveaSchema".into(),
        instance_theory: "ThFoveaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // Ontology layer
            "persona".into(),
            "ontology".into(),
            "entity-type".into(),
            "event-type".into(),
            "role-type".into(),
            "relation-type".into(),
            "ontology-relation".into(),
            // World-state layer
            "entity".into(),
            "event".into(),
            "location".into(),
            "time".into(),
            "entity-collection".into(),
            "event-collection".into(),
            // Annotation layer
            "annotation".into(),
            "bounding-box".into(),
            // Claims layer
            "claim".into(),
            "claim-relation".into(),
            // Supporting
            "video".into(),
            "video-summary".into(),
            "string".into(),
            "boolean".into(),
            "float".into(),
        ],
        constraint_sorts: vec![
            "name".into(),
            "role".into(),
            "information-need".into(),
            "confidence".into(),
            "justification".into(),
            "symmetric".into(),
            "transitive".into(),
            "collection-type".into(),
            "location-type".into(),
            "time-type".into(),
            "frame-number".into(),
            "x".into(),
            "y".into(),
            "width".into(),
            "height".into(),
            "label".into(),
            "source".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for FOVEA with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_typed_graph_wtype(registry, "ThFoveaSchema", "ThFoveaInstance");
}

/// Parse a JSON FOVEA document into a [`Schema`].
///
/// The expected JSON structure mirrors the FOVEA export format:
///
/// ```json
/// {
///   "personas": [...],
///   "personaOntologies": [...],
///   "world": { "entities": [...], "events": [...], ... },
///   "annotations": [...],
///   "videos": [...],
///   "claims": [...],
///   "claimRelations": [...]
/// }
/// ```
///
/// All IDs in the document are used as vertex IDs in the schema. Edges are
/// created in two passes so that forward references do not cause errors.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the input cannot be parsed.
#[allow(clippy::too_many_lines)]
pub fn parse_fovea(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    // Track registered vertex IDs so pass-2 edge code can guard forward refs.
    let mut known: HashSet<String> = HashSet::new();

    /// Register a vertex and record its ID in `known`.
    macro_rules! add_vertex {
        ($id:expr, $kind:expr) => {{
            let id_str: &str = $id;
            builder = builder
                .vertex(id_str, $kind, None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            known.insert(id_str.to_owned());
        }};
    }

    /// Add an edge only when both endpoints are known.
    macro_rules! add_edge {
        ($src:expr, $tgt:expr, $kind:expr, $name:expr) => {{
            let src_str: &str = $src;
            let tgt_str: &str = $tgt;
            if known.contains(src_str) && known.contains(tgt_str) {
                builder = builder
                    .edge(src_str, tgt_str, $kind, $name)
                    .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            }
        }};
    }

    // ── Pass 1: register all vertices ────────────────────────────────────────

    // Personas
    if let Some(personas) = json.get("personas").and_then(serde_json::Value::as_array) {
        for p in personas {
            let id = required_str(p, "id")?;
            add_vertex!(id, "persona");
            if let Some(name) = p.get("name").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "name", name);
            }
            if let Some(role) = p.get("role").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "role", role);
            }
            if let Some(need) = p.get("informationNeed").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(id, "information-need", need);
            }
        }
    }

    // Persona ontologies: one ontology vertex per persona ontology; nested types are separate vertices.
    if let Some(ontologies) = json
        .get("personaOntologies")
        .and_then(serde_json::Value::as_array)
    {
        for ont in ontologies {
            let ont_id = required_str(ont, "id")?;
            add_vertex!(ont_id, "ontology");

            // Entity types
            if let Some(etypes) = ont.get("entities").and_then(serde_json::Value::as_array) {
                for et in etypes {
                    let et_id = required_str(et, "id")?;
                    add_vertex!(et_id, "entity-type");
                    if let Some(name) = et.get("name").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(et_id, "name", name);
                    }
                }
            }

            // Role types
            if let Some(rtypes) = ont.get("roles").and_then(serde_json::Value::as_array) {
                for rt in rtypes {
                    let rt_id = required_str(rt, "id")?;
                    add_vertex!(rt_id, "role-type");
                    if let Some(name) = rt.get("name").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(rt_id, "name", name);
                    }
                }
            }

            // Event types
            if let Some(event_types) = ont.get("events").and_then(serde_json::Value::as_array) {
                for evt_t in event_types {
                    let evt_t_id = required_str(evt_t, "id")?;
                    add_vertex!(evt_t_id, "event-type");
                    if let Some(name) = evt_t.get("name").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(evt_t_id, "name", name);
                    }
                }
            }

            // Relation types
            if let Some(rel_types) = ont
                .get("relationTypes")
                .and_then(serde_json::Value::as_array)
            {
                for relt in rel_types {
                    let relt_id = required_str(relt, "id")?;
                    add_vertex!(relt_id, "relation-type");
                    if let Some(name) = relt.get("name").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(relt_id, "name", name);
                    }
                    if let Some(sym) = relt.get("symmetric").and_then(serde_json::Value::as_bool) {
                        builder = builder.constraint(
                            relt_id,
                            "symmetric",
                            if sym { "true" } else { "false" },
                        );
                    }
                    if let Some(trans) = relt.get("transitive").and_then(serde_json::Value::as_bool)
                    {
                        builder = builder.constraint(
                            relt_id,
                            "transitive",
                            if trans { "true" } else { "false" },
                        );
                    }
                }
            }

            // Ontology relation instances
            if let Some(relations) = ont.get("relations").and_then(serde_json::Value::as_array) {
                for rel in relations {
                    let rel_id = required_str(rel, "id")?;
                    add_vertex!(rel_id, "ontology-relation");
                    if let Some(src) = rel.get("source").and_then(serde_json::Value::as_str) {
                        builder = builder.constraint(rel_id, "source", src);
                    }
                }
            }
        }
    }

    // World state: entities, events, locations, times, collections
    if let Some(world) = json.get("world") {
        // Entities
        if let Some(entities) = world.get("entities").and_then(serde_json::Value::as_array) {
            for ent in entities {
                let ent_id = required_str(ent, "id")?;
                add_vertex!(ent_id, "entity");
                if let Some(name) = ent.get("name").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(ent_id, "name", name);
                }
            }
        }

        // Locations (specialised entity kind)
        if let Some(locs) = world.get("locations").and_then(serde_json::Value::as_array) {
            for loc in locs {
                let loc_id = required_str(loc, "id")?;
                add_vertex!(loc_id, "location");
                if let Some(name) = loc.get("name").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(loc_id, "name", name);
                }
                if let Some(lt) = loc.get("locationType").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(loc_id, "location-type", lt);
                }
            }
        }

        // Times
        if let Some(times) = world.get("times").and_then(serde_json::Value::as_array) {
            for t in times {
                let t_id = required_str(t, "id")?;
                add_vertex!(t_id, "time");
                if let Some(label) = t.get("label").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(t_id, "label", label);
                }
                if let Some(tt) = t.get("type").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(t_id, "time-type", tt);
                }
            }
        }

        // Events
        if let Some(events) = world.get("events").and_then(serde_json::Value::as_array) {
            for ev in events {
                let ev_id = required_str(ev, "id")?;
                add_vertex!(ev_id, "event");
                if let Some(name) = ev.get("name").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(ev_id, "name", name);
                }
            }
        }

        // Entity collections
        if let Some(ecols) = world
            .get("entityCollections")
            .and_then(serde_json::Value::as_array)
        {
            for ec in ecols {
                let ec_id = required_str(ec, "id")?;
                add_vertex!(ec_id, "entity-collection");
                if let Some(name) = ec.get("name").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(ec_id, "name", name);
                }
                if let Some(ct) = ec.get("collectionType").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(ec_id, "collection-type", ct);
                }
            }
        }

        // Event collections
        if let Some(evcols) = world
            .get("eventCollections")
            .and_then(serde_json::Value::as_array)
        {
            for evc in evcols {
                let evc_id = required_str(evc, "id")?;
                add_vertex!(evc_id, "event-collection");
                if let Some(name) = evc.get("name").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(evc_id, "name", name);
                }
                if let Some(ct) = evc
                    .get("collectionType")
                    .and_then(serde_json::Value::as_str)
                {
                    builder = builder.constraint(evc_id, "collection-type", ct);
                }
            }
        }
    }

    // Annotations and their bounding boxes
    if let Some(annotations) = json
        .get("annotations")
        .and_then(serde_json::Value::as_array)
    {
        for ann in annotations {
            let ann_id = required_str(ann, "id")?;
            add_vertex!(ann_id, "annotation");
            if let Some(conf) = ann.get("confidence").and_then(serde_json::Value::as_f64) {
                builder = builder.constraint(ann_id, "confidence", &conf.to_string());
            }

            // Bounding boxes nested inside the annotation's bounding-box sequence
            if let Some(bbseq) = ann.get("boundingBoxSequence") {
                if let Some(boxes) = bbseq.get("boxes").and_then(serde_json::Value::as_array) {
                    for (bi, bb) in boxes.iter().enumerate() {
                        let bb_id = format!("{ann_id}.bbox_{bi}");
                        add_vertex!(&bb_id, "bounding-box");
                        if let Some(fn_val) =
                            bb.get("frameNumber").and_then(serde_json::Value::as_i64)
                        {
                            builder =
                                builder.constraint(&bb_id, "frame-number", &fn_val.to_string());
                        }
                        if let Some(x) = bb.get("x").and_then(serde_json::Value::as_f64) {
                            builder = builder.constraint(&bb_id, "x", &x.to_string());
                        }
                        if let Some(y) = bb.get("y").and_then(serde_json::Value::as_f64) {
                            builder = builder.constraint(&bb_id, "y", &y.to_string());
                        }
                        if let Some(w) = bb.get("width").and_then(serde_json::Value::as_f64) {
                            builder = builder.constraint(&bb_id, "width", &w.to_string());
                        }
                        if let Some(h) = bb.get("height").and_then(serde_json::Value::as_f64) {
                            builder = builder.constraint(&bb_id, "height", &h.to_string());
                        }
                    }
                }
            }
        }
    }

    // Videos
    if let Some(videos) = json.get("videos").and_then(serde_json::Value::as_array) {
        for vid in videos {
            let vid_id = required_str(vid, "id")?;
            add_vertex!(vid_id, "video");
            if let Some(name) = vid.get("title").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(vid_id, "name", name);
            }
        }
    }

    // Video summaries
    if let Some(summaries) = json
        .get("videoSummaries")
        .and_then(serde_json::Value::as_array)
    {
        for sum in summaries {
            let sum_id = required_str(sum, "id")?;
            add_vertex!(sum_id, "video-summary");
        }
    }

    // Claims
    if let Some(claims) = json.get("claims").and_then(serde_json::Value::as_array) {
        for cl in claims {
            let cl_id = required_str(cl, "id")?;
            add_vertex!(cl_id, "claim");
            if let Some(conf) = cl.get("confidence").and_then(serde_json::Value::as_f64) {
                builder = builder.constraint(cl_id, "confidence", &conf.to_string());
            }
            if let Some(label) = cl.get("text").and_then(serde_json::Value::as_str) {
                builder = builder.constraint(cl_id, "label", label);
            }
        }
    }

    // Claim relations
    if let Some(claim_rels) = json
        .get("claimRelations")
        .and_then(serde_json::Value::as_array)
    {
        for cr in claim_rels {
            let cr_id = required_str(cr, "id")?;
            add_vertex!(cr_id, "claim-relation");
            if let Some(conf) = cr.get("confidence").and_then(serde_json::Value::as_f64) {
                builder = builder.constraint(cr_id, "confidence", &conf.to_string());
            }
        }
    }

    // ── Pass 2: register all edges ────────────────────────────────────────────

    // persona → ontology via has-ontology
    if let Some(ontologies) = json
        .get("personaOntologies")
        .and_then(serde_json::Value::as_array)
    {
        for ont in ontologies {
            let ont_id = required_str(ont, "id")?;
            let persona_id = required_str(ont, "personaId")?;
            add_edge!(persona_id, ont_id, "has-ontology", None);

            // ontology → entity-type, role-type, event-type, relation-type via contains
            for type_arr_key in &["entities", "roles", "events", "relationTypes"] {
                if let Some(type_arr) = ont.get(type_arr_key).and_then(serde_json::Value::as_array)
                {
                    for t in type_arr {
                        let t_id = required_str(t, "id")?;
                        add_edge!(ont_id, t_id, "contains", None);
                    }
                }
            }

            // ontology → ontology-relation via contains
            if let Some(relations) = ont.get("relations").and_then(serde_json::Value::as_array) {
                for rel in relations {
                    let rel_id = required_str(rel, "id")?;
                    add_edge!(ont_id, rel_id, "contains", None);
                }
            }

            // event-type → parent event-type via contains (hierarchy);
            // event-type → role-type via has-role
            if let Some(event_types) = ont.get("events").and_then(serde_json::Value::as_array) {
                for evt_t in event_types {
                    let evt_t_id = required_str(evt_t, "id")?;

                    if let Some(parent_id) = evt_t
                        .get("parentEventId")
                        .and_then(serde_json::Value::as_str)
                    {
                        add_edge!(evt_t_id, parent_id, "contains", None);
                    }

                    if let Some(roles) = evt_t.get("roles").and_then(serde_json::Value::as_array) {
                        for role_slot in roles {
                            if let Some(rt_id) = role_slot
                                .get("roleTypeId")
                                .and_then(serde_json::Value::as_str)
                            {
                                add_edge!(evt_t_id, rt_id, "has-role", None);
                            }
                        }
                    }
                }
            }

            // ontology-relation → source/target via relates
            if let Some(relations) = ont.get("relations").and_then(serde_json::Value::as_array) {
                for rel in relations {
                    let rel_id = required_str(rel, "id")?;
                    if let Some(src_id) = rel.get("sourceId").and_then(serde_json::Value::as_str) {
                        add_edge!(rel_id, src_id, "relates", Some("source"));
                    }
                    if let Some(tgt_id) = rel.get("targetId").and_then(serde_json::Value::as_str) {
                        add_edge!(rel_id, tgt_id, "relates", Some("target"));
                    }
                }
            }
        }
    }

    // World-state edges
    if let Some(world) = json.get("world") {
        // entity → entity-type via type-assignment (edge name = personaId)
        if let Some(entities) = world.get("entities").and_then(serde_json::Value::as_array) {
            for ent in entities {
                let ent_id = required_str(ent, "id")?;
                if let Some(assignments) = ent
                    .get("typeAssignments")
                    .and_then(serde_json::Value::as_array)
                {
                    for asgn in assignments {
                        let persona_id = asgn
                            .get("personaId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        let et_id = asgn
                            .get("entityTypeId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        if !persona_id.is_empty() && !et_id.is_empty() {
                            add_edge!(ent_id, et_id, "type-assignment", Some(persona_id));
                        }
                        if let Some(conf) =
                            asgn.get("confidence").and_then(serde_json::Value::as_f64)
                        {
                            builder = builder.constraint(ent_id, "confidence", &conf.to_string());
                        }
                        if let Some(j) = asgn
                            .get("justification")
                            .and_then(serde_json::Value::as_str)
                        {
                            builder = builder.constraint(ent_id, "justification", j);
                        }
                    }
                }
            }
        }

        // event → event-type via interpretation (edge name = personaId)
        if let Some(events) = world.get("events").and_then(serde_json::Value::as_array) {
            for ev in events {
                let ev_id = required_str(ev, "id")?;
                if let Some(interps) = ev
                    .get("personaInterpretations")
                    .and_then(serde_json::Value::as_array)
                {
                    for interp in interps {
                        let persona_id = interp
                            .get("personaId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        let evt_t_id = interp
                            .get("eventTypeId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("");
                        if !persona_id.is_empty() && !evt_t_id.is_empty() {
                            add_edge!(ev_id, evt_t_id, "interpretation", Some(persona_id));
                        }
                    }
                }
            }
        }

        // entity-collection → entity via contains
        if let Some(ecols) = world
            .get("entityCollections")
            .and_then(serde_json::Value::as_array)
        {
            for ec in ecols {
                let ec_id = required_str(ec, "id")?;
                if let Some(entity_ids) = ec.get("entityIds").and_then(serde_json::Value::as_array)
                {
                    for eid_val in entity_ids {
                        if let Some(eid) = eid_val.as_str() {
                            add_edge!(ec_id, eid, "contains", None);
                        }
                    }
                }
            }
        }

        // event-collection → event via contains
        if let Some(evcols) = world
            .get("eventCollections")
            .and_then(serde_json::Value::as_array)
        {
            for evc in evcols {
                let evc_id = required_str(evc, "id")?;
                if let Some(event_ids) = evc.get("eventIds").and_then(serde_json::Value::as_array) {
                    for eid_val in event_ids {
                        if let Some(eid) = eid_val.as_str() {
                            add_edge!(evc_id, eid, "contains", None);
                        }
                    }
                }
            }
        }
    }

    // annotation → world-object via annotates; annotation → bounding-box via has-bbox
    if let Some(annotations) = json
        .get("annotations")
        .and_then(serde_json::Value::as_array)
    {
        for ann in annotations {
            let ann_id = required_str(ann, "id")?;

            for link_field in &[
                "linkedEntityId",
                "linkedEventId",
                "linkedTimeId",
                "linkedLocationId",
            ] {
                if let Some(linked_id) = ann.get(link_field).and_then(serde_json::Value::as_str) {
                    add_edge!(ann_id, linked_id, "annotates", None);
                }
            }

            if let Some(bbseq) = ann.get("boundingBoxSequence") {
                if let Some(boxes) = bbseq.get("boxes").and_then(serde_json::Value::as_array) {
                    for (bi, _bb) in boxes.iter().enumerate() {
                        let bb_id = format!("{ann_id}.bbox_{bi}");
                        let bi_str = bi.to_string();
                        add_edge!(ann_id, &bb_id, "has-bbox", Some(bi_str.as_str()));
                    }
                }
            }
        }
    }

    // video-summary → video/persona via prop
    if let Some(summaries) = json
        .get("videoSummaries")
        .and_then(serde_json::Value::as_array)
    {
        for sum in summaries {
            let sum_id = required_str(sum, "id")?;
            if let Some(vid_id) = sum.get("videoId").and_then(serde_json::Value::as_str) {
                add_edge!(sum_id, vid_id, "prop", Some("video"));
            }
            if let Some(pid) = sum.get("personaId").and_then(serde_json::Value::as_str) {
                add_edge!(sum_id, pid, "prop", Some("persona"));
            }
        }
    }

    // claim → parent-claim via parent-claim
    if let Some(claims) = json.get("claims").and_then(serde_json::Value::as_array) {
        for cl in claims {
            let cl_id = required_str(cl, "id")?;
            if let Some(parent_id) = cl.get("parentClaimId").and_then(serde_json::Value::as_str) {
                add_edge!(cl_id, parent_id, "parent-claim", None);
            }
        }
    }

    // claim-relation → source/target claim via claim-rel
    if let Some(claim_rels) = json
        .get("claimRelations")
        .and_then(serde_json::Value::as_array)
    {
        for cr in claim_rels {
            let cr_id = required_str(cr, "id")?;
            if let Some(src_id) = cr.get("sourceClaimId").and_then(serde_json::Value::as_str) {
                add_edge!(cr_id, src_id, "claim-rel", Some("source"));
            }
            if let Some(tgt_id) = cr.get("targetClaimId").and_then(serde_json::Value::as_str) {
                add_edge!(cr_id, tgt_id, "claim-rel", Some("target"));
            }
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] back to a JSON FOVEA document.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
#[allow(clippy::too_many_lines)]
pub fn emit_fovea(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let mut personas = Vec::new();
    let mut persona_ontologies = Vec::new();
    let mut entities = Vec::new();
    let mut events = Vec::new();
    let mut locations = Vec::new();
    let mut times = Vec::new();
    let mut entity_collections = Vec::new();
    let mut event_collections = Vec::new();
    let mut annotations = Vec::new();
    let mut videos = Vec::new();
    let mut video_summaries = Vec::new();
    let mut claims = Vec::new();
    let mut claim_relations = Vec::new();

    // Sort vertices for deterministic output.
    let mut verts: Vec<_> = schema.vertices.values().collect();
    verts.sort_by(|a, b| a.id.cmp(&b.id));

    for v in &verts {
        let mut obj = serde_json::Map::new();
        obj.insert("id".into(), serde_json::json!(v.id));

        // Collect all constraints for this vertex.
        for c in vertex_constraints(schema, &v.id) {
            obj.insert(c.sort.to_string(), serde_json::json!(c.value));
        }

        match v.kind.as_str() {
            "persona" => {
                personas.push(serde_json::Value::Object(obj));
            }
            "ontology" => {
                obj.insert(
                    "personaId".into(),
                    // The persona that owns this ontology is the src of has-ontology
                    serde_json::json!(
                        schema
                            .incoming_edges(&v.id)
                            .iter()
                            .find(|e| e.kind == "has-ontology")
                            .map_or("", |e| e.src.as_str())
                    ),
                );

                // Collect contained types.
                let contained = children_by_edge(schema, &v.id, "contains");
                let mut et_arr = Vec::new();
                let mut rt_arr = Vec::new();
                let mut evt_arr = Vec::new();
                let mut relt_arr = Vec::new();
                let mut rel_arr = Vec::new();

                for (_e, child) in &contained {
                    let mut child_obj = serde_json::Map::new();
                    child_obj.insert("id".into(), serde_json::json!(child.id));
                    for c in vertex_constraints(schema, &child.id) {
                        child_obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                    }
                    match child.kind.as_str() {
                        "entity-type" => et_arr.push(serde_json::Value::Object(child_obj)),
                        "role-type" => rt_arr.push(serde_json::Value::Object(child_obj)),
                        "event-type" => {
                            // Attach has-role edges.
                            let role_edges = children_by_edge(schema, &child.id, "has-role");
                            let roles_json: Vec<serde_json::Value> = role_edges
                                .iter()
                                .map(|(_, rt)| serde_json::json!({ "roleTypeId": rt.id }))
                                .collect();
                            child_obj.insert("roles".into(), serde_json::Value::Array(roles_json));
                            evt_arr.push(serde_json::Value::Object(child_obj));
                        }
                        "relation-type" => relt_arr.push(serde_json::Value::Object(child_obj)),
                        "ontology-relation" => {
                            // Recover source/target from relates edges.
                            let relates = children_by_edge(schema, &child.id, "relates");
                            for (re, rt_v) in &relates {
                                if re.name.as_deref() == Some("source") {
                                    child_obj.insert("sourceId".into(), serde_json::json!(rt_v.id));
                                } else if re.name.as_deref() == Some("target") {
                                    child_obj.insert("targetId".into(), serde_json::json!(rt_v.id));
                                }
                            }
                            rel_arr.push(serde_json::Value::Object(child_obj));
                        }
                        _ => {}
                    }
                }

                obj.insert("entities".into(), serde_json::Value::Array(et_arr));
                obj.insert("roles".into(), serde_json::Value::Array(rt_arr));
                obj.insert("events".into(), serde_json::Value::Array(evt_arr));
                obj.insert("relationTypes".into(), serde_json::Value::Array(relt_arr));
                obj.insert("relations".into(), serde_json::Value::Array(rel_arr));
                persona_ontologies.push(serde_json::Value::Object(obj));
            }
            "entity" => {
                // Reconstruct type-assignment edges.
                let asgns: Vec<serde_json::Value> = schema
                    .outgoing_edges(&v.id)
                    .iter()
                    .filter(|e| e.kind == "type-assignment")
                    .filter_map(|e| {
                        e.name.as_deref().map(|pid| {
                            serde_json::json!({
                                "personaId": pid,
                                "entityTypeId": e.tgt,
                            })
                        })
                    })
                    .collect();
                obj.insert("typeAssignments".into(), serde_json::Value::Array(asgns));
                entities.push(serde_json::Value::Object(obj));
            }
            "event" => {
                // Reconstruct interpretation edges.
                let interps: Vec<serde_json::Value> = schema
                    .outgoing_edges(&v.id)
                    .iter()
                    .filter(|e| e.kind == "interpretation")
                    .filter_map(|e| {
                        e.name.as_deref().map(|pid| {
                            serde_json::json!({
                                "personaId": pid,
                                "eventTypeId": e.tgt,
                            })
                        })
                    })
                    .collect();
                obj.insert(
                    "personaInterpretations".into(),
                    serde_json::Value::Array(interps),
                );
                events.push(serde_json::Value::Object(obj));
            }
            "location" => {
                locations.push(serde_json::Value::Object(obj));
            }
            "time" => {
                times.push(serde_json::Value::Object(obj));
            }
            "entity-collection" => {
                let member_ids: Vec<serde_json::Value> =
                    children_by_edge(schema, &v.id, "contains")
                        .iter()
                        .map(|(_, child)| serde_json::json!(child.id))
                        .collect();
                obj.insert("entityIds".into(), serde_json::Value::Array(member_ids));
                entity_collections.push(serde_json::Value::Object(obj));
            }
            "event-collection" => {
                let member_ids: Vec<serde_json::Value> =
                    children_by_edge(schema, &v.id, "contains")
                        .iter()
                        .map(|(_, child)| serde_json::json!(child.id))
                        .collect();
                obj.insert("eventIds".into(), serde_json::Value::Array(member_ids));
                event_collections.push(serde_json::Value::Object(obj));
            }
            "annotation" => {
                // Linked objects
                let annotates = children_by_edge(schema, &v.id, "annotates");
                for (_e, linked) in &annotates {
                    let field = match linked.kind.as_str() {
                        "entity" => "linkedEntityId",
                        "event" => "linkedEventId",
                        "time" => "linkedTimeId",
                        "location" => "linkedLocationId",
                        _ => continue,
                    };
                    obj.insert(field.into(), serde_json::json!(linked.id));
                }

                // Bounding boxes
                let bb_edges = children_by_edge(schema, &v.id, "has-bbox");
                let boxes_json: Vec<serde_json::Value> = bb_edges
                    .iter()
                    .map(|(_, bb_v)| {
                        let mut bb_obj = serde_json::Map::new();
                        for c in vertex_constraints(schema, &bb_v.id) {
                            bb_obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                        }
                        serde_json::Value::Object(bb_obj)
                    })
                    .collect();
                if !boxes_json.is_empty() {
                    obj.insert(
                        "boundingBoxSequence".into(),
                        serde_json::json!({ "boxes": boxes_json }),
                    );
                }
                annotations.push(serde_json::Value::Object(obj));
            }
            "video" => {
                // Re-key 'name' constraint as 'title' on output
                if let Some(name_val) = constraint_value(schema, &v.id, "name") {
                    obj.insert("title".into(), serde_json::json!(name_val));
                }
                videos.push(serde_json::Value::Object(obj));
            }
            "video-summary" => {
                // Recover video/persona links from prop edges.
                for (e, tgt) in children_by_edge(schema, &v.id, "prop") {
                    match e.name.as_deref() {
                        Some("video") => {
                            obj.insert("videoId".into(), serde_json::json!(tgt.id));
                        }
                        Some("persona") => {
                            obj.insert("personaId".into(), serde_json::json!(tgt.id));
                        }
                        _ => {}
                    }
                }
                video_summaries.push(serde_json::Value::Object(obj));
            }
            "claim" => {
                // Recover parent-claim link.
                let parent_edges = children_by_edge(schema, &v.id, "parent-claim");
                if let Some((_, parent_v)) = parent_edges.first() {
                    obj.insert("parentClaimId".into(), serde_json::json!(parent_v.id));
                }
                claims.push(serde_json::Value::Object(obj));
            }
            "claim-relation" => {
                // Recover source/target claims.
                for (e, tgt) in children_by_edge(schema, &v.id, "claim-rel") {
                    match e.name.as_deref() {
                        Some("source") => {
                            obj.insert("sourceClaimId".into(), serde_json::json!(tgt.id));
                        }
                        Some("target") => {
                            obj.insert("targetClaimId".into(), serde_json::json!(tgt.id));
                        }
                        _ => {}
                    }
                }
                claim_relations.push(serde_json::Value::Object(obj));
            }
            // Leaf / scalar kinds: not emitted as top-level arrays.
            "entity-type" | "event-type" | "role-type" | "relation-type" | "ontology-relation"
            | "bounding-box" | "string" | "boolean" | "float" => {}
            _ => {}
        }
    }

    Ok(serde_json::json!({
        "personas": personas,
        "personaOntologies": persona_ontologies,
        "world": {
            "entities": entities,
            "events": events,
            "locations": locations,
            "times": times,
            "entityCollections": entity_collections,
            "eventCollections": event_collections,
        },
        "annotations": annotations,
        "videos": videos,
        "videoSummaries": video_summaries,
        "claims": claims,
        "claimRelations": claim_relations,
    }))
}

// ── Edge rules ────────────────────────────────────────────────────────────────

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "has-ontology".into(),
            src_kinds: vec!["persona".into()],
            tgt_kinds: vec!["ontology".into()],
        },
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![
                "ontology".into(),
                "entity-collection".into(),
                "event-collection".into(),
                "event-type".into(),
            ],
            tgt_kinds: vec![
                "entity-type".into(),
                "event-type".into(),
                "role-type".into(),
                "relation-type".into(),
                "ontology-relation".into(),
                "entity".into(),
                "event".into(),
            ],
        },
        EdgeRule {
            edge_kind: "type-assignment".into(),
            src_kinds: vec!["entity".into(), "location".into()],
            tgt_kinds: vec!["entity-type".into()],
        },
        EdgeRule {
            edge_kind: "interpretation".into(),
            src_kinds: vec!["event".into()],
            tgt_kinds: vec!["event-type".into()],
        },
        EdgeRule {
            edge_kind: "has-role".into(),
            src_kinds: vec!["event-type".into()],
            tgt_kinds: vec!["role-type".into()],
        },
        EdgeRule {
            edge_kind: "relates".into(),
            src_kinds: vec!["ontology-relation".into()],
            tgt_kinds: vec![
                "entity-type".into(),
                "event-type".into(),
                "role-type".into(),
                "entity".into(),
                "event".into(),
                "time".into(),
                "claim".into(),
            ],
        },
        EdgeRule {
            edge_kind: "annotates".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: vec![
                "entity".into(),
                "event".into(),
                "time".into(),
                "location".into(),
            ],
        },
        EdgeRule {
            edge_kind: "has-bbox".into(),
            src_kinds: vec!["annotation".into()],
            tgt_kinds: vec!["bounding-box".into()],
        },
        EdgeRule {
            edge_kind: "parent-claim".into(),
            src_kinds: vec!["claim".into()],
            tgt_kinds: vec!["claim".into()],
        },
        EdgeRule {
            edge_kind: "claim-rel".into(),
            src_kinds: vec!["claim-relation".into()],
            tgt_kinds: vec!["claim".into()],
        },
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["video-summary".into()],
            tgt_kinds: vec!["video".into(), "persona".into()],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["entity-collection".into(), "event-collection".into()],
            tgt_kinds: vec!["entity".into(), "event".into()],
        },
    ]
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a required string field from a JSON object.
fn required_str<'a>(obj: &'a serde_json::Value, field: &str) -> Result<&'a str, ProtocolError> {
    obj.get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProtocolError::MissingField(field.into()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "fovea");
        assert_eq!(p.schema_theory, "ThFoveaSchema");
        assert_eq!(p.instance_theory, "ThFoveaInstance");

        // Check vertex kinds
        assert!(p.obj_kinds.contains(&"persona".into()));
        assert!(p.obj_kinds.contains(&"ontology".into()));
        assert!(p.obj_kinds.contains(&"entity-type".into()));
        assert!(p.obj_kinds.contains(&"event-type".into()));
        assert!(p.obj_kinds.contains(&"role-type".into()));
        assert!(p.obj_kinds.contains(&"relation-type".into()));
        assert!(p.obj_kinds.contains(&"ontology-relation".into()));
        assert!(p.obj_kinds.contains(&"entity".into()));
        assert!(p.obj_kinds.contains(&"event".into()));
        assert!(p.obj_kinds.contains(&"location".into()));
        assert!(p.obj_kinds.contains(&"time".into()));
        assert!(p.obj_kinds.contains(&"entity-collection".into()));
        assert!(p.obj_kinds.contains(&"event-collection".into()));
        assert!(p.obj_kinds.contains(&"annotation".into()));
        assert!(p.obj_kinds.contains(&"bounding-box".into()));
        assert!(p.obj_kinds.contains(&"claim".into()));
        assert!(p.obj_kinds.contains(&"claim-relation".into()));
        assert!(p.obj_kinds.contains(&"video".into()));
        assert!(p.obj_kinds.contains(&"video-summary".into()));

        // Check edge rules
        assert!(p.find_edge_rule("has-ontology").is_some());
        assert!(p.find_edge_rule("contains").is_some());
        assert!(p.find_edge_rule("type-assignment").is_some());
        assert!(p.find_edge_rule("interpretation").is_some());
        assert!(p.find_edge_rule("has-role").is_some());
        assert!(p.find_edge_rule("relates").is_some());
        assert!(p.find_edge_rule("annotates").is_some());
        assert!(p.find_edge_rule("has-bbox").is_some());
        assert!(p.find_edge_rule("parent-claim").is_some());
        assert!(p.find_edge_rule("claim-rel").is_some());
        assert!(p.find_edge_rule("prop").is_some());
        assert!(p.find_edge_rule("items").is_some());

        // Check constraint sorts
        assert!(p.constraint_sorts.contains(&"name".into()));
        assert!(p.constraint_sorts.contains(&"confidence".into()));
        assert!(p.constraint_sorts.contains(&"symmetric".into()));
        assert!(p.constraint_sorts.contains(&"transitive".into()));
        assert!(p.constraint_sorts.contains(&"collection-type".into()));
        assert!(p.constraint_sorts.contains(&"location-type".into()));
        assert!(p.constraint_sorts.contains(&"time-type".into()));
        assert!(p.constraint_sorts.contains(&"frame-number".into()));
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThFoveaSchema"));
        assert!(registry.contains_key("ThFoveaInstance"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "personas": [
                {
                    "id": "persona-1",
                    "name": "Intelligence Analyst",
                    "role": "analyst",
                    "informationNeed": "Identify key actors and events"
                }
            ],
            "personaOntologies": [
                {
                    "id": "ont-1",
                    "personaId": "persona-1",
                    "entities": [
                        { "id": "et-person", "name": "Person" },
                        { "id": "et-vehicle", "name": "Vehicle" }
                    ],
                    "roles": [
                        { "id": "rt-agent", "name": "Agent" },
                        { "id": "rt-patient", "name": "Patient" }
                    ],
                    "events": [
                        {
                            "id": "evt-meeting",
                            "name": "Meeting",
                            "parentEventId": null,
                            "roles": [
                                { "roleTypeId": "rt-agent", "optional": false }
                            ]
                        }
                    ],
                    "relationTypes": [
                        {
                            "id": "relt-knows",
                            "name": "knows",
                            "symmetric": true,
                            "transitive": false
                        }
                    ],
                    "relations": [
                        {
                            "id": "rel-1",
                            "relationTypeId": "relt-knows",
                            "sourceId": "et-person",
                            "targetId": "et-person",
                            "source": "et-person"
                        }
                    ]
                }
            ],
            "world": {
                "entities": [
                    {
                        "id": "ent-alice",
                        "name": "Alice",
                        "typeAssignments": [
                            { "personaId": "persona-1", "entityTypeId": "et-person", "confidence": 0.95 }
                        ]
                    }
                ],
                "events": [
                    {
                        "id": "ev-briefing",
                        "name": "Morning Briefing",
                        "personaInterpretations": [
                            { "personaId": "persona-1", "eventTypeId": "evt-meeting", "participants": [] }
                        ]
                    }
                ],
                "locations": [],
                "times": [
                    { "id": "t-1", "label": "Morning", "type": "interval" }
                ],
                "entityCollections": [
                    {
                        "id": "ec-group",
                        "name": "Main Group",
                        "collectionType": "group",
                        "entityIds": ["ent-alice"]
                    }
                ],
                "eventCollections": []
            },
            "annotations": [
                {
                    "id": "ann-1",
                    "videoId": "vid-1",
                    "linkedEntityId": "ent-alice",
                    "confidence": 0.9,
                    "boundingBoxSequence": {
                        "boxes": [
                            { "x": 100.0, "y": 200.0, "width": 50.0, "height": 80.0, "frameNumber": 10 }
                        ]
                    }
                }
            ],
            "videos": [
                { "id": "vid-1", "title": "Surveillance Footage" }
            ],
            "videoSummaries": [
                { "id": "vs-1", "videoId": "vid-1", "personaId": "persona-1" }
            ],
            "claims": [
                { "id": "cl-1", "text": "Alice attends the briefing.", "confidence": 0.85 }
            ],
            "claimRelations": [
                { "id": "cr-1", "sourceClaimId": "cl-1", "targetClaimId": "cl-1", "confidence": 0.7 }
            ]
        });

        let schema = parse_fovea(&json).expect("should parse");

        // Verify persona
        assert!(schema.has_vertex("persona-1"));
        assert_eq!(schema.vertices.get("persona-1").unwrap().kind, "persona");

        // Verify ontology
        assert!(schema.has_vertex("ont-1"));
        assert_eq!(schema.vertices.get("ont-1").unwrap().kind, "ontology");

        // Verify type vertices
        assert!(schema.has_vertex("et-person"));
        assert_eq!(
            schema.vertices.get("et-person").unwrap().kind,
            "entity-type"
        );
        assert!(schema.has_vertex("rt-agent"));
        assert_eq!(schema.vertices.get("rt-agent").unwrap().kind, "role-type");
        assert!(schema.has_vertex("evt-meeting"));
        assert_eq!(
            schema.vertices.get("evt-meeting").unwrap().kind,
            "event-type"
        );
        assert!(schema.has_vertex("relt-knows"));
        assert!(schema.has_vertex("rel-1"));

        // Verify world-state vertices
        assert!(schema.has_vertex("ent-alice"));
        assert_eq!(schema.vertices.get("ent-alice").unwrap().kind, "entity");
        assert!(schema.has_vertex("ev-briefing"));
        assert!(schema.has_vertex("t-1"));
        assert_eq!(schema.vertices.get("t-1").unwrap().kind, "time");
        assert!(schema.has_vertex("ec-group"));

        // Verify annotation and bounding box
        assert!(schema.has_vertex("ann-1"));
        assert!(schema.has_vertex("ann-1.bbox_0"));
        assert_eq!(
            constraint_value(&schema, "ann-1.bbox_0", "frame-number"),
            Some("10")
        );
        assert_eq!(constraint_value(&schema, "ann-1.bbox_0", "x"), Some("100"));

        // Verify video
        assert!(schema.has_vertex("vid-1"));
        assert_eq!(schema.vertices.get("vid-1").unwrap().kind, "video");

        // Verify claims
        assert!(schema.has_vertex("cl-1"));
        assert_eq!(schema.vertices.get("cl-1").unwrap().kind, "claim");
        assert!(schema.has_vertex("cr-1"));
        assert_eq!(schema.vertices.get("cr-1").unwrap().kind, "claim-relation");

        // Constraint checks
        assert_eq!(
            constraint_value(&schema, "persona-1", "role"),
            Some("analyst")
        );
        assert_eq!(
            constraint_value(&schema, "relt-knows", "symmetric"),
            Some("true")
        );
        assert_eq!(
            constraint_value(&schema, "t-1", "time-type"),
            Some("interval")
        );
        assert_eq!(
            constraint_value(&schema, "ec-group", "collection-type"),
            Some("group")
        );

        // Roundtrip: emit and re-parse.
        let emitted = emit_fovea(&schema).expect("should emit");
        let schema2 = parse_fovea(&emitted).expect("should re-parse emitted");
        assert_eq!(schema.vertex_count(), schema2.vertex_count());
    }
}
