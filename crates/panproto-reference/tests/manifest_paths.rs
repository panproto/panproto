#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Deterministic audit of every semantic path in the schema-v3 manifest.
//!
//! The crate-level rustdoc table is the compile-time path audit: strict
//! rustdoc rejects any unresolved link. This test proves that the links
//! cover exactly the manifest's reference set at the recorded digest.

fn repository_root() -> std::path::PathBuf {
    let checkout = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    if checkout
        .join("notes/graph-parity/records/correctness_manifest.json")
        .is_file()
    {
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

fn documented_table_functions() -> std::collections::BTreeSet<String> {
    include_str!("../src/lib.rs")
        .lines()
        .filter(|line| line.starts_with("//! |"))
        .flat_map(|line| line.split("[`").skip(1))
        .filter_map(|tail| tail.split_once("`]").map(|(path, _)| path))
        .filter(|path| !path.contains(char::is_whitespace))
        .map(|path| format!("panproto_reference::{path}"))
        .collect()
}

#[test]
fn the_a13_table_is_exactly_the_manifest_reference_population() {
    let root = repository_root();
    let manifest_text =
        std::fs::read_to_string(root.join("notes/graph-parity/records/correctness_manifest.json"))
            .unwrap();
    let manifest = panproto_reference::manifest::load(&manifest_text).unwrap();
    let status_counts = manifest
        .pages
        .values()
        .fold([0_usize; 3], |mut counts, page| {
            let index = match page.inventory_status {
                panproto_reference::manifest::InventoryStatus::Complete => 0,
                panproto_reference::manifest::InventoryStatus::Partial => 1,
                panproto_reference::manifest::InventoryStatus::Pending => 2,
            };
            counts[index] += 1;
            counts
        });
    assert_eq!(status_counts, [8, 4, 21]);

    let laws: Vec<_> = manifest
        .pages
        .values()
        .flat_map(|page| &page.laws)
        .collect();
    assert_eq!(laws.len(), 206);
    assert_eq!(
        laws.iter()
            .filter(|law| law.due_gate == panproto_reference::manifest::Gate::Phase3)
            .count(),
        192
    );
    assert_eq!(
        laws.iter()
            .filter(|law| law.due_gate == panproto_reference::manifest::Gate::Final)
            .count(),
        14
    );
    assert_eq!(
        laws.iter().map(|law| law.references.len()).sum::<usize>(),
        322
    );

    for page in manifest.pages.values().filter(|page| {
        page.inventory_status == panproto_reference::manifest::InventoryStatus::Partial
    }) {
        let mut expected_scope = Vec::new();
        for locator in page.laws.iter().flat_map(|law| &law.contract) {
            if !expected_scope.contains(locator) {
                expected_scope.push(locator.clone());
            }
        }
        assert_eq!(page.contract_scope, expected_scope);
    }

    let partial_laws: Vec<_> = manifest
        .pages
        .values()
        .filter(|page| {
            page.inventory_status == panproto_reference::manifest::InventoryStatus::Partial
        })
        .flat_map(|page| &page.laws)
        .collect();
    let partial_paths: std::collections::BTreeSet<_> = partial_laws
        .iter()
        .flat_map(|law| &law.references)
        .cloned()
        .collect();
    assert_eq!(partial_laws.len(), 58);
    assert_eq!(partial_paths.len(), 45);

    let expected: std::collections::BTreeSet<_> = manifest
        .pages
        .values()
        .flat_map(|page| &page.laws)
        .flat_map(|law| &law.references)
        .cloned()
        .collect();
    let documented = documented_table_functions();
    assert_eq!(expected.len(), 183);
    assert_eq!(documented.len(), 183);
    assert_eq!(documented, expected);
}

#[test]
fn validation_law_to_public_path_incidences_are_exact() {
    let manifest_text = std::fs::read_to_string(
        repository_root().join("notes/graph-parity/records/correctness_manifest.json"),
    )
    .unwrap();
    let manifest = panproto_reference::manifest::load(&manifest_text).unwrap();
    let page = &manifest.pages["validation"];
    let core = vec![
        "panproto_reference::validation::CheckStatus::all".to_owned(),
        "panproto_reference::validation::status".to_owned(),
        "panproto_reference::validation::validate".to_owned(),
    ];
    let mut expected = (1..=11)
        .map(|number| (format!("validation.v{number}"), core.clone()))
        .collect::<Vec<_>>();
    expected.extend([
        (
            "validation.admission-policies".to_owned(),
            vec!["panproto_reference::validation::admits".to_owned()],
        ),
        (
            "validation.key-equality-checks".to_owned(),
            vec!["panproto_reference::validation::key_equality".to_owned()],
        ),
        (
            "validation.variable-binding-checks".to_owned(),
            vec!["panproto_reference::validation::variable_binding".to_owned()],
        ),
        (
            "validation.guard-target-set-sort-key-checks".to_owned(),
            vec!["panproto_reference::validation::key_forms".to_owned()],
        ),
        (
            "validation.index-null-match-checks".to_owned(),
            vec!["panproto_reference::validation::index_and_null_match".to_owned()],
        ),
        (
            "validation.lexical-space-advisory".to_owned(),
            vec!["panproto_reference::validation::lexical_space".to_owned()],
        ),
        (
            "validation.annotated-format-no-assertion-equivalence".to_owned(),
            vec!["panproto_reference::validation::annotated_format_parse".to_owned()],
        ),
        (
            "validation.annotated-format-advisory-exactness".to_owned(),
            vec!["panproto_reference::validation::annotated_format_advisory".to_owned()],
        ),
        (
            "validation.format-assertion-acceptance".to_owned(),
            vec!["panproto_reference::validation::format_assertion_acceptance".to_owned()],
        ),
    ]);
    let actual = page
        .laws
        .iter()
        .map(|law| (law.id.clone(), law.references.clone()))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert_eq!(page.laws.len(), 20);
    assert_eq!(
        page.laws
            .iter()
            .map(|law| law.references.len())
            .sum::<usize>(),
        42,
    );
}
