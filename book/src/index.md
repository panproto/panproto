# Preface

Data outlives its schemas. Any system that stores records for more than a few months eventually faces a situation in which the records on disk were written against one shape and the code that reads them expects another, and the gap between the two has to be closed by somebody working by hand, under time pressure, without any mechanical check that the thing being done is correct. The tools most developers use on either side of this problem, source control and serialization formats, were designed to handle something narrower. [Git](https://git-scm.com/) versions byte sequences. [Protobuf](https://protobuf.dev/), [Avro](https://avro.apache.org/), and [JSON Schema](https://json-schema.org/) describe the shape of those sequences. Neither addresses the relationship between the two, which is where things go wrong.

This book presents a different way of arranging those tools. Its central claim is that the schema, the data under it, and the transformations between schema versions can all be treated as members of the same kind of mathematical object. Once the treatment is carried out, the operations one ordinarily does by hand (merging a schema change across two branches; migrating data across a schema version boundary) follow from machinery developed carefully in a part of mathematics most working developers have not yet seen. The machinery is the categorical treatment of generalised algebraic theories and their morphisms, due in large part to @eilenbergmaclane1945general, @lawvere1963functorial, and @cartmell1986generalised, and applied to the setting of databases by @spivak2012functorial and @spivakwisnesky2015relational. The software discussed in the book, panproto, is an implementation of that arrangement.

## Who this book is for

This book is written for developers comfortable reading code in at least one statically typed language, who have not previously had reason to pick up a category theory textbook. Every concept is introduced from first principles rather than by reference to a prerequisite course. A reader who follows the foundations chapters closely will be in a position to read papers on functorial data migration, bidirectional lenses, and generalised algebraic theories without further help.

The book also has an academic audience in view. Researchers in formal methods, database theory, type theory, and programming languages will find panproto's particular design arranged against a body of literature they are likely to know better than the authors do. Where panproto's design follows published work, the book cites the published work directly; departures and known gaps are also named, the first with justifications, the second as invitations. The bibliography at the end of the book is a single alphabetical list, and every entry has been read before being cited.

## How this book is organized

The book is divided into seven parts plus appendices. Two opening chapters orient the reader and frame panproto against the tools the reader has likely used already. Part I, the mathematical foundations, develops categories, functors and natural transformations, universal properties, colimits, and generalised algebraic theories, in that order. Each concept is introduced through running examples from code before being stated abstractly.

Part II presents panproto's core constructions. It begins with the statement that a protocol is a generalised algebraic theory and a schema is a model of one, and it ends with the corresponding statement that migrations are lenses. The intermediate chapters develop the restrict-and-lift pipeline panproto uses to carry data along a migration, the bidirectional lens laws that make a migration safe to run in either direction, and the dependent families panproto calls protolenses, which apply the same lens shape to infinitely many pairs of related schemas.

Part III documents the small pure expression language the system uses when a migration needs to compute something that depends on a value. Part IV introduces the protocols panproto already knows about through four case studies: ATProto, Avro, a relational protocol, and FHIR as a document protocol. Each case study shows what a well-written protocol definition looks like and what the auto-derivation from [tree-sitter](https://tree-sitter.github.io/tree-sitter/) grammars gives on top of that.

Part V turns to version control. It opens with a careful account of what git actually versions (byte sequences arranged in a Merkle DAG) and what it does not (everything that gives those byte sequences meaning), and uses the gap to motivate the object model panproto-vcs puts in its place. From there the part develops the merge algorithm as a pushout in the category of schemas, the automatic data migration that follows from the inferred schema diff, and the bidirectional bridge between a panproto repository and an ordinary git remote.

Part VI is operational: the [WebAssembly](https://webassembly.org/) boundary, the [Rust](https://www.rust-lang.org/), [TypeScript](https://www.typescriptlang.org/), and [Python](https://www.python.org/) SDKs built on top of it, and the command line. For contributors, Part VII absorbs what previously lived in a separate developer guide. Three appendices close the book: a notation reference, a glossary with back-links to defining chapters, and an open-problems list.

## How to read this book

Readers come with different backgrounds. Someone with prior category-theory experience can start at Part II and treat Part I as reference. A reader without that background should read the first two parts in order and then jump to the protocol, VCS, or SDK part matching whatever practical problem is at hand. Contributors to panproto's code should also read Part VII before the others. The book assumes no previous encounter with panproto itself; everything it uses is developed in its chapters.

## What this book does not claim

Panproto is pre-release software. Several of its subsystems work well enough for the worked examples in the book and would not yet stand up to production use in isolation. We have tried to mark this honestly throughout. Feature-gated and still-shifting subsystems are catalogued in the chapter on experimental and feature-gated subsystems. The open-problems appendix lists the places where the software is ahead of the theory we have a citation for, and the places where the theory is ahead of the software. Reading one of the later chapters as a product announcement would be a mistake, and the chapters are written to prevent that reading.

## Trust commitments

Two commitments run through the book and are stated clearly here in one place. First, every claim about what panproto does is grounded in a specific file in the panproto repository, and the file is named in the chapter that makes the claim. The book does not repeat earlier documentation; the code has been read. Second, every claim about what someone else has already proved or argued is grounded in a specific paper or book, given as an inline citation and collected in the bibliography. Each entry in the bibliography is a work that has been read in full before being cited. Where no prior work on a topic is known, the book says so rather than implying the territory is unexplored.

## Acknowledgments

Panproto is joint work of Aaron Steven White and Blaine Cook, whose patience with the abstraction has shaped the design as much as anything in the chapters that follow. The machinery this book presents sits on top of decades of work in category theory, database theory, programming languages, and formal semantics, and the bibliography is the long version of our thanks to the authors of that work. All remaining errors and misjudgments are ours.
