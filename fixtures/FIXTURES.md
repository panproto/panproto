# Vendored Fixtures

Real-world schemas and data vendored for use by examples and benchmarks across
the workspace. Every fixture is pinned to a specific upstream commit, tag, or
capture timestamp, and is used offline by `cargo run --example` and
`cargo bench`. Nothing under this tree is fetched at build time.

## atproto/

AT Protocol Lexicons and live record payloads from Bluesky.

### lexicons/

Fetched from `github.com/bluesky-social/atproto` at commit
`750cfe9020a11c5de1ce6b2e3647d52939a3e284` (`main` on 2026-04-19).

| File | Source path |
| --- | --- |
| `app.bsky.feed.post.json` | `lexicons/app/bsky/feed/post.json` |
| `app.bsky.actor.profile.json` | `lexicons/app/bsky/actor/profile.json` |
| `app.bsky.feed.like.json` | `lexicons/app/bsky/feed/like.json` |
| `app.bsky.feed.repost.json` | `lexicons/app/bsky/feed/repost.json` |
| `app.bsky.graph.follow.json` | `lexicons/app/bsky/graph/follow.json` |
| `com.atproto.repo.createRecord.json` | `lexicons/com/atproto/repo/createRecord.json` |

License: MIT / Apache-2.0 (bluesky-social/atproto is dual-licensed).

### records/

Live responses captured from the public Bluesky AppView
(`public.api.bsky.app`) on 2026-04-19.

| File | Endpoint |
| --- | --- |
| `profile-bsky.app.json` | `app.bsky.actor.getProfile?actor=bsky.app` |
| `feed-bsky.app.json` | `app.bsky.feed.getAuthorFeed?actor=bsky.app&limit=10` |

These are public records produced by the `@bsky.app` account. Included for
testing the shape of real records, not to reproduce content.

Derived from the two AppView responses, flat record-shaped fixtures:

| File | Derivation |
| --- | --- |
| `post-0.json` … `post-4.json` | `feed-bsky.app.json[feed[0..5].post.record]` — bare `app.bsky.feed.post` records |
| `profile-record.json` | `profile-bsky.app.json` restricted to the `app.bsky.actor.profile` record fields (`displayName`, `description`, `avatar`, `banner`, `createdAt`) |

## jsonschema/

Fetched from `schemastore.org` on 2026-04-19.

| File | Source |
| --- | --- |
| `package.json` | `https://json.schemastore.org/package.json` |
| `tsconfig.json` | `https://json.schemastore.org/tsconfig.json` |
| `github-workflow.json` | `https://json.schemastore.org/github-workflow.json` |

License: Apache-2.0 (SchemaStore).

## protobuf/

### OpenTelemetry proto

Fetched from `github.com/open-telemetry/opentelemetry-proto` at commit
`85e63b1ad6d0667e48707e8c0a88f366e79a68ce`.

| File | Source path |
| --- | --- |
| `trace.proto` | `opentelemetry/proto/trace/v1/trace.proto` |
| `common.proto` | `opentelemetry/proto/common/v1/common.proto` |
| `resource.proto` | `opentelemetry/proto/resource/v1/resource.proto` |

License: Apache-2.0.

### descriptor.proto

Fetched from `github.com/protocolbuffers/protobuf` at commit
`e3370c2e26bbfaa63bc9f8e4ac0f8dc066ba3eeb`:
`src/google/protobuf/descriptor.proto`.

License: BSD-3-Clause (protocolbuffers/protobuf).

## graphql/

### swapi.graphql

Fetched from `github.com/graphql/swapi-graphql` at commit
`48d66bcfe3368b1df660b4a24f87caf7b5028e36`: `schema.graphql`.

License: BSD-3-Clause (graphql/swapi-graphql).

## sql/

Sakila sample database schemas, fetched from `github.com/jOOQ/sakila` at commit
`aed53ce65404eac184f4134f34239a08c464df77`.

| File | Source path |
| --- | --- |
| `sakila-postgres.sql` | `postgres-sakila-db/postgres-sakila-schema.sql` |
| `sakila-sqlite.sql` | `sqlite-sakila-db/sqlite-sakila-schema.sql` |

License: BSD-3-Clause (jOOQ/sakila).

## Refreshing fixtures

To re-pin fixtures to newer upstream commits, update the SHAs above and re-run
the curl commands recorded in the commit that introduced this directory.
Benchmark baselines in `.claude/skills/bench/SKILL.md` assume the pinned set;
flag any re-pin that materially changes fixture size in the PR description.
