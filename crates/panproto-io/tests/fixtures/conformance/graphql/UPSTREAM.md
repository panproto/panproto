# GraphQL type system documents

Source: <https://github.com/graphql/graphql-js>, licensed under the MIT
License (`LICENSE`, the repository's file at the commit below,
unchanged). One commit is used: branch `17.x.x`, commit
`52f67c02ffb6aeb72a0e67f4be150665aa229f43` (2026-11-10). Two files of
`src/__testUtils__/` hold a type system document as a TypeScript template
string: `kitchenSinkSDL.ts` and `viralSDL.ts`. Each string is kept as
`upstream/kitchenSinkSDL.graphql` and `upstream/viralSDL.graphql`, its text
taken as the template evaluates it (a backslash before a backtick or a line
break dropped) and otherwise unchanged.

The reader follows the September 2025 edition of the GraphQL
specification (<https://spec.graphql.org/September2025/>, tag
`September2025` of <https://github.com/graphql/graphql-spec>, commit
`89d93ebbe05db06787646d76a696ead8de117b2b`); no specification text is
vendored.

The files under `invalid/` were written for panproto from that edition,
each breaking one rule or using one construct the reader does not read;
`MANIFEST` lists the rule and the refusal expected, and lists the upstream
documents the reader refuses. `conformance_graphql.rs` runs every case,
with the repository's `fixtures/graphql/swapi.graphql` (BSD-3-Clause,
recorded in `fixtures/FIXTURES.md`) read as a third upstream document; the
cases the reader does not pass are listed with their limit in
`../graphql-limits.txt`.
