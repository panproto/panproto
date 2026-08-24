# Install the CLI

The CLI is a single binary called `schema`. It is the entry point for the [`panproto-cli`](https://crates.io/crates/panproto-cli) crate.

## Prerequisites

A POSIX shell on macOS or Linux, or PowerShell on Windows. The binary releases cover the targets listed in the repository's cargo-dist configuration. Other targets require a Rust toolchain.

## Install

### Homebrew (macOS, Linux)

```sh
brew install panproto/tap/schema
```

### Shell installer (macOS, Linux, WSL)

```sh
curl --proto '=https' -LsSf https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.sh | sh
```

### PowerShell installer (Windows)

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/panproto/panproto/releases/latest/download/panproto-cli-installer.ps1 | iex"
```

### From source

```sh
cargo install panproto-cli
```

Requires a Rust toolchain (1.85 or newer).

## Verification

```sh
schema --version
```

prints the installed version. The full subcommand list is at [Reference: CLI](../../reference/cli.md), or `schema --help`.

## Common mistakes

- Installing through `cargo install` without an up-to-date toolchain. panproto requires Rust 1.85 or later.
- Mixing the Homebrew install with a from-source install on the same machine: only one `schema` ends up first on `PATH`.

## See also

- [Tutorial: your first schema](../../tutorials/your-first-schema.md).
- [Reference: CLI](../../reference/cli.md).
