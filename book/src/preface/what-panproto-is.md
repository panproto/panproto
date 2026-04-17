# What panproto is

This chapter frames panproto against the tools a working developer is likely to have used already. The framing is concrete. No category theory appears below the fold of this chapter; that work begins in [Categories](../foundations/categories.md).

A simple scenario sets the stage. A small service stores user records as JSON and keeps the schema for those records in a file called `user.schema.json`, checked into the same repository as the code. The schema declares a `name` field and an `email` field, both required. After a few months, a new requirement arrives: users should have a display name that may differ from the account name. The schema grows a `displayName` field. Old records in the database do not have a `displayName`; new records do. A developer writes a small migration script that adds `displayName` values to the old records, and commits the script alongside the schema change.

This is not unusual. Every non-trivial production system runs some version of the same sequence. The specifics vary between [ATProto](https://atproto.com/), [Apache Avro](https://avro.apache.org/), [FHIR](https://www.hl7.org/fhir/), Postgres DDL, and the two dozen other schema languages in wide use, but the shape is the same: a schema file is edited, a migration runs against existing data, and the system is now operating against a new schema.

The ordinary tools handle the parts of this sequence they were designed for. [Git](https://git-scm.com/) versions the schema file and the migration script as text. [Protobuf](https://protobuf.dev/) and [JSON Schema](https://json-schema.org/) validate records against their schemas. Database migration frameworks (Flyway, Alembic, Rails ActiveRecord migrations, the `schema.rb` pattern, the Cassandra `ALTER TABLE` DDL) sequence the script's application against the data store.

What none of these tools do is verify that the migration script is *correct*. Git sees a text diff to a schema file plus a text diff to a script. Protobuf sees the new schema as a set of new constraints to enforce on incoming records. A migration framework sees the script as a procedure to run. Nothing in the stack verifies that the script produces well-formed records under the new schema, or that the script undoes itself when reversed, or that the script's effect on the records is the same if it runs once as if it runs twice.

The gap is where the trouble lives. The script can go wrong in several small ways that are all easy to miss. The `displayName` default may violate a uniqueness constraint on the new schema, or the migration may forget one of the three tables that contain user records. Alternatively, the migration's inverse may differ subtly from its forward direction, so that a rollback introduces data loss the original forward step did not announce. Each of these problems is a bug whose manifestation is a production incident.

Panproto's claim is that these are not separate problems, to be solved separately, with a different tool for each case. They are all instances of a single operation: the lift of a morphism between two schemas to a function on the data each schema holds. If schemas are mathematical objects (specifically, models of generalised algebraic theories), and if migrations are morphisms of those models, then the correctness properties above are specific, mechanical equations. A library that represents schemas and migrations this way can verify the equations at build time, before a migration runs against production data.

Panproto is an implementation of that representation. It exposes:

- Schemas as models of a GAT registered as a protocol, with the model's well-formedness mechanically checked.
- Migrations as morphisms between models, with the migration engine's pipeline producing actionable diagnostics at each stage where a migration can fail.
- Bidirectional transformations (lenses) between schemas, whose round-trip laws the library verifies automatically.
- Version control for schemas as first-class objects, so that a schema's history is trackable independently of the byte history of its file.
- Cross-protocol translation, so that a schema or a record written against one protocol can be mapped mechanically to another.

None of these is exotic. Every one of them is a specific construction in category theory applied to a specific problem in working data engineering, and each has a chapter of its own later in the book.

## What this book is not

The book is not a category theory textbook. It introduces the mathematical machinery panproto rests on, but it does so by showing how the machinery applies to schemas, migrations, and version control. A reader who finishes the book has seen category theory at the level that is necessary to understand panproto's design. A reader who wants to go deeper into category theory itself should turn to the references that close each chapter; @maclane1998categories, @awodey2010category, and @lawvereschanuel2009conceptual are the three starting points.

The book is also not a reference manual for panproto. Every chapter references specific parts of the Rust code (linked to [docs.rs](https://docs.rs/)), but the reference documentation is [the generated rustdoc](https://docs.rs/panproto-core/latest/panproto_core/). This book develops the mathematical and engineering design behind panproto's API; the API itself is documented alongside the code.

The book is not, finally, a polemic against existing tools. Git works. Protobuf works. Flyway works. Each of these is excellent at the job it was designed for. Panproto is what arises when the job is enlarged to include the relationship between schemas and data, and the reader who finishes this book will have a picture of how the enlargement is carried out, not an argument for why the smaller tools are inadequate.

## Closing

The next chapter, [A note on notation](./notation.md), documents the typographic conventions the book uses. After that, [Categories](../foundations/categories.md) starts the mathematical development.
