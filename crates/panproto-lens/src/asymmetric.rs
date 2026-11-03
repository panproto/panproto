//! Asymmetric lens operations: `get` and `put` with complement tracking.
//!
//! The `get` direction runs the restrict pipeline (forward migration) while
//! capturing everything that was discarded into a [`Complement`]. The `put`
//! direction restores the original source instance from a (possibly modified)
//! view plus the complement.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::identity::{
    ComplementShape, IdentityCarrier, IdentityEnvelope, IdentityPolicy, IdentityStatus,
    compose_identities,
};
use panproto_inst::{
    EdgeFate, EdgeId, Fan, FanResolver, FanViolation, GInstance, InstError, Node, PutDomainError,
    RestrictError, RestrictPlan, SynthKey, ViewRef, WInstance, canonical_label_shape,
    wtype_restrict_with_plan,
};
use panproto_schema::{Edge, Schema};

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

        // Arc order is one global sequence, not a keyed resource. Two
        // non-empty records are compatible only when they state the same
        // sequence: sorting would destroy collection order, while a
        // left-biased union would make composition non-commutative.
        let arc_order = match (self.arc_order.is_empty(), other.arc_order.is_empty()) {
            (true, _) => other.arc_order.clone(),
            (_, true) => self.arc_order.clone(),
            (false, false) if self.arc_order == other.arc_order => self.arc_order.clone(),
            (false, false) => {
                return Err(LensError::ComplementConflict {
                    kind: "arc_order",
                    key: "ordered sequence".to_owned(),
                });
            }
        };

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
        let mut preimages: HashMap<Edge, Vec<Edge>> = HashMap::new();
        if let Ok(plan) = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
        {
            for (source, fate) in &plan.edge_image.fates {
                if let EdgeFate::Keep(image) = fate {
                    preimages
                        .entry(image.clone())
                        .or_default()
                        .push(source.clone());
                }
            }
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
    let view = derived_cells_on_fields(lens, view, complement);
    if let Err(found) = check_put_domain(lens, &view, complement)
        && let Some(first) = found.into_iter().next()
    {
        return Err(LensError::PutDomain(Box::new(first)));
    }
    let source = reassemble(lens, &view, complement)?;
    Ok((source, status))
}

/// `view` with every derived cell that it presents as a child held among
/// its node's own fields again, where `get` wrote it.
///
/// A computed field (`ComputeField`) or a default (`AddField`) is written
/// into the fields of the node it computes for. Serialization writes such a
/// field and a child attribute of the same name alike, and a reader of the
/// serialized view, which knows the target schema but not the lens, reads
/// the attribute back as a scalar child on the edge the field names. That
/// child is a presentation of the derived cell, not a structural insertion:
/// the source held no child there, and the view holds no field of that name.
///
/// A child is folded back into its parent's field `key` exactly when the
/// node's forward transforms derive `key`, the node holds no field `key`,
/// and the child is the only one under an edge named `key`, has no children
/// of its own, takes part in no fan, and is not a source child of the node
/// (the complement records no source arc between them). The child presents
/// a scalar or null as its own value with no fields, and a record as its
/// fields with no value of its own (see [`presented_cell`]). The
/// derived cell is then checked and restored as any field: an unedited one
/// is put back to the source it was computed from, and an edited one is
/// refused as an edit of a derived cell.
fn derived_cells_on_fields<'v>(
    lens: &Lens,
    view: &'v WInstance,
    complement: &Complement,
) -> std::borrow::Cow<'v, WInstance> {
    if lens.compiled.field_transforms.is_empty() && lens.compiled.op_term_assignments.is_empty()
    {
        return std::borrow::Cow::Borrowed(view);
    }
    let reverse_anchors = ReverseAnchors::new(&lens.compiled.vertex_remap);
    let source_arcs: HashSet<(u32, u32)> = complement.arc_order.iter().copied().collect();
    let mut children: HashMap<u32, Vec<(u32, &Edge)>> = HashMap::new();
    for (parent, child, edge) in &view.arcs {
        children.entry(*parent).or_default().push((*child, edge));
    }
    let fanned: HashSet<u32> = view
        .fans
        .iter()
        .flat_map(|fan| std::iter::once(fan.parent).chain(fan.children.values().copied()))
        .collect();

    let mut ids: Vec<u32> = view.nodes.keys().copied().collect();
    ids.sort_unstable();
    let mut folds: Vec<(u32, String, u32, panproto_inst::value::Value)> = Vec::new();
    for id in ids {
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let node = &view.nodes[&id];
        let anchor = match complement.original_anchors.get(&id) {
            Some(anchor) => anchor,
            None => match reverse_anchors.source_anchor(&node.anchor) {
                Ok(anchor) => anchor,
                Err(_) => continue,
            },
        };
        let mut keys = BTreeSet::new();
        derived_cell_keys(&lens.compiled.value_transforms(anchor), &mut keys);
        for key in keys {
            if node.extra_fields.contains_key(&key) {
                continue;
            }
            let mut named = children.get(&id).into_iter().flatten().filter(|(_, edge)| {
                edge.name.as_deref().unwrap_or_else(|| edge.tgt.as_str()) == key
            });
            let (Some(&(child, _)), None) = (named.next(), named.next()) else {
                continue;
            };
            let Some(child_node) = view.nodes.get(&child) else {
                continue;
            };
            let folds_here = !children.contains_key(&child)
                && !fanned.contains(&child)
                && !source_arcs.contains(&(id, child))
                && !complement.synthesized_nodes.contains(&child);
            if folds_here && let Some(value) = presented_cell(child_node) {
                folds.push((id, key, child, value));
            }
        }
    }
    if folds.is_empty() {
        return std::borrow::Cow::Borrowed(view);
    }

    let mut nodes = view.nodes.clone();
    let mut folded = HashSet::new();
    for (parent, key, child, value) in folds {
        nodes.remove(&child);
        if let Some(parent) = nodes.get_mut(&parent) {
            parent.extra_fields.insert(key, value);
        }
        folded.insert(child);
    }
    let arcs = view
        .arcs
        .iter()
        .filter(|(_, child, _)| !folded.contains(child))
        .cloned()
        .collect();
    std::borrow::Cow::Owned(WInstance::new(
        nodes,
        arcs,
        view.fans.clone(),
        view.root,
        view.schema_root.clone(),
    ))
}

/// The cell value a leaf child presents: its own scalar, or null, when it
/// holds no fields; the record of its fields when it holds no value of its
/// own. A serialized record read back at a vertex whose protocol classifies
/// no value kind lands as such fields. `None` for a child holding both, or
/// an absent value with no fields.
fn presented_cell(child: &Node) -> Option<panproto_inst::value::Value> {
    use panproto_inst::value::{FieldPresence, Value};

    match &child.value {
        Some(FieldPresence::Present(value)) if child.extra_fields.is_empty() => {
            Some(value.clone())
        }
        Some(FieldPresence::Null) if child.extra_fields.is_empty() => Some(Value::Null),
        None if !child.extra_fields.is_empty() => Some(Value::Unknown(child.extra_fields.clone())),
        _ => None,
    }
}

/// The fields `transforms` derive at the node they run on: the targets of
/// computed fields and defaults, in every case branch and under an empty
/// path.
fn derived_cell_keys(transforms: &[panproto_inst::FieldTransform], keys: &mut BTreeSet<String>) {
    use panproto_inst::FieldTransform;

    for transform in transforms {
        match transform {
            FieldTransform::ComputeField { target_key, .. } => {
                keys.insert(target_key.clone());
            }
            FieldTransform::AddField { key, .. } => {
                keys.insert(key.clone());
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                derived_cell_keys(std::slice::from_ref(inner), keys);
            }
            FieldTransform::Case { branches } => {
                for branch in branches {
                    derived_cell_keys(&branch.transforms, keys);
                }
            }
            _ => {}
        }
    }
}

/// The domain predicate of tree [`put`]: every reason `view` cannot be
/// written back through `lens` against `complement`, or `Ok` when `put`
/// accepts it.
///
/// Checked:
///
/// * [`PutDomainError::PlanCompilation`]: the migration has no executable
///   restrict plan, so no view is in its put domain.
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
/// * [`PutDomainError::DuplicateKey`], [`PutDomainError::MissingRequiredCell`]
///   and [`PutDomainError::MalformedFan`]: the restored source violates the
///   source schema's key, required-cell or hyper-edge fan declarations.
/// * [`PutDomainError::KeyRefused`]: the shared keying kernel refuses a
///   restored value of a source uniqueness key (for instance a value with
///   no canonical bytes), so that key's uniqueness is undecided.
///
/// All violations are reported, ordered by variant (the declaration order
/// of [`PutDomainError`]) and then by the element's [`ViewRef`]. Duplicate
/// keys at the same element are ordered by table and canonical key; missing
/// required cells at the same element are ordered by field. Other ties retain
/// their discovery order. The value checks run on every view node whose
/// restoration a structural violation leaves determined, so a structural
/// violation elsewhere in the view hides none of them: a view node colliding
/// with a dropped node is checked as
/// the view holds it, and only a node with no single source anchor, or
/// under an arc with no single source edge, is left out together with the
/// view subtree below it, since `put` has nothing to restore it as. Put is
/// total on views for which this returns `Ok`.
///
/// A computed field or default that the view presents as a scalar child of
/// its node, as a serialized view read back under the target schema does, is
/// checked as the field `get` wrote: an unedited one is accepted and an
/// edited one is refused with [`PutDomainError::DerivedCellEdited`].
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
    if let Err(error) = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled) {
        return Err(vec![PutDomainError::PlanCompilation {
            detail: error.to_string(),
        }]);
    }
    let canonical = derived_cells_on_fields(lens, view, complement);
    let view = &*canonical;
    let mut found = structural_violations(lens, view, complement);
    let restorable = if found.is_empty() {
        None
    } else {
        Some(restorable_part(view, complement, &found))
    };
    let (view, complement) = restorable
        .as_ref()
        .map_or((view, complement), |(view, complement)| (view, complement));
    match reassemble(lens, view, complement) {
        Ok(source) => found.extend(value_violations(lens, view, complement, &source)),
        Err(LensError::PutDomain(error)) => found.push(*error),
        Err(LensError::Restrict(error)) => {
            found.extend(put_refusal_from_restrict(lens, view, error));
        }
        Err(error) => found.push(PutDomainError::SurvivalPredicateRefused {
            elem: ViewRef::Node(view.root),
            refusal: panproto_inst::PredicateRefusal::EvaluationFailed {
                message: error.to_string(),
            },
        }),
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
fn remove_reserved_node_residue(complement: &mut Complement, ids: &BTreeSet<u32>) {
    let mut incident_source_edges = BTreeSet::new();
    let mut source_edge_reindex = BTreeMap::new();
    let mut retained_index = 0_usize;
    for (old_index, (parent, child)) in complement.arc_order.iter().enumerate() {
        let Ok(old_index) = u32::try_from(old_index) else {
            retained_index += usize::from(!ids.contains(parent) && !ids.contains(child));
            continue;
        };
        if ids.contains(parent) || ids.contains(child) {
            incident_source_edges.insert(EdgeId(old_index));
        } else {
            if let Ok(new_index) = u32::try_from(retained_index) {
                source_edge_reindex.insert(EdgeId(old_index), EdgeId(new_index));
            }
            retained_index += 1;
        }
    }
    let mentions_collision = |elem: &ViewRef| match elem {
        ViewRef::Node(node) => ids.contains(node),
        ViewRef::Arc { parent, child } => ids.contains(parent) || ids.contains(child),
        ViewRef::Edge(_) | ViewRef::Row(_) | ViewRef::Pair { .. } => false,
    };
    complement.dropped_nodes.retain(|id, _| !ids.contains(id));
    complement
        .dropped_arcs
        .retain(|(parent, child, _)| !ids.contains(parent) && !ids.contains(child));
    complement.dropped_fans.retain(|fan| {
        !ids.contains(&fan.parent) && fan.children.values().all(|child| !ids.contains(child))
    });
    complement
        .contraction_choices
        .retain(|(parent, child), _| !ids.contains(parent) && !ids.contains(child));
    complement
        .original_parent
        .retain(|child, parent| !ids.contains(child) && !ids.contains(parent));
    complement
        .arc_edges
        .retain(|(parent, child), _| !ids.contains(parent) && !ids.contains(child));
    complement
        .arc_order
        .retain(|(parent, child)| !ids.contains(parent) && !ids.contains(child));
    complement
        .original_extra_fields
        .retain(|id, _| !ids.contains(id));
    complement.original_values.retain(|id, _| !ids.contains(id));
    complement.synthesized_nodes.retain(|id| !ids.contains(id));
    complement
        .contracted_into
        .retain(|source, target| !ids.contains(source) && !ids.contains(target));
    complement
        .original_anchors
        .retain(|id, _| !ids.contains(id));
    complement.synthesized_keys.retain(|node, key| {
        !ids.contains(node)
            && match key {
                SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
                    !ids.contains(src) && !ids.contains(tgt)
                }
                SynthKey::Expand { source_edge, .. } => {
                    if incident_source_edges.contains(source_edge) {
                        false
                    } else if let Some(reindexed) = source_edge_reindex.get(source_edge) {
                        *source_edge = *reindexed;
                        true
                    } else {
                        false
                    }
                }
                SynthKey::ResolutionReference { holder, .. } => !mentions_collision(holder),
                SynthKey::ResolutionSpan { text, .. } => !mentions_collision(text.site().holder()),
            }
    });
}

fn restorable_part(
    view: &WInstance,
    complement: &Complement,
    found: &[PutDomainError],
) -> (WInstance, Complement) {
    let mut complement = complement.clone();
    let collisions = found
        .iter()
        .filter_map(|violation| match violation {
            PutDomainError::IdCollision {
                elem: ViewRef::Node(id),
            } => Some(*id),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if !collisions.is_empty() {
        remove_reserved_node_residue(&mut complement, &collisions);
    }
    let mut unrestorable: Vec<u32> = Vec::new();
    for violation in found {
        match violation {
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
    PutDomainError::sort_report(&mut found);
    found
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
/// translated to the source. The one exception is removing an original
/// endpoint of the synthesized chain: the presentation-only chain then
/// disappears with a representable source-node deletion. Detecting other
/// edits before reassembly is
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
        let endpoint_removed = complement
            .synthesized_keys
            .get(&id)
            .is_some_and(|key| synth_chain_endpoint_removed(view, complement, key));
        if !unchanged && !endpoint_removed {
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
        if !matches && !synthesized_arc_endpoint_removed(view, complement, parent, child, expected)
        {
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

fn synth_chain_endpoints(complement: &Complement, key: &SynthKey) -> Option<(u32, u32)> {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            Some((*src, *tgt))
        }
        SynthKey::Expand { source_edge, .. } => usize::try_from(source_edge.0)
            .ok()
            .and_then(|index| complement.arc_order.get(index).copied()),
        SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => None,
    }
}

fn synth_chain_endpoint_removed(view: &WInstance, complement: &Complement, key: &SynthKey) -> bool {
    synth_chain_endpoints(complement, key)
        .is_some_and(|(src, tgt)| !view.nodes.contains_key(&src) || !view.nodes.contains_key(&tgt))
}

fn synthesized_arc_endpoint_removed(
    view: &WInstance,
    complement: &Complement,
    parent: u32,
    child: u32,
    edge: &Edge,
) -> bool {
    let key = complement
        .synthesized_keys
        .get(&child)
        .or_else(|| complement.synthesized_keys.get(&parent));
    if let Some(key) = key {
        return synth_chain_endpoint_removed(view, complement, key);
    }
    complement
        .contraction_choices
        .get(&(parent, child))
        .filter(|recorded| *recorded == edge)
        .is_some_and(|recorded| {
            synth_chain_endpoint_removed(
                view,
                complement,
                &SynthKey::Contract {
                    src: parent,
                    tgt: child,
                    edge: recorded.clone(),
                },
            )
        })
}

/// The canonical synthesized-arc order for every parent.
///
/// Tree get emits each parent's kept arcs before its synthesized arcs, and
/// emits those synthesized arcs in [`SynthKey`] order. The flat arc vector
/// may interleave distinct parents arbitrarily: only a parent's own child
/// sequence is observable. If one parent violates either ordering rule, the
/// smallest synthesized key among all invalid parents supplies the one
/// canonical tree-coordinate witness.
fn synthesized_sibling_order(view: &WInstance, complement: &Complement) -> Vec<PutDomainError> {
    #[derive(Default)]
    struct SiblingOrder {
        previous_key: Option<SynthKey>,
        synthesized: Vec<(SynthKey, u32, u32)>,
        invalid: bool,
    }

    let Ok((_, embedding)) = GInstance::from_wtype_embedded(view, None) else {
        return Vec::new();
    };
    // Expansion-chain arcs are named by the synthesized node key (and the
    // terminal successor of that key), not by a presentation-dependent
    // `Contract` key. This is the same normalization TreeShape uses.
    let mut indexed_complement = complement.clone();
    indexed_complement
        .contraction_choices
        .retain(|&(parent, child), _| {
            !complement.synthesized_keys.contains_key(&parent)
                && !complement.synthesized_keys.contains_key(&child)
        });
    let index = indexed_complement.synth_index(&embedding);

    let mut parents: BTreeMap<u32, SiblingOrder> = BTreeMap::new();
    for (parent, child, _) in &view.arcs {
        let state = parents.entry(*parent).or_default();
        let key = embedding
            .arcs
            .get(child)
            .and_then(|edge| index.edges.get(edge));
        let Some(key) = key else {
            if !state.synthesized.is_empty() {
                state.invalid = true;
            }
            continue;
        };
        if state
            .previous_key
            .as_ref()
            .is_some_and(|previous| previous > key)
        {
            state.invalid = true;
        }
        state.previous_key = Some(key.clone());
        state.synthesized.push((key.clone(), *parent, *child));
    }

    parents
        .into_values()
        .filter(|state| state.invalid)
        .flat_map(|state| state.synthesized)
        .min()
        .map(|(_, parent, child)| PutDomainError::SynthesizedEdited {
            elem: ViewRef::Arc { parent, child },
        })
        .into_iter()
        .collect()
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

/// Survival predicates that are not true after writing the view back.
fn survival_violations(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    source: &WInstance,
) -> Vec<PutDomainError> {
    if lens.compiled.conditional_survival.is_empty() {
        return Vec::new();
    }
    let Ok(plan) = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled) else {
        return Vec::new();
    };

    // Every predicate is evaluated on the restored source exactly as the
    // next `get` evaluates it: over the node's fields and its children
    // bound by column name.
    let verdicts = panproto_inst::tree_survival_verdicts(source, &plan);
    let mut found = Vec::new();
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
    found
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
    let compiled = &lens.compiled;
    let mut found = tree_schema_violations(source, &lens.src_schema, view);
    let rewrites_fields =
        !compiled.field_transforms.is_empty() || !compiled.op_term_assignments.is_empty();
    if !rewrites_fields && compiled.conditional_survival.is_empty() {
        return in_report_order(found);
    }
    found.extend(survival_violations(lens, view, complement, source));
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
    // Reassembly already validated this baseline; report any discrepancy
    // instead of treating a failed trace as an empty footprint.
    let baseline = match reassemble_baseline(lens, view, complement) {
        Ok(baseline) => baseline,
        Err(error) => {
            found.push(PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(view.root),
                refusal: panproto_inst::PredicateRefusal::EvaluationFailed {
                    message: error.to_string(),
                },
            });
            return in_report_order(found);
        }
    };
    for (&id, view_node) in &view.nodes {
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let (Some(_), Some(again)) = (source.nodes.get(&id), recomputed.nodes.get(&id)) else {
            continue;
        };
        let mut transforms = match tree_transform_trace(lens, &baseline, id) {
            Ok(trace) => trace.operations,
            Err(error) => {
                found.extend(put_refusal_from_restrict(lens, &baseline, error));
                continue;
            }
        };
        match tree_transform_trace(lens, source, id) {
            Ok(trace) => transforms.extend(trace.operations),
            Err(error) => {
                found.extend(put_refusal_from_restrict(lens, source, error));
                continue;
            }
        }
        found.extend(replay_disagreements(
            view,
            id,
            view_node,
            again,
            &transforms,
        ));
    }
    in_report_order(found)
}

/// The coordinates of node `id` that a transform writes and whose shown
/// value differs from the forward replay `again`, each refused under the
/// last transform writing it.
fn replay_disagreements(
    view: &WInstance,
    id: u32,
    view_node: &Node,
    again: &Node,
    transforms: &[panproto_inst::FieldTransform],
) -> Vec<PutDomainError> {
    let mut found = Vec::new();
    let mut written =
        transformed_coordinates(transforms, &view_node.extra_fields, &again.extra_fields);
    // The last transform writing a key decides what the key is.
    written.reverse();
    let mut seen = HashSet::new();
    for (coordinate, transform) in written {
        if !seen.insert(coordinate) {
            continue;
        }
        let differs = match coordinate {
            TransformCoordinate::OwnValue => {
                !presence_equiv(view_node.value.as_ref(), again.value.as_ref())
            }
            TransformCoordinate::Field(key) => {
                match (view_node.extra_fields.get(key), again.extra_fields.get(key)) {
                    (Some(shown), Some(recomputed)) => !value_equiv(shown, recomputed),
                    (None, Some(recomputed)) => inverse_child_output(
                        transform,
                        key,
                        view.arcs.iter().filter_map(|(parent, child, edge)| {
                            (*parent == id)
                                .then(|| view.nodes.get(child).map(|node| (edge, node)))
                                .flatten()
                        }),
                    )
                    .is_none_or(|shown| !value_equiv(shown, recomputed)),
                    (Some(_), None) => true,
                    (None, None) => false,
                }
            }
        };
        if !differs {
            continue;
        }
        let field = coordinate.label().to_owned();
        let elem = ViewRef::Node(id);
        found.push(match transform {
            panproto_inst::FieldTransform::AddField { .. }
            | panproto_inst::FieldTransform::ComputeField { .. } => {
                PutDomainError::DerivedCellEdited { elem, field }
            }
            _ => PutDomainError::TransformNotInvertible { elem, field },
        });
    }
    found
}

/// Every native fan violation of a restored tree under `schema`.
///
/// This is the tree-side fan-domain predicate: a fan's parent and every child
/// must be present, its hyper-edge must be declared, and every child label
/// must occur in that hyper-edge's signature. Labels are examined in lexical
/// order so the report is independent of [`HashMap`] iteration order.
fn native_tree_fan_violations(
    source: &WInstance,
    schema: &panproto_schema::Schema,
) -> Vec<PutDomainError> {
    let mut found = Vec::new();
    for fan in &source.fans {
        if !source.nodes.contains_key(&fan.parent) {
            found.push(PutDomainError::MalformedFan {
                node: fan.parent,
                reason: FanViolation::MissingParent,
            });
        }

        let mut labels: Vec<&str> = fan.children.keys().map(String::as_str).collect();
        labels.sort_unstable();
        for label in &labels {
            if !source.nodes.contains_key(&fan.children[*label]) {
                found.push(PutDomainError::MalformedFan {
                    node: fan.parent,
                    reason: FanViolation::MissingChild {
                        label: Name::from(*label),
                    },
                });
            }
        }

        match schema.hyper_edges.get(fan.hyper_edge_id.as_str()) {
            None => found.push(PutDomainError::MalformedFan {
                node: fan.parent,
                reason: FanViolation::UnknownHyperEdge {
                    hyper_edge: Name::from(fan.hyper_edge_id.as_str()),
                },
            }),
            Some(hyper_edge) => {
                for label in labels {
                    if !hyper_edge.signature.contains_key(label) {
                        found.push(PutDomainError::MalformedFan {
                            node: fan.parent,
                            reason: FanViolation::UnknownLabel {
                                label: Name::from(label),
                            },
                        });
                    }
                }
            }
        }
    }
    found
}

/// Validate a restored tree under the source schema through its canonical
/// graph embedding. Required cells and keys are independent of native fans,
/// so the embedding omits fans and the tree-side predicate above reports
/// their exact coordinates. This preserves required/key diagnostics even
/// when a missing fan participant makes the full embedding impossible. The
/// primary-key presence rule applies to the restored nodes of `view`.
fn tree_schema_violations(
    source: &WInstance,
    schema: &panproto_schema::Schema,
    view: &WInstance,
) -> Vec<PutDomainError> {
    let mut found = native_tree_fan_violations(source, schema);
    let fanless = WInstance::new(
        source.nodes.clone(),
        source.arcs.clone(),
        Vec::new(),
        source.root,
        source.schema_root.clone(),
    );
    match GInstance::from_wtype_with_schema(&fanless, schema) {
        Ok(graph) => found.extend(crate::graph_lens::schema_violations(
            &graph,
            schema,
            &|id| view.nodes.contains_key(&id),
        )),
        Err(InstError::MalformedFan { node, reason }) => {
            found.push(PutDomainError::MalformedFan { node, reason });
        }
        Err(error @ InstError::ReservedVocabulary { .. }) => {
            found.push(reserved_vocabulary_violation(&fanless).unwrap_or_else(|| {
                PutDomainError::SurvivalPredicateRefused {
                    elem: ViewRef::Node(source.root),
                    refusal: panproto_inst::PredicateRefusal::EvaluationFailed {
                        message: error.to_string(),
                    },
                }
            }));
        }
        Err(error) => {
            found.push(PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(source.root),
                refusal: panproto_inst::PredicateRefusal::EvaluationFailed {
                    message: error.to_string(),
                },
            });
        }
    }
    found
}

/// The graph-coordinate fan violation of a fanless tree that holds reserved
/// fan vocabulary, which the tree embedding refuses outright.
///
/// The tree's nodes and its arcs, under the edge ids the canonical embedding
/// assigns in arc order, form the graph the put domain reports against; the
/// first [`FanDomain`](panproto_inst::FanDomain) violation at its least
/// offending node is the reported coordinate. `None` when that graph cannot
/// be built or holds no fan violation.
fn reserved_vocabulary_violation(fanless: &WInstance) -> Option<PutDomainError> {
    let mut graph = GInstance::new();
    let mut ids: Vec<u32> = fanless.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        graph.insert_node(fanless.nodes[&id].clone()).ok()?;
    }
    for (index, (src, tgt, edge)) in fanless.arcs.iter().enumerate() {
        let id = panproto_inst::EdgeId(u32::try_from(index).ok()?);
        graph
            .insert_edge(id, panproto_inst::GEdge::new(*src, *tgt, edge.clone()))
            .ok()?;
    }
    match panproto_inst::FanDomain::check(&graph) {
        Err(InstError::MalformedFan { node, reason }) => {
            Some(PutDomainError::MalformedFan { node, reason })
        }
        _ => None,
    }
}

/// Translate a forward refusal on the reconstructed source into the typed
/// put-domain reason that prevents `PutGet` from attempting that source.
fn put_refusal_from_restrict(
    lens: &Lens,
    source: &WInstance,
    error: panproto_inst::RestrictError,
) -> Vec<PutDomainError> {
    use panproto_inst::{PredicateRefusal, RestrictError};

    let (error, location) = match error {
        RestrictError::At { key, error } => (*error, Some(key.id)),
        other => (other, None),
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
            let node = location
                .or_else(|| {
                    ids.iter().copied().find(|id| {
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
            elem: ViewRef::Node(location.unwrap_or(source.root)),
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

/// A transform's coordinate; the own value has a separate namespace from fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TransformCoordinate<'a> {
    Field(&'a str),
    OwnValue,
}

impl<'a> TransformCoordinate<'a> {
    /// The coordinate's label in a put-domain report.
    pub(crate) const fn label(self) -> &'a str {
        match self {
            Self::Field(key) => key,
            Self::OwnValue => "__value__",
        }
    }
}

/// Coordinates whose presence or value a transform writes or erases.
pub(crate) fn transformed_coordinates<'a>(
    transforms: &'a [panproto_inst::FieldTransform],
    view_fields: &'a HashMap<String, panproto_inst::value::Value>,
    recomputed_fields: &'a HashMap<String, panproto_inst::value::Value>,
) -> Vec<(TransformCoordinate<'a>, &'a panproto_inst::FieldTransform)> {
    use panproto_inst::FieldTransform;

    let mut coordinates = Vec::new();
    for transform in transforms {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => {
                coordinates.push((TransformCoordinate::Field(old_key), transform));
                coordinates.push((TransformCoordinate::Field(new_key), transform));
            }
            FieldTransform::PathTransform { path, .. } if !path.is_empty() => {
                coordinates.push((TransformCoordinate::Field(&path[0]), transform));
            }
            FieldTransform::DropField { key } => {
                coordinates.push((TransformCoordinate::Field(key), transform));
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                coordinates.extend(transformed_coordinates(
                    std::slice::from_ref(inner),
                    view_fields,
                    recomputed_fields,
                ));
            }
            FieldTransform::KeepFields { keys } => {
                // A baseline erasure can disappear on replay. Include
                // keys reintroduced by replay as well as edited insertions.
                let erased: BTreeSet<&str> = view_fields
                    .keys()
                    .chain(recomputed_fields.keys())
                    .filter(|key| !keys.contains(key))
                    .map(String::as_str)
                    .collect();
                coordinates.extend(
                    erased
                        .into_iter()
                        .map(|key| (TransformCoordinate::Field(key), transform)),
                );
            }
            _ => {
                if let Some(key) = written_key(transform) {
                    let coordinate = if transforms_node_value(transform) && key == "__value__" {
                        TransformCoordinate::OwnValue
                    } else {
                        TransformCoordinate::Field(key)
                    };
                    coordinates.push((coordinate, transform));
                }
            }
        }
    }
    coordinates
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
            // An injective vertex rename also settles an ambiguity introduced
            // by an implicit namesake edge. In that case the edge has several
            // source preimages, but the view anchor itself has exactly one
            // explicit preimage and therefore still determines the source.
            let from_vertex = reverse_anchors
                .source_anchor(view_anchor)
                .ok()
                .filter(|anchor| many.contains(anchor));
            return from_record.or(from_vertex).ok_or_else(|| {
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
    let baseline = reassemble_baseline(lens, view, complement)?;
    let mut source = baseline.clone();
    let mut traces = HashMap::new();
    let mut ids: Vec<_> = view.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        if complement.synthesized_nodes.contains(&id) {
            continue;
        }
        let trace = tree_transform_trace(lens, &baseline, id)?;
        if let Some(node) = source.nodes.get_mut(&id) {
            restore_traced_fields(
                node,
                &view.nodes[&id],
                &trace,
                complement.original_extra_fields.contains_key(&id) || complement.identity.is_some(),
            );
        }
        traces.insert(id, trace.operations);
    }
    invert_computations_on_child_scalars(view, &traces, &mut source.nodes);
    Ok(source)
}

#[derive(Default)]
pub(crate) struct TransformTrace {
    pub(crate) operations: Vec<panproto_inst::FieldTransform>,
    pub(crate) fields: HashMap<String, panproto_inst::value::Value>,
    pub(crate) value: Option<panproto_inst::FieldPresence>,
}

fn tree_transform_trace(
    lens: &Lens,
    source: &WInstance,
    id: u32,
) -> Result<TransformTrace, panproto_inst::RestrictError> {
    let Some(node) = source.nodes.get(&id) else {
        return Ok(TransformTrace::default());
    };
    let transforms = lens.compiled.value_transforms(&node.anchor);
    if transforms.is_empty() {
        return Ok(TransformTrace {
            operations: Vec::new(),
            fields: HashMap::new(),
            value: node.value.clone(),
        });
    }
    let mut node = node.clone();
    let ctx = panproto_inst::TransformContext::new(Some(&lens.src_schema), source, id, &transforms);
    let operations = panproto_inst::apply_field_transforms_traced(&mut node, &transforms, &ctx)
        .map_err(|error| panproto_inst::RestrictError::At {
            key: panproto_inst::segment::ErrorKey {
                stage: 0,
                id,
                id2: 0,
                kind: error.kind_index(),
            },
            error: Box::new(error),
        })?;
    Ok(TransformTrace {
        operations,
        fields: node.extra_fields,
        value: node.value,
    })
}

/// Inputs newly supplied by the edited view can seed a baseline when no
/// operation in the current program can write or erase them. Existing
/// snapshot inputs retain their baseline values until the pullback step.
pub(crate) fn seed_new_baseline_inputs(
    node: &mut Node,
    shown: &Node,
    transforms: &[panproto_inst::FieldTransform],
) {
    let untouched = untouched_by(transforms);
    for (key, value) in &shown.extra_fields {
        if untouched(key) {
            node.extra_fields
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }
}

/// Restore only coordinates owned by the baseline's executed operations;
/// every other coordinate, including its absence, comes from the edited view.
pub(crate) fn restore_traced_fields(
    node: &mut Node,
    shown: &Node,
    trace: &TransformTrace,
    stamped: bool,
) {
    let transforms = &trace.operations;
    if stamped {
        propagate_field_differences(node, &shown.extra_fields, transforms, Some(&trace.fields));
    } else {
        apply_inverse_transforms(node, transforms);
    }
    if !transforms.iter().any(transforms_node_value) {
        node.value.clone_from(&shown.value);
        return;
    }
    // The unchanged output keeps the original value exactly, even when an
    // advertised inverse chooses a different representative of its fiber.
    if same_canonical_presence(shown.value.as_ref(), trace.value.as_ref()) {
        return;
    }
    let mut working = shown.value.clone();
    for transform in transforms
        .iter()
        .rev()
        .filter(|transform| transforms_node_value(transform))
    {
        let panproto_inst::FieldTransform::ApplyExpr {
            inverse: Some(inverse),
            ..
        } = transform
        else {
            return;
        };
        if let Some(panproto_inst::FieldPresence::Present(value)) = &working {
            let Ok(value) = panproto_inst::value_to_expr_literal(value) else {
                return;
            };
            let env = panproto_expr::Env::new()
                .extend("v".into(), value.clone())
                .extend("__value__".into(), value);
            let Ok(value) =
                panproto_expr::eval(inverse, &env, &panproto_expr::EvalConfig::default())
            else {
                return;
            };
            working = Some(panproto_inst::FieldPresence::Present(
                panproto_inst::expr_literal_to_value(&value),
            ));
        }
    }
    node.value = working;
}

fn reassemble_baseline(
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
        // Reconstruct the baseline before binding any transform context.
        // Contexts may read children, so pullback waits until arcs and fans
        // have also been restored.
        let transforms = lens.compiled.value_transforms(&restored_node.anchor);
        if let Some(original_fields) = complement.original_extra_fields.get(&id) {
            restored_node.extra_fields.clone_from(original_fields);
            seed_new_baseline_inputs(&mut restored_node, node, &transforms);
        }
        // Restore original node.value for __value__ coercions
        if let Some(original_val) = complement
            .original_values
            .get(&id)
            .filter(|_| transforms.iter().any(transforms_node_value))
        {
            restored_node.value.clone_from(original_val);
        }
        nodes.insert(id, restored_node);
    }

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
/// The result reaches the child in two ways. A field-level `ApplyExpr`
/// whose key names a child scalar, the node having no field of that name,
/// writes its result onto that child in the forward pass: the attribute
/// lives there, and the view holds one value for it. A `ComputeField`
/// writes into `extra_fields`, where serialization lets it shadow the child
/// that supplied it; crossing the JSON boundary (serialize the view, parse
/// it back, then `put`, which is exactly what the WASM `put_json` does)
/// moves the value onto the child node, so the parent's `extra_fields`
/// comes back empty and the child holds the *computed* value. Either way
/// `propagate_view_edits_through_inverse` looks only at `extra_fields`,
/// finds nothing to invert, and the computed value would survive into the
/// reconstruction.
///
/// This pass covers that shape. For each `ComputeField` or field-level
/// `ApplyExpr` carrying an inverse whose output names a child scalar and
/// whose expression reads only that same coordinate, it evaluates the
/// inverse against the view's own child values and writes the result back
/// to that child. A computation reading a distinct child leaves that child
/// as the view shows it.
fn invert_computations_on_child_scalars(
    view: &WInstance,
    traces: &HashMap<u32, Vec<panproto_inst::FieldTransform>>,
    nodes: &mut HashMap<u32, Node>,
) {
    use panproto_inst::value::{FieldPresence, Value};

    for (&parent_id, parent) in &view.nodes {
        let Some(transforms) = traces.get(&parent_id) else {
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
            let Some((target_key, expr, inv_expr)) = invertible_field_output(transform) else {
                continue;
            };
            // Only when the computed value actually landed on the child.
            //
            // A computed field held in the parent's `extra_fields` shadows
            // a child that still holds the *source* value; the snapshot
            // path restores that correctly and inverting here as well
            // would apply the inverse twice. Its absence from
            // `extra_fields` is what says the value sits on the child,
            // which is the case this pass exists for.
            if parent.extra_fields.contains_key(target_key) || !child_of.contains_key(target_key) {
                continue;
            }
            let Some(source_key) = crate::derived::sole_source_var(expr) else {
                continue;
            };
            // A distinct source shown in the view stays authoritative, as
            // for a field source: forward replay checks the shown output
            // against it, so only a self-reading computation is inverted.
            if source_key != target_key {
                continue;
            }
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

/// The output key and forward expression of a field transform with an
/// inverse: a `ComputeField` or a field-level `ApplyExpr`.
fn invertible_field_output(
    transform: &panproto_inst::FieldTransform,
) -> Option<(&str, &panproto_expr::Expr, &panproto_expr::Expr)> {
    match transform {
        panproto_inst::FieldTransform::ComputeField {
            target_key,
            expr,
            inverse: Some(inverse),
            ..
        } => Some((target_key, expr, inverse)),
        panproto_inst::FieldTransform::ApplyExpr {
            key,
            expr,
            inverse: Some(inverse),
            ..
        } if key != "__value__" => Some((key, expr, inverse)),
        _ => None,
    }
}

/// A serialized invertible field output can return on its scalar child
/// instead of the parent's field map. Require unique target and source
/// children, matching the inverse's single source coordinate, before using
/// that presentation in the forward comparison.
pub(crate) fn inverse_child_output<'a>(
    transform: &panproto_inst::FieldTransform,
    field: &str,
    children: impl Iterator<Item = (&'a Edge, &'a Node)>,
) -> Option<&'a panproto_inst::value::Value> {
    let (target_key, expr, _) = invertible_field_output(transform)?;
    if target_key != field {
        return None;
    }
    let source_key = crate::derived::sole_source_var(expr)?;
    let mut output = None;
    let mut source_present = false;
    for (edge, child) in children {
        let name = edge.name.as_deref().unwrap_or(&edge.tgt);
        let Some(panproto_inst::value::FieldPresence::Present(value)) = &child.value else {
            continue;
        };
        if name == target_key && output.replace(value).is_some() {
            return None;
        }
        if name == source_key && std::mem::replace(&mut source_present, true) {
            return None;
        }
    }
    source_present.then_some(output).flatten()
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
    propagate_field_differences(node, view_fields, transforms, None);
}

/// Pull back changed output coordinates while retaining every unchanged
/// coordinate's exact baseline source, even when an inverse chooses another
/// representative with the same forward image.
fn propagate_field_differences(
    node: &mut Node,
    view_fields: &HashMap<String, panproto_inst::value::Value>,
    transforms: &[panproto_inst::FieldTransform],
    baseline_fields: Option<&HashMap<String, panproto_inst::value::Value>>,
) {
    use panproto_inst::FieldTransform;

    let mut changed = baseline_fields.map(|baseline| {
        baseline
            .keys()
            .chain(view_fields.keys())
            .filter(|key| match (baseline.get(*key), view_fields.get(*key)) {
                (Some(before), Some(after)) => !before.canonical_cmp(after).is_eq(),
                (None, None) => false,
                _ => true,
            })
            .cloned()
            .collect::<HashSet<_>>()
    });

    // A field no current transform writes, removes or rewrites is an
    // independent coordinate. Its presence and value come from the view,
    // including after a relabel makes an earlier transform inapplicable.
    let untouched = untouched_by(transforms);
    node.extra_fields
        .retain(|key, _| !untouched(key) || view_fields.contains_key(key));
    for (key, value) in view_fields {
        if untouched(key) {
            node.extra_fields.insert(key.clone(), value.clone());
        }
    }

    // Mutable working copy so ApplyExpr lookups see prior rewrites.
    let mut working: std::collections::HashMap<String, panproto_inst::value::Value> =
        view_fields.clone();

    for transform in transforms.iter().rev() {
        match transform {
            FieldTransform::RenameField { old_key, new_key }
                if changed.as_ref().is_none_or(|keys| keys.contains(new_key)) =>
            {
                if let Some(val) = working.remove(new_key) {
                    node.extra_fields.insert(old_key.clone(), val.clone());
                    working.insert(old_key.clone(), val);
                    if let Some(keys) = &mut changed {
                        keys.remove(new_key);
                        keys.insert(old_key.clone());
                    }
                }
            }
            FieldTransform::ApplyExpr {
                key,
                inverse: Some(inv_expr),
                ..
            } if changed.as_ref().is_none_or(|keys| keys.contains(key)) => {
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
            } if changed
                .as_ref()
                .is_none_or(|keys| keys.contains(target_key)) =>
            {
                pull_computed_field(
                    node,
                    &mut working,
                    &mut changed,
                    target_key,
                    expr,
                    inverse.as_ref(),
                );
            }
            // For an added key the snapshot remains authoritative: the key
            // is present exactly when the source already held it.
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

fn pull_computed_field(
    node: &mut Node,
    working: &mut HashMap<String, panproto_inst::value::Value>,
    changed: &mut Option<HashSet<String>>,
    target_key: &str,
    expr: &panproto_expr::Expr,
    inverse: Option<&panproto_expr::Expr>,
) {
    // The inverse recovers the *source* coordinate the forward
    // expression read. The snapshot remains authoritative for
    // every other source coordinate, including an existing
    // coordinate overwritten by the computed key.
    // While a distinct source coordinate is still in the view the
    // computed key is derived from it (`collect_derived_fields`):
    // the view's own value of the source is authoritative and
    // the inverse has no say. A computation that rewrites its own
    // source in place leaves no other view coordinate for it, so
    // the inverse recovers the source from the shown value.
    if let Some(inv_expr) = inverse
        && working.contains_key(target_key)
        && let Some(source_key) = crate::derived::sole_source_var(expr)
        && (!working.contains_key(&source_key) || source_key == target_key)
        // Only an `extra_fields` source can be written here. A
        // child scalar lives on another node, and inserting it
        // on this one would invent a field the source lacked;
        // `collect_derived_fields` classifies that case derived
        // so the snapshot stays authoritative for it.
        && node.extra_fields.contains_key(&source_key)
    {
        let env = panproto_inst::build_env_from_extra_fields(working);
        let config = panproto_expr::EvalConfig::default();
        if let Ok(result) = panproto_expr::eval(inv_expr, &env, &config) {
            let inverted = panproto_inst::expr_literal_to_value(&result);
            node.extra_fields
                .insert(source_key.clone(), inverted.clone());
            working.insert(source_key.clone(), inverted);
            if let Some(keys) = changed {
                keys.remove(target_key);
                keys.insert(source_key);
            }
        }
    }
}

/// Whether a transform can change the node's own value, rather than a field.
pub(crate) fn transforms_node_value(transform: &panproto_inst::FieldTransform) -> bool {
    use panproto_inst::FieldTransform;

    match transform {
        FieldTransform::ApplyExpr { key, .. } => key == "__value__",
        FieldTransform::PathTransform { path, inner } => {
            path.is_empty() && transforms_node_value(inner)
        }
        FieldTransform::Case { branches } => branches
            .iter()
            .flat_map(|branch| &branch.transforms)
            .any(transforms_node_value),
        _ => false,
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
            FieldTransform::ApplyExpr { key: k, .. } if k == "__value__" => false,
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
/// function of the view alone. This succeeds for the statically classified
/// lenses whose every `get` leaves trivial residue
/// ([`Complement::residue_is_trivial`]) and whose vertex and edge maps are
/// onto the target. Thus every source coordinate survives uniquely, every
/// target coordinate has a source preimage, no vertex is filtered by a
/// predicate, and the migration has no value-transform program. It refuses
/// everything else.
///
/// A structural source-side refusal witnesses non-terminal residue, while a
/// target-only coordinate witnesses failure of surjectivity. The
/// transform-free boundary is conservative: refusal there does not prove
/// that no inverse lens exists, only that this classifier lacks an executable
/// proof for the transform. Thus [`LensError::NotAnIsomorphism`] is a refusal
/// from this certified fragment, not a global non-isomorphism theorem.
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
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;

    // The bookkeeping half of the complement, read off the view. Sound
    // only because the residue is known empty: every node and arc of the
    // source is present in the view, so the view's own shape is the
    // source's shape.
    let mut original_parent = HashMap::new();
    let mut arc_edges = HashMap::new();
    let mut arc_order = Vec::with_capacity(view.arcs.len());
    let reverse_remap = build_reverse_remap(&lens.compiled.vertex_remap);
    // Read the inverse from the compiled edge fates, not only from the
    // explicit edge-remap table. A unique endpoint resolver is also a direct
    // kept edge image, and the classifier has already proved this map
    // bijective.
    let reverse_edges: HashMap<&Edge, &Edge> = plan
        .edge_image
        .fates
        .iter()
        .filter_map(|(source, fate)| match fate {
            EdgeFate::Keep(target) => Some((target, source)),
            EdgeFate::Expand { .. } | EdgeFate::Drop | EdgeFate::Unresolved(_) => None,
        })
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

/// The first reason `lens` is outside the classified bijective, complement-
/// `Unit` tree fragment, or `None` when it belongs to that fragment.
///
/// Survival is read as `get` reads it: a source vertex survives when its
/// image under the vertex remap is a surviving target vertex, so an
/// injective rename is not a drop. Both the vertex and edge maps must also
/// be onto the target; otherwise `get` is not surjective even if its source
/// residue is trivial. Source and target hyper-edge declarations are excluded
/// because a valid zero-child source fan is currently residue. The certified
/// fragment also admits no value transforms or op-to-term assignments. In
/// particular, scalar and child-scalar `ApplyExpr` transforms are refused
/// rather than trusting a declared coercion class or an unchecked expression
/// pair. Neither schema may impose a required-cell or key-uniqueness
/// obligation, because tree get does not validate its input while tree put
/// validates the restored source.
pub(crate) fn obstruction_to_tree_isomorphism(lens: &Lens) -> Option<String> {
    let compiled = &lens.compiled;
    let mut vertices: Vec<&Name> = lens.src_schema.vertices.keys().collect();
    vertices.sort();
    for v in &vertices {
        let image = compiled.vertex_remap.get(*v).unwrap_or(v);
        if !vertex_survives(lens, v) || !lens.tgt_schema.vertices.contains_key(image) {
            return Some(format!(
                "vertex `{v}` has no surviving target vertex, so its data is set aside in the complement"
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
    let mut target_vertices = lens.tgt_schema.vertices.keys().collect::<Vec<_>>();
    target_vertices.sort();
    if let Some(target) = target_vertices
        .into_iter()
        .find(|target| !images.keys().any(|image| *image == *target))
    {
        return Some(format!(
            "target vertex `{target}` has no source preimage, so the vertex map is not bijective"
        ));
    }
    let plan = match RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, compiled) {
        Ok(plan) => plan,
        Err(error) => {
            return Some(format!(
                "the migration is not executable as a tree lens: {error}"
            ));
        }
    };
    for (source, fate) in &plan.edge_image.fates {
        let EdgeFate::Keep(image) = fate else {
            return Some(format!(
                "edge `{:?}` is not kept as one tree edge, so it cannot be reconstructed without a complement",
                source.name
            ));
        };
        if plan.edge_image.preimages(image).len() != 1 {
            return Some(format!(
                "more than one source edge maps to `{:?}`, so a view arc does not determine its source label",
                image.name
            ));
        }
    }
    let mut target_edges = lens.tgt_schema.edges.keys().collect::<Vec<_>>();
    target_edges.sort();
    if let Some(target) = target_edges
        .into_iter()
        .find(|target| plan.edge_image.preimages(target).is_empty())
    {
        return Some(format!(
            "target edge `{:?}` has no source preimage, so the edge map is not bijective",
            target.name
        ));
    }
    if let Some(source) = lens.src_schema.hyper_edges.keys().min() {
        return Some(format!(
            "source hyper-edge `{source}` admits a valid zero-child fan that tree get records as residue"
        ));
    }
    if let Some(target) = lens.tgt_schema.hyper_edges.keys().min() {
        return Some(format!(
            "target hyper-edge `{target}` has no admitted source preimage, so tree get is not surjective"
        ));
    }
    if !compiled.hyper_resolver.is_empty() {
        return Some(
            "a hyper-edge resolver retargets a fan or its child labels, which the tree view does \
             not determine without residue"
                .to_owned(),
        );
    }
    if let Some(detail) = tree_validation_obstruction(&lens.src_schema, "source") {
        return Some(detail);
    }
    if let Some(detail) = tree_validation_obstruction(&lens.tgt_schema, "target") {
        return Some(detail);
    }
    tree_transform_program_obstruction(lens)
}

fn tree_validation_obstruction(schema: &Schema, side: &str) -> Option<String> {
    let mut required = schema
        .required
        .iter()
        .filter(|(vertex, _)| schema.vertices.contains_key(*vertex))
        .flat_map(|(vertex, edges)| edges.iter().map(move |edge| (vertex, edge)))
        .filter(|(_, edge)| !schema.defaults.contains_key(&edge.tgt))
        .collect::<Vec<_>>();
    required.sort();
    if let Some((vertex, edge)) = required.first() {
        let field = edge
            .name
            .as_ref()
            .map_or_else(|| edge.tgt.to_string(), Name::to_string);
        return Some(format!(
            "{side} vertex `{vertex}` requires field `{field}`, so the tree carrier is not the unconstrained bijective fragment"
        ));
    }

    let mut keys = schema
        .declarations
        .keys
        .iter()
        .filter(|(vertex, _)| schema.vertices.contains_key(*vertex))
        .flat_map(|(vertex, keys)| keys.iter().map(move |key| (vertex, &key.id)))
        .collect::<Vec<_>>();
    keys.sort();
    keys.first().map(|(vertex, key)| {
        format!(
            "{side} vertex `{vertex}` declares key `{key}`, so the tree carrier is not the unconstrained bijective fragment"
        )
    })
}

fn tree_transform_program_obstruction(lens: &Lens) -> Option<String> {
    let mut field_anchors = lens.compiled.field_transforms.keys().collect::<Vec<_>>();
    field_anchors.sort();
    if let Some(anchor) = field_anchors
        .into_iter()
        .find(|anchor| !lens.compiled.field_transforms[*anchor].is_empty())
    {
        return Some(format!(
            "vertex `{anchor}` has value transforms outside the certified transform-free tree fragment"
        ));
    }
    let mut term_anchors = lens.compiled.op_term_assignments.keys().collect::<Vec<_>>();
    term_anchors.sort();
    term_anchors
        .into_iter()
        .find(|anchor| !lens.compiled.op_term_assignments[*anchor].is_empty())
        .map(|anchor| {
            format!(
                "vertex `{anchor}` has op-to-term assignments outside the certified transform-free tree fragment"
            )
        })
}

/// The `extra_fields` key a transform writes, when it writes one.
const fn transform_target_key(transform: &panproto_inst::FieldTransform) -> Option<&str> {
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
    fn collision_residue_reindexes_surviving_expansions_for_all_collisions() {
        let mut complement = Complement::empty();
        complement.arc_order = vec![(0, 2), (0, 4), (3, 5), (4, 6)];
        complement.synthesized_nodes = [2, 3, 10, 11, 12, 13].into();
        complement.synthesized_keys = BTreeMap::from([
            (
                10,
                SynthKey::Expand {
                    source_edge: EdgeId(0),
                    hop: 0,
                },
            ),
            (
                11,
                SynthKey::Expand {
                    source_edge: EdgeId(1),
                    hop: 0,
                },
            ),
            (
                12,
                SynthKey::Expand {
                    source_edge: EdgeId(3),
                    hop: 0,
                },
            ),
            (
                13,
                SynthKey::Expand {
                    source_edge: EdgeId(2),
                    hop: 0,
                },
            ),
        ]);

        let collisions = BTreeSet::from([2, 3]);
        remove_reserved_node_residue(&mut complement, &collisions);
        assert_eq!(
            complement.synthesized_nodes,
            HashSet::from([10, 11, 12, 13])
        );
        assert_eq!(
            complement.synthesized_keys,
            BTreeMap::from([
                (
                    11,
                    SynthKey::Expand {
                        source_edge: EdgeId(0),
                        hop: 0,
                    },
                ),
                (
                    12,
                    SynthKey::Expand {
                        source_edge: EdgeId(1),
                        hop: 0,
                    },
                ),
            ])
        );
        assert_eq!(complement.arc_order, vec![(0, 4), (4, 6)]);
    }

    /// The native tree put-domain checker must expose every validation
    /// obligation that the graph presentation exposes. These focused cases
    /// protect the common-domain premise used by the graph/tree square; the
    /// independent reference certificate lives in `reference_tree_validation`.
    mod tree_validation_parity {
        use super::*;
        use panproto_inst::value::{FieldPresence, Value};
        use panproto_inst::{GInstance, PutDomainError};
        use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, SchemaBuilder};

        fn child_edge() -> Edge {
            Edge {
                src: "root".into(),
                tgt: "item".into(),
                kind: "prop".into(),
                name: Some("item".into()),
            }
        }

        fn base_schema() -> panproto_schema::Schema {
            let protocol = Protocol::default();
            SchemaBuilder::new(&protocol)
                .vertex("root", "object", None)
                .unwrap()
                .vertex("item", "string", None)
                .unwrap()
                .edge("root", "item", "prop", Some("item"))
                .unwrap()
                .build()
                .unwrap()
        }

        fn tree(
            nodes: impl IntoIterator<Item = (u32, Node)>,
            arcs: Vec<(u32, u32, Edge)>,
        ) -> WInstance {
            WInstance::new(
                nodes.into_iter().collect(),
                arcs,
                Vec::new(),
                0,
                "root".into(),
            )
        }

        fn graph_and_tree_reports(
            lens: &Lens,
            source: &WInstance,
        ) -> (Vec<PutDomainError>, Vec<PutDomainError>) {
            let graph = GInstance::from_wtype_with_schema(source, &lens.src_schema)
                .expect("the validation witness has a graph presentation");
            let (tree_view, tree_complement) = get(lens, source).expect("tree get");
            let (graph_view, graph_complement) =
                crate::graph_lens::get(lens, &graph).expect("graph get");
            let tree_report = check_put_domain(lens, &tree_view, &tree_complement)
                .expect_err("the tree view violates its source validation declaration");
            let graph_report =
                crate::graph_lens::check_put_domain(lens, &graph_view, &graph_complement)
                    .expect_err("the graph view violates its source validation declaration");
            (tree_report, graph_report)
        }

        #[test]
        fn a_plan_compilation_failure_is_outside_the_tree_put_domain() {
            let schema = three_node_schema();
            let mut lens = identity_lens(&schema);
            let source_edge = schema
                .edges
                .keys()
                .next()
                .cloned()
                .expect("the fixture has an edge");
            let mut incoherent_image = source_edge.clone();
            incoherent_image.tgt = "missing-target".into();
            lens.compiled
                .edge_remap
                .insert(source_edge, incoherent_image);

            let view = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 0, "a".into());
            let mut complement = Complement::empty();
            complement.stamp(lens.identity(ComplementShape::WType), &lens.src_schema);

            let report = check_put_domain(&lens, &view, &complement)
                .expect_err("an unexecutable lens has no admitted tree view");
            assert!(matches!(
                report.as_slice(),
                [PutDomainError::PlanCompilation { detail }]
                    if detail.contains("incoherent") || detail.contains("missing-target")
            ));
            assert!(matches!(
                put(&lens, &view, &complement),
                Err(LensError::PutDomain(error))
                    if matches!(*error, PutDomainError::PlanCompilation { .. })
            ));
        }

        #[test]
        fn required_cell_refusal_matches_the_graph_route_exactly() {
            let mut schema = base_schema();
            schema.required.insert("root".into(), vec![child_edge()]);
            let lens = identity_lens(&schema);
            let source = tree([(0, Node::new(0, "root"))], Vec::new());

            let (tree_report, graph_report) = graph_and_tree_reports(&lens, &source);
            assert_eq!(tree_report, graph_report);
            assert!(matches!(
                tree_report.as_slice(),
                [PutDomainError::MissingRequiredCell {
                    elem: ViewRef::Node(0),
                    field,
                }] if field == "item"
            ));
        }

        #[test]
        fn duplicate_key_refusal_matches_the_graph_route_exactly() {
            let mut schema = base_schema();
            schema.declarations.keys.insert(
                "item".into(),
                vec![Key {
                    id: "item-value".into(),
                    parts: vec![KeyPart::Path(Vec::new())],
                    role: KeyRole::Unique,
                    scope: KeyScope::Global,
                    equality: vec![],
                }],
            );
            let lens = identity_lens(&schema);
            let item = |id| {
                Node::new(id, "item")
                    .with_value(FieldPresence::Present(Value::Str("same".to_owned())))
            };
            let source = tree(
                [(0, Node::new(0, "root")), (1, item(1)), (2, item(2))],
                vec![(0, 1, child_edge()), (0, 2, child_edge())],
            );

            let (tree_report, graph_report) = graph_and_tree_reports(&lens, &source);
            assert_eq!(tree_report, graph_report);
            assert!(matches!(
                tree_report.as_slice(),
                [PutDomainError::DuplicateKey { table, elems, .. }]
                    if table.as_str() == "item"
                        && elems == &[ViewRef::Node(1), ViewRef::Node(2)]
            ));
        }

        #[test]
        fn a_missing_native_fan_child_has_its_tree_coordinate() {
            let relation = Edge {
                src: "root".into(),
                tgt: "item".into(),
                kind: "rel".into(),
                name: Some("item".into()),
            };
            let protocol = Protocol::default();
            let schema = SchemaBuilder::new(&protocol)
                .vertex("root", "object", None)
                .unwrap()
                .vertex("item", "object", None)
                .unwrap()
                .edge("root", "item", "rel", Some("item"))
                .unwrap()
                .hyper_edge(
                    "pair",
                    "rel",
                    HashMap::from([
                        ("parent".to_owned(), "root".to_owned()),
                        ("member".to_owned(), "item".to_owned()),
                    ]),
                    "parent",
                )
                .unwrap()
                .build()
                .unwrap();
            let lens = identity_lens(&schema);
            let source = WInstance::new(
                HashMap::from([(0, Node::new(0, "root")), (1, Node::new(1, "item"))]),
                vec![(0, 1, relation)],
                vec![Fan::new("pair", 0).with_child("member", 1)],
                0,
                "root".into(),
            );
            let (mut view, complement) = get(&lens, &source).unwrap();
            view.fans[0].children.insert("member".to_owned(), 99);

            assert_eq!(
                check_put_domain(&lens, &view, &complement),
                Err(vec![PutDomainError::MalformedFan {
                    node: 0,
                    reason: FanViolation::MissingChild {
                        label: "member".into(),
                    },
                }])
            );
        }
    }

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
                edge_map: HashMap::from([(prop("r", "x", "x"), prop("r", "x", "x"))]),
                hyper_edge_map: HashMap::new(),
                label_map: HashMap::new(),
                resolver: HashMap::from([(
                    (Name::from("r"), Name::from("y")),
                    prop("r", "y", "c"),
                )]),
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

        fn two_contracted_siblings() -> WInstance {
            WInstance::new(
                [
                    (0, Node::new(0, "r")),
                    (2, Node::new(2, "m")),
                    (3, Node::new(3, "y")),
                    (4, Node::new(4, "m")),
                    (5, Node::new(5, "y")),
                ]
                .into_iter()
                .collect(),
                vec![
                    (0, 2, prop("r", "m", "m")),
                    (2, 3, prop("m", "y", "y")),
                    (0, 4, prop("r", "m", "m")),
                    (4, 5, prop("m", "y", "y")),
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
                view.schema_root,
            );
            if let Ok(restored) = put(&lens, &moved, &complement) {
                let (again, _) = get(&lens, &restored).unwrap();
                println!(
                    "admitted: the view orders {:?}, get after put orders {:?}",
                    moved.arcs, again.arcs
                );
            }
            let refused = check_put_domain(&lens, &moved, &complement).unwrap_err();
            assert_eq!(
                refused,
                vec![PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Arc {
                        parent: 0,
                        child: 3,
                    },
                }]
            );
        }

        #[test]
        fn synthesized_arcs_from_different_parents_may_interleave() {
            let left = prop("r", "x", "x");
            let right = prop("m", "y", "y");
            let view = WInstance::new(
                [
                    (0, Node::new(0, "r")),
                    (1, Node::new(1, "x")),
                    (2, Node::new(2, "m")),
                    (3, Node::new(3, "y")),
                ]
                .into_iter()
                .collect(),
                vec![
                    (2, 3, right.clone()),
                    (0, 2, prop("r", "m", "m")),
                    (0, 1, left.clone()),
                ],
                Vec::new(),
                0,
                "r".into(),
            );
            let mut complement = Complement::empty();
            complement.contraction_choices = HashMap::from([((0, 1), left), ((2, 3), right)]);

            assert_eq!(synthesized_sibling_order(&view, &complement), Vec::new());
        }

        #[test]
        fn same_parent_synthesized_arcs_follow_key_order() {
            let lens = lens();
            let (mut view, complement) = get(&lens, &two_contracted_siblings()).unwrap();
            assert_eq!(
                view.arcs,
                vec![(0, 3, prop("r", "y", "c")), (0, 5, prop("r", "y", "c")),]
            );
            view.arcs.reverse();

            assert_eq!(
                check_put_domain(&lens, &view, &complement),
                Err(vec![PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Arc {
                        parent: 0,
                        child: 3,
                    },
                }])
            );
        }

        #[test]
        fn removing_a_chain_endpoint_explains_its_synthesized_arc() {
            let lens = lens();
            let (view, complement) = get(&lens, &source()).unwrap();
            let mut edited = view;
            edited.nodes.remove(&3);
            edited.arcs.retain(|(_, child, _)| *child != 3);

            let restored = put(&lens, &edited, &complement)
                .expect("deleting a source endpoint also removes its presentation-only chain");
            assert!(!restored.nodes.contains_key(&3));
            let (again, _) = get(&lens, &restored).unwrap();
            assert_eq!(again.nodes.len(), edited.nodes.len());
            assert!(again.nodes.iter().all(|(id, node)| {
                edited
                    .nodes
                    .get(id)
                    .is_some_and(|other| node_equiv(node, other))
            }));
            assert_eq!(again.arcs, edited.arcs);
            assert_eq!(again.fans, edited.fans);
            assert_eq!(again.root, edited.root);
            assert_eq!(again.schema_root, edited.schema_root);
        }
    }

    #[test]
    fn implicit_kept_edges_participate_in_reverse_anchor_recovery() {
        let protocol = panproto_schema::Protocol::default();
        let source_schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("A", "obj", None)
            .unwrap()
            .vertex("B1", "obj", None)
            .unwrap()
            .vertex("B2", "obj", None)
            .unwrap()
            .edge("A", "B1", "prop", Some("x"))
            .unwrap()
            .edge("A", "B2", "prop", Some("y"))
            .unwrap()
            .build()
            .unwrap();
        let target_schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("A", "obj", None)
            .unwrap()
            .vertex("B", "obj", None)
            .unwrap()
            .edge("A", "B", "prop", Some("x"))
            .unwrap()
            .edge("A", "B", "prop", Some("y"))
            .unwrap()
            .build()
            .unwrap();
        let lens = Lens {
            compiled: panproto_inst::CompiledMigration {
                surviving_verts: target_schema.vertices.keys().cloned().collect(),
                surviving_edges: target_schema.edges.keys().cloned().collect(),
                vertex_remap: [
                    (Name::from("B1"), Name::from("B")),
                    (Name::from("B2"), Name::from("B")),
                ]
                .into(),
                ..panproto_inst::CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let source_edge = lens
            .src_schema
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some("x"))
            .cloned()
            .unwrap();
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "A")), (1, Node::new(1, "B1"))]),
            vec![(0, 1, source_edge)],
            Vec::new(),
            0,
            Name::from("A"),
        );

        let (view, complement) = get(&lens, &source).unwrap();
        assert_eq!(complement.original_anchors.get(&1), Some(&Name::from("B1")));
        let restored = put(&lens, &view, &complement)
            .expect("the implicit kept edge identifies the recorded source anchor");
        assert_eq!(restored.nodes.len(), source.nodes.len());
        assert!(restored.nodes.iter().all(|(id, node)| {
            source
                .nodes
                .get(id)
                .is_some_and(|other| node_equiv(node, other))
        }));
        assert_eq!(restored.arcs, source.arcs);
        assert_eq!(restored.fans, source.fans);
        assert_eq!(restored.root, source.root);
        assert_eq!(restored.schema_root, source.schema_root);
    }

    mod transformed_payload {
        use super::*;
        use panproto_expr::{Expr, Literal};
        use panproto_inst::value::{FieldPresence, Value};
        use panproto_inst::{FieldTransform, TermAssignment};
        use panproto_schema::{Protocol, SchemaBuilder};

        fn lens(transform: FieldTransform, terms: bool) -> Lens {
            let schema = SchemaBuilder::new(&Protocol::default())
                .vertex("a", "object", None)
                .unwrap()
                .vertex("b", "object", None)
                .unwrap()
                .vertex("value", "integer", None)
                .unwrap()
                .edge("a", "value", "prop", Some("__value__"))
                .unwrap()
                .edge("a", "value", "prop", Some("f"))
                .unwrap()
                .edge("a", "value", "prop", Some("g"))
                .unwrap()
                .edge("b", "value", "prop", Some("g"))
                .unwrap()
                .edge("b", "value", "prop", Some("f"))
                .unwrap()
                .build()
                .unwrap();
            let mut lens = identity_lens(&schema);
            if terms {
                lens.compiled.op_term_assignments.insert(
                    "a".into(),
                    vec![TermAssignment::from_field_transform(&transform)],
                );
            } else {
                lens.compiled
                    .field_transforms
                    .insert("a".into(), vec![transform]);
            }
            lens
        }

        fn default_field() -> FieldTransform {
            FieldTransform::AddField {
                key: "f".to_owned(),
                value: Value::Int(0),
            }
        }

        fn own_value() -> FieldTransform {
            FieldTransform::ApplyExpr {
                key: "__value__".to_owned(),
                expr: Expr::Lit(Literal::Int(0)),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            }
        }

        fn tree(node: Node) -> WInstance {
            WInstance::new(
                HashMap::from([(0, node)]),
                Vec::new(),
                Vec::new(),
                0,
                "a".into(),
            )
        }

        fn assert_roundtrip(lens: &Lens, source: &Node, edit: impl Fn(&mut Node)) {
            let (mut view, complement) = get(lens, &tree(source.clone())).unwrap();
            edit(view.nodes.get_mut(&0).unwrap());
            assert_eq!(check_put_domain(lens, &view, &complement), Ok(()));
            let restored = put(lens, &view, &complement).unwrap();
            let (again, next) = get(lens, &restored).unwrap();
            assert!(node_equiv(&again.nodes[&0], &view.nodes[&0]));
            let twice = put(lens, &view, &next).unwrap();
            assert!(node_equiv(&twice.nodes[&0], &restored.nodes[&0]));

            let source = GInstance::new().with_node(source.clone());
            let (mut view, complement) = crate::graph_lens::get(lens, &source).unwrap();
            edit(view.node_mut(0).unwrap());
            assert_eq!(
                crate::graph_lens::check_put_domain(lens, &view, &complement),
                Ok(())
            );
            let restored = crate::graph_lens::put(lens, &view, &complement).unwrap();
            let (again, next) = crate::graph_lens::get(lens, &restored).unwrap();
            assert!(node_equiv(again.node(0).unwrap(), view.node(0).unwrap()));
            let twice = crate::graph_lens::put(lens, &view, &next).unwrap();
            assert!(node_equiv(
                twice.node(0).unwrap(),
                restored.node(0).unwrap()
            ));
        }

        fn assert_refusal(
            lens: &Lens,
            source: &Node,
            edit: impl Fn(&mut Node),
            expected: &PutDomainError,
        ) {
            let (mut view, complement) = get(lens, &tree(source.clone())).unwrap();
            assert_eq!(check_put_domain(lens, &view, &complement), Ok(()));
            edit(view.nodes.get_mut(&0).unwrap());
            assert_eq!(
                check_put_domain(lens, &view, &complement),
                Err(vec![expected.clone()])
            );
            let Err(LensError::PutDomain(error)) = put(lens, &view, &complement) else {
                panic!("tree put must return the domain refusal");
            };
            assert_eq!(error.as_ref(), expected);

            let source = GInstance::new().with_node(source.clone());
            let (mut view, complement) = crate::graph_lens::get(lens, &source).unwrap();
            assert_eq!(
                crate::graph_lens::check_put_domain(lens, &view, &complement),
                Ok(())
            );
            edit(view.node_mut(0).unwrap());
            assert_eq!(
                crate::graph_lens::check_put_domain(lens, &view, &complement),
                Err(vec![expected.clone()])
            );
            let Err(LensError::PutDomain(error)) = crate::graph_lens::put(lens, &view, &complement)
            else {
                panic!("graph put must return the domain refusal");
            };
            assert_eq!(error.as_ref(), expected);
        }

        fn sequence(transforms: Vec<FieldTransform>, terms: bool) -> Lens {
            let mut result = lens(default_field(), false);
            result.compiled.field_transforms.clear();
            if terms {
                result.compiled.op_term_assignments.insert(
                    "a".into(),
                    transforms
                        .iter()
                        .map(TermAssignment::from_field_transform)
                        .collect(),
                );
            } else {
                result
                    .compiled
                    .field_transforms
                    .insert("a".into(), transforms);
            }
            result
        }

        fn assert_restored_payload(
            lens: &Lens,
            source: &Node,
            edit: impl Fn(&mut Node),
            expected: &Node,
        ) {
            assert_roundtrip(lens, source, &edit);
            let (mut view, complement) = get(lens, &tree(source.clone())).unwrap();
            edit(view.nodes.get_mut(&0).unwrap());
            let restored = put(lens, &view, &complement).unwrap();
            assert!(node_equiv(&restored.nodes[&0], expected));

            let source = GInstance::new().with_node(source.clone());
            let (mut view, complement) = crate::graph_lens::get(lens, &source).unwrap();
            edit(view.node_mut(0).unwrap());
            let restored = crate::graph_lens::put(lens, &view, &complement).unwrap();
            assert!(node_equiv(restored.node(0).unwrap(), expected));
        }

        fn negate_field(key: &str) -> FieldTransform {
            let expr = Expr::Builtin(panproto_expr::BuiltinOp::Neg, vec![Expr::Var(key.into())]);
            FieldTransform::ApplyExpr {
                key: key.to_owned(),
                expr: expr.clone(),
                inverse: Some(expr),
                coercion_class: panproto_gat::CoercionClass::Iso,
            }
        }

        #[test]
        fn unchanged_inverse_outputs_keep_their_exact_source_fields() {
            for terms in [false, true] {
                let lens = sequence(
                    vec![
                        negate_field("f"),
                        FieldTransform::ApplyExpr {
                            key: "g".to_owned(),
                            expr: Expr::Builtin(
                                panproto_expr::BuiltinOp::Upper,
                                vec![Expr::Var("g".into())],
                            ),
                            inverse: Some(Expr::Builtin(
                                panproto_expr::BuiltinOp::Lower,
                                vec![Expr::Var("g".into())],
                            )),
                            coercion_class: panproto_gat::CoercionClass::Iso,
                        },
                    ],
                    terms,
                );
                let mut source = Node::new(0, "a");
                source.extra_fields.insert("f".to_owned(), Value::Int(3));
                source
                    .extra_fields
                    .insert("g".to_owned(), Value::Str("UPPER".to_owned()));
                for change_f in [false, true] {
                    let mut expected = source.clone();
                    expected
                        .extra_fields
                        .insert("other".to_owned(), Value::Int(1));
                    if change_f {
                        expected.extra_fields.insert("f".to_owned(), Value::Int(4));
                    }
                    assert_restored_payload(
                        &lens,
                        &source,
                        |node| {
                            node.extra_fields.insert("other".to_owned(), Value::Int(1));
                            if change_f {
                                node.extra_fields.insert("f".to_owned(), Value::Int(-4));
                            }
                        },
                        &expected,
                    );
                }
            }
        }

        #[test]
        fn changed_rename_outputs_reach_their_upstream_inverse() {
            for terms in [false, true] {
                let lens = sequence(
                    vec![
                        negate_field("f"),
                        FieldTransform::RenameField {
                            old_key: "f".to_owned(),
                            new_key: "g".to_owned(),
                        },
                    ],
                    terms,
                );
                let mut source = Node::new(0, "a");
                source.extra_fields.insert("f".to_owned(), Value::Int(3));
                source.extra_fields.insert("g".to_owned(), Value::Int(9));
                let mut expected = source.clone();
                expected.extra_fields.insert("f".to_owned(), Value::Int(4));
                assert_restored_payload(
                    &lens,
                    &source,
                    |node| {
                        node.extra_fields.insert("g".to_owned(), Value::Int(-4));
                    },
                    &expected,
                );
            }
        }

        #[test]
        fn changed_overwriting_computations_use_their_inverse() {
            let expr = Expr::Builtin(panproto_expr::BuiltinOp::Neg, vec![Expr::Var("f".into())]);
            for terms in [false, true] {
                let lens = lens(
                    FieldTransform::ComputeField {
                        target_key: "f".to_owned(),
                        expr: expr.clone(),
                        inverse: Some(expr.clone()),
                        coercion_class: panproto_gat::CoercionClass::Iso,
                    },
                    terms,
                );
                let mut source = Node::new(0, "a");
                source.extra_fields.insert("f".to_owned(), Value::Int(3));
                let mut expected = source.clone();
                expected.extra_fields.insert("f".to_owned(), Value::Int(4));
                assert_restored_payload(
                    &lens,
                    &source,
                    |node| {
                        node.extra_fields.insert("f".to_owned(), Value::Int(-4));
                    },
                    &expected,
                );
            }
        }

        fn child_source(lens: &Lens) -> WInstance {
            let edge = lens
                .src_schema
                .edges
                .keys()
                .find(|edge| edge.src == "a" && edge.name.as_deref() == Some("f"))
                .unwrap()
                .clone();
            WInstance::new(
                HashMap::from([
                    (0, Node::new(0, "a")),
                    (
                        1,
                        Node::new(1, "value").with_value(FieldPresence::Present(Value::Int(3))),
                    ),
                ]),
                vec![(0, 1, edge)],
                Vec::new(),
                0,
                "a".into(),
            )
        }

        /// An empty-path wrapper evaluates its inner transform on the node's
        /// own fields and binds no children, so a computation reading a child
        /// scalar is refused by get in both shapes rather than inverted.
        fn assert_empty_path_binds_no_child(
            transform: &FieldTransform,
            terms: bool,
            source: fn(&Lens) -> WInstance,
            child: &str,
        ) {
            let lens = lens(
                FieldTransform::PathTransform {
                    path: Vec::new(),
                    inner: Box::new(transform.clone()),
                },
                terms,
            );
            let unbound = |error: &LensError| {
                let LensError::Restrict(error) = error else {
                    return false;
                };
                // The graph engine locates the failure at its node.
                let error = match error {
                    panproto_inst::RestrictError::At { error, .. } => error.as_ref(),
                    other => other,
                };
                matches!(
                    error,
                    panproto_inst::RestrictError::FieldTransformFailed {
                        key,
                        source: panproto_expr::ExprError::UnboundVariable(name),
                    } if key == "f" && name == child
                )
            };
            let tree = source(&lens);
            assert!(get(&lens, &tree).as_ref().is_err_and(unbound));
            let graph = GInstance::from_wtype(&tree).unwrap();
            let outcome = crate::graph_lens::get(&lens, &graph);
            assert!(
                outcome.as_ref().is_err_and(unbound),
                "graph get: {:?}",
                outcome.map(|(view, _)| view)
            );
        }

        fn assert_serialized_child_inverse(lens: &Lens) {
            let source = child_source(lens);
            let (mut view, complement) = get(lens, &source).unwrap();
            let output = view
                .nodes
                .get_mut(&0)
                .unwrap()
                .extra_fields
                .remove("f")
                .unwrap();
            view.nodes.get_mut(&1).unwrap().value = Some(FieldPresence::Present(output.clone()));
            assert_eq!(check_put_domain(lens, &view, &complement), Ok(()));
            let restored = put(lens, &view, &complement).unwrap();
            assert!(node_equiv(&restored.nodes[&0], &source.nodes[&0]));
            assert!(node_equiv(&restored.nodes[&1], &source.nodes[&1]));
            let (again, _) = get(lens, &restored).unwrap();
            assert_eq!(again.nodes[&0].extra_fields.get("f"), Some(&output));

            let source = GInstance::from_wtype(&source).unwrap();
            let (mut view, complement) = crate::graph_lens::get(lens, &source).unwrap();
            let output = view.node_mut(0).unwrap().extra_fields.remove("f").unwrap();
            view.node_mut(1).unwrap().value = Some(FieldPresence::Present(output.clone()));
            assert_eq!(
                crate::graph_lens::check_put_domain(lens, &view, &complement),
                Ok(())
            );
            let restored = crate::graph_lens::put(lens, &view, &complement).unwrap();
            assert!(node_equiv(
                restored.node(0).unwrap(),
                source.node(0).unwrap()
            ));
            assert!(node_equiv(
                restored.node(1).unwrap(),
                source.node(1).unwrap()
            ));
            let (again, _) = crate::graph_lens::get(lens, &restored).unwrap();
            assert_eq!(again.node(0).unwrap().extra_fields.get("f"), Some(&output));
        }

        #[test]
        fn serialized_inverse_outputs_are_checked_on_their_child() {
            let expr = Expr::Builtin(panproto_expr::BuiltinOp::Neg, vec![Expr::Var("f".into())]);
            let transform = FieldTransform::ComputeField {
                target_key: "f".to_owned(),
                expr: expr.clone(),
                inverse: Some(expr),
                coercion_class: panproto_gat::CoercionClass::Iso,
            };
            for terms in [false, true] {
                assert_empty_path_binds_no_child(&transform, terms, child_source, "f");
                for program in [
                    transform.clone(),
                    FieldTransform::Case {
                        branches: vec![panproto_inst::CaseBranch {
                            predicate: Expr::Lit(Literal::Bool(true)),
                            transforms: vec![transform.clone()],
                        }],
                    },
                ] {
                    assert_serialized_child_inverse(&lens(program, terms));
                }
            }
        }

        fn distinct_child_source(lens: &Lens) -> WInstance {
            let edge = |name: &str| {
                lens.src_schema
                    .edges
                    .keys()
                    .find(|edge| edge.src == "a" && edge.name.as_deref() == Some(name))
                    .unwrap()
                    .clone()
            };
            let scalar = |id: u32, value: i64| {
                Node::new(id, "value").with_value(FieldPresence::Present(Value::Int(value)))
            };
            WInstance::new(
                HashMap::from([(0, Node::new(0, "a")), (1, scalar(1, 7)), (2, scalar(2, 3))]),
                vec![(0, 1, edge("f")), (0, 2, edge("g"))],
                Vec::new(),
                0,
                "a".into(),
            )
        }

        /// A view edit of the computed output `f` and of its distinct
        /// source child `g`, applied to the view's root and to its two
        /// children. `serialized` moves the shown output onto `f`'s child,
        /// as a serialize-and-parse round trip does.
        struct DistinctChildEdit {
            serialized: bool,
            f: Option<i64>,
            g: Option<i64>,
        }

        impl DistinctChildEdit {
            fn apply(&self, root: &mut Node, f_child: &mut Node, g_child: &mut Node) {
                let shown = root.extra_fields.remove("f").unwrap();
                let shown = self.f.map_or(shown, Value::Int);
                if self.serialized {
                    f_child.value = Some(FieldPresence::Present(shown));
                } else {
                    root.extra_fields.insert("f".to_owned(), shown);
                }
                if let Some(g) = self.g {
                    g_child.value = Some(FieldPresence::Present(Value::Int(g)));
                }
            }
        }

        /// Put a distinct-source computation's edited view in both shapes.
        /// `expected` is the restored `g` child value, or `None` for the
        /// derived-field refusal of an `f` that `g` no longer computes.
        fn assert_distinct_child_source(
            lens: &Lens,
            edit: &DistinctChildEdit,
            expected: Option<i64>,
        ) {
            let refusal = PutDomainError::DerivedCellEdited {
                elem: ViewRef::Node(0),
                field: "f".to_owned(),
            };
            let restored_g = |restored: &Node| restored.value.clone();
            let expected_g = expected.map(|g| FieldPresence::Present(Value::Int(g)));

            let source = distinct_child_source(lens);
            let (mut view, complement) = get(lens, &source).unwrap();
            let shown = edit
                .f
                .map_or_else(|| view.nodes[&0].extra_fields["f"].clone(), Value::Int);
            // A serialized view shows the output on `f`'s child, shadowing that
            // child's own value, so the child takes the shown output; in memory
            // the child keeps its source value.
            let expected_f_child = |source_child: &Node| {
                if edit.serialized {
                    Some(FieldPresence::Present(shown.clone()))
                } else {
                    source_child.value.clone()
                }
            };
            let [Some(root), Some(f_child), Some(g_child)] =
                view.nodes.get_disjoint_mut([&0, &1, &2])
            else {
                panic!("the view keeps the root and both scalar children");
            };
            edit.apply(root, f_child, g_child);
            let tree_result = put(lens, &view, &complement);
            if let Some(expected_g) = &expected_g {
                assert_eq!(check_put_domain(lens, &view, &complement), Ok(()));
                let restored = tree_result.unwrap();
                assert_eq!(restored_g(&restored.nodes[&2]).as_ref(), Some(expected_g));
                assert!(node_equiv(&restored.nodes[&0], &source.nodes[&0]));
                assert_eq!(
                    restored.nodes[&1].value,
                    expected_f_child(&source.nodes[&1])
                );
                let (again, _) = get(lens, &restored).unwrap();
                assert_eq!(again.nodes[&0].extra_fields.get("f"), Some(&shown));
            } else {
                assert_eq!(
                    check_put_domain(lens, &view, &complement),
                    Err(vec![refusal.clone()])
                );
                let Err(LensError::PutDomain(error)) = tree_result else {
                    panic!("tree put must refuse an output its visible source does not compute");
                };
                assert_eq!(error.as_ref(), &refusal);
            }

            let source = GInstance::from_wtype(&source).unwrap();
            let (mut view, complement) = crate::graph_lens::get(lens, &source).unwrap();
            let mut root = view.node(0).unwrap().clone();
            let mut f_child = view.node(1).unwrap().clone();
            let mut g_child = view.node(2).unwrap().clone();
            edit.apply(&mut root, &mut f_child, &mut g_child);
            *view.node_mut(0).unwrap() = root;
            *view.node_mut(1).unwrap() = f_child;
            *view.node_mut(2).unwrap() = g_child;
            let graph_result = crate::graph_lens::put(lens, &view, &complement);
            if let Some(expected_g) = &expected_g {
                assert_eq!(
                    crate::graph_lens::check_put_domain(lens, &view, &complement),
                    Ok(())
                );
                let restored = graph_result.unwrap();
                assert_eq!(
                    restored_g(restored.node(2).unwrap()).as_ref(),
                    Some(expected_g)
                );
                assert!(node_equiv(
                    restored.node(0).unwrap(),
                    source.node(0).unwrap()
                ));
                assert_eq!(
                    restored.node(1).unwrap().value,
                    expected_f_child(source.node(1).unwrap())
                );
                let (again, _) = crate::graph_lens::get(lens, &restored).unwrap();
                assert_eq!(again.node(0).unwrap().extra_fields.get("f"), Some(&shown));
            } else {
                assert_eq!(
                    crate::graph_lens::check_put_domain(lens, &view, &complement),
                    Err(vec![refusal.clone()])
                );
                let Err(LensError::PutDomain(error)) =
                    crate::graph_lens::put(lens, &view, &complement)
                else {
                    panic!("graph put must refuse an output its visible source does not compute");
                };
                assert_eq!(error.as_ref(), &refusal);
            }
        }

        #[test]
        fn a_visible_distinct_source_child_stays_authoritative_over_an_inverse() {
            // f = g / 2 reads a distinct source g; the inverse 2f picks
            // another representative of g's fiber whenever g is odd.
            let expr = Expr::Builtin(
                panproto_expr::BuiltinOp::Div,
                vec![Expr::Var("g".into()), Expr::Lit(Literal::Int(2))],
            );
            let inverse = Expr::Builtin(
                panproto_expr::BuiltinOp::Mul,
                vec![Expr::Var("f".into()), Expr::Lit(Literal::Int(2))],
            );
            let transform = FieldTransform::ComputeField {
                target_key: "f".to_owned(),
                expr,
                inverse: Some(inverse),
                coercion_class: panproto_gat::CoercionClass::Retraction,
            };
            for terms in [false, true] {
                assert_empty_path_binds_no_child(&transform, terms, distinct_child_source, "g");
                for program in [
                    transform.clone(),
                    FieldTransform::Case {
                        branches: vec![panproto_inst::CaseBranch {
                            predicate: Expr::Lit(Literal::Bool(true)),
                            transforms: vec![transform.clone()],
                        }],
                    },
                ] {
                    let lens = lens(program, terms);
                    let (view, _) = get(&lens, &distinct_child_source(&lens)).unwrap();
                    assert_eq!(view.nodes[&0].extra_fields.get("f"), Some(&Value::Int(1)));
                    for serialized in [false, true] {
                        // Unchanged: the odd source g = 3 survives, not 2f = 2.
                        let unchanged = DistinctChildEdit {
                            serialized,
                            f: None,
                            g: None,
                        };
                        assert_distinct_child_source(&lens, &unchanged, Some(3));
                        // A consistent edit of both keeps the edited g.
                        let both = DistinctChildEdit {
                            serialized,
                            f: Some(2),
                            g: Some(5),
                        };
                        assert_distinct_child_source(&lens, &both, Some(5));
                        // An edited g with a stale f is refused by replay
                        // rather than overwritten by the inverse of f.
                        let stale = DistinctChildEdit {
                            serialized,
                            f: None,
                            g: Some(5),
                        };
                        assert_distinct_child_source(&lens, &stale, None);
                        // An edited f alone is not pulled back into g.
                        let output = DistinctChildEdit {
                            serialized,
                            f: Some(4),
                            g: None,
                        };
                        assert_distinct_child_source(&lens, &output, None);
                    }
                }
            }
        }

        #[test]
        fn a_scalar_child_does_not_excuse_a_missing_default_field() {
            for terms in [false, true] {
                let lens = lens(
                    FieldTransform::AddField {
                        key: "f".to_owned(),
                        value: Value::Int(3),
                    },
                    terms,
                );
                let source = child_source(&lens);
                let (mut view, complement) = get(&lens, &source).unwrap();
                view.nodes.get_mut(&0).unwrap().extra_fields.remove("f");
                let expected = PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Node(0),
                    field: "f".to_owned(),
                };
                assert_eq!(
                    check_put_domain(&lens, &view, &complement),
                    Err(vec![expected.clone()])
                );

                let source = GInstance::from_wtype(&source).unwrap();
                let (mut view, complement) = crate::graph_lens::get(&lens, &source).unwrap();
                view.node_mut(0).unwrap().extra_fields.remove("f");
                assert_eq!(
                    crate::graph_lens::check_put_domain(&lens, &view, &complement),
                    Err(vec![expected])
                );
            }
        }

        #[test]
        fn own_value_and_a_field_named_value_have_independent_domains() {
            for terms in [false, true] {
                let lens = sequence(
                    vec![
                        own_value(),
                        FieldTransform::ComputeField {
                            target_key: "__value__".to_owned(),
                            expr: Expr::Lit(Literal::Int(7)),
                            inverse: None,
                            coercion_class: panproto_gat::CoercionClass::Projection,
                        },
                    ],
                    terms,
                );
                let source = Node::new(0, "a").with_value(FieldPresence::Present(Value::Int(1)));
                assert_refusal(
                    &lens,
                    &source,
                    |node| node.value = None,
                    &PutDomainError::TransformNotInvertible {
                        elem: ViewRef::Node(0),
                        field: "__value__".to_owned(),
                    },
                );
                assert_refusal(
                    &lens,
                    &source,
                    |node| {
                        node.extra_fields.remove("__value__");
                    },
                    &PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Node(0),
                        field: "__value__".to_owned(),
                    },
                );
            }
        }

        #[test]
        fn own_value_transforms_preserve_an_unrelated_field_named_value() {
            for terms in [false, true] {
                let lens = sequence(vec![own_value(), default_field()], terms);
                let mut source =
                    Node::new(0, "a").with_value(FieldPresence::Present(Value::Int(1)));
                source
                    .extra_fields
                    .insert("__value__".to_owned(), Value::Int(9));
                assert_roundtrip(&lens, &source, |node| {
                    node.extra_fields
                        .insert("__value__".to_owned(), Value::Int(2));
                });
            }
        }

        #[test]
        fn an_empty_path_keeps_its_inner_derived_field_domain() {
            for terms in [false, true] {
                let lens = lens(
                    FieldTransform::PathTransform {
                        path: Vec::new(),
                        inner: Box::new(default_field()),
                    },
                    terms,
                );
                assert_refusal(
                    &lens,
                    &Node::new(0, "a"),
                    |node| {
                        node.extra_fields.insert("f".to_owned(), Value::Int(1));
                    },
                    &PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Node(0),
                        field: "f".to_owned(),
                    },
                );
            }
        }

        #[test]
        fn a_rename_without_its_source_preserves_the_existing_target() {
            for terms in [false, true] {
                let lens = sequence(
                    vec![
                        FieldTransform::DropField {
                            key: "hidden".to_owned(),
                        },
                        FieldTransform::RenameField {
                            old_key: "f".to_owned(),
                            new_key: "g".to_owned(),
                        },
                    ],
                    terms,
                );
                let mut source = Node::new(0, "a");
                source
                    .extra_fields
                    .insert("hidden".to_owned(), Value::Int(9));
                source.extra_fields.insert("g".to_owned(), Value::Int(7));
                assert_roundtrip(&lens, &source, |node| {
                    node.extra_fields.insert("g".to_owned(), Value::Int(8));
                });
                let (mut view, complement) = get(&lens, &tree(source)).unwrap();
                view.nodes
                    .get_mut(&0)
                    .unwrap()
                    .extra_fields
                    .insert("g".to_owned(), Value::Int(8));
                let restored = put(&lens, &view, &complement).unwrap();
                assert!(!restored.nodes[&0].extra_fields.contains_key("f"));
            }
        }

        #[test]
        fn a_selected_noop_default_preserves_the_existing_field() {
            for terms in [false, true] {
                for selected in [false, true] {
                    let lens = sequence(
                        vec![
                            FieldTransform::DropField {
                                key: "hidden".to_owned(),
                            },
                            FieldTransform::Case {
                                branches: vec![panproto_inst::CaseBranch {
                                    predicate: Expr::Lit(Literal::Bool(selected)),
                                    transforms: vec![default_field()],
                                }],
                            },
                        ],
                        terms,
                    );
                    let mut source = Node::new(0, "a");
                    source
                        .extra_fields
                        .insert("hidden".to_owned(), Value::Int(9));
                    source.extra_fields.insert("f".to_owned(), Value::Int(7));
                    assert_roundtrip(&lens, &source, |node| {
                        node.extra_fields.insert("f".to_owned(), Value::Int(8));
                    });
                }
            }
        }

        #[test]
        fn reanchoring_can_supply_a_new_computations_input() {
            for terms in [false, true] {
                let mut lens = lens(default_field(), terms);
                lens.compiled.field_transforms.insert(
                    "b".into(),
                    vec![FieldTransform::ComputeField {
                        target_key: "g".to_owned(),
                        expr: Expr::Var("input".into()),
                        inverse: None,
                        coercion_class: panproto_gat::CoercionClass::Projection,
                    }],
                );
                assert_roundtrip(&lens, &Node::new(0, "a"), |node| {
                    node.anchor = "b".into();
                    node.extra_fields.insert("input".to_owned(), Value::Int(2));
                    node.extra_fields.insert("g".to_owned(), Value::Int(2));
                });
            }
        }

        #[test]
        fn replay_expression_failures_are_typed_in_both_shapes() {
            use panproto_expr::BuiltinOp;
            let quotient = Expr::Builtin(
                BuiltinOp::Div,
                vec![Expr::Lit(Literal::Int(1)), Expr::Var("g".into())],
            );
            for terms in [false, true] {
                for transform in [
                    FieldTransform::ComputeField {
                        target_key: "f".to_owned(),
                        expr: quotient.clone(),
                        inverse: None,
                        coercion_class: panproto_gat::CoercionClass::Projection,
                    },
                    FieldTransform::Case {
                        branches: vec![panproto_inst::CaseBranch {
                            predicate: Expr::Builtin(
                                BuiltinOp::Eq,
                                vec![quotient.clone(), Expr::Lit(Literal::Int(1))],
                            ),
                            transforms: vec![default_field()],
                        }],
                    },
                ] {
                    let mut source = Node::new(0, "a");
                    source.extra_fields.insert("g".to_owned(), Value::Int(1));
                    assert_refusal(
                        &lens(transform, terms),
                        &source,
                        |node| {
                            node.extra_fields.insert("g".to_owned(), Value::Int(0));
                        },
                        &PutDomainError::SurvivalPredicateRefused {
                            elem: ViewRef::Node(0),
                            refusal: panproto_inst::PredicateRefusal::EvaluationFailed {
                                message: panproto_expr::ExprError::DivisionByZero.to_string(),
                            },
                        },
                    );
                }
            }
        }

        #[test]
        fn executed_own_value_inverses_pull_back_edits() {
            let expression =
                Expr::Builtin(panproto_expr::BuiltinOp::Not, vec![Expr::Var("v".into())]);
            let inverse = FieldTransform::ApplyExpr {
                key: "__value__".to_owned(),
                expr: expression.clone(),
                inverse: Some(expression),
                coercion_class: panproto_gat::CoercionClass::Iso,
            };
            let source = Node::new(0, "a").with_value(FieldPresence::Present(Value::Bool(true)));
            for terms in [false, true] {
                for transform in [
                    inverse.clone(),
                    FieldTransform::PathTransform {
                        path: Vec::new(),
                        inner: Box::new(inverse.clone()),
                    },
                    FieldTransform::Case {
                        branches: vec![panproto_inst::CaseBranch {
                            predicate: Expr::Lit(Literal::Bool(true)),
                            transforms: vec![inverse.clone()],
                        }],
                    },
                ] {
                    let lens = lens(transform, terms);
                    assert_roundtrip(&lens, &source, |node| {
                        node.value = Some(FieldPresence::Present(Value::Bool(true)));
                    });
                    let (mut view, complement) = get(&lens, &tree(source.clone())).unwrap();
                    view.nodes.get_mut(&0).unwrap().value =
                        Some(FieldPresence::Present(Value::Bool(true)));
                    assert_eq!(
                        put(&lens, &view, &complement).unwrap().nodes[&0].value,
                        Some(FieldPresence::Present(Value::Bool(false)))
                    );
                }
            }
        }

        #[test]
        fn disabling_a_keep_branch_cannot_reintroduce_a_hidden_field() {
            for terms in [false, true] {
                let transform = FieldTransform::Case {
                    branches: vec![panproto_inst::CaseBranch {
                        predicate: Expr::Builtin(
                            panproto_expr::BuiltinOp::Eq,
                            vec![Expr::Var("g".into()), Expr::Lit(Literal::Int(0))],
                        ),
                        transforms: vec![FieldTransform::KeepFields {
                            keys: vec!["g".to_owned()],
                        }],
                    }],
                };
                let mut source = Node::new(0, "a");
                source.extra_fields.insert("g".to_owned(), Value::Int(0));
                source.extra_fields.insert("f".to_owned(), Value::Int(9));
                assert_refusal(
                    &lens(transform, terms),
                    &source,
                    |node| {
                        node.extra_fields.insert("g".to_owned(), Value::Int(1));
                        assert!(!node.extra_fields.contains_key("f"));
                    },
                    &PutDomainError::TransformNotInvertible {
                        elem: ViewRef::Node(0),
                        field: "f".to_owned(),
                    },
                );
            }
        }

        #[test]
        fn reanchoring_keeps_fields_when_the_old_transform_no_longer_applies() {
            for terms in [false, true] {
                assert_roundtrip(&lens(default_field(), terms), &Node::new(0, "a"), |node| {
                    node.anchor = "value".into();
                });
            }
        }

        #[test]
        fn reanchoring_to_another_transform_keeps_its_untouched_fields() {
            for terms in [false, true] {
                let mut lens = lens(default_field(), terms);
                lens.compiled.field_transforms.insert(
                    "b".into(),
                    vec![FieldTransform::AddField {
                        key: "g".to_owned(),
                        value: Value::Int(0),
                    }],
                );
                assert_roundtrip(&lens, &Node::new(0, "a"), |node| {
                    node.anchor = "b".into();
                    node.extra_fields.insert("g".to_owned(), Value::Int(0));
                });
            }
        }

        #[test]
        fn untouched_fields_keep_their_edited_presence() {
            for terms in [false, true] {
                let lens = lens(default_field(), terms);
                assert_roundtrip(&lens, &Node::new(0, "a"), |node| {
                    node.extra_fields.insert("g".to_owned(), Value::Int(1));
                });
                let mut source = Node::new(0, "a");
                source.extra_fields.insert("g".to_owned(), Value::Int(1));
                assert_roundtrip(&lens, &source, |node| {
                    node.extra_fields.remove("g");
                });
            }
        }

        #[test]
        fn reanchoring_keeps_own_values_outside_the_current_transform() {
            for terms in [false, true] {
                let mut lens = lens(own_value(), terms);
                lens.compiled
                    .field_transforms
                    .insert("b".into(), vec![default_field()]);
                let source = Node::new(0, "a").with_value(FieldPresence::Present(Value::Int(1)));
                for anchor in ["value", "b"] {
                    assert_roundtrip(&lens, &source, |node| {
                        node.anchor = anchor.into();
                        node.value = Some(FieldPresence::Present(Value::Int(2)));
                        if anchor == "b" {
                            node.extra_fields.insert("f".to_owned(), Value::Int(0));
                        }
                    });
                }
            }
        }

        #[test]
        fn deleting_an_added_default_is_a_derived_field_edit() {
            for terms in [false, true] {
                assert_refusal(
                    &lens(default_field(), terms),
                    &Node::new(0, "a"),
                    |node| {
                        node.extra_fields.remove("f");
                    },
                    &PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Node(0),
                        field: "f".to_owned(),
                    },
                );
            }
        }

        #[test]
        fn deleting_a_recomputed_own_value_is_a_noninvertible_edit() {
            let source = Node::new(0, "a").with_value(FieldPresence::Present(Value::Int(1)));
            for terms in [false, true] {
                assert_refusal(
                    &lens(own_value(), terms),
                    &source,
                    |node| node.value = None,
                    &PutDomainError::TransformNotInvertible {
                        elem: ViewRef::Node(0),
                        field: "__value__".to_owned(),
                    },
                );
            }
        }

        #[test]
        fn inserting_an_erased_field_is_a_noninvertible_edit() {
            for terms in [false, true] {
                for transform in [
                    FieldTransform::DropField {
                        key: "f".to_owned(),
                    },
                    FieldTransform::KeepFields {
                        keys: vec!["g".to_owned()],
                    },
                ] {
                    assert_refusal(
                        &lens(transform, terms),
                        &Node::new(0, "a"),
                        |node| {
                            node.extra_fields.insert("f".to_owned(), Value::Int(1));
                        },
                        &PutDomainError::TransformNotInvertible {
                            elem: ViewRef::Node(0),
                            field: "f".to_owned(),
                        },
                    );
                }
            }
        }

        #[test]
        fn a_later_default_decides_the_erased_fields_edit_refusal() {
            for terms in [false, true] {
                for erasure in [
                    FieldTransform::DropField {
                        key: "f".to_owned(),
                    },
                    FieldTransform::KeepFields {
                        keys: vec!["g".to_owned()],
                    },
                ] {
                    let mut lens = lens(erasure, terms);
                    if terms {
                        lens.compiled
                            .op_term_assignments
                            .get_mut(&Name::from("a"))
                            .unwrap()
                            .push(TermAssignment::from_field_transform(&default_field()));
                    } else {
                        lens.compiled
                            .field_transforms
                            .get_mut(&Name::from("a"))
                            .unwrap()
                            .push(default_field());
                    }
                    assert_refusal(
                        &lens,
                        &Node::new(0, "a"),
                        |node| {
                            node.extra_fields.insert("f".to_owned(), Value::Int(1));
                        },
                        &PutDomainError::DerivedCellEdited {
                            elem: ViewRef::Node(0),
                            field: "f".to_owned(),
                        },
                    );
                }
            }
        }

        #[test]
        fn a_later_erasure_decides_the_derived_fields_edit_refusal() {
            for terms in [false, true] {
                for erasure in [
                    FieldTransform::DropField {
                        key: "f".to_owned(),
                    },
                    FieldTransform::KeepFields {
                        keys: vec!["g".to_owned()],
                    },
                ] {
                    let mut lens = lens(default_field(), terms);
                    if terms {
                        lens.compiled
                            .op_term_assignments
                            .get_mut(&Name::from("a"))
                            .unwrap()
                            .push(TermAssignment::from_field_transform(&erasure));
                    } else {
                        lens.compiled
                            .field_transforms
                            .get_mut(&Name::from("a"))
                            .unwrap()
                            .push(erasure);
                    }
                    assert_refusal(
                        &lens,
                        &Node::new(0, "a"),
                        |node| {
                            node.extra_fields.insert("f".to_owned(), Value::Int(1));
                        },
                        &PutDomainError::TransformNotInvertible {
                            elem: ViewRef::Node(0),
                            field: "f".to_owned(),
                        },
                    );
                }
            }
        }
    }

    /// A field the migration adds that the source already carries with the
    /// added value is unchanged by get, so get records nothing for the node
    /// and put must keep the field: `GetPut` in both shapes.
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
                .vertex("value", "integer", None)
                .unwrap()
                .edge("a", "value", "prop", Some("f"))
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
                HashMap::from([(0, carrying())]),
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
                .vertex("value", "integer", None)
                .unwrap()
                .edge("a", "value", "prop", Some("f"))
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
                HashMap::from([(0, Node::new(0, "a"))]),
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
                HashMap::from([(0, Node::new(0, "a"))]),
                Vec::new(),
                Vec::new(),
                0,
                "a".into(),
            );
            let (view, complement) = get(&lens, &source).unwrap();
            let mut edited = view;
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
            let mut edited = view;
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
                HashMap::from([(0, Node::new(0, "r"))]),
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
            assert!(
                !complement.original_anchors.contains_key(&1),
                "the injective vertex rename must remain reversible without anchor residue"
            );
            assert_eq!(check_put_domain(&lens, &view, &complement), Ok(()));

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

        #[test]
        fn arc_order_is_a_global_compatibility_constraint() {
            let mut left = Complement::empty();
            left.arc_order = vec![(0, 1), (0, 2)];
            left.source_fingerprint = BOUND;
            let mut right = Complement::empty();
            right.arc_order = vec![(0, 2), (0, 1)];
            right.source_fingerprint = BOUND;

            assert!(matches!(
                left.compose(&right),
                Err(LensError::ComplementConflict {
                    kind: "arc_order",
                    ..
                })
            ));
            assert!(matches!(
                right.compose(&left),
                Err(LensError::ComplementConflict {
                    kind: "arc_order",
                    ..
                })
            ));
            assert!(!left.is_compatible(&right));
        }

        #[test]
        fn equal_arc_orders_merge_and_empty_order_is_the_unit() {
            let mut ordered = Complement::empty();
            ordered.arc_order = vec![(0, 2), (0, 1)];
            ordered.source_fingerprint = BOUND;
            let mut same = ordered.clone();
            same.id_floor = 7;
            let mut no_order = Complement::empty();
            no_order.source_fingerprint = BOUND;

            assert_eq!(
                ordered.compose(&same).expect("equal order").arc_order,
                ordered.arc_order
            );
            assert_eq!(
                no_order.compose(&ordered).expect("left unit").arc_order,
                ordered.arc_order
            );
            assert_eq!(
                ordered.compose(&no_order).expect("right unit").arc_order,
                ordered.arc_order
            );
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
            use panproto_inst::{EdgeId, SynthKey};
            use proptest::prelude::*;
            use std::collections::{BTreeMap, HashMap, HashSet};

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
            fn canon_anchor(k: u32) -> Name {
                Name::from(format!("anchor-{k}"))
            }
            fn canon_synth_key(k: u32) -> SynthKey {
                SynthKey::Expand {
                    source_edge: EdgeId(k),
                    hop: u16::try_from(k).expect("generated key is in u16 range"),
                }
            }

            type IdSet = HashSet<u32>;
            type PairSet = HashSet<(u32, u32)>;

            fn arb_id_set() -> impl Strategy<Value = IdSet> {
                prop::collection::hash_set(0u32..KEY_POOL, 0..=4)
            }
            fn arb_pair_set() -> impl Strategy<Value = PairSet> {
                prop::collection::hash_set((0u32..KEY_POOL, 0u32..KEY_POOL), 0..=4)
            }
            fn arb_arc_order() -> impl Strategy<Value = Vec<(u32, u32)>> {
                prop::collection::vec((0u32..KEY_POOL, 0u32..KEY_POOL), 0..=4).prop_map(|pairs| {
                    let mut seen = HashSet::new();
                    pairs
                        .into_iter()
                        .filter(|pair| seen.insert(*pair))
                        .collect()
                })
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
            /// `original_values`, `synthesized_nodes`, `contracted_into`,
            /// `original_anchors`, `synthesized_keys`, `id_floor`, and a
            /// fingerprint all independently populated from the shared key
            /// pool. `arc_order` is either empty or the case's one shared
            /// sequence, which keeps generated operands inside its partial
            /// compatibility domain.
            fn arb_complement(
                binding: Binding,
                shared_arc_order: Vec<(u32, u32)>,
            ) -> impl Strategy<Value = Complement> {
                (
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
                    ),
                    (
                        arb_id_set(),
                        arb_id_set(),
                        arb_id_set(),
                        0u32..128,
                        any::<bool>(),
                    ),
                )
                    .prop_map(
                        move |(
                            (
                                nodes,
                                arcs,
                                fans,
                                contraction,
                                parents,
                                extra,
                                arc_edges,
                                values,
                                synth,
                            ),
                            (contracted, anchors, synth_keys, id_floor, records_arc_order),
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
                            for k in contracted {
                                c.contracted_into.insert(k, canon_parent(k));
                            }
                            for k in anchors {
                                c.original_anchors.insert(k, canon_anchor(k));
                            }
                            c.synthesized_keys = synth_keys
                                .into_iter()
                                .map(|k| (k, canon_synth_key(k)))
                                .collect::<BTreeMap<_, _>>();
                            c.id_floor = id_floor;
                            if records_arc_order {
                                c.arc_order.clone_from(&shared_arc_order);
                            }
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
                    && a.contracted_into == b.contracted_into
                    && a.original_anchors == b.original_anchors
                    && a.arc_order == b.arc_order
                    && a.id_floor == b.id_floor
                    && a.synthesized_keys == b.synthesized_keys
            }

            fn arb_one() -> impl Strategy<Value = Complement> {
                (arb_binding(), arb_arc_order())
                    .prop_flat_map(|(binding, order)| arb_complement(binding, order))
            }

            fn arb_two() -> impl Strategy<Value = (Complement, Complement)> {
                (arb_binding(), arb_arc_order()).prop_flat_map(|(binding, order)| {
                    (
                        arb_complement(binding, order.clone()),
                        arb_complement(binding, order),
                    )
                })
            }

            fn arb_three() -> impl Strategy<Value = (Complement, Complement, Complement)> {
                (arb_binding(), arb_arc_order()).prop_flat_map(|(binding, order)| {
                    (
                        arb_complement(binding, order.clone()),
                        arb_complement(binding, order.clone()),
                        arb_complement(binding, order),
                    )
                })
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
