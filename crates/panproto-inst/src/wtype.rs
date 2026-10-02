//! W-type instance representation and the `wtype_restrict` pipeline.
//!
//! A [`WInstance`] is a tree-shaped data instance conforming to a schema.
//! The restrict operation (`wtype_restrict`) is a fused single-pass pipeline
//! that projects a W-type instance along a migration mapping.
//!
//! The pipeline fuses four concerns into one BFS traversal:
//! 1. Anchor survival check: does this node's schema vertex survive?
//! 2. Reachability: is this node reachable from the root?
//! 3. Ancestor contraction: who is the nearest surviving ancestor?
//! 4. Edge resolution: what edge connects the contracted arc?
//!
//! Fan reconstruction (step 5) runs as a separate pass since it operates
//! on the original fan list, not the BFS tree.
//!
//! The five individual step functions are retained for testing and debugging.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::error::RestrictError;
use crate::fan::Fan;
use crate::metadata::Node;
use crate::value::Value;

/// A resolver entry: the hyper-edge a shape retargets to, and the label
/// remapping its children take.
pub type HyperResolverEntry = (Name, HashMap<Name, Name>);

/// A fan shape: a hyper-edge ID with the sorted, deduplicated label set that
/// selects one of that hyper-edge's resolver entries.
pub type FanShape = (Name, Vec<Name>);

/// The compiled hyper-edge contraction table.
///
/// Maps a fan shape — a hyper-edge ID together with the sorted, deduplicated
/// set of child labels the fan carries — to the target hyper-edge ID and the
/// label remapping that shape's children take.
pub type HyperResolverTable = HashMap<FanShape, HyperResolverEntry>;

/// A compiled migration specification (minimal version for panproto-inst).
///
/// The full `CompiledMigration` lives in `panproto-mig`. This type provides
/// the subset of fields that `wtype_restrict` and `functor_restrict` need.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    /// Hyper-edge contraction resolver, keyed by fan shape.
    ///
    /// The key is a hyper-edge ID paired with the sorted, deduplicated set of
    /// child labels the fan carries, so one hyper-edge may retarget
    /// differently per shape. The value is the target hyper-edge ID and the
    /// label remapping to apply to that shape's children.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub hyper_resolver: HyperResolverTable,
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
    /// Value-level op-to-term assignments applied to surviving rows/nodes.
    ///
    /// Keyed by source vertex anchor, mirroring [`Self::field_transforms`].
    /// Each assignment computes a migrated field by substituting the row's
    /// field values into a term; this is how a migration acts on values,
    /// whichever way it carries them. `panproto-mig`'s compiler emits its
    /// value transforms here rather than as direct [`FieldTransform`]s.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub op_term_assignments: HashMap<Name, Vec<TermAssignment>>,
    /// Multi-hop expansion paths for nest-style migrations.
    ///
    /// When a direct edge `src --> tgt` existed in the source schema but
    /// only a multi-hop path `src --> i1 --> i2 --> ... --> tgt` exists in
    /// the target (as happens after `combinators::nest_field`), this map
    /// records the sequence of intermediate target anchor ids to insert
    /// when walking the source arc during `wtype_restrict`. The value is
    /// the intermediate anchors only (endpoints excluded), ordered from
    /// parent-adjacent to child-adjacent.
    ///
    /// Dual of the ancestor-contraction mechanism: contraction collapses
    /// a path to a direct arc (hoist), expansion fans a direct arc out
    /// into a path by materializing fresh view nodes (nest).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub expansion_path: HashMap<(Name, Name), Vec<Name>>,
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
        /// Optional inverse expression for round-tripping.
        inverse: Option<panproto_expr::Expr>,
        /// Round-trip classification of this transformation.
        coercion_class: panproto_gat::CoercionClass,
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
    /// endomorphism algebra of field transforms; it lifts a transform
    /// from a leaf to an inner node of the Value tree.
    PathTransform {
        /// Path to navigate (e.g., `vec!["attrs"]` for nested attrs objects).
        path: Vec<String>,
        /// The transform to apply at the resolved path.
        inner: Box<Self>,
    },
    /// Compute a field value from an expression with access to the full
    /// fiber over the parent vertex.
    ///
    /// Unlike `ApplyExpr` which binds a single field, `ComputeField` binds
    /// all `extra_fields`, nested attrs, AND scalar values from immediate
    /// child nodes (the dependent-sum projection) as variables, evaluates
    /// the expression, and stores the result in the target field.
    ///
    /// This means `ComputeField` can access any scalar property of the
    /// parent object, whether it was parsed as an extra field or as a
    /// schema-defined child vertex (e.g., a string field with a `"format"`
    /// annotation like `"at-uri"`). Computed results are always written to
    /// `extra_fields`, making them available to subsequent transforms and
    /// to `to_json` serialization (where `extra_fields` overwrite child
    /// values with the same key).
    ///
    /// Computed fields are classified by `coercion_class`:
    /// - `Iso`: the computation is invertible via `inverse`; the lens law
    ///   `PutGet` holds for modifications to the computed field.
    /// - `Opaque`: no inverse exists; the complement stores the entire
    ///   original value. Modifications to the computed field in the view
    ///   are not independently round-trippable. This is analogous to SQL
    ///   computed columns: the lens law holds for the independent
    ///   (non-derived) components of the view, and the derived components
    ///   are re-computed deterministically.
    ///
    /// This enables template name computation like
    ///   `target_key`: "name",
    ///   `expr`: `(concat "h" (int_to_str attrs.level))`
    /// which computes "h1", "h2", etc. from the level attribute, as well
    /// as AT-URI decomposition where the `repo` field is a schema-defined
    /// child vertex.
    ComputeField {
        /// The field to store the computed result in.
        target_key: String,
        /// The expression, with all `extra_fields` bound as variables.
        expr: panproto_expr::Expr,
        /// Optional inverse expression for round-tripping.
        inverse: Option<panproto_expr::Expr>,
        /// Round-trip classification of this transformation.
        coercion_class: panproto_gat::CoercionClass,
    },
    /// Case analysis on node values: the coproduct eliminator for the
    /// field transform algebra.
    ///
    /// Each branch is a (predicate, transforms) pair. Branches are evaluated
    /// in order with the node's `extra_fields` (and nested `attrs.*` keys)
    /// bound as expression variables. The first branch whose predicate
    /// evaluates to `true` has its transforms applied. If no branch matches,
    /// the node passes through unchanged.
    ///
    /// This is the dependent function space lift of field transforms:
    /// `Π(x : Value). FieldTransform`, a transform that depends on the
    /// runtime value of the node, not just its schema vertex. It composes
    /// naturally with all other transform variants (including nesting
    /// inside `PathTransform`).
    ///
    /// Use cases:
    /// - `matchAttrs`: "if `level == 1` then rename to `h1`, if `level == 2`
    ///   then rename to `h2`", where each heading level is a branch.
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
    /// - If the value is a `Value::List`, each string element is checked
    ///   and the list is rebuilt with renames applied and drops removed.
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

impl FieldTransform {
    /// Compute the coercion class of this field transform.
    ///
    /// The class describes the round-trip properties: whether the transform
    /// is lossless (`Iso`), has a left inverse (`Retraction`), is a
    /// deterministic derivation (`Projection`), or has no structural
    /// round-trip property (`Opaque`).
    #[must_use]
    pub fn coercion_class(&self) -> panproto_gat::CoercionClass {
        match self {
            Self::RenameField { .. } => panproto_gat::CoercionClass::Iso,
            Self::DropField { .. } | Self::KeepFields { .. } => panproto_gat::CoercionClass::Opaque,
            Self::AddField { .. } | Self::MapReferences { .. } => {
                panproto_gat::CoercionClass::Retraction
            }
            Self::ApplyExpr { coercion_class, .. } | Self::ComputeField { coercion_class, .. } => {
                *coercion_class
            }
            Self::PathTransform { inner, .. } => inner.coercion_class(),
            Self::Case { branches } => branches
                .iter()
                .flat_map(|b| b.transforms.iter())
                .fold(panproto_gat::CoercionClass::Iso, |acc, t| {
                    acc.compose(t.coercion_class())
                }),
        }
    }
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

/// The scope of field values a [`TermAssignment::Compute`] term sees.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TermScope {
    /// Bind only the target field (single-field substitution). The term's
    /// free variable is the target field's own name.
    Field,
    /// Bind the whole row: every field plus, for tree instances, the
    /// scalar values of immediate child nodes.
    Row,
}

/// A branch of a [`TermAssignment::Case`] analysis.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TermBranch {
    /// Predicate evaluated with the row's values bound as variables.
    pub predicate: panproto_expr::Expr,
    /// Assignments applied when the predicate holds.
    pub assignments: Vec<TermAssignment>,
}

/// An op-to-term assignment: how one migrated field is produced from a
/// source row.
///
/// A [`Self::Compute`] assignment carries a term (`panproto_expr::Expr`)
/// whose free variables are source field names; evaluating it substitutes
/// the row's field values for those variables. This is the substitution
/// semantics through which a migration acts on values: the surviving-fragment
/// forms ([`wtype_restrict`], [`crate::functor_restrict`]) and the
/// total ones ([`wtype_extend`], [`crate::functor_extend`]) alike
/// compute migrated columns by substituting each carried row. The remaining
/// variants
/// describe structural field operations (rename, drop, keep, default,
/// reference remap, nested-path scoping, and case analysis).
///
/// Every [`FieldTransform`] variant translates to a `TermAssignment` via
/// [`Self::from_field_transform`] and lowers back via
/// [`Self::to_field_transform`]; applying a translated assignment produces
/// the same result as applying the original transform. Migrations produced
/// by `panproto-mig`'s compiler carry their value transforms as term
/// assignments rather than direct [`FieldTransform`]s.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TermAssignment {
    /// Compute `target` by substituting the row's values into `term`.
    Compute {
        /// The migrated field written by this assignment.
        target: String,
        /// Which source fields the term sees.
        scope: TermScope,
        /// The term computing the field.
        term: panproto_expr::Expr,
        /// Optional inverse for round-tripping.
        inverse: Option<panproto_expr::Expr>,
        /// Round-trip classification of this assignment.
        coercion_class: panproto_gat::CoercionClass,
    },
    /// Rename field `old` to `new`.
    Rename {
        /// The current field name.
        old: String,
        /// The new field name.
        new: String,
    },
    /// Drop field `key`.
    Drop {
        /// The field to remove.
        key: String,
    },
    /// Add `value` at `key` when the field is absent.
    Default {
        /// The field to add.
        key: String,
        /// The default value.
        value: Value,
    },
    /// Keep only the listed fields.
    Keep {
        /// The fields to retain.
        keys: Vec<String>,
    },
    /// Remap string references in `field` through `rename_map`.
    MapReferences {
        /// The field carrying references.
        field: String,
        /// Old-name to new-name map (`None` removes the reference).
        rename_map: HashMap<String, Option<String>>,
    },
    /// Apply `inner` at a nested `path` within the row's `Value` tree.
    AtPath {
        /// The path of nested object keys.
        path: Vec<String>,
        /// The assignment applied at the resolved path.
        inner: Box<Self>,
    },
    /// First matching branch's assignments apply.
    Case {
        /// Ordered branches; the first matching predicate wins.
        branches: Vec<TermBranch>,
    },
}

impl TermAssignment {
    /// Translate a [`FieldTransform`] into the equivalent term assignment.
    #[must_use]
    pub fn from_field_transform(ft: &FieldTransform) -> Self {
        match ft {
            FieldTransform::RenameField { old_key, new_key } => Self::Rename {
                old: old_key.clone(),
                new: new_key.clone(),
            },
            FieldTransform::DropField { key } => Self::Drop { key: key.clone() },
            FieldTransform::AddField { key, value } => Self::Default {
                key: key.clone(),
                value: value.clone(),
            },
            FieldTransform::KeepFields { keys } => Self::Keep { keys: keys.clone() },
            FieldTransform::ApplyExpr {
                key,
                expr,
                inverse,
                coercion_class,
            } => Self::Compute {
                target: key.clone(),
                scope: TermScope::Field,
                term: expr.clone(),
                inverse: inverse.clone(),
                coercion_class: *coercion_class,
            },
            FieldTransform::ComputeField {
                target_key,
                expr,
                inverse,
                coercion_class,
            } => Self::Compute {
                target: target_key.clone(),
                scope: TermScope::Row,
                term: expr.clone(),
                inverse: inverse.clone(),
                coercion_class: *coercion_class,
            },
            FieldTransform::PathTransform { path, inner } => Self::AtPath {
                path: path.clone(),
                inner: Box::new(Self::from_field_transform(inner)),
            },
            FieldTransform::MapReferences { field, rename_map } => Self::MapReferences {
                field: field.clone(),
                rename_map: rename_map.clone(),
            },
            FieldTransform::Case { branches } => Self::Case {
                branches: branches
                    .iter()
                    .map(|b| TermBranch {
                        predicate: b.predicate.clone(),
                        assignments: b
                            .transforms
                            .iter()
                            .map(Self::from_field_transform)
                            .collect(),
                    })
                    .collect(),
            },
        }
    }

    /// Lower this term assignment to the equivalent [`FieldTransform`].
    #[must_use]
    pub fn to_field_transform(&self) -> FieldTransform {
        match self {
            Self::Rename { old, new } => FieldTransform::RenameField {
                old_key: old.clone(),
                new_key: new.clone(),
            },
            Self::Drop { key } => FieldTransform::DropField { key: key.clone() },
            Self::Default { key, value } => FieldTransform::AddField {
                key: key.clone(),
                value: value.clone(),
            },
            Self::Keep { keys } => FieldTransform::KeepFields { keys: keys.clone() },
            Self::Compute {
                target,
                scope: TermScope::Field,
                term,
                inverse,
                coercion_class,
            } => FieldTransform::ApplyExpr {
                key: target.clone(),
                expr: term.clone(),
                inverse: inverse.clone(),
                coercion_class: *coercion_class,
            },
            Self::Compute {
                target,
                scope: TermScope::Row,
                term,
                inverse,
                coercion_class,
            } => FieldTransform::ComputeField {
                target_key: target.clone(),
                expr: term.clone(),
                inverse: inverse.clone(),
                coercion_class: *coercion_class,
            },
            Self::MapReferences { field, rename_map } => FieldTransform::MapReferences {
                field: field.clone(),
                rename_map: rename_map.clone(),
            },
            Self::AtPath { path, inner } => FieldTransform::PathTransform {
                path: path.clone(),
                inner: Box::new(inner.to_field_transform()),
            },
            Self::Case { branches } => FieldTransform::Case {
                branches: branches
                    .iter()
                    .map(|b| CaseBranch {
                        predicate: b.predicate.clone(),
                        transforms: b.assignments.iter().map(Self::to_field_transform).collect(),
                    })
                    .collect(),
            },
        }
    }

    /// Round-trip classification of this assignment, matching the
    /// classification of its lowered [`FieldTransform`].
    #[must_use]
    pub fn coercion_class(&self) -> panproto_gat::CoercionClass {
        self.to_field_transform().coercion_class()
    }
}

/// Apply a sequence of op-to-term assignments to a flat relational row,
/// substituting the row's field values.
///
/// The row is wrapped in a scratch node so the shared field-transform
/// evaluator ([`apply_field_transforms`]) performs the substitution; a
/// flat row has no child fibers, so the child-scalar environment is empty.
/// This is how a migration acts on the values of a set-valued (relational)
/// instance.
///
/// # Errors
///
/// Returns [`RestrictError::FieldTransformFailed`] if an assignment's
/// lowered transform fails to evaluate. The row is restored to its
/// pre-transform contents before the error propagates.
pub fn apply_term_assignments_to_row(
    row: &mut HashMap<String, Value>,
    assignments: &[TermAssignment],
) -> Result<(), RestrictError> {
    if assignments.is_empty() {
        return Ok(());
    }
    let transforms: Vec<FieldTransform> = assignments
        .iter()
        .map(TermAssignment::to_field_transform)
        .collect();
    let mut node = Node::new(0, "");
    node.extra_fields = std::mem::take(row);
    let outcome = apply_field_transforms(&mut node, &transforms, &TransformContext::detached());
    // Restore the row whether or not the transforms succeeded: `mem::take`
    // emptied it, so an early return would hand back a blank row.
    *row = node.extra_fields;
    outcome
}

impl CompiledMigration {
    /// Compute the composite coercion class of every value transform in
    /// this migration.
    ///
    /// Folds over all vertices using `CoercionClass::compose`, starting
    /// from `Iso` (the identity element), so a migration carrying no value
    /// transforms classifies as `Iso` and one carrying a single `Opaque`
    /// transform classifies as `Opaque`.
    ///
    /// Both carriers are folded: the direct [`FieldTransform`]s and the
    /// lowered [`TermAssignment`]s. `panproto-mig`'s compiler emits its
    /// value transforms as term assignments rather than as field
    /// transforms, so folding only the latter would report `Iso` — the
    /// identity element of an empty fold — for exactly the migrations that
    /// carry the most value-level coercion.
    #[must_use]
    pub fn coercion_class(&self) -> panproto_gat::CoercionClass {
        let from_fields = self
            .field_transforms
            .values()
            .flat_map(|ts| ts.iter())
            .fold(panproto_gat::CoercionClass::Iso, |acc, t| {
                acc.compose(t.coercion_class())
            });
        self.op_term_assignments
            .values()
            .flat_map(|ts| ts.iter())
            .fold(from_fields, |acc, t| acc.compose(t.coercion_class()))
    }

    /// All value transforms for `anchor`: the legacy [`FieldTransform`]s
    /// followed by the lowered op-to-term assignments.
    ///
    /// Tree-instance consumers apply this unified sequence so that a
    /// migration whose value transforms are carried as op-to-term
    /// assignments behaves the same as one carrying direct field
    /// transforms.
    #[must_use]
    pub fn value_transforms(&self, anchor: &Name) -> Vec<FieldTransform> {
        let mut out: Vec<FieldTransform> = self
            .field_transforms
            .get(anchor)
            .cloned()
            .unwrap_or_default();
        if let Some(assignments) = self.op_term_assignments.get(anchor) {
            out.extend(assignments.iter().map(TermAssignment::to_field_transform));
        }
        out
    }

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
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
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
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
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
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
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
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
    pub parent_map: HashMap<u32, u32>,
    /// Precomputed children map: `parent_id` -> child IDs.
    #[serde(with = "panproto_schema::serde_helpers::sorted_map")]
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
    // Check resolver: avoid allocation by scanning for matching key
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

/// The canonical label-set component of a `hyper_resolver` key: sorted and
/// deduplicated, so a fan's shape is independent of child insertion order.
#[must_use]
pub fn canonical_label_shape<I, S>(labels: I) -> Vec<Name>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut shape: Vec<Name> = labels.into_iter().map(|l| Name::from(l.as_ref())).collect();
    shape.sort_unstable();
    shape.dedup();
    shape
}

/// Borrowed resolver entries grouped under the target hyper-edge they produce.
type BackwardIndex<'a> = FxHashMap<&'a str, Vec<(&'a FanShape, &'a HyperResolverEntry)>>;

/// Fan-shape lookup over a compiled migration's hyper-edge resolver.
///
/// Building the index once and querying it per fan keeps resolution linear in
/// the number of fans, and it makes both directions total orders rather than
/// scans whose answer would depend on the table's iteration order.
pub struct FanResolver<'a> {
    table: &'a HyperResolverTable,
    /// Hyper-edge ID to its sole entry, recorded only for hyper-edges that
    /// carry exactly one shape. A fan matching no shape exactly still resolves
    /// when its hyper-edge has a single unambiguous retarget; when several
    /// shapes compete, nothing is chosen, since picking one would depend on
    /// table order.
    sole_forward: FxHashMap<&'a str, Option<&'a HyperResolverEntry>>,
    /// Target hyper-edge ID to every entry that retargets onto it, ordered by
    /// source key so the backward direction is a function of the table.
    backward: BackwardIndex<'a>,
}

impl<'a> FanResolver<'a> {
    /// Index a compiled migration's hyper-edge resolver.
    #[must_use]
    pub fn new(migration: &'a CompiledMigration) -> Self {
        let mut sole_forward: FxHashMap<&'a str, Option<&'a HyperResolverEntry>> =
            FxHashMap::default();
        let mut backward: BackwardIndex<'a> = FxHashMap::default();

        for (key, entry) in &migration.hyper_resolver {
            sole_forward
                .entry(key.0.as_str())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(entry));
            backward
                .entry(entry.0.as_str())
                .or_default()
                .push((key, entry));
        }
        for candidates in backward.values_mut() {
            candidates.sort_unstable_by(|a, b| a.0.cmp(b.0));
        }

        Self {
            table: &migration.hyper_resolver,
            sole_forward,
            backward,
        }
    }

    /// Select the entry that governs a fan carrying `shape`, where
    /// `full_shape` is the fan's shape before any children were pruned.
    ///
    /// `shape` is tried first, since restriction has already dropped the
    /// pruned children; `full_shape` is tried next, so a resolver written
    /// against the unrestricted source still applies; failing both, the
    /// hyper-edge's sole entry applies when it has exactly one.
    #[must_use]
    pub fn resolve(
        &self,
        hyper_edge_id: &str,
        shape: &[Name],
        full_shape: &[Name],
    ) -> Option<&'a HyperResolverEntry> {
        let he = Name::from(hyper_edge_id);
        if let Some(entry) = self.table.get(&(he.clone(), shape.to_vec())) {
            return Some(entry);
        }
        if full_shape != shape
            && let Some(entry) = self.table.get(&(he, full_shape.to_vec()))
        {
            return Some(entry);
        }
        self.sole_forward.get(hyper_edge_id).copied().flatten()
    }

    /// Select the entry that produced a fan now carrying `shape` under the
    /// target hyper-edge `hyper_edge_id`, returning the source hyper-edge ID
    /// alongside it.
    ///
    /// Preference goes to the entry whose label remapping carries its own
    /// shape exactly onto `shape`; where none does, the least entry in source
    /// key order applies, so the answer is a function of the table.
    #[must_use]
    pub fn resolve_backward(
        &self,
        hyper_edge_id: &str,
        shape: &[Name],
    ) -> Option<(&'a Name, &'a HyperResolverEntry)> {
        let candidates = self.backward.get(hyper_edge_id)?;
        let exact = candidates.iter().find(|((_, source_shape), (_, labels))| {
            let image = canonical_label_shape(source_shape.iter().map(|label| {
                labels
                    .get(label)
                    .map_or_else(|| label.as_str(), Name::as_str)
            }));
            image == shape
        });
        let ((source_he, _), entry) = exact.or_else(|| candidates.first())?;
        Some((source_he, entry))
    }
}

/// Reconstruct fans after restriction.
///
/// Fans are taken in `fans` order. A fan survives when its parent and at
/// least one child survive; when the resolver governs its surviving shape it
/// is retargeted and its surviving labels renamed.
///
/// # Errors
///
/// Returns `RestrictError::FanReconstructionFailed` when the renaming sends
/// two surviving children to one label, since the fan could then keep only
/// one of them.
pub fn reconstruct_fans(
    instance: &WInstance,
    surviving: &FxHashSet<u32>,
    _ancestors: &FxHashMap<u32, u32>,
    migration: &CompiledMigration,
    _tgt_schema: &Schema,
) -> Result<Vec<Fan>, RestrictError> {
    let mut result = Vec::new();
    let resolver = FanResolver::new(migration);

    for fan in &instance.fans {
        if !surviving.contains(&fan.parent) {
            continue;
        }

        let mut surviving_children: Vec<(&String, u32)> = fan
            .children
            .iter()
            .filter(|(_, node_id)| surviving.contains(node_id))
            .map(|(label, node_id)| (label, *node_id))
            .collect();
        if surviving_children.is_empty() {
            continue;
        }
        surviving_children.sort_unstable();

        let surviving_shape = canonical_label_shape(surviving_children.iter().map(|(l, _)| l));
        let full_shape = canonical_label_shape(fan.children.keys());
        let entry = resolver.resolve(&fan.hyper_edge_id, &surviving_shape, &full_shape);
        let hyper_edge_id =
            entry.map_or_else(|| fan.hyper_edge_id.clone(), |(he, _)| he.to_string());
        let mut children = HashMap::new();
        for (old_label, node_id) in surviving_children {
            let new_label = entry
                .and_then(|(_, labels)| labels.get(old_label.as_str()))
                .map_or_else(|| old_label.clone(), ToString::to_string);
            if children.contains_key(&new_label) {
                return Err(RestrictError::FanReconstructionFailed {
                    hyper_edge_id,
                    detail: format!("two children are relabelled `{new_label}`"),
                });
            }
            children.insert(new_label, node_id);
        }
        result.push(Fan {
            hyper_edge_id,
            parent: fan.parent,
            children,
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// The tree restrict engine
// ---------------------------------------------------------------------------

/// What the tree restrict synthesized, for tree get to record in its
/// complement: the node id floor it allocated from and the key of every
/// node it synthesized.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeSynthesis {
    /// The tree floor: one above every node id of the source's graph form
    /// (tree nodes and fan nodes), saturating at `u32::MAX`.
    pub id_floor: u32,
    /// Each synthesized node, by id, with its key.
    pub synthesized_keys: BTreeMap<u32, crate::view_ref::SynthKey>,
}

/// A path of synthesized elements a restrict emits: the one edge of a
/// contracted pair, or the nodes and edges of an expansion.
pub(crate) enum SynthChain {
    /// A contracted pair resolved to one edge.
    Contract { src: u32, tgt: u32, edge: Edge },
    /// A source edge (a tree arc, by its edge id in the tree's graph form)
    /// expanded into a path.
    Expand {
        arc: crate::ginstance::EdgeId,
        parent: u32,
        child: u32,
        intermediates: Vec<Name>,
        labels: Vec<Edge>,
    },
    /// A contracted pair resolved to a path.
    ContractExpand {
        src: u32,
        tgt: u32,
        intermediates: Vec<Name>,
        labels: Vec<Edge>,
    },
}

/// The hop of a path position as a key component.
fn hop(h: usize) -> Result<u16, RestrictError> {
    u16::try_from(h).map_err(|_| RestrictError::IdsExhausted {
        kind: crate::segment::IdKind::Node,
    })
}

impl SynthChain {
    /// The keys of the chain's synthesized nodes, one per intermediate.
    pub(crate) fn node_keys(
        &self,
    ) -> Result<Vec<(crate::view_ref::SynthKey, Name)>, RestrictError> {
        use crate::view_ref::SynthKey;
        let (intermediates, key): (&[Name], &dyn Fn(u16) -> SynthKey) = match self {
            Self::Contract { .. } => return Ok(Vec::new()),
            Self::Expand {
                arc, intermediates, ..
            } => (intermediates, &|h| SynthKey::Expand {
                source_edge: *arc,
                hop: h,
            }),
            Self::ContractExpand {
                src,
                tgt,
                intermediates,
                ..
            } => (intermediates, &|h| SynthKey::ContractExpand {
                src: *src,
                tgt: *tgt,
                hop: h,
            }),
        };
        intermediates
            .iter()
            .enumerate()
            .map(|(h, anchor)| Ok((key(hop(h)?), anchor.clone())))
            .collect()
    }

    /// The chain's synthesized arcs, each with its key, given the ids of
    /// the synthesized nodes.
    pub(crate) fn arcs(
        &self,
        ids: &BTreeMap<crate::view_ref::SynthKey, u32>,
    ) -> Result<Vec<(crate::view_ref::SynthKey, u32, u32, Edge)>, RestrictError> {
        use crate::view_ref::SynthKey;
        let (from, to, labels, key): (u32, u32, &[Edge], &dyn Fn(u16) -> SynthKey) = match self {
            Self::Contract { src, tgt, edge } => {
                return Ok(vec![(
                    SynthKey::Contract {
                        src: *src,
                        tgt: *tgt,
                        edge: edge.clone(),
                    },
                    *src,
                    *tgt,
                    edge.clone(),
                )]);
            }
            Self::Expand {
                arc,
                parent,
                child,
                labels,
                ..
            } => (*parent, *child, labels, &|h| SynthKey::Expand {
                source_edge: *arc,
                hop: h,
            }),
            Self::ContractExpand {
                src, tgt, labels, ..
            } => (*src, *tgt, labels, &|h| SynthKey::ContractExpand {
                src: *src,
                tgt: *tgt,
                hop: h,
            }),
        };
        let last = labels.len().saturating_sub(1);
        let mut previous = from;
        let mut arcs = Vec::with_capacity(labels.len());
        for (h, label) in labels.iter().enumerate() {
            let key = key(hop(h)?);
            let next = if h == last {
                to
            } else {
                ids.get(&key).copied().ok_or(RestrictError::IdsExhausted {
                    kind: crate::segment::IdKind::Node,
                })?
            };
            arcs.push((key, previous, next, label.clone()));
            previous = next;
        }
        Ok(arcs)
    }
}

/// The nodes whose survival predicate is false, among those whose anchor
/// survives, evaluated before any transform and in ascending node id.
fn failing_predicates(
    instance: &WInstance,
    plan: &crate::restrict_plan::RestrictPlan,
    children: &TreeChildren<'_>,
) -> Result<HashSet<u32>, RestrictError> {
    let mut ids: Vec<u32> = instance.nodes.keys().copied().collect();
    ids.sort_unstable();
    let mut failing: HashSet<u32> = HashSet::new();
    for id in ids {
        let node = &instance.nodes[&id];
        if !plan.anchor_survives(&node.anchor)
            || !plan
                .migration()
                .conditional_survival
                .contains_key(&node.anchor)
        {
            continue;
        }
        let ctx = TransformContext::bound(
            Some(plan.source()),
            children,
            InstanceRef::Tree(instance, id),
            &plan.demand(&node.anchor),
        );
        if !plan.passes_predicate(&node.anchor, &ctx, &node.extra_fields)? {
            failing.insert(id);
        }
    }
    Ok(failing)
}

/// What the tree traversal keeps: the surviving source nodes, the kept arcs
/// with their source index, and the synthesized paths.
struct Traversal {
    surviving: BTreeSet<u32>,
    kept: Vec<(usize, u32, u32, Edge)>,
    chains: Vec<SynthChain>,
}

/// The breadth-first traversal of the tree from its root, deciding which
/// nodes the view keeps and how each is joined to its parent in the view.
fn traverse(
    instance: &WInstance,
    plan: &crate::restrict_plan::RestrictPlan,
    children: &TreeChildren<'_>,
    failing: &HashSet<u32>,
) -> Result<Traversal, RestrictError> {
    let survives = |id: u32| {
        instance
            .nodes
            .get(&id)
            .is_some_and(|n| plan.anchor_survives(&n.anchor) && !failing.contains(&id))
    };
    let image = |id: u32| plan.target_anchor(&instance.nodes[&id].anchor);
    let mut surviving: BTreeSet<u32> = BTreeSet::from([instance.root]);
    let mut kept: Vec<(usize, u32, u32, Edge)> = Vec::new();
    let mut chains: Vec<SynthChain> = Vec::new();
    let mut queue: VecDeque<(u32, Option<u32>)> = VecDeque::from([(instance.root, None)]);
    while let Some((current, ancestor)) = queue.pop_front() {
        let current_survives = surviving.contains(&current);
        let child_ancestor = if current_survives {
            Some(current)
        } else {
            ancestor
        };
        for &index in children
            .by_parent
            .get(&current)
            .map_or(&[][..], Vec::as_slice)
        {
            let (parent, child, ref label) = instance.arcs[index];
            if !instance.nodes.contains_key(&child) {
                continue;
            }
            if survives(child) {
                if current_survives {
                    match plan.edge_image.fate(label) {
                        None => {
                            return Err(RestrictError::IncoherentEdgeImage {
                                source_edge: Box::new(label.clone()),
                                image: Box::new(label.clone()),
                            });
                        }
                        Some(crate::restrict_plan::EdgeFate::Keep(edge)) => {
                            kept.push((index, parent, child, edge.clone()));
                        }
                        Some(crate::restrict_plan::EdgeFate::Expand {
                            intermediates,
                            labels,
                        }) => chains.push(SynthChain::Expand {
                            arc: crate::ginstance::EdgeId(u32::try_from(index).map_err(|_| {
                                RestrictError::IdsExhausted {
                                    kind: crate::segment::IdKind::Edge,
                                }
                            })?),
                            parent,
                            child,
                            intermediates: intermediates.clone(),
                            labels: labels.clone(),
                        }),
                        // The subtree under a dropped arc goes with it.
                        Some(crate::restrict_plan::EdgeFate::Drop) => continue,
                        Some(crate::restrict_plan::EdgeFate::Unresolved(reason)) => {
                            return Err(plan.unresolved(image(parent), image(child), *reason));
                        }
                    }
                } else {
                    let Some(source) = child_ancestor else {
                        return Err(RestrictError::RootPruned);
                    };
                    chains.push(contracted_chain(instance, plan, source, child)?);
                }
                surviving.insert(child);
            }
            queue.push_back((child, child_ancestor));
        }
    }
    Ok(Traversal {
        surviving,
        kept,
        chains,
    })
}

/// The path joining a surviving node to its nearest surviving ancestor
/// across dropped nodes: the plan's resolution of their anchors.
fn contracted_chain(
    instance: &WInstance,
    plan: &crate::restrict_plan::RestrictPlan,
    source: u32,
    child: u32,
) -> Result<SynthChain, RestrictError> {
    let source_anchor = &instance.nodes[&source].anchor;
    let child_anchor = &instance.nodes[&child].anchor;
    let Some(pair) = plan.contraction.pair(source_anchor, child_anchor) else {
        return Err(RestrictError::NoEdgeFound {
            src: plan.target_anchor(source_anchor).to_string(),
            tgt: plan.target_anchor(child_anchor).to_string(),
        });
    };
    match &pair.resolution {
        crate::contraction_plan::PairResolution::Label(edge) => Ok(SynthChain::Contract {
            src: source,
            tgt: child,
            edge: edge.clone(),
        }),
        crate::contraction_plan::PairResolution::Expand {
            intermediates,
            labels,
        } => Ok(SynthChain::ContractExpand {
            src: source,
            tgt: child,
            intermediates: intermediates.clone(),
            labels: labels.clone(),
        }),
        crate::contraction_plan::PairResolution::Unresolved(reason) => {
            Err(plan.unresolved(&pair.target.0, &pair.target.1, *reason))
        }
    }
}

/// The restrict of a W-type instance along a compiled plan: the single tree
/// engine.
///
/// The plan's precondition is the caller's: this function validates
/// nothing, so the instance should be valid against the plan's source
/// schema ([`wtype_restrict`] validates both boundaries). The result is
/// the view and what the restrict synthesized ([`TreeSynthesis`]).
///
/// A node survives when its anchor's image under the vertex map is a
/// surviving vertex and its vertex's survival predicate, evaluated over its
/// original fields and children, is `Bool(true)` (a predicate that does not
/// yield a boolean is refused). The root must survive. A surviving child of
/// a surviving parent travels along its arc's fate in the plan's
/// [`EdgeImage`](crate::EdgeImage): kept, relabelled; expanded into a path
/// of synthesized nodes; or dropped together with its subtree. A surviving
/// node whose parent does not survive is joined to its nearest surviving
/// ancestor by the plan's resolution of their pair: one edge, or a path. A
/// surviving node is re-anchored and transformed as every engine transforms
/// (value transforms, then the coercion of the columns they write when the
/// plan has a cell layer; trees apply no cell fates, since a tree's columns
/// are child nodes). Fans are rebuilt over the survivors.
///
/// Synthesized nodes take ids from the tree floor (one above every node of
/// the tree's graph form, fan nodes included) in ascending key order, never
/// `u32::MAX`. Each parent's arcs are its kept arcs in source arc order,
/// then its synthesized first hops in ascending key, which is the order the
/// graph view lists the same out-edges.
///
/// # Errors
///
/// In this order: [`RestrictError::RootPruned`] when the root's anchor does
/// not survive; the refusals of survival predicates, in ascending node id,
/// and `RootPruned` when the root's predicate is false;
/// [`RestrictError::IncoherentEdgeImage`] for an arc whose label is no
/// source schema edge, [`RestrictError::NoEdgeFound`] or
/// [`RestrictError::AmbiguousEdge`] for an unresolved arc or contracted
/// pair, in traversal order; [`RestrictError::IdsExhausted`] when the
/// synthesized nodes do not fit below `u32::MAX`; and the errors of the
/// transforms, in ascending node id.
pub fn wtype_restrict_with_plan(
    instance: &WInstance,
    plan: &crate::restrict_plan::RestrictPlan,
) -> Result<(WInstance, TreeSynthesis), RestrictError> {
    use crate::view_ref::SynthKey;

    let migration = plan.migration();
    let src = plan.source();
    let root = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;
    if !plan.anchor_survives(&root.anchor) {
        return Err(RestrictError::RootPruned);
    }
    let children = TreeChildren::new(instance);
    let failing = failing_predicates(instance, plan, &children)?;
    if failing.contains(&instance.root) {
        return Err(RestrictError::RootPruned);
    }
    let Traversal {
        surviving,
        mut kept,
        chains,
    } = traverse(instance, plan, &children, &failing)?;

    // Synthesized node ids, in ascending key order from the tree floor.
    let id_floor = crate::complement::tree_id_floor(instance).unwrap_or(u32::MAX);
    let mut synthesized_anchors: BTreeMap<SynthKey, Name> = BTreeMap::new();
    for chain in &chains {
        synthesized_anchors.extend(chain.node_keys()?);
    }
    let synthesized_ids = crate::restrict_plan::allocate_synthesized(
        synthesized_anchors.keys().cloned(),
        id_floor,
        crate::segment::IdKind::Node,
    )?;

    // The surviving nodes, re-anchored and transformed, in ascending id.
    let mut new_nodes: HashMap<u32, Node> = HashMap::with_capacity(surviving.len());
    for &id in &surviving {
        let source = &instance.nodes[&id];
        let mut node = source.clone();
        node.anchor.clone_from(plan.target_anchor(&source.anchor));
        if !migration.value_transforms(&source.anchor).is_empty() {
            let ctx = TransformContext::bound(
                Some(src),
                &children,
                InstanceRef::Tree(instance, id),
                &plan.demand(&source.anchor),
            );
            plan.transform(&mut node, &source.anchor, &ctx, false)?;
        }
        new_nodes.insert(id, node);
    }
    let mut synthesized_keys = BTreeMap::new();
    for (key, anchor) in &synthesized_anchors {
        let id = synthesized_ids[key];
        new_nodes.insert(id, Node::new(id, anchor.clone()));
        synthesized_keys.insert(id, key.clone());
    }

    // Kept arcs in source arc order, then synthesized arcs in key order.
    kept.sort_unstable_by_key(|(index, ..)| *index);
    let mut synthesized_arcs = Vec::new();
    for chain in &chains {
        synthesized_arcs.extend(chain.arcs(&synthesized_ids)?);
    }
    synthesized_arcs.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let new_arcs: Vec<(u32, u32, Edge)> = kept
        .into_iter()
        .map(|(_, p, c, e)| (p, c, e))
        .chain(synthesized_arcs.into_iter().map(|(_, p, c, e)| (p, c, e)))
        .collect();

    let surviving_fx: FxHashSet<u32> = surviving.iter().copied().collect();
    let new_fans = reconstruct_fans(
        instance,
        &surviving_fx,
        &FxHashMap::default(),
        migration,
        plan.target(),
    )?;
    let new_schema_root = plan.target_anchor(&instance.schema_root).clone();
    Ok((
        WInstance::new(
            new_nodes,
            new_arcs,
            new_fans,
            instance.root,
            new_schema_root,
        ),
        TreeSynthesis {
            id_floor,
            synthesized_keys,
        },
    ))
}

/// The restrict of a W-type instance from `src_schema` to `tgt_schema`
/// along `migration`, validating both boundaries, with what it
/// synthesized.
///
/// The input is validated against the source schema under
/// `protocols.src`; a report that does not pass is
/// [`RestrictError::Validation`] on the input side, and nothing is compiled
/// or run. Otherwise the plan is compiled with the protocols
/// ([`RestrictPlan::compile_with_protocols`]), the engine
/// ([`wtype_restrict_with_plan`]) runs, and the output is validated
/// against the target schema under `protocols.tgt`; a report that does not
/// pass is `Validation` on the output side. So a valid input yields the
/// plan's compilation error, the engine's error, an output-side failure, or
/// an output that validates.
///
/// [`RestrictPlan::compile_with_protocols`]: crate::RestrictPlan::compile_with_protocols
///
/// # Errors
///
/// As stated above.
pub fn wtype_restrict_with_synthesis(
    instance: &WInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: crate::restrict_boundary::BoundaryProtocols<'_>,
) -> Result<(WInstance, TreeSynthesis), RestrictError> {
    use crate::restrict_boundary::{BoundarySide, boundary_plan, require_passed, unlimited};
    let (budget, external) = unlimited();
    let src_plan = boundary_plan(src_schema, protocols.src);
    require_passed(
        src_plan.validate_tree(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let plan = crate::restrict_plan::RestrictPlan::compile_with_protocols(
        src_schema, tgt_schema, migration, protocols,
    )?;
    let restricted = wtype_restrict_with_plan(instance, &plan)?;
    let tgt_plan = boundary_plan(tgt_schema, protocols.tgt);
    require_passed(
        tgt_plan.validate_tree(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// The restrict of a W-type instance, validating both boundaries: the
/// first projection of [`wtype_restrict_with_synthesis`].
///
/// # Errors
///
/// As [`wtype_restrict_with_synthesis`].
pub fn wtype_restrict(
    instance: &WInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: crate::restrict_boundary::BoundaryProtocols<'_>,
) -> Result<WInstance, RestrictError> {
    wtype_restrict_with_synthesis(instance, src_schema, tgt_schema, migration, protocols)
        .map(|(view, _)| view)
}

/// [`wtype_restrict_with_synthesis`] over plans compiled already: the input
/// validated by `src_plan`, the engine run on `plan`, the output validated
/// by `tgt_plan`.
///
/// With the plans the schema-taking function compiles, it returns what that
/// function returns.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] before anything runs, on the input
/// side when `src_plan` was compiled for another schema than `plan`'s
/// source and then on the output side for `tgt_plan` and `plan`'s target;
/// otherwise as [`wtype_restrict_with_synthesis`] after compilation.
pub fn wtype_restrict_validated(
    instance: &WInstance,
    src_plan: &crate::validation::ValidationPlan,
    tgt_plan: &crate::validation::ValidationPlan,
    plan: &crate::restrict_plan::RestrictPlan,
) -> Result<(WInstance, TreeSynthesis), RestrictError> {
    use crate::restrict_boundary::{BoundarySide, require_passed, require_schema, unlimited};
    require_schema(src_plan, &plan.src_schema, BoundarySide::Input)?;
    require_schema(tgt_plan, &plan.tgt_schema, BoundarySide::Output)?;
    let (budget, external) = unlimited();
    require_passed(
        src_plan.validate_tree(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let restricted = wtype_restrict_with_plan(instance, plan)?;
    require_passed(
        tgt_plan.validate_tree(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// Resolve the image of a direct source arc by complete edge identity.
///
/// `edge_remap` is the explicit source-to-target map. An unchanged label and
/// kind may also carry across remapped endpoint vertices. The legacy
/// endpoint resolver is consulted only when the source endpoint pair has one
/// edge; with parallel source edges it cannot identify which edge it maps.
pub(crate) fn direct_source_edge_image(
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    parent_anchor: &Name,
    child_anchor: &Name,
    source_edge: &Edge,
) -> Option<Edge> {
    if let Some(mapped) = migration.edge_remap.get(source_edge) {
        return Some(mapped.clone());
    }

    if let Some(carried) = tgt_schema
        .edges_between(parent_anchor.as_str(), child_anchor.as_str())
        .iter()
        .find(|candidate| candidate.kind == source_edge.kind && candidate.name == source_edge.name)
    {
        return Some(carried.clone());
    }

    if src_schema
        .edges_between(source_edge.src.as_str(), source_edge.tgt.as_str())
        .len()
        == 1
    {
        for ((src, tgt), resolved) in &migration.resolver {
            if src == parent_anchor && tgt == child_anchor {
                return Some(resolved.clone());
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Value-level field transforms
// ---------------------------------------------------------------------------

/// Apply a sequence of field transforms to a node's `extra_fields`.
///
/// Called by every restrict engine after an element survives and its anchor
/// is remapped. Operations are applied in order.
///
/// Expressions read the node's `extra_fields` and the children bound in
/// `ctx` (the dependent-sum projection of the fiber over the vertex in the
/// Grothendieck fibration); `extra_fields` override children on a name
/// collision, since they may hold values an earlier transform of the same
/// sequence wrote. Before an expression is evaluated it is refused when it
/// reads a repeated child, a structural child that is no tree, or a value
/// with no expression literal (see [`TransformContext`]); only the names it
/// reads are converted.
///
/// Computed fields (via `ComputeField`) are derived data in the sense of
/// dependent projections: they are functionally determined by the source
/// fiber data. The `CoercionClass` on each `ComputeField` classifies the
/// round-trip behavior:
/// - `Iso`: the computation is invertible; `PutGet` holds for
///   modifications to the computed field (via the `inverse` expression).
/// - `Opaque`: no inverse exists; the complement stores the entire
///   original value. Modifications to the computed field in the view
///   are not independently round-trippable. This is analogous to SQL
///   computed columns or database views with derived columns. `PutGet`
///   holds for the independent (non-derived) components of the view,
///   and derived components are re-computed deterministically.
///
/// # Errors
///
/// Returns [`RestrictError::FieldTransformFailed`] if a transform's
/// expression fails to evaluate. A failed transform is reported rather
/// than skipped: leaving the field untouched makes a broken lens
/// indistinguishable from one that ran and changed nothing. Returns
/// [`RestrictError::AmbiguousChildField`],
/// [`RestrictError::StructuralChildNotATree`] or
/// [`RestrictError::ValueNotExpressible`] for an expression refused before
/// evaluation.
pub fn apply_field_transforms(
    node: &mut Node,
    transforms: &[FieldTransform],
    ctx: &TransformContext<'_>,
) -> Result<(), RestrictError> {
    let mut track = KeyTrack::over(node.extra_fields.keys());
    apply_transforms_tracked(node, transforms, ctx, &mut track)
}

/// What a run of transforms did to a node's `extra_fields` keys: the keys
/// it wrote, and, for each key holding a value carried from a key the node
/// had when the run began (moved there by renames, possibly changed in
/// place), that original key.
///
/// A key a transform computes afresh, or adds, has no origin; a key whose
/// value is changed in place keeps its origin. So after the run a source
/// key `c` is held by the view under the key whose origin is `c`, if any.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyTrack {
    /// The keys a transform wrote.
    pub(crate) written: BTreeSet<String>,
    /// For each key carrying an original key's value, that key.
    pub(crate) origin: BTreeMap<String, String>,
}

impl KeyTrack {
    /// The track at the start of a run over `keys`: nothing written, each
    /// key its own origin.
    pub(crate) fn over<'k>(keys: impl Iterator<Item = &'k String>) -> Self {
        Self {
            written: BTreeSet::new(),
            origin: keys.map(|k| (k.clone(), k.clone())).collect(),
        }
    }

    /// The key `from` was moved to `to`, replacing what `to` held.
    fn moved(&mut self, from: &str, to: &str) {
        let was_written = self.written.remove(from);
        let origin = self.origin.remove(from);
        if was_written {
            self.written.insert(to.to_owned());
        } else {
            self.written.remove(to);
        }
        match origin {
            Some(origin) => self.origin.insert(to.to_owned(), origin),
            None => self.origin.remove(to),
        };
    }

    /// The key was removed.
    pub(crate) fn removed(&mut self, key: &str) {
        self.written.remove(key);
        self.origin.remove(key);
    }

    /// The key was written with a fresh value, carried from no key.
    fn computed(&mut self, key: &str) {
        self.written.insert(key.to_owned());
        self.origin.remove(key);
    }

    /// The key's value was changed in place.
    fn changed(&mut self, key: &str) {
        self.written.insert(key.to_owned());
    }

    /// The key that holds the value of the original key `key`, if any: the
    /// key whose origin it is, else the key itself when the node holds it
    /// with a fresh value (computed over it); `None` when its value was
    /// dropped or overwritten by a value moved onto it.
    pub(crate) fn image_of<'t>(
        &'t self,
        key: &'t str,
        fields: &HashMap<String, Value>,
    ) -> Option<&'t str> {
        if let Some((held, _)) = self.origin.iter().find(|(_, o)| o.as_str() == key) {
            return Some(held.as_str());
        }
        (fields.contains_key(key) && !self.origin.contains_key(key)).then_some(key)
    }
}

/// [`apply_field_transforms`], recording in `track` what each transform
/// did to the node's keys.
pub(crate) fn apply_transforms_tracked(
    node: &mut Node,
    transforms: &[FieldTransform],
    ctx: &TransformContext<'_>,
    track: &mut KeyTrack,
) -> Result<(), RestrictError> {
    for transform in transforms {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => {
                if let Some(val) = node.extra_fields.remove(old_key) {
                    node.extra_fields.insert(new_key.clone(), val);
                    track.moved(old_key, new_key);
                    track.written.insert(new_key.clone());
                }
            }
            FieldTransform::DropField { key } => {
                node.extra_fields.remove(key);
                track.removed(key);
            }
            FieldTransform::AddField { key, value } => {
                if let std::collections::hash_map::Entry::Vacant(slot) =
                    node.extra_fields.entry(key.clone())
                {
                    slot.insert(value.clone());
                    track.computed(key);
                }
            }
            FieldTransform::KeepFields { keys } => {
                node.extra_fields.retain(|k, _| keys.contains(k));
                track.written.retain(|k| keys.contains(k));
                track.origin.retain(|k, _| keys.contains(k));
            }
            FieldTransform::ApplyExpr { key, expr, .. } => {
                if apply_expr_transform(node, key, expr, ctx)? {
                    track.changed(key);
                }
            }
            FieldTransform::ComputeField {
                target_key, expr, ..
            } => {
                let env = ctx.environment(expr, &node.extra_fields)?;
                let config = panproto_expr::EvalConfig::default();
                let result = ctx.eval(expr, &env, &config).map_err(|source| {
                    RestrictError::FieldTransformFailed {
                        key: target_key.clone(),
                        source,
                    }
                })?;
                node.extra_fields
                    .insert(target_key.clone(), expr_literal_to_value(&result));
                track.computed(target_key);
            }
            FieldTransform::PathTransform { path, inner } => {
                if path.is_empty() {
                    // Empty path = apply directly. PathTransform operates on nested
                    // extra_fields, not the instance tree, so it binds no children.
                    apply_transforms_tracked(
                        node,
                        std::slice::from_ref(inner),
                        &ctx.with_child_values(HashMap::new()),
                        track,
                    )?;
                } else if apply_path_transform(node, path, inner, ctx)? {
                    track.changed(&path[0]);
                }
            }
            FieldTransform::MapReferences { field, rename_map } => {
                if apply_map_references(node, field, rename_map) {
                    track.changed(field);
                }
            }
            FieldTransform::Case { branches } => {
                // Case predicates evaluate against the full fiber (extra_fields +
                // children) so that branching can depend on schema-defined
                // child values.
                let config = panproto_expr::EvalConfig::default();
                for (index, branch) in branches.iter().enumerate() {
                    let failed = |source| RestrictError::FieldTransformFailed {
                        key: format!("<case branch {index}>"),
                        source,
                    };
                    let env = ctx.environment(&branch.predicate, &node.extra_fields)?;
                    let result = ctx.eval(&branch.predicate, &env, &config).map_err(failed)?;
                    if matches!(result, panproto_expr::Literal::Bool(true)) {
                        apply_transforms_tracked(node, &branch.transforms, ctx, track)?;
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Apply an `ApplyExpr` transform: evaluate `expr` over the value bound at
/// `key` and store the result. Returns whether an `extra_fields` key was
/// written.
///
/// The key `"__value__"` targets the node's own leaf value rather than an
/// `extra_fields` entry, which is how a coercion (a kind change) is
/// applied; the expression sees it under both `v` and `__value__`.
///
/// Any other key is looked up in `extra_fields` first, so that a value
/// rewritten by an earlier transform in the same sequence is the one read,
/// and in the children second; a child the key names is refused as an
/// expression reading it would be (a repeated child, or a structural child
/// that is no tree). The result is written to `extra_fields` whichever side
/// it came from: `to_json` serializes `extra_fields` after children, so the
/// transform's output stays authoritative over the original child vertex
/// value.
fn apply_expr_transform(
    node: &mut Node,
    key: &str,
    expr: &panproto_expr::Expr,
    ctx: &TransformContext<'_>,
) -> Result<bool, RestrictError> {
    let config = panproto_expr::EvalConfig::default();
    let failed = |source| RestrictError::FieldTransformFailed {
        key: key.to_string(),
        source,
    };
    let unexpressible = |field: &str, e: NotExpressible| RestrictError::ValueNotExpressible {
        node: ctx.element(),
        field: field.to_owned(),
        value_type: e.value_type.to_owned(),
    };

    if key == "__value__" {
        if let Some(crate::value::FieldPresence::Present(val)) = &node.value {
            let input = value_to_expr_literal(val).map_err(|e| unexpressible(key, e))?;
            let env = panproto_expr::Env::new()
                .extend(std::sync::Arc::from("v"), input.clone())
                .extend(std::sync::Arc::from("__value__"), input);
            let result = ctx.eval(expr, &env, &config).map_err(failed)?;
            node.value = Some(crate::value::FieldPresence::Present(expr_literal_to_value(
                &result,
            )));
        }
        return Ok(false);
    }

    ctx.refuse_unreadable([key], false, &node.extra_fields)?;
    if let Some(val) = node
        .extra_fields
        .get(key)
        .or_else(|| ctx.child_values.get(key))
    {
        let input = value_to_expr_literal(val).map_err(|e| unexpressible(key, e))?;
        let env = panproto_expr::Env::new().extend(std::sync::Arc::from(key), input);
        let result = ctx.eval(expr, &env, &config).map_err(failed)?;
        node.extra_fields
            .insert(key.to_string(), expr_literal_to_value(&result));
        return Ok(true);
    }
    Ok(false)
}

/// Navigate into nested `Value::Unknown` maps along `path` and apply the
/// inner transform at the resolved location. Returns whether the path
/// resolved, so that the inner transform ran.
fn apply_path_transform(
    node: &mut Node,
    path: &[String],
    inner: &FieldTransform,
    ctx: &TransformContext<'_>,
) -> Result<bool, RestrictError> {
    let first = &path[0];
    let Some(Value::Unknown(map)) = node.extra_fields.get_mut(first) else {
        return Ok(false);
    };

    // Move the nested map into a temporary node so the transform can run
    // against it as an ordinary `extra_fields` map.
    let mut temp_node = Node::new(0, "");
    temp_node.extra_fields = std::mem::take(map);

    let outcome = if path.len() == 1 {
        // At the target; apply inner transform to this map. PathTransform
        // operates on nested extra_fields, not the instance tree, so it binds
        // no children.
        apply_field_transforms(
            &mut temp_node,
            std::slice::from_ref(inner),
            &ctx.with_child_values(HashMap::new()),
        )
        .map(|()| true)
    } else {
        apply_path_transform(&mut temp_node, &path[1..], inner, ctx)
    };

    // Restore the map before propagating: the node was left holding an
    // empty map by `mem::take`, so an early return on failure would
    // otherwise erase the nested fields it was asked to transform.
    if let Some(Value::Unknown(slot)) = node.extra_fields.get_mut(first) {
        *slot = temp_node.extra_fields;
    }
    outcome
}

/// Apply a `MapReferences` transform to a node's field, handling both
/// a scalar `Value::Str` reference and a `Value::List` of references.
/// Returns whether the field is present afterwards (a reference mapped to
/// `None` removes it).
///
/// This is the action of the rename map on string-typed leaves that
/// denote vertex names. The transform is functorial: it commutes with
/// the list constructor (renaming a list of references is the same as
/// mapping the rename over the list).
fn apply_map_references(
    node: &mut Node,
    field: &str,
    rename_map: &HashMap<String, Option<String>>,
) -> bool {
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
            Value::List(items) => {
                // Rebuild the list with renames applied and entries
                // mapped to None dropped. Non-string items pass through
                // unchanged: `MapReferences` is specifically the action
                // of the rename map on string leaves.
                let mut new_items = Vec::with_capacity(items.len());
                for item in items.iter() {
                    match item {
                        Value::Str(s) => match rename_map.get(s.as_str()) {
                            Some(Some(new_name)) => {
                                new_items.push(Value::Str(new_name.clone()));
                            }
                            Some(None) => {} // drop
                            None => new_items.push(Value::Str(s.clone())),
                        },
                        other => new_items.push(other.clone()),
                    }
                }
                *items = new_items;
            }
            _ => {}
        }
    }
    node.extra_fields.contains_key(field)
}

/// The instance an expression is evaluated in, with the element it is
/// evaluated at.
///
/// The graph-traversal builtins (`children("self")`, `edge("self", k)`,
/// `edge_count("self")`, `anchor("self")`) walk from the current element, so
/// they need both; [`TransformContext::eval`] resolves them through the
/// instance's [`ElementOps`](crate::ElementOps).
#[derive(Debug, Clone, Copy)]
pub enum InstanceRef<'a> {
    /// A tree and one of its nodes.
    Tree(&'a WInstance, u32),
    /// A graph and one of its nodes.
    Graph(&'a crate::ginstance::GInstance, u32),
    /// A table instance and one of its rows.
    Table(&'a crate::functor::FInstance, crate::view_ref::RowId),
}

impl InstanceRef<'_> {
    /// The id of the element: the node id, or the number inside the row id.
    #[must_use]
    pub const fn element(&self) -> u32 {
        match self {
            Self::Tree(_, id) | Self::Graph(_, id) => *id,
            Self::Table(_, row) => row.0,
        }
    }
}

/// Whether an edge label has one of the two reserved fan kinds, so that the
/// edge belongs to a reified fan rather than binding a child.
pub(crate) fn is_fan_edge(edge: &Edge) -> bool {
    edge.kind == crate::ginstance::HYPER_PARENT_KIND
        || edge.kind == crate::ginstance::HYPER_LABEL_KIND
}

/// The names the expressions at an element read, which bounds the children
/// worth materializing.
#[derive(Debug, Clone, Default)]
pub(crate) enum Demand {
    /// Every child: an expression reads `self`, the whole fiber.
    #[default]
    All,
    /// The free variables of the expressions.
    Names(BTreeSet<String>),
}

impl Demand {
    /// The demand of a set of names: everything when it holds `self`.
    pub(crate) fn from_names(names: BTreeSet<String>) -> Self {
        if names.contains("self") {
            Self::All
        } else {
            Self::Names(names)
        }
    }

    fn wants(&self, name: &str) -> bool {
        match self {
            Self::All => true,
            Self::Names(names) => names.contains(name),
        }
    }
}

/// The children of the elements of an instance, as every shape binds them.
///
/// An element's children are its out-edges other than fan edges and loops,
/// in child order (a tree's arc order, a graph's
/// [`ordered_out_edges`](crate::GInstance::ordered_out_edges), a table row's
/// pairs in ascending foreign key and then ascending target row), each
/// named by its column name: the edge's name, else its target vertex.
pub(crate) trait ChildSource {
    /// The element's own fields: a node's `extra_fields`, a row's cells.
    fn fields(&self, id: u32) -> Option<&HashMap<String, Value>>;
    /// The element's own value, when it has one.
    fn presence(&self, id: u32) -> Option<&crate::value::FieldPresence>;
    /// The element's children, in child order.
    fn children_of(&self, id: u32) -> Vec<Child<'_>>;
    /// Whether the element carries the parser's list annotation.
    fn is_list(&self, id: u32) -> bool;
    /// Whether the element carries a discriminator.
    fn has_discriminator(&self, id: u32) -> bool;
    /// The element's anchor.
    fn anchor(&self, id: u32) -> Option<&Name>;
}

/// One child of an element: the edge's label and the target element.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Child<'a> {
    pub(crate) label: &'a Edge,
    pub(crate) target: u32,
}

impl Child<'_> {
    /// The name the child binds under: the edge's name, else its target
    /// vertex.
    pub(crate) fn column(&self) -> &str {
        self.label.name.as_deref().unwrap_or(&*self.label.tgt)
    }
}

/// A tree with its arcs indexed by parent, so that each node's children
/// are found without a scan of every arc.
pub(crate) struct TreeChildren<'a> {
    tree: &'a WInstance,
    by_parent: HashMap<u32, Vec<usize>>,
}

impl<'a> TreeChildren<'a> {
    pub(crate) fn new(tree: &'a WInstance) -> Self {
        let mut by_parent: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, (parent, _, _)) in tree.arcs.iter().enumerate() {
            by_parent.entry(*parent).or_default().push(i);
        }
        Self { tree, by_parent }
    }

    /// The tree the index is of.
    pub(crate) const fn tree(&self) -> &'a WInstance {
        self.tree
    }

    /// The number of arcs whose parent is `id`, loops included.
    pub(crate) fn arc_count(&self, id: u32) -> usize {
        self.by_parent.get(&id).map_or(0, Vec::len)
    }
}

impl ChildSource for TreeChildren<'_> {
    fn fields(&self, id: u32) -> Option<&HashMap<String, Value>> {
        self.tree.nodes.get(&id).map(|n| &n.extra_fields)
    }
    fn presence(&self, id: u32) -> Option<&crate::value::FieldPresence> {
        self.tree.nodes.get(&id).and_then(|n| n.value.as_ref())
    }
    fn children_of(&self, id: u32) -> Vec<Child<'_>> {
        self.by_parent
            .get(&id)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|&i| &self.tree.arcs[i])
            .filter(|(parent, child, _)| parent != child)
            .map(|(_, child, label)| Child {
                label,
                target: *child,
            })
            .collect()
    }
    fn is_list(&self, id: u32) -> bool {
        self.tree.nodes.get(&id).is_some_and(Node::is_list)
    }
    fn has_discriminator(&self, id: u32) -> bool {
        self.tree
            .nodes
            .get(&id)
            .is_some_and(|n| n.discriminator.is_some())
    }
    fn anchor(&self, id: u32) -> Option<&Name> {
        self.tree.nodes.get(&id).map(|n| &n.anchor)
    }
}

impl ChildSource for crate::ginstance::GInstance {
    fn fields(&self, id: u32) -> Option<&HashMap<String, Value>> {
        self.node(id).map(|n| &n.extra_fields)
    }
    fn presence(&self, id: u32) -> Option<&crate::value::FieldPresence> {
        self.node(id).and_then(|n| n.value.as_ref())
    }
    fn children_of(&self, id: u32) -> Vec<Child<'_>> {
        self.ordered_out_edges(id)
            .into_iter()
            .filter_map(|e| self.edge(e))
            .filter(|e| e.src != e.tgt && !is_fan_edge(&e.edge))
            .map(|e| Child {
                label: &e.edge,
                target: e.tgt,
            })
            .collect()
    }
    fn is_list(&self, id: u32) -> bool {
        self.node(id).is_some_and(Node::is_list)
    }
    fn has_discriminator(&self, id: u32) -> bool {
        self.node(id).is_some_and(|n| n.discriminator.is_some())
    }
    fn anchor(&self, id: u32) -> Option<&Name> {
        self.node(id).map(|n| &n.anchor)
    }
}

impl ChildSource for crate::functor::FInstance {
    fn fields(&self, id: u32) -> Option<&HashMap<String, Value>> {
        self.row(crate::view_ref::RowId(id))
    }
    fn presence(&self, _id: u32) -> Option<&crate::value::FieldPresence> {
        None
    }
    fn children_of(&self, id: u32) -> Vec<Child<'_>> {
        let row = crate::view_ref::RowId(id);
        let Some(table) = self.table_of(row) else {
            return Vec::new();
        };
        self.foreign_keys()
            .into_iter()
            .filter(|(edge, _)| edge.src == *table)
            .flat_map(|(edge, _)| {
                self.targets_of(edge, row)
                    .filter(move |t| *t != row)
                    .map(move |t| Child {
                        label: edge,
                        target: t.0,
                    })
            })
            .collect()
    }
    fn is_list(&self, _id: u32) -> bool {
        false
    }
    fn has_discriminator(&self, _id: u32) -> bool {
        false
    }
    fn anchor(&self, id: u32) -> Option<&Name> {
        self.table_of(crate::view_ref::RowId(id))
    }
}

/// The value of an element's subtree, materialized as `node_to_value`
/// describes, or `None` when the subgraph reachable from it through
/// children is not an out-tree rooted at it.
pub(crate) fn materialize<S: ChildSource + ?Sized>(
    schema: Option<&Schema>,
    source: &S,
    root: u32,
) -> Option<Value> {
    if !is_out_tree(source, root) {
        return None;
    }
    Some(materialize_tree(schema, source, root))
}

/// Whether the subgraph reachable from `root` through children is an
/// out-tree rooted at `root`: `root` has no in-edge from inside it, and every
/// other element of it exactly one.
fn is_out_tree<S: ChildSource + ?Sized>(source: &S, root: u32) -> bool {
    let mut in_degree: HashMap<u32, usize> = HashMap::new();
    let mut seen: HashSet<u32> = HashSet::from([root]);
    let mut frontier = vec![root];
    while let Some(at) = frontier.pop() {
        for child in source.children_of(at) {
            let degree = in_degree.entry(child.target).or_insert(0);
            *degree += 1;
            if child.target == root || *degree > 1 {
                return false;
            }
            if seen.insert(child.target) {
                frontier.push(child.target);
            }
        }
    }
    true
}

fn materialize_tree<S: ChildSource + ?Sized>(
    schema: Option<&Schema>,
    source: &S,
    id: u32,
) -> Value {
    match source.presence(id) {
        Some(crate::value::FieldPresence::Present(val)) => return val.clone(),
        Some(crate::value::FieldPresence::Null | crate::value::FieldPresence::Absent) => {
            return Value::Null;
        }
        None => {}
    }
    let children = source.children_of(id);
    if is_collection(schema, source, id, &children) {
        return Value::List(
            children
                .iter()
                .map(|c| materialize_tree(schema, source, c.target))
                .collect(),
        );
    }
    let mut grouped: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for child in &children {
        grouped
            .entry(child.column())
            .or_default()
            .push(materialize_tree(schema, source, child.target));
    }
    let mut fields: HashMap<String, Value> = grouped
        .into_iter()
        .map(|(name, mut values)| {
            let value = if values.len() == 1 {
                values.remove(0)
            } else {
                Value::List(values)
            };
            (name.to_owned(), value)
        })
        .collect();
    for (key, val) in source.fields(id).into_iter().flatten() {
        fields.insert(key.clone(), val.clone());
    }
    Value::Unknown(fields)
}

/// Whether an element denotes an ordered collection rather than a record.
///
/// The same three signals `to_json` uses, and for the same reason: the
/// schema shape (every outgoing edge anonymous) is a heuristic that a
/// hand-built record can trip, so evidence about the *data* (the parser's
/// list annotation, or two or more children under one label, which no object
/// can have) is what decides, and object-only evidence on the element
/// (fields of its own, or a discriminator) vetoes the schema heuristic.
fn is_collection<S: ChildSource + ?Sized>(
    schema: Option<&Schema>,
    source: &S,
    id: u32,
    children: &[Child<'_>],
) -> bool {
    if source.is_list(id) {
        return true;
    }
    if let Some(first) = children.first()
        && children.len() >= 2
        && children
            .iter()
            .all(|c| c.label.kind == first.label.kind && c.label.name == first.label.name)
    {
        return true;
    }
    let (Some(schema), Some(anchor)) = (schema, source.anchor(id)) else {
        return false;
    };
    let outgoing = schema.outgoing_edges(anchor);
    let via_schema = !outgoing.is_empty() && outgoing.iter().all(|e| e.name.is_none());
    let object_only =
        source.fields(id).is_some_and(|f| !f.is_empty()) || source.has_discriminator(id);
    via_schema && !object_only
}

/// The children an element binds, by column name, under `demand`.
///
/// A child whose target holds a present value binds it. A child the
/// demand names whose target holds none binds its materialization; when
/// the target's subgraph is not a tree, the name is recorded in
/// `not_trees` instead. A name two or more children carry is recorded in
/// `repeated` and binds the list of the bindings of its children, in child
/// order, a child that binds nothing adding no element. A name the
/// element's own fields hold is shadowed: it binds nothing here, since the
/// field wins.
pub(crate) struct ChildBindings {
    pub(crate) values: HashMap<String, Value>,
    pub(crate) repeated: BTreeSet<String>,
    pub(crate) not_trees: BTreeSet<String>,
}

pub(crate) fn bind_children<S: ChildSource + ?Sized>(
    schema: Option<&Schema>,
    source: &S,
    id: u32,
    demand: &Demand,
) -> ChildBindings {
    let own = source.fields(id);
    let mut grouped: BTreeMap<&str, Vec<Child<'_>>> = BTreeMap::new();
    let children = source.children_of(id);
    for child in &children {
        if own.is_some_and(|f| f.contains_key(child.column())) {
            continue;
        }
        grouped.entry(child.column()).or_default().push(*child);
    }
    let mut bindings = ChildBindings {
        values: HashMap::new(),
        repeated: BTreeSet::new(),
        not_trees: BTreeSet::new(),
    };
    for (name, group) in grouped {
        let wanted = demand.wants(name);
        let mut values = Vec::with_capacity(group.len());
        for child in &group {
            match source.presence(child.target) {
                Some(crate::value::FieldPresence::Present(val)) => values.push(val.clone()),
                _ if wanted => match materialize(schema, source, child.target) {
                    Some(val) => values.push(val),
                    None => {
                        bindings.not_trees.insert(name.to_owned());
                    }
                },
                _ => {}
            }
        }
        if group.len() >= 2 {
            bindings.repeated.insert(name.to_owned());
            bindings.values.insert(name.to_owned(), Value::List(values));
        } else if let Some(value) = values.pop() {
            bindings.values.insert(name.to_owned(), value);
        }
    }
    bindings
}

/// What a field transform's expressions may read besides the node itself.
///
/// Two things live here. `child_values` binds the element's children by
/// column name ([`collect_child_values`] states the rule: present values,
/// materialized structural children an expression reads, and a list for a
/// name two or more children carry). `instance` carries the instance and
/// the element, which is what makes the graph-traversal builtins
/// resolvable: `children("self")`, `edge("self", k)`, `edge_count("self")`
/// and `anchor("self")` all need a current element to walk from.
///
/// Before an expression is evaluated, the context refuses it when its free
/// variables name a repeated child
/// ([`RestrictError::AmbiguousChildField`]), a structural child whose
/// subgraph is not a tree ([`RestrictError::StructuralChildNotATree`]), or a
/// value with no expression literal ([`RestrictError::ValueNotExpressible`]):
/// only the fields an expression names are converted.
///
/// A caller that holds a node but no instance (an edit lens rewriting a
/// node in isolation, say) builds this with [`TransformContext::detached`]
/// and gets the `extra_fields`-only behaviour.
#[derive(Debug, Clone)]
pub struct TransformContext<'a> {
    /// The element's children, keyed by column name.
    pub child_values: HashMap<String, Value>,
    /// The instance and the element being transformed.
    pub instance: Option<InstanceRef<'a>>,
    /// The child names two or more children carry.
    repeated: BTreeSet<String>,
    /// The child names whose structural target is no tree.
    not_trees: BTreeSet<String>,
}

impl Default for TransformContext<'_> {
    fn default() -> Self {
        Self::detached()
    }
}

impl<'a> TransformContext<'a> {
    /// A context with neither children nor an instance: expressions see
    /// only the node's own `extra_fields`.
    #[must_use]
    pub fn detached() -> Self {
        Self {
            child_values: HashMap::new(),
            instance: None,
            repeated: BTreeSet::new(),
            not_trees: BTreeSet::new(),
        }
    }

    /// A context over the tree `instance` at `node_id`, binding every child
    /// the expressions in `transforms` might read.
    #[must_use]
    pub fn new(
        schema: Option<&Schema>,
        instance: &'a WInstance,
        node_id: u32,
        transforms: &[FieldTransform],
    ) -> Self {
        Self::bound(
            schema,
            &TreeChildren::new(instance),
            InstanceRef::Tree(instance, node_id),
            &transform_demand(transforms),
        )
    }

    /// A context over the graph `instance` at `node`, binding every child
    /// the expressions in `transforms` might read.
    #[must_use]
    pub fn for_graph(
        schema: Option<&Schema>,
        instance: &'a crate::ginstance::GInstance,
        node: u32,
        transforms: &[FieldTransform],
    ) -> Self {
        Self::bound(
            schema,
            instance,
            InstanceRef::Graph(instance, node),
            &transform_demand(transforms),
        )
    }

    /// A context over the table instance `instance` at `row`, binding every
    /// child (a row its pairs reach) the expressions in `transforms` might
    /// read.
    #[must_use]
    pub fn for_table(
        schema: Option<&Schema>,
        instance: &'a crate::functor::FInstance,
        row: crate::view_ref::RowId,
        transforms: &[FieldTransform],
    ) -> Self {
        Self::bound(
            schema,
            instance,
            InstanceRef::Table(instance, row),
            &transform_demand(transforms),
        )
    }

    /// A context at `at` with the children of `source` bound under `demand`.
    pub(crate) fn bound<S: ChildSource + ?Sized>(
        schema: Option<&Schema>,
        source: &S,
        at: InstanceRef<'a>,
        demand: &Demand,
    ) -> Self {
        let bindings = bind_children(schema, source, at.element(), demand);
        Self {
            child_values: bindings.values,
            instance: Some(at),
            repeated: bindings.repeated,
            not_trees: bindings.not_trees,
        }
    }

    /// A context carrying pre-collected child values and no instance.
    #[must_use]
    pub fn from_child_values(child_values: HashMap<String, Value>) -> Self {
        Self {
            child_values,
            ..Self::detached()
        }
    }

    /// The same context with the child bindings replaced.
    #[must_use]
    pub const fn with_child_values(&self, child_values: HashMap<String, Value>) -> Self {
        Self {
            child_values,
            instance: self.instance,
            repeated: BTreeSet::new(),
            not_trees: BTreeSet::new(),
        }
    }

    /// The element the context is at, or 0 when it is detached.
    #[must_use]
    pub fn element(&self) -> u32 {
        self.instance.as_ref().map_or(0, InstanceRef::element)
    }

    /// Evaluate `expr`, resolving graph-traversal builtins against the
    /// instance when one is present.
    ///
    /// # Errors
    ///
    /// Returns whatever the evaluator reports.
    pub fn eval(
        &self,
        expr: &panproto_expr::Expr,
        env: &panproto_expr::Env,
        config: &panproto_expr::EvalConfig,
    ) -> Result<panproto_expr::Literal, panproto_expr::ExprError> {
        use crate::instance_env::eval_with_element_ops;
        match self.instance {
            Some(InstanceRef::Tree(instance, node)) => {
                eval_with_element_ops(expr, env, config, instance, Some(node))
            }
            Some(InstanceRef::Graph(instance, node)) => {
                eval_with_element_ops(expr, env, config, instance, Some(node))
            }
            Some(InstanceRef::Table(instance, row)) => {
                eval_with_element_ops(expr, env, config, instance, Some(row.0))
            }
            None => panproto_expr::eval(expr, env, config),
        }
    }

    /// The environment `expr` is evaluated in over `fields` (the element's
    /// fields as the transforms have left them) and the bound children,
    /// with only the names `expr` reads converted.
    ///
    /// # Errors
    ///
    /// In this order: [`RestrictError::AmbiguousChildField`] for the least
    /// name the expression reads that two or more children carry and no
    /// field shadows; [`RestrictError::StructuralChildNotATree`] for the
    /// least name it reads (every name, when it reads `self`) whose
    /// structural child is no tree; [`RestrictError::ValueNotExpressible`]
    /// for the least name it reads whose value has no expression literal.
    pub(crate) fn environment(
        &self,
        expr: &panproto_expr::Expr,
        fields: &HashMap<String, Value>,
    ) -> Result<panproto_expr::Env, RestrictError> {
        let free: BTreeSet<String> = panproto_expr::free_vars(expr)
            .into_iter()
            .map(|v| v.to_string())
            .collect();
        self.refuse_unreadable(
            free.iter().map(String::as_str),
            free.contains("self"),
            fields,
        )?;
        demand_env(self.element(), &free, fields, &self.child_values)
    }

    /// Refuse a read of `names` (and of every child, when `whole` says the
    /// expression reads `self`) that names a repeated child or a structural
    /// child that is no tree.
    pub(crate) fn refuse_unreadable<'n>(
        &self,
        names: impl IntoIterator<Item = &'n str> + Clone,
        whole: bool,
        fields: &HashMap<String, Value>,
    ) -> Result<(), RestrictError> {
        let node = self.element();
        let mut read: BTreeSet<&str> = names.into_iter().collect();
        if let Some(name) = read
            .iter()
            .find(|n| self.repeated.contains(**n) && !fields.contains_key(**n))
        {
            return Err(RestrictError::AmbiguousChildField {
                node,
                field: (*name).to_owned(),
            });
        }
        if whole {
            read.extend(self.not_trees.iter().map(String::as_str));
        }
        if let Some(name) = read
            .iter()
            .find(|n| self.not_trees.contains(**n) && !fields.contains_key(**n))
        {
            return Err(RestrictError::StructuralChildNotATree {
                node,
                field: (*name).to_owned(),
            });
        }
        Ok(())
    }
}

/// The demand of the expressions in `transforms`.
fn transform_demand(transforms: &[FieldTransform]) -> Demand {
    Demand::from_names(referenced_names(transforms).into_iter().collect())
}

/// The environment of an expression reading `free` over `fields` and
/// `children`: the bindings of [`build_env_with_children`], of which only
/// those `free` names are converted (all of `self`, when it is named).
fn demand_env(
    node: u32,
    free: &BTreeSet<String>,
    fields: &HashMap<String, Value>,
    children: &HashMap<String, Value>,
) -> Result<panproto_expr::Env, RestrictError> {
    let mut combined = children.clone();
    for (key, val) in fields {
        combined.insert(key.clone(), val.clone());
    }
    let convert = |field: &str, val: &Value| {
        value_to_expr_literal(val).map_err(|e| RestrictError::ValueNotExpressible {
            node,
            field: field.to_owned(),
            value_type: e.value_type.to_owned(),
        })
    };
    let mut bindings: BTreeMap<String, (String, &Value)> = BTreeMap::new();
    for (key, val) in &combined {
        bindings.insert(key.clone(), (key.clone(), val));
        if key != "attrs" && key != "name" && key != "$type" && key != "parents" {
            bindings.insert(format!("attrs.{key}"), (key.clone(), val));
        }
    }
    if let Some(Value::Unknown(attrs)) = combined.get("attrs") {
        for (key, val) in attrs {
            let qualified = format!("attrs.{key}");
            bindings.insert(qualified.clone(), (qualified.clone(), val));
            if !combined.contains_key(key) {
                bindings.insert(key.clone(), (qualified, val));
            }
        }
    }
    let mut env = panproto_expr::Env::new();
    if free.contains("self") {
        let mut entries: Vec<(&String, &Value)> = combined.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut record = Vec::with_capacity(entries.len());
        for (key, val) in entries {
            record.push((std::sync::Arc::from(key.as_str()), convert(key, val)?));
        }
        env = env.extend(
            std::sync::Arc::from("self"),
            panproto_expr::Literal::Record(record),
        );
    }
    for name in free {
        if let Some((field, val)) = bindings.get(name) {
            env = env.extend(std::sync::Arc::from(name.as_str()), convert(field, val)?);
        }
    }
    Ok(env)
}

/// Every top-level name the expressions in `transforms` read, the key an
/// `ApplyExpr` rewrites included.
///
/// Used to bound how much of the instance a transform context
/// materializes: a structural child is only worth walking when something
/// is about to name it.
#[must_use]
pub fn referenced_names(transforms: &[FieldTransform]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    collect_referenced_names(transforms, &mut names);
    names
}

fn collect_referenced_names(
    transforms: &[FieldTransform],
    names: &mut std::collections::HashSet<String>,
) {
    let add = |expr: &panproto_expr::Expr, names: &mut std::collections::HashSet<String>| {
        for var in panproto_expr::free_vars(expr) {
            names.insert(var.to_string());
        }
    };
    for transform in transforms {
        match transform {
            FieldTransform::ApplyExpr {
                key, expr, inverse, ..
            } => {
                if key != "__value__" {
                    names.insert(key.clone());
                }
                add(expr, names);
                if let Some(inv) = inverse {
                    add(inv, names);
                }
            }
            FieldTransform::ComputeField { expr, inverse, .. } => {
                add(expr, names);
                if let Some(inv) = inverse {
                    add(inv, names);
                }
            }
            FieldTransform::Case { branches } => {
                for branch in branches {
                    add(&branch.predicate, names);
                    collect_referenced_names(&branch.transforms, names);
                }
            }
            FieldTransform::PathTransform { inner, .. } => {
                collect_referenced_names(std::slice::from_ref(inner), names);
            }
            _ => {}
        }
    }
}

/// Collect the present values of a node's immediate children, keyed by
/// column name (the edge's name, else its target vertex).
///
/// This is the dependent-sum projection from the total fiber over vertex
/// `v` in the Grothendieck fibration. In the W-type model, a node at `v`
/// with children via edges `e_i: v -> w_i` has total fiber
///
/// ```text
/// Fiber(v) = ExtraFields(v) x Product_{i} Fiber(w_i)
/// ```
///
/// This function projects the leaf (scalar) components of the product
/// into a flat map, making them available to fiber endomorphisms (field
/// transforms). Only children with a present leaf value are bound;
/// structural children (objects, arrays) are omitted. A name two or more
/// arcs carry binds the [`Value::List`] of their present values in arc
/// order, never the last of them, and a name the node's `extra_fields`
/// hold binds nothing here, since the field wins.
///
/// For the structural components too, an array-of-objects child bound as
/// a [`Value::List`] of [`Value::Unknown`] being what a parent-level
/// aggregate needs, use [`collect_child_values`].
#[must_use]
pub fn collect_scalar_child_values(instance: &WInstance, node_id: u32) -> HashMap<String, Value> {
    bind_children(
        None,
        &TreeChildren::new(instance),
        node_id,
        &Demand::Names(BTreeSet::new()),
    )
    .values
}

/// Collect a node's immediate children, keyed by column name, including
/// the structural ones.
///
/// [`collect_scalar_child_values`] projects only the leaf components of
/// the fiber, so a child that is an object or an array of objects binds to
/// nothing and a parent-level aggregate over it (the minimum and maximum
/// of a field across an array of child records, say) cannot be written at
/// all. This function materializes those children as well, as
/// [`node_to_value`] does: an ordered collection becomes a [`Value::List`]
/// and a record becomes a [`Value::Unknown`], recursively.
/// [`value_to_expr_literal`] carries both through structurally, so `map`
/// and `fold` and field projection reach them directly.
///
/// Materializing a subtree is not free, so `wanted` bounds the work: a
/// structural child is materialized only when its name appears in that
/// set, which callers derive from the free variables of the expressions
/// about to run. `None` materializes every child, which is what a
/// reference to the whole fiber (`self`) needs. Present children are always
/// included, so an empty `wanted` gives exactly
/// [`collect_scalar_child_values`]. A name two or more arcs carry binds
/// the list of the bindings of its arcs, in arc order.
#[must_use]
pub fn collect_child_values(
    schema: Option<&Schema>,
    instance: &WInstance,
    node_id: u32,
    wanted: Option<&std::collections::HashSet<String>>,
) -> HashMap<String, Value> {
    let demand = wanted.map_or(Demand::All, |w| Demand::Names(w.iter().cloned().collect()));
    bind_children(schema, &TreeChildren::new(instance), node_id, &demand).values
}

/// Materialize a node's subtree as a [`Value`].
///
/// Mirrors the shape decisions `to_json` makes, so what an expression sees
/// under a structural child is what serialization would emit for it: a
/// present leaf is its value, a null or absent one is [`Value::Null`], an
/// ordered collection is a [`Value::List`] of its children in arc order,
/// and anything else is a [`Value::Unknown`] of its children (a name two or
/// more arcs carry binding the list of them, in arc order) and
/// `extra_fields`, with `extra_fields` taking precedence on a key collision
/// exactly as in `to_json`.
#[must_use]
pub fn node_to_value(schema: Option<&Schema>, instance: &WInstance, node_id: u32) -> Value {
    if !instance.nodes.contains_key(&node_id) {
        return Value::Null;
    }
    materialize_tree(schema, &TreeChildren::new(instance), node_id)
}

/// Build an expression evaluation environment from the full fiber over a
/// vertex: both `extra_fields` and scalar child values.
///
/// The binding order is `child_scalars` first, then `extra_fields`. This
/// ensures that `extra_fields` take precedence on key collision, which
/// is correct because `extra_fields` may contain values modified by
/// earlier transforms in the same sequence, and the transform pipeline
/// must see the most recent values.
///
/// Categorically, this constructs the left-biased coproduct injection
/// `ExtraFields + ChildScalars → Env` where `ExtraFields` has priority:
/// both maps contribute bindings, but on key collision the `ExtraFields`
/// value wins. This models the fiber projection
/// `π : ExtraFields(v) × Π_e Fiber(target(e)) → Env` where
/// `ExtraFields` carries transform-local state and `ChildScalars`
/// carries the dependent-sum projection of the structural children.
///
/// The whole fiber is additionally bound as `self`, so an expression can
/// reach a field it also names directly, `self.timeMs` and `timeMs` being
/// the same binding, and so that a name colliding with a builtin or a
/// reserved word stays reachable. A field genuinely called `self` wins
/// over the handle, since the flat bindings are applied last.
///
/// Every value is converted, and a value with no expression literal
/// ([`value_to_expr_literal`]) is left unbound rather than given an
/// invented one, so an expression reading it fails as reading an unbound
/// name. The restrict engines convert on demand instead, and refuse such a
/// read before evaluating ([`RestrictError::ValueNotExpressible`]).
#[must_use]
pub fn build_env_with_children(
    fields: &HashMap<String, Value>,
    child_scalars: &HashMap<String, Value>,
) -> panproto_expr::Env {
    // Start with child scalars, then overlay extra_fields so that
    // extra_fields take precedence.
    let mut combined = child_scalars.clone();
    for (key, val) in fields {
        combined.insert(key.clone(), val.clone());
    }
    let this = Value::Unknown(combined.clone());
    let mut env = panproto_expr::Env::new();
    if let Ok(lit) = value_to_expr_literal(&this) {
        env = env.extend(std::sync::Arc::from("self"), lit);
    }
    extend_env_from_extra_fields(env, &combined)
}

/// Build an evaluation environment from a node's `extra_fields`.
///
/// Each field is bound as a top-level variable. If an `attrs` field
/// contains a `Value::Unknown` map, its entries are also bound with
/// qualified names (e.g., `attrs.level`).
///
/// Those qualified names are a compatibility surface, not the primary
/// access path. They exist because `Value::Unknown` used to convert to
/// `Literal::Null`, so a genuine field projection — `Expr::Field` over
/// `attrs` — failed with "expected record, got null", and reaching a
/// nested entry required a flattened variable whose name happened to
/// contain a dot. [`value_to_expr_literal`] is now structure-preserving,
/// so `attrs` binds to a `Literal::Record` and field projection resolves
/// natively; the qualified bindings are redundant with it rather than
/// load-bearing. They are kept so that an expression already written
/// against the flattened names keeps working, and because the flat
/// aliasing also serves records that are *not* nested under `attrs`.
#[must_use]
pub fn build_env_from_extra_fields(fields: &HashMap<String, Value>) -> panproto_expr::Env {
    extend_env_from_extra_fields(panproto_expr::Env::new(), fields)
}

/// [`build_env_from_extra_fields`] onto an existing environment, so a
/// caller can seed bindings that the field bindings then take precedence
/// over.
#[must_use]
pub fn extend_env_from_extra_fields(
    base: panproto_expr::Env,
    fields: &HashMap<String, Value>,
) -> panproto_expr::Env {
    let mut env = base;
    for (key, val) in fields {
        let Ok(lit) = value_to_expr_literal(val) else {
            continue;
        };
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
            let Ok(lit) = value_to_expr_literal(val) else {
                continue;
            };
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

/// A value with no expression literal: a content link, a blob, a token, an
/// opaque value or a labelled null, at any depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a {value_type} has no expression literal")]
pub struct NotExpressible {
    /// The type name of the value ([`Value::type_name`]).
    pub value_type: &'static str,
}

/// Convert an instance `Value` to a `panproto_expr::Literal` for expression
/// evaluation.
///
/// The conversion is structure-preserving on the two container
/// variants: `Value::List` maps to `Literal::List` and `Value::Unknown`
/// maps to `Literal::Record`, in both cases converting the contents
/// recursively. This makes list- and record-valued fields reachable by
/// `map` / `fold` / `head` and by field projection, rather than
/// collapsing them to a scalar before the expression ever runs. The exact
/// variants map to their literals: `Value::Decimal`, `Value::BigInt` and
/// `Value::Temporal` to `Literal::Decimal`, `Literal::BigInt` and
/// `Literal::Temporal`.
///
/// Record fields are emitted in sorted key order. `Value::Unknown` is
/// backed by a `HashMap`, whose iteration order varies between runs, so
/// sorting is what makes the conversion a function: the same map always
/// yields the same `Literal::Record`, and equality and hashing over the
/// result are stable.
///
/// ```
/// use panproto_inst::{Value, value_to_expr_literal};
/// use panproto_expr::Literal;
///
/// assert_eq!(value_to_expr_literal(&Value::Int(3)), Ok(Literal::Int(3)));
/// let token = value_to_expr_literal(&Value::Token("red".into())).unwrap_err();
/// assert_eq!(token.value_type, "token");
/// ```
///
/// # Errors
///
/// [`NotExpressible`] for a `Value::CidLink`, `Value::Blob`,
/// `Value::Token`, `Value::Opaque` or `Value::LabeledNull`, which have no
/// faithful `Literal` counterpart, naming the first one found (a container
/// is searched in order). No value is invented in its place.
pub fn value_to_expr_literal(val: &Value) -> Result<panproto_expr::Literal, NotExpressible> {
    use panproto_expr::Literal;
    Ok(match val {
        Value::Bool(b) => Literal::Bool(*b),
        Value::Int(i) => Literal::Int(*i),
        Value::Float(f) => Literal::Float(*f),
        Value::Str(s) => Literal::Str(s.clone()),
        Value::Bytes(b) => Literal::Bytes(b.clone()),
        Value::Null => Literal::Null,
        Value::Decimal(d) => Literal::Decimal(d.clone()),
        Value::BigInt(b) => Literal::BigInt(b.clone()),
        Value::Temporal(t) => Literal::Temporal(t.clone()),
        Value::List(items) => Literal::List(
            items
                .iter()
                .map(value_to_expr_literal)
                .collect::<Result<_, _>>()?,
        ),
        Value::Unknown(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut fields = Vec::with_capacity(entries.len());
            for (k, v) in entries {
                fields.push((std::sync::Arc::from(k.as_str()), value_to_expr_literal(v)?));
            }
            Literal::Record(fields)
        }
        Value::CidLink(_)
        | Value::Blob { .. }
        | Value::Token(_)
        | Value::Opaque { .. }
        | Value::LabeledNull(_) => {
            return Err(NotExpressible {
                value_type: val.type_name(),
            });
        }
    })
}

/// Convert a `panproto_expr::Literal` back to an instance `Value`.
///
/// The inverse direction of [`value_to_expr_literal`], and
/// structure-preserving on the same containers: `Literal::List` maps to
/// `Value::List` and `Literal::Record` to `Value::Unknown`, recursively.
/// An expression that *returns* a list of records — the shape produced
/// by `map (\x -> { .. }) xs` — is therefore written back as structured
/// data rather than collapsing to a null.
///
/// A float stays a float, integral or not: the column a value is written
/// to decides its kind ([`coerce_value`](crate::value::coerce::coerce_value)),
/// not the evaluator.
///
/// `Literal::Closure` has no instance counterpart and converts to
/// `Value::Null`: a closure is an intermediate of evaluation, never a
/// value that should be persisted into a record.
///
/// ```
/// use panproto_inst::{Value, expr_literal_to_value};
/// use panproto_expr::Literal;
///
/// assert_eq!(expr_literal_to_value(&Literal::Float(2.0)), Value::Float(2.0));
/// ```
#[must_use]
pub fn expr_literal_to_value(lit: &panproto_expr::Literal) -> Value {
    match lit {
        panproto_expr::Literal::Bool(b) => Value::Bool(*b),
        panproto_expr::Literal::Int(i) => Value::Int(*i),
        panproto_expr::Literal::Float(f) => Value::Float(*f),
        panproto_expr::Literal::Str(s) => Value::Str(s.clone()),
        panproto_expr::Literal::Bytes(b) => Value::Bytes(b.clone()),
        panproto_expr::Literal::List(items) => {
            Value::List(items.iter().map(expr_literal_to_value).collect())
        }
        panproto_expr::Literal::Record(fields) => Value::Unknown(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), expr_literal_to_value(v)))
                .collect(),
        ),
        panproto_expr::Literal::Null | panproto_expr::Literal::Closure { .. } => Value::Null,
        panproto_expr::Literal::Decimal(d) => Value::Decimal(d.clone()),
        panproto_expr::Literal::BigInt(b) => Value::BigInt(b.clone()),
        panproto_expr::Literal::Temporal(t) => Value::Temporal(t.clone()),
    }
}

// ---------------------------------------------------------------------------
// Left Kan extension (Σ_F) for W-type instances
// ---------------------------------------------------------------------------

/// Left Kan extension (`Sigma_F`) for W-type instances.
///
/// Pushes a W-type instance forward along a migration morphism, mapping
/// every source node into the target schema and remapping anchors and edges
/// according to the compiled migration.
///
/// This is a *total* operation. Left Kan extension along a total functor
/// never deletes: every source node's anchor must be either remapped
/// (present as a key in `vertex_remap`) or surviving (present in
/// `surviving_verts`). A node whose anchor satisfies neither has no image in
/// the target schema, and rather than drop it silently `wtype_extend`
/// returns [`RestrictError::UnmappedAnchor`]. Callers that genuinely want
/// partial extension — keeping the mappable nodes and learning which nodes
/// were dropped — should use [`wtype_extend_partial`] instead.
///
/// The output is validated against `tgt_schema` under `tgt_protocol`; the
/// input's schema is not a parameter, so its callers validate the input at
/// their own boundary.
///
/// # Errors
///
/// An arc whose edge the migration named no image for keeps its own kind and
/// name, and is dropped when the target carries no such edge between the
/// remapped anchors. It is never given a different label: resolving by anchors
/// alone would deliver the value under a sibling field's name.
///
/// Returns [`RestrictError::UnmappedAnchor`] if any source node's anchor is
/// neither remapped nor surviving, [`RestrictError::NonInjectiveVertexMap`] if
/// two source anchors share a target anchor, since a W-instance cannot
/// represent the merge that asks for, [`RestrictError::RootPruned`] if the root
/// itself cannot be mapped, another [`RestrictError`] variant if edge
/// resolution fails, or [`RestrictError::Validation`] on the output side
/// when the output does not validate.
pub fn wtype_extend(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    tgt_protocol: &panproto_schema::Protocol,
) -> Result<WInstance, RestrictError> {
    let tgt_plan = crate::restrict_boundary::boundary_plan(tgt_schema, tgt_protocol);
    wtype_extend_validated(instance, tgt_schema, migration, &tgt_plan)
}

/// The extend of a W-type instance with no validation: the engine of
/// [`wtype_extend`].
///
/// It serves operations that are no boundary (the W-type `Sigma_F` of the
/// adjunctions) and callers that hold no target protocol.
///
/// # Errors
///
/// As [`wtype_extend`], without the validation failure.
pub fn wtype_extend_engine(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<WInstance, RestrictError> {
    wtype_extend_inner(instance, tgt_schema, migration, false).map(|(extended, _)| extended)
}

/// [`wtype_extend`] with the target's validation plan compiled already.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] on the output side, before
/// anything runs, when `tgt_plan` was compiled for another schema than
/// `tgt_schema`; otherwise as [`wtype_extend`].
pub fn wtype_extend_validated(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    tgt_plan: &crate::validation::ValidationPlan,
) -> Result<WInstance, RestrictError> {
    use crate::restrict_boundary::{BoundarySide, require_passed, require_schema, unlimited};
    require_schema(
        tgt_plan,
        &crate::identity::schema_digest(tgt_schema),
        BoundarySide::Output,
    )?;
    let extended = wtype_extend_engine(instance, tgt_schema, migration)?;
    let (budget, external) = unlimited();
    require_passed(
        tgt_plan.validate_tree(&extended, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(extended)
}

/// Partial left Kan extension: extend what can be mapped, report the rest.
///
/// Like [`wtype_extend`], but instead of failing on a node whose anchor is
/// neither remapped nor surviving, this variant drops that node and records
/// its id. The returned `Vec<u32>` lists the dropped source node ids (empty
/// when the migration is total, in which case the result equals that of
/// [`wtype_extend`]). Arcs touching a dropped node are dropped as well.
///
/// Use this variant when partial extension is intended, and inspect the
/// returned ids so the dropped nodes are surfaced rather than lost silently.
///
/// # Errors
///
/// Returns [`RestrictError::RootPruned`] if the root cannot be mapped, or
/// another [`RestrictError`] variant if edge resolution fails. Partiality is
/// about which *nodes* travel, so [`RestrictError::NonInjectiveVertexMap`] is
/// refused here too: the instance it describes is malformed rather than
/// incomplete.
pub fn wtype_extend_partial(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
) -> Result<(WInstance, Vec<u32>), RestrictError> {
    wtype_extend_inner(instance, tgt_schema, migration, true)
}

/// The source anchors landing on each target anchor.
///
/// A vertex that survives without being remapped is its own fiber, so a target
/// anchor reached both by a rename and by a survival is a contraction like any
/// other.
fn fiber_map(migration: &CompiledMigration) -> HashMap<Name, Vec<Name>> {
    let mut fibers: HashMap<Name, Vec<Name>> = HashMap::new();
    let remapped: FxHashSet<&Name> = migration.vertex_remap.values().collect();

    for (source, target) in &migration.vertex_remap {
        fibers
            .entry(target.clone())
            .or_default()
            .push(source.clone());
    }
    for vertex in &migration.surviving_verts {
        if !migration.vertex_remap.contains_key(vertex) && !remapped.contains(vertex) {
            fibers
                .entry(vertex.clone())
                .or_default()
                .push(vertex.clone());
        }
    }
    fibers
}

/// The image of an arc whose edge the migration named no image for.
///
/// The resolver wins first: an entry there is a caller saying explicitly which
/// target edge joins the two anchors. Failing that, the arc keeps its own kind
/// and name, which the target must actually carry between the remapped
/// anchors. `None` means the migration does not carry this arc, and the arc is
/// dropped rather than given a sibling's label.
///
/// There is no unique-edge fallback, and its absence is the point.
/// [`resolve_edge`] has one, which is right where a *contracted* arc has to
/// land somewhere and wrong here, where the question is what an unmapped arc
/// should be called: a source with parallel `a` and `b` arcs migrating onto a
/// target that kept only `a` would send `b`'s value under `a`'s name.
fn carry_edge(
    tgt_schema: &Schema,
    resolver: &HashMap<(Name, Name), Edge>,
    parent_anchor: &Name,
    child_anchor: &Name,
    edge: &Edge,
) -> Option<Edge> {
    for ((key_src, key_tgt), resolved) in resolver {
        if key_src == parent_anchor && key_tgt == child_anchor {
            return Some(resolved.clone());
        }
    }
    tgt_schema
        .edges_between(parent_anchor.as_str(), child_anchor.as_str())
        .iter()
        .find(|candidate| candidate.kind == edge.kind && candidate.name == edge.name)
        .cloned()
}

/// Shared implementation for [`wtype_extend`] and [`wtype_extend_partial`].
///
/// When `partial` is `false`, an unmapped-anchor node yields
/// [`RestrictError::UnmappedAnchor`]. When `partial` is `true`, such a node
/// is dropped and its id is collected into the returned vector.
fn wtype_extend_inner(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    partial: bool,
) -> Result<(WInstance, Vec<u32>), RestrictError> {
    // A contracting vertex map has no W-type image. Two source anchors sent to
    // one target anchor ask for the subtrees under them to be merged, and a
    // W-instance has no way to say that: the node it produces carries both sets
    // of children side by side, so a record whose `title` and `subtitle` both
    // migrate onto `heading` comes out with two arcs labelled `heading` out of
    // one node. That is not an instance of the target schema, and no later pass
    // rejects it, so it is refused here — the same guard `wtype_pi` applies, for
    // the same reason.
    //
    // `Migration::resolver` does not authorise the merge. It is an edge
    // disambiguation table, `(src anchor, tgt anchor) -> Edge`, not a rule for
    // combining two values into one, and nothing in the lift path reads
    // `Schema::mergers`.
    for (target, sources) in fiber_map(migration) {
        if sources.len() > 1 {
            let mut sources = sources;
            sources.sort_unstable();
            return Err(RestrictError::NonInjectiveVertexMap { target, sources });
        }
    }

    // Check root can be mapped
    let mut dropped_nodes: Vec<u32> = Vec::new();
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
    let children = TreeChildren::new(instance);
    let mut new_nodes: HashMap<u32, Node> = HashMap::with_capacity(instance.nodes.len());
    for (&id, node) in &instance.nodes {
        let mut new_node = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            new_node.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            // Node's anchor has no image in the target schema. A total left
            // Kan extension cannot map it, so report the loss; only the
            // partial variant records the id and continues.
            if partial {
                dropped_nodes.push(id);
                continue;
            }
            return Err(RestrictError::UnmappedAnchor {
                anchor: node.anchor.clone(),
                node_id: id,
            });
        }
        // Apply field transforms (coercions) to the extended node.
        // Collect scalar child values from the original instance for the
        // full fiber projection.
        let transforms = migration.value_transforms(&node.anchor);
        if !transforms.is_empty() {
            let ctx = TransformContext::bound(
                None,
                &children,
                InstanceRef::Tree(instance, id),
                &transform_demand(&transforms),
            );
            apply_field_transforms(&mut new_node, &transforms, &ctx)?;
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
        } else {
            // The migration named no image for this edge, so the arc carries
            // its own label across: the target edge between the remapped
            // anchors that agrees with it on kind *and* name.
            //
            // Resolving by anchors alone is how a wrong label gets invented.
            // `resolve_edge` returns the unique target edge between two anchors
            // whenever there is exactly one, so an arc whose edge was dropped
            // comes back wearing a *sibling's* name and delivers its value
            // under a field it was never sent to. A dropped field is a loss; a
            // relabelled one is a lie, and nothing downstream can tell.
            //
            // The resolver still wins where a caller supplied one: that is an
            // explicit instruction rather than a guess.
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            if migration.surviving_edges.contains(edge)
                && edge.src == *parent_anchor
                && edge.tgt == *child_anchor
            {
                // The edge survives and neither anchor moved, so it is already
                // a target edge between the right two vertices.
                new_arcs.push((parent, child, edge.clone()));
            } else if let Some(carried) = carry_edge(
                tgt_schema,
                &migration.resolver,
                parent_anchor,
                child_anchor,
                edge,
            ) {
                new_arcs.push((parent, child, carried));
            }
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

    Ok((
        WInstance::new(
            new_nodes,
            new_arcs,
            new_fans,
            instance.root,
            new_schema_root,
        ),
        dropped_nodes,
    ))
}

#[cfg(test)]
mod tests {
    /// The tree engine over the plan of `(src, tgt, migration)`.
    fn restrict_engine(
        instance: &WInstance,
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
    ) -> Result<WInstance, RestrictError> {
        let plan = crate::restrict_plan::RestrictPlan::compile(src, tgt, migration)?;
        wtype_restrict_with_plan(instance, &plan).map(|(view, _)| view)
    }

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
            between,
            ..Default::default()
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
            ..Default::default()
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
            edges: edges.iter().map(|e| (e.clone(), e.kind.clone())).collect(),
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
            between,
            ..Default::default()
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
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };
        let result = wtype_extend_engine(&inst, &schema, &migration).unwrap();
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
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };
        let result = wtype_extend_engine(&inst, &tgt_schema, &migration).unwrap();
        assert_eq!(result.node_count(), 3);
        assert_eq!(result.arc_count(), 2);
        assert_eq!(result.schema_root, Name::from("article:body"));
        assert_eq!(result.nodes[&0].anchor, Name::from("article:body"));
        assert_eq!(result.nodes[&1].anchor, Name::from("article:body.text"));
    }

    /// A migration that sends two source anchors to one target anchor asks for
    /// a merge a W-instance cannot represent, so the extension refuses instead
    /// of emitting a node with two children under one field.
    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_refuses_a_contracting_vertex_map() {
        let inst = three_node_instance();
        let tgt_edge = Edge {
            src: "article:body".into(),
            tgt: "article:body.one".into(),
            kind: "prop".into(),
            name: Some("one".into()),
        };
        let tgt_schema = make_test_schema(&["article:body", "article:body.one"], &[tgt_edge]);

        // Both `text` and `createdAt` land on the single target field.
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post:body"), Name::from("article:body"));
        vertex_remap.insert(Name::from("post:body.text"), Name::from("article:body.one"));
        vertex_remap.insert(
            Name::from("post:body.createdAt"),
            Name::from("article:body.one"),
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([
                Name::from("article:body"),
                Name::from("article:body.one"),
            ]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let refused = wtype_extend_engine(&inst, &tgt_schema, &migration);
        assert!(
            matches!(refused, Err(RestrictError::NonInjectiveVertexMap { .. })),
            "a contracting vertex map must be refused, not answered with a node \
             carrying both values: got {refused:?}"
        );

        // The partial variant is about which *nodes* travel, so it refuses too.
        assert!(matches!(
            wtype_extend_partial(&inst, &tgt_schema, &migration),
            Err(RestrictError::NonInjectiveVertexMap { .. })
        ));
    }

    /// An arc whose edge the migration dropped must not be relabelled onto a
    /// sibling's edge.
    ///
    /// Two parallel source arcs, `a` and `b`, between one vertex pair; the
    /// target kept only `a`; the migration maps `a` and says nothing about `b`.
    /// Resolving `b` from its remapped anchors finds the one edge that runs
    /// between them, which is `a`, and delivers `b`'s value under `a`'s name.
    #[test]
    #[allow(clippy::unwrap_used)]
    fn a_dropped_edge_is_not_relabelled_onto_a_sibling() {
        let src_a = Edge {
            src: "root".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };
        let src_b = Edge {
            src: "root".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("b".into()),
        };
        let tgt_a = Edge {
            src: "root".into(),
            tgt: "leaf2".into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(
            1,
            Node::new(1, "leaf").with_value(FieldPresence::Present(Value::Str("A".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "leaf").with_value(FieldPresence::Present(Value::Str("B".into()))),
        );
        let inst = WInstance::new(
            nodes,
            vec![(0, 1, src_a.clone()), (0, 2, src_b)],
            vec![],
            0,
            Name::from("root"),
        );

        let tgt_schema = make_test_schema(&["root", "leaf2"], std::slice::from_ref(&tgt_a));
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("leaf"), Name::from("leaf2"));
        let mut edge_remap = HashMap::new();
        edge_remap.insert(src_a, tgt_a.clone());
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("leaf2")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let extended = wtype_extend_engine(&inst, &tgt_schema, &migration).unwrap();
        let labelled_a = extended
            .arcs
            .iter()
            .filter(|(_, _, edge)| *edge == tgt_a)
            .count();
        assert_eq!(
            labelled_a, 1,
            "only the arc the migration mapped may wear `a`: got {:?}",
            extended.arcs
        );
        // `b`'s value went nowhere rather than arriving under `a`.
        assert!(
            !extended.arcs.iter().any(|(_, child, _)| *child == 2),
            "the unmapped arc is dropped, not relabelled: {:?}",
            extended.arcs
        );
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
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };
        let result = wtype_extend_engine(&inst, &tgt_schema, &migration).unwrap();
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
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };
        let result = wtype_extend_engine(&inst, &schema, &migration).unwrap();
        // Verify parent/children maps are correctly rebuilt
        assert_eq!(result.parent(1), Some(0));
        assert_eq!(result.parent(2), Some(0));
        assert!(result.children(0).contains(&1));
        assert!(result.children(0).contains(&2));
        // Verify values are preserved
        assert!(result.nodes[&1].has_value());
        assert!(result.nodes[&2].has_value());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn extend_errors_on_unmapped_anchor() {
        // three_node_instance: post:body(0) -> post:body.text(1),
        //                      post:body(0) -> post:body.createdAt(2).
        let inst = three_node_instance();
        let edge_text = Edge {
            src: "post:body".into(),
            tgt: "post:body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        // Target schema omits createdAt entirely.
        let schema = make_test_schema(
            &["post:body", "post:body.text"],
            std::slice::from_ref(&edge_text),
        );
        // Migration keeps body and body.text but neither remaps nor survives
        // post:body.createdAt, so node 2 has no image in the target schema.
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("post:body"), Name::from("post:body.text")]),
            surviving_edges: HashSet::from([edge_text]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // Total extend must error rather than silently drop node 2.
        let err = wtype_extend_engine(&inst, &schema, &migration).unwrap_err();
        assert!(
            matches!(err, RestrictError::UnmappedAnchor { node_id: 2, .. }),
            "expected UnmappedAnchor for node 2, got {err:?}"
        );

        // The explicit partial variant drops node 2 and reports its id.
        let (extended, dropped) = wtype_extend_partial(&inst, &schema, &migration).unwrap();
        assert_eq!(dropped, vec![2]);
        assert_eq!(extended.node_count(), 2);
        assert!(!extended.nodes.contains_key(&2));
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
        // Source instance: post:body { text: "hello", title: "world" }
        let src_text_edge = Edge {
            src: "post:body".into(),
            tgt: "post:text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let src_title_edge = Edge {
            src: "post:body".into(),
            tgt: "post:title".into(),
            kind: "prop".into(),
            name: Some("title".into()),
        };
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
            (0, 1, src_text_edge.clone()),
            (0, 2, src_title_edge.clone()),
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
            smallvec::smallvec![tgt_content_edge.clone()],
        );
        tgt_between.insert(
            (Name::from("post:body"), Name::from("post:title")),
            smallvec::smallvec![tgt_title_edge.clone()],
        );
        let tgt_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::from([
                (tgt_content_edge.clone(), Name::from("prop")),
                (tgt_title_edge.clone(), Name::from("prop")),
            ]),
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
            between: tgt_between,
            ..Default::default()
        };

        // Migration: post:text → post:content and its incident edge are
        // both renamed; post:title and its edge keep their names.
        let mut surviving_verts = HashSet::new();
        surviving_verts.insert(Name::from("post:body"));
        surviving_verts.insert(Name::from("post:content")); // target name
        surviving_verts.insert(Name::from("post:title"));

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("post:text"), Name::from("post:content"));

        let migration = CompiledMigration {
            surviving_verts,
            surviving_edges: HashSet::from([tgt_content_edge.clone(), tgt_title_edge]),
            vertex_remap,
            edge_remap: HashMap::from([(src_text_edge.clone(), tgt_content_edge.clone())]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let src_schema = make_test_schema(
            &["post:body", "post:text", "post:title"],
            &[src_text_edge, src_title_edge],
        );

        let result = restrict_engine(&inst, &src_schema, &tgt_schema, &migration)
            .expect("restrict should succeed");

        // All three nodes must survive (root + renamed + unchanged)
        assert_eq!(result.nodes.len(), 3, "all three nodes should survive");

        // The renamed node should now have anchor "post:content"
        let renamed_node = result.nodes.get(&1).expect("node 1 should survive");
        assert_eq!(renamed_node.anchor.as_ref(), "post:content");
        assert!(renamed_node.has_value(), "renamed node must keep its value");
        assert!(
            result.arcs.iter().any(|(parent, child, edge)| *parent == 0
                && *child == 1
                && edge == &tgt_content_edge),
            "the renamed node must be attached by the mapped target edge"
        );

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

    /// Restriction must not infer the image of an unmapped parallel source
    /// edge from endpoints alone.
    #[test]
    #[allow(clippy::unwrap_used)]
    fn restrict_drops_an_unmapped_parallel_edge_instead_of_relabelling_it() {
        let src_a = Edge {
            src: "root".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };
        let src_b = Edge {
            src: "root".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("b".into()),
        };
        let tgt_a = Edge {
            src: "root".into(),
            tgt: "leaf2".into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "leaf"));
        nodes.insert(2, Node::new(2, "leaf"));
        let instance = WInstance::new(
            nodes,
            vec![(0, 1, src_a.clone()), (0, 2, src_b.clone())],
            vec![],
            0,
            Name::from("root"),
        );
        let src_schema = make_test_schema(&["root", "leaf"], &[src_a.clone(), src_b]);
        let tgt_schema = make_test_schema(&["root", "leaf2"], std::slice::from_ref(&tgt_a));
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("leaf2")]),
            surviving_edges: HashSet::from([tgt_a.clone()]),
            vertex_remap: HashMap::from([(Name::from("leaf"), Name::from("leaf2"))]),
            edge_remap: HashMap::from([(src_a, tgt_a.clone())]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let restricted = restrict_engine(&instance, &src_schema, &tgt_schema, &migration).unwrap();
        assert!(restricted.nodes.contains_key(&1));
        assert!(!restricted.nodes.contains_key(&2));
        assert_eq!(restricted.arcs, vec![(0, 1, tgt_a)]);
    }

    // --- PathTransform tests ---

    #[test]
    #[allow(clippy::expect_used)]
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
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        match node.extra_fields.get("attrs") {
            Some(Value::Unknown(map)) => {
                assert!(!map.contains_key("old_attr"));
                assert_eq!(map.get("new_attr"), Some(&Value::Str("val".into())));
            }
            other => panic!("expected Unknown map, got {other:?}"),
        }
    }

    #[test]
    #[allow(clippy::expect_used)]
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
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        assert!(!node.extra_fields.contains_key("color"));
        assert_eq!(
            node.extra_fields.get("colour"),
            Some(&Value::Str("red".into()))
        );
    }

    // --- MapReferences tests ---

    #[test]
    #[allow(clippy::expect_used)]
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
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("parent"),
            Some(&Value::Str("new_name".into()))
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn map_references_filters_list() {
        let mut node = Node::new(0, "v");
        node.extra_fields.insert(
            "parents".to_string(),
            Value::List(vec![
                Value::Str("alpha".into()),
                Value::Str("beta".into()),
                Value::Str("gamma".into()),
            ]),
        );

        let mut rename_map = HashMap::new();
        rename_map.insert("alpha".to_string(), Some("alpha_v2".to_string()));
        rename_map.insert("beta".to_string(), None); // drop

        let transform = FieldTransform::MapReferences {
            field: "parents".to_string(),
            rename_map,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        match node.extra_fields.get("parents") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Str("alpha_v2".into()));
                assert_eq!(items[1], Value::Str("gamma".into()));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn map_references_drops_removed_entries() {
        let mut node = Node::new(0, "v");
        node.extra_fields.insert(
            "refs".to_string(),
            Value::List(vec![
                Value::Str("gone".into()),
                Value::Str("also_gone".into()),
            ]),
        );

        let mut rename_map = HashMap::new();
        rename_map.insert("gone".to_string(), None);
        rename_map.insert("also_gone".to_string(), None);

        let transform = FieldTransform::MapReferences {
            field: "refs".to_string(),
            rename_map,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        match node.extra_fields.get("refs") {
            Some(Value::List(items)) => {
                assert!(items.is_empty(), "expected empty list, got {items:?}");
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn value_to_expr_literal_preserves_string_list() -> Result<(), NotExpressible> {
        // A list converts to `Literal::List`, element-wise, so that `map`
        // and `fold` can reach it. It is NOT joined into a string.
        let val = Value::List(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
            Value::Str("c".into()),
        ]);
        match value_to_expr_literal(&val)? {
            panproto_expr::Literal::List(items) => assert_eq!(
                items,
                vec![
                    panproto_expr::Literal::Str("a".into()),
                    panproto_expr::Literal::Str("b".into()),
                    panproto_expr::Literal::Str("c".into()),
                ]
            ),
            other => panic!("expected Literal::List, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn value_to_expr_literal_keeps_non_string_list_elements() -> Result<(), NotExpressible> {
        // Mixed-type elements all survive. The previous joined-string
        // projection dropped every non-string element, which turned a list
        // of integers into an empty string.
        let val = Value::List(vec![
            Value::Str("keep".into()),
            Value::Int(42),
            Value::Bool(true),
            Value::Null,
        ]);
        match value_to_expr_literal(&val)? {
            panproto_expr::Literal::List(items) => assert_eq!(
                items,
                vec![
                    panproto_expr::Literal::Str("keep".into()),
                    panproto_expr::Literal::Int(42),
                    panproto_expr::Literal::Bool(true),
                    panproto_expr::Literal::Null,
                ],
                "non-string list elements must survive the conversion"
            ),
            other => panic!("expected Literal::List, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn value_to_expr_literal_empty_list_is_empty_list() -> Result<(), NotExpressible> {
        match value_to_expr_literal(&Value::List(Vec::new()))? {
            panproto_expr::Literal::List(items) => assert!(items.is_empty()),
            other => panic!("expected Literal::List, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn value_to_expr_literal_nests_lists_and_records() -> Result<(), NotExpressible> {
        // A list of records: the shape an ATProto array-of-objects field
        // takes, and the one `map (\o -> o.a) objs` has to traverse.
        let val = Value::List(vec![Value::Unknown(HashMap::from([
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::Int(10)),
        ]))]);
        match value_to_expr_literal(&val)? {
            panproto_expr::Literal::List(items) => match &items[..] {
                [panproto_expr::Literal::Record(fields)] => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(&*fields[0].0, "a");
                    assert_eq!(fields[0].1, panproto_expr::Literal::Int(1));
                    assert_eq!(&*fields[1].0, "b");
                    assert_eq!(fields[1].1, panproto_expr::Literal::Int(10));
                }
                other => panic!("expected one Record element, got {other:?}"),
            },
            other => panic!("expected Literal::List, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn value_to_expr_literal_record_fields_are_sorted() -> Result<(), NotExpressible> {
        // `Value::Unknown` is a HashMap, so field order must be imposed
        // rather than inherited: the same map must always produce the same
        // `Literal::Record`, or equality and hashing over it are unstable.
        let val = Value::Unknown(HashMap::from([
            ("zulu".to_string(), Value::Int(3)),
            ("alpha".to_string(), Value::Int(1)),
            ("mike".to_string(), Value::Int(2)),
        ]));
        for _ in 0..16 {
            match value_to_expr_literal(&val)? {
                panproto_expr::Literal::Record(fields) => {
                    let keys: Vec<&str> = fields.iter().map(|(k, _)| &**k).collect();
                    assert_eq!(keys, vec!["alpha", "mike", "zulu"]);
                }
                other => panic!("expected Literal::Record, got {other:?}"),
            }
        }
        Ok(())
    }

    #[test]
    fn value_to_expr_literal_non_collection_variants_pass_through() -> Result<(), NotExpressible> {
        assert!(matches!(
            value_to_expr_literal(&Value::Bool(true))?,
            panproto_expr::Literal::Bool(true)
        ));
        assert!(matches!(
            value_to_expr_literal(&Value::Int(7))?,
            panproto_expr::Literal::Int(7)
        ));
        assert!(matches!(
            value_to_expr_literal(&Value::Null)?,
            panproto_expr::Literal::Null
        ));
        assert_eq!(
            value_to_expr_literal(&Value::Bytes(vec![1, 2, 3]))?,
            panproto_expr::Literal::Bytes(vec![1, 2, 3])
        );
        // An empty record is a record, not a null.
        assert!(matches!(
            value_to_expr_literal(&Value::Unknown(HashMap::new()))?,
            panproto_expr::Literal::Record(ref f) if f.is_empty()
        ));
        // Variants with no faithful Literal counterpart are refused, never
        // read as a null.
        assert_eq!(
            value_to_expr_literal(&Value::Token("t".into())),
            Err(NotExpressible {
                value_type: "token"
            })
        );
        Ok(())
    }

    #[test]
    fn expr_literal_to_value_preserves_lists_and_records() {
        // The reverse direction: an expression that RETURNS a list of
        // records must be written back as structured data, not collapsed
        // to a null.
        let lit = panproto_expr::Literal::List(vec![panproto_expr::Literal::Record(vec![
            (std::sync::Arc::from("x"), panproto_expr::Literal::Int(1)),
            (
                std::sync::Arc::from("y"),
                panproto_expr::Literal::Str("s".into()),
            ),
        ])]);
        match expr_literal_to_value(&lit) {
            Value::List(items) => match &items[..] {
                [Value::Unknown(map)] => {
                    assert_eq!(map.get("x"), Some(&Value::Int(1)));
                    assert_eq!(map.get("y"), Some(&Value::Str("s".into())));
                }
                other => panic!("expected one Unknown element, got {other:?}"),
            },
            other => panic!("expected Value::List, got {other:?}"),
        }
    }

    #[test]
    fn value_literal_round_trip_is_identity_on_containers() -> Result<(), NotExpressible> {
        // The two converters are mutually inverse on the container
        // variants, which is what makes a transform's output re-readable
        // by the next transform in the sequence.
        let val = Value::Unknown(HashMap::from([
            (
                "nums".to_string(),
                Value::List(vec![Value::Int(1), Value::Int(2)]),
            ),
            (
                "nested".to_string(),
                Value::Unknown(HashMap::from([("a".to_string(), Value::Str("z".into()))])),
            ),
            ("flag".to_string(), Value::Bool(false)),
        ]));
        assert_eq!(expr_literal_to_value(&value_to_expr_literal(&val)?), val);
        Ok(())
    }

    // -----------------------------------------------------------------
    // List- and record-valued field transforms
    //
    // Each of these mirrors one of the reproduction cases reported
    // against `@panproto/core` 0.59.0, where the transform applied
    // cleanly to a scalar field but silently no-op'd on a list- or
    // object-valued one. They fail on the joined-string conversion.
    // -----------------------------------------------------------------

    /// A node carrying the four field shapes from the report: a scalar
    /// array, an array of objects, and a nested object.
    fn node_with_container_fields() -> Node {
        let mut node = Node::new(0, "rec");
        node.extra_fields.insert(
            "nums".to_string(),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
        );
        node.extra_fields.insert(
            "objs".to_string(),
            Value::List(vec![
                Value::Unknown(HashMap::from([
                    ("a".to_string(), Value::Int(1)),
                    ("b".to_string(), Value::Int(10)),
                ])),
                Value::Unknown(HashMap::from([
                    ("a".to_string(), Value::Int(2)),
                    ("b".to_string(), Value::Int(20)),
                ])),
            ]),
        );
        node.extra_fields.insert(
            "nested".to_string(),
            Value::Unknown(HashMap::from([
                ("a".to_string(), Value::Int(7)),
                ("b".to_string(), Value::Int(70)),
            ])),
        );
        node
    }

    /// `\x -> x + 1` as a lambda expression.
    fn increment_lambda() -> panproto_expr::Expr {
        panproto_expr::Expr::Lam(
            std::sync::Arc::from("x"),
            Box::new(panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Add,
                vec![
                    panproto_expr::Expr::Var(std::sync::Arc::from("x")),
                    panproto_expr::Expr::Lit(panproto_expr::Literal::Int(1)),
                ],
            )),
        )
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn apply_expr_maps_over_an_integer_list() {
        // `map (\x -> x + 1) nums` over [1,2,3].
        let mut node = node_with_container_fields();
        let transform = FieldTransform::ApplyExpr {
            key: "nums".to_string(),
            expr: panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Map,
                vec![
                    panproto_expr::Expr::Var(std::sync::Arc::from("nums")),
                    increment_lambda(),
                ],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("map over a list field should evaluate");

        assert_eq!(
            node.extra_fields.get("nums"),
            Some(&Value::List(vec![
                Value::Int(2),
                Value::Int(3),
                Value::Int(4)
            ])),
            "map over an integer list must produce the incremented list, not leave it untouched"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn apply_expr_maps_a_projection_over_a_record_list() {
        // `map (\o -> o.a) objs` over [{a:1,b:10},{a:2,b:20}].
        let mut node = node_with_container_fields();
        let project_a = panproto_expr::Expr::Lam(
            std::sync::Arc::from("o"),
            Box::new(panproto_expr::Expr::Field(
                Box::new(panproto_expr::Expr::Var(std::sync::Arc::from("o"))),
                std::sync::Arc::from("a"),
            )),
        );
        let transform = FieldTransform::ApplyExpr {
            key: "objs".to_string(),
            expr: panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Map,
                vec![
                    panproto_expr::Expr::Var(std::sync::Arc::from("objs")),
                    project_a,
                ],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("map over a record list should evaluate");

        assert_eq!(
            node.extra_fields.get("objs"),
            Some(&Value::List(vec![Value::Int(1), Value::Int(2)])),
            "field projection over a list of records must reach each record's fields"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn compute_field_reads_through_a_nested_record() {
        // `nested.a` where nested = {a:7,b:70}.
        let mut node = node_with_container_fields();
        let transform = FieldTransform::ComputeField {
            target_key: "out".to_string(),
            expr: panproto_expr::Expr::Field(
                Box::new(panproto_expr::Expr::Var(std::sync::Arc::from("nested"))),
                std::sync::Arc::from("a"),
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("field access into a nested record should evaluate");

        assert_eq!(
            node.extra_fields.get("out"),
            Some(&Value::Int(7)),
            "field access into a nested object must resolve, not emit nothing"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn compute_field_folds_over_a_list() {
        // `fold (\x y -> x + y) 0 nums` over [1,2,3].
        let mut node = node_with_container_fields();
        let add = panproto_expr::Expr::Lam(
            std::sync::Arc::from("x"),
            Box::new(panproto_expr::Expr::Lam(
                std::sync::Arc::from("y"),
                Box::new(panproto_expr::Expr::Builtin(
                    panproto_expr::BuiltinOp::Add,
                    vec![
                        panproto_expr::Expr::Var(std::sync::Arc::from("x")),
                        panproto_expr::Expr::Var(std::sync::Arc::from("y")),
                    ],
                )),
            )),
        );
        let transform = FieldTransform::ComputeField {
            target_key: "out".to_string(),
            expr: panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Fold,
                vec![
                    panproto_expr::Expr::Var(std::sync::Arc::from("nums")),
                    panproto_expr::Expr::Lit(panproto_expr::Literal::Int(0)),
                    add,
                ],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("fold over a list field should evaluate");

        assert_eq!(
            node.extra_fields.get("out"),
            Some(&Value::Int(6)),
            "fold over an integer list must sum it"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn compute_field_builds_a_nested_record_list() {
        // The regroup shape the report is blocked on: build a list of
        // records from a list of records, nesting some of the source
        // fields one level deeper. Exercises structure preservation in
        // BOTH directions in a single transform.
        let mut node = node_with_container_fields();
        let regroup = panproto_expr::Expr::Lam(
            std::sync::Arc::from("o"),
            Box::new(panproto_expr::Expr::Record(vec![
                (
                    std::sync::Arc::from("outer"),
                    panproto_expr::Expr::Field(
                        Box::new(panproto_expr::Expr::Var(std::sync::Arc::from("o"))),
                        std::sync::Arc::from("a"),
                    ),
                ),
                (
                    std::sync::Arc::from("inner"),
                    panproto_expr::Expr::Record(vec![(
                        std::sync::Arc::from("deep"),
                        panproto_expr::Expr::Field(
                            Box::new(panproto_expr::Expr::Var(std::sync::Arc::from("o"))),
                            std::sync::Arc::from("b"),
                        ),
                    )]),
                ),
            ])),
        );
        let transform = FieldTransform::ComputeField {
            target_key: "regrouped".to_string(),
            expr: panproto_expr::Expr::Builtin(
                panproto_expr::BuiltinOp::Map,
                vec![
                    panproto_expr::Expr::Var(std::sync::Arc::from("objs")),
                    regroup,
                ],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("building a nested record list should evaluate");

        let expected = Value::List(vec![
            Value::Unknown(HashMap::from([
                ("outer".to_string(), Value::Int(1)),
                (
                    "inner".to_string(),
                    Value::Unknown(HashMap::from([("deep".to_string(), Value::Int(10))])),
                ),
            ])),
            Value::Unknown(HashMap::from([
                ("outer".to_string(), Value::Int(2)),
                (
                    "inner".to_string(),
                    Value::Unknown(HashMap::from([("deep".to_string(), Value::Int(20))])),
                ),
            ])),
        ]);
        assert_eq!(node.extra_fields.get("regrouped"), Some(&expected));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn contains_tests_membership_on_a_list_field() {
        // Membership predicates over a list field are what the joined
        // string used to serve. `Contains` now takes the list directly.
        let mut node = Node::new(0, "rec");
        node.extra_fields.insert(
            "tags".to_string(),
            Value::List(vec![Value::Str("alpha".into()), Value::Str("beta".into())]),
        );

        let case = FieldTransform::Case {
            branches: vec![CaseBranch {
                predicate: panproto_expr::Expr::builtin(
                    panproto_expr::BuiltinOp::Contains,
                    vec![
                        panproto_expr::Expr::Var(std::sync::Arc::from("tags")),
                        panproto_expr::Expr::Lit(panproto_expr::Literal::Str("beta".into())),
                    ],
                ),
                transforms: vec![FieldTransform::AddField {
                    key: "matched".into(),
                    value: Value::Bool(true),
                }],
            }],
        };
        apply_field_transforms(&mut node, &[case], &TransformContext::detached())
            .expect("membership predicate should evaluate");

        assert_eq!(
            node.extra_fields.get("matched"),
            Some(&Value::Bool(true)),
            "Contains must test element membership on a list-valued field"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn contains_on_a_list_does_not_match_a_substring_of_an_element() {
        // Membership is exact-element, not substring. Under the old
        // joined-string form `Contains(["alpha"], "lph")` matched, which
        // is not what a membership predicate means.
        let mut node = Node::new(0, "rec");
        node.extra_fields.insert(
            "tags".to_string(),
            Value::List(vec![Value::Str("alpha".into())]),
        );

        let case = FieldTransform::Case {
            branches: vec![CaseBranch {
                predicate: panproto_expr::Expr::builtin(
                    panproto_expr::BuiltinOp::Contains,
                    vec![
                        panproto_expr::Expr::Var(std::sync::Arc::from("tags")),
                        panproto_expr::Expr::Lit(panproto_expr::Literal::Str("lph".into())),
                    ],
                ),
                transforms: vec![FieldTransform::AddField {
                    key: "matched".into(),
                    value: Value::Bool(true),
                }],
            }],
        };
        apply_field_transforms(&mut node, &[case], &TransformContext::detached())
            .expect("membership predicate should evaluate");

        assert!(
            !node.extra_fields.contains_key("matched"),
            "list membership must be exact-element, not substring"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn failed_transform_reports_instead_of_silently_skipping() {
        // The diagnosability half of the fix: an expression that cannot
        // evaluate surfaces an error naming the field, rather than
        // leaving the field untouched and returning success.
        let mut node = Node::new(0, "rec");
        node.extra_fields.insert("n".to_string(), Value::Int(1));

        let transform = FieldTransform::ComputeField {
            target_key: "out".to_string(),
            // `missing` is unbound in this environment.
            expr: panproto_expr::Expr::Var(std::sync::Arc::from("missing")),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        let err = apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect_err("an unevaluable transform must report");

        match err {
            RestrictError::FieldTransformFailed { key, .. } => assert_eq!(key, "out"),
            other => panic!("expected FieldTransformFailed, got {other:?}"),
        }
        assert!(
            !node.extra_fields.contains_key("out"),
            "a failed transform must not write a partial result"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn failed_nested_transform_preserves_the_nested_map() {
        // `apply_path_transform` moves the nested map out to operate on
        // it. A failure mid-way must still put it back, or the transform
        // that could not run would also erase what it was reading.
        let mut node = Node::new(0, "rec");
        node.extra_fields.insert(
            "outer".to_string(),
            Value::Unknown(HashMap::from([("kept".to_string(), Value::Int(5))])),
        );

        let transform = FieldTransform::PathTransform {
            path: vec!["outer".to_string()],
            inner: Box::new(FieldTransform::ComputeField {
                target_key: "out".to_string(),
                expr: panproto_expr::Expr::Var(std::sync::Arc::from("missing")),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            }),
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect_err("an unevaluable nested transform must report");

        assert_eq!(
            node.extra_fields.get("outer"),
            Some(&Value::Unknown(HashMap::from([(
                "kept".to_string(),
                Value::Int(5)
            )]))),
            "the nested map must survive a failed transform"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn map_references_preserves_non_string_elements() {
        // MapReferences is the action of the rename on *string leaves*;
        // non-string elements in a list pass through unchanged. This
        // pins the functoriality in the Kleisli category of the "rename
        // or drop" partial map.
        let mut node = Node::new(0, "v");
        node.extra_fields.insert(
            "mixed".to_string(),
            Value::List(vec![
                Value::Str("renameme".into()),
                Value::Int(42),
                Value::Bool(true),
                Value::Str("dropme".into()),
            ]),
        );

        let mut rename_map = HashMap::new();
        rename_map.insert("renameme".to_string(), Some("renamed".to_string()));
        rename_map.insert("dropme".to_string(), None);

        let transform = FieldTransform::MapReferences {
            field: "mixed".to_string(),
            rename_map,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        match node.extra_fields.get("mixed") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::Str("renamed".into()));
                assert_eq!(items[1], Value::Int(42));
                assert_eq!(items[2], Value::Bool(true));
            }
            other => panic!("expected List, got {other:?}"),
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
            edges: HashMap::from([(edge.clone(), Name::from("prop"))]),
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
            between,
            ..Default::default()
        };
        let src_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::from([(edge.clone(), Name::from("prop"))]),
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
            ..Default::default()
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
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };
        migration.add_conditional_survival("item", predicate);

        let result =
            restrict_engine(&inst, &src_schema, &tgt_schema, &migration).expect("restrict ok");

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
            edges: HashMap::from([(edge.clone(), Name::from("prop"))]),
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
            between,
            ..Default::default()
        };
        let src_schema = Schema {
            protocol: "test".into(),
            vertices: HashMap::new(),
            edges: HashMap::from([(edge.clone(), Name::from("prop"))]),
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
            ..Default::default()
        };

        // No conditional_survival predicates; node should survive normally
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("item")]),
            surviving_edges: HashSet::from([edge]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result =
            restrict_engine(&inst, &src_schema, &tgt_schema, &migration).expect("restrict ok");

        assert_eq!(result.node_count(), 2);
        assert!(result.nodes.contains_key(&1));
    }

    // --- ComputeField tests ---

    #[test]
    #[allow(clippy::expect_used)]
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
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("name"),
            Some(&Value::Str("h2".into()))
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
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
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("name"),
            Some(&Value::Str("h3".into()))
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
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
                        inverse: None,
                        coercion_class: panproto_gat::CoercionClass::Opaque,
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
                        inverse: None,
                        coercion_class: panproto_gat::CoercionClass::Opaque,
                    }],
                },
            ],
        };

        apply_field_transforms(&mut node, &[case], &TransformContext::detached())
            .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("name"),
            Some(&Value::Str("h1".into()))
        );
    }

    // --- Child scalar access tests ---

    /// Build a 3-node instance: root object + two string children.
    fn instance_with_scalar_children() -> (WInstance, HashMap<String, Value>) {
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.repo").with_value(FieldPresence::Present(Value::Str(
                "at://did:plc:abc/app.bsky.feed.post/rkey123".into(),
            ))),
        );
        nodes.insert(
            2,
            Node::new(2, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello world".into()))),
        );

        let edge_repo = Edge {
            src: "body".into(),
            tgt: "body.repo".into(),
            kind: "prop".into(),
            name: Some("repo".into()),
        };
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };

        let arcs = vec![(0, 1, edge_repo), (0, 2, edge_text)];
        let instance = WInstance::new(nodes, arcs, vec![], 0, "body".into());
        let scalars = collect_scalar_child_values(&instance, 0);
        (instance, scalars)
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn compute_field_reads_scalar_child() {
        // ComputeField must read string fields stored as child
        // vertices, not just those in `extra_fields`. This unit test
        // covers the basic access path; the integration test
        // `at_uri_decomposition_end_to_end` exercises real Split /
        // Index expressions for full AT-URI parsing.
        let (_instance, scalars) = instance_with_scalar_children();
        let mut node = Node::new(0, "body");

        let expr = panproto_expr::Expr::Var(std::sync::Arc::from("repo"));

        let transform = FieldTransform::ComputeField {
            target_key: "repo_copy".to_string(),
            expr,
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(
            &mut node,
            &[transform],
            &TransformContext::from_child_values(scalars),
        )
        .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("repo_copy"),
            Some(&Value::Str(
                "at://did:plc:abc/app.bsky.feed.post/rkey123".into()
            )),
            "ComputeField should read scalar child value via dependent-sum projection"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn apply_expr_on_scalar_child() {
        let (_instance, scalars) = instance_with_scalar_children();
        let mut node = Node::new(0, "body");

        // ApplyExpr on "text" (a child scalar): should find it and write
        // the transformed result to extra_fields.
        let expr = panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Concat,
            vec![
                panproto_expr::Expr::Var(std::sync::Arc::from("text")),
                panproto_expr::Expr::Lit(panproto_expr::Literal::Str("!".into())),
            ],
        );
        let transform = FieldTransform::ApplyExpr {
            key: "text".to_string(),
            expr,
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(
            &mut node,
            &[transform],
            &TransformContext::from_child_values(scalars),
        )
        .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("text"),
            Some(&Value::Str("hello world!".into())),
            "ApplyExpr should read child scalar and write result to extra_fields"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn case_branch_on_scalar_child() {
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use std::sync::Arc;

        let (_instance, scalars) = instance_with_scalar_children();
        let mut node = Node::new(0, "body");

        // Branch: if (contains repo "did:plc") then add field "has_did" = true
        let case = FieldTransform::Case {
            branches: vec![CaseBranch {
                predicate: Expr::builtin(
                    BuiltinOp::Contains,
                    vec![
                        Expr::Var(Arc::from("repo")),
                        Expr::Lit(Literal::Str("did:plc".into())),
                    ],
                ),
                transforms: vec![FieldTransform::AddField {
                    key: "has_did".into(),
                    value: Value::Bool(true),
                }],
            }],
        };
        apply_field_transforms(
            &mut node,
            &[case],
            &TransformContext::from_child_values(scalars),
        )
        .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("has_did"),
            Some(&Value::Bool(true)),
            "Case predicate should evaluate against child scalar values"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn drop_field_on_extra_field_still_works() {
        let mut node = Node::new(0, "v");
        node.extra_fields
            .insert("keep".into(), Value::Str("yes".into()));
        node.extra_fields
            .insert("drop_me".into(), Value::Str("bye".into()));

        let transform = FieldTransform::DropField {
            key: "drop_me".into(),
        };
        apply_field_transforms(&mut node, &[transform], &TransformContext::detached())
            .expect("transform should evaluate");

        assert!(node.extra_fields.contains_key("keep"));
        assert!(!node.extra_fields.contains_key("drop_me"));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn child_scalars_do_not_override_extra_fields() {
        // When a key exists in both extra_fields and child_scalars,
        // extra_fields must take precedence (binding order correctness).
        let mut node = Node::new(0, "v");
        node.extra_fields
            .insert("repo".into(), Value::Str("from_extra_fields".into()));

        let mut child_scalars = HashMap::new();
        child_scalars.insert("repo".into(), Value::Str("from_child".into()));

        let expr = panproto_expr::Expr::Var(std::sync::Arc::from("repo"));
        let transform = FieldTransform::ComputeField {
            target_key: "repo_copy".to_string(),
            expr,
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };
        apply_field_transforms(
            &mut node,
            &[transform],
            &TransformContext::from_child_values(child_scalars.clone()),
        )
        .expect("transform should evaluate");

        assert_eq!(
            node.extra_fields.get("repo_copy"),
            Some(&Value::Str("from_extra_fields".into())),
            "extra_fields must take precedence over child_scalars"
        );
    }

    #[test]
    fn collect_scalar_child_values_completeness() {
        let (instance, scalars) = instance_with_scalar_children();
        assert_eq!(scalars.len(), 2, "should collect both scalar children");
        assert_eq!(
            scalars.get("repo"),
            Some(&Value::Str(
                "at://did:plc:abc/app.bsky.feed.post/rkey123".into()
            ))
        );
        assert_eq!(scalars.get("text"), Some(&Value::Str("hello world".into())));

        // Root node has no parent, so collecting from a non-existent parent returns empty
        assert!(collect_scalar_child_values(&instance, 99).is_empty());
    }

    #[test]
    fn env_monotonicity() {
        // build_env_with_children must bind every key that
        // build_env_from_extra_fields binds, with the same value.
        let mut extra = HashMap::new();
        extra.insert("alpha".into(), Value::Str("a".into()));
        extra.insert("beta".into(), Value::Int(42));

        let mut children = HashMap::new();
        children.insert("gamma".into(), Value::Str("g".into()));
        children.insert("delta".into(), Value::Bool(true));

        let env_base = build_env_from_extra_fields(&extra);
        let env_extended = build_env_with_children(&extra, &children);

        // Every binding from base must be present in extended
        let config = panproto_expr::EvalConfig::default();
        for key in ["alpha", "beta"] {
            let var = panproto_expr::Expr::Var(std::sync::Arc::from(key));
            let base_result = panproto_expr::eval(&var, &env_base, &config).ok();
            let ext_result = panproto_expr::eval(&var, &env_extended, &config).ok();
            assert_eq!(
                base_result, ext_result,
                "binding for {key} must match between base and extended env"
            );
        }

        // Extended env should also have child bindings
        for key in ["gamma", "delta"] {
            let var = panproto_expr::Expr::Var(std::sync::Arc::from(key));
            assert!(
                panproto_expr::eval(&var, &env_extended, &config).is_ok(),
                "extended env should bind child scalar {key}"
            );
        }
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn compute_field_deterministic() {
        // Applying the same ComputeField twice produces the same result
        // (fiber endomorphism idempotence when source data is unchanged).
        let (_instance, scalars) = instance_with_scalar_children();
        let expr = panproto_expr::Expr::Var(std::sync::Arc::from("repo"));
        let transform = FieldTransform::ComputeField {
            target_key: "derived".to_string(),
            expr,
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        };

        let mut node1 = Node::new(0, "body");
        apply_field_transforms(
            &mut node1,
            std::slice::from_ref(&transform),
            &TransformContext::from_child_values(scalars.clone()),
        )
        .expect("transform should evaluate");
        let result1 = node1.extra_fields.get("derived").cloned();

        let mut node2 = Node::new(0, "body");
        apply_field_transforms(
            &mut node2,
            std::slice::from_ref(&transform),
            &TransformContext::from_child_values(scalars),
        )
        .expect("transform should evaluate");
        let result2 = node2.extra_fields.get("derived").cloned();

        assert_eq!(result1, result2, "ComputeField must be deterministic");
    }

    /// Every [`FieldTransform`] variant, translated to a [`TermAssignment`]
    /// and lowered back, produces the same result as applying the original
    /// transform directly. This certifies that the term-assignment algebra
    /// faithfully re-expresses each field transform.
    #[test]
    #[allow(clippy::expect_used)]
    fn field_transform_term_equivalence() {
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use std::sync::Arc;

        fn fixture() -> Node {
            let mut node = Node::new(0, "row");
            node.extra_fields
                .insert("a".into(), Value::Str("hello".into()));
            node.extra_fields.insert(
                "refs".into(),
                Value::List(vec![Value::Str("x".into()), Value::Str("keep".into())]),
            );
            let mut attrs = HashMap::new();
            attrs.insert("k".into(), Value::Int(1));
            node.extra_fields
                .insert("attrs".into(), Value::Unknown(attrs));
            node
        }

        let variants: Vec<FieldTransform> = vec![
            FieldTransform::RenameField {
                old_key: "a".into(),
                new_key: "b".into(),
            },
            FieldTransform::DropField { key: "a".into() },
            FieldTransform::AddField {
                key: "c".into(),
                value: Value::Int(7),
            },
            FieldTransform::KeepFields {
                keys: vec!["a".into()],
            },
            FieldTransform::ApplyExpr {
                key: "a".into(),
                expr: Expr::Builtin(
                    BuiltinOp::Concat,
                    vec![
                        Expr::Var(Arc::from("a")),
                        Expr::Lit(Literal::Str("!".into())),
                    ],
                ),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Opaque,
            },
            FieldTransform::ComputeField {
                target_key: "d".into(),
                expr: Expr::Var(Arc::from("a")),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
            FieldTransform::PathTransform {
                path: vec!["attrs".into()],
                inner: Box::new(FieldTransform::RenameField {
                    old_key: "k".into(),
                    new_key: "kk".into(),
                }),
            },
            FieldTransform::MapReferences {
                field: "refs".into(),
                rename_map: HashMap::from([("x".to_string(), Some("y".to_string()))]),
            },
            FieldTransform::Case {
                branches: vec![CaseBranch {
                    predicate: Expr::Lit(Literal::Bool(true)),
                    transforms: vec![FieldTransform::AddField {
                        key: "flag".into(),
                        value: Value::Bool(true),
                    }],
                }],
            },
        ];

        let apply = |ft: &FieldTransform| {
            let mut node = fixture();
            let ctx = TransformContext::detached();
            apply_field_transforms(&mut node, std::slice::from_ref(ft), &ctx)
                .expect("transform should evaluate");
            node
        };

        for ft in &variants {
            let direct = apply(ft);

            // Round-trip the transform through the term-assignment algebra
            // and apply the lowered assignment.
            let assignment = TermAssignment::from_field_transform(ft);
            let via_term = apply(&assignment.to_field_transform());

            assert_eq!(
                direct.extra_fields, via_term.extra_fields,
                "term-assignment path must match direct field transform for {ft:?}",
            );

            // The flat-row substitution path agrees for the whole-row cases
            // that carry over to a relational row.
            let mut row = fixture().extra_fields;
            apply_term_assignments_to_row(&mut row, std::slice::from_ref(&assignment))
                .expect("transform should evaluate");
            assert_eq!(
                row, direct.extra_fields,
                "flat-row term substitution must match for {ft:?}",
            );
        }
    }

    // --- Property-based tests ---

    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    mod property {
        use super::*;
        use proptest::prelude::*;

        /// Generate a random schema + instance with N scalar children
        /// under a root object node.
        fn arb_instance_with_scalars()
        -> impl Strategy<Value = (WInstance, HashMap<String, Value>, Vec<String>)> {
            (1..=5usize).prop_flat_map(|n| {
                prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n..=n).prop_flat_map(
                    move |values| {
                        prop::collection::vec("[a-z]{1,6}".prop_map(String::from), n..=n).prop_map(
                            move |names| {
                                let values = values.clone();
                                // Deduplicate names
                                let mut seen = std::collections::HashSet::new();
                                let deduped: Vec<String> = names
                                    .iter()
                                    .map(|name| {
                                        let mut candidate = name.clone();
                                        let mut i = 0;
                                        while seen.contains(&candidate) {
                                            candidate = format!("{name}{i}");
                                            i += 1;
                                        }
                                        seen.insert(candidate.clone());
                                        candidate
                                    })
                                    .collect();

                                let mut nodes = HashMap::new();
                                nodes.insert(0, Node::new(0, "root"));

                                let mut arcs = Vec::new();
                                for (i, (name, val)) in
                                    deduped.iter().zip(values.iter()).enumerate()
                                {
                                    let nid = u32::try_from(i + 1).unwrap();
                                    let anchor = format!("root.{name}");
                                    nodes.insert(
                                        nid,
                                        Node::new(nid, anchor.as_str()).with_value(
                                            FieldPresence::Present(Value::Str(val.clone())),
                                        ),
                                    );
                                    arcs.push((
                                        0,
                                        nid,
                                        Edge {
                                            src: "root".into(),
                                            tgt: Name::from(anchor.as_str()),
                                            kind: "prop".into(),
                                            name: Some(Name::from(name.as_str())),
                                        },
                                    ));
                                }

                                let instance =
                                    WInstance::new(nodes, arcs, vec![], 0, "root".into());
                                let scalars = collect_scalar_child_values(&instance, 0);
                                (instance, scalars, deduped)
                            },
                        )
                    },
                )
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(128))]

            #[test]
            fn prop_child_scalar_collection_complete(
                (_instance, scalars, names) in arb_instance_with_scalars()
            ) {
                // Every child name must appear in the scalar collection.
                for name in &names {
                    prop_assert!(
                        scalars.contains_key(name),
                        "child scalar {name} missing from collection"
                    );
                }
                prop_assert_eq!(
                    scalars.len(), names.len(),
                    "scalar count must match child count"
                );
            }

            #[test]
            #[allow(clippy::expect_used)]
            fn prop_compute_field_reads_any_child(
                (_instance, scalars, names) in arb_instance_with_scalars()
            ) {
                // ComputeField should be able to read any child scalar by name.
                for name in &names {
                    let expr = panproto_expr::Expr::Var(std::sync::Arc::from(name.as_str()));
                    let transform = FieldTransform::ComputeField {
                        target_key: format!("{name}_copy"),
                        expr,
                        inverse: None,
                        coercion_class: panproto_gat::CoercionClass::Projection,
                    };
                    let mut node = Node::new(0, "root");
                    apply_field_transforms(&mut node, &[transform], &TransformContext::from_child_values(scalars.clone())).expect("transform should evaluate");
                    let expected = scalars.get(name);
                    let actual = node.extra_fields.get(&format!("{name}_copy"));
                    prop_assert_eq!(
                        actual, expected,
                        "ComputeField should read child scalar"
                    );
                }
            }

            #[test]
            fn prop_env_monotonicity(
                (_instance, scalars, _names) in arb_instance_with_scalars()
            ) {
                // Adding child_scalars must not remove or change any existing
                // extra_field binding. (Monotonicity of environment extension.)
                let mut extra = HashMap::new();
                extra.insert("sentinel".into(), Value::Str("sentinel_val".into()));

                let env_base = build_env_from_extra_fields(&extra);
                let env_extended = build_env_with_children(&extra, &scalars);

                let var = panproto_expr::Expr::Var(std::sync::Arc::from("sentinel"));
                let config = panproto_expr::EvalConfig::default();
                let base_result = panproto_expr::eval(&var, &env_base, &config).ok();
                let ext_result = panproto_expr::eval(&var, &env_extended, &config).ok();
                prop_assert_eq!(
                    base_result, ext_result,
                    "existing extra_field binding must be preserved"
                );
            }

            #[test]
            fn prop_identity_restrict_preserves_all_values(
                (instance, _scalars, _names) in arb_instance_with_scalars()
            ) {
                // Identity migration with empty field_transforms: passing
                // child_scalars must not corrupt the instance.
                use smallvec::SmallVec;

                let mut vertices = HashMap::new();
                let mut edges_map = HashMap::new();
                let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
                let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
                let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

                for node in instance.nodes.values() {
                    vertices.insert(
                        node.anchor.clone(),
                        panproto_schema::Vertex {
                            id: node.anchor.clone(),
                            kind: if node.value.is_some() { "string".into() } else { "object".into() },
                            nsid: None,
                        },
                    );
                }
                for (p, c, e) in &instance.arcs {
                    let _ = p;
                    let _ = c;
                    edges_map.insert(e.clone(), e.kind.clone());
                    outgoing.entry(e.src.clone()).or_default().push(e.clone());
                    incoming.entry(e.tgt.clone()).or_default().push(e.clone());
                    between.entry((e.src.clone(), e.tgt.clone())).or_default().push(e.clone());
                }

                let schema = panproto_schema::Schema {
                    protocol: "test".into(),
                    vertices,
                    edges: edges_map,
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
                    outgoing,
                    incoming,
                    between,
                    ..Default::default()
                };

                let surviving_verts = schema.vertices.keys().cloned().collect();
                let surviving_edges = schema.edges.keys().cloned().collect();
                let migration = CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms: HashMap::new(),
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                };

                let result = restrict_engine(&instance, &schema, &schema, &migration);
                prop_assert!(result.is_ok(), "identity restrict should succeed");
                let restricted = result.unwrap();
                prop_assert_eq!(
                    restricted.node_count(), instance.node_count(),
                    "identity restrict must preserve node count"
                );
                for (&id, node) in &instance.nodes {
                    let r_node = restricted.nodes.get(&id).unwrap();
                    prop_assert_eq!(&node.anchor, &r_node.anchor);
                    prop_assert_eq!(&node.value, &r_node.value);
                    prop_assert_eq!(&node.extra_fields, &r_node.extra_fields);
                }
            }
        }
    }
}
