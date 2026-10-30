//! Asymmetric lenses over table-shaped instances.
//!
//! A table put first reconstructs the source recorded by the complement,
//! then pulls view edits back through the last structural writer of each
//! target cell. This baseline-and-pullback rule keeps the complement
//! authoritative for `GetPut` while allowing edits through column renames.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_inst::identity::{
    ComplementShape, IdentityCarrier, IdentityPolicy, IdentityStatus, compose_identities,
};
use panproto_inst::{
    BoundaryProtocols, CellFate, CellPlan, CheckKind, EntityFunctor, FInstance, FieldTransform,
    IdAllocator, IdRange, IncompleteReason, InstError, KeyRefusal, PairResolution,
    PredicateRefusal, PutDomainError,
    RestrictError, RestrictPlan, Row, RowId, SchemaRef, TableComplement, Value, ViewRef, Violation,
    functor_restrict_with_plan, validate_table,
};
use panproto_schema::{ConstraintValue, Edge, Protocol, ScalarKind, Schema};

use crate::asymmetric::{
    ComplementCompose, apply_inverse_transforms, propagate_view_edits_through_inverse, value_equiv,
};
use crate::compose::{check_boundary, duplicate, tree_fusion};
use crate::graph_lens::{GraphComposite, SequentialGraphLens};
use crate::{Lens, LensError};

/// Forward table-lens direction.
///
/// The table restrict plan is compiled once for this call and retained by a
/// bound lens for the plan-taking engine.
///
/// # Errors
///
/// Returns table-plan compilation or restrict errors.
pub fn get(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(FInstance, TableComplement), LensError> {
    let source_protocol = protocols.src;
    Bound::compile(lens, protocols)?.get(source, source_protocol)
}

/// A table lens paired with the restrict plan compiled for one boundary.
struct Bound<'a> {
    lens: &'a Lens,
    plan: RestrictPlan,
}

impl<'a> Bound<'a> {
    fn compile(lens: &'a Lens, protocols: BoundaryProtocols<'_>) -> Result<Self, LensError> {
        Self::compile_with(lens, || {
            RestrictPlan::compile_for_tables(
                &lens.src_schema,
                &lens.tgt_schema,
                &lens.compiled,
                protocols,
            )
        })
    }

    fn compile_with<F>(lens: &'a Lens, compile: F) -> Result<Self, LensError>
    where
        F: FnOnce() -> Result<RestrictPlan, RestrictError>,
    {
        Ok(Self {
            lens,
            plan: compile()?,
        })
    }

    fn get(
        &self,
        source: &FInstance,
        source_protocol: &Protocol,
    ) -> Result<(FInstance, TableComplement), LensError> {
        let (view, mut complement) =
            restrict_finite_rows(self.lens, source, &self.plan, source_protocol)?;
        complement.stamp(
            self.lens.identity(ComplementShape::Functor),
            &self.lens.src_schema,
        );
        Ok((view, complement))
    }
}

fn law_context(protocols: BoundaryProtocols<'_>) -> crate::TableContext {
    crate::TableContext {
        src: protocols.src.clone(),
        tgt: protocols.tgt.clone(),
    }
}

/// Run the gating table-lens laws over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`; it is not a
/// proof over all table instances.
///
/// # Errors
///
/// The first law violation in the specified gate order.
pub fn check_laws(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<crate::laws::LawReport, crate::LawViolation> {
    crate::laws::check_laws_report::<crate::TableShape>(lens, source, &law_context(protocols))
}

/// Check table `GetPut`.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// An operational error or `GetPut` violation.
pub fn check_get_put(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_get_put_for_shape::<crate::TableShape>(lens, source, &law_context(protocols))
}

/// Check strict table `PutGet` over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// An operational error or `PutGet` violation.
pub fn check_put_get(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_put_get_for_shape::<crate::TableShape>(lens, source, &law_context(protocols))
}

/// Check table put totality over its generated admitted views.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first admitted mutation on which put fails.
pub fn check_totality(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_totality_for_shape::<crate::TableShape>(
        lens,
        source,
        &law_context(protocols),
    )
}

/// Check table `PutTwice` over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first repeated-put drift.
pub fn check_put_twice(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_put_twice_for_shape::<crate::TableShape>(
        lens,
        source,
        &law_context(protocols),
    )
}

/// Check table presentation congruence.
///
/// A pass means "no counterexample found" among the coherent
/// presentation witnesses for `source`.
///
/// # Errors
///
/// The first domain or put result that is not congruent.
pub fn check_congruence(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_congruence_for_shape::<crate::TableShape>(
        lens,
        source,
        &law_context(protocols),
    )
}

/// Check table `WeakPutGet` over the finite single-step basis.
///
/// A pass means "no counterexample found" for `source`. This is a
/// named non-gating check.
///
/// # Errors
///
/// The first weak round-trip violation.
pub fn check_weak_put_get(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_weak_put_get_for_shape::<crate::TableShape>(
        lens,
        source,
        &law_context(protocols),
    )
}

/// Check table put injectivity on generated distinct admitted views.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first pair of inequivalent views with one put result.
pub fn check_put_injectivity(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_put_injectivity_for_shape::<crate::TableShape>(
        lens,
        source,
        &law_context(protocols),
    )
}

/// Check table put locality against the exact inverse-image-plus-cascade
/// footprint.
///
/// A pass means "no counterexample found" for `source`.
///
/// # Errors
///
/// The first mutation whose actual and specified footprints differ.
pub fn check_locality(
    lens: &Lens,
    source: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), crate::LawViolation> {
    crate::laws::check_locality_for_shape::<crate::TableShape>(
        lens,
        source,
        &law_context(protocols),
    )
}

/// Check the gating laws on every labelled table in a finite canonical scope.
///
/// No labelled isomorphs are pruned. Each value-bearing coordinate is checked
/// against every other admitted member of the fixed six-value alphabet. A
/// pass means "no counterexample found" in `scope`; it is not a proof outside
/// that finite universe. At each table the Single-Step Basis covers only its
/// single-edit neighbourhood; `PutTwice`, the `PutPut` classifications and E.6
/// exercise multi-step behaviour separately.
///
/// # Errors
///
/// The first law violation in deterministic enumeration order.
pub fn check_laws_exhaustive(
    lens: &Lens,
    schema: &Schema,
    protocols: BoundaryProtocols<'_>,
    scope: crate::laws::SmallScope,
) -> Result<Vec<crate::laws::LawReport>, crate::LawViolation> {
    let ctx = law_context(protocols);
    <crate::TableShape as crate::laws::LawShape>::enumerate_small(schema, scope, &ctx)
        .iter()
        .map(|source| {
            crate::laws::check_laws_report_exhaustive::<crate::TableShape>(lens, source, &ctx)
        })
        .collect()
}

/// Backward table-lens direction under the default identity policy.
///
/// A deleted surviving row is an SPO-style deletion: hidden foreign-key
/// pairs incident to it are not restored. A hidden row itself remains.
/// The baseline-and-pullback is a relational view-update translation in the
/// sense of `dayalbernstein1982correct`, Section 5.4, modulo functional
/// dependencies (inferred), and follows criteria 4 and 2 of
/// `keller1985algorithms`. A many-to-one rename is handled as drop-then-rename:
/// the last structural writer receives an edit and complement residue restores
/// the other sources, matching PostgreSQL updatable-view behaviour. A derived
/// output is refused as in `postgresql2026createview`; an explicitly registered
/// inverse pulls the edit back as in `virtuososqlinverse`.
///
/// # Errors
///
/// Identity errors precede ordered put-domain errors.
pub fn put(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    protocols: BoundaryProtocols<'_>,
) -> Result<FInstance, LensError> {
    put_checked(
        lens,
        view,
        complement,
        protocols,
        IdentityPolicy::AllowLegacy,
    )
    .map(|(source, _)| source)
}

/// Backward table-lens direction under an explicit identity policy.
///
/// # Errors
///
/// As [`put`], and returns the accepted identity status.
pub fn put_checked(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    protocols: BoundaryProtocols<'_>,
    policy: IdentityPolicy,
) -> Result<(FInstance, IdentityStatus), LensError> {
    let status = crate::identity::verify_with_identity(
        complement,
        lens,
        &lens.identity(ComplementShape::Functor),
        policy,
    )?;
    let plan = compile(lens, protocols)?;
    let source =
        check_and_restore(lens, view, complement, protocols.src, &plan).map_err(first_put_error)?;
    Ok((source, status))
}

/// Reconstruct a table source through an isomorphism without a complement.
///
/// # Errors
///
/// [`LensError::NotAnIsomorphism`] when the view does not determine the
/// source, or an ordinary table put error.
pub fn put_without_complement(
    lens: &Lens,
    view: &FInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<FInstance, LensError> {
    if let Some(detail) = obstruction_to_isomorphism(lens, protocols) {
        return Err(LensError::NotAnIsomorphism { detail });
    }
    let mut complement = TableComplement::empty();
    complement.stamp(lens.identity(ComplementShape::Functor), &lens.src_schema);
    put(lens, view, &complement, protocols)
}

/// Whether a table source is determined by its view alone.
///
/// This classifies bijective lenses whose complement is `Unit`, a class
/// narrower than isomorphism in the category of lenses. A `false` result is
/// an obstruction to complement-free put, not the absence of an inverse lens,
/// and does not decide whether the lens is forgetful. This includes
/// table-plan executability under both boundary protocols;
/// schema-level losslessness alone is insufficient when the migration is not
/// a table functor on those boundaries. The domain is all finite,
/// schema-supported table instances on which get is defined, including
/// absent optional cells. [`crate::LawShape::enumerate_small`] samples
/// bounded subuniverses of that domain. Table get refuses rows with
/// undeclared keys, so untyped coordinates are outside it.
#[must_use]
pub fn is_isomorphism(lens: &Lens, protocols: BoundaryProtocols<'_>) -> bool {
    obstruction_to_isomorphism(lens, protocols).is_none()
}

/// The first table coordinate that prevents complement-free reconstruction.
///
/// This concerns the same narrower, bijective (complement `Unit`) class as
/// [`is_isomorphism`]. It reports an obstruction to complement-free put, not
/// proof that no inverse lens exists, and does not decide whether the lens is
/// forgetful. A pass means that no obstruction was found by the complete
/// static table classifier for these protocols; it is not a claim about other
/// protocol boundaries.
#[must_use]
pub fn obstruction_to_isomorphism(lens: &Lens, protocols: BoundaryProtocols<'_>) -> Option<String> {
    let plan = match RestrictPlan::compile_for_tables(
        &lens.src_schema,
        &lens.tgt_schema,
        &lens.compiled,
        protocols,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return Some(format!(
                "the migration is not executable as a table lens on these boundaries: {error}"
            ));
        }
    };
    let Some(boundary) = &plan.boundary else {
        return Some("the table plan has no boundary layer".to_owned());
    };
    let (Some(functor), Some(cells)) = (&boundary.table, &boundary.cells) else {
        return Some("the table plan has no table functor or cell layer".to_owned());
    };

    if let Some(detail) = table_map_obstruction(lens, functor, cells) {
        return Some(detail);
    }
    if let Some(detail) = relation_map_obstruction(functor) {
        return Some(detail);
    }
    let field_vertices = lens
        .src_schema
        .edges
        .keys()
        .filter(|edge| {
            edge.kind == "prop" && edge.name.is_some() && functor.source().is_table(&edge.src)
        })
        .map(|edge| edge.tgt.clone())
        .collect::<BTreeSet<_>>();
    if let Some(detail) = table_validation_obstruction(
        &lens.src_schema,
        protocols.src,
        functor.source(),
        &field_vertices,
    ) {
        return Some(detail);
    }
    None
}

fn table_map_obstruction(lens: &Lens, functor: &EntityFunctor, cells: &CellPlan) -> Option<String> {
    let mut table_images = BTreeMap::new();
    for table in functor.source().tables() {
        let Some(image) = functor.table_map().get(table) else {
            return Some(format!(
                "table `{table}` is dropped, so its rows cannot be reconstructed without a complement"
            ));
        };
        if let Some(other) = table_images.insert(image, table) {
            return Some(format!(
                "tables `{other}` and `{table}` both map to `{image}`, so a view row does not determine its source table"
            ));
        }
        if lens.compiled.conditional_survival.contains_key(table) {
            return Some(format!(
                "table `{table}` survives conditionally, so some rows are dropped"
            ));
        }

        let source_columns = functor
            .source()
            .columns(table)
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let target_columns = functor
            .target()
            .columns(image)
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let transforms = lens.compiled.value_transforms(table);
        let fates = cells.fates.get(table).map_or(&[][..], Vec::as_slice);
        if let Some(detail) =
            table_column_obstruction(&transforms, &source_columns, &target_columns, fates)
        {
            return Some(format!("table `{table}` {detail}"));
        }
    }
    functor
        .target()
        .tables()
        .iter()
        .find(|target| !table_images.contains_key(*target))
        .map(|target| {
            format!(
                "target table `{target}` has no source preimage, so the table map is not bijective"
            )
        })
}

fn relation_map_obstruction(functor: &EntityFunctor) -> Option<String> {
    let mut relation_images = BTreeMap::new();
    for relation in functor.source().relations() {
        let Some(image) = functor.relation_map().get(relation) else {
            return Some(format!(
                "foreign key `{:?}` is dropped, so its pairs cannot be reconstructed without a complement",
                relation.name
            ));
        };
        if let Some(other) = relation_images.insert(image, relation) {
            return Some(format!(
                "foreign keys `{:?}` and `{:?}` have one image, so a view pair does not determine its source relation",
                other.name, relation.name
            ));
        }
    }
    functor
        .target()
        .relations()
        .iter()
        .find(|target| !relation_images.contains_key(*target))
        .map(|target| {
            format!(
                "target foreign key `{:?}` has no source preimage, so the relation map is not bijective",
                target.name
            )
        })
}

fn table_validation_obstruction(
    schema: &Schema,
    protocol: &Protocol,
    source: &panproto_inst::RelSchema,
    field_vertices: &BTreeSet<Name>,
) -> Option<String> {
    let mut required = schema
        .constraints
        .iter()
        .flat_map(|(vertex, constraints)| {
            constraints.iter().filter_map(move |constraint| {
                if !field_vertices.contains(vertex) {
                    return None;
                }
                let semantics = protocol
                    .constraint_semantics
                    .get(constraint.sort.as_ref())?;
                matches!(
                    semantics.parse_value(&constraint.sort, &constraint.value),
                    Ok(ConstraintValue::Required(true))
                )
                .then_some((vertex, &constraint.sort))
            })
        })
        .collect::<Vec<_>>();
    required.sort();
    if let Some((vertex, sort)) = required.first() {
        return Some(format!(
            "vertex `{vertex}` has required-value constraint `{sort}` at a named table field site, so table put can refuse a finite source in which that field is absent"
        ));
    }

    let mut keys = schema
        .declarations
        .keys
        .iter()
        .filter(|(table, _)| source.is_table(table))
        .flat_map(|(table, keys)| {
            keys.iter().filter_map(move |key| {
                let primary_can_miss = matches!(key.role, panproto_schema::KeyRole::Primary)
                    && key
                        .parts
                        .iter()
                        .any(|part| matches!(part, panproto_schema::KeyPart::Path(_)));
                let uniqueness_can_fail = key.parts.iter().all(|part| match part {
                    panproto_schema::KeyPart::Path(path) => {
                        table_path_is_evaluable(source, table, path)
                    }
                    panproto_schema::KeyPart::Position { .. } => false,
                });
                (primary_can_miss || uniqueness_can_fail).then_some((
                    table,
                    &key.id,
                    primary_can_miss,
                ))
            })
        })
        .collect::<Vec<_>>();
    keys.sort();
    keys.first().map(|(table, key, primary_can_miss)| {
        if *primary_can_miss {
            format!(
                "table `{table}` declares primary key `{key}`, so table put can refuse a finite source with a missing key path"
            )
        } else {
            format!(
                "table `{table}` declares key `{key}`, so table put can refuse a finite source with duplicate key values"
            )
        }
    })
}

fn table_path_is_evaluable(source: &panproto_inst::RelSchema, table: &Name, path: &[Edge]) -> bool {
    let Some((last, prefix)) = path.split_last() else {
        return false;
    };
    let mut at = table;
    for hop in prefix {
        if &hop.src != at || !source.relations().contains(hop) {
            return false;
        }
        at = &hop.tgt;
    }
    &last.src == at && source.columns(at).values().any(|column| column == last)
}

#[derive(Clone, Debug)]
struct TableTransformState {
    sources: BTreeSet<String>,
    carriers: BTreeMap<String, BTreeSet<OriginTrace>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OriginTrace {
    source: String,
    coordinate: OriginCoordinate,
    codecs: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum OriginCoordinate {
    Value,
    Presence,
}

impl TableTransformState {
    fn new(source_columns: &BTreeSet<String>) -> Self {
        Self {
            sources: source_columns.clone(),
            carriers: source_columns
                .iter()
                .map(|column| {
                    (
                        column.clone(),
                        BTreeSet::from([
                            OriginTrace {
                                source: column.clone(),
                                coordinate: OriginCoordinate::Value,
                                codecs: Vec::new(),
                            },
                            OriginTrace {
                                source: column.clone(),
                                coordinate: OriginCoordinate::Presence,
                                codecs: Vec::new(),
                            },
                        ]),
                    )
                })
                .collect(),
        }
    }

    fn first_missing_source(&self) -> Option<String> {
        self.sources
            .iter()
            .find(|source| {
                !self
                    .carriers
                    .values()
                    .any(|origins| origins.iter().any(|origin| origin.source == **source))
            })
            .cloned()
    }

    fn first_missing_without(&self, excluded: &str) -> Option<String> {
        self.sources
            .iter()
            .find(|source| {
                !self.carriers.iter().any(|(key, origins)| {
                    key != excluded && origins.iter().any(|origin| origin.source == **source)
                })
            })
            .cloned()
    }
}

fn table_column_obstruction(
    transforms: &[FieldTransform],
    source_columns: &BTreeSet<String>,
    target_columns: &BTreeSet<String>,
    fates: &[CellFate],
) -> Option<String> {
    let mut state = TableTransformState::new(source_columns);
    let flat = flattened_transforms(transforms);
    for (step, transform) in flat.iter().enumerate() {
        if let Some(detail) = table_transform_step_obstruction(transform, step, &mut state) {
            return Some(detail);
        }
    }
    let output_columns = state
        .carriers
        .keys()
        .filter_map(|column| structural_column_output(column, fates))
        .collect::<BTreeSet<_>>();
    if let Some(target) = target_columns
        .iter()
        .find(|target| !output_columns.contains(*target))
    {
        return Some(format!(
            "has target column `{target}` with no source preimage, so its cell cannot be reconstructed"
        ));
    }
    if let Some(output) = output_columns
        .iter()
        .find(|output| !target_columns.contains(*output))
    {
        return Some(format!(
            "produces column `{output}` outside the target table, so the column map is not bijective"
        ));
    }
    let decoded = decode_source_origins(&state, &flat, fates);
    if let Some(source) = source_columns.iter().find(|source| {
        let raw = BTreeSet::from([
            OriginTrace {
                source: (*source).clone(),
                coordinate: OriginCoordinate::Value,
                codecs: Vec::new(),
            },
            OriginTrace {
                source: (*source).clone(),
                coordinate: OriginCoordinate::Presence,
                codecs: Vec::new(),
            },
        ]);
        decoded.get(*source).is_none_or(|origins| origins != &raw)
    }) {
        return Some(format!(
            "drops source column `{source}`, so its cell cannot be reconstructed without a complement"
        ));
    }
    None
}

fn flattened_transforms(transforms: &[FieldTransform]) -> Vec<&FieldTransform> {
    fn append<'a>(transforms: &'a [FieldTransform], flat: &mut Vec<&'a FieldTransform>) {
        for transform in transforms {
            if let FieldTransform::PathTransform { path, inner } = transform
                && path.is_empty()
            {
                append(std::slice::from_ref(inner), flat);
            } else {
                flat.push(transform);
            }
        }
    }
    let mut flat = Vec::new();
    append(transforms, &mut flat);
    flat
}

fn table_transform_step_obstruction(
    transform: &FieldTransform,
    step: usize,
    state: &mut TableTransformState,
) -> Option<String> {
    match transform {
        FieldTransform::RenameField { old_key, new_key } => {
            rename_field_obstruction(state, old_key, new_key)
        }
        FieldTransform::DropField { key } => remove_carrier(state, key, "drops"),
        FieldTransform::AddField { key, .. } => add_field_obstruction(state, key),
        FieldTransform::KeepFields { keys } => keep_fields_obstruction(state, keys),
        FieldTransform::ApplyExpr {
            key,
            inverse,
            coercion_class,
            ..
        } => apply_expr_obstruction(state, key, inverse.as_ref(), *coercion_class, step),
        FieldTransform::ComputeField {
            target_key,
            expr,
            inverse,
            coercion_class,
            ..
        } => compute_field_obstruction(
            state,
            target_key,
            expr,
            inverse.as_ref(),
            *coercion_class,
            step,
        ),
        FieldTransform::MapReferences { field, rename_map }
            if rename_map
                .iter()
                .all(|(source, target)| target.as_deref() == Some(source.as_str())) =>
        {
            let _ = field;
            None
        }
        FieldTransform::Case { branches }
            if branches.iter().all(|branch| branch.transforms.is_empty()) =>
        {
            None
        }
        FieldTransform::PathTransform { .. } => Some(
            "has a nested path transform that table put cannot invert without a complement"
                .to_owned(),
        ),
        FieldTransform::Case { .. } => Some(
            "has a conditional transform that table put cannot invert without a complement"
                .to_owned(),
        ),
        FieldTransform::MapReferences { field, .. } => {
            if state.carriers.contains_key(field) {
                let presence = state
                    .carriers
                    .get(field)
                    .into_iter()
                    .flatten()
                    .filter(|origin| origin.coordinate == OriginCoordinate::Presence)
                    .cloned()
                    .collect();
                replace_carrier(state, field, presence, "maps references in")
            } else {
                None
            }
        }
    }
}

fn rename_field_obstruction(
    state: &mut TableTransformState,
    old_key: &str,
    new_key: &str,
) -> Option<String> {
    let origins = state.carriers.remove(old_key)?;
    state.carriers.insert(new_key.to_owned(), origins);
    state.first_missing_source().map(|source| {
        format!("renames `{old_key}` onto `{new_key}`, losing source column `{source}`")
    })
}

fn keep_fields_obstruction(state: &mut TableTransformState, keys: &[String]) -> Option<String> {
    state.carriers.retain(|key, _| keys.contains(key));
    state
        .first_missing_source()
        .map(|source| format!("removes source column `{source}` through KeepFields"))
}

fn compute_field_obstruction(
    state: &mut TableTransformState,
    target_key: &str,
    expr: &panproto_expr::Expr,
    inverse: Option<&panproto_expr::Expr>,
    coercion_class: panproto_gat::CoercionClass,
    step: usize,
) -> Option<String> {
    let mut origins = state
        .carriers
        .get(target_key)
        .into_iter()
        .flatten()
        .filter(|origin| origin.coordinate == OriginCoordinate::Presence)
        .cloned()
        .collect::<BTreeSet<_>>();
    if inverse.is_some() && matches!(coercion_class, panproto_gat::CoercionClass::Iso) {
        let values = crate::derived::sole_source_var(expr)
            .and_then(|source| state.carriers.get(&source).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|origin| origin.coordinate == OriginCoordinate::Value)
            .map(|mut origin| {
                origin.codecs.push(step);
                origin
            });
        origins.extend(values);
    }
    replace_carrier(state, target_key, origins, "computes over")
}

fn apply_expr_obstruction(
    state: &mut TableTransformState,
    key: &str,
    inverse: Option<&panproto_expr::Expr>,
    coercion_class: panproto_gat::CoercionClass,
    step: usize,
) -> Option<String> {
    if !state.carriers.contains_key(key) {
        return None;
    }
    if inverse.is_some() && matches!(coercion_class, panproto_gat::CoercionClass::Iso) {
        if let Some(origins) = state.carriers.get_mut(key) {
            *origins = std::mem::take(origins)
                .into_iter()
                .map(|mut origin| {
                    if origin.coordinate == OriginCoordinate::Value {
                        origin.codecs.push(step);
                    }
                    origin
                })
                .collect();
        }
        return None;
    }
    let presence = state
        .carriers
        .get(key)
        .into_iter()
        .flatten()
        .filter(|origin| origin.coordinate == OriginCoordinate::Presence)
        .cloned()
        .collect();
    replace_carrier(state, key, presence, "transforms")
}

fn add_field_obstruction(state: &mut TableTransformState, key: &str) -> Option<String> {
    if !state.carriers.contains_key(key) {
        state.carriers.insert(key.to_owned(), BTreeSet::new());
        return None;
    }
    if let Some(source) = state.first_missing_without(key) {
        return Some(format!(
            "adds occupied column `{key}`, whose complement-free inverse would remove source column `{source}`"
        ));
    }
    state.carriers.insert(key.to_owned(), BTreeSet::new());
    None
}

fn remove_carrier(state: &mut TableTransformState, key: &str, operation: &str) -> Option<String> {
    state.carriers.remove(key);
    state
        .first_missing_source()
        .map(|source| format!("{operation} source column `{source}`"))
}

fn replace_carrier(
    state: &mut TableTransformState,
    key: &str,
    origins: BTreeSet<OriginTrace>,
    operation: &str,
) -> Option<String> {
    state.carriers.insert(key.to_owned(), origins);
    state
        .first_missing_source()
        .map(|source| format!("{operation} `{key}`, losing source column `{source}`"))
}

fn decode_source_origins(
    state: &TableTransformState,
    transforms: &[&FieldTransform],
    fates: &[CellFate],
) -> BTreeMap<String, BTreeSet<OriginTrace>> {
    let mut view = state.carriers.clone();
    let mut moved = Vec::new();
    for fate in fates {
        match fate {
            CellFate::Drop(source) => {
                view.remove(source);
            }
            CellFate::Rename(source, target) => {
                if let Some(origins) = view.remove(source) {
                    moved.push((target.clone(), origins));
                }
            }
        }
    }
    for (target, origins) in moved {
        view.insert(target, origins);
    }

    let mut decoded = BTreeMap::new();
    for (target, origins) in view {
        if let Some(writer) = cell_writer(fates, &target) {
            decoded.insert(writer, origins);
        }
    }
    decode_transform_origins(transforms, &mut decoded);
    decoded
}

fn decode_transform_origins(
    transforms: &[&FieldTransform],
    decoded: &mut BTreeMap<String, BTreeSet<OriginTrace>>,
) {
    for (step, transform) in transforms.iter().enumerate().rev() {
        match transform {
            FieldTransform::RenameField { old_key, new_key } => {
                if let Some(origins) = decoded.remove(new_key) {
                    decoded.insert(old_key.clone(), origins);
                }
            }
            FieldTransform::AddField { key, .. } => {
                decoded.remove(key);
            }
            FieldTransform::ApplyExpr {
                key,
                inverse: Some(_),
                ..
            } => {
                if let Some(origins) = decoded.get_mut(key) {
                    *origins = decode_codec(origins, step);
                }
            }
            FieldTransform::ComputeField {
                target_key,
                expr,
                inverse: Some(_),
                ..
            } => {
                if let Some(origins) = decoded.get(target_key).cloned()
                    && let Some(source_key) = crate::derived::sole_source_var(expr)
                {
                    let origins = decode_codec(&origins, step);
                    if source_key == *target_key {
                        decoded.insert(source_key, origins);
                    } else {
                        decoded.remove(target_key);
                        decoded.entry(source_key).or_default().extend(origins);
                    }
                } else {
                    decoded.remove(target_key);
                }
            }
            FieldTransform::ComputeField { target_key, .. } => {
                decoded.remove(target_key);
            }
            FieldTransform::ApplyExpr { .. }
            | FieldTransform::DropField { .. }
            | FieldTransform::KeepFields { .. }
            | FieldTransform::PathTransform { .. }
            | FieldTransform::Case { .. }
            | FieldTransform::MapReferences { .. } => {}
        }
    }
}

fn decode_codec(origins: &BTreeSet<OriginTrace>, step: usize) -> BTreeSet<OriginTrace> {
    origins
        .iter()
        .filter_map(|origin| {
            let mut decoded = origin.clone();
            if decoded.coordinate == OriginCoordinate::Presence {
                Some(decoded)
            } else {
                (decoded.codecs.pop() == Some(step)).then_some(decoded)
            }
        })
        .collect()
}

fn structural_column_output(source: &str, fates: &[CellFate]) -> Option<String> {
    for fate in fates {
        match fate {
            CellFate::Drop(candidate) if candidate == source => return None,
            CellFate::Rename(candidate, target) if candidate == source => {
                return Some(target.clone());
            }
            CellFate::Drop(_) | CellFate::Rename(_, _) => {}
        }
    }
    Some(source.to_owned())
}

/// Every reason `view` lies outside the table put domain.
///
/// Reports are sorted by variant and table element. Duplicate keys at one
/// element are ordered by table and canonical key; missing cells at one
/// element are ordered by field. Other ties retain discovery order.
/// The domain is the run-time-checked constraint domain of
/// `trankatohu2020programmable`, and a total source-view put domain with strong
/// `PutGet` is the domain of `kohu2018axiomatic`, Definition 3.2 and Theorems 2.2
/// and 3.3. This is a comparison of domain structures, not a claim that this
/// domain strictly contains earlier domains.
///
/// # Errors
///
/// Every violation in report order.
pub fn check_put_domain(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    protocols: BoundaryProtocols<'_>,
) -> Result<(), Vec<PutDomainError>> {
    let Ok(plan) = compile(lens, protocols) else {
        return Ok(());
    };
    check_and_restore(lens, view, complement, protocols.src, &plan).map(|_| ())
}

fn check_and_restore(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    source_protocol: &Protocol,
    plan: &RestrictPlan,
) -> Result<FInstance, Vec<PutDomainError>> {
    let mut found = id_collisions(view, complement);
    found.extend(synthesized_edits(view, complement));
    let mut restored = None;
    match restore_with_plan(lens, view, complement, source_protocol, plan) {
        Ok(source) => {
            found.extend(forward_stability(
                lens,
                view,
                complement,
                &source,
                source_protocol,
                plan,
            ));
            found.extend(schema_violations(
                &source,
                &lens.src_schema,
                source_protocol,
            ));
            restored = Some(source);
        }
        Err(errors) => found.extend(errors),
    }
    sort_violations(&mut found);
    if found.is_empty() {
        restored.ok_or_else(Vec::new)
    } else {
        Err(found)
    }
}

/// A deterministic row id for a single writer adding to a table view.
///
/// # Errors
///
/// [`InstError::NodeIdsExhausted`] when no id below `u32::MAX` remains.
pub fn fresh_view_row_id(
    view: &FInstance,
    complement: &TableComplement,
) -> Result<RowId, InstError> {
    let allocator = IdAllocator::from_floor(IdRange::FULL, complement.id_floor);
    let taken = view
        .row_ids()
        .into_iter()
        .map(|id| id.0)
        .chain(complement.dropped_rows.keys().map(|id| id.0));
    allocator
        .fresh(taken)
        .map(RowId)
        .map_err(|_| InstError::NodeIdsExhausted)
}

impl ComplementCompose for TableComplement {
    fn compose(&self, other: &Self) -> Result<Self, LensError> {
        let (identity, _) = compose_identities(self, other)?;
        let mut result = self.clone();
        merge_map(
            &mut result.dropped_rows,
            &other.dropped_rows,
            "dropped_rows",
            |a, b| a.table == b.table && rows_equivalent_for_shape(&a.row, &b.row),
        )?;
        merge_map(
            &mut result.dropped_cells,
            &other.dropped_cells,
            "dropped_cells",
            value_equiv,
        )?;
        merge_map(
            &mut result.dropped_fk_pairs,
            &other.dropped_fk_pairs,
            "dropped_fk_pairs",
            PartialEq::eq,
        )?;
        merge_map(
            &mut result.original_fk_edges,
            &other.original_fk_edges,
            "original_fk_edges",
            PartialEq::eq,
        )?;
        merge_map(
            &mut result.original_tables,
            &other.original_tables,
            "original_tables",
            PartialEq::eq,
        )?;
        merge_map(
            &mut result.original_cells,
            &other.original_cells,
            "original_cells",
            value_equiv,
        )?;
        result
            .synthesized_fk_pairs
            .extend(other.synthesized_fk_pairs.iter().cloned());
        result.id_floor = result.id_floor.max(other.id_floor);
        result.identity = identity;
        Ok(result)
    }

    fn is_compatible(&self, other: &Self) -> bool {
        self.compose(other).is_ok()
    }
}

/// Compose two table lenses, fusing only when all three table plans compile.
///
/// # Errors
///
/// [`LensError::CompositionMismatch`] when the schemas do not meet.
pub fn compose(
    first: &Lens,
    second: &Lens,
    source: &Protocol,
    middle: &Protocol,
    target: &Protocol,
) -> Result<GraphComposite, LensError> {
    check_boundary(&first.tgt_schema, &second.src_schema)?;
    Ok(
        table_fusion(first, second, source, middle, target).map_or_else(
            || {
                GraphComposite::Sequential(Box::new(SequentialGraphLens {
                    first: duplicate(first),
                    second: duplicate(second),
                }))
            },
            |lens| GraphComposite::Compiled(Box::new(lens)),
        ),
    )
}

/// The table fusion rule used by table path normalization.
///
/// A first-stage field transform or op-term assignment remains atomic. The
/// intermediate finite-map adapter validates the first view against the
/// second stage's relational source boundary; fusing would skip that check
/// and could admit a field introduced outside the boundary. A second-stage
/// value layer also remains atomic when the source and middle protocols or
/// the first plan's relational boundaries differ, because their typed
/// placeholders for absent cells may differ.
/// Finally, the first target and second source relational schemas must be
/// identical, the three entity functors must compose literally, and the
/// second column map must be injective within each table, so fusion cannot
/// change a many-to-one cell-map winner by removing the intermediate key
/// order. The second source must also have no protocol-derived required-value
/// obligation: an atomic second put validates that middle table, while the
/// fused put does not.
#[must_use]
pub(crate) fn table_fusion(
    first: &Lens,
    second: &Lens,
    source: &Protocol,
    middle: &Protocol,
    target: &Protocol,
) -> Option<Lens> {
    let first_writes_fields = first
        .compiled
        .field_transforms
        .values()
        .any(|transforms| !transforms.is_empty())
        || first
            .compiled
            .op_term_assignments
            .values()
            .any(|assignments| !assignments.is_empty());
    if first_writes_fields {
        return None;
    }
    let second_has_value_layer = !second.compiled.conditional_survival.is_empty()
        || second
            .compiled
            .field_transforms
            .values()
            .any(|transforms| !transforms.is_empty())
        || second
            .compiled
            .op_term_assignments
            .values()
            .any(|assignments| !assignments.is_empty());
    let candidate = tree_fusion(first, second)?;
    let first_plan = compile(
        first,
        BoundaryProtocols {
            src: source,
            tgt: middle,
        },
    )
    .ok()?;
    let second_plan = compile(
        second,
        BoundaryProtocols {
            src: middle,
            tgt: target,
        },
    )
    .ok()?;
    let candidate_plan = compile(
        &candidate,
        BoundaryProtocols {
            src: source,
            tgt: target,
        },
    )
    .ok()?;
    let first_functor = first_plan.boundary.as_ref()?.table.as_ref()?;
    let second_functor = second_plan.boundary.as_ref()?.table.as_ref()?;
    let candidate_functor = candidate_plan.boundary.as_ref()?.table.as_ref()?;
    if obstruction_to_isomorphism(
        first,
        BoundaryProtocols {
            src: source,
            tgt: middle,
        },
    )
    .is_some()
    {
        return None;
    }
    if first_functor.target() != second_functor.source() {
        return None;
    }
    let middle_field_vertices = second_functor
        .source()
        .all_columns()
        .values()
        .flat_map(BTreeMap::values)
        .map(|edge| edge.tgt.clone())
        .collect::<BTreeSet<_>>();
    if table_validation_obstruction(
        &second.src_schema,
        middle,
        second_functor.source(),
        &middle_field_vertices,
    )
    .is_some()
    {
        return None;
    }
    if second_has_value_layer
        && (source != middle || first_functor.source() != first_functor.target())
    {
        return None;
    }
    if !table_functors_compose(first_functor, second_functor, candidate_functor)
        || !table_columns_are_injective(second_functor)
    {
        return None;
    }
    Some(candidate)
}

fn table_columns_are_injective(functor: &panproto_inst::EntityFunctor) -> bool {
    let mut images = BTreeSet::new();
    functor
        .column_map()
        .iter()
        .all(|((table, _), target)| images.insert((table.clone(), target.clone())))
}

fn table_functors_compose(
    first: &panproto_inst::EntityFunctor,
    second: &panproto_inst::EntityFunctor,
    candidate: &panproto_inst::EntityFunctor,
) -> bool {
    let tables_agree = first.source().tables().iter().all(|source| {
        first
            .table_map()
            .get(source)
            .and_then(|middle| second.table_map().get(middle))
            == candidate.table_map().get(source)
    });
    let relations_agree = first.source().relations().iter().all(|source| {
        first
            .relation_map()
            .get(source)
            .and_then(|middle| second.relation_map().get(middle))
            == candidate.relation_map().get(source)
    });
    let columns_agree = first
        .source()
        .all_columns()
        .iter()
        .all(|(source_table, columns)| {
            columns.keys().all(|source_column| {
                let source_key = (source_table.clone(), source_column.clone());
                let sequential = first
                    .column_map()
                    .get(&source_key)
                    .and_then(|middle_column| {
                        let middle_table = first.table_map().get(source_table)?;
                        second
                            .column_map()
                            .get(&(middle_table.clone(), middle_column.clone()))
                    });
                sequential == candidate.column_map().get(&source_key)
            })
        });
    tables_agree && relations_agree && columns_agree
}

fn compile(lens: &Lens, protocols: BoundaryProtocols<'_>) -> Result<RestrictPlan, LensError> {
    Ok(RestrictPlan::compile_for_tables(
        &lens.src_schema,
        &lens.tgt_schema,
        &lens.compiled,
        protocols,
    )?)
}

fn restore_with_plan(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    source_protocol: &Protocol,
    plan: &RestrictPlan,
) -> Result<FInstance, Vec<PutDomainError>> {
    let Some(boundary) = &plan.boundary else {
        return Err(Vec::new());
    };
    let (Some(functor), Some(cells)) = (&boundary.table, &boundary.cells) else {
        return Err(Vec::new());
    };
    let (mut source, mut errors) = restore_rows(lens, view, complement, functor, cells);

    restore_view_pairs(view, complement, plan, functor, &mut source, &mut errors);
    restore_dropped_pairs(complement, &mut source);

    if !errors.is_empty() {
        return Err(errors);
    }

    let baseline_view = match restrict_finite_rows(lens, &source, plan, source_protocol) {
        Ok((baseline, _)) => baseline,
        Err(error) => return Err(refusal_from_restrict(error)),
    };
    errors.extend(pull_changed_rows(
        lens,
        view,
        &baseline_view,
        functor,
        cells,
        &mut source,
    ));
    if errors.is_empty() {
        Ok(source)
    } else {
        Err(errors)
    }
}

fn restore_rows(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    functor: &panproto_inst::EntityFunctor,
    cells: &panproto_inst::CellPlan,
) -> (FInstance, Vec<PutDomainError>) {
    let mut source = FInstance::new();
    let mut errors = Vec::new();
    for id in view.row_ids() {
        let Some(view_table) = view.table_of(id) else {
            continue;
        };
        let candidates: Vec<&Name> = functor
            .table_map()
            .iter()
            .filter(|(_, image)| *image == view_table)
            .map(|(source, _)| source)
            .collect();
        let recorded = complement.original_tables.get(&id).filter(|table| {
            functor
                .table_map()
                .get(*table)
                .is_some_and(|image| image == view_table)
        });
        let source_table = recorded.cloned().or_else(|| match candidates.as_slice() {
            [only] => Some((*only).clone()),
            _ => None,
        });
        let Some(source_table) = source_table else {
            errors.push(PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Row(id),
                candidates: candidates.into_iter().cloned().collect(),
            });
            continue;
        };
        let Some(view_row) = view.row(id) else {
            continue;
        };
        let columns = functor.source().columns(&source_table);
        let mut row = Row::new();
        for column in columns.keys() {
            let key = (id, column.clone());
            if let Some(value) = complement
                .dropped_cells
                .get(&key)
                .or_else(|| complement.original_cells.get(&key))
                .or_else(|| view_row.get(column))
            {
                row.insert(column.clone(), value.clone());
            }
        }
        let recorded = complement.dropped_cells.keys().any(|(row, _)| *row == id)
            || complement.original_cells.keys().any(|(row, _)| *row == id)
            || complement.original_tables.contains_key(&id);
        if !(recorded || id.0 < complement.id_floor) {
            let source_columns = columns.keys().cloned().collect();
            pull_row_edits(
                lens,
                &source_table,
                view_row,
                &mut row,
                &source_columns,
                cells
                    .fates
                    .get(&source_table)
                    .map_or(&[][..], Vec::as_slice),
                PullRowOptions {
                    removed_targets: &[],
                    has_snapshot: false,
                },
            );
        }
        if source.insert_row(source_table, id, row).is_err() {
            errors.push(PutDomainError::IdCollision {
                elem: ViewRef::Row(id),
            });
        }
    }
    for (&id, dropped) in &complement.dropped_rows {
        if view.row(id).is_none()
            && source
                .insert_row(dropped.table.clone(), id, dropped.row.clone())
                .is_err()
        {
            errors.push(PutDomainError::IdCollision {
                elem: ViewRef::Row(id),
            });
        }
    }
    (source, errors)
}

fn pull_changed_rows(
    lens: &Lens,
    view: &FInstance,
    baseline_view: &FInstance,
    functor: &panproto_inst::EntityFunctor,
    cells: &panproto_inst::CellPlan,
    source: &mut FInstance,
) -> Vec<PutDomainError> {
    let mut errors = Vec::new();
    for id in view.row_ids() {
        let (Some(view_row), Some(baseline_row), Some(source_table)) = (
            view.row(id),
            baseline_view.row(id),
            source.table_of(id).cloned(),
        ) else {
            continue;
        };
        if rows_equivalent_for_shape(view_row, baseline_row) {
            continue;
        }
        let columns = functor.source().columns(&source_table);
        let fates = cells
            .fates
            .get(&source_table)
            .map_or(&[][..], Vec::as_slice);
        let source_columns: BTreeSet<String> = columns.keys().cloned().collect();
        let differing = differing_fields(view_row, baseline_row);
        for field in &differing {
            let writer = cell_writer(fates, field);
            if let Some(error) =
                cell_edit_obstruction(lens, &source_table, id, field.clone(), writer.as_deref())
            {
                errors.push(error);
            }
        }
        let removed: Vec<String> = differing
            .into_iter()
            .filter(|field| !view_row.contains_key(field))
            .collect();
        if let Some(row) = source.row_mut(id) {
            pull_row_edits(
                lens,
                &source_table,
                view_row,
                row,
                &source_columns,
                fates,
                PullRowOptions {
                    removed_targets: &removed,
                    has_snapshot: true,
                },
            );
        }
    }
    errors
}

#[derive(Clone, Copy)]
struct PullRowOptions<'a> {
    removed_targets: &'a [String],
    has_snapshot: bool,
}

fn pull_row_edits(
    lens: &Lens,
    source_table: &Name,
    view_row: &Row,
    source_row: &mut Row,
    columns: &BTreeSet<String>,
    fates: &[CellFate],
    options: PullRowOptions<'_>,
) {
    let mut post_transform = source_row.clone();
    for target in options.removed_targets {
        if let Some(writer) = cell_writer(fates, target) {
            post_transform.remove(&writer);
            if options.has_snapshot {
                source_row.remove(&writer);
            }
        }
    }
    for (target, value) in view_row {
        if let Some(writer) = cell_writer(fates, target) {
            post_transform.insert(writer, value.clone());
        }
    }
    let transforms = lens.compiled.value_transforms(source_table);
    let outputs = transform_outputs(&transforms);
    if options.has_snapshot {
        for (key, value) in &post_transform {
            if columns.contains(key) && !outputs.contains(key) {
                source_row.insert(key.clone(), value.clone());
            }
        }
        let mut node = panproto_inst::Node::new(0, source_table.clone());
        node.extra_fields = std::mem::take(source_row);
        propagate_view_edits_through_inverse(&mut node, &post_transform, &transforms);
        *source_row = node.extra_fields;
    } else {
        let mut node = panproto_inst::Node::new(0, source_table.clone());
        node.extra_fields = post_transform;
        apply_inverse_transforms(&mut node, &transforms);
        *source_row = node.extra_fields;
    }
    source_row.retain(|key, _| columns.contains(key));
}

fn cell_writer(fates: &[CellFate], target: &str) -> Option<String> {
    let renamed = fates.iter().rev().find_map(|fate| match fate {
        CellFate::Rename(source, image) if image == target => Some(source.clone()),
        CellFate::Drop(_) | CellFate::Rename(_, _) => None,
    });
    if renamed.is_some() {
        return renamed;
    }
    let displaced = fates.iter().any(|fate| match fate {
        CellFate::Drop(source) | CellFate::Rename(source, _) => source == target,
    });
    (!displaced).then(|| target.to_owned())
}

fn transform_outputs(transforms: &[FieldTransform]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for transform in transforms {
        match transform {
            FieldTransform::RenameField { new_key, .. } => {
                out.insert(new_key.clone());
            }
            FieldTransform::AddField { key, .. } | FieldTransform::ApplyExpr { key, .. } => {
                out.insert(key.clone());
            }
            FieldTransform::ComputeField { target_key, .. } => {
                out.insert(target_key.clone());
            }
            FieldTransform::PathTransform { path, inner } if path.is_empty() => {
                out.extend(transform_outputs(std::slice::from_ref(inner)));
            }
            _ => {}
        }
    }
    out
}

fn restore_view_pairs(
    view: &FInstance,
    complement: &TableComplement,
    plan: &RestrictPlan,
    functor: &panproto_inst::EntityFunctor,
    source: &mut FInstance,
    errors: &mut Vec<PutDomainError>,
) {
    for (edge, src, tgt) in view.all_pairs() {
        let key = (edge.clone(), src, tgt);
        let recorded = complement.original_fk_edges.get(&key).filter(|carriers| {
            carriers
                .iter()
                .all(|carrier| functor.relation_map().get(carrier) == Some(&edge))
        });
        let candidates: Vec<Edge> = recorded.map_or_else(
            || {
                functor
                    .relation_map()
                    .iter()
                    .filter(|(_, image)| *image == &edge)
                    .map(|(source, _)| source)
                    .filter(|candidate| {
                        source.table_of(src) == Some(&candidate.src)
                            && source.table_of(tgt) == Some(&candidate.tgt)
                    })
                    .cloned()
                    .collect()
            },
            |carriers| carriers.iter().cloned().collect(),
        );
        if candidates.is_empty() {
            if complement
                .synthesized_fk_pairs
                .contains(&(edge.clone(), src, tgt))
            {
                continue;
            }
            let contracted = source
                .table_of(src)
                .zip(source.table_of(tgt))
                .and_then(|(a, b)| plan.contraction.pair(a, b))
                .is_some_and(
                    |pair| matches!(&pair.resolution, PairResolution::Label(e) if e == &edge),
                );
            errors.push(if contracted {
                PutDomainError::ContractedEdgeInserted {
                    elem: ViewRef::Pair { edge, src, tgt },
                }
            } else {
                PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair { edge, src, tgt },
                }
            });
            continue;
        }
        if recorded.is_none() && candidates.len() > 1 {
            errors.push(PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Pair { edge, src, tgt },
            });
            continue;
        }
        for carrier in candidates {
            if !source.contains_pair(&carrier, src, tgt)
                && source.insert_foreign_key(carrier, src, tgt).is_err()
            {
                errors.push(PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair {
                        edge: edge.clone(),
                        src,
                        tgt,
                    },
                });
            }
        }
    }
}

fn restore_dropped_pairs(complement: &TableComplement, source: &mut FInstance) {
    for (edge, pairs) in &complement.dropped_fk_pairs {
        for &(src, tgt) in pairs {
            if source.table_of(src) == Some(&edge.src)
                && source.table_of(tgt) == Some(&edge.tgt)
                && !source.contains_pair(edge, src, tgt)
            {
                let _ = source.insert_foreign_key(edge.clone(), src, tgt);
            }
        }
    }
}

fn id_collisions(view: &FInstance, complement: &TableComplement) -> Vec<PutDomainError> {
    view.row_ids()
        .into_iter()
        .filter(|id| complement.dropped_rows.contains_key(id))
        .map(|id| PutDomainError::IdCollision {
            elem: ViewRef::Row(id),
        })
        .collect()
}

fn synthesized_edits(view: &FInstance, complement: &TableComplement) -> Vec<PutDomainError> {
    complement
        .synthesized_fk_pairs
        .iter()
        .filter(|(edge, src, tgt)| {
            view.row(*src).is_some()
                && view.row(*tgt).is_some()
                && !view.contains_pair(edge, *src, *tgt)
        })
        .map(|(edge, src, tgt)| PutDomainError::SynthesizedEdited {
            elem: ViewRef::Pair {
                edge: edge.clone(),
                src: *src,
                tgt: *tgt,
            },
        })
        .collect()
}

fn forward_stability(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    source: &FInstance,
    source_protocol: &Protocol,
    plan: &RestrictPlan,
) -> Vec<PutDomainError> {
    let regenerated = match restrict_finite_rows(lens, source, plan, source_protocol) {
        Ok((regenerated, _)) => regenerated,
        Err(error) => return refusal_from_restrict(error),
    };
    let mut found = Vec::new();
    for id in view.row_ids() {
        let Some(expected) = view.row(id) else {
            continue;
        };
        let Some(actual) = regenerated.row(id) else {
            found.push(PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Row(id),
            });
            continue;
        };
        if rows_equivalent_for_shape(expected, actual) {
            continue;
        }
        let source_table = source.table_of(id);
        let fates = source_table
            .and_then(|table| plan.boundary.as_ref()?.cells.as_ref()?.fates.get(table))
            .map_or(&[][..], Vec::as_slice);
        for field in differing_fields(expected, actual) {
            let writer = cell_writer(fates, &field);
            found.push(classify_cell_edit(
                lens,
                source_table,
                id,
                field,
                writer.as_deref(),
            ));
        }
    }
    for (edge, src, tgt) in view.all_pairs() {
        if !regenerated.contains_pair(&edge, src, tgt)
            && !complement
                .synthesized_fk_pairs
                .contains(&(edge.clone(), src, tgt))
        {
            found.push(PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Pair { edge, src, tgt },
            });
        }
    }
    found
}

/// Run the total-row restrict over a finite-map source without turning an
/// absent optional cell into a null cell. Scratch nulls satisfy the engine's
/// internal total-row precondition; their view and complement coordinates
/// are removed afterward, while a real null in `source` remains.
fn restrict_finite_rows(
    lens: &Lens,
    source: &FInstance,
    plan: &RestrictPlan,
    source_protocol: &Protocol,
) -> Result<(FInstance, TableComplement), RestrictError> {
    let Some(boundary) = &plan.boundary else {
        return functor_restrict_with_plan(source, plan);
    };
    let (Some(functor), Some(cells)) = (&boundary.table, &boundary.cells) else {
        return functor_restrict_with_plan(source, plan);
    };
    let mut scratch = source.clone();
    let mut absent = BTreeMap::new();
    for id in source.row_ids() {
        let Some(table) = source.table_of(id) else {
            continue;
        };
        let fates = cells.fates.get(table).map_or(&[][..], Vec::as_slice);
        let transforms = lens.compiled.value_transforms(table);
        for (column, edge) in functor.source().columns(table) {
            if source.row(id).is_some_and(|row| !row.contains_key(column)) {
                if let Some(row) = scratch.row_mut(id) {
                    row.insert(
                        column.clone(),
                        finite_map_placeholder(functor.source(), edge, source_protocol),
                    );
                }
                absent.insert(
                    (id, column.clone()),
                    output_column(column, &transforms, fates),
                );
            }
        }
    }
    let (mut view, mut complement) = functor_restrict_with_plan(&scratch, plan)?;
    for (key, output) in absent {
        if let Some(dropped) = complement.dropped_rows.get_mut(&key.0) {
            dropped.row.remove(&key.1);
        }
        complement.dropped_cells.remove(&key);
        complement.original_cells.remove(&key);
        if let Some(output) = output
            && let Some(row) = view.row_mut(key.0)
        {
            row.remove(&output);
        }
    }
    Ok((view, complement))
}

fn finite_map_placeholder(
    schema: &panproto_inst::RelSchema,
    column: &Edge,
    protocol: &Protocol,
) -> Value {
    let kind = schema
        .value_kind(&column.tgt)
        .and_then(|name| protocol.value_kinds.get(name.as_str()));
    match kind {
        Some(ScalarKind::Bool) => Value::Bool(false),
        Some(ScalarKind::Int { .. } | ScalarKind::BigInt) => Value::Int(1),
        Some(ScalarKind::Float { .. }) => Value::Float(1.0),
        Some(ScalarKind::Decimal) => {
            panproto_inst::value::DecimalValue::parse("1").map_or(Value::Int(1), Value::Decimal)
        }
        Some(ScalarKind::Str) => Value::Str(String::new()),
        Some(ScalarKind::Bytes(_)) => Value::Bytes(Vec::new()),
        Some(ScalarKind::Date) => panproto_inst::value::Temporal::parse_date("1970-01-01")
            .map_or(Value::Null, Value::Temporal),
        Some(ScalarKind::Time) => panproto_inst::value::Temporal::parse_time("00:00:00")
            .map_or(Value::Null, Value::Temporal),
        Some(ScalarKind::DateTime) => {
            panproto_inst::value::Temporal::parse_date_time("1970-01-01T00:00:00")
                .map_or(Value::Null, Value::Temporal)
        }
        Some(ScalarKind::Token) => Value::Token(String::new()),
        Some(ScalarKind::Any) | None => Value::Null,
    }
}

fn output_column(
    source: &str,
    transforms: &[FieldTransform],
    fates: &[CellFate],
) -> Option<String> {
    let mut output = Some(source.to_owned());
    for transform in transforms {
        match transform {
            FieldTransform::RenameField { old_key, new_key }
                if output.as_deref() == Some(old_key) =>
            {
                output = Some(new_key.clone());
            }
            FieldTransform::DropField { key } if output.as_deref() == Some(key) => output = None,
            FieldTransform::KeepFields { keys }
                if output.as_ref().is_some_and(|key| !keys.contains(key)) =>
            {
                output = None;
            }
            _ => {}
        }
    }
    let key = output?;
    structural_column_output(&key, fates)
}

fn classify_cell_edit(
    lens: &Lens,
    table: Option<&Name>,
    id: RowId,
    target: String,
    writer: Option<&str>,
) -> PutDomainError {
    let transforms = table.map_or_else(Vec::new, |table| lens.compiled.value_transforms(table));
    let key = writer.unwrap_or(&target);
    for transform in &transforms {
        match transform {
            FieldTransform::ApplyExpr {
                key: output,
                inverse,
                ..
            } if output == key => {
                return if inverse.is_some() {
                    PutDomainError::DerivedCellEdited {
                        elem: ViewRef::Row(id),
                        field: target,
                    }
                } else {
                    PutDomainError::TransformNotInvertible {
                        elem: ViewRef::Row(id),
                        field: target,
                    }
                };
            }
            FieldTransform::AddField { key: output, .. } if output == key => {
                return PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(id),
                    field: target,
                };
            }
            FieldTransform::ComputeField { target_key, .. } if target_key == key => {
                return PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(id),
                    field: target,
                };
            }
            _ => {}
        }
    }
    PutDomainError::DerivedCellEdited {
        elem: ViewRef::Row(id),
        field: target,
    }
}

fn cell_edit_obstruction(
    lens: &Lens,
    table: &Name,
    id: RowId,
    target: String,
    writer: Option<&str>,
) -> Option<PutDomainError> {
    let Some(writer) = writer else {
        return Some(PutDomainError::DerivedCellEdited {
            elem: ViewRef::Row(id),
            field: target,
        });
    };
    for transform in lens.compiled.value_transforms(table) {
        match transform {
            FieldTransform::AddField { key, .. } if key == writer => {
                return Some(PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(id),
                    field: target,
                });
            }
            FieldTransform::ComputeField {
                target_key,
                inverse,
                ..
            } if target_key == writer && inverse.is_none() => {
                return Some(PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(id),
                    field: target,
                });
            }
            FieldTransform::ApplyExpr { key, inverse, .. }
                if key == writer && inverse.is_none() =>
            {
                return Some(PutDomainError::TransformNotInvertible {
                    elem: ViewRef::Row(id),
                    field: target,
                });
            }
            _ => {}
        }
    }
    None
}

fn schema_violations(
    source: &FInstance,
    schema: &panproto_schema::Schema,
    protocol: &Protocol,
) -> Vec<PutDomainError> {
    let mut found = Vec::new();
    let report = validate_table(source, schema, protocol);
    for incomplete in &report.incomplete {
        if incomplete.check.kind == CheckKind::KeyUnique
            && let SchemaRef::Key { vertex, key } = &incomplete.check.decl
            && let Some(reason) = key_refusal(schema, vertex, key, &incomplete.reason)
        {
            found.push(PutDomainError::KeyRefused {
                table: vertex.clone(),
                key: key.clone(),
                reason,
            });
        }
    }
    for diagnostic in report.diagnostics {
        match (&diagnostic.check.kind, &diagnostic.violation) {
            (CheckKind::RequiredValue, Violation::MissingValue { field: Some(field) }) => {
                if let Some(elem) = diagnostic.subject.clone() {
                    found.push(PutDomainError::MissingRequiredCell {
                        elem,
                        field: field.clone(),
                    });
                }
            }
            (CheckKind::KeyUnique, Violation::DuplicateKey { key, elems }) => {
                let table = match &diagnostic.check.decl {
                    SchemaRef::Key { vertex, .. } => vertex.clone(),
                    _ => continue,
                };
                let mut elems = elems.clone();
                elems.sort();
                found.push(PutDomainError::DuplicateKey {
                    table,
                    key: key.clone(),
                    elems,
                });
            }
            _ => {}
        }
    }
    found
}

/// The keying refusal behind an undecided `KeyUnique` check of key `key`
/// on `vertex`, or `None` when the check is undecided for another reason.
///
/// The validator reports the shared keying kernel's refusal at the least
/// row whose key tuple it refuses; an equality vector of the wrong arity
/// reaches it as an ill-formed declaration, whose arity is the key's own.
fn key_refusal(
    schema: &panproto_schema::Schema,
    vertex: &Name,
    key: &Name,
    reason: &IncompleteReason,
) -> Option<KeyRefusal> {
    match reason {
        IncompleteReason::UnsupportedCollation { collation, .. } => {
            Some(KeyRefusal::UnsupportedCollation {
                collation: collation.clone(),
            })
        }
        IncompleteReason::InvalidKeyRepresentation { kind, .. } => {
            Some(KeyRefusal::InvalidNumericRepresentation { kind: kind.clone() })
        }
        IncompleteReason::NoCanonicalKeyBytes { .. } => Some(KeyRefusal::NoCanonicalBytes),
        IncompleteReason::IllFormedDeclaration { .. } => schema
            .declarations
            .keys
            .get(vertex)?
            .iter()
            .find(|declared| declared.id == *key)
            .map(|declared| KeyRefusal::EqualityArity {
                expected: declared.parts.len(),
                found: declared.equality.len(),
            }),
        _ => None,
    }
}

fn refusal_from_restrict(error: RestrictError) -> Vec<PutDomainError> {
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
                elem: ViewRef::Row(RowId(node)),
                refusal,
            }]
        }
        RestrictError::ValueNotExpressible {
            node,
            field,
            value_type,
        } => vec![PutDomainError::SurvivalPredicateRefused {
            elem: ViewRef::Row(RowId(node)),
            refusal: PredicateRefusal::ValueNotExpressible { field, value_type },
        }],
        _ => Vec::new(),
    }
}

pub(crate) fn rows_equivalent_for_shape(left: &Row, right: &Row) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, value)| {
            right
                .get(key)
                .is_some_and(|other| value_equiv(value, other))
        })
}

pub(crate) fn value_maps_equivalent_for_shape<K: Ord>(
    left: &BTreeMap<K, Value>,
    right: &BTreeMap<K, Value>,
) -> bool {
    left.len() == right.len()
        && left.iter().all(|(key, value)| {
            right
                .get(key)
                .is_some_and(|other| value_equiv(value, other))
        })
}

fn differing_fields(left: &Row, right: &Row) -> Vec<String> {
    let keys: BTreeSet<String> = left.keys().chain(right.keys()).cloned().collect();
    keys.into_iter()
        .filter(|key| match (left.get(key), right.get(key)) {
            (Some(a), Some(b)) => !value_equiv(a, b),
            (None, None) => false,
            _ => true,
        })
        .collect()
}

fn first_put_error(mut errors: Vec<PutDomainError>) -> LensError {
    sort_violations(&mut errors);
    errors.into_iter().next().map_or_else(
        || LensError::ComplementMismatch {
            detail: "table restoration was not determined".to_owned(),
        },
        |error| LensError::PutDomain(Box::new(error)),
    )
}

fn sort_violations(found: &mut Vec<PutDomainError>) {
    PutDomainError::sort_report(found);
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use panproto_inst::{CompiledMigration, Value, wtype::FieldTransform};
    use panproto_schema::{Constraint, ConstraintSemantics, ScalarKind, SchemaBuilder};

    use super::*;
    use crate::tests::TestUnwrap;

    #[test]
    fn writer_prefers_the_last_explicit_rename_over_an_implicit_name() {
        assert_eq!(cell_writer(&[], "x"), Some("x".to_owned()));
        assert_eq!(cell_writer(&[CellFate::Drop("x".to_owned())], "x"), None);
        assert_eq!(
            cell_writer(
                &[
                    CellFate::Rename("y".to_owned(), "x".to_owned()),
                    CellFate::Rename("z".to_owned(), "x".to_owned()),
                ],
                "x",
            ),
            Some("z".to_owned())
        );
        assert_eq!(
            cell_writer(
                &[
                    CellFate::Rename("z".to_owned(), "x".to_owned()),
                    CellFate::Rename("x".to_owned(), "y".to_owned()),
                ],
                "x",
            ),
            Some("z".to_owned())
        );
    }

    #[test]
    fn table_get_compiles_its_plan_once() {
        let protocol = Protocol {
            non_value_kinds: ["record".to_owned()].into(),
            ..Protocol::default()
        };
        let Ok(builder) = SchemaBuilder::new(&protocol).vertex("row", "record", None) else {
            panic!("row schema must build")
        };
        let Ok(schema) = builder.build() else {
            panic!("schema must build")
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: schema.clone(),
            tgt_schema: schema,
        };
        let calls = Cell::new(0_u8);
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let Ok(bound) = Bound::compile_with(&lens, || {
            calls.set(calls.get() + 1);
            RestrictPlan::compile_for_tables(
                &lens.src_schema,
                &lens.tgt_schema,
                &lens.compiled,
                protocols,
            )
        }) else {
            panic!("table plan must compile")
        };
        let Ok(source) = FInstance::new().with_rows("row", [(RowId(0), Row::new())]) else {
            panic!("source row must be admitted")
        };
        assert!(bound.get(&source, &protocol).is_ok());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn table_fusion_retains_the_intermediate_stray_cell_check() {
        let protocol = Protocol {
            non_value_kinds: ["record".to_owned()].into(),
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)
            .test_unwrap("row schema must build")
            .build()
            .test_unwrap("row schema must build");
        let identity = || Lens {
            compiled: CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: schema.clone(),
            tgt_schema: schema.clone(),
        };
        let mut first = identity();
        first.compiled.field_transforms.insert(
            "row".into(),
            vec![FieldTransform::AddField {
                key: "outside".to_owned(),
                value: Value::Int(0),
            }],
        );
        let second = identity();
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let source = FInstance::new()
            .with_rows("row", [(RowId(0), Row::new())])
            .test_unwrap("source row must be admitted");

        let (intermediate, _) = get(&first, &source, context)
            .test_unwrap("the first stage may produce an untyped field");
        assert!(
            get(&second, &intermediate, context).is_err(),
            "the atomic second stage must reject the introduced stray cell"
        );
        assert!(
            table_fusion(&first, &second, &protocol, &protocol, &protocol).is_none(),
            "fusion must not erase the intermediate source-boundary check"
        );
    }

    fn column_protocol(scalar: ScalarKind) -> Protocol {
        Protocol {
            value_kinds: BTreeMap::from([("value".to_owned(), scalar)]),
            non_value_kinds: ["record".to_owned()].into(),
            ..Protocol::default()
        }
    }

    fn schema_with_columns(protocol: &Protocol, columns: &[&str]) -> Schema {
        schema_with_column_kind(protocol, "value", columns)
    }

    fn schema_with_column_kind(protocol: &Protocol, value_kind: &str, columns: &[&str]) -> Schema {
        let mut builder = SchemaBuilder::new(protocol)
            .vertex("row", "record", None)
            .test_unwrap("row schema must build")
            .vertex("value", value_kind, None)
            .test_unwrap("value schema must build");
        for column in columns {
            builder = builder
                .edge("row", "value", "property", Some(*column))
                .test_unwrap("column must build");
        }
        builder.build().test_unwrap("column schema must build")
    }

    fn edge_named(schema: &Schema, name: &str) -> Edge {
        schema
            .edges
            .keys()
            .find(|edge| edge.name.as_deref() == Some(name))
            .cloned()
            .test_unwrap("named column exists")
    }

    fn mapped_lens(
        source: &Schema,
        target: &Schema,
        edge_map: impl IntoIterator<Item = (Edge, Edge)>,
    ) -> Lens {
        Lens {
            compiled: CompiledMigration {
                surviving_verts: target.vertices.keys().cloned().collect(),
                surviving_edges: target.edges.keys().cloned().collect(),
                edge_remap: edge_map.into_iter().collect(),
                ..CompiledMigration::default()
            },
            src_schema: source.clone(),
            tgt_schema: target.clone(),
        }
    }

    #[test]
    fn table_fusion_rejects_a_second_stage_column_collision() {
        let protocol = column_protocol(ScalarKind::Int {
            bits: 64,
            signed: true,
        });
        let source_schema = schema_with_columns(&protocol, &["a", "z"]);
        let middle_schema = schema_with_columns(&protocol, &["x", "y"]);
        let target_schema = schema_with_columns(&protocol, &["q"]);
        let first = mapped_lens(
            &source_schema,
            &middle_schema,
            [
                (
                    edge_named(&source_schema, "a"),
                    edge_named(&middle_schema, "y"),
                ),
                (
                    edge_named(&source_schema, "z"),
                    edge_named(&middle_schema, "x"),
                ),
            ],
        );
        let second = mapped_lens(
            &middle_schema,
            &target_schema,
            [
                (
                    edge_named(&middle_schema, "x"),
                    edge_named(&target_schema, "q"),
                ),
                (
                    edge_named(&middle_schema, "y"),
                    edge_named(&target_schema, "q"),
                ),
            ],
        );
        let source = FInstance::new()
            .with_rows(
                "row",
                [(
                    RowId(0),
                    Row::from([
                        ("a".to_owned(), Value::Str("A".to_owned())),
                        ("z".to_owned(), Value::Str("Z".to_owned())),
                    ]),
                )],
            )
            .test_unwrap("source row must be admitted");
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let (middle_view, _) = get(&first, &source, context).test_unwrap("first table get");
        let (sequential, _) = get(&second, &middle_view, context).test_unwrap("second table get");
        let candidate = mapped_lens(
            &source_schema,
            &target_schema,
            [
                (
                    edge_named(&source_schema, "a"),
                    edge_named(&target_schema, "q"),
                ),
                (
                    edge_named(&source_schema, "z"),
                    edge_named(&target_schema, "q"),
                ),
            ],
        );
        let (fused, _) = get(&candidate, &source, context).test_unwrap("fused table get");
        assert_eq!(
            sequential.row(RowId(0)).test_unwrap("expected a value")["q"],
            Value::Str("A".to_owned())
        );
        assert_eq!(
            fused.row(RowId(0)).test_unwrap("expected a value")["q"],
            Value::Str("Z".to_owned())
        );
        assert!(
            table_fusion(&first, &second, &protocol, &protocol, &protocol).is_none(),
            "table fusion must retain the intermediate collision winner"
        );
    }

    #[test]
    fn table_fusion_retains_the_intermediate_placeholder_semantics() {
        let source_protocol = column_protocol(ScalarKind::Bool);
        let middle_protocol = column_protocol(ScalarKind::Int {
            bits: 64,
            signed: true,
        });
        let schema = schema_with_columns(&source_protocol, &["f"]);
        let first = mapped_lens(&schema, &schema, std::iter::empty());
        let mut second = mapped_lens(&schema, &schema, std::iter::empty());
        second
            .compiled
            .conditional_survival
            .insert("row".into(), panproto_expr::Expr::Var("f".into()));
        let source = FInstance::new()
            .with_rows("row", [(RowId(0), Row::new())])
            .test_unwrap("source row must be admitted");
        let (middle_view, _) = get(
            &first,
            &source,
            BoundaryProtocols {
                src: &source_protocol,
                tgt: &middle_protocol,
            },
        )
        .test_unwrap("the identity stage strips its typed placeholder");
        assert!(
            get(
                &second,
                &middle_view,
                BoundaryProtocols {
                    src: &middle_protocol,
                    tgt: &middle_protocol,
                },
            )
            .is_err(),
            "the atomic second stage observes the middle protocol's integer placeholder"
        );
        let candidate =
            tree_fusion(&first, &second).test_unwrap("graph/tree fusion admits the pair");
        assert!(
            get(
                &candidate,
                &source,
                BoundaryProtocols {
                    src: &source_protocol,
                    tgt: &middle_protocol,
                },
            )
            .is_ok(),
            "the fused candidate instead observes the source protocol's Boolean placeholder"
        );
        assert!(
            table_fusion(
                &first,
                &second,
                &source_protocol,
                &middle_protocol,
                &middle_protocol,
            )
            .is_none(),
            "table fusion must retain the intermediate placeholder type"
        );
    }

    #[test]
    fn table_fusion_retains_placeholder_semantics_across_value_kind_drift() {
        let protocol = Protocol {
            value_kinds: BTreeMap::from([
                ("bool".to_owned(), ScalarKind::Bool),
                (
                    "int".to_owned(),
                    ScalarKind::Int {
                        bits: 64,
                        signed: true,
                    },
                ),
            ]),
            non_value_kinds: ["record".to_owned()].into(),
            ..Protocol::default()
        };
        let source_schema = schema_with_column_kind(&protocol, "bool", &["f"]);
        let middle_schema = schema_with_column_kind(&protocol, "int", &["f"]);
        let first = mapped_lens(&source_schema, &middle_schema, std::iter::empty());
        let mut second = mapped_lens(&middle_schema, &middle_schema, std::iter::empty());
        second
            .compiled
            .conditional_survival
            .insert("row".into(), panproto_expr::Expr::Var("f".into()));
        let source = FInstance::new()
            .with_rows("row", [(RowId(0), Row::new())])
            .test_unwrap("source row must be admitted");
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let (middle_view, _) = get(&first, &source, context).test_unwrap("first table get");
        assert!(
            get(&second, &middle_view, context).is_err(),
            "the atomic second stage observes the middle integer placeholder"
        );
        let candidate =
            tree_fusion(&first, &second).test_unwrap("graph/tree fusion admits the pair");
        assert!(
            get(&candidate, &source, context).is_ok(),
            "the fused candidate instead observes the source Boolean placeholder"
        );
        assert!(
            table_fusion(&first, &second, &protocol, &protocol, &protocol).is_none(),
            "table fusion must retain placeholder kinds even under one protocol"
        );
    }

    #[test]
    fn table_fusion_retains_middle_required_value_validation() {
        let mut protocol = column_protocol(ScalarKind::Int {
            bits: 64,
            signed: true,
        });
        protocol.constraint_sorts.push("required".to_owned());
        protocol
            .constraint_semantics
            .insert("required".to_owned(), ConstraintSemantics::RequiredValue);
        let base = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)
            .test_unwrap("row schema must build")
            .vertex("row.f", "value", None)
            .test_unwrap("value schema must build")
            .edge("row", "row.f", "prop", Some("f"))
            .test_unwrap("column must build")
            .build()
            .test_unwrap("column schema must build");
        let mut second_source = base.clone();
        second_source.constraints.insert(
            Name::from("row.f"),
            vec![Constraint {
                sort: Name::from("required"),
                value: "true".to_owned(),
            }],
        );
        let first = mapped_lens(&base, &base, std::iter::empty());
        let second = mapped_lens(&second_source, &base, std::iter::empty());
        let candidate = mapped_lens(&base, &base, std::iter::empty());
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let source = FInstance::new()
            .with_rows("row", [(RowId(0), Row::new())])
            .test_unwrap("the optional source cell may be absent");
        let (middle_view, _) = get(&first, &source, context).test_unwrap("first table get");
        let (final_view, second_complement) =
            get(&second, &middle_view, context).test_unwrap("second table get");
        let atomic_put = put(&second, &final_view, &second_complement, context);
        assert!(
            atomic_put.is_err(),
            "the atomic second put validates its required middle cell: {atomic_put:?}"
        );
        let (candidate_view, candidate_complement) =
            get(&candidate, &source, context).test_unwrap("candidate table get");
        assert!(
            put(&candidate, &candidate_view, &candidate_complement, context).is_ok(),
            "the fused candidate has no corresponding source validation"
        );
        assert!(
            table_fusion(&first, &second, &protocol, &protocol, &protocol).is_none(),
            "table fusion must retain protocol-derived required-value validation"
        );
    }

    #[test]
    fn table_isomorphism_requires_target_table_surjectivity() {
        let protocol = Protocol {
            non_value_kinds: ["record".to_owned()].into(),
            ..Protocol::default()
        };
        let source = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)
            .test_unwrap("row schema must build")
            .build()
            .test_unwrap("source schema must build");
        let target = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)
            .test_unwrap("row schema must build")
            .vertex("target_only", "record", None)
            .test_unwrap("target-only schema must build")
            .build()
            .test_unwrap("target schema must build");
        let lens = mapped_lens(&source, &target, std::iter::empty());
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        assert!(!is_isomorphism(&lens, context));
        assert!(
            obstruction_to_isomorphism(&lens, context)
                .is_some_and(|detail| detail.contains("target table `target_only`"))
        );

        let target_view = FInstance::new()
            .with_rows("target_only", [(RowId(0), Row::new())])
            .test_unwrap("the target-only row must be admitted");
        assert!(matches!(
            put_without_complement(&lens, &target_view, context),
            Err(LensError::NotAnIsomorphism { .. })
        ));
    }

    #[test]
    fn table_isomorphism_requires_target_relation_surjectivity() {
        let protocol = Protocol {
            non_value_kinds: ["record".to_owned()].into(),
            ..Protocol::default()
        };
        let source = SchemaBuilder::new(&protocol)
            .vertex("parent", "record", None)
            .test_unwrap("parent schema must build")
            .vertex("child", "record", None)
            .test_unwrap("child schema must build")
            .build()
            .test_unwrap("source schema must build");
        let target = SchemaBuilder::new(&protocol)
            .vertex("parent", "record", None)
            .test_unwrap("parent schema must build")
            .vertex("child", "record", None)
            .test_unwrap("child schema must build")
            .edge("child", "parent", "foreign-key", Some("parent"))
            .test_unwrap("target-only relation must build")
            .build()
            .test_unwrap("target schema must build");
        let lens = mapped_lens(&source, &target, std::iter::empty());
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        assert!(!is_isomorphism(&lens, context));
        assert!(
            obstruction_to_isomorphism(&lens, context)
                .is_some_and(|detail| detail.contains("target foreign key"))
        );
    }

    #[test]
    fn table_isomorphism_requires_target_column_surjectivity() {
        let protocol = column_protocol(ScalarKind::Int {
            bits: 64,
            signed: true,
        });
        let source = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)
            .test_unwrap("row schema must build")
            .vertex("value", "value", None)
            .test_unwrap("value schema must build")
            .build()
            .test_unwrap("source schema must build");
        let target = schema_with_columns(&protocol, &["target_only"]);
        let lens = mapped_lens(&source, &target, std::iter::empty());
        let context = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        assert!(!is_isomorphism(&lens, context));
        assert!(
            obstruction_to_isomorphism(&lens, context)
                .is_some_and(|detail| detail.contains("target column `target_only`"))
        );
    }
}
