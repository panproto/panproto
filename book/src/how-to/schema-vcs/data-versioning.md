# Version data alongside schemas

panproto-vcs commits can carry data instances. When a branch's schema migrates, the data carried by its commits is automatically lifted forward by the migration's lens. This makes data evolution part of the schema history rather than a parallel concern.

## Prerequisites

A panproto repository with at least one schema and a corresponding data instance.

## The task

```sh
schema data add records/users.jsonl
schema commit -m "v1 schema and seed data"

# evolve the schema, generating a migration
schema add user-v2.json
schema commit -m "v2 schema"

# the lens auto-derived from v1->v2 lifts the data
schema data show HEAD       # shows lifted v2-shape records
schema data show HEAD~1     # shows v1-shape records
```

`data add` stages a data instance against the current schema. Subsequent commits that change the schema automatically lift the data via the migration's lens; both v1 and v2 shapes remain accessible by walking the history.

To extract data at a specific commit:

```sh
schema data export --at <commit> --out records-at-commit.jsonl
```

## Verification

```sh
schema data verify --at HEAD
```

Re-validates the data at the named commit against that commit's schema. A pass means the lift was lossless and the result conforms.

## Common mistakes

- Editing data inside the store directly. Like schemas, data objects are content-addressed.
- Skipping data when committing schema changes. If you commit a v2 schema without ever staging v1 data, there is nothing for the lens to lift; this is fine, but the v2 commit will have no data even though v1 might.
- History rewrites (rebase, amend) on a branch carrying data. The rewrite must lift the data through the new history; this is automatic, but verify with `schema data verify` afterwards.

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Build a migration](../build-migration.md).
- [Use lenses](../use-lenses.md).
