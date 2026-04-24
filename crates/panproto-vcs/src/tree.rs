//! Merkle tree of per-file schemas.
//!
//! A project schema is stored as a tree of [`FileSchemaObject`] leaves
//! joined by [`SchemaTreeObject`] inner nodes, mirroring git's blob/tree
//! model. Each file's schema is content-addressed independently so an
//! unchanged file reuses its [`ObjectId`] across commits; only the
//! [`SchemaTreeObject`] nodes on the path from the changed file to the
//! root need to be rewritten.
//!
//! The flat assembled form produced by [`assemble_schema`] is
//! byte-identical to what [`panproto_project::ProjectBuilder::build`]
//! emits from the same set of per-file schemas, so downstream consumers
//! of the assembled project schema see no behavioral change.

use std::path::{Path, PathBuf};

use panproto_schema::{Protocol, Schema, SchemaBuilder};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{FileSchemaObject, Object, SchemaTreeEntry, SchemaTreeObject};
use crate::store::Store;

/// Walk a schema tree rooted at `root_id` depth-first, invoking
/// `visit` for every [`FileSchemaObject`] leaf with its accumulated
/// path prefix.
///
/// The path prefix is the sequence of tree-entry names from the root
/// to the leaf joined by the platform-agnostic forward-slash
/// separator, matching how project paths are stored on disk.
///
/// # Errors
///
/// Returns [`VcsError::ObjectNotFound`] if any referenced object is
/// missing, or [`VcsError::WrongObjectType`] if an inner reference
/// resolves to an object that is not a `FileSchema` or `SchemaTree`.
pub fn walk_tree<S, F>(store: &S, root_id: &ObjectId, mut visit: F) -> Result<(), VcsError>
where
    S: Store,
    F: FnMut(&Path, &FileSchemaObject) -> Result<(), VcsError>,
{
    walk_tree_inner(store, root_id, &mut PathBuf::new(), &mut visit)
}

fn walk_tree_inner<S, F>(
    store: &S,
    node_id: &ObjectId,
    prefix: &mut PathBuf,
    visit: &mut F,
) -> Result<(), VcsError>
where
    S: Store,
    F: FnMut(&Path, &FileSchemaObject) -> Result<(), VcsError>,
{
    match store.get(node_id)? {
        Object::FileSchema(file) => {
            let path = if prefix.as_os_str().is_empty() {
                PathBuf::from(&file.path)
            } else {
                prefix.clone()
            };
            visit(&path, &file)
        }
        Object::SchemaTree(tree) => {
            for (name, entry) in &tree.entries {
                prefix.push(name);
                let child_id = match entry {
                    SchemaTreeEntry::File(id) | SchemaTreeEntry::Tree(id) => id,
                };
                walk_tree_inner(store, child_id, prefix, visit)?;
                prefix.pop();
            }
            Ok(())
        }
        other => Err(VcsError::WrongObjectType {
            expected: "file_schema or schema_tree",
            found: other.type_name(),
        }),
    }
}

/// Assemble a flat project schema from a schema tree.
///
/// Walks the tree rooted at `root_id` and returns the schema that
/// would have been produced by running the project-coproduct
/// construction over the same per-file schemas.
///
/// `protocol` is the coproduct protocol used for the assembled
/// schema; callers usually pass the "project" protocol that matches
/// what [`panproto_project::ProjectBuilder::build`] uses.
///
/// # Errors
///
/// Returns [`VcsError`] if tree walk fails, or a coproduct-assembly
/// error wrapped as [`VcsError::Other`] if vertex/edge insertion
/// violates the coproduct protocol's rules.
pub fn assemble_schema<S: Store>(
    store: &S,
    root_id: &ObjectId,
    protocol: &Protocol,
) -> Result<Schema, VcsError> {
    // Collect (path, schema) pairs in tree-walk order.
    let mut files: Vec<(PathBuf, Schema)> = Vec::new();
    walk_tree(store, root_id, |path, file| {
        files.push((path.to_path_buf(), file.schema.clone()));
        Ok(())
    })?;

    assemble_from_files(protocol, &files)
}

/// Assemble a flat project schema from `(path, per_file_schema)` pairs
/// using the same path-prefixed coproduct convention as
/// [`panproto_project::ProjectBuilder::build`].
///
/// Exposed so that migration tooling can reuse the same assembly path
/// without storing intermediate objects.
///
/// # Errors
///
/// Returns [`VcsError::Other`] if the coproduct-schema builder rejects
/// an input vertex or edge.
pub fn assemble_from_files(
    protocol: &Protocol,
    files: &[(PathBuf, Schema)],
) -> Result<Schema, VcsError> {
    // Single-file optimization matches ProjectBuilder::build.
    if files.len() == 1 {
        return Ok(files[0].1.clone());
    }

    let mut builder = SchemaBuilder::new(protocol);
    for (path, schema) in files {
        let prefix = path.display().to_string();

        for (name, vertex) in &schema.vertices {
            let prefixed_name = format!("{prefix}::{name}");
            builder = builder
                .vertex(&prefixed_name, vertex.kind.as_ref(), None)
                .map_err(|e| VcsError::Other(format!("vertex {prefixed_name}: {e}")))?;

            if let Some(constraints) = schema.constraints.get(name) {
                for c in constraints {
                    builder = builder.constraint(&prefixed_name, c.sort.as_ref(), &c.value);
                }
            }
        }

        for edge in schema.edges.keys() {
            let prefixed_src = format!("{prefix}::{}", edge.src);
            let prefixed_tgt = format!("{prefix}::{}", edge.tgt);
            let edge_name = edge.name.as_ref().map(|n| format!("{prefix}::{n}"));
            builder = builder
                .edge(
                    &prefixed_src,
                    &prefixed_tgt,
                    edge.kind.as_ref(),
                    edge_name.as_deref(),
                )
                .map_err(|e| {
                    VcsError::Other(format!("edge {prefixed_src} -> {prefixed_tgt}: {e}"))
                })?;
        }
    }

    builder
        .build()
        .map_err(|e| VcsError::Other(format!("assemble build: {e}")))
}

/// The standard project-coproduct protocol used by both
/// [`panproto_project::ProjectBuilder::build`] and [`assemble_schema`].
#[must_use]
pub fn project_coproduct_protocol() -> Protocol {
    Protocol {
        name: "project".into(),
        schema_theory: "ThProjectSchema".into(),
        instance_theory: "ThProjectInstance".into(),
        schema_composition: None,
        instance_composition: None,
        edge_rules: vec![],
        obj_kinds: vec![],
        constraint_sorts: vec![],
        has_order: true,
        has_coproducts: false,
        has_recursion: false,
        has_causal: false,
        nominal_identity: false,
        has_defaults: false,
        has_coercions: false,
        has_mergers: false,
        has_policies: false,
    }
}

/// Build a schema tree from a flat list of path-keyed file schemas.
///
/// Stores [`FileSchemaObject`] leaves and
/// [`SchemaTreeObject`] inner nodes and returning the root tree's
/// [`ObjectId`].
///
/// Directory structure is inferred from the path components. Each
/// intermediate directory becomes a [`SchemaTreeObject`] with entries
/// sorted lexicographically so the resulting [`ObjectId`] is stable
/// regardless of the input ordering.
///
/// # Errors
///
/// Returns [`VcsError`] if storing any object fails.
pub fn build_schema_tree<S: Store>(
    store: &mut S,
    files: Vec<(PathBuf, FileSchemaObject)>,
) -> Result<ObjectId, VcsError> {
    // Store each file schema leaf.
    let mut leaves: Vec<(PathBuf, ObjectId)> = Vec::with_capacity(files.len());
    for (path, file) in files {
        let id = store.put(&Object::FileSchema(Box::new(file)))?;
        leaves.push((path, id));
    }

    build_tree_from_leaves(store, leaves)
}

/// Build a schema tree from pre-stored `FileSchema` leaves.
///
/// Callers that have already deduplicated leaf [`ObjectId`]s (e.g., a git
/// importer reusing blob-OID-keyed cache entries) should use this
/// variant.
///
/// # Errors
///
/// Returns [`VcsError`] if storing a `SchemaTree` object fails.
pub fn build_tree_from_leaves<S: Store>(
    store: &mut S,
    leaves: Vec<(PathBuf, ObjectId)>,
) -> Result<ObjectId, VcsError> {
    // Group by top-level component: "name" -> either a leaf ObjectId
    // (if the path has only one component) or a deeper nested set.
    //
    // Represent the subtree incrementally with an in-memory node
    // structure, then emit SchemaTree objects bottom-up.
    enum Node {
        Leaf(ObjectId),
        Tree(Vec<(String, Self)>),
    }

    fn insert(node: &mut Node, components: &[String], leaf: ObjectId) {
        match node {
            Node::Leaf(_) => {
                // Collision: a leaf already occupies this path. In
                // practice this only happens if the same path is
                // inserted twice; replace deterministically.
                *node = Node::Leaf(leaf);
            }
            Node::Tree(entries) => {
                let Some((head, tail)) = components.split_first() else {
                    return;
                };
                if let Some(pos) = entries.iter().position(|(n, _)| n == head) {
                    if tail.is_empty() {
                        entries[pos].1 = Node::Leaf(leaf);
                    } else {
                        insert(&mut entries[pos].1, tail, leaf);
                    }
                } else if tail.is_empty() {
                    entries.push((head.clone(), Node::Leaf(leaf)));
                } else {
                    let mut child = Node::Tree(Vec::new());
                    insert(&mut child, tail, leaf);
                    entries.push((head.clone(), child));
                }
            }
        }
    }

    fn emit<S: Store>(store: &mut S, node: Node) -> Result<(ObjectId, bool), VcsError> {
        match node {
            Node::Leaf(id) => Ok((id, true)),
            Node::Tree(entries) => {
                let mut out: Vec<(String, SchemaTreeEntry)> = Vec::with_capacity(entries.len());
                for (name, child) in entries {
                    let (id, is_leaf) = emit(store, child)?;
                    let entry = if is_leaf {
                        SchemaTreeEntry::File(id)
                    } else {
                        SchemaTreeEntry::Tree(id)
                    };
                    out.push((name, entry));
                }
                out.sort_by(|a, b| a.0.cmp(&b.0));
                let tree = SchemaTreeObject { entries: out };
                let id = store.put(&Object::SchemaTree(Box::new(tree)))?;
                Ok((id, false))
            }
        }
    }

    let mut root = Node::Tree(Vec::new());
    for (path, id) in leaves {
        let components: Vec<String> = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if components.is_empty() {
            continue;
        }
        insert(&mut root, &components, id);
    }

    let (root_id, _) = emit(store, root)?;
    Ok(root_id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::mem_store::MemStore;
    use panproto_schema::SchemaBuilder;

    fn tiny_schema(vertex: &str) -> Schema {
        let protocol = project_coproduct_protocol();
        SchemaBuilder::new(&protocol)
            .vertex(vertex, "record", None)
            .unwrap()
            .build()
            .unwrap()
    }

    fn file_schema(path: &str, vertex: &str) -> FileSchemaObject {
        FileSchemaObject {
            path: path.to_owned(),
            protocol: "project".to_owned(),
            schema: tiny_schema(vertex),
        }
    }

    #[test]
    fn single_file_round_trip() {
        let mut store = MemStore::new();
        let file = file_schema("src/main.rs", "main");
        let root =
            build_schema_tree(&mut store, vec![(PathBuf::from("src/main.rs"), file)]).unwrap();

        let mut seen: Vec<String> = Vec::new();
        walk_tree(&store, &root, |p, f| {
            seen.push(format!("{}->{}", p.display(), f.path));
            Ok(())
        })
        .unwrap();
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn nested_tree_round_trip() {
        let mut store = MemStore::new();
        let files = vec![
            (PathBuf::from("a/b/x.rs"), file_schema("a/b/x.rs", "x")),
            (PathBuf::from("a/b/y.rs"), file_schema("a/b/y.rs", "y")),
            (PathBuf::from("a/z.rs"), file_schema("a/z.rs", "z")),
        ];
        let root = build_schema_tree(&mut store, files).unwrap();
        let mut count = 0usize;
        walk_tree(&store, &root, |_, _| {
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn deterministic_regardless_of_order() {
        let paths = ["a.rs", "b.rs", "c/d.rs"];
        let mut first = MemStore::new();
        let mut second = MemStore::new();

        let files_a: Vec<(PathBuf, FileSchemaObject)> = paths
            .iter()
            .map(|p| (PathBuf::from(p), file_schema(p, "v")))
            .collect();
        let mut files_b = files_a.clone();
        files_b.reverse();

        let root_a = build_schema_tree(&mut first, files_a).unwrap();
        let root_b = build_schema_tree(&mut second, files_b).unwrap();
        assert_eq!(root_a, root_b);
    }

    #[test]
    fn empty_files_empty_tree() {
        let mut store = MemStore::new();
        let root = build_schema_tree(&mut store, vec![]).unwrap();
        match store.get(&root).unwrap() {
            Object::SchemaTree(t) => assert!(t.entries.is_empty()),
            other => panic!("expected schema_tree, got {}", other.type_name()),
        }
    }

    #[test]
    fn assemble_matches_single_file() {
        let mut store = MemStore::new();
        let file = file_schema("lonely.rs", "only");
        let expected = file.schema.clone();
        let root = build_schema_tree(&mut store, vec![(PathBuf::from("lonely.rs"), file)]).unwrap();
        let proto = project_coproduct_protocol();
        let got = assemble_schema(&store, &root, &proto).unwrap();
        assert_eq!(got.vertices.len(), expected.vertices.len());
    }
}
