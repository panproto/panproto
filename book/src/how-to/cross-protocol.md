# Translate across protocols

panproto has two different cross-protocol cases. The TypeScript SDK can apply an explicit vertex-and-edge mapping between two protocol-tagged schema handles. It does not construct a shared theory or emit a complete target-language document. The CLI does not expose that limited two-protocol path: each `--protocol` argument selects one protocol for both schemas, and the current CLI resolver accepts only `atproto`.

## Decide whether the task is supported

Use [Convert data between schemas](./convert-data.md) when both schemas use `atproto` and are already stored as panproto schema JSON. For a small explicit map in TypeScript, build both schema handles, map every relevant vertex and edge, and compile the migration as shown in [Cross-protocol translation](../tutorials/cross-protocol-translation.md). `checkExistence` selects the source schema's registered protocol, so that report does not establish validity under both protocols. The result of `liftJson` is target-shaped JSON, not a complete OpenAPI, Protobuf, or other target document.

A general bridge requires repository-level implementation: a shared protocol theory, explicit source and target translations into that theory, and format-specific parsing and emission at the boundaries.

The theory DSL can compile a colimit of building-block theories, but compilation alone does not register a runtime protocol or translate existing JSON Schema, Protobuf, ATProto, or other built-in schemas into the result. Thus a DSL `compose` document is only one component of a cross-protocol bridge.

## Implement a bridge in Rust

The supported building blocks are available in Rust:

1. Define the shared theory and its instance theory, then register both under stable names.
2. Define a `Protocol` whose `schema_theory` and `instance_theory` use those names.
3. Parse each source format with its protocol-specific parser, and write an explicit schema translation into the shared protocol.
4. Build and check a migration between the translated schemas.
5. Parse source records, apply the migration or lens, then serialize with the target format's emitter.

Each translation in steps 3 and 5 is format-specific code. panproto does not infer those boundary translations from the fact that two protocol theories share sorts with names such as `Vertex` or `Edge`.

## Verify the bridge

Test the three boundaries separately: source parsing into the shared schema, migration or lens laws over representative instances, and target emission followed by the target protocol's parser. Constraints with no representation in the shared theory must be reported or handled explicitly; no current generic command detects and reports every such loss.

## CLI boundaries

The following patterns do not provide cross-protocol translation:

- `schema data convert --protocol <name>` accepts one protocol name and loads both schemas under it. The current resolver accepts only `atproto`.
- `schema lens generate --protocol <name>` likewise resolves one protocol for the entire lens, again only `atproto` in the current CLI.
- `schema theory compile` validates and compiles a theory document but does not add it to the running CLI's built-in protocol lookup.

## See also

- [Build a custom protocol](./build-protocol.md) for the repository changes needed to register a protocol.
- [Composing protocols by colimit](../explanation/protocol-colimits.md) for the theory construction.
- [Convert data between schemas](./convert-data.md) for the supported single-protocol path.
