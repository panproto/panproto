# The vocabulary in plain terms

panproto uses mathematical terms to distinguish operations that ordinary migration vocabulary often groups together. This page gives working translations and points to the chapters that define the terms more precisely.

*Prerequisites:* familiarity with schemas and data migration. This page also supports the intermediate route in the [explanation reading guide](./index.md).

## Terms

| panproto says | Working meaning | Nearby familiar concept |
|---|---|---|
| **protocol** | A registered schema language, including the names of its schema and instance theories and its structural rules | An entry in a format registry, such as JSON Schema, Protobuf, or SQL DDL |
| **theory** (GAT) | A specification of the sorts, operations, and equations used to describe a family of structures | A typed algebraic signature with laws |
| **schema** | A schema document parsed into panproto's common representation | An `api.yaml` or `.proto` file |
| **instance** | Data interpreted under a schema | A row or JSON document |
| **vertex / edge** | A schema type and a directed field or relation between types | Nodes and arrows in a schema diagram |
| **migration** (morphism) | A map from source schema elements to target schema elements, with optional value transforms | The structural part of a migration plan |
| **lift** | Applying a compiled migration to data | Running a data conversion; the exact direction depends on the selected lifting operation |
| **restrict** | Reinterpreting target-side structure through a migration on the source side | Projecting an interface onto the source fields that support it |
| **lens** | A forward transformation that returns a view and a complement, paired with reconstruction from that view and complement | A bidirectional converter with explicit saved state |
| **complement** | Information retained during the forward transformation so that reconstruction can restore it | An undo record |
| **round-trip laws** (GetPut, PutGet, PutPut) | Equations relating forward transformation and reconstruction | Properties checked on concrete inputs or generated test cases |
| **protolens** | A composable description from which a lens can be instantiated for matching schemas | A schema-indexed transformation template |
| **dependent optic** | A protolens step whose applicability depends on schema structure | A template operation with a structural precondition |
| **colimit** | A construction that combines theories along explicitly shared parts | Gluing typed specifications over a common interface |
| **pushout** | A colimit that combines two objects receiving maps from a common object | The categorical shape associated with a structural three-way merge |
| **existence check** | A finite validation that a proposed migration covers the required cases | A static precondition check rather than execution on sample data |

Tutorials chiefly use *schema*, *migration*, *lens*, and *complement*. How-to guides also refer to protocols and protolenses. Remaining terms appear in the explanation chapters where their distinctions affect an operation. The [glossary](../glossary.md) provides shorter formal definitions, while [Schemas as theories](./schemas-as-theories.md) and [Migrations as morphisms](./migrations-as-morphisms.md) develop the central representations.

## See also

- [Glossary](../glossary.md) for formal definitions.
- [What panproto solves](./what-panproto-solves.md) for the problem statement.
- [Schemas as theories](./schemas-as-theories.md) and [Migrations as morphisms](./migrations-as-morphisms.md) for the underlying representations.
