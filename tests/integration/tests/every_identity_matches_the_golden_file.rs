//! Every schema and lens in the identity corpus has the digest the golden
//! file records.
//!
//! The digests are what a complement's envelope and a stored object name,
//! so they must not change by accident: a change to the canonical schema
//! encoding, to the compiled-lens encoding, or to an envelope field is a
//! change to every stored identity. Such a change bumps the context string
//! (and `IDENTITY_ENVELOPE_VERSION`) in the same change, and this test is
//! what fails when it does not.
//!
//! Regenerate after a deliberate change with
//! `PP_IDENTITY_GOLDEN=bless cargo test -p panproto-integration-tests --test every_identity_matches_the_golden_file`.

#![allow(
    clippy::expect_used,
    reason = "a corpus entry that will not build is a defect in the fixture"
)]

mod identity_corpus;

use std::collections::{BTreeMap, HashMap};

use identity_corpus::{Golden, build_lens, load_schemas, read_golden, repo_root};
use panproto_inst::identity::{
    Digest, GRAPH_INSTANCE_CONTEXT, IDENTITY_ENVELOPE_VERSION, LENS_CONTEXT, SCHEMA_CONTEXT,
    VCS_GRAPH_COMPLEMENT_CONTEXT, VCS_GRAPH_DATASET_CONTEXT, VCS_GRAPH_EDIT_LOG_CONTEXT,
    canonical_compiled_bytes, schema_digest,
};

fn contexts() -> BTreeMap<String, String> {
    [
        ("schema", SCHEMA_CONTEXT),
        ("lens", LENS_CONTEXT),
        ("graph_instance", GRAPH_INSTANCE_CONTEXT),
        ("vcs_graph_dataset", VCS_GRAPH_DATASET_CONTEXT),
        ("vcs_graph_complement", VCS_GRAPH_COMPLEMENT_CONTEXT),
        ("vcs_graph_edit_log", VCS_GRAPH_EDIT_LOG_CONTEXT),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

fn blessing() -> bool {
    std::env::var("PP_IDENTITY_GOLDEN").is_ok_and(|v| v == "bless")
}

/// The corpus as this build computes it, in the golden file's shape.
fn recompute(golden: &Golden) -> Golden {
    let schemas = load_schemas(golden);
    let mut out = golden.clone();
    out.contexts = contexts();
    out.envelope_version = IDENTITY_ENVELOPE_VERSION;
    for entry in &mut out.schemas {
        entry.digest = schema_digest(&schemas[&entry.name]).to_hex();
    }
    for entry in &mut out.lenses {
        let lens = build_lens(entry, &schemas).unwrap_or_else(|e| panic!("{e}"));
        entry.envelope = lens.identity(entry.shape);
    }
    out
}

#[test]
fn every_identity_matches_the_golden_file() {
    let golden = read_golden();
    let now = recompute(&golden);

    if blessing() {
        let path = repo_root().join(identity_corpus::GOLDEN);
        let mut text = serde_json::to_string_pretty(&now).expect("the corpus serializes");
        text.push('\n');
        std::fs::write(&path, text).expect("the golden file is writable");
        return;
    }

    assert_eq!(
        golden.contexts, now.contexts,
        "a context string changed; bump its version and re-bless"
    );
    assert_eq!(golden.envelope_version, now.envelope_version);
    let mut drift = Vec::new();
    for (was, is) in golden.schemas.iter().zip(&now.schemas) {
        if was.digest != is.digest {
            drift.push(format!(
                "schema {}: {} -> {}",
                was.name, was.digest, is.digest
            ));
        }
    }
    for (was, is) in golden.lenses.iter().zip(&now.lenses) {
        if was.envelope != is.envelope {
            drift.push(format!(
                "lens {}: {:?} -> {:?}",
                was.name, was.envelope, is.envelope
            ));
        }
    }
    assert!(
        drift.is_empty(),
        "identities drifted from the golden file. If the encoding changed on purpose, bump \
         the context version and re-bless:\n{}",
        drift.join("\n")
    );
}

/// Two corpus schemas share a digest exactly when they share canonical
/// bytes, and two lenses exactly when they share both schemas and a
/// compiled encoding. No lens digest is a schema digest.
#[test]
fn distinct_content_has_distinct_digests() {
    let golden = read_golden();
    let schemas = load_schemas(&golden);

    let mut by_digest: HashMap<Digest, (String, Vec<u8>)> = HashMap::new();
    for entry in &golden.schemas {
        let schema = &schemas[&entry.name];
        let bytes = panproto_schema::canonical_bytes(schema);
        let digest = schema_digest(schema);
        if let Some((other, other_bytes)) = by_digest.get(&digest) {
            assert_eq!(
                other_bytes, &bytes,
                "{} and {other} share a digest but not their content",
                entry.name
            );
        } else {
            for (other, (other_name, other_bytes)) in &by_digest {
                assert!(
                    other_bytes != &bytes,
                    "{} and {other_name} share their content but not a digest ({other})",
                    entry.name
                );
            }
            by_digest.insert(digest, (entry.name.clone(), bytes));
        }
    }

    let mut lens_by_digest: HashMap<Digest, (String, Vec<u8>)> = HashMap::new();
    for entry in &golden.lenses {
        let lens = build_lens(entry, &schemas).unwrap_or_else(|e| panic!("{e}"));
        let envelope = lens.identity(entry.shape);
        assert!(
            !by_digest.contains_key(&envelope.lens),
            "lens {} has a schema's digest",
            entry.name
        );
        let mut content = envelope.source_schema.0.to_vec();
        content.extend_from_slice(&envelope.target_schema.0);
        content.extend(canonical_compiled_bytes(&lens.compiled));
        if let Some((other, other_content)) = lens_by_digest.get(&envelope.lens) {
            assert_eq!(
                other_content, &content,
                "lenses {} and {other} share a digest but not their content",
                entry.name
            );
        } else {
            for (other_name, other_content) in lens_by_digest.values() {
                assert!(
                    other_content != &content,
                    "lenses {} and {other_name} share their content but not a digest",
                    entry.name
                );
            }
            lens_by_digest.insert(envelope.lens, (entry.name.clone(), content));
        }
    }
}
