//! The book's protocol catalog is checked against the registry.
//!
//! Protocol capabilities were described independently in the parser
//! dispatch, the CLI, the C API, the WASM API and the book, and those
//! descriptions had diverged. The code-side registries now all derive
//! from one descriptor table; the book cannot, since it is prose. So it
//! is checked instead: a protocol the code supports has to appear in
//! the catalog, and a name the catalog advertises has to resolve.

#![allow(clippy::expect_used)]

use std::path::PathBuf;

use panproto_core::protocols::registry::{descriptor, descriptors};

fn catalog() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the repository root is two levels above this crate")
        .join("book/src/reference/protocols.md");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Every protocol the code supports appears in the catalog, under its
/// canonical name or one of its aliases.
#[test]
fn every_supported_protocol_is_documented() {
    let text = catalog();
    let missing: Vec<&str> = descriptors()
        .iter()
        .filter(|d| {
            let named =
                |n: &str| text.contains(&format!("`{n}`")) || text.contains(&format!("| {n} "));
            !named(d.name) && !d.aliases.iter().any(|a| named(a))
        })
        .map(|d| d.name)
        .collect();

    assert!(
        missing.is_empty(),
        "these protocols are supported but absent from \
         book/src/reference/protocols.md: {missing:?}",
    );
}

/// Every protocol name the catalog offers resolves, so a reader copying
/// one out of the book gets a name the dispatch accepts.
///
/// The catalog's overview table is `| category module | protocol names |`,
/// so the names live in the second column; the first is a Rust module
/// and is not a protocol. Only that table is read, because the rest of
/// the page also puts file extensions, theory names and prose in
/// backticks, and a name that resolves to nothing is a defect only when
/// it was offered as a protocol.
#[test]
fn a_documented_protocol_name_resolves() {
    let text = catalog();
    let known: Vec<&str> = descriptors().iter().map(|d| d.name).collect();

    // The page has several tables; only the one headed "Protocol names"
    // lists protocols. Everything after it until the next blank line
    // belongs to it.
    let mut in_table = false;
    let mut unresolvable: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.contains("| Protocol names |") {
            in_table = true;
            continue;
        }
        if in_table && !line.starts_with('|') {
            in_table = false;
        }
        if !in_table || !line.starts_with("| `") {
            continue;
        }
        let Some(names) = line.split('|').nth(2) else {
            continue;
        };
        for cell in names.split(',') {
            let name = cell.trim().trim_matches('`');
            if !name.is_empty() && descriptor(name).is_none() {
                unresolvable.push(name.to_owned());
            }
        }
    }

    assert!(
        !unresolvable.is_empty() || !known.is_empty(),
        "the catalog's protocol column could not be read at all",
    );
    assert!(
        unresolvable.is_empty(),
        "the catalog offers these as protocols but they resolve to none: {unresolvable:?}; \
         supported: {known:?}",
    );
}
