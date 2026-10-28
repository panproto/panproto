#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Deterministic audit of every semantic path in the schema-v3 manifest.
//!
//! The crate-level rustdoc table is the compile-time path audit: strict
//! rustdoc rejects any unresolved link. This test proves that the links
//! cover exactly the manifest's reference set at the recorded digest.

fn correctness_manifest_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/correctness_manifest.json")
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
    let manifest_text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
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
    assert_eq!(status_counts, [11, 1, 21]);

    let laws: Vec<_> = manifest
        .pages
        .values()
        .flat_map(|page| &page.laws)
        .collect();
    assert_eq!(laws.len(), 205);
    assert_eq!(
        laws.iter()
            .filter(|law| law.due_gate == panproto_reference::manifest::Gate::Phase3)
            .count(),
        191
    );
    assert_eq!(
        laws.iter()
            .filter(|law| law.due_gate == panproto_reference::manifest::Gate::Final)
            .count(),
        14
    );
    assert_eq!(
        laws.iter().map(|law| law.references.len()).sum::<usize>(),
        341
    );

    let value_io_l21 = manifest.pages["value-io"]
        .laws
        .iter()
        .find(|law| law.id == "value-io.vf8-presentation-preservation-cst")
        .unwrap();
    assert_eq!(
        value_io_l21.references,
        vec![
            "panproto_reference::value_io::preserve".to_owned(),
            "panproto_reference::value_io::preserve_cst".to_owned(),
        ]
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
    assert_eq!(partial_laws.len(), 20);
    assert_eq!(
        partial_laws
            .iter()
            .map(|law| law.references.len())
            .sum::<usize>(),
        67,
    );
    assert_eq!(partial_paths.len(), 26);

    let expected: std::collections::BTreeSet<_> = manifest
        .pages
        .values()
        .flat_map(|page| &page.laws)
        .flat_map(|law| &law.references)
        .cloned()
        .collect();
    let documented = documented_table_functions();
    assert_eq!(expected.len(), 218);
    assert_eq!(documented.len(), 218);
    assert_eq!(documented, expected);
}

#[test]
fn lenses_law_to_public_path_incidences_are_exact() {
    let manifest_text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
    let manifest = panproto_reference::manifest::load(&manifest_text).unwrap();
    let page = &manifest.pages["lenses"];
    let expected = [
        (
            "lenses.l5",
            vec![
                "panproto_reference::lens::GraphLens::check_put_domain".to_owned(),
                "panproto_reference::lens::GraphLens::put".to_owned(),
                "panproto_reference::lens::get".to_owned(),
                "panproto_reference::lens::put".to_owned(),
                "panproto_reference::lens::tree_get".to_owned(),
                "panproto_reference::lens::tree_put".to_owned(),
            ],
        ),
        (
            "lenses.l6",
            vec!["panproto_reference::lens::compose_complement".to_owned()],
        ),
        (
            "lenses.t3",
            vec!["panproto_reference::table::get".to_owned()],
        ),
        (
            "lenses.path-left-identity",
            vec![
                "panproto_reference::path::LensPath::then".to_owned(),
                "panproto_reference::path::get_path".to_owned(),
                "panproto_reference::path::put_path".to_owned(),
            ],
        ),
        (
            "lenses.path-right-identity",
            vec![
                "panproto_reference::path::LensPath::then".to_owned(),
                "panproto_reference::path::get_path".to_owned(),
                "panproto_reference::path::put_path".to_owned(),
            ],
        ),
        (
            "lenses.path-associativity",
            vec![
                "panproto_reference::path::LensPath::then".to_owned(),
                "panproto_reference::path::get_path".to_owned(),
                "panproto_reference::path::put_path".to_owned(),
            ],
        ),
        (
            "lenses.fusion-soundness",
            vec![
                "panproto_reference::path::get_path".to_owned(),
                "panproto_reference::path::put_path".to_owned(),
                "panproto_reference::lens::TreeShape::get".to_owned(),
                "panproto_reference::lens::TreeShape::put".to_owned(),
                "panproto_reference::lens::TableShape::get".to_owned(),
                "panproto_reference::lens::TableShape::put".to_owned(),
            ],
        ),
        (
            "lenses.fb-graph-square",
            vec![
                "panproto_reference::lens::graph_square".to_owned(),
                "panproto_reference::lens::graph_put_square".to_owned(),
            ],
        ),
        (
            "lenses.fb-table-square",
            vec![
                "panproto_reference::lens::table_square".to_owned(),
                "panproto_reference::lens::table_put_square".to_owned(),
                "panproto_reference::lens::relational_table_get".to_owned(),
                "panproto_reference::lens::relational_table_put".to_owned(),
            ],
        ),
    ];
    for (id, references) in expected {
        let law = page.laws.iter().find(|law| law.id == id).unwrap();
        assert_eq!(law.references, references, "unexpected references for {id}");
    }
    assert_eq!(page.laws.len(), 20);
    assert_eq!(
        page.laws
            .iter()
            .map(|law| law.references.len())
            .sum::<usize>(),
        67,
    );
    let unique_paths = page
        .laws
        .iter()
        .flat_map(|law| &law.references)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique_paths.len(), 26);
}

#[test]
fn validation_law_to_public_path_incidences_are_exact() {
    let manifest_text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
    let manifest = panproto_reference::manifest::load(&manifest_text).unwrap();
    let page = &manifest.pages["validation"];
    let expected = vec![
        (
            "validation.v1".to_owned(),
            vec![
                "panproto_reference::validation::validate".to_owned(),
                "panproto_reference::validation::validate_table".to_owned(),
                "panproto_reference::validation::validate_tree".to_owned(),
            ],
        ),
        (
            "validation.v2".to_owned(),
            vec!["panproto_reference::validation::validate".to_owned()],
        ),
        (
            "validation.v3".to_owned(),
            vec!["panproto_reference::validation::witness_with_trust".to_owned()],
        ),
        (
            "validation.v4".to_owned(),
            vec!["panproto_reference::validation::witness_with_trust".to_owned()],
        ),
        (
            "validation.v5".to_owned(),
            vec!["panproto_reference::validation::witness_with_trust".to_owned()],
        ),
        (
            "validation.v6".to_owned(),
            vec!["panproto_reference::validation::witness_with_trust".to_owned()],
        ),
        (
            "validation.v7".to_owned(),
            vec!["panproto_reference::validation::witness_with_trust".to_owned()],
        ),
        (
            "validation.v8".to_owned(),
            vec![
                "panproto_reference::validation::validate".to_owned(),
                "panproto_reference::validation::validate_table".to_owned(),
                "panproto_reference::validation::validate_tree".to_owned(),
            ],
        ),
        (
            "validation.v9".to_owned(),
            vec!["panproto_reference::validation::restriction_boundary".to_owned()],
        ),
        (
            "validation.v10".to_owned(),
            vec!["panproto_reference::validation::validate".to_owned()],
        ),
        (
            "validation.v11".to_owned(),
            vec!["panproto_reference::validation::validate".to_owned()],
        ),
        (
            "validation.admission-policies".to_owned(),
            vec!["panproto_reference::validation::admits".to_owned()],
        ),
        (
            "validation.key-equality-checks".to_owned(),
            vec!["panproto_reference::resolution::declared_key_equality".to_owned()],
        ),
        (
            "validation.variable-binding-checks".to_owned(),
            vec!["panproto_reference::resolution::nearest_preceding".to_owned()],
        ),
        (
            "validation.guard-target-set-sort-key-checks".to_owned(),
            vec![
                "panproto_reference::keys::target_set".to_owned(),
                "panproto_reference::schema::validate_declarations".to_owned(),
            ],
        ),
        (
            "validation.index-null-match-checks".to_owned(),
            vec![
                "panproto_reference::resolution::get".to_owned(),
                "panproto_reference::resolution::index_target".to_owned(),
                "panproto_reference::resolution::put".to_owned(),
            ],
        ),
        (
            "validation.lexical-space-advisory".to_owned(),
            vec![
                "panproto_reference::validation::classify_selector".to_owned(),
                "panproto_reference::validation::lexical_space".to_owned(),
            ],
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
    ];
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
        28,
    );
}
