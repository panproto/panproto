# panproto-c

[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

`panproto-c` exposes panproto through a C ABI generated with
[`safer-ffi`](https://docs.rs/safer-ffi). The Haskell and Swift bindings use this
ABI.

## Boundary types

Most stateful values remain in Rust. C callers receive `uint32_t` handles into
a process-global resource table. A handle created on one operating-system
thread may be used on another. The table reuses slots after
`pp_handle_free()`, and freeing an invalid or already-freed handle has no
effect.

Structured request and response values cross the boundary as CBOR. Buffers
returned by Rust must be released once with `pp_buf_free()`. Releasing the same
buffer twice is undefined behavior.

The resource table contains protocols, schemas, compiled migrations, I/O
registries, GAT theories and models, repositories, protolens chains, symmetric
lenses, and data sets. The `full-parse` and `project` features add parser and
project resources.

Every status-returning exported function runs inside `catch_unwind`. A Rust
panic is converted to `PpStatus::Panic` and recorded as a CBOR error envelope.
Call `pp_last_error_take()` immediately after a failed call. The error slot is
process-global and holds only the most recent error, so concurrent callers must
serialize each failing call with its error-drain call.

## Status codes

The numeric values are part of the ABI and must not be reordered.

| Code | Variant | Meaning |
|---:|---|---|
| 0 | `Ok` | The operation succeeded. |
| 1 | `Err` | The operation failed without a more specific category. |
| 2 | `Panic` | Rust panicked and the boundary caught the unwind. |
| 3 | `InvalidHandle` | The handle does not name a live resource. |
| 4 | `TypeMismatch` | The handle names the wrong resource type. |
| 5 | `Serialization` | CBOR encoding or decoding failed. |
| 6 | `Internal` | An internal panproto operation failed. |
| 7 | `Operation` | A migration, lens, VCS, parse, or other domain operation failed. |

## Build and features

From the workspace root:

```sh
cargo build -p panproto-c --release
```

The library is built as `cdylib`, `staticlib`, and `rlib`. No optional feature
is enabled by default. The available features are:

| Feature | Additional surface |
|---|---|
| `headers` | Enables the ignored header-generation test. |
| `panic-test` | Exports the internal panic probe used by boundary tests. |
| `full-parse` | Enables the full tree-sitter parser registry. |
| `project` | Enables multi-file project assembly. |
| `git` | Enables the on-disk Git bridge. |
| `format-preserving` | Enables tree-sitter-backed parse and emit operations. |
| `full` | Enables `full-parse`, `project`, `git`, and `format-preserving`. |

Regenerate the committed header after changing an exported function:

```sh
cargo test -p panproto-c --features headers --test headers -- --ignored
```

The generated header is
[`include/panproto.h`](include/panproto.h). The Swift parity gate checks that
the raw Swift layer binds every function in the generated feature headers:

```sh
cd bindings/swift
python3 Scripts/parity-gate.py
```

## Distribution

Release workflows build archives and shared libraries for the supported Linux,
macOS, and Windows targets. Haskell downloads these artifacts because Hackage
packages cannot include precompiled libraries. Swift packages the same ABI in
an XCFramework.

## License

[MIT](../../LICENSE)
