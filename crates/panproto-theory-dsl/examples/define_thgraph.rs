//! Parse a declarative `ThGraph` theory from YAML.

use panproto_theory_dsl::eval;

const SRC: &str = r#"{
  "id": "dev.panproto.theories.th-graph",
  "description": "Graph theory: vertices, edges, source and target",
  "theory": "ThGraph",
  "sorts": [{"name": "Vertex"}, {"name": "Edge"}]
}"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = eval::eval_json(SRC)?;
    println!("loaded theory doc: {}", doc.id);
    println!("description: {}", doc.description);
    Ok(())
}
