//! Asymmetric lens operations: `get` and `put` with complement tracking.
//!
//! The `get` direction runs the restrict pipeline (forward migration) while
//! capturing everything that was discarded into a [`Complement`]. The `put`
//! direction restores the original source instance from a (possibly modified)
//! view plus the complement.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::identity::{
    ComplementShape, IdentityCarrier, IdentityEnvelope, IdentityPolicy, IdentityStatus,
    compose_identities,
};
use panproto_inst::{Fan, Node, WInstance, wtype_restrict};
use panproto_schema::Edge;

use crate::Lens;
use crate::error::{IdKind, LensError};

/// The complement: data discarded by `get`, needed by `put` to restore the
/// original source instance.
///
/// The canonical definition lives in [`panproto_inst::Complement`] and is
/// shared with the restrict pipeline; it is re-exported here because the
/// asymmetric lens is its primary producer and consumer. Composition of
/// complements as a partial monoid is provided by [`ComplementCompose`],
/// which lives in this crate because its conflict semantics are expressed in
/// terms of [`LensError`].
pub use panproto_inst::Complement;

/// Partial-commutative-monoid composition of [`Complement`]s.
///
/// This is an extension trait (rather than inherent methods) because the
/// merge's conflict signalling is expressed with [`LensError`], which lives
/// in `panproto-lens`, while the [`Complement`] type itself lives in
/// `panproto-inst`.
pub trait ComplementCompose: Sized {
    /// Compose two complements as a *partial commutative monoid* (in the
    /// sense of separation-algebra / Clog-style algebras): the merge is
    /// defined exactly when the two complements agree on every shared key,
    /// in which case the result records everything lost by both.
    ///
    /// [`Complement::empty`] is a two-sided identity. On the domain of
    /// definition (pairs satisfying [`is_compatible`](Self::is_compatible)),
    /// composition is associative and commutative; this is verified by the
    /// `complement_partial_monoid_*` proptests.
    ///
    /// # Errors
    ///
    /// Returns [`LensError::ComplementConflict`] when both operands carry
    /// distinct entries on the same key (the partial-monoid's
    /// disjointness/agreement condition fails). Returns
    /// [`LensError::Identity`] when the two identities cannot be combined,
    /// by the table at
    /// [`compose_identities`]:
    /// complements of different lenses, a stamped complement with a legacy
    /// one, two legacy complements with different fingerprints, or any
    /// complement that carries data but no identity.
    fn compose(&self, other: &Self) -> Result<Self, LensError>;

    /// Returns `true` exactly when [`compose`](Self::compose) would succeed.
    ///
    /// Equivalent to running `compose` and discarding the result, but avoids
    /// the allocation when only the predicate is needed.
    fn is_compatible(&self, other: &Self) -> bool;
}

impl ComplementCompose for Complement {
    fn compose(&self, other: &Self) -> Result<Self, LensError> {
        let (identity, source_fingerprint) = compose_identities(self, other)?;

        let mut dropped_nodes = self.dropped_nodes.clone();
        merge_keyed_with_eq(
            &mut dropped_nodes,
            &other.dropped_nodes,
            "dropped_nodes",
            node_equiv,
        )?;

        let mut dropped_arcs = self.dropped_arcs.clone();
        merge_vec_dedup(&mut dropped_arcs, &other.dropped_arcs);

        let mut dropped_fans = self.dropped_fans.clone();
        merge_vec_dedup(&mut dropped_fans, &other.dropped_fans);

        let mut contraction_choices = self.contraction_choices.clone();
        merge_keyed_with_eq(
            &mut contraction_choices,
            &other.contraction_choices,
            "contraction_choices",
            PartialEq::eq,
        )?;

        let mut original_parent = self.original_parent.clone();
        merge_keyed_with_eq(
            &mut original_parent,
            &other.original_parent,
            "original_parent",
            PartialEq::eq,
        )?;

        let mut original_extra_fields = self.original_extra_fields.clone();
        merge_keyed_with_eq(
            &mut original_extra_fields,
            &other.original_extra_fields,
            "original_extra_fields",
            extra_fields_equiv,
        )?;

        let mut arc_edges = self.arc_edges.clone();
        merge_keyed_with_eq(&mut arc_edges, &other.arc_edges, "arc_edges", PartialEq::eq)?;

        let mut original_values = self.original_values.clone();
        merge_keyed_with_eq(
            &mut original_values,
            &other.original_values,
            "original_values",
            |a, b| presence_equiv(a.as_ref(), b.as_ref()),
        )?;

        let mut synthesized_nodes = self.synthesized_nodes.clone();
        synthesized_nodes.extend(other.synthesized_nodes.iter().copied());

        let mut contracted_into = self.contracted_into.clone();
        merge_keyed_with_eq(
            &mut contracted_into,
            &other.contracted_into,
            "contracted_into",
            PartialEq::eq,
        )?;

        let mut original_anchors = self.original_anchors.clone();
        merge_keyed_with_eq(
            &mut original_anchors,
            &other.original_anchors,
            "original_anchors",
            PartialEq::eq,
        )?;

        // Arc order: keep this operand's sequence and append any pair the
        // other records that this one does not, so a merged complement
        // still describes a single consistent ordering. Membership goes
        // through a set: `compose` is called repeatedly (the associativity
        // proptests compose in a loop), and a linear scan per pair makes
        // that quadratic in the arc count.
        let mut arc_order = self.arc_order.clone();
        let mut present: HashSet<(u32, u32)> = arc_order.iter().copied().collect();
        for pair in &other.arc_order {
            if present.insert(*pair) {
                arc_order.push(*pair);
            }
        }

        Ok(Self {
            dropped_nodes,
            dropped_arcs,
            dropped_fans,
            contraction_choices,
            original_parent,
            source_fingerprint,
            original_extra_fields,
            arc_edges,
            arc_order,
            original_values,
            synthesized_nodes,
            contracted_into,
            original_anchors,
            identity,
        })
    }

    fn is_compatible(&self, other: &Self) -> bool {
        self.compose(other).is_ok()
    }
}

fn merge_keyed_with_eq<K, V, F>(
    target: &mut HashMap<K, V>,
    source: &HashMap<K, V>,
    kind: &'static str,
    eq: F,
) -> Result<(), LensError>
where
    K: Eq + std::hash::Hash + Clone + std::fmt::Debug,
    V: Clone,
    F: Fn(&V, &V) -> bool,
{
    for (k, v) in source {
        match target.get(k) {
            None => {
                target.insert(k.clone(), v.clone());
            }
            Some(existing) if eq(existing, v) => {}
            Some(_) => {
                return Err(LensError::ComplementConflict {
                    kind,
                    key: format!("{k:?}"),
                });
            }
        }
    }
    Ok(())
}

fn merge_vec_dedup<T: Clone + PartialEq>(target: &mut Vec<T>, source: &[T]) {
    for item in source {
        if !target.contains(item) {
            target.push(item.clone());
        }
    }
}

/// Total structural equivalence on [`Node`].
///
/// `Node` does not derive `PartialEq` (its `Value` field carries
/// `f64`/`HashMap` content). We compare each field directly,
/// delegating to [`value_equiv`] for `Value`-bearing positions so that
/// `f64::NaN` compares equal to itself (bit-equality) and `HashMap`
/// values compare order-independently.
fn node_equiv(a: &Node, b: &Node) -> bool {
    a.id == b.id
        && a.anchor == b.anchor
        && a.discriminator == b.discriminator
        && a.position == b.position
        && a.shape == b.shape
        && presence_equiv(a.value.as_ref(), b.value.as_ref())
        && extra_fields_equiv(&a.extra_fields, &b.extra_fields)
        && extra_fields_equiv(&a.annotations, &b.annotations)
}

/// Total structural equivalence on a `HashMap<String, Value>`.
pub(crate) fn extra_fields_equiv(
    a: &HashMap<String, panproto_inst::value::Value>,
    b: &HashMap<String, panproto_inst::value::Value>,
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .all(|(k, v)| b.get(k).is_some_and(|w| value_equiv(v, w)))
}

/// Total structural equivalence on `Option<FieldPresence>`.
pub(crate) fn presence_equiv(
    a: Option<&panproto_inst::value::FieldPresence>,
    b: Option<&panproto_inst::value::FieldPresence>,
) -> bool {
    use panproto_inst::value::FieldPresence;
    match (a, b) {
        (None, None)
        | (Some(FieldPresence::Absent), Some(FieldPresence::Absent))
        | (Some(FieldPresence::Null), Some(FieldPresence::Null)) => true,
        (Some(FieldPresence::Present(x)), Some(FieldPresence::Present(y))) => value_equiv(x, y),
        _ => false,
    }
}

/// IEEE-754 equality with NaN reflexive: `+0.0 == -0.0`, every NaN
/// equals every NaN. Used by [`value_equiv`] so that complement
/// composition treats a NaN-bearing node as self-equal (the derived
/// `PartialEq` on `f64` would say NaN ≠ NaN and falsely report a
/// conflict).
///
/// Implemented via `partial_cmp`: it returns `None` for NaN
/// arguments (caught explicitly above) and `Some(Ordering::Equal)`
/// for `+0.0 == -0.0` and every other IEEE-754-equal pair.
fn float_equiv(x: f64, y: f64) -> bool {
    if x.is_nan() && y.is_nan() {
        return true;
    }
    x.partial_cmp(&y) == Some(std::cmp::Ordering::Equal)
}

/// Total structural equivalence on [`Value`](panproto_inst::value::Value).
///
/// Differs from `Value`'s derived `PartialEq` in exactly one place:
/// `Float(NaN)` compares equal to `Float(NaN)`, so complement
/// composition does not spuriously reject a node against itself.
/// All other floats fall through to ordinary `==`, which means
/// `+0.0` and `-0.0` compare equal (matching IEEE-754 numeric
/// equality) — using bit-equality there would over-discriminate.
///
/// Distinct numeric variants (`Int(1)` vs `Float(1.0)`) remain
/// distinct: panproto preserves the source-format distinction by
/// design, and conflating them would lose round-trip fidelity.
///
/// `Unknown`/`Opaque` field-bag comparisons recurse, so NaN-equality
/// propagates through nesting.
pub(crate) fn value_equiv(
    a: &panproto_inst::value::Value,
    b: &panproto_inst::value::Value,
) -> bool {
    use panproto_inst::value::Value;
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => float_equiv(*x, *y),
        (Value::List(xs), Value::List(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| value_equiv(x, y))
        }
        (Value::Unknown(m1), Value::Unknown(m2)) => extra_fields_equiv(m1, m2),
        (
            Value::Opaque {
                type_: t1,
                fields: f1,
            },
            Value::Opaque {
                type_: t2,
                fields: f2,
            },
        ) => t1 == t2 && extra_fields_equiv(f1, f2),
        _ => a == b,
    }
}

/// The source anchor a view anchor maps back to, read off the vertex remap.
///
/// A target vertex with exactly one explicit preimage maps back to it; one
/// with none was not renamed and maps back to itself. One with several has
/// no inverse, and `put` takes the anchor from
/// [`Complement::original_anchors`] instead, which `get` fills for exactly
/// the nodes this inverse would get wrong.
struct ReverseAnchors {
    preimages: HashMap<Name, Vec<Name>>,
}

impl ReverseAnchors {
    fn new(vertex_remap: &HashMap<Name, Name>) -> Self {
        let mut preimages: HashMap<Name, Vec<Name>> = HashMap::new();
        for (src, tgt) in vertex_remap {
            preimages.entry(tgt.clone()).or_default().push(src.clone());
        }
        for candidates in preimages.values_mut() {
            candidates.sort();
        }
        Self { preimages }
    }

    /// The source anchor of `view_anchor`, or every candidate when there is
    /// more than one.
    fn source_anchor<'a>(&'a self, view_anchor: &'a Name) -> Result<&'a Name, &'a [Name]> {
        match self.preimages.get(view_anchor).map(Vec::as_slice) {
            None | Some([]) => Ok(view_anchor),
            Some([only]) => Ok(only),
            Some(many) => Err(many),
        }
    }
}

/// The source edges behind each view edge, read off the source schema.
///
/// A source edge's image is its explicit entry in the edge remap, or, for an
/// edge the remap does not name, the same kind and label across remapped
/// endpoints; this indexes the source schema's edges by that image.
struct EdgePreimages {
    preimages: HashMap<Edge, Vec<Edge>>,
}

impl EdgePreimages {
    fn new(lens: &Lens) -> Self {
        let mut preimages: HashMap<Edge, Vec<Edge>> = HashMap::new();
        for source in lens.src_schema.edges.keys() {
            preimages
                .entry(edge_image(lens, source))
                .or_default()
                .push(source.clone());
        }
        for sources in preimages.values_mut() {
            sources.sort();
        }
        Self { preimages }
    }

    /// The source edges whose image is `view_edge`, in ascending order.
    fn of(&self, view_edge: &Edge) -> &[Edge] {
        self.preimages.get(view_edge).map_or(&[], Vec::as_slice)
    }

    /// The distinct source anchors the edges behind `view_edge` point
    /// into, in ascending order.
    fn target_anchors(&self, view_edge: &Edge) -> Vec<&Name> {
        let mut anchors: Vec<&Name> = self.of(view_edge).iter().map(|edge| &edge.tgt).collect();
        anchors.sort();
        anchors.dedup();
        anchors
    }
}

/// Forward lens direction: restrict the source instance to the target view
/// and capture the complement.
///
/// This runs `wtype_restrict` and then computes the set difference between
/// the source and result to populate the complement.
///
/// The complement is stamped with the lens's
/// [`identity`](Lens::identity), which `put` checks.
///
/// # Errors
///
/// Returns `LensError::Restrict` if the underlying restrict pipeline fails.
pub fn get(lens: &Lens, instance: &WInstance) -> Result<(WInstance, Complement), LensError> {
    get_with_identity(lens, instance, &lens.identity(ComplementShape::WType))
}

/// [`get`] with the lens's envelope already computed, for callers that run
/// many `get`s through one lens.
pub(crate) fn get_with_identity(
    lens: &Lens,
    instance: &WInstance,
    envelope: &IdentityEnvelope,
) -> Result<(WInstance, Complement), LensError> {
    let view = wtype_restrict(instance, &lens.src_schema, &lens.tgt_schema, &lens.compiled)?;

    // Compute complement: everything in source not in view
    let mut dropped_nodes = HashMap::new();
    for (&id, node) in &instance.nodes {
        if !view.nodes.contains_key(&id) {
            dropped_nodes.insert(id, node.clone());
        }
    }

    let mut dropped_arcs = Vec::new();
    for arc in &instance.arcs {
        let (parent, child, _) = arc;
        if !view.nodes.contains_key(parent) || !view.nodes.contains_key(child) {
            dropped_arcs.push(arc.clone());
        }
    }

    // Capture contraction choices: for each arc in the view that connects
    // nodes that were not directly connected in the source, record the
    // resolver decision.
    let mut contraction_choices = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        // Check if this arc existed in the original
        let was_direct = instance
            .arcs
            .iter()
            .any(|(p, c, _)| p == parent && c == child);
        if !was_direct {
            contraction_choices.insert((*parent, *child), edge.clone());
        }
    }

    // Capture original parent mapping for all surviving nodes
    let mut original_parent = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(&parent) = instance.parent_map.get(&id) {
            original_parent.insert(id, parent);
        }
    }

    // Capture fans whose parent or any children were dropped
    let mut dropped_fans = Vec::new();
    for fan in &instance.fans {
        let parent_survives = view.nodes.contains_key(&fan.parent);
        let all_children_survive = fan
            .children
            .values()
            .all(|node_id| view.nodes.contains_key(node_id));
        if !parent_survives || !all_children_survive {
            dropped_fans.push(fan.clone());
        }
    }

    let (original_extra_fields, original_values) =
        capture_pre_transform_state(lens, instance, &view);

    // Record the exact edge for every arc in the source instance whose
    // parent and child both survive. This makes `put` deterministic when
    // the source schema has parallel edges between the same vertex pair.
    let mut arc_edges = HashMap::new();
    for (parent, child, edge) in &instance.arcs {
        if view.nodes.contains_key(parent) && view.nodes.contains_key(child) {
            arc_edges.insert((*parent, *child), edge.clone());
        }
    }

    // The source anchor of every surviving node the remap's inverse would
    // not recover: a node whose view anchor has several preimages, or a
    // single preimage that is not where it came from.
    let reverse = ReverseAnchors::new(&lens.compiled.vertex_remap);
    let mut original_anchors = HashMap::new();
    for (&id, view_node) in &view.nodes {
        if let Some(source_node) = instance.nodes.get(&id)
            && reverse.source_anchor(&view_node.anchor) != Ok(&source_node.anchor)
        {
            original_anchors.insert(id, source_node.anchor.clone());
        }
    }

    // Capture node ids that appear in the view but not in the source
    // instance. These were synthesized by `wtype_restrict` to satisfy a
    // nest-style `expansion_path` in the compiled migration (a direct
    // source arc expanded into a multi-hop target path). `put` drops
    // them when collapsing back to the source.
    let synthesized_nodes: std::collections::HashSet<u32> = view
        .nodes
        .keys()
        .copied()
        .filter(|id| !instance.nodes.contains_key(id))
        .collect();

    // The source's arcs in order. Array order is read straight off this
    // sequence when the instance is serialized, so it has to survive the
    // round trip.
    let arc_order: Vec<(u32, u32)> = instance
        .arcs
        .iter()
        .map(|(parent, child, _)| (*parent, *child))
        .collect();

    let mut complement = Complement {
        dropped_nodes,
        dropped_arcs,
        dropped_fans,
        contraction_choices,
        original_parent,
        source_fingerprint: 0,
        original_extra_fields,
        arc_edges,
        arc_order,
        original_values,
        synthesized_nodes,
        // The lens `get` path records no ancestor-contraction fibre; that
        // field is populated only by the restrict pipeline.
        contracted_into: HashMap::new(),
        original_anchors,
        identity: None,
    };
    complement.stamp(envelope.clone(), &lens.src_schema);

    Ok((view, complement))
}

/// Backward lens direction: restore a source instance from a (possibly
/// modified) view and the complement.
///
/// The complement provides the dropped nodes and arcs; the view provides
/// the (potentially modified) surviving data. Together they reconstruct
/// the full source instance.
///
/// The complement's identity is checked first, under
/// [`IdentityPolicy::AllowLegacy`]: a complement stamped by this lens, a
/// legacy complement whose fingerprint matches this lens's source schema,
/// and an empty complement are accepted. [`put_checked`] takes the policy
/// explicitly and reports the status.
///
/// # Errors
///
/// Returns [`LensError::Identity`] if the complement's identity is refused,
/// [`LensError::ComplementMismatch`] if the complement is inconsistent with
/// the view, [`LensError::IdCollision`] if a view node has the id of a node
/// the complement holds, and [`LensError::AmbiguousReverseAnchor`] if a
/// node added in the view, or moved in the view away from the anchor `get`
/// gave it, has no single source anchor.
pub fn put(lens: &Lens, view: &WInstance, complement: &Complement) -> Result<WInstance, LensError> {
    put_with_identity(
        lens,
        view,
        complement,
        &lens.identity(ComplementShape::WType),
        IdentityPolicy::AllowLegacy,
    )
    .map(|(source, _)| source)
}

/// [`put`] under an explicit identity policy, reporting how the complement
/// was bound to the lens.
///
/// # Errors
///
/// As [`put`], with the identity check made under `policy`.
pub fn put_checked(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    policy: IdentityPolicy,
) -> Result<(WInstance, IdentityStatus), LensError> {
    put_with_identity(
        lens,
        view,
        complement,
        &lens.identity(ComplementShape::WType),
        policy,
    )
}

/// [`put_checked`] with the lens's envelope already computed, for callers
/// that run many `put`s through one lens.
pub(crate) fn put_with_identity(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    envelope: &IdentityEnvelope,
    policy: IdentityPolicy,
) -> Result<(WInstance, IdentityStatus), LensError> {
    let status = crate::identity::verify_with_identity(complement, lens, envelope, policy)?;
    reassemble(lens, view, complement).map(|source| (source, status))
}

/// The source anchor of view node `id`, whose view anchor is `view_anchor`
/// and whose incoming view arc carries `view_edge`.
///
/// The anchor the complement recorded applies only while the view still
/// holds the node where `get` put it: the remap sends the recorded anchor
/// to `view_anchor`, and the node's arc still carries the image of the edge
/// `get` recorded for it. A view edit that moved the node, or relabelled
/// the arc into it, is followed back instead, so the edit survives the
/// round trip. For a node whose arc `get` recorded, the view edge is read
/// back first: the source edges whose image it is name the node's anchor
/// through their targets, which can tell apart the preimages of a merged
/// anchor that the vertex remap alone cannot. A node the view added, or a
/// view edge that is the image of no source edge, falls back to the vertex
/// remap's inverse, keeping the recorded anchor while it still fits. Several candidates are reported as
/// [`LensError::AmbiguousReverseAnchor`] rather than picked from, unless
/// the recorded anchor is one of them.
fn restored_anchor<'a>(
    lens: &Lens,
    reverse_anchors: &'a ReverseAnchors,
    edge_preimages: &'a EdgePreimages,
    complement: &'a Complement,
    id: u32,
    view_anchor: &'a Name,
    view_arc: Option<(u32, &Edge)>,
) -> Result<&'a Name, LensError> {
    let recorded = complement.original_anchors.get(&id).filter(|recorded| {
        lens.compiled
            .vertex_remap
            .get(*recorded)
            .unwrap_or(recorded)
            == view_anchor
    });
    let recorded_edge = view_arc.and_then(|(parent, _)| complement.arc_edges.get(&(parent, id)));
    let edge_kept = match (view_arc, recorded_edge) {
        (Some((_, view_edge)), Some(recorded_edge)) => {
            edge_image(lens, recorded_edge) == *view_edge
        }
        _ => true,
    };
    if edge_kept && let Some(recorded) = recorded {
        return Ok(recorded);
    }

    // Only anchors the node's own view anchor is the image of: an arc whose
    // edge points elsewhere says nothing about where this node belongs.
    // Only for a node whose arc `get` recorded: a node the view added has
    // no source to be faithful to, and takes its anchor from the vertex
    // remap alone.
    let mut by_edge = view_arc
        .filter(|_| recorded_edge.is_some())
        .map(|(_, view_edge)| edge_preimages.target_anchors(view_edge))
        .unwrap_or_default();
    by_edge
        .retain(|anchor| lens.compiled.vertex_remap.get(*anchor).unwrap_or(anchor) == view_anchor);
    match by_edge.as_slice() {
        [only] => return Ok(only),
        [] => {}
        many => {
            // The source anchor the complement recorded settles it, when
            // it is one of the anchors the edge could have come from.
            let from_record = complement
                .original_anchors
                .get(&id)
                .filter(|recorded| many.contains(recorded));
            return from_record.ok_or_else(|| LensError::AmbiguousReverseAnchor {
                node: id,
                candidates: many.iter().map(|anchor| (*anchor).clone()).collect(),
            });
        }
    }

    recorded.map_or_else(
        || {
            reverse_anchors
                .source_anchor(view_anchor)
                .map_err(|candidates| LensError::AmbiguousReverseAnchor {
                    node: id,
                    candidates: candidates.to_vec(),
                })
        },
        Ok,
    )
}

/// The backward direction proper, once the complement's identity is
/// accepted.
fn reassemble(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<WInstance, LensError> {
    // `put` inverts `get` on the image of `get`, and only there. A
    // complement that records no parent for any of the view's non-root
    // nodes cannot have come from a `get` of a source with this shape: it
    // is outside that image, and the reassembly below would rebuild the
    // node set with no arcs between the nodes, which serializes to an empty
    // record. Returning that silently is worse than refusing, because the
    // caller cannot tell a successful reconstruction of an empty record
    // from the total loss of a populated one.
    let view_has_arcs = view.arcs.iter().any(|(_, child, _)| *child != view.root);
    if view_has_arcs
        && !view
            .arcs
            .iter()
            .any(|(_, child, _)| complement.original_parent.contains_key(child))
    {
        return Err(LensError::ComplementMismatch {
            detail: format!(
                "complement records no parent for any of the view's {} arcs, so it did not come \
                 from a projection of a source with this shape; reconstructing from it would \
                 return an empty record",
                view.arcs.len()
            ),
        });
    }

    // Start with all nodes from the view (un-remap anchors back to source).
    // Skip nodes that were synthesized during forward eval by the
    // nest-style `expansion_path` mechanism: they exist only in the view,
    // not in the source, so collapsing them back means dropping them.
    let mut nodes = HashMap::new();
    let reverse_remap = build_reverse_remap(&lens.compiled.vertex_remap);
    let reverse_anchors = ReverseAnchors::new(&lens.compiled.vertex_remap);
    let edge_preimages = EdgePreimages::new(lens);
    // The arc into each view node: its parent and the edge it carries. A
    // W-type node has at most one; the first is kept if a malformed view
    // has more, so the choice is the same in every process.
    let mut view_arc_into: HashMap<u32, (u32, &Edge)> = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        view_arc_into.entry(*child).or_insert((*parent, edge));
    }

    // Visited in ascending id order so that, of several nodes whose anchor
    // cannot be reversed, the one reported is the same in every process.
    let mut view_ids: Vec<u32> = view.nodes.keys().copied().collect();
    view_ids.sort_unstable();
    for id in view_ids {
        let node = &view.nodes[&id];
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let mut restored_node = node.clone();
        restored_node.anchor.clone_from(restored_anchor(
            lens,
            &reverse_anchors,
            &edge_preimages,
            complement,
            id,
            &node.anchor,
            view_arc_into.get(&id).copied(),
        )?);
        // Restore original extra_fields.
        //
        // When a complement snapshot is present, start from the snapshot
        // (so fields dropped by the forward pass and lossy computed
        // fields are restored exactly), then propagate any view edits
        // back through the inverse of each forward transform. This
        // preserves user edits in the view while still honoring the
        // snapshot as the source of truth for information that the
        // forward pass threw away.
        //
        // When there is no snapshot (externally constructed complements
        // or composed lenses that did not propagate one), fall back to
        // applying inverse transforms to the view's fields directly.
        let transforms = lens.compiled.field_transforms.get(&restored_node.anchor);
        if let Some(original_fields) = complement.original_extra_fields.get(&id) {
            let view_fields = restored_node.extra_fields.clone();
            restored_node.extra_fields.clone_from(original_fields);
            if let Some(transforms) = transforms {
                propagate_view_edits_through_inverse(&mut restored_node, &view_fields, transforms);
            }
        } else if let Some(transforms) = transforms {
            apply_inverse_transforms(&mut restored_node, transforms);
        }
        // Restore original node.value for __value__ coercions
        if let Some(original_val) = complement.original_values.get(&id) {
            restored_node.value.clone_from(original_val);
        }
        nodes.insert(id, restored_node);
    }

    invert_computations_on_child_scalars(lens, view, &mut nodes);

    // Re-insert dropped nodes from complement. A view node already holding
    // one of their ids was added in the view under an id the source still
    // uses; restoring the dropped node would silently overwrite it.
    let mut dropped_ids: Vec<u32> = complement.dropped_nodes.keys().copied().collect();
    dropped_ids.sort_unstable();
    for id in dropped_ids {
        if nodes.contains_key(&id) {
            return Err(LensError::IdCollision {
                kind: IdKind::Node,
                id,
            });
        }
        nodes.insert(id, complement.dropped_nodes[&id].clone());
    }

    // Rebuild arcs: start with original parent relationships for view nodes,
    // then add back dropped arcs.
    //
    // A given child can be reached through two distinct restoration paths:
    // its `original_parent` entry (restored below for surviving view nodes)
    // and a `dropped_arcs` entry (restored when a sort containing that arc
    // was dropped). When a combinator both drops a sort and keeps that
    // sort's child in the view (e.g. `hoist_field`, which drops the
    // intermediate `profile` sort while hoisting its `name` child), the same
    // `(parent, child)` arc is produced by both paths. Emitting it twice
    // gives the parent two identical outgoing arcs, which the JSON encoder
    // reads as a repeated-edge list signal and serializes as a duplicated
    // `Value::List` instead of the original record. We therefore dedupe by
    // `(parent, child)`: a W-type instance has at most one arc per
    // parent/child pair, and genuine list nodes are distinguished by having
    // multiple arcs to *distinct* children, which this key preserves.
    let arcs = rebuild_source_arcs(lens, view, complement, &nodes);

    // Reconstruct fans: start with the view's fans (un-remapping vertex
    // references), then re-insert dropped fans from the complement whose
    // parent and children are all present in the restored node set.
    let mut fans: Vec<Fan> = view
        .fans
        .iter()
        .map(|fan| {
            let mut restored_fan = fan.clone();
            // Un-remap the hyper-edge ID if needed
            if let Some(original_he) = reverse_remap.get(fan.hyper_edge_id.as_str()) {
                restored_fan.hyper_edge_id = original_he.to_string();
            }
            restored_fan
        })
        .collect();

    // Re-insert dropped fans whose nodes are all present after restoration
    for fan in &complement.dropped_fans {
        let parent_present = nodes.contains_key(&fan.parent);
        let all_children_present = fan
            .children
            .values()
            .all(|node_id| nodes.contains_key(node_id));
        if parent_present && all_children_present {
            fans.push(fan.clone());
        }
    }

    // The root's restored anchor is the source's schema root; the remap's
    // inverse is the fallback when the root is not among the nodes.
    let schema_root = nodes.get(&view.root).map_or_else(
        || {
            reverse_anchors
                .source_anchor(&view.schema_root)
                .map_or_else(|_| view.schema_root.clone(), Clone::clone)
        },
        |root| root.anchor.clone(),
    );

    Ok(WInstance::new(nodes, arcs, fans, view.root, schema_root))
}

/// A node's pre-transform `extra_fields`, keyed by node id.
type ExtraFieldSnapshots = HashMap<u32, HashMap<String, panproto_inst::value::Value>>;

/// A node's pre-coercion leaf value, keyed by node id.
type ValueSnapshots = HashMap<u32, Option<panproto_inst::value::FieldPresence>>;

/// Snapshot the pre-transform state of every node a field transform
/// touches: its `extra_fields`, and its leaf value when a `__value__`
/// coercion applies.
///
/// This is what lets the backward direction restore information the
/// forward pass overwrote or discarded, rather than inferring it.
fn capture_pre_transform_state(
    lens: &Lens,
    instance: &WInstance,
    view: &WInstance,
) -> (ExtraFieldSnapshots, ValueSnapshots) {
    // Capture pre-transform extra_fields for nodes that had field_transforms
    let mut original_extra_fields = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(source_node) = instance.nodes.get(&id) {
            if lens
                .compiled
                .field_transforms
                .contains_key(&source_node.anchor)
            {
                original_extra_fields.insert(id, source_node.extra_fields.clone());
            }
        }
    }

    // Capture pre-coercion node.value for nodes that had __value__ field transforms
    let mut original_values = HashMap::new();
    for &id in view.nodes.keys() {
        if let Some(source_node) = instance.nodes.get(&id) {
            if lens
                .compiled
                .field_transforms
                .get(&source_node.anchor)
                .is_some_and(|ts| {
                    ts.iter().any(|t| {
                        matches!(t, panproto_inst::FieldTransform::ApplyExpr { key, .. } if key == "__value__")
                    })
                })
            {
                original_values.insert(id, source_node.value.clone());
            }
        }
    }

    (original_extra_fields, original_values)
}

/// Rebuild the source's arcs from the view and complement.
///
/// Order is the point. The children of a collection node are its elements
/// in sequence, so serialization reads array order straight off this list.
/// Both restoration paths (a surviving child recorded in `original_parent`,
/// and a dropped arc) therefore have to be replayed in the source's order
/// rather than one after the other: running them in sequence lands every
/// dropped arc after the surviving ones instead of back where it was, and
/// iterating `original_parent` (a `HashMap`) permutes them besides.
///
/// `complement.arc_order` is that sequence. Anything it does not mention (a
/// complement recorded before it existed) falls back to the previous
/// arrangement, surviving children in ascending id order and then dropped
/// arcs, which is at least a function of the input rather than of hash
/// seeding.
///
/// A child reachable by both paths is emitted once. When a combinator both
/// drops a sort and keeps that sort's child in the view (`hoist_field`
/// drops the intermediate sort while hoisting its child), the same
/// `(parent, child)` arc comes from each; emitting it twice gives the
/// parent two identical outgoing arcs, which the JSON encoder reads as a
/// repeated-edge list signal and serializes as a duplicated `Value::List`
/// instead of the original record. A W-type instance has at most one arc
/// per parent/child pair, and a genuine list node is distinguished by
/// arcs to *distinct* children, which deduping on that key preserves.
fn rebuild_source_arcs(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    nodes: &HashMap<u32, Node>,
) -> Vec<(u32, u32, Edge)> {
    let mut arcs = Vec::new();
    let mut seen_pairs: HashSet<(u32, u32)> = HashSet::new();

    let restore_surviving =
        |child_id: u32, arcs: &mut Vec<(u32, u32, Edge)>, seen: &mut HashSet<(u32, u32)>| {
            let Some(&original_parent) = complement.original_parent.get(&child_id) else {
                return;
            };
            if !nodes.contains_key(&child_id) || child_id == view.root {
                return;
            }
            if let Some(arc) = find_original_arc(
                lens,
                view,
                nodes,
                original_parent,
                child_id,
                &complement.contraction_choices,
                &complement.arc_edges,
            ) && seen.insert((arc.0, arc.1))
            {
                arcs.push(arc);
            }
        };
    let restore_dropped =
        |pair: (u32, u32), arcs: &mut Vec<(u32, u32, Edge)>, seen: &mut HashSet<(u32, u32)>| {
            if let Some(arc) = complement
                .dropped_arcs
                .iter()
                .find(|arc| (arc.0, arc.1) == pair)
                && nodes.contains_key(&arc.0)
                && nodes.contains_key(&arc.1)
                && seen.insert(pair)
            {
                arcs.push(arc.clone());
            }
        };

    for &(parent, child) in &complement.arc_order {
        restore_surviving(child, &mut arcs, &mut seen_pairs);
        restore_dropped((parent, child), &mut arcs, &mut seen_pairs);
    }

    // Whatever the recorded sequence did not cover.
    let mut remaining: Vec<u32> = complement.original_parent.keys().copied().collect();
    remaining.sort_unstable();
    for child_id in remaining {
        restore_surviving(child_id, &mut arcs, &mut seen_pairs);
    }
    for arc in &complement.dropped_arcs {
        if nodes.contains_key(&arc.0)
            && nodes.contains_key(&arc.1)
            && seen_pairs.insert((arc.0, arc.1))
        {
            arcs.push(arc.clone());
        }
    }

    arcs
}

/// Undo an invertible computation whose result sits on a child node
/// rather than in the parent's `extra_fields`.
///
/// The forward pass writes a computed value into `extra_fields`, where
/// serialization lets it shadow the child that supplied it. That is fine
/// while the view stays in memory, but crossing the JSON boundary
/// (serialize the view, parse it back, then `put`, which is exactly what
/// the WASM `put_json` does) moves the value onto the child node: the
/// parent's `extra_fields` comes back empty and the child holds the
/// *computed* number. `propagate_view_edits_through_inverse` looks only at
/// `extra_fields`, so it finds nothing to invert and the computed value
/// survives into the reconstruction.
///
/// This pass covers that shape. For each `ComputeField` carrying an
/// inverse whose target names a child scalar, it evaluates the inverse
/// against the view's own child values and writes the result to the child
/// the forward expression read.
fn invert_computations_on_child_scalars(
    lens: &Lens,
    view: &WInstance,
    nodes: &mut HashMap<u32, Node>,
) {
    use panproto_inst::FieldTransform;
    use panproto_inst::value::{FieldPresence, Value};

    for (&parent_id, parent) in &view.nodes {
        // The parent's anchor as already restored to the source.
        let Some(src_anchor) = nodes.get(&parent_id).map(|n| n.anchor.clone()) else {
            continue;
        };
        let Some(transforms) = lens.compiled.field_transforms.get(&src_anchor) else {
            continue;
        };

        // The view's child scalars for this parent, keyed by edge name.
        let mut child_of: HashMap<String, (u32, Value)> = HashMap::new();
        for (p, child, edge) in &view.arcs {
            if *p != parent_id {
                continue;
            }
            if let Some(child_node) = view.nodes.get(child)
                && let Some(FieldPresence::Present(value)) = &child_node.value
            {
                let field = edge.name.as_deref().unwrap_or(&edge.tgt).to_string();
                child_of.insert(field, (*child, value.clone()));
            }
        }
        if child_of.is_empty() {
            continue;
        }

        let env_fields: HashMap<String, Value> = child_of
            .iter()
            .map(|(k, (_, v))| (k.clone(), v.clone()))
            .collect();

        for transform in transforms {
            let FieldTransform::ComputeField {
                target_key,
                expr,
                inverse: Some(inv_expr),
                ..
            } = transform
            else {
                continue;
            };
            // Only when the computed value actually landed on the child.
            //
            // While the view is in memory the computed value sits in the
            // parent's `extra_fields`, shadowing a child that still holds
            // the *source* value; the snapshot path restores that correctly
            // and inverting here as well would apply the inverse twice.
            // Its absence from `extra_fields` is what says the value moved
            // onto the child, which is the case this pass exists for.
            if parent.extra_fields.contains_key(target_key) || !child_of.contains_key(target_key) {
                continue;
            }
            let Some(source_key) = crate::derived::sole_source_var(expr) else {
                continue;
            };
            let Some(&(target_child, _)) = child_of.get(&source_key) else {
                continue;
            };

            let env = panproto_inst::build_env_from_extra_fields(&env_fields);
            let config = panproto_expr::EvalConfig::default();
            if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config)
                && let Some(node) = nodes.get_mut(&target_child)
            {
                node.value = Some(FieldPresence::Present(
                    panproto_inst::expr_literal_to_value(&result),
                ));
            }
        }
    }
}

/// Propagate user edits from a view's `extra_fields` back into a node
/// pre-populated from the complement snapshot.
///
/// The snapshot already contains the pre-transform values, so we only
/// need to overwrite entries for which the view's (possibly edited)
/// value differs. For each forward transform whose output key is still
/// meaningful (`RenameField`, `ApplyExpr`, `ComputeField`), we read the
/// view's value at the forward-target key and, when the transform has a
/// usable inverse, write the inverted value to the corresponding
/// source-side key. For `AddField` we simply drop the added key because
/// the source never had it.
///
/// Transforms are processed in reverse of their forward order so that
/// when multiple transforms chain (e.g. rename-then-compute) the later
/// forward transform is undone first.
fn propagate_view_edits_through_inverse(
    node: &mut Node,
    view_fields: &std::collections::HashMap<String, panproto_inst::value::Value>,
    transforms: &[panproto_inst::FieldTransform],
) {
    use panproto_inst::FieldTransform;

    // Mutable working copy so ApplyExpr lookups see prior rewrites.
    let mut working: std::collections::HashMap<String, panproto_inst::value::Value> =
        view_fields.clone();

    for transform in transforms.iter().rev() {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => {
                if let Some(val) = working.remove(new_key) {
                    node.extra_fields.insert(old_key.clone(), val.clone());
                    working.insert(old_key.clone(), val);
                }
            }
            FieldTransform::ApplyExpr {
                key,
                inverse: Some(inv_expr),
                ..
            } => {
                if key == "__value__" {
                    // Handled via complement.original_values elsewhere.
                    continue;
                }
                // The forward transform writes its result to `extra_fields`
                // whether it read the value from there or from a child
                // scalar. Only the first case has a source-side entry to
                // restore: when the snapshot has no such key the value
                // belongs to a child node, which the snapshot restores on
                // its own, and writing it here would invent a field the
                // source never carried.
                if working.contains_key(key) && node.extra_fields.contains_key(key) {
                    let env = panproto_inst::build_env_from_extra_fields(&working);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                        let inverted = panproto_inst::expr_literal_to_value(&result);
                        node.extra_fields.insert(key.clone(), inverted.clone());
                        working.insert(key.clone(), inverted);
                    }
                }
            }
            FieldTransform::ComputeField {
                target_key,
                expr,
                inverse,
                ..
            } => {
                // The source never carried the computed key, so it must not
                // appear in the restored node whatever else happens here.
                node.extra_fields.remove(target_key);

                // The inverse recovers the *source* coordinate the forward
                // expression read, not the computed key it wrote; writing it
                // back to `target_key` would both reinstate a field the
                // source never had and drop the edit on the floor.
                if let Some(inv_expr) = inverse
                    && working.contains_key(target_key)
                    && let Some(source_key) = crate::derived::sole_source_var(expr)
                    // Only an `extra_fields` source can be written here. A
                    // child scalar lives on another node, and inserting it
                    // on this one would invent a field the source lacked;
                    // `collect_derived_fields` classifies that case derived
                    // so the snapshot stays authoritative for it.
                    && node.extra_fields.contains_key(&source_key)
                {
                    let env = panproto_inst::build_env_from_extra_fields(&working);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                        let inverted = panproto_inst::expr_literal_to_value(&result);
                        node.extra_fields
                            .insert(source_key.clone(), inverted.clone());
                        working.insert(source_key, inverted);
                    }
                }
            }
            FieldTransform::AddField { key, .. } => {
                // Snapshot has no entry for the added key; make sure
                // nothing else slipped it in.
                node.extra_fields.remove(key);
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                propagate_view_edits_through_inverse(
                    node,
                    view_fields,
                    std::slice::from_ref(inner),
                );
            }
            // DropField / KeepFields / Case / MapReferences / ComputeField
            // without inverse / ApplyExpr without inverse: the snapshot
            // is authoritative, nothing to propagate.
            _ => {}
        }
    }
}

/// Apply inverse field transforms to a node, undoing the forward coercion.
///
/// For each `ApplyExpr` or `ComputeField` with an inverse expression, evaluate
/// the inverse on the node's current value to recover the pre-coercion value.
/// Transforms without inverses are skipped (the view's value is kept as-is).
///
/// This is a fallback path used when the complement does not contain a
/// snapshot of the original `extra_fields`. The primary `put` path restores
/// from `complement.original_extra_fields`, which is always captured by
/// `get`. This function is only reached for externally constructed
/// complements or composed lenses where snapshots were not propagated.
fn apply_inverse_transforms(node: &mut Node, transforms: &[panproto_inst::FieldTransform]) {
    use panproto_inst::FieldTransform;

    // Apply in reverse order (undo last transform first).
    for transform in transforms.iter().rev() {
        match transform {
            FieldTransform::ApplyExpr {
                key,
                inverse: Some(inv_expr),
                ..
            } => {
                // Handle "__value__" specially: target node.value, not extra_fields.
                if key == "__value__" {
                    if let Some(panproto_inst::value::FieldPresence::Present(val)) = &node.value {
                        let input = panproto_inst::value_to_expr_literal(val);
                        let env = panproto_expr::Env::new()
                            .extend(std::sync::Arc::from("v"), input.clone())
                            .extend(std::sync::Arc::from("__value__"), input);
                        let config = panproto_expr::EvalConfig::default();
                        if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                            node.value = Some(panproto_inst::value::FieldPresence::Present(
                                panproto_inst::expr_literal_to_value(&result),
                            ));
                        }
                    }
                } else if node.extra_fields.contains_key(key) {
                    let env = panproto_inst::build_env_from_extra_fields(&node.extra_fields);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                        node.extra_fields
                            .insert(key.clone(), panproto_inst::expr_literal_to_value(&result));
                    }
                }
            }
            FieldTransform::ComputeField {
                target_key,
                expr,
                inverse,
                ..
            } if node.extra_fields.contains_key(target_key) => {
                // As in `propagate_view_edits_through_inverse`: the inverse
                // restores the source coordinate the forward expression
                // read, and the computed key itself is not part of the
                // source. Unlike that path there is no snapshot to fall
                // back on here, so the source key is written whether or not
                // it is already present.
                if let Some(inv_expr) = inverse
                    && let Some(source_key) = crate::derived::sole_source_var(expr)
                {
                    let env = panproto_inst::build_env_from_extra_fields(&node.extra_fields);
                    let config = panproto_expr::EvalConfig::default();
                    if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
                        node.extra_fields
                            .insert(source_key, panproto_inst::expr_literal_to_value(&result));
                    }
                }
                node.extra_fields.remove(target_key);
            }
            FieldTransform::RenameField { old_key, new_key } => {
                // Reverse: rename new_key back to old_key.
                if let Some(val) = node.extra_fields.remove(new_key) {
                    node.extra_fields.insert(old_key.clone(), val);
                }
            }
            FieldTransform::AddField { key, .. } => {
                // Reverse of adding: remove the field.
                node.extra_fields.remove(key);
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                // Recurse into nested structure. Non-empty paths into
                // nested Value structures are not inverted (would
                // require traversing Value::Unknown maps).
                apply_inverse_transforms(node, std::slice::from_ref(inner));
            }
            // DropField, KeepFields, Case, MapReferences: data is lost, cannot invert.
            _ => {}
        }
    }
}

/// Reconstruct a source from a view alone, with no complement.
///
/// A lens with complement decomposes its source as `S ≅ V × C`. When the
/// complement is terminal the decomposition collapses to `S ≅ V × 1 ≅ V`,
/// the projection `get` becomes an isomorphism, and its inverse is a
/// function of the view alone. That is exactly the condition
/// [`Lens::is_isomorphism`] tests, so this succeeds precisely for lenses
/// satisfying it and refuses everything else.
///
/// The refusal is not a limitation of this implementation. For a lens with
/// non-trivial complement, `get` is not injective: distinct sources share a
/// view, the fibre over that view has more than one point, and no section
/// of `get` exists to be written. Anything returned in that case would be a
/// guess.
///
/// What remains once the residue is empty is the reassembly bookkeeping,
/// which [`Complement::residue_is_trivial`] explains is a function of the
/// view's own structure rather than of the record. This derives it from the
/// view and hands the result to [`put`].
///
/// # Errors
///
/// Returns [`LensError::NotAnIsomorphism`] naming the obstruction when the
/// lens is not invertible, or whatever [`put`] reports.
pub fn put_without_complement(lens: &Lens, view: &WInstance) -> Result<WInstance, LensError> {
    if let Some(obstruction) = lens.obstruction_to_isomorphism() {
        return Err(LensError::NotAnIsomorphism {
            detail: obstruction,
        });
    }

    // The bookkeeping half of the complement, read off the view. Sound
    // only because the residue is known empty: every node and arc of the
    // source is present in the view, so the view's own shape is the
    // source's shape.
    let mut original_parent = HashMap::new();
    let mut arc_edges = HashMap::new();
    let mut arc_order = Vec::with_capacity(view.arcs.len());
    let reverse_remap = build_reverse_remap(&lens.compiled.vertex_remap);
    let reverse_edges: HashMap<&Edge, &Edge> = lens
        .compiled
        .edge_remap
        .iter()
        .map(|(src, tgt)| (tgt, src))
        .collect();

    for (parent, child, edge) in &view.arcs {
        original_parent.insert(*child, *parent);
        arc_order.push((*parent, *child));
        // Un-rename the edge: the view carries the target spelling, and the
        // source arc has to be rebuilt under the source one.
        let source_edge = reverse_edges
            .get(edge)
            .copied()
            .cloned()
            .unwrap_or_else(|| {
                let mut e = edge.clone();
                if let Some(src) = reverse_remap.get(&e.src) {
                    e.src = src.clone();
                }
                if let Some(tgt) = reverse_remap.get(&e.tgt) {
                    e.tgt = tgt.clone();
                }
                e
            });
        arc_edges.insert((*parent, *child), source_edge);
    }

    // The source's `extra_fields` at each node, likewise derived. The
    // forward pass writes each transform's target there; when that target
    // also names a child edge the write was a *shadow* over a child the
    // source carried, and the source itself had no such entry. Dropping
    // those keys is what distinguishes the reconstruction from the
    // no-snapshot fallback, which leaves the inverted value sitting in
    // `extra_fields` and hands back a node the source never had. Every
    // other key is one the source did carry, and `put` applies the
    // inverse to it.
    let mut original_extra_fields = HashMap::new();
    let reverse_remap_for_fields = build_reverse_remap(&lens.compiled.vertex_remap);
    for (&id, node) in &view.nodes {
        let src_anchor = reverse_remap_for_fields
            .get(&node.anchor)
            .unwrap_or(&node.anchor);
        let Some(transforms) = lens.compiled.field_transforms.get(src_anchor) else {
            original_extra_fields.insert(id, node.extra_fields.clone());
            continue;
        };
        let child_names: HashSet<String> = view
            .arcs
            .iter()
            .filter(|(parent, _, _)| *parent == id)
            .map(|(_, _, edge)| edge.name.as_deref().unwrap_or(&edge.tgt).to_string())
            .collect();
        let shadowed: HashSet<&str> = transforms
            .iter()
            .filter_map(transform_target_key)
            .filter(|key| child_names.contains(*key))
            .collect();
        let mut fields = node.extra_fields.clone();
        fields.retain(|key, _| !shadowed.contains(key.as_str()));
        original_extra_fields.insert(id, fields);
    }

    let mut complement = Complement {
        original_parent,
        arc_edges,
        arc_order,
        original_extra_fields,
        ..Complement::empty()
    };
    let envelope = lens.identity(ComplementShape::WType);
    complement.stamp(envelope.clone(), &lens.src_schema);

    put_with_identity(
        lens,
        view,
        &complement,
        &envelope,
        IdentityPolicy::AllowLegacy,
    )
    .map(|(source, _)| source)
}

/// The `extra_fields` key a transform writes, when it writes one.
fn transform_target_key(transform: &panproto_inst::FieldTransform) -> Option<&str> {
    use panproto_inst::FieldTransform;
    match transform {
        FieldTransform::ApplyExpr { key, .. } | FieldTransform::AddField { key, .. } => {
            Some(key.as_str())
        }
        FieldTransform::ComputeField { target_key, .. } => Some(target_key.as_str()),
        FieldTransform::RenameField { new_key, .. } => Some(new_key.as_str()),
        _ => None,
    }
}

/// Build a reverse mapping from target vertex IDs back to source vertex IDs.
fn build_reverse_remap(forward: &HashMap<Name, Name>) -> HashMap<Name, Name> {
    forward
        .iter()
        .map(|(k, v)| (v.clone(), k.clone()))
        .collect()
}

/// Find the original arc between a parent and child in the source schema.
///
/// Consults `arc_edges` (exact edge recorded during `get`) first, then
/// `contraction_choices`, then falls back to the source schema. When the
/// complement was produced by this module's `get`, `arc_edges` contains the
/// exact edge, which keeps this function deterministic even when the source
/// schema has parallel edges.
///
/// The recorded edge applies only while it still joins the restored
/// endpoints and its image is still the edge the view arc carries. A view
/// edit that moves the child (or the parent) to another anchor restores
/// that node at a different source anchor, and the recorded edge would
/// then point into the anchor the node left; a view edit that relabels the
/// arc onto another target edge leaves the anchors alone, and the recorded
/// edge would then undo the relabel. Either way the arc is instead read off
/// the source schema between the restored anchors, preferring the source
/// edge whose image is the edge the view arc now carries, so the view's
/// edge edit survives the round trip. When the source schema has no edge
/// between the restored anchors the recorded edge is kept, as nothing
/// better is known.
fn find_original_arc(
    lens: &Lens,
    view: &WInstance,
    nodes: &HashMap<u32, Node>,
    parent_id: u32,
    child_id: u32,
    contraction_choices: &HashMap<(u32, u32), Edge>,
    arc_edges: &HashMap<(u32, u32), Edge>,
) -> Option<(u32, u32, Edge)> {
    let endpoints = nodes
        .get(&parent_id)
        .zip(nodes.get(&child_id))
        .map(|(parent, child)| (&parent.anchor, &child.anchor));
    let view_edge = view
        .arcs
        .iter()
        .find(|(p, c, _)| (*p, *c) == (parent_id, child_id))
        .map(|(_, _, edge)| edge);

    // Exact edge recorded during get, while it still fits the endpoints
    // and the view has not relabelled the arc.
    let recorded = arc_edges.get(&(parent_id, child_id));
    if let Some(edge) = recorded
        && endpoints.is_none_or(|(src, tgt)| edge.src == *src && edge.tgt == *tgt)
        && view_edge.is_none_or(|view_edge| edge_image(lens, edge) == *view_edge)
    {
        return Some((parent_id, child_id, edge.clone()));
    }

    // Contraction choice: edges created by ancestor contraction.
    if let Some(edge) = contraction_choices.get(&(parent_id, child_id)) {
        return Some((parent_id, child_id, edge.clone()));
    }

    // The source schema between the restored anchors: a complement that
    // predates `arc_edges`, or a recorded edge a view edit made stale.
    let (src_anchor, tgt_anchor) = endpoints?;
    let candidates = lens.src_schema.edges_between(src_anchor, tgt_anchor);
    let chosen = view_edge
        .and_then(|view_edge| {
            candidates
                .iter()
                .find(|candidate| edge_image(lens, candidate) == *view_edge)
        })
        .or_else(|| candidates.first())
        .or(recorded);
    chosen.map(|edge| (parent_id, child_id, edge.clone()))
}

/// The view edge `get` carries source edge `source` onto: its entry in the
/// explicit edge remap, or, for an edge the remap does not name, the same
/// kind and label across remapped endpoints.
fn edge_image(lens: &Lens, source: &Edge) -> Edge {
    if let Some(mapped) = lens.compiled.edge_remap.get(source) {
        return mapped.clone();
    }
    let remap = |vertex: &Name| {
        lens.compiled
            .vertex_remap
            .get(vertex)
            .cloned()
            .unwrap_or_else(|| vertex.clone())
    };
    Edge {
        src: remap(&source.src),
        tgt: remap(&source.tgt),
        kind: source.kind.clone(),
        name: source.name.clone(),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};

    #[test]
    fn get_with_identity_lens_produces_empty_complement() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        assert_eq!(view.node_count(), instance.node_count());
        assert!(
            complement.dropped_nodes.is_empty(),
            "no nodes should be dropped by identity lens"
        );
    }

    #[test]
    fn get_then_put_round_trips_identity() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let (view, complement) =
            get(&lens, &instance).unwrap_or_else(|e| panic!("get failed: {e}"));
        let restored = put(&lens, &view, &complement).unwrap_or_else(|e| panic!("put failed: {e}"));

        assert_eq!(
            restored.node_count(),
            instance.node_count(),
            "restored should have same node count"
        );
        assert_eq!(restored.root, instance.root, "restored root should match");
    }

    #[test]
    fn complement_is_empty_for_identity() {
        let complement = Complement::empty();
        assert!(complement.is_empty());
    }

    /// A view edit that moves a node to another anchor survives `put`,
    /// even for a node whose source anchor the complement recorded.
    mod reanchoring {
        use super::*;
        use panproto_inst::value::{FieldPresence, Value};
        use panproto_mig::Migration;
        use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

        fn prop(src: &str, tgt: &str, name: &str) -> Edge {
            Edge {
                src: src.into(),
                tgt: tgt.into(),
                kind: "prop".into(),
                name: Some(name.into()),
            }
        }

        fn schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
            let protocol = Protocol::default();
            let mut builder = SchemaBuilder::new(&protocol);
            for (id, kind) in vertices {
                builder = builder.vertex(id, kind, None).unwrap();
            }
            for e in edges {
                builder = builder
                    .edge(&e.src, &e.tgt, &e.kind, e.name.as_deref())
                    .unwrap();
            }
            builder.build().unwrap()
        }

        fn leaf(id: u32, anchor: &str, s: &str) -> Node {
            Node::new(id, anchor).with_value(FieldPresence::Present(Value::Str(s.into())))
        }

        /// `r -> r`, `a -> t`, `b -> t`, `c -> u`, and `d -> t2`, `e -> t2`:
        /// `t` and `t2` each have two preimages, `u` has one.
        fn merging_lens() -> Lens {
            let src_edges: Vec<Edge> = ["a", "b", "c", "d", "e"]
                .iter()
                .map(|v| prop("r", v, v))
                .collect();
            let tgt_edges = vec![
                prop("r", "t", "a"),
                prop("r", "t", "b"),
                prop("r", "u", "c"),
                prop("r", "t2", "d"),
                prop("r", "t2", "e"),
            ];
            let src = schema(
                &[
                    ("r", "object"),
                    ("a", "string"),
                    ("b", "string"),
                    ("c", "string"),
                    ("d", "string"),
                    ("e", "string"),
                ],
                &src_edges,
            );
            let tgt = schema(
                &[
                    ("r", "object"),
                    ("t", "string"),
                    ("u", "string"),
                    ("t2", "string"),
                ],
                &tgt_edges,
            );
            let migration = Migration {
                vertex_map: [
                    ("r", "r"),
                    ("a", "t"),
                    ("b", "t"),
                    ("c", "u"),
                    ("d", "t2"),
                    ("e", "t2"),
                ]
                .iter()
                .map(|(s, t)| (Name::from(*s), Name::from(*t)))
                .collect(),
                edge_map: src_edges.into_iter().zip(tgt_edges).collect(),
                hyper_edge_map: HashMap::new(),
                label_map: HashMap::new(),
                resolver: HashMap::new(),
                hyper_resolver: HashMap::new(),
                expr_resolvers: HashMap::new(),
                coercions: HashMap::new(),
                domain: None,
                codomain: None,
            };
            let compiled = panproto_mig::compile(&src, &tgt, &migration).unwrap();
            Lens {
                compiled,
                src_schema: src,
                tgt_schema: tgt,
            }
        }

        fn source() -> WInstance {
            let nodes = vec![
                Node::new(0, "r"),
                leaf(1, "a", "one"),
                leaf(2, "b", "two"),
                leaf(3, "c", "three"),
            ];
            let arcs = vec![
                (0, 1, prop("r", "a", "a")),
                (0, 2, prop("r", "b", "b")),
                (0, 3, prop("r", "c", "c")),
            ];
            WInstance::new(
                nodes.into_iter().map(|n| (n.id, n)).collect(),
                arcs,
                Vec::new(),
                0,
                "r".into(),
            )
        }

        /// Move view node `id` to `anchor`, carrying its arc onto the
        /// target edge `r -> anchor` named `edge_name`, as a real view edit
        /// must to stay well typed.
        fn reanchor(view: &mut WInstance, id: u32, anchor: &str, edge_name: &str) {
            view.nodes.get_mut(&id).unwrap().anchor = anchor.into();
            for (_, child, edge) in &mut view.arcs {
                if *child == id {
                    *edge = prop("r", anchor, edge_name);
                }
            }
        }

        fn arc_edge(instance: &WInstance, parent: u32, child: u32) -> &Edge {
            &instance
                .arcs
                .iter()
                .find(|(p, c, _)| (*p, *c) == (parent, child))
                .unwrap_or_else(|| panic!("no arc {parent} -> {child}"))
                .2
        }

        #[test]
        fn a_recorded_anchor_is_restored_while_the_view_keeps_it() {
            let lens = merging_lens();
            let (view, complement) = get(&lens, &source()).unwrap();
            assert_eq!(complement.original_anchors.get(&1), Some(&Name::from("a")));
            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(restored.nodes[&1].anchor, "a");
            assert_eq!(restored.nodes[&2].anchor, "b");
            assert_eq!(arc_edge(&restored, 0, 1), &prop("r", "a", "a"));
            assert_eq!(arc_edge(&restored, 0, 2), &prop("r", "b", "b"));
        }

        #[test]
        fn a_view_edit_that_moves_a_recorded_node_is_kept() {
            let lens = merging_lens();
            let (mut view, complement) = get(&lens, &source()).unwrap();
            assert_eq!(view.nodes[&1].anchor, "t");
            assert_eq!(arc_edge(&view, 0, 1), &prop("r", "t", "a"));
            reanchor(&mut view, 1, "u", "c");

            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(restored.nodes[&1].anchor, "c");
            assert_eq!(
                arc_edge(&restored, 0, 1),
                &prop("r", "c", "c"),
                "the restored arc must be the source edge into the node's restored anchor"
            );
            let (again, _) = get(&lens, &restored).unwrap();
            assert_eq!(again.nodes[&1].anchor, "u", "PutGet lost the edit");
            assert_eq!(
                arc_edge(&again, 0, 1),
                &prop("r", "u", "c"),
                "PutGet lost the arc's edit"
            );
        }

        #[test]
        fn a_node_moved_to_a_merged_anchor_takes_the_anchor_its_edge_names() {
            let lens = merging_lens();
            let (mut view, complement) = get(&lens, &source()).unwrap();
            reanchor(&mut view, 1, "t2", "d");

            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(restored.nodes[&1].anchor, "d");
            assert_eq!(arc_edge(&restored, 0, 1), &prop("r", "d", "d"));
            let (again, _) = get(&lens, &restored).unwrap();
            assert_eq!(again.nodes[&1].anchor, "t2", "PutGet lost the move");
            assert_eq!(arc_edge(&again, 0, 1), &prop("r", "t2", "d"));
        }

        #[test]
        fn a_node_moved_to_a_merged_anchor_under_an_unknown_edge_is_ambiguous() {
            let lens = merging_lens();
            let (mut view, complement) = get(&lens, &source()).unwrap();
            reanchor(&mut view, 1, "t2", "unmapped");

            match put(&lens, &view, &complement) {
                Err(LensError::AmbiguousReverseAnchor {
                    node: 1,
                    candidates,
                }) => assert_eq!(candidates, vec![Name::from("d"), Name::from("e")]),
                other => panic!("expected an ambiguous reverse anchor for node 1, got {other:?}"),
            }
        }

        /// Relabel the view arc into `child` to the target edge `r -> t`
        /// named `edge_name`, leaving the child's anchor where it is.
        fn relabel(view: &mut WInstance, child: u32, edge_name: &str) {
            for (_, c, edge) in &mut view.arcs {
                if *c == child {
                    edge.name = Some(edge_name.into());
                }
            }
        }

        #[test]
        fn a_relabelled_arc_on_a_merged_anchor_restores_the_other_preimage() {
            let lens = merging_lens();
            let (mut view, complement) = get(&lens, &source()).unwrap();
            assert_eq!(arc_edge(&view, 0, 1), &prop("r", "t", "a"));
            relabel(&mut view, 1, "b");

            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(
                restored.nodes[&1].anchor, "b",
                "the relabelled arc names the source edge into b"
            );
            assert_eq!(arc_edge(&restored, 0, 1), &prop("r", "b", "b"));
            let (again, _) = get(&lens, &restored).unwrap();
            assert_eq!(
                arc_edge(&again, 0, 1),
                &prop("r", "t", "b"),
                "PutGet lost a relabel"
            );
        }

        /// `r -> a` carries two parallel edges `x` and `y`; the vertex map
        /// is injective (`a -> t`).
        fn parallel_lens() -> Lens {
            let src_edges = vec![prop("r", "a", "x"), prop("r", "a", "y")];
            let tgt_edges = vec![prop("r", "t", "x"), prop("r", "t", "y")];
            let src = schema(&[("r", "object"), ("a", "string")], &src_edges);
            let tgt = schema(&[("r", "object"), ("t", "string")], &tgt_edges);
            let migration = Migration {
                vertex_map: [("r", "r"), ("a", "t")]
                    .iter()
                    .map(|(s, t)| (Name::from(*s), Name::from(*t)))
                    .collect(),
                edge_map: src_edges.into_iter().zip(tgt_edges).collect(),
                hyper_edge_map: HashMap::new(),
                label_map: HashMap::new(),
                resolver: HashMap::new(),
                hyper_resolver: HashMap::new(),
                expr_resolvers: HashMap::new(),
                coercions: HashMap::new(),
                domain: None,
                codomain: None,
            };
            let compiled = panproto_mig::compile(&src, &tgt, &migration).unwrap();
            Lens {
                compiled,
                src_schema: src,
                tgt_schema: tgt,
            }
        }

        #[test]
        fn a_relabelled_parallel_arc_is_restored_on_the_edge_it_names() {
            let lens = parallel_lens();
            let nodes = vec![Node::new(0, "r"), leaf(1, "a", "one")];
            let source = WInstance::new(
                nodes.into_iter().map(|n| (n.id, n)).collect(),
                vec![(0, 1, prop("r", "a", "x"))],
                Vec::new(),
                0,
                "r".into(),
            );
            let (mut view, complement) = get(&lens, &source).unwrap();
            assert_eq!(arc_edge(&view, 0, 1), &prop("r", "t", "x"));
            relabel(&mut view, 1, "y");

            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(restored.nodes[&1].anchor, "a");
            assert_eq!(arc_edge(&restored, 0, 1), &prop("r", "a", "y"));
            let (again, _) = get(&lens, &restored).unwrap();
            assert_eq!(
                arc_edge(&again, 0, 1),
                &prop("r", "t", "y"),
                "PutGet lost a relabel"
            );
        }

        /// `r -> d` and `r -> e` both carry an edge named `m`, and both map
        /// onto the one target edge `r -> t2` named `m`.
        fn merged_edge_lens() -> Lens {
            let src_edges = vec![
                prop("r", "a", "a"),
                prop("r", "d", "m"),
                prop("r", "e", "m"),
            ];
            let tgt_edges = vec![
                prop("r", "t", "a"),
                prop("r", "t2", "m"),
                prop("r", "t2", "m"),
            ];
            let src = schema(
                &[
                    ("r", "object"),
                    ("a", "string"),
                    ("d", "string"),
                    ("e", "string"),
                ],
                &src_edges,
            );
            let tgt = schema(
                &[("r", "object"), ("t", "string"), ("t2", "string")],
                &tgt_edges[..2],
            );
            let migration = Migration {
                vertex_map: [("r", "r"), ("a", "t"), ("d", "t2"), ("e", "t2")]
                    .iter()
                    .map(|(s, t)| (Name::from(*s), Name::from(*t)))
                    .collect(),
                edge_map: src_edges.into_iter().zip(tgt_edges).collect(),
                hyper_edge_map: HashMap::new(),
                label_map: HashMap::new(),
                resolver: HashMap::new(),
                hyper_resolver: HashMap::new(),
                expr_resolvers: HashMap::new(),
                coercions: HashMap::new(),
                domain: None,
                codomain: None,
            };
            let compiled = panproto_mig::compile(&src, &tgt, &migration).unwrap();
            Lens {
                compiled,
                src_schema: src,
                tgt_schema: tgt,
            }
        }

        #[test]
        fn a_node_moved_onto_an_edge_with_several_preimages_is_ambiguous() {
            let lens = merged_edge_lens();
            let nodes = vec![Node::new(0, "r"), leaf(1, "a", "one")];
            let source = WInstance::new(
                nodes.into_iter().map(|n| (n.id, n)).collect(),
                vec![(0, 1, prop("r", "a", "a"))],
                Vec::new(),
                0,
                "r".into(),
            );
            let (mut view, complement) = get(&lens, &source).unwrap();
            reanchor(&mut view, 1, "t2", "m");

            match put(&lens, &view, &complement) {
                Err(LensError::AmbiguousReverseAnchor {
                    node: 1,
                    candidates,
                }) => assert_eq!(candidates, vec![Name::from("d"), Name::from("e")]),
                other => panic!("expected an ambiguous reverse anchor for node 1, got {other:?}"),
            }
        }
    }

    /// Partial-monoid laws: identity, idempotence, associativity, and
    /// commutativity on the domain of definition.
    mod partial_monoid {
        use super::*;
        use crate::asymmetric::ComplementCompose;
        use panproto_inst::Node as InstNode;
        use panproto_schema::Edge;

        fn mk_node(id: u32, anchor: &str) -> InstNode {
            InstNode::new(id, anchor)
        }

        fn mk_edge(src: &str, tgt: &str, kind: &str) -> Edge {
            Edge {
                src: src.into(),
                tgt: tgt.into(),
                kind: kind.into(),
                name: None,
            }
        }

        /// The legacy fingerprint the hand-built complements below are
        /// bound by. A complement that carries data must be bound to some
        /// lens to take part in composition at all.
        const BOUND: u64 = 0x00C0_FFEE;

        fn singleton_dropped_node(id: u32, anchor: &str) -> Complement {
            let mut c = Complement::empty();
            c.dropped_nodes.insert(id, mk_node(id, anchor));
            c.source_fingerprint = BOUND;
            c
        }

        fn singleton_arc_edge(parent: u32, child: u32, kind: &str) -> Complement {
            let mut c = Complement::empty();
            c.arc_edges.insert((parent, child), mk_edge("a", "b", kind));
            c.source_fingerprint = BOUND;
            c
        }

        #[test]
        fn empty_is_left_identity() {
            let c = singleton_dropped_node(1, "x");
            let composed = Complement::empty().compose(&c).expect("compatible");
            assert_eq!(composed.dropped_nodes.len(), 1);
            assert!(composed.dropped_nodes.contains_key(&1));
        }

        #[test]
        fn empty_is_right_identity() {
            let c = singleton_dropped_node(1, "x");
            let composed = c.compose(&Complement::empty()).expect("compatible");
            assert_eq!(composed.dropped_nodes.len(), 1);
        }

        #[test]
        fn idempotent_on_equal_entries() {
            let c = singleton_dropped_node(7, "v7");
            let composed = c.compose(&c).expect("equal entries idempotent");
            assert_eq!(composed.dropped_nodes.len(), 1);
        }

        #[test]
        fn rejects_distinct_entries_on_same_key() {
            let a = singleton_dropped_node(3, "left");
            let b = singleton_dropped_node(3, "right");
            assert!(matches!(
                a.compose(&b),
                Err(LensError::ComplementConflict {
                    kind: "dropped_nodes",
                    ..
                })
            ));
            assert!(!a.is_compatible(&b));
        }

        #[test]
        fn rejects_cross_fingerprint_when_both_set() {
            let mut a = Complement::empty();
            a.source_fingerprint = 0xAAAA;
            let mut b = Complement::empty();
            b.source_fingerprint = 0xBBBB;
            assert!(matches!(
                a.compose(&b),
                Err(LensError::Identity(
                    panproto_inst::IdentityError::LegacyComplementRejected {
                        reason: panproto_inst::identity::LegacyRejectReason::FingerprintMismatch,
                    }
                ))
            ));
        }

        #[test]
        fn rejects_data_bound_to_no_lens() {
            let mut unbound = Complement::empty();
            unbound.dropped_nodes.insert(1, mk_node(1, "x"));
            for (a, b) in [
                (&unbound, &Complement::empty()),
                (&Complement::empty(), &unbound),
            ] {
                assert!(matches!(
                    a.compose(b),
                    Err(LensError::Identity(
                        panproto_inst::IdentityError::UnboundComplement
                    ))
                ));
            }
        }

        #[test]
        fn propagates_fingerprint_when_only_one_set() {
            let mut a = Complement::empty();
            a.source_fingerprint = 0xC0DE;
            let b = Complement::empty();
            let r = a.compose(&b).expect("compatible");
            assert_eq!(r.source_fingerprint, 0xC0DE);
            let r2 = b.compose(&a).expect("compatible");
            assert_eq!(r2.source_fingerprint, 0xC0DE);
        }

        #[test]
        fn associative_on_disjoint_keys() {
            let a = singleton_dropped_node(1, "v1");
            let b = singleton_dropped_node(2, "v2");
            let c = singleton_arc_edge(3, 4, "prop");

            let left = a
                .compose(&b)
                .expect("ab compatible")
                .compose(&c)
                .expect("(ab)c compatible");
            let right = a
                .compose(&b.compose(&c).expect("bc compatible"))
                .expect("a(bc) compatible");
            assert_eq!(left.dropped_nodes.len(), right.dropped_nodes.len());
            assert_eq!(left.arc_edges.len(), right.arc_edges.len());
            for (k, v) in &left.dropped_nodes {
                let other = right.dropped_nodes.get(k).expect("present on right");
                assert!(node_equiv(v, other));
            }
            for (k, v) in &left.arc_edges {
                assert_eq!(right.arc_edges.get(k), Some(v));
            }
        }

        #[test]
        fn commutative_on_disjoint_keys() {
            let a = singleton_dropped_node(10, "x");
            let b = singleton_dropped_node(20, "y");
            let ab = a.compose(&b).expect("compatible");
            let ba = b.compose(&a).expect("compatible");
            assert_eq!(ab.dropped_nodes.len(), ba.dropped_nodes.len());
            for (k, v) in &ab.dropped_nodes {
                let other = ba.dropped_nodes.get(k).expect("present");
                assert!(node_equiv(v, other));
            }
        }

        /// Property-based partial-monoid laws for [`Complement::compose`].
        ///
        /// These are the `complement_partial_monoid_*` proptests that the
        /// `Complement::compose` doc comment cites. Each draws complements
        /// with every field independently populated from a shared key pool,
        /// so distinct draws mix disjoint keys with agreeing shared keys. On
        /// the domain of definition the merge is an idempotent partial
        /// commutative monoid with `empty()` as identity; off it, `compose`
        /// reports a precise error.
        mod props {
            use super::*;
            use crate::asymmetric::ComplementCompose;
            use panproto_inst::value::{FieldPresence, Value};
            use proptest::prelude::*;
            use std::collections::{HashMap, HashSet};

            /// Size of the shared key pool. Small enough that independently
            /// drawn subsets overlap often (so shared keys are genuinely
            /// exercised) yet large enough to also yield disjoint keys.
            const KEY_POOL: u32 = 6;

            /// A non-zero fingerprint used alongside `0`. Any two complements
            /// drawn from `{0, FP}` are fingerprint-compatible, keeping the
            /// algebraic-law generators inside the domain of definition.
            const FP: u64 = 0xC0FF_EE00;

            // Canonical value for each key. Because every field's value is a
            // pure function of its key, two independently generated
            // complements agree on every shared key by construction. Hence
            // `compose` is always defined between them and the partial monoid
            // restricted to these inputs is total, which is what lets the
            // commutativity and associativity properties assert an
            // unconditional equality.
            fn canon_node(k: u32) -> InstNode {
                mk_node(k, &format!("anchor-{}", k % 3))
            }
            fn canon_edge(p: u32, c: u32) -> Edge {
                mk_edge(&format!("s{p}"), &format!("t{c}"), "prop")
            }
            fn canon_arc(p: u32, c: u32) -> (u32, u32, Edge) {
                (p, c, canon_edge(p, c))
            }
            fn canon_fan(k: u32) -> Fan {
                Fan::new(format!("he-{k}"), k).with_child("child", k + 100)
            }
            fn canon_extra_fields(k: u32) -> HashMap<String, Value> {
                let mut m = HashMap::new();
                m.insert("payload".to_owned(), Value::Int(i64::from(k)));
                m
            }
            fn canon_presence(k: u32) -> Option<FieldPresence> {
                // Vary across the `Option<FieldPresence>` variants so the
                // merge's `presence_equiv` comparison exercises absent, null,
                // and present-with-value keys.
                match k % 3 {
                    0 => None,
                    1 => Some(FieldPresence::Null),
                    _ => Some(FieldPresence::Present(Value::Str(format!("v{k}")))),
                }
            }
            fn canon_parent(k: u32) -> u32 {
                k + 100
            }

            type IdSet = HashSet<u32>;
            type PairSet = HashSet<(u32, u32)>;

            fn arb_id_set() -> impl Strategy<Value = IdSet> {
                prop::collection::hash_set(0u32..KEY_POOL, 0..=4)
            }
            fn arb_pair_set() -> impl Strategy<Value = PairSet> {
                prop::collection::hash_set((0u32..KEY_POOL, 0u32..KEY_POOL), 0..=4)
            }
            /// How every complement of one generated case is bound to its
            /// lens: all by one legacy fingerprint, or all by one envelope.
            /// Operands bound alike are always composable, which keeps the
            /// algebraic laws inside the domain of definition.
            #[derive(Clone, Copy, Debug)]
            enum Binding {
                Legacy,
                Stamped,
            }

            fn arb_binding() -> impl Strategy<Value = Binding> {
                prop_oneof![Just(Binding::Legacy), Just(Binding::Stamped)]
            }

            fn bind(c: &mut Complement, binding: Binding) {
                match binding {
                    Binding::Legacy => c.source_fingerprint = FP,
                    Binding::Stamped => {
                        let schema = crate::tests::three_node_schema();
                        let lens = crate::tests::identity_lens(&schema);
                        c.stamp(lens.identity(ComplementShape::WType), &schema);
                    }
                }
            }

            /// Strategy generating a [`Complement`] with `dropped_nodes`,
            /// `dropped_arcs`, `dropped_fans`, `contraction_choices`,
            /// `original_parent`, `original_extra_fields`, `arc_edges`,
            /// `original_values`, `synthesized_nodes`, and a fingerprint all
            /// independently populated from the shared key pool.
            fn arb_complement(binding: Binding) -> impl Strategy<Value = Complement> {
                (
                    arb_id_set(),
                    arb_pair_set(),
                    arb_id_set(),
                    arb_pair_set(),
                    arb_id_set(),
                    arb_id_set(),
                    arb_pair_set(),
                    arb_id_set(),
                    arb_id_set(),
                )
                    .prop_map(
                        move |(
                            nodes,
                            arcs,
                            fans,
                            contraction,
                            parents,
                            extra,
                            arc_edges,
                            values,
                            synth,
                        )| {
                            let mut c = Complement::empty();
                            for k in nodes {
                                c.dropped_nodes.insert(k, canon_node(k));
                            }
                            for (p, ch) in arcs {
                                c.dropped_arcs.push(canon_arc(p, ch));
                            }
                            for k in fans {
                                c.dropped_fans.push(canon_fan(k));
                            }
                            for (p, ch) in contraction {
                                c.contraction_choices.insert((p, ch), canon_edge(p, ch));
                            }
                            for k in parents {
                                c.original_parent.insert(k, canon_parent(k));
                            }
                            for k in extra {
                                c.original_extra_fields.insert(k, canon_extra_fields(k));
                            }
                            for (p, ch) in arc_edges {
                                c.arc_edges.insert((p, ch), canon_edge(p, ch));
                            }
                            for k in values {
                                c.original_values.insert(k, canon_presence(k));
                            }
                            c.synthesized_nodes = synth;
                            bind(&mut c, binding);
                            c
                        },
                    )
            }

            // --- order-independent structural equality on Complement ---

            fn map_equiv<K, V, F>(a: &HashMap<K, V>, b: &HashMap<K, V>, eq: F) -> bool
            where
                K: Eq + std::hash::Hash,
                F: Fn(&V, &V) -> bool,
            {
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| eq(v, w)))
            }

            // Composition dedups these vectors, so equal length plus one-way
            // containment is full set equality.
            fn vec_set_equiv<T: PartialEq>(a: &[T], b: &[T]) -> bool {
                a.len() == b.len() && a.iter().all(|x| b.contains(x))
            }

            fn complement_equiv(a: &Complement, b: &Complement) -> bool {
                a.source_fingerprint == b.source_fingerprint
                    && a.identity == b.identity
                    && map_equiv(&a.dropped_nodes, &b.dropped_nodes, node_equiv)
                    && vec_set_equiv(&a.dropped_arcs, &b.dropped_arcs)
                    && vec_set_equiv(&a.dropped_fans, &b.dropped_fans)
                    && map_equiv(
                        &a.contraction_choices,
                        &b.contraction_choices,
                        PartialEq::eq,
                    )
                    && a.original_parent == b.original_parent
                    && map_equiv(
                        &a.original_extra_fields,
                        &b.original_extra_fields,
                        extra_fields_equiv,
                    )
                    && map_equiv(&a.arc_edges, &b.arc_edges, PartialEq::eq)
                    && map_equiv(&a.original_values, &b.original_values, |x, y| {
                        presence_equiv(x.as_ref(), y.as_ref())
                    })
                    && a.synthesized_nodes == b.synthesized_nodes
            }

            fn arb_one() -> impl Strategy<Value = Complement> {
                arb_binding().prop_flat_map(arb_complement)
            }

            fn arb_two() -> impl Strategy<Value = (Complement, Complement)> {
                arb_binding().prop_flat_map(|b| (arb_complement(b), arb_complement(b)))
            }

            fn arb_three() -> impl Strategy<Value = (Complement, Complement, Complement)> {
                arb_binding()
                    .prop_flat_map(|b| (arb_complement(b), arb_complement(b), arb_complement(b)))
            }

            proptest! {
                #![proptest_config(ProptestConfig::with_cases(128))]

                /// `empty()` is a two-sided identity for `compose` on every
                /// generated complement. The composite is always defined
                /// because `empty()` carries no data and no identity, which
                /// makes it the unit whatever the other operand is bound by.
                #[test]
                fn complement_partial_monoid_identity(c in arb_one()) {
                    let e = Complement::empty();
                    let left = e.compose(&c).expect("empty is left-compatible");
                    let right = c.compose(&e).expect("empty is right-compatible");
                    prop_assert!(complement_equiv(&left, &c), "left identity");
                    prop_assert!(complement_equiv(&right, &c), "right identity");
                }

                /// On the domain of definition `compose` is commutative:
                /// `a.compose(b)` and `b.compose(a)` agree. The generator makes
                /// every pair compatible by construction (agreeing values,
                /// one binding), so both sides are defined and equality is
                /// asserted unconditionally.
                #[test]
                fn complement_partial_monoid_commutative((a, b) in arb_two()) {
                    prop_assert!(a.is_compatible(&b), "compatible by construction");
                    let ab = a.compose(&b).expect("ab defined");
                    let ba = b.compose(&a).expect("ba defined");
                    prop_assert!(complement_equiv(&ab, &ba));
                }

                /// On the domain of definition `compose` is associative:
                /// `(a.b).c == a.(b.c)`. Every triple drawn from the shared
                /// key pool under one binding is pairwise compatible, so both
                /// bracketings are defined.
                #[test]
                fn complement_partial_monoid_associative((a, b, c) in arb_three()) {
                    let left = a
                        .compose(&b)
                        .and_then(|ab| ab.compose(&c))
                        .expect("(ab)c defined");
                    let right = b
                        .compose(&c)
                        .and_then(|bc| a.compose(&bc))
                        .expect("a(bc) defined");
                    prop_assert!(complement_equiv(&left, &right));
                }

                /// Off the domain of definition `compose` reports a precise
                /// error: a shared key with distinct values yields
                /// [`LensError::ComplementConflict`], and two distinct
                /// legacy fingerprints yield [`LensError::Identity`].
                #[test]
                fn complement_partial_monoid_conflict(
                    key in 0u32..KEY_POOL,
                    left_tag in 0u32..3,
                    right_tag in 0u32..3,
                    fp_a in 1u64..0x1_0000,
                    fp_b in 1u64..0x1_0000,
                ) {
                    // Distinct dropped-node values on a shared key -> conflict.
                    if left_tag != right_tag {
                        let mut a = Complement::empty();
                        a.dropped_nodes.insert(key, mk_node(key, &format!("L{left_tag}")));
                        a.source_fingerprint = FP;
                        let mut b = Complement::empty();
                        b.dropped_nodes.insert(key, mk_node(key, &format!("R{right_tag}")));
                        b.source_fingerprint = FP;
                        let node_conflict = matches!(
                            a.compose(&b),
                            Err(LensError::ComplementConflict {
                                kind: "dropped_nodes",
                                ..
                            })
                        );
                        prop_assert!(node_conflict, "dropped_nodes conflict expected");
                        prop_assert!(!a.is_compatible(&b));

                        // The same clash surfaces through an edge-keyed field.
                        let mut a2 = Complement::empty();
                        a2.arc_edges
                            .insert((0, 1), mk_edge("s", "t", &format!("L{left_tag}")));
                        a2.source_fingerprint = FP;
                        let mut b2 = Complement::empty();
                        b2.arc_edges
                            .insert((0, 1), mk_edge("s", "t", &format!("R{right_tag}")));
                        b2.source_fingerprint = FP;
                        let edge_conflict = matches!(
                            a2.compose(&b2),
                            Err(LensError::ComplementConflict {
                                kind: "arc_edges",
                                ..
                            })
                        );
                        prop_assert!(edge_conflict, "arc_edges conflict expected");
                    }

                    // Two distinct legacy fingerprints -> refused.
                    if fp_a != fp_b {
                        let mut a = Complement::empty();
                        a.source_fingerprint = fp_a;
                        let mut b = Complement::empty();
                        b.source_fingerprint = fp_b;
                        let fp_mismatch = matches!(
                            a.compose(&b),
                            Err(LensError::Identity(
                                panproto_inst::IdentityError::LegacyComplementRejected { .. }
                            ))
                        );
                        prop_assert!(fp_mismatch, "fingerprint mismatch expected");
                    }
                }
            }
        }
    }
}
