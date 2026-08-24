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
//!    `HashMap`s, whose iteration order is a function of the hash seed. Each
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
//! [`canonical_bytes`] covers **all twenty-one** [`Schema`] fields, `entries`
//! included. `panproto_vcs::hash::CanonicalSchema` covers seventeen: it omits
//! `entries` and the three derived adjacency indices. The consequence is the
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

use crate::schema::{
    CoercionSpec, Constraint, Edge, HyperEdge, Schema, UsageMode, Variant, Vertex,
};

/// Encode a schema as a total, deterministic byte string.
///
/// Equal schemas always produce equal bytes, in any process and under any
/// hash seed. Schemas that differ in any of their twenty-one fields produce
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
    let mut out = Vec::new();
    push_graph(&mut out, schema);
    push_annotations(&mut out, schema);
    push_enrichment(&mut out, schema);
    push_indices(&mut out, schema);
    out
}

/// The blake3 digest of a schema's [`canonical_bytes`].
///
/// This is the content identity of a schema, and in particular of a span's
/// apex: two apices with the same digest are the same schema, and two apices
/// that differ in any of the twenty-one fields have different digests.
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
fn push_graph(out: &mut Vec<u8>, schema: &Schema) {
    // 1. protocol
    push_str(out, &schema.protocol);

    // 2. vertices, keyed by vertex id
    let vertices = sorted(&schema.vertices);
    push_len(out, vertices.len());
    for (id, vertex) in vertices {
        push_name(out, id);
        push_vertex(out, vertex);
    }

    // 3. edges, keyed by the edge itself; the value is the edge kind
    let edges = sorted(&schema.edges);
    push_len(out, edges.len());
    for (edge, kind) in edges {
        push_edge(out, edge);
        push_name(out, kind);
    }

    // 4. hyper_edges, keyed by hyper-edge id
    let hyper_edges = sorted(&schema.hyper_edges);
    push_len(out, hyper_edges.len());
    for (id, hyper_edge) in hyper_edges {
        push_name(out, id);
        push_hyper_edge(out, hyper_edge);
    }

    // 5. constraints, keyed by vertex id; lists in stored order, empties kept
    let constraints = sorted(&schema.constraints);
    push_len(out, constraints.len());
    for (id, list) in constraints {
        push_name(out, id);
        push_len(out, list.len());
        for constraint in list {
            push_constraint(out, constraint);
        }
    }

    // 6. required, keyed by vertex id; lists in stored order, empties kept
    let required = sorted(&schema.required);
    push_len(out, required.len());
    for (id, list) in required {
        push_name(out, id);
        push_len(out, list.len());
        for edge in list {
            push_edge(out, edge);
        }
    }

    // 7. nsids, keyed by vertex id
    let nsids = sorted(&schema.nsids);
    push_len(out, nsids.len());
    for (id, nsid) in nsids {
        push_name(out, id);
        push_name(out, nsid);
    }

    // 8. entries: an ordered family of basepoints, written in stored order
    push_len(out, schema.entries.len());
    for entry in &schema.entries {
        push_name(out, entry);
    }
}

/// Fields 9 to 14: the structural annotations the builder cannot set.
fn push_annotations(out: &mut Vec<u8>, schema: &Schema) {
    // 9. variants, keyed by the parent coproduct vertex id
    let variants = sorted(&schema.variants);
    push_len(out, variants.len());
    for (parent, arms) in variants {
        push_name(out, parent);
        push_len(out, arms.len());
        for arm in arms {
            push_variant(out, arm);
        }
    }

    // 10. orderings, keyed by the edge
    let orderings = sorted(&schema.orderings);
    push_len(out, orderings.len());
    for (edge, position) in orderings {
        push_edge(out, edge);
        out.extend_from_slice(&position.to_le_bytes());
    }

    // 11. recursion_points, keyed by the fixpoint marker's vertex id
    let recursion_points = sorted(&schema.recursion_points);
    push_len(out, recursion_points.len());
    for (mu, point) in recursion_points {
        push_name(out, mu);
        push_name(out, &point.target_vertex);
    }

    // 12. spans, keyed by span id
    let spans = sorted(&schema.spans);
    push_len(out, spans.len());
    for (id, span) in spans {
        push_name(out, id);
        push_name(out, &span.id);
        push_name(out, &span.left);
        push_name(out, &span.right);
    }

    // 13. usage_modes, keyed by the edge
    let usage_modes = sorted(&schema.usage_modes);
    push_len(out, usage_modes.len());
    for (edge, mode) in usage_modes {
        push_edge(out, edge);
        push_usage_mode(out, mode);
    }

    // 14. nominal, keyed by vertex id
    let nominal = sorted(&schema.nominal);
    push_len(out, nominal.len());
    for (id, flag) in nominal {
        push_name(out, id);
        out.push(u8::from(*flag));
    }
}

/// Fields 15 to 18: the enrichment fibres.
fn push_enrichment(out: &mut Vec<u8>, schema: &Schema) {
    // 15. coercions, keyed by (source_kind, target_kind)
    let coercions = sorted(&schema.coercions);
    push_len(out, coercions.len());
    for ((source_kind, target_kind), spec) in coercions {
        push_name(out, source_kind);
        push_name(out, target_kind);
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
fn push_indices(out: &mut Vec<u8>, schema: &Schema) {
    // 19. outgoing, keyed by Edge::src
    push_vertex_index(out, &schema.outgoing);
    // 20. incoming, keyed by Edge::tgt
    push_vertex_index(out, &schema.incoming);

    // 21. between, keyed by (src, tgt)
    let between = sorted(&schema.between);
    push_len(out, between.len());
    for ((src, tgt), bucket) in between {
        push_name(out, src);
        push_name(out, tgt);
        push_edges(out, bucket.as_slice());
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
// Primitives. Every variable-length run carries a u64 little-endian length.
// ---------------------------------------------------------------------------

/// Write a length as eight little-endian bytes.
fn push_len(out: &mut Vec<u8>, len: usize) {
    let wide = u64::try_from(len).unwrap_or(u64::MAX);
    out.extend_from_slice(&wide.to_le_bytes());
}

/// Write a length-prefixed byte string.
fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_len(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// Write a length-prefixed UTF-8 string.
fn push_str(out: &mut Vec<u8>, value: &str) {
    push_bytes(out, value.as_bytes());
}

/// Write a length-prefixed [`Name`].
fn push_name(out: &mut Vec<u8>, name: &Name) {
    push_str(out, name.as_ref());
}

/// Write an optional [`Name`] as a presence byte followed by the name.
fn push_opt_name(out: &mut Vec<u8>, name: Option<&Name>) {
    match name {
        None => out.push(0),
        Some(value) => {
            out.push(1);
            push_name(out, value);
        }
    }
}

// ---------------------------------------------------------------------------
// Schema value types.
// ---------------------------------------------------------------------------

/// Write a vertex: id, kind, optional NSID.
fn push_vertex(out: &mut Vec<u8>, vertex: &Vertex) {
    push_name(out, &vertex.id);
    push_name(out, &vertex.kind);
    push_opt_name(out, vertex.nsid.as_ref());
}

/// Write an edge: source, target, kind, optional label.
fn push_edge(out: &mut Vec<u8>, edge: &Edge) {
    push_name(out, &edge.src);
    push_name(out, &edge.tgt);
    push_name(out, &edge.kind);
    push_opt_name(out, edge.name.as_ref());
}

/// Write a length-prefixed run of edges in stored order.
fn push_edges(out: &mut Vec<u8>, edges: &[Edge]) {
    push_len(out, edges.len());
    for edge in edges {
        push_edge(out, edge);
    }
}

/// Write a hyper-edge, its signature sorted by label.
fn push_hyper_edge(out: &mut Vec<u8>, hyper_edge: &HyperEdge) {
    push_name(out, &hyper_edge.id);
    push_name(out, &hyper_edge.kind);
    let signature = sorted(&hyper_edge.signature);
    push_len(out, signature.len());
    for (label, vertex_id) in signature {
        push_name(out, label);
        push_name(out, vertex_id);
    }
    push_name(out, &hyper_edge.parent_label);
}

/// Write a constraint: sort then value.
fn push_constraint(out: &mut Vec<u8>, constraint: &Constraint) {
    push_name(out, &constraint.sort);
    push_str(out, &constraint.value);
}

/// Write a coproduct arm.
fn push_variant(out: &mut Vec<u8>, variant: &Variant) {
    push_name(out, &variant.id);
    push_name(out, &variant.parent_vertex);
    push_opt_name(out, variant.tag.as_ref());
}

/// Write a use-counting mode as a one-byte discriminant.
fn push_usage_mode(out: &mut Vec<u8>, mode: &UsageMode) {
    out.push(match mode {
        UsageMode::Structural => 0,
        UsageMode::Linear => 1,
        UsageMode::Affine => 2,
    });
}

/// Write a coercion specification.
///
/// The round-trip class is a fieldless enum, so its [`Debug`] rendering is
/// exactly the variant name: total, injective, and stable.
fn push_coercion_spec(out: &mut Vec<u8>, spec: &CoercionSpec) {
    push_expr(out, &spec.forward);
    match &spec.inverse {
        None => out.push(0),
        Some(inverse) => {
            out.push(1);
            push_expr(out, inverse);
        }
    }
    push_str(out, &format!("{:?}", spec.class));
}

/// Write a [`Name`]-keyed map of expressions in ascending key order.
fn push_expr_map<S>(out: &mut Vec<u8>, map: &HashMap<Name, Expr, S>) {
    let items = sorted(map);
    push_len(out, items.len());
    for (key, expr) in items {
        push_name(out, key);
        push_expr(out, expr);
    }
}

/// Write a vertex-keyed adjacency index, each bucket in stored order.
fn push_vertex_index<S>(out: &mut Vec<u8>, index: &HashMap<Name, SmallVec<Edge, 4>, S>) {
    let items = sorted(index);
    push_len(out, items.len());
    for (vertex_id, bucket) in items {
        push_name(out, vertex_id);
        push_edges(out, bucket.as_slice());
    }
}

// ---------------------------------------------------------------------------
// The expression language.
// ---------------------------------------------------------------------------

/// Write a length-prefixed `Arc<str>`.
fn push_arc_str(out: &mut Vec<u8>, value: &std::sync::Arc<str>) {
    push_str(out, value);
}

/// Write an expression, discriminant first.
fn push_expr(out: &mut Vec<u8>, expr: &Expr) {
    match expr {
        Expr::Var(name) => {
            out.push(0);
            push_arc_str(out, name);
        }
        Expr::Lam(param, body) => {
            out.push(1);
            push_arc_str(out, param);
            push_expr(out, body);
        }
        Expr::App(func, arg) => {
            out.push(2);
            push_expr(out, func);
            push_expr(out, arg);
        }
        Expr::Lit(literal) => {
            out.push(3);
            push_literal(out, literal);
        }
        Expr::Record(fields) => {
            out.push(4);
            push_len(out, fields.len());
            for (name, value) in fields {
                push_arc_str(out, name);
                push_expr(out, value);
            }
        }
        Expr::List(items) => {
            out.push(5);
            push_len(out, items.len());
            for item in items {
                push_expr(out, item);
            }
        }
        Expr::Field(base, field) => {
            out.push(6);
            push_expr(out, base);
            push_arc_str(out, field);
        }
        Expr::Index(base, index) => {
            out.push(7);
            push_expr(out, base);
            push_expr(out, index);
        }
        Expr::Match { scrutinee, arms } => {
            out.push(8);
            push_expr(out, scrutinee);
            push_len(out, arms.len());
            for (pattern, body) in arms {
                push_pattern(out, pattern);
                push_expr(out, body);
            }
        }
        Expr::Let { name, value, body } => {
            out.push(9);
            push_arc_str(out, name);
            push_expr(out, value);
            push_expr(out, body);
        }
        Expr::Builtin(op, args) => {
            out.push(10);
            // `BuiltinOp` is a fieldless enum, so its `Debug` rendering is
            // exactly the variant name.
            push_str(out, &format!("{op:?}"));
            push_len(out, args.len());
            for arg in args {
                push_expr(out, arg);
            }
        }
    }
}

/// Write a literal value, discriminant first. Floats go in as raw bits, which
/// is total where a decimal rendering would not be.
fn push_literal(out: &mut Vec<u8>, literal: &Literal) {
    match literal {
        Literal::Bool(value) => {
            out.push(0);
            out.push(u8::from(*value));
        }
        Literal::Int(value) => {
            out.push(1);
            out.extend_from_slice(&value.to_le_bytes());
        }
        Literal::Float(value) => {
            out.push(2);
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        Literal::Str(value) => {
            out.push(3);
            push_str(out, value);
        }
        Literal::Bytes(value) => {
            out.push(4);
            push_bytes(out, value);
        }
        Literal::Null => out.push(5),
        Literal::Record(fields) => {
            out.push(6);
            push_len(out, fields.len());
            for (name, value) in fields {
                push_arc_str(out, name);
                push_literal(out, value);
            }
        }
        Literal::List(items) => {
            out.push(7);
            push_len(out, items.len());
            for item in items {
                push_literal(out, item);
            }
        }
        Literal::Closure { param, body, env } => {
            out.push(8);
            push_arc_str(out, param);
            push_expr(out, body);
            push_len(out, env.len());
            for (name, value) in env.iter() {
                push_arc_str(out, name);
                push_literal(out, value);
            }
        }
    }
}

/// Write a match pattern, discriminant first.
fn push_pattern(out: &mut Vec<u8>, pattern: &Pattern) {
    match pattern {
        Pattern::Wildcard => out.push(0),
        Pattern::Var(name) => {
            out.push(1);
            push_arc_str(out, name);
        }
        Pattern::Lit(literal) => {
            out.push(2);
            push_literal(out, literal);
        }
        Pattern::Record(fields) => {
            out.push(3);
            push_len(out, fields.len());
            for (name, sub) in fields {
                push_arc_str(out, name);
                push_pattern(out, sub);
            }
        }
        Pattern::List(items) => {
            out.push(4);
            push_len(out, items.len());
            for item in items {
                push_pattern(out, item);
            }
        }
        Pattern::Constructor(tag, args) => {
            out.push(5);
            push_arc_str(out, tag);
            push_len(out, args.len());
            for arg in args {
                push_pattern(out, arg);
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
            ..Protocol::default()
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
