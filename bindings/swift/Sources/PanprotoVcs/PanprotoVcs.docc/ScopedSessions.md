# The scoped session

Why repository work is written as a session, what the session owns, and
why its throws clause is untyped.

## Overview

``withRepository(at:_:)`` opens the store rooted at a directory, runs a
body against it, and releases the handle on the way out.

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

The body runs the whole revision: the check the caller wants, the stage,
and the commit. Two things make this the shape to reach for rather than
``RepositoryHandle/open(at:)`` followed by a release.

The body is isolated to the `PanprotoEngine` global actor, so a run of
operations inside it reaches the store without an actor hop each time,
and each one sees what the one before it left behind. Staging and
committing in particular are a pair: a commit is built from the index
rather than from a schema handed to it, so
``RepositoryHandle/commit(message:author:)`` is always preceded by an
``RepositoryHandle/add(_:)``.

And the handle is released at the end of the scope rather than whenever
its last reference goes. That matters less for correctness than for
predictability, since a handle frees itself either way, but a session
makes the lifetime a scope you can read.

## What the session owns, and what it does not

Only the in-process handle is released on the way out. The `.panproto/`
store stays on disk, which is the whole point: a session is a unit of
work against a repository, not the repository's lifetime.

```swift
func catalogCommitThenCount(
    at directory: URL,
    schema: SchemaHandle,
    against protocolHandle: ProtocolHandle
) async throws -> Int {
    _ = try await catalogRecordRevision(
        at: directory,
        schema: schema,
        against: protocolHandle,
        message: "record the published lexicon",
        author: "alice"
    )
    return try await withRepository(at: directory) { repository in
        try repository.log().entries.count
    }
}
```

Opening a directory that holds no store is not a failure. The directory
structure is written and HEAD is set to an unborn `main`, which is what
lets a first run and every later one call the same thing. Opening one
directory twice yields two handles onto one store, so a write through
either becomes visible to the other once it lands on disk.

Reach for ``RepositoryHandle/open(at:)`` when the handle has to outlive
one scope, which is the case a session cannot express: a long-lived
service that holds a repository open across requests, or a type that owns
one as a stored property.

```swift
@PanprotoEngine
func catalogOpenRepository(at directory: URL) throws(PanprotoError) -> RepositoryHandle {
    try RepositoryHandle.open(at: directory)
}
```

The caller owns what comes back, and the slab entry goes back when the
last reference to it does; `PanprotoHandle.release()` returns it sooner.

## Why the throws clause is untyped

Every failure ``withRepository(at:_:)`` raises itself is a
`PanprotoError.vcs(_:)`, so a typed `throws(PanprotoError)` clause would
seem to be the honest signature. It is not, for two reasons.

The body should be free to fail its own way. A session that reads a file,
checks an invariant, or is cancelled should not have to launder that into
an engine error, and the sample above throws a `RevisionRefused` for
exactly that reason. Holding a schema against its protocol is a policy
this repository applies, not something the store does on its behalf:
``RepositoryHandle/add(_:)`` reports its own verdict in
`VcsAddResult.valid`, and the protocol's verdict comes from
`SchemaHandle.violations(against:)`, which is a call in `Panproto` rather
than here.

And Swift declines to infer a closure's thrown type from context, so a
typed clause would put an explicit `throws(PanprotoError)` annotation on
every session a caller writes. That is a real cost paid at every call
site for a guarantee the body does not want.

## Where a session sits relative to the rest

A session is a scope, not a transaction. Nothing is rolled back if the
body throws after a commit has landed: the commit is on disk, and the
store is exactly as the last successful operation left it. Where an
all-or-nothing revision matters, stage and commit as the last two
statements of the body, so that everything that could refuse has already
run.

Reading is cheap and needs no session of its own if you already have one:
``RepositoryHandle/status()``, ``RepositoryHandle/log(limit:)``,
``RepositoryHandle/diffHead()``, and ``RepositoryHandle/blame(vertex:)``
all read the store as it stands, so a session that writes and then reads
sees its own writes.

## Topics

### Opening

- ``withRepository(at:_:)``
- ``RepositoryHandle/open(at:)``
- ``RepositoryHandle``

### What a session usually does

- ``RepositoryHandle/add(_:)``
- ``RepositoryHandle/commit(message:author:)``
- ``RepositoryHandle/status()``
- ``RepositoryHandle/log(limit:)``
