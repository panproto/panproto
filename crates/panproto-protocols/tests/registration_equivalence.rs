//! Protocol theory registration composes each theory as one diagram with
//! explicit identity and gives the theory the 0.74 amalgamated
//! construction gave.
//!
//! For every registered protocol the schema theory's canonical bytes equal
//! those of the 0.74 construction (a chain of pushouts by name), and the
//! instance theory's equal those of the 0.74 construction over the closed
//! instance building blocks. The protocols whose instance theory the shape
//! declarations re-declare are held to their declared shapes instead, and
//! protocols registered after 0.74 to their declarations. Every
//! registration report is free of name clashes, passes, and has no failed,
//! inapplicable or incomplete check.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};

use panproto_gat::{
    CheckOutcome, CheckStatus, ClashPolicy, Composition, CompositionMode, CompositionStep, SymKind,
    SymRef, Theory, pushout_by_name, recompose,
};
use panproto_protocols::registry::descriptors;
use panproto_protocols::semantics::shapes;
use panproto_protocols::theories::{
    RegistrationDiagram, constrained_hypergraph_diagram, constrained_multigraph_diagram,
    constrained_simple_graph_diagram, hypergraph_functor_specs, registered_composition,
    registered_recipe, th_constraint, th_flat, th_functor, th_graph, th_graph_instance,
    th_hypergraph, th_interface, th_meta, th_multi, th_simple_graph, th_wtype, typed_graph_diagram,
    wtype_meta_diagram,
};
use panproto_schema::InstanceShapeKind::{self, Graph, Table, Tree};
use panproto_schema::canonical_theory_bytes;

/// The registration group a 0.74 protocol used.
#[derive(Clone, Copy, Debug)]
enum Group {
    /// Constrained multigraph and W-type.
    A,
    /// Hypergraph and functor.
    B,
    /// Simple graph and flat.
    C,
    /// Typed graph and W-type.
    D,
    /// Constrained multigraph, W-type and metadata.
    E,
    /// Constrained multigraph and graph instance.
    F,
    /// `ATProto`'s own pair: the constrained multigraph, and W-type and
    /// metadata over `Node` alone.
    Atproto,
}

/// Every protocol registered in 0.74 with the group it registered through.
const ZERO_74: &[(&str, Group)] = &[
    ("amr", Group::D),
    ("ansible", Group::A),
    ("arrow", Group::B),
    ("asn1", Group::C),
    ("asyncapi", Group::A),
    ("atproto", Group::Atproto),
    ("avro", Group::C),
    ("bead", Group::A),
    ("bond", Group::C),
    ("brat", Group::A),
    ("bson", Group::A),
    ("cassandra", Group::B),
    ("cddl", Group::A),
    ("cloudformation", Group::A),
    ("concrete", Group::A),
    ("conllu", Group::B),
    ("dataframe", Group::B),
    ("decomp", Group::A),
    ("docx", Group::E),
    ("dynamodb", Group::B),
    ("edi-x12", Group::B),
    ("elan", Group::E),
    ("fhir", Group::A),
    ("flatbuffers", Group::C),
    ("folia", Group::E),
    ("fovea", Group::D),
    ("geojson", Group::A),
    ("graphql", Group::D),
    ("iso-space", Group::A),
    ("json-schema", Group::A),
    ("jsonapi", Group::A),
    ("k8s-crd", Group::A),
    ("laf-graf", Group::A),
    ("mongodb", Group::A),
    ("msgpack-schema", Group::A),
    ("naf", Group::B),
    ("neo4j", Group::F),
    ("nif", Group::A),
    ("odf", Group::E),
    ("openapi", Group::A),
    ("parquet", Group::B),
    ("paula", Group::A),
    ("protobuf", Group::C),
    ("raml", Group::A),
    ("redis", Group::C),
    ("rss-atom", Group::A),
    ("sql", Group::B),
    ("swift-mt", Group::B),
    ("tei", Group::E),
    ("timeml", Group::A),
    ("ucca", Group::D),
    ("uima", Group::D),
    ("vcard-ical", Group::A),
    ("web-annotation", Group::A),
];

/// The protocols whose instance theory the shape declarations re-declare,
/// with their declared syntactic and semantic shapes. Their registration
/// is held to the declaration, not to the 0.74 bytes.
const REDECLARED: &[(&str, InstanceShapeKind, InstanceShapeKind)] = &[
    ("naf", Tree, Graph),
    ("cassandra", Tree, Table),
    ("dynamodb", Tree, Table),
    ("neo4j", Tree, Graph),
    ("avro", Tree, Tree),
    ("protobuf", Tree, Tree),
    ("flatbuffers", Tree, Tree),
    ("asn1", Tree, Tree),
    ("bond", Tree, Tree),
    ("redis", Table, Table),
];

/// Protocols with no 0.74 registration to compare against.
const NEW_IN_0_75: &[&str] = &[
    "amr-triples",
    "csv",
    "tsv",
    "yaml",
    "toml",
    "ntriples",
    "nquads",
    "turtle",
    "jsonld",
    "edgelist",
];

fn shared(name: &str, sorts: &[&str]) -> Theory {
    Theory::new(
        name,
        sorts
            .iter()
            .map(|s| panproto_gat::Sort::simple(*s))
            .collect(),
        vec![],
        vec![],
    )
}

fn pushout(t1: &Theory, t2: &Theory, over: &Theory) -> Theory {
    pushout_by_name(t1, t2, over).unwrap().theory
}

/// `ThGraph + ThConstraint + ThMulti` as 0.74 built it.
fn constrained_multigraph_0_74() -> Theory {
    let gc = pushout(
        &th_graph(),
        &th_constraint(),
        &shared("ThVertex", &["Vertex"]),
    );
    pushout(
        &gc,
        &th_multi(),
        &shared("ThVertexEdge", &["Vertex", "Edge"]),
    )
}

/// The 0.74 construction of a group's schema and instance theories, over
/// the closed instance building blocks.
fn reference(group: Group) -> (Theory, Theory) {
    match group {
        Group::A => (constrained_multigraph_0_74(), th_wtype()),
        Group::B => (
            pushout(
                &th_hypergraph(),
                &th_constraint(),
                &shared("ThVertex", &["Vertex"]),
            ),
            th_functor(),
        ),
        Group::C => (
            pushout(
                &th_simple_graph(),
                &th_constraint(),
                &shared("ThVertex", &["Vertex"]),
            ),
            th_flat(),
        ),
        Group::D => (
            pushout(
                &constrained_multigraph_0_74(),
                &th_interface(),
                &shared("ThVertex2", &["Vertex"]),
            ),
            th_wtype(),
        ),
        Group::E => (
            constrained_multigraph_0_74(),
            pushout(
                &th_wtype(),
                &th_meta(),
                &shared("ThNodeValue", &["Node", "Value"]),
            ),
        ),
        Group::F => (constrained_multigraph_0_74(), th_graph_instance()),
        Group::Atproto => (
            constrained_multigraph_0_74(),
            pushout(&th_wtype(), &th_meta(), &shared("ThNode", &["Node"])),
        ),
    }
}

/// Canonical bytes of `theory` under the name `name`.
fn bytes_as(theory: &Theory, name: &str) -> Vec<u8> {
    let mut named = theory.clone();
    named.name = name.into();
    canonical_theory_bytes(&named)
}

/// A registration report is clash free, passes, and has no failed,
/// inapplicable or incomplete check.
fn assert_status_clause(composition: &Composition, what: &str) {
    let report = &composition.report;
    assert!(report.name_clash_free, "{what}: name clash");
    assert_eq!(report.status, CheckStatus::Passed, "{what}");
    for record in &report.checks {
        assert!(
            matches!(
                record.outcome,
                CheckOutcome::Passed | CheckOutcome::PassedByEquation { .. }
            ),
            "{what}: {record:?}"
        );
    }
}

/// Register one protocol's theories into a fresh registry.
fn register(name: &str) -> (panproto_schema::Protocol, HashMap<String, Theory>) {
    let descriptor = descriptors()
        .iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("{name} is registered"));
    let mut registry = HashMap::new();
    (descriptor.register_theories)(&mut registry).unwrap();
    ((descriptor.protocol)(), registry)
}

#[test]
fn registration_equivalence() {
    let zero_74: HashMap<&str, Group> = ZERO_74.iter().copied().collect();
    for descriptor in descriptors() {
        let name = descriptor.name;
        let (protocol, registry) = register(name);
        let schema = &registry[&protocol.schema_theory];
        let instance = &registry[&protocol.instance_theory];
        for theory_name in [&protocol.schema_theory, &protocol.instance_theory] {
            let composition = registered_composition(theory_name)
                .unwrap_or_else(|| panic!("{name}: {theory_name} has no composition"));
            assert_status_clause(&composition, &format!("{name}: {theory_name}"));
        }

        if let Some((_, syntactic, semantic)) = REDECLARED
            .iter()
            .find(|(redeclared, ..)| *redeclared == name)
        {
            let row =
                shapes::declared(name).unwrap_or_else(|| panic!("{name} declares its shapes"));
            assert_eq!(
                (row.syntactic, row.semantic),
                (*syntactic, *semantic),
                "{name}"
            );
            assert_eq!(protocol.syntactic_shape, *syntactic, "{name}");
            assert_eq!(protocol.semantic_shape, *semantic, "{name}");
            // Only the instance theory is re-declared: the schema theory
            // is still held to the 0.74 construction.
            let group = zero_74
                .get(name)
                .unwrap_or_else(|| panic!("{name} is a 0.74 protocol"));
            let (schema_ref, _) = reference(*group);
            assert_eq!(
                canonical_theory_bytes(schema),
                bytes_as(&schema_ref, &protocol.schema_theory),
                "{name}: schema theory"
            );
        } else if let Some(group) = zero_74.get(name) {
            let (schema_ref, instance_ref) = reference(*group);
            assert_eq!(
                canonical_theory_bytes(schema),
                bytes_as(&schema_ref, &protocol.schema_theory),
                "{name}: schema theory"
            );
            assert_eq!(
                canonical_theory_bytes(instance),
                bytes_as(&instance_ref, &protocol.instance_theory),
                "{name}: instance theory"
            );
        } else {
            assert!(
                NEW_IN_0_75.contains(&name),
                "{name} is neither a 0.74 protocol nor listed as new in 0.75"
            );
            assert!(
                shapes::declared(name).is_some(),
                "{name} declares its shapes"
            );
        }
    }
}

#[test]
fn every_registered_protocol_records_a_diagram_step() {
    for descriptor in descriptors() {
        let (protocol, _) = register(descriptor.name);
        for (theory_name, carried) in [
            (&protocol.schema_theory, &protocol.schema_composition),
            (&protocol.instance_theory, &protocol.instance_composition),
        ] {
            let recipe = carried
                .clone()
                .unwrap_or_else(|| panic!("{}: {theory_name} has no recipe", descriptor.name));
            assert_eq!(
                Some(&recipe),
                registered_recipe(theory_name).as_ref(),
                "{theory_name}: the protocol carries the recipe registration recorded"
            );
            assert_eq!(recipe.result_name, *theory_name);
            assert_eq!(recipe.version, 1, "{theory_name}");
            assert!(
                matches!(
                    recipe.steps.as_slice(),
                    [CompositionStep::Diagram {
                        mode: CompositionMode::ExplicitIdentity,
                        clash: ClashPolicy::Reject,
                        ..
                    }]
                ),
                "{theory_name}: {:?}",
                recipe.steps
            );
        }
    }
}

#[test]
fn recompose_of_every_registration_spec_reproduces_its_theory() {
    for descriptor in descriptors() {
        let (protocol, registry) = register(descriptor.name);
        for (theory_name, carried) in [
            (&protocol.schema_theory, &protocol.schema_composition),
            (&protocol.instance_theory, &protocol.instance_composition),
        ] {
            let recipe = carried
                .clone()
                .unwrap_or_else(|| panic!("{}: {theory_name} has no recipe", descriptor.name));
            let json = serde_json::to_string(&recipe).unwrap();
            let stored: panproto_gat::CompositionSpec = serde_json::from_str(&json).unwrap();
            let replayed =
                recompose(&stored, &registry).unwrap_or_else(|e| panic!("{theory_name}: {e}"));
            assert_eq!(
                canonical_theory_bytes(&replayed),
                canonical_theory_bytes(&registry[theory_name.as_str()]),
                "{theory_name}"
            );
        }
    }
}

/// Set in the child process of
/// [`a_protocol_carries_its_recipes_without_earlier_registration`].
const FRESH_PROCESS: &str = "PANPROTO_PROTOCOLS_FRESH_PROCESS";

#[test]
fn a_protocol_carries_its_recipes_without_earlier_registration() {
    if std::env::var_os(FRESH_PROCESS).is_none() {
        // Rerun this test alone in a new process, where nothing has been
        // registered before the protocols are built.
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "a_protocol_carries_its_recipes_without_earlier_registration",
                "--test-threads=1",
            ])
            .env(FRESH_PROCESS, "1")
            .status()
            .unwrap();
        assert!(status.success(), "the fresh process failed: {status}");
        return;
    }
    assert!(
        registered_recipe("ThSQLSchema").is_none(),
        "the process registered theories before building a protocol"
    );
    // Build every protocol before registering anything explicitly.
    let built: Vec<_> = descriptors().iter().map(|d| (d, (d.protocol)())).collect();
    for (descriptor, protocol) in &built {
        let mut registry = HashMap::new();
        (descriptor.register_theories)(&mut registry).unwrap();
        assert_eq!(
            protocol.schema_composition,
            registered_recipe(&protocol.schema_theory),
            "{}",
            descriptor.name
        );
        assert_eq!(
            protocol.instance_composition,
            registered_recipe(&protocol.instance_theory),
            "{}",
            descriptor.name
        );
        assert!(protocol.schema_composition.is_some(), "{}", descriptor.name);
        assert!(
            protocol.instance_composition.is_some(),
            "{}",
            descriptor.name
        );
    }
    let sql = &built.iter().find(|(d, _)| d.name == "sql").unwrap().1;
    let (schema, instance) = hypergraph_functor_specs("ThSQLSchema", "ThSQLInstance");
    assert_eq!(sql.schema_composition, Some(schema));
    assert_eq!(sql.instance_composition, Some(instance));
}

#[test]
fn every_theory_registration_inserts_has_its_composition() {
    for descriptor in descriptors() {
        let (_, registry) = register(descriptor.name);
        for (theory_name, theory) in &registry {
            let composition = registered_composition(theory_name)
                .unwrap_or_else(|| panic!("{}: {theory_name} has no composition", descriptor.name));
            assert_eq!(
                canonical_theory_bytes(&composition.theory),
                canonical_theory_bytes(theory),
                "{}: {theory_name}",
                descriptor.name
            );
            assert_status_clause(&composition, theory_name);
            let recipe = registered_recipe(theory_name)
                .unwrap_or_else(|| panic!("{}: {theory_name} has no recipe", descriptor.name));
            let replayed = recompose(&recipe, &registry)
                .unwrap_or_else(|e| panic!("{}: {theory_name}: {e}", descriptor.name));
            assert_eq!(
                canonical_theory_bytes(&replayed),
                canonical_theory_bytes(theory),
                "{}: {theory_name}",
                descriptor.name
            );
        }
    }
}

/// Every registration diagram of the six groups and `ATProto`, with the
/// helper-independent name of its result.
fn all_diagrams() -> Vec<(&'static str, RegistrationDiagram)> {
    vec![
        ("constrained_multigraph", constrained_multigraph_diagram()),
        ("typed_graph", typed_graph_diagram()),
        ("constrained_hypergraph", constrained_hypergraph_diagram()),
        (
            "constrained_simple_graph",
            constrained_simple_graph_diagram(),
        ),
        ("wtype_meta_node_value", wtype_meta_diagram("ThNodeValue")),
        ("wtype_meta_node", wtype_meta_diagram("ThNode")),
        ("wtype", RegistrationDiagram::single(th_wtype())),
        ("functor", RegistrationDiagram::single(th_functor())),
        ("flat", RegistrationDiagram::single(th_flat())),
        (
            "graph_instance",
            RegistrationDiagram::single(th_graph_instance()),
        ),
    ]
}

#[test]
fn every_registered_theory_has_its_composition() {
    let mut groups_seen: BTreeSet<String> = BTreeSet::new();
    for descriptor in descriptors() {
        let (protocol, registry) = register(descriptor.name);
        for theory_name in [&protocol.schema_theory, &protocol.instance_theory] {
            let composition = registered_composition(theory_name).unwrap();
            assert_eq!(
                composition.theory,
                registry[theory_name.as_str()],
                "{theory_name}"
            );
            let recipe = registered_recipe(theory_name).unwrap();
            let [CompositionStep::Diagram { nodes, inline, .. }] = recipe.steps.as_slice() else {
                panic!("{theory_name}: {:?}", recipe.steps);
            };
            assert_eq!(composition.inclusions.len(), nodes.len(), "{theory_name}");
            for (inclusion, node) in composition.inclusions.iter().zip(nodes) {
                assert_eq!(&*inclusion.domain, node.as_str(), "{theory_name}");
                assert_eq!(&*inclusion.codomain, theory_name.as_str());
            }
            let blocks: Vec<&String> = nodes
                .iter()
                .filter(|n| !inline.iter().any(|t| &*t.name == n.as_str()))
                .collect();
            groups_seen.insert(format!("{blocks:?}"));
        }
    }
    // The six groups and ATProto use these block sets between them.
    let expected: BTreeSet<String> = [
        vec!["ThGraph", "ThConstraint", "ThMulti"],
        vec!["ThGraph", "ThConstraint", "ThMulti", "ThInterface"],
        vec!["ThHypergraph", "ThConstraint"],
        vec!["ThSimpleGraph", "ThConstraint"],
        vec!["ThWType", "ThMeta"],
        vec!["ThWType"],
        vec!["ThFunctor"],
        vec!["ThFlat"],
        vec!["ThGraphInstance"],
    ]
    .into_iter()
    .map(|v| format!("{v:?}"))
    .collect();
    assert_eq!(groups_seen, expected);
}

#[test]
fn registration_provenance_is_total_for_every_source_element() {
    for (name, diagram) in all_diagrams() {
        let composition = diagram.compose(name).unwrap();
        let graph = diagram.diagram();
        for node in &graph.nodes {
            let theory = &node.theory;
            let elements = [
                (
                    SymKind::Sort,
                    theory
                        .sorts
                        .iter()
                        .map(|s| s.name.clone())
                        .collect::<Vec<_>>(),
                ),
                (
                    SymKind::Op,
                    theory.ops.iter().map(|o| o.name.clone()).collect(),
                ),
                (
                    SymKind::Eq,
                    theory.eqs.iter().map(|e| e.name.clone()).collect(),
                ),
                (
                    SymKind::DirectedEq,
                    theory.directed_eqs.iter().map(|e| e.name.clone()).collect(),
                ),
                (
                    SymKind::Policy,
                    theory.policies.iter().map(|p| p.name.clone()).collect(),
                ),
            ];
            for (kind, names) in elements {
                for element in names {
                    let source = SymRef::new(node.label.clone(), element);
                    let image = composition
                        .provenance
                        .image(kind, &source)
                        .unwrap_or_else(|| panic!("{name}: {kind} {source} has no image"));
                    let present = match kind {
                        SymKind::Sort => composition.theory.has_sort(image),
                        SymKind::Op => composition.theory.has_op(image),
                        SymKind::Eq => composition.theory.eqs.iter().any(|e| e.name == *image),
                        SymKind::DirectedEq => composition
                            .theory
                            .directed_eqs
                            .iter()
                            .any(|e| e.name == *image),
                        SymKind::Policy => {
                            composition.theory.policies.iter().any(|p| p.name == *image)
                        }
                    };
                    assert!(
                        present,
                        "{name}: {kind} {source} -> {image} is not in the result"
                    );
                }
            }
        }
    }
}

#[test]
fn registration_does_not_depend_on_the_order_of_its_diagram() {
    for (name, diagram) in all_diagrams() {
        let bytes = canonical_theory_bytes(&diagram.compose(name).unwrap().theory);
        let mut reversed = diagram.clone();
        reversed.blocks.reverse();
        reversed.shared.reverse();
        for (_, targets) in &mut reversed.shared {
            targets.reverse();
        }
        let theory = reversed.compose(name).unwrap().theory;
        assert_eq!(&*theory.name, name);
        assert_eq!(canonical_theory_bytes(&theory), bytes, "{name}");
    }
}

/// The committed theory composition corpus.
fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/theory_composition")
}

/// The files of a directory, by name, with their bytes.
fn files(dir: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn theory_composition_fixtures_match_the_generator() {
    let out = std::env::temp_dir().join(format!(
        "panproto-theory-composition-{}",
        std::process::id()
    ));
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_theory-composition-fixtures"))
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "the generator failed: {status}");
    let generated = files(&out);
    std::fs::remove_dir_all(&out).unwrap();
    let committed = files(&corpus_dir());
    assert_eq!(
        generated.keys().collect::<Vec<_>>(),
        committed.keys().collect::<Vec<_>>(),
        "regenerate with `cargo run -p panproto-protocols --features fixtures --bin theory-composition-fixtures`"
    );
    for (name, bytes) in &generated {
        assert!(
            committed[name] == *bytes,
            "{name} differs from the generator's output; regenerate with \
             `cargo run -p panproto-protocols --features fixtures --bin theory-composition-fixtures`"
        );
    }
}

/// One corpus fixture as decoded.
#[derive(serde::Deserialize)]
struct Fixture {
    diagram: panproto_gat::TheoryDiagram,
    options: panproto_gat::ComposeOptions,
    composition: Composition,
}

#[derive(serde::Deserialize)]
struct Golden {
    context: String,
    digests: std::collections::BTreeMap<String, String>,
}

#[test]
fn theory_composition_fixtures_recompose_to_their_bytes_and_digests() {
    let dir = corpus_dir();
    let golden: Golden =
        serde_json::from_slice(&std::fs::read(dir.join("golden.json")).unwrap()).unwrap();
    assert_eq!(golden.context, panproto_schema::COMPOSITION_CONTEXT);
    let groups: BTreeSet<&str> = golden
        .digests
        .keys()
        .filter_map(|name| name.split('_').next())
        .collect();
    for group in ["tree", "table", "flat", "graph"] {
        assert!(groups.contains(group), "no {group} group fixture");
    }
    let hex = |bytes: &[u8]| -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push(char::from(DIGITS[usize::from(b >> 4)]));
            out.push(char::from(DIGITS[usize::from(b & 0xf)]));
        }
        out
    };
    for (name, digest) in &golden.digests {
        let read = |ext: &str| std::fs::read(dir.join(format!("{name}.{ext}"))).unwrap();
        let decoded: [Fixture; 3] = [
            rmp_serde::from_slice(&read("named.msgpack")).unwrap(),
            ciborium::from_reader(read("cbor").as_slice()).unwrap(),
            serde_json::from_slice(&read("json")).unwrap(),
        ];
        for fixture in decoded {
            let recomputed = panproto_gat::compose(&fixture.diagram, &fixture.options)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            let bytes = panproto_schema::canonical_composition_bytes(&recomputed);
            assert_eq!(
                panproto_schema::canonical_composition_bytes(&fixture.composition),
                bytes,
                "{name}"
            );
            assert_eq!(
                hex(&panproto_schema::composition_digest(&recomputed)),
                *digest,
                "{name}"
            );
        }
    }
}
