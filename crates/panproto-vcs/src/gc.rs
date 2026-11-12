//! Garbage collection: remove unreachable objects.
//!
//! An object is reachable when a path of references leads to it from a
//! root. The roots are the commit `HEAD` resolves to, the target of every
//! ref under `refs/` (branches, tags, remote-tracking refs, the stash and
//! any other namespace), every entry of the stash's reflog (the stash list),
//! and, for [`Repository::gc`](crate::Repository::gc), every object the
//! staging index names. [`references`] lists the references of each object
//! kind. Collection deletes exactly the stored objects no path reaches, so
//! it never removes an object reachable from a ref.

use std::collections::HashSet;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{Object, SchemaTreeEntry, SchemaTreeObject};
use crate::store::Store;

/// Options controlling garbage collection behavior.
#[derive(Clone, Debug, Default)]
pub struct GcOptions {
    /// If true, only report what would be deleted without deleting.
    pub dry_run: bool,
}

/// Report from a garbage collection run.
#[derive(Clone, Debug, Default)]
pub struct GcReport {
    /// Number of objects marked as reachable.
    pub reachable: usize,
    /// Object IDs that were deleted, in ascending order.
    pub deleted: Vec<ObjectId>,
}

/// The objects `object` refers to by identifier, in field order.
///
/// A commit refers to its schema, migration, parents, protocol, data sets,
/// complements, edit logs, theories and CST complements; a migration to
/// its source and target schemas; a schema tree to its entries; a tag to
/// its target; a data set to its schema; a complement to its migration and
/// data set; an edit log to its schema, data set and final complement; a
/// CST complement to its data set. Protocols, expressions, theories, theory
/// morphisms, file schemas and flat schemas refer to nothing.
#[must_use]
pub fn references(object: &Object) -> Vec<ObjectId> {
    match object {
        Object::Commit(commit) => {
            let mut out = vec![commit.schema_id];
            out.extend(commit.migration_id);
            out.extend(&commit.parents);
            out.extend(commit.protocol_id);
            out.extend(&commit.data_ids);
            out.extend(&commit.complement_ids);
            out.extend(&commit.edit_log_ids);
            out.extend(commit.theory_ids.values());
            out.extend(&commit.cst_complement_ids);
            out
        }
        Object::Migration { src, tgt, .. } => vec![*src, *tgt],
        Object::SchemaTree(tree) => match tree.as_ref() {
            SchemaTreeObject::SingleLeaf { file_schema_id } => vec![*file_schema_id],
            SchemaTreeObject::Directory { .. } => tree
                .sorted_entries()
                .into_iter()
                .map(|(_, entry)| match entry {
                    SchemaTreeEntry::File(id) | SchemaTreeEntry::Tree(id) => *id,
                })
                .collect(),
        },
        Object::Tag(tag) => vec![tag.target],
        Object::DataSet(dataset) => vec![dataset.schema_id],
        Object::Complement(complement) => vec![complement.migration_id, complement.data_id],
        Object::EditLog(edit_log) => vec![
            edit_log.schema_id,
            edit_log.data_id,
            edit_log.final_complement,
        ],
        Object::CstComplement(cst) => vec![cst.data_id],
        Object::Protocol(_)
        | Object::Expr(_)
        | Object::Theory(_)
        | Object::TheoryMorphism(_)
        | Object::FileSchema(_)
        | Object::FlatSchema(_) => Vec::new(),
    }
}

/// Mark all objects reachable from the given roots.
///
/// Follows [`references`] transitively. An identifier the store does not
/// hold is marked but not followed.
///
/// # Errors
///
/// Returns an error if a stored object cannot be read, so that a
/// collection never proceeds from an incomplete mark.
pub fn mark_reachable(
    store: &dyn Store,
    roots: &[ObjectId],
) -> Result<HashSet<ObjectId>, VcsError> {
    let mut reachable = HashSet::new();
    let mut queue: Vec<ObjectId> = roots.to_vec();

    while let Some(id) = queue.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if !store.has(&id) {
            continue;
        }
        queue.extend(references(&store.get(&id)?));
    }

    Ok(reachable)
}

/// Collect the roots: `HEAD`, every ref under `refs/`, and every entry of
/// the stash's reflog.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn collect_roots(store: &dyn Store) -> Result<Vec<ObjectId>, VcsError> {
    let mut roots = Vec::new();

    if let Some(id) = crate::store::resolve_head(store)? {
        roots.push(id);
    }

    for (_, id) in store.list_refs("refs/")? {
        roots.push(id);
    }

    // Older stash entries are named only by the stash's reflog.
    for entry in store.read_reflog("refs/stash", None)? {
        roots.push(entry.new_id);
    }

    roots.sort_unstable();
    roots.dedup();
    Ok(roots)
}

/// Run garbage collection: mark reachable objects, delete the rest.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn gc(store: &mut dyn Store) -> Result<GcReport, VcsError> {
    gc_with_roots(store, &[], &GcOptions::default())
}

/// Run garbage collection with options.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn gc_with_options(store: &mut dyn Store, options: &GcOptions) -> Result<GcReport, VcsError> {
    gc_with_roots(store, &[], options)
}

/// Run garbage collection keeping, besides the roots [`collect_roots`]
/// finds, everything reachable from `extra_roots`.
///
/// # Errors
///
/// Returns an error on I/O failure or when a reachable object cannot be
/// read; nothing is deleted then.
pub fn gc_with_roots(
    store: &mut dyn Store,
    extra_roots: &[ObjectId],
    options: &GcOptions,
) -> Result<GcReport, VcsError> {
    let mut roots = collect_roots(store)?;
    roots.extend_from_slice(extra_roots);
    let reachable = mark_reachable(store, &roots)?;
    let mut deleted: Vec<ObjectId> = store
        .list_objects()?
        .into_iter()
        .filter(|id| !reachable.contains(id))
        .collect();
    deleted.sort_unstable();
    if !options.dry_run {
        for id in &deleted {
            store.delete_object(id)?;
        }
    }
    Ok(GcReport {
        reachable: reachable.len(),
        deleted,
    })
}

/// Compute reachability without deleting anything.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn gc_report(store: &dyn Store) -> Result<GcReport, VcsError> {
    let roots = collect_roots(store)?;
    let reachable = mark_reachable(store, &roots)?;

    Ok(GcReport {
        reachable: reachable.len(),
        deleted: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::error::VcsError;
    use crate::object::CommitObject;

    fn empty_schema() -> panproto_schema::Schema {
        panproto_schema::Schema {
            protocol: "test".into(),
            vertices: std::collections::HashMap::new(),
            edges: std::collections::HashMap::new(),
            hyper_edges: std::collections::HashMap::new(),
            constraints: std::collections::HashMap::new(),
            required: std::collections::HashMap::new(),
            nsids: std::collections::HashMap::new(),
            entries: Vec::new(),
            variants: std::collections::HashMap::new(),
            orderings: std::collections::HashMap::new(),
            recursion_points: std::collections::HashMap::new(),
            spans: std::collections::HashMap::new(),
            usage_modes: std::collections::HashMap::new(),
            nominal: std::collections::HashMap::new(),
            coercions: std::collections::HashMap::new(),
            mergers: std::collections::HashMap::new(),
            defaults: std::collections::HashMap::new(),
            policies: std::collections::HashMap::new(),
            outgoing: std::collections::HashMap::new(),
            incoming: std::collections::HashMap::new(),
            between: std::collections::HashMap::new(),
            ..Default::default()
        }
    }

    #[test]
    fn mark_reachable_follows_commits() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let schema_id = crate::tree::store_schema_as_tree(&mut store, empty_schema())?;

        let c0 = CommitObject::builder(schema_id, "test", "test", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject::builder(schema_id, "test", "test", "second")
            .parents(vec![c0_id])
            .timestamp(200)
            .build();
        let c1_id = store.put(&Object::Commit(c1))?;

        let reachable = mark_reachable(&store, &[c1_id])?;
        assert!(reachable.contains(&c1_id));
        assert!(reachable.contains(&c0_id));
        assert!(reachable.contains(&schema_id));
        Ok(())
    }

    #[test]
    fn gc_deletes_unreachable() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let schema_id = crate::tree::store_schema_as_tree(&mut store, empty_schema())?;

        let c0 = CommitObject::builder(schema_id, "test", "test", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;
        store.set_ref("refs/heads/main", c0_id)?;

        // Add an orphan object not reachable from any ref.
        let orphan_schema_id = crate::tree::store_schema_as_tree(&mut store, empty_schema())?;
        let orphan = CommitObject::builder(orphan_schema_id, "test", "test", "orphan")
            .timestamp(300)
            .build();
        let orphan_id = store.put(&Object::Commit(orphan))?;

        // Before GC: orphan exists.
        assert!(store.has(&orphan_id));

        let report = gc(&mut store)?;
        // c0 + SchemaTree root + FileSchema leaf = 3.
        assert_eq!(report.reachable, 3);
        assert!(report.deleted.contains(&orphan_id));

        // After GC: orphan is gone.
        assert!(!store.has(&orphan_id));
        Ok(())
    }

    #[test]
    fn gc_report_counts_reachable() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let schema_id = crate::tree::store_schema_as_tree(&mut store, empty_schema())?;

        let c0 = CommitObject::builder(schema_id, "test", "test", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;
        store.set_ref("refs/heads/main", c0_id)?;

        let report = gc_report(&store)?;
        // c0 + SchemaTree root + FileSchema leaf = 3.
        assert_eq!(report.reachable, 3);
        Ok(())
    }

    #[test]
    fn gc_marks_theory_ids_and_cst_complements_reachable() -> Result<(), VcsError> {
        use crate::object::CstComplementObject;
        use std::collections::BTreeMap;

        let mut store = MemStore::new();

        let schema_id = crate::tree::store_schema_as_tree(&mut store, empty_schema())?;

        // A theory object reached through commit.theory_ids.
        let theory = panproto_gat::Theory::new(
            "ThTest",
            vec![panproto_gat::Sort::simple("Vertex")],
            vec![],
            vec![],
        );
        let theory_id = store.put(&Object::Theory(Box::new(theory)))?;

        // A CST-complement object reached through commit.cst_complement_ids.
        let cst = CstComplementObject {
            data_id: ObjectId::from_bytes([77; 32]),
            cst_complement: vec![1, 2, 3],
        };
        let cst_id = store.put(&Object::CstComplement(cst))?;

        let mut theory_ids = BTreeMap::new();
        theory_ids.insert("ThTest".to_owned(), theory_id);

        let commit = CommitObject::builder(schema_id, "test", "test", "initial")
            .timestamp(100)
            .theory_ids(theory_ids)
            .cst_complement_ids(vec![cst_id])
            .build();
        let commit_id = store.put(&Object::Commit(commit))?;
        store.set_ref("refs/heads/main", commit_id)?;

        let report = gc(&mut store)?;
        assert!(!report.deleted.contains(&theory_id));
        assert!(!report.deleted.contains(&cst_id));
        assert!(store.has(&theory_id));
        assert!(store.has(&cst_id));
        Ok(())
    }

    #[test]
    fn gc_marks_data_complement_protocol_reachable() -> Result<(), VcsError> {
        use crate::object::{ComplementObject, DataSetObject};

        let mut store = MemStore::new();

        let schema_id = crate::tree::store_schema_as_tree(&mut store, empty_schema())?;

        // Store a protocol.
        let protocol = panproto_schema::Protocol {
            name: "test-proto".into(),
            ..Default::default()
        };
        let protocol_id = store.put(&Object::Protocol(Box::new(protocol)))?;

        // Store a dataset.
        let dataset = DataSetObject {
            schema_id,
            data: vec![1, 2, 3],
            record_count: 1,
            key: None,
            encoding: crate::object::PayloadEncoding::TREE,
        };
        let data_id = store.put(&Object::DataSet(dataset))?;

        // Store a complement.
        let complement = ComplementObject {
            migration_id: ObjectId::from_bytes([99; 32]),
            data_id,
            complement: vec![4, 5, 6],
            encoding: crate::object::PayloadEncoding::TREE,
        };
        let complement_id = store.put(&Object::Complement(complement))?;

        // Create commit referencing all three.
        let c0 = CommitObject::builder(schema_id, "test", "test", "initial")
            .timestamp(100)
            .protocol_id(protocol_id)
            .data_ids(vec![data_id])
            .complement_ids(vec![complement_id])
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;
        store.set_ref("refs/heads/main", c0_id)?;

        let reachable = mark_reachable(&store, &[c0_id])?;

        // All referenced objects should be reachable.
        assert!(reachable.contains(&protocol_id));
        assert!(reachable.contains(&data_id));
        assert!(reachable.contains(&complement_id));
        // DataSet references schema_id.
        assert!(reachable.contains(&schema_id));

        Ok(())
    }
}
