# panproto

panproto compares [schema](./glossary.md#schema "A schema is a model of a protocol's schema theory.") versions, constructs [migrations](./glossary.md#migration "A migration maps a source schema to a target schema and determines how instances move between them."), and applies those migrations to data. [Your first diff](./tutorials/your-first-diff.md) introduces that workflow with two ATProto Lexicon documents and the [structural diff](./glossary.md#structural-diff "A structural diff records added, removed, and modified schema elements without classifying compatibility.") between them.

## Choose a path

Choose a path based on the work you need to do:

| Experience | Start here | Continue with |
|---|---|---|
| New to panproto | [Your first diff](./tutorials/your-first-diff.md) | [Your first schema](./tutorials/your-first-schema.md), then [Your first migration](./tutorials/your-first-migration.md) |
| Adding panproto to a project | [Install](./how-to/install/index.md) | [Define a schema](./how-to/define-schema/index.md), [build a migration](./how-to/build-migration.md), then add a [breaking-change gate](./how-to/ci/breaking-change-gate.md) |
| Designing protocols or extending the system | [Build a custom protocol](./how-to/build-protocol.md) | [Find schema spans](./how-to/spans.md), study the [architecture](./explanation/architecture.md), then consult the [denotational semantics](./explanation/semantics/index.md) |

The [vocabulary in plain terms](./explanation/decoder-ring.md) introduces unfamiliar terms. The [glossary](./glossary.md) records their reference definitions.

## Find the kind of answer you need

The book follows the four-part [Diátaxis](https://diataxis.fr/) structure:

| Section | Use it when | What you will find |
|---|---|---|
| [Tutorials](./tutorials/index.md) | You are learning by doing | Guided sequences |
| [How-to guides](./how-to/index.md) | You have a specific task | Procedures, verification steps, and common failures |
| [Reference](./reference/index.md) | You need an exact contract | Commands, signatures, configuration fields, and supported surfaces |
| [Explanation](./explanation/index.md) | You need the reason or the model | Design arguments, categorical constructions, architecture, and semantics |

Begin with a tutorial if you need both context and a worked sequence. Use reference pages when you already know the operation and need its exact contract.
