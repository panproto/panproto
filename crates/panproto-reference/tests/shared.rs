#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples for shared parsers and the schema-v2 manifest loader.

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
    assert!(graph.ordered_out_edges(1).is_empty());

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
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let direct = checkout.join("notes/graph-parity/records/correctness_manifest.json");
    if direct.is_file() {
        return checkout.to_path_buf();
    }
    let git_file = std::fs::read_to_string(checkout.join(".git")).unwrap();
    let git_dir = std::path::Path::new(git_file.trim_start_matches("gitdir: ").trim());
    git_dir
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .unwrap()
        .to_path_buf()
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

#[test]
fn schema_v2_manifest_loading_checks_partition_invariants() {
    let text = r#"{
      "schema_version": 2,
      "pages": {
        "values": {
          "inventory_status": "complete",
          "reference_files": ["crates/panproto-reference/src/pages/values.rs"],
          "laws": [{
            "id": "values.example",
            "anchor": "L-values-1",
            "contract": [{"section": "P.3", "clause": "VF1"}],
            "references": ["panproto_reference::value::representation"],
            "due_gate": "phase3"
          }],
          "due_gate": "phase3"
        },
        "parsing": {
          "inventory_status": "pending",
          "reference_files": ["crates/panproto-reference/src/pages/parsing.rs"],
          "laws": [],
          "contract_scope": [{"section": "O11", "clause": "D20(d)"}],
          "due_gate": "final"
        }
      }
    }"#;
    let loaded = manifest::load(text).unwrap();
    assert_eq!(loaded.pages.len(), 2);
    assert_eq!(
        loaded.pages["values"].laws[0].due_gate,
        manifest::Gate::Phase3
    );
    assert_eq!(
        loaded.pages["parsing"].inventory_status,
        manifest::InventoryStatus::Pending
    );

    let duplicate = text.replacen(
        "\"schema_version\": 2",
        "\"schema_version\": 2, \"schema_version\": 2",
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
fn actual_schema_v2_manifest_loads_and_every_owned_file_exists() {
    let root = repository_root();
    let path = root.join("notes/graph-parity/records/correctness_manifest.json");
    let text = std::fs::read_to_string(path).unwrap();
    let loaded = manifest::load(&text).unwrap();
    assert_eq!(loaded.pages.len(), 33);
    assert_eq!(
        loaded
            .pages
            .values()
            .map(|page| page.laws.len())
            .sum::<usize>(),
        206
    );
    for page in loaded.pages.values() {
        for relative in &page.reference_files {
            let checkout_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap()
                .join(relative);
            assert!(checkout_file.is_file(), "missing {relative}");
        }
    }
}
