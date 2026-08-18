//! Generate `book/src/reference/cli.md` from the live `schema` binary.
//!
//! Runs `cargo run -q -p panproto-cli -- <path...> --help` recursively,
//! capturing the output verbatim and grouping it into Markdown sections.
//! Output is deterministic: the same CLI surface produces the same file.
//!
//! The `HEADER` constant is the page's hand-written prologue. Everything
//! after it is generated, so prose that a `--help` block cannot carry belongs
//! in that constant and nowhere else: the CI gate compares the regenerated
//! file against the one checked in, so prose added to `cli.md` directly is
//! both overwritten here and reported as drift there.
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

## Discovering a migration

`schema auto-migrate` runs one search over spans. Write the apex as $A$; the two
legs have the shape

$$
\\mathit{old} \\xleftarrow{\\ell} A \\xrightarrow{r} \\mathit{new}.
$$

The apex is the sub-schema of `old` whose vertices found a target in `new`. That
search never refuses for want of a match: leaving every source vertex out of the
apex is always feasible, so two schemas with nothing in common come back with an
empty apex rather than with an error. Two of the three flags below therefore
select which of its answers counts as an answer, and the search underneath is the
same one either way; the third constrains the search itself.

`--span` accepts every answer, the empty apex included. Without it the command
accepts a span covering at least one source vertex and reports the empty apex as
a failure naming the two files.

`--total` accepts only a span whose left leg is onto, which is to say a total
morphism. Totality is a condition on the edges as well as the vertices, so an
apex holding every source vertex is not on its own enough: a source arc that
found no image in the target leaves the answer partial at full vertex coverage.
The command does not read that off the span alone. An optimal span that
drops a vertex is no evidence about whether a total morphism exists, because span
quality excludes the drop count while the objective is lexicographic in quality
first and drops second, so a span that drops a vertex can score strictly better
than a total morphism that keeps it. When the optimal span is not total, the
command runs the total-morphism search before giving up, and it fails only when
that second search comes back empty, quoting the coverage the span did reach.
`--total` and `--span` conflict, and the pair is rejected before either search
runs.

`--monic` constrains the search rather than the acceptance, so it composes with
either of the others. It requires the vertex map to be injective, so that no two
source vertices land on the same target. Injectivity on vertices does not force
injectivity on edges, and a monic answer may still send two parallel source edges
to one target edge. When the answer is not injective on vertices, the command
says so on stderr, since a migration identifying two source vertices has no
well-defined lift without a rule for combining them.

The human report opens with the shape, the score, and the interval the search
certified, then sizes the apex:

```text
Found span (quality: 0.812, bounds: [0.812, 0.884]):

Apex: 7 of 9 vertices (77.8% coverage), 6 edges
```

The bounds collapse to a point exactly when the answer was proved optimal. When
they do not, a following line records that the search stopped before it could
rule out a better span, which is what separates a quality of 0.812 that nothing
beats from a quality of 0.812 the search never got to improve on. Underneath
come the right leg's vertex map and, when it has one, its edge map. A total
morphism recovered by the second search prints a shorter report, with no apex
line and no bounds, because it carries neither.

`--json` writes the span's right leg, a migration out of the apex, to stdout.
Warnings stay on stderr, so the output pipes.
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
