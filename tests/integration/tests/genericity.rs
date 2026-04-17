//! Genericity guardrail: enforce protocol-genericity of the generic crates.
//!
//! The autolens plan requires that protocol-specific terms (atproto, lexicon,
//! protobuf, graphql, mysql, and so on) live only in `panproto-protocols/` and
//! test corpora. The generic kernel crates (`panproto-gat`, `panproto-schema`,
//! `panproto-inst`, `panproto-mig`, `panproto-lens`, `panproto-lens-dsl`,
//! `panproto-check`, `panproto-core`, `panproto-cli`, `panproto-py`,
//! `panproto-wasm`) must be free of such leakage.
//!
//! Two tests encode the rule:
//!
//! 1. `no_protocol_names_in_generic_crates` scans source files of the generic
//!    crates for a denylist of protocol/format/database names.
//! 2. `no_programming_language_names_as_identifiers` scans the same files for
//!    type, function, and module definitions whose names embed a programming
//!    language name (Rust, Python, TypeScript, and so on).
//!
//! Both tests read the real tree on disk; they are not no-ops. A pair of
//! synthetic fixtures (`good.rs`, `bad.rs`) under `genericity_fixtures/` is
//! used by a third test to validate the scanners themselves.
//!
//! ## Baseline and ratchet
//!
//! The project predates this guardrail; the generic crates already contain a
//! large set of pre-existing protocol-name references (mostly NSID/ATProto
//! vestiges in doc comments, schema field names, and validation messages).
//! Removing those is a separate cleanup task covering dozens of files. To
//! still enforce the rule against **new** leakage, each real-tree scan compares
//! its findings against a baseline file
//! (`tests/integration/tests/genericity_baseline.txt`) and fails only if
//! - a new violation is present that is not in the baseline, or
//! - a baseline entry no longer appears in the tree (baseline is stale).
//!
//! Regenerate the baseline by running with `UPDATE_GENERICITY_BASELINE=1`:
//!
//! ```text
//! UPDATE_GENERICITY_BASELINE=1 cargo test -p panproto-integration-tests \
//!     --test genericity no_protocol_names_in_generic_crates -- --nocapture
//! ```
//!
//! Any PR that shrinks the baseline is welcome; any PR that grows it must
//! justify the new violation or exempt the crate in this file.

use std::path::{Path, PathBuf};

/// Generic crates whose source trees must stay protocol-agnostic.
const GENERIC_CRATES: &[&str] = &[
    "panproto-gat",
    "panproto-schema",
    "panproto-inst",
    "panproto-mig",
    "panproto-lens",
    "panproto-lens-dsl",
    "panproto-check",
    "panproto-core",
    "panproto-cli",
    "panproto-py",
    "panproto-wasm",
];

/// Protocol/format/database names that must not appear outside protocol crates.
const PROTOCOL_DENYLIST: &[&str] = &[
    "atproto",
    "lexicon",
    "nsid",
    "bsky",
    "bluesky",
    "strongref",
    "did:plc",
    "at-uri",
    "protobuf",
    "graphql",
    "openapi",
    "json-schema",
    "jsonschema",
    "json_schema",
    "mysql",
    "postgres",
    "sqlite",
    "t-sql",
    "pl/sql",
    "mongodb",
];

/// Programming-language fragments that must not appear in item names.
///
/// Each entry is a case-insensitive substring of an identifier. The scanner
/// triggers whenever an item definition's name contains any of these fragments.
const LANGUAGE_FRAGMENTS: &[&str] = &[
    "rust_",
    "_rust",
    "rust",
    "python_",
    "_python",
    "python",
    "typescript_",
    "_typescript",
    "typescript",
    "javascript_",
    "_javascript",
    "javascript",
    "golang_",
    "_golang",
    "golang",
];

/// Crates whose very purpose is to bridge to a specific language toolchain.
///
/// These are allowed to use language names as identifiers. They are not part
/// of the generic-crate list, but the exception is made explicit here so the
/// intent is obvious at the scan site. `panproto-py` is included because it is
/// the pyo3 bridge to `CPython` and its identifiers (`to_python`, `from_python`)
/// describe a genuine language boundary, analogous to `panproto-parse` and
/// `panproto-llvm`.
const LANGUAGE_ID_EXEMPT_CRATES: &[&str] = &["panproto-parse", "panproto-llvm", "panproto-py"];

/// Locate the workspace root relative to this test crate.
fn workspace_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `<root>/tests/integration` when the test runs.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Some(integration) = manifest.parent() else {
        panic!("CARGO_MANIFEST_DIR has no parent: {}", manifest.display());
    };
    let Some(root) = integration.parent() else {
        panic!("integration dir has no parent: {}", integration.display());
    };
    root.to_path_buf()
}

/// Recursively walk `path`, invoking `visitor(file_path, contents)` for every
/// `.rs` file beneath it. Non-UTF-8 files are skipped silently (they would not
/// be valid Rust source anyway).
pub fn walk_files(path: &Path, visitor: &mut impl FnMut(&Path, String)) {
    if !path.exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk_files(&p, visitor);
        } else if file_type.is_file() && p.extension().is_some_and(|e| e == "rs") {
            if let Ok(contents) = std::fs::read_to_string(&p) {
                visitor(&p, contents);
            }
        }
    }
}

/// Classification of a single source line used by the protocol-denylist scanner.
#[derive(Clone, Copy)]
enum LineView<'a> {
    /// A non-comment code line with comments stripped.
    Code(&'a str),
    /// A doc-comment line (`///` or `//!`). The string is the body after the
    /// doc prefix. Doc lines inside a `# Examples` section are skipped earlier
    /// and do not reach this enum.
    Doc(&'a str),
}

/// Strip `/* ... */` block comments from the source, replacing them with
/// spaces to preserve byte offsets (and therefore line numbers).
fn strip_block_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Walk to the closing `*/`, preserving newlines and replacing the
            // rest with spaces so line numbering is unchanged.
            out.push(b' ');
            out.push(b' ');
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                out.push(if bytes[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            if i + 1 < bytes.len() {
                out.push(b' ');
                out.push(b' ');
                i += 2;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    // Safe: we only replaced ASCII bytes with ASCII bytes, so `out` is still
    // valid UTF-8. If a future change breaks that invariant we surface an
    // empty string rather than unwinding; the scanner will simply report no
    // violations for that file, which is observable in CI.
    String::from_utf8(out).unwrap_or_default()
}

/// Split a line into a [`LineView`] view suitable for denylist scanning.
///
/// The rules:
///
/// - Doc comments (`///` or `//!`) are reported as [`LineView::Doc`] with the
///   leading prefix removed.
/// - Ordinary line comments (`//`) are stripped from the line before returning.
/// - Strings are not parsed; a denylisted term in a string literal will still
///   be flagged. This is intentional: protocol names embedded in string
///   constants are exactly what the rule forbids.
fn classify_line(line: &str) -> LineView<'_> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("///") {
        return LineView::Doc(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("//!") {
        return LineView::Doc(rest);
    }
    // Strip a trailing `//` line comment if present.
    if let Some(idx) = line.find("//") {
        return LineView::Code(&line[..idx]);
    }
    LineView::Code(line)
}

/// Test whether `needle` appears in `haystack` at a word boundary,
/// case-insensitively. `needle` may contain non-word characters
/// (for example `did:plc`, `t-sql`, `pl/sql`, `json-schema`); for those the
/// "word boundary" check is relaxed to "not adjacent to a word character on
/// either side of the needle's outer word-character edges".
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let hay = haystack.to_ascii_lowercase();
    let ndl = needle.to_ascii_lowercase();
    let ndl_bytes = ndl.as_bytes();
    let hay_bytes = hay.as_bytes();
    if ndl_bytes.len() > hay_bytes.len() {
        return false;
    }
    let left_is_word = ndl_bytes
        .first()
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    let right_is_word = ndl_bytes
        .last()
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    let mut i = 0;
    while i + ndl_bytes.len() <= hay_bytes.len() {
        if &hay_bytes[i..i + ndl_bytes.len()] == ndl_bytes {
            let left_ok = if left_is_word {
                i == 0 || !is_word_byte(hay_bytes[i - 1])
            } else {
                true
            };
            let right_idx = i + ndl_bytes.len();
            let right_ok = if right_is_word {
                right_idx >= hay_bytes.len() || !is_word_byte(hay_bytes[right_idx])
            } else {
                true
            };
            if left_ok && right_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

const fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// One reported protocol-name violation.
#[derive(Debug, Clone)]
struct ProtocolViolation {
    file: PathBuf,
    line: usize,
    term: String,
}

/// Scan a single source file for protocol-denylist hits.
fn scan_protocol_names(path: &Path, contents: &str) -> Vec<ProtocolViolation> {
    let stripped = strip_block_comments(contents);
    let mut violations = Vec::new();
    let mut in_examples = false;
    for (idx, line) in stripped.lines().enumerate() {
        let line_no = idx + 1;
        match classify_line(line) {
            LineView::Doc(body) => {
                // Track `# Examples` section boundaries. Any other ATX heading
                // at `#` or `##` level closes the examples section.
                let t = body.trim();
                if let Some(rest) = t.strip_prefix('#') {
                    let rest = rest.trim_start_matches('#').trim();
                    let heading = rest.to_ascii_lowercase();
                    if heading == "examples" || heading == "example" {
                        in_examples = true;
                        continue;
                    } else if !rest.is_empty() {
                        in_examples = false;
                    }
                }
                if in_examples {
                    continue;
                }
                for term in PROTOCOL_DENYLIST {
                    if contains_word(body, term) {
                        violations.push(ProtocolViolation {
                            file: path.to_path_buf(),
                            line: line_no,
                            term: (*term).to_string(),
                        });
                    }
                }
            }
            LineView::Code(code) => {
                for term in PROTOCOL_DENYLIST {
                    if contains_word(code, term) {
                        violations.push(ProtocolViolation {
                            file: path.to_path_buf(),
                            line: line_no,
                            term: (*term).to_string(),
                        });
                    }
                }
            }
        }
    }
    violations
}

/// One reported language-in-identifier violation.
#[derive(Debug, Clone)]
struct LanguageViolation {
    file: PathBuf,
    line: usize,
    item: String,
    fragment: String,
}

/// Scan a single source file for item definitions whose name embeds a
/// programming-language fragment.
fn scan_language_identifiers(path: &Path, contents: &str) -> Vec<LanguageViolation> {
    let stripped = strip_block_comments(contents);
    let mut violations = Vec::new();
    let keywords = [
        "fn", "struct", "enum", "trait", "type", "mod", "const", "static",
    ];
    for (idx, line) in stripped.lines().enumerate() {
        let line_no = idx + 1;
        // Skip doc comments entirely for this scan.
        let trimmed = line.trim_start();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            continue;
        }
        // Also skip attribute macros like `#[foo(bar)]`.
        if trimmed.starts_with('#') && trimmed.contains('[') {
            continue;
        }
        let code = match classify_line(line) {
            LineView::Code(c) => c,
            LineView::Doc(_) => continue,
        };
        for keyword in keywords {
            if let Some(name) = extract_item_name(code, keyword) {
                let lower = name.to_ascii_lowercase();
                for fragment in LANGUAGE_FRAGMENTS {
                    if lower.contains(fragment) {
                        violations.push(LanguageViolation {
                            file: path.to_path_buf(),
                            line: line_no,
                            item: name.to_string(),
                            fragment: (*fragment).to_string(),
                        });
                        break;
                    }
                }
            }
        }
    }
    violations
}

/// Pull the identifier that immediately follows `keyword` on a code line, if
/// the line actually introduces an item. Returns `None` when `keyword` occurs
/// in a context that is not an item head (for example `fn` inside a function
/// body as part of `Fn(...)` would not match because we require whitespace on
/// both sides and a valid identifier immediately after).
fn extract_item_name<'a>(code: &'a str, keyword: &str) -> Option<&'a str> {
    let bytes = code.as_bytes();
    let kw = keyword.as_bytes();
    let mut i = 0;
    while i + kw.len() <= bytes.len() {
        if &bytes[i..i + kw.len()] == kw {
            let left_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let right_idx = i + kw.len();
            let right_ok = right_idx < bytes.len() && bytes[right_idx].is_ascii_whitespace();
            if left_ok && right_ok {
                // Skip whitespace, then take the following identifier.
                let mut j = right_idx + 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && is_word_byte(bytes[j]) {
                    j += 1;
                }
                if j > start {
                    return Some(&code[start..j]);
                }
            }
        }
        i += 1;
    }
    None
}

/// Normalize a file path relative to the workspace root. Line numbers are
/// not included in the baseline key (they shift under refactors); the keyed
/// pair is `(relative_path, term)`.
fn relative_key(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Load the baseline file as a sorted set of lines. Missing file is treated
/// as an empty baseline (useful for first-time initialization).
fn load_baseline(path: &Path) -> std::collections::BTreeSet<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Write `entries` back to the baseline file with a header explaining its
/// role. Invoked only when `UPDATE_GENERICITY_BASELINE=1` is set.
fn write_baseline(path: &Path, entries: &std::collections::BTreeSet<String>, header: &str) {
    let mut body = String::new();
    body.push_str("# Genericity guardrail baseline.\n");
    body.push_str("# ");
    body.push_str(header);
    body.push('\n');
    body.push_str("# Each line is `<relative-path>\\t<term>`. The test fails if the set grows.\n");
    body.push_str("# Regenerate by running with UPDATE_GENERICITY_BASELINE=1.\n");
    for e in entries {
        body.push_str(e);
        body.push('\n');
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(path, body) {
        panic!("failed to write baseline to {}: {e}", path.display());
    }
}

/// Compare the live violation set against the baseline and fail on drift.
fn ratchet<T, F>(
    violations: &[T],
    baseline_path: &Path,
    header: &str,
    mut key: F,
    format_violation: impl Fn(&T) -> String,
) where
    F: FnMut(&T) -> String,
{
    let current: std::collections::BTreeSet<String> = violations.iter().map(&mut key).collect();
    // Dump the current snapshot into `target/` for debugging; this is a side
    // channel, not the source of truth. Using `target/` keeps the tests tree
    // clean (no stray `.current.txt` files to ignore).
    if let Some(file_name) = baseline_path.file_name() {
        let snapshot = std::env::temp_dir().join(format!(
            "panproto_genericity_{}.current.txt",
            file_name.to_string_lossy()
        ));
        write_baseline(&snapshot, &current, header);
    }
    if std::env::var("UPDATE_GENERICITY_BASELINE").is_ok() {
        write_baseline(baseline_path, &current, header);
        eprintln!(
            "[genericity] wrote {} entries to {}",
            current.len(),
            baseline_path.display()
        );
        return;
    }
    let baseline = load_baseline(baseline_path);
    let new_leaks: Vec<&String> = current.difference(&baseline).collect();
    let stale: Vec<&String> = baseline.difference(&current).collect();
    if new_leaks.is_empty() && stale.is_empty() {
        return;
    }
    let mut msg = String::new();
    if !new_leaks.is_empty() {
        msg.push_str("new protocol/language leakage in generic crates:\n");
        for k in &new_leaks {
            msg.push_str("  + ");
            msg.push_str(k);
            msg.push('\n');
            if let Some(v) = violations.iter().find(|v| &&key(v) == k) {
                msg.push_str("      at ");
                msg.push_str(&format_violation(v));
                msg.push('\n');
            }
        }
    }
    if !stale.is_empty() {
        msg.push_str("\nstale baseline entries (no longer present, please update):\n");
        for k in &stale {
            msg.push_str("  - ");
            msg.push_str(k);
            msg.push('\n');
        }
        msg.push_str("\nRun with UPDATE_GENERICITY_BASELINE=1 to refresh.\n");
    }
    panic!("{msg}");
}

/// Path to the protocol-name baseline file.
fn protocol_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("genericity_protocol_baseline.txt")
}

/// Path to the language-identifier baseline file.
fn language_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("genericity_language_baseline.txt")
}

/// Test entry point: scan every generic crate's source tree for protocol
/// names and fail when the violation set grows past the baseline.
#[test]
fn no_protocol_names_in_generic_crates() {
    let root = workspace_root();
    let mut violations = Vec::<ProtocolViolation>::new();
    let mut scanned_files: usize = 0;
    for crate_name in GENERIC_CRATES {
        let src = root.join("crates").join(crate_name).join("src");
        walk_files(&src, &mut |p, contents| {
            scanned_files += 1;
            violations.extend(scan_protocol_names(p, &contents));
        });
    }
    assert!(
        scanned_files > 0,
        "genericity test scanned zero files; workspace layout changed? (root = {})",
        root.display()
    );
    ratchet(
        &violations,
        &protocol_baseline_path(),
        "Protocol/format/database name leakage, pinned to current tree.",
        move |v| format!("{}\t{}", relative_key(&root, &v.file), v.term),
        |v| format!("{}:{}", v.file.display(), v.line),
    );
}

/// Test entry point: scan every generic crate (minus language-bridge
/// exceptions) for item definitions whose name embeds a programming-language
/// fragment, and ratchet against the baseline.
#[test]
fn no_programming_language_names_as_identifiers() {
    let root = workspace_root();
    let mut violations = Vec::<LanguageViolation>::new();
    let mut scanned_files: usize = 0;
    for crate_name in GENERIC_CRATES {
        if LANGUAGE_ID_EXEMPT_CRATES.contains(crate_name) {
            continue;
        }
        let src = root.join("crates").join(crate_name).join("src");
        walk_files(&src, &mut |p, contents| {
            scanned_files += 1;
            violations.extend(scan_language_identifiers(p, &contents));
        });
    }
    assert!(
        scanned_files > 0,
        "language-identifier test scanned zero files; workspace layout changed? (root = {})",
        root.display()
    );
    ratchet(
        &violations,
        &language_baseline_path(),
        "Programming-language identifier leakage, pinned to current tree.",
        move |v| {
            format!(
                "{}\t{}\t{}",
                relative_key(&root, &v.file),
                v.item,
                v.fragment
            )
        },
        |v| {
            format!(
                "{}:{}: `{}` contains `{}`",
                v.file.display(),
                v.line,
                v.item,
                v.fragment
            )
        },
    );
}

/// Validation harness: run the scanners against the in-tree synthetic
/// fixtures and verify the clean file is clean and the dirty file trips both
/// rules.
#[test]
fn scanners_detect_synthetic_violations() {
    let good = include_str!("genericity_fixtures/good.rs");
    let bad = include_str!("genericity_fixtures/bad.rs");
    let good_path = PathBuf::from("genericity_fixtures/good.rs");
    let bad_path = PathBuf::from("genericity_fixtures/bad.rs");

    let good_protocol = scan_protocol_names(&good_path, good);
    let good_language = scan_language_identifiers(&good_path, good);
    assert!(
        good_protocol.is_empty(),
        "good.rs unexpectedly flagged for protocol names: {good_protocol:?}"
    );
    assert!(
        good_language.is_empty(),
        "good.rs unexpectedly flagged for language identifiers: {good_language:?}"
    );

    let bad_protocol = scan_protocol_names(&bad_path, bad);
    let bad_language = scan_language_identifiers(&bad_path, bad);
    assert!(
        bad_protocol.iter().any(|v| v.term == "bsky"),
        "bad.rs should have tripped `bsky` protocol rule, got {bad_protocol:?}"
    );
    assert!(
        bad_language
            .iter()
            .any(|v| v.item == "RustBridge" && v.fragment.contains("rust")),
        "bad.rs should have tripped `RustBridge` language rule, got {bad_language:?}"
    );
}
