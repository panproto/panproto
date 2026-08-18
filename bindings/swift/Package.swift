// swift-tools-version: 6.1

import Foundation
import PackageDescription

// MARK: - Release pin
//
// The XCFramework published for the most recent release. A tagged
// checkout resolves against this with no configuration, which is what
// makes the package usable as an ordinary SwiftPM dependency and is the
// only mode that reaches iOS. `publish-swift.yml` rewrites both
// constants when it publishes an artifact; while they are empty, a
// consumer must dev-link or point at an XCFramework explicitly.

// The values sit on their own lines because neither fits the 100 column
// limit beside its declaration: a release asset URL is as long as it is,
// and a SHA-256 is 64 characters. `swift-format-ignore` does not help,
// since LineLength is enforced by the pretty-printer rather than by the
// node rules that directive suppresses.
//
// The rewrite in `publish-swift.yml` captures everything up to the opening
// quote and writes it back verbatim, so the line break and its indentation
// survive every republish.
private let releaseXCFrameworkURL =
    "https://github.com/panproto/panproto/releases/download/v0.71.0/panproto_c.xcframework.zip"
private let releaseXCFrameworkChecksum =
    "dc7a9a22e208749dadd6c7c916e5df4dd9a658f81bdf165a4377b095b94fdee5"

// MARK: - Build configuration
//
// The package resolves the panproto-c library in one of three modes,
// in this order of precedence.
//
//   xcframework   Set `PANPROTO_SWIFT_XCFRAMEWORK` to a local
//                 `panproto_c.xcframework`, or `..._URL` together with
//                 `..._CHECKSUM` for a specific published artifact.
//
//   dev-link      `bootstrap/dev-link.sh` builds the workspace crate and
//                 stages `libpanproto_c` under `.panproto-c/lib`. The
//                 `CPanproto` system-library target picks it up from
//                 there. Override the directory with `PANPROTO_C_LIB_DIR`.
//                 This is what a checkout of the repository uses.
//
//   release pin   The constants above, once a release has filled them
//                 in and neither of the other two modes applies.
//
// Feature-gated domains (parse, project, git) are absent from the
// default cdylib, and are selected with package traits:
//
//     swift build --traits PANPROTO_PARSE,PANPROTO_PROJECT,PANPROTO_GIT
//
// A trait defines a compilation condition of the same name, which is
// what the `#if PANPROTO_PARSE` blocks in the gated sources read. The
// trait must name a feature the linked library was actually built with;
// `bootstrap/dev-link.sh` prints the matching invocation. The gated
// products always exist so the package graph is stable, and compile to
// an empty module when their trait is off.

private let env = ProcessInfo.processInfo.environment

private let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

/// Whether an explicit XCFramework was named in the environment.
private let xcframeworkRequested =
    (env["PANPROTO_SWIFT_XCFRAMEWORK"].map { !$0.isEmpty } ?? false)
    || (env["PANPROTO_SWIFT_XCFRAMEWORK_URL"].map { !$0.isEmpty } ?? false)

/// Whether a staged library is present for dev-link mode.
private let stagedLibraryDirectory: String? = {
    if let explicit = env["PANPROTO_C_LIB_DIR"], !explicit.isEmpty {
        return URL(fileURLWithPath: explicit).standardizedFileURL.path
    }
    let staged = packageDirectory.appendingPathComponent(".panproto-c/lib").standardizedFileURL
    return FileManager.default.fileExists(atPath: staged.path) ? staged.path : nil
}()

/// Directory holding `libpanproto_c.dylib` / `.a` in dev-link mode.
///
/// Only a directory that exists counts. Emitting a search path for one
/// that does not is what made this package ineligible as a dependency:
/// SwiftPM rejects any package whose manifest carries `unsafeFlags`, so
/// a consumer resolving it got the flags whether or not anyone had ever
/// dev-linked. A checkout that has run `bootstrap/dev-link.sh` gets
/// them; nothing else does.
private let devLinkLibraryDirectory: String? = {
    guard !xcframeworkRequested else { return nil }
    return stagedLibraryDirectory
}()

// Search-path and rpath flags for the staged library. SwiftPM marks
// these unsafe, which is why they appear only when a staged library is
// present, and never in the mode a released consumer resolves.
private let devLinkSettings: [LinkerSetting] = {
    guard let directory = devLinkLibraryDirectory else { return [] }
    return [
        .unsafeFlags([
            "-L\(directory)",
            "-Xlinker", "-rpath", "-Xlinker", directory,
        ])
    ]
}()

/// Express `path` relative to the package root.
///
/// Returns `path` unchanged when it is already relative. When it is
/// absolute, walks up from the package root as far as the two share a
/// prefix, which covers a staged directory inside the package and an
/// artifact anywhere else on the filesystem alike.
private func packageRelative(_ path: String) -> String {
    guard path.hasPrefix("/") else { return path }
    let target = URL(fileURLWithPath: path).standardizedFileURL.pathComponents
    let base = packageDirectory.standardizedFileURL.pathComponents
    var shared = 0
    while shared < min(target.count, base.count), target[shared] == base[shared] {
        shared += 1
    }
    let ascent = Array(repeating: "..", count: base.count - shared)
    return (ascent + target[shared...]).joined(separator: "/")
}

private let cPanprotoTarget: Target = {
    if let local = env["PANPROTO_SWIFT_XCFRAMEWORK"], !local.isEmpty {
        // `binaryTarget(path:)` takes a package-relative path and
        // rejects an absolute one outright, so an absolute path is
        // relativized here rather than pushed onto the caller.
        // `fetch-bindist.sh` stages into an absolute directory and
        // prints it, which is the spelling most people will paste.
        return .binaryTarget(name: "CPanproto", path: packageRelative(local))
    }
    if let url = env["PANPROTO_SWIFT_XCFRAMEWORK_URL"], !url.isEmpty,
        let checksum = env["PANPROTO_SWIFT_XCFRAMEWORK_CHECKSUM"], !checksum.isEmpty
    {
        return .binaryTarget(name: "CPanproto", url: url, checksum: checksum)
    }
    if devLinkLibraryDirectory == nil, !releaseXCFrameworkURL.isEmpty {
        return .binaryTarget(
            name: "CPanproto",
            url: releaseXCFrameworkURL,
            checksum: releaseXCFrameworkChecksum
        )
    }
    return .systemLibrary(name: "CPanproto", path: "Sources/CPanproto")
}()

// The DocC plugin is the package's only external dependency, and it is
// needed for exactly one command. Pulling it in unconditionally would
// make `swift build` reach the network on a fresh checkout, so it is
// opted into with `PANPROTO_SWIFT_DOCC=1`, which the publish workflow
// sets.
private let documentationDependencies: [Package.Dependency] =
    env["PANPROTO_SWIFT_DOCC"] == "1"
    ? [.package(url: "https://github.com/swiftlang/swift-docc-plugin", from: "1.4.0")]
    : []

let package = Package(
    name: "panproto",
    platforms: [
        .macOS(.v14),
        .iOS(.v17),
    ],
    products: [
        // Pure value layer: schemas, chains, migrations as Swift values,
        // plus the CBOR codec they encode through. No FFI, no engine.
        .library(name: "PanprotoStructural", targets: ["PanprotoStructural"]),
        // Engine-backed core: protocol, schema, instance, io, check,
        // migration, lens, expression, gat, enriched, hom, graph, data.
        .library(name: "Panproto", targets: ["Panproto"]),
        // Schematic version control.
        .library(name: "PanprotoVcs", targets: ["PanprotoVcs"]),
        // Feature-gated tiers. Present in the graph unconditionally;
        // empty unless the matching feature is requested.
        .library(name: "PanprotoParse", targets: ["PanprotoParse"]),
        .library(name: "PanprotoProject", targets: ["PanprotoProject"]),
        .library(name: "PanprotoGit", targets: ["PanprotoGit"]),
    ],
    traits: [
        // Each trait defines a compilation condition of its own name,
        // which is what the `#if` blocks in the gated sources read. None
        // is on by default: the default `libpanproto_c` does not export
        // the symbols behind them, so a build that enabled one without
        // linking a matching library would fail at link time.
        .trait(
            name: "PANPROTO_PARSE",
            description: "Full-AST source parsing. Needs a libpanproto_c built with `full-parse`."
        ),
        .trait(
            name: "PANPROTO_PROJECT",
            description: "Multi-file project assembly. Needs a libpanproto_c built with `project`."
        ),
        .trait(
            name: "PANPROTO_GIT",
            description: "The git bridge. Needs a libpanproto_c built with `git`."
        ),
    ],
    dependencies: documentationDependencies,
    targets: [
        cPanprotoTarget,

        .target(
            name: "PanprotoStructural"
        ),

        .target(
            name: "PanprotoFFI",
            dependencies: ["CPanproto", "PanprotoStructural"],
            linkerSettings: devLinkSettings
        ),

        .target(
            name: "Panproto",
            dependencies: ["PanprotoFFI", "PanprotoStructural"]
        ),

        .target(
            name: "PanprotoVcs",
            dependencies: ["Panproto"]
        ),

        .target(
            name: "PanprotoParse",
            dependencies: ["Panproto"]
        ),

        .target(
            name: "PanprotoProject",
            dependencies: ["Panproto"]
        ),

        .target(
            name: "PanprotoGit",
            dependencies: ["Panproto", "PanprotoVcs"]
        ),

        .executableTarget(
            name: "atproto-post-migration",
            dependencies: ["Panproto", "PanprotoStructural"],
            path: "Examples/AtprotoPostMigration"
        ),

        // Captures the committed test fixtures by driving the raw shim
        // layer against the live engine. Development tooling: it needs a
        // linked library and a checkout of the repository's JSON inputs.
        .executableTarget(
            name: "generate-fixtures",
            dependencies: ["PanprotoFFI", "PanprotoStructural"],
            path: "Scripts/GenerateFixtures"
        ),

        .testTarget(
            name: "PanprotoStructuralTests",
            dependencies: ["PanprotoStructural"]
        ),

        .testTarget(
            name: "PanprotoTests",
            dependencies: ["Panproto", "PanprotoStructural", "PanprotoFFI"],
            resources: [.copy("Fixtures")]
        ),

        .testTarget(
            name: "PanprotoVcsTests",
            dependencies: ["PanprotoVcs", "Panproto"]
        ),

        .testTarget(
            name: "PanprotoFeatureTests",
            dependencies: ["PanprotoParse", "PanprotoProject", "PanprotoGit", "Panproto"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
