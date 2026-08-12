# How-to guides

How-to pages are recipes. Each one assumes you know what you are trying to do and need the steps to do it. Every page follows the same skeleton:

1. **Prerequisites.** What must be installed and what state your project must be in.
2. **The task.** The minimum sequence of commands or code to accomplish the goal.
3. **Verification.** How to confirm the task succeeded.
4. **Common mistakes.** Failure modes and how to recognise them.
5. **See also.** Adjacent how-tos, the relevant reference page, and the explanation page that covers the underlying model.

If you want to understand *why* a step works, follow the link to the explanation quadrant. If you want a complete walk-through with no prior context, start with a [tutorial](../tutorials/index.md).

## Index

### Setup
- [Install the CLI](./install/cli.md), [Rust](./install/rust.md), [TypeScript](./install/typescript.md), [Python](./install/python.md)

### Schemas and migrations
- [Define a schema](./define-schema/index.md) (CLI, TypeScript, Python, Rust)
- [Build a migration](./build-migration.md)
- [Find a span between two schemas](./spans.md)
- [Apply field transforms](./field-transforms.md)
- [Use lenses](./use-lenses.md), [protolenses](./protolenses.md), [dependent optics](./dependent-optics.md)
- [Write lenses in the lens DSL](./lens-dsl.md)
- [Build a custom protocol](./build-protocol.md)

### Working with data
- [Query instances](./query-instances.md)
- [Convert data between formats](./convert-data.md)
- [Round-trip with format preservation](./format-preserving.md)
- [Parse full ASTs](./parse-full-ast.md)
- [Decorate an abstract schema](./decorate-schemas.md)

### Version control
- [Init and commit](./schema-vcs/init-and-commit.md), [branch and merge](./schema-vcs/branch-and-merge.md), [data versioning](./schema-vcs/data-versioning.md), [git bridge](./schema-vcs/git-bridge.md)

### Translation and integration
- [Translate across protocols](./cross-protocol.md)

### CI
- [Breaking-change gate](./ci/breaking-change-gate.md), [GitHub Actions](./ci/github-actions.md), [pre-commit hooks](./ci/pre-commit-hooks.md)
