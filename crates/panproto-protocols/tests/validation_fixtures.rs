//! The frozen validation corpus under `tests/fixtures/validation/`.
//!
//! The corpus is written by the `validation-fixtures` binary; these tests
//! check that the committed files are its output, that every fixture reads
//! back the same from each of its encodings and revalidates to its stored
//! report, that every diagnostic's failure witness verifies against the
//! fixture's instance, and that each fixture shows what it is named for.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use panproto_gat::CheckStatus;
use panproto_inst::validation::{
    AdmissionPolicy, CheckInstance, CheckKind, EditValidation, EditWitness, EndpointSide,
    ExternalVerdicts, IncompleteReason, Requirement, RunVerdict, SchemaRef, SourceLocation,
    ValidationBudget, ValidationOptions, ValidationPlan, ValidationReport, Violation,
    WitnessOptions,
};
use panproto_inst::{
    BoundaryProtocols, BoundarySide, CompiledMigration, EdgeId, EditError, EditRejection,
    FInstance, GInstance, GraphEdit, RestrictError, TableEdit, TreeEdit, ValidatedGraph, ViewRef,
    WInstance, WitnessReplay, from_node_link_json, functor_extend, functor_restrict, graph_extend,
    graph_restrict, wtype_extend, wtype_restrict,
};
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

/// A directory removed, with everything under it, when this value drops,
/// so a check that fails inside it leaves nothing behind.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if !self.0.exists() {
            return;
        }
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            // A second panic while unwinding would abort the test binary.
            assert!(
                std::thread::panicking(),
                "removing {}: {e}",
                self.0.display()
            );
            eprintln!("removing {}: {e}", self.0.display());
        }
    }
}

/// Run `f` on a directory under the system temporary directory, named after
/// `tag` and this process, and remove the directory afterwards, whether `f`
/// returns or panics.
fn with_scratch_dir<T>(tag: &str, f: impl FnOnce(&Path) -> T) -> T {
    let dir = ScratchDir(std::env::temp_dir().join(format!("{tag}-{}", std::process::id())));
    f(&dir.0)
}

#[test]
fn validation_fixtures_match_the_generator() {
    let generated = with_scratch_dir("panproto-validation-fixtures", |out| {
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_validation-fixtures"))
            .arg(out)
            .status()
            .unwrap();
        assert!(status.success(), "the generator failed: {status}");
        files(out)
    });
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

/// A check that fails inside the scratch directory, after writing to it,
/// leaves no directory behind.
#[test]
fn a_failing_check_leaves_no_scratch_directory() {
    let seen = std::sync::Mutex::new(None);
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_scratch_dir("panproto-validation-scratch-probe", |dir| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("partial"), b"x").unwrap();
            *seen.lock().unwrap() = Some(dir.to_path_buf());
            panic!("the check failed");
        })
    }));
    assert!(unwound.is_err());
    let dir = seen.into_inner().unwrap().unwrap();
    assert!(!dir.exists(), "{} was left behind", dir.display());
}

/// The instance of a fixture, in its shape.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Instance {
    Graph(GInstance),
    Tree(WInstance),
    Table(FInstance),
}

/// The edits applied to a fixture's instance, as the edits of its shape.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum EditTrace {
    Graph(Vec<GraphEdit>),
    Tree(Vec<TreeEdit>),
    Table(Vec<TableEdit>),
}

/// What one edit of a trace did: the report validating the instance after
/// it gives, or the error applying it gives, which leaves the instance as
/// it was.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum EditOutcome {
    Applied(ValidationReport),
    Rejected(String),
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BoundaryOperation {
    GraphRestrict,
    GraphExtend,
    TreeRestrict,
    TreeExtend,
    TableRestrict,
    TableExtend,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BoundaryOutcome {
    Passed(Instance),
    Validation {
        side: BoundarySide,
        report: ValidationReport,
    },
    Rejected(String),
}

#[derive(Debug, Deserialize, Serialize)]
struct BoundaryTransition {
    operation: BoundaryOperation,
    target_schema: Schema,
    migration: BoundaryMigration,
    outcome: BoundaryOutcome,
}

#[derive(Debug, Deserialize, Serialize)]
struct BoundaryMigration {
    surviving_verts: std::collections::BTreeSet<panproto_gat::Name>,
    surviving_edges: std::collections::BTreeSet<Edge>,
    vertex_remap: BTreeMap<panproto_gat::Name, panproto_gat::Name>,
    edge_remap: BTreeMap<Edge, Edge>,
}

impl BoundaryMigration {
    fn compiled(&self) -> CompiledMigration {
        CompiledMigration {
            surviving_verts: self.surviving_verts.iter().cloned().collect(),
            surviving_edges: self.surviving_edges.iter().cloned().collect(),
            vertex_remap: self
                .vertex_remap
                .iter()
                .map(|(source, target)| (source.clone(), target.clone()))
                .collect(),
            edge_remap: self
                .edge_remap
                .iter()
                .map(|(source, target)| (source.clone(), target.clone()))
                .collect(),
            ..CompiledMigration::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum IncrementalOutcome {
    Applied {
        validation: EditValidation,
        witness: EditWitness,
    },
    NotAdmitted(EditValidation),
    ApplyError(String),
}

#[derive(Debug, Deserialize, Serialize)]
struct IncrementalProbe {
    edit: GraphEdit,
    policy: AdmissionPolicy,
    budget: ValidationBudget,
    external: ExternalVerdicts,
    options: WitnessOptions,
    outcome: IncrementalOutcome,
}

#[derive(Debug, Deserialize, Serialize)]
struct ViolationTuple {
    check: CheckKind,
    subject: Option<ViewRef>,
    violation: Violation,
}

#[derive(Debug, Deserialize, Serialize)]
struct ViolationSequence {
    initial: Vec<ViolationTuple>,
    after_edits: Vec<Vec<ViolationTuple>>,
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
    edit_trace: EditTrace,
    report: ValidationReport,
    edit_outcomes: Vec<EditOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boundary: Option<BoundaryTransition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    incremental: Option<IncrementalProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_violations: Option<ViolationSequence>,
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

    /// The report validating `instance` under the fixture's plan, budget
    /// and verdicts gives now.
    fn validate(&self, plan: &ValidationPlan, instance: &Instance) -> ValidationReport {
        match instance {
            Instance::Graph(g) => plan.validate_graph(g, &self.budget, &self.external),
            Instance::Tree(t) => plan.validate_tree(t, &self.budget, &self.external),
            Instance::Table(f) => plan.validate_table(f, &self.budget, &self.external),
        }
    }

    /// The report validating the fixture's instance gives now.
    fn revalidate(&self) -> ValidationReport {
        self.validate(&self.plan(), &self.instance)
    }

    /// The diagnostics of `report` that do not verify against `instance`
    /// and the verdicts.
    fn unverified(&self, report: &ValidationReport, instance: &Instance) -> Vec<String> {
        let plan = self.plan();
        report
            .diagnostics
            .iter()
            .filter(|d| !match instance {
                Instance::Graph(g) => d.verify_with_verdicts(&plan, g, &self.external),
                Instance::Tree(t) => d.verify_with_verdicts(&plan, t, &self.external),
                Instance::Table(f) => d.verify_with_verdicts(&plan, f, &self.external),
            })
            .map(|d| format!("{}: {d}", self.name))
            .collect()
    }

    /// Whether every diagnostic of the stored report verifies against the
    /// instance and the verdicts.
    fn witnesses_verify(&self) -> Vec<String> {
        self.unverified(&self.report, &self.instance)
    }

    /// The number of edits of the trace.
    fn edits(&self) -> usize {
        match &self.edit_trace {
            EditTrace::Graph(edits) => edits.len(),
            EditTrace::Tree(edits) => edits.len(),
            EditTrace::Table(edits) => edits.len(),
        }
    }

    /// The trace replayed now: after each edit, the instance and what the
    /// edit did. An edit that applies gives the report validating the
    /// instance after it; one that fails gives its error and leaves the
    /// instance as it was.
    fn replay(&self) -> Vec<(Instance, EditOutcome)> {
        let plan = self.plan();
        let mut current = self.instance.clone();
        let mut out = Vec::new();
        for k in 0..self.edits() {
            let mut next = current.clone();
            let applied = match (&mut next, &self.edit_trace) {
                (Instance::Graph(g), EditTrace::Graph(edits)) => edits[k].apply(g),
                (Instance::Tree(t), EditTrace::Tree(edits)) => edits[k].apply(t),
                (Instance::Table(f), EditTrace::Table(edits)) => edits[k].apply(f),
                (instance, trace) => panic!("{}: a trace {trace:?} of {instance:?}", self.name),
            };
            let outcome = match applied {
                Ok(()) => {
                    current = next;
                    EditOutcome::Applied(self.validate(&plan, &current))
                }
                Err(e) => EditOutcome::Rejected(e.to_string()),
            };
            out.push((current.clone(), outcome));
        }
        out
    }
}

/// What a rejection's three files share: everything but the instance,
/// which each file holds in its own encoding's wire form.
#[derive(Debug, Deserialize, Serialize)]
struct Rejection {
    name: String,
    shape: InstanceShapeKind,
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

/// A rejection's stored file read with its instance typed as `I`, so that
/// reading the file decodes the instance; the other fields are skipped.
#[derive(Deserialize)]
struct TypedRejection<I> {
    instance: I,
}

/// The three encodings a rejection is stored in.
const ENCODINGS: [&str; 3] = ["json", "named.msgpack", "cbor"];

/// The bytes of `<name>.<ext>` in the corpus.
fn corpus_bytes(name: &str, ext: &str) -> Vec<u8> {
    std::fs::read(corpus_dir().join(format!("{name}.{ext}"))).unwrap()
}

/// A JSON decoding error's message without the position `serde_json`
/// appends to it.
fn json_message(e: &serde_json::Error) -> String {
    let text = e.to_string();
    let position = format!(" at line {} column {}", e.line(), e.column());
    text.strip_suffix(&position).unwrap_or(&text).to_owned()
}

/// `bytes` decoded in the encoding `ext` names; a refusal is the error's
/// message (for JSON, without the position; for CBOR, the message a
/// semantic error carries).
fn decode<T: DeserializeOwned>(ext: &str, bytes: &[u8]) -> Result<T, String> {
    match ext {
        "json" => serde_json::from_slice(bytes).map_err(|e| json_message(&e)),
        "named.msgpack" => rmp_serde::from_slice(bytes).map_err(|e| e.to_string()),
        "cbor" => ciborium::from_reader(bytes).map_err(|e| match e {
            ciborium::de::Error::Semantic(_, message) => message,
            other => format!("malformed CBOR: {other}"),
        }),
        other => unreachable!("no encoding {other}"),
    }
}

/// `value` written in the encoding `ext` names.
fn encode<T: Serialize>(ext: &str, value: &T) -> Vec<u8> {
    match ext {
        "json" => serde_json::to_vec(value).unwrap(),
        "named.msgpack" => rmp_serde::to_vec_named(value).unwrap(),
        "cbor" => {
            let mut bytes = Vec::new();
            ciborium::into_writer(value, &mut bytes).unwrap();
            bytes
        }
        other => unreachable!("no encoding {other}"),
    }
}

/// Point every integer 99 under the key `key` in `value` back at 1,
/// undoing the redirection a rejection's instance was made with; how many
/// were changed.
fn redirect_back(value: &mut ciborium::Value, key: &str) -> usize {
    match value {
        ciborium::Value::Map(entries) => entries
            .iter_mut()
            .map(|(k, v)| {
                if k.as_text() == Some(key) && v.as_integer().is_some_and(|i| i128::from(i) == 99) {
                    *v = ciborium::Value::from(1u64);
                    1
                } else {
                    redirect_back(v, key)
                }
            })
            .sum(),
        ciborium::Value::Array(items) => items.iter_mut().map(|v| redirect_back(v, key)).sum(),
        _ => 0,
    }
}

/// Point every foreign-key pair target 99 of a table instance's wire form
/// back at row 1; how many were changed.
fn redirect_pairs_back(value: &mut ciborium::Value) -> usize {
    let mut changed = 0;
    for (k, v) in value.as_map_mut().into_iter().flatten() {
        if k.as_text() != Some("foreign_keys") {
            continue;
        }
        for entry in v.as_array_mut().into_iter().flatten() {
            let pairs = entry.as_array_mut().and_then(|e| e.get_mut(1));
            for pair in pairs
                .and_then(ciborium::Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let tgt = pair.as_array_mut().and_then(|p| p.get_mut(1));
                if let Some(tgt) =
                    tgt.filter(|t| t.as_integer().is_some_and(|i| i128::from(i) == 99))
                {
                    *tgt = ciborium::Value::from(1u64);
                    changed += 1;
                }
            }
        }
    }
    changed
}

/// Check the rejection `name` in each of its stored encodings: decoding the
/// file with its instance typed as `I` is refused with exactly `error`, and
/// the instance, with its one dangling endpoint pointed back at the node or
/// row it was taken from, decodes in that encoding to the same `I` in all
/// three. Returns that repaired instance.
fn check_stored_refusals<I: DeserializeOwned + Serialize>(
    name: &str,
    error: &str,
    repair: fn(&mut ciborium::Value) -> usize,
) -> I {
    let mut repaired: Vec<(&str, I)> = Vec::new();
    for ext in ENCODINGS {
        let bytes = corpus_bytes(name, ext);
        match decode::<TypedRejection<I>>(ext, &bytes) {
            Ok(_) => panic!("{name}.{ext}: the instance decodes"),
            Err(refusal) => assert_eq!(refusal, error, "{name}.{ext}: the refusal"),
        }
        let mut wire = decode::<TypedRejection<ciborium::Value>>(ext, &bytes)
            .unwrap_or_else(|e| panic!("{name}.{ext}: {e}"))
            .instance;
        assert_eq!(repair(&mut wire), 1, "{name}.{ext}: one dangling endpoint");
        let instance = decode::<I>(ext, &encode(ext, &wire))
            .unwrap_or_else(|e| panic!("{name}.{ext}: the repaired instance: {e}"));
        repaired.push((ext, instance));
    }
    let text = |i: &I| serde_json::to_value(i).unwrap();
    for (ext, instance) in &repaired[1..] {
        assert_eq!(
            text(instance),
            text(&repaired[0].1),
            "{name}: the {ext} and JSON files hold different instances"
        );
    }
    repaired.swap_remove(0).1
}

/// Each rejected instance is refused by decoding with exactly the error the
/// case records, in each of its stored encodings (JSON, named `MessagePack`
/// and CBOR, each file holding the instance in that encoding's own wire
/// form), and the three files hold the same instance: pointing its one
/// dangling endpoint back gives the same well-formed instance in each. A
/// graph's stored node-link form is refused by `from_node_link_json` with
/// `InstError::DanglingEdge` and the same error, and repaired the same way
/// gives the same graph.
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
        assert_eq!(rejection.name, name);
        let node_link = corpus_dir().join(format!("{name}.nodelink.json"));
        match rejection.shape {
            InstanceShapeKind::Graph => {
                let graph: GInstance =
                    check_stored_refusals(&name, &rejection.error, |v| redirect_back(v, "tgt"));
                let mut document: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&node_link).unwrap()).unwrap();
                let error = from_node_link_json(&document)
                    .err()
                    .unwrap_or_else(|| panic!("{name}.nodelink.json: the graph decodes"));
                assert!(
                    matches!(
                        error,
                        panproto_inst::InstError::DanglingEdge { endpoint: 99, .. }
                    ),
                    "{name}.nodelink.json: {error:?}"
                );
                assert_eq!(error.to_string(), rejection.error, "{name}.nodelink.json");
                let mut wire: ciborium::Value = serde_json::from_value(document.clone()).unwrap();
                assert_eq!(redirect_back(&mut wire, "tgt"), 1, "{name}.nodelink.json");
                document = serde_json::to_value(&wire).unwrap();
                let back = from_node_link_json(&document).unwrap();
                assert!(
                    GInstance::identical(&back, &graph),
                    "{name}: the node-link form holds a different graph"
                );
            }
            InstanceShapeKind::Table => {
                let _: FInstance =
                    check_stored_refusals(&name, &rejection.error, redirect_pairs_back);
                assert!(!node_link.exists(), "{name}: a node-link form for a table");
            }
            InstanceShapeKind::Tree => {
                panic!("{name}: no tree rejection is expected")
            }
        }
    }
}

/// A case's node-link forms are exactly those of its graph-shaped cases:
/// every graph fixture and every graph rejection has one, and no tree or
/// table case does.
#[test]
fn every_graph_case_and_only_a_graph_case_has_a_node_link_form() {
    let (fixture_names, rejection_names) = names();
    let mut graph_cases: Vec<String> = fixtures()
        .into_iter()
        .filter(|(_, fixture)| matches!(fixture.instance, Instance::Graph(_)))
        .map(|(name, _)| name)
        .collect();
    graph_cases.extend(rejection_names.into_iter().filter(|name| {
        let rejection: Rejection = decode_three(name);
        rejection.shape == InstanceShapeKind::Graph
    }));
    graph_cases.sort();
    let node_link: Vec<String> = files(&corpus_dir())
        .keys()
        .filter_map(|file| file.strip_suffix(".nodelink.json").map(str::to_owned))
        .collect();
    assert!(!fixture_names.is_empty());
    assert_eq!(
        node_link, graph_cases,
        "the node-link forms differ from the graph cases; regenerate with `{REGENERATE}`"
    );
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
        "anchors/tree_undeclared_label",
        CheckStatus::Failed,
        &[(CheckKind::EdgeDeclared, "undeclared_edge")],
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
        "cardinality/table_degree_above_upper_bound",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/table_degree_at_lower_bound",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/table_degree_at_upper_bound",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/table_degree_below_lower_bound",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/table_required_foreign_key_absent",
        CheckStatus::Failed,
        &[(CheckKind::RequiredEdge, "missing_required_edge")],
    ),
    (
        "cardinality/table_required_foreign_key_present",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/tree_degree_above_upper_bound",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/tree_degree_at_lower_bound",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/tree_degree_at_upper_bound",
        CheckStatus::Passed,
        &[],
    ),
    (
        "cardinality/tree_degree_below_lower_bound",
        CheckStatus::Failed,
        &[(CheckKind::OutDegree, "out_degree")],
    ),
    (
        "cardinality/tree_required_edge_absent",
        CheckStatus::Failed,
        &[(CheckKind::RequiredEdge, "missing_required_edge")],
    ),
    (
        "cardinality/tree_required_edge_present",
        CheckStatus::Passed,
        &[],
    ),
    ("cycles/acyclic_chain", CheckStatus::Passed, &[]),
    (
        "cycles/edit_breaks_cycle",
        CheckStatus::Failed,
        &[(CheckKind::Acyclic, "cycle")],
    ),
    ("cycles/edit_closes_cycle", CheckStatus::Passed, &[]),
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
        "cycles/tree_arcs_close_a_cycle_off_the_root",
        CheckStatus::Failed,
        &[
            (CheckKind::TreeReachable, "unreachable"),
            (CheckKind::TreeReachable, "unreachable"),
            (CheckKind::TreeReachable, "unreachable"),
        ],
    ),
    (
        "cycles/tree_edit_moves_subtree_under_its_own_descendant",
        CheckStatus::Passed,
        &[],
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
        "endpoints/edit_anchor_change_breaks_kept_edge",
        CheckStatus::Passed,
        &[],
    ),
    (
        "endpoints/edit_relabel_breaks_kept_edge",
        CheckStatus::Passed,
        &[],
    ),
    (
        "endpoints/incremental_repair_kept_edge",
        CheckStatus::Failed,
        &[(CheckKind::EndpointCoherence, "incoherent_endpoint")],
    ),
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
    ("endpoints/table_coherent", CheckStatus::Passed, &[]),
    (
        "endpoints/tgt_side",
        CheckStatus::Failed,
        &[(CheckKind::EndpointCoherence, "incoherent_endpoint")],
    ),
    (
        "endpoints/tree_both_sides",
        CheckStatus::Failed,
        &[
            (CheckKind::EndpointCoherence, "incoherent_endpoint"),
            (CheckKind::EndpointCoherence, "incoherent_endpoint"),
        ],
    ),
    ("endpoints/tree_coherent", CheckStatus::Passed, &[]),
    (
        "endpoints/tree_edit_anchor_change_breaks_kept_arc",
        CheckStatus::Passed,
        &[],
    ),
    (
        "endpoints/tree_edit_relabel_breaks_kept_arc",
        CheckStatus::Passed,
        &[],
    ),
    (
        "endpoints/tree_src_side",
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
    ("incomplete/table_budget", CheckStatus::Incomplete, &[]),
    (
        "incomplete/table_external_missing",
        CheckStatus::Incomplete,
        &[],
    ),
    (
        "incomplete/table_external_rejected",
        CheckStatus::Failed,
        &[(CheckKind::External, "external_rejected")],
    ),
    (
        "incomplete/table_mixed_failure_and_incomplete",
        CheckStatus::Failed,
        &[(CheckKind::ValueConstraint, "value_constraint")],
    ),
    (
        "incomplete/table_pattern_lookahead",
        CheckStatus::Incomplete,
        &[],
    ),
    (
        "incomplete/table_source_map",
        CheckStatus::Failed,
        &[(CheckKind::ValueConstraint, "value_constraint")],
    ),
    (
        "incomplete/table_unregistered_sort",
        CheckStatus::Incomplete,
        &[],
    ),
    ("incomplete/tree_budget", CheckStatus::Incomplete, &[]),
    (
        "incomplete/tree_external_missing",
        CheckStatus::Incomplete,
        &[],
    ),
    (
        "incomplete/tree_external_rejected",
        CheckStatus::Failed,
        &[(CheckKind::External, "external_rejected")],
    ),
    (
        "incomplete/tree_mixed_failure_and_incomplete",
        CheckStatus::Failed,
        &[(CheckKind::ValueConstraint, "value_constraint")],
    ),
    (
        "incomplete/tree_pattern_lookahead",
        CheckStatus::Incomplete,
        &[],
    ),
    (
        "incomplete/tree_source_map",
        CheckStatus::Failed,
        &[(CheckKind::ValueConstraint, "value_constraint")],
    ),
    (
        "incomplete/tree_unregistered_sort",
        CheckStatus::Incomplete,
        &[],
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
    (
        "keys/graph_dangling_foreign_key_lenient",
        CheckStatus::Failed,
        &[
            (CheckKind::RequiredEdge, "missing_required_edge"),
            (CheckKind::ReferenceResolves, "dangling_reference"),
        ],
    ),
    (
        "keys/graph_dangling_foreign_key_strict",
        CheckStatus::Failed,
        &[
            (CheckKind::RequiredEdge, "missing_required_edge"),
            (CheckKind::ReferenceResolves, "dangling_reference"),
        ],
    ),
    ("keys/graph_sakila_composite_key", CheckStatus::Passed, &[]),
    (
        "keys/graph_sakila_composite_key_duplicate",
        CheckStatus::Failed,
        &[(CheckKind::KeyUnique, "duplicate_key")],
    ),
    (
        "keys/graph_self_referencing_foreign_key",
        CheckStatus::Passed,
        &[],
    ),
    (
        "keys/graph_within_nested_maps_accept_an_equal_key",
        CheckStatus::Passed,
        &[],
    ),
    (
        "keys/graph_within_no_enclosing_map_is_outside_the_key",
        CheckStatus::Passed,
        &[
            (CheckKind::KeyUnique, "outside_key_scope"),
            (CheckKind::KeyUnique, "outside_key_scope"),
        ],
    ),
    (
        "keys/graph_within_one_map_rejects_a_repeated_key",
        CheckStatus::Failed,
        &[(CheckKind::KeyUnique, "duplicate_key")],
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
        "keys/tree_dangling_foreign_key_lenient",
        CheckStatus::Passed,
        &[(CheckKind::ReferenceResolves, "dangling_reference")],
    ),
    (
        "keys/tree_dangling_foreign_key_strict",
        CheckStatus::Failed,
        &[(CheckKind::ReferenceResolves, "dangling_reference")],
    ),
    ("keys/tree_sakila_composite_key", CheckStatus::Passed, &[]),
    (
        "keys/tree_sakila_composite_key_duplicate",
        CheckStatus::Failed,
        &[(CheckKind::KeyUnique, "duplicate_key")],
    ),
    (
        "keys/tree_self_referencing_foreign_key",
        CheckStatus::Passed,
        &[],
    ),
    (
        "keys/within_nested_maps_accept_an_equal_key",
        CheckStatus::Passed,
        &[],
    ),
    (
        "keys/within_no_enclosing_map_is_outside_the_key",
        CheckStatus::Passed,
        &[(CheckKind::KeyUnique, "outside_key_scope")],
    ),
    (
        "keys/within_one_map_rejects_a_repeated_key",
        CheckStatus::Failed,
        &[(CheckKind::KeyUnique, "duplicate_key")],
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
        "ordering/edit_reorder_creates_and_repairs_duplicate",
        CheckStatus::Passed,
        &[],
    ),
    (
        "ordering/incremental_repair_duplicate",
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
/// of each diagnostic (all required except the reference diagnostics of a
/// lenient fixture, which are advisory).
#[test]
fn each_fixture_shows_what_it_is_named_for() {
    let fixtures = fixtures();
    let ordinary: Vec<&str> = fixtures
        .keys()
        .map(String::as_str)
        .filter(|name| !name.starts_with("boundary/") && !name.starts_with("train_benchmark/"))
        .collect();
    assert_eq!(
        ordinary,
        EXPECTED.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        "every ordinary fixture has an expectation"
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
                    // A lenient reference's failures and an element outside
                    // a key's scope are reported advisory.
                    if (lenient && *k == CheckKind::ReferenceResolves) || *v == "outside_key_scope"
                    {
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
        ("acyclic_budget", "budget"),
        ("unregistered_sort", "unsupported_declaration"),
        ("pattern_lookahead", "ill_formed_declaration"),
        ("external_missing", "missing_external_witness"),
        ("external_rejected", ""),
        ("mixed_failure_and_incomplete", "missing_external_witness"),
        ("source_map", ""),
    ];
    // Each case in each shape: the tree and table budgets run out on the
    // checks those shapes have, acyclicity being a graph check.
    let shaped = cases.into_iter().flat_map(|(case, reason)| {
        let other = if case == "acyclic_budget" {
            "budget"
        } else {
            case
        };
        [
            format!("incomplete/{case}"),
            format!("incomplete/tree_{other}"),
            format!("incomplete/table_{other}"),
        ]
        .map(|name| (name, reason))
    });
    let mut seen = 0;
    for (name, reason) in shaped {
        seen += 1;
        let name = name.as_str();
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
    assert_eq!(
        seen,
        fixtures
            .keys()
            .filter(|n| n.starts_with("incomplete/"))
            .count(),
        "every incompleteness fixture is checked"
    );
    for name in ["incomplete/tree_budget", "incomplete/table_budget"] {
        assert!(
            fixtures[name]
                .report
                .incomplete
                .iter()
                .all(|i| matches!(i.reason, IncompleteReason::Budget { limit: 3, .. })),
            "{name}"
        );
    }
    for name in [
        "incomplete/tree_mixed_failure_and_incomplete",
        "incomplete/table_mixed_failure_and_incomplete",
    ] {
        let mixed = &fixtures[name].report;
        assert_eq!(mixed.status, CheckStatus::Failed, "{name}");
        assert_eq!(
            (mixed.diagnostics.len(), mixed.incomplete.len()),
            (1, 1),
            "{name}"
        );
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
    let location = SourceLocation {
        source: "schemas/name.json".into(),
        start_byte: 12,
        end_byte: 27,
    };
    for name in [
        "incomplete/source_map",
        "incomplete/tree_source_map",
        "incomplete/table_source_map",
    ] {
        let fixture = &fixtures[name];
        assert_eq!(
            fixture.options.sources.0.values().collect::<Vec<_>>(),
            vec![&location],
            "{name}"
        );
        assert_eq!(fixture.report.diagnostics.len(), 1, "{name}");
        assert_eq!(
            fixture.report.diagnostics[0].source.as_ref(),
            Some(&location),
            "{name}"
        );
    }
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

/// The coherent endpoint cases pass because their one edge, arc or pair is
/// checked and found coherent, not because nothing is checked.
#[test]
fn the_coherent_endpoint_cases_check_their_one_edge() {
    let fixtures = fixtures();
    for name in [
        "endpoints/coherent",
        "endpoints/tree_coherent",
        "endpoints/table_coherent",
    ] {
        let coverage: Vec<(u64, u64)> = fixtures[name]
            .report
            .coverage
            .iter()
            .filter(|(key, _)| key.kind == CheckKind::EndpointCoherence)
            .map(|(_, c)| (c.instances, c.passed))
            .collect();
        assert_eq!(coverage, vec![(1, 1)], "{name}");
    }
}

/// What one edit of a trace shows: the status of the report after it and
/// the kind and violation of each of its diagnostics, or, where applying the
/// edit fails, the error it fails with, built here rather than read from the
/// corpus.
type StepExpectation =
    Result<(CheckStatus, &'static [(CheckKind, &'static str)]), fn() -> EditError>;

/// The refusal to move node 1 under its own descendant, node 2.
const fn moves_node_1_under_its_descendant_2() -> EditError {
    EditError::CycleDetected {
        node_id: 1,
        new_parent: 2,
    }
}

/// The diagnostics of an incoherent endpoint on one side.
const ONE_SIDE: &[(CheckKind, &str)] = &[(CheckKind::EndpointCoherence, "incoherent_endpoint")];

/// The diagnostics of incoherent endpoints on both sides.
const BOTH_SIDES: &[(CheckKind, &str)] = &[
    (CheckKind::EndpointCoherence, "incoherent_endpoint"),
    (CheckKind::EndpointCoherence, "incoherent_endpoint"),
];

/// The cases whose instance is edited after it is validated, and what each
/// edit of the trace shows.
const EDITED: &[(&str, &[StepExpectation])] = &[
    (
        "cycles/edit_breaks_cycle",
        &[Ok((CheckStatus::Passed, &[]))],
    ),
    (
        "cycles/edit_closes_cycle",
        &[Ok((CheckStatus::Failed, &[(CheckKind::Acyclic, "cycle")]))],
    ),
    (
        "cycles/tree_edit_moves_subtree_under_its_own_descendant",
        &[Err(moves_node_1_under_its_descendant_2)],
    ),
    (
        "endpoints/incremental_repair_kept_edge",
        &[Ok((CheckStatus::Passed, &[]))],
    ),
    (
        "endpoints/edit_anchor_change_breaks_kept_edge",
        &[Ok((CheckStatus::Failed, ONE_SIDE))],
    ),
    (
        "endpoints/edit_relabel_breaks_kept_edge",
        &[Ok((CheckStatus::Failed, BOTH_SIDES))],
    ),
    (
        "endpoints/tree_edit_anchor_change_breaks_kept_arc",
        &[Ok((CheckStatus::Failed, ONE_SIDE))],
    ),
    (
        "endpoints/tree_edit_relabel_breaks_kept_arc",
        &[Ok((CheckStatus::Failed, BOTH_SIDES))],
    ),
    (
        "ordering/incremental_repair_duplicate",
        &[Ok((CheckStatus::Passed, &[]))],
    ),
    (
        "ordering/edit_reorder_creates_and_repairs_duplicate",
        &[
            Ok((
                CheckStatus::Failed,
                &[(CheckKind::PositionDistinct, "duplicate_position")],
            )),
            Ok((CheckStatus::Passed, &[])),
        ],
    ),
];

/// Every fixture stores the edits applied to its instance as a trace of
/// edits of the instance's own shape, with one outcome per edit. The
/// trace is empty exactly in the cases that validate their instance alone;
/// in the edit cases, each edit shows what the case is named for.
#[test]
fn every_fixture_stores_an_edit_trace_of_its_shape() {
    let fixtures = fixtures();
    let edited: BTreeMap<&str, &[StepExpectation]> = EDITED.iter().copied().collect();
    for name in edited.keys() {
        assert!(fixtures.contains_key(*name), "{name} is not in the corpus");
    }
    for (name, fixture) in &fixtures {
        match (&fixture.instance, &fixture.edit_trace) {
            (Instance::Graph(_), EditTrace::Graph(_))
            | (Instance::Tree(_), EditTrace::Tree(_))
            | (Instance::Table(_), EditTrace::Table(_)) => {}
            (instance, trace) => panic!("{name}: a trace {trace:?} of {instance:?}"),
        }
        assert_eq!(
            fixture.edit_outcomes.len(),
            fixture.edits(),
            "{name}: one outcome per edit"
        );
        let steps = edited.get(name.as_str()).copied().unwrap_or(&[]);
        if name.starts_with("train_benchmark/") {
            assert_eq!(fixture.edits(), 1, "{name}: one inject or repair edit");
            continue;
        }
        assert_eq!(fixture.edits(), steps.len(), "{name}: the number of edits");
        for (k, (outcome, step)) in fixture.edit_outcomes.iter().zip(steps).enumerate() {
            match (outcome, step) {
                (EditOutcome::Applied(report), Ok((status, diagnostics))) => {
                    assert_eq!(report.status, *status, "{name}, edit {k}");
                    let wanted: Vec<(CheckKind, String, Requirement)> = diagnostics
                        .iter()
                        .map(|(kind, v)| (*kind, (*v).to_owned(), Requirement::Required))
                        .collect();
                    assert_eq!(summary(report).0, wanted, "{name}, edit {k}");
                }
                (EditOutcome::Rejected(reason), Err(refusal)) => {
                    assert_eq!(reason, &refusal().to_string(), "{name}, edit {k}");
                }
                (outcome, step) => panic!("{name}, edit {k}: {outcome:?}, expected {step:?}"),
            }
        }
    }
}

/// A tree whose arcs close a cycle away from the root has one `Unreachable`
/// diagnostic at each node of the cycle and none at the root's child.
#[test]
fn tree_arcs_closing_a_cycle_off_the_root_leave_each_cycle_node_unreachable() {
    let fixtures = fixtures();
    let fixture = &fixtures["cycles/tree_arcs_close_a_cycle_off_the_root"];
    let Instance::Tree(tree) = &fixture.instance else {
        panic!("a tree case");
    };
    assert_eq!(tree.root, 0);
    assert_eq!(fixture.report.status, CheckStatus::Failed);
    let unreachable: Vec<(CheckKind, Option<ViewRef>, Violation)> = fixture
        .report
        .diagnostics
        .iter()
        .map(|d| (d.check.kind, d.subject.clone(), d.violation.clone()))
        .collect();
    assert_eq!(
        unreachable,
        [1, 2, 3]
            .map(|n| (
                CheckKind::TreeReachable,
                Some(ViewRef::Node(n)),
                Violation::Unreachable
            ))
            .to_vec()
    );
}

/// The tree arc of `endpoints/tree_edit_relabel_breaks_kept_arc` is kept by
/// its identity in the tree, the pair (parent, child), while the graph
/// lowering of its move deletes the arc's edge and adds the relabelled one
/// under a fresh edge id, with no label change of a kept edge.
#[test]
fn a_kept_arc_moved_under_its_parent_lowers_to_a_fresh_edge() {
    let fixtures = fixtures();
    let fixture = &fixtures["endpoints/tree_edit_relabel_breaks_kept_arc"];
    let (Instance::Tree(tree), EditTrace::Tree(edits)) = (&fixture.instance, &fixture.edit_trace)
    else {
        panic!("a tree case with a tree trace");
    };
    assert_eq!(edits.len(), 1);
    let mut after = tree.clone();
    edits[0].apply(&mut after).unwrap();
    let arcs =
        |t: &WInstance| -> Vec<(u32, u32)> { t.arcs.iter().map(|(p, c, _)| (*p, *c)).collect() };
    assert_eq!(arcs(tree), vec![(0, 1)]);
    assert_eq!(arcs(&after), vec![(0, 1)], "the arc (0, 1) is kept");
    let (graph, mut embedding) = GInstance::from_wtype_embedded(tree, None).unwrap();
    let parts = panproto_inst::GraphDelta::from_tree_edit(&graph, &mut embedding, &edits[0])
        .unwrap()
        .into_parts();
    let deleted: Vec<(EdgeId, u32, u32, String)> = parts
        .del_edges
        .iter()
        .map(|(id, e)| (*id, e.src, e.tgt, e.edge.name.as_ref().unwrap().to_string()))
        .collect();
    let added: Vec<(EdgeId, u32, u32, String)> = parts
        .add_edges
        .iter()
        .map(|(id, e)| (*id, e.src, e.tgt, e.edge.name.as_ref().unwrap().to_string()))
        .collect();
    assert_eq!(deleted.len(), 1, "{deleted:?}");
    assert_eq!(added.len(), 1, "{added:?}");
    assert_ne!(deleted[0].0, added[0].0, "a fresh edge id");
    assert_eq!(
        (deleted[0].1, deleted[0].2, deleted[0].3.as_str()),
        (0, 1, "ab")
    );
    assert_eq!((added[0].1, added[0].2, added[0].3.as_str()), (0, 1, "ba"));
    assert!(
        parts
            .attrs
            .keys()
            .all(|k| !matches!(k, panproto_inst::graph_edit::AttrKey::EdgeLabel { .. })),
        "{:?}",
        parts.attrs
    );
}

/// Replaying each fixture's trace now gives its stored outcomes byte for
/// byte, and every diagnostic of the report after an edit verifies against
/// the instance after it.
#[test]
fn edit_traces_replay_to_their_outcomes() {
    let mut unverified = Vec::new();
    for (name, fixture) in fixtures() {
        let replayed = fixture.replay();
        assert_eq!(replayed.len(), fixture.edit_outcomes.len(), "{name}");
        for (k, ((instance, now), stored)) in
            replayed.iter().zip(&fixture.edit_outcomes).enumerate()
        {
            match (now, stored) {
                (EditOutcome::Applied(now), EditOutcome::Applied(stored)) => {
                    assert_eq!(
                        now.canonical_bytes(),
                        stored.canonical_bytes(),
                        "{name}, edit {k}: regenerate with `{REGENERATE}`"
                    );
                    unverified.extend(fixture.unverified(stored, instance));
                }
                (EditOutcome::Rejected(now), EditOutcome::Rejected(stored)) => {
                    assert_eq!(now, stored, "{name}, edit {k}");
                }
                (now, stored) => panic!("{name}, edit {k}: {now:?}, stored {stored:?}"),
            }
        }
    }
    assert!(
        unverified.is_empty(),
        "witnesses that do not verify: {unverified:#?}"
    );
}

fn boundary_outcome<T>(
    result: Result<T, RestrictError>,
    wrap: impl FnOnce(T) -> Instance,
) -> BoundaryOutcome {
    match result {
        Ok(output) => BoundaryOutcome::Passed(wrap(output)),
        Err(RestrictError::Validation { side, report }) => BoundaryOutcome::Validation {
            side,
            report: *report,
        },
        Err(error) => BoundaryOutcome::Rejected(error.to_string()),
    }
}

fn recompute_boundary(fixture: &Fixture, transition: &BoundaryTransition) -> BoundaryOutcome {
    let protocol = fixture.protocol();
    let migration = transition.migration.compiled();
    let protocols = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    match (transition.operation, &fixture.instance) {
        (BoundaryOperation::GraphRestrict, Instance::Graph(graph)) => boundary_outcome(
            graph_restrict(
                graph,
                &fixture.schema,
                &transition.target_schema,
                &migration,
                protocols,
            ),
            Instance::Graph,
        ),
        (BoundaryOperation::GraphExtend, Instance::Graph(graph)) => boundary_outcome(
            graph_extend(
                graph,
                &fixture.schema,
                &transition.target_schema,
                &migration,
                protocols,
            ),
            Instance::Graph,
        ),
        (BoundaryOperation::TreeRestrict, Instance::Tree(tree)) => boundary_outcome(
            wtype_restrict(
                tree,
                &fixture.schema,
                &transition.target_schema,
                &migration,
                protocols,
            ),
            Instance::Tree,
        ),
        (BoundaryOperation::TreeExtend, Instance::Tree(tree)) => boundary_outcome(
            wtype_extend(
                tree,
                &fixture.schema,
                &transition.target_schema,
                &migration,
                protocols,
            ),
            Instance::Tree,
        ),
        (BoundaryOperation::TableRestrict, Instance::Table(table)) => boundary_outcome(
            functor_restrict(
                table,
                &fixture.schema,
                &transition.target_schema,
                &migration,
                protocols,
            ),
            Instance::Table,
        ),
        (BoundaryOperation::TableExtend, Instance::Table(table)) => boundary_outcome(
            functor_extend(
                table,
                &fixture.schema,
                &transition.target_schema,
                &migration,
                protocols,
            ),
            Instance::Table,
        ),
        pair => panic!("{}: mismatched boundary operation {pair:?}", fixture.name),
    }
}

fn assert_boundary_outcome(name: &str, stored: &BoundaryOutcome, now: &BoundaryOutcome) {
    match (stored, now) {
        (BoundaryOutcome::Passed(stored), BoundaryOutcome::Passed(now)) => assert_eq!(
            serde_json::to_value(stored).unwrap(),
            serde_json::to_value(now).unwrap(),
            "{name}: boundary output"
        ),
        (
            BoundaryOutcome::Validation {
                side: stored_side,
                report: stored_report,
            },
            BoundaryOutcome::Validation {
                side: now_side,
                report: now_report,
            },
        ) => {
            assert_eq!(stored_side, now_side, "{name}: boundary side");
            assert_eq!(
                stored_report.canonical_bytes(),
                now_report.canonical_bytes(),
                "{name}: boundary report"
            );
        }
        (BoundaryOutcome::Rejected(stored), BoundaryOutcome::Rejected(now)) => {
            assert_eq!(stored, now, "{name}: boundary rejection");
        }
        pair => panic!("{name}: boundary outcome changed: {pair:?}"),
    }
}

#[test]
fn boundary_fixtures_replay_restrict_and_extend_for_every_shape() {
    let fixtures = fixtures();
    let boundary: Vec<_> = fixtures
        .iter()
        .filter(|(name, _)| name.starts_with("boundary/"))
        .collect();
    assert_eq!(boundary.len(), 6);
    for (name, fixture) in boundary {
        let transition = fixture.boundary.as_ref().expect("a boundary transition");
        let now = recompute_boundary(fixture, transition);
        assert_boundary_outcome(name, &transition.outcome, &now);
        let BoundaryOutcome::Validation { side, report } = &transition.outcome else {
            panic!("{name}: this corpus pins an output validation failure");
        };
        assert_eq!(*side, BoundarySide::Output, "{name}");
        assert_ne!(report.status, CheckStatus::Passed, "{name}");
    }
}

fn violation_tuples(report: &ValidationReport) -> Vec<(CheckKind, Option<ViewRef>, Violation)> {
    report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.check.kind,
                diagnostic.subject.clone(),
                diagnostic.violation.clone(),
            )
        })
        .collect()
}

fn stored_tuples(tuples: &[ViolationTuple]) -> Vec<(CheckKind, Option<ViewRef>, Violation)> {
    tuples
        .iter()
        .map(|tuple| (tuple.check, tuple.subject.clone(), tuple.violation.clone()))
        .collect()
}

fn replay_incremental_probe<'a>(name: &str, fixture: &'a Fixture) -> &'a EditWitness {
    let probe = fixture.incremental.as_ref().expect("incremental probe");
    let Instance::Graph(graph) = &fixture.instance else {
        panic!("{name}: incremental corpus probes are graphs");
    };
    let plan = Arc::new(fixture.plan());
    let (mut validated, _) = ValidatedGraph::new(
        plan.clone(),
        graph.clone(),
        &fixture.budget,
        &fixture.external,
    );
    let baseline = validated.state().clone();
    let prior = validated.instance().clone();
    let delta = probe.edit.to_delta().expect("stored edit lowers");
    let dry = validated
        .check_edit(&probe.edit, &probe.budget, &probe.external)
        .expect("stored edit applies");
    let actual = validated.apply_edit_with_options(
        &probe.edit,
        probe.policy,
        &probe.budget,
        &probe.external,
        &probe.options,
    );
    let IncrementalOutcome::Applied {
        validation,
        witness,
    } = &probe.outcome
    else {
        panic!("{name}: stored edit is admitted");
    };
    assert_eq!(
        rmp_serde::to_vec_named(validation).unwrap(),
        rmp_serde::to_vec_named(&dry).unwrap(),
        "{name}: dry-run validation"
    );
    let actual = actual.expect("stored edit remains admitted");
    assert_eq!(
        actual.canonical_bytes(),
        witness.canonical_bytes(),
        "{name}"
    );
    witness
        .verify(&WitnessReplay {
            plan: &plan,
            baseline: &baseline,
            prior: &prior,
            delta: &delta,
            policy: probe.policy,
            budget: &probe.budget,
            external: &probe.external,
            expected_options: probe.options,
        })
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    witness
}

fn replay_rejected_incremental_probe<'a>(name: &str, fixture: &'a Fixture) -> &'a EditValidation {
    let probe = fixture.incremental.as_ref().expect("incremental probe");
    let Instance::Graph(graph) = &fixture.instance else {
        panic!("{name}: incremental corpus probes are graphs");
    };
    let plan = Arc::new(fixture.plan());
    let (mut validated, _) =
        ValidatedGraph::new(plan, graph.clone(), &fixture.budget, &fixture.external);
    let dry = validated
        .check_edit(&probe.edit, &probe.budget, &probe.external)
        .expect("stored edit applies");
    let actual = validated.apply_edit_with_options(
        &probe.edit,
        probe.policy,
        &probe.budget,
        &probe.external,
        &probe.options,
    );
    let IncrementalOutcome::NotAdmitted(stored) = &probe.outcome else {
        panic!("{name}: stored edit is rejected by admission");
    };
    assert_eq!(
        rmp_serde::to_vec_named(&dry).unwrap(),
        rmp_serde::to_vec_named(stored).unwrap(),
        "{name}: stored dry-run validation"
    );
    let Err(EditRejection::NotAdmitted { validation }) = actual else {
        panic!("{name}: replayed edit is rejected by admission");
    };
    assert_eq!(
        rmp_serde::to_vec_named(&validation).unwrap(),
        rmp_serde::to_vec_named(stored).unwrap(),
        "{name}: replayed rejected validation"
    );
    stored
}

fn fixture_check_instance(
    fixture: &Fixture,
    kind: CheckKind,
    subject: Option<ViewRef>,
) -> CheckInstance {
    let plan = fixture.plan();
    let mut checks = plan.checks().keys().filter(|check| check.kind == kind);
    let check = checks
        .next()
        .cloned()
        .unwrap_or_else(|| panic!("{}: plan contains {kind:?}", fixture.name));
    assert!(
        checks.next().is_none(),
        "{}: plan contains one {kind:?} check",
        fixture.name
    );
    CheckInstance { check, subject }
}

#[test]
fn endpoint_ordering_and_cycle_repairs_have_replayable_incremental_witnesses() {
    let fixtures = fixtures();
    for name in [
        "endpoints/incremental_repair_kept_edge",
        "ordering/incremental_repair_duplicate",
        "ordering/edit_reorder_creates_and_repairs_duplicate",
        "cycles/edit_breaks_cycle",
    ] {
        let witness = replay_incremental_probe(name, &fixtures[name]);
        assert!(witness.prior.is_some(), "{name}: prior digest");
        assert!(witness.result.is_some(), "{name}: result digest");
        assert!(!witness.runs.is_empty(), "{name}: scheduled runs");
    }
}

#[test]
fn ordering_witnesses_retain_the_complete_position_run() {
    let fixtures = fixtures();
    for name in [
        "ordering/incremental_repair_duplicate",
        "ordering/edit_reorder_creates_and_repairs_duplicate",
    ] {
        let fixture = &fixtures[name];
        let expected =
            fixture_check_instance(fixture, CheckKind::PositionDistinct, Some(ViewRef::Node(0)));
        let witness = replay_incremental_probe(name, fixture);
        let [run] = witness.runs.as_slice() else {
            panic!("{name}: ordering edit schedules exactly one run");
        };
        assert_eq!(
            run.instance, expected,
            "{name}: exact PositionDistinct run identity"
        );
        assert_eq!(
            run.reads,
            vec![
                ViewRef::Node(0),
                ViewRef::Edge(EdgeId(0)),
                ViewRef::Edge(EdgeId(1)),
            ],
            "{name}: canonical ordering dependencies"
        );
        let RunVerdict::Evaluated {
            findings,
            incomplete,
        } = &run.verdict
        else {
            panic!("{name}: ordering run is evaluated");
        };
        assert!(findings.is_empty(), "{name}: repaired ordering findings");
        assert!(
            incomplete.is_empty(),
            "{name}: repaired ordering incomplete reasons"
        );
    }
}

fn assert_cycle_close_footprint(
    name: &str,
    fixture: &Fixture,
    close: &EditValidation,
    acyclic: &CheckInstance,
) {
    let endpoint_0 = fixture_check_instance(
        fixture,
        CheckKind::EndpointCoherence,
        Some(ViewRef::Edge(EdgeId(0))),
    );
    let endpoint_1 = fixture_check_instance(
        fixture,
        CheckKind::EndpointCoherence,
        Some(ViewRef::Edge(EdgeId(1))),
    );
    let endpoint_2 = fixture_check_instance(
        fixture,
        CheckKind::EndpointCoherence,
        Some(ViewRef::Edge(EdgeId(2))),
    );
    let declared_2 = fixture_check_instance(
        fixture,
        CheckKind::EdgeDeclared,
        Some(ViewRef::Edge(EdgeId(2))),
    );
    assert_eq!(
        close.footprint.rerun,
        BTreeSet::from([endpoint_0, endpoint_1, acyclic.clone()]),
        "{name}: exact rerun set"
    );
    assert_eq!(
        close.footprint.created,
        BTreeSet::from([declared_2, endpoint_2]),
        "{name}: exact created set"
    );
    assert!(
        close.footprint.retired.is_empty(),
        "{name}: no retired checks"
    );
    let all_cycle_elements = BTreeSet::from([
        ViewRef::Node(0),
        ViewRef::Node(1),
        ViewRef::Node(2),
        ViewRef::Edge(EdgeId(0)),
        ViewRef::Edge(EdgeId(1)),
        ViewRef::Edge(EdgeId(2)),
    ]);
    assert_eq!(
        close.footprint.reads, all_cycle_elements,
        "{name}: exact whole-instance dependency union"
    );
    assert_eq!(
        close.footprint.touched,
        BTreeSet::from([ViewRef::Node(0), ViewRef::Node(2), ViewRef::Edge(EdgeId(2)),]),
        "{name}: exact delta support"
    );
    assert_eq!(
        close.affected,
        BTreeSet::from([
            ViewRef::Node(0),
            ViewRef::Node(2),
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Edge(EdgeId(1)),
            ViewRef::Edge(EdgeId(2)),
        ]),
        "{name}: exact affected carrier"
    );
}

fn assert_cycle_close_report(
    name: &str,
    fixture: &Fixture,
    close: &EditValidation,
    acyclic: &CheckInstance,
) {
    assert_eq!(
        close.report.status,
        CheckStatus::Failed,
        "{name}: closing a cycle fails"
    );
    assert!(
        close.report.incomplete.is_empty(),
        "{name}: cycle result is complete"
    );
    let [diagnostic] = close.report.diagnostics.as_slice() else {
        panic!("{name}: exactly one cycle diagnostic");
    };
    assert_eq!(
        diagnostic.check, acyclic.check,
        "{name}: exact Acyclic diagnostic identity"
    );
    assert_eq!(
        diagnostic.subject, None,
        "{name}: Acyclic is a whole-instance check"
    );
    assert_eq!(
        diagnostic.elements,
        vec![
            ViewRef::Node(0),
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Edge(EdgeId(1)),
            ViewRef::Edge(EdgeId(2)),
        ],
        "{name}: canonical cycle elements"
    );
    assert_eq!(
        diagnostic.violation,
        Violation::Cycle {
            scc: vec![ViewRef::Node(0), ViewRef::Node(1), ViewRef::Node(2)],
            witness: vec![EdgeId(0), EdgeId(1), EdgeId(2)],
        },
        "{name}: exact cycle witness"
    );

    let probe = fixture
        .incremental
        .as_ref()
        .expect("cycle-closing incremental probe");
    let Instance::Graph(graph) = &fixture.instance else {
        panic!("{name}: graph fixture");
    };
    let mut target = graph.clone();
    probe.edit.apply(&mut target).unwrap();
    let mut full = fixture
        .plan()
        .validate_graph(&target, &probe.budget, &probe.external);
    full.scope = close.report.scope.clone();
    assert_eq!(
        close.report.canonical_bytes(),
        full.canonical_bytes(),
        "{name}: rejected incremental report equals full validation"
    );
}

fn assert_cycle_break(name: &str, fixture: &Fixture) {
    let expected = fixture_check_instance(fixture, CheckKind::Acyclic, None);
    let witness = replay_incremental_probe(name, fixture);
    let [run] = witness.runs.as_slice() else {
        panic!("{name}: breaking a cycle schedules exactly one run");
    };
    assert_eq!(run.instance, expected, "{name}: exact Acyclic run identity");
    assert_eq!(
        run.reads,
        vec![
            ViewRef::Node(0),
            ViewRef::Node(1),
            ViewRef::Node(2),
            ViewRef::Edge(EdgeId(0)),
            ViewRef::Edge(EdgeId(1)),
        ],
        "{name}: complete post-edit Acyclic trace"
    );
    let RunVerdict::Evaluated {
        findings,
        incomplete,
    } = &run.verdict
    else {
        panic!("{name}: Acyclic run is evaluated");
    };
    assert!(findings.is_empty(), "{name}: cycle is repaired");
    assert!(
        incomplete.is_empty(),
        "{name}: repaired cycle result is complete"
    );
}

#[test]
fn cycle_edits_retain_complete_whole_instance_results() {
    let fixtures = fixtures();
    let close_name = "cycles/edit_closes_cycle";
    let close_fixture = &fixtures[close_name];
    let acyclic = fixture_check_instance(close_fixture, CheckKind::Acyclic, None);
    let close = replay_rejected_incremental_probe(close_name, close_fixture);
    assert_cycle_close_footprint(close_name, close_fixture, close, &acyclic);
    assert_cycle_close_report(close_name, close_fixture, close, &acyclic);
    assert_cycle_break(
        "cycles/edit_breaks_cycle",
        &fixtures["cycles/edit_breaks_cycle"],
    );
}

#[test]
fn endpoint_and_cycle_breaking_directions_store_incremental_results() {
    let fixtures = fixtures();
    for (name, kind) in [
        (
            "endpoints/edit_anchor_change_breaks_kept_edge",
            CheckKind::EndpointCoherence,
        ),
        (
            "endpoints/edit_relabel_breaks_kept_edge",
            CheckKind::EndpointCoherence,
        ),
        ("cycles/edit_closes_cycle", CheckKind::Acyclic),
    ] {
        let probe = fixtures[name]
            .incremental
            .as_ref()
            .expect("breaking direction has an incremental result");
        let IncrementalOutcome::NotAdmitted(validation) = &probe.outcome else {
            panic!("{name}: breaking edit is rejected by admission");
        };
        assert!(
            validation
                .report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.check.kind == kind),
            "{name}: rejected target reports {kind:?}"
        );
        assert!(
            validation
                .footprint
                .reads
                .iter()
                .any(|read| matches!(read, ViewRef::Edge(_))),
            "{name}: incremental footprint retains edge dependencies"
        );
    }
}

#[test]
fn train_benchmark_sequences_retain_multiplicity_and_replay_witnesses() {
    let fixtures = fixtures();
    let train: Vec<_> = fixtures
        .iter()
        .filter(|(name, _)| name.starts_with("train_benchmark/"))
        .collect();
    assert_eq!(train.len(), 6);
    for (name, fixture) in train {
        let expected = fixture
            .expected_violations
            .as_ref()
            .expect("train cases store violation tuples");
        assert_eq!(
            stored_tuples(&expected.initial),
            violation_tuples(&fixture.report),
            "{name}"
        );
        assert_eq!(
            expected.after_edits.len(),
            fixture.edit_outcomes.len(),
            "{name}"
        );
        for (stored, outcome) in expected.after_edits.iter().zip(&fixture.edit_outcomes) {
            let now = match outcome {
                EditOutcome::Applied(report) => violation_tuples(report),
                EditOutcome::Rejected(_) => Vec::new(),
            };
            assert_eq!(stored_tuples(stored), now, "{name}");
        }
        if name.ends_with("_inject") {
            let EditOutcome::Applied(report) = &fixture.edit_outcomes[0] else {
                panic!("{name}: injection applies in the raw sequence");
            };
            assert_eq!(
                report
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.check.kind == CheckKind::ValueConstraint)
                    .count(),
                2,
                "{name}: repeated violations retain multiplicity"
            );
            for kind in [CheckKind::RequiredEdge, CheckKind::Acyclic] {
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.check.kind == kind),
                    "{name}: injected sequence exercises {kind:?}"
                );
            }
            assert!(fixture.incremental.is_none());
            continue;
        }

        let witness = replay_incremental_probe(name, fixture);
        assert!(
            witness
                .runs
                .iter()
                .any(|run| matches!(run.verdict, RunVerdict::Evaluated { .. })),
            "{name}"
        );
        assert!(
            witness
                .runs
                .iter()
                .any(|run| matches!(run.verdict, RunVerdict::Skipped { .. })),
            "{name}"
        );
        assert!(
            witness.runs.iter().all(|run| !run.reads.is_empty()),
            "{name}: full dependency reads"
        );
    }
}

/// Each case of the corpus by the shapes that write it: its graph, tree and
/// table cases, `None` where the shape cannot express the case. Tables
/// hold no edge positions, cycles or fans, and construction refuses a pair
/// between rows of the wrong tables, so a table holds a coherent pair but
/// no incoherent one; tree arcs hold no positions, no
/// self-loops, no parallel arcs and no cycles; a tree fan has one parent
/// and one child per label, and a tree holds no fan nodes for an edge to
/// meet; `Acyclic` and `PositionDistinct` are graph checks, and the tree
/// checks (`TreeReachable`, and `MissingChild` for a fan child that is not
/// a node) have no graph or table counterpart.
const SHAPES: &[[Option<&str>; 3]] = &[
    [
        Some("anchors/probe"),
        Some("anchors/tree_unknown_root_anchor"),
        Some("anchors/unknown_table"),
    ],
    [
        Some("anchors/probe"),
        Some("anchors/tree_undeclared_label"),
        Some("anchors/unknown_fk_label"),
    ],
    [
        Some("endpoints/coherent"),
        Some("endpoints/tree_coherent"),
        Some("endpoints/table_coherent"),
    ],
    [
        Some("endpoints/src_side"),
        Some("endpoints/tree_src_side"),
        None,
    ],
    [
        Some("endpoints/tgt_side"),
        Some("endpoints/tree_tgt_side"),
        None,
    ],
    [
        Some("endpoints/both_sides"),
        Some("endpoints/tree_both_sides"),
        None,
    ],
    [Some("endpoints/self_loop_on_non_self_edge"), None, None],
    [Some("endpoints/incremental_repair_kept_edge"), None, None],
    [
        Some("endpoints/edit_anchor_change_breaks_kept_edge"),
        Some("endpoints/tree_edit_anchor_change_breaks_kept_arc"),
        None,
    ],
    [
        Some("endpoints/edit_relabel_breaks_kept_edge"),
        Some("endpoints/tree_edit_relabel_breaks_kept_arc"),
        None,
    ],
    [
        Some("values/graph_values_and_attributes"),
        Some("values/tree_values_and_attributes"),
        Some("values/table_values_and_attributes"),
    ],
    [
        Some("cardinality/required_edge_present"),
        Some("cardinality/tree_required_edge_present"),
        Some("cardinality/table_required_foreign_key_present"),
    ],
    [
        Some("cardinality/required_edge_absent"),
        Some("cardinality/tree_required_edge_absent"),
        Some("cardinality/table_required_foreign_key_absent"),
    ],
    [
        Some("cardinality/degree_at_upper_bound"),
        Some("cardinality/tree_degree_at_upper_bound"),
        Some("cardinality/table_degree_at_upper_bound"),
    ],
    [
        Some("cardinality/degree_above_upper_bound"),
        Some("cardinality/tree_degree_above_upper_bound"),
        Some("cardinality/table_degree_above_upper_bound"),
    ],
    [
        Some("cardinality/degree_at_lower_bound"),
        Some("cardinality/tree_degree_at_lower_bound"),
        Some("cardinality/table_degree_at_lower_bound"),
    ],
    [
        Some("cardinality/degree_below_lower_bound"),
        Some("cardinality/tree_degree_below_lower_bound"),
        Some("cardinality/table_degree_below_lower_bound"),
    ],
    [Some("cardinality/parallel_edges_counted"), None, None],
    [Some("ordering/duplicate_positions"), None, None],
    [Some("ordering/absent_positions_allowed"), None, None],
    [Some("ordering/distinct_positions"), None, None],
    [Some("ordering/incremental_repair_duplicate"), None, None],
    [
        Some("ordering/edit_reorder_creates_and_repairs_duplicate"),
        None,
        None,
    ],
    [Some("cycles/legal_without_declaration"), None, None],
    [Some("cycles/two_cycle"), None, None],
    [Some("cycles/long_cycle"), None, None],
    [Some("cycles/self_loop"), None, None],
    [Some("cycles/acyclic_chain"), None, None],
    [Some("cycles/edit_closes_cycle"), None, None],
    [Some("cycles/edit_breaks_cycle"), None, None],
    [
        None,
        Some("cycles/tree_edit_moves_subtree_under_its_own_descendant"),
        None,
    ],
    [
        None,
        Some("cycles/tree_subtree_under_its_own_descendant"),
        None,
    ],
    [
        None,
        Some("cycles/tree_arcs_close_a_cycle_off_the_root"),
        None,
    ],
    [
        Some("fans/well_formed"),
        Some("fans/tree_well_formed"),
        None,
    ],
    [
        Some("fans/missing_parent"),
        Some("fans/tree_missing_parent"),
        None,
    ],
    [
        Some("fans/unknown_label"),
        Some("fans/tree_unknown_label"),
        None,
    ],
    [
        Some("fans/unknown_hyper_edge"),
        Some("fans/tree_unknown_hyper_edge"),
        None,
    ],
    [None, Some("fans/tree_missing_child"), None],
    [Some("fans/multiple_parents"), None, None],
    [Some("fans/duplicate_label"), None, None],
    [Some("fans/reserved_kind_outside_fan"), None, None],
    [Some("fans/foreign_incident_edge"), None, None],
    [
        Some("keys/graph_sakila_composite_key"),
        Some("keys/tree_sakila_composite_key"),
        Some("keys/sakila_composite_key"),
    ],
    [
        Some("keys/graph_sakila_composite_key_duplicate"),
        Some("keys/tree_sakila_composite_key_duplicate"),
        Some("keys/sakila_composite_key_duplicate"),
    ],
    [
        Some("keys/graph_self_referencing_foreign_key"),
        Some("keys/tree_self_referencing_foreign_key"),
        Some("keys/self_referencing_foreign_key"),
    ],
    [
        Some("keys/graph_dangling_foreign_key_strict"),
        Some("keys/tree_dangling_foreign_key_strict"),
        Some("keys/dangling_foreign_key_strict"),
    ],
    [
        Some("keys/graph_dangling_foreign_key_lenient"),
        Some("keys/tree_dangling_foreign_key_lenient"),
        Some("keys/dangling_foreign_key_lenient"),
    ],
    [
        Some("keys/graph_within_nested_maps_accept_an_equal_key"),
        Some("keys/within_nested_maps_accept_an_equal_key"),
        None,
    ],
    [
        Some("keys/graph_within_one_map_rejects_a_repeated_key"),
        Some("keys/within_one_map_rejects_a_repeated_key"),
        None,
    ],
    [
        Some("keys/graph_within_no_enclosing_map_is_outside_the_key"),
        Some("keys/within_no_enclosing_map_is_outside_the_key"),
        None,
    ],
    [
        Some("incomplete/acyclic_budget"),
        Some("incomplete/tree_budget"),
        Some("incomplete/table_budget"),
    ],
    [
        Some("incomplete/unregistered_sort"),
        Some("incomplete/tree_unregistered_sort"),
        Some("incomplete/table_unregistered_sort"),
    ],
    [
        Some("incomplete/pattern_lookahead"),
        Some("incomplete/tree_pattern_lookahead"),
        Some("incomplete/table_pattern_lookahead"),
    ],
    [
        Some("incomplete/external_missing"),
        Some("incomplete/tree_external_missing"),
        Some("incomplete/table_external_missing"),
    ],
    [
        Some("incomplete/external_rejected"),
        Some("incomplete/tree_external_rejected"),
        Some("incomplete/table_external_rejected"),
    ],
    [
        Some("incomplete/mixed_failure_and_incomplete"),
        Some("incomplete/tree_mixed_failure_and_incomplete"),
        Some("incomplete/table_mixed_failure_and_incomplete"),
    ],
    [
        Some("incomplete/source_map"),
        Some("incomplete/tree_source_map"),
        Some("incomplete/table_source_map"),
    ],
];

/// Each case is written in every shape that can express it, each shape's
/// fixture holds an instance of that shape, and every fixture of the
/// corpus is one of the cases.
#[test]
fn every_case_is_written_in_every_shape_that_can_express_it() {
    let fixtures = fixtures();
    let mut listed = std::collections::BTreeSet::new();
    for row in SHAPES {
        assert!(row.iter().any(Option::is_some), "an empty row");
        for (column, name) in row.iter().enumerate() {
            let Some(name) = name else { continue };
            listed.insert(*name);
            let fixture = fixtures
                .get(*name)
                .unwrap_or_else(|| panic!("{name} is not in the corpus"));
            let shape = match fixture.instance {
                Instance::Graph(_) => 0,
                Instance::Tree(_) => 1,
                Instance::Table(_) => 2,
            };
            assert_eq!(shape, column, "{name} is written in the wrong shape");
        }
    }
    let unlisted: Vec<&String> = fixtures
        .keys()
        .filter(|name| {
            !listed.contains(name.as_str())
                && !name.starts_with("boundary/")
                && !name.starts_with("train_benchmark/")
        })
        .collect();
    assert!(unlisted.is_empty(), "fixtures in no case: {unlisted:?}");
}
