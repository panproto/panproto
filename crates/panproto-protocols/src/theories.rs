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
