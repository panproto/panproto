# Why bounded pure evaluation

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

Every choice of DSL in a system as opinionated as panproto invites the question of whether an existing language would have served. The question has a specific answer, and this chapter gives it.

The short version: panproto needs a language that is pure, bounded, deterministic, serialisable, and capable of pattern-matching over panproto's own schema-indexed types, and no existing candidate satisfies all five. The chapter walks through the requirements, the four closest candidates — Starlark, Dhall, Nickel, CUE — and what each one gives up at the boundary that turned out to matter.

## The five requirements

The migration engine embeds a language for one job. At each site of a field transform or pushforward declaration, the engine evaluates a user-written expression that consumes values visible at the site (fields of the current record, possibly a schema handle, possibly an instance value) and produces a value for the target. Evaluation happens inside the engine's compile stage, outside any user-controlled runtime, and the result is an input to the lift function that will run against every record of the input instance.

Five requirements follow from this context. Each rules out a different class of otherwise reasonable language.

*Purity.* The language cannot perform I/O. Compile-stage evaluation runs in environments where I/O may not be available or may not be desired, and a language that allows side effects leaks them into places they should not be. Starlark satisfies this; most full-featured scripting languages do not.

*Boundedness.* Every evaluation must terminate within a configurable budget. A migration that embeds a user expression and runs it across millions of records cannot afford to block on one expression that happens to loop. This rules out Turing-complete languages that cannot make termination a static property.

*Determinism.* Two evaluations of the same expression on the same input must produce the same output. The engine treats expression evaluation as a pure component of a larger functorial computation, and non-determinism at this layer would propagate to every subsequent stage.

*Serialisability.* An in-flight evaluation must be suspendable and resumable across process boundaries. Panproto's batch-migration tooling parallelises evaluation across workers, and lazy evaluation schemes whose suspended state cannot travel cleanly between processes do not work.

*Native handling of panproto types.* The language must be able to inspect, construct, and pattern-match on schemas and instances without serialising them through an external format on every operation. A language that treats panproto values as opaque data that has to be marshalled in and out on every call pays a translation cost large enough to rule it out for expressions that run millions of times.

No existing configuration or data-transformation DSL meets all five.

## Starlark

[Starlark](https://bazel.build/rules/language) is Google's configuration DSL, developed for [Bazel](https://bazel.build/) BUILD files and used across several Bazel-adjacent tools. It is deterministic by design, has no I/O in its standard form, and carries a large ecosystem of Python-familiar users. The grammar is a Python subset with mutation disallowed.

Two things rule Starlark out for panproto. It is Turing-complete, with general `while` loops and function recursion, so an evaluation can fail to terminate without reaching a resource bound; the Bazel runner works around this with a hard time limit rather than promising totality. And Starlark has no native types for schemas or instances. A Starlark-based panproto would have to serialise schemas into Starlark dictionaries on every call and deserialise the result, with no pattern-matching support that would make the cycle pleasant to write.

## Dhall

[Dhall](https://dhall-lang.org/) is a total, strongly typed configuration DSL in the style of the simply typed lambda calculus with extensions. It squarely satisfies purity, totality, determinism, and serialisability, and its design is closest to what panproto needs at the level of the core calculus.

The gap is ergonomic rather than foundational. Dhall's primary target is configuration, and its standard library is oriented toward producing JSON, YAML, or other text-formatted outputs. It has no notion of types parameterised by a runtime schema, no record-manipulation primitives designed around panproto's attributed-C-set representation, and no way to compose its user-facing records with panproto's schema identifiers without an external translation layer. A Dhall-based panproto would work; every migration would involve writing serialisers and deserialisers between Dhall records and panproto instances, which the engine then has to verify for correctness separately.

## Nickel

[Nickel](https://nickel-lang.org/), developed by Tweag, is a lazily evaluated configuration language with a contract system for runtime validation. It shares Dhall's purity and determinism, relaxes totality slightly by allowing recursion, and ships a contract system that plays well with JSON-schema-like validations. Panproto's own lens DSL ([`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/)) accepts Nickel as one of its surface syntaxes.

Nickel is a better fit than Dhall for panproto's external-configuration use cases, and panproto uses it exactly there. For the migration engine's internal use, two issues arise. Laziness complicates serialisation: a paused evaluation in Nickel is a thunk graph whose full state is not straightforward to move between processes. And Nickel's contract system is a runtime check rather than a static type check, which means the errors panproto's engine wants to catch at compile time are visible to Nickel only at evaluation time, leaving the engine to run its own static checks after the fact and defeating part of the reason to use the language.

## CUE

[CUE](https://cuelang.org/) is a constraint-based language: values and types are unified in a single lattice, and evaluation means finding the greatest lower bound of a set of constraints. It is pure, deterministic, and its constraint system is genuinely novel for schema validation. Panproto borrows ideas from CUE in its own constraint machinery.

The gap is that CUE is not a function language. Expressing a migration's pushforward computation as a constraint-solving problem, rather than as a function from input to output, would force the engine to inhabit CUE's execution model, which does not fit how the migration engine already treats instances and morphisms. Running CUE for validation and something else for computation is a coherent design; replacing `panproto-expr` wholesale with CUE is not.

## What panproto-expr keeps and loses

The language the engine ended up with is tuned for what remains after the requirements have ruled out the alternatives. Step and depth limits replace Turing-completeness with guaranteed termination. The strict eager reduction strategy replaces laziness, at the cost of forcing eager evaluation in some places where a lazier language would have deferred work, but with the benefit that every intermediate state is serialisable. Native handling of panproto's own types means the language does not double as a general-purpose data-exchange format; [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) and [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) are first-class opaque values.

The losses are real. A user whose pushforward computation genuinely needs a Turing-complete host language cannot express it in `panproto-expr`; such a user has to write a Rust function, compile it through panproto's Rust SDK, and register it with the migration engine as a foreign function. A user who wants to configure their migration declaratively without writing evaluated expressions can use the lens DSL in Nickel or JSON form, which compiles through a separate pipeline. Neither loss is hypothetical; both are what the engine's escape hatches are for.

## Further reading

For the totality argument in a broader context, @turner2004total is foundational. For the lineage of small functional expression languages the `panproto-expr` surface borrows from, @landin1966next's ISWIM paper is the originating reference and @harper2016practical the modern textbook. The documentation for the [`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/) crate shows where panproto does use Nickel in production — for declarative lens specifications — and where it does not.

## Closing

Part IV opens with [Defining a protocol](../protocols/defining.md), which catalogues the specific protocols panproto supports and shows how each fits the framework of Part II.
