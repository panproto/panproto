//! Shared component theory definitions (11 building blocks).
//!
//! These are the building-block GATs that protocols compose via colimit
//! to form their schema and instance theories. Each function returns
//! a standalone [`panproto_gat::Theory`] that can be registered and
//! referenced by name.
//!
//! ## Formal invariants
//!
//! Every theory satisfies:
//! 1. **Sort closure**: all sort names referenced in operation inputs/outputs
//!    are declared in the theory's sorts list.
//! 2. **Equation well-typedness**: both sides of every equation have the
//!    same output sort under the declared operations.
//! 3. **No tautological equations**: `lhs ≠ rhs` syntactically for all equations.
//! 4. **Rewrite-system soundness**: the directed rewrite system is locally
//!    confluent and LPO-terminating, checked at registration.
//!
//! ## Inventory
//!
//! | #  | Theory | Category |
//! |----|--------|----------|
//! | 1  | `ThGraph` | Schema shape |
//! | 2  | `ThConstraint` | Schema modifier |
//! | 3  | `ThMulti` | Schema modifier |
//! | 4  | `ThWType` | Instance shape |
//! | 5  | `ThMeta` | Instance modifier |
//! | 6  | `ThSimpleGraph` | Schema shape |
//! | 7  | `ThHypergraph` | Schema shape |
//! | 8  | `ThInterface` | Schema shape |
//! | 9  | `ThFunctor` | Instance shape |
//! | 10 | `ThFlat` | Instance shape |
//! | 11 | `ThGraphInstance` | Instance shape |
//!
//! The registration helpers in the middle of this module compose these into
//! the schema/instance theory pair each protocol group uses, and the
//! composition specs below them are the declarative recipe for the same
//! colimits.

use panproto_gat::{
    CompositionMode, CompositionSpec, CompositionStep, Equation, Operation, Sort, SortParam, Term,
    Theory,
};

// ═══════════════════════════════════════════════════════════════════
// Building blocks
// ═══════════════════════════════════════════════════════════════════

/// `ThGraph`: directed graph.
///
/// Sorts: `Vertex`, `Edge`.
/// Ops: `src : Edge → Vertex`, `tgt : Edge → Vertex`.
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

/// `ThConstraint`: vertex-attached constraints.
///
/// Sorts: `Vertex`, `Constraint(v: Vertex)` (dependent).
/// Ops: `target : Constraint → Vertex`.
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

/// `ThMulti`: multigraph (parallel edges).
///
/// Sorts: `Vertex`, `Edge`, `EdgeLabel`.
/// Ops: `edge_label : Edge → EdgeLabel`.
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

/// `ThWType`: W-type instance theory (tree-shaped data).
///
/// Sorts: `Node`, `Arc`, `Value`, `Vertex`, `Edge`.
/// Ops: `anchor`, `arc_src`, `arc_tgt`, `arc_edge`, `node_value`, `src`, `tgt`.
///
/// `anchor : Node → Vertex` and `arc_edge : Arc → Edge` land in the schema
/// sorts, and the schema's `src`/`tgt` projections are declared here so the
/// two coherence axioms can be stated: an arc's endpoints are anchored at the
/// endpoints of the schema edge it is an instance of.
///
/// ```text
/// arc_src_anchor : anchor(arc_src(a)) = src(arc_edge(a))
/// arc_tgt_anchor : anchor(arc_tgt(a)) = tgt(arc_edge(a))
/// ```
///
/// The shared sorts and projections are identified with the schema theory's
/// when the instance theory is composed with it via colimit.
#[must_use]
pub fn th_wtype() -> Theory {
    Theory::new(
        "ThWType",
        vec![
            Sort::simple("Node"),
            Sort::simple("Arc"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
        ],
        vec![
            Operation::unary("anchor", "n", "Node", "Vertex"),
            Operation::unary("arc_src", "a", "Arc", "Node"),
            Operation::unary("arc_tgt", "a", "Arc", "Node"),
            Operation::unary("arc_edge", "a", "Arc", "Edge"),
            Operation::unary("node_value", "n", "Node", "Value"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![
            Equation::new(
                "arc_src_anchor",
                Term::app("anchor", vec![Term::app("arc_src", vec![Term::var("a")])]),
                Term::app("src", vec![Term::app("arc_edge", vec![Term::var("a")])]),
            ),
            Equation::new(
                "arc_tgt_anchor",
                Term::app("anchor", vec![Term::app("arc_tgt", vec![Term::var("a")])]),
                Term::app("tgt", vec![Term::app("arc_edge", vec![Term::var("a")])]),
            ),
        ],
    )
}

/// `ThMeta`: metadata extension for W-type instances.
///
/// Sorts: `Node`, `Discriminator`, `ExtraField`.
/// Ops: `discriminator`, `extra_field`.
///
/// Note: `extra_field` outputs `Value`, which is also declared in `ThWType`.
/// These sorts are identified when `ThMeta` is composed with `ThWType` via
/// colimit over shared `{Node, Value}`.
#[must_use]
pub fn th_meta() -> Theory {
    Theory::new(
        "ThMeta",
        vec![
            Sort::simple("Node"),
            Sort::simple("Discriminator"),
            Sort::simple("ExtraField"),
            Sort::simple("Value"),
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

// ═══════════════════════════════════════════════════════════════════
// Theory group registration helpers
// ═══════════════════════════════════════════════════════════════════

use panproto_gat::pushout_by_name;
use panproto_gat::validate_rewrite_system;
use std::collections::HashMap;

/// Gate a to-be-registered theory on the soundness of its directed rewrite
/// system (local confluence and LPO termination under operation-declaration
/// order). Theories with no directed equations (every built-in composed
/// here) pass trivially.
///
/// The theories this gate sees are assembled from the building blocks in
/// this module, not supplied by a caller, so a failure is a defect in
/// those blocks rather than bad input. It is reported the same way the
/// colimits above report theirs.
///
/// # Panics
///
/// Panics if the soundness analysis cannot be completed or reports a
/// non-joining critical pair or an LPO violation. Normalization decides a
/// theory's equality judgment by running its rewrite system, so
/// registering one that is not provably sound would leave every schema
/// built on the theory resting on an unsound judgment.
fn gate_rewrite_system(theory: &Theory) {
    let report = validate_rewrite_system(theory).unwrap_or_else(|e| {
        panic!(
            "theory `{}`: rewrite-system soundness could not be decided: {e}",
            theory.name
        )
    });
    let warnings = report.warnings();
    assert!(
        warnings.is_empty(),
        "theory `{}` has an unsound rewrite system: {}",
        theory.name,
        warnings.join("; ")
    );
}

/// Register a **constrained multigraph + W-type** theory pair (Group A).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `ThWType`.
///
/// Used by: `ATProto`, `JSON Schema`, `OpenAPI`, `AsyncAPI`, `RAML`, `JSON:API`,
/// `MongoDB`, `YAML Schema`, `TOML Schema`, `INI`, `CDDL`, `BSON`, `MsgPack`,
/// `K8s CRD`, `CloudFormation`, `Ansible`, `FHIR`, `RSS/Atom`, `vCard/iCal`,
/// `GeoJSON`, `Markdown`, and more.
///
/// # Panics
///
/// Panics if the underlying colimits over the building-block theories
/// fail. This is a static, build-time configuration; a panic here
/// indicates a panproto-internal bug in the building-block theories,
/// not a runtime input error.
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
    let gc = pushout_by_name(&g, &c, &shared_vertex)
        .unwrap_or_else(|e| panic!("colimit ThGraph + ThConstraint over ThVertex failed: {e}"))
        .theory;
    let shared_ve = Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    );
    let mut schema_theory = pushout_by_name(&gc, &m, &shared_ve)
        .unwrap_or_else(|e| {
            panic!("colimit (ThGraph+ThConstraint) + ThMulti over ThVertexEdge failed: {e}")
        })
        .theory;
    schema_theory.name = schema_name.into();
    gate_rewrite_system(&schema_theory);
    registry.insert(schema_name.into(), schema_theory);

    let mut inst = w;
    inst.name = instance_name.into();
    gate_rewrite_system(&inst);
    registry.insert(instance_name.into(), inst);
}

/// Register a **hypergraph + functor** theory pair (Group B).
///
/// Schema: `colimit(ThHypergraph, ThConstraint)`.
/// Instance: `ThFunctor`.
///
/// Used by: SQL, Cassandra, `DynamoDB`, Parquet, Arrow, `DataFrame`,
/// CSV/Table Schema, EDI X12, SWIFT MT.
///
/// # Panics
///
/// Panics if the colimit construction over the building-block
/// theories fails (a panproto-internal bug, not a runtime input
/// error).
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
    let mut schema_theory = pushout_by_name(&h, &c, &shared_vertex)
        .unwrap_or_else(|e| panic!("colimit ThHypergraph + ThConstraint over ThVertex failed: {e}"))
        .theory;
    schema_theory.name = schema_name.into();
    gate_rewrite_system(&schema_theory);
    registry.insert(schema_name.into(), schema_theory);

    let mut inst = f;
    inst.name = instance_name.into();
    gate_rewrite_system(&inst);
    registry.insert(instance_name.into(), inst);
}

/// Register a **simple graph + flat** theory pair (Group C).
///
/// Schema: `colimit(ThSimpleGraph, ThConstraint)`.
/// Instance: `ThFlat`.
///
/// Used by: `Protobuf`, `Avro`, `FlatBuffers`, `ASN.1`, `Bond`, `Redis`.
///
/// # Panics
///
/// Panics if the colimit construction over the building-block
/// theories fails (a panproto-internal bug, not a runtime input
/// error).
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
    let mut schema_theory = pushout_by_name(&sg, &c, &shared_vertex)
        .unwrap_or_else(|e| {
            panic!("colimit ThSimpleGraph + ThConstraint over ThVertex failed: {e}")
        })
        .theory;
    schema_theory.name = schema_name.into();
    gate_rewrite_system(&schema_theory);
    registry.insert(schema_name.into(), schema_theory);

    let mut inst = fl;
    inst.name = instance_name.into();
    gate_rewrite_system(&inst);
    registry.insert(instance_name.into(), inst);
}

/// Register a **typed graph + W-type** theory pair with interfaces (Group D).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti, ThInterface)`.
/// Instance: `ThWType`.
///
/// Used by: GraphQL, TypeScript, Python, Rust Serde, Java, Go,
/// Swift, Kotlin, C#, JSX/React, Vue, Svelte.
///
/// # Panics
///
/// Panics if the colimit construction over the building-block
/// theories fails (a panproto-internal bug, not a runtime input
/// error).
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
    let gc = pushout_by_name(&g, &c, &shared_vertex)
        .unwrap_or_else(|e| panic!("colimit ThGraph + ThConstraint over ThVertex failed: {e}"))
        .theory;
    let shared_ve = Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    );
    let gcm = pushout_by_name(&gc, &m, &shared_ve)
        .unwrap_or_else(|e| {
            panic!("colimit (ThGraph+ThConstraint) + ThMulti over ThVertexEdge failed: {e}")
        })
        .theory;
    let shared_vertex_only = Theory::new("ThVertex2", vec![Sort::simple("Vertex")], vec![], vec![]);
    let mut schema_theory = pushout_by_name(&gcm, &iface, &shared_vertex_only)
        .unwrap_or_else(|e| panic!("colimit (...+ThMulti) + ThInterface over ThVertex failed: {e}"))
        .theory;
    schema_theory.name = schema_name.into();
    gate_rewrite_system(&schema_theory);
    registry.insert(schema_name.into(), schema_theory);

    let mut inst = w;
    inst.name = instance_name.into();
    gate_rewrite_system(&inst);
    registry.insert(instance_name.into(), inst);
}

/// Register a **constrained multigraph + W-type + metadata** theory pair (Group E).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `colimit(ThWType, ThMeta)`.
///
/// Used by: XML/XSD, HTML, CSS, DOCX, ODF.
///
/// # Panics
///
/// Panics if the colimit construction over the building-block
/// theories fails (a panproto-internal bug, not a runtime input
/// error).
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
    let gc = pushout_by_name(&g, &c, &shared_vertex)
        .unwrap_or_else(|e| panic!("colimit ThGraph + ThConstraint over ThVertex failed: {e}"))
        .theory;
    let shared_ve = Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    );
    let mut schema_theory = pushout_by_name(&gc, &m, &shared_ve)
        .unwrap_or_else(|e| {
            panic!("colimit (ThGraph+ThConstraint) + ThMulti over ThVertexEdge failed: {e}")
        })
        .theory;
    schema_theory.name = schema_name.into();
    gate_rewrite_system(&schema_theory);
    registry.insert(schema_name.into(), schema_theory);

    let shared_node_value = Theory::new(
        "ThNodeValue",
        vec![Sort::simple("Node"), Sort::simple("Value")],
        vec![],
        vec![],
    );
    let mut inst_theory = pushout_by_name(&w, &meta, &shared_node_value)
        .unwrap_or_else(|e| panic!("colimit ThWType + ThMeta over ThNodeValue failed: {e}"))
        .theory;
    inst_theory.name = instance_name.into();
    gate_rewrite_system(&inst_theory);
    registry.insert(instance_name.into(), inst_theory);
}

/// Register a **constrained multigraph + graph instance** theory pair (Group F).
///
/// Schema: `colimit(ThGraph, ThConstraint, ThMulti)`.
/// Instance: `ThGraphInstance`.
///
/// Used by: Neo4j, and future graph-native protocols (RDF, OWL,
/// JSON-LD, knowledge graphs).
///
/// # Panics
///
/// Panics if the colimit construction over the building-block
/// theories fails (a panproto-internal bug, not a runtime input
/// error).
pub fn register_constrained_graph_instance<S: ::std::hash::BuildHasher>(
    registry: &mut HashMap<String, Theory, S>,
    schema_name: &str,
    instance_name: &str,
) {
    let g = th_graph();
    let c = th_constraint();
    let m = th_multi();
    let gi = th_graph_instance();

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
        .entry("ThGraphInstance".into())
        .or_insert_with(|| gi.clone());

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    let gc = pushout_by_name(&g, &c, &shared_vertex)
        .unwrap_or_else(|e| panic!("colimit ThGraph + ThConstraint over ThVertex failed: {e}"))
        .theory;
    let shared_ve = Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    );
    let mut schema_theory = pushout_by_name(&gc, &m, &shared_ve)
        .unwrap_or_else(|e| {
            panic!("colimit (ThGraph+ThConstraint) + ThMulti over ThVertexEdge failed: {e}")
        })
        .theory;
    schema_theory.name = schema_name.into();
    gate_rewrite_system(&schema_theory);
    registry.insert(schema_name.into(), schema_theory);

    let mut inst = gi;
    inst.name = instance_name.into();
    gate_rewrite_system(&inst);
    registry.insert(instance_name.into(), inst);
}

// ═══════════════════════════════════════════════════════════════════
// Composition specs (declarative recipes for each group)
// ═══════════════════════════════════════════════════════════════════

/// Composition specs for Group A (constrained multigraph + W-type).
///
/// Schema: `colimit(colimit(ThGraph, ThConstraint; Vertex), ThMulti; Vertex, Edge)`.
/// Instance: `ThWType`.
#[must_use]
pub fn constrained_multigraph_wtype_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    let schema_spec = CompositionSpec {
        result_name: schema_name.to_owned(),
        version: 0,
        steps: vec![
            CompositionStep::Colimit {
                left: "ThGraph".to_owned(),
                right: "ThConstraint".to_owned(),
                shared_sorts: vec!["Vertex".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
            CompositionStep::Colimit {
                left: "step_0".to_owned(),
                right: "ThMulti".to_owned(),
                shared_sorts: vec!["Vertex".to_owned(), "Edge".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
        ],
    };
    let instance_spec = CompositionSpec {
        result_name: instance_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Base("ThWType".to_owned())],
    };
    (schema_spec, instance_spec)
}

/// Composition specs for Group B (hypergraph + functor).
///
/// Schema: `colimit(ThHypergraph, ThConstraint; Vertex)`.
/// Instance: `ThFunctor`.
#[must_use]
pub fn hypergraph_functor_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    let schema_spec = CompositionSpec {
        result_name: schema_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Colimit {
            left: "ThHypergraph".to_owned(),
            right: "ThConstraint".to_owned(),
            shared_sorts: vec!["Vertex".to_owned()],
            shared_ops: vec![],
            mode: CompositionMode::Amalgamate,
        }],
    };
    let instance_spec = CompositionSpec {
        result_name: instance_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Base("ThFunctor".to_owned())],
    };
    (schema_spec, instance_spec)
}

/// Composition specs for Group C (simple graph + flat).
///
/// Schema: `colimit(ThSimpleGraph, ThConstraint; Vertex)`.
/// Instance: `ThFlat`.
#[must_use]
pub fn simple_graph_flat_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    let schema_spec = CompositionSpec {
        result_name: schema_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Colimit {
            left: "ThSimpleGraph".to_owned(),
            right: "ThConstraint".to_owned(),
            shared_sorts: vec!["Vertex".to_owned()],
            shared_ops: vec![],
            mode: CompositionMode::Amalgamate,
        }],
    };
    let instance_spec = CompositionSpec {
        result_name: instance_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Base("ThFlat".to_owned())],
    };
    (schema_spec, instance_spec)
}

/// Composition specs for Group D (typed graph + W-type with interfaces).
///
/// Schema: `colimit(colimit(colimit(ThGraph, ThConstraint; Vertex), ThMulti; Vertex, Edge), ThInterface; Vertex)`.
/// Instance: `ThWType`.
#[must_use]
pub fn typed_graph_wtype_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    let schema_spec = CompositionSpec {
        result_name: schema_name.to_owned(),
        version: 0,
        steps: vec![
            CompositionStep::Colimit {
                left: "ThGraph".to_owned(),
                right: "ThConstraint".to_owned(),
                shared_sorts: vec!["Vertex".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
            CompositionStep::Colimit {
                left: "step_0".to_owned(),
                right: "ThMulti".to_owned(),
                shared_sorts: vec!["Vertex".to_owned(), "Edge".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
            CompositionStep::Colimit {
                left: "step_1".to_owned(),
                right: "ThInterface".to_owned(),
                shared_sorts: vec!["Vertex".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
        ],
    };
    let instance_spec = CompositionSpec {
        result_name: instance_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Base("ThWType".to_owned())],
    };
    (schema_spec, instance_spec)
}

/// Composition specs for Group E (constrained multigraph + W-type + metadata).
///
/// Schema: `colimit(colimit(ThGraph, ThConstraint; Vertex), ThMulti; Vertex, Edge)`.
/// Instance: `colimit(ThWType, ThMeta; Node)`.
#[must_use]
pub fn multigraph_wtype_meta_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    let schema_spec = CompositionSpec {
        result_name: schema_name.to_owned(),
        version: 0,
        steps: vec![
            CompositionStep::Colimit {
                left: "ThGraph".to_owned(),
                right: "ThConstraint".to_owned(),
                shared_sorts: vec!["Vertex".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
            CompositionStep::Colimit {
                left: "step_0".to_owned(),
                right: "ThMulti".to_owned(),
                shared_sorts: vec!["Vertex".to_owned(), "Edge".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
        ],
    };
    let instance_spec = CompositionSpec {
        result_name: instance_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Colimit {
            left: "ThWType".to_owned(),
            right: "ThMeta".to_owned(),
            shared_sorts: vec!["Node".to_owned(), "Value".to_owned()],
            shared_ops: vec![],
            mode: CompositionMode::Amalgamate,
        }],
    };
    (schema_spec, instance_spec)
}

/// Composition specs for Group F (constrained multigraph + graph instance).
///
/// Schema: `colimit(colimit(ThGraph, ThConstraint; Vertex), ThMulti; Vertex, Edge)`.
/// Instance: `ThGraphInstance`.
#[must_use]
pub fn constrained_graph_instance_specs(
    schema_name: &str,
    instance_name: &str,
) -> (CompositionSpec, CompositionSpec) {
    let schema_spec = CompositionSpec {
        result_name: schema_name.to_owned(),
        version: 0,
        steps: vec![
            CompositionStep::Colimit {
                left: "ThGraph".to_owned(),
                right: "ThConstraint".to_owned(),
                shared_sorts: vec!["Vertex".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
            CompositionStep::Colimit {
                left: "step_0".to_owned(),
                right: "ThMulti".to_owned(),
                shared_sorts: vec!["Vertex".to_owned(), "Edge".to_owned()],
                shared_ops: vec![],
                mode: CompositionMode::Amalgamate,
            },
        ],
    };
    let instance_spec = CompositionSpec {
        result_name: instance_name.to_owned(),
        version: 0,
        steps: vec![CompositionStep::Base("ThGraphInstance".to_owned())],
    };
    (schema_spec, instance_spec)
}

// ═══════════════════════════════════════════════════════════════════
// Building blocks the registration helpers above compose
// ═══════════════════════════════════════════════════════════════════

/// `ThSimpleGraph`: simple graph (no parallel edges).
///
/// Uses a dependent sort `Edge(s: Vertex, t: Vertex)` to encode edge
/// uniqueness structurally.
#[must_use]
pub fn th_simple_graph() -> Theory {
    Theory::new(
        "ThSimpleGraph",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent(
                "Edge",
                vec![SortParam::new("s", "Vertex"), SortParam::new("t", "Vertex")],
            ),
        ],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    )
}

/// `ThHypergraph`: hypergraph with labeled incidence.
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

/// `ThInterface`: interface types (GraphQL, TypeScript, etc.).
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

/// `ThFunctor`: set-valued functor instance (relational data).
///
/// Sorts: `Table`, `Row`, `ForeignKey`, and the schema sorts `Vertex`,
/// `Edge`.
/// Ops: `table_vertex`, `row_table`, `fk_src`, `fk_tgt`, `fk_edge`, and the
/// schema projections `src`, `tgt`.
///
/// A table is anchored at a schema vertex and a foreign key at the schema
/// edge it instantiates; the schema sorts and projections are declared here
/// so the two coherence axioms can be stated: a foreign key runs between
/// rows of the tables anchored at its schema edge's endpoints.
///
/// ```text
/// fk_src_anchor : src(fk_edge(k)) = table_vertex(row_table(fk_src(k)))
/// fk_tgt_anchor : tgt(fk_edge(k)) = table_vertex(row_table(fk_tgt(k)))
/// ```
///
/// Composed with a table schema theory, which has vertices but no edges,
/// only `Vertex` is shared; `Edge`, `src` and `tgt` stay the instance
/// theory's own.
#[must_use]
pub fn th_functor() -> Theory {
    let anchored = |proj: &str, end: &str| {
        (
            Term::app(proj, vec![Term::app("fk_edge", vec![Term::var("k")])]),
            Term::app(
                "table_vertex",
                vec![Term::app(
                    "row_table",
                    vec![Term::app(end, vec![Term::var("k")])],
                )],
            ),
        )
    };
    let (src_lhs, src_rhs) = anchored("src", "fk_src");
    let (tgt_lhs, tgt_rhs) = anchored("tgt", "fk_tgt");
    Theory::new(
        "ThFunctor",
        vec![
            Sort::simple("Table"),
            Sort::simple("Row"),
            Sort::simple("ForeignKey"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
        ],
        vec![
            Operation::unary("table_vertex", "t", "Table", "Vertex"),
            Operation::new("fk_src", vec![("fk".into(), "ForeignKey".into())], "Row"),
            Operation::new("fk_tgt", vec![("fk".into(), "ForeignKey".into())], "Row"),
            Operation::unary("row_table", "r", "Row", "Table"),
            Operation::unary("fk_edge", "fk", "ForeignKey", "Edge"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![
            Equation::new("fk_src_anchor", src_lhs, src_rhs),
            Equation::new("fk_tgt_anchor", tgt_lhs, tgt_rhs),
        ],
    )
}

/// `ThFlat`: flat instance theory (key-value data).
///
/// Sorts: `Node`, `Field`, `Value`, and the schema sort `Vertex` that a
/// node is anchored at.
#[must_use]
pub fn th_flat() -> Theory {
    Theory::new(
        "ThFlat",
        vec![
            Sort::simple("Node"),
            Sort::simple("Field"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
        ],
        vec![
            Operation::unary("field_node", "f", "Field", "Node"),
            Operation::unary("field_value", "f", "Field", "Value"),
            Operation::unary("node_anchor", "n", "Node", "Vertex"),
        ],
        vec![],
    )
}

/// `ThGraphInstance`: graph-shaped instance data (most general form).
///
/// Sorts: `IVertex`, `IEdge`, `Value`, the schema sorts `Vertex`, `Edge`,
/// and the fan incidence sorts `FanNode`, `FanPort`, `HyperEdge`, `Label`.
/// Ops: `i_src`, `i_tgt`, `iv_anchor`, `ie_anchor`, `iv_value`, the schema
/// projections `src`, `tgt`, and the fan incidence operations
/// `fan_anchor`, `fan_parent`, `port_fan`, `port_label`, `port_target`.
///
/// An instance edge is anchored at the schema edge it instantiates, and its
/// endpoints at that schema edge's endpoints:
///
/// ```text
/// ie_src_anchor : src(ie_anchor(e)) = iv_anchor(i_src(e))
/// ie_tgt_anchor : tgt(ie_anchor(e)) = iv_anchor(i_tgt(e))
/// ```
///
/// An n-ary relation (a fan) is not an instance edge. A fan node is
/// anchored at a hyper-edge name and has a parent instance vertex; each of
/// its ports carries a signature label and reaches one participant. No
/// equation relates a hyper-edge name or a label to a schema vertex, so
/// fan incidence is never read as an edge between vertices.
#[must_use]
pub fn th_graph_instance() -> Theory {
    let anchored = |proj: &str, end: &str| {
        (
            Term::app(proj, vec![Term::app("ie_anchor", vec![Term::var("e")])]),
            Term::app("iv_anchor", vec![Term::app(end, vec![Term::var("e")])]),
        )
    };
    let (src_lhs, src_rhs) = anchored("src", "i_src");
    let (tgt_lhs, tgt_rhs) = anchored("tgt", "i_tgt");
    Theory::new(
        "ThGraphInstance",
        vec![
            Sort::simple("IVertex"),
            Sort::simple("IEdge"),
            Sort::simple("Value"),
            Sort::simple("Vertex"),
            Sort::simple("Edge"),
            Sort::simple("FanNode"),
            Sort::simple("FanPort"),
            Sort::simple("HyperEdge"),
            Sort::simple("Label"),
        ],
        vec![
            Operation::unary("i_src", "e", "IEdge", "IVertex"),
            Operation::unary("i_tgt", "e", "IEdge", "IVertex"),
            Operation::unary("iv_anchor", "v", "IVertex", "Vertex"),
            Operation::unary("ie_anchor", "e", "IEdge", "Edge"),
            Operation::unary("iv_value", "v", "IVertex", "Value"),
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
            Operation::unary("fan_anchor", "f", "FanNode", "HyperEdge"),
            Operation::unary("fan_parent", "f", "FanNode", "IVertex"),
            Operation::unary("port_fan", "p", "FanPort", "FanNode"),
            Operation::unary("port_label", "p", "FanPort", "Label"),
            Operation::unary("port_target", "p", "FanPort", "IVertex"),
        ],
        vec![
            Equation::new("ie_src_anchor", src_lhs, src_rhs),
            Equation::new("ie_tgt_anchor", tgt_lhs, tgt_rhs),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::{
        CheckOutcome, ClashPolicy, ComposeOptions, Composition, CompositionCheckKind,
        CompositionMode, DEFAULT_VERIFY_BUDGET, Term, TheoryDiagram, compose, typecheck_signature,
        typecheck_theory,
    };

    /// The interface a tree or graph instance theory shares with its schema
    /// theory: the schema's vertices, edges and edge endpoints.
    fn vertex_edge_interface() -> Theory {
        Theory::new(
            "Interface",
            vec![Sort::simple("Vertex"), Sort::simple("Edge")],
            vec![
                Operation::unary("src", "e", "Edge", "Vertex"),
                Operation::unary("tgt", "e", "Edge", "Vertex"),
            ],
            vec![],
        )
    }

    /// The interface a table or flat instance theory shares with its schema
    /// theory: the schema's vertices only.
    fn vertex_interface() -> Theory {
        Theory::new("Interface", vec![Sort::simple("Vertex")], vec![], vec![])
    }

    type Register = fn(&mut HashMap<String, Theory>, &str, &str);

    /// The schema theory a registration helper registers under
    /// `{tag}Schema`. Each helper gets its own tag, since a theory name
    /// stands for one composition in a process.
    fn schema_of(register: Register, tag: &str) -> Theory {
        let mut registry = HashMap::new();
        let name = format!("{tag}Schema");
        register(&mut registry, &name, &format!("{tag}Instance"));
        registry
            .remove(&name)
            .expect("the helper registers its schema theory")
    }

    /// The instance theory a registration helper registers under
    /// `{tag}Instance`.
    fn instance_of(register: Register, tag: &str) -> Theory {
        let mut registry = HashMap::new();
        let name = format!("{tag}Instance");
        register(&mut registry, &format!("{tag}Schema"), &name);
        registry
            .remove(&name)
            .expect("the helper registers its instance theory")
    }

    /// `compose` of `instance` with `schema` over `interface`, identifying
    /// exactly the interface and refusing any other shared name.
    fn compose_over(interface: Theory, schema: &Theory, instance: &Theory) -> Composition {
        let diagram = TheoryDiagram::by_name(
            ("Interface", interface),
            ("Schema", schema.clone()),
            ("Instance", instance.clone()),
        )
        .unwrap_or_else(|e| {
            panic!(
                "{} over the interface into {}: {e}",
                instance.name, schema.name
            )
        });
        let opts = ComposeOptions {
            mode: CompositionMode::ExplicitIdentity,
            result_name: "SchemaAndInstance".into(),
            clash: ClashPolicy::Reject,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        };
        compose(&diagram, &opts)
            .unwrap_or_else(|e| panic!("{} with {}: {e}", instance.name, schema.name))
    }

    /// Every inclusion of `composition` is a morphism and the identity is
    /// its universal mediator, each recorded as `Passed`, and no check of it
    /// is inapplicable.
    fn assert_total(composition: &Composition, what: &str) {
        for check in [
            CompositionCheckKind::InclusionIsMorphism,
            CompositionCheckKind::UniversalIdentity,
        ] {
            let records: Vec<_> = composition.report.records(check).collect();
            assert!(!records.is_empty(), "{what}: no {check:?} record");
            for record in records {
                assert_eq!(
                    record.outcome,
                    CheckOutcome::Passed,
                    "{what}: {check:?} on {:?}",
                    record.subject
                );
            }
        }
        let inapplicable: Vec<_> = composition
            .report
            .checks
            .iter()
            .filter(|r| matches!(r.outcome, CheckOutcome::Inapplicable { .. }))
            .collect();
        assert!(inapplicable.is_empty(), "{what}: {inapplicable:?}");
    }

    /// A building block typechecks on its own, signature and body.
    fn assert_typechecks(theory: &Theory) {
        assert_eq!(
            typecheck_signature(theory),
            Ok(()),
            "{}: signature",
            theory.name
        );
        assert!(
            typecheck_theory(theory).is_ok(),
            "{}: {:?}",
            theory.name,
            typecheck_theory(theory)
        );
    }

    #[test]
    fn every_real_building_block_typechecks_standalone() {
        for theory in [
            th_graph(),
            th_constraint(),
            th_multi(),
            th_wtype(),
            th_meta(),
            th_simple_graph(),
            th_hypergraph(),
            th_interface(),
            th_functor(),
            th_flat(),
            th_graph_instance(),
        ] {
            assert_typechecks(&theory);
        }
    }

    #[test]
    fn th_graph_instance_typechecks_and_composes_with_every_graph_schema_theory() {
        let theory = th_graph_instance();
        assert_typechecks(&theory);
        let names: Vec<&str> = theory.eqs.iter().map(|e| &*e.name).collect();
        assert_eq!(names, vec!["ie_src_anchor", "ie_tgt_anchor"]);
        // src(ie_anchor(e)) = iv_anchor(i_src(e)), and likewise for tgt.
        let anchored = |proj: &str, end: &str| {
            (
                Term::app(proj, vec![Term::app("ie_anchor", vec![Term::var("e")])]),
                Term::app("iv_anchor", vec![Term::app(end, vec![Term::var("e")])]),
            )
        };
        assert_eq!(
            (theory.eqs[0].lhs.clone(), theory.eqs[0].rhs.clone()),
            anchored("src", "i_src")
        );
        assert_eq!(
            (theory.eqs[1].lhs.clone(), theory.eqs[1].rhs.clone()),
            anchored("tgt", "i_tgt")
        );
        let schema = schema_of(register_constrained_graph_instance, "Q6Graph");
        assert_total(
            &compose_over(vertex_edge_interface(), &schema, &theory),
            "ThGraphInstance with the constrained multigraph",
        );
    }

    #[test]
    fn fan_ports_are_not_related_to_vertices() {
        let theory = th_graph_instance();
        assert_typechecks(&theory);
        // Fan incidence is its own sort pair, anchored at hyper-edge names
        // and signature labels.
        for sort in ["FanNode", "FanPort", "HyperEdge", "Label"] {
            assert!(theory.has_sort(sort), "{sort} is declared");
        }
        let output = |op: &str| {
            theory
                .find_op(op)
                .map(|o| o.output.head().to_string())
                .unwrap_or_else(|| panic!("{op} is declared"))
        };
        assert_eq!(output("fan_anchor"), "HyperEdge");
        assert_eq!(output("port_label"), "Label");
        assert_eq!(output("port_fan"), "FanNode");
        // No operation reaches a schema vertex or edge from the fan sorts,
        // and no equation mentions a fan operation, so nothing equates a
        // hyper-edge name with a vertex.
        let fan_sorts = ["FanNode", "FanPort", "HyperEdge", "Label"];
        for op in &theory.ops {
            let from_fan = op
                .inputs
                .iter()
                .any(|(_, sort, _)| fan_sorts.contains(&sort.head().as_ref()));
            let to_schema = ["Vertex", "Edge"].contains(&op.output.head().as_ref());
            assert!(
                !(from_fan && to_schema),
                "{} relates a fan to the schema",
                op.name
            );
        }
        let fan_ops: Vec<&str> = theory
            .ops
            .iter()
            .filter(|op| {
                op.inputs
                    .iter()
                    .any(|(_, sort, _)| fan_sorts.contains(&sort.head().as_ref()))
            })
            .map(|op| &*op.name)
            .collect();
        assert!(!fan_ops.is_empty());
        for eq in &theory.eqs {
            let text = format!("{:?} {:?}", eq.lhs, eq.rhs);
            for op in &fan_ops {
                assert!(
                    !text.contains(&format!("\"{op}\"")),
                    "{} mentions {op}",
                    eq.name
                );
            }
        }
    }

    #[test]
    fn th_functor_typechecks_and_composes_with_every_table_schema_theory() {
        let theory = th_functor();
        assert_typechecks(&theory);
        for sort in ["Table", "Row", "ForeignKey", "Vertex", "Edge"] {
            assert!(theory.has_sort(sort), "{sort} is declared");
        }
        for op in [
            "table_vertex",
            "fk_src",
            "fk_tgt",
            "src",
            "tgt",
            "row_table",
            "fk_edge",
        ] {
            assert!(theory.has_op(op), "{op} is declared");
        }
        let names: Vec<&str> = theory.eqs.iter().map(|e| &*e.name).collect();
        assert_eq!(names, vec!["fk_src_anchor", "fk_tgt_anchor"]);
        let anchored = |proj: &str, end: &str| {
            (
                Term::app(proj, vec![Term::app("fk_edge", vec![Term::var("k")])]),
                Term::app(
                    "table_vertex",
                    vec![Term::app(
                        "row_table",
                        vec![Term::app(end, vec![Term::var("k")])],
                    )],
                ),
            )
        };
        assert_eq!(
            (theory.eqs[0].lhs.clone(), theory.eqs[0].rhs.clone()),
            anchored("src", "fk_src")
        );
        assert_eq!(
            (theory.eqs[1].lhs.clone(), theory.eqs[1].rhs.clone()),
            anchored("tgt", "fk_tgt")
        );
        let schema = schema_of(register_hypergraph_functor, "Q6Table");
        assert_total(
            &compose_over(vertex_interface(), &schema, &theory),
            "ThFunctor with the constrained hypergraph",
        );
    }

    #[test]
    fn th_flat_typechecks_and_composes_with_its_schema_theory() {
        let theory = th_flat();
        assert_typechecks(&theory);
        assert!(theory.has_sort("Vertex"));
        let schema = schema_of(register_simple_graph_flat, "Q6Flat");
        assert_total(
            &compose_over(vertex_interface(), &schema, &theory),
            "ThFlat with the constrained simple graph",
        );
    }

    #[test]
    fn standalone_total_theories_pass_inclusion_and_universal_checks() {
        let tree: [(&str, Register); 3] = [
            ("TotalA", register_constrained_multigraph_wtype),
            ("TotalD", register_typed_graph_wtype),
            ("TotalE", register_multigraph_wtype_meta),
        ];
        for (tag, register) in tree {
            assert_total(
                &compose_over(
                    vertex_edge_interface(),
                    &schema_of(register, tag),
                    &instance_of(register, tag),
                ),
                tag,
            );
        }
        let graph: Register = register_constrained_graph_instance;
        assert_total(
            &compose_over(
                vertex_edge_interface(),
                &schema_of(graph, "TotalF"),
                &instance_of(graph, "TotalF"),
            ),
            "TotalF",
        );
        let table: Register = register_hypergraph_functor;
        assert_total(
            &compose_over(
                vertex_interface(),
                &schema_of(table, "TotalB"),
                &instance_of(table, "TotalB"),
            ),
            "TotalB",
        );
        let flat: Register = register_simple_graph_flat;
        assert_total(
            &compose_over(
                vertex_interface(),
                &schema_of(flat, "TotalC"),
                &instance_of(flat, "TotalC"),
            ),
            "TotalC",
        );
    }

    /// The W-type instance theory states the arc/anchor coherence axioms and
    /// declares every sort and projection they mention, so it typechecks on
    /// its own rather than deferring to a later colimit.
    #[test]
    fn wtype_theory_states_the_arc_anchor_coherence_axioms() {
        let theory = th_wtype();
        assert!(
            panproto_gat::typecheck_theory(&theory).is_ok(),
            "ThWType must typecheck standalone: {:?}",
            panproto_gat::typecheck_theory(&theory),
        );

        let names: Vec<&str> = theory.eqs.iter().map(|e| &*e.name).collect();
        assert_eq!(
            names,
            vec!["arc_src_anchor", "arc_tgt_anchor"],
            "both endpoint coherence axioms must be stated",
        );
    }
}
