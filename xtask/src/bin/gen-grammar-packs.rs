// xtask binaries are repository-task scripts; the workspace's
// pedantic / nursery / unwrap-used / expect-used denials are
// over-zealous for a script that prints an error and exits on every
// failure path anyway.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::uninlined_format_args,
    clippy::manual_assert,
    clippy::missing_const_for_fn
)]

//! Generate the grammar companion manifests from `grammar-packs.toml`.
//!
//! Every `crates/panproto-grammars-<pack>/Cargo.toml` is a wrapper crate
//! that bundles one `panproto-grammars` `group-<pack>` feature into a pyo3
//! cdylib. The manifests differ only in the pack name, description, cdylib
//! name, and selected group, so they are derived from a single source of
//! truth (`grammar-packs.toml`) rather than hand-maintained ten times over.
//!
//! Run from the repo root:
//!
//! ```sh
//! cargo run -p xtask --bin gen-grammar-packs           # write the manifests
//! cargo run -p xtask --bin gen-grammar-packs -- --check # verify, do not write
//! ```
//!
//! CI runs the `--check` form and fails if any checked-in manifest drifts
//! from the generator output.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Deserialize;

/// One companion package: the table key is the pack name, and the body
/// carries the human-readable description fragment.
#[derive(Deserialize)]
struct PackSpec {
    /// Description fragment, wrapped into the crate's package description.
    description: String,
}

fn main() -> ExitCode {
    let check = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        args => {
            eprintln!("usage: gen-grammar-packs [--check]");
            eprintln!("unexpected arguments: {:?}", args);
            return ExitCode::FAILURE;
        }
    };

    let version = workspace_version();
    let packs = load_packs();

    let mut drifted = Vec::new();
    for (pack, spec) in &packs {
        let rendered = render_manifest(pack, &spec.description, &version);
        let path = repo_relative(&format!("crates/panproto-grammars-{pack}/Cargo.toml"));

        if check {
            let current = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
            if current != rendered {
                drifted.push(pack.clone());
            }
        } else {
            std::fs::write(&path, rendered.as_bytes())
                .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
            println!("wrote {}", path.display());
        }
    }

    if check {
        if drifted.is_empty() {
            println!("{} grammar-pack manifests up to date", packs.len());
        } else {
            eprintln!("grammar-pack manifests out of date: {}", drifted.join(", "));
            eprintln!("run `cargo run -p xtask --bin gen-grammar-packs` to regenerate");
            return ExitCode::FAILURE;
        }
    }

    ExitCode::SUCCESS
}

/// Render one companion manifest. The output is byte-for-byte stable so
/// `--check` can compare it against the checked-in file.
fn render_manifest(pack: &str, description: &str, version: &str) -> String {
    let cdylib = format!("panproto_grammars_{}_impl", pack.replace('-', "_"));
    format!(
        "[package]
name = \"panproto-grammars-{pack}\"
description = \"Companion grammar package: {description} for the panproto Python wheel\"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[lib]
# Globally-unique cdylib name to avoid `PyInit_<name>` symbol
# collisions across companions in the same Python process.
name = \"{cdylib}\"
crate-type = [\"cdylib\"]

[dependencies]
# `default-features = false` lets this companion ship only the
# `group-{pack}` grammars, not the workspace-default `group-core`.
# Workspace inheritance is bypassed because the workspace dep
# doesn't pin default-features.
panproto-grammars = {{ version = \"={version}\", path = \"../panproto-grammars\", default-features = false, features = [\"group-{pack}\"] }}
pyo3 = {{ workspace = true }}
tree-sitter = {{ workspace = true }}

[lints]
workspace = true
"
    )
}

/// Read the pack table from `grammar-packs.toml`.
fn load_packs() -> BTreeMap<String, PackSpec> {
    let path = repo_relative("grammar-packs.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

/// Extract `[workspace.package] version` from the root `Cargo.toml`.
fn workspace_version() -> String {
    let path = repo_relative("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    let manifest: toml::Value =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()));
    manifest
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .expect("[workspace.package] version missing from root Cargo.toml")
        .to_string()
}

/// Resolve a path relative to the repository root (the xtask crate sits
/// one level below it).
fn repo_relative(rel: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push(rel);
    p
}
