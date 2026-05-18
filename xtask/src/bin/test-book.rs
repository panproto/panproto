// xtask binaries are repository-task scripts; the workspace's
// pedantic / nursery / unwrap-used / expect-used denials are
// over-zealous for a script that prints an error and exits on every
// failure path anyway.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::uninlined_format_args,
    clippy::manual_assert,
    clippy::missing_const_for_fn,
)]

//! Test Rust code blocks in `book/src/**/*.md` via `rustdoc --test`.
//!
//! mdbook's own `mdbook test --library-path <dir>` cannot handle the
//! workspace's deps directory (multiple candidates for each crate
//! name confuse rustdoc's resolver). This driver does the legwork
//! mdbook does not: build a stub crate that pulls in the panproto
//! crates the book examples reference, parse cargo's
//! `--message-format=json` output to find the specific rmeta path
//! for each package, then hand each Rust block to `rustdoc --test`
//! with explicit `--extern <name>=<path>` flags.
//!
//! Run from the repo root:
//!
//! ```sh
//! cargo run -p xtask --bin test-book
//! ```
//!
//! CI invokes the same command after `cargo build`.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

/// Crates the book's Rust examples are allowed to reference.
///
/// Anything not in this list will not be available; if a book example
/// references a crate not here, add it (and ensure the stub crate's
/// `Cargo.toml` lists it as a dependency).
const ALLOWED_EXTERN_CRATES: &[&str] = &[
    "panproto_core",
    "panproto_gat",
    "panproto_inst",
    "panproto_lens",
    "panproto_schema",
    "panproto_parse",
    "panproto_protocols",
    "panproto_mig",
    "panproto_io",
    "miette",
    "anyhow",
    "serde_json",
    "smallvec",
];

fn main() {
    let repo_root = repo_root();
    let book_src = repo_root.join("book").join("src");
    let stub = repo_root.join("crates").join("book-doctest-stub");
    if !stub.exists() {
        eprintln!(
            "test-book: stub crate not found at {} \u{2014} create it before running.",
            stub.display()
        );
        std::process::exit(2);
    }

    // 1. Build the stub crate; capture compiler-artifact messages so
    //    we know which rmeta belongs to which package.
    eprintln!("test-book: building book-doctest-stub");
    let mut child = Command::new("cargo")
        .current_dir(&repo_root)
        .args([
            "build",
            "-p",
            "book-doctest-stub",
            "--message-format=json",
            "--quiet",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn cargo build");
    let mut json_out = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut json_out)
        .expect("read cargo json output");
    let status = child.wait().expect("wait cargo");
    if !status.success() {
        eprintln!("test-book: cargo build -p book-doctest-stub failed");
        std::process::exit(1);
    }

    let externs = collect_externs(&json_out);
    // Verify every allowed crate produced an artifact. Missing ones
    // would silently turn into 'cannot find crate' rustdoc errors
    // later, with much worse diagnostics.
    let missing: Vec<&&str> = ALLOWED_EXTERN_CRATES
        .iter()
        .filter(|c| !externs.contains_key(**c))
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "test-book: no compiler artifact found for: {:?}. \
             Add the missing crates as direct dependencies of book-doctest-stub.",
            missing,
        );
        std::process::exit(1);
    }

    // 2. Walk book/src for Rust blocks and run rustdoc --test on each.
    let mut total = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut ignored = 0usize;
    let blocks = collect_blocks(&book_src);
    for block in &blocks {
        if block.ignored {
            ignored += 1;
            continue;
        }
        total += 1;
        match run_block(&repo_root, block, &externs) {
            Ok(()) => {
                println!("ok    {}", block.label());
            }
            Err(detail) => {
                println!("FAIL  {}", block.label());
                failed.push(format!("{}\n{}\n", block.label(), detail));
            }
        }
    }

    println!();
    println!(
        "test-book: {total} block(s) tested, {} failed, {ignored} ignored",
        failed.len(),
    );
    if !failed.is_empty() {
        for f in &failed {
            println!("---");
            println!("{f}");
        }
        std::process::exit(1);
    }
}

/// Locate the repo root from the current working directory by walking
/// up until a directory containing `Cargo.toml` and `book/` is found.
fn repo_root() -> PathBuf {
    let mut p = std::env::current_dir().expect("cwd");
    loop {
        if p.join("Cargo.toml").is_file() && p.join("book").is_dir() {
            return p;
        }
        if !p.pop() {
            panic!("could not locate repo root from cwd");
        }
    }
}

/// One Rust block in the book.
struct Block {
    file: PathBuf,
    /// 1-based line where the opening fence sits.
    line: usize,
    /// `ignore`, `no_run`, `should_panic`, `edition2024`, etc.
    attrs: Vec<String>,
    /// Whether the block should be skipped entirely.
    ignored: bool,
    /// The block body, *with* hidden `# ` lines preserved (rustdoc
    /// strips them at render time).
    body: String,
}

impl Block {
    fn label(&self) -> String {
        format!("{}:{}", self.file.display(), self.line)
    }
    fn should_compile(&self) -> bool {
        // rustdoc --test runs all blocks that aren't marked
        // `ignore`. `no_run` compiles but doesn't execute, which is
        // the behaviour we want for examples that touch the
        // filesystem or invent variables at the top level.
        !self.ignored
    }
}

fn collect_blocks(book_src: &Path) -> Vec<Block> {
    let mut blocks = Vec::new();
    walk_md(book_src, &mut |path| {
        let text = fs::read_to_string(path).expect("read md");
        let mut lines = text.lines().enumerate();
        while let Some((i, line)) = lines.next() {
            let Some(rest) = line.strip_prefix("```") else {
                continue;
            };
            // Only Rust blocks: the fence info-string starts with
            // `rust` and is followed by either nothing, whitespace,
            // or a comma. `rust,ignore` and `rust,no_run` count;
            // `rusty` does not.
            let is_rust = rest == "rust" || rest.starts_with("rust,") || rest.starts_with("rust ");
            if !is_rust {
                continue;
            }
            let attrs: Vec<String> = rest
                .strip_prefix("rust")
                .map(|s| {
                    s.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let ignored = attrs.iter().any(|a| a == "ignore");

            let mut body = String::new();
            for (_, body_line) in lines.by_ref() {
                if body_line == "```" {
                    break;
                }
                body.push_str(body_line);
                body.push('\n');
            }
            blocks.push(Block {
                file: path.to_path_buf(),
                line: i + 1,
                attrs,
                ignored,
                body,
            });
        }
    });
    blocks
}

fn walk_md<F: FnMut(&Path)>(dir: &Path, f: &mut F) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            walk_md(&path, f);
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            f(&path);
        }
    }
}

/// Parsed `cargo build --message-format=json` output: crate name ->
/// rmeta path, picking the artifact most appropriate for downstream
/// linking (lib crate-type, most recent if duplicates).
fn collect_externs(json: &str) -> BTreeMap<String, PathBuf> {
    #[derive(Deserialize)]
    struct Artifact {
        reason: String,
        #[serde(default)]
        target: Target,
        #[serde(default)]
        filenames: Vec<PathBuf>,
    }
    #[derive(Deserialize, Default)]
    struct Target {
        #[serde(default)]
        name: String,
        #[serde(default)]
        crate_types: Vec<String>,
    }

    let mut out: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut seen_for_name: HashSet<String> = HashSet::new();
    for line in json.lines() {
        let Ok(a) = serde_json::from_str::<Artifact>(line) else {
            continue;
        };
        if a.reason != "compiler-artifact" {
            continue;
        }
        // We only want library artifacts (not bin, proc-macro for
        // build scripts, etc.). The lib crate-type covers normal
        // rlibs; proc-macro is needed for serde-derive-like macros
        // but those go via separate `--extern` mechanisms and are
        // not referenced by name in book examples.
        if !a.target.crate_types.iter().any(|t| t == "lib") {
            continue;
        }
        let rmeta = a
            .filenames
            .iter()
            .find(|p| p.extension().and_then(|e| e.to_str()) == Some("rmeta"));
        let Some(rmeta) = rmeta else {
            continue;
        };
        // For any crate name we already recorded, prefer the
        // *first* artifact: cargo emits artifacts in
        // dependency-bottom-up order, so the first artifact for a
        // given crate name is the one consumers higher in the
        // dependency tree were linked against. Picking a later
        // (different-feature-set) duplicate produces a rustc
        // "multiple different versions of crate X" mismatch when the
        // doctest later calls into a consumer that expects the first
        // build.
        let name = a.target.name.replace('-', "_");
        if seen_for_name.insert(name.clone()) {
            out.insert(name, rmeta.clone());
        }
    }
    out
}

/// Invoke `rustdoc --test` on one Rust block.
///
/// Writes the block to a temp `.rs` file, wraps it in a minimal
/// `fn main()` if it does not already have one (mimicking rustdoc's
/// markdown wrapping behaviour), and runs rustdoc with `--extern`
/// flags for every allowed dep.
fn run_block(
    repo_root: &Path,
    block: &Block,
    externs: &BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if !block.should_compile() {
        return Ok(());
    }

    // Write a markdown file containing only this Rust block; rustdoc
    // accepts markdown and handles hidden-line (`# `) semantics.
    let tmp = repo_root
        .join("target")
        .join("book-doctest-tmp")
        .join(format!(
            "{}_L{}.md",
            block
                .file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("block"),
            block.line,
        ));
    if let Some(parent) = tmp.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let attr_string = if block.attrs.is_empty() {
        String::new()
    } else {
        format!(",{}", block.attrs.join(","))
    };
    let md = format!("```rust{attr_string}\n{}```\n", block.body);
    fs::write(&tmp, &md).map_err(|e| format!("write tmp md: {e}"))?;

    let mut cmd = Command::new("rustdoc");
    cmd.arg("--test").arg("--edition=2024");
    for (name, path) in externs {
        if !ALLOWED_EXTERN_CRATES.contains(&name.as_str()) {
            continue;
        }
        let mut spec: OsString = name.into();
        spec.push("=");
        spec.push(path);
        cmd.arg("--extern").arg(spec);
    }
    // The deps directory is on the library search path so transitive
    // deps resolve. Multi-candidate ambiguity is avoided because we
    // pass --extern for every name our examples actually use.
    cmd.arg("-L");
    cmd.arg(repo_root.join("target").join("debug").join("deps"));
    cmd.arg(&tmp);

    let output = cmd.output().map_err(|e| format!("spawn rustdoc: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        return Err(format!("{stderr}\n{stdout}"));
    }
    Ok(())
}
