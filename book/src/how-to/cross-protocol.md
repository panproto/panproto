# Translate across protocols

panproto does not currently expose an end-to-end CLI or SDK operation that parses a schema in one built-in protocol, constructs a shared theory with another built-in protocol, and emits the target protocol's schema or data. The CLI's `--protocol` arguments select one registered protocol for both schemas.

## Decide whether the task is supported

Use [Convert data between schemas](./convert-data.md) when both schemas already name the same protocol. For different protocol names, a bridge requires repository-level implementation: a shared protocol theory, source and target schema translations into that theory, and format-specific parsing and emission at the boundaries.

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

## Unsupported commands

The following patterns do not provide cross-protocol translation:

- `schema data convert --protocol <name>` accepts one protocol name and loads both schemas under it.
- `schema lens generate --protocol <name>` likewise resolves one protocol for the entire lens.
- `schema theory compile` validates and compiles a theory document but does not add it to the running CLI's built-in protocol lookup.

## See also

- [Build a custom protocol](./build-protocol.md) for the repository changes needed to register a protocol.
- [Composing protocols by colimit](../explanation/protocol-colimits.md) for the theory construction.
- [Convert data between schemas](./convert-data.md) for the supported single-protocol path.
