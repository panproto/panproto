# Contributing to panproto

Contributions are welcome: bug reports, fixes, documentation, new protocols and grammars. This page covers the
mechanics; the pull request template lists the checks a change is expected to pass.

## Before you start

- For anything larger than a small fix, open an issue first so the design can be discussed before code is written.
- The planned releases and their themes are in [ROADMAP.md](ROADMAP.md); each has a GitHub milestone.
- By contributing you agree that your contribution is licensed under the [MIT license](LICENSE).
- Material from other projects (grammars, conformance corpora) is listed with its licence in [THIRD-PARTY.md](THIRD-PARTY.md); add a row there when you vendor anything, and vendor only permissively licensed material.
- Participation is governed by the [code of conduct](CODE_OF_CONDUCT.md).

## Building and testing

The workspace needs a stable Rust toolchain at or above the version in `Cargo.toml` (`rust-version`) and
[cargo-nextest](https://nexte.st). The checks CI runs on every pull request are:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Some exhaustive tests take minutes to hours and are excluded from the default nextest run; `.config/nextest.toml`
lists them. Each binding has its own test command, listed in the
[pull request template](.github/pull_request_template.md), and Rust code blocks in the book are compiled as described
in [book/CONTRIBUTING.md](book/CONTRIBUTING.md).

## Conventions

- Commit subjects follow [Conventional Commits](https://www.conventionalcommits.org) (`feat(lens): ...`,
  `fix(io): ...`). Pull requests are squash-merged, so the pull request title becomes the commit subject on `main`.
- Warnings are fixed, not silenced with `#[allow]`.
- Comments and documentation describe the code as it is, not its history.
- A change to a published API adds an entry to `CHANGELOG.md`. Before 1.0, breaking changes are acceptable when the
  entry says what breaks and how to migrate.
- Versions are bumped only at release time, with `.github/scripts/bump_version.py`.

## Mutation testing

Pull requests run [cargo-mutants](https://mutants.rs) on the lines they change, and a scheduled job runs it over the
core crates on `main`. A surviving mutant points at behaviour no test distinguishes; the usual response is a test that
does. Neither job blocks a merge or a release.

## Reporting security problems

See [SECURITY.md](SECURITY.md). Do not file public issues for vulnerabilities.
