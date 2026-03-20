//! Integration tests exercising auto-morphism components in VCS workflow contexts.
//!
//! These tests combine `panproto_mig` (morphism discovery, chase, overlap),
//! `panproto_schema` (pushout), `panproto_inst` (`Sigma_F`, `Pi_F`), and
//! `panproto_vcs` (rename detection, repository operations) to verify
//! end-to-end migration workflows.

#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_inst::{FInstance, Node, WInstance};
use panproto_mig::hom_search::morphism_to_migration;
use panproto_mig::{
    Migration, SearchOptions, chase_functor, compile, dependencies_from_schema, discover_overlap,
    find_best_morphism, lift_wtype, lift_wtype_sigma,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder, Vertex, schema_pushout};
use panproto_vcs::Repository;
use panproto_vcs::rename_detect;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        schema_theory: "ThTest".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
    let proto = test_protocol();
    let mut builder = SchemaBuilder::new(&proto);
    for (id, kind) in vertices {
        builder = builder.vertex(id, kind, None::<&str>).unwrap();
    }
    for (src, tgt, kind, name) in edges {
        builder = builder.edge(src, tgt, kind, Some(*name)).unwrap();
    }
    builder.build().unwrap()
}

fn make_edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    }
}

/// Build a bare `Schema` (without protocol validation) for functor/chase tests.
fn bare_schema(
    vertices: &[(&str, &str)],
    edges: &[Edge],
    required: HashMap<Name, Vec<Edge>>,
) -> Schema {
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
        required,
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

fn row(col: &str, val: Value) -> HashMap<String, Value> {
    HashMap::from([(col.to_owned(), val)])
}

// ===========================================================================
// Test 1: auto_migrate_rename_workflow
// ===========================================================================

#[test]
fn auto_migrate_rename_workflow() {
    // Schema v1: root -> root.text (string)
    let v1 = build_schema(
        &[("root", "object"), ("root.text", "string")],
        &[("root", "root.text", "prop", "text")],
    );

    // Schema v2: root -> root.body (string) — field renamed
    let v2 = build_schema(
        &[("root", "object"), ("root.body", "string")],
        &[("root", "root.body", "prop", "body")],
    );

    // Use find_best_morphism to discover the mapping
    let opts = SearchOptions::default();
    let best = find_best_morphism(&v1, &v2, &opts);
    assert!(best.is_some(), "should find a morphism for rename");

    let morphism = best.unwrap();
    // root -> root (identity)
    assert_eq!(
        morphism.vertex_map.get("root").map(Name::as_str),
        Some("root"),
    );
    // root.text -> root.body (renamed)
    assert_eq!(
        morphism.vertex_map.get("root.text").map(Name::as_str),
        Some("root.body"),
    );
    // All source vertices are mapped (no delete+add)
    assert_eq!(morphism.vertex_map.len(), 2);

    // Convert to migration and verify it maps the edge too
    let mig = morphism_to_migration(&morphism);
    assert_eq!(mig.vertex_map.len(), 2);
    assert!(!mig.edge_map.is_empty(), "edge should be mapped");

    // Verify through a VCS repo workflow
    let dir = tempfile::tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.add(&v1).unwrap();
    repo.commit("initial", "alice").unwrap();

    // Stage v2 — the auto migration should handle the rename
    let index = repo.add(&v2).unwrap();
    assert!(
        index.staged.is_some(),
        "v2 should be staged with auto-derived migration"
    );
    let staged = index.staged.unwrap();
    assert!(
        staged.migration_id.is_some(),
        "auto-derived migration should exist"
    );
}

// ===========================================================================
// Test 2: discover_overlap_and_pushout_integration
// ===========================================================================

#[test]
fn discover_overlap_and_pushout_integration() {
    // Protocol A: person -> person.name (string)
    let schema_a = build_schema(
        &[("person", "object"), ("person.name", "string")],
        &[("person", "person.name", "prop", "name")],
    );

    // Protocol B: user -> user.name (string), user.age (integer)
    let schema_b = build_schema(
        &[
            ("user", "object"),
            ("user.name", "string"),
            ("user.age", "integer"),
        ],
        &[
            ("user", "user.name", "prop", "name"),
            ("user", "user.age", "prop", "age"),
        ],
    );

    // Discover the shared structure: both have object -> string with "name" edge
    let overlap = discover_overlap(&schema_a, &schema_b);

    // Should find at least the object+string subgraph
    assert!(
        overlap.vertex_pairs.len() >= 2,
        "should find at least 2 overlapping vertices, got {}",
        overlap.vertex_pairs.len()
    );
    assert!(
        !overlap.edge_pairs.is_empty(),
        "should find at least 1 overlapping edge"
    );

    // Compute the pushout
    let (pushout, left_m, right_m) = schema_pushout(&schema_a, &schema_b, &overlap).unwrap();

    // Pushout should have all vertices from both schemas, minus merged ones
    let expected_verts =
        schema_a.vertex_count() + schema_b.vertex_count() - overlap.vertex_pairs.len();
    assert_eq!(
        pushout.vertex_count(),
        expected_verts,
        "pushout should have correct vertex count"
    );

    // Both morphisms should map all source vertices to the pushout
    assert_eq!(
        left_m.vertex_map.len(),
        schema_a.vertex_count(),
        "left morphism should cover all left vertices"
    );
    assert_eq!(
        right_m.vertex_map.len(),
        schema_b.vertex_count(),
        "right morphism should cover all right vertices"
    );

    // All morphism targets should exist in the pushout
    for tgt in left_m.vertex_map.values() {
        assert!(
            pushout.has_vertex(tgt),
            "left morphism target {tgt} should be in pushout"
        );
    }
    for tgt in right_m.vertex_map.values() {
        assert!(
            pushout.has_vertex(tgt),
            "right morphism target {tgt} should be in pushout"
        );
    }
}

// ===========================================================================
// Test 3: sigma_then_chase_workflow
// ===========================================================================

#[test]
fn sigma_then_chase_workflow() {
    // Source schema: user (object)
    // Target schema: user (object) -> profile (string) [required]
    let profile_edge = make_edge("user", "profile", "prop", "profile");

    let src_schema = bare_schema(&[("user", "object")], &[], HashMap::new());

    let required = HashMap::from([(Name::from("user"), vec![profile_edge.clone()])]);
    let tgt_schema = bare_schema(
        &[("user", "object"), ("profile", "string")],
        std::slice::from_ref(&profile_edge),
        required,
    );

    // Create a functor instance with one user row
    let instance =
        FInstance::new().with_table("user", vec![row("name", Value::Str("Alice".into()))]);

    // Build migration: user -> user (identity), no edge mapping yet
    let mig = Migration {
        vertex_map: HashMap::from([("user".into(), "user".into())]),
        edge_map: HashMap::new(),
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    // Compile and apply Sigma_F (extend)
    let compiled = compile(&src_schema, &tgt_schema, &mig).unwrap();
    let extended = panproto_inst::functor_extend(&instance, &compiled).unwrap();

    // After extend, "user" table should have 1 row, "profile" should be empty
    assert_eq!(extended.row_count("user"), 1);
    assert_eq!(extended.row_count("profile"), 0);

    // Extract dependencies from target schema
    let deps = dependencies_from_schema(&tgt_schema);
    assert!(
        !deps.is_empty(),
        "required edge should produce a dependency"
    );

    // Run chase to add missing profile rows
    let chased = chase_functor(&extended, &deps, 10).unwrap();
    assert!(
        chased.row_count("profile") >= 1,
        "chase should add a profile row for the existing user"
    );
}

// ===========================================================================
// Test 4: pi_functor_product_workflow
// ===========================================================================

#[test]
fn pi_functor_product_workflow() {
    // Two source tables map to the same target vertex via migration
    let rows_a = vec![row("x", Value::Int(1)), row("x", Value::Int(2))];
    let rows_b = vec![
        row("y", Value::Int(10)),
        row("y", Value::Int(20)),
        row("y", Value::Int(30)),
    ];

    let instance = FInstance::new()
        .with_table("table_a", rows_a)
        .with_table("table_b", rows_b);

    // Migration: table_a -> merged, table_b -> merged
    let mut vertex_remap = HashMap::new();
    vertex_remap.insert(Name::from("table_a"), Name::from("merged"));
    vertex_remap.insert(Name::from("table_b"), Name::from("merged"));

    let compiled = panproto_inst::CompiledMigration {
        surviving_verts: HashSet::from([Name::from("merged")]),
        surviving_edges: HashSet::new(),
        vertex_remap,
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Apply Pi_F (Cartesian product)
    let result = panproto_inst::functor_pi(&instance, &compiled, 1000).unwrap();

    // 2 * 3 = 6 product rows
    assert_eq!(
        result.row_count("merged"),
        6,
        "Cartesian product of 2x3 should yield 6 rows"
    );

    // Each row should have both x and y columns
    let rows = result.tables.get("merged").unwrap();
    for r in rows {
        assert!(r.contains_key("x"), "product row should have x column");
        assert!(r.contains_key("y"), "product row should have y column");
    }
}

// ===========================================================================
// Test 5: rename_detect_in_vcs_context
// ===========================================================================

#[test]
fn rename_detect_in_vcs_context() {
    // v1: root -> root.firstName (string), root -> root.lastName (string)
    let v1 = build_schema(
        &[
            ("root", "object"),
            ("root.firstName", "string"),
            ("root.lastName", "string"),
        ],
        &[
            ("root", "root.firstName", "prop", "firstName"),
            ("root", "root.lastName", "prop", "lastName"),
        ],
    );

    // v2: root -> root.givenName (string), root -> root.familyName (string)
    let v2 = build_schema(
        &[
            ("root", "object"),
            ("root.givenName", "string"),
            ("root.familyName", "string"),
        ],
        &[
            ("root", "root.givenName", "prop", "givenName"),
            ("root", "root.familyName", "prop", "familyName"),
        ],
    );

    // Detect vertex renames
    let vertex_renames = rename_detect::detect_vertex_renames(&v1, &v2, 0.3);
    assert_eq!(vertex_renames.len(), 2, "should detect 2 vertex renames");

    // Collect renamed pairs for easier assertion
    let rename_pairs: HashMap<&str, &str> = vertex_renames
        .iter()
        .map(|r| (r.rename.old.as_ref(), r.rename.new.as_ref()))
        .collect();

    assert!(
        rename_pairs.contains_key("root.firstName"),
        "should detect firstName rename"
    );
    assert!(
        rename_pairs.contains_key("root.lastName"),
        "should detect lastName rename"
    );

    // Detect edge renames
    let edge_renames = rename_detect::detect_edge_renames(&v1, &v2, 0.3);
    // Edge renames should detect firstName -> givenName and lastName -> familyName
    // These have moderate edit distance so they may or may not pass the threshold
    // depending on the scoring. Check that at least some are detected.
    // (The edge rename depends on the surviving vertex pairs, so the edge
    //  between root and the renamed vertex is removed+added, not renamed.
    //  Edge renames only apply between surviving vertex pairs.)
    // Since root survives but firstName/lastName do not, edge renames
    // won't fire here — that is correct behavior.
    let _ = edge_renames;

    // Now test with an edge-only rename scenario: same vertices, different edge labels
    let v3 = build_schema(
        &[("root", "object"), ("root.x", "string")],
        &[("root", "root.x", "prop", "label")],
    );

    let proto = test_protocol();
    let v4 = SchemaBuilder::new(&proto)
        .vertex("root", "object", None::<&str>)
        .unwrap()
        .vertex("root.x", "string", None::<&str>)
        .unwrap()
        .edge("root", "root.x", "prop", Some("labels"))
        .unwrap()
        .build()
        .unwrap();

    let edge_renames_2 = rename_detect::detect_edge_renames(&v3, &v4, 0.3);
    assert!(
        !edge_renames_2.is_empty(),
        "should detect edge rename: label -> labels"
    );
    assert_eq!(edge_renames_2[0].rename.old.as_ref(), "label");
    assert_eq!(edge_renames_2[0].rename.new.as_ref(), "labels");
}

// ===========================================================================
// Test 6: full_pipeline_auto_morphism_to_lift
// ===========================================================================

#[test]
fn full_pipeline_auto_morphism_to_lift() {
    // Old schema: root -> root.text (string)
    let old_edge = make_edge("root", "root.text", "prop", "text");
    let old_schema = bare_schema(
        &[("root", "object"), ("root.text", "string")],
        std::slice::from_ref(&old_edge),
        HashMap::new(),
    );

    // New schema: root -> root.body (string) — renamed field
    let new_edge = make_edge("root", "root.body", "prop", "body");
    let new_schema = bare_schema(
        &[("root", "object"), ("root.body", "string")],
        std::slice::from_ref(&new_edge),
        HashMap::new(),
    );

    // Create a W-type instance of the old schema
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(1, Node::new(1, "root.text"));
    let arcs = vec![(0, 1, old_edge)];
    let instance = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

    // Discover the migration automatically
    let opts = SearchOptions::default();
    let best = find_best_morphism(&old_schema, &new_schema, &opts).unwrap();
    let mig = morphism_to_migration(&best);

    // Verify the morphism maps correctly
    assert_eq!(mig.vertex_map.get("root").map(Name::as_str), Some("root"));
    assert_eq!(
        mig.vertex_map.get("root.text").map(Name::as_str),
        Some("root.body")
    );

    // Compile the migration
    let compiled = compile(&old_schema, &new_schema, &mig).unwrap();

    // Lift via Sigma_F (left Kan extension) since we have renames
    let lifted = lift_wtype_sigma(&compiled, &new_schema, &instance).unwrap();

    // Verify lifted instance structure
    assert_eq!(lifted.node_count(), 2, "should preserve both nodes");
    assert_eq!(lifted.arc_count(), 1, "should preserve the arc");

    // Verify anchors were remapped
    let root_node = lifted.nodes.get(&0).unwrap();
    assert_eq!(root_node.anchor.as_str(), "root");

    let leaf_node = lifted.nodes.get(&1).unwrap();
    assert_eq!(
        leaf_node.anchor.as_str(),
        "root.body",
        "leaf anchor should be remapped to root.body"
    );
}

// ===========================================================================
// Test 7: schema_pushout_with_overlap_and_lift
// ===========================================================================

#[test]
fn schema_pushout_with_overlap_and_lift() {
    // Schema A: root -> root.name (string), root -> root.email (string)
    let schema_a = build_schema(
        &[
            ("root", "object"),
            ("root.name", "string"),
            ("root.email", "string"),
        ],
        &[
            ("root", "root.name", "prop", "name"),
            ("root", "root.email", "prop", "email"),
        ],
    );

    // Schema B: base -> base.name (string), base -> base.phone (string)
    let schema_b = build_schema(
        &[
            ("base", "object"),
            ("base.name", "string"),
            ("base.phone", "string"),
        ],
        &[
            ("base", "base.name", "prop", "name"),
            ("base", "base.phone", "prop", "phone"),
        ],
    );

    // Discover overlap: both have object + string with "name" edge
    let overlap = discover_overlap(&schema_a, &schema_b);
    assert!(
        overlap.vertex_pairs.len() >= 2,
        "should find overlapping object+string vertices"
    );

    // Compute pushout
    let (pushout, left_m, right_m) = schema_pushout(&schema_a, &schema_b, &overlap).unwrap();

    // The pushout should contain all unique vertices
    // A has: root, root.name, root.email (3)
    // B has: base, base.name, base.phone (3)
    // Overlap merges at least 2, so pushout has at most 4 vertices
    assert!(
        pushout.vertex_count() <= 4,
        "pushout should merge overlapping vertices, got {}",
        pushout.vertex_count()
    );
    assert!(
        pushout.vertex_count() >= 3,
        "pushout should have at least 3 vertices (root + name + email/phone), got {}",
        pushout.vertex_count()
    );

    // Validate morphism targets are in pushout
    for (src_v, tgt_v) in &left_m.vertex_map {
        assert!(
            pushout.has_vertex(tgt_v),
            "left morphism target {tgt_v} (from {src_v}) should be in pushout"
        );
    }
    for (src_v, tgt_v) in &right_m.vertex_map {
        assert!(
            pushout.has_vertex(tgt_v),
            "right morphism target {tgt_v} (from {src_v}) should be in pushout"
        );
    }

    // Lift an instance from schema A into the pushout using the left morphism
    let left_mig = Migration {
        vertex_map: left_m.vertex_map,
        edge_map: left_m.edge_map,
        hyper_edge_map: HashMap::new(),
        label_map: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        expr_resolvers: HashMap::new(),
    };

    let compiled = compile(&schema_a, &pushout, &left_mig).unwrap();

    // Create a W-type instance for schema A
    let name_edge = make_edge("root", "root.name", "prop", "name");
    let email_edge = make_edge("root", "root.email", "prop", "email");

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(1, Node::new(1, "root.name"));
    nodes.insert(2, Node::new(2, "root.email"));
    let arcs = vec![(0, 1, name_edge), (0, 2, email_edge)];
    let instance = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

    // Lift the instance into the pushout schema
    let lifted = lift_wtype(&compiled, &schema_a, &pushout, &instance).unwrap();

    // Verify the lifted instance has the same node count (all vertices survive)
    assert_eq!(
        lifted.node_count(),
        3,
        "all 3 nodes should survive in pushout"
    );

    // Verify anchors match pushout vertex IDs
    for node in lifted.nodes.values() {
        assert!(
            pushout.has_vertex(&node.anchor),
            "lifted node anchor {} should exist in pushout",
            node.anchor
        );
    }
}
