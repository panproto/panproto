# TypeScript SDK reference

The [TypeScript](https://www.typescriptlang.org/) package is [`@panproto/core`](https://www.npmjs.com/package/@panproto/core). It requires Node.js 20 or later when run under Node and loads the engine through WebAssembly.

```sh
npm install @panproto/core
```

## Initialization

```ts
import { Panproto } from '@panproto/core';

const panproto = await Panproto.init();
```

The initialization signature is:

```ts
static init(input?: string | URL | WasmGlueModule): Promise<Panproto>
```

With no argument, `init` loads the glue and WASM binary bundled with the package. A URL identifies the wasm-bindgen JavaScript glue module, rather than the `.wasm` file itself. Bundlers may instead pass a pre-imported `WasmGlueModule`.

## Principal exports

The generated declaration file in the package is the signature authority. The table below is an index into that surface.

| Export | Contract |
|---|---|
| `Panproto` | Engine initialization and convenience methods for protocols, parsing, diffs, migrations, lenses, instance I/O, VCS, and data sets |
| `Protocol` | Protocol specification and `schema(): SchemaBuilder` |
| `SchemaBuilder` | Immutable builder. Each mutation returns a new builder, and `build()` returns `BuiltSchema`. |
| `BuiltSchema` | Engine-backed schema with structural metadata, normalization, and validation |
| `MigrationBuilder` | Immutable vertex, edge, and resolver mapping builder |
| `CompiledMigration` | `lift`, complement-carrying `get` and `put`, plus JSON convenience methods |
| `ProtolensChainHandle`, `LensHandle`, `SymmetricLensHandle` | Chain construction, lens execution, composition, and law checks |
| `Instance`, `IoRegistry` | Instance values and protocol-specific parse or emit operations |
| `FullDiffReport`, `CompatReport`, `ValidationResult` | Diff, compatibility, and validation results |
| `TheoryHandle`, `TheoryBuilder` | GAT construction, colimits, and morphism operations |
| `Repository`, `DataSetHandle` | In-memory VCS and data-versioning resources |
| `parseExpr`, `evalExpr`, `formatExpr`, `ExprBuilder` | Expression parsing, evaluation, formatting, and construction |
| `executeQuery`, `fiberAt`, `fiberDecomposition`, `polyHom` | Instance-query and structural exports. `executeQuery` has the boundary mismatch described below. |

The source export list is [`bindings/typescript/src/index.ts`](https://github.com/panproto/panproto/blob/main/bindings/typescript/src/index.ts). Package consumers should import from `@panproto/core`, since files under `src` are not package subpath exports.

## Selected signatures

```ts
class SchemaBuilder {
  vertex(id: string, kind: string, options?: VertexOptions): SchemaBuilder;
  edge(src: string, tgt: string, kind: string, options?: EdgeOptions): SchemaBuilder;
  constraint(vertexId: string, sort: string, value: string): SchemaBuilder;
  build(): BuiltSchema;
}

class MigrationBuilder {
  map(srcVertex: string, tgtVertex: string): MigrationBuilder;
  mapEdge(srcEdge: Edge, tgtEdge: Edge): MigrationBuilder;
  resolve(srcKind: string, tgtKind: string, resolvedEdge: Edge): MigrationBuilder;
  compile(): CompiledMigration;
}

class CompiledMigration {
  lift(record: unknown): LiftResult;
  get(record: unknown): GetResult;
  put(view: unknown, complement: Uint8Array): LiftResult;
}
```

`LiftResult.data` is `unknown`. `GetResult` contains `view: unknown` and `complement: Uint8Array`. The complement must be passed back unchanged unless an operation explicitly returns a replacement.

If the compiled mapping has schema direction \(S\to T\), `lift` accepts an \(S\)-record and returns the surviving fragment as a \(T\)-record. It calls Rust's restrict-based `lift_wtype`. It is neither the left Kan extension \(\Sigma_F\) nor precomposition \(\Delta_F\). `get` has the same source-to-target direction and additionally captures the complement. `put` takes a \(T\)-view and that complement and reconstructs an \(S\)-record.

## Resource ownership

Engine-backed wrappers implement `Disposable`. This includes `Protocol`, `BuiltSchema`, `CompiledMigration`, the three lens-handle classes, `IoRegistry`, `TheoryHandle`, `Repository`, and `DataSetHandle`. Dispose each owned wrapper after its last use, preferably with an explicit resource-management scope:

```ts
using schema = protocol.schema().vertex('root', 'object').build();
```

Disposal is idempotent. Accessing a disposed handle raises `WasmError`. A `FinalizationRegistry` frees a leaked handle as a fallback, but collection time is nondeterministic. Disposing `Panproto` releases its cached `Protocol` objects. It does not own every schema, migration, lens, registry, repository, or data-set wrapper created from it.

`Instance`, result objects, and `SpanResponse` are plain JavaScript data and do not implement `Disposable`.

## Span search

```ts
span(
  from: BuiltSchema,
  to: BuiltSchema,
  hints?: Readonly<Record<string, string>>,
): SpanResponse
```

The optional hints are fixed source-to-target vertex mappings. `SpanResponse` contains `apex_vertices`, `apex_edges`, `vertex_map`, `quality`, `quality_bounds`, `apex_coverage`, `proven_optimal`, `is_total`, and `apex_digest`. It contains no engine handle.

The WASM surface exposes span search but not the Rust total-morphism functions `find_morphisms` and `find_best_morphism`. Use `span.is_total` when a caller needs to know whether the returned apex covers the complete source.

## Boundary limits

The SDK passes structured payloads through the WASM layer and stores live engine resources behind integer handles. The package does not expose the Rust `panproto-parse` full-AST registry, multi-file `panproto-project` builder, or `panproto-git` bridge. Schema-document and schema-source parsers available through `Panproto.parseSchemaDocument` and `Panproto.parseSchemaSource` are separate from that full-AST surface.

The current `executeQuery` wrapper does not match the current WASM entry point. TypeScript sends only a query and instance, while Rust requires query, instance, and schema payloads. The TypeScript wire fields also use `projection`, `groupBy`, and `nodeId`, while the Rust query types use `project`, `group_by`, and `node_id`. Treat `executeQuery` as unavailable until the binding and WASM signatures are aligned.

## See also

- [Install the TypeScript SDK](../how-to/install/typescript.md)
- [Define a schema from TypeScript](../how-to/define-schema/typescript.md)
- [WASM boundary](../explanation/architecture.md#wasm-boundary)
- [Find a span between two schemas](../how-to/spans.md)
