# The TypeScript SDK

<!-- lm-disclaimer -->
> **Disclaimer.** The content of this page is largely LM-generated.
> It was written as a stopgap to make the panproto system legible while we work
> through the book verifying and editing the content by hand. When a chapter
> has been verified or edited by a human, the parts that were verified or
> edited will be noted at the head of the chapter.

The TypeScript SDK, published on npm as [`@panproto/core`](https://www.npmjs.com/package/@panproto/core), wraps the WebAssembly boundary developed in [the previous chapter](./wasm-boundary.md) in a typed, ergonomic API for JavaScript and TypeScript callers. What the wrapping adds is threefold: ordinary JavaScript object lifetimes instead of manual handle management; MessagePack serialisation hidden inside method calls rather than exposed at the API level; and a fluent builder API that bundles the many discrete WASM calls of schema and migration construction into a single round-trip.

The source lives in `sdk/typescript/` in the panproto repository.

## Initialisation

The WASM module needs to be loaded and initialised before any panproto call. The SDK handles this through a top-level `init` function that takes either a `URL`, a `Response`, or a `BufferSource` (depending on the environment) and returns a promise that resolves when the module is ready.

```typescript
import { init, Schema } from "@panproto/core";

await init(); // loads the bundled WASM from the package's own assets
const schema = await Schema.fromLexicon(lexiconJson);
```

*Listing 8.1: Initialising the SDK from the default bundled WASM. The bundled path is a small helper that resolves to the WASM file shipped inside the npm package; callers with specific load strategies (CDN, inline data URI, custom caching) may pass an explicit URL.*

The module is loaded once per process. Subsequent `init` calls are idempotent; the SDK caches the initialised module globally and re-uses it. A caller that needs multiple independent panproto instances (uncommon; mostly for testing) may explicitly construct fresh module contexts, but the default one-module-per-process model fits every production case.

## Typed API surface

Every Rust type with a stable API has a TypeScript class that wraps it. The wrapping is thin: each class holds a `u32` handle from the WASM module's slab allocator and exposes methods that dispatch to the underlying WASM entry points, with MessagePack serialisation and deserialisation applied transparently.

The top-level classes are [`Schema`](https://docs.rs/panproto-schema/latest/panproto_schema/schema/struct.Schema.html), [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/migration/struct.Migration.html), [`Instance`](https://docs.rs/panproto-inst/latest/panproto_inst/), [`Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/), [`Protocol`](https://docs.rs/panproto-schema/latest/panproto_schema/protocol/struct.Protocol.html), and [`Repository`](https://docs.rs/panproto-vcs/latest/panproto_vcs/) (the VCS entry point). Each class has a small set of methods covering construction, validation, inspection, and the specific operations the Rust type supports. The method names match the Rust ones where possible, with camelCase substituted for snake_case per TypeScript convention.

## Handle lifecycle

JavaScript has no manual memory management, but the WASM module's handles need to be freed explicitly: a handle that is never released occupies a slot in the module's allocator indefinitely. The SDK bridges this gap through [FinalizationRegistry](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/FinalizationRegistry), which calls a cleanup function when a JavaScript object is garbage-collected.

Every SDK class registers its handle with the registry on construction. When the JavaScript object becomes unreachable, the registry invokes a callback that releases the handle back to the WASM module's allocator. The pattern relies on the JavaScript GC running in a timely fashion; for long-running services that want deterministic release, every class also exposes an explicit `dispose()` method that frees the handle immediately. The TypeScript 5.2 `using` keyword picks up `dispose()` automatically.

```typescript
{
  using schema = await Schema.fromLexicon(lexiconJson);
  // ... work with schema ...
} // schema.dispose() is called automatically when the block exits
```

*Listing 8.2: Deterministic handle release through the TypeScript 5.2 `using` keyword.*

## The fluent builder

Schema and migration construction from raw primitives would require many WASM round-trips (one per sort, one per operation, one per equation). The SDK has a fluent builder for these construction cases that accumulates the full declaration in TypeScript and sends it to the module as a single MessagePack-encoded payload.

```typescript
const schema = await Schema.builder(protocol)
  .sort("Person")
  .sort("Address")
  .operation("name", ["Person"], "String")
  .operation("livesAt", ["Person"], "Address")
  .build();
```

*Listing 8.3: Schema construction through the fluent builder. The builder accumulates declarations locally and invokes the WASM module once at `.build()` time.*

The builder pattern is also available on [`Migration`](https://docs.rs/panproto-mig/latest/panproto_mig/) and on [`Lens`](https://docs.rs/panproto-lens/latest/panproto_lens/). For bulk operations, the amortised cost of a single WASM round-trip is substantially lower than the per-operation cost of dispatching each primitive through a separate call.

## Type generation

The TypeScript types that represent panproto values (record shapes, pattern-matchable enums, error kinds) are generated from the Rust type definitions at publish time. The generator runs [`tsify`](https://github.com/madonoharu/tsify) against the WASM crate to produce a `.d.ts` file that reflects the public types with full generic parameters and discriminated unions. A caller's TypeScript compiler sees exactly the same type surface the Rust code exposes, minus the lifetimes Rust has and TypeScript does not.

The generation step runs as part of the npm package's build script. Consumers of `@panproto/core` do not need to run it themselves; they receive the pre-generated definitions with the package.

## Error handling

Rust errors cross the boundary as MessagePack payloads with a discriminant and a message. The SDK deserialises them into JavaScript `Error` subclasses (`PanprotoParseError`, `PanprotoTypeCheckError`, `PanprotoLensLawError`, and so on) whose prototype chain preserves the Rust error variant information. A caller can check `error instanceof PanprotoLensLawError` to match on a specific failure; the classes are exported from the top-level SDK module for this purpose.

Every method that can fail returns a `Promise<T>` that rejects with the appropriate error subclass. The async surface is the natural one for a WASM-backed API and composes with the rest of modern JavaScript's async machinery.

## Closing

The next chapter, [the Python SDK](./python.md), covers the analogous wrapper for Python. Since panproto ships native PyO3 wheels (the pure-Python WASM-backed SDK having been deprecated), the Python story differs from the TypeScript one in several ways worth spelling out.
