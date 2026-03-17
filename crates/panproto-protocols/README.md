# panproto-protocols

[![crates.io](https://img.shields.io/crates/v/panproto-protocols.svg)](https://crates.io/crates/panproto-protocols)
[![docs.rs](https://docs.rs/panproto-protocols/badge.svg)](https://docs.rs/panproto-protocols)

Built-in protocol definitions for panproto.

Each of the 76 protocols is defined by a schema theory [GAT](https://ncatlab.org/nlab/show/generalized+algebraic+theory) and an instance theory GAT, composed via [colimit](https://ncatlab.org/nlab/show/colimit) from 27 reusable building-block theories organized in six groups. This crate includes parsers for each protocol's native schema format.

## Protocols

| Protocol | Schema type | Instance type | Parser |
|----------|-------------|---------------|--------|
| [ATProto](https://atproto.com/) | Constrained multigraph | W-type | Lexicon JSON |
| SQL | Hypergraph | Set-valued functor | DDL |
| [Protobuf](https://protobuf.dev/) | Simple graph | Flat | `.proto` files |
| [GraphQL](https://graphql.org/) | Typed graph | W-type | SDL |
| [JSON Schema](https://json-schema.org/) | Constrained multigraph | W-type | JSON Schema |
| ...and 71 more | | | |

See the [protocol catalog](https://panproto.dev/tutorial/appendices/D-protocol-catalog.html) for the full list.

## Building-Block Theories (27)

Protocols compose their schema and instance theories from reusable building blocks via colimit:

| Group | Theories |
|-------|----------|
| A: Core graphs | ThGraph, ThSimpleGraph, ThHypergraph, ThConstraint, ThMulti, ThInterface |
| B: Instance shapes | ThWType, ThMeta, ThFunctor, ThFlat |
| C: Structure | ThOrder, ThCoproduct, ThRecursion, ThSpan, ThCospan, ThPartial, ThLinear, ThNominal |
| D: Symmetry | ThReflexiveGraph, ThSymmetricGraph |
| E: Process | ThPetriNet, ThCausal |
| F: Composition | ThGraphInstance, ThAnnotation, ThOperad, ThTracedMonoidal, ThSimplicial |

## API

| Item | Description |
|------|-------------|
| `atproto` / `sql` / `protobuf` / `graphql` / `json_schema` | Core protocol modules with definitions, parsers, and theory registration |
| `theories` | All 27 building-block theory definitions |
| `ProtocolError` | Error type |

## Example

```rust,ignore
use panproto_protocols::atproto;

let protocol = atproto::protocol();
let schema = atproto::parse_lexicon(&lexicon_json)?;
```

## License

[MIT](../../LICENSE)
