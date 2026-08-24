# Schemas as theories

A JSON Schema object, a Protobuf message, and an ATProto Lexicon record use different surface syntax. Once parsed, however, each identifies types, directed fields between types, and constraints on those elements. panproto stores this information in a common `Schema` representation. Its principal tables record vertices, edges, hyperedges, constraints, required edges, variants, ordering and recursion information, source spans, and protocol-specific metadata. Derived adjacency indices support traversal, and validation rejects references to absent schema elements.

This concrete representation permits a structural comparison between documents from different schema languages. It does not erase their differences. Each schema retains its protocol identifier, and the protocol determines which structures are meaningful and which validation rules apply.

[The vocabulary in plain terms](./decoder-ring.md) introduces the graph terminology used below. The formal account distinguishes a specification from a structure that satisfies it.

## Protocol theories and schema models

A registered `Protocol` names a **schema theory** and an **instance theory**. The registry associates those names with generalized algebraic theory (GAT) presentations [@cartmell1986generalised]. A presentation may declare sorts, operations over those sorts, and equations or directed equations. The graph theory `ThGraph`, for instance, declares the sorts `Vertex` and `Edge` and the operations `src` and `tgt` from edges to vertices.

The protocols crate defines five foundational theories without equations of their own. `ThGraph` describes directed graphs, and `ThConstraint` attaches constraints to vertices. `ThMulti` adds edge labels. For tree-shaped instances, `ThWType` describes nodes, arcs, and values, while `ThMeta` describes discriminator and extra-field metadata. Higher-level theories compose these foundations for particular groups of protocols. [Composing protocols by colimit](./protocol-colimits.md) describes that composition.

A parsed schema is a **model** of its protocol's schema theory: its concrete vertices and edges interpret the corresponding sorts, while their endpoints interpret `src` and `tgt`. An instance similarly supplies concrete nodes, arcs, and values under the instance theory and a particular schema. This yields the **theory-model distinction**. A theory specifies a family of admissible structures; a model supplies one member of that family.

The distinction also separates two kinds of map. A schema morphism maps the vertices and edges of one concrete schema to those of another while preserving endpoints and other required structure. A theory morphism maps the sorts and operations used to specify a family of structures. Migrations use the former. Cross-protocol theory composition uses the latter.

## Representational limits

The theory-model account describes structure that a protocol registration exposes. It does not describe the running time of operations over a large schema or application behavior absent from the schema. Whitespace, comments, and other source-layout details also lie outside the ordinary schema model. The layout-enrichment and format-preserving layers can retain such details when the parser supplies them.

Validation establishes consistency with the registered structural rules. Application invariants remain unchecked when neither the protocol theory nor the concrete schema represents them.

## Related work

The schemas-as-theories account also draws on Spivak's functorial data model [@spivak2012functorial], the algebraic-databases program [@schultzwisnesky2017algebraic; @schultzspivakvasilakopoulouwisnesky2017algebraic], attributed C-sets [@pattersonlynchfairbanks2022categorical], and Lu's work on multi-model unification [@lu2025categorical]. panproto assigns a GAT presentation to each protocol and can combine presentations by colimit. The attributed C-set and algebraic-database approaches instead fix a meta-theory and parameterize schemas within it. [Related work](./related-work.md) develops the comparison.

## See also

- [Composing protocols by colimit](./protocol-colimits.md) for composition of foundational theories.
- [Migrations as morphisms](./migrations-as-morphisms.md) for maps between concrete schemas.
- [Theory DSL: denotational semantics](./semantics/theory-dsl.md) for the formal model of a GAT presentation.
- The bibliography entries @cartmell1986generalised and @spivakwisnesky2015relational for the GAT and functorial-data-migration backgrounds.
