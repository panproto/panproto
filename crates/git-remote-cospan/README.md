# git-remote-cospan

Git remote helper for `cospan://` URLs.

## What it does

When git encounters a remote URL that starts with `cospan://`, it looks for a binary called `git-remote-cospan` on your PATH and hands control to it. This binary speaks the git remote-helper protocol on stdin and stdout, translating git's push and fetch commands into calls against a panproto node over XRPC.

On push, the helper imports the git commits being pushed into panproto objects (using `panproto-git`), then sends those objects to the node server using `panproto-xrpc`. On fetch, it pulls panproto objects from the node and exports them back to git tree and commit objects so git can merge them into the local repository. The translation preserves author names, emails, timestamps, commit messages, and parent links in both directions.

A persistent cache lives under `$GIT_DIR/cospan-cache/<remote>/` so that subsequent pushes and fetches only process commits that are new since the last operation. The first `git clone` of a large repository still walks the full history, but every `git push` and `git pull` after that is incremental.

## Quick example

```sh
# Clone a repository from a panproto node.
git clone cospan://did:plc:abc123/my-schema-repo

# Push changes to the node (installed alongside panproto-cli).
git push cospan main

# Pull changes from the node.
git pull cospan main
```

The binary is installed automatically alongside `schema` (panproto-cli). You do not need to install it separately.

## License

[MIT](../../LICENSE)
