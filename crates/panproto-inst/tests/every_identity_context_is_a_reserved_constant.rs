//! Every blake3 key-derivation context is a fixed constant string.
//!
//! A context names a kind of object and the version of its encoding, so the
//! strings are part of every stored digest: they are pinned here verbatim,
//! and they must be pairwise distinct, since two kinds sharing a context
//! could collide by construction.

use std::collections::BTreeSet;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionMode, DEFAULT_VERIFY_BUDGET, DiagramNode, Operation,
    Sort, Theory, TheoryDiagram, compose,
};
use panproto_inst::identity::{
    COMPOSITION_CONTEXT, Digest, FUNCTOR_COMPLEMENT_FORMAT_VERSION,
    GRAPH_COMPLEMENT_FORMAT_VERSION, GRAPH_DELTA_CONTEXT, GRAPH_INSTANCE_CONTEXT, LENS_CONTEXT,
    MANIFEST_CONTEXT, MANIFEST_LEAF_CONTEXT, MANIFEST_NODE_CONTEXT,
    RESOLUTION_COMPLEMENT_FORMAT_VERSION, RESOLUTION_CONTEXT, RUN_JOURNAL_CONTEXT, SCHEMA_CONTEXT,
    SEGMENT_COMPLEMENT_CONTEXT, SEGMENT_CONTEXT, SEGMENT_CROSS_CONTEXT, SEGMENT_EDGE_IDS_CONTEXT,
    SEGMENT_FORMAT_VERSION, SEGMENT_MANIFEST_VERSION, SEGMENT_TASK_CONTEXT, TABLE_DELTA_CONTEXT,
    TABLE_INSTANCE_CONTEXT, THEORY_CONTEXT, TREE_COMPLEMENT_FORMAT_VERSION, TREE_EDIT_CONTEXT,
    TREE_INSTANCE_CONTEXT, VALIDATION_PLAN_CONTEXT, VALIDATION_REPORT_CONTEXT,
    VCS_GRAPH_COMPLEMENT_CONTEXT, VCS_GRAPH_DATASET_CONTEXT, VCS_GRAPH_EDIT_LOG_CONTEXT,
    VCS_TABLE_COMPLEMENT_CONTEXT, VCS_TABLE_DATASET_CONTEXT, VCS_TABLE_EDIT_LOG_CONTEXT,
};

/// Every context this module defines or re-exports, each with the string it
/// must be, except [`SCHEMA_CONTEXT`], whose version moves with the schema
/// encoding and is pinned by the golden identity file instead.
const CONTEXTS: [(&str, &str); 27] = [
    (LENS_CONTEXT, "panproto 2026-09 compiled lens identity v1"),
    (GRAPH_INSTANCE_CONTEXT, "panproto 2026-09 graph instance v1"),
    (
        VCS_GRAPH_DATASET_CONTEXT,
        "panproto 2026-09 vcs graph_dataset v1",
    ),
    (
        VCS_GRAPH_COMPLEMENT_CONTEXT,
        "panproto 2026-09 vcs graph_complement v1",
    ),
    (
        VCS_GRAPH_EDIT_LOG_CONTEXT,
        "panproto 2026-09 vcs graph_editlog v1",
    ),
    (TABLE_INSTANCE_CONTEXT, "panproto 2026-09 table instance v1"),
    (
        VCS_TABLE_DATASET_CONTEXT,
        "panproto 2026-09 vcs table_dataset v1",
    ),
    (
        VCS_TABLE_COMPLEMENT_CONTEXT,
        "panproto 2026-09 vcs table_complement v1",
    ),
    (
        VCS_TABLE_EDIT_LOG_CONTEXT,
        "panproto 2026-09 vcs table_editlog v1",
    ),
    (TREE_INSTANCE_CONTEXT, "panproto 2026-09 tree instance v1"),
    (GRAPH_DELTA_CONTEXT, "panproto 2026-09 graph delta v1"),
    (TABLE_DELTA_CONTEXT, "panproto 2026-09 table delta v1"),
    (TREE_EDIT_CONTEXT, "panproto 2026-09 tree edit v1"),
    (
        VALIDATION_PLAN_CONTEXT,
        "panproto 2026-09 validation plan v1",
    ),
    (
        VALIDATION_REPORT_CONTEXT,
        "panproto 2026-09 validation report v1",
    ),
    (SEGMENT_CONTEXT, "panproto 2026-09 instance segment v1"),
    (
        SEGMENT_COMPLEMENT_CONTEXT,
        "panproto 2026-09 complement segment v1",
    ),
    (
        SEGMENT_CROSS_CONTEXT,
        "panproto 2026-09 segment cross references v1",
    ),
    (
        SEGMENT_TASK_CONTEXT,
        "panproto 2026-09 segment task artifact v1",
    ),
    (
        MANIFEST_LEAF_CONTEXT,
        "panproto 2026-09 segment manifest leaf v1",
    ),
    (
        MANIFEST_NODE_CONTEXT,
        "panproto 2026-09 segment manifest node v1",
    ),
    (MANIFEST_CONTEXT, "panproto 2026-09 segment manifest v1"),
    (
        RUN_JOURNAL_CONTEXT,
        "panproto 2026-09 segment run journal v1",
    ),
    (
        RESOLUTION_CONTEXT,
        "panproto 2026-09 resolution identity v1",
    ),
    (THEORY_CONTEXT, "panproto 2026-09 theory identity v1"),
    (
        COMPOSITION_CONTEXT,
        "panproto 2026-09 theory composition v1",
    ),
    (
        SEGMENT_EDGE_IDS_CONTEXT,
        "panproto 2026-09 segment edge ids v1",
    ),
];

#[test]
fn every_context_is_its_contract_string() {
    for (constant, expected) in CONTEXTS {
        assert_eq!(constant, expected);
    }
}

/// Every context, [`SCHEMA_CONTEXT`] included.
fn all_contexts() -> Vec<&'static str> {
    let mut all: Vec<&str> = CONTEXTS.iter().map(|(c, _)| *c).collect();
    all.push(SCHEMA_CONTEXT);
    all
}

#[test]
fn contexts_are_pairwise_distinct() {
    let all = all_contexts();
    let distinct: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(distinct.len(), all.len());
}

/// No context carries an id: each is the fixed prefix, a kind of object and
/// a version, and nothing else.
#[test]
fn every_context_names_a_kind_and_a_version() {
    for constant in all_contexts() {
        let kind = constant
            .strip_prefix("panproto 2026-09 ")
            .unwrap_or_else(|| panic!("{constant:?} lacks the fixed prefix"));
        let (kind, version) = kind
            .rsplit_once(' ')
            .unwrap_or_else(|| panic!("{constant:?} lacks a version"));
        assert!(
            version
                .strip_prefix('v')
                .is_some_and(|n| n.parse::<u16>().is_ok()),
            "{constant:?} ends in {version:?}"
        );
        assert!(
            kind.chars()
                .all(|c| c.is_ascii_lowercase() || c == ' ' || c == '_'),
            "{constant:?} names its kind with something other than words"
        );
    }
}

#[test]
fn segment_context_strings_are_the_contract_strings() {
    assert_eq!(SEGMENT_CONTEXT, "panproto 2026-09 instance segment v1");
    assert_eq!(
        SEGMENT_COMPLEMENT_CONTEXT,
        "panproto 2026-09 complement segment v1"
    );
    assert_eq!(
        SEGMENT_CROSS_CONTEXT,
        "panproto 2026-09 segment cross references v1"
    );
    assert_eq!(
        SEGMENT_TASK_CONTEXT,
        "panproto 2026-09 segment task artifact v1"
    );
    assert_eq!(
        MANIFEST_LEAF_CONTEXT,
        "panproto 2026-09 segment manifest leaf v1"
    );
    assert_eq!(
        MANIFEST_NODE_CONTEXT,
        "panproto 2026-09 segment manifest node v1"
    );
    assert_eq!(MANIFEST_CONTEXT, "panproto 2026-09 segment manifest v1");
    assert_eq!(
        RUN_JOURNAL_CONTEXT,
        "panproto 2026-09 segment run journal v1"
    );
    assert_eq!(SEGMENT_FORMAT_VERSION, 1);
    assert_eq!(SEGMENT_MANIFEST_VERSION, 1);
}

#[test]
fn every_complement_format_version_is_one() {
    assert_eq!(TREE_COMPLEMENT_FORMAT_VERSION, 1);
    assert_eq!(GRAPH_COMPLEMENT_FORMAT_VERSION, 1);
    assert_eq!(FUNCTOR_COMPLEMENT_FORMAT_VERSION, 1);
    assert_eq!(RESOLUTION_COMPLEMENT_FORMAT_VERSION, 1);
}

/// The theory contexts this module re-exports are the constants the theory
/// encoders hash under, so a digest derived here equals the one
/// `panproto_schema` computes.
#[test]
fn the_theory_contexts_are_the_ones_the_theory_digests_use() {
    assert_eq!(
        THEORY_CONTEXT,
        panproto_schema::canonical_theory::THEORY_CONTEXT
    );
    assert_eq!(
        COMPOSITION_CONTEXT,
        panproto_schema::canonical_theory::COMPOSITION_CONTEXT
    );

    let theory = Theory::new(
        "ThGraph",
        vec![Sort::simple("V"), Sort::simple("E")],
        vec![
            Operation::unary("src", "e", "E", "V"),
            Operation::unary("tgt", "e", "E", "V"),
        ],
        vec![],
    );
    assert_eq!(
        Digest::derive(
            THEORY_CONTEXT,
            &panproto_schema::canonical_theory_bytes(&theory)
        )
        .0,
        panproto_schema::theory_digest(&theory)
    );

    let diagram = TheoryDiagram {
        nodes: vec![DiagramNode {
            label: "ThGraph".into(),
            theory,
        }],
        edges: vec![],
    };
    let opts = ComposeOptions {
        mode: CompositionMode::ExplicitIdentity,
        result_name: "Composed".into(),
        clash: ClashPolicy::Reject,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    };
    let composition =
        compose(&diagram, &opts).unwrap_or_else(|e| panic!("a one-node diagram composes: {e}"));
    assert_eq!(
        Digest::derive(
            COMPOSITION_CONTEXT,
            &panproto_schema::canonical_composition_bytes(&composition)
        )
        .0,
        panproto_schema::composition_digest(&composition)
    );
}
