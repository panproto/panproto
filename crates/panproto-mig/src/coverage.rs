//! Coverage analysis for partial migrations.
//!
//! Provides dry-run capability: check which records can be migrated
//! and which will fail, before modifying any data.

use panproto_inst::{BoundaryProtocols, CompiledMigration, EdgeId, GInstance, WInstance};
use panproto_schema::Schema;
use serde::{Deserialize, Serialize};

/// A report of migration coverage across a set of records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageReport {
    /// Total number of records analyzed.
    pub total_records: usize,
    /// Number of records that can be successfully migrated.
    pub successful: usize,
    /// Records that failed, with reasons.
    pub failed: Vec<PartialFailure>,
    /// Ratio of successful to total (0.0..=1.0).
    pub coverage_ratio: f64,
}

/// A single record that failed migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialFailure {
    /// The record's node ID in the source instance.
    pub record_id: u32,
    /// Why the migration failed for this record.
    pub reason: PartialReason,
}

/// The reason a record failed to migrate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartialReason {
    /// A constraint was violated (e.g., value too long after coercion).
    ConstraintViolation {
        /// The constraint that was violated.
        constraint: String,
        /// The value that violated the constraint.
        value: String,
    },
    /// A required field was missing and no default was provided.
    MissingRequiredField {
        /// The missing field name.
        field: String,
    },
    /// A type coercion failed (e.g., `parse_int` on non-numeric string).
    TypeMismatch {
        /// The expected type.
        expected: String,
        /// The actual type encountered.
        got: String,
    },
    /// A custom expression evaluation failed.
    ExprEvalFailed {
        /// The expression name that failed.
        expr_name: String,
        /// The error message.
        error: String,
    },
}

/// Check migration coverage by attempting the restrict on each record.
///
/// Each tree is one record, named by its root. A record succeeds when the
/// schema-taking restrict ([`panproto_inst::wtype_restrict`], which
/// validates both boundaries under `protocols`) returns a view, and fails
/// with the reason its error gives otherwise.
#[must_use]
pub fn check_coverage(
    compiled: &CompiledMigration,
    instances: &[WInstance],
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocols: BoundaryProtocols<'_>,
) -> CoverageReport {
    let outcomes = instances.iter().map(|instance| {
        let outcome =
            panproto_inst::wtype_restrict(instance, src_schema, tgt_schema, compiled, protocols)
                .map(|_| ())
                .map_err(|e| restrict_error_to_reason(e.unlocated()));
        (instance.root, outcome)
    });
    coverage_of(outcomes)
}

/// The coverage of records with their outcomes.
fn coverage_of(
    outcomes: impl IntoIterator<Item = (u32, Result<(), PartialReason>)>,
) -> CoverageReport {
    let mut total_records: usize = 0;
    let mut successful: usize = 0;
    let mut failed = Vec::new();
    for (record_id, outcome) in outcomes {
        total_records += 1;
        match outcome {
            Ok(()) => successful += 1,
            Err(reason) => failed.push(PartialFailure { record_id, reason }),
        }
    }
    let as_float = |n: usize| f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    let coverage_ratio = if total_records == 0 {
        1.0
    } else {
        as_float(successful) / as_float(total_records)
    };
    CoverageReport {
        total_records,
        successful,
        failed,
        coverage_ratio,
    }
}

/// The coverage of a graph migration, with the ids the successful records
/// drop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphCoverageReport {
    /// The coverage: one record per weakly connected component.
    pub coverage: CoverageReport,
    /// The nodes the restrict drops from the records that succeed,
    /// ascending.
    pub dropped_nodes: Vec<u32>,
    /// The edges the restrict drops from the records that succeed,
    /// ascending.
    pub dropped_edges: Vec<EdgeId>,
}

/// Check graph migration coverage, one record per weakly connected
/// component of each instance.
///
/// A record is named by the least node id of its component. It fails
/// exactly when the schema-taking graph restrict
/// ([`panproto_inst::graph_restrict_with_complement`], which validates both
/// boundaries under `protocols`) of the component's induced subgraph
/// returns an error, with the reason that error (without its location)
/// gives; dropping data is not a failure. The ids the restrict drops from
/// the components that succeed, read off their complements, are reported
/// beside the coverage.
#[must_use]
pub fn check_graph_coverage(
    compiled: &CompiledMigration,
    instances: &[GInstance],
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocols: BoundaryProtocols<'_>,
) -> GraphCoverageReport {
    let mut dropped_nodes = std::collections::BTreeSet::new();
    let mut dropped_edges = std::collections::BTreeSet::new();
    let mut outcomes = Vec::new();
    for instance in instances {
        for component in weak_components(instance) {
            let record_id = component.first().copied().unwrap_or_default();
            let outcome = induced(instance, &component)
                .map_err(|error| PartialReason::ExprEvalFailed {
                    expr_name: "induced subgraph".into(),
                    error: error.to_string(),
                })
                .and_then(|part| {
                    panproto_inst::graph_restrict_with_complement(
                        &part, src_schema, tgt_schema, compiled, protocols,
                    )
                    .map_err(|e| restrict_error_to_reason(e.unlocated()))
                })
                .map(|(_, complement)| {
                    dropped_nodes.extend(complement.dropped_nodes.keys().copied());
                    dropped_edges.extend(complement.dropped_edges.keys().copied());
                });
            outcomes.push((record_id, outcome));
        }
    }
    GraphCoverageReport {
        coverage: coverage_of(outcomes),
        dropped_nodes: dropped_nodes.into_iter().collect(),
        dropped_edges: dropped_edges.into_iter().collect(),
    }
}

/// The weakly connected components of a graph, each ascending, in
/// ascending order of their least node.
fn weak_components(instance: &GInstance) -> Vec<Vec<u32>> {
    let mut ids: Vec<u32> = instance.nodes().keys().copied().collect();
    ids.sort_unstable();
    let mut seen = std::collections::HashSet::new();
    let mut components = Vec::new();
    for &start in &ids {
        if !seen.insert(start) {
            continue;
        }
        let mut component = vec![start];
        let mut frontier = vec![start];
        while let Some(at) = frontier.pop() {
            let neighbours = instance
                .successors(at)
                .chain(instance.predecessors(at))
                .collect::<Vec<_>>();
            for next in neighbours {
                if seen.insert(next) {
                    component.push(next);
                    frontier.push(next);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    components
}

/// The subgraph of `instance` induced by `nodes`, its edges under their own
/// ids.
fn induced(instance: &GInstance, nodes: &[u32]) -> Result<GInstance, panproto_inst::InstError> {
    let mut part = GInstance::new();
    for &id in nodes {
        if let Some(node) = instance.node(id) {
            part.insert_node(node.clone())?;
        }
    }
    for &id in nodes {
        for &edge_id in instance.out_edges(id) {
            if let Some(edge) = instance.edge(edge_id) {
                part.insert_edge(edge_id, edge.clone())?;
            }
        }
    }
    Ok(part)
}

/// Map a [`panproto_inst::RestrictError`] to a [`PartialReason`].
///
/// Uses the structured error variants from the restrict algorithm's five
/// stages (anchor surviving, reachability, ancestor contraction, edge
/// resolution, fan reconstruction) rather than string matching.
fn restrict_error_to_reason(err: &panproto_inst::RestrictError) -> PartialReason {
    use panproto_inst::RestrictError;
    match err {
        RestrictError::NoEdgeFound { src, tgt } => PartialReason::MissingRequiredField {
            field: format!("edge {src} → {tgt}"),
        },
        RestrictError::AmbiguousEdge { src, tgt, count } => PartialReason::TypeMismatch {
            expected: format!("unique edge {src} → {tgt}"),
            got: format!("{count} candidates"),
        },
        RestrictError::RootPruned => PartialReason::MissingRequiredField {
            field: "root node (pruned during restriction)".into(),
        },
        RestrictError::FanReconstructionFailed {
            hyper_edge_id,
            detail,
        } => PartialReason::ConstraintViolation {
            constraint: format!("fan reconstruction for {hyper_edge_id}"),
            value: detail.clone(),
        },
        // Handle any future RestrictError variants.
        other => PartialReason::ExprEvalFailed {
            expr_name: String::new(),
            error: other.to_string(),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use panproto_gat::Name;
    use panproto_inst::value::FieldPresence;
    use panproto_inst::{CompiledMigration, Node, Value, WInstance};
    use panproto_schema::{Edge, Protocol, Schema, Vertex};

    use super::*;

    /// The same protocol on both sides.
    fn both(protocol: &Protocol) -> BoundaryProtocols<'_> {
        BoundaryProtocols {
            src: protocol,
            tgt: protocol,
        }
    }

    fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }

        for edge in edges {
            edge_map.insert(edge.clone(), edge.kind.clone());
            outgoing
                .entry(edge.src.clone())
                .or_default()
                .push(edge.clone());
            incoming
                .entry(edge.tgt.clone())
                .or_default()
                .push(edge.clone());
            between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_default()
                .push(edge.clone());
        }

        Schema {
            protocol: "test".into(),
            vertices: vert_map,
            edges: edge_map,
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing,
            incoming,
            between,
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            ..Default::default()
        }
    }

    fn make_instance(root_id: u32, text: &str) -> (Schema, WInstance) {
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };

        let schema = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge_text),
        );

        let mut nodes = HashMap::new();
        nodes.insert(root_id, Node::new(root_id, "body"));
        nodes.insert(
            root_id + 1,
            Node::new(root_id + 1, "body.text")
                .with_value(FieldPresence::Present(Value::Str(text.into()))),
        );

        let arcs = vec![(root_id, root_id + 1, edge_text)];
        let instance = WInstance::new(nodes, arcs, vec![], root_id, Name::from("body"));

        (schema, instance)
    }

    #[test]
    fn coverage_all_successful() {
        let (schema, inst1) = make_instance(0, "hello");
        let (_, inst2) = make_instance(10, "world");

        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["body".into(), "body.text".into()]),
            surviving_edges: HashSet::from([Edge {
                src: "body".into(),
                tgt: "body.text".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            }]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let report = check_coverage(
            &compiled,
            &[inst1, inst2],
            &schema,
            &schema,
            both(&Protocol::default()),
        );
        assert_eq!(report.total_records, 2);
        assert_eq!(report.successful, 2);
        assert!(report.failed.is_empty());
        assert!((report.coverage_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn coverage_empty_instances() {
        let (schema, _) = make_instance(0, "unused");

        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["body".into()]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let report = check_coverage(&compiled, &[], &schema, &schema, both(&Protocol::default()));
        assert_eq!(report.total_records, 0);
        assert_eq!(report.successful, 0);
        assert!(report.failed.is_empty());
        assert!((report.coverage_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn coverage_partial_failure() {
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };

        let src_schema = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge_text),
        );

        let tgt_schema = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge_text),
        );

        // First instance: normal, should succeed
        let (_, good_instance) = make_instance(0, "hello");

        // Second instance: anchored to a vertex not in surviving set
        let mut bad_nodes = HashMap::new();
        bad_nodes.insert(20, Node::new(20, "nonexistent_type"));
        let bad_instance = WInstance::new(
            bad_nodes,
            vec![],
            vec![],
            20,
            Name::from("nonexistent_type"),
        );

        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["body".into(), "body.text".into()]),
            surviving_edges: HashSet::from([edge_text]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let report = check_coverage(
            &compiled,
            &[good_instance, bad_instance],
            &src_schema,
            &tgt_schema,
            both(&Protocol::default()),
        );
        assert_eq!(report.total_records, 2);
        assert_eq!(report.successful, 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].record_id, 20);
        assert!(report.coverage_ratio < 1.0);
        assert!(report.coverage_ratio > 0.0);
    }

    #[test]
    fn partial_reason_variants_serialize() {
        let reasons = vec![
            PartialReason::ConstraintViolation {
                constraint: "maxLength".into(),
                value: "too long".into(),
            },
            PartialReason::MissingRequiredField {
                field: "name".into(),
            },
            PartialReason::TypeMismatch {
                expected: "integer".into(),
                got: "string".into(),
            },
            PartialReason::ExprEvalFailed {
                expr_name: "coerce".into(),
                error: "parse failed".into(),
            },
        ];

        for reason in &reasons {
            let json = serde_json::to_string(reason).unwrap();
            let _roundtrip: PartialReason = serde_json::from_str(&json).unwrap();
        }
    }
}
