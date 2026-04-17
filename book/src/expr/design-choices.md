# Why bounded pure evaluation

The language of [Syntax and semantics](./syntax-semantics.md), bounded by the step and depth limits of [Totality and termination](./totality.md), is one choice among several small-DSL candidates panproto could have adopted. This chapter walks through the alternatives the design considered and the reasons the current shape was retained.

The chapter is short and opinionated. It exists so that a developer who reads [`panproto-expr`](https://docs.rs/panproto-expr/latest/panproto_expr/) and wonders "why this rather than Starlark?" has a specific answer to point at. The short version: panproto needs a language that is pure, bounded, deterministic, serializable, and capable of pattern-matching over panproto's own schema-indexed types, and no existing candidate satisfies all five.

## The five requirements

Panproto's migration engine embeds a language for one job. The closest-fit prior art is the object-theoretic typed calculus of @abadicardelli1996theory and the family of small configuration-oriented DSLs below; all of them share an ancestor in @landin1966next's ISWIM. At each site of a field-transform or pushforward declaration, the engine needs to evaluate a user-written expression that consumes some values visible at the site (fields of the current record, possibly a schema handle, possibly an instance value) and produces a value to place at the target. The evaluation happens inside the engine's own compile stage, outside of any user-controlled runtime, and its result is an input to the lift function that will run against every record in the input instance.

Five requirements follow from this context.

1. *Purity.* The language cannot perform I/O, since the compile stage runs in contexts where I/O is not available or not desired.
2. *Boundedness.* Every evaluation must terminate within a configurable budget; the engine cannot afford to block on a user expression that happens to loop.
3. *Determinism.* Two evaluations of the same expression on the same input must produce the same output. The engine uses expression evaluation as a pure component of a larger functorial computation and cannot tolerate non-determinism.
4. *Serializability.* An in-flight evaluation must be suspendable and resumable across process boundaries, since panproto's batch-migration tooling parallelises evaluation across workers.
5. *Native support for panproto types.* The language must be able to inspect, construct, and pattern-match on schemas and instances without serialising them through an external format on every operation.

No existing configuration or data-transformation DSL meets all five.

## Starlark

[Starlark](https://bazel.build/rules/language) [@starlark] is Google's configuration DSL, developed for [Bazel](https://bazel.build/) BUILD files and used in several Bazel-adjacent tools. Starlark is deterministic by design, has no I/O in its standard form, and has a large ecosystem of Python-familiar users. Its grammar is a Python subset with mutation disallowed.

Two things rule Starlark out. It is Turing-complete, with general `while` loops and function recursion, so an evaluation can fail to terminate without reaching a resource bound; the Bazel runner works around this with a hard time limit but does not promise totality. And it has no native types for panproto's schemas or instances. A Starlark-based panproto would need to serialise schemas into Starlark dictionaries on every call and deserialise the result, and the grammar has no pattern-matching that would make this pleasant to write.

## Dhall

[Dhall](https://dhall-lang.org/) [@dhalllang] is a total, strongly typed configuration DSL in the style of the simply typed lambda calculus with extensions. Dhall satisfies the purity, totality, determinism, and serialisability requirements squarely. Its design is closest to what panproto needs at the level of the core calculus.

The gap is ergonomic. Dhall's primary target is configuration, and its standard library is oriented toward producing JSON, YAML, or other text-formatted outputs. It has no notion of types parameterised by a runtime schema, no record-manipulation primitives designed around panproto's attributed-C-set representation, and no way to compose its user-facing records with panproto's schema identifiers without an external translation layer. A Dhall-based panproto would work, but every migration would involve writing serialisers and deserialisers between Dhall records and panproto instances, which the engine then has to verify for correctness separately.

## Nickel

[Nickel](https://nickel-lang.org/) [@nickellang], developed by Tweag, is a lazily evaluated configuration language with a contract system for runtime validation. It shares Dhall's purity and determinism, relaxes totality slightly by allowing recursion, and ships a contract system that plays well with JSON-schema-like validations. Panproto's own lens DSL ([`panproto-lens-dsl`](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/)) accepts Nickel as one of its surface syntaxes.

Nickel is a better fit than Dhall for panproto's external-configuration use cases, and panproto uses it exactly there. For the migration engine's internal use, two issues arise. Laziness complicates serialisation: a paused evaluation in Nickel is a thunk graph whose full state is not straightforward to move between processes. And Nickel's contract system, which is a runtime check rather than a static type check, doesn't catch the kinds of errors panproto's migration engine needs to catch at compile time, so the engine ends up running its own static checks on Nickel expressions after the fact, which defeats part of the reason to use the language in the first place.

## CUE

[CUE](https://cuelang.org/) [@cuelang] is a constraint-based language: values and types are unified in a single lattice, and evaluation means finding the greatest lower bound of a set of constraints. CUE is pure, deterministic, and its constraint system is genuinely novel for schema validation. Panproto borrows ideas from CUE in its own constraint machinery.

The gap is that CUE is not a function language. Expressing a migration's pushforward computation as a constraint-solving problem rather than as a function from input to output would force the engine to inhabit CUE's execution model, which does not fit how the migration engine already treats instances and morphisms. Running CUE for validation and something else for computation is a coherent design; replacing `panproto-expr` wholesale with CUE is not.

## What panproto-expr keeps and loses

The language the engine ended up with is specifically tuned for what remains after the requirements have ruled out the alternatives. Step and depth limits bound it, replacing Turing-completeness with guaranteed termination. Serialisability across process boundaries demanded a strict eager reduction strategy rather than the laziness that Nickel (for instance) relies on. Giving the language native handling of panproto's own types meant accepting that it would not double as a general-purpose data-exchange format; [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) and [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/) are first-class opaque values.

The losses are real. A user whose pushforward computation genuinely needs a Turing-complete host language cannot express it in panproto-expr; such a user must instead write a Rust function, compile it through panproto's Rust SDK ([The Rust SDK](../sdks/rust.md)), and register it with the migration engine as a foreign function. A user who wants to configure their migration declaratively without writing evaluated expressions at all can use the [lens DSL](https://docs.rs/panproto-lens-dsl/latest/panproto_lens_dsl/) in Nickel or [JSON](https://json-schema.org/) form, which compiles through a separate pipeline.

## Closing

Part III closes here. Part IV, which opens with [Defining a protocol](../protocols/defining.md), catalogues the specific protocols panproto supports (ATProto, Avro, a relational case, FHIR as a document case) and shows how each is expressed as an instance of the constructions developed in Part II.

<!--
STATUS: Design choices chapter drafted.

CITATIONS:
  - Google's Starlark specification (link in prose).
  - Dhall language reference (link in prose).
  - Nickel language reference (link in prose).
  - CUE language reference (link in prose).
  - Turner 2004 on total functional programming (pending BibTeX).
-->
