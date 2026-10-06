//! Canonical put: a source for a view without a complement.
//!
//! For a tree source the root is the least node anchored at the first
//! syntactic entry vertex that has one. The spanning arborescence is
//! discovered depth first from the root, exploring each node's outgoing
//! structural edges in ascending `(label, target id, edge id)` order: an
//! edge into an undiscovered node is a tree arc, so every node's structural
//! embedding precedes, in document order, every reference to it. Every
//! other edge is written by its declared writer: a redirecting writer adds
//! a holder child at the edge's place among its source's children, an
//! adding writer's field is already in the view. For a table source the
//! edges with a writer are references and the rest must read back as a
//! table instance. In both kinds the candidate is resolved again, its view
//! must be the requested view up to the presentation of synthesized
//! carriers, and its complement is renamed to the requested ids.

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::resolution_complement::ResolutionComplement;
use panproto_inst::wtype_check::is_reserved_edge_kind;
use panproto_inst::{
    AnyInstance, EdgeId, GInstance, Node, NodeShape, SynthIndex, ViewRef, WInstance,
};
use panproto_schema::{Edge, ResolveMode};

use super::build::{FormChoice, write_literal};
use super::compile::RefForm;
use super::error::{ResolutionInstanceError, ResolveDomainError, ResolveError};
use super::get::instance_error;
use super::occur::tuple_literal;
use super::put::{Refusal, refuse};
use super::rekey::Renaming;
use super::source::Source;
use super::verify::{Diff, extend_index, view_diff};
use super::{Resolution, ResolveContext, ResolveOptions};

const fn domain(error: ResolveDomainError) -> ResolveError {
    ResolveError::PutDomain(error)
}

/// An outgoing edge in exploration order: its label, its target and its id.
type Outgoing = (Edge, u32, EdgeId);

/// What [`Resolution::plan_tree`] finds: each tree node's ordered children,
/// the fans kept, and the redirecting edges that need a holder under their
/// source.
struct TreePlan {
    children: BTreeMap<u32, Vec<(Outgoing, Child)>>,
    kept_fans: BTreeSet<u32>,
    holder_edges: Vec<(u32, Outgoing)>,
}

/// One child slot of a parent in canonical order: a tree arc into a
/// discovered node, or the holder of a non-spanning redirecting edge with
/// the id it is given.
enum Child {
    Arc(u32, Edge),
    Holder(EdgeId, u32),
}

impl Resolution {
    /// Canonical put for a resolving kind.
    pub(super) fn put_canonical_graph(
        &self,
        view: &AnyInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<(AnyInstance, ResolutionComplement), ResolveError> {
        let AnyInstance::Graph(graph) = view else {
            return Err(ResolveError::ShapeMismatch {
                expected: self.spec.kind.semantic_shape(),
                found: view.shape(),
            });
        };
        let source = match self.spec.kind {
            panproto_inst::ResolutionKind::TreeToGraph => {
                AnyInstance::Tree(self.canonical_tree(graph, ctx)?)
            }
            _ => AnyInstance::Table(self.canonical_table(graph)?),
        };
        let (regotten, complement) = self.get(&source, &ResolveOptions::default(), ctx)?;
        let AnyInstance::Graph(regotten) = regotten else {
            return Err(ResolveError::Instance(ResolutionInstanceError::NotGraph));
        };
        let reindex = complement.synth_index();
        let index = extend_index(graph, &SynthIndex::default(), &regotten, &reindex);
        if let Some(diff) = view_diff(graph, &index, &regotten, &reindex)
            .into_iter()
            .next()
        {
            return Err(domain(self.canonical_violation(graph, diff)));
        }
        let renamed = Renaming::between(&reindex, &index).complement(&complement);
        Ok((source, renamed))
    }

    /// The violation a disagreement between the requested view and the
    /// candidate's view stands for.
    fn canonical_violation(&self, graph: &GInstance, diff: Diff) -> ResolveDomainError {
        match diff {
            Diff::LeftNode(id) | Diff::RightNode(id) => ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(id),
            },
            Diff::LeftEdge { id, .. } => {
                match graph.edge(id).map(|edge| self.writer(id, &edge.edge)) {
                    Some(Ok(writer)) => ResolveDomainError::UnrepresentableReference {
                        carrier: ViewRef::Edge(id),
                        reference: writer.reference.id.clone(),
                    },
                    Some(Err(violation)) => violation,
                    None => ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Edge(id),
                    },
                }
            }
            Diff::RightEdge(id) => ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(id),
            },
        }
    }

    /// The canonical tree of a view.
    fn canonical_tree(
        &self,
        graph: &GInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<WInstance, ResolveError> {
        if graph.node_count() == 0 {
            return Err(ResolveError::Instance(ResolutionInstanceError::NotTree));
        }
        let fan_nodes: BTreeSet<u32> = graph
            .nodes()
            .iter()
            .filter(|(_, node)| node.shape == NodeShape::HyperEdge)
            .map(|(&id, _)| id)
            .collect();
        let root = self
            .syntactic_schema
            .entry_vertices()
            .iter()
            .find_map(|entry| {
                graph
                    .nodes()
                    .iter()
                    .filter(|(id, node)| node.anchor == *entry && !fan_nodes.contains(id))
                    .map(|(&id, _)| id)
                    .min()
            })
            .map_or_else(
                || {
                    graph.nodes().keys().copied().min().map_or(
                        Err(ResolveError::Instance(ResolutionInstanceError::NotTree)),
                        |least| {
                            Err(domain(ResolveDomainError::NoPlacement {
                                carrier: ViewRef::Node(least),
                            }))
                        },
                    )
                },
                Ok,
            )?;
        let structural = |edge: &Edge| self.spec.structural_edges.contains(edge);
        let outgoing = |node: u32| -> Vec<Outgoing> {
            let mut edges: Vec<Outgoing> = graph
                .out_edges(node)
                .iter()
                .filter_map(|&id| graph.edge(id).map(|edge| (edge.edge.clone(), edge.tgt, id)))
                .filter(|(label, target, _)| structural(label) && !fan_nodes.contains(target))
                .collect();
            edges.sort();
            edges
        };
        let mut discovered = BTreeSet::from([root]);
        let mut tree_arcs: BTreeSet<EdgeId> = BTreeSet::new();
        let mut stack: Vec<(Vec<Outgoing>, usize)> = vec![(outgoing(root), 0)];
        while let Some((edges, next)) = stack.pop() {
            let Some((_, target, id)) = edges.get(next).cloned() else {
                continue;
            };
            stack.push((edges, next + 1));
            if discovered.insert(target) {
                tree_arcs.insert(id);
                stack.push((outgoing(target), 0));
            }
        }
        self.lay_out_tree(graph, root, &discovered, &tree_arcs, &fan_nodes, ctx)
    }
}

impl Resolution {
    fn canonical_table(&self, graph: &GInstance) -> Result<panproto_inst::FInstance, ResolveError> {
        let mut structural = GInstance::new();
        let span_vertices: BTreeSet<&panproto_gat::Name> = self
            .compiled
            .values()
            .filter(|compiled| matches!(compiled.form, RefForm::Standoff { key: None, .. }))
            .map(|compiled| &compiled.reference.target_vertex)
            .collect();
        for (id, node) in graph.nodes() {
            let span = span_vertices.contains(&node.anchor)
                && graph.out_edges(*id).is_empty()
                && graph.in_edges(*id).iter().all(|edge| {
                    graph
                        .edge(*edge)
                        .is_some_and(|edge| self.spec.reference_writers.contains_key(&edge.edge))
                })
                && !graph.in_edges(*id).is_empty();
            if !span {
                structural
                    .insert_node(node.clone())
                    .map_err(|error| instance_error(&error))?;
            }
        }
        for (id, edge) in graph.edges_sorted() {
            if is_reserved_edge_kind(&edge.edge.kind) {
                return Err(domain(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Edge(id),
                }));
            }
            if self.spec.reference_writers.contains_key(&edge.edge) {
                continue;
            }
            structural
                .insert_edge(id, edge.clone())
                .map_err(|error| instance_error(&error))?;
        }
        match super::build::read_table(&structural) {
            Ok(table) => Ok(table),
            Err(super::put::Refusal::Domain(violations)) => Err(domain(violations.first)),
            Err(super::put::Refusal::Operational(error)) => Err(error),
        }
    }
}

impl Resolution {
    /// The children of each tree node and the redirecting edges that need a
    /// holder, after checking that every fan, edge and node of `graph` has
    /// a place in the tree discovered from the root.
    fn plan_tree(
        &self,
        graph: &GInstance,
        discovered: &BTreeSet<u32>,
        tree_arcs: &BTreeSet<EdgeId>,
        fan_nodes: &BTreeSet<u32>,
    ) -> Result<TreePlan, ResolveError> {
        let mut violations = Vec::new();
        let mut children: BTreeMap<u32, Vec<(Outgoing, Child)>> = BTreeMap::new();
        let mut kept_fans = BTreeSet::new();
        for &fan in fan_nodes {
            let touches_all = graph
                .out_edges(fan)
                .iter()
                .filter_map(|id| graph.edge(*id))
                .all(|edge| discovered.contains(&edge.tgt));
            if touches_all && graph.in_edges(fan).is_empty() {
                kept_fans.insert(fan);
            } else {
                violations.push(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Node(fan),
                });
            }
        }
        let mut targeted = BTreeSet::new();
        let mut holder_edges = Vec::new();
        for (id, edge) in graph.edges_sorted() {
            if is_reserved_edge_kind(&edge.edge.kind) {
                continue;
            }
            if tree_arcs.contains(&id) {
                children.entry(edge.src).or_default().push((
                    (edge.edge.clone(), edge.tgt, id),
                    Child::Arc(edge.tgt, edge.edge.clone()),
                ));
                continue;
            }
            let writer = match self.writer(id, &edge.edge) {
                Ok(writer) => writer,
                Err(violation) => {
                    violations.push(violation);
                    continue;
                }
            };
            targeted.insert(edge.tgt);
            if writer.reference.mode != ResolveMode::Redirect {
                continue;
            }
            let representable = discovered.contains(&edge.src)
                && discovered.contains(&edge.tgt)
                && !matches!(writer.form, RefForm::Standoff { .. });
            if representable {
                holder_edges.push((edge.src, (edge.edge.clone(), edge.tgt, id)));
            } else {
                violations.push(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(id),
                    reference: writer.reference.id.clone(),
                });
            }
        }
        for (&id, node) in graph.nodes() {
            let placed = discovered.contains(&id) || fan_nodes.contains(&id);
            let span = targeted.contains(&id) && self.is_span_node(graph, id, node);
            if !placed && !span {
                violations.push(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Node(id),
                });
            }
        }
        super::sort_domain_errors(&mut violations);
        if let Some(first) = violations.into_iter().next() {
            return Err(domain(first));
        }
        Ok(TreePlan {
            children,
            kept_fans,
            holder_edges,
        })
    }

    /// The canonical tree: the discovered nodes under their tree arcs and
    /// the redirect holders of every non-spanning redirecting edge, each
    /// parent's children in ascending `(label, target id, edge id)` order,
    /// arcs in document order.
    fn lay_out_tree(
        &self,
        graph: &GInstance,
        root: u32,
        discovered: &BTreeSet<u32>,
        tree_arcs: &BTreeSet<EdgeId>,
        fan_nodes: &BTreeSet<u32>,
        ctx: &ResolveContext<'_>,
    ) -> Result<WInstance, ResolveError> {
        let TreePlan {
            mut children,
            kept_fans,
            holder_edges,
        } = self.plan_tree(graph, discovered, tree_arcs, fan_nodes)?;
        let mut next_id = discovered
            .iter()
            .max()
            .and_then(|max| max.checked_add(1))
            .ok_or(ResolveError::IdExhausted {
                kind: panproto_inst::segment::IdKind::Node,
            })?;
        let mut holders = Vec::new();
        for (parent, key) in holder_edges {
            let id = next_id;
            next_id = next_id.checked_add(1).ok_or(ResolveError::IdExhausted {
                kind: panproto_inst::segment::IdKind::Node,
            })?;
            holders.push((key.2, id, key.1));
            children
                .entry(parent)
                .or_default()
                .push((key.clone(), Child::Holder(key.2, id)));
        }
        for entries in children.values_mut() {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
        }

        let mut structural = GInstance::new();
        for id in discovered.iter().chain(&kept_fans) {
            if let Some(node) = graph.node(*id) {
                structural
                    .insert_node(node.clone())
                    .map_err(|error| instance_error(&error))?;
            }
        }
        for (id, edge) in graph.edges_sorted() {
            let fan_edge = is_reserved_edge_kind(&edge.edge.kind) && kept_fans.contains(&edge.src);
            if tree_arcs.contains(&id) || fan_edge {
                structural
                    .insert_edge(id, edge.clone())
                    .map_err(|error| instance_error(&error))?;
            }
        }
        let (base, _) = structural
            .to_wtype_embedded(&self.syntactic_schema)
            .map_err(|error| instance_error(&error))?;

        let mut nodes = base.nodes.clone();
        let mut arcs = Vec::new();
        let mut stack: Vec<(u32, usize)> = vec![(root, 0)];
        while let Some((node, next)) = stack.pop() {
            let Some((_, child)) = children.get(&node).and_then(|entries| entries.get(next)) else {
                continue;
            };
            stack.push((node, next + 1));
            match child {
                Child::Arc(target, label) => {
                    arcs.push((node, *target, label.clone()));
                    stack.push((*target, 0));
                }
                Child::Holder(edge, id) => {
                    let (writer, label) = self.holder_writer(graph, *edge)?;
                    nodes.insert(*id, Node::new(*id, writer.reference.holder.clone()));
                    arcs.push((node, *id, label));
                }
            }
        }
        let anchor = nodes
            .get(&root)
            .map(|node| node.anchor.clone())
            .ok_or(ResolveError::Instance(ResolutionInstanceError::NotTree))?;
        let tree = WInstance::new(nodes, arcs, base.fans.clone(), root, anchor);
        let mut written = Vec::new();
        for (edge, id, target) in holders {
            let (writer, _) = self.holder_writer(graph, edge)?;
            let first = self
                .canonical_literal(writer, id, target, &base, graph, ctx)
                .map_err(|error| match error {
                    ResolveError::PutDomain(ResolveDomainError::UnrepresentableReference {
                        reference,
                        ..
                    }) => domain(ResolveDomainError::UnrepresentableReference {
                        carrier: ViewRef::Edge(edge),
                        reference,
                    }),
                    other => other,
                })?;
            let mut forms = vec![first];
            if matches!(writer.form, RefForm::KeyValue { .. } | RefForm::Path { .. }) {
                forms.extend(self.provider.qualified_forms(
                    &writer.reference,
                    ViewRef::Node(id),
                    ViewRef::Node(target),
                    &forms[0],
                    &AnyInstance::Graph(graph.clone()),
                    ctx,
                )?);
            }
            written.push((edge, id, writer, target, forms));
        }
        let choices: Vec<FormChoice> = written
            .iter()
            .map(|(edge, id, writer, target, forms)| FormChoice {
                carrier: *edge,
                key: (ViewRef::Node(*id), writer.reference.id.clone(), 0),
                target: *target,
                forms: forms.len(),
                checked: true,
            })
            .collect();
        let build = |chosen: &[usize]| -> Result<WInstance, Refusal> {
            let mut candidate = tree.clone();
            let mut failures = Vec::new();
            for ((edge, id, writer, _, forms), choice) in written.iter().zip(chosen) {
                let written = forms
                    .get(*choice)
                    .is_some_and(|literal| write_literal(&mut candidate, *id, writer, 0, literal).is_ok());
                if !written {
                    failures.push(ResolveDomainError::UnrepresentableReference {
                        carrier: ViewRef::Edge(*edge),
                        reference: writer.reference.id.clone(),
                    });
                }
            }
            refuse(failures)?;
            Ok(candidate)
        };
        match self.choose(&choices, &build, ctx) {
            Ok(tree) => Ok(tree),
            Err(Refusal::Domain(violations)) => Err(domain(violations.first)),
            Err(Refusal::Operational(error)) => Err(error),
        }
    }
}

impl Resolution {
    /// Whether an unplaced node is a span node: a node with no data and no
    /// out-edges, anchored at the target vertex of a materializing standoff
    /// reference, every in-edge of which is written by such a reference.
    fn is_span_node(&self, graph: &GInstance, id: u32, node: &Node) -> bool {
        let materializing = |label: &Edge| {
            self.spec
                .reference_writers
                .get(label)
                .and_then(|name| self.compiled.get(name))
                .is_some_and(|compiled| {
                    matches!(compiled.form, RefForm::Standoff { key: None, .. })
                        && compiled.reference.target_vertex == node.anchor
                })
        };
        graph.out_edges(id).is_empty()
            && super::verify::node_data_equiv(node, &Node::new(id, node.anchor.clone()))
            && graph.in_edges(id).iter().all(|edge| {
                graph
                    .edge(*edge)
                    .is_some_and(|edge| materializing(&edge.edge))
            })
    }

    /// The writer of a non-spanning redirecting edge and the arc label its
    /// holder hangs from: the one syntactic edge from the source's anchor
    /// to the writer's holder vertex.
    fn holder_writer(
        &self,
        graph: &GInstance,
        edge: EdgeId,
    ) -> Result<(&super::compile::CompiledRef, Edge), ResolveError> {
        let no_placement = || {
            domain(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(edge),
            })
        };
        let found = graph.edge(edge).ok_or_else(no_placement)?;
        let writer = self.writer(edge, &found.edge).map_err(domain)?;
        let anchor = graph
            .node(found.src)
            .map(|node| node.anchor.clone())
            .ok_or_else(no_placement)?;
        let arcs: Vec<&Edge> = self
            .syntactic_schema
            .outgoing_edges(&anchor)
            .iter()
            .filter(|arc| arc.tgt == writer.reference.holder)
            .collect();
        super::build::holder_arc(&arcs, &found.edge)
            .map(|arc| (writer, arc))
            .ok_or_else(no_placement)
    }

    /// The literal a canonical holder writes for its target: the target's
    /// key tuple, or the provider's canonical path.
    fn canonical_literal(
        &self,
        writer: &super::compile::CompiledRef,
        holder: u32,
        target: u32,
        base: &WInstance,
        graph: &GInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<panproto_inst::Value, ResolveError> {
        let at = ViewRef::Node(target);
        let key = match &writer.form {
            RefForm::KeyValue { key, .. } | RefForm::Variable { key, .. } => key,
            RefForm::Path { .. } => {
                return self.provider.render_path(
                    &writer.reference,
                    ViewRef::Node(holder),
                    at,
                    &AnyInstance::Graph(graph.clone()),
                    ctx,
                );
            }
            RefForm::Standoff { .. } => {
                return Err(domain(ResolveDomainError::UnrepresentableReference {
                    carrier: at,
                    reference: writer.reference.id.clone(),
                }));
            }
        };
        Source::tree(base)
            .key_values(&at, key)
            .map(|values| tuple_literal(&values))
            .ok_or_else(|| {
                domain(ResolveDomainError::UnrepresentableReference {
                    carrier: at,
                    reference: writer.reference.id.clone(),
                })
            })
    }
}
