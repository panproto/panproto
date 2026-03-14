//! Storage trait for the VCS object store and ref system.
//!
//! The [`Store`] trait abstracts over the backing storage, enabling both
//! filesystem-backed repositories ([`FsStore`](crate::fs_store::FsStore))
//! and in-memory stores ([`MemStore`](crate::mem_store::MemStore)) for
//! testing and WASM.

use serde::{Deserialize, Serialize};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::Object;

/// The state of HEAD.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadState {
    /// HEAD points to a branch by name (e.g., `"main"`).
    Branch(String),
    /// HEAD is detached, pointing directly at a commit.
    Detached(ObjectId),
}

/// A reflog entry recording a ref mutation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReflogEntry {
    /// The previous value of the ref (`None` for newly created refs).
    pub old_id: Option<ObjectId>,
    /// The new value of the ref.
    pub new_id: ObjectId,
    /// Who made the change.
    pub author: String,
    /// When the change was made (Unix seconds).
    pub timestamp: u64,
    /// A description of the change (e.g., `"commit: add user field"`).
    pub message: String,
}

/// Storage backend for the VCS.
///
/// Implementations provide content-addressed object storage, named
/// references (branches and tags), and HEAD management. All mutable
/// operations take `&mut self`.
pub trait Store {
    // -- Objects --

    /// Check whether an object exists in the store.
    fn has(&self, id: &ObjectId) -> bool;

    /// Retrieve an object by its content-addressed ID.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::ObjectNotFound`] if the object does not exist.
    fn get(&self, id: &ObjectId) -> Result<Object, VcsError>;

    /// Store an object and return its content-addressed ID.
    ///
    /// If the object already exists (same hash), this is a no-op that
    /// returns the existing ID.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or I/O fails.
    fn put(&mut self, object: &Object) -> Result<ObjectId, VcsError>;

    // -- Refs --

    /// Read a named reference (branch or tag).
    ///
    /// Returns `None` if the ref does not exist.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn get_ref(&self, name: &str) -> Result<Option<ObjectId>, VcsError>;

    /// Create or update a named reference.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn set_ref(&mut self, name: &str, id: ObjectId) -> Result<(), VcsError>;

    /// Delete a named reference.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::RefNotFound`] if the ref does not exist.
    fn delete_ref(&mut self, name: &str) -> Result<(), VcsError>;

    /// List all references whose names start with `prefix`.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn list_refs(&self, prefix: &str) -> Result<Vec<(String, ObjectId)>, VcsError>;

    // -- HEAD --

    /// Read the current HEAD state.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn get_head(&self) -> Result<HeadState, VcsError>;

    /// Update the HEAD state.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn set_head(&mut self, state: HeadState) -> Result<(), VcsError>;

    // -- Enumeration --

    /// List all object IDs in the store.
    ///
    /// Used by garbage collection to find unreachable objects.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn list_objects(&self) -> Result<Vec<ObjectId>, VcsError>;

    /// Delete an object from the store.
    ///
    /// # Errors
    ///
    /// Returns an error if the object does not exist or I/O fails.
    fn delete_object(&mut self, id: &ObjectId) -> Result<(), VcsError>;

    // -- Reflog --

    /// Append an entry to a ref's reflog.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn append_reflog(&mut self, ref_name: &str, entry: ReflogEntry) -> Result<(), VcsError>;

    /// Read reflog entries for a ref, newest first.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    fn read_reflog(
        &self,
        ref_name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ReflogEntry>, VcsError>;
}

/// Resolve HEAD to a commit `ObjectId`.
///
/// If HEAD points to a branch, follows the branch ref. Returns `None` if
/// HEAD points to a branch that has no commits yet (empty repository).
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn resolve_head(store: &dyn Store) -> Result<Option<ObjectId>, VcsError> {
    match store.get_head()? {
        HeadState::Branch(name) => {
            let ref_name = format!("refs/heads/{name}");
            store.get_ref(&ref_name)
        }
        HeadState::Detached(id) => Ok(Some(id)),
    }
}
