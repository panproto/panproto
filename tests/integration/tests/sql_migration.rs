//! Integration test 4: SQL migration.
//!
//! Verifies: add column migration, FK migration, and set-valued
//! functor restrict.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FInstance};
use panproto_mig::{Migration, compile, lift_functor};
use panproto_protocols::sql;
use panproto_schema::Edge;

#[test]
fn parse_simple_ddl() -> Result<(), Box<dyn std::error::Error>> {
    let ddl = "CREATE TABLE users (
        id INTEGER PRIMARY KEY NOT NULL,
        name VARCHAR(255) NOT NULL,
        email TEXT UNIQUE
    );";

    let schema = sql::parse_ddl(ddl)?;
    assert!(schema.has_vertex("users"), "table vertex should exist");
    assert!(schema.has_vertex("users.id"), "id column should exist");
    assert!(schema.has_vertex("users.name"), "name column should exist");
    assert!(
        schema.has_vertex("users.email"),
        "email column should exist"
    );

    Ok(())
}

#[test]
fn add_column_migration() -> Result<(), Box<dyn std::error::Error>> {
    // Source: users(id, name)
    let src_ddl = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);";
    let src_schema = sql::parse_ddl(src_ddl)?;

    // Target: users(id, name, email)
    let tgt_ddl = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT);";
    let tgt_schema = sql::parse_ddl(tgt_ddl)?;

    // Verify target has the new column.
    assert!(
        tgt_schema.has_vertex("users.email"),
        "email should exist in target"
    );

    // Migration: identity on existing columns.
    let mut vertex_map = HashMap::new();
    vertex_map.insert("users".into(), "users".into());
    vertex_map.insert("users.id".into(), "users.id".into());
    vertex_map.insert("users.name".into(), "users.name".into());

    let edge_id = Edge {
        src: "users".into(),
        tgt: "users.id".into(),
        kind: "prop".into(),
        name: Some("id".into()),
    };
    let edge_name = Edge {
        src: "users".into(),
        tgt: "users.name".into(),
        kind: "prop".into(),
        name: Some("name".into()),
    };

    let migration = Migration {
        vertex_map,
        edge_map: HashMap::from([(edge_id.clone(), edge_id), (edge_name.clone(), edge_name)]),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let compiled = compile(&src_schema, &tgt_schema, &migration)?;
    assert!(
        compiled.surviving_verts.contains("users"),
        "users table should survive"
    );
    assert!(
        compiled.surviving_verts.contains("users.id"),
        "id column should survive"
    );

    Ok(())
}

#[test]
fn functor_restrict_drops_table() -> Result<(), Box<dyn std::error::Error>> {
    // Build an FInstance with two tables.
    let mut users_row = HashMap::new();
    users_row.insert("name".to_string(), Value::Str("Alice".into()));

    let mut posts_row = HashMap::new();
    posts_row.insert("title".to_string(), Value::Str("Hello".into()));

    let fk_edge = Edge {
        src: "posts".into(),
        tgt: "users".into(),
        kind: "fk".into(),
        name: Some("author".into()),
    };

    let instance = FInstance::new()
        .with_table("users", vec![users_row])
        .with_table("posts", vec![posts_row])
        .with_foreign_key(fk_edge, vec![(0, 0)]);

    assert_eq!(instance.table_count(), 2);

    // Migration that only keeps "users".
    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([Name::from("users")]),
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let restricted = lift_functor(&compiled, &instance)?;
    assert_eq!(
        restricted.table_count(),
        1,
        "should have 1 table after restrict"
    );
    assert!(
        restricted.tables.contains_key("users"),
        "users should survive"
    );
    assert!(
        !restricted.tables.contains_key("posts"),
        "posts should be dropped"
    );
    assert!(
        restricted.foreign_keys.is_empty(),
        "FK should be dropped (crosses boundary)"
    );

    Ok(())
}

#[test]
fn functor_restrict_preserves_rows() -> Result<(), Box<dyn std::error::Error>> {
    let rows = vec![
        HashMap::from([
            ("id".into(), Value::Int(1)),
            ("name".into(), Value::Str("Alice".into())),
        ]),
        HashMap::from([
            ("id".into(), Value::Int(2)),
            ("name".into(), Value::Str("Bob".into())),
        ]),
    ];

    let instance = FInstance::new().with_table("users", rows);

    // Identity migration: keep everything.
    let compiled = CompiledMigration {
        surviving_verts: HashSet::from([Name::from("users")]),
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
    };

    let restricted = lift_functor(&compiled, &instance)?;
    assert_eq!(
        restricted.row_count("users"),
        2,
        "should preserve both rows"
    );

    Ok(())
}
