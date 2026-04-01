# panproto-xrpc

XRPC client for cospan node VCS operations.

Provides an async HTTP client for communicating with cospan nodes via the [XRPC protocol](https://atproto.com/specs/xrpc) (the AT Protocol's RPC layer). Used by `git-remote-cospan` and the `schema` CLI to push, pull, and sync panproto VCS repositories with remote cospan nodes.

## API

| Item | Description |
|------|-------------|
| `XrpcClient` | Async HTTP client for XRPC endpoints |
| `push_objects` | Push content-addressed objects to a remote node |
| `pull_objects` | Fetch objects by hash from a remote node |
| `negotiate_refs` | Exchange branch and tag references with a remote |
| `XrpcError` | Error type for network, auth, and protocol failures |

## Transport

Objects are serialized with MessagePack (`rmp-serde`) for wire efficiency. The client uses `reqwest` with rustls for TLS. Authentication uses AT Protocol DID-based session tokens.
