# git-remote-cospan

Git remote helper for `cospan://` URLs.

When git encounters a remote URL starting with `cospan://`, it invokes this binary as `git-remote-cospan`. The helper translates between git's remote-helper protocol (stdin/stdout line commands) and panproto's XRPC-based VCS operations, enabling `git push cospan main` and `git fetch cospan` to synchronize structural schemas with a cospan node.

## Protocol

Git sends commands on stdin, one per line:

| Command | Description |
|---------|-------------|
| `capabilities` | Advertise supported features (`push`, `fetch`, `option`) |
| `list` | List remote refs (branches, tags) |
| `list for-push` | List remote refs with push permission |
| `push <src>:<dst>` | Push local ref to remote ref |
| `fetch <sha> <name>` | Fetch a remote ref |

## Translation

On **push**: local git trees are parsed through `panproto-project` to produce structural schemas, which are committed to panproto-vcs and pushed via `panproto-xrpc`.

On **fetch**: panproto-vcs commits are pulled via XRPC, schemas are emitted back to source text via `panproto-parse`, and git objects are created from the emitted files.
