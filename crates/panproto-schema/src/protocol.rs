//! Protocol definition.
//!
//! A [`Protocol`] identifies which schema theory and instance theory a
//! particular data format uses, together with well-formedness rules
//! for edges and the set of recognized vertex/constraint kinds.

use serde::{Deserialize, Serialize};

/// A well-formedness rule for edges of a given kind.
///
/// When `src_kinds` is non-empty, only vertices whose kind appears in the
/// list may serve as the source of an edge of this kind. An empty list
/// means any vertex kind is allowed. The same applies to `tgt_kinds`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeRule {
    /// The edge kind this rule governs (e.g., `"prop"`, `"record-schema"`).
    pub edge_kind: String,
    /// Permitted source vertex kinds (empty = any).
    pub src_kinds: Vec<String>,
    /// Permitted target vertex kinds (empty = any).
    pub tgt_kinds: Vec<String>,
}

/// Identifies the schema and instance theories for a data-format protocol,
/// together with structural well-formedness rules.
///
/// Protocols are the Level-1 configuration objects that drive schema
/// construction and validation. Each protocol names a schema theory GAT
/// and an instance theory GAT (both defined in `panproto-protocols`),
/// and supplies edge rules, recognized vertex kinds, and constraint sorts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Protocol {
    /// Human-readable protocol name (e.g., `"atproto"`, `"sql"`).
    pub name: String,
    /// Name of the schema theory GAT in the theory registry.
    pub schema_theory: String,
    /// Name of the instance theory GAT in the theory registry.
    pub instance_theory: String,
    /// Well-formedness rules for each edge kind.
    pub edge_rules: Vec<EdgeRule>,
    /// Vertex kinds that are considered "object-like" (containers).
    pub obj_kinds: Vec<String>,
    /// Recognized constraint sorts (e.g., `"maxLength"`, `"format"`).
    pub constraint_sorts: Vec<String>,

    // -- structural feature flags (all default to false) --
    /// Whether this protocol uses ordered collections (`ThOrder`).
    #[serde(default)]
    pub has_order: bool,
    /// Whether this protocol has coproduct/union types (`ThCoproduct`).
    #[serde(default)]
    pub has_coproducts: bool,
    /// Whether this protocol supports recursive types (`ThRecursion`).
    #[serde(default)]
    pub has_recursion: bool,
    /// Whether this protocol has causal/temporal ordering (`ThCausal`).
    #[serde(default)]
    pub has_causal: bool,
    /// Whether this protocol uses nominal identity (`ThNominal`).
    #[serde(default)]
    pub nominal_identity: bool,

    // -- enrichment feature flags (all default to false) --
    /// Whether this protocol supports default value expressions (`ThValued`).
    #[serde(default)]
    pub has_defaults: bool,
    /// Whether this protocol supports type coercion expressions (`ThCoercible`).
    #[serde(default)]
    pub has_coercions: bool,
    /// Whether this protocol supports merge/split expressions (`ThMergeable`).
    #[serde(default)]
    pub has_mergers: bool,
    /// Whether this protocol supports conflict resolution policies (`ThPolicied`).
    #[serde(default)]
    pub has_policies: bool,
}

impl Protocol {
    /// Returns the [`EdgeRule`] for the given edge kind, if one exists.
    #[must_use]
    pub fn find_edge_rule(&self, edge_kind: &str) -> Option<&EdgeRule> {
        self.edge_rules.iter().find(|r| r.edge_kind == edge_kind)
    }

    /// Returns `true` if `kind` is a recognized vertex kind in this protocol.
    ///
    /// The set of recognized kinds is the union of all kinds mentioned in
    /// edge rules (both source and target) plus `obj_kinds`.
    #[must_use]
    pub fn is_known_vertex_kind(&self, kind: &str) -> bool {
        if self.obj_kinds.iter().any(|k| k == kind) {
            return true;
        }
        for rule in &self.edge_rules {
            if rule.src_kinds.iter().any(|k| k == kind) {
                return true;
            }
            if rule.tgt_kinds.iter().any(|k| k == kind) {
                return true;
            }
        }
        false
    }
}
