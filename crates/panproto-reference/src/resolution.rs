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
use crate::schema::{
    Cardinality, KeyScope, OnUnresolved, RefSource, Reference, ResolveMode, Schema,
};
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
fn candidates(
    schema: &Schema,
    graph: &Graph,
    reference: &Reference,
    holder: u32,
    key: &[Value],
) -> Vec<u32> {
    let Some(tkey) = schema.keys.get(&reference.target_vertex).and_then(|ks| {
        ks.iter()
            .find(|tkey| Some(&tkey.id) == reference.target_key.as_ref())
    }) else {
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
        .filter(|(_, node)| node.anchor == reference.target_vertex)
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

/// Resolution get.
///
/// # Errors
///
/// The first strict unresolved occurrence in (holder, reference) order,
/// or [`ResolveError::NotATree`].
pub fn get(
    schema: &Schema,
    t: &Tree,
    lenient: bool,
) -> Result<Option<(Graph, ResComplement)>, ResolveError> {
    if !schema.references.values().all(in_domain) {
        return Ok(None);
    }
    let (mut g, _) = tree_to_graph(t, Some(schema)).map_err(|_| ResolveError::NotATree)?;
    let base = g.clone();
    let mut c = ResComplement {
        root: t.root,
        ..ResComplement::default()
    };
    let mut next = crate::restrict::floor_of(g.edges.keys().copied());
    for (h, n) in &base.nodes {
        for r in schema.references.values().filter(|r| r.holder == n.anchor) {
            let RefSource::KeyValue { parts } = &r.source else {
                continue;
            };
            let values: Option<Vec<Value>> = parts
                .iter()
                .map(|p| keys::path_value(&base, *h, p))
                .collect();
            let Some(values) = values else { continue };
            let occ = Occurrence {
                holder: *h,
                reference: r.id.clone(),
                index: 0,
                literal: literal(&values),
            };
            let cands = candidates(schema, &base, r, *h, &values);
            match cands.as_slice() {
                [target] => {
                    let label = r
                        .edge
                        .clone()
                        .unwrap_or_else(|| SEdge::new("", "", "", None));
                    g.edges.insert(
                        next,
                        GEdge {
                            src: *h,
                            tgt: *target,
                            label,
                            position: None,
                        },
                    );
                    c.resolved.insert(next, occ);
                    next += 1;
                }
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
    Ok(Some((g, c)))
}

/// Resolution put.
///
/// Drop every reference-labelled view edge, write each one's target key into its holder's fields (a
/// kept occurrence keeps its literal), clear the fields of a recorded occurrence whose edge the
/// view removed, read the rest back as the tree with the recorded root, and require every written
/// occurrence to re-resolve to its target.
///
/// # Errors
///
/// [`ResolveError::UnrepresentableReference`] or [`ResolveError::NotATree`].
pub fn put(schema: &Schema, view: &Graph, comp: &ResComplement) -> Result<Tree, ResolveError> {
    let mut graph = view.clone();
    let declared: Vec<&Reference> = schema.references.values().collect();
    let mut writes: Vec<(u32, &Reference, u32)> = Vec::new();
    for (id, edge) in &view.edges {
        let writing: Vec<&&Reference> = declared
            .iter()
            .filter(|reference| {
                reference.edge.as_ref() == Some(&edge.label)
                    && view
                        .nodes
                        .get(&edge.src)
                        .is_some_and(|n| n.anchor == reference.holder)
            })
            .collect();
        if r_label_is_reference(&declared, &edge.label) {
            graph.edges.remove(id);
            match writing.as_slice() {
                [reference] => writes.push((edge.src, reference, edge.tgt)),
                _ => {
                    return Err(ResolveError::UnrepresentableReference {
                        carrier: ViewRef::Edge(*id),
                        reference: None,
                    });
                }
            }
        }
    }
    for (id, occ) in &comp.resolved {
        if !view.edges.contains_key(id) {
            let reference = &schema.references[&occ.reference];
            if let RefSource::KeyValue { parts } = &reference.source {
                if let Some(node) = graph.nodes.get_mut(&occ.holder) {
                    for part in parts {
                        node.fields.remove(part[0].column());
                    }
                }
            }
        }
    }
    for (holder, reference, target) in &writes {
        let RefSource::KeyValue { parts } = &reference.source else {
            continue;
        };
        let key = schema.keys.get(&reference.target_vertex).and_then(|ks| {
            ks.iter()
                .find(|key| Some(&key.id) == reference.target_key.as_ref())
        });
        let values = key.and_then(|key| keys::key_value(view, *target, key));
        let Some(values) = values else {
            return Err(ResolveError::UnrepresentableReference {
                carrier: ViewRef::Node(*holder),
                reference: Some(reference.id.clone()),
            });
        };
        if let Some(node) = graph.nodes.get_mut(holder) {
            for (part, value) in parts.iter().zip(values) {
                node.fields.insert(part[0].column().to_owned(), value);
            }
        }
    }
    let tree = crate::encode::graph_to_tree(&graph, comp.root).ok_or(ResolveError::NotATree)?;
    let (check, _) = tree_to_graph(&tree, Some(schema)).map_err(|_| ResolveError::NotATree)?;
    for (holder, reference, target) in &writes {
        let RefSource::KeyValue { parts } = &reference.source else {
            continue;
        };
        let values: Option<Vec<Value>> = parts
            .iter()
            .map(|part| keys::path_value(&check, *holder, part))
            .collect();
        let ok = values.is_some_and(|value| {
            candidates(schema, &check, reference, *holder, &value) == vec![*target]
        });
        if !ok {
            return Err(ResolveError::UnrepresentableReference {
                carrier: ViewRef::Node(*holder),
                reference: Some(reference.id.clone()),
            });
        }
    }
    Ok(tree)
}

fn r_label_is_reference(refs: &[&Reference], label: &SEdge) -> bool {
    refs.iter().any(|r| r.edge.as_ref() == Some(label))
}
