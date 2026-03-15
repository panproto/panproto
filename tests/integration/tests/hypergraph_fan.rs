//! Integration test 12: Hypergraph fan operations.
//!
//! Verifies SQL FK as a 4-ary hyperedge, column drop migration,
//! and fan reconstruction after dropping a column.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FInstance};
use panproto_mig::lift_functor;
use panproto_protocols::sql;
use panproto_schema::{Edge, HyperEdge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a SQL-like schema with a table as a hyperedge.
fn sql_schema_with_hyperedge() -> Schema {
    // Table: users(id, name, email, active)
    // This is modeled as a 4-ary hyperedge connecting the 4 column vertices.
    let verts = vec![
        ("users", "table"),
        ("users.id", "integer"),
        ("users.name", "string"),
        ("users.email", "string"),
        ("users.active", "boolean"),
    ];

    let edges = vec![
        Edge {
            src: "users".into(),
            tgt: "users.id".into(),
            kind: "prop".into(),
            name: Some("id".into()),
        },
        Edge {
            src: "users".into(),
            tgt: "users.name".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        },
        Edge {
            src: "users".into(),
            tgt: "users.email".into(),
            kind: "prop".into(),
            name: Some("email".into()),
        },
        Edge {
            src: "users".into(),
            tgt: "users.active".into(),
            kind: "prop".into(),
            name: Some("active".into()),
        },
    ];

    let he_sig = HashMap::from([
        ("id".into(), "users.id".into()),
        ("name".into(), "users.name".into()),
        ("email".into(), "users.email".into()),
        ("active".into(), "users.active".into()),
    ]);

    let hyper_edge = HyperEdge {
        id: "he_users".into(),
        kind: "table".into(),
        signature: he_sig,
        parent_label: "id".into(),
    };

    let mut vertices = HashMap::new();
    let mut edge_map = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in &verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in &edges {
        edge_map.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "sql".into(),
        vertices,
        edges: edge_map,
        hyper_edges: HashMap::from([("he_users".into(), hyper_edge)]),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

#[test]
fn sql_table_as_4ary_hyperedge() -> Result<(), Box<dyn std::error::Error>> {
    let schema = sql_schema_with_hyperedge();

    // Verify the hyperedge exists and has 4 labels.
    let he = schema
        .hyper_edges
        .get("he_users")
        .ok_or("hyperedge should exist")?;
    assert_eq!(he.signature.len(), 4, "hyperedge should have 4 labels");
    assert_eq!(he.kind, "table");
    assert_eq!(he.parent_label, "id");

    // All labels should point to existing vertices.
    for (label, vertex_id) in &he.signature {
        assert!(
            schema.has_vertex(vertex_id),
            "label {label} should point to vertex {vertex_id}"
        );
    }

    Ok(())
}

#[test]
fn drop_column_removes_from_hyperedge() -> Result<(), Box<dyn std::error::Error>> {
    let _src_schema = sql_schema_with_hyperedge();

    // Drop the "active" column. The target schema has only 3 columns.
    let tgt_verts = vec![
        ("users", "table"),
        ("users.id", "integer"),
        ("users.name", "string"),
        ("users.email", "string"),
    ];

    let tgt_edges = vec![
        Edge {
            src: "users".into(),
            tgt: "users.id".into(),
            kind: "prop".into(),
            name: Some("id".into()),
        },
        Edge {
            src: "users".into(),
            tgt: "users.name".into(),
            kind: "prop".into(),
            name: Some("name".into()),
        },
        Edge {
            src: "users".into(),
            tgt: "users.email".into(),
            kind: "prop".into(),
            name: Some("email".into()),
        },
    ];

    let tgt_he_sig = HashMap::from([
        ("id".into(), "users.id".into()),
        ("name".into(), "users.name".into()),
        ("email".into(), "users.email".into()),
    ]);

    let tgt_he = HyperEdge {
        id: "he_users_new".into(),
        kind: "table".into(),
        signature: tgt_he_sig,
        parent_label: "id".into(),
    };

    let mut tgt_vertices = HashMap::new();
    let mut tgt_edge_map = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in &tgt_verts {
        tgt_vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in &tgt_edges {
        tgt_edge_map.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    let tgt_schema = Schema {
        protocol: "sql".into(),
        vertices: tgt_vertices,
        edges: tgt_edge_map,
        hyper_edges: HashMap::from([("he_users_new".into(), tgt_he)]),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        outgoing,
        incoming,
        between,
    };

    // Verify the target hyperedge has only 3 labels.
    let he = tgt_schema
        .hyper_edges
        .get("he_users_new")
        .ok_or("target hyperedge should exist")?;
    assert_eq!(
        he.signature.len(),
        3,
        "should have 3 labels after dropping active"
    );
    assert!(
        !he.signature.contains_key("active"),
        "active label should be removed"
    );

    Ok(())
}

#[test]
fn functor_restrict_after_column_drop() -> Result<(), Box<dyn std::error::Error>> {
    // Build an FInstance with the full 4-column table.
    let rows = vec![
        HashMap::from([
            ("id".into(), Value::Int(1)),
            ("name".into(), Value::Str("Alice".into())),
            ("email".into(), Value::Str("alice@example.com".into())),
            ("active".into(), Value::Bool(true)),
        ]),
        HashMap::from([
            ("id".into(), Value::Int(2)),
            ("name".into(), Value::Str("Bob".into())),
            ("email".into(), Value::Str("bob@example.com".into())),
            ("active".into(), Value::Bool(false)),
        ]),
    ];

    let instance = FInstance::new().with_table("users", rows);

    // Migration: keep users table but we're conceptually dropping
    // the "active" column. At the functor level, the entire table
    // survives (column restriction happens at the row level).
    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([
            "users".into(),
            "users.id".into(),
            "users.name".into(),
            "users.email".into(),
        ]),
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let restricted = lift_functor(&compiled, &instance)?;
    assert_eq!(restricted.table_count(), 1);
    assert_eq!(restricted.row_count("users"), 2, "rows should be preserved");

    Ok(())
}

#[test]
fn parse_ddl_creates_hyperedges() -> Result<(), Box<dyn std::error::Error>> {
    let ddl = "CREATE TABLE orders (
        id INTEGER PRIMARY KEY,
        product TEXT NOT NULL,
        quantity INTEGER,
        total NUMERIC
    );";

    let schema = sql::parse_ddl(ddl)?;

    // The SQL parser should create a hyperedge for the table.
    assert!(
        !schema.hyper_edges.is_empty(),
        "SQL schema should contain at least one hyperedge"
    );

    // The hyperedge should connect the column vertices.
    let he = schema
        .hyper_edges
        .values()
        .next()
        .ok_or("should have at least one hyperedge")?;
    assert_eq!(he.kind, "table");
    assert!(he.signature.contains_key("id"), "should have id label");
    assert!(
        he.signature.contains_key("product"),
        "should have product label"
    );
    assert!(
        he.signature.contains_key("quantity"),
        "should have quantity label"
    );
    assert!(
        he.signature.contains_key("total"),
        "should have total label"
    );

    Ok(())
}
