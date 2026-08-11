# ``PanprotoStructural``

Schemas, instances, migrations, chains, theories, and expressions as Swift
values, together with the CBOR codec they cross the C ABI as.

## Overview

Every payload panproto's C ABI reads or writes is CBOR produced by
[`ciborium`](https://docs.rs/ciborium) driven by
[`serde`](https://serde.rs/). This module is the Swift side of that
agreement: one `Codable` type for each Rust type the ABI names, and an
encoder and a decoder written against `serde`'s data model rather than a
general-purpose one.

Nothing here links the engine, and the package graph enforces it: this
target depends on no FFI module. A tool that only rewrites schemas can
link it alone, and it will still find the algebra it needs. Migration
specifications compose, protolens chains concatenate and fuse, optic
kinds fold, schemas diff against each other and answer whether they have
one shape between them, and expressions render back to the surface
syntax they were written in. None of that starts an engine. See
<doc:TheValueLayer> for what that buys and where it stops.

The types are also the vocabulary the engine-backed `Panproto` module
speaks. Every domain method there takes and returns the values declared
here, encoding on the way in and decoding on the way out, so a host that
holds a ``Schema`` can hand it to an engine call, persist it, diff it
against another, or send it somewhere else without translating it first.

Two conventions run through the whole module and are worth reading once
rather than per type.

First, a Rust map whose key is not a string cannot be a CBOR map with
text keys, so the engine writes it as an array of pairs. Those fields are
Swift dictionaries here, and ``WirePair`` and ``WireMap`` are what carry
them across; a type holding one declares a custom `encode(to:)` for the
purpose, ordered by the encoded key so that one value always produces one
byte string.

Second, a Rust enum reaches the wire externally tagged: a unit variant is
a text string, and every other variant is a one-entry map keyed by the
variant name. The Swift enums reproduce that spelling exactly, which is
why several of them carry hand-written coding rather than a synthesized
conformance.

```swift
func catalogRoundTrip() throws -> Schema {
    var schema = Schema(protocol: "atproto")
    schema.addVertex(id: "post", kind: "record")
    schema.addVertex(id: "post.text", kind: "string")
    schema.addEdge(src: "post", tgt: "post.text", kind: "prop", name: "text")
    schema.addEntry("post")

    let bytes = try CBOREncoder().encode(schema)
    return try CBORDecoder().decode(Schema.self, from: bytes)
}
```

## Topics

### Articles

- <doc:TheValueLayer>
- <doc:TheCBORCodec>

### Schemas

- ``Schema``
- ``Vertex``
- ``Edge``
- ``HyperEdge``
- ``Constraint``
- ``Variant``
- ``Ordering``
- ``RecursionPoint``
- ``Span``
- ``UsageMode``
- ``CoercionSpec``
- ``SchemaMetadata``
- ``EdgeShape``

### Instances and values

- ``Instance``
- ``Node``
- ``NodeShape``
- ``InstanceArc``
- ``Fan``
- ``Value``
- ``FieldPresence``
- ``InstanceQuery``
- ``QueryMatchElement``

### Migrations

- ``Migration``
- ``CompiledMigration``
- ``ExistenceReport``
- ``ExistenceError``
- ``CoverageReport``
- ``BuildOp``

### Value-level transforms

- ``FieldTransform``
- ``FieldTransformBranch``
- ``TermAssignment``
- ``TermBranch``
- ``TermScope``

### Lenses and complements

- ``Complement``
- ``NodePair``
- ``GetRecordEnvelope``
- ``ComplementSpec``
- ``ComplementKind``
- ``CapturedField``
- ``DefaultRequirement``
- ``MergerSpec``
- ``PolicySpec``
- ``LawCheckResult``

### Protolens chains

- ``ProtolensChain``
- ``ProtolensStepInfo``
- ``ElementaryStep``
- ``OpticKind``
- ``ValueKindSlug``
- ``StrategyTag``
- ``DiffSpec``
- ``AutoLensCandidates``
- ``LensCandidate``
- ``LensCandidateStep``
- ``CoerceProposal``

### Diffing and compatibility

- ``SchemaDiff``
- ``StructuralDiff``
- ``EdgeDiff``
- ``StructuralKindChange``
- ``KindChange``
- ``ConstraintDiff``
- ``ConstraintChange``
- ``HyperEdgeChange``
- ``VariantChange``
- ``RecursionPointChange``
- ``SpanChange``
- ``Classification``
- ``CompatReport``
- ``BreakingChange``
- ``NonBreakingChange``

### Expressions

- ``Expr``
- ``Literal``
- ``Pattern``
- ``BuiltinOp``
- ``CheckOutput``
- ``LiteralEnv``
- ``ModelValueEnv``
- ``TypecheckContext``
- ``ConstraintPairList``

### Theories

- ``Theory``
- ``Sort``
- ``SortExpr``
- ``SortKind``
- ``SortParam``
- ``SortClosure``
- ``Operation``
- ``OperationInput``
- ``Implicit``
- ``Term``
- ``CaseBranch``
- ``Equation``
- ``DirectedEquation``
- ``ConflictPolicy``
- ``ConflictStrategy``
- ``ValueKind``
- ``CoercionClass``

### Models and theory morphisms

- ``ModelValue``
- ``ModelValueList``
- ``SortInterpMap``
- ``TheoryMorphism``
- ``OpAssignment``
- ``MorphismCheckResult``
- ``FreeModelConfigSpec``
- ``ViolationList``

### Protocols

- ``ProtocolSpec``
- ``CompositionSpec``
- ``CompositionStep``
- ``EdgeRule``

### Homomorphisms and graphs

- ``MorphismSearchOptions``
- ``FoundMorphism``
- ``SchemaMorphism``
- ``GraphEdge``
- ``PathResult``
- ``FiberAtAnchor``
- ``FiberDecomposition``
- ``StalenessReport``

### Version-control records

- ``VcsObjectID``
- ``HeadState``
- ``VcsStatus``
- ``VcsAddResult``
- ``VcsCommitResult``
- ``VcsLogResult``
- ``LogEntry``
- ``BranchInfo``
- ``VcsBranchResult``
- ``VcsOpResult``
- ``VcsMergeResult``
- ``VcsDiffResult``
- ``StashEntry``
- ``VcsStashResult``
- ``VcsStashPopResult``
- ``BlameReport``

### Names

- ``Name``
- ``Ident``
- ``ScopeTag``
- ``NameSite``
- ``SiteRename``

### Projects and parsed sources

- ``ProtocolMap``
- ``ProtocolNames``
- ``GitImportResult``

### The CBOR codec

- ``CBOREncoder``
- ``CBORDecoder``
- ``CBORValue``
- ``CBORError``

### Crossing the boundary

- ``RawStatus``
- ``ErrorEnvelope``
- ``WirePair``
- ``WireTriple``
- ``WireMap``
- ``UInt32KeyedMap``
