# The vocabulary in plain terms

## In plain terms

panproto uses mathematical terms where they distinguish operations that ordinary migration vocabulary tends to conflate. You do not need those definitions to run the tool. This page supplies a working translation for each term and links to the chapter that develops it.

The translations are deliberately approximate. They are sufficient for the tutorials and how-to guides; the [glossary](../glossary.md) and linked explanation chapters give the distinctions needed for edge cases.

*Prerequisites:* familiarity with schemas and data migration. This page is also the prerequisite for the intermediate path in the [explanation reading guide](./index.md).

## The table

| panproto says | Plain terms | Nearest familiar thing |
|---|---|---|
| **protocol** | A schema language panproto can read and write | An entry in a format registry: "JSON Schema", "Protobuf", "SQL DDL" |
| **theory** (GAT) | The rulebook saying what a well-formed schema in one language looks like | The spec for a format, executable |
| **schema** | Your actual schema, parsed into panproto's internal form | The `api.yaml` or `.proto` you already have |
| **instance** | A data record conforming to a schema | A row; a JSON document |
| **vertex / edge** | A type in a schema / a field connecting types | Nodes and arrows, if you drew your schema on a whiteboard |
| **migration** (morphism) | The map saying where every part of the new schema comes from in the old one | The plan of a migration script, minus the script |
| **lift** | Moving source data forward through a compiled migration | `alembic upgrade` |
| **restrict** | Pulling target-side structure back along a migration | Projecting a new interface onto the source fields that support it |
| **lens** | A two-way converter: a forward transform paired with a backward one that cannot drift apart | A serializer/deserializer pair, kept honest mechanically |
| **complement** | The stash of whatever the forward direction dropped, kept so the backward direction can restore it | An undo buffer; `git stash` |
| **round-trip laws** (GetPut, PutGet, PutPut) | Equations relating the forward and backward directions | Properties exercised by runtime checks and generated tests |
| **protolens** | A lens template that works on any schema matching a pattern, not one fixed pair | A generic function, where a lens is the monomorphic one |
| **dependent optic** | A protolens step with a precondition on the schemas it applies to | A guard clause on the template |
| **colimit** | Gluing several rulebooks together along their shared parts | Merging config fragments that share keys |
| **pushout** | The specific gluing used to merge two divergent schema branches | A three-way merge that understands structure, not lines |
| **existence check** | The pre-flight test that a migration can actually run on all conforming data | A dry run that is a proof rather than a sample |

## Which terms you actually need, and when

For the CLI workflow (diff, generate, convert, verify) you need *lens*, *complement*, and *migration*, and the one-line versions above suffice. The tutorials use only those three. The how-to quadrant adds *protolens* and *protocol*. Only the explanation quadrant's formal sections, and the [semantics](./semantics/index.md) cluster in particular, use the rest of the vocabulary, and those pages restate each term before using it.

The shortest route is to keep this page beside the tutorials, use the how-to guides as needed, and follow an explanation link when a one-line translation stops being enough.

## See also

- [Glossary](../glossary.md) for the formal definitions.
- [What panproto solves](./what-panproto-solves.md) for the problem statement in plain terms.
- [Schemas as theories](./schemas-as-theories.md) and [Migrations as morphisms](./migrations-as-morphisms.md), the two pages where the vocabulary starts earning its precision.
