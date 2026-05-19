# Schemas as theories

## In plain terms

A schema is a description of what shapes your data can take: which fields exist, which types they hold, which combinations are allowed. JSON Schema, Protobuf, GraphQL, SQL DDL, ATProto Lexicons all do this, each in their own syntax.

panproto treats a schema as an *algebraic theory*: a set of basic kinds of thing (records, fields, types, references), a set of operations on them (add a field, set a constraint, link two records), and a set of equations describing when two ways of building the same shape are equivalent. The theory is a precise mathematical object; an actual JSON Schema file or Protobuf .proto file is a *presentation* of one.

The reason this matters is that once you have schemas as theories, you can talk about *morphisms* between them with the same precision. A morphism is a structured map from one theory to another: a translation that respects every operation. Migrations between schema versions, translations between schema languages, and the merging of two divergent schema branches are all morphisms. The whole pipeline downstream of the schema treats them uniformly.

## The model

A schema is an instance of a generalized algebraic theory (GAT). A GAT consists of:

- **Sorts.** The basic kinds of thing the theory talks about: vertices, edges, types, constraints.
- **Operations.** Constructors that produce sorts from other sorts: `vertex` of a given kind, `edge` between two vertices, `prop` edge with a name, `item` edge for collection membership, `variant` edge for sum-typed alternatives.
- **Equations.** Equalities between operation chains that the theory imposes: composition is associative; the identity edge is a no-op; certain constraints are equivalent rewrites of others.

Each protocol panproto recognises is a GAT plus a parser/emitter pair. JSON Schema is one such GAT; Protobuf is another. The shared building-block theories (graph, multigraph, constraint, W-type, ...) are composed by colimit to produce each protocol's specific theory. See [Composing protocols by colimit](./protocol-colimits.md).

A *schema* in a protocol is a model of that protocol's theory: a concrete choice of vertices, edges, and constraint values that satisfies the theory's equations. An *instance* is a model of the schema's instance theory: actual data records that conform to the schema.

This separation, schema-theory versus instance-theory, runs through the entire system. Schemas have their own GAT; instances have their own GAT, parameterised over a schema; migrations are morphisms between schema theories; lift is the operation that takes records over the source schema and produces records over the target.

## What is not modelled

- Performance characteristics (a schema with 10,000 vertices is a valid model just like one with 5).
- Wire-format minutiae beyond what the schema theory describes (whitespace, comment syntax, ordering conventions).
- Application-level semantic constraints not expressible in the protocol's theory.

The first two are handled by the format-preserving codec layer. The third is by design: panproto is precise about exactly what it can guarantee.

## Related work

The schemas-as-theories framing belongs to a lineage that runs from Cartmell's generalised algebraic theories through Spivak's functorial data model and the algebraic-databases programme of Schultz and Wisnesky to Patterson's ACSets and Lu's recent multi-model unification. panproto presents each protocol as its own GAT and takes a colimit in the category of GAT presentations; ACSets and the Spivak-Wisnesky tradition fix a single meta-theory and parameterise schemas inside it. See [Related work](./related-work.md) for the full discussion.

## See also

- [Composing protocols by colimit](./protocol-colimits.md) for how protocols are built from shared building-block theories.
- [Migrations as morphisms](./migrations-as-morphisms.md) for what *morphism between schemas* means concretely.
- [Theory DSL: denotational semantics](./semantics/theory-dsl.md) for the formal model of a GAT presentation.
- The book's bibliography references @cartmell1986generalised for the original GAT formulation and @spivakwisnesky2015relational for the functorial-data-migration application.
