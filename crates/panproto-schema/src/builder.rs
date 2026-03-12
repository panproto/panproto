//! Incremental schema construction with protocol-aware validation.
//!
//! [`SchemaBuilder`] provides a fluent API for constructing a [`Schema`].
//! Each `vertex()` and `edge()` call validates against the [`Protocol`]'s
//! edge rules before accepting the element. The final `build()` call
//! computes adjacency indices and returns the finished schema.

use std::collections::HashMap;

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::error::SchemaError;
use crate::protocol::Protocol;
use crate::schema::{Constraint, Edge, HyperEdge, Schema, Vertex};

/// A builder for incrementally constructing a validated [`Schema`].
///
/// # Example
///
/// ```ignore
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("post", "record", Some("app.bsky.feed.post"))?
///     .vertex("post:body", "object", None)?
///     .edge("post", "post:body", "record-schema", None)?
///     .build()?;
/// ```
pub struct SchemaBuilder {
    protocol: Protocol,
    vertices: HashMap<String, Vertex>,
    edges: Vec<Edge>,
    hyper_edges: HashMap<String, HyperEdge>,
    constraints: HashMap<String, Vec<Constraint>>,
    required: HashMap<String, Vec<Edge>>,
    nsids: HashMap<String, String>,
    edge_set: FxHashSet<(String, String, String, Option<String>)>,
}

impl SchemaBuilder {
    /// Create a new builder for the given protocol.
    #[must_use]
    pub fn new(protocol: &Protocol) -> Self {
        Self {
            protocol: protocol.clone(),
            vertices: HashMap::new(),
            edges: Vec::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            edge_set: FxHashSet::default(),
        }
    }

    /// Add a vertex to the schema.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::DuplicateVertex`] if a vertex with the same ID
    /// already exists, or [`SchemaError::UnknownVertexKind`] if the kind is
    /// not recognized by the protocol.
    pub fn vertex(mut self, id: &str, kind: &str, nsid: Option<&str>) -> Result<Self, SchemaError> {
        if self.vertices.contains_key(id) {
            return Err(SchemaError::DuplicateVertex(id.to_owned()));
        }

        // Validate vertex kind against the protocol if the protocol
        // has any known kinds at all. If no kinds are declared,
        // we allow anything (open protocol).
        if (!self.protocol.obj_kinds.is_empty() || !self.protocol.edge_rules.is_empty())
            && !self.protocol.is_known_vertex_kind(kind)
        {
            return Err(SchemaError::UnknownVertexKind(kind.to_owned()));
        }

        let vertex = Vertex {
            id: id.to_owned(),
            kind: kind.to_owned(),
            nsid: nsid.map(str::to_owned),
        };

        if let Some(nsid_val) = nsid {
            self.nsids.insert(id.to_owned(), nsid_val.to_owned());
        }

        self.vertices.insert(id.to_owned(), vertex);
        Ok(self)
    }

    /// Add a binary edge to the schema.
    ///
    /// Validates that:
    /// - Both `src` and `tgt` vertices exist
    /// - The edge kind is recognized by the protocol
    /// - The source and target vertex kinds satisfy the edge rule
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::VertexNotFound`], [`SchemaError::UnknownEdgeKind`],
    /// [`SchemaError::InvalidEdgeSource`], or [`SchemaError::InvalidEdgeTarget`].
    pub fn edge(
        mut self,
        src: &str,
        tgt: &str,
        kind: &str,
        name: Option<&str>,
    ) -> Result<Self, SchemaError> {
        let src_vertex = self
            .vertices
            .get(src)
            .ok_or_else(|| SchemaError::VertexNotFound(src.to_owned()))?;
        let tgt_vertex = self
            .vertices
            .get(tgt)
            .ok_or_else(|| SchemaError::VertexNotFound(tgt.to_owned()))?;

        // Validate against edge rules (if any rules are defined).
        if let Some(rule) = self.protocol.find_edge_rule(kind) {
            // Check source kind constraint.
            if !rule.src_kinds.is_empty() && !rule.src_kinds.iter().any(|k| k == &src_vertex.kind) {
                return Err(SchemaError::InvalidEdgeSource {
                    kind: kind.to_owned(),
                    src_kind: src_vertex.kind.clone(),
                    permitted: rule.src_kinds.join(", "),
                });
            }
            // Check target kind constraint.
            if !rule.tgt_kinds.is_empty() && !rule.tgt_kinds.iter().any(|k| k == &tgt_vertex.kind) {
                return Err(SchemaError::InvalidEdgeTarget {
                    kind: kind.to_owned(),
                    tgt_kind: tgt_vertex.kind.clone(),
                    permitted: rule.tgt_kinds.join(", "),
                });
            }
        } else if !self.protocol.edge_rules.is_empty() {
            // The protocol has rules but none matches this edge kind.
            return Err(SchemaError::UnknownEdgeKind(kind.to_owned()));
        }

        let edge_key = (
            src.to_owned(),
            tgt.to_owned(),
            kind.to_owned(),
            name.map(str::to_owned),
        );
        if !self.edge_set.insert(edge_key) {
            return Err(SchemaError::DuplicateEdge {
                src: src.to_owned(),
                tgt: tgt.to_owned(),
                kind: kind.to_owned(),
            });
        }

        let edge = Edge {
            src: src.to_owned(),
            tgt: tgt.to_owned(),
            kind: kind.to_owned(),
            name: name.map(str::to_owned),
        };
        self.edges.push(edge);
        Ok(self)
    }

    /// Add a hyper-edge to the schema.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::DuplicateHyperEdge`] if a hyper-edge with the
    /// same ID already exists, or [`SchemaError::VertexNotFound`] if any
    /// vertex in the signature is missing.
    pub fn hyper_edge(
        mut self,
        id: &str,
        kind: &str,
        sig: HashMap<String, String>,
        parent: &str,
    ) -> Result<Self, SchemaError> {
        if self.hyper_edges.contains_key(id) {
            return Err(SchemaError::DuplicateHyperEdge(id.to_owned()));
        }

        // Validate all vertices in signature exist.
        for (label, vertex_id) in &sig {
            if !self.vertices.contains_key(vertex_id) {
                return Err(SchemaError::VertexNotFound(format!(
                    "{vertex_id} (in hyper-edge {id}, label {label})"
                )));
            }
        }

        let hyper_edge = HyperEdge {
            id: id.to_owned(),
            kind: kind.to_owned(),
            signature: sig,
            parent_label: parent.to_owned(),
        };
        self.hyper_edges.insert(id.to_owned(), hyper_edge);
        Ok(self)
    }

    /// Add a constraint to a vertex.
    ///
    /// Constraints are not validated during building; use [`validate`](crate::validate)
    /// to check them against the protocol's constraint sorts.
    #[must_use]
    pub fn constraint(mut self, vertex: &str, sort: &str, value: &str) -> Self {
        self.constraints
            .entry(vertex.to_owned())
            .or_default()
            .push(Constraint {
                sort: sort.to_owned(),
                value: value.to_owned(),
            });
        self
    }

    /// Declare required edges for a vertex.
    #[must_use]
    pub fn required(mut self, vertex: &str, edges: Vec<Edge>) -> Self {
        self.required
            .entry(vertex.to_owned())
            .or_default()
            .extend(edges);
        self
    }

    /// Consume the builder and produce a validated [`Schema`] with
    /// precomputed adjacency indices.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::EmptySchema`] if no vertices were added.
    pub fn build(self) -> Result<Schema, SchemaError> {
        if self.vertices.is_empty() {
            return Err(SchemaError::EmptySchema);
        }

        // Build edge map.
        let mut edge_map: HashMap<Edge, String> = HashMap::with_capacity(self.edges.len());
        let mut outgoing: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<String, SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(String, String), SmallVec<Edge, 2>> = HashMap::new();

        for edge in &self.edges {
            edge_map.insert(edge.clone(), edge.kind.clone());

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

        Ok(Schema {
            protocol: self.protocol.name.clone(),
            vertices: self.vertices,
            edges: edge_map,
            hyper_edges: self.hyper_edges,
            constraints: self.constraints,
            required: self.required,
            nsids: self.nsids,
            outgoing,
            incoming,
            between,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::protocol::EdgeRule;

    /// Build a minimal ATProto-like protocol for testing.
    fn atproto_protocol() -> Protocol {
        Protocol {
            name: "atproto".to_owned(),
            schema_theory: "ThATProtoSchema".to_owned(),
            instance_theory: "ThWType".to_owned(),
            edge_rules: vec![
                EdgeRule {
                    edge_kind: "record-schema".to_owned(),
                    src_kinds: vec!["record".to_owned()],
                    tgt_kinds: vec!["object".to_owned()],
                },
                EdgeRule {
                    edge_kind: "prop".to_owned(),
                    src_kinds: vec!["object".to_owned()],
                    tgt_kinds: vec![
                        "string".to_owned(),
                        "integer".to_owned(),
                        "object".to_owned(),
                        "ref".to_owned(),
                        "array".to_owned(),
                        "union".to_owned(),
                        "boolean".to_owned(),
                    ],
                },
            ],
            obj_kinds: vec![
                "record".to_owned(),
                "object".to_owned(),
                "string".to_owned(),
                "integer".to_owned(),
                "ref".to_owned(),
                "array".to_owned(),
                "union".to_owned(),
                "boolean".to_owned(),
            ],
            constraint_sorts: vec![
                "maxLength".to_owned(),
                "minLength".to_owned(),
                "format".to_owned(),
                "minimum".to_owned(),
                "maximum".to_owned(),
            ],
        }
    }

    #[test]
    fn build_atproto_schema() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("post", "record", Some("app.bsky.feed.post"))
            .expect("vertex post")
            .vertex("post:body", "object", None)
            .expect("vertex body")
            .vertex("post:body.text", "string", None)
            .expect("vertex text")
            .edge("post", "post:body", "record-schema", None)
            .expect("edge record-schema")
            .edge("post:body", "post:body.text", "prop", Some("text"))
            .expect("edge prop")
            .constraint("post:body.text", "maxLength", "3000")
            .build()
            .expect("build");

        assert_eq!(schema.vertex_count(), 3);
        assert_eq!(schema.edge_count(), 2);
        assert_eq!(schema.outgoing_edges("post").len(), 1);
        assert_eq!(schema.incoming_edges("post:body").len(), 1);
        assert_eq!(
            schema.nsids.get("post").map(String::as_str),
            Some("app.bsky.feed.post")
        );
        assert_eq!(
            schema.constraints.get("post:body.text").map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn invalid_edge_rejected() {
        let proto = atproto_protocol();
        // Attempt to add a record-schema edge from string to integer (should fail).
        let result = SchemaBuilder::new(&proto)
            .vertex("s", "string", None)
            .expect("vertex string")
            .vertex("i", "integer", None)
            .expect("vertex integer")
            .edge("s", "i", "record-schema", None);

        assert!(
            matches!(result, Err(SchemaError::InvalidEdgeSource { .. })),
            "expected InvalidEdgeSource"
        );
    }

    #[test]
    fn duplicate_vertex_rejected() {
        let proto = atproto_protocol();
        let result = SchemaBuilder::new(&proto)
            .vertex("v", "record", None)
            .expect("first vertex")
            .vertex("v", "record", None);

        assert!(
            matches!(result, Err(SchemaError::DuplicateVertex(_))),
            "expected DuplicateVertex"
        );
    }

    #[test]
    fn edge_to_missing_vertex_rejected() {
        let proto = atproto_protocol();
        let result = SchemaBuilder::new(&proto)
            .vertex("a", "record", None)
            .expect("vertex a")
            .edge("a", "missing", "record-schema", None);

        assert!(
            matches!(result, Err(SchemaError::VertexNotFound(_))),
            "expected VertexNotFound"
        );
    }

    #[test]
    fn empty_schema_rejected() {
        let proto = atproto_protocol();
        let result = SchemaBuilder::new(&proto).build();
        assert!(
            matches!(result, Err(SchemaError::EmptySchema)),
            "expected EmptySchema"
        );
    }

    #[test]
    fn between_index_works() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("r", "record", None)
            .expect("vertex r")
            .vertex("o", "object", None)
            .expect("vertex o")
            .edge("r", "o", "record-schema", None)
            .expect("edge")
            .build()
            .expect("build");

        assert_eq!(schema.edges_between("r", "o").len(), 1);
        assert_eq!(schema.edges_between("o", "r").len(), 0);
    }
}
