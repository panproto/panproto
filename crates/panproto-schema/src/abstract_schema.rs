//! Typed newtypes for the abstract / decorated schema distinction.
//!
//! A bare [`Schema`] can be in either of two states: it is *abstract*
//! when no constraint sort belongs to the layout enrichment fibre, and
//! it is *decorated* when the parser walker has attached layout
//! witnesses (byte spans, interstitials, CHOICE discriminators).
//!
//! These newtypes lift that distinction to a Rust type so that the
//! parse/decorate/emit lens can be wired through the type system:
//! `decorate` consumes an [`AbstractSchema`] and returns a
//! [`DecoratedSchema`]; the operational `emit_pretty` and `decorate`
//! entry points keep abstract and decorated inputs distinguishable
//! at every call site without `Deref` erasure.
//!
//! ## Construction
//!
//! - [`AbstractSchema::from_layout_free`] validates that no
//!   layout-fibre constraint is present (returns
//!   [`LayoutConstraintsPresent`] when the invariant fails); this is
//!   the checked entry that callers should prefer.
//! - [`AbstractSchema::from_layout_free_unchecked`] skips the scan
//!   for callers that just ran `forget_layout` themselves.
//! - [`DecoratedSchema::wrap_unchecked`] wraps a [`Schema`] without
//!   checking the layout fibre. The legitimate sources are the
//!   parse walker's output and the `decorate` synthesis driver;
//!   misuse degrades emit correctness silently.
//!
//! Construction is *not* sealed at the type system level
//! (panproto's `Schema` does not yet carry a phantom theory parameter
//! that would let us refuse arbitrary cross-crate constructions).
//! The checked / unchecked split is the load-bearing safety net.

use std::collections::HashMap;

use panproto_gat::Name;

use crate::Schema;
use crate::schema::Constraint;

/// Returned by [`AbstractSchema::from_layout_free`] when the input
/// schema carries constraints in the layout enrichment fibre and
/// therefore cannot be treated as abstract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot construct AbstractSchema: {count} layout-fibre constraint(s) present; \
     call Schema::forget_layout first"
)]
pub struct LayoutConstraintsPresent {
    /// Number of offending constraint entries detected.
    pub count: usize,
}

/// A schema with no layout enrichment.
///
/// Carrying only vertex kinds, edges, and content-level constraints
/// (`literal-value`, `field:*`, and any protocol-defined constraint
/// sorts that are *not* in the layout fibre). Constructed exclusively
/// via [`SchemaBuilder::build_abstract`](crate::SchemaBuilder::build_abstract)
/// or by stripping a [`DecoratedSchema`] through
/// [`DecoratedSchema::forget_layout`].
#[derive(Clone, Debug)]
pub struct AbstractSchema {
    inner: Schema,
}

/// A schema carrying a complete layout enrichment over its abstract
/// content.
///
/// Constructed exclusively by `ParserRegistry::parse_with_protocol`
/// (the get-direction of the parse/emit lens) or by `decorate` (its
/// put-direction). Direct serialization round-trips a `Schema`; the
/// newtype is enforced only at the Rust type level.
#[derive(Clone, Debug)]
pub struct DecoratedSchema {
    inner: Schema,
}

/// Per-vertex view of the layout witness data carried by a
/// [`DecoratedSchema`].
///
/// This is a read-only projection: it borrows the underlying
/// constraint list so callers can inspect a vertex's byte span,
/// interstitial text, or chosen CHOICE alternative without round-
/// tripping through the schema-level constraint maps.
#[derive(Clone, Copy, Debug)]
pub struct LayoutWitness<'a> {
    constraints: &'a [Constraint],
}

impl AbstractSchema {
    /// Construct an [`AbstractSchema`] from a [`Schema`] that already
    /// satisfies the no-layout invariant.
    ///
    /// The invariant is checked at runtime in every build (debug and
    /// release): a non-layout-free schema is a programming error in
    /// the caller, but a load-bearing one — emit and parse use the
    /// type-level distinction to dispatch, and a silently-wrong
    /// `AbstractSchema` would corrupt downstream behaviour. Returns
    /// `Err(LayoutConstraintsPresent { count })` carrying the number
    /// of offending constraint entries so callers can diagnose.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutConstraintsPresent`] when `schema.is_layout_free()`
    /// returns `false`. Use [`Schema::forget_layout`] first if a
    /// decorated schema needs to be downcast.
    pub fn from_layout_free(schema: Schema) -> Result<Self, LayoutConstraintsPresent> {
        let offending = schema
            .constraints
            .values()
            .flat_map(|cs| cs.iter())
            .filter(|c| panproto_gat::is_layout_sort(c.sort.as_ref()))
            .count();
        if offending == 0 {
            Ok(Self { inner: schema })
        } else {
            Err(LayoutConstraintsPresent { count: offending })
        }
    }

    /// Construct an [`AbstractSchema`] from a [`Schema`] without
    /// checking the layout-free invariant.
    ///
    /// Reserved for callers that have *just* run `forget_layout` on
    /// the input and want to skip the redundant scan. Misuse degrades
    /// emit/decorate correctness silently; prefer
    /// [`from_layout_free`](Self::from_layout_free) elsewhere.
    #[must_use]
    pub const fn from_layout_free_unchecked(schema: Schema) -> Self {
        Self { inner: schema }
    }

    /// Borrow the underlying schema for read-only consumption.
    ///
    /// This is the audited bridge to the raw [`Schema`] type: it is
    /// explicit at every call site that we are crossing the typed
    /// boundary in the get-only direction. There is no
    /// `Deref<Target = Schema>` because that would silently erase the
    /// type-level distinction; every consumer must opt in.
    #[must_use]
    pub const fn as_schema(&self) -> &Schema {
        &self.inner
    }

    /// Returns the schema's protocol name.
    #[must_use]
    pub fn protocol(&self) -> &str {
        &self.inner.protocol
    }

    /// Returns the number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.inner.vertex_count()
    }
}

impl DecoratedSchema {
    /// Wrap a [`Schema`] as a [`DecoratedSchema`] without checking the
    /// layout-fibre invariant.
    ///
    /// Construction is *not* enforced at the type level (panproto's
    /// `Schema` does not yet carry a phantom theory parameter), so
    /// this constructor trusts the caller. The legitimate sources are:
    ///
    /// - Output of [`ParserRegistry::parse_with_protocol`](https://docs.rs/panproto-parse) —
    ///   the parse walker attaches a complete layout fibre.
    /// - Output of [`ParserRegistry::decorate`](https://docs.rs/panproto-parse) —
    ///   the put-direction of the parse/emit lens.
    ///
    /// Wrapping a hand-built or otherwise abstract schema produces a
    /// `DecoratedSchema` that subsequent `emit_pretty` calls will
    /// fall back to grammar-walking on (since the layout fibre is
    /// empty), which is well-defined but loses the "round-trips via
    /// byte-position arithmetic" advantage of true decoration.
    #[must_use]
    pub const fn wrap_unchecked(schema: Schema) -> Self {
        Self { inner: schema }
    }

    /// Deprecated alias for [`wrap_unchecked`](Self::wrap_unchecked).
    /// The name `from_schema` understated the operation's
    /// preconditions; use the explicit name at every call site.
    #[must_use]
    #[deprecated(
        since = "0.48.0",
        note = "renamed to `wrap_unchecked` to reflect that it does not validate the layout-fibre invariant"
    )]
    pub const fn from_schema(schema: Schema) -> Self {
        Self::wrap_unchecked(schema)
    }

    /// Borrow the underlying schema for read-only consumption.
    ///
    /// See [`AbstractSchema::as_schema`] for the rationale: this is an
    /// explicit, audited bridge to the raw type, intentionally
    /// non-`Deref`.
    #[must_use]
    pub const fn as_schema(&self) -> &Schema {
        &self.inner
    }

    /// Returns the schema's protocol name.
    #[must_use]
    pub fn protocol(&self) -> &str {
        &self.inner.protocol
    }

    /// Project to the abstract schema by forgetting all layout-fibre
    /// constraints. This is the lens get-direction realised in types.
    ///
    /// Cannot fail: `Schema::forget_layout` always returns a
    /// layout-free schema, so the invariant of [`AbstractSchema`] is
    /// satisfied by construction.
    #[must_use]
    pub fn forget_layout(&self) -> AbstractSchema {
        AbstractSchema::from_layout_free_unchecked(self.inner.forget_layout())
    }

    /// Returns a read-only view of the layout witness at `vertex_id`,
    /// or `None` if the vertex carries no layout constraints (which
    /// implies the schema was synthesised by `decorate` on a vertex
    /// outside the policy's coverage; this is a bug, not user input).
    #[must_use]
    pub fn layout_witness(&self, vertex_id: &str) -> Option<LayoutWitness<'_>> {
        let cs = self.inner.constraints.get(vertex_id)?;
        Some(LayoutWitness { constraints: cs })
    }

    /// Returns the per-vertex layout-fibre witness map.
    ///
    /// The returned map contains exactly the constraints whose sort
    /// satisfies [`panproto_gat::is_layout_sort`], grouped by vertex
    /// id. This is the snapshot the lens complement stores when the
    /// forgetful U strips layout from a decorated schema.
    #[must_use]
    pub fn layout_constraint_map(&self) -> HashMap<Name, Vec<Constraint>> {
        let mut map: HashMap<Name, Vec<Constraint>> = HashMap::new();
        for (vid, cs) in &self.inner.constraints {
            let kept: Vec<Constraint> = cs
                .iter()
                .filter(|c| panproto_gat::is_layout_sort(c.sort.as_ref()))
                .cloned()
                .collect();
            if !kept.is_empty() {
                map.insert(vid.clone(), kept);
            }
        }
        map
    }
}

impl<'a> LayoutWitness<'a> {
    /// Iterate over every layout-fibre constraint at this vertex.
    pub fn iter(&self) -> impl Iterator<Item = &'a Constraint> + '_ {
        self.constraints
            .iter()
            .filter(|c| panproto_gat::is_layout_sort(c.sort.as_ref()))
    }

    /// Return the value of the `start-byte` constraint, if present.
    #[must_use]
    pub fn start_byte(&self) -> Option<usize> {
        self.constraints
            .iter()
            .find(|c| c.sort.as_ref() == "start-byte")
            .and_then(|c| c.value.parse().ok())
    }

    /// Return the value of the `end-byte` constraint, if present.
    #[must_use]
    pub fn end_byte(&self) -> Option<usize> {
        self.constraints
            .iter()
            .find(|c| c.sort.as_ref() == "end-byte")
            .and_then(|c| c.value.parse().ok())
    }

    /// Return the `chose-alt-fingerprint` value, if recorded.
    #[must_use]
    pub fn chose_alt_fingerprint(&self) -> Option<&'a str> {
        self.constraints
            .iter()
            .find(|c| c.sort.as_ref() == "chose-alt-fingerprint")
            .map(|c| c.value.as_str())
    }

    /// Return the `chose-alt-child-kinds` value, if recorded.
    #[must_use]
    pub fn chose_alt_child_kinds(&self) -> Option<&'a str> {
        self.constraints
            .iter()
            .find(|c| c.sort.as_ref() == "chose-alt-child-kinds")
            .map(|c| c.value.as_str())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{EdgeRule, Protocol, SchemaBuilder};

    fn empty_protocol() -> Protocol {
        Protocol {
            name: "test".to_owned(),
            schema_theory: "ThTest".to_owned(),
            instance_theory: "ThWType".to_owned(),
            edge_rules: vec![EdgeRule {
                edge_kind: "child_of".to_owned(),
                src_kinds: vec!["node".to_owned()],
                tgt_kinds: vec!["node".to_owned()],
            }],
            obj_kinds: vec!["node".to_owned()],
            ..Default::default()
        }
    }

    #[test]
    fn forget_layout_strips_layout_sorts_only() {
        let p = empty_protocol();
        let schema = SchemaBuilder::new(&p)
            .vertex("v0", "node", None)
            .unwrap()
            .constraint("v0", "start-byte", "10")
            .constraint("v0", "end-byte", "20")
            .constraint("v0", "literal-value", "hi")
            .build()
            .unwrap();

        let stripped = schema.forget_layout();
        let cs = stripped.constraints.get(&Name::from("v0")).unwrap();
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].sort.as_ref(), "literal-value");
        assert!(stripped.is_layout_free());
    }

    #[test]
    fn forget_layout_is_idempotent() {
        let p = empty_protocol();
        let schema = SchemaBuilder::new(&p)
            .vertex("v0", "node", None)
            .unwrap()
            .constraint("v0", "interstitial-0", " ")
            .constraint("v0", "chose-alt-fingerprint", "{ }")
            .build()
            .unwrap();
        let once = schema.forget_layout();
        let twice = once.forget_layout();
        assert_eq!(once.constraints, twice.constraints);
        assert!(twice.is_layout_free());
    }

    #[test]
    fn decorated_layout_witness_round_trips_byte_span() {
        let p = empty_protocol();
        let schema = SchemaBuilder::new(&p)
            .vertex("v0", "node", None)
            .unwrap()
            .constraint("v0", "start-byte", "3")
            .constraint("v0", "end-byte", "7")
            .build()
            .unwrap();
        let decorated = DecoratedSchema::wrap_unchecked(schema);
        let w = decorated.layout_witness("v0").unwrap();
        assert_eq!(w.start_byte(), Some(3));
        assert_eq!(w.end_byte(), Some(7));
    }
}
