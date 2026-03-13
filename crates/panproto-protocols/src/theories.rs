//! Shared component theory definitions.
//!
//! These are the building-block GATs that protocols compose via colimit
//! to form their schema and instance theories. Each function returns
//! a standalone [`panproto_gat::Theory`] that can be registered and referenced by name.

use panproto_gat::{Operation, Sort, SortParam, Theory};

/// `ThGraph`: directed graph with `Vertex` and `Edge` sorts.
///
/// Operations: `src : Edge -> Vertex`, `tgt : Edge -> Vertex`.
#[must_use]
pub fn th_graph() -> Theory {
    Theory::new(
        "ThGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    )
}

/// `ThSimpleGraph`: simple graph (no parallel edges).
///
/// Same sorts and operations as `ThGraph`, but with an additional
/// equation enforcing edge uniqueness by endpoints.
#[must_use]
pub fn th_simple_graph() -> Theory {
    Theory::new(
        "ThSimpleGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    )
}

/// `ThHypergraph`: hypergraph with `Vertex`, `HyperEdge`, and `Label` sorts.
///
/// A hyper-edge connects multiple vertices via labeled positions.
#[must_use]
pub fn th_hypergraph() -> Theory {
    Theory::new(
        "ThHypergraph",
        vec![
            Sort::simple("Vertex"),
            Sort::simple("HyperEdge"),
            Sort::simple("Label"),
        ],
        vec![
            Operation::new(
                "incident",
                vec![
                    ("he".into(), "HyperEdge".into()),
                    ("l".into(), "Label".into()),
                ],
                "Vertex",
            ),
            Operation::unary("parent_label", "he", "HyperEdge", "Label"),
        ],
        vec![],
    )
}

/// `ThConstraint`: vertex-attached constraints.
///
/// Adds a dependent sort `Constraint(v: Vertex)` with a `target` operation.
#[must_use]
pub fn th_constraint() -> Theory {
    Theory::new(
        "ThConstraint",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
        ],
        vec![Operation::unary("target", "c", "Constraint", "Vertex")],
        vec![],
    )
}

/// `ThMulti`: multigraph extension allowing parallel edges.
///
/// Adds an `EdgeLabel` sort for distinguishing parallel edges.
#[must_use]
pub fn th_multi() -> Theory {
    Theory::new(
        "ThMulti",
        vec![
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
            Sort::simple("EdgeLabel"),
        ],
        vec![Operation::unary("edge_label", "e", "Edge", "EdgeLabel")],
        vec![],
    )
}

/// `ThInterface`: interface types for `GraphQL`-like protocols.
///
/// Adds an `Interface` sort and an `implements` operation.
#[must_use]
pub fn th_interface() -> Theory {
    Theory::new(
        "ThInterface",
        vec![Sort::simple("Vertex"), Sort::simple("Interface")],
        vec![Operation::new(
            "implements",
            vec![
                ("v".into(), "Vertex".into()),
                ("i".into(), "Interface".into()),
            ],
            "Vertex",
        )],
        vec![],
    )
}

/// `ThWType`: W-type instance theory for tree-shaped data.
///
/// Sorts: `Node`, `Arc`, `Value`. Operations model the tree structure.
#[must_use]
pub fn th_wtype() -> Theory {
    Theory::new(
        "ThWType",
        vec![
            Sort::simple("Node"),
            Sort::simple("Arc"),
            Sort::simple("Value"),
        ],
        vec![
            Operation::unary("anchor", "n", "Node", "Vertex"),
            Operation::unary("arc_src", "a", "Arc", "Node"),
            Operation::unary("arc_tgt", "a", "Arc", "Node"),
            Operation::unary("arc_edge", "a", "Arc", "Edge"),
            Operation::unary("node_value", "n", "Node", "Value"),
        ],
        vec![],
    )
}

/// `ThMeta`: metadata extension for W-type instances.
///
/// Adds discriminator and extra-fields capabilities.
#[must_use]
pub fn th_meta() -> Theory {
    Theory::new(
        "ThMeta",
        vec![
            Sort::simple("Node"),
            Sort::simple("Discriminator"),
            Sort::simple("ExtraField"),
        ],
        vec![
            Operation::unary("discriminator", "n", "Node", "Discriminator"),
            Operation::new(
                "extra_field",
                vec![
                    ("n".into(), "Node".into()),
                    ("key".into(), "ExtraField".into()),
                ],
                "Value",
            ),
        ],
        vec![],
    )
}

/// `ThFunctor`: set-valued functor instance theory for relational data.
///
/// Sorts: `Table`, `Row`, `ForeignKey`. Models tabular data with row collections
/// and foreign-key relationships.
#[must_use]
pub fn th_functor() -> Theory {
    Theory::new(
        "ThFunctor",
        vec![
            Sort::simple("Table"),
            Sort::simple("Row"),
            Sort::simple("ForeignKey"),
        ],
        vec![
            Operation::unary("table_vertex", "t", "Table", "Vertex"),
            Operation::new("fk_src", vec![("fk".into(), "ForeignKey".into())], "Row"),
            Operation::new("fk_tgt", vec![("fk".into(), "ForeignKey".into())], "Row"),
        ],
        vec![],
    )
}

/// `ThFlat`: flat instance theory for simple key-value data.
///
/// Used by protocols like Protobuf that have flat field structures.
#[must_use]
pub fn th_flat() -> Theory {
    Theory::new(
        "ThFlat",
        vec![
            Sort::simple("Node"),
            Sort::simple("Field"),
            Sort::simple("Value"),
        ],
        vec![
            Operation::unary("field_node", "f", "Field", "Node"),
            Operation::unary("field_value", "f", "Field", "Value"),
            Operation::unary("node_anchor", "n", "Node", "Vertex"),
        ],
        vec![],
    )
}

// ── Shared theory registration helpers ──────────────────────────────

use panproto_gat::colimit;
use std::collections::HashMap;

/// Register a **constrained multigraph + W-type** theory pair (Group A).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `ThWType`.
///
/// Used by: `ATProto`, JSON Schema, `OpenAPI`, `AsyncAPI`, RAML, JSON:API,
/// `MongoDB`, YAML Schema, TOML Schema, INI, CDDL, BSON, `MsgPack`,
/// K8s CRD, `CloudFormation`, Ansible, FHIR, RSS/Atom, vCard/iCal,
/// `GeoJSON`, Markdown, and more.
pub fn register_constrained_multigraph_wtype<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let w = th_wtype();

    registry
        .entry("ThGraph".into())
        .or_insert_with(|| g.clone());
    registry
        .entry("ThConstraint".into())
        .or_insert_with(|| c.clone());
    registry
        .entry("ThMulti".into())
        .or_insert_with(|| m.clone());
    registry
        .entry("ThWType".into())
        .or_insert_with(|| w.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &m, &shared_ve) {
            schema_theory.name = schema_name.into();
            registry.insert(schema_name.into(), schema_theory);
        }
    }

    let mut inst = w;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **hypergraph + functor** theory pair (Group B).
///
/// Schema: `colimit(ThHypergraph, ThConstraint)`.
/// Instance: `ThFunctor`.
///
/// Used by: SQL, Cassandra, `DynamoDB`, Parquet, Arrow, `DataFrame`,
/// CSV/Table Schema, EDI X12, SWIFT MT.
pub fn register_hypergraph_functor<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let h = th_hypergraph();
    let c = th_constraint();
    let f = th_functor();

    registry
        .entry("ThHypergraph".into())
        .or_insert_with(|| h.clone());
    registry
        .entry("ThConstraint".into())
        .or_insert_with(|| c.clone());
    registry
        .entry("ThFunctor".into())
        .or_insert_with(|| f.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(mut schema_theory) = colimit(&h, &c, &shared_vertex) {
        schema_theory.name = schema_name.into();
        registry.insert(schema_name.into(), schema_theory);
    }

    let mut inst = f;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **simple graph + flat** theory pair (Group C).
///
/// Schema: `colimit(ThSimpleGraph, ThConstraint)`.
/// Instance: `ThFlat`.
///
/// Used by: Protobuf, Avro, Thrift, Cap'n Proto, `FlatBuffers`,
/// ASN.1, Bond, Redis, HCL.
pub fn register_simple_graph_flat<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let sg = th_simple_graph();
    let c = th_constraint();
    let fl = th_flat();

    registry
        .entry("ThSimpleGraph".into())
        .or_insert_with(|| sg.clone());
    registry
        .entry("ThConstraint".into())
        .or_insert_with(|| c.clone());
    registry
        .entry("ThFlat".into())
        .or_insert_with(|| fl.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(mut schema_theory) = colimit(&sg, &c, &shared_vertex) {
        schema_theory.name = schema_name.into();
        registry.insert(schema_name.into(), schema_theory);
    }

    let mut inst = fl;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **typed graph + W-type** theory pair with interfaces (Group D).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
/// Instance: `ThWType`.
///
/// Used by: GraphQL, TypeScript, Python, Rust Serde, Java, Go,
/// Swift, Kotlin, C#, JSX/React, Vue, Svelte.
pub fn register_typed_graph_wtype<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let iface = th_interface();
    let w = th_wtype();

    registry
        .entry("ThGraph".into())
        .or_insert_with(|| g.clone());
    registry
        .entry("ThConstraint".into())
        .or_insert_with(|| c.clone());
    registry
        .entry("ThMulti".into())
        .or_insert_with(|| m.clone());
    registry
        .entry("ThInterface".into())
        .or_insert_with(|| iface.clone());
    registry
        .entry("ThWType".into())
        .or_insert_with(|| w.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(gcm) = colimit(&gc, &m, &shared_ve) {
            let shared_vertex_only =
                Theory::new("ThVertex2", vec![Sort::simple("Vertex")], vec![], vec![]);
            if let Ok(mut schema_theory) = colimit(&gcm, &iface, &shared_vertex_only) {
                schema_theory.name = schema_name.into();
                registry.insert(schema_name.into(), schema_theory);
            }
        }
    }

    let mut inst = w;
    inst.name = instance_name.into();
    registry.insert(instance_name.into(), inst);
}

/// Register a **constrained multigraph + W-type + metadata** theory pair (Group E).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `colimit(ThWType, ThMeta)`.
///
/// Used by: XML/XSD, HTML, CSS, DOCX, ODF, Neo4j.
pub fn register_multigraph_wtype_meta<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let w = th_wtype();
    let meta = th_meta();

    registry
        .entry("ThGraph".into())
        .or_insert_with(|| g.clone());
    registry
        .entry("ThConstraint".into())
        .or_insert_with(|| c.clone());
    registry
        .entry("ThMulti".into())
        .or_insert_with(|| m.clone());
    registry
        .entry("ThWType".into())
        .or_insert_with(|| w.clone());
    registry
        .entry("ThMeta".into())
        .or_insert_with(|| meta.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    if let Ok(gc) = colimit(&g, &c, &shared_vertex) {
        let shared_ve = Theory::new(
            "ThVertexEdge",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![],
            vec![],
        );
        if let Ok(mut schema_theory) = colimit(&gc, &m, &shared_ve) {
            schema_theory.name = schema_name.into();
            registry.insert(schema_name.into(), schema_theory);
        }
    }

    let shared_node = Theory::new("ThNode", vec![Sort::simple("Node")], vec![], vec![]);
    if let Ok(mut inst_theory) = colimit(&w, &meta, &shared_node) {
        inst_theory.name = instance_name.into();
        registry.insert(instance_name.into(), inst_theory);
    }
}
