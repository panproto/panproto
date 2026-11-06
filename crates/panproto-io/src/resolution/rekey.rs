//! Coherent rekeying of a resolution's presentation ids.
//!
//! The ids of span nodes and occurrence edges are presentation: the keys
//! [`ResolutionComplement::synth_index`] gives them are what identifies
//! them. Rekeying renames those ids in a view and its complement together,
//! including every `ViewRef` nested in a complement coordinate that denotes
//! a view carrier (occurrence holders, value sites of span texts, spanning
//! and order coordinates, definitions, unresolved holders and candidates,
//! assembly trace view coordinates). A redirected fragment's attachment and
//! saved fans also name retained view nodes at its boundary; those coordinates
//! move. Ids of hidden nodes inside any redirected fragment do not change.

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::resolution_complement::{
    ResolutionComplement, ResolutionSpan, ResolutionTextId, ResolutionValueSite,
};
use panproto_inst::{EdgeId, GEdge, GInstance, SynthIndex, ViewRef};

/// A renaming of view node ids and edge ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Renaming {
    /// Node ids, old to new.
    pub nodes: BTreeMap<u32, u32>,
    /// Edge ids, old to new.
    pub edges: BTreeMap<EdgeId, EdgeId>,
}

impl Renaming {
    /// The renaming that sends each key's id under `from` to its id under
    /// `to`, for the keys both indexes hold.
    #[must_use]
    pub fn between(from: &SynthIndex, to: &SynthIndex) -> Self {
        let node_ids: BTreeMap<_, _> = to.nodes.iter().map(|(&id, key)| (key, id)).collect();
        let edge_ids: BTreeMap<_, _> = to.edges.iter().map(|(&id, key)| (key, id)).collect();
        Self {
            nodes: from
                .nodes
                .iter()
                .filter_map(|(&id, key)| node_ids.get(key).map(|&to| (id, to)))
                .collect(),
            edges: from
                .edges
                .iter()
                .filter_map(|(&id, key)| edge_ids.get(key).map(|&to| (id, to)))
                .collect(),
        }
    }

    /// A node id renamed.
    #[must_use]
    pub fn node(&self, id: u32) -> u32 {
        self.nodes.get(&id).copied().unwrap_or(id)
    }

    /// An edge id renamed.
    #[must_use]
    pub fn edge(&self, id: EdgeId) -> EdgeId {
        self.edges.get(&id).copied().unwrap_or(id)
    }

    /// A view reference renamed: a node or an edge by its id; a row, an
    /// arc or a pair is not a renamed view id.
    #[must_use]
    pub fn view_ref(&self, reference: &ViewRef) -> ViewRef {
        match reference {
            ViewRef::Node(id) => ViewRef::Node(self.node(*id)),
            ViewRef::Edge(id) => ViewRef::Edge(self.edge(*id)),
            other => other.clone(),
        }
    }

    fn site(&self, site: &ResolutionValueSite) -> ResolutionValueSite {
        site.with_holder(self.view_ref(site.holder()))
    }

    fn text(&self, text: &ResolutionTextId) -> ResolutionTextId {
        text.with_site(self.site(text.site()))
    }

    fn span(&self, span: &ResolutionSpan) -> ResolutionSpan {
        ResolutionSpan {
            text: self.text(&span.text),
            start: span.start,
            end: span.end,
        }
    }

    /// Transport retained endpoints without changing hidden source residue.
    fn redirect_boundaries(&self, complement: &mut ResolutionComplement) {
        let hidden: BTreeSet<u32> = complement
            .redirected
            .values()
            .flat_map(|fragment| fragment.nodes.keys().copied())
            .collect();
        let boundary_node = |id| {
            if hidden.contains(&id) {
                id
            } else {
                self.node(id)
            }
        };
        for fragment in complement.redirected.values_mut() {
            if let Some(attachment) = &mut fragment.attachment {
                attachment.parent = boundary_node(attachment.parent);
            }
            for fan in &mut fragment.fans {
                fan.parent = boundary_node(fan.parent);
                for child in fan.children.values_mut() {
                    *child = boundary_node(*child);
                }
            }
        }
    }

    /// A complement with every view coordinate renamed.
    #[must_use]
    pub fn complement(&self, complement: &ResolutionComplement) -> ResolutionComplement {
        let mut next = complement.clone();
        self.redirect_boundaries(&mut next);
        next.occurrences = complement
            .occurrences
            .iter()
            .map(|(carrier, records)| {
                let records = records
                    .iter()
                    .map(|record| {
                        let mut record = record.clone();
                        record.holder = self.view_ref(&record.holder);
                        record
                    })
                    .collect();
                (self.view_ref(carrier), records)
            })
            .collect();
        next.definitions = complement
            .definitions
            .iter()
            .map(|(key, holder)| {
                let mut key = key.clone();
                key.target = self.node(key.target);
                (key, self.view_ref(holder))
            })
            .collect();
        next.order = complement
            .order
            .iter()
            .map(|(parent, children)| {
                (
                    self.node(*parent),
                    children.iter().map(|child| self.view_ref(child)).collect(),
                )
            })
            .collect();
        next.spanning = complement
            .spanning
            .iter()
            .map(|(child, edge)| (self.node(*child), self.view_ref(edge)))
            .collect();
        next.span_nodes = complement
            .span_nodes
            .iter()
            .map(|(id, span)| (self.node(*id), self.span(span)))
            .collect();
        for record in &mut next.unresolved {
            record.holder = self.view_ref(&record.holder);
            if let panproto_inst::resolution_complement::UnresolvedReason::Ambiguous {
                candidates,
            } = &mut record.reason
            {
                for candidate in candidates.iter_mut() {
                    *candidate = self.view_ref(candidate);
                }
                candidates.sort();
            }
        }
        next.unresolved.sort_by(|a, b| {
            (&a.holder, &a.reference, a.index).cmp(&(&b.holder, &b.reference, b.index))
        });
        if let Some(assembly) = &mut next.assembly {
            for trace in &mut assembly.trace {
                for carrier in &mut trace.view {
                    *carrier = self.view_ref(carrier);
                }
            }
        }
        next
    }

    /// A graph with its node and edge ids renamed.
    ///
    /// # Errors
    ///
    /// An instance error when the renaming sends two ids to one.
    pub fn graph(&self, graph: &GInstance) -> Result<GInstance, panproto_inst::InstError> {
        let mut next = GInstance::new();
        for (id, node) in graph.nodes() {
            let mut node = node.clone();
            node.id = self.node(*id);
            next.insert_node(node)?;
        }
        for (id, edge) in graph.edges_sorted() {
            next.insert_edge(
                self.edge(id),
                GEdge {
                    src: self.node(edge.src),
                    tgt: self.node(edge.tgt),
                    edge: edge.edge.clone(),
                    position: edge.position,
                },
            )?;
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use panproto_inst::resolution_complement::{
        DefinitionKey, RedirectedArc, RedirectedAttachment, RedirectedFragment, RefOccurrence,
        ResolutionComplement, ResolutionSpan, ResolutionTextId, ResolutionValueSite,
        UnresolvedReason, UnresolvedRef,
    };
    use panproto_inst::{EdgeId, Fan, Node, SynthKey, Value, ViewRef};

    use super::Renaming;

    fn span_complement(holder: u32, span: u32, edge: u32) -> ResolutionComplement {
        let text = ResolutionTextId::Local(ResolutionValueSite::Field {
            holder: ViewRef::Node(holder),
            field: "text".to_owned(),
        });
        let mut complement = ResolutionComplement::default();
        complement.span_nodes.insert(
            span,
            ResolutionSpan {
                text,
                start: 0,
                end: 3,
            },
        );
        complement.occurrences.insert(
            ViewRef::Edge(EdgeId(edge)),
            vec![RefOccurrence {
                holder: ViewRef::Node(holder),
                reference: "r".into(),
                index: 0,
                literal: Value::Int(0),
                form: None,
                context: None,
            }],
        );
        complement.definitions.insert(
            DefinitionKey {
                reference: "v".into(),
                target: holder,
            },
            ViewRef::Node(holder),
        );
        complement.spanning.insert(holder, ViewRef::Edge(EdgeId(0)));
        complement.order.insert(0, vec![ViewRef::Node(holder)]);
        complement.unresolved.push(UnresolvedRef {
            holder: ViewRef::Node(holder),
            reference: "r".into(),
            index: 1,
            literal: Value::Int(1),
            reason: UnresolvedReason::Ambiguous {
                candidates: vec![ViewRef::Node(holder), ViewRef::Node(span)],
            },
            context: None,
        });
        complement
    }

    #[test]
    fn span_congruence_rekeys_the_nested_text_site() -> Result<(), serde_json::Error> {
        let complement = span_complement(1, 5, 7);
        let renaming = Renaming {
            nodes: [(1, 11), (5, 15)].into_iter().collect(),
            edges: std::iter::once((EdgeId(7), EdgeId(17))).collect(),
        };
        let renamed = renaming.complement(&complement);
        assert_eq!(
            serde_json::to_value(&renamed)?,
            serde_json::to_value(span_complement(11, 15, 17))?
        );
        let index = renamed.synth_index();
        assert_eq!(
            index.nodes.get(&15),
            Some(&SynthKey::ResolutionSpan {
                text: ResolutionTextId::Local(ResolutionValueSite::Field {
                    holder: ViewRef::Node(11),
                    field: "text".to_owned(),
                }),
                start: 0,
                end: 3,
            })
        );
        let back = Renaming::between(&renamed.synth_index(), &complement.synth_index());
        assert_eq!(
            back.nodes.get(&15),
            None,
            "the keys differ once the holder is renamed"
        );
        let synthesized_only = Renaming {
            nodes: std::iter::once((5, 15)).collect(),
            edges: std::iter::once((EdgeId(7), EdgeId(17))).collect(),
        };
        let renamed = synthesized_only.complement(&complement);
        let back = Renaming::between(&renamed.synth_index(), &complement.synth_index());
        assert_eq!(
            serde_json::to_value(back.complement(&renamed))?,
            serde_json::to_value(&complement)?
        );
        Ok(())
    }

    #[test]
    fn a_renaming_is_the_identity_off_its_domain() {
        let renaming = Renaming::default();
        assert_eq!(renaming.node(4), 4);
        assert_eq!(renaming.edge(EdgeId(4)), EdgeId(4));
        assert_eq!(
            renaming.view_ref(&ViewRef::Row(panproto_inst::RowId(2))),
            ViewRef::Row(panproto_inst::RowId(2))
        );
    }

    fn redirect_complement(parent: u32, retained: u32) -> ResolutionComplement {
        let edge = panproto_schema::Edge {
            src: "doc".into(),
            tgt: "link".into(),
            kind: "prop".into(),
            name: Some("links".into()),
        };
        let fragment = |root, parent, source_slot| RedirectedFragment {
            root,
            attachment: Some(RedirectedAttachment {
                parent,
                child: root,
                edge: edge.clone(),
                source_slot,
            }),
            nodes: [(root, Node::new(root, "link"))].into_iter().collect(),
            arcs: Vec::new(),
            fans: Vec::new(),
        };
        let mut first = fragment(4, parent, 3);
        first.nodes.insert(5, Node::new(5, "link"));
        first.arcs.push(RedirectedArc {
            parent: 4,
            child: 5,
            edge: edge.clone(),
            source_slot: 4,
        });
        first.fans = vec![
            Fan::new("boundary", parent)
                .with_child("hidden", 4)
                .with_child("retained", retained),
            Fan::new("cross-fragment", 4)
                .with_child("other-hidden", 7)
                .with_child("retained", retained),
        ];
        let mut second = fragment(7, retained, 6);
        second.fans.push(
            Fan::new("hidden-parent", 7)
                .with_child("other-hidden", 5)
                .with_child("retained", parent),
        );
        let mut complement = ResolutionComplement::default();
        complement.redirected = [(4, first), (7, second)].into_iter().collect();
        complement.order.insert(
            parent,
            vec![ViewRef::Node(retained), ViewRef::Node(4), ViewRef::Node(7)],
        );
        complement
            .spanning
            .insert(retained, ViewRef::Edge(EdgeId(0)));
        complement
    }

    #[test]
    fn retained_redirect_boundaries_follow_lawful_renaming_exactly() -> Result<(), serde_json::Error>
    {
        let complement = redirect_complement(0, 1);
        let renaming = Renaming {
            nodes: [(0, 10), (1, 11)].into_iter().collect(),
            edges: std::collections::BTreeMap::new(),
        };
        let renamed = renaming.complement(&complement);
        assert_eq!(
            serde_json::to_value(&renamed)?,
            serde_json::to_value(redirect_complement(10, 11))?,
            "only retained attachment and fan endpoints move; all source residue stays exact"
        );
        let inverse = Renaming {
            nodes: [(10, 0), (11, 1)].into_iter().collect(),
            edges: std::collections::BTreeMap::new(),
        };
        assert_eq!(
            serde_json::to_value(inverse.complement(&renamed))?,
            serde_json::to_value(&complement)?,
            "inverse transport restores every complement coordinate"
        );
        assert_eq!(
            serde_json::to_value(Renaming::default().complement(&complement))?,
            serde_json::to_value(complement)?,
            "identity transport preserves redirected fragments"
        );
        Ok(())
    }

    #[test]
    fn saved_fan_endpoints_use_the_union_of_hidden_fragment_ids() -> Result<(), serde_json::Error> {
        let mut complement = redirect_complement(0, 1);
        // Deliberately include hidden IDs to check the boundary helper's
        // protection independently of the lawful visible-map precondition.
        let renaming = Renaming {
            nodes: [(0, 10), (1, 11), (4, 40), (5, 50), (7, 70)]
                .into_iter()
                .collect(),
            edges: std::collections::BTreeMap::new(),
        };
        renaming.redirect_boundaries(&mut complement);
        assert_eq!(
            serde_json::to_value(&complement.redirected)?,
            serde_json::to_value(redirect_complement(10, 11).redirected)?,
            "an endpoint hidden in either fragment remains fixed in every saved fan"
        );
        Ok(())
    }
}
