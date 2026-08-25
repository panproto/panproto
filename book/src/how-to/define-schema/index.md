# Define a schema

Choose a surface according to where the schema enters the project. The CLI loads and inspects schema files; the five SDKs construct schemas inside an application.

| Surface | Page |
|---|---|
| `schema` CLI | [From the CLI](./cli.md) |
| TypeScript SDK | [From TypeScript](./typescript.md) |
| Python SDK | [From Python](./python.md) |
| Rust SDK | [From Rust](./rust.md) |
| Haskell SDK | [From Haskell](./haskell.md) |
| Swift SDK | [From Swift](./swift.md) |

The CLI's schema-checking commands currently accept panproto's internal schema JSON and resolve only the `atproto` protocol. Its shared loaders can parse an ATProto Lexicon when a manifest selects `atproto`, and its full-AST commands parse supported source files as syntax trees. Use a language SDK's `parseSchemaDocument` or `parseSchemaSource` dispatch for the other external schema languages in the protocol catalog. Start with a language SDK when the application constructs the schema programmatically.

## See also

- [Reference: protocol catalog](../../reference/protocols.md) for the list of supported protocols.
- [Schemas as theories](../../explanation/schemas-as-theories.md) for the model.
