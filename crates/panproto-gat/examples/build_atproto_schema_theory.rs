//! Build the AT Protocol schema theory from its component GATs.

use panproto_gat::{Sort, Theory, colimit_by_name};
use panproto_protocols::theories;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let th_graph = theories::th_graph();
    let th_constraint = theories::th_constraint();
    let th_multi = theories::th_multi();

    let shared_vertex = Theory::new("ThVertex", vec![Sort::simple("Vertex")], vec![], vec![]);
    let gc = colimit_by_name(&th_graph, &th_constraint, &shared_vertex)?;
    println!(
        "ThGraph + ThConstraint: {} sorts, {} ops",
        gc.sorts.len(),
        gc.ops.len()
    );

    let shared_ve = Theory::new(
        "ThVertexEdge",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![],
        vec![],
    );
    let mut schema_theory = colimit_by_name(&gc, &th_multi, &shared_ve)?;
    schema_theory.name = "ThATProtoSchema".into();

    println!(
        "ThATProtoSchema: {} sorts, {} ops, {} equations",
        schema_theory.sorts.len(),
        schema_theory.ops.len(),
        schema_theory.eqs.len()
    );
    Ok(())
}
