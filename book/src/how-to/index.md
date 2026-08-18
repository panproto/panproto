# How-to guides

How-to guides assume that the goal is settled and the missing piece is a procedure. A first encounter with panproto belongs in the [tutorials](../tutorials/index.md); an exact flag, signature, or grammar belongs in the [reference](../reference/index.md).

## First working path

A reader starting from an existing [schema](../glossary.md#schema "A schema is a model of a protocol's schema theory.") file can follow one short sequence: [install a surface](./install/index.md), [define or load a schema](./define-schema/index.md), construct a [migration](../glossary.md#migration "A migration maps a source schema to a target schema and determines how instances move between them.") with the [migration guide](./build-migration.md), and [convert data](./convert-data.md). Each guide includes a verification step before the next operation changes data or repository state.

## Intermediate entry points

These guides assume that you can already load and validate a schema.

| Goal | Start here | Continue with |
|---|---|---|
| Control an inferred [span](../glossary.md#span "A span states a partial correspondence through a common apex and one morphism into each schema.") | [Find a span between two schemas](./spans.md) | [Apply field transforms](./field-transforms.md) |
| Run a [lens](../glossary.md#lens "A lens pairs a forward conversion with a law-governed backward update.") in both directions | [Use lenses](./use-lenses.md) | [Write lenses in the lens DSL](./lens-dsl.md) |
| Inspect or select records | [Query instances](./query-instances.md) | [Expression-language reference](../reference/expression-language.md) |
| Preserve source formatting | [Round-trip with format preservation](./format-preserving.md) | [Decorate an abstract schema](./decorate-schemas.md) |
| Put schema changes under version control | [Initialize and commit](./schema-vcs/init-and-commit.md) | [Branch and merge](./schema-vcs/branch-and-merge.md) |
| Reject incompatible changes automatically | [Create a breaking-change gate](./ci/breaking-change-gate.md) | [Run it in GitHub Actions](./ci/github-actions.md) |

## Advanced entry points

These guides cover reusable transformations, language tooling, and protocol extension.

| Goal | Start here | Related contract |
|---|---|---|
| Reuse one lens across a family of schemas | [Use protolenses](./protolenses.md) | [Lens combinators](../reference/lens-combinators.md) |
| Select an optic from schema structure | [Use dependent optics](./dependent-optics.md) | [Lens combinators](../reference/lens-combinators.md#optic-kinds) |
| Parse and migrate syntax trees | [Parse full ASTs](./parse-full-ast.md) | [Rust SDK](../reference/sdk-rust.md#feature-flags) |
| Translate between schema languages | [Translate across protocols](./cross-protocol.md) | [Protocol catalog](../reference/protocols.md) |
| Add a schema language | [Build a custom protocol](./build-protocol.md) | [Crate map](../reference/crate-map.md) |
| Version data with its schema | [Version data alongside schemas](./schema-vcs/data-versioning.md) | [CLI reference](../reference/cli.md#schema-data) |

## Tasks by area

Setup and schema creation are collected under [Install panproto](./install/index.md) and [Define a schema](./define-schema/index.md). Migration work begins with [Build a migration](./build-migration.md) and extends through [field transforms](./field-transforms.md), [lenses](./use-lenses.md), [protolenses](./protolenses.md), [dependent optics](./dependent-optics.md), and the [lens DSL](./lens-dsl.md).

Data tasks cover [conversion](./convert-data.md), [queries](./query-instances.md), [format-preserving round trips](./format-preserving.md), [full-AST parsing](./parse-full-ast.md), and [schema decoration](./decorate-schemas.md). Repository tasks are grouped under [Schema version control](./schema-vcs/index.md) and [Continuous integration](./ci/index.md); [Translate across protocols](./cross-protocol.md) and [Build a custom protocol](./build-protocol.md) cover extension across protocol boundaries.
