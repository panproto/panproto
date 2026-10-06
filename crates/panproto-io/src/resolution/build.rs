//! Building the candidate source of a classified view.
//!
//! A tree candidate is the view's structural part read back as a tree, with
//! every live redirect fragment spliced in at its recorded arc slots and
//! fan slots, and one new holder node for every new redirecting reference
//! edge. A table candidate is the view's structural part read back as a
//! table instance.

use std::collections::{BTreeMap, BTreeSet};

use panproto_gat::Name;
use panproto_inst::resolution_complement::{
    RedirectedFragment, ResolutionComplement, ResolutionSpan, ResolutionTextId,
};
use panproto_inst::{
    AnyInstance, EdgeId, FInstance, FieldPresence, GEdge, GInstance, Node, TreeShapeReport, Value,
    ViewRef, WInstance,
};
use panproto_schema::{Edge, OffsetUnit, RefTextSource, ResolveMode};

use super::compile::{CompiledRef, RefForm};
use super::error::{ResolveDomainError, ResolveError};
use super::get::instance_error;
use super::occur::tuple_literal;
use super::put::{Classified, Refusal, Violations, not_tree_carrier, refuse};
use super::source::{Source, field_name};
use super::standoff;
use super::{Resolution, ResolveContext, ResolveOptions};

/// A new holder node to create for a new redirecting reference edge.
#[derive(Debug)]
struct NewHolder<'r> {
    /// The new edge.
    carrier: EdgeId,
    /// The writer.
    compiled: &'r CompiledRef,
    /// The parent the holder hangs from.
    parent: u32,
    /// The arc label into the holder.
    arc: Edge,
    /// The edge's target.
    target: u32,
}

/// A recorded occurrence of a live redirect fragment whose edge the view
/// removed: its element leaves the holder's `Many` literal.
#[derive(Debug)]
struct Removal<'r> {
    /// The removed occurrence edge.
    carrier: EdgeId,
    /// The fragment root holding the literal.
    holder: u32,
    /// The reference read at the holder.
    compiled: &'r CompiledRef,
    /// The occurrence's index at the holder.
    index: u32,
}

/// What a literal is rendered against: the candidate without new holders,
/// the view and the run's services.
struct Canvas<'a> {
    base: &'a WInstance,
    view: &'a GInstance,
    ctx: &'a ResolveContext<'a>,
}

/// Everything a tree candidate is assembled from.
struct TreeParts<'c, 'r> {
    structural: GInstance,
    live: Vec<(u32, &'c RedirectedFragment)>,
    holders: Vec<NewHolder<'r>>,
    /// Removed occurrences of live fragments, by descending index within
    /// each holder and reference.
    removed: Vec<Removal<'r>>,
    /// One above every tree node of the source the view came from.
    node_floor: u32,
    /// One above every arc slot in use.
    slot_floor: u32,
}

impl Resolution {
    /// The tree candidate of a view.
    pub(super) fn tree_candidate(
        &self,
        graph: &GInstance,
        complement: &ResolutionComplement,
        ctx: &ResolveContext<'_>,
    ) -> Result<WInstance, Refusal> {
        let mut violations = Vec::new();
        let mut classified = self.classify(graph, complement, &mut violations);
        let root = complement
            .order
            .keys()
            .copied()
            .find(|node| !complement.spanning.contains_key(node));
        self.place_tree_edges(graph, complement, &mut classified, &mut violations);
        let parts = self.tree_parts(graph, complement, &classified, root, &mut violations)?;
        refuse(violations)?;
        let base = self.assemble(&parts, None, &BTreeMap::new())?;
        let mut writes = BTreeMap::new();
        let mut failures = Vec::new();
        for (holder, id) in parts.holders.iter().zip(parts.holder_ids()?) {
            let canvas = Canvas {
                base: &base,
                view: graph,
                ctx,
            };
            match self.render(&canvas, holder.compiled, id, holder.target, None) {
                Ok(literal) => {
                    writes.insert(holder.carrier, literal);
                }
                Err(Refusal::Domain(violations)) => failures.extend(violations.into_vec()),
                Err(refusal) => return Err(refusal),
            }
        }
        refuse(failures)?;
        let first = self.assemble(&parts, Some(&writes), &BTreeMap::new())?;
        let canvas = Canvas {
            base: &base,
            view: graph,
            ctx,
        };
        let rewrites = self.rewrites(&parts, &classified, complement, &first, &canvas)?;
        if rewrites.is_empty() {
            Ok(first)
        } else {
            self.assemble(&parts, Some(&writes), &rewrites)
        }
    }

    /// Mark recorded spanning edges structural, promote one new edge into
    /// each node that has lost its structural parent, and give every other
    /// new edge its writer.
    fn place_tree_edges(
        &self,
        graph: &GInstance,
        complement: &ResolutionComplement,
        classified: &mut Classified<'_>,
        violations: &mut Vec<ResolveDomainError>,
    ) {
        let mut has_parent = BTreeSet::new();
        for (child, carrier) in &complement.spanning {
            let ViewRef::Edge(id) = carrier else {
                continue;
            };
            let Some(edge) = graph.edge(*id) else {
                continue;
            };
            if edge.tgt == *child
                && classified.elements.contains(child)
                && classified.elements.contains(&edge.src)
                && !classified.occurrences.contains_key(id)
                && !classified.fan_edges.contains(id)
            {
                classified.structural.insert(*id);
                has_parent.insert(*child);
            }
        }
        let fresh: Vec<(EdgeId, &GEdge)> = graph
            .edges_sorted()
            .filter(|(id, _)| {
                !classified.structural.contains(id)
                    && !classified.occurrences.contains_key(id)
                    && !classified.fan_edges.contains(id)
            })
            .collect();
        let mut promoted: BTreeMap<u32, (&Edge, u32, EdgeId)> = BTreeMap::new();
        for (id, edge) in &fresh {
            let promotable = classified.elements.contains(&edge.tgt)
                && classified.elements.contains(&edge.src)
                && !has_parent.contains(&edge.tgt)
                && self.syntactic_schema.edges.contains_key(&edge.edge);
            if promotable {
                let key = (&edge.edge, edge.src, *id);
                promoted
                    .entry(edge.tgt)
                    .and_modify(|best| {
                        if key < *best {
                            *best = key;
                        }
                    })
                    .or_insert(key);
            }
        }
        for (id, edge) in fresh {
            if promoted.get(&edge.tgt).is_some_and(|best| best.2 == id) {
                classified.structural.insert(id);
                continue;
            }
            match self.writer(id, &edge.edge) {
                Ok(_) => classified.new_references.push((id, edge.edge.clone())),
                Err(violation) => violations.push(violation),
            }
        }
    }

    /// The structural graph, the live fragments and the new holders, with
    /// every placement violation.
    fn tree_parts<'c>(
        &'c self,
        graph: &GInstance,
        complement: &'c ResolutionComplement,
        classified: &Classified<'_>,
        root: Option<u32>,
        violations: &mut Vec<ResolveDomainError>,
    ) -> Result<TreeParts<'c, 'c>, Refusal> {
        let mut structural = GInstance::new();
        for id in classified.elements.iter().chain(&classified.fan_nodes) {
            if let Some(node) = graph.node(*id) {
                structural
                    .insert_node(node.clone())
                    .map_err(|error| Refusal::Operational(instance_error(&error)))?;
            }
        }
        for id in classified.structural.iter().chain(&classified.fan_edges) {
            if let Some(edge) = graph.edge(*id) {
                structural
                    .insert_edge(*id, edge.clone())
                    .map_err(|error| Refusal::Operational(instance_error(&error)))?;
            }
        }
        match structural.tree_shape(&self.syntactic_schema) {
            TreeShapeReport::Tree { .. } => {}
            TreeShapeReport::Forest { roots } => violations.extend(
                roots
                    .into_iter()
                    .filter(|candidate| Some(*candidate) != root)
                    .map(|node| ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Node(node),
                    }),
            ),
            TreeShapeReport::NotTree { reason } => match not_tree_carrier(&reason, root) {
                Some(carrier) => violations.push(ResolveDomainError::NoPlacement { carrier }),
                None => {
                    return Err(Refusal::Operational(ResolveError::Instance(
                        super::ResolutionInstanceError::NotTree,
                    )));
                }
            },
        }

        let taken_slots: BTreeSet<u32> = classified.structural.iter().map(|id| id.0).collect();
        let (live, taken_slots) =
            self.live_fragments(graph, complement, classified, taken_slots, violations);
        let all_tree_nodes = classified.elements.iter().copied().chain(
            complement
                .redirected
                .values()
                .flat_map(|f| f.nodes.keys().copied()),
        );
        let node_floor = all_tree_nodes.max().map_or(Some(0), |id| id.checked_add(1));
        let slot_floor = taken_slots
            .iter()
            .max()
            .map_or(Some(0), |id| id.checked_add(1));
        let (Some(node_floor), Some(slot_floor)) = (node_floor, slot_floor) else {
            return Err(Refusal::Operational(ResolveError::IdExhausted {
                kind: panproto_inst::IdKind::Node,
            }));
        };
        let mut holders = Vec::new();
        for (carrier, label) in &classified.new_references {
            let Ok(compiled) = self.writer(*carrier, label) else {
                continue;
            };
            if compiled.reference.mode != ResolveMode::Redirect {
                continue;
            }
            match self.new_holder(graph, classified, *carrier, compiled) {
                Ok(holder) => holders.push(holder),
                Err(violation) => violations.push(violation),
            }
        }
        let removed = self.removals(graph, complement, &live);
        Ok(TreeParts {
            structural,
            live,
            holders,
            removed,
            node_floor,
            slot_floor,
        })
    }

    /// The recorded redirecting occurrences at a live fragment's root whose
    /// edges the view no longer holds, by holder, reference and descending
    /// index.
    fn removals<'c>(
        &'c self,
        graph: &GInstance,
        complement: &ResolutionComplement,
        live: &[(u32, &RedirectedFragment)],
    ) -> Vec<Removal<'c>> {
        let roots: BTreeSet<u32> = live.iter().map(|(root, _)| *root).collect();
        let mut removed: Vec<Removal<'c>> = complement
            .occurrences
            .iter()
            .filter_map(|(carrier, records)| {
                let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) else {
                    return None;
                };
                let ViewRef::Node(holder) = record.holder else {
                    return None;
                };
                let compiled = self.compiled.get(&record.reference)?;
                let removed = graph.edge(*id).is_none()
                    && roots.contains(&holder)
                    && compiled.reference.mode == ResolveMode::Redirect;
                removed.then_some(Removal {
                    carrier: *id,
                    holder,
                    compiled,
                    index: record.index,
                })
            })
            .collect();
        removed.sort_by(|a, b| {
            (a.holder, &a.compiled.reference.id, b.index).cmp(&(
                b.holder,
                &b.compiled.reference.id,
                a.index,
            ))
        });
        removed
    }

    /// The fragments whose redirecting edges survive in the view, with the
    /// arc slots in use; a fragment node id the view uses, or a slot a
    /// structural edge or another fragment uses, is a placement violation.
    fn live_fragments<'c>(
        &self,
        graph: &GInstance,
        complement: &'c ResolutionComplement,
        classified: &Classified<'_>,
        mut taken_slots: BTreeSet<u32>,
        violations: &mut Vec<ResolveDomainError>,
    ) -> (Vec<(u32, &'c RedirectedFragment)>, BTreeSet<u32>) {
        let mut live_roots = BTreeSet::new();
        for record in classified.occurrences.values() {
            if let (ViewRef::Node(holder), Some(compiled)) =
                (&record.holder, self.compiled.get(&record.reference))
            {
                if compiled.reference.mode == ResolveMode::Redirect {
                    live_roots.insert(*holder);
                }
            }
        }
        let mut taken_nodes: BTreeSet<u32> = graph.nodes().keys().copied().collect();
        let mut live = Vec::new();
        for root in live_roots {
            let Some(fragment) = complement.redirected.get(&root) else {
                continue;
            };
            for id in fragment.nodes.keys() {
                if !taken_nodes.insert(*id) {
                    violations.push(ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Node(*id),
                    });
                }
            }
            let slots = fragment
                .attachment
                .iter()
                .map(|attachment| attachment.source_slot)
                .chain(fragment.arcs.iter().map(|arc| arc.source_slot));
            for slot in slots {
                if !taken_slots.insert(slot) {
                    violations.push(ResolveDomainError::NoPlacement {
                        carrier: ViewRef::Edge(EdgeId(slot)),
                    });
                }
            }
            live.push((root, fragment));
        }
        (live, taken_slots)
    }

    /// The new holder a new redirecting reference edge needs, or why it
    /// cannot be placed.
    fn new_holder<'r>(
        &self,
        graph: &GInstance,
        classified: &Classified<'_>,
        carrier: EdgeId,
        compiled: &'r CompiledRef,
    ) -> Result<NewHolder<'r>, ResolveDomainError> {
        let reference = &compiled.reference;
        let no_placement = ResolveDomainError::NoPlacement {
            carrier: ViewRef::Edge(carrier),
        };
        let unrepresentable = ResolveDomainError::UnrepresentableReference {
            carrier: ViewRef::Edge(carrier),
            reference: reference.id.clone(),
        };
        let edge = graph.edge(carrier).ok_or_else(|| no_placement.clone())?;
        if !classified.elements.contains(&edge.src) || !classified.elements.contains(&edge.tgt) {
            return Err(unrepresentable);
        }
        let writable = match &compiled.form {
            RefForm::KeyValue { parts, .. } => {
                parts.iter().all(|path| path.len() <= 1)
                    && parts.iter().filter(|path| path.is_empty()).count() <= 1
            }
            RefForm::Variable { field, .. } | RefForm::Path { field } => field.len() <= 1,
            RefForm::Standoff { .. } => false,
        };
        if !writable {
            return Err(unrepresentable);
        }
        let parent_anchor = graph
            .node(edge.src)
            .map(|node| node.anchor.clone())
            .ok_or_else(|| no_placement.clone())?;
        let arcs: Vec<&Edge> = self
            .syntactic_schema
            .outgoing_edges(&parent_anchor)
            .iter()
            .filter(|arc| arc.tgt == reference.holder)
            .collect();
        let arc = holder_arc(&arcs, &edge.edge).ok_or(no_placement)?;
        Ok(NewHolder {
            carrier,
            compiled,
            parent: edge.src,
            arc,
            target: edge.tgt,
        })
    }

    /// The literal that writes a reference from a holder to `target`, read
    /// from `base`.
    fn render(
        &self,
        on: &Canvas<'_>,
        compiled: &CompiledRef,
        holder: u32,
        target: u32,
        span: Option<&ResolutionSpan>,
    ) -> Result<Value, Refusal> {
        let (base, view, ctx) = (on.base, on.view, on.ctx);
        let source = Source::tree(base);
        let at = ViewRef::Node(target);
        let key = match &compiled.form {
            RefForm::KeyValue { key, .. } | RefForm::Variable { key, .. } => key,
            RefForm::Path { .. } => {
                let holder = ViewRef::Node(holder);
                return self
                    .provider
                    .render_path(
                        &compiled.reference,
                        holder,
                        at,
                        &AnyInstance::Graph(view.clone()),
                        ctx,
                    )
                    .map_err(Refusal::Operational);
            }
            RefForm::Standoff { .. } => {
                return span.map_or_else(
                    || Err(unrepresentable(compiled, at)),
                    |span| self.render_standoff(compiled, holder, span, base, ctx),
                );
            }
        };
        source
            .key_values(&at, key)
            .map(|values| tuple_literal(&values))
            .ok_or_else(|| unrepresentable(compiled, at))
    }
}

/// One live occurrence whose literal put rewrites: its holder node, its
/// writer, its index and the new literal.
type Rewrite<'r> = (u32, &'r CompiledRef, u32, Value);

impl TreeParts<'_, '_> {
    /// The ids of the new holders, in order, from one above every tree
    /// node.
    /// The index occurrence `index` of `reference` at `holder` has once the
    /// removed occurrences before it have left the literal.
    fn shifted(&self, holder: u32, reference: &Name, index: u32) -> u32 {
        let before = self
            .removed
            .iter()
            .filter(|removal| {
                removal.holder == holder
                    && removal.compiled.reference.id == *reference
                    && removal.index < index
            })
            .count();
        index - u32::try_from(before).unwrap_or(index)
    }

    fn holder_ids(&self) -> Result<Vec<u32>, Refusal> {
        (0..self.holders.len())
            .map(|k| {
                u32::try_from(k)
                    .ok()
                    .and_then(|k| self.node_floor.checked_add(k))
                    .ok_or(Refusal::Operational(ResolveError::IdExhausted {
                        kind: panproto_inst::IdKind::Node,
                    }))
            })
            .collect()
    }
}

impl Resolution {
    /// Assemble the candidate tree: the structural part read as a tree,
    /// the live fragments spliced in at their arc and fan slots, the new
    /// holders appended, then the new holders' literals and the rewritten
    /// literals written.
    fn assemble(
        &self,
        parts: &TreeParts<'_, '_>,
        writes: Option<&BTreeMap<EdgeId, Value>>,
        rewrites: &BTreeMap<EdgeId, Rewrite<'_>>,
    ) -> Result<WInstance, Refusal> {
        let (tree, embedding) = parts
            .structural
            .to_wtype_embedded(&self.syntactic_schema)
            .map_err(|error| Refusal::Operational(instance_error(&error)))?;
        let mut nodes = tree.nodes;
        let mut arcs: Vec<(u32, u32, u32, Edge)> = Vec::new();
        for (parent, child, edge) in tree.arcs {
            let slot = embedding
                .arcs
                .get(&child)
                .map(|id| id.0)
                .ok_or(Refusal::Operational(ResolveError::Instance(
                    super::ResolutionInstanceError::NotTree,
                )))?;
            arcs.push((slot, parent, child, edge));
        }
        let mut fragment_fans = Vec::new();
        for (_, fragment) in &parts.live {
            for (id, node) in &fragment.nodes {
                nodes.insert(*id, node.clone());
            }
            if let Some(attachment) = &fragment.attachment {
                arcs.push((
                    attachment.source_slot,
                    attachment.parent,
                    attachment.child,
                    attachment.edge.clone(),
                ));
            }
            for arc in &fragment.arcs {
                arcs.push((arc.source_slot, arc.parent, arc.child, arc.edge.clone()));
            }
            fragment_fans.extend(fragment.fans.iter().cloned());
        }
        let holder_ids = parts.holder_ids()?;
        let holders: &[NewHolder<'_>] = if writes.is_some() {
            &parts.holders
        } else {
            &[]
        };
        for ((holder, id), k) in holders.iter().zip(&holder_ids).zip(0u32..) {
            nodes.insert(
                *id,
                Node::new(*id, holder.compiled.reference.holder.clone()),
            );
            let slot = parts.slot_floor.checked_add(k).ok_or(Refusal::Operational(
                ResolveError::IdExhausted {
                    kind: panproto_inst::IdKind::Edge,
                },
            ))?;
            arcs.push((slot, holder.parent, *id, holder.arc.clone()));
        }
        arcs.sort_by_key(|arc| arc.0);
        // A fragment's fan returns only with every node it names: a fan
        // whose other fragment the view removed has lost a member.
        fragment_fans.retain(|fan: &panproto_inst::Fan| {
            nodes.contains_key(&fan.parent)
                && fan.children.values().all(|child| nodes.contains_key(child))
        });
        let fan_ids: Vec<u32> = embedding.fans.iter().map(|fan| fan.node).collect();
        let fans = merge_fans(tree.fans, &fan_ids, fragment_fans, parts.node_floor);
        let mut candidate = WInstance::new(
            nodes,
            arcs.into_iter()
                .map(|(_, parent, child, edge)| (parent, child, edge))
                .collect(),
            fans,
            tree.root,
            tree.schema_root,
        );
        write_literals(
            &mut candidate,
            parts,
            holders.iter().zip(&holder_ids),
            writes,
            rewrites,
        )?;
        Ok(candidate)
    }

    /// The live redirecting occurrences whose kept literal no longer
    /// resolves to the target the view gives their edge, with the literal
    /// that does.
    fn rewrites<'r>(
        &'r self,
        parts: &TreeParts<'_, '_>,
        classified: &Classified<'_>,
        complement: &ResolutionComplement,
        first: &WInstance,
        canvas: &Canvas<'_>,
    ) -> Result<BTreeMap<EdgeId, Rewrite<'r>>, Refusal> {
        let (graph, ctx) = (canvas.view, canvas.ctx);
        let lenient = ResolveOptions { lenient: true };
        let Ok((regotten, recomplement)) =
            self.get_graph(&AnyInstance::Tree(first.clone()), lenient, ctx)
        else {
            return Ok(BTreeMap::new());
        };
        let targets: BTreeMap<(ViewRef, Name, u32), u32> = recomplement
            .occurrences
            .iter()
            .filter_map(|(carrier, records)| {
                let (ViewRef::Edge(id), [record]) = (carrier, records.as_slice()) else {
                    return None;
                };
                let edge = regotten.edge(*id)?;
                Some((
                    (
                        record.holder.clone(),
                        record.reference.clone(),
                        record.index,
                    ),
                    edge.tgt,
                ))
            })
            .collect();
        let mut rewrites = BTreeMap::new();
        let mut failures = Vec::new();
        for (id, record) in &classified.occurrences {
            let (Some(compiled), ViewRef::Node(holder), Some(edge)) = (
                self.compiled.get(&record.reference),
                &record.holder,
                graph.edge(*id),
            ) else {
                continue;
            };
            if compiled.reference.mode != ResolveMode::Redirect {
                continue;
            }
            let index = parts.shifted(*holder, &record.reference, record.index);
            let key = (record.holder.clone(), record.reference.clone(), index);
            let kept = targets.get(&key).copied();
            let same = match (complement.span_nodes.get(&edge.tgt), kept) {
                (Some(span), Some(found)) => recomplement.span_nodes.get(&found) == Some(span),
                (None, Some(found)) => found == edge.tgt,
                (_, None) => false,
            };
            if same {
                continue;
            }
            let span = complement.span_nodes.get(&edge.tgt);
            match self.render(canvas, compiled, *holder, edge.tgt, span) {
                Ok(literal) => {
                    rewrites.insert(*id, (*holder, compiled, index, literal));
                }
                Err(Refusal::Domain(violations)) => failures.push(at_edge(violations.first, *id)),
                Err(refusal) => return Err(refusal),
            }
        }
        refuse(failures)?;
        Ok(rewrites)
    }

    /// The table candidate of a view: its structural part read back as a
    /// table instance.
    pub(super) fn table_candidate(
        &self,
        graph: &GInstance,
        complement: &ResolutionComplement,
    ) -> Result<FInstance, Refusal> {
        let mut violations = Vec::new();
        let classified = self.classify(graph, complement, &mut violations);
        let mut structural = GInstance::new();
        for id in &classified.elements {
            if let Some(node) = graph.node(*id) {
                structural
                    .insert_node(node.clone())
                    .map_err(|error| Refusal::Operational(instance_error(&error)))?;
            }
        }
        for id in &classified.fan_nodes {
            violations.push(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(*id),
            });
        }
        for (id, edge) in graph.edges_sorted() {
            if classified.occurrences.contains_key(&id) {
                continue;
            }
            if self.spec.reference_writers.contains_key(&edge.edge) {
                if let Err(violation) = self.writer(id, &edge.edge) {
                    violations.push(violation);
                }
                continue;
            }
            if classified.elements.contains(&edge.src) && classified.elements.contains(&edge.tgt) {
                structural
                    .insert_edge(id, edge.clone())
                    .map_err(|error| Refusal::Operational(instance_error(&error)))?;
            } else {
                violations.push(ResolveDomainError::NoPlacement {
                    carrier: ViewRef::Edge(id),
                });
            }
        }
        refuse(violations)?;
        read_table(&structural)
    }
}

/// The tree's fan list: the retained fans at the slots their fan-node ids
/// name (fan `k` of a tree's graph form is node `node_floor + k`), and the
/// fragments' fans in the remaining slots, in the order given (live
/// fragments by root, each in canonical fan record order). When the
/// retained ids name no consistent slots, the retained fans come first.
fn merge_fans(
    retained: Vec<panproto_inst::Fan>,
    retained_ids: &[u32],
    fragment: Vec<panproto_inst::Fan>,
    node_floor: u32,
) -> Vec<panproto_inst::Fan> {
    let total = retained.len() + fragment.len();
    let slots: Option<Vec<usize>> = retained_ids
        .iter()
        .map(|id| {
            id.checked_sub(node_floor)
                .and_then(|slot| usize::try_from(slot).ok())
                .filter(|slot| *slot < total)
        })
        .collect();
    let Some(slots) = slots.filter(|slots| {
        slots.windows(2).all(|pair| pair[0] < pair[1]) && slots.len() == retained.len()
    }) else {
        return retained.into_iter().chain(fragment).collect();
    };
    let mut placed: Vec<Option<panproto_inst::Fan>> = vec![None; total];
    for (slot, fan) in slots.into_iter().zip(retained) {
        placed[slot] = Some(fan);
    }
    let mut fragment = fragment.into_iter();
    placed
        .into_iter()
        .filter_map(|fan| fan.or_else(|| fragment.next()))
        .collect()
}

/// The paths a reference writes, one per literal part.
fn written_paths(compiled: &CompiledRef) -> Vec<&[Edge]> {
    match &compiled.form {
        RefForm::KeyValue { parts, .. } => parts.iter().map(Vec::as_slice).collect(),
        RefForm::Variable { field, .. } | RefForm::Path { field } => vec![field.as_slice()],
        RefForm::Standoff { start, end, .. } => vec![start.as_slice(), end.as_slice()],
    }
}

/// Write the literal of occurrence `index` of a reference at `holder`.
pub(super) fn write_literal(
    tree: &mut WInstance,
    holder: u32,
    compiled: &CompiledRef,
    index: u32,
    literal: &Value,
) -> Result<(), ()> {
    let paths = written_paths(compiled);
    let values: Vec<&Value> = match (paths.len(), literal) {
        (1, value) => vec![value],
        (count, Value::List(values)) if values.len() == count => values.iter().collect(),
        _ => return Err(()),
    };
    let many = compiled.reference.cardinality == panproto_schema::Cardinality::Many;
    for (path, value) in paths.into_iter().zip(values) {
        write_part(tree, holder, path, many, index, value)?;
    }
    Ok(())
}

/// Write the new holders' literals, remove the removed occurrences and
/// write the rewritten literals into a candidate, in that order.
fn write_literals<'h, 'r: 'h>(
    candidate: &mut WInstance,
    parts: &TreeParts<'_, '_>,
    holders: impl Iterator<Item = (&'h NewHolder<'r>, &'h u32)>,
    writes: Option<&BTreeMap<EdgeId, Value>>,
    rewrites: &BTreeMap<EdgeId, Rewrite<'_>>,
) -> Result<(), Refusal> {
    let mut failures = Vec::new();
    for (holder, id) in holders {
        let written = writes
            .and_then(|writes| writes.get(&holder.carrier))
            .is_some_and(|literal| {
                write_literal(candidate, *id, holder.compiled, 0, literal).is_ok()
            });
        if !written {
            failures.push(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(holder.carrier),
            });
        }
    }
    for removal in &parts.removed {
        if remove_literal(candidate, removal.holder, removal.compiled, removal.index).is_err() {
            failures.push(ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Edge(removal.carrier),
                reference: removal.compiled.reference.id.clone(),
            });
        }
    }
    for (carrier, (holder, compiled, index, literal)) in rewrites {
        if write_literal(candidate, *holder, compiled, *index, literal).is_err() {
            failures.push(ResolveDomainError::UnrepresentableReference {
                carrier: ViewRef::Edge(*carrier),
                reference: compiled.reference.id.clone(),
            });
        }
    }
    refuse(failures)
}

/// Remove occurrence `index` of a `Many` reference at `holder`: the
/// `index`-th element of every part's sequence, which must sit in a list
/// value. A `One` reference has no element to remove.
fn remove_literal(
    tree: &mut WInstance,
    holder: u32,
    compiled: &CompiledRef,
    index: u32,
) -> Result<(), ()> {
    if compiled.reference.cardinality != panproto_schema::Cardinality::Many {
        return Err(());
    }
    let index = usize::try_from(index).map_err(|_| ())?;
    for path in written_paths(compiled) {
        let sites = write_sites(tree, holder, path);
        let mut seen = 0usize;
        let mut found = None;
        for site in &sites {
            let length = match site_value(tree, site) {
                Some(Value::List(items)) => items.len(),
                Some(_) => 1,
                None => 0,
            };
            if index < seen + length {
                found = Some((site.clone(), index - seen));
                break;
            }
            seen += length;
        }
        let (site, position) = found.ok_or(())?;
        let Some(Value::List(items)) = site_value(tree, &site) else {
            return Err(());
        };
        let mut items = items.clone();
        items.remove(position);
        let node = tree.nodes.get_mut(&site.0).ok_or(())?;
        match site.1 {
            None => node.value = Some(FieldPresence::Present(Value::List(items))),
            Some(field) => {
                node.extra_fields.insert(field, Value::List(items));
            }
        }
    }
    Ok(())
}

/// Write `value` as the occurrence `index` of one reference part at
/// `holder`, where reading the part finds it.
///
/// Reading follows the path's hops and, at the last hop, takes the own
/// value of every node reached, or the field named like the last edge of
/// every node before it when none is reached. Writing chooses the same
/// sites: a `One` part needs exactly one, which takes `value`; a `Many`
/// part's sequence (a list value contributes its items) has its `index`-th
/// element replaced, or, one past its end at a single site, `value`
/// appended.
fn write_part(
    tree: &mut WInstance,
    holder: u32,
    path: &[Edge],
    many: bool,
    index: u32,
    value: &Value,
) -> Result<(), ()> {
    let sites = write_sites(tree, holder, path);
    let index = usize::try_from(index).map_err(|_| ())?;
    let (site, replacement) = if many {
        let mut seen = 0usize;
        let mut found = None;
        for site in &sites {
            let current = site_value(tree, site);
            let length = match current {
                Some(Value::List(items)) => items.len(),
                Some(_) => 1,
                None => 0,
            };
            if index < seen + length {
                let replacement = match current {
                    Some(Value::List(items)) => {
                        let mut items = items.clone();
                        items[index - seen] = value.clone();
                        Value::List(items)
                    }
                    _ => value.clone(),
                };
                found = Some((site.clone(), replacement));
                break;
            }
            seen += length;
        }
        match (found, sites.as_slice()) {
            (Some(found), _) => found,
            (None, [site]) if index == seen => {
                let replacement = match site_value(tree, site) {
                    Some(Value::List(items)) => {
                        let mut items = items.clone();
                        items.push(value.clone());
                        Value::List(items)
                    }
                    Some(other) => Value::List(vec![other.clone(), value.clone()]),
                    None => value.clone(),
                };
                (site.clone(), replacement)
            }
            _ => return Err(()),
        }
    } else {
        let [site] = sites.as_slice() else {
            return Err(());
        };
        (site.clone(), value.clone())
    };
    let node = tree.nodes.get_mut(&site.0).ok_or(())?;
    match site.1 {
        None => node.value = Some(FieldPresence::Present(replacement)),
        Some(field) => {
            node.extra_fields.insert(field, replacement);
        }
    }
    Ok(())
}

/// The sites reading `path` from `holder` finds, present or not: the nodes
/// the last hop reaches (their own values), else the field named like the
/// last edge of each node before it.
fn write_sites(tree: &WInstance, holder: u32, path: &[Edge]) -> Vec<(u32, Option<String>)> {
    let Some((last, hops)) = path.split_last() else {
        return vec![(holder, None)];
    };
    let children = |node: u32, label: &Edge| -> Vec<u32> {
        tree.arcs
            .iter()
            .filter(|(parent, _, edge)| *parent == node && edge == label)
            .map(|(_, child, _)| *child)
            .collect()
    };
    let mut frontier = vec![holder];
    for hop in hops {
        frontier = frontier
            .iter()
            .flat_map(|node| children(*node, hop))
            .collect();
    }
    let reached: Vec<u32> = frontier
        .iter()
        .flat_map(|node| children(*node, last))
        .collect();
    if reached.is_empty() {
        let field = field_name(last);
        frontier
            .into_iter()
            .map(|node| (node, Some(field.clone())))
            .collect()
    } else {
        reached.into_iter().map(|node| (node, None)).collect()
    }
}

/// The present value at a write site.
fn site_value<'t>(tree: &'t WInstance, site: &(u32, Option<String>)) -> Option<&'t Value> {
    let node = tree.nodes.get(&site.0)?;
    let value = site.1.as_ref().map_or_else(
        || match &node.value {
            Some(FieldPresence::Present(value)) => Some(value),
            Some(FieldPresence::Absent | FieldPresence::Null) | None => None,
        },
        |field| node.extra_fields.get(field),
    );
    value.filter(|value| !matches!(value, Value::Null))
}

/// A violation renamed to name the occurrence edge it was found for.
fn at_edge(violation: ResolveDomainError, edge: EdgeId) -> ResolveDomainError {
    let carrier = ViewRef::Edge(edge);
    match violation {
        ResolveDomainError::OffsetOutOfRange { reference, .. } => {
            ResolveDomainError::OffsetOutOfRange { carrier, reference }
        }
        ResolveDomainError::OffsetUnitViolation {
            reference, reason, ..
        } => ResolveDomainError::OffsetUnitViolation {
            carrier,
            reference,
            reason,
        },
        ResolveDomainError::UnrepresentableReference { reference, .. } => {
            ResolveDomainError::UnrepresentableReference { carrier, reference }
        }
        other => other,
    }
}

impl Resolution {
    /// The declared `(start, end)` literal that writes a redirecting
    /// standoff holder's reference to `span`.
    ///
    /// The holder's text path must reach exactly the span's text site, as
    /// the same local or external kind: a reconnect to another text is
    /// unrepresentable, since the holder's own text path cannot name it.
    /// The span's byte interval converts to the declared unit only at unit
    /// boundaries, and an inclusive declaration cannot write an empty span.
    fn render_standoff(
        &self,
        compiled: &CompiledRef,
        holder: u32,
        span: &ResolutionSpan,
        base: &WInstance,
        ctx: &ResolveContext<'_>,
    ) -> Result<Value, Refusal> {
        let RefForm::Standoff {
            text,
            unit,
            end_inclusive,
            ..
        } = &compiled.form
        else {
            return Err(unrepresentable(compiled, ViewRef::Node(holder)));
        };
        let source = Source::tree(base);
        let (path, local) = match text {
            RefTextSource::Local(path) => (path, true),
            RefTextSource::External(path) => (path, false),
        };
        let sites = source.path_sites(&ViewRef::Node(holder), path);
        let [(site, value)] = sites.as_slice() else {
            return Err(unrepresentable(compiled, ViewRef::Node(holder)));
        };
        let id = if local {
            ResolutionTextId::Local(site.clone())
        } else {
            ResolutionTextId::ExternalKey(site.clone())
        };
        if id != span.text {
            return Err(unrepresentable(compiled, ViewRef::Node(holder)));
        }
        let bytes = if local {
            match value {
                Value::Str(text) => text.as_bytes().to_vec(),
                Value::Bytes(bytes) => bytes.clone(),
                _ => return Err(unrepresentable(compiled, ViewRef::Node(holder))),
            }
        } else {
            let store = ctx.external_text.ok_or_else(|| {
                Refusal::Operational(ResolveError::UnsupportedFeature {
                    provider: Some(self.spec.provider.clone()),
                    feature: super::ResolutionFeature::ExternalText,
                })
            })?;
            store.load(value)?
        };
        let tokens = if matches!(unit, OffsetUnit::Token) {
            let provider = ctx.token_boundaries.ok_or_else(|| {
                Refusal::Operational(ResolveError::UnsupportedFeature {
                    provider: Some(self.spec.provider.clone()),
                    feature: super::ResolutionFeature::TokenBoundaries,
                })
            })?;
            Some(provider.boundaries(id, &bytes)?)
        } else {
            None
        };
        let violation = |failure: standoff::UnitFailure| {
            let reference = compiled.reference.id.clone();
            let carrier = ViewRef::Node(holder);
            let violation = match failure {
                standoff::UnitFailure::OutOfRange => {
                    ResolveDomainError::OffsetOutOfRange { carrier, reference }
                }
                standoff::UnitFailure::Violation(reason) => {
                    ResolveDomainError::OffsetUnitViolation {
                        carrier,
                        reference,
                        reason,
                    }
                }
            };
            Refusal::Domain(Violations::one(violation))
        };
        let boundaries = standoff::unit_boundaries(&bytes, unit, tokens.as_deref())
            .map_err(|reason| violation(standoff::UnitFailure::Violation(reason)))?;
        let (start, end) =
            standoff::bytes_to_declared(&boundaries, span.start, span.end, *end_inclusive)
                .map_err(violation)?;
        let int = |offset: u64| {
            i64::try_from(offset)
                .map(Value::Int)
                .map_err(|_| violation(standoff::UnitFailure::OutOfRange))
        };
        Ok(Value::List(vec![int(start)?, int(end)?]))
    }
}

/// The refusal that `compiled` cannot write a reference at `carrier`.
fn unrepresentable(compiled: &CompiledRef, carrier: ViewRef) -> Refusal {
    Refusal::Domain(Violations::one(
        ResolveDomainError::UnrepresentableReference {
            carrier,
            reference: compiled.reference.id.clone(),
        },
    ))
}

/// The table instance a structural graph is the form of: each node a row
/// of its anchor's table with its fields as cells, each edge a foreign-key
/// pair. A row keeps list and record cells exactly as the graph holds them.
///
/// # Errors
///
/// [`Refusal::Domain`] with [`ResolveDomainError::NoPlacement`] naming
/// every node that carries more than a row (a value, a discriminator, a
/// position, a shape or annotations) and every edge that carries a
/// position, joins rows of other tables than its label's or repeats a pair.
pub(super) fn read_table(structural: &GInstance) -> Result<FInstance, Refusal> {
    let mut table = FInstance::new();
    let mut violations = Vec::new();
    let mut ids: Vec<u32> = structural.nodes().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let Some(node) = structural.node(id) else {
            continue;
        };
        let row_like = node.value.is_none()
            && node.discriminator.is_none()
            && node.position.is_none()
            && node.shape == panproto_inst::NodeShape::Plain
            && node.annotations.is_empty();
        if row_like {
            table
                .insert_row(
                    node.anchor.clone(),
                    panproto_inst::RowId(id),
                    node.extra_fields.clone(),
                )
                .map_err(|error| Refusal::Operational(instance_error(&error)))?;
        } else {
            violations.push(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(id),
            });
        }
    }
    for (id, edge) in structural.edges_sorted() {
        let placed = edge.position.is_none()
            && table
                .insert_foreign_key(
                    edge.edge.clone(),
                    panproto_inst::RowId(edge.src),
                    panproto_inst::RowId(edge.tgt),
                )
                .is_ok();
        if !placed {
            violations.push(ResolveDomainError::NoPlacement {
                carrier: ViewRef::Edge(id),
            });
        }
    }
    refuse(violations)?;
    Ok(table)
}

/// The arc a new redirect holder hangs from its parent by: the one
/// syntactic edge from the parent's vertex to the holder's vertex, or,
/// among several, the semantic edge's own label when it is one of them.
pub(super) fn holder_arc(arcs: &[&Edge], label: &Edge) -> Option<Edge> {
    match arcs {
        [arc] => Some((*arc).clone()),
        _ => arcs
            .iter()
            .find(|arc| **arc == label)
            .map(|arc| (*arc).clone()),
    }
}

#[cfg(test)]
mod tests {
    use panproto_inst::Fan;

    use super::merge_fans;

    fn fan(parent: u32) -> Fan {
        Fan::new("h", parent)
    }

    #[test]
    fn fans_fill_their_slots_and_fragments_fill_the_rest() {
        let merged = merge_fans(vec![fan(1), fan(3)], &[10, 12], vec![fan(2)], 10);
        let parents: Vec<u32> = merged.iter().map(|fan| fan.parent).collect();
        assert_eq!(parents, vec![1, 2, 3]);
    }

    #[test]
    fn a_retained_fan_past_the_last_slot_puts_the_retained_fans_first() {
        let merged = merge_fans(vec![fan(1)], &[11], vec![], 10);
        let parents: Vec<u32> = merged.iter().map(|fan| fan.parent).collect();
        assert_eq!(parents, vec![1]);
        let merged = merge_fans(vec![fan(1), fan(3)], &[10, 12], vec![], 10);
        let parents: Vec<u32> = merged.iter().map(|fan| fan.parent).collect();
        assert_eq!(parents, vec![1, 3]);
    }

    #[test]
    fn retained_fans_out_of_order_come_first() {
        let merged = merge_fans(vec![fan(1), fan(3)], &[11, 10], vec![fan(2)], 10);
        let parents: Vec<u32> = merged.iter().map(|fan| fan.parent).collect();
        assert_eq!(parents, vec![1, 3, 2]);
    }
}
