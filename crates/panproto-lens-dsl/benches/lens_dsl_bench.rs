//! Lens DSL parse + compile benchmarks with a realistic AT Proto field rename.

#![allow(clippy::expect_used)]

use panproto_lens_dsl::{compile, eval};

fn main() {
    divan::main();
}

/// Rename `displayName → name` on `app.bsky.actor.profile`. Mirrors the
/// shape of a real Lexicon-evolution lens.
const BSKY_RENAME_YAML: &str = r#"
id: app.bsky.actor.profile.display-name-rename
description: "Rename displayName to name on app.bsky.actor.profile"
source: app.bsky.actor.profile
target: app.bsky.actor.profile-v2
steps:
  - rename_field:
      old: displayName
      new: name
"#;

/// Remove the `avatar` field — a realistic breaking-change scenario.
const BSKY_REMOVE_YAML: &str = r#"
id: app.bsky.actor.profile.drop-avatar
description: "Drop avatar field"
source: app.bsky.actor.profile
target: app.bsky.actor.profile-minimal
steps:
  - remove_field: avatar
"#;

#[divan::bench]
fn parse_rename_yaml(bencher: divan::Bencher) {
    bencher.bench(|| eval::eval_yaml(BSKY_RENAME_YAML));
}

#[divan::bench]
fn parse_remove_yaml(bencher: divan::Bencher) {
    bencher.bench(|| eval::eval_yaml(BSKY_REMOVE_YAML));
}

#[divan::bench]
fn parse_and_compile_rename(bencher: divan::Bencher) {
    bencher.bench(|| {
        let doc = eval::eval_yaml(BSKY_RENAME_YAML).expect("yaml");
        compile(&doc, "app.bsky.actor.profile", &|_| None)
    });
}
