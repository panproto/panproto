# Version data alongside schemas

Stage data with its schema when a commit must retain both. Data migration is a separate operation over a commit range.

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

# Sync the working directory across parent..HEAD.
schema data sync records/
```

`schema add --data <DATA>` stages each immediate JSON file in the directory. Staging is all or nothing for the data files. `schema data sync` compares a target commit with its first parent, generates a lens, and rewrites records it can migrate; failed records are reported as skipped.

Preview the default `parent..HEAD` range and run coverage without writing:

```sh
schema data migrate records/ --dry-run --coverage
```

`--dry-run` prints the selected plan without attempting the conversions. The coverage pass then tries each immediate JSON file and reports successes and failures. Use `--range old..new` to select another pair.

```sh
schema status --data records/
schema data status records/
```

Checkout changes only the repository ref. Pass `--migrate` to request a corresponding on-disk data migration:

```sh
schema checkout <commit> --migrate records/
```

## Verification

```sh
schema data status records/
```

prints the number of immediate JSON files, the `HEAD` schema ID, and the number of data sets tracked by that commit. It does not parse the files or prove conformance.

`schema data migrate records/ --dry-run --coverage` is the non-writing per-record check.

## Common mistakes

- Editing data inside the store directly. Like schemas, data objects are content-addressed.
- Skipping data when committing schema changes. If you commit a v2 schema without ever staging v1 data, there is nothing for the lens to lift; this is fine, but the v2 commit will have no data even though v1 might.
- Assuming rebase or amend migrates working data. Those commands take no data-directory option. Run an explicit data migration after rewriting history.

## See also

- [Schema version control semantics](../../explanation/vcs-semantics.md).
- [Build a migration](../build-migration.md).
- [Use lenses](../use-lenses.md).
