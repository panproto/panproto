//! Failure witnesses: checking a diagnostic against the instance it refutes.
//!
//! A certifying algorithm returns, with each answer, a witness that a checker
//! much simpler than the algorithm confirms (`McConnell` et al. 2011, stated).
//! A [`Diagnostic`]'s witness is its [`Violation`] payload with its subject
//! and elements, and [`Diagnostic::verify`] is the checker: it reads the
//! elements the violation names and confirms the refutation, without
//! rerunning the validator over the instance. A cycle is confirmed by
//! walking its edges, a duplicate key by reading the key of each element it
//! names, an unknown anchor by one schema lookup, and so on.
//!
//! Besides the violation itself, a diagnostic verifies only when it is the
//! diagnostic of a compiled check of the plan with the check's class,
//! requirement and declared source, when its subject is an instance of that
//! check in the instance, and when its elements are exactly the elements the
//! violation names, ascending.
//!
//! Verification confirms that a diagnostic refutes the instance, not that it
//! is the diagnostic the validator reports. Where one fault has several
//! witnesses, each verifies: a `Cycle` verifies with any closed walk of
//! edges labelled acyclic through its component, so the validator's walk
//! rotated to start at another node, or walked twice, verifies too; and a
//! `ParentMismatch` verifies with the parent of any arc into the node other
//! than the recorded parent, while the validator names the first. A
//! diagnostic altered so that it no longer refutes the instance (another
//! subject, another element, a walk that does not close or leaves the
//! component, a key the named elements do not share) fails to verify.
//!
//! ```
//! use panproto_gat::CheckStatus;
//! use panproto_inst::validation::{
//!     ExternalVerdicts, ValidationBudget, ValidationOptions, ValidationPlan, Violation,
//! };
//! use panproto_inst::{GInstance, Node, ViewRef};
//! use panproto_schema::{Protocol, Schema, Vertex};
//!
//! let mut schema = Schema::default();
//! schema.vertices.insert(
//!     "person".into(),
//!     Vertex { id: "person".into(), kind: "object".into(), nsid: None },
//! );
//! let plan =
//!     ValidationPlan::compile(&schema, &Protocol::default(), &ValidationOptions::default());
//! let ghost = GInstance::new().with_node(Node::new(0, "ghost"));
//! let report =
//!     plan.validate_graph(&ghost, &ValidationBudget::default(), &ExternalVerdicts::default());
//! assert_eq!(report.status, CheckStatus::Failed);
//! let diagnostic = &report.diagnostics[0];
//! assert!(diagnostic.verify(&plan, &ghost));
//!
//! // A diagnostic naming another anchor is not a refutation of this graph.
//! let mut forged = diagnostic.clone();
//! forged.violation = Violation::UnknownAnchor { anchor: "phantom".into() };
//! assert!(!forged.verify(&plan, &ghost));
//! ```
//!
//! A cycle's walk is one witness among several:
//!
//! ```
//! use panproto_inst::validation::{
//!     CheckKind, ExternalVerdicts, ValidationBudget, ValidationOptions, ValidationPlan, Violation,
//! };
//! use panproto_inst::{GInstance, Node};
//! use panproto_schema::{
//!     Constraint, ConstraintSemantics, Edge, Protocol, Schema, Vertex,
//! };
//!
//! let next = Edge {
//!     src: "item".into(),
//!     tgt: "item".into(),
//!     kind: "prop".into(),
//!     name: Some("next".into()),
//! };
//! let mut schema = Schema::default();
//! schema.vertices.insert(
//!     "item".into(),
//!     Vertex { id: "item".into(), kind: "object".into(), nsid: None },
//! );
//! schema.edges.insert(next.clone(), "prop".into());
//! schema.outgoing.entry("item".into()).or_default().push(next.clone());
//! schema.incoming.entry("item".into()).or_default().push(next.clone());
//! schema.between.entry(("item".into(), "item".into())).or_default().push(next.clone());
//! schema.constraints.insert(
//!     "item".into(),
//!     vec![Constraint { sort: "acyclic".into(), value: "next".into() }],
//! );
//! let protocol = Protocol {
//!     constraint_sorts: vec!["acyclic".into()],
//!     constraint_semantics: [("acyclic".to_owned(), ConstraintSemantics::Acyclic)].into(),
//!     ..Protocol::default()
//! };
//! let plan = ValidationPlan::compile(&schema, &protocol, &ValidationOptions::default());
//! let ring = GInstance::new()
//!     .with_node(Node::new(0, "item"))
//!     .with_node(Node::new(1, "item"))
//!     .with_node(Node::new(2, "item"))
//!     .with_edge(0, 1, next.clone())
//!     .with_edge(1, 2, next.clone())
//!     .with_edge(2, 0, next);
//! let report =
//!     plan.validate_graph(&ring, &ValidationBudget::default(), &ExternalVerdicts::default());
//! let cycle = report
//!     .diagnostics
//!     .iter()
//!     .find(|d| d.check.kind == CheckKind::Acyclic)
//!     .expect("the ring is a cycle");
//! let Violation::Cycle { scc, witness } = &cycle.violation else { unreachable!() };
//! assert!(cycle.verify(&plan, &ring));
//!
//! // The same walk from another node, and walked twice: other witnesses of
//! // the same cycle, which verify although the validator reports neither.
//! let mut rotated = cycle.clone();
//! let mut walk = witness.clone();
//! walk.rotate_left(1);
//! rotated.violation = Violation::Cycle { scc: scc.clone(), witness: walk };
//! assert!(rotated.verify(&plan, &ring) && rotated != *cycle);
//! let mut doubled = cycle.clone();
//! doubled.violation = Violation::Cycle { scc: scc.clone(), witness: witness.repeat(2) };
//! assert!(doubled.verify(&plan, &ring) && doubled != *cycle);
//!
//! // A walk that does not close is no witness.
//! let mut open = cycle.clone();
//! open.violation = Violation::Cycle { scc: scc.clone(), witness: witness[..2].to_vec() };
//! assert!(!open.verify(&plan, &ring));
//! ```

use std::collections::{BTreeSet, HashSet, VecDeque};

use panproto_gat::{CheckStatus, Name};
use panproto_schema::{CanonicalWriter, Cardinality, Edge, Key, KeyPart, RefSource, Reference};
use serde::{Deserialize, Serialize};

use crate::attributes::AttributeViolation;
use crate::fan_violation::FanViolation;
use crate::functor::FInstance;
use crate::ginstance::{EdgeId, FanDomain, GInstance};
use crate::identity::{CanonicalSink, Digest};
use crate::value::Value;
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

use super::checks::{
    AnchorFamilies, Context, EdgeRec, InstanceView, TreeArcs, edges_at, evaluate, instances,
};
use super::plan::{Compiled, ExternalVerdicts, ValidationBudget, ValidationPlan};
use super::report::{
    CheckInstance, CheckKind, Diagnostic, EndpointSide, FanRecord, IncompleteReason, Requirement,
    Violation, write_check_key, write_kind, write_reason, write_status, write_view_ref,
    write_violation,
};
use super::state::{
    AdmissionPolicy, Prepared, ValidatedShape, ValidationState, check_state, key_values, part_name,
    part_value, path_values, prepare_delta, scope_of,
};

/// Optional O(n) instance commitments carried by an edit witness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WitnessOptions {
    /// Include prior and result instance digests.
    pub digests: bool,
}

/// One failed result of an evaluated check instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunFinding {
    /// Elements named by the violation, ascending without repeats.
    pub elements: Vec<ViewRef>,
    /// The typed violation.
    pub violation: Violation,
}

/// What happened when one scheduled check instance was considered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunVerdict {
    /// The check was evaluated, retaining every finding and incomplete reason.
    Evaluated {
        /// Findings in canonical order.
        findings: Vec<RunFinding>,
        /// Incomplete reasons in canonical order.
        incomplete: Vec<IncompleteReason>,
    },
    /// The scheduler did not evaluate the check.
    Skipped {
        /// Why it was skipped.
        reason: IncompleteReason,
    },
}

/// One scheduled check and its certified dependencies and outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CheckRun {
    /// The check instance.
    pub instance: CheckInstance,
    /// Dependencies read, ascending without repeats.
    pub reads: Vec<ViewRef>,
    /// Evaluated outcome or scheduler disposition.
    pub verdict: RunVerdict,
}

/// Replayable certificate of one admitted edit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditWitness {
    /// Validation plan identity.
    pub plan: Digest,
    /// Baseline sequence number.
    pub baseline_seq: u64,
    /// Baseline aggregate status.
    pub baseline_status: CheckStatus,
    /// Canonical normalized-delta digest.
    pub delta: Digest,
    /// Result aggregate status.
    pub result_status: CheckStatus,
    /// Scheduled checks, ascending by check instance.
    pub runs: Vec<CheckRun>,
    /// Compiled kinds whose trigger rows the edit did not meet.
    pub not_triggered: BTreeSet<CheckKind>,
    /// Embedded commitment policy.
    pub options: WitnessOptions,
    /// Prior instance digest when requested.
    pub prior: Option<Digest>,
    /// Result instance digest when requested.
    pub result: Option<Digest>,
}

/// Every trusted input of [`EditWitness::verify`]: the replay context of
/// one admitted transition.
pub struct WitnessReplay<'a, I: ValidatedShape> {
    /// The plan the transition was validated against.
    pub plan: &'a ValidationPlan,
    /// The trusted committed state paired with `prior`.
    pub baseline: &'a ValidationState,
    /// The instance before the transition.
    pub prior: &'a I,
    /// The normalized delta.
    pub delta: &'a I::Delta,
    /// The admission policy.
    pub policy: AdmissionPolicy,
    /// The transition budget.
    pub budget: &'a ValidationBudget,
    /// The target external projection.
    pub external: &'a ExternalVerdicts,
    /// The witness options the verifier expects.
    pub expected_options: WitnessOptions,
}

/// A field of an edit witness did not replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("edit witness mismatch in {field}")]
pub struct WitnessMismatch {
    /// The mismatching field.
    pub field: String,
    /// The affected run, when one can be named (boxed, so that the error
    /// stays small; its serialized form is the instance's).
    pub instance: Option<Box<CheckInstance>>,
}

impl EditWitness {
    /// Canonical bytes of the complete replay certificate.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut writer = CanonicalWriter::new();
        write_edit_witness(&mut writer, self);
        writer.into_bytes()
    }

    /// Replay the transition from its trusted paired baseline.
    ///
    /// `replay` carries every input the replay needs, each as admission
    /// used it: the plan, the trusted baseline state paired with the prior
    /// instance, the normalized delta, the policy, the budget, the target
    /// external projection and the trusted expected witness options. The
    /// expected options prevent a verifier from accepting a digest-bearing
    /// witness after both its commitments and its embedded option bit have
    /// been stripped.
    ///
    /// # Errors
    ///
    /// Returns [`WitnessMismatch`] naming the first scalar, replay input, or
    /// scheduled run that differs from the recomputed transition.
    pub fn verify<I: ValidatedShape>(
        &self,
        replay: &WitnessReplay<'_, I>,
    ) -> Result<(), WitnessMismatch> {
        let &WitnessReplay {
            plan,
            baseline,
            prior,
            delta,
            policy,
            budget,
            external,
            expected_options,
        } = replay;
        if self.plan != *plan.digest() || baseline.plan != *plan.digest() {
            return Err(witness_mismatch("plan", None));
        }
        check_state(plan, prior, baseline).map_err(|_| witness_mismatch("baseline_state", None))?;
        if self.baseline_seq != baseline.seq || baseline.seq == u64::MAX {
            return Err(witness_mismatch("baseline_seq", None));
        }
        if self.baseline_status != baseline.status {
            return Err(witness_mismatch("baseline_status", None));
        }
        if self.delta != I::delta_digest(delta) {
            return Err(witness_mismatch("delta", None));
        }
        if self.options != expected_options {
            return Err(witness_mismatch("options", None));
        }
        if self.options.digests != self.prior.is_some()
            || self.options.digests != self.result.is_some()
        {
            return Err(witness_mismatch("options", None));
        }
        if self.options.digests && self.prior != Some(prior.instance_digest()) {
            return Err(witness_mismatch("prior", None));
        }
        let prepared = prepare_delta(plan, baseline, prior, delta, budget, external)
            .map_err(|_| witness_mismatch("apply", None))?;
        if !policy.admits(plan, baseline, &prepared.state) {
            return Err(witness_mismatch("admission", None));
        }
        let expected = build_witness(plan, baseline, prior, delta, self.options, &prepared);
        if self.result_status != expected.result_status {
            return Err(witness_mismatch("result_status", None));
        }
        if self.not_triggered != expected.not_triggered {
            return Err(witness_mismatch("not_triggered", None));
        }
        if self.result != expected.result {
            return Err(witness_mismatch("result", None));
        }
        if self.runs.len() != expected.runs.len() {
            return Err(witness_mismatch("runs", None));
        }
        for (actual, wanted) in self.runs.iter().zip(&expected.runs) {
            if run_bytes(actual) != run_bytes(wanted) {
                return Err(witness_mismatch("run", Some(wanted.instance.clone())));
            }
        }
        Ok(())
    }
}

/// The witness of the prepared transition from the baseline `(prior,
/// baseline)` by `delta`: one run per scheduled check instance with its
/// post-state trace, as the transition discovered it, and its committed
/// disposition and outcome.
pub fn build_witness<I: ValidatedShape>(
    plan: &ValidationPlan,
    baseline: &ValidationState,
    prior: &I,
    delta: &I::Delta,
    options: WitnessOptions,
    prepared: &Prepared<I>,
) -> EditWitness {
    let footprint = &prepared.validation.footprint;
    let runs = footprint
        .rerun
        .union(&footprint.created)
        .map(|instance| CheckRun {
            instance: instance.clone(),
            reads: prepared
                .traces
                .get(instance)
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            verdict: run_verdict(plan, &prepared.state, instance),
        })
        .collect();
    let met: BTreeSet<CheckKind> = I::change_kinds(delta)
        .into_iter()
        .flat_map(|change| plan.triggers().kinds(change).into_iter().flatten().copied())
        .collect();
    let compiled: BTreeSet<CheckKind> = plan.checks().keys().map(|key| key.kind).collect();
    let not_triggered = compiled.difference(&met).copied().collect();
    EditWitness {
        plan: *plan.digest(),
        baseline_seq: baseline.seq,
        baseline_status: baseline.status,
        delta: I::delta_digest(delta),
        result_status: prepared.state.status,
        runs,
        not_triggered,
        options,
        prior: options.digests.then(|| prior.instance_digest()),
        result: options.digests.then(|| prepared.instance.instance_digest()),
    }
}

fn run_verdict(
    plan: &ValidationPlan,
    state: &ValidationState,
    instance: &CheckInstance,
) -> RunVerdict {
    if plan.is_statically_incomplete(&instance.check)
        && let Some(finding) = plan
            .static_findings()
            .iter()
            .find(|finding| finding.check == instance.check)
    {
        return RunVerdict::Skipped {
            reason: finding.reason.clone(),
        };
    }
    if let Some(cut) = state.budget_cuts.get(&instance.check)
        && cut.skipped.binary_search(instance).is_ok()
    {
        return RunVerdict::Skipped {
            reason: IncompleteReason::Budget {
                limit: cut.limit,
                spent: cut.spent,
                from: cut.from.clone(),
            },
        };
    }
    let open = state.open.get(instance);
    RunVerdict::Evaluated {
        findings: open
            .into_iter()
            .flat_map(|verdict| &verdict.diagnostics)
            .map(|diagnostic| RunFinding {
                elements: diagnostic.elements.clone(),
                violation: diagnostic.violation.clone(),
            })
            .collect(),
        incomplete: open
            .into_iter()
            .flat_map(|verdict| &verdict.incomplete)
            .map(|incomplete| incomplete.reason.clone())
            .collect(),
    }
}

fn witness_mismatch(field: &str, instance: Option<CheckInstance>) -> WitnessMismatch {
    WitnessMismatch {
        field: field.to_owned(),
        instance: instance.map(Box::new),
    }
}

fn run_bytes(run: &CheckRun) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    write_run(&mut writer, run);
    writer.into_bytes()
}

fn write_edit_witness<W: CanonicalSink + ?Sized>(w: &mut W, witness: &EditWitness) {
    w.write(&witness.plan.0);
    w.u64(witness.baseline_seq);
    write_status(w, witness.baseline_status);
    w.write(&witness.delta.0);
    write_status(w, witness.result_status);
    w.len(witness.runs.len());
    for run in &witness.runs {
        write_run(w, run);
    }
    w.len(witness.not_triggered.len());
    for kind in &witness.not_triggered {
        write_kind(w, *kind);
    }
    w.bool(witness.options.digests);
    write_opt_digest(w, witness.prior.as_ref());
    write_opt_digest(w, witness.result.as_ref());
}

fn write_run<W: CanonicalSink + ?Sized>(w: &mut W, run: &CheckRun) {
    write_check_key(w, &run.instance.check);
    write_opt_view(w, run.instance.subject.as_ref());
    w.len(run.reads.len());
    for read in &run.reads {
        write_view_ref(w, read);
    }
    match &run.verdict {
        RunVerdict::Evaluated {
            findings,
            incomplete,
        } => {
            w.tag(0);
            w.len(findings.len());
            for finding in findings {
                w.len(finding.elements.len());
                for element in &finding.elements {
                    write_view_ref(w, element);
                }
                write_violation(w, &finding.violation);
            }
            w.len(incomplete.len());
            for reason in incomplete {
                write_reason(w, reason);
            }
        }
        RunVerdict::Skipped { reason } => {
            w.tag(1);
            write_reason(w, reason);
        }
    }
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

fn write_opt_digest<W: CanonicalSink + ?Sized>(w: &mut W, value: Option<&Digest>) {
    match value {
        None => w.tag(0),
        Some(value) => {
            w.tag(1);
            w.write(&value.0);
        }
    }
}

mod sealed {
    use super::InstanceView;

    /// Reads an instance of any shape through the validator's view of it.
    pub trait Sealed {
        /// Call `f` with the view of this instance.
        fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R;
    }
}

/// An instance validation reads: a [`GInstance`], a [`WInstance`] or an
/// [`FInstance`]. Sealed: these are the three shapes.
pub trait CheckedInstance: sealed::Sealed {}

impl sealed::Sealed for GInstance {
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        f(InstanceView::Graph(self))
    }
}

impl sealed::Sealed for WInstance {
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        let arcs = TreeArcs::new(self);
        f(InstanceView::Tree(self, &arcs))
    }
}

impl sealed::Sealed for FInstance {
    fn with_view<R>(&self, f: impl FnOnce(InstanceView<'_>) -> R) -> R {
        f(InstanceView::Table(self))
    }
}

impl CheckedInstance for GInstance {}
impl CheckedInstance for WInstance {}
impl CheckedInstance for FInstance {}

impl Diagnostic {
    /// Whether this diagnostic is a true refutation of `instance` under
    /// `plan`: its check is compiled by the plan with this class,
    /// requirement and source; its subject is an instance of the check; its
    /// elements are exactly those its violation names, ascending; and the
    /// violation holds of the elements it names.
    ///
    /// A fault with several witnesses verifies with each of them, not only
    /// with the one the validator reports: any closed walk of edges labelled
    /// acyclic through a `Cycle`'s component (the validator's walk rotated,
    /// or walked twice, among them), and the parent of any arc into the node
    /// other than the recorded one for a `ParentMismatch`.
    ///
    /// Each violation is confirmed by reading what it names: an
    /// `UnknownAnchor` by looking its anchor up in the schema, an
    /// `IncoherentEndpoint` by reading the edge's endpoint, a `Cycle` by
    /// walking its edges (consecutive edges join, the last edge returns to
    /// the first edge's source, every edge carries a label declared acyclic)
    /// and checking that its component is the strongly connected component
    /// the walk lies in, a `DuplicateKey` by reading the key of each element
    /// it names and confirming that no other element of its scope shares it,
    /// a `DanglingReference` or `AmbiguousReference` by reading the elements
    /// of the target vertex that carry the named key, and so on. A
    /// `ValueConstraint` is confirmed by reading the value it names at the
    /// subject and testing it against the compiled constraint.
    ///
    /// An `ExternalRejected` diagnostic is confirmed as an instance of its
    /// external check: the verdict is the application's, and panproto never
    /// evaluates the rule behind it. [`Diagnostic::verify_with_verdicts`]
    /// also confirms the verdict.
    #[must_use]
    pub fn verify<I: CheckedInstance>(&self, plan: &ValidationPlan, instance: &I) -> bool {
        instance.with_view(|view| verify_in(self, plan, view, None))
    }

    /// [`Diagnostic::verify`], confirming an `ExternalRejected` diagnostic
    /// against the verdicts the validation read: the verdict of its check on
    /// its subject rejects it, with the witness the diagnostic names.
    #[must_use]
    pub fn verify_with_verdicts<I: CheckedInstance>(
        &self,
        plan: &ValidationPlan,
        instance: &I,
        external: &ExternalVerdicts,
    ) -> bool {
        instance.with_view(|view| verify_in(self, plan, view, Some(external)))
    }
}

/// The canonical bytes of a violation, which compare values by their bytes
/// (a `NaN` value equals itself).
fn violation_bytes(v: &Violation) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    write_violation(&mut w, v);
    w.into_bytes()
}

/// Whether two values are one value: equal canonical bytes.
fn same_value(a: &Value, b: &Value) -> bool {
    a.canonical_cmp(b).is_eq()
}

/// Whether two value lists are one list, value by value.
fn same_values(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_value(x, y))
}

/// `refs`, ascending and without repeats.
fn sorted(mut refs: Vec<ViewRef>) -> Vec<ViewRef> {
    refs.sort();
    refs.dedup();
    refs
}

fn verify_in(
    d: &Diagnostic,
    plan: &ValidationPlan,
    view: InstanceView<'_>,
    external: Option<&ExternalVerdicts>,
) -> bool {
    let Some(spec) = plan.checks().get(&d.check) else {
        return false;
    };
    if plan.is_statically_incomplete(&d.check)
        || !spec.applies_to(view.shape())
        || spec.class != d.class
        || d.requirement
            != if d.violation.refutes() {
                spec.requirement
            } else {
                Requirement::Advisory
            }
        || plan.options.sources.0.get(&d.check.decl) != d.source.as_ref()
    {
        return false;
    }
    let none = ExternalVerdicts::default();
    let ctx = Context::new(plan, view, external.unwrap_or(&none));
    if let Violation::OutsideKeyScope { key, scope } = &d.violation {
        return outside_key_scope(&ctx, d, key, scope);
    }
    if !instances(&ctx, &d.check).contains(&d.subject) {
        return false;
    }
    expected_elements(&ctx, d)
        .is_some_and(|elements| elements == d.elements && holds(&ctx, d, external.is_some()))
}

/// The edges a graph fan violation names.
fn fan_edges(reason: &FanViolation) -> Vec<ViewRef> {
    match reason {
        FanViolation::MultipleParents { edges } | FanViolation::DuplicateLabel { edges, .. } => {
            edges.iter().map(|e| ViewRef::Edge(*e)).collect()
        }
        FanViolation::ReservedKindOutsideFan { edge }
        | FanViolation::ForeignIncidentEdge { edge } => vec![ViewRef::Edge(*edge)],
        FanViolation::MissingParent
        | FanViolation::UnknownLabel { .. }
        | FanViolation::UnknownHyperEdge { .. }
        | FanViolation::MissingChild { .. } => Vec::new(),
    }
}

/// The endpoint at `side` of the edge `e`.
fn endpoint(view: InstanceView<'_>, e: &ViewRef, side: EndpointSide) -> Option<ViewRef> {
    let rec = edges_at(view, e).into_iter().next()?;
    Some(match side {
        EndpointSide::Src => rec.src,
        EndpointSide::Tgt => rec.tgt,
    })
}

/// The elements the diagnostic's violation names, with its subject,
/// ascending; `None` when the violation cannot have the diagnostic's
/// subject.
fn expected_elements(ctx: &Context<'_>, d: &Diagnostic) -> Option<Vec<ViewRef>> {
    let view = ctx.view;
    let subject = d.subject.clone();
    let with_subject = |mut refs: Vec<ViewRef>| -> Option<Vec<ViewRef>> {
        refs.push(subject.clone()?);
        Some(sorted(refs))
    };
    match &d.violation {
        Violation::UnknownAnchor { .. }
        | Violation::UndeclaredEdge { .. }
        | Violation::ReservedVocabulary
        | Violation::MissingRequiredEdge { .. }
        | Violation::ValueConstraint { .. }
        | Violation::MissingValue { .. }
        | Violation::UndeclaredAttribute { .. }
        | Violation::MistypedAttribute { .. }
        | Violation::DanglingReference { .. }
        | Violation::Unreachable
        | Violation::ExternalRejected { .. }
        | Violation::OutsideKeyScope { .. } => with_subject(Vec::new()),
        Violation::IncoherentEndpoint { side, .. } => {
            let e = subject.clone()?;
            with_subject(vec![endpoint(view, &e, *side)?])
        }
        Violation::MalformedFan { reason, fan } => match (view, fan) {
            (InstanceView::Graph(_), None) => with_subject(fan_edges(reason)),
            (InstanceView::Tree(..), Some(record)) => with_subject(
                record
                    .children
                    .iter()
                    .map(|(_, child)| ViewRef::Node(*child))
                    .collect(),
            ),
            _ => None,
        },
        Violation::OutDegree { edges, .. } => with_subject(edges.clone()),
        Violation::DuplicatePosition { edges, .. } => {
            with_subject(edges.iter().map(|e| ViewRef::Edge(*e)).collect())
        }
        Violation::DuplicateKey { elems, .. } => subject.is_none().then(|| sorted(elems.clone())),
        Violation::AmbiguousReference { candidates, .. } => with_subject(candidates.clone()),
        Violation::Cycle { scc, witness } => {
            let mut refs = scc.clone();
            refs.extend(witness.iter().map(|e| ViewRef::Edge(*e)));
            subject.is_none().then(|| sorted(refs))
        }
        Violation::MissingRoot => subject.is_none().then(Vec::new),
        Violation::ParentMismatch { actual, .. } => {
            let ViewRef::Node(id) = subject.clone()? else {
                return None;
            };
            with_subject(
                actual
                    .iter()
                    .map(|p| ViewRef::Arc {
                        parent: *p,
                        child: id,
                    })
                    .collect(),
            )
        }
    }
}

/// An element outside a `Within` key's domain: the diagnostic's check is a
/// compiled `KeyUnique` of a key scoped within `scope`, its subject is the
/// one element it names, anchored at the key's vertex with a complete key
/// value, and no ancestor of the subject is anchored at `scope`.
fn outside_key_scope(ctx: &Context<'_>, d: &Diagnostic, key_id: &Name, scope: &Name) -> bool {
    let Some(Compiled::KeyUnique { vertex, key }) = ctx.plan.compiled.get(&d.check) else {
        return false;
    };
    let Some(subject) = d.subject.as_ref() else {
        return false;
    };
    let view = ctx.view;
    d.check.kind == CheckKind::KeyUnique
        && key.id == *key_id
        && matches!(&key.scope, panproto_schema::KeyScope::Within(v) if v == scope)
        && d.elements == [subject.clone()]
        && view
            .node(subject)
            .is_some_and(|node| !node.fan && node.anchor == vertex)
        && key_values(view, subject, key).is_some()
        && scope_of(view, subject, &key.scope).is_none()
}

/// Whether `refs` is strictly ascending.
fn ascending<T: Ord>(refs: &[T]) -> bool {
    refs.windows(2).all(|w| w[0] < w[1])
}

/// Whether the violation holds of what it names, for the diagnostic's check
/// and subject. `verdicts` says whether an external rejection is confirmed
/// against the context's verdicts.
fn holds(ctx: &Context<'_>, d: &Diagnostic, verdicts: bool) -> bool {
    match d.check.kind {
        CheckKind::NodeAnchor
        | CheckKind::EdgeDeclared
        | CheckKind::EndpointCoherence
        | CheckKind::FanWellFormed
        | CheckKind::ReservedVocabulary => element_holds(ctx, d),
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::ValueConstraint
        | CheckKind::RequiredValue
        | CheckKind::Attribute => local_holds(ctx, d),
        CheckKind::KeyUnique | CheckKind::ReferenceResolves | CheckKind::Acyclic => {
            global_holds(ctx, d)
        }
        CheckKind::TreeRoot | CheckKind::TreeParentMap | CheckKind::TreeReachable => {
            tree_holds(ctx, d)
        }
        CheckKind::External => external_holds(ctx, d, verdicts),
        CheckKind::SchemaWellFormed | CheckKind::DeclarationInterpretable => false,
    }
}

/// The anchor, edge, endpoint, fan and reserved-vocabulary violations.
fn element_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    match (d.check.kind, &d.violation, d.subject.as_ref()) {
        (CheckKind::NodeAnchor, Violation::UnknownAnchor { anchor }, Some(n)) => {
            ctx.view
                .node(n)
                .is_some_and(|node| !node.fan && node.anchor == anchor)
                && !ctx.plan.schema.has_vertex(anchor)
        }
        (CheckKind::EdgeDeclared, Violation::UndeclaredEdge { label }, Some(e)) => {
            !ctx.plan.schema.edges.contains_key(label)
                && edges_at(ctx.view, e)
                    .iter()
                    .any(|rec| rec.label == label && !ctx.view.is_fan_edge(rec))
        }
        (
            CheckKind::EndpointCoherence,
            Violation::IncoherentEndpoint {
                side,
                expected,
                found,
            },
            Some(e),
        ) => incoherent(ctx, e, *side, expected, found),
        (CheckKind::FanWellFormed, Violation::MalformedFan { reason, fan }, Some(f)) => {
            malformed_fan(ctx, f, reason, fan.as_ref())
        }
        (CheckKind::ReservedVocabulary, Violation::ReservedVocabulary, Some(x)) => ctx
            .view
            .tree()
            .is_some_and(|t| crate::wtype_check::reserved_vocabulary(t).contains(x)),
        _ => false,
    }
}

/// The violations of one element's out-edges, values and attributes.
fn local_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    let compiled = ctx.plan.compiled.get(&d.check);
    match (d.check.kind, &d.violation, d.subject.as_ref()) {
        (CheckKind::RequiredEdge, Violation::MissingRequiredEdge { edge }, Some(n)) => {
            matches!(compiled, Some(Compiled::Required { edge: e }) if e == edge)
                && !non_fan_out_edges(ctx.view, n)
                    .iter()
                    .any(|rec| rec.label == edge)
        }
        (CheckKind::OutDegree, Violation::OutDegree { .. }, Some(n)) => {
            out_degree(ctx, compiled, n, &d.violation)
        }
        (
            CheckKind::PositionDistinct,
            Violation::DuplicatePosition { position, edges },
            Some(n),
        ) => match compiled {
            Some(Compiled::Ordering { edge: label }) => {
                let mut at: Vec<EdgeId> = ctx
                    .view
                    .out_edges(n)
                    .into_iter()
                    .filter(|rec| rec.label == label && rec.position == Some(*position))
                    .filter_map(|rec| match rec.at {
                        ViewRef::Edge(id) => Some(id),
                        _ => None,
                    })
                    .collect();
                at.sort();
                edges.len() > 1 && ascending(edges) && at == *edges
            }
            _ => false,
        },
        (CheckKind::ValueConstraint, Violation::ValueConstraint { .. }, Some(n)) => {
            value_refuted(ctx, d, n)
        }
        (CheckKind::RequiredValue, Violation::MissingValue { field }, Some(n)) => {
            missing_value(ctx, d, n, field.as_ref())
        }
        (
            CheckKind::Attribute,
            Violation::UndeclaredAttribute { .. } | Violation::MistypedAttribute { .. },
            Some(n),
        ) => attribute_refuted(ctx, n, &d.violation),
        _ => false,
    }
}

/// The key, reference and cycle violations.
fn global_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    let compiled = ctx.plan.compiled.get(&d.check);
    match (compiled, &d.violation, d.subject.as_ref()) {
        (
            Some(Compiled::KeyUnique { vertex, key: k }),
            Violation::DuplicateKey { key, elems },
            None,
        ) => duplicate_key(ctx, vertex, k, key, elems),
        (Some(Compiled::Reference { .. }), Violation::MissingValue { field }, Some(h)) => {
            missing_value(ctx, d, h, field.as_ref())
        }
        (
            Some(Compiled::Reference {
                reference: r,
                target,
            }),
            Violation::DanglingReference { reference, key },
            Some(h),
        ) => {
            r.id == *reference
                && holder_names(ctx, h, r, target, key)
                && target_elements(ctx, h, r, target, key).is_empty()
        }
        (
            Some(Compiled::Reference {
                reference: r,
                target,
            }),
            Violation::AmbiguousReference {
                reference,
                key,
                candidates,
            },
            Some(h),
        ) => {
            r.id == *reference
                && candidates.len() > 1
                && holder_names(ctx, h, r, target, key)
                && target_elements(ctx, h, r, target, key) == *candidates
        }
        (Some(Compiled::Acyclic { labels }), Violation::Cycle { scc, witness }, None) => {
            cycle(ctx, labels, scc, witness)
        }
        _ => false,
    }
}

/// The root, parent-map and reachability violations of a tree.
fn tree_holds(ctx: &Context<'_>, d: &Diagnostic) -> bool {
    let Some(t) = ctx.view.tree() else {
        return false;
    };
    match (d.check.kind, &d.violation, d.subject.as_ref()) {
        (CheckKind::TreeRoot, Violation::MissingRoot, None) => !t.nodes.contains_key(&t.root),
        (CheckKind::TreeReachable, Violation::Unreachable, Some(ViewRef::Node(id))) => {
            t.nodes.contains_key(id) && !reachable_from_root(t).contains(id)
        }
        (
            CheckKind::TreeParentMap,
            Violation::ParentMismatch { recorded, actual },
            Some(ViewRef::Node(id)),
        ) => {
            let into: Vec<u32> = t
                .arcs
                .iter()
                .filter(|(_, c, _)| c == id)
                .map(|(p, _, _)| *p)
                .collect();
            t.parent_map.get(id).copied() == *recorded
                && actual.as_ref().map_or_else(
                    || into.is_empty() && recorded.is_some(),
                    |p| into.contains(p) && Some(*p) != *recorded,
                )
        }
        _ => false,
    }
}

/// An external rejection names an instance of its external check; with
/// `verdicts`, the verdict of that check on the subject rejects it with the
/// witness the violation names.
fn external_holds(ctx: &Context<'_>, d: &Diagnostic, verdicts: bool) -> bool {
    match (
        ctx.plan.compiled.get(&d.check),
        &d.violation,
        d.subject.as_ref(),
    ) {
        (Some(Compiled::External { name }), Violation::ExternalRejected { witness }, Some(n)) => {
            !verdicts
                || ctx
                    .external
                    .get(name, n)
                    .is_some_and(|v| !v.passed && v.witness == *witness)
        }
        _ => false,
    }
}

/// The out-edges of `n` that are not fan edges.
fn non_fan_out_edges<'a>(view: InstanceView<'a>, n: &ViewRef) -> Vec<EdgeRec<'a>> {
    view.out_edges(n)
        .into_iter()
        .filter(|rec| !view.is_fan_edge(rec))
        .collect()
}

/// The nodes of a tree reachable from its root through its children.
fn reachable_from_root(tree: &WInstance) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    if tree.nodes.contains_key(&tree.root) {
        reachable.insert(tree.root);
        queue.push_back(tree.root);
    }
    while let Some(current) = queue.pop_front() {
        for &child in tree.children(current) {
            if reachable.insert(child) {
                queue.push_back(child);
            }
        }
    }
    reachable
}

/// The endpoint at `side` of a non-fan edge at `e`, whose label names
/// `expected` there, is anchored at `found`, which the anchor families of
/// `expected` do not admit.
fn incoherent(
    ctx: &Context<'_>,
    e: &ViewRef,
    side: EndpointSide,
    expected: &Name,
    found: &Name,
) -> bool {
    let families = AnchorFamilies::new(&ctx.plan.schema);
    edges_at(ctx.view, e).iter().any(|rec| {
        let (end, declared) = match side {
            EndpointSide::Src => (&rec.src, &rec.label.src),
            EndpointSide::Tgt => (&rec.tgt, &rec.label.tgt),
        };
        !ctx.view.is_fan_edge(rec)
            && declared == expected
            && ctx.view.node(end).is_some_and(|node| node.anchor == found)
            && !families.accepts(expected, found)
    })
}

/// A graph fan node breaks `reason` (one of the violations
/// [`FanDomain::violations_at`] lists for it under the plan's schema); a tree
/// fan with record `fan`, at parent `f`, breaks `reason`.
fn malformed_fan(
    ctx: &Context<'_>,
    f: &ViewRef,
    reason: &FanViolation,
    fan: Option<&FanRecord>,
) -> bool {
    let schema = &ctx.plan.schema;
    match (ctx.view, f, fan) {
        (InstanceView::Graph(g), ViewRef::Node(id), None) => {
            FanDomain::violations_at(g, *id, Some(schema)).contains(reason)
        }
        (InstanceView::Tree(t, _), ViewRef::Node(parent), Some(record)) => {
            record.parent == *parent
                && t.fans.iter().any(|fan| FanRecord::of(fan) == *record)
                && match reason {
                    FanViolation::MissingParent => !t.nodes.contains_key(&record.parent),
                    FanViolation::MissingChild { label } => record
                        .children
                        .iter()
                        .any(|(l, child)| l == label && !t.nodes.contains_key(child)),
                    FanViolation::UnknownHyperEdge { hyper_edge } => {
                        *hyper_edge == record.hyper_edge
                            && !schema.hyper_edges.contains_key(hyper_edge)
                    }
                    FanViolation::UnknownLabel { label } => {
                        schema
                            .hyper_edges
                            .get(&record.hyper_edge)
                            .is_some_and(|h| !h.signature.contains_key(label))
                            && record.children.iter().any(|(l, _)| l == label)
                    }
                    FanViolation::MultipleParents { .. }
                    | FanViolation::DuplicateLabel { .. }
                    | FanViolation::ReservedKindOutsideFan { .. }
                    | FanViolation::ForeignIncidentEdge { .. } => false,
                }
        }
        _ => false,
    }
}

/// The out-degree the violation reports is the compiled bound's count of
/// `n`'s non-fan out-edges (along the bound's edge), it lists exactly the
/// edges counted, ascending, and the count breaks the bound.
fn out_degree(
    ctx: &Context<'_>,
    compiled: Option<&Compiled>,
    n: &ViewRef,
    violation: &Violation,
) -> bool {
    let (
        Some(Compiled::Degree {
            edge: c_edge,
            min: c_min,
            max: c_max,
        }),
        Violation::OutDegree {
            edge,
            min,
            max,
            found,
            edges,
        },
    ) = (compiled, violation)
    else {
        return false;
    };
    let counted = sorted(
        non_fan_out_edges(ctx.view, n)
            .into_iter()
            .filter(|rec| edge.as_ref().is_none_or(|label| rec.label == label))
            .map(|rec| rec.at)
            .collect(),
    );
    let count = u32::try_from(counted.len()).unwrap_or(u32::MAX);
    c_edge == edge
        && c_min == min
        && c_max == max
        && ascending(edges)
        && counted == *edges
        && count == *found
        && (min.is_some_and(|lo| count < lo) || max.is_some_and(|hi| count > hi))
}

/// The value the violation names is a value of the constrained vertex at
/// `n`, under the compiled constraint's sort and bound, and the compiled
/// test refutes it at `n`.
fn value_refuted(ctx: &Context<'_>, d: &Diagnostic, n: &ViewRef) -> bool {
    let (
        Some(Compiled::Value {
            vertex,
            sort,
            bound,
            ..
        }),
        Violation::ValueConstraint {
            sort: named_sort,
            bound: named_bound,
            value: Some(value),
        },
    ) = (ctx.plan.compiled.get(&d.check), &d.violation)
    else {
        return false;
    };
    let named = ctx
        .view
        .value_sites_at(&ctx.plan.schema, vertex, n)
        .iter()
        .any(|(_, presence)| presence.value().is_some_and(|x| same_value(x, value)));
    let bytes = violation_bytes(&d.violation);
    named
        && named_sort == sort
        && named_bound == bound
        && evaluate(ctx, &d.check, Some(n))
            .findings
            .iter()
            .any(|f| violation_bytes(&f.violation) == bytes)
}

/// The named field of `n` has no value: a value site of a required vertex,
/// a part of a primary key, or a key path of a reference that may not be
/// null.
fn missing_value(ctx: &Context<'_>, d: &Diagnostic, n: &ViewRef, field: Option<&String>) -> bool {
    let view = ctx.view;
    match (d.check.kind, ctx.plan.compiled.get(&d.check)) {
        (
            CheckKind::RequiredValue,
            Some(Compiled::RequiredValue {
                vertex,
                parts: None,
            }),
        ) => view
            .value_sites_at(&ctx.plan.schema, vertex, n)
            .iter()
            .any(|(f, presence)| f.as_ref() == field && presence.value().is_none()),
        (
            CheckKind::RequiredValue,
            Some(Compiled::RequiredValue {
                parts: Some(parts), ..
            }),
        ) => field.is_some_and(|name| {
            parts.iter().any(|part| {
                part_name(part) == *name
                    && !(matches!(view, InstanceView::Table(_))
                        && matches!(part, KeyPart::Position { .. }))
                    && part_value(view, n, part).is_none()
            })
        }),
        (CheckKind::ReferenceResolves, Some(Compiled::Reference { reference, target })) => {
            let RefSource::KeyValue { parts } = &reference.source else {
                return false;
            };
            !reference.nullable
                && parts.len() == target.parts.len()
                && (reference.cardinality == Cardinality::One || parts.len() == 1)
                && field.is_some_and(|name| {
                    parts.iter().any(|path| {
                        part_name(&KeyPart::Path(path.clone())) == *name
                            && path_values(view, n, path).is_empty()
                    })
                })
        }
        _ => false,
    }
}

/// The attribute discipline of the schema reports the violation's attribute
/// at `n`.
fn attribute_refuted(ctx: &Context<'_>, n: &ViewRef, violation: &Violation) -> bool {
    let attrs = &ctx.plan.attributes;
    let (found, id) = match (ctx.view, n) {
        (InstanceView::Graph(g), ViewRef::Node(id)) => (attrs.validate_ginstance(g), *id),
        (InstanceView::Tree(t, _), ViewRef::Node(id)) => (attrs.validate_wtype(t), *id),
        (InstanceView::Table(f), ViewRef::Row(RowId(id))) => (attrs.validate_finstance(f), *id),
        _ => return false,
    };
    found.iter().any(|v| match (v, violation) {
        (
            AttributeViolation::Undeclared { node_id, key, .. },
            Violation::UndeclaredAttribute { key: k },
        ) => *node_id == id && key == k,
        (
            AttributeViolation::Mistyped {
                node_id,
                key,
                expected,
                found,
                ..
            },
            Violation::MistypedAttribute {
                key: k,
                expected: e,
                found: f,
            },
        ) => *node_id == id && key == k && expected == e && found == f,
        _ => false,
    })
}

/// The elements share one key value in one scope, the violation names the
/// key's parts with that value, and no other element of the keyed vertex
/// in that scope carries it.
fn duplicate_key(
    ctx: &Context<'_>,
    vertex: &Name,
    key: &Key,
    named: &[(String, Value)],
    elems: &[ViewRef],
) -> bool {
    let view = ctx.view;
    if matches!(view, InstanceView::Table(_))
        && key
            .parts
            .iter()
            .any(|p| matches!(p, KeyPart::Position { .. }))
    {
        return false;
    }
    let Some(first) = elems.first() else {
        return false;
    };
    let names_match = named.len() == key.parts.len()
        && named
            .iter()
            .zip(&key.parts)
            .all(|((name, _), part)| *name == part_name(part));
    let values: Vec<Value> = named.iter().map(|(_, v)| v.clone()).collect();
    let scope = scope_of(view, first, &key.scope);
    if scope.is_none() && matches!(key.scope, panproto_schema::KeyScope::Within(_)) {
        // An element with no ancestor at a `Within` key's scope vertex is
        // outside the key's domain and duplicates nothing.
        return false;
    }
    let carries = |element: &ViewRef| {
        key_values(view, element, key).is_some_and(|vs| same_values(&vs, &values))
            && scope_of(view, element, &key.scope) == scope
    };
    let anchored = |element: &ViewRef| {
        view.node(element)
            .is_some_and(|node| !node.fan && node.anchor == vertex)
    };
    let all: Vec<ViewRef> = view
        .anchored(vertex)
        .into_iter()
        .filter(|e| carries(e))
        .collect();
    names_match
        && elems.len() > 1
        && ascending(elems)
        && elems.iter().all(|e| anchored(e) && carries(e))
        && all == elems
}

/// The key values `holder` names through `reference`: one value per part
/// for a single-target reference, one single-part value per target for a
/// many-target one; empty when a single-target reference that may be null
/// names nothing.
fn holder_keys(
    view: InstanceView<'_>,
    holder: &ViewRef,
    reference: &Reference,
    target: &Key,
) -> Option<Vec<Vec<Value>>> {
    let RefSource::KeyValue { parts } = &reference.source else {
        return None;
    };
    if parts.len() != target.parts.len() {
        return None;
    }
    match reference.cardinality {
        Cardinality::One => {
            let mut values = Vec::with_capacity(parts.len());
            for path in parts {
                match path_values(view, holder, path).first() {
                    Some(v) => values.push((*v).clone()),
                    None if reference.nullable => return Some(Vec::new()),
                    None => return None,
                }
            }
            Some(vec![values])
        }
        Cardinality::Many => {
            let [path] = parts.as_slice() else {
                return None;
            };
            Some(
                path_values(view, holder, path)
                    .into_iter()
                    .map(|v| vec![v.clone()])
                    .collect(),
            )
        }
    }
}

/// The holder names `key` through the reference.
fn holder_names(
    ctx: &Context<'_>,
    holder: &ViewRef,
    reference: &Reference,
    target: &Key,
    key: &[Value],
) -> bool {
    holder_keys(ctx.view, holder, reference, target)
        .is_some_and(|keys| keys.iter().any(|k| same_values(k, key)))
}

/// The elements of the reference's target vertex whose value of the target
/// key is `key` in the holder's scope under the key, ascending: under a
/// `Within(v)` key, those whose nearest v-ancestor is the holder's, and none
/// when the holder has no v-ancestor.
fn target_elements(
    ctx: &Context<'_>,
    holder: &ViewRef,
    reference: &Reference,
    target: &Key,
    key: &[Value],
) -> Vec<ViewRef> {
    let scope = scope_of(ctx.view, holder, &target.scope);
    if scope.is_none() && matches!(target.scope, panproto_schema::KeyScope::Within(_)) {
        return Vec::new();
    }
    ctx.view
        .anchored(&reference.target_vertex)
        .into_iter()
        .filter(|e| {
            key_values(ctx.view, e, target).is_some_and(|vs| same_values(&vs, key))
                && scope_of(ctx.view, e, &target.scope) == scope
        })
        .collect()
}

/// The witness is a closed walk along edges whose labels are declared
/// acyclic (consecutive edges join, the last edge returns to the first
/// edge's source), and `scc` is the strongly connected component of the
/// labelled edges that holds the walk, ascending.
fn cycle(ctx: &Context<'_>, labels: &BTreeSet<Edge>, scc: &[ViewRef], witness: &[EdgeId]) -> bool {
    let Some(g) = ctx.view.graph() else {
        return false;
    };
    let mut ends = Vec::with_capacity(witness.len());
    for id in witness {
        match g.edge(*id) {
            Some(e) if labels.contains(&e.edge) => ends.push((e.src, e.tgt)),
            _ => return false,
        }
    }
    let (Some(&(start, _)), Some(&(_, last))) = (ends.first(), ends.last()) else {
        return false;
    };
    let joined = ends.windows(2).all(|w| w[0].1 == w[1].0) && last == start;
    let labelled: Vec<(u32, u32)> = g
        .edges_sorted()
        .filter(|(_, e)| labels.contains(&e.edge))
        .map(|(_, e)| (e.src, e.tgt))
        .collect();
    let reach = |forward: bool| -> BTreeSet<u32> {
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(u) = queue.pop_front() {
            for &(s, t) in &labelled {
                let (from, to) = if forward { (s, t) } else { (t, s) };
                if from == u && seen.insert(to) {
                    queue.push_back(to);
                }
            }
        }
        seen
    };
    let component: Vec<ViewRef> = reach(true)
        .intersection(&reach(false))
        .map(|n| ViewRef::Node(*n))
        .collect();
    joined && component == scc
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use panproto_schema::{
        Cardinality, Constraint, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope,
        OnUnresolved, Protocol, RefSource, Reference, ResolveMode, Schema, Vertex,
    };

    use super::{
        CheckRun, CheckStatus, Digest, EditWitness, ExternalVerdicts, IncompleteReason, RunVerdict,
        ValidationPlan, ValidationState, ViewRef, WitnessOptions, run_verdict,
    };
    use crate::validation::{
        CheckClass, CheckInstance, CheckKey, CheckKind, SchemaRef, ValidationBudget,
        ValidationOptions, Violation,
    };
    use crate::{FieldPresence, GInstance, Node, Value};

    fn empty_state(plan: &ValidationPlan) -> ValidationState {
        ValidationState {
            plan: *plan.digest(),
            seq: 0,
            status: CheckStatus::Incomplete,
            open: BTreeMap::new(),
            budget_cuts: BTreeMap::new(),
            coverage: BTreeMap::new(),
            external: ExternalVerdicts::default(),
            key_indexes: BTreeMap::new(),
            reference_indexes: BTreeMap::new(),
        }
    }

    fn skipped_reason(
        plan: &ValidationPlan,
        state: &ValidationState,
        check: CheckKey,
    ) -> IncompleteReason {
        match run_verdict(
            plan,
            state,
            &CheckInstance {
                check,
                subject: Some(ViewRef::Node(0)),
            },
        ) {
            RunVerdict::Skipped { reason } => reason,
            RunVerdict::Evaluated { .. } => panic!("a statically incomplete check must be skipped"),
        }
    }

    #[test]
    fn static_skip_reason_belongs_to_the_scheduled_check() {
        let required = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("missing".into()),
        };
        let constraint = Constraint {
            sort: "mystery".into(),
            value: "1".into(),
        };
        let mut schema = Schema::default();
        schema.vertices.insert(
            "a".into(),
            Vertex {
                id: "a".into(),
                kind: "string".into(),
                nsid: None,
            },
        );
        schema.required.insert("a".into(), vec![required.clone()]);
        schema
            .constraints
            .insert("a".into(), vec![constraint.clone()]);
        let plan = ValidationPlan::compile(
            &schema,
            &Protocol {
                obj_kinds: vec!["string".into()],
                ..Protocol::default()
            },
            &ValidationOptions::default(),
        );
        let value_key = CheckKey::new(
            CheckKind::ValueConstraint,
            SchemaRef::Constraint {
                vertex: "a".into(),
                constraint,
            },
        );
        let required_key = CheckKey::new(
            CheckKind::RequiredEdge,
            SchemaRef::Required {
                vertex: "a".into(),
                edge: required,
            },
        );
        let expected = |key: &CheckKey| {
            plan.static_findings()
                .iter()
                .find(|finding| &finding.check == key)
                .unwrap_or_else(|| panic!("{key:?} has its own static finding"))
                .reason
                .clone()
        };
        let value_reason = expected(&value_key);
        let required_reason = expected(&required_key);
        assert_ne!(value_key.kind, required_key.kind);
        assert_ne!(value_reason, required_reason);

        let state = empty_state(&plan);
        assert_eq!(
            skipped_reason(&plan, &state, value_key),
            value_reason,
            "the value check cannot inherit the required-edge finding"
        );
        assert_eq!(
            skipped_reason(&plan, &state, required_key),
            required_reason,
            "the required-edge check cannot inherit the value finding"
        );

        let ordinary = CheckInstance {
            check: CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Schema),
            subject: Some(ViewRef::Node(0)),
        };
        assert!(matches!(
            run_verdict(&plan, &state, &ordinary),
            RunVerdict::Evaluated {
                findings,
                incomplete
            } if findings.is_empty() && incomplete.is_empty()
        ));
    }

    #[test]
    fn diagnostic_class_must_match_the_compiled_check() {
        let mut schema = Schema::default();
        schema.vertices.insert(
            "known".into(),
            Vertex {
                id: "known".into(),
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
        let graph = GInstance::new().with_node(Node::new(0, "ghost"));
        let report = plan.validate_graph(
            &graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.check.kind == CheckKind::NodeAnchor)
            .unwrap_or_else(|| panic!("the unknown anchor has a node-anchor diagnostic"));
        assert_eq!(diagnostic.class, CheckClass::GraphLocal);
        assert!(diagnostic.verify(&plan, &graph));

        let mut forged = diagnostic.clone();
        forged.class = CheckClass::MaintainedGlobal;
        assert!(!forged.verify(&plan, &graph));
    }

    #[test]
    fn outside_scope_subject_must_have_the_key_vertex_anchor() {
        let key_edge = Edge {
            src: "entry".into(),
            tgt: "key".into(),
            kind: "prop".into(),
            name: Some("key".into()),
        };
        let mut schema = Schema::default();
        for (id, kind) in [("entry", "object"), ("key", "string"), ("other", "object")] {
            schema.vertices.insert(
                id.into(),
                Vertex {
                    id: id.into(),
                    kind: kind.into(),
                    nsid: None,
                },
            );
        }
        schema.edges.insert(key_edge.clone(), key_edge.kind.clone());
        schema
            .outgoing
            .insert("entry".into(), vec![key_edge.clone()].into());
        schema
            .incoming
            .insert("key".into(), vec![key_edge.clone()].into());
        schema.between.insert(
            ("entry".into(), "key".into()),
            vec![key_edge.clone()].into(),
        );
        schema.declarations.insert_key(
            "entry".into(),
            Key {
                id: "entry_key".into(),
                parts: vec![KeyPart::Path(vec![key_edge.clone()])],
                role: KeyRole::Unique,
                scope: KeyScope::Within("map".into()),
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
        let graph = GInstance::new()
            .with_node(Node::new(0, "entry"))
            .with_node(
                Node::new(1, "key").with_value(FieldPresence::Present(Value::Str("entry".into()))),
            )
            .with_node(Node::new(2, "other"))
            .with_node(
                Node::new(3, "key").with_value(FieldPresence::Present(Value::Str("other".into()))),
            )
            .with_edge(0, 1, key_edge.clone())
            .with_edge(2, 3, key_edge);
        let report = plan.validate_graph(
            &graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.subject == Some(ViewRef::Node(0))
                    && matches!(
                        diagnostic.violation,
                        super::Violation::OutsideKeyScope { .. }
                    )
            })
            .unwrap_or_else(|| panic!("the out-of-scope keyed entry has an observation"));
        assert!(diagnostic.verify(&plan, &graph));

        let mut forged = diagnostic.clone();
        forged.subject = Some(ViewRef::Node(2));
        forged.elements = vec![ViewRef::Node(2)];
        assert!(!forged.verify(&plan, &graph));
    }

    fn ambiguous_reference_plan() -> (ValidationPlan, Edge, Edge) {
        let id_edge = Edge {
            src: "person".into(),
            tgt: "person.id".into(),
            kind: "prop".into(),
            name: Some("id".into()),
        };
        let boss_edge = Edge {
            src: "person".into(),
            tgt: "person.boss".into(),
            kind: "prop".into(),
            name: Some("boss".into()),
        };
        let mut schema = Schema::default();
        for (id, kind) in [
            ("person", "object"),
            ("person.id", "integer"),
            ("person.boss", "integer"),
        ] {
            schema.vertices.insert(
                id.into(),
                Vertex {
                    id: id.into(),
                    kind: kind.into(),
                    nsid: None,
                },
            );
        }
        for edge in [&id_edge, &boss_edge] {
            schema.edges.insert(edge.clone(), edge.kind.clone());
            schema
                .outgoing
                .entry(edge.src.clone())
                .or_default()
                .push(edge.clone());
            schema
                .incoming
                .entry(edge.tgt.clone())
                .or_default()
                .push(edge.clone());
            schema
                .between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_default()
                .push(edge.clone());
        }
        schema.declarations.insert_key(
            "person".into(),
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![id_edge.clone()])],
                role: KeyRole::Identifier,
                scope: KeyScope::Document,
            },
        );
        schema.declarations.insert_reference(Reference {
            id: "boss".into(),
            holder: "person".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![boss_edge.clone()]],
            },
            target_vertex: "person".into(),
            target_key: Some("pk".into()),
            edge: None,
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
        });
        (
            ValidationPlan::compile(
                &schema,
                &Protocol {
                    syntactic_shape: InstanceShapeKind::Graph,
                    semantic_shape: InstanceShapeKind::Graph,
                    ..Protocol::default()
                },
                &ValidationOptions::default(),
            ),
            id_edge,
            boss_edge,
        )
    }

    fn ambiguous_reference_graph(id_edge: Edge, boss_edge: Edge) -> GInstance {
        GInstance::new()
            .with_node(Node::new(0, "person"))
            .with_node(
                Node::new(1, "person.boss").with_value(FieldPresence::Present(Value::Int(10))),
            )
            .with_node(Node::new(2, "person"))
            .with_node(Node::new(3, "person.id").with_value(FieldPresence::Present(Value::Int(10))))
            .with_node(Node::new(4, "person"))
            .with_node(Node::new(5, "person.id").with_value(FieldPresence::Present(Value::Int(10))))
            .with_edge(0, 1, boss_edge)
            .with_edge(2, 3, id_edge.clone())
            .with_edge(4, 5, id_edge)
    }

    #[test]
    fn ambiguous_reference_witness_must_name_the_compiled_reference() {
        let (plan, id_edge, boss_edge) = ambiguous_reference_plan();
        let graph = ambiguous_reference_graph(id_edge, boss_edge);
        let report = plan.validate_graph(
            &graph,
            &ValidationBudget::default(),
            &ExternalVerdicts::default(),
        );
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.subject == Some(ViewRef::Node(0))
                    && matches!(diagnostic.violation, Violation::AmbiguousReference { .. })
            })
            .unwrap_or_else(|| panic!("the duplicated target key makes the reference ambiguous"));
        assert!(diagnostic.verify(&plan, &graph));

        let mut forged = diagnostic.clone();
        let Violation::AmbiguousReference { reference, .. } = &mut forged.violation else {
            unreachable!()
        };
        *reference = "other-reference".into();
        assert!(!forged.verify(&plan, &graph));
    }

    fn witness_with_subject(subject: Option<ViewRef>) -> EditWitness {
        EditWitness {
            plan: Digest([0; 32]),
            baseline_seq: 0,
            baseline_status: CheckStatus::Passed,
            delta: Digest([1; 32]),
            result_status: CheckStatus::Passed,
            runs: vec![CheckRun {
                instance: CheckInstance {
                    check: CheckKey::new(CheckKind::NodeAnchor, SchemaRef::Schema),
                    subject,
                },
                reads: Vec::new(),
                verdict: RunVerdict::Evaluated {
                    findings: Vec::new(),
                    incomplete: Vec::new(),
                },
            }],
            not_triggered: BTreeSet::new(),
            options: WitnessOptions::default(),
            prior: None,
            result: None,
        }
    }

    #[test]
    fn witness_bytes_distinguish_absent_and_distinct_subjects() {
        let absent = witness_with_subject(None).canonical_bytes();
        let node_zero = witness_with_subject(Some(ViewRef::Node(0))).canonical_bytes();
        let node_one = witness_with_subject(Some(ViewRef::Node(1))).canonical_bytes();

        assert_ne!(absent, node_zero, "the option tag is committed");
        assert_ne!(node_zero, node_one, "the subject reference is committed");
    }
}
