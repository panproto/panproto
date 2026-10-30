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
        369
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
    assert_eq!(expected.len(), 240);
    assert_eq!(documented.len(), 240);
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

/// The validation page's laws and their reference paths, in manifest order.
const VALIDATION_INCIDENCES: &[(&str, &[&str])] = &[
    (
        "validation.v1",
        &[
            "panproto_reference::validation::validate",
            "panproto_reference::validation::validate_table",
            "panproto_reference::validation::validate_tree",
        ],
    ),
    (
        "validation.v2",
        &["panproto_reference::validation::validate"],
    ),
    (
        "validation.v3",
        &[
            "panproto_reference::validation::witness_with_trust",
            "panproto_reference::validation::incremental_footprint",
        ],
    ),
    (
        "validation.v4",
        &[
            "panproto_reference::validation::witness_with_trust",
            "panproto_reference::validation::incremental_report",
        ],
    ),
    (
        "validation.v5",
        &["panproto_reference::validation::witness_with_trust"],
    ),
    (
        "validation.v6",
        &["panproto_reference::validation::witness_with_trust"],
    ),
    (
        "validation.v7",
        &["panproto_reference::validation::witness_with_trust"],
    ),
    (
        "validation.v8",
        &[
            "panproto_reference::validation::validate",
            "panproto_reference::validation::validate_table",
            "panproto_reference::validation::validate_tree",
        ],
    ),
    (
        "validation.v9",
        &["panproto_reference::validation::restriction_boundary"],
    ),
    (
        "validation.v10",
        &["panproto_reference::validation::validate"],
    ),
    (
        "validation.v11",
        &["panproto_reference::validation::validate"],
    ),
    (
        "validation.admission-policies",
        &["panproto_reference::validation::admits"],
    ),
    (
        "validation.key-equality-checks",
        &["panproto_reference::resolution::declared_key_equality"],
    ),
    (
        "validation.variable-binding-checks",
        &["panproto_reference::resolution::nearest_preceding"],
    ),
    (
        "validation.guard-target-set-sort-key-checks",
        &[
            "panproto_reference::keys::target_set",
            "panproto_reference::schema::validate_declarations",
        ],
    ),
    (
        "validation.index-null-match-checks",
        &[
            "panproto_reference::resolution::get",
            "panproto_reference::resolution::put",
            "panproto_reference::schema::validate_declarations",
        ],
    ),
    (
        "validation.lexical-space-advisory",
        &[
            "panproto_reference::validation::classify_selector",
            "panproto_reference::validation::lexical_space",
        ],
    ),
    (
        "validation.annotated-format-no-assertion-equivalence",
        &["panproto_reference::validation::annotated_format_parse"],
    ),
    (
        "validation.annotated-format-advisory-exactness",
        &["panproto_reference::validation::annotated_format_advisory"],
    ),
    (
        "validation.format-assertion-acceptance",
        &["panproto_reference::validation::format_assertion_acceptance"],
    ),
];

#[test]
fn validation_law_to_public_path_incidences_are_exact() {
    let manifest_text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
    let manifest = panproto_reference::manifest::load(&manifest_text).unwrap();
    let page = &manifest.pages["validation"];
    let expected = VALIDATION_INCIDENCES
        .iter()
        .map(|(law, paths)| {
            (
                (*law).to_owned(),
                paths
                    .iter()
                    .map(|path| (*path).to_owned())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
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
        30,
    );
}

/// Pages whose block-named tests must equal their manifest consumers exactly.
/// Drift on every other page with a correctness chapter is printed so that the
/// owning page can repair it.
const CONSUMER_STRICT_PAGES: [&str; 1] = ["lenses"];

/// One test a correctness block names: its function name and, when the block
/// line states a `-p <crate> --test <binary>` command, the binary
/// `<crate>::<binary>` that holds it.
type NamedTest = (String, Option<String>);

fn json_member<'a>(
    value: &'a panproto_reference::json::Value,
    key: &str,
) -> Option<&'a panproto_reference::json::Value> {
    let panproto_reference::json::Value::Object(members) = value else {
        return None;
    };
    members
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value)
}

fn json_str<'a>(value: &'a panproto_reference::json::Value, key: &str) -> Option<&'a str> {
    match json_member(value, key)? {
        panproto_reference::json::Value::String(text) => Some(text),
        _ => None,
    }
}

fn json_array<'a>(
    value: &'a panproto_reference::json::Value,
    key: &str,
) -> &'a [panproto_reference::json::Value] {
    match json_member(value, key) {
        Some(panproto_reference::json::Value::Array(items)) => items,
        _ => &[],
    }
}

fn is_test_identifier(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

/// The binary a nextest or cargo-test command names, as `<crate>::<binary>`.
fn command_binary(command: &str) -> Option<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let after = |flag: &str| {
        words
            .windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].to_owned())
    };
    Some(format!("{}::{}", after("-p")?, after("--test")?))
}

/// The tests named on every `- ... test:` or `- ... tests:` line of the block
/// whose heading carries `{#anchor}`.
fn block_named_tests(chapter: &str, anchor: &str) -> Option<Vec<NamedTest>> {
    let start = chapter.find(&format!("{{#{anchor}}}"))?;
    let block = &chapter[start..];
    let block = block.find("\n## ").map_or(block, |end| &block[..end]);
    let mut named = Vec::new();
    for line in block.lines() {
        let Some(rest) = line.strip_prefix("- ") else {
            continue;
        };
        let Some((label, body)) = rest.split_once(": ") else {
            continue;
        };
        if !(label.ends_with(" test") || label.ends_with(" tests")) {
            continue;
        }
        let spans: Vec<&str> = body.split('`').skip(1).step_by(2).collect();
        let mut pending: Vec<String> = Vec::new();
        for span in spans {
            if let Some((name, command)) = span.split_once("; command: ") {
                if is_test_identifier(name) {
                    named.push((name.to_owned(), command_binary(command)));
                }
            } else if let Some(command) = span.strip_prefix("command: ") {
                let binary = command_binary(command);
                named.extend(
                    std::mem::take(&mut pending)
                        .into_iter()
                        .map(|name| (name, binary.clone())),
                );
            } else if is_test_identifier(span) {
                pending.push(span.to_owned());
            }
        }
        named.extend(pending.into_iter().map(|name| (name, None)));
    }
    Some(named)
}

/// Whether the test source of `<crate>::<binary>` defines `fn <test>`.
fn binary_defines(workspace: &std::path::Path, binary: &str, test: &str) -> bool {
    let Some((krate, target)) = binary.split_once("::") else {
        return false;
    };
    let tests = workspace.join("crates").join(krate).join("tests");
    let needle = format!("fn {test}(");
    [
        tests.join(format!("{target}.rs")),
        tests.join(target).join("main.rs"),
    ]
    .iter()
    .any(|path| std::fs::read_to_string(path).is_ok_and(|source| source.contains(&needle)))
}

/// Every difference between a law's block-named tests and its consumers.
fn consumer_drift(
    workspace: &std::path::Path,
    law: &panproto_reference::json::Value,
    named: &[NamedTest],
) -> Vec<String> {
    let consumers: Vec<(String, String)> = json_array(law, "consumers")
        .iter()
        .filter_map(|consumer| {
            Some((
                json_str(consumer, "test")?.to_owned(),
                json_str(consumer, "binary")?.to_owned(),
            ))
        })
        .collect();
    let named_names: std::collections::BTreeSet<&str> =
        named.iter().map(|(name, _)| name.as_str()).collect();
    let consumer_names: std::collections::BTreeSet<&str> =
        consumers.iter().map(|(name, _)| name.as_str()).collect();
    let mut drift = Vec::new();
    for name in named_names.difference(&consumer_names) {
        drift.push(format!("block names `{name}`, which is not a consumer"));
    }
    for name in consumer_names.difference(&named_names) {
        drift.push(format!("consumer `{name}` is not named by the block"));
    }
    for (name, binary) in named {
        if let Some(binary) = binary
            && consumer_names.contains(name.as_str())
            && !consumers
                .iter()
                .any(|(test, held)| test == name && held == binary)
        {
            drift.push(format!(
                "block runs `{name}` in `{binary}`; no consumer does"
            ));
        }
    }
    for (name, binary) in &consumers {
        if binary.contains("::") && !binary_defines(workspace, binary, name) {
            drift.push(format!("consumer `{name}` is not defined in `{binary}`"));
        }
    }
    drift
}

#[test]
fn block_named_tests_are_exactly_the_manifest_consumers() {
    let manifest_text = std::fs::read_to_string(correctness_manifest_path()).unwrap();
    let manifest = panproto_reference::json::parse(&manifest_text).unwrap();
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let pages = json_member(&manifest, "pages").unwrap();
    let panproto_reference::json::Value::Object(pages) = pages else {
        panic!("manifest.pages is not an object");
    };
    let mut strict_drift = Vec::new();
    let mut strict_laws = 0_usize;
    for (slug, page) in pages {
        let chapter_path = workspace
            .join("book/src/correctness")
            .join(format!("{slug}.md"));
        let Ok(chapter) = std::fs::read_to_string(&chapter_path) else {
            continue;
        };
        let strict = CONSUMER_STRICT_PAGES.contains(&slug.as_str());
        for law in json_array(page, "laws") {
            let id = json_str(law, "id").unwrap();
            let anchor = json_str(law, "anchor").unwrap();
            let drift = block_named_tests(&chapter, anchor).map_or_else(
                || vec![format!("no block carries {{#{anchor}}}")],
                |named| consumer_drift(&workspace, law, &named),
            );
            if strict {
                strict_laws += 1;
            }
            for item in drift {
                let line = format!("{id} ({anchor}): {item}");
                if strict {
                    strict_drift.push(line);
                } else {
                    eprintln!("consumer drift outside the strict pages: {line}");
                }
            }
        }
    }
    assert_eq!(strict_laws, 20, "the lenses page registers twenty laws");
    assert!(
        strict_drift.is_empty(),
        "block-named tests differ from manifest consumers:\n{}",
        strict_drift.join("\n")
    );
}
