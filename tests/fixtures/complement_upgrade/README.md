# Legacy complement fixtures

Complements written by panproto 0.74.4, before a complement carried an identity
envelope. Each one binds itself to its lens only through `source_fingerprint`, a
64-bit hash of the source schema's sorted vertex names and its edge count. That
hash was taken with the standard library's `DefaultHasher`, which hashes the edge
count as a `usize` in native byte order, so a complement produced by a 64-bit host
and one produced by the wasm32 build carry different fingerprints for the same
schema. Both are here.

## Cases

Every case directory holds the lens inputs and the complement its `get` produced.

| case            | what the lens does                                                       |
|-----------------|--------------------------------------------------------------------------|
| `drop_field`    | drops the `subtitle` field of a `post` record                            |
| `rename_vertex` | renames `post` to `article` and the `text` edge to `body`                |
| `contract`      | drops the intermediate `post.meta` object and contracts `lang` onto `post` |
| `ordered_list`  | drops `note` children interleaved with the `tag` children it keeps       |

Inputs, in each case directory:

- `protocol.json` / `protocol.msgpack`: the protocol both schemas are built in;
- `src_ops.json` / `src_ops.msgpack`, `tgt_ops.json` / `tgt_ops.msgpack`: the schema
  build operations, in the form the WASM `build_schema` entry point reads;
- `migration.json` / `migration.msgpack`: the `Migration` compiled into the lens;
- `instance.json` / `instance.msgpack`: the source `WInstance`.

The `.msgpack` inputs are named msgpack and are what the wasm32 producer reads.

## Complements

| file                    | encoding                                   | producer                                   |
|-------------------------|--------------------------------------------|--------------------------------------------|
| `native.pos.msgpack`    | `rmp_serde::to_vec` (positional, the VCS form) | 0.74.4, 64-bit host                    |
| `native.named.msgpack`  | `rmp_serde::to_vec_named` (the WASM/TS form) | 0.74.4, 64-bit host                      |
| `native.cbor`           | `ciborium` via `panproto_c::canonical::encode` (the Swift/Haskell form) | 0.74.4, 64-bit host |
| `native.json`           | `serde_json::to_vec` (the Python form)      | 0.74.4, 64-bit host                        |
| `wasm32.named.msgpack`  | `rmp_serde::to_vec_named`                  | 0.74.4 wasm32 build, through `get_record`  |
| `wasm32.pos.msgpack`, `wasm32.cbor`, `wasm32.json` | as above                | the wasm32 complement, transcoded natively |

`fingerprints.json` records the `source_fingerprint` of each producer's complement.

| case            | 64-bit fingerprint       | wasm32 fingerprint        |
|-----------------|--------------------------|---------------------------|
| `contract`      | `8537667589647975768`    | `14067926949814075051`    |
| `drop_field`    | `2770883639970332002`    | `16649305309498269837`    |
| `ordered_list`  | `5568901458156004314`    | `8140304790023999500`     |
| `rename_vertex` | `5545373909098406705`    | `9223972107605509029`     |

The two producers' complements differ in `source_fingerprint` and nowhere else.

## Compatibility

| encoding        | producer version | readable by 0.75 | writable by 0.75 in legacy form | 0.74 reading 0.75 output |
|-----------------|------------------|------------------|---------------------------------|--------------------------|
| positional msgpack | 0.74.4        | yes              | no                              | a stamped complement is unreadable: 0.74 rejects the extra trailing fields |
| named msgpack   | 0.74.4           | yes              | no                              | readable; the envelope and `original_anchors` are ignored |
| CBOR            | 0.74.4           | yes              | no                              | readable; the envelope and `original_anchors` are ignored |
| JSON            | 0.74.4           | yes              | no                              | readable; the envelope and `original_anchors` are ignored |

A 0.75 reader reports every complement here as legacy, with the pointer width
whose fingerprint matched, and `put` accepts it under the `AllowLegacy` policy.
0.75 never writes a complement without an envelope. It still writes the 64-bit
legacy fingerprint next to the envelope, so a 0.74 reader of the named, CBOR or
JSON form keeps its fingerprint check.

## Regenerating

The native files and the transcoded wasm32 files come from
`xtask/src/bin/gen-complement-upgrade.rs`:

```sh
cargo run -p xtask --bin gen-complement-upgrade -- write
cargo run -p xtask --bin gen-complement-upgrade -- check
```

`check` regenerates in memory and fails if any file differs. `wasm32.named.msgpack`
was produced by the 0.74.4 wasm32 build:

```sh
wasm-pack build crates/panproto-wasm --target nodejs --dev --out-dir <pkg> --out-name panproto_wasm
node tests/fixtures/complement_upgrade/produce-wasm32.mjs <pkg> bindings/typescript/node_modules/@msgpack/msgpack
```

Run against a later build, the script writes that build's output instead, which
is no longer the legacy form; these files are frozen.
