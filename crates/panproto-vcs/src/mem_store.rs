//! In-memory store implementation for testing and WASM.

use std::collections::HashMap;

use crate::error::VcsError;
use crate::hash::{self, ObjectId};
use crate::object::Object;
use crate::store::{HeadState, ReflogEntry, Store};

/// An in-memory [`Store`] backed by `HashMap`s.
///
/// Useful for unit tests and WASM environments where filesystem access
/// is unavailable.
#[derive(Debug)]
pub struct MemStore {
    objects: HashMap<ObjectId, Object>,
    refs: HashMap<String, ObjectId>,
    head: HeadState,
    reflogs: HashMap<String, Vec<ReflogEntry>>,
}

impl MemStore {
    /// Create a new empty store with HEAD pointing to `main`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            objects: HashMap::new(),
            refs: HashMap::new(),
            head: HeadState::Branch("main".into()),
            reflogs: HashMap::new(),
        }
    }
}

impl Default for MemStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for MemStore {
    fn has(&self, id: &ObjectId) -> bool {
        self.objects.contains_key(id)
    }

    fn get(&self, id: &ObjectId) -> Result<Object, VcsError> {
        self.objects
            .get(id)
            .cloned()
            .ok_or(VcsError::ObjectNotFound { id: *id })
    }

    fn put(&mut self, object: &Object) -> Result<ObjectId, VcsError> {
        let id = compute_object_id(object)?;
        self.objects.entry(id).or_insert_with(|| object.clone());
        Ok(id)
    }

    fn get_ref(&self, name: &str) -> Result<Option<ObjectId>, VcsError> {
        Ok(self.refs.get(name).copied())
    }

    fn set_ref(&mut self, name: &str, id: ObjectId) -> Result<(), VcsError> {
        self.refs.insert(name.to_owned(), id);
        Ok(())
    }

    fn delete_ref(&mut self, name: &str) -> Result<(), VcsError> {
        self.refs
            .remove(name)
            .ok_or_else(|| VcsError::RefNotFound {
                name: name.to_owned(),
            })?;
        Ok(())
    }

    fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let mut result: Vec<(String, ObjectId)> = self
            .refs
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    fn get_head(&self) -> Result<HeadState, VcsError> {
        Ok(self.head.clone())
    }

    fn set_head(&mut self, state: HeadState) -> Result<(), VcsError> {
        self.head = state;
        Ok(())
    }

    fn list_objects(&self) -> Result<Vec<ObjectId>, VcsError> {
        Ok(self.objects.keys().copied().collect())
    }

    fn delete_object(&mut self, id: &ObjectId) -> Result<(), VcsError> {
        self.objects
            .remove(id)
            .ok_or(VcsError::ObjectNotFound { id: *id })?;
        Ok(())
    }

    fn append_reflog(&mut self, ref_name: &str, entry: ReflogEntry) -> Result<(), VcsError> {
        self.reflogs
            .entry(ref_name.to_owned())
            .or_default()
            .push(entry);
        Ok(())
    }

    fn read_reflog(
        &self,
        ref_name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ReflogEntry>, VcsError> {
        let entries = self.reflogs.get(ref_name).cloned().unwrap_or_default();
        // Return newest first.
        let mut reversed: Vec<ReflogEntry> = entries.into_iter().rev().collect();
        if let Some(n) = limit {
            reversed.truncate(n);
        }
        Ok(reversed)
    }
}

/// Compute the `ObjectId` for any [`Object`].
fn compute_object_id(object: &Object) -> Result<ObjectId, VcsError> {
    match object {
        Object::Schema(schema) => hash::hash_schema(schema),
        Object::Migration { src, tgt, mapping } => hash::hash_migration(*src, *tgt, mapping),
        Object::Commit(commit) => hash::hash_commit(commit),
        Object::Tag(tag) => hash::hash_tag(tag),
        Object::DataSet(dataset) => hash::hash_dataset(dataset),
        Object::Complement(complement) => hash::hash_complement(complement),
        Object::Protocol(protocol) => hash::hash_protocol(protocol),
        Object::Expr(expr) => hash::hash_expr(expr),
        Object::EditLog(edit_log) => hash::hash_edit_log(edit_log),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::VcsError;
    use panproto_schema::{Schema, Vertex};
    use std::collections::HashMap;

    fn test_schema() -> Schema {
        use panproto_gat::Name;
        let mut vertices = HashMap::new();
        vertices.insert(
            Name::from("root"),
            Vertex {
                id: Name::from("root"),
                kind: Name::from("object"),
                nsid: None,
            },
        );
        Schema {
            protocol: "test".into(),
            vertices,
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
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn put_get_round_trip() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let schema = test_schema();
        let obj = Object::Schema(Box::new(schema.clone()));
        let id = store.put(&obj)?;

        assert!(store.has(&id));

        let retrieved = store.get(&id)?;
        match retrieved {
            Object::Schema(s) => assert_eq!(s.protocol, schema.protocol),
            _ => panic!("expected Schema object"),
        }
        Ok(())
    }

    #[test]
    fn put_idempotent() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let obj = Object::Schema(Box::new(test_schema()));
        let id1 = store.put(&obj)?;
        let id2 = store.put(&obj)?;
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn get_nonexistent_returns_error() {
        let store = MemStore::new();
        let result = store.get(&ObjectId::ZERO);
        assert!(result.is_err());
    }

    #[test]
    fn ref_operations() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([42; 32]);

        // Set and get.
        store.set_ref("refs/heads/main", id)?;
        assert_eq!(store.get_ref("refs/heads/main")?, Some(id));

        // List.
        let refs = store.list_refs("refs/heads/")?;
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "refs/heads/main");

        // Delete.
        store.delete_ref("refs/heads/main")?;
        assert_eq!(store.get_ref("refs/heads/main")?, None);
        Ok(())
    }

    #[test]
    fn head_state() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        assert_eq!(store.get_head()?, HeadState::Branch("main".into()));

        let id = ObjectId::from_bytes([1; 32]);
        store.set_head(HeadState::Detached(id))?;
        assert_eq!(store.get_head()?, HeadState::Detached(id));
        Ok(())
    }

    #[test]
    fn reflog_append_and_read() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let entry1 = ReflogEntry {
            old_id: None,
            new_id: ObjectId::from_bytes([1; 32]),
            author: "test".into(),
            timestamp: 100,
            message: "first".into(),
        };
        let entry2 = ReflogEntry {
            old_id: Some(ObjectId::from_bytes([1; 32])),
            new_id: ObjectId::from_bytes([2; 32]),
            author: "test".into(),
            timestamp: 200,
            message: "second".into(),
        };

        store.append_reflog("HEAD", entry1)?;
        store.append_reflog("HEAD", entry2)?;

        let log = store.read_reflog("HEAD", None)?;
        assert_eq!(log.len(), 2);
        // Newest first.
        assert_eq!(log[0].message, "second");
        assert_eq!(log[1].message, "first");

        // With limit.
        let log = store.read_reflog("HEAD", Some(1))?;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].message, "second");
        Ok(())
    }

    #[test]
    fn resolve_head_empty_repo() -> Result<(), VcsError> {
        let store = MemStore::new();
        // HEAD points to main, but main has no commits.
        assert_eq!(crate::store::resolve_head(&store)?, None);
        Ok(())
    }

    #[test]
    fn resolve_head_with_branch() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([1; 32]);
        store.set_ref("refs/heads/main", id)?;
        assert_eq!(crate::store::resolve_head(&store)?, Some(id));
        Ok(())
    }

    #[test]
    fn resolve_head_detached() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([1; 32]);
        store.set_head(HeadState::Detached(id))?;
        assert_eq!(crate::store::resolve_head(&store)?, Some(id));
        Ok(())
    }
}
