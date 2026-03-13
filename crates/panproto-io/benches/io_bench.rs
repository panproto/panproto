//! Throughput benchmarks for panproto-io.
//!
//! Measures parse/emit speed in MB/s per protocol using real fixture data.

fn main() {
    divan::main();
}

#[divan::bench]
fn json_pathway_parse_small(bencher: divan::Bencher<'_, '_>) {
    let input = br#"{"name": "Alice", "age": 30}"#;
    let proto = panproto_schema::Protocol {
        name: "test".into(),
        schema_theory: "ThTestSchema".into(),
        instance_theory: "ThTestInstance".into(),
        edge_rules: vec![],
        obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
        constraint_sorts: vec![],
    };
    let schema = panproto_schema::SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("v")
        .vertex("root:name", "string", None)
        .expect("v")
        .vertex("root:age", "integer", None)
        .expect("v")
        .edge("root", "root:name", "prop", Some("name"))
        .expect("e")
        .edge("root", "root:age", "prop", Some("age"))
        .expect("e")
        .build()
        .expect("build");

    bencher.bench_local(|| {
        panproto_io::json_pathway::parse_json_bytes(&schema, "root", input, "test")
            .expect("parse")
    });
}
