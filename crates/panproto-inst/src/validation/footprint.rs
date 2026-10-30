//! Footprints: the elements one check instance reads.
//!
//! One static rule per kind (Egyed 2006, stated: per rule instance, the
//! elements its evaluation read; here the kinds form a closed enum, so the
//! rule is fixed per kind). Tables read rows for nodes and foreign-key pairs
//! for edges; trees read nodes for nodes and arcs for edges.
//!
//! | kind | footprint |
//! |---|---|
//! | `NodeAnchor(n)`, `ReservedVocabulary(n)`, `Attribute(n)` | `{n}` |
//! | `EdgeDeclared(e)` | `{e}` |
//! | `EndpointCoherence(e)` | `{e, src(e), tgt(e)}` |
//! | `FanWellFormed(f)` | `{f}` and the in- and out-edges of `f`; for a tree, the parent and the children of its fans at `f` |
//! | `RequiredEdge(n)`, `OutDegree(n)`, `PositionDistinct(n)` | `{n}` and the out-edges of `n` |
//! | `ValueConstraint(n)`, `RequiredValue(n)` | `{n}` and the out-edges of `n`, whose labels decide which fields of `n` are value sites |
//! | `TreeParentMap(n)` | `{n}` and the arcs into `n` |
//! | `TreeRoot` | the declared root reference, whether or not that node exists |
//! | `KeyUnique` | every node inspected while selecting the keyed vertex, plus each selected key path |
//! | `ReferenceResolves(h)` | `{h}`, its source paths, and every node/key path inspected to build the target lookup |
//! | `Acyclic` | every edge inspected for its label, plus the endpoints of edges with an acyclic label |
//! | `TreeReachable(n)` | `n`, its ancestors and the arcs between them |
//! | `External(n)` | `{n}` and the edges incident to `n` |
//!
//! The static kinds read nothing: they were decided when the plan was
//! compiled.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::edit_error::EditError;
use crate::view_ref::ViewRef;

#[cfg(test)]
use super::checks::Context;
use panproto_gat::Name;

use super::plan::{CheckSpec, Compiled};
use super::report::CheckClass;
use super::report::{CheckInstance, CheckKind};
use super::schedule::{self, Session};
use super::state::ValidatedShape;
use super::{ExternalVerdicts, ValidationPlan};

/// The checks and carrier elements affected by one normalized edit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    /// Persistent check instances scheduled again.
    pub rerun: BTreeSet<CheckInstance>,
    /// Check instances present only after the edit.
    pub created: BTreeSet<CheckInstance>,
    /// Check instances present only before the edit.
    pub retired: BTreeSet<CheckInstance>,
    /// Union of certified pre/post dependencies for scheduled instances.
    pub reads: BTreeSet<ViewRef>,
    /// Exact carrier support named by the normalized delta.
    pub touched: BTreeSet<ViewRef>,
}

/// A transition computed over its pre- and post-state: the footprint, the
/// post-state instance, and the post-state trace of every scheduled (rerun
/// or created) check instance.
pub struct Transition<I> {
    /// The footprint.
    pub footprint: Footprint,
    /// The instance after the delta.
    pub post: I,
    /// The post-state trace of every instance of `rerun` and `created`.
    pub traces: BTreeMap<CheckInstance, BTreeSet<ViewRef>>,
}

/// Whether instances of the check are decided from their subject's
/// neighborhood (a local or external check, other than a primary-key
/// requirement reached through key paths).
fn is_local(plan: &ValidationPlan, spec: &CheckSpec) -> bool {
    matches!(
        spec.class,
        CheckClass::GraphLocal | CheckClass::BoundedExternal
    ) && !matches!(
        plan.compiled.get(&spec.key),
        Some(Compiled::RequiredValue { parts: Some(_), .. })
    )
}

/// Apply `delta` to a clone of `pre` and compute the transition footprint.
///
/// `pre_external` is the projection the pre-state was validated under,
/// `post_external` the target projection, and `formerly_skipped` the
/// instances the pre-state's budget cuts skipped.
///
/// `created` and `retired` are the exact differences of the pre- and
/// post-state check-instance sets. Whether an element is the subject of an
/// instance of a check depends on the element and its incident elements
/// only, and every element whose own data or incidence the delta changes is
/// a candidate subject, so the two sets differ only at candidate subjects;
/// they are compared there, and whole-instance checks exist on both sides.
///
/// A persistent check instance is rerun when it was formerly skipped, when
/// it is an external check whose verdict or missingness changes between
/// the projections, or when its kind is triggered by one of the delta's
/// element kinds, its subject (for a local or external check) is a
/// candidate subject, and its pre- or post-state trace reads an element the
/// delta touches. The instances of a triggered maintained-global check are
/// all considered, wherever their subject lies.
///
/// # Errors
///
/// Returns the delta's typed applicability error.
pub fn transition<I: ValidatedShape>(
    plan: &ValidationPlan,
    pre_external: &ExternalVerdicts,
    formerly_skipped: &BTreeSet<CheckInstance>,
    pre: &I,
    delta: &I::Delta,
    post_external: &ExternalVerdicts,
) -> Result<Transition<I>, EditError> {
    let mut post = pre.clone();
    I::apply_delta(delta, &mut post)?;
    let touched = I::touched(delta, pre, &post);
    let candidates = I::candidate_subjects(delta, pre, &post, &touched);
    let triggered: BTreeSet<CheckKind> = I::change_kinds(delta)
        .into_iter()
        .flat_map(|change| plan.triggers().kinds(change).into_iter().flatten().copied())
        .collect();
    let (footprint, traces) = pre.with_view(|pre_view| {
        post.with_view(|post_view| {
            Between {
                plan,
                before: &Session::new(plan, pre_view, pre_external),
                after: &Session::new(plan, post_view, post_external),
                shape: I::shape(),
                touched,
                candidates: &candidates,
                triggered: &triggered,
                formerly_skipped,
                pre_external,
                post_external,
            }
            .footprint()
        })
    });
    Ok(Transition {
        footprint,
        post,
        traces,
    })
}

/// The pre- and post-state of one transition, read through one session
/// each, with what the delta touched and triggered.
struct Between<'s, 'a> {
    plan: &'s ValidationPlan,
    before: &'s Session<'a>,
    after: &'s Session<'a>,
    shape: panproto_schema::InstanceShapeKind,
    touched: BTreeSet<ViewRef>,
    candidates: &'s BTreeSet<ViewRef>,
    triggered: &'s BTreeSet<CheckKind>,
    formerly_skipped: &'s BTreeSet<CheckInstance>,
    pre_external: &'s ExternalVerdicts,
    post_external: &'s ExternalVerdicts,
}

impl Between<'_, '_> {
    /// The footprint and the post-state trace of every scheduled instance.
    fn footprint(self) -> (Footprint, BTreeMap<CheckInstance, BTreeSet<ViewRef>>) {
        let before_at = self.before.instances_at(self.candidates);
        let after_at = self.after.instances_at(self.candidates);
        let created: BTreeSet<CheckInstance> = after_at.difference(&before_at).cloned().collect();
        let retired: BTreeSet<CheckInstance> = before_at.difference(&after_at).cloned().collect();
        let persistent: BTreeSet<CheckInstance> =
            before_at.intersection(&after_at).cloned().collect();
        let (considered, forced) = self.considered(persistent, &created);
        let mut pre_traces: BTreeMap<CheckInstance, BTreeSet<ViewRef>> = BTreeMap::new();
        let mut post_traces: BTreeMap<CheckInstance, BTreeSet<ViewRef>> = BTreeMap::new();
        let mut rerun = BTreeSet::new();
        for instance in considered {
            let Some(spec) = self.plan.checks().get(&instance.check) else {
                continue;
            };
            let is_forced = forced.contains(&instance);
            if !is_forced
                && (!self.triggered.contains(&instance.check.kind)
                    || (is_local(self.plan, spec)
                        && instance
                            .subject
                            .as_ref()
                            .is_some_and(|subject| !self.candidates.contains(subject))))
            {
                continue;
            }
            let before_reads = self.before.trace(&instance);
            let after_reads = self.after.trace(&instance);
            let hit = before_reads
                .iter()
                .chain(&after_reads)
                .any(|read| self.touched.contains(read));
            if is_forced || hit {
                pre_traces.insert(instance.clone(), before_reads);
                post_traces.insert(instance.clone(), after_reads);
                rerun.insert(instance);
            }
        }
        let mut reads = BTreeSet::new();
        for instance in &rerun {
            reads.extend(pre_traces.get(instance).into_iter().flatten().cloned());
            reads.extend(post_traces.get(instance).into_iter().flatten().cloned());
        }
        for instance in &created {
            let trace = self.after.trace(instance);
            reads.extend(trace.iter().cloned());
            post_traces.insert(instance.clone(), trace);
        }
        for instance in &retired {
            reads.extend(self.before.trace(instance));
        }
        (
            Footprint {
                rerun,
                created,
                retired,
                reads,
                touched: self.touched,
            },
            post_traces,
        )
    }

    /// Every persistent instance that may be rerun, and among them those a
    /// former budget cut or a changed external verdict forces: the
    /// persistent candidate instances, the whole-instance checks, every
    /// instance of a triggered check that is not local, and the forced
    /// instances.
    fn considered(
        &self,
        persistent: BTreeSet<CheckInstance>,
        created: &BTreeSet<CheckInstance>,
    ) -> (BTreeSet<CheckInstance>, BTreeSet<CheckInstance>) {
        let mut considered = persistent;
        for (key, spec) in self.plan.checks() {
            if !spec.applies_to(self.shape) || spec.class == CheckClass::Static {
                continue;
            }
            if schedule::is_whole_instance(key.kind) {
                considered.insert(CheckInstance {
                    check: key.clone(),
                    subject: None,
                });
            } else if self.triggered.contains(&key.kind) && !is_local(self.plan, spec) {
                considered.extend(
                    self.after
                        .instances_of(key)
                        .into_iter()
                        .filter(|instance| !created.contains(instance)),
                );
            }
        }
        let persists =
            |instance: &CheckInstance| self.before.holds(instance) && self.after.holds(instance);
        let mut forced: BTreeSet<CheckInstance> = self
            .formerly_skipped
            .iter()
            .filter(|instance| persists(instance))
            .cloned()
            .collect();
        forced.extend(
            external_changes(self.plan, self.pre_external, self.post_external)
                .filter(|instance| persists(instance)),
        );
        considered.extend(forced.iter().cloned());
        (considered, forced)
    }
}

/// Every external check instance whose verdict or missingness differs
/// between the two projections: for each `(name, element)` entry of either
/// projection that the other does not hold with the same verdict, the
/// instance of each compiled external check of that name at the element.
fn external_changes<'a>(
    plan: &'a ValidationPlan,
    before: &'a ExternalVerdicts,
    after: &'a ExternalVerdicts,
) -> impl Iterator<Item = CheckInstance> + 'a {
    let changed: BTreeSet<(&Name, &ViewRef)> = before
        .0
        .keys()
        .chain(after.0.keys())
        .filter(|(name, element)| before.get(name, element) != after.get(name, element))
        .map(|(name, element)| (name, element))
        .collect();
    changed.into_iter().flat_map(move |(name, element)| {
        plan.compiled
            .iter()
            .filter(move |(_, compiled)| {
                matches!(compiled, Compiled::External { name: n } if n == name)
            })
            .map(move |(key, _)| CheckInstance {
                check: key.clone(),
                subject: Some(element.clone()),
            })
    })
}

/// The elements the check instance `instance` reads.
#[cfg(test)]
pub fn reads(ctx: &Context<'_>, instance: &CheckInstance) -> BTreeSet<ViewRef> {
    super::checks::evaluate_traced(ctx, &instance.check, instance.subject.as_ref()).reads
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use panproto_schema::{
        Constraint, ConstraintSemantics, Edge, InstanceShapeKind, Key, KeyPart, KeyRole, KeyScope,
        Protocol, Schema, Vertex,
    };

    use super::{Between, is_local, reads};
    use crate::functor::FInstance;
    use crate::ginstance::{EdgeId, GEdge, GInstance};
    use crate::metadata::Node;
    use crate::validation::checks::{Context, InstanceView, TreeArcs, evaluate_traced};
    use crate::validation::plan::{
        ExternalCheckDecl, ExternalVerdicts, ValidationOptions, ValidationPlan,
    };
    use crate::validation::report::{
        CheckClass, CheckInstance, CheckKey, CheckKind, Requirement, SchemaRef, Violation,
    };
    use crate::view_ref::{RowId, ViewRef};
    use crate::wtype::WInstance;
    use crate::{Fan, InstError, reify_fans};

    fn e(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: Some(name.into()),
        }
    }

    fn schema() -> Schema {
        let mut s = Schema::default();
        for v in ["n", "s"] {
            s.vertices.insert(
                v.into(),
                Vertex {
                    id: v.into(),
                    kind: "object".into(),
                    nsid: None,
                },
            );
        }
        let next = e("n", "n", "ref", "next");
        let name = e("n", "s", "prop", "name");
        for edge in [&next, &name] {
            s.edges.insert(edge.clone(), edge.kind.clone());
            s.outgoing
                .entry(edge.src.clone())
                .or_default()
                .push(edge.clone());
        }
        s.orderings.insert(next, 0);
        s.required.insert("n".into(), vec![name.clone()]);
        s.constraints.insert(
            "n".into(),
            vec![Constraint {
                sort: "min_out".into(),
                value: "0".into(),
            }],
        );
        s.constraints.insert(
            "s".into(),
            vec![
                Constraint {
                    sort: "minimum".into(),
                    value: "0".into(),
                },
                Constraint {
                    sort: "required".into(),
                    value: "true".into(),
                },
            ],
        );
        s.declarations.insert_key(
            "n".into(),
            Key {
                id: "k".into(),
                parts: vec![KeyPart::Path(vec![name])],
                role: KeyRole::Unique,
                scope: KeyScope::Document,
                equality: vec![],
            },
        );
        s
    }

    fn instance(plan: &ValidationPlan, kind: CheckKind, subject: Option<ViewRef>) -> CheckInstance {
        let check = plan
            .checks()
            .keys()
            .find(|key| key.kind == kind)
            .unwrap_or_else(|| panic!("the footprint plan compiles {kind:?}"))
            .clone();
        CheckInstance { check, subject }
    }

    fn set(refs: &[ViewRef]) -> BTreeSet<ViewRef> {
        refs.iter().cloned().collect()
    }

    fn plan() -> ValidationPlan {
        let protocol = Protocol {
            constraint_sorts: vec!["min_out".into(), "minimum".into(), "required".into()],
            constraint_semantics: BTreeMap::from([
                ("min_out".into(), ConstraintSemantics::MinOutDegree),
                ("minimum".into(), ConstraintSemantics::Minimum),
                ("required".into(), ConstraintSemantics::RequiredValue),
            ]),
            ..Protocol::default()
        };
        let options = ValidationOptions {
            external: BTreeMap::from([(
                "audit".into(),
                ExternalCheckDecl {
                    name: "audit".into(),
                    anchors: BTreeSet::from(["n".into()]),
                    requirement: Requirement::Required,
                },
            )]),
            ..ValidationOptions::default()
        };
        ValidationPlan::compile(&schema(), &protocol, &options)
    }

    #[test]
    fn footprint_rules_are_the_contract_rules_per_kind() -> Result<(), InstError> {
        let plan = plan();
        let next = e("n", "n", "ref", "next");
        let graph = GInstance::new()
            .with_node(Node::new(0, "n"))
            .with_node(Node::new(1, "n"))
            .with_node(Node::new(2, "s"))
            .with_edge(0, 1, next.clone())
            .with_edge(1, 0, next)
            .with_edge(0, 2, e("n", "s", "prop", "name"));
        let graph = reify_fans(&graph, &[Fan::new("rel", 0).with_child("arg", 1)], None)?;
        let external = ExternalVerdicts::default();
        let ctx = Context::new(&plan, InstanceView::Graph(&graph), &external);
        let (n0, n2, fan) = (ViewRef::Node(0), ViewRef::Node(2), ViewRef::Node(3));
        let (e0, e1, e2, e3, e4) = (
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Edge(EdgeId(1)),
            ViewRef::Edge(EdgeId(2)),
            ViewRef::Edge(EdgeId(3)),
            ViewRef::Edge(EdgeId(4)),
        );
        let at = |kind, subject: Option<ViewRef>| reads(&ctx, &instance(&plan, kind, subject));
        assert_eq!(at(CheckKind::SchemaWellFormed, None), set(&[]));
        assert_eq!(
            at(CheckKind::NodeAnchor, Some(n0.clone())),
            set(std::slice::from_ref(&n0))
        );
        // A value check reads its subject's fields only where no out-edge
        // carries the field's edge, so it reads the out-edges too.
        for kind in [CheckKind::ValueConstraint, CheckKind::RequiredValue] {
            assert_eq!(
                at(kind, Some(n0.clone())),
                set(&[n0.clone(), e0.clone(), e2.clone()]),
                "{kind:?}"
            );
            assert_eq!(
                at(kind, Some(n2.clone())),
                set(std::slice::from_ref(&n2)),
                "{kind:?}"
            );
        }
        assert_eq!(
            at(CheckKind::Attribute, Some(n0.clone())),
            set(std::slice::from_ref(&n0))
        );
        assert_eq!(
            at(CheckKind::EdgeDeclared, Some(e0.clone())),
            set(std::slice::from_ref(&e0))
        );
        assert_eq!(
            at(CheckKind::EndpointCoherence, Some(e2.clone())),
            set(&[e2.clone(), n0.clone(), n2])
        );
        // The fan node, its parent edge and its child edge.
        assert_eq!(
            at(CheckKind::FanWellFormed, Some(fan.clone())),
            set(&[fan, e3.clone(), e4])
        );
        for kind in [
            CheckKind::RequiredEdge,
            CheckKind::OutDegree,
            CheckKind::PositionDistinct,
        ] {
            assert_eq!(
                at(kind, Some(n0.clone())),
                set(&[n0.clone(), e0.clone(), e2.clone()]),
                "{kind:?}"
            );
        }
        assert_eq!(
            at(CheckKind::External, Some(n0.clone())),
            set(&[n0, e0, e1, e2, e3])
        );
        let key_unique = CheckInstance {
            check: CheckKey::new(
                CheckKind::KeyUnique,
                SchemaRef::Key {
                    vertex: "n".into(),
                    key: "k".into(),
                },
            ),
            subject: None,
        };
        assert_eq!(
            reads(&ctx, &key_unique),
            set(&[
                ViewRef::Node(0),
                ViewRef::Node(1),
                ViewRef::Node(2),
                ViewRef::Node(3),
                ViewRef::Edge(EdgeId(0)),
                ViewRef::Edge(EdgeId(1)),
                ViewRef::Edge(EdgeId(2)),
            ])
        );
        Ok(())
    }

    #[test]
    fn graph_position_key_reads_the_parent_and_every_ordered_sibling() -> Result<(), InstError> {
        let within = e("group", "item", "ordered", "items");
        let mut schema = Schema::default();
        for vertex in ["group", "item"] {
            schema.vertices.insert(
                vertex.into(),
                Vertex {
                    id: vertex.into(),
                    kind: "object".into(),
                    nsid: None,
                },
            );
        }
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
                parts: vec![KeyPart::Position {
                    within: within.clone(),
                }],
                role: KeyRole::Primary,
                scope: KeyScope::Document,
                equality: vec![],
            },
        );
        let plan =
            ValidationPlan::compile(&schema, &Protocol::default(), &ValidationOptions::default());
        let mut graph = GInstance::new()
            .with_node(Node::new(0, "group"))
            .with_node(Node::new(1, "item"))
            .with_node(Node::new(2, "item"));
        graph.insert_edge(EdgeId(0), GEdge::new(0, 1, within.clone()))?;
        graph.insert_edge(EdgeId(1), GEdge::new(0, 2, within))?;
        let external = ExternalVerdicts::default();
        let ctx = Context::new(&plan, InstanceView::Graph(&graph), &external);
        let instance = CheckInstance {
            check: CheckKey::new(
                CheckKind::RequiredValue,
                SchemaRef::Key {
                    vertex: "item".into(),
                    key: "position".into(),
                },
            ),
            subject: Some(ViewRef::Node(1)),
        };
        assert!(!plan.is_statically_incomplete(&instance.check));
        let spec = plan
            .checks()
            .get(&instance.check)
            .unwrap_or_else(|| panic!("the position key compiles its required-value check"));
        assert!(!is_local(&plan, spec));
        assert_eq!(
            reads(&ctx, &instance),
            set(&[
                ViewRef::Node(0),
                ViewRef::Node(1),
                ViewRef::Edge(EdgeId(0)),
                ViewRef::Edge(EdgeId(1)),
            ])
        );
        Ok(())
    }

    #[test]
    fn locality_classifier_distinguishes_neighborhood_checks() {
        let plan = plan();
        let local = plan
            .checks()
            .values()
            .find(|spec| spec.class == CheckClass::GraphLocal)
            .unwrap_or_else(|| panic!("the footprint plan compiles a graph-local check"));
        let external = plan
            .checks()
            .values()
            .find(|spec| spec.class == CheckClass::BoundedExternal)
            .unwrap_or_else(|| panic!("the footprint plan compiles a bounded-external check"));
        let global = plan
            .checks()
            .values()
            .find(|spec| spec.class == CheckClass::MaintainedGlobal)
            .unwrap_or_else(|| panic!("the footprint plan compiles a maintained-global check"));

        assert!(is_local(&plan, local));
        assert!(is_local(&plan, external));
        assert!(!is_local(&plan, global));
    }

    #[test]
    fn local_triggers_stay_candidate_scoped_and_untriggered_checks_do_not_rerun() {
        let plan = plan();
        let name = e("n", "s", "prop", "name");
        let graph = GInstance::new()
            .with_node(Node::new(0, "n"))
            .with_node(Node::new(1, "s"))
            .with_edge(0, 1, name);
        let external = ExternalVerdicts::default();
        let before = super::Session::new(&plan, InstanceView::Graph(&graph), &external);
        let after = super::Session::new(&plan, InstanceView::Graph(&graph), &external);
        let touched = set(&[ViewRef::Node(0)]);
        let candidates = BTreeSet::new();
        let local_trigger = BTreeSet::from([CheckKind::NodeAnchor]);
        let no_triggers = BTreeSet::new();
        let formerly_skipped = BTreeSet::new();
        let created = BTreeSet::new();

        let between = |touched, triggered| Between {
            plan: &plan,
            before: &before,
            after: &after,
            shape: InstanceShapeKind::Graph,
            touched,
            candidates: &candidates,
            triggered,
            formerly_skipped: &formerly_skipped,
            pre_external: &external,
            post_external: &external,
        };
        let (considered, forced) =
            between(touched.clone(), &local_trigger).considered(BTreeSet::new(), &created);
        let expected: BTreeSet<CheckInstance> = plan
            .checks()
            .iter()
            .filter(|(key, spec)| {
                spec.applies_to(InstanceShapeKind::Graph)
                    && spec.class != CheckClass::Static
                    && super::schedule::is_whole_instance(key.kind)
            })
            .map(|(key, _)| CheckInstance {
                check: key.clone(),
                subject: None,
            })
            .collect();
        assert!(!expected.is_empty());
        assert_eq!(considered, expected);
        assert!(forced.is_empty());

        let (footprint, traces) = between(touched.clone(), &no_triggers).footprint();
        assert!(footprint.rerun.is_empty());
        assert!(footprint.created.is_empty());
        assert!(footprint.retired.is_empty());
        assert!(footprint.reads.is_empty());
        assert_eq!(footprint.touched, touched);
        assert!(traces.is_empty());
    }

    #[test]
    fn missing_tree_root_trace_names_the_absent_root() {
        let plan = plan();
        let tree = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 42, "n".into());
        let arcs = TreeArcs::new(&tree);
        let external = ExternalVerdicts::default();
        let ctx = Context::new(&plan, InstanceView::Tree(&tree, &arcs), &external);
        let key = CheckKey::new(CheckKind::TreeRoot, SchemaRef::Schema);
        let evaluation = evaluate_traced(&ctx, &key, None);
        assert_eq!(evaluation.reads, set(&[ViewRef::Node(42)]));
        assert!(
            evaluation
                .outcome
                .findings
                .iter()
                .any(|finding| finding.violation == Violation::MissingRoot)
        );
    }

    #[test]
    fn own_value_constraint_does_not_read_unrelated_out_edges() -> Result<(), InstError> {
        let link = e("item", "item", "link", "next");
        let mut schema = Schema::default();
        schema.vertices.insert(
            "item".into(),
            Vertex {
                id: "item".into(),
                kind: "object".into(),
                nsid: None,
            },
        );
        schema.edges.insert(link.clone(), link.kind.clone());
        schema
            .outgoing
            .entry(link.src.clone())
            .or_default()
            .push(link.clone());
        schema.constraints.insert(
            "item".into(),
            vec![Constraint {
                sort: "minimum".into(),
                value: "0".into(),
            }],
        );
        let plan = ValidationPlan::compile(
            &schema,
            &Protocol {
                constraint_sorts: vec!["minimum".into()],
                constraint_semantics: BTreeMap::from([(
                    "minimum".into(),
                    ConstraintSemantics::Minimum,
                )]),
                ..Protocol::default()
            },
            &ValidationOptions::default(),
        );
        let mut graph = GInstance::new()
            .with_node(
                Node::new(0, "item").with_value(crate::value::FieldPresence::Present(
                    crate::value::Value::Int(1),
                )),
            )
            .with_node(Node::new(1, "item"))
            .with_node(Node::new(2, "item"));
        graph.insert_edge(EdgeId(0), GEdge::new(0, 1, link.clone()))?;
        graph.insert_edge(EdgeId(1), GEdge::new(0, 2, link))?;
        let external = ExternalVerdicts::default();
        let ctx = Context::new(&plan, InstanceView::Graph(&graph), &external);
        let key = plan
            .checks()
            .keys()
            .find(|key| key.kind == CheckKind::ValueConstraint)
            .unwrap_or_else(|| panic!("minimum compiles a value check"))
            .clone();
        let evaluation = evaluate_traced(&ctx, &key, Some(&ViewRef::Node(0)));
        assert_eq!(evaluation.reads, set(&[ViewRef::Node(0)]));
        assert!(evaluation.outcome.findings.is_empty());
        Ok(())
    }

    /// Trees read nodes for nodes and arcs for edges.
    #[test]
    fn tree_footprints_read_nodes_and_arcs() {
        let plan = plan();
        let external = ExternalVerdicts::default();
        let (n0, n1, n2) = (ViewRef::Node(0), ViewRef::Node(1), ViewRef::Node(2));
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "n")),
                (1, Node::new(1, "n")),
                (2, Node::new(2, "s")),
            ]),
            vec![
                (0, 1, e("n", "n", "ref", "next")),
                (1, 2, e("n", "s", "prop", "name")),
            ],
            vec![Fan::new("rel", 1).with_child("arg", 2)],
            0,
            "n".into(),
        );
        let arcs = TreeArcs::new(&tree);
        let ctx = Context::new(&plan, InstanceView::Tree(&tree, &arcs), &external);
        let at = |kind, subject: Option<ViewRef>| reads(&ctx, &instance(&plan, kind, subject));
        let a01 = ViewRef::Arc {
            parent: 0,
            child: 1,
        };
        let a12 = ViewRef::Arc {
            parent: 1,
            child: 2,
        };
        assert_eq!(
            at(CheckKind::TreeParentMap, Some(n2.clone())),
            set(&[n2.clone(), a12.clone()])
        );
        for kind in [CheckKind::ValueConstraint, CheckKind::RequiredValue] {
            assert_eq!(
                at(kind, Some(n1.clone())),
                set(&[n1.clone(), a12.clone()]),
                "{kind:?}"
            );
        }
        assert_eq!(
            at(CheckKind::TreeRoot, None),
            set(std::slice::from_ref(&n0))
        );
        assert_eq!(
            at(CheckKind::TreeReachable, Some(n2.clone())),
            set(&[n2.clone(), a12, n1.clone(), a01, n0])
        );
        assert_eq!(
            at(CheckKind::FanWellFormed, Some(n1.clone())),
            set(&[n1, n2])
        );
    }

    /// Tables read rows for nodes and foreign-key pairs for edges.
    #[test]
    fn table_footprints_read_rows_and_pairs() -> Result<(), InstError> {
        let plan = plan();
        let external = ExternalVerdicts::default();
        let fk = e("n", "n", "ref", "next");
        let mut table = FInstance::new();
        table.insert_row("n", RowId(0), HashMap::new())?;
        table.insert_row("n", RowId(1), HashMap::new())?;
        table.insert_foreign_key(fk.clone(), RowId(0), RowId(1))?;
        let ctx = Context::new(&plan, InstanceView::Table(&table), &external);
        let pair = ViewRef::Pair {
            edge: fk,
            src: RowId(0),
            tgt: RowId(1),
        };
        for kind in [
            CheckKind::OutDegree,
            CheckKind::ValueConstraint,
            CheckKind::RequiredValue,
        ] {
            assert_eq!(
                reads(&ctx, &instance(&plan, kind, Some(ViewRef::Row(RowId(0)))),),
                set(&[ViewRef::Row(RowId(0)), pair.clone()]),
                "{kind:?}"
            );
            assert_eq!(
                reads(&ctx, &instance(&plan, kind, Some(ViewRef::Row(RowId(1)))),),
                set(&[ViewRef::Row(RowId(1)), pair.clone()]),
                "{kind:?}"
            );
        }
        assert_eq!(
            reads(
                &ctx,
                &instance(&plan, CheckKind::EndpointCoherence, Some(pair.clone()),)
            ),
            set(&[pair, ViewRef::Row(RowId(0)), ViewRef::Row(RowId(1))])
        );
        Ok(())
    }
}
