//! The edge-lists reference on the examples and refusals the page states,
//! and the closed-form counts of its scopes.

use panproto_reference::enumerate::edge_lists::{
    documents, documents_count, edge_lists, mutations, mutations_count,
};
use panproto_reference::pages::edge_lists::{
    Dialect, Edge, ReadRefusal, WriteRefusal, canonical_text, encode, read_document, write_graph,
};

fn edge(source: &str, target: &str, label: Option<&str>) -> Edge {
    Edge(source.into(), target.into(), label.map(Into::into))
}

#[test]
fn edge_lists_read_records_as_directed_edges_and_keep_parallel_edges_and_loops() {
    assert_eq!(
        read_document("Source,Target,Label\na,b,x\na,b,x\na,a,\n", Dialect::Csv),
        Ok(vec![
            edge("a", "b", Some("x")),
            edge("a", "b", Some("x")),
            edge("a", "a", Some("")),
        ])
    );
    assert_eq!(
        read_document("target,source\r\n\"x,1\",\"y\"\"z\"\r\n\r\n", Dialect::Csv),
        Ok(vec![edge("y\"z", "x,1", None)])
    );
    assert_eq!(
        read_document("\u{FEFF}source\ttarget\n a \t\"b\n\nc\td", Dialect::Tsv),
        Ok(vec![edge(" a ", "\"b", None), edge("c", "d", None)])
    );
    assert_eq!(read_document("source,target", Dialect::Csv), Ok(vec![]));
}

#[test]
fn edge_list_refusals_follow_the_order_the_page_states() {
    let csv = |text: &str| read_document(text, Dialect::Csv);
    assert_eq!(csv(""), Err(ReadRefusal::NoHeader));
    assert_eq!(csv("\nsource,target\n"), Err(ReadRefusal::Table));
    assert_eq!(csv("source,target\n\"a\"b,c\n"), Err(ReadRefusal::Table));
    assert_eq!(csv("source,target\na\rb,c\n"), Err(ReadRefusal::Table));
    assert_eq!(
        csv("source,weight,target\n"),
        Err(ReadRefusal::UnsupportedColumn(1, "weight".into()))
    );
    assert_eq!(
        csv("source,TARGET,Target\n"),
        Err(ReadRefusal::DuplicateColumn(2, "Target".into()))
    );
    assert_eq!(
        csv("label,target\n"),
        Err(ReadRefusal::MissingColumn("source".into()))
    );
    assert_eq!(csv("source,target\n,b\na\n"), Err(ReadRefusal::Table));
    assert_eq!(
        csv("source,target\na,b\na,\"\"\n"),
        Err(ReadRefusal::EmptyName(2, "target".into()))
    );
    assert_eq!(
        read_document("source\ttarget\na\rb\tc\n", Dialect::Tsv),
        Err(ReadRefusal::Table)
    );
}

#[test]
fn the_encoding_numbers_nodes_by_record_and_keeps_every_record() {
    let graph = encode(&[edge("a", "b", Some("l")), edge("b", "b", None)]);
    let nodes: Vec<_> = graph
        .nodes
        .iter()
        .map(|(id, (anchor, value))| (*id, anchor.as_str(), value.as_deref()))
        .collect();
    assert_eq!(
        nodes,
        [
            (0, "edge", Some("l")),
            (1, "node", Some("a")),
            (2, "node", Some("b")),
            (3, "edge", None),
        ]
    );
    assert_eq!(
        graph.edges,
        [
            (0, 1, "source".into()),
            (0, 2, "target".into()),
            (3, 2, "source".into()),
            (3, 2, "target".into()),
        ]
    );
}

#[test]
fn canonical_edge_lists_quote_and_refuse_as_the_page_states() {
    let edges = [edge("x,y", "a", Some("")), edge("a", "a", Some("q\"r"))];
    assert_eq!(
        canonical_text(&edges, Dialect::Csv).as_deref(),
        Some("source,target,label\n\"x,y\",a,\"\"\na,a,\"q\"\"r\"\n")
    );
    assert_eq!(
        canonical_text(&edges, Dialect::Tsv).as_deref(),
        Some("source\ttarget\tlabel\nx,y\ta\t\na\ta\tq\"r\n")
    );
    assert_eq!(
        canonical_text(&[], Dialect::Tsv).as_deref(),
        Some("source\ttarget\n")
    );
    assert_eq!(
        canonical_text(
            &[edge("a", "b", None), edge("a", "b", Some("l"))],
            Dialect::Csv
        ),
        None
    );
    assert_eq!(
        canonical_text(&[edge("a", "b", Some("m\nn"))], Dialect::Tsv),
        None
    );
}

#[test]
fn the_writer_refuses_what_no_edge_list_reads_back_as() {
    let graph = encode(&[edge("a", "b", None)]);
    assert_eq!(
        write_graph(&graph, Dialect::Csv).as_deref(),
        Ok("source,target\na,b\n")
    );
    let mut isolated = graph.clone();
    isolated.nodes.insert(3, ("node".into(), Some("z".into())));
    assert_eq!(
        write_graph(&isolated, Dialect::Csv),
        Err((3, WriteRefusal::Isolated))
    );
    let mut duplicate = isolated.clone();
    duplicate.nodes.insert(3, ("node".into(), Some("a".into())));
    assert_eq!(
        write_graph(&duplicate, Dialect::Csv),
        Err((3, WriteRefusal::DuplicateNode(1)))
    );
    let mut parallel = graph.clone();
    parallel.nodes.insert(3, ("edge".into(), Some("l".into())));
    parallel
        .edges
        .extend([(3, 1, "source".into()), (3, 2, "target".into())]);
    assert_eq!(
        write_graph(&parallel, Dialect::Csv),
        Err((3, WriteRefusal::MixedLabels(0)))
    );
    let mut dropped = graph.clone();
    dropped.edges.remove(1);
    assert_eq!(
        write_graph(&dropped, Dialect::Csv),
        Err((0, WriteRefusal::EdgeCount("target".into())))
    );
    let tab = encode(&[edge("t\tu", "a", None)]);
    assert_eq!(
        write_graph(&tab, Dialect::Tsv),
        Err((1, WriteRefusal::UnwritableCharacter('\t')))
    );
}

#[test]
fn the_edge_list_scopes_have_their_closed_form_counts() {
    for (dialect, count) in [(Dialect::Csv, 32_761), (Dialect::Tsv, 21_547)] {
        assert_eq!(documents_count(dialect), count);
        assert_eq!(documents(dialect).len(), count);
    }
    let lists = edge_lists();
    assert_eq!(lists.len(), 73);
    let graphs: usize = lists
        .iter()
        .map(|list| mutations(&encode(list)).len())
        .sum();
    let counted: usize = lists
        .iter()
        .map(|list| mutations_count(&encode(list)))
        .sum();
    assert_eq!(graphs, counted);
    assert_eq!(graphs, 1_387);
}
