//! The resolution lens from a tree to a graph, for key-value references.
//!
//! Get lays the tree out as a graph and resolves each occurrence of a
//! reference at a holder: the occurrence's key tuple is looked up among the
//! targets of the reference's target key (a `Within` key's candidates
//! coming only from the holder's scope bucket). A reference of cardinality
//! `One` has one occurrence per holder whose parts all have a value; one of
//! cardinality `Many` has the positional zip of its parts' value sequences,
//! one occurrence per position. A resolved occurrence of an `AddEdge`
//! reference adds the reference's edge from the holder to the target; one
//! of a `Redirect` reference removes the holder's subtree (the fragment
//! goes to the complement) and adds the edge from the holder's parent to
//! the target. Resolution edges take ids above the graph's in ascending
//! (holder, reference, occurrence) order. Put removes the resolution edges,
//! restores the fragments of the redirected occurrences the view keeps,
//! writes each occurrence's key tuple (a kept occurrence keeps its literal,
//! a reconnected one takes its new target's key, a removed one is removed
//! from its holder's sequence), reads the result back as the tree with the
//! recorded root, and requires every occurrence to re-resolve to exactly
//! its target.
//!
//! Path, Variable and Standoff sources resolve through a provider or a
//! binder that the reference model does not hold; a schema declaring one is
//! refused with [`ResolveError::UnsupportedSource`].

use std::collections::{BTreeMap, BTreeSet};

use crate::encode::{schema_parent_name, tree_to_graph};
use crate::keys;
use crate::lens::ViewRef;
use crate::model::{GEdge, Graph, Name, Node, NodeShape, SEdge, Tree};
use crate::schema::{
    Cardinality, KeyScope, OnUnresolved, RefSource, Reference, ResolveMode, Schema,
};
use crate::value::{Presence, Value};

/// A recorded occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    /// Holder node.
    pub holder: u32,
    /// Reference id.
    pub reference: Name,
    /// Occurrence index (the position in a `Many` sequence; 0 for `One`).
    pub index: u32,
    /// The literal (one value, or the list of the parts' values).
    pub literal: Value,
}

/// Why an occurrence did not resolve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unresolved {
    /// No target.
    Dangling,
    /// Several targets.
    Ambiguous(Vec<u32>),
}

/// A subtree a redirected occurrence removed: its nodes (fan nodes
/// included) and every edge that the removal took out of the graph, by id,
/// with the parent the subtree hung from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    /// The node the fragment's root hung from.
    pub parent: u32,
    /// The removed nodes.
    pub nodes: BTreeMap<u32, Node>,
    /// The removed edges: the attachment arc, the arcs inside, and the
    /// edges of every fan touching the fragment.
    pub edges: BTreeMap<u32, GEdge>,
}

/// The resolution complement.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ResComplement {
    /// Resolved occurrences by their edge id.
    pub resolved: BTreeMap<u32, Occurrence>,
    /// The target each resolved occurrence's edge reached at get.
    pub targets: BTreeMap<u32, u32>,
    /// Unresolved occurrences with their reason, in (holder, reference,
    /// index) order.
    pub unresolved: Vec<(Occurrence, Unresolved)>,
    /// The fragments of redirected holders, by their root.
    pub redirected: BTreeMap<u32, Fragment>,
    /// The tree's root.
    pub root: u32,
}

/// A resolution refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// A strict occurrence with no target.
    DanglingReference {
        /// Reference.
        reference: Name,
        /// Holder.
        at: ViewRef,
        /// Key.
        key: Vec<Value>,
    },
    /// A strict occurrence with several targets.
    AmbiguousReference {
        /// Reference.
        reference: Name,
        /// Holder.
        at: ViewRef,
        /// Key.
        key: Vec<Value>,
        /// Candidates.
        candidates: Vec<ViewRef>,
    },
    /// The parts of a `Many` occurrence vector have unequal lengths, or a
    /// non-nullable one has none.
    ReferenceCardinalityMismatch {
        /// Reference.
        reference: Name,
        /// Holder.
        at: ViewRef,
        /// The length of each part's sequence.
        lengths: Vec<u64>,
    },
    /// A reference whose source the reference model does not resolve.
    UnsupportedSource {
        /// Reference.
        reference: Name,
    },
    /// A resolved target lies in a subtree a redirect removed.
    TargetRemoved {
        /// Reference.
        reference: Name,
        /// Holder.
        at: ViewRef,
    },
    /// A written occurrence that does not re-resolve to its target, or a
    /// view edge no reference writes.
    UnrepresentableReference {
        /// The edge.
        carrier: ViewRef,
        /// The reference, when one writes the edge.
        reference: Option<Name>,
    },
    /// The tree has no graph form or the result is no tree.
    NotATree,
}

/// The target key of a reference.
fn target_key<'a>(schema: &'a Schema, reference: &Reference) -> Option<&'a crate::schema::Key> {
    schema
        .keys
        .get(&reference.target_vertex)?
        .iter()
        .find(|k| Some(&k.id) == reference.target_key.as_ref())
}

/// The candidates of an occurrence: targets whose target-key value is the
/// key, from the holder's scope bucket for a `Within` key (none when the
/// holder has no such ancestor).
fn candidates(
    schema: &Schema,
    graph: &Graph,
    reference: &Reference,
    holder: u32,
    key: &[Value],
) -> Vec<u32> {
    let Some(tkey) = target_key(schema, reference) else {
        return Vec::new();
    };
    let scope = match &tkey.scope {
        KeyScope::Within(vertex) => match keys::nearest_ancestor(graph, holder, vertex) {
            Some(anchor) => Some((vertex.clone(), anchor)),
            None => return Vec::new(),
        },
        _ => None,
    };
    graph
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.anchor == reference.target_vertex && node.shape != NodeShape::HyperEdge
        })
        .filter(|(id, _)| keys::key_value(graph, **id, tkey).as_deref() == Some(key))
        .filter(|(id, _)| {
            scope.as_ref().is_none_or(|(vertex, anchor)| {
                keys::nearest_ancestor(graph, **id, vertex) == Some(*anchor)
            })
        })
        .map(|(id, _)| *id)
        .collect()
}

fn literal(values: &[Value]) -> Value {
    match values {
        [one] => one.clone(),
        many => Value::List(many.to_vec()),
    }
}

/// A part's value as a sequence: a list's items, one value, or none.
fn sequence(v: Option<Value>) -> Vec<Value> {
    match v {
        None => Vec::new(),
        Some(Value::List(items)) => items,
        Some(other) => vec![other],
    }
}

/// The key tuples of a reference at a holder, one per occurrence: for
/// `One`, the parts' values when every part has one (else none); for
/// `Many`, the positional zip of the parts' sequences.
///
/// # Errors
///
/// [`ResolveError::ReferenceCardinalityMismatch`] when the sequences of a
/// `Many` reference have unequal lengths, or all have length 0 and the
/// reference is not nullable.
fn tuples(g: &Graph, holder: u32, r: &Reference) -> Result<Vec<Vec<Value>>, ResolveError> {
    let RefSource::KeyValue { parts } = &r.source else {
        return Ok(Vec::new());
    };
    match r.cardinality {
        Cardinality::One => Ok(parts
            .iter()
            .map(|p| keys::path_value(g, holder, p))
            .collect::<Option<Vec<Value>>>()
            .into_iter()
            .collect()),
        Cardinality::Many => {
            let seqs: Vec<Vec<Value>> = parts
                .iter()
                .map(|p| sequence(keys::path_value(g, holder, p)))
                .collect();
            let lengths: Vec<u64> = seqs
                .iter()
                .map(|s| u64::try_from(s.len()).unwrap_or(u64::MAX))
                .collect();
            let n = seqs.first().map_or(0, Vec::len);
            let equal = seqs.iter().all(|s| s.len() == n);
            if !equal || (n == 0 && !r.nullable && !parts.is_empty()) {
                return Err(ResolveError::ReferenceCardinalityMismatch {
                    reference: r.id.clone(),
                    at: ViewRef::Node(holder),
                    lengths,
                });
            }
            Ok((0..n)
                .map(|i| seqs.iter().map(|s| s[i].clone()).collect())
                .collect())
        }
    }
}

/// One resolved occurrence waiting for its edge.
struct Resolved {
    occurrence: Occurrence,
    target: u32,
    redirect: bool,
}

/// The non-fan nodes of the subtree at `root`, following arcs.
fn subtree(g: &Graph, root: u32) -> BTreeSet<u32> {
    let fan = |n: u32| {
        g.nodes
            .get(&n)
            .is_some_and(|x| x.shape == NodeShape::HyperEdge)
    };
    let mut seen = BTreeSet::from([root]);
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        for e in g.edges.values() {
            if e.src == n && !fan(e.src) && !fan(e.tgt) && seen.insert(e.tgt) {
                stack.push(e.tgt);
            }
        }
    }
    seen
}

/// The parent of a non-fan node along its arc.
fn parent_of(g: &Graph, n: u32) -> Option<u32> {
    g.edges
        .values()
        .find(|e| {
            e.tgt == n
                && g.nodes
                    .get(&e.src)
                    .is_some_and(|x| x.shape != NodeShape::HyperEdge)
        })
        .map(|e| e.src)
}

/// Remove the subtree at `root` with its attachment arc and every fan
/// touching it, returning the fragment; `None` for the root of the tree.
fn capture(g: &mut Graph, root: u32) -> Option<Fragment> {
    let parent = parent_of(g, root)?;
    let inside = subtree(g, root);
    let fans: BTreeSet<u32> = g
        .nodes
        .iter()
        .filter(|(_, n)| n.shape == NodeShape::HyperEdge)
        .map(|(i, _)| *i)
        .filter(|f| {
            g.edges
                .values()
                .any(|e| e.src == *f && inside.contains(&e.tgt))
        })
        .collect();
    let edges: BTreeMap<u32, GEdge> = g
        .edges
        .iter()
        .filter(|(_, e)| {
            inside.contains(&e.tgt) || inside.contains(&e.src) || fans.contains(&e.src)
        })
        .map(|(i, e)| (*i, e.clone()))
        .collect();
    let nodes: BTreeMap<u32, Node> = g
        .nodes
        .iter()
        .filter(|(i, _)| inside.contains(i) || fans.contains(i))
        .map(|(i, n)| (*i, n.clone()))
        .collect();
    for id in edges.keys() {
        g.edges.remove(id);
    }
    for id in nodes.keys() {
        g.nodes.remove(id);
    }
    Some(Fragment {
        parent,
        nodes,
        edges,
    })
}

/// Resolution get.
///
/// # Errors
///
/// [`ResolveError::UnsupportedSource`] for a reference whose source is not
/// key-value; [`ResolveError::NotATree`] when the tree has no graph form;
/// the first [`ResolveError::ReferenceCardinalityMismatch`] or strict
/// unresolved occurrence in (holder, reference, index) order; and
/// [`ResolveError::TargetRemoved`] for a target inside a redirected
/// subtree.
pub fn get(
    schema: &Schema,
    t: &Tree,
    lenient: bool,
) -> Result<(Graph, ResComplement), ResolveError> {
    if let Some(r) = schema
        .references
        .values()
        .find(|r| !matches!(r.source, RefSource::KeyValue { .. }))
    {
        return Err(ResolveError::UnsupportedSource {
            reference: r.id.clone(),
        });
    }
    let (mut g, _) = tree_to_graph(t, Some(schema)).map_err(|_| ResolveError::NotATree)?;
    let base = g.clone();
    let mut c = ResComplement {
        root: t.root,
        ..ResComplement::default()
    };
    let mut resolved: Vec<Resolved> = Vec::new();
    for (h, n) in &base.nodes {
        if n.shape == NodeShape::HyperEdge {
            continue;
        }
        for r in schema.references.values().filter(|r| r.holder == n.anchor) {
            for (index, values) in tuples(&base, *h, r)?.into_iter().enumerate() {
                let occ = Occurrence {
                    holder: *h,
                    reference: r.id.clone(),
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    literal: literal(&values),
                };
                match candidates(schema, &base, r, *h, &values).as_slice() {
                    [target] => resolved.push(Resolved {
                        occurrence: occ,
                        target: *target,
                        redirect: r.mode == ResolveMode::Redirect,
                    }),
                    found => {
                        let reason = if found.is_empty() {
                            Unresolved::Dangling
                        } else {
                            Unresolved::Ambiguous(found.to_vec())
                        };
                        if r.on_unresolved == OnUnresolved::Error && !lenient {
                            return Err(match reason {
                                Unresolved::Dangling => ResolveError::DanglingReference {
                                    reference: r.id.clone(),
                                    at: ViewRef::Node(*h),
                                    key: values,
                                },
                                Unresolved::Ambiguous(cs) => ResolveError::AmbiguousReference {
                                    reference: r.id.clone(),
                                    at: ViewRef::Node(*h),
                                    key: values,
                                    candidates: cs.into_iter().map(ViewRef::Node).collect(),
                                },
                            });
                        }
                        c.unresolved.push((occ, reason));
                    }
                }
            }
        }
    }
    // Redirect each maximal redirected holder (one no redirected ancestor
    // holds).
    let redirected: BTreeSet<u32> = resolved
        .iter()
        .filter(|x| x.redirect)
        .map(|x| x.occurrence.holder)
        .collect();
    let mut inside: BTreeSet<u32> = BTreeSet::new();
    for root in &redirected {
        let mut ancestor = parent_of(&base, *root);
        let mut maximal = true;
        while let Some(a) = ancestor {
            if redirected.contains(&a) {
                maximal = false;
                break;
            }
            ancestor = parent_of(&base, a);
        }
        if !maximal {
            continue;
        }
        if let Some(fragment) = capture(&mut g, *root) {
            inside.extend(fragment.nodes.keys().copied());
            c.redirected.insert(*root, fragment);
        }
    }
    let mut next = crate::restrict::floor_of(g.edges.keys().copied());
    for x in resolved {
        let holder = x.occurrence.holder;
        let visible = !inside.contains(&holder) || (x.redirect && c.redirected.contains_key(&holder));
        if !visible {
            continue;
        }
        if inside.contains(&x.target) {
            return Err(ResolveError::TargetRemoved {
                reference: x.occurrence.reference,
                at: ViewRef::Node(holder),
            });
        }
        let source = match (x.redirect, c.redirected.get(&holder)) {
            (true, Some(fragment)) => fragment.parent,
            _ => holder,
        };
        let label = schema
            .references
            .get(&x.occurrence.reference)
            .and_then(|r| r.edge.clone())
            .unwrap_or_else(|| SEdge::new("", "", "", None));
        g.edges.insert(
            next,
            GEdge {
                src: source,
                tgt: x.target,
                label,
                position: None,
            },
        );
        c.resolved.insert(next, x.occurrence);
        c.targets.insert(next, x.target);
        next += 1;
    }
    c.unresolved
        .retain(|(o, _)| !inside.contains(&o.holder) || c.redirected.contains_key(&o.holder));
    Ok((g, c))
}

/// Where a key path's value lives at a holder: a node's own value, or a
/// node's field (the field fallback of the last hop).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Site {
    Own(u32),
    Field(u32, String),
}

/// The site a key path reads at `n`, as [`keys::path_value`] reads it: the
/// first node of the last hop's frontier that has a target along the last
/// edge (its first target's own value), else the first frontier node (its
/// field named by the last edge).
fn path_site(g: &Graph, n: u32, path: &[SEdge]) -> Option<Site> {
    let (last, hops) = path.split_last()?;
    let mut frontier = vec![n];
    for hop in hops {
        frontier = frontier
            .iter()
            .flat_map(|x| {
                g.edges
                    .values()
                    .filter(move |e| e.src == *x && e.label == *hop)
                    .map(|e| e.tgt)
            })
            .collect();
    }
    let x = *frontier.first()?;
    let target = g
        .edges
        .values()
        .find(|e| e.src == x && e.label == *last)
        .map(|e| e.tgt);
    Some(target.map_or_else(|| Site::Field(x, last.column().to_owned()), Site::Own))
}

/// Write `value` at a site (`None` removes it).
fn write_site(g: &mut Graph, site: &Site, value: Option<Value>) {
    match site {
        Site::Own(n) => {
            if let Some(node) = g.nodes.get_mut(n) {
                node.value = value.map(Presence::Present);
            }
        }
        Site::Field(n, f) => {
            if let Some(node) = g.nodes.get_mut(n) {
                match value {
                    Some(v) => node.fields.insert(f.clone(), v),
                    None => node.fields.remove(f),
                };
            }
        }
    }
}

/// An occurrence as put writes it back: its key tuple and the target it
/// must re-resolve to (`None` for a recorded unresolved occurrence, which
/// keeps its literal and is not re-resolved).
struct Written {
    tuple: Vec<Value>,
    target: Option<u32>,
}

/// The key tuple an occurrence's literal holds.
fn tuple_of(literal: &Value, parts: usize) -> Vec<Value> {
    match literal {
        Value::List(items) if parts > 1 => items.clone(),
        other => vec![other.clone()],
    }
}

/// Resolution put.
///
/// Every view edge labelled with a reference's edge is a resolution edge:
/// one the complement records, with its source unchanged (the holder, or
/// for a redirect the fragment's parent), keeps its occurrence, written
/// with its literal when its target is unchanged and with the new target's
/// key otherwise; one the complement does not record is a new occurrence of
/// the one `AddEdge` reference whose edge it carries and whose holder
/// vertex anchors its source, appended to the holder's sequence; any other
/// is `UnrepresentableReference`. A recorded occurrence whose edge the view
/// removed is removed from its holder's sequence, and its fragment is not
/// restored. The fragments of kept redirected occurrences are restored,
/// each holder's sequences are written at its key sites (for `One` the one
/// tuple or none; for `Many` each part's list), the result is read back as
/// the tree with the recorded root, and every written occurrence must
/// re-resolve to exactly its target.
///
/// # Errors
///
/// [`ResolveError::UnrepresentableReference`] or [`ResolveError::NotATree`].
pub fn put(schema: &Schema, view: &Graph, comp: &ResComplement) -> Result<Tree, ResolveError> {
    let mut graph = view.clone();
    let declared: Vec<&Reference> = schema.references.values().collect();
    // (holder, reference) to the occurrences by index, recorded first.
    let mut sequences: BTreeMap<(u32, Name), BTreeMap<u32, Written>> = BTreeMap::new();
    for (occ, _) in &comp.unresolved {
        sequences
            .entry((occ.holder, occ.reference.clone()))
            .or_default()
            .insert(
                occ.index,
                Written {
                    tuple: tuple_of(&occ.literal, parts_len(schema, &occ.reference)),
                    target: None,
                },
            );
    }
    let mut kept_fragments: BTreeSet<u32> = BTreeSet::new();
    let mut appended: Vec<(u32, Name, u32)> = Vec::new();
    for (id, edge) in &view.edges {
        if !declared.iter().any(|r| r.edge.as_ref() == Some(&edge.label)) {
            continue;
        }
        graph.edges.remove(id);
        let recorded = comp.resolved.get(id).filter(|occ| {
            let r = &schema.references[&occ.reference];
            let source = if r.mode == ResolveMode::Redirect {
                comp.redirected.get(&occ.holder).map(|f| f.parent)
            } else {
                Some(occ.holder)
            };
            r.edge.as_ref() == Some(&edge.label) && source == Some(edge.src)
        });
        match recorded {
            Some(occ) => {
                if comp.redirected.contains_key(&occ.holder) {
                    kept_fragments.insert(occ.holder);
                }
                let unchanged = view_target(comp, *id) == Some(edge.tgt);
                let tuple = if unchanged {
                    tuple_of(&occ.literal, parts_len(schema, &occ.reference))
                } else {
                    target_tuple(schema, view, &occ.reference, edge.tgt).ok_or_else(|| {
                        ResolveError::UnrepresentableReference {
                            carrier: ViewRef::Edge(*id),
                            reference: Some(occ.reference.clone()),
                        }
                    })?
                };
                sequences
                    .entry((occ.holder, occ.reference.clone()))
                    .or_default()
                    .insert(
                        occ.index,
                        Written {
                            tuple,
                            target: Some(edge.tgt),
                        },
                    );
            }
            None => {
                let writers: Vec<&&Reference> = declared
                    .iter()
                    .filter(|r| {
                        r.edge.as_ref() == Some(&edge.label)
                            && r.mode == ResolveMode::AddEdge
                            && view
                                .nodes
                                .get(&edge.src)
                                .is_some_and(|n| n.anchor == r.holder)
                    })
                    .collect();
                let [r] = writers.as_slice() else {
                    return Err(ResolveError::UnrepresentableReference {
                        carrier: ViewRef::Edge(*id),
                        reference: None,
                    });
                };
                appended.push((edge.src, r.id.clone(), edge.tgt));
            }
        }
    }
    for (holder, reference, target) in appended {
        let tuple = target_tuple(schema, view, &reference, target).ok_or_else(|| {
            ResolveError::UnrepresentableReference {
                carrier: ViewRef::Node(holder),
                reference: Some(reference.clone()),
            }
        })?;
        let seq = sequences.entry((holder, reference)).or_default();
        let next = seq.keys().next_back().map_or(0, |k| k + 1);
        seq.insert(
            next,
            Written {
                tuple,
                target: Some(target),
            },
        );
    }
    for root in &kept_fragments {
        let fragment = &comp.redirected[root];
        graph
            .nodes
            .extend(fragment.nodes.iter().map(|(i, n)| (*i, n.clone())));
        graph
            .edges
            .extend(fragment.edges.iter().map(|(i, e)| (*i, e.clone())));
    }
    write_sequences(schema, comp, &mut graph, &sequences)?;
    let names = schema_parent_name(Some(schema));
    let tree = crate::encode::graph_to_tree(&graph, comp.root, &names)
        .map_err(|_| ResolveError::NotATree)?;
    let (check, _) = tree_to_graph(&tree, Some(schema)).map_err(|_| ResolveError::NotATree)?;
    for ((holder, reference), seq) in &sequences {
        let r = &schema.references[reference];
        let found = tuples(&check, *holder, r).map_err(|_| ResolveError::UnrepresentableReference {
            carrier: ViewRef::Node(*holder),
            reference: Some(reference.clone()),
        })?;
        for (k, (_, w)) in seq.iter().enumerate() {
            let Some(target) = w.target else { continue };
            let ok = found
                .get(k)
                .is_some_and(|key| candidates(schema, &check, r, *holder, key) == vec![target]);
            if !ok {
                return Err(ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Node(*holder),
                    reference: Some(reference.clone()),
                });
            }
        }
    }
    Ok(tree)
}

/// The number of parts of a reference's key-value source.
fn parts_len(schema: &Schema, reference: &str) -> usize {
    match schema.references.get(reference).map(|r| &r.source) {
        Some(RefSource::KeyValue { parts }) => parts.len(),
        _ => 0,
    }
}

/// The target a recorded resolution edge reached at get.
fn view_target(comp: &ResComplement, id: u32) -> Option<u32> {
    comp.targets.get(&id).copied()
}

/// The key tuple of a target under a reference's target key, read in the
/// view.
fn target_tuple(schema: &Schema, view: &Graph, reference: &str, target: u32) -> Option<Vec<Value>> {
    let r = schema.references.get(reference)?;
    keys::key_value(view, target, target_key(schema, r)?)
}

/// Write each holder's occurrence sequences at its key sites: for `One`
/// the one tuple's values (none removes the sites), for `Many` each part's
/// list of the tuples' values in index order.
fn write_sequences(
    schema: &Schema,
    comp: &ResComplement,
    graph: &mut Graph,
    sequences: &BTreeMap<(u32, Name), BTreeMap<u32, Written>>,
) -> Result<(), ResolveError> {
    let mut touched: BTreeSet<(u32, Name)> = comp
        .resolved
        .values()
        .map(|o| (o.holder, o.reference.clone()))
        .collect();
    touched.extend(sequences.keys().cloned());
    for (holder, reference) in touched {
        let r = &schema.references[&reference];
        let RefSource::KeyValue { parts } = &r.source else {
            continue;
        };
        if !graph.nodes.contains_key(&holder) {
            continue;
        }
        let seq: Vec<&Written> = sequences
            .get(&(holder, reference.clone()))
            .map(|s| s.values().collect())
            .unwrap_or_default();
        for (k, part) in parts.iter().enumerate() {
            let site = path_site(graph, holder, part).ok_or_else(|| {
                ResolveError::UnrepresentableReference {
                    carrier: ViewRef::Node(holder),
                    reference: Some(reference.clone()),
                }
            })?;
            let value = match r.cardinality {
                Cardinality::One => seq.first().and_then(|w| w.tuple.get(k).cloned()),
                Cardinality::Many => Some(Value::List(
                    seq.iter().filter_map(|w| w.tuple.get(k).cloned()).collect(),
                )),
            };
            write_site(graph, &site, value);
        }
    }
    Ok(())
}
