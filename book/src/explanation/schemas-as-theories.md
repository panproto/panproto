# Schemas as theories

A JSON Schema object, a Protobuf message, and an ATProto Lexicon record use different surface syntax. Once parsed, each identifies types, directed fields between types, and constraints on those elements. panproto stores this information in a common `Schema` representation that retains graph structure, protocol-specific constraints and metadata, and derived adjacency indices. The linked [`Schema` API](https://docs.rs/panproto-schema/latest/panproto_schema/struct.Schema.html) gives the complete field inventory. [`panproto_schema::validate`](https://docs.rs/panproto-schema/latest/panproto_schema/fn.validate.html) reports structural findings. Callers decide whether those findings reject an operation.

This concrete representation permits a structural comparison between documents from different schema languages. It does not erase their differences. Each schema retains its protocol identifier, and the protocol determines which structures are meaningful and which validation rules apply.

[The vocabulary in plain terms](./decoder-ring.md) introduces the graph terminology used below. The formal account distinguishes a specification from a structure that satisfies it.

## Protocol theories and schema models

A registered `Protocol` names a **schema theory** and an **instance theory**. The registry associates those names with generalized algebraic theory (GAT) presentations [@cartmell1986generalised]. A presentation may declare sorts, operations over those sorts, and equations or directed equations. The graph theory `ThGraph`, for instance, declares the sorts `Vertex` and `Edge` and the operations `src` and `tgt` from edges to vertices.

The protocols crate defines five foundational theories. `ThGraph` describes directed graphs, `ThConstraint` attaches constraints to vertices, and `ThMulti` adds edge labels. `ThMeta` describes discriminator and extra-field metadata. These four contain no equations. `ThWType` describes nodes, arcs, and values and contains two endpoint-coherence equations: an arc's source and target nodes must be anchored at the endpoints of the schema edge named by that arc. Higher-level theories compose these foundations for particular groups of protocols. [Composing protocols by colimit](./protocol-colimits.md) describes that composition.

A parsed schema has the intended mathematical reading of a **model** of its protocol's schema theory: concrete vertices and edges interpret the corresponding sorts, while their endpoints interpret `src` and `tgt`. The Rust `Schema` type is a dedicated concrete representation, however, and does not implement `panproto_gat::Model`. `panproto_schema::validate` checks the protocol's structural rules rather than evaluating every theory equation. Equation satisfaction uses a separately constructed finite `Model` and the bounded checker in `panproto-gat`.

The distinction also separates two kinds of map. A schema morphism maps the vertices and edges of one concrete schema to those of another while preserving endpoints and other required structure. A theory morphism maps the sorts and operations used to specify a family of structures. Migrations use the former. Cross-protocol theory composition uses the latter.

## Representational limits

The theory-model account describes structure that a protocol registration exposes. It does not describe the running time of operations over a large schema or application behavior absent from the schema. Whitespace, comments, and other source-layout details also lie outside the ordinary schema model. The layout-enrichment and format-preserving layers can retain such details when the parser supplies them.

Validation reports consistency findings for the registered structural rules. Application invariants remain unchecked when neither the protocol theory nor the concrete schema represents them.

## Related work

The schemas-as-theories account also draws on Spivak's functorial data model [@spivak2012functorial], the algebraic-databases program [@schultzwisnesky2017algebraic; @schultzspivakvasilakopoulouwisnesky2017algebraic], attributed C-sets [@pattersonlynchfairbanks2022categorical], and Lu's work on multi-model unification [@lu2025categorical]. panproto assigns a GAT presentation to each protocol and can combine presentations by colimit. The attributed C-set and algebraic-database approaches instead fix a meta-theory and parameterize schemas within it. [Related work](./related-work.md) develops the comparison.

## See also

- [Composing protocols by colimit](./protocol-colimits.md) for composition of foundational theories.
- [Migrations as morphisms](./migrations-as-morphisms.md) for maps between concrete schemas.
- [Theory DSL: denotational semantics](./semantics/theory-dsl.md) for the formal model of a GAT presentation.
- The bibliography entries @cartmell1986generalised and @spivakwisnesky2015relational for the GAT and functorial-data-migration backgrounds.
