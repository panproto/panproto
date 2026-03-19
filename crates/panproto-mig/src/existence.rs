//! Theory-derived existence checking.
//!
//! The conditions checked by [`check_existence`] are NOT hardcoded.
//! Instead, the function inspects the protocol's schema and instance
//! theory sorts to determine which checks apply. This keeps the
//! migration engine generic across protocols.

use std::collections::HashMap;

use panproto_gat::Theory;
use panproto_schema::{Edge, Protocol, Schema};
use rustc_hash::FxHashSet;

use crate::error::ExistenceError;
use crate::migration::Migration;

/// Result of existence checking.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ExistenceReport {
    /// Whether all conditions are satisfied.
    pub valid: bool,
    /// Individual errors (empty when `valid` is true).
    pub errors: Vec<ExistenceError>,
}

/// Check existence conditions for a migration.
///
/// The conditions checked are DERIVED from the schema and instance
/// theory structure -- not a hardcoded list. The function inspects
/// the theory's sorts to decide which checks to apply.
///
/// Always checks: vertex map validity, edge map validity, kind consistency.
/// Conditionally checks (based on theory sorts):
/// - `Constraint` sort present -> constraint compatibility
/// - `HyperEdge` sort present -> signature coherence + simultaneity
/// - Instance theory has `Node` sort (W-type) -> reachability risks
#[must_use]
pub fn check_existence(
    protocol: &Protocol,
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
    theory_registry: &HashMap<String, Theory>,
) -> ExistenceReport {
    let mut errors = Vec::new();

    // Look up the schema theory to determine which checks apply.
    let schema_theory = theory_registry.get(&protocol.schema_theory);

    if let Some(theory) = schema_theory {
        // Theory-derived conditional checks.
        if theory.find_sort("Constraint").is_some() {
            errors.extend(check_constraint_compatibility(src, tgt, migration));
        }
        if theory.find_sort("HyperEdge").is_some() {
            errors.extend(check_signature_coherence(src, tgt, migration));
            errors.extend(check_simultaneity(src, tgt, migration));
        }
    }

    // Look up the instance theory for W-type checks.
    let inst_theory = theory_registry.get(&protocol.instance_theory);
    if let Some(theory) = inst_theory {
        if theory.find_sort("Node").is_some() {
            errors.extend(check_reachability(src, tgt, migration));
        }
    }

    // New theory-derived checks from building blocks.
    if let Some(theory) = schema_theory {
        if theory.find_sort("Variant").is_some() {
            errors.extend(check_variant_preservation(src, tgt, migration));
        }
        if theory.find_sort("Position").is_some() {
            errors.extend(check_order_compatibility(src, tgt));
        }
        if theory.find_sort("Mu").is_some() {
            errors.extend(check_recursion_compatibility(src, tgt, migration));
        }
        if theory.find_sort("Usage").is_some() {
            errors.extend(check_linearity(src, tgt, migration));
        }
    }

    // Always check basic morphism validity.
    errors.extend(check_vertex_map(src, tgt, migration));
    errors.extend(check_edge_map(src, tgt, migration));
    errors.extend(check_kind_consistency(src, tgt, migration));

    ExistenceReport {
        valid: errors.is_empty(),
        errors,
    }
}

/// Verify that every mapped vertex exists in both source and target schemas.
fn check_vertex_map(src: &Schema, tgt: &Schema, migration: &Migration) -> Vec<ExistenceError> {
    let mut errors = Vec::new();
    for (src_v, tgt_v) in &migration.vertex_map {
        if !src.has_vertex(src_v) {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "vertex_map maps {src_v} to {tgt_v}, but {src_v} is not in the source schema"
                ),
            });
        }
        if !tgt.has_vertex(tgt_v) {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "vertex_map maps {src_v} to {tgt_v}, but {tgt_v} is not in the target schema"
                ),
            });
        }
    }
    errors
}

/// Verify that edge mappings are well-formed: source edges exist in the
/// source schema and target edges exist in the target schema.
fn check_edge_map(src: &Schema, tgt: &Schema, migration: &Migration) -> Vec<ExistenceError> {
    let mut errors = Vec::new();
    for (src_edge, tgt_edge) in &migration.edge_map {
        if !src.edges.contains_key(src_edge) {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "edge_map references source edge {} -> {} ({}), but it is not in the source schema",
                    src_edge.src, src_edge.tgt, src_edge.kind
                ),
            });
        }
        if !tgt.edges.contains_key(tgt_edge) {
            errors.push(ExistenceError::EdgeMissing {
                src: tgt_edge.src.to_string(),
                tgt: tgt_edge.tgt.to_string(),
                kind: tgt_edge.kind.to_string(),
            });
        }
    }
    errors
}

/// Check that vertices mapped to the same target have consistent kinds.
fn check_kind_consistency(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for (src_id, tgt_id) in &migration.vertex_map {
        let src_vertex = src.vertex(src_id);
        let tgt_vertex = tgt.vertex(tgt_id);

        if let (Some(sv), Some(tv)) = (src_vertex, tgt_vertex) {
            if sv.kind != tv.kind {
                errors.push(ExistenceError::KindInconsistency {
                    kind: sv.kind.to_string(),
                    targets: vec![sv.kind.to_string(), tv.kind.to_string()],
                });
            }
        }
    }

    errors
}

/// Check constraint compatibility: target constraints must not be
/// strictly tighter than source constraints.
fn check_constraint_compatibility(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for (src_id, tgt_id) in &migration.vertex_map {
        let src_constraints = src.constraints.get(src_id);
        let tgt_constraints = tgt.constraints.get(tgt_id);

        if let (Some(src_cs), Some(tgt_cs)) = (src_constraints, tgt_constraints) {
            for tgt_c in tgt_cs {
                // Find matching constraint in source by sort.
                if let Some(src_c) = src_cs.iter().find(|c| c.sort == tgt_c.sort) {
                    if is_constraint_tightened(&src_c.sort, &src_c.value, &tgt_c.value) {
                        errors.push(ExistenceError::ConstraintTightened {
                            vertex: tgt_id.to_string(),
                            sort: tgt_c.sort.to_string(),
                            src_val: src_c.value.clone(),
                            tgt_val: tgt_c.value.clone(),
                        });
                    }
                }
            }
        }

        // Also check: target requires fields that the source does not.
        let tgt_required = tgt.required.get(tgt_id);
        if let Some(required_edges) = tgt_required {
            let src_required: FxHashSet<&Edge> = src
                .required
                .get(src_id)
                .map_or_else(FxHashSet::default, |edges| edges.iter().collect());

            for req_edge in required_edges {
                // Check if this required edge has a preimage in the migration
                let has_preimage = migration.edge_map.values().any(|e| e == req_edge)
                    || src_required.iter().any(|&se| {
                        migration
                            .edge_map
                            .get(se)
                            .is_some_and(|mapped| mapped == req_edge)
                    });

                if !has_preimage {
                    errors.push(ExistenceError::RequiredFieldMissing {
                        vertex: tgt_id.to_string(),
                        field: req_edge.name.as_ref().map_or_else(
                            || format!("{} -> {}", req_edge.src, req_edge.tgt),
                            std::string::ToString::to_string,
                        ),
                    });
                }
            }
        }
    }

    errors
}

/// Determine if a constraint has been tightened (made more restrictive).
///
/// For numeric constraints like `maxLength`, a smaller target value is tighter.
/// For `minLength`, a larger target value is tighter.
fn is_constraint_tightened(sort: &str, src_val: &str, tgt_val: &str) -> bool {
    match sort {
        "maxLength" | "maxSize" | "maximum" => {
            // Tightened if target max < source max
            let src_n: Result<i64, _> = src_val.parse();
            let tgt_n: Result<i64, _> = tgt_val.parse();
            if let (Ok(s), Ok(t)) = (src_n, tgt_n) {
                return t < s;
            }
            false
        }
        "minLength" | "minimum" => {
            // Tightened if target min > source min
            let src_n: Result<i64, _> = src_val.parse();
            let tgt_n: Result<i64, _> = tgt_val.parse();
            if let (Ok(s), Ok(t)) = (src_n, tgt_n) {
                return t > s;
            }
            false
        }
        _ => {
            // For other constraint types, any change is potentially tightening
            src_val != tgt_val
        }
    }
}

/// Check hyper-edge signature coherence: mapped hyper-edges must have
/// compatible signatures.
fn check_signature_coherence(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for (src_he_id, tgt_he_id) in &migration.hyper_edge_map {
        let src_he = src.hyper_edges.get(src_he_id);
        let tgt_he = tgt.hyper_edges.get(tgt_he_id);

        if let (Some(she), Some(the)) = (src_he, tgt_he) {
            // Each label in the target signature must map to a surviving vertex.
            for (label, tgt_vertex_id) in &the.signature {
                // Determine the source label (via label_map or identity).
                let src_label = migration
                    .label_map
                    .get(&(src_he_id.clone(), label.clone()))
                    .cloned()
                    .unwrap_or_else(|| label.clone());

                if let Some(src_vertex_id) = she.signature.get(&src_label) {
                    // Verify the vertex mapping is consistent.
                    if let Some(mapped) = migration.vertex_map.get(src_vertex_id) {
                        if mapped != tgt_vertex_id {
                            errors.push(ExistenceError::SignatureCoherence {
                                hyper_edge: tgt_he_id.to_string(),
                                label: label.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    errors
}

/// Check simultaneity: all labels in a target hyper-edge must be
/// simultaneously present. Also verifies that dropped vertices
/// referenced by source hyper-edges actually exist in the source schema.
fn check_simultaneity(src: &Schema, tgt: &Schema, migration: &Migration) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    let surviving_verts: FxHashSet<&str> = migration.vertex_map.values().map(|n| &**n).collect();

    // Verify that source hyper-edge vertices exist in the source schema.
    for src_he_id in migration.hyper_edge_map.keys() {
        if let Some(he) = src.hyper_edges.get(src_he_id) {
            for (label, vertex_id) in &he.signature {
                if !src.has_vertex(vertex_id) {
                    errors.push(ExistenceError::WellFormedness {
                        message: format!(
                            "source hyper-edge {src_he_id} references vertex {vertex_id} (label {label}), but it is not in the source schema"
                        ),
                    });
                }
            }
        }
    }

    for tgt_he_id in migration.hyper_edge_map.values() {
        if let Some(he) = tgt.hyper_edges.get(tgt_he_id) {
            for (label, vertex_id) in &he.signature {
                if !surviving_verts.contains(&**vertex_id) {
                    errors.push(ExistenceError::Simultaneity {
                        hyper_edge: tgt_he_id.to_string(),
                        missing_label: label.to_string(),
                    });
                }
            }
        }
    }

    errors
}

/// Check reachability risks for W-type instances: vertices that become
/// disconnected from the root after migration.
fn check_reachability(src: &Schema, tgt: &Schema, migration: &Migration) -> Vec<ExistenceError> {
    let mut errors = Vec::new();
    let surviving: FxHashSet<&str> = migration.vertex_map.values().map(|n| &**n).collect();

    // For each surviving vertex, check reachability in both schemas.
    for (src_id, tgt_id) in &migration.vertex_map {
        // Verify the target vertex exists in the target schema.
        if !tgt.has_vertex(tgt_id) {
            errors.push(ExistenceError::ReachabilityRisk {
                vertex: tgt_id.to_string(),
                reason: format!("target vertex {tgt_id} does not exist in the target schema"),
            });
            continue;
        }

        // Check if all vertices on any path from root to this vertex survive.
        // We approximate this by checking if the vertex has at least one
        // incoming edge from a surviving vertex in the source.
        let has_surviving_parent = src
            .incoming_edges(src_id)
            .iter()
            .any(|e| migration.vertex_map.contains_key(&e.src) && surviving.contains(&*e.src));

        // Root vertices or vertices with surviving parents are fine.
        let is_root_like = src.incoming_edges(src_id).is_empty();
        if !is_root_like && !has_surviving_parent {
            errors.push(ExistenceError::ReachabilityRisk {
                vertex: tgt_id.to_string(),
                reason: format!("no surviving parent for vertex {src_id} in source schema"),
            });
        }
    }

    errors
}

/// Check that coproduct variants are preserved by the migration.
///
/// Dropping a variant from a coproduct is a type error — existing
/// data tagged with that variant becomes ill-typed.
fn check_variant_preservation(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for (parent_id, src_variants) in &src.variants {
        if let Some(tgt_parent) = migration.vertex_map.get(parent_id) {
            let tgt_variants = tgt.variants.get(tgt_parent).cloned().unwrap_or_default();
            let tgt_variant_ids: std::collections::HashSet<&str> =
                tgt_variants.iter().map(|v| &*v.id).collect();

            for v in src_variants {
                if !tgt_variant_ids.contains(&*v.id) {
                    errors.push(ExistenceError::WellFormedness {
                        message: format!(
                            "variant '{}' of coproduct '{}' was dropped (type error for existing data)",
                            v.id, parent_id
                        ),
                    });
                }
            }
        }
    }

    errors
}

/// Check that ordering compatibility is maintained.
///
/// Ordered → unordered is a lossy migration.
fn check_order_compatibility(src: &Schema, tgt: &Schema) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for edge in src.orderings.keys() {
        if !tgt.orderings.contains_key(edge) && tgt.edges.contains_key(edge) {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "edge {} → {} ({}) was ordered in source but unordered in target",
                    edge.src, edge.tgt, edge.kind
                ),
            });
        }
    }

    errors
}

/// Check that recursion structure is preserved.
///
/// Removing a recursion point breaks recursive types.
fn check_recursion_compatibility(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for (mu_id, rp) in &src.recursion_points {
        // Check if the fixpoint vertex survives.
        if migration.vertex_map.contains_key(&rp.target_vertex)
            && !tgt.recursion_points.contains_key(mu_id)
        {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "recursion point '{}' targeting '{}' was removed (breaks recursive types)",
                    mu_id, rp.target_vertex
                ),
            });
        }
    }

    errors
}

/// Check that linearity constraints are not tightened.
///
/// Structural → linear is a tightening that invalidates existing data
/// using the edge multiple times.
fn check_linearity(src: &Schema, tgt: &Schema, migration: &Migration) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for (src_edge, tgt_edge) in &migration.edge_map {
        let src_mode = src.usage_modes.get(src_edge).cloned().unwrap_or_default();
        let tgt_mode = tgt.usage_modes.get(tgt_edge).cloned().unwrap_or_default();

        let is_tightened = matches!(
            (&src_mode, &tgt_mode),
            (
                panproto_schema::UsageMode::Structural,
                panproto_schema::UsageMode::Linear | panproto_schema::UsageMode::Affine
            ) | (
                panproto_schema::UsageMode::Affine,
                panproto_schema::UsageMode::Linear
            )
        );

        if is_tightened {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "edge {} → {} ({}) usage tightened from {src_mode:?} to {tgt_mode:?}",
                    src_edge.src, src_edge.tgt, src_edge.kind
                ),
            });
        }
    }

    errors
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::{Constraint, Vertex};

    /// Helper: build a minimal protocol for testing.
    fn test_protocol(schema_theory: &str, instance_theory: &str) -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: schema_theory.into(),
            instance_theory: instance_theory.into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into()],
            constraint_sorts: vec!["maxLength".into()],
            ..Protocol::default()
        }
    }

    /// Helper: build a minimal schema with given vertices and edges.
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
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing,
            incoming,
            between,
        }
    }

    #[test]
    fn constraint_obstruction_detected() {
        // Test 4: constraint tightened maxLength 3000 -> 300
        let protocol = test_protocol("ThConstrained", "ThWType");
        let edge = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };

        let mut src = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge),
        );
        src.constraints.insert(
            Name::from("body.text"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".into(),
            }],
        );

        let mut tgt = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge),
        );
        tgt.constraints.insert(
            Name::from("body.text"),
            vec![Constraint {
                sort: "maxLength".into(),
                value: "300".into(),
            }],
        );

        let mig = Migration {
            vertex_map: HashMap::from([
                (Name::from("body"), Name::from("body")),
                (Name::from("body.text"), Name::from("body.text")),
            ]),
            edge_map: HashMap::from([(edge.clone(), edge)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        // Provide a theory with Constraint sort so the check fires.
        let mut registry = HashMap::new();
        registry.insert(
            "ThConstrained".into(),
            Theory::new(
                "ThConstrained",
                vec![
                    panproto_gat::Sort::simple("Vertex"),
                    panproto_gat::Sort::simple("Edge"),
                    panproto_gat::Sort::simple("Constraint"),
                ],
                vec![],
                vec![],
            ),
        );

        let report = check_existence(&protocol, &src, &tgt, &mig, &registry);
        assert!(!report.valid, "should detect constraint tightening");
        assert!(
            report
                .errors
                .iter()
                .any(|e| matches!(e, ExistenceError::ConstraintTightened { .. })),
            "expected ConstraintTightened error"
        );
    }

    #[test]
    fn kind_inconsistency_detected() {
        // Test 5: string -> int is a kind inconsistency
        let protocol = test_protocol("ThGraph", "ThWType");
        let src = test_schema(&[("body", "object"), ("body.text", "string")], &[]);
        let tgt = test_schema(&[("body", "object"), ("body.text", "integer")], &[]);

        let mig = Migration {
            vertex_map: HashMap::from([
                (Name::from("body"), Name::from("body")),
                (Name::from("body.text"), Name::from("body.text")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let registry = HashMap::new();
        let report = check_existence(&protocol, &src, &tgt, &mig, &registry);
        assert!(!report.valid, "should detect kind inconsistency");
        assert!(
            report
                .errors
                .iter()
                .any(|e| matches!(e, ExistenceError::KindInconsistency { .. })),
            "expected KindInconsistency error"
        );
    }

    #[test]
    fn required_field_missing_detected() {
        // Test 6: target requires "name", source lacks it
        let protocol = test_protocol("ThConstrained", "ThWType");
        let name_edge = Edge {
            src: "body".into(),
            tgt: "body.name".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        };

        let src = test_schema(&[("body", "object")], &[]);

        let mut tgt = test_schema(
            &[("body", "object"), ("body.name", "string")],
            std::slice::from_ref(&name_edge),
        );
        tgt.required.insert(Name::from("body"), vec![name_edge]);

        let mig = Migration {
            vertex_map: HashMap::from([(Name::from("body"), Name::from("body"))]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
        };

        let mut registry = HashMap::new();
        registry.insert(
            "ThConstrained".into(),
            Theory::new(
                "ThConstrained",
                vec![
                    panproto_gat::Sort::simple("Vertex"),
                    panproto_gat::Sort::simple("Constraint"),
                ],
                vec![],
                vec![],
            ),
        );

        let report = check_existence(&protocol, &src, &tgt, &mig, &registry);
        assert!(!report.valid, "should detect required field missing");
        assert!(
            report
                .errors
                .iter()
                .any(|e| matches!(e, ExistenceError::RequiredFieldMissing { .. })),
            "expected RequiredFieldMissing error"
        );
    }
}
