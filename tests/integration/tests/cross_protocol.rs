//! Integration test 5: Cross-protocol migration.
//!
//! Verifies that `ATProto` and SQL schemas can be related via a
//! common sub-theory (`ThGraph`, which both extend). This tests the
//! theory-morphism path for cross-protocol interoperability.

use std::collections::HashMap;

use panproto_gat::{Sort, Theory, TheoryMorphism, check_morphism, colimit};
use panproto_protocols::{atproto, sql};

#[test]
fn both_protocols_share_graph_theory() -> Result<(), Box<dyn std::error::Error>> {
    let mut at_registry = HashMap::new();
    atproto::register_theories(&mut at_registry);

    let mut sql_registry = HashMap::new();
    sql::register_theories(&mut sql_registry);

    // Both should have ThGraph (or its components) in their registries.
    let at_graph = at_registry
        .get("ThGraph")
        .ok_or("ATProto registry should contain ThGraph")?;
    let sql_hypergraph = sql_registry
        .get("ThHypergraph")
        .ok_or("SQL registry should contain ThHypergraph")?;

    // Both theories contain a Vertex sort.
    assert!(
        at_graph.find_sort("Vertex").is_some(),
        "ATProto ThGraph should have Vertex"
    );
    assert!(
        sql_hypergraph.find_sort("Vertex").is_some(),
        "SQL ThHypergraph should have Vertex"
    );

    Ok(())
}

#[test]
fn common_sub_theory_morphisms() -> Result<(), Box<dyn std::error::Error>> {
    let mut at_registry = HashMap::new();
    atproto::register_theories(&mut at_registry);

    let mut sql_registry = HashMap::new();
    sql::register_theories(&mut sql_registry);

    // The common sub-theory is ThVertex (just the Vertex sort).
    let th_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    let at_graph = at_registry.get("ThGraph").ok_or("ThGraph not found")?;

    // Morphism: ThVertex -> ThGraph (inclusion).
    let sort_map = HashMap::from([("Vertex".into(), "Vertex".into())]);
    let inclusion_at = TheoryMorphism::new(
        "include_at",
        "ThVertex",
        "ThGraph",
        sort_map.clone(),
        HashMap::new(),
    );
    check_morphism(&inclusion_at, &th_vertex, at_graph)?;

    // Morphism: ThVertex -> ThHypergraph (inclusion).
    let sql_hypergraph = sql_registry
        .get("ThHypergraph")
        .ok_or("ThHypergraph not found")?;
    let inclusion_sql = TheoryMorphism::new(
        "include_sql",
        "ThVertex",
        "ThHypergraph",
        sort_map,
        HashMap::new(),
    );
    check_morphism(&inclusion_sql, &th_vertex, sql_hypergraph)?;

    Ok(())
}

#[test]
fn cross_protocol_colimit() -> Result<(), Box<dyn std::error::Error>> {
    // Compute the colimit of ATProto's ThGraph and SQL's ThHypergraph
    // over the shared ThVertex. This gives a "universal" schema theory
    // that subsumes both.
    let mut at_registry = HashMap::new();
    atproto::register_theories(&mut at_registry);

    let mut sql_registry = HashMap::new();
    sql::register_theories(&mut sql_registry);

    let th_graph = at_registry.get("ThGraph").ok_or("ThGraph")?;
    let th_hypergraph = sql_registry.get("ThHypergraph").ok_or("ThHypergraph")?;

    let shared = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    let universal = colimit(th_graph, th_hypergraph, &shared)?;

    // The universal theory should have sorts from both.
    assert!(
        universal.find_sort("Vertex").is_some(),
        "should have Vertex"
    );
    assert!(
        universal.find_sort("Edge").is_some(),
        "should have Edge from ThGraph"
    );
    assert!(
        universal.find_sort("HyperEdge").is_some(),
        "should have HyperEdge from ThHypergraph"
    );
    assert!(
        universal.find_sort("Label").is_some(),
        "should have Label from ThHypergraph"
    );

    // And operations from both.
    assert!(
        universal.find_op("src").is_some(),
        "should have src from ThGraph"
    );
    assert!(
        universal.find_op("tgt").is_some(),
        "should have tgt from ThGraph"
    );
    assert!(
        universal.find_op("incident").is_some(),
        "should have incident from ThHypergraph"
    );

    Ok(())
}

#[test]
fn both_protocols_parse_successfully() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that real schemas can be built with both protocols.
    let at_lexicon = serde_json::json!({
        "lexicon": 1,
        "id": "test.record",
        "defs": {
            "main": {
                "type": "record",
                "record": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    }
                }
            }
        }
    });
    let at_schema = atproto::parse_lexicon(&at_lexicon)?;
    assert!(
        at_schema.has_vertex("test.record"),
        "ATProto schema should parse"
    );

    let sql_ddl = "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT);";
    let sql_schema = sql::parse_ddl(sql_ddl)?;
    assert!(sql_schema.has_vertex("test"), "SQL schema should parse");

    // Both schemas have a vertex named with their respective conventions.
    // The common structure is that both have vertices and edges.
    assert!(
        !at_schema.vertices.is_empty(),
        "ATProto schema has vertices"
    );
    assert!(!sql_schema.vertices.is_empty(), "SQL schema has vertices");

    Ok(())
}
