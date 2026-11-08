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
    AnyInstance, EdgeId, FInstance, FieldPresence, GEdge, GInstance, IdAllocator, IdRange, Node,
    TreeShapeReport, Value, ViewRef, WInstance,
};
use panproto_schema::{Cardinality, Edge, OffsetUnit, RefTextSource, ResolveMode};

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

/// One occurrence put writes: a new holder's, or a recorded redirecting
/// occurrence's.
#[derive(Debug)]
struct Written<'r> {
    /// The occurrence's edge in the view.
    carrier: EdgeId,
    /// The node holding the literal in the candidate.
    holder: u32,
    /// The writer.
    compiled: &'r CompiledRef,
    /// The occurrence's index at the holder in the candidate.
    index: u32,
    /// The target the view gives the edge.
    target: u32,
    /// The written forms, first form first.
    forms: Vec<Value>,
    /// Whether the literal replaces a recorded one rather than filling a
    /// new holder.
    rewrite: bool,
}

impl Written<'_> {
    /// The occurrence as a form choice.
    fn choice(&self) -> FormChoice {
        FormChoice {
            carrier: self.carrier,
            key: (
                ViewRef::Node(self.holder),
                self.compiled.reference.id.clone(),
                self.index,
            ),
            target: self.target,
            forms: self.forms.len(),
            checked: !matches!(self.compiled.form, RefForm::Standoff { .. }),
        }
    }
}

/// One written occurrence whose form [`Resolution::choose`] picks.
#[derive(Debug)]
pub(super) struct FormChoice {
    /// The occurrence's edge in the view.
    pub carrier: EdgeId,
    /// The key get gives the occurrence in the candidate: holder,
    /// reference and index.
    pub key: (ViewRef, Name, u32),
    /// The target the view gives the edge.
    pub target: u32,
    /// The number of written forms.
    pub forms: usize,
    /// Whether the occurrence's target is an element whose resolution is
    /// checked here, rather than a span verification compares by key.
    pub checked: bool,
}

/// The target of each resolved occurrence of a resolved view, by holder,
/// reference and index.
fn occurrence_targets(
    regotten: &GInstance,
    recomplement: &ResolutionComplement,
) -> BTreeMap<(ViewRef, Name, u32), u32> {
    recomplement
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
        .collect()
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

/// The next appended index follows every retained semantic word position.
fn appended_index_count(
    holder: u32,
    reference: &Name,
    classified: &Classified<'_>,
    complement: &ResolutionComplement,
    written: &[Written<'_>],
) -> Result<usize, Refusal> {
    let at = |record_holder: &ViewRef, record_reference: &Name| {
        *record_holder == ViewRef::Node(holder) && record_reference == reference
    };
    let resolved = classified
        .occurrences
        .values()
        .filter(|record| at(&record.holder, &record.reference))
        .count();
    let unresolved = complement
        .unresolved
        .iter()
        .filter(|record| {
            at(&record.holder, &record.reference)
                && !matches!(
                    record.reason,
                    panproto_inst::resolution_complement::UnresolvedReason::Unselected { .. }
                )
        })
        .count();
    let definitions = complement
        .definition_occurrences
        .iter()
        .filter(|record| at(&record.holder, &record.reference))
        .count();
    let added = written
        .iter()
        .filter(|record| record.holder == holder && &record.compiled.reference.id == reference)
        .count();
    [resolved, unresolved, definitions, added]
        .into_iter()
        .try_fold(0_usize, |count, more| {
            count
                .checked_add(more)
                .ok_or(Refusal::Operational(ResolveError::IdExhausted {
                    kind: panproto_inst::segment::IdKind::Edge,
                }))
        })
}

impl Resolution {
    /// The tree candidate of a view.
    ///
    /// The view's structural part is read as a tree with the live fragments
    /// spliced in; every occurrence put writes (a new holder's, or a
    /// recorded redirecting occurrence's whose kept literal no longer names
    /// its target) then takes the first of its written forms that resolves
    /// back to its target.
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
        let canvas = Canvas {
            base: &base,
            view: graph,
            ctx,
        };
        let mut written = Vec::new();
        let mut failures = Vec::new();
        for (holder, id) in parts.holders.iter().zip(parts.holder_ids()?) {
            match self.forms(&canvas, holder.compiled, id, holder.target, None) {
                Ok(forms) => written.push(Written {
                    carrier: holder.carrier,
                    holder: id,
                    compiled: holder.compiled,
                    index: 0,
                    target: holder.target,
                    forms,
                    rewrite: false,
                }),
                Err(Refusal::Domain(violations)) => failures.extend(
                    violations
                        .into_vec()
                        .into_iter()
                        .map(|violation| at_edge(violation, holder.carrier)),
                ),
                Err(refusal) => return Err(refusal),
            }
        }
        written.extend(self.added_occurrences(&canvas, &classified, complement)?);
        refuse(failures)?;
        let first = self.assemble_written(&parts, &written, &vec![0; written.len()])?;
        written.extend(self.rewrites(&parts, &classified, complement, &first, &canvas)?);
        // The order is the occurrences' keys in the candidate, which no
        // renaming of the view's presentation ids moves.
        written.sort_by_key(|occurrence| {
            (
                occurrence.holder,
                occurrence.compiled.reference.id.clone(),
                occurrence.index,
            )
        });
        self.choose_forms(&parts, &written, ctx)
    }

    /// New `AddEdge` occurrences written at holders that already exist in the
    /// source tree. Their index is the number of retained occurrences at the
    /// same holder and declaration, including unresolved tuples and earlier
    /// additions. Unindexed edited carriers are appended in edge-id order.
    fn added_occurrences<'r>(
        &'r self,
        canvas: &Canvas<'_>,
        classified: &Classified<'_>,
        complement: &ResolutionComplement,
    ) -> Result<Vec<Written<'r>>, Refusal> {
        let mut written: Vec<Written<'r>> = Vec::new();
        let mut failures = Vec::new();
        for (carrier, label) in &classified.new_references {
            let Ok(compiled) = self.writer(*carrier, label) else {
                continue;
            };
            if compiled.reference.mode != ResolveMode::AddEdge {
                continue;
            }
            let Some(edge) = canvas.view.edge(*carrier) else {
                continue;
            };
            let holder_is_writable = classified.elements.contains(&edge.src)
                && classified.elements.contains(&edge.tgt)
                && canvas
                    .view
                    .node(edge.src)
                    .is_some_and(|node| node.anchor == compiled.reference.holder);
            let count = appended_index_count(
                edge.src,
                &compiled.reference.id,
                classified,
                complement,
                &written,
            )?;
            if !holder_is_writable
                || (compiled.reference.cardinality == Cardinality::One && count != 0)
            {
                failures.push(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(*carrier),
                    reference: compiled.reference.id.clone(),
                });
                continue;
            }
            let index = u32::try_from(count).map_err(|_| {
                Refusal::Operational(ResolveError::IdExhausted {
                    kind: panproto_inst::segment::IdKind::Edge,
                })
            })?;
            match self.forms(canvas, compiled, edge.src, edge.tgt, None) {
                Ok(forms) => written.push(Written {
                    carrier: *carrier,
                    holder: edge.src,
                    compiled,
                    index,
                    target: edge.tgt,
                    forms,
                    // The holder exists already, so assembly writes through
                    // the same path used for a changed recorded literal.
                    rewrite: true,
                }),
                Err(Refusal::Domain(violations)) => failures.extend(
                    violations
                        .into_vec()
                        .into_iter()
                        .map(|violation| at_edge(violation, *carrier)),
                ),
                Err(refusal) => return Err(refusal),
            }
        }
        refuse(failures)?;
        Ok(written)
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
                    .and_modify(|best| *best = (*best).min(key))
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
        let node_floor = tree_node_floor(all_tree_nodes);
        let slot_floor = taken_slots
            .iter()
            .max()
            .map_or(Some(0), |id| id.checked_add(1))
            .ok_or(Refusal::Operational(ResolveError::IdExhausted {
                kind: panproto_inst::segment::IdKind::Node,
            }))?;
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
                && compiled.reference.mode == ResolveMode::Redirect
            {
                live_roots.insert(*holder);
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
        holder_ids_from(self.node_floor, self.holders.len()).map_err(Refusal::Operational)
    }
}

/// The node floor of a source's tree nodes: one above the largest,
/// saturating at `u32::MAX`, and 0 for none.
fn tree_node_floor(ids: impl Iterator<Item = u32>) -> u32 {
    IdAllocator::from_source(IdRange::FULL, ids).floor()
}

/// The node ids of `count` new holders, upward from `node_floor`; the id
/// `u32::MAX` is never handed out.
fn holder_ids_from(node_floor: u32, count: usize) -> Result<Vec<u32>, ResolveError> {
    let mut ids = IdAllocator::from_floor(IdRange::FULL, node_floor);
    (0..count)
        .map(|_| {
            ids.allocate().map_err(|_| ResolveError::IdExhausted {
                kind: panproto_inst::segment::IdKind::Node,
            })
        })
        .collect()
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
                    kind: panproto_inst::segment::IdKind::Edge,
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

    /// The recorded redirecting occurrences put writes: those whose kept
    /// literal no longer resolves to the target the view gives their edge
    /// and is not one of the written forms of that target, that is, those
    /// the view reconnected or whose target the view respelled.
    ///
    /// An occurrence whose kept literal is still one of its target's
    /// written forms is kept, never respelled, even when it no longer
    /// resolves to the target: the view did not change it, so its source
    /// text stays, and verification refuses it if an edit elsewhere
    /// captured it.
    fn rewrites<'r>(
        &'r self,
        parts: &TreeParts<'_, '_>,
        classified: &Classified<'_>,
        complement: &ResolutionComplement,
        first: &WInstance,
        canvas: &Canvas<'_>,
    ) -> Result<Vec<Written<'r>>, Refusal> {
        let (graph, ctx) = (canvas.view, canvas.ctx);
        let lenient = ResolveOptions { lenient: true };
        let Ok((regotten, recomplement)) =
            self.get_graph(&AnyInstance::Tree(first.clone()), lenient, ctx)
        else {
            return Ok(Vec::new());
        };
        let targets = occurrence_targets(&regotten, &recomplement);
        let mut written = Vec::new();
        let mut failures = Vec::new();
        let mut occurrences: Vec<_> = classified.occurrences.iter().collect();
        occurrences.sort_by(|(_, left), (_, right)| {
            (&left.holder, &left.reference, left.index).cmp(&(
                &right.holder,
                &right.reference,
                right.index,
            ))
        });
        for (id, record) in occurrences {
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
            match self.forms(canvas, compiled, *holder, edge.tgt, span) {
                Ok(forms) if forms.contains(&record.literal) => {}
                Ok(forms) => written.push(Written {
                    carrier: *id,
                    holder: *holder,
                    compiled,
                    index,
                    target: edge.tgt,
                    forms,
                    rewrite: true,
                }),
                Err(Refusal::Domain(violations)) => failures.push(at_edge(violations.first, *id)),
                Err(refusal) => return Err(refusal),
            }
        }
        refuse(failures)?;
        Ok(written)
    }

    /// The written forms of a reference from a holder to `target`: the
    /// first form (the target's key tuple, the provider's canonical
    /// rendering, the definition's name, the span's declared offsets),
    /// followed for a key or path reference by the provider's more
    /// qualified forms.
    fn forms(
        &self,
        on: &Canvas<'_>,
        compiled: &CompiledRef,
        holder: u32,
        target: u32,
        span: Option<&ResolutionSpan>,
    ) -> Result<Vec<Value>, Refusal> {
        let first = self.render(on, compiled, holder, target, span)?;
        let mut forms = vec![first];
        if matches!(
            compiled.form,
            RefForm::KeyValue { .. } | RefForm::Path { .. }
        ) {
            forms.extend(
                self.provider
                    .qualified_forms(
                        &compiled.reference,
                        ViewRef::Node(holder),
                        ViewRef::Node(target),
                        &forms[0],
                        &AnyInstance::Graph(on.view.clone()),
                        on.ctx,
                    )
                    .map_err(Refusal::Operational)?,
            );
        }
        Ok(forms)
    }

    /// The candidate with each written occurrence holding the form
    /// `choices` gives it.
    fn assemble_written(
        &self,
        parts: &TreeParts<'_, '_>,
        written: &[Written<'_>],
        choices: &[usize],
    ) -> Result<WInstance, Refusal> {
        let mut writes = BTreeMap::new();
        let mut rewrites = BTreeMap::new();
        for (occurrence, choice) in written.iter().zip(choices) {
            let Some(literal) = occurrence.forms.get(*choice).cloned() else {
                continue;
            };
            if occurrence.rewrite {
                rewrites.insert(
                    occurrence.carrier,
                    (
                        occurrence.holder,
                        occurrence.compiled,
                        occurrence.index,
                        literal,
                    ),
                );
            } else {
                writes.insert(occurrence.carrier, literal);
            }
        }
        self.assemble(parts, Some(&writes), &rewrites)
    }

    /// The candidate with every written occurrence holding the first of
    /// its forms that resolves back to its target.
    fn choose_forms(
        &self,
        parts: &TreeParts<'_, '_>,
        written: &[Written<'_>],
        ctx: &ResolveContext<'_>,
    ) -> Result<WInstance, Refusal> {
        let choices: Vec<FormChoice> = written.iter().map(Written::choice).collect();
        let build = |chosen: &[usize]| self.assemble_written(parts, written, chosen);
        self.choose(&choices, &build, ctx)
    }

    /// The candidate `build` makes with every written occurrence holding
    /// the first of its forms that resolves back to its target.
    ///
    /// The occurrences are taken in the order given (ascending key in the
    /// candidate);
    /// each keeps its first form when that resolves to its target in the
    /// candidate holding the forms chosen so far, and otherwise takes the
    /// first more qualified form that does. An occurrence of a key, path
    /// or variable reference none of whose forms resolves back is
    /// [`ResolveDomainError::UnrepresentableReference`]; a span's
    /// occurrence is left to verification, which compares spans by key. A
    /// candidate get cannot read leaves every remaining choice at the first
    /// form, for verification to report.
    pub(super) fn choose(
        &self,
        choices: &[FormChoice],
        build: &dyn Fn(&[usize]) -> Result<WInstance, Refusal>,
        ctx: &ResolveContext<'_>,
    ) -> Result<WInstance, Refusal> {
        let mut chosen = vec![0; choices.len()];
        let mut candidate = build(&chosen)?;
        let mut targets = self.written_targets(&candidate, ctx);
        let mut failures = Vec::new();
        for (position, choice) in choices.iter().enumerate() {
            if !choice.checked {
                continue;
            }
            let Some(found) = &targets else {
                break;
            };
            if found.get(&choice.key) == Some(&choice.target) {
                continue;
            }
            let mut resolved = false;
            for form in 1..choice.forms {
                chosen[position] = form;
                let next = build(&chosen)?;
                let next_targets = self.written_targets(&next, ctx);
                if next_targets
                    .as_ref()
                    .is_some_and(|found| found.get(&choice.key) == Some(&choice.target))
                {
                    candidate = next;
                    targets = next_targets;
                    resolved = true;
                    break;
                }
            }
            if !resolved {
                chosen[position] = 0;
                failures.push(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(choice.carrier),
                    reference: choice.key.1.clone(),
                });
            }
        }
        refuse(failures)?;
        Ok(candidate)
    }

    /// The target each occurrence of a candidate resolves to, by holder,
    /// reference and index, or `None` when get cannot read the candidate.
    fn written_targets(
        &self,
        candidate: &WInstance,
        ctx: &ResolveContext<'_>,
    ) -> Option<BTreeMap<(ViewRef, Name, u32), u32>> {
        let lenient = ResolveOptions { lenient: true };
        self.get_graph(&AnyInstance::Tree(candidate.clone()), lenient, ctx)
            .ok()
            .map(|(regotten, recomplement)| occurrence_targets(&regotten, &recomplement))
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
    // Every declaration addresses its original word even when source sites
    // overlap. Later writers still commit in source declaration order.
    let groups: Vec<_> = literal_edit_groups(parts, rewrites)
        .into_iter()
        .map(|((holder, _), edits)| {
            let word = keyed_word(candidate, holder, edits.compiled);
            (holder, edits, word)
        })
        .collect();
    for (holder, edits, word) in groups {
        match write_keyed_word(candidate, holder, &edits, word) {
            Ok(true) => continue,
            Err(carrier) => {
                failures.push(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(carrier),
                    reference: edits.compiled.reference.id.clone(),
                });
                continue;
            }
            Ok(false) => {}
        }
        for (index, carrier) in edits.removed {
            if remove_literal(candidate, holder, edits.compiled, index).is_err() {
                failures.push(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(carrier),
                    reference: edits.compiled.reference.id.clone(),
                });
            }
        }
        for (index, carrier, literal) in edits.rewritten {
            if write_literal(candidate, holder, edits.compiled, index, literal).is_err() {
                failures.push(ResolveDomainError::UnrepresentableReference {
                    carrier: ViewRef::Edge(carrier),
                    reference: edits.compiled.reference.id.clone(),
                });
            }
        }
    }
    refuse(failures)
}

/// Changes to one declaration, with removals in original coordinates and
/// rewrites in the retained word's coordinates.
struct LiteralEdits<'a> {
    compiled: &'a CompiledRef,
    removed: Vec<(u32, EdgeId)>,
    rewritten: Vec<(u32, EdgeId, &'a Value)>,
}

/// Source coordinates, rather than presentation edge ids, order writers.
fn literal_edit_groups<'a>(
    parts: &'a TreeParts<'_, '_>,
    rewrites: &'a BTreeMap<EdgeId, Rewrite<'_>>,
) -> BTreeMap<(u32, Name), LiteralEdits<'a>> {
    let mut groups = BTreeMap::new();
    for removal in &parts.removed {
        groups
            .entry((removal.holder, removal.compiled.reference.id.clone()))
            .or_insert_with(|| LiteralEdits {
                compiled: removal.compiled,
                removed: Vec::new(),
                rewritten: Vec::new(),
            })
            .removed
            .push((removal.index, removal.carrier));
    }
    for (carrier, (holder, compiled, index, literal)) in rewrites {
        groups
            .entry((*holder, compiled.reference.id.clone()))
            .or_insert_with(|| LiteralEdits {
                compiled,
                removed: Vec::new(),
                rewritten: Vec::new(),
            })
            .rewritten
            .push((*index, *carrier, literal));
    }
    for edits in groups.values_mut() {
        edits
            .removed
            .sort_by_key(|entry| std::cmp::Reverse(entry.0));
        edits
            .rewritten
            .sort_by_key(|(index, carrier, _)| (*index, *carrier));
    }
    groups
}

/// A writable own value or named field in the original source tree.
type TreeValueSite = (u32, Option<String>);
/// Original presence at one writable source site.
type OriginalSite = (TreeValueSite, Option<Value>);
/// One compatible value replacement at a source site.
type SiteReplacement = (TreeValueSite, Value);
/// One requested tuple and its optional original raw row.
type RequestedRow = (Option<usize>, Vec<Value>);

/// One part's original sites and flattened values, before any writer mutates them.
struct KeyedPart {
    sites: Vec<OriginalSite>,
    values: Vec<Value>,
}

/// The original site partition and null-filtered semantic occurrence word.
struct KeyedWord {
    parts: Vec<KeyedPart>,
    rows: Vec<usize>,
    tuples: Vec<Vec<Value>>,
}

/// Read the keyed reader's flattened zip and null filter, retaining raw rows.
fn keyed_word(
    tree: &WInstance,
    holder: u32,
    compiled: &CompiledRef,
) -> Result<Option<KeyedWord>, ()> {
    if compiled.reference.cardinality != Cardinality::Many {
        return Ok(None);
    }
    let (parts, keyed): (&[Vec<Edge>], bool) = match &compiled.form {
        RefForm::KeyValue { parts, .. } => (parts, true),
        RefForm::Variable { field, .. } => (std::slice::from_ref(field), true),
        RefForm::Path { field } => (std::slice::from_ref(field), false),
        RefForm::Standoff { .. } => return Ok(None),
    };
    let mut columns = Vec::new();
    for path in parts {
        let sites: Vec<_> = write_sites(tree, holder, path)
            .into_iter()
            .map(|site| {
                let value = site_value(tree, &site).cloned();
                (site, value)
            })
            .collect();
        let values = sites
            .iter()
            .flat_map(|(_, value)| match value {
                Some(Value::List(values)) => values.clone(),
                Some(value) => vec![value.clone()],
                None => Vec::new(),
            })
            .collect();
        columns.push(KeyedPart { sites, values });
    }
    let length = columns.first().map_or(0, |part| part.values.len());
    if columns.iter().any(|part| part.values.len() != length)
        || (length == 0 && !compiled.reference.nullable)
    {
        return Err(());
    }
    let mut rows = Vec::new();
    let mut tuples = Vec::new();
    for index in 0..length {
        let tuple: Vec<_> = columns
            .iter()
            .map(|part| part.values[index].clone())
            .collect();
        let present = !keyed
            || match compiled.reference.null_match {
                panproto_schema::NullMatch::Simple => {
                    tuple.iter().all(|value| *value != Value::Null)
                }
                panproto_schema::NullMatch::Full => tuple.iter().any(|value| *value != Value::Null),
            };
        if present {
            rows.push(index);
            tuples.push(tuple);
        }
    }
    Ok(Some(KeyedWord {
        parts: columns,
        rows,
        tuples,
    }))
}

/// Requested tuples paired with their original raw rows; appends have no row.
fn edited_keyed_rows(
    word: &KeyedWord,
    edits: &LiteralEdits<'_>,
) -> Result<Vec<RequestedRow>, EdgeId> {
    let mut requested: Vec<_> = word
        .rows
        .iter()
        .copied()
        .zip(word.tuples.iter().cloned())
        .map(|(row, tuple)| (Some(row), tuple))
        .collect();
    for (index, carrier) in &edits.removed {
        let index = usize::try_from(*index).map_err(|_| *carrier)?;
        if index >= requested.len() {
            return Err(*carrier);
        }
        requested.remove(index);
    }
    for (index, carrier, literal) in &edits.rewritten {
        let tuple = match (word.parts.len(), *literal) {
            (1, value) => vec![value.clone()],
            (count, Value::List(values)) if values.len() == count => values.clone(),
            _ => return Err(*carrier),
        };
        let index = usize::try_from(*index).map_err(|_| *carrier)?;
        if index == requested.len() {
            requested.push((None, tuple));
        } else if let Some((_, old)) = requested.get_mut(index) {
            *old = tuple;
        } else {
            return Err(*carrier);
        }
    }
    Ok(requested)
}

/// Reconstruct one original multi-site part, preserving unobserved null slots.
fn partitioned_part(
    part: &KeyedPart,
    column: usize,
    original_rows: &BTreeSet<usize>,
    retained: &BTreeMap<usize, &Vec<Value>>,
) -> Result<Vec<SiteReplacement>, ()> {
    let mut replacements = Vec::new();
    let mut row = 0usize;
    for (site, value) in &part.sites {
        let Some(value) = value else { continue };
        let replacement = match value {
            Value::List(values) => {
                let mut result = Vec::new();
                for value in values {
                    if let Some(tuple) = retained.get(&row) {
                        result.push(tuple[column].clone());
                    } else if !original_rows.contains(&row) {
                        result.push(value.clone());
                    }
                    row += 1;
                }
                Value::List(result)
            }
            value => {
                let replacement = if let Some(tuple) = retained.get(&row) {
                    tuple[column].clone()
                } else if original_rows.contains(&row) {
                    return Err(());
                } else {
                    value.clone()
                };
                row += 1;
                replacement
            }
        };
        replacements.push((site.clone(), replacement));
    }
    Ok(replacements)
}

/// Gather compatible writes before mutating an aliased site.
fn keyed_replacements(
    word: &KeyedWord,
    requested: &[(Option<usize>, Vec<Value>)],
) -> Result<BTreeMap<TreeValueSite, Value>, ()> {
    let single = word.parts.iter().all(|part| part.sites.len() == 1);
    let original_rows = word.rows.iter().copied().collect();
    let retained: BTreeMap<_, _> = requested
        .iter()
        .filter_map(|(row, tuple)| row.map(|row| (row, tuple)))
        .collect();
    if !single && requested.iter().any(|(row, _)| row.is_none()) {
        return Err(());
    }
    let mut writes: BTreeMap<TreeValueSite, Value> = BTreeMap::new();
    for (column, part) in word.parts.iter().enumerate() {
        let replacements = if single {
            vec![(
                part.sites[0].0.clone(),
                Value::List(
                    requested
                        .iter()
                        .map(|(_, tuple)| tuple[column].clone())
                        .collect(),
                ),
            )]
        } else {
            partitioned_part(part, column, &original_rows, &retained)?
        };
        for (site, value) in replacements {
            if writes
                .get(&site)
                .is_some_and(|previous| !previous.canonical_cmp(&value).is_eq())
            {
                return Err(());
            }
            writes.insert(site, value);
        }
    }
    Ok(writes)
}

/// Equal words retain exact sites; changed words follow their source partition.
fn write_keyed_word(
    tree: &mut WInstance,
    holder: u32,
    edits: &LiteralEdits<'_>,
    original: Result<Option<KeyedWord>, ()>,
) -> Result<bool, EdgeId> {
    let carrier = edits
        .removed
        .first()
        .map(|(_, carrier)| *carrier)
        .or_else(|| edits.rewritten.first().map(|(_, carrier, _)| *carrier));
    let Some(carrier) = carrier else {
        return Ok(false);
    };
    let Some(word) = original.map_err(|()| carrier)? else {
        return Ok(false);
    };
    let requested = edited_keyed_rows(&word, edits)?;
    let current = keyed_word(tree, holder, edits.compiled)
        .map_err(|()| carrier)?
        .ok_or(carrier)?;
    let unchanged = current.tuples.len() == requested.len()
        && current
            .tuples
            .iter()
            .zip(&requested)
            .all(|(left, (_, right))| {
                // Both constructors enforce the compiled part count.
                left.iter()
                    .zip(right)
                    .all(|(left, right)| left.canonical_cmp(right).is_eq())
            });
    if !unchanged {
        let writes = keyed_replacements(&word, &requested).map_err(|()| carrier)?;
        for ((node, field), value) in writes {
            let node = tree.nodes.get_mut(&node).ok_or(carrier)?;
            if let Some(field) = field {
                node.extra_fields.insert(field, value);
            } else {
                node.value = Some(FieldPresence::Present(value));
            }
        }
    }
    Ok(true)
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
            end_form,
            origin,
            ..
        } = &compiled.form
        else {
            return Err(unrepresentable(compiled, ViewRef::Node(holder)));
        };
        let (id, local, value) = self.resolution_text_site(compiled, holder, text, base)?;
        if id != span.text {
            return Err(unrepresentable(compiled, ViewRef::Node(holder)));
        }
        let bytes = if local {
            match value {
                Value::Str(text) => text.into_bytes(),
                Value::Bytes(bytes) => bytes,
                _ => return Err(unrepresentable(compiled, ViewRef::Node(holder))),
            }
        } else {
            let store = ctx.external_text.ok_or_else(|| {
                Refusal::Operational(ResolveError::UnsupportedFeature {
                    provider: Some(self.spec.provider.clone()),
                    feature: super::ResolutionFeature::ExternalText,
                })
            })?;
            store.load(&value)?
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
        let (start, end) =
            standoff::offsets_to_form(start, end, *origin, *end_form).map_err(violation)?;
        let int = |offset: u64| {
            i64::try_from(offset)
                .map(Value::Int)
                .map_err(|_| violation(standoff::UnitFailure::OutOfRange))
        };
        Ok(Value::List(vec![int(start)?, int(end)?]))
    }

    fn resolution_text_site(
        &self,
        compiled: &CompiledRef,
        holder: u32,
        text: &RefTextSource,
        base: &WInstance,
    ) -> Result<(ResolutionTextId, bool, Value), Refusal> {
        let source = Source::tree(base);
        let (path, local) = match text {
            RefTextSource::Local(path) => (path, true),
            RefTextSource::External(path) => (path, false),
            RefTextSource::Referenced { .. } => {
                return Err(Refusal::Operational(ResolveError::UnsupportedFeature {
                    provider: Some(self.spec.provider.clone()),
                    feature: super::ResolutionFeature::ReferencedText,
                }));
            }
            RefTextSource::Span { .. } => {
                return Err(Refusal::Operational(ResolveError::UnsupportedFeature {
                    provider: Some(self.spec.provider.clone()),
                    feature: super::ResolutionFeature::SpanText,
                }));
            }
            RefTextSource::ExternalUri { .. } => {
                return Err(Refusal::Operational(ResolveError::UnsupportedFeature {
                    provider: Some(self.spec.provider.clone()),
                    feature: super::ResolutionFeature::ExternalUriText,
                }));
            }
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
        Ok((id, local, (*value).clone()))
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
    use panproto_inst::segment::IdKind;

    use super::{holder_ids_from, merge_fans, tree_node_floor};
    use crate::resolution::error::ResolveError;

    #[test]
    fn appended_indices_include_each_definition_position() {
        use panproto_inst::resolution_complement::{
            RefOccurrence, ResolutionComplement, UnresolvedReason, UnresolvedRef,
        };
        use panproto_inst::{EdgeId, Value, ViewRef};
        let record = |holder, reference: &str, index| RefOccurrence {
            holder: ViewRef::Node(holder),
            reference: reference.into(),
            index,
            literal: Value::Str(format!("name-{index}")),
            form: None,
            context: None,
        };
        let resolved = record(1, "variable", 1);
        let mut classified = super::Classified::default();
        classified.occurrences.insert(EdgeId(7), &resolved);
        let mut complement = ResolutionComplement {
            definition_occurrences: vec![
                record(1, "variable", 0),
                record(1, "variable", 2),
                record(2, "variable", 0),
                record(1, "other", 0),
            ],
            ..ResolutionComplement::default()
        };
        for reason in [
            UnresolvedReason::Dangling,
            UnresolvedReason::Unselected { value: Value::Null },
        ] {
            complement.unresolved.push(UnresolvedRef {
                holder: ViewRef::Node(1),
                reference: "variable".into(),
                index: 3,
                literal: Value::Str("missing".into()),
                reason,
                context: None,
            });
        }
        assert_eq!(
            super::appended_index_count(1, &"variable".into(), &classified, &complement, &[]).ok(),
            Some(4)
        );
        classified.occurrences.clear();
        assert_eq!(
            super::appended_index_count(1, &"variable".into(), &classified, &complement, &[]).ok(),
            Some(3)
        );
    }

    const TOP: u32 = u32::MAX;

    fn exhausted<T>(result: &Result<T, ResolveError>) -> bool {
        matches!(
            result,
            Err(ResolveError::IdExhausted { kind: IdKind::Node })
        )
    }

    #[test]
    fn a_source_holding_the_top_id_records_the_floor_top() {
        assert_eq!(tree_node_floor([0, TOP].into_iter()), TOP);
        assert_eq!(tree_node_floor([0, TOP - 1].into_iter()), TOP);
        assert_eq!(tree_node_floor(std::iter::empty()), 0);
    }

    #[test]
    fn holder_ids_withhold_the_top_id() {
        assert_eq!(holder_ids_from(TOP - 1, 1).ok(), Some(vec![TOP - 1]));
        assert!(exhausted(&holder_ids_from(TOP - 1, 2)));
        assert_eq!(holder_ids_from(TOP, 0).ok(), Some(Vec::new()));
        assert!(exhausted(&holder_ids_from(TOP, 1)));
    }

    /// At every floor in the last four ids, `count` holders take the ids
    /// from the floor up when they all lie below `u32::MAX`.
    #[test]
    fn holder_ids_are_exhaustive_at_the_top() {
        for floor in (TOP - 3)..=TOP {
            for count in 0..=4_u32 {
                let ids = holder_ids_from(floor, usize::try_from(count).unwrap_or(0));
                if count <= TOP - floor {
                    assert_eq!(ids.ok(), Some((floor..floor + count).collect()));
                } else {
                    assert!(exhausted(&ids), "{count} holders at floor {floor}");
                }
            }
        }
    }

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

#[cfg(test)]
mod dependency_contract_tests {
    #![allow(clippy::expect_used)]

    use std::collections::HashMap;

    use panproto_inst::{
        FieldPresence, GInstance, Node, NodeShape, RowId, Value, ViewRef, WInstance,
    };
    use panproto_schema::{
        Cardinality, Edge, NullMatch, OnUnresolved, PathSyntax, RefSource, Reference, ResolveMode,
    };

    use super::{
        CompiledRef, RefForm, Refusal, ResolveDomainError, holder_arc, read_table, remove_literal,
        write_part,
    };

    fn label(name: &str) -> Edge {
        Edge {
            src: "holder".into(),
            tgt: "part".into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    fn sites(values: Vec<Value>) -> WInstance {
        let mut nodes = HashMap::from([(0, Node::new(0, "holder"))]);
        let mut arcs = Vec::new();
        for (index, value) in values.into_iter().enumerate() {
            let id = u32::try_from(index + 1).expect("small fixture");
            nodes.insert(
                id,
                Node::new(id, "part").with_value(FieldPresence::Present(value)),
            );
            arcs.push((0, id, label("value")));
        }
        WInstance::new(nodes, arcs, vec![], 0, "holder".into())
    }

    fn path_reference(cardinality: Cardinality) -> CompiledRef {
        let field = vec![label("value")];
        CompiledRef {
            reference: Reference {
                id: "reference".into(),
                holder: "holder".into(),
                source: RefSource::Path {
                    field: field.clone(),
                    syntax: PathSyntax::JsonPointer,
                },
                target_vertex: "target".into(),
                target_key: None,
                edge: None,
                cardinality,
                nullable: false,
                on_unresolved: OnUnresolved::Complement,
                mode: ResolveMode::Redirect,
                guard: None,
                null_match: NullMatch::default(),
                null_vertex: None,
                scope_via: None,
            },
            form: RefForm::Path { field },
        }
    }

    fn lists(tree: &WInstance) -> Vec<Value> {
        (1..u32::try_from(tree.nodes.len()).expect("small fixture"))
            .map(|id| {
                let Some(FieldPresence::Present(value)) = &tree.nodes[&id].value else {
                    panic!("fixture site lost its value")
                };
                value.clone()
            })
            .collect()
    }

    #[test]
    fn many_part_writes_preserve_site_boundaries_and_append_only_at_end() {
        let original = vec![
            Value::List(vec![Value::Int(10), Value::Int(11)]),
            Value::List(vec![Value::Int(12), Value::Int(13)]),
        ];
        for index in 0_u32..4 {
            let mut tree = sites(original.clone());
            assert_eq!(
                write_part(
                    &mut tree,
                    0,
                    &[label("value")],
                    true,
                    index,
                    &Value::Int(99)
                ),
                Ok(())
            );
            let mut expected = original.clone();
            let Value::List(items) = &mut expected[usize::try_from(index / 2).expect("site")]
            else {
                unreachable!()
            };
            items[usize::try_from(index % 2).expect("slot")] = Value::Int(99);
            assert_eq!(lists(&tree), expected, "index {index}");
        }
        let mut multiple = sites(original.clone());
        assert_eq!(
            write_part(
                &mut multiple,
                0,
                &[label("value")],
                true,
                4,
                &Value::Int(99)
            ),
            Err(())
        );
        assert_eq!(lists(&multiple), original);
        for initial in [Value::Int(10), Value::List(vec![Value::Int(10)])] {
            let mut tree = sites(vec![initial.clone()]);
            assert_eq!(
                write_part(&mut tree, 0, &[label("value")], true, 2, &Value::Int(99)),
                Err(())
            );
            assert_eq!(lists(&tree), vec![initial]);
            assert_eq!(
                write_part(&mut tree, 0, &[label("value")], true, 1, &Value::Int(99)),
                Ok(())
            );
            assert_eq!(
                lists(&tree),
                vec![Value::List(vec![Value::Int(10), Value::Int(99)])]
            );
        }
    }

    #[test]
    fn many_literal_removal_uses_the_original_partitioned_index() {
        let original = vec![
            Value::List(vec![Value::Int(10), Value::Int(11)]),
            Value::List(vec![Value::Int(12), Value::Int(13)]),
        ];
        let compiled = path_reference(Cardinality::Many);
        for index in 0_u32..4 {
            let mut tree = sites(original.clone());
            assert_eq!(remove_literal(&mut tree, 0, &compiled, index), Ok(()));
            let mut expected = original.clone();
            let Value::List(items) = &mut expected[usize::try_from(index / 2).expect("site")]
            else {
                unreachable!()
            };
            items.remove(usize::try_from(index % 2).expect("slot"));
            assert_eq!(lists(&tree), expected, "index {index}");
        }
        let mut tree = sites(original.clone());
        assert_eq!(remove_literal(&mut tree, 0, &compiled, 4), Err(()));
        assert_eq!(lists(&tree), original);
        assert_eq!(
            remove_literal(&mut tree, 0, &path_reference(Cardinality::One), 0),
            Err(())
        );
        assert_eq!(lists(&tree), original);
        let mut scalar = sites(vec![Value::Int(10)]);
        assert_eq!(remove_literal(&mut scalar, 0, &compiled, 0), Err(()));
        assert_eq!(lists(&scalar), vec![Value::Int(10)]);
    }

    #[test]
    fn table_reading_rejects_each_nonrow_metadata_field() -> Result<(), panproto_inst::InstError> {
        let fields = HashMap::from([("payload".to_owned(), Value::List(vec![Value::Int(4)]))]);
        for fault in 0..=5 {
            let mut node = Node::new(7, "rows");
            node.extra_fields = fields.clone();
            match fault {
                0 => {}
                1 => node.value = Some(FieldPresence::Null),
                2 => node.discriminator = Some("tag".into()),
                3 => node.position = Some(0),
                4 => node.shape = NodeShape::List,
                5 => {
                    node.annotations
                        .insert("annotation".to_owned(), Value::Int(1));
                }
                _ => unreachable!(),
            }
            let mut graph = GInstance::new();
            graph.insert_node(node)?;
            match read_table(&graph) {
                Ok(table) => {
                    assert_eq!(fault, 0);
                    assert_eq!(table.row(RowId(7)), Some(&fields));
                }
                Err(Refusal::Domain(violations)) => {
                    assert_ne!(fault, 0);
                    assert_eq!(
                        violations.into_vec(),
                        vec![ResolveDomainError::NoPlacement {
                            carrier: ViewRef::Node(7)
                        }]
                    );
                }
                Err(other) => panic!("unexpected table refusal: {other:?}"),
            }
        }
        Ok(())
    }

    #[test]
    fn holder_attachment_selects_the_matching_label_among_multiple_arcs() {
        let first = label("first");
        let second = label("second");
        assert_eq!(holder_arc(&[&first, &second], &first), Some(first.clone()));
        assert_eq!(
            holder_arc(&[&first, &second], &second),
            Some(second.clone())
        );
        assert_eq!(holder_arc(&[&first, &second], &label("absent")), None);
        assert_eq!(holder_arc(&[&first], &second), Some(first));
        assert_eq!(holder_arc(&[], &second), None);
    }

    #[test]
    fn shifted_positions_count_only_strictly_earlier_matching_records() {
        let compiled = path_reference(Cardinality::Many);
        let mut other = compiled.clone();
        other.reference.id = "other".into();
        let removed = [
            (0, &compiled, 0),
            (0, &compiled, 1),
            (0, &compiled, 3),
            (0, &compiled, 5),
            (1, &compiled, 0),
            (0, &other, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (holder, compiled, index))| super::Removal {
            carrier: panproto_inst::EdgeId(u32::try_from(id).expect("small fixture")),
            holder,
            compiled,
            index,
        })
        .collect();
        let parts = super::TreeParts {
            structural: GInstance::new(),
            live: vec![],
            holders: vec![],
            removed,
            node_floor: 1,
            slot_floor: 0,
        };
        // Raw records can share a coordinate. Equality is not an earlier position.
        assert_eq!(parts.shifted(0, &"reference".into(), 3), 1);
        assert_eq!(parts.shifted(0, &"absent".into(), 3), 3);
    }

    #[test]
    fn appended_count_excludes_other_holders_and_other_declarations() {
        let compiled = path_reference(Cardinality::Many);
        let mut other = compiled.clone();
        other.reference.id = "other".into();
        let written: Vec<_> = [(0, &compiled), (1, &compiled), (0, &other)]
            .into_iter()
            .map(|(holder, compiled)| super::Written {
                carrier: panproto_inst::EdgeId(7),
                holder,
                compiled,
                index: 0,
                target: 2,
                forms: vec![],
                rewrite: false,
            })
            .collect();
        assert_eq!(
            super::appended_index_count(
                0,
                &"reference".into(),
                &super::Classified::default(),
                &super::ResolutionComplement::default(),
                &written
            )
            .ok(),
            Some(1)
        );
    }

    #[test]
    fn fan_slots_require_distinct_increasing_coordinates() {
        use panproto_inst::Fan;
        let retained = vec![Fan::new("fan", 1), Fan::new("fan", 3)];
        let fragment = vec![Fan::new("fan", 2)];
        for slots in [[10, 10], [11, 10], [9, 11], [10, 13]] {
            assert_eq!(
                super::merge_fans(retained.clone(), &slots, fragment.clone(), 10),
                vec![
                    retained[0].clone(),
                    retained[1].clone(),
                    fragment[0].clone()
                ]
            );
        }
        assert_eq!(
            super::merge_fans(retained.clone(), &[10, 12], fragment.clone(), 10),
            vec![
                retained[0].clone(),
                fragment[0].clone(),
                retained[1].clone()
            ]
        );
    }

    fn two_part_reference() -> CompiledRef {
        let mut compiled = path_reference(Cardinality::Many);
        compiled.form = RefForm::KeyValue {
            key: panproto_schema::Key {
                id: "key".into(),
                parts: Vec::new(),
                role: panproto_schema::KeyRole::Identifier,
                scope: panproto_schema::KeyScope::Global,
                equality: Vec::new(),
            },
            parts: vec![vec![label("value")], vec![label("other")]],
        };
        compiled
    }

    #[test]
    fn composite_literal_writes_reject_short_and_long_provider_tuples() {
        let compiled = two_part_reference();
        let original = sites(vec![Value::List(vec![Value::Int(1)])]);
        for width in [0, 1, 3] {
            let literal = Value::List(vec![Value::Int(9); width]);
            let mut tree = original.clone();
            assert_eq!(
                super::write_literal(&mut tree, 0, &compiled, 0, &literal),
                Err(())
            );
            assert_eq!(lists(&tree), lists(&original));
            let word = super::KeyedWord {
                parts: (0..2)
                    .map(|_| super::KeyedPart {
                        sites: Vec::new(),
                        values: Vec::new(),
                    })
                    .collect(),
                rows: vec![0],
                tuples: vec![vec![Value::Int(1), Value::Int(2)]],
            };
            let edits = super::LiteralEdits {
                compiled: &compiled,
                removed: Vec::new(),
                rewritten: vec![(0, panproto_inst::EdgeId(7), &literal)],
            };
            assert_eq!(
                super::edited_keyed_rows(&word, &edits),
                Err(panproto_inst::EdgeId(7))
            );
        }
    }

    #[test]
    fn keyed_columns_require_equal_lengths_and_nonempty_nonnullable_words() {
        let mut compiled = two_part_reference();
        let unequal = sites(vec![Value::List(vec![Value::Int(1)])]);
        assert!(super::keyed_word(&unequal, 0, &compiled).is_err());
        compiled.reference.nullable = true;
        assert!(super::keyed_word(&unequal, 0, &compiled).is_err());
        let empty = sites(Vec::new());
        let word = super::keyed_word(&empty, 0, &compiled)
            .expect("nullable empty word")
            .expect("Many keyed word");
        assert_eq!(word.rows, Vec::<usize>::new());
        assert_eq!(word.tuples, Vec::<Vec<Value>>::new());
        compiled.reference.nullable = false;
        assert!(super::keyed_word(&empty, 0, &compiled).is_err());
    }

    #[test]
    fn added_occurrences_require_both_elements_holder_anchor_and_capacity()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::resolution_complement::{RefOccurrence, ResolutionComplement};
        use panproto_inst::{EdgeId, GEdge};
        use std::collections::BTreeSet;
        let mut resolution =
            crate::resolution::verify::dependency_contract_tests::helper_resolution(
                ResolveMode::AddEdge,
            )?;
        let mut compiled = two_part_reference();
        compiled.reference.mode = ResolveMode::AddEdge;
        let RefForm::KeyValue { key, parts } = &mut compiled.form else {
            unreachable!()
        };
        key.parts = vec![panproto_schema::KeyPart::Path(Vec::new())];
        *parts = vec![vec![label("value")]];
        let edge_label = label("reference");
        resolution
            .spec
            .reference_writers
            .insert(edge_label.clone(), "reference".into());
        let base = sites(vec![Value::Int(42)]);
        let context = super::ResolveContext::empty();
        let complement = ResolutionComplement::default();
        let record = RefOccurrence {
            holder: ViewRef::Node(0),
            reference: "reference".into(),
            index: 0,
            literal: Value::Int(42),
            form: None,
            context: None,
        };
        for bits in 0_u8..32 {
            let source_element = bits & 1 != 0;
            let target_element = bits & 2 != 0;
            let matching_holder = bits & 4 != 0;
            let occupied = bits & 8 != 0;
            let many = bits & 16 != 0;
            compiled.reference.cardinality = if many {
                Cardinality::Many
            } else {
                Cardinality::One
            };
            resolution
                .compiled
                .insert("reference".into(), compiled.clone());
            let mut view = GInstance::new();
            view.insert_node(Node::new(
                0,
                if matching_holder { "holder" } else { "other" },
            ))?;
            view.insert_node(Node::new(1, "part"))?;
            view.insert_edge(EdgeId(7), GEdge::new(0, 1, edge_label.clone()))?;
            let mut classified = super::Classified {
                elements: BTreeSet::new(),
                new_references: vec![(EdgeId(7), edge_label.clone())],
                ..super::Classified::default()
            };
            if source_element {
                classified.elements.insert(0);
            }
            if target_element {
                classified.elements.insert(1);
            }
            if occupied {
                classified.occurrences.insert(EdgeId(8), &record);
            }
            let canvas = super::Canvas {
                base: &base,
                view: &view,
                ctx: &context,
            };
            let admitted =
                source_element && target_element && matching_holder && (many || !occupied);
            match resolution.added_occurrences(&canvas, &classified, &complement) {
                Ok(written) => {
                    assert!(admitted, "case {bits}");
                    assert_eq!(written.len(), 1);
                    assert_eq!(written[0].index, u32::from(occupied));
                    assert_eq!(written[0].forms, vec![Value::Int(42)]);
                }
                Err(Refusal::Domain(violations)) => {
                    assert!(!admitted, "case {bits}");
                    assert_eq!(
                        violations.into_vec(),
                        vec![ResolveDomainError::UnrepresentableReference {
                            carrier: ViewRef::Edge(EdgeId(7)),
                            reference: "reference".into()
                        }]
                    );
                }
                Err(Refusal::Operational(error)) => return Err(error.into()),
            }
        }
        Ok(())
    }

    #[test]
    fn standoff_rendering_preserves_byte_string_offsets() -> Result<(), Box<dyn std::error::Error>>
    {
        use panproto_inst::resolution_complement::{
            ResolutionSpan, ResolutionTextId, ResolutionValueSite,
        };
        let resolution = crate::resolution::verify::dependency_contract_tests::helper_resolution(
            ResolveMode::Redirect,
        )?;
        let mut compiled = path_reference(Cardinality::One);
        compiled.form = RefForm::Standoff {
            key: None,
            text: panproto_schema::RefTextSource::Local(vec![label("value")]),
            start: Vec::new(),
            end: Vec::new(),
            unit: panproto_schema::OffsetUnit::Byte,
            end_inclusive: false,
            end_form: panproto_schema::SpanEnd::Offset,
            origin: 0,
        };
        let base = sites(vec![Value::Bytes(vec![0x61, 0xff, 0x62, 0x63])]);
        let span = ResolutionSpan {
            text: ResolutionTextId::Local(ResolutionValueSite::Own(ViewRef::Node(1))),
            start: 1,
            end: 3,
        };
        let result =
            resolution.render_standoff(&compiled, 0, &span, &base, &super::ResolveContext::empty());
        assert!(
            matches!(result, Ok(ref value) if *value == Value::List(vec![Value::Int(1), Value::Int(3)])),
            "{result:?}"
        );
        Ok(())
    }
}
