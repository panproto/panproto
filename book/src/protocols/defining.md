# Defining a protocol

A protocol in panproto, as developed across Part II, is a generalised algebraic theory paired with a parser, an emitter, and a registered entry in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/). This chapter walks through the registration pattern using a small toy protocol so the pieces are visible together, and then points at one of the real protocols shipped with the crate.

A reader familiar with [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) has already seen the theoretical content; the chapter here concentrates on what the Rust code actually does.

## What a protocol supplies

A protocol definition supplies four artefacts to the engine. A [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) value declares the protocol's schema language (sorts, operations, and equations). A parser reads the protocol's native surface syntax (ATProto Lexicon JSON, Avro IDL, Parquet binary, and the like) into a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) under that theory. An emitter performs the inverse. A registry entry, finally, binds the three together under a protocol identifier the engine can look up by name.

All four live in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/), organised by category: serialisation formats under `serialization/`, database and storage protocols under `database/`, document and web formats under `web_document/` and `domain/`, and data-science formats under `data_science/`. Each subdirectory contains one file per protocol plus a `mod.rs` that collects the registrations.

## A toy protocol

The simplest useful protocol is one for a tagged key-value store. The theory declares a sort $\mathsf{Tag}$ (string-like, for record tags) and a sort $\mathsf{Record}$ (the values the store holds), with one operation $\mathsf{tag} : \mathsf{Record} \to \mathsf{Tag}$ sending every record to its tag. The theory has no equations beyond well-typedness.

A schema under this protocol fixes a choice of how tags and records are represented concretely: a schema might choose tags as UTF-8 strings up to 64 bytes and records as JSON values of any shape. A second schema under the same protocol might choose tags as 128-bit UUIDs and records as protobuf-serialised byte strings. Both are schemas of the same theory; they differ in the interpretations of $\mathsf{Tag}$ and $\mathsf{Record}$.

The Rust construction looks like the following.

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

*Listing 6.1: Registering a toy protocol through the [`panproto_protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) crate. The theory builder introduces sorts and operations; the `Protocol` wrapper attaches the parser and emitter; `register` installs the protocol under the string identifier `"toy.kv"` for later lookup.*

Once registered, the protocol is available to every subsequent panproto operation by its identifier. A developer who writes a schema against `"toy.kv"` gets type-checking against the theory, a concrete parser for reading existing `toy.kv` documents from disk, and an emitter for writing schemas back out.

## The parser and emitter

The parser and emitter are user-supplied functions bound by a small trait. The parser takes a byte slice in the protocol's native format and returns a `Result<Schema, ParseError>`. The emitter takes a schema and returns `Result<Vec<u8>, EmitError>`. Both are expected to respect the theory they are registered against: a parser that returns a schema failing to satisfy the theory's equations is an error in the parser, and the type-checker rejects the schema at build time regardless of how it was produced.

Parsers for panproto's shipped protocols are implemented by hand against each protocol's specification. Parsers for programming languages (Python, Rust, TypeScript, and the other 245 tree-sitter-supported languages) are auto-derived from tree-sitter grammars, a process the [Tree-sitter chapter](./tree-sitter.md) develops in full.

## A real protocol

A real protocol worth reading as a reference is [ATProto](https://atproto.com/), whose registration lives in [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/). Its theory declares sorts for lexicons, records, strings, blobs, and the various scalar types ATProto supports; its parser consumes lexicon JSON and produces schemas; its emitter serialises back to the JSON form. The construction follows the pattern above, scaled up to the complexity ATProto requires. The next chapter, [ATProto lexicons](./atproto.md), walks through that scale-up in detail.

## Closing

The remainder of Part IV documents the protocols panproto ships with: [ATProto lexicons](./atproto.md), [Apache Avro](./avro.md), [a relational case study](./relational.md), and [FHIR as a document case study](./document.md). A separate chapter on [tree-sitter and full-AST parsing](./tree-sitter.md) explains how the programming-language protocols are produced automatically from tree-sitter grammars.
