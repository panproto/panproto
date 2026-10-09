//! Schema element attribution: which commit introduced a vertex, edge,
//! or constraint.
//!
//! Walks the DAG backwards from a given commit, checking at each step
//! whether the element exists in that commit's schema but not in its
//! parent's schema (or was modified). Vertex blame additionally transports
//! the queried vertex backwards through each child commit's migration.

use panproto_mig::Migration;
use panproto_schema::{Edge, Schema};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, Object};
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
/// Walks the first-parent chain from `head` backwards. At each child commit,
/// its stored migration maps source vertices in the first parent to target
/// vertices in the child. A unique source preimage becomes the query for the
/// next step; no preimage means the child introduced the vertex. Commits with
/// no migration metadata use the legacy same-ID comparison.
///
/// # Errors
///
/// Returns an error if the vertex is absent at `head`, object loading or
/// migration validation fails, or a migration has multiple source preimages
/// for the queried target vertex.
pub fn blame_vertex(
    store: &dyn Store,
    head: ObjectId,
    vertex_id: &str,
) -> Result<BlameEntry, VcsError> {
    let mut current_id = head;
    let mut current_vertex = vertex_id.to_owned();

    loop {
        let commit = load_commit(store, current_id)?;
        let schema = crate::tree::resolve_commit_schema_dyn(store, &commit)?;
        if !schema.vertices.contains_key(current_vertex.as_str()) {
            return Err(VcsError::RefNotFound {
                name: format!(
                    "vertex '{current_vertex}' not found in commit {}",
                    current_id.short()
                ),
            });
        }

        let entry = blame_entry(current_id, &commit);
        let Some(&parent_id) = commit.parents.first() else {
            return Ok(entry);
        };
        let parent = load_commit(store, parent_id)?;
        let parent_schema = crate::tree::resolve_commit_schema_dyn(store, &parent)?;

        let Some(migration_id) = commit.migration_id else {
            if parent_schema.vertices.contains_key(current_vertex.as_str()) {
                current_id = parent_id;
                continue;
            }
            return Ok(entry);
        };

        let migration = load_validated_migration(store, migration_id, &parent_schema, &schema)?;
        let mut preimages: Vec<String> = migration
            .vertex_map
            .iter()
            .filter(|(_, target)| target.as_ref() == current_vertex)
            .map(|(source, _)| source.to_string())
            .collect();
        preimages.sort_unstable();

        match preimages.as_slice() {
            [] => return Ok(entry),
            [source] => {
                current_vertex.clone_from(source);
                current_id = parent_id;
            }
            _ => {
                return Err(VcsError::AmbiguousVertexPreimage {
                    vertex: current_vertex,
                    migration_id,
                    preimages,
                });
            }
        }
    }
}

fn load_commit(store: &dyn Store, id: ObjectId) -> Result<CommitObject, VcsError> {
    match store.get(&id)? {
        Object::Commit(commit) => Ok(commit),
        other => Err(VcsError::WrongObjectType {
            expected: "commit",
            found: other.type_name(),
        }),
    }
}

fn load_flat_schema(store: &dyn Store, id: ObjectId) -> Result<Schema, VcsError> {
    match store.get(&id)? {
        Object::FlatSchema(schema) => Ok(*schema),
        other => Err(VcsError::WrongObjectType {
            expected: "flat_schema",
            found: other.type_name(),
        }),
    }
}

/// Load a migration and both of its typed schema endpoints, then validate that
/// it describes the first-parent step being blamed.
fn load_validated_migration(
    store: &dyn Store,
    migration_id: ObjectId,
    parent_schema: &Schema,
    child_schema: &Schema,
) -> Result<Migration, VcsError> {
    let (src_id, tgt_id, migration) = match store.get(&migration_id)? {
        Object::Migration { src, tgt, mapping } => (src, tgt, mapping),
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "migration",
                found: other.type_name(),
            });
        }
    };

    // Loading these through Store::get verifies their content addresses and
    // rejects a migration whose endpoint IDs name another object kind.
    let source = load_flat_schema(store, src_id)?;
    let target = load_flat_schema(store, tgt_id)?;
    let expected_src = crate::hash::hash_schema(parent_schema)?;
    let expected_tgt = crate::hash::hash_schema(child_schema)?;
    if src_id != expected_src || tgt_id != expected_tgt {
        return Err(VcsError::ValidationFailed {
            reasons: vec![format!(
                "migration {migration_id} endpoints do not match first-parent step: \
                 expected {expected_src} -> {expected_tgt}, found {src_id} -> {tgt_id}"
            )],
        });
    }

    let diagnostics = crate::gat_validate::validate_migration(&source, &target, &migration);
    if diagnostics.has_errors() {
        return Err(VcsError::ValidationFailed {
            reasons: diagnostics.all_errors(),
        });
    }
    Ok(migration)
}

fn blame_entry(commit_id: ObjectId, commit: &CommitObject) -> BlameEntry {
    BlameEntry {
        commit_id,
        author: commit.author.clone(),
        timestamp: commit.timestamp,
        message: commit.message.clone(),
    }
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

        let schema = {
            let proto = crate::tree::project_coproduct_protocol();
            crate::tree::assemble_schema_dyn(store, &commit.schema_id, &proto)?
        };

        if predicate(&schema) {
            last_present = Some(blame_entry(current_id, &commit));
        } else {
            // Element not present: the introducing commit is the
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
            // Root commit: the element was introduced here.
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
    use panproto_schema::{Constraint, Schema, SchemaBuilder, Vertex};
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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    fn store_root(
        store: &mut MemStore,
        schema: &Schema,
        author: &str,
        message: &str,
        timestamp: u64,
    ) -> Result<ObjectId, VcsError> {
        let schema_id = crate::tree::store_schema_as_tree(store, schema.clone())?;
        let commit = CommitObject::builder(schema_id, "test", author, message)
            .timestamp(timestamp)
            .build();
        store.put(&Object::Commit(commit))
    }

    fn store_child(
        store: &mut MemStore,
        parent_id: ObjectId,
        parent_schema: &Schema,
        child_schema: &Schema,
        vertex_map: Option<&[(&str, &str)]>,
        message: &str,
        timestamp: u64,
    ) -> Result<ObjectId, VcsError> {
        let schema_id = crate::tree::store_schema_as_tree(store, child_schema.clone())?;
        let mut builder = CommitObject::builder(schema_id, "test", "bob", message)
            .parents(vec![parent_id])
            .timestamp(timestamp);

        if let Some(vertex_map) = vertex_map {
            let src = store.put(&Object::FlatSchema(Box::new(parent_schema.clone())))?;
            let tgt = store.put(&Object::FlatSchema(Box::new(child_schema.clone())))?;
            let mut migration = Migration::empty();
            for &(source, target) in vertex_map {
                migration
                    .vertex_map
                    .insert(Name::from(source), Name::from(target));
            }
            migration = migration.with_endpoints(
                Some(Name::from(src.to_string())),
                Some(Name::from(tgt.to_string())),
            );
            let migration_id = store.put(&Object::Migration {
                src,
                tgt,
                mapping: migration,
            })?;
            builder = builder.migration_id(migration_id);
        }

        store.put(&Object::Commit(builder.build()))
    }

    #[test]
    fn blame_vertex_in_root() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let s = make_schema(&[("a", "object")]);
        let schema_id = crate::tree::store_schema_as_tree(&mut store, s)?;
        let commit = CommitObject::builder(schema_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
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
        let s0_id = crate::tree::store_schema_as_tree(&mut store, s0)?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: adds vertex "b"
        let s1 = make_schema(&[("a", "object"), ("b", "string")]);
        let s1_id = crate::tree::store_schema_as_tree(&mut store, s1)?;
        let c1 = CommitObject::builder(s1_id, "test", "bob", "add b")
            .parents(vec![c0_id])
            .timestamp(200)
            .build();
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
        let schema_id = crate::tree::store_schema_as_tree(&mut store, s)?;
        let commit = CommitObject::builder(schema_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let commit_id = store.put(&Object::Commit(commit))?;

        assert!(blame_vertex(&store, commit_id, "nonexistent").is_err());
        Ok(())
    }

    #[test]
    fn blame_vertex_follows_one_rename() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let source = make_schema(&[("old", "object")]);
        let root_id = store_root(&mut store, &source, "alice", "initial", 100)?;
        let target = make_schema(&[("new", "object")]);
        let child_id = store_child(
            &mut store,
            root_id,
            &source,
            &target,
            Some(&[("old", "new")]),
            "rename old to new",
            200,
        )?;

        let entry = blame_vertex(&store, child_id, "new")?;
        assert_eq!(entry.commit_id, root_id);
        assert_eq!(entry.author, "alice");
        Ok(())
    }

    #[test]
    fn blame_vertex_follows_a_rename_chain() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let first = make_schema(&[("a", "object")]);
        let first_id = store_root(&mut store, &first, "alice", "initial", 100)?;
        let second = make_schema(&[("b", "object")]);
        let second_id = store_child(
            &mut store,
            first_id,
            &first,
            &second,
            Some(&[("a", "b")]),
            "rename a to b",
            200,
        )?;
        let third = make_schema(&[("c", "object")]);
        let third_id = store_child(
            &mut store,
            second_id,
            &second,
            &third,
            Some(&[("b", "c")]),
            "rename b to c",
            300,
        )?;

        assert_eq!(blame_vertex(&store, third_id, "c")?.commit_id, first_id);
        Ok(())
    }

    #[test]
    fn blame_vertex_stops_when_migration_has_no_preimage() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let child = make_schema(&[("a", "object"), ("new", "string")]);
        let child_id = store_child(
            &mut store,
            parent_id,
            &parent,
            &child,
            Some(&[("a", "a")]),
            "add new",
            200,
        )?;

        assert_eq!(blame_vertex(&store, child_id, "new")?.commit_id, child_id);
        Ok(())
    }

    #[test]
    fn blame_vertex_without_migration_metadata_keeps_same_id_behavior()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let child = make_schema(&[("a", "object")]);
        let child_id = store_child(
            &mut store,
            parent_id,
            &parent,
            &child,
            None,
            "legacy child",
            200,
        )?;

        assert_eq!(blame_vertex(&store, child_id, "a")?.commit_id, parent_id);
        Ok(())
    }

    #[test]
    fn blame_vertex_reports_sorted_ambiguous_preimages() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let parent = make_schema(&[("z", "object"), ("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let child = make_schema(&[("joined", "object")]);
        let child_id = store_child(
            &mut store,
            parent_id,
            &parent,
            &child,
            Some(&[("z", "joined"), ("a", "joined")]),
            "contract",
            200,
        )?;

        let Err(error) = blame_vertex(&store, child_id, "joined") else {
            panic!("expected ambiguous blame");
        };
        assert!(matches!(
            error,
            VcsError::AmbiguousVertexPreimage {
                vertex,
                preimages,
                ..
            } if vertex == "joined" && preimages == ["a", "z"]
        ));
        Ok(())
    }

    #[test]
    fn blame_vertex_rejects_a_wrongly_typed_migration_reference()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let child = make_schema(&[("a", "object")]);
        let child_schema_id = crate::tree::store_schema_as_tree(&mut store, child)?;
        let child_commit = CommitObject::builder(child_schema_id, "test", "bob", "bad migration")
            .parents(vec![parent_id])
            .migration_id(child_schema_id)
            .timestamp(200)
            .build();
        let child_id = store.put(&Object::Commit(child_commit))?;

        let Err(error) = blame_vertex(&store, child_id, "a") else {
            panic!("expected wrong object type");
        };
        assert!(matches!(
            error,
            VcsError::WrongObjectType {
                expected: "migration",
                found: "schema_tree"
            }
        ));
        Ok(())
    }

    #[test]
    fn blame_vertex_rejects_a_wrongly_typed_migration_endpoint()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let parent_tree_id = match store.get(&parent_id)? {
            Object::Commit(commit) => commit.schema_id,
            other => panic!("expected commit, got {}", other.type_name()),
        };
        let child = make_schema(&[("a", "object")]);
        let child_schema_id = crate::tree::store_schema_as_tree(&mut store, child.clone())?;
        let target_id = store.put(&Object::FlatSchema(Box::new(child)))?;
        let mut migration = Migration::empty();
        migration
            .vertex_map
            .insert(Name::from("a"), Name::from("a"));
        let migration_id = store.put(&Object::Migration {
            src: parent_tree_id,
            tgt: target_id,
            mapping: migration,
        })?;
        let child_commit = CommitObject::builder(child_schema_id, "test", "bob", "bad endpoint")
            .parents(vec![parent_id])
            .migration_id(migration_id)
            .timestamp(200)
            .build();
        let child_id = store.put(&Object::Commit(child_commit))?;

        let Err(error) = blame_vertex(&store, child_id, "a") else {
            panic!("expected wrong endpoint type");
        };
        assert!(matches!(
            error,
            VcsError::WrongObjectType {
                expected: "flat_schema",
                found: "schema_tree"
            }
        ));
        Ok(())
    }

    #[test]
    fn blame_vertex_reports_a_missing_migration_endpoint() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let child = make_schema(&[("a", "object")]);
        let child_schema_id = crate::tree::store_schema_as_tree(&mut store, child.clone())?;
        let target_id = store.put(&Object::FlatSchema(Box::new(child)))?;
        let missing_id = ObjectId::from_bytes([0x42; 32]);
        let migration_id = store.put(&Object::Migration {
            src: missing_id,
            tgt: target_id,
            mapping: Migration::empty(),
        })?;
        let child_commit =
            CommitObject::builder(child_schema_id, "test", "bob", "missing endpoint")
                .parents(vec![parent_id])
                .migration_id(migration_id)
                .timestamp(200)
                .build();
        let child_id = store.put(&Object::Commit(child_commit))?;

        let Err(error) = blame_vertex(&store, child_id, "a") else {
            panic!("expected missing endpoint");
        };
        assert!(matches!(error, VcsError::ObjectNotFound { id } if id == missing_id));
        Ok(())
    }

    #[test]
    fn blame_vertex_rejects_migration_endpoints_for_another_schema_pair()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let child = make_schema(&[("a", "object")]);
        let child_schema_id = crate::tree::store_schema_as_tree(&mut store, child.clone())?;
        let unrelated = make_schema(&[("other", "object")]);
        let source_id = store.put(&Object::FlatSchema(Box::new(unrelated)))?;
        let target_id = store.put(&Object::FlatSchema(Box::new(child)))?;
        let mut migration = Migration::empty();
        migration
            .vertex_map
            .insert(Name::from("other"), Name::from("a"));
        let migration_id = store.put(&Object::Migration {
            src: source_id,
            tgt: target_id,
            mapping: migration,
        })?;
        let child_commit =
            CommitObject::builder(child_schema_id, "test", "bob", "mismatched endpoints")
                .parents(vec![parent_id])
                .migration_id(migration_id)
                .timestamp(200)
                .build();
        let child_id = store.put(&Object::Commit(child_commit))?;

        let Err(error) = blame_vertex(&store, child_id, "a") else {
            panic!("expected endpoint mismatch");
        };
        assert!(matches!(
            error,
            VcsError::ValidationFailed { reasons }
                if reasons.iter().any(|reason| reason.contains("endpoints do not match"))
        ));
        Ok(())
    }

    #[test]
    fn repository_created_rename_blames_the_original_commit()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = crate::tree::project_coproduct_protocol();
        let old = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)?
            .vertex("root.text", "string", None)?
            .edge("root", "root.text", "prop", Some("text"))?
            .build()?;
        let new = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)?
            .vertex("root.body", "string", None)?
            .edge("root", "root.body", "prop", Some("body"))?
            .build()?;

        let dir = tempfile::tempdir()?;
        let mut repo = crate::Repository::init(dir.path())?;
        repo.add(&old)?;
        let original = repo.commit("initial", "alice")?;
        repo.add(&new)?;
        let renamed = repo.commit("rename text to body", "bob")?;

        assert_eq!(
            blame_vertex(repo.store(), renamed, "root.body")?.commit_id,
            original
        );
        Ok(())
    }

    #[test]
    fn edge_and_constraint_blame_keep_legacy_behavior() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let parent = make_schema(&[("a", "object"), ("b", "string")]);
        let parent_id = store_root(&mut store, &parent, "alice", "initial", 100)?;
        let mut child = parent.clone();
        let edge = Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: Some(Name::from("b")),
        };
        child.edges.insert(edge.clone(), Name::from("prop"));
        child.constraints.insert(
            Name::from("b"),
            vec![Constraint {
                sort: Name::from("maxLength"),
                value: "10".to_owned(),
            }],
        );
        let child_id = store_child(
            &mut store,
            parent_id,
            &parent,
            &child,
            None,
            "add edge and constraint",
            200,
        )?;

        assert_eq!(blame_edge(&store, child_id, &edge)?.commit_id, child_id);
        assert_eq!(
            blame_constraint(&store, child_id, "b", "maxLength")?.commit_id,
            child_id
        );
        Ok(())
    }
}
