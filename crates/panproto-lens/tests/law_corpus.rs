//! Frozen, executable graph and table lens-law corpus.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use panproto_inst::{
    BoundaryProtocols, CompiledMigration, EdgeId, FInstance, Fan, FieldPresence, GEdge, GInstance,
    Node, PutDomainError, RowId, Value, WInstance,
};
use panproto_lens::graph_lens;
use panproto_lens::laws::{FamilyReport, LawReport, LawShape, Mutation, SkipReason, SmallScope};
use panproto_lens::symmetric::{SymmetricLens, TraceObservation, check_observational_equality};
use panproto_lens::table_lens;
use panproto_lens::{GraphShape, Lens, LensError, LensShape, TableContext, TableShape, TreeShape};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Expected {
    Ran,
    TypedRefusal,
    ShapeCannotExpress,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum ExpectedRefusal {
    SynthesizedEdited,
    ContractedEdgeInserted,
    IdCollision,
    MalformedFan,
}

impl ExpectedRefusal {
    const ALL: [Self; 4] = [
        Self::SynthesizedEdited,
        Self::ContractedEdgeInserted,
        Self::IdCollision,
        Self::MalformedFan,
    ];

    const fn matches(self, error: &PutDomainError) -> bool {
        matches!(
            (self, error),
            (
                Self::SynthesizedEdited,
                PutDomainError::SynthesizedEdited { .. }
            ) | (
                Self::ContractedEdgeInserted,
                PutDomainError::ContractedEdgeInserted { .. }
            ) | (Self::IdCollision, PutDomainError::IdCollision { .. })
                | (Self::MalformedFan, PutDomainError::MalformedFan { .. })
        )
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FrozenObservation {
    ran: u32,
    shape_cannot_express: u32,
    no_candidate: u32,
    in_domain: u32,
    in_domain_ok: u32,
    negative: u32,
    negative_ok: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u16,
    mutation: Mutation,
    expected: Expected,
    observation: FrozenObservation,
    refusal: Option<ExpectedRefusal>,
    certificates: Vec<Vec<PutDomainError>>,
}

#[derive(Debug, Default)]
struct RefusalObservations {
    grouped: BTreeMap<(Mutation, ExpectedRefusal), u32>,
    certificates: BTreeMap<Mutation, Vec<Vec<PutDomainError>>>,
}

fn fixture_root(shape: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(format!("{shape}_laws"))
}

fn read_manifests(shape: &str) -> BTreeMap<String, Manifest> {
    let root = fixture_root(shape);
    let mut entries = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", root.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    entries
        .into_iter()
        .map(|entry| {
            let family = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path().join("manifest.json");
            let manifest: Manifest = serde_json::from_str(
                &fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display())),
            )
            .unwrap_or_else(|error| panic!("cannot decode {}: {error}", path.display()));
            (family, manifest)
        })
        .collect()
}

fn edge(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some(name.into()),
    }
}

fn graph_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for (vertex, kind) in vertices {
        builder = builder.vertex(vertex, kind, None).expect("graph vertex");
    }
    for label in edges {
        builder = builder
            .edge(&label.src, &label.tgt, &label.kind, label.name.as_deref())
            .expect("graph edge");
    }
    builder.build().expect("graph schema")
}

fn table_protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn table_schema(
    protocol: &Protocol,
    tables: &[(&str, &[(&str, &str)])],
    relations: &[Edge],
) -> Schema {
    let mut builder = SchemaBuilder::new(protocol);
    for (table, columns) in tables {
        builder = builder.vertex(table, "record", None).expect("table");
        for (column, kind) in *columns {
            let vertex = format!("{table}.{column}");
            builder = builder
                .vertex(&vertex, kind, None)
                .expect("column vertex")
                .edge(table, &vertex, "prop", Some(*column))
                .expect("column edge");
        }
    }
    for relation in relations {
        builder = builder
            .edge(
                &relation.src,
                &relation.tgt,
                &relation.kind,
                relation.name.as_deref(),
            )
            .expect("relation");
    }
    builder.build().expect("table schema")
}

fn identity_lens(schema: &Schema) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

fn row(values: &[(&str, Value)]) -> HashMap<String, Value> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn exercise_graph(name: &str, lens: &Lens, source: &GInstance) -> LawReport {
    graph_lens::check_get_put(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph GetPut: {error:?}"));
    graph_lens::check_put_get(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph PutGet: {error:?}"));
    graph_lens::check_totality(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph Totality: {error:?}"));
    graph_lens::check_put_twice(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph PutTwice: {error:?}"));
    graph_lens::check_congruence(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph Congruence: {error:?}"));
    let report = graph_lens::check_laws(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph gating laws: {error:?}"));
    graph_lens::check_put_injectivity(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph put injectivity: {error:?}"));
    graph_lens::check_locality(lens, source)
        .unwrap_or_else(|error| panic!("{name} graph locality: {error:?}"));
    report
}

fn exercise_table(lens: &Lens, source: &FInstance, protocol: &Protocol) -> LawReport {
    let boundary = || BoundaryProtocols {
        src: protocol,
        tgt: protocol,
    };
    let report = table_lens::check_laws(lens, source, boundary()).expect("table gating laws");
    table_lens::check_put_injectivity(lens, source, boundary()).expect("table put injectivity");
    table_lens::check_locality(lens, source, boundary()).expect("table locality");
    report
}

fn exercise_tree(lens: &Lens, source: &WInstance) -> LawReport {
    let report = panproto_lens::laws::check_laws_report::<TreeShape>(lens, source, &())
        .expect("tree gating laws and classification");
    panproto_lens::laws::check_put_injectivity(lens, source).expect("tree put injectivity");
    panproto_lens::laws::check_locality(lens, source).expect("tree locality");
    report
}

fn dense_graph_fixture() -> (Lens, GInstance) {
    let aa = edge("a", "a", "rel", "aa");
    let ab = edge("a", "b", "rel", "ab");
    let ab_alt = edge("a", "b", "rel", "ab_alt");
    let schema = graph_schema(
        &[("a", "record"), ("b", "record"), ("number", "int")],
        &[aa.clone(), ab.clone(), ab_alt.clone()],
    );
    let mut field_node = Node::new(7, "a");
    field_node
        .extra_fields
        .insert("flag".to_owned(), Value::Bool(false));
    let source = GInstance::new()
        .with_node(field_node)
        .with_node(Node::new(11, "a"))
        .with_node(Node::new(13, "a"))
        .with_node(Node::new(19, "b"))
        .with_node(Node::new(23, "b"))
        .with_node(Node::new(31, "number").with_value(FieldPresence::Present(Value::Int(4))))
        .with_edge_id(EdgeId(41), 7, 19, ab.clone())
        .with_edge_id(EdgeId(43), 11, 19, ab)
        .with_edge_id(EdgeId(47), 7, 19, ab_alt)
        .with_edge_id(EdgeId(53), 7, 7, aa);
    (identity_lens(&schema), source)
}

fn graph_projection_fixture() -> (Lens, GInstance) {
    let source_schema = graph_schema(&[("kept", "record"), ("hidden", "record")], &[]);
    let target_schema = graph_schema(&[("kept", "record")], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("kept".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = GInstance::new()
        .with_node(Node::new(7, "kept"))
        .with_node(Node::new(29, "hidden"));
    (lens, source)
}

fn graph_expansion_fixture() -> (Lens, GInstance) {
    let ab = edge("a", "b", "rel", "b");
    let am = edge("a", "m", "rel", "m");
    let mb = edge("m", "b", "rel", "b");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            expansion_path: std::iter::once((("a".into(), "b".into()), vec!["m".into()])).collect(),
            ..CompiledMigration::default()
        },
        src_schema: graph_schema(
            &[("a", "record"), ("b", "record")],
            std::slice::from_ref(&ab),
        ),
        tgt_schema: graph_schema(
            &[("a", "record"), ("m", "record"), ("b", "record")],
            &[am, mb],
        ),
    };
    let source = GInstance::new()
        .with_node(Node::new(7, "a"))
        .with_node(Node::new(19, "b"))
        .with_edge_id(EdgeId(41), 7, 19, ab);
    (lens, source)
}

fn graph_contraction_fixture() -> (Lens, GInstance) {
    let am = edge("a", "m", "rel", "m");
    let mb = edge("m", "b", "rel", "b");
    let ab = edge("a", "b", "rel", "b");
    let target = graph_schema(
        &[("a", "record"), ("b", "record")],
        std::slice::from_ref(&ab),
    );
    let lens = Lens {
        compiled: identity_lens(&target).compiled,
        src_schema: graph_schema(
            &[("a", "record"), ("m", "record"), ("b", "record")],
            &[am, mb],
        ),
        tgt_schema: target,
    };
    let source = GInstance::new()
        .with_node(Node::new(7, "a"))
        .with_node(Node::new(19, "b"));
    (lens, source)
}

fn graph_contracted_path_fixture() -> (Lens, GInstance) {
    let (lens, source) = graph_contraction_fixture();
    let am = edge("a", "m", "rel", "m");
    let mb = edge("m", "b", "rel", "b");
    let source = source
        .with_node(Node::new(17, "m"))
        .with_node(Node::new(23, "b"))
        .with_edge_id(EdgeId(41), 7, 17, am)
        .with_edge_id(EdgeId(43), 17, 19, mb);
    (lens, source)
}

fn graph_fan_fixture() -> (Lens, GInstance) {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .expect("root")
        .vertex("c", "record", None)
        .expect("child")
        .edge("r", "c", "rel", Some("child"))
        .expect("child edge")
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([
                ("p".to_owned(), "r".to_owned()),
                ("x".to_owned(), "c".to_owned()),
                ("y".to_owned(), "c".to_owned()),
            ]),
            "p",
        )
        .expect("hyper edge")
        .build()
        .expect("fan schema");
    let tree = WInstance::new(
        [
            (7, Node::new(7, "r")),
            (19, Node::new(19, "c")),
            (23, Node::new(23, "c")),
        ]
        .into(),
        vec![
            (7, 19, edge("r", "c", "rel", "child")),
            (7, 23, edge("r", "c", "rel", "child")),
        ],
        vec![Fan {
            hyper_edge_id: "h".to_owned(),
            parent: 7,
            children: HashMap::from([("x".to_owned(), 19), ("y".to_owned(), 23)]),
        }],
        7,
        "r".into(),
    );
    let source = GInstance::from_wtype_with_schema(&tree, &schema).expect("fan graph");
    (identity_lens(&schema), source)
}

fn dense_table_fixture(protocol: &Protocol) -> (Lens, FInstance) {
    let ab = edge("a", "b", "fk", "ab");
    let ab_alt = edge("a", "b", "fk", "ab_alt");
    let aa = edge("a", "a", "fk", "aa");
    let schema = table_schema(
        protocol,
        &[
            ("a", &[("other", "int"), ("value", "int")]),
            ("b", &[("other", "int"), ("value", "int")]),
        ],
        &[ab.clone(), ab_alt.clone(), aa.clone()],
    );
    let source = FInstance::new()
        .with_rows(
            "a",
            [
                (
                    RowId(7),
                    row(&[("other", Value::Int(10)), ("value", Value::Int(1))]),
                ),
                (
                    RowId(11),
                    row(&[("other", Value::Int(20)), ("value", Value::Int(2))]),
                ),
                (
                    RowId(13),
                    row(&[("other", Value::Int(30)), ("value", Value::Int(3))]),
                ),
            ],
        )
        .expect("a rows")
        .with_rows(
            "b",
            [
                (
                    RowId(19),
                    row(&[("other", Value::Int(40)), ("value", Value::Int(4))]),
                ),
                (
                    RowId(23),
                    row(&[("other", Value::Int(50)), ("value", Value::Int(5))]),
                ),
            ],
        )
        .expect("b rows")
        .with_foreign_key(ab, [(RowId(7), RowId(19)), (RowId(11), RowId(19))])
        .expect("ab pairs")
        .with_foreign_key(ab_alt, [(RowId(7), RowId(19))])
        .expect("alternate pair")
        .with_foreign_key(aa, [(RowId(7), RowId(7))])
        .expect("self pair");
    (identity_lens(&schema), source)
}

fn table_projection_fixture(protocol: &Protocol) -> (Lens, FInstance) {
    let source_schema = table_schema(protocol, &[("kept", &[]), ("hidden", &[])], &[]);
    let target_schema = table_schema(protocol, &[("kept", &[])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("kept".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("kept", [(RowId(7), HashMap::new())])
        .expect("kept row")
        .with_rows("hidden", [(RowId(29), HashMap::new())])
        .expect("hidden row");
    (lens, source)
}

fn table_contraction_fixture(protocol: &Protocol, with_path: bool) -> (Lens, FInstance) {
    let am = edge("a", "m", "fk", "am");
    let mb = edge("m", "b", "fk", "mb");
    let ab = edge("a", "b", "fk", "ab");
    let ab_direct = edge("a", "b", "fk", "ab_direct");
    let mut source_relations = vec![am.clone(), mb.clone()];
    if with_path {
        source_relations.push(ab_direct.clone());
    }
    let source_schema = table_schema(
        protocol,
        &[("a", &[]), ("m", &[]), ("b", &[])],
        &source_relations,
    );
    let target_schema = table_schema(
        protocol,
        &[("a", &[]), ("b", &[])],
        std::slice::from_ref(&ab),
    );
    let mut compiled = identity_lens(&target_schema).compiled;
    if with_path {
        compiled.edge_remap.insert(ab_direct.clone(), ab);
    }
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let mut source = FInstance::new()
        .with_rows("a", [(RowId(7), HashMap::new())])
        .expect("a row")
        .with_rows(
            "b",
            [(RowId(19), HashMap::new()), (RowId(23), HashMap::new())],
        )
        .expect("b rows");
    if with_path {
        source = source
            .with_rows("m", [(RowId(17), HashMap::new())])
            .expect("middle row")
            .with_foreign_key(am, [(RowId(7), RowId(17))])
            .expect("first path leg")
            .with_foreign_key(mb, [(RowId(17), RowId(19))])
            .expect("second path leg")
            .with_foreign_key(ab_direct, [(RowId(7), RowId(19))])
            .expect("direct origin of the contracted pair");
    }
    (lens, source)
}

fn assert_report_partition(report: &LawReport) {
    let ran = report
        .ran
        .iter()
        .map(|family| family.mutation)
        .collect::<BTreeSet<_>>();
    let skipped = report
        .skipped
        .iter()
        .map(|(mutation, _)| *mutation)
        .collect::<BTreeSet<_>>();
    assert!(
        ran.is_disjoint(&skipped),
        "a family was both run and skipped"
    );
    assert_eq!(
        ran.union(&skipped).copied().collect::<BTreeSet<_>>(),
        Mutation::ALL.into_iter().collect(),
        "every report must classify all public mutation families"
    );
}

fn reports_for(reports: &[LawReport], mutation: Mutation) -> impl Iterator<Item = &FamilyReport> {
    reports
        .iter()
        .flat_map(|report| report.ran.iter())
        .filter(move |family| family.mutation == mutation)
}

fn observed_counts(reports: &[LawReport], mutation: Mutation) -> FrozenObservation {
    let families = reports_for(reports, mutation).collect::<Vec<_>>();
    FrozenObservation {
        ran: u32::try_from(families.len()).expect("corpus report count fits u32"),
        shape_cannot_express: u32::try_from(
            reports
                .iter()
                .filter(|report| {
                    report
                        .skipped
                        .contains(&(mutation, SkipReason::ShapeCannotExpress))
                })
                .count(),
        )
        .expect("corpus skip count fits u32"),
        no_candidate: u32::try_from(
            reports
                .iter()
                .filter(|report| {
                    report
                        .skipped
                        .contains(&(mutation, SkipReason::NoCandidate))
                })
                .count(),
        )
        .expect("corpus skip count fits u32"),
        in_domain: families.iter().map(|family| family.in_domain).sum(),
        in_domain_ok: families.iter().map(|family| family.in_domain_ok).sum(),
        negative: families.iter().map(|family| family.negative).sum(),
        negative_ok: families.iter().map(|family| family.negative_ok).sum(),
    }
}

fn record_declared_refusal(
    refusals: &mut RefusalObservations,
    mutation: Mutation,
    expected: &[PutDomainError],
) {
    refusals
        .certificates
        .entry(mutation)
        .or_default()
        .push(expected.to_vec());
    for refusal in ExpectedRefusal::ALL {
        if expected.iter().all(|error| refusal.matches(error)) {
            *refusals.grouped.entry((mutation, refusal)).or_default() += 1;
        }
    }
}

fn graph_refusal_observations(
    fixtures: &[(impl AsRef<str>, (Lens, GInstance))],
) -> RefusalObservations {
    let mut refusals = RefusalObservations::default();
    for (_, (lens, source)) in fixtures {
        let (view, complement) = graph_lens::get(lens, source).expect("graph corpus get");
        for mutation in Mutation::ALL {
            let cases = GraphShape::negative_mutation_cases(
                lens,
                source,
                &view,
                &complement,
                &(),
                mutation,
            );
            for case in cases {
                assert!(
                    !case.expected.is_empty(),
                    "{mutation:?} declared an empty negative graph certificate"
                );
                assert_eq!(
                    graph_lens::check_put_domain(lens, &case.view, &complement),
                    Err(case.expected.clone()),
                    "{mutation:?} graph predicate drifted from its declared ordered refusal"
                );
                assert!(
                    matches!(
                        graph_lens::put(lens, &case.view, &complement),
                        Err(LensError::PutDomain(error))
                            if error.as_ref() == &case.expected[0]
                    ),
                    "{mutation:?} graph put did not return the declared first refusal"
                );
                record_declared_refusal(&mut refusals, mutation, &case.expected);
            }
        }
    }
    refusals
}

fn table_refusal_observations(
    fixtures: &[(Lens, FInstance)],
    protocol: &Protocol,
) -> RefusalObservations {
    let mut refusals = RefusalObservations::default();
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol.clone(),
    };
    for (lens, source) in fixtures {
        let boundary = || BoundaryProtocols {
            src: protocol,
            tgt: protocol,
        };
        let (view, complement) =
            table_lens::get(lens, source, boundary()).expect("table corpus get");
        for mutation in Mutation::ALL {
            let cases = TableShape::negative_mutation_cases(
                lens,
                source,
                &view,
                &complement,
                &context,
                mutation,
            );
            for case in cases {
                assert!(
                    !case.expected.is_empty(),
                    "{mutation:?} declared an empty negative table certificate"
                );
                assert_eq!(
                    table_lens::check_put_domain(lens, &case.view, &complement, boundary()),
                    Err(case.expected.clone()),
                    "{mutation:?} table predicate drifted from its declared ordered refusal"
                );
                assert!(
                    matches!(
                        table_lens::put(lens, &case.view, &complement, boundary()),
                        Err(LensError::PutDomain(error))
                            if error.as_ref() == &case.expected[0]
                    ),
                    "{mutation:?} table put did not return the declared first refusal"
                );
                record_declared_refusal(&mut refusals, mutation, &case.expected);
            }
        }
    }
    refusals
}

fn assert_manifests(shape: &str, reports: &[LawReport], refusals: &RefusalObservations) {
    reports.iter().for_each(assert_report_partition);
    let manifests = read_manifests(shape);
    let public = Mutation::ALL
        .into_iter()
        .map(|mutation| mutation.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(manifests.keys().cloned().collect::<BTreeSet<_>>(), public);

    for (directory, manifest) in manifests {
        assert_eq!(manifest.format_version, 3, "{shape}/{directory}");
        assert_eq!(manifest.mutation.as_str(), directory, "{shape}/{directory}");
        let families = reports_for(reports, manifest.mutation).collect::<Vec<_>>();
        let certificates = refusals
            .certificates
            .get(&manifest.mutation)
            .cloned()
            .unwrap_or_default();
        let observation = observed_counts(reports, manifest.mutation);
        assert_eq!(
            &certificates, &manifest.certificates,
            "{shape}/{directory} drifted from its frozen ordered negative certificates"
        );
        assert_eq!(
            observation, manifest.observation,
            "{shape}/{directory} drifted from its frozen executable observation"
        );
        match manifest.expected {
            Expected::Ran => {
                assert_eq!(manifest.refusal, None, "{shape}/{directory}");
                assert!(manifest.certificates.is_empty(), "{shape}/{directory}");
                assert!(
                    manifest.observation.in_domain > 0
                        && manifest.observation.in_domain_ok == manifest.observation.in_domain,
                    "{shape}/{directory} has no successfully executed in-domain candidate: {families:?}"
                );
            }
            Expected::TypedRefusal => {
                let expected = manifest
                    .refusal
                    .expect("typed-refusal manifest names its exact error variant");
                assert!(
                    !manifest.certificates.is_empty(),
                    "{shape}/{directory} must freeze at least one exact certificate"
                );
                assert!(
                    manifest.observation.negative > 0
                        && manifest.observation.negative_ok == manifest.observation.negative,
                    "{shape}/{directory} has a missing or inexact typed-refusal witness: {families:?}"
                );
                assert_eq!(
                    refusals
                        .grouped
                        .get(&(manifest.mutation, expected))
                        .copied()
                        .unwrap_or_default(),
                    manifest.observation.negative_ok,
                    "{shape}/{directory} did not produce the frozen exact refusal variant"
                );
            }
            Expected::ShapeCannotExpress => {
                assert_eq!(manifest.refusal, None, "{shape}/{directory}");
                assert!(manifest.certificates.is_empty(), "{shape}/{directory}");
                assert!(families.is_empty(), "{shape}/{directory} unexpectedly ran");
                assert!(reports.iter().all(|report| {
                    report
                        .skipped
                        .contains(&(manifest.mutation, SkipReason::ShapeCannotExpress))
                }));
            }
        }
    }
}

#[test]
fn graph_corpus_executes_every_frozen_mutation_expectation() {
    let fixtures = vec![
        ("dense", dense_graph_fixture()),
        ("projection", graph_projection_fixture()),
        ("expansion", graph_expansion_fixture()),
        ("contraction", graph_contraction_fixture()),
        ("contracted-path", graph_contracted_path_fixture()),
        ("fan", graph_fan_fixture()),
    ]
    .into_boxed_slice();
    let reports = fixtures
        .iter()
        .map(|(name, (lens, source))| exercise_graph(name, lens, source))
        .collect::<Vec<_>>();
    let refusals = graph_refusal_observations(&fixtures);
    assert_manifests("graph", &reports, &refusals);
}

#[test]
fn table_corpus_executes_every_frozen_mutation_expectation() {
    let protocol = table_protocol();
    let fixtures = [
        dense_table_fixture(&protocol),
        table_projection_fixture(&protocol),
        table_contraction_fixture(&protocol, true),
        table_contraction_fixture(&protocol, false),
    ];
    let reports = fixtures
        .iter()
        .map(|(lens, source)| exercise_table(lens, source, &protocol))
        .collect::<Vec<_>>();
    let refusals = table_refusal_observations(&fixtures, &protocol);
    assert_manifests("table", &reports, &refusals);
}

#[test]
fn tree_corpus_executes_standalone_checks_and_report_classification() {
    let child = edge("root", "leaf", "rel", "child");
    let schema = graph_schema(
        &[("root", "record"), ("leaf", "record")],
        std::slice::from_ref(&child),
    );
    let lens = identity_lens(&schema);
    let source = WInstance::new(
        HashMap::from([(7, Node::new(7, "root")), (19, Node::new(19, "leaf"))]),
        vec![(7, 19, child)],
        Vec::new(),
        7,
        "root".into(),
    );

    let report = exercise_tree(&lens, &source);
    assert_report_partition(&report);
    assert!(report.classification.put_put.is_some());
}

#[test]
fn graph_corpus_pins_noncanonical_and_u32_max_identifiers() {
    let loop_edge = edge("a", "a", "rel", "loop");
    let schema = graph_schema(&[("a", "record")], std::slice::from_ref(&loop_edge));
    let lens = identity_lens(&schema);
    let source = GInstance::new()
        .with_node(Node::new(u32::MAX, "a"))
        .with_edge_id(EdgeId(u32::MAX), u32::MAX, u32::MAX, loop_edge);
    let report = exercise_graph("u32-max", &lens, &source);
    assert!(
        report
            .skipped
            .contains(&(Mutation::AddNode, SkipReason::NoCandidate))
    );
    assert!(
        report
            .skipped
            .contains(&(Mutation::AddEdge, SkipReason::NoCandidate))
    );

    let (view, complement) = graph_lens::get(&lens, &source).expect("boundary graph get");
    let restored = graph_lens::put(&lens, &view, &complement).expect("boundary graph put");
    assert!(GInstance::identical(&source, &restored));
}

#[test]
fn table_corpus_pins_noncanonical_and_u32_max_identifiers() {
    let protocol = table_protocol();
    let schema = table_schema(&protocol, &[("a", &[])], &[]);
    let lens = identity_lens(&schema);
    let source = FInstance::new()
        .with_rows("a", [(RowId(u32::MAX), HashMap::new())])
        .expect("boundary row");
    let report = exercise_table(&lens, &source, &protocol);
    assert!(
        report
            .skipped
            .contains(&(Mutation::AddNode, SkipReason::NoCandidate))
    );

    let boundary = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let (view, complement) = table_lens::get(&lens, &source, boundary).expect("boundary table get");
    let restored =
        table_lens::put(&lens, &view, &complement, boundary).expect("boundary table put");
    assert_eq!(source, restored);
}

#[test]
fn corpus_does_not_accept_untyped_operational_failures_as_refusals() {
    let (lens, source) = graph_contraction_fixture();
    let (mut view, complement) = graph_lens::get(&lens, &source).expect("contraction get");
    let label = lens
        .tgt_schema
        .edges
        .keys()
        .next()
        .expect("contracted label");
    view.insert_edge(EdgeId(97), GEdge::new(7, 19, label.clone()))
        .expect("candidate contracted edge");
    assert!(matches!(
        graph_lens::put(&lens, &view, &complement),
        Err(LensError::PutDomain(error))
            if matches!(*error, panproto_inst::PutDomainError::ContractedEdgeInserted { .. })
    ));
}

fn assert_tree_sync_words() {
    let child = edge("root", "leaf", "rel", "child");
    let schema = graph_schema(
        &[("root", "record"), ("leaf", "record")],
        std::slice::from_ref(&child),
    );
    let first = SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema))
        .expect("first identity span");
    let second = SymmetricLens::from_span(identity_lens(&schema), identity_lens(&schema))
        .expect("second identity span");
    let middle = WInstance::new(
        HashMap::from([(7, Node::new(7, "root")), (19, Node::new(19, "leaf"))]),
        vec![(7, 19, child)],
        Vec::new(),
        7,
        "root".into(),
    );
    let first_state = TreeShape::get(&first.left, &middle, &()).expect("first observation");
    let second_state = TreeShape::get(&second.left, &middle, &()).expect("second observation");
    let report = check_observational_equality::<TreeShape>(
        &TraceObservation {
            lens: &first,
            left_context: &(),
            right_context: &(),
            view: &first_state.0,
            complement: &first_state.1,
        },
        &TraceObservation {
            lens: &second,
            left_context: &(),
            right_context: &(),
            view: &second_state.0,
            complement: &second_state.1,
        },
    )
    .expect("identical spans agree on every bounded word");
    assert_eq!(report.max_word_len, 4);
    assert_eq!(report.words_checked, 2 + 4 + 8 + 16);
}

fn assert_graph_sync_words() {
    let (graph_identity, graph_middle) = dense_graph_fixture();
    let graph_schema = graph_identity.src_schema;
    let graph_first =
        SymmetricLens::from_span(identity_lens(&graph_schema), identity_lens(&graph_schema))
            .expect("first graph identity span");
    let graph_second =
        SymmetricLens::from_span(identity_lens(&graph_schema), identity_lens(&graph_schema))
            .expect("second graph identity span");
    let graph_first_state =
        GraphShape::get(&graph_first.left, &graph_middle, &()).expect("first graph observation");
    let graph_second_state =
        GraphShape::get(&graph_second.left, &graph_middle, &()).expect("second graph observation");
    let graph_report = check_observational_equality::<GraphShape>(
        &TraceObservation {
            lens: &graph_first,
            left_context: &(),
            right_context: &(),
            view: &graph_first_state.0,
            complement: &graph_first_state.1,
        },
        &TraceObservation {
            lens: &graph_second,
            left_context: &(),
            right_context: &(),
            view: &graph_second_state.0,
            complement: &graph_second_state.1,
        },
    )
    .expect("identical graph spans agree on every bounded word");
    assert_eq!(graph_report.max_word_len, 4);
    assert_eq!(graph_report.words_checked, 30);
}

fn assert_table_sync_words() {
    let protocol = table_protocol();
    let (table_identity, table_middle) = dense_table_fixture(&protocol);
    let table_schema = table_identity.src_schema;
    let table_first =
        SymmetricLens::from_span(identity_lens(&table_schema), identity_lens(&table_schema))
            .expect("first table identity span");
    let table_second =
        SymmetricLens::from_span(identity_lens(&table_schema), identity_lens(&table_schema))
            .expect("second table identity span");
    let table_context = TableContext {
        src: protocol.clone(),
        tgt: protocol,
    };
    let table_first_state = TableShape::get(&table_first.left, &table_middle, &table_context)
        .expect("first table observation");
    let table_second_state = TableShape::get(&table_second.left, &table_middle, &table_context)
        .expect("second table observation");
    let table_report = check_observational_equality::<TableShape>(
        &TraceObservation {
            lens: &table_first,
            left_context: &table_context,
            right_context: &table_context,
            view: &table_first_state.0,
            complement: &table_first_state.1,
        },
        &TraceObservation {
            lens: &table_second,
            left_context: &table_context,
            right_context: &table_context,
            view: &table_second_state.0,
            complement: &table_second_state.1,
        },
    )
    .expect("identical table spans agree on every bounded word");
    assert_eq!(table_report.max_word_len, 4);
    assert_eq!(table_report.words_checked, 30);
}

#[test]
fn symmetric_observation_runs_every_sync_word_up_to_the_bound() {
    assert_tree_sync_words();
    assert_graph_sync_words();
    assert_table_sync_words();
}

fn value_family_counts(reports: &[LawReport]) -> Vec<u32> {
    reports
        .iter()
        .filter_map(|report| {
            report
                .ran
                .iter()
                .find(|family| family.mutation == Mutation::ValueEdit)
        })
        .map(|family| family.in_domain)
        .collect()
}

fn assert_graph_exhaustive_values(protocol: &Protocol, expected: &[Value]) {
    let graph_schema = SchemaBuilder::new(protocol)
        .vertex("value", "any", None)
        .and_then(SchemaBuilder::build)
        .expect("one-value graph schema");
    let graph_identity = identity_lens(&graph_schema);
    let graph_source =
        GInstance::new().with_node(Node::new(0, "value").with_value(FieldPresence::Null));
    let (graph_view, graph_complement) =
        GraphShape::get(&graph_identity, &graph_source, &()).expect("graph get");
    let graph_candidates = GraphShape::exhaustive_value_cases(
        &graph_identity,
        &graph_source,
        &graph_view,
        &graph_complement,
        &(),
    );
    let graph_values = graph_candidates
        .iter()
        .map(|candidate| candidate.node(0).expect("value node"))
        .map(|node| node.value.as_ref().expect("value coordinate"))
        .map(|presence| match presence {
            FieldPresence::Null => Value::Null,
            FieldPresence::Present(value) => value.clone(),
            FieldPresence::Absent => panic!("enumerated value coordinate became absent"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        graph_values.as_slice(),
        expected,
        "graph value coordinate reaches every other alphabet member"
    );
    let graph_reports = graph_lens::check_laws_exhaustive(
        &graph_identity,
        &graph_schema,
        SmallScope {
            max_nodes: 1,
            max_edges: 0,
        },
    )
    .expect("the exhaustive graph wrapper checks every full-alphabet edit");
    assert_eq!(graph_reports.len(), 7, "empty plus six valued graphs");
    assert_eq!(value_family_counts(&graph_reports), vec![5; 6]);
}

fn assert_tree_exhaustive_values(protocol: &Protocol, expected: &[Value]) {
    let graph_schema = SchemaBuilder::new(protocol)
        .vertex("value", "any", None)
        .and_then(SchemaBuilder::build)
        .expect("one-value tree schema");
    let tree_lens = identity_lens(&graph_schema);
    let tree_source = WInstance::new(
        HashMap::from([(0, Node::new(0, "value").with_value(FieldPresence::Null))]),
        Vec::new(),
        Vec::new(),
        0,
        "value".into(),
    );
    let (tree_view, tree_complement) =
        TreeShape::get(&tree_lens, &tree_source, &()).expect("tree get");
    let tree_candidates = TreeShape::exhaustive_value_cases(
        &tree_lens,
        &tree_source,
        &tree_view,
        &tree_complement,
        &(),
    );
    let tree_values = tree_candidates
        .iter()
        .map(|candidate| candidate.nodes.get(&0).expect("value node"))
        .map(|node| node.value.as_ref().expect("value coordinate"))
        .map(|presence| match presence {
            FieldPresence::Null => Value::Null,
            FieldPresence::Present(value) => value.clone(),
            FieldPresence::Absent => panic!("enumerated value coordinate became absent"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tree_values.as_slice(),
        expected,
        "tree value coordinate reaches every other alphabet member"
    );
    let tree_reports = panproto_lens::laws::check_laws_exhaustive(
        &tree_lens,
        &graph_schema,
        SmallScope {
            max_nodes: 1,
            max_edges: 0,
        },
    )
    .expect("the exhaustive tree wrapper checks every full-alphabet edit");
    assert_eq!(tree_reports.len(), 6, "six one-node valued trees");
    assert_eq!(value_family_counts(&tree_reports), vec![5; 6]);
}

fn assert_table_exhaustive_values(protocol: &Protocol, expected: &[Value]) {
    let schema = table_schema(protocol, &[("row", &[("value", "any")])], &[]);
    let lens = identity_lens(&schema);
    let source = FInstance::new()
        .with_rows("row", [(RowId(0), row(&[("value", Value::Null)]))])
        .expect("one-row any-valued table");
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol.clone(),
    };
    let (view, complement) = TableShape::get(&lens, &source, &context).expect("table get");
    let candidates =
        TableShape::exhaustive_value_cases(&lens, &source, &view, &complement, &context);
    assert_eq!(
        candidates.len(),
        5,
        "six-value alphabet minus the source value"
    );
    let observed = candidates
        .iter()
        .filter_map(|candidate| candidate.row(RowId(0)))
        .filter_map(|row| row.get("value"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(observed.as_slice(), expected);
    let reports = table_lens::check_laws_exhaustive(
        &lens,
        &schema,
        BoundaryProtocols {
            src: protocol,
            tgt: protocol,
        },
        SmallScope {
            max_nodes: 1,
            max_edges: 0,
        },
    )
    .expect("the exhaustive wrapper checks every full-alphabet edit");
    assert_eq!(reports.len(), 8, "empty plus seven one-row cell states");
    assert_eq!(
        value_family_counts(&reports),
        vec![5; 6],
        "each of the six valued sources reports all five other values"
    );
}

#[test]
fn exhaustive_value_edits_reach_every_other_alphabet_value() {
    let protocol = Protocol {
        value_kinds: BTreeMap::from([("any".to_owned(), ScalarKind::Any)]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let expected_other_values = [
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(0),
        Value::Int(1),
        Value::Str(String::new()),
    ];

    assert_graph_exhaustive_values(&protocol, &expected_other_values);
    assert_tree_exhaustive_values(&protocol, &expected_other_values);
    assert_table_exhaustive_values(&protocol, &expected_other_values);
}

#[test]
fn small_scope_graph_count_matches_labelled_product_formula() {
    let loop_edge = edge("a", "a", "rel", "loop");
    let schema = graph_schema(&[("a", "record")], std::slice::from_ref(&loop_edge));
    let scope = SmallScope {
        max_nodes: 2,
        max_edges: 2,
    };
    let instances = GraphShape::enumerate_small(&schema, scope, &());
    // Empty plus, for n=1 and n=2 labelled nodes, sum_{e=0}^2 (n^2)^e.
    let independent = 1 + (1 + 1 + 1) + (1 + 4 + 16);
    println!(
        "labelled graph SmallScope(max_nodes=2,max_edges=2): closed-product count={independent}"
    );
    assert_eq!(instances.len(), independent);
}

#[test]
fn small_scope_tree_count_matches_labelled_rooted_formula() {
    let loop_edge = edge("a", "a", "rel", "loop");
    let schema = graph_schema(&[("a", "record")], std::slice::from_ref(&loop_edge));
    let scope = SmallScope {
        max_nodes: 2,
        max_edges: 2,
    };
    let instances = TreeShape::enumerate_small(&schema, scope, &());
    // The canonical root is id 0: one one-node tree and the labelled 0 -> 1 tree.
    println!("labelled tree SmallScope(max_nodes=2,max_edges=2): rooted count=2");
    assert_eq!(instances.len(), 2);
}

#[test]
fn small_scope_table_count_matches_labelled_pair_formula() {
    let protocol = table_protocol();
    let self_fk = edge("row", "row", "fk", "self");
    let schema = table_schema(&protocol, &[("row", &[])], &[self_fk]);
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol,
    };
    let scope = SmallScope {
        max_nodes: 2,
        max_edges: 2,
    };
    let instances = TableShape::enumerate_small(&schema, scope, &context);
    // Empty plus n=1: C(1,0)+C(1,1), and n=2:
    // C(4,0)+C(4,1)+C(4,2). Foreign-key pairs form labelled sets.
    let independent = 1 + 2 + (1 + 4 + 6);
    println!("labelled table SmallScope(max_nodes=2,max_edges=2): pair-set count={independent}");
    assert_eq!(instances.len(), independent);
}
