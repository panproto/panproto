# Extending panproto

A contributor writing new code for panproto is usually doing one of three things: adding support for a data format or schema language the engine does not yet know, adding a transformation shape the lens library does not yet offer, or adding a small computation the expression language needs but cannot express with its existing builtins. The present chapter walks through each case end-to-end.

## A new protocol

Adding a protocol is the most common contribution and follows a predictable shape. The contributor starts by identifying the protocol's category (serialisation, database, document, or domain) and the subdirectory of [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) that houses it. A new file under that subdirectory collects the four artefacts the protocol supplies ([the GAT, the parser, the emitter, the registry entry](../protocols/defining.md)) and a `mod.rs` update adds the file to the crate's module tree.

The theory is written first, usually through [`panproto-theory-dsl`](https://docs.rs/panproto-theory-dsl/latest/panproto_theory_dsl/) in [Nickel](https://nickel-lang.org/) form for readability, though a direct Rust construction through the [`panproto_gat::theory::Theory`](https://docs.rs/panproto-gat/latest/panproto_gat/theory/struct.Theory.html) builder works equally well. A type-checker pass validates the theory before the parser and emitter are written against it.

The parser is the longest piece. It consumes bytes in the protocol's native format and produces a [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) value under the theory. Panproto's existing parsers vary in strategy: the Avro parser is a hand-written recursive-descent parser over the JSON form, the FHIR parser uses [`serde_json`](https://docs.rs/serde_json/latest/serde_json/) directly and maps the resulting `Value` tree to schema construction calls, and the tree-sitter-based language parsers run the tree-sitter library and walk the resulting concrete-syntax tree. A contributor picks the strategy that fits their protocol's format.

The emitter is the inverse. It takes a schema and serialises it back to the protocol's native format, respecting the round-trip property (the emitter's output, parsed again, produces a schema equivalent to the original). Round-trip equivalence is verified by a test suite the contributor writes alongside the parser and emitter.

The registry entry is a two-line call to the `register` function of [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/), binding the theory, parser, and emitter under a string identifier. The identifier is usually the reverse-DNS name of the protocol's specification organisation, to avoid collisions with other protocols.

A protocol is considered tested when three checks pass together: the theory validates as well-formed, parse-then-emit reproduces the original bytes up to a declared equivalence, and at least one integration test drives a schema through a trivial migration and inspects the result.

## A new lens combinator

Lens combinators extend [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) with new ways of composing lenses or of constructing lenses from ingredients the library did not previously support. A contributor adding a combinator follows the [Cambria](https://docs.rs/panproto-lens/latest/panproto_lens/) pattern documented in [Bidirectional lenses](../core/lenses.md): the combinator is a function that takes inputs and returns a [`Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) value whose `get` and `put` are derived from the inputs.

The work is in verifying the round-trip laws. A combinator's `put` must satisfy GetPut and PutGet against the combinator's `get`; a contributor cannot rely on the laws being obvious from the code and must write a law-verification test. The standard pattern is a [`proptest`](https://docs.rs/proptest/latest/proptest/) test over the combinator's input space that constructs a lens, samples input values and modifications, and checks that the round-trip equations hold pointwise.

Combinators that cannot satisfy the round-trip laws on all inputs but do satisfy them on a restricted subset are still valuable; the contributor declares the restriction explicitly and documents it in the combinator's rustdoc. A typical restriction is "GetPut holds for sources whose complement is consistent with the modification's view"; [`panproto-lens`](https://docs.rs/panproto-lens/latest/panproto_lens/) has a trait for expressing this kind of pre-condition, and the law-verification test respects it.

Symbolic simplification rules ([`panproto_lens::symbolic`](https://docs.rs/panproto-lens/latest/panproto_lens/symbolic/)) may need updating when a new combinator is added. The simplifier is a term-rewriting system over a finite set of lens identities; a new combinator that reduces under composition with existing ones should have its simplification rules added.

## A new expression-language builtin

Builtins live in [`panproto_expr::builtin`](https://docs.rs/panproto-expr/latest/panproto_expr/builtin/). Each builtin is a function with a fixed arity, a type signature declared against [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/)'s type language, and a reduction rule that evaluates it on value arguments. A contributor adding a builtin writes:

- A new variant of the `Builtin` enum.
- A type-checker clause in [`panproto_expr::typecheck`](https://docs.rs/panproto-expr/latest/panproto_expr/typecheck/) that accepts the builtin and produces its result type.
- A reduction rule in [`panproto_expr::eval`](https://docs.rs/panproto-expr/latest/panproto_expr/eval/) that evaluates the builtin on values.
- Documentation in the public rustdoc naming the builtin's semantics precisely enough that the reduction rule is a consequence.
- At least one test per kind of input the builtin accepts.

Builtins must be total on well-typed inputs. A builtin that would otherwise fail on a bad input (division by zero, taking the head of an empty list) produces a `BuiltinError` that the evaluator propagates as a standard `EvalError` at the call site. The error rather than a panic is critical: panproto expects every builtin to behave predictably against every input the type-checker admits.

Builtins are not casually added. A builtin that covers a genuine gap in what the expression language can express is worth the addition; a builtin that can be composed from existing ones is better not added, since it enlarges the trusted surface without giving the language anything new. A contributor should discuss a proposed builtin on the issue tracker before writing the PR, partly to ensure the gap is real and partly to align on the name and signature.

## Adding a cross-protocol translation

A separate kind of contribution is a cross-protocol translation: a lens between two existing protocols that panproto does not yet know how to mediate. The construction goes through [`panproto_lens::symmetric`](https://docs.rs/panproto-lens/latest/panproto_lens/symmetric/) (see [Bidirectional lenses](../core/lenses.md)) and is registered through [`panproto-protocols`](https://docs.rs/panproto-protocols/latest/panproto_protocols/) in the cross-protocol table. The details are in the `panproto-cross-protocol` skill.

## Closing

The final chapter of Part VII, [Experimental and feature-gated subsystems](./experimental.md), covers the subsystems that are in the workspace but have not yet stabilised. A contributor whose extension falls into one of those subsystems should read that chapter before proceeding.
