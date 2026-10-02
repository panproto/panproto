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
//! (also `datetime`), `date`, `time`, `uri`, `uuid`, `ipv4`, `ipv6` and
//! `did`. Any other format is an unsupported declaration.
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
mod footprint;
mod plan;
mod report;
mod state;
mod witness;

use std::collections::BTreeMap;

use panproto_schema::{Protocol, Schema};

use crate::functor::FInstance;
use crate::ginstance::GInstance;
use crate::view_ref::ViewRef;
use crate::wtype::WInstance;

use checks::{Context, InstanceView, evaluate, instances};

pub use plan::{
    CheckSpec, DeltaElementKind, ExternalCheckDecl, ExternalVerdict, ExternalVerdicts,
    TriggerTable, ValidationBudget, ValidationOptions, ValidationPlan, ValueFormat,
};
pub use report::{
    CheckClass, CheckCoverage, CheckInstance, CheckKey, CheckKind, Diagnostic, EndpointSide,
    FanRecord, IncompleteCheck, IncompleteReason, ReportDecodeError, ReportScope, Requirement,
    SchemaRef, SchemaSourceMap, SourceLocation, ValidationReport, Violation, aggregate_status,
};
pub use witness::CheckedInstance;

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
        run(self, InstanceView::Graph(instance), budget, external)
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
        run(self, InstanceView::Tree(instance, &arcs), budget, external)
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
        run(self, InstanceView::Table(instance), budget, external)
    }
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

/// The state of one validation run: the entries found so far and the
/// budget spent.
struct Run<'a> {
    ctx: Context<'a>,
    budget: ValidationBudget,
    diagnostics: Vec<Diagnostic>,
    incomplete: Vec<IncompleteCheck>,
    coverage: BTreeMap<CheckKey, CheckCoverage>,
    /// For each key the budget cut, the subject of its first instance left
    /// unevaluated.
    cuts: BTreeMap<CheckKey, Option<ViewRef>>,
    spent: u64,
    exhausted: bool,
}

impl Run<'_> {
    /// Charge one instance its footprint; `false` when the budget cannot
    /// pay for it, which exhausts the budget for every later instance.
    fn charge(&mut self, instance: &CheckInstance) -> bool {
        if self.exhausted {
            return false;
        }
        let cost = footprint::reads(&self.ctx, instance).len() as u64;
        if self
            .budget
            .max_reads
            .is_some_and(|limit| self.spent.saturating_add(cost) > limit)
        {
            self.exhausted = true;
            return false;
        }
        self.spent += cost;
        true
    }

    /// Run every instance of one compiled check.
    fn check(&mut self, key: &CheckKey, spec: &CheckSpec) {
        let plan = self.ctx.plan;
        let subjects = instances(&self.ctx, key);
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
        let source = plan.options.sources.0.get(&key.decl).cloned();
        for subject in subjects {
            let instance = CheckInstance {
                check: key.clone(),
                subject,
            };
            if !self.charge(&instance) {
                cov.incomplete += 1;
                self.cuts.entry(key.clone()).or_insert(instance.subject);
                continue;
            }
            let outcome = evaluate(&self.ctx, key, instance.subject.as_ref());
            if !outcome.findings.is_empty() {
                cov.failed += 1;
            } else if outcome.incomplete.is_empty() {
                cov.passed += 1;
            } else {
                cov.incomplete += 1;
            }
            for finding in outcome.findings {
                self.diagnostics.push(Diagnostic {
                    check: key.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: instance.subject.clone(),
                    elements: finding.elements,
                    violation: finding.violation,
                    source: source.clone(),
                });
            }
            for reason in outcome.incomplete {
                self.incomplete.push(IncompleteCheck {
                    check: key.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: instance.subject.clone(),
                    reason,
                });
            }
        }
        self.coverage.insert(key.clone(), cov);
    }

    /// The report: one budget entry per key the budget cut, then the plan's
    /// static findings.
    fn finish(mut self) -> ValidationReport {
        let plan = self.ctx.plan;
        let limit = self.budget.max_reads.unwrap_or(u64::MAX);
        for (key, from) in std::mem::take(&mut self.cuts) {
            let spec = &plan.checks()[&key];
            self.incomplete.push(IncompleteCheck {
                class: spec.class,
                requirement: spec.requirement,
                check: key,
                subject: None,
                reason: IncompleteReason::Budget {
                    limit,
                    spent: self.spent,
                    from,
                },
            });
        }
        self.incomplete
            .extend(plan.static_findings().iter().cloned());
        ValidationReport::new(
            self.ctx.view.shape(),
            ReportScope::Instance,
            *plan.digest(),
            self.diagnostics,
            self.incomplete,
            self.coverage,
        )
    }
}

/// Run every compiled check that applies to the view's shape, in ascending
/// [`CheckInstance`] order, charging each instance its footprint against the
/// budget.
fn run(
    plan: &ValidationPlan,
    view: InstanceView<'_>,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
) -> ValidationReport {
    let shape = view.shape();
    let mut state = Run {
        ctx: Context::new(plan, view, external),
        budget: *budget,
        diagnostics: Vec::new(),
        incomplete: Vec::new(),
        coverage: BTreeMap::new(),
        cuts: BTreeMap::new(),
        spent: 0,
        exhausted: false,
    };
    for (key, spec) in plan.checks() {
        if spec.applies_to(shape) {
            state.check(key, spec);
        }
    }
    state.finish()
}
