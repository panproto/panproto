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

The CLI's schema-checking commands accept panproto's internal schema JSON and resolve registered protocol descriptors. The shared loader sends a declared external JSON document through its descriptor's document parser; non-JSON files go through the tree-sitter source-file registry. Use the corresponding SDK parser when an application needs either semantic-protocol parser directly: `parseSchemaDocument` or `parseSchemaSource` in TypeScript, and `parse_schema_document` or `parse_schema_source` in Python. Start with a language SDK when the application constructs the schema programmatically.

## See also

- [Reference: protocol catalog](../../reference/protocols.md) for the list of supported protocols.
- [Schemas as theories](../../explanation/schemas-as-theories.md) for the model.
