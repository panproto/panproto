//! Shared graph/table lens domain types and key-domain helpers.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::fan::FanViolation;
use crate::model::{Graph, Name, SEdge};
use crate::restrict::{PredicateRefusal, is_fan_kind};
use crate::schema::Schema;
use crate::value::Value;

/// The column name used to report a missing key part.
pub(super) fn part_column(part: &crate::schema::KeyPart) -> String {
    match part {
        crate::schema::KeyPart::Path(path) => path
            .last()
            .map(|edge| edge.column().to_owned())
            .unwrap_or_default(),
        crate::schema::KeyPart::Position { within } => within.column().to_owned(),
    }
}

/// Whether a key role imposes uniqueness in the put domain.
pub(super) const fn uniqueness_role(role: crate::schema::KeyRole) -> bool {
    matches!(
        role,
        crate::schema::KeyRole::Primary
            | crate::schema::KeyRole::Unique
            | crate::schema::KeyRole::Identifier
    )
}

/// Whether one required key part has a value at `node`.
pub(super) fn key_part_present(
    graph: &crate::model::Graph,
    node: u32,
    part: &crate::schema::KeyPart,
) -> bool {
    match part {
        crate::schema::KeyPart::Path(path) => crate::keys::path_value(graph, node, path).is_some(),
        crate::schema::KeyPart::Position { .. } => {
            let singleton = crate::schema::Key {
                id: String::new(),
                parts: vec![part.clone()],
                role: crate::schema::KeyRole::Index,
                scope: crate::schema::KeyScope::Global,
                equality: Vec::new(),
            };
            crate::keys::key_value(graph, node, &singleton).is_some()
        }
    }
}

/// The uniqueness clause of one key on a restored graph: a `DuplicateKey`
/// for each bucket of two or more elements, or, when the keying function
/// refuses a key tuple, the key's `KeyRefused` alone. `elem` names a
/// graph node as an element of the view's shape.
pub(super) fn key_uniqueness_violations(
    graph: &Graph,
    vertex: &Name,
    key: &crate::schema::Key,
    elem: impl Fn(u32) -> ViewRef,
) -> Vec<PutDomainError> {
    match crate::keys::buckets(graph, vertex, key) {
        Ok((buckets, _)) => buckets
            .into_values()
            .filter(|(_, elems)| elems.len() > 1)
            .map(|(values, elems)| PutDomainError::DuplicateKey {
                table: vertex.clone(),
                key: key.parts.iter().map(part_column).zip(values).collect(),
                elems: elems.into_iter().map(&elem).collect(),
            })
            .collect(),
        Err(reason) => vec![PutDomainError::KeyRefused {
            table: vertex.clone(),
            key: key.id.clone(),
            reason,
        }],
    }
}

/// Compare named key values by column name and canonical value bytes.
fn key_values_cmp(
    left: &[(String, crate::value::Value)],
    right: &[(String, crate::value::Value)],
) -> Ordering {
    for ((left_name, left_value), (right_name, right_value)) in left.iter().zip(right) {
        let ordering = left_name
            .cmp(right_name)
            .then_with(|| crate::canonical::canonical_cmp(left_value, right_value));
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

/// An instance element: graph nodes and edges, tree arcs, table rows and pairs.
///
/// The declaration order is the shared shape-native order used by production.
/// Canonical validation tags are assigned explicitly and do not depend on it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ViewRef {
    /// A node.
    Node(u32),
    /// An edge.
    Edge(u32),
    /// A tree arc, named by its parent and child node ids.
    Arc {
        /// The parent node id.
        parent: u32,
        /// The child node id.
        child: u32,
    },
    /// A row.
    Row(u32),
    /// A foreign-key pair.
    Pair(Box<SEdge>, u32, u32),
}

/// A violation of the put domain, in report order (variant, then element).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PutDomainError {
    /// A view id names a complement-dropped element.
    IdCollision {
        /// The element.
        elem: ViewRef,
    },
    /// A synthesized object is missing or changed, unexplained.
    SynthesizedEdited {
        /// The element.
        elem: ViewRef,
    },
    /// A view edge would be a new contracted pair.
    ContractedEdgeInserted {
        /// The edge.
        elem: ViewRef,
    },
    /// A view edge with no unique source label.
    UnrepresentableViewEdge {
        /// The edge.
        elem: ViewRef,
    },
    /// A view node's anchor has several preimages.
    AmbiguousReverseAnchor {
        /// The node.
        elem: ViewRef,
        /// The preimages.
        candidates: Vec<Name>,
    },
    /// A restored node's survival predicate is false.
    SurvivalPredicateFails {
        /// The node.
        elem: ViewRef,
    },
    /// An edited field the migration adds.
    DerivedCellEdited {
        /// The node.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// An edited field written by a transform without an inverse.
    TransformNotInvertible {
        /// The node.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// A restored element duplicates a declared key.
    DuplicateKey {
        /// The keyed vertex.
        table: Name,
        /// The key's columns and values.
        key: Vec<(String, Value)>,
        /// The elements, ascending.
        elems: Vec<ViewRef>,
    },
    /// A required cell is absent.
    MissingRequiredCell {
        /// The node.
        elem: ViewRef,
        /// The field.
        field: String,
    },
    /// The restored source leaves the fan domain.
    MalformedFan {
        /// The fan node.
        node: u32,
        /// The first violation.
        reason: FanViolation,
    },
    /// A restored node's survival predicate is refused.
    SurvivalPredicateRefused {
        /// The node.
        elem: ViewRef,
        /// The refusal.
        refusal: PredicateRefusal,
    },
    /// A uniqueness key of the restored source is undecided: the keying
    /// function refuses the key tuple of its least such element.
    KeyRefused {
        /// The keyed vertex.
        table: Name,
        /// The key's id.
        key: String,
        /// The refusal.
        reason: crate::keys::KeyingError,
    },
}

impl PutDomainError {
    /// The report order key: the variant index, then the element (the fan
    /// node for `MalformedFan`, the least element for `DuplicateKey`, none
    /// for `KeyRefused`).
    #[must_use]
    pub fn order_key(&self) -> (u8, Option<ViewRef>) {
        match self {
            Self::IdCollision { elem } => (0, Some(elem.clone())),
            Self::SynthesizedEdited { elem } => (1, Some(elem.clone())),
            Self::ContractedEdgeInserted { elem } => (2, Some(elem.clone())),
            Self::UnrepresentableViewEdge { elem } => (3, Some(elem.clone())),
            Self::AmbiguousReverseAnchor { elem, .. } => (4, Some(elem.clone())),
            Self::SurvivalPredicateFails { elem } => (5, Some(elem.clone())),
            Self::DerivedCellEdited { elem, .. } => (6, Some(elem.clone())),
            Self::TransformNotInvertible { elem, .. } => (7, Some(elem.clone())),
            Self::DuplicateKey { elems, .. } => (8, elems.iter().min().cloned()),
            Self::MissingRequiredCell { elem, .. } => (9, Some(elem.clone())),
            Self::MalformedFan { node, .. } => (10, Some(ViewRef::Node(*node))),
            Self::SurvivalPredicateRefused { elem, .. } => (11, Some(elem.clone())),
            Self::KeyRefused { .. } => (12, None),
        }
    }

    /// Compare two violations in complete deterministic report order.
    pub(super) fn report_cmp(&self, other: &Self) -> Ordering {
        self.order_key()
            .cmp(&other.order_key())
            .then_with(|| match (self, other) {
                (
                    Self::DuplicateKey {
                        table: left_table,
                        key: left_key,
                        ..
                    },
                    Self::DuplicateKey {
                        table: right_table,
                        key: right_key,
                        ..
                    },
                ) => left_table
                    .cmp(right_table)
                    .then_with(|| key_values_cmp(left_key, right_key)),
                (
                    Self::MissingRequiredCell {
                        field: left_field, ..
                    },
                    Self::MissingRequiredCell {
                        field: right_field, ..
                    },
                ) => left_field.cmp(right_field),
                (
                    Self::KeyRefused {
                        table: left_table,
                        key: left_key,
                        ..
                    },
                    Self::KeyRefused {
                        table: right_table,
                        key: right_key,
                        ..
                    },
                ) => left_table.cmp(right_table).then_with(|| left_key.cmp(right_key)),
                _ => Ordering::Equal,
            })
    }

    /// Stable-sort a complete report and remove exact repeats within each
    /// comparator-equivalent run.
    ///
    /// `report_cmp` deliberately ignores non-ordering payloads such as an
    /// ambiguous anchor's candidate list. Adjacent `Vec::dedup` is therefore
    /// insufficient for a run shaped `A, B, A`: the two exact `A` values need
    /// not become adjacent under the stable sort.
    pub(super) fn sort_report(errors: &mut Vec<Self>) {
        errors.sort_by(Self::report_cmp);
        let mut unique = Vec::with_capacity(errors.len());
        let mut run_start = 0;
        for error in errors.drain(..) {
            let new_run = unique
                .last()
                .is_some_and(|previous| Self::report_cmp(previous, &error) != Ordering::Equal);
            if new_run {
                run_start = unique.len();
            }
            if !unique[run_start..].contains(&error) {
                unique.push(error);
            }
        }
        *errors = unique;
    }
}

/// Missing structural cells required by declared source-schema edges.
///
/// The reference schema has no default-expression coordinate, so its complete
/// carrier for this clause consists of exact outgoing edges and graph-node
/// fields. An edge's written name, or its target vertex when unnamed, is the
/// corresponding field coordinate.
pub(super) fn structural_required_cell_violations(
    schema: &Schema,
    graph: &Graph,
    elem: impl Fn(u32) -> ViewRef,
) -> Vec<PutDomainError> {
    let mut missing = BTreeSet::new();
    for (id, node) in &graph.nodes {
        for edge in schema.required.get(&node.anchor).into_iter().flatten() {
            let has_edge = graph
                .edges
                .values()
                .any(|candidate| candidate.src == *id && candidate.label == *edge);
            let field = edge.column();
            if !has_edge && !node.fields.contains_key(field) {
                missing.insert((elem(*id), field.to_owned()));
            }
        }
    }
    missing
        .into_iter()
        .map(|(elem, field)| PutDomainError::MissingRequiredCell { elem, field })
        .collect()
}

/// Missing source value sites required by declared constraint semantics.
pub(super) fn required_value_violations(
    schema: &Schema,
    semantics: &BTreeMap<Name, crate::validation::Semantics>,
    graph: &Graph,
    elem: impl Fn(u32) -> ViewRef,
) -> Vec<PutDomainError> {
    let required: BTreeSet<&Name> = schema
        .constraints
        .iter()
        .filter(|(_, constraints)| {
            constraints.iter().any(|(sort, _)| {
                semantics.get(sort) == Some(&crate::validation::Semantics::RequiredValue)
            })
        })
        .map(|(vertex, _)| vertex)
        .collect();
    let mut missing = BTreeSet::new();
    for (id, _) in graph.nodes.iter().filter(|(id, node)| {
        node.shape != crate::model::NodeShape::HyperEdge
            && schema.vertices.contains_key(&node.anchor)
            && !graph
                .edges
                .values()
                .any(|edge| edge.src == **id && is_fan_kind(&edge.label.kind))
    }) {
        for vertex in &required {
            for (field, value) in crate::validation::value_sites(schema, graph, vertex, *id) {
                if value.as_ref().is_none_or(|value| *value == Value::Null) {
                    missing.insert((elem(*id), field.unwrap_or_default()));
                }
            }
        }
    }
    missing
        .into_iter()
        .map(|(elem, field)| PutDomainError::MissingRequiredCell { elem, field })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{PutDomainError, ViewRef, structural_required_cell_violations};
    use crate::model::{GEdge, Graph, Node, SEdge};
    use crate::schema::Schema;
    use crate::value::Value;

    fn structural_required_fixture() -> (Schema, SEdge, Graph) {
        let edge = SEdge::new("a", "b", "rel", Some("b"));
        let mut schema = Schema::default()
            .vertex("a", "obj")
            .vertex("b", "obj")
            .edge("a", "b", "rel", Some("b"));
        schema.required.insert("a".to_owned(), vec![edge.clone()]);
        let graph = Graph {
            nodes: [(0, Node::new(0, "a"))].into(),
            ..Graph::default()
        };
        (schema, edge, graph)
    }

    #[test]
    fn structural_required_cell_reports_a_missing_edge_and_field() {
        let (schema, _, graph) = structural_required_fixture();

        assert_eq!(
            structural_required_cell_violations(&schema, &graph, ViewRef::Node),
            vec![PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(0),
                field: "b".to_owned(),
            }]
        );
    }

    #[test]
    fn structural_required_cell_accepts_an_exact_outgoing_edge() {
        let (schema, edge, mut graph) = structural_required_fixture();
        graph.nodes.insert(1, Node::new(1, "b"));
        graph.edges.insert(
            0,
            GEdge {
                src: 0,
                tgt: 1,
                label: edge,
                position: None,
            },
        );

        assert!(structural_required_cell_violations(&schema, &graph, ViewRef::Node).is_empty());
    }

    #[test]
    fn structural_required_cell_accepts_the_same_named_node_field() {
        let (schema, _, mut graph) = structural_required_fixture();
        let mut node = Node::new(0, "a");
        node.fields.insert("b".to_owned(), Value::Int(1));
        graph.nodes.insert(0, node);

        assert!(structural_required_cell_violations(&schema, &graph, ViewRef::Node).is_empty());
    }

    #[test]
    fn view_reference_order_matches_the_shape_native_contract() {
        let node = ViewRef::Node(u32::MAX);
        let edge = ViewRef::Edge(u32::MAX);
        let arc = ViewRef::Arc {
            parent: u32::MAX,
            child: u32::MAX,
        };
        let row = ViewRef::Row(0);
        let pair = ViewRef::Pair(Box::new(SEdge::new("a", "b", "r", None)), 0, 0);

        assert!(node < edge);
        assert!(edge < arc);
        assert!(arc < row);
        assert!(row < pair);
    }

    #[test]
    fn report_sort_removes_nonadjacent_exact_repeats_inside_a_tie_run() {
        let first = PutDomainError::AmbiguousReverseAnchor {
            elem: ViewRef::Node(7),
            candidates: vec!["a".to_owned()],
        };
        let second = PutDomainError::AmbiguousReverseAnchor {
            elem: ViewRef::Node(7),
            candidates: vec!["b".to_owned()],
        };
        let mut errors = vec![first.clone(), second.clone(), first.clone()];

        PutDomainError::sort_report(&mut errors);

        assert_eq!(errors, vec![first, second]);
    }
}
