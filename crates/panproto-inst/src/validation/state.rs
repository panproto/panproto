//! The key indexes validation keeps: for each key, the elements sharing
//! each key value.
//!
//! Whole-instance validation builds an index per key to decide `KeyUnique`
//! and `ReferenceResolves`; incremental validation maintains the same
//! indexes, so its memory is the open verdicts plus these buckets.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use panproto_gat::CheckStatus;
use panproto_gat::Name;
use panproto_schema::{Edge, InstanceShapeKind, Key, KeyPart, KeyScope};
use serde::{Deserialize, Serialize};

use crate::edit_error::EditError;
use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::graph_edit::{AttrKey, GraphDelta, GraphEdit};
use crate::identity::{CanonicalSink, Digest};
use crate::table_delta::TableDelta;
use crate::table_edit::TableEdit;
use crate::tree_edit::TreeEdit;
use crate::value::Value;
use crate::view_ref::ViewRef;
use crate::wtype::WInstance;

use super::checks::{InstanceView, TreeArcs};
use super::footprint::{Footprint, Transition, transition};
use super::plan::{Compiled, DeltaElementKind, ExternalVerdicts, ValidationBudget, ValidationPlan};
use super::report::{
    CheckCoverage, CheckInstance, CheckKey, Diagnostic, FanRecord, IncompleteCheck,
    IncompleteReason, ReportScope, Requirement, ValidationReport, write_check_key,
    write_diagnostic, write_incomplete, write_status, write_view_ref,
};
use super::schedule::{self, Session, Simulation, Walk, simulate};
use super::witness::{EditWitness, WitnessOptions};

/// The name a key part is reported under: the labels of its path joined by
/// `/` (an unlabelled edge by its target vertex), or `position(<label>)` for
/// a position part.
pub fn part_name(part: &KeyPart) -> String {
    let label = |e: &Edge| {
        e.name
            .as_ref()
            .map_or_else(|| e.tgt.to_string(), Name::to_string)
    };
    match part {
        KeyPart::Path(path) => path.iter().map(label).collect::<Vec<_>>().join("/"),
        KeyPart::Position { within } => format!("position({})", label(within)),
    }
}

/// The values reached from `start` along `path`, in element order.
///
/// Each hop follows the out-edges labelled with the hop's edge. At the last
/// hop the value is the own value of each node reached; when no node is
/// reached there, the field named like the last edge (a node's extra field,
/// a row's cell) of each node before it is read instead. Absent and null
/// values are skipped.
pub fn path_values<'a>(view: InstanceView<'a>, start: &ViewRef, path: &[Edge]) -> Vec<&'a Value> {
    let Some((last, hops)) = path.split_last() else {
        return view.own_value(start).value().into_iter().collect();
    };
    let mut frontier = vec![start.clone()];
    for hop in hops {
        frontier = frontier
            .iter()
            .flat_map(|n| view.out_edges(n))
            .filter(|e| e.label == hop)
            .map(|e| e.tgt)
            .collect();
    }
    let reached: Vec<ViewRef> = frontier
        .iter()
        .flat_map(|n| view.out_edges(n))
        .filter(|e| e.label == last)
        .map(|e| e.tgt)
        .collect();
    if reached.is_empty() {
        let field = last
            .name
            .as_ref()
            .map_or_else(|| last.tgt.to_string(), Name::to_string);
        frontier
            .iter()
            .filter_map(|n| view.field(n, &field).value())
            .collect()
    } else {
        reached
            .iter()
            .filter_map(|n| view.own_value(n).value())
            .collect()
    }
}

/// The value of one key part of `element`: the first value along its path,
/// or its position among the targets of the ordered edge. `None` when the
/// part has no value, which SQL reads as NULL.
pub fn part_value(view: InstanceView<'_>, element: &ViewRef, part: &KeyPart) -> Option<Value> {
    match part {
        KeyPart::Path(path) => path_values(view, element, path)
            .first()
            .map(|v| (*v).clone()),
        KeyPart::Position { within } => view
            .position_of(element, within)
            .map(|p| Value::Int(i64::from(p))),
    }
}

/// The value of every part of `key` for `element`, or `None` when some part
/// has none (an element with a NULL key part takes no part in uniqueness).
pub fn key_values(view: InstanceView<'_>, element: &ViewRef, key: &Key) -> Option<Vec<Value>> {
    key.parts
        .iter()
        .map(|part| part_value(view, element, part))
        .collect()
}

/// The canonical bytes of a key value: each part's value through
/// [`CanonicalSink::value`], behind the part count.
pub fn key_bytes(values: &[Value]) -> Vec<u8> {
    let mut w = panproto_schema::CanonicalWriter::new();
    w.len(values.len());
    for v in values {
        w.value(v);
    }
    w.into_bytes()
}

/// The scope element of `element` under `scope`: for `Within(vertex)`, the
/// nearest node anchored at `vertex` reached backwards along in-edges (the
/// least such node when several are equally near), else `None`. Document
/// and global scopes are the whole instance, `None`.
pub fn scope_of(view: InstanceView<'_>, element: &ViewRef, scope: &KeyScope) -> Option<ViewRef> {
    let KeyScope::Within(vertex) = scope else {
        return None;
    };
    let mut seen = std::collections::BTreeSet::from([element.clone()]);
    let mut frontier = vec![element.clone()];
    while !frontier.is_empty() {
        let mut next: Vec<ViewRef> = frontier
            .iter()
            .flat_map(|n| view.in_edges(n))
            .map(|e| e.src)
            .filter(|n| seen.insert(n.clone()))
            .collect();
        next.sort();
        if let Some(found) = next.iter().find(|n| {
            view.node(n)
                .is_some_and(|rec| rec.anchor == vertex && !rec.fan)
        }) {
            return Some(found.clone());
        }
        frontier = next;
    }
    None
}

/// One key's index: the elements anchored at its vertex, bucketed by scope
/// and key value. Elements with a NULL part are left out (SQL NULL
/// semantics). Under a scoped `Within(v)` key, an element with a complete
/// key value but no ancestor anchored at `v` is outside the key's domain: it
/// joins no bucket and is listed in [`KeyIndex::outside`] instead.
#[derive(Clone, Debug, Default)]
pub struct KeyIndex {
    /// The elements of each `(scope, key bytes)` bucket, ascending.
    pub buckets: BTreeMap<(Option<ViewRef>, Vec<u8>), Vec<ViewRef>>,
    /// The elements of each key value, in any scope, ascending.
    pub by_value: BTreeMap<Vec<u8>, Vec<ViewRef>>,
    /// The key value of each indexed element.
    pub values: BTreeMap<ViewRef, Vec<Value>>,
    /// The elements outside a `Within` key's domain, ascending.
    pub outside: Vec<ViewRef>,
}

impl KeyIndex {
    /// Index the elements of `vertex` by `key`, scoped when `scoped`.
    pub(crate) fn build(view: InstanceView<'_>, vertex: &Name, key: &Key, scoped: bool) -> Self {
        let mut index = Self::default();
        for element in view.anchored(vertex) {
            let Some(values) = key_values(view, &element, key) else {
                continue;
            };
            let scope = if scoped {
                let scope = scope_of(view, &element, &key.scope);
                if scope.is_none() && matches!(key.scope, KeyScope::Within(_)) {
                    index.outside.push(element);
                    continue;
                }
                scope
            } else {
                None
            };
            let bytes = key_bytes(&values);
            index
                .by_value
                .entry(bytes.clone())
                .or_default()
                .push(element.clone());
            index
                .buckets
                .entry((scope, bytes))
                .or_default()
                .push(element.clone());
            index.values.insert(element, values);
        }
        index
    }
}

/// The serializable form of the exact index used by key and reference checks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyIndexState {
    /// Elements grouped by `(scope, canonical key bytes)`.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub buckets: BTreeMap<(Option<ViewRef>, Vec<u8>), Vec<ViewRef>>,
    /// Elements grouped by canonical key bytes without scope.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub by_value: BTreeMap<Vec<u8>, Vec<ViewRef>>,
    /// The complete key value of every indexed element.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub values: BTreeMap<ViewRef, Vec<Value>>,
}

impl From<&KeyIndex> for KeyIndexState {
    fn from(index: &KeyIndex) -> Self {
        Self {
            buckets: index.buckets.clone(),
            by_value: index.by_value.clone(),
            values: index.values.clone(),
        }
    }
}

/// The complete non-passing outcome of one evaluated check instance.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenVerdict {
    /// Every diagnostic, in canonical order and with multiplicity retained.
    pub diagnostics: Vec<Diagnostic>,
    /// Every incomplete result, in canonical order and with multiplicity retained.
    pub incomplete: Vec<IncompleteCheck>,
}

/// The unevaluated suffix of one check key after budget exhaustion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetCut {
    /// The read limit.
    pub limit: u64,
    /// Reads charged before the cut.
    pub spent: u64,
    /// The first subject not evaluated.
    pub from: Option<ViewRef>,
    /// The exact transition schedule, in `CheckInstance` order.
    pub scheduled: Vec<CheckInstance>,
    /// The exact budget-skipped instances, in `CheckInstance` order.
    pub skipped: Vec<CheckInstance>,
}

/// Durable state paired with a validated instance.
///
/// Passing outcomes are omitted. Memory is therefore the sum of four
/// parts: the open outcome bundles (one entry per failed or incomplete check
/// instance, with every diagnostic and incomplete result it produced); the
/// budget cuts, of which there is one per check key with a budget-skipped
/// instance and each repeats the complete scheduled and skipped vectors of
/// the cut run, so that a cut run of `s` scheduled instances cutting `k`
/// keys stores at most `2 * k * s` check instances (after a cut initialization
/// `s` is the number of check instances); the active external projection
/// (at most one verdict per external check instance); and the maintained
/// key and reference indexes (each indexed element once in its bucket, once
/// in its by-value bucket and once with its key value, per key or
/// reference). Without a budget cut the state holds no schedule at all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValidationState {
    /// The validation plan identity.
    pub plan: Digest,
    /// Successfully admitted edits since initialization.
    pub seq: u64,
    /// Aggregate status.
    pub status: CheckStatus,
    /// Complete non-passing evaluated outcomes.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub open: BTreeMap<CheckInstance, OpenVerdict>,
    /// Unevaluated suffixes, separately from evaluated outcomes.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub budget_cuts: BTreeMap<CheckKey, BudgetCut>,
    /// Coverage by compiled check.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub coverage: BTreeMap<CheckKey, CheckCoverage>,
    /// Exactly the supplied verdicts addressable by active external checks.
    pub external: ExternalVerdicts,
    /// Exact indexes used by `KeyUnique` checks.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub key_indexes: BTreeMap<CheckKey, KeyIndexState>,
    /// Exact target indexes used by `ReferenceResolves` checks: each target
    /// key's elements bucketed by their scope under the key, as a holder's
    /// reference looks its targets up in the bucket of its own scope.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub reference_indexes: BTreeMap<CheckKey, KeyIndexState>,
}

impl ValidationState {
    /// Canonical bytes used by atomicity and sequence laws.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut writer = panproto_schema::CanonicalWriter::new();
        self.write_canonical(&mut writer);
        writer.into_bytes()
    }

    fn write_canonical<W: CanonicalSink + ?Sized>(&self, w: &mut W) {
        w.write(&self.plan.0);
        w.u64(self.seq);
        write_status(w, self.status);
        w.len(self.open.len());
        for (instance, verdict) in &self.open {
            write_check_instance(w, instance);
            write_open_verdict(w, verdict);
        }
        w.len(self.budget_cuts.len());
        for (key, cut) in &self.budget_cuts {
            write_check_key(w, key);
            w.u64(cut.limit);
            w.u64(cut.spent);
            write_opt_view(w, cut.from.as_ref());
            w.len(cut.scheduled.len());
            for instance in &cut.scheduled {
                write_check_instance(w, instance);
            }
            w.len(cut.skipped.len());
            for instance in &cut.skipped {
                write_check_instance(w, instance);
            }
        }
        w.len(self.coverage.len());
        for (key, coverage) in &self.coverage {
            write_check_key(w, key);
            write_coverage(w, coverage);
        }
        write_external(w, &self.external);
        write_indexes(w, &self.key_indexes);
        write_indexes(w, &self.reference_indexes);
    }

    fn report(
        &self,
        plan: &ValidationPlan,
        shape: InstanceShapeKind,
        scope: ReportScope,
    ) -> ValidationReport {
        let mut diagnostics = Vec::new();
        let mut incomplete = Vec::new();
        for verdict in self.open.values() {
            diagnostics.extend(verdict.diagnostics.iter().cloned());
            incomplete.extend(verdict.incomplete.iter().cloned());
        }
        for (key, cut) in &self.budget_cuts {
            if let Some(spec) = plan.checks().get(key) {
                incomplete.push(IncompleteCheck {
                    check: key.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: None,
                    reason: IncompleteReason::Budget {
                        limit: cut.limit,
                        spent: cut.spent,
                        from: cut.from.clone(),
                    },
                });
            }
        }
        incomplete.extend(plan.static_findings().iter().cloned());
        ValidationReport::new(
            shape,
            scope,
            self.plan,
            diagnostics,
            incomplete,
            self.coverage.clone(),
        )
    }
}

/// The state component that failed checked reattachment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateField {
    /// Plan identity.
    Plan,
    /// Aggregate status.
    Status,
    /// Coverage.
    Coverage,
    /// Evaluated open outcomes.
    Open,
    /// Budget cuts.
    BudgetCuts,
    /// Active external projection.
    External,
    /// Key indexes.
    KeyIndexes,
    /// Reference indexes.
    ReferenceIndexes,
}

/// A checked state does not match its plan or paired instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("validation state mismatch in {field:?}")]
pub struct StateMismatch {
    /// The mismatching component.
    pub field: StateField,
    /// The first affected check instance, when one can be named (boxed, so
    /// that the error stays small; its serialized form is the instance's).
    pub instance: Option<Box<CheckInstance>>,
}

/// Atomic admission policy for an edit.
///
/// On a `Passed` baseline the two policies admit exactly the same edits: a
/// `Passed` baseline has no open required outcome, so `NoNewFailures`
/// admits exactly the results with no required failed or incomplete
/// outcome, which are exactly the `Passed` results `RequirePassed` admits.
/// On a baseline that is not `Passed`, `RequirePassed` is the non-tolerant
/// stance: it admits only an edit that repairs every open required
/// outcome. `NoNewFailures` is inconsistency-tolerant
/// integrity checking (Decker and Martinenghi 2011, Def 3.2; correspondence
/// inferred), with each check instance a case, made stricter in three ways:
/// an open required outcome must keep equal canonical bytes, not merely stay
/// open; an `Incomplete` outcome counts as a failure; and every instance the
/// baseline's budget skipped counts as an open `Incomplete` outcome, so a
/// failure discovered there is new.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionPolicy {
    /// Admit exactly a passed target.
    #[default]
    RequirePassed,
    /// Admit repairs that introduce no new required open outcome.
    NoNewFailures,
}

/// The dry-run result for an edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditValidation {
    /// Whole target report, with edit scope.
    pub report: ValidationReport,
    /// Scheduled checks and carrier support.
    pub footprint: Footprint,
    /// Subjects of scheduled instances together with touched elements.
    pub affected: BTreeSet<ViewRef>,
}

/// An edit failed to apply or was not admitted.
#[derive(Clone, Debug, PartialEq, Serialize, thiserror::Error)]
#[non_exhaustive]
pub enum EditRejection {
    /// Lowering or atomic application failed.
    #[error(transparent)]
    Apply(EditError),
    /// Validation completed, but the selected policy refused it.
    #[error("edit was not admitted")]
    NotAdmitted {
        /// The complete dry-run result.
        validation: Box<EditValidation>,
    },
    /// The state has admitted `u64::MAX` edits, so its sequence number
    /// cannot count another; no edit is applied.
    #[error("the validation state at sequence {seq} cannot admit another edit")]
    SequenceExhausted {
        /// The state's sequence number, `u64::MAX`.
        seq: u64,
    },
}

/// One of the three instance shapes supported by incremental validation.
///
/// The trait is sealed; its associated edit/delta pairs are exactly graph
/// surface/delta, tree edit/edit, and table surface/delta.
pub trait ValidatedShape: sealed::Sealed + Clone {
    /// Caller-facing edit type.
    type Edit;
    /// Canonical transition type.
    type Delta: Clone;

    /// Lower one surface edit to its canonical transition.
    ///
    /// # Errors
    ///
    /// Returns a typed edit error when the surface edit does not lower to an
    /// applicable normalized transition.
    fn lower(edit: &Self::Edit) -> Result<Self::Delta, EditError>;

    /// Apply a canonical transition.
    ///
    /// # Errors
    ///
    /// Returns a typed edit error when the transition is not applicable to
    /// the supplied instance.
    fn apply_delta(delta: &Self::Delta, instance: &mut Self) -> Result<(), EditError>;

    #[doc(hidden)]
    fn shape() -> InstanceShapeKind;
    #[doc(hidden)]
    fn validate(
        &self,
        plan: &ValidationPlan,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport;
    #[doc(hidden)]
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R;
    #[doc(hidden)]
    fn check_instances(&self, plan: &ValidationPlan) -> BTreeSet<CheckInstance> {
        let none = ExternalVerdicts::default();
        self.with_view(|view| Session::new(plan, view, &none).instances())
    }
    #[doc(hidden)]
    fn check_instances_at(
        &self,
        plan: &ValidationPlan,
        candidates: &BTreeSet<ViewRef>,
    ) -> BTreeSet<CheckInstance> {
        let none = ExternalVerdicts::default();
        self.with_view(|view| Session::new(plan, view, &none).instances_at(candidates))
    }
    #[doc(hidden)]
    fn evaluate(
        &self,
        plan: &ValidationPlan,
        external: &ExternalVerdicts,
        instance: &CheckInstance,
    ) -> (OpenVerdict, BTreeSet<ViewRef>) {
        self.with_view(|view| {
            let session = Session::new(plan, view, external);
            let reads = session.trace(instance);
            (session.evaluate(instance, Some(reads.clone())), reads)
        })
    }
    #[doc(hidden)]
    fn reads(
        &self,
        plan: &ValidationPlan,
        external: &ExternalVerdicts,
        instance: &CheckInstance,
    ) -> BTreeSet<ViewRef> {
        self.with_view(|view| Session::new(plan, view, external).trace(instance))
    }
    #[doc(hidden)]
    fn touched(delta: &Self::Delta, pre: &Self, post: &Self) -> BTreeSet<ViewRef>;
    #[doc(hidden)]
    fn candidate_subjects(
        delta: &Self::Delta,
        pre: &Self,
        post: &Self,
        touched: &BTreeSet<ViewRef>,
    ) -> BTreeSet<ViewRef>;
    #[doc(hidden)]
    fn change_kinds(delta: &Self::Delta) -> BTreeSet<DeltaElementKind>;
    #[doc(hidden)]
    fn delta_digest(delta: &Self::Delta) -> Digest;
    #[doc(hidden)]
    fn instance_digest(&self) -> Digest;
}

mod sealed {
    pub trait Sealed {}
}

/// A graph paired with validation state that cannot drift from it.
pub type ValidatedGraph = Validated<GInstance>;
/// A tree paired with validation state that cannot drift from it.
pub type ValidatedTree = Validated<WInstance>;
/// A table paired with validation state that cannot drift from it.
pub type ValidatedTable = Validated<FInstance>;

/// An instance, its immutable plan, and checked incremental state.
#[derive(Clone, Debug)]
pub struct Validated<I: ValidatedShape> {
    instance: I,
    plan: Arc<ValidationPlan>,
    state: ValidationState,
}

impl<I: ValidatedShape> Validated<I> {
    /// Validate an instance once and establish its baseline state.
    ///
    /// The returned report is the whole-instance report of
    /// [`ValidationPlan::validate_graph`] (or its tree and table
    /// counterparts) for the same budget and external verdicts: every
    /// applicable check instance is scheduled in ascending order, and once
    /// the budget cannot pay for an instance, it and every later one are
    /// recorded as budget-skipped without being evaluated.
    #[must_use]
    pub fn new(
        plan: Arc<ValidationPlan>,
        instance: I,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> (Self, ValidationReport) {
        let state = initialize(&plan, &instance, budget, external);
        let report = state.report(&plan, I::shape(), ReportScope::Instance);
        (
            Self {
                instance,
                plan,
                state,
            },
            report,
        )
    }

    /// Reattach serialized state after checking every derived component.
    ///
    /// # Errors
    ///
    /// Returns [`StateMismatch`] when any committed field disagrees with the
    /// plan, instance, or replayed finite-budget schedule.
    pub fn from_parts(
        plan: Arc<ValidationPlan>,
        instance: I,
        state: ValidationState,
    ) -> Result<Self, StateMismatch> {
        check_state(&plan, &instance, &state)?;
        Ok(Self {
            instance,
            plan,
            state,
        })
    }

    /// The paired instance.
    #[must_use]
    pub const fn instance(&self) -> &I {
        &self.instance
    }

    /// The checked state.
    #[must_use]
    pub const fn state(&self) -> &ValidationState {
        &self.state
    }

    /// Reconstruct the instance report without re-evaluating checks.
    #[must_use]
    pub fn report(&self) -> ValidationReport {
        self.state
            .report(&self.plan, I::shape(), ReportScope::Instance)
    }

    /// Separate the instance from its serializable state.
    #[must_use]
    pub fn into_parts(self) -> (I, ValidationState) {
        (self.instance, self.state)
    }

    /// Compute exact instance-set changes and a sound dependency footprint.
    ///
    /// # Errors
    ///
    /// Returns the edit's typed lowering or applicability error.
    pub fn footprint(
        &self,
        edit: &I::Edit,
        external: &ExternalVerdicts,
    ) -> Result<Footprint, EditError> {
        let delta = I::lower(edit)?;
        transition(
            &self.plan,
            &self.state.external,
            &skipped_instances(&self.state),
            &self.instance,
            &delta,
            external,
        )
        .map(|transition| transition.footprint)
    }

    /// Validate an edit without changing either the instance or its state.
    ///
    /// # Errors
    ///
    /// Returns the edit's typed lowering or applicability error.
    pub fn check_edit(
        &self,
        edit: &I::Edit,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> Result<EditValidation, EditError> {
        let delta = I::lower(edit)?;
        prepare_delta(
            &self.plan,
            &self.state,
            &self.instance,
            &delta,
            budget,
            external,
        )
        .map(|prepared| prepared.validation)
    }

    /// Validate and atomically apply an edit with default witness options.
    ///
    /// # Errors
    ///
    /// Returns [`EditRejection::SequenceExhausted`] when the state has
    /// already admitted `u64::MAX` edits, [`EditRejection::Apply`] when the
    /// edit cannot be applied and [`EditRejection::NotAdmitted`] when the
    /// selected policy rejects the complete target validation result.
    pub fn apply_edit(
        &mut self,
        edit: &I::Edit,
        policy: AdmissionPolicy,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> Result<EditWitness, EditRejection> {
        self.apply_edit_with_options(edit, policy, budget, external, &WitnessOptions::default())
    }

    /// Validate and atomically apply an edit with explicit witness options.
    ///
    /// # Errors
    ///
    /// Returns [`EditRejection::SequenceExhausted`] when the state has
    /// already admitted `u64::MAX` edits, [`EditRejection::Apply`] when the
    /// edit cannot be applied and [`EditRejection::NotAdmitted`] when the
    /// selected policy rejects the complete target validation result. On
    /// every error the instance and the state are unchanged.
    pub fn apply_edit_with_options(
        &mut self,
        edit: &I::Edit,
        policy: AdmissionPolicy,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
        options: &WitnessOptions,
    ) -> Result<EditWitness, EditRejection> {
        let Some(next) = self.state.seq.checked_add(1) else {
            return Err(EditRejection::SequenceExhausted {
                seq: self.state.seq,
            });
        };
        let delta = I::lower(edit).map_err(EditRejection::Apply)?;
        let mut prepared = prepare_delta(
            &self.plan,
            &self.state,
            &self.instance,
            &delta,
            budget,
            external,
        )
        .map_err(EditRejection::Apply)?;
        if !admitted(&self.plan, &self.state, &prepared.state, policy) {
            return Err(EditRejection::NotAdmitted {
                validation: Box::new(prepared.validation),
            });
        }
        let witness = super::witness::build_witness(
            &self.plan,
            &self.state,
            &self.instance,
            &delta,
            *options,
            &prepared,
        );
        prepared.state.seq = next;
        self.instance = prepared.instance;
        self.state = prepared.state;
        Ok(witness)
    }
}

/// A validated transition that has not been committed: the target instance
/// and state, the dry-run result, and the post-state trace of every
/// scheduled check instance.
pub struct Prepared<I> {
    pub(crate) instance: I,
    pub(crate) state: ValidationState,
    pub(crate) validation: EditValidation,
    pub(crate) traces: BTreeMap<CheckInstance, BTreeSet<ViewRef>>,
}

/// Validate `delta` from the trusted baseline `(prior, baseline)` under
/// `budget` and the target projection `external`, without committing.
///
/// The target state keeps the baseline's sequence number; the caller that
/// commits it advances that number.
pub fn prepare_delta<I: ValidatedShape>(
    plan: &ValidationPlan,
    baseline: &ValidationState,
    prior: &I,
    delta: &I::Delta,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
) -> Result<Prepared<I>, EditError> {
    let Transition {
        footprint,
        post,
        traces,
    } = transition(
        plan,
        &baseline.external,
        &skipped_instances(baseline),
        prior,
        delta,
        external,
    )?;
    let scheduled: Vec<CheckInstance> =
        footprint.rerun.union(&footprint.created).cloned().collect();
    let state = post.with_view(|view| {
        let session = Session::new(plan, view, external);
        let simulation = simulate(&session, &scheduled, &traces, budget, Walk::Evaluate);
        let next = Next {
            session: &session,
            footprint: &footprint,
            scheduled: &scheduled,
            budget,
            external,
        };
        next.state(plan, baseline, simulation, I::shape())
    });
    let report = state.report(
        plan,
        I::shape(),
        ReportScope::Edit {
            baseline_seq: baseline.seq,
            baseline_status: baseline.status,
        },
    );
    let mut affected = footprint.touched.clone();
    affected.extend(
        footprint
            .rerun
            .iter()
            .chain(&footprint.created)
            .filter_map(|instance| instance.subject.clone()),
    );
    Ok(Prepared {
        instance: post,
        state,
        validation: EditValidation {
            report,
            footprint,
            affected,
        },
        traces,
    })
}

/// What a transition's target state is assembled from: the target view's
/// session, the footprint, the schedule it ran, its budget and the target
/// external projection.
struct Next<'s, 'a> {
    session: &'s Session<'a>,
    footprint: &'s Footprint,
    scheduled: &'s [CheckInstance],
    budget: &'s ValidationBudget,
    external: &'s ExternalVerdicts,
}

impl Next<'_, '_> {
    /// The target state: the baseline with every retired and scheduled
    /// bundle replaced by the simulation's nonempty outcomes, the cuts of
    /// the schedule, and coverage and active external projection updated
    /// at exactly the retired, rerun and created instances (every other
    /// instance is carried with its count and verdict); the indexes are the
    /// target view's.
    fn state(
        &self,
        plan: &ValidationPlan,
        baseline: &ValidationState,
        simulation: Simulation,
        shape: InstanceShapeKind,
    ) -> ValidationState {
        let footprint = self.footprint;
        let mut open = baseline.open.clone();
        for instance in footprint.retired.iter().chain(self.scheduled) {
            open.remove(instance);
        }
        for (instance, verdict) in simulation.evaluated {
            if !verdict.diagnostics.is_empty() || !verdict.incomplete.is_empty() {
                open.insert(instance, verdict);
            }
        }
        let budget_cuts = cuts_for_schedule(
            plan,
            self.scheduled,
            &simulation.skipped,
            self.budget,
            simulation.spent,
        );
        let skipped: BTreeSet<CheckInstance> = simulation.skipped.into_iter().collect();
        let baseline_skipped = skipped_instances(baseline);
        let mut coverage = baseline.coverage.clone();
        for instance in footprint.retired.iter().chain(&footprint.rerun) {
            if let Some(entry) = coverage.get_mut(&instance.check) {
                let was = schedule::classify(plan, instance, &baseline.open, &baseline_skipped);
                schedule::count(entry, was, false);
                if footprint.retired.contains(instance) {
                    entry.instances -= 1;
                }
            }
        }
        for instance in footprint.rerun.iter().chain(&footprint.created) {
            if let Some(entry) = coverage.get_mut(&instance.check) {
                let now = schedule::classify(plan, instance, &open, &skipped);
                schedule::count(entry, now, true);
                if footprint.created.contains(instance) {
                    entry.instances += 1;
                }
            }
        }
        let mut state = ValidationState {
            plan: *plan.digest(),
            seq: baseline.seq,
            status: CheckStatus::Passed,
            open,
            budget_cuts,
            coverage,
            external: self.active_external(plan, &baseline.external),
            key_indexes: BTreeMap::new(),
            reference_indexes: BTreeMap::new(),
        };
        (state.key_indexes, state.reference_indexes) = self.session.indexes();
        state.status = state.report(plan, shape, ReportScope::Instance).status;
        state
    }

    /// The active external projection after the transition: the baseline's,
    /// updated at every `(name, element)` of a retired, rerun or created
    /// external instance to the target verdict when some external check of
    /// that name still has an instance at the element, and removed
    /// otherwise.
    fn active_external(
        &self,
        plan: &ValidationPlan,
        baseline: &ExternalVerdicts,
    ) -> ExternalVerdicts {
        let footprint = self.footprint;
        let mut active = baseline.clone();
        let affected: BTreeSet<(&Name, &ViewRef)> = footprint
            .retired
            .iter()
            .chain(&footprint.rerun)
            .chain(&footprint.created)
            .filter_map(
                |instance| match (plan.compiled.get(&instance.check), &instance.subject) {
                    (Some(Compiled::External { name }), Some(subject)) => Some((name, subject)),
                    _ => None,
                },
            )
            .collect();
        for (name, subject) in affected {
            let addressed = plan.compiled.iter().any(|(key, compiled)| {
                matches!(compiled, Compiled::External { name: n } if n == name)
                    && self.session.holds(&CheckInstance {
                        check: key.clone(),
                        subject: Some(subject.clone()),
                    })
            });
            match self.external.get(name, subject).filter(|_| addressed) {
                Some(verdict) => {
                    active.insert(name.clone(), subject.clone(), *verdict);
                }
                None => {
                    active.0.remove(&(name.clone(), subject.clone()));
                }
            }
        }
        active
    }
}

/// Whether `policy` admits the transition from `baseline` to `target`.
///
/// `RequirePassed` admits exactly a `Passed` target. `NoNewFailures`
/// admits iff every check instance with a required failed or incomplete
/// entry in the target had, in the baseline, the same required entries,
/// compared by canonical bytes; a baseline budget-skipped instance counts
/// as one required budget incompleteness. Advisory entries decide neither
/// policy.
pub fn admitted(
    plan: &ValidationPlan,
    baseline: &ValidationState,
    target: &ValidationState,
    policy: AdmissionPolicy,
) -> bool {
    match policy {
        AdmissionPolicy::RequirePassed => target.status == CheckStatus::Passed,
        AdmissionPolicy::NoNewFailures => {
            let before = required_outcomes(plan, baseline);
            required_outcomes(plan, target)
                .iter()
                .all(|(instance, verdict)| {
                    before
                        .get(instance)
                        .is_some_and(|old| open_bytes(old) == open_bytes(verdict))
                })
        }
    }
}

/// The required entries of every open bundle, and a virtual required
/// budget incompleteness for every budget-skipped instance of a required
/// check; instances with no required entry are left out.
fn required_outcomes(
    plan: &ValidationPlan,
    state: &ValidationState,
) -> BTreeMap<CheckInstance, OpenVerdict> {
    let mut outcomes: BTreeMap<CheckInstance, OpenVerdict> = BTreeMap::new();
    for (instance, verdict) in &state.open {
        let required = OpenVerdict {
            diagnostics: verdict
                .diagnostics
                .iter()
                .filter(|d| d.requirement == Requirement::Required)
                .cloned()
                .collect(),
            incomplete: verdict
                .incomplete
                .iter()
                .filter(|i| i.requirement == Requirement::Required)
                .cloned()
                .collect(),
        };
        if !required.diagnostics.is_empty() || !required.incomplete.is_empty() {
            outcomes.insert(instance.clone(), required);
        }
    }
    for instance in skipped_instances(state) {
        let (Some(spec), Some(cut)) = (
            plan.checks().get(&instance.check),
            state.budget_cuts.get(&instance.check),
        ) else {
            continue;
        };
        if spec.requirement != Requirement::Required {
            continue;
        }
        // The virtual entry is the instance's share of its own key's
        // reported budget incompleteness.
        let entry = IncompleteCheck {
            check: instance.check.clone(),
            class: spec.class,
            requirement: spec.requirement,
            subject: instance.subject.clone(),
            reason: IncompleteReason::Budget {
                limit: cut.limit,
                spent: cut.spent,
                from: cut.from.clone(),
            },
        };
        outcomes.entry(instance).or_default().incomplete.push(entry);
    }
    outcomes
}

/// The components of a state that the transition or initialization
/// computes over the target view.
struct Parts {
    seq: u64,
    open: BTreeMap<CheckInstance, OpenVerdict>,
    external: ExternalVerdicts,
    indexes: (
        BTreeMap<CheckKey, KeyIndexState>,
        BTreeMap<CheckKey, KeyIndexState>,
    ),
}

/// Validate `instance` in full under `budget` and `external`, as the
/// baseline state of sequence number zero.
fn initialize<I: ValidatedShape>(
    plan: &ValidationPlan,
    instance: &I,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
) -> ValidationState {
    instance.with_view(|view| {
        let session = Session::new(plan, view, external);
        let all = session.instances();
        let scheduled: Vec<CheckInstance> = all.iter().cloned().collect();
        let simulation = simulate(
            &session,
            &scheduled,
            &BTreeMap::new(),
            budget,
            Walk::Evaluate,
        );
        let parts = Parts {
            seq: 0,
            open: BTreeMap::new(),
            external: session.active_external(&all, external),
            indexes: session.indexes(),
        };
        finish_state(
            plan,
            I::shape(),
            &all,
            parts,
            simulation,
            &scheduled,
            budget,
        )
    })
}

/// The state whose open bundles are `parts.open` updated with every
/// nonempty evaluated outcome of `simulation`, whose budget cuts commit
/// `scheduled` and the simulation's skipped suffix, and whose coverage and
/// status follow from them over the instance set `all`.
fn finish_state(
    plan: &ValidationPlan,
    shape: InstanceShapeKind,
    all: &BTreeSet<CheckInstance>,
    parts: Parts,
    simulation: Simulation,
    scheduled: &[CheckInstance],
    budget: &ValidationBudget,
) -> ValidationState {
    let Parts {
        seq,
        mut open,
        external,
        indexes: (key_indexes, reference_indexes),
    } = parts;
    for (instance, verdict) in simulation.evaluated {
        if !verdict.diagnostics.is_empty() || !verdict.incomplete.is_empty() {
            open.insert(instance, verdict);
        }
    }
    let budget_cuts = cuts_for_schedule(
        plan,
        scheduled,
        &simulation.skipped,
        budget,
        simulation.spent,
    );
    let skipped: BTreeSet<CheckInstance> = simulation.skipped.into_iter().collect();
    let coverage = schedule::coverage(plan, shape, all, &open, &skipped);
    let mut state = ValidationState {
        plan: *plan.digest(),
        seq,
        status: CheckStatus::Passed,
        open,
        budget_cuts,
        coverage,
        external,
        key_indexes,
        reference_indexes,
    };
    state.status = state.report(plan, shape, ReportScope::Instance).status;
    state
}

/// One budget cut per check key with a skipped instance, each repeating
/// the exact schedule and its skipped suffix; none without a limit.
fn cuts_for_schedule(
    plan: &ValidationPlan,
    scheduled: &[CheckInstance],
    skipped: &[CheckInstance],
    budget: &ValidationBudget,
    spent: u64,
) -> BTreeMap<CheckKey, BudgetCut> {
    let Some(limit) = budget.max_reads else {
        return BTreeMap::new();
    };
    let mut cuts = BTreeMap::new();
    for instance in skipped {
        if !plan.checks().contains_key(&instance.check) {
            continue;
        }
        cuts.entry(instance.check.clone())
            .or_insert_with(|| BudgetCut {
                limit,
                spent,
                from: instance.subject.clone(),
                scheduled: scheduled.to_vec(),
                skipped: skipped.to_vec(),
            });
    }
    cuts
}

/// Every budget-skipped instance a state commits.
pub fn skipped_instances(state: &ValidationState) -> BTreeSet<CheckInstance> {
    state
        .budget_cuts
        .values()
        .flat_map(|cut| cut.skipped.iter().cloned())
        .collect()
}

/// Check every component of `state` against the plan and its paired
/// instance: the plan digest, the budget cuts by replaying their committed
/// schedule, the active external projection, both index maps, the open
/// bundles by evaluating every instance outside the skipped suffix, then
/// coverage and status.
pub fn check_state<I: ValidatedShape>(
    plan: &ValidationPlan,
    instance: &I,
    state: &ValidationState,
) -> Result<(), StateMismatch> {
    if state.plan != *plan.digest() {
        return Err(mismatch(StateField::Plan));
    }
    instance.with_view(|view| {
        let session = Session::new(plan, view, &state.external);
        let all = session.instances();
        let skipped = checked_budget_cuts(plan, &session, &all, state)?;
        if session.active_external(&all, &state.external) != state.external {
            return Err(mismatch(StateField::External));
        }
        let (expected_keys, expected_references) = session.indexes();
        if index_map_bytes(&state.key_indexes) != index_map_bytes(&expected_keys) {
            return Err(mismatch(StateField::KeyIndexes));
        }
        if index_map_bytes(&state.reference_indexes) != index_map_bytes(&expected_references) {
            return Err(mismatch(StateField::ReferenceIndexes));
        }
        let mut expected_open = BTreeMap::new();
        for check in &all {
            if session.inert(check) || skipped.contains(check) {
                continue;
            }
            let verdict = session.evaluate(check, None);
            if !verdict.diagnostics.is_empty() || !verdict.incomplete.is_empty() {
                expected_open.insert(check.clone(), verdict);
            }
        }
        if open_map_bytes(&state.open) != open_map_bytes(&expected_open) {
            return Err(StateMismatch {
                field: StateField::Open,
                instance: first_open_difference(&state.open, &expected_open).map(Box::new),
            });
        }
        let coverage = schedule::coverage(plan, I::shape(), &all, &state.open, &skipped);
        if state.coverage != coverage {
            return Err(mismatch(StateField::Coverage));
        }
        if state.status != state.report(plan, I::shape(), ReportScope::Instance).status {
            return Err(mismatch(StateField::Status));
        }
        Ok(())
    })
}

/// The skipped instances of the state's budget cuts, after checking that
/// every cut repeats one strictly ascending schedule of existing instances
/// and its skipped suffix, and that replaying the schedule under the cut's
/// limit, without evaluating it, gives exactly the committed spent count,
/// skipped suffix and cut keys.
fn checked_budget_cuts(
    plan: &ValidationPlan,
    session: &Session<'_>,
    all: &BTreeSet<CheckInstance>,
    state: &ValidationState,
) -> Result<BTreeSet<CheckInstance>, StateMismatch> {
    let Some(first) = state.budget_cuts.values().next() else {
        return Ok(BTreeSet::new());
    };
    let strictly_sorted = |checks: &[CheckInstance]| checks.windows(2).all(|w| w[0] < w[1]);
    if !strictly_sorted(&first.scheduled) || !strictly_sorted(&first.skipped) {
        return Err(mismatch(StateField::BudgetCuts));
    }
    if state.budget_cuts.values().any(|cut| {
        cut.limit != first.limit
            || cut.spent != first.spent
            || cut.scheduled != first.scheduled
            || cut.skipped != first.skipped
    }) {
        return Err(mismatch(StateField::BudgetCuts));
    }
    if first.scheduled.iter().any(|check| !all.contains(check)) {
        return Err(mismatch(StateField::BudgetCuts));
    }
    let budget = ValidationBudget {
        max_reads: Some(first.limit),
    };
    let replay = simulate(
        session,
        &first.scheduled,
        &BTreeMap::new(),
        &budget,
        Walk::Replay,
    );
    if replay.skipped != first.skipped || replay.spent != first.spent {
        return Err(mismatch(StateField::BudgetCuts));
    }
    let expected = cuts_for_schedule(
        plan,
        &first.scheduled,
        &replay.skipped,
        &budget,
        replay.spent,
    );
    if expected != state.budget_cuts {
        return Err(mismatch(StateField::BudgetCuts));
    }
    Ok(replay.skipped.into_iter().collect())
}

const fn mismatch(field: StateField) -> StateMismatch {
    StateMismatch {
        field,
        instance: None,
    }
}

fn first_open_difference(
    left: &BTreeMap<CheckInstance, OpenVerdict>,
    right: &BTreeMap<CheckInstance, OpenVerdict>,
) -> Option<CheckInstance> {
    left.keys()
        .chain(right.keys())
        .find(|key| {
            left.get(*key).map(open_bytes).as_deref() != right.get(*key).map(open_bytes).as_deref()
        })
        .cloned()
}

impl sealed::Sealed for GInstance {}
impl ValidatedShape for GInstance {
    type Edit = GraphEdit;
    type Delta = GraphDelta;

    fn lower(edit: &Self::Edit) -> Result<Self::Delta, EditError> {
        edit.to_delta().map_err(EditError::from)
    }

    fn apply_delta(delta: &Self::Delta, instance: &mut Self) -> Result<(), EditError> {
        delta.apply(instance)
    }

    fn shape() -> InstanceShapeKind {
        InstanceShapeKind::Graph
    }

    fn validate(
        &self,
        plan: &ValidationPlan,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport {
        plan.validate_graph(self, budget, external)
    }

    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        f(InstanceView::Graph(self))
    }
    fn touched(delta: &Self::Delta, _pre: &Self, _post: &Self) -> BTreeSet<ViewRef> {
        let mut touched: BTreeSet<ViewRef> = delta
            .touched_nodes()
            .into_iter()
            .map(ViewRef::Node)
            .collect();
        touched.extend(delta.touched_edges().into_iter().map(ViewRef::Edge));
        touched
    }

    fn candidate_subjects(
        _delta: &Self::Delta,
        pre: &Self,
        post: &Self,
        touched: &BTreeSet<ViewRef>,
    ) -> BTreeSet<ViewRef> {
        let mut candidates = touched.clone();
        for at in touched {
            match at {
                ViewRef::Node(node) => {
                    for graph in [pre, post] {
                        candidates.extend(
                            graph
                                .out_edges(*node)
                                .iter()
                                .chain(graph.in_edges(*node))
                                .copied()
                                .map(ViewRef::Edge),
                        );
                    }
                }
                ViewRef::Edge(edge) => {
                    for graph in [pre, post] {
                        if let Some(record) = graph.edge(*edge) {
                            candidates.insert(ViewRef::Node(record.src));
                            candidates.insert(ViewRef::Node(record.tgt));
                        }
                    }
                }
                _ => {}
            }
        }
        candidates
    }

    fn change_kinds(delta: &Self::Delta) -> BTreeSet<DeltaElementKind> {
        let mut out = BTreeSet::new();
        if !delta.add_nodes().is_empty() {
            out.insert(DeltaElementKind::AddNode);
        }
        if !delta.del_nodes().is_empty() {
            out.insert(DeltaElementKind::DelNode);
        }
        if !delta.add_edges().is_empty() {
            out.insert(DeltaElementKind::AddEdge);
        }
        if !delta.del_edges().is_empty() {
            out.insert(DeltaElementKind::DelEdge);
        }
        for key in delta.attrs().keys() {
            out.insert(match key {
                AttrKey::Value { .. } => DeltaElementKind::AttrValue,
                AttrKey::Field { .. } => DeltaElementKind::AttrField,
                AttrKey::Meta { .. } => DeltaElementKind::AttrMeta,
                AttrKey::Anchor { .. } => DeltaElementKind::AttrAnchor,
                AttrKey::EdgeLabel { .. } => DeltaElementKind::AttrEdgeLabel,
                AttrKey::EdgePosition { .. } => DeltaElementKind::AttrEdgePosition,
                AttrKey::Cell { .. } | AttrKey::Table { .. } => continue,
            });
        }
        out
    }

    fn delta_digest(delta: &Self::Delta) -> Digest {
        delta.digest()
    }

    fn instance_digest(&self) -> Digest {
        self.digest()
    }
}

impl sealed::Sealed for FInstance {}
impl ValidatedShape for FInstance {
    type Edit = TableEdit;
    type Delta = TableDelta;

    fn lower(edit: &Self::Edit) -> Result<Self::Delta, EditError> {
        edit.to_delta().map_err(EditError::from)
    }

    fn apply_delta(delta: &Self::Delta, instance: &mut Self) -> Result<(), EditError> {
        delta.apply(instance)
    }

    fn shape() -> InstanceShapeKind {
        InstanceShapeKind::Table
    }

    fn validate(
        &self,
        plan: &ValidationPlan,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport {
        plan.validate_table(self, budget, external)
    }

    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        f(InstanceView::Table(self))
    }
    fn touched(delta: &Self::Delta, _pre: &Self, _post: &Self) -> BTreeSet<ViewRef> {
        let mut touched: BTreeSet<ViewRef> =
            delta.touched_rows().into_iter().map(ViewRef::Row).collect();
        touched.extend(
            delta
                .touched_pairs()
                .into_iter()
                .map(|(edge, src, tgt)| ViewRef::Pair { edge, src, tgt }),
        );
        touched
    }

    fn candidate_subjects(
        _delta: &Self::Delta,
        pre: &Self,
        post: &Self,
        touched: &BTreeSet<ViewRef>,
    ) -> BTreeSet<ViewRef> {
        let mut candidates = touched.clone();
        for at in touched {
            match at {
                ViewRef::Row(row) => {
                    for table in [pre, post] {
                        for (edge, pairs) in table.foreign_keys() {
                            candidates.extend(
                                pairs
                                    .iter()
                                    .filter(|(src, tgt)| src == row || tgt == row)
                                    .map(|(src, tgt)| ViewRef::Pair {
                                        edge: edge.clone(),
                                        src: *src,
                                        tgt: *tgt,
                                    }),
                            );
                        }
                    }
                }
                ViewRef::Pair { src, tgt, .. } => {
                    candidates.insert(ViewRef::Row(*src));
                    candidates.insert(ViewRef::Row(*tgt));
                }
                _ => {}
            }
        }
        candidates
    }

    fn change_kinds(delta: &Self::Delta) -> BTreeSet<DeltaElementKind> {
        let mut out = BTreeSet::new();
        if !delta.add_rows().is_empty() {
            out.insert(DeltaElementKind::AddRow);
        }
        if !delta.del_rows().is_empty() {
            out.insert(DeltaElementKind::DelRow);
        }
        if !delta.add_pairs().is_empty() {
            out.insert(DeltaElementKind::AddPair);
        }
        if !delta.del_pairs().is_empty() {
            out.insert(DeltaElementKind::DelPair);
        }
        if !delta.cells().is_empty() {
            out.insert(DeltaElementKind::Cell);
        }
        if !delta.tables().is_empty() {
            out.insert(DeltaElementKind::Table);
        }
        out
    }

    fn delta_digest(delta: &Self::Delta) -> Digest {
        delta.digest()
    }

    fn instance_digest(&self) -> Digest {
        self.digest()
    }
}

/// The out-arc sequence of each node of a tree (each child with the arc's
/// label, in arc order) and its in-arc sequence (each parent with the
/// label, in arc order).
type Incidence<'a> = BTreeMap<u32, Vec<(u32, &'a Edge)>>;

fn tree_incidence(tree: &WInstance) -> (Incidence<'_>, Incidence<'_>) {
    let mut out: Incidence<'_> = BTreeMap::new();
    let mut inn: Incidence<'_> = BTreeMap::new();
    for (parent, child, label) in &tree.arcs {
        out.entry(*parent).or_default().push((*child, label));
        inn.entry(*child).or_default().push((*parent, label));
    }
    (out, inn)
}

/// The labels of each arc `(parent, child)` of a tree, in arc order.
fn tree_arc_labels(tree: &WInstance) -> BTreeMap<(u32, u32), Vec<&Edge>> {
    let mut arcs: BTreeMap<(u32, u32), Vec<&Edge>> = BTreeMap::new();
    for (parent, child, label) in &tree.arcs {
        arcs.entry((*parent, *child)).or_default().push(label);
    }
    arcs
}

impl sealed::Sealed for WInstance {}
impl ValidatedShape for WInstance {
    type Edit = TreeEdit;
    type Delta = TreeEdit;

    fn lower(edit: &Self::Edit) -> Result<Self::Delta, EditError> {
        let mut steps: Vec<TreeEdit> = edit.steps().into_iter().cloned().collect();
        Ok(match steps.len() {
            0 => TreeEdit::Identity,
            1 => steps.pop().unwrap_or(TreeEdit::Identity),
            _ => TreeEdit::Sequence(steps),
        })
    }

    fn apply_delta(delta: &Self::Delta, instance: &mut Self) -> Result<(), EditError> {
        delta.apply(instance)
    }

    fn shape() -> InstanceShapeKind {
        InstanceShapeKind::Tree
    }

    fn validate(
        &self,
        plan: &ValidationPlan,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport {
        plan.validate_tree(self, budget, external)
    }

    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        let arcs = TreeArcs::new(self);
        f(InstanceView::Tree(self, &arcs))
    }
    /// The exact carrier support of a tree edit: every node whose record or
    /// recorded parent differs between `pre` and `post` (present in one of
    /// them only included), every arc `(parent, child)` whose labels differ,
    /// the parent and children of every fan present in one of them only,
    /// and both roots when the root changes.
    fn touched(_delta: &Self::Delta, pre: &Self, post: &Self) -> BTreeSet<ViewRef> {
        let record = |tree: &Self, id: u32| {
            tree.nodes.get(&id).map(|node| {
                let mut w = panproto_schema::CanonicalWriter::new();
                crate::ginstance::write_node_record(&mut w, node);
                (w.into_bytes(), tree.parent_map.get(&id).copied())
            })
        };
        let mut out = BTreeSet::new();
        let ids: BTreeSet<u32> = pre.nodes.keys().chain(post.nodes.keys()).copied().collect();
        for id in ids {
            if record(pre, id) != record(post, id) {
                out.insert(ViewRef::Node(id));
            }
        }
        // A node's ordered children and ordered parents are read as one
        // sequence each, through the node: a node whose sequence of out-arcs
        // or of in-arcs changes, in membership, label or order, is touched.
        let (out_before, in_before) = tree_incidence(pre);
        let (out_after, in_after) = tree_incidence(post);
        for (sequences_before, sequences_after) in
            [(&out_before, &out_after), (&in_before, &in_after)]
        {
            for id in sequences_before.keys().chain(sequences_after.keys()) {
                if sequences_before.get(id) != sequences_after.get(id) {
                    out.insert(ViewRef::Node(*id));
                }
            }
        }
        let (before, after) = (tree_arc_labels(pre), tree_arc_labels(post));
        for key in before.keys().chain(after.keys()) {
            if before.get(key) != after.get(key) {
                out.insert(ViewRef::Arc {
                    parent: key.0,
                    child: key.1,
                });
            }
        }
        let fans = |tree: &Self| {
            let mut fans: Vec<FanRecord> = tree.fans.iter().map(FanRecord::of).collect();
            fans.sort();
            fans
        };
        let (before, after) = (fans(pre), fans(post));
        for fan in before
            .iter()
            .filter(|f| !after.contains(f))
            .chain(after.iter().filter(|f| !before.contains(f)))
        {
            out.insert(ViewRef::Node(fan.parent));
            out.extend(fan.children.iter().map(|(_, id)| ViewRef::Node(*id)));
        }
        if pre.root != post.root {
            out.insert(ViewRef::Node(pre.root));
            out.insert(ViewRef::Node(post.root));
        }
        out
    }

    fn candidate_subjects(
        _delta: &Self::Delta,
        pre: &Self,
        post: &Self,
        touched: &BTreeSet<ViewRef>,
    ) -> BTreeSet<ViewRef> {
        let mut candidates = touched.clone();
        let mut changed_subtree_roots = BTreeSet::new();
        for at in touched {
            match at {
                ViewRef::Node(node) => {
                    for tree in [pre, post] {
                        for (parent, child, _) in &tree.arcs {
                            if parent == node || child == node {
                                candidates.insert(ViewRef::Arc {
                                    parent: *parent,
                                    child: *child,
                                });
                            }
                        }
                    }
                }
                ViewRef::Arc { parent, child } => {
                    candidates.insert(ViewRef::Node(*parent));
                    candidates.insert(ViewRef::Node(*child));
                    changed_subtree_roots.insert(*child);
                }
                _ => {}
            }
        }
        // A fan's well-formedness instance, at its parent, reads the parent
        // and every child of every fan there: a touched parent or child
        // makes the parent a candidate, before and after.
        for tree in [pre, post] {
            for fan in &tree.fans {
                if std::iter::once(&fan.parent)
                    .chain(fan.children.values())
                    .any(|id| touched.contains(&ViewRef::Node(*id)))
                {
                    candidates.insert(ViewRef::Node(fan.parent));
                }
            }
        }
        // A reachability instance at any descendant reads every ancestor arc.
        // Thus each changed arc invalidates the complete pre/post subtree rooted
        // at its child, independently of which tree edit produced the arc.
        // The walk visits each node once, so an arc cycle off the root
        // (which a tree may hold, and `TreeReachable` reports) ends it.
        for tree in [pre, post] {
            let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for (parent, child, _) in &tree.arcs {
                children.entry(*parent).or_default().push(*child);
            }
            let mut seen = BTreeSet::new();
            let mut frontier: Vec<u32> = changed_subtree_roots.iter().copied().collect();
            while let Some(parent) = frontier.pop() {
                if !seen.insert(parent) {
                    continue;
                }
                candidates.insert(ViewRef::Node(parent));
                for child in children.get(&parent).into_iter().flatten() {
                    candidates.insert(ViewRef::Arc {
                        parent,
                        child: *child,
                    });
                    frontier.push(*child);
                }
            }
        }
        candidates
    }

    fn change_kinds(delta: &Self::Delta) -> BTreeSet<DeltaElementKind> {
        delta
            .steps()
            .into_iter()
            .filter_map(|step| match step {
                TreeEdit::InsertNode { .. } => Some(DeltaElementKind::InsertNode),
                TreeEdit::DeleteNode { .. } => Some(DeltaElementKind::DeleteNode),
                TreeEdit::ContractNode { .. } => Some(DeltaElementKind::ContractNode),
                TreeEdit::RelabelNode { .. } => Some(DeltaElementKind::RelabelNode),
                TreeEdit::SetField { .. } => Some(DeltaElementKind::SetField),
                TreeEdit::RemoveField { .. } => Some(DeltaElementKind::RemoveField),
                TreeEdit::MoveSubtree { .. } => Some(DeltaElementKind::MoveSubtree),
                TreeEdit::InsertFan { .. } => Some(DeltaElementKind::InsertFan),
                TreeEdit::DeleteFan { .. } => Some(DeltaElementKind::DeleteFan),
                TreeEdit::JoinFeatures { .. } => Some(DeltaElementKind::JoinFeatures),
                // `steps` flattens sequences and drops identities.
                TreeEdit::Identity | TreeEdit::Sequence(_) => None,
            })
            .collect()
    }

    fn delta_digest(delta: &Self::Delta) -> Digest {
        delta.digest()
    }

    fn instance_digest(&self) -> Digest {
        crate::segment::winstance_digest(self)
    }
}

fn write_check_instance<W: CanonicalSink + ?Sized>(w: &mut W, instance: &CheckInstance) {
    write_check_key(w, &instance.check);
    write_opt_view(w, instance.subject.as_ref());
}

fn write_opt_view<W: CanonicalSink + ?Sized>(w: &mut W, value: Option<&ViewRef>) {
    match value {
        None => w.tag(0),
        Some(value) => {
            w.tag(1);
            write_view_ref(w, value);
        }
    }
}

fn write_open_verdict<W: CanonicalSink + ?Sized>(w: &mut W, verdict: &OpenVerdict) {
    w.len(verdict.diagnostics.len());
    for diagnostic in &verdict.diagnostics {
        write_diagnostic(w, diagnostic);
    }
    w.len(verdict.incomplete.len());
    for incomplete in &verdict.incomplete {
        write_incomplete(w, incomplete);
    }
}

fn write_coverage<W: CanonicalSink + ?Sized>(w: &mut W, coverage: &CheckCoverage) {
    w.tag(match coverage.class {
        super::report::CheckClass::Static => 0,
        super::report::CheckClass::GraphLocal => 1,
        super::report::CheckClass::MaintainedGlobal => 2,
        super::report::CheckClass::BoundedExternal => 3,
    });
    w.tag(match coverage.requirement {
        Requirement::Required => 0,
        Requirement::Advisory => 1,
    });
    w.u64(coverage.instances);
    w.u64(coverage.passed);
    w.u64(coverage.failed);
    w.u64(coverage.incomplete);
}

fn write_external<W: CanonicalSink + ?Sized>(w: &mut W, external: &ExternalVerdicts) {
    w.len(external.0.len());
    for ((name, element), verdict) in &external.0 {
        w.name(name);
        write_view_ref(w, element);
        w.tag(u8::from(verdict.passed));
        w.write(&verdict.witness.0);
    }
}

fn write_indexes<W: CanonicalSink + ?Sized>(
    w: &mut W,
    indexes: &BTreeMap<CheckKey, KeyIndexState>,
) {
    w.len(indexes.len());
    for (key, index) in indexes {
        write_check_key(w, key);
        write_index(w, index);
    }
}

fn write_index<W: CanonicalSink + ?Sized>(w: &mut W, index: &KeyIndexState) {
    w.len(index.buckets.len());
    for ((scope, bytes), elements) in &index.buckets {
        write_opt_view(w, scope.as_ref());
        w.bytes(bytes);
        write_views(w, elements);
    }
    w.len(index.by_value.len());
    for (bytes, elements) in &index.by_value {
        w.bytes(bytes);
        write_views(w, elements);
    }
    w.len(index.values.len());
    for (element, values) in &index.values {
        write_view_ref(w, element);
        w.len(values.len());
        for value in values {
            w.value(value);
        }
    }
}

fn write_views<W: CanonicalSink + ?Sized>(w: &mut W, values: &[ViewRef]) {
    w.len(values.len());
    for value in values {
        write_view_ref(w, value);
    }
}

pub fn open_bytes(open: &OpenVerdict) -> Vec<u8> {
    let mut writer = panproto_schema::CanonicalWriter::new();
    write_open_verdict(&mut writer, open);
    writer.into_bytes()
}

fn open_map_bytes(open: &BTreeMap<CheckInstance, OpenVerdict>) -> Vec<u8> {
    let mut writer = panproto_schema::CanonicalWriter::new();
    writer.len(open.len());
    for (instance, verdict) in open {
        write_check_instance(&mut writer, instance);
        write_open_verdict(&mut writer, verdict);
    }
    writer.into_bytes()
}

fn index_map_bytes(indexes: &BTreeMap<CheckKey, KeyIndexState>) -> Vec<u8> {
    let mut writer = panproto_schema::CanonicalWriter::new();
    write_indexes(&mut writer, indexes);
    writer.into_bytes()
}
