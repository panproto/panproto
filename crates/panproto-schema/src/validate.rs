//! Schema validation against a protocol's rules.
//!
//! The [`validate`] function checks a finished [`Schema`] against a
//! [`Protocol`]'s edge rules, vertex kinds, and constraint sorts,
//! returning a list of all violations found.

use std::collections::{BTreeSet, HashMap, VecDeque};

use panproto_gat::Name;

use crate::error::ValidationError;
use crate::keys::{
    Key, KeyPart, KeyRefError, KeyRole, KeyScope, PathSyntax, RefSource, RefTextSource, Reference,
};
use crate::protocol::Protocol;
use crate::schema::{Edge, Schema};

/// Validate a schema against a protocol's structural rules.
///
/// Checks:
/// 1. All vertex kinds are recognized by the protocol.
/// 2. All edges satisfy the protocol's edge rules.
/// 3. All constraint sorts are recognized by the protocol.
/// 4. All required edges reference vertices that exist.
/// 5. All recursion points name vertices that exist, at both ends.
/// 6. Every key and reference declaration is well formed, each finding
///    reported as [`ValidationError::KeyRef`]: see [`KeyRefError`] for the
///    conditions. Target typing is exact: a reference resolves only through a
///    key declared on its own `target_vertex`, never through one declared on
///    a vertex related to it.
///
/// Returns an empty vector if the schema is valid.
///
/// This is a report rather than a gate: nothing calls it on the way into a
/// search, so a caller loading a schema from bytes has to run it. What it
/// buys is that a dangling reference is named here instead of surfacing later
/// as a part quietly missing from an induced sub-schema.
#[must_use]
pub fn validate(schema: &Schema, protocol: &Protocol) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // 1. Check vertex kinds.
    if !protocol.obj_kinds.is_empty() || !protocol.edge_rules.is_empty() {
        for (id, vertex) in &schema.vertices {
            if !protocol.is_known_vertex_kind(&vertex.kind) {
                errors.push(ValidationError::InvalidVertexKind {
                    vertex: id.to_string(),
                    kind: vertex.kind.to_string(),
                });
            }
        }
    }

    // 2. Check edge rules.
    for edge in schema.edges.keys() {
        if let Some(rule) = protocol.find_edge_rule(&edge.kind) {
            // Check source kind.
            if let Some(src_vertex) = schema.vertices.get(&edge.src) {
                if !rule.src_kinds.is_empty()
                    && !rule.src_kinds.iter().any(|k| k == src_vertex.kind.as_ref())
                {
                    errors.push(ValidationError::InvalidEdge {
                        src: edge.src.to_string(),
                        tgt: edge.tgt.to_string(),
                        kind: edge.kind.to_string(),
                        reason: format!(
                            "source kind '{}' not in permitted: {:?}",
                            src_vertex.kind, rule.src_kinds
                        ),
                    });
                }
            }
            // Check target kind.
            if let Some(tgt_vertex) = schema.vertices.get(&edge.tgt) {
                if !rule.tgt_kinds.is_empty()
                    && !rule.tgt_kinds.iter().any(|k| k == tgt_vertex.kind.as_ref())
                {
                    errors.push(ValidationError::InvalidEdge {
                        src: edge.src.to_string(),
                        tgt: edge.tgt.to_string(),
                        kind: edge.kind.to_string(),
                        reason: format!(
                            "target kind '{}' not in permitted: {:?}",
                            tgt_vertex.kind, rule.tgt_kinds
                        ),
                    });
                }
            }
        } else if !protocol.edge_rules.is_empty() {
            errors.push(ValidationError::InvalidEdge {
                src: edge.src.to_string(),
                tgt: edge.tgt.to_string(),
                kind: edge.kind.to_string(),
                reason: format!("unknown edge kind '{}'", edge.kind),
            });
        }
    }

    // 3. Check constraint sorts.
    if !protocol.constraint_sorts.is_empty() {
        for (vertex_id, constraints) in &schema.constraints {
            for constraint in constraints {
                if !protocol
                    .constraint_sorts
                    .iter()
                    .any(|s| s == constraint.sort.as_ref())
                {
                    errors.push(ValidationError::InvalidConstraintSort {
                        vertex: vertex_id.to_string(),
                        sort: constraint.sort.to_string(),
                    });
                }
            }
        }
    }

    // 4. Check required edges reference existing vertices.
    for (vertex_id, required_edges) in &schema.required {
        for req_edge in required_edges {
            if !schema.vertices.contains_key(&req_edge.src)
                || !schema.vertices.contains_key(&req_edge.tgt)
            {
                errors.push(ValidationError::DanglingRequiredEdge {
                    vertex: vertex_id.to_string(),
                    edge: format!("{} -> {} ({})", req_edge.src, req_edge.tgt, req_edge.kind),
                });
            }
        }
    }

    // 5. Check recursion points name existing vertices at both ends.
    //
    // Inducing a sub-schema keeps a marker only when both ends survive, so a
    // marker naming a vertex that was never there is dropped rather than
    // reported, and the resulting apex looks complete. Reporting it here is
    // what makes the difference visible before a search runs.
    for (mu, point) in &schema.recursion_points {
        if !schema.vertices.contains_key(mu) {
            errors.push(ValidationError::DanglingRecursionPoint {
                mu: mu.to_string(),
                missing: mu.to_string(),
            });
        }
        if !schema.vertices.contains_key(&point.target_vertex) {
            errors.push(ValidationError::DanglingRecursionPoint {
                mu: mu.to_string(),
                missing: point.target_vertex.to_string(),
            });
        }
    }

    // 6. Check key and reference declarations.
    errors.extend(
        declaration_errors(schema, protocol)
            .into_iter()
            .map(ValidationError::KeyRef),
    );

    errors
}

/// Every well-formedness violation of the schema's key and reference
/// declarations, keys first (by vertex, then key), then references (by id).
fn declaration_errors(schema: &Schema, protocol: &Protocol) -> Vec<KeyRefError> {
    let mut errors = Vec::new();
    let ancestry = Ancestry::new(schema);

    for (vertex, keys) in &schema.declarations.keys {
        if !schema.vertices.contains_key(vertex) {
            errors.push(KeyRefError::UnknownVertex {
                vertex: vertex.clone(),
            });
            continue;
        }
        let mut seen: BTreeSet<&Name> = BTreeSet::new();
        let mut repeated: BTreeSet<&Name> = BTreeSet::new();
        for key in keys {
            if !seen.insert(&key.id) && repeated.insert(&key.id) {
                errors.push(KeyRefError::DuplicateKeyId {
                    vertex: vertex.clone(),
                    key: key.id.clone(),
                });
            }
            key_errors(schema, vertex, key, &ancestry, &mut errors);
        }
    }

    let shapes_differ = protocol.syntactic_shape != protocol.semantic_shape;
    for reference in schema.declarations.references.values() {
        reference_errors(schema, reference, shapes_differ, &mut errors);
    }

    errors
}

/// The violations of one key declared on `vertex`.
fn key_errors(
    schema: &Schema,
    vertex: &Name,
    key: &Key,
    ancestry: &Ancestry<'_>,
    errors: &mut Vec<KeyRefError>,
) {
    for part in &key.parts {
        match part {
            KeyPart::Path(path) => check_path(schema, vertex, path, errors),
            KeyPart::Position { within } => {
                if !schema.edges.contains_key(within) || within.tgt != *vertex {
                    errors.push(KeyRefError::PathDoesNotResolve {
                        owner: vertex.clone(),
                        path: vec![within.clone()],
                    });
                } else if !schema.orderings.contains_key(within) {
                    errors.push(KeyRefError::PositionNotOrdered {
                        key: key.id.clone(),
                        edge: within.clone(),
                    });
                }
            }
        }
    }
    if let KeyScope::Within(ancestor) = &key.scope {
        if !schema.vertices.contains_key(ancestor) {
            errors.push(KeyRefError::UnknownVertex {
                vertex: ancestor.clone(),
            });
        } else if !ancestry.is_proper_ancestor(ancestor, vertex) {
            errors.push(KeyRefError::WithinNotAncestor {
                key: key.id.clone(),
                vertex: ancestor.clone(),
            });
        }
    }
}

/// The violations of one reference.
fn reference_errors(
    schema: &Schema,
    reference: &Reference,
    shapes_differ: bool,
    errors: &mut Vec<KeyRefError>,
) {
    let holder_known = schema.vertices.contains_key(&reference.holder);
    let target_known = schema.vertices.contains_key(&reference.target_vertex);
    if !holder_known {
        errors.push(KeyRefError::UnknownVertex {
            vertex: reference.holder.clone(),
        });
    }
    if !target_known {
        errors.push(KeyRefError::UnknownVertex {
            vertex: reference.target_vertex.clone(),
        });
    }
    if !holder_known || !target_known {
        return;
    }

    let declarations = &schema.declarations;
    let target = reference.target_vertex.as_ref();
    if let Some(id) = &reference.target_key {
        if declarations.key(target, id).is_none() {
            errors.push(KeyRefError::UnknownTargetKey {
                reference: reference.id.clone(),
                key: id.clone(),
            });
        }
    }

    for path in source_paths(&reference.source) {
        check_path(schema, &reference.holder, path, errors);
    }
    match &reference.source {
        RefSource::KeyValue { parts } => {
            let unknown_named_key = reference
                .target_key
                .as_ref()
                .is_some_and(|id| declarations.key(target, id).is_none());
            if !unknown_named_key {
                let expected = declarations
                    .resolving_key(reference)
                    .map_or(0, |key| key.parts.len());
                if expected != parts.len() {
                    errors.push(KeyRefError::ArityMismatch {
                        reference: reference.id.clone(),
                        expected,
                        found: parts.len(),
                    });
                }
            }
        }
        RefSource::Variable { binder_key, .. } => match declarations.key(target, binder_key) {
            None => errors.push(KeyRefError::UnknownTargetKey {
                reference: reference.id.clone(),
                key: binder_key.clone(),
            }),
            Some(key) if key.role != KeyRole::Identifier => {
                errors.push(KeyRefError::BinderNotIdentifier {
                    reference: reference.id.clone(),
                    key: binder_key.clone(),
                });
            }
            Some(_) => {}
        },
        RefSource::Path { .. } | RefSource::Standoff { .. } => {}
    }

    if let Some(edge) = &reference.edge {
        if edge.src != reference.holder || edge.tgt != reference.target_vertex {
            errors.push(KeyRefError::EdgeMismatch {
                reference: reference.id.clone(),
                edge: edge.clone(),
            });
        } else if !schema.edges.contains_key(edge) {
            errors.push(KeyRefError::PathDoesNotResolve {
                owner: reference.holder.clone(),
                path: vec![edge.clone()],
            });
        }
    } else if shapes_differ {
        errors.push(KeyRefError::EdgeRequired {
            reference: reference.id.clone(),
        });
    }
}

/// Every edge path a reference source reads, each rooted at the holder.
fn source_paths(source: &RefSource) -> Vec<&[Edge]> {
    match source {
        RefSource::KeyValue { parts } => parts.iter().map(Vec::as_slice).collect(),
        RefSource::Path { field, syntax } => {
            let mut paths = vec![field.as_slice()];
            if let PathSyntax::Uri { base: Some(base) } = syntax {
                paths.push(base.as_slice());
            }
            paths
        }
        RefSource::Standoff {
            text, start, end, ..
        } => {
            let text = match text {
                RefTextSource::Local(path) | RefTextSource::External(path) => path.as_slice(),
            };
            vec![text, start.as_slice(), end.as_slice()]
        }
        RefSource::Variable { field, .. } => vec![field.as_slice()],
    }
}

/// Report `path` unless it is a chain of schema edges starting at `owner`.
///
/// The empty path resolves: it names the owner itself.
fn check_path(schema: &Schema, owner: &Name, path: &[Edge], errors: &mut Vec<KeyRefError>) {
    let mut at = owner;
    for edge in path {
        if edge.src != *at || !schema.edges.contains_key(edge) {
            errors.push(KeyRefError::PathDoesNotResolve {
                owner: owner.clone(),
                path: path.to_vec(),
            });
            return;
        }
        at = &edge.tgt;
    }
}

/// The proper ancestors of vertices along schema edges, computed on demand.
pub struct Ancestry<'a> {
    /// Sources of the edges into each vertex.
    parents: HashMap<&'a Name, Vec<&'a Name>>,
}

impl<'a> Ancestry<'a> {
    /// Index the edges of `schema` by target.
    pub fn new(schema: &'a Schema) -> Self {
        let mut parents: HashMap<&'a Name, Vec<&'a Name>> = HashMap::new();
        for edge in schema.edges.keys() {
            parents.entry(&edge.tgt).or_default().push(&edge.src);
        }
        Self { parents }
    }

    /// Whether a path of one or more edges runs from `ancestor` to `vertex`.
    pub fn is_proper_ancestor(&self, ancestor: &Name, vertex: &Name) -> bool {
        let mut seen: BTreeSet<&Name> = BTreeSet::new();
        let mut queue: VecDeque<&Name> = VecDeque::new();
        queue.push_back(vertex);
        while let Some(at) = queue.pop_front() {
            for parent in self.parents.get(at).map_or(&[][..], Vec::as_slice) {
                if *parent == ancestor {
                    return true;
                }
                if seen.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        false
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builder::SchemaBuilder;
    use crate::protocol::{EdgeRule, Protocol};

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
                        "boolean".to_owned(),
                    ],
                },
            ],
            obj_kinds: vec![
                "record".to_owned(),
                "object".to_owned(),
                "string".to_owned(),
                "integer".to_owned(),
                "boolean".to_owned(),
            ],
            constraint_sorts: vec![
                "maxLength".to_owned(),
                "minLength".to_owned(),
                "format".to_owned(),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn valid_schema_passes() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("post", "record", Some("app.bsky.feed.post"))
            .expect("vertex")
            .vertex("post:body", "object", None)
            .expect("vertex")
            .vertex("post:body.text", "string", None)
            .expect("vertex")
            .edge("post", "post:body", "record-schema", None)
            .expect("edge")
            .edge("post:body", "post:body.text", "prop", Some("text"))
            .expect("edge")
            .constraint("post:body.text", "maxLength", "3000")
            .build()
            .expect("build");

        let errors = validate(&schema, &proto);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    #[test]
    fn invalid_constraint_sort_detected() {
        let proto = atproto_protocol();
        let schema = SchemaBuilder::new(&proto)
            .vertex("v", "string", None)
            .expect("vertex")
            .constraint("v", "nonexistent_sort", "value")
            .build()
            .expect("build");

        let errors = validate(&schema, &proto);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidConstraintSort { .. })),
            "expected InvalidConstraintSort, got: {errors:?}"
        );
    }
}
