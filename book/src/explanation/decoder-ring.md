# The vocabulary in plain terms

## In plain terms

panproto's vocabulary comes from the mathematics it implements, and inside the project the terms are load-bearing: they are what lets the implementation be checked against a precise specification. But you do not need the mathematics to use the tool, and you should not need a category theory course to read this book. This page translates each term once, in one line, into vocabulary a working developer already has. Every row links to the page where the term is defined properly.

Two reading rules. First, the analogies below are approximations: good enough to read the how-to and tutorial quadrants fluently, not good enough to reason about edge cases. When an edge case matters, follow the link. Second, if a page in this book uses one of these terms without explanation, that is a defect; the [glossary](../glossary.md) has the formal definitions; this page has the informal ones.

## The table

| panproto says | Plain terms | Nearest familiar thing |
|---|---|---|
| **protocol** | A schema language panproto can read and write | An entry in a format registry: "JSON Schema", "Protobuf", "SQL DDL" |
| **theory** (GAT) | The rulebook saying what a well-formed schema in one language looks like | The spec for a format, executable |
| **schema** | Your actual schema, parsed into panproto's internal form | The `api.yaml` or `.proto` you already have |
| **instance** | A data record conforming to a schema | A row; a JSON document |
| **vertex / edge** | A type in a schema / a field connecting types | Nodes and arrows, if you drew your schema on a whiteboard |
| **migration** (morphism) | The map saying where every part of the new schema comes from in the old one | The plan of a migration script, minus the script |
| **lift** | Running a migration forward over data | `alembic upgrade` |
| **restrict** | Reading the map backwards to see what the new schema requires from the old | The dependency check before the upgrade runs |
| **lens** | A two-way converter: a forward transform paired with a backward one that cannot drift apart | A serializer/deserializer pair, kept honest mechanically |
| **complement** | The stash of whatever the forward direction dropped, kept so the backward direction can restore it | An undo buffer; `git stash` |
| **round-trip laws** (GetPut, PutGet) | The checked promises that forward-then-back returns the original and back-then-forward returns the view | Property tests the tool runs for you |
| **protolens** | A lens template that works on any schema matching a pattern, not one fixed pair | A generic function, where a lens is the monomorphic one |
| **dependent optic** | A protolens step with a precondition on the schemas it applies to | A guard clause on the template |
| **colimit** | Gluing several rulebooks together along their shared parts | Merging config fragments that share keys |
| **pushout** | The specific gluing used to merge two divergent schema branches | A three-way merge that understands structure, not lines |
| **existence check** | The pre-flight test that a migration can actually run on all conforming data | A dry run that is a proof rather than a sample |

## Which terms you actually need, and when

For the CLI workflow (diff, generate, convert, verify) you need *lens*, *complement*, and *migration*, and the one-line versions above suffice. The tutorials use only those three. The how-to quadrant adds *protolens* and *protocol*. Only the explanation quadrant's formal sections, and the [semantics](./semantics/index.md) cluster in particular, use the rest of the vocabulary, and those pages restate each term before using it.

Thus the cheapest reading order: tutorials with this page open in a second tab, how-to guides as needed, explanation pages when a term's one-line version stops being enough.

## See also

- [Glossary](../glossary.md) for the formal definitions.
- [What panproto solves](./what-panproto-solves.md) for the problem statement in plain terms.
- [Schemas as theories](./schemas-as-theories.md) and [Migrations as morphisms](./migrations-as-morphisms.md), the two pages where the vocabulary starts earning its precision.
