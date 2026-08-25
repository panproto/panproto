# panproto-protocols

[![crates.io](https://img.shields.io/crates/v/panproto-protocols.svg)](https://crates.io/crates/panproto-protocols)
[![docs.rs](https://docs.rs/panproto-protocols/badge.svg)](https://docs.rs/panproto-protocols)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

Protocol definitions and schema parsers for semantic data formats.

## Coverage

The crate contains 54 format-specific schema parsers exposed through two generic
dispatch functions. `parse_schema_document` accepts the 43 JSON-document protocols.
`parse_schema_source` accepts the 11 text-source protocols. The latter group includes
SQL DDL, GraphQL SDL, Protobuf, CDDL, CQL, Cypher, Redis schema text, ASN.1, Bond,
FlatBuffers, and CoNLL-U. The `raw_file` module is a separate fallback for project
assembly.

The protocol modules cover annotation formats, API descriptions, configuration,
data-schema formats, data-science schemas, databases, domain formats, serialization
IDLs, and web or document formats. Programming-language grammars are provided by
`panproto-parse`, not this crate.

Only ATProto currently has generic bundle dispatch through `parse_schema_bundle` and
`parse_schema_bundle_project`. Other protocols must be parsed one document at a time
unless their module exposes a separate specialized API.

## Theories

Each `Protocol` names a schema theory and an instance theory. `theories` provides 11
reusable constructors: `ThGraph`, `ThConstraint`, `ThMulti`, `ThWType`, `ThMeta`,
`ThSimpleGraph`, `ThHypergraph`, `ThInterface`, `ThFunctor`, `ThFlat`, and
`ThGraphInstance`. Individual protocols may register additional composed theories.

The generalized-algebraic-theory terminology follows
[Cartmell (1986)](https://doi.org/10.1016/0168-0072(86)90053-9). The use of colimits
to combine theory presentations follows the structured-specification line begun by
[Burstall and Goguen (1977)](https://www.ijcai.org/Proceedings/77-2/Papers/095.pdf).

## Example

```rust,ignore
use panproto_protocols::atproto;

let protocol = atproto::protocol();
let document: serde_json::Value = serde_json::from_slice(&lexicon_bytes)?;
let schema = atproto::parse_lexicon(&document)?;
```

## Main entry points

| Item | Purpose |
|------|---------|
| `parse_schema_document` | Dispatch a JSON value by protocol name |
| `parse_schema_source` | Dispatch source text by protocol name |
| `parse_schema_bundle` | Parse a supported cross-document bundle |
| `parse_schema_bundle_project` | Retain per-file ATProto provenance |
| `bundle_parser_protocols`, `bundle_project_protocols` | Report bundle support |
| `theories` | Reusable theory constructors and registration functions |
| `ProtocolError` | Parser and emitter errors |

Each format module also exposes its own `protocol()`, parser, emitter, and theory
registration functions. See [docs.rs](https://docs.rs/panproto-protocols) for those
module-specific signatures.

## License

[MIT](../../LICENSE)
