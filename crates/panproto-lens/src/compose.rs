//! Lens composition.
//!
//! Lenses compose in the category of lens paths ([`LensPath`]): a path is
//! the sequence of the lenses it was built from, and [`compose`] puts two
//! lenses in sequence. Whether two adjacent stages may run as one fused
//! lens is decided by each shape's fold over a path; this module states the
//! tree fold's fusion rule.

use std::collections::{HashMap, HashSet};

use panproto_expr::Expr;
use panproto_gat::Name;
use panproto_inst::contraction_keys::{compose_contraction_keys, translate_edge, translate_vertex};
use panproto_inst::{CompiledMigration, ContractionMode, EdgeFate, RestrictPlan};
use panproto_mig::{OnMissing, compose_relabeling};
use panproto_schema::{Edge, Schema};

use crate::Lens;
use crate::error::LensError;
use crate::lens_path::LensPath;

/// Compose two lenses: the path that runs `l1` and then `l2`, from
/// `l1.src_schema` to `l2.tgt_schema`.
///
/// The path keeps both lenses as its stages and never fuses them; its tree
/// fold ([`LensPath::tree_normal_form`]) runs them as one fused lens exactly
/// when the tree fusion rule admits the pair, and
/// [`LensPath::into_single`] gives that lens to a consumer that needs one.
///
/// # Errors
///
/// Returns `LensError::CompositionMismatch` if the path boundary of `l1`'s
/// target differs from the path boundary of `l2`'s source.
pub fn compose(l1: &Lens, l2: &Lens) -> Result<LensPath, LensError> {
    LensPath::single(duplicate(l1)).then(&LensPath::single(duplicate(l2)))
}

/// The fused lens of `l1` then `l2`, whose schemas are known to meet.
///
/// Its edge map sends every source edge whose endpoints survive both lenses
/// to the edge the two lenses give its arcs in sequence, so that an edge
/// either lens maps implicitly (same kind and label across remapped
/// endpoints) is carried through the other lens's explicit map.
fn fuse(l1: &Lens, l2: &Lens) -> Result<Lens, LensError> {
    let mut compiled = compose_compiled_migrations(&l1.compiled, &l2.compiled, &l1.src_schema)?;
    compiled.edge_remap = l1
        .src_schema
        .edges
        .keys()
        .filter_map(|edge| sequential_edge_image(l1, l2, edge).map(|image| (edge.clone(), image)))
        .collect();
    Ok(Lens {
        compiled,
        src_schema: l1.src_schema.clone(),
        tgt_schema: l2.tgt_schema.clone(),
    })
}

/// The edge `get` through `lens` gives a direct arc along the source edge
/// `edge` when both its endpoints survive, or `None` when it drops the arc.
///
/// This is the direct-arc rule of the tree restrict: the explicit edge map,
/// else the target edge of the same kind and label between the remapped
/// endpoints, else the resolver's entry for the remapped endpoints when the
/// source has no parallel edge.
fn direct_edge_image(lens: &Lens, edge: &Edge) -> Option<Edge> {
    if !crate::asymmetric::vertex_survives(lens, &edge.src)
        || !crate::asymmetric::vertex_survives(lens, &edge.tgt)
    {
        return None;
    }
    if let Some(mapped) = lens.compiled.edge_remap.get(edge) {
        return Some(mapped.clone());
    }
    let remap = |v: &Name| lens.compiled.vertex_remap.get(v).unwrap_or(v).clone();
    let (src, tgt) = (remap(&edge.src), remap(&edge.tgt));
    if let Some(carried) = lens
        .tgt_schema
        .edges_between(src.as_str(), tgt.as_str())
        .iter()
        .find(|candidate| candidate.kind == edge.kind && candidate.name == edge.name)
    {
        return Some(carried.clone());
    }
    if lens
        .src_schema
        .edges_between(edge.src.as_str(), edge.tgt.as_str())
        .len()
        == 1
    {
        return lens.compiled.resolver.get(&(src, tgt)).cloned();
    }
    None
}

/// The edge `l1` then `l2` give a direct arc along the source edge `edge`,
/// or `None` when either lens drops it.
fn sequential_edge_image(l1: &Lens, l2: &Lens, edge: &Edge) -> Option<Edge> {
    direct_edge_image(l1, edge).and_then(|middle| direct_edge_image(l2, &middle))
}

/// Check that `left` (the target schema of the earlier stage) is `right`
/// (the source schema of the later one): the same protocol and the same
/// vertex, edge and hyper-edge sets.
///
/// # Errors
///
/// [`LensError::CompositionMismatch`] when they differ.
pub(crate) fn check_boundary(left: &Schema, right: &Schema) -> Result<(), LensError> {
    if left.vertex_count() != right.vertex_count() || left.protocol != right.protocol {
        return Err(LensError::CompositionMismatch);
    }
    // The middle boundary must agree not only on vertices but on the
    // relational structure carried over them: the edge set and the
    // hyper-edge set. Two schemas with identical vertices but a differing
    // edge, edge kind, or fan would compose into a path whose declared
    // intermediate schema does not exist, silently dropping or misrouting
    // arcs. Compare the vertex, edge, and hyper-edge key sets exactly.
    let keys = |schema: &Schema| {
        (
            schema
                .vertices
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            schema
                .edges
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            schema
                .hyper_edges
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
        )
    };
    if keys(left) != keys(right) {
        return Err(LensError::CompositionMismatch);
    }
    Ok(())
}

/// The tree fusion rule returns the fused lens for `l1` followed by `l2` when
/// the fused execution is observationally equivalent to the pair in sequence.
/// It returns `None` when the pair must remain sequential. The schemas of the
/// pair are known to meet.
///
/// The pair is fused only when all of the following hold:
///
/// 1. neither lens nor the composite contracts a path: no surviving source
///    vertex reaches a surviving vertex through a vertex that is dropped or
///    survives only under a predicate, and each of the three compiled
///    plans has no contraction pair;
/// 2. the two compiled migrations compose;
/// 3. the composite agrees with the pair on every source vertex and edge.
///    A source vertex survives the composite exactly when it survives `l1`
///    and its middle image survives `l2`, and a surviving vertex has the same
///    final image in both executions. This check is needed because the
///    migration representation stores surviving target names: a remap onto
///    one of those names could otherwise resurrect a different source vertex
///    that `l1` dropped, while eliding a remap can retain the wrong image for
///    an isolated vertex. For edges, every explicit image joins the images of
///    its endpoints and is an edge of the target (the endpoint check), and
///    the direct-arc rule of the tree restrict, run on the composite, gives
///    every source edge the fate obtained by running `l1`'s fate and then
///    `l2`'s. This edge check is needed because a tree migration states a
///    drop only by leaving an edge out of its edge map: when the pair drops
///    an edge whose kind and label the target still carries between the
///    endpoints' images, the composite would carry the arc there implicitly;
/// 4. `l1` is a complement-`Unit` bijection, which also excludes source and
///    target validation obligations, and every target carrier of `l2` is
///    reached from its source.
///    In addition, both structural plans are identities on carriers before
///    value-layer evaluation; the second layer may subsequently change or
///    remove nodes. Under that structural identity requirement,
///    reverse-fibre injectivity is automatic; the separate coverage check
///    rules out only target-only carriers. An operational identity still checks
///    that its input lies in its put domain, so deleting the sole identity
///    beside a nonidentity stage could erase that intermediate check.
///    Identity in this sense also requires
///    every raw `vertex_remap` entry to be a self-map, including entries whose
///    source lies outside the declared source schema. This condition makes the
///    identity-extended partial inverses compose even on a view edit whose
///    anchor or edge lies outside the declared target schema. The first-stage
///    restrictions preserve staged error precedence: if the second atomic put
///    refuses, fusion cannot report an independent first-stage residue or
///    validation error before it. Put also computes reverse vertex and edge
///    fibres. Target coverage prevents the atomic reverse from refusing an edit
///    at a target-only carrier before the earlier stage runs while the fused
///    reverse reports an earlier-stage residue collision instead;
/// 5. the second stage's representation of the middle schema has no
///    required-cell or key-uniqueness declaration. The first-stage
///    representation is already covered by its isomorphism classification in
///    condition 4. Each atomic put validates its restored source schema;
///    fusion would otherwise skip a validation obligation that exists only at
///    the middle boundary;
/// 6. the plans of `l1`, of `l2` and of the composite are all empty: none of
///    the three contracts a path or expands an arc into synthesized nodes.
///    The composite's plan alone does not suffice: when `l1` expands an arc
///    into a path through a synthesized node and `l2` drops the arc's
///    child, the composite carries no expansion, yet the pair still
///    synthesizes the intermediate node and `l2` may keep it;
/// 7. `l1` carries no value layer, as follows already from its
///    complement-`Unit` classification. If `l2` carries one, `l1` is a
///    structural identity on carriers in this same sense, so `l2` reads
///    the same pre-layer instance in both. Every effective value-layer entry
///    of `l2` is keyed by a declared middle carrier: migration composition
///    intentionally drops entries whose anchors have no first-stage
///    preimage, while an atomic stage can still observe a raw outside-schema
///    entry on an edited node;
/// 8. neither lens retargets a fan, and none of the four schema objects that
///    represent the three boundaries declares a hyper-edge. A fan identity is
///    operational rather than complement-`Unit`: its first stage may retain a
///    noncanonical raw fan label that the second stage normalizes. Collapsing
///    the two complements then loses that retained label, even when all four
///    declarations are equal;
/// 9. the restrict plan compiles for `l1`, for `l2` and for the composite,
///    and every edge fate of each is to keep or to drop the edge. A tree
///    `get` compiles its whole plan before it reads the instance, so a stage
///    whose plan fails refuses every source, and the composite's plan does
///    not see that refusal.
///
/// Under these conditions the fused lens and the pair in sequence are
/// observationally equivalent: their views agree, so do the sources they
/// write back, and one refuses exactly when the other does, with the same
/// error. The categorical discrete-opfibration result motivates the shape of
/// these restrictions, but the implementation establishes soundness through
/// the operational predicates above rather than identifying any one predicate
/// with that categorical condition.
pub(crate) fn tree_fusion(l1: &Lens, l2: &Lens) -> Option<Lens> {
    // Condition 1 for the two lenses, before their migrations are composed.
    if !contraction_free(l1) || !contraction_free(l2) {
        return None;
    }
    // Condition 4: a lossy first stage contributes residue and independent
    // put-domain failures. A fused complement cannot retain the sequential
    // right-to-left error precedence without stage provenance.
    if !l1.is_isomorphism() {
        return None;
    }
    // Condition 8.
    if !l1.compiled.hyper_resolver.is_empty()
        || !l2.compiled.hyper_resolver.is_empty()
        || !l1.src_schema.hyper_edges.is_empty()
        || !l1.tgt_schema.hyper_edges.is_empty()
        || !l2.src_schema.hyper_edges.is_empty()
        || !l2.tgt_schema.hyper_edges.is_empty()
    {
        return None;
    }
    // Condition 4.
    if !target_carriers_covered(l2) {
        return None;
    }
    // Condition 5.
    if !put_domain_is_trivial(&l2.src_schema) {
        return None;
    }
    // Condition 2.
    let composite = fuse(l1, l2).ok()?;
    // Condition 9, and the compiled half of conditions 1 and 6.
    let plans = [plan_of(l1)?, plan_of(l2)?, plan_of(&composite)?];
    if !structural_identity_on_carriers(l1, &plans[0])
        || !structural_identity_on_carriers(l2, &plans[1])
    {
        return None;
    }
    // Condition 3, and the structural half of conditions 1 and 6.
    if !vertex_images_agree(l1, l2, &composite)
        || !edge_image_compiles(l1, l2, &composite)
        || !plan_is_empty(l1)
        || !plan_is_empty(l2)
        || !plan_is_empty(&composite)
    {
        return None;
    }
    // Condition 7.
    if !value_layers_compose(l1, &plans[0], l2, &plans[1]) {
        return None;
    }
    Some(composite)
}

/// Condition 5 of [`tree_fusion`]: the intermediate put performs no
/// required-cell or uniqueness check that the fused put would skip.
fn put_domain_is_trivial(schema: &Schema) -> bool {
    schema.required.values().all(Vec::is_empty)
        && schema.declarations.keys.values().all(Vec::is_empty)
}

/// Condition 4 of [`tree_fusion`]: every target vertex and direct target edge
/// has a source preimage through the second stage. Condition 4 separately
/// requires the stage to be a structural identity on carriers, which already
/// makes these reverse fibres injective.
fn target_carriers_covered(lens: &Lens) -> bool {
    let vertices = lens
        .src_schema
        .vertices
        .keys()
        .filter(|source| crate::asymmetric::vertex_survives(lens, source))
        .map(|source| lens.compiled.vertex_remap.get(source).unwrap_or(source))
        .collect::<HashSet<_>>();
    if !lens
        .tgt_schema
        .vertices
        .keys()
        .all(|target| vertices.contains(target))
    {
        return false;
    }

    let edges = lens
        .src_schema
        .edges
        .keys()
        .filter_map(|source| direct_edge_image(lens, source))
        .collect::<HashSet<_>>();
    lens.tgt_schema
        .edges
        .keys()
        .all(|target| edges.contains(target))
}

/// Condition 3 of [`tree_fusion`] for vertices: the composite preserves both
/// the sequential survival decision and each surviving image in the source
/// frame.
fn vertex_images_agree(l1: &Lens, l2: &Lens, composite: &Lens) -> bool {
    l1.src_schema.vertices.keys().all(|source| {
        let middle = l1.compiled.vertex_remap.get(source).unwrap_or(source);
        let sequential = crate::asymmetric::vertex_survives(l1, source)
            && crate::asymmetric::vertex_survives(l2, middle);
        let candidate = crate::asymmetric::vertex_survives(composite, source);
        candidate == sequential
            && (!sequential
                || composite
                    .compiled
                    .vertex_remap
                    .get(source)
                    .unwrap_or(source)
                    == l2.compiled.vertex_remap.get(middle).unwrap_or(middle))
    })
}

/// The restrict plan of `lens` when it compiles with no contraction pair
/// and only fates that keep or drop an edge, as conditions 1, 4 and 7 of
/// [`tree_fusion`] require.
fn plan_of(lens: &Lens) -> Option<RestrictPlan> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).ok()?;
    let fates_keep_or_drop = plan
        .edge_image
        .fates
        .values()
        .all(|fate| matches!(fate, EdgeFate::Keep(_) | EdgeFate::Drop));
    (plan.contraction.mode() == ContractionMode::None && fates_keep_or_drop).then_some(plan)
}

/// Condition 3 of [`tree_fusion`]: the composite's explicit edge images
/// pass the endpoint check, and the composite gives every direct arc the
/// fate the pair gives it in sequence.
fn edge_image_compiles(l1: &Lens, l2: &Lens, composite: &Lens) -> bool {
    edge_images_coherent(composite) && direct_images_agree(l1, l2, composite)
}

/// Whether the plan of `get` through `lens` is structurally empty, as
/// condition 6 of [`tree_fusion`] requires of each lens and of the
/// composite: it neither contracts a path nor expands an arc into
/// synthesized nodes.
fn plan_is_empty(lens: &Lens) -> bool {
    contraction_free(lens) && lens.compiled.expansion_path.is_empty()
}

/// Whether `lens` carries a value layer: a survival predicate, or a
/// non-empty list of field transforms or op-term assignments.
fn has_value_layer(lens: &Lens) -> bool {
    !lens.compiled.conditional_survival.is_empty()
        || lens
            .compiled
            .field_transforms
            .values()
            .any(|t| !t.is_empty())
        || lens
            .compiled
            .op_term_assignments
            .values()
            .any(|t| !t.is_empty())
}

/// Condition 7 of [`tree_fusion`], with the compiled plan of `l1`: the first
/// stage has no value layer, and a layer on `l2` needs an `l1` that is a
/// structural identity on carriers and whose entries are all keyed by
/// declared middle carriers.
fn value_layers_compose(l1: &Lens, plan1: &RestrictPlan, l2: &Lens, plan2: &RestrictPlan) -> bool {
    let _ = plan2;
    !has_value_layer(l1)
        && (!has_value_layer(l2)
            || (structural_identity_on_carriers(l1, plan1) && value_layer_anchors_declared(l2)))
}

/// Whether every effective value-layer entry is keyed by a declared source
/// carrier. A raw entry outside this carrier remains observable to an atomic
/// stage on an edited outside-schema anchor, but migration composition has no
/// source preimage on which to retain it.
fn value_layer_anchors_declared(lens: &Lens) -> bool {
    let declared = |anchor: &Name| lens.src_schema.vertices.contains_key(anchor);
    lens.compiled.conditional_survival.keys().all(&declared)
        && lens
            .compiled
            .field_transforms
            .iter()
            .filter(|(_, transforms)| !transforms.is_empty())
            .map(|(anchor, _)| anchor)
            .all(&declared)
        && lens
            .compiled
            .op_term_assignments
            .iter()
            .filter(|(_, assignments)| !assignments.is_empty())
            .map(|(anchor, _)| anchor)
            .all(declared)
}

/// Whether the structural plan fixes every carrier before any value layer:
/// every vertex survives with itself as its image, and every edge fate keeps
/// the edge as itself. Conditional survival and field or term transforms are
/// handled separately by [`value_layers_compose`].
fn structural_identity_on_carriers(lens: &Lens, plan: &RestrictPlan) -> bool {
    lens.compiled
        .vertex_remap
        .iter()
        .all(|(source, target)| source == target)
        && lens.src_schema.vertices.keys().all(|v| {
            lens.compiled.vertex_remap.get(v).unwrap_or(v) == v
                && crate::asymmetric::vertex_survives(lens, v)
        })
        && plan
            .edge_image
            .fates
            .iter()
            .all(|(source, fate)| matches!(fate, EdgeFate::Keep(image) if image == source))
}

/// Whether the fused lens gives every direct arc the edge the pair gives it
/// in sequence. The fused edge map records every edge the pair keeps; an
/// edge the pair drops must also be dropped by the fused lens, which a
/// migration cannot state when the target has an edge of the same kind and
/// label between the endpoints' images.
fn direct_images_agree(l1: &Lens, l2: &Lens, composite: &Lens) -> bool {
    l1.src_schema
        .edges
        .keys()
        .all(|edge| direct_edge_image(composite, edge) == sequential_edge_image(l1, l2, edge))
}

/// A copy of `lens`.
pub(crate) fn duplicate(lens: &Lens) -> Lens {
    Lens {
        compiled: lens.compiled.clone(),
        src_schema: lens.src_schema.clone(),
        tgt_schema: lens.tgt_schema.clone(),
    }
}

/// Whether `get` through `lens` contracts no path: no surviving source
/// vertex reaches another (or itself) in the source schema through a
/// vertex some of whose nodes are dropped, that is a vertex that does not
/// survive or survives only under a predicate.
fn contraction_free(lens: &Lens) -> bool {
    let schema = &lens.src_schema;
    let endpoint = |v: &Name| crate::asymmetric::vertex_survives(lens, v);
    let interior = |v: &Name| {
        !crate::asymmetric::vertex_survives(lens, v)
            || lens.compiled.conditional_survival.contains_key(v)
    };
    for a in schema.vertices.keys().filter(|v| endpoint(v)) {
        let mut seen: HashSet<&Name> = HashSet::new();
        let mut frontier: Vec<&Name> = schema
            .outgoing_edges(a)
            .iter()
            .map(|e| &e.tgt)
            .filter(|v| interior(v))
            .collect();
        while let Some(x) = frontier.pop() {
            if !seen.insert(x) {
                continue;
            }
            for e in schema.outgoing_edges(x) {
                if endpoint(&e.tgt) {
                    return false;
                }
                if interior(&e.tgt) {
                    frontier.push(&e.tgt);
                }
            }
        }
    }
    true
}

/// Whether every explicit edge image of `lens` whose source endpoints
/// survive joins the images of those endpoints and is an edge of the
/// target schema.
fn edge_images_coherent(lens: &Lens) -> bool {
    let image_of = |v: &Name| lens.compiled.vertex_remap.get(v).unwrap_or(v).clone();
    lens.compiled.edge_remap.iter().all(|(source, image)| {
        let survives = crate::asymmetric::vertex_survives(lens, &source.src)
            && crate::asymmetric::vertex_survives(lens, &source.tgt);
        !survives
            || (image.src == image_of(&source.src)
                && image.tgt == image_of(&source.tgt)
                && lens.tgt_schema.edges.contains_key(image))
    })
}

/// Compose two compiled migrations.
///
/// The surviving sets are intersected (a vertex/edge must survive both),
/// and remaps are composed (l1's output feeds into l2's input).
///
/// `src_schema` is `m1`'s source schema. `m2`'s per-anchor entries are
/// re-keyed onto every vertex of it that `m1` sends to their anchor, and it
/// tells a dropped `extra_fields` key apart from a surviving child edge of
/// the same name.
///
/// # Errors
///
/// Returns [`LensError::ComposeUnboundField`] when a value-level transform
/// of `m2` reads a field that `m1` drops. The tree fusion predicate treats
/// this as a reason to retain two sequential groups.
pub(crate) fn compose_compiled_migrations(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
    src_schema: &panproto_schema::Schema,
) -> Result<CompiledMigration, LensError> {
    // Both surviving sets are keyed in their migration's target space (the
    // image of its vertex and edge maps), so m1's survivors are carried
    // through m2: a vertex or edge survives the composite when its image
    // under m2 survives m2.
    let surviving_verts: HashSet<Name> = m1
        .surviving_verts
        .iter()
        .filter_map(|v| translate_vertex(m2, v))
        .collect();
    let surviving_edges: HashSet<Edge> = m1
        .surviving_edges
        .iter()
        .filter_map(|e| translate_edge(m2, e))
        .collect();

    // Compose vertex remaps through the shared relabeling kernel: apply m1's
    // remap, then m2's remap, keeping m1's target where m2 does not remap it.
    let mut vertex_remap = compose_relabeling(
        &m1.vertex_remap,
        &m2.vertex_remap,
        OnMissing::KeepIntermediate,
    )
    .map;
    // Also include m2 remaps for vertices m1's remap does not reach.
    // The reached set is collected once: asking `values().any(...)` per
    // entry walks all of m1's remap for every entry of m2's.
    let m1_targets: HashSet<&Name> = m1.vertex_remap.values().collect();
    for (mid, tgt) in &m2.vertex_remap {
        if !m1_targets.contains(mid) {
            vertex_remap
                .entry(mid.clone())
                .or_insert_with(|| tgt.clone());
        }
    }

    // Compose edge remaps through the same kernel.
    let edge_remap: HashMap<Edge, Edge> =
        compose_relabeling(&m1.edge_remap, &m2.edge_remap, OnMissing::KeepIntermediate).map;

    // The resolver and expansion paths are keyed in the target space, so
    // m1's entries are carried through m2 into the composite's target space.
    let (resolver, expansion_path, _dropped) = compose_contraction_keys(m1, m2);

    let hyper_resolver = compose_hyper_resolvers(m1, m2);

    let field_transforms = compose_field_transforms(m1, m2, src_schema)?;
    let op_term_assignments = compose_op_term_assignments(m1, m2, src_schema);
    let conditional_survival = compose_conditional_survival(m1, m2, src_schema);

    Ok(CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap,
        edge_remap,
        resolver,
        hyper_resolver,
        field_transforms,
        conditional_survival,
        op_term_assignments,
        expansion_path,
    })
}

/// Compose two hyper-edge resolvers.
///
/// An entry of `m1` is keyed by a fan shape of `m1`'s source and points
/// into `m1`'s target, which is `m2`'s source, so it is carried through
/// `m2`: the entry `m2` selects for the shape `m1`'s entry produces (the
/// image of its labels under its label map) is applied after it, and the
/// two label maps are composed. An entry of `m2` is kept under its own key
/// when `m1` retargets no shape of its hyper-edge, since fans of that
/// hyper-edge reach `m2` as they are.
///
/// The table selects an entry by the labels of a fan's surviving children,
/// which the composite decides in one pass and the two migrations in two,
/// so the composite gives the pair's fans only when both stages see the
/// same surviving children; [`tree_fusion`] keeps a pair in sequence when
/// either lens retargets a fan.
fn compose_hyper_resolvers(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
) -> panproto_inst::HyperResolverTable {
    let selector = panproto_inst::FanResolver::new(m2);
    let mut result = panproto_inst::HyperResolverTable::default();
    for (key, (target, labels)) in &m1.hyper_resolver {
        let middle = |label: &Name| labels.get(label).unwrap_or(label).clone();
        let image = panproto_inst::canonical_label_shape(key.1.iter().map(&middle));
        let entry = match selector.resolve(target, &image, &image) {
            Some((final_target, final_labels)) => {
                let last = |label: &Name| final_labels.get(label).unwrap_or(label).clone();
                let mut composed: HashMap<Name, Name> = key
                    .1
                    .iter()
                    .chain(labels.keys())
                    .map(|label| (label.clone(), last(&middle(label))))
                    .collect();
                for (label, image) in final_labels {
                    if !labels.contains_key(label) {
                        composed
                            .entry(label.clone())
                            .or_insert_with(|| image.clone());
                    }
                }
                (final_target.clone(), composed)
            }
            None => (target.clone(), labels.clone()),
        };
        result.insert(key.clone(), entry);
    }
    let retargeted: HashSet<&Name> = m1.hyper_resolver.keys().map(|key| &key.0).collect();
    for (key, entry) in &m2.hyper_resolver {
        if !retargeted.contains(&key.0) {
            result.insert(key.clone(), entry.clone());
        }
    }
    result
}

/// The vertices of `m1`'s source that `m1` sends to `anchor`, a vertex of
/// its target: those whose image under `m1.vertex_remap` is `anchor`, and
/// `anchor` itself when it is a source vertex with no entry in the remap and
/// so keeps its name. These are the anchors an `m2` entry on `anchor` is
/// re-keyed onto in the composite. Every one of them is needed: when `m1`
/// merges a renamed vertex onto one that keeps its name, the nodes of both
/// reach `m2` under `anchor`.
fn m1_preimages(
    m1: &CompiledMigration,
    src_schema: &panproto_schema::Schema,
    anchor: &Name,
) -> std::collections::BTreeSet<Name> {
    let mut preimages: std::collections::BTreeSet<Name> = m1
        .vertex_remap
        .iter()
        .filter(|(_, image)| *image == anchor)
        .map(|(source, _)| source.clone())
        .collect();
    if src_schema.vertices.contains_key(anchor) && !m1.vertex_remap.contains_key(anchor) {
        preimages.insert(anchor.clone());
    }
    preimages
}

/// Compose `field_transforms` from two migrations, re-keying each `m2`
/// anchor onto its [`m1_preimages`]. The composed map is keyed by
/// `m1`-source anchors throughout. An `m2`-anchor with no preimage lives
/// only in `m1`'s target space; its transforms cannot be expressed against
/// `m1`'s source and are dropped from the composed map (they would
/// otherwise corrupt the keyspace invariant).
///
/// Both coordinates are conjugated, not just the anchor. `m2`'s entries
/// are written against `m1`'s **output** field names, but the composed
/// list is evaluated in one batch against `m1`'s **source** node, so the
/// expressions' free variables are rewritten along the inverse of `m1`'s
/// renames on that anchor. Without this the composite interprets `m2`'s
/// variables in the wrong frame: `get(m2 ∘ m1)` rejects the expression
/// that `get(m2) ∘ get(m1)` accepts, and accepts one naming a field that
/// does not exist in `m2`'s input schema, so composition is not
/// functorial on value-level transforms.
///
/// The anchor coordinate is conjugated through [`m1_preimages`]; this is
/// the field coordinate's counterpart.
///
/// # Errors
///
/// Returns [`LensError::ComposeUnboundField`] when an `m2` expression
/// reads a field `m1` drops outright, making the two lists unsafe to fuse.
fn compose_field_transforms(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
    src_schema: &panproto_schema::Schema,
) -> Result<HashMap<panproto_gat::Name, Vec<panproto_inst::wtype::FieldTransform>>, LensError> {
    use panproto_gat::Name;
    use panproto_inst::wtype::FieldTransform;

    let mut result = m1.field_transforms.clone();

    // Inject each m2 anchor's entries, conjugated into m1's source frame.
    let mut inject = |m1_anchor: &Name, entries: &[FieldTransform]| -> Result<(), LensError> {
        let renames = field_rename_inverse(m1, m1_anchor);
        let unavailable = unavailable_fields(m1, m1_anchor, src_schema);
        let conjugated = conjugate_transforms(entries, &renames, &unavailable, m1_anchor)?;
        result
            .entry(m1_anchor.clone())
            .or_default()
            .extend(conjugated);
        Ok(())
    };

    for (m2_anchor, m2_entries) in &m2.field_transforms {
        for m1_anchor in m1_preimages(m1, src_schema, m2_anchor) {
            inject(&m1_anchor, m2_entries)?;
        }
    }

    Ok(result)
}

/// The child-scalar renames `m1` performs on `anchor`, inverted: a map
/// from the field name visible in `m1`'s **output** back to the name it
/// has in `m1`'s **source**.
///
/// Only schema edge renames contribute. An edge rename
/// is applied via `edge_remap` when output arcs are materialized, and
/// never reaches the `child_scalars` map that
/// `collect_scalar_child_values` projects into the expression
/// environment, so an `m2` expression naming the new edge name finds
/// nothing under it in the merged batch. That is the frame mismatch this
/// map repairs.
///
/// `FieldTransform::RenameField` deliberately does *not* contribute.
/// It rewrites the `extra_fields` key in place, and `m1`'s entries run
/// ahead of `m2`'s in the merged batch, so by the time `m2`'s expression
/// evaluates the field really is under its new name. Conjugating those
/// as well would rewrite a correct reference back to a name that no
/// longer exists. This asymmetry between the two rename routes is the
/// whole of the defect: one is reflected in the value layer's
/// environment and the other is not.
///
/// Renames nested under a `PathTransform` and renames inside a `Case`
/// branch are not included: the first affects nested keys rather than
/// the top-level bindings `free_vars` reports, and the second is
/// value-dependent, so no static rewrite is sound for it.
///
/// `edge_remap`'s entries are simultaneous, one per source edge, so the
/// map is read off directly rather than folded. A chain of renames across
/// several migrations has already been collapsed into a single entry by
/// `compose_relabeling` before it reaches here, which is what makes
/// `a → b` then `b → c` resolve `c` to `a` in one step.
fn field_rename_inverse(
    m1: &CompiledMigration,
    anchor: &panproto_gat::Name,
) -> HashMap<String, String> {
    let mut origin: HashMap<String, String> = HashMap::new();

    for (src_edge, tgt_edge) in &m1.edge_remap {
        if &src_edge.src != anchor {
            continue;
        }
        let old_field = edge_field_name(src_edge);
        let new_field = edge_field_name(tgt_edge);
        if old_field == new_field {
            continue;
        }
        origin.insert(new_field, old_field);
    }

    origin
}

/// The environment key a child edge contributes, matching
/// `collect_scalar_child_values`: the edge's name when it has one, else
/// its target vertex.
fn edge_field_name(edge: &Edge) -> String {
    edge.name.as_deref().unwrap_or(&edge.tgt).to_string()
}

/// Names that do not exist in `m1`'s output frame at `anchor`, so an `m2`
/// expression reading one is written against the wrong frame.
///
/// Two ways a name goes missing:
///
/// * `DropField` removes an `extra_fields` key outright.
/// * A schema edge rename takes a child-scalar name away, leaving nothing
///   bound under the old name in `m1`'s output.
///
/// Both are then forgiven when the name is put back: by another surviving
/// child edge carrying it in the output frame, or by an `m1` transform
/// writing it (`AddField`, `ComputeField`, `ApplyExpr`, or a `RenameField`
/// target).
///
/// `RenameField`'s *old* key is not reported. It ceases to exist in
/// `m1`'s output, but `m1` and `m2` share one evaluation batch, so a
/// stale reference resolves against whatever `m1` left in `extra_fields`
/// rather than failing; reporting it would reject compositions that run
/// correctly today. `KeepFields` is likewise excluded: it filters
/// `extra_fields` only, so a name it omits may still be bound as a child
/// scalar.
fn unavailable_fields(
    m1: &CompiledMigration,
    anchor: &panproto_gat::Name,
    schema: &panproto_schema::Schema,
) -> std::collections::HashSet<String> {
    use panproto_inst::wtype::FieldTransform;

    let mut unavailable = std::collections::HashSet::new();

    // Child-scalar names an edge rename takes away.
    let outgoing = schema.outgoing.get(anchor);
    let mut output_child_names = std::collections::HashSet::new();
    if let Some(edges) = outgoing {
        for edge in edges {
            if !m1.surviving_edges.contains(edge) {
                continue;
            }
            let out_edge = m1.edge_remap.get(edge).unwrap_or(edge);
            output_child_names.insert(edge_field_name(out_edge));
        }
        for edge in edges {
            if !m1.surviving_edges.contains(edge) {
                continue;
            }
            let source_name = edge_field_name(edge);
            if !output_child_names.contains(&source_name) {
                unavailable.insert(source_name);
            }
        }
    }

    if let Some(transforms) = m1.field_transforms.get(anchor) {
        for transform in transforms {
            match transform {
                FieldTransform::DropField { key } => {
                    if !output_child_names.contains(key) {
                        unavailable.insert(key.clone());
                    }
                }
                FieldTransform::AddField { key, .. }
                | FieldTransform::ComputeField {
                    target_key: key, ..
                }
                | FieldTransform::ApplyExpr { key, .. } => {
                    unavailable.remove(key);
                }
                FieldTransform::RenameField { new_key, .. } => {
                    unavailable.remove(new_key);
                }
                _ => {}
            }
        }
    }

    unavailable
}

/// Rewrite each transform's expressions from `m1`'s output frame into
/// `m1`'s source frame.
fn conjugate_transforms(
    entries: &[panproto_inst::wtype::FieldTransform],
    renames: &HashMap<String, String>,
    unavailable: &std::collections::HashSet<String>,
    anchor: &panproto_gat::Name,
) -> Result<Vec<panproto_inst::wtype::FieldTransform>, LensError> {
    use panproto_inst::wtype::FieldTransform;

    // Keys this m2 list writes itself. A read of one of these resolves at
    // evaluation time even if m1 dropped the same name earlier, because
    // m2's own transform runs first in the merged batch.
    let mut produced: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut next = entry.clone();
        conjugate_transform_exprs(&mut next, renames, unavailable, &produced, anchor)?;
        match entry {
            FieldTransform::ComputeField { target_key, .. } => {
                produced.insert(target_key.clone());
            }
            FieldTransform::AddField { key, .. } | FieldTransform::ApplyExpr { key, .. } => {
                produced.insert(key.clone());
            }
            FieldTransform::RenameField { new_key, .. } => {
                produced.insert(new_key.clone());
            }
            _ => {}
        }
        out.push(next);
    }
    Ok(out)
}

/// Rewrite the expressions carried by one transform, recursing through
/// `PathTransform` and `Case`.
fn conjugate_transform_exprs(
    transform: &mut panproto_inst::wtype::FieldTransform,
    renames: &HashMap<String, String>,
    unavailable: &std::collections::HashSet<String>,
    produced: &std::collections::HashSet<String>,
    anchor: &panproto_gat::Name,
) -> Result<(), LensError> {
    use panproto_inst::wtype::FieldTransform;

    let check = |expr: &panproto_expr::Expr| -> Result<(), LensError> {
        for var in panproto_expr::free_vars(expr) {
            if unavailable.contains(var.as_ref()) && !produced.contains(var.as_ref()) {
                return Err(LensError::ComposeUnboundField {
                    anchor: anchor.to_string(),
                    field: var.to_string(),
                });
            }
        }
        Ok(())
    };
    let rewrite = |expr: &mut panproto_expr::Expr| -> Result<(), LensError> {
        check(expr)?;
        *expr = rename_free_vars(expr, renames);
        Ok(())
    };

    // `ApplyExpr` reads and writes one and the same key. When `m1` renamed
    // that key the two coordinates part company: the read has to happen
    // under `m1`'s source name, because that is what the child-scalar map
    // is keyed by, while the write has to land on the output name, because
    // that is what the remapped arc will be emitted under. Conjugating the
    // key would move both and leave the composite emitting the original
    // value under the new edge name beside the transformed value under the
    // old one; leaving it alone finds nothing to read and silently does
    // nothing. Neither is `ApplyExpr`-shaped, so it becomes the transform
    // that can hold the two coordinates apart: a `ComputeField` writing
    // the output name from an expression reading the source name. The
    // expression's free variables lie within the read key, which the
    // fiber environment binds, and `inverse` and `coercion_class` carry
    // over unchanged. `ApplyExpr` skips a key it cannot find whereas
    // `ComputeField` reports it, which is the direction this codebase
    // already takes for a transform that cannot evaluate.
    if let FieldTransform::ApplyExpr {
        key,
        expr,
        inverse,
        coercion_class,
    } = &*transform
        && renames.contains_key(key.as_str())
    {
        check(expr)?;
        let replacement = FieldTransform::ComputeField {
            target_key: key.clone(),
            expr: rename_free_vars(expr, renames),
            inverse: inverse.as_ref().map(|i| rename_free_vars(i, renames)),
            coercion_class: *coercion_class,
        };
        *transform = replacement;
        return Ok(());
    }

    match transform {
        FieldTransform::ComputeField { expr, inverse, .. } => {
            rewrite(expr)?;
            if let Some(inv) = inverse {
                *inv = rename_free_vars(inv, renames);
            }
        }
        FieldTransform::ApplyExpr { expr, inverse, .. } => {
            // The renamed-key case is rewritten to a `ComputeField` above;
            // reaching here means the key is a fixed point, so only the
            // body needs conjugating.
            rewrite(expr)?;
            if let Some(inv) = inverse {
                *inv = rename_free_vars(inv, renames);
            }
        }
        FieldTransform::Case { branches } => {
            for branch in branches {
                rewrite(&mut branch.predicate)?;
                for inner in &mut branch.transforms {
                    conjugate_transform_exprs(inner, renames, unavailable, produced, anchor)?;
                }
            }
        }
        FieldTransform::PathTransform { inner, .. } => {
            // A nested transform reads the nested map, not the top-level
            // bindings the renames describe, so only its own recursion
            // applies.
            conjugate_transform_exprs(inner, renames, unavailable, produced, anchor)?;
        }
        FieldTransform::RenameField { old_key, .. } => {
            // The key this renames away is named in m1's output frame.
            if let Some(source) = renames.get(old_key.as_str()) {
                old_key.clone_from(source);
            }
        }
        FieldTransform::DropField { key } => {
            if let Some(source) = renames.get(key.as_str()) {
                key.clone_from(source);
            }
        }
        FieldTransform::KeepFields { keys } => {
            for key in keys {
                if let Some(source) = renames.get(key.as_str()) {
                    key.clone_from(source);
                }
            }
        }
        FieldTransform::AddField { .. } | FieldTransform::MapReferences { .. } => {}
    }
    Ok(())
}

/// Simultaneously rename an expression's free variables.
///
/// `panproto_expr::substitute` replaces one name at a time, so applying a
/// map sequentially would compose the replacements: a swap `{a → b,
/// b → a}` would send every `a` to `b` and then straight back to `a`.
/// Routing through fresh placeholders makes the renaming simultaneous,
/// which is what conjugation into another frame requires.
///
/// Both the bare key and its `attrs.`-qualified alias are rewritten,
/// since `build_env_from_extra_fields` binds a field under both.
fn rename_free_vars(expr: &panproto_expr::Expr, renames: &HashMap<String, String>) -> Expr {
    use std::sync::Arc;

    if renames.is_empty() {
        return expr.clone();
    }

    let free = panproto_expr::free_vars(expr);
    // Pair every applicable rename with a placeholder that appears
    // nowhere in the expression or in the rename map.
    let mut pairs: Vec<(String, String, String)> = Vec::new();
    for (index, (visible, source)) in renames.iter().enumerate() {
        for (from, to) in [
            (visible.clone(), source.clone()),
            (format!("attrs.{visible}"), format!("attrs.{source}")),
        ] {
            if !free.iter().any(|v| v.as_ref() == from) {
                continue;
            }
            let mut placeholder = format!("\u{0}compose{index}\u{0}{from}");
            while free.iter().any(|v| v.as_ref() == placeholder.as_str())
                || renames.contains_key(&placeholder)
            {
                placeholder.push('\u{0}');
            }
            pairs.push((from, placeholder, to));
        }
    }

    let mut out = expr.clone();
    for (from, placeholder, _) in &pairs {
        out = panproto_expr::substitute(&out, from, &Expr::Var(Arc::from(placeholder.as_str())));
    }
    for (_, placeholder, to) in &pairs {
        out = panproto_expr::substitute(&out, placeholder, &Expr::Var(Arc::from(to.as_str())));
    }
    out
}

/// Compose `op_term_assignments` from two migrations, re-keying each `m2`
/// anchor onto its [`m1_preimages`] as [`compose_field_transforms`] does.
fn compose_op_term_assignments(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
    src_schema: &panproto_schema::Schema,
) -> HashMap<panproto_gat::Name, Vec<panproto_inst::wtype::TermAssignment>> {
    compose_anchor_keyed_lists(
        m1,
        src_schema,
        &m1.op_term_assignments,
        &m2.op_term_assignments,
    )
}

/// Compose two anchor-keyed transform lists, re-keying each `m2` anchor's
/// entries onto its [`m1_preimages`] in `m1`'s source frame. An
/// `m2`-anchor with no preimage lives only in `m1`'s target space; its
/// entries have no representation in `m1`'s source and are dropped to
/// preserve the keyspace invariant.
fn compose_anchor_keyed_lists<T: Clone>(
    m1: &CompiledMigration,
    src_schema: &panproto_schema::Schema,
    m1_map: &HashMap<panproto_gat::Name, Vec<T>>,
    m2_map: &HashMap<panproto_gat::Name, Vec<T>>,
) -> HashMap<panproto_gat::Name, Vec<T>> {
    let mut result = m1_map.clone();
    for (m2_anchor, m2_entries) in m2_map {
        for m1_anchor in m1_preimages(m1, src_schema, m2_anchor) {
            result
                .entry(m1_anchor)
                .or_default()
                .extend(m2_entries.iter().cloned());
        }
    }
    result
}

/// Compose `conditional_survival` predicates, AND-ing when both exist.
/// Re-keys each `m2` predicate onto every one of its [`m1_preimages`], as
/// [`compose_field_transforms`] does: predicates whose anchor lives only in
/// `m1`'s target space are dropped rather than injected with a foreign
/// key. The AND-conjunction is taken in the composed-source frame, so
/// `m2_pred`'s free variables are interpreted against the schema
/// presented to `m1`'s output (= `m2`'s input).
///
/// Free-variable scope: when `m1` explicitly drops or renames a field
/// on `anchor` whose name is also free in `m2_pred`, the AND-merged
/// predicate would reference a variable that does not exist at
/// evaluation time. We detect that statically: any `m2_pred` whose
/// free-variable set intersects the keys dropped or renamed-away by
/// `m1`'s field transforms on the corresponding anchor is
/// conservatively rewritten to `false` on its own anchor (the
/// variable cannot be present, so the predicate cannot legitimately
/// be evaluated; refusing to keep the row is the safe default and
/// matches the audit's "default fail" recommendation).
fn compose_conditional_survival(
    m1: &CompiledMigration,
    m2: &CompiledMigration,
    src_schema: &panproto_schema::Schema,
) -> HashMap<panproto_gat::Name, panproto_expr::Expr> {
    let mut result = m1.conditional_survival.clone();
    for (m2_anchor, m2_pred) in &m2.conditional_survival {
        for m1_anchor in m1_preimages(m1, src_schema, m2_anchor) {
            let scoped = scope_check_predicate(m2_pred, m1, &m1_anchor);
            result
                .entry(m1_anchor)
                .and_modify(|existing| {
                    *existing = panproto_expr::Expr::Builtin(
                        panproto_expr::BuiltinOp::And,
                        vec![existing.clone(), scoped.clone()],
                    );
                })
                .or_insert(scoped);
        }
    }
    result
}

/// Returns `m2_pred` unchanged when every free top-level variable
/// still exists at the composed evaluation site, or the constant
/// `false` when `m1` drops, renames-away, or filters-out a field
/// referenced in `m2_pred`.
///
/// Detection rules (top-level keys only — nested-path access through
/// `Field`/`Index` is not analysed because `panproto_expr::free_vars`
/// returns only top-level `Var` names; a `PathTransform` on `attrs`
/// affects only nested keys and so is not relevant here):
///
/// * `DropField { key }` removes `key`.
/// * `RenameField { old_key, .. }` removes `old_key`.
/// * `KeepFields { keys }` removes any top-level field not in `keys`;
///   we conservatively flag every free variable not in the
///   intersection of all `KeepFields` retain sets on this anchor.
/// * `Case { branches }` would require all branches to drop a key
///   for that key to be statically certain-dropped; the conservative
///   approximation here is to skip `Case` entirely (no false
///   positives on conditional drops, with a known soundness gap if
///   every branch happens to drop the same key).
/// * `PathTransform`, `AddField`, `ApplyExpr`, `ComputeField`,
///   `MapReferences`: do not remove top-level bindings.
fn scope_check_predicate(
    pred: &panproto_expr::Expr,
    m1: &CompiledMigration,
    anchor: &panproto_gat::Name,
) -> panproto_expr::Expr {
    let Some(m1_xforms) = m1.field_transforms.get(anchor) else {
        return pred.clone();
    };
    let analysis = analyse_field_transforms(m1_xforms);
    if analysis.dropped.is_empty() && analysis.keep_intersection.is_none() {
        return pred.clone();
    }
    let free = panproto_expr::free_vars(pred);
    let dropped_hit = free.iter().any(|v| analysis.dropped.contains(v.as_ref()));
    let keep_violation = analysis
        .keep_intersection
        .as_ref()
        .is_some_and(|keep| free.iter().any(|v| !keep.contains(v.as_ref())));
    if dropped_hit || keep_violation {
        // Conservative: refuse to keep the row when the predicate
        // depends on a field that no longer exists.
        return panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(false));
    }
    pred.clone()
}

/// Static analysis result for a single anchor's transform list.
struct FieldDropAnalysis {
    /// Keys explicitly dropped or renamed-away.
    dropped: std::collections::HashSet<String>,
    /// If any `KeepFields` is present, the intersection of all its
    /// retain sets — every free variable outside this set has been
    /// dropped by the filter.
    keep_intersection: Option<std::collections::HashSet<String>>,
}

fn analyse_field_transforms(xforms: &[panproto_inst::wtype::FieldTransform]) -> FieldDropAnalysis {
    use panproto_inst::wtype::FieldTransform;
    let mut dropped = std::collections::HashSet::new();
    let mut keep_intersection: Option<std::collections::HashSet<String>> = None;
    for x in xforms {
        match x {
            FieldTransform::DropField { key } => {
                dropped.insert(key.clone());
            }
            FieldTransform::RenameField { old_key, .. } => {
                dropped.insert(old_key.clone());
            }
            FieldTransform::KeepFields { keys } => {
                let next: std::collections::HashSet<String> = keys.iter().cloned().collect();
                keep_intersection = Some(match keep_intersection {
                    None => next,
                    Some(prev) => prev.intersection(&next).cloned().collect(),
                });
            }
            // PathTransform on `path = ["attrs"]` drops nested
            // `attrs.x`, not top-level `x`; free_vars sees only
            // top-level Var names, so PathTransform is not relevant
            // to this analysis.
            //
            // Case branches are conditional; static drop-detection
            // would require all branches to drop the same key. We
            // skip Case rather than return spurious false-rewrites.
            //
            // AddField / ApplyExpr / ComputeField / MapReferences:
            // do not remove a free-name binding.
            _ => {}
        }
    }
    FieldDropAnalysis {
        dropped,
        keep_intersection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_expr::Expr;
    use panproto_inst::FieldTransform;

    use crate::tests::{TestUnwrap, TestUnwrapErr, identity_lens, three_node_schema};

    #[test]
    fn compose_identity_with_identity() {
        let schema = three_node_schema();
        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&schema);

        let composed = compose(&l1, &l2);
        assert!(composed.is_ok(), "composing identity lenses should succeed");

        let lens = composed.unwrap_or_else(|e| panic!("compose failed: {e}"));
        assert_eq!(
            lens.src_schema().vertex_count(),
            schema.vertex_count(),
            "composed src schema should match original"
        );
        assert_eq!(
            lens.tgt_schema().vertex_count(),
            schema.vertex_count(),
            "composed tgt schema should match original"
        );
    }

    #[test]
    fn compose_rejects_edge_mismatch() {
        use std::collections::BTreeSet;
        let schema = three_node_schema();
        // Identical vertices, but the middle boundary differs by one edge.
        let mut altered = schema.clone();
        let victim = altered.edges.keys().next().cloned();
        if let Some(victim) = victim {
            altered.edges.remove(&victim);
        }

        // Precondition of the test: vertex sets still match exactly, so
        // the vertex-only check would have let this through.
        assert_eq!(
            schema.vertices.keys().collect::<BTreeSet<_>>(),
            altered.vertices.keys().collect::<BTreeSet<_>>(),
            "vertex sets must be identical for this test to exercise the edge check"
        );

        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&altered);
        let result = compose(&l1, &l2);
        assert!(
            matches!(result, Err(LensError::CompositionMismatch)),
            "composition across an edge mismatch must be rejected: {result:?}"
        );
    }

    #[test]
    fn compose_rejects_hyper_edge_mismatch() {
        use panproto_gat::Name;
        use panproto_schema::HyperEdge;
        use std::collections::HashMap;

        let schema = three_node_schema();
        // Identical vertices and edges, but add a hyper-edge to one side.
        let mut altered = schema.clone();
        altered.hyper_edges.insert(
            Name::from("fan"),
            HyperEdge {
                id: Name::from("fan"),
                kind: Name::from("fan"),
                signature: HashMap::new(),
                parent_label: Name::from("post:body"),
            },
        );

        let l1 = identity_lens(&schema);
        let l2 = identity_lens(&altered);
        let result = compose(&l1, &l2);
        assert!(
            matches!(result, Err(LensError::CompositionMismatch)),
            "composition across a hyper-edge mismatch must be rejected: {result:?}"
        );
    }

    #[test]
    fn fusion_reports_a_field_read_after_a_drop() {
        let protocol = panproto_schema::Protocol::default();
        let Ok(builder) = panproto_schema::SchemaBuilder::new(&protocol).vertex("a", "obj", None)
        else {
            panic!("the C11 vertex is valid");
        };
        let Ok(schema) = builder.build() else {
            panic!("the C11 schema is valid");
        };
        let anchor = panproto_gat::Name::from("a");
        let mut first = identity_lens(&schema);
        first.compiled.field_transforms.insert(
            anchor.clone(),
            vec![FieldTransform::DropField {
                key: "f".to_owned(),
            }],
        );
        let mut second = identity_lens(&schema);
        second.compiled.field_transforms.insert(
            anchor.clone(),
            vec![FieldTransform::ApplyExpr {
                key: "f".to_owned(),
                expr: Expr::Var("f".into()),
                inverse: Some(Expr::Var("f".into())),
                coercion_class: panproto_gat::CoercionClass::Iso,
            }],
        );
        assert!(matches!(
            fuse(&first, &second),
            Err(LensError::ComposeUnboundField {
                anchor: refused_anchor,
                field,
            }) if refused_anchor == anchor.as_str() && field == "f"
        ));
    }

    #[test]
    fn fusion_does_not_resurrect_a_dropped_source_vertex_through_a_remap() {
        let protocol = panproto_schema::Protocol::default();
        let schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .build()
            .test_unwrap("the two-vertex schema is valid");
        let a = panproto_gat::Name::from("a");
        let b = panproto_gat::Name::from("b");

        let mut first = identity_lens(&schema);
        first.compiled.surviving_verts.remove(&b);
        let mut second = identity_lens(&schema);
        second.compiled.vertex_remap.insert(a, b.clone());

        let candidate = fuse(&first, &second).test_unwrap("the migrations compose structurally");
        assert!(!crate::asymmetric::vertex_survives(&first, &b));
        assert!(crate::asymmetric::vertex_survives(&candidate, &b));
        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must not resurrect source b after the first stage drops it"
        );
    }

    #[test]
    fn fusion_preserves_the_image_of_an_isolated_source_vertex() {
        let protocol = panproto_schema::Protocol::default();
        let schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .vertex("x", "obj", None)
            .test_unwrap("x is a valid vertex")
            .vertex("y", "obj", None)
            .test_unwrap("y is a valid vertex")
            .build()
            .test_unwrap("the isolated-vertex schema is valid");
        let a = panproto_gat::Name::from("a");
        let b = panproto_gat::Name::from("b");
        let x = panproto_gat::Name::from("x");
        let y = panproto_gat::Name::from("y");

        let mut first = identity_lens(&schema);
        first.compiled.vertex_remap.insert(x, a.clone());
        let mut second = identity_lens(&schema);
        second.compiled.vertex_remap.insert(a.clone(), b);
        second.compiled.vertex_remap.insert(y, a.clone());

        let candidate = fuse(&first, &second).test_unwrap("the migrations compose structurally");
        assert!(crate::asymmetric::vertex_survives(&candidate, &a));
        assert_eq!(
            candidate.compiled.vertex_remap.get(&a).unwrap_or(&a),
            &a,
            "the structural composite elides the sequential a-to-b remap"
        );
        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain the sequential image of an isolated source vertex"
        );
    }

    #[test]
    fn fusion_preserves_reverse_vertex_fibres() {
        let protocol = panproto_schema::Protocol::default();
        let source = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("A", "obj", None)
            .test_unwrap("A is a valid vertex")
            .vertex("B", "obj", None)
            .test_unwrap("B is a valid vertex")
            .build()
            .test_unwrap("the source schema is valid");
        let middle = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("X", "obj", None)
            .test_unwrap("X is a valid vertex")
            .vertex("Y", "obj", None)
            .test_unwrap("Y is a valid vertex")
            .build()
            .test_unwrap("the middle schema is valid");
        let target = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("Y", "obj", None)
            .test_unwrap("Y is a valid vertex")
            .build()
            .test_unwrap("the target schema is valid");
        let first = Lens {
            compiled: CompiledMigration {
                surviving_verts: [Name::from("X"), Name::from("Y")].into(),
                vertex_remap: [
                    (Name::from("A"), Name::from("X")),
                    (Name::from("B"), Name::from("Y")),
                ]
                .into(),
                ..CompiledMigration::default()
            },
            src_schema: source,
            tgt_schema: middle.clone(),
        };
        let second = Lens {
            compiled: CompiledMigration {
                surviving_verts: [Name::from("Y")].into(),
                vertex_remap: [(Name::from("X"), Name::from("Y"))].into(),
                ..CompiledMigration::default()
            },
            src_schema: middle,
            tgt_schema: target,
        };

        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain the second stage's distinct reverse fibres"
        );
    }

    #[test]
    fn fusion_preserves_reverse_edge_fibres() {
        let protocol = panproto_schema::Protocol::default();
        let middle = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .edge("a", "b", "prop", Some("e1"))
            .test_unwrap("e1 is a valid edge")
            .edge("a", "b", "prop", Some("e2"))
            .test_unwrap("e2 is a valid edge")
            .build()
            .test_unwrap("the middle schema is valid");
        let target = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .edge("a", "b", "prop", Some("q"))
            .test_unwrap("q is a valid edge")
            .build()
            .test_unwrap("the target schema is valid");
        let first = identity_lens(&middle);
        let target_edge = target
            .edges
            .keys()
            .next()
            .cloned()
            .test_unwrap("the target edge exists");
        let second = Lens {
            compiled: CompiledMigration {
                surviving_verts: target.vertices.keys().cloned().collect(),
                surviving_edges: [target_edge.clone()].into(),
                edge_remap: middle
                    .edges
                    .keys()
                    .cloned()
                    .map(|source| (source, target_edge.clone()))
                    .collect(),
                ..CompiledMigration::default()
            },
            src_schema: middle,
            tgt_schema: target,
        };

        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain the second stage's distinct reverse edge fibres"
        );
    }

    #[test]
    fn fusion_retains_target_only_reverse_carriers() {
        let protocol = panproto_schema::Protocol::default();
        let middle = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .build()
            .test_unwrap("the middle schema is valid");
        let target = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("fresh", "obj", None)
            .test_unwrap("fresh is a valid vertex")
            .build()
            .test_unwrap("the target schema is valid");
        let first = identity_lens(&middle);
        let second = Lens {
            compiled: CompiledMigration {
                surviving_verts: target.vertices.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: middle,
            tgt_schema: target,
        };

        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain the atomic refusal at a target-only reverse carrier"
        );
    }

    #[test]
    fn fusion_rejects_a_value_layer_at_an_undeclared_anchor() {
        let protocol = panproto_schema::Protocol::default();
        let schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .build()
            .test_unwrap("the schema is valid");
        let first = identity_lens(&schema);
        let mut second = identity_lens(&schema);
        second.compiled.field_transforms.insert(
            "outside".into(),
            vec![panproto_inst::FieldTransform::RenameField {
                old_key: "old".to_owned(),
                new_key: "new".to_owned(),
            }],
        );

        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion would drop a raw value-layer entry with no first-stage preimage"
        );
    }

    #[test]
    fn fusion_retains_staged_error_precedence_after_a_lossy_first_stage() {
        let protocol = panproto_schema::Protocol::default();
        let source_schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("kept", "obj", None)
            .test_unwrap("kept is a valid vertex")
            .vertex("dropped", "obj", None)
            .test_unwrap("dropped is a valid vertex")
            .build()
            .test_unwrap("the source schema is valid");
        let middle_schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("kept", "obj", None)
            .test_unwrap("kept is a valid vertex")
            .build()
            .test_unwrap("the middle schema is valid");
        let first = Lens {
            compiled: CompiledMigration {
                surviving_verts: middle_schema.vertices.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: middle_schema.clone(),
        };
        let second = identity_lens(&middle_schema);
        let source = panproto_inst::GInstance::new()
            .with_node(panproto_inst::Node::new(0, "kept"))
            .with_node(panproto_inst::Node::new(1, "dropped"));

        let (middle, _) = crate::graph_lens::get(&first, &source).test_unwrap("first graph get");
        let (mut sequential_view, second_complement) =
            crate::graph_lens::get(&second, &middle).test_unwrap("second graph get");
        sequential_view
            .insert_node(panproto_inst::Node::new(1, "outside"))
            .test_unwrap("the edit reuses the dropped source id");
        let sequential_error =
            crate::graph_lens::put(&second, &sequential_view, &second_complement)
                .test_unwrap_err("the atomic second put rejects the outside anchor");

        let candidate = fuse(&first, &second).test_unwrap("the structural migrations compose");
        let (mut fused_view, fused_complement) =
            crate::graph_lens::get(&candidate, &source).test_unwrap("candidate graph get");
        fused_view
            .insert_node(panproto_inst::Node::new(1, "outside"))
            .test_unwrap("the same edit is available on the candidate view");
        let fused_error = crate::graph_lens::put(&candidate, &fused_view, &fused_complement)
            .test_unwrap_err("the fused candidate rejects the reserved id");

        assert!(matches!(
            sequential_error,
            LensError::PutDomain(error)
                if matches!(*error, panproto_inst::PutDomainError::AmbiguousReverseAnchor {
                    elem: panproto_inst::ViewRef::Node(1),
                    ref candidates,
                } if candidates.is_empty())
        ));
        assert!(matches!(
            fused_error,
            LensError::PutDomain(error)
                if matches!(*error, panproto_inst::PutDomainError::IdCollision {
                    elem: panproto_inst::ViewRef::Node(1),
                })
        ));
        assert!(
            tree_fusion(&first, &second).is_none(),
            "a lossy first stage must remain explicit so the rightmost refusal wins"
        );
    }

    #[test]
    fn fusion_preserves_identity_extended_reverse_anchor_composition() {
        let protocol = panproto_schema::Protocol::default();
        let schema = |vertex: &str| {
            panproto_schema::SchemaBuilder::new(&protocol)
                .vertex(vertex, "obj", None)
                .test_unwrap("the vertex is valid")
                .build()
                .test_unwrap("the schema is valid")
        };
        let a = schema("a");
        let b = schema("b");
        let c = schema("c");
        let relabel = |source: &Schema, target: &Schema, from: &str, to: &str| Lens {
            compiled: CompiledMigration {
                surviving_verts: [Name::from(to)].into(),
                vertex_remap: [(Name::from(from), Name::from(to))].into(),
                ..CompiledMigration::default()
            },
            src_schema: source.clone(),
            tgt_schema: target.clone(),
        };
        let first = relabel(&a, &b, "a", "b");
        let second = relabel(&b, &c, "b", "c");
        let source = panproto_inst::WInstance::new(
            HashMap::from([(0, panproto_inst::Node::new(0, "a"))]),
            Vec::new(),
            Vec::new(),
            0,
            Name::from("a"),
        );

        let (middle, first_complement) =
            crate::asymmetric::get(&first, &source).test_unwrap("first tree get");
        let (mut sequential_view, second_complement) =
            crate::asymmetric::get(&second, &middle).test_unwrap("second tree get");
        sequential_view
            .nodes
            .get_mut(&0)
            .test_unwrap("root exists")
            .anchor = Name::from("b");
        sequential_view.schema_root = Name::from("b");
        let intermediate = crate::asymmetric::put(&second, &sequential_view, &second_complement)
            .test_unwrap("second put uses identity extension outside c");
        let sequential = crate::asymmetric::put(&first, &intermediate, &first_complement)
            .test_unwrap("first put maps b back to a");

        let candidate = fuse(&first, &second).test_unwrap("the structural migrations compose");
        let (mut fused_view, fused_complement) =
            crate::asymmetric::get(&candidate, &source).test_unwrap("candidate tree get");
        fused_view
            .nodes
            .get_mut(&0)
            .test_unwrap("root exists")
            .anchor = Name::from("b");
        fused_view.schema_root = Name::from("b");
        let fused = crate::asymmetric::put(&candidate, &fused_view, &fused_complement)
            .test_unwrap("candidate put uses its own identity extension");

        assert_eq!(sequential.nodes[&0].anchor, Name::from("a"));
        assert_eq!(fused.nodes[&0].anchor, Name::from("b"));
        assert!(
            tree_fusion(&first, &second).is_none(),
            "two non-identity relabelings must remain atomic on outside-schema edits"
        );
    }

    #[test]
    fn fusion_accounts_for_extraneous_reverse_anchor_entries() {
        let protocol = panproto_schema::Protocol::default();
        let schema = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .build()
            .test_unwrap("the schema is valid");
        let lens = |from: &str, to: &str| Lens {
            compiled: CompiledMigration {
                surviving_verts: [Name::from("a")].into(),
                vertex_remap: [(Name::from(from), Name::from(to))].into(),
                ..CompiledMigration::default()
            },
            src_schema: schema.clone(),
            tgt_schema: schema.clone(),
        };
        let first = lens("x", "y");
        let second = lens("y", "z");
        let source = panproto_inst::WInstance::new(
            HashMap::from([(0, panproto_inst::Node::new(0, "a"))]),
            Vec::new(),
            Vec::new(),
            0,
            Name::from("a"),
        );
        let (middle, first_complement) =
            crate::asymmetric::get(&first, &source).test_unwrap("first tree get");
        let (mut sequential_view, second_complement) =
            crate::asymmetric::get(&second, &middle).test_unwrap("second tree get");
        sequential_view.nodes.get_mut(&0).test_unwrap("root").anchor = Name::from("y");
        sequential_view.schema_root = Name::from("y");
        let intermediate = crate::asymmetric::put(&second, &sequential_view, &second_complement)
            .test_unwrap("second put uses identity extension at y");
        let sequential = crate::asymmetric::put(&first, &intermediate, &first_complement)
            .test_unwrap("first put uses its raw reverse entry");

        let candidate = fuse(&first, &second).test_unwrap("the raw migrations compose");
        let (mut fused_view, fused_complement) =
            crate::asymmetric::get(&candidate, &source).test_unwrap("candidate tree get");
        fused_view.nodes.get_mut(&0).test_unwrap("root").anchor = Name::from("y");
        fused_view.schema_root = Name::from("y");
        let fused = crate::asymmetric::put(&candidate, &fused_view, &fused_complement)
            .test_unwrap("candidate put uses its composed reverse map");

        assert_eq!(sequential.nodes[&0].anchor, Name::from("x"));
        assert_eq!(fused.nodes[&0].anchor, Name::from("y"));
        assert!(
            tree_fusion(&first, &second).is_none(),
            "raw remap entries outside the declared carrier still affect put"
        );
    }

    #[test]
    fn fusion_does_not_reintroduce_a_dropped_hyper_edge_declaration() {
        let protocol = panproto_schema::Protocol::default();
        let mut source = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .build()
            .test_unwrap("the source schema is valid");
        source.hyper_edges.insert(
            Name::from("h"),
            panproto_schema::HyperEdge {
                id: Name::from("h"),
                kind: Name::from("fan"),
                signature: HashMap::from([
                    (Name::from("p"), Name::from("a")),
                    (Name::from("l"), Name::from("b")),
                ]),
                parent_label: Name::from("p"),
            },
        );
        let mut middle = source.clone();
        middle.hyper_edges.clear();
        let first = Lens {
            compiled: CompiledMigration {
                surviving_verts: middle.vertices.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: source.clone(),
            tgt_schema: middle.clone(),
        };
        let second = Lens {
            compiled: CompiledMigration {
                surviving_verts: source.vertices.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: middle,
            tgt_schema: source,
        };

        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain a middle boundary that drops fan declarations"
        );
    }

    #[test]
    fn fusion_retains_intermediate_required_cell_validation() {
        let protocol = panproto_schema::Protocol::default();
        let base = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .edge("a", "b", "prop", Some("f"))
            .test_unwrap("f is a valid edge")
            .build()
            .test_unwrap("the base schema is valid");
        let required_edge = base
            .edges
            .keys()
            .next()
            .cloned()
            .test_unwrap("the required edge exists");
        let mut middle = base.clone();
        middle.required.insert(Name::from("a"), vec![required_edge]);
        let compiled = || CompiledMigration {
            surviving_verts: base.vertices.keys().cloned().collect(),
            surviving_edges: base.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        };
        let first = Lens {
            compiled: compiled(),
            src_schema: base.clone(),
            tgt_schema: middle.clone(),
        };
        let second = Lens {
            compiled: compiled(),
            src_schema: middle,
            tgt_schema: base,
        };

        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain the required-cell check at the middle boundary"
        );
    }

    #[test]
    fn fusion_retains_validation_on_the_second_middle_representation() {
        let protocol = panproto_schema::Protocol::default();
        let base = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .edge("a", "b", "prop", Some("f"))
            .test_unwrap("f is a valid edge")
            .build()
            .test_unwrap("the base schema is valid");
        let required_edge = base
            .edges
            .keys()
            .next()
            .cloned()
            .test_unwrap("the required edge exists");
        let mut second_source = base.clone();
        second_source
            .required
            .insert(Name::from("a"), vec![required_edge]);
        let compiled = || CompiledMigration {
            surviving_verts: base.vertices.keys().cloned().collect(),
            surviving_edges: base.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        };
        let first = Lens {
            compiled: compiled(),
            src_schema: base.clone(),
            tgt_schema: base.clone(),
        };
        let second = Lens {
            compiled: compiled(),
            src_schema: second_source,
            tgt_schema: base,
        };

        assert!(
            check_boundary(&first.tgt_schema, &second.src_schema).is_ok(),
            "ordinary composition equates the middle carrier rather than its declarations"
        );
        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must inspect the schema that the atomic second put validates"
        );
    }

    #[test]
    fn fusion_retains_hyper_validation_on_the_second_middle_representation() {
        let protocol = panproto_schema::Protocol::default();
        let mut base = panproto_schema::SchemaBuilder::new(&protocol)
            .vertex("a", "obj", None)
            .test_unwrap("a is a valid vertex")
            .vertex("b", "obj", None)
            .test_unwrap("b is a valid vertex")
            .build()
            .test_unwrap("the base schema is valid");
        base.hyper_edges.insert(
            Name::from("h"),
            panproto_schema::HyperEdge {
                id: Name::from("h"),
                kind: Name::from("fan"),
                signature: HashMap::from([
                    (Name::from("p"), Name::from("a")),
                    (Name::from("l"), Name::from("b")),
                ]),
                parent_label: Name::from("p"),
            },
        );
        let mut second_source = base.clone();
        second_source
            .hyper_edges
            .get_mut(&Name::from("h"))
            .test_unwrap("h is present")
            .parent_label = Name::from("l");
        let compiled = || CompiledMigration {
            surviving_verts: base.vertices.keys().cloned().collect(),
            ..CompiledMigration::default()
        };
        let first = Lens {
            compiled: compiled(),
            src_schema: base.clone(),
            tgt_schema: base.clone(),
        };
        let second = Lens {
            compiled: compiled(),
            src_schema: second_source,
            tgt_schema: base,
        };

        assert!(
            check_boundary(&first.tgt_schema, &second.src_schema).is_ok(),
            "ordinary composition equates hyper-edge keys rather than declarations"
        );
        assert!(
            tree_fusion(&first, &second).is_none(),
            "fusion must retain the second middle representation's fan validation"
        );
    }
}
