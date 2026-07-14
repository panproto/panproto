//! ACSet attribute discipline.
//!
//! An attributed C-set separates *combinatorial* data (nodes and arcs over the
//! schema shape) from *attribute* data (scalar values hanging off objects). An
//! [`AttributeSchema`] records, per vertex, the attributes an instance node
//! over that vertex may carry: an ordered list of `(name, value kind)` pairs
//! derived from the schema's scalar-valued outgoing edges.
//!
//! [`AttributeSchema::validate_wtype`] and
//! [`AttributeSchema::validate_finstance`] check an instance's attribute
//! payload against the declaration, reporting undeclared keys and kind
//! mismatches. The check is opt-in: it is not part of the structural
//! [`validate_wtype`](crate::validate::validate_wtype) pass, since instances
//! routinely carry round-trip fields that no schema edge declares.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::Schema;

use crate::functor::FInstance;
use crate::value::Value;
use crate::wtype::WInstance;

/// A single attribute declaration: an attribute name and the schema kind of
/// the vertex that supplies its values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeDecl {
    /// The attribute name (an edge label, or the target vertex id when the
    /// edge is unlabeled).
    pub name: String,
    /// The schema kind of the attribute's value vertex (e.g. `"string"`).
    pub kind: Name,
}

/// The attribute discipline of a schema: per vertex, the ordered attributes a
/// conforming instance node may carry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttributeSchema {
    /// Per-vertex attribute declarations, each ordered by attribute name.
    pub attrs: HashMap<Name, Vec<AttributeDecl>>,
}

/// A violation found when validating an instance's attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttributeViolation {
    /// An attribute key is present on an instance node but not declared for
    /// that node's vertex.
    Undeclared {
        /// The offending node id (W-type) or row index (F-instance).
        node_id: u32,
        /// The vertex the node sits over.
        anchor: Name,
        /// The undeclared attribute key.
        key: String,
    },
    /// An attribute value's kind does not match the declared kind.
    Mistyped {
        /// The offending node id (W-type) or row index (F-instance).
        node_id: u32,
        /// The vertex the node sits over.
        anchor: Name,
        /// The attribute key.
        key: String,
        /// The declared value kind.
        expected: Name,
        /// The value's actual type name.
        found: String,
    },
}

impl AttributeSchema {
    /// Derive the attribute discipline of a schema.
    ///
    /// For each vertex, its attributes are its outgoing edges whose target is a
    /// *leaf* vertex (one with no outgoing edges of its own). The attribute
    /// name is the edge label, or the target vertex id when the edge is
    /// unlabeled; the value kind is the target vertex's kind. Attributes are
    /// ordered by name; duplicate names keep the first declaration.
    #[must_use]
    pub fn from_schema(schema: &Schema) -> Self {
        let mut attrs: HashMap<Name, Vec<AttributeDecl>> = HashMap::new();
        for vertex_id in schema.vertices.keys() {
            let mut decls: Vec<AttributeDecl> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for edge in schema.outgoing_edges(vertex_id.as_ref()) {
                // Only scalar-valued edges (to leaf vertices) are attributes.
                if !schema.outgoing_edges(edge.tgt.as_ref()).is_empty() {
                    continue;
                }
                let Some(tgt_vertex) = schema.vertex(edge.tgt.as_ref()) else {
                    continue;
                };
                let name = edge
                    .name
                    .as_ref()
                    .map_or_else(|| edge.tgt.to_string(), Name::to_string);
                if !seen.insert(name.clone()) {
                    continue;
                }
                decls.push(AttributeDecl {
                    name,
                    kind: tgt_vertex.kind.clone(),
                });
            }
            if !decls.is_empty() {
                decls.sort_by(|a, b| a.name.cmp(&b.name));
                attrs.insert(vertex_id.clone(), decls);
            }
        }
        Self { attrs }
    }

    /// Look up the declared attributes of a vertex.
    #[must_use]
    pub fn for_vertex(&self, vertex: &str) -> &[AttributeDecl] {
        self.attrs.get(vertex).map_or(&[], Vec::as_slice)
    }

    /// Validate the attribute payload (`extra_fields`) of every node in a
    /// W-type instance against this schema.
    ///
    /// Reports undeclared keys and kind mismatches. Nodes whose anchor has no
    /// declared attributes are unconstrained (any extra fields are round-trip
    /// data, not schema attributes) and skipped.
    #[must_use]
    pub fn validate_wtype(&self, instance: &WInstance) -> Vec<AttributeViolation> {
        let mut violations = Vec::new();
        for node in instance.nodes.values() {
            let Some(decls) = self.attrs.get(&node.anchor) else {
                continue;
            };
            for (key, value) in &node.extra_fields {
                Self::check_one(node.id, &node.anchor, key, value, decls, &mut violations);
            }
        }
        violations
    }

    /// Validate the columns of every row of a functor (relational) instance
    /// against this schema. The reported `node_id` is the row index.
    #[must_use]
    pub fn validate_finstance(&self, instance: &FInstance) -> Vec<AttributeViolation> {
        let mut violations = Vec::new();
        for (table, rows) in &instance.tables {
            let anchor = Name::from(table.as_str());
            let Some(decls) = self.attrs.get(&anchor) else {
                continue;
            };
            for (row_idx, row) in rows.iter().enumerate() {
                let row_id = u32::try_from(row_idx).unwrap_or(u32::MAX);
                for (key, value) in row {
                    Self::check_one(row_id, &anchor, key, value, decls, &mut violations);
                }
            }
        }
        violations
    }

    fn check_one(
        node_id: u32,
        anchor: &Name,
        key: &str,
        value: &Value,
        decls: &[AttributeDecl],
        out: &mut Vec<AttributeViolation>,
    ) {
        match decls.iter().find(|d| d.name == key) {
            None => out.push(AttributeViolation::Undeclared {
                node_id,
                anchor: anchor.clone(),
                key: key.to_owned(),
            }),
            Some(decl) if !kind_accepts(&decl.kind, value) => {
                out.push(AttributeViolation::Mistyped {
                    node_id,
                    anchor: anchor.clone(),
                    key: key.to_owned(),
                    expected: decl.kind.clone(),
                    found: value.type_name().to_owned(),
                });
            }
            Some(_) => {}
        }
    }
}

/// Returns `true` if a value is admissible for a declared scalar kind.
///
/// Recognized primitive kinds are matched structurally; kinds the classifier
/// does not recognize accept any value, and an explicit null is always
/// admissible (it stands for an absent optional attribute).
fn kind_accepts(kind: &Name, value: &Value) -> bool {
    if matches!(value, Value::Null) {
        return true;
    }
    match kind.as_ref() {
        "string" | "str" | "text" => {
            matches!(value, Value::Str(_) | Value::Token(_) | Value::CidLink(_))
        }
        "int" | "integer" => matches!(value, Value::Int(_)),
        "float" | "number" | "double" | "decimal" => {
            matches!(value, Value::Float(_) | Value::Int(_))
        }
        "bool" | "boolean" => matches!(value, Value::Bool(_)),
        "bytes" | "blob" => matches!(value, Value::Bytes(_) | Value::Blob { .. }),
        // Unrecognized kinds cannot be classified, so anything is admissible.
        _ => true,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use panproto_schema::{Edge, Schema, Vertex};

    use super::*;
    use crate::metadata::Node;

    fn attr_schema() -> Schema {
        use smallvec::{SmallVec, smallvec};

        let vertices = [
            ("post", "object"),
            ("post.title", "string"),
            ("post.count", "int"),
        ]
        .into_iter()
        .map(|(id, kind)| {
            (
                Name::from(id),
                Vertex {
                    id: Name::from(id),
                    kind: Name::from(kind),
                    nsid: None,
                },
            )
        })
        .collect();

        let title = Edge {
            src: "post".into(),
            tgt: "post.title".into(),
            kind: "prop".into(),
            name: Some("title".into()),
        };
        let count = Edge {
            src: "post".into(),
            tgt: "post.count".into(),
            kind: "prop".into(),
            name: Some("count".into()),
        };
        let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
        outgoing.insert("post".into(), smallvec![title.clone(), count.clone()]);
        let edges = HashMap::from([(title, Name::from("prop")), (count, Name::from("prop"))]);

        Schema {
            protocol: "test".into(),
            vertices,
            edges,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn derives_ordered_attributes() {
        let attrs = AttributeSchema::from_schema(&attr_schema());
        let post = attrs.for_vertex("post");
        assert_eq!(post.len(), 2);
        // Ordered by name: count before title.
        assert_eq!(post[0].name, "count");
        assert_eq!(post[0].kind.as_ref(), "int");
        assert_eq!(post[1].name, "title");
        assert_eq!(post[1].kind.as_ref(), "string");
    }

    #[test]
    fn accepts_declared_and_well_typed() {
        let attrs = AttributeSchema::from_schema(&attr_schema());
        let mut nodes = HashMap::new();
        nodes.insert(
            0,
            Node::new(0, "post")
                .with_extra_field("title", Value::Str("hello".into()))
                .with_extra_field("count", Value::Int(3)),
        );
        let inst = WInstance::new(nodes, vec![], vec![], 0, "post".into());
        assert!(attrs.validate_wtype(&inst).is_empty());
    }

    #[test]
    fn flags_undeclared_key() {
        let attrs = AttributeSchema::from_schema(&attr_schema());
        let mut nodes = HashMap::new();
        nodes.insert(
            0,
            Node::new(0, "post").with_extra_field("bogus", Value::Int(1)),
        );
        let inst = WInstance::new(nodes, vec![], vec![], 0, "post".into());
        let v = attrs.validate_wtype(&inst);
        assert!(matches!(
            v.as_slice(),
            [AttributeViolation::Undeclared { key, node_id: 0, .. }] if key == "bogus"
        ));
    }

    #[test]
    fn flags_mistyped_value() {
        let attrs = AttributeSchema::from_schema(&attr_schema());
        let mut nodes = HashMap::new();
        nodes.insert(
            0,
            Node::new(0, "post").with_extra_field("count", Value::Str("three".into())),
        );
        let inst = WInstance::new(nodes, vec![], vec![], 0, "post".into());
        let v = attrs.validate_wtype(&inst);
        assert!(matches!(
            v.as_slice(),
            [AttributeViolation::Mistyped { key, expected, .. }]
                if key == "count" && expected.as_ref() == "int"
        ));
    }

    #[test]
    fn validates_finstance_rows() {
        let attrs = AttributeSchema::from_schema(&attr_schema());
        let good = HashMap::from([
            ("title".to_owned(), Value::Str("x".into())),
            ("count".to_owned(), Value::Int(1)),
        ]);
        let bad = HashMap::from([("count".to_owned(), Value::Str("nope".into()))]);
        let inst = FInstance::new().with_table("post", vec![good, bad]);
        let v = attrs.validate_finstance(&inst);
        assert_eq!(v.len(), 1);
        assert!(matches!(
            &v[0],
            AttributeViolation::Mistyped { node_id: 1, key, .. } if key == "count"
        ));
    }
}
