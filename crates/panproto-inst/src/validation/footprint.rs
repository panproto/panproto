//! Footprints: the elements one check instance reads.
//!
//! One static rule per kind (Egyed 2006, stated: per rule instance, the
//! elements its evaluation read; here the kinds form a closed enum, so the
//! rule is fixed per kind). Tables read rows for nodes and foreign-key pairs
//! for edges; trees read nodes for nodes and arcs for edges.
//!
//! | kind | footprint |
//! |---|---|
//! | `NodeAnchor(n)`, `ReservedVocabulary(n)`, `ValueConstraint(n)`, `RequiredValue(n)`, `Attribute(n)` | `{n}` |
//! | `EdgeDeclared(e)` | `{e}` |
//! | `EndpointCoherence(e)` | `{e, src(e), tgt(e)}` |
//! | `FanWellFormed(f)` | `{f}` and the in- and out-edges of `f`; for a tree, the parent and the children of its fans at `f` |
//! | `RequiredEdge(n)`, `OutDegree(n)`, `PositionDistinct(n)` | `{n}` and the out-edges of `n` |
//! | `TreeParentMap(n)` | `{n}` and the arcs into `n` |
//! | `TreeRoot` | the root, when it is a node |
//! | `KeyUnique` | every element anchored at the keyed vertex |
//! | `ReferenceResolves(h)` | `{h}` and the elements of the index buckets its key values name |
//! | `Acyclic` | every edge with an acyclic label and its endpoints |
//! | `TreeReachable(n)` | `n`, its ancestors and the arcs between them |
//! | `External(n)` | `{n}` and the edges incident to `n` |
//!
//! The static kinds read nothing: they were decided when the plan was
//! compiled.

use std::collections::BTreeSet;

use crate::view_ref::ViewRef;

use super::checks::{Context, InstanceView};
use super::plan::Compiled;
use super::report::{CheckInstance, CheckKind};

/// The elements the check instance `instance` reads.
pub fn reads(ctx: &Context<'_>, instance: &CheckInstance) -> BTreeSet<ViewRef> {
    let view = ctx.view;
    let key = &instance.check;
    let mut out = BTreeSet::new();
    match (key.kind, instance.subject.as_ref()) {
        (
            CheckKind::NodeAnchor
            | CheckKind::ReservedVocabulary
            | CheckKind::EdgeDeclared
            | CheckKind::ValueConstraint
            | CheckKind::RequiredValue
            | CheckKind::Attribute,
            Some(n),
        ) => {
            out.insert(n.clone());
        }
        (CheckKind::EndpointCoherence, Some(e)) => {
            out.insert(e.clone());
            for rec in super::checks::edges_at(view, e) {
                out.insert(rec.src);
                out.insert(rec.tgt);
            }
        }
        (CheckKind::FanWellFormed, Some(f)) => fan_reads(view, f, &mut out),
        (CheckKind::RequiredEdge | CheckKind::OutDegree | CheckKind::PositionDistinct, Some(n)) => {
            out.insert(n.clone());
            out.extend(view.out_edges(n).into_iter().map(|e| e.at));
        }
        (CheckKind::TreeParentMap, Some(n)) => {
            out.insert(n.clone());
            out.extend(view.in_edges(n).into_iter().map(|e| e.at));
        }
        (CheckKind::TreeRoot, None) => {
            if let Some(t) = view.tree()
                && t.nodes.contains_key(&t.root)
            {
                out.insert(ViewRef::Node(t.root));
            }
        }
        (CheckKind::KeyUnique, None) => {
            if let Some(Compiled::KeyUnique { vertex, .. }) = ctx.plan.compiled.get(key) {
                out.extend(view.anchored(vertex));
            }
        }
        (CheckKind::ReferenceResolves, Some(h)) => {
            out.insert(h.clone());
            out.extend(super::checks::reference_reads(ctx, h, key));
        }
        (CheckKind::Acyclic, None) => {
            if let Some(Compiled::Acyclic { labels }) = ctx.plan.compiled.get(key) {
                for rec in view
                    .edges()
                    .into_iter()
                    .filter(|rec| labels.contains(rec.label))
                {
                    out.insert(rec.at);
                    out.insert(rec.src);
                    out.insert(rec.tgt);
                }
            }
        }
        (CheckKind::TreeReachable, Some(n)) => ancestor_reads(view, n, &mut out),
        (CheckKind::External, Some(n)) => {
            out.insert(n.clone());
            out.extend(view.out_edges(n).into_iter().map(|e| e.at));
            out.extend(view.in_edges(n).into_iter().map(|e| e.at));
        }
        _ => {}
    }
    out
}

/// A graph fan node and its incident edges; a tree's fan parent and the
/// children of its fans there.
fn fan_reads(view: InstanceView<'_>, f: &ViewRef, out: &mut BTreeSet<ViewRef>) {
    out.insert(f.clone());
    if let InstanceView::Tree(t, _) = view {
        for fan in t.fans.iter().filter(|fan| ViewRef::Node(fan.parent) == *f) {
            out.extend(fan.children.values().map(|c| ViewRef::Node(*c)));
        }
    } else {
        out.extend(view.out_edges(f).into_iter().map(|e| e.at));
        out.extend(view.in_edges(f).into_iter().map(|e| e.at));
    }
}

/// `n`, its ancestors and the arcs between them, up to the root or the
/// first node met twice.
fn ancestor_reads(view: InstanceView<'_>, n: &ViewRef, out: &mut BTreeSet<ViewRef>) {
    let mut at = n.clone();
    out.insert(at.clone());
    while let Some(arc) = view.in_edges(&at).into_iter().next() {
        out.insert(arc.at);
        if !out.insert(arc.src.clone()) {
            break;
        }
        at = arc.src;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, Vertex};

    use super::reads;
    use crate::functor::FInstance;
    use crate::ginstance::{EdgeId, GInstance};
    use crate::metadata::Node;
    use crate::validation::checks::{Context, InstanceView, TreeArcs};
    use crate::validation::plan::{ExternalVerdicts, ValidationOptions, ValidationPlan};
    use crate::validation::report::{CheckInstance, CheckKey, CheckKind, SchemaRef};
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
        s.required.insert("n".into(), vec![name.clone()]);
        s.declarations.insert_key(
            "n".into(),
            Key {
                id: "k".into(),
                parts: vec![KeyPart::Path(vec![name])],
                role: KeyRole::Unique,
                scope: KeyScope::Document,
            },
        );
        s
    }

    fn instance(kind: CheckKind, subject: Option<ViewRef>) -> CheckInstance {
        CheckInstance {
            check: CheckKey::new(kind, SchemaRef::Schema),
            subject,
        }
    }

    fn set(refs: &[ViewRef]) -> BTreeSet<ViewRef> {
        refs.iter().cloned().collect()
    }

    fn plan() -> ValidationPlan {
        ValidationPlan::compile(
            &schema(),
            &Protocol::default(),
            &ValidationOptions::default(),
        )
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
        let (n0, n1, n2, fan) = (
            ViewRef::Node(0),
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Node(3),
        );
        let (e0, e1, e2, e3, e4) = (
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Edge(EdgeId(1)),
            ViewRef::Edge(EdgeId(2)),
            ViewRef::Edge(EdgeId(3)),
            ViewRef::Edge(EdgeId(4)),
        );
        let at = |kind, subject: Option<ViewRef>| reads(&ctx, &instance(kind, subject));
        assert_eq!(at(CheckKind::SchemaWellFormed, None), set(&[]));
        assert_eq!(
            at(CheckKind::NodeAnchor, Some(n0.clone())),
            set(std::slice::from_ref(&n0))
        );
        assert_eq!(
            at(CheckKind::ValueConstraint, Some(n2.clone())),
            set(std::slice::from_ref(&n2))
        );
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
            set(&[n0.clone(), e0, e1, e2, e3])
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
        assert_eq!(reads(&ctx, &key_unique), set(&[n0, n1]));
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
        let at = |kind, subject: Option<ViewRef>| reads(&ctx, &instance(kind, subject));
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
        assert_eq!(
            reads(
                &ctx,
                &instance(CheckKind::OutDegree, Some(ViewRef::Row(RowId(0))))
            ),
            set(&[ViewRef::Row(RowId(0)), pair.clone()])
        );
        assert_eq!(
            reads(
                &ctx,
                &instance(CheckKind::EndpointCoherence, Some(pair.clone()))
            ),
            set(&[pair, ViewRef::Row(RowId(0)), ViewRef::Row(RowId(1))])
        );
        Ok(())
    }
}
