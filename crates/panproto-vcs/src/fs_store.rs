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
//!   HEAD                           JSON HeadState
//!   objects/<hex[0..2]>/<hex[2..]>  rmp-serde bytes of Object
//!   refs/heads/main                hex ObjectId (plain text)
//!   refs/tags/v1.0                 hex ObjectId
//!   index.json                     staged schema state
//!   logs/                          reflog entries (NDJSON)
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

    /// Resolve a ref name to its file inside the store.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::InvalidRefName`] when the name would address
    /// a file outside the store root.
    fn ref_path(&self, name: &str) -> Result<PathBuf, VcsError> {
        validate_ref_name(name)?;
        Ok(self.root.join(name))
    }

    /// Resolve a ref-name *prefix* to the directory it names.
    ///
    /// The empty prefix names the store root, which is how callers
    /// enumerate every ref at once; every other prefix is validated like
    /// a ref name.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::InvalidRefName`] when the prefix would
    /// address a directory outside the store root.
    fn ref_prefix_path(&self, prefix: &str) -> Result<PathBuf, VcsError> {
        if prefix.is_empty() {
            return Ok(self.root.clone());
        }
        self.ref_path(prefix)
    }

    fn head_path(&self) -> PathBuf {
        self.root.join("HEAD")
    }

    /// Resolve a ref name to its reflog file.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::InvalidRefName`] when the name would address
    /// a file outside the store's log directory.
    fn reflog_path(&self, ref_name: &str) -> Result<PathBuf, VcsError> {
        validate_ref_name(ref_name)?;
        Ok(self.root.join("logs").join(ref_name))
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
        // The store is content-addressed: the bytes filed under an ID
        // must hash back to it. Re-deriving the address on read turns a
        // torn write or a substituted file into an error rather than a
        // silently wrong object.
        let actual = hash::object_id(&object)?;
        if actual != *id {
            return Err(VcsError::ObjectCorrupted { id: *id, actual });
        }
        Ok(object)
    }

    fn put(&mut self, object: &Object) -> Result<ObjectId, VcsError> {
        let id = hash::object_id(object)?;
        let path = self.object_path(&id);
        if path.exists() {
            return Ok(id);
        }
        // Create fan-out directory if needed.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = rmp_serde::to_vec(object)?;
        write_atomically(&path, &bytes)?;
        Ok(id)
    }

    fn get_ref(&self, name: &str) -> Result<Option<ObjectId>, VcsError> {
        let path = self.ref_path(name)?;
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
        let path = self.ref_path(name)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomically(&path, format!("{id}\n").as_bytes())?;
        Ok(())
    }

    fn delete_ref(&mut self, name: &str) -> Result<(), VcsError> {
        let path = self.ref_path(name)?;
        if !path.exists() {
            return Err(VcsError::RefNotFound {
                name: name.to_owned(),
            });
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let base = self.ref_prefix_path(prefix)?;
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
        let path = self.reflog_path(ref_name)?;
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
        let path = self.reflog_path(ref_name)?;
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

/// Check that a ref name addresses a file inside the store.
///
/// A ref name is joined onto the store root, so anything but a chain of
/// ordinary path components can leave the repository: `..` climbs out,
/// a leading `/` (or a Windows drive prefix) replaces the root outright,
/// and `.` is at best noise. Ref names arrive from remotes over the
/// network, so this check is the boundary that keeps a remote from
/// naming a write target of its choosing.
///
/// # Errors
///
/// Returns [`VcsError::InvalidRefName`] describing the first component
/// that fails.
fn validate_ref_name(name: &str) -> Result<(), VcsError> {
    use std::path::Component;

    let reject = |reason: &'static str| {
        Err(VcsError::InvalidRefName {
            name: name.to_owned(),
            reason,
        })
    };

    if name.is_empty() {
        return reject("a ref name may not be empty");
    }
    if name.contains('\0') {
        return reject("a ref name may not contain a NUL byte");
    }
    // Windows accepts both separators, so a name that is relative under
    // Unix component rules can still be absolute there. Rejecting the
    // backslash outright keeps the accepted set identical on every host.
    if name.contains('\\') {
        return reject("a ref name may not contain a backslash");
    }
    for component in Path::new(name).components() {
        match component {
            Component::Normal(_) => {}
            Component::ParentDir => return reject("a ref name may not contain \"..\""),
            Component::CurDir => return reject("a ref name may not contain \".\""),
            Component::RootDir | Component::Prefix(_) => {
                return reject("a ref name must be relative to the store root");
            }
        }
    }
    Ok(())
}

/// Write `bytes` to `path` so a reader sees either the previous contents
/// or all of the new ones, never a prefix.
///
/// The bytes land in a sibling temporary file, are flushed to the
/// device, and are then renamed over the destination; `rename` within a
/// directory is atomic, so an interrupted write leaves the temporary
/// behind rather than a half-written object. The store is
/// content-addressed and its readers verify the address, so a leftover
/// temporary is inert: it is not named by any ID.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), VcsError> {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = parent.join(format!(
        ".{stem}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));

    let write = |tmp: &Path| -> Result<(), std::io::Error> {
        let mut file = fs::File::create(tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(tmp, path)
    };

    match write(&tmp) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Do not leave the staging file behind on a failure we can
            // see; the removal itself is best-effort.
            drop(fs::remove_file(&tmp));
            Err(VcsError::Io(e))
        }
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
        let id = crate::tree::store_schema_as_tree(&mut store, test_schema())?;

        assert!(store.has(&id));

        let retrieved = store.get(&id)?;
        match retrieved {
            Object::SchemaTree(tree) => match *tree {
                crate::object::SchemaTreeObject::SingleLeaf { .. } => {}
                crate::object::SchemaTreeObject::Directory { .. } => {
                    panic!("expected SingleLeaf wrapper, got Directory")
                }
            },
            _ => panic!("expected SchemaTree object"),
        }
        Ok(())
    }

    #[test]
    fn flat_schema_put_get_round_trip_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = store.put(&Object::FlatSchema(Box::new(test_schema())))?;
        match store.get(&id)? {
            Object::FlatSchema(s) => {
                assert_eq!(s.vertices.len(), 1);
            }
            other => panic!("expected FlatSchema, got {}", other.type_name()),
        }
        // After reopen, the object must still deserialize cleanly.
        let reopened = FsStore::open(dir.path())?;
        match reopened.get(&id)? {
            Object::FlatSchema(_) => {}
            other => panic!(
                "expected FlatSchema after reopen, got {}",
                other.type_name()
            ),
        }
        Ok(())
    }

    #[test]
    fn multi_leaf_schema_tree_round_trips_fs() -> Result<(), Box<dyn std::error::Error>> {
        use crate::object::FileSchemaObject;
        use std::path::PathBuf;

        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;

        let mk_file = |path: &str| FileSchemaObject {
            path: path.to_owned(),
            protocol: "project".to_owned(),
            schema: test_schema(),
            cross_file_edges: Vec::new(),
        };

        let root = crate::tree::build_schema_tree(
            &mut store,
            vec![
                (PathBuf::from("src/a.rs"), mk_file("src/a.rs")),
                (PathBuf::from("src/b.rs"), mk_file("src/b.rs")),
                (PathBuf::from("c.rs"), mk_file("c.rs")),
            ],
        )?;

        // Assemble back via walk; the tree must carry all three leaves.
        let mut count = 0usize;
        crate::tree::walk_tree(&store, &root, |_, _| {
            count += 1;
            Ok(())
        })?;
        assert_eq!(count, 3);

        // Re-open the store from disk and confirm the tree is still
        // intact: this catches serialization bugs that only show up
        // after a round-trip through FsStore.
        let reopened = FsStore::open(dir.path())?;
        let mut recount = 0usize;
        crate::tree::walk_tree(&reopened, &root, |_, _| {
            recount += 1;
            Ok(())
        })?;
        assert_eq!(recount, 3);
        Ok(())
    }

    #[test]
    fn put_idempotent_fs() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id1 = crate::tree::store_schema_as_tree(&mut store, test_schema())?;
        let id2 = crate::tree::store_schema_as_tree(&mut store, test_schema())?;
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

    #[test]
    fn ref_name_escaping_the_store_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = ObjectId::from_bytes([7; 32]);

        let outside = dir.path().join("pwned");
        match store.set_ref("refs/heads/../../pwned", id) {
            Err(VcsError::InvalidRefName { .. }) => {}
            Err(other) => panic!("expected InvalidRefName, got {other}"),
            Ok(()) => panic!("a ref name climbing out of the store must be refused"),
        }
        assert!(
            !outside.exists(),
            "no file may be written outside the store root"
        );
        Ok(())
    }

    #[test]
    fn absolute_ref_name_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = ObjectId::from_bytes([8; 32]);
        match store.set_ref("/tmp/panproto-pwned", id) {
            Err(VcsError::InvalidRefName { .. }) => {}
            Err(other) => panic!("expected InvalidRefName, got {other}"),
            Ok(()) => panic!("an absolute ref name must be refused"),
        }
        Ok(())
    }

    #[test]
    fn ref_reads_reject_an_escaping_name() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        assert!(store.get_ref("refs/heads/../../HEAD").is_err());
        assert!(store.delete_ref("refs/heads/../../HEAD").is_err());
        assert!(store.list_refs("refs/heads/../..").is_err());
        assert!(
            store
                .append_reflog(
                    "../../escaped",
                    ReflogEntry {
                        old_id: None,
                        new_id: ObjectId::from_bytes([3; 32]),
                        author: "test".into(),
                        timestamp: 1,
                        message: "escape".into(),
                    },
                )
                .is_err()
        );
        assert!(store.read_reflog("../../escaped", None).is_err());
        Ok(())
    }

    #[test]
    fn tampered_object_is_refused_on_read() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = store.put(&Object::FlatSchema(Box::new(test_schema())))?;

        // Overwrite the stored bytes with a different, well-formed
        // object. A torn write or a hostile mirror produces exactly this
        // state: the content address no longer describes the bytes it
        // names.
        let hex = id.to_string();
        let path = dir
            .path()
            .join(".panproto/objects")
            .join(&hex[..2])
            .join(&hex[2..]);
        let mut other = test_schema();
        other.protocol = "tampered".into();
        fs::write(
            &path,
            rmp_serde::to_vec(&Object::FlatSchema(Box::new(other)))?,
        )?;

        match store.get(&id) {
            Err(VcsError::ObjectCorrupted { id: reported, .. }) => assert_eq!(reported, id),
            Err(other) => panic!("expected ObjectCorrupted, got {other}"),
            Ok(obj) => panic!("tampered object returned as genuine: {}", obj.type_name()),
        }
        Ok(())
    }

    #[test]
    fn truncated_object_is_refused_on_read() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = store.put(&Object::FlatSchema(Box::new(test_schema())))?;

        let hex = id.to_string();
        let path = dir
            .path()
            .join(".panproto/objects")
            .join(&hex[..2])
            .join(&hex[2..]);
        let bytes = fs::read(&path)?;
        fs::write(&path, &bytes[..bytes.len() / 2])?;

        assert!(store.get(&id).is_err(), "a torn object must not decode");
        Ok(())
    }

    #[test]
    fn put_leaves_no_partial_file_behind() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut store = FsStore::init(dir.path())?;
        let id = store.put(&Object::FlatSchema(Box::new(test_schema())))?;

        // The object is published under its content address and nothing
        // else: a write-then-rename must not leave its staging file in
        // the fan-out directory.
        let hex = id.to_string();
        let fan = dir.path().join(".panproto/objects").join(&hex[..2]);
        let names: Vec<String> = fs::read_dir(&fan)?
            .map(|e| Ok::<_, std::io::Error>(e?.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        assert_eq!(names, vec![hex[2..].to_owned()]);
        Ok(())
    }
}
