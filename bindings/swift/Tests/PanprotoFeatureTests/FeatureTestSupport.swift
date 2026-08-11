// What the feature-gated suites share: a scratch directory that cleans
// up after itself, and a real git repository to import.
//
// Each helper carries the compile-time condition of the suites that use
// it, so a build without those features compiles this file to nothing
// rather than to unused code.

#if PANPROTO_PROJECT || PANPROTO_GIT

import Foundation

/// Run `body` against a fresh directory, removing it afterwards.
///
/// Each test gets its own directory, so a tree one test writes is
/// invisible to every other and the suites run in parallel without
/// sharing a filesystem.
func withTemporaryDirectory<T>(_ body: (URL) async throws -> T) async throws -> T {
    let directory = FileManager.default.temporaryDirectory
        .appendingPathComponent("panproto-feature-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    return try await body(directory)
}

/// Write `contents` to `path` under `directory`, creating the
/// intervening directories.
func write(_ contents: String, to path: String, under directory: URL) throws {
    let file = directory.appendingPathComponent(path)
    try FileManager.default.createDirectory(
        at: file.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    try Data(contents.utf8).write(to: file)
}

#endif

#if PANPROTO_GIT

/// A failure running `git`, carrying what the command wrote to standard
/// error.
struct GitCommandFailure: Error, CustomStringConvertible {
    /// The arguments the command was run with.
    let arguments: [String]
    /// The exit status the command reported.
    let status: Int32
    /// What the command wrote to standard error.
    let standardError: String

    /// The command line, its status, and its complaint.
    var description: String {
        "git \(arguments.joined(separator: " ")) exited \(status): \(standardError)"
    }
}

/// Run `git` in `directory` and wait for it.
///
/// The identity flags are passed per invocation rather than written into
/// a config file, so the test neither reads nor writes the developer's
/// own git configuration.
@discardableResult
func git(_ arguments: [String], in directory: URL) throws -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.currentDirectoryURL = directory
    process.arguments =
        ["git", "-c", "user.name=Panproto Test", "-c", "user.email=test@panproto.dev"] + arguments
    let output = Pipe()
    let errors = Pipe()
    process.standardOutput = output
    process.standardError = errors
    try process.run()
    let written = output.fileHandleForReading.readDataToEndOfFile()
    let complaint = errors.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else {
        throw GitCommandFailure(
            arguments: arguments,
            status: process.terminationStatus,
            standardError: String(decoding: complaint, as: UTF8.self)
        )
    }
    return String(decoding: written, as: UTF8.self)
}

/// Build a git repository in `directory` with one commit per element of
/// `commits`, applied in order.
///
/// Each element is the file set that commit writes, so a path appearing
/// in two elements is modified rather than added the second time.
func makeGitRepository(
    at directory: URL,
    commits: [(message: String, files: [String: String])]
) throws {
    try git(["init", "--quiet", "--initial-branch=main"], in: directory)
    for commit in commits {
        for (path, contents) in commit.files {
            try write(contents, to: path, under: directory)
        }
        try git(["add", "--all"], in: directory)
        try git(["commit", "--quiet", "--message", commit.message], in: directory)
    }
}

#endif
