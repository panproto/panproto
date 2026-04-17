# A relational case study

The relational model of @codd1970relational is the setting closest to the original categorical-database treatment of Spivak, in which a database schema is a small category with finite products and a database state is a product-preserving functor from the schema into the category of sets. The framework has since been extended to handle richer data types through the algebraic-databases construction of @schultzspivakvasilakopoulouwisnesky2017algebraic, which panproto follows at the level of its GAT models. Panproto's relational protocols follow this tradition. This chapter walks through what the relational case looks like inside panproto's framework. [Apache Cassandra](https://cassandra.apache.org/) and a subset of SQL are the concrete examples.

The relevant code lives in [`panproto_protocols::database`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/). The mathematical background is in [Protocols as theories, schemas as instances](../core/schemas-as-instances.md) and in the citation of Spivak's functorial-data-migration framework given in [Theory morphisms and instance migration](../core/morphisms-and-migration.md).

## What a relational schema is

A relational schema in the standard sense is a collection of tables, each with a declared set of columns and column types, plus a collection of foreign-key constraints linking columns of one table to primary keys of another. The rows of an instance populate the tables; foreign-key constraints are satisfied whenever every foreign-key value refers to a row that actually exists in the target table.

The categorical reading of this structure is direct. The schema is a category whose objects are the tables and whose morphisms are the functional dependencies induced by foreign keys plus the projections that send a row to one of its column values. An instance is a product-preserving functor from the schema into $\mathbf{Set}$: it assigns each table to its row-set and each morphism to the corresponding function on rows.

Panproto's relational protocols represent this structure as a GAT. Each table becomes a sort in the theory. Each column becomes an operation from the table's sort to the column's type sort. Each foreign-key constraint becomes an equation saying that a particular operation factors through the target table's primary key. The theory's models, in [`panproto_schema`](https://docs.rs/panproto-schema/latest/panproto_schema/)'s sense, are the schemas; a schema's instances are populations of rows satisfying the equations.

## A Cassandra schema

[Apache Cassandra](https://cassandra.apache.org/) [@lakshmanmalik2010cassandra] is a wide-column store with a schema language that differs from SQL in a few well-known ways. Its design lineage runs through the Dynamo key-value store of @decandia2007dynamo. A Cassandra table has a compound primary key made of a partition key (for distribution across nodes) and a clustering key (for ordering within a partition); secondary indexes are optional; cross-partition joins are not supported at the query layer. The underlying data model is still relational, and the categorical treatment above applies with minor adjustments.

Panproto's Cassandra protocol registers a theory whose sorts include `Table`, `PartitionKey`, `ClusteringKey`, and one sort per scalar type Cassandra supports (`Text`, `Int`, `Bigint`, `Uuid`, `Timestamp`, `Blob`, and the rest). Operations expose the column accessors and the primary-key components. Equations encode two constraints: that every row's primary-key components are total (no nulls in a primary key) and that partition-key values are hashed deterministically.

A schema under this protocol fixes the specific tables, columns, and primary-key structures of a given Cassandra keyspace. Loading a Cassandra schema from a CQL `CREATE TABLE` script goes through the parser in [`panproto_protocols::database`](https://docs.rs/panproto-protocols/latest/panproto_protocols/database/), which produces a `Schema` value whose sorts match the tables and whose operations match the columns.

## Migrations across schema versions

Adding a column to a table extends the source theory with a new operation from the table's sort to the column's type sort. The migration compiles through [the restrict/lift pipeline](../core/restrict-lift.md) as any other: a $\Sigma_f$-style pushforward supplies the column's default value (or `NULL`, if the column is declared nullable) for every existing row.

Dropping a column is the dual. The theory morphism removes the operation; the pushforward is a $\Delta_f$-pullback that forgets the column. Data loss is explicit: the dropped column's values are gone after the migration, and panproto's inversion stage reports the irreversibility when asked.

A rename maps the old operation name to the new. Unlike Avro, Cassandra does not support column aliases, so the rename is irreversible at the schema level; the migration remains trivial to apply but records no reverse path.

Foreign-key constraints translate to equations in the target theory. The pushforward is empty on the data side (no rows change), but existence checking rejects any source instance that violates the new constraint. The rejection report identifies the specific rows whose foreign-key values do not resolve. The diagnostic is produced by [`panproto_mig::existence`](https://docs.rs/panproto-mig/latest/panproto_mig/existence/).

## A SQL subset

Panproto's SQL support covers a subset suitable for schema migrations: `CREATE TABLE`, `ALTER TABLE ADD/DROP COLUMN`, `ALTER TABLE RENAME`, and `ALTER TABLE ADD/DROP CONSTRAINT`. It does not cover trigger, stored-procedure, or view definitions. The subset is wide enough to handle the schema-evolution cases real applications generate and narrow enough that the translation into panproto's framework is unambiguous.

The SQL parser registers a separate theory from the Cassandra one, since SQL and Cassandra disagree on which types are primitive (SQL has `DECIMAL`, Cassandra does not; Cassandra has `Counter`, SQL does not). Two schemas under the SQL theory can be migrated into each other directly; two schemas under different theories require a theory morphism between the protocols themselves, which is the subject of [Protocol colimits](../core/protocol-colimits.md).

## Further reading

@codd1970relational is the founding paper of the relational model, and is still the most readable single source on what makes a schema relational. @spivak2012functorial is the categorical reformulation that panproto's relational protocols follow directly, and @schultzspivakvasilakopoulouwisnesky2017algebraic extends the framework to accommodate the richer data types a working database needs. @wisnesky2013functional is the companion PhD thesis and the most direct precedent for panproto's engine; the CQL system that grew out of it is the closest existing tool to the relational side of panproto.

For the specific protocols discussed in this chapter, @lakshmanmalik2010cassandra is the Cassandra system paper; @decandia2007dynamo is the Dynamo paper whose techniques Cassandra and several other NoSQL systems draw on. For SQL as a practical schema language, @kleppmann2017designing chapter 2 ("Data Models and Query Languages") is the most accessible treatment.

## Closing

The next chapter turns to [FHIR as a document case study](./document.md). FHIR stresses the representation in a different direction from the relational protocols here: its schemas are deeply nested, heavily constrained, and versioned irregularly. Panproto handles it through the same mechanisms Parts II and III developed, but the specific translation choices are worth tracing in detail.
