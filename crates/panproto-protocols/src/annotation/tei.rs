//! TEI XML (Text Encoding Initiative) protocol definition.
//!
//! Uses Group E theory: constrained multigraph + W-type + metadata.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, find_roots, vertex_constraints};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the TEI protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "tei".into(),
        schema_theory: "ThTeiSchema".into(),
        instance_theory: "ThTeiInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            // Document structure
            "tei-document".into(),
            "tei-header".into(),
            "file-desc".into(),
            "title-stmt".into(),
            "text".into(),
            "body".into(),
            "front".into(),
            "back".into(),
            "div".into(),
            "group".into(),
            // Core inline / block
            "p".into(),
            "ab".into(),
            "head".into(),
            "note".into(),
            "ref".into(),
            "hi".into(),
            "list".into(),
            "item".into(),
            "label".into(),
            // Analysis: sentence, word, character, punctuation, segment
            "s".into(),
            "w".into(),
            "c".into(),
            "pc".into(),
            "seg".into(),
            // Phrases, clauses, referential spans
            "phr".into(),
            "cl".into(),
            "rs".into(),
            // Verse
            "lg".into(),
            "l".into(),
            // Drama
            "sp".into(),
            "speaker".into(),
            "stage".into(),
            // Named entities / dates
            "name".into(),
            "pers-name".into(),
            "place-name".into(),
            "org-name".into(),
            "date".into(),
            "time".into(),
            // Transcription / critical apparatus
            "choice".into(),
            "sic".into(),
            "corr".into(),
            "orig".into(),
            "reg".into(),
            "abbr".into(),
            "expan".into(),
            "app".into(),
            "rdg".into(),
            "lem".into(),
            // Linking
            "link".into(),
            "link-grp".into(),
            // Primitives
            "string".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![
            "xml-id".into(),
            "type".into(),
            "subtype".into(),
            "lang".into(),
            "n".into(),
            "value".into(),
            "rend".into(),
            "resp".into(),
            "when".into(),
            "who".into(),
            "scheme".into(),
            "target".into(),
            "corresp".into(),
            "wit".into(),
            "cause".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for TEI.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_multigraph_wtype_meta(registry, "ThTeiSchema", "ThTeiInstance");
}

/// Parse a JSON-based TEI document schema into a [`Schema`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if parsing fails.
pub fn parse_tei(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    let document = json
        .get("document")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ProtocolError::MissingField("document".into()))?;

    let doc_id = document
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tei-doc");
    builder = builder.vertex(doc_id, "tei-document", None)?;

    if let Some(lang) = document.get("lang").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(doc_id, "lang", lang);
    }

    if let Some(header) = document.get("header").and_then(serde_json::Value::as_object) {
        let header_id = format!("{doc_id}.header");
        builder = builder.vertex(&header_id, "tei-header", None)?;
        builder = builder.edge(doc_id, &header_id, "contains", Some("header"))?;
        builder = parse_header(builder, header, &header_id)?;
    }

    if let Some(text) = document.get("text").and_then(serde_json::Value::as_object) {
        let text_id = format!("{doc_id}.text");
        builder = builder.vertex(&text_id, "text", None)?;
        builder = builder.edge(doc_id, &text_id, "contains", Some("text"))?;
        builder = parse_text_body(builder, text, &text_id)?;
    }

    let schema = builder.build()?;
    Ok(schema)
}

#[allow(clippy::too_many_lines)]
fn parse_header(
    mut b: SchemaBuilder,
    header: &serde_json::Map<String, serde_json::Value>,
    header_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(file_desc) = header.get("fileDesc").and_then(serde_json::Value::as_object) {
        let fd_id = format!("{header_id}.fileDesc");
        b = b.vertex(&fd_id, "file-desc", None)?;
        b = b.edge(header_id, &fd_id, "metadata", Some("fileDesc"))?;

        if let Some(title_stmt) = file_desc
            .get("titleStmt")
            .and_then(serde_json::Value::as_object)
        {
            let ts_id = format!("{fd_id}.titleStmt");
            b = b.vertex(&ts_id, "title-stmt", None)?;
            b = b.edge(&fd_id, &ts_id, "contains", Some("titleStmt"))?;

            if let Some(title) = title_stmt.get("title").and_then(serde_json::Value::as_str) {
                let t_id = format!("{ts_id}.title");
                b = b.vertex(&t_id, "string", None)?;
                b = b.edge(&ts_id, &t_id, "contains", Some("title"))?;
                b = b.constraint(&t_id, "value", title);
            }
        }
    }

    if let Some(notes) = header.get("notes").and_then(serde_json::Value::as_array) {
        for (i, note) in notes.iter().enumerate() {
            if let Some(note_obj) = note.as_object() {
                let note_id = format!("{header_id}.note.{i}");
                b = b.vertex(&note_id, "note", None)?;
                b = b.edge(header_id, &note_id, "metadata", Some(&format!("note.{i}")))?;
                b = apply_common_attrs(b, note_obj, &note_id);
            }
        }
    }

    Ok(b)
}

fn parse_text_body(
    mut b: SchemaBuilder,
    text: &serde_json::Map<String, serde_json::Value>,
    text_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(body) = text.get("body").and_then(serde_json::Value::as_object) {
        let body_id = format!("{text_id}.body");
        b = b.vertex(&body_id, "body", None)?;
        b = b.edge(text_id, &body_id, "contains", Some("body"))?;

        if let Some(divs) = body.get("divs").and_then(serde_json::Value::as_array) {
            for (i, div) in divs.iter().enumerate() {
                if let Some(div_obj) = div.as_object() {
                    b = parse_div(b, div_obj, &body_id, i)?;
                }
            }
        }
    }

    Ok(b)
}

#[allow(clippy::too_many_lines)]
fn parse_div(
    mut b: SchemaBuilder,
    div: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let div_id = div
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.div.{index}"), String::from);
    b = b.vertex(&div_id, "div", None)?;
    b = b.edge(parent_id, &div_id, "contains", Some(&format!("div.{index}")))?;
    b = apply_common_attrs(b, div, &div_id);

    if let Some(paragraphs) = div.get("paragraphs").and_then(serde_json::Value::as_array) {
        for (i, p) in paragraphs.iter().enumerate() {
            if let Some(p_obj) = p.as_object() {
                b = parse_paragraph(b, p_obj, &div_id, i)?;
            }
        }
    }

    if let Some(line_groups) = div.get("lineGroups").and_then(serde_json::Value::as_array) {
        for (i, lg) in line_groups.iter().enumerate() {
            if let Some(lg_obj) = lg.as_object() {
                b = parse_line_group(b, lg_obj, &div_id, i)?;
            }
        }
    }

    if let Some(speeches) = div.get("speeches").and_then(serde_json::Value::as_array) {
        for (i, sp) in speeches.iter().enumerate() {
            if let Some(sp_obj) = sp.as_object() {
                b = parse_speech(b, sp_obj, &div_id, i)?;
            }
        }
    }

    if let Some(apps) = div.get("apps").and_then(serde_json::Value::as_array) {
        for (i, app) in apps.iter().enumerate() {
            if let Some(app_obj) = app.as_object() {
                b = parse_app(b, app_obj, &div_id, i)?;
            }
        }
    }

    if let Some(sub_divs) = div.get("divs").and_then(serde_json::Value::as_array) {
        for (i, sub_div) in sub_divs.iter().enumerate() {
            if let Some(sub_div_obj) = sub_div.as_object() {
                b = parse_div(b, sub_div_obj, &div_id, i)?;
            }
        }
    }

    Ok(b)
}

/// Parse a `<lg>` (line group) element containing verse lines.
fn parse_line_group(
    mut b: SchemaBuilder,
    lg: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let lg_id = lg
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.lg.{index}"), String::from);
    b = b.vertex(&lg_id, "lg", None)?;
    b = b.edge(parent_id, &lg_id, "contains", Some(&format!("lg.{index}")))?;
    b = apply_common_attrs(b, lg, &lg_id);

    if let Some(lines) = lg.get("lines").and_then(serde_json::Value::as_array) {
        for (i, l) in lines.iter().enumerate() {
            if let Some(l_obj) = l.as_object() {
                let l_id = l_obj
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| format!("{lg_id}.l.{i}"), String::from);
                b = b.vertex(&l_id, "l", None)?;
                b = b.edge(&lg_id, &l_id, "contains", Some(&format!("l.{i}")))?;
                b = apply_common_attrs(b, l_obj, &l_id);

                if let Some(tokens) = l_obj.get("tokens").and_then(serde_json::Value::as_array) {
                    for (j, tok) in tokens.iter().enumerate() {
                        if let Some(tok_obj) = tok.as_object() {
                            b = parse_token(b, tok_obj, &l_id, j)?;
                        }
                    }
                }
            }
        }
    }

    Ok(b)
}

/// Parse a `<sp>` (speech) element for drama.
fn parse_speech(
    mut b: SchemaBuilder,
    sp: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let sp_id = sp
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.sp.{index}"), String::from);
    b = b.vertex(&sp_id, "sp", None)?;
    b = b.edge(parent_id, &sp_id, "contains", Some(&format!("sp.{index}")))?;
    b = apply_common_attrs(b, sp, &sp_id);

    if let Some(who) = sp.get("who").and_then(serde_json::Value::as_str) {
        b = b.constraint(&sp_id, "who", who);
    }

    if let Some(speaker) = sp.get("speaker").and_then(serde_json::Value::as_str) {
        let spkr_id = format!("{sp_id}.speaker");
        b = b.vertex(&spkr_id, "speaker", None)?;
        b = b.edge(&sp_id, &spkr_id, "contains", Some("speaker"))?;
        b = b.constraint(&spkr_id, "value", speaker);
    }

    if let Some(stage) = sp.get("stage").and_then(serde_json::Value::as_str) {
        let stage_id = format!("{sp_id}.stage");
        b = b.vertex(&stage_id, "stage", None)?;
        b = b.edge(&sp_id, &stage_id, "contains", Some("stage"))?;
        b = b.constraint(&stage_id, "value", stage);
    }

    if let Some(paragraphs) = sp.get("paragraphs").and_then(serde_json::Value::as_array) {
        for (i, p) in paragraphs.iter().enumerate() {
            if let Some(p_obj) = p.as_object() {
                b = parse_paragraph(b, p_obj, &sp_id, i)?;
            }
        }
    }

    Ok(b)
}

/// Parse an `<app>` (apparatus entry) containing `<lem>` and `<rdg>` witnesses.
fn parse_app(
    mut b: SchemaBuilder,
    app: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let app_id = app
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.app.{index}"), String::from);
    b = b.vertex(&app_id, "app", None)?;
    b = b.edge(parent_id, &app_id, "contains", Some(&format!("app.{index}")))?;
    b = apply_common_attrs(b, app, &app_id);

    if let Some(lem_obj) = app.get("lem").and_then(serde_json::Value::as_object) {
        let lem_id = format!("{app_id}.lem");
        b = b.vertex(&lem_id, "lem", None)?;
        b = b.edge(&app_id, &lem_id, "contains", Some("lem"))?;
        b = apply_common_attrs(b, lem_obj, &lem_id);
        if let Some(v) = lem_obj.get("value").and_then(serde_json::Value::as_str) {
            b = b.constraint(&lem_id, "value", v);
        }
    }

    if let Some(readings) = app.get("readings").and_then(serde_json::Value::as_array) {
        for (i, rdg) in readings.iter().enumerate() {
            if let Some(rdg_obj) = rdg.as_object() {
                let rdg_id = format!("{app_id}.rdg.{i}");
                b = b.vertex(&rdg_id, "rdg", None)?;
                b = b.edge(&app_id, &rdg_id, "contains", Some(&format!("rdg.{i}")))?;
                b = apply_common_attrs(b, rdg_obj, &rdg_id);
                if let Some(wit) = rdg_obj.get("wit").and_then(serde_json::Value::as_str) {
                    b = b.constraint(&rdg_id, "wit", wit);
                }
                if let Some(v) = rdg_obj.get("value").and_then(serde_json::Value::as_str) {
                    b = b.constraint(&rdg_id, "value", v);
                }
            }
        }
    }

    Ok(b)
}

#[allow(clippy::too_many_lines)]
fn parse_paragraph(
    mut b: SchemaBuilder,
    para: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let p_id = para
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.p.{index}"), String::from);
    b = b.vertex(&p_id, "p", None)?;
    b = b.edge(parent_id, &p_id, "contains", Some(&format!("p.{index}")))?;
    b = apply_common_attrs(b, para, &p_id);

    if let Some(sentences) = para.get("sentences").and_then(serde_json::Value::as_array) {
        for (i, s) in sentences.iter().enumerate() {
            if let Some(s_obj) = s.as_object() {
                b = parse_sentence(b, s_obj, &p_id, i)?;
            }
        }
    }

    if let Some(tokens) = para.get("tokens").and_then(serde_json::Value::as_array) {
        for (i, tok) in tokens.iter().enumerate() {
            if let Some(tok_obj) = tok.as_object() {
                b = parse_token(b, tok_obj, &p_id, i)?;
            }
        }
    }

    if let Some(entities) = para.get("entities").and_then(serde_json::Value::as_array) {
        for (i, ent) in entities.iter().enumerate() {
            if let Some(ent_obj) = ent.as_object() {
                b = parse_entity(b, ent_obj, &p_id, i)?;
            }
        }
    }

    Ok(b)
}

#[allow(clippy::too_many_lines)]
fn parse_sentence(
    mut b: SchemaBuilder,
    sent: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let s_id = sent
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.s.{index}"), String::from);
    b = b.vertex(&s_id, "s", None)?;
    b = b.edge(parent_id, &s_id, "contains", Some(&format!("s.{index}")))?;
    b = apply_common_attrs(b, sent, &s_id);

    if let Some(tokens) = sent.get("tokens").and_then(serde_json::Value::as_array) {
        for (i, tok) in tokens.iter().enumerate() {
            if let Some(tok_obj) = tok.as_object() {
                b = parse_token(b, tok_obj, &s_id, i)?;
            }
        }
    }

    if let Some(entities) = sent.get("entities").and_then(serde_json::Value::as_array) {
        for (i, ent) in entities.iter().enumerate() {
            if let Some(ent_obj) = ent.as_object() {
                b = parse_entity(b, ent_obj, &s_id, i)?;
            }
        }
    }

    if let Some(segs) = sent.get("segments").and_then(serde_json::Value::as_array) {
        for (i, seg) in segs.iter().enumerate() {
            if let Some(seg_obj) = seg.as_object() {
                let seg_id = seg_obj
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(|| format!("{s_id}.seg.{i}"), String::from);
                b = b.vertex(&seg_id, "seg", None)?;
                b = b.edge(&s_id, &seg_id, "contains", Some(&format!("seg.{i}")))?;
                b = apply_common_attrs(b, seg_obj, &seg_id);
            }
        }
    }

    Ok(b)
}

fn parse_token(
    mut b: SchemaBuilder,
    tok: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let kind = tok
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("w");
    let tok_id = tok
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.{kind}.{index}"), String::from);
    b = b.vertex(&tok_id, kind, None)?;
    b = b.edge(
        parent_id,
        &tok_id,
        "contains",
        Some(&format!("{kind}.{index}")),
    )?;
    b = apply_common_attrs(b, tok, &tok_id);

    if let Some(pos) = tok.get("pos").and_then(serde_json::Value::as_str) {
        let pos_id = format!("{tok_id}.pos");
        b = b.vertex(&pos_id, "string", None)?;
        b = b.edge(&tok_id, &pos_id, "annotation", Some("pos"))?;
        b = b.constraint(&pos_id, "value", pos);
    }
    if let Some(lemma) = tok.get("lemma").and_then(serde_json::Value::as_str) {
        let lemma_id = format!("{tok_id}.lemma");
        b = b.vertex(&lemma_id, "string", None)?;
        b = b.edge(&tok_id, &lemma_id, "annotation", Some("lemma"))?;
        b = b.constraint(&lemma_id, "value", lemma);
    }
    if let Some(morph) = tok.get("morph").and_then(serde_json::Value::as_object) {
        for (feat, val) in morph {
            if let Some(val_str) = val.as_str() {
                let m_id = format!("{tok_id}.m.{feat}");
                b = b.vertex(&m_id, "string", None)?;
                b = b.edge(&tok_id, &m_id, "annotation", Some(feat))?;
                b = b.constraint(&m_id, "value", val_str);
            }
        }
    }

    Ok(b)
}

fn parse_entity(
    mut b: SchemaBuilder,
    ent: &serde_json::Map<String, serde_json::Value>,
    parent_id: &str,
    index: usize,
) -> Result<SchemaBuilder, ProtocolError> {
    let kind = ent
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("name");
    let ent_id = ent
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("{parent_id}.{kind}.{index}"), String::from);
    b = b.vertex(&ent_id, kind, None)?;
    b = b.edge(
        parent_id,
        &ent_id,
        "contains",
        Some(&format!("{kind}.{index}")),
    )?;
    b = apply_common_attrs(b, ent, &ent_id);

    // Named entities may carry annotation (e.g. confidence, ref)
    if let Some(corresp) = ent.get("corresp").and_then(serde_json::Value::as_str) {
        b = b.constraint(&ent_id, "corresp", corresp);
    }

    Ok(b)
}

fn apply_common_attrs(
    mut b: SchemaBuilder,
    obj: &serde_json::Map<String, serde_json::Value>,
    vertex_id: &str,
) -> SchemaBuilder {
    let attr_sorts = [
        "xml-id", "type", "subtype", "lang", "n", "rend", "resp", "when", "who", "scheme",
    ];
    for sort in &attr_sorts {
        if let Some(val) = obj.get(*sort).and_then(serde_json::Value::as_str) {
            b = b.constraint(vertex_id, sort, val);
        }
    }
    b
}

/// Emit a [`Schema`] as a JSON TEI document representation.
///
/// # Errors
///
/// Returns [`ProtocolError`] if emission fails.
pub fn emit_tei(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    let structural = &["contains", "metadata", "annotation", "link"];
    let roots = find_roots(schema, structural);

    let doc_root = roots
        .iter()
        .find(|v| v.kind == "tei-document")
        .ok_or_else(|| ProtocolError::Emit("no tei-document root found".into()))?;

    let mut doc_obj = serde_json::Map::new();
    doc_obj.insert("id".into(), serde_json::json!(doc_root.id));

    for c in vertex_constraints(schema, &doc_root.id) {
        if c.sort == "lang" {
            doc_obj.insert("lang".into(), serde_json::json!(c.value));
        }
    }

    let header_children = children_by_edge(schema, &doc_root.id, "contains");
    for (_, child) in &header_children {
        if child.kind == "tei-header" {
            doc_obj.insert("header".into(), emit_header(schema, &child.id)?);
        }
        if child.kind == "text" {
            doc_obj.insert("text".into(), emit_text_body(schema, &child.id)?);
        }
    }

    Ok(serde_json::json!({ "document": doc_obj }))
}

#[allow(clippy::unnecessary_wraps)]
fn emit_header(
    schema: &Schema,
    header_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut header = serde_json::Map::new();

    let meta_children = children_by_edge(schema, header_id, "metadata");
    for (_, child) in &meta_children {
        if child.kind == "file-desc" {
            let mut fd = serde_json::Map::new();
            let fd_children = children_by_edge(schema, &child.id, "contains");
            for (_, fc) in &fd_children {
                if fc.kind == "title-stmt" {
                    let mut ts = serde_json::Map::new();
                    let ts_children = children_by_edge(schema, &fc.id, "contains");
                    for (_, tc) in &ts_children {
                        if tc.kind == "string" {
                            for c in vertex_constraints(schema, &tc.id) {
                                if c.sort == "value" {
                                    ts.insert("title".into(), serde_json::json!(c.value));
                                }
                            }
                        }
                    }
                    fd.insert("titleStmt".into(), serde_json::Value::Object(ts));
                }
            }
            header.insert("fileDesc".into(), serde_json::Value::Object(fd));
        }
        if child.kind == "note" {
            let notes = header
                .entry("notes")
                .or_insert_with(|| serde_json::json!([]));
            if let serde_json::Value::Array(arr) = notes {
                arr.push(emit_constrained_obj(schema, &child.id));
            }
        }
    }

    Ok(serde_json::Value::Object(header))
}

fn emit_text_body(
    schema: &Schema,
    text_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut text_obj = serde_json::Map::new();

    let text_children = children_by_edge(schema, text_id, "contains");
    for (_, child) in &text_children {
        if child.kind == "body" {
            let mut body_obj = serde_json::Map::new();
            let body_children = children_by_edge(schema, &child.id, "contains");
            let mut divs = Vec::new();
            for (_, bc) in &body_children {
                if bc.kind == "div" {
                    divs.push(emit_div(schema, &bc.id)?);
                }
            }
            if !divs.is_empty() {
                body_obj.insert("divs".into(), serde_json::Value::Array(divs));
            }
            text_obj.insert("body".into(), serde_json::Value::Object(body_obj));
        }
    }

    Ok(serde_json::Value::Object(text_obj))
}

#[allow(clippy::too_many_lines)]
fn emit_div(
    schema: &Schema,
    div_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = emit_constrained_map(schema, div_id);

    let children = children_by_edge(schema, div_id, "contains");
    let mut paragraphs = Vec::new();
    let mut sub_divs = Vec::new();
    let mut line_groups = Vec::new();
    let mut speeches = Vec::new();
    let mut apps = Vec::new();
    for (_, child) in &children {
        match child.kind.as_str() {
            "p" => paragraphs.push(emit_paragraph(schema, &child.id)?),
            "div" => sub_divs.push(emit_div(schema, &child.id)?),
            "lg" => line_groups.push(emit_line_group(schema, &child.id)?),
            "sp" => speeches.push(emit_speech(schema, &child.id)?),
            "app" => apps.push(emit_app(schema, &child.id)?),
            _ => {}
        }
    }
    if !paragraphs.is_empty() {
        obj.insert("paragraphs".into(), serde_json::Value::Array(paragraphs));
    }
    if !sub_divs.is_empty() {
        obj.insert("divs".into(), serde_json::Value::Array(sub_divs));
    }
    if !line_groups.is_empty() {
        obj.insert("lineGroups".into(), serde_json::Value::Array(line_groups));
    }
    if !speeches.is_empty() {
        obj.insert("speeches".into(), serde_json::Value::Array(speeches));
    }
    if !apps.is_empty() {
        obj.insert("apps".into(), serde_json::Value::Array(apps));
    }

    Ok(serde_json::Value::Object(obj))
}

fn emit_line_group(
    schema: &Schema,
    lg_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = emit_constrained_map(schema, lg_id);
    let children = children_by_edge(schema, lg_id, "contains");
    let mut lines = Vec::new();
    for (_, child) in &children {
        if child.kind == "l" {
            let mut l_obj = emit_constrained_map(schema, &child.id);
            let tok_children = children_by_edge(schema, &child.id, "contains");
            let mut tokens = Vec::new();
            for (_, tc) in &tok_children {
                match tc.kind.as_str() {
                    "w" | "pc" | "c" => tokens.push(emit_token(schema, &tc.id)?),
                    _ => {}
                }
            }
            if !tokens.is_empty() {
                l_obj.insert("tokens".into(), serde_json::Value::Array(tokens));
            }
            lines.push(serde_json::Value::Object(l_obj));
        }
    }
    if !lines.is_empty() {
        obj.insert("lines".into(), serde_json::Value::Array(lines));
    }
    Ok(serde_json::Value::Object(obj))
}

fn emit_speech(
    schema: &Schema,
    sp_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = emit_constrained_map(schema, sp_id);
    let children = children_by_edge(schema, sp_id, "contains");
    let mut paragraphs = Vec::new();
    for (_, child) in &children {
        match child.kind.as_str() {
            "speaker" => {
                if let Some(c) = vertex_constraints(schema, &child.id)
                    .into_iter()
                    .find(|c| c.sort == "value")
                {
                    obj.insert("speaker".into(), serde_json::json!(c.value));
                }
            }
            "stage" => {
                if let Some(c) = vertex_constraints(schema, &child.id)
                    .into_iter()
                    .find(|c| c.sort == "value")
                {
                    obj.insert("stage".into(), serde_json::json!(c.value));
                }
            }
            "p" => paragraphs.push(emit_paragraph(schema, &child.id)?),
            _ => {}
        }
    }
    if !paragraphs.is_empty() {
        obj.insert("paragraphs".into(), serde_json::Value::Array(paragraphs));
    }
    Ok(serde_json::Value::Object(obj))
}

#[allow(clippy::unnecessary_wraps)]
fn emit_app(
    schema: &Schema,
    app_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = emit_constrained_map(schema, app_id);
    let children = children_by_edge(schema, app_id, "contains");
    let mut readings = Vec::new();
    for (_, child) in &children {
        match child.kind.as_str() {
            "lem" => {
                obj.insert("lem".into(), emit_constrained_obj(schema, &child.id));
            }
            "rdg" => {
                readings.push(emit_constrained_obj(schema, &child.id));
            }
            _ => {}
        }
    }
    if !readings.is_empty() {
        obj.insert("readings".into(), serde_json::Value::Array(readings));
    }
    Ok(serde_json::Value::Object(obj))
}

#[allow(clippy::too_many_lines)]
fn emit_paragraph(
    schema: &Schema,
    p_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = emit_constrained_map(schema, p_id);

    let children = children_by_edge(schema, p_id, "contains");
    let mut sentences = Vec::new();
    let mut tokens = Vec::new();
    let mut entities = Vec::new();
    for (_, child) in &children {
        match child.kind.as_str() {
            "s" => sentences.push(emit_sentence(schema, &child.id)?),
            "w" | "pc" | "c" => tokens.push(emit_token(schema, &child.id)?),
            "name" | "pers-name" | "place-name" | "org-name" | "date" | "time" | "rs"
            | "ref" => {
                entities.push(emit_entity(schema, &child.id)?);
            }
            _ => {}
        }
    }
    if !sentences.is_empty() {
        obj.insert("sentences".into(), serde_json::Value::Array(sentences));
    }
    if !tokens.is_empty() {
        obj.insert("tokens".into(), serde_json::Value::Array(tokens));
    }
    if !entities.is_empty() {
        obj.insert("entities".into(), serde_json::Value::Array(entities));
    }

    Ok(serde_json::Value::Object(obj))
}

#[allow(clippy::too_many_lines)]
fn emit_sentence(
    schema: &Schema,
    s_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let mut obj = emit_constrained_map(schema, s_id);

    let children = children_by_edge(schema, s_id, "contains");
    let mut tokens = Vec::new();
    let mut entities = Vec::new();
    let mut segments = Vec::new();
    for (_, child) in &children {
        match child.kind.as_str() {
            "w" | "pc" | "c" => tokens.push(emit_token(schema, &child.id)?),
            "name" | "pers-name" | "place-name" | "org-name" | "date" | "time" | "rs"
            | "ref" => {
                entities.push(emit_entity(schema, &child.id)?);
            }
            "seg" | "phr" | "cl" => {
                segments.push(emit_constrained_obj(schema, &child.id));
            }
            _ => {}
        }
    }
    if !tokens.is_empty() {
        obj.insert("tokens".into(), serde_json::Value::Array(tokens));
    }
    if !entities.is_empty() {
        obj.insert("entities".into(), serde_json::Value::Array(entities));
    }
    if !segments.is_empty() {
        obj.insert("segments".into(), serde_json::Value::Array(segments));
    }

    Ok(serde_json::Value::Object(obj))
}

fn emit_token(
    schema: &Schema,
    tok_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let vertex = schema
        .vertices
        .get(tok_id)
        .ok_or_else(|| ProtocolError::Emit(format!("missing token vertex {tok_id}")))?;
    let mut obj = emit_constrained_map(schema, tok_id);
    if vertex.kind != "w" {
        obj.insert("kind".into(), serde_json::json!(vertex.kind));
    }

    let annotations = children_by_edge(schema, tok_id, "annotation");
    let mut morph = serde_json::Map::new();
    for (edge, ann_v) in &annotations {
        let ann_name = edge.name.as_deref().unwrap_or("");
        let value = vertex_constraints(schema, &ann_v.id)
            .iter()
            .find(|c| c.sort == "value")
            .map(|c| c.value.clone())
            .unwrap_or_default();
        match ann_name {
            "pos" => {
                obj.insert("pos".into(), serde_json::json!(value));
            }
            "lemma" => {
                obj.insert("lemma".into(), serde_json::json!(value));
            }
            _ => {
                morph.insert(ann_name.to_string(), serde_json::json!(value));
            }
        }
    }
    if !morph.is_empty() {
        obj.insert("morph".into(), serde_json::Value::Object(morph));
    }

    Ok(serde_json::Value::Object(obj))
}

fn emit_entity(
    schema: &Schema,
    ent_id: &str,
) -> Result<serde_json::Value, ProtocolError> {
    let vertex = schema
        .vertices
        .get(ent_id)
        .ok_or_else(|| ProtocolError::Emit(format!("missing entity vertex {ent_id}")))?;
    let mut obj = emit_constrained_map(schema, ent_id);
    obj.insert("kind".into(), serde_json::json!(vertex.kind));
    Ok(serde_json::Value::Object(obj))
}

fn emit_constrained_map(
    schema: &Schema,
    vertex_id: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut obj = serde_json::Map::new();
    for c in vertex_constraints(schema, vertex_id) {
        match c.sort.as_str() {
            "xml-id" | "type" | "subtype" | "lang" | "n" | "value" | "rend" | "resp" | "when"
            | "who" | "scheme" | "corresp" | "wit" => {
                obj.insert(c.sort.clone(), serde_json::json!(c.value));
            }
            _ => {}
        }
    }
    obj
}

fn emit_constrained_obj(schema: &Schema, vertex_id: &str) -> serde_json::Value {
    serde_json::Value::Object(emit_constrained_map(schema, vertex_id))
}

#[allow(clippy::too_many_lines)]
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec![
                "tei-document".into(),
                "tei-header".into(),
                "file-desc".into(),
                "title-stmt".into(),
                "text".into(),
                "body".into(),
                "front".into(),
                "back".into(),
                "group".into(),
                "div".into(),
                "p".into(),
                "ab".into(),
                "s".into(),
                "seg".into(),
                "phr".into(),
                "cl".into(),
                "lg".into(),
                "l".into(),
                "sp".into(),
                "app".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "metadata".into(),
            src_kinds: vec!["tei-header".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "annotation".into(),
            src_kinds: vec![
                "w".into(),
                "c".into(),
                "pc".into(),
                "seg".into(),
                "phr".into(),
                "cl".into(),
                "rs".into(),
                // Named entity kinds may carry annotation edges too
                "name".into(),
                "pers-name".into(),
                "place-name".into(),
                "org-name".into(),
                "date".into(),
                "time".into(),
            ],
            tgt_kinds: vec!["string".into(), "integer".into()],
        },
        EdgeRule {
            edge_kind: "link".into(),
            src_kinds: vec!["link".into(), "link-grp".into()],
            tgt_kinds: vec![],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn protocol_def() {
        let p = protocol();
        assert_eq!(p.name, "tei");
        assert_eq!(p.schema_theory, "ThTeiSchema");
        assert_eq!(p.instance_theory, "ThTeiInstance");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThTeiSchema"));
        assert!(registry.contains_key("ThTeiInstance"));
    }

    #[test]
    fn protocol_has_new_vertex_kinds() {
        let p = protocol();
        let kinds: Vec<&str> = p.obj_kinds.iter().map(String::as_str).collect();
        assert!(kinds.contains(&"c"), "missing c (character)");
        assert!(kinds.contains(&"sp"), "missing sp (speech)");
        assert!(kinds.contains(&"speaker"), "missing speaker");
        assert!(kinds.contains(&"stage"), "missing stage");
        assert!(kinds.contains(&"lg"), "missing lg (line group)");
        assert!(kinds.contains(&"l"), "missing l (verse line)");
        assert!(kinds.contains(&"rs"), "missing rs");
        assert!(kinds.contains(&"phr"), "missing phr");
        assert!(kinds.contains(&"cl"), "missing cl");
        assert!(kinds.contains(&"app"), "missing app");
        assert!(kinds.contains(&"rdg"), "missing rdg");
        assert!(kinds.contains(&"lem"), "missing lem");
        assert!(kinds.contains(&"link"), "missing link");
        assert!(kinds.contains(&"link-grp"), "missing link-grp");
    }

    #[test]
    fn protocol_has_new_constraint_sorts() {
        let p = protocol();
        let sorts: Vec<&str> = p.constraint_sorts.iter().map(String::as_str).collect();
        assert!(sorts.contains(&"when"), "missing when");
        assert!(sorts.contains(&"who"), "missing who");
        assert!(sorts.contains(&"scheme"), "missing scheme");
        assert!(sorts.contains(&"value"), "missing value");
    }

    #[test]
    fn annotation_edge_includes_entity_kinds() {
        let rules = edge_rules();
        let ann = rules
            .iter()
            .find(|r| r.edge_kind == "annotation")
            .expect("annotation edge rule missing");
        let src: Vec<&str> = ann.src_kinds.iter().map(String::as_str).collect();
        assert!(src.contains(&"name"));
        assert!(src.contains(&"pers-name"));
        assert!(src.contains(&"place-name"));
        assert!(src.contains(&"org-name"));
        assert!(src.contains(&"date"));
    }

    #[test]
    fn link_edge_rule_present() {
        let rules = edge_rules();
        assert!(
            rules.iter().any(|r| r.edge_kind == "link"),
            "link edge rule missing"
        );
    }

    #[test]
    fn parse_and_emit() {
        let json = serde_json::json!({
            "document": {
                "id": "doc1",
                "lang": "en",
                "header": {
                    "fileDesc": {
                        "titleStmt": {
                            "title": "Sample TEI Document"
                        }
                    }
                },
                "text": {
                    "body": {
                        "divs": [
                            {
                                "id": "d1",
                                "type": "chapter",
                                "n": "1",
                                "paragraphs": [
                                    {
                                        "id": "p1",
                                        "sentences": [
                                            {
                                                "id": "s1",
                                                "tokens": [
                                                    {"id": "w1", "n": "The", "pos": "DET", "lemma": "the"},
                                                    {"id": "w2", "n": "cat", "pos": "NOUN", "lemma": "cat", "morph": {"Number": "Sing"}},
                                                    {"id": "pc1", "kind": "pc", "n": "."}
                                                ],
                                                "entities": [
                                                    {"kind": "pers-name", "n": "Alice", "xml-id": "e1"}
                                                ]
                                            }
                                        ]
                                    }
                                ]
                            }
                        ]
                    }
                }
            }
        });

        let schema = parse_tei(&json).expect("should parse");
        assert!(schema.has_vertex("doc1"));
        assert!(schema.has_vertex("w1"));
        assert!(schema.has_vertex("w1.pos"));

        let emitted = emit_tei(&schema).expect("should emit");
        let doc = emitted.get("document").unwrap();
        assert_eq!(doc.get("id").unwrap(), "doc1");
        assert_eq!(doc.get("lang").unwrap(), "en");
    }

    #[test]
    fn title_stored_with_value_constraint() {
        let json = serde_json::json!({
            "document": {
                "id": "td",
                "header": {
                    "fileDesc": {
                        "titleStmt": { "title": "My Title" }
                    }
                }
            }
        });
        let schema = parse_tei(&json).expect("parse");
        // The title string vertex should carry a "value" constraint, not "n"
        let title_id = "td.header.fileDesc.titleStmt.title";
        let constraints: Vec<_> = vertex_constraints(&schema, title_id);
        assert!(
            constraints.iter().any(|c| c.sort == "value" && c.value == "My Title"),
            "title should use 'value' constraint, got: {constraints:?}"
        );
        assert!(
            !constraints.iter().any(|c| c.sort == "n"),
            "title should not use 'n' constraint"
        );
    }

    #[test]
    fn parse_verse_and_drama() {
        let json = serde_json::json!({
            "document": {
                "id": "vd",
                "text": {
                    "body": {
                        "divs": [{
                            "id": "div1",
                            "lineGroups": [{
                                "id": "lg1",
                                "type": "stanza",
                                "lines": [
                                    {"id": "l1", "n": "1", "tokens": [{"id": "w1", "n": "Sing"}]},
                                    {"id": "l2", "n": "2", "tokens": [{"id": "w2", "n": "O"}]}
                                ]
                            }],
                            "speeches": [{
                                "id": "sp1",
                                "who": "#hamlet",
                                "speaker": "Hamlet",
                                "paragraphs": [{"tokens": [{"id": "sw1", "n": "To"}]}]
                            }]
                        }]
                    }
                }
            }
        });

        let schema = parse_tei(&json).expect("parse verse/drama");
        assert!(schema.has_vertex("lg1"), "line group missing");
        assert!(schema.has_vertex("l1"), "verse line l1 missing");
        assert!(schema.has_vertex("l2"), "verse line l2 missing");
        assert!(schema.has_vertex("w1"), "verse token w1 missing");
        assert!(schema.has_vertex("sp1"), "speech sp1 missing");
        assert!(schema.has_vertex("sp1.speaker"), "speaker vertex missing");

        let emitted = emit_tei(&schema).expect("emit verse/drama");
        let body = emitted["document"]["text"]["body"].as_object().unwrap();
        let divs = body["divs"].as_array().unwrap();
        assert!(!divs.is_empty());
        let line_groups = divs[0]["lineGroups"].as_array().unwrap();
        assert_eq!(line_groups.len(), 1);
        let speeches = divs[0]["speeches"].as_array().unwrap();
        assert_eq!(speeches.len(), 1);
        assert_eq!(speeches[0]["who"], "#hamlet");
    }

    #[test]
    fn parse_critical_apparatus() {
        let json = serde_json::json!({
            "document": {
                "id": "cd",
                "text": {
                    "body": {
                        "divs": [{
                            "apps": [{
                                "id": "app1",
                                "lem": {"value": "original", "resp": "#ed"},
                                "readings": [
                                    {"wit": "#A", "value": "variant-a"},
                                    {"wit": "#B", "value": "variant-b"}
                                ]
                            }]
                        }]
                    }
                }
            }
        });

        let schema = parse_tei(&json).expect("parse apparatus");
        assert!(schema.has_vertex("app1"), "app vertex missing");
        assert!(schema.has_vertex("app1.lem"), "lem vertex missing");
        assert!(schema.has_vertex("app1.rdg.0"), "rdg.0 missing");
        assert!(schema.has_vertex("app1.rdg.1"), "rdg.1 missing");

        let lem_constraints: Vec<_> = vertex_constraints(&schema, "app1.lem");
        assert!(lem_constraints.iter().any(|c| c.sort == "value" && c.value == "original"));

        let emitted = emit_tei(&schema).expect("emit apparatus");
        let div = &emitted["document"]["text"]["body"]["divs"][0];
        let apps = div["apps"].as_array().unwrap();
        assert_eq!(apps.len(), 1);
        let readings = apps[0]["readings"].as_array().unwrap();
        assert_eq!(readings.len(), 2);
    }

    #[test]
    fn parse_character_tokens() {
        let json = serde_json::json!({
            "document": {
                "id": "ctd",
                "text": {
                    "body": {
                        "divs": [{
                            "paragraphs": [{
                                "tokens": [
                                    {"id": "c1", "kind": "c", "n": "H"},
                                    {"id": "c2", "kind": "c", "n": "i"}
                                ]
                            }]
                        }]
                    }
                }
            }
        });

        let schema = parse_tei(&json).expect("parse character tokens");
        assert!(schema.has_vertex("c1"), "character token c1 missing");
        assert!(schema.has_vertex("c2"), "character token c2 missing");
    }

    #[test]
    fn roundtrip() {
        let json = serde_json::json!({
            "document": {
                "id": "rt-doc",
                "header": {
                    "fileDesc": {
                        "titleStmt": { "title": "Roundtrip Test" }
                    }
                },
                "text": {
                    "body": {
                        "divs": [{
                            "type": "section",
                            "paragraphs": [{
                                "tokens": [{"n": "Hello", "pos": "INTJ"}]
                            }]
                        }]
                    }
                }
            }
        });

        let s1 = parse_tei(&json).expect("parse");
        let emitted = emit_tei(&s1).expect("emit");
        let s2 = parse_tei(&emitted).expect("re-parse");
        assert_eq!(s1.vertex_count(), s2.vertex_count());
    }

    #[test]
    fn parse_missing_document_errors() {
        let json = serde_json::json!({});
        assert!(parse_tei(&json).is_err());
    }
}
