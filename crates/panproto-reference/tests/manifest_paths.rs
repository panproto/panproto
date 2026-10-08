#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Deterministic audit of every semantic path in the schema-v2 manifest.
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

fn documented_paths() -> std::collections::BTreeSet<String> {
    include_str!("../src/lib.rs")
        .split("[`")
        .skip(1)
        .filter_map(|tail| tail.split_once("`]").map(|(path, _)| path))
        .filter(|path| !path.contains(char::is_whitespace))
        .map(|path| format!("panproto_reference::{path}"))
        .collect()
}

#[test]
fn every_manifest_reference_path_has_a_strict_rustdoc_link() {
    let root = repository_root();
    let manifest_text =
        std::fs::read_to_string(root.join("notes/graph-parity/records/correctness_manifest.json"))
            .unwrap();
    let manifest = panproto_reference::manifest::load(&manifest_text).unwrap();
    let expected: std::collections::BTreeSet<_> = manifest
        .pages
        .values()
        .flat_map(|page| &page.laws)
        .flat_map(|law| &law.references)
        .cloned()
        .collect();
    let documented = documented_paths();
    let audited: std::collections::BTreeSet<_> =
        documented.intersection(&expected).cloned().collect();
    assert_eq!(audited.len(), 144);
    assert_eq!(audited, expected);
}
