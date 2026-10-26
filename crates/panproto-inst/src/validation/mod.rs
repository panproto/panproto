//! Validation of instances against their schemas.
//!
//! Validation is a boundary: every public validator returns a
//! [`ValidationReport`] whose status is a [`CheckStatus`](panproto_gat::CheckStatus),
//! `Passed`, `Failed` or `Incomplete`, never an empty list or a bool. A
//! report lists every failed check instance as a typed [`Diagnostic`], every
//! check instance it could not decide as an [`IncompleteCheck`] with its
//! reason, and how the instances of each compiled check fared.
//!
//! A [`ValidationPlan`] compiles a schema, its protocol's constraint
//! semantics and the caller's [`ValidationOptions`] into checks once;
//! [`ValidationPlan::validate_graph`], [`ValidationPlan::validate_tree`] and
//! [`ValidationPlan::validate_table`] run them over an instance of each
//! shape within a [`ValidationBudget`], reading application judgments from
//! [`ExternalVerdicts`]. [`validate_graph`], [`validate_wtype`] and
//! [`validate_table`] compile and run in one call, with an unlimited budget
//! and no external verdicts.
//!
//! Trees, tables and graphs each have their own validator, and validation is
//! never routed through a shape bridge: a tree is validated as a tree, with
//! the tree-only checks of its root, parent map, reachability and reserved
//! vocabulary.
//!
//! A [`ConstraintSemantics::Format`](panproto_schema::ConstraintSemantics::Format)
//! constraint is evaluated for the fixed set of [`ValueFormat`]s: `date-time`
//! (also `datetime`), `date`, `time`, `uri`, `uuid`, `ipv4`, `ipv6`, `did`,
//! and the AT Protocol Lexicon string formats `at-identifier`, `at-uri`,
//! `cid`, `handle`, `nsid`, `tid`, `record-key` and `language`. Any other
//! format is an unsupported declaration. The constraint's format family
//! selects additional rules: the AT Protocol Lexicon (family `atproto`)
//! also applies its DID rules to `did`.
//!
//! ```
//! use panproto_inst::validation::validate_graph;
//! use panproto_inst::{GInstance, Node};
//! use panproto_gat::CheckStatus;
//! use panproto_schema::{Protocol, Schema, Vertex};
//!
//! let mut schema = Schema::default();
//! schema.vertices.insert(
//!     "person".into(),
//!     Vertex { id: "person".into(), kind: "object".into(), nsid: None },
//! );
//! let protocol = Protocol::default();
//!
//! let ok = GInstance::new().with_node(Node::new(0, "person"));
//! assert_eq!(validate_graph(&ok, &schema, &protocol).status, CheckStatus::Passed);
//!
//! let ghost = GInstance::new().with_node(Node::new(0, "ghost"));
//! let report = validate_graph(&ghost, &schema, &protocol);
//! assert_eq!(report.status, CheckStatus::Failed);
//! assert_eq!(report.diagnostics.len(), 1);
//! ```

mod checks;
mod did;
mod footprint;
mod lexicon_formats;
mod multiformats;
mod plan;
mod report;
mod schedule;
mod state;
#[cfg(test)]
mod state_tests;
mod witness;

use std::collections::{BTreeMap, BTreeSet};

use panproto_schema::{Protocol, Schema};

use crate::edit_error::EditError;
use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::graph_edit::GraphDelta;
use crate::table_delta::TableDelta;
use crate::tree_edit::TreeEdit;
use crate::view_ref::ViewRef;
use crate::wtype::WInstance;

use checks::{Context, InstanceView, instances};

pub use footprint::Footprint;
pub use plan::{
    CheckSpec, DeltaElementKind, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts,
    TriggerTable, ValidationBudget, ValidationOptions, ValidationPlan, ValueFormat,
};
pub use report::{
    CheckClass, CheckCoverage, CheckInstance, CheckKey, CheckKind, Diagnostic, EndpointSide,
    FanRecord, IncompleteCheck, IncompleteReason, ReportDecodeError, ReportScope, Requirement,
    SchemaRef, SchemaSourceMap, SourceLocation, ValidationReport, Violation, aggregate_status,
};
pub use state::{
    AdmissionPolicy, BudgetCut, EditRejection, EditValidation, KeyIndexState, OpenVerdict,
    StateField, StateMismatch, Validated, ValidatedGraph, ValidatedShape, ValidatedTable,
    ValidatedTree, ValidationState,
};
pub use witness::{
    CheckRun, CheckedInstance, EditWitness, RunFinding, RunVerdict, WitnessMismatch,
    WitnessOptions, WitnessReplay,
};

/// Shared exact-read cursor for several ordered subset-validation slices.
///
/// Under a limit, `spent` counts the reads charged so far; without one no
/// cut can occur, nothing consumes a trace, and nothing is charged. This is crate-private because only segmented validation composes local
/// views. Slices must be submitted in canonical `CheckInstance` order.
#[derive(Clone, Debug)]
pub(crate) struct SubsetBudget {
    pub(crate) limit: Option<u64>,
    pub(crate) spent: u64,
    pub(crate) exhausted: bool,
    cuts: BTreeMap<CheckKey, Option<ViewRef>>,
}

impl SubsetBudget {
    pub(crate) const fn new(budget: &ValidationBudget) -> Self {
        Self {
            limit: budget.max_reads,
            spent: 0,
            exhausted: false,
            cuts: BTreeMap::new(),
        }
    }

    /// Whether an earlier instance exhausted this shared cursor.
    pub(crate) const fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// Charge `instance`, the next instance in canonical `CheckInstance`
    /// order, its exact footprint cardinality `reads`. Without a limit
    /// nothing is charged and every instance is paid for. Under a limit the
    /// instance is paid for iff the cursor is not exhausted and `spent +
    /// reads` does not exceed the limit; otherwise it is refused as by
    /// [`Self::refuse`] and `spent` is unchanged. Segment composition uses
    /// this for synthetic and representable table checks, including
    /// footprints augmented by boundary in-stub dependencies.
    pub(crate) fn charge(&mut self, instance: &CheckInstance, reads: u64) -> bool {
        if self.exhausted {
            self.refuse(instance);
            return false;
        }
        let Some(limit) = self.limit else {
            return true;
        };
        match self.spent.checked_add(reads) {
            Some(total) if total <= limit => {
                self.spent = total;
                true
            }
            _ => {
                self.refuse(instance);
                false
            }
        }
    }

    /// Refuse `instance`: the budget is exhausted for it and every later
    /// instance, and its check key records its subject as the cut point
    /// unless an earlier instance of the key was cut already.
    pub(crate) fn refuse(&mut self, instance: &CheckInstance) {
        self.exhausted = true;
        self.cuts
            .entry(instance.check.clone())
            .or_insert_with(|| instance.subject.clone());
    }

    /// The residual budget under a limit.
    const fn residual(&self) -> Option<u64> {
        match self.limit {
            // `spent` never exceeds the limit: `charge` adds only what fits.
            Some(limit) => Some(limit - self.spent),
            None => None,
        }
    }

    /// Materialize the accumulated per-key cuts after every slice ran.
    pub(crate) fn incomplete(&self, plan: &ValidationPlan) -> Vec<IncompleteCheck> {
        let Some(limit) = self.limit else {
            return Vec::new();
        };
        self.cuts
            .iter()
            .filter_map(|(key, from)| {
                plan.checks().get(key).map(|spec| IncompleteCheck {
                    check: key.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: None,
                    reason: IncompleteReason::Budget {
                        limit,
                        spent: self.spent,
                        from: from.clone(),
                    },
                })
            })
            .collect()
    }
}

impl ValidationPlan {
    /// Validate a graph against the plan (scope
    /// [`ReportScope::Instance`]).
    ///
    /// Fan nodes and fan edges are checked by `FanWellFormed` against the
    /// schema's hyper-edges and are exempt from `EdgeDeclared` and
    /// `EndpointCoherence`. Parallel edges and self-loops are legal and count
    /// toward out-degrees and keys like any other edge. Dangling endpoints
    /// cannot occur: construction and decoding of a [`GInstance`] reject
    /// them.
    #[must_use]
    pub fn validate_graph(
        &self,
        instance: &GInstance,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport {
        let view = InstanceView::Graph(instance);
        let admitted = all_instances(self, view, external);
        let mut cursor = SubsetBudget::new(budget);
        let report = self.validate_graph_subset_cursor(
            instance,
            &admitted,
            &mut cursor,
            external,
            ReportScope::Instance,
        );
        finish_subset_report(self, report, &cursor)
    }

    /// Validate a tree against the plan (scope [`ReportScope::Instance`]).
    ///
    /// Besides the kinds shared with graphs and tables, a tree is checked for
    /// its root (`TreeRoot`), the agreement of its parent map with its arcs
    /// (`TreeParentMap`), the reachability of every node from its root
    /// (`TreeReachable`) and the absence of reserved fan vocabulary
    /// (`ReservedVocabulary`).
    #[must_use]
    pub fn validate_tree(
        &self,
        instance: &WInstance,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport {
        let arcs = checks::TreeArcs::new(instance);
        let admitted = all_instances(self, InstanceView::Tree(instance, &arcs), external);
        let mut cursor = SubsetBudget::new(budget);
        let report = self.validate_tree_subset_cursor(
            instance,
            &admitted,
            &mut cursor,
            external,
            ReportScope::Instance,
        );
        finish_subset_report(self, report, &cursor)
    }

    /// Validate a table instance against the plan (scope
    /// [`ReportScope::Instance`]).
    ///
    /// A row of a table the schema does not declare is an `UnknownAnchor`
    /// with the row as subject; a foreign-key pair whose edge the schema does
    /// not declare is an `UndeclaredEdge` with the pair as subject.
    #[must_use]
    pub fn validate_table(
        &self,
        instance: &FInstance,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> ValidationReport {
        let view = InstanceView::Table(instance);
        let admitted = all_instances(self, view, external);
        let mut cursor = SubsetBudget::new(budget);
        let report = self.validate_table_subset_cursor(
            instance,
            &admitted,
            &mut cursor,
            external,
            ReportScope::Instance,
        );
        finish_subset_report(self, report, &cursor)
    }

    /// One graph slice of a multi-view subset run, sharing its exact budget.
    pub(crate) fn validate_graph_subset_cursor(
        &self,
        instance: &GInstance,
        admitted: &BTreeSet<CheckInstance>,
        cursor: &mut SubsetBudget,
        external: &ExternalVerdicts,
        scope: ReportScope,
    ) -> ValidationReport {
        run_subset_cursor(
            self,
            InstanceView::Graph(instance),
            admitted,
            cursor,
            external,
            scope,
        )
    }

    /// One native-tree slice of a multi-view subset run.
    pub(crate) fn validate_tree_subset_cursor(
        &self,
        instance: &WInstance,
        admitted: &BTreeSet<CheckInstance>,
        cursor: &mut SubsetBudget,
        external: &ExternalVerdicts,
        scope: ReportScope,
    ) -> ValidationReport {
        let arcs = checks::TreeArcs::new(instance);
        run_subset_cursor(
            self,
            InstanceView::Tree(instance, &arcs),
            admitted,
            cursor,
            external,
            scope,
        )
    }

    /// One table slice of a multi-view subset run, sharing its exact budget.
    pub(crate) fn validate_table_subset_cursor(
        &self,
        instance: &FInstance,
        admitted: &BTreeSet<CheckInstance>,
        cursor: &mut SubsetBudget,
        external: &ExternalVerdicts,
        scope: ReportScope,
    ) -> ValidationReport {
        run_subset_cursor(
            self,
            InstanceView::Table(instance),
            admitted,
            cursor,
            external,
            scope,
        )
    }

    /// Check a graph delta without a validated baseline (scope
    /// [`ReportScope::DeltaLocal`]).
    ///
    /// The report evaluates, in the post-state and under `external`, the
    /// static checks, the local and external check instances of the delta's
    /// footprint (those it creates and those whose pre- or post-state trace
    /// reads an element it touches), and every instance of every
    /// maintained-global check, all in ascending order under one `budget`.
    /// A maintained-global check that the budget cut is also reported
    /// [`IncompleteReason::NoValidatedBaseline`]: without a validated
    /// baseline, only its evaluation in full re-establishes it. A `Passed`
    /// report thus means that the delta introduces no local violation and
    /// that every global check holds of the post-state.
    ///
    /// # Errors
    ///
    /// Returns the delta's typed applicability error when it does not apply
    /// to `pre`; no report is produced for a delta outside the domain.
    pub fn check_graph_delta(
        &self,
        pre: &GInstance,
        delta: &GraphDelta,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> Result<ValidationReport, EditError> {
        delta_local(self, pre, delta, budget, external)
    }

    /// Check a table delta without a validated baseline, as
    /// [`Self::check_graph_delta`] checks a graph delta.
    ///
    /// # Errors
    ///
    /// Returns the delta's typed applicability error when it does not apply
    /// to `pre`.
    pub fn check_table_delta(
        &self,
        pre: &FInstance,
        delta: &TableDelta,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> Result<ValidationReport, EditError> {
        delta_local(self, pre, delta, budget, external)
    }

    /// Check a tree edit without a validated baseline, as
    /// [`Self::check_graph_delta`] checks a graph delta.
    ///
    /// # Errors
    ///
    /// Returns the edit's typed applicability error when it does not apply
    /// to `pre`.
    pub fn check_tree_edit(
        &self,
        pre: &WInstance,
        edit: &TreeEdit,
        budget: &ValidationBudget,
        external: &ExternalVerdicts,
    ) -> Result<ValidationReport, EditError> {
        delta_local(self, pre, edit, budget, external)
    }
}

/// The delta-local report of `delta` from `pre` (see
/// [`ValidationPlan::check_graph_delta`]).
fn delta_local<I: ValidatedShape>(
    plan: &ValidationPlan,
    pre: &I,
    delta: &I::Delta,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
) -> Result<ValidationReport, EditError> {
    let footprint::Transition {
        footprint, post, ..
    } = footprint::transition(plan, external, &BTreeSet::new(), pre, delta, external)?;
    // The local and external instances of the footprint, and every static
    // and maintained-global instance of the post-state.
    let mut admitted: BTreeSet<CheckInstance> = footprint
        .rerun
        .iter()
        .chain(&footprint.created)
        .filter(|instance| {
            plan.checks().get(&instance.check).is_some_and(|spec| {
                matches!(
                    spec.class,
                    CheckClass::GraphLocal | CheckClass::BoundedExternal
                )
            })
        })
        .cloned()
        .collect();
    let after = post.with_view(|view| all_instances(plan, view, external));
    admitted.extend(after.into_iter().filter(|instance| {
        plan.checks().get(&instance.check).is_some_and(|spec| {
            matches!(
                spec.class,
                CheckClass::Static | CheckClass::MaintainedGlobal
            )
        })
    }));
    Ok(post.with_view(|view| {
        let mut cursor = SubsetBudget::new(budget);
        let report = run_subset_cursor(
            plan,
            view,
            &admitted,
            &mut cursor,
            external,
            ReportScope::DeltaLocal,
        );
        let mut incomplete = report.incomplete;
        incomplete.extend(cursor.incomplete(plan));
        for (key, spec) in plan.checks() {
            if spec.applies_to(view.shape())
                && spec.class == CheckClass::MaintainedGlobal
                && cursor.cuts.contains_key(key)
            {
                incomplete.push(IncompleteCheck {
                    check: key.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: None,
                    reason: IncompleteReason::NoValidatedBaseline,
                });
            }
        }
        ValidationReport::new(
            report.shape,
            ReportScope::DeltaLocal,
            report.plan,
            report.diagnostics,
            incomplete,
            report.coverage,
        )
    }))
}

/// Validate a graph against `schema` under `protocol`, with an unlimited
/// budget and no external verdicts.
#[must_use]
pub fn validate_graph(
    instance: &GInstance,
    schema: &Schema,
    protocol: &Protocol,
) -> ValidationReport {
    ValidationPlan::compile(schema, protocol, &ValidationOptions::default()).validate_graph(
        instance,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )
}

/// Validate a tree against `schema` under `protocol`, with an unlimited
/// budget and no external verdicts.
#[must_use]
pub fn validate_wtype(
    instance: &WInstance,
    schema: &Schema,
    protocol: &Protocol,
) -> ValidationReport {
    ValidationPlan::compile(schema, protocol, &ValidationOptions::default()).validate_tree(
        instance,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )
}

/// Validate a table instance against `schema` under `protocol`, with an
/// unlimited budget and no external verdicts.
#[must_use]
pub fn validate_table(
    instance: &FInstance,
    schema: &Schema,
    protocol: &Protocol,
) -> ValidationReport {
    ValidationPlan::compile(schema, protocol, &ValidationOptions::default()).validate_table(
        instance,
        &ValidationBudget::default(),
        &ExternalVerdicts::default(),
    )
}

/// The state of one validation run: the entries found so far, and the
/// budget cursor it shares with the other slices of its run.
struct Run<'a, 'c> {
    ctx: Context<'a>,
    cursor: &'c mut SubsetBudget,
    diagnostics: Vec<Diagnostic>,
    incomplete: Vec<IncompleteCheck>,
    coverage: BTreeMap<CheckKey, CheckCoverage>,
}

impl Run<'_, '_> {
    /// Run the selected instances of one compiled check, in ascending
    /// subject order.
    ///
    /// Under a limit each instance's trace is discovered within the
    /// residual budget and charged before the instance is evaluated; an
    /// instance whose trace does not fit, and every later instance, is
    /// incomplete and never evaluated. Without a limit no trace is
    /// discovered, since nothing consumes it.
    fn check_selected(&mut self, key: &CheckKey, spec: &CheckSpec, subjects: &[Option<ViewRef>]) {
        let plan = self.ctx.plan;
        let mut cov = CheckCoverage {
            class: spec.class,
            requirement: spec.requirement,
            instances: subjects.len() as u64,
            passed: 0,
            failed: 0,
            incomplete: 0,
        };
        if plan.is_statically_incomplete(key) {
            cov.incomplete = cov.instances;
            self.coverage.insert(key.clone(), cov);
            return;
        }
        let source = plan.source(&key.decl);
        for subject in subjects {
            let instance = CheckInstance {
                check: key.clone(),
                subject: subject.clone(),
            };
            let reads = match self.cursor.residual() {
                _ if self.cursor.is_exhausted() => {
                    self.cursor.refuse(&instance);
                    cov.incomplete += 1;
                    continue;
                }
                None => None,
                // At zero residual, bounded discovery must still run: a
                // check instance can have an exact empty trace.
                Some(residual) => {
                    let bounded = checks::dependency_trace_bounded(
                        &self.ctx,
                        key,
                        subject.as_ref(),
                        usize::try_from(residual).unwrap_or(usize::MAX),
                    );
                    let Some(reads) = bounded else {
                        // Discovery stopped at the first unpaid distinct
                        // read: the instance is the cut, never evaluated.
                        self.cursor.refuse(&instance);
                        cov.incomplete += 1;
                        continue;
                    };
                    Some(reads)
                }
            };
            // A trace discovered within the residual budget fits it, and
            // nothing is charged without a limit, so the charge succeeds.
            let charged = self
                .cursor
                .charge(&instance, reads.as_ref().map_or(0, |r| r.len() as u64));
            debug_assert!(charged, "a trace discovered within the residual fits it");
            let outcome = match reads {
                Some(reads) => {
                    checks::evaluate_with_trace(&self.ctx, key, subject.as_ref(), &reads)
                }
                None => checks::evaluate_untraced(&self.ctx, key, subject.as_ref()),
            };
            if outcome.findings.iter().any(|f| f.violation.refutes()) {
                cov.failed += 1;
            } else if outcome.incomplete.is_empty() {
                cov.passed += 1;
            } else {
                cov.incomplete += 1;
            }
            for finding in outcome.findings {
                self.diagnostics.push(checks::diagnostic_of(
                    key,
                    spec,
                    subject.as_ref(),
                    finding,
                    source.clone(),
                ));
            }
            for reason in outcome.incomplete {
                self.incomplete.push(IncompleteCheck {
                    check: key.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: subject.clone(),
                    reason,
                });
            }
        }
        self.coverage.insert(key.clone(), cov);
    }
}

fn all_instances(
    plan: &ValidationPlan,
    view: InstanceView<'_>,
    external: &ExternalVerdicts,
) -> BTreeSet<CheckInstance> {
    let ctx = Context::new(plan, view, external);
    plan.checks()
        .iter()
        .filter(|(_, spec)| spec.applies_to(view.shape()))
        .flat_map(|(key, _)| {
            instances(&ctx, key)
                .into_iter()
                .map(|subject| CheckInstance {
                    check: key.clone(),
                    subject,
                })
        })
        .collect()
}

fn finish_subset_report(
    plan: &ValidationPlan,
    report: ValidationReport,
    cursor: &SubsetBudget,
) -> ValidationReport {
    let mut incomplete = report.incomplete;
    incomplete.extend(cursor.incomplete(plan));
    ValidationReport::new(
        report.shape,
        report.scope,
        report.plan,
        report.diagnostics,
        incomplete,
        report.coverage,
    )
}

/// One slice of an ordered subset run: the report of the `admitted`
/// instances of `view`, without its budget cuts, which accumulate in
/// `cursor` for the caller to materialize once every slice ran.
fn run_subset_cursor(
    plan: &ValidationPlan,
    view: InstanceView<'_>,
    admitted: &BTreeSet<CheckInstance>,
    cursor: &mut SubsetBudget,
    external: &ExternalVerdicts,
    scope: ReportScope,
) -> ValidationReport {
    let shape = view.shape();
    let mut run = Run {
        ctx: Context::new(plan, view, external),
        cursor,
        diagnostics: Vec::new(),
        incomplete: Vec::new(),
        coverage: BTreeMap::new(),
    };
    let mut selected: BTreeMap<CheckKey, Vec<Option<ViewRef>>> = BTreeMap::new();
    for instance in admitted {
        if plan
            .checks()
            .get(&instance.check)
            .is_some_and(|spec| spec.applies_to(shape))
        {
            selected
                .entry(instance.check.clone())
                .or_default()
                .push(instance.subject.clone());
        }
    }
    for (key, spec) in plan.checks() {
        if !spec.applies_to(shape) {
            continue;
        }
        if let Some(subjects) = selected.get(key) {
            run.check_selected(key, spec, subjects);
        } else {
            run.coverage.insert(
                key.clone(),
                CheckCoverage {
                    class: spec.class,
                    requirement: spec.requirement,
                    instances: 0,
                    passed: 0,
                    failed: 0,
                    incomplete: 0,
                },
            );
        }
    }
    let mut incomplete = run.incomplete;
    incomplete.extend(plan.static_findings().iter().cloned());
    ValidationReport::new(
        shape,
        scope,
        *plan.digest(),
        run.diagnostics,
        incomplete,
        run.coverage,
    )
}

#[cfg(test)]
mod subset_tests {
    use std::collections::{BTreeSet, HashMap};

    use panproto_schema::{
        Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, Vertex,
    };

    use super::{
        CheckInstance, CheckKey, CheckKind, ExternalVerdicts, ReportScope, SchemaRef, SubsetBudget,
        ValidationBudget, ValidationOptions, ValidationPlan,
    };
    use crate::functor::FInstance;
    use crate::ginstance::GInstance;
    use crate::metadata::{Node, NodeShape};
    use crate::segment::SegmentId;
    use crate::value::Value;
    use crate::view_ref::{RowId, ViewRef};

    fn fixture() -> (ValidationPlan, GInstance, CheckInstance) {
        let mut schema = Schema::default();
        schema.vertices.insert(
            "item".into(),
            Vertex {
                id: "item".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        let plan = ValidationPlan::compile(
            &schema,
            &Protocol {
                syntactic_shape: InstanceShapeKind::Graph,
                semantic_shape: InstanceShapeKind::Graph,
                ..Protocol::default()
            },
            &ValidationOptions::default(),
        );
        let key = CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Schema);
        let graph = GInstance::new()
            .with_node(Node::new(0, "item"))
            .with_node(Node::new(1, "ghost"));
        let instance = CheckInstance {
            check: key,
            subject: Some(ViewRef::Node(1)),
        };
        (plan, graph, instance)
    }

    #[test]
    fn subset_validation_restricts_subjects_and_charges_the_shared_trace() {
        let (plan, graph, selected) = fixture();
        let admitted = BTreeSet::from([selected.clone()]);
        let mut full_cursor = SubsetBudget::new(&ValidationBudget::default());
        let full = plan.validate_graph_subset_cursor(
            &graph,
            &admitted,
            &mut full_cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        let full = super::finish_subset_report(&plan, full, &full_cursor);
        assert_eq!(full.diagnostics.len(), 1);
        assert_eq!(full.diagnostics[0].subject, selected.subject);
        assert_eq!(full.coverage[&selected.check].instances, 1);

        let mut cut_cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(0) });
        let cut = plan.validate_graph_subset_cursor(
            &graph,
            &admitted,
            &mut cut_cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        let cut = super::finish_subset_report(&plan, cut, &cut_cursor);
        assert!(cut.diagnostics.is_empty());
        assert_eq!(cut.coverage[&selected.check].incomplete, 1);
        assert!(cut.incomplete.iter().any(|entry| {
            entry.check == selected.check
                && matches!(
                    entry.reason,
                    super::IncompleteReason::Budget { spent: 0, .. }
                )
        }));
    }

    #[test]
    fn zero_budget_never_semantically_evaluates_a_selected_fan() {
        let (plan, _, _) = fixture();
        let graph =
            GInstance::new().with_node(Node::new(0, "fan").with_shape(NodeShape::HyperEdge));
        let key = plan
            .checks()
            .keys()
            .find(|key| key.kind == CheckKind::FanWellFormed)
            .unwrap_or_else(|| panic!("the structural plan has a fan check"))
            .clone();
        let selected = CheckInstance {
            check: key,
            subject: Some(ViewRef::Node(0)),
        };
        assert!(!plan.is_statically_incomplete(&selected.check));
        super::checks::reset_semantic_evaluations();
        let mut cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(0) });
        let report = plan.validate_graph_subset_cursor(
            &graph,
            &BTreeSet::from([selected.clone()]),
            &mut cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        assert_eq!(super::checks::semantic_evaluations(), 0);
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.coverage[&selected.check].incomplete, 1);
        assert_eq!(cursor.spent, 0);
        assert!(cursor.is_exhausted());
    }

    #[test]
    fn residual_budget_stops_dependency_discovery_before_the_unpaid_suffix() {
        let mut schema = Schema::default();
        for id in ["source", "target"] {
            schema.vertices.insert(
                id.into(),
                Vertex {
                    id: id.into(),
                    kind: "object".into(),
                    nsid: None,
                },
            );
        }
        let edge = Edge {
            src: "source".into(),
            tgt: "target".into(),
            kind: "link".into(),
            name: None,
        };
        schema.edges.insert(edge.clone(), edge.kind.clone());
        schema.required.insert("source".into(), vec![edge.clone()]);
        let plan = ValidationPlan::compile(
            &schema,
            &Protocol {
                syntactic_shape: InstanceShapeKind::Graph,
                semantic_shape: InstanceShapeKind::Graph,
                ..Protocol::default()
            },
            &ValidationOptions::default(),
        );
        let mut graph = GInstance::new().with_node(Node::new(0, "source"));
        for id in 1..=10 {
            graph = graph
                .with_node(Node::new(id, "target"))
                .with_edge(0, id, edge.clone());
        }
        let key = plan
            .checks()
            .keys()
            .find(|key| key.kind == CheckKind::RequiredEdge)
            .unwrap_or_else(|| panic!("the schema compiles a required-edge check"))
            .clone();
        let selected = CheckInstance {
            check: key,
            subject: Some(ViewRef::Node(0)),
        };
        super::checks::reset_semantic_evaluations();
        let mut cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(1) });
        let report = plan.validate_graph_subset_cursor(
            &graph,
            &BTreeSet::from([selected.clone()]),
            &mut cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        assert_eq!(super::checks::semantic_evaluations(), 0);
        assert_eq!(report.coverage[&selected.check].incomplete, 1);
        assert_eq!(cursor.spent, 0, "a rejected instance charges atomically");
        assert!(cursor.exhausted);
    }

    #[test]
    fn selected_attribute_reads_only_its_subject() {
        let mut schema = Schema::default();
        for (id, kind) in [("item", "object"), ("name", "string")] {
            schema.vertices.insert(
                id.into(),
                Vertex {
                    id: id.into(),
                    kind: kind.into(),
                    nsid: None,
                },
            );
        }
        let name = Edge {
            src: "item".into(),
            tgt: "name".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        };
        schema.edges.insert(name.clone(), name.kind.clone());
        schema
            .outgoing
            .entry(name.src.clone())
            .or_default()
            .push(name);
        let plan = ValidationPlan::compile(
            &schema,
            &Protocol {
                syntactic_shape: InstanceShapeKind::Graph,
                semantic_shape: InstanceShapeKind::Graph,
                ..Protocol::default()
            },
            &ValidationOptions::default(),
        );
        let graph = GInstance::new()
            .with_node(Node::new(0, "item").with_extra_field("name", Value::Str("a".into())))
            .with_node(Node::new(1, "item").with_extra_field("foreign", Value::Bool(true)));
        let key = plan
            .checks()
            .keys()
            .find(|key| key.kind == CheckKind::Attribute)
            .unwrap_or_else(|| panic!("the schema compiles an attribute check"))
            .clone();
        let selected = CheckInstance {
            check: key,
            subject: Some(ViewRef::Node(0)),
        };
        let mut cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(1) });
        let report = plan.validate_graph_subset_cursor(
            &graph,
            &BTreeSet::from([selected.clone()]),
            &mut cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.coverage[&selected.check].passed, 1);
        assert_eq!(cursor.spent, 1);
    }

    #[test]
    fn selected_table_position_requirement_has_the_same_zero_read_trace() {
        let mut schema = Schema::default();
        for id in ["group", "item"] {
            schema.vertices.insert(
                id.into(),
                Vertex {
                    id: id.into(),
                    kind: "object".into(),
                    nsid: None,
                },
            );
        }
        let within = Edge {
            src: "group".into(),
            tgt: "item".into(),
            kind: "ordered".into(),
            name: Some("items".into()),
        };
        schema.edges.insert(within.clone(), within.kind.clone());
        schema
            .outgoing
            .entry(within.src.clone())
            .or_default()
            .push(within.clone());
        schema
            .incoming
            .entry(within.tgt.clone())
            .or_default()
            .push(within.clone());
        schema.orderings.insert(within.clone(), 0);
        schema.declarations.insert_key(
            "item".into(),
            Key {
                id: "position".into(),
                parts: vec![KeyPart::Position { within }],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
            },
        );
        let plan = ValidationPlan::compile(
            &schema,
            &Protocol {
                syntactic_shape: InstanceShapeKind::Table,
                semantic_shape: InstanceShapeKind::Graph,
                ..Protocol::default()
            },
            &ValidationOptions::default(),
        );
        let table = FInstance::new()
            .with_rows("item", [(RowId(0), HashMap::new())])
            .unwrap_or_else(|error| panic!("one table row is valid: {error}"));
        let key = plan
            .checks()
            .keys()
            .find(|key| {
                key.kind == CheckKind::RequiredValue && matches!(key.decl, SchemaRef::Key { .. })
            })
            .unwrap_or_else(|| panic!("a primary position key compiles a required-value check"))
            .clone();
        assert!(!plan.is_statically_incomplete(&key));
        let selected = CheckInstance {
            check: key,
            subject: Some(ViewRef::Row(RowId(0))),
        };
        let external = ExternalVerdicts::default();
        let ctx = super::checks::Context::new(
            &plan,
            super::checks::InstanceView::Table(&table),
            &external,
        );
        let evaluation =
            super::checks::evaluate_traced(&ctx, &selected.check, selected.subject.as_ref());
        assert!(evaluation.reads.is_empty());
        assert!(matches!(
            evaluation.outcome.incomplete.as_slice(),
            [super::IncompleteReason::Unsupported { .. }]
        ));
        super::checks::reset_semantic_evaluations();
        let mut cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(0) });
        let report = plan.validate_table_subset_cursor(
            &table,
            &BTreeSet::from([selected.clone()]),
            &mut cursor,
            &external,
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        let report = super::finish_subset_report(&plan, report, &cursor);
        assert_eq!(super::checks::semantic_evaluations(), 1);
        assert_eq!(cursor.spent, 0);
        assert!(!cursor.exhausted);
        assert_eq!(report.coverage[&selected.check].incomplete, 1);
    }

    #[test]
    fn subset_validation_relies_on_the_selected_subject_schedule() {
        let (plan, graph, selected) = fixture();
        let foreign_key = CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Vertex("item".into()));
        let admitted = BTreeSet::from([
            CheckInstance {
                check: selected.check.clone(),
                subject: Some(ViewRef::Node(u32::MAX)),
            },
            CheckInstance {
                check: foreign_key,
                subject: selected.subject,
            },
        ]);
        let mut cursor = SubsetBudget::new(&ValidationBudget::default());
        let report = plan.validate_graph_subset_cursor(
            &graph,
            &admitted,
            &mut cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );

        assert!(report.diagnostics.is_empty());
        assert_eq!(report.coverage[&selected.check].instances, 1);
        assert_eq!(report.coverage[&selected.check].passed, 1);
        // Without a limit nothing is charged: no cut can occur.
        assert_eq!(cursor.spent, 0);
    }

    #[test]
    fn subset_cursor_preserves_one_budget_across_ordered_view_slices() {
        let (plan, graph, second) = fixture();
        let first = CheckInstance {
            check: second.check.clone(),
            subject: Some(ViewRef::Node(0)),
        };
        let mut cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(1) });
        let first_report = plan.validate_graph_subset_cursor(
            &graph,
            &BTreeSet::from([first.clone()]),
            &mut cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        assert_eq!(first_report.coverage[&first.check].passed, 1);
        assert_eq!(cursor.spent, 1);
        assert!(!cursor.exhausted);

        let second_report = plan.validate_graph_subset_cursor(
            &graph,
            &BTreeSet::from([second.clone()]),
            &mut cursor,
            &ExternalVerdicts::default(),
            ReportScope::Segment {
                segment: SegmentId(0),
            },
        );
        assert_eq!(second_report.coverage[&second.check].incomplete, 1);
        assert!(second_report.incomplete.is_empty());
        assert_eq!(cursor.spent, 1);
        assert!(cursor.exhausted);
        let cuts = cursor.incomplete(&plan);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].check, second.check);
        assert!(matches!(
            cuts[0].reason,
            super::IncompleteReason::Budget {
                spent: 1,
                from: Some(ViewRef::Node(1)),
                ..
            }
        ));
    }

    #[test]
    fn subset_cursor_charges_unrepresentable_instances_in_the_same_budget() {
        let (plan, _graph, selected) = fixture();
        let mut cursor = SubsetBudget::new(&ValidationBudget { max_reads: Some(2) });
        assert!(cursor.charge(&selected, 2));
        assert!(!cursor.charge(&selected, 1));
        assert_eq!(cursor.spent, 2);
        assert!(cursor.exhausted);
        let cuts = cursor.incomplete(&plan);
        assert_eq!(cuts.len(), 1);
        assert!(matches!(
            cuts[0].reason,
            super::IncompleteReason::Budget { spent: 2, .. }
        ));
    }
}
