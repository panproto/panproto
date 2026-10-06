//! Get for the two resolving kinds: a tree or a table instance resolved
//! into a graph.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_inst::resolution_complement::{
    DefinitionKey, RedirectedArc, RedirectedAttachment, RedirectedFragment, RefOccurrence,
    ResolutionComplement, ResolutionSpan, UnresolvedReason, UnresolvedRef,
};
use panproto_inst::{
    AnyInstance, EdgeId, FInstance, GEdge, GInstance, IdAllocator, IdKind, IdRange, InstError,
    Node, TreeShapeReport, ViewRef, WInstance,
};
use panproto_schema::ResolveMode;

use super::error::{ResolutionInstanceError, ResolveError};
use super::occur::{Occurrence, Outcome, Reader, admit, resolve};
use super::source::{Source, node_of};
use super::{Resolution, ResolveContext, ResolveOptions};

/// One occurrence with how it resolved, after the unresolved precedence.
pub type Admitted<'r> = (Occurrence<'r>, Outcome);

/// An instance error at the resolution boundary, as its serializable
/// projection: an exhausted id, a dangling carrier, or the shape the
/// failing conversion needed.
pub fn instance_error(error: &InstError) -> ResolveError {
    let instance = |error| ResolveError::Instance(error);
    match error {
        InstError::NodeIdsExhausted => ResolveError::IdExhausted { kind: IdKind::Node },
        InstError::EdgeIdsExhausted => ResolveError::IdExhausted { kind: IdKind::Edge },
        InstError::DanglingEdge { edge, .. } => {
            instance(ResolutionInstanceError::DanglingCarrier {
                carrier: ViewRef::Edge(*edge),
            })
        }
        InstError::DanglingArc { src, tgt } => instance(ResolutionInstanceError::DanglingCarrier {
            carrier: ViewRef::Arc {
                parent: *src,
                child: *tgt,
            },
        }),
        InstError::NodeNotFound(node) | InstError::NodeIdMismatch { key: node, .. } => {
            instance(ResolutionInstanceError::MissingCarrier {
                carrier: ViewRef::Node(*node),
            })
        }
        InstError::EdgeNotFound(edge) => instance(ResolutionInstanceError::MissingCarrier {
            carrier: ViewRef::Edge(*edge),
        }),
        InstError::MissingRoot
        | InstError::NotATree { .. }
        | InstError::MalformedFan { .. }
        | InstError::ReservedVocabulary { .. } => instance(ResolutionInstanceError::NotTree),
        InstError::NotATable { .. }
        | InstError::DanglingForeignKey { .. }
        | InstError::DuplicateForeignKey { .. }
        | InstError::DuplicateRowId { .. }
        | InstError::RowHasForeignKeys { .. }
        | InstError::HomTableMismatch { .. } => instance(ResolutionInstanceError::NotTable),
        InstError::VertexNotFound(_)
        | InstError::EvalHom(_)
        | InstError::DuplicateNode(_)
        | InstError::DuplicateEdge(_)
        | InstError::IncidentEdges { .. }
        | InstError::NodeLink(_) => instance(ResolutionInstanceError::NotGraph),
        // `InstError` is non-exhaustive across crates; every variant it has
        // today is named above, and one it adds later leaves no graph to
        // read either.
        _ => instance(ResolutionInstanceError::NotGraph),
    }
}

fn allocate(allocator: &mut IdAllocator, kind: IdKind) -> Result<u32, ResolveError> {
    allocator
        .allocate()
        .map_err(|_| ResolveError::IdExhausted { kind })
}

impl Resolution {
    /// Every occurrence of the source with its outcome, after the
    /// unresolved precedence.
    pub(super) fn admitted<'r>(
        &'r self,
        source: &Source<'_>,
        any: &AnyInstance,
        options: ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<Vec<Admitted<'r>>, ResolveError> {
        let reader = Reader {
            source,
            any,
            provider: self.provider.as_ref(),
            provider_id: &self.spec.provider,
            ctx,
        };
        let occurrences = reader.read(&self.compiled)?;
        let outcomes = resolve(source, &occurrences);
        admit(occurrences, outcomes, options.lenient)
    }

    /// Get for a resolving kind.
    pub(super) fn get_graph(
        &self,
        source: &AnyInstance,
        options: ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(GInstance, ResolutionComplement), ResolveError> {
        match source {
            AnyInstance::Tree(tree) => self.get_tree(tree, source, options, ctx),
            AnyInstance::Table(table) => self.get_table(table, source, options, ctx),
            AnyInstance::Graph(_) => Err(ResolveError::ShapeMismatch {
                expected: self.spec.kind.syntactic_shape(),
                found: source.shape(),
            }),
        }
    }

    fn get_tree(
        &self,
        tree: &WInstance,
        any: &AnyInstance,
        options: ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(GInstance, ResolutionComplement), ResolveError> {
        let (mut graph, embedding) =
            GInstance::from_wtype_embedded(tree, Some(&self.syntactic_schema))
                .map_err(|error| instance_error(&error))?;
        let is_tree = matches!(
            graph.tree_shape(&self.syntactic_schema),
            TreeShapeReport::Tree { root } if root == tree.root
        );
        let rooted = tree
            .nodes
            .get(&tree.root)
            .is_some_and(|node| node.anchor == tree.schema_root);
        if !is_tree || !rooted {
            return Err(ResolveError::Instance(ResolutionInstanceError::NotTree));
        }
        let source = Source::tree(tree);
        let admitted = self.admitted(&source, any, options, ctx)?;

        let mut complement = ResolutionComplement::default();
        let inside = redirect(tree, &source, &admitted, &mut complement)?;
        let fans = fan_slots(tree, &inside);
        remove_fragments(&mut graph, &embedding, &inside, &fans)?;

        let visible = |occurrence: &Occurrence<'_>, outcome: &Outcome| {
            node_of(&occurrence.holder).is_some_and(|holder| {
                !inside.contains(&holder)
                    || (complement.redirected.contains_key(&holder)
                        && redirects(occurrence, outcome))
            })
        };
        let recorded: Vec<&Admitted<'_>> = admitted
            .iter()
            .filter(|(occurrence, outcome)| visible(occurrence, outcome))
            .collect();

        let mut nodes = IdAllocator::from_floor(IdRange::FULL, embedding.next_node);
        let spans = allocate_spans(&recorded, &mut graph, &mut nodes, &mut complement)?;
        let mut edges =
            IdAllocator::from_source(IdRange::FULL, graph.edges().keys().map(|id| id.0));
        for (occurrence, outcome) in recorded {
            let holder = node_of(&occurrence.holder).ok_or_else(|| missing(&occurrence.holder))?;
            let source_node = if redirects(occurrence, outcome) {
                complement
                    .redirected
                    .get(&holder)
                    .and_then(|fragment| fragment.attachment.as_ref())
                    .map(|attachment| attachment.parent)
                    .ok_or_else(|| missing(&occurrence.holder))?
            } else {
                holder
            };
            let target = match outcome {
                Outcome::Resolved(target) => {
                    let node = node_of(target).ok_or_else(|| missing(target))?;
                    if inside.contains(&node) {
                        return Err(ResolveError::Instance(
                            ResolutionInstanceError::DanglingCarrier {
                                carrier: target.clone(),
                            },
                        ));
                    }
                    node
                }
                Outcome::Span => span_target(&spans, occurrence)?,
                Outcome::Definition => {
                    record_definition(&mut complement, occurrence, holder);
                    continue;
                }
                Outcome::Dangling | Outcome::Ambiguous(_) => {
                    record_unresolved(&mut complement, occurrence, outcome);
                    continue;
                }
            };
            add_occurrence_edge(
                &mut graph,
                &mut edges,
                &mut complement,
                occurrence,
                source_node,
                target,
            )?;
        }

        record_structure(tree, &source, &embedding, &inside, &mut complement);
        Ok((graph, complement))
    }

    fn get_table(
        &self,
        table: &FInstance,
        any: &AnyInstance,
        options: ResolveOptions,
        ctx: &ResolveContext<'_>,
    ) -> Result<(GInstance, ResolutionComplement), ResolveError> {
        let source = Source::table(table);
        let admitted = self.admitted(&source, any, options, ctx)?;
        let (mut graph, embedding) =
            GInstance::from_finstance(table).map_err(|error| instance_error(&error))?;
        let mut complement = ResolutionComplement::default();
        let recorded: Vec<&Admitted<'_>> = admitted.iter().collect();
        let mut nodes = IdAllocator::from_source(IdRange::FULL, graph.nodes().keys().copied());
        let spans = allocate_spans(&recorded, &mut graph, &mut nodes, &mut complement)?;
        let mut edges = IdAllocator::from_floor(IdRange::FULL, embedding.next_edge);
        for (occurrence, outcome) in recorded {
            let ViewRef::Row(holder) = occurrence.holder else {
                return Err(missing(&occurrence.holder));
            };
            let target = match outcome {
                Outcome::Resolved(ViewRef::Row(target)) => target.0,
                Outcome::Resolved(other) => return Err(missing(other)),
                Outcome::Span => span_target(&spans, occurrence)?,
                Outcome::Definition => {
                    record_definition(&mut complement, occurrence, holder.0);
                    continue;
                }
                Outcome::Dangling | Outcome::Ambiguous(_) => {
                    record_unresolved(&mut complement, occurrence, outcome);
                    continue;
                }
            };
            add_occurrence_edge(
                &mut graph,
                &mut edges,
                &mut complement,
                occurrence,
                holder.0,
                target,
            )?;
        }
        Ok((graph, complement))
    }
}

/// Allocate one node per distinct span the recorded occurrences
/// materialize, in ascending span order, anchored at the reference's
/// target vertex.
fn allocate_spans(
    recorded: &[&Admitted<'_>],
    graph: &mut GInstance,
    nodes: &mut IdAllocator,
    complement: &mut ResolutionComplement,
) -> Result<BTreeMap<ResolutionSpan, u32>, ResolveError> {
    let mut anchors: BTreeMap<ResolutionSpan, &Name> = BTreeMap::new();
    for (occurrence, outcome) in recorded {
        let (Outcome::Span, super::occur::Lookup::Span(span)) = (outcome, &occurrence.lookup)
        else {
            continue;
        };
        let anchor = &occurrence.compiled.reference.target_vertex;
        if anchors
            .get(span)
            .is_some_and(|existing| *existing != anchor)
        {
            return Err(ResolveError::ReferenceConflict {
                reference: occurrence.compiled.reference.id.clone(),
            });
        }
        anchors.insert(span.clone(), anchor);
    }
    let mut ids = BTreeMap::new();
    for (span, anchor) in anchors {
        let id = allocate(nodes, IdKind::Node)?;
        graph
            .insert_node(Node::new(id, anchor.clone()))
            .map_err(|error| instance_error(&error))?;
        complement.span_nodes.insert(id, span.clone());
        ids.insert(span, id);
    }
    Ok(ids)
}

/// Add the semantic edge of one resolved occurrence and record it.
fn add_occurrence_edge(
    graph: &mut GInstance,
    edges: &mut IdAllocator,
    complement: &mut ResolutionComplement,
    occurrence: &Occurrence<'_>,
    source: u32,
    target: u32,
) -> Result<(), ResolveError> {
    let reference = &occurrence.compiled.reference;
    let label = reference.edge.clone().ok_or_else(|| {
        ResolveError::Schema(panproto_schema::KeyRefError::EdgeRequired {
            reference: reference.id.clone(),
        })
    })?;
    let id = EdgeId(allocate(edges, IdKind::Edge)?);
    graph
        .insert_edge(id, GEdge::new(source, target, label))
        .map_err(|error| instance_error(&error))?;
    complement.occurrences.insert(
        ViewRef::Edge(id),
        vec![RefOccurrence {
            holder: occurrence.holder.clone(),
            reference: reference.id.clone(),
            index: occurrence.index,
            literal: occurrence.literal.clone(),
            form: None,
            context: occurrence.context,
        }],
    );
    Ok(())
}

/// Capture the fragment of every maximal redirected holder into the
/// complement, returning the nodes inside fragments.
fn redirect(
    tree: &WInstance,
    source: &Source<'_>,
    admitted: &[Admitted<'_>],
    complement: &mut ResolutionComplement,
) -> Result<BTreeSet<u32>, ResolveError> {
    let holders: BTreeSet<u32> = admitted
        .iter()
        .filter(|(occurrence, outcome)| redirects(occurrence, outcome))
        .filter_map(|(occurrence, _)| node_of(&occurrence.holder))
        .collect();
    let mut inside = BTreeSet::new();
    let mut claimed = BTreeSet::new();
    for &root in holders
        .iter()
        .filter(|holder| !has_ancestor_in(tree, **holder, &holders))
    {
        let nodes = source.subtree(root);
        complement
            .redirected
            .insert(root, capture(tree, source, root, &nodes, &mut claimed)?);
        inside.extend(nodes);
    }
    Ok(inside)
}

/// Whether a fan names a node of `nodes`, as its parent or a child.
fn fan_touches(fan: &panproto_inst::Fan, nodes: &BTreeSet<u32>) -> bool {
    nodes.contains(&fan.parent) || fan.children.values().any(|child| nodes.contains(child))
}

/// The source positions of the fans that touch a node inside a fragment,
/// ascending.
fn fan_slots(tree: &WInstance, inside: &BTreeSet<u32>) -> Vec<usize> {
    tree.fans
        .iter()
        .enumerate()
        .filter(|(_, fan)| fan_touches(fan, inside))
        .map(|(slot, _)| slot)
        .collect()
}

/// Record each retained node's children in arc order (and the root, with
/// or without children) and each retained non-root node's arc edge.
fn record_structure(
    tree: &WInstance,
    source: &Source<'_>,
    embedding: &panproto_inst::TreeEmbedding,
    inside: &BTreeSet<u32>,
    complement: &mut ResolutionComplement,
) {
    for &node in tree.nodes.keys() {
        if inside.contains(&node) {
            continue;
        }
        let children = source.out_steps(&ViewRef::Node(node));
        if !children.is_empty() || node == tree.root {
            complement
                .order
                .insert(node, children.into_iter().map(|step| step.other).collect());
        }
        if let Some(edge) = embedding.arcs.get(&node) {
            complement.spanning.insert(node, ViewRef::Edge(*edge));
        }
    }
}

/// Whether an occurrence redirects its holder: a resolved occurrence of a
/// redirecting reference.
pub fn redirects(occurrence: &Occurrence<'_>, outcome: &Outcome) -> bool {
    occurrence.compiled.reference.mode == ResolveMode::Redirect
        && matches!(outcome, Outcome::Resolved(_) | Outcome::Span)
}

/// Whether a proper ancestor of `node` lies in `set`, in a tree.
fn has_ancestor_in(tree: &WInstance, node: u32, set: &BTreeSet<u32>) -> bool {
    let mut current = tree.parent(node);
    while let Some(parent) = current {
        if set.contains(&parent) {
            return true;
        }
        current = tree.parent(parent);
    }
    false
}

fn missing(carrier: &ViewRef) -> ResolveError {
    ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
        carrier: carrier.clone(),
    })
}

fn span_target(
    spans: &BTreeMap<ResolutionSpan, u32>,
    occurrence: &Occurrence<'_>,
) -> Result<u32, ResolveError> {
    match &occurrence.lookup {
        super::occur::Lookup::Span(span) => spans.get(span).copied(),
        _ => None,
    }
    .ok_or_else(|| missing(&occurrence.holder))
}

fn record_definition(
    complement: &mut ResolutionComplement,
    occurrence: &Occurrence<'_>,
    target: u32,
) {
    complement.definitions.insert(
        DefinitionKey {
            reference: occurrence.compiled.reference.id.clone(),
            target,
        },
        occurrence.holder.clone(),
    );
}

fn record_unresolved(
    complement: &mut ResolutionComplement,
    occurrence: &Occurrence<'_>,
    outcome: &Outcome,
) {
    let reason = match outcome {
        Outcome::Ambiguous(candidates) => UnresolvedReason::Ambiguous {
            candidates: candidates.clone(),
        },
        _ => UnresolvedReason::Dangling,
    };
    complement.unresolved.push(UnresolvedRef {
        holder: occurrence.holder.clone(),
        reference: occurrence.compiled.reference.id.clone(),
        index: occurrence.index,
        literal: occurrence.literal.clone(),
        reason,
        context: occurrence.context,
    });
}

/// The exact fragment a redirect removes: the holder's subtree, the arc
/// that attaches it with its source slot, the arcs inside it with theirs,
/// and every fan that touches it and no fragment of a lesser root, in
/// canonical fan record order.
///
/// A fan's position in the tree's fan list is not recorded: the instance
/// identity writes fans in their record order, so a tree is the same tree
/// whatever order it lists its fans in, and put returns the fragment's
/// fans to the free fan slots in this order. `claimed` holds the source
/// positions of the fans a lesser fragment already holds, so a fan that
/// touches several fragments is held once, by the least.
fn capture(
    tree: &WInstance,
    source: &Source<'_>,
    root: u32,
    nodes: &BTreeSet<u32>,
    claimed: &mut BTreeSet<usize>,
) -> Result<RedirectedFragment, ResolveError> {
    let (parent, slot) = source
        .parent_arc(&ViewRef::Node(root))
        .ok_or_else(|| missing(&ViewRef::Node(root)))?;
    let slot_u32 = |slot: usize| {
        u32::try_from(slot).map_err(|_| ResolveError::IdExhausted { kind: IdKind::Edge })
    };
    let attachment = RedirectedAttachment {
        parent,
        child: root,
        edge: tree.arcs[slot].2.clone(),
        source_slot: slot_u32(slot)?,
    };
    let mut arcs = Vec::new();
    for (index, (from, to, edge)) in tree.arcs.iter().enumerate() {
        if nodes.contains(from) && nodes.contains(to) {
            arcs.push(RedirectedArc {
                parent: *from,
                child: *to,
                edge: edge.clone(),
                source_slot: slot_u32(index)?,
            });
        }
    }
    let mut fans: Vec<panproto_inst::Fan> = Vec::new();
    for (slot, fan) in tree.fans.iter().enumerate() {
        if fan_touches(fan, nodes) && claimed.insert(slot) {
            fans.push(fan.clone());
        }
    }
    fans.sort_by_cached_key(panproto_inst::FanRecord::of);
    let records = nodes
        .iter()
        .map(|id| {
            tree.nodes
                .get(id)
                .cloned()
                .map(|node| (*id, node))
                .ok_or_else(|| missing(&ViewRef::Node(*id)))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(RedirectedFragment {
        root,
        attachment: Some(attachment),
        nodes: records,
        arcs,
        fans,
    })
}

/// Remove the fragments' nodes, their arcs and every fan touching them from
/// the tree's graph form.
fn remove_fragments(
    graph: &mut GInstance,
    embedding: &panproto_inst::TreeEmbedding,
    inside: &BTreeSet<u32>,
    fan_slots: &[usize],
) -> Result<(), ResolveError> {
    let mut edges = BTreeSet::new();
    let mut fan_nodes = Vec::new();
    for &slot in fan_slots {
        let fan = embedding
            .fans
            .get(slot)
            .ok_or(ResolveError::Instance(ResolutionInstanceError::NotTree))?;
        fan_nodes.push(fan.node);
        edges.insert(fan.parent_edge);
        edges.extend(fan.child_edges.values().copied());
    }
    for node in inside {
        if let Some(edge) = embedding.arcs.get(node) {
            edges.insert(*edge);
        }
    }
    for edge in edges {
        graph
            .remove_edge(edge)
            .map_err(|error| instance_error(&error))?;
    }
    for node in fan_nodes.into_iter().chain(inside.iter().copied()) {
        graph
            .remove_node(node)
            .map_err(|error| instance_error(&error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use panproto_inst::{IdAllocator, IdKind, IdRange};

    use super::allocate;
    use crate::resolution::error::ResolveError;

    #[test]
    fn allocation_is_checked_at_the_top_of_the_id_space() {
        for kind in [IdKind::Node, IdKind::Edge] {
            let mut ids = IdAllocator::from_floor(IdRange::FULL, u32::MAX);
            assert!(matches!(allocate(&mut ids, kind), Ok(u32::MAX)));
            assert!(matches!(
                allocate(&mut ids, kind),
                Err(ResolveError::IdExhausted { kind: found }) if found == kind
            ));
        }
    }

    #[test]
    fn instance_errors_project_onto_their_carrier_or_shape() {
        use panproto_inst::{EdgeId, InstError, RowId, ViewRef};

        use super::instance_error;
        use crate::resolution::error::ResolutionInstanceError;

        let label = panproto_schema::Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "fk".into(),
            name: None,
        };

        let cases: Vec<(InstError, ResolveError)> = vec![
            (
                InstError::NodeIdsExhausted,
                ResolveError::IdExhausted { kind: IdKind::Node },
            ),
            (
                InstError::EdgeIdsExhausted,
                ResolveError::IdExhausted { kind: IdKind::Edge },
            ),
            (
                InstError::DanglingEdge {
                    edge: EdgeId(3),
                    endpoint: 9,
                },
                ResolveError::Instance(ResolutionInstanceError::DanglingCarrier {
                    carrier: ViewRef::Edge(EdgeId(3)),
                }),
            ),
            (
                InstError::DanglingArc { src: 1, tgt: 2 },
                ResolveError::Instance(ResolutionInstanceError::DanglingCarrier {
                    carrier: ViewRef::Arc {
                        parent: 1,
                        child: 2,
                    },
                }),
            ),
            (
                InstError::NodeNotFound(4),
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Node(4),
                }),
            ),
            (
                InstError::NodeIdMismatch { key: 5, node_id: 6 },
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Node(5),
                }),
            ),
            (
                InstError::EdgeNotFound(EdgeId(7)),
                ResolveError::Instance(ResolutionInstanceError::MissingCarrier {
                    carrier: ViewRef::Edge(EdgeId(7)),
                }),
            ),
            (
                InstError::MissingRoot,
                ResolveError::Instance(ResolutionInstanceError::NotTree),
            ),
            (
                InstError::HomTableMismatch { row: RowId(1) },
                ResolveError::Instance(ResolutionInstanceError::NotTable),
            ),
            (
                InstError::DuplicateRowId {
                    row: RowId(1),
                    tables: Vec::new(),
                },
                ResolveError::Instance(ResolutionInstanceError::NotTable),
            ),
            (
                InstError::DuplicateNode(1),
                ResolveError::Instance(ResolutionInstanceError::NotGraph),
            ),
            (
                InstError::VertexNotFound("v".to_owned()),
                ResolveError::Instance(ResolutionInstanceError::NotGraph),
            ),
            (
                InstError::EvalHom("h".to_owned()),
                ResolveError::Instance(ResolutionInstanceError::NotGraph),
            ),
            (
                InstError::DuplicateEdge(EdgeId(2)),
                ResolveError::Instance(ResolutionInstanceError::NotGraph),
            ),
            (
                InstError::IncidentEdges {
                    node: 1,
                    edges: vec![EdgeId(2)],
                },
                ResolveError::Instance(ResolutionInstanceError::NotGraph),
            ),
            (
                InstError::NodeLink("n".to_owned()),
                ResolveError::Instance(ResolutionInstanceError::NotGraph),
            ),
            (
                InstError::NotATree {
                    reason: panproto_inst::NotTreeReason::NoRoot,
                },
                ResolveError::Instance(ResolutionInstanceError::NotTree),
            ),
            (
                InstError::MalformedFan {
                    node: 1,
                    reason: panproto_inst::FanViolation::MissingParent,
                },
                ResolveError::Instance(ResolutionInstanceError::NotTree),
            ),
            (
                InstError::ReservedVocabulary {
                    at: ViewRef::Node(1),
                },
                ResolveError::Instance(ResolutionInstanceError::NotTree),
            ),
            (
                InstError::NotATable {
                    reason: panproto_inst::NotTableReason::NodeDataNotRepresentable { node: 1 },
                },
                ResolveError::Instance(ResolutionInstanceError::NotTable),
            ),
            (
                InstError::DanglingForeignKey {
                    edge: label.clone(),
                    row: RowId(1),
                },
                ResolveError::Instance(ResolutionInstanceError::NotTable),
            ),
            (
                InstError::DuplicateForeignKey {
                    edge: label.clone(),
                    src: RowId(1),
                    tgt: RowId(2),
                },
                ResolveError::Instance(ResolutionInstanceError::NotTable),
            ),
            (
                InstError::RowHasForeignKeys {
                    row: RowId(1),
                    from: "a".into(),
                    to: "b".into(),
                    edge: label,
                    src: RowId(1),
                    tgt: RowId(2),
                },
                ResolveError::Instance(ResolutionInstanceError::NotTable),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(
                format!("{:?}", instance_error(&error)),
                format!("{expected:?}"),
                "{error:?}"
            );
        }
    }
}
