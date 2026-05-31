//! Corpus-driven emit verification audit.
//!
//! For each protocol, runs the strengthened emit oracle (byte fixed point +
//! vertex-kind multiset + edge-shape multiset preservation) over **every**
//! entry in the grammar author's own `test/corpus/` — the inputs that
//! exercise the full grammar, not a single hand-written sample. A protocol is
//! genuinely emit-verified only when it round-trips every corpus entry that
//! parses without error.
//!
//! Run as a report (lists pass/fail counts per protocol and the first failing
//! snippet) over a comma-separated `PP_AUDIT` protocol list:
//!
//! ```text
//! PP_AUDIT=rust,go,python cargo test -p panproto-parse \
//!     --all-features --test emit_corpus_audit -- --nocapture audit
//! ```

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use panproto_parse::ParserRegistry;
use panproto_schema::{edge_multiset, kind_multiset};

fn corpus_dir(protocol: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../grammars")
        .join(protocol)
        .join("test/corpus")
}

/// One corpus entry's source, with `:skip` / `:error` entries dropped.
fn corpus_sources(protocol: &str) -> Vec<(String, String)> {
    let dir = corpus_dir(protocol);
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                stack.extend(rd.filter_map(|e| e.ok().map(|e| e.path())));
            }
        } else if p.extension().is_some_and(|e| e == "txt") {
            files.push(p);
        }
    }
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        parse_corpus_file(&text, &mut out);
    }
    out
}

/// Parse one corpus file into (name, source) entries. The tree-sitter format:
/// a header block `===…` / name / (optional `:attr` lines) / `===…`, then the
/// source, then a `---…` divider, then the expected sexp (ignored), repeating.
fn parse_corpus_file(text: &str, out: &mut Vec<(String, String)>) {
    let lines: Vec<&str> = text.lines().collect();
    let is_eq = |l: &str| l.len() >= 3 && l.chars().all(|c| c == '=');
    let is_dash = |l: &str| l.len() >= 3 && l.chars().all(|c| c == '-');
    let mut i = 0;
    while i < lines.len() {
        if !is_eq(lines[i]) {
            i += 1;
            continue;
        }
        // Header: `===` / name / [attrs] / `===`.
        let name = lines.get(i + 1).copied().unwrap_or("").to_string();
        let mut j = i + 2;
        let mut skip = false;
        while j < lines.len() && !is_eq(lines[j]) {
            let t = lines[j].trim();
            if t == ":skip" || t == ":error" || t.starts_with(":error") {
                skip = true;
            }
            j += 1;
        }
        // j is the closing `===`. Source runs until the `---` divider.
        let mut k = j + 1;
        let src_start = k;
        while k < lines.len() && !is_dash(lines[k]) {
            k += 1;
        }
        let src = lines[src_start..k].join("\n");
        if !skip && !src.trim().is_empty() {
            out.push((name, src));
        }
        // Advance past the sexp to the next header.
        let mut m = k + 1;
        while m < lines.len() && !is_eq(lines[m]) {
            m += 1;
        }
        i = m;
    }
}

struct Tally {
    total: usize,
    passed: usize,
    parse_err: usize,
    first_fail: Option<(String, String, String)>, // name, e1, e2-or-reason
}

fn audit(protocol: &str) -> Tally {
    let reg = ParserRegistry::new();
    let file = format!("sample.{protocol}");
    let mut t = Tally {
        total: 0,
        passed: 0,
        parse_err: 0,
        first_fail: None,
    };
    for (name, src) in corpus_sources(protocol) {
        let bytes = src.as_bytes();
        let Ok(s1) = reg.parse_with_protocol(protocol, bytes, &file) else {
            t.parse_err += 1;
            continue;
        };
        // Skip entries that do not cleanly parse: empty, or containing tree-
        // sitter ERROR / MISSING recovery nodes. Many corpus files include
        // intentional-error tests (`Error detected at …`) that exercise the
        // parser's recovery, not the emitter.
        let has_error = s1.vertices.values().any(|v| {
            matches!(v.kind.as_ref(), "ERROR" | "MISSING") || v.kind.as_ref().contains("ERROR")
        });
        if s1.vertices.is_empty() || has_error {
            t.parse_err += 1;
            continue;
        }
        t.total += 1;
        let e1 = match reg.emit_pretty_with_protocol(protocol, &s1) {
            Ok(b) => b,
            Err(e) => {
                if t.first_fail.is_none() {
                    t.first_fail = Some((name, format!("EMIT-ERR {e}"), String::new()));
                }
                continue;
            }
        };
        let Ok(s2) = reg.parse_with_protocol(protocol, &e1, &file) else {
            if t.first_fail.is_none() {
                t.first_fail = Some((
                    name,
                    format!("REPARSE-ERR e1={:?}", String::from_utf8_lossy(&e1)),
                    String::new(),
                ));
            }
            continue;
        };
        let e2 = reg
            .emit_pretty_with_protocol(protocol, &s2)
            .unwrap_or_default();
        let ok = e1 == e2
            && kind_multiset(&s1) == kind_multiset(&s2)
            && edge_multiset(&s1) == edge_multiset(&s2);
        if ok {
            t.passed += 1;
        } else if t.first_fail.is_none() {
            t.first_fail = Some((
                name,
                String::from_utf8_lossy(&e1).into_owned(),
                String::from_utf8_lossy(&e2).into_owned(),
            ));
        }
    }
    t
}

/// The **strip-complement** structural audit: the verification bar for
/// the *canonical section* (the transpilation path).
///
/// Where [`audit`] keeps the parser's layout complement and checks a
/// byte fixed point, this strips the entire layout fibre via
/// [`Schema::forget_layout`] (byte spans, interstitials, `chose-alt-*`,
/// and the `ptrace-*` variant tag) to simulate a by-construction /
/// transpiled abstract schema that never carried a parse trace. It then
/// emits through the canonical section, reparses, and checks **only
/// structural equivalence** (kind- and edge-multiset) — there is no
/// complement to reproduce the original bytes with, so a byte fixed
/// point is not the right bar here. This measures whether grammar
/// unification alone yields a structurally faithful emit.
fn strip_audit(protocol: &str) -> Tally {
    let reg = ParserRegistry::new();
    let file = format!("sample.{protocol}");
    let mut t = Tally {
        total: 0,
        passed: 0,
        parse_err: 0,
        first_fail: None,
    };
    for (name, src) in corpus_sources(protocol) {
        let Ok(s1) = reg.parse_with_protocol(protocol, src.as_bytes(), &file) else {
            t.parse_err += 1;
            continue;
        };
        let has_error = s1.vertices.values().any(|v| {
            matches!(v.kind.as_ref(), "ERROR" | "MISSING") || v.kind.as_ref().contains("ERROR")
        });
        if s1.vertices.is_empty() || has_error {
            t.parse_err += 1;
            continue;
        }
        t.total += 1;
        // Forget the entire layout fibre: the abstract (transpiled) schema.
        let abstract_schema = s1.forget_layout();
        let e1 = match reg.emit_pretty_with_protocol(protocol, &abstract_schema) {
            Ok(b) => b,
            Err(e) => {
                if t.first_fail.is_none() {
                    t.first_fail = Some((name, format!("EMIT-ERR {e}"), String::new()));
                }
                continue;
            }
        };
        let Ok(s2) = reg.parse_with_protocol(protocol, &e1, &file) else {
            if t.first_fail.is_none() {
                t.first_fail = Some((
                    name,
                    format!("REPARSE-ERR e1={:?}", String::from_utf8_lossy(&e1)),
                    String::new(),
                ));
            }
            continue;
        };
        // Structural equivalence only (no byte fixed point without a
        // complement). Compare against the abstract schema's structure.
        let ok = kind_multiset(&abstract_schema) == kind_multiset(&s2)
            && edge_multiset(&abstract_schema) == edge_multiset(&s2);
        if ok {
            t.passed += 1;
        } else if t.first_fail.is_none() {
            t.first_fail = Some((
                name,
                String::from_utf8_lossy(&e1).into_owned(),
                String::new(),
            ));
        }
    }
    t
}

/// Report mode for the strip-complement (canonical-section) audit.
/// `PP_STRIP_AUDIT=proto1,proto2,...`
#[test]
fn corpus_strip_audit_report() {
    let Ok(list) = std::env::var("PP_STRIP_AUDIT") else {
        eprintln!("set PP_STRIP_AUDIT=proto1,proto2,... to run the strip-complement audit");
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            for proto in list.split(',').filter(|s| !s.is_empty()) {
                let t = strip_audit(proto);
                let status = if t.total > 0 && t.passed == t.total {
                    "FULL-PASS"
                } else {
                    "PARTIAL"
                };
                eprintln!(
                    "{status} {proto} (strip): {}/{} structurally faithful ({} parse-skipped)",
                    t.passed, t.total, t.parse_err
                );
                if let Some((name, e1, _)) = t.first_fail {
                    let e1s: String = e1.chars().take(100).collect();
                    eprintln!("    first fail [{name}]: E1={e1s:?}");
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// The protocols whose `emit_pretty` round-trips EVERY entry of their
/// grammar's vendored `test/corpus/` under the full oracle. These are the
/// corpus-verified members of `VERIFIED_EMIT_PROTOCOLS`; their corpus is
/// committed under `grammars/<name>/test/corpus/` so this test is a permanent,
/// CI-runnable regression guard (not a one-off audit).
const CORPUS_VERIFIED: &[&str] = &[
    "arduino",
    "bass",
    "chuck",
    "fidl",
    "firrtl",
    "graphql",
    "gstlaunch",
    "json",
    "ungrammar",
];

#[test]
fn corpus_verified_protocols_round_trip_full_corpus() {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            for proto in CORPUS_VERIFIED {
                // Skip protocols not compiled into this build (the file is
                // exercised in full only under `--all-features`).
                if ParserRegistry::new()
                    .parse_with_protocol(proto, b"", &format!("e.{proto}"))
                    .err()
                    .is_some_and(|e| {
                        matches!(e, panproto_parse::ParseError::UnknownLanguage { .. })
                    })
                {
                    continue;
                }
                let entries = corpus_sources(proto);
                assert!(
                    !entries.is_empty(),
                    "{proto}: no vendored corpus at {} — corpus-verified protocols must \
                     ship their grammar's test/corpus",
                    corpus_dir(proto).display()
                );
                let t = audit(proto);
                if let Some((name, e1, e2)) = &t.first_fail {
                    let e1s: String = e1.chars().take(120).collect();
                    let e2s: String = e2.chars().take(120).collect();
                    panic!(
                        "{proto} is corpus-verified but failed corpus entry [{name}]: \
                         {}/{} passed.\nE1={e1s:?}\nE2={e2s:?}",
                        t.passed, t.total
                    );
                }
                assert_eq!(
                    t.passed, t.total,
                    "{proto}: {}/{} corpus entries pass",
                    t.passed, t.total
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Protocols whose **canonical section** (grammar unification, no
/// complement) emits a structurally-faithful representative of EVERY
/// vendored corpus entry — the strip-complement bar. This is the
/// transpilation guarantee: even with the entire layout fibre forgotten,
/// emit reconstructs the same kind- and edge-multiset. A permanent
/// CI regression guard for the canonical-section dispatch.
const STRIP_VERIFIED: &[&str] = &[
    "arduino",
    "bass",
    "fidl",
    "firrtl",
    "graphql",
    "gstlaunch",
    "json",
    "ungrammar",
];

#[test]
fn strip_complement_canonical_section_is_structurally_faithful() {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            for proto in STRIP_VERIFIED {
                if ParserRegistry::new()
                    .parse_with_protocol(proto, b"", &format!("e.{proto}"))
                    .err()
                    .is_some_and(|e| {
                        matches!(e, panproto_parse::ParseError::UnknownLanguage { .. })
                    })
                {
                    continue;
                }
                let entries = corpus_sources(proto);
                assert!(!entries.is_empty(), "{proto}: no vendored corpus");
                let t = strip_audit(proto);
                if let Some((name, e1, _)) = &t.first_fail {
                    let e1s: String = e1.chars().take(120).collect();
                    panic!(
                        "{proto} strip-complement (canonical section) failed entry [{name}]: \
                         {}/{} structurally faithful.\nE1={e1s:?}",
                        t.passed, t.total
                    );
                }
                assert_eq!(
                    t.passed, t.total,
                    "{proto}: {}/{} structurally faithful under strip-complement",
                    t.passed, t.total
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn corpus_audit_report() {
    let Ok(list) = std::env::var("PP_AUDIT") else {
        eprintln!("set PP_AUDIT=proto1,proto2,... to run the audit");
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let mut full_pass = Vec::new();
            for proto in list.split(',').filter(|s| !s.is_empty()) {
                let t = audit(proto);
                let status = if t.total > 0 && t.passed == t.total {
                    full_pass.push(proto.to_string());
                    "FULL-PASS"
                } else {
                    "PARTIAL"
                };
                eprintln!(
                    "{status} {proto}: {}/{} corpus entries pass ({} parse-skipped)",
                    t.passed, t.total, t.parse_err
                );
                if let Some((name, e1, e2)) = t.first_fail {
                    let e1s: String = e1.chars().take(80).collect();
                    let e2s: String = e2.chars().take(80).collect();
                    eprintln!("    first fail [{name}]: E1={e1s:?} E2={e2s:?}");
                }
            }
            eprintln!(
                "\nFULL-PASS protocols ({}): {}",
                full_pass.len(),
                full_pass.join(",")
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
