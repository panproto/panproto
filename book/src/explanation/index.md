# Explanation

The explanation quadrant develops the concepts behind panproto. It assumes that you can read a [schema](../glossary.md#schema "A schema is a model of a protocol's schema theory.") and a [structural diff](../glossary.md#structural-diff "A structural diff records added, removed, and modified schema elements without classifying compatibility."), but it does not assume category theory. The pages remain explanations in the Diátaxis sense: they account for design choices and guarantees, while the [how-to guides](../how-to/index.md) provide procedures and the [reference](../reference/index.md) records the interfaces.

The material has three levels. Each level has a stopping point, so the route can match the question that brought you here.

## Intermediate path: from schemas to migrations

Start with [What panproto solves](./what-panproto-solves.md), then keep [The vocabulary in plain terms](./decoder-ring.md) nearby while reading [Schemas as theories](./schemas-as-theories.md) and [Migrations as morphisms](./migrations-as-morphisms.md). This route explains why [protocols](../glossary.md#protocol "A protocol identifies a schema language and the theories and structural rules that define it.") share one internal model, what a [migration](../glossary.md#migration "A migration maps a source schema to a target schema and determines how instances move between them.") records, and why a partial correspondence is returned as a [span](../glossary.md#span "A span states a partial correspondence through a common apex and one morphism into each schema."). It is enough background for the tutorials and most how-to guides.

When backward updates or information loss are part of the problem, continue from the glossary definition of a [lens](../glossary.md#lens "A lens pairs a forward conversion with a law-governed backward update.") to [Lenses and round-trip laws](./lenses-roundtrip.md). The intermediate stopping point is [What panproto verifies](./what-is-verified.md), which separates runtime checks, CI evidence, and properties that remain assumptions.

## Advanced path: search, composition, and version control

The advanced route assumes the intermediate one. [Searching for a morphism](./morphism-search.md) gives the optimization model; [Alignment evidence](./alignment-evidence.md) explains how uncertain correspondences enter that model. These two chapters are the densest part of the main path, and the second depends on the reward-only evidence discipline defined in the first.

Protocol and repository composition form the next pair. The [colimit](../glossary.md#colimit "A colimit combines theories over their explicitly shared parts.") construction is developed in [Composing protocols by colimit](./protocol-colimits.md), then [Schema version control semantics](./vcs-semantics.md) applies the related pushout construction to merge. [Layout enrichment](./layout-enrichment.md) and [Source-code emission](./emit-pretty.md) form a separate branch for readers concerned with parsing, canonical emission, or source-layout preservation.

Read [Architecture](./architecture.md) after one of these branches rather than before it. The crate graph is easier to retain once the abstractions crossing its boundaries have names.

## Formal path: denotational semantics

The [denotational semantics](./semantics/index.md) cluster is the formal endpoint. Its pages specify the expression language, both DSLs, protolens composition, pushout-based merge, and the theory REPL. Begin with [Shared notation](./semantics/shared-notation.md); the cluster otherwise assumes comfort with typed abstract syntax, inference rules, and elementary category theory.

[Related work](./related-work.md) is a capstone for either the advanced or formal route. It locates the design in the literature without introducing the constructions from scratch.

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
