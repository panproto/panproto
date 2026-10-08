//! Asymmetric lenses over graph-shaped instances.
//!
//! The forward direction is the graph restrict engine. The backward
//! direction restores the source coordinates recorded by
//! [`GraphComplement`], while preserving edits to coordinates the view
//! determines.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_inst::identity::{
    CanonicalSink, ComplementShape, IdentityCarrier, IdentityPolicy, IdentityStatus,
    compose_identities,
};
use panproto_inst::{
    BoundaryProtocols, ContractionMode, ContractionPlan, EdgeFate, EdgeId, FanResolver,
    FieldTransform, GEdge, GInstance, GraphComplement, HYPER_LABEL_KIND, HYPER_PARENT_KIND,
    IdAllocator, IdRange, InstError, PairResolution, PutDomainError, RestrictPlan, SynthKey,
    ViewRef, canonical_label_shape, graph_restrict_with_plan,
};
use panproto_schema::{Edge, Schema};

use crate::asymmetric::{
    ComplementCompose, apply_inverse_transforms, extra_fields_equiv, node_equiv, presence_equiv,
    propagate_view_edits_through_inverse,
};
use crate::compose::{check_boundary, duplicate, tree_fusion};
use crate::{Lens, LensError};

type KeyBuckets = BTreeMap<(Option<u32>, Vec<u8>), (Vec<panproto_inst::Value>, Vec<u32>)>;

/// Forward graph-lens direction.
///
/// The graph restrict is compiled once for this call and its complement is
/// stamped with the graph identity of `lens`.
///
/// # Errors
///
/// Returns the plan compilation or graph restrict error.
pub fn get(lens: &Lens, source: &GInstance) -> Result<(GInstance, GraphComplement), LensError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;
    get_with_plan(lens, source, &plan)
}

/// Forward graph-lens direction with explicit boundary protocols.
///
/// This additive companion to [`get`] installs the plan's cell layer, so a
/// value written by a transform is checked against the declared target
/// column kind. The protocol-free base API remains the structural graph
/// path. Both paths stamp the same graph-lens identity.
///
/// # Errors
///
/// Returns the boundary-aware plan compilation or graph restrict error,
/// including typed value-fidelity failures at target column vertices.
pub fn get_with_protocols(
    lens: &Lens,
    source: &GInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(GInstance, GraphComplement), LensError> {
    let plan = RestrictPlan::compile_with_protocols(
        &lens.src_schema,
        &lens.tgt_schema,
        &lens.compiled,
        protocols,
    )?;
    get_with_plan(lens, source, &plan)
}

fn get_with_plan(
    lens: &Lens,
    source: &GInstance,
    plan: &RestrictPlan,
) -> Result<(GInstance, GraphComplement), LensError> {
    let (view, mut complement) = graph_restrict_with_plan(source, plan)?;
    complement.stamp(lens.identity(ComplementShape::Graph), &lens.src_schema);
    Ok((view, complement))
}

/// Run the gating graph-lens laws over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`; it is not a
/// proof over all graph instances.
///
/// # Errors
///
/// The first law violation in the specified gate order.
pub fn check_laws(
    lens: &Lens,
    source: &GInstance,
) -> Result<crate::laws::LawReport, crate::LawViolation> {
    crate::laws::check_laws_report::<crate::GraphShape>(lens, source, &())
}

/// Check graph `GetPut`.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// An operational error or `GetPut` violation.
pub fn check_get_put(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_get_put_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check strict graph `PutGet` over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// An operational error or `PutGet` violation.
pub fn check_put_get(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_put_get_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check graph put totality over its generated admitted views.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first admitted mutation on which put fails.
pub fn check_totality(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_totality_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check graph `PutTwice` over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first repeated-put drift.
pub fn check_put_twice(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_put_twice_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check graph presentation congruence.
///
/// A pass means "no counterexample found" among the coherent
/// presentation witnesses for `source`.
///
/// # Errors
///
/// The first domain or put result that is not congruent.
pub fn check_congruence(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_congruence_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check graph `WeakPutGet` over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`. This is a
/// named non-gating check.
///
/// # Errors
///
/// The first weak round-trip violation.
pub fn check_weak_put_get(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_weak_put_get_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check graph put injectivity on generated distinct admitted views.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first pair of inequivalent views with one put result.
pub fn check_put_injectivity(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_put_injectivity_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check graph put locality against the exact inverse-image-plus-cascade
/// footprint.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first mutation whose actual and specified footprints differ.
pub fn check_locality(lens: &Lens, source: &GInstance) -> Result<(), crate::LawViolation> {
    crate::laws::check_locality_for_shape::<crate::GraphShape>(lens, source, &())
}

/// Check the gating laws on every labelled graph in a finite canonical scope.
///
/// No labelled isomorphs are pruned. Each value-bearing coordinate is checked
/// against every other admitted member of the fixed six-value alphabet. A
/// pass means "no counterexample found" in `scope`; it is not a proof outside
/// that finite universe. At each graph the Single-Step Basis covers only its
/// single-edit neighbourhood; `PutTwice`, the `PutPut` classifications and E.6
/// exercise multi-step behaviour separately.
///
/// # Errors
///
/// The first law violation in deterministic enumeration order.
pub fn check_laws_exhaustive(
    lens: &Lens,
    schema: &Schema,
    scope: crate::laws::SmallScope,
) -> Result<Vec<crate::laws::LawReport>, crate::LawViolation> {
    <crate::GraphShape as crate::laws::LawShape>::enumerate_small(schema, scope, &())
        .iter()
        .map(|source| {
            crate::laws::check_laws_report_exhaustive::<crate::GraphShape>(lens, source, &())
        })
        .collect()
}

/// Backward graph-lens direction under the default identity policy.
///
/// Removing a surviving view node applies an SPO-style deletion in the
/// unknown context carried by the complement. Once the incident hidden
/// edges have been read, the reconstructed source is a DPO rule satisfying
/// the dangling condition by construction (`harmer2020reversibility`, Remark
/// 2.2; `brown2022computational`, Equations 3--4).
///
/// On the Delta fragment -- subschema restriction and monic deletions, with
/// no contraction, expansion, or survival predicate -- the cascade for a
/// deletion-only view edit is the largest source subinstance whose
/// restriction is the edited view. Indeed, put keeps every source node whose
/// surviving image remains in the view, every dropped node, and exactly the
/// source edges whose endpoints remain; any other source subinstance with
/// that restriction can contain no deleted surviving node or incident edge,
/// so it is contained in this result. The same argument proves the universal
/// form: the union of all source subinstances whose restriction is contained
/// in the edit still has restriction contained in it and equals the cascade.
/// This proof was checked against the independent exhaustive oracle recorded
/// for O9 (14) (e).
///
/// Its correspondence to the propagatable delete (cartesian lift) of
/// `johnsonrosebrugh2007fibrations` is inferred. The mono-restricted lift is
/// the cascade where the inclusion of path categories is full and faithful,
/// following `weighill2014bifibrational`, Theorem 18 and Proposition 29.
/// Outside the Delta fragment this cartesian reading remains a conjecture.
///
/// # Errors
///
/// Identity errors precede put-domain errors. Otherwise this is total on
/// the domain accepted by [`check_put_domain`].
pub fn put(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<GInstance, LensError> {
    put_checked(lens, view, complement, IdentityPolicy::AllowLegacy).map(|(source, _)| source)
}

/// Backward graph-lens direction under an explicit identity policy.
///
/// # Errors
///
/// As [`put`], and returns the accepted identity status.
pub fn put_checked(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    policy: IdentityPolicy,
) -> Result<(GInstance, IdentityStatus), LensError> {
    let status = crate::identity::verify_with_identity(
        complement,
        lens,
        &lens.identity(ComplementShape::Graph),
        policy,
    )?;
    if let Err(found) = check_put_domain(lens, view, complement) {
        return Err(LensError::PutDomain(Box::new(found[0].clone())));
    }
    Ok((
        restore(lens, view, complement).map_err(first_put_error)?,
        status,
    ))
}

/// Reconstruct a graph through an isomorphism without a complement.
///
/// # Errors
///
/// [`LensError::NotAnIsomorphism`] when the lens has non-terminal residue,
/// or an ordinary graph put error.
pub fn put_without_complement(lens: &Lens, view: &GInstance) -> Result<GInstance, LensError> {
    if let Some(detail) = obstruction_to_isomorphism(lens) {
        return Err(LensError::NotAnIsomorphism { detail });
    }
    let mut complement = GraphComplement::empty();
    complement.stamp(lens.identity(ComplementShape::Graph), &lens.src_schema);
    put(lens, view, &complement)
}

/// Whether graph get leaves no source information in its complement.
///
/// This classifies bijective lenses whose complement is `Unit`, a class
/// narrower than isomorphism in the category of lenses. A `false` result is
/// an obstruction to complement-free put, not the absence of an inverse lens,
/// and does not decide whether the lens is forgetful. This is the graph
/// counterpart of [`Lens::is_isomorphism`]. In addition
/// to the ordinary vertex, edge and value coordinates, it accounts for
/// reified hyper-edges: a non-identity resolver changes a fan anchor or child
/// label and graph get records that source coordinate in its complement.
#[must_use]
pub fn is_isomorphism(lens: &Lens) -> bool {
    obstruction_to_isomorphism(lens).is_none()
}

/// The first graph coordinate that prevents complement-free reconstruction.
///
/// The result is deterministic and concerns the same narrower, bijective
/// (complement `Unit`) class as [`is_isomorphism`]. It reports an obstruction
/// to complement-free put, not proof that no inverse lens exists, and does not
/// decide whether the lens is forgetful. A pass means that no obstruction was
/// found by the complete static graph classifier; it is not a claim about
/// lenses outside the graph restrict domain.
#[must_use]
pub fn obstruction_to_isomorphism(lens: &Lens) -> Option<String> {
    let plan = match RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled) {
        Ok(plan) => plan,
        Err(error) => {
            return Some(format!(
                "the migration is not executable as a graph lens: {error}"
            ));
        }
    };

    let mut vertices = lens.src_schema.vertices.keys().collect::<Vec<_>>();
    vertices.sort();
    let mut vertex_images = BTreeMap::new();
    for source in vertices {
        if !crate::asymmetric::vertex_survives(lens, source) {
            return Some(format!(
                "vertex `{source}` does not survive, so a graph node over it cannot be reconstructed"
            ));
        }
        if lens.compiled.conditional_survival.contains_key(source) {
            return Some(format!(
                "vertex `{source}` survives conditionally, so some graph nodes over it are dropped"
            ));
        }
        let image = lens.compiled.vertex_remap.get(source).unwrap_or(source);
        if let Some(other) = vertex_images.insert(image, source) {
            return Some(format!(
                "vertices `{other}` and `{source}` both map to `{image}`, so a view node does not determine its source anchor"
            ));
        }
        if lens
            .compiled
            .op_term_assignments
            .get(source)
            .is_some_and(|assignments| !assignments.is_empty())
        {
            return Some(format!(
                "vertex `{source}` has term assignments that graph put does not invert without a complement"
            ));
        }
        if let Some(transforms) = lens.compiled.field_transforms.get(source)
            && let Some(detail) = graph_transform_obstruction(transforms)
        {
            return Some(format!("vertex `{source}` {detail}"));
        }
    }

    for (source, fate) in &plan.edge_image.fates {
        let EdgeFate::Keep(image) = fate else {
            return Some(format!(
                "edge `{:?}` is not kept as one graph edge, so it cannot be reconstructed without a complement",
                source.name
            ));
        };
        if plan.edge_image.preimages(image).len() != 1 {
            return Some(format!(
                "more than one source edge maps to `{:?}`, so a view edge does not determine its source label",
                image.name
            ));
        }
    }

    if let Some(source) = lens.src_schema.hyper_edges.keys().min() {
        return Some(format!(
            "hyper-edge `{source}` admits a schema-valid parent-only fan, which graph restrict currently drops"
        ));
    }
    if let Some(detail) = graph_validation_obstruction(&lens.src_schema) {
        return Some(detail);
    }
    None
}

fn graph_validation_obstruction(schema: &panproto_schema::Schema) -> Option<String> {
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
            "vertex `{vertex}` requires field `{field}`, so graph put can refuse a finite source in which it is absent"
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
            "vertex `{vertex}` declares key `{key}`, so graph put can refuse a finite source with duplicate key values"
        )
    })
}

fn graph_transform_obstruction(transforms: &[FieldTransform]) -> Option<String> {
    for transform in transforms {
        let detail = match transform {
            FieldTransform::RenameField { old_key, new_key } if old_key == new_key => None,
            FieldTransform::RenameField { .. } => Some(
                "renames an open graph field and may overwrite an independently present target key"
                    .to_owned(),
            ),
            FieldTransform::ApplyExpr {
                inverse,
                coercion_class,
                ..
            } if inverse.is_some()
                && matches!(coercion_class, panproto_gat::CoercionClass::Iso) =>
            {
                None
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                graph_transform_obstruction(std::slice::from_ref(inner))
            }
            FieldTransform::MapReferences { rename_map, .. }
                if rename_map
                    .iter()
                    .all(|(source, target)| target.as_deref() == Some(source.as_str())) =>
            {
                None
            }
            FieldTransform::Case { branches }
                if branches.iter().all(|branch| branch.transforms.is_empty()) =>
            {
                None
            }
            FieldTransform::PathTransform { .. } => Some(
                "has a nested path transform that graph put cannot invert without a complement"
                    .to_owned(),
            ),
            FieldTransform::Case { .. } => Some(
                "has a conditional transform that graph put cannot invert without a complement"
                    .to_owned(),
            ),
            FieldTransform::AddField { .. }
            | FieldTransform::DropField { .. }
            | FieldTransform::KeepFields { .. }
            | FieldTransform::MapReferences { .. }
            | FieldTransform::ApplyExpr { .. }
            | FieldTransform::ComputeField { .. } => {
                Some("has a value transform without an operational graph inverse".to_owned())
            }
        };
        if detail.is_some() {
            return detail;
        }
    }
    None
}

/// Every reason `view` lies outside the graph put domain.
///
/// Reports are sorted by variant and then by their graph element. A
/// successful result guarantees that [`put`] has a determined source
/// anchor and edge label for every restored object. Decidable put domains
/// follow the precedents of `kohu2018axiomatic`, Definition 3.2 and Theorems
/// 2.2 and 3.3, and `trankatohu2020programmable`. Returning every violation
/// in this stable order is panproto policy; XSLT (`w3c2017xslt30`, Section
/// 2.14) and BIRDS instead report the first error only.
///
/// # Errors
///
/// Every violation in report order.
pub fn check_put_domain(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<(), Vec<PutDomainError>> {
    let Ok(plan) = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled) else {
        return Ok(());
    };
    let mut found = id_collisions(view, complement);
    found.extend(synthesized_edits(view, complement, &plan));
    match restore_with_plan(lens, view, complement, &plan) {
        Ok(source) => {
            found.extend(forward_stability(lens, view, complement, &source, &plan));
            found.extend(schema_violations(&source, &lens.src_schema));
            found.extend(fan_violations(&source, &lens.src_schema));
        }
        Err(errors) => found.extend(errors),
    }
    sort_violations(&mut found);
    if found.is_empty() { Ok(()) } else { Err(found) }
}

/// A deterministic node id for a single writer adding to a graph view.
///
/// It avoids every id in the view, every id reserved by dropped source
/// objects, and every synthesized presentation id. Concurrent writers must
/// still coordinate: the guarantee is for one writer observing one view.
///
/// # Errors
///
/// [`InstError::NodeIdsExhausted`] when no id below `u32::MAX` remains.
pub fn fresh_view_node_id(
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<u32, InstError> {
    let allocator = IdAllocator::from_floor(IdRange::FULL, complement.id_floor.node);
    let taken = view
        .nodes()
        .keys()
        .copied()
        .chain(complement.dropped_nodes.keys().copied())
        .chain(complement.synthesized_nodes.keys().copied());
    match allocator.fresh(taken) {
        Ok(u32::MAX) | Err(_) => Err(InstError::NodeIdsExhausted),
        Ok(id) => Ok(id),
    }
}

/// A deterministic edge id for a single writer adding to a graph view.
///
/// It avoids every view, reserved and synthesized edge id. Concurrent
/// writers must coordinate separately.
///
/// # Errors
///
/// [`InstError::EdgeIdsExhausted`] when no id below `u32::MAX` remains.
pub fn fresh_view_edge_id(
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<EdgeId, InstError> {
    let allocator = IdAllocator::from_floor(IdRange::FULL, complement.id_floor.edge);
    let taken = view
        .edges()
        .keys()
        .map(|id| id.0)
        .chain(complement.dropped_edges.keys().map(|id| id.0))
        .chain(complement.synthesized_edges.keys().map(|id| id.0));
    match allocator.fresh(taken) {
        Ok(u32::MAX) | Err(_) => Err(InstError::EdgeIdsExhausted),
        Ok(id) => Ok(EdgeId(id)),
    }
}

impl ComplementCompose for GraphComplement {
    fn compose(&self, other: &Self) -> Result<Self, LensError> {
        let (identity, _) = compose_identities(self, other)?;
        let mut result = self.clone();
        merge_map(
            &mut result.dropped_nodes,
            &other.dropped_nodes,
            "dropped_nodes",
            node_equiv,
        )?;
        merge_map(
            &mut result.dropped_edges,
            &other.dropped_edges,
            "dropped_edges",
            PartialEq::eq,
        )?;
        merge_map(
            &mut result.original_edges,
            &other.original_edges,
            "original_edges",
            PartialEq::eq,
        )?;
        merge_map(
            &mut result.original_anchors,
            &other.original_anchors,
            "original_anchors",
            PartialEq::eq,
        )?;
        merge_map(
            &mut result.original_extra_fields,
            &other.original_extra_fields,
            "original_extra_fields",
            btree_values_equiv,
        )?;
        merge_map(
            &mut result.original_values,
            &other.original_values,
            "original_values",
            |a, b| presence_equiv(a.as_ref(), b.as_ref()),
        )?;
        merge_synth_map(
            &mut result.synthesized_nodes,
            &other.synthesized_nodes,
            "synthesized_nodes",
        )?;
        merge_synth_map(
            &mut result.synthesized_edges,
            &other.synthesized_edges,
            "synthesized_edges",
        )?;
        result.id_floor = result.id_floor.max(other.id_floor);
        result.identity = identity;
        Ok(result)
    }

    fn is_compatible(&self, other: &Self) -> bool {
        self.compose(other).is_ok()
    }
}

/// A compiled or deliberately sequential graph composite.
#[derive(Debug)]
pub enum GraphComposite {
    /// One lens proven equivalent to the pair on graphs.
    Compiled(Box<Lens>),
    /// The pair must execute in sequence.
    Sequential(Box<SequentialGraphLens>),
}

/// Two graph lenses executed in sequence.
#[derive(Debug)]
pub struct SequentialGraphLens {
    /// Earlier stage.
    pub first: Lens,
    /// Later stage.
    pub second: Lens,
}

impl GraphComposite {
    /// Run the composite's forward direction.
    ///
    /// # Errors
    ///
    /// A stage's graph get error.
    pub fn get(
        &self,
        source: &GInstance,
    ) -> Result<(GInstance, GraphCompositeComplement), LensError> {
        match self {
            Self::Compiled(lens) => {
                let (view, complement) = get(lens, source)?;
                Ok((
                    view,
                    GraphCompositeComplement::Compiled(Box::new(complement)),
                ))
            }
            Self::Sequential(pair) => {
                let (middle, first) = get(&pair.first, source)?;
                let (view, second) = get(&pair.second, &middle)?;
                Ok((
                    view,
                    GraphCompositeComplement::Sequential(Box::new((first, second))),
                ))
            }
        }
    }

    /// Run the composite's backward direction.
    ///
    /// # Errors
    ///
    /// A mismatched complement form or a stage's graph put error.
    pub fn put(
        &self,
        view: &GInstance,
        complement: &GraphCompositeComplement,
    ) -> Result<GInstance, LensError> {
        match (self, complement) {
            (Self::Compiled(lens), GraphCompositeComplement::Compiled(complement)) => {
                put(lens, view, complement)
            }
            (Self::Sequential(pair), GraphCompositeComplement::Sequential(complements)) => {
                let (first, second) = &**complements;
                let middle = put(&pair.second, view, second)?;
                put(&pair.first, &middle, first)
            }
            _ => Err(LensError::ComplementMismatch {
                detail: "graph composite and complement have different forms".to_owned(),
            }),
        }
    }
}

/// Complement of a [`GraphComposite`].
#[derive(Clone, Debug)]
pub enum GraphCompositeComplement {
    /// Complement of a fused composite.
    Compiled(Box<GraphComplement>),
    /// Complements of the first and second stages.
    Sequential(Box<(GraphComplement, GraphComplement)>),
}

/// Compose two graph lenses using the graph fusion predicate.
///
/// # Errors
///
/// [`LensError::CompositionMismatch`] when the schemas do not meet.
pub fn compose(l1: &Lens, l2: &Lens) -> Result<GraphComposite, LensError> {
    check_boundary(&l1.tgt_schema, &l2.src_schema)?;
    Ok(graph_fusion(l1, l2).map_or_else(
        || {
            GraphComposite::Sequential(Box::new(SequentialGraphLens {
                first: duplicate(l1),
                second: duplicate(l2),
            }))
        },
        |lens| GraphComposite::Compiled(Box::new(lens)),
    ))
}

/// The graph fusion rule used by graph path normalization.
#[must_use]
pub(crate) fn graph_fusion(l1: &Lens, l2: &Lens) -> Option<Lens> {
    tree_fusion(l1, l2)
}

impl Lens {
    /// Compile this lens's total edge-image plan.
    ///
    /// # Errors
    ///
    /// The restrict-plan compilation error.
    pub fn edge_image(&self) -> Result<panproto_inst::EdgeImage, LensError> {
        Ok(RestrictPlan::compile(&self.src_schema, &self.tgt_schema, &self.compiled)?.edge_image)
    }

    /// Compile this lens's contraction plan.
    ///
    /// The path search is the vertex-elimination path lemma of
    /// `rosetarjan1978algorithmic`, Lemma 1; condition C.3 excludes a direct
    /// edge between the endpoints. Writable contractions are contrasted with
    /// the read-only case by `hidaka2010bidirectionalizing`, Theorem 1.
    ///
    /// # Errors
    ///
    /// The restrict-plan compilation error.
    pub fn contraction_plan(&self) -> Result<ContractionPlan, LensError> {
        Ok(RestrictPlan::compile(&self.src_schema, &self.tgt_schema, &self.compiled)?.contraction)
    }

    /// Classify this lens's contraction behavior.
    ///
    /// # Errors
    ///
    /// The restrict-plan compilation error.
    pub fn contraction_mode(&self) -> Result<ContractionMode, LensError> {
        Ok(self.contraction_plan()?.mode())
    }
}

/// A partial injection from new graph ids to prior graph ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Alignment {
    /// New node id to old node id.
    pub nodes: BTreeMap<u32, u32>,
    /// New edge id to old edge id.
    pub edges: BTreeMap<EdgeId, EdgeId>,
}

/// A strategy for aligning a newly loaded view with a previous one.
pub trait Aligner {
    /// Propose a partial injection from `new` ids to `old` ids.
    fn align(&self, old: &GInstance, new: &GInstance) -> Alignment;
}

/// Alignment by equal ids.
#[derive(Clone, Copy, Debug, Default)]
pub struct IdAligner;

impl Aligner for IdAligner {
    fn align(&self, old: &GInstance, new: &GInstance) -> Alignment {
        Alignment {
            nodes: new
                .nodes()
                .keys()
                .filter(|id| old.node(**id).is_some())
                .map(|&id| (id, id))
                .collect(),
            edges: new
                .edges()
                .keys()
                .filter(|id| old.edge(**id).is_some())
                .map(|&id| (id, id))
                .collect(),
        }
    }
}

/// Why an alignment was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AlignError {
    /// Two new objects claim one old id.
    #[error("alignment is not injective for {kind} id {id}")]
    NotInjective {
        /// `node` or `edge`.
        kind: &'static str,
        /// Reused old id.
        id: u32,
    },
    /// An alignment names an object absent from one side.
    #[error("alignment names a missing {kind} id {id}")]
    Missing {
        /// `old node`, `new node`, `old edge` or `new edge`.
        kind: &'static str,
        /// Missing id.
        id: u32,
    },
    /// Self-alignment is not the identity.
    #[error("self-alignment changes {kind} id {from} to {to}")]
    SelfMismatch {
        /// `node` or `edge`.
        kind: &'static str,
        /// New id.
        from: u32,
        /// Proposed old id.
        to: u32,
    },
    /// Rekeying would collide with an unaligned object.
    #[error("alignment rekeys {kind} id {from} onto occupied id {to}")]
    Collision {
        /// `node` or `edge`.
        kind: &'static str,
        /// New id.
        from: u32,
        /// Old id.
        to: u32,
    },
    /// The aligned graphs have incompatible object content.
    #[error("aligned {kind} {id} differs from the previous view")]
    ContentMismatch {
        /// `node` or `edge`.
        kind: &'static str,
        /// New id.
        id: u32,
    },
}

/// Check the identity law of an aligner on `view`.
///
/// # Errors
///
/// A missing, non-injective, content-changing or non-identity mapping.
pub fn check_aligner(view: &GInstance, aligner: &dyn Aligner) -> Result<(), AlignError> {
    let alignment = aligner.align(view, view);
    validate_alignment(view, view, &alignment)?;
    for &id in view.nodes().keys() {
        if !alignment.nodes.contains_key(&id) {
            return Err(AlignError::Missing {
                kind: "self node",
                id,
            });
        }
    }
    for &id in view.edges().keys() {
        if !alignment.edges.contains_key(&id) {
            return Err(AlignError::Missing {
                kind: "self edge",
                id: id.0,
            });
        }
    }
    for (&from, &to) in &alignment.nodes {
        if from != to {
            return Err(AlignError::SelfMismatch {
                kind: "node",
                from,
                to,
            });
        }
    }
    for (&from, &to) in &alignment.edges {
        if from != to {
            return Err(AlignError::SelfMismatch {
                kind: "edge",
                from: from.0,
                to: to.0,
            });
        }
    }
    Ok(())
}

/// Align `view` to durable ids in `previous_view`.
///
/// # Errors
///
/// A proposed mapping is not a content-preserving partial injection or its
/// rekeying collides with an unaligned object.
pub fn align(
    view: &GInstance,
    previous_view: &GInstance,
    aligner: &dyn Aligner,
) -> Result<GInstance, AlignError> {
    let alignment = aligner.align(previous_view, view);
    validate_alignment(previous_view, view, &alignment)?;
    rekey(view, &alignment)
}

/// The BFS-tree witness for each contraction edge synthesized by `get`.
///
/// The path is derived from the source and is never complement residue.
/// Bounded regions cost their region depth; an unbounded dropped SCC may
/// require visiting the whole SCC, so the cost is not bounded by edit size.
/// This writable witness is the contrast with the read-only bidirectional
/// transformation of `hidaka2010bidirectionalizing`, Theorem 1.
///
/// # Errors
///
/// Plan compilation or graph restrict failure.
pub fn contraction_witnesses(
    lens: &Lens,
    source: &GInstance,
) -> Result<BTreeMap<SynthKey, Vec<EdgeId>>, LensError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;
    let (view, complement) = graph_restrict_with_plan(source, &plan)?;
    let wanted: BTreeSet<SynthKey> = complement
        .synthesized_edges
        .values()
        .filter(|key| {
            matches!(
                key,
                SynthKey::Contract { .. } | SynthKey::ContractExpand { .. }
            )
        })
        .cloned()
        .collect();
    let mut result = BTreeMap::new();
    for key in wanted {
        let endpoints = match &key {
            SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
                Some((*src, *tgt))
            }
            SynthKey::Expand { .. }
            | SynthKey::ResolutionReference { .. }
            | SynthKey::ResolutionSpan { .. } => None,
        };
        if let Some((src, tgt)) = endpoints
            && let Some(path) = bfs_contraction_path(source, &view, src, tgt)
        {
            result.insert(key, path);
        }
    }
    Ok(result)
}

fn restore(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<GInstance, Vec<PutDomainError>> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
        .map_err(|_| Vec::new())?;
    restore_with_plan(lens, view, complement, &plan)
}

fn restore_with_plan(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    plan: &RestrictPlan,
) -> Result<GInstance, Vec<PutDomainError>> {
    let reverse = reverse_anchors(lens);
    let mut source = GInstance::new();
    let mut errors = Vec::new();
    let mut ids: Vec<u32> = view.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        let node = &view.nodes()[&id];
        let candidates = reverse.get(&node.anchor).cloned().unwrap_or_else(|| {
            if (lens.src_schema.vertices.contains_key(&node.anchor)
                && lens.tgt_schema.vertices.contains_key(&node.anchor)
                && lens.compiled.surviving_verts.contains(&node.anchor))
                || (lens.src_schema.hyper_edges.contains_key(&node.anchor)
                    && lens.tgt_schema.hyper_edges.contains_key(&node.anchor))
            {
                vec![node.anchor.clone()]
            } else {
                Vec::new()
            }
        });
        let recorded = complement.original_anchors.get(&id).filter(|recorded| {
            recorded_anchor_maps_to(lens, view, complement, id, recorded, &node.anchor)
        });
        let Some(anchor) = recorded.or_else(|| single(&candidates)) else {
            errors.push(PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Node(id),
                candidates,
            });
            continue;
        };
        let anchor = anchor.clone();
        let mut restored = node.clone();
        restored.anchor = anchor;
        let transforms = lens.compiled.field_transforms.get(&restored.anchor);
        if let Some(original) = complement.original_extra_fields.get(&id) {
            let view_fields = restored.extra_fields.clone();
            restored.extra_fields = original
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if let Some(transforms) = transforms {
                propagate_view_edits_through_inverse(&mut restored, &view_fields, transforms);
            }
        } else if let Some(transforms) = transforms {
            apply_inverse_transforms(&mut restored, transforms);
        }
        if let Some(value) = complement.original_values.get(&id) {
            restored.value.clone_from(value);
        }
        let _ = source.insert_node(restored);
    }
    for (&id, node) in &complement.dropped_nodes {
        if source.node(id).is_none() {
            let _ = source.insert_node(node.clone());
        }
    }
    invert_graph_child_scalars(lens, view, &mut source);
    let mut edge_ids: Vec<EdgeId> = view.edges().keys().copied().collect();
    edge_ids.sort_unstable();
    for id in edge_ids {
        if complement.synthesized_edges.contains_key(&id) {
            continue;
        }
        let edge = &view.edges()[&id];
        if source.node(edge.src).is_none() || source.node(edge.tgt).is_none() {
            continue;
        }
        match source_edge(lens, view, complement, plan, id, edge, &source) {
            Ok(label) => {
                let restored = GEdge {
                    src: edge.src,
                    tgt: edge.tgt,
                    edge: label,
                    position: edge.position,
                };
                if source.insert_edge(id, restored).is_err() {
                    errors.push(PutDomainError::IdCollision {
                        elem: ViewRef::Edge(id),
                    });
                }
            }
            Err(error) => errors.push(*error),
        }
    }
    for (&id, edge) in &complement.dropped_edges {
        if source.node(edge.src).is_some()
            && source.node(edge.tgt).is_some()
            && source.edge(id).is_none()
        {
            let _ = source.insert_edge(id, edge.clone());
        }
    }
    if errors.is_empty() {
        Ok(source)
    } else {
        Err(errors)
    }
}

fn invert_graph_child_scalars(lens: &Lens, view: &GInstance, source: &mut GInstance) {
    use panproto_inst::{FieldPresence, FieldTransform};

    let mut parent_ids: Vec<u32> = view.nodes().keys().copied().collect();
    parent_ids.sort_unstable();
    for parent_id in parent_ids {
        let Some(parent) = view.node(parent_id) else {
            continue;
        };
        let Some(source_anchor) = source.node(parent_id).map(|node| node.anchor.clone()) else {
            continue;
        };
        let Some(transforms) = lens.compiled.field_transforms.get(&source_anchor) else {
            continue;
        };
        let child_of: HashMap<String, (u32, panproto_inst::Value)> = view
            .out_edges(parent_id)
            .iter()
            .filter_map(|id| {
                let edge = view.edge(*id)?;
                let child = view.node(edge.tgt)?;
                let Some(FieldPresence::Present(value)) = &child.value else {
                    return None;
                };
                let field = edge
                    .edge
                    .name
                    .as_ref()
                    .map_or_else(|| edge.edge.tgt.to_string(), Name::to_string);
                Some((field, (edge.tgt, value.clone())))
            })
            .collect();
        let env_fields: HashMap<String, panproto_inst::Value> = child_of
            .iter()
            .map(|(name, (_, value))| (name.clone(), value.clone()))
            .collect();
        for transform in transforms {
            let FieldTransform::ComputeField {
                target_key,
                expr,
                inverse: Some(inverse),
                ..
            } = transform
            else {
                continue;
            };
            if parent.extra_fields.contains_key(target_key) || !child_of.contains_key(target_key) {
                continue;
            }
            let Some(source_key) = crate::derived::sole_source_var(expr) else {
                continue;
            };
            let Some((target_child, _)) = child_of.get(&source_key) else {
                continue;
            };
            let env = panproto_inst::build_env_from_extra_fields(&env_fields);
            if let Ok(result) =
                panproto_expr::eval(inverse, &env, &panproto_expr::EvalConfig::default())
                && let Some(node) = source.node_mut(*target_child)
            {
                node.value = Some(FieldPresence::Present(
                    panproto_inst::expr_literal_to_value(&result),
                ));
            }
        }
    }
}

fn source_edge(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    plan: &RestrictPlan,
    id: EdgeId,
    edge: &GEdge,
    source: &GInstance,
) -> Result<Edge, Box<PutDomainError>> {
    if let Some(recorded) = complement.original_edges.get(&id)
        && recorded.src == edge.src
        && recorded.tgt == edge.tgt
        && (matches!(plan.edge_image.fate(&recorded.edge), Some(EdgeFate::Keep(image)) if image == &edge.edge)
            || expected_view_fan_label(lens, view, complement, recorded).as_ref()
                == Some(&edge.edge))
    {
        return Ok(recorded.edge.clone());
    }
    if is_fan_edge(&edge.edge) {
        return fan_edge_back(lens, view, complement, edge, source).ok_or_else(|| {
            Box::new(PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(id),
            })
        });
    }
    let src_anchor = &source.nodes()[&edge.src].anchor;
    let tgt_anchor = &source.nodes()[&edge.tgt].anchor;
    let candidates: Vec<&Edge> = plan
        .edge_image
        .preimages(&edge.edge)
        .into_iter()
        .filter(|candidate| &candidate.src == src_anchor && &candidate.tgt == tgt_anchor)
        .collect();
    match candidates.as_slice() {
        [only] => Ok((*only).clone()),
        [] if plan.contraction.pair(src_anchor, tgt_anchor).is_some_and(
            |pair| matches!(&pair.resolution, PairResolution::Label(label) if label == &edge.edge),
        ) =>
        {
            Err(Box::new(PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Edge(id),
            }))
        }
        _ => Err(Box::new(PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Edge(id),
        })),
    }
}

fn expected_view_fan_label(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    recorded: &GEdge,
) -> Option<Edge> {
    if !is_fan_edge(&recorded.edge) {
        return None;
    }
    let fan = complement
        .original_anchors
        .get(&recorded.src)
        .or_else(|| view.node(recorded.src).map(|node| &node.anchor))?;
    let target = view.node(recorded.tgt)?;
    let (surviving, full) = source_fan_shapes(view, complement, recorded.src);
    let entry = FanResolver::new(&lens.compiled).resolve(fan, &surviving, &full);
    let hyper = entry.map_or_else(|| fan.clone(), |(hyper, _)| hyper.clone());
    let name = if recorded.edge.kind == HYPER_PARENT_KIND {
        recorded.edge.name.as_ref().map(|_| {
            lens.tgt_schema.hyper_edges.get(&hyper).map_or_else(
                || recorded.edge.name.clone().unwrap_or_default(),
                |target_hyper| target_hyper.parent_label.clone(),
            )
        })
    } else {
        recorded.edge.name.as_ref().map(|name| {
            entry
                .and_then(|(_, labels)| labels.get(name))
                .cloned()
                .unwrap_or_else(|| name.clone())
        })
    };
    Some(Edge {
        src: hyper,
        tgt: target.anchor.clone(),
        kind: recorded.edge.kind.clone(),
        name,
    })
}

fn source_fan_shapes(
    view: &GInstance,
    complement: &GraphComplement,
    fan: u32,
) -> (Vec<Name>, Vec<Name>) {
    let mut surviving = Vec::new();
    let mut full = Vec::new();
    for (&id, edge) in view.edges() {
        if edge.src != fan || edge.edge.kind != HYPER_LABEL_KIND {
            continue;
        }
        let source = complement.original_edges.get(&id).unwrap_or(edge);
        if let Some(name) = &source.edge.name {
            surviving.push(name.clone());
            full.push(name.clone());
        }
    }
    full.extend(
        complement
            .dropped_edges
            .values()
            .filter(|edge| edge.src == fan && edge.edge.kind == HYPER_LABEL_KIND)
            .filter_map(|edge| edge.edge.name.clone()),
    );
    (
        canonical_label_shape(surviving),
        canonical_label_shape(full),
    )
}

fn recorded_anchor_maps_to(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    id: u32,
    recorded: &Name,
    view_anchor: &Name,
) -> bool {
    if lens.compiled.vertex_remap.get(recorded).unwrap_or(recorded) == view_anchor {
        return true;
    }
    if !lens.src_schema.hyper_edges.contains_key(recorded) {
        return false;
    }
    let (surviving, full) = source_fan_shapes(view, complement, id);
    FanResolver::new(&lens.compiled)
        .resolve(recorded, &surviving, &full)
        .is_some_and(|(target, _)| target == view_anchor)
}

fn fan_edge_back(
    lens: &Lens,
    _view: &GInstance,
    _complement: &GraphComplement,
    edge: &GEdge,
    source: &GInstance,
) -> Option<Edge> {
    let fan = source.node(edge.src)?;
    let target = source.node(edge.tgt)?;
    let name = if edge.edge.kind.as_str() == "hyper_parent" {
        edge.edge.name.as_ref().map(|_| {
            lens.src_schema.hyper_edges.get(&fan.anchor).map_or_else(
                || edge.edge.name.clone().unwrap_or_default(),
                |hyper| hyper.parent_label.clone(),
            )
        })
    } else {
        edge.edge.name.clone()
    };
    let label = Edge {
        src: fan.anchor.clone(),
        tgt: target.anchor.clone(),
        kind: edge.edge.kind.clone(),
        name,
    };
    let hyper = lens.src_schema.hyper_edges.get(&fan.anchor)?;
    let valid = if label.kind.as_str() == "hyper_parent" {
        true
    } else {
        label
            .name
            .as_ref()
            .is_some_and(|name| hyper.signature.get(name) == Some(&target.anchor))
    };
    valid.then_some(label)
}

fn id_collisions(view: &GInstance, complement: &GraphComplement) -> Vec<PutDomainError> {
    let mut found = Vec::new();
    found.extend(
        view.nodes()
            .keys()
            .filter(|id| complement.dropped_nodes.contains_key(id))
            .map(|&id| PutDomainError::IdCollision {
                elem: ViewRef::Node(id),
            }),
    );
    found.extend(
        view.edges()
            .keys()
            .filter(|id| complement.dropped_edges.contains_key(id))
            .map(|&id| PutDomainError::IdCollision {
                elem: ViewRef::Edge(id),
            }),
    );
    found
}

fn synthesized_edits(
    view: &GInstance,
    complement: &GraphComplement,
    plan: &RestrictPlan,
) -> Vec<PutDomainError> {
    let removed_nodes: HashSet<u32> = complement
        .synthesized_nodes
        .iter()
        .filter_map(|(&id, key)| {
            (!synth_node_matches(view, complement, plan, id, key)).then_some(id)
        })
        .collect();
    let mut found: Vec<PutDomainError> = removed_nodes
        .iter()
        .filter(|&&id| {
            !chain_endpoint_removed(view, complement, &complement.synthesized_nodes[&id])
        })
        .map(|&id| PutDomainError::SynthesizedEdited {
            elem: ViewRef::Node(id),
        })
        .collect();
    for (&id, key) in &complement.synthesized_edges {
        if !synth_edge_matches(view, complement, plan, id, key)
            && !chain_endpoint_removed(view, complement, key)
        {
            found.push(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(id),
            });
        }
    }
    found
}

fn synth_node_matches(
    view: &GInstance,
    complement: &GraphComplement,
    plan: &RestrictPlan,
    id: u32,
    key: &SynthKey,
) -> bool {
    let Some(node) = view.node(id) else {
        return false;
    };
    let expected = match key {
        SynthKey::Expand { source_edge, hop } => complement
            .dropped_edges
            .get(source_edge)
            .and_then(|source| match plan.edge_image.fate(&source.edge) {
                Some(EdgeFate::Expand { intermediates, .. }) => {
                    intermediates.get(usize::from(*hop))
                }
                _ => None,
            }),
        SynthKey::ContractExpand { src, tgt, hop } => {
            let endpoints = view.node(*src).zip(view.node(*tgt));
            endpoints.and_then(|(src, tgt)| {
                plan.contraction.pairs.values().find_map(|pair| {
                    (pair.target == (src.anchor.clone(), tgt.anchor.clone()))
                        .then_some(&pair.resolution)
                        .and_then(|resolution| match resolution {
                            PairResolution::Expand { intermediates, .. } => {
                                intermediates.get(usize::from(*hop))
                            }
                            _ => None,
                        })
                })
            })
        }
        SynthKey::Contract { .. }
        | SynthKey::ResolutionReference { .. }
        | SynthKey::ResolutionSpan { .. } => None,
    };
    expected.is_some_and(|anchor| {
        &node.anchor == anchor
            && node.value.is_none()
            && node.extra_fields.is_empty()
            && node.discriminator.is_none()
            && node.position.is_none()
            && node.annotations.is_empty()
    })
}

fn synth_edge_matches(
    view: &GInstance,
    complement: &GraphComplement,
    plan: &RestrictPlan,
    id: EdgeId,
    key: &SynthKey,
) -> bool {
    let Some(edge) = view.edge(id) else {
        return false;
    };
    let expected = expected_synth_edge(view, complement, plan, key);
    expected.is_some_and(|(src, tgt, label)| {
        edge.src == src && edge.tgt == tgt && edge.edge == label && edge.position.is_none()
    })
}

fn expected_synth_edge(
    view: &GInstance,
    complement: &GraphComplement,
    plan: &RestrictPlan,
    key: &SynthKey,
) -> Option<(u32, u32, Edge)> {
    match key {
        SynthKey::Contract { src, tgt, edge } => Some((*src, *tgt, edge.clone())),
        SynthKey::Expand { source_edge, hop } => {
            let source = complement.dropped_edges.get(source_edge)?;
            let EdgeFate::Expand {
                intermediates,
                labels,
            } = plan.edge_image.fate(&source.edge)?
            else {
                return None;
            };
            chain_hop(
                source.src,
                source.tgt,
                intermediates.len(),
                labels,
                |h| {
                    complement.synth_node_id(&SynthKey::Expand {
                        source_edge: *source_edge,
                        hop: h,
                    })
                },
                *hop,
            )
        }
        SynthKey::ContractExpand { src, tgt, hop } => {
            let src_anchor = view.node(*src)?.anchor.clone();
            let tgt_anchor = view.node(*tgt)?.anchor.clone();
            let PairResolution::Expand {
                intermediates,
                labels,
            } = &plan
                .contraction
                .pairs
                .values()
                .find(|pair| pair.target == (src_anchor.clone(), tgt_anchor.clone()))?
                .resolution
            else {
                return None;
            };
            chain_hop(
                *src,
                *tgt,
                intermediates.len(),
                labels,
                |h| {
                    complement.synth_node_id(&SynthKey::ContractExpand {
                        src: *src,
                        tgt: *tgt,
                        hop: h,
                    })
                },
                *hop,
            )
        }
        SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => None,
    }
}

fn chain_hop(
    src: u32,
    tgt: u32,
    inner_len: usize,
    labels: &[Edge],
    node: impl Fn(u16) -> Option<u32>,
    hop: u16,
) -> Option<(u32, u32, Edge)> {
    let h = usize::from(hop);
    let from = if h == 0 { src } else { node(hop - 1)? };
    let to = if h == inner_len { tgt } else { node(hop)? };
    Some((from, to, labels.get(h)?.clone()))
}

fn chain_endpoint_removed(view: &GInstance, complement: &GraphComplement, key: &SynthKey) -> bool {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            view.node(*src).is_none() || view.node(*tgt).is_none()
        }
        SynthKey::Expand { source_edge, .. } => complement
            .dropped_edges
            .get(source_edge)
            .is_some_and(|edge| view.node(edge.src).is_none() || view.node(edge.tgt).is_none()),
        SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => false,
    }
}

fn forward_stability(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    source: &GInstance,
    plan: &RestrictPlan,
) -> Vec<PutDomainError> {
    let regenerated = match graph_restrict_with_plan(source, plan) {
        Ok((regenerated, _)) => regenerated,
        Err(error) => return refusal_from_restrict(error),
    };
    let mut found = Vec::new();
    for (&id, node) in view.nodes() {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        let Some(actual) = regenerated.node(id) else {
            found.push(PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Node(id),
            });
            continue;
        };
        if !node_equiv(node, actual) {
            let source_anchor = source.node(id).map(|node| &node.anchor);
            classify_transform_differences(lens, id, source_anchor, node, actual, &mut found);
        }
    }
    found
}

fn refusal_from_restrict(error: panproto_inst::RestrictError) -> Vec<PutDomainError> {
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
        _ => Vec::new(),
    }
}

fn classify_transform_differences(
    lens: &Lens,
    id: u32,
    source_anchor: Option<&Name>,
    expected: &panproto_inst::Node,
    actual: &panproto_inst::Node,
    found: &mut Vec<PutDomainError>,
) {
    let source_anchor = source_anchor.unwrap_or(&expected.anchor);
    let transforms = lens.compiled.field_transforms.get(source_anchor);
    let Some(transforms) = transforms else {
        return;
    };
    for transform in transforms {
        use panproto_inst::FieldTransform;
        let (field, invertible, derived) = match transform {
            FieldTransform::AddField { key, .. } => (key.as_str(), false, true),
            FieldTransform::ComputeField {
                target_key,
                inverse,
                ..
            } => (target_key.as_str(), inverse.is_some(), true),
            FieldTransform::ApplyExpr { key, inverse, .. } => {
                (key.as_str(), inverse.is_some(), false)
            }
            _ => continue,
        };
        let differs = if field == "__value__" {
            !presence_equiv(expected.value.as_ref(), actual.value.as_ref())
        } else {
            match (
                expected.extra_fields.get(field),
                actual.extra_fields.get(field),
            ) {
                (Some(a), Some(b)) => !extra_fields_equiv(
                    &HashMap::from([(field.to_owned(), a.clone())]),
                    &HashMap::from([(field.to_owned(), b.clone())]),
                ),
                (None, None) => false,
                _ => true,
            }
        };
        if differs && (derived || !invertible) {
            found.push(if derived {
                PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Node(id),
                    field: field.to_owned(),
                }
            } else {
                PutDomainError::TransformNotInvertible {
                    elem: ViewRef::Node(id),
                    field: field.to_owned(),
                }
            });
        }
    }
}

fn fan_violations(source: &GInstance, schema: &panproto_schema::Schema) -> Vec<PutDomainError> {
    let mut nodes: Vec<u32> = source.nodes().keys().copied().collect();
    nodes.sort_unstable();
    nodes
        .into_iter()
        .flat_map(|node| {
            panproto_inst::FanDomain::violations_at(source, node, Some(schema))
                .into_iter()
                .map(move |reason| PutDomainError::MalformedFan { node, reason })
        })
        .collect()
}

fn schema_violations(source: &GInstance, schema: &panproto_schema::Schema) -> Vec<PutDomainError> {
    let mut found = required_violations(source, schema);
    found.extend(key_violations(source, schema));
    found
}

fn required_violations(
    source: &GInstance,
    schema: &panproto_schema::Schema,
) -> Vec<PutDomainError> {
    let mut found = Vec::new();
    let mut ids: Vec<u32> = source.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &source.nodes()[&id];
        let Some(required) = schema.required.get(&node.anchor) else {
            continue;
        };
        let mut required = required.clone();
        required.sort();
        for edge in required {
            let has_edge = source
                .out_edges(id)
                .iter()
                .any(|edge_id| source.edges()[edge_id].edge == edge);
            let field = edge
                .name
                .as_ref()
                .map_or_else(|| edge.tgt.to_string(), Name::to_string);
            let has_field = node.extra_fields.contains_key(&field);
            let has_default = schema.defaults.contains_key(&edge.tgt);
            if !has_edge && !has_field && !has_default {
                found.push(PutDomainError::MissingRequiredCell {
                    elem: ViewRef::Node(id),
                    field,
                });
            }
        }
    }
    found
}

fn key_violations(source: &GInstance, schema: &panproto_schema::Schema) -> Vec<PutDomainError> {
    let mut found = Vec::new();
    for (vertex, keys) in &schema.declarations.keys {
        for key in keys {
            let mut buckets: KeyBuckets = BTreeMap::new();
            let mut ids: Vec<u32> = source
                .nodes()
                .iter()
                .filter_map(|(&id, node)| (&node.anchor == vertex).then_some(id))
                .collect();
            ids.sort_unstable();
            for id in ids {
                let Some(values) = key_values(source, id, &key.parts) else {
                    continue;
                };
                let scope = key_scope(source, id, &key.scope);
                let bytes = key_bytes(&values);
                let entry = buckets
                    .entry((scope, bytes))
                    .or_insert_with(|| (values, Vec::new()));
                entry.1.push(id);
            }
            for (_, (values, ids)) in buckets.into_iter().filter(|(_, (_, ids))| ids.len() > 1) {
                let named = key.parts.iter().map(key_part_name).zip(values).collect();
                found.push(PutDomainError::DuplicateKey {
                    table: vertex.clone(),
                    key: named,
                    elems: ids.into_iter().map(ViewRef::Node).collect(),
                });
            }
        }
    }
    found
}

fn key_values(
    source: &GInstance,
    node: u32,
    parts: &[panproto_schema::KeyPart],
) -> Option<Vec<panproto_inst::Value>> {
    parts
        .iter()
        .map(|part| match part {
            panproto_schema::KeyPart::Path(path) => path_value(source, node, path),
            panproto_schema::KeyPart::Position { within } => graph_position(source, node, within)
                .map(|position| panproto_inst::Value::Int(i64::from(position))),
        })
        .collect()
}

fn graph_position(source: &GInstance, node: u32, within: &Edge) -> Option<u32> {
    let id = source
        .in_edges(node)
        .iter()
        .copied()
        .find(|id| source.edge(*id).is_some_and(|edge| &edge.edge == within))?;
    let edge = source.edge(id)?;
    if let Some(position) = edge.position {
        return Some(position);
    }
    let index = source
        .ordered_out_edges(edge.src)
        .into_iter()
        .filter(|candidate| {
            source
                .edge(*candidate)
                .is_some_and(|edge| &edge.edge == within)
        })
        .position(|candidate| candidate == id)?;
    u32::try_from(index).ok()
}

fn path_value(source: &GInstance, start: u32, path: &[Edge]) -> Option<panproto_inst::Value> {
    let Some((last, prefix)) = path.split_last() else {
        return source
            .node(start)?
            .value
            .as_ref()
            .and_then(presence_value)
            .cloned();
    };
    let mut frontier = vec![start];
    for hop in prefix {
        frontier = frontier
            .into_iter()
            .flat_map(|node| source.out_edges(node).iter().copied())
            .filter_map(|id| {
                let edge = &source.edges()[&id];
                (edge.edge == *hop).then_some(edge.tgt)
            })
            .collect();
    }
    let mut reached: Vec<u32> = frontier
        .iter()
        .flat_map(|node| source.out_edges(*node).iter().copied())
        .filter_map(|id| {
            let edge = &source.edges()[&id];
            (edge.edge == *last).then_some(edge.tgt)
        })
        .collect();
    reached.sort_unstable();
    if let Some(id) = reached.first() {
        source
            .node(*id)?
            .value
            .as_ref()
            .and_then(presence_value)
            .cloned()
    } else {
        let field = last
            .name
            .as_ref()
            .map_or_else(|| last.tgt.to_string(), Name::to_string);
        frontier.sort_unstable();
        frontier
            .into_iter()
            .find_map(|id| source.node(id)?.extra_fields.get(&field).cloned())
    }
}

const fn presence_value(value: &panproto_inst::FieldPresence) -> Option<&panproto_inst::Value> {
    match value {
        panproto_inst::FieldPresence::Present(value) => Some(value),
        panproto_inst::FieldPresence::Null | panproto_inst::FieldPresence::Absent => None,
    }
}

fn key_scope(source: &GInstance, start: u32, scope: &panproto_schema::KeyScope) -> Option<u32> {
    let panproto_schema::KeyScope::Within(vertex) = scope else {
        return None;
    };
    let mut seen = BTreeSet::from([start]);
    let mut frontier = vec![start];
    while !frontier.is_empty() {
        let mut next: Vec<u32> = frontier
            .iter()
            .flat_map(|id| source.in_edges(*id).iter().copied())
            .map(|edge| source.edges()[&edge].src)
            .filter(|id| seen.insert(*id))
            .collect();
        next.sort_unstable();
        if let Some(found) = next
            .iter()
            .find(|id| source.node(**id).is_some_and(|node| &node.anchor == vertex))
        {
            return Some(*found);
        }
        frontier = next;
    }
    None
}

fn key_bytes(values: &[panproto_inst::Value]) -> Vec<u8> {
    let mut writer = panproto_schema::CanonicalWriter::new();
    writer.len(values.len());
    for value in values {
        writer.value(value);
    }
    writer.into_bytes()
}

fn key_part_name(part: &panproto_schema::KeyPart) -> String {
    let edge_name = |edge: &Edge| {
        edge.name
            .as_ref()
            .map_or_else(|| edge.tgt.to_string(), Name::to_string)
    };
    match part {
        panproto_schema::KeyPart::Path(path) => {
            path.iter().map(edge_name).collect::<Vec<_>>().join("/")
        }
        panproto_schema::KeyPart::Position { within } => {
            format!("position({})", edge_name(within))
        }
    }
}

fn reverse_anchors(lens: &Lens) -> BTreeMap<Name, Vec<Name>> {
    let mut reverse: BTreeMap<Name, Vec<Name>> = BTreeMap::new();
    let mut sources: Vec<Name> = lens.src_schema.vertices.keys().cloned().collect();
    sources.sort();
    for source in sources {
        let image = lens.compiled.vertex_remap.get(&source).unwrap_or(&source);
        if lens.compiled.surviving_verts.contains(image) {
            reverse.entry(image.clone()).or_default().push(source);
        }
    }
    reverse
}

const fn single<T>(values: &[T]) -> Option<&T> {
    match values {
        [only] => Some(only),
        _ => None,
    }
}

fn is_fan_edge(edge: &Edge) -> bool {
    matches!(edge.kind.as_str(), "hyper_parent" | "hyper_label")
}

fn first_put_error(mut errors: Vec<PutDomainError>) -> LensError {
    sort_violations(&mut errors);
    errors.into_iter().next().map_or_else(
        || LensError::ComplementMismatch {
            detail: "graph restoration was not determined".to_owned(),
        },
        |error| LensError::PutDomain(Box::new(error)),
    )
}

fn sort_violations(found: &mut Vec<PutDomainError>) {
    found.sort_by(|a, b| {
        a.variant_index()
            .cmp(&b.variant_index())
            .then_with(|| violation_ref(a).cmp(&violation_ref(b)))
            .then_with(|| a.to_string().cmp(&b.to_string()))
    });
    found.dedup();
}

fn violation_ref(error: &PutDomainError) -> Option<ViewRef> {
    match error {
        PutDomainError::IdCollision { elem }
        | PutDomainError::SynthesizedEdited { elem }
        | PutDomainError::ContractedEdgeInserted { elem }
        | PutDomainError::UnrepresentableViewEdge { elem }
        | PutDomainError::AmbiguousReverseAnchor { elem, .. }
        | PutDomainError::SurvivalPredicateFails { elem }
        | PutDomainError::DerivedCellEdited { elem, .. }
        | PutDomainError::TransformNotInvertible { elem, .. }
        | PutDomainError::MissingRequiredCell { elem, .. }
        | PutDomainError::SurvivalPredicateRefused { elem, .. } => Some(elem.clone()),
        PutDomainError::DuplicateKey { elems, .. } => elems.first().cloned(),
        PutDomainError::MalformedFan { node, .. } => Some(ViewRef::Node(*node)),
    }
}

fn merge_map<K, V, F>(
    target: &mut BTreeMap<K, V>,
    source: &BTreeMap<K, V>,
    kind: &'static str,
    equal: F,
) -> Result<(), LensError>
where
    K: Ord + Clone + std::fmt::Debug,
    V: Clone,
    F: Fn(&V, &V) -> bool,
{
    for (key, value) in source {
        match target.get(key) {
            None => {
                target.insert(key.clone(), value.clone());
            }
            Some(existing) if equal(existing, value) => {}
            Some(_) => {
                return Err(LensError::ComplementConflict {
                    kind,
                    key: format!("{key:?}"),
                });
            }
        }
    }
    Ok(())
}

fn merge_synth_map<K: Ord + Clone + std::fmt::Debug>(
    target: &mut BTreeMap<K, SynthKey>,
    source: &BTreeMap<K, SynthKey>,
    kind: &'static str,
) -> Result<(), LensError> {
    let mut inverse: BTreeMap<SynthKey, K> = target
        .iter()
        .map(|(id, key)| (key.clone(), id.clone()))
        .collect();
    for (id, key) in source {
        let conflicts = target.get(id).is_some_and(|mine| mine != key)
            || inverse.get(key).is_some_and(|mine| mine != id);
        if conflicts {
            return Err(LensError::ComplementConflict {
                kind,
                key: format!("{id:?} {key:?}"),
            });
        }
        target.insert(id.clone(), key.clone());
        inverse.insert(key.clone(), id.clone());
    }
    Ok(())
}

fn btree_values_equiv(
    a: &BTreeMap<String, panproto_inst::Value>,
    b: &BTreeMap<String, panproto_inst::Value>,
) -> bool {
    let a: HashMap<String, panproto_inst::Value> =
        a.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let b: HashMap<String, panproto_inst::Value> =
        b.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    extra_fields_equiv(&a, &b)
}

fn validate_alignment(
    old: &GInstance,
    new: &GInstance,
    alignment: &Alignment,
) -> Result<(), AlignError> {
    let mut old_nodes = BTreeSet::new();
    for (&new_id, &old_id) in &alignment.nodes {
        let new_node = new.node(new_id).ok_or(AlignError::Missing {
            kind: "new node",
            id: new_id,
        })?;
        let old_node = old.node(old_id).ok_or(AlignError::Missing {
            kind: "old node",
            id: old_id,
        })?;
        if !old_nodes.insert(old_id) {
            return Err(AlignError::NotInjective {
                kind: "node",
                id: old_id,
            });
        }
        let mut rekeyed = new_node.clone();
        rekeyed.id = old_id;
        if !node_equiv(&rekeyed, old_node) {
            return Err(AlignError::ContentMismatch {
                kind: "node",
                id: new_id,
            });
        }
    }
    let mut old_edges = BTreeSet::new();
    for (&new_id, &old_id) in &alignment.edges {
        let new_edge = new.edge(new_id).ok_or(AlignError::Missing {
            kind: "new edge",
            id: new_id.0,
        })?;
        let old_edge = old.edge(old_id).ok_or(AlignError::Missing {
            kind: "old edge",
            id: old_id.0,
        })?;
        if !old_edges.insert(old_id) {
            return Err(AlignError::NotInjective {
                kind: "edge",
                id: old_id.0,
            });
        }
        let endpoints = (
            alignment
                .nodes
                .get(&new_edge.src)
                .copied()
                .unwrap_or(new_edge.src),
            alignment
                .nodes
                .get(&new_edge.tgt)
                .copied()
                .unwrap_or(new_edge.tgt),
        );
        if (old_edge.src, old_edge.tgt) != endpoints
            || old_edge.edge != new_edge.edge
            || old_edge.position != new_edge.position
        {
            return Err(AlignError::ContentMismatch {
                kind: "edge",
                id: new_id.0,
            });
        }
    }
    Ok(())
}

fn rekey(view: &GInstance, alignment: &Alignment) -> Result<GInstance, AlignError> {
    let mut result = GInstance::new();
    let mapped_nodes: BTreeSet<u32> = alignment.nodes.values().copied().collect();
    for node in view.nodes().values() {
        let id = alignment.nodes.get(&node.id).copied().unwrap_or(node.id);
        if id != node.id && view.node(id).is_some() && !alignment.nodes.contains_key(&id) {
            return Err(AlignError::Collision {
                kind: "node",
                from: node.id,
                to: id,
            });
        }
        if result.node(id).is_some() || (!mapped_nodes.contains(&id) && id != node.id) {
            return Err(AlignError::Collision {
                kind: "node",
                from: node.id,
                to: id,
            });
        }
        let mut copy = node.clone();
        copy.id = id;
        let _ = result.insert_node(copy);
    }
    for (id, edge) in view.edges_sorted() {
        let new_id = alignment.edges.get(&id).copied().unwrap_or(id);
        if result.edge(new_id).is_some() {
            return Err(AlignError::Collision {
                kind: "edge",
                from: id.0,
                to: new_id.0,
            });
        }
        let copy = GEdge {
            src: alignment.nodes.get(&edge.src).copied().unwrap_or(edge.src),
            tgt: alignment.nodes.get(&edge.tgt).copied().unwrap_or(edge.tgt),
            edge: edge.edge.clone(),
            position: edge.position,
        };
        result
            .insert_edge(new_id, copy)
            .map_err(|_| AlignError::Collision {
                kind: "edge",
                from: id.0,
                to: new_id.0,
            })?;
    }
    Ok(result)
}

fn bfs_contraction_path(
    source: &GInstance,
    view: &GInstance,
    start: u32,
    target: u32,
) -> Option<Vec<EdgeId>> {
    source.node(start)?;
    source.node(target)?;
    let mut queue = VecDeque::new();
    let mut previous: HashMap<u32, (u32, EdgeId)> = HashMap::new();
    let mut seen = HashSet::from([start]);
    for &edge_id in source.out_edges(start) {
        let edge = source.edge(edge_id)?;
        if view.node(edge.tgt).is_none()
            && source
                .node(edge.tgt)
                .is_some_and(|node| node.shape != panproto_inst::NodeShape::HyperEdge)
            && seen.insert(edge.tgt)
        {
            previous.insert(edge.tgt, (start, edge_id));
            queue.push_back(edge.tgt);
        }
    }
    while let Some(at) = queue.pop_front() {
        for &edge_id in source.out_edges(at) {
            let edge = source.edge(edge_id)?;
            if edge.tgt == target {
                let mut path = Vec::new();
                let mut cursor = at;
                while cursor != start {
                    let (parent, edge) = previous[&cursor];
                    path.push(edge);
                    cursor = parent;
                }
                path.reverse();
                path.push(edge_id);
                return Some(path);
            }
            if view.node(edge.tgt).is_some() {
                continue;
            }
            if source
                .node(edge.tgt)
                .is_some_and(|node| node.shape == panproto_inst::NodeShape::HyperEdge)
            {
                continue;
            }
            if !seen.insert(edge.tgt) {
                continue;
            }
            previous.insert(edge.tgt, (at, edge_id));
            queue.push_back(edge.tgt);
        }
    }
    None
}
