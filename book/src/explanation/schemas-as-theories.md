# Schemas as theories

## In plain terms

Suppose one user record appears as a JSON Schema object, a Protobuf message, and an ATProto Lexicon record. Their field declarations look different, but each document states which fields exist, which types they hold, and which combinations are allowed. To compare them, panproto needs a common account of those commitments. That account is the schema.

panproto separates a protocol's rulebook from a schema written under that rulebook. The rulebook is a *generalized algebraic theory* (GAT): it names sorts such as vertices and edges, operations such as source and target, and equations those operations must satisfy. A parsed JSON Schema or `.proto` document is then a model governed by the corresponding protocol theory.

This separation gives schema translations a fixed structure. A schema morphism maps vertices and edges while respecting their endpoints and kinds; a theory morphism maps the sorts and operations that define a protocol. Migrations, cross-protocol translations, and merge use these related maps at different levels, which is why the distinction between theory and model must remain visible.

[The vocabulary in plain terms](./decoder-ring.md) names the graph objects used below. The formal path also requires the distinction between a signature and one of its models.

## The model

A protocol theory is represented as a GAT presentation. Its implementation-level ingredients are:

- **Sorts.** The basic kinds of thing the theory talks about: vertices, edges, types, constraints.
- **Operations.** Structure maps such as `src : Edge -> Vertex`, `tgt : Edge -> Vertex`, and `target : Constraint -> Vertex`.
- **Equations and directed equations.** Equalities and executable rewrites declared by theories that need them. The five basic theories in `panproto-protocols` currently contain no equations of their own; protocol-specific theories may add them.

Each registered protocol carries a schema theory and an instance theory together with its edge interpretation; parsers and emitters live in the I/O layers. Shared building blocks such as `ThGraph`, `ThConstraint`, `ThMulti`, and `ThWType` are composed into the theory groups used by protocol registrations. [Composing protocols by colimit](./protocol-colimits.md) develops that construction.

A *schema* supplies concrete vertices, edges, and constraint values under a protocol's schema theory. An *instance* supplies nodes, arcs, and values anchored to that schema under the protocol's instance theory.

This schema-theory/instance-theory split runs through the system. Migrations carry schema-level maps and compile into operations over instances; lift takes source instances to target instances along the compiled migration.

## What is not modeled

- Performance characteristics (a schema with 10,000 vertices is a valid model just like one with 5).
- Wire-format details beyond what the schema theory describes, such as whitespace and comments.
- Application-level semantic constraints not expressible in the protocol's theory.

Performance is an engineering property outside the model. Source layout can be retained by the format-preserving and layout-enrichment layers. Application constraints remain outside panproto unless a protocol theory or schema constraint expresses them.

## Related work

The schemas-as-theories framing belongs to a lineage that runs from Cartmell's generalized algebraic theories through Spivak's functorial data model and the algebraic-databases program of Schultz and Wisnesky to Patterson's ACSets and Lu's recent multi-model unification. panproto presents each protocol as its own GAT and takes a colimit in the category of GAT presentations; ACSets and the Spivak-Wisnesky tradition fix a single meta-theory and parameterize schemas inside it. See [Related work](./related-work.md) for the full discussion.

## See also

- [Composing protocols by colimit](./protocol-colimits.md) for how protocols are built from shared building-block theories.
- [Migrations as morphisms](./migrations-as-morphisms.md) for what *morphism between schemas* means concretely.
- [Theory DSL: denotational semantics](./semantics/theory-dsl.md) for the formal model of a GAT presentation.
- The book's bibliography references @cartmell1986generalised for the original GAT formulation and @spivakwisnesky2015relational for the functorial-data-migration application.
