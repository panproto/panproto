# Tutorials

Begin with a [structural diff](../glossary.md#structural-diff "A structural diff records added, removed, and modified schema elements without classifying compatibility."). The remaining tutorials extend the same `User` example from a [schema](../glossary.md#schema "A schema is a model of a protocol's schema theory.") to a [migration](../glossary.md#migration "A migration maps a source schema to a target schema and determines how instances move between them."), a version history, and a conversion between [protocols](../glossary.md#protocol "A protocol identifies a schema language and the theories and structural rules that define it.").

Follow the beginner path in order:

1. [Your first diff](./your-first-diff.md) compares two ATProto Lexicon documents from the command line. This is the quick success path.
2. [Your first schema](./your-first-schema.md) builds and validates the same model with the TypeScript SDK.
3. [Your first migration](./your-first-migration.md) renames a field, converts a record, and checks the reverse trip.

The next two tutorials branch from that foundation. [Schema version control basics](./schema-vcs-basics.md) is the intermediate path for commits, branches, and structural merge. [Cross-protocol translation](./cross-protocol-translation.md) is the advanced path for an explicit conversion between schemas registered under different protocols.

| Tutorial | Result |
|---|---|
| [Your first diff](./your-first-diff.md) | One structural diff over two schema documents |
| [Your first schema](./your-first-schema.md) | A schema plus valid and invalid records |
| [Your first migration](./your-first-migration.md) | A checked rename with a round-trip assertion |
| [Schema version control basics](./schema-vcs-basics.md) | A repository with a branch and fast-forward merge |
| [Cross-protocol translation](./cross-protocol-translation.md) | A narrow forward conversion between structurally compatible schemas |

Tutorials are learning-oriented: each leaves you with a runnable result and explains only the concepts needed to produce it. The [how-to guides](../how-to/index.md) collect task-specific procedures, the [reference](../reference/index.md) records the complete interfaces, and the [explanation chapters](../explanation/index.md) develop the theory behind the commands.
