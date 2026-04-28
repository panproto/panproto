# Defining a protocol

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Part IV is where the framework of Part II meets the protocols panproto ships with. The remaining chapters in the part work through specific cases — ATProto, Avro, a relational case study, FHIR, and the tree-sitter-derived protocols — each one an instance of the constructions developed abstractly earlier. The present chapter is the template those cases instantiate: what a protocol registration looks like in Rust, and what obligations the registration imposes on its parser and emitter.

A reader who has followed [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) has seen the theoretical content. The present chapter concentrates on what the Rust code actually does.

## What a protocol supplies

A protocol supplies four artefacts. A [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) value declares the protocol's schema language — its sorts, operations, and equations. A parser takes a byte slice in the protocol's native surface syntax and returns a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) under the theory. An emitter takes a schema and renders it back into the native syntax. A registry entry binds the three together under a protocol identifier that the engine can look up by name.

All four live in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/), organised by category. Serialisation formats live under `serialization/`; database and storage protocols under `database/`; document and web formats under `web_document/` and `domain/`; data-science formats under `data_science/`. Each subdirectory holds one file per protocol plus a `mod.rs` that collects the registrations.

## A toy protocol

The simplest useful protocol is one for a tagged key-value store. The theory has a sort $\mathsf{Tag}$ for record tags and a sort $\mathsf{Record}$ for the stored values, with one operation $\mathsf{tag} : \mathsf{Record} \to \mathsf{Tag}$ mapping every record to its tag. No equations are imposed beyond well-typedness.

A schema under this protocol fixes a concrete interpretation of both sorts. One schema might choose tags as UTF-8 strings up to 64 bytes and records as JSON values of any shape; another might choose tags as 128-bit UUIDs and records as protobuf-serialised byte strings. Both are schemas under the same protocol; they differ in how they interpret $\mathsf{Tag}$ and $\mathsf{Record}$.

In Rust:

```rust
use panproto_gat::theory::Theory;
use panproto_schema::protocol::Protocol;
use panproto_protocols::register;

let theory = Theory::builder()
    .sort("Tag")
    .sort("Record")
    .operation("tag", ("Record",), "Tag")
    .build()?;

let protocol = Protocol::new("toy.kv", theory)
    .with_parser(toy_kv_parser)
    .with_emitter(toy_kv_emitter);

register(protocol);
```

The theory builder introduces sorts and operations one by one; the `Protocol` wrapper attaches the parser and emitter; `register` installs the protocol under the string identifier `"toy.kv"` for later lookup. Once registered, the protocol is available to every subsequent panproto operation by its identifier, and a developer who writes a schema against `"toy.kv"` gets type-checking against the theory, a concrete parser for reading existing documents, and an emitter for writing schemas back.

The toy protocol has two sorts and one operation. A real protocol has tens or hundreds of each. The structural shape is the same: declare the theory, attach a parser and an emitter, register under an identifier. What varies between protocols is the complexity of the theory and the subtlety of the parser and emitter, not the shape of the registration itself.

## The parser and emitter contract

The parser and emitter are user-supplied functions bound by a small trait. The parser takes a byte slice in the protocol's native format and returns a `Result<Schema, ParseError>`; the emitter takes a schema and returns `Result<Vec<u8>, EmitError>`.

Both are expected to respect the theory they are registered against. A parser returning a schema that fails the theory's equations is an error in the parser, and the validator in [`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/validate/) rejects the schema at build time regardless of what the parser thought it was producing.

Most parsers aim to satisfy a round-trip law: emit after parse is the identity on the original bytes. The exact form of the law depends on the protocol. For protocols with unambiguous surface syntax the law is literal. For human-edited formats — YAML, SQL DDL, source code — the law is weakened to "parse-then-emit is the identity up to whitespace and comment layout", and the `panproto-io` crate supplies a *CST complement* that captures the remaining bytes outside the theory's grip. The machinery is developed separately; the parser/emitter trait itself does not require it.

Parsers for the shipped protocols are implemented by hand against each protocol's specification. Parsers for programming languages — Python, Rust, TypeScript, and the other 247 tree-sitter-supported languages — are auto-derived from tree-sitter grammars, a process [the tree-sitter chapter](./tree-sitter.md) develops in full.

## A real protocol

[ATProto](https://atproto.com/) is worth reading as a reference. Its registration lives in [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/). The theory declares sorts for lexicons, records, strings, blobs, and the various scalar types ATProto supports; the parser consumes lexicon JSON and produces schemas; the emitter serialises back to JSON. The construction follows the same four-step pattern as the toy protocol, scaled up to the complexity ATProto requires.

The next chapter walks through the scaled-up version in detail. A reader who wants to see how a protocol is defined in practice rather than in the abstract should read it as the immediate follow-up to this one.

## Further reading

@sannella2012foundations is the textbook-length reference for the algebraic-specification tradition in which protocol definition sits. For practical references on writing parsers and emitters, the [`nom`](https://docs.rs/nom/latest/nom/) parser-combinator crate is what panproto's hand-written parsers use, and [`serde`](https://serde.rs/) is what the emitters use for serialisation.

## Closing

The remaining chapters of Part IV document the shipped protocols: [ATProto lexicons](./atproto.md), [Apache Avro](./avro.md), [a relational case study](./relational.md), and [FHIR as a document case study](./document.md). A separate chapter on [tree-sitter and full-AST parsing](./tree-sitter.md) explains how programming-language protocols are derived from grammars rather than written by hand.
