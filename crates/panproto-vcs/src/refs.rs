//! Branch, tag, and HEAD operations.
//!
//! Convenience functions for creating, deleting, and listing named
//! references on top of the [`Store`] trait.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{Object, TagObject};
use crate::store::{HeadState, ReflogEntry, Store};

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

/// Delete a branch, checking that it is fully merged into HEAD.
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if the branch does not exist.
/// Returns [`VcsError::BranchNotMerged`] if the branch is not an
/// ancestor of the current HEAD.
pub fn delete_branch(store: &mut dyn Store, name: &str) -> Result<(), VcsError> {
    let ref_name = format!("refs/heads/{name}");
    let branch_id = store
        .get_ref(&ref_name)?
        .ok_or_else(|| VcsError::RefNotFound {
            name: name.to_owned(),
        })?;

    // Check if branch is merged into HEAD.
    if let Ok(Some(head_id)) = crate::store::resolve_head(store) {
        if head_id != branch_id
            && !crate::dag::is_ancestor(store, branch_id, head_id).unwrap_or(false)
        {
            return Err(VcsError::BranchNotMerged {
                name: name.to_owned(),
            });
        }
    }

    store.delete_ref(&ref_name)
}

/// Force-delete a branch without checking merge status.
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if the branch does not exist.
pub fn force_delete_branch(store: &mut dyn Store, name: &str) -> Result<(), VcsError> {
    let ref_name = format!("refs/heads/{name}");
    store.delete_ref(&ref_name)
}

/// Rename a branch.
///
/// # Errors
///
/// Returns [`VcsError::RefNotFound`] if the old branch does not exist.
/// Returns [`VcsError::BranchExists`] if the new name already exists.
pub fn rename_branch(
    store: &mut dyn Store,
    old_name: &str,
    new_name: &str,
) -> Result<(), VcsError> {
    let old_ref = format!("refs/heads/{old_name}");
    let new_ref = format!("refs/heads/{new_name}");

    let id = store
        .get_ref(&old_ref)?
        .ok_or_else(|| VcsError::RefNotFound {
            name: old_name.to_owned(),
        })?;

    if store.get_ref(&new_ref)?.is_some() {
        return Err(VcsError::BranchExists {
            name: new_name.to_owned(),
        });
    }

    store.set_ref(&new_ref, id)?;
    store.delete_ref(&old_ref)?;

    // Copy reflog entries from old to new.
    if let Ok(entries) = store.read_reflog(&old_ref, None) {
        for entry in &entries {
            store.append_reflog(
                &new_ref,
                ReflogEntry {
                    old_id: entry.old_id,
                    new_id: entry.new_id,
                    author: entry.author.clone(),
                    timestamp: entry.timestamp,
                    message: format!("renamed from {old_name}: {}", entry.message),
                },
            )?;
        }
    }

    // If HEAD points at the old branch, update it.
    if let Ok(HeadState::Branch(current)) = store.get_head() {
        if current == old_name {
            store.set_head(HeadState::Branch(new_name.to_owned()))?;
        }
    }

    Ok(())
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

/// Create a lightweight tag pointing at the given commit.
///
/// # Errors
///
/// Returns [`VcsError::TagExists`] if the tag already exists.
pub fn create_tag(store: &mut dyn Store, name: &str, commit_id: ObjectId) -> Result<(), VcsError> {
    let ref_name = format!("refs/tags/{name}");
    if store.get_ref(&ref_name)?.is_some() {
        return Err(VcsError::TagExists {
            name: name.to_owned(),
        });
    }
    store.set_ref(&ref_name, commit_id)
}

/// Create a lightweight tag, overwriting if it already exists.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn create_tag_force(
    store: &mut dyn Store,
    name: &str,
    commit_id: ObjectId,
) -> Result<(), VcsError> {
    let ref_name = format!("refs/tags/{name}");
    store.set_ref(&ref_name, commit_id)
}

/// Create an annotated tag with a message.
///
/// Stores a [`TagObject`] in the object store and points the tag ref at it.
///
/// # Errors
///
/// Returns [`VcsError::TagExists`] if the tag already exists.
pub fn create_annotated_tag(
    store: &mut dyn Store,
    name: &str,
    target: ObjectId,
    tagger: &str,
    message: &str,
) -> Result<ObjectId, VcsError> {
    let ref_name = format!("refs/tags/{name}");
    if store.get_ref(&ref_name)?.is_some() {
        return Err(VcsError::TagExists {
            name: name.to_owned(),
        });
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let tag_obj = Object::Tag(TagObject {
        target,
        tagger: tagger.to_owned(),
        timestamp,
        message: message.to_owned(),
    });

    let tag_id = store.put(&tag_obj)?;
    store.set_ref(&ref_name, tag_id)?;
    Ok(tag_id)
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

/// Create a new branch at the given commit and switch to it.
///
/// Combines [`create_branch`] and [`checkout_branch`] in a single call,
/// analogous to `git checkout -b`.
///
/// # Errors
///
/// Returns [`VcsError::BranchExists`] if the branch already exists.
pub fn create_and_checkout_branch(
    store: &mut dyn Store,
    name: &str,
    commit_id: ObjectId,
) -> Result<(), VcsError> {
    create_branch(store, name, commit_id)?;
    store.set_head(HeadState::Branch(name.to_owned()))
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

    // Try as a tag — peel annotated tags to find the underlying commit.
    let tag_ref = format!("refs/tags/{target}");
    if let Some(id) = store.get_ref(&tag_ref)? {
        return Ok(peel_tag(store, id));
    }

    Err(VcsError::RefNotFound {
        name: target.to_owned(),
    })
}

/// Peel an object ID through annotated tag objects to find the
/// underlying commit or other non-tag object.
fn peel_tag(store: &dyn Store, mut id: ObjectId) -> ObjectId {
    // Follow up to 10 tag indirections to prevent infinite loops.
    for _ in 0..10 {
        match store.get(&id) {
            Ok(Object::Tag(tag)) => id = tag.target,
            _ => return id,
        }
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::error::VcsError;

    #[test]
    fn branch_create_list_delete() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([1; 32]);

        create_branch(&mut store, "feature", id)?;
        let branches = list_branches(&store)?;
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].0, "feature");
        assert_eq!(branches[0].1, id);

        // Duplicate should fail.
        assert!(create_branch(&mut store, "feature", id).is_err());

        delete_branch(&mut store, "feature")?;
        assert!(list_branches(&store)?.is_empty());
        Ok(())
    }

    #[test]
    fn tag_create_list_delete() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([2; 32]);

        create_tag(&mut store, "v1.0", id)?;
        let tags = list_tags(&store)?;
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].0, "v1.0");

        delete_tag(&mut store, "v1.0")?;
        assert!(list_tags(&store)?.is_empty());
        Ok(())
    }

    #[test]
    fn checkout_branch_test() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([1; 32]);
        store.set_ref("refs/heads/dev", id)?;

        checkout_branch(&mut store, "dev")?;
        assert_eq!(store.get_head()?, HeadState::Branch("dev".into()));
        Ok(())
    }

    #[test]
    fn checkout_nonexistent_branch_fails() {
        let mut store = MemStore::new();
        assert!(checkout_branch(&mut store, "nonexistent").is_err());
    }

    #[test]
    fn resolve_ref_branch() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([3; 32]);
        store.set_ref("refs/heads/main", id)?;

        assert_eq!(resolve_ref(&store, "main")?, id);
        Ok(())
    }

    #[test]
    fn resolve_ref_tag() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([4; 32]);
        store.set_ref("refs/tags/v1.0", id)?;

        assert_eq!(resolve_ref(&store, "v1.0")?, id);
        Ok(())
    }

    #[test]
    fn resolve_ref_hex() -> Result<(), VcsError> {
        let store = MemStore::new();
        let id = ObjectId::from_bytes([5; 32]);
        let hex = id.to_string();

        assert_eq!(resolve_ref(&store, &hex)?, id);
        Ok(())
    }

    #[test]
    fn resolve_ref_head() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let id = ObjectId::from_bytes([6; 32]);
        store.set_ref("refs/heads/main", id)?;

        assert_eq!(resolve_ref(&store, "HEAD")?, id);
        Ok(())
    }

    #[test]
    fn resolve_ref_nonexistent() {
        let store = MemStore::new();
        assert!(resolve_ref(&store, "nonexistent").is_err());
    }
}
