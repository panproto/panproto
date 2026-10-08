#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples for shared parsers and the schema-v2 manifest loader.

use panproto_reference::{json, manifest, pattern};

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
        200
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
