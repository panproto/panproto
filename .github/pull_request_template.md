<!--
Thanks for contributing to panproto. Fill in the sections below; remove
any that don't apply. The reviewer's checklist is at the bottom.
-->

## Summary

<!-- One paragraph: what does this PR change, and why now? -->

## Changes

<!-- Bullet list of the main edits, grouped by area. Link to the
     specific files / line ranges where useful. -->

-

## Type of change

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change (API, schema, or migration semantics)
- [ ] Build / CI / release infrastructure
- [ ] Documentation
- [ ] Refactor / internal cleanup
- [ ] Performance
- [ ] Security

## Affected surfaces

<!-- Tick every layer this PR touches. The release skill assumes a
     change to any of these except "Internal" requires version-bump
     coordination. -->

- [ ] Rust crates (`crates/`) — list which:
- [ ] WASM boundary (`crates/panproto-wasm`)
- [ ] TypeScript SDK (`sdk/typescript`, `@panproto/core`)
- [ ] Python SDK (`sdk/python`, `crates/panproto-py`)
- [ ] CLI (`crates/panproto-cli`)
- [ ] Book (`book/src/`)
- [ ] CI workflows (`.github/workflows/`)
- [ ] Internal only (no published-surface change)

## Linked issues

<!-- "Closes #N" auto-closes the issue on merge. Use "Refs #N" for
     issues this advances but doesn't fully resolve. -->

- Closes #
- Refs #

## Breaking changes

<!-- If "Breaking change" is checked above, fill this in; otherwise
     delete this section. Pre-1.0 we make breaking changes freely,
     but every breaking change needs an entry in CHANGELOG.md and a
     migration note here. -->

**What breaks:**

**Migration path for downstream users:**

## Test plan

<!-- How did you verify this works? Be concrete: commands run, suites
     that passed, manual UI walks, etc. CI must be green; this is the
     human-readable companion. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo nextest run --workspace`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] `wasm-pack build crates/panproto-wasm --target web --dev` (if WASM affected)
- [ ] `cd sdk/typescript && pnpm install && pnpm test && pnpm exec tsc --noEmit` (if TS SDK affected)
- [ ] `cd sdk/python && uv run pytest tests/ -x` (if Python SDK affected)
- [ ] Manual: <!-- describe any manual verification, e.g. "ran panproto schema parse on a 5kLOC Rust crate" -->

## Documentation

- [ ] Updated relevant `README.md` files
- [ ] Updated `book/src/` chapters if user-facing concepts changed
- [ ] Updated CHANGELOG.md under `## [Unreleased]`
- [ ] Updated docstrings / rustdoc on changed public items
- [ ] Documentation not required (pure refactor or internal only)

## Release coordination

<!-- Most PRs should not bump versions; the /release skill handles
     that as a separate step. Tick only if this PR itself is a
     release-shape change. -->

- [ ] This PR bumps versions (`Cargo.toml`, `sdk/*/package.json`, etc.)
- [ ] This PR adds, modifies, or removes a published-surface API and a CHANGELOG entry was added
- [ ] CI workflow change requires a one-time setup step on a third-party service (npm, crates.io, PyPI) — describe below

<!-- One-time setup notes (if any): -->

## Reviewer checklist

- [ ] Code compiles cleanly with `clippy -D warnings`
- [ ] Tests cover the change and existing tests still pass
- [ ] Public API changes are documented and CHANGELOG-noted
- [ ] No secrets, tokens, or PII in the diff
- [ ] No `unwrap` / `expect` / `panic!` introduced on the WASM or SDK boundary
- [ ] No `--no-verify`, `--allow-dirty`, or `unsafe` without explicit justification in this PR description
- [ ] Diff size is reviewable; if not, this PR is split into smaller pieces
