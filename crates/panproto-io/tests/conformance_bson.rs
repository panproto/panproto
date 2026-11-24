//! The BSON codec against the BSON corpus of the MongoDB specifications.
//!
//! The corpus is not kept in the repository: `tools/fetch-bson-corpus.py`
//! downloads it at the commit `fixtures/conformance/bson-corpus/MANIFEST`
//! records, checks each file's SHA-256 against the manifest, and puts it in
//! `fixtures/conformance/bson-corpus/tests`. When that directory is absent
//! the test fails, naming the command, if `PANPROTO_BSON_CORPUS` is
//! `required` (as the corpus gate sets it), and otherwise reports itself
//! skipped; `conformance_bson_spec.rs` runs the original set in every build.
//!
//! Every case of every file is run. A `valid` case is run once
//! per representation it gives, with the assertions the corpus document
//! states for a codec with a native representation: canonical BSON decodes,
//! encodes back to itself (directly and through the BSON tree encoding), and
//! writes the canonical and relaxed Extended JSON of the case; canonical
//! Extended JSON reads, writes itself back and encodes to the canonical
//! BSON unless the case is lossy; relaxed Extended JSON reads and writes
//! itself back; degenerate BSON and Extended JSON read as the canonical
//! forms. Extended JSON texts are compared as JSON values, whose member
//! order and number spelling the corpus document lets a codec normalize. A
//! `decodeErrors` case must be refused by the decoder, and a `parseErrors`
//! case by the Extended JSON reader (and read by the shared JSON route, so
//! the refusal is Extended JSON's) or, for decimal128, by the decimal128
//! string parser. A representation that does not pass is a declared limit
//! in `bson-corpus-limits.txt` with the class of its refusal, which the
//! test checks, and the list must be exactly the representations that do
//! not pass. The `converted_bson` and `converted_extjson` forms of the
//! deprecated types, which the corpus offers as an optional conversion to
//! another type, are not run: the codec keeps every deprecated type.

#![cfg(feature = "tree-sitter")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use panproto_io::config::tree::read_json;
use panproto_io::data_schema::bson::{
    BsonDocument, Mode, decode, encode, from_instance, parse_decimal128, read_extended_json,
    to_instance, write_extended_json,
};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// The fetched corpus files: the file names of the manifest, which must be
/// exactly the files of the directory, beside the `REVISION` the fetch
/// writes, which must name the manifest's commit. `None` when the corpus
/// has not been fetched and is not required.
fn fetched_files() -> Option<Vec<PathBuf>> {
    let directory = corpus().join("bson-corpus/tests");
    let manifest = std::fs::read_to_string(corpus().join("bson-corpus/MANIFEST"))
        .unwrap_or_else(|e| panic!("{e}"));
    let rows: Vec<Vec<&str>> = manifest
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| line.split('\t').collect())
        .collect();
    let commit = rows
        .iter()
        .find(|row| row[0] == "source")
        .map_or_else(|| panic!("the manifest has no source line"), |row| row[2]);
    if !directory.is_dir() {
        let command = "python3 tools/fetch-bson-corpus.py";
        assert!(
            std::env::var("PANPROTO_BSON_CORPUS").as_deref() != Ok("required"),
            "the BSON corpus is required and has not been fetched: run `{command}`"
        );
        eprintln!("conformance_bson: skipped, the BSON corpus has not been fetched (`{command}`)");
        return None;
    }
    let revision = std::fs::read_to_string(directory.join("REVISION"))
        .unwrap_or_else(|e| panic!("REVISION: {e}"));
    assert_eq!(
        revision.trim(),
        commit,
        "the fetched corpus is of another commit"
    );
    let mut names: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|entry| entry.unwrap_or_else(|e| panic!("{e}")).file_name())
        .filter_map(|name| name.to_str().map(str::to_owned))
        .filter(|name| name != "REVISION")
        .collect();
    names.sort();
    let mut listed: Vec<String> = rows
        .iter()
        .filter(|row| row[0] != "source")
        .map(|row| row[0].to_owned())
        .collect();
    listed.sort();
    assert_eq!(names, listed);
    Some(names.iter().map(|name| directory.join(name)).collect())
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap_or_else(|e| panic!("{text}: {e}")))
        .collect()
}

/// Two Extended JSON texts as JSON values, which ignores member order and
/// number spelling.
fn same_json(left: &str, right: &str) -> bool {
    let parse = |text: &str| {
        serde_json::from_str::<serde_json::Value>(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    parse(left) == parse(right)
}

/// The outcome of one representation: `Ok` when it passes, else the
/// class of its refusal or of its disagreement.
type Outcome = Result<(), String>;

fn decoded(bytes: &[u8]) -> Result<BsonDocument, String> {
    decode(bytes).map_err(|e| e.kind.class().to_owned())
}

fn read(text: &str) -> Result<BsonDocument, String> {
    read_extended_json(text.as_bytes()).map_err(|e| e.kind.class().to_owned())
}

fn written(document: &BsonDocument, mode: Mode) -> Result<String, String> {
    write_extended_json(document, mode).map_err(|e| e.kind.class().to_owned())
}

fn encoded(document: &BsonDocument) -> Result<Vec<u8>, String> {
    encode(document).map_err(|e| e.kind.class().to_owned())
}

fn agree(left: &str, right: &str, what: &str) -> Outcome {
    if same_json(left, right) {
        Ok(())
    } else {
        Err(format!("mismatch {what}: {left} against {right}"))
    }
}

fn bytes_agree(left: &[u8], right: &[u8], what: &str) -> Outcome {
    if left == right {
        Ok(())
    } else {
        Err(format!("mismatch {what}"))
    }
}

/// The fields of one `valid` case.
struct Valid<'c> {
    canonical_bson: Vec<u8>,
    canonical_extjson: &'c str,
    relaxed_extjson: Option<&'c str>,
    lossy: bool,
}

fn canonical_bson(case: &Valid<'_>) -> Outcome {
    let document = decoded(&case.canonical_bson)?;
    bytes_agree(&encoded(&document)?, &case.canonical_bson, "cB to cB")?;
    let instance = to_instance(&document).map_err(|e| format!("mismatch {e}"))?;
    let again = from_instance(&instance).map_err(|e| format!("mismatch {e}"))?;
    bytes_agree(
        &encoded(&again)?,
        &case.canonical_bson,
        "cB to instance to cB",
    )?;
    let canonical = written(&document, Mode::Canonical)?;
    agree(&canonical, case.canonical_extjson, "cB to cEJ")?;
    bytes_agree(
        &encoded(&read(&canonical)?)?,
        &case.canonical_bson,
        "cB to cEJ to cB",
    )?;
    if let Some(relaxed) = case.relaxed_extjson {
        agree(&written(&document, Mode::Relaxed)?, relaxed, "cB to rEJ")?;
    }
    Ok(())
}

fn canonical_extjson(case: &Valid<'_>) -> Outcome {
    let document = read(case.canonical_extjson)?;
    agree(
        &written(&document, Mode::Canonical)?,
        case.canonical_extjson,
        "cEJ to cEJ",
    )?;
    if !case.lossy {
        bytes_agree(&encoded(&document)?, &case.canonical_bson, "cEJ to cB")?;
    }
    Ok(())
}

fn relaxed_extjson(text: &str) -> Outcome {
    agree(&written(&read(text)?, Mode::Relaxed)?, text, "rEJ to rEJ")
}

fn degenerate_bson(case: &Valid<'_>, bytes: &[u8]) -> Outcome {
    bytes_agree(
        &encoded(&decoded(bytes)?)?,
        &case.canonical_bson,
        "dB to cB",
    )
}

fn degenerate_extjson(case: &Valid<'_>, text: &str) -> Outcome {
    let document = read(text)?;
    agree(
        &written(&document, Mode::Canonical)?,
        case.canonical_extjson,
        "dEJ to cEJ",
    )?;
    if !case.lossy {
        bytes_agree(&encoded(&document)?, &case.canonical_bson, "dEJ to cB")?;
    }
    Ok(())
}

fn string_field<'v>(case: &'v serde_json::Value, key: &str) -> Option<&'v str> {
    case.get(key).and_then(serde_json::Value::as_str)
}

/// Every representation of every case, by id, with its outcome.
fn outcomes(files: Vec<PathBuf>) -> BTreeMap<String, Outcome> {
    let mut out = BTreeMap::new();
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let file: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let cases = |key: &str| {
            file.get(key)
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        for (i, case) in cases("valid").iter().enumerate() {
            let valid = Valid {
                canonical_bson: hex(string_field(case, "canonical_bson").unwrap_or_default()),
                canonical_extjson: string_field(case, "canonical_extjson").unwrap_or_default(),
                relaxed_extjson: string_field(case, "relaxed_extjson"),
                lossy: case.get("lossy").and_then(serde_json::Value::as_bool) == Some(true),
            };
            let id = |tag: &str| format!("{name}#valid#{i}#{tag}");
            out.insert(id("canonical_bson"), canonical_bson(&valid));
            out.insert(id("canonical_extjson"), canonical_extjson(&valid));
            if let Some(relaxed) = valid.relaxed_extjson {
                out.insert(id("relaxed_extjson"), relaxed_extjson(relaxed));
            }
            if let Some(bytes) = string_field(case, "degenerate_bson") {
                out.insert(id("degenerate_bson"), degenerate_bson(&valid, &hex(bytes)));
            }
            if let Some(text) = string_field(case, "degenerate_extjson") {
                out.insert(id("degenerate_extjson"), degenerate_extjson(&valid, text));
            }
        }
        for (i, case) in cases("decodeErrors").iter().enumerate() {
            let bytes = hex(string_field(case, "bson").unwrap_or_default());
            let outcome = match decode(&bytes) {
                Err(_) => Ok(()),
                Ok(_) => Err("accepted".to_owned()),
            };
            out.insert(format!("{name}#decodeErrors#{i}"), outcome);
        }
        let decimal = file.get("bson_type").and_then(serde_json::Value::as_str) == Some("0x13");
        for (i, case) in cases("parseErrors").iter().enumerate() {
            let text = string_field(case, "string").unwrap_or_default();
            let outcome = if decimal {
                parse_decimal128(text).map_or(Ok(()), |_| Err("accepted".to_owned()))
            } else if read_json(text.as_bytes()).is_err() {
                Err("not JSON".to_owned())
            } else {
                read_extended_json(text.as_bytes()).map_or(Ok(()), |_| Err("accepted".to_owned()))
            };
            out.insert(format!("{name}#parseErrors#{i}"), outcome);
        }
    }
    out
}

/// The declared limits: case id to class.
fn limits() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(corpus().join("bson-corpus-limits.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let (case, class) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("no class: {line}"));
            (case.to_owned(), class.to_owned())
        })
        .collect()
}

#[test]
fn every_bson_corpus_case_passes_or_is_a_declared_limit() {
    let Some(files) = fetched_files() else {
        return;
    };
    assert_eq!(files.len(), 31);
    let outcomes = outcomes(files);
    assert_eq!(outcomes.len(), 731 + 731 + 27 + 4 + 325 + 75 + 180);
    let limits = limits();
    let failing: BTreeMap<String, String> = outcomes
        .iter()
        .filter_map(|(id, outcome)| outcome.clone().err().map(|class| (id.clone(), class)))
        .collect();
    assert_eq!(failing, limits);
    let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
    for class in limits.values() {
        *classes.entry(class.as_str()).or_default() += 1;
    }
    assert_eq!(
        classes,
        BTreeMap::from([
            ("array-key", 3),
            ("decimal-representation", 57),
            ("non-canonical-decimal128", 3),
            ("regex-options", 2),
            ("write-nan", 7),
        ])
    );
}
