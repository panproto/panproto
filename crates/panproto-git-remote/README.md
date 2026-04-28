# panproto-git-remote

[![crates.io](https://img.shields.io/crates/v/panproto-git-remote.svg)](https://crates.io/crates/panproto-git-remote)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Git remote helper for `panproto://` URLs; ships the `git-remote-panproto` binary.

## What it does

When git encounters a remote URL that starts with `panproto://`, it looks for a binary called `git-remote-panproto` on your PATH and hands control to it. This binary speaks the git remote-helper protocol on stdin and stdout, translating git's push and fetch commands into calls against a panproto node over XRPC.

On push, the helper imports the git commits being pushed into panproto objects (using `panproto-git`), then sends those objects to the node server using `panproto-xrpc`. On fetch, it pulls panproto objects from the node and exports them back to git tree and commit objects so git can merge them into the local repository. The translation preserves author names, emails, timestamps, commit messages, and parent links in both directions.

A persistent cache lives under `$GIT_DIR/panproto-cache/<remote>/` so that subsequent pushes and fetches only process commits that are new since the last operation. The first `git clone` of a large repository still walks the full history, but every `git push` and `git pull` after that is incremental. The legacy `cospan-cache/<remote>/` location is still read if present, to avoid forcing a re-import on upgrade.

## Quick example

```sh
# Clone a repository from a panproto node.
git clone panproto://did:plc:abc123/my-schema-repo

# Push changes to the node.
git push panproto main

# Pull changes from the node.
git pull panproto main
```

The legacy `cospan://` URL scheme is accepted as an alias.

## Installation

Install via Homebrew:

```sh
brew install panproto/tap/panproto-git-remote
```

Or via the shell installer (macOS/Linux):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/panproto/panproto/releases/latest/download/panproto-git-remote-installer.sh | sh
```

Or download binaries directly from [GitHub Releases](https://github.com/panproto/panproto/releases).

Verify the installation:

```sh
git-remote-panproto --help
```

## Authentication

Set the `PANPROTO_PUSH_TOKEN` environment variable to authenticate with the panproto node:

```sh
export PANPROTO_PUSH_TOKEN="your-token-here"
git push panproto main
```

`PANPROTO_TOKEN`, `COSPAN_PUSH_TOKEN`, and `COSPAN_TOKEN` are accepted as fallbacks, in that order.

## License

[MIT](../../LICENSE)
