# The WebAssembly boundary

The engine is written in Rust, and the non-Rust clients — the TypeScript SDK for browsers and Node, the browser-side of the Python SDK that has since been deprecated in favour of native wheels — cannot share memory with Rust or respect Rust's ownership discipline from the outside. What they can do is call into a [WebAssembly](https://webassembly.org/) module [@haas2017bringing] that holds the Rust values internally and exposes a small API that exchanges opaque handles and serialised data. The module is [`panproto-wasm`](https://docs.rs/panproto-wasm/latest/panproto_wasm/), built with [`wasm-bindgen`](https://rustwasm.github.io/wasm-bindgen/). The present chapter explains the shape of the API and the reasons it takes the shape it does.

## Handles, not pointers

Rust types like [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html) and [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html) carry internal references, lifetimes, and non-trivial invariants. Exposing them directly to a non-Rust caller would require the caller to respect Rust's ownership discipline from outside Rust, which is not a contract WebAssembly can enforce.

The boundary sidesteps the problem by making every Rust value opaque. When a caller constructs a schema through the WASM module, what comes back is a `u32` **handle**, not a pointer. The handle is an index into a slab allocator that lives inside the WASM module; the Rust value it points at never leaves the module. Subsequent calls that operate on the schema pass the handle back in, and the module dereferences it internally to retrieve the Rust value.

The slab allocator is implemented in [`panproto_wasm::handle`](https://docs.rs/panproto-wasm/latest/panproto_wasm/). Each slot is a tagged-union `Object` that can hold any of panproto's major types. Allocation hands out the lowest free index; deallocation returns the index to the free list. Handle reuse is not a problem in practice, since every call that produces a handle returns a fresh one; the caller's code frees handles it is finished with through an explicit `drop_handle` call, not through GC across the boundary.

## MessagePack for data

Values the caller does need to inspect (field values, record contents, error diagnostics) cross the boundary as [MessagePack](https://msgpack.org/) [@messagepack] byte sequences. MessagePack was chosen over [JSON Schema](https://json-schema.org/) [@jsonschema2020] for compactness on large instance payloads and over [Protocol Buffers](https://protobuf.dev/) [@protobuf] for the absence of a pre-negotiated schema: the contents of a MessagePack message need not be declared in a `.proto` file ahead of time. Panproto's own schemas are the things being exchanged, which rules out any representation that would require them to be known to both sides in advance.

Serialisation into MessagePack uses [serde](https://serde.rs/) on the Rust side and [`@msgpack/msgpack`](https://www.npmjs.com/package/@msgpack/msgpack) on the TypeScript side. Both libraries produce the same byte representation for any serde-compatible value. The TypeScript types that model panproto's values are generated from the Rust type definitions through a short build step that [`sdk/typescript`](https://www.npmjs.com/package/@panproto/core) runs as part of its prepublish.

## The entry-point surface

[`panproto-wasm`](https://docs.rs/panproto-wasm/latest/panproto_wasm/) exposes approximately fifty entry points. They cluster into six groups by functional area: schema construction and validation, migration construction and compilation, instance manipulation, lens construction and law-checking, VCS operations (commit, branch, merge), and protocol registration. Each group maps to a submodule of the crate, and each submodule's functions follow the same pattern: the arguments are a combination of `u32` handles and MessagePack byte slices; the return value is either a new handle, a MessagePack byte slice, or a structured error.

A full catalogue of the entry points is in [`sdk/typescript`](https://www.npmjs.com/package/@panproto/core)'s TypeScript definitions, which are the source-of-truth mapping between the JavaScript-visible function names and the Rust functions they bind to.

## Errors

Every entry point returns a `Result<T, PanprotoError>`. A successful call returns the output; a failed call returns a structured error with a kind discriminator and a human-readable message, serialised as MessagePack. The TypeScript wrapper deserialises errors into JavaScript `Error` subclasses that preserve the kind and message, so a caller can pattern-match on the specific failure (a parse error, a type-check error, an existence-check failure, a lens-law violation) without parsing strings.

## Sizing

The WASM module compiles to about three megabytes after [`wasm-opt`](https://github.com/WebAssembly/binaryen) passes and gzip compression. Tree-sitter grammars are the dominant size contributor; a build that disables [`panproto-parse`](https://docs.rs/panproto-parse/latest/panproto_parse/) through the feature flags comes down to about one megabyte. Both sizes are within the budget a typical web application allocates for a first-party module, and the lazy-load pattern (load the module only when a user action requires it) is the common deployment strategy.

## What the TypeScript SDK adds on top

The TypeScript SDK ([`@panproto/core`](https://www.npmjs.com/package/@panproto/core), covered in [the TypeScript SDK chapter](./typescript.md)) layers three things on the WASM boundary: ergonomic wrappers that hide handle manipulation behind ordinary JavaScript object lifetimes; MessagePack serialisation and deserialisation with type-safe bindings generated from the Rust types; and a fluent builder API for common operations (schema construction, migration composition) that would otherwise require many discrete WASM calls.

## Further reading

@haas2017bringing is the original paper introducing WebAssembly and the clearest source on what the platform guarantees and what it does not. @w3c2019wasm is the normative W3C specification. For the Rust-specific machinery the panproto-wasm crate uses, the [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) documentation is authoritative; @wasmbindgen is the BibTeX entry for the project itself. For MessagePack's wire format, @messagepack is the specification.

## Closing

The next chapter, [the Rust SDK](./rust.md), turns to the cross-language core: [`panproto-core`](https://docs.rs/panproto-core/latest/panproto_core/), the facade crate that re-exports every subsystem's public API and gates the optional ones behind Cargo features.
