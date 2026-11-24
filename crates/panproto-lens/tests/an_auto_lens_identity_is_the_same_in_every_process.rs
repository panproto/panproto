//! The identity of an auto-generated lens is a function of its two schemas.
//!
//! `auto_generate` builds a protolens chain and instantiates it into a
//! compiled migration whose digest is the lens part of the identity envelope.
//! Any step that walks a hash map while it builds that migration (the
//! resolver, the vertex and edge maps, the field transforms) would make the
//! digest depend on the process's hash seed. This computes the envelopes of
//! four corpus lenses in several separate processes and demands one answer,
//! and pins that answer.
//!
//! Run the child directly with `PP_AUTO_LENS_ID_DUMP=1` to see one process's
//! answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use panproto_inst::ComplementShape;
use panproto_lens::{AutoLensConfig, auto_generate};
use panproto_schema::Schema;

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 3;

/// The lens digest of each corpus lens. The same lenses are entries of the
/// integration identity corpus, `tests/fixtures/identity/golden.json`; a
/// change here is a change of lens identity and re-blesses that file too.
const PINNED: [(&str, &str); 4] = [
    (
        "like-to-follow",
        "5affcaa86235d68bb707677fe0d66e98fbe614f6e2ffc204ce2e1283b1a0eb69",
    ),
    (
        "profile-tests-to-profile",
        "d25b35cb900dc8e64c2a488e109870cb515296a4737db398be8f8b24cea86cd6",
    ),
    (
        "resource-to-descriptor",
        "253633f36845663d924669507566918158ef1241a5ac68830ce618708a03b88e",
    ),
    (
        "trace-to-descriptor",
        "accacb804fe7aa9280910f27ba03e2976fc197aa4f7a4e551f92226620264853",
    ),
];

/// The corpus lenses: name, protocol, source file and target file, relative
/// to the repository root.
const LENSES: [(&str, &str, &str, &str); 4] = [
    (
        "like-to-follow",
        "atproto",
        "fixtures/atproto/lexicons/app.bsky.feed.like.json",
        "fixtures/atproto/lexicons/app.bsky.graph.follow.json",
    ),
    (
        "profile-tests-to-profile",
        "atproto",
        "tests/fixtures/atproto/app.bsky.actor.profile.json",
        "fixtures/atproto/lexicons/app.bsky.actor.profile.json",
    ),
    (
        "resource-to-descriptor",
        "protobuf",
        "fixtures/protobuf/resource.proto",
        "fixtures/protobuf/descriptor.proto",
    ),
    (
        "trace-to-descriptor",
        "protobuf",
        "fixtures/protobuf/trace.proto",
        "fixtures/protobuf/descriptor.proto",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(protocol: &str, file: &str) -> Schema {
    let path = repo_root().join(file);
    let text = std::fs::read_to_string(&path).expect("the corpus file reads");
    if panproto_protocols::document_parser_protocols().contains(&protocol) {
        let document: serde_json::Value =
            serde_json::from_str(&text).expect("the corpus document is JSON");
        panproto_protocols::parse_schema_document(protocol, &document)
    } else {
        panproto_protocols::parse_schema_source(protocol, &text)
    }
    .expect("the corpus schema parses")
}

/// Each corpus lens's name and the lens digest of its tree identity.
fn digests() -> Vec<(String, String)> {
    LENSES
        .iter()
        .map(|(name, protocol, src, tgt)| {
            let descriptor = panproto_protocols::registry::descriptor(protocol)
                .expect("the protocol is registered");
            let lens = auto_generate(
                &load(protocol, src),
                &load(protocol, tgt),
                &(descriptor.protocol)(),
                &AutoLensConfig::default(),
            )
            .expect("the corpus lens is found")
            .lens;
            let envelope = lens.identity(ComplementShape::WType);
            let json = serde_json::to_value(envelope).expect("an envelope serializes");
            (
                (*name).to_owned(),
                json["lens"]
                    .as_str()
                    .expect("the envelope carries a lens digest")
                    .to_owned(),
            )
        })
        .collect()
}

/// The child half: one process's digests, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_AUTO_LENS_ID_DUMP").is_none() {
        return;
    }
    let answer: Vec<String> = digests()
        .into_iter()
        .map(|(name, digest)| format!("{name}={digest}"))
        .collect();
    print!("<<<{}>>>", answer.join(","));
}

fn child_answer(exe: &Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_AUTO_LENS_ID_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

#[test]
fn the_auto_lens_identity_does_not_depend_on_the_hash_seed() {
    if std::env::var_os("PP_AUTO_LENS_ID_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut distinct: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *distinct.entry(child_answer(&exe)).or_insert(0) += 1;
    }
    assert_eq!(
        distinct.len(),
        1,
        "{PROCESSES} processes gave {} different answers:\n{distinct:#?}",
        distinct.len()
    );
    let pinned: Vec<String> = PINNED
        .iter()
        .map(|(name, digest)| format!("{name}={digest}"))
        .collect();
    assert_eq!(distinct.keys().next(), Some(&pinned.join(",")));
}
