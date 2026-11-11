//! Every payload the C ABI hands out is a function of its inputs alone.
//!
//! A schema's vertex, edge, constraint, required-edge and NSID tables are
//! `HashMap`s, and a `HashMap` enumerates its entries in an order its random
//! hash keys decide. An encoder that writes a result in the order it meets
//! those entries hands the keys a say in the bytes: the same two schemas then
//! diff to different payloads in different processes, and a host that stores,
//! compares or hashes a payload sees a change where there is none.
//!
//! Each payload below is computed from freshly parsed schemas, so every run
//! builds its tables under new hash keys. The payload is computed many times in
//! this process, on several threads (each thread draws its own keys), and in
//! many separate processes, and every copy must be byte-identical. Run a child
//! directly with `PP_C_PAYLOAD_DUMP` set to a payload's name to see one
//! process's answer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use panproto_c::api::{
    pp_buf_free, pp_check_classify, pp_check_diff_full, pp_check_diff_simple, pp_handle_free,
    pp_protocol_define, pp_registry_get_builtin, pp_schema_metadata,
    pp_schema_parse_atproto_lexicon,
};
use panproto_c::error::PpStatus;
use safer_ffi::prelude::*;

/// How many separate processes each payload is compared across.
const PROCESSES: usize = 16;

/// How many times each payload is recomputed inside one process.
const REPEATS: usize = 16;

/// How many threads recompute each payload, each under its own hash keys.
const THREADS: usize = 4;

/// Every payload under test, by the name a child process is asked for.
const PAYLOADS: &[&str] = &[
    "schema-metadata-post",
    "diff-simple-post-profile",
    "diff-full-post-profile",
    "compat-report",
];

fn lexicon(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/atproto/lexicons")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Take the bytes out of an ABI buffer and free it.
fn take(buf: repr_c::Vec<u8>) -> Vec<u8> {
    let bytes = buf.to_vec();
    pp_buf_free(buf);
    bytes
}

fn status_ok(status: i32, what: &str) {
    assert_eq!(status, PpStatus::Ok as i32, "{what} failed");
}

/// Parse an `ATProto` lexicon through the ABI, as a host does.
fn parse(name: &str) -> u32 {
    let json = lexicon(name);
    let mut handle = 0_u32;
    status_ok(
        pp_schema_parse_atproto_lexicon(json.as_slice().into(), &mut handle),
        "pp_schema_parse_atproto_lexicon",
    );
    handle
}

/// The named payload, computed from schemas parsed for this call alone.
fn payload(name: &str) -> Vec<u8> {
    let post = parse("app.bsky.feed.post.json");
    let profile = parse("app.bsky.actor.profile.json");
    let mut out: repr_c::Vec<u8> = Vec::new().into();
    let bytes = match name {
        "schema-metadata-post" => {
            status_ok(pp_schema_metadata(post, &mut out), "pp_schema_metadata");
            take(out)
        }
        "diff-simple-post-profile" => {
            status_ok(
                pp_check_diff_simple(post, profile, &mut out),
                "pp_check_diff_simple",
            );
            take(out)
        }
        "diff-full-post-profile" => {
            status_ok(
                pp_check_diff_full(post, profile, &mut out),
                "pp_check_diff_full",
            );
            take(out)
        }
        "compat-report" => {
            status_ok(
                pp_check_diff_full(post, profile, &mut out),
                "pp_check_diff_full",
            );
            let diff = take(out);

            let mut spec: repr_c::Vec<u8> = Vec::new().into();
            status_ok(
                pp_registry_get_builtin(b"atproto".as_slice().into(), &mut spec),
                "pp_registry_get_builtin",
            );
            let spec = take(spec);
            let mut proto = 0_u32;
            status_ok(
                pp_protocol_define(spec.as_slice().into(), &mut proto),
                "pp_protocol_define",
            );

            let mut report: repr_c::Vec<u8> = Vec::new().into();
            status_ok(
                pp_check_classify(proto, diff.as_slice().into(), &mut report),
                "pp_check_classify",
            );
            let _ = pp_handle_free(proto);
            take(report)
        }
        other => panic!("no payload named {other}"),
    };
    let _ = pp_handle_free(post);
    let _ = pp_handle_free(profile);
    bytes
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The child half: one process's bytes, printed.
#[test]
fn one_process_answer() {
    let Some(name) = std::env::var_os("PP_C_PAYLOAD_DUMP") else {
        // Nothing to do: the cross-process test below runs this with the
        // variable set. Left as a normal test so it type-checks in every run.
        return;
    };
    print!("<<<{}>>>", hex(&payload(&name.to_string_lossy())));
}

fn child_answer(exe: &std::path::Path, name: &str) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_C_PAYLOAD_DUMP", name)
        .output()
        .unwrap_or_else(|e| panic!("re-running the test binary: {e}"));
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text
        .find("<<<")
        .unwrap_or_else(|| panic!("the child printed no answer for {name}: {text}"));
    let end = text
        .find(">>>")
        .unwrap_or_else(|| panic!("the child did not close its answer for {name}"));
    text[start + 3..end].to_owned()
}

#[test]
fn each_payload_is_the_same_on_every_call_and_every_thread() {
    if std::env::var_os("PP_C_PAYLOAD_DUMP").is_some() {
        return;
    }
    let mut unstable: Vec<String> = Vec::new();
    for name in PAYLOADS {
        let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();
        for _ in 0..REPEATS {
            seen.insert(payload(name));
        }
        let answers: Vec<Vec<u8>> = std::thread::scope(|scope| {
            // Every worker is spawned before any is joined, so they run at once.
            let mut workers = Vec::with_capacity(THREADS);
            for _ in 0..THREADS {
                workers.push(scope.spawn(|| payload(name)));
            }
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|_| panic!("{name}: a worker thread panicked"))
                })
                .collect()
        });
        seen.extend(answers);
        if seen.len() != 1 {
            unstable.push(format!(
                "{name}: {} calls on {} threads wrote {} different encodings",
                REPEATS + THREADS,
                THREADS + 1,
                seen.len()
            ));
        }
    }
    assert!(unstable.is_empty(), "{}", unstable.join("\n"));
}

#[test]
fn each_payload_is_the_same_in_every_process() {
    if std::env::var_os("PP_C_PAYLOAD_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("locating the test binary: {e}"));
    let mut unstable: Vec<String> = Vec::new();
    for name in PAYLOADS {
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for _ in 0..PROCESSES {
            *seen.entry(child_answer(&exe, name)).or_insert(0) += 1;
        }
        if seen.len() != 1 {
            unstable.push(format!(
                "{name}: {PROCESSES} processes wrote {} different encodings",
                seen.len()
            ));
        } else if !seen.contains_key(&hex(&payload(name))) {
            unstable.push(format!(
                "{name}: the child processes agree with each other but not with this one"
            ));
        }
    }
    assert!(unstable.is_empty(), "{}", unstable.join("\n"));
}
