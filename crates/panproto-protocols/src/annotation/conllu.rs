//! CoNLL-U (Universal Dependencies) protocol definition.
//!
//! CoNLL-U is a tab-separated tabular format for morphosyntactic annotation
//! used by the Universal Dependencies project. This protocol uses a
//! constrained hypergraph schema theory (`colimit(ThHypergraph, ThConstraint)`)
//! and a set-valued functor instance theory (`ThFunctor`).

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::emit::{children_by_edge, constraint_value};
use crate::error::ProtocolError;
use crate::theories;

/// Returns the CoNLL-U protocol definition.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "conllu".into(),
        schema_theory: "ThConlluSchema".into(),
        instance_theory: "ThConlluInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "sentence".into(),
            "word".into(),
            "multiword".into(),
            "empty".into(),
            "upos-tag".into(),
            "xpos-tag".into(),
            "deprel".into(),
            "feature".into(),
            "lemma".into(),
        ],
        constraint_sorts: vec![
            "form".into(),
            "id-range".into(),
            "head".into(),
            "misc".into(),
            "sent-id".into(),
            "text".into(),
            "newpar".into(),
            "newdoc".into(),
        ],
        has_order: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for CoNLL-U with a theory registry.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_hypergraph_functor(registry, "ThConlluSchema", "ThConlluInstance");
}

/// Parse a CoNLL-U formatted string into a [`Schema`].
///
/// The CoNLL-U format uses ten tab-separated columns per token line:
/// ID, FORM, LEMMA, UPOS, XPOS, FEATS, HEAD, DEPREL, DEPS, MISC.
/// Sentences are separated by blank lines; comment lines start with `#`.
///
/// Sentence-level comments `# sent_id`, `# text`, `# newpar`, and `# newdoc`
/// are captured as constraints on the sentence vertex.
///
/// The DEPS column (enhanced dependencies) is modeled with `enhanced-dep`
/// edges. Empty nodes (decimal IDs such as `1.1`) do not carry UPOS, XPOS,
/// LEMMA, or FEATS information per the CoNLL-U specification.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the input cannot be parsed.
#[allow(clippy::too_many_lines)]
pub fn parse_conllu(input: &str) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);
    let mut he_counter: usize = 0;

    let sentences = split_sentences(input);

    if sentences.is_empty() {
        return Err(ProtocolError::Parse("no sentences found".into()));
    }

    for (sent_counter, (comments, token_lines)) in sentences.iter().enumerate() {
        let sent_id = format!("sent_{sent_counter}");
        builder = builder
            .vertex(&sent_id, "sentence", None)
            .map_err(|e| ProtocolError::Parse(e.to_string()))?;

        // Capture sentence-level comments.
        // Comments are stored with the leading '#' stripped and trimmed.
        for comment in comments {
            let trimmed = comment.trim_start_matches('#').trim();
            if let Some(rest) = trimmed.strip_prefix("sent_id") {
                // rest is " = value" or "= value"
                let val = rest.trim().trim_start_matches('=').trim();
                builder = builder.constraint(&sent_id, "sent-id", val);
            } else if let Some(rest) = trimmed.strip_prefix("text") {
                let val = rest.trim().trim_start_matches('=').trim();
                builder = builder.constraint(&sent_id, "text", val);
            } else if trimmed == "newdoc" {
                builder = builder.constraint(&sent_id, "newdoc", "true");
            } else if let Some(rest) = trimmed.strip_prefix("newdoc id") {
                // "newdoc id = <value>"
                let val = rest.trim().trim_start_matches('=').trim();
                builder = builder.constraint(&sent_id, "newdoc", val);
            } else if trimmed == "newpar" {
                builder = builder.constraint(&sent_id, "newpar", "true");
            } else if let Some(rest) = trimmed.strip_prefix("newpar id") {
                // "newpar id = <value>"
                let val = rest.trim().trim_start_matches('=').trim();
                builder = builder.constraint(&sent_id, "newpar", val);
            }
        }

        // Collect token IDs for dependency resolution.
        let mut token_ids: HashMap<String, String> = HashMap::new();
        // Deferred basic dependency edges: (token_vertex_id, head_col, deprel_col).
        let mut deferred_deps: Vec<(String, String, String)> = Vec::new();
        // Deferred enhanced dependency edges: (token_vertex_id, deps_col).
        let mut deferred_enhanced: Vec<(String, String)> = Vec::new();

        for line in token_lines {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() != 10 {
                return Err(ProtocolError::Parse(format!(
                    "expected 10 columns, got {}: {line}",
                    cols.len()
                )));
            }

            let id_col = cols[0];
            let form = cols[1];
            let lemma_str = cols[2];
            let upos = cols[3];
            let xpos = cols[4];
            let feats = cols[5];
            let head = cols[6];
            let deprel_str = cols[7];
            let deps_col = cols[8];
            let misc = cols[9];

            let token_kind = classify_id(id_col);
            let token_vertex_id = format!("{sent_id}.tok_{id_col}");

            builder = builder
                .vertex(&token_vertex_id, &token_kind, None)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;

            // contains edge: sentence -> token
            builder = builder
                .edge(&sent_id, &token_vertex_id, "contains", Some(id_col))
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;

            // Form constraint
            builder = builder.constraint(&token_vertex_id, "form", form);

            // ID range constraint
            builder = builder.constraint(&token_vertex_id, "id-range", id_col);

            // Per CoNLL-U spec: empty nodes (decimal IDs) and multiword tokens
            // do not carry LEMMA, UPOS, XPOS, or FEATS; only words do.
            let is_word = token_kind == "word";

            if is_word {
                // Lemma vertex + edge
                if lemma_str != "_" {
                    let lemma_id = format!("{token_vertex_id}.lemma");
                    builder = builder
                        .vertex(&lemma_id, "lemma", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(&token_vertex_id, &lemma_id, "lemma-of", Some(lemma_str))
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                }

                // UPOS vertex + edge
                if upos != "_" {
                    let upos_id = format!("{token_vertex_id}.upos");
                    builder = builder
                        .vertex(&upos_id, "upos-tag", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(&token_vertex_id, &upos_id, "upos", Some(upos))
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                }

                // XPOS vertex + edge
                if xpos != "_" {
                    let xpos_id = format!("{token_vertex_id}.xpos");
                    builder = builder
                        .vertex(&xpos_id, "xpos-tag", None)
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    builder = builder
                        .edge(&token_vertex_id, &xpos_id, "xpos", Some(xpos))
                        .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                }

                // FEATS -> feature vertices
                if feats != "_" {
                    for (fi, feat_pair) in feats.split('|').enumerate() {
                        let feat_id = format!("{token_vertex_id}.feat_{fi}");
                        builder = builder
                            .vertex(&feat_id, "feature", None)
                            .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                        builder = builder
                            .edge(&token_vertex_id, &feat_id, "feat", Some(feat_pair))
                            .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    }
                }
            }

            // MISC constraint (applies to all token kinds)
            if misc != "_" {
                builder = builder.constraint(&token_vertex_id, "misc", misc);
            }

            // HEAD constraint + deferred basic dep (words and empty nodes only;
            // multiword tokens have _ for HEAD/DEPREL per spec).
            if head != "_" {
                builder = builder.constraint(&token_vertex_id, "head", head);
                deferred_deps.push((
                    token_vertex_id.clone(),
                    head.to_string(),
                    deprel_str.to_string(),
                ));
            }

            // DEPS column: enhanced dependencies
            if deps_col != "_" {
                deferred_enhanced.push((token_vertex_id.clone(), deps_col.to_string()));
            }

            token_ids.insert(id_col.to_string(), token_vertex_id);
        }

        // Resolve basic dependency edges.
        for (dep_vertex, head_col, deprel_col) in &deferred_deps {
            if head_col == "0" {
                // Root dependency: create a deprel vertex attached to sentence.
                let deprel_id = format!("{dep_vertex}.deprel");
                builder = builder
                    .vertex(&deprel_id, "deprel", None)
                    .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                builder = builder
                    .edge(dep_vertex, &deprel_id, "dep", Some(deprel_col))
                    .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            } else if let Some(head_vertex) = token_ids.get(head_col) {
                // dep edge from head to dependent
                builder = builder
                    .edge(head_vertex, dep_vertex, "dep", Some(deprel_col))
                    .map_err(|e| ProtocolError::Parse(e.to_string()))?;
            }
        }

        // Resolve enhanced dependency edges.
        // Format: "head:relation|head:relation" where head is a token ID.
        for (dep_vertex, deps_col) in &deferred_enhanced {
            for (ei, pair) in deps_col.split('|').enumerate() {
                // Split on the first colon: head_id:relation
                if let Some(colon_pos) = pair.find(':') {
                    let head_id = &pair[..colon_pos];
                    let relation = &pair[colon_pos + 1..];
                    let label = pair.to_string();
                    if head_id == "0" {
                        // Enhanced root
                        let edep_id = format!("{dep_vertex}.edep_{ei}");
                        builder = builder
                            .vertex(&edep_id, "deprel", None)
                            .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                        builder = builder
                            .edge(dep_vertex, &edep_id, "enhanced-dep", Some(relation))
                            .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    } else if let Some(head_vertex) = token_ids.get(head_id) {
                        builder = builder
                            .edge(head_vertex, dep_vertex, "enhanced-dep", Some(&label))
                            .map_err(|e| ProtocolError::Parse(e.to_string()))?;
                    }
                }
            }
        }

        // Hyper-edge connecting sentence to its tokens.
        if !token_ids.is_empty() {
            let he_id = format!("he_{he_counter}");
            he_counter += 1;
            let sig: HashMap<String, String> = token_ids
                .iter()
                .map(|(label, vid)| (label.clone(), vid.clone()))
                .collect();
            builder = builder
                .hyper_edge(&he_id, "sentence", sig, &sent_id)
                .map_err(|e| ProtocolError::Parse(e.to_string()))?;
        }
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// Emit a [`Schema`] back to CoNLL-U text format.
///
/// Sentence-level comments (`# sent_id`, `# text`, `# newpar`, `# newdoc`)
/// are emitted before each sentence block when present. Enhanced dependencies
/// from `enhanced-dep` edges are serialized into the DEPS column.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
#[allow(clippy::too_many_lines)]
pub fn emit_conllu(schema: &Schema) -> Result<String, ProtocolError> {
    let mut output = String::new();

    // Find sentence vertices.
    let mut sentences: Vec<_> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "sentence")
        .collect();
    sentences.sort_by(|a, b| a.id.cmp(&b.id));

    for sentence in &sentences {
        // Emit sentence-level comments.
        if let Some(newdoc) = constraint_value(schema, &sentence.id, "newdoc") {
            if newdoc == "true" {
                output.push_str("# newdoc\n");
            } else {
                output.push_str("# newdoc id = ");
                output.push_str(newdoc);
                output.push('\n');
            }
        }
        if let Some(newpar) = constraint_value(schema, &sentence.id, "newpar") {
            if newpar == "true" {
                output.push_str("# newpar\n");
            } else {
                output.push_str("# newpar id = ");
                output.push_str(newpar);
                output.push('\n');
            }
        }
        if let Some(sid) = constraint_value(schema, &sentence.id, "sent-id") {
            output.push_str("# sent_id = ");
            output.push_str(sid);
            output.push('\n');
        }
        if let Some(text) = constraint_value(schema, &sentence.id, "text") {
            output.push_str("# text = ");
            output.push_str(text);
            output.push('\n');
        }

        // Get tokens via contains edges.
        let tokens = children_by_edge(schema, &sentence.id, "contains");

        // Collect enhanced dep lookup: token_vertex_id -> Vec<"head_id:relation">
        // Built from enhanced-dep edges (incoming to each token).
        let mut enhanced_map: HashMap<String, Vec<String>> = HashMap::new();
        for (_edge, token_vertex) in &tokens {
            let incoming = schema.incoming_edges(&token_vertex.id);
            let mut edeps: Vec<String> = incoming
                .iter()
                .filter(|e| e.kind == "enhanced-dep")
                .filter_map(|e| {
                    // Recover head_id from the source vertex's id-range constraint.
                    let head_id = constraint_value(schema, &e.src, "id-range").unwrap_or("0");
                    e.name.as_deref().map(|label| {
                        // label is already "head_id:relation" if stored that way,
                        // but we reconstruct from head_id + relation (the label).
                        // For root enhanced deps the name is just the relation.
                        if label.contains(':') {
                            label.to_string()
                        } else {
                            format!("{head_id}:{label}")
                        }
                    })
                })
                .collect();
            // Also check outgoing enhanced-dep from this node to a deprel vertex (root).
            let outgoing = schema.outgoing_edges(&token_vertex.id);
            for e in outgoing.iter().filter(|e| e.kind == "enhanced-dep") {
                if let Some(tgt_v) = schema.vertices.get(&e.tgt) {
                    if tgt_v.kind == "deprel" {
                        if let Some(rel) = e.name.as_deref() {
                            edeps.push(format!("0:{rel}"));
                        }
                    }
                }
            }
            if !edeps.is_empty() {
                // Sort by head index numerically.
                edeps.sort_by(|a, b| {
                    let a_head = a.split(':').next().unwrap_or("0");
                    let b_head = b.split(':').next().unwrap_or("0");
                    let a_n = a_head.parse::<f64>().unwrap_or(0.0);
                    let b_n = b_head.parse::<f64>().unwrap_or(0.0);
                    a_n.partial_cmp(&b_n).unwrap_or(std::cmp::Ordering::Equal)
                });
                enhanced_map.insert(token_vertex.id.to_string(), edeps);
            }
        }

        // Collect token data and sort by ID column.
        let mut token_lines: Vec<(String, String)> = Vec::new();

        for (_edge, token_vertex) in &tokens {
            let id_col = constraint_value(schema, &token_vertex.id, "id-range").unwrap_or("_");
            let form = constraint_value(schema, &token_vertex.id, "form").unwrap_or("_");

            let is_word = token_vertex.kind == "word";

            // Lemma from lemma-of edge name (words only).
            let lemma = if is_word {
                let lemma_edges = children_by_edge(schema, &token_vertex.id, "lemma-of");
                lemma_edges
                    .first()
                    .and_then(|(e, _)| e.name.as_deref())
                    .unwrap_or("_")
            } else {
                "_"
            };

            // UPOS from upos edge name (words only).
            let upos = if is_word {
                let upos_edges = children_by_edge(schema, &token_vertex.id, "upos");
                upos_edges
                    .first()
                    .and_then(|(e, _)| e.name.as_deref())
                    .unwrap_or("_")
            } else {
                "_"
            };

            // XPOS from xpos edge name (words only).
            let xpos = if is_word {
                let xpos_edges = children_by_edge(schema, &token_vertex.id, "xpos");
                xpos_edges
                    .first()
                    .and_then(|(e, _)| e.name.as_deref())
                    .unwrap_or("_")
            } else {
                "_"
            };

            // FEATS from feat edge names (words only).
            let feats = if is_word {
                let feat_edges = children_by_edge(schema, &token_vertex.id, "feat");
                if feat_edges.is_empty() {
                    "_".to_string()
                } else {
                    feat_edges
                        .iter()
                        .filter_map(|(e, _)| e.name.as_deref())
                        .collect::<Vec<_>>()
                        .join("|")
                }
            } else {
                "_".to_string()
            };

            // HEAD from head constraint.
            let head = constraint_value(schema, &token_vertex.id, "head").unwrap_or("_");

            // DEPREL: recover from basic dep edge going either out (root) or
            // being the edge name on incoming dep edges whose src has a dep edge
            // to this vertex. For basic deps, the head emits the edge to the
            // dependent, labelled with deprel. We find the incoming dep edge.
            let deprel = {
                let incoming_dep = schema
                    .incoming_edges(&token_vertex.id)
                    .iter()
                    .find(|e| e.kind == "dep")
                    .and_then(|e| e.name.as_deref());
                // Also check outgoing dep to a deprel vertex (root case).
                let root_dep = schema
                    .outgoing_edges(&token_vertex.id)
                    .iter()
                    .find(|e| e.kind == "dep")
                    .and_then(|e| e.name.as_deref());
                incoming_dep.or(root_dep).unwrap_or("_")
            };

            // DEPS: enhanced dependencies.
            let deps: String = enhanced_map
                .get(token_vertex.id.as_str())
                .map_or_else(|| "_".to_string(), |v| v.join("|"));

            // MISC from misc constraint.
            let misc = constraint_value(schema, &token_vertex.id, "misc").unwrap_or("_");

            let line = format!(
                "{id_col}\t{form}\t{lemma}\t{upos}\t{xpos}\t{feats}\t{head}\t{deprel}\t{deps}\t{misc}"
            );

            token_lines.push((id_col.to_string(), line));
        }

        // Sort token lines by CoNLL-U ID order.
        token_lines.sort_by(|a, b| cmp_conllu_id(&a.0, &b.0));

        for (_, line) in &token_lines {
            output.push_str(line);
            output.push('\n');
        }

        // Blank line between sentences.
        output.push('\n');
    }

    Ok(output)
}

/// Classify a CoNLL-U ID field into a token kind.
fn classify_id(id: &str) -> String {
    if id.contains('-') {
        "multiword".into()
    } else if id.contains('.') {
        "empty".into()
    } else {
        "word".into()
    }
}

/// Split input text into sentence blocks.
///
/// Returns `Vec<(comments, token_lines)>` where `comments` are the raw comment
/// strings (with leading `#`) belonging to each sentence, and `token_lines`
/// are the data lines.
fn split_sentences(input: &str) -> Vec<(Vec<String>, Vec<String>)> {
    let mut sentences = Vec::new();
    let mut current_comments: Vec<String> = Vec::new();
    let mut current_lines: Vec<String> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_lines.is_empty() {
                sentences.push((
                    std::mem::take(&mut current_comments),
                    std::mem::take(&mut current_lines),
                ));
            }
        } else if trimmed.starts_with('#') {
            // If we already have token lines, this comment belongs to the next
            // sentence, so flush the current one first.
            if !current_lines.is_empty() {
                sentences.push((
                    std::mem::take(&mut current_comments),
                    std::mem::take(&mut current_lines),
                ));
            }
            current_comments.push(trimmed.to_string());
        } else {
            current_lines.push(trimmed.to_string());
        }
    }

    if !current_lines.is_empty() {
        sentences.push((current_comments, current_lines));
    }

    sentences
}

/// Compare two CoNLL-U ID strings for ordering.
///
/// Numeric IDs sort numerically. Multiword ranges (e.g. "2-3") sort by
/// their start index. Empty word IDs (e.g. "1.1") sort after their integer part.
fn cmp_conllu_id(a: &str, b: &str) -> std::cmp::Ordering {
    let a_key = conllu_id_sort_key(a);
    let b_key = conllu_id_sort_key(b);
    a_key.cmp(&b_key)
}

/// Produce a sort key (`major`, `minor`, `is_range`) from a CoNLL-U ID.
fn conllu_id_sort_key(id: &str) -> (u32, u32, u8) {
    if let Some((start, _end)) = id.split_once('-') {
        // Multiword token: sort before the first word in the range.
        let major = start.parse::<u32>().unwrap_or(0);
        (major, 0, 0)
    } else if let Some((int_part, dec_part)) = id.split_once('.') {
        // Empty word: sort after its integer part.
        let major = int_part.parse::<u32>().unwrap_or(0);
        let minor = dec_part.parse::<u32>().unwrap_or(0);
        (major, minor, 2)
    } else {
        let major = id.parse::<u32>().unwrap_or(0);
        (major, 0, 1)
    }
}

fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "contains".into(),
            src_kinds: vec!["sentence".into()],
            tgt_kinds: vec!["word".into(), "multiword".into(), "empty".into()],
        },
        EdgeRule {
            edge_kind: "dep".into(),
            src_kinds: vec!["word".into(), "empty".into()],
            tgt_kinds: vec!["word".into(), "empty".into(), "deprel".into()],
        },
        EdgeRule {
            edge_kind: "enhanced-dep".into(),
            src_kinds: vec!["word".into(), "empty".into()],
            tgt_kinds: vec!["word".into(), "empty".into(), "deprel".into()],
        },
        EdgeRule {
            edge_kind: "feat".into(),
            src_kinds: vec!["word".into()],
            tgt_kinds: vec!["feature".into()],
        },
        EdgeRule {
            edge_kind: "upos".into(),
            src_kinds: vec!["word".into()],
            tgt_kinds: vec!["upos-tag".into()],
        },
        EdgeRule {
            edge_kind: "xpos".into(),
            src_kinds: vec!["word".into()],
            tgt_kinds: vec!["xpos-tag".into()],
        },
        EdgeRule {
            edge_kind: "lemma-of".into(),
            src_kinds: vec!["word".into()],
            tgt_kinds: vec!["lemma".into()],
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
        assert_eq!(p.name, "conllu");
        assert_eq!(p.schema_theory, "ThConlluSchema");
        assert_eq!(p.instance_theory, "ThConlluInstance");
        assert!(p.find_edge_rule("contains").is_some());
        assert!(p.find_edge_rule("dep").is_some());
        assert!(p.find_edge_rule("enhanced-dep").is_some());
        assert!(p.find_edge_rule("feat").is_some());
        assert!(p.find_edge_rule("upos").is_some());
        assert!(p.find_edge_rule("xpos").is_some());
        assert!(p.find_edge_rule("lemma-of").is_some());
        // `token` vertex kind removed; must not appear.
        assert!(!p.obj_kinds.contains(&"token".to_string()));
        // `deprel` not in constraint_sorts (it is an obj_kind).
        assert!(!p.constraint_sorts.contains(&"deprel".to_string()));
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThConlluSchema"));
        assert!(registry.contains_key("ThConlluInstance"));
    }

    #[test]
    fn parse_and_emit_roundtrip() {
        let conllu_text = "\
# sent_id = test-01
# text = The cat sat.
1\tThe\tthe\tDET\tDT\tDefinite=Def|PronType=Art\t2\tdet\t_\t_
2\tcat\tcat\tNOUN\tNN\tNumber=Sing\t3\tnsubj\t_\t_
3\tsat\tsit\tVERB\tVBD\tMood=Ind|Tense=Past|VerbForm=Fin\t0\troot\t_\tSpaceAfter=No
";

        let schema = parse_conllu(conllu_text).expect("should parse");

        // Verify sentence vertex exists.
        assert!(schema.has_vertex("sent_0"));
        assert_eq!(schema.vertices.get("sent_0").unwrap().kind, "sentence");

        // Verify sent_id comment was captured.
        assert_eq!(
            constraint_value(&schema, "sent_0", "sent-id"),
            Some("test-01")
        );
        assert_eq!(
            constraint_value(&schema, "sent_0", "text"),
            Some("The cat sat.")
        );

        // Verify token vertices.
        assert!(schema.has_vertex("sent_0.tok_1"));
        assert!(schema.has_vertex("sent_0.tok_2"));
        assert!(schema.has_vertex("sent_0.tok_3"));
        assert_eq!(schema.vertices.get("sent_0.tok_1").unwrap().kind, "word");

        // Verify constraints.
        assert_eq!(
            constraint_value(&schema, "sent_0.tok_1", "form"),
            Some("The")
        );
        assert_eq!(
            constraint_value(&schema, "sent_0.tok_3", "misc"),
            Some("SpaceAfter=No")
        );

        // Verify UPOS vertex.
        assert!(schema.has_vertex("sent_0.tok_1.upos"));
        assert_eq!(
            schema.vertices.get("sent_0.tok_1.upos").unwrap().kind,
            "upos-tag"
        );

        // Verify lemma vertex.
        assert!(schema.has_vertex("sent_0.tok_1.lemma"));
        assert_eq!(
            schema.vertices.get("sent_0.tok_1.lemma").unwrap().kind,
            "lemma"
        );

        // Roundtrip: emit and re-parse.
        let emitted = emit_conllu(&schema).expect("should emit");
        let schema2 = parse_conllu(&emitted).expect("should re-parse");
        assert_eq!(schema.vertex_count(), schema2.vertex_count());
    }

    #[test]
    fn parse_empty_input_fails() {
        let result = parse_conllu("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_multiword_token() {
        let conllu_text = "\
1\tI\tI\tPRON\tPRP\t_\t2\tnsubj\t_\t_
2-3\tdon't\t_\t_\t_\t_\t_\t_\t_\t_
2\tdo\tdo\tAUX\tVBP\t_\t0\troot\t_\t_
3\tnot\tnot\tPART\tRB\t_\t2\tadvmod\t_\t_
";

        let schema = parse_conllu(conllu_text).expect("should parse multiword");
        assert!(schema.has_vertex("sent_0.tok_2-3"));
        assert_eq!(
            schema.vertices.get("sent_0.tok_2-3").unwrap().kind,
            "multiword"
        );
    }

    #[test]
    fn empty_nodes_no_upos_xpos_lemma() {
        // Empty nodes (decimal IDs) must NOT produce upos/xpos/lemma vertices.
        let conllu_text = "\
1\tThey\tthey\tPRON\tPRP\t_\t2\tnsubj\t_\t_
1.1\tare\tbe\tAUX\tVBZ\t_\t_\t_\t2:cop\t_
2\thappy\thappy\tADJ\tJJ\t_\t0\troot\t_\t_
";

        let schema = parse_conllu(conllu_text).expect("should parse empty node");
        assert!(schema.has_vertex("sent_0.tok_1.1"));
        assert_eq!(schema.vertices.get("sent_0.tok_1.1").unwrap().kind, "empty");
        // Must NOT have upos/xpos/lemma for the empty node.
        assert!(!schema.has_vertex("sent_0.tok_1.1.upos"));
        assert!(!schema.has_vertex("sent_0.tok_1.1.xpos"));
        assert!(!schema.has_vertex("sent_0.tok_1.1.lemma"));
    }

    #[test]
    fn enhanced_deps_parsed() {
        // DEPS column with enhanced dependency "2:cop" should create an
        // enhanced-dep edge from tok_2 to tok_1.1.
        let conllu_text = "\
1\tThey\tthey\tPRON\tPRP\t_\t2\tnsubj\t_\t_
1.1\tare\tbe\tAUX\tVBZ\t_\t_\t_\t2:cop\t_
2\thappy\thappy\tADJ\tJJ\t_\t0\troot\t_\t_
";

        let schema = parse_conllu(conllu_text).expect("should parse enhanced deps");
        // enhanced-dep edge should exist from tok_2 -> tok_1.1.
        let enhanced = schema.edges_between("sent_0.tok_2", "sent_0.tok_1.1");
        assert!(
            enhanced.iter().any(|e| e.kind == "enhanced-dep"),
            "expected enhanced-dep edge from tok_2 to tok_1.1"
        );
    }

    #[test]
    fn newpar_newdoc_comments() {
        let conllu_text = "\
# newdoc
# newpar id = par-1
# sent_id = s1
1\tHello\thello\tINTJ\tUH\t_\t0\troot\t_\t_
";
        let schema = parse_conllu(conllu_text).expect("should parse");
        assert_eq!(constraint_value(&schema, "sent_0", "newdoc"), Some("true"));
        assert_eq!(constraint_value(&schema, "sent_0", "newpar"), Some("par-1"));
        assert_eq!(constraint_value(&schema, "sent_0", "sent-id"), Some("s1"));
    }

    #[test]
    fn deprel_not_in_constraint_sorts() {
        // deprel must be an obj_kind only, not a constraint sort.
        let p = protocol();
        assert!(p.obj_kinds.contains(&"deprel".to_string()));
        assert!(!p.constraint_sorts.contains(&"deprel".to_string()));
    }

    #[test]
    fn upos_xpos_feat_lemma_rules_word_only() {
        // Edge rules for upos/xpos/feat/lemma-of must only allow "word" sources.
        let p = protocol();
        for kind in &["upos", "xpos", "feat", "lemma-of"] {
            let rule = p
                .find_edge_rule(kind)
                .unwrap_or_else(|| panic!("no rule for {kind}"));
            assert_eq!(
                rule.src_kinds,
                vec!["word".to_string()],
                "edge rule '{kind}' should only allow 'word' sources"
            );
        }
    }
}
