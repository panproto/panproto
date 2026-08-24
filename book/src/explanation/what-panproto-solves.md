# What panproto solves

Changing a schema usually creates a corresponding data problem. A renamed field may require only a direct correspondence, while a split record or retired variant may require a default, a value transform, or saved information. When those decisions live only in migration scripts, it can be difficult to compare them with the schemas that they connect.

panproto represents a schema change as structured data. It can compare two schemas, classify their compatibility, compile a migration between them, and apply that migration to schema-typed data. The same representations support histories of schema objects and structural three-way merge. The [protocol catalog](../reference/protocols.md) records the formats for which parsers or other protocol support are registered.

These operations depend on the **protocol-theory model**. A protocol identifies the theories used to describe its schema and instance structure. A parsed schema supplies the concrete types, fields, constraints, and related metadata governed by those theories. [Schemas as theories](./schemas-as-theories.md) develops this distinction.

*Prerequisites:* familiarity with fields, records, and schema versions. No category theory is assumed.

## Compare and classify schema changes

A structural diff records additions, removals, renames, and modifications. The compatibility classifier then assigns the report one of three classifications: fully compatible, backward compatible, or breaking. A `CompatReport` retains the classification together with its breaking and non-breaking findings, which allows a CI job to reject changes under a chosen policy.

## Compile and apply migrations

A migration records correspondences from a source schema to a target schema together with any required value transforms. Compilation checks that the correspondence preserves the relevant schema structure before producing the tables used to transform data. If a forward transformation discards source information, a lens may retain that information in a complement for a later backward update. [Migrations as morphisms](./migrations-as-morphisms.md) describes compilation and data movement; [Lenses and round-trip laws](./lenses-roundtrip.md) describes complements and the available law checks.

## Record and merge schema histories

The version-control layer stores schemas, migrations, data sets, and related metadata as content-addressed objects. Its commands expose commits, branches, tags, diffs, blame, and structural merge over those objects. When two branches make incompatible structural changes, merge returns typed conflict descriptions for explicit resolution. [Schema version control semantics](./vcs-semantics.md) gives the details and limits of this construction.

## Scope

panproto does not supply application behavior that a schema leaves unspecified, deploy a migration, or replace a database. It operates on schema documents and schema-typed data. Its outputs include structural reports, compiled migrations, converted data, and repository objects.

## See also

- [Schemas as theories](./schemas-as-theories.md) for the common representation used across schema languages.
- [Migrations as morphisms](./migrations-as-morphisms.md) for the structure and execution of migrations.
- [Schema version control semantics](./vcs-semantics.md) for structural history and merge.
