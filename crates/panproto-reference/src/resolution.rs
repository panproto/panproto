//! The resolution lens from a tree to a graph, for key-value references
//! resolved by adding an edge.
//!
//! Get lays the tree out as a graph and adds, for each occurrence of a
//! reference at a holder, the reference's edge to the unique target whose
//! target-key value equals the holder's key value (a `Within` key's
//! candidates coming only from the holder's scope bucket); edge ids follow
//! the graph's, in ascending (holder, reference, occurrence). Put removes
//! the resolution edges and writes each view resolution edge's target key
//! into its holder's fields, then requires every written occurrence to
//! re-resolve to exactly its target.
//!
//! Domain: references of cardinality one, with a key-value source whose
//! parts are one edge each read from the holder's fields, mode `AddEdge`,
//! and reference edges no tree arc carries. Other references give `None`.

use std::collections::BTreeMap;

use crate::encode::tree_to_graph;
use crate::keys;
use crate::lens::ViewRef;
use crate::model::{GEdge, Graph, Name, SEdge, Tree};
use crate::schema::{Cardinality, KeyScope, OnUnresolved, RefSource, Reference, ResolveMode, Schema};
use crate::value::Value;

/// A recorded occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    /// Holder node.
    pub holder: u32,
    /// Reference id.
    pub reference: Name,
    /// Occurrence index.
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

/// The resolution complement.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ResComplement {
    /// Resolved occurrences by their edge id.
    pub resolved: BTreeMap<u32, Occurrence>,
    /// Unresolved occurrences with their reason.
    pub unresolved: Vec<(Occurrence, Unresolved)>,
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

fn in_domain(r: &Reference) -> bool {
    matches!(&r.source, RefSource::KeyValue { parts } if parts.iter().all(|p| p.len() == 1))
        && r.cardinality == Cardinality::One
        && r.mode == ResolveMode::AddEdge
        && r.edge.is_some()
}

/// The candidates of an occurrence: targets whose target-key value is the
/// key, from the holder's scope bucket for a `Within` key (none when the
/// holder has no such ancestor).
fn candidates(schema: &Schema, g: &Graph, r: &Reference, holder: u32, key: &[Value]) -> Vec<u32> {
    let Some(k) = schema.keys.get(&r.target_vertex).and_then(|ks| ks.iter().find(|k| Some(&k.id) == r.target_key.as_ref())) else {
        return Vec::new();
    };
    let scope = match &k.scope {
        KeyScope::Within(v) => match keys::nearest_ancestor(g, holder, v) {
            Some(a) => Some((v.clone(), a)),
            None => return Vec::new(),
        },
        _ => None,
    };
    g.nodes
        .iter()
        .filter(|(_, n)| n.anchor == r.target_vertex)
        .filter(|(i, _)| keys::key_value(g, **i, k).as_deref() == Some(key))
        .filter(|(i, _)| scope.as_ref().is_none_or(|(v, a)| keys::nearest_ancestor(g, **i, v) == Some(*a)))
        .map(|(i, _)| *i)
        .collect()
}

fn literal(values: &[Value]) -> Value {
    match values {
        [one] => one.clone(),
        many => Value::List(many.to_vec()),
    }
}

/// Resolution get.
///
/// # Errors
///
/// The first strict unresolved occurrence in (holder, reference) order,
/// or [`ResolveError::NotATree`].
pub fn get(schema: &Schema, t: &Tree, lenient: bool) -> Result<Option<(Graph, ResComplement)>, ResolveError> {
    if !schema.references.values().all(in_domain) {
        return Ok(None);
    }
    let (mut g, _) = tree_to_graph(t, Some(schema)).map_err(|_| ResolveError::NotATree)?;
    let base = g.clone();
    let mut c = ResComplement { root: t.root, ..ResComplement::default() };
    let mut next = crate::restrict::floor_of(g.edges.keys().copied());
    for (h, n) in &base.nodes {
        for r in schema.references.values().filter(|r| r.holder == n.anchor) {
            let RefSource::KeyValue { parts } = &r.source else { continue };
            let values: Option<Vec<Value>> = parts.iter().map(|p| keys::path_value(&base, *h, p)).collect();
            let Some(values) = values else { continue };
            let occ = Occurrence { holder: *h, reference: r.id.clone(), index: 0, literal: literal(&values) };
            let cands = candidates(schema, &base, r, *h, &values);
            match cands.as_slice() {
                [target] => {
                    let label = r.edge.clone().unwrap_or_else(|| SEdge::new("", "", "", None));
                    g.edges.insert(next, GEdge { src: *h, tgt: *target, label, position: None });
                    c.resolved.insert(next, occ);
                    next += 1;
                }
                found => {
                    let reason = if found.is_empty() { Unresolved::Dangling } else { Unresolved::Ambiguous(found.to_vec()) };
                    if r.on_unresolved == OnUnresolved::Error && !lenient {
                        return Err(match reason {
                            Unresolved::Dangling => ResolveError::DanglingReference { reference: r.id.clone(), at: ViewRef::Node(*h), key: values },
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
    Ok(Some((g, c)))
}

/// Resolution put: drop every reference-labelled view edge, write each
/// one's target key into its holder's fields (a kept occurrence keeps its
/// literal), clear the fields of a recorded occurrence whose edge the view
/// removed, read the rest back as the tree with the recorded root, and
/// require every written occurrence to re-resolve to its target.
///
/// # Errors
///
/// [`ResolveError::UnrepresentableReference`] or [`ResolveError::NotATree`].
pub fn put(schema: &Schema, view: &Graph, c: &ResComplement) -> Result<Tree, ResolveError> {
    let mut g = view.clone();
    let ref_labels: Vec<&Reference> = schema.references.values().collect();
    let mut writes: Vec<(u32, &Reference, u32)> = Vec::new();
    for (id, e) in &view.edges {
        let writers: Vec<&&Reference> = ref_labels.iter().filter(|r| r.edge.as_ref() == Some(&e.label) && view.nodes[&e.src].anchor == r.holder).collect();
        if r_label_is_reference(&ref_labels, &e.label) {
            g.edges.remove(id);
            match writers.as_slice() {
                [r] => writes.push((e.src, r, e.tgt)),
                _ => return Err(ResolveError::UnrepresentableReference { carrier: ViewRef::Edge(*id), reference: None }),
            }
        }
    }
    for (id, occ) in &c.resolved {
        if !view.edges.contains_key(id) {
            let r = &schema.references[&occ.reference];
            if let RefSource::KeyValue { parts } = &r.source {
                if let Some(n) = g.nodes.get_mut(&occ.holder) {
                    for p in parts {
                        n.fields.remove(p[0].column());
                    }
                }
            }
        }
    }
    for (h, r, t) in &writes {
        let RefSource::KeyValue { parts } = &r.source else { continue };
        let key = schema.keys.get(&r.target_vertex).and_then(|ks| ks.iter().find(|k| Some(&k.id) == r.target_key.as_ref()));
        let values = key.and_then(|k| keys::key_value(view, *t, k));
        let Some(values) = values else { return Err(ResolveError::UnrepresentableReference { carrier: ViewRef::Node(*h), reference: Some(r.id.clone()) }) };
        if let Some(n) = g.nodes.get_mut(h) {
            for (p, v) in parts.iter().zip(values) {
                n.fields.insert(p[0].column().to_owned(), v);
            }
        }
    }
    let tree = crate::encode::graph_to_tree(&g, c.root).ok_or(ResolveError::NotATree)?;
    let (check, _) = tree_to_graph(&tree, Some(schema)).map_err(|_| ResolveError::NotATree)?;
    for (h, r, t) in &writes {
        let RefSource::KeyValue { parts } = &r.source else { continue };
        let values: Option<Vec<Value>> = parts.iter().map(|p| keys::path_value(&check, *h, p)).collect();
        let ok = values.is_some_and(|v| candidates(schema, &check, r, *h, &v) == vec![*t]);
        if !ok {
            return Err(ResolveError::UnrepresentableReference { carrier: ViewRef::Node(*h), reference: Some(r.id.clone()) });
        }
    }
    Ok(tree)
}

fn r_label_is_reference(refs: &[&Reference], label: &SEdge) -> bool {
    refs.iter().any(|r| r.edge.as_ref() == Some(label))
}
