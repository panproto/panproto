//! Schema element attribution: which commit introduced a vertex, edge,
//! or constraint.
//!
//! Walks the DAG backwards from a given commit, checking at each step
//! whether the element exists in that commit's schema but not in its
//! parent's schema (or was modified).

use panproto_schema::Edge;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::Object;
use crate::store::Store;

/// Attribution information for a schema element.
#[derive(Clone, Debug)]
pub struct BlameEntry {
    /// The commit that introduced or last modified this element.
    pub commit_id: ObjectId,
    /// Author of the commit.
    pub author: String,
    /// Timestamp of the commit (Unix seconds).
    pub timestamp: u64,
    /// Commit message.
    pub message: String,
}

/// Find which commit introduced a vertex.
///
/// Walks the first-parent chain from `head` backwards. Returns the
/// earliest commit in which the vertex appears, compared to its parent.
///
/// # Errors
///
/// Returns an error if the vertex is not found in any commit or if
/// loading objects fails.
pub fn blame_vertex(
    store: &dyn Store,
    head: ObjectId,
    vertex_id: &str,
) -> Result<BlameEntry, VcsError> {
    walk_blame(store, head, |schema| {
        schema.vertices.contains_key(vertex_id)
    })
}

/// Find which commit introduced an edge.
///
/// # Errors
///
/// Returns an error if the edge is not found or loading fails.
pub fn blame_edge(store: &dyn Store, head: ObjectId, edge: &Edge) -> Result<BlameEntry, VcsError> {
    walk_blame(store, head, |schema| schema.edges.contains_key(edge))
}

/// Find which commit introduced or last modified a constraint.
///
/// # Errors
///
/// Returns an error if the constraint is not found or loading fails.
pub fn blame_constraint(
    store: &dyn Store,
    head: ObjectId,
    vertex_id: &str,
    sort: &str,
) -> Result<BlameEntry, VcsError> {
    walk_blame(store, head, |schema| {
        schema
            .constraints
            .get(vertex_id)
            .is_some_and(|constraints| constraints.iter().any(|c| c.sort == sort))
    })
}

/// Generic blame walk: find the commit that introduced a schema property.
///
/// `predicate` returns `true` if the element is present in the schema.
/// We walk backwards following first parents, and return the commit where
/// the element first appears (i.e., it's present in this commit but not
/// in its first parent, or this is the root commit).
fn walk_blame(
    store: &dyn Store,
    head: ObjectId,
    predicate: impl Fn(&panproto_schema::Schema) -> bool,
) -> Result<BlameEntry, VcsError> {
    let mut current_id = head;
    let mut last_present: Option<BlameEntry> = None;

    loop {
        let commit = match store.get(&current_id)? {
            Object::Commit(c) => c,
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "commit",
                    found: other.type_name(),
                });
            }
        };

        let schema = match store.get(&commit.schema_id)? {
            Object::Schema(s) => *s,
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "schema",
                    found: other.type_name(),
                });
            }
        };

        if predicate(&schema) {
            last_present = Some(BlameEntry {
                commit_id: current_id,
                author: commit.author.clone(),
                timestamp: commit.timestamp,
                message: commit.message.clone(),
            });
        } else {
            // Element not present — the introducing commit is the
            // one we saved in last_present.
            if let Some(entry) = last_present {
                return Ok(entry);
            }
            // Element was never present.
            return Err(VcsError::RefNotFound {
                name: format!("element not found in commit {}", current_id.short()),
            });
        }

        // Follow first parent.
        if let Some(&parent) = commit.parents.first() {
            current_id = parent;
        } else {
            // Root commit — the element was introduced here.
            if let Some(entry) = last_present {
                return Ok(entry);
            }
            return Err(VcsError::RefNotFound {
                name: format!("element not found in commit {}", current_id.short()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::object::CommitObject;
    use panproto_gat::Name;
    use panproto_schema::{Schema, Vertex};
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)]) -> Schema {
        let mut vert_map = HashMap::new();
        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn blame_vertex_in_root() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let s = make_schema(&[("a", "object")]);
        let schema_id = store.put(&Object::Schema(Box::new(s)))?;
        let commit = CommitObject {
            schema_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "alice".into(),
            timestamp: 100,
            message: "initial".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let commit_id = store.put(&Object::Commit(commit))?;

        let entry = blame_vertex(&store, commit_id, "a")?;
        assert_eq!(entry.commit_id, commit_id);
        assert_eq!(entry.author, "alice");
        Ok(())
    }

    #[test]
    fn blame_vertex_introduced_later() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        // c0: only vertex "a"
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = store.put(&Object::Schema(Box::new(s0)))?;
        let c0 = CommitObject {
            schema_id: s0_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "alice".into(),
            timestamp: 100,
            message: "initial".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: adds vertex "b"
        let s1 = make_schema(&[("a", "object"), ("b", "string")]);
        let s1_id = store.put(&Object::Schema(Box::new(s1)))?;
        let c1 = CommitObject {
            schema_id: s1_id,
            parents: vec![c0_id],
            migration_id: None,
            protocol: "test".into(),
            author: "bob".into(),
            timestamp: 200,
            message: "add b".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let c1_id = store.put(&Object::Commit(c1))?;

        let entry = blame_vertex(&store, c1_id, "b")?;
        assert_eq!(entry.commit_id, c1_id);
        assert_eq!(entry.author, "bob");
        assert_eq!(entry.message, "add b");
        Ok(())
    }

    #[test]
    fn blame_vertex_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let s = make_schema(&[("a", "object")]);
        let schema_id = store.put(&Object::Schema(Box::new(s)))?;
        let commit = CommitObject {
            schema_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "alice".into(),
            timestamp: 100,
            message: "initial".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let commit_id = store.put(&Object::Commit(commit))?;

        assert!(blame_vertex(&store, commit_id, "nonexistent").is_err());
        Ok(())
    }
}
