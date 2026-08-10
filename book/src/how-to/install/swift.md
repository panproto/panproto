# Install the Swift SDK

## Prerequisites

[Swift](https://www.swift.org/) 6.0 or later, which on Apple platforms means Xcode 16 or later. The package builds in Swift 6 language mode with strict concurrency, so an older toolchain will not compile it.

Building `libpanproto_c` from source additionally needs a [Rust](https://www.rust-lang.org/) toolchain; `rustup` is recommended. The prebuilt path below needs neither Rust nor a workspace checkout.

## Install

The package is not yet on a registry; it lives at [`bindings/swift/`](https://github.com/panproto/panproto/tree/main/bindings/swift) in the repository. Every product links [`libpanproto_c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c), the C ABI exposed by the [`panproto-c`](https://github.com/panproto/panproto/tree/main/crates/panproto-c) crate, so the library has to be staged before `swift build` will link. There are three ways to get it.

### Build from source

`bootstrap/dev-link.sh` runs `cargo build -p panproto-c --release`, stages the resulting library and header under `bindings/swift/.panproto-c/`, and syncs the vendored copy of `panproto.h` that the package compiles against. `Package.swift` looks in `.panproto-c/lib` by default, so nothing else needs configuring.

```sh
git clone https://github.com/panproto/panproto.git
cd panproto/bindings/swift
./bootstrap/dev-link.sh
swift build
swift test
```

Run `dev-link.sh` again after every change to `panproto-c` or the workspace `Cargo.toml`. Set `PANPROTO_C_LIB_DIR` to stage somewhere else.

### Prebuilt binaries

`bootstrap/fetch-bindist.sh [version] [variant]` downloads the prebuilt library for the host platform from the corresponding GitHub Release. It detects `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `aarch64-unknown-linux-gnu`. The version defaults to whatever the checkout declares.

```sh
cd panproto/bindings/swift
./bootstrap/fetch-bindist.sh
swift build
```

### XCFramework

iOS builds go through the XCFramework, which carries a macOS-universal slice, an iOS device slice, and a universal simulator slice, together with the headers and a module map.

```sh
cd panproto/bindings/swift
./bootstrap/fetch-bindist.sh v0.69.0 default --xcframework
PANPROTO_SWIFT_XCFRAMEWORK=.panproto-c/panproto_c.xcframework swift build
```

To depend on the package from another project, point `PANPROTO_SWIFT_XCFRAMEWORK_URL` and `PANPROTO_SWIFT_XCFRAMEWORK_CHECKSUM` at the published artifact and its checksum, both of which the release attaches. That mode adds no linker flags of its own, which is what makes the package usable as a dependency; the `dev-link.sh` mode passes an unsafe `-L` flag and is for building the package directly.

## Products

| Product | Contents |
| --- | --- |
| `PanprotoStructural` | The pure value layer: schemas, chains, migrations, and instances as Swift values, plus the CBOR codec. No engine, no FFI. |
| `Panproto` | The engine-backed core: protocols, schemas, instances, I/O codecs, compatibility checking, migrations, lenses, expressions, theories, enrichment, homomorphism search, graph fibers, and datasets. |
| `PanprotoVcs` | Schematic version control. |
| `PanprotoParse` | Full-AST source parsing. Feature-gated. |
| `PanprotoProject` | Multi-file project assembly. Feature-gated. |
| `PanprotoGit` | The git bridge. Feature-gated. |

## Feature-gated tiers

The default `libpanproto_c` exports 103 entry points. The `parse`, `project`, and `git` tiers add 17 more that are absent from that build, so reaching them takes a library built with the matching cargo features and a Swift build told to compile the gated shims in:

```sh
PANPROTO_C_FEATURES=full ./bootstrap/dev-link.sh
PANPROTO_SWIFT_FEATURES=parse,project,git swift build
```

The three gated products exist in the package graph either way, so a build that omits the features still resolves; their modules are simply empty. On the prebuilt path, fetch the `full` variant:

```sh
./bootstrap/fetch-bindist.sh v0.69.0 full
PANPROTO_SWIFT_FEATURES=parse,project,git swift build
```

## Verification

```swift
import Panproto
import PanprotoStructural

let names = try await ProtocolSpec.builtinNames()
print(names.count, "builtin protocols")

let atproto = try await ProtocolHandle.builtin("atproto")
let lexicon = try Data(contentsOf: URL(fileURLWithPath: "app.bsky.feed.post.json"))
let schema = try await SchemaHandle.parseAtprotoLexicon(lexicon)
let value = try await schema.value()
print(value.protocolName, value.vertexCount, "vertices")

let messages = try await schema.violations(against: atproto)
print(messages.isEmpty ? "valid" : messages.joined(separator: "\n"))
```

Every call is `await`: engine work runs on the `@PanprotoEngine` global actor, which is pinned to a single thread. See the [Swift SDK reference](../../reference/sdk-swift.md) for why, and for what that means when you hold a handle.
