//! Shape-specific execution and observational equivalence for lenses.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::{
    EdgeId, GEdge, GInstance, GraphComplement, PutDomainError, SynthIndex, SynthKey, WInstance,
};
use panproto_schema::Edge;

use crate::derived::{DerivedMap, extra_fields_equiv_modulo};
use crate::{Complement, Lens, LensError};

/// The instance operations a generic lens law needs from one shape.
pub trait LensShape {
    /// Instance representation.
    type Instance: Clone;
    /// Complement representation.
    type Complement: Clone;
    /// Shape-specific execution context.
    type Context;

    /// Forward direction.
    ///
    /// # Errors
    ///
    /// Plan compilation or shape execution failure.
    fn get(
        lens: &Lens,
        source: &Self::Instance,
        ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), LensError>;

    /// Backward direction.
    ///
    /// # Errors
    ///
    /// Identity, put-domain or restoration failure.
    fn put(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Result<Self::Instance, LensError>;

    /// Put-domain predicate.
    ///
    /// # Errors
    ///
    /// Every ordered reason the view cannot be written back.
    fn check_put_domain(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>>;

    /// Stable keys of synthesized presentation objects in this instance.
    ///
    /// The instance is part of the input because tree arcs and table
    /// foreign-key pairs acquire graph edge ids only through their current
    /// embedding. A complement alone cannot name those presentation ids.
    fn synth_index(instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex;

    /// Law equivalence, indexed by synthesized keys.
    ///
    /// Unequal node or edge `SynthKey` sets are never equivalent, even when
    /// the affected objects happen to use the same raw presentation ids.
    fn equivalent(a: &Self::Instance, ia: &SynthIndex, b: &Self::Instance, ib: &SynthIndex)
    -> bool;

    /// The first difference under [`Self::equivalent`].
    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String>;

    /// The first independent-coordinate difference.
    fn divergence_modulo_derived(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
        derived: &DerivedMap,
    ) -> Option<String>;

    /// The first complement field read by put on which two complements differ.
    fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String>;

    /// Equivalent but non-identical presentation witnesses.
    fn congruence_witnesses(
        instance: &Self::Instance,
        index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)>;
}

/// Tree-shaped lens execution.
#[derive(Clone, Copy, Debug, Default)]
pub struct TreeShape;

/// Graph-shaped lens execution.
#[derive(Clone, Copy, Debug, Default)]
pub struct GraphShape;

/// A tree-complement node coordinate with presentation-only synthesized ids
/// replaced by their stable synthesis keys.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NormalizedId {
    Origin(u32),
    Synthesized(SynthKey),
}

fn normalized_id(complement: &Complement, id: u32) -> NormalizedId {
    complement
        .synthesized_keys
        .get(&id)
        .cloned()
        .map_or(NormalizedId::Origin(id), NormalizedId::Synthesized)
}

fn normalized_id_map_equiv<V, F>(
    a: &HashMap<u32, V>,
    ac: &Complement,
    b: &HashMap<u32, V>,
    bc: &Complement,
    values_equiv: F,
) -> bool
where
    F: Fn(&V, &Complement, &V, &Complement) -> bool,
{
    if a.len() != b.len() {
        return false;
    }
    let mut seen = BTreeSet::new();
    a.iter().all(|(&id, value)| {
        let key = normalized_id(ac, id);
        if !seen.insert(key.clone()) {
            return false;
        }
        let mut matches = b
            .iter()
            .filter(|(other_id, _)| normalized_id(bc, **other_id) == key);
        let Some((_, other)) = matches.next() else {
            return false;
        };
        matches.next().is_none() && values_equiv(value, ac, other, bc)
    })
}

fn normalized_pair_map_equiv<V, F>(
    a: &HashMap<(u32, u32), V>,
    ac: &Complement,
    b: &HashMap<(u32, u32), V>,
    bc: &Complement,
    values_equiv: F,
) -> bool
where
    F: Fn(&V, &V) -> bool,
{
    if a.len() != b.len() {
        return false;
    }
    let mut seen = BTreeSet::new();
    a.iter().all(|(&(parent, child), value)| {
        let key = (normalized_id(ac, parent), normalized_id(ac, child));
        if !seen.insert(key.clone()) {
            return false;
        }
        let mut matches = b.iter().filter(|((other_parent, other_child), _)| {
            (
                normalized_id(bc, *other_parent),
                normalized_id(bc, *other_child),
            ) == key
        });
        let Some((_, other)) = matches.next() else {
            return false;
        };
        matches.next().is_none() && values_equiv(value, other)
    })
}

fn normalized_parent_map_equiv(a: &Complement, b: &Complement) -> bool {
    normalized_id_map_equiv(
        &a.original_parent,
        a,
        &b.original_parent,
        b,
        |parent, ac, other, bc| normalized_id(ac, *parent) == normalized_id(bc, *other),
    )
}

fn normalized_node_map_equiv(a: &Complement, b: &Complement) -> bool {
    normalized_id_map_equiv(
        &a.dropped_nodes,
        a,
        &b.dropped_nodes,
        b,
        |node, ac, other, bc| {
            if normalized_id(ac, node.id) != normalized_id(bc, other.id) {
                return false;
            }
            let mut left = node.clone();
            let mut right = other.clone();
            left.id = 0;
            right.id = 0;
            crate::asymmetric::node_equiv(&left, &right)
        },
    )
}

fn normalized_arc_set_equiv(a: &Complement, b: &Complement) -> bool {
    if a.dropped_arcs.len() != b.dropped_arcs.len() {
        return false;
    }
    let mut used = vec![false; b.dropped_arcs.len()];
    a.dropped_arcs.iter().all(|(parent, child, edge)| {
        let endpoints = (normalized_id(a, *parent), normalized_id(a, *child));
        let Some(index) = b.dropped_arcs.iter().enumerate().position(
            |(index, (other_parent, other_child, other_edge))| {
                !used[index]
                    && endpoints
                        == (
                            normalized_id(b, *other_parent),
                            normalized_id(b, *other_child),
                        )
                    && edge == other_edge
            },
        ) else {
            return false;
        };
        used[index] = true;
        true
    })
}

fn normalized_fan_equiv(
    a: &panproto_inst::Fan,
    ac: &Complement,
    b: &panproto_inst::Fan,
    bc: &Complement,
) -> bool {
    a.hyper_edge_id == b.hyper_edge_id
        && normalized_id(ac, a.parent) == normalized_id(bc, b.parent)
        && a.children.len() == b.children.len()
        && a.children.iter().all(|(label, child)| {
            b.children
                .get(label)
                .is_some_and(|other| normalized_id(ac, *child) == normalized_id(bc, *other))
        })
}

fn normalized_fans_equiv(a: &Complement, b: &Complement) -> bool {
    a.dropped_fans.len() == b.dropped_fans.len()
        && a.dropped_fans
            .iter()
            .zip(&b.dropped_fans)
            .all(|(left, right)| normalized_fan_equiv(left, a, right, b))
}

fn normalized_arc_order_equiv(a: &Complement, b: &Complement) -> bool {
    a.arc_order.len() == b.arc_order.len()
        && a.arc_order.iter().zip(&b.arc_order).all(
            |(&(parent, child), &(other_parent, other_child))| {
                (normalized_id(a, parent), normalized_id(a, child))
                    == (
                        normalized_id(b, other_parent),
                        normalized_id(b, other_child),
                    )
            },
        )
}

fn normalized_synthesized_nodes(complement: &Complement) -> BTreeSet<NormalizedId> {
    complement
        .synthesized_nodes
        .iter()
        .map(|&id| normalized_id(complement, id))
        .collect()
}

fn synthesized_metadata_coherent(complement: &Complement) -> bool {
    complement.synthesized_nodes.len() == complement.synthesized_keys.len()
        && complement
            .synthesized_nodes
            .iter()
            .all(|id| complement.synthesized_keys.contains_key(id))
}

fn synthesized_keys_equiv(a: &Complement, b: &Complement) -> bool {
    let a_keys: BTreeSet<&SynthKey> = a.synthesized_keys.values().collect();
    let b_keys: BTreeSet<&SynthKey> = b.synthesized_keys.values().collect();
    a_keys.len() == a.synthesized_keys.len()
        && b_keys.len() == b.synthesized_keys.len()
        && a_keys == b_keys
}

impl LensShape for TreeShape {
    type Instance = WInstance;
    type Complement = Complement;
    type Context = ();

    fn get(
        lens: &Lens,
        source: &Self::Instance,
        _ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), LensError> {
        crate::asymmetric::get(lens, source)
    }

    fn put(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<Self::Instance, LensError> {
        crate::asymmetric::put(lens, view, complement)
    }

    fn check_put_domain(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        crate::asymmetric::check_put_domain(lens, view, complement)
    }

    fn synth_index(instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex {
        // `contraction_choices` also records the arcs of an expansion chain.
        // Index those arcs by their expansion keys, which are stable under a
        // coherent presentation-id change, rather than by a `Contract` key
        // containing the chain's current presentation endpoints.
        let mut indexed_complement = complement.clone();
        indexed_complement
            .contraction_choices
            .retain(|&(parent, child), _| {
                !complement.synthesized_keys.contains_key(&parent)
                    && !complement.synthesized_keys.contains_key(&child)
            });
        GInstance::from_wtype_embedded(instance, None).map_or_else(
            |_| SynthIndex::default(),
            |(_, embedding)| {
                let mut index = indexed_complement.synth_index(&embedding);
                index.nodes.retain(|id, _| instance.nodes.contains_key(id));
                index
            },
        )
    }

    fn equivalent(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> bool {
        tree_divergence(a, ia, b, ib).is_none()
    }

    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String> {
        tree_divergence(a, ia, b, ib)
    }

    fn divergence_modulo_derived(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
        _derived: &DerivedMap,
    ) -> Option<String> {
        Self::divergence(a, ia, b, ib)
    }

    fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
        if !synthesized_metadata_coherent(a) || !synthesized_metadata_coherent(b) {
            return Some("synthesized metadata is incoherent".to_owned());
        }
        if !synthesized_keys_equiv(a, b) {
            return Some("synthesized keys differ".to_owned());
        }
        if normalized_synthesized_nodes(a) != normalized_synthesized_nodes(b) {
            return Some("synthesized nodes differ".to_owned());
        }
        if !normalized_node_map_equiv(a, b) {
            return Some("dropped_nodes differ".to_owned());
        }
        if !normalized_arc_set_equiv(a, b) {
            return Some("dropped_arcs differ".to_owned());
        }
        if !normalized_fans_equiv(a, b) {
            return Some("dropped_fans differ".to_owned());
        }
        if !normalized_pair_map_equiv(
            &a.contraction_choices,
            a,
            &b.contraction_choices,
            b,
            PartialEq::eq,
        ) {
            return Some("contraction_choices differ".to_owned());
        }
        if !normalized_parent_map_equiv(a, b) {
            return Some("original_parent differs".to_owned());
        }
        if !normalized_pair_map_equiv(&a.arc_edges, a, &b.arc_edges, b, PartialEq::eq) {
            return Some("arc_edges differ".to_owned());
        }
        if !normalized_arc_order_equiv(a, b) {
            return Some("arc_order differs".to_owned());
        }
        if !normalized_id_map_equiv(
            &a.original_anchors,
            a,
            &b.original_anchors,
            b,
            |anchor, _, other, _| anchor == other,
        ) {
            return Some("original_anchors differ".to_owned());
        }
        if !normalized_id_map_equiv(
            &a.original_extra_fields,
            a,
            &b.original_extra_fields,
            b,
            |fields, _, other, _| crate::asymmetric::extra_fields_equiv(fields, other),
        ) {
            return Some("original_extra_fields differ".to_owned());
        }
        if !normalized_id_map_equiv(
            &a.original_values,
            a,
            &b.original_values,
            b,
            |value, _, other, _| crate::asymmetric::presence_equiv(value.as_ref(), other.as_ref()),
        ) {
            return Some("original_values differ".to_owned());
        }
        None
    }

    fn congruence_witnesses(
        instance: &Self::Instance,
        index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)> {
        let Some((&old, key)) = index.nodes.iter().next() else {
            return Vec::new();
        };
        let Some(new) = fresh_node_id(instance.nodes.keys().copied()) else {
            return Vec::new();
        };
        let mut nodes = instance.nodes.clone();
        let Some(mut node) = nodes.remove(&old) else {
            return Vec::new();
        };
        node.id = new;
        nodes.insert(new, node);
        let arcs = instance
            .arcs
            .iter()
            .map(|(parent, child, edge)| {
                (
                    if *parent == old { new } else { *parent },
                    if *child == old { new } else { *child },
                    edge.clone(),
                )
            })
            .collect();
        let mut fans = instance.fans.clone();
        for fan in &mut fans {
            if fan.parent == old {
                fan.parent = new;
            }
            for child in fan.children.values_mut() {
                if *child == old {
                    *child = new;
                }
            }
        }
        let mut next = index.clone();
        next.nodes.remove(&old);
        next.nodes.insert(new, key.clone());
        vec![(
            WInstance::new(
                nodes,
                arcs,
                fans,
                if instance.root == old {
                    new
                } else {
                    instance.root
                },
                instance.schema_root.clone(),
            ),
            next,
        )]
    }
}

impl LensShape for GraphShape {
    type Instance = GInstance;
    type Complement = GraphComplement;
    type Context = ();

    fn get(
        lens: &Lens,
        source: &Self::Instance,
        _ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), LensError> {
        crate::graph_lens::get(lens, source)
    }

    fn put(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<Self::Instance, LensError> {
        crate::graph_lens::put(lens, view, complement)
    }

    fn check_put_domain(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        crate::graph_lens::check_put_domain(lens, view, complement)
    }

    fn synth_index(instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex {
        let mut index = complement.synth_index();
        index.nodes.retain(|id, _| instance.node(*id).is_some());
        index.edges.retain(|id, _| instance.edge(*id).is_some());
        index
    }

    fn equivalent(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> bool {
        graph_divergence(a, ia, b, ib, None).is_none()
    }

    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String> {
        graph_divergence(a, ia, b, ib, None)
    }

    fn divergence_modulo_derived(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
        derived: &DerivedMap,
    ) -> Option<String> {
        graph_divergence(a, ia, b, ib, Some(derived))
    }

    fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
        graph_complement_divergence(a, b)
    }

    fn congruence_witnesses(
        instance: &Self::Instance,
        index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)> {
        graph_congruence_witness(instance, index)
            .into_iter()
            .collect()
    }
}

fn tree_divergence(
    a: &WInstance,
    ia: &SynthIndex,
    b: &WInstance,
    ib: &SynthIndex,
) -> Option<String> {
    let Some((left, right)) = tree_normalized(a, ia, b, ib) else {
        return Some("synthesized key sets differ".to_owned());
    };
    if left.root != right.root {
        return Some(format!("root: {} vs {}", left.root, right.root));
    }
    if left.schema_root != right.schema_root {
        return Some(format!(
            "schema root: {} vs {}",
            left.schema_root, right.schema_root
        ));
    }
    if left.nodes.len() != right.nodes.len() {
        return Some(format!(
            "node count: {} vs {}",
            left.nodes.len(),
            right.nodes.len()
        ));
    }
    for (id, node) in &left.nodes {
        let Some(other) = right.nodes.get(id) else {
            return Some(format!("node {id} is absent on the right"));
        };
        if !crate::asymmetric::node_equiv(node, other) {
            return Some(format!("node {id} differs"));
        }
    }
    if tree_ordered_arcs(&left) != tree_ordered_arcs(&right) {
        return Some("per-parent ordered arcs differ".to_owned());
    }
    (left.fans != right.fans).then(|| "ordered fans differ".to_owned())
}

/// Each parent's ordered child/label sequence.
///
/// A tree's flat arc vector is an encoding of these sequences. Interleaving
/// arcs belonging to distinct parents changes only that encoding, whereas
/// changing the order within one parent changes the observable tree.
fn tree_ordered_arcs(instance: &WInstance) -> BTreeMap<u32, Vec<(u32, Edge)>> {
    let mut by_parent: BTreeMap<u32, Vec<(u32, Edge)>> = BTreeMap::new();
    for (parent, child, edge) in &instance.arcs {
        by_parent
            .entry(*parent)
            .or_default()
            .push((*child, edge.clone()));
    }
    by_parent
}

#[allow(
    clippy::too_many_lines,
    reason = "keep the structural comparison in one auditable decision procedure"
)]
fn graph_divergence(
    a: &GInstance,
    ia: &SynthIndex,
    b: &GInstance,
    ib: &SynthIndex,
    derived: Option<&DerivedMap>,
) -> Option<String> {
    if !graph_synth_index_coherent(a, ia) || !graph_synth_index_coherent(b, ib) {
        return Some("synthesized metadata is incoherent".to_owned());
    }
    let a_node_keys: BTreeSet<&SynthKey> = ia.nodes.values().collect();
    let b_node_keys: BTreeSet<&SynthKey> = ib.nodes.values().collect();
    if a_node_keys != b_node_keys {
        return Some("synthesized node key sets differ".to_owned());
    }
    let a_edge_keys: BTreeSet<&SynthKey> = ia.edges.values().collect();
    let b_edge_keys: BTreeSet<&SynthKey> = ib.edges.values().collect();
    if a_edge_keys != b_edge_keys {
        return Some("synthesized edge key sets differ".to_owned());
    }
    if a.node_count() != b.node_count() {
        return Some(format!(
            "node count: {} vs {}",
            a.node_count(),
            b.node_count()
        ));
    }
    let b_nodes_by_key: BTreeMap<&SynthKey, u32> =
        ib.nodes.iter().map(|(&id, key)| (key, id)).collect();
    let mut a_to_b = BTreeMap::new();
    let mut a_ids: Vec<u32> = a.nodes().keys().copied().collect();
    a_ids.sort_unstable();
    for id in a_ids {
        if !ia.nodes.contains_key(&id) && ib.nodes.contains_key(&id) {
            return Some(format!("source node {id} is synthesized on the right"));
        }
        let other_id = ia
            .nodes
            .get(&id)
            .and_then(|key| b_nodes_by_key.get(key).copied())
            .unwrap_or(id);
        let left = &a.nodes()[&id];
        let Some(right) = b.node(other_id) else {
            return Some(format!("node {id} is absent on the right"));
        };
        if left.anchor != right.anchor
            || left.discriminator != right.discriminator
            || left.position != right.position
            || left.shape != right.shape
            || !crate::asymmetric::extra_fields_equiv(&left.annotations, &right.annotations)
        {
            return Some(format!("node {id} differs"));
        }
        if let Some(derived) = derived {
            if let Some(fiber) = derived.get(&left.anchor) {
                if let Some(detail) =
                    extra_fields_equiv_modulo(&left.extra_fields, &right.extra_fields, fiber, &[])
                {
                    return Some(format!("node {id}: {detail}"));
                }
                if !fiber.value_is_derived()
                    && !crate::asymmetric::presence_equiv(left.value.as_ref(), right.value.as_ref())
                {
                    return Some(format!("node {id} value differs"));
                }
            } else if !crate::asymmetric::extra_fields_equiv(
                &left.extra_fields,
                &right.extra_fields,
            ) || !crate::asymmetric::presence_equiv(
                left.value.as_ref(),
                right.value.as_ref(),
            ) {
                return Some(format!("node {id} fields differ"));
            }
        } else if !crate::asymmetric::extra_fields_equiv(&left.extra_fields, &right.extra_fields)
            || !crate::asymmetric::presence_equiv(left.value.as_ref(), right.value.as_ref())
        {
            return Some(format!("node {id} fields differ"));
        }
        a_to_b.insert(id, other_id);
    }
    if a.edge_count() != b.edge_count() {
        return Some(format!(
            "edge count: {} vs {}",
            a.edge_count(),
            b.edge_count()
        ));
    }
    let b_edges_by_key: BTreeMap<&SynthKey, EdgeId> =
        ib.edges.iter().map(|(&id, key)| (key, id)).collect();
    for (id, left) in a.edges_sorted() {
        if !ia.edges.contains_key(&id) && ib.edges.contains_key(&id) {
            return Some(format!("source edge {id} is synthesized on the right"));
        }
        let other_id = ia
            .edges
            .get(&id)
            .and_then(|key| b_edges_by_key.get(key).copied())
            .unwrap_or(id);
        let Some(right) = b.edge(other_id) else {
            return Some(format!("edge {id} is absent on the right"));
        };
        if a_to_b.get(&left.src).copied().unwrap_or(left.src) != right.src
            || a_to_b.get(&left.tgt).copied().unwrap_or(left.tgt) != right.tgt
            || left.edge != right.edge
            || left.position != right.position
        {
            return Some(format!("edge {id} differs"));
        }
    }
    None
}

fn synth_map_is_injective<K: Ord>(map: &BTreeMap<K, SynthKey>) -> bool {
    map.values().collect::<BTreeSet<_>>().len() == map.len()
}

fn graph_synth_index_coherent(instance: &GInstance, index: &SynthIndex) -> bool {
    synth_map_is_injective(&index.nodes)
        && synth_map_is_injective(&index.edges)
        && index.nodes.keys().all(|id| instance.node(*id).is_some())
        && index.edges.keys().all(|id| instance.edge(*id).is_some())
}

fn tree_synth_index_coherent(instance: &WInstance, index: &SynthIndex) -> bool {
    let Ok((graph, _)) = GInstance::from_wtype_embedded(instance, None) else {
        return false;
    };
    graph_synth_index_coherent(&graph, index)
}

fn graph_complement_divergence(a: &GraphComplement, b: &GraphComplement) -> Option<String> {
    if !synth_map_is_injective(&a.synthesized_nodes)
        || !synth_map_is_injective(&a.synthesized_edges)
        || !synth_map_is_injective(&b.synthesized_nodes)
        || !synth_map_is_injective(&b.synthesized_edges)
    {
        return Some("synthesized metadata is incoherent".to_owned());
    }
    if !node_btree_equiv(&a.dropped_nodes, &b.dropped_nodes) {
        return Some("dropped_nodes differ".to_owned());
    }
    if a.dropped_edges != b.dropped_edges {
        return Some("dropped_edges differ".to_owned());
    }
    if a.original_edges != b.original_edges {
        return Some("original_edges differ".to_owned());
    }
    if a.original_anchors != b.original_anchors {
        return Some("original_anchors differ".to_owned());
    }
    if !btree_value_maps_equiv(&a.original_extra_fields, &b.original_extra_fields) {
        return Some("original_extra_fields differ".to_owned());
    }
    if !btree_presence_maps_equiv(&a.original_values, &b.original_values) {
        return Some("original_values differ".to_owned());
    }
    let a_nodes: BTreeSet<&SynthKey> = a.synthesized_nodes.values().collect();
    let b_nodes: BTreeSet<&SynthKey> = b.synthesized_nodes.values().collect();
    if a_nodes != b_nodes {
        return Some("synthesized node keys differ".to_owned());
    }
    let a_edges: BTreeSet<&SynthKey> = a.synthesized_edges.values().collect();
    let b_edges: BTreeSet<&SynthKey> = b.synthesized_edges.values().collect();
    (a_edges != b_edges).then(|| "synthesized edge keys differ".to_owned())
}

fn graph_congruence_witness(
    instance: &GInstance,
    index: &SynthIndex,
) -> Option<(GInstance, SynthIndex)> {
    let node_new = if index.nodes.is_empty() {
        None
    } else {
        fresh_node_id(instance.nodes().keys().copied())
    };
    let edge_new = if index.edges.is_empty() {
        None
    } else {
        fresh_edge_id(instance.edges().keys().map(|id| id.0))
    };
    let node_pair = index.nodes.keys().next().copied().zip(node_new);
    let edge_pair = index.edges.keys().next().copied().zip(edge_new.map(EdgeId));
    if node_pair.is_none() && edge_pair.is_none() {
        return None;
    }
    let mut graph = GInstance::new();
    for node in instance.nodes().values() {
        let mut copy = node.clone();
        if node_pair.is_some_and(|(old, _)| old == copy.id) {
            copy.id = node_pair.map_or(copy.id, |(_, new)| new);
        }
        graph.insert_node(copy).ok()?;
    }
    for (id, edge) in instance.edges_sorted() {
        let copy = GEdge {
            src: node_pair
                .filter(|(old, _)| *old == edge.src)
                .map_or(edge.src, |(_, new)| new),
            tgt: node_pair
                .filter(|(old, _)| *old == edge.tgt)
                .map_or(edge.tgt, |(_, new)| new),
            edge: edge.edge.clone(),
            position: edge.position,
        };
        let new_id = edge_pair
            .filter(|(old, _)| *old == id)
            .map_or(id, |(_, new)| new);
        graph.insert_edge(new_id, copy).ok()?;
    }
    let mut next = index.clone();
    if let Some((old, new)) = node_pair
        && let Some(key) = next.nodes.remove(&old)
    {
        next.nodes.insert(new, key);
    }
    if let Some((old, new)) = edge_pair
        && let Some(key) = next.edges.remove(&old)
    {
        next.edges.insert(new, key);
    }
    Some((graph, next))
}

fn tree_normalized(
    a: &WInstance,
    ia: &SynthIndex,
    b: &WInstance,
    ib: &SynthIndex,
) -> Option<(WInstance, WInstance)> {
    if !tree_synth_index_coherent(a, ia) || !tree_synth_index_coherent(b, ib) {
        return None;
    }
    let a_keys: BTreeSet<&SynthKey> = ia.nodes.values().collect();
    let b_keys: BTreeSet<&SynthKey> = ib.nodes.values().collect();
    if a_keys != b_keys {
        return None;
    }
    let a_edge_keys: BTreeSet<&SynthKey> = ia.edges.values().collect();
    let b_edge_keys: BTreeSet<&SynthKey> = ib.edges.values().collect();
    if a_edge_keys != b_edge_keys {
        return None;
    }
    let b_by_key: BTreeMap<&SynthKey, u32> = ib.nodes.iter().map(|(&id, key)| (key, id)).collect();
    let remap: HashMap<u32, u32> = ia
        .nodes
        .iter()
        .filter_map(|(&a_id, key)| b_by_key.get(key).map(|&b_id| (b_id, a_id)))
        .collect();
    Some((a.clone(), remap_tree(b, &remap)?))
}

fn remap_tree(instance: &WInstance, remap: &HashMap<u32, u32>) -> Option<WInstance> {
    let mut nodes = HashMap::new();
    for node in instance.nodes.values() {
        let mut copy = node.clone();
        copy.id = remap.get(&copy.id).copied().unwrap_or(copy.id);
        if nodes.insert(copy.id, copy).is_some() {
            return None;
        }
    }
    let arcs = instance
        .arcs
        .iter()
        .map(|(parent, child, edge)| {
            (
                remap.get(parent).copied().unwrap_or(*parent),
                remap.get(child).copied().unwrap_or(*child),
                edge.clone(),
            )
        })
        .collect();
    let mut fans = instance.fans.clone();
    for fan in &mut fans {
        fan.parent = remap.get(&fan.parent).copied().unwrap_or(fan.parent);
        for child in fan.children.values_mut() {
            *child = remap.get(child).copied().unwrap_or(*child);
        }
    }
    Some(WInstance::new(
        nodes,
        arcs,
        fans,
        remap.get(&instance.root).copied().unwrap_or(instance.root),
        instance.schema_root.clone(),
    ))
}

fn fresh_node_id(ids: impl Iterator<Item = u32>) -> Option<u32> {
    ids.max().map_or(Some(0), |id| id.checked_add(1))
}

fn fresh_edge_id(ids: impl Iterator<Item = u32>) -> Option<u32> {
    fresh_node_id(ids)
}

fn node_btree_equiv(
    a: &BTreeMap<u32, panproto_inst::Node>,
    b: &BTreeMap<u32, panproto_inst::Node>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(id, node)| {
            b.get(id)
                .is_some_and(|other| crate::asymmetric::node_equiv(node, other))
        })
}

fn btree_value_maps_equiv(
    a: &BTreeMap<u32, BTreeMap<String, panproto_inst::Value>>,
    b: &BTreeMap<u32, BTreeMap<String, panproto_inst::Value>>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(id, fields)| {
            b.get(id).is_some_and(|other| {
                fields.len() == other.len()
                    && fields.iter().all(|(key, value)| {
                        other.get(key).is_some_and(|other_value| {
                            crate::asymmetric::value_equiv(value, other_value)
                        })
                    })
            })
        })
}

fn btree_presence_maps_equiv(
    a: &BTreeMap<u32, Option<panproto_inst::FieldPresence>>,
    b: &BTreeMap<u32, Option<panproto_inst::FieldPresence>>,
) -> bool {
    a.len() == b.len()
        && a.iter().all(|(id, value)| {
            b.get(id).is_some_and(|other| {
                crate::asymmetric::presence_equiv(value.as_ref(), other.as_ref())
            })
        })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use panproto_inst::{
        EdgeId, FieldPresence, GEdge, GInstance, GraphComplement, Node, NodeShape, SynthIndex,
        SynthKey, Value,
    };
    use panproto_schema::Edge;

    use super::{GraphShape, LensShape, TreeShape};
    use crate::Complement;

    fn edge(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    #[test]
    fn tree_synth_index_names_expansion_arcs_from_the_view_embedding() {
        let view = panproto_inst::WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (1, Node::new(1, "middle")),
                (2, Node::new(2, "leaf")),
            ]),
            vec![
                (0, 1, edge("root", "middle", "m")),
                (1, 2, edge("middle", "leaf", "v")),
            ],
            Vec::new(),
            0,
            "root".into(),
        );
        let mut complement = Complement::empty();
        let first = panproto_inst::SynthKey::Expand {
            source_edge: panproto_inst::EdgeId(0),
            hop: 0,
        };
        complement.synthesized_keys.insert(1, first.clone());
        complement.arc_order.push((0, 2));

        let index = TreeShape::synth_index(&view, &complement);
        assert_eq!(index.nodes.get(&1), Some(&first));
        assert_eq!(index.edges.get(&panproto_inst::EdgeId(0)), Some(&first));
        assert_eq!(
            index.edges.get(&panproto_inst::EdgeId(1)),
            Some(&panproto_inst::SynthKey::Expand {
                source_edge: panproto_inst::EdgeId(0),
                hop: 1,
            })
        );
    }

    #[test]
    fn tree_equivalence_ignores_only_cross_parent_arc_interleaving() {
        let branch = edge("root", "branch", "branch");
        let leaf = edge("branch", "leaf", "leaf");
        let nodes = HashMap::from([
            (0, Node::new(0, "root")),
            (1, Node::new(1, "branch")),
            (2, Node::new(2, "branch")),
            (3, Node::new(3, "leaf")),
            (4, Node::new(4, "leaf")),
        ]);
        let left = panproto_inst::WInstance::new(
            nodes.clone(),
            vec![
                (0, 1, branch.clone()),
                (1, 3, leaf.clone()),
                (0, 2, branch.clone()),
                (2, 4, leaf.clone()),
            ],
            Vec::new(),
            0,
            "root".into(),
        );
        let interleaved = panproto_inst::WInstance::new(
            nodes.clone(),
            vec![
                (0, 1, branch.clone()),
                (0, 2, branch.clone()),
                (1, 3, leaf.clone()),
                (2, 4, leaf),
            ],
            Vec::new(),
            0,
            "root".into(),
        );
        let reordered_siblings = panproto_inst::WInstance::new(
            nodes,
            vec![
                (0, 2, branch.clone()),
                (1, 3, edge("branch", "leaf", "leaf")),
                (0, 1, branch),
                (2, 4, edge("branch", "leaf", "leaf")),
            ],
            Vec::new(),
            0,
            "root".into(),
        );
        let index = SynthIndex::default();

        assert!(TreeShape::equivalent(&left, &index, &interleaved, &index));
        assert!(TreeShape::equivalent(&interleaved, &index, &left, &index));
        assert!(!TreeShape::equivalent(
            &left,
            &index,
            &reordered_siblings,
            &index
        ));
    }

    #[test]
    fn graph_equivalence_observes_synthesized_edge_position() {
        let label = edge("root", "leaf", "leaf");
        let mut left_edge = GEdge::new(0, 1, label.clone());
        left_edge.position = Some(3);
        let left = GInstance::new()
            .with_node(Node::new(0, "root"))
            .with_node(Node::new(1, "leaf"))
            .with_edge_id(EdgeId(7), 0, 1, label.clone());
        let mut left = left;
        assert!(left.remove_edge(EdgeId(7)).is_ok());
        assert!(left.insert_edge(EdgeId(7), left_edge).is_ok());

        let mut right_edge = GEdge::new(0, 1, label);
        right_edge.position = Some(4);
        let mut right = GInstance::new()
            .with_node(Node::new(0, "root"))
            .with_node(Node::new(1, "leaf"));
        assert!(right.insert_edge(EdgeId(9), right_edge).is_ok());

        let key = SynthKey::Contract {
            src: 0,
            tgt: 1,
            edge: edge("root", "leaf", "leaf"),
        };
        let left_index = SynthIndex {
            nodes: BTreeMap::new(),
            edges: std::iter::once((EdgeId(7), key.clone())).collect(),
        };
        let right_index = SynthIndex {
            nodes: BTreeMap::new(),
            edges: std::iter::once((EdgeId(9), key)).collect(),
        };
        assert!(!GraphShape::equivalent(
            &left,
            &left_index,
            &right,
            &right_index,
        ));
    }

    #[test]
    fn graph_equivalence_rejects_mismatched_keys_at_the_same_raw_ids() {
        let label = edge("root", "leaf", "leaf");
        let graph = GInstance::new()
            .with_node(Node::new(0, "root"))
            .with_node(Node::new(1, "leaf"))
            .with_edge_id(EdgeId(7), 0, 1, label.clone());
        let node_key = SynthKey::Expand {
            source_edge: EdgeId(1),
            hop: 0,
        };
        let other_node_key = SynthKey::Expand {
            source_edge: EdgeId(2),
            hop: 0,
        };
        let edge_key = SynthKey::Contract {
            src: 0,
            tgt: 1,
            edge: label,
        };
        let other_edge_key = SynthKey::Expand {
            source_edge: EdgeId(3),
            hop: 1,
        };
        let baseline = SynthIndex {
            nodes: BTreeMap::from([(0, node_key.clone())]),
            edges: BTreeMap::from([(EdgeId(7), edge_key.clone())]),
        };
        let mismatched_node = SynthIndex {
            nodes: BTreeMap::from([(0, other_node_key)]),
            edges: BTreeMap::from([(EdgeId(7), edge_key)]),
        };
        let mismatched_edge = SynthIndex {
            nodes: BTreeMap::from([(0, node_key)]),
            edges: BTreeMap::from([(EdgeId(7), other_edge_key)]),
        };

        assert!(!GraphShape::equivalent(
            &graph,
            &baseline,
            &graph,
            &mismatched_node,
        ));
        assert!(!GraphShape::equivalent(
            &graph,
            &baseline,
            &graph,
            &mismatched_edge,
        ));
    }

    #[test]
    fn graph_equivalence_rejects_noninjective_synthesized_indices_symmetrically() {
        let graph = GInstance::new()
            .with_node(Node::new(1, "leaf"))
            .with_node(Node::new(2, "leaf"));
        let key = SynthKey::Expand {
            source_edge: EdgeId(4),
            hop: 0,
        };
        let duplicated = SynthIndex {
            nodes: BTreeMap::from([(1, key.clone()), (2, key.clone())]),
            edges: BTreeMap::new(),
        };
        let coherent = SynthIndex {
            nodes: BTreeMap::from([(1, key)]),
            edges: BTreeMap::new(),
        };

        assert!(!GraphShape::equivalent(
            &graph,
            &duplicated,
            &graph,
            &coherent,
        ));
        assert!(!GraphShape::equivalent(
            &graph,
            &coherent,
            &graph,
            &duplicated,
        ));
    }

    #[test]
    fn graph_complement_equivalence_rejects_noninjective_synthesis_metadata() {
        let key = SynthKey::Expand {
            source_edge: EdgeId(4),
            hop: 0,
        };
        let mut duplicated = GraphComplement::empty();
        duplicated.synthesized_nodes = BTreeMap::from([(1, key.clone()), (2, key.clone())]);
        let mut coherent = GraphComplement::empty();
        coherent.synthesized_nodes = BTreeMap::from([(1, key)]);

        assert!(GraphShape::complement_divergence(&duplicated, &coherent).is_some());
        assert!(GraphShape::complement_divergence(&coherent, &duplicated).is_some());
    }

    #[test]
    fn synthesized_indices_exclude_stale_complement_entries() {
        let label = edge("root", "leaf", "leaf");
        let graph = GInstance::new()
            .with_node(Node::new(0, "root"))
            .with_node(Node::new(1, "leaf"))
            .with_edge_id(EdgeId(7), 0, 1, label.clone());
        let node_key = SynthKey::Expand {
            source_edge: EdgeId(1),
            hop: 0,
        };
        let stale_node_key = SynthKey::Expand {
            source_edge: EdgeId(2),
            hop: 0,
        };
        let edge_key = SynthKey::Contract {
            src: 0,
            tgt: 1,
            edge: label.clone(),
        };
        let stale_edge_key = SynthKey::Expand {
            source_edge: EdgeId(3),
            hop: 1,
        };
        let mut graph_complement = GraphComplement::default();
        graph_complement
            .synthesized_nodes
            .insert(0, node_key.clone());
        graph_complement
            .synthesized_nodes
            .insert(99, stale_node_key.clone());
        graph_complement
            .synthesized_edges
            .insert(EdgeId(7), edge_key.clone());
        graph_complement
            .synthesized_edges
            .insert(EdgeId(8), stale_edge_key);
        let graph_index = GraphShape::synth_index(&graph, &graph_complement);
        assert_eq!(graph_index.nodes, BTreeMap::from([(0, node_key.clone())]));
        assert_eq!(graph_index.edges, BTreeMap::from([(EdgeId(7), edge_key)]));

        let tree = panproto_inst::WInstance::new(
            HashMap::from([(0, Node::new(0, "root")), (1, Node::new(1, "leaf"))]),
            vec![(0, 1, label)],
            Vec::new(),
            0,
            "root".into(),
        );
        let mut tree_complement = Complement::default();
        tree_complement.synthesized_keys.insert(1, node_key.clone());
        tree_complement.synthesized_keys.insert(99, stale_node_key);
        assert_eq!(
            TreeShape::synth_index(&tree, &tree_complement).nodes,
            BTreeMap::from([(1, node_key)])
        );
    }

    #[test]
    fn tree_equivalence_rejects_mismatched_edge_keys_at_the_same_raw_ids() {
        let label = edge("root", "leaf", "leaf");
        let tree = panproto_inst::WInstance::new(
            HashMap::from([(0, Node::new(0, "root")), (1, Node::new(1, "leaf"))]),
            vec![(0, 1, label.clone())],
            Vec::new(),
            0,
            "root".into(),
        );
        let node_key = SynthKey::Expand {
            source_edge: EdgeId(1),
            hop: 0,
        };
        let baseline = SynthIndex {
            nodes: BTreeMap::from([(1, node_key.clone())]),
            edges: BTreeMap::from([(
                EdgeId(0),
                SynthKey::Contract {
                    src: 0,
                    tgt: 1,
                    edge: label,
                },
            )]),
        };
        let mismatched = SynthIndex {
            nodes: BTreeMap::from([(1, node_key)]),
            edges: BTreeMap::from([(
                EdgeId(0),
                SynthKey::Expand {
                    source_edge: EdgeId(4),
                    hop: 1,
                },
            )]),
        };

        assert!(!TreeShape::equivalent(&tree, &baseline, &tree, &mismatched,));
    }

    #[test]
    fn tree_equivalence_rejects_noninjective_synthesized_indices_symmetrically() {
        let tree = panproto_inst::WInstance::new(
            HashMap::from([
                (0, Node::new(0, "root")),
                (1, Node::new(1, "leaf")),
                (2, Node::new(2, "leaf")),
            ]),
            vec![
                (0, 1, edge("root", "leaf", "left")),
                (0, 2, edge("root", "leaf", "right")),
            ],
            Vec::new(),
            0,
            "root".into(),
        );
        let key = SynthKey::Expand {
            source_edge: EdgeId(4),
            hop: 0,
        };
        let duplicated = SynthIndex {
            nodes: BTreeMap::from([(1, key.clone()), (2, key.clone())]),
            edges: BTreeMap::new(),
        };
        let coherent = SynthIndex {
            nodes: BTreeMap::from([(1, key)]),
            edges: BTreeMap::new(),
        };

        assert!(!TreeShape::equivalent(&tree, &duplicated, &tree, &coherent,));
        assert!(!TreeShape::equivalent(&tree, &coherent, &tree, &duplicated,));
    }

    #[test]
    fn tree_equivalence_is_strict_on_full_node_metadata() {
        let mut node = Node::new(0, "root")
            .with_value(FieldPresence::Present(Value::Float(f64::NAN)))
            .with_shape(NodeShape::List);
        node.discriminator = Some("tag".into());
        node.position = Some(7);
        node.annotations
            .insert("source".to_owned(), Value::Str("fixture".to_owned()));
        let base = panproto_inst::WInstance::new(
            HashMap::from([(0, node)]),
            Vec::new(),
            Vec::new(),
            0,
            "root".into(),
        );
        let index = panproto_inst::SynthIndex::default();
        assert!(TreeShape::equivalent(&base, &index, &base, &index));

        let mut changed = base.clone();
        if let Some(node) = changed.nodes.get_mut(&0) {
            node.position = Some(8);
        }
        assert!(!TreeShape::equivalent(&base, &index, &changed, &index));

        let mut changed = base.clone();
        if let Some(node) = changed.nodes.get_mut(&0) {
            node.annotations.clear();
        }
        assert!(!TreeShape::equivalent(&base, &index, &changed, &index));
    }

    fn complement_with_synthesized_id(id: u32) -> Complement {
        let key = SynthKey::Expand {
            source_edge: EdgeId(4),
            hop: 1,
        };
        let label = edge("root", "leaf", "leaf");
        let mut complement = Complement::empty();
        complement.synthesized_keys.insert(id, key);
        complement.synthesized_nodes.insert(id);
        complement.dropped_nodes.insert(id, Node::new(id, "leaf"));
        complement.dropped_arcs.push((0, id, label.clone()));
        complement
            .dropped_fans
            .push(panproto_inst::Fan::new("fan", id).with_child("child", id));
        complement
            .contraction_choices
            .insert((0, id), label.clone());
        complement.original_parent.insert(id, 0);
        complement
            .original_extra_fields
            .insert(id, HashMap::from([("payload".to_owned(), Value::Int(7))]));
        complement.arc_edges.insert((0, id), label);
        complement.arc_order = vec![(0, id)];
        complement.original_values.insert(
            id,
            Some(FieldPresence::Present(Value::Str("original".to_owned()))),
        );
        complement.original_anchors.insert(id, "leaf".into());
        complement
    }

    #[test]
    fn tree_complement_comparison_accepts_a_coherent_synthesized_rekey() {
        let left = complement_with_synthesized_id(7);
        let right = complement_with_synthesized_id(19);

        assert_eq!(TreeShape::complement_divergence(&left, &right), None);
    }

    #[test]
    fn tree_complement_comparison_rejects_incoherent_synthesis_metadata() {
        let key = SynthKey::Expand {
            source_edge: EdgeId(4),
            hop: 1,
        };
        let mut orphan_node = Complement::empty();
        orphan_node.synthesized_nodes.insert(7);
        assert_eq!(
            TreeShape::complement_divergence(&orphan_node, &orphan_node).as_deref(),
            Some("synthesized metadata is incoherent")
        );

        let mut orphan_key = Complement::empty();
        orphan_key.synthesized_keys.insert(7, key);
        assert_eq!(
            TreeShape::complement_divergence(&orphan_key, &orphan_key).as_deref(),
            Some("synthesized metadata is incoherent")
        );
    }

    #[test]
    fn tree_complement_comparison_observes_contraction_choices_used_by_put() {
        let schema = crate::tests::three_node_schema();
        let lens = crate::tests::identity_lens(&schema);
        let mut view = crate::tests::three_node_instance();
        let Some(node) = view.nodes.get_mut(&2) else {
            panic!("fixture has node 2");
        };
        node.value = None;
        let Some((parent, child, expected)) =
            view.arcs.iter().find(|(_, child, _)| *child == 2).cloned()
        else {
            panic!("fixture has an arc to node 2");
        };
        let mut accepted = Complement::empty();
        accepted.synthesized_nodes.insert(child);
        accepted.synthesized_keys.insert(
            child,
            SynthKey::Expand {
                source_edge: EdgeId(0),
                hop: 0,
            },
        );
        accepted
            .contraction_choices
            .insert((parent, child), expected.clone());

        let mut changed = accepted.clone();
        let mut unexpected = expected;
        unexpected.name = Some("other".into());
        changed
            .contraction_choices
            .insert((parent, child), unexpected);

        assert_eq!(
            TreeShape::complement_divergence(&accepted, &changed).as_deref(),
            Some("contraction_choices differ")
        );
        let accepted_errors = crate::asymmetric::check_put_domain(&lens, &view, &accepted)
            .err()
            .unwrap_or_default();
        assert!(!accepted_errors.iter().any(|error| {
            matches!(
                error,
                panproto_inst::PutDomainError::SynthesizedEdited { .. }
            )
        }));
        let Err(changed_errors) = crate::asymmetric::check_put_domain(&lens, &view, &changed)
        else {
            panic!("the changed contraction choice must edit a synthesized arc");
        };
        assert!(changed_errors.iter().any(|error| {
            matches!(
                error,
                panproto_inst::PutDomainError::SynthesizedEdited { .. }
            )
        }));
    }
}
