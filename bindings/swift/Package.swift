// swift-tools-version: 6.0

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

private let releaseXCFrameworkURL = ""
private let releaseXCFrameworkChecksum = ""

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
//                 This is what a checkout of the repository uses, and it
//                 is the only mode that passes an unsafe linker flag,
//                 which is why it is not the released one.
//
//   release pin   The constants above, once a release has filled them
//                 in and neither of the other two modes applies.
//
// Feature-gated domains (parse, project, git) are absent from the
// default cdylib. `PANPROTO_SWIFT_FEATURES` is a comma-separated list
// drawn from `parse`, `project`, `git`, and `full`; it must name the
// same features the linked library was built with. The gated products
// always exist so the package graph is stable, but their sources
// compile to an empty module unless the matching feature is on.

private let env = ProcessInfo.processInfo.environment

private let packageDirectory = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

private let requestedFeatures: Set<String> = {
    guard let raw = env["PANPROTO_SWIFT_FEATURES"], !raw.isEmpty else { return [] }
    var names = Set(
        raw.split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespaces).lowercased() }
            .filter { !$0.isEmpty }
    )
    if names.contains("full") {
        names.formUnion(["parse", "project", "git"])
    }
    // `full-parse` is the cargo feature name; accept it as an alias so
    // one environment variable can drive both cargo and SwiftPM.
    if names.contains("full-parse") { names.insert("parse") }
    return names
}()

private let parseEnabled = requestedFeatures.contains("parse")
private let projectEnabled = requestedFeatures.contains("project")
private let gitEnabled = requestedFeatures.contains("git")

/// Compile-time defines that switch the gated shims and domain APIs on.
private let featureDefines: [SwiftSetting] = {
    var settings: [SwiftSetting] = []
    if parseEnabled { settings.append(.define("PANPROTO_PARSE")) }
    if projectEnabled { settings.append(.define("PANPROTO_PROJECT")) }
    if gitEnabled { settings.append(.define("PANPROTO_GIT")) }
    return settings
}()

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
private let devLinkLibraryDirectory: String? = {
    guard !xcframeworkRequested else { return nil }
    if let staged = stagedLibraryDirectory { return staged }
    // Nothing staged. Fall back to the release pin when there is one,
    // and otherwise keep the default path so the linker error names the
    // directory `dev-link.sh` would have written.
    guard releaseXCFrameworkURL.isEmpty else { return nil }
    return packageDirectory.appendingPathComponent(".panproto-c/lib").standardizedFileURL.path
}()

// Search-path and rpath flags for the staged library. SwiftPM marks
// these unsafe, which makes the package ineligible as a *dependency*
// while dev-linking; that is the intended trade. Released consumers
// use the xcframework mode below, which needs no flags at all.
private let devLinkSettings: [LinkerSetting] = {
    guard let directory = devLinkLibraryDirectory else { return [] }
    return [
        .unsafeFlags([
            "-L\(directory)",
            "-Xlinker", "-rpath", "-Xlinker", directory,
        ])
    ]
}()

private let cPanprotoTarget: Target = {
    if let local = env["PANPROTO_SWIFT_XCFRAMEWORK"], !local.isEmpty {
        return .binaryTarget(name: "CPanproto", path: local)
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
    dependencies: documentationDependencies,
    targets: [
        cPanprotoTarget,

        .target(
            name: "PanprotoStructural",
            swiftSettings: featureDefines
        ),

        .target(
            name: "PanprotoFFI",
            dependencies: ["CPanproto"],
            swiftSettings: featureDefines,
            linkerSettings: devLinkSettings
        ),

        .target(
            name: "Panproto",
            dependencies: ["PanprotoFFI", "PanprotoStructural"],
            swiftSettings: featureDefines
        ),

        .target(
            name: "PanprotoVcs",
            dependencies: ["Panproto"],
            swiftSettings: featureDefines
        ),

        .target(
            name: "PanprotoParse",
            dependencies: ["Panproto"],
            swiftSettings: featureDefines
        ),

        .target(
            name: "PanprotoProject",
            dependencies: ["Panproto"],
            swiftSettings: featureDefines
        ),

        .target(
            name: "PanprotoGit",
            dependencies: ["Panproto", "PanprotoVcs"],
            swiftSettings: featureDefines
        ),

        .executableTarget(
            name: "atproto-post-migration",
            dependencies: ["Panproto", "PanprotoStructural"],
            path: "Examples/AtprotoPostMigration",
            swiftSettings: featureDefines
        ),

        // Captures the committed test fixtures by driving the raw shim
        // layer against the live engine. Development tooling: it needs a
        // linked library and a checkout of the repository's JSON inputs.
        .executableTarget(
            name: "generate-fixtures",
            dependencies: ["PanprotoFFI", "PanprotoStructural"],
            path: "Scripts/GenerateFixtures",
            swiftSettings: featureDefines
        ),

        .testTarget(
            name: "PanprotoStructuralTests",
            dependencies: ["PanprotoStructural"],
            swiftSettings: featureDefines
        ),

        .testTarget(
            name: "PanprotoTests",
            dependencies: ["Panproto", "PanprotoStructural", "PanprotoFFI"],
            resources: [.copy("Fixtures")],
            swiftSettings: featureDefines
        ),

        .testTarget(
            name: "PanprotoVcsTests",
            dependencies: ["PanprotoVcs", "Panproto"],
            swiftSettings: featureDefines
        ),

        .testTarget(
            name: "PanprotoFeatureTests",
            dependencies: ["PanprotoParse", "PanprotoProject", "PanprotoGit", "Panproto"],
            swiftSettings: featureDefines
        ),
    ],
    swiftLanguageModes: [.v6]
)
