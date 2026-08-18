# What panproto solves

## In plain terms

Changing the shape of data usually creates a second task: mapping existing records into the new shape. A renamed field may be routine, while a split record or a retired variant can require stored information, defaults, or an explicit value transform. Hand-written migration code tends to scatter those decisions across scripts that are difficult to compare with the schemas they connect.

panproto reads two schemas, computes a structural difference, and can compile an executable migration artifact between them. The compatibility classifier reports a change as fully compatible, backward compatible, or breaking. The same structural operations also support schema history through git-style commands and work across the formats listed in the [protocol catalog](../reference/protocols.md).

The common representation is the **protocol-theory model**: each schema language supplies theories that describe its schema and instance structure, while each parsed schema supplies the concrete vertices, edges, and constraints those theories govern. The rest of this quadrant develops that model from migrations through merge.

*Prerequisites:* familiarity with fields, records, and schema versions. No category theory is assumed.

## The three concrete jobs

panproto groups three jobs that are often handled separately:

1. **Diff and classify schema changes.** Given two versions of a schema, identify what was added, removed, renamed, or had its type changed; classify the overall change as fully compatible, backward compatible, or breaking (the shipped `CompatReport` carries a `classification` tier alongside a `breaking` list, a `non_breaking` list, and a `compatible` boolean); and surface the result in a way CI can gate on.
2. **Compile the migration.** Produce a structured transform that lifts old records to the new shape. When the transform drops source information, a complement can retain that information for the backward direction. Runtime checks and property tests exercise the round-trip laws, with the limits described in [What panproto verifies](./what-is-verified.md).
3. **Version-control schemas as first-class objects.** Record commits, branches, merges, diffs, blame, and tags over schema objects. Structural merge can surface schema conflicts as typed descriptors instead of line-oriented conflict markers.

## What it does not solve

panproto does not write application logic, validate behavior that the schema does not express, or replace a database. It operates between schema documents and schema-typed data. Its outputs are structural reports and migration artifacts, not deployment infrastructure.

## See also

- [Schemas as theories](./schemas-as-theories.md) for the structure that lets the same workflow apply across schema languages.
- [Migrations as morphisms](./migrations-as-morphisms.md) for the model behind generated migrations.
- [Schema version control semantics](./vcs-semantics.md) for the merge story.
