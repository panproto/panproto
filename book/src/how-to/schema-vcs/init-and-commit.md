# Init and commit

Initialize a panproto repository, stage a schema, and record the first content-addressed commit.

## Prerequisites

The `schema` CLI installed. A directory containing schema files (or a fresh directory you are about to populate).

## The task

```sh
cd my-schemas/
schema init                 # create .panproto/
schema add user.json
schema commit -m "initial user schema"
schema log                  # show history
```

`init` creates a `.panproto/` object store and a `main` branch. It may also generate `panproto.toml` when package detection finds source packages. `add` parses or loads the supplied path and stages the resulting schema; `commit` records the staged schema. A path ending in `.json` is deserialized directly as panproto's internal `Schema` representation. To stage an ATProto Lexicon bundle, point `add` at a directory whose manifest declares a homogeneous `atproto` package. Non-JSON source files use the full-AST parser.

To inspect the current state:

```sh
schema status
schema diff --staged      # diff the staged schema against HEAD
schema show <commit-hash>
```

`schema diff` without `--staged` requires two file paths. A staged diff also requires an existing commit, so use it after the first commit.

## Verification

```sh
schema log --oneline
```

prints one line per commit. The default long format includes the commit ID, schema ID, author, timestamp, and message.

## Common mistakes

- Forgetting to `schema add` before `schema commit`. Like git, the staging area is explicit; commits only include staged changes.
- Editing inside `.panproto/objects/` directly. The store is content-addressed; manual edits break the hash invariants.

## See also

- [Branch and merge](./branch-and-merge.md).
- [Reference: CLI](../../reference/cli.md).
- [Schema version control semantics](../../explanation/vcs-semantics.md).
