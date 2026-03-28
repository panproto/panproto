//! W-type instance representation and the `wtype_restrict` pipeline.
//!
//! A [`WInstance`] is a tree-shaped data instance conforming to a schema.
//! The restrict operation (`wtype_restrict`) is a fused single-pass pipeline
//! that projects a W-type instance along a migration mapping.
//!
//! The pipeline fuses four concerns into one BFS traversal:
//! 1. Anchor survival check — does this node's schema vertex survive?
//! 2. Reachability — is this node reachable from the root?
//! 3. Ancestor contraction — who is the nearest surviving ancestor?
//! 4. Edge resolution — what edge connects the contracted arc?
//!
//! Fan reconstruction (step 5) runs as a separate pass since it operates
//! on the original fan list, not the BFS tree.
//!
//! The five individual step functions are retained for testing and debugging.

use std::collections::{HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::error::RestrictError;
use crate::fan::Fan;
use crate::metadata::Node;
use crate::value::Value;

/// A compiled migration specification (minimal version for panproto-inst).
///
/// The full `CompiledMigration` lives in `panproto-mig`. This type provides
/// the subset of fields that `wtype_restrict` and `functor_restrict` need.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompiledMigration {
    /// Vertices that survive the migration.
    pub surviving_verts: HashSet<Name>,
    /// Edges that survive the migration.
    pub surviving_edges: HashSet<Edge>,
    /// Vertex remapping: source vertex ID to target vertex ID.
    pub vertex_remap: HashMap<Name, Name>,
    /// Edge remapping: source edge to target edge.
    pub edge_remap: HashMap<Edge, Edge>,
    /// Binary contraction resolver: (`src_anchor`, `tgt_anchor`) to resolved edge.
    pub resolver: HashMap<(Name, Name), Edge>,
    /// Hyper-edge contraction resolver.
    pub hyper_resolver: HashMap<Name, (Name, HashMap<Name, Name>)>,
    /// Value-level field transforms applied to surviving nodes' `extra_fields`.
    ///
    /// Keyed by source vertex anchor. Each entry is a list of field operations
    /// applied in order after the node survives and is remapped.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub field_transforms: HashMap<Name, Vec<FieldTransform>>,
    /// Value-dependent survival predicates.
    ///
    /// During `wtype_restrict`, after checking that a node's anchor vertex
    /// is in `surviving_verts`, the conditional survival predicate (if any)
    /// is evaluated with the node's `extra_fields` bound as variables.
    /// If the predicate evaluates to `false`, the node is dropped despite
    /// its anchor surviving.
    ///
    /// This enables value-dependent filtering: "keep this vertex only if
    /// attrs.level == 2" (matchAttrs), or "keep this vertex only if
    /// class contains 'u-url'" (matchAttrsAll).
    ///
    /// Categorically, this is a refinement of the survival predicate
    /// from a structural predicate (vertex set membership) to a
    /// value-dependent predicate (vertex set membership AND value predicate).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub conditional_survival: HashMap<Name, panproto_expr::Expr>,
}

/// A value-level transformation on a node's `extra_fields`.
///
/// These are applied during `wtype_restrict` after structural operations
/// (anchor remapping, vertex survival). They enable the instance pipeline
/// to handle value-dependent migrations (attribute renames, drops, value
/// transforms) that go beyond pure structural schema changes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldTransform {
    /// Rename a field key: `old_key` → `new_key`.
    RenameField {
        /// The current field name.
        old_key: String,
        /// The new field name.
        new_key: String,
    },
    /// Drop a field by key.
    DropField {
        /// The field to remove.
        key: String,
    },
    /// Add a field with a constant default value.
    AddField {
        /// The field name to add.
        key: String,
        /// The default value.
        value: Value,
    },
    /// Keep only the specified fields (all others are dropped).
    KeepFields {
        /// The field names to retain.
        keys: Vec<String>,
    },
    /// Apply an expression to a field's value, storing the result.
    ApplyExpr {
        /// The field whose value is transformed.
        key: String,
        /// The expression to evaluate (receives the field value as input).
        expr: panproto_expr::Expr,
    },
    /// Apply a field transform at a nested path within the Value tree.
    ///
    /// The path is a sequence of string keys navigating through nested
    /// `Value::Unknown` (object) structures. The inner transform is applied
    /// to the `extra_fields` map at the resolved path.
    ///
    /// This generalizes flat field transforms to operate on the full
    /// Value algebra. A `PathTransform` with an empty path is equivalent
    /// to applying the inner transform directly.
    ///
    /// Categorically, this is the action of a path functor on the
    /// endomorphism algebra of field transforms — it lifts a transform
    /// from a leaf to an inner node of the Value tree.
    PathTransform {
        /// Path to navigate (e.g., `vec!["attrs"]` for nested attrs objects).
        path: Vec<String>,
        /// The transform to apply at the resolved path.
        inner: Box<Self>,
    },
    /// Compute a field value from an expression with access to ALL `extra_fields`.
    ///
    /// Unlike `ApplyExpr` which binds a single field, `ComputeField` binds
    /// all `extra_fields` (and nested attrs) as variables, evaluates the
    /// expression, and stores the result in the target field.
    ///
    /// This enables template name computation like
    ///   `target_key`: "name",
    ///   `expr`: `(concat "h" (int_to_str attrs.level))`
    /// which computes "h1", "h2", etc. from the level attribute.
    ComputeField {
        /// The field to store the computed result in.
        target_key: String,
        /// The expression, with all `extra_fields` bound as variables.
        expr: panproto_expr::Expr,
    },
    /// Case analysis on node values — the coproduct eliminator for the
    /// field transform algebra.
    ///
    /// Each branch is a (predicate, transforms) pair. Branches are evaluated
    /// in order with the node's `extra_fields` (and nested `attrs.*` keys)
    /// bound as expression variables. The first branch whose predicate
    /// evaluates to `true` has its transforms applied. If no branch matches,
    /// the node passes through unchanged.
    ///
    /// This is the dependent function space lift of field transforms:
    /// `Π(x : Value). FieldTransform` — a transform that depends on the
    /// runtime value of the node, not just its schema vertex. It composes
    /// naturally with all other transform variants (including nesting
    /// inside `PathTransform`).
    ///
    /// Use cases:
    /// - `matchAttrs`: "if `level == 1` then rename to `h1`, if `level == 2`
    ///   then rename to `h2`" — each heading level is a branch.
    /// - Conditional attribute injection: "if `list == 'ordered'` then add
    ///   `type: ol`, else add `type: ul`".
    Case {
        /// Ordered branches: first matching predicate wins.
        branches: Vec<CaseBranch>,
    },
    /// Update string values that reference vertex names.
    ///
    /// When vertices are renamed or dropped during migration, string fields
    /// that reference those vertices by name must be updated to reflect the
    /// new names. This is the functorial action of the vertex rename map
    /// on the name-reference algebra.
    ///
    /// For each field value:
    /// - If the value is a `Value::Str` matching a key in `rename_map`,
    ///   it is replaced with the mapped value (or removed if mapped to None).
    /// - If the value is a `Value::Unknown` containing an `__array_len`
    ///   sentinel (encoded array), each string element is checked.
    ///
    /// This handles parent reference arrays, cross-annotation links,
    /// and any other string fields that carry vertex identity.
    MapReferences {
        /// The field containing references (e.g., "parents").
        field: String,
        /// Map from old name to new name (None = remove the reference).
        rename_map: HashMap<String, Option<String>>,
    },
}

/// A branch in a [`FieldTransform::Case`] analysis.
///
/// Contains a predicate expression and a sequence of transforms to apply
/// if the predicate evaluates to `true`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseBranch {
    /// Predicate evaluated with the node's `extra_fields` as variables.
    pub predicate: panproto_expr::Expr,
    /// Transforms to apply if the predicate is true.
    pub transforms: Vec<FieldTransform>,
}

impl CompiledMigration {
    /// Add a field rename transform for a vertex.
    ///
    /// After the node survives and its anchor is remapped, the field
    /// `old_key` in `extra_fields` is renamed to `new_key`.
    pub fn add_field_rename(&mut self, vertex: &str, old_key: &str, new_key: &str) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::RenameField {
                old_key: old_key.to_owned(),
                new_key: new_key.to_owned(),
            });
    }

    /// Add a field drop transform for a vertex.
    ///
    /// The field `key` is removed from the node's `extra_fields`.
    pub fn add_field_drop(&mut self, vertex: &str, key: &str) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::DropField {
                key: key.to_owned(),
            });
    }

    /// Add a field with a default value for a vertex.
    ///
    /// The field `key` is added to `extra_fields` with the given value
    /// if it does not already exist.
    pub fn add_field_default(&mut self, vertex: &str, key: &str, value: Value) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::AddField {
                key: key.to_owned(),
                value,
            });
    }

    /// Add a keep-fields transform for a vertex.
    ///
    /// Only the specified fields are retained in `extra_fields`;
    /// all others are dropped.
    pub fn add_field_keep(&mut self, vertex: &str, keys: &[&str]) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::KeepFields {
                keys: keys.iter().map(|k| (*k).to_owned()).collect(),
            });
    }

    /// Add an expression transform for a field on a vertex.
    ///
    /// The expression is evaluated with the field's current value
    /// bound to the variable named `key`, and the result replaces
    /// the field value.
    pub fn add_field_expr(&mut self, vertex: &str, key: &str, expr: panproto_expr::Expr) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::ApplyExpr {
                key: key.to_owned(),
                expr,
            });
    }

    /// Add a path-based field transform for a vertex.
    ///
    /// The inner transform is applied at the nested path within the
    /// node's `extra_fields` tree, navigating through `Value::Unknown`
    /// maps at each path segment.
    pub fn add_path_transform(&mut self, vertex: &str, path: &[&str], inner: FieldTransform) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::PathTransform {
                path: path.iter().map(|s| (*s).to_owned()).collect(),
                inner: Box::new(inner),
            });
    }

    /// Add a computed field transform for a vertex.
    ///
    /// The expression is evaluated with all `extra_fields` (and nested
    /// attrs) bound as variables, and the result is stored in `target_key`.
    pub fn add_computed_field(
        &mut self,
        vertex: &str,
        target_key: &str,
        expr: panproto_expr::Expr,
    ) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::ComputeField {
                target_key: target_key.to_owned(),
                expr,
            });
    }

    /// Add a conditional survival predicate for a vertex.
    ///
    /// The expression is evaluated with the node's `extra_fields` bound
    /// as variables. If it returns false, the node is dropped.
    pub fn add_conditional_survival(&mut self, vertex: &str, predicate: panproto_expr::Expr) {
        self.conditional_survival
            .entry(Name::from(vertex))
            .or_insert(predicate);
    }

    /// Add a reference map transform for a vertex's field.
    ///
    /// String values (or encoded array elements) in the given field
    /// are renamed or removed according to the `rename_map`.
    pub fn add_map_references(
        &mut self,
        vertex: &str,
        field: &str,
        rename_map: HashMap<String, Option<String>>,
    ) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::MapReferences {
                field: field.to_owned(),
                rename_map,
            });
    }

    /// Add a case-analysis transform for a vertex.
    ///
    /// The branches are evaluated in order; the first matching predicate's
    /// transforms are applied. This is the dependent function space lift
    /// of field transforms.
    pub fn add_case_transform(&mut self, vertex: &str, branches: Vec<CaseBranch>) {
        self.field_transforms
            .entry(Name::from(vertex))
            .or_default()
            .push(FieldTransform::Case { branches });
    }
}

/// A W-type instance: tree-shaped data conforming to a schema.
///
/// Nodes are anchored to schema vertices, connected by arcs that
/// correspond to schema edges. The tree is rooted at `root`.
/// Precomputed `parent_map` and `children_map` enable fast traversal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WInstance {
    /// All nodes keyed by their numeric ID.
    pub nodes: HashMap<u32, Node>,
    /// Arcs: (`parent_id`, `child_id`, `schema_edge`).
    pub arcs: Vec<(u32, u32, Edge)>,
    /// Hyper-edge fans.
    pub fans: Vec<Fan>,
    /// Root node ID.
    pub root: u32,
    /// Schema vertex that the root node is anchored to.
    pub schema_root: Name,
    /// Precomputed parent map: `child_id` -> `parent_id`.
    pub parent_map: HashMap<u32, u32>,
    /// Precomputed children map: `parent_id` -> child IDs.
    pub children_map: HashMap<u32, SmallVec<u32, 4>>,
}

impl WInstance {
    /// Build a new W-type instance, computing parent and children maps from arcs.
    #[must_use]
    pub fn new(
        nodes: HashMap<u32, Node>,
        arcs: Vec<(u32, u32, Edge)>,
        fans: Vec<Fan>,
        root: u32,
        schema_root: Name,
    ) -> Self {
        let mut parent_map = HashMap::with_capacity(arcs.len());
        let mut children_map: HashMap<u32, SmallVec<u32, 4>> = HashMap::new();
        for &(parent, child, _) in &arcs {
            parent_map.insert(child, parent);
            children_map.entry(parent).or_default().push(child);
        }
        Self {
            nodes,
            arcs,
            fans,
            root,
            schema_root,
            parent_map,
            children_map,
        }
    }

    /// Returns the number of nodes.
    #[inline]
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of arcs.
    #[inline]
    #[must_use]
    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }

    /// Get a node by ID.
    #[inline]
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Get the children of a node.
    #[inline]
    #[must_use]
    pub fn children(&self, id: u32) -> &[u32] {
        self.children_map.get(&id).map_or(&[], SmallVec::as_slice)
    }

    /// Get the parent of a node.
    #[inline]
    #[must_use]
    pub fn parent(&self, id: u32) -> Option<u32> {
        self.parent_map.get(&id).copied()
    }
}

// ---------------------------------------------------------------------------
// Step 1: Signature restriction (retained for testing)
// ---------------------------------------------------------------------------

/// Keep nodes whose anchor vertex is in the surviving vertex set.
#[must_use]
pub fn anchor_surviving(instance: &WInstance, surviving_verts: &HashSet<Name>) -> HashSet<u32> {
    instance
        .nodes
        .iter()
        .filter(|(_, node)| surviving_verts.contains(&node.anchor))
        .map(|(&id, _)| id)
        .collect()
}

// ---------------------------------------------------------------------------
// Step 2: Reachability BFS (retained for testing)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Step 3: Ancestor contraction with path compression (retained for testing)
// ---------------------------------------------------------------------------

/// For each surviving non-root node, find its nearest surviving ancestor.
///
/// Uses path compression: when we walk the parent chain for a node,
/// we cache the result for every intermediate node visited. Subsequent
/// queries hitting a cached node return in O(1). This gives O(n)
/// amortized complexity instead of O(n × depth).
#[must_use]
pub fn ancestor_contraction(instance: &WInstance, surviving: &HashSet<u32>) -> HashMap<u32, u32> {
    let mut cache: FxHashMap<u32, u32> = FxHashMap::default();
    let mut ancestors = HashMap::new();

    for &node_id in surviving {
        if node_id == instance.root {
            continue;
        }

        // Check cache first
        if let Some(&cached) = cache.get(&node_id) {
            ancestors.insert(node_id, cached);
            continue;
        }

        // Walk the parent chain, recording the path for compression
        let mut path = Vec::new();
        let mut current = node_id;
        let mut found_ancestor = None;

        while let Some(parent) = instance.parent(current) {
            if let Some(&cached) = cache.get(&parent) {
                found_ancestor = Some(cached);
                break;
            }
            if surviving.contains(&parent) {
                found_ancestor = Some(parent);
                break;
            }
            path.push(parent);
            current = parent;
        }

        // Path compression: cache the ancestor for all nodes on the path
        if let Some(ancestor) = found_ancestor {
            ancestors.insert(node_id, ancestor);
            cache.insert(node_id, ancestor);
            for &intermediate in &path {
                cache.insert(intermediate, ancestor);
            }
        }
    }
    ancestors
}

// ---------------------------------------------------------------------------
// Step 4: Edge resolution (retained for testing)
// ---------------------------------------------------------------------------

/// Resolve the edge for a contracted arc in the target schema.
///
/// Avoids allocating a `(String, String)` tuple for the resolver lookup
/// by checking the resolver with borrowed references.
///
/// # Errors
///
/// Returns `RestrictError::NoEdgeFound` if no edge exists, or
/// `RestrictError::AmbiguousEdge` if multiple edges exist without
/// a resolver entry.
pub fn resolve_edge(
    tgt_schema: &Schema,
    resolver: &HashMap<(Name, Name), Edge>,
    src_v: &str,
    tgt_v: &str,
) -> Result<Edge, RestrictError> {
    // Check resolver — avoid allocation by scanning for matching key
    for ((k_src, k_tgt), edge) in resolver {
        if k_src == src_v && k_tgt == tgt_v {
            return Ok(edge.clone());
        }
    }

    // Fall back to unique-edge lookup
    let candidates = tgt_schema.edges_between(src_v, tgt_v);
    match candidates.len() {
        0 => Err(RestrictError::NoEdgeFound {
            src: src_v.to_string(),
            tgt: tgt_v.to_string(),
        }),
        1 => Ok(candidates[0].clone()),
        n => Err(RestrictError::AmbiguousEdge {
            src: src_v.to_string(),
            tgt: tgt_v.to_string(),
            count: n,
        }),
    }
}

// ---------------------------------------------------------------------------
// Step 5: Fan reconstruction (retained for testing)
// ---------------------------------------------------------------------------

/// Reconstruct fans after restriction.
///
/// # Errors
///
/// Returns `RestrictError::FanReconstructionFailed` if a fan cannot
/// be validly reconstructed.
pub fn reconstruct_fans(
    instance: &WInstance,
    surviving: &FxHashSet<u32>,
    _ancestors: &FxHashMap<u32, u32>,
    migration: &CompiledMigration,
    _tgt_schema: &Schema,
) -> Result<Vec<Fan>, RestrictError> {
    let mut result = Vec::new();

    for fan in &instance.fans {
        if !surviving.contains(&fan.parent) {
            continue;
        }

        let surviving_children: HashMap<String, u32> = fan
            .children
            .iter()
            .filter(|(_, node_id)| surviving.contains(node_id))
            .map(|(label, node_id)| (label.clone(), *node_id))
            .collect();

        if surviving_children.is_empty() {
            continue;
        }

        if let Some((new_he_id, label_map)) =
            migration.hyper_resolver.get(fan.hyper_edge_id.as_str())
        {
            let mut new_children = HashMap::new();
            for (old_label, &node_id) in &surviving_children {
                let new_label = label_map
                    .get(old_label.as_str())
                    .map_or_else(|| old_label.clone(), std::string::ToString::to_string);
                new_children.insert(new_label, node_id);
            }
            result.push(Fan {
                hyper_edge_id: new_he_id.to_string(),
                parent: fan.parent,
                children: new_children,
            });
        } else {
            result.push(Fan {
                hyper_edge_id: fan.hyper_edge_id.clone(),
                parent: fan.parent,
                children: surviving_children,
            });
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Main restrict function: fused single-pass pipeline
// ---------------------------------------------------------------------------

/// The restrict operation for W-type instances.
///
/// Executes a fused single-pass pipeline that combines anchor checking,
/// BFS reachability, ancestor contraction, and edge resolution into one
/// traversal. Fan reconstruction runs as a separate pass.
///
/// The fused approach visits each node at most once (O(n)) versus
/// the sequential 5-step approach which makes 3-4 passes.
///
/// # Errors
///
/// Returns `RestrictError` if edge resolution fails or the root
/// is pruned during restriction.
pub fn wtype_restrict(
    instance: &WInstance,
    _src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<WInstance, RestrictError> {
    // Check root survives
    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;
    let root_target_anchor = migration
        .vertex_remap
        .get(&root_node.anchor)
        .unwrap_or(&root_node.anchor);
    if !migration.surviving_verts.contains(root_target_anchor) {
        return Err(RestrictError::RootPruned);
    }

    // Fused BFS: traverse the tree from root, tracking the nearest
    // surviving ancestor for each node as we go.
    //
    // For each node in the BFS:
    //   - If its anchor survives: it becomes part of the result.
    //     Its nearest surviving ancestor is used to build an arc.
    //     It becomes the "current surviving ancestor" for its subtree.
    //   - If its anchor does not survive: skip it, but continue BFS
    //     into its children (they might survive). Pass along the
    //     current surviving ancestor unchanged.

    let mut new_nodes: HashMap<u32, Node> = HashMap::new();
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::new();
    let mut surviving_set: FxHashSet<u32> = FxHashSet::default();

    // Queue entries: (node_id, nearest_surviving_ancestor_id)
    let mut queue: VecDeque<(u32, Option<u32>)> = VecDeque::new();

    // Process root: remap, check conditional survival, apply field transforms.
    let root_node_cloned = prepare_root_node(root_node, migration)?;
    new_nodes.insert(instance.root, root_node_cloned);
    surviving_set.insert(instance.root);
    queue.push_back((instance.root, None));

    while let Some((current_id, ancestor_id)) = queue.pop_front() {
        let current_survives = surviving_set.contains(&current_id);
        // The ancestor for children: if current survives, it's the new ancestor;
        // otherwise, pass along the existing ancestor.
        let child_ancestor = if current_survives {
            Some(current_id)
        } else {
            ancestor_id
        };

        for &child_id in instance.children(current_id) {
            let Some(child_node) = instance.nodes.get(&child_id) else {
                continue;
            };

            // Check if this vertex survives: look up the remapped target name,
            // falling back to the source name for unmapped vertices.
            let target_anchor = migration
                .vertex_remap
                .get(&child_node.anchor)
                .unwrap_or(&child_node.anchor);
            if migration.surviving_verts.contains(target_anchor) {
                // Check conditional survival predicate if one exists
                if let Some(pred) = migration.conditional_survival.get(&child_node.anchor) {
                    let env = build_env_from_extra_fields(&child_node.extra_fields);
                    let config = panproto_expr::EvalConfig::default();
                    if matches!(
                        panproto_expr::eval(pred, &env, &config),
                        Ok(panproto_expr::Literal::Bool(false))
                    ) {
                        // Predicate is false — skip this node (treat as non-surviving)
                        queue.push_back((child_id, child_ancestor));
                        continue;
                    }
                }

                // This child survives — add it to results
                surviving_set.insert(child_id);
                let mut new_node = child_node.clone();
                if let Some(remapped) = migration.vertex_remap.get(&child_node.anchor) {
                    new_node.anchor.clone_from(remapped);
                }
                // Apply value-level field transforms if any exist for this vertex
                if let Some(transforms) = migration.field_transforms.get(&child_node.anchor) {
                    apply_field_transforms(&mut new_node, transforms);
                }
                new_nodes.insert(child_id, new_node.clone());

                // Build the arc from nearest surviving ancestor to this node
                if let Some(anc_id) = child_ancestor {
                    let anc_node = new_nodes.get(&anc_id).ok_or(RestrictError::RootPruned)?;
                    let edge = resolve_edge(
                        tgt_schema,
                        &migration.resolver,
                        &anc_node.anchor,
                        &new_node.anchor,
                    )?;
                    new_arcs.push((anc_id, child_id, edge));
                }
            }

            // Always continue BFS into children (non-surviving intermediate
            // nodes may have surviving descendants)
            queue.push_back((child_id, child_ancestor));
        }
    }

    // Step 5: Fan reconstruction (separate pass — operates on original fans)
    let fused_surviving = &surviving_set;
    let empty_ancestors = FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        fused_surviving,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

    let new_schema_root = migration
        .vertex_remap
        .get(&instance.schema_root)
        .cloned()
        .unwrap_or_else(|| instance.schema_root.clone());

    Ok(WInstance::new(
        new_nodes,
        new_arcs,
        new_fans,
        instance.root,
        new_schema_root,
    ))
}

// ---------------------------------------------------------------------------
// Value-level field transforms
// ---------------------------------------------------------------------------

/// Apply a sequence of field transforms to a node's `extra_fields`.
///
/// Called during `wtype_restrict` after a node survives and its anchor
/// is remapped. Operations are applied in order.
pub fn apply_field_transforms(node: &mut Node, transforms: &[FieldTransform]) {
    for transform in transforms {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => {
                if let Some(val) = node.extra_fields.remove(old_key) {
                    node.extra_fields.insert(new_key.clone(), val);
                }
            }
            FieldTransform::DropField { key } => {
                node.extra_fields.remove(key);
            }
            FieldTransform::AddField { key, value } => {
                node.extra_fields
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
            FieldTransform::KeepFields { keys } => {
                node.extra_fields.retain(|k, _| keys.contains(k));
            }
            FieldTransform::ApplyExpr { key, expr } => {
                if let Some(val) = node.extra_fields.get(key) {
                    let input = value_to_expr_literal(val);
                    let env =
                        panproto_expr::Env::new().extend(std::sync::Arc::from(key.as_str()), input);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(expr, &env, &config) {
                        node.extra_fields
                            .insert(key.clone(), expr_literal_to_value(&result));
                    }
                }
            }
            FieldTransform::ComputeField { target_key, expr } => {
                let env = build_env_from_extra_fields(&node.extra_fields);
                let config = panproto_expr::EvalConfig::default();
                if let Ok(result) = panproto_expr::eval(expr, &env, &config) {
                    node.extra_fields
                        .insert(target_key.clone(), expr_literal_to_value(&result));
                }
            }
            FieldTransform::PathTransform { path, inner } => {
                if path.is_empty() {
                    // Empty path = apply directly
                    apply_field_transforms(node, std::slice::from_ref(inner));
                } else {
                    apply_path_transform(node, path, inner);
                }
            }
            FieldTransform::MapReferences { field, rename_map } => {
                apply_map_references(node, field, rename_map);
            }
            FieldTransform::Case { branches } => {
                let env = build_env_from_extra_fields(&node.extra_fields);
                let config = panproto_expr::EvalConfig::default();
                for branch in branches {
                    let result = panproto_expr::eval(&branch.predicate, &env, &config);
                    if matches!(result, Ok(panproto_expr::Literal::Bool(true))) {
                        apply_field_transforms(node, &branch.transforms);
                        break;
                    }
                }
            }
        }
    }
}

/// Navigate into nested `Value::Unknown` maps along `path` and apply the
/// inner transform at the resolved location.
fn apply_path_transform(node: &mut Node, path: &[String], inner: &FieldTransform) {
    let first = &path[0];
    if let Some(Value::Unknown(map)) = node.extra_fields.get_mut(first) {
        if path.len() == 1 {
            // At the target — apply inner transform to this map
            let mut temp_node = Node::new(0, "");
            temp_node.extra_fields = std::mem::take(map);
            apply_field_transforms(&mut temp_node, std::slice::from_ref(inner));
            *map = temp_node.extra_fields;
        } else {
            // Recurse deeper — wrap the remaining path in a temporary node
            let rest = &path[1..];
            let mut temp_node = Node::new(0, "");
            temp_node.extra_fields = std::mem::take(map);
            apply_path_transform(&mut temp_node, rest, inner);
            *map = temp_node.extra_fields;
        }
    }
}

/// Apply a `MapReferences` transform to a node's field, handling both
/// flat `Value::Str` and encoded arrays with `__array_len` sentinels.
fn apply_map_references(
    node: &mut Node,
    field: &str,
    rename_map: &HashMap<String, Option<String>>,
) {
    if let Some(val) = node.extra_fields.get_mut(field) {
        match val {
            Value::Str(s) => {
                if let Some(replacement) = rename_map.get(s.as_str()) {
                    match replacement {
                        Some(new_name) => *s = new_name.clone(),
                        None => {
                            node.extra_fields.remove(field);
                        }
                    }
                }
            }
            Value::Unknown(map) => {
                // Encoded array: check for __array_len sentinel
                if map.contains_key("__array_len") {
                    let len = match map.get("__array_len") {
                        Some(Value::Int(n)) => usize::try_from(*n).unwrap_or(0),
                        _ => 0,
                    };
                    let mut new_entries = Vec::new();
                    for i in 0..len {
                        let key = i.to_string();
                        if let Some(Value::Str(s)) = map.get(&key) {
                            match rename_map.get(s.as_str()) {
                                Some(Some(new_name)) => {
                                    new_entries.push(Value::Str(new_name.clone()));
                                }
                                Some(None) => {} // drop
                                None => new_entries.push(Value::Str(s.clone())),
                            }
                        }
                    }
                    // Rebuild the encoded array
                    let mut new_map = HashMap::new();
                    for (i, v) in new_entries.iter().enumerate() {
                        new_map.insert(i.to_string(), v.clone());
                    }
                    let new_len = i64::try_from(new_entries.len()).unwrap_or(0);
                    new_map.insert("__array_len".to_string(), Value::Int(new_len));
                    *map = new_map;
                }
            }
            _ => {}
        }
    }
}

/// Build an evaluation environment from a node's `extra_fields`.
///
/// Each field is bound as a top-level variable. If an `attrs` field
/// contains a `Value::Unknown` map, its entries are also bound with
/// qualified names (e.g., `attrs.level`).
#[must_use]
pub fn build_env_from_extra_fields(fields: &HashMap<String, Value>) -> panproto_expr::Env {
    let mut env = panproto_expr::Env::new();
    for (key, val) in fields {
        let lit = value_to_expr_literal(val);
        // Bind flat key
        env = env.extend(std::sync::Arc::from(key.as_str()), lit.clone());
        // Also bind as attrs.key (so predicates work regardless of nesting style)
        if key != "attrs" && key != "name" && key != "$type" && key != "parents" {
            let qualified = format!("attrs.{key}");
            env = env.extend(std::sync::Arc::from(qualified.as_str()), lit);
        }
    }
    // Also bind nested "attrs" entries as both qualified and flat
    if let Some(Value::Unknown(attrs)) = fields.get("attrs") {
        for (key, val) in attrs {
            let lit = value_to_expr_literal(val);
            let qualified = format!("attrs.{key}");
            env = env.extend(std::sync::Arc::from(qualified.as_str()), lit.clone());
            // Also bind as flat key if not already present
            if !fields.contains_key(key) {
                env = env.extend(std::sync::Arc::from(key.as_str()), lit);
            }
        }
    }
    env
}

/// Convert an instance `Value` to a `panproto_expr::Literal` for expression evaluation.
#[must_use]
pub fn value_to_expr_literal(val: &Value) -> panproto_expr::Literal {
    match val {
        Value::Bool(b) => panproto_expr::Literal::Bool(*b),
        Value::Int(i) => panproto_expr::Literal::Int(*i),
        Value::Float(f) => panproto_expr::Literal::Float(*f),
        Value::Str(s) => panproto_expr::Literal::Str(s.clone()),
        Value::Unknown(map) => {
            // Encoded arrays (with __array_len): serialize as comma-separated
            // string so Contains can check membership.
            if let Some(Value::Int(len)) = map.get("__array_len") {
                let mut parts = Vec::new();
                let count = usize::try_from(*len).unwrap_or(0);
                for i in 0..count {
                    if let Some(Value::Str(s)) = map.get(&i.to_string()) {
                        parts.push(s.as_str());
                    }
                }
                panproto_expr::Literal::Str(parts.join(","))
            } else {
                panproto_expr::Literal::Null
            }
        }
        _ => panproto_expr::Literal::Null,
    }
}

/// Convert a `panproto_expr::Literal` back to an instance `Value`.
///
/// Integer-valued floats are normalized to `Value::Int` for round-trip
/// fidelity with JSON (which doesn't distinguish int/float).
fn expr_literal_to_value(lit: &panproto_expr::Literal) -> Value {
    match lit {
        panproto_expr::Literal::Bool(b) => Value::Bool(*b),
        panproto_expr::Literal::Int(i) => Value::Int(*i),
        panproto_expr::Literal::Float(f) => {
            // Normalize integer-valued floats to Int for JSON round-trip fidelity.
            // Use safe bounds that avoid precision loss in f64→i64 conversion.
            #[allow(clippy::cast_precision_loss)]
            let fits = f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64;
            if fits {
                #[allow(clippy::cast_possible_truncation)]
                let i = *f as i64;
                Value::Int(i)
            } else {
                Value::Float(*f)
            }
        }
        panproto_expr::Literal::Str(s) => Value::Str(s.clone()),
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------------------
// Left Kan extension (Σ_F) for W-type instances
// ---------------------------------------------------------------------------

/// Left Kan extension (`Sigma_F`) for W-type instances.
///
/// Pushes a W-type instance forward along a migration morphism.
/// Unlike [`wtype_restrict`] (which drops unmapped nodes), extend
/// maps all source nodes into the target schema, remapping anchors
/// Prepare the root node for restriction: remap anchor, check conditional
/// survival, and apply field transforms.
fn prepare_root_node(
    root_node: &Node,
    migration: &CompiledMigration,
) -> Result<Node, RestrictError> {
    let mut node = root_node.clone();
    if let Some(remapped) = migration.vertex_remap.get(&root_node.anchor) {
        node.anchor.clone_from(remapped);
    }
    if let Some(pred) = migration.conditional_survival.get(&root_node.anchor) {
        let env = build_env_from_extra_fields(&root_node.extra_fields);
        let config = panproto_expr::EvalConfig::default();
        if matches!(
            panproto_expr::eval(pred, &env, &config),
            Ok(panproto_expr::Literal::Bool(false))
        ) {
            return Err(RestrictError::RootPruned);
        }
    }
    if let Some(transforms) = migration.field_transforms.get(&root_node.anchor) {
        apply_field_transforms(&mut node, transforms);
    }
    Ok(node)
}

/// and edges according to the compiled migration.
///
/// # Errors
///
/// Returns [`RestrictError`] if edge resolution fails or the root
/// cannot be mapped.
pub fn wtype_extend(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<WInstance, RestrictError> {
    // Check root can be mapped
    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;

    let root_anchor = &root_node.anchor;
    if !migration.surviving_verts.contains(root_anchor)
        && !migration.vertex_remap.contains_key(root_anchor)
    {
        return Err(RestrictError::RootPruned);
    }

    // Build new nodes: remap anchors where applicable
    let mut new_nodes: HashMap<u32, Node> = HashMap::with_capacity(instance.nodes.len());
    for (&id, node) in &instance.nodes {
        let mut new_node = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            new_node.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            // Node's anchor has no remap and doesn't survive — skip it
            continue;
        }
        new_nodes.insert(id, new_node);
    }

    // Build new arcs: remap edges where applicable
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::with_capacity(instance.arcs.len());
    for &(parent, child, ref edge) in &instance.arcs {
        // Both endpoints must be in the new node set
        if !new_nodes.contains_key(&parent) || !new_nodes.contains_key(&child) {
            continue;
        }

        if let Some(new_edge) = migration.edge_remap.get(edge) {
            new_arcs.push((parent, child, new_edge.clone()));
        } else if migration.surviving_edges.contains(edge) {
            // Edge survives unchanged, but anchors may have been remapped.
            // Rebuild the edge with the remapped src/tgt vertex names.
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            if edge.src == *parent_anchor && edge.tgt == *child_anchor {
                new_arcs.push((parent, child, edge.clone()));
            } else {
                // Anchors were remapped; resolve the edge in the target schema
                let resolved =
                    resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
                new_arcs.push((parent, child, resolved));
            }
        } else {
            // Edge not in surviving_edges or edge_remap — try to resolve
            // from remapped anchors
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            let resolved =
                resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
            new_arcs.push((parent, child, resolved));
        }
    }

    // Handle fans similarly to restrict's reconstruct_fans
    let surviving_ids: FxHashSet<u32> = new_nodes.keys().copied().collect();
    let empty_ancestors = FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        &surviving_ids,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

    let new_schema_root = migration
        .vertex_remap
        .get(&instance.schema_root)
        .cloned()
        .unwrap_or_else(|| instance.schema_root.clone());

    Ok(WInstance::new(
        new_nodes,
        new_arcs,
        new_fans,
        instance.root,
        new_schema_root,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{FieldPresence, Value};

    /// Helper: build a simple 3-node instance (object with two string children).
    fn three_node_instance() -> WInstance {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, panproto_gat::Name::from("post:body")));
        nodes.insert(
            1,
            Node::new(1, "post:body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "post:body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            ),
            (
                0,
                2,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:body.createdAt".into(),
                    kind: "prop".into(),
                    name: Some("createdAt".into()),
                },
            ),
        ];

        WInstance::new(
            nodes,
            arcs,
            vec![],
            0,
            panproto_gat::Name::from("post:body"),
        )
    }

    #[test]
    fn anchor_surviving_keeps_matching_nodes() {
        let inst = three_node_instance();
        let surviving_verts: HashSet<Name> = ["post:body", "post:body.text"]
            .iter()
            .map(|&s| Name::from(s))
            .collect();

        let result = anchor_surviving(&inst, &surviving_verts);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(!result.contains(&2));
    }

    #[test]
    fn ancestor_contraction_direct_parent() {
        let inst = three_node_instance();
        let surviving: HashSet<u32> = [0, 1, 2].iter().copied().collect();
        let ancestors = ancestor_contraction(&inst, &surviving);
        assert_eq!(ancestors.get(&1), Some(&0));
        assert_eq!(ancestors.get(&2), Some(&0));
    }

    #[test]
    fn resolve_edge_unique() {
        use smallvec::smallvec;
        let mut between = HashMap::new();
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        between.insert((Name::from("a"), Name::from("b")), smallvec![edge.clone()]);

        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
            between,
        };

        let resolver = HashMap::new();
        let result = resolve_edge(&schema, &resolver, "a", "b");
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(edge));
    }

    #[test]
    fn resolve_edge_uses_resolver() {
        let schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
        };

        let resolved_edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("resolved".into()),
        };
        let mut resolver = HashMap::new();
        resolver.insert((Name::from("a"), Name::from("b")), resolved_edge.clone());

        let result = resolve_edge(&schema, &resolver, "a", "b");
        assert!(result.is_ok());
        assert_eq!(result.ok(), Some(resolved_edge));
    }

    // --- wtype_extend tests ---

    #[allow(clippy::unwrap_used)]
    fn make_test_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
        use smallvec::smallvec;
        let mut between = HashMap::new();
        for edge in edges {
            between
                .entry((Name::from(&*edge.src), Name::from(&*edge.tgt)))
                .or_insert_with(|| smallvec![])
                .push(edge.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        panproto_schema::Vertex {
                            id: Name::from(v),
                            kind: Name::from("object"),
                            nsid: None,
                        },
                    )
                })
                .collect(),
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
            between,
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_identity_migration() {
        let inst = three_node_instance();
        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let surviving_edges = HashSet::from([edge_text.clone(), edge_time.clone()]);
        let schema = make_test_schema(
            &["post:body", "post:body.text", "post:body.createdAt"],
            &[edge_text, edge_time],
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("post:body"),
                Name::from("post:body.text"),
                Name::from("post:body.createdAt"),
            ]),
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };
        let result = wtype_extend(&inst, &schema, &migration).unwrap();
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.arc_count(), 2);
        assert_eq!(result.schema_root, Name::from("post:body"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_with_vertex_remap() {
        let inst = three_node_instance();
        let tgt_edge_text = Edge {
            src: "article:body".into(),
            tgt: "article:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let tgt_edge_time = Edge {
            src: "article:body".into(),
            tgt: "article:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let tgt_schema = make_test_schema(
            &[
                "article:body",
                "article:body.text",
                "article:body.createdAt",
            ],
            &[tgt_edge_text, tgt_edge_time],
        );
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post:body"), Name::from("article:body"));
        vertex_remap.insert(
            Name::from("post:body.text"),
            Name::from("article:body.text"),
        );
        vertex_remap.insert(
            Name::from("post:body.createdAt"),
            Name::from("article:body.createdAt"),
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("article:body"),
                Name::from("article:body.text"),
                Name::from("article:body.createdAt"),
            ]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };
        let result = wtype_extend(&inst, &tgt_schema, &migration).unwrap();
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.arc_count(), 2);
        assert_eq!(result.schema_root, Name::from("article:body"));
        assert_eq!(result.nodes[&0].anchor, Name::from("article:body"));
        assert_eq!(result.nodes[&1].anchor, Name::from("article:body.text"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_with_edge_remap() {
        let inst = three_node_instance();
        let src_edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let new_edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("content".into()),
        };
        let edge_time = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let surviving_edges = HashSet::from([edge_time.clone()]);
        let tgt_schema = make_test_schema(
            &["post:body", "post:body.text", "post:body.createdAt"],
            &[new_edge_text.clone(), edge_time],
        );
        let mut edge_remap = HashMap::new();
        edge_remap.insert(src_edge_text, new_edge_text);
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("post:body"),
                Name::from("post:body.text"),
                Name::from("post:body.createdAt"),
            ]),
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };
        let result = wtype_extend(&inst, &tgt_schema, &migration).unwrap();
        assert_eq!(result.arc_count(), 2);
        // Check that the remapped edge is used
        let text_arc = result.arcs.iter().find(|a| a.1 == 1).unwrap();
        assert_eq!(text_arc.2.name.as_deref(), Some("content"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_preserves_structure() {
        let inst = three_node_instance();
        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "post:body".into(),
            tgt: "post:body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };
        let surviving_edges = HashSet::from([edge_text.clone(), edge_time.clone()]);
        let schema = make_test_schema(
            &["post:body", "post:body.text", "post:body.createdAt"],
            &[edge_text, edge_time],
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("post:body"),
                Name::from("post:body.text"),
                Name::from("post:body.createdAt"),
            ]),
            surviving_edges,
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };
        let result = wtype_extend(&inst, &schema, &migration).unwrap();
        // Verify parent/children maps are correctly rebuilt
        assert_eq!(result.parent(1), Some(0));
        assert_eq!(result.parent(2), Some(0));
        assert!(result.children(0).contains(&1));
        assert!(result.children(0).contains(&2));
        // Verify values are preserved
        assert!(result.nodes[&1].has_value());
        assert!(result.nodes[&2].has_value());
    }

    /// Regression test: renamed vertices must survive restrict.
    ///
    /// When a migration maps source vertex `A` to target vertex `B`, the
    /// `surviving_verts` set contains `B` (the target). The restrict BFS
    /// must remap `A` → `B` before checking membership, otherwise the
    /// node is incorrectly pruned and its value is lost.
    #[test]
    #[allow(clippy::expect_used, clippy::too_many_lines)]
    fn restrict_renamed_vertex_preserves_value() {
        use smallvec::smallvec;

        // Source instance: post:body { text: "hello", title: "world" }
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, Name::from("post:body")));
        nodes.insert(
            1,
            Node::new(1, "post:text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "post:title")
                .with_value(FieldPresence::Present(Value::Str("world".into()))),
        );
        let arcs = vec![
            (
                0,
                1,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            ),
            (
                0,
                2,
                Edge {
                    src: "post:body".into(),
                    tgt: "post:title".into(),
                    kind: "prop".into(),
                    name: Some("title".into()),
                },
            ),
        ];
        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("post:body"));

        // Target schema: post:body has edges to post:content and post:title
        let tgt_content_edge = Edge {
            src: "post:body".into(),
            tgt: "post:content".into(),
            kind: "prop".into(),
            name: Some("content".into()),
        };
        let tgt_title_edge = Edge {
            src: "post:body".into(),
            tgt: "post:title".into(),
            kind: "prop".into(),
            name: Some("title".into()),
        };
        let mut tgt_between = HashMap::new();
        tgt_between.insert(
            (Name::from("post:body"), Name::from("post:content")),
            smallvec![tgt_content_edge],
        );
        tgt_between.insert(
            (Name::from("post:body"), Name::from("post:title")),
            smallvec![tgt_title_edge],
        );
        let tgt_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
            between: tgt_between,
        };

        // Migration: post:text → post:content (renamed), post:title stays
        let mut surviving_verts = HashSet::new();
        surviving_verts.insert(Name::from("post:body"));
        surviving_verts.insert(Name::from("post:content")); // target name
        surviving_verts.insert(Name::from("post:title"));

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post:text"), Name::from("post:content"));

        let migration = CompiledMigration {
            surviving_verts,
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };

        let src_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
        };

        let result = wtype_restrict(&inst, &src_schema, &tgt_schema, &migration)
            .expect("restrict should succeed");

        // All three nodes must survive (root + renamed + unchanged)
        assert_eq!(result.nodes.len(), 3, "all three nodes should survive");

        // The renamed node should now have anchor "post:content"
        let renamed_node = result.nodes.get(&1).expect("node 1 should survive");
        assert_eq!(renamed_node.anchor.as_ref(), "post:content");
        assert!(renamed_node.has_value(), "renamed node must keep its value");

        // The value should be preserved
        assert!(
            matches!(
                &renamed_node.value,
                Some(FieldPresence::Present(Value::Str(s))) if s.as_str() == "hello"
            ),
            "expected Some(Present(Str(\"hello\"))), got {:?}",
            renamed_node.value,
        );
    }

    // --- PathTransform tests ---

    #[test]
    fn path_transform_renames_nested_field() {
        let mut node = Node::new(0, "v");
        let mut inner_map = HashMap::new();
        inner_map.insert("old_attr".to_string(), Value::Str("val".into()));
        node.extra_fields
            .insert("attrs".to_string(), Value::Unknown(inner_map));

        let transform = FieldTransform::PathTransform {
            path: vec!["attrs".to_string()],
            inner: Box::new(FieldTransform::RenameField {
                old_key: "old_attr".to_string(),
                new_key: "new_attr".to_string(),
            }),
        };
        apply_field_transforms(&mut node, &[transform]);

        match node.extra_fields.get("attrs") {
            Some(Value::Unknown(map)) => {
                assert!(!map.contains_key("old_attr"));
                assert_eq!(map.get("new_attr"), Some(&Value::Str("val".into())));
            }
            other => panic!("expected Unknown map, got {other:?}"),
        }
    }

    #[test]
    fn path_transform_empty_path_is_identity() {
        let mut node = Node::new(0, "v");
        node.extra_fields
            .insert("color".to_string(), Value::Str("red".into()));

        let transform = FieldTransform::PathTransform {
            path: vec![],
            inner: Box::new(FieldTransform::RenameField {
                old_key: "color".to_string(),
                new_key: "colour".to_string(),
            }),
        };
        apply_field_transforms(&mut node, &[transform]);

        assert!(!node.extra_fields.contains_key("color"));
        assert_eq!(
            node.extra_fields.get("colour"),
            Some(&Value::Str("red".into()))
        );
    }

    // --- MapReferences tests ---

    #[test]
    fn map_references_renames_string_field() {
        let mut node = Node::new(0, "v");
        node.extra_fields
            .insert("parent".to_string(), Value::Str("old_name".into()));

        let mut rename_map = HashMap::new();
        rename_map.insert("old_name".to_string(), Some("new_name".to_string()));

        let transform = FieldTransform::MapReferences {
            field: "parent".to_string(),
            rename_map,
        };
        apply_field_transforms(&mut node, &[transform]);

        assert_eq!(
            node.extra_fields.get("parent"),
            Some(&Value::Str("new_name".into()))
        );
    }

    #[test]
    fn map_references_filters_encoded_array() {
        let mut node = Node::new(0, "v");
        let mut arr = HashMap::new();
        arr.insert("__array_len".to_string(), Value::Int(3));
        arr.insert("0".to_string(), Value::Str("alpha".into()));
        arr.insert("1".to_string(), Value::Str("beta".into()));
        arr.insert("2".to_string(), Value::Str("gamma".into()));
        node.extra_fields
            .insert("parents".to_string(), Value::Unknown(arr));

        let mut rename_map = HashMap::new();
        rename_map.insert("alpha".to_string(), Some("alpha_v2".to_string()));
        rename_map.insert("beta".to_string(), None); // drop

        let transform = FieldTransform::MapReferences {
            field: "parents".to_string(),
            rename_map,
        };
        apply_field_transforms(&mut node, &[transform]);

        match node.extra_fields.get("parents") {
            Some(Value::Unknown(map)) => {
                assert_eq!(map.get("__array_len"), Some(&Value::Int(2)));
                assert_eq!(map.get("0"), Some(&Value::Str("alpha_v2".into())));
                assert_eq!(map.get("1"), Some(&Value::Str("gamma".into())));
            }
            other => panic!("expected Unknown map, got {other:?}"),
        }
    }

    #[test]
    fn map_references_drops_removed_entries() {
        let mut node = Node::new(0, "v");
        let mut arr = HashMap::new();
        arr.insert("__array_len".to_string(), Value::Int(2));
        arr.insert("0".to_string(), Value::Str("gone".into()));
        arr.insert("1".to_string(), Value::Str("also_gone".into()));
        node.extra_fields
            .insert("refs".to_string(), Value::Unknown(arr));

        let mut rename_map = HashMap::new();
        rename_map.insert("gone".to_string(), None);
        rename_map.insert("also_gone".to_string(), None);

        let transform = FieldTransform::MapReferences {
            field: "refs".to_string(),
            rename_map,
        };
        apply_field_transforms(&mut node, &[transform]);

        match node.extra_fields.get("refs") {
            Some(Value::Unknown(map)) => {
                assert_eq!(map.get("__array_len"), Some(&Value::Int(0)));
                assert!(!map.contains_key("0"));
                assert!(!map.contains_key("1"));
            }
            other => panic!("expected Unknown map, got {other:?}"),
        }
    }

    // --- ConditionalSurvival tests ---

    #[test]
    #[allow(clippy::expect_used)]
    fn conditional_survival_drops_non_matching_node() {
        use smallvec::smallvec;

        // Two child nodes anchored to "item", with different level values.
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, Name::from("root")));
        nodes.insert(
            1,
            Node::new(1, "item").with_extra_field("level", Value::Int(2)),
        );
        nodes.insert(
            2,
            Node::new(2, "item").with_extra_field("level", Value::Int(1)),
        );

        let edge = Edge {
            src: "root".into(),
            tgt: "item".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        let arcs = vec![(0, 1, edge.clone()), (0, 2, edge.clone())];
        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let mut between = HashMap::new();
        between.insert(
            (Name::from("root"), Name::from("item")),
            smallvec![edge.clone()],
        );
        let tgt_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
            between,
        };
        let src_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
        };

        // Predicate: (== level 2)
        let predicate = panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Eq,
            vec![
                panproto_expr::Expr::Var(std::sync::Arc::from("level")),
                panproto_expr::Expr::Lit(panproto_expr::Literal::Int(2)),
            ],
        );

        let mut migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("item")]),
            surviving_edges: HashSet::from([edge]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };
        migration.add_conditional_survival("item", predicate);

        let result =
            wtype_restrict(&inst, &src_schema, &tgt_schema, &migration).expect("restrict ok");

        // Node 1 (level=2) survives, node 2 (level=1) is dropped
        assert_eq!(result.node_count(), 2);
        assert!(result.nodes.contains_key(&0));
        assert!(result.nodes.contains_key(&1));
        assert!(!result.nodes.contains_key(&2));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn conditional_survival_no_predicate_survives() {
        use smallvec::smallvec;

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, Name::from("root")));
        nodes.insert(
            1,
            Node::new(1, "item").with_extra_field("level", Value::Int(1)),
        );

        let edge = Edge {
            src: "root".into(),
            tgt: "item".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        let arcs = vec![(0, 1, edge.clone())];
        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let mut between = HashMap::new();
        between.insert(
            (Name::from("root"), Name::from("item")),
            smallvec![edge.clone()],
        );
        let tgt_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
            between,
        };
        let src_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
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
        };

        // No conditional_survival predicates — node should survive normally
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("item")]),
            surviving_edges: HashSet::from([edge]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
        };

        let result =
            wtype_restrict(&inst, &src_schema, &tgt_schema, &migration).expect("restrict ok");

        assert_eq!(result.node_count(), 2);
        assert!(result.nodes.contains_key(&1));
    }

    // --- ComputeField tests ---

    #[test]
    fn computed_field_template_name() {
        let mut node = Node::new(0, "heading");
        node.extra_fields.insert("level".to_string(), Value::Int(2));

        // (concat "h" (int_to_str level))
        let expr = panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Concat,
            vec![
                panproto_expr::Expr::Lit(panproto_expr::Literal::Str("h".to_string())),
                panproto_expr::Expr::Builtin(
                    panproto_expr::BuiltinOp::IntToStr,
                    vec![panproto_expr::Expr::Var(std::sync::Arc::from("level"))],
                ),
            ],
        );

        let transform = FieldTransform::ComputeField {
            target_key: "name".to_string(),
            expr,
        };
        apply_field_transforms(&mut node, &[transform]);

        assert_eq!(
            node.extra_fields.get("name"),
            Some(&Value::Str("h2".into()))
        );
    }

    #[test]
    fn computed_field_reads_nested_attrs() {
        let mut node = Node::new(0, "heading");
        let mut attrs = HashMap::new();
        attrs.insert("level".to_string(), Value::Int(3));
        node.extra_fields
            .insert("attrs".to_string(), Value::Unknown(attrs));

        // (concat "h" (int_to_str attrs.level))
        let expr = panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Concat,
            vec![
                panproto_expr::Expr::Lit(panproto_expr::Literal::Str("h".to_string())),
                panproto_expr::Expr::Builtin(
                    panproto_expr::BuiltinOp::IntToStr,
                    vec![panproto_expr::Expr::Var(std::sync::Arc::from(
                        "attrs.level",
                    ))],
                ),
            ],
        );

        let transform = FieldTransform::ComputeField {
            target_key: "name".to_string(),
            expr,
        };
        apply_field_transforms(&mut node, &[transform]);

        assert_eq!(
            node.extra_fields.get("name"),
            Some(&Value::Str("h3".into()))
        );
    }

    #[test]
    fn case_transform_sets_field_conditionally() {
        use crate::value::Value;
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use std::sync::Arc;

        let mut node = Node::new(0, "heading");
        node.extra_fields.insert("level".into(), Value::Int(1));
        node.extra_fields
            .insert("name".into(), Value::Str("heading".into()));

        let case = FieldTransform::Case {
            branches: vec![
                CaseBranch {
                    predicate: Expr::builtin(
                        BuiltinOp::Eq,
                        vec![Expr::Var(Arc::from("level")), Expr::Lit(Literal::Int(1))],
                    ),
                    transforms: vec![FieldTransform::ComputeField {
                        target_key: "name".into(),
                        expr: Expr::Lit(Literal::Str("h1".into())),
                    }],
                },
                CaseBranch {
                    predicate: Expr::builtin(
                        BuiltinOp::Eq,
                        vec![Expr::Var(Arc::from("level")), Expr::Lit(Literal::Int(2))],
                    ),
                    transforms: vec![FieldTransform::ComputeField {
                        target_key: "name".into(),
                        expr: Expr::Lit(Literal::Str("h2".into())),
                    }],
                },
            ],
        };

        apply_field_transforms(&mut node, &[case]);

        assert_eq!(
            node.extra_fields.get("name"),
            Some(&Value::Str("h1".into()))
        );
    }
}
