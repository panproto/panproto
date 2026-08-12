//! The measured lexicon corpus, shared by the sweep, the shape snapshot and
//! the benchmarks.
//!
//! Seventy-seven lexicons live in the repository: seven `atproto` fixtures
//! under `fixtures/atproto/lexicons/` and seventy `dev.panproto` definitions
//! under `lexicons/dev/panproto/`. They are the corpus every performance claim
//! about the span search is stated against, so they are read from disk rather
//! than re-declared here: a lexicon added to the repository joins the sweep
//! without anyone remembering to list it.
//!
//! The paths are resolved from `CARGO_MANIFEST_DIR`, which the compiler
//! substitutes as an absolute path, so a consumer runs the same whatever the
//! working directory is.

#![allow(
    dead_code,
    reason = "each consumer uses a different part of this module"
)]

use std::path::{Path, PathBuf};

use panproto_schema::Schema;

/// One lexicon, parsed.
pub struct Lexicon {
    /// The NSID the document declares, which is what a row in a snapshot or a
    /// failure message names it by.
    pub nsid: String,
    /// The `main` definition's `type`, one of `record`, `query`, `procedure`
    /// or `subscription`.
    pub main_type: String,
    /// The schema the lexicon parses to under the `atproto` protocol.
    pub schema: Schema,
}

/// The repository root, two levels above this crate.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate directory has a grandparent")
        .to_path_buf()
}

/// Every `.json` under `dir`, recursively, in ascending path order.
fn json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            json_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
}

/// The whole corpus, parsed, sorted by NSID.
///
/// Sorted so that a pair index means the same thing on every machine: the
/// stride sample in the benchmarks and the row order in the snapshot both
/// depend on it.
///
/// # Panics
///
/// If a committed fixture does not parse. A malformed fixture is a defect in
/// the repository rather than a result to report.
pub fn corpus() -> Vec<Lexicon> {
    let root = repo_root();
    let mut paths = Vec::new();
    json_files(&root.join("fixtures/atproto/lexicons"), &mut paths);
    json_files(&root.join("lexicons/dev/panproto"), &mut paths);

    let mut lexicons: Vec<Lexicon> = paths
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            let json: serde_json::Value = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()));
            let nsid = json["id"]
                .as_str()
                .unwrap_or_else(|| panic!("{} declares no id", path.display()))
                .to_owned();
            let main_type = json["defs"]["main"]["type"]
                .as_str()
                .unwrap_or("<none>")
                .to_owned();
            let schema = panproto_protocols::atproto::parse_lexicon(&json)
                .unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
            Lexicon {
                nsid,
                main_type,
                schema,
            }
        })
        .collect();

    lexicons.sort_by(|a, b| a.nsid.cmp(&b.nsid));
    assert_eq!(
        lexicons.len(),
        77,
        "the corpus is the whole lexicon set; a file was added or removed and the counts the \
         sweep and the snapshot are stated against no longer describe it"
    );
    lexicons
}

/// The forty-two lexicons whose `main` definition is a record.
///
/// A record carries a full property graph; a query or a procedure carries its
/// parameters and its output, which are shallower and less interesting to
/// align. The shape snapshot takes these because eight hundred and sixty-one
/// unordered pairs of them is a file a reviewer can read, where the full
/// corpus is not.
///
/// # Panics
///
/// If the count moved, for the same reason [`corpus`] panics.
pub fn record_typed() -> Vec<Lexicon> {
    let records: Vec<Lexicon> = corpus()
        .into_iter()
        .filter(|lexicon| lexicon.main_type == "record")
        .collect();
    assert_eq!(
        records.len(),
        42,
        "the record-typed subset moved, so the eight hundred and sixty-one pairs the shape \
         snapshot records are no longer the pairs it was written against"
    );
    records
}

/// One named lexicon, for a benchmark that measures a single pair.
///
/// # Panics
///
/// If `text` does not parse.
pub fn parse(text: &str) -> Schema {
    let json: serde_json::Value = serde_json::from_str(text).expect("the fixture is JSON");
    panproto_protocols::atproto::parse_lexicon(&json).expect("the fixture parses as a schema")
}
