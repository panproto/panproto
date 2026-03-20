//! Integration test 14: Custom protocol.
//!
//! Defines a brand-new protocol (Level 1 data only), builds a
//! schema, and lifts records through it. Verifies that the panproto
//! architecture is truly data-driven: new protocols require no code
//! changes to the engine.

use std::collections::{HashMap, HashSet};

use panproto_gat::{Name, Operation, Sort, Theory};
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance};
use panproto_mig::{Migration, compile, lift_wtype};
use panproto_schema::{Edge, EdgeRule, Protocol, Schema, SchemaBuilder, Vertex};
use smallvec::SmallVec;

/// Define a custom "`ConfigFile`" protocol.
///
/// Schema theory: simple graph (sections contain keys).
/// Instance theory: W-type (tree of section -> key -> value).
fn config_protocol() -> Protocol {
    Protocol {
        name: "config".into(),
        schema_theory: "ThConfigSchema".into(),
        instance_theory: "ThConfigInstance".into(),
        edge_rules: vec![
            EdgeRule {
                edge_kind: "contains".into(),
                src_kinds: vec!["section".into()],
                tgt_kinds: vec!["key".into()],
            },
            EdgeRule {
                edge_kind: "prop".into(),
                src_kinds: vec!["root".into()],
                tgt_kinds: vec!["section".into()],
            },
        ],
        obj_kinds: vec!["root".into(), "section".into(), "key".into()],
        constraint_sorts: vec!["type".into(), "default".into()],
        ..Protocol::default()
    }
}

/// Register theories for the custom protocol.
fn register_config_theories(registry: &mut HashMap<String, Theory>) {
    let th_graph = Theory::new(
        "ThSimpleGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    );

    registry.insert("ThSimpleGraph".into(), th_graph.clone());

    // For the schema theory, we just use ThSimpleGraph directly.
    let mut schema_theory = th_graph;
    schema_theory.name = "ThConfigSchema".into();
    registry.insert("ThConfigSchema".into(), schema_theory);

    // Instance theory: use ThWType.
    let th_wtype = Theory::new(
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
            Operation::unary("node_value", "n", "Node", "Value"),
        ],
        vec![],
    );

    let mut inst_theory = th_wtype.clone();
    inst_theory.name = "ThConfigInstance".into();
    registry.insert("ThWType".into(), th_wtype);
    registry.insert("ThConfigInstance".into(), inst_theory);
}

#[test]
fn custom_protocol_theory_registration() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = HashMap::new();
    register_config_theories(&mut registry);

    assert!(
        registry.contains_key("ThConfigSchema"),
        "schema theory should be registered"
    );
    assert!(
        registry.contains_key("ThConfigInstance"),
        "instance theory should be registered"
    );

    let schema_theory = registry
        .get("ThConfigSchema")
        .ok_or("ThConfigSchema not found")?;
    assert!(schema_theory.find_sort("Vertex").is_some());
    assert!(schema_theory.find_sort("Edge").is_some());

    Ok(())
}

#[test]
fn custom_protocol_build_schema() -> Result<(), Box<dyn std::error::Error>> {
    let proto = config_protocol();
    let builder = SchemaBuilder::new(&proto);

    // Build a config schema:
    //   root
    //   -> database (section)
    //     -> host (key)
    //     -> port (key)
    //   -> logging (section)
    //     -> level (key)
    let schema = builder
        .vertex("root", "root", None)?
        .vertex("database", "section", None)?
        .vertex("database.host", "key", None)?
        .vertex("database.port", "key", None)?
        .vertex("logging", "section", None)?
        .vertex("logging.level", "key", None)?
        .edge("root", "database", "prop", None)?
        .edge("root", "logging", "prop", None)?
        .edge("database", "database.host", "contains", Some("host"))?
        .edge("database", "database.port", "contains", Some("port"))?
        .edge("logging", "logging.level", "contains", Some("level"))?
        .build()?;

    assert!(schema.has_vertex("root"));
    assert!(schema.has_vertex("database"));
    assert!(schema.has_vertex("database.host"));
    assert!(schema.has_vertex("logging.level"));
    assert_eq!(schema.vertices.len(), 6);

    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn custom_protocol_build_instance_and_lift() -> Result<(), Box<dyn std::error::Error>> {
    // Build a config file instance.
    let edge_db = Edge {
        src: "root".into(),
        tgt: "database".into(),
        kind: "prop".into(),
        name: None,
    };
    let edge_host = Edge {
        src: "database".into(),
        tgt: "database.host".into(),
        kind: "contains".into(),
        name: Some("host".into()),
    };
    let edge_port = Edge {
        src: "database".into(),
        tgt: "database.port".into(),
        kind: "contains".into(),
        name: Some("port".into()),
    };
    let edge_log = Edge {
        src: "root".into(),
        tgt: "logging".into(),
        kind: "prop".into(),
        name: None,
    };
    let edge_level = Edge {
        src: "logging".into(),
        tgt: "logging.level".into(),
        kind: "contains".into(),
        name: Some("level".into()),
    };

    let all_edges = vec![
        edge_db.clone(),
        edge_host.clone(),
        edge_port.clone(),
        edge_log.clone(),
        edge_level.clone(),
    ];

    let mut vertices = HashMap::new();
    for (id, kind) in &[
        ("root", "root"),
        ("database", "section"),
        ("database.host", "key"),
        ("database.port", "key"),
        ("logging", "section"),
        ("logging.level", "key"),
    ] {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }

    let mut edges_map = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    for e in &all_edges {
        edges_map.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    let schema = Schema {
        protocol: "config".into(),
        vertices,
        edges: edges_map,
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
    };

    // Build the instance.
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(1, Node::new(1, "database"));
    nodes.insert(
        2,
        Node::new(2, "database.host")
            .with_value(FieldPresence::Present(Value::Str("localhost".into()))),
    );
    nodes.insert(
        3,
        Node::new(3, "database.port").with_value(FieldPresence::Present(Value::Int(5432))),
    );
    nodes.insert(4, Node::new(4, "logging"));
    nodes.insert(
        5,
        Node::new(5, "logging.level").with_value(FieldPresence::Present(Value::Str("info".into()))),
    );

    let arcs = vec![
        (0, 1, edge_db),
        (1, 2, edge_host),
        (1, 3, edge_port),
        (0, 4, edge_log),
        (4, 5, edge_level),
    ];

    let instance = WInstance::new(nodes, arcs, vec![], 0, "root".into());
    assert_eq!(instance.node_count(), 6);

    // Build an identity migration and lift.
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
    };

    let lifted = lift_wtype(&compiled, &schema, &schema, &instance)?;
    assert_eq!(
        lifted.node_count(),
        6,
        "identity lift should preserve all nodes"
    );

    Ok(())
}

#[test]
#[allow(clippy::too_many_lines)]
fn custom_protocol_projection_migration() -> Result<(), Box<dyn std::error::Error>> {
    // Migration: drop the logging section entirely.
    let edge_db = Edge {
        src: "root".into(),
        tgt: "database".into(),
        kind: "prop".into(),
        name: None,
    };
    let edge_host = Edge {
        src: "database".into(),
        tgt: "database.host".into(),
        kind: "contains".into(),
        name: Some("host".into()),
    };
    let edge_port = Edge {
        src: "database".into(),
        tgt: "database.port".into(),
        kind: "contains".into(),
        name: Some("port".into()),
    };
    let edge_log = Edge {
        src: "root".into(),
        tgt: "logging".into(),
        kind: "prop".into(),
        name: None,
    };
    let edge_level = Edge {
        src: "logging".into(),
        tgt: "logging.level".into(),
        kind: "contains".into(),
        name: Some("level".into()),
    };

    // Source schema (full).
    let all_edges = vec![
        edge_db.clone(),
        edge_host.clone(),
        edge_port.clone(),
        edge_log,
        edge_level,
    ];

    let mut src_vertices = HashMap::new();
    for (id, kind) in &[
        ("root", "root"),
        ("database", "section"),
        ("database.host", "key"),
        ("database.port", "key"),
        ("logging", "section"),
        ("logging.level", "key"),
    ] {
        src_vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }

    let mut src_edges = HashMap::new();
    let mut out: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut inc: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut btw: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    for e in &all_edges {
        src_edges.insert(e.clone(), e.kind.clone());
        out.entry(e.src.clone()).or_default().push(e.clone());
        inc.entry(e.tgt.clone()).or_default().push(e.clone());
        btw.entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    let src_schema = Schema {
        protocol: "config".into(),
        vertices: src_vertices,
        edges: src_edges,
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
        outgoing: out,
        incoming: inc,
        between: btw,
    };

    // Target schema: only database section.
    let tgt_edges_list = vec![edge_db.clone(), edge_host.clone(), edge_port.clone()];
    let mut tgt_vertices = HashMap::new();
    for (id, kind) in &[
        ("root", "root"),
        ("database", "section"),
        ("database.host", "key"),
        ("database.port", "key"),
    ] {
        tgt_vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }

    let mut tgt_edge_map = HashMap::new();
    let mut tout: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut tinc: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut tbtw: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();
    for e in &tgt_edges_list {
        tgt_edge_map.insert(e.clone(), e.kind.clone());
        tout.entry(e.src.clone()).or_default().push(e.clone());
        tinc.entry(e.tgt.clone()).or_default().push(e.clone());
        tbtw.entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    let tgt_schema = Schema {
        protocol: "config".into(),
        vertices: tgt_vertices,
        edges: tgt_edge_map,
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
        outgoing: tout,
        incoming: tinc,
        between: tbtw,
    };

    // Build migration.
    let migration = Migration {
        vertex_map: HashMap::from([
            ("root".into(), "root".into()),
            ("database".into(), "database".into()),
            ("database.host".into(), "database.host".into()),
            ("database.port".into(), "database.port".into()),
        ]),
        edge_map: HashMap::from([
            (edge_db.clone(), edge_db),
            (edge_host.clone(), edge_host),
            (edge_port.clone(), edge_port),
        ]),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    let compiled = compile(&src_schema, &tgt_schema, &migration)?;

    // Build instance and lift.
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(1, Node::new(1, "database"));
    nodes.insert(
        2,
        Node::new(2, "database.host")
            .with_value(FieldPresence::Present(Value::Str("localhost".into()))),
    );
    nodes.insert(
        3,
        Node::new(3, "database.port").with_value(FieldPresence::Present(Value::Int(5432))),
    );
    nodes.insert(4, Node::new(4, "logging"));
    nodes.insert(
        5,
        Node::new(5, "logging.level").with_value(FieldPresence::Present(Value::Str("info".into()))),
    );

    let instance = WInstance::new(
        nodes,
        vec![
            (
                0,
                1,
                Edge {
                    src: "root".into(),
                    tgt: "database".into(),
                    kind: "prop".into(),
                    name: None,
                },
            ),
            (
                1,
                2,
                Edge {
                    src: "database".into(),
                    tgt: "database.host".into(),
                    kind: "contains".into(),
                    name: Some("host".into()),
                },
            ),
            (
                1,
                3,
                Edge {
                    src: "database".into(),
                    tgt: "database.port".into(),
                    kind: "contains".into(),
                    name: Some("port".into()),
                },
            ),
            (
                0,
                4,
                Edge {
                    src: "root".into(),
                    tgt: "logging".into(),
                    kind: "prop".into(),
                    name: None,
                },
            ),
            (
                4,
                5,
                Edge {
                    src: "logging".into(),
                    tgt: "logging.level".into(),
                    kind: "contains".into(),
                    name: Some("level".into()),
                },
            ),
        ],
        vec![],
        0,
        "root".into(),
    );

    let lifted = lift_wtype(&compiled, &tgt_schema, &tgt_schema, &instance)?;

    // Should drop the logging section (2 nodes).
    assert!(
        lifted.node_count() <= 4,
        "should have at most 4 nodes after dropping logging, got {}",
        lifted.node_count()
    );
    assert!(lifted.nodes.contains_key(&0), "root should survive");
    assert!(!lifted.nodes.contains_key(&4), "logging should be dropped");
    assert!(
        !lifted.nodes.contains_key(&5),
        "logging.level should be dropped"
    );

    Ok(())
}
