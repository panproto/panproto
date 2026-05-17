//! Sealed typed newtypes for the abstract / decorated schema distinction.
//!
//! A bare [`Schema`] can be in either of two states: it is *abstract*
//! when no constraint sort belongs to the layout enrichment fibre, and
//! it is *decorated* when the parser walker has attached layout
//! witnesses (byte spans, interstitials, CHOICE discriminators).
//!
//! These newtypes turn that distinction into a type-level invariant so
//! that the parse/decorate/emit lens can be wired through the type
//! system: `emit_pretty` accepts only [`DecoratedSchema`]; `decorate`
//! consumes [`AbstractSchema`]; the two are bridged exclusively via
//! the lens. There is no `From<Schema>` escape hatch.
//!
//! Construction is sealed: only `panproto-schema` itself (via the
//! `SchemaBuilder`) and `panproto-parse` (via the parser registry and
//! the layout-enrichment driver) may construct values of these types.

use std::collections::HashMap;

use panproto_gat::Name;

use crate::Schema;
use crate::schema::Constraint;

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
    /// Internal constructor. Callers in this crate (`SchemaBuilder`)
    /// and in the lens crate (when applying the forgetful U) use this
    /// after verifying the no-layout invariant; external code cannot
    /// reach it.
    #[doc(hidden)]
    #[must_use]
    pub fn from_layout_free(schema: Schema) -> Self {
        debug_assert!(
            schema.is_layout_free(),
            "AbstractSchema::from_layout_free called with layout constraints present",
        );
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
    pub fn as_schema(&self) -> &Schema {
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
    /// Internal constructor. Callers (`panproto-parse`'s walker; the
    /// `decorate` synthesis driver) use this after asserting that a
    /// complete layout fibre has been attached.
    #[doc(hidden)]
    #[must_use]
    pub fn from_schema(schema: Schema) -> Self {
        Self { inner: schema }
    }

    /// Borrow the underlying schema for read-only consumption.
    ///
    /// See [`AbstractSchema::as_schema`] for the rationale: this is an
    /// explicit, audited bridge to the raw type, intentionally
    /// non-`Deref`.
    #[must_use]
    pub fn as_schema(&self) -> &Schema {
        &self.inner
    }

    /// Returns the schema's protocol name.
    #[must_use]
    pub fn protocol(&self) -> &str {
        &self.inner.protocol
    }

    /// Project to the abstract schema by forgetting all layout-fibre
    /// constraints. This is the lens get-direction realised in types.
    #[must_use]
    pub fn forget_layout(&self) -> AbstractSchema {
        AbstractSchema::from_layout_free(self.inner.forget_layout())
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
        let decorated = DecoratedSchema::from_schema(schema);
        let w = decorated.layout_witness("v0").unwrap();
        assert_eq!(w.start_byte(), Some(3));
        assert_eq!(w.end_byte(), Some(7));
    }
}
