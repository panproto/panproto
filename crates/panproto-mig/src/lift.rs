//! Lift operations: applying compiled migrations to instances.
//!
//! `lift_wtype`, `lift_graph` and `lift_functor` delegate to the
//! schema-taking restricts of `panproto-inst`, which validate both
//! boundaries under the protocols they are given; the `Sigma` lifts
//! validate their output against the target schema under the target
//! protocol.

use panproto_inst::{
    BoundaryProtocols, BoundarySide, CompiledMigration, FInstance, FInstanceHom, GInstance,
    PiProvenance, RestrictError, WInstance,
};
use panproto_schema::{Protocol, Schema};

use crate::error::LiftError;

/// Apply a compiled migration to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_restrict`], which validates the
/// instance against `src_schema` under `protocols.src`, compiles the plan of
/// the migration, runs the tree engine on it and validates the result
/// against `tgt_schema` under `protocols.tgt`.
///
/// # Errors
///
/// Returns `LiftError::Restrict` carrying
/// [`RestrictError::Validation`] when a boundary does not validate (the
/// input before anything runs, the output after the engine), or the error
/// of the plan or the engine (e.g., edge resolution ambiguity, root
/// pruned).
pub fn lift_wtype(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &WInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<WInstance, LiftError> {
    Ok(panproto_inst::wtype_restrict(
        instance, src_schema, tgt_schema, compiled, protocols,
    )?)
}

/// Apply a compiled migration to a graph instance.
///
/// The graph twin of [`lift_wtype`]: delegates to
/// [`panproto_inst::graph_restrict()`], which validates the instance against
/// `src_schema` under `protocols.src`, runs the graph engine on the
/// migration's plan and validates the result against `tgt_schema` under
/// `protocols.tgt`. On a tree-shaped graph whose lens preserves tree shape,
/// whose root survives and which drops no edge between surviving vertices,
/// the result is the graph of [`lift_wtype`]'s result on the tree.
///
/// # Errors
///
/// As [`lift_wtype`], with the graph engine's errors.
pub fn lift_graph(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &GInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<GInstance, LiftError> {
    Ok(panproto_inst::graph_restrict(
        instance, src_schema, tgt_schema, compiled, protocols,
    )?)
}

/// Apply a compiled migration to a set-valued functor instance.
///
/// Delegates to [`panproto_inst::functor_restrict`], which carries the
/// instance forward along the migration and keeps the fragment that
/// survives, reading tables and columns through the two protocols and
/// validating both boundaries.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying restrict operation fails.
pub fn lift_functor(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &FInstance,
    protocols: panproto_inst::BoundaryProtocols<'_>,
) -> Result<FInstance, LiftError> {
    let result =
        panproto_inst::functor_restrict(instance, src_schema, tgt_schema, compiled, protocols)?;
    Ok(result)
}

/// Apply a compiled migration as a left Kan extension (`Sigma_F`) to a W-type instance.
///
/// Delegates to [`panproto_inst::wtype_extend`], which pushes every node
/// forward along the migration morphism, remapping anchors and edges, and
/// validates the result against `tgt_schema` under `tgt_protocol`. This
/// is a *total* operation: it requires each source node's anchor to be
/// remapped or surviving, and reports an unmapped anchor rather than
/// dropping the node silently.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the underlying extend operation fails,
/// including when a source node's anchor is neither remapped nor surviving
/// (`RestrictError::UnmappedAnchor`), and carrying
/// [`RestrictError::Validation`] on the output side when the result does
/// not validate.
pub fn lift_wtype_sigma(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &WInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<WInstance, LiftError> {
    Ok(panproto_inst::wtype_extend(
        instance, src_schema, tgt_schema, compiled, protocols,
    )?)
}

/// Apply a compiled migration as relational graph `Sigma_F`.
///
/// This is the graph-Kan lift. It transports through the relational graph
/// encoding and therefore has the exact domain documented by
/// [`panproto_inst::g_sigma`]. It agrees with [`lift_wtype_sigma`] on trees
/// whose schema image is bijective and relationally encodable.
///
/// # Errors
///
/// Returns [`LiftError::GraphKan`] for a classification, encoding, table
/// Sigma, or decoding refusal.
pub fn lift_graph_sigma(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &GInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<GInstance, LiftError> {
    Ok(panproto_inst::g_sigma(
        instance,
        src_schema,
        protocols.src,
        tgt_schema,
        protocols.tgt,
        compiled,
    )?)
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

/// Apply a compiled migration to a functor instance along the total
/// extension path, then close it under a term-level chase.
///
/// This is the `sigma` pipeline entry for set-valued instances: it runs
/// [`panproto_inst::functor_extend_engine`], the total table `Sigma_F`, and
/// then saturates the result under `dependencies` with
/// [`crate::chase::chase`], enforcing tuple- and equality-generating
/// dependencies. Pass an empty `dependencies` slice to run the extension
/// alone. A target column outside the image receives a fresh labelled null,
/// as in [`panproto_inst::f_sigma`].
///
/// The result after the chase is validated against `tgt_schema` under
/// `tgt_protocol`. The extension alone is never validated: the nulls it pads
/// with are what the chase repairs.
///
/// # Errors
///
/// Returns `LiftError::Restrict` if the extension fails,
/// `LiftError::Chase` carrying the chase's own error if the chase
/// reports an equality conflict or a saturation that did not converge,
/// or `LiftError::ChaseBudgetExhausted` naming the budget when the
/// term-level chase runs out of it. `LiftError::is_retryable` tells the
/// budget failures apart from the conflict. A chased result that does not
/// validate is `LiftError::Restrict` carrying
/// [`RestrictError::Validation`] on the output side.
pub fn lift_functor_sigma(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &FInstance,
    dependencies: &[crate::chase::Dependency],
    budget: crate::chase::ChaseBudget,
    protocols: BoundaryProtocols<'_>,
) -> Result<FInstance, LiftError> {
    lift_functor_sigma_with_hom(
        compiled,
        src_schema,
        tgt_schema,
        instance,
        dependencies,
        budget,
        protocols,
    )
    .map(|(result, _)| result)
}

/// [`lift_functor_sigma`], with the row map of the chase that creates rows.
///
/// The extension ([`panproto_inst::functor_extend_engine`]) keeps every row's id and
/// creates no row; the chase then adds rows and keeps every extended row.
/// The returned [`FInstanceHom`] is the chase's homomorphism
/// ([`crate::chase::chase_with_hom`]) from the extension of `instance`
/// into the result: its row map sends every row, which is a row of
/// `instance` under the same id, to itself, its null map records what the
/// equalities forced on each labeled null, and the result's rows outside
/// its image are the ones the chase created, in firing order. With no
/// dependencies the map is the identity on the extension. The result is
/// validated after the chase, as [`lift_functor_sigma`] states.
///
/// # Errors
///
/// As [`lift_functor_sigma`].
pub fn lift_functor_sigma_with_hom(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &FInstance,
    dependencies: &[crate::chase::Dependency],
    budget: crate::chase::ChaseBudget,
    protocols: BoundaryProtocols<'_>,
) -> Result<(FInstance, FInstanceHom), LiftError> {
    let extended = panproto_inst::functor_extend_engine(
        instance, src_schema, tgt_schema, compiled, protocols,
    )?;
    let (result, hom) = if dependencies.is_empty() {
        let identity = FInstanceHom::identity(&extended);
        (extended, identity)
    } else {
        match crate::chase::chase_with_hom(&extended, dependencies, budget)? {
            (crate::chase::ChaseOutcome::Saturated(result), hom) => (result, hom),
            (crate::chase::ChaseOutcome::NonTermination, _) => {
                return Err(LiftError::ChaseBudgetExhausted {
                    max_iterations: budget.max_iterations,
                    max_nulls: budget.max_nulls,
                });
            }
        }
    };
    let report = panproto_inst::validate_table(&result, tgt_schema, protocols.tgt);
    if report.status != panproto_gat::CheckStatus::Passed {
        return Err(LiftError::Restrict(RestrictError::Validation {
            side: BoundarySide::Output,
            report: Box::new(report),
        }));
    }
    Ok((result, hom))
}

/// Apply a compiled migration as relational graph `Pi_F`.
///
/// It agrees with [`lift_wtype_pi`] on trees whose schema image is bijective
/// and relationally encodable.
///
/// # Errors
///
/// Returns [`LiftError::GraphKan`] for a classification, encoding, table
/// Pi, or decoding refusal.
pub fn lift_graph_pi(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    tgt_schema: &Schema,
    instance: &GInstance,
    protocols: BoundaryProtocols<'_>,
    max_product_size: usize,
) -> Result<GInstance, LiftError> {
    Ok(panproto_inst::g_pi(
        instance,
        src_schema,
        protocols.src,
        tgt_schema,
        protocols.tgt,
        compiled,
        max_product_size,
    )?)
}

/// Apply a compiled migration as a right Kan extension (`Pi_F`) to a functor instance.
///
/// The first projection of [`lift_functor_pi_with_provenance`]; see there.
///
/// # Errors
///
/// As [`lift_functor_pi_with_provenance`].
pub fn lift_functor_pi(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    instance: &FInstance,
    max_product_size: usize,
) -> Result<FInstance, LiftError> {
    lift_functor_pi_with_provenance(
        compiled,
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        instance,
        max_product_size,
    )
    .map(|(result, _)| result)
}

/// [`lift_functor_pi`], with the tuple of source rows behind each row it
/// returns.
///
/// Builds the [`MigrationFunctor`](panproto_inst::MigrationFunctor) the
/// migration states between the schemas, each classified under its
/// protocol, and delegates to
/// [`panproto_inst::functor_pi_along_with_provenance`]: at each target table
/// the natural join of its fibre's tables on the columns they share there
/// (one row without cells at a table outside the image), and at each target
/// foreign key the pairs of those rows that every foreign key carried to it
/// relates (every pair when none is).
///
/// # Errors
///
/// Returns [`LiftError::Adjunction`] when the functor cannot be built (a
/// schema with no relation-ified form under its protocol, a kept table sent
/// to a vertex that is no table of `tgt_schema`, or a carried foreign key or
/// column whose image does not fit), when the functor is not surjective on
/// attributes, when the instance does not conform to `src_schema`, and when
/// the product bound is exceeded.
pub fn lift_functor_pi_with_provenance(
    compiled: &CompiledMigration,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    instance: &FInstance,
    max_product_size: usize,
) -> Result<(FInstance, PiProvenance), LiftError> {
    let functor = panproto_inst::MigrationFunctor::new(
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        compiled,
    )
    .map_err(panproto_inst::AdjunctionError::Pi)?;
    Ok(panproto_inst::functor_pi_along_with_provenance(
        instance,
        &functor,
        max_product_size,
    )?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

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
            ..Default::default()
        }
    }

    /// The protocol of the test schemas: no value kinds, so every value is
    /// of any kind.
    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            obj_kinds: vec!["object".into(), "string".into()],
            ..Protocol::default()
        }
    }

    /// The same protocol at both boundaries.
    const fn both(protocol: &Protocol) -> BoundaryProtocols<'_> {
        BoundaryProtocols {
            src: protocol,
            tgt: protocol,
        }
    }

    /// The protocol of the table schemas: tables of kind `table`, columns
    /// of kind `string`.
    fn table_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            value_kinds: BTreeMap::from([("string".to_owned(), panproto_schema::ScalarKind::Str)]),
            non_value_kinds: std::collections::BTreeSet::from(["table".to_owned()]),
            ..Protocol::default()
        }
    }

    /// A schema of tables, each with its string columns: a vertex `t` of
    /// kind `table` per table and a vertex `t.c` of kind `string` reached by
    /// the edge `t -c-> t.c` per column `c`.
    fn table_schema(tables: &[(&str, &[&str])]) -> Schema {
        let mut vertices: Vec<(String, &str)> = Vec::new();
        let mut edges = Vec::new();
        for (table, columns) in tables {
            vertices.push(((*table).to_owned(), "table"));
            for column in *columns {
                let id = format!("{table}.{column}");
                edges.push(Edge {
                    src: (*table).into(),
                    tgt: id.as_str().into(),
                    kind: "prop".into(),
                    name: Some((*column).into()),
                });
                vertices.push((id, "string"));
            }
        }
        let vertices: Vec<(&str, &str)> = vertices.iter().map(|(v, k)| (v.as_str(), *k)).collect();
        test_schema(&vertices, &edges)
    }

    fn map_column(
        compiled: &mut CompiledMigration,
        source_table: &str,
        source_column: &str,
        target_table: &str,
        target_column: &str,
    ) {
        let source_vertex: panproto_gat::Name = format!("{source_table}.{source_column}").into();
        let target_vertex: panproto_gat::Name = format!("{target_table}.{target_column}").into();
        let source_edge = Edge {
            src: source_table.into(),
            tgt: source_vertex.clone(),
            kind: "prop".into(),
            name: Some(source_column.into()),
        };
        let target_edge = Edge {
            src: target_table.into(),
            tgt: target_vertex.clone(),
            kind: "prop".into(),
            name: Some(target_column.into()),
        };
        compiled.surviving_verts.insert(target_vertex.clone());
        compiled.surviving_edges.insert(target_edge.clone());
        compiled.vertex_remap.insert(source_vertex, target_vertex);
        compiled.edge_remap.insert(source_edge, target_edge);
    }

    #[test]
    fn graph_sigma_and_pi_lifts_match_tree_lifts_on_trees() {
        let protocol = table_protocol();
        let schema = table_schema(&[("t", &[])]);
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["t".into()]),
            ..CompiledMigration::default()
        };
        let tree = WInstance::new(
            HashMap::from([(7, Node::new(7, "t"))]),
            vec![],
            vec![],
            7,
            "t".into(),
        );
        let (graph, _) = GInstance::from_wtype_embedded(&tree, None).unwrap();

        let tree_sigma =
            lift_wtype_sigma(&compiled, &schema, &schema, &tree, both(&protocol)).unwrap();
        let graph_sigma =
            lift_graph_sigma(&compiled, &schema, &schema, &graph, both(&protocol)).unwrap();
        let (tree_sigma_graph, _) = GInstance::from_wtype_embedded(&tree_sigma, None).unwrap();
        assert!(GInstance::identical(&tree_sigma_graph, &graph_sigma));

        let tree_pi = lift_wtype_pi(&compiled, &schema, &tree, 8).unwrap();
        let graph_pi =
            lift_graph_pi(&compiled, &schema, &schema, &graph, both(&protocol), 8).unwrap();
        let (tree_pi_graph, _) = GInstance::from_wtype_embedded(&tree_pi, None).unwrap();
        assert!(GInstance::identical(&tree_pi_graph, &graph_pi));
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

        let result = lift_wtype(
            &compiled,
            &schema,
            &schema,
            &instance,
            both(&test_protocol()),
        );
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

        // The instance lives in the source schema, which also has the
        // `createdAt` field the target drops.
        let src = test_schema(
            &[
                ("body", "object"),
                ("body.text", "string"),
                ("body.createdAt", "string"),
            ],
            &[edge_text.clone(), edge_time.clone()],
        );
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

        let result = lift_wtype(&compiled, &src, &schema, &instance, both(&test_protocol()));
        assert!(result.is_ok(), "projection lift should succeed: {result:?}");
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
        let s1 = test_schema(
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
        let protocol = test_protocol();
        let step1 = lift_wtype_sigma(&m1, &s1, &s2, &instance, both(&protocol)).unwrap();
        let sequential = lift_wtype_sigma(&m2, &s2, &s3, &step1, both(&protocol)).unwrap();

        // Direct: lift_sigma(m12, I)
        let direct = lift_wtype_sigma(&m12, &s1, &s3, &instance, both(&protocol)).unwrap();

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

        let protocol = test_protocol();
        let result =
            lift_wtype_sigma(&id_migration, &schema, &schema, &instance, both(&protocol)).unwrap();
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

        // The instance lives in the source schema; the target keeps only
        // the root and its direct leaf.
        let src = test_schema(
            &[
                ("root", "object"),
                ("container", "object"),
                ("leaf1", "string"),
                ("leaf2", "string"),
                ("leaf3", "string"),
            ],
            &[
                edge_root_container.clone(),
                edge_container_leaf1.clone(),
                edge_container_leaf2.clone(),
                edge_root_leaf3.clone(),
            ],
        );
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

        let result = lift_wtype(&compiled, &src, &schema, &instance, both(&test_protocol()));
        assert!(
            result.is_ok(),
            "recursive projection should succeed: {result:?}"
        );
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
        let instance = FInstance::new()
            .with_table(
                "t",
                vec![HashMap::from([("a".to_owned(), Value::Str("x".into()))])],
            )
            .unwrap();

        // An EGD equating the column's constant with a different
        // constant can never hold, at any budget.
        let conflicting = Dependency::Egd {
            body: vec![Atom::new("t", [("a", AtomTerm::Var("v".into()))])],
            left: AtomTerm::Var("v".into()),
            right: AtomTerm::Const(Value::Str("z".into())),
        };
        let schema = table_schema(&[("t", &["a"])]);
        let protocol = table_protocol();
        let Err(err) = lift_functor_sigma(
            &compiled,
            &schema,
            &schema,
            &instance,
            &[conflicting],
            ChaseBudget::new(50, 50),
            both(&protocol),
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
        let target = table_schema(&[("t", &["a", "prev"])]);
        let Err(err) = lift_functor_sigma(
            &compiled,
            &schema,
            &target,
            &instance,
            &[regenerating],
            ChaseBudget::new(5, 3),
            both(&protocol),
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

    /// Merging two tables into one fills each row's missing image column
    /// with a fresh labelled null, keeps every row id, and creates no row.
    /// A lift with no chase then refuses the unresolved nulls at validation.
    #[test]
    fn sigma_engine_fills_a_merged_table_and_the_lift_requires_a_chase() {
        use crate::chase::ChaseBudget;
        use panproto_inst::{FInstance, RowId};

        let cell =
            |column: &str, v: &str| HashMap::from([(column.to_owned(), Value::Str(v.into()))]);
        let instance = FInstance::new()
            .with_table("t1", vec![cell("a", "x")])
            .unwrap()
            .with_table("t2", vec![cell("b", "y")])
            .unwrap();
        let mut compiled = CompiledMigration::default();
        compiled.surviving_verts.insert("u".into());
        compiled.vertex_remap.insert("t1".into(), "u".into());
        compiled.vertex_remap.insert("t2".into(), "u".into());
        map_column(&mut compiled, "t1", "a", "u", "a");
        map_column(&mut compiled, "t2", "b", "u", "b");

        let src = table_schema(&[("t1", &["a"]), ("t2", &["b"])]);
        let tgt = table_schema(&[("u", &["a", "b"])]);
        let protocol = table_protocol();
        let result =
            panproto_inst::functor_extend_engine(&instance, &src, &tgt, &compiled, both(&protocol))
                .unwrap();
        assert_eq!(result.row_ids(), vec![RowId(0), RowId(1)]);
        for id in [RowId(0), RowId(1)] {
            assert_eq!(result.table_of(id).map(AsRef::as_ref), Some("u"));
        }
        let row = |id| result.row(id).cloned().unwrap();
        assert_eq!(
            row(RowId(0)),
            HashMap::from([
                ("a".to_owned(), Value::Str("x".into())),
                ("b".to_owned(), Value::LabeledNull(0)),
            ])
        );
        assert_eq!(
            row(RowId(1)),
            HashMap::from([
                ("a".to_owned(), Value::LabeledNull(1)),
                ("b".to_owned(), Value::Str("y".into())),
            ])
        );
        assert!(matches!(
            lift_functor_sigma(
                &compiled,
                &src,
                &tgt,
                &instance,
                &[],
                ChaseBudget::new(10, 10),
                both(&protocol),
            ),
            Err(LiftError::Restrict(RestrictError::Validation {
                side: BoundarySide::Output,
                ..
            }))
        ));
    }

    /// Every lift that creates rows returns the row map of the step that
    /// creates them. The `sigma` lift's chase adds rows to the extension
    /// and keeps every extended row's id, so its map sends each row of
    /// the extension of `I` to itself, and the rows outside its image are the ones
    /// the chase created, in firing order. The `Pi` lift returns the
    /// projection of each product row onto each fibre component.
    #[test]
    fn row_creating_lifts_return_their_row_maps() {
        use crate::chase::{Atom, AtomTerm, ChaseBudget, Dependency};
        use panproto_inst::{FInstance, RowId};

        let named = |v: &str| HashMap::from([("a".to_owned(), Value::Str(v.into()))]);
        let instance = FInstance::new()
            .with_table("t", vec![named("x"), named("y")])
            .unwrap();
        let mut compiled = CompiledMigration::default();
        compiled.surviving_verts.insert("u".into());
        compiled.vertex_remap.insert("t".into(), "u".into());
        map_column(&mut compiled, "t", "a", "u", "a");
        let copy = Dependency::Tgd {
            body: vec![Atom::new("u", [("a", AtomTerm::Var("v".into()))])],
            head: vec![Atom::new("w", [("b", AtomTerm::Var("v".into()))])],
        };
        let budget = ChaseBudget::new(10, 10);
        let src = table_schema(&[("t", &["a"])]);
        let tgt = table_schema(&[("u", &["a"]), ("w", &["b"])]);
        let protocol = table_protocol();

        let extended =
            panproto_inst::functor_extend_engine(&instance, &src, &tgt, &compiled, both(&protocol))
                .unwrap();
        let (result, hom) = lift_functor_sigma_with_hom(
            &compiled,
            &src,
            &tgt,
            &instance,
            std::slice::from_ref(&copy),
            budget,
            both(&protocol),
        )
        .unwrap();
        assert_eq!(
            result,
            lift_functor_sigma(
                &compiled,
                &src,
                &tgt,
                &instance,
                &[copy],
                budget,
                both(&protocol),
            )
            .unwrap()
        );
        let checked = hom.check(&extended, &result);
        assert!(
            checked.is_ok(),
            "the chase map is a homomorphism out of the extension: {checked:?}"
        );
        assert_eq!(
            hom.row_map,
            [(RowId(0), RowId(0)), (RowId(1), RowId(1))].into()
        );
        let created: Vec<RowId> = result
            .row_ids()
            .into_iter()
            .filter(|id| !hom.row_map.values().any(|image| image == id))
            .collect();
        assert_eq!(created, vec![RowId(2), RowId(3)]);
        assert_eq!(result.table_of(RowId(2)).map(AsRef::as_ref), Some("w"));
        assert_eq!(
            result.row(RowId(2)).and_then(|r| r.get("b")),
            Some(&Value::Str("x".into()))
        );
        assert_eq!(
            result.row(RowId(3)).and_then(|r| r.get("b")),
            Some(&Value::Str("y".into()))
        );

        let (alone, identity) = lift_functor_sigma_with_hom(
            &compiled,
            &src,
            &tgt,
            &instance,
            &[],
            budget,
            both(&protocol),
        )
        .unwrap();
        assert_eq!(alone, extended);
        assert_eq!(identity, panproto_inst::FInstanceHom::identity(&extended));

        pi_lift_joins_on_the_shared_column(&named);
    }

    /// The Pi part of [`row_creating_lifts_return_their_row_maps`].
    fn pi_lift_joins_on_the_shared_column(named: &dyn Fn(&str) -> HashMap<String, Value>) {
        use panproto_inst::{FInstance, RowId};

        // Pi over a fibre of two tables creates product rows: one for each
        // pair of rows that agree on their shared column `a`.
        let factors = FInstance::new()
            .with_table("a", vec![named("p"), named("p"), named("q")])
            .and_then(|f| f.with_table("b", vec![named("p")]))
            .unwrap();
        let column = |table: &str| Edge {
            src: table.into(),
            tgt: format!("{table}.a").into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };
        let mut product = CompiledMigration::default();
        product.surviving_verts.insert("t".into());
        for table in ["a", "b"] {
            product.vertex_remap.insert(table.into(), "t".into());
            product
                .vertex_remap
                .insert(format!("{table}.a").into(), "t.a".into());
            product.edge_remap.insert(column(table), column("t"));
        }
        let src = test_schema(
            &[
                ("a", "table"),
                ("b", "table"),
                ("a.a", "string"),
                ("b.a", "string"),
            ],
            &[column("a"), column("b")],
        );
        let tgt = test_schema(&[("t", "table"), ("t.a", "string")], &[column("t")]);
        let protocol = Protocol {
            value_kinds: BTreeMap::from([("string".to_owned(), panproto_schema::ScalarKind::Str)]),
            non_value_kinds: std::collections::BTreeSet::from(["table".to_owned()]),
            ..Protocol::default()
        };
        let (pi, provenance) = lift_functor_pi_with_provenance(
            &product, &src, &protocol, &tgt, &protocol, &factors, 16,
        )
        .unwrap();
        assert_eq!(
            pi,
            lift_functor_pi(&product, &src, &protocol, &tgt, &protocol, &factors, 16).unwrap()
        );
        assert_eq!(pi.row_ids(), vec![RowId(4), RowId(5)]);
        assert_eq!(
            provenance.tuples,
            BTreeMap::from([
                (RowId(4), ("t".into(), vec![RowId(0), RowId(3)])),
                (RowId(5), ("t".into(), vec![RowId(1), RowId(3)])),
            ])
        );
        for id in pi.row_ids() {
            assert_eq!(pi.row(id), Some(&named("p")), "the joined row holds a = p");
        }
    }

    /// On a tree in the domain where the graph engine agrees with the tree
    /// engine (the lens keeps tree shape, the root survives, and no edge
    /// between surviving vertices is dropped), the graph lift of the tree's
    /// graph is the graph of the tree lift: the same nodes, and one edge per
    /// arc with the arc's endpoints and label.
    #[test]
    fn lift_graph_matches_lift_wtype_on_a_tree() {
        let e = |src: &str, tgt: &str, name: &str| Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        };
        let (text, time, title) = (
            e("body", "body.text", "text"),
            e("body", "body.time", "time"),
            e("post", "body", "body"),
        );
        let src = test_schema(
            &[
                ("post", "object"),
                ("body", "object"),
                ("body.text", "string"),
                ("body.time", "string"),
            ],
            &[title.clone(), text.clone(), time.clone()],
        );
        // The target renames the post to an article and drops the time leaf.
        let article_body = e("article", "body", "body");
        let tgt = test_schema(
            &[
                ("article", "object"),
                ("body", "object"),
                ("body.text", "string"),
            ],
            &[article_body.clone(), text.clone()],
        );
        let compiled = CompiledMigration {
            surviving_verts: HashSet::from(["article".into(), "body".into(), "body.text".into()]),
            surviving_edges: HashSet::from([article_body.clone(), text.clone()]),
            vertex_remap: HashMap::from([("post".into(), "article".into())]),
            edge_remap: HashMap::from([(title.clone(), article_body)]),
            ..CompiledMigration::default()
        };
        let present = |v: &str| FieldPresence::Present(Value::Str(v.into()));
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "post")),
                (1, Node::new(1, "body")),
                (2, Node::new(2, "body.text").with_value(present("hi"))),
                (3, Node::new(3, "body.time").with_value(present("noon"))),
            ]),
            vec![(0, 1, title), (1, 2, text), (1, 3, time)],
            vec![],
            0,
            "post".into(),
        );
        let protocol = test_protocol();
        let lifted_tree = lift_wtype(&compiled, &src, &tgt, &tree, both(&protocol)).unwrap();
        let graph = GInstance::from_wtype(&tree).unwrap();
        let lifted_graph = lift_graph(&compiled, &src, &tgt, &graph, both(&protocol)).unwrap();

        // A node is compared on its anchor, its value and its fields.
        let shown = |n: &Node| {
            let mut fields: Vec<String> = n
                .extra_fields
                .iter()
                .map(|(k, v)| format!("{k}={v:?}"))
                .collect();
            fields.sort();
            (n.anchor.clone(), format!("{:?}", n.value), fields)
        };
        let tree_nodes: std::collections::BTreeMap<u32, _> = lifted_tree
            .nodes
            .iter()
            .map(|(id, n)| (*id, shown(n)))
            .collect();
        let graph_nodes: std::collections::BTreeMap<u32, _> = lifted_graph
            .nodes()
            .iter()
            .map(|(id, n)| (*id, shown(n)))
            .collect();
        assert_eq!(tree_nodes, graph_nodes);
        assert_eq!(tree_nodes.len(), 3, "the time leaf is dropped");
        let mut arcs: Vec<(u32, u32, Edge)> = lifted_tree.arcs;
        arcs.sort();
        let mut edges: Vec<(u32, u32, Edge)> = lifted_graph
            .edges_sorted()
            .map(|(_, edge)| (edge.src, edge.tgt, edge.edge.clone()))
            .collect();
        edges.sort();
        assert_eq!(arcs, edges);
        assert_eq!(&*lifted_graph.nodes()[&0].anchor, "article");
    }

    /// The table schema of the reference tests: a table `u` whose `tag`
    /// column names a row of the table `w` by its `name` key, unresolved
    /// references refused.
    fn referencing_schema() -> Schema {
        use panproto_schema::{
            Cardinality, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, RefSource, Reference,
            ResolveMode,
        };
        let mut schema = table_schema(&[("u", &["tag"]), ("w", &["name"])]);
        let column = |table: &str, name: &str| Edge {
            src: table.into(),
            tgt: format!("{table}.{name}").into(),
            kind: "prop".into(),
            name: Some(name.into()),
        };
        schema.declarations.insert_key(
            "w".into(),
            Key {
                id: "pk".into(),
                parts: vec![KeyPart::Path(vec![column("w", "name")])],
                role: KeyRole::Primary,
                scope: KeyScope::Global,
            },
        );
        schema.declarations.insert_reference(Reference {
            id: "names".into(),
            holder: "u".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![column("u", "tag")]],
            },
            target_vertex: "w".into(),
            target_key: Some("pk".into()),
            edge: None,
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
        });
        schema
    }

    /// The protocol of the reference tests: the table protocol, whose
    /// instances are tables in both syntax and meaning.
    fn referencing_protocol() -> Protocol {
        Protocol {
            syntactic_shape: panproto_schema::InstanceShapeKind::Table,
            semantic_shape: panproto_schema::InstanceShapeKind::Table,
            ..table_protocol()
        }
    }

    /// A table `t` with one row whose `tag` is `x`, and the migration that
    /// renames it to the referencing table `u` of [`referencing_schema`].
    fn tagged_rows() -> (FInstance, CompiledMigration) {
        let instance = FInstance::new()
            .with_table(
                "t",
                vec![HashMap::from([("tag".to_owned(), Value::Str("x".into()))])],
            )
            .unwrap();
        let mut compiled = CompiledMigration::default();
        compiled.surviving_verts.insert("u".into());
        compiled.surviving_verts.insert("w".into());
        compiled.vertex_remap.insert("t".into(), "u".into());
        map_column(&mut compiled, "t", "tag", "u", "tag");
        (instance, compiled)
    }

    /// The functor `Sigma` lift validates what its chase returns, never the
    /// padded extension: here the extension names a `w` row no table holds,
    /// so it does not validate, and the chase adds the row it names, so the
    /// lift is accepted.
    #[test]
    fn the_functor_sigma_lift_validates_after_the_chase() {
        use crate::chase::{Atom, AtomTerm, ChaseBudget, Dependency};

        let (instance, compiled) = tagged_rows();
        let src = table_schema(&[("t", &["tag"])]);
        let (tgt, protocol) = (referencing_schema(), referencing_protocol());
        let extended =
            panproto_inst::functor_extend_engine(&instance, &src, &tgt, &compiled, both(&protocol))
                .unwrap();
        let unchased = panproto_inst::validate_table(&extended, &tgt, &protocol);
        assert_eq!(
            unchased.status,
            panproto_gat::CheckStatus::Failed,
            "{unchased:?}"
        );
        let named = Dependency::Tgd {
            body: vec![Atom::new("u", [("tag", AtomTerm::Var("v".into()))])],
            head: vec![Atom::new("w", [("name", AtomTerm::Var("v".into()))])],
        };
        let result = lift_functor_sigma(
            &compiled,
            &src,
            &tgt,
            &instance,
            &[named],
            ChaseBudget::new(10, 10),
            both(&protocol),
        )
        .unwrap();
        assert_eq!(result.row_count("w"), 1);
        assert_eq!(
            panproto_inst::validate_table(&result, &tgt, &protocol).status,
            panproto_gat::CheckStatus::Passed
        );
    }

    /// A `Sigma` lift whose output does not validate is refused with the
    /// report, on the output side, in both shapes.
    #[test]
    fn a_sigma_lift_refuses_an_invalid_output() {
        use crate::chase::ChaseBudget;

        // Tables: without the chase the named row is missing.
        let (instance, compiled) = tagged_rows();
        let src = table_schema(&[("t", &["tag"])]);
        let (tgt, protocol) = (referencing_schema(), referencing_protocol());
        let err = lift_functor_sigma(
            &compiled,
            &src,
            &tgt,
            &instance,
            &[],
            ChaseBudget::new(10, 10),
            both(&protocol),
        )
        .unwrap_err();
        assert!(
            matches!(
                &err,
                LiftError::Restrict(RestrictError::Validation {
                    side: BoundarySide::Output,
                    report,
                }) if report.status == panproto_gat::CheckStatus::Failed
            ),
            "{err:?}"
        );

        // Trees: the target bounds the length of the value the node carries.
        let leaf = Edge {
            src: "body".into(),
            tgt: "body.text".into(),
            kind: "prop".into(),
            name: Some("text".into()),
        };
        let mut tgt = test_schema(
            &[("body", "object"), ("body.text", "string")],
            std::slice::from_ref(&leaf),
        );
        let src = tgt.clone();
        tgt.constraints.insert(
            "body.text".into(),
            vec![panproto_schema::Constraint {
                sort: "maxLength".into(),
                value: "3".into(),
            }],
        );
        let mut protocol = test_protocol();
        protocol.constraint_sorts.push("maxLength".into());
        protocol.constraint_semantics.insert(
            "maxLength".into(),
            panproto_schema::ConstraintSemantics::MaxLength(panproto_schema::LengthUnit::Chars),
        );
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "body")),
                (
                    1,
                    Node::new(1, "body.text")
                        .with_value(FieldPresence::Present(Value::Str("too long".into()))),
                ),
            ]),
            vec![(0, 1, leaf.clone())],
            vec![],
            0,
            "body".into(),
        );
        let identity = CompiledMigration {
            surviving_verts: HashSet::from(["body".into(), "body.text".into()]),
            surviving_edges: HashSet::from([leaf]),
            ..CompiledMigration::default()
        };
        let err = lift_wtype_sigma(&identity, &src, &tgt, &tree, both(&protocol)).unwrap_err();
        assert!(
            matches!(
                &err,
                LiftError::Restrict(RestrictError::Validation {
                    side: BoundarySide::Output,
                    ..
                })
            ),
            "{err:?}"
        );
        // A short enough value passes.
        let mut short = tree;
        if let Some(node) = short.nodes.get_mut(&1) {
            node.value = Some(FieldPresence::Present(Value::Str("ok".into())));
        }
        assert!(lift_wtype_sigma(&identity, &src, &tgt, &short, both(&protocol)).is_ok());
    }

    /// The table `Pi` lift is the right Kan extension along the one
    /// migration functor that both schemas and both protocols determine:
    /// it equals `Pi` along [`panproto_inst::MigrationFunctor::new`] of the
    /// same arguments, and a protocol that classifies no table refuses it
    /// with that constructor's error.
    #[test]
    fn lift_functor_pi_uses_the_migration_functor() {
        let named = |v: &str| HashMap::from([("a".to_owned(), Value::Str(v.into()))]);
        let factors = FInstance::new()
            .with_table("a", vec![named("p"), named("q")])
            .and_then(|f| f.with_table("b", vec![named("p")]))
            .unwrap();
        let column = |table: &str| Edge {
            src: table.into(),
            tgt: format!("{table}.a").into(),
            kind: "prop".into(),
            name: Some("a".into()),
        };
        let mut product = CompiledMigration::default();
        product.surviving_verts.insert("t".into());
        for table in ["a", "b"] {
            product.vertex_remap.insert(table.into(), "t".into());
            product
                .vertex_remap
                .insert(format!("{table}.a").into(), "t.a".into());
            product.edge_remap.insert(column(table), column("t"));
        }
        let src = table_schema(&[("a", &["a"]), ("b", &["a"])]);
        let tgt = table_schema(&[("t", &["a"])]);
        let protocol = table_protocol();
        let functor =
            panproto_inst::MigrationFunctor::new(&src, &protocol, &tgt, &protocol, &product)
                .unwrap();
        assert_eq!(
            lift_functor_pi(&product, &src, &protocol, &tgt, &protocol, &factors, 16).unwrap(),
            panproto_inst::functor_pi_along(&factors, &functor, 16).unwrap()
        );
        // Under a protocol with no table kinds the functor cannot be built,
        // and the lift reports exactly that.
        let bare = test_protocol();
        let refusal =
            panproto_inst::MigrationFunctor::new(&src, &bare, &tgt, &bare, &product).unwrap_err();
        let err = lift_functor_pi(&product, &src, &bare, &tgt, &bare, &factors, 16).unwrap_err();
        assert_eq!(
            format!("{err:?}"),
            format!(
                "{:?}",
                LiftError::Adjunction(panproto_inst::AdjunctionError::Pi(refusal))
            )
        );
    }
}
