//! JSON Schema protocol definition and parser.
//!
//! JSON Schema uses the same constrained multigraph schema theory as
//! `ATProto`: `colimit(ThGraph, ThConstraint, ThMulti)`.
//! Instance theory: `ThWType`.
//!
//! The parser handles `$ref`, `allOf`, `oneOf`, `anyOf`, and standard
//! type/constraint keywords.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use panproto_gat::Theory;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

use crate::error::ProtocolError;
use crate::theories;

/// Returns the JSON Schema protocol definition.
///
/// Schema theory: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance theory: `ThWType`.
#[must_use]
pub fn protocol() -> Protocol {
    Protocol {
        name: "json-schema".into(),
        schema_theory: "ThJsonSchemaSchema".into(),
        instance_theory: "ThJsonSchemaInstance".into(),
        edge_rules: edge_rules(),
        obj_kinds: vec![
            "object".into(),
            "array".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "unknown".into(),
            "not".into(),
            "if".into(),
            "then".into(),
            "else".into(),
            "union".into(),
        ],
        constraint_sorts: vec![
            "type".into(),
            "minLength".into(),
            "maxLength".into(),
            "pattern".into(),
            "minimum".into(),
            "maximum".into(),
            "required".into(),
            "format".into(),
            "enum".into(),
            "const".into(),
            "additionalProperties".into(),
        ],
        has_order: true,
        has_coproducts: true,
        has_recursion: true,
        ..Protocol::default()
    }
}

/// Register the component GATs for JSON Schema with a theory registry.
///
/// JSON Schema shares the constrained-multigraph schema theory and
/// W-type instance theory with `ATProto`, `bson`, and `cddl`, so it
/// registers through the shared helper rather than composing the
/// colimit by hand.
pub fn register_theories<S: BuildHasher>(registry: &mut HashMap<String, Theory, S>) {
    theories::register_constrained_multigraph_wtype(
        registry,
        "ThJsonSchemaSchema",
        "ThJsonSchemaInstance",
    );
}

/// Parse a JSON Schema document into a [`Schema`].
///
/// Handles `type`, `properties`, `items`, `allOf`, `oneOf`, `anyOf`,
/// and `$ref` keywords. Nested schemas are flattened into vertices
/// and edges.
///
/// # Errors
///
/// Returns [`ProtocolError`] if the JSON is not a valid schema or
/// if construction fails.
pub fn parse_json_schema(json: &serde_json::Value) -> Result<Schema, ProtocolError> {
    parse_json_schema_bundle(std::slice::from_ref(json))
}

/// The vertex-id prefix for a document, and the `$id` other documents
/// address it by.
///
/// A document's identity is its `$id`, which is what a cross-document
/// `$ref` names. A document without one is addressed as `root` when it
/// is alone and `doc{i}` otherwise, so ids stay unique and a
/// single-document parse keeps the `root:` prefix it has always had.
fn document_identity(
    json: &serde_json::Value,
    index: usize,
    count: usize,
) -> (String, Option<&str>) {
    let declared = json.get("$id").and_then(serde_json::Value::as_str);
    let prefix = match declared {
        Some(id) => id.to_owned(),
        None if count == 1 => "root".to_owned(),
        None => format!("doc{index}"),
    };
    (prefix, declared)
}

/// Parse a bundle of JSON Schema documents into one [`Schema`],
/// resolving `$ref`s across the whole bundle.
///
/// A `$ref` may name a definition in a sibling document, spelled
/// against that document's `$id` (`https://example.com/common.json#/$defs/Address`).
/// The full reference map is built for every document before any
/// document's body is walked, so such a ref lands on the referenced
/// definition's real, typed vertex rather than on the opaque
/// placeholder the parser creates for an unresolvable ref. A ref to a
/// document outside the bundle still becomes a placeholder, which is
/// what marks it as genuinely external.
///
/// Building the whole map first also resolves a `$ref` to a definition
/// declared later in the same document, which the incremental map could
/// not.
///
/// A relative-path `$ref` (`./common.json#/$defs/Address`) is not
/// resolved: a bundle is an unordered array of documents with no
/// filenames, so there is nothing for a path to be relative to. Give
/// the documents `$id`s to have their cross-references resolve.
///
/// Passing a single document is equivalent to [`parse_json_schema`].
///
/// # Errors
///
/// Returns [`ProtocolError::Parse`] if two documents declare the same
/// `$id`, or a construction error from the schema builder.
pub fn parse_json_schema_bundle(docs: &[serde_json::Value]) -> Result<Schema, ProtocolError> {
    let proto = protocol();
    let mut builder = SchemaBuilder::new(&proto);

    // Pass 1, over the whole bundle: map every reference spelling to the
    // vertex id it names, before walking any body. A definition's vertex
    // id follows from its document and its name, so this needs no walk,
    // and doing it first is what lets a ref reach a sibling document (or
    // a later definition in its own).
    let mut defs_map: HashMap<String, String> = HashMap::new();
    let mut identities: Vec<(String, Option<&str>)> = Vec::with_capacity(docs.len());
    let mut seen_ids: HashSet<&str> = HashSet::with_capacity(docs.len());

    for (i, json) in docs.iter().enumerate() {
        let (prefix, declared) = document_identity(json, i, docs.len());
        if let Some(id) = declared {
            if !seen_ids.insert(id) {
                return Err(ProtocolError::Parse(format!(
                    "duplicate $id in bundle: {id}"
                )));
            }
        }

        for defs_key in &["$defs", "definitions"] {
            if let Some(defs) = json.get(*defs_key).and_then(serde_json::Value::as_object) {
                for def_name in defs.keys() {
                    let def_id = format!("{prefix}:{defs_key}/{def_name}");
                    // Within the document: `#/$defs/Name`. Only the
                    // sole document of a single-document parse claims
                    // the bare spelling, since in a bundle it would be
                    // ambiguous between documents.
                    if docs.len() == 1 {
                        defs_map.insert(format!("#/{defs_key}/{def_name}"), def_id.clone());
                    }
                    // From anywhere: `<$id>#/$defs/Name`.
                    if let Some(id) = declared {
                        defs_map.insert(format!("{id}#/{defs_key}/{def_name}"), def_id.clone());
                    }
                }
            }
        }
        identities.push((prefix, declared));
    }

    // Pass 2: walk each document. Every in-bundle ref target is already
    // in `defs_map`, so the placeholder path is never taken for one.
    let mut counter: usize = 0;
    let mut refs = Vec::new();
    for (json, (prefix, _)) in docs.iter().zip(&identities) {
        let mut local = defs_map.clone();
        for defs_key in &["$defs", "definitions"] {
            if let Some(defs) = json.get(*defs_key).and_then(serde_json::Value::as_object) {
                for def_name in defs.keys() {
                    // A document's own `#/...` refs resolve to its own
                    // definitions, whichever document is being walked.
                    local.insert(
                        format!("#/{defs_key}/{def_name}"),
                        format!("{prefix}:{defs_key}/{def_name}"),
                    );
                }
            }
        }

        let mut walker = Walker {
            counter,
            defs_map: &local,
            doc: json,
            pointers: HashMap::new(),
            refs: Vec::new(),
            pointer_refs: Vec::new(),
        };
        for defs_key in &["$defs", "definitions"] {
            if let Some(defs) = json.get(*defs_key).and_then(serde_json::Value::as_object) {
                for (def_name, def_schema) in defs {
                    let def_id = format!("{prefix}:{defs_key}/{def_name}");
                    let pointer = format!("#/{defs_key}/{}", pointer_token(def_name));
                    builder = walker.walk(builder, def_schema, &def_id, &pointer)?;
                    // Each named definition is an entry (basepoint) so a
                    // lens or migration can root at it, mirroring how
                    // `bson`/`cddl` declare their top-level rules.
                    builder = builder.entry(&def_id);
                }
            }
        }

        builder = walker.walk(builder, json, prefix, "#")?;
        builder = builder.entry(prefix);
        builder = walker.resolve_pointer_refs(builder)?;
        counter = walker.counter;
        refs.append(&mut walker.refs);
    }
    // Every vertex of every document exists now, so each in-bundle ref
    // edge has its target.
    for (src, tgt, name) in &refs {
        builder = builder.edge(src, tgt, "ref", Some(name))?;
    }

    let schema = builder.build()?;
    Ok(schema)
}

/// The state one walk over a bundle carries: the counter that keeps
/// anonymous vertex ids unique, the map from `$ref` spellings to
/// definition vertices, and the `ref` edges to in-bundle definitions.
///
/// A `ref` edge to a definition is recorded rather than added when it is
/// met, because its target need not be walked yet: a definition may
/// reference one declared after it, and a document's body may reference
/// a definition of a later document. The bundle parse adds them once
/// every document is walked.
///
/// A `$ref` that is a JSON pointer into its own document
/// (`#/definitions/a/properties/b`, or `#` for the root) lands on the
/// vertex walked for the subschema it points at, once the document is
/// walked; one pointing at something that is not a walked subschema gets
/// a placeholder, as an unresolvable ref does.
struct Walker<'a> {
    counter: usize,
    defs_map: &'a HashMap<String, String>,
    /// The document being walked.
    doc: &'a serde_json::Value,
    /// The JSON pointer (`#/...`) of each walked subschema of the
    /// document, to its vertex.
    pointers: HashMap<String, String>,
    /// `(source, definition vertex, ref spelling)` for each ref edge.
    refs: Vec<(String, String, String)>,
    /// `(source, pointer)` for each ref into the document itself.
    pointer_refs: Vec<(String, String)>,
}

/// A JSON object key escaped as a JSON pointer reference token.
fn pointer_token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

impl Walker<'_> {
    /// Add the ref edge of each `$ref` into the document just walked.
    fn resolve_pointer_refs(
        &mut self,
        mut builder: SchemaBuilder,
    ) -> Result<SchemaBuilder, ProtocolError> {
        for (src, pointer) in std::mem::take(&mut self.pointer_refs) {
            if let Some(target) = self.pointers.get(&pointer) {
                builder = builder.edge(&src, target, "ref", Some(&pointer))?;
            } else {
                self.counter += 1;
                let ref_id = format!("{src}:ref{}", self.counter);
                builder = builder.vertex(&ref_id, "object", None)?;
                builder = builder.edge(&src, &ref_id, "ref", Some(&pointer))?;
            }
        }
        Ok(builder)
    }

    /// Recursively walk a JSON Schema object.
    ///
    /// Structural keywords apply to the instances of one JSON type:
    /// `properties`, `patternProperties` and `additionalProperties` to
    /// objects, `items` to arrays. On a schema whose `type` is a list,
    /// they hang off the list's variant of that type, so
    /// `{"type": ["object", "null"], "properties": {...}}` gives its
    /// properties to the `object` variant. On a schema that admits no
    /// instance of the keyword's type (`{"type": "string", "items": ...}`)
    /// the keyword constrains nothing, as JSON Schema validation defines
    /// it, and adds no vertex.
    fn walk(
        &mut self,
        mut builder: SchemaBuilder,
        schema: &serde_json::Value,
        current_id: &str,
        pointer: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        self.pointers
            .insert(pointer.to_owned(), current_id.to_owned());
        let (b, object_holder, array_holder) = type_vertices(builder, schema, current_id)?;
        builder = constraints(b, schema, current_id);

        if let Some(holder) = object_holder.as_deref() {
            builder = self.walk_object_keywords(builder, schema, current_id, pointer, holder)?;
        }

        // Handle items (array type).
        if let (Some(holder), Some(items)) = (array_holder.as_deref(), schema.get("items")) {
            let items_id = format!("{current_id}:items");
            builder = self.walk(builder, items, &items_id, &format!("{pointer}/items"))?;
            builder = builder.edge(holder, &items_id, "items", None)?;
        }

        // Handle allOf / oneOf / anyOf as union-like structures.
        for combiner in &["allOf", "oneOf", "anyOf"] {
            if let Some(arr) = schema.get(*combiner).and_then(serde_json::Value::as_array) {
                for (i, sub_schema) in arr.iter().enumerate() {
                    let sub_id = format!("{current_id}:{combiner}{i}");
                    let sub_pointer = format!("{pointer}/{combiner}/{i}");
                    builder = self.walk(builder, sub_schema, &sub_id, &sub_pointer)?;
                    builder = builder.edge(current_id, &sub_id, "variant", Some(combiner))?;
                }
            }
        }

        // Handle `not`: recurse into the schema, creating a vertex with kind "not".
        if let Some(not_schema) = schema.get("not") {
            self.counter += 1;
            let not_id = format!("{current_id}:not{}", self.counter);
            builder = builder.vertex(&not_id, "not", None)?;
            // Walk the negated schema as a child of the not vertex.
            let negated_id = format!("{not_id}:negated");
            builder = self.walk(builder, not_schema, &negated_id, &format!("{pointer}/not"))?;
            builder = builder.edge(&not_id, &negated_id, "variant", Some("not"))?;
            builder = builder.edge(current_id, &not_id, "variant", Some("not"))?;
        }

        builder = self.walk_conditional(builder, schema, current_id, pointer)?;

        // Handle $ref: resolve to a definition vertex or create a ref vertex.
        if let Some(ref_str) = schema.get("$ref").and_then(serde_json::Value::as_str) {
            if let Some(def_vertex_id) = self.defs_map.get(ref_str) {
                self.refs.push((
                    current_id.to_owned(),
                    def_vertex_id.clone(),
                    ref_str.to_owned(),
                ));
            } else if ref_str
                .strip_prefix('#')
                .is_some_and(|path| self.doc.pointer(path).is_some())
            {
                self.pointer_refs
                    .push((current_id.to_owned(), ref_str.to_owned()));
            } else {
                self.counter += 1;
                let ref_id = format!("{current_id}:ref{}", self.counter);
                builder = builder.vertex(&ref_id, "object", None)?;
                builder = builder.edge(current_id, &ref_id, "ref", Some(ref_str))?;
            }
        }

        Ok(builder)
    }

    /// Walk `if`/`then`/`else`, a vertex for each conditional branch.
    fn walk_conditional(
        &mut self,
        mut builder: SchemaBuilder,
        schema: &serde_json::Value,
        current_id: &str,
        pointer: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        let Some(if_schema) = schema.get("if") else {
            return Ok(builder);
        };
        self.counter += 1;
        let n = self.counter;
        let if_id = format!("{current_id}:if{n}");
        builder = builder.vertex(&if_id, "if", None)?;
        let if_cond_id = format!("{if_id}:cond");
        builder = self.walk(builder, if_schema, &if_cond_id, &format!("{pointer}/if"))?;
        builder = builder.edge(&if_id, &if_cond_id, "variant", Some("condition"))?;
        builder = builder.edge(current_id, &if_id, "variant", Some("if"))?;

        if let Some(then_schema) = schema.get("then") {
            let then_id = format!("{current_id}:then{n}");
            builder = builder.vertex(&then_id, "then", None)?;
            let then_body_id = format!("{then_id}:body");
            let then_pointer = format!("{pointer}/then");
            builder = self.walk(builder, then_schema, &then_body_id, &then_pointer)?;
            builder = builder.edge(&then_id, &then_body_id, "variant", Some("body"))?;
            builder = builder.edge(current_id, &then_id, "variant", Some("then"))?;
        }

        if let Some(else_schema) = schema.get("else") {
            let else_id = format!("{current_id}:else{n}");
            builder = builder.vertex(&else_id, "else", None)?;
            let else_body_id = format!("{else_id}:body");
            let else_pointer = format!("{pointer}/else");
            builder = self.walk(builder, else_schema, &else_body_id, &else_pointer)?;
            builder = builder.edge(&else_id, &else_body_id, "variant", Some("body"))?;
            builder = builder.edge(current_id, &else_id, "variant", Some("else"))?;
        }
        Ok(builder)
    }

    /// Walk `properties`, `patternProperties` and `additionalProperties`,
    /// hanging each property off `holder`, the vertex of the schema's
    /// object instances. Property vertex ids stay rooted at `current_id`,
    /// the schema's own vertex.
    fn walk_object_keywords(
        &mut self,
        mut builder: SchemaBuilder,
        schema: &serde_json::Value,
        current_id: &str,
        pointer: &str,
        holder: &str,
    ) -> Result<SchemaBuilder, ProtocolError> {
        if let Some(properties) = schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
        {
            for (prop_name, prop_schema) in properties {
                let prop_id = format!("{current_id}.{prop_name}");
                let prop_pointer = format!("{pointer}/properties/{}", pointer_token(prop_name));
                builder = self.walk(builder, prop_schema, &prop_id, &prop_pointer)?;
                builder = builder.edge(holder, &prop_id, "prop", Some(prop_name))?;
            }
        }

        if let Some(pattern_props) = schema
            .get("patternProperties")
            .and_then(serde_json::Value::as_object)
        {
            for (pattern, prop_schema) in pattern_props {
                self.counter += 1;
                let prop_id = format!("{current_id}:patternProp{}", self.counter);
                let prop_pointer =
                    format!("{pointer}/patternProperties/{}", pointer_token(pattern));
                builder = self.walk(builder, prop_schema, &prop_id, &prop_pointer)?;
                builder = builder.edge(holder, &prop_id, "pattern-prop", Some(pattern))?;
            }
        }

        if let Some(additional) = schema.get("additionalProperties") {
            match additional {
                serde_json::Value::Bool(false) => {
                    builder = builder.constraint(holder, "additionalProperties", "false");
                }
                serde_json::Value::Object(_) => {
                    self.counter += 1;
                    let add_id = format!("{current_id}:additionalProperties{}", self.counter);
                    let add_pointer = format!("{pointer}/additionalProperties");
                    builder = self.walk(builder, additional, &add_id, &add_pointer)?;
                    builder =
                        builder.edge(holder, &add_id, "prop", Some("additionalProperties"))?;
                }
                // true is the default; no constraint needed for other variants.
                _ => {}
            }
        }

        Ok(builder)
    }
}

/// The vertex for a schema's `type`, and the vertices that carry its
/// object keywords and its array keywords when it admits such instances.
///
/// A `type` list becomes a `union` vertex with one `variant` per listed
/// type, and the keywords go to the variant of their type.
fn type_vertices(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
) -> Result<(SchemaBuilder, Option<String>, Option<String>), ProtocolError> {
    if let Some(serde_json::Value::Array(types)) = schema.get("type") {
        builder = builder.vertex(current_id, "union", None)?;
        let mut object_holder = None;
        let mut array_holder = None;
        for (i, t) in types.iter().enumerate() {
            if let Some(t_str) = t.as_str() {
                let sub_id = format!("{current_id}:type{i}");
                let sub_kind = json_type_to_kind(t_str);
                builder = builder.vertex(&sub_id, &sub_kind, None)?;
                builder = builder.edge(current_id, &sub_id, "variant", Some(t_str))?;
                match sub_kind.as_str() {
                    "object" if object_holder.is_none() => object_holder = Some(sub_id),
                    "array" if array_holder.is_none() => array_holder = Some(sub_id),
                    _ => {}
                }
            }
        }
        return Ok((builder, object_holder, array_holder));
    }
    let type_str = schema
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("object");
    let kind = json_type_to_kind(type_str);
    builder = builder.vertex(current_id, &kind, None)?;
    let own = Some(current_id.to_owned());
    Ok(match kind.as_str() {
        "object" => (builder, own, None),
        "array" => (builder, None, own),
        _ => (builder, None, None),
    })
}

/// Record a schema's validation keywords as constraints on its vertex.
fn constraints(
    mut builder: SchemaBuilder,
    schema: &serde_json::Value,
    current_id: &str,
) -> SchemaBuilder {
    let constraint_fields = [
        "minLength",
        "maxLength",
        "pattern",
        "minimum",
        "maximum",
        "format",
    ];
    for field in &constraint_fields {
        if let Some(val) = schema.get(field) {
            let val_str = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            builder = builder.constraint(current_id, field, &val_str);
        }
    }

    if let Some(enum_val) = schema.get("enum").and_then(serde_json::Value::as_array) {
        let vals: Vec<String> = enum_val
            .iter()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .collect();
        builder = builder.constraint(current_id, "enum", &vals.join(","));
    }

    if let Some(const_val) = schema.get("const") {
        let val_str = match const_val {
            serde_json::Value::String(s) => s.clone(),
            _ => const_val.to_string(),
        };
        builder = builder.constraint(current_id, "const", &val_str);
    }
    builder
}

/// Map JSON Schema type to vertex kind.
fn json_type_to_kind(type_str: &str) -> String {
    match type_str {
        "array" => "array",
        "string" => "string",
        "integer" | "number" => "integer",
        "boolean" => "boolean",
        "null" => "unknown",
        // "object" and all others default to "object".
        _ => "object",
    }
    .to_string()
}

/// Map a vertex kind back to a JSON Schema type keyword.
fn kind_to_json_type(kind: &str) -> &'static str {
    match kind {
        "string" => "string",
        "integer" => "integer",
        "boolean" => "boolean",
        "array" => "array",
        "unknown" => "null",
        _ => "object",
    }
}

/// Emit a [`Schema`] as a JSON Schema document.
///
/// Reconstructs the JSON Schema from the schema graph, including
/// properties, items, type keywords, and validation constraints.
///
/// # Errors
///
/// Returns [`ProtocolError::Emit`] if the schema cannot be serialized.
pub fn emit_json_schema(schema: &Schema) -> Result<serde_json::Value, ProtocolError> {
    // The root vertex is "root".
    let root = schema
        .vertices
        .get("root")
        .ok_or_else(|| ProtocolError::Emit("no root vertex found".into()))?;

    emit_json_schema_vertex(schema, root)
}

/// Emit a single vertex as a JSON Schema value.
fn emit_json_schema_vertex(
    schema: &Schema,
    vertex: &panproto_schema::Vertex,
) -> Result<serde_json::Value, ProtocolError> {
    use crate::emit::{children_by_edge, vertex_constraints};

    let mut obj = serde_json::Map::new();

    // Handle union vertices (type array).
    if vertex.kind == "union" {
        let variants = children_by_edge(schema, &vertex.id, "variant");
        let types: Vec<serde_json::Value> = variants
            .iter()
            .map(|(_edge, v)| {
                let type_str = kind_to_json_type(&v.kind);
                serde_json::json!(type_str)
            })
            .collect();
        if !types.is_empty() {
            obj.insert("type".to_string(), serde_json::Value::Array(types));
        }
        return Ok(serde_json::Value::Object(obj));
    }

    // Set type keyword.
    let json_type = kind_to_json_type(&vertex.kind);
    obj.insert("type".to_string(), serde_json::json!(json_type));

    // Add constraints.
    let constraints = vertex_constraints(schema, &vertex.id);
    for c in &constraints {
        match c.sort.as_str() {
            "type" => {
                // Already handled above.
            }
            "minLength" | "maxLength" | "minimum" | "maximum" => {
                if let Ok(n) = c.value.parse::<i64>() {
                    obj.insert(c.sort.to_string(), serde_json::json!(n));
                } else {
                    obj.insert(c.sort.to_string(), serde_json::json!(c.value));
                }
            }
            "enum" => {
                let vals: Vec<serde_json::Value> = c
                    .value
                    .split(',')
                    .map(|s| serde_json::json!(s.trim()))
                    .collect();
                obj.insert("enum".to_string(), serde_json::Value::Array(vals));
            }
            "const" => {
                obj.insert("const".to_string(), serde_json::json!(c.value));
            }
            "additionalProperties" => {
                if c.value == "false" {
                    obj.insert("additionalProperties".to_string(), serde_json::json!(false));
                }
            }
            _ => {
                obj.insert(c.sort.to_string(), serde_json::json!(c.value));
            }
        }
    }

    // Handle properties (object type).
    let props = children_by_edge(schema, &vertex.id, "prop");
    if !props.is_empty() {
        let mut properties = serde_json::Map::new();
        for (edge, prop_vertex) in &props {
            let prop_name = edge.name.as_deref().unwrap_or(&prop_vertex.id);
            // Skip additionalProperties vertex (handled as constraint).
            if prop_name == "additionalProperties" {
                continue;
            }
            let prop_val = emit_json_schema_vertex(schema, prop_vertex)?;
            properties.insert(prop_name.to_string(), prop_val);
        }
        if !properties.is_empty() {
            obj.insert(
                "properties".to_string(),
                serde_json::Value::Object(properties),
            );
        }
    }

    // Handle items (array type).
    let items = children_by_edge(schema, &vertex.id, "items");
    if let Some((_, items_vertex)) = items.first() {
        let items_val = emit_json_schema_vertex(schema, items_vertex)?;
        obj.insert("items".to_string(), items_val);
    }

    // Handle variant edges for oneOf/anyOf/allOf.
    let variants = children_by_edge(schema, &vertex.id, "variant");
    if !variants.is_empty() {
        // Group variants by their edge name (combiner keyword).
        let mut combiners: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
        for (edge, variant_vertex) in &variants {
            let combiner = edge.name.as_deref().unwrap_or("oneOf");
            let val = emit_json_schema_vertex(schema, variant_vertex)?;
            combiners.entry(combiner.to_string()).or_default().push(val);
        }
        for (key, schemas) in &combiners {
            obj.insert(key.clone(), serde_json::Value::Array(schemas.clone()));
        }
    }

    Ok(serde_json::Value::Object(obj))
}

/// Well-formedness rules for JSON Schema edges.
fn edge_rules() -> Vec<EdgeRule> {
    vec![
        EdgeRule {
            edge_kind: "prop".into(),
            src_kinds: vec!["object".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "items".into(),
            src_kinds: vec!["array".into()],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "variant".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "ref".into(),
            src_kinds: vec![],
            tgt_kinds: vec![],
        },
        EdgeRule {
            edge_kind: "pattern-prop".into(),
            src_kinds: vec!["object".into()],
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
        assert_eq!(p.name, "json-schema");
        assert_eq!(p.schema_theory, "ThJsonSchemaSchema");
    }

    #[test]
    fn register_theories_works() {
        let mut registry = HashMap::new();
        register_theories(&mut registry);
        assert!(registry.contains_key("ThJsonSchemaSchema"));
        assert!(registry.contains_key("ThJsonSchemaInstance"));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn emit_schema_roundtrip() {
        let schema_json = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "maxLength": 100
                },
                "age": {
                    "type": "integer",
                    "minimum": 0
                }
            }
        });

        let schema1 = parse_json_schema(&schema_json).expect("first parse should succeed");
        let emitted = emit_json_schema(&schema1).expect("emit should succeed");
        let schema2 = parse_json_schema(&emitted).expect("re-parse should succeed");

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
    fn parse_simple_json_schema() {
        let schema_json = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "maxLength": 100
                },
                "age": {
                    "type": "integer",
                    "minimum": 0
                }
            }
        });

        let schema = parse_json_schema(&schema_json);
        assert!(
            schema.is_ok(),
            "parse_json_schema should succeed: {schema:?}"
        );
        let schema = schema.ok();
        let schema = schema.as_ref();

        assert!(schema.is_some_and(|s| s.has_vertex("root")));
        assert!(schema.is_some_and(|s| s.has_vertex("root.name")));
        assert!(schema.is_some_and(|s| s.has_vertex("root.age")));
    }
}
