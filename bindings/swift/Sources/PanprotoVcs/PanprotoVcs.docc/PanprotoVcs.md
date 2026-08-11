# ``PanprotoVcs``

Schematic version control: an on-disk `.panproto/` store, the staging
index in front of it, and the porcelain that drives both.

## Overview

This tier versions schemas the way a source control system versions
files. A schema is staged, committed, branched, merged, stashed, diffed,
and blamed, and the vocabulary is deliberately the familiar one, because
the operations mean what the names say.

What differs from a file-based system is what a revision is. A commit
records a schema rather than a text diff, so the change between two
commits is a structural diff over vertices and edges rather than a hunk
of lines, and staging a schema against a non-empty HEAD derives the
migration from HEAD's schema to the staged one at the same time.
`VcsAddResult.autoDerived` reports whether that happened; a first commit
has nothing to derive from.

Everything hangs off ``RepositoryHandle``, and
``withRepository(at:_:)`` is the way in: it opens the store, runs a
session against it on the engine, and hands the handle back when the
session ends. See <doc:ScopedSessions>.

The log is readable either way. ``RepositoryHandle/log(limit:)`` answers
a prefix of a stated length, and ``RepositoryHandle/history(pageSize:)``
answers a ``CommitHistory``, which reads a page at a time and stops
where the caller stops. Reach for the second where the depth is decided
by what the commits say rather than by a number known in advance.

```swift
func catalogRecordRevision(
    at directory: URL,
    schema: SchemaHandle,
    against protocolHandle: ProtocolHandle,
    message: String,
    author: String
) async throws -> String {
    try await withRepository(at: directory) { repository in
        let violations = try schema.violations(against: protocolHandle)
        guard violations.isEmpty else {
            throw RevisionRefused.invalidSchema(violations)
        }
        _ = try repository.add(schema)
        return try repository.commit(message: message, author: author).commitId
    }
}
```

The `RevisionRefused` there is the session's own error, not an engine
one: holding a schema against its protocol before staging is a policy the
caller applies, and <doc:ScopedSessions> covers why the session's throws
clause leaves room for it.

Two conventions hold across the whole tier, so neither is repeated on the
individual methods.

First, every failure arrives as `PanprotoError.vcs(_:)`, with the
operation named the way this API names it, so a message points at the
Swift call rather than at the C symbol underneath it.

Second, every result is the structural record its entry point writes.
Object ids cross the boundary as lowercase hex, 64 characters, and a
commit or a schema is named by its id rather than by a handle;
`VcsObjectID` carries the two facts about an id that a host reading one
needs. Those records are declared in `PanprotoStructural`, which
documents each of them alongside the rest of the wire types.

## Topics

### Articles

- <doc:ScopedSessions>

### Opening a repository

- ``RepositoryHandle``
- ``withRepository(at:_:)``

### Staging and committing

- ``RepositoryHandle/add(_:)``
- ``RepositoryHandle/commit(message:author:)``
- ``RepositoryHandle/status()``

### Reading history

- ``RepositoryHandle/log(limit:)``
- ``RepositoryHandle/history(pageSize:)``
- ``CommitHistory``
- ``RepositoryHandle/diff(from:to:)``
- ``RepositoryHandle/diffHead()``
- ``RepositoryHandle/blame(vertex:)``

### Branches and merges

- ``RepositoryHandle/listBranches()``
- ``RepositoryHandle/createBranch(named:)``
- ``RepositoryHandle/checkout(_:)``
- ``RepositoryHandle/merge(branch:author:)``

### The stash

- ``RepositoryHandle/pushStash()``
- ``RepositoryHandle/popStash()``
