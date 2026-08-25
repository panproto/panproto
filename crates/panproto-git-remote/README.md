# panproto-git-remote

[![crates.io](https://img.shields.io/crates/v/panproto-git-remote.svg)](https://crates.io/crates/panproto-git-remote)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Git remote helper for `panproto://` URLs. The crate installs the
`git-remote-panproto` binary.

## Operation

Git invokes the binary as `git-remote-panproto <remote> <url>`. The helper implements
the remote-helper `capabilities`, `list`, `fetch`, and `push` exchanges. Push imports
Git commits through `panproto-git` and sends panproto objects through `panproto-xrpc`.
Fetch performs the reverse translation for the requested remote ref.

Per-remote state is stored below `$GIT_DIR/panproto-cache/<remote>/`. If that cache is
absent and a legacy `$GIT_DIR/cospan-cache/<remote>/` store exists, the helper uses the
legacy store. Blob and commit marks make later translations incremental, subject to
the contents of those caches.

Translation retains author display names, timestamps, messages, and mapped parent
links. It does not retain Git author email because panproto commits have no email
field. Export synthesizes `<author>@panproto`.

The legacy `cospan://` URL prefix remains accepted.

## Standalone commands

```sh
# Pre-import commits into the shared warm cache. HEAD is the default revspec.
git-remote-panproto warm [<revspec>]

# Install the post-commit hook that updates that warm cache.
git-remote-panproto install-hooks
```

Install the published binary with Cargo:

```sh
cargo install panproto-git-remote
command -v git-remote-panproto
```

## Authentication

Push operations read the first available variable in this order:

1. `PANPROTO_PUSH_TOKEN`
2. `PANPROTO_TOKEN`
3. `COSPAN_PUSH_TOKEN`
4. `COSPAN_TOKEN`

## License

[MIT](../../LICENSE)
