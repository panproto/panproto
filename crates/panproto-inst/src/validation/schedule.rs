//! The budget scheduler shared by whole-instance initialization, incremental
//! transitions, checked reattachment and witness replay.
//!
//! A [`Session`] reads one instance view under one external projection
//! through a single [`Context`], so a key index is built once per view
//! however many check instances consult it. [`simulate`] walks an ordered
//! schedule in ascending `CheckInstance` order and charges each instance its
//! dependency trace before evaluating it. Once the budget cannot pay for an
//! instance, that instance and every later one are skipped: none of them is
//! evaluated, and the trace of the first is discovered only up to the
//! residual budget. A statically incomplete instance is neither charged nor
//! skipped. This is the order and the charge of whole-instance validation,
//! so the committed `spent` count and skipped suffix are the ones the full
//! validator records for the same schedule.

use std::collections::{BTreeMap, BTreeSet};

use crate::view_ref::ViewRef;

use super::checks::{
    Context, InstanceView, dependency_trace, dependency_trace_bounded, diagnostic_of,
    evaluate_untraced, evaluate_with_trace, instances, is_instance_subject,
};
use super::plan::{Compiled, ExternalVerdicts, ValidationBudget, ValidationPlan};
use super::report::{
    CheckCoverage, CheckInstance, CheckKey, IncompleteCheck, sort_diagnostics, sort_incomplete,
};
use super::state::{KeyIndexState, OpenVerdict};

/// One instance view, read under one external projection.
pub struct Session<'a> {
    ctx: Context<'a>,
}

impl<'a> Session<'a> {
    /// A session over `view` under `external`.
    pub fn new(
        plan: &'a ValidationPlan,
        view: InstanceView<'a>,
        external: &'a ExternalVerdicts,
    ) -> Self {
        Self {
            ctx: Context::new(plan, view, external),
        }
    }

    const fn plan(&self) -> &'a ValidationPlan {
        self.ctx.plan
    }

    /// Every check instance the plan compiles for the view.
    pub fn instances(&self) -> BTreeSet<CheckInstance> {
        let shape = self.ctx.view.shape();
        self.plan()
            .checks()
            .iter()
            .filter(|(_, spec)| spec.applies_to(shape))
            .flat_map(|(key, _)| {
                instances(&self.ctx, key)
                    .into_iter()
                    .map(|subject| CheckInstance {
                        check: key.clone(),
                        subject,
                    })
            })
            .collect()
    }

    /// Every instance of the compiled check `key` in the view, ascending.
    pub fn instances_of(&self, key: &CheckKey) -> Vec<CheckInstance> {
        if !self
            .plan()
            .checks()
            .get(key)
            .is_some_and(|spec| spec.applies_to(self.ctx.view.shape()))
        {
            return Vec::new();
        }
        instances(&self.ctx, key)
            .into_iter()
            .map(|subject| CheckInstance {
                check: key.clone(),
                subject,
            })
            .collect()
    }

    /// Whether `instance` is a check instance of the view: an applicable
    /// compiled check, and its subject an instance subject of it (a
    /// whole-instance check has the subject `None`).
    pub fn holds(&self, instance: &CheckInstance) -> bool {
        let Some(spec) = self.plan().checks().get(&instance.check) else {
            return false;
        };
        if !spec.applies_to(self.ctx.view.shape()) {
            return false;
        }
        instance.subject.as_ref().map_or_else(
            || is_whole_instance(instance.check.kind),
            |subject| is_instance_subject(&self.ctx, &instance.check, subject),
        )
    }

    /// The check instances whose subject is one of `candidates`.
    pub fn instances_at(&self, candidates: &BTreeSet<ViewRef>) -> BTreeSet<CheckInstance> {
        let shape = self.ctx.view.shape();
        self.plan()
            .checks()
            .iter()
            .filter(|(_, spec)| spec.applies_to(shape))
            .flat_map(|(key, _)| {
                candidates
                    .iter()
                    .filter(|subject| is_instance_subject(&self.ctx, key, subject))
                    .map(|subject| CheckInstance {
                        check: key.clone(),
                        subject: Some(subject.clone()),
                    })
            })
            .collect()
    }

    /// Whether the instance is decided at compile time or not compiled at
    /// all, so that it reads nothing and is never evaluated.
    pub fn inert(&self, instance: &CheckInstance) -> bool {
        self.plan().is_statically_incomplete(&instance.check)
            || !self.plan().checks().contains_key(&instance.check)
    }

    /// The exact dependency trace of one instance, discovered without
    /// evaluating it.
    pub fn trace(&self, instance: &CheckInstance) -> BTreeSet<ViewRef> {
        if self.inert(instance) {
            return BTreeSet::new();
        }
        dependency_trace(&self.ctx, &instance.check, instance.subject.as_ref())
    }

    /// The dependency trace of one instance when it has at most `limit`
    /// distinct reads; discovery stops at the first read beyond `limit`.
    pub fn trace_within(&self, instance: &CheckInstance, limit: u64) -> Option<BTreeSet<ViewRef>> {
        if self.inert(instance) {
            return Some(BTreeSet::new());
        }
        dependency_trace_bounded(
            &self.ctx,
            &instance.check,
            instance.subject.as_ref(),
            usize::try_from(limit).unwrap_or(usize::MAX),
        )
    }

    /// Evaluate one instance and return its complete, canonically sorted
    /// outcome bundle. With `reads`, the evaluation is checked (in debug
    /// builds) to read exactly that already discovered trace.
    pub fn evaluate(
        &self,
        instance: &CheckInstance,
        reads: Option<BTreeSet<ViewRef>>,
    ) -> OpenVerdict {
        let plan = self.plan();
        let Some(spec) = plan.checks().get(&instance.check) else {
            return OpenVerdict::default();
        };
        if plan.is_statically_incomplete(&instance.check) {
            return OpenVerdict::default();
        }
        let subject = instance.subject.as_ref();
        let outcome = reads.map_or_else(
            || evaluate_untraced(&self.ctx, &instance.check, subject),
            |reads| evaluate_with_trace(&self.ctx, &instance.check, subject, &reads),
        );
        let source = plan.source(&instance.check.decl);
        let mut verdict = OpenVerdict {
            diagnostics: outcome
                .findings
                .into_iter()
                .map(|finding| {
                    diagnostic_of(&instance.check, spec, subject, finding, source.clone())
                })
                .collect(),
            incomplete: outcome
                .incomplete
                .into_iter()
                .map(|reason| IncompleteCheck {
                    check: instance.check.clone(),
                    class: spec.class,
                    requirement: spec.requirement,
                    subject: instance.subject.clone(),
                    reason,
                })
                .collect(),
        };
        sort_diagnostics(&mut verdict.diagnostics);
        sort_incomplete(&mut verdict.incomplete);
        verdict
    }

    /// The maintained key and reference indexes of the view.
    pub fn indexes(
        &self,
    ) -> (
        BTreeMap<CheckKey, KeyIndexState>,
        BTreeMap<CheckKey, KeyIndexState>,
    ) {
        let plan = self.plan();
        let shape = self.ctx.view.shape();
        let mut keys = BTreeMap::new();
        let mut references = BTreeMap::new();
        for (check, compiled) in &plan.compiled {
            if !plan
                .checks()
                .get(check)
                .is_some_and(|spec| spec.applies_to(shape))
            {
                continue;
            }
            match compiled {
                Compiled::KeyUnique { vertex, key } => {
                    if let Some(index) = self.ctx.key_index(vertex, key, true) {
                        keys.insert(check.clone(), KeyIndexState::from(&*index));
                    }
                }
                Compiled::Reference { reference, target } => {
                    if let Some(index) = self.ctx.key_index(&reference.target_vertex, target, true)
                    {
                        references.insert(check.clone(), KeyIndexState::from(&*index));
                    }
                }
                _ => {}
            }
        }
        (keys, references)
    }

    /// Exactly the entries of `external` addressable by the external check
    /// instances among `instances`.
    pub fn active_external(
        &self,
        instances: &BTreeSet<CheckInstance>,
        external: &ExternalVerdicts,
    ) -> ExternalVerdicts {
        let mut active = ExternalVerdicts::default();
        for check in instances {
            let Some(Compiled::External { name }) = self.plan().compiled.get(&check.check) else {
                continue;
            };
            let Some(subject) = &check.subject else {
                continue;
            };
            if let Some(verdict) = external.get(name, subject) {
                active.insert(name.clone(), subject.clone(), *verdict);
            }
        }
        active
    }
}

/// Whether a budget walk evaluates the instances it pays for, or only
/// replays the schedule to find its cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Walk {
    /// Evaluate every paid instance.
    Evaluate,
    /// Charge every paid instance its trace without evaluating it.
    Replay,
}

/// What a budget walk over one schedule did.
#[derive(Debug, Default)]
pub struct Simulation {
    /// The complete outcome of every evaluated instance.
    pub evaluated: BTreeMap<CheckInstance, OpenVerdict>,
    /// The budget-skipped suffix, ascending.
    pub skipped: Vec<CheckInstance>,
    /// Reads charged before the cut (zero without a limit).
    pub spent: u64,
}

/// Walk `scheduled` (ascending, without repeats) under `budget`.
///
/// `known` supplies the full trace of instances whose trace a caller has
/// already discovered (an incremental transition discovers every scheduled
/// trace for its footprint); any other instance is discovered with a read
/// limit equal to the residual budget, or, without a limit, not discovered
/// at all, since nothing consumes its trace. Under [`Walk::Replay`] nothing
/// is evaluated.
pub fn simulate(
    session: &Session<'_>,
    scheduled: &[CheckInstance],
    known: &BTreeMap<CheckInstance, BTreeSet<ViewRef>>,
    budget: &ValidationBudget,
    walk: Walk,
) -> Simulation {
    let mut out = Simulation::default();
    let mut exhausted = false;
    for instance in scheduled {
        if session.inert(instance) {
            continue;
        }
        if exhausted {
            out.skipped.push(instance.clone());
            continue;
        }
        let reads = match budget.max_reads {
            None => known.get(instance).cloned(),
            Some(limit) => {
                let residual = limit - out.spent;
                let paid = known.get(instance).map_or_else(
                    || session.trace_within(instance, residual),
                    |reads| (reads.len() as u64 <= residual).then(|| reads.clone()),
                );
                let Some(reads) = paid else {
                    exhausted = true;
                    out.skipped.push(instance.clone());
                    continue;
                };
                out.spent += reads.len() as u64;
                Some(reads)
            }
        };
        if walk == Walk::Evaluate {
            let verdict = session.evaluate(instance, reads);
            out.evaluated.insert(instance.clone(), verdict);
        }
    }
    out
}

/// How one check instance counts in its check's coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counted {
    /// Evaluated with no refuting diagnostic and no incomplete result.
    Passed,
    /// Evaluated with a refuting diagnostic.
    Failed,
    /// Budget-skipped, statically incomplete, or evaluated with an
    /// incomplete result and no refuting diagnostic.
    Incomplete,
}

/// How `instance` counts, given the open bundles and the skipped instances
/// of its state: an advisory observation that refutes nothing counts as a
/// pass.
pub fn classify(
    plan: &ValidationPlan,
    instance: &CheckInstance,
    open: &BTreeMap<CheckInstance, OpenVerdict>,
    skipped: &BTreeSet<CheckInstance>,
) -> Counted {
    if plan.is_statically_incomplete(&instance.check) || skipped.contains(instance) {
        return Counted::Incomplete;
    }
    match open.get(instance) {
        Some(bundle) if bundle.diagnostics.iter().any(|d| d.violation.refutes()) => Counted::Failed,
        Some(bundle) if !bundle.incomplete.is_empty() => Counted::Incomplete,
        _ => Counted::Passed,
    }
}

/// Count `instance` once more (`add`) or once less in `entry` as `counted`.
pub const fn count(entry: &mut CheckCoverage, counted: Counted, add: bool) {
    let field = match counted {
        Counted::Passed => &mut entry.passed,
        Counted::Failed => &mut entry.failed,
        Counted::Incomplete => &mut entry.incomplete,
    };
    if add {
        *field += 1;
    } else {
        *field -= 1;
    }
}

/// The coverage of every applicable compiled check: its instances among
/// `all`, each counted as [`classify`] says.
pub fn coverage(
    plan: &ValidationPlan,
    shape: panproto_schema::InstanceShapeKind,
    all: &BTreeSet<CheckInstance>,
    open: &BTreeMap<CheckInstance, OpenVerdict>,
    skipped: &BTreeSet<CheckInstance>,
) -> BTreeMap<CheckKey, CheckCoverage> {
    let mut out = BTreeMap::new();
    for (key, spec) in plan.checks() {
        if !spec.applies_to(shape) {
            continue;
        }
        let first = CheckInstance {
            check: key.clone(),
            subject: None,
        };
        let mut entry = CheckCoverage {
            class: spec.class,
            requirement: spec.requirement,
            instances: 0,
            passed: 0,
            failed: 0,
            incomplete: 0,
        };
        for instance in all.range(first..).take_while(|i| &i.check == key) {
            entry.instances += 1;
            count(&mut entry, classify(plan, instance, open, skipped), true);
        }
        out.insert(key.clone(), entry);
    }
    out
}

/// Whether the kind's one instance is about the whole instance (subject
/// `None`), as `instances` enumerates it.
pub const fn is_whole_instance(kind: super::report::CheckKind) -> bool {
    use super::report::CheckKind as K;
    matches!(
        kind,
        K::SchemaWellFormed | K::DeclarationInterpretable | K::TreeRoot | K::KeyUnique | K::Acyclic
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use panproto_schema::{InstanceShapeKind, Protocol, Schema};

    use super::Session;
    use crate::ginstance::GInstance;
    use crate::validation::checks::InstanceView;
    use crate::validation::{
        CheckInstance, CheckKey, CheckKind, ExternalVerdicts, SchemaRef, ValidationOptions,
        ValidationPlan,
    };
    use crate::view_ref::ViewRef;

    #[test]
    fn a_check_absent_from_the_plan_is_inert() {
        let schema = Schema::default();
        let protocol = Protocol {
            syntactic_shape: InstanceShapeKind::Graph,
            semantic_shape: InstanceShapeKind::Graph,
            ..Protocol::default()
        };
        let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
        let graph = GInstance::new();
        let external = ExternalVerdicts::default();
        let session = Session::new(&plan, InstanceView::Graph(&graph), &external);
        let absent = CheckInstance {
            check: CheckKey::new(CheckKind::External, SchemaRef::External("absent".into())),
            subject: Some(ViewRef::Node(0)),
        };

        assert!(session.inert(&absent));
        assert!(session.trace(&absent).is_empty());
        assert_eq!(session.trace_within(&absent, 0), Some(BTreeSet::new()));
    }
}
