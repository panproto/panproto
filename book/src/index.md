# panproto

panproto compares [schema](./glossary.md#schema "A schema is a model of a protocol's schema theory.") versions, constructs [migrations](./glossary.md#migration "A migration maps a source schema to a target schema and determines how instances move between them."), and applies those migrations to data. The shortest useful introduction is [Your first diff](./tutorials/your-first-diff.md): two ordinary ATProto Lexicon documents go in, and a [structural diff](./glossary.md#structural-diff "A structural diff records added, removed, and modified schema elements without classifying compatibility.") with a possible field rename comes out. It takes about five minutes and assumes no category theory.

## Choose a path

The book has three reading paths. Each path moves among tutorials, how-to guides, reference pages, and explanation without collapsing their different jobs.

| Experience | Start here | Continue with |
|---|---|---|
| **New to panproto** | [Your first diff](./tutorials/your-first-diff.md) | [Your first schema](./tutorials/your-first-schema.md), then [Your first migration](./tutorials/your-first-migration.md) |
| **Adding panproto to a project** | [Install](./how-to/install/index.md) | [Define a schema](./how-to/define-schema/index.md), [build a migration](./how-to/build-migration.md), then add a [breaking-change gate](./how-to/ci/breaking-change-gate.md) |
| **Designing protocols or extending the system** | [Build a custom protocol](./how-to/build-protocol.md) | [Find schema spans](./how-to/spans.md), study the [architecture](./explanation/architecture.md), then use the [denotational semantics](./explanation/semantics/index.md) as the mathematical specification |

The [vocabulary in plain terms](./explanation/decoder-ring.md) is a short decoder for unfamiliar terms, and the [glossary](./glossary.md) gives their precise definitions. Keep either page open beside these paths when a term gets in the way.

## Find the kind of answer you need

The book retains the four-part [Diátaxis](https://diataxis.fr/) structure. The distinction concerns the kind of help a page provides, rather than the reader's level.

| Section | Use it when | What you will find |
|---|---|---|
| [Tutorials](./tutorials/index.md) | You are learning by doing | Guided sequences that end with a working artifact |
| [How-to guides](./how-to/index.md) | You have a specific task | Focused procedures, verification steps, and common failures |
| [Reference](./reference/index.md) | You need an exact contract | Commands, signatures, grammars, configuration fields, and protocol support |
| [Explanation](./explanation/index.md) | You need the reason or the model | Design arguments, categorical constructions, architecture, and formal semantics |

If you are unsure which mode you need, begin with a tutorial. A tutorial supplies the context that how-to and reference pages deliberately leave out.
