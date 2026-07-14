# panproto

panproto is a toolchain for evolving schemas: defining them, migrating data across versions, translating between protocols (JSON Schema, Protobuf, GraphQL, Avro, ATProto, SQL DDL, and many more), versioning the whole thing the way git versions source, and round-tripping data through bidirectional transforms whose laws are mechanically checked.

This documentation is organised in four quadrants, following the [Diataxis](https://diataxis.fr/) framework. Pick the one that matches what you are trying to do.

## [Tutorials](./tutorials/index.md)

Learning by doing. Pick this if you are new and want a guided sit-down with a working example at the end. No prior knowledge of category theory or schema theory is assumed.

## [How-to guides](./how-to/index.md)

Recipes for specific tasks: defining a schema in your language of choice, building a migration, wiring breaking-change detection into CI, querying instances, parsing full ASTs, bridging panproto's version control to git. Pick this when you know what you want to do and need the steps.

## [Reference](./reference/index.md)

Authoritative listings: every CLI subcommand, the SDK surfaces for Rust, TypeScript, and Python, the protocol catalogue, the expression-language builtins, the lens combinator algebra, the `panproto.toml` schema. Tables and signatures, no exposition.

## [Explanation](./explanation/index.md)

Why the system is shaped the way it is. What schemas, migrations, lenses, and merges *mean* under the hood. The denotational semantics of panproto's three DSLs, and a precise account of which categorical properties the implementation mechanically verifies. Most pages here are accessible to working developers; the `semantics/` cluster is the place where the math is load-bearing.

## Where to start

| You are... | Start at |
|---|---|
| New to panproto | [Your first diff](./tutorials/your-first-diff.md) |
| Looking up a term of art | [The vocabulary in plain terms](./explanation/decoder-ring.md) |
| Adding panproto to an existing project | [Install](./how-to/install/index.md) |
| Looking up a CLI flag | [CLI reference](./reference/cli.md) |
| Wondering what schemas-as-theories means | [Schemas as theories](./explanation/schemas-as-theories.md) |
| Building a custom protocol | [Build a custom protocol](./how-to/build-protocol.md) |
| Setting up CI gates | [Breaking-change gate](./how-to/ci/breaking-change-gate.md) |
