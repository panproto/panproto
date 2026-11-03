#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples for shared parsers and the schema-v4 manifest loader.

use panproto_reference::{json, manifest, model, pattern};

#[test]
fn shared_model_observables_follow_their_definitions() {
    let named = model::SEdge::new("s", "t", "prop", Some("column"));
    let unnamed = model::SEdge::new("s", "target", "prop", None);
    assert_eq!(named.column(), "column");
    assert_eq!(unnamed.column(), "target");

    let mut graph = model::Graph::default();
    graph.nodes.insert(0, model::Node::new(0, "s"));
    graph.nodes.insert(1, model::Node::new(1, "t"));
    graph.edges.insert(
        7,
        model::GEdge {
            src: 0,
            tgt: 1,
            label: named.clone(),
            position: Some(1),
        },
    );
    graph.edges.insert(
        3,
        model::GEdge {
            src: 0,
            tgt: 1,
            label: named.clone(),
            position: None,
        },
    );
    graph.edges.insert(
        5,
        model::GEdge {
            src: 0,
            tgt: 1,
            label: named.clone(),
            position: Some(0),
        },
    );
    assert!(graph.well_formed());
    assert_eq!(graph.ordered_out_edges(0), vec![3, 5, 7]);
    assert_eq!(graph.ordered_out_edges(1), []);

    let mut bad_id = graph.clone();
    bad_id.nodes.get_mut(&0).unwrap().id = 9;
    assert!(!bad_id.well_formed());
    let mut bad_src = graph.clone();
    bad_src.edges.get_mut(&3).unwrap().src = 9;
    assert!(!bad_src.well_formed());
    let mut bad_tgt = graph;
    bad_tgt.edges.get_mut(&3).unwrap().tgt = 9;
    assert!(!bad_tgt.well_formed());

    let mut table = model::Table::default();
    table
        .rows
        .insert(0, ("s".to_owned(), std::collections::BTreeMap::default()));
    table
        .rows
        .insert(1, ("t".to_owned(), std::collections::BTreeMap::default()));
    table.pairs.insert((named.clone(), 0, 1));
    assert!(table.well_formed());
    table.pairs.insert((named.clone(), 9, 1));
    assert!(!table.well_formed());
    table.pairs.remove(&(named.clone(), 9, 1));
    table.pairs.insert((named, 0, 9));
    assert!(!table.well_formed());
}

fn repository_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

fn correctness_manifest_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/correctness_manifest.json")
}

#[test]
fn json_keeps_exact_numbers_and_duplicate_members() {
    let value = json::parse(r#"{"x":1e+02,"x":"\uD83D\uDE00"}"#).unwrap();
    assert_eq!(
        value,
        json::Value::Object(vec![
            ("x".to_owned(), json::Value::Number("1e+02".to_owned())),
            ("x".to_owned(), json::Value::String("😀".to_owned())),
        ])
    );
}

#[test]
fn the_pattern_reference_searches_by_exhaustive_backtracking() {
    let pattern = pattern::compile(r"^(?:ab|a[[:digit:]]){1,2}$").unwrap();
    assert!(pattern.is_match("aba7"));
    assert!(!pattern.is_match("aba"));
    assert_eq!(
        pattern::compile(r"(?=x)"),
        Err(pattern::PatternError::Invalid(
            "look-ahead is not supported by the syntax".to_owned()
        ))
    );
}

const fn schema_v4_fixture() -> &'static str {
    r#"{
      "schema_version": 4,
      "pages": {
        "values": {
          "inventory_status": "complete",
          "reference_files": ["crates/panproto-reference/src/pages/values.rs"],
          "laws": [{
            "id": "values.example",
            "anchor": "L-values-1",
            "references": ["panproto_reference::value::representation"],
            "due_gate": "candidate"
          }],
          "due_gate": "candidate"
        },
        "edits": {
          "inventory_status": "partial",
          "reference_files": ["crates/panproto-reference/src/pages/edits.rs"],
          "laws": [{"id":"edits.example","anchor":"L-edits-1","references":["panproto_reference::edit::apply"],"due_gate":"candidate"}],
          "open_scope": ["L-edits-1"],
          "due_gate": "candidate"
        },
        "parsing": {
          "inventory_status": "pending",
          "reference_files": ["crates/panproto-reference/src/pages/parsing.rs"],
          "laws": [],
          "open_scope": ["first independent inventory of laws, operations, domains and refusals"],
          "due_gate": "release"
        }
      }
    }"#
}

fn assert_manifest_shape(text: &str, message: &str) {
    assert_eq!(
        manifest::load(text),
        Err(manifest::ManifestError::Shape(message.to_owned()))
    );
}

#[test]
fn schema_v4_manifest_loading_retains_all_inventory_statuses() {
    let text = schema_v4_fixture();
    let loaded = manifest::load(text).unwrap();
    assert_eq!(loaded.pages.len(), 3);
    assert_eq!(
        loaded.pages["values"].laws[0].due_gate,
        manifest::Gate::Candidate
    );
    assert_eq!(
        loaded.pages["edits"].inventory_status,
        manifest::InventoryStatus::Partial
    );
    assert_eq!(loaded.pages["edits"].laws.len(), 1);
    assert_eq!(loaded.pages["edits"].open_scope.len(), 1);
    assert_eq!(
        loaded.pages["parsing"].inventory_status,
        manifest::InventoryStatus::Pending
    );

    let duplicate = text.replacen(
        "\"schema_version\": 4",
        "\"schema_version\": 4, \"schema_version\": 4",
        1,
    );
    assert_eq!(
        manifest::load(&duplicate),
        Err(manifest::ManifestError::DuplicateMember(
            "manifest.schema_version".to_owned()
        ))
    );
}

#[test]
fn schema_v4_manifest_refuses_every_other_schema_version() {
    let text = schema_v4_fixture();
    assert!(manifest::load(text).is_ok());
    for version in [
        "3", "5", "40", "4.0", "4e0", "-4", "\"4\"", "true", "null", "[4]",
    ] {
        let other = text.replacen(
            "\"schema_version\": 4",
            &format!("\"schema_version\": {version}"),
            1,
        );
        assert_manifest_shape(&other, "manifest.schema_version is not 4");
    }
}

#[test]
fn schema_v4_manifest_rejects_duplicate_semantic_law_ids() {
    let duplicate =
        schema_v4_fixture().replacen(r#""id":"edits.example""#, r#""id":"values.example""#, 1);
    assert_eq!(
        manifest::load(&duplicate),
        Err(manifest::ManifestError::DuplicateValue(
            "law id values.example".to_owned()
        ))
    );
}

#[test]
fn schema_v4_scope_entries_cite_only_known_law_anchors() {
    let text = schema_v4_fixture();
    let unknown = text.replacen(
        r#""open_scope": ["L-edits-1"]"#,
        r#""open_scope": ["L-edits-9"]"#,
        1,
    );
    assert_eq!(
        manifest::load(&unknown),
        Err(manifest::ManifestError::UnknownAnchor(
            "manifest.pages.edits cites L-edits-9".to_owned()
        ))
    );

    let release = text.replacen(
        "          \"due_gate\": \"release\"\n",
        "          \"due_gate\": \"release\",\n          \"release_scope\": [\"L-values-1\", \"L-values-2\"]\n",
        1,
    );
    assert_eq!(
        manifest::load(&release),
        Err(manifest::ManifestError::UnknownAnchor(
            "manifest.pages.parsing cites L-values-2".to_owned()
        ))
    );
    let described = release.replacen("\"L-values-2\"", "\"sublaws of L-values-1\"", 1);
    assert_eq!(
        manifest::load(&described).unwrap().pages["parsing"].release_scope,
        ["L-values-1", "sublaws of L-values-1"]
    );
}

#[test]
fn schema_v4_partial_inventory_requires_laws_and_scope() {
    let text = schema_v4_fixture();
    let partial_without_laws = text.replacen(
        r#""laws": [{"id":"edits.example","anchor":"L-edits-1","references":["panproto_reference::edit::apply"],"due_gate":"candidate"}]"#,
        r#""laws": []"#,
        1,
    );
    assert_manifest_shape(
        &partial_without_laws,
        "manifest.pages.edits partial inventory must have laws and an open_scope",
    );

    let partial_without_scope =
        text.replacen("          \"open_scope\": [\"L-edits-1\"],\n", "", 1);
    assert_manifest_shape(
        &partial_without_scope,
        "manifest.pages.edits partial inventory must have laws and an open_scope",
    );

    let partial_with_empty_scope =
        text.replacen("\"open_scope\": [\"L-edits-1\"]", "\"open_scope\": []", 1);
    assert_manifest_shape(
        &partial_with_empty_scope,
        "manifest.pages.edits partial inventory must have laws and an open_scope",
    );

    let pending_with_empty_scope = text.replacen(
        "\"open_scope\": [\"first independent inventory of laws, operations, domains and refusals\"]",
        "\"open_scope\": []",
        1,
    );
    assert_manifest_shape(
        &pending_with_empty_scope,
        "manifest.pages.parsing pending inventory must have no laws and an open_scope",
    );
}

#[test]
fn schema_v4_complete_inventory_requires_laws_and_absent_scope_key() {
    let text = schema_v4_fixture();
    let complete_with_scope = text.replacen(
        "          \"inventory_status\": \"complete\",\n",
        "          \"inventory_status\": \"complete\",\n          \"open_scope\": [\"L-values-1\"],\n",
        1,
    );
    assert_manifest_shape(
        &complete_with_scope,
        "manifest.pages.values complete inventory must have laws and no open_scope",
    );

    let complete_with_empty_scope = text.replacen(
        "          \"inventory_status\": \"complete\",\n",
        "          \"inventory_status\": \"complete\",\n          \"open_scope\": [],\n",
        1,
    );
    assert_manifest_shape(
        &complete_with_empty_scope,
        "manifest.pages.values complete inventory must have laws and no open_scope",
    );

    let complete_without_laws = text.replacen(
        r#"          "laws": [{
            "id": "values.example",
            "anchor": "L-values-1",
            "references": ["panproto_reference::value::representation"],
            "due_gate": "candidate"
          }],"#,
        r#"          "laws": [],"#,
        1,
    );
    assert_manifest_shape(
        &complete_without_laws,
        "manifest.pages.values complete inventory must have laws and no open_scope",
    );
}

#[test]
fn schema_v4_pending_inventory_requires_no_laws_and_nonempty_scope() {
    let text = schema_v4_fixture();
    let pending_with_laws = text.replacen(
        "          \"laws\": [],\n",
        "          \"laws\": [{\"id\":\"parsing.example\",\"anchor\":\"L-parsing-1\",\"references\":[\"panproto_reference::value::representation\"],\"due_gate\":\"release\"}],\n",
        1,
    );
    assert_manifest_shape(
        &pending_with_laws,
        "manifest.pages.parsing pending inventory must have no laws and an open_scope",
    );

    let pending_without_scope = text.replacen(
        "          \"open_scope\": [\"first independent inventory of laws, operations, domains and refusals\"],\n",
        "",
        1,
    );
    assert_manifest_shape(
        &pending_without_scope,
        "manifest.pages.parsing pending inventory must have no laws and an open_scope",
    );

    let pending_with_empty_scope = text.replacen(
        "          \"open_scope\": [\"first independent inventory of laws, operations, domains and refusals\"],\n",
        "          \"open_scope\": [],\n",
        1,
    );
    assert_manifest_shape(
        &pending_with_empty_scope,
        "manifest.pages.parsing pending inventory must have no laws and an open_scope",
    );
}

#[test]
fn fixture_schema_v4_manifest_loads_and_every_owned_file_exists() {
    let root = repository_root();
    let text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
    let loaded = manifest::load(&text).unwrap();
    assert_eq!(loaded.pages.len(), 33);
    assert_eq!(
        loaded
            .pages
            .values()
            .filter(|page| page.inventory_status == manifest::InventoryStatus::Complete)
            .count(),
        11
    );
    assert_eq!(
        loaded
            .pages
            .values()
            .filter(|page| page.inventory_status == manifest::InventoryStatus::Partial)
            .count(),
        1
    );
    assert_eq!(
        loaded
            .pages
            .values()
            .filter(|page| page.inventory_status == manifest::InventoryStatus::Pending)
            .count(),
        21
    );
    assert_eq!(
        loaded
            .pages
            .values()
            .map(|page| page.laws.len())
            .sum::<usize>(),
        205
    );
    for page in loaded.pages.values() {
        for relative in &page.reference_files {
            let checkout_file = root.join(relative);
            assert!(checkout_file.is_file(), "missing {relative}");
        }
    }
}
