//! Theory document types and all supporting spec types.
//!
//! A [`TheoryDocument`] is the top-level type deserialized from Nickel,
//! JSON, or YAML. It contains exactly one body variant: a theory
//! definition, a morphism, a composition, a protocol, or a bundle
//! of multiple definitions.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::Protocol;
use serde::{Deserialize, Serialize};

/// Top-level theory document. Exactly one body variant must be present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TheoryDocument {
    /// Unique identifier (reverse-DNS, e.g. `"dev.attitudes.conjunction"`).
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Exactly one body variant (discriminated by field presence).
    #[serde(flatten)]
    pub body: TheoryBody,
}

/// Body variant of a theory document.
///
/// Discriminated by the presence of a distinguishing key:
/// `theory`, `morphism`, `compose`, `protocol`, or `bundle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TheoryBody {
    /// Define a single theory (sorts, operations, equations).
    Theory(TheorySpec),
    /// Define a theory morphism between two named theories.
    Morphism(MorphismSpec),
    /// Compose theories via colimit to produce a new theory.
    Composition(CompositionBody),
    /// Define a complete protocol (schema theory + instance theory + edge rules).
    Protocol(Box<ProtocolSpec>),
    /// Bundle: multiple theories, morphisms, and compositions in one file.
    Bundle(Box<BundleSpec>),
    /// Typeclass-style class declaration. Compiles to a theory whose sorts
    /// are the listed `params` and whose operations are the `signatures`.
    Class(ClassSpec),
    /// Typeclass-style instance declaration. Compiles to a theory morphism
    /// from the class theory to the target theory.
    Instance(InstanceSpec),
    /// Inductive-type declaration. Compiles to a theory with one closed
    /// sort and one constructor op per entry.
    Inductive(InductiveSpec),
}

// ═══════════════════════════════════════════════════════════════════
// InductiveSpec
// ═══════════════════════════════════════════════════════════════════

/// Concise inductive-type declaration. Expands to a theory with one
/// closed sort (whose constructor list is `constructors.map(|c| c.name)`)
/// and one operation per constructor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InductiveSpec {
    /// Sort name to introduce.
    pub inductive: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Parameters of the inductive type (for dependent inductive types
    /// like `List<A>`). Each parameter becomes both a sort parameter of
    /// the inductive sort and an argument position on every constructor
    /// whose output sort is the inductive applied to these parameters.
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    /// Constructor declarations.
    pub constructors: Vec<ConstructorSpec>,
}

/// One constructor of an [`InductiveSpec`]. The output sort is implicit:
/// it is the inductive sort applied to the surrounding spec's `params`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructorSpec {
    /// Constructor op name.
    pub name: String,
    /// Constructor inputs. May reference the inductive type itself
    /// (for recursive constructors like `succ`).
    #[serde(default)]
    pub inputs: Vec<ParamSpec>,
}

// ═══════════════════════════════════════════════════════════════════
// ClassSpec and InstanceSpec
// ═══════════════════════════════════════════════════════════════════

/// Typeclass-style class declaration: a theory whose sorts are the named
/// parameters and whose operations are the listed signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassSpec {
    /// Class name, used as the theory name.
    pub class: String,
    /// Sort parameter names (e.g. `["A"]`). Each becomes a simple
    /// structural sort in the compiled theory.
    pub params: Vec<String>,
    /// Operation signatures declared by the class.
    pub signatures: Vec<OpSpec>,
    /// Equational axioms over the class operations.
    #[serde(default)]
    pub axioms: Vec<EquationSpec>,
}

/// Typeclass-style instance declaration.
///
/// An instance desugars to a theory morphism from the class theory to the
/// target theory. The `bindings` map carries both sort-to-sort entries
/// (for each class `param`) and op-to-op entries (for each class
/// signature).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSpec {
    /// Instance name.
    pub instance: String,
    /// The class theory name (morphism domain).
    pub class: String,
    /// The target theory name (morphism codomain).
    pub target: String,
    /// Name bindings from class-side names to target-side names. Entries
    /// whose domain key is one of the class's sort params become the
    /// `sort_map`; remaining entries become the `op_map`.
    pub bindings: HashMap<String, String>,
}

// ═══════════════════════════════════════════════════════════════════
// TheorySpec
// ═══════════════════════════════════════════════════════════════════

/// Single theory definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TheorySpec {
    /// Theory name (used as reference in morphisms and compositions).
    pub theory: String,
    /// Parent theories this theory extends.
    #[serde(default)]
    pub extends: Vec<String>,
    /// Imports from other theories, optionally namespaced.
    #[serde(default)]
    pub imports: Vec<ImportSpec>,
    /// Sort declarations.
    #[serde(default)]
    pub sorts: Vec<SortSpec>,
    /// Operation declarations.
    #[serde(default)]
    pub ops: Vec<OpSpec>,
    /// Undirected equations (judgemental equalities).
    #[serde(default)]
    pub equations: Vec<EquationSpec>,
    /// Directed equations (rewrite rules with implementations).
    #[serde(default)]
    pub directed_equations: Vec<DirectedEqSpec>,
    /// Conflict policies.
    #[serde(default)]
    pub policies: Vec<PolicySpec>,
}

/// Import directive: pull sorts and ops from another theory into this
/// theory's namespace.
///
/// The compiler treats each import as a pushout along the identity
/// morphism from the imported theory, renaming public symbols according
/// to `alias` and `expose`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSpec {
    /// The imported theory's name, as understood by the resolver.
    pub from: String,
    /// Optional namespace alias. When present, imported symbols are
    /// referred to as `Alias.Name` in this theory's sort/op expressions.
    #[serde(default)]
    pub alias: Option<String>,
    /// Names to expose without any alias prefix; each listed symbol can
    /// be referenced as the bare name in this theory.
    #[serde(default)]
    pub expose: Vec<String>,
}

/// Sort declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortSpec {
    /// Sort name.
    pub name: String,
    /// Dependent parameters (e.g. `[{ name: "v", sort: "Vertex" }]`).
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    /// Sort kind (defaults to structural).
    #[serde(default = "default_structural")]
    pub kind: SortKindSpec,
    /// Closure: `None` means open; `Some(constructors)` declares the
    /// sort closed against those constructor op names.
    #[serde(default)]
    pub closed: Option<Vec<String>>,
}

/// Named parameter for dependent sorts and operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Parameter name.
    pub name: String,
    /// Sort this parameter ranges over.
    pub sort: String,
    /// Whether this parameter is implicit (inferred at call sites by
    /// unification against explicit arguments). Defaults to `false`.
    #[serde(default)]
    pub implicit: bool,
}

/// Sort kind classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SortKindSpec {
    /// Standard structural sort.
    #[serde(rename = "structural")]
    Structural,
    /// Value sort carrying data of a specific kind.
    #[serde(rename = "val")]
    Val {
        /// The value kind (e.g. `"string"`, `"integer"`).
        value_kind: String,
    },
    /// Coercion sort between value kinds.
    #[serde(rename = "coercion")]
    Coercion {
        /// Source value kind.
        from: String,
        /// Target value kind.
        to: String,
        /// Coercion class (e.g. `"iso"`, `"retraction"`).
        class: String,
    },
    /// Merger sort for combining values.
    #[serde(rename = "merger")]
    Merger {
        /// The value kind being merged.
        value_kind: String,
    },
}

const fn default_structural() -> SortKindSpec {
    SortKindSpec::Structural
}

/// Operation declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpSpec {
    /// Operation name.
    pub name: String,
    /// For unary operations: shorthand input sort name.
    #[serde(default)]
    pub input: Option<String>,
    /// For multi-arity operations: full signature.
    #[serde(default)]
    pub inputs: Option<Vec<ParamSpec>>,
    /// Output sort name.
    pub output: String,
}

/// Undirected equation (judgemental equality).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationSpec {
    /// Equation name.
    pub name: String,
    /// Left-hand side (term expression string).
    pub lhs: String,
    /// Right-hand side (term expression string).
    pub rhs: String,
}

/// Directed equation (rewrite rule with implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectedEqSpec {
    /// Equation name.
    pub name: String,
    /// Left-hand side (pattern term).
    pub lhs: String,
    /// Right-hand side (rewrite target term).
    pub rhs: String,
    /// Implementation expression (panproto expression language).
    pub impl_expr: String,
    /// Optional inverse expression.
    #[serde(default)]
    pub inverse: Option<String>,
    /// Source value kind for coercion.
    #[serde(default)]
    pub source_kind: Option<String>,
    /// Target value kind for coercion.
    #[serde(default)]
    pub target_kind: Option<String>,
    /// Coercion class (defaults to `"iso"`).
    #[serde(default = "default_iso")]
    pub coercion_class: String,
}

fn default_iso() -> String {
    "iso".to_owned()
}

/// Conflict resolution policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySpec {
    /// Policy name.
    pub name: String,
    /// Value kind this policy applies to.
    pub value_kind: String,
    /// Resolution strategy.
    pub strategy: StrategySpec,
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StrategySpec {
    /// Keep left (ours) value.
    #[serde(rename = "keep_left")]
    KeepLeft,
    /// Keep right (theirs) value.
    #[serde(rename = "keep_right")]
    KeepRight,
    /// Fail on conflict.
    #[serde(rename = "fail")]
    Fail,
    /// Custom resolution expression.
    #[serde(rename = "custom")]
    Custom {
        /// Resolution expression in the panproto expression language.
        expr: String,
    },
}

// ═══════════════════════════════════════════════════════════════════
// MorphismSpec
// ═══════════════════════════════════════════════════════════════════

/// Theory morphism between two named theories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorphismSpec {
    /// Morphism name.
    pub morphism: String,
    /// Domain theory name.
    pub domain: String,
    /// Codomain theory name.
    pub codomain: String,
    /// Sort name mappings: domain sort -> codomain sort.
    pub sort_map: HashMap<String, String>,
    /// Operation name mappings: domain op -> codomain op.
    pub op_map: HashMap<String, String>,
}

// ═══════════════════════════════════════════════════════════════════
// CompositionBody
// ═══════════════════════════════════════════════════════════════════

/// Composition body: wraps a [`CompositionSpec_`] under the `compose` key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionBody {
    /// The composition specification.
    pub compose: CompositionSpec_,
}

/// Composition specification: ordered colimit steps over base theories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionSpec_ {
    /// Name of the resulting composed theory.
    pub result: String,
    /// Base theories to load before composing.
    #[serde(default)]
    pub bases: Vec<String>,
    /// Ordered colimit steps.
    pub steps: Vec<ColimitStepSpec>,
}

/// A single colimit step: identify shared sorts/ops in the pushout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColimitStepSpec {
    /// Left theory name.
    pub left: String,
    /// Right theory name.
    pub right: String,
    /// Sorts to identify in the pushout.
    pub shared_sorts: Vec<String>,
    /// Operations to identify in the pushout.
    #[serde(default)]
    pub shared_ops: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════
// ProtocolSpec
// ═══════════════════════════════════════════════════════════════════

/// Full protocol definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolSpec {
    /// Protocol name.
    pub protocol: String,
    /// Reference to schema theory (by name, inline definition, or inline composition).
    pub schema_theory: TheoryRef,
    /// Reference to instance theory (by name, inline definition, or inline composition).
    pub instance_theory: TheoryRef,
    /// Edge rules mapping edge kinds to source/target sort kinds.
    #[serde(default)]
    pub edge_rules: Vec<EdgeRuleSpec>,
}

/// Reference to a theory: by name, inline definition, or inline composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TheoryRef {
    /// Reference by name to a previously defined or built-in theory.
    Named(String),
    /// Inline theory definition.
    Inline(TheorySpec),
    /// Inline composition.
    Composed(CompositionSpec_),
}

/// Edge rule specification (singular src/tgt kinds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRuleSpec {
    /// Edge kind name.
    pub edge_kind: String,
    /// Source vertex kind.
    pub src_kind: String,
    /// Target vertex kind.
    pub tgt_kind: String,
}

// ═══════════════════════════════════════════════════════════════════
// BundleSpec
// ═══════════════════════════════════════════════════════════════════

/// Bundle: multiple theories, morphisms, compositions, and protocols
/// in a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSpec {
    /// Bundle name.
    pub bundle: String,
    /// Theory definitions.
    #[serde(default)]
    pub theories: Vec<TheorySpec>,
    /// Morphism definitions.
    #[serde(default)]
    pub morphisms: Vec<MorphismSpec>,
    /// Composition specifications.
    #[serde(default)]
    pub compositions: Vec<CompositionSpec_>,
    /// Protocol definitions.
    #[serde(default)]
    pub protocols: Vec<ProtocolSpec>,
}

// ═══════════════════════════════════════════════════════════════════
// Compiled output
// ═══════════════════════════════════════════════════════════════════

/// Result of compiling a [`TheoryDocument`].
pub struct CompiledTheorySet {
    /// Document ID.
    pub id: String,
    /// Compiled theories, keyed by theory name.
    pub theories: HashMap<String, Theory>,
    /// Compiled morphisms, keyed by morphism name.
    pub morphisms: HashMap<String, panproto_gat::TheoryMorphism>,
    /// Compiled protocols, keyed by protocol name.
    pub protocols: HashMap<String, Protocol>,
    /// Composition recipes (for storage alongside results).
    pub composition_specs: HashMap<String, panproto_gat::CompositionSpec>,
}

/// Convenience: display all names in the compiled set.
impl std::fmt::Debug for CompiledTheorySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledTheorySet")
            .field("id", &self.id)
            .field("theories", &self.theories.keys().collect::<Vec<_>>())
            .field("morphisms", &self.morphisms.keys().collect::<Vec<_>>())
            .field("protocols", &self.protocols.keys().collect::<Vec<_>>())
            .field(
                "composition_specs",
                &self.composition_specs.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}
