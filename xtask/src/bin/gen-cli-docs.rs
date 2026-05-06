//! Generate `book/src/reference/cli.md` from the live `schema` binary.
//!
//! Runs `cargo run -q -p panproto-cli -- <path...> --help` recursively,
//! capturing the output verbatim and grouping it into Markdown sections.
//! Output is deterministic: the same CLI surface produces the same file.
//!
//! Run from the repo root: `cargo run -p xtask --bin gen-cli-docs`.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const HEADER: &str = "\
# CLI reference

Every `schema` subcommand, with its full `--help` text. This page is regenerated
from the live binary by `xtask/src/bin/gen-cli-docs.rs`; edit the CLI, not the
page.

To regenerate locally:

```sh
cargo run -p xtask --bin gen-cli-docs
```

CI runs the same command and fails if the result differs from what is checked in.

For the model that the commands operate on, see
[Schemas as theories](../explanation/schemas-as-theories.md),
[Migrations as morphisms](../explanation/migrations-as-morphisms.md), and
[Schema version control semantics](../explanation/vcs-semantics.md).
";

/// Maximum recursion depth into nested subcommand groups.
const MAX_DEPTH: usize = 4;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = repo_relative("book/src/reference/cli.md");

    let mut buf = String::from(HEADER);
    buf.push('\n');

    let mut visited = BTreeSet::new();
    walk(&[], &mut buf, &mut visited, 0)?;

    std::fs::write(&out_path, buf.as_bytes())?;
    println!("wrote {}", out_path.display());
    Ok(())
}

fn walk(
    path: &[String],
    buf: &mut String,
    visited: &mut BTreeSet<Vec<String>>,
    depth: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    if !visited.insert(path.to_vec()) {
        return Ok(());
    }

    let help = run_help(path)?;
    let heading = if path.is_empty() {
        "schema".to_owned()
    } else {
        format!("schema {}", path.join(" "))
    };
    let level = "#".repeat((depth + 2).min(6));
    writeln!(buf, "{level} `{heading}`")?;
    buf.push('\n');
    buf.push_str("```text\n");
    buf.push_str(help.trim_end());
    buf.push_str("\n```\n\n");

    for sub in subcommands(&help) {
        let mut next = path.to_vec();
        next.push(sub);
        walk(&next, buf, visited, depth + 1)?;
    }
    Ok(())
}

fn run_help(path: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-q", "-p", "panproto-cli", "--"]);
    for segment in path {
        cmd.arg(segment);
    }
    cmd.arg("--help");
    cmd.stderr(Stdio::piped()).stdout(Stdio::piped());
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(format!(
            "schema {} --help failed (status {}): {}",
            path.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

/// Extract the subcommand names from a clap-rendered `--help` block.
///
/// Looks for the `Commands:` (or `Subcommands:`) section header and reads
/// the leading word of each indented line until the first blank line or
/// next section. Aliases (`help`, names beginning with non-letters) are
/// skipped.
fn subcommands(help: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in help.lines() {
        let trimmed_left = line.trim_end();
        if !in_section {
            if trimmed_left.starts_with("Commands:") || trimmed_left.starts_with("Subcommands:") {
                in_section = true;
            }
            continue;
        }
        if trimmed_left.is_empty() {
            break;
        }
        if !line.starts_with("  ") {
            break;
        }
        let token = line.trim_start();
        let name = token.split_whitespace().next().unwrap_or("");
        if name.is_empty() || name == "help" {
            continue;
        }
        if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
            continue;
        }
        out.push(name.to_owned());
    }
    out
}

fn repo_relative(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push(rel);
    p
}
