//! Serde-compatible types for the lens DSL document format.
//!
//! These types represent the intermediate form between Nickel/JSON/YAML
//! surface syntax and the compiled panproto lens algebra. Nickel evaluates
//! to a record, which is deserialized into [`LensDocument`] via `to_serde`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A lens document: the top-level unit of the DSL.
///
/// Exactly one body variant (`steps`, `rules`, `compose`, or `auto`) must
/// be present. The Nickel contract library validates this at evaluation
/// time; the compiler checks again at compile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LensDocument {
    /// Unique lens identifier (reverse-DNS, e.g. `dev.cospan.repo.db-projection`).
    pub id: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,

    /// Source schema or theory NSID.
    pub source: String,

    /// Target schema or theory NSID.
    pub target: String,

    // -- Body variants (exactly one present) --
    /// Pipeline of sequential lens steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<Step>>,

    /// Pattern-match rewrite rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,

    /// Composition of named lens references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose: Option<ComposeSpec>,

    /// Auto-generation configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<AutoSpec>,

    // -- Rule-specific metadata --
    /// Behavior for features not matched by any rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<Passthrough>,

    /// Whether the lens is invertible (rules variant).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invertible: Option<bool>,

    // -- Extensions --
    /// Protocol-specific extension metadata (opaque to the core compiler).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

/// A single step in a lens pipeline.
///
/// Each variant is a tagged single-key object:
/// ```json
/// { "remove_field": "node" }
/// { "rename_field": { "old": "x", "new": "y" } }
/// ```
///
/// Uses `#[serde(untagged)]` so the deserializer tries each variant in order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Step {
    // -- High-level field combinators --
    /// Remove a field (drop a sort and its incoming edges).
    RemoveField {
        /// Vertex ID of the field to remove.
        remove_field: String,
    },

    /// Rename a field's JSON property key.
    RenameField {
        /// Rename specification.
        rename_field: RenameSpec,
    },

    /// Add a field with a default value and optional computed expression.
    AddField {
        /// Add-field specification.
        add_field: AddFieldSpec,
    },

    // -- Value-level transforms --
    /// Apply an expression to an existing field's value.
    ApplyExpr {
        /// Apply-expression specification.
        apply_expr: ApplyExprSpec,
    },

    /// Compute a new field value from an expression over the parent fiber.
    ComputeField {
        /// Compute-field specification.
        compute_field: ComputeFieldSpec,
    },

    // -- Structural combinators --
    /// Hoist a nested field up one level.
    HoistField {
        /// Hoist specification.
        hoist_field: HoistSpec,
    },

    /// Nest a direct child under a new intermediate vertex.
    NestField {
        /// Nest specification.
        nest_field: NestSpec,
    },

    /// Apply an inner pipeline to each element of an array (scoped traversal).
    Scoped {
        /// Scoped-transform specification.
        scoped: ScopedSpec,
    },

    /// Pullback along a theory morphism.
    Pullback {
        /// Pullback specification.
        pullback: PullbackSpec,
    },

    // -- Sort-level coercions and merges --
    /// Coerce a sort's value kind with round-trip classification.
    CoerceSort {
        /// Coerce specification.
        coerce_sort: CoerceSortSpec,
    },

    /// Merge two sorts into one via an expression.
    MergeSorts {
        /// Merge specification.
        merge_sorts: MergeSortsSpec,
    },

    // -- Elementary theory operations --
    /// Add a sort (vertex kind) to the theory.
    AddSort {
        /// Add-sort specification.
        add_sort: AddSortSpec,
    },

    /// Drop a sort from the theory.
    DropSort {
        /// Name of the sort to drop.
        drop_sort: String,
    },

    /// Rename a sort.
    RenameSort {
        /// Rename specification.
        rename_sort: RenameSpec,
    },

    /// Add an operation (edge) to the theory.
    AddOp {
        /// Add-operation specification.
        add_op: AddOpSpec,
    },

    /// Drop an operation from the theory.
    DropOp {
        /// Name of the operation to drop.
        drop_op: String,
    },

    /// Rename an operation.
    RenameOp {
        /// Rename specification.
        rename_op: RenameSpec,
    },

    /// Add an equation (constraint) to the theory.
    AddEquation {
        /// Equation specification.
        add_equation: EquationSpec,
    },

    /// Drop an equation from the theory.
    DropEquation {
        /// Name of the equation to drop.
        drop_equation: String,
    },
}

// ---------------------------------------------------------------------------
// Step spec types
// ---------------------------------------------------------------------------

/// Rename specification: old name to new name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameSpec {
    /// The current name.
    pub old: String,
    /// The new name.
    pub new: String,
}

/// Add-field specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddFieldSpec {
    /// Field name to add.
    pub name: String,
    /// Vertex kind (e.g. `"string"`, `"integer"`, `"boolean"`, `"object"`).
    pub kind: String,
    /// Default value for the field.
    #[serde(default)]
    pub default: serde_json::Value,
    /// Optional expression to compute the field value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
}

/// Apply-expression specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyExprSpec {
    /// The field whose value is transformed.
    pub field: String,
    /// The expression to evaluate.
    pub expr: String,
    /// Optional inverse expression for round-tripping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
    /// Round-trip classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coercion: Option<CoercionKind>,
}

/// Compute-field specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeFieldSpec {
    /// Target field name for the computed value.
    pub target: String,
    /// The expression to evaluate.
    pub expr: String,
    /// Optional inverse expression for round-tripping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
    /// Round-trip classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coercion: Option<CoercionKind>,
}

/// Hoist-field specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoistSpec {
    /// Parent vertex.
    pub parent: String,
    /// Intermediate vertex to collapse.
    pub intermediate: String,
    /// Child vertex to hoist.
    pub child: String,
}

/// Nest-field specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NestSpec {
    /// Parent vertex.
    pub parent: String,
    /// Child vertex to nest.
    pub child: String,
    /// New intermediate vertex name.
    pub intermediate: String,
    /// Kind of the new intermediate vertex.
    pub intermediate_kind: String,
    /// Edge kind to remove (the original direct edge from parent to child).
    pub edge_kind: String,
}

/// Scoped-transform specification (recursive).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedSpec {
    /// Focus vertex (the array element schema vertex).
    pub focus: String,
    /// Inner steps applied to each element.
    pub inner: Vec<Step>,
}

/// Pullback specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullbackSpec {
    /// Morphism name.
    pub name: String,
    /// Domain theory name.
    pub domain: String,
    /// Codomain theory name.
    pub codomain: String,
    /// Sort mapping: domain sort → codomain sort.
    #[serde(default)]
    pub sort_map: HashMap<String, String>,
    /// Operation mapping: domain op → codomain op.
    #[serde(default)]
    pub op_map: HashMap<String, String>,
}

/// Coerce-sort specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoerceSortSpec {
    /// Sort to coerce.
    pub sort: String,
    /// Target vertex kind.
    pub target_kind: String,
    /// Forward coercion expression.
    pub expr: String,
    /// Optional inverse expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
    /// Round-trip classification.
    pub coercion: CoercionKind,
}

/// Merge-sorts specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSortsSpec {
    /// First sort to merge.
    pub sort_a: String,
    /// Second sort to merge.
    pub sort_b: String,
    /// Name of the merged result sort.
    pub merged: String,
    /// Merger expression.
    pub expr: String,
}

/// Add-sort specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSortSpec {
    /// Sort name.
    pub name: String,
    /// Vertex kind.
    pub kind: String,
    /// Default value.
    #[serde(default)]
    pub default: serde_json::Value,
}

/// Add-operation specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOpSpec {
    /// Operation name.
    pub name: String,
    /// Source sort.
    pub src: String,
    /// Target sort.
    pub tgt: String,
    /// Edge kind.
    pub kind: String,
}

/// Equation specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationSpec {
    /// Equation name.
    pub name: String,
    /// Left-hand side term (as string).
    pub lhs: String,
    /// Right-hand side term (as string).
    pub rhs: String,
}

/// Round-trip coercion classification (DSL surface form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoercionKind {
    /// Isomorphism: both round-trip laws hold.
    Iso,
    /// Forward map preserves information (left inverse exists).
    Retraction,
    /// Deterministic derivation, no inverse.
    Projection,
    /// No structural relationship.
    Opaque,
}

impl CoercionKind {
    /// Convert to the GAT-level [`panproto_gat::CoercionClass`].
    #[must_use]
    pub const fn to_coercion_class(self) -> panproto_gat::CoercionClass {
        match self {
            Self::Iso => panproto_gat::CoercionClass::Iso,
            Self::Retraction => panproto_gat::CoercionClass::Retraction,
            Self::Projection => panproto_gat::CoercionClass::Projection,
            Self::Opaque => panproto_gat::CoercionClass::Opaque,
        }
    }
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/// A pattern-match rewrite rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// The pattern to match.
    #[serde(rename = "match")]
    pub match_: FeaturePattern,

    /// The replacement, or `None` to drop the matched feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<Replacement>,
}

/// Pattern for matching features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturePattern {
    /// Match by feature name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Match by `$type` string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
}

/// Replacement descriptor for a matched feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    /// New name (string literal or `{ "template": "h{level}" }`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<ReplacementName>,

    /// Rename attribute keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_attrs: Option<HashMap<String, String>>,

    /// Inject constant attributes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_attrs: Option<HashMap<String, serde_json::Value>>,

    /// Remove these attribute keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop_attrs: Option<Vec<String>>,

    /// Whitelist: keep only these attribute keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_attrs: Option<Vec<String>>,

    /// Transform attribute values by key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_attr_value: Option<HashMap<String, serde_json::Value>>,
}

/// A replacement name: either a literal string or a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReplacementName {
    /// A literal string replacement.
    Literal(String),
    /// A template with placeholders (e.g. `"h{level}"`).
    Template {
        /// The template string.
        template: String,
    },
}

// ---------------------------------------------------------------------------
// Compose
// ---------------------------------------------------------------------------

/// Composition specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeSpec {
    /// Composition mode.
    pub mode: ComposeMode,
    /// Ordered list of lenses to compose.
    pub lenses: Vec<LensRef>,
}

/// Composition mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposeMode {
    /// Sequential: target of each lens feeds into source of the next.
    Vertical,
    /// Parallel: endofunctors composed via horizontal composition.
    Horizontal,
}

/// A reference to a lens within a composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LensRef {
    /// Reference to another lens document by ID.
    Ref {
        /// The lens document ID.
        r#ref: String,
    },
    /// Inline lens definition.
    Inline {
        /// Inline lens with steps.
        inline: InlineLens,
    },
}

/// An inline lens definition within a composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineLens {
    /// Pipeline steps.
    pub steps: Vec<Step>,
}

// ---------------------------------------------------------------------------
// Auto
// ---------------------------------------------------------------------------

/// Auto-generation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSpec {
    /// Minimum alignment quality threshold (0.0 to 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_threshold: Option<f64>,

    /// Whether to try overlap-based alignment as fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_overlap: Option<bool>,

    /// Maximum search depth for morphism discovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_search_depth: Option<usize>,
}

// ---------------------------------------------------------------------------
// Passthrough
// ---------------------------------------------------------------------------

/// Behavior for unmatched features in rule-based lenses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Passthrough {
    /// Keep unmatched features unchanged.
    Keep,
    /// Drop unmatched features.
    Drop,
}
