# Define a schema

A schema in panproto is a graph of vertices and edges, validated against a protocol's GAT. You can build one from any of the four surfaces, with the same outcome.

| Surface | Page |
|---|---|
| `schema` CLI | [From the CLI](./cli.md) |
| TypeScript SDK | [From TypeScript](./typescript.md) |
| Python SDK | [From Python](./python.md) |
| Rust SDK | [From Rust](./rust.md) |
| Haskell SDK | [From Haskell](./haskell.md) |
| Swift SDK | [From Swift](./swift.md) |

The CLI is the recommended starting point if you have an existing schema file (JSON Schema, Protobuf, ATProto Lexicon, ...) you want to load and inspect. The language SDKs are the recommended starting point if you are building schemas programmatically inside an application.

## See also

- [Reference: protocol catalogue](../../reference/protocols.md) for the list of supported protocols.
- [Schemas as theories](../../explanation/schemas-as-theories.md) for the model.
