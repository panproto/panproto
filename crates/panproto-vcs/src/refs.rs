//! Branch, tag, and HEAD operations.
//!
//! Convenience functions for creating, deleting, and listing named
//! references on top of the [`Store`] trait.

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::store::{HeadState, Store};

/// Create a new branch pointing at the given commit.
///
/// # Errors
///
/// Returns [`VcsError::BranchExists`] if the branch already exists.
pub fn create_branch(
    store: &mut dyn Store,
    name: &str,
    commit_id: ObjectId,
) -> Result<(), VcsError> {
    let ref_name = format!("refs/heads/{name}");
    if store.get_ref(&ref_name)?.is_some() {
        return Err(VcsError::BranchExists {
            name: name.to_owned(),
        });
    }
    store.set_ref(&ref_name, commit_id)
}

/// Delete a branch.
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if the branch does not exist.
pub fn delete_branch(store: &mut dyn Store, name: &str) -> Result<(), VcsError> {
    let ref_name = format!("refs/heads/{name}");
    store.delete_ref(&ref_name)
}

/// List all branches and their commit IDs.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn list_branches(store: &dyn Store) -> Result<Vec<(String, ObjectId)>, VcsError> {
    let refs = store.list_refs("refs/heads/")?;
    Ok(refs
        .into_iter()
        .map(|(full_name, id)| {
            let name = full_name
                .strip_prefix("refs/heads/")
                .unwrap_or(&full_name)
                .to_owned();
            (name, id)
        })
        .collect())
}

/// Create a tag pointing at the given commit.
///
/// # Errors
///
/// Returns [`VcsError::BranchExists`] if the tag already exists (reuses
/// the same error variant for simplicity).
pub fn create_tag(store: &mut dyn Store, name: &str, commit_id: ObjectId) -> Result<(), VcsError> {
    let ref_name = format!("refs/tags/{name}");
    if store.get_ref(&ref_name)?.is_some() {
        return Err(VcsError::BranchExists {
            name: name.to_owned(),
        });
    }
    store.set_ref(&ref_name, commit_id)
}

/// Delete a tag.
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if the tag does not exist.
pub fn delete_tag(store: &mut dyn Store, name: &str) -> Result<(), VcsError> {
    let ref_name = format!("refs/tags/{name}");
    store.delete_ref(&ref_name)
}

/// List all tags and their commit IDs.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn list_tags(store: &dyn Store) -> Result<Vec<(String, ObjectId)>, VcsError> {
    let refs = store.list_refs("refs/tags/")?;
    Ok(refs
        .into_iter()
        .map(|(full_name, id)| {
            let name = full_name
                .strip_prefix("refs/tags/")
                .unwrap_or(&full_name)
                .to_owned();
            (name, id)
        })
        .collect())
}

/// Switch HEAD to point at a branch.
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if the branch does not exist.
pub fn checkout_branch(store: &mut dyn Store, name: &str) -> Result<(), VcsError> {
    let ref_name = format!("refs/heads/{name}");
    if store.get_ref(&ref_name)?.is_none() {
        return Err(VcsError::RefNotFound {
            name: name.to_owned(),
        });
    }
    store.set_head(HeadState::Branch(name.to_owned()))
}

/// Detach HEAD at a specific commit.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn checkout_detached(store: &mut dyn Store, commit_id: ObjectId) -> Result<(), VcsError> {
    store.set_head(HeadState::Detached(commit_id))
}

/// Resolve a ref-like string to an `ObjectId`.
///
/// Tries, in order:
/// 1. Full hex `ObjectId` (64 chars)
/// 2. Branch name (`refs/heads/<target>`)
/// 3. Tag name (`refs/tags/<target>`)
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if nothing matches.
pub fn resolve_ref(store: &dyn Store, target: &str) -> Result<ObjectId, VcsError> {
    // Try as a full object ID.
    if target.len() == 64 {
        if let Ok(id) = target.parse::<ObjectId>() {
            return Ok(id);
        }
    }

    // Try as HEAD.
    if target == "HEAD" {
        return crate::store::resolve_head(store)?.ok_or_else(|| VcsError::RefNotFound {
            name: "HEAD".to_owned(),
        });
    }

    // Try as a branch.
    let branch_ref = format!("refs/heads/{target}");
    if let Some(id) = store.get_ref(&branch_ref)? {
        return Ok(id);
    }

    // Try as a tag.
    let tag_ref = format!("refs/tags/{target}");
    if let Some(id) = store.get_ref(&tag_ref)? {
        return Ok(id);
    }

    Err(VcsError::RefNotFound {
        name: target.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;

    #[test]
    fn branch_create_list_delete() {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([1; 32]);

        create_branch(&mut store, "feature", id).unwrap();
        let branches = list_branches(&store).unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].0, "feature");
        assert_eq!(branches[0].1, id);

        // Duplicate should fail.
        assert!(create_branch(&mut store, "feature", id).is_err());

        delete_branch(&mut store, "feature").unwrap();
        assert!(list_branches(&store).unwrap().is_empty());
    }

    #[test]
    fn tag_create_list_delete() {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([2; 32]);

        create_tag(&mut store, "v1.0", id).unwrap();
        let tags = list_tags(&store).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].0, "v1.0");

        delete_tag(&mut store, "v1.0").unwrap();
        assert!(list_tags(&store).unwrap().is_empty());
    }

    #[test]
    fn checkout_branch_test() {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([1; 32]);
        store.set_ref("refs/heads/dev", id).unwrap();

        checkout_branch(&mut store, "dev").unwrap();
        assert_eq!(store.get_head().unwrap(), HeadState::Branch("dev".into()));
    }

    #[test]
    fn checkout_nonexistent_branch_fails() {
        let mut store = MemStore::new();
        assert!(checkout_branch(&mut store, "nonexistent").is_err());
    }

    #[test]
    fn resolve_ref_branch() {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([3; 32]);
        store.set_ref("refs/heads/main", id).unwrap();

        assert_eq!(resolve_ref(&store, "main").unwrap(), id);
    }

    #[test]
    fn resolve_ref_tag() {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([4; 32]);
        store.set_ref("refs/tags/v1.0", id).unwrap();

        assert_eq!(resolve_ref(&store, "v1.0").unwrap(), id);
    }

    #[test]
    fn resolve_ref_hex() {
        let store = MemStore::new();
        let id = ObjectId::from_bytes([5; 32]);
        let hex = id.to_string();

        assert_eq!(resolve_ref(&store, &hex).unwrap(), id);
    }

    #[test]
    fn resolve_ref_head() {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([6; 32]);
        store.set_ref("refs/heads/main", id).unwrap();

        assert_eq!(resolve_ref(&store, "HEAD").unwrap(), id);
    }

    #[test]
    fn resolve_ref_nonexistent() {
        let store = MemStore::new();
        assert!(resolve_ref(&store, "nonexistent").is_err());
    }
}
