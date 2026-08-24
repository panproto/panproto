//! Migration specification type.
//!
//! A [`Migration`] describes a mapping between two schemas: how vertices,
//! edges, hyper-edges, and labels in the source correspond to elements
//! in the target. Resolvers handle ambiguous cases where ancestor
//! contraction produces multiple candidate edges.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};
use serde::{Deserialize, Serialize};

/// A migration specification: maps between two schemas.
///
/// The vertex and edge maps define the core graph morphism. The resolver
/// and hyper-resolver handle contraction ambiguities that arise when
/// intermediate vertices are dropped.
///
/// The optional `domain` and `codomain` carry a schema identifier (a
/// content hash or a protocol-qualified name) for the source and target
/// schemas. When both are present on a composable pair, they let
/// [`compose`](fn@crate::compose) reject a composition whose intermediate
/// schemas do not agree. Both default to `None`, in which case
/// composition is permissive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Migration {
    /// Maps source vertex IDs to target vertex IDs.
    pub vertex_map: HashMap<Name, Name>,
    /// Maps source edges to target edges.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub edge_map: HashMap<Edge, Edge>,
    /// Maps source hyper-edge IDs to target hyper-edge IDs.
    pub hyper_edge_map: HashMap<Name, Name>,
    /// Maps (hyper-edge ID, label) pairs to new labels.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub label_map: HashMap<(Name, Name), Name>,
    /// Binary contraction resolver: `(src_vertex, tgt_vertex)` -> resolved edge.
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub resolver: HashMap<(Name, Name), Edge>,
    /// Hyper-edge contraction resolver: maps `(hyper_edge_id, labels)` to
    /// `(target_hyper_edge_id, label_remap)`.
    #[allow(clippy::type_complexity)]
    #[serde(with = "panproto_schema::serde_helpers::map_as_vec")]
    pub hyper_resolver: HashMap<(Name, Vec<Name>), (Name, HashMap<Name, Name>)>,
    /// Expression-based resolvers for enriched migrations.
    #[serde(default, with = "panproto_schema::serde_helpers::map_as_vec_default")]
    pub expr_resolvers: HashMap<(Name, Name), panproto_expr::Expr>,
    /// The value-level action this migration applies, keyed by source vertex.
    ///
    /// A vertex map that changes a vertex's kind changes the type of the values
    /// stored there, and the coercion recorded here is how those values are
    /// rewritten. It is carried on the migration rather than looked up from the
    /// endpoints at compile time because composition needs it: the composite of
    /// two kind-changing steps runs between schemas that need not know the
    /// coercion the intermediate schema registered, so a composite that did not
    /// carry its steps' actions would emit values typed for the schema it came
    /// from. [`with_coercions`](Self::with_coercions) fills this in from a
    /// source and target schema.
    #[serde(default)]
    pub coercions: HashMap<Name, panproto_schema::CoercionSpec>,
    /// Identifier of the source schema (content hash or protocol-qualified
    /// name). `None` when the migration carries no schema identity.
    #[serde(default)]
    pub domain: Option<Name>,
    /// Identifier of the target schema (content hash or protocol-qualified
    /// name). `None` when the migration carries no schema identity.
    #[serde(default)]
    pub codomain: Option<Name>,
}

impl Migration {
    /// Create an identity migration for the given schema vertex and edge sets.
    ///
    /// Every vertex maps to itself and every edge maps to itself.
    #[must_use]
    pub fn identity(vertices: &[Name], edges: &[Edge]) -> Self {
        let vertex_map: HashMap<Name, Name> =
            vertices.iter().map(|v| (v.clone(), v.clone())).collect();
        let edge_map: HashMap<Edge, Edge> = edges.iter().map(|e| (e.clone(), e.clone())).collect();
        Self {
            vertex_map,
            edge_map,
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        }
    }

    /// Create an identity migration carrying source and target schema
    /// identifiers.
    ///
    /// Like [`identity`](Self::identity), but records `id` as both the
    /// `domain` and `codomain`, since an identity migration maps a schema
    /// to itself.
    #[must_use]
    pub fn identity_for(vertices: &[Name], edges: &[Edge], id: Name) -> Self {
        let mut mig = Self::identity(vertices, edges);
        mig.domain = Some(id.clone());
        mig.codomain = Some(id);
        mig
    }

    /// Create an empty migration (no mappings).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            vertex_map: HashMap::new(),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            coercions: HashMap::new(),
            domain: None,
            codomain: None,
        }
    }

    /// Set the source and target schema identifiers, returning `self`.
    #[must_use]
    pub fn with_endpoints(mut self, domain: Option<Name>, codomain: Option<Name>) -> Self {
        self.domain = domain;
        self.codomain = codomain;
        self
    }

    /// Record the value-level action this migration takes between `src` and
    /// `tgt`, returning `self`.
    ///
    /// Every vertex the map sends to a vertex of a different kind picks up the
    /// coercion `tgt` registers for that kind pair. A vertex whose kind is
    /// unchanged, or whose kind change the target schema declares no coercion
    /// for, records nothing.
    ///
    /// Do this while both schemas are at hand. Composition carries the recorded
    /// actions forward, so a composite of two kind-changing steps still applies
    /// both, and [`compile`](fn@crate::compile) prefers what a migration
    /// carries over what it can infer from the two schemas it is handed.
    #[must_use]
    pub fn with_coercions(mut self, src: &Schema, tgt: &Schema) -> Self {
        self.coercions.clear();
        for (src_v, tgt_v) in &self.vertex_map {
            let (Some(sv), Some(tv)) = (src.vertex(src_v), tgt.vertex(tgt_v)) else {
                continue;
            };
            if sv.kind == tv.kind {
                continue;
            }
            if let Some(spec) = tgt.coercions.get(&(sv.kind.clone(), tv.kind.clone())) {
                self.coercions.insert(src_v.clone(), spec.clone());
            }
        }
        self
    }
}

/// The variable a coercion's term reads the incoming value under.
///
/// A [`CoercionSpec`](panproto_schema::CoercionSpec)'s forward and inverse
/// terms are written against this name, and the compiled migration computes
/// the coerced value by substituting the stored value for it.
pub const COERCION_INPUT: &str = "__value__";

/// Compose two value-level coercions: `second` applied to the result of
/// `first`.
///
/// Both terms read their input under [`COERCION_INPUT`], so the composite is
/// `second`'s term with `first`'s term substituted for that variable. The
/// inverse runs the other way — `first`'s inverse applied to `second`'s — and
/// exists only when both steps have one, since a step with no inverse leaves
/// the composite with no way back. The round-trip class is the composite of the
/// two classes.
#[must_use]
pub fn compose_coercions(
    first: &panproto_schema::CoercionSpec,
    second: &panproto_schema::CoercionSpec,
) -> panproto_schema::CoercionSpec {
    let forward = panproto_expr::substitute(&second.forward, COERCION_INPUT, &first.forward);
    let inverse = match (&first.inverse, &second.inverse) {
        (Some(first_inv), Some(second_inv)) => Some(panproto_expr::substitute(
            first_inv,
            COERCION_INPUT,
            second_inv,
        )),
        _ => None,
    };
    panproto_schema::CoercionSpec {
        forward,
        inverse,
        class: first.class.compose(second.class),
    }
}
