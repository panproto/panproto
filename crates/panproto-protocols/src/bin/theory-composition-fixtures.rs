//! Regenerate the frozen theory composition corpus.
//!
//! Each fixture is one diagram, the options it is composed with, and the
//! [`Composition`] that [`compose()`] returns, written three times: as named
//! `MessagePack` (`<name>.named.msgpack`), as CBOR (`<name>.cbor`) and as
//! JSON (`<name>.json`). `golden.json` records each composition's
//! [`composition_digest`] in hex under the context it is derived with.
//! The diagrams cover every shape group: the registration diagrams of the
//! tree, table, flat and graph groups, each group's schema theory composed
//! with its instance theory over the interface they share, and one
//! composition that freshens clashing names apart.
//!
//! Usage: `theory-composition-fixtures [OUT_DIR]`, where `OUT_DIR` defaults
//! to the repository's `tests/fixtures/theory_composition`. The
//! registration equivalence tests run this binary into a temporary
//! directory and fail when its output differs from the committed corpus.

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, Composition, CompositionMode, DEFAULT_VERIFY_BUDGET, Operation,
    Sort, Theory, TheoryDiagram, compose,
};
use panproto_protocols::theories::{
    RegistrationDiagram, constrained_hypergraph_diagram, constrained_multigraph_diagram,
    constrained_simple_graph_diagram, register_constrained_graph_instance,
    register_constrained_multigraph_wtype, register_hypergraph_functor,
    register_multigraph_wtype_meta, register_simple_graph_flat, th_flat, th_functor,
    th_graph_instance, th_wtype, typed_graph_diagram, wtype_meta_diagram,
};
use panproto_schema::{COMPOSITION_CONTEXT, composition_digest};
use serde::Serialize;

/// One fixture as it is written to disk.
#[derive(Serialize)]
struct Fixture<'a> {
    diagram: &'a TheoryDiagram,
    options: &'a ComposeOptions,
    composition: &'a Composition,
}

/// The digests file.
#[derive(Serialize)]
struct Golden {
    context: &'static str,
    digests: BTreeMap<String, String>,
}

/// The interface a tree or graph instance theory shares with its schema
/// theory.
fn vertex_edge_interface() -> Theory {
    Theory::new(
        "Interface",
        vec![Sort::simple("Vertex"), Sort::simple("Edge")],
        vec![
            Operation::unary("src", "e", "Edge", "Vertex"),
            Operation::unary("tgt", "e", "Edge", "Vertex"),
        ],
        vec![],
    )
}

/// The interface a table or flat instance theory shares with its schema
/// theory.
fn vertex_interface() -> Theory {
    Theory::new("Interface", vec![Sort::simple("Vertex")], vec![], vec![])
}

/// The schema theory `register` registers, renamed `Schema`.
fn schema_of(
    register: fn(&mut HashMap<String, Theory>, &str, &str),
    tag: &str,
) -> Result<Theory, Box<dyn Error>> {
    let mut registry = HashMap::new();
    let name = format!("ThFixture{tag}Schema");
    register(&mut registry, &name, &format!("ThFixture{tag}Instance"));
    let mut schema = registry
        .remove(&name)
        .ok_or_else(|| format!("{name} was not registered"))?;
    schema.name = "Schema".into();
    Ok(schema)
}

/// `instance` composed with `schema` over `interface`.
fn schema_and_instance(interface: Theory, schema: Theory, instance: Theory) -> RegistrationDiagram {
    let targets = vec![Arc::clone(&schema.name), Arc::clone(&instance.name)];
    RegistrationDiagram {
        blocks: vec![schema, instance],
        shared: vec![(interface, targets)],
    }
}

/// One fixture's name, diagram and options.
type Entry = (&'static str, TheoryDiagram, ComposeOptions);

/// Every fixture: its name, diagram and options.
fn corpus() -> Result<Vec<Entry>, Box<dyn Error>> {
    let registration = |name: &'static str, diagram: RegistrationDiagram| {
        (name, diagram.diagram(), RegistrationDiagram::options(name))
    };
    let mut fixtures = vec![
        registration(
            "tree_schema_constrained_multigraph",
            constrained_multigraph_diagram(),
        ),
        registration("tree_schema_typed_graph", typed_graph_diagram()),
        registration(
            "tree_instance_wtype",
            RegistrationDiagram::single(th_wtype()),
        ),
        registration(
            "tree_instance_wtype_meta",
            wtype_meta_diagram("ThNodeValue"),
        ),
        registration(
            "table_schema_constrained_hypergraph",
            constrained_hypergraph_diagram(),
        ),
        registration(
            "table_instance_functor",
            RegistrationDiagram::single(th_functor()),
        ),
        registration(
            "flat_schema_constrained_simple_graph",
            constrained_simple_graph_diagram(),
        ),
        registration("flat_instance_flat", RegistrationDiagram::single(th_flat())),
        registration(
            "graph_instance_graph_instance",
            RegistrationDiagram::single(th_graph_instance()),
        ),
        registration(
            "tree_schema_and_instance",
            schema_and_instance(
                vertex_edge_interface(),
                schema_of(register_constrained_multigraph_wtype, "A")?,
                th_wtype(),
            ),
        ),
        registration(
            "tree_schema_and_instance_with_metadata",
            schema_and_instance(
                vertex_edge_interface(),
                schema_of(register_multigraph_wtype_meta, "E")?,
                wtype_meta_diagram("ThNodeValue")
                    .compose("ThWTypeMeta")?
                    .theory,
            ),
        ),
        registration(
            "table_schema_and_instance",
            schema_and_instance(
                vertex_interface(),
                schema_of(register_hypergraph_functor, "B")?,
                th_functor(),
            ),
        ),
        registration(
            "flat_schema_and_instance",
            schema_and_instance(
                vertex_interface(),
                schema_of(register_simple_graph_flat, "C")?,
                th_flat(),
            ),
        ),
        registration(
            "graph_schema_and_instance",
            schema_and_instance(
                vertex_edge_interface(),
                schema_of(register_constrained_graph_instance, "F")?,
                th_graph_instance(),
            ),
        ),
    ];
    // The constrained multigraph without Edge shared: the two Edge sorts
    // clash and are freshened apart.
    let mut unshared = constrained_multigraph_diagram();
    unshared.shared[1].0 =
        Theory::new("ThVertexEdge", vec![Sort::simple("Vertex")], vec![], vec![]);
    fixtures.push((
        "freshened_edges",
        unshared.diagram(),
        ComposeOptions {
            mode: CompositionMode::ExplicitIdentity,
            result_name: "freshened_edges".into(),
            clash: ClashPolicy::Freshen,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        },
    ));
    Ok(fixtures)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0xf)]));
    }
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = std::env::args_os().nth(1).map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/theory_composition")
        },
        PathBuf::from,
    );
    std::fs::create_dir_all(&out)?;
    let mut digests = BTreeMap::new();
    for (name, diagram, options) in corpus()? {
        let composition =
            compose(&diagram, &options).map_err(|e| format!("fixture {name}: {e}"))?;
        let fixture = Fixture {
            diagram: &diagram,
            options: &options,
            composition: &composition,
        };
        std::fs::write(
            out.join(format!("{name}.named.msgpack")),
            rmp_serde::to_vec_named(&fixture)?,
        )?;
        let mut cbor = Vec::new();
        ciborium::into_writer(&fixture, &mut cbor)?;
        std::fs::write(out.join(format!("{name}.cbor")), cbor)?;
        let mut json = serde_json::to_string_pretty(&fixture)?;
        json.push('\n');
        std::fs::write(out.join(format!("{name}.json")), json)?;
        digests.insert(name.to_owned(), hex(&composition_digest(&composition)));
    }
    let mut golden = serde_json::to_string_pretty(&Golden {
        context: COMPOSITION_CONTEXT,
        digests,
    })?;
    golden.push('\n');
    std::fs::write(out.join("golden.json"), golden)?;
    Ok(())
}
