//! Cutting a well-formed sub-schema out of a schema.
//!
//! [`induce`] is the one supported way to restrict a [`Schema`] to a subset
//! of its vertices and edges. A [`Schema`] carries twenty-one fields spread
//! over four distinct key spaces (vertex id, [`Edge`], `(kind, kind)` pair,
//! and constraint sort name), and only three of them are derived. Copying
//! `vertices` and `edges` and cloning the rest therefore leaves dangling
//! references in most of the remaining fields: required-edge lists that name
//! removed edges, hyper-edge signatures and recursion points and spans that
//! name removed vertices, coproduct arms whose member vertices are gone, and
//! adjacency indices that hand callers edges into vertices that no longer
//! exist.
//!
//! [`induce`] accounts for every field by its own key space, then re-runs
//! [`validate`](crate::validate) against the protocol and refuses to return a
//! sub-schema that does not pass.
//!
//! The re-validation is a second line of defence rather than the first.
//! [`validate`](crate::validate) checks vertex kinds, edge rules, constraint
//! sorts and required-edge endpoints; it never inspects `hyper_edges`,
//! `variants`, `recursion_points`, `spans` or the adjacency indices. Referential
//! integrity on those is enforced by the field rules below, so a defect in one
//! of them is a defect in this module and not something the re-validation would
//! catch.
//!
//! ## Key spaces, in one table
//!
//! | Key space | Fields |
//! |---|---|
//! | vertex id | `vertices`, `constraints`, `required` (key), `nsids`, `entries`, `variants` (key), `recursion_points`, `nominal`, `mergers`, `defaults` |
//! | [`Edge`] | `edges`, `required` (elements), `orderings`, `usage_modes` |
//! | vertex ids held inside a value | `hyper_edges.signature`, `recursion_points.target_vertex`, `spans.left` and `spans.right`, `variants.id` and `variants.parent_vertex` |
//! | `(kind, kind)` | `coercions` |
//! | constraint sort name | `policies` |
//! | derived from `edges` | `outgoing`, `incoming`, `between` |

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;

use panproto_gat::Name;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::error::SchemaError;
use crate::protocol::Protocol;
use crate::schema::{CoercionSpec, Edge, HyperEdge, RecursionPoint, Schema, Span, Variant, Vertex};
use crate::validate::validate;

/// Induce the sub-schema of `schema` on the vertices `keep_v` and the edges
/// `keep_e`.
///
/// The result is a [`Schema`] whose every field has been restricted in its
/// own key space, whose three adjacency indices have been rebuilt over the
/// surviving edges rather than copied, and which has been checked with
/// [`validate`](crate::validate) against `protocol`.
///
/// `protocol` is a parameter because the function re-validates the result,
/// and [`validate`](crate::validate) needs the protocol's edge rules, vertex
/// kinds and constraint sorts to do so. A [`Schema`] stores only the protocol
/// *name*, so the protocol cannot be recovered from `schema` alone.
///
/// # Selection
///
/// `keep_v` is intersected with `schema.vertices`: ids naming no vertex are
/// ignored. `keep_e` is intersected with `schema.edges` and with the edges
/// whose endpoints are both *surviving vertices*; an edge in `keep_e` with an
/// endpoint outside the apex is silently dropped rather than rejected, so a
/// caller may hand over a whole edge set alongside a shrinking vertex set
/// without pre-filtering it. Testing the endpoints against the surviving
/// vertex map rather than against `keep_v` is what makes induction total: a
/// parent whose edge map named a vertex it did not hold cannot pass that edge
/// on, so the apex is dangling-edge free for *any* parent rather than only for
/// a well-formed one. [`induce_on_vertices`] is the shorthand for the common
/// case where `keep_e` is exactly the induced edge set.
///
/// # Field rules
///
/// Six of the rules are not what a naive restriction would do, and each is a
/// place where a dangling reference would otherwise survive:
///
/// 1. `required` is filtered in *both* of its key spaces: the key by `keep_v`
///    and every element of the inner `Vec<Edge>` by the surviving edge set. A
///    key whose list empties is dropped.
/// 2. `hyper_edges` is retained only when *every* `signature` value survives.
///    The hyper-edge id is its own key space and is never tested against
///    `keep_v`.
/// 3. `recursion_points` is retained only when the key, `mu_id` and
///    `target_vertex` all survive.
/// 4. `spans` is retained only when `left` and `right` both survive. The span
///    id is its own key space.
/// 5. `coercions` is keyed by `(source_kind, target_kind)`, so it is filtered
///    by the *kinds* the surviving vertices carry, never by vertex id.
/// 6. `policies` is keyed by constraint sort name, so it is copied wholesale.
///
/// `entries` keeps its order and is de-duplicated. No basepoint is
/// synthesised when it empties: an unpointed apex is a fact about the cut
/// rather than a defect to paper over, and
/// [`primary_entry`](crate::primary_entry) already supplies a fallback for
/// consumers that need a single root.
///
/// A surviving `variants` key whose arms have all left keeps its entry with
/// an empty arm list, so the apex still records the coproduct as a coproduct.
/// This is the one place where the empty-list convention differs from
/// `required`, whose empty keys are dropped.
///
/// The `edges` map's value is re-derived as `edge.kind` rather than copied,
/// so induction repairs a parent whose edge map had drifted from its keys.
///
/// The three adjacency indices take their *membership* from the filtered edge
/// map, so a parent whose index held an entry its edge map did not cannot pass
/// that entry on, and their *order within a bucket* from the parent's own
/// index. Order is carried rather than canonicalised because it is observable
/// through [`outgoing_edges`](crate::Schema::outgoing_edges) and a consumer
/// that reconstructs source text from a parsed schema reads it, so sorting here
/// would silently reorder the children of every vertex in the apex. A
/// surviving edge the parent's index omitted is appended in ascending
/// [`Edge`] order, which keeps the result a total function of its inputs even
/// when the parent's index was incomplete.
///
/// # Errors
///
/// Returns [`SchemaError::InducedSchemaInvalid`] when
/// [`validate`](crate::validate) reports anything about the result. Induction
/// invents nothing, so every such finding is inherited from the parent.
///
/// # Panics
///
/// In debug builds, panics if the surviving `nsids` map and the surviving
/// [`Vertex::nsid`](crate::Vertex::nsid) fields disagree in either direction,
/// and if a `recursion_points` entry is filed under a key other than its
/// `mu_id`. Both are redundancies that every constructor in the tree keeps in
/// step and that nothing else cross-checks, so a disagreement means a schema
/// arrived by direct field mutation or by deserialisation.
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_schema::{Protocol, SchemaBuilder, induce};
/// use rustc_hash::FxHashSet;
///
/// let protocol = Protocol::default();
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None)?
///     .vertex("kept", "string", None)?
///     .vertex("cut", "string", None)?
///     .edge("root", "kept", "prop", Some("kept"))?
///     .edge("root", "cut", "prop", Some("cut"))?
///     .entry("root")
///     .build()?;
///
/// let keep_v: FxHashSet<Name> = ["root", "kept"].into_iter().map(Name::from).collect();
/// let keep_e: FxHashSet<_> = schema.edges.keys().cloned().collect();
///
/// // The `root -> cut` edge is in `keep_e`, but `cut` is not in `keep_v`,
/// // so induction drops it.
/// let apex = induce(&schema, &protocol, &keep_v, &keep_e)?;
/// assert_eq!(apex.vertex_count(), 2);
/// assert_eq!(apex.edge_count(), 1);
/// assert_eq!(apex.outgoing_edges("root").len(), 1);
/// assert_eq!(apex.entry_vertices(), [Name::from("root")].as_slice());
/// # Ok::<(), panproto_schema::SchemaError>(())
/// ```
pub fn induce<VS: BuildHasher, ES: BuildHasher>(
    schema: &Schema,
    protocol: &Protocol,
    keep_v: &HashSet<Name, VS>,
    keep_e: &HashSet<Edge, ES>,
) -> Result<Schema, SchemaError> {
    // 2. `vertices`: key space is vertex id.
    let vertices = retain_by_vertex(&schema.vertices, keep_v);

    // 3. `edges`: key space is `Edge`. The retained keys are the intersection
    //    of `keep_e` with the edges whose endpoints are both surviving
    //    vertices; the value is re-derived as `edge.kind`.
    let edges: HashMap<Edge, Name> = schema
        .edges
        .keys()
        .filter(|edge| {
            keep_e.contains(*edge)
                && vertices.contains_key(&edge.src)
                && vertices.contains_key(&edge.tgt)
        })
        .map(|edge| (edge.clone(), edge.kind.clone()))
        .collect();

    // 7. `nsids`: key space is vertex id.
    let nsids = retain_by_vertex(&schema.nsids, keep_v);
    debug_assert!(
        nsids_agree_with_vertices(&nsids, &vertices),
        "induce: schema.nsids disagrees with Vertex::nsid"
    );

    // 6. `required`: key space is vertex id, element space is `Edge`.
    let required = induce_required(&schema.required, keep_v, &edges);
    // 10. `orderings`: key space is `Edge`.
    let orderings = retain_by_edge(&schema.orderings, &edges);
    // 13. `usage_modes`: key space is `Edge`.
    let usage_modes = retain_by_edge(&schema.usage_modes, &edges);
    // 19, 20, 21. `outgoing`, `incoming`, `between` take their membership from
    //     the filtered `edges` and their bucket order from the parent's own
    //     indices; neither is copied wholesale.
    let adjacency = build_adjacency(&edges, schema);

    let apex = Schema {
        // 1. `protocol`: the protocol name, copied.
        protocol: schema.protocol.clone(),
        vertices,
        edges,
        // 4. `hyper_edges`: key space is hyper-edge id; the vertex ids live in
        //    `signature`.
        hyper_edges: induce_hyper_edges(&schema.hyper_edges, keep_v),
        // 5. `constraints`: key space is vertex id.
        constraints: retain_by_vertex(&schema.constraints, keep_v),
        required,
        nsids,
        // 8. `entries`: key space is vertex id; ordered and de-duplicated.
        entries: induce_entries(&schema.entries, keep_v),
        // 9. `variants`: key space is vertex id, and `id` and `parent_vertex`
        //    are vertex ids too.
        variants: induce_variants(&schema.variants, keep_v),
        orderings,
        // 11. `recursion_points`: key space is vertex id, and `target_vertex`
        //     is a vertex id.
        recursion_points: induce_recursion_points(&schema.recursion_points, keep_v),
        // 12. `spans`: key space is span id; `left` and `right` are vertex ids.
        spans: induce_spans(&schema.spans, keep_v),
        usage_modes,
        // 14. `nominal`: key space is vertex id.
        nominal: retain_by_vertex(&schema.nominal, keep_v),
        // 15. `coercions`: key space is `(source_kind, target_kind)`.
        coercions: induce_coercions(&schema.coercions, &schema.vertices, keep_v),
        // 16. `mergers`: key space is vertex id.
        mergers: retain_by_vertex(&schema.mergers, keep_v),
        // 17. `defaults`: key space is vertex id.
        defaults: retain_by_vertex(&schema.defaults, keep_v),
        // 18. `policies`: key space is constraint sort name, so nothing in it
        //     is a vertex id and the whole map carries over.
        policies: schema.policies.clone(),
        outgoing: adjacency.outgoing,
        incoming: adjacency.incoming,
        between: adjacency.between,
    };

    let findings = validate(&apex, protocol);
    if findings.is_empty() {
        Ok(apex)
    } else {
        Err(SchemaError::InducedSchemaInvalid { findings })
    }
}

/// Induce the sub-schema of `schema` on `keep_v`, taking `keep_e` to be the
/// induced edge set: every edge of `schema` whose source and target both
/// survive.
///
/// This is the form a morphism search wants, where the vertex set is the
/// search's own decision and the edge set follows from it.
///
/// # Errors
///
/// Returns [`SchemaError::InducedSchemaInvalid`] under exactly the conditions
/// [`induce`] does.
///
/// # Panics
///
/// Under the same debug-only condition as [`induce`].
///
/// # Examples
///
/// ```
/// use panproto_gat::Name;
/// use panproto_schema::{Protocol, SchemaBuilder, induce_on_vertices};
/// use rustc_hash::FxHashSet;
///
/// let protocol = Protocol::default();
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None)?
///     .vertex("kept", "string", None)?
///     .vertex("cut", "string", None)?
///     .edge("root", "kept", "prop", Some("kept"))?
///     .edge("root", "cut", "prop", Some("cut"))?
///     .build()?;
///
/// let keep_v: FxHashSet<Name> = ["root", "kept"].into_iter().map(Name::from).collect();
/// let apex = induce_on_vertices(&schema, &protocol, &keep_v)?;
/// assert_eq!(apex.edge_count(), 1);
/// # Ok::<(), panproto_schema::SchemaError>(())
/// ```
pub fn induce_on_vertices<VS: BuildHasher>(
    schema: &Schema,
    protocol: &Protocol,
    keep_v: &HashSet<Name, VS>,
) -> Result<Schema, SchemaError> {
    let keep_e: FxHashSet<Edge> = schema
        .edges
        .keys()
        .filter(|edge| keep_v.contains(&edge.src) && keep_v.contains(&edge.tgt))
        .cloned()
        .collect();
    induce(schema, protocol, keep_v, &keep_e)
}

/// The edges of `schema` as a deterministic sequence that preserves the order
/// the schema already gives its siblings.
///
/// `schema.edges` is a [`HashMap`], so iterating it yields a hash-seed order
/// that differs between processes. Anything that rebuilds the adjacency
/// indices from that iteration inherits the variation, which is why the
/// derived constructions ([`crate::colimit`], [`crate::normalize`]) take their
/// edge sequence from here instead.
///
/// Order is read from `schema.outgoing`: source vertices are visited in
/// ascending [`Name`] order and each bucket is emitted in its stored order.
/// Since a bucket holds only edges sharing that source, an edge's position
/// among its siblings is exactly the position the schema already recorded, so
/// a schema built by [`SchemaBuilder`](crate::SchemaBuilder) keeps its
/// insertion order. Edges the index does not mention are appended in ascending
/// [`Edge`] order, which keeps the result a total function of `schema` even
/// when the index and the edge map disagree.
///
/// What is preserved is *sibling* order, one source vertex at a time, not the
/// global order the edges were inserted in: two vertices' buckets interleave in
/// ascending source name order rather than in insertion order. That is the
/// order the consumers care about, because a consumer reconstructing source
/// text walks `outgoing_edges` for one vertex at a time. `incoming` buckets are
/// rebuilt in ascending source order for the same reason: nothing reads them
/// for anything but emptiness, and `between` buckets hold edges sharing both
/// endpoints, so their order comes from the source bucket and survives.
pub(crate) fn ordered_edges(schema: &Schema) -> Vec<Edge> {
    let mut sources: Vec<&Name> = schema.outgoing.keys().collect();
    sources.sort_unstable();

    let mut out: Vec<Edge> = Vec::with_capacity(schema.edges.len());
    let mut seen: FxHashSet<&Edge> = FxHashSet::default();

    for src in sources {
        let Some(bucket) = schema.outgoing.get(src) else {
            continue;
        };
        for edge in bucket {
            if edge.src == *src && schema.edges.contains_key(edge) && seen.insert(edge) {
                out.push(edge.clone());
            }
        }
    }

    let mut rest: Vec<&Edge> = schema
        .edges
        .keys()
        .filter(|edge| !seen.contains(*edge))
        .collect();
    rest.sort_unstable();
    out.extend(rest.into_iter().cloned());

    out
}

/// The three derived adjacency indices of a schema.
struct Adjacency {
    /// Edges keyed by [`Edge::src`].
    outgoing: HashMap<Name, SmallVec<Edge, 4>>,
    /// Edges keyed by [`Edge::tgt`].
    incoming: HashMap<Name, SmallVec<Edge, 4>>,
    /// Edges keyed by `(Edge::src, Edge::tgt)`.
    between: HashMap<(Name, Name), SmallVec<Edge, 2>>,
}

/// Rebuild the three adjacency indices over the surviving edge map, carrying
/// the parent's bucket order.
///
/// Membership is read from `edges` alone, so an entry the parent's index held
/// but its edge map did not is dropped. Order within a bucket is read from the
/// parent's index, and a surviving edge no bucket of the parent's index placed
/// correctly is appended in ascending [`Edge`] order. The result is therefore a
/// function of `edges` and `parent` and does not depend on any hash seed: at
/// most one bucket of an index can satisfy the key test for a given edge, so
/// the order in which buckets are visited cannot change the outcome.
fn build_adjacency(edges: &HashMap<Edge, Name>, parent: &Schema) -> Adjacency {
    Adjacency {
        outgoing: restrict_index(&parent.outgoing, edges, |edge| edge.src.clone()),
        incoming: restrict_index(&parent.incoming, edges, |edge| edge.tgt.clone()),
        between: restrict_index(&parent.between, edges, |edge| {
            (edge.src.clone(), edge.tgt.clone())
        }),
    }
}

/// Restrict one adjacency index to the surviving edges, keeping the parent's
/// order within each bucket.
///
/// `key_of` is the index's own keying function, which is what lets an entry
/// filed under the wrong key be rejected rather than carried over.
fn restrict_index<K, const N: usize>(
    index: &HashMap<K, SmallVec<Edge, N>>,
    edges: &HashMap<Edge, Name>,
    key_of: impl Fn(&Edge) -> K,
) -> HashMap<K, SmallVec<Edge, N>>
where
    K: Clone + Eq + std::hash::Hash,
{
    let mut out: HashMap<K, SmallVec<Edge, N>> = HashMap::new();
    let mut placed: FxHashSet<Edge> = FxHashSet::default();

    for (key, bucket) in index {
        for edge in bucket {
            if edges.contains_key(edge) && key_of(edge) == *key && placed.insert(edge.clone()) {
                out.entry(key.clone()).or_default().push(edge.clone());
            }
        }
    }

    let mut omitted: Vec<&Edge> = edges
        .keys()
        .filter(|edge| !placed.contains(*edge))
        .collect();
    omitted.sort_unstable();
    for edge in omitted {
        out.entry(key_of(edge)).or_default().push(edge.clone());
    }

    out
}

/// Whether every `nsids` row agrees with the corresponding
/// [`Vertex::nsid`](crate::Vertex::nsid), in both directions.
///
/// The two are redundant storage of one fact. A row whose vertex declares no
/// NSID, a vertex whose NSID has no row, and a row that contradicts its vertex
/// are all disagreements, and only the last of the three is reachable through a
/// constructor.
fn nsids_agree_with_vertices(
    nsids: &HashMap<Name, Name>,
    vertices: &HashMap<Name, Vertex>,
) -> bool {
    nsids.iter().all(|(id, nsid)| {
        vertices
            .get(id)
            .is_none_or(|v| v.nsid.as_ref() == Some(nsid))
    }) && vertices.iter().all(|(id, vertex)| {
        vertex
            .nsid
            .as_ref()
            .is_none_or(|declared| nsids.get(id) == Some(declared))
    })
}

/// Restrict a vertex-id-keyed map to `keep_v`.
fn retain_by_vertex<V: Clone, S: BuildHasher>(
    map: &HashMap<Name, V>,
    keep_v: &HashSet<Name, S>,
) -> HashMap<Name, V> {
    map.iter()
        .filter(|(id, _)| keep_v.contains(*id))
        .map(|(id, value)| (id.clone(), value.clone()))
        .collect()
}

/// Restrict an [`Edge`]-keyed map to the surviving edge set.
fn retain_by_edge<V: Clone>(
    map: &HashMap<Edge, V>,
    edges: &HashMap<Edge, Name>,
) -> HashMap<Edge, V> {
    map.iter()
        .filter(|(edge, _)| edges.contains_key(*edge))
        .map(|(edge, value)| (edge.clone(), value.clone()))
        .collect()
}

/// Retain a hyper-edge only when every vertex id in its signature survives.
fn induce_hyper_edges<S: BuildHasher>(
    hyper_edges: &HashMap<Name, HyperEdge>,
    keep_v: &HashSet<Name, S>,
) -> HashMap<Name, HyperEdge> {
    hyper_edges
        .iter()
        .filter(|(_, hyper_edge)| hyper_edge.signature.values().all(|v| keep_v.contains(v)))
        .map(|(id, hyper_edge)| (id.clone(), hyper_edge.clone()))
        .collect()
}

/// Filter `required` in both of its key spaces, dropping keys whose edge list
/// empties.
fn induce_required<S: BuildHasher>(
    required: &HashMap<Name, Vec<Edge>>,
    keep_v: &HashSet<Name, S>,
    edges: &HashMap<Edge, Name>,
) -> HashMap<Name, Vec<Edge>> {
    let mut out: HashMap<Name, Vec<Edge>> = HashMap::new();
    for (vertex_id, required_edges) in required {
        if !keep_v.contains(vertex_id) {
            continue;
        }
        let kept: Vec<Edge> = required_edges
            .iter()
            .filter(|edge| edges.contains_key(*edge))
            .cloned()
            .collect();
        if !kept.is_empty() {
            out.insert(vertex_id.clone(), kept);
        }
    }
    out
}

/// Restrict the basepoints to `keep_v`, preserving order and dropping
/// duplicates.
fn induce_entries<S: BuildHasher>(entries: &[Name], keep_v: &HashSet<Name, S>) -> Vec<Name> {
    let mut seen: FxHashSet<Name> = FxHashSet::default();
    entries
        .iter()
        .filter(|id| keep_v.contains(*id))
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect()
}

/// Restrict the coproduct arms. The parent key, each arm's `parent_vertex`
/// and each arm's `id` are all vertex ids.
fn induce_variants<S: BuildHasher>(
    variants: &HashMap<Name, Vec<Variant>>,
    keep_v: &HashSet<Name, S>,
) -> HashMap<Name, Vec<Variant>> {
    variants
        .iter()
        .filter(|(parent, _)| keep_v.contains(*parent))
        .map(|(parent, arms)| {
            let kept: Vec<Variant> = arms
                .iter()
                .filter(|arm| keep_v.contains(&arm.parent_vertex) && keep_v.contains(&arm.id))
                .cloned()
                .collect();
            (parent.clone(), kept)
        })
        .collect()
}

/// Retain a fixpoint marker only when its key, its `mu_id` and its
/// `target_vertex` all survive.
///
/// The key clause looks redundant beside the `mu_id` clause and is not: the
/// map's key space is vertex id, so a key outside `keep_v` would be a dangling
/// key even when the value it points at is intact. The two clauses coincide
/// under the invariant that a marker is filed under its own `mu_id`, which
/// every constructor in the tree maintains and which the debug assertion below
/// pins.
///
/// # Panics
///
/// In debug builds, if a marker is filed under a key other than its `mu_id`.
fn induce_recursion_points<S: BuildHasher>(
    recursion_points: &HashMap<Name, RecursionPoint>,
    keep_v: &HashSet<Name, S>,
) -> HashMap<Name, RecursionPoint> {
    debug_assert!(
        recursion_points
            .iter()
            .all(|(mu, point)| *mu == point.mu_id),
        "induce: a recursion point is filed under a key other than its mu_id"
    );
    recursion_points
        .iter()
        .filter(|(mu, point)| {
            keep_v.contains(*mu)
                && keep_v.contains(&point.mu_id)
                && keep_v.contains(&point.target_vertex)
        })
        .map(|(mu, point)| (mu.clone(), point.clone()))
        .collect()
}

/// Retain a span only when both of its legs land on surviving vertices. The
/// span id is its own key space and is not tested.
fn induce_spans<S: BuildHasher>(
    spans: &HashMap<Name, Span>,
    keep_v: &HashSet<Name, S>,
) -> HashMap<Name, Span> {
    spans
        .iter()
        .filter(|(_, span)| keep_v.contains(&span.left) && keep_v.contains(&span.right))
        .map(|(id, span)| (id.clone(), span.clone()))
        .collect()
}

/// Retain a coercion only when both of its kinds are still carried by some
/// surviving vertex.
fn induce_coercions<S: BuildHasher>(
    coercions: &HashMap<(Name, Name), CoercionSpec>,
    vertices: &HashMap<Name, Vertex>,
    keep_v: &HashSet<Name, S>,
) -> HashMap<(Name, Name), CoercionSpec> {
    let surviving_kinds: FxHashSet<&Name> = vertices
        .iter()
        .filter(|(id, _)| keep_v.contains(*id))
        .map(|(_, vertex)| &vertex.kind)
        .collect();

    coercions
        .iter()
        .filter(|((source_kind, target_kind), _)| {
            surviving_kinds.contains(source_kind) && surviving_kinds.contains(target_kind)
        })
        .map(|(pair, spec)| (pair.clone(), spec.clone()))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use super::*;
    use crate::builder::SchemaBuilder;
    use crate::error::ValidationError;
    use crate::schema::{Constraint, UsageMode};
    use panproto_expr::{Expr, Literal};
    use panproto_gat::CoercionClass;

    fn protocol() -> Protocol {
        Protocol {
            name: "fixture".to_owned(),
            ..Protocol::default()
        }
    }

    fn expr(tag: &str) -> Expr {
        Expr::Lit(Literal::Str(tag.to_owned()))
    }

    fn coercion(tag: &str) -> CoercionSpec {
        CoercionSpec {
            forward: expr(tag),
            inverse: None,
            class: CoercionClass::Opaque,
        }
    }

    fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
        Edge {
            src: Name::from(src),
            tgt: Name::from(tgt),
            kind: Name::from(kind),
            name: name.map(Name::from),
        }
    }

    fn names(ids: &[&str]) -> FxHashSet<Name> {
        ids.iter().copied().map(Name::from).collect()
    }

    /// A schema in which every one of the twenty-one fields is non-empty.
    ///
    /// The vertex kinds are chosen so that `integer` is carried by exactly one
    /// vertex (`cut`), which is what lets the coercion assertions distinguish
    /// filtering by kind from filtering by vertex id.
    fn fixture() -> Schema {
        let protocol = protocol();
        let e_keep = edge("root", "kept", "prop", Some("kept"));
        let e_cut = edge("root", "cut", "prop", Some("cut"));

        let mut signature_kept: HashMap<String, String> = HashMap::new();
        signature_kept.insert("parent".to_owned(), "root".to_owned());
        signature_kept.insert("child".to_owned(), "kept".to_owned());
        let mut signature_cut: HashMap<String, String> = HashMap::new();
        signature_cut.insert("parent".to_owned(), "root".to_owned());
        signature_cut.insert("child".to_owned(), "cut".to_owned());

        let mut schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", Some("com.example.root"))
            .expect("root")
            .vertex("kept", "string", None)
            .expect("kept")
            .vertex("cut", "integer", Some("com.example.cut"))
            .expect("cut")
            .vertex("mu", "mu", None)
            .expect("mu")
            .vertex("mu-dangling", "mu", None)
            .expect("mu-dangling")
            .vertex("union", "union", None)
            .expect("union")
            .vertex("arm", "string", None)
            .expect("arm")
            .vertex("arm-cut", "string", None)
            .expect("arm-cut")
            .edge("root", "kept", "prop", Some("kept"))
            .expect("root -> kept")
            .edge("root", "cut", "prop", Some("cut"))
            .expect("root -> cut")
            .edge("union", "arm", "variant", Some("arm"))
            .expect("union -> arm")
            .edge("union", "arm-cut", "variant", Some("arm-cut"))
            .expect("union -> arm-cut")
            .edge("mu", "root", "unfold", None)
            .expect("mu -> root")
            .edge("mu-dangling", "cut", "unfold", None)
            .expect("mu-dangling -> cut")
            .hyper_edge("he-kept", "record", signature_kept, "parent")
            .expect("he-kept")
            .hyper_edge("he-cut", "record", signature_cut, "parent")
            .expect("he-cut")
            .constraint("root", "maxLength", "10")
            .constraint("cut", "maxLength", "5")
            .required("root", vec![e_keep.clone(), e_cut.clone()])
            .required("cut", vec![e_cut.clone()])
            .coercion("object", "string", coercion("object->string"))
            .coercion("string", "integer", coercion("string->integer"))
            .coercion("root", "kept", coercion("vertex-id-shaped"))
            .merger("root", expr("merge-root"))
            .merger("cut", expr("merge-cut"))
            .default_expr("kept", expr("default-kept"))
            .default_expr("cut", expr("default-cut"))
            .policy("maxLength", expr("policy-maxLength"))
            .policy("cut", expr("policy-named-like-a-vertex"))
            .entry("root")
            .entry("cut")
            .build()
            .expect("build");

        // A duplicate basepoint, to exercise de-duplication.
        schema.entries.push(Name::from("root"));

        // The six fields the builder cannot set.
        schema.variants.insert(
            Name::from("union"),
            vec![
                Variant {
                    id: Name::from("arm"),
                    parent_vertex: Name::from("union"),
                    tag: Some(Name::from("a")),
                },
                Variant {
                    id: Name::from("arm-cut"),
                    parent_vertex: Name::from("union"),
                    tag: Some(Name::from("b")),
                },
            ],
        );
        schema.variants.insert(
            Name::from("cut"),
            vec![Variant {
                id: Name::from("arm"),
                parent_vertex: Name::from("cut"),
                tag: None,
            }],
        );
        schema.orderings.insert(e_keep.clone(), 0);
        schema.orderings.insert(e_cut.clone(), 1);
        schema.recursion_points.insert(
            Name::from("mu"),
            RecursionPoint {
                mu_id: Name::from("mu"),
                target_vertex: Name::from("root"),
            },
        );
        schema.recursion_points.insert(
            Name::from("mu-dangling"),
            RecursionPoint {
                mu_id: Name::from("mu-dangling"),
                target_vertex: Name::from("cut"),
            },
        );
        schema.spans.insert(
            Name::from("span-kept"),
            Span {
                id: Name::from("span-kept"),
                left: Name::from("root"),
                right: Name::from("kept"),
            },
        );
        schema.spans.insert(
            Name::from("span-cut"),
            Span {
                id: Name::from("span-cut"),
                left: Name::from("root"),
                right: Name::from("cut"),
            },
        );
        schema.usage_modes.insert(e_keep, UsageMode::Linear);
        schema.usage_modes.insert(e_cut, UsageMode::Affine);
        schema.nominal.insert(Name::from("root"), true);
        schema.nominal.insert(Name::from("cut"), false);

        schema
    }

    /// Every field of the fixture is non-empty, so the induction test below
    /// really does exercise all twenty-one.
    #[test]
    fn fixture_populates_every_field() {
        let schema = fixture();
        assert!(!schema.protocol.is_empty());
        assert!(!schema.vertices.is_empty());
        assert!(!schema.edges.is_empty());
        assert!(!schema.hyper_edges.is_empty());
        assert!(!schema.constraints.is_empty());
        assert!(!schema.required.is_empty());
        assert!(!schema.nsids.is_empty());
        assert!(!schema.entries.is_empty());
        assert!(!schema.variants.is_empty());
        assert!(!schema.orderings.is_empty());
        assert!(!schema.recursion_points.is_empty());
        assert!(!schema.spans.is_empty());
        assert!(!schema.usage_modes.is_empty());
        assert!(!schema.nominal.is_empty());
        assert!(!schema.coercions.is_empty());
        assert!(!schema.mergers.is_empty());
        assert!(!schema.defaults.is_empty());
        assert!(!schema.policies.is_empty());
        assert!(!schema.outgoing.is_empty());
        assert!(!schema.incoming.is_empty());
        assert!(!schema.between.is_empty());
    }

    /// The central test: induce on a proper subset and check every field.
    #[test]
    fn induction_restricts_every_field() {
        let protocol = protocol();
        let schema = fixture();
        let keep_v = names(&["root", "kept", "mu", "mu-dangling", "union", "arm"]);
        let apex = induce_on_vertices(&schema, &protocol, &keep_v).expect("induce");

        let e_keep = edge("root", "kept", "prop", Some("kept"));
        let e_cut = edge("root", "cut", "prop", Some("cut"));
        let e_arm = edge("union", "arm", "variant", Some("arm"));
        let e_mu = edge("mu", "root", "unfold", None);

        // 1. protocol
        assert_eq!(apex.protocol, schema.protocol);

        // 2. vertices
        assert_eq!(apex.vertices.len(), 6);
        assert!(apex.has_vertex("root"));
        assert!(!apex.has_vertex("cut"));
        assert!(!apex.has_vertex("arm-cut"));

        // 3. edges. `root -> cut`, `union -> arm-cut` and `mu-dangling -> cut`
        //    each lose an endpoint.
        assert_eq!(apex.edges.len(), 3);
        assert!(apex.edges.contains_key(&e_keep));
        assert!(apex.edges.contains_key(&e_arm));
        assert!(apex.edges.contains_key(&e_mu));
        assert!(!apex.edges.contains_key(&e_cut));
        assert!(
            apex.edges.iter().all(|(key, kind)| *kind == key.kind),
            "the edge map's value must be the edge kind"
        );

        // 4. hyper_edges. `he-cut` names `cut` in its signature.
        assert!(apex.hyper_edges.contains_key("he-kept"));
        assert!(!apex.hyper_edges.contains_key("he-cut"));

        // 5. constraints
        assert!(apex.constraints.contains_key("root"));
        assert!(!apex.constraints.contains_key("cut"));

        // 6. required. The `cut` key leaves, and the surviving `root` key has
        //    its inner list filtered down to the one surviving edge.
        assert!(!apex.required.contains_key("cut"));
        assert_eq!(
            apex.required.get("root").map(Vec::as_slice),
            Some([e_keep.clone()].as_slice()),
            "the inner Vec<Edge> of `required` must be filtered too"
        );

        // 7. nsids
        assert!(apex.nsids.contains_key("root"));
        assert!(!apex.nsids.contains_key("cut"));

        // 8. entries. Order preserved, `cut` dropped, duplicate removed.
        assert_eq!(apex.entries, vec![Name::from("root")]);

        // 9. variants. The `cut` key leaves; under `union` the `arm-cut` arm
        //    leaves because its `id` did.
        assert!(!apex.variants.contains_key("cut"));
        let union_arms = apex.variants.get("union").expect("union arms");
        assert_eq!(union_arms.len(), 1);
        assert_eq!(union_arms[0].id, Name::from("arm"));

        // 10. orderings
        assert!(apex.orderings.contains_key(&e_keep));
        assert!(!apex.orderings.contains_key(&e_cut));

        // 11. recursion_points. `mu-dangling` survives as a vertex but its
        //     target does not, so the fixpoint goes.
        assert!(apex.recursion_points.contains_key("mu"));
        assert!(
            !apex.recursion_points.contains_key("mu-dangling"),
            "a fixpoint whose target left must not survive"
        );

        // 12. spans. `span-cut` has a dangling right leg.
        assert!(apex.spans.contains_key("span-kept"));
        assert!(!apex.spans.contains_key("span-cut"));

        // 13. usage_modes
        assert_eq!(apex.usage_modes.get(&e_keep), Some(&UsageMode::Linear));
        assert!(!apex.usage_modes.contains_key(&e_cut));

        // 14. nominal
        assert_eq!(apex.nominal.get("root"), Some(&true));
        assert!(!apex.nominal.contains_key("cut"));

        // 15. coercions, filtered by kind and never by vertex id. `integer` is
        //     carried only by `cut`, so `(string, integer)` goes; `(root,
        //     kept)` is a pair of vertex ids, which are nobody's kind, so it
        //     goes too even though both vertices survive.
        let surviving: Vec<(Name, Name)> = apex.coercions.keys().cloned().collect();
        assert_eq!(surviving.len(), 1, "surviving coercions: {surviving:?}");
        assert!(
            apex.coercions
                .contains_key(&(Name::from("object"), Name::from("string")))
        );
        assert!(
            !apex
                .coercions
                .contains_key(&(Name::from("string"), Name::from("integer")))
        );
        assert!(
            !apex
                .coercions
                .contains_key(&(Name::from("root"), Name::from("kept"))),
            "coercion keys are kinds, so a vertex-id-shaped key must not survive"
        );

        // 16. mergers
        assert!(apex.mergers.contains_key("root"));
        assert!(!apex.mergers.contains_key("cut"));

        // 17. defaults
        assert!(apex.defaults.contains_key("kept"));
        assert!(!apex.defaults.contains_key("cut"));

        // 18. policies. Sort names, copied wholesale; the `cut` key is a sort
        //     name that happens to spell a dropped vertex id.
        assert_eq!(apex.policies.len(), schema.policies.len());
        assert!(apex.policies.contains_key("cut"));

        // 19, 20, 21. The indices equal a fresh index over the filtered edges.
        let fresh = build_adjacency(&apex.edges, &apex);
        assert_eq!(apex.outgoing, fresh.outgoing);
        assert_eq!(apex.incoming, fresh.incoming);
        assert_eq!(apex.between, fresh.between);
        assert!(
            apex.outgoing
                .values()
                .flat_map(|bucket| bucket.as_slice().iter())
                .all(|e| apex.has_vertex(&e.src) && apex.has_vertex(&e.tgt)),
            "no index entry may name a dropped vertex"
        );

        // And the apex validates.
        assert!(validate(&apex, &protocol).is_empty());
    }

    #[test]
    fn identity_induction_is_the_identity() {
        let protocol = protocol();
        let schema = fixture();
        let keep_v: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
        let keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
        let apex = induce(&schema, &protocol, &keep_v, &keep_e).expect("induce");

        assert_eq!(apex.protocol, schema.protocol);
        assert_eq!(apex.vertices, schema.vertices);
        assert_eq!(apex.edges, schema.edges);
        assert_eq!(apex.hyper_edges, schema.hyper_edges);
        assert_eq!(apex.constraints, schema.constraints);
        assert_eq!(apex.required, schema.required);
        assert_eq!(apex.nsids, schema.nsids);
        // The fixture's basepoint list carries a duplicate, which induction
        // removes; the surviving order is first-occurrence order.
        assert_eq!(apex.entries, vec![Name::from("root"), Name::from("cut")]);
        assert_eq!(apex.variants, schema.variants);
        assert_eq!(apex.orderings, schema.orderings);
        assert_eq!(apex.recursion_points, schema.recursion_points);
        assert_eq!(apex.spans, schema.spans);
        assert_eq!(apex.usage_modes, schema.usage_modes);
        assert_eq!(apex.nominal, schema.nominal);
        // Induction is the identity on `coercions` only for keys that name a
        // kind some vertex actually carries. The fixture's `(root, kept)` key
        // names two vertex ids, which are nobody's kind, so even the identity
        // cut drops it: the by-kind rule is unconditional.
        let mut expected_coercions = schema.coercions.clone();
        expected_coercions.remove(&(Name::from("root"), Name::from("kept")));
        assert_eq!(apex.coercions, expected_coercions);
        assert_eq!(apex.mergers, schema.mergers);
        assert_eq!(apex.defaults, schema.defaults);
        assert_eq!(apex.policies, schema.policies);

        // The indices agree with the parent's exactly, order included: the
        // apex carries the parent's bucket order rather than a canonical one.
        assert_eq!(apex.outgoing, schema.outgoing);
        assert_eq!(apex.incoming, schema.incoming);
        assert_eq!(apex.between, schema.between);
    }

    /// The parent's declaration order within an adjacency bucket survives a
    /// cut. Sorting the buckets instead would reorder the children of every
    /// vertex in the apex, which `panproto_io::cst_extract` reads to
    /// reconstruct source text.
    #[test]
    fn induction_carries_the_parents_bucket_order() {
        let protocol = protocol();
        // Insert the edges in descending name order, which is the reverse of
        // the order a sorted rebuild would produce.
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .vertex("leaf", "string", None)
            .expect("leaf")
            .edge("root", "leaf", "prop", Some("z"))
            .expect("z")
            .edge("root", "leaf", "prop", Some("y"))
            .expect("y")
            .edge("root", "leaf", "prop", Some("x"))
            .expect("x")
            .build()
            .expect("build");

        let names = |s: &Schema| -> Vec<Name> {
            s.outgoing_edges("root")
                .iter()
                .filter_map(|e| e.name.clone())
                .collect()
        };
        assert_eq!(names(&schema), ["z", "y", "x"].map(Name::from));

        let keep_v: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
        let apex = induce_on_vertices(&schema, &protocol, &keep_v).expect("induce");
        assert_eq!(
            names(&apex),
            names(&schema),
            "the identity cut must not reorder an adjacency bucket"
        );

        // And a cut that removes one edge keeps the survivors in their
        // original relative order rather than resorting them.
        let mut keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
        keep_e.remove(&edge("root", "leaf", "prop", Some("y")));
        let cut = induce(&schema, &protocol, &keep_v, &keep_e).expect("induce");
        assert_eq!(names(&cut), ["z", "x"].map(Name::from));
    }

    /// A parent whose edge map names a vertex it does not hold must not hand
    /// that edge, or an index entry for it, to the apex. Every `Schema` field
    /// is public and the type is `Deserialize`, so such a parent is reachable.
    #[test]
    fn an_edge_with_a_phantom_endpoint_never_reaches_the_apex() {
        let protocol = protocol();
        let mut schema = fixture();
        let phantom = edge("phantom", "kept", "prop", Some("p"));
        schema.edges.insert(phantom.clone(), Name::from("prop"));
        schema
            .outgoing
            .entry(Name::from("phantom"))
            .or_default()
            .push(phantom.clone());
        schema
            .incoming
            .entry(Name::from("kept"))
            .or_default()
            .push(phantom.clone());

        // `phantom` is named in `keep_v` even though it is not a vertex.
        let keep_v = names(&["root", "kept", "phantom"]);
        let keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
        let apex = induce(&schema, &protocol, &keep_v, &keep_e).expect("induce");

        assert!(!apex.has_vertex("phantom"));
        assert!(
            !apex.edges.contains_key(&phantom),
            "an edge whose endpoint is not a vertex of the apex must be dropped"
        );
        assert!(
            apex.outgoing_edges("phantom").is_empty(),
            "and so must its adjacency entry"
        );
        assert!(
            apex.edges
                .keys()
                .chain(apex.outgoing.values().flat_map(SmallVec::as_slice))
                .chain(apex.incoming.values().flat_map(SmallVec::as_slice))
                .chain(apex.between.values().flat_map(SmallVec::as_slice))
                .all(|e| apex.has_vertex(&e.src) && apex.has_vertex(&e.tgt)),
            "no edge anywhere in the apex may name a vertex the apex lacks"
        );
    }

    /// A parent whose index omitted a live edge still gets a complete index
    /// back, which is what keeps induction a total function of its inputs.
    #[test]
    fn an_index_entry_the_parent_omitted_is_supplied() {
        let protocol = protocol();
        let mut schema = fixture();
        let e_keep = edge("root", "kept", "prop", Some("kept"));
        schema.outgoing.remove("root");

        let keep_v = names(&["root", "kept"]);
        let apex = induce_on_vertices(&schema, &protocol, &keep_v).expect("induce");
        assert_eq!(apex.outgoing_edges("root"), [e_keep].as_slice());
    }

    #[test]
    fn empty_induction_is_empty_but_valid() {
        let protocol = protocol();
        let schema = fixture();
        let apex = induce_on_vertices(&schema, &protocol, &FxHashSet::default()).expect("induce");

        assert!(apex.vertices.is_empty());
        assert!(apex.edges.is_empty());
        assert!(apex.hyper_edges.is_empty());
        assert!(apex.constraints.is_empty());
        assert!(apex.required.is_empty());
        assert!(apex.nsids.is_empty());
        assert!(apex.entries.is_empty());
        assert!(apex.variants.is_empty());
        assert!(apex.orderings.is_empty());
        assert!(apex.recursion_points.is_empty());
        assert!(apex.spans.is_empty());
        assert!(apex.usage_modes.is_empty());
        assert!(apex.nominal.is_empty());
        assert!(apex.coercions.is_empty());
        assert!(apex.mergers.is_empty());
        assert!(apex.defaults.is_empty());
        assert!(apex.outgoing.is_empty());
        assert!(apex.incoming.is_empty());
        assert!(apex.between.is_empty());
        // `policies` is keyed by sort name, so an empty vertex set leaves it
        // untouched.
        assert_eq!(apex.policies, schema.policies);
        assert!(validate(&apex, &protocol).is_empty());
    }

    #[test]
    fn induction_is_idempotent() {
        let protocol = protocol();
        let schema = fixture();
        let keep_v = names(&["root", "kept", "mu", "union", "arm"]);
        let once = induce_on_vertices(&schema, &protocol, &keep_v).expect("once");
        let twice = induce_on_vertices(&once, &protocol, &keep_v).expect("twice");

        assert_eq!(once.vertices, twice.vertices);
        assert_eq!(once.edges, twice.edges);
        assert_eq!(once.hyper_edges, twice.hyper_edges);
        assert_eq!(once.constraints, twice.constraints);
        assert_eq!(once.required, twice.required);
        assert_eq!(once.nsids, twice.nsids);
        assert_eq!(once.entries, twice.entries);
        assert_eq!(once.variants, twice.variants);
        assert_eq!(once.orderings, twice.orderings);
        assert_eq!(once.recursion_points, twice.recursion_points);
        assert_eq!(once.spans, twice.spans);
        assert_eq!(once.usage_modes, twice.usage_modes);
        assert_eq!(once.nominal, twice.nominal);
        assert_eq!(once.coercions, twice.coercions);
        assert_eq!(once.mergers, twice.mergers);
        assert_eq!(once.defaults, twice.defaults);
        assert_eq!(once.policies, twice.policies);
        assert_eq!(once.outgoing, twice.outgoing);
        assert_eq!(once.incoming, twice.incoming);
        assert_eq!(once.between, twice.between);
    }

    #[test]
    fn keep_e_is_intersected_not_rejected() {
        let protocol = protocol();
        let schema = fixture();
        let keep_v = names(&["root", "kept"]);
        // Hand `induce` the parent's whole edge set even though most of it now
        // dangles.
        let keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
        let apex = induce(&schema, &protocol, &keep_v, &keep_e).expect("induce");

        assert_eq!(apex.edge_count(), 1);
        assert!(
            apex.edges
                .contains_key(&edge("root", "kept", "prop", Some("kept")))
        );
    }

    #[test]
    fn unknown_ids_in_keep_sets_are_ignored() {
        let protocol = protocol();
        let schema = fixture();
        let keep_v = names(&["root", "kept", "no-such-vertex"]);
        let mut keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
        keep_e.insert(edge("root", "kept", "no-such-kind", None));

        let apex = induce(&schema, &protocol, &keep_v, &keep_e).expect("induce");
        assert_eq!(apex.vertex_count(), 2);
        assert_eq!(apex.edge_count(), 1);
    }

    #[test]
    fn an_inherited_defect_surfaces_as_induced_schema_invalid() {
        let protocol = Protocol {
            name: "strict".to_owned(),
            constraint_sorts: vec!["format".to_owned()],
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None)
            .expect("root")
            .constraint("root", "maxLength", "10")
            .build()
            .expect("build");

        let err = induce_on_vertices(&schema, &protocol, &names(&["root"]))
            .expect_err("the inherited constraint sort must be reported");
        match err {
            SchemaError::InducedSchemaInvalid { findings } => {
                assert_eq!(findings.len(), 1);
                assert!(matches!(
                    findings[0],
                    ValidationError::InvalidConstraintSort { .. }
                ));
            }
            other => panic!("expected InducedSchemaInvalid, got {other:?}"),
        }
    }

    #[test]
    fn constraints_survive_with_their_values() {
        let protocol = protocol();
        let schema = fixture();
        let apex =
            induce_on_vertices(&schema, &protocol, &names(&["root", "kept"])).expect("induce");
        assert_eq!(
            apex.constraints_for("root"),
            [Constraint {
                sort: Name::from("maxLength"),
                value: "10".to_owned(),
            }]
            .as_slice()
        );
    }

    // ── Adjacency-index determinism ──────────────────────────────────────
    //
    // `Schema::edges` is a `HashMap`, and `RandomState` draws a fresh key per
    // instance, so two schemas built from the same input in the same process
    // already iterate their edge maps differently. Any index built by walking
    // `edges.keys()` therefore varies run to run, which breaks reproducibility
    // of anything downstream that reads a bucket in order. These tests pin the
    // three indices to the order the schema was built with.

    /// Twelve sibling edges out of `root`, inserted in ascending numeric
    /// order.
    ///
    /// The ids mirror the parser's `$N` scheme, so ascending numeric order and
    /// ascending lexicographic order disagree (`n10 < n2` as strings). A
    /// caller that sorts the edges rather than preserving insertion order
    /// therefore fails these tests instead of silently passing them.
    fn wide_fixture() -> Schema {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None::<&str>)
            .expect("root");
        for i in 0..12 {
            builder = builder
                .vertex(&format!("n{i}"), "string", None::<&str>)
                .expect("leaf")
                .edge("root", &format!("n{i}"), "prop", Some(&format!("f{i}")))
                .expect("edge");
        }
        builder.build().expect("build")
    }

    /// The same shape, but with every leaf reached through a ref vertex, so
    /// [`crate::normalize::normalize`] actually collapses something instead of
    /// short-circuiting on an empty ref set.
    fn wide_ref_fixture() -> Schema {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol)
            .vertex("root", "object", None::<&str>)
            .expect("root");
        for i in 0..12 {
            builder = builder
                .vertex(&format!("r{i}"), "ref", None::<&str>)
                .expect("ref")
                .vertex(&format!("n{i}"), "string", None::<&str>)
                .expect("leaf")
                .edge("root", &format!("r{i}"), "prop", Some(&format!("f{i}")))
                .expect("edge")
                .edge(
                    &format!("r{i}"),
                    &format!("n{i}"),
                    "ref-target",
                    None::<&str>,
                )
                .expect("ref edge");
        }
        builder.build().expect("build")
    }

    /// Every bucket of all three indices, keyed so the snapshot compares
    /// bucket *order* without being perturbed by map iteration order.
    fn index_snapshot(schema: &Schema) -> Vec<(String, Vec<Edge>)> {
        let mut rows: Vec<(String, Vec<Edge>)> = Vec::new();
        for (k, v) in &schema.outgoing {
            rows.push((format!("out/{k}"), v.as_slice().to_vec()));
        }
        for (k, v) in &schema.incoming {
            rows.push((format!("in/{k}"), v.as_slice().to_vec()));
        }
        for ((s, t), v) in &schema.between {
            rows.push((format!("btw/{s}->{t}"), v.as_slice().to_vec()));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    /// The `name` labels on `root`'s outgoing bucket, which is the order a
    /// consumer walking `outgoing_edges` observes.
    fn out_labels(schema: &Schema) -> Vec<String> {
        schema
            .outgoing_edges("root")
            .iter()
            .map(|e| e.name.clone().unwrap_or_default().to_string())
            .collect()
    }

    fn insertion_labels() -> Vec<String> {
        (0..12).map(|i| format!("f{i}")).collect()
    }

    #[test]
    fn ordered_edges_preserves_sibling_order() {
        let schema = wide_fixture();
        let seq: Vec<String> = ordered_edges(&schema)
            .iter()
            .map(|e| e.name.clone().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            seq,
            insertion_labels(),
            "ordered_edges must reproduce the builder's sibling order, not a sort"
        );
    }

    #[test]
    fn ordered_edges_orders_by_source_and_keeps_siblings_as_stored() {
        // `wide_fixture` hangs every edge off one vertex, so it cannot tell the
        // claim apart from the stronger one it is easy to read into the name:
        // with a single source, sibling order and global insertion order are
        // the same sequence. Two sources separate them.
        //
        // What is guaranteed is per source: sources ascend by name, and each
        // one's bucket is emitted as stored. Global insertion order is *not*
        // preserved and is not meant to be, because the consumers that read an
        // order read `outgoing_edges` for one vertex at a time.
        let protocol = protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("zeta", "object", None::<&str>)
            .expect("zeta")
            .vertex("alpha", "object", None::<&str>)
            .expect("alpha")
            .vertex("leaf", "string", None::<&str>)
            .expect("leaf")
            .vertex("other", "string", None::<&str>)
            .expect("other")
            .edge("zeta", "leaf", "prop", Some("first"))
            .expect("first")
            .edge("alpha", "leaf", "prop", Some("second"))
            .expect("second")
            .edge("zeta", "other", "prop", Some("third"))
            .expect("third")
            .edge("alpha", "other", "prop", Some("fourth"))
            .expect("fourth")
            .build()
            .expect("build");

        let seq: Vec<String> = ordered_edges(&schema)
            .iter()
            .map(|e| e.name.clone().unwrap_or_default().to_string())
            .collect();
        assert_eq!(
            seq,
            vec![
                "second".to_owned(),
                "fourth".to_owned(),
                "first".to_owned(),
                "third".to_owned(),
            ],
            "alpha's bucket in its own order, then zeta's"
        );

        // Sibling order, which is the claim that is load-bearing, does hold on
        // both buckets.
        assert_eq!(out_labels_of(&schema, "zeta"), vec!["first", "third"]);
        assert_eq!(out_labels_of(&schema, "alpha"), vec!["second", "fourth"]);
    }

    /// The `name` labels on one vertex's outgoing bucket.
    fn out_labels_of(schema: &Schema, vertex: &str) -> Vec<String> {
        schema
            .outgoing_edges(vertex)
            .iter()
            .map(|e| e.name.clone().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn ordered_edges_is_independent_of_the_hash_seed() {
        let first = ordered_edges(&wide_fixture());
        for _ in 0..8 {
            assert_eq!(
                ordered_edges(&wide_fixture()),
                first,
                "ordered_edges varied across two equal schemas, so it is reading a hash order"
            );
        }
    }

    #[test]
    fn builder_indices_are_independent_of_the_hash_seed() {
        let first = index_snapshot(&wide_fixture());
        for _ in 0..8 {
            assert_eq!(index_snapshot(&wide_fixture()), first);
        }
        assert_eq!(out_labels(&wide_fixture()), insertion_labels());
    }

    #[test]
    fn colimit_indices_are_independent_of_the_hash_seed() {
        use crate::colimit::{SchemaOverlap, schema_pushout};

        let pushout = || {
            let left = wide_fixture();
            let right = SchemaBuilder::new(&protocol())
                .vertex("other", "object", None::<&str>)
                .expect("other")
                .vertex("other.z", "string", None::<&str>)
                .expect("z")
                .edge("other", "other.z", "prop", Some("z"))
                .expect("edge")
                .build()
                .expect("build");
            let (apex, _, _) =
                schema_pushout(&left, &right, &SchemaOverlap::default()).expect("pushout");
            apex
        };

        let first = index_snapshot(&pushout());
        for _ in 0..8 {
            assert_eq!(
                index_snapshot(&pushout()),
                first,
                "pushout bucket order varied across runs"
            );
        }
        assert_eq!(
            out_labels(&pushout()),
            insertion_labels(),
            "the pushout must carry the left schema's sibling order through"
        );
    }

    #[test]
    fn normalize_indices_are_independent_of_the_hash_seed() {
        use crate::normalize::normalize;

        let first = index_snapshot(&normalize(&wide_ref_fixture()));
        for _ in 0..8 {
            assert_eq!(
                index_snapshot(&normalize(&wide_ref_fixture())),
                first,
                "normalized bucket order varied across runs"
            );
        }
        assert_eq!(
            out_labels(&normalize(&wide_ref_fixture())),
            insertion_labels(),
            "collapsing refs must keep the order the edges were declared in"
        );
    }

    #[test]
    fn induce_indices_are_independent_of_the_hash_seed() {
        let induced = || {
            let schema = wide_fixture();
            let keep_v: FxHashSet<Name> = schema.vertices.keys().cloned().collect();
            let keep_e: FxHashSet<Edge> = schema.edges.keys().cloned().collect();
            induce(&schema, &protocol(), &keep_v, &keep_e).expect("induce")
        };

        let first = index_snapshot(&induced());
        for _ in 0..8 {
            assert_eq!(index_snapshot(&induced()), first);
        }
        assert_eq!(out_labels(&induced()), insertion_labels());
    }

    #[test]
    fn every_path_producing_a_schema_agrees_on_bucket_order() {
        use crate::colimit::{SchemaOverlap, schema_pushout};
        use crate::normalize::normalize;

        let built = wide_fixture();
        let expected = insertion_labels();

        // The builder is the reference: it stores edges in insertion order.
        assert_eq!(out_labels(&built), expected, "builder");

        // Normalizing a ref-free schema is the identity on edges.
        assert_eq!(out_labels(&normalize(&built)), expected, "normalize");

        // Collapsing refs reproduces the same order on the collapsed schema.
        assert_eq!(
            out_labels(&normalize(&wide_ref_fixture())),
            expected,
            "normalize over refs"
        );

        // A pushout along a total overlap returns the schema itself.
        let overlap = SchemaOverlap {
            vertex_pairs: built
                .vertices
                .keys()
                .map(|v| (v.clone(), v.clone()))
                .collect(),
            edge_pairs: built.edges.keys().map(|e| (e.clone(), e.clone())).collect(),
        };
        let (apex, _, _) = schema_pushout(&built, &built, &overlap).expect("pushout");
        assert_eq!(out_labels(&apex), expected, "colimit");

        // Inducing on everything is the identity.
        let keep_v: FxHashSet<Name> = built.vertices.keys().cloned().collect();
        let keep_e: FxHashSet<Edge> = built.edges.keys().cloned().collect();
        let ind = induce(&built, &protocol(), &keep_v, &keep_e).expect("induce");
        assert_eq!(out_labels(&ind), expected, "induce");

        // And the composite of all three still agrees, which is the property
        // a span's right leg depends on.
        let composite = induce(&normalize(&apex), &protocol(), &keep_v, &keep_e)
            .expect("induce of normalize of colimit");
        assert_eq!(
            out_labels(&composite),
            expected,
            "induce ∘ normalize ∘ colimit"
        );
    }
}
