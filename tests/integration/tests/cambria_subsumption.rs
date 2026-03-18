//! Integration test 11: Protolens subsumption.
//!
//! Verifies that protolens elementary constructors (rename, add, remove)
//! subsume Cambria-style operations within panproto's lens framework.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, Node, Value, WInstance};
use panproto_lens::{Lens, check_laws, get, put};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a schema from vertices and edges.
fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in edge_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
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

/// Build an identity lens for the schema.
fn identity_lens(schema: &Schema) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

#[test]
fn protolens_rename_sort_is_representable() -> Result<(), Box<dyn std::error::Error>> {
    // Elementary protolens: rename_sort("text", "content")
    // This is what Cambria called RenameField.
    let _ = panproto_lens::elementary::rename_sort("string", "text_type");

    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };
    let edge_content = Edge {
        src: "body".into(),
        tgt: "body.content".into(),
        kind: "prop".into(),
        name: Some("content".into()),
    };

    let src_schema = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge_text),
    );
    let tgt_schema = make_schema(
        &[("body", "object"), ("body.content", "string")],
        std::slice::from_ref(&edge_content),
    );

    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: tgt_schema.vertices.keys().cloned().collect(),
            surviving_edges: tgt_schema.edges.keys().cloned().collect(),
            vertex_remap: HashMap::from([
                ("body".into(), "body".into()),
                ("body.text".into(), "body.content".into()),
            ]),
            edge_remap: HashMap::from([(edge_text, edge_content)]),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        },
        src_schema,
        tgt_schema,
    };

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "body"));
    nodes.insert(
        1,
        Node::new(1, "body.text").with_value(FieldPresence::Present(Value::Str("hello".into()))),
    );

    let instance = WInstance::new(
        nodes,
        vec![(
            0,
            1,
            Edge {
                src: "body".into(),
                tgt: "body.text".into(),
                kind: "prop".into(),
                name: Some("text".into()),
            },
        )],
        vec![],
        0,
        "body".into(),
    );

    let (view, _complement) = get(&lens, &instance)?;
    assert!(view.node_count() >= 1, "view should have at least the root");

    Ok(())
}

#[test]
fn protolens_drop_sort_is_projection() -> Result<(), Box<dyn std::error::Error>> {
    // Elementary protolens: drop_sort("string") for the deprecated field.
    let _ = panproto_lens::elementary::drop_sort("string");

    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };
    let edge_dep = Edge {
        src: "body".into(),
        tgt: "body.deprecated".into(),
        kind: "prop".into(),
        name: Some("deprecated".into()),
    };

    let src_schema = make_schema(
        &[
            ("body", "object"),
            ("body.text", "string"),
            ("body.deprecated", "string"),
        ],
        &[edge_text.clone(), edge_dep],
    );
    let tgt_schema = make_schema(
        &[("body", "object"), ("body.text", "string")],
        std::slice::from_ref(&edge_text),
    );

    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: tgt_schema.vertices.keys().cloned().collect(),
            surviving_edges: tgt_schema.edges.keys().cloned().collect(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        },
        src_schema,
        tgt_schema,
    };

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "body"));
    nodes.insert(
        1,
        Node::new(1, "body.text").with_value(FieldPresence::Present(Value::Str("keep".into()))),
    );
    nodes.insert(
        2,
        Node::new(2, "body.deprecated")
            .with_value(FieldPresence::Present(Value::Str("gone".into()))),
    );

    let instance = WInstance::new(
        nodes,
        vec![
            (0, 1, edge_text),
            (
                0,
                2,
                Edge {
                    src: "body".into(),
                    tgt: "body.deprecated".into(),
                    kind: "prop".into(),
                    name: Some("deprecated".into()),
                },
            ),
        ],
        vec![],
        0,
        "body".into(),
    );

    let (view, complement) = get(&lens, &instance)?;

    assert!(
        view.node_count() <= 2,
        "view should have at most 2 nodes (root + text)"
    );

    assert!(
        !complement.dropped_nodes.is_empty(),
        "complement should track dropped nodes"
    );

    let restored = put(&lens, &view, &complement)?;
    assert_eq!(
        restored.node_count(),
        instance.node_count(),
        "put should restore all nodes"
    );

    Ok(())
}

#[test]
fn protolens_elementary_constructors_exist() {
    // Verify that all elementary protolens constructors are accessible
    // and produce valid Protolens values.
    let add = panproto_lens::elementary::add_sort("tags", "array", Value::Null);
    let drop_s = panproto_lens::elementary::drop_sort("deprecated");
    let rename_s = panproto_lens::elementary::rename_sort("old", "new");
    let add_op = panproto_lens::elementary::add_op("link", "src", "tgt", "ref");
    let drop_op = panproto_lens::elementary::drop_op("obsolete");
    let rename_op = panproto_lens::elementary::rename_op("src", "source");

    // Renames are lossless; adds require defaults; drops capture data
    assert!(!add.is_lossless()); // requires default value
    assert!(!drop_s.is_lossless());
    assert!(rename_s.is_lossless());
    assert!(!add_op.is_lossless()); // requires default
    assert!(rename_op.is_lossless());
    assert!(!drop_op.is_lossless());
}

#[test]
fn identity_lens_satisfies_laws() -> Result<(), Box<dyn std::error::Error>> {
    let edge = Edge {
        src: "root".into(),
        tgt: "root.field".into(),
        kind: "prop".into(),
        name: Some("field".into()),
    };

    let schema = make_schema(
        &[("root", "object"), ("root.field", "string")],
        std::slice::from_ref(&edge),
    );

    let lens = identity_lens(&schema);

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));
    nodes.insert(
        1,
        Node::new(1, "root.field").with_value(FieldPresence::Present(Value::Str("value".into()))),
    );

    let instance = WInstance::new(nodes, vec![(0, 1, edge)], vec![], 0, "root".into());

    check_laws(&lens, &instance)?;

    Ok(())
}

#[test]
fn protolens_chain_composition() {
    // Verify that protolens chains compose via vertical_compose.
    let p1 = panproto_lens::elementary::rename_sort("string", "text");
    let p2 = panproto_lens::elementary::add_sort("tags", "array", Value::Null);
    let composed = panproto_lens::protolens_vertical(&p1, &p2)
        .unwrap_or_else(|e| panic!("vertical compose failed: {e}"));
    assert!(composed.name.contains('.'));
}
