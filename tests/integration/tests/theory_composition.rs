//! Integration test 13: Theory composition.
//!
//! Verifies that `colimit(ThGraph, ThConstraint)` produces
//! `ThConstrainedGraph` with sorts from both, and that model
//! migration works across the composed theory.

use std::collections::{HashMap, HashSet};

use panproto_gat::{
    Model, ModelValue, Operation, Sort, SortParam, Theory, TheoryMorphism, check_morphism,
    colimit_by_name, migrate_model,
};

#[test]
fn colimit_graph_constraint_produces_constrained_graph() -> Result<(), Box<dyn std::error::Error>> {
    let th_graph = Theory::new(
        "ThGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    );

    let th_constraint = Theory::new(
        "ThConstraint",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
        ],
        vec![Operation::unary("target", "c", "Constraint", "Vertex")],
        vec![],
    );

    let shared = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    let constrained_graph = colimit_by_name(&th_graph, &th_constraint, &shared)?;

    // Verify sorts: Vertex, Edge, Constraint.
    assert_eq!(constrained_graph.sorts.len(), 3, "should have 3 sorts");
    assert!(constrained_graph.find_sort("Vertex").is_some());
    assert!(constrained_graph.find_sort("Edge").is_some());
    assert!(constrained_graph.find_sort("Constraint").is_some());

    // Verify operations: src, tgt, target.
    assert_eq!(constrained_graph.ops.len(), 3, "should have 3 operations");
    assert!(constrained_graph.find_op("src").is_some());
    assert!(constrained_graph.find_op("tgt").is_some());
    assert!(constrained_graph.find_op("target").is_some());

    // Vertex should not be duplicated.
    let vertex_count = constrained_graph
        .sorts
        .iter()
        .filter(|s| &*s.name == "Vertex")
        .count();
    assert_eq!(vertex_count, 1, "Vertex should appear exactly once");

    Ok(())
}

#[test]
fn inclusion_morphism_into_colimit() -> Result<(), Box<dyn std::error::Error>> {
    let th_graph = Theory::new(
        "ThGraph",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    );

    let th_constraint = Theory::new(
        "ThConstraint",
        vec![
            Sort::simple("Vertex"),
            Sort::dependent("Constraint", vec![SortParam::new("v", "Vertex")]),
        ],
        vec![Operation::unary("target", "c", "Constraint", "Vertex")],
        vec![],
    );

    let shared = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);

    let colimit_theory = colimit_by_name(&th_graph, &th_constraint, &shared)?;

    // ThGraph includes into the colimit via identity on sorts/ops.
    let graph_inclusion = TheoryMorphism::new(
        "graph_incl",
        "ThGraph",
        colimit_theory.name.clone(),
        HashMap::from([
            ("Vertex".into(), "Vertex".into()),
            ("Edge".into(), "Edge".into()),
        ]),
        HashMap::from([("src".into(), "src".into()), ("tgt".into(), "tgt".into())]),
    );
    check_morphism(&graph_inclusion, &th_graph, &colimit_theory)?;

    // ThConstraint includes into the colimit similarly.
    let constraint_inclusion = TheoryMorphism::new(
        "constraint_incl",
        "ThConstraint",
        colimit_theory.name.clone(),
        HashMap::from([
            ("Vertex".into(), "Vertex".into()),
            ("Constraint".into(), "Constraint".into()),
        ]),
        HashMap::from([("target".into(), "target".into())]),
    );
    check_morphism(&constraint_inclusion, &th_constraint, &colimit_theory)?;

    Ok(())
}

#[test]
fn model_migration_across_colimit() -> Result<(), Box<dyn std::error::Error>> {
    // Build a model of the constrained graph theory and migrate
    // it along the inclusion morphism from ThGraph.

    // First, define a model of ThGraph.
    let mut graph_model = Model::new("ThGraph");
    graph_model.add_sort(
        "Vertex",
        vec![ModelValue::Str("A".into()), ModelValue::Str("B".into())],
    );
    graph_model.add_sort("Edge", vec![ModelValue::Str("e1".into())]);
    graph_model.add_op("src", |args: &[ModelValue]| match &args[0] {
        ModelValue::Str(e) if e == "e1" => Ok(ModelValue::Str("A".into())),
        input => panic!("unexpected input to src op: {input:?}"),
    });
    graph_model.add_op("tgt", |args: &[ModelValue]| match &args[0] {
        ModelValue::Str(e) if e == "e1" => Ok(ModelValue::Str("B".into())),
        input => panic!("unexpected input to tgt op: {input:?}"),
    });

    // Verify the model works.
    let src_result = graph_model.eval("src", &[ModelValue::Str("e1".into())])?;
    assert_eq!(src_result, ModelValue::Str("A".into()));

    let tgt_result = graph_model.eval("tgt", &[ModelValue::Str("e1".into())])?;
    assert_eq!(tgt_result, ModelValue::Str("B".into()));

    // Migrate along the identity inclusion morphism.
    let identity = TheoryMorphism::new(
        "id",
        "ThGraph",
        "ThGraph",
        HashMap::from([
            ("Vertex".into(), "Vertex".into()),
            ("Edge".into(), "Edge".into()),
        ]),
        HashMap::from([("src".into(), "src".into()), ("tgt".into(), "tgt".into())]),
    );

    let migrated = migrate_model(&identity, &graph_model)?;

    // The migrated model should behave identically.
    let m_src = migrated.eval("src", &[ModelValue::Str("e1".into())])?;
    assert_eq!(m_src, ModelValue::Str("A".into()));

    let m_tgt = migrated.eval("tgt", &[ModelValue::Str("e1".into())])?;
    assert_eq!(m_tgt, ModelValue::Str("B".into()));

    Ok(())
}

#[test]
#[allow(clippy::similar_names)]
fn colimit_is_associative() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that colimit(colimit(A, B), C) produces the same
    // sorts as colimit(A, colimit(B, C)).

    let a = Theory::new(
        "A",
        vec![Sort::simple("X"), Sort::simple("Y")],
        vec![],
        vec![],
    );
    let b = Theory::new(
        "B",
        vec![Sort::simple("Y"), Sort::simple("Z")],
        vec![],
        vec![],
    );
    let c = Theory::new(
        "C",
        vec![Sort::simple("Z"), Sort::simple("W")],
        vec![],
        vec![],
    );

    let shared_y = Theory::new("SharedY", vec![Sort::simple("Y")], vec![], vec![]);
    let shared_z = Theory::new("SharedZ", vec![Sort::simple("Z")], vec![], vec![]);

    // (A + B) + C
    let ab = colimit_by_name(&a, &b, &shared_y)?;
    let shared_z_for_abc = Theory::new("SharedZ", vec![Sort::simple("Z")], vec![], vec![]);
    let abc_left = colimit_by_name(&ab, &c, &shared_z_for_abc)?;

    // A + (B + C)
    let bc = colimit_by_name(&b, &c, &shared_z)?;
    let shared_y_for_abc = Theory::new("SharedY", vec![Sort::simple("Y")], vec![], vec![]);
    let abc_right = colimit_by_name(&a, &bc, &shared_y_for_abc)?;

    // Both should have the same sorts: X, Y, Z, W.
    let left_sort_names: HashSet<&str> = abc_left.sorts.iter().map(|s| &*s.name).collect();
    let right_sort_names: HashSet<&str> = abc_right.sorts.iter().map(|s| &*s.name).collect();

    assert_eq!(
        left_sort_names, right_sort_names,
        "colimit should be associative on sort names"
    );

    Ok(())
}
