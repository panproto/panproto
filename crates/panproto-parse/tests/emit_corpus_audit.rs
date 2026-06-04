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

use std::collections::BTreeMap;
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
        } else {
            // Tree-sitter's own test runner reads every file under
            // `test/corpus/` regardless of extension. Some grammars name
            // their corpus after the language (scheme -> `.scm`, etc.); the
            // header/divider format is identical. Accept `.txt` anywhere, and
            // any file living under a `corpus` directory.
            let under_corpus = p.components().any(|c| c.as_os_str() == "corpus");
            if under_corpus || p.extension().is_some_and(|e| e == "txt") {
                files.push(p);
            }
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
        // A truncated/malformed trailing header can leave `j` at end-of-file,
        // so clamp the source window to the available range.
        let mut k = (j + 1).min(lines.len());
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

/// Source-EXACTNESS probe (`PP_SF=lang1,lang2`): for every cleanly-parsing
/// corpus entry, report whether `emit(parse(src)) == src` (modulo a single
/// trailing newline). This is a STRICTER bar than the project's `verified`
/// standard, which is the [`audit`] fixed-point + AST-multiset gate: most
/// verified protocols legitimately REFORMAT (`json` pretty-prints arrays,
/// `go` normalizes spacing) and so score low here while being correctly verified.
/// So a low `PP_SF` score is NOT grounds to reject a promotion. What this probe
/// IS good for: (a) identifying the byte-EXACT subset (`toml`/`circom`/`cylc` emit
/// their corpus verbatim), and (b) as one input — alongside the gate's
/// multiset and an eyeball — to spot genuine content DROP/CORRUPTION (the
/// `todotxt`/`wolfram` degenerate class), distinguished from benign reformatting
/// by reading the emitted output.
#[test]
fn corpus_source_faithful_report() {
    let Ok(langs) = std::env::var("PP_SF") else {
        return;
    };
    let reg = ParserRegistry::new();
    for proto in langs.split(',').filter(|s| !s.is_empty()) {
        let file = format!("sample.{proto}");
        let (mut total, mut faithful, mut firstbad) = (0usize, 0usize, None);
        for (name, src) in corpus_sources(proto) {
            let bytes = src.as_bytes();
            let Ok(s1) = reg.parse_with_protocol(proto, bytes, &file) else {
                continue;
            };
            let has_error = s1.vertices.values().any(|v| {
                matches!(v.kind.as_ref(), "ERROR" | "MISSING") || v.kind.as_ref().contains("ERROR")
            });
            if s1.vertices.is_empty() || has_error {
                continue;
            }
            total += 1;
            let Ok(e1) = reg.emit_pretty_with_protocol(proto, &s1) else {
                if firstbad.is_none() {
                    firstbad = Some((name.clone(), "EMIT-ERR".to_string()));
                }
                continue;
            };
            // Compare modulo a single trailing newline on either side.
            let trim = |b: &[u8]| -> Vec<u8> {
                let mut v = b.to_vec();
                if v.last() == Some(&b'\n') {
                    v.pop();
                }
                v
            };
            if trim(&e1) == trim(bytes) {
                faithful += 1;
            } else if firstbad.is_none() {
                let s: String = String::from_utf8_lossy(&e1).chars().take(80).collect();
                firstbad = Some((name.clone(), format!("emit={s:?}")));
            }
        }
        let verdict = if total > 0 && faithful == total {
            "SOURCE-FAITHFUL"
        } else {
            "DEGENERATE/PARTIAL"
        };
        println!(
            "PP_SF {proto}: {faithful}/{total} source-faithful [{verdict}] first-bad={firstbad:?}"
        );
    }
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

/// Three-grade classification of canonical-section (abstract-schema)
/// emit, per the project's grading scheme:
///
/// * **Grade 1** — emit does not reconstruct the AST: the re-parse of the
///   emitted bytes differs structurally (kind/edge multiset) from the
///   abstract schema, or does not parse / emit at all. Either a genuine
///   information loss (a distinction held only in a stripped anonymous
///   token) or an emit bug.
/// * **Grade 2** — emit reconstructs the AST (re-parse is structurally
///   identical) but the bytes differ from the original source. A correct
///   program with non-canonical-looking formatting.
/// * **Grade 3** — emit reconstructs the AST *and* is byte-identical to
///   the source. The canonical formatting happened to match.
///
/// Grade 3 ⊆ Grade 2; "Grade 2+" (2 or 3) is the meaningful transpilation
/// bar (the emitted program is the same program). Returns
/// `(grade1, grade2, grade3, parse_skipped)` counts over the corpus.
fn grade_audit(protocol: &str) -> (usize, usize, usize, usize) {
    let reg = ParserRegistry::new();
    let file = format!("sample.{protocol}");
    let (mut g1, mut g2, mut g3, mut skip) = (0usize, 0usize, 0usize, 0usize);
    for (_name, src) in corpus_sources(protocol) {
        let Ok(s1) = reg.parse_with_protocol(protocol, src.as_bytes(), &file) else {
            skip += 1;
            continue;
        };
        let has_error = s1.vertices.values().any(|v| {
            matches!(v.kind.as_ref(), "ERROR" | "MISSING") || v.kind.as_ref().contains("ERROR")
        });
        if s1.vertices.is_empty() || has_error {
            skip += 1;
            continue;
        }
        let abstract_schema = s1.forget_layout();
        let Ok(e1) = reg.emit_pretty_with_protocol(protocol, &abstract_schema) else {
            g1 += 1;
            continue;
        };
        let Ok(s2) = reg.parse_with_protocol(protocol, &e1, &file) else {
            g1 += 1;
            continue;
        };
        let ast_equal = kind_multiset(&abstract_schema) == kind_multiset(&s2)
            && edge_multiset(&abstract_schema) == edge_multiset(&s2);
        if !ast_equal {
            g1 += 1;
        } else if e1 == src.as_bytes() {
            g3 += 1;
        } else {
            g2 += 1;
        }
    }
    (g1, g2, g3, skip)
}

/// Emit a single source snippet through the canonical section and print
/// the result + AST-equivalence verdict. `PP_EMIT_PROTO=rust
/// PP_EMIT_SRC='let s = "hi";'`.
#[test]
fn emit_one_probe() {
    let (Ok(proto), Ok(src)) = (std::env::var("PP_EMIT_PROTO"), std::env::var("PP_EMIT_SRC"))
    else {
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let reg = ParserRegistry::new();
            let s1 = reg
                .parse_with_protocol(&proto, src.as_bytes(), "probe")
                .expect("parse");
            // PP_STRIP_BYTE reproduces emit_pretty_core_pack's partial
            // strip (byte spans + interstitials only, keeping ptrace /
            // chose-alt / field constraints) to diagnose the replay path;
            // default is the full canonical-section strip.
            let abstract_schema = if std::env::var("PP_REPLAY").is_ok() {
                // Keep the FULL complement: the byte-exact replay path that
                // the byte-FP corpus audit exercises.
                s1
            } else if std::env::var("PP_STRIP_BYTE").is_ok() {
                let mut s = s1;
                for constraints in s.constraints.values_mut() {
                    constraints.retain(|c| {
                        let so = c.sort.as_ref();
                        !(so == "start-byte" || so == "end-byte" || so.starts_with("interstitial-"))
                    });
                }
                s
            } else {
                s1.forget_layout()
            };
            if std::env::var("PP_DUMP").is_ok() {
                for (id, v) in &abstract_schema.vertices {
                    let mut kids = abstract_schema
                        .edges
                        .iter()
                        .filter(|(e, _)| &e.src == id)
                        .filter_map(|(e, ord)| {
                            abstract_schema.vertices.get(&e.tgt).map(|cv| {
                                (
                                    ord.as_ref().to_string(),
                                    format!("{}={}", e.kind.as_ref(), cv.kind.as_ref()),
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    kids.sort();
                    let kids: Vec<String> = kids.into_iter().map(|(_, s)| s).collect();
                    let cons: Vec<String> = abstract_schema
                        .constraints
                        .get(id)
                        .map(|cs| {
                            cs.iter()
                                .map(|c| format!("{}={:?}", c.sort.as_ref(), c.value))
                                .collect()
                        })
                        .unwrap_or_default();
                    eprintln!(
                        "DUMP {} [{}] kids={kids:?} cons={cons:?}",
                        v.kind.as_ref(),
                        id.as_ref()
                    );
                }
            }
            let e1 = reg
                .emit_pretty_with_protocol(&proto, &abstract_schema)
                .expect("emit");
            eprintln!("PROBE src={src:?}");
            eprintln!("PROBE out={:?}", String::from_utf8_lossy(&e1));
            if let Ok(s2) = reg.parse_with_protocol(&proto, &e1, "probe2") {
                let ok = kind_multiset(&abstract_schema) == kind_multiset(&s2)
                    && edge_multiset(&abstract_schema) == edge_multiset(&s2);
                eprintln!("PROBE ast_equal={ok}");
                if !ok {
                    let ka = kind_multiset(&abstract_schema);
                    let kb = kind_multiset(&s2);
                    let mut keys: std::collections::BTreeSet<String> = ka.keys().cloned().collect();
                    keys.extend(kb.keys().cloned());
                    for k in keys {
                        let d = i64::try_from(*kb.get(&k).unwrap_or(&0)).unwrap_or(0)
                            - i64::try_from(*ka.get(&k).unwrap_or(&0)).unwrap_or(0);
                        if d != 0 {
                            eprintln!("  delta {k}{d:+}");
                        }
                    }
                }
            } else {
                eprintln!("PROBE reparse FAILED");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// List the NAMES of corpus entries that FAIL the byte-FP audit (the
/// `audit` fixed-point + multiset check), for before/after diffing.
/// `PP_BYTEFAIL=proto`.
#[test]
fn corpus_bytefail_report() {
    let Ok(proto) = std::env::var("PP_BYTEFAIL") else {
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let reg = ParserRegistry::new();
            let file = format!("sample.{proto}");
            for (name, src) in corpus_sources(&proto) {
                let Ok(s1) = reg.parse_with_protocol(&proto, src.as_bytes(), &file) else {
                    continue;
                };
                let has_error = s1.vertices.values().any(|v| {
                    matches!(v.kind.as_ref(), "ERROR" | "MISSING")
                        || v.kind.as_ref().contains("ERROR")
                });
                if s1.vertices.is_empty() || has_error {
                    continue;
                }
                let Ok(e1) = reg.emit_pretty_with_protocol(&proto, &s1) else {
                    eprintln!("BYTEFAIL[{name}] EMIT-ERR");
                    continue;
                };
                let Ok(s2) = reg.parse_with_protocol(&proto, &e1, &file) else {
                    eprintln!("BYTEFAIL[{name}] REPARSE-ERR");
                    continue;
                };
                let e2 = reg
                    .emit_pretty_with_protocol(&proto, &s2)
                    .unwrap_or_default();
                let ok = e1 == e2
                    && kind_multiset(&s1) == kind_multiset(&s2)
                    && edge_multiset(&s1) == edge_multiset(&s2);
                if !ok {
                    eprintln!("BYTEFAIL[{name}]");
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// For each Grade-1 entry, print the kind-multiset DELTA (abstract vs
/// re-parse) so the dominant breakage cause is visible. `PP_G1=proto`.
#[test]
fn corpus_g1_diff_report() {
    let Ok(proto) = std::env::var("PP_G1") else {
        eprintln!("set PP_G1=proto for the Grade-1 kind-delta report");
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let reg = ParserRegistry::new();
            let file = format!("sample.{proto}");
            let mut delta_tally: BTreeMap<String, i64> = BTreeMap::new();
            let mut shown = 0;
            for (name, src) in corpus_sources(&proto) {
                let Ok(s1) = reg.parse_with_protocol(&proto, src.as_bytes(), &file) else {
                    continue;
                };
                if s1.vertices.is_empty()
                    || s1
                        .vertices
                        .values()
                        .any(|v| v.kind.as_ref().contains("ERROR"))
                {
                    continue;
                }
                let abstract_schema = s1.forget_layout();
                let Ok(e1) = reg.emit_pretty_with_protocol(&proto, &abstract_schema) else {
                    continue;
                };
                let Ok(s2) = reg.parse_with_protocol(&proto, &e1, &file) else {
                    eprintln!("G1[{name}] REPARSE-FAIL");
                    continue;
                };
                let ka = kind_multiset(&abstract_schema);
                let kb = kind_multiset(&s2);
                if ka == kb && edge_multiset(&abstract_schema) == edge_multiset(&s2) {
                    continue;
                }
                // accumulate per-kind delta (re-parse minus abstract)
                let mut keys: std::collections::BTreeSet<String> = ka.keys().cloned().collect();
                keys.extend(kb.keys().cloned());
                let mut line = Vec::new();
                for k in keys {
                    let d = i64::try_from(*kb.get(&k).unwrap_or(&0)).unwrap_or(0)
                        - i64::try_from(*ka.get(&k).unwrap_or(&0)).unwrap_or(0);
                    if d != 0 {
                        *delta_tally.entry(k.clone()).or_default() += d;
                        line.push(format!("{k}{d:+}"));
                    }
                }
                let cap = std::env::var("PP_G1_CAP")
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(8);
                if shown < cap {
                    eprintln!("G1[{name}]: {}", line.join(" "));
                    shown += 1;
                }
            }
            let mut sorted: Vec<_> = delta_tally.into_iter().collect();
            sorted.sort_by_key(|(_, d)| -(d.abs()));
            eprintln!("--- aggregate kind delta (top) ---");
            for (k, d) in sorted.into_iter().take(15) {
                eprintln!("  {k}: {d:+}");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Report the three-grade distribution. `PP_GRADE=proto1,proto2,...`
#[test]
fn corpus_grade_audit_report() {
    let Ok(list) = std::env::var("PP_GRADE") else {
        eprintln!("set PP_GRADE=proto1,proto2,... for the three-grade report");
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            for proto in list.split(',').filter(|s| !s.is_empty()) {
                let (g1, g2, g3, skip) = grade_audit(proto);
                let total = g1 + g2 + g3;
                let g2plus = g2 + g3;
                eprintln!(
                    "{proto}: Grade2+ (AST-faithful) {g2plus}/{total} \
                     [G1(broken)={g1} G2(reformatted)={g2} G3(byte-exact)={g3}] \
                     ({skip} parse-skipped)",
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
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
    "actionscript",
    "ada",
    "agda",
    "al",
    "angular",
    "apex",
    "arduino",
    "astro",
    "awk",
    "bash",
    "bass",
    "beancount",
    "bicep",
    "blade",
    "brightscript",
    "c",
    "cairo",
    "capnp",
    "chatito",
    "chuck",
    "circom",
    "clarity",
    "clojure",
    "cmake",
    "commonlisp",
    "cpon",
    "cpp",
    "crystal",
    "csharp",
    "css",
    "csv",
    "cuda",
    "cue",
    "cylc",
    "d",
    "dart",
    "desktop",
    "devicetree",
    "diff",
    "dockerfile",
    "dot",
    "doxygen",
    "dtd",
    "earthfile",
    "editorconfig",
    "eex",
    "elisp",
    "elixir",
    "elm",
    "enforce",
    "erlang",
    "facility",
    "faust",
    "fennel",
    "fidl",
    "firrtl",
    "fish",
    "foam",
    "forth",
    "fortran",
    "fsharp",
    "fsharp_signature",
    "gdscript",
    "git_config",
    "git_rebase",
    "gitattributes",
    "gitcommit",
    "gitignore",
    "gleam",
    "glsl",
    "go",
    "godot_resource",
    "gomod",
    "graphql",
    "gstlaunch",
    "hack",
    "haxe",
    "hcl",
    "heex",
    "hlsl",
    "html",
    "hyprlang",
    "idris",
    "ini",
    "ispc",
    "janet",
    "java",
    "jinja2",
    "jsdoc",
    "json",
    "jsonnet",
    "just",
    "kdl",
    "kotlin",
    "latex",
    "lean",
    "ledger",
    "lilypond",
    "lua",
    "luadoc",
    "magik",
    "matlab",
    "mermaid",
    "meson",
    "mojo",
    "netlinx",
    "nginx",
    "nickel",
    "nim",
    "nix",
    "nqc",
    "objc",
    "ocaml",
    "ocaml_interface",
    "odin",
    "pascal",
    "pem",
    "pgn",
    "php",
    "pkl",
    "pony",
    "postscript",
    "powershell",
    "printf",
    "prolog",
    "promql",
    "properties",
    "protobuf",
    "psv",
    "pug",
    "purescript",
    "pymanifest",
    "ql",
    "qml",
    "qmldir",
    "query",
    "qvr",
    "r",
    "readline",
    "regex",
    "rego",
    "requirements",
    "robot",
    "ron",
    "rst",
    "ruby",
    "rust",
    "scss",
    "solidity",
    "sparql",
    "sql",
    "ssh_config",
    "starlark",
    "strudel_mini",
    "supercollider",
    "svelte",
    "swift",
    "tcl",
    "teal",
    "templ",
    "terraform",
    "thrift",
    "tlaplus",
    "tmux",
    "toml",
    "tsv",
    "tsx",
    "turtle",
    "twig",
    "typescript",
    "udev",
    "ungrammar",
    "uxntal",
    "v",
    "verilog",
    "vhdl",
    "vim",
    "vue",
    "wit",
    "xcompose",
    "yuck",
    "zsh",
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
    "cairo",
    "circom",
    "csv",
    "dot",
    "elisp",
    "fidl",
    "firrtl",
    "glsl",
    "go",
    "godot_resource",
    "graphql",
    "gstlaunch",
    "html",
    "janet",
    "java",
    "json",
    "nqc",
    "prolog",
    "promql",
    "qmldir",
    "rego",
    "solidity",
    "sparql",
    "strudel_mini",
    "turtle",
    "ungrammar",
    "xcompose",
    "yuck",
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

/// The unified `emit` is total: an abstract (by-construction / transpiled)
/// schema carrying NO layout complement now emits via the canonical
/// section instead of erroring with "schema has no text fragments". This
/// is the parse-level unification of the reconstruction flow
/// (`emit_from_schema` replay) and the canonical flow (`emit_pretty`).
#[cfg(feature = "lang-json")]
#[test]
fn unified_emit_handles_abstract_schema_via_canonical_section() {
    let reg = ParserRegistry::new();
    let parsed = reg
        .parse_with_protocol("json", br#"{"a": [1, 2]}"#, "x.json")
        .expect("parse json");
    // Forget the entire layout fibre → a by-construction abstract schema.
    let abstract_schema = parsed.forget_layout();
    assert!(
        !abstract_schema
            .constraints
            .values()
            .any(|cs| cs.iter().any(|c| c.sort.as_ref() == "start-byte")),
        "forget_layout must drop the start-byte anchors"
    );
    // The unified emit (replay-or-canonical) must now succeed via the
    // canonical section and reparse to the same structure.
    let bytes = reg
        .emit_with_protocol("json", &abstract_schema)
        .expect("unified emit must handle a complement-free schema");
    let reparsed = reg
        .parse_with_protocol("json", &bytes, "y.json")
        .expect("emit output must reparse");
    assert_eq!(
        kind_multiset(&abstract_schema),
        kind_multiset(&reparsed),
        "canonical-section emit must be structurally faithful"
    );
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

#[test]
fn corpus_degeneracy_report() {
    let Ok(list) = std::env::var("PP_DEGEN") else {
        return;
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let reg = ParserRegistry::new();
            // With PP_DEGEN_STRIP=1, run emit through the canonical section
            // (forget_layout) instead of the replay path: this validates a
            // STRIP candidate against content corruption (the ispc
            // `unsigned`->`signed` anonymous-token-CHOICE class) that the
            // structural-only strip gate cannot see.
            let strip_mode = std::env::var("PP_DEGEN_STRIP").is_ok();
            // The non-whitespace character multiset of a byte slice.
            let char_multiset = |b: &[u8]| -> BTreeMap<u8, usize> {
                let mut m = BTreeMap::new();
                for c in b.iter().filter(|c| !c.is_ascii_whitespace()) {
                    *m.entry(*c).or_insert(0) += 1;
                }
                m
            };
            for proto in list.split(',').filter(|s| !s.is_empty()) {
                let file = format!("sample.{proto}");
                let mut first_divergence: Option<(String, String, String)> = None;
                let mut flagged = 0usize;
                let mut total = 0usize;
                for (name, src) in corpus_sources(proto) {
                    let Ok(s1) = reg.parse_with_protocol(proto, src.as_bytes(), &file) else {
                        continue;
                    };
                    let has_error = s1.vertices.values().any(|v| {
                        matches!(v.kind.as_ref(), "ERROR" | "MISSING")
                            || v.kind.as_ref().contains("ERROR")
                    });
                    if s1.vertices.is_empty() || has_error {
                        continue;
                    }
                    let schema = if strip_mode {
                        s1.forget_layout()
                    } else {
                        s1.clone()
                    };
                    let Ok(e1) = reg.emit_pretty_with_protocol(proto, &schema) else {
                        continue;
                    };
                    total += 1;
                    // Spacing reformatting is legitimate; a difference in the
                    // non-whitespace character multiset is a content drop or
                    // substitution.
                    if char_multiset(src.as_bytes()) != char_multiset(&e1) {
                        flagged += 1;
                        if first_divergence.is_none() {
                            let src_sample: String = src.chars().take(120).collect();
                            let out_sample: String =
                                String::from_utf8_lossy(&e1).chars().take(120).collect();
                            first_divergence = Some((name.clone(), src_sample, out_sample));
                        }
                    }
                }
                if let Some((name, src_sample, out_sample)) = first_divergence {
                    eprintln!(
                        "DEGEN-SUSPECT {proto}: {flagged}/{total} entries change the \
                         non-whitespace char multiset; first [{name}]\n  IN ={src_sample:?}\n  OUT={out_sample:?}"
                    );
                } else {
                    eprintln!("CLEAN {proto}: 0/{total} entries change the char multiset");
                }
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
