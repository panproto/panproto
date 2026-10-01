//! The frozen validation corpus under `tests/fixtures/validation/`.
//!
//! The corpus is written by the `validation-fixtures` binary; these tests
//! check that the committed files are its output, that every fixture reads
//! back the same from each of its encodings and revalidates to its stored
//! report, that every diagnostic's failure witness verifies against the
//! fixture's instance, and that each fixture shows what it is named for.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_gat::CheckStatus;
use panproto_inst::validation::{
    CheckKind, EndpointSide, ExternalVerdicts, IncompleteReason, Requirement, SchemaRef,
    SourceLocation, ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport,
    Violation,
};
use panproto_inst::{EdgeId, FInstance, GInstance, ViewRef, WInstance, from_node_link_json};
use panproto_protocols::registry::descriptor;
use panproto_schema::{Constraint, Edge, InstanceShapeKind, Protocol, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The regeneration command the failure messages name.
const REGENERATE: &str =
    "cargo run -p panproto-protocols --features fixtures --bin validation-fixtures";

/// The committed corpus.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/validation")
}

/// Every file under `dir`, by its path relative to `dir`, with its bytes.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(name, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn validation_fixtures_match_the_generator() {
    let out = std::env::temp_dir().join(format!(
        "panproto-validation-fixtures-{}",
        std::process::id()
    ));
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_validation-fixtures"))
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
        "regenerate with `{REGENERATE}`"
    );
    for (name, bytes) in &generated {
        assert!(
            committed[name] == *bytes,
            "{name} differs from the generator's output; regenerate with `{REGENERATE}`"
        );
    }
}

/// The instance of a fixture, in its shape.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Instance {
    Graph(GInstance),
    Tree(WInstance),
    Table(FInstance),
}

/// One validation fixture as decoded.
#[derive(Debug, Deserialize, Serialize)]
struct Fixture {
    name: String,
    protocol: String,
    protocol_definition: Option<Protocol>,
    schema: Schema,
    options: ValidationOptions,
    budget: ValidationBudget,
    external: ExternalVerdicts,
    instance: Instance,
    report: ValidationReport,
}

impl Fixture {
    /// The fixture's protocol: its stored definition, or the registered
    /// protocol of its name.
    fn protocol(&self) -> Protocol {
        self.protocol_definition.clone().unwrap_or_else(|| {
            (descriptor(&self.protocol)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: protocol {} is not registered",
                        self.name, self.protocol
                    )
                })
                .protocol)()
        })
    }

    fn plan(&self) -> ValidationPlan {
        ValidationPlan::compile(&self.schema, &self.protocol(), &self.options)
    }

    /// The report validating the fixture's instance gives now.
    fn revalidate(&self) -> ValidationReport {
        let plan = self.plan();
        match &self.instance {
            Instance::Graph(g) => plan.validate_graph(g, &self.budget, &self.external),
            Instance::Tree(t) => plan.validate_tree(t, &self.budget, &self.external),
            Instance::Table(f) => plan.validate_table(f, &self.budget, &self.external),
        }
    }

    /// Whether every diagnostic of the stored report verifies against the
    /// instance and the verdicts.
    fn witnesses_verify(&self) -> Vec<String> {
        let plan = self.plan();
        self.report
            .diagnostics
            .iter()
            .filter(|d| !match &self.instance {
                Instance::Graph(g) => d.verify_with_verdicts(&plan, g, &self.external),
                Instance::Tree(t) => d.verify_with_verdicts(&plan, t, &self.external),
                Instance::Table(f) => d.verify_with_verdicts(&plan, f, &self.external),
            })
            .map(|d| format!("{}: {d}", self.name))
            .collect()
    }
}

/// One rejected instance as decoded.
#[derive(Debug, Deserialize, Serialize)]
struct Rejection {
    name: String,
    shape: InstanceShapeKind,
    instance: serde_json::Value,
    error: String,
}

/// Decode `<name>.<ext>` for each of the three encodings, checking that
/// they agree.
fn decode_three<T: DeserializeOwned + Serialize>(name: &str) -> T {
    let path = |ext: &str| corpus_dir().join(format!("{name}.{ext}"));
    let json: T = serde_json::from_slice(&std::fs::read(path("json")).unwrap())
        .unwrap_or_else(|e| panic!("{name}.json: {e}"));
    let msgpack: T = rmp_serde::from_slice(&std::fs::read(path("named.msgpack")).unwrap())
        .unwrap_or_else(|e| panic!("{name}.named.msgpack: {e}"));
    let cbor: T = ciborium::from_reader(std::fs::read(path("cbor")).unwrap().as_slice())
        .unwrap_or_else(|e| panic!("{name}.cbor: {e}"));
    let text = |value: &T| serde_json::to_string(value).unwrap();
    assert_eq!(
        text(&json),
        text(&msgpack),
        "{name}: JSON and MessagePack differ"
    );
    assert_eq!(text(&json), text(&cbor), "{name}: JSON and CBOR differ");
    json
}

/// The fixture names of the corpus, and the names of its rejections.
fn names() -> (Vec<String>, Vec<String>) {
    let mut fixtures = Vec::new();
    let mut rejections = Vec::new();
    for file in files(&corpus_dir()).keys() {
        let Some(name) = file.strip_suffix(".json") else {
            continue;
        };
        if name.ends_with(".nodelink") {
            continue;
        }
        if name.ends_with(".rejection") {
            rejections.push(name.to_owned());
        } else {
            fixtures.push(name.to_owned());
        }
    }
    (fixtures, rejections)
}

/// Every fixture of the corpus, decoded.
fn fixtures() -> BTreeMap<String, Fixture> {
    names()
        .0
        .into_iter()
        .map(|name| {
            let fixture: Fixture = decode_three(&name);
            assert_eq!(fixture.name, name);
            (name, fixture)
        })
        .collect()
}

/// Every fixture reads back the same from its three encodings, revalidates
/// to its stored report byte for byte, and has failure witnesses that
/// verify against its instance and verdicts; a graph's node-link form reads
/// back to the same graph.
#[test]
fn validation_fixtures_revalidate_to_their_reports_and_witnesses() {
    let fixtures = fixtures();
    assert!(fixtures.len() > 50, "{} fixtures", fixtures.len());
    let mut unverified = Vec::new();
    for (name, fixture) in &fixtures {
        assert_eq!(
            fixture.revalidate().canonical_bytes(),
            fixture.report.canonical_bytes(),
            "{name} no longer validates to its report; regenerate with `{REGENERATE}`"
        );
        unverified.extend(fixture.witnesses_verify());
        if let Instance::Graph(g) = &fixture.instance {
            let path = corpus_dir().join(format!("{name}.nodelink.json"));
            let text: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let back = from_node_link_json(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                GInstance::identical(&back, g),
                "{name}: the node-link form differs"
            );
        }
    }
    assert!(
        unverified.is_empty(),
        "witnesses that do not verify: {unverified:#?}"
    );
}

/// Each rejected instance is refused by decoding, in every encoding, with
/// the error the fixture records.
#[test]
fn rejected_instances_are_refused_by_decoding() {
    let rejections = names().1;
    assert_eq!(
        rejections,
        vec![
            "anchors/dangling_edge.rejection".to_owned(),
            "anchors/dangling_foreign_key.rejection".to_owned()
        ]
    );
    for name in rejections {
        let rejection: Rejection = decode_three(&name);
        let error = match rejection.shape {
            InstanceShapeKind::Graph => {
                serde_json::from_value::<GInstance>(rejection.instance.clone()).err()
            }
            InstanceShapeKind::Table => {
                serde_json::from_value::<FInstance>(rejection.instance.clone()).err()
            }
            InstanceShapeKind::Tree => {
                serde_json::from_value::<WInstance>(rejection.instance.clone()).err()
            }
        };
        assert_eq!(
            error.map(|e| e.to_string()),
            Some(rejection.error.clone()),
            "{name}"
        );
        let bytes = rmp_serde::to_vec_named(&rejection.instance).unwrap();
        let refused = match rejection.shape {
            InstanceShapeKind::Graph => rmp_serde::from_slice::<GInstance>(&bytes).is_err(),
            InstanceShapeKind::Table => rmp_serde::from_slice::<FInstance>(&bytes).is_err(),
            InstanceShapeKind::Tree => rmp_serde::from_slice::<WInstance>(&bytes).is_err(),
        };
        assert!(refused, "{name}: MessagePack decoding accepts the instance");
    }
}

/// Construction refuses the same instances the rejections record: an edge
/// to an absent node, and a foreign-key pair to an absent row.
#[test]
fn construction_refuses_dangling_endpoints() {
    use panproto_inst::{InstError, Node, RowId};
    let ab = Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "prop".into(),
        name: Some("ab".into()),
    };
    let mut graph = GInstance::new().with_node(Node::new(0, "a"));
    assert!(matches!(
        graph.add_edge(0, 99, ab),
        Err(InstError::DanglingEdge { endpoint: 99, .. })
    ));
    let stars = Edge {
        src: "film".into(),
        tgt: "actor".into(),
        kind: "fk".into(),
        name: Some("stars".into()),
    };
    let mut films = FInstance::new()
        .with_rows("film", [(RowId(0), std::collections::HashMap::new())])
        .unwrap();
    assert!(matches!(
        films.insert_foreign_key(stars, RowId(0), RowId(99)),
        Err(InstError::DanglingForeignKey { row: RowId(99), .. })
    ));
}

/// The kind, violation and requirement of each diagnostic of a report,
/// and the kind and reason of each undecided check.
type Summary = (
    Vec<(CheckKind, String, Requirement)>,
    Vec<(CheckKind, String)>,
);

/// The kind, violation and requirement of each diagnostic of a report, and
/// the check kind and reason of each undecided check.
fn summary(report: &ValidationReport) -> Summary {
    let tag = |v: serde_json::Value| match v {
        serde_json::Value::String(s) => s,
        serde_json::Value::Object(m) => m.keys().next().cloned().unwrap_or_default(),
        other => other.to_string(),
    };
    (
        report
            .diagnostics
            .iter()
            .map(|d| {
                (
                    d.check.kind,
                    tag(serde_json::to_value(&d.violation).unwrap()),
                    d.requirement,
                )
            })
            .collect(),
        report
            .incomplete
            .iter()
            .map(|i| (i.check.kind, tag(serde_json::to_value(&i.reason).unwrap())))
            .collect(),
    )
}

/// One fixture's expected outcome: its status, and the kind and violation
/// of each of its diagnostics.
type Expectation = (
    &'static str,
    CheckStatus,
    &'static [(CheckKind, &'static str)],
);

/// What each fixture is named for.
const EXPECTED: &[Expectation] = &[
    (
        "anchors/probe",
        CheckStatus::Failed,
        &[
            (CheckKind::NodeAnchor, "unknown_anchor"),
            (CheckKind::EdgeDeclared, "undeclared_edge"),
            (CheckKind::EndpointCoherence, "incoherent_endpoint"),
            (CheckKind::RequiredEdge, "missing_required_edge"),
            (CheckKind::ValueConstraint, "value_constraint"),
        ],
    ),
    (
        "anchors/tree_unknown_root_anchor",
        CheckStatus::Failed,
        &[(CheckKind::NodeAnchor, "unknown_anchor")],
    ),
    (
        "anchors/unknown_fk_label",
        CheckStatus::Failed,
        &[(CheckKind::EdgeDeclared, "undeclared_edge")],
    ),
    (
        "anchors/unknown_table",
        CheckStatus::Failed,
        &[(CheckKind::NodeAnchor, "unknown_anchor")],
    ),
    (
        "cardinality/degree_above_upper_bound",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/degree_at_lower_bound",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/degree_at_upper_bound",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/degree_below_lower_bound",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/parallel_edges_counted",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/required_edge_absent",
        CheckStatus::Failed,
        &[(CheckKind::RequiredEdge, "missing_required_edge")],
    ),
    (
        "cardinality/required_edge_present",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/table_required_foreign_key_absent",
        CheckStatus::Failed,
        &[(CheckKind::RequiredEdge, "missing_required_edge")],
    ),
    ("cycles/acyclic_chain", CheckStatus::Passed, &[]),
    ("cycles/legal_without_declaration", CheckStatus::Passed, &[]),
    (
        "cycles/long_cycle",
        CheckStatus::Failed,
        &[(CheckKind::Acyclic, "cycle")],
    ),
    (
        "cycles/self_loop",
        CheckStatus::Failed,
        &[(CheckKind::Acyclic, "cycle")],
    ),
    (
        "cycles/tree_subtree_under_its_own_descendant",
        CheckStatus::Failed,
        &[
            (CheckKind::TreeReachable, "unreachable"),
            (CheckKind::TreeReachable, "unreachable"),
        ],
    ),
    (
        "cycles/two_cycle",
        CheckStatus::Failed,
        &[(CheckKind::Acyclic, "cycle")],
    ),
    (
        "endpoints/both_sides",
        CheckStatus::Failed,
        &[
            (CheckKind::EndpointCoherence, "incoherent_endpoint"),
            (CheckKind::EndpointCoherence, "incoherent_endpoint"),
        ],
    ),
    ("endpoints/coherent", CheckStatus::Passed, &[]),
    (
        "endpoints/self_loop_on_non_self_edge",
        CheckStatus::Failed,
        &[(CheckKind::EndpointCoherence, "incoherent_endpoint")],
    ),
    (
        "endpoints/src_side",
        CheckStatus::Failed,
        &[(CheckKind::EndpointCoherence, "incoherent_endpoint")],
    ),
    (
        "endpoints/tgt_side",
        CheckStatus::Failed,
        &[(CheckKind::EndpointCoherence, "incoherent_endpoint")],
    ),
    (
        "endpoints/tree_tgt_side",
        CheckStatus::Failed,
        &[(CheckKind::EndpointCoherence, "incoherent_endpoint")],
    ),
    (
        "fans/duplicate_label",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/foreign_incident_edge",
        CheckStatus::Failed,
        &[
            (CheckKind::EndpointCoherence, "incoherent_endpoint"),
            (CheckKind::FanWellFormed, "malformed_fan"),
        ],
    ),
    (
        "fans/missing_parent",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/multiple_parents",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/reserved_kind_outside_fan",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/tree_missing_child",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/tree_missing_parent",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/tree_unknown_hyper_edge",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/tree_unknown_label",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    ("fans/tree_well_formed", CheckStatus::Passed, &[]),
    (
        "fans/unknown_hyper_edge",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    (
        "fans/unknown_label",
        CheckStatus::Failed,
        &[(CheckKind::FanWellFormed, "malformed_fan")],
    ),
    ("fans/well_formed", CheckStatus::Passed, &[]),
    ("incomplete/acyclic_budget", CheckStatus::Incomplete, &[]),
    ("incomplete/external_missing", CheckStatus::Incomplete, &[]),
    (
        "incomplete/external_rejected",
        CheckStatus::Failed,
        &[(CheckKind::External, "external_rejected")],
    ),
    (
        "incomplete/mixed_failure_and_incomplete",
        CheckStatus::Failed,
        &[(CheckKind::NodeAnchor, "unknown_anchor")],
    ),
    ("incomplete/pattern_lookahead", CheckStatus::Incomplete, &[]),
    (
        "incomplete/source_map",
        CheckStatus::Failed,
        &[(CheckKind::ValueConstraint, "value_constraint")],
    ),
    ("incomplete/unregistered_sort", CheckStatus::Incomplete, &[]),
    (
        "keys/dangling_foreign_key_lenient",
        CheckStatus::Passed,
        &[(CheckKind::ReferenceResolves, "dangling_reference")],
    ),
    (
        "keys/dangling_foreign_key_strict",
        CheckStatus::Failed,
        &[(CheckKind::ReferenceResolves, "dangling_reference")],
    ),
    ("keys/sakila_composite_key", CheckStatus::Passed, &[]),
    (
        "keys/sakila_composite_key_duplicate",
        CheckStatus::Failed,
        &[(CheckKind::KeyUnique, "duplicate_key")],
    ),
    (
        "keys/self_referencing_foreign_key",
        CheckStatus::Passed,
        &[],
    ),
    (
        "ordering/absent_positions_allowed",
        CheckStatus::Passed,
        &[],
    ),
    ("ordering/distinct_positions", CheckStatus::Passed, &[]),
    (
        "ordering/duplicate_positions",
        CheckStatus::Failed,
        &[(CheckKind::PositionDistinct, "duplicate_position")],
    ),
    (
        "values/graph_values_and_attributes",
        CheckStatus::Failed,
        &[
            (CheckKind::ValueConstraint, "value_constraint"),
            (CheckKind::Attribute, "undeclared_attribute"),
            (CheckKind::Attribute, "mistyped_attribute"),
        ],
    ),
    (
        "values/table_values_and_attributes",
        CheckStatus::Failed,
        &[
            (CheckKind::ValueConstraint, "value_constraint"),
            (CheckKind::Attribute, "undeclared_attribute"),
            (CheckKind::Attribute, "mistyped_attribute"),
        ],
    ),
    (
        "values/tree_values_and_attributes",
        CheckStatus::Failed,
        &[
            (CheckKind::ValueConstraint, "value_constraint"),
            (CheckKind::Attribute, "undeclared_attribute"),
            (CheckKind::Attribute, "mistyped_attribute"),
        ],
    ),
];

/// What each fixture is named for: its status, and the kind and violation
/// of each diagnostic (all required unless the fixture is lenient).
#[test]
fn each_fixture_shows_what_it_is_named_for() {
    let fixtures = fixtures();
    assert_eq!(
        fixtures.keys().map(String::as_str).collect::<Vec<_>>(),
        EXPECTED.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        "every fixture has an expectation"
    );
    for (name, status, diagnostics) in EXPECTED {
        let report = &fixtures[*name].report;
        assert_eq!(report.status, *status, "{name}");
        let (found, _) = summary(report);
        let lenient = name.ends_with("_lenient");
        let wanted: Vec<(CheckKind, String, Requirement)> = diagnostics
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    (*v).to_owned(),
                    if lenient {
                        Requirement::Advisory
                    } else {
                        Requirement::Required
                    },
                )
            })
            .collect();
        assert_eq!(found, wanted, "{name}");
    }
}

fn person_edge(name: &str, tgt: &str) -> Edge {
    Edge {
        src: "person".into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// The probe: five faults, five diagnostics, each naming the node or edge
/// it is about and the schema declaration it checks.
#[test]
fn the_probe_fails_with_five_diagnostics_naming_ids_and_declarations() {
    let fixtures = fixtures();
    let report = &fixtures["anchors/probe"].report;
    assert_eq!(report.status, CheckStatus::Failed);
    let found: Vec<(CheckKind, SchemaRef, Option<ViewRef>, Violation)> = report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.check.kind,
                d.check.decl.clone(),
                d.subject.clone(),
                d.violation.clone(),
            )
        })
        .collect();
    let works_at = person_edge("worksAt", "org");
    assert_eq!(
        found,
        vec![
            (
                CheckKind::NodeAnchor,
                SchemaRef::Schema,
                Some(ViewRef::Node(4)),
                Violation::UnknownAnchor {
                    anchor: "ghost".into()
                },
            ),
            (
                CheckKind::EdgeDeclared,
                SchemaRef::Schema,
                Some(ViewRef::Edge(EdgeId(4))),
                Violation::UndeclaredEdge {
                    label: person_edge("undeclared", "org")
                },
            ),
            (
                CheckKind::EndpointCoherence,
                SchemaRef::Schema,
                Some(ViewRef::Edge(EdgeId(3))),
                Violation::IncoherentEndpoint {
                    side: EndpointSide::Tgt,
                    expected: "org".into(),
                    found: "person".into(),
                },
            ),
            (
                CheckKind::RequiredEdge,
                SchemaRef::Required {
                    vertex: "person".into(),
                    edge: works_at
                },
                Some(ViewRef::Node(3)),
                Violation::MissingRequiredEdge {
                    edge: person_edge("worksAt", "org")
                },
            ),
            (
                CheckKind::ValueConstraint,
                SchemaRef::Constraint {
                    vertex: "person".into(),
                    constraint: Constraint {
                        sort: "maxLength".into(),
                        value: "3".into()
                    },
                },
                Some(ViewRef::Node(1)),
                Violation::ValueConstraint {
                    sort: "maxLength".into(),
                    bound: "3".into(),
                    value: Some(panproto_inst::Value::Str("abcdefgh".into())),
                },
            ),
        ]
    );
    // The two parallel `worksAt` edges of node 0 are legal.
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| !d.elements.contains(&ViewRef::Edge(EdgeId(1))))
    );
}

/// Each incompleteness fixture is not `Passed` and its report is not empty,
/// for the reason it is named for.
#[test]
fn incompleteness_fixtures_are_never_passed() {
    let fixtures = fixtures();
    let cases: [(&str, &str); 7] = [
        ("incomplete/acyclic_budget", "budget"),
        ("incomplete/unregistered_sort", "unsupported_declaration"),
        ("incomplete/pattern_lookahead", "ill_formed_declaration"),
        ("incomplete/external_missing", "missing_external_witness"),
        ("incomplete/external_rejected", ""),
        (
            "incomplete/mixed_failure_and_incomplete",
            "missing_external_witness",
        ),
        ("incomplete/source_map", ""),
    ];
    for (name, reason) in cases {
        let report = &fixtures[name].report;
        assert_ne!(report.status, CheckStatus::Passed, "{name}");
        assert!(
            !report.diagnostics.is_empty() || !report.incomplete.is_empty(),
            "{name}"
        );
        if !reason.is_empty() {
            let (_, incomplete) = summary(report);
            assert!(
                incomplete.iter().any(|(_, r)| r == reason),
                "{name}: {incomplete:?}"
            );
        }
    }
    // The budget cut names its limit and leaves the acyclicity check
    // undecided.
    let budget = &fixtures["incomplete/acyclic_budget"].report;
    assert!(
        budget
            .incomplete
            .iter()
            .any(|i| i.check.kind == CheckKind::Acyclic
                && matches!(i.reason, IncompleteReason::Budget { limit: 3, .. }))
    );
    // A failure beside an undecided check fails, and lists both.
    let mixed = &fixtures["incomplete/mixed_failure_and_incomplete"].report;
    assert_eq!(mixed.status, CheckStatus::Failed);
    assert_eq!((mixed.diagnostics.len(), mixed.incomplete.len()), (1, 1));
}

/// A source map supplied with the options reaches the diagnostics of the
/// declaration it locates, and survives every encoding.
#[test]
fn a_source_map_reaches_the_diagnostics() {
    let fixtures = fixtures();
    let fixture = &fixtures["incomplete/source_map"];
    let location = SourceLocation {
        source: "schemas/name.json".into(),
        start_byte: 12,
        end_byte: 27,
    };
    assert_eq!(
        fixture.options.sources.0.values().collect::<Vec<_>>(),
        vec![&location]
    );
    assert_eq!(fixture.report.diagnostics.len(), 1);
    assert_eq!(
        fixture.report.diagnostics[0].source.as_ref(),
        Some(&location)
    );
}

/// The payloads the fan, cardinality, table-anchor and key fixtures are
/// named for: every fan violation occurs, fan edges are exempt from the edge
/// checks, parallel edges count toward an out-degree, table faults name
/// their row or pair, and the composite key names both its parts.
#[test]
fn fixture_payloads_name_what_they_check() {
    use panproto_inst::FanViolation;
    let fixtures = fixtures();
    let mut reasons = std::collections::BTreeSet::new();
    for (name, fixture) in fixtures.iter().filter(|(n, _)| n.starts_with("fans/")) {
        for d in &fixture.report.diagnostics {
            if let Violation::MalformedFan { reason, fan } = &d.violation {
                let tag = serde_json::to_value(reason).unwrap();
                reasons.insert(match tag {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Object(m) => m.keys().next().cloned().unwrap(),
                    other => other.to_string(),
                });
                // A tree diagnostic names its fan; a graph one is about the
                // fan node.
                assert_eq!(fan.is_some(), name.starts_with("fans/tree_"), "{name}");
            }
            if matches!(
                d.check.kind,
                CheckKind::EdgeDeclared | CheckKind::EndpointCoherence
            ) {
                assert_eq!(
                    *name, "fans/foreign_incident_edge",
                    "a fan edge was checked as an edge"
                );
            }
        }
        if let Instance::Graph(g) = &fixture.instance {
            assert!(
                g.edges_sorted()
                    .any(|(_, e)| e.edge.kind.as_ref().starts_with("hyper_")),
                "{name} holds fan edges"
            );
        }
    }
    let every: Vec<String> = [
        FanViolation::MissingParent,
        FanViolation::MultipleParents { edges: vec![] },
        FanViolation::DuplicateLabel {
            label: "l".into(),
            edges: vec![],
        },
        FanViolation::UnknownLabel { label: "l".into() },
        FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
        FanViolation::ForeignIncidentEdge { edge: EdgeId(0) },
        FanViolation::UnknownHyperEdge {
            hyper_edge: "h".into(),
        },
        FanViolation::MissingChild { label: "l".into() },
    ]
    .iter()
    .map(|v| match serde_json::to_value(v).unwrap() {
        serde_json::Value::String(s) => s,
        serde_json::Value::Object(m) => m.keys().next().cloned().unwrap(),
        other => other.to_string(),
    })
    .collect();
    assert_eq!(reasons.into_iter().collect::<Vec<_>>(), {
        let mut every = every;
        every.sort();
        every
    });

    let parallel = &fixtures["cardinality/parallel_edges_counted"]
        .report
        .diagnostics;
    assert_eq!(parallel.len(), 1);
    assert!(matches!(
        &parallel[0].violation,
        Violation::OutDegree { found: 2, max: Some(1), edges, .. }
            if *edges == vec![ViewRef::Edge(EdgeId(0)), ViewRef::Edge(EdgeId(1))]
    ));

    let table = &fixtures["anchors/unknown_table"].report.diagnostics[0];
    assert_eq!(table.subject, Some(ViewRef::Row(panproto_inst::RowId(1))));
    let pair = &fixtures["anchors/unknown_fk_label"].report.diagnostics[0];
    assert!(matches!(pair.subject, Some(ViewRef::Pair { .. })));

    let duplicate = &fixtures["keys/sakila_composite_key_duplicate"]
        .report
        .diagnostics[0];
    match &duplicate.violation {
        Violation::DuplicateKey { key, elems } => {
            let parts: Vec<&str> = key.iter().map(|(p, _)| p.as_str()).collect();
            assert_eq!(parts, vec!["actor_id", "film_id"]);
            assert_eq!(elems.len(), 2);
        }
        other => panic!("not a duplicate key: {other:?}"),
    }
}
