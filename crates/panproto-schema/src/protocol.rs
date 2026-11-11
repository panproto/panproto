//! Protocol definition.
//!
//! A [`Protocol`] identifies which schema theory and instance theory a
//! particular data format uses, together with well-formedness rules
//! for edges and the set of recognized vertex/constraint kinds.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::CompositionSpec;
use serde::{Deserialize, Serialize};

use crate::constraint_semantics::ConstraintSemantics;
use crate::shape_kind::InstanceShapeKind;
use crate::value_kinds::ScalarKind;

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
    /// Composition recipe that produced the schema theory.
    pub schema_composition: Option<CompositionSpec>,
    /// Composition recipe that produced the instance theory.
    pub instance_composition: Option<CompositionSpec>,
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

    // -- shape and value declarations (all default to empty) --
    //
    // The built-in protocols of `panproto-protocols` fill these four fields
    // from its per-protocol tables as each `protocol()` constructor builds
    // its `Protocol`. Each is `#[serde(default)]`, so a protocol serialized
    // without them decodes with the defaults.
    /// The shape of the instances this protocol's instance codec produces:
    /// the shape its syntax has.
    ///
    /// Nested text formats are trees, delimited and columnar formats are
    /// tables, and native graph formats (triples, edge lists) are graphs.
    /// A plain parse of the protocol's data returns an instance of this
    /// shape, and the codec's native representation converts to it.
    /// Defaults to [`InstanceShapeKind::Tree`].
    #[serde(default)]
    pub syntactic_shape: InstanceShapeKind,
    /// The shape of this protocol's instances once the references its
    /// schemas declare are resolved.
    ///
    /// It differs from [`syntactic_shape`](Self::syntactic_shape) exactly
    /// when references carry structure the syntax does not: a tree format
    /// whose identifiers and references encode a graph declares `Tree` and
    /// `Graph`. Defaults to [`InstanceShapeKind::Tree`].
    #[serde(default)]
    pub semantic_shape: InstanceShapeKind,
    /// The meaning of each constraint sort, keyed by sort name.
    ///
    /// Covers the entries of [`constraint_sorts`](Self::constraint_sorts):
    /// validation reads a constraint through the meaning of its sort (for
    /// instance whether `maxLength` counts UTF-8 bytes or characters), and
    /// a sort absent from this map is a declaration validation cannot
    /// interpret. [`ConstraintSemantics::parse_value`] reads a constraint's
    /// value under its meaning. Defaults to empty.
    #[serde(default)]
    pub constraint_semantics: BTreeMap<String, ConstraintSemantics>,
    /// The scalar kind of each scalar vertex kind, keyed by vertex kind.
    ///
    /// Codecs coerce a value at a vertex to the kind its vertex kind maps
    /// to here, refined by the vertex's value-parameter constraints through
    /// [`ValueSpec::for_vertex`](crate::ValueSpec::for_vertex); a vertex
    /// kind absent from this map holds [`ScalarKind::Any`]. Defaults to
    /// empty.
    #[serde(default)]
    pub value_kinds: BTreeMap<String, ScalarKind>,
    /// The vertex kinds whose vertices hold no value of their own: records,
    /// lists of records and declarations, as opposed to the value kinds of
    /// [`value_kinds`](Self::value_kinds).
    ///
    /// Together the two classify every vertex kind of a table protocol: a
    /// vertex of a kind listed here is a table of its schema's
    /// relation-ified form, and a vertex of a value kind is an attribute
    /// (a column of the table it hangs from). A kind is in at most one of the
    /// two. Defaults to empty, so a protocol serialized without the field
    /// decodes with none.
    #[serde(default)]
    pub non_value_kinds: BTreeSet<String>,
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
