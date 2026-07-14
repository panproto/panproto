# What panproto solves

## In plain terms

Whenever you change the shape of your data: add a column, rename a field, split one record into two, retire an enum value, switch from JSON Schema to Protobuf, you have to write migration code to bring existing data forward. That code is tedious, error-prone, and easy to get subtly wrong in ways that only surface in production.

panproto reads two versions of a schema, computes the difference, and produces the migration code for you. It also classifies the change as fully compatible, backward compatible, or breaking, version-controls the schema history with git-style commands, and lets the same diff/migrate/version-control workflow run across schema languages: JSON Schema, Protobuf, GraphQL, ATProto Lexicons, SQL DDL, Avro, and 45 others.

The thing that makes this work uniformly across all those languages is that panproto treats every schema as an instance of a common structure. The shared structure is what the rest of the explanation quadrant unpacks.

## The three concrete jobs

panproto exists to do three jobs that today are usually done by hand:

1. **Diff and classify schema changes.** Given two versions of a schema, identify what was added, removed, renamed, or had its type changed; classify the overall change as fully compatible, backward compatible, or breaking (the shipped `CompatReport` carries a `classification` tier alongside a `breaking` list, a `non_breaking` list, and a `compatible` boolean); and surface the result in a way CI can gate on.
2. **Generate the migration code.** Produce a *bidirectional* transform between the two versions: a function that lifts old records to the new shape, paired with a function that pushes new records back to the old shape without losing data. The pair is checked to satisfy round-trip laws, so the migration cannot silently corrupt data.
3. **Version-control schemas as first-class objects.** Treat the schema history like a git history: commits, branches, merges, diffs, blame, tags. Merge conflicts at the schema level are resolved by a precise, well-defined operation rather than a manual three-way text merge.

## What it does not solve

panproto does not write your application code, validate your runtime behaviour beyond what the schema describes, or replace your database. It is a layer between schema files and the data they describe. The output is migration code and a diff classification, not infrastructure.

## See also

- [Schemas as theories](./schemas-as-theories.md) for the structure that lets the same workflow apply across schema languages.
- [Migrations as morphisms](./migrations-as-morphisms.md) for the model behind generated migrations.
- [Schema version control semantics](./vcs-semantics.md) for the merge story.
