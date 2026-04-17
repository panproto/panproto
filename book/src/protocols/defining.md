# Defining a protocol

Part IV opens with the practical mechanics of registering a protocol. A protocol, as developed across Part II, is a generalised algebraic theory paired with a parser, an emitter, and an entry in panproto's registry. The present chapter walks through the Rust code that assembles those four artefacts into a working protocol, using a toy key-value store as the running example. The chapters that follow then cover panproto's real protocols as case studies.

A reader familiar with [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) has seen the theoretical content already. This chapter is about what the implementation actually looks like: which types go in each slot, how the parser and emitter fit together, and where the registered protocol lives in the crate layout.

This chapter covers:

- the four artefacts a protocol supplies (theory, parser, emitter, registry entry)
- the crate layout in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/)
- a toy protocol (a tagged key-value store) registered end-to-end
- the parser/emitter contract and the trait it implements
- where to find a real protocol (ATProto) whose structure matches the toy one at a larger scale

The chapters after this one walk through specific shipped protocols in depth. This chapter is the template the others instantiate.

## What a protocol supplies

A protocol definition supplies four artefacts to the engine.

A [`Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) value declares the protocol's schema language: the sorts, operations, and equations that every schema under the protocol must respect.

A parser reads the protocol's native surface syntax — ATProto Lexicon JSON, Avro IDL, Parquet binary, and the like — into a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) under that theory. The parser is expected to respect the theory: a parser that returns a schema failing to satisfy the theory's equations is an error in the parser, and the validator will reject the schema regardless.

An emitter performs the inverse: it takes a schema and renders it back into the protocol's native surface syntax. Round-trip consistency (parse-then-emit reproduces the input bytes up to a declared equivalence) is a quality bar panproto's test suite enforces for every shipped protocol.

A registry entry, finally, binds the three together under a protocol identifier the engine can look up by name.

All four live in [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/), organised by category. Serialisation formats live under `serialization/`. Database and storage protocols live under `database/`. Document and web formats live under `web_document/` and `domain/`. Data-science formats live under `data_science/`. Each subdirectory contains one file per protocol plus a `mod.rs` that collects the registrations.

## A toy protocol

The simplest useful protocol is one for a tagged key-value store. The theory declares a sort $\mathsf{Tag}$ (string-like, for record tags) and a sort $\mathsf{Record}$ (the values the store holds), with one operation $\mathsf{tag} : \mathsf{Record} \to \mathsf{Tag}$ sending every record to its tag. The theory has no equations beyond well-typedness.

A schema under this protocol fixes a choice of how tags and records are represented concretely. One schema might choose tags as UTF-8 strings up to 64 bytes and records as JSON values of any shape. A second schema under the same protocol might choose tags as 128-bit UUIDs and records as protobuf-serialised byte strings. Both are schemas of the same theory; they differ in the interpretations of $\mathsf{Tag}$ and $\mathsf{Record}$.

The Rust construction:

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

*Listing 9.1: Registering a toy protocol through the [`panproto_protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) crate. The theory builder introduces sorts and operations; the `Protocol` wrapper attaches the parser and emitter; `register` installs the protocol under the string identifier `"toy.kv"` for later lookup.*

Once registered, the protocol is available to every subsequent panproto operation by its identifier. A developer who writes a schema against `"toy.kv"` gets type-checking against the theory, a concrete parser for reading existing `toy.kv` documents from disk, and an emitter for writing schemas back out.

The toy protocol has two sorts and one operation. A real protocol has tens or hundreds of each. The structural shape, however, is the same: declare the theory, attach a parser and an emitter, register under an identifier. What varies between protocols is the complexity of the theory and the subtlety of the parser and emitter, not the shape of the registration itself.

## The parser and emitter contract

The parser and emitter are user-supplied functions bound by a small trait. The parser takes a byte slice in the protocol's native format and returns a `Result<Schema, ParseError>`. The emitter takes a schema and returns `Result<Vec<u8>, EmitError>`.

Both are expected to respect the theory they are registered against. A parser that returns a schema failing to satisfy the theory's equations is an error in the parser, and the validator in [`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/validate/) rejects the schema at build time regardless of how it was produced.

The round-trip law most parsers and emitters aim to satisfy is

$$\mathrm{emit}(\mathrm{parse}(\mathsf{bytes})) \;=\; \mathsf{bytes}$$

for every `bytes` that `parse` accepts. In practice this exact law holds only for protocols whose surface syntax is unambiguous; for human-edited formats (YAML, DDL, source code) the engineering goal is the weaker "parse-emit is the identity up to whitespace and comment layout", and the `panproto-io` crate supplies a *CST complement* that captures the remaining bytes outside the theory's grip. That machinery is developed separately; the parser/emitter trait itself does not require it.

Parsers for panproto's shipped protocols are implemented by hand against each protocol's specification. Parsers for programming languages (Python, Rust, TypeScript, and the other 245 tree-sitter-supported languages) are auto-derived from tree-sitter grammars, a process the [Tree-sitter chapter](./tree-sitter.md) develops in full.

## A real protocol

A real protocol worth reading as a reference is [ATProto](https://atproto.com/), whose registration lives in [`panproto_protocols::web_document::atproto`](https://docs.rs/panproto-protocols/latest/panproto_protocols/web_document/atproto/). Its theory declares sorts for lexicons, records, strings, blobs, and the various scalar types ATProto supports. Its parser consumes lexicon JSON and produces schemas. Its emitter serialises back to the JSON form.

The construction follows the pattern of Listing 9.1, scaled up to the complexity ATProto requires. The next chapter, [ATProto lexicons](./atproto.md), walks through that scale-up in detail. A reader who wants to see how a protocol is defined in practice rather than in the abstract should read that chapter as soon as they are done with this one.

## Further reading

For the abstract framework this chapter's registration is an instantiation of, @sannella2012foundations is the textbook-length reference. The algebraic-specification tradition it documents treats protocol definition as the specification of a theory together with implementations; panproto's Rust structure follows that tradition closely.

For practical references on writing parsers and emitters, the [`nom`](https://docs.rs/nom/latest/nom/) parser-combinator crate is what panproto's hand-written parsers use, and [`serde`](https://serde.rs/) is what the emitters use for serialisation. Neither is specific to panproto; both are Rust-ecosystem standards a protocol author should be comfortable with before writing a parser by hand.

## Closing

The remainder of Part IV documents the protocols panproto ships with: [ATProto lexicons](./atproto.md), [Apache Avro](./avro.md), [a relational case study](./relational.md), and [FHIR as a document case study](./document.md). A separate chapter, [Tree-sitter and full-AST parsing](./tree-sitter.md), explains how the programming-language protocols are produced automatically from tree-sitter grammars rather than written by hand.
