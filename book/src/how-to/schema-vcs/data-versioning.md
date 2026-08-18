# Version data alongside schemas

Stage data with its schema when repository operations must migrate both together. A later schema migration lifts the committed data through the associated lens.

## Prerequisites

A panproto repository with at least one schema and a corresponding data instance.

## The task

Data is staged together with its schema via `schema add --data`:

```sh
schema add user.json --data records/
schema commit -m "v1 schema and seed data"

# Evolve the schema and re-stage with the same data directory.
schema add user-v2.json --data records/
schema commit -m "v2 schema"

# Sync the working data directory to the latest schema via the auto-derived chain.
schema data sync records/
```

`schema add --data <DATA>` stages a data directory alongside the schema. `schema data sync` lifts the on-disk data forward through any schema migrations between the recorded commit and the target ref (default `HEAD`). To preview without writing:

```sh
schema data migrate records/ --dry-run
```

`schema data migrate` runs the migration between two specific commits (default `parent..HEAD`); add `--coverage` to print statistics. Inspect data status:

```sh
schema status --data records/
schema data status records/
```

To extract historical data, check out the commit:

```sh
schema checkout <commit>
# read records/
```

## Verification

```sh
schema data status records/
```

reports staleness relative to the current schema. A clean status means the data conforms.

`schema data migrate --coverage records/` prints how much of the data was actually transformed by the lift, surfacing partial migrations.

## Common mistakes

- Editing data inside the store directly. Like schemas, data objects are content-addressed.
- Skipping data when committing schema changes. If you commit a v2 schema without ever staging v1 data, there is nothing for the lens to lift; this is fine, but the v2 commit will have no data even though v1 might.
- History rewrites (rebase, amend) on a branch carrying data. The rewrite must lift the data through the new history; this is automatic, but re-run `schema data status records/` afterwards to confirm the on-disk data is in sync.

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Build a migration](../build-migration.md).
- [Use lenses](../use-lenses.md).
