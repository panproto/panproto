# Preface

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Data outlives its schemas. Any system that stores records for more than a few months eventually faces a situation in which the records on disk were written against one shape and the code that reads them expects another, and the gap between the two has to be closed by somebody working by hand, under time pressure, without a mechanical check that the thing being done is correct. The tools most developers reach for on either side of this problem — source control on one side, serialisation formats on the other — were designed for something narrower. [Git](https://git-scm.com/) versions byte sequences. [Protobuf](https://protobuf.dev/), [Avro](https://avro.apache.org/), and [JSON Schema](https://json-schema.org/) describe the shape of those sequences. Neither addresses the relationship between the two, which is where things go wrong.

This book presents a different way of arranging those tools. Its central claim is that the schema, the data under it, and the transformations between schema versions can all be treated as members of the same kind of mathematical object. Once the treatment is carried out, the operations one ordinarily does by hand — merging a schema change across two branches, migrating data across a schema version boundary — follow from constructions worked out carefully in a part of mathematics most working developers have not yet seen. The mathematics is the categorical treatment of generalised algebraic theories and their morphisms, due largely to @eilenbergmaclane1945general, @lawvere1963functorial, and @cartmell1986generalised, applied to databases by @spivak2012functorial and @spivakwisnesky2015relational. The software discussed in the book, panproto, is an implementation of the arrangement.

## Who this book is for

The book is written for developers comfortable reading code in at least one statically typed language, who have not previously picked up a category theory textbook. Every concept is introduced from first principles rather than by reference to a prerequisite course. A reader who follows the foundations chapters closely will be in a position to read papers on functorial data migration, bidirectional lenses, and generalised algebraic theories without further help.

Researchers in formal methods, database theory, type theory, and programming languages will recognise the underlying literature: functorial semantics, bidirectional transformations, generalised algebraic theories. Citations point to the papers whose constructions panproto adopts, and departures from those designs are flagged at the point they occur.

## How this book is organised

The book has seven parts plus appendices. Two opening chapters orient the reader and position panproto against the tools a working developer has already used. Part I develops the mathematical foundations — categories, functors and natural transformations, universal properties, colimits, and generalised algebraic theories, in that order. Each concept is introduced through running examples from code before being stated abstractly.

Part II develops panproto's core constructions. It opens with the identification of a protocol with a generalised algebraic theory and a schema with a model of one, and closes with the corresponding identification of migrations with bidirectional lenses. The intermediate chapters cover the restrict-and-lift pipeline panproto uses to carry data along a migration, the round-trip laws that make a migration safe to run in either direction, and the dependent families panproto calls protolenses.

Part III documents the small pure expression language the engine uses when a migration needs to compute a value that depends on a record. Part IV covers the protocols panproto already knows about, through four case studies — ATProto, Avro, a relational case, and FHIR as a document case — plus the tree-sitter-based auto-derivation that produces protocols for 248 programming languages.

Part V turns to version control. It opens with a careful account of what git actually versions (byte sequences in a Merkle DAG) and what it does not (everything that gives those byte sequences meaning), and uses the gap to motivate the object model panproto-vcs puts in its place. From there the part develops the merge algorithm as a pushout in the category of schemas, the automatic data migration that follows from the inferred schema diff, and the bidirectional bridge between a panproto repository and an ordinary git remote.

Part VI is operational: the [WebAssembly](https://webassembly.org/) boundary, the [Rust](https://www.rust-lang.org/), [TypeScript](https://www.typescriptlang.org/), and [Python](https://www.python.org/) SDKs built on top of it, and the command line. Part VII is for contributors: the workspace layout, the CI pipeline, how to extend panproto with a new protocol or lens combinator, and the experimental subsystems. Three appendices close the book: a notation reference, a glossary, and an open-problems list.

## How to read this book

A reader with prior category-theory experience can start at Part II and treat Part I as reference. Without that background, the first two parts repay reading in order; the protocol, VCS, and SDK parts that follow can be read in whatever order matches the practical problem at hand. Contributors to panproto's code should also read Part VII before the others. The book assumes no previous encounter with panproto.

## What the software can do today

Panproto is pre-release software. Several of its subsystems are complete enough to support the worked examples of the book and would not yet stand up to production use in isolation. Feature-gated subsystems still under development are catalogued in the chapter on experimental and feature-gated subsystems. The open-problems appendix lists the places where the software is ahead of the theory we have a citation for, and the places where the theory is ahead of the software.
