//! Parse the real OpenTelemetry trace.proto using the bundled protobuf grammar.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src = include_bytes!("../../../fixtures/protobuf/trace.proto");
    let grammars = panproto_grammars::grammars();
    let grammar = grammars
        .iter()
        .find(|g| g.name == "protobuf")
        .ok_or("protobuf grammar not available")?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar.language)?;
    let tree = parser.parse(src, None).ok_or("parse failed")?;
    println!("root node kind: {}", tree.root_node().kind());
    println!("root node child count: {}", tree.root_node().child_count());
    println!("byte range: {:?}", tree.root_node().byte_range());
    Ok(())
}
