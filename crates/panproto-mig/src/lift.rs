//! Lift operations: applying compiled migrations to instances.
//!
//! `lift_wtype` and `lift_functor` delegate to the restrict
//! implementations in `panproto-inst`, passing the compiled
//! migration's precomputed tables.

use panproto_inst::{CompiledMigration, FInstance, WInstance};
use panproto_schema::Schema;

use crate::error::LiftError;

/// Apply a compiled migration to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_restrict`], which executes the
/// 5-step pipeline: anchor surviving, reachability BFS, ancestor
/// contraction, edge resolution, and fan reconstruction.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying restrict operation
/// fails (e.g., edge resolution ambiguity, root pruned).
pub fn lift_wtype(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &WInstance,
) -> Result<WInstance, LiftError> {
    let result = panproto_inst::wtype_restrict(instance, src_schema, tgt_schema, compiled)?;
    Ok(result)
}

/// Apply a compiled migration to a set-valued functor instance.
///
/// Delegates to [`panproto_inst::functor_restrict`], which performs
/// precomposition (`Delta_F`): for each table in the target, pull the
/// corresponding table from the source via the vertex remap.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying restrict operation fails.
pub fn lift_functor(
    compiled: &CompiledMigration,
    instance: &FInstance,
) -> Result<FInstance, LiftError> {
    let result = panproto_inst::functor_restrict(instance, compiled)?;
    Ok(result)
}

/// Apply a compiled migration as a left Kan extension (`Sigma_F`) to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_extend`], which pushes every node
/// forward along the migration morphism, remapping anchors and edges. This
/// is a *total* operation: it requires each source node's anchor to be
/// remapped or surviving, and reports an unmapped anchor rather than
/// dropping the node silently.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying extend operation fails,
/// including when a source node's anchor is neither remapped nor surviving
/// (`RestrictError::UnmappedAnchor`).
pub fn lift_wtype_sigma(
    compiled: &CompiledMigration,
    tgt_schema: &Schema,
    instance: &WInstance,
) -> Result<WInstance, LiftError> {
    let result = panproto_inst::wtype_extend(instance, tgt_schema, compiled)?;
    Ok(result)
}

/// Apply a compiled migration as a right Kan extension (`Pi_F`) to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_pi`], which is defined only for
/// vertex-injective migrations: under that restriction it relabels anchors
/// and edges without forming any product. A non-injective migration is
/// rejected rather than silently producing `Sigma`-shaped output; the
/// `max_product_nodes` argument is retained for signature compatibility and
/// imposes no bound on this path.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying pi operation fails —
/// notably `RestrictError::NonInjectiveVertexMap` when two source vertices
/// map to one target, or `RestrictError::UnmappedAnchor` when a node's
/// anchor is neither remapped nor surviving.
pub fn lift_wtype_pi(
    compiled: &CompiledMigration,
    tgt_schema: &Schema,
    instance: &WInstance,
    max_product_nodes: usize,
) -> Result<WInstance, LiftError> {
    let result = panproto_inst::wtype_pi(instance, tgt_schema, compiled, max_product_nodes)?;
    Ok(result)
}

/// Apply a compiled migration as a left Kan extension (`Sigma_F`) to a
/// functor instance, then close it under a term-level chase.
///
/// This is the `Sigma` pipeline entry for set-valued instances: it runs
/// `Sigma_F` ([`panproto_inst::functor_extend`]) and then saturates the
/// result under `dependencies` with [`crate::chase::chase`], enforcing
/// tuple- and equality-generating dependencies (with labeled nulls) that a
/// pure extension cannot. Pass an empty `dependencies` slice to run
/// `Sigma_F` alone.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the extension fails,
/// `LiftError::Chase` carrying the chase's own error if the chase
/// reports an equality conflict or a saturation that did not converge,
/// or `LiftError::ChaseBudgetExhausted` naming the budget when the
/// term-level chase runs out of it. `LiftError::is_retryable` tells the
/// budget failures apart from the conflict.
pub fn lift_functor_sigma(
    compiled: &CompiledMigration,
    instance: &FInstance,
    dependencies: &[crate::chase::Dependency],
    budget: crate::chase::ChaseBudget,
) -> Result<FInstance, LiftError> {
    let extended = panproto_inst::functor_extend(instance, compiled)?;
    if dependencies.is_empty() {
        return Ok(extended);
    }
    match crate::chase::chase(&extended, dependencies, budget)? {
        crate::chase::ChaseOutcome::Saturated(result) => Ok(result),
        crate::chase::ChaseOutcome::NonTermination => Err(LiftError::ChaseBudgetExhausted {
            max_iterations: budget.max_iterations,
            max_nulls: budget.max_nulls,
        }),
    }
}

/// Apply a compiled migration as a right Kan extension (`Pi_F`) to a functor instance.
///
/// Delegates to [`panproto_inst::functor_pi`], which computes Cartesian
/// products over fibers.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying pi operation fails
/// (e.g., product size exceeded).
pub fn lift_functor_pi(
    compiled: &CompiledMigration,
    instance: &FInstance,
    max_product_size: usize,
) -> Result<FInstance, LiftError> {
    let result = panproto_inst::functor_pi(instance, compiled, max_product_size)?;
    Ok(result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use panproto_inst::value::FieldPresence;
    use panproto_inst::{Node, Value, WInstanceHom};
    use panproto_schema::{Edge, Vertex};

    fn test_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let mut vert_map = HashMap::new();
        let mut edge_map = HashMap::new();
        let mut outgoing: HashMap<panproto_gat::Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut incoming: HashMap<panproto_gat::Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
        let mut between: HashMap<
            (panproto_gat::Name, panproto_gat::Name),
            smallvec::SmallVec<Edge, 2>,
        > = HashMap::new();

        for (id, kind) in vertices {
            vert_map.insert(
                panproto_gat::Name::from(*id),
                Vertex {
                    id: panproto_gat::Name::from(*id),
                    kind: panproto_gat::Name::from(*kind),
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
    fn identity_migration_preserves_all_nodes() {
        // Test 1: identity migration preserves all nodes.
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "body".into(),
            tgt: "body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };

        let schema = test_schema(
            &[
                ("body", "object"),
                ("body.text", "string"),
                ("body.createdAt", "string"),
            ],
            &[edge_text.clone(), edge_time.clone()],
        );

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![(0, 1, edge_text.clone()), (0, 2, edge_time.clone())];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("body"));

        // Identity compiled migration
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from([
                "body".into(),
                "body.text".into(),
                "body.createdAt".into(),
            ]),
            surviving_edges: HashSet::from([edge_text, edge_time]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = lift_wtype(&compiled, &schema, &schema, &instance);
        assert!(result.is_ok(), "identity lift should succeed");
        let lifted = result.unwrap_or_else(|_| panic!("lift should succeed"));
        assert_eq!(
            lifted.node_count(),
            instance.node_count(),
            "identity migration should preserve all nodes"
        );
        assert_eq!(
            lifted.arc_count(),
            instance.arc_count(),
            "identity migration should preserve all arcs"
        );
    }

    #[test]
    fn projection_drops_vertices() {
        // Test 2: projection - drop vertices, verify surviving set.
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let edge_time = Edge {
            src: "body".into(),
            tgt: "body.createdAt".into(),
            kind: "prop".into(),
            name: Some("createdAt".into()),
        };

        let schema = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge_text),
        );

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        nodes.insert(
            2,
            Node::new(2, "body.createdAt")
                .with_value(FieldPresence::Present(Value::Str("2024-01-01".into()))),
        );

        let arcs = vec![(0, 1, edge_text.clone()), (0, 2, edge_time)];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("body"));

        // Migration that drops body.createdAt
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

        let result = lift_wtype(&compiled, &schema, &schema, &instance);
        assert!(result.is_ok(), "projection lift should succeed");
        let lifted = result.unwrap_or_else(|_| panic!("lift should succeed"));
        assert_eq!(lifted.node_count(), 2, "should have 2 surviving nodes");
        assert!(lifted.nodes.contains_key(&0), "root should survive");
        assert!(lifted.nodes.contains_key(&1), "text node should survive");
        assert!(
            !lifted.nodes.contains_key(&2),
            "createdAt node should be dropped"
        );
    }

    /// Test functoriality of Σ (left Kan extension): lifting along a
    /// composed migration should equal sequential lifting.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn sigma_functoriality() {
        let edge_text = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };

        // Schema 1: body + body.text + body.createdAt (source, not used directly)
        let _s1 = test_schema(
            &[
                ("body", "object"),
                ("body.text", "string"),
                ("body.createdAt", "string"),
            ],
            &[
                edge_text.clone(),
                Edge {
                    src: "body".into(),
                    tgt: "body.createdAt".into(),
                    kind: "prop".into(),
                    name: Some("createdAt".into()),
                },
            ],
        );

        // Schema 2: body + body.text (drop createdAt)
        let s2 = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&edge_text),
        );

        // Schema 3: post + post.text (rename body -> post)
        let edge_text_renamed = Edge {
            src: "post".into(),
            tgt: "post.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let s3 = test_schema(
            &[("post", "object"), ("post.text", "string")],
            std::slice::from_ref(&edge_text_renamed),
        );

        // Migration m1: s1 -> s2 (drop createdAt)
        let m1 = CompiledMigration {
            surviving_verts: HashSet::from(["body".into(), "body.text".into()]),
            surviving_edges: HashSet::from([edge_text.clone()]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // Migration m2: s2 -> s3 (rename body->post, body.text->post.text)
        let m2 = CompiledMigration {
            surviving_verts: HashSet::from(["post".into(), "post.text".into()]),
            surviving_edges: HashSet::from([edge_text_renamed.clone()]),
            vertex_remap: HashMap::from([
                ("body".into(), "post".into()),
                ("body.text".into(), "post.text".into()),
            ]),
            edge_remap: HashMap::from([(edge_text.clone(), edge_text_renamed.clone())]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // Composed m12: s1 -> s3 directly
        let m12 = CompiledMigration {
            surviving_verts: HashSet::from(["post".into(), "post.text".into()]),
            surviving_edges: HashSet::from([edge_text_renamed]),
            vertex_remap: HashMap::from([
                ("body".into(), "post".into()),
                ("body.text".into(), "post.text".into()),
            ]),
            edge_remap: HashMap::from([(
                edge_text,
                Edge {
                    src: "post".into(),
                    tgt: "post.text".into(),
                    kind: "prop".into(),
                    name: Some("text".into()),
                },
            )]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // Build instance on s1. Σ (left Kan extension) is total: it cannot
        // extend a node whose anchor a migration drops, so the instance
        // contains only nodes that survive every migration exercised below
        // (m1 drops createdAt, so no createdAt node is present here). This
        // keeps the test on the functoriality of Σ over renames, which is
        // where Σ — as opposed to the projection Δ — is the right operation.
        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into())))
                .with_extra_field("$lang", Value::Str("en".into())),
        );
        let arcs = vec![(
            0,
            1,
            Edge {
                src: "body".into(),
                tgt: "body.text".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            },
        )];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("body"));

        // Sequential: lift_sigma(m2, lift_sigma(m1, I))
        let step1 = lift_wtype_sigma(&m1, &s2, &instance).unwrap();
        let sequential = lift_wtype_sigma(&m2, &s3, &step1).unwrap();

        // Direct: lift_sigma(m12, I)
        let direct = lift_wtype_sigma(&m12, &s3, &instance).unwrap();

        // Σ preserves node ids, so the identity node map is the candidate
        // isomorphism between the sequential and direct results. Asserting it
        // is an isomorphism checks that anchors, arcs, fans, and the root all
        // agree — far stronger than a node-count match.
        let node_map: HashMap<u32, u32> = sequential.nodes.keys().map(|&id| (id, id)).collect();
        let hom = WInstanceHom::new(node_map);
        assert!(
            hom.is_isomorphism(&sequential, &direct),
            "Σ functoriality: sequential and direct results must be isomorphic"
        );

        // The isomorphism already enforces attribute preservation; assert the
        // value and extra-field agreement explicitly as a regression guard.
        for (&id, node) in &sequential.nodes {
            let image = direct.nodes.get(&id).unwrap();
            assert_eq!(node.value, image.value, "node {id} value must agree");
            assert_eq!(
                node.extra_fields, image.extra_fields,
                "node {id} extra fields must agree"
            );
        }

        // Flipping one anchor in the direct result must break the isomorphism,
        // confirming the check is sensitive to anchor preservation.
        let mut tampered = direct;
        if let Some(root) = tampered.nodes.get_mut(&0) {
            root.anchor = panproto_gat::Name::from("tampered");
        }
        assert!(
            !hom.is_isomorphism(&sequential, &tampered),
            "flipping an anchor must break the Σ-functoriality isomorphism"
        );
    }

    /// Test that Σ on identity migration preserves the instance.
    #[test]
    fn sigma_identity_preserves_instance() {
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
        nodes.insert(0, Node::new(0, "body"));
        nodes.insert(
            1,
            Node::new(1, "body.text")
                .with_value(FieldPresence::Present(Value::Str("hello".into()))),
        );
        let arcs = vec![(0, 1, edge_text.clone())];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("body"));

        let id_migration = CompiledMigration {
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

        let result = lift_wtype_sigma(&id_migration, &schema, &instance).unwrap();
        assert_eq!(
            result.node_count(),
            instance.node_count(),
            "Σ on identity should preserve node count"
        );
    }

    #[test]
    fn recursive_projection_via_wtype_restrict() {
        // Test 3: Recursive schema with nested children. Drop intermediate
        // vertices and verify the reachability filter prunes unreachable
        // subtrees via wtype_restrict.
        //
        // Schema: root -> container -> leaf1
        //                           -> leaf2
        //         root -> leaf3
        //
        // Migration drops "container", so leaf1 and leaf2 become
        // unreachable (no surviving ancestor path from root).
        let edge_root_container = Edge {
            src: "root".into(),
            tgt: "container".into(),
            kind: "prop".into(),
            name: Some("items".into()),
        };
        let edge_container_leaf1 = Edge {
            src: "container".into(),
            tgt: "leaf1".into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };
        let edge_container_leaf2 = Edge {
            src: "container".into(),
            tgt: "leaf2".into(),
            kind: "prop".into(),
            name: Some("b".into()),
        };
        let edge_root_leaf3 = Edge {
            src: "root".into(),
            tgt: "leaf3".into(),
            kind: "prop".into(),
            name: Some("direct".into()),
        };

        let schema = test_schema(
            &[("root", "object"), ("leaf3", "string")],
            std::slice::from_ref(&edge_root_leaf3),
        );

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "container"));
        nodes.insert(
            2,
            Node::new(2, "leaf1").with_value(FieldPresence::Present(Value::Str("val1".into()))),
        );
        nodes.insert(
            3,
            Node::new(3, "leaf2").with_value(FieldPresence::Present(Value::Str("val2".into()))),
        );
        nodes.insert(
            4,
            Node::new(4, "leaf3").with_value(FieldPresence::Present(Value::Str("val3".into()))),
        );

        let arcs = vec![
            (0, 1, edge_root_container),
            (1, 2, edge_container_leaf1),
            (1, 3, edge_container_leaf2),
            (0, 4, edge_root_leaf3.clone()),
        ];
        let instance = WInstance::new(nodes, arcs, vec![], 0, panproto_gat::Name::from("root"));

        // Migration: drop "container", keep root and leaf3 only.
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["root".into(), "leaf3".into()]),
            surviving_edges: HashSet::from([edge_root_leaf3]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = lift_wtype(&compiled, &schema, &schema, &instance);
        assert!(result.is_ok(), "recursive projection should succeed");
        let lifted = result.unwrap_or_else(|_| panic!("lift should succeed"));

        // Only root (0) and leaf3 (4) should survive; container (1),
        // leaf1 (2), and leaf2 (3) are all pruned.
        assert_eq!(
            lifted.node_count(),
            2,
            "should have 2 surviving nodes (root + leaf3)"
        );
        assert!(lifted.nodes.contains_key(&0), "root should survive");
        assert!(lifted.nodes.contains_key(&4), "leaf3 should survive");
        assert!(!lifted.nodes.contains_key(&1), "container should be pruned");
        assert!(
            !lifted.nodes.contains_key(&2),
            "leaf1 should be pruned (unreachable)"
        );
        assert!(
            !lifted.nodes.contains_key(&3),
            "leaf2 should be pruned (unreachable)"
        );
    }

    /// A `Sigma` lift whose chase hits an equality conflict must report
    /// the chase's own error, not a sentence about it, so a caller can
    /// tell a conflict from a budget that ran out.
    #[test]
    fn a_chase_conflict_and_a_spent_budget_are_told_apart() {
        use crate::chase::{Atom, AtomTerm, ChaseBudget, ChaseError, Dependency};
        use panproto_inst::FInstance;

        let compiled = CompiledMigration::default();
        let instance = FInstance::new().with_table(
            "t",
            vec![HashMap::from([("a".to_owned(), Value::Str("x".into()))])],
        );

        // An EGD equating the column's constant with a different
        // constant can never hold, at any budget.
        let conflicting = Dependency::Egd {
            body: vec![Atom::new("t", [("a", AtomTerm::Var("v".into()))])],
            left: AtomTerm::Var("v".into()),
            right: AtomTerm::Const(Value::Str("z".into())),
        };
        let Err(err) = lift_functor_sigma(
            &compiled,
            &instance,
            &[conflicting],
            ChaseBudget::new(50, 50),
        ) else {
            panic!("an unsatisfiable equality must fail the lift");
        };
        assert!(
            matches!(
                &err,
                LiftError::Chase(ChaseError::Inconsistent { left, right })
                    if left.contains('x') && right.contains('z')
            ),
            "the chase's own error must survive the lift boundary, got {err:?}",
        );
        assert!(
            !err.is_retryable(),
            "an equality conflict cannot be retried away",
        );

        // A dependency that invents a fresh null every round exhausts
        // the budget instead, which a bigger budget could survive.
        let regenerating = Dependency::Tgd {
            body: vec![Atom::new("t", [("a", AtomTerm::Var("v".into()))])],
            head: vec![Atom::new(
                "t",
                [
                    ("prev", AtomTerm::Var("v".into())),
                    ("a", AtomTerm::Var("w".into())),
                ],
            )],
        };
        let Err(err) = lift_functor_sigma(
            &compiled,
            &instance,
            &[regenerating],
            ChaseBudget::new(5, 3),
        ) else {
            panic!("a chase that cannot converge must fail the lift");
        };
        assert!(
            matches!(
                err,
                LiftError::ChaseBudgetExhausted {
                    max_iterations: 5,
                    max_nulls: 3
                }
            ),
            "the spent budget must be named, got {err:?}",
        );
        assert!(err.is_retryable(), "a spent budget invites a retry");
    }
}
