#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples for shared parsers and the correctness manifest loader.

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

const fn manifest_fixture() -> &'static str {
    r#"{
      "schema_version": 5,
      "pages": {
        "values": {
          "reference_files": ["crates/panproto-reference/src/pages/values.rs"],
          "laws": [{
            "id": "values.example",
            "anchor": "L-values-1",
            "references": ["panproto_reference::value::representation"]
          }]
        },
        "edits": {
          "reference_files": ["crates/panproto-reference/src/pages/edits.rs"],
          "laws": [{"id":"edits.example","anchor":"L-edits-1","references":["panproto_reference::edit::apply"]}]
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
fn a_manifest_loads_its_pages_laws_and_references() {
    let text = manifest_fixture();
    let loaded = manifest::load(text).unwrap();
    assert_eq!(loaded.pages.len(), 2);
    assert_eq!(loaded.pages["values"].laws[0].anchor, "L-values-1");
    assert_eq!(
        loaded.pages["edits"].laws[0].references,
        ["panproto_reference::edit::apply"]
    );
    assert_eq!(
        loaded.pages["edits"].reference_files,
        ["crates/panproto-reference/src/pages/edits.rs"]
    );

    let duplicate = text.replacen(
        "\"schema_version\": 5",
        "\"schema_version\": 5, \"schema_version\": 5",
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
fn a_manifest_refuses_every_other_schema_version() {
    let text = manifest_fixture();
    assert!(manifest::load(text).is_ok());
    for version in [
        "4", "6", "50", "5.0", "5e0", "-5", "\"5\"", "true", "null", "[5]",
    ] {
        let other = text.replacen(
            "\"schema_version\": 5",
            &format!("\"schema_version\": {version}"),
            1,
        );
        assert_manifest_shape(&other, "manifest.schema_version is not 5");
    }
}

#[test]
fn a_manifest_rejects_duplicate_law_ids_anchors_and_reference_files() {
    let text = manifest_fixture();
    let duplicate = text.replacen(r#""id":"edits.example""#, r#""id":"values.example""#, 1);
    assert_eq!(
        manifest::load(&duplicate),
        Err(manifest::ManifestError::DuplicateValue(
            "law id values.example".to_owned()
        ))
    );
    let duplicate = text.replacen(r#""anchor":"L-edits-1""#, r#""anchor":"L-values-1""#, 1);
    assert_eq!(
        manifest::load(&duplicate),
        Err(manifest::ManifestError::DuplicateValue(
            "anchor L-values-1".to_owned()
        ))
    );
    let duplicate = text.replacen("src/pages/edits.rs", "src/pages/values.rs", 1);
    assert_eq!(
        manifest::load(&duplicate),
        Err(manifest::ManifestError::DuplicateValue(
            "reference file crates/panproto-reference/src/pages/values.rs".to_owned()
        ))
    );
}

#[test]
fn a_manifest_page_without_laws_is_refused() {
    let empty = manifest_fixture().replacen(
        r#""laws": [{"id":"edits.example","anchor":"L-edits-1","references":["panproto_reference::edit::apply"]}]"#,
        r#""laws": []"#,
        1,
    );
    assert_manifest_shape(&empty, "manifest.pages.edits has no laws");
}

#[test]
fn the_checked_in_manifest_loads_and_every_owned_file_exists() {
    let root = repository_root();
    let text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
    let loaded = manifest::load(&text).unwrap();
    assert_eq!(loaded.pages.len(), 16);
    assert_eq!(
        loaded
            .pages
            .values()
            .map(|page| page.laws.len())
            .sum::<usize>(),
        230
    );
    for page in loaded.pages.values() {
        for relative in &page.reference_files {
            let checkout_file = root.join(relative);
            assert!(checkout_file.is_file(), "missing {relative}");
        }
    }
}
