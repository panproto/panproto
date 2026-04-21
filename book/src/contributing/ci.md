# CI, semver-checks, and release

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Every commit to panproto's main branch runs through a CI pipeline whose job is to catch regressions before they land — across formatting, lints, tests, documentation, semver compatibility, and the workspace-hack invariant. The present chapter walks through each check, says why it exists, and gives the local command a contributor can run to reproduce it before pushing.

The pipeline definition lives in `.github/workflows/ci.yml`.

## The checks

Formatting runs first through [`cargo fmt --all --check`](https://doc.rust-lang.org/cargo/commands/cargo-fmt.html). The workspace uses the default `rustfmt.toml` configuration, with no overrides. A commit that fails `fmt --check` does not advance to the later stages; the local equivalent is `cargo fmt --all`.

Linting comes next, through [`cargo clippy --workspace --all-targets -- -D warnings`](https://doc.rust-lang.org/cargo/commands/cargo-clippy.html). Every warning is a CI failure, and contributors do not silence warnings with `#[allow]` attributes unless the silencing is justified with a comment. The clippy configuration includes `clippy::pedantic` and `clippy::nursery`, which catch patterns that the default clippy level lets through. The local equivalent is `cargo clippy --workspace --all-targets`.

The test suite runs through [`cargo nextest run --workspace`](https://nexte.st/). Nextest replaces the default `cargo test` for speed: it runs tests in parallel more effectively and produces structured output. The test suite includes property-based tests (through [`proptest`](https://docs.rs/proptest/latest/proptest/)) for the mathematical invariants panproto promises, including lens-law checking and functor-axiom verification.

Documentation is checked with [`cargo doc --workspace --no-deps --all-features -- -D warnings`](https://doc.rust-lang.org/cargo/commands/cargo-doc.html), which verifies that rustdoc can render the whole workspace without warnings. Broken doc-links and missing-documentation lints on public items are CI failures.

Semver compatibility is checked with [`cargo semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks), which runs against the last published release on crates.io and rejects any breaking API change that is not accompanied by a semver-major version bump. The check catches removed public items, changed function signatures, changed type exports, and added required trait methods. The local equivalent is `cargo semver-checks`, after installing the binary through `cargo install cargo-semver-checks`.

The workspace-hack invariant is verified by [`cargo hakari verify`](https://docs.rs/cargo-hakari/latest/cargo_hakari/), which confirms that the workspace's generated `workspace-hack` crate is up to date. Hakari speeds up builds by unifying dependency features across the workspace; the generated crate has to be regenerated whenever workspace dependencies change, and CI rejects commits that leave it stale. The local equivalent is `cargo hakari generate && cargo hakari manage-deps`.

The pipeline runs against two toolchain versions: the MSRV (minimum supported Rust version, currently stable N-2) and the current stable. A commit that passes both is merged.

## Release

A release follows the workflow documented in the `release` skill. The steps are version bump (through [`release-please`](https://github.com/googleapis/release-please) or the equivalent), changelog update (driven by [`git-cliff`](https://git-cliff.org/) against conventional commits), test-suite dry-run across all SDK builds, publication to [crates.io](https://crates.io/) for Rust crates, to [npm](https://www.npmjs.com/package/@panproto/core) for the TypeScript SDK, and to [PyPI](https://pypi.org/project/panproto/) for the Python wheel. Each step is scripted; a human is in the loop for the final version-bump commit and the git tag.

The SDK wheels for Python are built across platforms through a matrix job in [`.github/workflows/python-wheels.yml`](https://github.com/aaronstevenwhite/panproto/tree/main/.github/workflows/python-wheels.yml). The matrix covers Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64). A source distribution is published alongside the binary wheels for platforms the matrix does not cover.

## Pre-commit hooks

Contributors who want local feedback before pushing install the pre-commit hooks through the `panproto-pre-commit-hooks` skill. The hooks run a subset of CI locally on every commit: formatting, clippy on the changed files, and a fast test subset. A full CI reproduction is still a good idea before pushing to a long-lived branch, but the hooks catch the common cases (a missed `rustfmt`, a new clippy warning, a regressed test) without waiting for CI.

## Reproducing CI locally

The `lint` and `test` skills each run a bundled subset of the CI pipeline. `lint` covers formatting, clippy, and doc warnings; `test` covers the test suites. A contributor who runs both skills sequentially reproduces CI's correctness checks; the semver-checks and hakari checks are run separately through the `semver` and `hakari` skills.

The sequence that reproduces every CI step is documented in the `contributing` skill and is worth running end-to-end before a first PR, so that a contributor sees the same pass and fail output the CI pipeline produces.

## Closing

The next chapter, [Extending panproto](./extending.md), walks through the three most common contributions: a new protocol, a new lens combinator, and a new expression-language builtin. Each has a specific shape that the crate structure documented here encourages.
