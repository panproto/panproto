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
//! byte-identical to what `panproto_project::ProjectBuilder::build`
//! emits from the same set of per-file schemas, so downstream consumers
//! of the assembled project schema see no behavioral change.

use std::path::{Path, PathBuf};

use panproto_schema::{Protocol, Schema, SchemaBuilder};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, FileSchemaObject, Object, SchemaTreeEntry, SchemaTreeObject};
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
            if path.as_os_str().is_empty() {
                // A wrapper tree leaf produced by
                // `store_schema_as_tree` carries no path; the flat
                // assembly path handles it via the single-file
                // shortcut in `assemble_from_files`.
                visit(Path::new(""), &file)
            } else {
                visit(&path, &file)
            }
        }
        Object::SchemaTree(tree) => match tree.as_ref() {
            SchemaTreeObject::SingleLeaf { file_schema_id } => {
                // Walk through the wrapped leaf without pushing a
                // name component; the leaf's own `path` supplies the
                // display path.
                walk_tree_inner(store, file_schema_id, prefix, visit)
            }
            SchemaTreeObject::Directory { .. } => {
                for (name, entry) in tree.sorted_entries() {
                    match entry {
                        SchemaTreeEntry::File(id) | SchemaTreeEntry::Tree(id) => {
                            prefix.push(name);
                            walk_tree_inner(store, id, prefix, visit)?;
                            prefix.pop();
                        }
                    }
                }
                Ok(())
            }
        },
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
/// what `panproto_project::ProjectBuilder::build` uses.
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
    // Collect (path, schema, cross_file_edges) triples in tree-walk order.
    let mut files: Vec<(PathBuf, Schema, Vec<panproto_schema::Edge>)> = Vec::new();
    walk_tree(store, root_id, |path, file| {
        files.push((
            path.to_path_buf(),
            file.schema.clone(),
            file.cross_file_edges.clone(),
        ));
        Ok(())
    })?;

    assemble_from_file_objects(protocol, &files)
}

/// Assemble a flat project schema from `(path, per_file_schema)` pairs
/// using the same path-prefixed coproduct convention as
/// `panproto_project::ProjectBuilder::build`.
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
    let triples: Vec<(PathBuf, Schema, Vec<panproto_schema::Edge>)> = files
        .iter()
        .map(|(p, s)| (p.clone(), s.clone(), Vec::new()))
        .collect();
    assemble_from_file_objects(protocol, &triples)
}

/// Assemble a flat project schema from per-file triples.
///
/// Each triple is `(path, per_file_schema, cross_file_edges)`; the
/// already-prefixed cross-file edges are added verbatim after per-file
/// vertices and edges have been prefixed and inserted.
///
/// # Errors
///
/// Returns [`VcsError::Other`] if the coproduct-schema builder rejects
/// an input vertex or edge.
pub fn assemble_from_file_objects(
    protocol: &Protocol,
    files: &[(PathBuf, Schema, Vec<panproto_schema::Edge>)],
) -> Result<Schema, VcsError> {
    // Single-file optimization matches ProjectBuilder::build.
    if files.len() == 1 && files[0].2.is_empty() {
        return Ok(files[0].1.clone());
    }

    let mut builder = SchemaBuilder::new(protocol);
    for (path, schema, _cross) in files {
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

        // Propagate the per-file pointing to the assembled schema.
        for entry in &schema.entries {
            builder = builder.entry(&format!("{prefix}::{entry}"));
        }
    }

    // Cross-file edges carry vertex names already prefixed with
    // their owning file, so they are added verbatim after the
    // per-file passes ensure both endpoints exist.
    for (_path, _schema, cross) in files {
        for edge in cross {
            builder = builder
                .edge(
                    edge.src.as_ref(),
                    edge.tgt.as_ref(),
                    edge.kind.as_ref(),
                    edge.name.as_deref(),
                )
                .map_err(|e| {
                    VcsError::Other(format!("cross-file edge {} -> {}: {e}", edge.src, edge.tgt))
                })?;
        }
    }

    let mut schema = builder
        .build()
        .map_err(|e| VcsError::Other(format!("assemble build: {e}")))?;

    // SchemaBuilder only accepts the core (vertices, edges,
    // constraints, entries) plumbing; the remaining fields of the
    // per-file schemas are stitched in here so the round-trip
    // preserves every semantically meaningful field.
    merge_enrichment_fields(&mut schema, files);

    Ok(schema)
}

/// Copy per-file enrichment fields into the assembled flat schema,
/// rewriting every vertex-id reference with the `<path>::<name>`
/// prefix convention used for the assembled vertices.
fn merge_enrichment_fields(
    out: &mut Schema,
    files: &[(PathBuf, Schema, Vec<panproto_schema::Edge>)],
) {
    use panproto_gat::Name;
    use panproto_schema::{Edge, HyperEdge, RecursionPoint, Span, UsageMode, Variant};

    let name = |prefix: &str, n: &Name| -> Name { Name::from(format!("{prefix}::{n}").as_str()) };

    let prefixed_edge = |prefix: &str, e: &Edge| -> Edge {
        Edge {
            src: name(prefix, &e.src),
            tgt: name(prefix, &e.tgt),
            kind: e.kind.clone(),
            name: e.name.as_ref().map(|n| name(prefix, n)),
        }
    };

    for (path, schema, _cross) in files {
        let prefix = path.display().to_string();

        // hyper_edges: id and signature vertex-id values are
        // schema-local, so rewrite both.
        for (id, he) in &schema.hyper_edges {
            let new_id = name(&prefix, id);
            let signature = he
                .signature
                .iter()
                .map(|(lbl, vid)| (lbl.clone(), name(&prefix, vid)))
                .collect();
            out.hyper_edges.insert(
                new_id.clone(),
                HyperEdge {
                    id: new_id,
                    kind: he.kind.clone(),
                    signature,
                    parent_label: he.parent_label.clone(),
                },
            );
        }

        // required: key is a vertex id, values are edges.
        for (vid, edges) in &schema.required {
            let key = name(&prefix, vid);
            let rewritten: Vec<Edge> = edges.iter().map(|e| prefixed_edge(&prefix, e)).collect();
            out.required.entry(key).or_default().extend(rewritten);
        }

        // nsids: key is a vertex id, value is the nsid itself (not a vertex id).
        for (vid, nsid) in &schema.nsids {
            out.nsids.insert(name(&prefix, vid), nsid.clone());
        }

        // variants: key is the parent vertex id.
        for (vid, variants) in &schema.variants {
            let key = name(&prefix, vid);
            let rewritten: Vec<Variant> = variants
                .iter()
                .map(|v| Variant {
                    id: name(&prefix, &v.id),
                    parent_vertex: name(&prefix, &v.parent_vertex),
                    tag: v.tag.clone(),
                })
                .collect();
            out.variants.entry(key).or_default().extend(rewritten);
        }

        // orderings: keys are edges.
        for (edge, pos) in &schema.orderings {
            out.orderings.insert(prefixed_edge(&prefix, edge), *pos);
        }

        // recursion_points: mu_id and target_vertex are both vertex ids.
        for (mu, rp) in &schema.recursion_points {
            let new_mu = name(&prefix, mu);
            out.recursion_points.insert(
                new_mu.clone(),
                RecursionPoint {
                    mu_id: new_mu,
                    target_vertex: name(&prefix, &rp.target_vertex),
                },
            );
        }

        // spans: id is a span id, left/right are vertex ids.
        for (id, span) in &schema.spans {
            let new_id = name(&prefix, id);
            out.spans.insert(
                new_id.clone(),
                Span {
                    id: new_id,
                    left: name(&prefix, &span.left),
                    right: name(&prefix, &span.right),
                },
            );
        }

        // usage_modes: keys are edges.
        for (edge, mode) in &schema.usage_modes {
            let cloned: UsageMode = mode.clone();
            out.usage_modes.insert(prefixed_edge(&prefix, edge), cloned);
        }

        // nominal: keys are vertex ids.
        for (vid, flag) in &schema.nominal {
            out.nominal.insert(name(&prefix, vid), *flag);
        }

        // coercions: keys are (kind, kind) not vertex ids.
        for (key, spec) in &schema.coercions {
            out.coercions.insert(key.clone(), spec.clone());
        }

        // mergers / defaults: keys are vertex ids.
        for (vid, expr) in &schema.mergers {
            out.mergers.insert(name(&prefix, vid), expr.clone());
        }
        for (vid, expr) in &schema.defaults {
            out.defaults.insert(name(&prefix, vid), expr.clone());
        }

        // policies: keys are sort names (not vertex ids).
        for (sort, expr) in &schema.policies {
            out.policies.insert(sort.clone(), expr.clone());
        }
    }
}

/// Resolve a commit's `schema_id` to a flat [`Schema`].
///
/// The `schema_id` must point at an [`Object::SchemaTree`] root.
/// The tree is walked and assembled via [`assemble_schema`] using
/// [`project_coproduct_protocol`]. Any other target object type is
/// a [`VcsError::WrongObjectType`].
///
/// # Errors
///
/// Returns [`VcsError::ObjectNotFound`] if the tree root is missing,
/// [`VcsError::WrongObjectType`] if the target is not a
/// `schema_tree`, or a tree-walk error from [`assemble_schema`] if
/// assembly fails.
pub fn resolve_commit_schema_dyn(
    store: &dyn Store,
    commit: &CommitObject,
) -> Result<Schema, VcsError> {
    match store.get(&commit.schema_id)? {
        Object::SchemaTree(_) => {
            let proto = project_coproduct_protocol();
            assemble_schema_dyn(store, &commit.schema_id, &proto)
        }
        other => Err(VcsError::WrongObjectType {
            expected: "schema_tree",
            found: other.type_name(),
        }),
    }
}

/// Like [`assemble_schema`] but accepts a `&dyn Store` for callers
/// that hold a trait object (e.g., [`crate::rebase`] and
/// [`crate::cherry_pick`]).
///
/// # Errors
///
/// See [`assemble_schema`].
pub fn assemble_schema_dyn(
    store: &dyn Store,
    root_id: &ObjectId,
    protocol: &Protocol,
) -> Result<Schema, VcsError> {
    let mut files: Vec<(PathBuf, Schema, Vec<panproto_schema::Edge>)> = Vec::new();
    walk_tree_dyn(store, root_id, &mut PathBuf::new(), &mut |path, file| {
        files.push((
            path.to_path_buf(),
            file.schema.clone(),
            file.cross_file_edges.clone(),
        ));
        Ok(())
    })?;
    assemble_from_file_objects(protocol, &files)
}

fn walk_tree_dyn(
    store: &dyn Store,
    node_id: &ObjectId,
    prefix: &mut PathBuf,
    visit: &mut dyn FnMut(&Path, &FileSchemaObject) -> Result<(), VcsError>,
) -> Result<(), VcsError> {
    match store.get(node_id)? {
        Object::FileSchema(file) => {
            let path = if prefix.as_os_str().is_empty() {
                PathBuf::from(&file.path)
            } else {
                prefix.clone()
            };
            visit(&path, &file)
        }
        Object::SchemaTree(tree) => match tree.as_ref() {
            SchemaTreeObject::SingleLeaf { file_schema_id } => {
                walk_tree_dyn(store, file_schema_id, prefix, visit)
            }
            SchemaTreeObject::Directory { .. } => {
                for (name, entry) in tree.sorted_entries() {
                    match entry {
                        SchemaTreeEntry::File(id) | SchemaTreeEntry::Tree(id) => {
                            prefix.push(name);
                            walk_tree_dyn(store, id, prefix, visit)?;
                            prefix.pop();
                        }
                    }
                }
                Ok(())
            }
        },
        other => Err(VcsError::WrongObjectType {
            expected: "file_schema or schema_tree",
            found: other.type_name(),
        }),
    }
}

/// Resolve a commit's schema-tree root to a flat [`Schema`].
///
/// Generic counterpart of [`resolve_commit_schema_dyn`] that takes a
/// concrete [`Store`] implementation. Prefer this variant when the
/// caller owns the store directly; use the `_dyn` variant when the
/// store is behind a `&dyn Store`.
///
/// # Errors
///
/// See [`resolve_commit_schema_dyn`].
pub fn resolve_commit_schema<S: Store>(
    store: &S,
    commit: &CommitObject,
) -> Result<Schema, VcsError> {
    match store.get(&commit.schema_id)? {
        Object::SchemaTree(_) => {
            let proto = project_coproduct_protocol();
            assemble_schema(store, &commit.schema_id, &proto)
        }
        other => Err(VcsError::WrongObjectType {
            expected: "schema_tree",
            found: other.type_name(),
        }),
    }
}

/// The standard project-coproduct protocol used by both
/// `panproto_project::ProjectBuilder::build` and [`assemble_schema`].
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

/// Store a flat [`Schema`] as a single-leaf schema tree.
///
/// Used by commit paths that produce a single assembled or merged
/// [`Schema`] (e.g., merge, rebase, cherry-pick, and the CLI's
/// single-file `schema commit`). The resulting tree is a
/// [`SchemaTreeObject::SingleLeaf`] pointing at the stored
/// [`FileSchemaObject`]; that shape has no name slot and therefore
/// cannot collide with any real project path, so mixed-use stores
/// cannot produce ambiguous walks.
///
/// A subsequent [`assemble_schema`] call returns the input schema
/// unchanged (single-file optimization in [`assemble_from_files`]).
///
/// # Errors
///
/// Returns [`VcsError`] if storing either the leaf or the root tree
/// fails.
pub fn store_schema_as_tree(store: &mut dyn Store, schema: Schema) -> Result<ObjectId, VcsError> {
    let protocol = schema.protocol.clone();
    let file = FileSchemaObject {
        path: String::new(),
        protocol,
        schema,
        cross_file_edges: Vec::new(),
    };
    let leaf_id = store.put(&Object::FileSchema(Box::new(file)))?;
    let tree = SchemaTreeObject::SingleLeaf {
        file_schema_id: leaf_id,
    };
    store.put(&Object::SchemaTree(Box::new(tree)))
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

    fn insert(
        node: &mut Node,
        components: &[String],
        full_path: &str,
        leaf: ObjectId,
    ) -> Result<(), VcsError> {
        match node {
            Node::Leaf(_) => Err(VcsError::DuplicatePath {
                path: full_path.to_owned(),
            }),
            Node::Tree(entries) => {
                let Some((head, tail)) = components.split_first() else {
                    return Ok(());
                };
                if let Some(pos) = entries.iter().position(|(n, _)| n == head) {
                    if tail.is_empty() {
                        // Either a leaf already lives here (duplicate)
                        // or a subtree lives here whose path prefix
                        // clashes with a file of the same name.
                        return Err(VcsError::DuplicatePath {
                            path: full_path.to_owned(),
                        });
                    }
                    insert(&mut entries[pos].1, tail, full_path, leaf)
                } else if tail.is_empty() {
                    entries.push((head.clone(), Node::Leaf(leaf)));
                    Ok(())
                } else {
                    let mut child = Node::Tree(Vec::new());
                    insert(&mut child, tail, full_path, leaf)?;
                    entries.push((head.clone(), child));
                    Ok(())
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
                let tree = SchemaTreeObject::Directory { entries: out };
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
        if components.is_empty() || components.iter().any(String::is_empty) {
            return Err(VcsError::EmptyPath);
        }
        let display = path.display().to_string();
        insert(&mut root, &components, &display, id)?;
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
            cross_file_edges: Vec::new(),
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
    fn duplicate_path_is_an_error() {
        let mut store = MemStore::new();
        let a = file_schema("a.rs", "a");
        let b = file_schema("a.rs", "a2");
        let err = build_schema_tree(
            &mut store,
            vec![(PathBuf::from("a.rs"), a), (PathBuf::from("a.rs"), b)],
        )
        .unwrap_err();
        assert!(matches!(err, VcsError::DuplicatePath { .. }));
    }

    #[test]
    fn empty_path_component_is_an_error() {
        let mut store = MemStore::new();
        let leaf_id = ObjectId::ZERO;
        // An explicitly empty path, and a path containing an empty
        // component, both must error rather than be silently dropped.
        let err = build_tree_from_leaves(&mut store, vec![(PathBuf::new(), leaf_id)]).unwrap_err();
        assert!(matches!(err, VcsError::EmptyPath));
    }

    #[test]
    fn empty_files_empty_tree() {
        let mut store = MemStore::new();
        let root = build_schema_tree(&mut store, vec![]).unwrap();
        match store.get(&root).unwrap() {
            Object::SchemaTree(t) => match *t {
                SchemaTreeObject::Directory { entries } => assert!(entries.is_empty()),
                SchemaTreeObject::SingleLeaf { .. } => {
                    panic!("expected Directory, got SingleLeaf")
                }
            },
            other => panic!("expected schema_tree, got {}", other.type_name()),
        }
    }

    #[test]
    fn resolve_commit_schema_walks_tree() {
        use crate::object::CommitObject;

        let mut store = MemStore::new();

        let file = file_schema("only.rs", "only");
        let root = build_schema_tree(&mut store, vec![(PathBuf::from("only.rs"), file)]).unwrap();
        let tree_commit = CommitObject::builder(root, "p", "a", "m").build();
        let resolved = resolve_commit_schema(&store, &tree_commit).unwrap();
        assert_eq!(resolved.vertices.len(), 1);
    }

    #[test]
    fn store_schema_as_tree_round_trip() {
        use crate::object::CommitObject;

        let mut store = MemStore::new();
        let schema = tiny_schema("round_trip");
        let root = store_schema_as_tree(&mut store, schema.clone()).unwrap();
        let commit = CommitObject::builder(root, "p", "a", "m").build();
        let resolved = resolve_commit_schema(&store, &commit).unwrap();
        assert_eq!(resolved.vertices.len(), schema.vertices.len());
    }

    #[test]
    fn single_leaf_wrapper_walks_once_without_name_component() {
        let mut store = MemStore::new();
        let schema = tiny_schema("wrapped");
        let root = store_schema_as_tree(&mut store, schema).unwrap();

        let mut seen: Vec<String> = Vec::new();
        walk_tree(&store, &root, |p, _f| {
            seen.push(p.display().to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen.len(), 1);
        // The wrapper leaf has no name component, so the walker
        // reports an empty path.
        assert_eq!(seen[0], "");
    }

    #[test]
    fn schema_tree_deserializes_in_canonical_order() {
        use crate::object::{SchemaTreeEntry, SchemaTreeObject};

        // Construct an adversarial wire form whose entries are NOT
        // sorted; a well-behaved consumer must still see them in
        // canonical order via `sorted_entries`.
        let id_a = ObjectId::from_bytes([1; 32]);
        let id_b = ObjectId::from_bytes([2; 32]);
        let unsorted = SchemaTreeObject::Directory {
            entries: vec![
                ("z".to_owned(), SchemaTreeEntry::File(id_a)),
                ("a".to_owned(), SchemaTreeEntry::File(id_b)),
            ],
        };
        let bytes = rmp_serde::to_vec(&unsorted).unwrap();
        let round: SchemaTreeObject = rmp_serde::from_slice(&bytes).unwrap();
        let sorted = round.sorted_entries();
        assert_eq!(sorted[0].0, "a");
        assert_eq!(sorted[1].0, "z");
    }

    fn base_schema(v: &str) -> Schema {
        use panproto_gat::Name;
        use std::collections::HashMap;
        let mut verts = HashMap::new();
        verts.insert(
            Name::from(v),
            panproto_schema::Vertex {
                id: Name::from(v),
                kind: Name::from("record"),
                nsid: None,
            },
        );
        Schema {
            protocol: "project".into(),
            vertices: verts,
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

    fn rich_schema_a() -> Schema {
        use panproto_gat::Name;
        use panproto_schema::{
            CoercionSpec, Edge, HyperEdge, RecursionPoint, Span, UsageMode, Variant,
        };
        let mut s = base_schema("a");
        s.entries.push(Name::from("a"));
        s.hyper_edges.insert(
            Name::from("he"),
            HyperEdge {
                id: Name::from("he"),
                kind: Name::from("hedge"),
                signature: std::iter::once((Name::from("lbl"), Name::from("a"))).collect(),
                parent_label: Name::from("lbl"),
            },
        );
        s.nsids.insert(Name::from("a"), Name::from("ns.a"));
        s.variants.insert(
            Name::from("a"),
            vec![Variant {
                id: Name::from("a_v1"),
                parent_vertex: Name::from("a"),
                tag: Some(Name::from("v1")),
            }],
        );
        s.recursion_points.insert(
            Name::from("a"),
            RecursionPoint {
                mu_id: Name::from("a"),
                target_vertex: Name::from("a"),
            },
        );
        s.spans.insert(
            Name::from("sp"),
            Span {
                id: Name::from("sp"),
                left: Name::from("a"),
                right: Name::from("a"),
            },
        );
        s.nominal.insert(Name::from("a"), true);
        s.coercions.insert(
            (Name::from("int"), Name::from("str")),
            CoercionSpec {
                forward: panproto_expr::Expr::Lit(panproto_expr::Literal::Int(0)),
                inverse: None,
                class: panproto_gat::CoercionClass::Iso,
            },
        );
        s.mergers.insert(
            Name::from("a"),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Int(1)),
        );
        s.defaults.insert(
            Name::from("a"),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Int(2)),
        );
        s.policies.insert(
            Name::from("p1"),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Int(3)),
        );
        let edge_a = Edge {
            src: Name::from("a"),
            tgt: Name::from("a"),
            kind: Name::from("loop"),
            name: None,
        };
        s.orderings.insert(edge_a.clone(), 7);
        s.usage_modes.insert(edge_a.clone(), UsageMode::Linear);
        s.required.insert(Name::from("a"), vec![edge_a]);
        s
    }

    #[test]
    fn assemble_preserves_every_schema_field() {
        use panproto_gat::Name;

        let s_a = rich_schema_a();
        let s_b = base_schema("b");

        let file_a = FileSchemaObject {
            path: "x.rs".to_owned(),
            protocol: "project".to_owned(),
            schema: s_a,
            cross_file_edges: Vec::new(),
        };
        let file_b = FileSchemaObject {
            path: "y.rs".to_owned(),
            protocol: "project".to_owned(),
            schema: s_b,
            cross_file_edges: Vec::new(),
        };

        let mut store = MemStore::new();
        let root = build_schema_tree(
            &mut store,
            vec![
                (PathBuf::from("x.rs"), file_a),
                (PathBuf::from("y.rs"), file_b),
            ],
        )
        .unwrap();

        let proto = project_coproduct_protocol();
        let flat = assemble_schema(&store, &root, &proto).unwrap();

        // Every enrichment field survives with prefixed keys.
        assert!(flat.entries.iter().any(|n| n.as_ref() == "x.rs::a"));
        assert!(flat.hyper_edges.contains_key(&Name::from("x.rs::he")));
        assert_eq!(
            flat.nsids.get(&Name::from("x.rs::a")).map(Name::as_ref),
            Some("ns.a")
        );
        assert!(flat.variants.contains_key(&Name::from("x.rs::a")));
        assert!(flat.recursion_points.contains_key(&Name::from("x.rs::a")));
        assert!(flat.spans.contains_key(&Name::from("x.rs::sp")));
        assert_eq!(flat.nominal.get(&Name::from("x.rs::a")), Some(&true));
        assert!(
            flat.coercions
                .contains_key(&(Name::from("int"), Name::from("str")))
        );
        assert!(flat.mergers.contains_key(&Name::from("x.rs::a")));
        assert!(flat.defaults.contains_key(&Name::from("x.rs::a")));
        assert!(flat.policies.contains_key(&Name::from("p1")));
        assert!(flat.required.contains_key(&Name::from("x.rs::a")));
        assert!(!flat.orderings.is_empty());
        assert!(!flat.usage_modes.is_empty());
    }

    #[test]
    fn walk_tree_visits_entries_in_lexicographic_order() {
        use crate::object::{SchemaTreeEntry, SchemaTreeObject};

        // Build a Directory whose entries are in REVERSE lex order on
        // the wire. `walk_tree` must still report them in lex order.
        let mut store = MemStore::new();
        let leaf_a = store
            .put(&Object::FileSchema(Box::new(file_schema("a.rs", "av"))))
            .unwrap();
        let leaf_m = store
            .put(&Object::FileSchema(Box::new(file_schema("m.rs", "mv"))))
            .unwrap();
        let leaf_z = store
            .put(&Object::FileSchema(Box::new(file_schema("z.rs", "zv"))))
            .unwrap();

        let unsorted = SchemaTreeObject::Directory {
            entries: vec![
                ("z.rs".to_owned(), SchemaTreeEntry::File(leaf_z)),
                ("m.rs".to_owned(), SchemaTreeEntry::File(leaf_m)),
                ("a.rs".to_owned(), SchemaTreeEntry::File(leaf_a)),
            ],
        };
        let root = store.put(&Object::SchemaTree(Box::new(unsorted))).unwrap();

        let mut seen: Vec<String> = Vec::new();
        walk_tree(&store, &root, |path, _| {
            seen.push(path.display().to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, vec!["a.rs", "m.rs", "z.rs"]);
    }

    #[test]
    fn walk_tree_broken_store_returns_error() {
        // A `SchemaTree` referencing a leaf id that the store cannot
        // resolve must surface a `VcsError` rather than panic.
        use crate::object::{SchemaTreeEntry, SchemaTreeObject};
        let mut store = MemStore::new();
        let ghost = ObjectId::from_bytes([255; 32]);
        let tree = SchemaTreeObject::Directory {
            entries: vec![("gone.rs".to_owned(), SchemaTreeEntry::File(ghost))],
        };
        let root = store.put(&Object::SchemaTree(Box::new(tree))).unwrap();
        let result = walk_tree(&store, &root, |_, _| Ok(()));
        assert!(result.is_err());
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
