# panproto-protocols

Built-in protocol definitions for panproto.

Each protocol is defined by a schema theory [GAT](https://ncatlab.org/nlab/show/generalized+algebraic+theory) and an instance theory GAT, composed via [colimit](https://ncatlab.org/nlab/show/colimit) from reusable building-block theories. This crate includes parsers for each protocol's native schema format ([Lexicon](https://atproto.com/specs/lexicon), DDL, [`.proto`](https://protobuf.dev/programming-guides/proto3/), [SDL](https://graphql.org/learn/schema/), [JSON Schema](https://json-schema.org/)).

## Supported Protocols

| Protocol | Schema type | Instance type | Parser |
|----------|-------------|---------------|--------|
| [ATProto](https://atproto.com/) | Constrained multigraph | [W-type](https://ncatlab.org/nlab/show/W-type) | [Lexicon](https://atproto.com/specs/lexicon) JSON |
| SQL | [Hypergraph](https://en.wikipedia.org/wiki/Hypergraph) | [Set-valued functor](https://ncatlab.org/nlab/show/functor) | DDL |
| [Protobuf](https://protobuf.dev/) | Simple graph | Flat | `.proto` files |
| [GraphQL](https://graphql.org/) | Typed graph | W-type | [SDL](https://graphql.org/learn/schema/) |
| [JSON Schema](https://json-schema.org/) | Constrained multigraph | W-type | JSON Schema |

## API

| Item | Description |
|------|-------------|
| `atproto` | ATProto protocol definition, Lexicon parser, and theory registration |
| `sql` | SQL protocol definition, DDL parser, and theory registration |
| `protobuf` | Protobuf protocol definition, `.proto` parser, and theory registration |
| `graphql` | GraphQL protocol definition, SDL parser, and theory registration |
| `json_schema` | JSON Schema protocol definition, parser, and theory registration |
| `theories` | Shared component theory definitions (building-block GATs) |
| `ProtocolError` | Error type for protocol operations |

## Example

```rust,ignore
use panproto_protocols::atproto;

let protocol = atproto::protocol();
let schema = atproto::parse_lexicon(&lexicon_json)?;
```

## License

[MIT](../../LICENSE)
