//! Asymmetric lens operations: `get` and `put` with complement tracking.
//!
//! The `get` direction runs the restrict pipeline (forward migration) while
//! capturing everything that was discarded into a [`Complement`]. The `put`
//! direction restores the original source instance from a (possibly modified)
//! view plus the complement.

use std::collections::{BTreeSet, HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::identity::{
    ComplementShape, IdentityCarrier, IdentityEnvelope, IdentityPolicy, IdentityStatus,
    compose_identities,
};
use panproto_inst::{
    Fan, FanResolver, Node, PutDomainError, RestrictError, RestrictPlan, ViewRef, WInstance,
    canonical_label_shape, wtype_restrict_with_plan,
};
use panproto_schema::Edge;

use crate::Lens;
use crate::error::LensError;

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

/// A node id for a node added to a tree view, drawn at or above the
/// complement's floor so that it never reuses the id of a source node. See
/// [`panproto_inst::complement::fresh_view_node_id`].
pub use panproto_inst::complement::fresh_view_node_id;

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

        let synthesized_keys = merge_synthesized_keys(self, other)?;

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
            id_floor: self.id_floor.max(other.id_floor),
            synthesized_keys,
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
pub(crate) fn node_equiv(a: &Node, b: &Node) -> bool {
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
/// endpoints; this indexes the source schema's edges by that image. Only
/// edges `get` carries into the view are indexed: those the edge remap
/// names, and those whose endpoints both survive. An edge into a dropped
/// vertex has no image, even when the dropped vertex shares its name with
/// a view vertex.
struct EdgePreimages {
    preimages: HashMap<Edge, Vec<Edge>>,
}

impl EdgePreimages {
    fn new(lens: &Lens) -> Self {
        let remap = &lens.compiled;
        let mut preimages: HashMap<Edge, Vec<Edge>> = HashMap::new();
        for source in lens.src_schema.edges.keys().filter(|source| {
            remap.edge_remap.contains_key(*source)
                || (remap.vertex_remap.contains_key(&source.src)
                    && remap.vertex_remap.contains_key(&source.tgt))
        }) {
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
/// This runs the tree restrict on the lens's plan
/// ([`wtype_restrict_with_plan`]) and then computes the set difference
/// between the source and result to populate the complement, recording the
/// key of every node the restrict synthesized
/// ([`Complement::synthesized_keys`](panproto_inst::Complement::synthesized_keys)).
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

/// The union of two complements' synthesized keys, which must agree by id
/// and by key: one id names one key, and one key one id.
fn merge_synthesized_keys(
    a: &Complement,
    b: &Complement,
) -> Result<std::collections::BTreeMap<u32, panproto_inst::SynthKey>, LensError> {
    let mut synthesized_keys = a.synthesized_keys.clone();
    let mut by_key: HashMap<&panproto_inst::SynthKey, u32> = a
        .synthesized_keys
        .iter()
        .map(|(&id, key)| (key, id))
        .collect();
    for (&id, key) in &b.synthesized_keys {
        let id_conflict = synthesized_keys.get(&id).is_some_and(|mine| mine != key);
        let key_conflict = by_key.get(key).is_some_and(|&mine| mine != id);
        if id_conflict || key_conflict {
            return Err(LensError::ComplementConflict {
                kind: "synthesized_keys",
                key: format!("{id:?} {key:?}"),
            });
        }
        synthesized_keys.insert(id, key.clone());
        by_key.insert(key, id);
    }
    Ok(synthesized_keys)
}

/// The source edge of every surviving arc whose view label does not
/// determine it: the source edges between its endpoints' anchors whose
/// image is the label the view gives the arc are not exactly this one.
/// Every other arc's edge is read back off the source schema by `put`.
fn undetermined_arc_edges(
    lens: &Lens,
    instance: &WInstance,
    view: &WInstance,
) -> HashMap<(u32, u32), Edge> {
    let view_edge_into: HashMap<(u32, u32), &Edge> = view
        .arcs
        .iter()
        .map(|(parent, child, edge)| ((*parent, *child), edge))
        .collect();
    let mut arc_edges = HashMap::new();
    for (parent, child, edge) in &instance.arcs {
        if !(view.nodes.contains_key(parent) && view.nodes.contains_key(child)) {
            continue;
        }
        let determined = view_edge_into
            .get(&(*parent, *child))
            .zip(instance.nodes.get(parent).zip(instance.nodes.get(child)))
            .is_some_and(|(view_edge, (p, c))| {
                let mut preimages = lens
                    .src_schema
                    .edges_between(&p.anchor, &c.anchor)
                    .iter()
                    .filter(|candidate| edge_image(lens, candidate) == **view_edge);
                preimages.next() == Some(edge) && preimages.next().is_none()
            });
        if !determined {
            arc_edges.insert((*parent, *child), edge.clone());
        }
    }
    arc_edges
}

/// Project one source fan by the same survivor and [`FanResolver`] rule as
/// the tree restrict engine.
///
/// Keeping this operation explicit lets `get` decide whether the source fan
/// itself is recoverable from its projection, and lets `put` remove exactly
/// that projection before restoring recorded source residue.
fn project_source_fan(
    fan: &Fan,
    surviving: &HashSet<u32>,
    resolver: &FanResolver<'_>,
) -> Result<Option<Fan>, LensError> {
    if !surviving.contains(&fan.parent) {
        return Ok(None);
    }

    let mut surviving_children: Vec<(&String, u32)> = fan
        .children
        .iter()
        .filter(|(_, node)| surviving.contains(node))
        .map(|(label, node)| (label, *node))
        .collect();
    if surviving_children.is_empty() {
        return Ok(None);
    }
    surviving_children.sort_unstable();

    let surviving_shape = canonical_label_shape(surviving_children.iter().map(|(label, _)| label));
    let full_shape = canonical_label_shape(fan.children.keys());
    let entry = resolver.resolve(&fan.hyper_edge_id, &surviving_shape, &full_shape);
    let hyper_edge_id = entry.map_or_else(
        || fan.hyper_edge_id.clone(),
        |(target, _)| target.to_string(),
    );
    let mut children = HashMap::new();
    for (source_label, node) in surviving_children {
        let target_label = entry
            .and_then(|(_, labels)| labels.get(source_label.as_str()))
            .map_or_else(|| source_label.clone(), ToString::to_string);
        if children.insert(target_label.clone(), node).is_some() {
            return Err(RestrictError::FanReconstructionFailed {
                hyper_edge_id,
                detail: format!("two children are relabelled `{target_label}`"),
            }
            .into());
        }
    }
    Ok(Some(Fan {
        hyper_edge_id,
        parent: fan.parent,
        children,
    }))
}

/// [`get`] with the lens's envelope already computed, for callers that run
/// many `get`s through one lens.
pub(crate) fn get_with_identity(
    lens: &Lens,
    instance: &WInstance,
    envelope: &IdentityEnvelope,
) -> Result<(WInstance, Complement), LensError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;
    let (view, synthesis) = wtype_restrict_with_plan(instance, &plan)?;

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

    // A source fan is residue exactly when its forward projection is absent
    // or non-identical. Besides dropped children, this records resolver
    // retargeting and relabelling that the view alone cannot invert.
    let resolver = FanResolver::new(&lens.compiled);
    let surviving: HashSet<u32> = view.nodes.keys().copied().collect();
    let mut dropped_fans = Vec::new();
    for fan in &instance.fans {
        if project_source_fan(fan, &surviving, &resolver)?.as_ref() != Some(fan) {
            dropped_fans.push(fan.clone());
        }
    }

    let (original_extra_fields, original_values) = capture_pre_transform_state(instance, &view);

    let arc_edges = undetermined_arc_edges(lens, instance, &view);

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
    // instance. These were synthesized by the restrict to satisfy a
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
        // A source holding the id `u32::MAX` has a floor above every `u32`;
        // it records `u32::MAX`, against which `fresh_view_node_id` reports
        // exhaustion.
        id_floor: panproto_inst::complement::tree_id_floor(instance).unwrap_or(u32::MAX),
        // The key of every node the restrict synthesized: an expansion node
        // by the source arc it expands (its edge id in the source's graph
        // form) and its hop.
        synthesized_keys: synthesis.synthesized_keys,
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
/// the view, and [`LensError::PutDomain`] with the first violation
/// [`check_put_domain`] reports when the view is outside the domain of
/// `put` (for instance a view node with the id of a node the complement
/// holds, or a node added in the view, or moved away from the anchor `get`
/// gave it, with no single source anchor).
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
    if matches!(status, IdentityStatus::Unbound) {
        unbound_provenance(view, complement)?;
    }
    if let Some(first) = structural_violations(lens, view, complement)
        .into_iter()
        .next()
    {
        return Err(LensError::PutDomain(Box::new(first)));
    }
    let source = reassemble(lens, view, complement)?;
    if let Some(first) = value_violations(lens, view, complement, &source)
        .into_iter()
        .next()
    {
        return Err(LensError::PutDomain(Box::new(first)));
    }
    Ok((source, status))
}

/// The domain predicate of tree [`put`]: every reason `view` cannot be
/// written back through `lens` against `complement`, or `Ok` when `put`
/// accepts it.
///
/// Checked:
///
/// * [`PutDomainError::IdCollision`]: a view node has the id of a node the
///   complement holds as dropped, so restoring that node would overwrite it.
/// * [`PutDomainError::SynthesizedEdited`]: a node or arc introduced by the
///   forward presentation of a contraction or expansion was removed or its
///   label was changed. Such an element has no independent source object to
///   which the edit could be written back.
/// * [`PutDomainError::ContractedEdgeInserted`]: a view arc that `get` did
///   not produce joins two nodes whose source anchors the lens contracts (a
///   path through at least one dropped vertex) to exactly the arc's label,
///   and no source edge carries that label between those anchors. No
///   source path can be written for it: `get` of any source puts a
///   contracted arc there only for a dropped interior node, which the view
///   does not name.
/// * [`PutDomainError::UnrepresentableViewEdge`]: any other view arc that
///   `get` did not produce whose label is the image of no source edge, or
///   of more than one, between its endpoints' source anchors. `put` would
///   have no source edge to write it back under.
/// * [`PutDomainError::AmbiguousReverseAnchor`]: a view node added or moved
///   in the view is anchored at a vertex with several source preimages.
/// * [`PutDomainError::SurvivalPredicateFails`]: a restored node's
///   survival predicate is false on the restored source, so the next `get`
///   would hide it again.
/// * [`PutDomainError::SurvivalPredicateRefused`]: a restored node's
///   survival predicate gives neither `true` nor `false` on the restored
///   source (it reads a repeated or non-tree child, a value with no
///   expression literal, fails to evaluate or yields no boolean), so the
///   next `get` would refuse the source. Predicates are evaluated as `get`
///   evaluates them, over the restored node's fields and its restored
///   children bound by column name.
/// * [`PutDomainError::DerivedCellEdited`] and
///   [`PutDomainError::TransformNotInvertible`]: recomputing the forward
///   transforms of a restored node does not give the value the view holds
///   on a field the lens computes: a field the migration adds (a default
///   or a computed field), or one written by a transform with no inverse.
///
/// All violations are reported, ordered by variant (the declaration order
/// of [`PutDomainError`]) and then by the element's [`ViewRef`]. The value
/// checks run on every view node whose restoration a structural violation
/// leaves determined, so a structural violation elsewhere in the view hides
/// none of them: a view node colliding with a dropped node is checked as
/// the view holds it, and only a node with no single source anchor, or
/// under an arc with no single source edge, is left out together with the
/// view subtree below it, since `put` has nothing to restore it as. Put is
/// total on views for which this returns `Ok`.
///
/// # Errors
///
/// Returns every violation found, in that order.
///
/// [`ViewRef`]: panproto_inst::ViewRef
pub fn check_put_domain(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<(), Vec<PutDomainError>> {
    let mut found = structural_violations(lens, view, complement);
    let restorable = if found.is_empty() {
        None
    } else {
        Some(restorable_part(view, complement, &found))
    };
    let (view, complement) = restorable
        .as_ref()
        .map_or((view, complement), |(view, complement)| (view, complement));
    if let Ok(source) = reassemble(lens, view, complement) {
        found.extend(value_violations(lens, view, complement, &source));
    }
    let found = in_report_order(found);
    if found.is_empty() { Ok(()) } else { Err(found) }
}

/// The part of `view` and `complement` that `put` can still restore given
/// the structural violations `found`: the view without every node that
/// has no single source anchor or sits under an arc with no single source
/// edge (and without the view subtree below each), and the complement
/// without the dropped nodes a view node collides with, so that the view
/// node is restored in their place.
fn restorable_part(
    view: &WInstance,
    complement: &Complement,
    found: &[PutDomainError],
) -> (WInstance, Complement) {
    let mut complement = complement.clone();
    let mut unrestorable: Vec<u32> = Vec::new();
    for violation in found {
        match violation {
            PutDomainError::IdCollision {
                elem: ViewRef::Node(id),
            } => {
                complement.dropped_nodes.remove(id);
            }
            PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Node(id),
                ..
            } => unrestorable.push(*id),
            PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Arc { child, .. },
            }
            | PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Arc { child, .. },
            } => unrestorable.push(*child),
            _ => {}
        }
    }
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (parent, child, _) in &view.arcs {
        children.entry(*parent).or_default().push(*child);
    }
    let mut removed: HashSet<u32> = HashSet::new();
    while let Some(id) = unrestorable.pop() {
        if id != view.root && removed.insert(id) {
            unrestorable.extend(children.get(&id).into_iter().flatten().copied());
        }
    }
    let nodes = view
        .nodes
        .iter()
        .filter(|(id, _)| !removed.contains(*id))
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    let arcs = view
        .arcs
        .iter()
        .filter(|(parent, child, _)| !removed.contains(parent) && !removed.contains(child))
        .cloned()
        .collect();
    let fans = view
        .fans
        .iter()
        .filter(|fan| {
            !removed.contains(&fan.parent) && fan.children.values().all(|c| !removed.contains(c))
        })
        .cloned()
        .collect();
    (
        WInstance::new(nodes, arcs, fans, view.root, view.schema_root.clone()),
        complement,
    )
}

/// Sort violations into report order and drop repeats.
fn in_report_order(mut found: Vec<PutDomainError>) -> Vec<PutDomainError> {
    found.sort_by(|a, b| {
        a.variant_index()
            .cmp(&b.variant_index())
            .then_with(|| violation_ref(a).cmp(&violation_ref(b)))
    });
    found.dedup();
    found
}

/// The element a violation names, as a [`ViewRef`].
fn violation_ref(violation: &PutDomainError) -> Option<ViewRef> {
    match violation {
        PutDomainError::IdCollision { elem }
        | PutDomainError::SynthesizedEdited { elem }
        | PutDomainError::ContractedEdgeInserted { elem }
        | PutDomainError::UnrepresentableViewEdge { elem }
        | PutDomainError::AmbiguousReverseAnchor { elem, .. }
        | PutDomainError::SurvivalPredicateFails { elem }
        | PutDomainError::SurvivalPredicateRefused { elem, .. }
        | PutDomainError::DerivedCellEdited { elem, .. }
        | PutDomainError::TransformNotInvertible { elem, .. }
        | PutDomainError::MissingRequiredCell { elem, .. } => Some(elem.clone()),
        PutDomainError::MalformedFan { node, .. } => Some(ViewRef::Node(*node)),
        PutDomainError::DuplicateKey { elems, .. } => elems.first().cloned(),
    }
}

/// The violations read off the view and complement alone: id collisions,
/// arcs inserted across a contracted pair, arcs with no single source edge,
/// and ambiguous reverse anchors.
fn structural_violations(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Vec<PutDomainError> {
    let mut found = synthesized_edits(view, complement);
    let reverse_anchors = ReverseAnchors::new(&lens.compiled.vertex_remap);
    let edge_preimages = EdgePreimages::new(lens);
    let mut view_arc_into: HashMap<u32, (u32, &Edge)> = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        view_arc_into.entry(*child).or_insert((*parent, edge));
    }

    let mut anchors: HashMap<u32, Name> = HashMap::new();
    for (&id, node) in &view.nodes {
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        if complement.dropped_nodes.contains_key(&id) {
            found.push(PutDomainError::IdCollision {
                elem: ViewRef::Node(id),
            });
        }
        match restored_anchor(
            lens,
            &reverse_anchors,
            &edge_preimages,
            complement,
            id,
            &node.anchor,
            view_arc_into.get(&id).copied(),
        ) {
            Ok(anchor) => {
                anchors.insert(id, anchor.clone());
            }
            Err(violation) => found.push(*violation),
        }
    }

    for (parent, child, edge) in &view.arcs {
        if complement.synthesized_nodes.contains(parent)
            || complement.synthesized_nodes.contains(child)
            || produced_by_get(lens, complement, *parent, *child, edge)
        {
            continue;
        }
        let (Some(src_anchor), Some(tgt_anchor)) = (anchors.get(parent), anchors.get(child)) else {
            continue;
        };
        let preimages = lens
            .src_schema
            .edges_between(src_anchor, tgt_anchor)
            .iter()
            .filter(|candidate| edge_image(lens, candidate) == *edge)
            .count();
        let elem = ViewRef::Arc {
            parent: *parent,
            child: *child,
        };
        match preimages {
            1 => {}
            0 if contracted_pair_label(lens, src_anchor, tgt_anchor).as_ref() == Some(edge) => {
                found.push(PutDomainError::ContractedEdgeInserted { elem });
            }
            _ => found.push(PutDomainError::UnrepresentableViewEdge { elem }),
        }
    }

    in_report_order(found)
}

/// Edits to the presentation-only part of a tree view.
///
/// Every arc recorded in `contraction_choices` was created by `get`: either
/// it bypasses a dropped source path or it is one hop of an expanded source
/// edge. Likewise, every id in `synthesized_nodes` names an expansion node
/// with no source counterpart. Removing one of these elements, relabelling
/// its arc, or inserting another arc through a synthesized node cannot be
/// translated to the source. Detecting the edit before reassembly is
/// essential: reassembly intentionally discards the presentation layer and
/// would otherwise accept the edit while the next `get` silently restores
/// the old presentation.
fn synthesized_edits(view: &WInstance, complement: &Complement) -> Vec<PutDomainError> {
    let mut found = Vec::new();

    for &id in &complement.synthesized_nodes {
        let expected_anchors: BTreeSet<_> = complement
            .contraction_choices
            .iter()
            .filter_map(|(&(parent, child), edge)| {
                if child == id {
                    Some(edge.tgt.clone())
                } else if parent == id {
                    Some(edge.src.clone())
                } else {
                    None
                }
            })
            .collect();
        let unchanged = view.nodes.get(&id).is_some_and(|node| {
            node.id == id
                && expected_anchors.len() == 1
                && expected_anchors.contains(&node.anchor)
                && node.value.is_none()
                && node.discriminator.is_none()
                && node.extra_fields.is_empty()
                && node.position.is_none()
                && matches!(node.shape, panproto_inst::NodeShape::Plain)
                && node.annotations.is_empty()
        });
        if !unchanged {
            found.push(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(id),
            });
        }
    }

    for (&(parent, child), expected) in &complement.contraction_choices {
        let matches = view.arcs.iter().any(|(p, c, edge)| {
            let same_endpoints = (*p, *c) == (parent, child);
            same_endpoints && edge == expected
        });
        if !matches {
            found.push(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc { parent, child },
            });
        }
    }

    for (parent, child, edge) in &view.arcs {
        let touches_synthesized = complement.synthesized_nodes.contains(parent)
            || complement.synthesized_nodes.contains(child);
        let is_recorded = complement.contraction_choices.get(&(*parent, *child)) == Some(edge);
        if touches_synthesized && !is_recorded {
            found.push(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                },
            });
        }
    }

    found.extend(synthesized_sibling_order(view, complement));
    found
}

/// Synthesized arcs moved before a kept sibling.
///
/// Tree get emits each parent's children as its kept arcs followed by its
/// synthesized first hops, so in a view `get` produced no kept arc follows a
/// synthesized arc of the same parent. A view where one does has moved the
/// synthesized arc: the next `get` of the restored source would order the
/// siblings back. Each synthesized arc with a later kept sibling is
/// reported.
fn synthesized_sibling_order(view: &WInstance, complement: &Complement) -> Vec<PutDomainError> {
    let synthesized = |parent: u32, child: u32, edge: &Edge| {
        complement.contraction_choices.get(&(parent, child)) == Some(edge)
            || complement.synthesized_nodes.contains(&child)
    };
    let mut found = Vec::new();
    for (index, (parent, child, edge)) in view.arcs.iter().enumerate() {
        if !synthesized(*parent, *child, edge) {
            continue;
        }
        let kept_after = view.arcs[index + 1..]
            .iter()
            .any(|(p, c, e)| p == parent && !synthesized(*p, *c, e));
        if kept_after {
            found.push(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                },
            });
        }
    }
    found
}

/// Whether the view arc `(parent, child, edge)` is one `get` put there: an
/// arc into a surviving source node from the node `get` connected it to,
/// carrying the edge `get` gave it (a contraction choice, or the image of
/// the source arc it recorded).
fn produced_by_get(
    lens: &Lens,
    complement: &Complement,
    parent: u32,
    child: u32,
    edge: &Edge,
) -> bool {
    complement.contraction_choices.get(&(parent, child)) == Some(edge)
        || complement
            .arc_edges
            .get(&(parent, child))
            .is_some_and(|recorded| edge_image(lens, recorded) == *edge)
}

/// The violations found by writing the view back and running the forward
/// direction again on the result: every restored node whose survival
/// predicate is not true, and every computed field whose recomputed value is
/// not the value the view holds.
///
/// The forward transforms are recomputed with the survival predicates
/// lifted, so a node whose predicate fails is still checked on its fields
/// and its failure hides no derived-cell edit, on it or on any other node.
/// A lens with no field transforms and no survival predicates has none, and
/// is not run again.
fn value_violations(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    source: &WInstance,
) -> Vec<PutDomainError> {
    use panproto_inst::value::FieldPresence;

    let compiled = &lens.compiled;
    let rewrites_fields =
        !compiled.field_transforms.is_empty() || !compiled.op_term_assignments.is_empty();
    if !rewrites_fields && compiled.conditional_survival.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    if !compiled.conditional_survival.is_empty()
        && let Ok(plan) = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, compiled)
    {
        // Every predicate is evaluated on the restored source exactly as the
        // next `get` evaluates it: over the node's fields and its children
        // bound by column name.
        let verdicts = panproto_inst::tree_survival_verdicts(source, &plan);
        for &id in view.nodes.keys() {
            if complement.synthesized_nodes.contains(&id) {
                continue;
            }
            let elem = ViewRef::Node(id);
            match verdicts.get(&id) {
                None | Some(Ok(true)) => {}
                Some(Ok(false)) => found.push(PutDomainError::SurvivalPredicateFails { elem }),
                Some(Err(refusal)) => found.push(PutDomainError::SurvivalPredicateRefused {
                    elem,
                    refusal: refusal.clone(),
                }),
            }
        }
    }
    if !rewrites_fields {
        return in_report_order(found);
    }

    let unconditional;
    let forward = if compiled.conditional_survival.is_empty() {
        compiled
    } else {
        unconditional = panproto_inst::CompiledMigration {
            conditional_survival: HashMap::new(),
            ..compiled.clone()
        };
        &unconditional
    };
    let recomputed = match RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, forward)
        .and_then(|plan| wtype_restrict_with_plan(source, &plan))
    {
        Ok((recomputed, _)) => recomputed,
        Err(error) => {
            found.extend(put_refusal_from_restrict(lens, source, error));
            return in_report_order(found);
        }
    };
    for (&id, view_node) in &view.nodes {
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let (Some(restored), Some(again)) = (source.nodes.get(&id), recomputed.nodes.get(&id))
        else {
            continue;
        };
        for field in computed_term_fields(compiled, &restored.anchor) {
            let differs = match (
                view_node.extra_fields.get(field),
                again.extra_fields.get(field),
            ) {
                (Some(shown), Some(recomputed)) => !value_equiv(shown, recomputed),
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            };
            if differs {
                found.push(PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Node(id),
                    field: field.to_owned(),
                });
            }
        }
        let Some(transforms) = compiled.field_transforms.get(&restored.anchor) else {
            continue;
        };
        let mut written: Vec<(&str, &panproto_inst::FieldTransform)> = transforms
            .iter()
            .filter_map(|t| written_key(t).map(|key| (key, t)))
            .collect();
        // The last transform writing a key decides what the key is.
        written.reverse();
        let mut seen: HashSet<&str> = HashSet::new();
        for (key, transform) in written {
            if !seen.insert(key) {
                continue;
            }
            // A leaf-value coercion is read off the leaf value, which a
            // view may leave absent; only a present value is an edit.
            let differs = if key == "__value__" {
                matches!(view_node.value, Some(FieldPresence::Present(_)))
                    && !presence_equiv(view_node.value.as_ref(), again.value.as_ref())
            } else {
                match (view_node.extra_fields.get(key), again.extra_fields.get(key)) {
                    (Some(shown), Some(recomputed)) => !value_equiv(shown, recomputed),
                    (Some(_), None) => true,
                    (None, _) => false,
                }
            };
            if !differs {
                continue;
            }
            let field = key.to_owned();
            let elem = ViewRef::Node(id);
            found.push(match transform {
                panproto_inst::FieldTransform::AddField { .. }
                | panproto_inst::FieldTransform::ComputeField { .. } => {
                    PutDomainError::DerivedCellEdited { elem, field }
                }
                _ => PutDomainError::TransformNotInvertible { elem, field },
            });
        }
    }
    in_report_order(found)
}

/// The fields the migration computes on nodes over `anchor` through term
/// assignments: the target of each `Compute` and the key of each `Default`.
/// Put recomputes them, so a view that shows another value has edited a
/// derived field.
pub(crate) fn computed_term_fields<'a>(
    compiled: &'a panproto_inst::CompiledMigration,
    anchor: &Name,
) -> Vec<&'a str> {
    let mut fields: Vec<&str> = compiled
        .op_term_assignments
        .get(anchor)
        .into_iter()
        .flatten()
        .filter_map(|assignment| match assignment {
            panproto_inst::TermAssignment::Compute { target, .. } => Some(target.as_str()),
            panproto_inst::TermAssignment::Default { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    fields.sort_unstable();
    fields.dedup();
    fields
}

/// Translate a forward refusal on the reconstructed source into the typed
/// put-domain reason that prevents `PutGet` from attempting that source.
fn put_refusal_from_restrict(
    lens: &Lens,
    source: &WInstance,
    error: panproto_inst::RestrictError,
) -> Vec<PutDomainError> {
    use panproto_inst::{PredicateRefusal, RestrictError};

    let error = match error {
        RestrictError::At { error, .. } => *error,
        other => other,
    };
    match error {
        RestrictError::SurvivalPredicateFailed { node, source } => {
            let refusal = match source {
                panproto_expr::ExprError::TypeError { expected, got } if expected == "bool" => {
                    PredicateRefusal::NotBoolean { got }
                }
                other => PredicateRefusal::EvaluationFailed {
                    message: other.to_string(),
                },
            };
            vec![PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(node),
                refusal,
            }]
        }
        RestrictError::AmbiguousChildField { node, field } => {
            vec![PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(node),
                refusal: PredicateRefusal::AmbiguousChildField { field },
            }]
        }
        RestrictError::StructuralChildNotATree { node, field } => {
            vec![PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(node),
                refusal: PredicateRefusal::StructuralChildNotATree { field },
            }]
        }
        RestrictError::ValueNotExpressible {
            node,
            field,
            value_type,
        } => vec![PutDomainError::SurvivalPredicateRefused {
            elem: ViewRef::Node(node),
            refusal: PredicateRefusal::ValueNotExpressible { field, value_type },
        }],
        RestrictError::FieldTransformFailed {
            key,
            source: failure,
        } => {
            let mut ids: Vec<_> = source.nodes.keys().copied().collect();
            ids.sort_unstable();
            let node = ids
                .iter()
                .copied()
                .find(|id| {
                    source.nodes.get(id).is_some_and(|node| {
                        lens.compiled
                            .field_transforms
                            .get(&node.anchor)
                            .is_some_and(|transforms| {
                                transforms.iter().any(|transform| {
                                    written_key(transform).is_some_and(|written| written == key)
                                })
                            })
                    })
                })
                .unwrap_or(source.root);
            vec![PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(node),
                refusal: PredicateRefusal::EvaluationFailed {
                    message: failure.to_string(),
                },
            }]
        }
        other => vec![PutDomainError::SurvivalPredicateRefused {
            elem: ViewRef::Node(source.root),
            refusal: PredicateRefusal::EvaluationFailed {
                message: other.to_string(),
            },
        }],
    }
}

/// The key a field transform writes, `__value__` for a leaf-value coercion,
/// looking through a top-level path transform.
fn written_key(transform: &panproto_inst::FieldTransform) -> Option<&str> {
    match transform {
        panproto_inst::FieldTransform::PathTransform { path, inner } if path.is_empty() => {
            written_key(inner)
        }
        other => transform_target_key(other),
    }
}

/// Whether a source vertex survives the lens: its image under the vertex
/// remap is a surviving target vertex.
pub(crate) fn vertex_survives(lens: &Lens, vertex: &Name) -> bool {
    let image = lens.compiled.vertex_remap.get(vertex).unwrap_or(vertex);
    lens.compiled.surviving_verts.contains(image)
}

/// The view edge the lens contracts the source anchor pair `(a, b)` to, when
/// it contracts that pair and resolves it to a single edge.
///
/// The pair is contracted when both vertices survive and `b` is reached from
/// `a` in the source schema through at least one interior vertex, one that
/// does not survive or carries a survival predicate (so some of its nodes
/// are dropped). Its label is the resolver's entry for the image pair, or
/// the unique target edge between the images.
fn contracted_pair_label(lens: &Lens, a: &Name, b: &Name) -> Option<Edge> {
    if !vertex_survives(lens, a) || !vertex_survives(lens, b) {
        return None;
    }
    let interior =
        |v: &Name| !vertex_survives(lens, v) || lens.compiled.conditional_survival.contains_key(v);
    let mut visited: HashSet<&Name> = HashSet::new();
    let mut frontier: Vec<&Name> = lens
        .src_schema
        .outgoing
        .get(a)
        .into_iter()
        .flatten()
        .map(|edge| &edge.tgt)
        .filter(|v| interior(v))
        .collect();
    let mut reached = false;
    while let Some(v) = frontier.pop() {
        if !visited.insert(v) {
            continue;
        }
        for edge in lens.src_schema.outgoing.get(v).into_iter().flatten() {
            if &edge.tgt == b {
                reached = true;
            }
            if interior(&edge.tgt) {
                frontier.push(&edge.tgt);
            }
        }
    }
    if !reached {
        return None;
    }
    let image = |v: &Name| lens.compiled.vertex_remap.get(v).unwrap_or(v).clone();
    panproto_inst::resolve_edge(
        &lens.tgt_schema,
        &lens.compiled.resolver,
        &image(a),
        &image(b),
    )
    .ok()
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
/// [`PutDomainError::AmbiguousReverseAnchor`] rather than picked from, unless
/// the recorded anchor is one of them.
fn restored_anchor<'a>(
    lens: &Lens,
    reverse_anchors: &'a ReverseAnchors,
    edge_preimages: &'a EdgePreimages,
    complement: &'a Complement,
    id: u32,
    view_anchor: &'a Name,
    view_arc: Option<(u32, &Edge)>,
) -> Result<&'a Name, Box<PutDomainError>> {
    let recorded = complement.original_anchors.get(&id).filter(|recorded| {
        lens.compiled
            .vertex_remap
            .get(*recorded)
            .unwrap_or(recorded)
            == view_anchor
    });
    let recorded_edge = view_arc.and_then(|(parent, _)| complement.arc_edges.get(&(parent, id)));
    // A node `get` carried over from the source, as opposed to one the view
    // added.
    let from_source = complement.original_parent.contains_key(&id) || recorded_edge.is_some();
    let edge_kept = match (view_arc, recorded_edge) {
        (Some((_, view_edge)), Some(recorded_edge)) => {
            edge_image(lens, recorded_edge) == *view_edge
        }
        // `get` records an arc's edge only when the view label does not
        // determine it; otherwise the label still fits the recorded anchor
        // exactly when some source edge into that anchor has it as image.
        (Some((_, view_edge)), None) if from_source => recorded.is_none_or(|anchor| {
            edge_preimages
                .of(view_edge)
                .iter()
                .any(|source| source.tgt == **anchor)
        }),
        _ => true,
    };
    if edge_kept && let Some(recorded) = recorded {
        return Ok(recorded);
    }

    // Only anchors the node's own view anchor is the image of: an arc whose
    // edge points elsewhere says nothing about where this node belongs.
    // Only for a node `get` carried over from the source: a node the view
    // added has no source to be faithful to, and takes its anchor from the
    // vertex remap alone.
    let mut by_edge = view_arc
        .filter(|_| from_source)
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
            return from_record.ok_or_else(|| {
                Box::new(PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Node(id),
                    candidates: many.iter().map(|anchor| (*anchor).clone()).collect(),
                })
            });
        }
    }

    recorded.map_or_else(
        || {
            reverse_anchors
                .source_anchor(view_anchor)
                .map_err(|candidates| {
                    Box::new(PutDomainError::AmbiguousReverseAnchor {
                        elem: ViewRef::Node(id),
                        candidates: candidates.to_vec(),
                    })
                })
        },
        Ok,
    )
}

fn put_domain(error: PutDomainError) -> LensError {
    LensError::PutDomain(Box::new(error))
}

/// Replace each recorded source fan's surviving projection with that source
/// fan, leaving edited or unrelated view fans in place.
fn restore_source_fans(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    nodes: &HashMap<u32, Node>,
) -> Result<Vec<Fan>, LensError> {
    let resolver = FanResolver::new(&lens.compiled);
    let surviving: HashSet<u32> = view.nodes.keys().copied().collect();
    let mut projected_fans = view.fans.clone();
    let mut restored_source_fans = Vec::new();
    for source_fan in &complement.dropped_fans {
        let parent_present = nodes.contains_key(&source_fan.parent);
        let all_children_present = source_fan
            .children
            .values()
            .all(|node_id| nodes.contains_key(node_id));
        if parent_present && all_children_present {
            if let Some(projected) = project_source_fan(source_fan, &surviving, &resolver)?
                && let Some(index) = projected_fans.iter().position(|fan| fan == &projected)
            {
                projected_fans.remove(index);
            }
            restored_source_fans.push(source_fan.clone());
        }
    }

    let fans = projected_fans
        .into_iter()
        .map(|mut restored_fan| {
            let shape = canonical_label_shape(restored_fan.children.keys());
            if let Some((source_id, (_, label_map))) =
                resolver.resolve_backward(&restored_fan.hyper_edge_id, &shape)
            {
                let mut label_pairs: Vec<_> = label_map.iter().collect();
                label_pairs.sort_unstable_by(|(source_a, target_a), (source_b, target_b)| {
                    target_a.cmp(target_b).then_with(|| source_a.cmp(source_b))
                });
                let mut reverse_labels = HashMap::new();
                for (source, target) in label_pairs {
                    reverse_labels
                        .entry(target.to_string())
                        .or_insert_with(|| source.to_string());
                }
                restored_fan.hyper_edge_id = source_id.to_string();
                restored_fan.children = restored_fan
                    .children
                    .into_iter()
                    .map(|(label, node)| {
                        let source_label = reverse_labels.get(&label).cloned().unwrap_or(label);
                        (source_label, node)
                    })
                    .collect();
            }
            restored_fan
        })
        .chain(restored_source_fans)
        .collect();
    Ok(fans)
}

/// The provenance check of an unbound complement (one carrying neither an
/// envelope nor a fingerprint, which the legacy policy accepts).
///
/// `put` inverts `get` on the image of `get`, and only there. A bound
/// complement was stamped by a `get` of this lens, so its provenance is
/// settled, and a view arc it records no parent for is a view edit. An
/// unbound complement that records no parent for any of the view's arcs
/// has no such evidence: reassembling from it would rebuild the node set
/// with no arcs between the nodes, which serializes to an empty record, and
/// the caller could not tell that from the total loss of a populated one.
/// It is refused.
fn unbound_provenance(view: &WInstance, complement: &Complement) -> Result<(), LensError> {
    let view_has_arcs = view.arcs.iter().any(|(_, child, _)| *child != view.root);
    if view_has_arcs
        && !view
            .arcs
            .iter()
            .any(|(_, child, _)| complement.original_parent.contains_key(child))
    {
        return Err(LensError::ComplementMismatch {
            detail: format!(
                "an unbound complement records no parent for any of the view's {} arcs, so it \
                 did not come from a projection of a source with this shape; reconstructing \
                 from it would return an empty record",
                view.arcs.len()
            ),
        });
    }
    Ok(())
}

/// The backward direction proper, once the complement's identity is
/// accepted.
fn reassemble(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<WInstance, LensError> {
    // Start with all nodes from the view (un-remap anchors back to source).
    // Skip nodes that were synthesized during forward eval by the
    // nest-style `expansion_path` mechanism: they exist only in the view,
    // not in the source, so collapsing them back means dropping them.
    let mut nodes = HashMap::new();
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
        let anchor = restored_anchor(
            lens,
            &reverse_anchors,
            &edge_preimages,
            complement,
            id,
            &node.anchor,
            view_arc_into.get(&id).copied(),
        )
        .map_err(LensError::PutDomain)?;
        let mut restored_node = node.clone();
        restored_node.anchor.clone_from(anchor);
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
            restore_unrecorded_fields(
                &mut restored_node,
                transforms,
                complement.identity.is_some(),
            );
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
            return Err(put_domain(PutDomainError::IdCollision {
                elem: ViewRef::Node(id),
            }));
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

    // Reconstruct fans by replacing each matching projected form with its
    // recorded source form. Removing exactly one projection matters when a
    // source fan survived only partially: retaining the partial fan and then
    // appending the source fan would duplicate one relation after GetPut.
    let fans = restore_source_fans(lens, view, complement, &nodes)?;

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
    instance: &WInstance,
    view: &WInstance,
) -> (ExtraFieldSnapshots, ValueSnapshots) {
    // A surviving node's source fields are recorded exactly when its view
    // fields differ from them, and its source value exactly when its view
    // value differs, compared by canonical bytes: the graph complement's
    // criterion, which the conversion between the two complements maps
    // field for field. Every forward writer (field transforms, term
    // assignments and value coercions) is covered alike.
    let mut original_extra_fields = HashMap::new();
    let mut original_values = HashMap::new();
    for (&id, view_node) in &view.nodes {
        let Some(source_node) = instance.nodes.get(&id) else {
            continue;
        };
        if !same_canonical_fields(&view_node.extra_fields, &source_node.extra_fields) {
            original_extra_fields.insert(id, source_node.extra_fields.clone());
        }
        if !same_canonical_presence(view_node.value.as_ref(), source_node.value.as_ref()) {
            original_values.insert(id, source_node.value.clone());
        }
    }
    (original_extra_fields, original_values)
}

/// Whether two field maps hold the same keys with values of equal canonical
/// bytes.
fn same_canonical_fields(
    a: &HashMap<String, panproto_inst::value::Value>,
    b: &HashMap<String, panproto_inst::value::Value>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            b.get(key)
                .is_some_and(|other| value.canonical_cmp(other).is_eq())
        })
}

/// Whether two node values are equal, a present value by canonical bytes.
fn same_canonical_presence(
    a: Option<&panproto_inst::value::FieldPresence>,
    b: Option<&panproto_inst::value::FieldPresence>,
) -> bool {
    use panproto_inst::value::FieldPresence;
    match (a, b) {
        (Some(FieldPresence::Present(x)), Some(FieldPresence::Present(y))) => {
            x.canonical_cmp(y).is_eq()
        }
        (x, y) => x == y,
    }
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
type SourceArc = (u32, u32, Edge);
type OrderedArcState = (Vec<SourceArc>, HashSet<(u32, u32)>, usize);

fn pull_back_view_arcs(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    nodes: &HashMap<u32, Node>,
) -> (Vec<SourceArc>, HashSet<u32>) {
    let mut desired = Vec::new();
    let mut presentation_children = HashSet::new();
    for (view_parent, child, _) in &view.arcs {
        let presentation_arc = complement.synthesized_nodes.contains(view_parent)
            || complement.synthesized_nodes.contains(child)
            || complement
                .contraction_choices
                .contains_key(&(*view_parent, *child));
        if presentation_arc {
            if !complement.synthesized_nodes.contains(child) {
                presentation_children.insert(*child);
            }
            continue;
        }
        if !nodes.contains_key(child) {
            continue;
        }
        let source_parent = complement
            .original_parent
            .get(child)
            .filter(|_| {
                complement
                    .contraction_choices
                    .contains_key(&(*view_parent, *child))
                    || complement.synthesized_nodes.contains(view_parent)
                    || !nodes.contains_key(view_parent)
            })
            .copied()
            .unwrap_or(*view_parent);
        if !nodes.contains_key(&source_parent) {
            continue;
        }
        if let Some(arc) = find_original_arc(
            lens,
            view,
            nodes,
            source_parent,
            *child,
            &complement.contraction_choices,
            &complement.arc_edges,
        ) {
            desired.push(arc);
        }
    }
    (desired, presentation_children)
}

fn append_ordered_source_arcs(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    nodes: &HashMap<u32, Node>,
    desired: &[SourceArc],
    presentation_children: &HashSet<u32>,
) -> OrderedArcState {
    let mut arcs = Vec::new();
    let mut seen_pairs = HashSet::new();
    let mut desired_index = 0;
    for &(parent, child) in &complement.arc_order {
        if complement.original_parent.contains_key(&child) {
            if presentation_children.contains(&child) {
                let restored = complement
                    .dropped_arcs
                    .iter()
                    .find(|arc| (arc.0, arc.1) == (parent, child))
                    .cloned()
                    .or_else(|| {
                        find_original_arc(
                            lens,
                            view,
                            nodes,
                            parent,
                            child,
                            &complement.contraction_choices,
                            &complement.arc_edges,
                        )
                    });
                if let Some(arc) = restored
                    && seen_pairs.insert((arc.0, arc.1))
                {
                    arcs.push(arc);
                }
            } else if let Some(arc) = desired.get(desired_index) {
                desired_index += 1;
                if seen_pairs.insert((arc.0, arc.1)) {
                    arcs.push(arc.clone());
                }
            }
        } else if let Some(arc) = complement
            .dropped_arcs
            .iter()
            .find(|arc| (arc.0, arc.1) == (parent, child))
            && nodes.contains_key(&arc.0)
            && nodes.contains_key(&arc.1)
            && seen_pairs.insert((parent, child))
        {
            arcs.push(arc.clone());
        }
    }
    (arcs, seen_pairs, desired_index)
}

fn rebuild_source_arcs(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    nodes: &HashMap<u32, Node>,
) -> Vec<SourceArc> {
    // Pull every independent view arc back to a source arc in view order.
    let (desired, presentation_children) = pull_back_view_arcs(lens, view, complement, nodes);
    let (mut arcs, mut seen_pairs, desired_index) = if complement.arc_order.is_empty() {
        // Legacy complements keep observable view order, then hidden arcs.
        let mut arcs = Vec::new();
        let mut seen_pairs = HashSet::new();
        for arc in &desired {
            if seen_pairs.insert((arc.0, arc.1)) {
                arcs.push(arc.clone());
            }
        }
        (arcs, seen_pairs, desired.len())
    } else {
        append_ordered_source_arcs(
            lens,
            view,
            complement,
            nodes,
            &desired,
            &presentation_children,
        )
    };

    for arc in desired.into_iter().skip(desired_index) {
        if seen_pairs.insert((arc.0, arc.1)) {
            arcs.push(arc);
        }
    }
    let recorded_pairs = complement.arc_order.iter().copied().collect::<HashSet<_>>();
    for arc in &complement.dropped_arcs {
        if nodes.contains_key(&arc.0)
            && nodes.contains_key(&arc.1)
            && (complement.arc_order.is_empty() || !recorded_pairs.contains(&(arc.0, arc.1)))
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
pub(crate) fn propagate_view_edits_through_inverse(
    node: &mut Node,
    view_fields: &std::collections::HashMap<String, panproto_inst::value::Value>,
    transforms: &[panproto_inst::FieldTransform],
) {
    use panproto_inst::FieldTransform;

    // A field the source carries and no transform writes, removes or
    // rewrites reaches the view unchanged under its own name, so the view's
    // value of it is an independent coordinate: an edit to it is the
    // user's, and the snapshot's copy is only its value before the edit.
    let untouched = untouched_by(transforms);
    for (key, value) in view_fields {
        if node.extra_fields.contains_key(key) && untouched(key) {
            node.extra_fields.insert(key.clone(), value.clone());
        }
    }

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
                // The inverse recovers the *source* coordinate the forward
                // expression read. The snapshot remains authoritative for
                // every other source coordinate, including an existing
                // coordinate overwritten by the computed key.
                // While the source coordinate is still in the view the
                // computed key is derived from it (`collect_derived_fields`):
                // the view's own value of the source is authoritative and
                // the inverse has no say.
                if let Some(inv_expr) = inverse
                    && working.contains_key(target_key)
                    && let Some(source_key) = crate::derived::sole_source_var(expr)
                    && !working.contains_key(&source_key)
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
            // The node starts from the snapshot of the source's fields, and
            // only keys the snapshot holds are updated, so an added key is
            // present exactly when the source held it: nothing to undo.
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

/// Whether a top-level `extra_fields` key passes through `transforms`
/// unchanged: no transform writes it (as a rename, added, computed or
/// rewritten key, including inside a `Case` branch or under a path), drops
/// it, or leaves it outside a `KeepFields` retain set.
fn untouched_by(transforms: &[panproto_inst::FieldTransform]) -> impl Fn(&str) -> bool + '_ {
    use panproto_inst::FieldTransform;

    fn touches(transform: &FieldTransform, key: &str) -> bool {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => old_key == key || new_key == key,
            FieldTransform::DropField { key: k }
            | FieldTransform::AddField { key: k, .. }
            | FieldTransform::ApplyExpr { key: k, .. }
            | FieldTransform::ComputeField { target_key: k, .. }
            | FieldTransform::MapReferences { field: k, .. } => k == key,
            FieldTransform::KeepFields { keys } => !keys.iter().any(|k| k == key),
            FieldTransform::PathTransform { path, inner } => path
                .first()
                .map_or_else(|| touches(inner, key), |first| first == key),
            FieldTransform::Case { branches } => branches
                .iter()
                .flat_map(|branch| branch.transforms.iter())
                .any(|t| touches(t, key)),
        }
    }

    move |key| !transforms.iter().any(|t| touches(t, key))
}

/// Restore the fields of a node the complement records no source fields
/// for.
///
/// A complement stamped by `get` records a node's source fields exactly
/// when its view fields differ from them, so an unrecorded node's source
/// fields were its view fields: put starts from the view's fields and
/// propagates only the view's edits back through the inverses, which keeps
/// a field the forward pass added or computed when the source already held
/// it. A complement with no stamp carries no such guarantee, and put applies
/// the inverses to the view's fields directly.
pub(crate) fn restore_unrecorded_fields(
    node: &mut Node,
    transforms: &[panproto_inst::FieldTransform],
    stamped: bool,
) {
    if stamped {
        let view_fields = node.extra_fields.clone();
        propagate_view_edits_through_inverse(node, &view_fields, transforms);
    } else {
        apply_inverse_transforms(node, transforms);
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
pub(crate) fn apply_inverse_transforms(
    node: &mut Node,
    transforms: &[panproto_inst::FieldTransform],
) {
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
                    if let Some(panproto_inst::value::FieldPresence::Present(val)) = &node.value
                        && let Ok(input) = panproto_inst::value_to_expr_literal(val)
                    {
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
                        node.extra_fields.insert(
                            source_key.clone(),
                            panproto_inst::expr_literal_to_value(&result),
                        );
                    }
                    if source_key != *target_key {
                        node.extra_fields.remove(target_key);
                    }
                } else {
                    node.extra_fields.remove(target_key);
                }
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
/// function of the view alone. This succeeds precisely for lenses whose
/// every `get` leaves trivial residue ([`Complement::residue_is_trivial`]):
/// every source vertex survives (an injective rename survives), no two
/// source vertices or parallel source edges share an image, no vertex is
/// filtered by a predicate, and every value transform is an isomorphism.
/// It refuses everything else.
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
    if let Some(obstruction) = obstruction_to_tree_isomorphism(lens) {
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

/// The first reason `get` through `lens` can leave residue in a tree
/// complement, or `None` when every complement it produces has trivial
/// residue ([`Complement::residue_is_trivial`]).
///
/// Survival is read as `get` reads it: a source vertex survives when its
/// image under the vertex remap is a surviving target vertex, so an
/// injective rename is not a drop. The obstructions, in the order they are
/// reported: a vertex that does not survive, or survives only under a
/// predicate; two source vertices with one image (their nodes' anchors are
/// then residue); a source edge with no image in the target; two source
/// edges between the same vertices with one image (their arcs' edges are
/// then residue); a fan resolver whose source labels cannot be recovered
/// from the view; and a value transform that is not an isomorphism.
pub(crate) fn obstruction_to_tree_isomorphism(lens: &Lens) -> Option<String> {
    let compiled = &lens.compiled;
    let mut vertices: Vec<&Name> = lens.src_schema.vertices.keys().collect();
    vertices.sort();
    for v in &vertices {
        if !vertex_survives(lens, v) {
            return Some(format!(
                "vertex `{v}` does not survive, so its data is set aside in the complement"
            ));
        }
        if compiled.conditional_survival.contains_key(*v) {
            return Some(format!(
                "vertex `{v}` survives only under a predicate, so the nodes it drops are set \
                 aside in the complement"
            ));
        }
    }
    let mut images: HashMap<&Name, &Name> = HashMap::new();
    for v in &vertices {
        let image = compiled.vertex_remap.get(*v).unwrap_or(v);
        if let Some(other) = images.insert(image, v) {
            return Some(format!(
                "vertices `{other}` and `{v}` both map to `{image}`, so a view anchor does not \
                 determine the source anchor"
            ));
        }
    }
    let mut edges: Vec<&Edge> = lens.src_schema.edges.keys().collect();
    edges.sort();
    let mut edge_images: HashMap<Edge, &Edge> = HashMap::new();
    for e in edges {
        let image = edge_image(lens, e);
        if !compiled.edge_remap.contains_key(e) && !lens.tgt_schema.edges.contains_key(&image) {
            return Some(format!(
                "edge `{}` -> `{}` does not survive, so the arcs over it are set aside in the \
                 complement",
                e.src, e.tgt
            ));
        }
        if let Some(other) = edge_images.insert(image, e) {
            return Some(format!(
                "edges `{:?}` and `{:?}` have one image, so a view arc does not determine its \
                 source edge",
                other.name, e.name
            ));
        }
    }
    if !compiled.hyper_resolver.is_empty() {
        return Some(
            "a hyper-edge resolver retargets a fan or its child labels, which the tree view does \
             not determine without residue"
                .to_owned(),
        );
    }
    let class = lens.coercion_class();
    if !matches!(class, panproto_gat::CoercionClass::Iso) {
        return Some(format!(
            "a value transform is classified `{class:?}`, which is not injective, so a view \
             value does not determine the source value it came from"
        ));
    }
    None
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

/// The view edge `get` carries source edge `source` onto, by the rule
/// `wtype_restrict` applies to a direct source arc: its entry in the
/// explicit edge remap; else the target edge with the same kind and label
/// across the remapped endpoints; else, when `source` is the only source
/// edge between its endpoints, the resolver's edge for the remapped
/// endpoint pair. With none of these the same kind and label across the
/// remapped endpoints is returned, which is no target edge.
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
    let carried = Edge {
        src: remap(&source.src),
        tgt: remap(&source.tgt),
        kind: source.kind.clone(),
        name: source.name.clone(),
    };
    if lens.tgt_schema.edges.contains_key(&carried) {
        return carried;
    }
    if lens
        .src_schema
        .edges_between(&source.src, &source.tgt)
        .len()
        == 1
        && let Some(resolved) = lens
            .compiled
            .resolver
            .get(&(carried.src.clone(), carried.tgt.clone()))
    {
        return resolved.clone();
    }
    carried
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

    /// Tree get emits a parent's kept arcs before its synthesized first hops,
    /// so a view that puts a synthesized arc before a kept sibling has moved
    /// the synthesized arc: it is `SynthesizedEdited`, since put would
    /// otherwise restore a source whose view orders them back.
    mod synthesized_sibling_order {
        use super::*;
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

        /// `r -> x` kept; `r -> m -> y` contracted to `r -> y` named `c`.
        fn lens() -> Lens {
            let src = schema(
                &[
                    ("r", "object"),
                    ("x", "object"),
                    ("m", "object"),
                    ("y", "object"),
                ],
                &[
                    prop("r", "x", "x"),
                    prop("r", "m", "m"),
                    prop("m", "y", "y"),
                ],
            );
            let tgt = schema(
                &[("r", "object"), ("x", "object"), ("y", "object")],
                &[prop("r", "x", "x"), prop("r", "y", "c")],
            );
            let migration = Migration {
                vertex_map: [("r", "r"), ("x", "x"), ("y", "y")]
                    .iter()
                    .map(|(s, t)| (Name::from(*s), Name::from(*t)))
                    .collect(),
                edge_map: [(prop("r", "x", "x"), prop("r", "x", "x"))]
                    .into_iter()
                    .collect(),
                hyper_edge_map: HashMap::new(),
                label_map: HashMap::new(),
                resolver: [((Name::from("r"), Name::from("y")), prop("r", "y", "c"))]
                    .into_iter()
                    .collect(),
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
            WInstance::new(
                [
                    (0, Node::new(0, "r")),
                    (1, Node::new(1, "x")),
                    (2, Node::new(2, "m")),
                    (3, Node::new(3, "y")),
                ]
                .into_iter()
                .collect(),
                vec![
                    (0, 1, prop("r", "x", "x")),
                    (0, 2, prop("r", "m", "m")),
                    (2, 3, prop("m", "y", "y")),
                ],
                Vec::new(),
                0,
                "r".into(),
            )
        }

        #[test]
        fn a_synthesized_arc_moved_before_a_kept_sibling_is_refused() {
            let lens = lens();
            let (view, complement) = get(&lens, &source()).unwrap();
            assert_eq!(
                view.arcs,
                vec![(0, 1, prop("r", "x", "x")), (0, 3, prop("r", "y", "c"))],
                "tree get emits the kept arc, then the synthesized first hop"
            );
            let mut arcs = view.arcs.clone();
            arcs.reverse();
            let moved = WInstance::new(
                view.nodes.clone(),
                arcs,
                view.fans.clone(),
                view.root,
                view.schema_root.clone(),
            );
            if let Ok(restored) = put(&lens, &moved, &complement) {
                let (again, _) = get(&lens, &restored).unwrap();
                println!(
                    "admitted: the view orders {:?}, get after put orders {:?}",
                    moved.arcs, again.arcs
                );
            }
            let refused = check_put_domain(&lens, &moved, &complement).unwrap_err();
            assert!(
                refused
                    .iter()
                    .any(|error| matches!(error, PutDomainError::SynthesizedEdited { .. })),
                "{refused:?}"
            );
        }
    }

    /// A field the migration adds that the source already carries with the
    /// added value is unchanged by get, so get records nothing for the node
    /// and put must keep the field: GetPut in both shapes.
    mod an_added_field_the_source_carries {
        use super::*;
        use panproto_inst::FieldTransform;
        use panproto_inst::GInstance;
        use panproto_inst::value::Value;
        use panproto_schema::{Protocol, SchemaBuilder};

        fn lens() -> Lens {
            let protocol = Protocol::default();
            let schema = SchemaBuilder::new(&protocol)
                .vertex("a", "object", None)
                .unwrap()
                .build()
                .unwrap();
            let mut lens = crate::tests::identity_lens(&schema);
            lens.compiled.field_transforms.insert(
                "a".into(),
                vec![FieldTransform::AddField {
                    key: "f".to_owned(),
                    value: Value::Int(0),
                }],
            );
            lens
        }

        fn carrying() -> Node {
            let mut node = Node::new(0, "a");
            node.extra_fields.insert("f".to_owned(), Value::Int(0));
            node
        }

        #[test]
        fn tree_put_keeps_it() {
            let lens = lens();
            let source = WInstance::new(
                [(0, carrying())].into_iter().collect(),
                Vec::new(),
                Vec::new(),
                0,
                "a".into(),
            );
            let (view, complement) = get(&lens, &source).unwrap();
            assert!(complement.original_extra_fields.is_empty());
            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(
                restored.nodes[&0].extra_fields.get("f"),
                Some(&Value::Int(0))
            );
        }

        #[test]
        fn graph_put_keeps_it() {
            let lens = lens();
            let source = GInstance::new().with_node(carrying());
            let (view, complement) = crate::graph_lens::get(&lens, &source).unwrap();
            assert!(complement.original_extra_fields.is_empty());
            let restored = crate::graph_lens::put(&lens, &view, &complement).unwrap();
            assert_eq!(
                restored.node(0).unwrap().extra_fields.get("f"),
                Some(&Value::Int(0))
            );
        }
    }

    /// A field computed by a term assignment is derived: get records the
    /// source fields it replaced, and a view that edits it is outside the
    /// put domain in both shapes.
    mod term_assignments {
        use super::*;
        use panproto_inst::value::Value;
        use panproto_inst::{GInstance, TermAssignment, TermScope};
        use panproto_schema::{Protocol, SchemaBuilder};

        fn lens() -> Lens {
            let protocol = Protocol::default();
            let schema = SchemaBuilder::new(&protocol)
                .vertex("a", "object", None)
                .unwrap()
                .build()
                .unwrap();
            let mut lens = crate::tests::identity_lens(&schema);
            lens.compiled.op_term_assignments.insert(
                "a".into(),
                vec![TermAssignment::Compute {
                    target: "f".to_owned(),
                    scope: TermScope::Row,
                    term: panproto_expr::Expr::Lit(panproto_expr::Literal::Int(0)),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Projection,
                }],
            );
            lens
        }

        fn without_f(node: &mut Node) {
            node.extra_fields.remove("f");
        }

        #[test]
        fn tree_get_then_put_restores_the_fields_a_term_replaced() {
            let lens = lens();
            let source = WInstance::new(
                [(0, Node::new(0, "a"))].into_iter().collect(),
                Vec::new(),
                Vec::new(),
                0,
                "a".into(),
            );
            let (view, complement) = get(&lens, &source).unwrap();
            assert_eq!(view.nodes[&0].extra_fields.get("f"), Some(&Value::Int(0)));
            let restored = put(&lens, &view, &complement).unwrap();
            assert!(restored.nodes[&0].extra_fields.is_empty());
        }

        #[test]
        fn tree_put_refuses_an_edited_computed_field() {
            let lens = lens();
            let source = WInstance::new(
                [(0, Node::new(0, "a"))].into_iter().collect(),
                Vec::new(),
                Vec::new(),
                0,
                "a".into(),
            );
            let (view, complement) = get(&lens, &source).unwrap();
            let mut edited = view.clone();
            without_f(edited.nodes.get_mut(&0).unwrap());
            let refused = check_put_domain(&lens, &edited, &complement).unwrap_err();
            assert!(
                matches!(refused.as_slice(), [PutDomainError::DerivedCellEdited { field, .. }] if field == "f"),
                "{refused:?}"
            );
        }

        #[test]
        fn graph_put_refuses_an_edited_computed_field() {
            let lens = lens();
            let source = GInstance::new().with_node(Node::new(0, "a"));
            let (view, complement) = crate::graph_lens::get(&lens, &source).unwrap();
            let mut edited = view.clone();
            without_f(edited.node_mut(0).unwrap());
            let refused =
                crate::graph_lens::check_put_domain(&lens, &edited, &complement).unwrap_err();
            assert!(
                matches!(refused.as_slice(), [PutDomainError::DerivedCellEdited { field, .. }] if field == "f"),
                "{refused:?}"
            );
        }
    }

    /// A child added under a lone root is written back: the complement
    /// `get` stamped records no parent because the source had no arc, and
    /// that is no evidence against its provenance.
    mod lone_root {
        use super::*;
        use panproto_schema::{Edge, Protocol, SchemaBuilder};

        fn child_edge() -> Edge {
            Edge {
                src: "r".into(),
                tgt: "c".into(),
                kind: "prop".into(),
                name: Some("c".into()),
            }
        }

        fn lens() -> Lens {
            let protocol = Protocol::default();
            let schema = SchemaBuilder::new(&protocol)
                .vertex("r", "object", None)
                .unwrap()
                .vertex("c", "object", None)
                .unwrap()
                .edge("r", "c", "prop", Some("c"))
                .unwrap()
                .build()
                .unwrap();
            crate::tests::identity_lens(&schema)
        }

        fn lone_root() -> WInstance {
            WInstance::new(
                [(0, Node::new(0, "r"))].into_iter().collect(),
                Vec::new(),
                Vec::new(),
                0,
                "r".into(),
            )
        }

        fn with_child(view: &WInstance) -> WInstance {
            let mut nodes = view.nodes.clone();
            nodes.insert(1, Node::new(1, "c"));
            let mut arcs = view.arcs.clone();
            arcs.push((0, 1, child_edge()));
            WInstance::new(
                nodes,
                arcs,
                view.fans.clone(),
                view.root,
                view.schema_root.clone(),
            )
        }

        #[test]
        fn a_child_added_under_a_lone_root_is_put_back() {
            let lens = lens();
            let (view, complement) = get(&lens, &lone_root()).unwrap();
            assert!(complement.original_parent.is_empty());
            let edited = with_child(&view);
            assert_eq!(check_put_domain(&lens, &edited, &complement), Ok(()));
            let restored = put(&lens, &edited, &complement).unwrap();
            assert_eq!(restored.nodes.len(), 2);
            assert_eq!(restored.arcs, vec![(0, 1, child_edge())]);
            let (again, _) = get(&lens, &restored).unwrap();
            assert_eq!(again.arcs, edited.arcs);
        }

        #[test]
        fn an_unbound_complement_recording_no_parent_is_still_refused() {
            let lens = lens();
            let (view, _) = get(&lens, &lone_root()).unwrap();
            let edited = with_child(&view);
            assert!(matches!(
                put(&lens, &edited, &Complement::empty()),
                Err(LensError::ComplementMismatch { .. })
            ));
        }
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
                Err(LensError::PutDomain(error)) => match *error {
                    PutDomainError::AmbiguousReverseAnchor {
                        elem: ViewRef::Node(1),
                        candidates,
                    } => assert_eq!(candidates, vec![Name::from("d"), Name::from("e")]),
                    other => {
                        panic!("expected an ambiguous reverse anchor for node 1, got {other:?}")
                    }
                },
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
                Err(LensError::PutDomain(error)) => match *error {
                    PutDomainError::AmbiguousReverseAnchor {
                        elem: ViewRef::Node(1),
                        candidates,
                    } => assert_eq!(candidates, vec![Name::from("d"), Name::from("e")]),
                    other => {
                        panic!("expected an ambiguous reverse anchor for node 1, got {other:?}")
                    }
                },
                other => panic!("expected an ambiguous reverse anchor for node 1, got {other:?}"),
            }
        }

        /// `a` is renamed onto `t`, and the source's own vertex `t`, which
        /// the lens drops, carries an edge with the same label as `a`'s.
        fn dropped_namesake_lens() -> Lens {
            let src_edges = vec![prop("r", "a", "k"), prop("r", "t", "k")];
            let tgt_edges = vec![prop("r", "t", "k")];
            let src = schema(
                &[("r", "object"), ("a", "string"), ("t", "string")],
                &src_edges,
            );
            let tgt = schema(&[("r", "object"), ("t", "string")], &tgt_edges);
            let migration = Migration {
                vertex_map: [("r", "r"), ("a", "t")]
                    .iter()
                    .map(|(s, t)| (Name::from(*s), Name::from(*t)))
                    .collect(),
                edge_map: src_edges.into_iter().take(1).zip(tgt_edges).collect(),
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
        fn a_rename_onto_the_name_of_a_dropped_vertex_round_trips() {
            let lens = dropped_namesake_lens();
            let nodes = vec![Node::new(0, "r"), leaf(1, "a", "one")];
            let source = WInstance::new(
                nodes.into_iter().map(|n| (n.id, n)).collect(),
                vec![(0, 1, prop("r", "a", "k"))],
                Vec::new(),
                0,
                "r".into(),
            );
            let (view, complement) = get(&lens, &source).unwrap();
            assert_eq!(view.nodes[&1].anchor, "t");

            let restored = put(&lens, &view, &complement).unwrap();
            assert_eq!(restored.nodes[&1].anchor, "a", "GetPut lost the anchor");
            assert_eq!(arc_edge(&restored, 0, 1), &prop("r", "a", "k"));
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
