//! A total, deterministic byte encoding of a [`Schema`], and the digest over
//! it.
//!
//! [`canonical_bytes`] turns a schema into a byte string that depends on the
//! schema's content and on nothing else: not on hash seeds, not on the
//! iteration order of a map, not on the process. [`canonical_digest`] is a
//! blake3 over those bytes. Together they let a schema carry a stable content
//! identity, which is what a span needs for the `domain` and `codomain` of its
//! legs.
//!
//! ## The two rules that make it total
//!
//! 1. **Every map is sorted before it is written.** [`Schema`] stores fifteen
//!    `HashMap`s, whose iteration order is a function of the hash seed, and
//!    the two `BTreeMap`s of its declarations. Each
//!    is written in ascending key order, using [`Name`]'s lexicographic
//!    [`Ord`] and [`Edge`]'s derived lexicographic `Ord` on
//!    `(src, tgt, kind, name)`.
//! 2. **Every variable-length run is length-prefixed.** Strings, byte
//!    strings, lists and maps are all written as a `u64` little-endian count
//!    followed by the elements. Without the string prefix, one vertex named
//!    `a` of kind `aab` would encode identically to one named `aa` of kind
//!    `b`, since a vertex is written as id ++ id ++ kind and the two runs
//!    concatenate alike, and concatenation ambiguity would collapse distinct
//!    schemas onto one digest.
//!
//! Sums are written as a one-byte discriminant before their payload, so
//! `Option`, [`UsageMode`], [`Expr`] and [`Literal`] are all self-delimiting.
//! Floats are written as [`f64::to_bits`], which is total and platform
//! independent where a decimal rendering would be neither.
//!
//! ## What it covers, and how that differs from the VCS content hash
//!
//! [`canonical_bytes`] covers **all twenty-two** [`Schema`] fields, `entries`
//! included, and writes the last of them, `declarations`, with the legacy
//! `keys` map followed by the `references` map. A nonempty target-set map is
//! an optional trailing extension; its absence preserves the established
//! declaration bytes. `panproto_vcs::hash::CanonicalSchema`
//! covers eighteen: it omits `entries` and the three derived adjacency
//! indices. The consequence is the
//! one that matters in practice: two schemas that differ only in their
//! *pointing* receive the **same** VCS object id and **different**
//! `canonical_bytes`.
//!
//! Both choices are right for their consumer. For the VCS a change of
//! pointing is not a change of schema content, so collapsing it keeps the
//! object graph small. For a span the pointing is part of the apex's
//! identity, because the apex's basepoints are what a downstream instance
//! layer roots its data at, and two apices that root differently are not
//! interchangeable.
//!
//! The three derived indices are included even though their *membership* is a
//! pure function of `edges`. Their bucket order is not derived: it is the
//! order a caller reads back out of
//! [`outgoing_edges`](crate::Schema::outgoing_edges), which
//! `panproto_io::cst_extract` walks to reconstruct source text. Encoding them
//! therefore separates two schemas whose children are ordered differently and
//! makes a corrupt index — a missing entry, an extra one, or a permuted bucket
//! — detectable rather than invisible.
//!
//! **Every `Vec` in the encoding is written in stored order**, constraint
//! lists and required-edge lists included, because for each of them the order
//! is observable: [`Schema::field_text`](crate::Schema::field_text) returns the
//! *first* matching `field:` constraint, so two schemas whose constraint lists
//! are permutations of one another emit different text. Empty lists are
//! written rather than dropped, so an empty entry and an absent key encode
//! differently. This is the third place the encoding diverges from
//! `panproto_vcs::hash::CanonicalSchema`, which sorts both lists and drops the
//! empties; the divergence is deliberate, and the direction is that
//! [`canonical_bytes`] separates schemas the VCS form identifies and never the
//! other way round.
//!
//! ## The digest
//!
//! [`canonical_digest`] is a blake3 over [`canonical_bytes`]. It is the
//! byte-level identity of a span's apex.

use std::collections::HashMap;

use panproto_expr::{Expr, Literal, Pattern};
use panproto_gat::Name;
use smallvec::SmallVec;

use crate::keys::{
    Cardinality, Key, KeyEquality, KeyPart, KeyRole, KeyScope, NullMatch, NumericPrimitive,
    OffsetUnit, OnUnresolved, PathSyntax, RefSource, RefTextSource, Reference, ResolveMode,
    SchemaDeclarations, SpanEnd, SqlCollation, SqlPad, UriBase, UriJoin, VariableBinding,
};
use crate::schema::{
    CoercionSpec, Constraint, Edge, HyperEdge, Schema, UsageMode, Variant, Vertex,
};

/// Encode a schema as a total, deterministic byte string.
///
/// Equal schemas always produce equal bytes, in any process and under any
/// hash seed. Schemas that differ in any of their twenty-two fields produce
/// different bytes, `entries` included: see the module documentation for how
/// that differs from the VCS content hash, which excludes the pointing.
///
/// # Examples
///
/// ```
/// use panproto_schema::{Protocol, SchemaBuilder, canonical_bytes};
///
/// let protocol = Protocol::default();
/// let build = || {
///     SchemaBuilder::new(&protocol)
///         .vertex("root", "object", None)?
///         .vertex("leaf", "string", None)?
///         .edge("root", "leaf", "prop", Some("leaf"))?
///         .build()
/// };
///
/// // Two structurally identical schemas, built independently, so their
/// // hash maps carry different seeds.
/// assert_eq!(canonical_bytes(&build()?), canonical_bytes(&build()?));
///
/// // The pointing is part of the identity.
/// let mut pointed = build()?;
/// pointed.entries.push("root".into());
/// assert_ne!(canonical_bytes(&build()?), canonical_bytes(&pointed));
/// # Ok::<(), panproto_schema::SchemaError>(())
/// ```
#[must_use]
pub fn canonical_bytes(schema: &Schema) -> Vec<u8> {
    let mut out = CanonicalWriter::new();
    push_graph(&mut out, schema);
    push_annotations(&mut out, schema);
    push_enrichment(&mut out, schema);
    push_indices(&mut out, schema);
    push_declarations(&mut out, &schema.declarations);
    out.into_bytes()
}

/// The blake3 digest of a schema's [`canonical_bytes`].
///
/// This is the content identity of a schema, and in particular of a span's
/// apex: two apices with the same digest are the same schema, and two apices
/// that differ in any of the twenty-two fields have different digests.
///
/// # Examples
///
/// ```
/// use panproto_schema::{Protocol, SchemaBuilder, canonical_digest};
///
/// let protocol = Protocol::default();
/// let build = || {
///     SchemaBuilder::new(&protocol)
///         .vertex("root", "object", None)?
///         .vertex("leaf", "string", None)?
///         .edge("root", "leaf", "prop", Some("leaf"))?
///         .build()
/// };
///
/// assert_eq!(canonical_digest(&build()?), canonical_digest(&build()?));
///
/// // The pointing is part of the identity.
/// let mut pointed = build()?;
/// pointed.entries.push("root".into());
/// assert_ne!(canonical_digest(&build()?), canonical_digest(&pointed));
/// # Ok::<(), panproto_schema::SchemaError>(())
/// ```
#[must_use]
pub fn canonical_digest(schema: &Schema) -> [u8; 32] {
    *blake3::hash(&canonical_bytes(schema)).as_bytes()
}

// ---------------------------------------------------------------------------
// Field groups, in declaration order.
// ---------------------------------------------------------------------------

/// Fields 1 to 8: the protocol name, the graph itself, and everything keyed
/// directly by a vertex id that the builder can set.
fn push_graph(out: &mut CanonicalWriter, schema: &Schema) {
    // 1. protocol
    out.str(&schema.protocol);

    // 2. vertices, keyed by vertex id
    let vertices = sorted(&schema.vertices);
    out.len(vertices.len());
    for (id, vertex) in vertices {
        out.name(id);
        push_vertex(out, vertex);
    }

    // 3. edges, keyed by the edge itself; the value is the edge kind
    let edges = sorted(&schema.edges);
    out.len(edges.len());
    for (edge, kind) in edges {
        out.edge(edge);
        out.name(kind);
    }

    // 4. hyper_edges, keyed by hyper-edge id
    let hyper_edges = sorted(&schema.hyper_edges);
    out.len(hyper_edges.len());
    for (id, hyper_edge) in hyper_edges {
        out.name(id);
        push_hyper_edge(out, hyper_edge);
    }

    // 5. constraints, keyed by vertex id; lists in stored order, empties kept
    let constraints = sorted(&schema.constraints);
    out.len(constraints.len());
    for (id, list) in constraints {
        out.name(id);
        out.len(list.len());
        for constraint in list {
            push_constraint(out, constraint);
        }
    }

    // 6. required, keyed by vertex id; lists in stored order, empties kept
    let required = sorted(&schema.required);
    out.len(required.len());
    for (id, list) in required {
        out.name(id);
        out.len(list.len());
        for edge in list {
            out.edge(edge);
        }
    }

    // 7. nsids, keyed by vertex id
    let nsids = sorted(&schema.nsids);
    out.len(nsids.len());
    for (id, nsid) in nsids {
        out.name(id);
        out.name(nsid);
    }

    // 8. entries: an ordered family of basepoints, written in stored order
    out.len(schema.entries.len());
    for entry in &schema.entries {
        out.name(entry);
    }
}

/// Fields 9 to 14: the structural annotations the builder cannot set.
fn push_annotations(out: &mut CanonicalWriter, schema: &Schema) {
    // 9. variants, keyed by the parent coproduct vertex id
    let variants = sorted(&schema.variants);
    out.len(variants.len());
    for (parent, arms) in variants {
        out.name(parent);
        out.len(arms.len());
        for arm in arms {
            push_variant(out, arm);
        }
    }

    // 10. orderings, keyed by the edge
    let orderings = sorted(&schema.orderings);
    out.len(orderings.len());
    for (edge, position) in orderings {
        out.edge(edge);
        out.u32(*position);
    }

    // 11. recursion_points, keyed by the fixpoint marker's vertex id
    let recursion_points = sorted(&schema.recursion_points);
    out.len(recursion_points.len());
    for (mu, point) in recursion_points {
        out.name(mu);
        out.name(&point.target_vertex);
    }

    // 12. spans, keyed by span id
    let spans = sorted(&schema.spans);
    out.len(spans.len());
    for (id, span) in spans {
        out.name(id);
        out.name(&span.id);
        out.name(&span.left);
        out.name(&span.right);
    }

    // 13. usage_modes, keyed by the edge
    let usage_modes = sorted(&schema.usage_modes);
    out.len(usage_modes.len());
    for (edge, mode) in usage_modes {
        out.edge(edge);
        push_usage_mode(out, mode);
    }

    // 14. nominal, keyed by vertex id
    let nominal = sorted(&schema.nominal);
    out.len(nominal.len());
    for (id, flag) in nominal {
        out.name(id);
        out.bool(*flag);
    }
}

/// Fields 15 to 18: the enrichment fibres.
fn push_enrichment(out: &mut CanonicalWriter, schema: &Schema) {
    // 15. coercions, keyed by (source_kind, target_kind)
    let coercions = sorted(&schema.coercions);
    out.len(coercions.len());
    for ((source_kind, target_kind), spec) in coercions {
        out.name(source_kind);
        out.name(target_kind);
        push_coercion_spec(out, spec);
    }

    // 16. mergers, keyed by vertex id
    push_expr_map(out, &schema.mergers);
    // 17. defaults, keyed by vertex id
    push_expr_map(out, &schema.defaults);
    // 18. policies, keyed by constraint sort name
    push_expr_map(out, &schema.policies);
}

/// Fields 19 to 21: the derived adjacency indices.
fn push_indices(out: &mut CanonicalWriter, schema: &Schema) {
    // 19. outgoing, keyed by Edge::src
    push_vertex_index(out, &schema.outgoing);
    // 20. incoming, keyed by Edge::tgt
    push_vertex_index(out, &schema.incoming);

    // 21. between, keyed by (src, tgt)
    let between = sorted(&schema.between);
    out.len(between.len());
    for ((src, tgt), bucket) in between {
        out.name(src);
        out.name(tgt);
        push_edges(out, bucket.as_slice());
    }
}

/// Field 22: declarations, whose legacy body is keys then references.
///
/// Nonempty target sets use an optional trailing extension, so declarations
/// with no target set retain their established bytes.
///
/// Both maps are written in ascending key order and each vertex's keys in
/// ascending key id order. Every field of [`Key`], [`KeyPart`], [`Reference`]
/// and [`RefSource`] is written in declaration order, and every sum behind a
/// one-byte discriminant in declaration order of its variants.
fn push_declarations(out: &mut CanonicalWriter, declarations: &SchemaDeclarations) {
    // 22. keys, keyed by vertex id
    out.len(declarations.keys.len());
    for (vertex, keys) in &declarations.keys {
        out.name(vertex);
        let mut sorted: Vec<&Key> = keys.iter().collect();
        sorted.sort_by(|left, right| left.id.cmp(&right.id));
        out.len(sorted.len());
        for key in sorted {
            push_key(out, key);
        }
    }

    // 23. references, keyed by reference id
    out.len(declarations.references.len());
    for (id, reference) in &declarations.references {
        out.name(id);
        push_reference(out, reference);
    }

    // Optional declaration extension. Absence is the exact legacy byte form.
    if !declarations.target_sets.is_empty() {
        out.tag(0xff);
        out.len(declarations.target_sets.len());
        for (id, set) in &declarations.target_sets {
            out.name(id);
            out.len(set.members.len());
            for (vertex, key) in &set.members {
                out.name(vertex);
                out.name(key);
            }
            push_key_scope(out, &set.scope);
        }
    }
}

/// Write a key: id, parts, role, scope.
fn push_key(out: &mut CanonicalWriter, key: &Key) {
    out.name(&key.id);
    out.len(key.parts.len());
    for part in &key.parts {
        match part {
            KeyPart::Path(path) => {
                out.tag(0);
                push_edges(out, path);
            }
            KeyPart::Position { within } => {
                out.tag(1);
                out.edge(within);
            }
        }
    }
    let role = match key.role {
        KeyRole::Primary => 0,
        KeyRole::Unique => 1,
        KeyRole::Identifier => 2,
        KeyRole::SortKey => 3,
        KeyRole::Index => 4,
    };
    out.tag(if key.equality.is_empty() {
        role
    } else {
        role | 0x80
    });
    push_key_scope(out, &key.scope);
    if !key.equality.is_empty() {
        out.len(key.equality.len());
        for equality in &key.equality {
            push_key_equality(out, equality);
        }
    }
}

/// Write a key scope behind its discriminant.
fn push_key_scope(out: &mut CanonicalWriter, scope: &KeyScope) {
    match scope {
        KeyScope::Document => out.tag(0),
        KeyScope::Global => out.tag(1),
        KeyScope::Within(ancestor) => {
            out.tag(2);
            out.name(ancestor);
        }
    }
}

/// Write one per-part key equality declaration.
fn push_key_equality(out: &mut CanonicalWriter, equality: &KeyEquality) {
    match equality {
        KeyEquality::Representation => out.tag(0),
        KeyEquality::NumericValue => out.tag(1),
        KeyEquality::NumericValueWithinKind(primitive) => {
            out.tag(2);
            match primitive {
                NumericPrimitive::Decimal => out.tag(0),
                NumericPrimitive::Float(width) => {
                    out.tag(1);
                    out.tag(*width);
                }
                NumericPrimitive::Any => out.tag(2),
            }
        }
        KeyEquality::Sql { collation, pad } => {
            out.tag(3);
            match collation {
                SqlCollation::Binary => out.tag(0),
                SqlCollation::AsciiCaseInsensitive => out.tag(1),
                SqlCollation::Named(name) => {
                    out.tag(2);
                    out.name(name);
                }
            }
            out.tag(match pad {
                SqlPad::NoPad => 0,
                SqlPad::PadSpace => 1,
            });
        }
        KeyEquality::XmlNonCdata => out.tag(4),
    }
}

/// Write a reference, field by field in declaration order.
fn push_reference(out: &mut CanonicalWriter, reference: &Reference) {
    out.name(&reference.id);
    out.name(&reference.holder);
    push_ref_source(out, &reference.source);
    out.name(&reference.target_vertex);
    out.opt_name(reference.target_key.as_ref());
    match &reference.edge {
        None => out.tag(0),
        Some(edge) => {
            out.tag(1);
            out.edge(edge);
        }
    }
    out.tag(match reference.cardinality {
        Cardinality::One => 0,
        Cardinality::Many => 1,
    });
    out.bool(reference.nullable);
    out.tag(match reference.on_unresolved {
        OnUnresolved::Error => 0,
        OnUnresolved::Complement => 1,
    });
    out.tag(match reference.mode {
        ResolveMode::AddEdge => 0,
        ResolveMode::Redirect => 1,
    });
    push_reference_extensions(out, reference);
}

/// Write the optional reference extension envelope.
fn push_reference_extensions(out: &mut CanonicalWriter, reference: &Reference) {
    let extended = reference.guard.is_some()
        || reference.null_match != NullMatch::Simple
        || reference.null_vertex.is_some()
        || reference.scope_via.is_some();
    if !extended {
        return;
    }
    out.tag(0xff);
    match &reference.guard {
        None => out.tag(0),
        Some(guard) => {
            out.tag(1);
            push_edges(out, &guard.field);
            out.raw(&guard.equals.canonical_bytes());
        }
    }
    out.tag(match reference.null_match {
        NullMatch::Simple => 0,
        NullMatch::Full => 1,
    });
    out.opt_name(reference.null_vertex.as_ref());
    match &reference.scope_via {
        None => out.tag(0),
        Some(via) => {
            out.tag(1);
            out.name(&via.ancestor);
            out.len(via.hops.len());
            for hop in &via.hops {
                out.name(hop);
            }
        }
    }
}

/// Write a reference source behind its discriminant.
fn push_ref_source(out: &mut CanonicalWriter, source: &RefSource) {
    match source {
        RefSource::KeyValue { parts } => {
            out.tag(0);
            out.len(parts.len());
            for path in parts {
                push_edges(out, path);
            }
        }
        RefSource::Path { field, syntax } => {
            out.tag(1);
            push_edges(out, field);
            match syntax {
                PathSyntax::JsonPointer => out.tag(0),
                PathSyntax::XPath => out.tag(1),
                PathSyntax::Uri { base } => {
                    out.tag(2);
                    push_uri_base(out, base);
                }
                PathSyntax::IdRef => out.tag(3),
            }
        }
        RefSource::Standoff {
            text,
            start,
            end,
            unit,
            end_inclusive,
            end_form,
            origin,
            fragments,
        } => {
            out.tag(2);
            push_ref_text_source(out, text);
            push_edges(out, start);
            push_edges(out, end);
            out.tag(match unit {
                OffsetUnit::Byte => 0,
                OffsetUnit::Utf8Char => 1,
                OffsetUnit::Utf16 => 2,
                OffsetUnit::Token => 3,
            });
            out.bool(*end_inclusive);
            if *end_form != SpanEnd::Offset || *origin != 0 || fragments.is_some() {
                out.tag(0xff);
                out.tag(match end_form {
                    SpanEnd::Offset => 0,
                    SpanEnd::Length => 1,
                });
                out.u64(*origin);
                match fragments {
                    None => out.tag(0),
                    Some(edge) => {
                        out.tag(1);
                        out.edge(edge);
                    }
                }
            }
        }
        RefSource::Variable {
            field,
            binder_key,
            binding,
        } => {
            out.tag(3);
            push_edges(out, field);
            out.name(binder_key);
            if *binding != VariableBinding::FirstDefines {
                out.tag(0xff);
                out.tag(match binding {
                    VariableBinding::FirstDefines => 0,
                    VariableBinding::NearestPreceding => 1,
                });
            }
        }
        RefSource::Inline { field } => {
            out.tag(4);
            push_edges(out, field);
        }
    }
}

/// Write a URI base while retaining the legacy `None` and `Some(path)` tags.
fn push_uri_base(out: &mut CanonicalWriter, base: &UriBase) {
    match base {
        UriBase::Absolute => out.tag(0),
        UriBase::Field(field) => {
            out.tag(1);
            push_edges(out, field);
        }
        UriBase::Positional { base, prefix } => {
            out.tag(2);
            out.opt_name(base.as_ref());
            out.opt_name(prefix.as_ref());
        }
        UriBase::Ancestor {
            ancestor,
            field,
            join,
        } => {
            out.tag(3);
            out.name(ancestor);
            push_edges(out, field);
            out.tag(match join {
                UriJoin::Rfc3986 => 0,
                UriJoin::FhirServiceBase => 1,
            });
        }
        UriBase::Inherited { field } => {
            out.tag(4);
            push_edges(out, field);
        }
    }
}

/// Write a standoff text source behind stable discriminants.
fn push_ref_text_source(out: &mut CanonicalWriter, source: &RefTextSource) {
    match source {
        RefTextSource::Local(path) => {
            out.tag(0);
            push_edges(out, path);
        }
        RefTextSource::External(path) => {
            out.tag(1);
            push_edges(out, path);
        }
        RefTextSource::Referenced {
            reference,
            text,
            external,
        } => {
            out.tag(2);
            out.name(reference);
            push_edges(out, text);
            match external {
                None => out.tag(0),
                Some(path) => {
                    out.tag(1);
                    push_edges(out, path);
                }
            }
        }
        RefTextSource::Span { reference } => {
            out.tag(3);
            out.name(reference);
        }
        RefTextSource::ExternalUri { field, base } => {
            out.tag(4);
            push_edges(out, field);
            push_uri_base(out, base);
        }
    }
}

// ---------------------------------------------------------------------------
// Map ordering.
// ---------------------------------------------------------------------------

/// Return a map's entries in ascending key order.
fn sorted<K: Ord, V, S>(map: &HashMap<K, V, S>) -> Vec<(&K, &V)> {
    let mut items: Vec<(&K, &V)> = map.iter().collect();
    items.sort_by(|left, right| left.0.cmp(right.0));
    items
}

// ---------------------------------------------------------------------------
// Schema value types.
// ---------------------------------------------------------------------------

/// Write a vertex: id, kind, optional NSID.
fn push_vertex(out: &mut CanonicalWriter, vertex: &Vertex) {
    out.name(&vertex.id);
    out.name(&vertex.kind);
    out.opt_name(vertex.nsid.as_ref());
}

/// Write a length-prefixed run of edges in stored order.
fn push_edges(out: &mut CanonicalWriter, edges: &[Edge]) {
    out.len(edges.len());
    for edge in edges {
        out.edge(edge);
    }
}

/// Write a hyper-edge, its signature sorted by label.
fn push_hyper_edge(out: &mut CanonicalWriter, hyper_edge: &HyperEdge) {
    out.name(&hyper_edge.id);
    out.name(&hyper_edge.kind);
    let signature = sorted(&hyper_edge.signature);
    out.len(signature.len());
    for (label, vertex_id) in signature {
        out.name(label);
        out.name(vertex_id);
    }
    out.name(&hyper_edge.parent_label);
}

/// Write a constraint: sort then value.
fn push_constraint(out: &mut CanonicalWriter, constraint: &Constraint) {
    out.name(&constraint.sort);
    out.str(&constraint.value);
}

/// Write a coproduct arm.
fn push_variant(out: &mut CanonicalWriter, variant: &Variant) {
    out.name(&variant.id);
    out.name(&variant.parent_vertex);
    out.opt_name(variant.tag.as_ref());
}

/// Write a use-counting mode as a one-byte discriminant.
fn push_usage_mode(out: &mut CanonicalWriter, mode: &UsageMode) {
    out.tag(match mode {
        UsageMode::Structural => 0,
        UsageMode::Linear => 1,
        UsageMode::Affine => 2,
    });
}

/// Write a coercion specification.
///
/// The round-trip class is a fieldless enum, so its [`Debug`] rendering is
/// exactly the variant name: total, injective, and stable.
fn push_coercion_spec(out: &mut CanonicalWriter, spec: &CoercionSpec) {
    out.expr(&spec.forward);
    match &spec.inverse {
        None => out.tag(0),
        Some(inverse) => {
            out.tag(1);
            out.expr(inverse);
        }
    }
    out.str(&format!("{:?}", spec.class));
}

/// Write a [`Name`]-keyed map of expressions in ascending key order.
fn push_expr_map<S>(out: &mut CanonicalWriter, map: &HashMap<Name, Expr, S>) {
    let items = sorted(map);
    out.len(items.len());
    for (key, expr) in items {
        out.name(key);
        out.expr(expr);
    }
}

/// Write a vertex-keyed adjacency index, each bucket in stored order.
fn push_vertex_index<S>(out: &mut CanonicalWriter, index: &HashMap<Name, SmallVec<Edge, 4>, S>) {
    let items = sorted(index);
    out.len(items.len());
    for (vertex_id, bucket) in items {
        out.name(vertex_id);
        push_edges(out, bucket.as_slice());
    }
}

// ---------------------------------------------------------------------------
// The writer. Every variable-length run carries a u64 little-endian length.
// ---------------------------------------------------------------------------

/// The canonical byte writer [`canonical_bytes`] is built on.
///
/// Each method appends one value in the encoding the module documentation
/// describes: lengths as eight little-endian bytes, strings and byte strings
/// length-prefixed, sums as a one-byte discriminant before their payload,
/// integers little-endian, floats as their IEEE-754 bit pattern. A value
/// built from these methods alone is self-delimiting, so a caller that writes
/// every map in ascending key order and every sequence with its length gets a
/// total, injective encoding of its own types.
///
/// # Examples
///
/// ```
/// use panproto_schema::CanonicalWriter;
///
/// let mut w = CanonicalWriter::new();
/// w.str("ab");
/// w.tag(7);
/// assert_eq!(w.into_bytes(), [2, 0, 0, 0, 0, 0, 0, 0, b'a', b'b', 7]);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalWriter(Vec<u8>);

impl CanonicalWriter {
    /// An empty writer.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// The bytes written so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the writer and return its bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Append `bytes` as they are, with no length prefix.
    ///
    /// The inherent encoders of this type write to the buffer directly and do
    /// not go through this method. It exists for encoders outside this crate,
    /// which cannot reach the private buffer: the `CanonicalSink`
    /// implementation for `CanonicalWriter` in panproto-inst appends through
    /// it, as does any caller that encodes a type of its own and writes its
    /// bytes here.
    ///
    /// ```
    /// use panproto_schema::CanonicalWriter;
    ///
    /// let mut w = CanonicalWriter::new();
    /// w.raw(&[1, 2]);
    /// w.tag(3);
    /// assert_eq!(w.into_bytes(), [1, 2, 3]);
    /// ```
    pub fn raw(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    /// Write a length (or count) as eight little-endian bytes.
    pub fn len(&mut self, len: usize) {
        let wide = u64::try_from(len).unwrap_or(u64::MAX);
        self.0.extend_from_slice(&wide.to_le_bytes());
    }

    /// Write a one-byte discriminant.
    pub fn tag(&mut self, tag: u8) {
        self.0.push(tag);
    }

    /// Write a boolean as one byte, `0` or `1`.
    pub fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }

    /// Write a `u32` as four little-endian bytes.
    pub fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a `u64` as eight little-endian bytes.
    pub fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an `i64` as eight little-endian bytes.
    pub fn i64(&mut self, value: i64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    /// Write an `f64` as the eight little-endian bytes of its bit pattern.
    ///
    /// Total where a decimal rendering would not be: `-0.0` and `0.0` are
    /// distinct, and a NaN is written with its payload.
    pub fn f64(&mut self, value: f64) {
        self.0.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    /// Write a length-prefixed byte string.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.len(bytes.len());
        self.0.extend_from_slice(bytes);
    }

    /// Write a length-prefixed UTF-8 string.
    pub fn str(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    /// Write a length-prefixed [`Name`].
    pub fn name(&mut self, name: &Name) {
        self.str(name.as_ref());
    }

    /// Write an optional [`Name`] as a presence byte followed by the name.
    pub fn opt_name(&mut self, name: Option<&Name>) {
        match name {
            None => self.tag(0),
            Some(value) => {
                self.tag(1);
                self.name(value);
            }
        }
    }

    /// Write an edge: source, target, kind, optional label.
    pub fn edge(&mut self, edge: &Edge) {
        self.name(&edge.src);
        self.name(&edge.tgt);
        self.name(&edge.kind);
        self.opt_name(edge.name.as_ref());
    }

    /// Write an expression, discriminant first.
    pub fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Var(name) => {
                self.tag(0);
                self.str(name);
            }
            Expr::Lam(param, body) => {
                self.tag(1);
                self.str(param);
                self.expr(body);
            }
            Expr::App(func, arg) => {
                self.tag(2);
                self.expr(func);
                self.expr(arg);
            }
            Expr::Lit(literal) => {
                self.tag(3);
                self.literal(literal);
            }
            Expr::Record(fields) => {
                self.tag(4);
                self.len(fields.len());
                for (name, value) in fields {
                    self.str(name);
                    self.expr(value);
                }
            }
            Expr::List(items) => {
                self.tag(5);
                self.len(items.len());
                for item in items {
                    self.expr(item);
                }
            }
            Expr::Field(base, field) => {
                self.tag(6);
                self.expr(base);
                self.str(field);
            }
            Expr::Index(base, index) => {
                self.tag(7);
                self.expr(base);
                self.expr(index);
            }
            Expr::Match { scrutinee, arms } => {
                self.tag(8);
                self.expr(scrutinee);
                self.len(arms.len());
                for (pattern, body) in arms {
                    self.pattern(pattern);
                    self.expr(body);
                }
            }
            Expr::Let { name, value, body } => {
                self.tag(9);
                self.str(name);
                self.expr(value);
                self.expr(body);
            }
            Expr::Builtin(op, args) => {
                self.tag(10);
                // `BuiltinOp` is a fieldless enum, so its `Debug` rendering is
                // exactly the variant name.
                self.str(&format!("{op:?}"));
                self.len(args.len());
                for arg in args {
                    self.expr(arg);
                }
            }
        }
    }

    /// Write a literal value, discriminant first. Floats go in as raw bits, which
    /// is total where a decimal rendering would not be.
    ///
    /// The exact values take the tags after the closure's `8`: `9` a
    /// decimal, `10` an integer outside `i64` and `11` a date or time, each
    /// followed by its `canonical_bytes` (see `panproto_expr::exact`).
    pub fn literal(&mut self, literal: &Literal) {
        match literal {
            Literal::Bool(value) => {
                self.tag(0);
                self.bool(*value);
            }
            Literal::Int(value) => {
                self.tag(1);
                self.i64(*value);
            }
            Literal::Float(value) => {
                self.tag(2);
                self.f64(*value);
            }
            Literal::Str(value) => {
                self.tag(3);
                self.str(value);
            }
            Literal::Bytes(value) => {
                self.tag(4);
                self.bytes(value);
            }
            Literal::Null => self.tag(5),
            Literal::Record(fields) => {
                self.tag(6);
                self.len(fields.len());
                for (name, value) in fields {
                    self.str(name);
                    self.literal(value);
                }
            }
            Literal::List(items) => {
                self.tag(7);
                self.len(items.len());
                for item in items {
                    self.literal(item);
                }
            }
            Literal::Closure { param, body, env } => {
                self.tag(8);
                self.str(param);
                self.expr(body);
                self.len(env.len());
                for (name, value) in env.iter() {
                    self.str(name);
                    self.literal(value);
                }
            }
            Literal::Decimal(value) => {
                self.tag(9);
                self.raw(&value.canonical_bytes());
            }
            Literal::BigInt(value) => {
                self.tag(10);
                self.raw(&value.canonical_bytes());
            }
            Literal::Temporal(value) => {
                self.tag(11);
                self.raw(&value.canonical_bytes());
            }
        }
    }

    /// Write a match pattern, discriminant first.
    pub fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Wildcard => self.tag(0),
            Pattern::Var(name) => {
                self.tag(1);
                self.str(name);
            }
            Pattern::Lit(literal) => {
                self.tag(2);
                self.literal(literal);
            }
            Pattern::Record(fields) => {
                self.tag(3);
                self.len(fields.len());
                for (name, sub) in fields {
                    self.str(name);
                    self.pattern(sub);
                }
            }
            Pattern::List(items) => {
                self.tag(4);
                self.len(items.len());
                for item in items {
                    self.pattern(item);
                }
            }
            Pattern::Constructor(tag, args) => {
                self.tag(5);
                self.str(tag);
                self.len(args.len());
                for arg in args {
                    self.pattern(arg);
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builder::SchemaBuilder;
    use crate::protocol::Protocol;
    use crate::schema::{RecursionPoint, Span};
    use panproto_gat::CoercionClass;

    fn protocol() -> Protocol {
        Protocol {
            name: "fixture".to_owned(),
            ..Default::default()
        }
    }

    /// A small schema touching every field group: graph, annotations,
    /// enrichment, indices.
    fn fixture() -> Schema {
        let protocol = protocol();
        let mut signature: HashMap<String, String> = HashMap::new();
        signature.insert("parent".to_owned(), "root".to_owned());
        signature.insert("child".to_owned(), "leaf".to_owned());

        let mut schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", Some("com.example.root"))
            .expect("root")
            .vertex("leaf", "string", None)
            .expect("leaf")
            .edge("root", "leaf", "prop", Some("leaf"))
            .expect("root -> leaf")
            .hyper_edge("he", "record", signature, "parent")
            .expect("he")
            .constraint("leaf", "maxLength", "10")
            .constraint("leaf", "format", "uuid")
            .required(
                "root",
                vec![Edge {
                    src: Name::from("root"),
                    tgt: Name::from("leaf"),
                    kind: Name::from("prop"),
                    name: Some(Name::from("leaf")),
                }],
            )
            .coercion(
                "object",
                "string",
                CoercionSpec {
                    forward: Expr::Lit(Literal::Float(1.5)),
                    inverse: Some(Expr::Var("x".into())),
                    class: CoercionClass::Retraction,
                },
            )
            .merger("root", Expr::Lit(Literal::Int(7)))
            .default_expr("leaf", Expr::Lit(Literal::Null))
            .policy("maxLength", Expr::Lit(Literal::Bool(true)))
            .entry("root")
            .build()
            .expect("build");

        schema.variants.insert(
            Name::from("root"),
            vec![Variant {
                id: Name::from("leaf"),
                parent_vertex: Name::from("root"),
                tag: Some(Name::from("t")),
            }],
        );
        let edge = Edge {
            src: Name::from("root"),
            tgt: Name::from("leaf"),
            kind: Name::from("prop"),
            name: Some(Name::from("leaf")),
        };
        schema.orderings.insert(edge.clone(), 3);
        schema.recursion_points.insert(
            Name::from("root"),
            RecursionPoint {
                target_vertex: Name::from("leaf"),
            },
        );
        schema.spans.insert(
            Name::from("s"),
            Span {
                id: Name::from("s"),
                left: Name::from("root"),
                right: Name::from("leaf"),
            },
        );
        schema.usage_modes.insert(edge, UsageMode::Linear);
        schema.nominal.insert(Name::from("root"), true);
        schema
    }

    #[test]
    fn encoding_is_stable_across_a_hundred_independent_builds() {
        let baseline = canonical_bytes(&fixture());
        for round in 0..100 {
            // Rebuilding the fixture gives fresh `HashMap`s with fresh hash
            // seeds, so iteration order differs even though content does not.
            assert_eq!(
                canonical_bytes(&fixture()),
                baseline,
                "encoding drifted on round {round}"
            );
        }
    }

    #[test]
    fn the_pointing_is_part_of_the_identity() {
        let unpointed = fixture();
        let mut repointed = unpointed.clone();
        repointed.entries = vec![Name::from("leaf"), Name::from("root")];

        assert_ne!(canonical_bytes(&unpointed), canonical_bytes(&repointed));

        // Order within `entries` matters too.
        let mut reordered = unpointed.clone();
        reordered.entries = vec![Name::from("root"), Name::from("leaf")];
        let mut other_order = unpointed;
        other_order.entries = vec![Name::from("leaf"), Name::from("root")];
        assert_ne!(canonical_bytes(&reordered), canonical_bytes(&other_order));
    }

    #[test]
    fn every_field_group_contributes() {
        let base = fixture();

        let mut changed_graph = base.clone();
        changed_graph.protocol = "other".to_owned();
        assert_ne!(canonical_bytes(&base), canonical_bytes(&changed_graph));

        let mut changed_annotation = base.clone();
        changed_annotation.nominal.insert(Name::from("leaf"), false);
        assert_ne!(canonical_bytes(&base), canonical_bytes(&changed_annotation));

        let mut changed_enrichment = base.clone();
        changed_enrichment
            .policies
            .insert(Name::from("format"), Expr::Lit(Literal::Null));
        assert_ne!(canonical_bytes(&base), canonical_bytes(&changed_enrichment));

        let mut changed_index = base.clone();
        changed_index.outgoing.remove("root");
        assert_ne!(canonical_bytes(&base), canonical_bytes(&changed_index));
    }

    /// Length prefixes are what stop two different schemas concatenating to
    /// the same bytes.
    ///
    /// The two pairs test different prefixes, and the second is the one that
    /// tests the string prefix at all. Vertex counts differ in the first pair,
    /// so the map's own length prefix separates them however the strings inside
    /// are written; only a pair with equal element counts whose *runs*
    /// concatenate alike can reach the string prefix. `a`/`aab` and `aa`/`b`
    /// are such a pair: a vertex is written as id ++ id ++ kind, and
    /// `"a" ++ "a" ++ "aab"` is `"aa" ++ "aa" ++ "b"`, so dropping the string
    /// prefix collapses the two onto one digest.
    #[test]
    fn concatenation_is_unambiguous() {
        let protocol = protocol();
        let split = SchemaBuilder::new(&protocol)
            .vertex("a", "object", None)
            .expect("a")
            .vertex("b", "object", None)
            .expect("b")
            .build()
            .expect("build");
        let joined = SchemaBuilder::new(&protocol)
            .vertex("ab", "object", None)
            .expect("ab")
            .build()
            .expect("build");

        assert_ne!(canonical_bytes(&split), canonical_bytes(&joined));

        let one = SchemaBuilder::new(&protocol)
            .vertex("a", "aab", None)
            .expect("a")
            .build()
            .expect("build");
        let other = SchemaBuilder::new(&protocol)
            .vertex("aa", "b", None)
            .expect("aa")
            .build()
            .expect("build");

        assert_ne!(
            canonical_bytes(&one),
            canonical_bytes(&other),
            "id ++ id ++ kind concatenates alike, so only the string prefix separates these"
        );
    }

    #[test]
    fn float_bits_separate_signed_zeroes() {
        let base = fixture();
        let mut negative = base.clone();
        negative.coercions.insert(
            (Name::from("object"), Name::from("string")),
            CoercionSpec {
                forward: Expr::Lit(Literal::Float(-0.0)),
                inverse: None,
                class: CoercionClass::Retraction,
            },
        );
        let mut positive = base;
        positive.coercions.insert(
            (Name::from("object"), Name::from("string")),
            CoercionSpec {
                forward: Expr::Lit(Literal::Float(0.0)),
                inverse: None,
                class: CoercionClass::Retraction,
            },
        );

        assert_ne!(canonical_bytes(&negative), canonical_bytes(&positive));
    }

    /// An empty list and an absent key are different states, so they must not
    /// share an encoding. Only `constraints` and `required` ever collided:
    /// `variants` has always written its empties.
    #[test]
    fn an_empty_list_does_not_encode_as_an_absent_key() {
        let base = fixture();

        let mut empty_required = base.clone();
        empty_required
            .required
            .insert(Name::from("leaf"), Vec::new());
        assert_ne!(
            canonical_bytes(&base),
            canonical_bytes(&empty_required),
            "an empty `required` list must not encode as an absent key"
        );

        let mut empty_constraints = base.clone();
        empty_constraints
            .constraints
            .insert(Name::from("root"), Vec::new());
        assert_ne!(
            canonical_bytes(&base),
            canonical_bytes(&empty_constraints),
            "an empty `constraints` list must not encode as an absent key"
        );

        let mut empty_variants = base.clone();
        empty_variants
            .variants
            .insert(Name::from("leaf"), Vec::new());
        assert_ne!(canonical_bytes(&base), canonical_bytes(&empty_variants));
    }

    /// Constraint order is observable through
    /// [`Schema::field_text`](crate::Schema::field_text), which returns the
    /// first match, so permuting a constraint list changes the schema's
    /// meaning and must change its bytes.
    #[test]
    fn constraint_order_is_part_of_the_identity() {
        let field = |value: &str| Constraint {
            sort: Name::from("field:op"),
            value: value.to_owned(),
        };

        let mut plus_first = fixture();
        plus_first
            .constraints
            .insert(Name::from("root"), vec![field("+"), field("-")]);
        let mut minus_first = fixture();
        minus_first
            .constraints
            .insert(Name::from("root"), vec![field("-"), field("+")]);

        assert_eq!(plus_first.field_text("root", "op"), Some("+"));
        assert_eq!(minus_first.field_text("root", "op"), Some("-"));
        assert_ne!(
            canonical_bytes(&plus_first),
            canonical_bytes(&minus_first),
            "two schemas that read back different field text must not share an encoding"
        );
    }

    /// Required-edge order is written as stored, like every other `Vec`.
    #[test]
    fn required_edge_order_is_part_of_the_identity() {
        let labelled = Edge {
            src: Name::from("root"),
            tgt: Name::from("leaf"),
            kind: Name::from("prop"),
            name: Some(Name::from("leaf")),
        };
        let bare = Edge {
            name: None,
            ..labelled.clone()
        };

        let mut forward = fixture();
        forward
            .required
            .insert(Name::from("root"), vec![labelled.clone(), bare.clone()]);
        let mut backward = fixture();
        backward
            .required
            .insert(Name::from("root"), vec![bare, labelled]);

        assert_ne!(canonical_bytes(&forward), canonical_bytes(&backward));
    }

    /// A permuted adjacency bucket is a different schema: the bucket order is
    /// what a caller reads back out of `outgoing_edges`.
    #[test]
    fn a_permuted_adjacency_bucket_is_visible() {
        let protocol = protocol();
        let build = |first: &str, second: &str| {
            SchemaBuilder::new(&protocol)
                .vertex("root", "object", None)
                .expect("root")
                .vertex("leaf", "string", None)
                .expect("leaf")
                .edge("root", "leaf", "prop", Some(first))
                .expect("first")
                .edge("root", "leaf", "prop", Some(second))
                .expect("second")
                .build()
                .expect("build")
        };
        let ab = build("a", "b");
        let ba = build("b", "a");

        assert_eq!(ab.edges, ba.edges, "the two differ only in bucket order");
        assert_ne!(canonical_bytes(&ab), canonical_bytes(&ba));
    }

    #[test]
    fn the_digest_tracks_the_bytes() {
        let base = fixture();
        assert_eq!(canonical_digest(&base), canonical_digest(&fixture()));

        let mut repointed = base.clone();
        repointed.entries = vec![Name::from("leaf")];
        assert_ne!(canonical_digest(&base), canonical_digest(&repointed));
        assert_eq!(
            canonical_digest(&base),
            *blake3::hash(&canonical_bytes(&base)).as_bytes()
        );
    }

    /// A merger touching every `Expr`, `Literal` and `Pattern` variant.
    fn every_expression_form() -> Expr {
        use panproto_expr::BuiltinOp;
        let closure = Literal::Closure {
            param: "p".into(),
            body: Box::new(Expr::Var("p".into())),
            env: std::iter::once(("k".into(), Literal::Int(-3))).collect(),
        };
        let literals = Literal::List(vec![
            Literal::Bool(false),
            Literal::Int(i64::MIN),
            Literal::Float(-0.0),
            Literal::Str("s".into()),
            Literal::Bytes(vec![0, 255]),
            Literal::Null,
            Literal::Record(vec![("f".into(), Literal::Float(f64::NAN))]),
            closure,
        ]);
        let pattern = Pattern::Record(vec![
            ("a".into(), Pattern::Wildcard),
            ("b".into(), Pattern::Var("v".into())),
            ("c".into(), Pattern::Lit(Literal::Int(1))),
            (
                "d".into(),
                Pattern::List(vec![Pattern::Constructor(
                    "Some".into(),
                    vec![Pattern::Wildcard],
                )]),
            ),
        ]);
        Expr::Let {
            name: "x".into(),
            value: Box::new(Expr::Lit(literals)),
            body: Box::new(Expr::Match {
                scrutinee: Box::new(Expr::App(
                    Box::new(Expr::Lam("y".into(), Box::new(Expr::Var("y".into())))),
                    Box::new(Expr::Record(vec![(
                        "r".into(),
                        Expr::List(vec![Expr::Field(
                            Box::new(Expr::Var("x".into())),
                            "f".into(),
                        )]),
                    )])),
                )),
                arms: vec![(
                    pattern,
                    Expr::Builtin(
                        BuiltinOp::Add,
                        vec![Expr::Index(
                            Box::new(Expr::Var("x".into())),
                            Box::new(Expr::Lit(Literal::Int(0))),
                        )],
                    ),
                )],
            }),
        }
    }

    /// The encoding is frozen: anything keyed by it (a span's apex digest,
    /// a schema identity) changes when it does. These are the blake3
    /// digests of the bytes, so any change to a single byte shows here.
    #[test]
    fn the_encoding_is_pinned() {
        let hex = |bytes: &[u8]| -> String { blake3::hash(bytes).to_hex().to_string() };

        let base = fixture();
        let mut expressive = fixture();
        expressive
            .mergers
            .insert(Name::from("leaf"), every_expression_form());

        assert_eq!(canonical_bytes(&base).len(), PINNED_FIXTURE_LEN);
        assert_eq!(hex(&canonical_bytes(&base)), PINNED_FIXTURE_DIGEST);
        assert_eq!(hex(&canonical_bytes(&expressive)), PINNED_EXPRESSIVE_DIGEST);
    }

    const PINNED_FIXTURE_LEN: usize = 1229;
    const PINNED_FIXTURE_DIGEST: &str =
        "546b6e7e984e9843ac0c17365da761a19c11996daeacb6ee202ce5939f2ae41b";
    const PINNED_EXPRESSIVE_DIGEST: &str =
        "8cbe7db62436b2a9dd582c9702916f025921800514aee8e544fab7c9be319975";

    #[test]
    fn the_writer_shows_exactly_the_bytes_it_appended() {
        let mut w = CanonicalWriter::new();
        assert_eq!(w.as_bytes(), [0_u8; 0]);
        w.u64(0x0102_0304_0506_0708);
        assert_eq!(w.as_bytes(), [8, 7, 6, 5, 4, 3, 2, 1]);
        w.tag(9);
        assert_eq!(w.as_bytes(), [8, 7, 6, 5, 4, 3, 2, 1, 9]);
        assert_eq!(w.clone().into_bytes(), w.as_bytes());
    }

    #[test]
    fn induced_apices_that_agree_encode_alike() {
        let protocol = protocol();
        let schema = fixture();
        let keep: rustc_hash::FxHashSet<Name> =
            ["root", "leaf"].into_iter().map(Name::from).collect();
        let once = crate::induce::induce_on_vertices(&schema, &protocol, &keep).expect("once");
        let twice = crate::induce::induce_on_vertices(&once, &protocol, &keep).expect("twice");
        assert_eq!(canonical_bytes(&once), canonical_bytes(&twice));
    }
}
