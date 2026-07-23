//! `ATProto` protocol definition.
//!
//! The AT Protocol uses a constrained multigraph schema theory
//! (colimit of `ThGraph`, `ThConstraint`, `ThMulti`) and a W-type
//! instance theory with metadata (`ThWType + ThMeta`).
//!
//! Vertex kinds: record, object, array, union, string, integer, boolean,
//! bytes, cid-link, blob, unknown, token.
//!
//! Edge kinds: record-schema, prop, items, variant, ref, self-ref.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use panproto_gat::{Name, Sort, Theory, pushout_by_name};
use panproto_schema::{Edge, EdgeRule, Protocol, Schema, SchemaBuilder};
use smallvec::SmallVec;

use crate::error::ProtocolError;
use crate::theories;

/// Returns the `ATProto` protocol definition.
///
/// Schema theory: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance theory: `ThWType + ThMeta`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "atproto".into(),
        schema_theory: "ThATProtoSchema".into(),
        instance_theory: "ThATProtoInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "record".into(),
            "object".into(),
            "array".into(),
            "union".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "bytes".into(),
            "cid-link".into(),
            "blob".into(),
            "unknown".into(),
            "token".into(),
            "query".into(),
            "procedure".into(),
            "subscription".into(),
            "ref".into(),
        ],
        constraint_sorts: vec![
            "minLength".into(),
            "maxLength".into(),
            "minimum".into(),
            "maximum".into(),
            "maxGraphemes".into(),
            "enum".into(),
            "const".into(),
            "default".into(),
            "closed".into(),
            "format".into(),
            "knownValues".into(),
            // Provenance of a `ref` property: the literal lexicon ref
            // target string, recorded alongside the structural ref edge
            // by `parse_object_def` / `parse_array_def`.
            "ref".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for `ATProto` with a theory registry.
///
/// Registers `ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta`,
/// and the composed schema/instance theories.
pub fn register_theories<S: ::std::hash::BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let th_multi = theories::th_multi();
    let th_wtype = theories::th_wtype();
    let th_meta = theories::th_meta();

    registry.insert("ThGraph".into(), th_graph.clone());
    registry.insert("ThConstraint".into(), th_constraint.clone());
    registry.insert("ThMulti".into(), th_multi.clone());
    registry.insert("ThWType".into(), th_wtype.clone());
    registry.insert("ThMeta".into(), th_meta.clone());

    // Compose schema theory via colimit.
    // Step 1: colimit(ThGraph, ThConstraint) over shared Vertex.
    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    if let Ok(gc) = pushout_by_name(&th_graph, &th_constraint, &shared_vertex).map(|r| r.theory) {
        // Step 2: colimit(gc, ThMulti) over shared {Vertex, Edge}.
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(mut schema_theory) = pushout_by_name(&gc, &th_multi, &shared_ve).map(|r| r.theory)
        {
            schema_theory.name = "ThATProtoSchema".into();
            registry.insert("ThATProtoSchema".into(), schema_theory);
        }
    }

    // Compose instance theory: colimit(ThWType, ThMeta) over shared Node.
    let shared_node = Theory::new("ThNode", vec![Sort::simple("Node")], vec![], vec![]);
    if let Ok(mut inst_theory) =
        pushout_by_name(&th_wtype, &th_meta, &shared_node).map(|r| r.theory)
    {
        inst_theory.name = "ThATProtoInstance".into();
        registry.insert("ThATProtoInstance".into(), inst_theory);
    }
}

/// Parse an `ATProto` lexicon JSON document into a [`Schema`].
///
/// Walks the `defs` object, creating vertices for each type definition
/// and edges for structural relationships (properties, array items,
/// union variants, references).
///
/// A `$ref` to a def in another lexicon document resolves to an opaque
/// `"ref"`-kind placeholder vertex, because this entry point sees only
/// the one document. To resolve refs across a set of documents, use
/// [`parse_lexicon_bundle`].
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is not a valid lexicon or
/// if schema construction fails.
pub fn parse_lexicon(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    parse_lexicon_bundle(std::slice::from_ref(json))
}

/// Parse a bundle of `ATProto` lexicon documents into one [`Schema`],
/// resolving `$ref`s across the whole bundle.
///
/// Every document's defs are registered as vertices before any
/// document's structure is parsed, so a `nsid#frag` ref into a *sibling*
/// document lands on that def's real, typed vertex instead of on an
/// opaque `"ref"` placeholder. A ref whose target is in no document of
/// the bundle still becomes a placeholder, which is what marks it as
/// genuinely external.
///
/// Passing a single document is equivalent to [`parse_lexicon`].
///
/// # Errors
///
/// Returns [`ProtocolError::MissingField`] if any document lacks `id`
/// or `defs`, [`ProtocolError::Parse`] if two documents declare the
/// same `id`, or a construction error from the schema builder.
pub fn parse_lexicon_bundle(docs: &[serde_json::Value]) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Destructure each document once, up front, so a malformed
    // document in the bundle fails before any vertex is registered.
    let mut parsed: Vec<(&str, &serde_json::Map<String, serde_json::Value>)> =
        Vec::with_capacity(docs.len());
    let mut seen_ids: HashSet<&str> = HashSet::with_capacity(docs.len());

    for json in docs {
        let lexicon_id = json
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ProtocolError::MissingField("id".into()))?;

        let defs = json
            .get("defs")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| ProtocolError::MissingField("defs".into()))?;

        if !seen_ids.insert(lexicon_id) {
            return Err(ProtocolError::Parse(format!(
                "duplicate lexicon id in bundle: {lexicon_id}"
            )));
        }

        parsed.push((lexicon_id, defs));
    }

    // First pass, over the whole bundle: create a vertex for every
    // top-level def of every document. This provides stable targets for
    // forward and cross-document `ref` edges, so a ref never creates a
    // placeholder that collides with the real def on a later iteration.
    for (lexicon_id, defs) in &parsed {
        builder = register_def_vertices(builder, lexicon_id, defs)?;
    }

    // Second pass: parse each document's type-specific structure. Every
    // in-bundle ref target now exists as a typed vertex.
    for (lexicon_id, defs) in &parsed {
        builder = parse_def_bodies(builder, lexicon_id, defs)?;
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// One `ATProto` lexicon document tagged with the project-relative path
/// it lives at.
pub struct LexiconDoc {
    /// Project-relative path of the lexicon file (e.g.
    /// `annotation/annotationLayer.json`).
    pub path: PathBuf,
    /// The lexicon document.
    pub value: serde_json::Value,
}

/// A lexicon set parsed with per-file provenance: each document's own
/// schema plus the ref edges that cross document boundaries.
///
/// Where [`parse_lexicon_bundle`] fuses every document into one flat
/// schema (correct, but with no per-file identity, so the version-control
/// layer cannot store or diff it as the per-file tree it is built
/// around), this keeps each document a separate schema and records
/// cross-document refs as `<path>::<name>`-prefixed edges that project
/// assembly adds verbatim. Feed `files` and `cross_file_edges` to
/// `panproto_project::build_project_tree` to store a lexicon set as a
/// per-file tree the VCS can diff incrementally.
pub struct LexiconProject {
    /// Each document's own schema (its owned defs as typed vertices),
    /// keyed by the document's path.
    pub files: Vec<(PathBuf, Schema)>,
    /// Cross-file ref edges, keyed by the owning (source) file. Both
    /// endpoints are already prefixed with their owning file's path, so
    /// project assembly adds them without re-prefixing.
    pub cross_file_edges: HashMap<PathBuf, Vec<Edge>>,
}

/// A vertex `vertex_id` is owned by the lexicon whose id is `doc_id`
/// when it is that lexicon's `main` record (`vertex_id == doc_id`) or a
/// sub-vertex under it (`doc_id` followed by a `#`, `:`, or `.`
/// separator). Callers try the longest matching `doc_id` first so a
/// document whose id is a prefix of another's does not steal its
/// vertices.
fn is_owned_by(vertex_id: &str, doc_id: &str) -> bool {
    vertex_id == doc_id
        || vertex_id
            .strip_prefix(doc_id)
            .is_some_and(|rest| rest.starts_with(['#', ':', '.']))
}

fn prefix_name(path: &std::path::Path, name: &str) -> Name {
    Name::from(format!("{}::{}", path.display(), name).as_str())
}

/// Build a per-file schema holding only the vertices in `owned` and the
/// edges in `internal`, recomputing the adjacency indices from the
/// retained edges.
fn retain_file_schema(m: &Schema, owned: &HashSet<Name>, internal: &HashSet<Edge>) -> Schema {
    fn by_vertex<V: Clone>(map: &HashMap<Name, V>, owned: &HashSet<Name>) -> HashMap<Name, V> {
        map.iter()
            .filter(|(k, _)| owned.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    let edges: HashMap<Edge, Name> = m
        .edges
        .iter()
        .filter(|(e, _)| internal.contains(*e))
        .map(|(e, k)| (e.clone(), k.clone()))
        .collect();

    // Recompute adjacency indices from the retained edges.
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    for edge in edges.keys() {
        outgoing
            .entry(edge.src.clone())
            .or_default()
            .push(edge.clone());
        incoming
            .entry(edge.tgt.clone())
            .or_default()
            .push(edge.clone());
        between
            .entry((edge.src.clone(), edge.tgt.clone()))
            .or_default()
            .push(edge.clone());
    }

    Schema {
        protocol: m.protocol.clone(),
        vertices: by_vertex(&m.vertices, owned),
        edges,
        hyper_edges: m.hyper_edges.clone(),
        constraints: by_vertex(&m.constraints, owned),
        required: by_vertex(&m.required, owned),
        nsids: by_vertex(&m.nsids, owned),
        entries: m
            .entries
            .iter()
            .filter(|e| owned.contains(*e))
            .cloned()
            .collect(),
        variants: by_vertex(&m.variants, owned),
        orderings: m
            .orderings
            .iter()
            .filter(|(e, _)| internal.contains(*e))
            .map(|(e, p)| (e.clone(), *p))
            .collect(),
        recursion_points: by_vertex(&m.recursion_points, owned),
        spans: m.spans.clone(),
        usage_modes: m
            .usage_modes
            .iter()
            .filter(|(e, _)| internal.contains(*e))
            .map(|(e, u)| (e.clone(), u.clone()))
            .collect(),
        nominal: by_vertex(&m.nominal, owned),
        coercions: m.coercions.clone(),
        mergers: by_vertex(&m.mergers, owned),
        defaults: by_vertex(&m.defaults, owned),
        policies: m.policies.clone(),
        outgoing,
        incoming,
        between,
    }
}

/// Parse a set of `ATProto` lexicon documents into per-file schemas with
/// cross-document refs resolved, retaining the per-file provenance the
/// version-control layer needs.
///
/// The whole set is first parsed as a bundle (so every in-set `$ref`
/// resolves to the referenced def's real, typed vertex), then the
/// resulting flat schema is partitioned back by NSID ownership: each
/// vertex and each same-file edge returns to the document that declared
/// it, while an edge whose endpoints live in different documents becomes
/// a cross-file edge with both endpoints prefixed by their owning file.
/// A ref whose target is in no document of the set stays an opaque
/// placeholder in the referencing file, exactly as [`parse_lexicon`]
/// leaves it.
///
/// # Errors
///
/// Returns the same errors as [`parse_lexicon_bundle`]: a document
/// missing `id` or `defs`, duplicate ids, or a schema-builder error.
pub fn parse_lexicon_project(docs: &[LexiconDoc]) -> Result<LexiconProject, ProtocolError> {
    let values: Vec<serde_json::Value> = docs.iter().map(|d| d.value.clone()).collect();
    let monolith = parse_lexicon_bundle(&values)?;

    // Document id -> path, longest id first so the longest-prefix match
    // wins when one lexicon id is a prefix of another.
    let mut doc_paths: Vec<(String, PathBuf)> = Vec::with_capacity(docs.len());
    for d in docs {
        let id = d
            .value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ProtocolError::MissingField("id".into()))?;
        doc_paths.push((id.to_string(), d.path.clone()));
    }
    doc_paths.sort_by_key(|(id, _)| std::cmp::Reverse(id.len()));

    // Assign every vertex to the document that owns it.
    let mut owner: HashMap<Name, PathBuf> = HashMap::new();
    for vid in monolith.vertices.keys() {
        if let Some((_, path)) = doc_paths.iter().find(|(id, _)| is_owned_by(vid, id)) {
            owner.insert(vid.clone(), path.clone());
        }
    }
    // An out-of-set ref target is an opaque placeholder owned by no
    // document; keep it in the file that references it so it stays a
    // (genuinely external) placeholder there rather than vanishing.
    for edge in monolith.edges.keys() {
        if !owner.contains_key(&edge.tgt) {
            if let Some(src_path) = owner.get(&edge.src).cloned() {
                owner.insert(edge.tgt.clone(), src_path);
            }
        }
    }

    // Partition edges into per-file internal sets and cross-file edges.
    let mut internal: HashMap<PathBuf, HashSet<Edge>> = HashMap::new();
    let mut cross_file_edges: HashMap<PathBuf, Vec<Edge>> = HashMap::new();
    for edge in monolith.edges.keys() {
        let (Some(src_path), Some(tgt_path)) = (owner.get(&edge.src), owner.get(&edge.tgt)) else {
            continue;
        };
        if src_path == tgt_path {
            internal
                .entry(src_path.clone())
                .or_default()
                .insert(edge.clone());
        } else {
            let prefixed = Edge {
                src: prefix_name(src_path, &edge.src),
                tgt: prefix_name(tgt_path, &edge.tgt),
                kind: edge.kind.clone(),
                name: edge.name.as_ref().map(|n| prefix_name(src_path, n)),
            };
            cross_file_edges
                .entry(src_path.clone())
                .or_default()
                .push(prefixed);
        }
    }

    // Build each document's per-file schema from the vertices it owns and
    // its internal edges, in input order for determinism.
    let mut files: Vec<(PathBuf, Schema)> = Vec::with_capacity(docs.len());
    for d in docs {
        let owned: HashSet<Name> = owner
            .iter()
            .filter(|(_, p)| **p == d.path)
            .map(|(v, _)| v.clone())
            .collect();
        let file_internal = internal.remove(&d.path).unwrap_or_default();
        let schema = retain_file_schema(&monolith, &owned, &file_internal);
        files.push((d.path.clone(), schema));
    }

    Ok(LexiconProject {
        files,
        cross_file_edges,
    })
}

/// Register a vertex for every top-level def in one document.
fn register_def_vertices(
    mut builder: SchemaBuilder,
    lexicon_id: &str,
    defs: &serde_json::Map<String, serde_json::Value>,
) -> Result<SchemaBuilder, ProtocolError> {
    for (def_name, def_value) in defs {
        let def_type = def_value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object");

        let vertex_id = if def_name == "main" {
            lexicon_id.to_string()
        } else {
            format!("{lexicon_id}#{def_name}")
        };

        let kind = lexicon_type_to_kind(def_type);
        let nsid = if def_name == "main" {
            Some(lexicon_id)
        } else {
            None
        };

        builder = builder.vertex(&vertex_id, &kind, nsid)?;
    }

    Ok(builder)
}

/// Parse the type-specific structure of every def in one document and
/// declare its entry sorts.
fn parse_def_bodies(
    mut builder: SchemaBuilder,
    lexicon_id: &str,
    defs: &serde_json::Map<String, serde_json::Value>,
) -> Result<SchemaBuilder, ProtocolError> {
    for (def_name, def_value) in defs {
        let def_type = def_value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object");

        let vertex_id = if def_name == "main" {
            lexicon_id.to_string()
        } else {
            format!("{lexicon_id}#{def_name}")
        };

        // Declare the basepoint. In ATProto, every top-level record,
        // query, procedure, or subscription is an intended entry sort:
        // it is a valid root for instances of this lexicon. Sub-defs
        // (`type: "object"`, `"array"`, `"union"`, etc.) are *not*
        // entries — they are referenced from entries via `ref` edges.
        match def_type {
            "record" | "query" | "procedure" | "subscription" => {
                builder = builder.entry(&vertex_id);
            }
            _ => {}
        }

        // Parse type-specific structure.
        match def_type {
            "record" => {
                builder = parse_record_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "object" => {
                builder = parse_object_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "string" | "integer" | "boolean" | "bytes" | "cid-link" | "blob" | "unknown"
            | "token" => {
                builder = parse_constraints(builder, &vertex_id, def_value);
            }
            "array" => {
                builder = parse_array_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "union" => {
                builder = parse_union_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            "query" | "procedure" | "subscription" => {
                builder = parse_query_procedure_def(builder, &vertex_id, def_value, lexicon_id)?;
            }
            other => {
                return Err(ProtocolError::Parse(format!(
                    "unrecognized Lexicon definition type: {other}"
                )));
            }
        }
    }

    Ok(builder)
}

/// Resolve a lexicon `ref` string (`"#frag"`, `"nsid"`, or `"nsid#frag"`)
/// to the full vertex id used by the schema graph.
fn resolve_ref_target(lexicon_id: &str, ref_target: &str) -> String {
    if let Some(frag) = ref_target.strip_prefix('#') {
        format!("{lexicon_id}#{frag}")
    } else {
        ref_target.to_owned()
    }
}

/// Emit the structural morphism for a `ref` property: resolve the
/// target, ensure a placeholder vertex exists for cross-lexicon
/// targets, and add a `ref` edge `src_vertex_id --ref--> resolved`.
///
/// Keeps the signature of the schema graph complete: every semantic
/// reference in the lexicon becomes an edge in `C_S`, so the referenced
/// sub-def has an incoming arrow and is no longer an edgeless
/// candidate for an entry vertex.
fn add_ref_edge(
    mut builder: SchemaBuilder,
    src_vertex_id: &str,
    lexicon_id: &str,
    ref_target: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    let resolved = resolve_ref_target(lexicon_id, ref_target);

    // Ensure the target vertex exists. Cross-lexicon refs and forward
    // refs to yet-unparsed defs need a placeholder; mirrors how
    // `parse_union_def` handles unresolved variants. Using `"ref"` kind
    // for placeholders marks them as opaque pointers rather than
    // typed structures.
    if !builder.has_vertex(&resolved) {
        builder = builder.vertex(&resolved, "ref", None)?;
    }

    builder = builder.edge(src_vertex_id, &resolved, "ref", None)?;
    Ok(builder)
}

/// Map a lexicon type string to our vertex kind.
fn lexicon_type_to_kind(type_str: &str) -> String {
    match type_str {
        "record" => "record",
        "array" => "array",
        "union" => "union",
        "string" => "string",
        "integer" => "integer",
        "boolean" => "boolean",
        "bytes" => "bytes",
        "cid-link" => "cid-link",
        "blob" => "blob",
        "unknown" => "unknown",
        "token" => "token",
        "query" => "query",
        "procedure" => "procedure",
        "subscription" => "subscription",
        "ref" => "ref",
        _ => "object",
    }
    .to_string()
}

/// Parse a record definition, creating the record-schema edge and body object.
fn parse_record_def(
    mut builder: SchemaBuilder,
    record_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    // A record has a record body (the "record" sub-object).
    if let Some(record_body) = def.get("record") {
        let body_id = format!("{record_id}:body");
        builder = builder.vertex(&body_id, "object", None)?;
        builder = builder.edge(record_id, &body_id, "record-schema", None)?;
        builder = parse_object_def(builder, &body_id, record_body, lexicon_id)?;
    }
    Ok(builder)
}

/// Parse an object definition, creating property edges.
fn parse_object_def(
    mut builder: SchemaBuilder,
    object_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(properties) = def.get("properties").and_then(serde_json::Value::as_object) {
        let required_fields: Vec<&str> = def
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|arr| arr.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default();

        for (prop_name, prop_def) in properties {
            let prop_type = prop_def
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("string");

            let prop_vertex_id = format!("{object_id}.{prop_name}");
            let prop_kind = lexicon_type_to_kind(prop_type);

            builder = builder.vertex(&prop_vertex_id, &prop_kind, None)?;
            builder = builder.edge(object_id, &prop_vertex_id, "prop", Some(prop_name))?;

            // Mark as required if in required list.
            if required_fields.contains(&prop_name.as_str()) {
                let req_edge = panproto_schema::Edge {
                    src: object_id.into(),
                    tgt: prop_vertex_id.as_str().into(),
                    kind: "prop".into(),
                    name: Some(prop_name.as_str().into()),
                };
                builder = builder.required(object_id, vec![req_edge]);
            }

            // Parse nested structure.
            match prop_type {
                "object" => {
                    builder = parse_object_def(builder, &prop_vertex_id, prop_def, lexicon_id)?;
                }
                "array" => {
                    builder = parse_array_def(builder, &prop_vertex_id, prop_def, lexicon_id)?;
                }
                "union" => {
                    builder = parse_union_def(builder, &prop_vertex_id, prop_def, lexicon_id)?;
                }
                "ref" => {
                    // A ref property is a morphism to the referenced sort.
                    // Record both the provenance constraint (literal
                    // lexicon string) and the structural edge in the
                    // signature graph.
                    if let Some(ref_target) =
                        prop_def.get("ref").and_then(serde_json::Value::as_str)
                    {
                        builder = builder.constraint(&prop_vertex_id, "ref", ref_target);
                        builder = add_ref_edge(builder, &prop_vertex_id, lexicon_id, ref_target)?;
                    }
                }
                _ => {
                    builder = parse_constraints(builder, &prop_vertex_id, prop_def);
                }
            }
        }
    }
    Ok(builder)
}

/// Parse an array definition, creating items edge.
fn parse_array_def(
    mut builder: SchemaBuilder,
    array_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(items) = def.get("items") {
        let items_type = items
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("string");

        let items_id = format!("{array_id}:items");
        let items_kind = lexicon_type_to_kind(items_type);

        builder = builder.vertex(&items_id, &items_kind, None)?;
        builder = builder.edge(array_id, &items_id, "items", None)?;

        match items_type {
            "object" => {
                builder = parse_object_def(builder, &items_id, items, lexicon_id)?;
            }
            "union" => {
                builder = parse_union_def(builder, &items_id, items, lexicon_id)?;
            }
            "ref" => {
                if let Some(ref_target) = items.get("ref").and_then(serde_json::Value::as_str) {
                    builder = builder.constraint(&items_id, "ref", ref_target);
                    builder = add_ref_edge(builder, &items_id, lexicon_id, ref_target)?;
                }
            }
            _ => {
                builder = parse_constraints(builder, &items_id, items);
            }
        }
    }
    Ok(builder)
}

/// Parse a union definition, creating variant edges.
///
/// A lexicon union is a coproduct `⊔_i T_i` where each `T_i` is the
/// sort referenced by `refs[i]`. In the schema graph we realize the
/// coproduct injections as pairs: a synthetic variant vertex carrying
/// the discriminant, and a `ref` morphism from that vertex to the
/// referenced sort. The variant edge remembers the lexicon discriminant
/// string in its `name`, and the ref edge restores reachability from
/// the union to the underlying sort so downstream machinery sees a
/// connected signature.
fn parse_union_def(
    mut builder: SchemaBuilder,
    union_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    if let Some(refs) = def.get("refs").and_then(serde_json::Value::as_array) {
        for (i, ref_val) in refs.iter().enumerate() {
            if let Some(ref_str) = ref_val.as_str() {
                let variant_id = format!("{union_id}:variant{i}");
                // The variant vertex is a stand-in for the coproduct
                // injection. Kind `"object"` reflects that in the
                // absence of resolution it exposes the same observables
                // as an object; the `ref` edge below then ties it to
                // the actual target sort.
                builder = builder.vertex(&variant_id, "object", None)?;
                builder = builder.edge(union_id, &variant_id, "variant", Some(ref_str))?;
                builder = add_ref_edge(builder, &variant_id, lexicon_id, ref_str)?;
            }
        }
    }
    Ok(builder)
}

/// Parse a query, procedure, or subscription definition.
///
/// These have optional `parameters` (input) and `output` sub-schemas.
fn parse_query_procedure_def(
    mut builder: SchemaBuilder,
    vertex_id: &str,
    def: &serde_json::Value,
    lexicon_id: &str,
) -> Result<SchemaBuilder, ProtocolError> {
    // Parse parameters (input schema).
    if let Some(params) = def.get("parameters") {
        let params_id = format!("{vertex_id}:params");
        let params_type = params
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object");
        let params_kind = lexicon_type_to_kind(params_type);
        builder = builder.vertex(&params_id, &params_kind, None)?;
        builder = builder.edge(vertex_id, &params_id, "prop", Some("parameters"))?;
        if params_type == "object" {
            builder = parse_object_def(builder, &params_id, params, lexicon_id)?;
        } else {
            builder = parse_constraints(builder, &params_id, params);
        }
    }

    // Parse input schema (used by procedures).
    if let Some(input) = def.get("input") {
        if let Some(input_schema) = input.get("schema") {
            let input_id = format!("{vertex_id}:input");
            let input_type = input_schema
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let input_kind = lexicon_type_to_kind(input_type);
            builder = builder.vertex(&input_id, &input_kind, None)?;
            builder = builder.edge(vertex_id, &input_id, "prop", Some("input"))?;
            if input_type == "object" {
                builder = parse_object_def(builder, &input_id, input_schema, lexicon_id)?;
            } else {
                builder = parse_constraints(builder, &input_id, input_schema);
            }
        }
    }

    // Parse output schema.
    if let Some(output) = def.get("output") {
        if let Some(output_schema) = output.get("schema") {
            let output_id = format!("{vertex_id}:output");
            let output_type = output_schema
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let output_kind = lexicon_type_to_kind(output_type);
            builder = builder.vertex(&output_id, &output_kind, None)?;
            builder = builder.edge(vertex_id, &output_id, "prop", Some("output"))?;
            if output_type == "object" {
                builder = parse_object_def(builder, &output_id, output_schema, lexicon_id)?;
            } else {
                builder = parse_constraints(builder, &output_id, output_schema);
            }
        }
    }

    // Parse message (used by subscriptions).
    if let Some(message) = def.get("message") {
        if let Some(msg_schema) = message.get("schema") {
            let msg_id = format!("{vertex_id}:message");
            let msg_type = msg_schema
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("object");
            let msg_kind = lexicon_type_to_kind(msg_type);
            builder = builder.vertex(&msg_id, &msg_kind, None)?;
            builder = builder.edge(vertex_id, &msg_id, "prop", Some("message"))?;
            if msg_type == "object" {
                builder = parse_object_def(builder, &msg_id, msg_schema, lexicon_id)?;
            } else {
                builder = parse_constraints(builder, &msg_id, msg_schema);
            }
        }
    }

    Ok(builder)
}

/// Parse constraints from a type definition.
///
/// In addition to the generic scalar constraints (`minLength`, `maxLength`,
/// `minimum`, `maximum`, `maxGraphemes`, `enum`, `const`, `default`,
/// `closed`), this preserves two atproto-specific string refinements
/// required by codegen and validation: `format` (datetime, did, at-uri,
/// cid, nsid, handle, tid, etc.) and `knownValues` (atproto's open enum).
/// Unknown `format` names pass through verbatim so the parser stays total
/// under atproto spec evolution.
fn parse_constraints(
    mut builder: SchemaBuilder,
    vertex_id: &str,
    def: &serde_json::Value,
) -> SchemaBuilder {
    let constraint_fields = [
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "maxGraphemes",
        "enum",
        "const",
        "default",
        "closed",
    ];

    for field in &constraint_fields {
        if let Some(value) = def.get(field) {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Array(arr) => {
                    // For enum arrays, join values.
                    arr.iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(",")
                }
                _ => value.to_string(),
            };
            builder = builder.constraint(vertex_id, field, &value_str);
        }
    }

    // atproto string refinement: `format` is a named string grammar
    // (datetime, at-uri, did, cid, nsid, handle, at-identifier, tid,
    // record-key, language, uri). Stored verbatim so codegen can pick
    // dedicated newtypes and so future spec additions parse total.
    if let Some(fmt) = def.get("format").and_then(serde_json::Value::as_str) {
        builder = builder.constraint(vertex_id, "format", fmt);
    }

    // atproto open enum: `knownValues` lists recognized string values;
    // unknown values are still wire-valid. Serialize the array as
    // canonical JSON into the existing string-valued Constraint shape
    // so downstream consumers deserialize it back.
    if let Some(known) = def.get("knownValues").and_then(serde_json::Value::as_array) {
        let values: Vec<&str> = known.iter().filter_map(serde_json::Value::as_str).collect();
        if !values.is_empty() {
            let serialized = serde_json::to_string(&values).unwrap_or_else(|_| String::from("[]"));
            builder = builder.constraint(vertex_id, "knownValues", &serialized);
        }
    }

    builder
}

/// Emit a [`Schema`] as an `ATProto` lexicon JSON value.
///
/// Reconstructs the lexicon document from the schema graph, including
/// the record body, properties, and constraints.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_lexicon(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // Find the root record vertex (has an nsid).
    let root = schema
        .vertices
        .values()
        .find(|v| v.nsid.is_some())
        .ok_or_else(|| ProtocolError::Emit("no root vertex with nsid found".into()))?;

    let nsid = root.nsid.as_deref().unwrap_or(&root.id);

    let mut defs = serde_json::Map::new();

    // Build the main definition.
    let main_def = emit_lexicon_def(schema, root)?;
    defs.insert("main".to_string(), main_def);

    Ok(serde_json::json!({
        "lexicon": 1,
        "id": nsid,
        "defs": defs
    }))
}

/// Emit a single lexicon definition as a JSON value.
fn emit_lexicon_def(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    use crate::emit::{children_by_edge, vertex_constraints};

    match vertex.kind.as_str() {
        "record" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!("record"));

            // Walk record-schema edge to get body object.
            let body_edges = children_by_edge(schema, &vertex.id, "record-schema");
            if let Some((_, body_vertex)) = body_edges.first() {
                let body = emit_lexicon_object(schema, body_vertex)?;
                obj.insert("record".to_string(), body);
            }

            Ok(serde_json::Value::Object(obj))
        }
        "object" => emit_lexicon_object(schema, vertex),
        "array" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!("array"));

            let items_edges = children_by_edge(schema, &vertex.id, "items");
            if let Some((_, items_vertex)) = items_edges.first() {
                let items_val = emit_lexicon_def(schema, items_vertex)?;
                obj.insert("items".to_string(), items_val);
            }

            Ok(serde_json::Value::Object(obj))
        }
        "union" => {
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!("union"));

            let variants = children_by_edge(schema, &vertex.id, "variant");
            let refs: Vec<serde_json::Value> = variants
                .iter()
                .filter_map(|(edge, _)| edge.name.as_deref().map(|n| serde_json::json!(n)))
                .collect();
            if !refs.is_empty() {
                obj.insert("refs".to_string(), serde_json::Value::Array(refs));
            }

            Ok(serde_json::Value::Object(obj))
        }
        _ => {
            // Scalar types: string, integer, boolean, etc.
            let mut obj = serde_json::Map::new();
            obj.insert("type".to_string(), serde_json::json!(vertex.kind.as_str()));

            // Add constraints.
            let constraints = vertex_constraints(schema, &vertex.id);
            for c in &constraints {
                let val = emit_constraint_value(c);
                obj.insert(c.sort.to_string(), val);
            }

            Ok(serde_json::Value::Object(obj))
        }
    }
}

/// Emit an object definition with properties.
fn emit_lexicon_object(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    use crate::emit::children_by_edge;

    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), serde_json::json!("object"));

    let props = children_by_edge(schema, &vertex.id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, prop_vertex) in &props {
            let prop_name = edge.name.as_deref().unwrap_or(&prop_vertex.id);
            let prop_val = emit_lexicon_def(schema, prop_vertex)?;
            properties.insert(prop_name.to_string(), prop_val);
        }
        obj.insert(
            "properties".to_string(),
            serde_json::Value::Object(properties),
        );
    }

    // Reconstruct required fields from schema.required.
    if let Some(req_edges) = schema.required.get(&vertex.id) {
        let required: Vec<serde_json::Value> = req_edges
            .iter()
            .filter_map(|e| e.name.as_deref().map(|n| serde_json::json!(n)))
            .collect();
        if !required.is_empty() {
            obj.insert("required".to_string(), serde_json::Value::Array(required));
        }
    }

    Ok(serde_json::Value::Object(obj))
}

/// Convert a constraint to a JSON value, using numbers where appropriate.
fn emit_constraint_value(c: &panproto_schema::Constraint) -> serde_json::Value {
    match c.sort.as_str() {
        "minLength" | "maxLength" | "minimum" | "maximum" | "maxGraphemes" => c
            .value
            .parse::<i64>()
            .map_or_else(|_| serde_json::json!(c.value), |n| serde_json::json!(n)),
        "closed" => c
            .value
            .parse::<bool>()
            .map_or_else(|_| serde_json::json!(c.value), |b| serde_json::json!(b)),
        "enum" => {
            let vals: Vec<serde_json::Value> = c
                .value
                .split(',')
                .map(|s| serde_json::json!(s.trim()))
                .collect();
            serde_json::Value::Array(vals)
        }
        _ => serde_json::json!(c.value),
    }
}

/// Well-formedness rules for `ATProto` edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "record-schema".into(),
            src_kinds: vec!["record".into()],
            tgt_kinds: vec!["object".into()],
        },
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec![
                "object".into(),
                "query".into(),
                "procedure".into(),
                "subscription".into(),
            ],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec!["union".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "self-ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_creates_valid_definition() {
        let p = protocol();
        assert_eq!(p.name, "atproto");
        assert_eq!(p.schema_theory, "ThATProtoSchema");
        assert_eq!(p.instance_theory, "ThATProtoInstance");
        assert!(!p.edge_rules.is_empty());
        assert!(p.find_edge_rule("record-schema").is_some());
        assert!(p.find_edge_rule("prop").is_some());
        assert!(p.find_edge_rule("items").is_some());
        assert!(p.find_edge_rule("variant").is_some());
    }

    #[test]
    fn register_theories_adds_correct_theories() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);

        assert!(registry.contains_key("ThGraph"), "ThGraph missing");
        assert!(
            registry.contains_key("ThConstraint"),
            "ThConstraint missing"
        );
        assert!(registry.contains_key("ThMulti"), "ThMulti missing");
        assert!(registry.contains_key("ThWType"), "ThWType missing");
        assert!(registry.contains_key("ThMeta"), "ThMeta missing");
        assert!(
            registry.contains_key("ThATProtoSchema"),
            "ThATProtoSchema missing"
        );
        assert!(
            registry.contains_key("ThATProtoInstance"),
            "ThATProtoInstance missing"
        );

        // Verify schema theory has expected sorts.
        let schema_t = &registry["ThATProtoSchema"];
        assert!(schema_t.find_sort("Vertex").is_some());
        assert!(schema_t.find_sort("Edge").is_some());
        assert!(schema_t.find_sort("Constraint").is_some());
    }

    #[test]
    fn parse_simple_lexicon() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "app.bsky.feed.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["text", "createdAt"],
                        "properties": {
                            "text": {
                                "type": "string",
                                "maxLength": 3000,
                                "maxGraphemes": 300
                            },
                            "createdAt": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        });

        let schema = parse_lexicon(&lexicon);
        assert!(schema.is_ok(), "parse_lexicon should succeed: {schema:?}");
        let schema = schema.ok();
        let schema = schema.as_ref();

        // Should have: record vertex, body object, text string, createdAt string.
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post")),
            "record vertex should exist"
        );
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post:body")),
            "body object vertex should exist"
        );
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post:body.text")),
            "text vertex should exist"
        );
        assert!(
            schema.is_some_and(|s| s.has_vertex("app.bsky.feed.post:body.createdAt")),
            "createdAt vertex should exist"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn emit_lexicon_roundtrip() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "app.bsky.feed.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["text", "createdAt"],
                        "properties": {
                            "text": {
                                "type": "string",
                                "maxLength": 3000,
                                "maxGraphemes": 300
                            },
                            "createdAt": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        });

        let schema1 = parse_lexicon(&lexicon).expect("first parse should succeed");
        let emitted = emit_lexicon(&schema1).expect("emit should succeed");
        let schema2 = parse_lexicon(&emitted).expect("re-parse should succeed");

        assert_eq!(
            schema1.vertex_count(),
            schema2.vertex_count(),
            "vertex counts should match after round-trip"
        );
        assert_eq!(
            schema1.edge_count(),
            schema2.edge_count(),
            "edge counts should match after round-trip"
        );
    }

    #[test]
    fn parse_lexicon_missing_id_fails() {
        let lexicon = serde_json::json!({
            "defs": {}
        });

        let result = parse_lexicon(&lexicon);
        assert!(result.is_err());
    }

    /// A `ref`-typed array item must emit a structural morphism from
    /// the synthetic items vertex to the referenced sort, mirroring the
    /// object-property case. Without this edge the referenced sub-def
    /// is disconnected from every array that contains it.
    #[test]
    #[allow(clippy::expect_used)]
    fn ref_array_item_emits_morphism() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "com.example.list",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "properties": {
                            "items": {
                                "type": "array",
                                "items": {"type": "ref", "ref": "#item"}
                            }
                        }
                    }
                },
                "item": {
                    "type": "object",
                    "properties": {"label": {"type": "string"}}
                }
            }
        });

        let schema = parse_lexicon(&lexicon).expect("parse");
        let items_vertex = "com.example.list:body.items:items";
        let item_def = "com.example.list#item";
        let has_edge = schema
            .edges
            .keys()
            .any(|e| &*e.src == items_vertex && &*e.tgt == item_def && &*e.kind == "ref");
        assert!(has_edge, "items vertex must have a ref morphism to #item");
        assert!(!schema.incoming_edges(item_def).is_empty());
    }

    /// Each variant of a lexicon union must emit a `ref` edge to the
    /// branch's target sort. The union-as-coproduct `⊔ᵢ Tᵢ` is
    /// realized by injection-labelled variant vertices plus a
    /// morphism from each variant to its branch.
    #[test]
    #[allow(clippy::expect_used)]
    fn union_variants_emit_ref_morphisms() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "com.example.msg",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "properties": {
                            "payload": {
                                "type": "union",
                                "refs": ["#a", "#b"]
                            }
                        }
                    }
                },
                "a": {"type": "object", "properties": {}},
                "b": {"type": "object", "properties": {}}
            }
        });

        let schema = parse_lexicon(&lexicon).expect("parse");
        let union_vertex = "com.example.msg:body.payload";
        let variant_a = format!("{union_vertex}:variant0");
        let variant_b = format!("{union_vertex}:variant1");

        let ref_edge_exists = |src: &str, tgt: &str| {
            schema
                .edges
                .keys()
                .any(|e| &*e.src == src && &*e.tgt == tgt && &*e.kind == "ref")
        };

        assert!(
            ref_edge_exists(&variant_a, "com.example.msg#a"),
            "variant0 must have a ref morphism to #a"
        );
        assert!(
            ref_edge_exists(&variant_b, "com.example.msg#b"),
            "variant1 must have a ref morphism to #b"
        );
        assert!(!schema.incoming_edges("com.example.msg#a").is_empty());
        assert!(!schema.incoming_edges("com.example.msg#b").is_empty());
    }

    /// A cross-lexicon ref must create an opaque placeholder vertex
    /// and point the morphism at it. The target becomes a legitimate
    /// sort in the signature graph so downstream reachability analysis
    /// can see the reference.
    #[test]
    #[allow(clippy::expect_used)]
    fn cross_lexicon_ref_creates_placeholder() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "com.example.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "properties": {
                            "attach": {
                                "type": "ref",
                                "ref": "com.atproto.repo.strongRef"
                            }
                        }
                    }
                }
            }
        });

        let schema = parse_lexicon(&lexicon).expect("parse");
        assert!(
            schema.has_vertex("com.atproto.repo.strongRef"),
            "placeholder vertex for cross-lexicon ref must exist"
        );
        let attach_vertex = "com.example.post:body.attach";
        let has_edge = schema.edges.keys().any(|e| {
            &*e.src == attach_vertex && &*e.tgt == "com.atproto.repo.strongRef" && &*e.kind == "ref"
        });
        assert!(has_edge, "cross-lexicon ref must emit a morphism");
    }

    /// Regression for panproto#35.
    ///
    /// A record with an optional `ref` property pointing to a sibling
    /// sub-def must (a) declare the record as the schema's entry
    /// basepoint and *not* the sub-def, and (b) carry a structural
    /// `ref` edge from the property vertex to the sub-def vertex so
    /// that the sub-def has an incoming arrow in the signature graph.
    #[test]
    #[allow(clippy::expect_used)]
    fn ref_property_emits_entry_and_morphism() {
        let lexicon = serde_json::json!({
            "lexicon": 1,
            "id": "app.bsky.feed.post",
            "defs": {
                "main": {
                    "type": "record",
                    "record": {
                        "type": "object",
                        "required": ["text", "createdAt"],
                        "properties": {
                            "text": {"type": "string"},
                            "createdAt": {"type": "string"},
                            "reply": {"type": "ref", "ref": "#replyRef"}
                        }
                    }
                },
                "replyRef": {
                    "type": "object",
                    "required": ["root", "parent"],
                    "properties": {
                        "root": {"type": "string"},
                        "parent": {"type": "string"}
                    }
                }
            }
        });

        let schema = parse_lexicon(&lexicon).expect("parse should succeed");

        // Entries: the record is an entry; the sub-def is not.
        let entries: Vec<&str> = schema.entry_vertices().iter().map(AsRef::as_ref).collect();
        assert_eq!(entries, vec!["app.bsky.feed.post"]);

        // primary_entry picks the record, not the orphaned sub-def.
        assert_eq!(
            panproto_schema::primary_entry(&schema).map(AsRef::as_ref),
            Some("app.bsky.feed.post"),
        );

        // A ref edge exists from the reply prop vertex to the sub-def;
        // the sub-def is no longer edgeless.
        let reply_prop = "app.bsky.feed.post:body.reply";
        let reply_ref_def = "app.bsky.feed.post#replyRef";
        let has_ref_edge = schema
            .edges
            .keys()
            .any(|e| &*e.src == reply_prop && &*e.tgt == reply_ref_def && &*e.kind == "ref");
        assert!(
            has_ref_edge,
            "expected a ref edge {reply_prop} -> {reply_ref_def}, edges: {:?}",
            schema.edges.keys().collect::<Vec<_>>()
        );
        assert!(
            !schema.incoming_edges(reply_ref_def).is_empty(),
            "sub-def should have at least one incoming edge after ref-morphism fix"
        );
    }

    /// The two documents of the cross-file ref case: an
    /// `annotationLayer` record whose `anchor` property refs a
    /// `spatioTemporalAnchor` living in a sibling `defs` document, which
    /// in turn refs a `boundingBox` beside it.
    fn cross_file_docs() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "lexicon": 1,
                "id": "pub.layers.annotation.annotationLayer",
                "defs": {
                    "main": {
                        "type": "record",
                        "record": {
                            "type": "object",
                            "required": ["anchor"],
                            "properties": {
                                "anchor": {
                                    "type": "ref",
                                    "ref": "pub.layers.defs#spatioTemporalAnchor"
                                }
                            }
                        }
                    }
                }
            }),
            serde_json::json!({
                "lexicon": 1,
                "id": "pub.layers.defs",
                "defs": {
                    "spatioTemporalAnchor": {
                        "type": "object",
                        "required": ["box"],
                        "properties": {
                            "box": {"type": "ref", "ref": "#boundingBox"}
                        }
                    },
                    "boundingBox": {
                        "type": "object",
                        "required": ["x", "y"],
                        "properties": {
                            "x": {"type": "integer"},
                            "y": {"type": "integer"}
                        }
                    }
                }
            }),
        ]
    }

    /// `parse_lexicon_project` keeps per-file provenance: each document
    /// becomes its own schema (with in-set refs resolved to typed defs),
    /// and a cross-document ref is lifted out of the referencing file
    /// into a path-prefixed cross-file edge.
    #[test]
    #[allow(clippy::expect_used)]
    fn lexicon_project_partitions_by_file_and_lifts_cross_refs() {
        let docs = cross_file_docs();
        let p1 = PathBuf::from("annotation/annotationLayer.json");
        let p2 = PathBuf::from("defs.json");
        let project = parse_lexicon_project(&[
            LexiconDoc {
                path: p1.clone(),
                value: docs[0].clone(),
            },
            LexiconDoc {
                path: p2.clone(),
                value: docs[1].clone(),
            },
        ])
        .expect("parse project should succeed");

        assert_eq!(project.files.len(), 2, "one schema per document");

        let file1 = &project
            .files
            .iter()
            .find(|(p, _)| *p == p1)
            .expect("annotationLayer file")
            .1;
        let file2 = &project
            .files
            .iter()
            .find(|(p, _)| *p == p2)
            .expect("defs file")
            .1;

        // The defs file owns the referenced def, typed as an object
        // (resolved by the bundle pass, not left an opaque placeholder),
        // and its own internal box -> boundingBox ref stays inside it.
        assert_eq!(
            &*file2.vertices["pub.layers.defs#spatioTemporalAnchor"].kind,
            "object"
        );
        assert!(file2.vertices.contains_key("pub.layers.defs#boundingBox"));
        assert!(
            file2
                .edges
                .keys()
                .any(|e| &*e.kind == "ref" && &*e.tgt == "pub.layers.defs#boundingBox"),
            "the same-file box -> boundingBox ref must stay internal"
        );

        // The referencing file does not carry the sibling document's def:
        // the cross-document ref was lifted out.
        assert!(
            !file1
                .vertices
                .contains_key("pub.layers.defs#spatioTemporalAnchor"),
            "a cross-document def must not appear in the referencing file"
        );

        // The lifted ref is recorded as a cross-file edge with both
        // endpoints prefixed by their owning file.
        let cross = project
            .cross_file_edges
            .get(&p1)
            .expect("cross-file edges for the referencing file");
        assert!(
            cross.iter().any(|e| &*e.kind == "ref"
                && e.tgt
                    .contains("defs.json::pub.layers.defs#spatioTemporalAnchor")
                && e.src.starts_with("annotation/annotationLayer.json::")),
            "expected a path-prefixed cross-file ref, got: {cross:?}"
        );
    }

    /// Parsing one document alone leaves its cross-document ref target
    /// an opaque `"ref"` placeholder: this is the behavior
    /// `parse_lexicon_bundle` exists to improve on.
    #[test]
    #[allow(clippy::expect_used)]
    fn single_document_leaves_cross_file_ref_opaque() {
        let docs = cross_file_docs();
        let schema = parse_lexicon(&docs[0]).expect("parse should succeed");

        let anchor_def = &schema.vertices["pub.layers.defs#spatioTemporalAnchor"];
        assert_eq!(
            &*anchor_def.kind, "ref",
            "a lone document cannot type its cross-file ref target"
        );
    }

    /// Parsing the same documents as a bundle resolves the ref chain to
    /// the real, typed defs, so the nested geometry is reachable.
    #[test]
    #[allow(clippy::expect_used)]
    fn bundle_resolves_cross_file_refs_to_typed_defs() {
        let docs = cross_file_docs();
        let schema = parse_lexicon_bundle(&docs).expect("bundle parse should succeed");

        // Both hops of the chain are typed objects, not placeholders.
        assert_eq!(
            &*schema.vertices["pub.layers.defs#spatioTemporalAnchor"].kind,
            "object"
        );
        assert_eq!(
            &*schema.vertices["pub.layers.defs#boundingBox"].kind,
            "object"
        );

        // The nested geometry the lens needs to bind to is present as
        // property vertices, which the placeholder never carried.
        for prop in [
            "pub.layers.defs#spatioTemporalAnchor.box",
            "pub.layers.defs#boundingBox.x",
            "pub.layers.defs#boundingBox.y",
        ] {
            assert!(
                schema.vertices.contains_key(prop),
                "expected resolved property vertex {prop}, vertices: {:?}",
                schema.vertices.keys().collect::<Vec<_>>()
            );
        }

        // The record remains the sole entry: pulling a sibling
        // document's defs in must not promote them to basepoints.
        let entries: Vec<&str> = schema.entry_vertices().iter().map(AsRef::as_ref).collect();
        assert_eq!(entries, vec!["pub.layers.annotation.annotationLayer"]);
    }

    /// A ref to a document outside the bundle still yields a
    /// placeholder: that is what marks it as genuinely external.
    #[test]
    #[allow(clippy::expect_used)]
    fn bundle_keeps_placeholder_for_out_of_bundle_ref() {
        let docs = cross_file_docs();
        let schema = parse_lexicon_bundle(&docs[..1]).expect("bundle parse should succeed");

        assert_eq!(
            &*schema.vertices["pub.layers.defs#spatioTemporalAnchor"].kind,
            "ref"
        );
    }

    /// A single-document bundle agrees with `parse_lexicon`, which is
    /// implemented in terms of it.
    #[test]
    #[allow(clippy::expect_used)]
    fn single_document_bundle_matches_parse_lexicon() {
        let docs = cross_file_docs();
        let direct = parse_lexicon(&docs[1]).expect("parse should succeed");
        let bundled = parse_lexicon_bundle(&docs[1..]).expect("bundle parse should succeed");

        assert_eq!(
            direct
                .vertices
                .keys()
                .collect::<std::collections::BTreeSet<_>>(),
            bundled
                .vertices
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
        );
        assert_eq!(
            direct
                .edges
                .keys()
                .collect::<std::collections::BTreeSet<_>>(),
            bundled
                .edges
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn bundle_rejects_duplicate_lexicon_ids() {
        let docs = cross_file_docs();
        let dupes = vec![docs[0].clone(), docs[0].clone()];

        let err = parse_lexicon_bundle(&dupes).expect_err("duplicate ids must be rejected");
        assert!(
            err.to_string().contains("duplicate lexicon id"),
            "unexpected error: {err}"
        );
    }
}
