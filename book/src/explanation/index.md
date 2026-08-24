# Explanation

These chapters explain the representations and checks that underlie panproto. They assume that you can read a [schema](../glossary.md#schema "A schema is a model of a protocol's schema theory.") and a [structural diff](../glossary.md#structural-diff "A structural diff records added, removed, and modified schema elements without classifying compatibility."), but they do not assume category theory. Procedures belong in the [how-to guides](../how-to/index.md), while interface details belong in the [reference](../reference/index.md).

## Schemas, migrations, and lenses

Begin with [What panproto solves](./what-panproto-solves.md), and consult [The vocabulary in plain terms](./decoder-ring.md) when an unfamiliar term appears. [Schemas as theories](./schemas-as-theories.md) first describes the common representation used for different [protocols](../glossary.md#protocol "A protocol identifies a schema language and the theories and structural rules that define it."). [Migrations as morphisms](./migrations-as-morphisms.md) then explains a [migration](../glossary.md#migration "A migration maps a source schema to a target schema and determines how instances move between them.") and the operations that move data along it. Search may return a partial correspondence as a [span](../glossary.md#span "A span states a partial correspondence through a common apex and one morphism into each schema."). Together, these chapters supply the background needed by the tutorials and most how-to guides.

When a conversion discards information or must support updates in both directions, continue from the glossary definition of a [lens](../glossary.md#lens "A lens pairs a forward conversion with a law-governed backward update.") to [Lenses and round-trip laws](./lenses-roundtrip.md). [What panproto verifies](./what-is-verified.md) distinguishes runtime validation and test evidence from properties that the implementation assumes.

## Search, composition, and version control

[Searching for a morphism](./morphism-search.md) describes how panproto searches for a partial or total schema correspondence. [Alignment evidence](./alignment-evidence.md) explains how names, types, and other evidence affect that search.

[Composing protocols by colimit](./protocol-colimits.md) concerns the [colimit](../glossary.md#colimit "A colimit combines theories over their explicitly shared parts.") construction used to describe protocol structure. [Schema version control semantics](./vcs-semantics.md) concerns structural changes to concrete schemas and their histories. The two chapters use related categorical constructions at different levels of the system.

[Layout enrichment](./layout-enrichment.md) and [Source-code emission](./emit-pretty.md) address parsing, canonical emission, and preservation of source layout. [Architecture](./architecture.md) identifies the crates that implement these operations and the data that crosses their boundaries.

## Denotational semantics

The [denotational semantics](./semantics/index.md) chapters specify the expression language, both DSLs, protolens composition, merge, and the theory REPL. Begin with [Shared notation](./semantics/shared-notation.md); the remaining chapters assume familiarity with typed abstract syntax, inference rules, and elementary category theory.

[Related work](./related-work.md) locates these constructions in the literature. It is best read after the chapters on schemas, migrations, and lenses.

## Chapter map

| Question | Read |
|---|---|
| What problem and vocabulary do I need? | [What panproto solves](./what-panproto-solves.md), then [The vocabulary in plain terms](./decoder-ring.md) |
| How are schemas and migrations represented? | [Schemas as theories](./schemas-as-theories.md), then [Migrations as morphisms](./migrations-as-morphisms.md) |
| How does automatic correspondence work? | [Searching for a morphism](./morphism-search.md), then [Alignment evidence](./alignment-evidence.md) |
| What makes a migration bidirectional? | [Lenses and round-trip laws](./lenses-roundtrip.md) |
| How are protocols or branches combined? | [Composing protocols by colimit](./protocol-colimits.md), then [Schema version control semantics](./vcs-semantics.md) |
| How does source-code round-tripping work? | [Layout enrichment](./layout-enrichment.md), then [Source-code emission](./emit-pretty.md) |
| Which claims are mechanically checked? | [What panproto verifies](./what-is-verified.md) |
| Where does the implementation live? | [Architecture](./architecture.md) |
