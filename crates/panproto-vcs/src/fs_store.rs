//! Filesystem-backed store implementation.
//!
//! Stores objects in `.panproto/objects/` using fan-out directories (first
//! 2 hex chars as subdirectory), refs as plain text files containing hex
//! `ObjectId`s, and reflogs as newline-delimited JSON in `.panproto/logs/`.
//!
//! ## Directory layout
//!
//! ```text
//! .panproto/
//!   HEAD                           — JSON HeadState
//!   objects/<hex[0..2]>/<hex[2..]>  — rmp-serde bytes of Object
//!   refs/heads/main                — hex ObjectId (plain text)
//!   refs/tags/v1.0                 — hex ObjectId
//!   index.json                     — staged schema state
//!   logs/                          — reflog entries (NDJSON)
//!     HEAD
//!     refs/heads/main
//! ```

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::error::VcsError;
use crate::hash::{self, ObjectId};
use crate::object::Object;
use crate::store::{HeadState, ReflogEntry, Store};

/// A filesystem-backed [`Store`].
///
/// All data lives under a `.panproto/` directory inside the repository root.
#[derive(Debug, Clone)]
pub struct FsStore {
    /// Path to the `.panproto/` directory.
    root: PathBuf,
}

impl FsStore {
    /// Open an existing `.panproto/` directory.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::NotARepository`] if the directory does not exist.
    pub fn open(repo_dir: &Path) -> Result<Self, VcsError> {
        let root = repo_dir.join(".panproto");
        if !root.is_dir() {
            return Err(VcsError::NotARepository);
        }
        Ok(Self { root })
    }

    /// Initialize a new `.panproto/` directory at the given path.
    ///
    /// Creates the directory structure and sets HEAD to `main`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory already exists or I/O fails.
    pub fn init(repo_dir: &Path) -> Result<Self, VcsError> {
        let root = repo_dir.join(".panproto");
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("refs/heads"))?;
        fs::create_dir_all(root.join("refs/tags"))?;
        fs::create_dir_all(root.join("logs/refs/heads"))?;

        let store = Self { root };
        store.write_head(&HeadState::Branch("main".into()))?;
        Ok(store)
    }

    /// Return the path to the `.panproto/` directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    // -- internal helpers --

    fn object_path(&self, id: &ObjectId) -> PathBuf {
        let hex = id.to_string();
        self.root.join("objects").join(&hex[..2]).join(&hex[2..])
    }

    fn ref_path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn head_path(&self) -> PathBuf {
        self.root.join("HEAD")
    }

    fn reflog_path(&self, ref_name: &str) -> PathBuf {
        self.root.join("logs").join(ref_name)
    }

    fn write_head(&self, state: &HeadState) -> Result<(), VcsError> {
        let json = serde_json::to_string(state).map_err(|e| {
            VcsError::Serialization(crate::error::SerializationError(e.to_string()))
        })?;
        fs::write(self.head_path(), json)?;
        Ok(())
    }
}

impl Store for FsStore {
    fn has(&self, id: &ObjectId) -> bool {
        self.object_path(id).exists()
    }

    fn get(&self, id: &ObjectId) -> Result<Object, VcsError> {
        let path = self.object_path(id);
        let bytes = fs::read(&path).map_err(|_| VcsError::ObjectNotFound { id: *id })?;
        let object: Object = rmp_serde::from_slice(&bytes)?;
        Ok(object)
    }

    fn put(&mut self, object: &Object) -> Result<ObjectId, VcsError> {
        let id = compute_object_id(object)?;
        let path = self.object_path(&id);
        if path.exists() {
            return Ok(id);
        }
        // Create fan-out directory if needed.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = rmp_serde::to_vec(object)?;
        fs::write(&path, bytes)?;
        Ok(id)
    }

    fn get_ref(&self, name: &str) -> Result<Option<ObjectId>, VcsError> {
        let path = self.ref_path(name);
        if !path.exists() {
            return Ok(None);
        }
        let hex = fs::read_to_string(&path)?;
        let id: ObjectId = hex
            .trim()
            .parse()
            .map_err(|e: crate::hash::ParseObjectIdError| {
                VcsError::Serialization(crate::error::SerializationError(e.to_string()))
            })?;
        Ok(Some(id))
    }

    fn set_ref(&mut self, name: &str, id: ObjectId) -> Result<(), VcsError> {
        let path = self.ref_path(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, format!("{id}\n"))?;
        Ok(())
    }

    fn delete_ref(&mut self, name: &str) -> Result<(), VcsError> {
        let path = self.ref_path(name);
        if !path.exists() {
            return Err(VcsError::RefNotFound {
                name: name.to_owned(),
            });
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let base = self.ref_path(prefix);
        if !base.is_dir() {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        collect_refs_recursive(&base, prefix, &mut result)?;
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    fn get_head(&self) -> Result<HeadState, VcsError> {
        let json = fs::read_to_string(self.head_path())?;
        let state: HeadState = serde_json::from_str(&json).map_err(|e| {
            VcsError::Serialization(crate::error::SerializationError(e.to_string()))
        })?;
        Ok(state)
    }

    fn set_head(&mut self, state: HeadState) -> Result<(), VcsError> {
        self.write_head(&state)
    }

    fn list_objects(&self) -> Result<Vec<ObjectId>, VcsError> {
        let objects_dir = self.root.join("objects");
        let mut ids = Vec::new();
        if !objects_dir.is_dir() {
            return Ok(ids);
        }
        for fan_entry in fs::read_dir(&objects_dir)? {
            let fan_entry = fan_entry?;
            if !fan_entry.path().is_dir() {
                continue;
            }
            let fan = fan_entry.file_name().to_string_lossy().to_string();
            for obj_entry in fs::read_dir(fan_entry.path())? {
                let obj_entry = obj_entry?;
                if !obj_entry.path().is_file() {
                    continue;
                }
                let rest = obj_entry.file_name().to_string_lossy().to_string();
                let hex = format!("{fan}{rest}");
                if let Ok(id) = hex.parse::<ObjectId>() {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    fn delete_object(&mut self, id: &ObjectId) -> Result<(), VcsError> {
        let path = self.object_path(id);
        if !path.exists() {
            return Err(VcsError::ObjectNotFound { id: *id });
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    fn append_reflog(&mut self, ref_name: &str, entry: ReflogEntry) -> Result<(), VcsError> {
        let path = self.reflog_path(ref_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(&entry).map_err(|e| {
            VcsError::Serialization(crate::error::SerializationError(e.to_string()))
        })?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(file, "{json}")?;
        Ok(())
    }

    fn read_reflog(
        &self,
        ref_name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ReflogEntry>, VcsError> {
        let path = self.reflog_path(ref_name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&path)?;
        let mut entries: Vec<ReflogEntry> = content
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        // Return newest first.
        entries.reverse();
        if let Some(n) = limit {
            entries.truncate(n);
        }
        Ok(entries)
    }
}

/// Recursively collect ref files under a directory.
fn collect_refs_recursive(
    dir: &Path,
    prefix: &str,
    result: &mut Vec<(String, ObjectId)>,
) -> Result<(), VcsError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            let sub_prefix = format!("{prefix}{name}/");
            collect_refs_recursive(&path, &sub_prefix, result)?;
        } else if path.is_file() {
            let hex = fs::read_to_string(&path)?;
            if let Ok(id) = hex.trim().parse::<ObjectId>() {
                result.push((format!("{prefix}{name}"), id));
            }
        }
    }
    Ok(())
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
        Object::Theory(theory) => hash::hash_theory(theory),
        Object::TheoryMorphism(morphism) => hash::hash_theory_morphism(morphism),
        Object::CstComplement(cst_comp) => hash::hash_cst_complement(cst_comp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn init_creates_directory_structure() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let _store = FsStore::init(dir.path())?;

        assert!(dir.path().join(".panproto/objects").is_dir());
        assert!(dir.path().join(".panproto/refs/heads").is_dir());
        assert!(dir.path().join(".panproto/refs/tags").is_dir());
        assert!(dir.path().join(".panproto/logs").is_dir());
        assert!(dir.path().join(".panproto/HEAD").is_file());
        Ok(())
    }

    #[test]
    fn open_nonexistent_returns_error() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let result = FsStore::open(dir.path());
        assert!(matches!(result, Err(VcsError::NotARepository)));
        Ok(())
    }

    #[test]
    fn open_after_init() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        FsStore::init(dir.path())?;
        let store = FsStore::open(dir.path())?;
        assert_eq!(store.get_head()?, HeadState::Branch("main".into()));
        Ok(())
    }

    #[test]
    fn put_get_round_trip_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let obj = Object::Schema(Box::new(test_schema()));
        let id = store.put(&obj)?;

        assert!(store.has(&id));

        let retrieved = store.get(&id)?;
        match retrieved {
            Object::Schema(s) => assert_eq!(s.protocol, "test"),
            _ => panic!("expected Schema object"),
        }
        Ok(())
    }

    #[test]
    fn put_idempotent_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let obj = Object::Schema(Box::new(test_schema()));
        let id1 = store.put(&obj)?;
        let id2 = store.put(&obj)?;
        assert_eq!(id1, id2);
        Ok(())
    }

    #[test]
    fn ref_operations_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = ObjectId::from_bytes([42; 32]);

        store.set_ref("refs/heads/main", id)?;
        assert_eq!(store.get_ref("refs/heads/main")?, Some(id));

        let refs = store.list_refs("refs/heads/")?;
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "refs/heads/main");

        store.delete_ref("refs/heads/main")?;
        assert_eq!(store.get_ref("refs/heads/main")?, None);
        Ok(())
    }

    #[test]
    fn head_state_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        assert_eq!(store.get_head()?, HeadState::Branch("main".into()));

        let id = ObjectId::from_bytes([1; 32]);
        store.set_head(HeadState::Detached(id))?;
        assert_eq!(store.get_head()?, HeadState::Detached(id));
        Ok(())
    }

    #[test]
    fn reflog_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;

        store.append_reflog(
            "HEAD",
            ReflogEntry {
                old_id: None,
                new_id: ObjectId::from_bytes([1; 32]),
                author: "test".into(),
                timestamp: 100,
                message: "first".into(),
            },
        )?;
        store.append_reflog(
            "HEAD",
            ReflogEntry {
                old_id: Some(ObjectId::from_bytes([1; 32])),
                new_id: ObjectId::from_bytes([2; 32]),
                author: "test".into(),
                timestamp: 200,
                message: "second".into(),
            },
        )?;

        let log = store.read_reflog("HEAD", None)?;
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].message, "second");
        assert_eq!(log[1].message, "first");
        Ok(())
    }

    #[test]
    fn nested_branch_refs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id1 = ObjectId::from_bytes([1; 32]);
        let id2 = ObjectId::from_bytes([2; 32]);

        store.set_ref("refs/heads/feature/add-field", id1)?;
        store.set_ref("refs/heads/feature/remove-field", id2)?;

        let refs = store.list_refs("refs/heads/")?;
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "refs/heads/feature/add-field");
        assert_eq!(refs[1].0, "refs/heads/feature/remove-field");
        Ok(())
    }
}
