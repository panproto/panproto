//! Existence checking gated by the well-known-sort convention.
//!
//! [`check_existence`] runs a fixed set of always-on morphism checks plus a
//! set of conditional obligations. Each conditional obligation is gated on the
//! presence of a conventionally-named sort — listed in [`WELL_KNOWN_SORTS`] —
//! in the protocol's schema or instance theory. The sort names are normative:
//! a protocol theory opts into a check by naming the relevant sort exactly
//! (`Constraint`, `HyperEdge`, `Node`, `Variant`, `Position`, `Mu`, `Usage`);
//! a theory that uses a different name for an equivalent sort receives no
//! conditional check. The theory thus acts as a name-keyed feature registry
//! for these obligations, not a structural derivation of them.
//!
//! Deriving obligations from theory *structure* — the operations and equations
//! that mention a sort — rather than from sort names is a possible future
//! refinement; it is out of scope here.

use std::collections::HashMap;

use panproto_gat::{Name, Theory};
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

/// Which theory a well-known sort is looked up in.
#[derive(Debug, Clone, Copy)]
pub enum TheoryKind {
    /// The protocol's schema theory (`protocol.schema_theory`).
    Schema,
    /// The protocol's instance theory (`protocol.instance_theory`).
    Instance,
}

/// A conditional existence check over a source schema, target schema, and
/// migration, returning any obligations it detects as violated.
pub type ExistenceCheck = fn(&Schema, &Schema, &Migration) -> Vec<ExistenceError>;

/// One entry in the well-known-sort convention: a conventionally-named sort,
/// the theory it must appear in, and the check that fires when it is present.
pub struct WellKnownSort {
    /// The conventional sort name that gates this obligation.
    pub sort: &'static str,
    /// Which theory the sort must appear in for the obligation to fire.
    pub theory: TheoryKind,
    /// The check run when the sort is present in that theory.
    pub check: ExistenceCheck,
}

/// The normative well-known-sort convention used by [`check_existence`].
///
/// Each entry gates a conditional obligation on the presence of a
/// conventionally-named sort in the schema or instance theory. Protocol
/// theories opt into a check by naming the corresponding sort exactly as
/// listed here; a theory that names an equivalent sort differently receives no
/// conditional check. This is a naming convention, not a structural
/// derivation. The `HyperEdge` sort gates two independent checks and so
/// appears twice.
pub const WELL_KNOWN_SORTS: &[WellKnownSort] = &[
    WellKnownSort {
        sort: "Constraint",
        theory: TheoryKind::Schema,
        check: check_constraint_compatibility,
    },
    WellKnownSort {
        sort: "HyperEdge",
        theory: TheoryKind::Schema,
        check: check_signature_coherence,
    },
    WellKnownSort {
        sort: "HyperEdge",
        theory: TheoryKind::Schema,
        check: check_simultaneity,
    },
    WellKnownSort {
        sort: "Node",
        theory: TheoryKind::Instance,
        check: check_reachability,
    },
    WellKnownSort {
        sort: "Variant",
        theory: TheoryKind::Schema,
        check: check_variant_preservation,
    },
    WellKnownSort {
        sort: "Position",
        theory: TheoryKind::Schema,
        check: check_order_compatibility,
    },
    WellKnownSort {
        sort: "Mu",
        theory: TheoryKind::Schema,
        check: check_recursion_compatibility,
    },
    WellKnownSort {
        sort: "Usage",
        theory: TheoryKind::Schema,
        check: check_linearity,
    },
];

/// Check existence conditions for a migration.
///
/// Always checks: vertex map validity, edge map validity, kind consistency.
///
/// Conditional checks are gated by the well-known-sort convention (see
/// [`WELL_KNOWN_SORTS`]): an obligation fires only when its conventionally-
/// named sort is present in the relevant theory. The sort names are normative;
/// a theory that names an equivalent sort differently receives no conditional
/// check.
///
/// - `Constraint` in the schema theory -> constraint compatibility
/// - `HyperEdge` in the schema theory -> signature coherence + simultaneity
/// - `Node` in the instance theory (W-type) -> reachability risks
/// - `Variant` in the schema theory -> variant preservation
/// - `Position` in the schema theory -> order compatibility
/// - `Mu` in the schema theory -> recursion compatibility
/// - `Usage` in the schema theory -> linearity
#[must_use]
pub fn check_existence(
    protocol: &Protocol,
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
    theory_registry: &HashMap<String, Theory>,
) -> ExistenceReport {
    let mut errors = Vec::new();

    // Look up the schema and instance theories named by the protocol; the
    // well-known-sort convention gates each conditional obligation on the
    // presence of a conventionally-named sort in one of them.
    let schema_theory = theory_registry.get(&protocol.schema_theory);
    let inst_theory = theory_registry.get(&protocol.instance_theory);

    for entry in WELL_KNOWN_SORTS {
        let theory = match entry.theory {
            TheoryKind::Schema => schema_theory,
            TheoryKind::Instance => inst_theory,
        };
        if let Some(theory) = theory {
            if theory.find_sort(entry.sort).is_some() {
                errors.extend((entry.check)(src, tgt, migration));
            }
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
///
/// Performs a full BFS from root vertices (those with no incoming edges)
/// through the source schema's edge graph. Non-surviving intermediate
/// vertices are traversed but not counted as reachable; only surviving
/// vertices reached through the BFS are considered reachable. A visited
/// set prevents infinite loops in schemas with cycles.
fn check_reachability(src: &Schema, _tgt: &Schema, migration: &Migration) -> Vec<ExistenceError> {
    use std::collections::VecDeque;

    let mut errors = Vec::new();
    let surviving_src: FxHashSet<&str> = migration.vertex_map.keys().map(|n| &**n).collect();

    // Find root vertices: source schema vertices with no incoming edges.
    let roots: Vec<&Name> = src
        .vertices
        .keys()
        .filter(|v| src.incoming_edges(v).is_empty())
        .collect();

    // BFS from all roots through the source schema edge graph.
    // We traverse through ALL vertices (including non-surviving intermediates)
    // but only track which surviving vertices are reachable.
    let mut visited: FxHashSet<&str> = FxHashSet::default();
    let mut reachable_surviving: FxHashSet<&str> = FxHashSet::default();
    let mut queue: VecDeque<&str> = VecDeque::new();

    for root in &roots {
        let root_str: &str = root;
        if visited.insert(root_str) {
            queue.push_back(root_str);
            if surviving_src.contains(root_str) {
                reachable_surviving.insert(root_str);
            }
        }
    }

    while let Some(current) = queue.pop_front() {
        // Follow outgoing edges from the current vertex
        for edge in src.outgoing_edges(current) {
            let target = &*edge.tgt;
            if visited.insert(target) {
                queue.push_back(target);
                if surviving_src.contains(target) {
                    reachable_surviving.insert(target);
                }
            }
        }
    }

    // Report surviving vertices that are not reachable from any root.
    for (src_id, tgt_id) in &migration.vertex_map {
        if !reachable_surviving.contains(&**src_id) {
            errors.push(ExistenceError::ReachabilityRisk {
                vertex: tgt_id.to_string(),
                reason: format!(
                    "vertex {src_id} is not reachable from any root in the source schema"
                ),
            });
        }
    }

    errors
}

/// Check that coproduct variants are preserved by the migration.
///
/// Dropping a variant from a coproduct is a type error; existing
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
/// Ordered to unordered is a lossy migration. Source edges are remapped
/// through the migration's `edge_map` before comparing against target
/// orderings.
fn check_order_compatibility(
    src: &Schema,
    tgt: &Schema,
    migration: &Migration,
) -> Vec<ExistenceError> {
    let mut errors = Vec::new();

    for edge in src.orderings.keys() {
        // Remap the source edge through the migration's edge_map
        let tgt_edge = migration.edge_map.get(edge).unwrap_or(edge);
        if !tgt.orderings.contains_key(tgt_edge) && tgt.edges.contains_key(tgt_edge) {
            errors.push(ExistenceError::WellFormedness {
                message: format!(
                    "edge {} -> {} ({}) was ordered in source but unordered in target",
                    tgt_edge.src, tgt_edge.tgt, tgt_edge.kind
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
            entries: Vec::new(),
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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
            domain: None,
            codomain: None,
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

    #[test]
    fn well_known_sort_convention_is_name_keyed() {
        // A tightened maxLength (3000 -> 300) is a ConstraintTightened
        // obligation that fires only when the schema theory names a
        // `Constraint` sort. A theory that names an equivalent sort differently
        // (`ConstraintX`) opts out of the check, pinning the convention as
        // name-keyed rather than structurally derived.
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
            domain: None,
            codomain: None,
        };

        let constraint_tightened = |report: &ExistenceReport| {
            report
                .errors
                .iter()
                .any(|e| matches!(e, ExistenceError::ConstraintTightened { .. }))
        };

        // Theory names the sort `Constraint`: the obligation fires.
        let mut named_registry = HashMap::new();
        named_registry.insert(
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
        let named_report = check_existence(&protocol, &src, &tgt, &mig, &named_registry);
        assert!(
            constraint_tightened(&named_report),
            "a `Constraint` sort must trigger the constraint-compatibility check"
        );

        // Theory names an equivalent sort `ConstraintX`: the obligation does
        // NOT fire, even though the schema data is identical.
        let mut renamed_registry = HashMap::new();
        renamed_registry.insert(
            "ThConstrained".into(),
            Theory::new(
                "ThConstrained",
                vec![
                    panproto_gat::Sort::simple("Vertex"),
                    panproto_gat::Sort::simple("Edge"),
                    panproto_gat::Sort::simple("ConstraintX"),
                ],
                vec![],
                vec![],
            ),
        );
        let renamed_report = check_existence(&protocol, &src, &tgt, &mig, &renamed_registry);
        assert!(
            !constraint_tightened(&renamed_report),
            "a renamed `ConstraintX` sort must not trigger the constraint check"
        );
    }
}
