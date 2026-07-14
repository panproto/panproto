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
    /// Unique lens identifier (reverse-DNS, e.g. `dev.example.repo.db-projection`).
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

    /// Generate the chain from the structural diff of the source and
    /// target schemas (via `diff_to_protolens`). Like `auto`, this body
    /// variant requires schema context and is only compilable through
    /// [`compile_with_schemas`](crate::compile::compile_with_schemas).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_diff: Option<FromDiffSpec>,

    /// Symmetric-lens body: two step pipelines (`left` and `right`)
    /// meeting at a shared middle. Compiles to a pair of protolens
    /// chains that
    /// [`SymmetricLens::from_protolens_chains`](panproto_lens::SymmetricLens::from_protolens_chains)
    /// can assemble at a concrete overlap schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symmetric: Option<SymmetricSpec>,

    // -- Modifiers (accompany a body) --
    /// Directed equations appended to the compiled chain. Each becomes a
    /// `directed_eq` protolens step (an oriented rewrite `lhs → rhs` with
    /// a computable implementation), mapping onto the lens crate's
    /// directed-equation machinery. Typically paired with a `steps` body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directed_equations: Option<Vec<DirectedEquationSpec>>,

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
    #[serde(default, rename = "fallback", alias = "default")]
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
    /// Edge kind stamped on the two new edges (typically `"prop"`).
    pub edge_kind: String,
    /// Label of the original `parent → child` edge to drop. Empty
    /// string means the original edge had no label.
    #[serde(default)]
    pub old_edge_name: String,
    /// Label for the new `parent → intermediate` edge. Defaults to the
    /// intermediate vertex id when empty.
    #[serde(default)]
    pub parent_to_intermediate: String,
    /// Label for the new `intermediate → child` edge. Defaults to the
    /// child vertex id when empty.
    #[serde(default)]
    pub intermediate_to_child: String,
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
    /// Source vertex kind of the coerced values. Determines which sample
    /// inputs the construction-time honesty check draws when verifying the
    /// declared coercion class. Absent means the check draws samples of
    /// every kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// Target vertex kind.
    pub target_kind: String,
    /// Forward coercion expression. The coerced value is bound as the free
    /// variable `v`.
    pub expr: String,
    /// Optional inverse expression. The forward result is bound as the free
    /// variable `v`.
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
    #[serde(default, rename = "fallback", alias = "default")]
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
    #[serde(rename = "pattern", alias = "match")]
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

    /// Hints for guiding the morphism search: anchors, constraints,
    /// and scoring preferences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hints: Option<HintSpec>,
}

// ---------------------------------------------------------------------------
// From-diff
// ---------------------------------------------------------------------------

/// Configuration for the `from_diff` body variant.
///
/// The chain is derived from the structural difference between the
/// source and target schemas supplied to
/// [`compile_with_schemas`](crate::compile::compile_with_schemas): the
/// added/removed vertices and edges, and vertex kind changes, are
/// converted to elementary protolenses via
/// [`diff_to_protolens`](panproto_lens::diff_to_protolens()).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FromDiffSpec {
    /// When `true`, edge kind changes are emitted as renames rather than
    /// drop-then-add. Reserved for future use; the current structural
    /// differ always emits drops and adds.
    #[serde(default)]
    pub rename_edges: bool,
}

// ---------------------------------------------------------------------------
// Symmetric
// ---------------------------------------------------------------------------

/// Configuration for the `symmetric` body variant.
///
/// A symmetric lens is a span `A ←l− M −r→ B`: two directions sharing a
/// middle. The DSL surface holds the two step pipelines; each compiles
/// to a [`ProtolensChain`](panproto_lens::ProtolensChain), and the pair
/// is stored on [`CompiledLens::symmetric`](crate::compile::CompiledLens).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymmetricSpec {
    /// Steps for the left leg (middle → left view).
    pub left: Vec<Step>,
    /// Steps for the right leg (middle → right view).
    pub right: Vec<Step>,
    /// Optional focus vertex for both legs. When empty, the compiler's
    /// `body_vertex` argument is used for both legs.
    #[serde(default)]
    pub focus: String,
}

// ---------------------------------------------------------------------------
// Directed equations
// ---------------------------------------------------------------------------

/// A directed (oriented) equation: an executable rewrite `lhs → rhs`.
///
/// Maps to [`panproto_gat::DirectedEquation`] and, through
/// [`elementary::directed_eq`](panproto_lens::elementary::directed_eq),
/// to a protolens step. Unlike a plain [`EquationSpec`], a directed
/// equation carries a computable forward implementation and, optionally,
/// an inverse for the backward (`put`) direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectedEquationSpec {
    /// Human-readable name.
    pub name: String,
    /// Left-hand side term (pattern), in the term grammar.
    pub lhs: String,
    /// Right-hand side term (rewrite target), in the term grammar.
    pub rhs: String,
    /// The computable forward implementation, as a panproto expression.
    #[serde(rename = "impl", alias = "impl_term")]
    pub impl_term: String,
    /// Optional inverse expression for the backward direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inverse: Option<String>,
    /// Optional source value kind (for value-level coercions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// Optional target value kind (for value-level coercions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<String>,
    /// Round-trip classification. Defaults to [`CoercionKind::Iso`].
    #[serde(default = "default_iso_coercion")]
    pub coercion: CoercionKind,
}

/// Default coercion class for a directed equation: [`CoercionKind::Iso`].
const fn default_iso_coercion() -> CoercionKind {
    CoercionKind::Iso
}

// ---------------------------------------------------------------------------
// Hints
// ---------------------------------------------------------------------------

/// Hint specification for guided auto-lens generation.
///
/// Anchors pin specific source vertices to target vertices before search.
/// Constraints restrict or bias the CSP solver's domain and scoring.
/// A forward-chaining fixpoint loop derives additional anchors from the
/// user-provided ones by propagating along unique edge-name matches.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HintSpec {
    /// Ground facts: source vertex name maps to target vertex name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub anchors: HashMap<String, String>,

    /// Domain restrictions and scoring preferences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,

    /// Stringency tier governing which alignment strategies run and how
    /// permissively the CSP searches. When `None`, the consumer falls
    /// back to its default (typically `balanced`).
    ///
    /// Encoded as one of `"strict" | "balanced" | "lenient" | "exploratory"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stringency: Option<HintStringency>,

    /// Additional alias clusters to merge into the built-in dictionary.
    /// Each cluster is a list of strings that should be treated as
    /// interchangeable field-name synonyms.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alias_clusters: Vec<Vec<String>>,
}

/// Stringency tier expressed in the hint DSL. Mirrors the engine's
/// `panproto_lens::Stringency` so hint files can pin a tier without
/// depending on the lens crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HintStringency {
    /// Kind-exact, edge-name-pruned CSP search; total morphism only.
    Strict,
    /// Adds alias dictionary and tight token-similarity priors.
    Balanced,
    /// Adds span-search and structural priors.
    Lenient,
    /// Adds lossy retraction witnesses and LM-proposed alignments.
    Exploratory,
}

impl HintSpec {
    /// Extract scoring weight overrides from `Prefer` constraints.
    ///
    /// Adjusts the default weights \[0.25, 0.25, 0.3, 0.2\] (name, edge,
    /// property, degree) based on preferences, then normalizes to sum to 1.0.
    /// Returns `None` if no `Prefer` constraints are present.
    #[must_use]
    pub fn scoring_weights(&self) -> Option<[f64; 4]> {
        let prefers: Vec<_> = self
            .constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::Prefer { predicate, weight } => Some((predicate, *weight)),
                _ => None,
            })
            .collect();

        if prefers.is_empty() {
            return None;
        }

        let mut weights = [0.25, 0.25, 0.3, 0.2];
        for (predicate, weight) in &prefers {
            match predicate {
                PreferencePredicate::SameEdgeName => weights[1] = *weight,
                PreferencePredicate::SimilarName { .. } => weights[0] = *weight,
                PreferencePredicate::SameKind => weights[3] = *weight,
            }
        }

        let sum: f64 = weights.iter().sum();
        if sum > 0.0 {
            for w in &mut weights {
                *w /= sum;
            }
        }

        Some(weights)
    }

    /// Extract the name similarity threshold from `SimilarName` preferences.
    ///
    /// If multiple `SimilarName` preferences exist, returns the maximum
    /// threshold (most restrictive).
    #[must_use]
    pub fn name_similarity_threshold(&self) -> Option<f64> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::Prefer {
                    predicate: PreferencePredicate::SimilarName { threshold },
                    ..
                } => Some(*threshold),
                _ => None,
            })
            .reduce(f64::max)
    }

    /// Extract scope constraint pairs as `(source_root, target_root)`.
    #[must_use]
    pub fn scope_pairs(&self) -> Vec<(String, String)> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::Scope { under, targets } => Some((under.clone(), targets.clone())),
                _ => None,
            })
            .collect()
    }

    /// Collect all excluded target vertex names.
    #[must_use]
    pub fn excluded_target_names(&self) -> Vec<String> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::ExcludeTargets { vertices } => Some(vertices.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Collect all excluded source vertex names.
    #[must_use]
    pub fn excluded_source_names(&self) -> Vec<String> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                Constraint::ExcludeSources { vertices } => Some(vertices.iter().cloned()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

/// A constraint that restricts or biases the morphism search.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Constraint {
    /// Children of `under` in source must map to children of `targets`
    /// in the target schema.
    Scope {
        /// Source vertex whose descendants are scoped.
        under: String,
        /// Target vertex whose descendants are the allowed range.
        targets: String,
    },

    /// Exclude specific target vertices from all candidate domains.
    ExcludeTargets {
        /// Target vertex names to exclude.
        vertices: Vec<String>,
    },

    /// Exclude specific source vertices from the search (treat as
    /// "don't care"; they will not appear in the morphism).
    ExcludeSources {
        /// Source vertex names to exclude.
        vertices: Vec<String>,
    },

    /// Soft preference: adjust quality scoring weights for the given
    /// predicate. Weight is in \[0.0, 1.0\]; higher values bias the
    /// score more strongly toward the predicate.
    Prefer {
        /// The scoring predicate to boost.
        predicate: PreferencePredicate,
        /// Weight for this predicate (higher = stronger preference).
        weight: f64,
    },
}

/// Predicate for soft preference constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreferencePredicate {
    /// Prefer mappings where edge names match between source and target.
    SameEdgeName,

    /// Prefer mappings where vertex names are similar (edit distance
    /// below threshold, normalized to \[0.0, 1.0\]).
    SimilarName {
        /// Minimum normalized similarity (0.0 to 1.0).
        threshold: f64,
    },

    /// Prefer mappings that preserve vertex kind. (Kinds must already
    /// match for validity; this boosts the quality score.)
    SameKind,
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
